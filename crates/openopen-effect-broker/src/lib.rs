//! Protected Mission-file effect broker engine.
//!
//! The production constructor rejects a caller whose authenticated audit EUID
//! equals the broker process EUID. The signed macOS wrapper remains responsible
//! for deriving that audit EUID from its authenticated XPC peer and for
//! supplying the protected root, enrolled keys, and session nonce.

mod journal;
mod workspace;

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use journal::{EffectState, Journal, JournalEntry};
use openopen_protocol::{
    EFFECT_PROTOCOL_VERSION, EffectBrokerSession, EffectCommand, EffectNonCommit, EffectPermit,
    EffectPermitPurpose, EffectReceipt, EffectReconciliation, MAX_EFFECT_APPROVAL_IDS,
    MAX_EFFECT_PAYLOAD_BYTES, MAX_EFFECT_SCOPE_DIGEST_BYTES, MissionFileEffect, PayloadDescriptor,
    effect_command_signing_bytes, effect_noncommit_signing_bytes, effect_permit_hash,
    effect_permit_signing_bytes, effect_receipt_signing_bytes, is_canonical_effect_identifier,
};
use rustix::fs::{FileType, FlockOperation, Mode, OFlags, fchmod, flock, fstat, open};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::io::Read;
use std::os::fd::OwnedFd;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;
use workspace::{CommittedFile, Workspace};

const MAX_PATH_COMPONENTS: usize = 16;
const MAX_PATH_COMPONENT_BYTES: usize = 128;
const MAX_PERMIT_TTL_MS: i64 = 30_000;
const PAYLOAD_VALIDATION_CHUNK_BYTES: usize = 64 * 1024;
const PROTECTED_USERS_ROOT: &str = "/Library/Application Support/com.thesongzhu.OpenOpen/Users";
const OPERATION_LOCK_NAME: &str = ".openopen-effect.lock";

#[derive(Debug, Error)]
pub enum BrokerError {
    #[error("broker configuration is not a distinct protected security principal")]
    InvalidSecurityBoundary,
    #[error("broker root is not an exact protected directory owned by this broker")]
    InvalidRoot,
    #[error("another broker worker owns the protected effect operation lock")]
    OperationBusy,
    #[error("broker session is malformed or expired")]
    InvalidSession,
    #[error("effect command is malformed")]
    InvalidCommand,
    #[error("effect permit is stale, changed, or signed by an unenrolled Core key")]
    InvalidPermit,
    #[error("effect id was reused for a different typed effect")]
    EffectConflict,
    #[error("recovery-only permit has no journaled committed effect to attest")]
    EffectNotCommitted,
    #[error("payload bytes do not match the signed length and SHA-256")]
    PayloadMismatch,
    #[error("protected broker journal is inconsistent")]
    JournalMismatch,
    #[error("filesystem boundary validation failed")]
    WorkspaceBoundary,
    #[error("system clock is outside the supported range")]
    InvalidSystemTime,
    #[error("cryptographic encoding or signature is malformed")]
    Crypto,
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Clone)]
pub struct BrokerConfig {
    pub protected_root: PathBuf,
    pub authenticated_audit_euid: u32,
    pub enrolled_core_verifying_key: [u8; 32],
    pub broker_signing_seed: [u8; 32],
    pub session_nonce: String,
    pub session_expires_at_ms: i64,
}

pub struct BrokerEngine {
    authenticated_audit_euid: u32,
    core_key_id: String,
    core_verifying_key: VerifyingKey,
    broker_key_id: String,
    broker_signing_key: SigningKey,
    session_nonce: String,
    session_expires_at_ms: i64,
    workspace: Workspace,
    journal: Journal,
    _operation_lock: OwnedFd,
}

impl BrokerEngine {
    /// Opens a production broker and rejects a same-EUID caller boundary.
    ///
    /// The caller EUID must come from the authenticated XPC audit token, never
    /// from an effect command or JSON field.
    ///
    /// # Errors
    ///
    /// Returns an error for a same-principal caller, invalid keys/session, or
    /// an unprotected root/journal.
    pub fn open(config: BrokerConfig) -> Result<Self, BrokerError> {
        if config.authenticated_audit_euid == rustix::process::geteuid().as_raw() {
            return Err(BrokerError::InvalidSecurityBoundary);
        }
        if config.protected_root != protected_root_for_audit_euid(config.authenticated_audit_euid) {
            return Err(BrokerError::InvalidRoot);
        }
        Self::open_internal(config)
    }

    fn open_internal(config: BrokerConfig) -> Result<Self, BrokerError> {
        if !is_lower_hex(&config.session_nonce, 64)
            || config.session_expires_at_ms <= current_unix_ms()?
        {
            return Err(BrokerError::InvalidSession);
        }
        let core_verifying_key = VerifyingKey::from_bytes(&config.enrolled_core_verifying_key)
            .map_err(|_| BrokerError::Crypto)?;
        let core_key_id = sha256_hex(core_verifying_key.as_bytes());
        let broker_signing_key = SigningKey::from_bytes(&config.broker_signing_seed);
        let broker_key_id = sha256_hex(broker_signing_key.verifying_key().as_bytes());
        let workspace = Workspace::open(&config.protected_root)?;
        let operation_lock = acquire_operation_lock(&config.protected_root)?;
        let journal = Journal::open(
            &config.protected_root,
            config.authenticated_audit_euid,
            &core_key_id,
            &broker_key_id,
        )?;
        Ok(Self {
            authenticated_audit_euid: config.authenticated_audit_euid,
            core_key_id,
            core_verifying_key,
            broker_key_id,
            broker_signing_key,
            session_nonce: config.session_nonce,
            session_expires_at_ms: config.session_expires_at_ms,
            workspace,
            journal,
            _operation_lock: operation_lock,
        })
    }

    #[must_use]
    pub fn broker_session(&self) -> EffectBrokerSession {
        EffectBrokerSession {
            protocol_version: EFFECT_PROTOCOL_VERSION,
            session_nonce: self.session_nonce.clone(),
            broker_key_id: self.broker_key_id.clone(),
            broker_verifying_key_hex: hex::encode(
                self.broker_signing_key.verifying_key().to_bytes(),
            ),
            expires_at_ms: self.session_expires_at_ms,
        }
    }

    #[must_use]
    pub const fn authenticated_audit_euid(&self) -> u32 {
        self.authenticated_audit_euid
    }

    /// Applies one signed `PutFile` using streaming payload validation.
    ///
    /// # Errors
    ///
    /// Fails closed for an invalid permit, changed payload, conflicting effect
    /// id, journal inconsistency, or path/identity change.
    pub fn put_file(
        &mut self,
        permit: &EffectPermit,
        payload: impl Read,
    ) -> Result<EffectReceipt, BrokerError> {
        self.put_file_with_completion_clock(permit, payload, current_unix_ms()?, current_unix_ms)
    }

    #[cfg(test)]
    fn put_file_at(
        &mut self,
        permit: &EffectPermit,
        payload: impl Read,
        now_ms: i64,
    ) -> Result<EffectReceipt, BrokerError> {
        self.put_file_with_completion_clock(permit, payload, now_ms, || {
            Ok(current_unix_ms()?.max(now_ms))
        })
    }

    fn put_file_with_completion_clock(
        &mut self,
        permit: &EffectPermit,
        mut payload: impl Read,
        now_ms: i64,
        completion_clock: impl FnOnce() -> Result<i64, BrokerError>,
    ) -> Result<EffectReceipt, BrokerError> {
        let validated = self.validate_permit(permit, now_ms)?;
        if permit.purpose == EffectPermitPurpose::Reconcile {
            return Err(BrokerError::InvalidPermit);
        }
        let permit_hash = effect_permit_hash(permit).map_err(|_| BrokerError::Crypto)?;
        if permit.purpose == EffectPermitPurpose::ReattestOnly {
            let entry = self.load_existing_effect(permit, &validated)?;
            if !matches!(
                entry.state,
                EffectState::FilesystemCommitted | EffectState::ReceiptCommitted
            ) {
                return Err(BrokerError::EffectNotCommitted);
            }
            self.bind_workspace_for_purpose(
                &permit.command.mission_id,
                EffectPermitPurpose::ReattestOnly,
            )?;
            validate_supplied_payload(&validated.payload, &mut payload)?;
            self.verify_completed_workspace_read_only(&entry, &validated)?;
            let attested_at_ms = permit
                .issued_at_ms
                .max(entry.completed_at_ms.ok_or(BrokerError::JournalMismatch)?);
            self.validate_active_instant(permit, attested_at_ms)?;
            return self.sign_receipt(
                &entry,
                &permit_hash,
                &permit.broker_session_nonce,
                attested_at_ms,
            );
        }

        let entry = self.journal.accept(
            &permit.command,
            &permit.stable_effect_hash,
            &validated.payload,
            &validated.path_components,
        )?;
        self.bind_workspace_for_purpose(&permit.command.mission_id, EffectPermitPurpose::Execute)?;
        let (entry, payload_consumed, attested_at_ms) = if entry.state == EffectState::Accepted {
            if entry.write_started {
                // Once a staged inode exists, only a reconciliation permit may
                // classify the crash state. Execute never guesses or cleans it.
                return Err(BrokerError::EffectNotCommitted);
            }
            let committed = self.commit_accepted_write(
                &entry,
                permit,
                &validated,
                &mut payload,
                completion_clock,
            )?;
            let completed_at_ms = committed
                .completed_at_ms
                .ok_or(BrokerError::JournalMismatch)?;
            (committed, true, completed_at_ms)
        } else {
            let attested_at_ms = completion_clock()?;
            self.validate_active_instant(permit, attested_at_ms)?;
            (entry, false, attested_at_ms)
        };
        if !matches!(
            entry.state,
            EffectState::FilesystemCommitted | EffectState::ReceiptCommitted
        ) {
            return Err(BrokerError::JournalMismatch);
        }
        if !payload_consumed {
            validate_supplied_payload(&validated.payload, &mut payload)?;
        }
        self.verify_completed_workspace_read_only(&entry, &validated)?;
        if entry.state == EffectState::ReceiptCommitted {
            let stored = self.verified_stored_receipt(&entry)?;
            if stored.permit_hash == permit_hash
                && stored.broker_session_nonce == permit.broker_session_nonce
                && stored.attested_at_ms >= permit.issued_at_ms
                && stored.attested_at_ms <= permit.expires_at_ms
            {
                return Ok(stored);
            }
        }
        let receipt = self.sign_receipt(
            &entry,
            &permit_hash,
            &permit.broker_session_nonce,
            attested_at_ms,
        )?;
        if entry.state == EffectState::FilesystemCommitted {
            self.journal.commit_receipt(
                &permit.command.effect_id,
                &permit.stable_effect_hash,
                &receipt,
            )?;
            let committed = self
                .journal
                .load(&permit.command.effect_id)?
                .ok_or(BrokerError::JournalMismatch)?;
            self.verified_stored_receipt(&committed)
        } else {
            Ok(receipt)
        }
    }

    fn commit_accepted_write(
        &mut self,
        entry: &JournalEntry,
        permit: &EffectPermit,
        validated: &ValidatedPut,
        payload: impl Read,
        completion_clock: impl FnOnce() -> Result<i64, BrokerError>,
    ) -> Result<JournalEntry, BrokerError> {
        if !entry.write_started {
            self.workspace.cleanup_stage(
                &permit.command.mission_id,
                &validated.path_components,
                &entry.stage_name,
            )?;
        }
        let staged_identity = self.workspace.prepare_stage(
            &permit.command.mission_id,
            &validated.path_components,
            &entry.stage_name,
        )?;
        self.journal.mark_stage_identity(
            &permit.command.effect_id,
            &permit.stable_effect_hash,
            staged_identity.device,
            staged_identity.inode,
        )?;
        let journal = &self.journal;
        let session_expires_at_ms = self.session_expires_at_ms;
        let committed = self.workspace.write_atomically(
            &permit.command.mission_id,
            &validated.path_components,
            &validated.payload,
            &entry.stage_name,
            payload,
            || {
                let commit_at_ms = current_unix_ms()?;
                Self::validate_commit_instant(permit, session_expires_at_ms, commit_at_ms)?;
                journal.mark_commit_intent(
                    &permit.command.effect_id,
                    &permit.stable_effect_hash,
                    commit_at_ms,
                    &permit.broker_session_nonce,
                )
            },
        )?;
        Self::require_committed_matches(&committed, &validated.payload)?;
        let completed_at_ms = completion_clock()?;
        self.validate_active_instant(permit, completed_at_ms)?;
        self.journal.mark_filesystem_committed(
            &permit.command.effect_id,
            &permit.stable_effect_hash,
            completed_at_ms,
            &permit.broker_session_nonce,
        )?;
        self.journal
            .load(&permit.command.effect_id)?
            .ok_or(BrokerError::JournalMismatch)
    }

    fn verify_completed_workspace_read_only(
        &self,
        entry: &JournalEntry,
        validated: &ValidatedPut,
    ) -> Result<(), BrokerError> {
        let expected_identity = workspace::FileIdentity {
            device: entry.stage_device.ok_or(BrokerError::JournalMismatch)?,
            inode: entry.stage_inode.ok_or(BrokerError::JournalMismatch)?,
        };
        let state = self.workspace.inspect_effect_read_only(
            &entry.mission_id,
            &validated.path_components,
            &entry.stage_name,
        )?;
        let final_file = state.final_file.ok_or(BrokerError::WorkspaceBoundary)?;
        if state.stage_directory_exists
            || state.stage_payload.is_some()
            || final_file.identity != expected_identity
            || final_file.content.sha256 != validated.payload.sha256
            || final_file.content.byte_len != validated.payload.byte_len
        {
            return Err(BrokerError::WorkspaceBoundary);
        }
        Ok(())
    }

    /// Reconciles one unresolved effect without authorizing a fresh write.
    /// A post-rename staged inode becomes a committed Receipt; a provably
    /// pre-rename stage is scrubbed and permanently tombstoned as noncommit.
    ///
    /// # Errors
    ///
    /// Returns an error for a non-reconciliation permit, ambiguous or changed
    /// filesystem identity, corrupt journal, expired authority, or I/O failure.
    pub fn reconcile_effect(
        &mut self,
        permit: &EffectPermit,
    ) -> Result<EffectReconciliation, BrokerError> {
        self.reconcile_effect_with_clock(permit, current_unix_ms()?, current_unix_ms)
    }

    fn reconcile_effect_with_clock(
        &mut self,
        permit: &EffectPermit,
        now_ms: i64,
        result_clock: impl FnOnce() -> Result<i64, BrokerError>,
    ) -> Result<EffectReconciliation, BrokerError> {
        let validated = self.validate_permit(permit, now_ms)?;
        if permit.purpose != EffectPermitPurpose::Reconcile {
            return Err(BrokerError::InvalidPermit);
        }
        let permit_hash = effect_permit_hash(permit).map_err(|_| BrokerError::Crypto)?;
        let entry = match self.journal.load(&permit.command.effect_id)? {
            Some(entry) => entry,
            None => self.journal.accept(
                &permit.command,
                &permit.stable_effect_hash,
                &validated.payload,
                &validated.path_components,
            )?,
        };
        Self::require_entry_matches(&entry, permit, &validated)?;
        if entry.state == EffectState::NotCommitted {
            let stored = self.verified_stored_noncommit(&entry)?;
            if stored.permit_hash == permit_hash
                && stored.broker_session_nonce == permit.broker_session_nonce
            {
                return Ok(EffectReconciliation::NotCommitted {
                    attestation: stored,
                });
            }
            let reconciled_at_ms = result_clock()?;
            self.validate_active_instant(permit, reconciled_at_ms)?;
            let attestation =
                self.sign_noncommit(&entry, &permit_hash, reconciled_at_ms, permit)?;
            self.journal.refresh_not_committed(
                &entry.effect_id,
                &entry.stable_effect_hash,
                &attestation,
            )?;
            let refreshed = self
                .journal
                .load(&entry.effect_id)?
                .ok_or(BrokerError::JournalMismatch)?;
            return Ok(EffectReconciliation::NotCommitted {
                attestation: self.verified_stored_noncommit(&refreshed)?,
            });
        }
        if matches!(
            entry.state,
            EffectState::FilesystemCommitted | EffectState::ReceiptCommitted
        ) {
            self.bind_workspace_for_purpose(&entry.mission_id, EffectPermitPurpose::Reconcile)?;
            self.verify_completed_workspace_read_only(&entry, &validated)?;
            let attested_at_ms = permit
                .issued_at_ms
                .max(entry.completed_at_ms.ok_or(BrokerError::JournalMismatch)?);
            self.validate_active_instant(permit, attested_at_ms)?;
            let receipt = self.sign_receipt(
                &entry,
                &permit_hash,
                &permit.broker_session_nonce,
                attested_at_ms,
            )?;
            return Ok(EffectReconciliation::Committed { receipt });
        }
        if entry.state != EffectState::Accepted {
            return Err(BrokerError::JournalMismatch);
        }
        self.reconcile_accepted(entry, permit, &validated, &permit_hash, result_clock)
    }

    fn reconcile_accepted(
        &mut self,
        mut entry: JournalEntry,
        permit: &EffectPermit,
        validated: &ValidatedPut,
        permit_hash: &str,
        result_clock: impl FnOnce() -> Result<i64, BrokerError>,
    ) -> Result<EffectReconciliation, BrokerError> {
        let Some((workspace_device, workspace_inode)) =
            self.journal.workspace_identity(&entry.mission_id)?
        else {
            if entry.write_started {
                return Err(BrokerError::JournalMismatch);
            }
            let reconciled_at_ms = result_clock()?;
            self.validate_active_instant(permit, reconciled_at_ms)?;
            let attestation = self.sign_noncommit(&entry, permit_hash, reconciled_at_ms, permit)?;
            self.journal.mark_not_committed(
                &entry.effect_id,
                &entry.stable_effect_hash,
                &attestation,
            )?;
            return Ok(EffectReconciliation::NotCommitted { attestation });
        };
        self.workspace.require_workspace_identity(
            &entry.mission_id,
            workspace::FileIdentity {
                device: workspace_device,
                inode: workspace_inode,
            },
        )?;
        let state = self.workspace.inspect_effect_read_only(
            &entry.mission_id,
            &validated.path_components,
            &entry.stage_name,
        )?;
        let staged_identity = match (entry.stage_device, entry.stage_inode) {
            (Some(device), Some(inode)) => Some(workspace::FileIdentity { device, inode }),
            (None, None) => None,
            _ => return Err(BrokerError::JournalMismatch),
        };
        if state.final_file.is_some() {
            let expected_identity = staged_identity.ok_or(BrokerError::WorkspaceBoundary)?;
            if !self.workspace.finalize_owned_commit(
                &entry.mission_id,
                &validated.path_components,
                &validated.payload,
                &entry.stage_name,
                expected_identity,
            )? {
                return Err(BrokerError::WorkspaceBoundary);
            }
            let completed_at_ms = result_clock()?;
            self.validate_active_instant(permit, completed_at_ms)?;
            self.journal.mark_filesystem_committed(
                &entry.effect_id,
                &entry.stable_effect_hash,
                completed_at_ms,
                &permit.broker_session_nonce,
            )?;
            entry = self
                .journal
                .load(&entry.effect_id)?
                .ok_or(BrokerError::JournalMismatch)?;
            let receipt = self.sign_receipt(
                &entry,
                permit_hash,
                &permit.broker_session_nonce,
                completed_at_ms,
            )?;
            return Ok(EffectReconciliation::Committed { receipt });
        }
        if let (Some(expected), Some(actual)) = (staged_identity, state.stage_payload.as_ref())
            && expected != actual.identity
        {
            return Err(BrokerError::WorkspaceBoundary);
        }
        self.workspace.discard_owned_stage(
            &entry.mission_id,
            &validated.path_components,
            &entry.stage_name,
            staged_identity,
        )?;
        let reconciled_at_ms = result_clock()?;
        self.validate_active_instant(permit, reconciled_at_ms)?;
        let attestation = self.sign_noncommit(&entry, permit_hash, reconciled_at_ms, permit)?;
        self.journal.mark_not_committed(
            &entry.effect_id,
            &entry.stable_effect_hash,
            &attestation,
        )?;
        Ok(EffectReconciliation::NotCommitted { attestation })
    }

    #[cfg(test)]
    fn bind_workspace(&self, mission_id: &str) -> Result<(), BrokerError> {
        self.bind_workspace_for_purpose(mission_id, EffectPermitPurpose::Execute)
    }

    fn bind_workspace_for_purpose(
        &self,
        mission_id: &str,
        purpose: EffectPermitPurpose,
    ) -> Result<(), BrokerError> {
        match purpose {
            EffectPermitPurpose::Execute => {
                let identity = self.workspace.ensure_workspace(mission_id)?;
                self.journal
                    .bind_workspace(mission_id, identity.device, identity.inode)?;
                self.workspace
                    .require_workspace_identity(mission_id, identity)
            }
            EffectPermitPurpose::ReattestOnly | EffectPermitPurpose::Reconcile => {
                let (device, inode) = self
                    .journal
                    .workspace_identity(mission_id)?
                    .ok_or(BrokerError::EffectNotCommitted)?;
                self.workspace.require_workspace_identity(
                    mission_id,
                    workspace::FileIdentity { device, inode },
                )
            }
        }
    }

    fn load_existing_effect(
        &self,
        permit: &EffectPermit,
        validated: &ValidatedPut,
    ) -> Result<JournalEntry, BrokerError> {
        let entry = self
            .journal
            .load(&permit.command.effect_id)?
            .ok_or(BrokerError::EffectNotCommitted)?;
        Self::require_entry_matches(&entry, permit, validated)?;
        Ok(entry)
    }

    fn require_entry_matches(
        entry: &JournalEntry,
        permit: &EffectPermit,
        validated: &ValidatedPut,
    ) -> Result<(), BrokerError> {
        if entry.stable_effect_hash != permit.stable_effect_hash
            || entry.mission_id != permit.command.mission_id
            || entry.path_components != validated.path_components
            || entry.payload_sha256 != validated.payload.sha256
            || entry.payload_byte_len != validated.payload.byte_len
        {
            return Err(BrokerError::EffectConflict);
        }
        Ok(())
    }

    fn validate_permit(
        &self,
        permit: &EffectPermit,
        now_ms: i64,
    ) -> Result<ValidatedPut, BrokerError> {
        validate_command(&permit.command)?;
        let expected_hash = sha256_hex(
            &effect_command_signing_bytes(&permit.command).map_err(|_| BrokerError::Crypto)?,
        );
        if permit.stable_effect_hash != expected_hash
            || permit.core_key_id != self.core_key_id
            || permit.authorization_anchor.sequence
                != permit.command.source_anchor.sequence.saturating_add(1)
            || !is_lower_hex(&permit.authorization_anchor.entry_hash, 64)
            || !is_lower_hex(&permit.authorization_anchor.signature_hex, 128)
            || permit.broker_session_nonce != self.session_nonce
            || permit.issued_at_ms > now_ms
            || permit.expires_at_ms <= now_ms
            || permit.expires_at_ms > self.session_expires_at_ms
            || permit.expires_at_ms <= permit.issued_at_ms
            || permit.expires_at_ms - permit.issued_at_ms > MAX_PERMIT_TTL_MS
        {
            return Err(BrokerError::InvalidPermit);
        }
        let signature_bytes =
            hex::decode(&permit.authorization_signature_hex).map_err(|_| BrokerError::Crypto)?;
        let signature = Signature::from_slice(&signature_bytes).map_err(|_| BrokerError::Crypto)?;
        let bytes = effect_permit_signing_bytes(permit).map_err(|_| BrokerError::Crypto)?;
        self.core_verifying_key
            .verify(&bytes, &signature)
            .map_err(|_| BrokerError::InvalidPermit)?;
        let MissionFileEffect::PutFile {
            path_components,
            payload,
            ..
        } = &permit.command.effect;
        Ok(ValidatedPut {
            path_components: path_components.clone(),
            payload: payload.clone(),
        })
    }

    fn require_committed_matches(
        committed: &CommittedFile,
        expected: &PayloadDescriptor,
    ) -> Result<(), BrokerError> {
        if committed.sha256 != expected.sha256 || committed.byte_len != expected.byte_len {
            Err(BrokerError::PayloadMismatch)
        } else {
            Ok(())
        }
    }

    fn validate_commit_instant(
        permit: &EffectPermit,
        session_expires_at_ms: i64,
        commit_at_ms: i64,
    ) -> Result<(), BrokerError> {
        if commit_at_ms < permit.issued_at_ms
            || commit_at_ms >= permit.expires_at_ms
            || commit_at_ms >= session_expires_at_ms
        {
            Err(BrokerError::InvalidPermit)
        } else {
            Ok(())
        }
    }

    fn validate_active_instant(
        &self,
        permit: &EffectPermit,
        instant_ms: i64,
    ) -> Result<(), BrokerError> {
        Self::validate_commit_instant(permit, self.session_expires_at_ms, instant_ms)
    }

    fn sign_receipt(
        &self,
        entry: &JournalEntry,
        permit_hash: &str,
        session_nonce: &str,
        attested_at_ms: i64,
    ) -> Result<EffectReceipt, BrokerError> {
        let committed_at_ms = entry.completed_at_ms.ok_or(BrokerError::JournalMismatch)?;
        if attested_at_ms < committed_at_ms {
            return Err(BrokerError::JournalMismatch);
        }
        let mut receipt = EffectReceipt {
            protocol_version: EFFECT_PROTOCOL_VERSION,
            effect_id: entry.effect_id.clone(),
            stable_effect_hash: entry.stable_effect_hash.clone(),
            permit_hash: permit_hash.to_owned(),
            mission_id: entry.mission_id.clone(),
            path_components: entry.path_components.clone(),
            payload_sha256: entry.payload_sha256.clone(),
            payload_byte_len: entry.payload_byte_len,
            broker_session_nonce: session_nonce.to_owned(),
            committed_at_ms,
            attested_at_ms,
            broker_key_id: self.broker_key_id.clone(),
            broker_signature_hex: String::new(),
        };
        let bytes = effect_receipt_signing_bytes(&receipt).map_err(|_| BrokerError::Crypto)?;
        receipt.broker_signature_hex = hex::encode(self.broker_signing_key.sign(&bytes).to_bytes());
        Ok(receipt)
    }

    fn verified_stored_receipt(&self, entry: &JournalEntry) -> Result<EffectReceipt, BrokerError> {
        let receipt = entry.receipt.clone().ok_or(BrokerError::JournalMismatch)?;
        if receipt.effect_id != entry.effect_id
            || receipt.protocol_version != EFFECT_PROTOCOL_VERSION
            || receipt.stable_effect_hash != entry.stable_effect_hash
            || !is_lower_hex(&receipt.permit_hash, 64)
            || receipt.mission_id != entry.mission_id
            || receipt.path_components != entry.path_components
            || receipt.payload_sha256 != entry.payload_sha256
            || receipt.payload_byte_len != entry.payload_byte_len
            || receipt.broker_key_id != self.broker_key_id
            || Some(receipt.committed_at_ms) != entry.completed_at_ms
            || receipt.attested_at_ms < receipt.committed_at_ms
            || Some(&receipt.broker_session_nonce) != entry.committed_session_nonce.as_ref()
        {
            return Err(BrokerError::JournalMismatch);
        }
        let bytes = effect_receipt_signing_bytes(&receipt).map_err(|_| BrokerError::Crypto)?;
        let signature_bytes =
            hex::decode(&receipt.broker_signature_hex).map_err(|_| BrokerError::Crypto)?;
        let signature = Signature::from_slice(&signature_bytes).map_err(|_| BrokerError::Crypto)?;
        self.broker_signing_key
            .verifying_key()
            .verify(&bytes, &signature)
            .map_err(|_| BrokerError::JournalMismatch)?;
        Ok(receipt)
    }

    fn sign_noncommit(
        &self,
        entry: &JournalEntry,
        permit_hash: &str,
        reconciled_at_ms: i64,
        permit: &EffectPermit,
    ) -> Result<EffectNonCommit, BrokerError> {
        let mut attestation = EffectNonCommit {
            protocol_version: EFFECT_PROTOCOL_VERSION,
            effect_id: entry.effect_id.clone(),
            stable_effect_hash: entry.stable_effect_hash.clone(),
            permit_hash: permit_hash.to_owned(),
            mission_id: entry.mission_id.clone(),
            broker_session_nonce: permit.broker_session_nonce.clone(),
            reconciled_at_ms,
            broker_key_id: self.broker_key_id.clone(),
            broker_signature_hex: String::new(),
        };
        let bytes =
            effect_noncommit_signing_bytes(&attestation).map_err(|_| BrokerError::Crypto)?;
        attestation.broker_signature_hex =
            hex::encode(self.broker_signing_key.sign(&bytes).to_bytes());
        Ok(attestation)
    }

    fn verified_stored_noncommit(
        &self,
        entry: &JournalEntry,
    ) -> Result<EffectNonCommit, BrokerError> {
        let attestation = entry
            .noncommit
            .clone()
            .ok_or(BrokerError::JournalMismatch)?;
        if entry.state != EffectState::NotCommitted
            || attestation.effect_id != entry.effect_id
            || attestation.protocol_version != EFFECT_PROTOCOL_VERSION
            || attestation.stable_effect_hash != entry.stable_effect_hash
            || !is_lower_hex(&attestation.permit_hash, 64)
            || attestation.mission_id != entry.mission_id
            || attestation.broker_key_id != self.broker_key_id
            || Some(attestation.reconciled_at_ms) != entry.reconciled_at_ms
        {
            return Err(BrokerError::JournalMismatch);
        }
        let bytes =
            effect_noncommit_signing_bytes(&attestation).map_err(|_| BrokerError::Crypto)?;
        let signature_bytes =
            hex::decode(&attestation.broker_signature_hex).map_err(|_| BrokerError::Crypto)?;
        let signature = Signature::from_slice(&signature_bytes).map_err(|_| BrokerError::Crypto)?;
        self.broker_signing_key
            .verifying_key()
            .verify(&bytes, &signature)
            .map_err(|_| BrokerError::JournalMismatch)?;
        Ok(attestation)
    }
}

fn validate_supplied_payload(
    expected: &PayloadDescriptor,
    mut payload: impl Read,
) -> Result<(), BrokerError> {
    let mut hasher = Sha256::new();
    let mut byte_len = 0_u64;
    let mut buffer = vec![0_u8; PAYLOAD_VALIDATION_CHUNK_BYTES].into_boxed_slice();
    loop {
        let count = payload.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        byte_len = byte_len
            .checked_add(u64::try_from(count).map_err(|_| BrokerError::PayloadMismatch)?)
            .ok_or(BrokerError::PayloadMismatch)?;
        if byte_len > expected.byte_len {
            return Err(BrokerError::PayloadMismatch);
        }
        hasher.update(&buffer[..count]);
    }
    if byte_len != expected.byte_len || hex::encode(hasher.finalize()) != expected.sha256 {
        return Err(BrokerError::PayloadMismatch);
    }
    Ok(())
}

/// Returns the sole production Mission root for an authenticated XPC audit
/// EUID. The EUID must come from the peer credential, never request JSON.
#[must_use]
pub fn protected_root_for_audit_euid(audit_euid: u32) -> PathBuf {
    PathBuf::from(PROTECTED_USERS_ROOT)
        .join(audit_euid.to_string())
        .join("Missions")
}

struct ValidatedPut {
    path_components: Vec<String>,
    payload: PayloadDescriptor,
}

fn validate_command(command: &EffectCommand) -> Result<(), BrokerError> {
    let MissionFileEffect::PutFile {
        path_components,
        payload,
        action_digest,
    } = &command.effect;
    let approval_ids = command.approval_ids.iter().collect::<HashSet<_>>();
    if command.protocol_version != EFFECT_PROTOCOL_VERSION
        || !is_canonical_effect_identifier(&command.effect_id)
        || !is_canonical_effect_identifier(&command.mission_id)
        || command.mission_updated_at_ms < 0
        || command.mission_scope_digest.is_empty()
        || command.mission_scope_digest.len() > MAX_EFFECT_SCOPE_DIGEST_BYTES
        || command.source_anchor.sequence <= 0
        || !is_lower_hex(&command.source_anchor.entry_hash, 64)
        || !is_lower_hex(&command.source_anchor.signature_hex, 128)
        || command.approval_ids.is_empty()
        || command.approval_ids.len() > MAX_EFFECT_APPROVAL_IDS
        || approval_ids.len() != command.approval_ids.len()
        || command
            .approval_ids
            .iter()
            .any(|id| !is_canonical_effect_identifier(id))
        || path_components.is_empty()
        || path_components.len() > MAX_PATH_COMPONENTS
        || path_components
            .iter()
            .any(|component| !is_canonical_path_component(component))
        || !is_lower_hex(&payload.sha256, 64)
        || payload.byte_len > MAX_EFFECT_PAYLOAD_BYTES
        || !is_lower_hex(action_digest, 64)
    {
        return Err(BrokerError::InvalidCommand);
    }
    Ok(())
}

fn is_canonical_path_component(value: &str) -> bool {
    !value.is_empty()
        && value != "."
        && value != ".."
        && value.len() <= MAX_PATH_COMPONENT_BYTES
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"._-".contains(&byte)
        })
}

fn is_lower_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn current_unix_ms() -> Result<i64, BrokerError> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| BrokerError::InvalidSystemTime)?;
    i64::try_from(duration.as_millis()).map_err(|_| BrokerError::InvalidSystemTime)
}

fn acquire_operation_lock(root: &Path) -> Result<OwnedFd, BrokerError> {
    let path = root.join(OPERATION_LOCK_NAME);
    let lock = open(
        &path,
        OFlags::RDWR | OFlags::CREATE | OFlags::CLOEXEC | OFlags::NOFOLLOW,
        Mode::RUSR | Mode::WUSR,
    )
    .map_err(|_| BrokerError::InvalidRoot)?;
    fchmod(&lock, Mode::RUSR | Mode::WUSR).map_err(|_| BrokerError::InvalidRoot)?;
    let stat = fstat(&lock).map_err(|_| BrokerError::InvalidRoot)?;
    let metadata = std::fs::symlink_metadata(&path).map_err(|_| BrokerError::InvalidRoot)?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.uid() != rustix::process::geteuid().as_raw()
        || metadata.permissions().mode() & 0o777 != 0o600
        || metadata.dev() != u64::try_from(stat.st_dev).map_err(|_| BrokerError::InvalidRoot)?
        || metadata.ino() != stat.st_ino
        || FileType::from_raw_mode(stat.st_mode) != FileType::RegularFile
        || stat.st_uid != rustix::process::geteuid().as_raw()
        || stat.st_mode & 0o777 != 0o600
        || stat.st_nlink != 1
    {
        return Err(BrokerError::InvalidRoot);
    }
    flock(&lock, FlockOperation::NonBlockingLockExclusive)
        .map_err(|_| BrokerError::OperationBusy)?;
    Ok(lock)
}

#[cfg(test)]
mod tests;
