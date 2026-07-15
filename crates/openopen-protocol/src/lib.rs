//! Stable local protocol shared by the Swift host and Rust core.
//!
//! The transport is newline-delimited JSON over stdio. This crate contains data
//! only; it performs no external effects and stores no credentials.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutcomeSuggestion {
    pub id: String,
    pub title: String,
    pub why_now: String,
    pub proposed_steps: Vec<String>,
    pub source_refs: Vec<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MissionStatus {
    Proposed,
    AwaitingConfirmation,
    Active,
    NeedsMe,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

impl MissionStatus {
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum WorkItemStatus {
    Pending,
    Active,
    NeedsMe,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkItem {
    pub id: String,
    pub title: String,
    pub status: WorkItemStatus,
    pub evidence_ids: Vec<String>,
    pub pending_approval_id: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ApprovalKind {
    MissionScope,
    ExpandedScope,
    NewRecipient,
    NewDataShare,
    NewExternalWrite,
    Cost,
    DeleteOrIrreversible,
    FinalDecision,
    WorkflowEnable,
    SkillPromotion,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ApprovalStatus {
    Pending,
    Approved,
    Rejected,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalRequest {
    pub id: String,
    pub work_item_id: Option<String>,
    pub kind: ApprovalKind,
    pub prompt: String,
    pub scope_digest: String,
    pub status: ApprovalStatus,
    pub requested_by_id: String,
    pub decided_by_id: Option<String>,
    pub requested_at_ms: i64,
    pub decided_at_ms: Option<i64>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum EvidenceKind {
    ReminderCompleted,
    ParticipantReply,
    OwnerFinalApproval,
    AvailabilityDecisionPublished,
    XlsxVerified,
    FileHash,
    ChannelDelivery,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceRef {
    pub id: String,
    pub mission_id: String,
    pub work_item_id: String,
    pub kind: EvidenceKind,
    pub source_id: String,
    pub sha256: Option<String>,
    pub issuer_id: String,
    pub signature_hex: String,
    pub observed_at_ms: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NeedsMe {
    pub id: String,
    pub prompt: String,
    pub approval_id: Option<String>,
    pub created_at_ms: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Mission {
    pub id: String,
    pub title: String,
    pub outcome: String,
    pub owner_id: String,
    pub scope_digest: String,
    pub status: MissionStatus,
    pub work_items: Vec<WorkItem>,
    pub approvals: Vec<ApprovalRequest>,
    pub needs_me: Option<NeedsMe>,
    pub evidence: Vec<EvidenceRef>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Receipt {
    pub id: String,
    pub mission_id: String,
    pub summary: String,
    pub actual_model: String,
    pub evidence_ids: Vec<String>,
    pub output_hashes: Vec<String>,
    pub completed_at_ms: i64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ChannelKind {
    IMessage,
    Discord,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelEnvelope {
    pub channel: ChannelKind,
    pub source_message_id: String,
    pub sender_id: String,
    pub conversation_id: String,
    pub content_sha256: String,
    pub received_at_ms: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowCandidate {
    pub id: String,
    pub title: String,
    pub successful_mission_ids: Vec<String>,
    pub proposed_definition: WorkflowDefinition,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowDefinition {
    pub id: String,
    pub title: String,
    pub trigger: String,
    pub bounded_steps: Vec<String>,
    pub approved_scope_digest: String,
    pub enabled: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SkillState {
    Candidate,
    Staged,
    Promoted,
    Runnable,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillPermissionManifest {
    pub filesystem_read: Vec<String>,
    pub filesystem_write: Vec<String>,
    pub network_domains: Vec<String>,
    pub external_actions: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillPackage {
    pub id: String,
    pub source_url: String,
    pub commit: String,
    pub digest: String,
    pub license: String,
    pub state: SkillState,
    pub permissions: SkillPermissionManifest,
    pub rollback_commit: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RpcRequest {
    pub jsonrpc: String,
    pub id: u64,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RpcResponse {
    pub jsonrpc: String,
    pub id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

impl RpcResponse {
    #[must_use]
    pub fn success(id: u64, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_owned(),
            id: Some(id),
            result: Some(result),
            error: None,
        }
    }

    #[must_use]
    pub fn failure(id: Option<u64>, error: RpcError) -> Self {
        Self {
            jsonrpc: "2.0".to_owned(),
            id,
            result: None,
            error: Some(error),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcError {
    pub code: i64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

pub const EFFECT_PROTOCOL_VERSION: u32 = 1;
pub const MAX_EFFECT_PAYLOAD_BYTES: u64 = 512 * 1024 * 1024;
pub const MAX_EFFECT_IDENTIFIER_BYTES: usize = 64;
pub const MAX_EFFECT_APPROVAL_IDS: usize = 64;
pub const MAX_EFFECT_SCOPE_DIGEST_BYTES: usize = 256;

#[must_use]
pub fn is_canonical_effect_identifier(value: &str) -> bool {
    let bytes = value.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= MAX_EFFECT_IDENTIFIER_BYTES
        && bytes.first().is_some_and(u8::is_ascii_alphanumeric)
        && bytes.last().is_some_and(u8::is_ascii_alphanumeric)
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EffectAuditAnchor {
    pub sequence: i64,
    pub entry_hash: String,
    pub signature_hex: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PayloadDescriptor {
    pub sha256: String,
    pub byte_len: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "type", deny_unknown_fields)]
pub enum MissionFileEffect {
    PutFile {
        path_components: Vec<String>,
        payload: PayloadDescriptor,
        action_digest: String,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EffectCommand {
    pub protocol_version: u32,
    pub effect_id: String,
    pub mission_id: String,
    pub mission_updated_at_ms: i64,
    pub mission_scope_digest: String,
    pub source_anchor: EffectAuditAnchor,
    pub approval_ids: Vec<String>,
    pub effect: MissionFileEffect,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EffectBrokerSession {
    pub protocol_version: u32,
    pub session_nonce: String,
    pub broker_key_id: String,
    pub broker_verifying_key_hex: String,
    pub expires_at_ms: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeControlAuthorization {
    pub protocol_version: u32,
    pub enabled: bool,
    pub revision: u64,
    pub updated_at_ms: i64,
    pub core_key_id: String,
    pub authorization_signature_hex: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeControlReceipt {
    pub protocol_version: u32,
    pub authorization_hash: String,
    pub checkpoint_nonce: String,
    pub request_nonce: Option<String>,
    pub broker_key_id: String,
    pub broker_signature_hex: String,
}

/// Root-broker authorization for exactly one Core process incarnation.
///
/// The protected broker persists at most one lease per audit EUID. Process
/// start times prevent PID-reuse replay, while `core_instance_nonce` binds the
/// lease to the freshly started Host rather than merely to its PID.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CoreInstanceLease {
    pub protocol_version: u32,
    pub audit_euid: u32,
    pub app_pid: i32,
    pub app_start_time_us: u64,
    pub core_pid: i32,
    pub core_start_time_us: u64,
    pub core_audit_token_hex: String,
    pub codex_pid: i32,
    pub codex_start_time_us: u64,
    pub codex_audit_token_hex: String,
    pub core_instance_nonce: String,
    pub issued_at_ms: i64,
    pub broker_key_id: String,
    pub broker_signature_hex: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum EffectPermitPurpose {
    Execute,
    ReattestOnly,
    Reconcile,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EffectPermit {
    pub command: EffectCommand,
    pub stable_effect_hash: String,
    pub authorization_anchor: EffectAuditAnchor,
    pub purpose: EffectPermitPurpose,
    pub runtime_revision: u64,
    pub broker_session_nonce: String,
    pub issued_at_ms: i64,
    pub expires_at_ms: i64,
    pub core_key_id: String,
    pub authorization_signature_hex: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EffectReceipt {
    pub protocol_version: u32,
    pub effect_id: String,
    pub stable_effect_hash: String,
    pub permit_hash: String,
    pub mission_id: String,
    pub path_components: Vec<String>,
    pub payload_sha256: String,
    pub payload_byte_len: u64,
    pub broker_session_nonce: String,
    pub committed_at_ms: i64,
    pub attested_at_ms: i64,
    pub broker_key_id: String,
    pub broker_signature_hex: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EffectNonCommit {
    pub protocol_version: u32,
    pub effect_id: String,
    pub stable_effect_hash: String,
    pub permit_hash: String,
    pub mission_id: String,
    pub broker_session_nonce: String,
    pub reconciled_at_ms: i64,
    pub broker_key_id: String,
    pub broker_signature_hex: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "outcome")]
pub enum EffectReconciliation {
    Committed { receipt: EffectReceipt },
    NotCommitted { attestation: EffectNonCommit },
}

/// Produces the versioned, sorted-key JSON signed by Core for a broker permit.
///
/// # Errors
///
/// Returns a serialization error if the typed command cannot be encoded.
pub fn effect_command_signing_bytes(command: &EffectCommand) -> Result<Vec<u8>, serde_json::Error> {
    serde_json::to_vec(&serde_json::json!({
        "approvalIds": command.approval_ids,
        "effect": command.effect,
        "effectId": command.effect_id,
        "missionId": command.mission_id,
        "missionScopeDigest": command.mission_scope_digest,
        "missionUpdatedAtMs": command.mission_updated_at_ms,
        "protocolVersion": command.protocol_version,
        "sourceAnchor": command.source_anchor,
    }))
}

/// Produces the versioned, sorted-key JSON covered by the Core authorization
/// signature. The signature itself is deliberately excluded.
///
/// # Errors
///
/// Returns a serialization error if the typed permit cannot be encoded.
pub fn effect_permit_signing_bytes(permit: &EffectPermit) -> Result<Vec<u8>, serde_json::Error> {
    serde_json::to_vec(&serde_json::json!({
        "authorizationAnchor": permit.authorization_anchor,
        "brokerSessionNonce": permit.broker_session_nonce,
        "command": permit.command,
        "coreKeyId": permit.core_key_id,
        "expiresAtMs": permit.expires_at_ms,
        "issuedAtMs": permit.issued_at_ms,
        "purpose": permit.purpose,
        "runtimeRevision": permit.runtime_revision,
        "stableEffectHash": permit.stable_effect_hash,
        "version": 1,
    }))
}

/// Produces the SHA-256 binding for one exact, fully signed permit.
///
/// Unlike [`effect_permit_signing_bytes`], this covers the Core signature as
/// well as every signed field. Broker Receipts carry this hash so an
/// attestation for one permit cannot be replayed for a different permit.
///
/// # Errors
///
/// Returns a serialization error if the typed permit cannot be encoded.
pub fn effect_permit_hash(permit: &EffectPermit) -> Result<String, serde_json::Error> {
    let bytes = serde_json::to_vec(&serde_json::json!({
        "authorizationAnchor": permit.authorization_anchor,
        "authorizationSignatureHex": permit.authorization_signature_hex,
        "brokerSessionNonce": permit.broker_session_nonce,
        "command": permit.command,
        "coreKeyId": permit.core_key_id,
        "expiresAtMs": permit.expires_at_ms,
        "issuedAtMs": permit.issued_at_ms,
        "purpose": permit.purpose,
        "runtimeRevision": permit.runtime_revision,
        "stableEffectHash": permit.stable_effect_hash,
        "version": 1,
    }))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

/// Produces the canonical bytes signed by Core for one global runtime-control
/// transition. The protected broker persists the greatest accepted revision.
///
/// # Errors
///
/// Returns a serialization error if the typed authorization cannot be encoded.
pub fn runtime_control_authorization_signing_bytes(
    authorization: &RuntimeControlAuthorization,
) -> Result<Vec<u8>, serde_json::Error> {
    serde_json::to_vec(&serde_json::json!({
        "coreKeyId": authorization.core_key_id,
        "enabled": authorization.enabled,
        "protocolVersion": authorization.protocol_version,
        "revision": authorization.revision,
        "updatedAtMs": authorization.updated_at_ms,
        "version": 1,
    }))
}

/// Hashes the complete signed Core authorization accepted by the broker.
///
/// # Errors
///
/// Returns a serialization error if the authorization cannot be encoded.
pub fn runtime_control_authorization_hash(
    authorization: &RuntimeControlAuthorization,
) -> Result<String, serde_json::Error> {
    let bytes = serde_json::to_vec(authorization)?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

/// Produces the canonical bytes signed by the protected broker after it has
/// durably accepted a runtime-control authorization.
///
/// # Errors
///
/// Returns a serialization error if the receipt cannot be encoded.
pub fn runtime_control_receipt_signing_bytes(
    receipt: &RuntimeControlReceipt,
) -> Result<Vec<u8>, serde_json::Error> {
    serde_json::to_vec(&serde_json::json!({
        "authorizationHash": receipt.authorization_hash,
        "brokerKeyId": receipt.broker_key_id,
        "checkpointNonce": receipt.checkpoint_nonce,
        "protocolVersion": receipt.protocol_version,
        "requestNonce": receipt.request_nonce,
        "version": 1,
    }))
}

/// Produces the canonical bytes signed by the root effect broker for a Core
/// instance lease. The signature itself is deliberately excluded.
///
/// # Errors
///
/// Returns a serialization error if the typed lease cannot be encoded.
pub fn core_instance_lease_signing_bytes(
    lease: &CoreInstanceLease,
) -> Result<Vec<u8>, serde_json::Error> {
    serde_json::to_vec(&serde_json::json!({
        "appPid": lease.app_pid,
        "appStartTimeUs": lease.app_start_time_us,
        "auditEuid": lease.audit_euid,
        "brokerKeyId": lease.broker_key_id,
        "coreInstanceNonce": lease.core_instance_nonce,
        "coreAuditTokenHex": lease.core_audit_token_hex,
        "corePid": lease.core_pid,
        "coreStartTimeUs": lease.core_start_time_us,
        "codexAuditTokenHex": lease.codex_audit_token_hex,
        "codexPid": lease.codex_pid,
        "codexStartTimeUs": lease.codex_start_time_us,
        "issuedAtMs": lease.issued_at_ms,
        "protocolVersion": lease.protocol_version,
        "version": 1,
    }))
}

/// Produces the versioned, sorted-key JSON covered by the broker Receipt
/// signature. The signature itself is deliberately excluded.
///
/// # Errors
///
/// Returns a serialization error if the typed Receipt cannot be encoded.
pub fn effect_receipt_signing_bytes(receipt: &EffectReceipt) -> Result<Vec<u8>, serde_json::Error> {
    serde_json::to_vec(&serde_json::json!({
        "brokerKeyId": receipt.broker_key_id,
        "brokerSessionNonce": receipt.broker_session_nonce,
        "attestedAtMs": receipt.attested_at_ms,
        "committedAtMs": receipt.committed_at_ms,
        "effectId": receipt.effect_id,
        "missionId": receipt.mission_id,
        "pathComponents": receipt.path_components,
        "payloadByteLen": receipt.payload_byte_len,
        "payloadSha256": receipt.payload_sha256,
        "permitHash": receipt.permit_hash,
        "protocolVersion": receipt.protocol_version,
        "stableEffectHash": receipt.stable_effect_hash,
        "version": 1,
    }))
}

/// Produces the canonical bytes signed by the broker for a definitive
/// noncommit reconciliation.
///
/// # Errors
///
/// Returns a serialization error if the typed attestation cannot be encoded.
pub fn effect_noncommit_signing_bytes(
    attestation: &EffectNonCommit,
) -> Result<Vec<u8>, serde_json::Error> {
    serde_json::to_vec(&serde_json::json!({
        "brokerKeyId": attestation.broker_key_id,
        "brokerSessionNonce": attestation.broker_session_nonce,
        "effectId": attestation.effect_id,
        "missionId": attestation.mission_id,
        "permitHash": attestation.permit_hash,
        "protocolVersion": attestation.protocol_version,
        "reconciledAtMs": attestation.reconciled_at_ms,
        "stableEffectHash": attestation.stable_effect_hash,
        "version": 1,
    }))
}

#[cfg(test)]
mod effect_tests {
    use super::{
        CoreInstanceLease, EFFECT_PROTOCOL_VERSION, EffectAuditAnchor, EffectCommand,
        EffectNonCommit, EffectPermit, EffectPermitPurpose, EffectReceipt, MissionFileEffect,
        PayloadDescriptor, RpcRequest, core_instance_lease_signing_bytes,
        effect_command_signing_bytes, effect_noncommit_signing_bytes, effect_permit_hash,
        effect_permit_signing_bytes, effect_receipt_signing_bytes,
    };

    fn command() -> EffectCommand {
        EffectCommand {
            protocol_version: EFFECT_PROTOCOL_VERSION,
            effect_id: "effect-1".into(),
            mission_id: "mission-1".into(),
            mission_updated_at_ms: 42,
            mission_scope_digest: "scope-v1".into(),
            source_anchor: EffectAuditAnchor {
                sequence: 7,
                entry_hash: "11".repeat(32),
                signature_hex: "22".repeat(64),
            },
            approval_ids: vec!["approval-1".into()],
            effect: MissionFileEffect::PutFile {
                path_components: vec!["reports".into(), "output.xlsx".into()],
                payload: PayloadDescriptor {
                    sha256: "33".repeat(32),
                    byte_len: 128,
                },
                action_digest: "44".repeat(32),
            },
        }
    }

    #[test]
    fn effect_wire_types_reject_unknown_fields() {
        let mut value = serde_json::to_value(command()).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .insert("unexpected".into(), serde_json::json!(true));
        assert!(serde_json::from_value::<EffectCommand>(value).is_err());
    }

    #[test]
    fn local_rpc_requests_reject_unknown_authority_fields() {
        assert!(
            serde_json::from_value::<RpcRequest>(serde_json::json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "mission.runtime.read",
                "params": {},
                "authority": "caller-selected"
            }))
            .is_err()
        );
    }

    #[test]
    fn effect_signature_preimages_are_stable_and_exclude_only_the_signature() {
        let command = command();
        let mut permit = EffectPermit {
            command: command.clone(),
            stable_effect_hash: "55".repeat(32),
            authorization_anchor: command.source_anchor.clone(),
            purpose: EffectPermitPurpose::Execute,
            runtime_revision: 1,
            broker_session_nonce: "66".repeat(32),
            issued_at_ms: 100,
            expires_at_ms: 200,
            core_key_id: "77".repeat(32),
            authorization_signature_hex: String::new(),
        };
        let command_bytes = effect_command_signing_bytes(&command).unwrap();
        assert_eq!(
            command_bytes,
            effect_command_signing_bytes(&command).unwrap()
        );
        let unsigned = effect_permit_signing_bytes(&permit).unwrap();
        permit.authorization_signature_hex = "88".repeat(64);
        assert_eq!(unsigned, effect_permit_signing_bytes(&permit).unwrap());
        let permit_hash = effect_permit_hash(&permit).unwrap();
        let mut changed_permit = permit.clone();
        changed_permit.authorization_signature_hex = "aa".repeat(64);
        assert_ne!(permit_hash, effect_permit_hash(&changed_permit).unwrap());
        let mut recovery_permit = permit.clone();
        recovery_permit.purpose = EffectPermitPurpose::ReattestOnly;
        assert_ne!(
            effect_permit_signing_bytes(&permit).unwrap(),
            effect_permit_signing_bytes(&recovery_permit).unwrap()
        );
        assert_ne!(permit_hash, effect_permit_hash(&recovery_permit).unwrap());

        let mut receipt = EffectReceipt {
            protocol_version: EFFECT_PROTOCOL_VERSION,
            effect_id: command.effect_id.clone(),
            stable_effect_hash: "55".repeat(32),
            permit_hash,
            mission_id: command.mission_id,
            path_components: vec!["reports".into(), "output.xlsx".into()],
            payload_sha256: "33".repeat(32),
            payload_byte_len: 128,
            broker_session_nonce: "66".repeat(32),
            committed_at_ms: 90,
            attested_at_ms: 100,
            broker_key_id: "99".repeat(32),
            broker_signature_hex: String::new(),
        };
        let unsigned_receipt = effect_receipt_signing_bytes(&receipt).unwrap();
        receipt.broker_signature_hex = "aa".repeat(64);
        assert_eq!(
            unsigned_receipt,
            effect_receipt_signing_bytes(&receipt).unwrap()
        );

        let mut reconciliation_permit = permit.clone();
        reconciliation_permit.purpose = EffectPermitPurpose::Reconcile;
        let mut noncommit = EffectNonCommit {
            protocol_version: EFFECT_PROTOCOL_VERSION,
            effect_id: command.effect_id,
            stable_effect_hash: "55".repeat(32),
            permit_hash: effect_permit_hash(&reconciliation_permit).unwrap(),
            mission_id: receipt.mission_id,
            broker_session_nonce: "66".repeat(32),
            reconciled_at_ms: 110,
            broker_key_id: "99".repeat(32),
            broker_signature_hex: String::new(),
        };
        let unsigned_noncommit = effect_noncommit_signing_bytes(&noncommit).unwrap();
        noncommit.broker_signature_hex = "bb".repeat(64);
        assert_eq!(
            unsigned_noncommit,
            effect_noncommit_signing_bytes(&noncommit).unwrap()
        );
    }

    #[test]
    fn core_instance_lease_preimage_binds_process_incarnation_not_signature() {
        let mut lease = CoreInstanceLease {
            protocol_version: EFFECT_PROTOCOL_VERSION,
            audit_euid: 501,
            app_pid: 10,
            app_start_time_us: 1_000,
            core_pid: 11,
            core_start_time_us: 1_001,
            core_audit_token_hex: "33".repeat(32),
            codex_pid: 12,
            codex_start_time_us: 1_002,
            codex_audit_token_hex: "44".repeat(32),
            core_instance_nonce: "11".repeat(32),
            issued_at_ms: 2_000,
            broker_key_id: "22".repeat(32),
            broker_signature_hex: String::new(),
        };
        let unsigned = core_instance_lease_signing_bytes(&lease).unwrap();
        lease.broker_signature_hex = "33".repeat(64);
        assert_eq!(unsigned, core_instance_lease_signing_bytes(&lease).unwrap());
        for changed in [
            {
                let mut changed = lease.clone();
                changed.core_start_time_us += 1;
                changed
            },
            {
                let mut changed = lease.clone();
                changed.core_audit_token_hex = "55".repeat(32);
                changed
            },
            {
                let mut changed = lease.clone();
                changed.codex_pid += 1;
                changed
            },
            {
                let mut changed = lease.clone();
                changed.codex_start_time_us += 1;
                changed
            },
            {
                let mut changed = lease.clone();
                changed.codex_audit_token_hex = "66".repeat(32);
                changed
            },
        ] {
            assert_ne!(
                unsigned,
                core_instance_lease_signing_bytes(&changed).unwrap()
            );
        }
    }
}
