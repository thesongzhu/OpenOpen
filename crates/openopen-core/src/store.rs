use crate::mission::{apply_mission_command, validate_mission_snapshot, validate_receipt};
use crate::{
    ActionGate, ActionProposal, ActionTarget, CryptoError, EffectKind, GateDecision,
    LocalAuthority, MissionCommand, MissionError, TrustedBrokerEnrollment,
};
use openopen_protocol::{
    ApprovalKind, ChannelEnvelope, EFFECT_PROTOCOL_VERSION, EffectAuditAnchor, EffectBrokerSession,
    EffectCommand, EffectNonCommit, EffectPermit, EffectPermitPurpose, EffectReceipt,
    MAX_EFFECT_APPROVAL_IDS, MAX_EFFECT_PAYLOAD_BYTES, MAX_EFFECT_SCOPE_DIGEST_BYTES, Mission,
    MissionFileEffect, PayloadDescriptor, Receipt, RuntimeControlAuthorization,
    RuntimeControlReceipt, is_canonical_effect_identifier,
};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

const MISSION_COMMAND_ACTION: &str = "mission.command";
const RECEIPT_COMMIT_ACTION: &str = "receipt.created";
const EFFECT_AUTHORIZATION_ACTION: &str = "effect.authorized";
const EFFECT_RECEIPT_ACTION: &str = "effect.committed";
const EFFECT_NONCOMMIT_ACTION: &str = "effect.not_committed";
const RUNTIME_CONTROL_ID: i64 = 1;
const STORE_SCHEMA: &str = "PRAGMA foreign_keys = ON;
CREATE TABLE IF NOT EXISTS runtime_control (
 singleton_id INTEGER PRIMARY KEY CHECK (singleton_id = 1),
 enabled INTEGER NOT NULL CHECK (enabled IN (0, 1)),
 revision INTEGER NOT NULL CHECK (revision > 0),
 updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= 0),
 signature_hex TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS runtime_control_history (
 revision INTEGER PRIMARY KEY CHECK (revision > 0),
 enabled INTEGER NOT NULL CHECK (enabled IN (0, 1)),
 updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= 0),
 signature_hex TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS runtime_control_recovery_checkpoint (
 revision INTEGER PRIMARY KEY CHECK (revision > 0),
 authorization_hash TEXT NOT NULL,
 checkpoint_nonce TEXT NOT NULL,
 request_nonce TEXT,
 broker_key_id TEXT NOT NULL,
 broker_signature_hex TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS mission_state (
 mission_id TEXT PRIMARY KEY, status_json TEXT NOT NULL, scope_digest TEXT NOT NULL,
 encrypted_blob BLOB NOT NULL, created_at_ms INTEGER NOT NULL, updated_at_ms INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS receipt_state (
 receipt_id TEXT PRIMARY KEY, mission_id TEXT NOT NULL, encrypted_blob BLOB NOT NULL,
 completed_at_ms INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS channel_envelope (
 channel_json TEXT NOT NULL, source_message_id TEXT NOT NULL, sender_id TEXT NOT NULL,
 conversation_id TEXT NOT NULL, content_sha256 TEXT NOT NULL, received_at_ms INTEGER NOT NULL,
 PRIMARY KEY(channel_json, source_message_id)
);
CREATE TABLE IF NOT EXISTS audit_ledger (
 sequence INTEGER PRIMARY KEY AUTOINCREMENT, audit_id TEXT NOT NULL UNIQUE,
 command_id TEXT NOT NULL, command_hash TEXT NOT NULL, actor TEXT NOT NULL,
 action TEXT NOT NULL, entity_id TEXT NOT NULL, created_at_ms INTEGER NOT NULL,
 observed_at_ms INTEGER NOT NULL, state_kind TEXT NOT NULL, state_hash TEXT NOT NULL,
 previous_hash TEXT NOT NULL, entry_hash TEXT NOT NULL, signature_hex TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS mission_command_result (
 command_id TEXT PRIMARY KEY, mission_id TEXT NOT NULL, command_hash TEXT NOT NULL,
 encrypted_result BLOB NOT NULL, result_hash TEXT NOT NULL, anchor_sequence INTEGER NOT NULL,
 anchor_entry_hash TEXT NOT NULL, anchor_signature_hex TEXT NOT NULL,
 record_signature_hex TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS effect_authorization (
 effect_id TEXT PRIMARY KEY, mission_id TEXT NOT NULL, stable_effect_hash TEXT NOT NULL,
 encrypted_command BLOB NOT NULL, command_blob_hash TEXT NOT NULL,
 source_anchor_sequence INTEGER NOT NULL, source_anchor_entry_hash TEXT NOT NULL,
 source_anchor_signature_hex TEXT NOT NULL, record_signature_hex TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS effect_receipt (
 effect_id TEXT PRIMARY KEY, mission_id TEXT NOT NULL, stable_effect_hash TEXT NOT NULL,
 encrypted_record BLOB NOT NULL, record_hash TEXT NOT NULL, anchor_sequence INTEGER NOT NULL,
 anchor_entry_hash TEXT NOT NULL, anchor_signature_hex TEXT NOT NULL,
 local_signature_hex TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS effect_fence (
 effect_id TEXT PRIMARY KEY, mission_id TEXT NOT NULL, stable_effect_hash TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS effect_noncommit (
 effect_id TEXT PRIMARY KEY, mission_id TEXT NOT NULL, stable_effect_hash TEXT NOT NULL,
 encrypted_record BLOB NOT NULL, record_hash TEXT NOT NULL, anchor_sequence INTEGER NOT NULL,
 anchor_entry_hash TEXT NOT NULL, anchor_signature_hex TEXT NOT NULL,
 local_signature_hex TEXT NOT NULL
);";

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("cryptographic storage error: {0}")]
    Crypto(#[from] CryptoError),
    #[error("Mission or Receipt invariant failed: {0}")]
    Domain(#[from] MissionError),
    #[error("channel message id was reused with different authorization provenance")]
    EnvelopeConflict,
    #[error("audit chain mismatch at sequence {0}")]
    AuditChainMismatch(i64),
    #[error("audit tail does not match the Keychain-owned anchor")]
    AuditAnchorMismatch,
    #[error("audit ledger is empty")]
    EmptyAuditLedger,
    #[error("existing nonempty audit ledger predates Store-observed ordering proof")]
    LegacyAuditObservationMissing,
    #[error("stored state is not bound to its latest signed audit row: {0}")]
    StateBindingMismatch(String),
    #[error("Receipt commit does not match the currently stored completed Mission")]
    MissionStateMismatch,
    #[error("Receipt id already exists and Receipts are immutable")]
    ReceiptAlreadyExists,
    #[error("command id must be non-empty")]
    InvalidCommandId,
    #[error("command id was reused with a different typed command")]
    CommandConflict,
    #[error("stored command result does not match its original bound result")]
    CommandResultMismatch,
    #[error("Mission command batch is empty, mixed, duplicated, or has an invalid chained anchor")]
    InvalidCommandBatch,
    #[error("effect id must be one canonical lowercase identifier")]
    InvalidEffectId,
    #[error("effect id was reused with a different typed effect")]
    EffectConflict,
    #[error("effect payload exceeds the bounded broker limit")]
    EffectPayloadTooLarge,
    #[error("effect authorization failed: {0:?}")]
    EffectAuthorization(GateDecision),
    #[error("stored effect authorization does not match its signed record: {0}")]
    EffectAuthorizationMismatch(String),
    #[error("effect protocol failed: {0}")]
    EffectProtocol(#[from] crate::EffectProtocolError),
    #[error("system clock is outside the supported effect-permit range")]
    InvalidSystemTime,
    #[error("Mission was not found for effect authorization")]
    MissionNotFound,
    #[error("effect Receipt conflicts with the immutable committed result")]
    EffectReceiptConflict,
    #[error("stored effect Receipt does not match its signed record: {0}")]
    EffectReceiptMismatch(String),
    #[error("audit advancement is fenced by unresolved effect: {0}")]
    EffectFenceActive(String),
    #[error("effect outcome does not match the one unresolved fence")]
    EffectFenceMismatch,
    #[error("effect was definitively reconciled as not committed")]
    EffectNotCommitted,
    #[error("effect noncommit conflicts with the immutable reconciled result")]
    EffectNonCommitConflict,
    #[error("stored effect noncommit does not match its signed record: {0}")]
    EffectNonCommitMismatch(String),
    #[error("no independently provisioned effect-broker enrollment is available")]
    MissingTrustedBrokerEnrollment,
    #[error("runtime control timestamp must be nonnegative")]
    InvalidRuntimeControlTimestamp,
    #[error("runtime control revision overflowed")]
    RuntimeControlRevisionOverflow,
    #[error("stored runtime control does not match its signed record")]
    RuntimeControlMismatch,
    #[error("OpenOpen is off")]
    RuntimeDisabled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EnvelopeInsert {
    Inserted,
    Duplicate,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditAnchor {
    pub sequence: i64,
    pub entry_hash: String,
    pub signature_hex: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MissionCommandEnvelope {
    pub command_id: String,
    pub expected_anchor: Option<AuditAnchor>,
    pub command: MissionCommand,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MissionCommandResult {
    pub mission: Mission,
    pub receipt: Option<Receipt>,
    pub anchor: AuditAnchor,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeControl {
    pub enabled: bool,
    pub revision: u64,
    pub updated_at_ms: i64,
}

struct AuditRecord<'a> {
    id: &'a str,
    command_id: &'a str,
    command_hash: &'a str,
    actor: &'a str,
    action: &'a str,
    entity_id: &'a str,
    created_at_ms: i64,
    state_kind: &'a str,
    state_hash: &'a str,
}

struct CommandAuditContext<'a> {
    command_id: &'a str,
    command_hash: &'a str,
    actor: &'a str,
}

struct StoredCommandResult {
    command_id: String,
    mission_id: String,
    command_hash: String,
    encrypted_result: Vec<u8>,
    result_hash: String,
    anchor: AuditAnchor,
    record_signature_hex: String,
}

struct StoredEffectAuthorization {
    effect_id: String,
    mission_id: String,
    stable_effect_hash: String,
    encrypted_command: Vec<u8>,
    command_blob_hash: String,
    source_anchor: AuditAnchor,
    record_signature_hex: String,
}

struct MissionFilePutRequest {
    path_components: Vec<String>,
    payload_sha256: String,
    payload_byte_len: u64,
    action_digest: String,
}

struct EffectResolution<'a, 'transaction> {
    transaction: &'a Transaction<'transaction>,
    authority: &'a LocalAuthority,
    effect_id: &'a str,
    expected_anchor: &'a AuditAnchor,
    proposal: &'a ActionProposal,
    payload: &'a [u8],
    trusted_broker: &'a TrustedBrokerEnrollment,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct StoredEffectReceiptPayload {
    broker_session: EffectBrokerSession,
    permit: EffectPermit,
    receipt: EffectReceipt,
}

struct StoredEffectReceipt {
    effect_id: String,
    mission_id: String,
    stable_effect_hash: String,
    encrypted_record: Vec<u8>,
    record_hash: String,
    anchor: AuditAnchor,
    local_signature_hex: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct StoredEffectNonCommitPayload {
    broker_session: EffectBrokerSession,
    permit: EffectPermit,
    attestation: EffectNonCommit,
}

struct StoredEffectNonCommit {
    effect_id: String,
    mission_id: String,
    stable_effect_hash: String,
    encrypted_record: Vec<u8>,
    record_hash: String,
    anchor: AuditAnchor,
    local_signature_hex: String,
}

pub struct Store {
    connection: Connection,
    authority: LocalAuthority,
    trusted_broker: Option<TrustedBrokerEnrollment>,
}

impl Store {
    /// Opens a persistent encrypted store using master material loaded by the host.
    ///
    /// # Errors
    ///
    /// Returns a database or migration error.
    pub fn open(path: &Path, authority: LocalAuthority) -> Result<Self, StoreError> {
        let connection = Connection::open(path)?;
        let store = Self {
            connection,
            authority,
            trusted_broker: None,
        };
        store.migrate()?;
        Ok(store)
    }

    /// Opens an encrypted in-memory store for deterministic tests.
    ///
    /// # Errors
    ///
    /// Returns a database or migration error.
    pub fn open_in_memory(authority: LocalAuthority) -> Result<Self, StoreError> {
        let connection = Connection::open_in_memory()?;
        let store = Self {
            connection,
            authority,
            trusted_broker: None,
        };
        store.migrate()?;
        Ok(store)
    }

    /// Opens a persistent store with broker trust material provisioned by the
    /// authenticated privileged-helper installation flow.
    ///
    /// # Errors
    ///
    /// Returns a database or migration error.
    pub fn open_with_trusted_broker(
        path: &Path,
        authority: LocalAuthority,
        trusted_broker: TrustedBrokerEnrollment,
    ) -> Result<Self, StoreError> {
        let connection = Connection::open(path)?;
        let store = Self {
            connection,
            authority,
            trusted_broker: Some(trusted_broker),
        };
        store.migrate()?;
        Ok(store)
    }

    /// Opens an in-memory store with explicit pinned broker trust for tests.
    ///
    /// # Errors
    ///
    /// Returns a database or migration error.
    pub fn open_in_memory_with_trusted_broker(
        authority: LocalAuthority,
        trusted_broker: TrustedBrokerEnrollment,
    ) -> Result<Self, StoreError> {
        let connection = Connection::open_in_memory()?;
        let store = Self {
            connection,
            authority,
            trusted_broker: Some(trusted_broker),
        };
        store.migrate()?;
        Ok(store)
    }

    #[must_use]
    pub const fn authority(&self) -> &LocalAuthority {
        &self.authority
    }

    #[must_use]
    pub const fn trusted_broker_enrollment(&self) -> Option<&TrustedBrokerEnrollment> {
        self.trusted_broker.as_ref()
    }

    /// Loads the exact Core-signed broker enrollment produced by the signed
    /// admin-approved installation flow. Exact retries are idempotent and a
    /// different broker identity is never accepted as rotation.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid Core signature, malformed enrollment,
    /// or attempted broker rotation.
    pub fn install_trusted_broker(
        &mut self,
        record: &crate::BrokerEnrollmentRecord,
    ) -> Result<(), StoreError> {
        let enrollment =
            TrustedBrokerEnrollment::from_signed_install_record(&self.authority, record)?;
        match &self.trusted_broker {
            None => self.trusted_broker = Some(enrollment),
            Some(existing) if existing == &enrollment => {}
            Some(_) => {
                return Err(StoreError::EffectProtocol(
                    crate::EffectProtocolError::UntrustedBroker,
                ));
            }
        }
        Ok(())
    }

    fn migrate(&self) -> Result<(), StoreError> {
        self.connection.execute_batch(STORE_SCHEMA)?;
        let mut columns = self.connection.prepare("PRAGMA table_info(audit_ledger)")?;
        let column_names = columns
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<Vec<_>, _>>()?;
        if !column_names.iter().any(|name| name == "observed_at_ms") {
            let row_count: i64 =
                self.connection
                    .query_row("SELECT COUNT(*) FROM audit_ledger", [], |row| row.get(0))?;
            if row_count != 0 {
                return Err(StoreError::LegacyAuditObservationMissing);
            }
            self.connection.execute(
                "ALTER TABLE audit_ledger ADD COLUMN observed_at_ms INTEGER NOT NULL",
                [],
            )?;
        }
        self.connection
            .prepare("SELECT record_signature_hex FROM mission_command_result LIMIT 0")?;
        self.connection
            .prepare("SELECT record_signature_hex FROM effect_authorization LIMIT 0")?;
        self.connection
            .prepare("SELECT local_signature_hex FROM effect_receipt LIMIT 0")?;
        self.connection
            .prepare("SELECT effect_id FROM effect_fence LIMIT 0")?;
        self.connection
            .prepare("SELECT local_signature_hex FROM effect_noncommit LIMIT 0")?;
        self.connection
            .prepare("SELECT signature_hex FROM runtime_control LIMIT 0")?;
        self.connection
            .prepare("SELECT signature_hex FROM runtime_control_history LIMIT 0")?;
        let recovery_columns = self
            .connection
            .prepare("PRAGMA table_info(runtime_control_recovery_checkpoint)")?
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<Vec<_>, _>>()?;
        if !recovery_columns.iter().any(|name| name == "request_nonce") {
            self.connection.execute(
                "ALTER TABLE runtime_control_recovery_checkpoint ADD COLUMN request_nonce TEXT",
                [],
            )?;
        }
        self.connection
            .prepare("SELECT checkpoint_nonce FROM runtime_control_recovery_checkpoint LIMIT 0")?;
        Ok(())
    }

    /// Reads the signed singleton runtime control. A missing row is the
    /// durable default-Off state; malformed or changed rows fail closed.
    ///
    /// # Errors
    ///
    /// Returns an error for database or signature mismatch.
    pub fn runtime_control(&self) -> Result<RuntimeControl, StoreError> {
        load_runtime_control(
            &self.connection,
            &self.authority,
            self.trusted_broker.as_ref(),
        )
    }

    /// Signs but does not persist the next runtime-control transition. The
    /// caller must first have the protected broker durably accept this exact
    /// authorization, then call [`Self::commit_runtime_control`].
    ///
    /// # Errors
    ///
    /// Returns an error for invalid time, corrupt current state, signing
    /// failure, or revision overflow.
    pub fn prepare_runtime_control(
        &self,
        enabled: bool,
        updated_at_ms: i64,
    ) -> Result<RuntimeControlAuthorization, StoreError> {
        if updated_at_ms < 0 {
            return Err(StoreError::InvalidRuntimeControlTimestamp);
        }
        let current = load_runtime_control(
            &self.connection,
            &self.authority,
            self.trusted_broker.as_ref(),
        )?;
        let revision = current
            .revision
            .checked_add(1)
            .ok_or(StoreError::RuntimeControlRevisionOverflow)?;
        let mut authorization = RuntimeControlAuthorization {
            protocol_version: EFFECT_PROTOCOL_VERSION,
            enabled,
            revision,
            updated_at_ms,
            core_key_id: self.authority.effect_key_id(),
            authorization_signature_hex: String::new(),
        };
        self.authority.sign_runtime_control(&mut authorization)?;
        Ok(authorization)
    }

    /// Commits the exact Core-signed transition previously accepted by the
    /// protected broker. Replays of the already-current value are idempotent;
    /// skipped, stale, changed, or foreign transitions fail closed.
    ///
    /// # Errors
    ///
    /// Returns an error unless both Core and the pinned broker signed the exact
    /// next transition, or if persistence/current-state verification fails.
    pub fn commit_runtime_control(
        &mut self,
        authorization: &RuntimeControlAuthorization,
        broker_receipt: &RuntimeControlReceipt,
    ) -> Result<RuntimeControl, StoreError> {
        if authorization.protocol_version != EFFECT_PROTOCOL_VERSION
            || authorization.updated_at_ms < 0
            || authorization.revision == 0
        {
            return Err(StoreError::RuntimeControlMismatch);
        }
        self.authority
            .verify_runtime_control(authorization)
            .map_err(|_| StoreError::RuntimeControlMismatch)?;
        let trusted_broker = self
            .trusted_broker
            .as_ref()
            .ok_or(StoreError::MissingTrustedBrokerEnrollment)?;
        crate::effect::verify_runtime_control_receipt(
            trusted_broker,
            authorization,
            broker_receipt,
        )?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current =
            load_runtime_control(&transaction, &self.authority, self.trusted_broker.as_ref())?;
        if current.enabled == authorization.enabled
            && current.revision == authorization.revision
            && current.updated_at_ms == authorization.updated_at_ms
        {
            transaction.commit()?;
            return Ok(current);
        }
        let expected_revision = current
            .revision
            .checked_add(1)
            .ok_or(StoreError::RuntimeControlRevisionOverflow)?;
        if authorization.revision != expected_revision {
            return Err(StoreError::RuntimeControlMismatch);
        }
        let revision_i64 = i64::try_from(authorization.revision)
            .map_err(|_| StoreError::RuntimeControlRevisionOverflow)?;
        transaction.execute(
            "INSERT INTO runtime_control_history
                (revision, enabled, updated_at_ms, signature_hex)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                revision_i64,
                i64::from(authorization.enabled),
                authorization.updated_at_ms,
                authorization.authorization_signature_hex,
            ],
        )?;
        transaction.execute(
            "INSERT INTO runtime_control
                (singleton_id, enabled, revision, updated_at_ms, signature_hex)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(singleton_id) DO UPDATE SET
                enabled = excluded.enabled,
                revision = excluded.revision,
                updated_at_ms = excluded.updated_at_ms,
                signature_hex = excluded.signature_hex",
            params![
                RUNTIME_CONTROL_ID,
                i64::from(authorization.enabled),
                revision_i64,
                authorization.updated_at_ms,
                authorization.authorization_signature_hex,
            ],
        )?;
        transaction.commit()?;
        Ok(RuntimeControl {
            enabled: authorization.enabled,
            revision: authorization.revision,
            updated_at_ms: authorization.updated_at_ms,
        })
    }

    /// Recovers a rolled-back Core Store from the protected broker's current,
    /// nonce-bound signed checkpoint. Only a strictly newer broker revision is
    /// allowed to jump the local history; stale or changed proofs fail closed.
    ///
    /// # Errors
    ///
    /// Returns an error unless the exact authorization is signed by Core and
    /// its checkpoint Receipt is signed by the pinned protected broker.
    pub fn recover_runtime_control(
        &mut self,
        authorization: &RuntimeControlAuthorization,
        broker_receipt: &RuntimeControlReceipt,
    ) -> Result<RuntimeControl, StoreError> {
        if authorization.protocol_version != EFFECT_PROTOCOL_VERSION
            || authorization.updated_at_ms < 0
            || authorization.revision == 0
        {
            return Err(StoreError::RuntimeControlMismatch);
        }
        self.authority
            .verify_runtime_control(authorization)
            .map_err(|_| StoreError::RuntimeControlMismatch)?;
        let trusted_broker = self
            .trusted_broker
            .as_ref()
            .ok_or(StoreError::MissingTrustedBrokerEnrollment)?;
        crate::effect::verify_runtime_control_receipt(
            trusted_broker,
            authorization,
            broker_receipt,
        )?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current =
            load_runtime_control(&transaction, &self.authority, self.trusted_broker.as_ref())?;
        if authorization.revision < current.revision {
            return Err(StoreError::RuntimeControlMismatch);
        }
        if authorization.revision == current.revision {
            if current.enabled == authorization.enabled
                && current.updated_at_ms == authorization.updated_at_ms
            {
                transaction.commit()?;
                return Ok(current);
            }
            return Err(StoreError::RuntimeControlMismatch);
        }
        let revision = i64::try_from(authorization.revision)
            .map_err(|_| StoreError::RuntimeControlRevisionOverflow)?;
        transaction.execute(
            "INSERT INTO runtime_control_history
                (revision, enabled, updated_at_ms, signature_hex)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                revision,
                i64::from(authorization.enabled),
                authorization.updated_at_ms,
                authorization.authorization_signature_hex,
            ],
        )?;
        transaction.execute(
            "INSERT INTO runtime_control_recovery_checkpoint
                (revision, authorization_hash, checkpoint_nonce, request_nonce,
                 broker_key_id, broker_signature_hex)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                revision,
                broker_receipt.authorization_hash,
                broker_receipt.checkpoint_nonce,
                broker_receipt.request_nonce,
                broker_receipt.broker_key_id,
                broker_receipt.broker_signature_hex,
            ],
        )?;
        transaction.execute(
            "INSERT INTO runtime_control
                (singleton_id, enabled, revision, updated_at_ms, signature_hex)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(singleton_id) DO UPDATE SET
                enabled = excluded.enabled,
                revision = excluded.revision,
                updated_at_ms = excluded.updated_at_ms,
                signature_hex = excluded.signature_hex",
            params![
                RUNTIME_CONTROL_ID,
                i64::from(authorization.enabled),
                revision,
                authorization.updated_at_ms,
                authorization.authorization_signature_hex,
            ],
        )?;
        transaction.commit()?;
        Ok(RuntimeControl {
            enabled: authorization.enabled,
            revision: authorization.revision,
            updated_at_ms: authorization.updated_at_ms,
        })
    }

    /// Fails closed unless the signed Store-owned global switch is On.
    ///
    /// # Errors
    ///
    /// Returns `RuntimeDisabled` for the default or explicit Off state and a
    /// mismatch error for tampered storage.
    pub fn require_runtime_enabled(&self) -> Result<(), StoreError> {
        require_runtime_enabled(
            &self.connection,
            &self.authority,
            self.trusted_broker.as_ref(),
        )
    }

    /// Requires an exact live protected-broker checkpoint for an enabled
    /// runtime before a model-bearing operation can begin.
    ///
    /// # Errors
    ///
    /// Returns an error for stale/replayed proof, local rollback, Off, or an
    /// untrusted broker signature.
    pub fn require_runtime_checkpoint(
        &self,
        authorization: &RuntimeControlAuthorization,
        broker_receipt: &RuntimeControlReceipt,
    ) -> Result<(), StoreError> {
        self.authority
            .verify_runtime_control(authorization)
            .map_err(|_| StoreError::RuntimeControlMismatch)?;
        let trusted_broker = self
            .trusted_broker
            .as_ref()
            .ok_or(StoreError::MissingTrustedBrokerEnrollment)?;
        crate::effect::verify_runtime_control_receipt(
            trusted_broker,
            authorization,
            broker_receipt,
        )?;
        let current =
            load_runtime_control(&self.connection, &self.authority, Some(trusted_broker))?;
        if !authorization.enabled
            || current.enabled != authorization.enabled
            || current.revision != authorization.revision
            || current.updated_at_ms != authorization.updated_at_ms
        {
            return Err(if authorization.enabled {
                StoreError::RuntimeControlMismatch
            } else {
                StoreError::RuntimeDisabled
            });
        }
        Ok(())
    }

    /// Loads, applies, validates, and persists one typed Mission command and its
    /// bound audit row in the same transaction. No raw Mission replacement API exists.
    ///
    /// Exact retries at the caller's current audit anchor return the original
    /// encrypted result. Reusing a command id with a different command fails closed.
    ///
    /// # Errors
    ///
    /// Returns an error without a state or audit write if concurrency, domain,
    /// audit, encryption, idempotency, or database validation fails.
    pub fn execute_mission_command(
        &mut self,
        envelope: &MissionCommandEnvelope,
    ) -> Result<MissionCommandResult, StoreError> {
        self.execute_mission_command_batch(std::slice::from_ref(envelope))?
            .pop()
            .ok_or(StoreError::InvalidCommandBatch)
    }

    /// Applies one ordered batch of typed commands for the same Mission in a
    /// single transaction. The first envelope carries the caller's verified
    /// anchor; later envelopes must omit it because the Store owns chaining to
    /// each preceding command result.
    ///
    /// Exact whole-batch retries return the original bound results. A new
    /// command after an already committed prefix is accepted only when that
    /// prefix is still the current audit tail. Any failure rolls back every
    /// state, Receipt, command-result, and audit write in the batch.
    ///
    /// # Errors
    ///
    /// Returns an error without a write for an empty/mixed/duplicate batch,
    /// caller-supplied inner anchor, concurrency conflict, domain failure,
    /// invalid binding, or database failure.
    pub fn execute_mission_command_batch(
        &mut self,
        envelopes: &[MissionCommandEnvelope],
    ) -> Result<Vec<MissionCommandResult>, StoreError> {
        let Some(first) = envelopes.first() else {
            return Err(StoreError::InvalidCommandBatch);
        };
        let mission_id = first.command.mission_id();
        let mut command_ids = HashSet::new();
        for (index, envelope) in envelopes.iter().enumerate() {
            validate_command_id(&envelope.command_id)?;
            if envelope.command.mission_id() != mission_id
                || !command_ids.insert(envelope.command_id.as_str())
                || (index > 0 && envelope.expected_anchor.is_some())
            {
                return Err(StoreError::InvalidCommandBatch);
            }
        }
        let transaction = self.connection.transaction()?;
        let mut expected_anchor = first.expected_anchor.clone();
        let mut results = Vec::with_capacity(envelopes.len());
        for envelope in envelopes {
            let result = execute_mission_command_in_transaction(
                &transaction,
                &self.authority,
                self.trusted_broker.as_ref(),
                envelope,
                expected_anchor.as_ref(),
            )?;
            expected_anchor = Some(result.anchor.clone());
            results.push(result);
        }
        transaction.commit()?;
        Ok(results)
    }

    /// Loads a Mission only after its ciphertext matches its latest signed audit row.
    ///
    /// # Errors
    ///
    /// Returns an error for query, state-binding, ciphertext, or domain failure.
    pub fn get_mission(
        &self,
        mission_id: &str,
        expected_anchor: &AuditAnchor,
    ) -> Result<Option<Mission>, StoreError> {
        self.verify_audit_chain(expected_anchor)?;
        load_mission_for_update(&self.connection, &self.authority, mission_id)
    }

    /// Persists and signs one exact Mission-file effect only after loading the
    /// current command-reachable Mission and verifying the complete audit tail.
    /// No caller-assembled Mission can enter this production issuance route.
    ///
    /// Exact retries reuse the original stable authorization. An unresolved
    /// authorization owns the global audit fence until a broker proves either
    /// durable commit or definitive noncommit. A committed authorization can
    /// only be reattested read-only. Reusing an effect id for changed target or
    /// payload fails closed.
    ///
    /// # Errors
    ///
    /// Returns an error for stale state, missing approval, malformed target or
    /// broker session, conflicting id reuse, or signed-ledger mismatch.
    pub fn prepare_mission_file_put(
        &mut self,
        effect_id: &str,
        expected_anchor: &AuditAnchor,
        proposal: &ActionProposal,
        payload: &[u8],
        broker_session: &EffectBrokerSession,
    ) -> Result<EffectPermit, StoreError> {
        validate_effect_id(effect_id)?;
        let trusted_broker = self
            .trusted_broker
            .clone()
            .ok_or(StoreError::MissingTrustedBrokerEnrollment)?;
        let now_ms = current_unix_ms()?;
        crate::effect::validate_broker_session(&trusted_broker, broker_session, now_ms)?;
        let request = mission_file_put_request(proposal, payload)?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let runtime = load_runtime_control(&transaction, &self.authority, Some(&trusted_broker))?;
        let (command, authorization_anchor, purpose) = resolve_effect_command(
            &EffectResolution {
                transaction: &transaction,
                authority: &self.authority,
                effect_id,
                expected_anchor,
                proposal,
                payload,
                trusted_broker: &trusted_broker,
            },
            &request,
        )?;
        let permit = crate::effect::issue_effect_permit(
            &self.authority,
            &trusted_broker,
            command,
            effect_anchor(&authorization_anchor),
            purpose,
            crate::effect::RuntimePermitContext {
                revision: runtime.revision,
                now_ms,
            },
            broker_session,
        )?;
        transaction.commit()?;
        Ok(permit)
    }

    /// Issues a reconciliation-only permit for the one unresolved global
    /// effect fence. The permit authorizes no new external write; it lets the
    /// broker either prove an already durable commit or persist a terminal
    /// noncommit tombstone.
    ///
    /// # Errors
    ///
    /// Returns an error for a missing or mismatched fence, stale anchor,
    /// invalid broker session, corrupt authorization, or signing failure.
    pub fn prepare_effect_reconciliation(
        &mut self,
        effect_id: &str,
        expected_anchor: &AuditAnchor,
        broker_session: &EffectBrokerSession,
    ) -> Result<EffectPermit, StoreError> {
        validate_effect_id(effect_id)?;
        let trusted_broker = self
            .trusted_broker
            .clone()
            .ok_or(StoreError::MissingTrustedBrokerEnrollment)?;
        let now_ms = current_unix_ms()?;
        crate::effect::validate_broker_session(&trusted_broker, broker_session, now_ms)?;
        let transaction = self.connection.transaction()?;
        let runtime = load_runtime_control(&transaction, &self.authority, Some(&trusted_broker))?;
        verify_expected_anchor(
            &transaction,
            &self.authority,
            Some(&trusted_broker),
            Some(expected_anchor),
        )?;
        require_effect_fence(&transaction, effect_id)?;
        let authorization = load_stored_effect_authorization(&transaction, effect_id)?
            .ok_or_else(|| StoreError::EffectAuthorizationMismatch(effect_id.to_owned()))?;
        let command =
            verify_stored_effect_authorization(&transaction, &self.authority, &authorization)?;
        let authorization_anchor = effect_authorization_anchor(
            &transaction,
            effect_id,
            &authorization.stable_effect_hash,
            &authorization.command_blob_hash,
        )?;
        let permit = crate::effect::issue_effect_permit(
            &self.authority,
            &trusted_broker,
            command,
            effect_anchor(&authorization_anchor),
            EffectPermitPurpose::Reconcile,
            crate::effect::RuntimePermitContext {
                revision: runtime.revision,
                now_ms,
            },
            broker_session,
        )?;
        transaction.commit()?;
        Ok(permit)
    }

    /// Verifies and atomically persists a broker-signed effect Receipt with a
    /// bound audit row before it can be used as Mission Evidence.
    ///
    /// # Errors
    ///
    /// Returns an error for an unissued permit, changed or invalid broker
    /// result, stale current audit anchor, conflicting retry, or storage
    /// mismatch. The receipt audit row and fence deletion are committed in the
    /// same Store transaction, so no Mission audit can linearize between effect
    /// authorization and its durable outcome.
    pub fn record_effect_receipt(
        &mut self,
        expected_anchor: &AuditAnchor,
        broker_session: &EffectBrokerSession,
        permit: &EffectPermit,
        receipt: &EffectReceipt,
    ) -> Result<AuditAnchor, StoreError> {
        let trusted_broker = self
            .trusted_broker
            .clone()
            .ok_or(StoreError::MissingTrustedBrokerEnrollment)?;
        let payload = StoredEffectReceiptPayload {
            broker_session: broker_session.clone(),
            permit: permit.clone(),
            receipt: receipt.clone(),
        };
        let transaction = self.connection.transaction()?;
        if load_stored_effect_noncommit(&transaction, &receipt.effect_id)?.is_some() {
            return Err(StoreError::EffectReceiptConflict);
        }
        if let Some(stored) = load_stored_effect_receipt(&transaction, &receipt.effect_id)? {
            verified_audit_tail(&transaction, &self.authority)?
                .ok_or(StoreError::EmptyAuditLedger)?;
            verify_all_bindings(&transaction, &self.authority, Some(&trusted_broker))?;
            let original = verify_stored_effect_receipt(
                &transaction,
                &self.authority,
                &trusted_broker,
                &stored,
            )?;
            let authorization =
                load_stored_effect_authorization(&transaction, &permit.command.effect_id)?
                    .ok_or_else(|| {
                        StoreError::EffectAuthorizationMismatch(permit.command.effect_id.clone())
                    })?;
            let command =
                verify_stored_effect_authorization(&transaction, &self.authority, &authorization)?;
            let authorization_anchor = effect_authorization_anchor(
                &transaction,
                &authorization.effect_id,
                &authorization.stable_effect_hash,
                &authorization.command_blob_hash,
            )?;
            self.authority
                .verify_effect_permit(permit)
                .map_err(|_| StoreError::EffectReceiptConflict)?;
            crate::verify_effect_receipt(&trusted_broker, broker_session, permit, receipt)
                .map_err(|_| StoreError::EffectReceiptConflict)?;
            if command != permit.command
                || authorization.stable_effect_hash != permit.stable_effect_hash
                || effect_anchor(&authorization_anchor) != permit.authorization_anchor
                || !same_immutable_effect_outcome(&original.receipt, receipt)
            {
                return Err(StoreError::EffectReceiptConflict);
            }
            transaction.commit()?;
            return Ok(stored.anchor);
        }
        verify_expected_anchor(
            &transaction,
            &self.authority,
            Some(&trusted_broker),
            Some(expected_anchor),
        )?;
        require_effect_fence(&transaction, &receipt.effect_id)?;
        let authorization =
            load_stored_effect_authorization(&transaction, &permit.command.effect_id)?.ok_or_else(
                || StoreError::EffectAuthorizationMismatch(permit.command.effect_id.clone()),
            )?;
        let command =
            verify_stored_effect_authorization(&transaction, &self.authority, &authorization)?;
        let authorization_anchor = effect_authorization_anchor(
            &transaction,
            &authorization.effect_id,
            &authorization.stable_effect_hash,
            &authorization.command_blob_hash,
        )?;
        self.authority.verify_effect_permit(permit)?;
        crate::verify_effect_receipt(&trusted_broker, broker_session, permit, receipt)?;
        if command != permit.command
            || authorization.stable_effect_hash != permit.stable_effect_hash
            || effect_anchor(&authorization_anchor) != permit.authorization_anchor
        {
            return Err(StoreError::EffectReceiptConflict);
        }
        let mission =
            load_mission_for_update(&transaction, &self.authority, &permit.command.mission_id)?
                .ok_or(StoreError::MissionNotFound)?;
        let anchor =
            write_effect_receipt(&transaction, &self.authority, &mission.owner_id, &payload)?;
        clear_effect_fence(&transaction, &receipt.effect_id)?;
        transaction.commit()?;
        Ok(anchor)
    }

    /// Verifies and atomically persists a broker-signed definitive noncommit
    /// attestation. The terminal record, its audit row, and fence deletion are
    /// one transaction; an old Execute permit can never be made current again.
    ///
    /// # Errors
    ///
    /// Returns an error for a stale anchor, invalid signature, changed effect,
    /// conflicting immutable outcome, missing fence, or atomic storage failure.
    pub fn record_effect_noncommit(
        &mut self,
        expected_anchor: &AuditAnchor,
        broker_session: &EffectBrokerSession,
        permit: &EffectPermit,
        attestation: &EffectNonCommit,
    ) -> Result<AuditAnchor, StoreError> {
        let trusted_broker = self
            .trusted_broker
            .clone()
            .ok_or(StoreError::MissingTrustedBrokerEnrollment)?;
        let payload = StoredEffectNonCommitPayload {
            broker_session: broker_session.clone(),
            permit: permit.clone(),
            attestation: attestation.clone(),
        };
        let transaction = self.connection.transaction()?;
        if load_stored_effect_receipt(&transaction, &attestation.effect_id)?.is_some() {
            return Err(StoreError::EffectNonCommitConflict);
        }
        if let Some(stored) = load_stored_effect_noncommit(&transaction, &attestation.effect_id)? {
            verified_audit_tail(&transaction, &self.authority)?
                .ok_or(StoreError::EmptyAuditLedger)?;
            verify_all_bindings(&transaction, &self.authority, Some(&trusted_broker))?;
            let original = verify_stored_effect_noncommit(
                &transaction,
                &self.authority,
                &trusted_broker,
                &stored,
            )?;
            self.authority
                .verify_effect_permit(permit)
                .map_err(|_| StoreError::EffectNonCommitConflict)?;
            crate::verify_effect_noncommit(&trusted_broker, broker_session, permit, attestation)
                .map_err(|_| StoreError::EffectNonCommitConflict)?;
            if original.attestation != *attestation {
                return Err(StoreError::EffectNonCommitConflict);
            }
            transaction.commit()?;
            return Ok(stored.anchor);
        }
        verify_expected_anchor(
            &transaction,
            &self.authority,
            Some(&trusted_broker),
            Some(expected_anchor),
        )?;
        require_effect_fence(&transaction, &attestation.effect_id)?;
        let authorization =
            load_stored_effect_authorization(&transaction, &permit.command.effect_id)?.ok_or_else(
                || StoreError::EffectAuthorizationMismatch(permit.command.effect_id.clone()),
            )?;
        let command =
            verify_stored_effect_authorization(&transaction, &self.authority, &authorization)?;
        let authorization_anchor = effect_authorization_anchor(
            &transaction,
            &authorization.effect_id,
            &authorization.stable_effect_hash,
            &authorization.command_blob_hash,
        )?;
        self.authority.verify_effect_permit(permit)?;
        crate::verify_effect_noncommit(&trusted_broker, broker_session, permit, attestation)?;
        if command != permit.command
            || authorization.stable_effect_hash != permit.stable_effect_hash
            || effect_anchor(&authorization_anchor) != permit.authorization_anchor
        {
            return Err(StoreError::EffectNonCommitConflict);
        }
        let mission =
            load_mission_for_update(&transaction, &self.authority, &permit.command.mission_id)?
                .ok_or(StoreError::MissionNotFound)?;
        let anchor =
            write_effect_noncommit(&transaction, &self.authority, &mission.owner_id, &payload)?;
        clear_effect_fence(&transaction, &attestation.effect_id)?;
        transaction.commit()?;
        Ok(anchor)
    }

    /// Loads an immutable Receipt after state/audit binding and Mission validation.
    ///
    /// # Errors
    ///
    /// Returns an error for query, state-binding, ciphertext, or domain failure.
    pub fn get_receipt(
        &self,
        receipt_id: &str,
        expected_anchor: &AuditAnchor,
    ) -> Result<Option<Receipt>, StoreError> {
        self.verify_audit_chain(expected_anchor)?;
        let row: Option<(String, Vec<u8>)> = self
            .connection
            .query_row(
                "SELECT mission_id, encrypted_blob FROM receipt_state WHERE receipt_id = ?1",
                [receipt_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        row.map(|(mission_id, blob)| {
            verify_blob_binding(
                &self.connection,
                RECEIPT_COMMIT_ACTION,
                receipt_id,
                "receipt",
                &blob,
            )?;
            let receipt: Receipt = self
                .authority
                .decrypt_json(&blob, receipt_aad(receipt_id, &mission_id).as_bytes())?;
            let mission = load_mission_for_update(&self.connection, &self.authority, &mission_id)?
                .ok_or(StoreError::MissionStateMismatch)?;
            validate_receipt(&mission, &receipt, &self.authority)?;
            Ok(receipt)
        })
        .transpose()
    }

    /// Records a channel envelope once with its full authorization provenance.
    ///
    /// # Errors
    ///
    /// Returns `EnvelopeConflict` for changed provenance under the same source id.
    pub fn record_envelope(
        &self,
        envelope: &ChannelEnvelope,
    ) -> Result<EnvelopeInsert, StoreError> {
        let channel = serde_json::to_string(&envelope.channel)
            .map_err(|error| CryptoError::Serialization(error.to_string()))?;
        let existing: Option<(String, String, String)> = self
            .connection
            .query_row(
                "SELECT sender_id, conversation_id, content_sha256 FROM channel_envelope
                 WHERE channel_json = ?1 AND source_message_id = ?2",
                params![channel, envelope.source_message_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        match existing {
            Some((sender, conversation, hash))
                if sender == envelope.sender_id
                    && conversation == envelope.conversation_id
                    && hash == envelope.content_sha256 =>
            {
                Ok(EnvelopeInsert::Duplicate)
            }
            Some(_) => Err(StoreError::EnvelopeConflict),
            None => {
                self.connection.execute(
                    "INSERT INTO channel_envelope
                        (channel_json, source_message_id, sender_id, conversation_id,
                         content_sha256, received_at_ms)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        channel,
                        envelope.source_message_id,
                        envelope.sender_id,
                        envelope.conversation_id,
                        envelope.content_sha256,
                        envelope.received_at_ms,
                    ],
                )?;
                Ok(EnvelopeInsert::Inserted)
            }
        }
    }

    /// Lists all verified Missions newest-first after checking the complete
    /// audit chain and every persisted binding.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid anchor, binding, ciphertext, or Mission.
    pub fn list_missions(&self, expected_anchor: &AuditAnchor) -> Result<Vec<Mission>, StoreError> {
        self.verify_audit_chain(expected_anchor)?;
        let mission_ids = {
            let mut statement = self.connection.prepare(
                "SELECT mission_id FROM mission_state ORDER BY updated_at_ms DESC, mission_id ASC",
            )?;
            statement
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?
        };
        mission_ids
            .into_iter()
            .map(|mission_id| {
                load_mission_for_update(&self.connection, &self.authority, &mission_id)?
                    .ok_or(StoreError::MissionStateMismatch)
            })
            .collect()
    }

    /// Lists immutable verified Receipts newest-first after checking the
    /// complete audit chain and every persisted binding.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid anchor, binding, ciphertext, Receipt,
    /// or completed Mission.
    pub fn list_receipts(&self, expected_anchor: &AuditAnchor) -> Result<Vec<Receipt>, StoreError> {
        self.verify_audit_chain(expected_anchor)?;
        let receipt_ids = {
            let mut statement = self.connection.prepare(
                "SELECT receipt_id FROM receipt_state ORDER BY completed_at_ms DESC, receipt_id ASC",
            )?;
            statement
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?
        };
        receipt_ids
            .into_iter()
            .map(|receipt_id| {
                self.get_receipt(&receipt_id, expected_anchor)?
                    .ok_or(StoreError::MissionStateMismatch)
            })
            .collect()
    }

    /// Returns the current audit tail only after the complete signed chain and
    /// every persisted binding have been verified. A pristine Store has no
    /// anchor yet.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid row, signature, tail, state, command,
    /// or effect binding.
    pub fn current_verified_audit_anchor(&self) -> Result<Option<AuditAnchor>, StoreError> {
        let anchor = verified_audit_tail(&self.connection, &self.authority)?;
        verify_all_bindings(
            &self.connection,
            &self.authority,
            self.trusted_broker.as_ref(),
        )?;
        Ok(anchor)
    }

    /// Verifies the complete signed chain, external tail, current state bindings,
    /// and bidirectional command/audit reconciliation.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid row, signature, tail, state, or command binding.
    pub fn verify_audit_chain(&self, expected: &AuditAnchor) -> Result<(), StoreError> {
        let actual = verified_audit_tail(&self.connection, &self.authority)?
            .ok_or(StoreError::EmptyAuditLedger)?;
        if &actual != expected {
            return Err(StoreError::AuditAnchorMismatch);
        }
        verify_all_bindings(
            &self.connection,
            &self.authority,
            self.trusted_broker.as_ref(),
        )?;
        Ok(())
    }
}

fn execute_mission_command_in_transaction(
    transaction: &Transaction<'_>,
    authority: &LocalAuthority,
    trusted_broker: Option<&TrustedBrokerEnrollment>,
    envelope: &MissionCommandEnvelope,
    expected_anchor: Option<&AuditAnchor>,
) -> Result<MissionCommandResult, StoreError> {
    let command_hash = command_hash(envelope)?;
    if let Some(result) = load_duplicate_result(
        transaction,
        authority,
        trusted_broker,
        &envelope.command_id,
        &command_hash,
    )? {
        return Ok(result);
    }
    require_no_effect_fence(transaction)?;
    verify_expected_anchor(transaction, authority, trusted_broker, expected_anchor)?;
    let mission_id = envelope.command.mission_id();
    let current = load_mission_for_update(transaction, authority, mission_id)?;
    let applied = apply_mission_command(current, &envelope.command, authority)?;
    let actor = applied.mission.owner_id.clone();
    let audit = CommandAuditContext {
        command_id: &envelope.command_id,
        command_hash: &command_hash,
        actor: &actor,
    };
    let mission_anchor = write_mission(
        transaction,
        authority,
        &applied.mission,
        &audit,
        &format!("{}:mission", envelope.command_id),
    )?;
    let anchor = if let Some(receipt) = applied.receipt.as_ref() {
        write_receipt(
            transaction,
            authority,
            &applied.mission,
            receipt,
            &audit,
            &format!("{}:receipt", envelope.command_id),
        )?
    } else {
        mission_anchor
    };
    let result = MissionCommandResult {
        mission: applied.mission,
        receipt: applied.receipt,
        anchor,
    };
    write_command_result(
        transaction,
        authority,
        &envelope.command_id,
        &command_hash,
        &result,
    )?;
    Ok(result)
}

fn validate_command_id(command_id: &str) -> Result<(), StoreError> {
    if command_id.trim().is_empty() {
        Err(StoreError::InvalidCommandId)
    } else {
        Ok(())
    }
}

fn command_hash(envelope: &MissionCommandEnvelope) -> Result<String, StoreError> {
    let bytes = serde_json::to_vec(&serde_json::json!({
        "command": envelope.command,
        "version": 1,
    }))
    .map_err(|error| CryptoError::Serialization(error.to_string()))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn validate_effect_id(effect_id: &str) -> Result<(), StoreError> {
    if !is_canonical_effect_identifier(effect_id) {
        return Err(StoreError::InvalidEffectId);
    }
    Ok(())
}

fn current_unix_ms() -> Result<i64, StoreError> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| StoreError::InvalidSystemTime)?;
    i64::try_from(duration.as_millis()).map_err(|_| StoreError::InvalidSystemTime)
}

fn effect_anchor(anchor: &AuditAnchor) -> EffectAuditAnchor {
    EffectAuditAnchor {
        sequence: anchor.sequence,
        entry_hash: anchor.entry_hash.clone(),
        signature_hex: anchor.signature_hex.clone(),
    }
}

fn mission_file_put_request(
    proposal: &ActionProposal,
    payload: &[u8],
) -> Result<MissionFilePutRequest, StoreError> {
    if proposal.effect != EffectKind::FileWrite {
        return Err(StoreError::EffectAuthorization(GateDecision::Denied(
            "effect is not a Mission file write",
        )));
    }
    let ActionTarget::MissionFile { relative_path } = &proposal.target else {
        return Err(StoreError::EffectAuthorization(GateDecision::Denied(
            "privileged broker accepts only Mission-relative file targets",
        )));
    };
    let path_components = crate::gate::mission_file_path_components(relative_path).ok_or(
        StoreError::EffectAuthorization(GateDecision::Denied(
            "file target escapes Mission workspace",
        )),
    )?;
    let payload_byte_len = u64::try_from(payload.len()).map_err(|_| StoreError::InvalidEffectId)?;
    if payload_byte_len > MAX_EFFECT_PAYLOAD_BYTES {
        return Err(StoreError::EffectPayloadTooLarge);
    }
    let action_digest = proposal
        .approval_digest(ApprovalKind::NewExternalWrite, Some(payload))
        .map_err(|error| CryptoError::Serialization(error.to_string()))?;
    Ok(MissionFilePutRequest {
        path_components,
        payload_sha256: format!("{:x}", Sha256::digest(payload)),
        payload_byte_len,
        action_digest,
    })
}

fn resolve_effect_command(
    context: &EffectResolution<'_, '_>,
    request: &MissionFilePutRequest,
) -> Result<(EffectCommand, AuditAnchor, EffectPermitPurpose), StoreError> {
    if let Some(stored) = load_stored_effect_authorization(context.transaction, context.effect_id)?
    {
        return resolve_existing_effect_command(context, request, &stored);
    }
    require_runtime_enabled(
        context.transaction,
        context.authority,
        Some(context.trusted_broker),
    )?;
    require_no_effect_fence(context.transaction)?;
    verify_expected_anchor(
        context.transaction,
        context.authority,
        Some(context.trusted_broker),
        Some(context.expected_anchor),
    )?;
    let mission = load_mission_for_update(
        context.transaction,
        context.authority,
        &context.proposal.mission_id,
    )?
    .ok_or(StoreError::MissionNotFound)?;
    let decision = ActionGate.authorize(&mission, context.proposal, Some(context.payload));
    if decision != GateDecision::Allowed {
        return Err(StoreError::EffectAuthorization(decision));
    }
    let mut approval_ids = effect_cost_approval_ids(&mission, context.proposal, context.payload)?;
    approval_ids.push(
        crate::gate::approved_owner_approval_id(
            &mission,
            ApprovalKind::NewExternalWrite,
            &request.action_digest,
        )
        .ok_or(StoreError::EffectAuthorization(GateDecision::NeedsMe(
            ApprovalKind::NewExternalWrite,
        )))?,
    );
    let command = EffectCommand {
        protocol_version: EFFECT_PROTOCOL_VERSION,
        effect_id: context.effect_id.to_owned(),
        mission_id: mission.id.clone(),
        mission_updated_at_ms: mission.updated_at_ms,
        mission_scope_digest: mission.scope_digest.clone(),
        source_anchor: effect_anchor(context.expected_anchor),
        approval_ids,
        effect: MissionFileEffect::PutFile {
            path_components: request.path_components.clone(),
            payload: PayloadDescriptor {
                sha256: request.payload_sha256.clone(),
                byte_len: request.payload_byte_len,
            },
            action_digest: request.action_digest.clone(),
        },
    };
    let authorization_anchor = write_effect_authorization(
        context.transaction,
        context.authority,
        &mission.owner_id,
        &command,
    )?;
    context.transaction.execute(
        "INSERT INTO effect_fence (effect_id, mission_id, stable_effect_hash)
         VALUES (?1, ?2, ?3)",
        params![
            command.effect_id,
            command.mission_id,
            crate::effect::stable_effect_hash(&command)?
        ],
    )?;
    Ok((command, authorization_anchor, EffectPermitPurpose::Execute))
}

fn resolve_existing_effect_command(
    context: &EffectResolution<'_, '_>,
    request: &MissionFilePutRequest,
    stored: &StoredEffectAuthorization,
) -> Result<(EffectCommand, AuditAnchor, EffectPermitPurpose), StoreError> {
    let current_tail = verified_audit_tail(context.transaction, context.authority)?
        .ok_or(StoreError::EmptyAuditLedger)?;
    verify_all_bindings(
        context.transaction,
        context.authority,
        Some(context.trusted_broker),
    )?;
    let command =
        verify_stored_effect_authorization(context.transaction, context.authority, stored)?;
    if !effect_request_matches(&command, context.proposal, request) {
        return Err(StoreError::EffectConflict);
    }
    let authorization_anchor = effect_authorization_anchor(
        context.transaction,
        &command.effect_id,
        &stored.stable_effect_hash,
        &stored.command_blob_hash,
    )?;
    if load_stored_effect_noncommit(context.transaction, context.effect_id)?.is_some() {
        return Err(StoreError::EffectNotCommitted);
    }
    if load_stored_effect_receipt(context.transaction, context.effect_id)?.is_some() {
        if current_tail != *context.expected_anchor {
            return Err(StoreError::AuditAnchorMismatch);
        }
        return Ok((
            command,
            authorization_anchor,
            EffectPermitPurpose::ReattestOnly,
        ));
    }
    require_runtime_enabled(
        context.transaction,
        context.authority,
        Some(context.trusted_broker),
    )?;
    require_effect_fence(context.transaction, context.effect_id)?;
    if current_tail != authorization_anchor || current_tail != *context.expected_anchor {
        return Err(StoreError::AuditAnchorMismatch);
    }
    let mission =
        load_mission_for_update(context.transaction, context.authority, &command.mission_id)?
            .ok_or(StoreError::MissionNotFound)?;
    let decision = ActionGate.authorize(&mission, context.proposal, Some(context.payload));
    if decision != GateDecision::Allowed {
        return Err(StoreError::EffectAuthorization(decision));
    }
    Ok((command, authorization_anchor, EffectPermitPurpose::Execute))
}

fn effect_cost_approval_ids(
    mission: &Mission,
    proposal: &ActionProposal,
    payload: &[u8],
) -> Result<Vec<String>, StoreError> {
    if proposal.estimated_cost_micros.unwrap_or_default() == 0 {
        return Ok(Vec::new());
    }
    let digest = proposal
        .approval_digest(ApprovalKind::Cost, Some(payload))
        .map_err(|error| CryptoError::Serialization(error.to_string()))?;
    let approval_id = crate::gate::approved_owner_approval_id(mission, ApprovalKind::Cost, &digest)
        .ok_or(StoreError::EffectAuthorization(GateDecision::NeedsMe(
            ApprovalKind::Cost,
        )))?;
    Ok(vec![approval_id])
}

fn effect_request_matches(
    command: &EffectCommand,
    proposal: &ActionProposal,
    request: &MissionFilePutRequest,
) -> bool {
    let MissionFileEffect::PutFile {
        path_components: stored_path,
        payload,
        action_digest: stored_digest,
    } = &command.effect;
    command.protocol_version == EFFECT_PROTOCOL_VERSION
        && command.mission_id == proposal.mission_id
        && command.mission_scope_digest == proposal.mission_scope_digest
        && stored_path == &request.path_components
        && payload.sha256 == request.payload_sha256
        && payload.byte_len == request.payload_byte_len
        && stored_digest == &request.action_digest
}

fn load_duplicate_result(
    transaction: &Transaction<'_>,
    authority: &LocalAuthority,
    trusted_broker: Option<&TrustedBrokerEnrollment>,
    command_id: &str,
    expected_command_hash: &str,
) -> Result<Option<MissionCommandResult>, StoreError> {
    let Some(stored) = load_stored_command_result(transaction, command_id)? else {
        return Ok(None);
    };
    verified_audit_tail(transaction, authority)?.ok_or(StoreError::EmptyAuditLedger)?;
    verify_all_bindings(transaction, authority, trusted_broker)?;
    if stored.command_hash != expected_command_hash {
        return Err(StoreError::CommandConflict);
    }
    verify_stored_command_result(transaction, authority, &stored).map(Some)
}

fn load_mission_for_update(
    connection: &Connection,
    authority: &LocalAuthority,
    mission_id: &str,
) -> Result<Option<Mission>, StoreError> {
    if !crate::mission::is_canonical_mission_id(mission_id) {
        return Err(MissionError::InvalidMissionId.into());
    }
    let row: Option<(String, Vec<u8>)> = connection
        .query_row(
            "SELECT status_json, encrypted_blob FROM mission_state WHERE mission_id = ?1",
            [mission_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    row.map(|(status, blob)| {
        verify_blob_binding(
            connection,
            MISSION_COMMAND_ACTION,
            mission_id,
            &format!("mission:{status}"),
            &blob,
        )?;
        let mission: Mission = authority.decrypt_json(&blob, mission_aad(mission_id).as_bytes())?;
        validate_mission_snapshot(&mission, authority)?;
        Ok(mission)
    })
    .transpose()
}

fn write_mission(
    transaction: &Transaction<'_>,
    authority: &LocalAuthority,
    mission: &Mission,
    audit: &CommandAuditContext<'_>,
    audit_id: &str,
) -> Result<AuditAnchor, StoreError> {
    let encrypted = authority.encrypt_json(mission, mission_aad(&mission.id).as_bytes())?;
    let status = serde_json::to_string(&mission.status)
        .map_err(|error| CryptoError::Serialization(error.to_string()))?;
    transaction.execute(
        "INSERT INTO mission_state
            (mission_id, status_json, scope_digest, encrypted_blob, created_at_ms, updated_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(mission_id) DO UPDATE SET
            status_json = excluded.status_json,
            scope_digest = excluded.scope_digest,
            encrypted_blob = excluded.encrypted_blob,
            updated_at_ms = excluded.updated_at_ms",
        params![
            mission.id,
            status,
            mission.scope_digest,
            encrypted,
            mission.created_at_ms,
            mission.updated_at_ms,
        ],
    )?;
    append_audit(
        transaction,
        authority,
        &AuditRecord {
            id: audit_id,
            command_id: audit.command_id,
            command_hash: audit.command_hash,
            actor: audit.actor,
            action: MISSION_COMMAND_ACTION,
            entity_id: &mission.id,
            created_at_ms: mission.updated_at_ms,
            state_kind: &format!("mission:{status}"),
            state_hash: &blob_hash(&encrypted),
        },
    )
}

fn write_receipt(
    transaction: &Transaction<'_>,
    authority: &LocalAuthority,
    mission: &Mission,
    receipt: &Receipt,
    audit: &CommandAuditContext<'_>,
    audit_id: &str,
) -> Result<AuditAnchor, StoreError> {
    validate_receipt(mission, receipt, authority)?;
    if transaction
        .query_row(
            "SELECT 1 FROM receipt_state WHERE receipt_id = ?1",
            [&receipt.id],
            |_| Ok(()),
        )
        .optional()?
        .is_some()
    {
        return Err(StoreError::ReceiptAlreadyExists);
    }
    let encrypted = authority.encrypt_json(
        receipt,
        receipt_aad(&receipt.id, &receipt.mission_id).as_bytes(),
    )?;
    transaction.execute(
        "INSERT INTO receipt_state
            (receipt_id, mission_id, encrypted_blob, completed_at_ms)
         VALUES (?1, ?2, ?3, ?4)",
        params![
            receipt.id,
            receipt.mission_id,
            encrypted,
            receipt.completed_at_ms,
        ],
    )?;
    append_audit(
        transaction,
        authority,
        &AuditRecord {
            id: audit_id,
            command_id: audit.command_id,
            command_hash: audit.command_hash,
            actor: audit.actor,
            action: RECEIPT_COMMIT_ACTION,
            entity_id: &receipt.id,
            created_at_ms: receipt.completed_at_ms,
            state_kind: "receipt",
            state_hash: &blob_hash(&encrypted),
        },
    )
}

fn write_command_result(
    transaction: &Transaction<'_>,
    authority: &LocalAuthority,
    command_id: &str,
    command_hash: &str,
    result: &MissionCommandResult,
) -> Result<(), StoreError> {
    let encrypted = authority.encrypt_json(result, command_result_aad(command_id).as_bytes())?;
    let result_hash = blob_hash(&encrypted);
    let record_signature_hex = authority.sign_bytes(&command_result_record_bytes(
        command_id,
        &result.mission.id,
        command_hash,
        &result_hash,
        &result.anchor,
    ));
    transaction.execute(
        "INSERT INTO mission_command_result
            (command_id, mission_id, command_hash, encrypted_result, result_hash,
             anchor_sequence, anchor_entry_hash, anchor_signature_hex, record_signature_hex)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            command_id,
            result.mission.id,
            command_hash,
            encrypted,
            result_hash,
            result.anchor.sequence,
            result.anchor.entry_hash,
            result.anchor.signature_hex,
            record_signature_hex,
        ],
    )?;
    Ok(())
}

fn write_effect_authorization(
    transaction: &Transaction<'_>,
    authority: &LocalAuthority,
    actor: &str,
    command: &EffectCommand,
) -> Result<AuditAnchor, StoreError> {
    let stable_effect_hash = crate::effect::stable_effect_hash(command)?;
    let source_anchor = audit_anchor(&command.source_anchor);
    let encrypted_command = authority.encrypt_json(
        command,
        effect_authorization_aad(&command.effect_id).as_bytes(),
    )?;
    let command_blob_hash = blob_hash(&encrypted_command);
    let record_signature_hex = authority.sign_effect_bytes(&effect_authorization_record_bytes(
        &command.effect_id,
        &command.mission_id,
        &stable_effect_hash,
        &command_blob_hash,
        &source_anchor,
    ));
    transaction.execute(
        "INSERT INTO effect_authorization
            (effect_id, mission_id, stable_effect_hash, encrypted_command,
             command_blob_hash, source_anchor_sequence, source_anchor_entry_hash,
             source_anchor_signature_hex, record_signature_hex)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            command.effect_id,
            command.mission_id,
            stable_effect_hash,
            encrypted_command,
            command_blob_hash,
            source_anchor.sequence,
            source_anchor.entry_hash,
            source_anchor.signature_hex,
            record_signature_hex,
        ],
    )?;
    append_audit(
        transaction,
        authority,
        &AuditRecord {
            id: &format!("effect:{}:authorization", command.effect_id),
            command_id: &command.effect_id,
            command_hash: &stable_effect_hash,
            actor,
            action: EFFECT_AUTHORIZATION_ACTION,
            entity_id: &command.effect_id,
            created_at_ms: command.mission_updated_at_ms,
            state_kind: "effectAuthorization",
            state_hash: &command_blob_hash,
        },
    )
}

fn effect_authorization_anchor(
    connection: &Connection,
    effect_id: &str,
    stable_effect_hash: &str,
    command_blob_hash: &str,
) -> Result<AuditAnchor, StoreError> {
    connection
        .query_row(
            "SELECT sequence, entry_hash, signature_hex FROM audit_ledger
             WHERE action = ?1 AND entity_id = ?2 AND command_id = ?2
               AND command_hash = ?3 AND state_kind = 'effectAuthorization'
               AND state_hash = ?4",
            params![
                EFFECT_AUTHORIZATION_ACTION,
                effect_id,
                stable_effect_hash,
                command_blob_hash,
            ],
            |row| {
                Ok(AuditAnchor {
                    sequence: row.get(0)?,
                    entry_hash: row.get(1)?,
                    signature_hex: row.get(2)?,
                })
            },
        )
        .optional()?
        .ok_or_else(|| StoreError::EffectAuthorizationMismatch(effect_id.to_owned()))
}

fn load_stored_effect_authorization(
    connection: &Connection,
    effect_id: &str,
) -> Result<Option<StoredEffectAuthorization>, StoreError> {
    connection
        .query_row(
            "SELECT effect_id, mission_id, stable_effect_hash, encrypted_command,
                    command_blob_hash, source_anchor_sequence, source_anchor_entry_hash,
                    source_anchor_signature_hex, record_signature_hex
             FROM effect_authorization WHERE effect_id = ?1",
            [effect_id],
            |row| {
                Ok(StoredEffectAuthorization {
                    effect_id: row.get(0)?,
                    mission_id: row.get(1)?,
                    stable_effect_hash: row.get(2)?,
                    encrypted_command: row.get(3)?,
                    command_blob_hash: row.get(4)?,
                    source_anchor: AuditAnchor {
                        sequence: row.get(5)?,
                        entry_hash: row.get(6)?,
                        signature_hex: row.get(7)?,
                    },
                    record_signature_hex: row.get(8)?,
                })
            },
        )
        .optional()
        .map_err(StoreError::from)
}

fn verify_stored_effect_authorization(
    connection: &Connection,
    authority: &LocalAuthority,
    stored: &StoredEffectAuthorization,
) -> Result<EffectCommand, StoreError> {
    let mismatch = || StoreError::EffectAuthorizationMismatch(stored.effect_id.clone());
    authority
        .verify_effect_bytes(
            &effect_authorization_record_bytes(
                &stored.effect_id,
                &stored.mission_id,
                &stored.stable_effect_hash,
                &stored.command_blob_hash,
                &stored.source_anchor,
            ),
            &stored.record_signature_hex,
        )
        .map_err(|_| mismatch())?;
    if blob_hash(&stored.encrypted_command) != stored.command_blob_hash {
        return Err(mismatch());
    }
    let command: EffectCommand = authority
        .decrypt_json(
            &stored.encrypted_command,
            effect_authorization_aad(&stored.effect_id).as_bytes(),
        )
        .map_err(|_| mismatch())?;
    if command.effect_id != stored.effect_id
        || command.mission_id != stored.mission_id
        || audit_anchor(&command.source_anchor) != stored.source_anchor
        || crate::effect::stable_effect_hash(&command).map_err(|_| mismatch())?
            != stored.stable_effect_hash
        || !effect_command_is_structurally_valid(&command)
    {
        return Err(mismatch());
    }
    let anchor_exists = connection
        .query_row(
            "SELECT 1 FROM audit_ledger
             WHERE sequence = ?1 AND entry_hash = ?2 AND signature_hex = ?3",
            params![
                stored.source_anchor.sequence,
                stored.source_anchor.entry_hash,
                stored.source_anchor.signature_hex
            ],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if !anchor_exists {
        return Err(mismatch());
    }
    Ok(command)
}

fn effect_command_is_structurally_valid(command: &EffectCommand) -> bool {
    if command.protocol_version != EFFECT_PROTOCOL_VERSION
        || validate_effect_id(&command.effect_id).is_err()
        || !crate::mission::is_canonical_mission_id(&command.mission_id)
        || command.mission_scope_digest.trim().is_empty()
        || command.mission_scope_digest.is_empty()
        || command.mission_updated_at_ms < 0
        || command.source_anchor.sequence <= 0
        || !is_lower_hex(&command.source_anchor.entry_hash, 64)
        || !is_lower_hex(&command.source_anchor.signature_hex, 128)
        || command.mission_scope_digest.len() > MAX_EFFECT_SCOPE_DIGEST_BYTES
        || command.approval_ids.is_empty()
        || command.approval_ids.len() > MAX_EFFECT_APPROVAL_IDS
        || command
            .approval_ids
            .iter()
            .any(|approval_id| !is_canonical_effect_identifier(approval_id))
        || command
            .approval_ids
            .iter()
            .enumerate()
            .any(|(index, approval_id)| command.approval_ids[..index].contains(approval_id))
    {
        return false;
    }
    let MissionFileEffect::PutFile {
        path_components,
        payload,
        action_digest,
    } = &command.effect;
    !path_components.is_empty()
        && crate::gate::mission_file_path_components(&path_components.join("/"))
            .is_some_and(|validated| validated == *path_components)
        && is_lower_hex(&payload.sha256, 64)
        && is_lower_hex(action_digest, 64)
}

fn write_effect_receipt(
    transaction: &Transaction<'_>,
    authority: &LocalAuthority,
    actor: &str,
    payload: &StoredEffectReceiptPayload,
) -> Result<AuditAnchor, StoreError> {
    let effect_id = &payload.receipt.effect_id;
    let encrypted_record =
        authority.encrypt_json(payload, effect_receipt_aad(effect_id).as_bytes())?;
    let record_hash = blob_hash(&encrypted_record);
    let anchor = append_audit(
        transaction,
        authority,
        &AuditRecord {
            id: &format!("effect:{effect_id}:receipt"),
            command_id: effect_id,
            command_hash: &payload.receipt.stable_effect_hash,
            actor,
            action: EFFECT_RECEIPT_ACTION,
            entity_id: effect_id,
            created_at_ms: payload.receipt.committed_at_ms,
            state_kind: "effectReceipt",
            state_hash: &record_hash,
        },
    )?;
    let local_signature_hex = authority.sign_effect_bytes(&effect_receipt_record_bytes(
        effect_id,
        &payload.receipt.mission_id,
        &payload.receipt.stable_effect_hash,
        &record_hash,
        &anchor,
    ));
    transaction.execute(
        "INSERT INTO effect_receipt
            (effect_id, mission_id, stable_effect_hash, encrypted_record, record_hash,
             anchor_sequence, anchor_entry_hash, anchor_signature_hex, local_signature_hex)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            effect_id,
            payload.receipt.mission_id,
            payload.receipt.stable_effect_hash,
            encrypted_record,
            record_hash,
            anchor.sequence,
            anchor.entry_hash,
            anchor.signature_hex,
            local_signature_hex,
        ],
    )?;
    Ok(anchor)
}

fn write_effect_noncommit(
    transaction: &Transaction<'_>,
    authority: &LocalAuthority,
    actor: &str,
    payload: &StoredEffectNonCommitPayload,
) -> Result<AuditAnchor, StoreError> {
    let effect_id = &payload.attestation.effect_id;
    let encrypted_record =
        authority.encrypt_json(payload, effect_noncommit_aad(effect_id).as_bytes())?;
    let record_hash = blob_hash(&encrypted_record);
    let anchor = append_audit(
        transaction,
        authority,
        &AuditRecord {
            id: &format!("effect:{effect_id}:noncommit"),
            command_id: effect_id,
            command_hash: &payload.attestation.stable_effect_hash,
            actor,
            action: EFFECT_NONCOMMIT_ACTION,
            entity_id: effect_id,
            created_at_ms: payload.attestation.reconciled_at_ms,
            state_kind: "effectNonCommit",
            state_hash: &record_hash,
        },
    )?;
    let local_signature_hex = authority.sign_effect_bytes(&effect_noncommit_record_bytes(
        effect_id,
        &payload.attestation.mission_id,
        &payload.attestation.stable_effect_hash,
        &record_hash,
        &anchor,
    ));
    transaction.execute(
        "INSERT INTO effect_noncommit
            (effect_id, mission_id, stable_effect_hash, encrypted_record, record_hash,
             anchor_sequence, anchor_entry_hash, anchor_signature_hex, local_signature_hex)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            effect_id,
            payload.attestation.mission_id,
            payload.attestation.stable_effect_hash,
            encrypted_record,
            record_hash,
            anchor.sequence,
            anchor.entry_hash,
            anchor.signature_hex,
            local_signature_hex,
        ],
    )?;
    Ok(anchor)
}

fn same_immutable_effect_outcome(left: &EffectReceipt, right: &EffectReceipt) -> bool {
    left.protocol_version == right.protocol_version
        && left.effect_id == right.effect_id
        && left.stable_effect_hash == right.stable_effect_hash
        && left.mission_id == right.mission_id
        && left.path_components == right.path_components
        && left.payload_sha256 == right.payload_sha256
        && left.payload_byte_len == right.payload_byte_len
        && left.committed_at_ms == right.committed_at_ms
        && left.broker_key_id == right.broker_key_id
}

fn load_stored_effect_receipt(
    connection: &Connection,
    effect_id: &str,
) -> Result<Option<StoredEffectReceipt>, StoreError> {
    connection
        .query_row(
            "SELECT effect_id, mission_id, stable_effect_hash, encrypted_record,
                    record_hash, anchor_sequence, anchor_entry_hash,
                    anchor_signature_hex, local_signature_hex
             FROM effect_receipt WHERE effect_id = ?1",
            [effect_id],
            |row| {
                Ok(StoredEffectReceipt {
                    effect_id: row.get(0)?,
                    mission_id: row.get(1)?,
                    stable_effect_hash: row.get(2)?,
                    encrypted_record: row.get(3)?,
                    record_hash: row.get(4)?,
                    anchor: AuditAnchor {
                        sequence: row.get(5)?,
                        entry_hash: row.get(6)?,
                        signature_hex: row.get(7)?,
                    },
                    local_signature_hex: row.get(8)?,
                })
            },
        )
        .optional()
        .map_err(StoreError::from)
}

fn load_stored_effect_noncommit(
    connection: &Connection,
    effect_id: &str,
) -> Result<Option<StoredEffectNonCommit>, StoreError> {
    connection
        .query_row(
            "SELECT effect_id, mission_id, stable_effect_hash, encrypted_record,
                    record_hash, anchor_sequence, anchor_entry_hash,
                    anchor_signature_hex, local_signature_hex
             FROM effect_noncommit WHERE effect_id = ?1",
            [effect_id],
            |row| {
                Ok(StoredEffectNonCommit {
                    effect_id: row.get(0)?,
                    mission_id: row.get(1)?,
                    stable_effect_hash: row.get(2)?,
                    encrypted_record: row.get(3)?,
                    record_hash: row.get(4)?,
                    anchor: AuditAnchor {
                        sequence: row.get(5)?,
                        entry_hash: row.get(6)?,
                        signature_hex: row.get(7)?,
                    },
                    local_signature_hex: row.get(8)?,
                })
            },
        )
        .optional()
        .map_err(StoreError::from)
}

fn verify_stored_effect_receipt(
    connection: &Connection,
    authority: &LocalAuthority,
    trusted_broker: &TrustedBrokerEnrollment,
    stored: &StoredEffectReceipt,
) -> Result<StoredEffectReceiptPayload, StoreError> {
    let mismatch = || StoreError::EffectReceiptMismatch(stored.effect_id.clone());
    authority
        .verify_effect_bytes(
            &effect_receipt_record_bytes(
                &stored.effect_id,
                &stored.mission_id,
                &stored.stable_effect_hash,
                &stored.record_hash,
                &stored.anchor,
            ),
            &stored.local_signature_hex,
        )
        .map_err(|_| mismatch())?;
    if blob_hash(&stored.encrypted_record) != stored.record_hash {
        return Err(mismatch());
    }
    let payload: StoredEffectReceiptPayload = authority
        .decrypt_json(
            &stored.encrypted_record,
            effect_receipt_aad(&stored.effect_id).as_bytes(),
        )
        .map_err(|_| mismatch())?;
    if payload.receipt.effect_id != stored.effect_id
        || payload.receipt.mission_id != stored.mission_id
        || payload.receipt.stable_effect_hash != stored.stable_effect_hash
        || payload.permit.command.effect_id != stored.effect_id
        || payload.permit.command.mission_id != stored.mission_id
        || payload.permit.stable_effect_hash != stored.stable_effect_hash
    {
        return Err(mismatch());
    }
    authority
        .verify_effect_permit(&payload.permit)
        .map_err(|_| mismatch())?;
    crate::verify_effect_receipt(
        trusted_broker,
        &payload.broker_session,
        &payload.permit,
        &payload.receipt,
    )
    .map_err(|_| mismatch())?;
    let authorization =
        load_stored_effect_authorization(connection, &stored.effect_id)?.ok_or_else(mismatch)?;
    let command = verify_stored_effect_authorization(connection, authority, &authorization)?;
    if command != payload.permit.command {
        return Err(mismatch());
    }
    verify_blob_binding(
        connection,
        EFFECT_RECEIPT_ACTION,
        &stored.effect_id,
        "effectReceipt",
        &stored.encrypted_record,
    )?;
    let anchor_matches = connection
        .query_row(
            "SELECT 1 FROM audit_ledger
             WHERE sequence = ?1 AND entry_hash = ?2 AND signature_hex = ?3
               AND command_id = ?4 AND command_hash = ?5 AND action = ?6
               AND entity_id = ?4",
            params![
                stored.anchor.sequence,
                stored.anchor.entry_hash,
                stored.anchor.signature_hex,
                stored.effect_id,
                stored.stable_effect_hash,
                EFFECT_RECEIPT_ACTION
            ],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if !anchor_matches {
        return Err(mismatch());
    }
    Ok(payload)
}

fn verify_stored_effect_noncommit(
    connection: &Connection,
    authority: &LocalAuthority,
    trusted_broker: &TrustedBrokerEnrollment,
    stored: &StoredEffectNonCommit,
) -> Result<StoredEffectNonCommitPayload, StoreError> {
    let mismatch = || StoreError::EffectNonCommitMismatch(stored.effect_id.clone());
    authority
        .verify_effect_bytes(
            &effect_noncommit_record_bytes(
                &stored.effect_id,
                &stored.mission_id,
                &stored.stable_effect_hash,
                &stored.record_hash,
                &stored.anchor,
            ),
            &stored.local_signature_hex,
        )
        .map_err(|_| mismatch())?;
    if blob_hash(&stored.encrypted_record) != stored.record_hash {
        return Err(mismatch());
    }
    let payload: StoredEffectNonCommitPayload = authority
        .decrypt_json(
            &stored.encrypted_record,
            effect_noncommit_aad(&stored.effect_id).as_bytes(),
        )
        .map_err(|_| mismatch())?;
    if payload.attestation.effect_id != stored.effect_id
        || payload.attestation.mission_id != stored.mission_id
        || payload.attestation.stable_effect_hash != stored.stable_effect_hash
        || payload.permit.command.effect_id != stored.effect_id
        || payload.permit.command.mission_id != stored.mission_id
        || payload.permit.stable_effect_hash != stored.stable_effect_hash
    {
        return Err(mismatch());
    }
    authority
        .verify_effect_permit(&payload.permit)
        .map_err(|_| mismatch())?;
    crate::verify_effect_noncommit(
        trusted_broker,
        &payload.broker_session,
        &payload.permit,
        &payload.attestation,
    )
    .map_err(|_| mismatch())?;
    let authorization =
        load_stored_effect_authorization(connection, &stored.effect_id)?.ok_or_else(mismatch)?;
    let command = verify_stored_effect_authorization(connection, authority, &authorization)?;
    if command != payload.permit.command {
        return Err(mismatch());
    }
    verify_blob_binding(
        connection,
        EFFECT_NONCOMMIT_ACTION,
        &stored.effect_id,
        "effectNonCommit",
        &stored.encrypted_record,
    )?;
    let anchor_matches = connection
        .query_row(
            "SELECT 1 FROM audit_ledger
             WHERE sequence = ?1 AND entry_hash = ?2 AND signature_hex = ?3
               AND command_id = ?4 AND command_hash = ?5 AND action = ?6
               AND entity_id = ?4",
            params![
                stored.anchor.sequence,
                stored.anchor.entry_hash,
                stored.anchor.signature_hex,
                stored.effect_id,
                stored.stable_effect_hash,
                EFFECT_NONCOMMIT_ACTION
            ],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if !anchor_matches {
        return Err(mismatch());
    }
    Ok(payload)
}

fn require_no_effect_fence(connection: &Connection) -> Result<(), StoreError> {
    let effect_id = connection
        .query_row(
            "SELECT effect_id FROM effect_fence ORDER BY effect_id LIMIT 1",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    match effect_id {
        Some(effect_id) => Err(StoreError::EffectFenceActive(effect_id)),
        None => Ok(()),
    }
}

fn require_effect_fence(connection: &Connection, effect_id: &str) -> Result<(), StoreError> {
    let pending = connection
        .query_row(
            "SELECT 1 FROM effect_fence WHERE effect_id = ?1",
            [effect_id],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if pending {
        Ok(())
    } else {
        Err(StoreError::EffectFenceMismatch)
    }
}

fn clear_effect_fence(transaction: &Transaction<'_>, effect_id: &str) -> Result<(), StoreError> {
    if transaction.execute("DELETE FROM effect_fence WHERE effect_id = ?1", [effect_id])? == 1 {
        Ok(())
    } else {
        Err(StoreError::EffectFenceMismatch)
    }
}

fn audit_anchor(anchor: &EffectAuditAnchor) -> AuditAnchor {
    AuditAnchor {
        sequence: anchor.sequence,
        entry_hash: anchor.entry_hash.clone(),
        signature_hex: anchor.signature_hex.clone(),
    }
}

fn is_lower_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn verify_expected_anchor(
    transaction: &Transaction<'_>,
    authority: &LocalAuthority,
    trusted_broker: Option<&TrustedBrokerEnrollment>,
    expected: Option<&AuditAnchor>,
) -> Result<(), StoreError> {
    let actual = verified_audit_tail(transaction, authority)?;
    match (actual, expected) {
        (None, None) => {}
        (Some(actual), Some(expected)) if &actual == expected => {}
        _ => return Err(StoreError::AuditAnchorMismatch),
    }
    verify_all_bindings(transaction, authority, trusted_broker)
}

fn verified_audit_tail(
    connection: &Connection,
    authority: &LocalAuthority,
) -> Result<Option<AuditAnchor>, StoreError> {
    let mut statement = connection.prepare(
        "SELECT sequence, audit_id, command_id, command_hash, actor, action, entity_id,
                created_at_ms, observed_at_ms, state_kind, state_hash, previous_hash, entry_hash,
                signature_hex
         FROM audit_ledger ORDER BY sequence ASC",
    )?;
    let mut rows = statement.query([])?;
    let mut expected_previous = "GENESIS".to_owned();
    let mut actual_tail = None;
    while let Some(row) = rows.next()? {
        let sequence: i64 = row.get(0)?;
        let audit_id: String = row.get(1)?;
        let command_id: String = row.get(2)?;
        let command_hash: String = row.get(3)?;
        let actor: String = row.get(4)?;
        let action: String = row.get(5)?;
        let entity_id: String = row.get(6)?;
        let created_at_ms: i64 = row.get(7)?;
        let observed_at_ms: i64 = row.get(8)?;
        let state_kind: String = row.get(9)?;
        let state_hash: String = row.get(10)?;
        let previous_hash: String = row.get(11)?;
        let entry_hash: String = row.get(12)?;
        let signature_hex: String = row.get(13)?;
        let expected_hash = audit_hash(
            &expected_previous,
            observed_at_ms,
            &AuditRecord {
                id: &audit_id,
                command_id: &command_id,
                command_hash: &command_hash,
                actor: &actor,
                action: &action,
                entity_id: &entity_id,
                created_at_ms,
                state_kind: &state_kind,
                state_hash: &state_hash,
            },
        );
        if previous_hash != expected_previous || entry_hash != expected_hash {
            return Err(StoreError::AuditChainMismatch(sequence));
        }
        authority.verify_bytes(entry_hash.as_bytes(), &signature_hex)?;
        expected_previous.clone_from(&entry_hash);
        actual_tail = Some(AuditAnchor {
            sequence,
            entry_hash,
            signature_hex,
        });
    }
    Ok(actual_tail)
}

fn append_audit(
    transaction: &Transaction<'_>,
    authority: &LocalAuthority,
    record: &AuditRecord<'_>,
) -> Result<AuditAnchor, StoreError> {
    let observed_at_ms = current_unix_ms()?;
    let previous_hash: String = transaction
        .query_row(
            "SELECT entry_hash FROM audit_ledger ORDER BY sequence DESC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .optional()?
        .unwrap_or_else(|| "GENESIS".to_owned());
    let entry_hash = audit_hash(&previous_hash, observed_at_ms, record);
    let signature_hex = authority.sign_bytes(entry_hash.as_bytes());
    transaction.execute(
        "INSERT INTO audit_ledger
            (audit_id, command_id, command_hash, actor, action, entity_id, created_at_ms,
             observed_at_ms, state_kind, state_hash, previous_hash, entry_hash, signature_hex)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        params![
            record.id,
            record.command_id,
            record.command_hash,
            record.actor,
            record.action,
            record.entity_id,
            record.created_at_ms,
            observed_at_ms,
            record.state_kind,
            record.state_hash,
            previous_hash,
            entry_hash,
            signature_hex,
        ],
    )?;
    Ok(AuditAnchor {
        sequence: transaction.last_insert_rowid(),
        entry_hash,
        signature_hex,
    })
}

fn verify_all_bindings(
    connection: &Connection,
    authority: &LocalAuthority,
    trusted_broker: Option<&TrustedBrokerEnrollment>,
) -> Result<(), StoreError> {
    load_runtime_control(connection, authority, trusted_broker)?;
    verify_audited_entities_exist(connection, MISSION_COMMAND_ACTION)?;
    verify_audited_entities_exist(connection, RECEIPT_COMMIT_ACTION)?;
    verify_audited_entities_exist(connection, EFFECT_AUTHORIZATION_ACTION)?;
    verify_audited_entities_exist(connection, EFFECT_RECEIPT_ACTION)?;
    verify_audited_entities_exist(connection, EFFECT_NONCOMMIT_ACTION)?;
    verify_command_audit_reconciliation(connection, authority)?;
    verify_effect_authorization_bindings(connection, authority)?;
    verify_effect_receipt_bindings(connection, authority, trusted_broker)?;
    verify_effect_noncommit_bindings(connection, authority, trusted_broker)?;
    verify_effect_resolution_bindings(connection)?;
    let missions = {
        let mut statement = connection.prepare(
            "SELECT mission_id, status_json, scope_digest, encrypted_blob,
                    created_at_ms, updated_at_ms FROM mission_state",
        )?;
        statement
            .query_map([], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            })?
            .collect::<Result<Vec<(String, String, String, Vec<u8>, i64, i64)>, _>>()?
    };
    for (mission_id, status, scope_digest, blob, created_at_ms, updated_at_ms) in missions {
        verify_blob_binding(
            connection,
            MISSION_COMMAND_ACTION,
            &mission_id,
            &format!("mission:{status}"),
            &blob,
        )?;
        let mission: Mission =
            authority.decrypt_json(&blob, mission_aad(&mission_id).as_bytes())?;
        let encoded_status = serde_json::to_string(&mission.status)
            .map_err(|error| CryptoError::Serialization(error.to_string()))?;
        if mission.id != mission_id
            || encoded_status != status
            || mission.scope_digest != scope_digest
            || mission.created_at_ms != created_at_ms
            || mission.updated_at_ms != updated_at_ms
        {
            return Err(StoreError::StateBindingMismatch(mission_id));
        }
        validate_mission_snapshot(&mission, authority)?;
    }
    let receipts = {
        let mut statement = connection.prepare(
            "SELECT receipt_id, mission_id, encrypted_blob, completed_at_ms FROM receipt_state",
        )?;
        statement
            .query_map([], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })?
            .collect::<Result<Vec<(String, String, Vec<u8>, i64)>, _>>()?
    };
    for (receipt_id, mission_id, blob, completed_at_ms) in receipts {
        verify_blob_binding(
            connection,
            RECEIPT_COMMIT_ACTION,
            &receipt_id,
            "receipt",
            &blob,
        )?;
        let receipt: Receipt =
            authority.decrypt_json(&blob, receipt_aad(&receipt_id, &mission_id).as_bytes())?;
        if receipt.id != receipt_id
            || receipt.mission_id != mission_id
            || receipt.completed_at_ms != completed_at_ms
        {
            return Err(StoreError::StateBindingMismatch(receipt_id));
        }
    }
    Ok(())
}

fn load_runtime_control(
    connection: &Connection,
    authority: &LocalAuthority,
    trusted_broker: Option<&TrustedBrokerEnrollment>,
) -> Result<RuntimeControl, StoreError> {
    let stored = connection
        .query_row(
            "SELECT enabled, revision, updated_at_ms, signature_hex
             FROM runtime_control WHERE singleton_id = ?1",
            [RUNTIME_CONTROL_ID],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                ))
            },
        )
        .optional()?;
    let mut statement = connection.prepare(
        "SELECT enabled, revision, updated_at_ms, signature_hex
         FROM runtime_control_history ORDER BY revision ASC",
    )?;
    let history = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut checkpoints = load_runtime_recovery_checkpoints(connection)?;
    let mut latest = None;
    let mut previous_revision = 0_u64;
    for (enabled, revision, updated_at_ms, signature_hex) in history {
        let revision = u64::try_from(revision).map_err(|_| StoreError::RuntimeControlMismatch)?;
        if !matches!(enabled, 0 | 1) || revision <= previous_revision || updated_at_ms < 0 {
            return Err(StoreError::RuntimeControlMismatch);
        }
        let authorization = RuntimeControlAuthorization {
            protocol_version: EFFECT_PROTOCOL_VERSION,
            enabled: enabled == 1,
            revision,
            updated_at_ms,
            core_key_id: authority.effect_key_id(),
            authorization_signature_hex: signature_hex,
        };
        authority
            .verify_runtime_control(&authorization)
            .map_err(|_| StoreError::RuntimeControlMismatch)?;
        let expected = previous_revision
            .checked_add(1)
            .ok_or(StoreError::RuntimeControlRevisionOverflow)?;
        if let Some(receipt) = checkpoints.remove(&revision) {
            crate::effect::verify_runtime_control_receipt(
                trusted_broker.ok_or(StoreError::MissingTrustedBrokerEnrollment)?,
                &authorization,
                &receipt,
            )?;
        } else if revision != expected {
            return Err(StoreError::RuntimeControlMismatch);
        }
        previous_revision = revision;
        latest = Some(authorization);
    }
    if !checkpoints.is_empty() {
        return Err(StoreError::RuntimeControlMismatch);
    }
    match (stored, latest) {
        (None, None) => Ok(RuntimeControl {
            enabled: false,
            revision: 0,
            updated_at_ms: 0,
        }),
        (Some((enabled, revision, updated_at_ms, signature_hex)), Some(latest)) => {
            let revision =
                u64::try_from(revision).map_err(|_| StoreError::RuntimeControlMismatch)?;
            if !matches!(enabled, 0 | 1)
                || (enabled == 1) != latest.enabled
                || revision != latest.revision
                || updated_at_ms != latest.updated_at_ms
                || signature_hex != latest.authorization_signature_hex
            {
                return Err(StoreError::RuntimeControlMismatch);
            }
            Ok(RuntimeControl {
                enabled: latest.enabled,
                revision: latest.revision,
                updated_at_ms: latest.updated_at_ms,
            })
        }
        _ => Err(StoreError::RuntimeControlMismatch),
    }
}

fn load_runtime_recovery_checkpoints(
    connection: &Connection,
) -> Result<BTreeMap<u64, RuntimeControlReceipt>, StoreError> {
    let mut statement = connection.prepare(
        "SELECT revision, authorization_hash, checkpoint_nonce, request_nonce,
                broker_key_id, broker_signature_hex
         FROM runtime_control_recovery_checkpoint ORDER BY revision ASC",
    )?;
    statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                RuntimeControlReceipt {
                    protocol_version: EFFECT_PROTOCOL_VERSION,
                    authorization_hash: row.get(1)?,
                    checkpoint_nonce: row.get(2)?,
                    request_nonce: row.get(3)?,
                    broker_key_id: row.get(4)?,
                    broker_signature_hex: row.get(5)?,
                },
            ))
        })?
        .map(|result| {
            let (revision, receipt) = result?;
            let revision =
                u64::try_from(revision).map_err(|_| StoreError::RuntimeControlMismatch)?;
            Ok((revision, receipt))
        })
        .collect()
}

fn require_runtime_enabled(
    connection: &Connection,
    authority: &LocalAuthority,
    trusted_broker: Option<&TrustedBrokerEnrollment>,
) -> Result<(), StoreError> {
    if load_runtime_control(connection, authority, trusted_broker)?.enabled {
        Ok(())
    } else {
        Err(StoreError::RuntimeDisabled)
    }
}

fn verify_effect_authorization_bindings(
    connection: &Connection,
    authority: &LocalAuthority,
) -> Result<(), StoreError> {
    let effect_ids = {
        let mut statement = connection.prepare("SELECT effect_id FROM effect_authorization")?;
        statement
            .query_map([], |row| row.get(0))?
            .collect::<Result<Vec<String>, _>>()?
    };
    for effect_id in effect_ids {
        let stored = load_stored_effect_authorization(connection, &effect_id)?
            .ok_or_else(|| StoreError::EffectAuthorizationMismatch(effect_id.clone()))?;
        verify_stored_effect_authorization(connection, authority, &stored)?;
        effect_authorization_anchor(
            connection,
            &effect_id,
            &stored.stable_effect_hash,
            &stored.command_blob_hash,
        )?;
    }
    Ok(())
}

fn verify_effect_receipt_bindings(
    connection: &Connection,
    authority: &LocalAuthority,
    trusted_broker: Option<&TrustedBrokerEnrollment>,
) -> Result<(), StoreError> {
    let effect_ids = {
        let mut statement = connection.prepare("SELECT effect_id FROM effect_receipt")?;
        statement
            .query_map([], |row| row.get(0))?
            .collect::<Result<Vec<String>, _>>()?
    };
    let trusted_broker = if effect_ids.is_empty() {
        None
    } else {
        Some(trusted_broker.ok_or(StoreError::MissingTrustedBrokerEnrollment)?)
    };
    for effect_id in effect_ids {
        let stored = load_stored_effect_receipt(connection, &effect_id)?
            .ok_or_else(|| StoreError::EffectReceiptMismatch(effect_id.clone()))?;
        verify_stored_effect_receipt(
            connection,
            authority,
            trusted_broker.expect("non-empty receipt rows require broker trust"),
            &stored,
        )?;
    }
    Ok(())
}

fn verify_effect_noncommit_bindings(
    connection: &Connection,
    authority: &LocalAuthority,
    trusted_broker: Option<&TrustedBrokerEnrollment>,
) -> Result<(), StoreError> {
    let effect_ids = {
        let mut statement = connection.prepare("SELECT effect_id FROM effect_noncommit")?;
        statement
            .query_map([], |row| row.get(0))?
            .collect::<Result<Vec<String>, _>>()?
    };
    let trusted_broker = if effect_ids.is_empty() {
        None
    } else {
        Some(trusted_broker.ok_or(StoreError::MissingTrustedBrokerEnrollment)?)
    };
    for effect_id in effect_ids {
        let stored = load_stored_effect_noncommit(connection, &effect_id)?
            .ok_or_else(|| StoreError::EffectNonCommitMismatch(effect_id.clone()))?;
        verify_stored_effect_noncommit(
            connection,
            authority,
            trusted_broker.expect("non-empty noncommit rows require broker trust"),
            &stored,
        )?;
    }
    Ok(())
}

fn verify_effect_resolution_bindings(connection: &Connection) -> Result<(), StoreError> {
    let authorizations = {
        let mut statement = connection.prepare(
            "SELECT effect_id, mission_id, stable_effect_hash FROM effect_authorization",
        )?;
        statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?
    };
    for (effect_id, mission_id, stable_effect_hash) in authorizations {
        let fence: Option<(String, String)> = connection
            .query_row(
                "SELECT mission_id, stable_effect_hash FROM effect_fence WHERE effect_id = ?1",
                [&effect_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        if fence
            .as_ref()
            .is_some_and(|(mission, hash)| mission != &mission_id || hash != &stable_effect_hash)
        {
            return Err(StoreError::EffectAuthorizationMismatch(effect_id));
        }
        let receipt_count: i64 = connection.query_row(
            "SELECT COUNT(*) FROM effect_receipt WHERE effect_id = ?1",
            [&effect_id],
            |row| row.get(0),
        )?;
        let noncommit_count: i64 = connection.query_row(
            "SELECT COUNT(*) FROM effect_noncommit WHERE effect_id = ?1",
            [&effect_id],
            |row| row.get(0),
        )?;
        let resolution_count = i64::from(fence.is_some()) + receipt_count + noncommit_count;
        if resolution_count != 1 {
            return Err(StoreError::EffectAuthorizationMismatch(effect_id));
        }
    }
    let orphan_fence: Option<String> = connection
        .query_row(
            "SELECT fence.effect_id FROM effect_fence AS fence
             LEFT JOIN effect_authorization AS authorization
               ON authorization.effect_id = fence.effect_id
             WHERE authorization.effect_id IS NULL LIMIT 1",
            [],
            |row| row.get(0),
        )
        .optional()?;
    if let Some(effect_id) = orphan_fence {
        return Err(StoreError::EffectAuthorizationMismatch(effect_id));
    }
    Ok(())
}

fn verify_audited_entities_exist(connection: &Connection, action: &str) -> Result<(), StoreError> {
    let lookup = match action {
        MISSION_COMMAND_ACTION => "SELECT 1 FROM mission_state WHERE mission_id = ?1",
        RECEIPT_COMMIT_ACTION => "SELECT 1 FROM receipt_state WHERE receipt_id = ?1",
        EFFECT_AUTHORIZATION_ACTION => "SELECT 1 FROM effect_authorization WHERE effect_id = ?1",
        EFFECT_RECEIPT_ACTION => "SELECT 1 FROM effect_receipt WHERE effect_id = ?1",
        EFFECT_NONCOMMIT_ACTION => "SELECT 1 FROM effect_noncommit WHERE effect_id = ?1",
        _ => return Err(StoreError::StateBindingMismatch(action.to_owned())),
    };
    let entities = {
        let mut statement =
            connection.prepare("SELECT DISTINCT entity_id FROM audit_ledger WHERE action = ?1")?;
        statement
            .query_map([action], |row| row.get(0))?
            .collect::<Result<Vec<String>, _>>()?
    };
    for entity_id in entities {
        if connection
            .query_row(lookup, [&entity_id], |_| Ok(()))
            .optional()?
            .is_none()
        {
            return Err(StoreError::StateBindingMismatch(entity_id));
        }
    }
    Ok(())
}

fn verify_command_audit_reconciliation(
    connection: &Connection,
    authority: &LocalAuthority,
) -> Result<(), StoreError> {
    let unbound_audit: Option<String> = connection
        .query_row(
            "SELECT audit.command_id FROM audit_ledger AS audit
             LEFT JOIN mission_command_result AS command
               ON command.command_id = audit.command_id
             WHERE audit.action IN (?1, ?2)
               AND (command.command_id IS NULL
                 OR command.command_hash != audit.command_hash
                 OR (audit.action = ?1 AND command.mission_id != audit.entity_id))
             LIMIT 1",
            params![MISSION_COMMAND_ACTION, RECEIPT_COMMIT_ACTION],
            |row| row.get(0),
        )
        .optional()?;
    if let Some(command_id) = unbound_audit {
        return Err(StoreError::StateBindingMismatch(command_id));
    }
    for command in load_all_stored_command_results(connection)? {
        verify_stored_command_result(connection, authority, &command)?;
    }
    Ok(())
}

fn load_stored_command_result(
    connection: &Connection,
    command_id: &str,
) -> Result<Option<StoredCommandResult>, StoreError> {
    connection
        .query_row(
            "SELECT command_id, mission_id, command_hash, encrypted_result, result_hash,
                    anchor_sequence, anchor_entry_hash, anchor_signature_hex,
                    record_signature_hex
             FROM mission_command_result WHERE command_id = ?1",
            [command_id],
            stored_command_result_from_row,
        )
        .optional()
        .map_err(StoreError::from)
}

fn load_all_stored_command_results(
    connection: &Connection,
) -> Result<Vec<StoredCommandResult>, StoreError> {
    let mut statement = connection.prepare(
        "SELECT command_id, mission_id, command_hash, encrypted_result, result_hash,
                anchor_sequence, anchor_entry_hash, anchor_signature_hex,
                record_signature_hex
         FROM mission_command_result",
    )?;
    let rows = statement.query_map([], stored_command_result_from_row)?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn stored_command_result_from_row(
    row: &rusqlite::Row<'_>,
) -> Result<StoredCommandResult, rusqlite::Error> {
    Ok(StoredCommandResult {
        command_id: row.get(0)?,
        mission_id: row.get(1)?,
        command_hash: row.get(2)?,
        encrypted_result: row.get(3)?,
        result_hash: row.get(4)?,
        anchor: AuditAnchor {
            sequence: row.get(5)?,
            entry_hash: row.get(6)?,
            signature_hex: row.get(7)?,
        },
        record_signature_hex: row.get(8)?,
    })
}

fn verify_stored_command_result(
    connection: &Connection,
    authority: &LocalAuthority,
    stored: &StoredCommandResult,
) -> Result<MissionCommandResult, StoreError> {
    let mismatch = || StoreError::StateBindingMismatch(stored.command_id.clone());
    authority
        .verify_bytes(
            &command_result_record_bytes(
                &stored.command_id,
                &stored.mission_id,
                &stored.command_hash,
                &stored.result_hash,
                &stored.anchor,
            ),
            &stored.record_signature_hex,
        )
        .map_err(|_| mismatch())?;
    if blob_hash(&stored.encrypted_result) != stored.result_hash {
        return Err(mismatch());
    }
    let result: MissionCommandResult = authority
        .decrypt_json(
            &stored.encrypted_result,
            command_result_aad(&stored.command_id).as_bytes(),
        )
        .map_err(|_| mismatch())?;
    if result.mission.id != stored.mission_id || result.anchor != stored.anchor {
        return Err(mismatch());
    }
    validate_mission_snapshot(&result.mission, authority).map_err(|_| mismatch())?;
    if let Some(receipt) = result.receipt.as_ref() {
        validate_receipt(&result.mission, receipt, authority).map_err(|_| mismatch())?;
    }
    let mission_audit_exists = connection
        .query_row(
            "SELECT 1 FROM audit_ledger
             WHERE command_id = ?1 AND command_hash = ?2 AND action = ?3 AND entity_id = ?4
             LIMIT 1",
            params![
                stored.command_id,
                stored.command_hash,
                MISSION_COMMAND_ACTION,
                stored.mission_id
            ],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    let anchor_matches = connection
        .query_row(
            "SELECT 1 FROM audit_ledger
             WHERE sequence = ?1 AND entry_hash = ?2 AND signature_hex = ?3
               AND command_id = ?4 AND command_hash = ?5",
            params![
                stored.anchor.sequence,
                stored.anchor.entry_hash,
                stored.anchor.signature_hex,
                stored.command_id,
                stored.command_hash
            ],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    let receipt_audit_matches = match result.receipt.as_ref() {
        Some(receipt) => connection
            .query_row(
                "SELECT 1 FROM audit_ledger
                 WHERE sequence = ?1 AND command_id = ?2 AND command_hash = ?3
                   AND action = ?4 AND entity_id = ?5",
                params![
                    stored.anchor.sequence,
                    stored.command_id,
                    stored.command_hash,
                    RECEIPT_COMMIT_ACTION,
                    receipt.id
                ],
                |_| Ok(()),
            )
            .optional()?
            .is_some(),
        None => true,
    };
    if !mission_audit_exists || !anchor_matches || !receipt_audit_matches {
        return Err(mismatch());
    }
    Ok(result)
}

fn verify_blob_binding(
    connection: &Connection,
    action: &str,
    entity_id: &str,
    expected_state_kind: &str,
    blob: &[u8],
) -> Result<(), StoreError> {
    let binding: Option<(String, String)> = connection
        .query_row(
            "SELECT state_kind, state_hash FROM audit_ledger
             WHERE action = ?1 AND entity_id = ?2 ORDER BY sequence DESC LIMIT 1",
            params![action, entity_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    if binding.as_ref() != Some(&(expected_state_kind.to_owned(), blob_hash(blob))) {
        return Err(StoreError::StateBindingMismatch(entity_id.to_owned()));
    }
    Ok(())
}

fn mission_aad(mission_id: &str) -> String {
    format!("openopen:mission:v2:{mission_id}")
}

fn receipt_aad(receipt_id: &str, mission_id: &str) -> String {
    format!("openopen:receipt:v2:{mission_id}:{receipt_id}")
}

fn command_result_aad(command_id: &str) -> String {
    format!("openopen:mission-command-result:v1:{command_id}")
}

fn effect_authorization_aad(effect_id: &str) -> String {
    format!("openopen:effect-authorization:v1:{effect_id}")
}

fn effect_receipt_aad(effect_id: &str) -> String {
    format!("openopen:effect-receipt:v1:{effect_id}")
}

fn effect_noncommit_aad(effect_id: &str) -> String {
    format!("openopen:effect-noncommit:v1:{effect_id}")
}

fn effect_authorization_record_bytes(
    effect_id: &str,
    mission_id: &str,
    stable_effect_hash: &str,
    command_blob_hash: &str,
    source_anchor: &AuditAnchor,
) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "commandBlobHash": command_blob_hash,
        "effectId": effect_id,
        "missionId": mission_id,
        "sourceAnchorEntryHash": source_anchor.entry_hash,
        "sourceAnchorSequence": source_anchor.sequence,
        "sourceAnchorSignatureHex": source_anchor.signature_hex,
        "stableEffectHash": stable_effect_hash,
        "version": 1,
    }))
    .expect("effect-authorization record fields are infallibly serializable")
}

fn effect_receipt_record_bytes(
    effect_id: &str,
    mission_id: &str,
    stable_effect_hash: &str,
    record_hash: &str,
    anchor: &AuditAnchor,
) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "anchorEntryHash": anchor.entry_hash,
        "anchorSequence": anchor.sequence,
        "anchorSignatureHex": anchor.signature_hex,
        "effectId": effect_id,
        "missionId": mission_id,
        "recordHash": record_hash,
        "stableEffectHash": stable_effect_hash,
        "version": 1,
    }))
    .expect("effect-receipt record fields are infallibly serializable")
}

fn effect_noncommit_record_bytes(
    effect_id: &str,
    mission_id: &str,
    stable_effect_hash: &str,
    record_hash: &str,
    anchor: &AuditAnchor,
) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "anchorEntryHash": anchor.entry_hash,
        "anchorSequence": anchor.sequence,
        "anchorSignatureHex": anchor.signature_hex,
        "effectId": effect_id,
        "missionId": mission_id,
        "recordHash": record_hash,
        "stableEffectHash": stable_effect_hash,
        "version": 1,
    }))
    .expect("effect-noncommit record fields are infallibly serializable")
}

fn command_result_record_bytes(
    command_id: &str,
    mission_id: &str,
    command_hash: &str,
    result_hash: &str,
    anchor: &AuditAnchor,
) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "anchorEntryHash": anchor.entry_hash,
        "anchorSequence": anchor.sequence,
        "anchorSignatureHex": anchor.signature_hex,
        "commandHash": command_hash,
        "commandId": command_id,
        "missionId": mission_id,
        "resultHash": result_hash,
        "version": 1,
    }))
    .expect("command-result record fields are infallibly serializable")
}

fn blob_hash(blob: &[u8]) -> String {
    format!("{:x}", Sha256::digest(blob))
}

fn audit_hash(previous_hash: &str, observed_at_ms: i64, record: &AuditRecord<'_>) -> String {
    let canonical = serde_json::json!({
        "action": record.action,
        "actor": record.actor,
        "auditId": record.id,
        "commandId": record.command_id,
        "commandHash": record.command_hash,
        "createdAtMs": record.created_at_ms,
        "entityId": record.entity_id,
        "observedAtMs": observed_at_ms,
        "previousHash": previous_hash,
        "stateHash": record.state_hash,
        "stateKind": record.state_kind,
        "version": 5,
    });
    format!("{:x}", Sha256::digest(canonical.to_string().as_bytes()))
}
