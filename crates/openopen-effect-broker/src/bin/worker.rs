use ed25519_dalek::{SigningKey, VerifyingKey};
use openopen_effect_broker::{BrokerConfig, BrokerEngine, protected_root_for_audit_euid};
use openopen_protocol::{EFFECT_PROTOCOL_VERSION, EffectBrokerSession, EffectPermit};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::fs::{DirBuilder, File, OpenOptions, Permissions};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

const SYSTEM_ROOT: &str = "/Library/Application Support/com.thesongzhu.OpenOpen";
const SESSION_TTL_MS: i64 = 5 * 60 * 1_000;
const MAX_REQUEST_BYTES: usize = 256 * 1024;

#[derive(Debug)]
enum WorkerError {
    InvalidInvocation,
    InvalidPeer,
    InvalidRequest,
    InvalidProtectedState,
    CoreNotEnrolled,
    EnrollmentConflict,
    SystemTime,
    Io(io::Error),
    Database(rusqlite::Error),
    Broker(openopen_effect_broker::BrokerError),
}

impl From<io::Error> for WorkerError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<rusqlite::Error> for WorkerError {
    fn from(value: rusqlite::Error) -> Self {
        Self::Database(value)
    }
}

impl From<openopen_effect_broker::BrokerError> for WorkerError {
    fn from(value: openopen_effect_broker::BrokerError) -> Self {
        Self::Broker(value)
    }
}

impl WorkerError {
    const fn code(&self) -> &'static str {
        match self {
            Self::InvalidInvocation => "invalidInvocation",
            Self::InvalidPeer => "invalidAuthenticatedPeer",
            Self::InvalidRequest => "invalidTypedRequest",
            Self::InvalidProtectedState => "invalidProtectedState",
            Self::CoreNotEnrolled => "coreNotEnrolled",
            Self::EnrollmentConflict => "coreEnrollmentConflict",
            Self::SystemTime => "invalidSystemTime",
            Self::Io(_) => "protectedIoFailure",
            Self::Database(_) => "protectedJournalFailure",
            Self::Broker(_) => "effectRejected",
        }
    }

    fn observe_for_lints(&self) {
        match self {
            Self::Io(error) => {
                let _ = error.kind();
            }
            Self::Database(error) => {
                let _ = error.sqlite_error_code();
            }
            Self::Broker(error) => {
                let _ = error.to_string();
            }
            _ => {}
        }
    }
}

#[derive(Clone)]
struct RuntimePaths {
    system_root: PathBuf,
    audit_euid: u32,
}

impl RuntimePaths {
    fn production(audit_euid: u32) -> Result<Self, WorkerError> {
        if rustix::process::geteuid().as_raw() != 0 || audit_euid == 0 {
            return Err(WorkerError::InvalidPeer);
        }
        Ok(Self {
            system_root: PathBuf::from(SYSTEM_ROOT),
            audit_euid,
        })
    }

    fn users_root(&self) -> PathBuf {
        self.system_root.join("Users")
    }

    fn user_root(&self) -> PathBuf {
        self.users_root().join(self.audit_euid.to_string())
    }

    fn missions_root(&self) -> PathBuf {
        self.user_root().join("Missions")
    }

    fn state_database(&self) -> PathBuf {
        self.user_root().join(".effect-broker-state.sqlite3")
    }

    fn broker_state_root(&self) -> PathBuf {
        self.system_root.join("EffectBroker")
    }

    fn signing_seed(&self) -> PathBuf {
        self.broker_state_root().join("broker-signing-seed")
    }

    fn prepare(&self) -> Result<(), WorkerError> {
        let expected = protected_root_for_audit_euid(self.audit_euid);
        if self.missions_root() != expected {
            return Err(WorkerError::InvalidProtectedState);
        }
        require_existing_root_directory(Path::new("/Library/Application Support"))?;
        ensure_private_directory(&self.system_root)?;
        ensure_private_directory(&self.users_root())?;
        ensure_private_directory(&self.user_root())?;
        ensure_private_directory(&self.missions_root())?;
        ensure_private_directory(&self.broker_state_root())
    }
}

struct ProtectedState {
    connection: Connection,
}

impl ProtectedState {
    fn open(path: &Path) -> Result<Self, WorkerError> {
        reject_wrong_existing_file(path)?;
        let connection = Connection::open(path)?;
        std::fs::set_permissions(path, Permissions::from_mode(0o600))?;
        require_private_regular_file(path, None)?;
        connection.execute_batch(
            "PRAGMA journal_mode = DELETE;
             PRAGMA synchronous = FULL;
             CREATE TABLE IF NOT EXISTS broker_state (
                singleton INTEGER PRIMARY KEY CHECK(singleton = 1),
                core_key_id TEXT,
                core_verifying_key_hex TEXT,
                session_nonce TEXT,
                session_expires_at_ms INTEGER
             );
             INSERT OR IGNORE INTO broker_state(singleton) VALUES (1);",
        )?;
        Ok(Self { connection })
    }

    fn core_key(&self) -> Result<Option<[u8; 32]>, WorkerError> {
        let row: Option<(Option<String>, Option<String>)> = self
            .connection
            .query_row(
                "SELECT core_key_id, core_verifying_key_hex
                 FROM broker_state WHERE singleton = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let Some((key_id, key_hex)) = row else {
            return Err(WorkerError::InvalidProtectedState);
        };
        match (key_id, key_hex) {
            (None, None) => Ok(None),
            (Some(key_id), Some(key_hex)) => validate_key_pair(&key_id, &key_hex).map(Some),
            _ => Err(WorkerError::InvalidProtectedState),
        }
    }

    fn enroll_core(&mut self, request: &EnrollCoreRequest) -> Result<(), WorkerError> {
        request.validate()?;
        validate_key_pair(&request.core_key_id, &request.core_verifying_key_hex)?;
        let transaction = self.connection.transaction()?;
        let existing: (Option<String>, Option<String>) = transaction.query_row(
            "SELECT core_key_id, core_verifying_key_hex
             FROM broker_state WHERE singleton = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        match existing {
            (None, None) => {
                transaction.execute(
                    "UPDATE broker_state
                     SET core_key_id = ?1, core_verifying_key_hex = ?2
                     WHERE singleton = 1",
                    params![request.core_key_id, request.core_verifying_key_hex],
                )?;
            }
            (Some(key_id), Some(key_hex))
                if key_id == request.core_key_id && key_hex == request.core_verifying_key_hex => {}
            _ => return Err(WorkerError::EnrollmentConflict),
        }
        transaction.commit()?;
        Ok(())
    }

    fn session(
        &mut self,
        broker_signing_key: &SigningKey,
        now_ms: i64,
    ) -> Result<EffectBrokerSession, WorkerError> {
        let transaction = self.connection.transaction()?;
        let existing: (Option<String>, Option<i64>) = transaction.query_row(
            "SELECT session_nonce, session_expires_at_ms
             FROM broker_state WHERE singleton = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let (nonce, expires_at_ms) = match existing {
            (Some(nonce), Some(expiry)) if is_lower_hex(&nonce, 64) && expiry > now_ms => {
                (nonce, expiry)
            }
            (None, None) => {
                let nonce = random_hex_32()?;
                let expiry = now_ms
                    .checked_add(SESSION_TTL_MS)
                    .ok_or(WorkerError::SystemTime)?;
                transaction.execute(
                    "UPDATE broker_state
                     SET session_nonce = ?1, session_expires_at_ms = ?2
                     WHERE singleton = 1",
                    params![nonce, expiry],
                )?;
                (nonce, expiry)
            }
            (Some(nonce), Some(expiry)) if is_lower_hex(&nonce, 64) && expiry <= now_ms => {
                let nonce = random_hex_32()?;
                let expiry = now_ms
                    .checked_add(SESSION_TTL_MS)
                    .ok_or(WorkerError::SystemTime)?;
                transaction.execute(
                    "UPDATE broker_state
                     SET session_nonce = ?1, session_expires_at_ms = ?2
                     WHERE singleton = 1",
                    params![nonce, expiry],
                )?;
                (nonce, expiry)
            }
            _ => return Err(WorkerError::InvalidProtectedState),
        };
        transaction.commit()?;
        let verifying_key = broker_signing_key.verifying_key().to_bytes();
        Ok(EffectBrokerSession {
            protocol_version: EFFECT_PROTOCOL_VERSION,
            session_nonce: nonce,
            broker_key_id: sha256_hex(verifying_key),
            broker_verifying_key_hex: hex::encode(verifying_key),
            expires_at_ms,
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct EnrollCoreRequest {
    #[serde(rename = "type")]
    request_type: String,
    version: u32,
    core_key_id: String,
    core_verifying_key_hex: String,
}

impl EnrollCoreRequest {
    fn validate(&self) -> Result<(), WorkerError> {
        if self.request_type == "enrollCore" && self.version == 1 {
            Ok(())
        } else {
            Err(WorkerError::InvalidRequest)
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PutRequest {
    #[serde(rename = "type")]
    request_type: String,
    version: u32,
    permit: EffectPermit,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReconcileRequest {
    #[serde(rename = "type")]
    request_type: String,
    version: u32,
    permit: EffectPermit,
}

impl ReconcileRequest {
    fn validate(&self) -> Result<(), WorkerError> {
        if self.request_type == "reconcileMissionFile" && self.version == 1 {
            Ok(())
        } else {
            Err(WorkerError::InvalidRequest)
        }
    }
}

impl PutRequest {
    fn validate(&self) -> Result<(), WorkerError> {
        if self.request_type == "putMissionFile" && self.version == 1 {
            Ok(())
        } else {
            Err(WorkerError::InvalidRequest)
        }
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(value) => {
            println!("{value}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            error.observe_for_lints();
            println!(
                "{}",
                json!({
                    "error": { "code": error.code() },
                    "status": "rejected",
                    "version": 1,
                })
            );
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<serde_json::Value, WorkerError> {
    let mut arguments = std::env::args().skip(1);
    let operation = arguments.next().ok_or(WorkerError::InvalidInvocation)?;
    let audit_euid = arguments
        .next()
        .ok_or(WorkerError::InvalidInvocation)?
        .parse::<u32>()
        .map_err(|_| WorkerError::InvalidInvocation)?;
    if arguments.next().is_some() {
        return Err(WorkerError::InvalidInvocation);
    }
    let paths = RuntimePaths::production(audit_euid)?;
    paths.prepare()?;
    let signing_key = load_or_create_signing_key(&paths.signing_seed())?;
    let mut state = ProtectedState::open(&paths.state_database())?;
    let now_ms = current_unix_ms()?;
    match operation.as_str() {
        "status" => Ok(json!({
            "coreEnrolled": state.core_key()?.is_some(),
            "status": "ready",
            "version": 1,
        })),
        "session" => serde_json::to_value(state.session(&signing_key, now_ms)?)
            .map_err(|_| WorkerError::InvalidProtectedState),
        "enroll-core" => {
            let request: EnrollCoreRequest = read_json_line(&mut io::stdin().lock())?;
            state.enroll_core(&request)?;
            Ok(json!({ "status": "enrolled", "version": 1 }))
        }
        "put" => {
            let mut input = BufReader::new(io::stdin().lock());
            let request: PutRequest = read_json_line(&mut input)?;
            request.validate()?;
            let core_key = state.core_key()?.ok_or(WorkerError::CoreNotEnrolled)?;
            let session = state.session(&signing_key, now_ms)?;
            let config = BrokerConfig {
                protected_root: paths.missions_root(),
                authenticated_audit_euid: audit_euid,
                enrolled_core_verifying_key: core_key,
                broker_signing_seed: signing_key.to_bytes(),
                session_nonce: session.session_nonce,
                session_expires_at_ms: session.expires_at_ms,
            };
            let mut engine = BrokerEngine::open(config)?;
            let receipt = engine.put_file(&request.permit, input)?;
            serde_json::to_value(receipt).map_err(|_| WorkerError::InvalidProtectedState)
        }
        "reconcile" => {
            let request: ReconcileRequest = read_json_line(&mut io::stdin().lock())?;
            request.validate()?;
            let core_key = state.core_key()?.ok_or(WorkerError::CoreNotEnrolled)?;
            let session = state.session(&signing_key, now_ms)?;
            let config = BrokerConfig {
                protected_root: paths.missions_root(),
                authenticated_audit_euid: audit_euid,
                enrolled_core_verifying_key: core_key,
                broker_signing_seed: signing_key.to_bytes(),
                session_nonce: session.session_nonce,
                session_expires_at_ms: session.expires_at_ms,
            };
            let mut engine = BrokerEngine::open(config)?;
            let reconciliation = engine.reconcile_effect(&request.permit)?;
            serde_json::to_value(reconciliation).map_err(|_| WorkerError::InvalidProtectedState)
        }
        _ => Err(WorkerError::InvalidInvocation),
    }
}

fn read_json_line<T: for<'de> Deserialize<'de>>(
    reader: &mut impl BufRead,
) -> Result<T, WorkerError> {
    let mut line = Vec::new();
    let read = reader.read_until(b'\n', &mut line)?;
    if read == 0 || read > MAX_REQUEST_BYTES || line.last() != Some(&b'\n') {
        return Err(WorkerError::InvalidRequest);
    }
    line.pop();
    serde_json::from_slice(&line).map_err(|_| WorkerError::InvalidRequest)
}

fn validate_key_pair(key_id: &str, key_hex: &str) -> Result<[u8; 32], WorkerError> {
    if !is_lower_hex(key_id, 64) || !is_lower_hex(key_hex, 64) {
        return Err(WorkerError::InvalidProtectedState);
    }
    let key: [u8; 32] = hex::decode(key_hex)
        .map_err(|_| WorkerError::InvalidProtectedState)?
        .try_into()
        .map_err(|_| WorkerError::InvalidProtectedState)?;
    VerifyingKey::from_bytes(&key).map_err(|_| WorkerError::InvalidProtectedState)?;
    if sha256_hex(key) != key_id {
        return Err(WorkerError::InvalidProtectedState);
    }
    Ok(key)
}

fn load_or_create_signing_key(path: &Path) -> Result<SigningKey, WorkerError> {
    reject_wrong_existing_file(path)?;
    if !path.exists() {
        let mut seed = [0_u8; 32];
        getrandom::fill(&mut seed).map_err(|_| WorkerError::InvalidProtectedState)?;
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
        {
            Ok(mut file) => {
                file.write_all(&seed)?;
                file.sync_all()?;
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
    }
    require_private_regular_file(path, Some(32))?;
    let mut seed = [0_u8; 32];
    File::open(path)?.read_exact(&mut seed)?;
    Ok(SigningKey::from_bytes(&seed))
}

fn require_existing_root_directory(path: &Path) -> Result<(), WorkerError> {
    let metadata = std::fs::symlink_metadata(path)?;
    let canonical = std::fs::canonicalize(path)?;
    if metadata.file_type().is_symlink()
        || !metadata.is_dir()
        || metadata.uid() != 0
        || canonical != path
    {
        return Err(WorkerError::InvalidProtectedState);
    }
    Ok(())
}

fn ensure_private_directory(path: &Path) -> Result<(), WorkerError> {
    if !path.exists() {
        DirBuilder::new().mode(0o700).create(path)?;
    }
    let metadata = std::fs::symlink_metadata(path)?;
    let canonical = std::fs::canonicalize(path)?;
    if metadata.file_type().is_symlink()
        || !metadata.is_dir()
        || metadata.uid() != rustix::process::geteuid().as_raw()
        || metadata.permissions().mode() & 0o777 != 0o700
        || canonical != path
    {
        return Err(WorkerError::InvalidProtectedState);
    }
    Ok(())
}

fn reject_wrong_existing_file(path: &Path) -> Result<(), WorkerError> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata)
            if metadata.file_type().is_symlink()
                || !metadata.is_file()
                || metadata.uid() != rustix::process::geteuid().as_raw() =>
        {
            Err(WorkerError::InvalidProtectedState)
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn require_private_regular_file(path: &Path, length: Option<u64>) -> Result<(), WorkerError> {
    let metadata = std::fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.uid() != rustix::process::geteuid().as_raw()
        || metadata.permissions().mode() & 0o777 != 0o600
        || length.is_some_and(|expected| metadata.len() != expected)
    {
        return Err(WorkerError::InvalidProtectedState);
    }
    Ok(())
}

fn random_hex_32() -> Result<String, WorkerError> {
    let mut value = [0_u8; 32];
    getrandom::fill(&mut value).map_err(|_| WorkerError::InvalidProtectedState)?;
    Ok(hex::encode(value))
}

fn current_unix_ms() -> Result<i64, WorkerError> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| WorkerError::SystemTime)?;
    i64::try_from(duration.as_millis()).map_err(|_| WorkerError::SystemTime)
}

fn is_lower_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn sha256_hex(bytes: impl AsRef<[u8]>) -> String {
    format!("{:x}", Sha256::digest(bytes.as_ref()))
}

#[cfg(test)]
mod tests {
    use super::{
        EnrollCoreRequest, ProtectedState, PutRequest, ReconcileRequest, WorkerError,
        read_json_line, validate_key_pair,
    };
    use ed25519_dalek::SigningKey;
    use sha2::{Digest, Sha256};
    use std::io::BufReader;

    #[test]
    fn worker_rejects_unknown_enrollment_fields_and_changed_key_ids() {
        let key = SigningKey::from_bytes(&[7_u8; 32])
            .verifying_key()
            .to_bytes();
        let key_hex = hex::encode(key);
        let key_id = format!("{:x}", Sha256::digest(key));
        validate_key_pair(&key_id, &key_hex).unwrap();
        assert!(validate_key_pair(&"00".repeat(32), &key_hex).is_err());

        let bytes = format!(
            "{{\"coreKeyId\":\"{key_id}\",\"coreVerifyingKeyHex\":\"{key_hex}\",\"extra\":true,\"type\":\"enrollCore\",\"version\":1}}\n"
        );
        let error = read_json_line::<EnrollCoreRequest>(&mut BufReader::new(bytes.as_bytes()))
            .expect_err("unknown enrollment fields must fail");
        assert!(matches!(error, WorkerError::InvalidRequest));
    }

    #[test]
    fn worker_put_parser_requires_one_bounded_line_before_payload() {
        let missing_newline = br#"{"type":"putMissionFile","version":1}"#;
        assert!(read_json_line::<PutRequest>(&mut BufReader::new(&missing_newline[..])).is_err());
        let oversized = vec![b'a'; super::MAX_REQUEST_BYTES + 1];
        assert!(read_json_line::<PutRequest>(&mut BufReader::new(&oversized[..])).is_err());
        let reconcile = br#"{"permit":{},"type":"reconcileMissionFile","version":1}
"#;
        assert!(read_json_line::<ReconcileRequest>(&mut BufReader::new(&reconcile[..])).is_err());
    }

    #[test]
    fn protected_state_pins_core_key_and_reuses_live_session() {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("state.sqlite3");
        let core_key = SigningKey::from_bytes(&[7_u8; 32])
            .verifying_key()
            .to_bytes();
        let request = EnrollCoreRequest {
            request_type: "enrollCore".into(),
            version: 1,
            core_key_id: format!("{:x}", Sha256::digest(core_key)),
            core_verifying_key_hex: hex::encode(core_key),
        };
        let broker_key = SigningKey::from_bytes(&[8_u8; 32]);
        let mut state = ProtectedState::open(&database).unwrap();
        state.enroll_core(&request).unwrap();
        state.enroll_core(&request).unwrap();
        assert_eq!(state.core_key().unwrap(), Some(core_key));
        let first = state.session(&broker_key, 1_000).unwrap();
        let duplicate = state.session(&broker_key, 1_001).unwrap();
        assert_eq!(duplicate, first);

        let changed_key = SigningKey::from_bytes(&[9_u8; 32])
            .verifying_key()
            .to_bytes();
        let changed = EnrollCoreRequest {
            request_type: "enrollCore".into(),
            version: 1,
            core_key_id: format!("{:x}", Sha256::digest(changed_key)),
            core_verifying_key_hex: hex::encode(changed_key),
        };
        assert!(matches!(
            state.enroll_core(&changed),
            Err(WorkerError::EnrollmentConflict)
        ));
    }

    #[test]
    fn protected_state_rejects_malformed_persisted_session_instead_of_rotating_it() {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("state.sqlite3");
        let broker_key = SigningKey::from_bytes(&[8_u8; 32]);
        let mut state = ProtectedState::open(&database).unwrap();
        state.session(&broker_key, 1_000).unwrap();
        state
            .connection
            .execute(
                "UPDATE broker_state SET session_nonce = 'not-hex' WHERE singleton = 1",
                [],
            )
            .unwrap();
        assert!(matches!(
            state.session(&broker_key, 1_001),
            Err(WorkerError::InvalidProtectedState)
        ));
    }
}
