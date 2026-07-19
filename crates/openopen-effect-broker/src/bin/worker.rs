use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use openopen_effect_broker::{
    BrokerConfig, BrokerEngine, acquire_protected_operation_guard, protected_root_for_audit_euid,
};
use openopen_protocol::{
    CoreInstanceLease, EFFECT_PROTOCOL_VERSION, EffectBrokerSession, EffectPermit,
    EffectPermitPurpose, RuntimeControlAuthorization, RuntimeControlReceipt,
    core_instance_lease_signing_bytes, runtime_control_authorization_hash,
    runtime_control_authorization_signing_bytes, runtime_control_receipt_signing_bytes,
};
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

type StoredRuntimeRow = (
    Option<i64>,
    Option<i64>,
    Option<i64>,
    Option<String>,
    Option<String>,
);

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
                session_expires_at_ms INTEGER,
                runtime_enabled INTEGER,
                runtime_revision INTEGER,
                runtime_updated_at_ms INTEGER,
                runtime_signature_hex TEXT,
                runtime_checkpoint_nonce TEXT,
                lease_app_pid INTEGER,
                lease_app_start_time_us INTEGER,
                lease_core_pid INTEGER,
                lease_core_start_time_us INTEGER,
                lease_core_audit_token_hex TEXT,
                lease_codex_pid INTEGER,
                lease_codex_start_time_us INTEGER,
                lease_codex_audit_token_hex TEXT,
                lease_core_instance_nonce TEXT,
                lease_issued_at_ms INTEGER,
                lease_broker_key_id TEXT,
                lease_broker_signature_hex TEXT
             );
             INSERT OR IGNORE INTO broker_state(singleton) VALUES (1);",
        )?;
        let columns = connection
            .prepare("PRAGMA table_info(broker_state)")?
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<Vec<_>, _>>()?;
        for (name, declaration) in [
            ("runtime_enabled", "INTEGER"),
            ("runtime_revision", "INTEGER"),
            ("runtime_updated_at_ms", "INTEGER"),
            ("runtime_signature_hex", "TEXT"),
            ("runtime_checkpoint_nonce", "TEXT"),
            ("lease_app_pid", "INTEGER"),
            ("lease_app_start_time_us", "INTEGER"),
            ("lease_core_pid", "INTEGER"),
            ("lease_core_start_time_us", "INTEGER"),
            ("lease_core_audit_token_hex", "TEXT"),
            ("lease_codex_pid", "INTEGER"),
            ("lease_codex_start_time_us", "INTEGER"),
            ("lease_codex_audit_token_hex", "TEXT"),
            ("lease_core_instance_nonce", "TEXT"),
            ("lease_issued_at_ms", "INTEGER"),
            ("lease_broker_key_id", "TEXT"),
            ("lease_broker_signature_hex", "TEXT"),
        ] {
            if !columns.iter().any(|column| column == name) {
                connection.execute(
                    &format!("ALTER TABLE broker_state ADD COLUMN {name} {declaration}"),
                    [],
                )?;
            }
        }
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

    fn runtime_checkpoint(
        &self,
    ) -> Result<Option<(RuntimeControlAuthorization, String)>, WorkerError> {
        let row: StoredRuntimeRow = self.connection.query_row(
            "SELECT runtime_enabled, runtime_revision, runtime_updated_at_ms,
                        runtime_signature_hex, runtime_checkpoint_nonce
                 FROM broker_state WHERE singleton = 1",
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )?;
        match row {
            (None, None, None, None, None) => Ok(None),
            (
                Some(enabled),
                Some(revision),
                Some(updated_at_ms),
                Some(signature_hex),
                Some(checkpoint_nonce),
            ) => {
                let key = self.core_key()?.ok_or(WorkerError::CoreNotEnrolled)?;
                let revision =
                    u64::try_from(revision).map_err(|_| WorkerError::InvalidProtectedState)?;
                if !matches!(enabled, 0 | 1)
                    || revision == 0
                    || updated_at_ms < 0
                    || !is_lower_hex(&checkpoint_nonce, 64)
                {
                    return Err(WorkerError::InvalidProtectedState);
                }
                let authorization = RuntimeControlAuthorization {
                    protocol_version: EFFECT_PROTOCOL_VERSION,
                    enabled: enabled == 1,
                    revision,
                    updated_at_ms,
                    core_key_id: sha256_hex(key),
                    authorization_signature_hex: signature_hex,
                };
                verify_runtime_control(&authorization, &key)?;
                Ok(Some((authorization, checkpoint_nonce)))
            }
            _ => Err(WorkerError::InvalidProtectedState),
        }
    }

    fn runtime_control(&self) -> Result<Option<RuntimeControlAuthorization>, WorkerError> {
        Ok(self
            .runtime_checkpoint()?
            .map(|(authorization, _)| authorization))
    }

    fn apply_runtime_control(
        &mut self,
        authorization: &RuntimeControlAuthorization,
    ) -> Result<String, WorkerError> {
        let key = self.core_key()?.ok_or(WorkerError::CoreNotEnrolled)?;
        verify_runtime_control(authorization, &key)?;
        let existing = self.runtime_checkpoint()?;
        if existing.as_ref().map(|(control, _)| control) == Some(authorization) {
            return Ok(existing.expect("matching checkpoint exists").1);
        }
        let expected_revision = existing
            .as_ref()
            .map_or(1, |(value, _)| value.revision.saturating_add(1));
        if authorization.revision != expected_revision {
            return Err(WorkerError::InvalidRequest);
        }
        let revision =
            i64::try_from(authorization.revision).map_err(|_| WorkerError::InvalidRequest)?;
        let checkpoint_nonce = random_hex_32()?;
        self.connection.execute(
            "UPDATE broker_state
             SET runtime_enabled = ?1, runtime_revision = ?2,
                 runtime_updated_at_ms = ?3, runtime_signature_hex = ?4,
                 runtime_checkpoint_nonce = ?5
             WHERE singleton = 1",
            params![
                i64::from(authorization.enabled),
                revision,
                authorization.updated_at_ms,
                authorization.authorization_signature_hex,
                checkpoint_nonce,
            ],
        )?;
        Ok(checkpoint_nonce)
    }

    fn require_runtime_for(&self, permit: &EffectPermit) -> Result<(), WorkerError> {
        self.require_runtime_revision(permit.runtime_revision, permit.purpose)
    }

    fn require_runtime_revision(
        &self,
        revision: u64,
        purpose: EffectPermitPurpose,
    ) -> Result<(), WorkerError> {
        let control = self
            .runtime_control()?
            .ok_or(WorkerError::InvalidProtectedState)?;
        if control.revision != revision
            || (purpose == EffectPermitPurpose::Execute && !control.enabled)
        {
            return Err(WorkerError::InvalidRequest);
        }
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

    fn core_lease(&self, audit_euid: u32) -> Result<Option<CoreInstanceLease>, WorkerError> {
        type LeaseRow = (
            Option<i64>,
            Option<i64>,
            Option<i64>,
            Option<i64>,
            Option<String>,
            Option<i64>,
            Option<i64>,
            Option<String>,
            Option<String>,
            Option<i64>,
            Option<String>,
            Option<String>,
        );
        let row: LeaseRow = self.connection.query_row(
            "SELECT lease_app_pid, lease_app_start_time_us, lease_core_pid,
                    lease_core_start_time_us, lease_core_audit_token_hex,
                    lease_codex_pid, lease_codex_start_time_us, lease_codex_audit_token_hex,
                    lease_core_instance_nonce,
                    lease_issued_at_ms, lease_broker_key_id, lease_broker_signature_hex
             FROM broker_state WHERE singleton = 1",
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                    row.get(8)?,
                    row.get(9)?,
                    row.get(10)?,
                    row.get(11)?,
                ))
            },
        )?;
        match row {
            (None, None, None, None, None, None, None, None, None, None, None, None) => Ok(None),
            (
                Some(app_pid),
                Some(app_start),
                Some(core_pid),
                Some(core_start),
                Some(core_audit_token),
                Some(codex_pid),
                Some(codex_start),
                Some(codex_audit_token),
                Some(nonce),
                Some(issued),
                Some(key_id),
                Some(signature),
            ) => {
                let lease = CoreInstanceLease {
                    protocol_version: EFFECT_PROTOCOL_VERSION,
                    audit_euid,
                    app_pid: i32::try_from(app_pid)
                        .map_err(|_| WorkerError::InvalidProtectedState)?,
                    app_start_time_us: u64::try_from(app_start)
                        .map_err(|_| WorkerError::InvalidProtectedState)?,
                    core_pid: i32::try_from(core_pid)
                        .map_err(|_| WorkerError::InvalidProtectedState)?,
                    core_start_time_us: u64::try_from(core_start)
                        .map_err(|_| WorkerError::InvalidProtectedState)?,
                    core_audit_token_hex: core_audit_token,
                    codex_pid: i32::try_from(codex_pid)
                        .map_err(|_| WorkerError::InvalidProtectedState)?,
                    codex_start_time_us: u64::try_from(codex_start)
                        .map_err(|_| WorkerError::InvalidProtectedState)?,
                    codex_audit_token_hex: codex_audit_token,
                    core_instance_nonce: nonce,
                    issued_at_ms: issued,
                    broker_key_id: key_id,
                    broker_signature_hex: signature,
                };
                validate_core_lease_shape(&lease)?;
                Ok(Some(lease))
            }
            _ => Err(WorkerError::InvalidProtectedState),
        }
    }

    fn acquire_core_lease(
        &mut self,
        audit_euid: u32,
        request: &CoreLeaseAcquireRequest,
        signing_key: &SigningKey,
        now_ms: i64,
    ) -> Result<CoreInstanceLease, WorkerError> {
        request.validate()?;
        let candidate = request.to_lease(audit_euid, signing_key, now_ms)?;
        if self.core_lease(audit_euid)?.is_some() {
            return Err(WorkerError::InvalidRequest);
        }
        let transaction = self.connection.transaction()?;
        let occupied: i64 = transaction.query_row(
            "SELECT COUNT(*) FROM broker_state WHERE singleton = 1 AND lease_core_pid IS NOT NULL",
            [],
            |row| row.get(0),
        )?;
        if occupied != 0 {
            return Err(WorkerError::InvalidRequest);
        }
        let changed = transaction.execute(
            "UPDATE broker_state SET
                lease_app_pid = ?1, lease_app_start_time_us = ?2,
                lease_core_pid = ?3, lease_core_start_time_us = ?4,
                lease_core_audit_token_hex = ?5,
                lease_codex_pid = ?6, lease_codex_start_time_us = ?7,
                lease_codex_audit_token_hex = ?8,
                lease_core_instance_nonce = ?9, lease_issued_at_ms = ?10,
                lease_broker_key_id = ?11, lease_broker_signature_hex = ?12
             WHERE singleton = 1 AND lease_core_pid IS NULL",
            params![
                candidate.app_pid,
                i64::try_from(candidate.app_start_time_us)
                    .map_err(|_| WorkerError::InvalidRequest)?,
                candidate.core_pid,
                i64::try_from(candidate.core_start_time_us)
                    .map_err(|_| WorkerError::InvalidRequest)?,
                candidate.core_audit_token_hex,
                candidate.codex_pid,
                i64::try_from(candidate.codex_start_time_us)
                    .map_err(|_| WorkerError::InvalidRequest)?,
                candidate.codex_audit_token_hex,
                candidate.core_instance_nonce,
                candidate.issued_at_ms,
                candidate.broker_key_id,
                candidate.broker_signature_hex
            ],
        )?;
        if changed != 1 {
            return Err(WorkerError::InvalidRequest);
        }
        transaction.commit()?;
        Ok(candidate)
    }

    fn release_core_lease(
        &mut self,
        audit_euid: u32,
        request: &CoreLeaseReleaseRequest,
    ) -> Result<(), WorkerError> {
        request.validate()?;
        let existing = self
            .core_lease(audit_euid)?
            .ok_or(WorkerError::InvalidRequest)?;
        if existing != request.lease {
            return Err(WorkerError::InvalidRequest);
        }
        let changed = self.connection.execute(
            "UPDATE broker_state SET
                lease_app_pid = NULL, lease_app_start_time_us = NULL,
                lease_core_pid = NULL, lease_core_start_time_us = NULL,
                lease_core_audit_token_hex = NULL,
                lease_codex_pid = NULL, lease_codex_start_time_us = NULL,
                lease_codex_audit_token_hex = NULL,
                lease_core_instance_nonce = NULL, lease_issued_at_ms = NULL,
                lease_broker_key_id = NULL, lease_broker_signature_hex = NULL
             WHERE singleton = 1 AND lease_broker_signature_hex = ?1",
            params![request.lease.broker_signature_hex],
        )?;
        if changed == 1 {
            Ok(())
        } else {
            Err(WorkerError::InvalidRequest)
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CoreLeaseAcquireRequest {
    #[serde(rename = "type")]
    request_type: String,
    version: u32,
    app_pid: i32,
    app_start_time_us: u64,
    core_pid: i32,
    core_start_time_us: u64,
    core_audit_token_hex: String,
    codex_pid: i32,
    codex_start_time_us: u64,
    codex_audit_token_hex: String,
    core_instance_nonce: String,
}

impl CoreLeaseAcquireRequest {
    fn validate(&self) -> Result<(), WorkerError> {
        if self.request_type != "coreLeaseAcquire"
            || self.version != 1
            || self.app_pid <= 0
            || self.app_start_time_us == 0
            || self.core_pid <= 0
            || self.core_start_time_us == 0
            || !is_lower_hex(&self.core_audit_token_hex, 64)
            || self.codex_pid <= 0
            || self.codex_start_time_us == 0
            || !is_lower_hex(&self.codex_audit_token_hex, 64)
            || !is_lower_hex(&self.core_instance_nonce, 64)
        {
            return Err(WorkerError::InvalidRequest);
        }
        Ok(())
    }

    fn to_lease(
        &self,
        audit_euid: u32,
        key: &SigningKey,
        now_ms: i64,
    ) -> Result<CoreInstanceLease, WorkerError> {
        let mut lease = CoreInstanceLease {
            protocol_version: EFFECT_PROTOCOL_VERSION,
            audit_euid,
            app_pid: self.app_pid,
            app_start_time_us: self.app_start_time_us,
            core_pid: self.core_pid,
            core_start_time_us: self.core_start_time_us,
            core_audit_token_hex: self.core_audit_token_hex.clone(),
            codex_pid: self.codex_pid,
            codex_start_time_us: self.codex_start_time_us,
            codex_audit_token_hex: self.codex_audit_token_hex.clone(),
            core_instance_nonce: self.core_instance_nonce.clone(),
            issued_at_ms: now_ms,
            broker_key_id: sha256_hex(key.verifying_key().to_bytes()),
            broker_signature_hex: String::new(),
        };
        let bytes = core_instance_lease_signing_bytes(&lease)
            .map_err(|_| WorkerError::InvalidProtectedState)?;
        lease.broker_signature_hex = hex::encode(key.sign(&bytes).to_bytes());
        Ok(lease)
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CoreLeaseReleaseRequest {
    #[serde(rename = "type")]
    request_type: String,
    version: u32,
    lease: CoreInstanceLease,
}

impl CoreLeaseReleaseRequest {
    fn validate(&self) -> Result<(), WorkerError> {
        if self.request_type != "coreLeaseRelease" || self.version != 1 {
            return Err(WorkerError::InvalidRequest);
        }
        validate_core_lease_shape(&self.lease)
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
struct RuntimeControlRequest {
    #[serde(rename = "type")]
    request_type: String,
    version: u32,
    control: RuntimeControlAuthorization,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StatusRequest {
    #[serde(rename = "type")]
    request_type: String,
    version: u32,
    challenge: String,
}

impl StatusRequest {
    fn validate(&self) -> Result<(), WorkerError> {
        if self.request_type == "brokerStatus"
            && self.version == 1
            && is_lower_hex(&self.challenge, 64)
        {
            Ok(())
        } else {
            Err(WorkerError::InvalidRequest)
        }
    }
}

impl RuntimeControlRequest {
    fn validate(&self) -> Result<(), WorkerError> {
        if self.request_type == "applyRuntimeControl" && self.version == 1 {
            Ok(())
        } else {
            Err(WorkerError::InvalidRequest)
        }
    }
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

fn status_response(
    state: &ProtectedState,
    signing_key: &SigningKey,
    request: &StatusRequest,
) -> Result<serde_json::Value, WorkerError> {
    request.validate()?;
    let runtime_checkpoint = state.runtime_checkpoint()?;
    let runtime_control = runtime_checkpoint
        .as_ref()
        .map(|(control, _)| control.clone());
    let runtime_receipt = runtime_checkpoint
        .as_ref()
        .map(|(control, nonce)| {
            sign_runtime_control_receipt(control, nonce, Some(&request.challenge), signing_key)
        })
        .transpose()?;
    Ok(json!({
        "coreEnrolled": state.core_key()?.is_some(),
        "runtimeControl": runtime_control,
        "runtimeReceipt": runtime_receipt,
        "status": "ready",
        "version": 1,
    }))
}

fn validate_core_lease_shape(lease: &CoreInstanceLease) -> Result<(), WorkerError> {
    if lease.protocol_version != EFFECT_PROTOCOL_VERSION
        || lease.audit_euid == 0
        || lease.app_pid <= 0
        || lease.app_start_time_us == 0
        || lease.core_pid <= 0
        || lease.core_start_time_us == 0
        || !is_lower_hex(&lease.core_audit_token_hex, 64)
        || lease.codex_pid <= 0
        || lease.codex_start_time_us == 0
        || !is_lower_hex(&lease.codex_audit_token_hex, 64)
        || !is_lower_hex(&lease.core_instance_nonce, 64)
        || lease.issued_at_ms <= 0
        || !is_lower_hex(&lease.broker_key_id, 64)
        || !is_lower_hex(&lease.broker_signature_hex, 128)
    {
        return Err(WorkerError::InvalidProtectedState);
    }
    Ok(())
}

fn apply_runtime_response(
    state: &mut ProtectedState,
    paths: &RuntimePaths,
    signing_key: &SigningKey,
    request: &RuntimeControlRequest,
) -> Result<serde_json::Value, WorkerError> {
    request.validate()?;
    let _operation_guard = acquire_protected_operation_guard(&paths.missions_root())?;
    let checkpoint_nonce = state.apply_runtime_control(&request.control)?;
    let receipt =
        sign_runtime_control_receipt(&request.control, &checkpoint_nonce, None, signing_key)?;
    Ok(json!({
        "runtimeControl": request.control,
        "runtimeReceipt": receipt,
        "status": "accepted",
        "version": 1
    }))
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
        "status" => {
            let request: StatusRequest = read_json_line(&mut io::stdin().lock())?;
            status_response(&state, &signing_key, &request)
        }
        "session" => serde_json::to_value(state.session(&signing_key, now_ms)?)
            .map_err(|_| WorkerError::InvalidProtectedState),
        "enroll-core" => {
            let request: EnrollCoreRequest = read_json_line(&mut io::stdin().lock())?;
            state.enroll_core(&request)?;
            Ok(json!({ "status": "enrolled", "version": 1 }))
        }
        "core-lease-status" => Ok(json!({
            "lease": state.core_lease(audit_euid)?,
            "status": "ready",
            "version": 1,
        })),
        "core-lease-acquire" => {
            let request: CoreLeaseAcquireRequest = read_json_line(&mut io::stdin().lock())?;
            let lease = state.acquire_core_lease(audit_euid, &request, &signing_key, now_ms)?;
            Ok(json!({"lease": lease, "status": "accepted", "version": 1}))
        }
        "core-lease-release" => {
            let request: CoreLeaseReleaseRequest = read_json_line(&mut io::stdin().lock())?;
            state.release_core_lease(audit_euid, &request)?;
            Ok(json!({"status": "released", "version": 1}))
        }
        "runtime-control" => {
            let request: RuntimeControlRequest = read_json_line(&mut io::stdin().lock())?;
            apply_runtime_response(&mut state, &paths, &signing_key, &request)
        }
        "put" => {
            let mut input = BufReader::new(io::stdin().lock());
            let request: PutRequest = read_json_line(&mut input)?;
            request.validate()?;
            let operation_guard = acquire_protected_operation_guard(&paths.missions_root())?;
            let core_key = state.core_key()?.ok_or(WorkerError::CoreNotEnrolled)?;
            state.require_runtime_for(&request.permit)?;
            let session = state.session(&signing_key, now_ms)?;
            let config = BrokerConfig {
                protected_root: paths.missions_root(),
                authenticated_audit_euid: audit_euid,
                enrolled_core_verifying_key: core_key,
                broker_signing_seed: signing_key.to_bytes(),
                session_nonce: session.session_nonce,
                session_expires_at_ms: session.expires_at_ms,
            };
            let mut engine = BrokerEngine::open_with_operation_guard(config, operation_guard)?;
            let receipt = engine.put_file_with_commit_fence(&request.permit, input, || {
                state
                    .require_runtime_for(&request.permit)
                    .map_err(|_| openopen_effect_broker::BrokerError::RuntimeRevoked)
            })?;
            serde_json::to_value(receipt).map_err(|_| WorkerError::InvalidProtectedState)
        }
        "reconcile" => {
            let request: ReconcileRequest = read_json_line(&mut io::stdin().lock())?;
            request.validate()?;
            let operation_guard = acquire_protected_operation_guard(&paths.missions_root())?;
            let core_key = state.core_key()?.ok_or(WorkerError::CoreNotEnrolled)?;
            state.require_runtime_for(&request.permit)?;
            let session = state.session(&signing_key, now_ms)?;
            let config = BrokerConfig {
                protected_root: paths.missions_root(),
                authenticated_audit_euid: audit_euid,
                enrolled_core_verifying_key: core_key,
                broker_signing_seed: signing_key.to_bytes(),
                session_nonce: session.session_nonce,
                session_expires_at_ms: session.expires_at_ms,
            };
            let mut engine = BrokerEngine::open_with_operation_guard(config, operation_guard)?;
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

fn verify_runtime_control(
    authorization: &RuntimeControlAuthorization,
    core_key: &[u8; 32],
) -> Result<(), WorkerError> {
    if authorization.protocol_version != EFFECT_PROTOCOL_VERSION
        || authorization.revision == 0
        || authorization.updated_at_ms < 0
        || authorization.core_key_id != sha256_hex(core_key)
    {
        return Err(WorkerError::InvalidRequest);
    }
    let signature_bytes: [u8; 64] = hex::decode(&authorization.authorization_signature_hex)
        .map_err(|_| WorkerError::InvalidRequest)?
        .try_into()
        .map_err(|_| WorkerError::InvalidRequest)?;
    let signature = Signature::from_bytes(&signature_bytes);
    let key = VerifyingKey::from_bytes(core_key).map_err(|_| WorkerError::InvalidProtectedState)?;
    let bytes = runtime_control_authorization_signing_bytes(authorization)
        .map_err(|_| WorkerError::InvalidRequest)?;
    key.verify(&bytes, &signature)
        .map_err(|_| WorkerError::InvalidRequest)
}

fn sign_runtime_control_receipt(
    authorization: &RuntimeControlAuthorization,
    checkpoint_nonce: &str,
    request_nonce: Option<&str>,
    key: &SigningKey,
) -> Result<RuntimeControlReceipt, WorkerError> {
    let mut receipt = RuntimeControlReceipt {
        protocol_version: EFFECT_PROTOCOL_VERSION,
        authorization_hash: runtime_control_authorization_hash(authorization)
            .map_err(|_| WorkerError::InvalidRequest)?,
        checkpoint_nonce: checkpoint_nonce.to_owned(),
        request_nonce: request_nonce.map(ToOwned::to_owned),
        broker_key_id: sha256_hex(key.verifying_key().to_bytes()),
        broker_signature_hex: String::new(),
    };
    let bytes = runtime_control_receipt_signing_bytes(&receipt)
        .map_err(|_| WorkerError::InvalidProtectedState)?;
    receipt.broker_signature_hex = hex::encode(key.sign(&bytes).to_bytes());
    Ok(receipt)
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
        CoreLeaseAcquireRequest, CoreLeaseReleaseRequest, EnrollCoreRequest, ProtectedState,
        PutRequest, ReconcileRequest, WorkerError, read_json_line, validate_key_pair,
    };
    use ed25519_dalek::{Signer, SigningKey};
    use openopen_protocol::{
        EFFECT_PROTOCOL_VERSION, EffectPermitPurpose, RuntimeControlAuthorization,
        runtime_control_authorization_signing_bytes,
    };
    use sha2::{Digest, Sha256};
    use std::io::BufReader;
    use std::sync::{Arc, Barrier};

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

    #[test]
    fn protected_runtime_revision_revokes_every_older_execute_permit() {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("state.sqlite3");
        let core_signing = SigningKey::from_bytes(&[7_u8; 32]);
        let core_key = core_signing.verifying_key().to_bytes();
        let request = EnrollCoreRequest {
            request_type: "enrollCore".into(),
            version: 1,
            core_key_id: format!("{:x}", Sha256::digest(core_key)),
            core_verifying_key_hex: hex::encode(core_key),
        };
        let mut state = ProtectedState::open(&database).unwrap();
        state.enroll_core(&request).unwrap();

        let on = signed_runtime_control(&core_signing, true, 1, 10);
        state.apply_runtime_control(&on).unwrap();
        state
            .require_runtime_revision(1, EffectPermitPurpose::Execute)
            .unwrap();

        let off = signed_runtime_control(&core_signing, false, 2, 11);
        state.apply_runtime_control(&off).unwrap();
        assert!(
            state
                .require_runtime_revision(1, EffectPermitPurpose::Execute)
                .is_err()
        );
        assert!(
            state
                .require_runtime_revision(2, EffectPermitPurpose::Execute)
                .is_err()
        );
        state
            .require_runtime_revision(2, EffectPermitPurpose::ReattestOnly)
            .unwrap();
        assert!(state.apply_runtime_control(&on).is_err());
    }

    #[test]
    fn protected_core_lease_is_durable_exclusive_and_exactly_released() {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("state.sqlite3");
        let broker_key = SigningKey::from_bytes(&[8_u8; 32]);
        let first_request = CoreLeaseAcquireRequest {
            request_type: "coreLeaseAcquire".into(),
            version: 1,
            app_pid: 101,
            app_start_time_us: 1_001,
            core_pid: 102,
            core_start_time_us: 1_002,
            core_audit_token_hex: "aa".repeat(32),
            codex_pid: 103,
            codex_start_time_us: 1_003,
            codex_audit_token_hex: "bb".repeat(32),
            core_instance_nonce: "ab".repeat(32),
        };
        let mut state = ProtectedState::open(&database).unwrap();
        let lease = state
            .acquire_core_lease(501, &first_request, &broker_key, 10_000)
            .unwrap();
        drop(state);

        let mut restarted = ProtectedState::open(&database).unwrap();
        assert_eq!(restarted.core_lease(501).unwrap(), Some(lease.clone()));
        let competing = CoreLeaseAcquireRequest {
            core_pid: 202,
            core_start_time_us: 2_002,
            core_audit_token_hex: "cc".repeat(32),
            codex_pid: 203,
            codex_start_time_us: 2_003,
            codex_audit_token_hex: "dd".repeat(32),
            core_instance_nonce: "cd".repeat(32),
            ..first_request
        };
        assert!(matches!(
            restarted.acquire_core_lease(501, &competing, &broker_key, 10_001),
            Err(WorkerError::InvalidRequest)
        ));
        for forged in [
            {
                let mut forged = lease.clone();
                forged.core_instance_nonce = "ef".repeat(32);
                forged
            },
            {
                let mut forged = lease.clone();
                forged.core_audit_token_hex = "cc".repeat(32);
                forged
            },
            {
                let mut forged = lease.clone();
                forged.codex_pid += 1;
                forged
            },
            {
                let mut forged = lease.clone();
                forged.codex_audit_token_hex = "dd".repeat(32);
                forged
            },
        ] {
            assert!(matches!(
                restarted.release_core_lease(
                    501,
                    &CoreLeaseReleaseRequest {
                        request_type: "coreLeaseRelease".into(),
                        version: 1,
                        lease: forged,
                    },
                ),
                Err(WorkerError::InvalidRequest)
            ));
        }
        restarted
            .release_core_lease(
                501,
                &CoreLeaseReleaseRequest {
                    request_type: "coreLeaseRelease".into(),
                    version: 1,
                    lease,
                },
            )
            .unwrap();
        assert!(restarted.core_lease(501).unwrap().is_none());
        restarted
            .acquire_core_lease(501, &competing, &broker_key, 10_002)
            .unwrap();
    }

    #[test]
    fn concurrent_protected_core_lease_acquire_has_exactly_one_winner() {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("state.sqlite3");
        let first_state = ProtectedState::open(&database).unwrap();
        let second_state = ProtectedState::open(&database).unwrap();
        let barrier = Arc::new(Barrier::new(3));
        let mut joins = Vec::new();
        for (mut state, core_pid, nonce) in [
            (first_state, 301, "11".repeat(32)),
            (second_state, 302, "22".repeat(32)),
        ] {
            let barrier = barrier.clone();
            joins.push(std::thread::spawn(move || {
                let key = SigningKey::from_bytes(&[8_u8; 32]);
                let request = CoreLeaseAcquireRequest {
                    request_type: "coreLeaseAcquire".into(),
                    version: 1,
                    app_pid: core_pid - 1,
                    app_start_time_us: u64::try_from(core_pid - 1).unwrap(),
                    core_pid,
                    core_start_time_us: u64::try_from(core_pid).unwrap(),
                    core_audit_token_hex: format!("{core_pid:064x}"),
                    codex_pid: core_pid + 100,
                    codex_start_time_us: u64::try_from(core_pid + 100).unwrap(),
                    codex_audit_token_hex: format!("{:064x}", core_pid + 100),
                    core_instance_nonce: nonce,
                };
                barrier.wait();
                state
                    .acquire_core_lease(501, &request, &key, 20_000)
                    .is_ok()
            }));
        }
        barrier.wait();
        let winners = joins
            .into_iter()
            .map(|join| join.join().unwrap())
            .filter(|won| *won)
            .count();
        assert_eq!(winners, 1);
        assert!(
            ProtectedState::open(&database)
                .unwrap()
                .core_lease(501)
                .unwrap()
                .is_some()
        );
    }

    fn signed_runtime_control(
        key: &SigningKey,
        enabled: bool,
        revision: u64,
        updated_at_ms: i64,
    ) -> RuntimeControlAuthorization {
        let mut authorization = RuntimeControlAuthorization {
            protocol_version: EFFECT_PROTOCOL_VERSION,
            enabled,
            revision,
            updated_at_ms,
            core_key_id: format!("{:x}", Sha256::digest(key.verifying_key().to_bytes())),
            authorization_signature_hex: String::new(),
        };
        let bytes = runtime_control_authorization_signing_bytes(&authorization).unwrap();
        authorization.authorization_signature_hex = hex::encode(key.sign(&bytes).to_bytes());
        authorization
    }
}
