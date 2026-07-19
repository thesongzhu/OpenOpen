use crate::LocalAuthority;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use openopen_protocol::{
    CoreInstanceLease, EFFECT_PROTOCOL_VERSION, EffectAuditAnchor, EffectBrokerSession,
    EffectCommand, EffectNonCommit, EffectPermit, EffectPermitPurpose, EffectReceipt,
    RuntimeControlAuthorization, RuntimeControlReceipt, core_instance_lease_signing_bytes,
    effect_command_signing_bytes, effect_noncommit_signing_bytes, effect_permit_hash,
    effect_receipt_signing_bytes, runtime_control_authorization_hash,
    runtime_control_receipt_signing_bytes,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

pub(crate) const EFFECT_PERMIT_TTL_MS: i64 = 30_000;

#[derive(Clone, Copy)]
pub(crate) struct RuntimePermitContext {
    pub revision: u64,
    pub now_ms: i64,
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum EffectProtocolError {
    #[error("effect protocol version is unsupported")]
    UnsupportedVersion,
    #[error("effect broker session is malformed or expired")]
    InvalidSession,
    #[error("trusted effect-broker enrollment is malformed")]
    InvalidEnrollment,
    #[error("effect broker session is not the independently enrolled broker")]
    UntrustedBroker,
    #[error("effect command canonicalization failed")]
    Canonicalization,
    #[error("effect Receipt does not match the expected effect")]
    ReceiptMismatch,
    #[error("effect Receipt signature is malformed")]
    InvalidReceiptSignature,
    #[error("effect Receipt signature verification failed")]
    ReceiptSignatureFailed,
    #[error("effect noncommit attestation does not match the reconciliation permit")]
    NonCommitMismatch,
    #[error("effect noncommit signature is malformed")]
    InvalidNonCommitSignature,
    #[error("effect noncommit signature verification failed")]
    NonCommitSignatureFailed,
    #[error("runtime-control broker Receipt does not match the Core authorization")]
    RuntimeControlMismatch,
    #[error("runtime-control broker Receipt signature is invalid")]
    RuntimeControlSignatureFailed,
    #[error("Core instance lease does not match this process incarnation")]
    CoreInstanceLeaseMismatch,
    #[error("Core instance lease broker signature is invalid")]
    CoreInstanceLeaseSignatureFailed,
    #[error("effect authorization signing failed: {0}")]
    AuthorizationSigning(String),
}

/// Verifies that a lease was signed by the independently enrolled broker.
/// Process-incarnation fields are checked by the Host before it installs the
/// lease; this function proves the protected broker authorized the exact
/// immutable record.
///
/// # Errors
///
/// Returns an error when the lease shape, enrolled broker binding, canonical
/// preimage, or broker signature is invalid.
pub fn verify_core_instance_lease(
    enrollment: &TrustedBrokerEnrollment,
    lease: &CoreInstanceLease,
) -> Result<(), EffectProtocolError> {
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
        || lease.issued_at_ms <= 0
        || !is_lower_hex(&lease.core_instance_nonce, 64)
        || lease.broker_key_id != enrollment.broker_key_id()
    {
        return Err(EffectProtocolError::CoreInstanceLeaseMismatch);
    }
    let raw_key = hex::decode(enrollment.broker_verifying_key_hex())
        .map_err(|_| EffectProtocolError::InvalidEnrollment)?;
    let key_bytes: [u8; 32] = raw_key
        .try_into()
        .map_err(|_| EffectProtocolError::InvalidEnrollment)?;
    let key =
        VerifyingKey::from_bytes(&key_bytes).map_err(|_| EffectProtocolError::InvalidEnrollment)?;
    let raw_signature = hex::decode(&lease.broker_signature_hex)
        .map_err(|_| EffectProtocolError::CoreInstanceLeaseSignatureFailed)?;
    let signature = Signature::from_slice(&raw_signature)
        .map_err(|_| EffectProtocolError::CoreInstanceLeaseSignatureFailed)?;
    let bytes = core_instance_lease_signing_bytes(lease)
        .map_err(|_| EffectProtocolError::Canonicalization)?;
    key.verify(&bytes, &signature)
        .map_err(|_| EffectProtocolError::CoreInstanceLeaseSignatureFailed)
}

pub(crate) fn verify_runtime_control_receipt(
    enrollment: &TrustedBrokerEnrollment,
    authorization: &RuntimeControlAuthorization,
    receipt: &RuntimeControlReceipt,
) -> Result<(), EffectProtocolError> {
    if receipt.protocol_version != EFFECT_PROTOCOL_VERSION
        || receipt.broker_key_id != enrollment.broker_key_id()
        || !is_lower_hex(&receipt.checkpoint_nonce, 64)
        || receipt
            .request_nonce
            .as_ref()
            .is_some_and(|nonce| !is_lower_hex(nonce, 64))
        || receipt.authorization_hash
            != runtime_control_authorization_hash(authorization)
                .map_err(|_| EffectProtocolError::Canonicalization)?
    {
        return Err(EffectProtocolError::RuntimeControlMismatch);
    }
    let raw_key = hex::decode(enrollment.broker_verifying_key_hex())
        .map_err(|_| EffectProtocolError::InvalidSession)?;
    let key_bytes: [u8; 32] = raw_key
        .try_into()
        .map_err(|_| EffectProtocolError::InvalidSession)?;
    let key =
        VerifyingKey::from_bytes(&key_bytes).map_err(|_| EffectProtocolError::InvalidSession)?;
    let raw_signature = hex::decode(&receipt.broker_signature_hex)
        .map_err(|_| EffectProtocolError::RuntimeControlSignatureFailed)?;
    let signature = Signature::from_slice(&raw_signature)
        .map_err(|_| EffectProtocolError::RuntimeControlSignatureFailed)?;
    let bytes = runtime_control_receipt_signing_bytes(receipt)
        .map_err(|_| EffectProtocolError::Canonicalization)?;
    key.verify(&bytes, &signature)
        .map_err(|_| EffectProtocolError::RuntimeControlSignatureFailed)
}

/// Immutable trust material provisioned only after the signed privileged
/// helper has been installed and authenticated outside the request path.
///
/// Core never learns a broker key from an effect session. The host must load
/// this enrollment from its protected installation record after verifying the
/// helper's exact designated requirement.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrustedBrokerEnrollment {
    broker_key_id: String,
    broker_verifying_key_hex: String,
    helper_designated_requirement_digest: String,
    installed_at_ms: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrokerEnrollmentRecord {
    pub version: u32,
    pub broker_key_id: String,
    pub broker_verifying_key_hex: String,
    pub helper_designated_requirement_digest: String,
    pub installed_at_ms: i64,
    pub core_key_id: String,
    pub core_authorization_signature_hex: String,
}

/// Canonical broker enrollment record preimage. Rust Core owns the signing
/// key and signs this public trust anchor before installation.
///
/// # Errors
///
/// Returns an error if the typed record cannot be canonicalized.
pub fn broker_enrollment_signing_bytes(
    record: &BrokerEnrollmentRecord,
) -> Result<Vec<u8>, EffectProtocolError> {
    serde_json::to_vec(&serde_json::json!({
        "brokerKeyId": record.broker_key_id,
        "brokerVerifyingKeyHex": record.broker_verifying_key_hex,
        "coreKeyId": record.core_key_id,
        "helperDesignatedRequirementDigest": record.helper_designated_requirement_digest,
        "installedAtMs": record.installed_at_ms,
        "version": record.version,
    }))
    .map_err(|_| EffectProtocolError::Canonicalization)
}

/// Constructs and signs the broker installation record inside Rust Core.
///
/// Swift supplies only the broker's public, code-signing-bound trust anchor;
/// private effect-authority material never crosses back into the app process.
///
/// # Errors
///
/// Returns an error if the typed record cannot be canonicalized.
pub fn authorize_broker_enrollment(
    authority: &LocalAuthority,
    broker_key_id: String,
    broker_verifying_key_hex: String,
    helper_designated_requirement_digest: String,
    installed_at_ms: i64,
) -> Result<BrokerEnrollmentRecord, EffectProtocolError> {
    let mut record = BrokerEnrollmentRecord {
        version: 1,
        broker_key_id,
        broker_verifying_key_hex,
        helper_designated_requirement_digest,
        installed_at_ms,
        core_key_id: authority.effect_key_id(),
        core_authorization_signature_hex: String::new(),
    };
    record.core_authorization_signature_hex =
        authority.sign_effect_bytes(&broker_enrollment_signing_bytes(&record)?);
    TrustedBrokerEnrollment::from_signed_install_record(authority, &record)?;
    Ok(record)
}

impl TrustedBrokerEnrollment {
    /// Constructs validated, pinned broker trust material.
    ///
    /// # Errors
    ///
    /// Returns an error for a malformed key, requirement digest, or timestamp.
    pub fn from_signed_install_record(
        authority: &LocalAuthority,
        record: &BrokerEnrollmentRecord,
    ) -> Result<Self, EffectProtocolError> {
        let broker_verifying_key_hex = record.broker_verifying_key_hex.clone();
        let helper_designated_requirement_digest =
            record.helper_designated_requirement_digest.clone();
        let installed_at_ms = record.installed_at_ms;
        if !is_lower_hex(&broker_verifying_key_hex, 64)
            || !is_lower_hex(&helper_designated_requirement_digest, 64)
            || installed_at_ms <= 0
            || record.version != 1
            || record.core_key_id != authority.effect_key_id()
        {
            return Err(EffectProtocolError::InvalidEnrollment);
        }
        let key = hex::decode(&broker_verifying_key_hex)
            .map_err(|_| EffectProtocolError::InvalidEnrollment)?;
        let key_bytes: [u8; 32] = key
            .try_into()
            .map_err(|_| EffectProtocolError::InvalidEnrollment)?;
        VerifyingKey::from_bytes(&key_bytes).map_err(|_| EffectProtocolError::InvalidEnrollment)?;
        let broker_key_id = format!("{:x}", Sha256::digest(key_bytes));
        if broker_key_id != record.broker_key_id {
            return Err(EffectProtocolError::InvalidEnrollment);
        }
        authority
            .verify_effect_bytes(
                &broker_enrollment_signing_bytes(record)?,
                &record.core_authorization_signature_hex,
            )
            .map_err(|_| EffectProtocolError::InvalidEnrollment)?;
        Ok(Self {
            broker_key_id,
            broker_verifying_key_hex,
            helper_designated_requirement_digest,
            installed_at_ms,
        })
    }

    #[must_use]
    pub fn broker_key_id(&self) -> &str {
        &self.broker_key_id
    }

    #[must_use]
    pub fn broker_verifying_key_hex(&self) -> &str {
        &self.broker_verifying_key_hex
    }

    #[must_use]
    pub fn helper_designated_requirement_digest(&self) -> &str {
        &self.helper_designated_requirement_digest
    }

    #[must_use]
    pub const fn installed_at_ms(&self) -> i64 {
        self.installed_at_ms
    }
}

pub(crate) fn stable_effect_hash(command: &EffectCommand) -> Result<String, EffectProtocolError> {
    let bytes =
        effect_command_signing_bytes(command).map_err(|_| EffectProtocolError::Canonicalization)?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

pub(crate) fn issue_effect_permit(
    authority: &LocalAuthority,
    enrollment: &TrustedBrokerEnrollment,
    command: EffectCommand,
    authorization_anchor: EffectAuditAnchor,
    purpose: EffectPermitPurpose,
    runtime: RuntimePermitContext,
    session: &EffectBrokerSession,
) -> Result<EffectPermit, EffectProtocolError> {
    validate_broker_session(enrollment, session, runtime.now_ms)?;
    let stable_effect_hash = stable_effect_hash(&command)?;
    let expires_at_ms = (runtime.now_ms + EFFECT_PERMIT_TTL_MS).min(session.expires_at_ms);
    if expires_at_ms <= runtime.now_ms {
        return Err(EffectProtocolError::InvalidSession);
    }
    let mut permit = EffectPermit {
        command,
        stable_effect_hash,
        authorization_anchor,
        purpose,
        runtime_revision: runtime.revision,
        broker_session_nonce: session.session_nonce.clone(),
        issued_at_ms: runtime.now_ms,
        expires_at_ms,
        core_key_id: authority.effect_key_id(),
        authorization_signature_hex: String::new(),
    };
    authority
        .sign_effect_permit(&mut permit)
        .map_err(|error| EffectProtocolError::AuthorizationSigning(error.to_string()))?;
    Ok(permit)
}

pub(crate) fn validate_broker_session(
    enrollment: &TrustedBrokerEnrollment,
    session: &EffectBrokerSession,
    now_ms: i64,
) -> Result<(), EffectProtocolError> {
    validate_broker_session_identity(enrollment, session)?;
    if session.expires_at_ms <= now_ms {
        return Err(EffectProtocolError::InvalidSession);
    }
    Ok(())
}

fn validate_broker_session_identity(
    enrollment: &TrustedBrokerEnrollment,
    session: &EffectBrokerSession,
) -> Result<(), EffectProtocolError> {
    if session.protocol_version != EFFECT_PROTOCOL_VERSION
        || !is_lower_hex(&session.session_nonce, 64)
        || !is_lower_hex(&session.broker_verifying_key_hex, 64)
        || !is_lower_hex(&session.broker_key_id, 64)
    {
        return Err(EffectProtocolError::InvalidSession);
    }
    let key = hex::decode(&session.broker_verifying_key_hex)
        .map_err(|_| EffectProtocolError::InvalidSession)?;
    if format!("{:x}", Sha256::digest(key)) != session.broker_key_id {
        return Err(EffectProtocolError::InvalidSession);
    }
    if session.broker_key_id != enrollment.broker_key_id
        || session.broker_verifying_key_hex != enrollment.broker_verifying_key_hex
    {
        return Err(EffectProtocolError::UntrustedBroker);
    }
    Ok(())
}

/// Verifies the broker signature and exact effect/result binding before a
/// Receipt can be persisted or used as Evidence.
///
/// # Errors
///
/// Returns an error for a malformed broker session, changed result fields, or
/// invalid broker signature.
pub fn verify_effect_receipt(
    enrollment: &TrustedBrokerEnrollment,
    session: &EffectBrokerSession,
    permit: &EffectPermit,
    receipt: &EffectReceipt,
) -> Result<(), EffectProtocolError> {
    validate_broker_session_identity(enrollment, session)?;
    let openopen_protocol::MissionFileEffect::PutFile {
        path_components,
        payload,
        ..
    } = &permit.command.effect;
    if stable_effect_hash(&permit.command)? != permit.stable_effect_hash
        || receipt.protocol_version != EFFECT_PROTOCOL_VERSION
        || receipt.effect_id != permit.command.effect_id
        || receipt.stable_effect_hash != permit.stable_effect_hash
        || receipt.permit_hash
            != effect_permit_hash(permit).map_err(|_| EffectProtocolError::Canonicalization)?
        || receipt.mission_id != permit.command.mission_id
        || &receipt.path_components != path_components
        || receipt.payload_sha256 != payload.sha256
        || receipt.payload_byte_len != payload.byte_len
        || receipt.broker_key_id != session.broker_key_id
        || permit.broker_session_nonce != session.session_nonce
        || receipt.broker_session_nonce != permit.broker_session_nonce
        || receipt.committed_at_ms > receipt.attested_at_ms
        || (permit.purpose == EffectPermitPurpose::Execute
            && receipt.committed_at_ms < permit.issued_at_ms)
        || receipt.committed_at_ms >= permit.expires_at_ms
        || receipt.attested_at_ms < permit.issued_at_ms
        || receipt.attested_at_ms > permit.expires_at_ms
    {
        return Err(EffectProtocolError::ReceiptMismatch);
    }
    let raw_key = hex::decode(enrollment.broker_verifying_key_hex())
        .map_err(|_| EffectProtocolError::InvalidSession)?;
    let key_bytes: [u8; 32] = raw_key
        .try_into()
        .map_err(|_| EffectProtocolError::InvalidSession)?;
    let key =
        VerifyingKey::from_bytes(&key_bytes).map_err(|_| EffectProtocolError::InvalidSession)?;
    let raw_signature = hex::decode(&receipt.broker_signature_hex)
        .map_err(|_| EffectProtocolError::InvalidReceiptSignature)?;
    let signature = Signature::from_slice(&raw_signature)
        .map_err(|_| EffectProtocolError::InvalidReceiptSignature)?;
    let bytes =
        effect_receipt_signing_bytes(receipt).map_err(|_| EffectProtocolError::Canonicalization)?;
    key.verify(&bytes, &signature)
        .map_err(|_| EffectProtocolError::ReceiptSignatureFailed)
}

/// Verifies that a trusted broker definitively reconciled one effect as not
/// committed under the exact Core-issued reconciliation permit.
///
/// # Errors
///
/// Returns an error for a changed field, wrong permit purpose, invalid broker
/// session identity, out-of-window reconciliation time, or invalid signature.
pub fn verify_effect_noncommit(
    enrollment: &TrustedBrokerEnrollment,
    session: &EffectBrokerSession,
    permit: &EffectPermit,
    attestation: &EffectNonCommit,
) -> Result<(), EffectProtocolError> {
    validate_broker_session_identity(enrollment, session)?;
    if permit.purpose != EffectPermitPurpose::Reconcile
        || stable_effect_hash(&permit.command)? != permit.stable_effect_hash
        || attestation.protocol_version != EFFECT_PROTOCOL_VERSION
        || attestation.effect_id != permit.command.effect_id
        || attestation.stable_effect_hash != permit.stable_effect_hash
        || attestation.permit_hash
            != effect_permit_hash(permit).map_err(|_| EffectProtocolError::Canonicalization)?
        || attestation.mission_id != permit.command.mission_id
        || permit.broker_session_nonce != session.session_nonce
        || attestation.broker_session_nonce != permit.broker_session_nonce
        || attestation.reconciled_at_ms < permit.issued_at_ms
        || attestation.reconciled_at_ms > permit.expires_at_ms
        || attestation.broker_key_id != session.broker_key_id
    {
        return Err(EffectProtocolError::NonCommitMismatch);
    }
    let raw_key = hex::decode(enrollment.broker_verifying_key_hex())
        .map_err(|_| EffectProtocolError::InvalidSession)?;
    let key_bytes: [u8; 32] = raw_key
        .try_into()
        .map_err(|_| EffectProtocolError::InvalidSession)?;
    let key =
        VerifyingKey::from_bytes(&key_bytes).map_err(|_| EffectProtocolError::InvalidSession)?;
    let raw_signature = hex::decode(&attestation.broker_signature_hex)
        .map_err(|_| EffectProtocolError::InvalidNonCommitSignature)?;
    let signature = Signature::from_slice(&raw_signature)
        .map_err(|_| EffectProtocolError::InvalidNonCommitSignature)?;
    let bytes = effect_noncommit_signing_bytes(attestation)
        .map_err(|_| EffectProtocolError::Canonicalization)?;
    key.verify(&bytes, &signature)
        .map_err(|_| EffectProtocolError::NonCommitSignatureFailed)
}

fn is_lower_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
