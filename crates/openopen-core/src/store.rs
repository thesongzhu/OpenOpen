use crate::channel::{
    channel_message_payload, channel_need_you_content, channel_receipt_content, validate_cursor,
    validate_delivery, validate_mission_event, validate_observation, validate_outbound,
    validate_pairing, validate_route_approval, validate_route_set,
};
use crate::markdown::{MarkdownRenderOutcome, MarkdownRoot};
use crate::mission::{apply_mission_command, validate_mission_snapshot, validate_receipt};
use crate::{
    ActionGate, ActionProposal, ActionTarget, CryptoError, EffectKind, GateDecision,
    LocalAuthority, MissionCommand, MissionError, TrustedBrokerEnrollment,
};
use openopen_protocol::{
    ApprovalKind, ApprovalStatus, ChannelCursor, ChannelDeliveryReceipt, ChannelEnvelope,
    ChannelFailureAcknowledgement, ChannelFailureClass, ChannelFailureIncident,
    ChannelInboundDecision, ChannelInboundMessageClass, ChannelInboundResult, ChannelKind,
    ChannelMessageKind, ChannelMissionEvent, ChannelModelDisposition, ChannelModelStart,
    ChannelObservation, ChannelOutboundDisposition, ChannelOutboundIntent, ChannelOutboundStart,
    ChannelPairing, ChannelRoute, ChannelRouteApproval, ChannelRouteApprovalDecision,
    ChannelRouteRole, ChannelRouteSet, ChoiceBeginAccepted, ChoiceBeginRecord,
    ChoiceConsolidatedConfirmation, ChoiceDIntakeRecord, ChoiceIMessageReplyDisposition,
    ChoiceIMessageReplyIntent, ChoiceIMessageReplyPreview, ChoiceIMessageReplyStart,
    ChoiceInitialResult, ChoiceLoopSnapshot, ChoiceRefinementContext, ChoiceRefinementOperation,
    ChoiceRefinementResult, ChoiceReminderSchedule, ChoiceReminderScheduleInput,
    ChoiceResumeResult, ChoiceSession, ChoiceSessionState, ChoiceSet, DocumentManifest,
    EFFECT_PROTOCOL_VERSION, EffectAuditAnchor, EffectBrokerSession, EffectCommand,
    EffectNonCommit, EffectPermit, EffectPermitPurpose, EffectReceipt, InterpretationFrame,
    MAX_EFFECT_APPROVAL_IDS, MAX_EFFECT_PAYLOAD_BYTES, MAX_EFFECT_SCOPE_DIGEST_BYTES,
    MarkdownBaseIdentity, MarkdownRenderIntent, MarkdownRenderReceipt, Mission, MissionFileEffect,
    MissionStatus, ModelSelection, ModelSelectionState, OptionSelection, OutcomeSuggestion,
    PayloadDescriptor, Receipt, RuntimeControlAuthorization, RuntimeControlReceipt, Selection,
    canonical_choice_set_digest, canonical_document_manifest_digest,
    is_canonical_effect_identifier,
};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;
use zeroize::Zeroizing;

const MISSION_COMMAND_ACTION: &str = "mission.command";
const RECEIPT_COMMIT_ACTION: &str = "receipt.created";
const EFFECT_AUTHORIZATION_ACTION: &str = "effect.authorized";
const EFFECT_RECEIPT_ACTION: &str = "effect.committed";
const EFFECT_NONCOMMIT_ACTION: &str = "effect.not_committed";
const CHANNEL_PAIRING_ACTION: &str = "channel.paired";
const CHANNEL_OBSERVATION_ACTION: &str = "channel.observed";
const CHANNEL_CURSOR_ACTION: &str = "channel.cursor_advanced";
const CHANNEL_MODEL_QUEUED_ACTION: &str = "channel.model_queued";
const CHANNEL_MODEL_ACTION: &str = "channel.model_started";
const CHANNEL_MODEL_FAILED_ACTION: &str = "channel.model_failed";
const CHANNEL_FAILURE_INCIDENT_ACTION: &str = "channel.failure_incident_recorded";
const CHANNEL_FAILURE_ACK_ACTION: &str = "channel.failure_incident_acknowledged";
const CHOICE_MODEL_SELECTION_ACTION: &str = "choice.model_selected";
const CHOICE_BEGIN_ACTION: &str = "choice.begin_accepted";
const CHOICE_D_INTAKE_ACTION: &str = "choice.d_intake_accepted";
const CHOICE_REFINEMENT_CONTEXT_ACTION: &str = "choice.refinement_context_accepted";
const CHOICE_REFINEMENT_RESULT_ACTION: &str = "choice.refinement_result_committed";
const CHOICE_BODY_RETIREMENT_ACTION: &str = "choice.private_body_retired";
const CHOICE_IMESSAGE_REPLY_PREPARED_ACTION: &str = "choice.imessage_reply_prepared";
const CHOICE_IMESSAGE_REPLY_AUTHORIZED_ACTION: &str = "choice.imessage_reply_authorized";
const CHOICE_IMESSAGE_REPLY_DELIVERED_ACTION: &str = "choice.imessage_reply_delivered";
const CHOICE_MARKDOWN_RENDER_INTENT_ACTION: &str = "choice.markdown_render_intent";
const CHOICE_MARKDOWN_RENDER_RECEIPT_ACTION: &str = "choice.markdown_render_receipt";
const CHOICE_REMINDER_SCHEDULE_ACTION: &str = "choice.reminder_schedule_selected";
const CHOICE_LOOP_STATE_ACTION: &str = "choice.loop_state_changed";
const CHOICE_IDLE_CLOCK_ACTION: &str = "choice.idle_clock_anchored";
/// The App contract accepts a bounded current incident projection. The full
/// verified/audited history remains durable in the Store; this limit only
/// bounds one UI/RPC response.
pub const CHANNEL_FAILURE_INCIDENT_PROJECTION_LIMIT: usize = 128;
const CHANNEL_SUGGESTION_ACTION: &str = "channel.suggestion_ready";
const CHANNEL_ROUTE_SET_ACTION: &str = "channel.route_set_changed";
const CHANNEL_MISSION_EVENT_ACTION: &str = "channel.mission_event_recorded";
const LEGACY_CHANNEL_ORIGIN_ACTION: &str = "channel.mission_origin_bound";
const CHANNEL_OUTBOUND_ACTION: &str = "channel.outbound_started";
const CHANNEL_DELIVERY_ACTION: &str = "channel.outbound_delivered";
const MAX_CHANNEL_CONTENT_BYTES: usize = 16 * 1024;
const MAX_CHANNEL_MODEL_CONTEXT_MESSAGES: usize = 2;
const CHANNEL_CORRECTION_PREFIX: &str = "correction to previous:";
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
CREATE TABLE IF NOT EXISTS channel_pairing (
 channel_json TEXT PRIMARY KEY, encrypted_blob BLOB NOT NULL,
 paired_at_ms INTEGER NOT NULL, blob_hash TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS channel_observation (
 channel_json TEXT NOT NULL, source_message_id TEXT NOT NULL, entity_id TEXT NOT NULL UNIQUE,
 conversation_id TEXT NOT NULL, cursor_order INTEGER NOT NULL,
 decision_json TEXT NOT NULL, encrypted_blob BLOB NOT NULL, blob_hash TEXT NOT NULL,
 PRIMARY KEY(channel_json, source_message_id)
);
CREATE TABLE IF NOT EXISTS channel_cursor (
 channel_json TEXT NOT NULL, conversation_id TEXT NOT NULL, entity_id TEXT NOT NULL UNIQUE,
 cursor_order INTEGER NOT NULL, encrypted_blob BLOB NOT NULL, blob_hash TEXT NOT NULL,
 PRIMARY KEY(channel_json, conversation_id)
);
CREATE TABLE IF NOT EXISTS channel_model_dispatch (
 entity_id TEXT PRIMARY KEY, channel_json TEXT NOT NULL, source_message_id TEXT NOT NULL,
 status_json TEXT NOT NULL, suggestion_id TEXT,
 encrypted_blob BLOB NOT NULL, blob_hash TEXT NOT NULL,
 UNIQUE(channel_json, source_message_id), UNIQUE(suggestion_id)
);
CREATE TABLE IF NOT EXISTS channel_failure_incident (
 incident_id TEXT PRIMARY KEY, channel_json TEXT NOT NULL, source_message_id TEXT NOT NULL,
 failure_class TEXT NOT NULL, acknowledged INTEGER NOT NULL CHECK (acknowledged IN (0, 1)),
 encrypted_blob BLOB NOT NULL, blob_hash TEXT NOT NULL,
 UNIQUE(channel_json, source_message_id, failure_class)
);
CREATE TABLE IF NOT EXISTS channel_route_set (
 mission_id TEXT PRIMARY KEY, revision INTEGER NOT NULL CHECK (revision > 0),
 primary_route_id TEXT NOT NULL, encrypted_blob BLOB NOT NULL, blob_hash TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS channel_mission_event (
 entity_id TEXT PRIMARY KEY, mission_id TEXT NOT NULL, route_id TEXT NOT NULL,
 route_set_revision INTEGER NOT NULL CHECK (route_set_revision > 0),
 mission_revision INTEGER NOT NULL CHECK (mission_revision > 0),
 channel_json TEXT NOT NULL, source_message_id TEXT NOT NULL,
 encrypted_blob BLOB NOT NULL, blob_hash TEXT NOT NULL,
 UNIQUE(channel_json, source_message_id)
);
CREATE TABLE IF NOT EXISTS channel_outbound (
 outbound_id TEXT PRIMARY KEY, mission_id TEXT NOT NULL, channel_json TEXT NOT NULL,
 conversation_id TEXT NOT NULL, content_sha256 TEXT NOT NULL,
 status_json TEXT NOT NULL, provider_message_id TEXT,
 encrypted_blob BLOB NOT NULL, blob_hash TEXT NOT NULL,
 UNIQUE(channel_json, provider_message_id)
);
CREATE TABLE IF NOT EXISTS choice_imessage_reply (
 reply_id TEXT PRIMARY KEY, source_message_id TEXT NOT NULL UNIQUE,
 choice_session_id TEXT NOT NULL, choice_set_id TEXT NOT NULL,
 preview_revision INTEGER NOT NULL CHECK (preview_revision > 0),
 confirmation_digest TEXT NOT NULL, status_json TEXT NOT NULL,
 provider_message_id TEXT UNIQUE, encrypted_blob BLOB NOT NULL, blob_hash TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS choice_model_selection (
 singleton_id INTEGER PRIMARY KEY CHECK (singleton_id = 1),
 encrypted_blob BLOB NOT NULL, blob_hash TEXT NOT NULL, selected_at_ms INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS choice_begin_request (
 request_id TEXT PRIMARY KEY, request_digest TEXT NOT NULL, choice_session_id TEXT NOT NULL UNIQUE,
 operation_id TEXT NOT NULL UNIQUE, source_envelope_id TEXT NOT NULL,
 conversation_turn_batch_id TEXT NOT NULL, encrypted_blob BLOB NOT NULL, blob_hash TEXT NOT NULL,
 accepted_at_ms INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS choice_d_request (
 request_id TEXT PRIMARY KEY, request_digest TEXT NOT NULL,
 operation_id TEXT NOT NULL UNIQUE, choice_session_id TEXT NOT NULL,
 source_envelope_id TEXT NOT NULL, conversation_turn_batch_id TEXT NOT NULL,
 encrypted_blob BLOB NOT NULL, blob_hash TEXT NOT NULL, accepted_at_ms INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS choice_refinement_context (
 operation_id TEXT PRIMARY KEY, selection_id TEXT NOT NULL, choice_session_id TEXT NOT NULL,
 source_envelope_id TEXT NOT NULL, conversation_turn_batch_id TEXT NOT NULL,
 encrypted_blob BLOB NOT NULL, blob_hash TEXT NOT NULL, created_at_ms INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS choice_private_body_tombstone (
 source_kind TEXT NOT NULL CHECK (source_kind IN ('begin', 'd')),
 request_id TEXT NOT NULL, request_digest TEXT NOT NULL, body_digest TEXT NOT NULL,
 choice_session_id TEXT NOT NULL, retired_at_ms INTEGER NOT NULL,
 PRIMARY KEY(source_kind, request_id)
);
CREATE TABLE IF NOT EXISTS choice_refinement_result (
 operation_id TEXT PRIMARY KEY, selection_id TEXT NOT NULL, choice_session_id TEXT NOT NULL,
 source_envelope_id TEXT NOT NULL, conversation_turn_batch_id TEXT NOT NULL,
 result_digest TEXT NOT NULL, encrypted_blob BLOB NOT NULL, blob_hash TEXT NOT NULL,
 completed_at_ms INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS choice_refinement_body_tombstone (
 operation_id TEXT PRIMARY KEY, result_digest TEXT NOT NULL, choice_session_id TEXT NOT NULL,
 retired_at_ms INTEGER NOT NULL
);
-- The legacy tombstone tables are deliberately only lookup indexes.  The
-- encrypted record below is the authoritative, locally authenticated proof
-- that a private body was retired.  Keeping the index supports bounded
-- replay rejection without retaining the body itself.
CREATE TABLE IF NOT EXISTS choice_private_body_retirement (
 source_kind TEXT NOT NULL CHECK (source_kind IN ('begin', 'd', 'refinement')),
 entity_id TEXT NOT NULL, encrypted_blob BLOB NOT NULL, blob_hash TEXT NOT NULL,
 retired_at_ms INTEGER NOT NULL,
 PRIMARY KEY(source_kind, entity_id)
);
CREATE TABLE IF NOT EXISTS choice_markdown_render_intent (
 intent_id TEXT PRIMARY KEY, intent_digest TEXT NOT NULL, encrypted_blob BLOB NOT NULL,
 blob_hash TEXT NOT NULL, created_at_ms INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS choice_markdown_render_receipt (
 intent_id TEXT PRIMARY KEY, encrypted_blob BLOB NOT NULL, blob_hash TEXT NOT NULL,
 committed_at_ms INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS choice_reminder_schedule (
 schedule_id TEXT PRIMARY KEY, request_id TEXT NOT NULL UNIQUE, choice_session_id TEXT NOT NULL,
 revision INTEGER NOT NULL CHECK (revision > 0), encrypted_blob BLOB NOT NULL,
 blob_hash TEXT NOT NULL, accepted_at_ms INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS choice_idle_clock_anchor (
 singleton_id INTEGER PRIMARY KEY CHECK (singleton_id = 1),
 encrypted_blob BLOB NOT NULL, blob_hash TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS choice_loop_state (
 singleton_id INTEGER PRIMARY KEY CHECK (singleton_id = 1),
 encrypted_blob BLOB NOT NULL, blob_hash TEXT NOT NULL, updated_at_ms INTEGER NOT NULL
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
    #[error("channel invariant failed: {0}")]
    Channel(#[from] crate::ChannelError),
    #[error("channel pairing conflicts with the immutable owner-confirmed pairing")]
    ChannelPairingConflict,
    #[error("channel cursor or observation conflicts with durable recovery state")]
    ChannelObservationConflict,
    #[error("Mission channel origin conflicts with the accepted originating message")]
    ChannelOriginConflict,
    #[error("Mission channel route set conflicts with its pairing, approval, or revision")]
    ChannelRouteConflict,
    #[error("channel outbound id was reused with a different typed message")]
    ChannelOutboundConflict,
    #[error("channel outbound action failed authorization: {0:?}")]
    ChannelAuthorization(GateDecision),
    #[error("stored channel state does not match its signed encrypted record: {0}")]
    ChannelStateMismatch(String),
    #[error(
        "channel failure incident conflicts with its immutable dispatch or acknowledgement: {0}"
    )]
    ChannelFailureIncidentConflict(String),
    #[error("channel model work is deferred while a Mission is nonterminal")]
    ChannelModelDeferredByMission,
    #[error("Choice model selection is missing, malformed, or conflicts with its audit binding")]
    ChoiceModelSelectionConflict,
    #[error(
        "Choice begin request is missing, malformed, stale, or conflicts with its audit binding"
    )]
    ChoiceBeginConflict,
    #[error("Choice Loop state is malformed, stale, or conflicts with its audit binding")]
    ChoiceLoopStateConflict,
    #[error("Choice Loop clock continuity is uncertain")]
    ChoiceClockUncertain,
    #[error("Mission confirmation conflicts with already-started channel model work")]
    MissionModelInFlight,
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

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredChannelObservation {
    observation: ChannelObservation,
    decision: ChannelInboundDecision,
    accepted_content: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredChannelOutbound {
    intent: ChannelOutboundIntent,
    delivery: Option<ChannelDeliveryReceipt>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredChannelModelDispatch {
    channel: ChannelKind,
    source_message_id: String,
    state: StoredChannelModelState,
    suggestion: Option<OutcomeSuggestion>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredChannelFailureAcknowledgement {
    acknowledged_at_ms: i64,
    runtime_revision: u64,
    incident_anchor: AuditAnchor,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredChannelFailureIncident {
    incident_id: String,
    channel: ChannelKind,
    source_message_id: String,
    failure_class: ChannelFailureClass,
    occurred_at_ms: i64,
    runtime_revision: u64,
    dispatch_state_hash: String,
    source_audit_anchor: AuditAnchor,
    acknowledgement: Option<StoredChannelFailureAcknowledgement>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct StoredMarkdownRenderIntent {
    intent: MarkdownRenderIntent,
    plaintext_body: Option<String>,
    #[serde(default)]
    reconciliation: Option<StoredMarkdownReconciliation>,
}

/// A body-free, authenticated record that publication needs explicit owner
/// review. It never contains a path outside the sealed intent or any private
/// Markdown bytes, and it prevents a generic retry loop after a conflict.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredMarkdownReconciliation {
    reason: String,
    recorded_at_ms: i64,
}

impl StoredMarkdownReconciliation {
    fn is_valid(&self) -> bool {
        self.reason == "descriptor-conflict" && self.recorded_at_ms >= 0
    }
}

/// Body-free, locally authenticated retirement evidence.  This deliberately
/// stores only stable identity and digest material: the original encrypted
/// blob is gone, but an attacker cannot alter the replay/cancellation marker
/// without failing AEAD verification and the bound audit record.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ChoicePrivateBodyRetirement {
    source_kind: String,
    entity_id: String,
    request_digest: Option<String>,
    body_digest: String,
    choice_session_id: String,
    source_blob_hash: String,
    retired_at_ms: i64,
    /// A pre-attestation plaintext tombstone cannot be promoted into trusted
    /// provenance. It is retained only as an authenticated *blocked* replay
    /// marker, whose marker digest must still match the legacy row.
    #[serde(default)]
    legacy_blocked: bool,
}

struct ChoicePrivateBodyTombstoneArgs<'a> {
    source_kind: &'a str,
    request_id: &'a str,
    request_digest: &'a str,
    body_digest: &'a str,
    choice_session_id: &'a str,
    source_blob_hash: &'a str,
    retired_at_ms: i64,
}

impl ChoicePrivateBodyRetirement {
    fn is_valid(&self) -> bool {
        matches!(self.source_kind.as_str(), "begin" | "d" | "refinement")
            && !self.entity_id.is_empty()
            && self
                .request_digest
                .as_ref()
                .is_none_or(|digest| is_sha256_hex(digest))
            && is_sha256_hex(&self.body_digest)
            && is_sha256_hex(&self.source_blob_hash)
            && !self.choice_session_id.is_empty()
            && self.retired_at_ms >= 0
    }
}

/// Body-free outcome of publishing an already command-owned Markdown intent.
/// The Store owns decryption and the descriptor writer; external callers can
/// neither provide plaintext nor obtain it from this capability.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MarkdownRenderPublication {
    Committed(MarkdownRenderReceipt),
    ReconciliationRequired,
}

/// Body-free result of verifying a durable render receipt and disposing its
/// retained displaced base. The only success path permits the separate Store
/// retirement transaction; it never recreates or exposes a Markdown body.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MarkdownReceiptCleanup {
    ReadyForRetirement,
    ReconciliationRequired,
}

/// Terminal body-free result of the sealed Markdown cleanup transaction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MarkdownRenderCleanup {
    Retired(Box<ChoiceLoopSnapshot>),
    ReconciliationRequired,
}

fn confirmed_choice_markdown_body(confirmation: &ChoiceConsolidatedConfirmation) -> String {
    let mut body = String::from("# Confirmed choice\n\n## Goal\n\n");
    body.push_str(&confirmation.goal);
    body.push_str("\n\n## Steps\n");
    for step in &confirmation.steps {
        body.push_str("\n- ");
        body.push_str(step);
    }
    // The immutable confirmation seals the entry digest.  Do not embed its
    // digest in the body: that would create a circular preimage in which the
    // body digest depends on the confirmation that itself binds that digest.
    body.push('\n');
    body
}

fn markdown_render_intent_id(
    confirmation: &ChoiceConsolidatedConfirmation,
    session_revision: u64,
    generation: u64,
    content_digest: &str,
) -> String {
    let digest = sha256_hex(
        serde_json::to_string(&json!({
            "confirmation": confirmation.id,
            "session": confirmation.choice_session_id,
            "revision": session_revision,
            "entry": content_digest,
            "generation": generation,
        }))
        .expect("fixed Markdown render identity serializes")
        .as_bytes(),
    );
    format!("choice-markdown-render-{}", &digest[..32])
}

fn markdown_render_record_for_confirmation(
    confirmation: &ChoiceConsolidatedConfirmation,
    session_revision: u64,
    generation: u64,
    created_at_ms: i64,
) -> Result<StoredMarkdownRenderIntent, StoreError> {
    let plaintext_body = confirmed_choice_markdown_body(confirmation);
    let entry = confirmation.markdown_entry.clone();
    if !confirmation.is_valid()
        || session_revision == 0
        || generation == 0
        || created_at_ms < 0
        || entry.relative_path != format!("sessions/{}/CHOICE.md", confirmation.choice_session_id)
        || entry.sha256 != sha256_hex(plaintext_body.as_bytes())
        || entry.byte_length
            != u64::try_from(plaintext_body.len())
                .map_err(|_| StoreError::ChoiceLoopStateConflict)?
        || entry.mode != 0o600
    {
        return Err(StoreError::ChoiceLoopStateConflict);
    }
    let intent = MarkdownRenderIntent {
        id: markdown_render_intent_id(confirmation, session_revision, generation, &entry.sha256),
        choice_session_id: confirmation.choice_session_id.clone(),
        expected_session_revision: session_revision,
        expected_generation: generation,
        entry: entry.clone(),
        expected_base: confirmation.markdown_expected_base.clone(),
        content_digest: entry.sha256,
        created_at_ms,
    };
    Ok(StoredMarkdownRenderIntent {
        intent,
        plaintext_body: Some(plaintext_body),
        reconciliation: None,
    })
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ChoiceIdleClockEvidence {
    pub boot_id: String,
    pub wall_clock_ms: i64,
    pub monotonic_ms: i64,
}

impl ChoiceIdleClockEvidence {
    fn is_valid(&self) -> bool {
        !self.boot_id.is_empty()
            && self.boot_id.len() <= 128
            && self
                .boot_id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
            && self.wall_clock_ms >= 0
            && self.monotonic_ms >= 0
    }
}

/// Store-owned classification for one Host clock sample. A calibration point
/// is intentionally distinct from a continuous no-op: reads may establish the
/// anchor, but an authority-consuming command must require a later continuous
/// sample before it may use the current `ChoiceSet`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ChoiceIdleAdvance {
    Calibrated(ChoiceLoopSnapshot),
    Unchanged(ChoiceLoopSnapshot),
    Transitioned(ChoiceLoopSnapshot),
}

impl ChoiceIdleAdvance {
    #[must_use]
    pub fn snapshot(&self) -> &ChoiceLoopSnapshot {
        match self {
            Self::Calibrated(snapshot)
            | Self::Unchanged(snapshot)
            | Self::Transitioned(snapshot) => snapshot,
        }
    }
}

struct LoadedChannelFailureIncident {
    stored: StoredChannelFailureIncident,
    incident_anchor: AuditAnchor,
    acknowledgement_anchor: Option<AuditAnchor>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LegacyChannelMissionOrigin {
    mission_id: String,
    channel: ChannelKind,
    conversation_id: String,
    owner_sender_id: String,
    source_message_id: String,
    bound_at_ms: i64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
enum StoredChannelModelState {
    Queued,
    Started,
    Failed,
    Ready,
}

pub struct Store {
    connection: Connection,
    authority: LocalAuthority,
    trusted_broker: Option<TrustedBrokerEnrollment>,
    // Bound once by the Host from its canonical, signed-in account path.
    // Never re-read HOME for an effectful Markdown operation: a mutable
    // process environment must not redirect a previously validated journal.
    choice_markdown_root: Option<PathBuf>,
}

impl Store {
    /// Opens a persistent encrypted store using master material loaded by the host.
    ///
    /// # Errors
    ///
    /// Returns a database or migration error.
    pub fn open(path: &Path, authority: LocalAuthority) -> Result<Self, StoreError> {
        let connection = Connection::open(path)?;
        let mut store = Self {
            connection,
            authority,
            trusted_broker: None,
            choice_markdown_root: None,
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
        let mut store = Self {
            connection,
            authority,
            trusted_broker: None,
            choice_markdown_root: None,
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
        let mut store = Self {
            connection,
            authority,
            trusted_broker: Some(trusted_broker),
            choice_markdown_root: None,
        };
        store.migrate()?;
        store.migrate_channel_failure_incidents()?;
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
        let mut store = Self {
            connection,
            authority,
            trusted_broker: Some(trusted_broker),
            choice_markdown_root: None,
        };
        store.migrate()?;
        store.migrate_channel_failure_incidents()?;
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

    /// Binds command-owned Markdown publication to the exact canonical home
    /// validated by the Host at startup. A later `HOME` environment change or
    /// a second caller cannot redirect an existing journal.
    ///
    /// # Errors
    ///
    /// Returns an error when the Host-provided canonical home is unsafe or a
    /// different root was previously bound to this Store instance. Production
    /// `HostPaths` validates this home before Store construction; test Hosts
    /// may bind an isolated temporary home without consulting process `HOME`.
    pub fn bind_choice_markdown_root(&mut self, user_home: &Path) -> Result<(), StoreError> {
        let supplied_home =
            std::fs::canonicalize(user_home).map_err(|_| StoreError::ChoiceLoopStateConflict)?;
        let root = supplied_home.join("Documents").join("OpenOpen");
        match &self.choice_markdown_root {
            Some(existing) if existing != &root => Err(StoreError::ChoiceLoopStateConflict),
            Some(_) => Ok(()),
            None => {
                self.choice_markdown_root = Some(root);
                Ok(())
            }
        }
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
        let previous = self.trusted_broker.clone();
        match &self.trusted_broker {
            None => self.trusted_broker = Some(enrollment),
            Some(existing) if existing == &enrollment => {}
            Some(_) => {
                return Err(StoreError::EffectProtocol(
                    crate::EffectProtocolError::UntrustedBroker,
                ));
            }
        }
        if let Err(error) = self.migrate_channel_failure_incidents() {
            self.trusted_broker = previous;
            return Err(error);
        }
        Ok(())
    }

    fn migrate(&mut self) -> Result<(), StoreError> {
        self.connection.execute_batch(STORE_SCHEMA)?;
        self.require_current_choice_private_identity_schema()?;
        self.require_current_choice_idle_clock_schema()?;
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
        drop(columns);
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
        self.migrate_legacy_channel_origins()?;
        self.migrate_choice_loop_delivery_bindings()?;
        self.migrate_choice_private_body_retirements()?;
        // Migration-blocked markers remain tied to their old body-free row;
        // validate them at open time as well as at each command boundary so a
        // tampered legacy row cannot hide until a later foreground action.
        verify_choice_private_body_retirements(&self.connection, &self.authority)?;
        Ok(())
    }

    /// The PR1 clock anchor originally existed as unauthenticated plaintext.
    /// An empty development table can be replaced mechanically, but a
    /// populated legacy anchor is not promoted into time authority.
    fn require_current_choice_idle_clock_schema(&mut self) -> Result<(), StoreError> {
        let columns = self
            .connection
            .prepare("PRAGMA table_info(choice_idle_clock_anchor)")?
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<Vec<_>, _>>()?;
        if columns.iter().any(|name| name == "encrypted_blob")
            && columns.iter().any(|name| name == "blob_hash")
            && !columns.iter().any(|name| name == "boot_id")
        {
            return Ok(());
        }
        let row_count: i64 = self.connection.query_row(
            "SELECT COUNT(*) FROM choice_idle_clock_anchor",
            [],
            |row| row.get(0),
        )?;
        if row_count != 0 {
            return Err(StoreError::ChoiceClockUncertain);
        }
        self.connection.execute_batch(
            "DROP TABLE choice_idle_clock_anchor;
             CREATE TABLE choice_idle_clock_anchor (
               singleton_id INTEGER PRIMARY KEY CHECK (singleton_id = 1),
               encrypted_blob BLOB NOT NULL, blob_hash TEXT NOT NULL
             );",
        )?;
        Ok(())
    }

    /// The private Choice intake/result tables were introduced only on the
    /// unshipped PR1 branch. An older development database may therefore have
    /// rows whose ciphertext was created before the complete identity tuple
    /// was part of its AAD. Re-encrypting those rows would promote data that
    /// was never authenticated under the stronger contract, so opening fails
    /// closed unless the old table is empty; empty tables are upgraded in
    /// place without retaining or inferring any private body identity.
    fn require_current_choice_private_identity_schema(&mut self) -> Result<(), StoreError> {
        for (table, required_columns) in [
            (
                "choice_begin_request",
                &["source_envelope_id", "conversation_turn_batch_id"][..],
            ),
            (
                "choice_d_request",
                &[
                    "choice_session_id",
                    "source_envelope_id",
                    "conversation_turn_batch_id",
                ][..],
            ),
            (
                "choice_refinement_context",
                &["source_envelope_id", "conversation_turn_batch_id"][..],
            ),
            (
                "choice_refinement_result",
                &[
                    "choice_session_id",
                    "source_envelope_id",
                    "conversation_turn_batch_id",
                ][..],
            ),
        ] {
            let column_names = self
                .connection
                .prepare(&format!("PRAGMA table_info({table})"))?
                .query_map([], |row| row.get::<_, String>(1))?
                .collect::<Result<Vec<_>, _>>()?;
            let missing = required_columns
                .iter()
                .copied()
                .filter(|column| !column_names.iter().any(|name| name == column))
                .collect::<Vec<_>>();
            if missing.is_empty() {
                continue;
            }
            let row_count: i64 =
                self.connection
                    .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                        row.get(0)
                    })?;
            if row_count != 0 {
                return Err(StoreError::ChoiceLoopStateConflict);
            }
            for column in missing {
                self.connection.execute(
                    &format!("ALTER TABLE {table} ADD COLUMN {column} TEXT NOT NULL DEFAULT ''"),
                    [],
                )?;
            }
        }
        Ok(())
    }

    /// Upgrades the previous plaintext-only retirement indexes into encrypted
    /// `LocalAuthority` records. The original signed audit state hash is copied
    /// as provenance; no private body is reconstructed or inferred. If that
    /// historic audit row is absent, opening fails closed rather than silently
    /// treating the old marker as a valid replay boundary.
    #[allow(clippy::too_many_lines)] // One IMMEDIATE migration must retain every fail-closed branch together.
    fn migrate_choice_private_body_retirements(&mut self) -> Result<(), StoreError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let has_legacy_rows: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM choice_private_body_tombstone)
                 OR EXISTS(SELECT 1 FROM choice_refinement_body_tombstone)",
            [],
            |row| row.get(0),
        )?;
        if !has_legacy_rows {
            transaction.commit()?;
            return Ok(());
        }
        // A migration changes authenticated retirement state. Verify the
        // complete existing audit chain before inspecting or writing any row,
        // so a tampered tail cannot be laundered into a newly signed marker.
        verified_audit_tail(&transaction, &self.authority)?;
        let private_rows = transaction
            .prepare(
                "SELECT source_kind, request_id, request_digest, body_digest,
                        choice_session_id, retired_at_ms
                 FROM choice_private_body_tombstone",
            )?
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, i64>(5)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        for (
            source_kind,
            entity_id,
            request_digest,
            body_digest,
            choice_session_id,
            retired_at_ms,
        ) in private_rows
        {
            let exists: bool = transaction.query_row(
                "SELECT EXISTS(SELECT 1 FROM choice_private_body_retirement
                 WHERE source_kind = ?1 AND entity_id = ?2)",
                params![&source_kind, &entity_id],
                |row| row.get(0),
            )?;
            if exists {
                continue;
            }
            if retirement_audit_exists(&transaction, &source_kind, &entity_id)? {
                // A previously authenticated retirement was deleted. Never
                // re-seal mutable legacy metadata over that missing record.
                return Err(StoreError::ChoiceLoopStateConflict);
            }
            // These old plaintext rows were not encrypted or bound to their
            // source request. Do not upgrade mutable metadata into source
            // provenance. Instead create an authenticated, body-free blocked
            // marker whose digest is rechecked against the legacy row at each
            // load. It keeps replay/effect authority closed while allowing
            // the Store (and Global Off/recovery) to remain reachable.
            let marker = legacy_private_body_marker_digest(&json!({
                "sourceKind": source_kind,
                "entityId": entity_id,
                "requestDigest": request_digest,
                "bodyDigest": body_digest,
                "choiceSessionId": choice_session_id,
                "retiredAtMs": retired_at_ms,
            }));
            persist_choice_private_body_retirement(
                &transaction,
                &self.authority,
                &ChoicePrivateBodyRetirement {
                    source_kind,
                    entity_id,
                    request_digest: None,
                    body_digest: marker.clone(),
                    choice_session_id: "legacy-retirement-blocked".to_owned(),
                    source_blob_hash: marker,
                    retired_at_ms: 0,
                    legacy_blocked: true,
                },
            )?;
        }
        let refinement_rows = transaction
            .prepare(
                "SELECT operation_id, result_digest, choice_session_id, retired_at_ms
                 FROM choice_refinement_body_tombstone",
            )?
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        for (entity_id, body_digest, choice_session_id, retired_at_ms) in refinement_rows {
            let exists: bool = transaction.query_row(
                "SELECT EXISTS(SELECT 1 FROM choice_private_body_retirement
                 WHERE source_kind = 'refinement' AND entity_id = ?1)",
                [&entity_id],
                |row| row.get(0),
            )?;
            if exists {
                continue;
            }
            if retirement_audit_exists(&transaction, "refinement", &entity_id)? {
                return Err(StoreError::ChoiceLoopStateConflict);
            }
            let marker = legacy_private_body_marker_digest(&json!({
                "sourceKind": "refinement",
                "entityId": entity_id,
                "bodyDigest": body_digest,
                "choiceSessionId": choice_session_id,
                "retiredAtMs": retired_at_ms,
            }));
            persist_choice_private_body_retirement(
                &transaction,
                &self.authority,
                &ChoicePrivateBodyRetirement {
                    source_kind: "refinement".to_owned(),
                    entity_id,
                    request_digest: None,
                    body_digest: marker.clone(),
                    choice_session_id: "legacy-retirement-blocked".to_owned(),
                    source_blob_hash: marker,
                    retired_at_ms: 0,
                    legacy_blocked: true,
                },
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    /// A pre-Choice-binding snapshot cannot safely be resumed. Persist one
    /// typed blocked recovery projection rather than inferring a delivery
    /// binding from owner, provider, body, timestamp, or another mutable
    /// field. This runs inside the Store migration transaction so restart
    /// observes the same audited blocked state instead of repeatedly deriving
    /// a transient UI-only warning.
    fn migrate_choice_loop_delivery_bindings(&mut self) -> Result<(), StoreError> {
        let raw = load_raw_choice_loop_snapshot(&self.connection, &self.authority)?;
        if !raw
            .as_ref()
            .is_some_and(raw_choice_loop_batch_lacks_delivery_binding)
        {
            return Ok(());
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        verified_audit_tail(&transaction, &self.authority)?;
        let raw = load_raw_choice_loop_snapshot(&transaction, &self.authority)?;
        if !raw
            .as_ref()
            .is_some_and(raw_choice_loop_batch_lacks_delivery_binding)
        {
            transaction.commit()?;
            return Ok(());
        }
        let snapshot = load_choice_loop_snapshot(&transaction, &self.authority)?
            .ok_or(StoreError::ChoiceLoopStateConflict)?;
        let updated_at_ms = current_unix_ms()?;
        persist_choice_loop_snapshot(&transaction, &self.authority, &snapshot, updated_at_ms)?;
        transaction.commit()?;
        Ok(())
    }

    fn migrate_legacy_channel_origins(&mut self) -> Result<(), StoreError> {
        let exists = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'channel_mission_origin')",
            [],
            |row| row.get::<_, bool>(0),
        )?;
        if !exists {
            return Ok(());
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        verified_audit_tail(&transaction, &self.authority)?;
        let mission_ids = transaction
            .prepare("SELECT mission_id FROM channel_mission_origin ORDER BY mission_id")?
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        if mission_ids.is_empty() {
            transaction.execute("DROP TABLE channel_mission_origin", [])?;
            transaction.commit()?;
            return Ok(());
        }
        for mission_id in mission_ids {
            if load_channel_route_set(&transaction, &self.authority, &mission_id)?.is_some() {
                return Err(StoreError::ChannelRouteConflict);
            }
            let legacy = load_legacy_channel_origin(&transaction, &self.authority, &mission_id)?
                .ok_or(StoreError::ChannelRouteConflict)?;
            let mission = load_mission_for_update(&transaction, &self.authority, &mission_id)?
                .ok_or(StoreError::MissionNotFound)?;
            let pairing = load_channel_pairing(&transaction, &self.authority, legacy.channel)?
                .ok_or(StoreError::ChannelRouteConflict)?;
            if pairing.conversation_id != legacy.conversation_id
                || pairing.owner_sender_id != legacy.owner_sender_id
            {
                return Err(StoreError::ChannelRouteConflict);
            }
            let route_id = channel_route_id(
                &legacy.mission_id,
                legacy.channel,
                &legacy.conversation_id,
                &legacy.owner_sender_id,
            );
            let approval_id = format!(
                "route-approval-{:x}",
                Sha256::digest(format!("legacy:{}", legacy.mission_id))
            );
            let audit_id = format!("channel-route-{}-1", legacy.mission_id);
            let route_set = ChannelRouteSet {
                mission_id: legacy.mission_id.clone(),
                revision: 1,
                primary_route_id: route_id.clone(),
                routes: vec![ChannelRoute {
                    route_id,
                    role: ChannelRouteRole::Primary,
                    channel: legacy.channel,
                    conversation_id: legacy.conversation_id,
                    owner_sender_id: legacy.owner_sender_id,
                    provider_identity: pairing_provider_identity(&pairing),
                    source_message_id: Some(legacy.source_message_id),
                    allowed_inbound_classes: vec![
                        ChannelInboundMessageClass::MissionParticipation,
                        ChannelInboundMessageClass::NeedYouResponse,
                    ],
                    allowed_outbound_classes: vec![
                        ChannelMessageKind::NeedYou,
                        ChannelMessageKind::Progress,
                        ChannelMessageKind::Receipt,
                    ],
                    revision: 1,
                    approval_id,
                    audit_id,
                    bound_at_ms: legacy.bound_at_ms,
                    updated_at_ms: legacy.bound_at_ms,
                }],
            };
            write_channel_route_set(&transaction, &self.authority, &route_set, &mission.owner_id)?;
        }
        transaction.execute(
            "ALTER TABLE channel_mission_origin RENAME TO channel_mission_origin_legacy",
            [],
        )?;
        transaction.commit()?;
        Ok(())
    }

    fn migrate_channel_failure_incidents(&mut self) -> Result<(), StoreError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        verified_audit_tail(&transaction, &self.authority)?;
        let runtime_revision =
            load_runtime_control(&transaction, &self.authority, self.trusted_broker.as_ref())?
                .revision;
        let failures = transaction
            .prepare(
                "SELECT channel_json, source_message_id, blob_hash
                 FROM channel_model_dispatch
                 WHERE status_json = 'failed' AND suggestion_id IS NULL
                 ORDER BY channel_json, source_message_id",
            )?
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        for (encoded_channel, source_message_id, dispatch_state_hash) in failures {
            let channel: ChannelKind = serde_json::from_str(&encoded_channel)
                .map_err(|_| StoreError::ChannelStateMismatch(source_message_id.clone()))?;
            let dispatch = load_channel_model_dispatch(
                &transaction,
                &self.authority,
                channel,
                &source_message_id,
            )?
            .ok_or_else(|| StoreError::ChannelStateMismatch(source_message_id.clone()))?;
            if dispatch.state != StoredChannelModelState::Failed {
                return Err(StoreError::ChannelStateMismatch(source_message_id));
            }
            let incident_id = channel_failure_incident_id(
                channel,
                &source_message_id,
                &dispatch_state_hash,
                ChannelFailureClass::ModelResultUnavailable,
            );
            if load_channel_failure_incident(&transaction, &self.authority, &incident_id)?.is_some()
            {
                continue;
            }
            let (source_audit_anchor, occurred_at_ms) = channel_failure_source_anchor(
                &transaction,
                channel,
                &source_message_id,
                &dispatch_state_hash,
            )?;
            write_channel_failure_incident(
                &transaction,
                &self.authority,
                &StoredChannelFailureIncident {
                    incident_id,
                    channel,
                    source_message_id,
                    failure_class: ChannelFailureClass::ModelResultUnavailable,
                    occurred_at_ms,
                    runtime_revision,
                    dispatch_state_hash,
                    source_audit_anchor,
                    acknowledgement: None,
                },
            )?;
        }
        transaction.commit()?;
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

    /// Persists the explicit user-owned model selection with its catalog and
    /// account provenance. Exact retries are idempotent; a changed selection
    /// creates a new audited state and never silently substitutes a model.
    ///
    /// # Errors
    ///
    /// Returns an error for malformed selection data, failed Store/audit
    /// verification, or a failed atomic commit.
    pub fn select_model_selection(
        &mut self,
        selection: &ModelSelection,
        selected_at_ms: i64,
    ) -> Result<ModelSelection, StoreError> {
        if selected_at_ms < 0 || !selection.is_valid() {
            return Err(StoreError::ChoiceModelSelectionConflict);
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_no_effect_fence(&transaction)?;
        verified_audit_tail(&transaction, &self.authority)?;
        verify_all_bindings(&transaction, &self.authority, self.trusted_broker.as_ref())?;
        if let Some(existing) = load_choice_model_selection(&transaction, &self.authority)?
            && existing == *selection
        {
            return Ok(existing);
        }
        let blob = self
            .authority
            .encrypt_json(selection, choice_model_selection_aad().as_bytes())?;
        let selection_hash = format!(
            "{:x}",
            Sha256::digest(
                serde_json::to_vec(selection)
                    .map_err(|error| CryptoError::Serialization(error.to_string()))?
            )
        );
        let encrypted_blob_hash = blob_hash(&blob);
        let audit_id = format!("choice-model-selection-{selection_hash}-{selected_at_ms}");
        transaction.execute(
            "INSERT INTO choice_model_selection (singleton_id, encrypted_blob, blob_hash, selected_at_ms)
             VALUES (1, ?1, ?2, ?3)
             ON CONFLICT(singleton_id) DO UPDATE SET encrypted_blob = excluded.encrypted_blob,
                 blob_hash = excluded.blob_hash, selected_at_ms = excluded.selected_at_ms",
            params![blob, encrypted_blob_hash, selected_at_ms],
        )?;
        append_audit(
            &transaction,
            &self.authority,
            &AuditRecord {
                id: &audit_id,
                command_id: &audit_id,
                command_hash: &selection_hash,
                actor: "owner",
                action: CHOICE_MODEL_SELECTION_ACTION,
                entity_id: "choice-model-selection",
                created_at_ms: selected_at_ms,
                state_kind: "choice:model_selection",
                state_hash: &encrypted_blob_hash,
            },
        )?;
        transaction.commit()?;
        Ok(selection.clone())
    }

    /// Loads a verified persisted model selection. An absent record means the
    /// product must ask the owner to choose; it never falls back to a fixed
    /// model or effort.
    ///
    /// # Errors
    ///
    /// Returns an error for a malformed encrypted record or audit binding.
    pub fn selected_model_selection(&self) -> Result<Option<ModelSelection>, StoreError> {
        verified_audit_tail(&self.connection, &self.authority)?;
        load_choice_model_selection(&self.connection, &self.authority)
    }

    /// Returns the complete encrypted foreground Choice Loop state. Its
    /// contents are continuity data only; callers must still pass the normal
    /// typed confirmation and broker gates before any external effect.
    ///
    /// # Errors
    ///
    /// Returns an error for malformed encrypted state or audit binding.
    pub fn choice_loop_snapshot(&self) -> Result<Option<ChoiceLoopSnapshot>, StoreError> {
        verified_audit_tail(&self.connection, &self.authority)?;
        let snapshot = load_choice_loop_snapshot(&self.connection, &self.authority)?;
        verify_choice_private_bindings(&self.connection, &self.authority, snapshot.as_ref())?;
        verify_choice_markdown_bindings(&self.connection, &self.authority, snapshot.as_ref())?;
        Ok(snapshot)
    }

    /// Derives and persists the encrypted Markdown body and command-owned
    /// render intent before the Host touches the filesystem. Production
    /// callers supply only the already sealed confirmation and the
    /// descriptor-observed replacement baseline; the Store derives the body,
    /// entry, digest, and intent identity itself.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid or stale intent, a mismatched body,
    /// runtime/Choice state drift, audit failure, or a transaction conflict.
    #[cfg(test)]
    pub fn begin_confirmed_markdown_render(
        &mut self,
        confirmation: &ChoiceConsolidatedConfirmation,
        expected_generation: u64,
        expected_base: Option<MarkdownBaseIdentity>,
        created_at_ms: i64,
    ) -> Result<MarkdownRenderIntent, StoreError> {
        if !confirmation.is_valid() || expected_generation == 0 || created_at_ms < 0 {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_no_effect_fence(&transaction)?;
        verified_audit_tail(&transaction, &self.authority)?;
        verify_all_bindings(&transaction, &self.authority, self.trusted_broker.as_ref())?;
        let current = load_choice_loop_snapshot(&transaction, &self.authority)?
            .ok_or(StoreError::ChoiceLoopStateConflict)?;
        let runtime =
            load_runtime_control(&transaction, &self.authority, self.trusted_broker.as_ref())?;
        if !runtime.enabled
            || runtime.revision != expected_generation
            || current.session.id != confirmation.choice_session_id
            || current.session.state != ChoiceSessionState::AwaitingConfirmation
            || current.confirmation.as_ref() != Some(confirmation)
            || current.session.pending_confirmation_id.as_deref() != Some(&confirmation.id)
            || confirmation.expected_session_revision.checked_add(1)
                != Some(current.session.revision)
        {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        let plaintext_body = confirmed_choice_markdown_body(confirmation);
        let entry = confirmation.markdown_entry.clone();
        if entry.relative_path != format!("sessions/{}/CHOICE.md", current.session.id)
            || entry.sha256 != sha256_hex(plaintext_body.as_bytes())
            || entry.byte_length
                != u64::try_from(plaintext_body.len())
                    .map_err(|_| StoreError::ChoiceLoopStateConflict)?
            || entry.mode != 0o600
            || expected_base != confirmation.markdown_expected_base
        {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        let intent = MarkdownRenderIntent {
            id: markdown_render_intent_id(
                confirmation,
                current.session.revision,
                expected_generation,
                &entry.sha256,
            ),
            choice_session_id: current.session.id.clone(),
            expected_session_revision: current.session.revision,
            expected_generation,
            entry: entry.clone(),
            expected_base,
            content_digest: entry.sha256.clone(),
            created_at_ms,
        };
        if let Some(existing) =
            load_markdown_render_intent(&transaction, &self.authority, &intent.id)?
        {
            return if existing.intent == intent
                && existing.plaintext_body.as_deref() == Some(&plaintext_body)
            {
                Ok(intent)
            } else {
                Err(StoreError::ChoiceLoopStateConflict)
            };
        }
        persist_markdown_render_intent(
            &transaction,
            &self.authority,
            &StoredMarkdownRenderIntent {
                intent: intent.clone(),
                plaintext_body: Some(plaintext_body),
                reconciliation: None,
            },
        )?;
        transaction.commit()?;
        Ok(intent)
    }

    /// Lower-level fixture seam. Production callers must use the sealed
    /// confirmation variant above; this is not compiled into the product.
    #[cfg(test)]
    fn begin_markdown_render_for_test(
        &mut self,
        intent: &MarkdownRenderIntent,
        plaintext_body: &str,
    ) -> Result<MarkdownRenderIntent, StoreError> {
        self.begin_markdown_render_inner(intent, plaintext_body, None)
    }

    #[cfg(test)]
    fn begin_markdown_render_inner(
        &mut self,
        intent: &MarkdownRenderIntent,
        plaintext_body: &str,
        confirmation: Option<&ChoiceConsolidatedConfirmation>,
    ) -> Result<MarkdownRenderIntent, StoreError> {
        if !intent.is_valid()
            || plaintext_body.len() as u64 != intent.entry.byte_length
            || sha256_hex(plaintext_body.as_bytes()) != intent.content_digest
            || plaintext_body.contains('\0')
        {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_no_effect_fence(&transaction)?;
        verified_audit_tail(&transaction, &self.authority)?;
        verify_all_bindings(&transaction, &self.authority, self.trusted_broker.as_ref())?;
        if let Some(existing) =
            load_markdown_render_intent(&transaction, &self.authority, &intent.id)?
        {
            return if existing.intent == *intent
                && existing.plaintext_body.as_deref() == Some(plaintext_body)
            {
                Ok(intent.clone())
            } else {
                Err(StoreError::ChoiceLoopStateConflict)
            };
        }
        let current = load_choice_loop_snapshot(&transaction, &self.authority)?
            .ok_or(StoreError::ChoiceLoopStateConflict)?;
        let runtime =
            load_runtime_control(&transaction, &self.authority, self.trusted_broker.as_ref())?;
        if !runtime.enabled
            || runtime.revision != intent.expected_generation
            || current.session.id != intent.choice_session_id
            || current.session.revision != intent.expected_session_revision
            || confirmation.is_some_and(|value| {
                current.session.state != ChoiceSessionState::AwaitingConfirmation
                    || current.confirmation.as_ref() != Some(value)
                    || current.session.pending_confirmation_id.as_deref() != Some(&value.id)
                    || value.expected_session_revision.checked_add(1)
                        != Some(current.session.revision)
            })
            || (confirmation.is_some()
                && (current.session.state != ChoiceSessionState::AwaitingConfirmation
                    || current.confirmation.is_none()))
        {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        persist_markdown_render_intent(
            &transaction,
            &self.authority,
            &StoredMarkdownRenderIntent {
                intent: intent.clone(),
                plaintext_body: Some(plaintext_body.to_owned()),
                reconciliation: None,
            },
        )?;
        transaction.commit()?;
        Ok(intent.clone())
    }

    /// Loads the encrypted render body for the private Host renderer.  This
    /// is not a product RPC and never returns a filesystem path supplied by a
    /// caller.  The intent remains durable until a verified receipt exists so
    /// a crash can resume or reconcile without recreating user text.
    ///
    /// # Errors
    ///
    /// Returns an error for a missing, malformed, or audit-unbound intent.
    #[cfg(test)]
    fn private_markdown_body_available_for_test(
        &self,
        intent_id: &str,
        allow_reconciliation: bool,
    ) -> Result<bool, StoreError> {
        verified_audit_tail(&self.connection, &self.authority)?;
        load_markdown_render_intent(&self.connection, &self.authority, intent_id).and_then(
            |record| {
                record
                    .map(|record| {
                        if record.reconciliation.is_some() && !allow_reconciliation {
                            return Err(StoreError::ChoiceLoopStateConflict);
                        }
                        if record.plaintext_body.is_some() {
                            Ok(true)
                        } else {
                            Err(StoreError::ChoiceLoopStateConflict)
                        }
                    })
                    .transpose()
                    .map(|value| value.unwrap_or(false))
            },
        )
    }

    /// Publishes one durable Store-owned render intent through a descriptor
    /// root. The plaintext never crosses the Store boundary: it is decrypted
    /// into a bounded zeroizing buffer only for this internal writer call.
    ///
    /// This is deliberately not a raw filesystem API. `intent_id` must name
    /// an existing encrypted command-owned journal, and the final path and
    /// manifest are derived from that journal rather than caller input.
    ///
    /// # Errors
    ///
    /// Returns an error when the journal, audit, private root, or descriptor
    /// publication cannot be verified without retaining or exposing plaintext.
    pub fn publish_markdown_render_intent(
        &mut self,
        intent_id: &str,
        allow_reconciliation: bool,
        committed_at_ms: i64,
    ) -> Result<Option<MarkdownRenderPublication>, StoreError> {
        if committed_at_ms < 0 {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        let root = self.open_choice_markdown_root()?;
        // Keep the IMMEDIATE transaction open across publication. This is not
        // merely a preflight: Global Off and all competing Choice transitions
        // are serialized until the descriptor-bound publication either gains
        // its receipt or fails closed.
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_no_effect_fence(&transaction)?;
        verified_audit_tail(&transaction, &self.authority)?;
        verify_all_bindings(&transaction, &self.authority, self.trusted_broker.as_ref())?;
        let Some(record) = load_markdown_render_intent(&transaction, &self.authority, intent_id)?
        else {
            return Ok(None);
        };
        if record.reconciliation.is_some() && !allow_reconciliation {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        require_current_markdown_render_authority(
            &transaction,
            &self.authority,
            self.trusted_broker.as_ref(),
            &record.intent,
        )?;
        let plaintext_body = record
            .plaintext_body
            .ok_or(StoreError::ChoiceLoopStateConflict)?;
        let outcome = root
            .render_no_clobber(
                &record.intent,
                Zeroizing::new(plaintext_body).as_bytes(),
                None,
                committed_at_ms,
            )
            .map_err(|_| StoreError::ChoiceLoopStateConflict)?;
        Ok(Some(match outcome {
            MarkdownRenderOutcome::Committed(receipt) => {
                // Publication is not a receipt. Re-open the exact final/base
                // descriptors while the Store still owns the only plaintext
                // capability, then persist the receipt in its own protected
                // transaction. No external caller can manufacture this step.
                if root
                    .verify_committed_receipt(&record.intent, &receipt)
                    .is_err()
                {
                    transaction.commit()?;
                    self.record_markdown_reconciliation(intent_id, committed_at_ms)?;
                    MarkdownRenderPublication::ReconciliationRequired
                } else {
                    persist_markdown_render_receipt(
                        &transaction,
                        &self.authority,
                        &record.intent,
                        &receipt,
                    )?;
                    transaction.commit()?;
                    MarkdownRenderPublication::Committed(receipt)
                }
            }
            MarkdownRenderOutcome::ReconciliationRequired => {
                transaction.commit()?;
                self.record_markdown_reconciliation(intent_id, committed_at_ms)?;
                MarkdownRenderPublication::ReconciliationRequired
            }
        }))
    }

    /// Re-opens and verifies an already durable receipt through the Store's
    /// descriptor boundary, then removes only its retained displaced base.
    /// A mismatched final/base becomes durable typed reconciliation instead of
    /// an externally callable journal mutation.
    ///
    /// # Errors
    ///
    /// Returns an error when the journal cannot be authenticated or retained
    /// base cleanup cannot complete without an ambiguous filesystem state.
    fn verify_and_cleanup_markdown_render_receipt(
        &mut self,
        intent_id: &str,
        recorded_at_ms: i64,
    ) -> Result<MarkdownReceiptCleanup, StoreError> {
        let root = self.open_choice_markdown_root()?;
        // Validate all signed authority and the exact receipt/session binding
        // before touching the retained Owner base. A corrupted row must leave
        // both files and encrypted bodies intact for typed reconciliation.
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_no_effect_fence(&transaction)?;
        verified_audit_tail(&transaction, &self.authority)?;
        verify_all_bindings(&transaction, &self.authority, self.trusted_broker.as_ref())?;
        let intent = load_markdown_render_intent(&transaction, &self.authority, intent_id)?
            .map(|record| record.intent)
            .ok_or(StoreError::ChoiceLoopStateConflict)?;
        let receipt = load_markdown_render_receipt(&transaction, &self.authority, intent_id)?
            .ok_or(StoreError::ChoiceLoopStateConflict)?;
        require_current_markdown_cleanup_authority(
            &transaction,
            &self.authority,
            self.trusted_broker.as_ref(),
            &intent,
            &receipt,
        )?;
        if root.verify_committed_receipt(&intent, &receipt).is_err() {
            transaction.commit()?;
            self.record_markdown_reconciliation(intent_id, recorded_at_ms)?;
            return Ok(MarkdownReceiptCleanup::ReconciliationRequired);
        }
        root.cleanup_displaced_base(&intent, &receipt)
            .map_err(|_| StoreError::ChoiceLoopStateConflict)?;
        transaction.commit()?;
        Ok(MarkdownReceiptCleanup::ReadyForRetirement)
    }

    /// Completes only the already-receipted Markdown cleanup path. It derives
    /// the fixed private root, re-verifies final/base descriptors, performs
    /// retained-base cleanup, and retires private bodies as one sealed Store
    /// authority chain. Callers never receive a raw receipt, root, or body.
    ///
    /// # Errors
    ///
    /// Returns an error for an unauthenticated journal, failed durable state
    /// transition, or a filesystem state that cannot be safely classified.
    pub fn complete_verified_markdown_render_cleanup(
        &mut self,
        intent_id: &str,
        completed_at_ms: i64,
    ) -> Result<MarkdownRenderCleanup, StoreError> {
        match self.verify_and_cleanup_markdown_render_receipt(intent_id, completed_at_ms)? {
            MarkdownReceiptCleanup::ReconciliationRequired => {
                Ok(MarkdownRenderCleanup::ReconciliationRequired)
            }
            MarkdownReceiptCleanup::ReadyForRetirement => self
                .retire_choice_private_bodies_after_render(intent_id, completed_at_ms)
                .map(|snapshot| MarkdownRenderCleanup::Retired(Box::new(snapshot))),
        }
    }

    /// Observes only the exact Host-created `~/Documents/OpenOpen` entry that
    /// a command-owned confirmation may replace. The root is derived from the
    /// current signed-in local account, never from RPC or a caller path.
    ///
    /// # Errors
    ///
    /// Returns an error if the canonical private root or its descriptor-bound
    /// entry cannot be verified.
    pub fn observe_choice_markdown_base(
        &self,
        relative_path: &str,
    ) -> Result<Option<MarkdownBaseIdentity>, StoreError> {
        let root = self
            .choice_markdown_root
            .as_ref()
            .ok_or(StoreError::ChoiceLoopStateConflict)?;
        // Confirmation preview is read-only. A missing private root is an
        // ordinary absent base, not permission for preview to create
        // `~/Documents/OpenOpen`; only the confirmed render worker may do
        // that. An existing but malformed root remains fail-closed.
        match std::fs::symlink_metadata(root) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(_) => Err(StoreError::ChoiceLoopStateConflict),
            Ok(_) => MarkdownRoot::open(root)
                .and_then(|markdown_root| markdown_root.observe_existing_entry(relative_path))
                .map_err(|_| StoreError::ChoiceLoopStateConflict),
        }
    }

    fn open_choice_markdown_root(&self) -> Result<MarkdownRoot, StoreError> {
        let root = self
            .choice_markdown_root
            .as_ref()
            .ok_or(StoreError::ChoiceLoopStateConflict)?;
        MarkdownRoot::open(root).map_err(|_| StoreError::ChoiceLoopStateConflict)
    }

    /// Records one durable, body-free reconciliation requirement. It does not
    /// overwrite an Owner edit or retry publication; only the dedicated Host
    /// reconciliation command can clear this marker for one explicit retry.
    ///
    /// # Errors
    ///
    /// Returns an error for an unknown/tampered intent, audit failure, or an
    /// attempt to replace a different durable reconciliation marker.
    fn record_markdown_reconciliation(
        &mut self,
        intent_id: &str,
        recorded_at_ms: i64,
    ) -> Result<(), StoreError> {
        if recorded_at_ms < 0 {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_no_effect_fence(&transaction)?;
        verified_audit_tail(&transaction, &self.authority)?;
        verify_all_bindings(&transaction, &self.authority, self.trusted_broker.as_ref())?;
        let mut record = load_markdown_render_intent(&transaction, &self.authority, intent_id)?
            .ok_or(StoreError::ChoiceLoopStateConflict)?;
        let current = load_choice_loop_snapshot(&transaction, &self.authority)?
            .ok_or(StoreError::ChoiceLoopStateConflict)?;
        let exact_pre_receipt_intent = current.session.id == record.intent.choice_session_id
            && current.session.revision == record.intent.expected_session_revision
            && matches!(
                current.session.state,
                ChoiceSessionState::Active | ChoiceSessionState::AwaitingConfirmation
            );
        let executing_after_receipt = current.session.id == record.intent.choice_session_id
            && current.session.revision
                == record
                    .intent
                    .expected_session_revision
                    .checked_add(1)
                    .ok_or(StoreError::ChoiceLoopStateConflict)?
            && current.session.state == ChoiceSessionState::Executing;
        if !exact_pre_receipt_intent && !executing_after_receipt {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        let marker = StoredMarkdownReconciliation {
            reason: "descriptor-conflict".to_owned(),
            recorded_at_ms,
        };
        if record.reconciliation.as_ref() == Some(&marker) {
            return Ok(());
        }
        if record.reconciliation.is_some() {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        record.reconciliation = Some(marker);
        update_markdown_render_intent(
            &transaction,
            &self.authority,
            &record,
            "reconciliation",
            recorded_at_ms,
        )?;
        if current.session.state == ChoiceSessionState::AwaitingConfirmation {
            let mut next = current.clone();
            next.session.revision = next
                .session
                .revision
                .checked_add(1)
                .ok_or(StoreError::ChoiceLoopStateConflict)?;
            next.session.state = ChoiceSessionState::Active;
            next.session.last_input_at_ms = recorded_at_ms.max(next.session.last_input_at_ms);
            next.session.soft_idle_at_ms = next.session.last_input_at_ms + 1_800_000;
            next.session.stale_review_at_ms = next.session.last_input_at_ms + 86_400_000;
            next.session.pending_confirmation_id = None;
            next.confirmation = None;
            let mut recovery_set = current
                .active_choice_set
                .clone()
                .ok_or(StoreError::ChoiceLoopStateConflict)?;
            recovery_set.id = format!(
                "reconfirm-{}",
                &sha256_hex(format!("{}:{}", record.intent.id, next.session.revision).as_bytes())
                    [..32]
            );
            recovery_set.session_revision = next.session.revision;
            recovery_set.generated_at_ms = recorded_at_ms;
            recovery_set.expires_on_revision = next.session.revision;
            next.session.active_choice_set_id = Some(recovery_set.id.clone());
            next.active_choice_set = Some(recovery_set);
            if !next.is_permitted_successor_of(&current) {
                return Err(StoreError::ChoiceLoopStateConflict);
            }
            persist_choice_loop_snapshot(&transaction, &self.authority, &next, recorded_at_ms)?;
        }
        transaction.commit()?;
        Ok(())
    }

    /// Loads only command-owned metadata for restart cleanup after the raw
    /// render body has been retired.  It is not a caller-supplied filesystem
    /// API and cannot recreate a render without a retained encrypted body.
    /// Returns the durable metadata for one command-owned render intent.
    ///
    /// # Errors
    ///
    /// Returns an error when the Store cannot open or validate the protected
    /// render record.
    pub fn markdown_render_intent(
        &self,
        intent_id: &str,
    ) -> Result<Option<MarkdownRenderIntent>, StoreError> {
        verified_audit_tail(&self.connection, &self.authority)?;
        load_markdown_render_intent(&self.connection, &self.authority, intent_id)
            .map(|record| record.map(|record| record.intent))
    }

    /// Returns the sole pending private render intent for a Choice session.
    /// It exposes metadata only; callers never receive the encrypted body.
    ///
    /// # Errors
    ///
    /// Returns an error for an unauditable/malformed journal or more than one
    /// pending journal for the same foreground session.
    pub fn pending_markdown_render_intent_for_session(
        &self,
        choice_session_id: &str,
    ) -> Result<Option<MarkdownRenderIntent>, StoreError> {
        verified_audit_tail(&self.connection, &self.authority)?;
        let intent_ids = self
            .connection
            .prepare("SELECT intent_id FROM choice_markdown_render_intent")?
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        let mut matched = Vec::new();
        for intent_id in intent_ids {
            let stored =
                load_markdown_render_intent(&self.connection, &self.authority, &intent_id)?
                    .ok_or(StoreError::ChoiceLoopStateConflict)?;
            if stored.intent.choice_session_id == choice_session_id
                && stored.plaintext_body.is_some()
            {
                matched.push(stored.intent);
            }
        }
        match matched.len() {
            0 => Ok(None),
            1 => Ok(matched.pop()),
            _ => Err(StoreError::ChoiceLoopStateConflict),
        }
    }

    /// Loads an already durable receipt for idempotent private render replay.
    /// It does not imply that the retained displaced base has been cleaned.
    ///
    /// # Errors
    ///
    /// Returns an error for a malformed encrypted receipt or audit binding.
    pub fn markdown_render_receipt(
        &self,
        intent_id: &str,
    ) -> Result<Option<MarkdownRenderReceipt>, StoreError> {
        verified_audit_tail(&self.connection, &self.authority)?;
        load_markdown_render_receipt(&self.connection, &self.authority, intent_id)
    }

    /// Lower-level fixture seam. Production receipt commits always require a
    /// sealed awaiting confirmation.
    #[cfg(test)]
    fn commit_markdown_render_receipt_for_test(
        &mut self,
        receipt: &MarkdownRenderReceipt,
    ) -> Result<MarkdownRenderReceipt, StoreError> {
        self.commit_markdown_render_receipt_inner(receipt, false)
    }

    #[cfg(test)]
    fn commit_markdown_render_receipt_inner(
        &mut self,
        receipt: &MarkdownRenderReceipt,
        require_confirmation: bool,
    ) -> Result<MarkdownRenderReceipt, StoreError> {
        if !receipt.is_valid() {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_no_effect_fence(&transaction)?;
        verified_audit_tail(&transaction, &self.authority)?;
        verify_all_bindings(&transaction, &self.authority, self.trusted_broker.as_ref())?;
        if let Some(existing) =
            load_markdown_render_receipt(&transaction, &self.authority, &receipt.intent_id)?
        {
            return if existing == *receipt {
                Ok(receipt.clone())
            } else {
                Err(StoreError::ChoiceLoopStateConflict)
            };
        }
        let stored =
            load_markdown_render_intent(&transaction, &self.authority, &receipt.intent_id)?
                .ok_or(StoreError::ChoiceLoopStateConflict)?;
        if receipt.final_entry != stored.intent.entry
            || receipt.displaced_base != stored.intent.expected_base
            || receipt.committed_at_ms < stored.intent.created_at_ms
        {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        let runtime =
            load_runtime_control(&transaction, &self.authority, self.trusted_broker.as_ref())?;
        let current = load_choice_loop_snapshot(&transaction, &self.authority)?
            .ok_or(StoreError::ChoiceLoopStateConflict)?;
        if !runtime.enabled
            || runtime.revision != stored.intent.expected_generation
            || current.session.id != stored.intent.choice_session_id
            || current.session.revision != stored.intent.expected_session_revision
            || (require_confirmation
                && (current.session.state != ChoiceSessionState::AwaitingConfirmation
                    || current.confirmation.is_none()
                    || current.session.pending_confirmation_id.is_none()))
        {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        persist_markdown_render_receipt(&transaction, &self.authority, &stored.intent, receipt)?;
        transaction.commit()?;
        Ok(receipt.clone())
    }

    /// Retires raw begin/D input only after a matching receipt has committed
    /// under the current signed runtime.  The renderer calls this only after
    /// its descriptor-relative cleanup has completed; a failure leaves the
    /// encrypted body and receipt available for safe restart reconciliation.
    ///
    /// # Errors
    ///
    /// Returns an error for a missing receipt, runtime/session drift, or an
    /// incomplete render journal.  It never deletes bodies on ambiguity.
    #[allow(clippy::too_many_lines)] // One IMMEDIATE transaction keeps receipt verification, retirement, and the next state atomic.
    fn retire_choice_private_bodies_after_render(
        &mut self,
        intent_id: &str,
        retired_at_ms: i64,
    ) -> Result<ChoiceLoopSnapshot, StoreError> {
        if retired_at_ms < 0 {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_no_effect_fence(&transaction)?;
        verified_audit_tail(&transaction, &self.authority)?;
        verify_all_bindings(&transaction, &self.authority, self.trusted_broker.as_ref())?;
        let stored = load_markdown_render_intent(&transaction, &self.authority, intent_id)?
            .ok_or(StoreError::ChoiceLoopStateConflict)?;
        let receipt = load_markdown_render_receipt(&transaction, &self.authority, intent_id)?
            .ok_or(StoreError::ChoiceLoopStateConflict)?;
        let current = load_choice_loop_snapshot(&transaction, &self.authority)?
            .ok_or(StoreError::ChoiceLoopStateConflict)?;
        if stored.plaintext_body.is_none() {
            let already_retired = current.session.id == stored.intent.choice_session_id
                && matches!(
                    current.session.state,
                    ChoiceSessionState::Executing | ChoiceSessionState::SoftIdle
                )
                && current.session.revision
                    == stored
                        .intent
                        .expected_session_revision
                        .checked_add(1)
                        .ok_or(StoreError::ChoiceLoopStateConflict)?
                && receipt.final_entry == stored.intent.entry
                && retired_at_ms >= receipt.committed_at_ms;
            let cancelled_after_off = current.session.id == stored.intent.choice_session_id
                && current.session.state == ChoiceSessionState::Cancelled
                && current.session.revision
                    == stored
                        .intent
                        .expected_session_revision
                        .checked_add(1)
                        .ok_or(StoreError::ChoiceLoopStateConflict)?
                && receipt.final_entry == stored.intent.entry
                && retired_at_ms >= receipt.committed_at_ms;
            if !already_retired && !cancelled_after_off {
                return Err(StoreError::ChoiceLoopStateConflict);
            }
            transaction.commit()?;
            return Ok(current);
        }
        if current.session.id != stored.intent.choice_session_id
            || current.session.revision != stored.intent.expected_session_revision
            || receipt.final_entry != stored.intent.entry
            || retired_at_ms < receipt.committed_at_ms
        {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        // Receipt-authenticated body retirement is a local deletion-only
        // cleanup. It must remain resumable after protected Off/revision
        // advancement; it neither starts work nor grants any effect.
        purge_choice_private_bodies(
            &transaction,
            &self.authority,
            &stored.intent.choice_session_id,
            retired_at_ms,
        )?;
        retire_choice_refinement_results(
            &transaction,
            &self.authority,
            &stored.intent.choice_session_id,
            retired_at_ms,
        )?;
        retire_choice_refinement_contexts(
            &transaction,
            &self.authority,
            &stored.intent.choice_session_id,
            retired_at_ms,
        )?;
        persist_retired_markdown_render_intent(
            &transaction,
            &self.authority,
            &stored.intent,
            retired_at_ms,
        )?;
        // A successful confirmation journal must not leave the foreground
        // path in a completed-looking but actionless confirmation state. A
        // lower-level Store fixture may exercise journal retirement without a
        // confirmation; that metadata-only path stays in its current state.
        let next = if current.session.state == ChoiceSessionState::AwaitingConfirmation
            && current.confirmation.is_some()
        {
            let mut next = current.clone();
            let confirmation = current
                .confirmation
                .as_ref()
                .ok_or(StoreError::ChoiceLoopStateConflict)?;
            let rendered_entries = vec![receipt.final_entry.clone()];
            let rendered_digest = canonical_document_manifest_digest(&rendered_entries)
                .ok_or(StoreError::ChoiceLoopStateConflict)?;
            if confirmation.markdown_manifest_digests.get(1) != Some(&rendered_digest) {
                return Err(StoreError::ChoiceLoopStateConflict);
            }
            next.session.revision = next
                .session
                .revision
                .checked_add(1)
                .ok_or(StoreError::ChoiceLoopStateConflict)?;
            next.session.state = ChoiceSessionState::SoftIdle;
            next.session.active_choice_set_id = None;
            next.session.active_interpretation_revision = None;
            next.interpretation = None;
            next.active_choice_set = None;
            // The verified Receipt and Markdown publication complete the
            // confirmed direction. The next menu must come from a new private
            // typed result, never from a revision-rewritten old ChoiceSet.
            next.session.active_choice_set_id = None;
            next.active_choice_set = None;
            next.active_batch = None;
            next.document_manifest = DocumentManifest {
                root_version: current
                    .document_manifest
                    .root_version
                    .checked_add(1)
                    .ok_or(StoreError::ChoiceLoopStateConflict)?,
                generated_at_ms: retired_at_ms,
                entries: rendered_entries,
                aggregate_digest: rendered_digest,
            };
            // Keep the sealed, effect-free confirmation as a durable replay
            // and separately-gated Reminder preparation reference. It is not
            // a Mission/effect grant, but losing it would make an ambiguous
            // confirmation response unrecoverable after the Markdown receipt.
            if !next.is_permitted_successor_of(&current) {
                return Err(StoreError::ChoiceLoopStateConflict);
            }
            persist_choice_loop_snapshot(&transaction, &self.authority, &next, retired_at_ms)?;
            next
        } else {
            current
        };
        transaction.commit()?;
        Ok(next)
    }

    /// Private Host timer transition. The scheduler supplies only an opaque
    /// wake hint; this Store command rechecks the protected runtime, exact
    /// revision, persisted deadline, and clock continuity before changing
    /// state. It never starts model or effect work.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid clock evidence, protected-runtime or
    /// session drift, ambiguous time continuity, audit failure, or a
    /// transaction conflict.
    #[cfg(test)]
    pub fn advance_choice_idle_state(
        &mut self,
        choice_session_id: &str,
        expected_session_revision: u64,
        expected_generation: u64,
        clock: &ChoiceIdleClockEvidence,
    ) -> Result<ChoiceLoopSnapshot, StoreError> {
        self.advance_choice_idle_state_classified(
            choice_session_id,
            expected_session_revision,
            expected_generation,
            clock,
        )
        .map(|outcome| outcome.snapshot().clone())
    }

    /// Applies the same private idle transition while preserving whether this
    /// sample merely calibrated, proved a continuous no-op, or changed the
    /// session. Host read and consuming paths use that distinction to prevent
    /// a first/ambiguous clock sample from authorizing a stale `ChoiceSet`.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid clock evidence, runtime/session drift,
    /// ambiguous continuity, audit failure, or a transaction conflict.
    pub fn advance_choice_idle_state_classified(
        &mut self,
        choice_session_id: &str,
        expected_session_revision: u64,
        expected_generation: u64,
        clock: &ChoiceIdleClockEvidence,
    ) -> Result<ChoiceIdleAdvance, StoreError> {
        if !clock.is_valid() || expected_session_revision == 0 || expected_generation == 0 {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_no_effect_fence(&transaction)?;
        verified_audit_tail(&transaction, &self.authority)?;
        verify_all_bindings(&transaction, &self.authority, self.trusted_broker.as_ref())?;
        let runtime =
            load_runtime_control(&transaction, &self.authority, self.trusted_broker.as_ref())?;
        let current = load_choice_loop_snapshot(&transaction, &self.authority)?
            .ok_or(StoreError::ChoiceLoopStateConflict)?;
        if !runtime.enabled
            || runtime.revision != expected_generation
            || current.session.id != choice_session_id
            || current.session.revision != expected_session_revision
            || !matches!(
                current.session.state,
                ChoiceSessionState::Active | ChoiceSessionState::SoftIdle
            )
        {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        // A first read after Store creation or a clock discontinuity is a
        // calibration point, never elapsed-time evidence. It also cannot leave
        // an old active ChoiceSet consumable: otherwise the first request would
        // fail closed, then an immediate retry could treat the new anchor as
        // continuous and consume pre-sleep/reboot authority. Invalid
        // runtime/session callers are rejected above and therefore cannot
        // launder an untrusted clock into either the anchor or session state.
        let trusted_now_ms = match classify_choice_idle_clock(&transaction, &self.authority, clock)?
        {
            ChoiceIdleClockContinuity::Calibrate { trusted_now_ms } => {
                return calibrate_choice_idle_state(
                    transaction,
                    &self.authority,
                    current,
                    clock,
                    trusted_now_ms,
                );
            }
            ChoiceIdleClockContinuity::Continuous { trusted_now_ms } => trusted_now_ms,
            ChoiceIdleClockContinuity::Uncertain => {
                return Err(StoreError::ChoiceClockUncertain);
            }
        };
        let next_state = match current.session.state {
            ChoiceSessionState::Active | ChoiceSessionState::SoftIdle
                if trusted_now_ms >= current.session.stale_review_at_ms =>
            {
                ChoiceSessionState::StaleReview
            }
            ChoiceSessionState::Active if trusted_now_ms >= current.session.soft_idle_at_ms => {
                ChoiceSessionState::SoftIdle
            }
            // A repeated scheduler wake has no authority to manufacture a
            // new revision or audit entry after the one permitted soft-idle
            // transition. Only a later stale deadline may advance it again.
            ChoiceSessionState::SoftIdle | ChoiceSessionState::Active => {
                transaction.commit()?;
                return Ok(ChoiceIdleAdvance::Unchanged(current));
            }
            _ => return Err(StoreError::ChoiceLoopStateConflict),
        };
        let mut next = current.clone();
        next.session.revision = current
            .session
            .revision
            .checked_add(1)
            .ok_or(StoreError::ChoiceLoopStateConflict)?;
        next.session.state = next_state;
        match next_state {
            ChoiceSessionState::SoftIdle => {
                // A timer has no authority to relabel old model output as a
                // fresh recap. Retire the old set; an explicit authenticated
                // owner-return command must create any later ChoiceSet.
                next.session.active_choice_set_id = None;
                next.active_choice_set = None;
            }
            ChoiceSessionState::StaleReview => {
                // Twenty-four-hour staleness retires every old option. A
                // timer cannot manufacture a fresh authenticated ChoiceSet,
                // and the prior menu must not remain selectable.
                next.session.active_choice_set_id = None;
                next.active_choice_set = None;
            }
            _ => return Err(StoreError::ChoiceLoopStateConflict),
        }
        if !next.is_permitted_successor_of(&current) {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        persist_choice_idle_clock_anchor(&transaction, &self.authority, clock, trusted_now_ms)?;
        persist_choice_loop_snapshot(&transaction, &self.authority, &next, trusted_now_ms)?;
        transaction.commit()?;
        Ok(ChoiceIdleAdvance::Transitioned(next))
    }

    /// Resolves an existing `choice.begin` request before the Host constructs
    /// any new session state. It exposes only the replay-safe acceptance
    /// projection; the encrypted local question remains Store-private.
    ///
    /// # Errors
    ///
    /// Returns an error when the encrypted/audited intake record is malformed,
    /// has a changed idempotency digest, or cannot be verified.
    pub fn choice_begin_replay(
        &self,
        request_id: &str,
        request_digest: &str,
    ) -> Result<Option<ChoiceBeginAccepted>, StoreError> {
        verified_audit_tail(&self.connection, &self.authority)?;
        match load_choice_begin_record(&self.connection, &self.authority, request_id)? {
            Some(record) if record.request_digest == request_digest => Ok(Some(record.accepted)),
            Some(_) => Err(StoreError::ChoiceBeginConflict),
            None if choice_private_body_tombstone_exists(
                &self.connection,
                &self.authority,
                "begin",
                request_id,
            )? =>
            {
                // A retired request is deliberately non-replayable. Its body
                // is gone, but the idempotency identity remains durable so a
                // restart cannot turn the same request ID into new intake.
                Err(StoreError::ChoiceBeginConflict)
            }
            None => Ok(None),
        }
    }

    /// Commits the sole public first-local-question transition. The Host has
    /// already derived every trusted envelope, batch, session, and operation
    /// field; this Store boundary accepts no caller-provided snapshot route.
    ///
    /// Exact request replays return their original accepted operation. A
    /// changed request ID, model/catalog/protocol mismatch, unresolved session,
    /// malformed initial state, or audit conflict leaves no partial record.
    ///
    /// # Errors
    ///
    /// Returns an error when the begin record cannot be atomically bound to
    /// the persisted current model selection and foreground Choice state.
    #[cfg(test)]
    pub fn begin_choice_session(
        &mut self,
        record: &ChoiceBeginRecord,
        snapshot: &ChoiceLoopSnapshot,
    ) -> Result<ChoiceBeginAccepted, StoreError> {
        self.begin_choice_session_with_clock(
            record,
            snapshot,
            &ChoiceIdleClockEvidence {
                boot_id: "test-boot".to_owned(),
                wall_clock_ms: record.accepted_at_ms,
                monotonic_ms: record.accepted_at_ms,
            },
        )
    }

    /// Commits a new first-question session together with its Host-owned clock
    /// calibration. This prevents the immediate continuity read from treating
    /// a just-accepted local question as an untrusted post-reboot sample.
    /// Exact request replay returns before the anchor write and remains
    /// strictly read-only.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid clock evidence, request/session drift,
    /// protected-runtime mismatch, replay conflict, or an atomic Store/audit
    /// failure.
    pub fn begin_choice_session_with_clock(
        &mut self,
        record: &ChoiceBeginRecord,
        snapshot: &ChoiceLoopSnapshot,
        clock: &ChoiceIdleClockEvidence,
    ) -> Result<ChoiceBeginAccepted, StoreError> {
        self.begin_choice_session_with_clock_and_cursor(record, snapshot, clock, None)
            .map(|(accepted, _)| accepted)
    }

    /// Atomically accepts one Host-derived iMessage Choice intake and advances
    /// its exact recovery cursor. Cursor validation happens before any Choice
    /// mutation so an out-of-order row cannot strand or partially create a
    /// foreground session. The boolean is true only while the exact durable
    /// initial operation still requires its private worker.
    ///
    /// # Errors
    ///
    /// Returns an error for any Choice, pairing, cursor, runtime, or replay
    /// conflict without committing a partial intake.
    pub fn begin_imessage_choice_session_with_clock(
        &mut self,
        record: &ChoiceBeginRecord,
        snapshot: &ChoiceLoopSnapshot,
        clock: &ChoiceIdleClockEvidence,
        cursor: &ChannelCursor,
    ) -> Result<(ChoiceBeginAccepted, bool), StoreError> {
        if cursor.channel != ChannelKind::IMessage {
            return Err(StoreError::ChoiceBeginConflict);
        }
        self.begin_choice_session_with_clock_and_cursor(record, snapshot, clock, Some(cursor))
    }

    /// Atomically recognizes an exact durable self-chat intake replay,
    /// advances its cursor, and reports whether the private initial worker is
    /// still required. No caller-supplied body or identity is accepted here.
    ///
    /// # Errors
    ///
    /// Returns an error for pairing, cursor, runtime, audit, or replay drift.
    pub fn replay_imessage_choice_begin_with_cursor(
        &mut self,
        request_id: &str,
        request_digest: &str,
        cursor: &ChannelCursor,
    ) -> Result<Option<(ChoiceBeginRecord, bool)>, StoreError> {
        if request_id.is_empty()
            || request_id.len() > 256
            || request_id.as_bytes().contains(&0)
            || !is_sha256_hex(request_digest)
            || cursor.channel != ChannelKind::IMessage
            || validate_cursor(cursor).is_err()
        {
            return Err(StoreError::ChoiceBeginConflict);
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_runtime_enabled(&transaction, &self.authority, self.trusted_broker.as_ref())?;
        require_no_effect_fence(&transaction)?;
        verified_audit_tail(&transaction, &self.authority)?;
        verify_all_bindings(&transaction, &self.authority, self.trusted_broker.as_ref())?;
        let Some(record) = load_choice_begin_record(&transaction, &self.authority, request_id)?
        else {
            return Ok(None);
        };
        if record.request_digest != request_digest
            || record.source_envelope.provider_message_id.is_none()
        {
            return Err(StoreError::ChoiceBeginConflict);
        }
        let pairing = load_channel_pairing(&transaction, &self.authority, cursor.channel)?
            .ok_or(StoreError::ChoiceBeginConflict)?;
        if pairing.conversation_id != cursor.conversation_id || pairing.imessage.is_none() {
            return Err(StoreError::ChoiceBeginConflict);
        }
        if let Some(current) = load_channel_cursor(
            &transaction,
            &self.authority,
            cursor.channel,
            &cursor.conversation_id,
        )? {
            if current != *cursor && cursor.order <= current.order {
                return Err(StoreError::ChannelObservationConflict);
            }
            if current != *cursor {
                write_channel_cursor(&transaction, &self.authority, cursor)?;
            }
        } else {
            write_channel_cursor(&transaction, &self.authority, cursor)?;
        }
        let current = load_choice_loop_snapshot(&transaction, &self.authority)?;
        let requires_worker = current.as_ref().is_some_and(|value| {
            value.session.id == record.accepted.choice_session_id
                && value.session.state == ChoiceSessionState::Interpreting
                && value.session.revision == record.accepted.accepted_session_revision
                && value.active_choice_set.is_none()
        });
        transaction.commit()?;
        Ok(Some((record, requires_worker)))
    }

    #[allow(clippy::too_many_lines)] // Atomic intake, replay, audit, clock, and optional cursor ownership remain one transaction.
    fn begin_choice_session_with_clock_and_cursor(
        &mut self,
        record: &ChoiceBeginRecord,
        snapshot: &ChoiceLoopSnapshot,
        clock: &ChoiceIdleClockEvidence,
        cursor: Option<&ChannelCursor>,
    ) -> Result<(ChoiceBeginAccepted, bool), StoreError> {
        if !record.is_valid()
            || !snapshot.is_valid()
            || !choice_begin_matches_snapshot(record, snapshot)
            || !clock.is_valid()
            || clock.wall_clock_ms != record.accepted_at_ms
            || cursor.is_some_and(|value| validate_cursor(value).is_err())
        {
            return Err(StoreError::ChoiceBeginConflict);
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_no_effect_fence(&transaction)?;
        verified_audit_tail(&transaction, &self.authority)?;
        verify_all_bindings(&transaction, &self.authority, self.trusted_broker.as_ref())?;
        let runtime =
            load_runtime_control(&transaction, &self.authority, self.trusted_broker.as_ref())?;
        if !runtime.enabled || runtime.revision != record.runtime_revision {
            return Err(StoreError::ChoiceBeginConflict);
        }

        if let Some(cursor) = cursor {
            let pairing = load_channel_pairing(&transaction, &self.authority, cursor.channel)?
                .ok_or(StoreError::ChoiceBeginConflict)?;
            if pairing.conversation_id != cursor.conversation_id
                || pairing.imessage.is_none()
                || record.source_envelope.provider_message_id.is_none()
                || record.source_envelope.delivery_binding_id
                    != snapshot
                        .session
                        .primary_delivery_binding_id
                        .clone()
                        .ok_or(StoreError::ChoiceBeginConflict)?
            {
                return Err(StoreError::ChoiceBeginConflict);
            }
            if let Some(current) = load_channel_cursor(
                &transaction,
                &self.authority,
                cursor.channel,
                &cursor.conversation_id,
            )? && current != *cursor
                && cursor.order <= current.order
            {
                return Err(StoreError::ChannelObservationConflict);
            }
        }

        if let Some(existing) =
            load_choice_begin_record(&transaction, &self.authority, &record.accepted.request_id)?
        {
            if existing.request_digest == record.request_digest {
                if let Some(cursor) = cursor
                    && load_channel_cursor(
                        &transaction,
                        &self.authority,
                        cursor.channel,
                        &cursor.conversation_id,
                    )?
                    .as_ref()
                        != Some(cursor)
                {
                    write_channel_cursor(&transaction, &self.authority, cursor)?;
                }
                let current = load_choice_loop_snapshot(&transaction, &self.authority)?;
                let requires_worker = current.as_ref().is_some_and(|value| {
                    value.session.id == existing.accepted.choice_session_id
                        && value.session.state == ChoiceSessionState::Interpreting
                        && value.session.revision == existing.accepted.accepted_session_revision
                        && value.active_choice_set.is_none()
                });
                transaction.commit()?;
                return Ok((existing.accepted, requires_worker));
            }
            return Err(StoreError::ChoiceBeginConflict);
        }

        let selected = load_choice_model_selection(&transaction, &self.authority)?
            .ok_or(StoreError::ChoiceBeginConflict)?;
        if selected != record.model_selection {
            return Err(StoreError::ChoiceBeginConflict);
        }

        match load_choice_loop_snapshot(&transaction, &self.authority)? {
            Some(current)
                if !matches!(
                    current.session.state,
                    ChoiceSessionState::Completed
                        | ChoiceSessionState::Cancelled
                        | ChoiceSessionState::Executing
                ) || !snapshot.is_permitted_successor_of(&current) =>
            {
                return Err(StoreError::ChoiceBeginConflict);
            }
            None if snapshot.session.revision != 1 => return Err(StoreError::ChoiceBeginConflict),
            Some(_) | None => {}
        }

        persist_choice_begin_record(&transaction, &self.authority, record)?;
        persist_choice_loop_snapshot(
            &transaction,
            &self.authority,
            snapshot,
            record.accepted_at_ms,
        )?;
        persist_choice_idle_clock_anchor(
            &transaction,
            &self.authority,
            clock,
            clock.wall_clock_ms,
        )?;
        if let Some(cursor) = cursor {
            write_channel_cursor(&transaction, &self.authority, cursor)?;
        }
        transaction.commit()?;
        Ok((record.accepted.clone(), true))
    }

    /// Commits the first Choice result through the Host-only operation path.
    /// There is intentionally no RPC accepting this shape: the Host supplies
    /// the operation/generation fence after it has validated a terminal model
    /// result, while this transaction binds it to the durable intake.
    ///
    /// # Errors
    ///
    /// Returns an error for any stale, replayed-with-different-content,
    /// provenance, runtime, audit, or snapshot-transition mismatch.
    pub fn commit_initial_choice_result(
        &mut self,
        result: &ChoiceInitialResult,
    ) -> Result<ChoiceLoopSnapshot, StoreError> {
        if !result.is_valid() {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_no_effect_fence(&transaction)?;
        verified_audit_tail(&transaction, &self.authority)?;
        verify_all_bindings(&transaction, &self.authority, self.trusted_broker.as_ref())?;

        let record = load_choice_begin_record_by_operation(
            &transaction,
            &self.authority,
            &result.operation_id,
        )?
        .ok_or(StoreError::ChoiceLoopStateConflict)?;
        let runtime =
            load_runtime_control(&transaction, &self.authority, self.trusted_broker.as_ref())?;
        if !runtime.enabled
            || runtime.revision != record.runtime_revision
            || result.expected_generation != record.runtime_revision
        {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        if load_choice_model_selection(&transaction, &self.authority)?.as_ref()
            != Some(&record.model_selection)
        {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        let current = load_choice_loop_snapshot(&transaction, &self.authority)?
            .ok_or(StoreError::ChoiceLoopStateConflict)?;

        if current.session.id == record.accepted.choice_session_id
            && current.session.state == ChoiceSessionState::Active
            && current.interpretation.as_ref() == Some(&result.interpretation)
            && current.active_choice_set.as_ref() == Some(&result.choice_set)
        {
            return Ok(current);
        }
        if current.session.id != record.accepted.choice_session_id
            || current.session.state != ChoiceSessionState::Interpreting
            || current.session.revision != result.expected_session_revision
            || current.active_batch.as_ref() != Some(&record.batch)
            || current.interpretation.is_some()
            || current.active_choice_set.is_some()
            || current.last_selection.is_some()
            || current.confirmation.is_some()
            || current.document_manifest != record.source_manifest
            || result.source_manifest_digest != record.source_manifest.aggregate_digest
            || result.persona_revision != record.persona_revision
            || result.completed_at_ms < record.accepted_at_ms
            || !model_provenance_matches_selection(
                &result.model_provenance,
                &record.model_selection,
            )
        {
            return Err(StoreError::ChoiceLoopStateConflict);
        }

        let next_revision = current
            .session
            .revision
            .checked_add(1)
            .ok_or(StoreError::ChoiceLoopStateConflict)?;
        if result.choice_set.choice_session_id != current.session.id
            || result.interpretation.choice_session_id != current.session.id
            || result.choice_set.session_revision != next_revision
            || result.choice_set.expires_on_revision != next_revision
            || result.choice_set.interpretation_revision != result.interpretation.revision
        {
            return Err(StoreError::ChoiceLoopStateConflict);
        }

        let mut next = current.clone();
        next.session.revision = next_revision;
        next.session.state = ChoiceSessionState::Active;
        next.session.last_input_at_ms =
            result.completed_at_ms.max(current.session.last_input_at_ms);
        next.session.soft_idle_at_ms = next
            .session
            .last_input_at_ms
            .checked_add(1_800_000)
            .ok_or(StoreError::ChoiceLoopStateConflict)?;
        next.session.stale_review_at_ms = next
            .session
            .last_input_at_ms
            .checked_add(86_400_000)
            .ok_or(StoreError::ChoiceLoopStateConflict)?;
        next.session.active_interpretation_revision = Some(result.interpretation.revision);
        next.session.active_choice_set_id = Some(result.choice_set.id.clone());
        next.session.model_selection_state = ModelSelectionState::Selected {
            model_provenance_ref: result.model_provenance.id.clone(),
        };
        next.active_batch = None;
        next.interpretation = Some(result.interpretation.clone());
        next.active_choice_set = Some(result.choice_set.clone());
        if !next.is_permitted_successor_of(&current) {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        persist_choice_loop_snapshot(&transaction, &self.authority, &next, result.completed_at_ms)?;
        transaction.commit()?;
        Ok(next)
    }

    /// Records a bounded, durable fail-closed result for the one private
    /// initial Choice generation. This is deliberately not a retry route: it
    /// only retires the sealed intake batch so a stalled or malformed model
    /// result cannot leave a foreground session indefinitely interpreting.
    ///
    /// # Errors
    ///
    /// Returns an error unless the exact accepted operation is still the
    /// current protected-On interpreting session. A cancelled, Off, stale, or
    /// replayed operation therefore cannot change durable continuity.
    pub fn block_initial_choice_operation(
        &mut self,
        operation_id: &str,
        expected_generation: u64,
        blocked_at_ms: i64,
    ) -> Result<ChoiceLoopSnapshot, StoreError> {
        if blocked_at_ms < 0 || operation_id.is_empty() || expected_generation == 0 {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_no_effect_fence(&transaction)?;
        verified_audit_tail(&transaction, &self.authority)?;
        verify_all_bindings(&transaction, &self.authority, self.trusted_broker.as_ref())?;
        let record =
            load_choice_begin_record_by_operation(&transaction, &self.authority, operation_id)?
                .ok_or(StoreError::ChoiceLoopStateConflict)?;
        let runtime =
            load_runtime_control(&transaction, &self.authority, self.trusted_broker.as_ref())?;
        if !runtime.enabled
            || runtime.revision != record.runtime_revision
            || expected_generation != record.runtime_revision
        {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        let current = load_choice_loop_snapshot(&transaction, &self.authority)?
            .ok_or(StoreError::ChoiceLoopStateConflict)?;
        if current.session.id != record.accepted.choice_session_id {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        if current.session.state == ChoiceSessionState::Blocked {
            return Ok(current);
        }
        if current.session.state != ChoiceSessionState::Interpreting
            || current.session.revision != record.accepted.accepted_session_revision
            || current.active_batch.as_ref() != Some(&record.batch)
            || current.interpretation.is_some()
            || current.active_choice_set.is_some()
            || current.last_selection.is_some()
            || current.confirmation.is_some()
        {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        let mut next = current.clone();
        next.session.revision = current
            .session
            .revision
            .checked_add(1)
            .ok_or(StoreError::ChoiceLoopStateConflict)?;
        next.session.state = ChoiceSessionState::Blocked;
        next.session.last_input_at_ms = blocked_at_ms.max(current.session.last_input_at_ms);
        next.session.soft_idle_at_ms = next
            .session
            .last_input_at_ms
            .checked_add(1_800_000)
            .ok_or(StoreError::ChoiceLoopStateConflict)?;
        next.session.stale_review_at_ms = next
            .session
            .last_input_at_ms
            .checked_add(86_400_000)
            .ok_or(StoreError::ChoiceLoopStateConflict)?;
        next.active_batch = None;
        if !next.is_permitted_successor_of(&current) {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        persist_choice_loop_snapshot(&transaction, &self.authority, &next, blocked_at_ms)?;
        transaction.commit()?;
        Ok(next)
    }

    /// Cancels the current foreground Choice session without granting or
    /// performing any external effect. The Host calls this only after it has
    /// retired the active generation token, so a late model result cannot
    /// replace the durable terminal snapshot.
    ///
    /// # Errors
    ///
    /// Returns an error for a stale/off runtime revision, invalid audited
    /// Store state, or any transition that cannot atomically retire the
    /// current foreground session.
    pub fn cancel_choice_session(
        &mut self,
        expected_runtime_revision: u64,
        cancelled_at_ms: i64,
    ) -> Result<ChoiceLoopSnapshot, StoreError> {
        if cancelled_at_ms < 0 || expected_runtime_revision == 0 {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_no_effect_fence(&transaction)?;
        verified_audit_tail(&transaction, &self.authority)?;
        verify_all_bindings(&transaction, &self.authority, self.trusted_broker.as_ref())?;
        let runtime =
            load_runtime_control(&transaction, &self.authority, self.trusted_broker.as_ref())?;
        if !runtime.enabled || runtime.revision != expected_runtime_revision {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        let current = load_choice_loop_snapshot(&transaction, &self.authority)?
            .ok_or(StoreError::ChoiceLoopStateConflict)?;
        if matches!(
            current.session.state,
            ChoiceSessionState::Completed | ChoiceSessionState::Cancelled
        ) {
            return Ok(current);
        }
        let mut next = current.clone();
        next.session.revision = current
            .session
            .revision
            .checked_add(1)
            .ok_or(StoreError::ChoiceLoopStateConflict)?;
        next.session.state = ChoiceSessionState::Cancelled;
        next.session.last_input_at_ms = cancelled_at_ms.max(current.session.last_input_at_ms);
        next.session.soft_idle_at_ms = next
            .session
            .last_input_at_ms
            .checked_add(1_800_000)
            .ok_or(StoreError::ChoiceLoopStateConflict)?;
        next.session.stale_review_at_ms = next
            .session
            .last_input_at_ms
            .checked_add(86_400_000)
            .ok_or(StoreError::ChoiceLoopStateConflict)?;
        next.session.active_choice_set_id = None;
        next.session.active_interpretation_revision = None;
        next.session.pending_confirmation_id = None;
        next.active_batch = None;
        next.interpretation = None;
        next.active_choice_set = None;
        next.last_selection = None;
        next.pending_refinement_operation = None;
        next.confirmation = None;
        if !next.is_permitted_successor_of(&current) {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        persist_choice_loop_snapshot(&transaction, &self.authority, &next, cancelled_at_ms)?;
        purge_choice_private_bodies(
            &transaction,
            &self.authority,
            &current.session.id,
            cancelled_at_ms,
        )?;
        retire_choice_refinement_results(
            &transaction,
            &self.authority,
            &current.session.id,
            cancelled_at_ms,
        )?;
        retire_choice_refinement_contexts(
            &transaction,
            &self.authority,
            &current.session.id,
            cancelled_at_ms,
        )?;
        retire_choice_markdown_intents_on_cancel(
            &transaction,
            &self.authority,
            &current.session.id,
            cancelled_at_ms,
        )?;
        transaction.commit()?;
        Ok(next)
    }

    /// Atomically replaces the one global foreground Choice Loop snapshot.
    /// Exact retries are idempotent. Any revision, stale `ChoiceSet`, document
    /// manifest, or encrypted/audit binding mismatch fails closed before a
    /// partial session state can become visible after restart.
    #[cfg(test)]
    pub(crate) fn save_choice_loop_snapshot(
        &mut self,
        snapshot: &ChoiceLoopSnapshot,
        updated_at_ms: i64,
    ) -> Result<ChoiceLoopSnapshot, StoreError> {
        if updated_at_ms < 0 || !snapshot.is_valid() {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_no_effect_fence(&transaction)?;
        verified_audit_tail(&transaction, &self.authority)?;
        verify_all_bindings(&transaction, &self.authority, self.trusted_broker.as_ref())?;
        if let Some(existing) = load_choice_loop_snapshot(&transaction, &self.authority)? {
            if existing == *snapshot {
                return Ok(existing);
            }
            let existing_updated_at_ms: i64 = transaction.query_row(
                "SELECT updated_at_ms FROM choice_loop_state WHERE singleton_id = 1",
                [],
                |row| row.get(0),
            )?;
            if updated_at_ms < existing_updated_at_ms
                || !snapshot.is_permitted_successor_of(&existing)
            {
                return Err(StoreError::ChoiceLoopStateConflict);
            }
        }
        persist_choice_loop_snapshot(&transaction, &self.authority, snapshot, updated_at_ms)?;
        transaction.commit()?;
        Ok(snapshot.clone())
    }

    /// Commits the sole production Selection transition. UI and adapters never
    /// receive whole-snapshot write authority.
    ///
    /// # Errors
    ///
    /// Returns an error for stale, replayed, cross-session, model-provenance,
    /// Store, audit, or transaction conflicts.
    pub fn commit_choice_selection(
        &mut self,
        selection: &Selection,
        expected_generation: u64,
        updated_at_ms: i64,
    ) -> Result<ChoiceLoopSnapshot, StoreError> {
        self.commit_choice_selection_inner(selection, None, expected_generation, updated_at_ms)
    }

    /// Commits the Host-derived D path.  The plaintext remains only in the
    /// encrypted request record; callers cannot supply its batch identity.
    ///
    /// # Errors
    ///
    /// Returns an error for malformed or stale intake, runtime/session or
    /// provenance drift, audit failure, or a transaction conflict.
    pub fn commit_choice_d_selection(
        &mut self,
        record: &ChoiceDIntakeRecord,
        expected_generation: u64,
    ) -> Result<ChoiceLoopSnapshot, StoreError> {
        self.commit_choice_selection_inner(
            &Selection::NaturalConversationSelection(record.selection.clone()),
            Some(record),
            expected_generation,
            record.selection.selected_at_ms,
        )
    }

    /// Records one authenticated owner re-entry from a durable idle review.
    /// This has no caller-supplied session, `ChoiceSet`, context, provenance, or
    /// time fields: the Host supplies only its protected runtime generation.
    /// An exact already-pending resume is an idempotent recovery read.
    ///
    /// # Errors
    ///
    /// Returns an error for malformed, drifted, off, missing-private-context,
    /// audit, or transaction-conflicting state.
    #[allow(clippy::too_many_lines)] // One IMMEDIATE transaction binds every resume fence.
    pub fn begin_choice_resume(
        &mut self,
        expected_generation: u64,
        resumed_at_ms: i64,
    ) -> Result<ChoiceLoopSnapshot, StoreError> {
        if expected_generation == 0 || resumed_at_ms < 0 {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_no_effect_fence(&transaction)?;
        verified_audit_tail(&transaction, &self.authority)?;
        verify_all_bindings(&transaction, &self.authority, self.trusted_broker.as_ref())?;
        let current = load_choice_loop_snapshot(&transaction, &self.authority)?
            .ok_or(StoreError::ChoiceLoopStateConflict)?;
        let runtime =
            load_runtime_control(&transaction, &self.authority, self.trusted_broker.as_ref())?;
        if !runtime.enabled || runtime.revision != expected_generation {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        let selected_model = load_choice_model_selection(&transaction, &self.authority)?
            .ok_or(StoreError::ChoiceLoopStateConflict)?;
        if current.session.state == ChoiceSessionState::Refining {
            let operation = current
                .pending_refinement_operation
                .as_ref()
                .filter(|operation| operation.is_owner_resume())
                .ok_or(StoreError::ChoiceLoopStateConflict)?;
            if operation.expected_generation == expected_generation
                && operation.choice_session_id == current.session.id
                && operation.expected_session_revision == current.session.revision
                && model_provenance_matches_selection(&operation.model_provenance, &selected_model)
            {
                return Ok(current);
            }
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        let prior_state = current.session.state;
        if !matches!(
            prior_state,
            ChoiceSessionState::SoftIdle | ChoiceSessionState::StaleReview
        ) {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        let begin = load_choice_begin_record_by_session(
            &transaction,
            &self.authority,
            &current.session.id,
        )?;
        let (source_envelope_id, conversation_turn_batch_id, persona_revision, interpretation) =
            if let Some(begin) = begin {
                if begin.source_manifest.aggregate_digest
                    != current.document_manifest.aggregate_digest
                    || begin.accepted.choice_session_id != current.session.id
                    || begin.runtime_revision == 0
                {
                    return Err(StoreError::ChoiceLoopStateConflict);
                }
                (
                    begin.accepted.source_envelope_id,
                    begin.accepted.conversation_turn_batch_id,
                    begin.persona_revision,
                    current
                        .interpretation
                        .clone()
                        .ok_or(StoreError::ChoiceLoopStateConflict)?,
                )
            } else {
                // After a verified Receipt + Markdown publication, private raw
                // intake has been retired. The sealed confirmation is the only
                // bounded typed context allowed to mint the next-choice worker.
                let confirmation = current
                    .confirmation
                    .as_ref()
                    .filter(|_| prior_state == ChoiceSessionState::SoftIdle)
                    .ok_or(StoreError::ChoiceLoopStateConflict)?;
                let marker = sha256_hex(
                    format!("{}:{}", confirmation.id, confirmation.payload_digest).as_bytes(),
                );
                let interpretation = InterpretationFrame {
                    choice_session_id: current.session.id.clone(),
                    revision: confirmation.interpretation_revision,
                    understood_goal: confirmation.goal.clone(),
                    current_context: format!(
                        "The confirmed plan is complete. {} ordered step(s) were recorded in {}.",
                        confirmation.steps.len(),
                        confirmation.markdown_entry.relative_path
                    ),
                    assumptions: Vec::new(),
                    constraints: confirmation.permissions.clone(),
                    uncertainties: Vec::new(),
                    what_to_avoid: confirmation.effect_classes.clone(),
                    source_manifest_digest: current.document_manifest.aggregate_digest.clone(),
                };
                if !interpretation.is_valid() {
                    return Err(StoreError::ChoiceLoopStateConflict);
                }
                (
                    format!("post-receipt-envelope-{}", &marker[..24]),
                    format!("post-receipt-batch-{}", &marker[..24]),
                    confirmation.persona_revision.clone(),
                    interpretation,
                )
            };
        let next_revision = current
            .session
            .revision
            .checked_add(1)
            .ok_or(StoreError::ChoiceLoopStateConflict)?;
        let state_marker = match prior_state {
            ChoiceSessionState::SoftIdle => "soft-idle",
            ChoiceSessionState::StaleReview => "stale-review",
            _ => return Err(StoreError::ChoiceLoopStateConflict),
        };
        let operation_id = format!("resume-{}-{}", current.session.id, next_revision);
        let selection_id = format!(
            "resume-{}-{}-{}",
            state_marker, current.session.id, next_revision
        );
        let model_provenance = selected_model
            .turn_provenance(
                format!("resume-provenance-{next_revision}"),
                format!("resume-turn-{next_revision}"),
            )
            .ok_or(StoreError::ChoiceLoopStateConflict)?;
        let operation = ChoiceRefinementOperation {
            id: operation_id.clone(),
            selection_id: selection_id.clone(),
            choice_session_id: current.session.id.clone(),
            source_envelope_id: source_envelope_id.clone(),
            conversation_turn_batch_id: conversation_turn_batch_id.clone(),
            expected_session_revision: next_revision,
            expected_generation,
            model_provenance,
            source_manifest_digest: current.document_manifest.aggregate_digest.clone(),
            persona_revision,
            d_request_id: None,
            d_input_digest: None,
            created_at_ms: resumed_at_ms,
        };
        let context = ChoiceRefinementContext {
            operation_id,
            selection_id,
            choice_session_id: current.session.id.clone(),
            source_envelope_id,
            conversation_turn_batch_id,
            expected_session_revision: next_revision,
            interpretation,
            selected_option: None,
        };
        if !operation.is_valid() || !operation.is_owner_resume() || !context.is_valid() {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        let mut next = current.clone();
        next.session.revision = next_revision;
        next.session.state = ChoiceSessionState::Refining;
        next.session.last_input_at_ms = resumed_at_ms.max(current.session.last_input_at_ms);
        next.session.soft_idle_at_ms = next
            .session
            .last_input_at_ms
            .checked_add(1_800_000)
            .ok_or(StoreError::ChoiceLoopStateConflict)?;
        next.session.stale_review_at_ms = next
            .session
            .last_input_at_ms
            .checked_add(86_400_000)
            .ok_or(StoreError::ChoiceLoopStateConflict)?;
        next.session.active_choice_set_id = None;
        next.active_choice_set = None;
        next.active_batch = None;
        next.pending_refinement_operation = Some(operation);
        next.session.pending_confirmation_id = None;
        next.confirmation = None;
        if !next.is_permitted_successor_of(&current) {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        persist_choice_refinement_context(&transaction, &self.authority, &context, resumed_at_ms)?;
        persist_choice_loop_snapshot(&transaction, &self.authority, &next, resumed_at_ms)?;
        transaction.commit()?;
        Ok(next)
    }

    /// Resolves an exact durable D request before the Host reserves a model
    /// operation. This makes an ambiguous transport retry return the same
    /// snapshot whether refinement is pending or already complete, without
    /// reopening work or accepting a changed request digest.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid encrypted request/audit binding or a
    /// changed request identity. Retired inputs remain non-replayable.
    pub fn choice_d_replay(
        &self,
        request_id: &str,
        request_digest: &str,
    ) -> Result<Option<ChoiceLoopSnapshot>, StoreError> {
        verified_audit_tail(&self.connection, &self.authority)?;
        let Some(record) = load_choice_d_record(&self.connection, &self.authority, request_id)?
        else {
            return if choice_private_body_tombstone_exists(
                &self.connection,
                &self.authority,
                "d",
                request_id,
            )? {
                Err(StoreError::ChoiceLoopStateConflict)
            } else {
                Ok(None)
            };
        };
        if record.request_digest != request_digest {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        let snapshot = load_choice_loop_snapshot(&self.connection, &self.authority)?
            .ok_or(StoreError::ChoiceLoopStateConflict)?;
        let expected_selection = Selection::NaturalConversationSelection(record.selection.clone());
        let pending_matches = snapshot
            .pending_refinement_operation
            .as_ref()
            .is_some_and(|operation| operation.d_request_id.as_deref() == Some(request_id));
        if snapshot.last_selection.as_ref() != Some(&expected_selection) && !pending_matches {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        Ok(Some(snapshot))
    }

    /// Loads the Host-private semantic context for the exact pending
    /// refinement operation. It is never part of a UI snapshot or RPC write
    /// shape, and authenticated audit/blob bindings are verified before the
    /// worker can construct its bounded model brief.
    ///
    /// # Errors
    ///
    /// Returns an error for a retired, stale, tampered, or mismatched context.
    pub fn choice_refinement_context(
        &self,
        operation: &ChoiceRefinementOperation,
    ) -> Result<ChoiceRefinementContext, StoreError> {
        verified_audit_tail(&self.connection, &self.authority)?;
        let snapshot = load_choice_loop_snapshot(&self.connection, &self.authority)?
            .ok_or(StoreError::ChoiceLoopStateConflict)?;
        if snapshot.pending_refinement_operation.as_ref() != Some(operation)
            || snapshot.session.state != ChoiceSessionState::Refining
        {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        let context =
            load_choice_refinement_context(&self.connection, &self.authority, &operation.id)?
                .ok_or(StoreError::ChoiceLoopStateConflict)?;
        if context.operation_id != operation.id
            || context.selection_id != operation.selection_id
            || context.choice_session_id != operation.choice_session_id
            || context.source_envelope_id != operation.source_envelope_id
            || context.conversation_turn_batch_id != operation.conversation_turn_batch_id
            || context.expected_session_revision != operation.expected_session_revision
            || context.interpretation.choice_session_id != operation.choice_session_id
            || (!operation.is_owner_resume()
                && (operation.d_request_id.is_some()) != context.selected_option.is_none())
        {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        Ok(context)
    }

    /// Returns the encrypted D body only for its exact pending Host operation.
    /// A/B/C operations have no D body, and no public RPC can query this
    /// record by arbitrary request ID.
    ///
    /// # Errors
    ///
    /// Returns an error for stale, retired, malformed, or audit-unbound D
    /// intake state.
    pub fn choice_d_intake_for_refinement(
        &self,
        operation: &ChoiceRefinementOperation,
    ) -> Result<Option<ChoiceDIntakeRecord>, StoreError> {
        let Some(request_id) = operation.d_request_id.as_deref() else {
            return Ok(None);
        };
        let snapshot = load_choice_loop_snapshot(&self.connection, &self.authority)?
            .ok_or(StoreError::ChoiceLoopStateConflict)?;
        if snapshot.pending_refinement_operation.as_ref() != Some(operation)
            || snapshot.session.state != ChoiceSessionState::Refining
        {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        let record = load_choice_d_record(&self.connection, &self.authority, request_id)?
            .ok_or(StoreError::ChoiceLoopStateConflict)?;
        if record.selection.id != operation.selection_id
            || record.selection.choice_session_id != operation.choice_session_id
            || record.selection.expected_session_revision.checked_add(1)
                != Some(operation.expected_session_revision)
            || record.input.request_id != request_id
            || record.input.bounded_text.is_empty()
            || record.source_envelope.id != operation.source_envelope_id
            || record.batch.id != operation.conversation_turn_batch_id
        {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        Ok(Some(record))
    }

    /// Reads an exact A/B/C replay before a Host worker slot is acquired.
    /// A different selection never receives this shortcut.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid selection or an unauditable durable
    /// Choice snapshot.
    pub fn choice_selection_replay(
        &self,
        selection: &Selection,
    ) -> Result<Option<ChoiceLoopSnapshot>, StoreError> {
        if !selection.is_valid() {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        verified_audit_tail(&self.connection, &self.authority)?;
        let snapshot = load_choice_loop_snapshot(&self.connection, &self.authority)?;
        Ok(snapshot.filter(|snapshot| snapshot.last_selection.as_ref() == Some(selection)))
    }

    /// Reads an exact A/B/C replay using only caller-owned selection identity.
    /// The accepted clock is deliberately excluded: the Host records it when
    /// first accepting the selection and an RPC caller must never be able to
    /// steer idle/stale deadlines by changing a timestamp on retry.
    ///
    /// # Errors
    ///
    /// Returns an error for a malformed selection or an unauditable snapshot.
    pub fn choice_option_selection_replay(
        &self,
        selection: &OptionSelection,
    ) -> Result<Option<ChoiceLoopSnapshot>, StoreError> {
        if !Selection::OptionSelection(selection.clone()).is_valid() {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        verified_audit_tail(&self.connection, &self.authority)?;
        let snapshot = load_choice_loop_snapshot(&self.connection, &self.authority)?;
        Ok(snapshot.filter(|snapshot| {
            matches!(snapshot.last_selection.as_ref(), Some(Selection::OptionSelection(stored))
                if stored.id == selection.id
                    && stored.choice_session_id == selection.choice_session_id
                    && stored.choice_set_id == selection.choice_set_id
                    && stored.selected_option_id == selection.selected_option_id
                    && stored.expected_session_revision == selection.expected_session_revision)
        }))
    }

    #[allow(clippy::too_many_lines)] // One transaction must visibly bind selection, operation, snapshot, and audit.
    fn commit_choice_selection_inner(
        &mut self,
        selection: &Selection,
        d_record: Option<&ChoiceDIntakeRecord>,
        expected_generation: u64,
        updated_at_ms: i64,
    ) -> Result<ChoiceLoopSnapshot, StoreError> {
        if updated_at_ms < 0 || expected_generation == 0 || !selection.is_valid() {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        if matches!(selection, Selection::NaturalConversationSelection(_)) != d_record.is_some()
            || d_record.is_some_and(|record| !record.is_valid())
        {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_no_effect_fence(&transaction)?;
        verified_audit_tail(&transaction, &self.authority)?;
        verify_all_bindings(&transaction, &self.authority, self.trusted_broker.as_ref())?;
        let current = load_choice_loop_snapshot(&transaction, &self.authority)?
            .ok_or(StoreError::ChoiceLoopStateConflict)?;
        let runtime =
            load_runtime_control(&transaction, &self.authority, self.trusted_broker.as_ref())?;
        let existing_updated_at_ms: i64 = transaction.query_row(
            "SELECT updated_at_ms FROM choice_loop_state WHERE singleton_id = 1",
            [],
            |row| row.get(0),
        )?;
        if updated_at_ms < existing_updated_at_ms
            || !runtime.enabled
            || runtime.revision != expected_generation
        {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        let selected_at_ms = match selection {
            Selection::OptionSelection(value) => value.selected_at_ms,
            Selection::NaturalConversationSelection(value) => value.selected_at_ms,
        };
        if selected_at_ms != updated_at_ms {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        if current.last_selection.as_ref() == Some(selection) {
            if let Some(record) = d_record {
                let stored =
                    load_choice_d_record(&transaction, &self.authority, &record.input.request_id)?
                        .ok_or(StoreError::ChoiceLoopStateConflict)?;
                if stored != *record {
                    return Err(StoreError::ChoiceLoopStateConflict);
                }
            }
            return Ok(current);
        }
        if current.session.state != ChoiceSessionState::Active {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        let choice_set = current
            .active_choice_set
            .as_ref()
            .ok_or(StoreError::ChoiceLoopStateConflict)?;
        let (session_id, choice_set_id, expected_revision) = match selection {
            Selection::OptionSelection(value) => (
                &value.choice_session_id,
                &value.choice_set_id,
                value.expected_session_revision,
            ),
            Selection::NaturalConversationSelection(value) => (
                &value.choice_session_id,
                &value.choice_set_id,
                value.expected_session_revision,
            ),
        };
        if session_id != &current.session.id
            || choice_set_id != &choice_set.id
            || expected_revision != current.session.revision
            || choice_set.expires_on_revision != current.session.revision
        {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        match selection {
            Selection::OptionSelection(value)
                if choice_set
                    .options
                    .iter()
                    .any(|option| option.id == value.selected_option_id) => {}
            Selection::NaturalConversationSelection(value)
                if choice_set.d_available
                    && d_record.is_some_and(|record| {
                        record.selection == *value
                            && current.session.primary_delivery_binding_id.as_deref()
                                == Some(&record.batch.delivery_binding_id)
                    }) => {}
            _ => return Err(StoreError::ChoiceLoopStateConflict),
        }
        let selected_model = load_choice_model_selection(&transaction, &self.authority)?
            .ok_or(StoreError::ChoiceLoopStateConflict)?;
        if !session_model_matches_choice_set(&current.session, choice_set)
            || !selected_model_matches_choice_set(&selected_model, choice_set)
        {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        // `active_batch` is intentionally retired by the selection state
        // transition. Capture the complete Host-authenticated source tuple
        // first, from the D intake for D or the still-encrypted initial begin
        // record for A/B/C. Never reconstruct it from a delivery binding or
        // another mutable snapshot field.
        let (source_envelope_id, conversation_turn_batch_id) = if let Some(record) = d_record {
            (record.source_envelope.id.clone(), record.batch.id.clone())
        } else {
            let record = load_choice_begin_record_by_session(
                &transaction,
                &self.authority,
                &current.session.id,
            )?;
            if let Some(record) = record {
                if record.accepted.choice_session_id != current.session.id
                    || record.accepted.source_envelope_id != record.source_envelope.id
                    || record.accepted.conversation_turn_batch_id != record.batch.id
                {
                    return Err(StoreError::ChoiceLoopStateConflict);
                }
                (
                    record.accepted.source_envelope_id,
                    record.accepted.conversation_turn_batch_id,
                )
            } else {
                // This narrow compatibility shape is only valid while the
                // authenticated sealed batch remains in the current signed
                // snapshot. It does not infer identity after the batch is
                // retired, and production begin flow always uses the
                // encrypted begin record above.
                let batch = current
                    .active_batch
                    .as_ref()
                    .filter(|batch| {
                        batch.choice_session_id == current.session.id
                            && batch.source_envelope_ids.len() == 1
                            && batch.delivery_binding_id
                                == current
                                    .session
                                    .primary_delivery_binding_id
                                    .as_deref()
                                    .unwrap_or_default()
                    })
                    .ok_or(StoreError::ChoiceLoopStateConflict)?;
                (batch.source_envelope_ids[0].clone(), batch.id.clone())
            }
        };
        let mut next = current.clone();
        next.session.revision = next
            .session
            .revision
            .checked_add(1)
            .ok_or(StoreError::ChoiceLoopStateConflict)?;
        next.session.state = ChoiceSessionState::Refining;
        next.session.last_input_at_ms = updated_at_ms.max(next.session.last_input_at_ms);
        next.session.soft_idle_at_ms = next
            .session
            .last_input_at_ms
            .checked_add(1_800_000)
            .ok_or(StoreError::ChoiceLoopStateConflict)?;
        next.session.stale_review_at_ms = next
            .session
            .last_input_at_ms
            .checked_add(86_400_000)
            .ok_or(StoreError::ChoiceLoopStateConflict)?;
        next.session.active_choice_set_id = None;
        next.active_choice_set = None;
        next.active_batch = None;
        next.last_selection = Some(selection.clone());
        let refinement_operation = ChoiceRefinementOperation {
            id: format!("refinement-{}", choice_selection_id(selection)),
            selection_id: choice_selection_id(selection).to_owned(),
            choice_session_id: next.session.id.clone(),
            source_envelope_id: source_envelope_id.clone(),
            conversation_turn_batch_id: conversation_turn_batch_id.clone(),
            expected_session_revision: next.session.revision,
            expected_generation,
            model_provenance: choice_set.model_provenance.clone(),
            source_manifest_digest: next.document_manifest.aggregate_digest.clone(),
            persona_revision: choice_set.persona_revision.clone(),
            d_request_id: d_record.map(|record| record.input.request_id.clone()),
            d_input_digest: d_record.map(|record| {
                format!("{:x}", Sha256::digest(record.input.bounded_text.as_bytes()))
            }),
            created_at_ms: updated_at_ms,
        };
        // The worker must receive the selected semantic direction, not merely
        // an opaque Selection identifier. Keep it in a private encrypted
        // record; D plaintext remains exclusively in the D intake record.
        let refinement_context = ChoiceRefinementContext {
            operation_id: refinement_operation.id.clone(),
            selection_id: refinement_operation.selection_id.clone(),
            choice_session_id: next.session.id.clone(),
            source_envelope_id,
            conversation_turn_batch_id,
            expected_session_revision: next.session.revision,
            interpretation: current
                .interpretation
                .clone()
                .ok_or(StoreError::ChoiceLoopStateConflict)?,
            selected_option: match selection {
                Selection::OptionSelection(value) => choice_set
                    .options
                    .iter()
                    .find(|option| option.id == value.selected_option_id)
                    .cloned(),
                Selection::NaturalConversationSelection(_) => None,
            },
        };
        if !refinement_context.is_valid()
            || matches!(selection, Selection::OptionSelection(_))
                != refinement_context.selected_option.is_some()
        {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        next.pending_refinement_operation = Some(refinement_operation);
        if !next.is_permitted_successor_of(&current) {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        if let Some(record) = d_record {
            persist_choice_d_record(
                &transaction,
                &self.authority,
                record,
                next.pending_refinement_operation
                    .as_ref()
                    .ok_or(StoreError::ChoiceLoopStateConflict)?,
            )?;
        }
        persist_choice_refinement_context(
            &transaction,
            &self.authority,
            &refinement_context,
            updated_at_ms,
        )?;
        persist_choice_loop_snapshot(&transaction, &self.authority, &next, updated_at_ms)?;
        transaction.commit()?;
        Ok(next)
    }

    /// Commits a private refinement result only when it is still bound to the
    /// exact durable selection operation.  No adapter or UI has a route to
    /// this transition, and the result creates neither a Mission nor an
    /// external effect.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid, stale, cancelled, or drifted result,
    /// audit/encrypted binding failure, or a transaction conflict.
    pub fn commit_choice_refinement_result(
        &mut self,
        result: &ChoiceRefinementResult,
    ) -> Result<ChoiceLoopSnapshot, StoreError> {
        if !result.is_valid() {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_no_effect_fence(&transaction)?;
        verified_audit_tail(&transaction, &self.authority)?;
        verify_all_bindings(&transaction, &self.authority, self.trusted_broker.as_ref())?;
        let current = load_choice_loop_snapshot(&transaction, &self.authority)?
            .ok_or(StoreError::ChoiceLoopStateConflict)?;
        let Some(operation) = current.pending_refinement_operation.as_ref() else {
            let existing =
                load_choice_refinement_result(&transaction, &self.authority, &result.operation_id)?;
            if existing.as_ref() == Some(result) {
                return Ok(current);
            }
            return Err(StoreError::ChoiceLoopStateConflict);
        };
        let runtime =
            load_runtime_control(&transaction, &self.authority, self.trusted_broker.as_ref())?;
        let selected_model = load_choice_model_selection(&transaction, &self.authority)?
            .ok_or(StoreError::ChoiceLoopStateConflict)?;
        let result_is_resume = result.selection_id.starts_with("resume-soft-idle-")
            || result.selection_id.starts_with("resume-stale-review-");
        if result.operation_id != operation.id
            || result.selection_id != operation.selection_id
            || result.expected_generation != operation.expected_generation
            || result.expected_session_revision != operation.expected_session_revision
            || result.source_envelope_id != operation.source_envelope_id
            || result.conversation_turn_batch_id != operation.conversation_turn_batch_id
            || current.session.id != operation.choice_session_id
            || current.session.revision != operation.expected_session_revision
            || current.session.state != ChoiceSessionState::Refining
            || !runtime.enabled
            || runtime.revision != operation.expected_generation
            || result.model_provenance != operation.model_provenance
            || !model_provenance_matches_selection(&result.model_provenance, &selected_model)
            || result.source_manifest_digest != operation.source_manifest_digest
            || result.persona_revision != operation.persona_revision
            || result_is_resume != operation.is_owner_resume()
            || result.interpretation.choice_session_id != current.session.id
            || result.choice_set.choice_session_id != current.session.id
            || result.choice_set.session_revision
                != current
                    .session
                    .revision
                    .checked_add(1)
                    .ok_or(StoreError::ChoiceLoopStateConflict)?
        {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        let mut next = current.clone();
        next.session.revision = current
            .session
            .revision
            .checked_add(1)
            .ok_or(StoreError::ChoiceLoopStateConflict)?;
        next.session.state = ChoiceSessionState::Active;
        next.session.last_input_at_ms =
            result.completed_at_ms.max(current.session.last_input_at_ms);
        next.session.soft_idle_at_ms = next.session.last_input_at_ms + 1_800_000;
        next.session.stale_review_at_ms = next.session.last_input_at_ms + 86_400_000;
        next.session.active_interpretation_revision = Some(result.interpretation.revision);
        next.session.active_choice_set_id = Some(result.choice_set.id.clone());
        next.interpretation = Some(result.interpretation.clone());
        next.active_choice_set = Some(result.choice_set.clone());
        next.pending_refinement_operation = None;
        if !next.is_permitted_successor_of(&current) {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        persist_choice_refinement_result(&transaction, &self.authority, result)?;
        retire_choice_refinement_contexts(
            &transaction,
            &self.authority,
            &current.session.id,
            result.completed_at_ms,
        )?;
        persist_choice_loop_snapshot(&transaction, &self.authority, &next, result.completed_at_ms)?;
        transaction.commit()?;
        Ok(next)
    }

    /// The only Store entrypoint for a Host-owned idle/stale recap result.
    /// It is deliberately separate from selected A/B/C/D refinement even
    /// though both reuse the same encrypted result persistence primitive.
    ///
    /// # Errors
    ///
    /// Returns an error for a non-resume result, stale operation, audit
    /// mismatch, protected-runtime drift, or transaction conflict.
    pub fn commit_choice_resume_result(
        &mut self,
        resume: &ChoiceResumeResult,
    ) -> Result<ChoiceLoopSnapshot, StoreError> {
        if !resume.is_valid() {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        let snapshot = self
            .choice_loop_snapshot()?
            .ok_or(StoreError::ChoiceLoopStateConflict)?;
        if snapshot.session.state == ChoiceSessionState::Refining {
            if snapshot
                .pending_refinement_operation
                .as_ref()
                .is_none_or(|operation| {
                    !operation.is_owner_resume() || operation.id != resume.result.operation_id
                })
            {
                return Err(StoreError::ChoiceLoopStateConflict);
            }
        } else {
            // The shared result primitive verifies the retained encrypted
            // result and returns only an exact idempotent replay.  Keep this
            // resume wrapper typed so a normal refinement can never enter it.
            return self.commit_choice_refinement_result(&resume.result);
        }
        self.commit_choice_refinement_result(&resume.result)
    }

    /// Retires a private refinement that could not produce a verified result.
    /// This is a durable fail-closed recovery transition, not a retry or a
    /// result publication path.  It exists so a cancelled or unavailable
    /// private worker cannot strand the foreground session in `Refining`.
    ///
    /// # Errors
    ///
    /// Returns an error when the protected runtime, pending operation, audit
    /// chain, or current session revision no longer match the exact worker.
    pub fn block_choice_refinement_operation(
        &mut self,
        operation_id: &str,
        expected_generation: u64,
        blocked_at_ms: i64,
    ) -> Result<ChoiceLoopSnapshot, StoreError> {
        if operation_id.is_empty() || expected_generation == 0 || blocked_at_ms < 0 {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_no_effect_fence(&transaction)?;
        verified_audit_tail(&transaction, &self.authority)?;
        verify_all_bindings(&transaction, &self.authority, self.trusted_broker.as_ref())?;
        let runtime =
            load_runtime_control(&transaction, &self.authority, self.trusted_broker.as_ref())?;
        let current = load_choice_loop_snapshot(&transaction, &self.authority)?
            .ok_or(StoreError::ChoiceLoopStateConflict)?;
        let operation = current
            .pending_refinement_operation
            .as_ref()
            .ok_or(StoreError::ChoiceLoopStateConflict)?;
        // A replacement protected-On generation must retire an interrupted
        // owner-resume operation from the preceding generation. It cannot
        // restart it: the transition only returns to the recorded idle/stale
        // review state with a fresh revision and no ChoiceSet. Ordinary
        // refinements remain bound to their exact generation.
        let stale_owner_resume =
            operation.is_owner_resume() && operation.expected_generation != expected_generation;
        if operation.id != operation_id
            || (operation.expected_generation != expected_generation && !stale_owner_resume)
            || !runtime.enabled
            || runtime.revision != expected_generation
            || current.session.state != ChoiceSessionState::Refining
            || current.session.revision != operation.expected_session_revision
        {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        let mut next = current.clone();
        next.session.revision = current
            .session
            .revision
            .checked_add(1)
            .ok_or(StoreError::ChoiceLoopStateConflict)?;
        // A failed owner re-entry is not a terminal model retry route. Return
        // to the recorded idle review state with a fresh revision and no
        // ChoiceSet, so only a later authenticated foreground re-entry can
        // request a new resume operation.
        next.session.state = operation
            .resume_prior_state()
            .unwrap_or(ChoiceSessionState::Blocked);
        next.session.last_input_at_ms = blocked_at_ms.max(current.session.last_input_at_ms);
        next.session.soft_idle_at_ms = next
            .session
            .last_input_at_ms
            .checked_add(1_800_000)
            .ok_or(StoreError::ChoiceLoopStateConflict)?;
        next.session.stale_review_at_ms = next
            .session
            .last_input_at_ms
            .checked_add(86_400_000)
            .ok_or(StoreError::ChoiceLoopStateConflict)?;
        next.session.active_choice_set_id = None;
        // A failed owner-resume intentionally returns to the prior idle review
        // state. Preserve the already authenticated interpretation so the
        // next *new* foreground re-entry can construct its bounded recap;
        // all pending operation/context state is still retired below.
        if !operation.is_owner_resume() {
            next.session.active_interpretation_revision = None;
        }
        next.active_batch = None;
        if !operation.is_owner_resume() {
            next.interpretation = None;
        }
        next.active_choice_set = None;
        next.pending_refinement_operation = None;
        if !next.is_permitted_successor_of(&current) {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        retire_choice_refinement_contexts(
            &transaction,
            &self.authority,
            &current.session.id,
            blocked_at_ms,
        )?;
        persist_choice_loop_snapshot(&transaction, &self.authority, &next, blocked_at_ms)?;
        transaction.commit()?;
        Ok(next)
    }

    /// Converts an interrupted in-process Choice worker into a durable,
    /// fail-closed recovery state after Host restart. Callers supply no
    /// operation identity or snapshot, so this cannot become a public retry
    /// or whole-state replacement route.
    ///
    /// # Errors
    ///
    /// Returns an error for runtime drift, malformed bindings, or a state
    /// that cannot be proved to be the sole interrupted foreground worker.
    pub fn recover_interrupted_choice_operation(
        &mut self,
        expected_generation: u64,
        blocked_at_ms: i64,
    ) -> Result<Option<ChoiceLoopSnapshot>, StoreError> {
        let snapshot = self.choice_loop_snapshot()?;
        let Some(snapshot) = snapshot else {
            return Ok(None);
        };
        match snapshot.session.state {
            ChoiceSessionState::Interpreting => {
                let record = load_choice_begin_record_by_session(
                    &self.connection,
                    &self.authority,
                    &snapshot.session.id,
                )?
                .ok_or(StoreError::ChoiceLoopStateConflict)?;
                self.block_initial_choice_operation(
                    &record.accepted.operation_id,
                    expected_generation,
                    blocked_at_ms,
                )
                .map(Some)
            }
            ChoiceSessionState::Refining => {
                let operation = snapshot
                    .pending_refinement_operation
                    .as_ref()
                    .ok_or(StoreError::ChoiceLoopStateConflict)?;
                self.block_choice_refinement_operation(
                    &operation.id,
                    expected_generation,
                    blocked_at_ms,
                )
                .map(Some)
            }
            _ => Ok(None),
        }
    }

    /// Resolves an exact durable schedule request before any clock-consuming
    /// work. Changed reuse fails closed; an exact lost-response retry remains
    /// read-only even when current clock continuity is uncertain.
    ///
    /// # Errors
    ///
    /// Returns an error for malformed, changed, unauditable, or corrupt state.
    pub fn choice_reminder_schedule_replay(
        &self,
        input: &ChoiceReminderScheduleInput,
    ) -> Result<Option<ChoiceReminderSchedule>, StoreError> {
        if !input.is_valid() {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        verified_audit_tail(&self.connection, &self.authority)?;
        match load_choice_reminder_schedule_by_request(
            &self.connection,
            &self.authority,
            &input.request_id,
        )? {
            Some(existing) if existing.input == *input => Ok(Some(existing)),
            Some(_) => Err(StoreError::ChoiceLoopStateConflict),
            None => Ok(None),
        }
    }

    /// Persists one explicit user schedule selection before confirmation.
    /// This command records only a revisioned local proposal; it never
    /// creates a `Reminder` or grants any external authority.
    ///
    /// # Errors
    ///
    /// Returns an error for stale, changed, unbound, Store, audit, or
    /// transaction conflicts.
    pub fn record_choice_reminder_schedule(
        &mut self,
        input: &ChoiceReminderScheduleInput,
        expected_generation: u64,
        accepted_at_ms: i64,
    ) -> Result<ChoiceReminderSchedule, StoreError> {
        if !input.is_valid()
            || !valid_choice_reminder_time_zone(&input.time_zone)
            || expected_generation == 0
            || accepted_at_ms < 0
        {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_no_effect_fence(&transaction)?;
        verified_audit_tail(&transaction, &self.authority)?;
        verify_all_bindings(&transaction, &self.authority, self.trusted_broker.as_ref())?;
        if let Some(existing) = load_choice_reminder_schedule_by_request(
            &transaction,
            &self.authority,
            &input.request_id,
        )? {
            if existing.input == *input {
                return Ok(existing);
            }
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        if input.due_at_ms <= accepted_at_ms {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        let runtime =
            load_runtime_control(&transaction, &self.authority, self.trusted_broker.as_ref())?;
        let current = load_choice_loop_snapshot(&transaction, &self.authority)?
            .ok_or(StoreError::ChoiceLoopStateConflict)?;
        if !runtime.enabled
            || runtime.revision != expected_generation
            || current.session.state != ChoiceSessionState::Active
            || current.session.id != input.choice_session_id
            || current.session.revision != input.expected_session_revision
            || current.active_choice_set.is_none()
            || current.last_selection.is_none()
            || accepted_at_ms < current.session.last_input_at_ms
        {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        let revision = u64::try_from(transaction.query_row(
            "SELECT COALESCE(MAX(revision), 0) FROM choice_reminder_schedule WHERE choice_session_id = ?1",
            [&input.choice_session_id], |row| row.get::<_, i64>(0),
        )?).map_err(|_| StoreError::ChoiceLoopStateConflict)?
        .checked_add(1).ok_or(StoreError::ChoiceLoopStateConflict)?;
        let id = format!("reminder-schedule-{}-{revision}", input.choice_session_id);
        let schedule = ChoiceReminderSchedule {
            id,
            input: input.clone(),
            revision,
            accepted_at_ms,
        };
        persist_choice_reminder_schedule(&transaction, &self.authority, &schedule)?;
        transaction.commit()?;
        Ok(schedule)
    }

    /// Reads the current durable local proposal for one Choice session.
    ///
    /// # Errors
    ///
    /// Returns an error when the encrypted record or its signed audit binding
    /// cannot be verified.
    pub fn current_choice_reminder_schedule(
        &self,
        choice_session_id: &str,
    ) -> Result<Option<ChoiceReminderSchedule>, StoreError> {
        load_current_choice_reminder_schedule(&self.connection, &self.authority, choice_session_id)
    }

    /// Reads a schedule only when it belongs to the exact current Choice
    /// revision. Older proposals are legitimate history, not malformed
    /// continuity data, and must never make a later Choice state look broken.
    ///
    /// # Errors
    ///
    /// Returns an error when the encrypted record or its signed audit binding
    /// cannot be verified.
    pub fn current_choice_reminder_schedule_for_revision(
        &self,
        choice_session_id: &str,
        expected_session_revision: u64,
    ) -> Result<Option<ChoiceReminderSchedule>, StoreError> {
        load_current_choice_reminder_schedule_for_revision(
            &self.connection,
            &self.authority,
            choice_session_id,
            expected_session_revision,
        )
    }

    /// Commits a prepared Choice confirmation and its Store-owned Markdown
    /// journal in one transaction. This is deliberately not the legacy
    /// Mission route and performs no external effect.
    ///
    /// # Errors
    ///
    /// Returns an error for stale, changed, unbound, model-provenance,
    /// schedule, Store, audit, or transaction conflicts.
    #[allow(clippy::too_many_lines)] // One IMMEDIATE transaction keeps confirmation, journal, snapshot, and audit binding visible together.
    pub fn commit_choice_confirmation_and_render_intent(
        &mut self,
        confirmation: &ChoiceConsolidatedConfirmation,
        expected_generation: u64,
        updated_at_ms: i64,
    ) -> Result<(ChoiceLoopSnapshot, MarkdownRenderIntent), StoreError> {
        if updated_at_ms < 0 || expected_generation == 0 || !confirmation.is_valid() {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_no_effect_fence(&transaction)?;
        verified_audit_tail(&transaction, &self.authority)?;
        verify_all_bindings(&transaction, &self.authority, self.trusted_broker.as_ref())?;
        let current = load_choice_loop_snapshot(&transaction, &self.authority)?
            .ok_or(StoreError::ChoiceLoopStateConflict)?;
        let runtime =
            load_runtime_control(&transaction, &self.authority, self.trusted_broker.as_ref())?;
        let existing_updated_at_ms: i64 = transaction.query_row(
            "SELECT updated_at_ms FROM choice_loop_state WHERE singleton_id = 1",
            [],
            |row| row.get(0),
        )?;
        if updated_at_ms < existing_updated_at_ms
            || !runtime.enabled
            || runtime.revision != expected_generation
        {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        if current.confirmation.as_ref() == Some(confirmation) {
            let intent_id = markdown_render_intent_id(
                confirmation,
                current.session.revision,
                expected_generation,
                &confirmation.markdown_entry.sha256,
            );
            let stored = load_markdown_render_intent(&transaction, &self.authority, &intent_id)?
                .ok_or(StoreError::ChoiceLoopStateConflict)?;
            let expected = markdown_render_record_for_confirmation(
                confirmation,
                current.session.revision,
                expected_generation,
                stored.intent.created_at_ms,
            )?;
            if stored != expected {
                return Err(StoreError::ChoiceLoopStateConflict);
            }
            transaction.commit()?;
            return Ok((current, stored.intent));
        }
        let choice_set = current
            .active_choice_set
            .as_ref()
            .ok_or(StoreError::ChoiceLoopStateConflict)?;
        let selection = current
            .last_selection
            .as_ref()
            .ok_or(StoreError::ChoiceLoopStateConflict)?;
        let selected_model = load_choice_model_selection(&transaction, &self.authority)?
            .ok_or(StoreError::ChoiceLoopStateConflict)?;
        let schedule = load_current_choice_reminder_schedule_for_revision(
            &transaction,
            &self.authority,
            &current.session.id,
            current.session.revision,
        )?
        .ok_or(StoreError::ChoiceLoopStateConflict)?;
        if confirmation.choice_session_id != current.session.id
            || confirmation.choice_set_id != choice_set.id
            || selection.id() != confirmation.selection_id
            || confirmation.expected_session_revision != current.session.revision
            || confirmation.interpretation_revision != choice_set.interpretation_revision
            || confirmation.model_provenance != choice_set.model_provenance
            || confirmation.persona_revision != choice_set.persona_revision
            || !session_model_matches_choice_set(&current.session, choice_set)
            || !selected_model_matches_choice_set(&selected_model, choice_set)
            || confirmation.markdown_manifest_digests.len() != 2
            || confirmation.markdown_manifest_digests[0]
                != current.document_manifest.aggregate_digest
            || canonical_document_manifest_digest(std::slice::from_ref(
                &confirmation.markdown_entry,
            ))
            .as_deref()
                != confirmation
                    .markdown_manifest_digests
                    .get(1)
                    .map(String::as_str)
            || !confirmation_delivery_is_bound(confirmation, &current.session)
            || confirmation.canonical_payload_revision(schedule.revision)
                != Some(confirmation.payload_revision)
            || confirmation.reminder_list_id != schedule.input.reminder_list_id
            || confirmation.reminder_count != schedule.input.reminder_count
            || confirmation.reminder_count as usize != confirmation.reminder_items.len()
            || confirmation.reminder_items.iter().any(|item| {
                item.due_at_ms != schedule.input.due_at_ms
                    || item.time_zone != schedule.input.time_zone
            })
        {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        let mut next = current.clone();
        next.session.revision = next
            .session
            .revision
            .checked_add(1)
            .ok_or(StoreError::ChoiceLoopStateConflict)?;
        next.session.state = ChoiceSessionState::AwaitingConfirmation;
        next.session.last_input_at_ms = updated_at_ms.max(next.session.last_input_at_ms);
        next.session.soft_idle_at_ms = next.session.last_input_at_ms + 1_800_000;
        next.session.stale_review_at_ms = next.session.last_input_at_ms + 86_400_000;
        next.session.pending_confirmation_id = Some(confirmation.id.clone());
        // Keep a revision-bound, hidden recovery copy of the exact ChoiceSet.
        // It is not selectable while AwaitingConfirmation, but a later
        // descriptor conflict can return to an explicit re-review without a
        // model retry or reconstructed alternatives.
        let mut recovery_choice_set = choice_set.clone();
        recovery_choice_set.id = format!(
            "confirm-recovery-{}",
            &sha256_hex(format!("{}:{}", choice_set.id, next.session.revision).as_bytes())[..32]
        );
        recovery_choice_set.session_revision = next.session.revision;
        recovery_choice_set.generated_at_ms = updated_at_ms;
        recovery_choice_set.expires_on_revision = next.session.revision;
        next.session.active_choice_set_id = Some(recovery_choice_set.id.clone());
        next.active_choice_set = Some(recovery_choice_set);
        // Confirmation is an immediate batching boundary. Retiring the batch
        // prevents an older quiet-window envelope from being rebound to the
        // new confirmation revision after restart.
        next.active_batch = None;
        next.confirmation = Some(confirmation.clone());
        if !next.is_permitted_successor_of(&current) {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        let render_record = markdown_render_record_for_confirmation(
            confirmation,
            next.session.revision,
            expected_generation,
            updated_at_ms,
        )?;
        retire_reconciled_markdown_intents_for_session(
            &transaction,
            &self.authority,
            &current.session.id,
            updated_at_ms,
        )?;
        persist_choice_loop_snapshot(&transaction, &self.authority, &next, updated_at_ms)?;
        persist_markdown_render_intent(&transaction, &self.authority, &render_record)?;
        transaction.commit()?;
        Ok((next, render_record.intent))
    }

    /// Test and internal compatibility projection for callers that need only
    /// the committed snapshot. The journal is still created atomically by the
    /// same command and can never be omitted.
    ///
    /// # Errors
    ///
    /// Returns any validation, runtime, replay, Store, or audit error from the
    /// atomic confirmation-and-render-intent command.
    pub fn commit_choice_confirmation(
        &mut self,
        confirmation: &ChoiceConsolidatedConfirmation,
        expected_generation: u64,
        updated_at_ms: i64,
    ) -> Result<ChoiceLoopSnapshot, StoreError> {
        self.commit_choice_confirmation_and_render_intent(
            confirmation,
            expected_generation,
            updated_at_ms,
        )
        .map(|(snapshot, _)| snapshot)
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
        validate_mission_command_batch(envelopes)?;
        let first = envelopes.first().ok_or(StoreError::InvalidCommandBatch)?;
        let transaction = self.connection.transaction()?;
        if !mission_command_batch_is_exact_retry(
            &transaction,
            &self.authority,
            self.trusted_broker.as_ref(),
            envelopes,
        )? {
            reject_mission_creation_during_started_channel_model(&transaction, first)?;
        }
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

    /// Applies an initial Mission-confirmation batch and binds its accepted
    /// channel origin in the same transaction. This is the only channel-input
    /// genesis route; an arbitrary conversation cannot be attached later by a
    /// caller racing Mission creation.
    ///
    /// # Errors
    ///
    /// Returns an error without a write for an invalid command batch, missing
    /// ready suggestion, mismatched pairing/source, or any Store invariant.
    #[allow(clippy::too_many_arguments)]
    pub fn execute_mission_command_batch_with_primary_channel_route(
        &mut self,
        envelopes: &[MissionCommandEnvelope],
        channel: ChannelKind,
        source_message_id: &str,
        suggestion_id: &str,
        bound_at_ms: i64,
    ) -> Result<Vec<MissionCommandResult>, StoreError> {
        validate_mission_command_batch(envelopes)?;
        if bound_at_ms < 0
            || !matches!(
                envelopes.first().map(|envelope| &envelope.command),
                Some(MissionCommand::Create { .. })
            )
        {
            return Err(StoreError::ChannelRouteConflict);
        }
        let first = envelopes.first().ok_or(StoreError::InvalidCommandBatch)?;
        let transaction = self.connection.transaction()?;
        if !mission_command_batch_is_exact_retry(
            &transaction,
            &self.authority,
            self.trusted_broker.as_ref(),
            envelopes,
        )? {
            reject_mission_creation_during_started_channel_model(&transaction, first)?;
        }
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
        let mission = &results
            .last()
            .ok_or(StoreError::InvalidCommandBatch)?
            .mission;
        let dispatch =
            load_channel_model_dispatch(&transaction, &self.authority, channel, source_message_id)?
                .filter(|value| {
                    value
                        .suggestion
                        .as_ref()
                        .is_some_and(|suggestion| suggestion.id == suggestion_id)
                })
                .ok_or(StoreError::ChannelRouteConflict)?;
        let observed = load_channel_observation(
            &transaction,
            &self.authority,
            dispatch.channel,
            &dispatch.source_message_id,
        )?
        .filter(|value| value.decision == ChannelInboundDecision::Accepted)
        .ok_or(StoreError::ChannelRouteConflict)?;
        let pairing = load_channel_pairing(&transaction, &self.authority, channel)?
            .ok_or(StoreError::ChannelRouteConflict)?;
        let envelope = observed.observation.envelope;
        if envelope.conversation_id != pairing.conversation_id
            || envelope.sender_id != pairing.owner_sender_id
        {
            return Err(StoreError::ChannelRouteConflict);
        }
        require_route_boundary_available(
            &transaction,
            &self.authority,
            &mission.id,
            channel,
            &envelope.conversation_id,
            &envelope.sender_id,
        )?;
        let route_set = primary_channel_route_set(mission, &pairing, &envelope, bound_at_ms)?;
        write_channel_route_set(&transaction, &self.authority, &route_set, &mission.owner_id)?;
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

    /// Persists one deterministic reactive-reply preview bound to the exact
    /// active self-chat-origin `ChoiceSet`. This creates no transport authority.
    ///
    /// # Errors
    ///
    /// Returns an error unless the preview and every current runtime, Choice,
    /// source-message, and self-chat binding are exact and auditable.
    pub fn prepare_choice_imessage_reply(
        &mut self,
        intent: &ChoiceIMessageReplyIntent,
    ) -> Result<(ChoiceIMessageReplyPreview, String), StoreError> {
        if !intent.is_valid() {
            return Err(StoreError::ChannelOutboundConflict);
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_runtime_enabled(&transaction, &self.authority, self.trusted_broker.as_ref())?;
        require_no_effect_fence(&transaction)?;
        verified_audit_tail(&transaction, &self.authority)?;
        verify_all_bindings(&transaction, &self.authority, self.trusted_broker.as_ref())?;
        if let Some((existing, status, _)) = load_choice_imessage_reply_by_source(
            &transaction,
            &self.authority,
            &intent.source_message_id,
        )? {
            let mut replay = intent.clone();
            replay.created_at_ms = existing.created_at_ms;
            replay.approved_at_ms = existing.approved_at_ms;
            replay.recovery_cursor.clone_from(&existing.recovery_cursor);
            return (existing == replay)
                .then_some((existing.preview, status))
                .ok_or(StoreError::ChannelOutboundConflict);
        }
        validate_choice_imessage_reply_current(&transaction, &self.authority, intent)?;
        let blob = self.authority.encrypt_json(
            intent,
            choice_imessage_reply_aad(
                &intent.preview.reply_id,
                &intent.source_message_id,
                &intent.choice_session_id,
                &intent.choice_set_id,
                intent.preview.preview_revision,
                &intent.preview.confirmation_digest,
            )
            .as_bytes(),
        )?;
        let hash = blob_hash(&blob);
        append_audit(
            &transaction,
            &self.authority,
            &AuditRecord {
                id: &format!("choice-imessage-reply-prepared-{}", intent.preview.reply_id),
                command_id: &intent.preview.reply_id,
                command_hash: &intent.preview.confirmation_digest,
                actor: "owner",
                action: CHOICE_IMESSAGE_REPLY_PREPARED_ACTION,
                entity_id: &intent.preview.reply_id,
                created_at_ms: intent.created_at_ms,
                state_kind: "choice:imessage_reply",
                state_hash: &hash,
            },
        )?;
        transaction.execute(
            "INSERT INTO choice_imessage_reply
             (reply_id, source_message_id, choice_session_id, choice_set_id,
              preview_revision, confirmation_digest, status_json,
              provider_message_id, encrypted_blob, blob_hash)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'prepared', NULL, ?7, ?8)",
            params![
                &intent.preview.reply_id,
                &intent.source_message_id,
                &intent.choice_session_id,
                &intent.choice_set_id,
                i64::try_from(intent.preview.preview_revision)
                    .map_err(|_| StoreError::ChannelOutboundConflict)?,
                &intent.preview.confirmation_digest,
                blob,
                hash,
            ],
        )?;
        transaction.commit()?;
        Ok((intent.preview.clone(), "prepared".to_owned()))
    }

    /// Returns only the body-free Host inputs needed to derive a reactive
    /// reply. The raw inbound body remains Store-private.
    ///
    /// # Errors
    ///
    /// Returns an error when durable bindings cannot be verified.
    pub fn current_choice_imessage_reply_context(
        &self,
    ) -> Result<Option<(ChoiceLoopSnapshot, ChannelPairing, String, String)>, StoreError> {
        verified_audit_tail(&self.connection, &self.authority)?;
        verify_all_bindings(
            &self.connection,
            &self.authority,
            self.trusted_broker.as_ref(),
        )?;
        let Some(snapshot) = load_choice_loop_snapshot(&self.connection, &self.authority)? else {
            return Ok(None);
        };
        let Some(begin) = load_choice_begin_record_by_session(
            &self.connection,
            &self.authority,
            &snapshot.session.id,
        )?
        else {
            return Ok(None);
        };
        let Some(source_message_id) = begin.source_envelope.provider_message_id else {
            return Ok(None);
        };
        if begin.source_envelope.surface != "imessage-self-chat" {
            return Ok(None);
        }
        let pairing =
            load_channel_pairing(&self.connection, &self.authority, ChannelKind::IMessage)?
                .ok_or(StoreError::ChannelOutboundConflict)?;
        Ok(Some((
            snapshot,
            pairing,
            source_message_id,
            begin.source_envelope.delivery_binding_id,
        )))
    }

    /// Consumes exactly one visible local approval for one persisted Choice
    /// reply. Only the first exact prepared transition can return `ExecuteNow`.
    ///
    /// # Errors
    ///
    /// Returns an error for missing approval or any runtime, preview, source,
    /// pairing, provenance, or audit drift.
    pub fn authorize_choice_imessage_reply(
        &mut self,
        reply_id: &str,
        preview_revision: u64,
        confirmation_digest: &str,
        explicitly_approved: bool,
        approved_at_ms: i64,
    ) -> Result<ChoiceIMessageReplyStart, StoreError> {
        if !explicitly_approved || approved_at_ms < 0 {
            return Err(StoreError::ChannelOutboundConflict);
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_no_effect_fence(&transaction)?;
        verified_audit_tail(&transaction, &self.authority)?;
        verify_all_bindings(&transaction, &self.authority, self.trusted_broker.as_ref())?;
        let (mut intent, status, _) =
            load_choice_imessage_reply_by_id(&transaction, &self.authority, reply_id)?
                .ok_or(StoreError::ChannelOutboundConflict)?;
        if intent.preview.preview_revision != preview_revision
            || intent.preview.confirmation_digest != confirmation_digest
        {
            return Err(StoreError::ChannelOutboundConflict);
        }
        let disposition = match status.as_str() {
            "delivered" => ChoiceIMessageReplyDisposition::AlreadySent,
            "authorized" => ChoiceIMessageReplyDisposition::RecoverOnly,
            "prepared" => {
                require_runtime_enabled(
                    &transaction,
                    &self.authority,
                    self.trusted_broker.as_ref(),
                )?;
                validate_choice_imessage_reply_current(&transaction, &self.authority, &intent)?;
                intent.approved_at_ms = Some(approved_at_ms.max(intent.created_at_ms));
                intent.recovery_cursor = load_channel_cursor(
                    &transaction,
                    &self.authority,
                    ChannelKind::IMessage,
                    &intent.pairing.conversation_id,
                )?;
                let blob = self.authority.encrypt_json(
                    &intent,
                    choice_imessage_reply_aad(
                        reply_id,
                        &intent.source_message_id,
                        &intent.choice_session_id,
                        &intent.choice_set_id,
                        intent.preview.preview_revision,
                        &intent.preview.confirmation_digest,
                    )
                    .as_bytes(),
                )?;
                let hash = blob_hash(&blob);
                append_audit(
                    &transaction,
                    &self.authority,
                    &AuditRecord {
                        id: &format!("choice-imessage-reply-authorized-{reply_id}"),
                        command_id: reply_id,
                        command_hash: confirmation_digest,
                        actor: "owner",
                        action: CHOICE_IMESSAGE_REPLY_AUTHORIZED_ACTION,
                        entity_id: reply_id,
                        created_at_ms: intent.approved_at_ms.unwrap_or(approved_at_ms),
                        state_kind: "choice:imessage_reply",
                        state_hash: &hash,
                    },
                )?;
                transaction.execute(
                    "UPDATE choice_imessage_reply
                     SET status_json = 'authorized', encrypted_blob = ?1, blob_hash = ?2
                     WHERE reply_id = ?3 AND status_json = 'prepared'",
                    params![blob, hash, reply_id],
                )?;
                ChoiceIMessageReplyDisposition::ExecuteNow
            }
            _ => return Err(StoreError::ChannelOutboundConflict),
        };
        transaction.commit()?;
        Ok(ChoiceIMessageReplyStart {
            intent,
            disposition,
        })
    }

    /// Records the provider GUID returned by the single authorized send. An
    /// uncertain result intentionally leaves the row authorized/recover-only.
    ///
    /// # Errors
    ///
    /// Returns an error unless the exact reply has consumed its one authority
    /// and the provider GUID is valid and unchanged.
    pub fn record_choice_imessage_reply_delivery(
        &mut self,
        reply_id: &str,
        provider_message_id: &str,
        delivered_at_ms: i64,
    ) -> Result<ChoiceIMessageReplyIntent, StoreError> {
        if provider_message_id.is_empty() || delivered_at_ms < 0 {
            return Err(StoreError::ChannelOutboundConflict);
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_no_effect_fence(&transaction)?;
        verified_audit_tail(&transaction, &self.authority)?;
        verify_all_bindings(&transaction, &self.authority, self.trusted_broker.as_ref())?;
        let (intent, status, existing_provider) =
            load_choice_imessage_reply_by_id(&transaction, &self.authority, reply_id)?
                .ok_or(StoreError::ChannelOutboundConflict)?;
        if status == "delivered" {
            return (existing_provider.as_deref() == Some(provider_message_id))
                .then_some(intent)
                .ok_or(StoreError::ChannelOutboundConflict);
        }
        if status != "authorized" || intent.approved_at_ms.is_none() {
            return Err(StoreError::ChannelOutboundConflict);
        }
        append_audit(
            &transaction,
            &self.authority,
            &AuditRecord {
                id: &format!("choice-imessage-reply-delivered-{reply_id}"),
                command_id: reply_id,
                command_hash: &intent.canonical_payload_sha256,
                actor: "imessage",
                action: CHOICE_IMESSAGE_REPLY_DELIVERED_ACTION,
                entity_id: reply_id,
                created_at_ms: delivered_at_ms,
                state_kind: "choice:imessage_reply",
                state_hash: &blob_hash(provider_message_id.as_bytes()),
            },
        )?;
        transaction.execute(
            "UPDATE choice_imessage_reply
             SET status_json = 'delivered', provider_message_id = ?1
             WHERE reply_id = ?2 AND status_json = 'authorized'",
            params![provider_message_id, reply_id],
        )?;
        transaction.commit()?;
        Ok(intent)
    }

    /// Verifies only the exact durable GUID and exact selected self-chat.
    ///
    /// # Errors
    ///
    /// Returns an error when durable reply or audit bindings are invalid.
    pub fn verify_choice_imessage_reply_echo(
        &self,
        conversation_id: &str,
        provider_message_id: &str,
    ) -> Result<bool, StoreError> {
        verified_audit_tail(&self.connection, &self.authority)?;
        verify_all_bindings(
            &self.connection,
            &self.authority,
            self.trusted_broker.as_ref(),
        )?;
        let row: Option<String> = self
            .connection
            .query_row(
                "SELECT reply_id FROM choice_imessage_reply
                 WHERE status_json = 'delivered' AND provider_message_id = ?1",
                [provider_message_id],
                |row| row.get(0),
            )
            .optional()?;
        let Some(reply_id) = row else {
            return Ok(false);
        };
        let Some((intent, _, _)) =
            load_choice_imessage_reply_by_id(&self.connection, &self.authority, &reply_id)?
        else {
            return Ok(false);
        };
        Ok(intent.pairing.conversation_id == conversation_id)
    }

    /// Persists the one owner-confirmed V1 pairing for a channel. Exact retries
    /// are idempotent; changing either owner or conversation requires a future
    /// explicit disconnect flow and never rotates this boundary implicitly.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid, conflicting, fenced, or tampered state.
    pub fn pair_channel(&mut self, pairing: &ChannelPairing) -> Result<(), StoreError> {
        validate_pairing(pairing)?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_no_effect_fence(&transaction)?;
        verified_audit_tail(&transaction, &self.authority)?;
        verify_all_bindings(&transaction, &self.authority, self.trusted_broker.as_ref())?;
        if let Some(existing) =
            load_channel_pairing(&transaction, &self.authority, pairing.channel)?
        {
            if existing == *pairing {
                return Ok(());
            }
            if pairing.channel == ChannelKind::IMessage
                && existing.imessage.is_none()
                && pairing.imessage.is_some()
                && pairing.owner_sender_id == existing.owner_sender_id
                && pairing.conversation_id == existing.conversation_id
                && pairing.paired_at_ms >= existing.paired_at_ms
            {
                let channel = channel_json(pairing.channel)?;
                let blob = self
                    .authority
                    .encrypt_json(pairing, channel_pairing_aad(&channel).as_bytes())?;
                let state_hash = blob_hash(&blob);
                let suffix = &state_hash[..16];
                append_audit(
                    &transaction,
                    &self.authority,
                    &AuditRecord {
                        id: &format!("channel:{channel}:pairing-upgrade:{suffix}"),
                        command_id: &format!("channel-pair-upgrade-{channel}-{suffix}"),
                        command_hash: &state_hash,
                        actor: &pairing.owner_sender_id,
                        action: CHANNEL_PAIRING_ACTION,
                        entity_id: &channel,
                        created_at_ms: pairing.paired_at_ms,
                        state_kind: "channelPairing",
                        state_hash: &state_hash,
                    },
                )?;
                if transaction.execute(
                    "UPDATE channel_pairing
                     SET encrypted_blob = ?1, paired_at_ms = ?2, blob_hash = ?3
                     WHERE channel_json = ?4",
                    params![blob, pairing.paired_at_ms, state_hash, channel,],
                )? != 1
                {
                    return Err(StoreError::ChannelPairingConflict);
                }
                transaction.commit()?;
                return Ok(());
            }
            return Err(StoreError::ChannelPairingConflict);
        }
        let channel = channel_json(pairing.channel)?;
        let blob = self
            .authority
            .encrypt_json(pairing, channel_pairing_aad(&channel).as_bytes())?;
        let state_hash = blob_hash(&blob);
        append_audit(
            &transaction,
            &self.authority,
            &AuditRecord {
                id: &format!("channel:{channel}:pairing"),
                command_id: &format!("channel-pair-{channel}"),
                command_hash: &state_hash,
                actor: &pairing.owner_sender_id,
                action: CHANNEL_PAIRING_ACTION,
                entity_id: &channel,
                created_at_ms: pairing.paired_at_ms,
                state_kind: "channelPairing",
                state_hash: &state_hash,
            },
        )?;
        transaction.execute(
            "INSERT INTO channel_pairing
                (channel_json, encrypted_blob, paired_at_ms, blob_hash)
             VALUES (?1, ?2, ?3, ?4)",
            params![channel, blob, pairing.paired_at_ms, state_hash],
        )?;
        transaction.commit()?;
        Ok(())
    }

    /// Returns the verified pairing for one channel.
    ///
    /// # Errors
    ///
    /// Returns an error if the signed audit chain or encrypted pairing is invalid.
    pub fn channel_pairing(
        &self,
        channel: ChannelKind,
    ) -> Result<Option<ChannelPairing>, StoreError> {
        verified_audit_tail(&self.connection, &self.authority)?;
        verify_all_bindings(
            &self.connection,
            &self.authority,
            self.trusted_broker.as_ref(),
        )?;
        load_channel_pairing(&self.connection, &self.authority, channel)
    }

    /// Returns the newest verified durable recovery cursor for one paired conversation.
    ///
    /// # Errors
    ///
    /// Returns an error if channel state or the audit chain is invalid.
    pub fn channel_cursor(
        &self,
        channel: ChannelKind,
        conversation_id: &str,
    ) -> Result<Option<ChannelCursor>, StoreError> {
        verified_audit_tail(&self.connection, &self.authority)?;
        verify_all_bindings(
            &self.connection,
            &self.authority,
            self.trusted_broker.as_ref(),
        )?;
        load_channel_cursor(&self.connection, &self.authority, channel, conversation_id)
    }

    /// Advances a paired channel's durable recovery high-water mark without
    /// inventing a message observation. This is used only for a provider's
    /// bounded recovery result after every accepted envelope in that result
    /// has been ingested.
    ///
    /// # Errors
    ///
    /// Returns an error for Off, an unpaired conversation, a changed cursor at
    /// the same order, or invalid durable state.
    pub fn advance_channel_cursor(&mut self, cursor: &ChannelCursor) -> Result<(), StoreError> {
        validate_cursor(cursor)?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_runtime_enabled(&transaction, &self.authority, self.trusted_broker.as_ref())?;
        require_no_effect_fence(&transaction)?;
        verified_audit_tail(&transaction, &self.authority)?;
        verify_all_bindings(&transaction, &self.authority, self.trusted_broker.as_ref())?;
        let pairing = load_channel_pairing(&transaction, &self.authority, cursor.channel)?
            .ok_or(StoreError::ChannelObservationConflict)?;
        if pairing.conversation_id != cursor.conversation_id {
            return Err(StoreError::ChannelObservationConflict);
        }
        if let Some(current) = load_channel_cursor(
            &transaction,
            &self.authority,
            cursor.channel,
            &cursor.conversation_id,
        )? {
            if current.channel == cursor.channel
                && current.conversation_id == cursor.conversation_id
                && current.opaque_value == cursor.opaque_value
                && current.order == cursor.order
            {
                return Ok(());
            }
            if cursor.order <= current.order {
                return Err(StoreError::ChannelObservationConflict);
            }
        }
        write_channel_cursor(&transaction, &self.authority, cursor)?;
        transaction.commit()?;
        Ok(())
    }

    /// Atomically filters one adapter observation before its body can reach a
    /// model, records durable dedupe provenance, and advances only the paired
    /// conversation's monotonic recovery cursor. Global Off rejects the whole
    /// operation before any channel state moves.
    ///
    /// # Errors
    ///
    /// Returns an error for Off, malformed/conflicting input, or invalid durable state.
    pub fn ingest_channel_message(
        &mut self,
        observation: &ChannelObservation,
        content: &str,
    ) -> Result<ChannelInboundResult, StoreError> {
        validate_observation(observation)?;
        validate_channel_content(observation, content)?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_runtime_enabled(&transaction, &self.authority, self.trusted_broker.as_ref())?;
        require_no_effect_fence(&transaction)?;
        verified_audit_tail(&transaction, &self.authority)?;
        verify_all_bindings(&transaction, &self.authority, self.trusted_broker.as_ref())?;
        let Some(pairing) =
            load_channel_pairing(&transaction, &self.authority, observation.envelope.channel)?
        else {
            return Ok(ignored_channel_inbound(
                ChannelInboundDecision::IgnoredUnpaired,
                observation.cursor.clone(),
            ));
        };
        if pairing.conversation_id != observation.envelope.conversation_id {
            return Ok(ignored_channel_inbound(
                ChannelInboundDecision::IgnoredConversation,
                observation.cursor.clone(),
            ));
        }
        if let Some(existing) = load_channel_observation(
            &transaction,
            &self.authority,
            observation.envelope.channel,
            &observation.envelope.source_message_id,
        )? {
            if existing.observation == *observation {
                return Ok(ChannelInboundResult {
                    decision: ChannelInboundDecision::Duplicate,
                    cursor: existing.observation.cursor,
                    mission_event: load_channel_mission_event(
                        &transaction,
                        &self.authority,
                        observation.envelope.channel,
                        &observation.envelope.source_message_id,
                    )?,
                });
            }
            return Err(StoreError::ChannelObservationConflict);
        }
        if let Some(current) = load_channel_cursor(
            &transaction,
            &self.authority,
            observation.cursor.channel,
            &observation.cursor.conversation_id,
        )? && observation.cursor.order <= current.order
        {
            return Ok(ignored_channel_inbound(
                ChannelInboundDecision::IgnoredStaleCursor,
                current,
            ));
        }
        let (decision, mission_event) =
            classify_channel_inbound(&transaction, &self.authority, &pairing, observation)?;
        write_channel_observation(
            &transaction,
            &self.authority,
            observation,
            decision,
            content,
        )?;
        if decision == ChannelInboundDecision::Accepted {
            write_channel_model_dispatch(
                &transaction,
                &self.authority,
                &StoredChannelModelDispatch {
                    channel: observation.envelope.channel,
                    source_message_id: observation.envelope.source_message_id.clone(),
                    state: StoredChannelModelState::Queued,
                    suggestion: None,
                },
                observation.envelope.received_at_ms,
            )?;
        }
        if let Some(event) = &mission_event {
            write_channel_mission_event(&transaction, &self.authority, event)?;
            record_channel_mission_participation(
                &transaction,
                &self.authority,
                self.trusted_broker.as_ref(),
                event,
            )?;
        }
        write_channel_cursor(&transaction, &self.authority, &observation.cursor)?;
        transaction.commit()?;
        Ok(ChannelInboundResult {
            decision,
            cursor: observation.cursor.clone(),
            mission_event,
        })
    }

    /// Consumes model-dispatch authority for one accepted channel message.
    /// Exact retries become recovery-only, while a persisted result is returned
    /// without another model call.
    ///
    /// # Errors
    ///
    /// Returns an error for Off, a non-accepted source, or invalid durable state.
    pub fn begin_channel_model(
        &mut self,
        channel: ChannelKind,
        source_message_id: &str,
    ) -> Result<ChannelModelStart, StoreError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_runtime_enabled(&transaction, &self.authority, self.trusted_broker.as_ref())?;
        require_no_effect_fence(&transaction)?;
        verified_audit_tail(&transaction, &self.authority)?;
        verify_all_bindings(&transaction, &self.authority, self.trusted_broker.as_ref())?;
        let observed =
            load_channel_observation(&transaction, &self.authority, channel, source_message_id)?
                .filter(|value| value.decision == ChannelInboundDecision::Accepted)
                .ok_or(StoreError::ChannelObservationConflict)?;
        let content = observed
            .accepted_content
            .clone()
            .ok_or(StoreError::ChannelObservationConflict)?;
        if let Some(existing) =
            load_channel_model_dispatch(&transaction, &self.authority, channel, source_message_id)?
        {
            match existing.state {
                StoredChannelModelState::Queued => {
                    if nonterminal_mission_exists(&transaction)? {
                        return Err(StoreError::ChannelModelDeferredByMission);
                    }
                    let encoded_channel = channel_json(channel)?;
                    let oldest_queued = transaction
                        .query_row(
                            "SELECT dispatch.source_message_id
                             FROM channel_model_dispatch AS dispatch
                             JOIN channel_observation AS observation
                               ON observation.channel_json = dispatch.channel_json
                              AND observation.source_message_id = dispatch.source_message_id
                             WHERE dispatch.channel_json = ?1
                               AND dispatch.status_json = 'queued'
                             ORDER BY observation.cursor_order ASC,
                                      dispatch.source_message_id ASC
                             LIMIT 1",
                            [&encoded_channel],
                            |row| row.get::<_, String>(0),
                        )
                        .optional()?;
                    let another_started = transaction.query_row(
                        "SELECT EXISTS(
                             SELECT 1 FROM channel_model_dispatch
                             WHERE channel_json = ?1 AND status_json = 'started'
                         )",
                        [&encoded_channel],
                        |row| row.get::<_, bool>(0),
                    )?;
                    if another_started || oldest_queued.as_deref() != Some(source_message_id) {
                        return Err(StoreError::ChannelObservationConflict);
                    }
                    update_channel_model_started(
                        &transaction,
                        &self.authority,
                        &StoredChannelModelDispatch {
                            state: StoredChannelModelState::Started,
                            ..existing
                        },
                        observed.observation.envelope.received_at_ms,
                    )?;
                    transaction.commit()?;
                    return Ok(ChannelModelStart {
                        envelope: observed.observation.envelope,
                        content,
                        disposition: ChannelModelDisposition::ExecuteNow,
                        suggestion: None,
                    });
                }
                StoredChannelModelState::Started
                | StoredChannelModelState::Failed
                | StoredChannelModelState::Ready => {}
            }
            return Ok(ChannelModelStart {
                envelope: observed.observation.envelope,
                content,
                disposition: match existing.state {
                    StoredChannelModelState::Started | StoredChannelModelState::Failed => {
                        ChannelModelDisposition::RecoverOnly
                    }
                    StoredChannelModelState::Ready => ChannelModelDisposition::SuggestionReady,
                    StoredChannelModelState::Queued => unreachable!(),
                },
                suggestion: existing.suggestion,
            });
        }
        Err(StoreError::ChannelObservationConflict)
    }

    /// Returns the oldest accepted model task whose one-shot execution
    /// authority has not yet been consumed. The observation, cursor, and this
    /// durable queue entry are created in one transaction, so a Host restart
    /// can claim it before polling beyond the persisted cursor.
    ///
    /// # Errors
    ///
    /// Returns an error for Off or invalid durable channel state.
    pub fn next_queued_channel_model(
        &self,
        channel: ChannelKind,
    ) -> Result<Option<String>, StoreError> {
        require_runtime_enabled(
            &self.connection,
            &self.authority,
            self.trusted_broker.as_ref(),
        )?;
        verified_audit_tail(&self.connection, &self.authority)?;
        verify_all_bindings(
            &self.connection,
            &self.authority,
            self.trusted_broker.as_ref(),
        )?;
        let encoded_channel = channel_json(channel)?;
        let source_message_id = self
            .connection
            .query_row(
                "SELECT dispatch.source_message_id
                 FROM channel_model_dispatch AS dispatch
                 JOIN channel_observation AS observation
                   ON observation.channel_json = dispatch.channel_json
                  AND observation.source_message_id = dispatch.source_message_id
                 WHERE dispatch.channel_json = ?1 AND dispatch.status_json = 'queued'
                   AND NOT EXISTS(
                       SELECT 1 FROM channel_model_dispatch AS active
                       WHERE active.channel_json = dispatch.channel_json
                         AND active.status_json = 'started'
                   )
                 ORDER BY observation.cursor_order ASC, dispatch.source_message_id ASC
                 LIMIT 1",
                [encoded_channel],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        source_message_id
            .map(|source_message_id| {
                let dispatch = load_channel_model_dispatch(
                    &self.connection,
                    &self.authority,
                    channel,
                    &source_message_id,
                )?
                .ok_or_else(|| StoreError::ChannelStateMismatch(source_message_id.clone()))?;
                if dispatch.state != StoredChannelModelState::Queued {
                    return Err(StoreError::ChannelStateMismatch(source_message_id));
                }
                Ok(dispatch.source_message_id)
            })
            .transpose()
    }

    /// Returns the exact in-flight channel model dispatch whose one-shot
    /// execution authority was consumed before a result was persisted. A Host
    /// restart uses this recovery-only identity to surface `Need you`; it must
    /// never grant another model call or skip forward to queued work.
    ///
    /// # Errors
    ///
    /// Returns an error for Off, invalid durable state, or more than one
    /// in-flight dispatch on the same channel.
    pub fn started_channel_model(
        &self,
        channel: ChannelKind,
    ) -> Result<Option<String>, StoreError> {
        require_runtime_enabled(
            &self.connection,
            &self.authority,
            self.trusted_broker.as_ref(),
        )?;
        verified_audit_tail(&self.connection, &self.authority)?;
        verify_all_bindings(
            &self.connection,
            &self.authority,
            self.trusted_broker.as_ref(),
        )?;
        let encoded_channel = channel_json(channel)?;
        let source_message_ids = self
            .connection
            .prepare(
                "SELECT source_message_id
                 FROM channel_model_dispatch
                 WHERE channel_json = ?1 AND status_json = 'started'
                 ORDER BY source_message_id ASC
                 LIMIT 2",
            )?
            .query_map([encoded_channel], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        match source_message_ids.as_slice() {
            [] => Ok(None),
            [source_message_id] => Ok(Some(source_message_id.clone())),
            _ => Err(StoreError::ChannelObservationConflict),
        }
    }

    /// Returns the newest accepted channel dispatch only when that exact
    /// dispatch ended in a durable failure. This keeps the owner-visible
    /// `Need you` state recoverable across Host/Core restarts without granting
    /// another model call. A later queued or ready correction supersedes the
    /// old failure by becoming the newest dispatch.
    ///
    /// # Errors
    ///
    /// Returns an error for Off or invalid durable channel state.
    pub fn latest_failed_channel_model(
        &self,
        channel: ChannelKind,
    ) -> Result<Option<String>, StoreError> {
        require_runtime_enabled(
            &self.connection,
            &self.authority,
            self.trusted_broker.as_ref(),
        )?;
        verified_audit_tail(&self.connection, &self.authority)?;
        verify_all_bindings(
            &self.connection,
            &self.authority,
            self.trusted_broker.as_ref(),
        )?;
        let encoded_channel = channel_json(channel)?;
        let source_message_id = self
            .connection
            .query_row(
                "SELECT dispatch.source_message_id
                 FROM channel_model_dispatch AS dispatch
                 JOIN channel_observation AS observation
                   ON observation.channel_json = dispatch.channel_json
                  AND observation.source_message_id = dispatch.source_message_id
                 WHERE dispatch.channel_json = ?1
                 ORDER BY observation.cursor_order DESC,
                          dispatch.source_message_id DESC
                 LIMIT 1",
                [encoded_channel],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        source_message_id
            .map(|source_message_id| {
                let dispatch = load_channel_model_dispatch(
                    &self.connection,
                    &self.authority,
                    channel,
                    &source_message_id,
                )?
                .ok_or_else(|| StoreError::ChannelStateMismatch(source_message_id.clone()))?;
                Ok((dispatch.state == StoredChannelModelState::Failed)
                    .then_some(dispatch.source_message_id))
            })
            .transpose()
            .map(Option::flatten)
    }

    /// Returns every verified terminal channel-model incident in stable
    /// chronological order. Acknowledged incidents remain visible; this read
    /// grants no model or provider authority.
    ///
    /// # Errors
    ///
    /// Returns an error for a malformed incident, dispatch, or audit binding.
    pub fn channel_failure_incidents(
        &self,
        channel: Option<ChannelKind>,
    ) -> Result<Vec<ChannelFailureIncident>, StoreError> {
        verified_audit_tail(&self.connection, &self.authority)?;
        verify_all_bindings(
            &self.connection,
            &self.authority,
            self.trusted_broker.as_ref(),
        )?;
        let incident_ids = match channel {
            Some(channel) => {
                let encoded = channel_json(channel)?;
                self.connection
                    .prepare(
                        "SELECT incident_id FROM channel_failure_incident
                         WHERE channel_json = ?1 ORDER BY incident_id",
                    )?
                    .query_map([encoded], |row| row.get::<_, String>(0))?
                    .collect::<Result<Vec<_>, _>>()?
            }
            None => self
                .connection
                .prepare("SELECT incident_id FROM channel_failure_incident ORDER BY incident_id")?
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?,
        };
        let mut incidents = incident_ids
            .into_iter()
            .map(|incident_id| {
                load_channel_failure_incident(&self.connection, &self.authority, &incident_id)?
                    .map(channel_failure_incident_public)
                    .transpose()?
                    .ok_or(StoreError::ChannelFailureIncidentConflict(incident_id))
            })
            .collect::<Result<Vec<_>, _>>()?;
        incidents.sort_by(|left, right| {
            left.occurred_at_ms
                .cmp(&right.occurred_at_ms)
                .then_with(|| left.incident_id.cmp(&right.incident_id))
        });
        Ok(incidents)
    }

    /// Returns a bounded, recoverable incident projection for product RPCs.
    ///
    /// Oldest unacknowledged incidents take priority so every outstanding
    /// incident eventually becomes actionable. Any spare capacity is filled
    /// with the newest acknowledged history. Acknowledging the first row of a
    /// full unacknowledged page therefore reveals the next durable row without
    /// deleting or rewriting any incident evidence.
    ///
    /// # Errors
    ///
    /// Returns an error if any incident, dispatch, or audit binding in the
    /// complete durable history is invalid.
    pub fn channel_failure_incident_projection(
        &self,
        channel: Option<ChannelKind>,
    ) -> Result<Vec<ChannelFailureIncident>, StoreError> {
        let incidents = self.channel_failure_incidents(channel)?;
        if incidents.len() <= CHANNEL_FAILURE_INCIDENT_PROJECTION_LIMIT {
            return Ok(incidents);
        }

        let mut projected = incidents
            .iter()
            .filter(|incident| incident.acknowledgement.is_none())
            .take(CHANNEL_FAILURE_INCIDENT_PROJECTION_LIMIT)
            .cloned()
            .collect::<Vec<_>>();
        let remaining = CHANNEL_FAILURE_INCIDENT_PROJECTION_LIMIT - projected.len();
        if remaining > 0 {
            projected.extend(
                incidents
                    .iter()
                    .rev()
                    .filter(|incident| incident.acknowledgement.is_some())
                    .take(remaining)
                    .cloned(),
            );
        }
        projected.sort_by(|left, right| {
            left.occurred_at_ms
                .cmp(&right.occurred_at_ms)
                .then_with(|| left.incident_id.cmp(&right.incident_id))
        });
        Ok(projected)
    }

    /// Atomically acknowledges one exact incident against its immutable
    /// creation anchor and the current protected runtime revision. Exact
    /// response-loss retries are idempotent and never alter the failed
    /// dispatch or grant new model/provider authority.
    ///
    /// # Errors
    ///
    /// Returns an error for Off, stale identity/revision/anchor, invalid time,
    /// or any Store/audit mismatch.
    pub fn acknowledge_channel_failure_incident(
        &mut self,
        incident_id: &str,
        expected_incident_anchor: &AuditAnchor,
        runtime_revision: u64,
        acknowledged_at_ms: i64,
    ) -> Result<ChannelFailureIncident, StoreError> {
        if acknowledged_at_ms < 0 {
            return Err(StoreError::ChannelFailureIncidentConflict(
                incident_id.to_owned(),
            ));
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_no_effect_fence(&transaction)?;
        let runtime =
            load_runtime_control(&transaction, &self.authority, self.trusted_broker.as_ref())?;
        if !runtime.enabled || runtime.revision != runtime_revision {
            return Err(StoreError::RuntimeDisabled);
        }
        verified_audit_tail(&transaction, &self.authority)?;
        verify_all_bindings(&transaction, &self.authority, self.trusted_broker.as_ref())?;
        let loaded = load_channel_failure_incident(&transaction, &self.authority, incident_id)?
            .ok_or_else(|| StoreError::ChannelFailureIncidentConflict(incident_id.to_owned()))?;
        if &loaded.incident_anchor != expected_incident_anchor {
            return Err(StoreError::AuditAnchorMismatch);
        }
        if acknowledged_at_ms < loaded.stored.occurred_at_ms
            || runtime_revision < loaded.stored.runtime_revision
        {
            return Err(StoreError::ChannelFailureIncidentConflict(
                incident_id.to_owned(),
            ));
        }
        if loaded.stored.acknowledgement.is_some() {
            return channel_failure_incident_public(loaded);
        }
        let mut acknowledged = loaded.stored;
        acknowledged.acknowledgement = Some(StoredChannelFailureAcknowledgement {
            acknowledged_at_ms,
            runtime_revision,
            incident_anchor: expected_incident_anchor.clone(),
        });
        update_channel_failure_acknowledgement(&transaction, &self.authority, &acknowledged)?;
        transaction.commit()?;
        load_channel_failure_incident(&self.connection, &self.authority, incident_id)?
            .map(channel_failure_incident_public)
            .transpose()?
            .ok_or_else(|| StoreError::ChannelFailureIncidentConflict(incident_id.to_owned()))
    }

    /// Reports whether one channel still has a queued or in-flight model
    /// dispatch. A ready suggestion is not current while later accepted work
    /// remains unresolved, including after a Host restart that lost a model
    /// response after consuming its one-shot authority.
    ///
    /// # Errors
    ///
    /// Returns an error if durable channel state or the audit chain is invalid.
    pub fn channel_model_work_pending(&self, channel: ChannelKind) -> Result<bool, StoreError> {
        verified_audit_tail(&self.connection, &self.authority)?;
        verify_all_bindings(
            &self.connection,
            &self.authority,
            self.trusted_broker.as_ref(),
        )?;
        let encoded_channel = channel_json(channel)?;
        let pending = self.connection.query_row(
            "SELECT EXISTS(
                 SELECT 1 FROM channel_model_dispatch
                 WHERE channel_json = ?1 AND status_json IN ('queued', 'started')
             )",
            [encoded_channel],
            |row| row.get::<_, bool>(0),
        )?;
        Ok(pending)
    }

    /// Returns the bounded chronological model context for one claimed
    /// dispatch. The current message stands alone unless the approved owner
    /// explicitly starts it with `Correction to previous:`. That directive may
    /// bind only the immediately preceding ready message, and only when this
    /// later message was already durable before the predecessor's suggestion
    /// completed. Time overlap alone never grants cross-message context.
    ///
    /// # Errors
    ///
    /// Returns an error for an unclaimed message, invalid durable state,
    /// changed sender/conversation, or an over-bounded correction chain. An
    /// explicit directive without one qualifying predecessor remains a
    /// single-message request and imports no earlier content.
    pub fn channel_model_context(
        &self,
        channel: ChannelKind,
        source_message_id: &str,
    ) -> Result<Vec<(ChannelEnvelope, String)>, StoreError> {
        verified_audit_tail(&self.connection, &self.authority)?;
        verify_all_bindings(
            &self.connection,
            &self.authority,
            self.trusted_broker.as_ref(),
        )?;
        let current_dispatch = load_channel_model_dispatch(
            &self.connection,
            &self.authority,
            channel,
            source_message_id,
        )?
        .ok_or(StoreError::ChannelObservationConflict)?;
        if current_dispatch.state != StoredChannelModelState::Started
            || current_dispatch.suggestion.is_some()
        {
            return Err(StoreError::ChannelObservationConflict);
        }
        let current = load_channel_observation(
            &self.connection,
            &self.authority,
            channel,
            source_message_id,
        )?
        .filter(|value| value.decision == ChannelInboundDecision::Accepted)
        .ok_or(StoreError::ChannelObservationConflict)?;
        let current_content = current
            .accepted_content
            .clone()
            .ok_or(StoreError::ChannelObservationConflict)?;
        let source_ids = if explicit_channel_correction(&current_content) {
            qualified_channel_correction_predecessor(
                &self.connection,
                &self.authority,
                channel,
                &current,
            )?
            .into_iter()
            .collect()
        } else {
            Vec::new()
        };
        if source_ids.len().saturating_add(1) > MAX_CHANNEL_MODEL_CONTEXT_MESSAGES {
            return Err(StoreError::ChannelObservationConflict);
        }
        let mut context = Vec::with_capacity(source_ids.len().saturating_add(1));
        for source_id in source_ids {
            let observed =
                load_channel_observation(&self.connection, &self.authority, channel, &source_id)?
                    .filter(|value| value.decision == ChannelInboundDecision::Accepted)
                    .ok_or(StoreError::ChannelObservationConflict)?;
            let envelope = observed.observation.envelope;
            if envelope.conversation_id != current.observation.envelope.conversation_id
                || envelope.sender_id != current.observation.envelope.sender_id
            {
                return Err(StoreError::ChannelObservationConflict);
            }
            context.push((
                envelope,
                observed
                    .accepted_content
                    .ok_or(StoreError::ChannelObservationConflict)?,
            ));
        }
        context.push((current.observation.envelope, current_content));
        Ok(context)
    }

    /// Persists the exact structured result for a consumed channel model call.
    /// The signed runtime must still be On in the same immediate transaction;
    /// a result that crosses Global Off is rejected and never becomes ready.
    ///
    /// # Errors
    ///
    /// Returns an error for an unknown dispatch, changed result, or invalid state.
    pub fn record_channel_suggestion(
        &mut self,
        channel: ChannelKind,
        source_message_id: &str,
        suggestion: &OutcomeSuggestion,
        observed_at_ms: i64,
    ) -> Result<(), StoreError> {
        if !valid_channel_suggestion(suggestion) || observed_at_ms < 0 {
            return Err(StoreError::ChannelObservationConflict);
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_runtime_enabled(&transaction, &self.authority, self.trusted_broker.as_ref())?;
        require_no_effect_fence(&transaction)?;
        verified_audit_tail(&transaction, &self.authority)?;
        verify_all_bindings(&transaction, &self.authority, self.trusted_broker.as_ref())?;
        let existing =
            load_channel_model_dispatch(&transaction, &self.authority, channel, source_message_id)?
                .ok_or(StoreError::ChannelObservationConflict)?;
        if let Some(current) = existing.suggestion {
            if current == *suggestion {
                return Ok(());
            }
            return Err(StoreError::ChannelObservationConflict);
        }
        update_channel_model_suggestion(
            &transaction,
            &self.authority,
            &StoredChannelModelDispatch {
                channel,
                source_message_id: source_message_id.to_owned(),
                state: StoredChannelModelState::Ready,
                suggestion: Some(suggestion.clone()),
            },
            observed_at_ms,
        )?;
        transaction.commit()?;
        Ok(())
    }

    /// Atomically records that a consumed one-shot channel model dispatch did
    /// not produce a durable suggestion. The failed source can never execute
    /// again; a later explicit owner correction is a distinct dispatch.
    ///
    /// # Errors
    ///
    /// Returns an error for an unknown dispatch, a non-started state, invalid
    /// time, or any audit/binding mismatch.
    pub fn fail_channel_model(
        &mut self,
        channel: ChannelKind,
        source_message_id: &str,
        observed_at_ms: i64,
    ) -> Result<(), StoreError> {
        if observed_at_ms < 0 {
            return Err(StoreError::ChannelObservationConflict);
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_no_effect_fence(&transaction)?;
        verified_audit_tail(&transaction, &self.authority)?;
        verify_all_bindings(&transaction, &self.authority, self.trusted_broker.as_ref())?;
        let existing =
            load_channel_model_dispatch(&transaction, &self.authority, channel, source_message_id)?
                .ok_or(StoreError::ChannelObservationConflict)?;
        match existing.state {
            StoredChannelModelState::Failed => return Ok(()),
            StoredChannelModelState::Started => {}
            StoredChannelModelState::Queued | StoredChannelModelState::Ready => {
                return Err(StoreError::ChannelObservationConflict);
            }
        }
        let runtime_revision =
            load_runtime_control(&transaction, &self.authority, self.trusted_broker.as_ref())?
                .revision;
        let (dispatch_state_hash, source_audit_anchor) = update_channel_model_failed(
            &transaction,
            &self.authority,
            &StoredChannelModelDispatch {
                state: StoredChannelModelState::Failed,
                ..existing
            },
            observed_at_ms,
        )?;
        let incident_id = channel_failure_incident_id(
            channel,
            source_message_id,
            &dispatch_state_hash,
            ChannelFailureClass::ModelResultUnavailable,
        );
        write_channel_failure_incident(
            &transaction,
            &self.authority,
            &StoredChannelFailureIncident {
                incident_id,
                channel,
                source_message_id: source_message_id.to_owned(),
                failure_class: ChannelFailureClass::ModelResultUnavailable,
                occurred_at_ms: observed_at_ms,
                runtime_revision,
                dispatch_state_hash,
                source_audit_anchor,
                acknowledgement: None,
            },
        )?;
        transaction.commit()?;
        Ok(())
    }

    /// Resolves a persisted suggestion to its accepted originating message.
    ///
    /// # Errors
    ///
    /// Returns an error if channel state or the audit chain is invalid.
    pub fn channel_source_for_suggestion(
        &self,
        suggestion_id: &str,
    ) -> Result<Option<ChannelEnvelope>, StoreError> {
        verified_audit_tail(&self.connection, &self.authority)?;
        verify_all_bindings(
            &self.connection,
            &self.authority,
            self.trusted_broker.as_ref(),
        )?;
        let source = self
            .connection
            .query_row(
                "SELECT channel_json, source_message_id FROM channel_model_dispatch
                 WHERE suggestion_id = ?1 AND status_json = 'ready'",
                [suggestion_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?;
        source
            .map(|(encoded, source_message_id)| {
                let channel: ChannelKind = serde_json::from_str(&encoded)
                    .map_err(|_| StoreError::ChannelStateMismatch(source_message_id.clone()))?;
                load_channel_observation(
                    &self.connection,
                    &self.authority,
                    channel,
                    &source_message_id,
                )?
                .filter(|value| value.decision == ChannelInboundDecision::Accepted)
                .map(|value| value.observation.envelope)
                .ok_or(StoreError::ChannelObservationConflict)
            })
            .transpose()
    }

    /// Returns the newest verified channel-origin suggestion, if any.
    ///
    /// # Errors
    ///
    /// Returns an error if channel state or the audit chain is invalid.
    pub fn latest_channel_suggestion(&self) -> Result<Option<OutcomeSuggestion>, StoreError> {
        verified_audit_tail(&self.connection, &self.authority)?;
        verify_all_bindings(
            &self.connection,
            &self.authority,
            self.trusted_broker.as_ref(),
        )?;
        let source = self
            .connection
            .query_row(
                "SELECT dispatch.channel_json, dispatch.source_message_id
                 FROM channel_model_dispatch AS dispatch
                 JOIN audit_ledger AS audit ON audit.entity_id = dispatch.entity_id
                    AND audit.action = ?1
                 WHERE dispatch.status_json = 'ready'
                 ORDER BY audit.sequence DESC LIMIT 1",
                [CHANNEL_SUGGESTION_ACTION],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?;
        source
            .map(|(encoded, source_message_id)| {
                let channel: ChannelKind = serde_json::from_str(&encoded)
                    .map_err(|_| StoreError::ChannelStateMismatch(source_message_id.clone()))?;
                load_channel_model_dispatch(
                    &self.connection,
                    &self.authority,
                    channel,
                    &source_message_id,
                )?
                .and_then(|value| value.suggestion)
                .ok_or(StoreError::ChannelObservationConflict)
            })
            .transpose()
    }

    /// Returns the newest verified ready suggestion for one exact channel.
    /// Callers must separately reject it while
    /// [`Self::channel_model_work_pending`] is true.
    ///
    /// # Errors
    ///
    /// Returns an error if channel state or the audit chain is invalid.
    pub fn latest_channel_suggestion_for(
        &self,
        channel: ChannelKind,
    ) -> Result<Option<OutcomeSuggestion>, StoreError> {
        verified_audit_tail(&self.connection, &self.authority)?;
        verify_all_bindings(
            &self.connection,
            &self.authority,
            self.trusted_broker.as_ref(),
        )?;
        let encoded_channel = channel_json(channel)?;
        let source_message_id = self
            .connection
            .query_row(
                "SELECT dispatch.source_message_id
                 FROM channel_model_dispatch AS dispatch
                 JOIN audit_ledger AS audit ON audit.entity_id = dispatch.entity_id
                    AND audit.action = ?1
                 WHERE dispatch.channel_json = ?2 AND dispatch.status_json = 'ready'
                 ORDER BY audit.sequence DESC LIMIT 1",
                params![CHANNEL_SUGGESTION_ACTION, encoded_channel],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        source_message_id
            .map(|source_message_id| {
                load_channel_model_dispatch(
                    &self.connection,
                    &self.authority,
                    channel,
                    &source_message_id,
                )?
                .and_then(|value| value.suggestion)
                .ok_or(StoreError::ChannelObservationConflict)
            })
            .transpose()
    }

    /// Returns the verified route set for one confirmed Mission.
    ///
    /// # Errors
    ///
    /// Returns an error if channel state or the audit chain is invalid.
    pub fn channel_route_set(
        &self,
        mission_id: &str,
    ) -> Result<Option<ChannelRouteSet>, StoreError> {
        verified_audit_tail(&self.connection, &self.authority)?;
        verify_all_bindings(
            &self.connection,
            &self.authority,
            self.trusted_broker.as_ref(),
        )?;
        load_channel_route_set(&self.connection, &self.authority, mission_id)
    }

    /// Returns the verified Mission-bound participation for one provider
    /// message, if that message was routed rather than queued as free intent.
    ///
    /// # Errors
    ///
    /// Returns an error for a damaged audit or encrypted channel binding.
    pub fn channel_mission_event(
        &self,
        channel: ChannelKind,
        source_message_id: &str,
    ) -> Result<Option<ChannelMissionEvent>, StoreError> {
        verified_audit_tail(&self.connection, &self.authority)?;
        verify_all_bindings(
            &self.connection,
            &self.authority,
            self.trusted_broker.as_ref(),
        )?;
        load_channel_mission_event(
            &self.connection,
            &self.authority,
            channel,
            source_message_id,
        )
    }

    /// Atomically binds one additional durable pairing after an exact typed
    /// owner approval. The Store derives the route/audit identities and owns
    /// the complete route-set replacement.
    ///
    /// # Errors
    ///
    /// Returns without state or audit movement for Off, rejection, stale
    /// revision, changed pairing, changed classes, wrong owner, or overlap.
    pub fn bind_additional_channel_route(
        &mut self,
        approval: &ChannelRouteApproval,
    ) -> Result<ChannelRouteSet, StoreError> {
        validate_route_approval(approval)?;
        if approval.decision != ChannelRouteApprovalDecision::Approve {
            return Err(StoreError::ChannelRouteConflict);
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_runtime_enabled(&transaction, &self.authority, self.trusted_broker.as_ref())?;
        require_no_effect_fence(&transaction)?;
        verified_audit_tail(&transaction, &self.authority)?;
        verify_all_bindings(&transaction, &self.authority, self.trusted_broker.as_ref())?;
        let mission = load_mission_for_update(&transaction, &self.authority, &approval.mission_id)?
            .ok_or(StoreError::MissionNotFound)?;
        if mission.status.is_terminal() || mission.owner_id != approval.actor_id {
            return Err(StoreError::ChannelRouteConflict);
        }
        let pairing = load_channel_pairing(&transaction, &self.authority, approval.channel)?
            .ok_or(StoreError::ChannelRouteConflict)?;
        if pairing.conversation_id != approval.conversation_id
            || pairing.owner_sender_id != approval.owner_sender_id
            || pairing_provider_identity(&pairing) != approval.provider_identity
        {
            return Err(StoreError::ChannelRouteConflict);
        }
        let mut route_set =
            load_channel_route_set(&transaction, &self.authority, &approval.mission_id)?
                .ok_or(StoreError::ChannelRouteConflict)?;
        if let Some(existing) = route_set
            .routes
            .iter()
            .find(|route| route.approval_id == approval.approval_id)
        {
            if existing.channel == approval.channel
                && existing.conversation_id == approval.conversation_id
                && existing.owner_sender_id == approval.owner_sender_id
                && existing.provider_identity == approval.provider_identity
                && existing.allowed_inbound_classes == approval.allowed_inbound_classes
                && existing.allowed_outbound_classes == approval.allowed_outbound_classes
                && approval
                    .expected_route_set_revision
                    .checked_add(1)
                    .is_some_and(|expected| route_set.revision == expected)
            {
                return Ok(route_set);
            }
            return Err(StoreError::ChannelRouteConflict);
        }
        if route_set.revision != approval.expected_route_set_revision
            || route_set.routes.iter().any(|route| {
                route.channel == approval.channel
                    && route.conversation_id == approval.conversation_id
                    && route.owner_sender_id == approval.owner_sender_id
            })
        {
            return Err(StoreError::ChannelRouteConflict);
        }
        require_route_boundary_available(
            &transaction,
            &self.authority,
            &mission.id,
            approval.channel,
            &approval.conversation_id,
            &approval.owner_sender_id,
        )?;
        let revision = route_set
            .revision
            .checked_add(1)
            .ok_or(StoreError::ChannelRouteConflict)?;
        route_set.routes.push(ChannelRoute {
            route_id: channel_route_id(
                &mission.id,
                approval.channel,
                &approval.conversation_id,
                &approval.owner_sender_id,
            ),
            role: ChannelRouteRole::Additional,
            channel: approval.channel,
            conversation_id: approval.conversation_id.clone(),
            owner_sender_id: approval.owner_sender_id.clone(),
            provider_identity: approval.provider_identity.clone(),
            source_message_id: pairing_source_identity(&pairing),
            allowed_inbound_classes: approval.allowed_inbound_classes.clone(),
            allowed_outbound_classes: approval.allowed_outbound_classes.clone(),
            revision,
            approval_id: approval.approval_id.clone(),
            audit_id: format!("channel-route-{}-{revision}", mission.id),
            bound_at_ms: approval.decided_at_ms,
            updated_at_ms: approval.decided_at_ms,
        });
        route_set.revision = revision;
        write_channel_route_set(&transaction, &self.authority, &route_set, &mission.owner_id)?;
        transaction.commit()?;
        Ok(route_set)
    }

    /// Durably consumes outbound execution authority before an adapter may
    /// perform a send. After response loss or restart the same intent is
    /// recovery-only and is never authorized for a second external write.
    ///
    /// # Errors
    ///
    /// Returns an error for Off, missing exact approvals, changed bytes, or invalid state.
    pub fn begin_channel_outbound(
        &mut self,
        intent: &ChannelOutboundIntent,
        content: &[u8],
    ) -> Result<ChannelOutboundStart, StoreError> {
        validate_outbound(intent)?;
        if format!("{:x}", Sha256::digest(content)) != intent.content_sha256 {
            return Err(StoreError::ChannelOutboundConflict);
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_runtime_enabled(&transaction, &self.authority, self.trusted_broker.as_ref())?;
        require_no_effect_fence(&transaction)?;
        verified_audit_tail(&transaction, &self.authority)?;
        verify_all_bindings(&transaction, &self.authority, self.trusted_broker.as_ref())?;
        if let Some(existing) =
            load_channel_outbound_for_recovery(&transaction, &self.authority, intent)?
        {
            return Ok(ChannelOutboundStart {
                intent: existing.intent,
                disposition: if existing.delivery.is_some() {
                    ChannelOutboundDisposition::AlreadySent
                } else {
                    ChannelOutboundDisposition::RecoverOnly
                },
                provider_message_id: existing
                    .delivery
                    .map(|delivery| delivery.provider_message_id),
            });
        }
        if let Some(existing) =
            load_channel_outbound(&transaction, &self.authority, &intent.outbound_id)?
        {
            if existing.intent != *intent {
                return Err(StoreError::ChannelOutboundConflict);
            }
            return Ok(ChannelOutboundStart {
                intent: existing.intent,
                disposition: if existing.delivery.is_some() {
                    ChannelOutboundDisposition::AlreadySent
                } else {
                    ChannelOutboundDisposition::RecoverOnly
                },
                provider_message_id: existing
                    .delivery
                    .map(|delivery| delivery.provider_message_id),
            });
        }
        let mission = load_mission_for_update(&transaction, &self.authority, &intent.mission_id)?
            .ok_or(StoreError::MissionNotFound)?;
        let route_set = load_channel_route_set(&transaction, &self.authority, &intent.mission_id)?
            .ok_or(StoreError::ChannelRouteConflict)?;
        let route = route_set
            .routes
            .iter()
            .find(|route| route.route_id == intent.route_id)
            .ok_or(StoreError::ChannelRouteConflict)?;
        let pairing = load_channel_pairing(&transaction, &self.authority, intent.channel)?
            .ok_or(StoreError::ChannelRouteConflict)?;
        if route_set.revision != intent.route_set_revision
            || route.channel != intent.channel
            || route.conversation_id != intent.conversation_id
            || route.owner_sender_id != intent.recipient_id
            || !route.allowed_outbound_classes.contains(&intent.kind)
            || pairing.conversation_id != intent.conversation_id
            || pairing.owner_sender_id != intent.recipient_id
            || pairing_provider_identity(&pairing) != route.provider_identity
        {
            return Err(StoreError::ChannelRouteConflict);
        }
        let proposal = ActionProposal {
            effect: EffectKind::ChannelSend,
            mission_id: mission.id.clone(),
            mission_scope_digest: mission.scope_digest.clone(),
            target: ActionTarget::Channel {
                channel: intent.channel,
                conversation_id: intent.conversation_id.clone(),
                recipient_ids: vec![intent.recipient_id.clone()],
            },
            estimated_cost_micros: None,
        };
        let decision = authorize_channel_outbound(
            &transaction,
            &self.authority,
            &mission,
            &proposal,
            intent,
            content,
        )?;
        if decision != GateDecision::Allowed {
            return Err(StoreError::ChannelAuthorization(decision));
        }
        let stored = StoredChannelOutbound {
            intent: intent.clone(),
            delivery: None,
        };
        write_channel_outbound(&transaction, &self.authority, &stored)?;
        transaction.commit()?;
        Ok(ChannelOutboundStart {
            intent: intent.clone(),
            disposition: ChannelOutboundDisposition::ExecuteNow,
            provider_message_id: None,
        })
    }

    /// Returns the immutable durable execution record for the same semantic
    /// outbound action, if one already exists. A UI retry may have a later
    /// observation time or cursor, but those transient values must never
    /// replace the approval time and recovery cursor that were consumed by
    /// the original Store transaction.
    ///
    /// # Errors
    ///
    /// Returns an error for Off, changed exact route/content authority, or an
    /// invalid durable binding. This method grants no new send authority.
    pub fn recover_channel_outbound(
        &self,
        proposed: &ChannelOutboundIntent,
        content: &[u8],
    ) -> Result<Option<ChannelOutboundStart>, StoreError> {
        validate_outbound(proposed)?;
        if format!("{:x}", Sha256::digest(content)) != proposed.content_sha256 {
            return Err(StoreError::ChannelOutboundConflict);
        }
        require_runtime_enabled(
            &self.connection,
            &self.authority,
            self.trusted_broker.as_ref(),
        )?;
        require_no_effect_fence(&self.connection)?;
        verified_audit_tail(&self.connection, &self.authority)?;
        verify_all_bindings(
            &self.connection,
            &self.authority,
            self.trusted_broker.as_ref(),
        )?;
        let Some(existing) =
            load_channel_outbound_for_recovery(&self.connection, &self.authority, proposed)?
        else {
            return Ok(None);
        };
        Ok(Some(ChannelOutboundStart {
            intent: existing.intent,
            disposition: if existing.delivery.is_some() {
                ChannelOutboundDisposition::AlreadySent
            } else {
                ChannelOutboundDisposition::RecoverOnly
            },
            provider_message_id: existing
                .delivery
                .map(|delivery| delivery.provider_message_id),
        }))
    }

    /// Reconciles the immutable provider result for a previously consumed
    /// outbound intent. This remains allowed after Off because it grants no
    /// new send authority.
    ///
    /// # Errors
    ///
    /// Returns an error for an unknown/conflicting result or invalid durable state.
    pub fn record_channel_delivery(
        &mut self,
        receipt: &ChannelDeliveryReceipt,
    ) -> Result<ChannelOutboundStart, StoreError> {
        validate_delivery(receipt)?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_no_effect_fence(&transaction)?;
        verified_audit_tail(&transaction, &self.authority)?;
        verify_all_bindings(&transaction, &self.authority, self.trusted_broker.as_ref())?;
        let mut stored =
            load_channel_outbound(&transaction, &self.authority, &receipt.outbound_id)?
                .ok_or(StoreError::ChannelOutboundConflict)?;
        if let Some(existing) = &stored.delivery {
            if existing == receipt {
                return Ok(ChannelOutboundStart {
                    intent: stored.intent,
                    disposition: ChannelOutboundDisposition::AlreadySent,
                    provider_message_id: Some(existing.provider_message_id.clone()),
                });
            }
            return Err(StoreError::ChannelOutboundConflict);
        }
        if receipt.delivered_at_ms < stored.intent.created_at_ms {
            return Err(StoreError::ChannelOutboundConflict);
        }
        stored.delivery = Some(receipt.clone());
        update_channel_outbound_delivery(&transaction, &self.authority, &stored)?;
        transaction.commit()?;
        Ok(ChannelOutboundStart {
            intent: stored.intent,
            disposition: ChannelOutboundDisposition::AlreadySent,
            provider_message_id: Some(receipt.provider_message_id.clone()),
        })
    }

    /// Verifies that one self-chat row is the exact durable echo of a
    /// previously recorded iMessage delivery. This read grants no send,
    /// model, Mission, or effect authority.
    ///
    /// # Errors
    ///
    /// Returns an error for malformed identity, ambiguous matches, or any
    /// invalid audit/encryption binding.
    pub fn verify_imessage_echo(
        &self,
        conversation_id: &str,
        provider_message_id: &str,
    ) -> Result<bool, StoreError> {
        if conversation_id.trim() != conversation_id
            || conversation_id.is_empty()
            || provider_message_id.trim() != provider_message_id
            || provider_message_id.is_empty()
        {
            return Err(StoreError::ChannelOutboundConflict);
        }
        verified_audit_tail(&self.connection, &self.authority)?;
        verify_all_bindings(
            &self.connection,
            &self.authority,
            self.trusted_broker.as_ref(),
        )?;
        let mut statement = self.connection.prepare(
            "SELECT outbound_id FROM channel_outbound
             WHERE channel_json = 'iMessage' AND conversation_id = ?1
               AND status_json = 'delivered' AND provider_message_id = ?2
             ORDER BY outbound_id LIMIT 2",
        )?;
        let outbound_ids = statement
            .query_map(params![conversation_id, provider_message_id], |row| {
                row.get::<_, String>(0)
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let [outbound_id] = outbound_ids.as_slice() else {
            return if outbound_ids.is_empty() {
                Ok(false)
            } else {
                Err(StoreError::ChannelOutboundConflict)
            };
        };
        let stored = load_channel_outbound(&self.connection, &self.authority, outbound_id)?
            .ok_or(StoreError::ChannelOutboundConflict)?;
        Ok(stored.intent.channel == ChannelKind::IMessage
            && stored.intent.conversation_id == conversation_id
            && stored
                .delivery
                .as_ref()
                .is_some_and(|delivery| delivery.provider_message_id == provider_message_id))
    }

    /// Reports whether any verified Mission has not reached a terminal state.
    ///
    /// This is a read-only scheduling gate. It grants no Mission, model, or
    /// effect authority and verifies every persisted binding before trusting
    /// the indexed status column.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid audit chain, binding, or Mission row.
    pub fn has_nonterminal_mission(&self) -> Result<bool, StoreError> {
        verified_audit_tail(&self.connection, &self.authority)?;
        verify_all_bindings(
            &self.connection,
            &self.authority,
            self.trusted_broker.as_ref(),
        )?;
        nonterminal_mission_exists(&self.connection)
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

fn authorize_channel_outbound(
    transaction: &Transaction<'_>,
    authority: &LocalAuthority,
    mission: &Mission,
    proposal: &ActionProposal,
    intent: &ChannelOutboundIntent,
    content: &[u8],
) -> Result<GateDecision, StoreError> {
    let decision = match intent.kind {
        ChannelMessageKind::Progress => ActionGate.authorize(mission, proposal, Some(content)),
        ChannelMessageKind::NeedYou => {
            let exact = mission.needs_me.as_ref().is_some_and(|needs_me| {
                mission.status == openopen_protocol::MissionStatus::NeedsMe
                    && intent.created_at_ms >= needs_me.created_at_ms
                    && channel_message_payload(intent.channel, &channel_need_you_content(needs_me))
                        == content
            });
            if exact {
                GateDecision::Allowed
            } else {
                GateDecision::Denied("Need-you send does not match current Mission boundary")
            }
        }
        ChannelMessageKind::Receipt => {
            let receipt = load_receipt_for_mission(transaction, authority, mission)?;
            let exact = receipt.as_ref().is_some_and(|receipt| {
                mission.status == openopen_protocol::MissionStatus::Completed
                    && intent.created_at_ms >= receipt.completed_at_ms
                    && channel_message_payload(intent.channel, &channel_receipt_content(receipt))
                        == content
            });
            if exact {
                let recipient = proposal
                    .approval_digest(ApprovalKind::NewRecipient, Some(content))
                    .ok()
                    .and_then(|digest| {
                        crate::gate::approved_owner_approval_id(
                            mission,
                            ApprovalKind::NewRecipient,
                            &digest,
                        )
                    });
                let disclosure = proposal
                    .approval_digest(ApprovalKind::NewDataShare, Some(content))
                    .ok()
                    .and_then(|digest| {
                        crate::gate::approved_owner_approval_id(
                            mission,
                            ApprovalKind::NewDataShare,
                            &digest,
                        )
                    });
                if recipient.is_some() && disclosure.is_some() {
                    GateDecision::Allowed
                } else {
                    GateDecision::NeedsMe(ApprovalKind::NewDataShare)
                }
            } else {
                GateDecision::Denied("Receipt send does not match completed Mission Receipt")
            }
        }
    };
    Ok(decision)
}

fn same_channel_outbound_recovery_identity(
    durable: &ChannelOutboundIntent,
    proposed: &ChannelOutboundIntent,
) -> bool {
    durable.mission_id == proposed.mission_id
        && durable.route_id == proposed.route_id
        && durable.channel == proposed.channel
        && durable.conversation_id == proposed.conversation_id
        && durable.recipient_id == proposed.recipient_id
        && durable.kind == proposed.kind
        && durable.content_sha256 == proposed.content_sha256
}

fn load_channel_outbound_for_recovery(
    connection: &Connection,
    authority: &LocalAuthority,
    proposed: &ChannelOutboundIntent,
) -> Result<Option<StoredChannelOutbound>, StoreError> {
    let encoded_channel = serde_json::to_string(&proposed.channel)
        .map_err(|_| StoreError::ChannelOutboundConflict)?;
    let outbound_ids = connection
        .prepare(
            "SELECT outbound_id FROM channel_outbound
             WHERE mission_id = ?1 AND channel_json = ?2
               AND conversation_id = ?3 AND content_sha256 = ?4
             ORDER BY outbound_id",
        )?
        .query_map(
            params![
                proposed.mission_id,
                encoded_channel,
                proposed.conversation_id,
                proposed.content_sha256,
            ],
            |row| row.get::<_, String>(0),
        )?
        .collect::<Result<Vec<_>, _>>()?;
    let mut matches = outbound_ids
        .into_iter()
        .map(|outbound_id| {
            load_channel_outbound(connection, authority, &outbound_id)
                .and_then(|stored| stored.ok_or(StoreError::ChannelOutboundConflict))
        })
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter(|stored| same_channel_outbound_recovery_identity(&stored.intent, proposed));
    let result = matches.next();
    if matches.next().is_some() {
        return Err(StoreError::ChannelOutboundConflict);
    }
    Ok(result)
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
    if let MissionCommand::RecordChannelParticipation { event, .. } = &envelope.command {
        if envelope.command_id != format!("{}:mission", event.event_id) {
            return Err(StoreError::ChannelRouteConflict);
        }
        let stored = load_channel_mission_event(
            transaction,
            authority,
            event.channel,
            &event.source_message_id,
        )?
        .ok_or(StoreError::ChannelRouteConflict)?;
        if stored != *event {
            return Err(StoreError::ChannelRouteConflict);
        }
    }
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

fn nonterminal_mission_exists(connection: &Connection) -> Result<bool, StoreError> {
    let completed = serde_json::to_string(&MissionStatus::Completed)
        .map_err(|error| CryptoError::Serialization(error.to_string()))?;
    let failed = serde_json::to_string(&MissionStatus::Failed)
        .map_err(|error| CryptoError::Serialization(error.to_string()))?;
    let cancelled = serde_json::to_string(&MissionStatus::Cancelled)
        .map_err(|error| CryptoError::Serialization(error.to_string()))?;
    connection
        .query_row(
            "SELECT EXISTS(
                 SELECT 1 FROM mission_state
                 WHERE status_json NOT IN (?1, ?2, ?3)
             )",
            params![completed, failed, cancelled],
            |row| row.get::<_, bool>(0),
        )
        .map_err(StoreError::from)
}

fn reject_mission_creation_during_started_channel_model(
    transaction: &Transaction<'_>,
    first: &MissionCommandEnvelope,
) -> Result<(), StoreError> {
    if !matches!(&first.command, MissionCommand::Create { .. }) {
        return Ok(());
    }
    let started = transaction.query_row(
        "SELECT EXISTS(
             SELECT 1 FROM channel_model_dispatch WHERE status_json = 'started'
         )",
        [],
        |row| row.get::<_, bool>(0),
    )?;
    if started {
        return Err(StoreError::MissionModelInFlight);
    }
    Ok(())
}

fn mission_command_batch_is_exact_retry(
    transaction: &Transaction<'_>,
    authority: &LocalAuthority,
    trusted_broker: Option<&TrustedBrokerEnrollment>,
    envelopes: &[MissionCommandEnvelope],
) -> Result<bool, StoreError> {
    for envelope in envelopes {
        let command_hash = command_hash(envelope)?;
        if load_duplicate_result(
            transaction,
            authority,
            trusted_broker,
            &envelope.command_id,
            &command_hash,
        )?
        .is_none()
        {
            return Ok(false);
        }
    }
    Ok(true)
}

fn record_channel_mission_participation(
    transaction: &Transaction<'_>,
    authority: &LocalAuthority,
    trusted_broker: Option<&TrustedBrokerEnrollment>,
    event: &ChannelMissionEvent,
) -> Result<MissionCommandResult, StoreError> {
    let expected_anchor = verified_audit_tail(transaction, authority)?;
    let command = MissionCommandEnvelope {
        command_id: format!("{}:mission", event.event_id),
        expected_anchor: expected_anchor.clone(),
        command: MissionCommand::RecordChannelParticipation {
            mission_id: event.mission_id.clone(),
            event: event.clone(),
        },
    };
    execute_mission_command_in_transaction(
        transaction,
        authority,
        trusted_broker,
        &command,
        expected_anchor.as_ref(),
    )
}

fn validate_command_id(command_id: &str) -> Result<(), StoreError> {
    if command_id.trim().is_empty() {
        Err(StoreError::InvalidCommandId)
    } else {
        Ok(())
    }
}

fn validate_mission_command_batch(envelopes: &[MissionCommandEnvelope]) -> Result<(), StoreError> {
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
    Ok(())
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

fn load_receipt_for_mission(
    connection: &Connection,
    authority: &LocalAuthority,
    mission: &Mission,
) -> Result<Option<Receipt>, StoreError> {
    let rows = {
        let mut statement = connection.prepare(
            "SELECT receipt_id, encrypted_blob, completed_at_ms
             FROM receipt_state WHERE mission_id = ?1",
        )?;
        statement
            .query_map([&mission.id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?
    };
    if rows.len() > 1 {
        return Err(StoreError::StateBindingMismatch(mission.id.clone()));
    }
    rows.into_iter()
        .next()
        .map(|(receipt_id, blob, completed_at_ms)| {
            verify_blob_binding(
                connection,
                RECEIPT_COMMIT_ACTION,
                &receipt_id,
                "receipt",
                &blob,
            )?;
            let receipt: Receipt =
                authority.decrypt_json(&blob, receipt_aad(&receipt_id, &mission.id).as_bytes())?;
            if receipt.id != receipt_id
                || receipt.mission_id != mission.id
                || receipt.completed_at_ms != completed_at_ms
            {
                return Err(StoreError::StateBindingMismatch(receipt_id));
            }
            validate_receipt(mission, &receipt, authority)?;
            Ok(receipt)
        })
        .transpose()
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
    verify_audited_state_entities_exist(connection, authority)?;
    verify_command_audit_reconciliation(connection, authority)?;
    verify_effect_authorization_bindings(connection, authority)?;
    verify_effect_receipt_bindings(connection, authority, trusted_broker)?;
    verify_effect_noncommit_bindings(connection, authority, trusted_broker)?;
    verify_effect_resolution_bindings(connection)?;
    verify_channel_bindings(connection, authority)?;
    let _ = load_choice_model_selection(connection, authority)?;
    let choice_snapshot = load_choice_loop_snapshot(connection, authority)?;
    verify_choice_private_bindings(connection, authority, choice_snapshot.as_ref())?;
    let schedule_ids = connection
        .prepare("SELECT schedule_id FROM choice_reminder_schedule")?
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    for schedule_id in schedule_ids {
        let _ = load_choice_reminder_schedule(connection, authority, &schedule_id)?;
    }
    let intent_ids = connection
        .prepare("SELECT intent_id FROM choice_markdown_render_intent")?
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    for intent_id in intent_ids {
        let _ = load_markdown_render_intent(connection, authority, &intent_id)?
            .ok_or(StoreError::ChoiceLoopStateConflict)?;
    }
    let receipt_ids = connection
        .prepare("SELECT intent_id FROM choice_markdown_render_receipt")?
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    for intent_id in receipt_ids {
        let _ = load_markdown_render_receipt(connection, authority, &intent_id)?
            .ok_or(StoreError::ChoiceLoopStateConflict)?;
    }
    let reply_ids = connection
        .prepare("SELECT reply_id FROM choice_imessage_reply ORDER BY reply_id")?
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    for reply_id in reply_ids {
        let _ = load_choice_imessage_reply_by_id(connection, authority, &reply_id)?
            .ok_or(StoreError::ChannelOutboundConflict)?;
    }
    verify_all_mission_and_receipt_bindings(connection, authority)
}

/// Verifies every live or retired private Choice body independently of the
/// broader effect/channel graph. Continuity reads use this bounded boundary so
/// a tampered worker row can never be presented as a healthy Choice session,
/// while Core can still open far enough to expose Off and typed recovery.
#[allow(clippy::too_many_lines)] // One bounded verifier keeps every live/retired private-row class in the same continuity gate.
fn verify_choice_private_bindings(
    connection: &Connection,
    authority: &LocalAuthority,
    snapshot: Option<&ChoiceLoopSnapshot>,
) -> Result<(), StoreError> {
    for action in [
        CHOICE_BEGIN_ACTION,
        CHOICE_D_INTAKE_ACTION,
        CHOICE_REFINEMENT_CONTEXT_ACTION,
        CHOICE_REFINEMENT_RESULT_ACTION,
        CHOICE_BODY_RETIREMENT_ACTION,
    ] {
        verify_audited_entities_exist(connection, action)?;
    }
    let mut live_rows = 0_usize;
    let begin_ids = connection
        .prepare("SELECT request_id FROM choice_begin_request")?
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    for request_id in begin_ids {
        let record = load_choice_begin_record(connection, authority, &request_id)?
            .ok_or(StoreError::ChoiceBeginConflict)?;
        let current = snapshot.ok_or(StoreError::ChoiceLoopStateConflict)?;
        if record.accepted.choice_session_id != current.session.id {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        live_rows += 1;
    }
    let d_ids = connection
        .prepare("SELECT request_id FROM choice_d_request")?
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    for request_id in d_ids {
        let record = load_choice_d_record(connection, authority, &request_id)?
            .ok_or(StoreError::ChoiceLoopStateConflict)?;
        let current = snapshot.ok_or(StoreError::ChoiceLoopStateConflict)?;
        if record.input.choice_session_id != current.session.id {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        live_rows += 1;
    }
    // These encrypted rows are private worker inputs/results rather than
    // snapshot fields.  They still remain live until their typed retirement,
    // so every transaction must reject a tampered row before it can advance
    // unrelated durable state.
    let refinement_context_ids = connection
        .prepare("SELECT operation_id FROM choice_refinement_context")?
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    for operation_id in refinement_context_ids {
        let context = load_choice_refinement_context(connection, authority, &operation_id)?
            .ok_or(StoreError::ChoiceLoopStateConflict)?;
        let current = snapshot.ok_or(StoreError::ChoiceLoopStateConflict)?;
        let pending = current
            .pending_refinement_operation
            .as_ref()
            .ok_or(StoreError::ChoiceLoopStateConflict)?;
        if current.session.state != ChoiceSessionState::Refining
            || context.choice_session_id != current.session.id
            || context.operation_id != pending.id
            || context.selection_id != pending.selection_id
            || context.expected_session_revision != pending.expected_session_revision
        {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        live_rows += 1;
    }
    let refinement_result_ids = connection
        .prepare("SELECT operation_id FROM choice_refinement_result")?
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    for operation_id in refinement_result_ids {
        let result = load_choice_refinement_result(connection, authority, &operation_id)?
            .ok_or(StoreError::ChoiceLoopStateConflict)?;
        let current = snapshot.ok_or(StoreError::ChoiceLoopStateConflict)?;
        if result.interpretation.choice_session_id != current.session.id {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        live_rows += 1;
    }
    if live_rows > 0
        && snapshot.is_some_and(|current| {
            matches!(
                current.session.state,
                ChoiceSessionState::Executing
                    | ChoiceSessionState::Cancelled
                    | ChoiceSessionState::Completed
            )
        })
    {
        let current = snapshot.ok_or(StoreError::ChoiceLoopStateConflict)?;
        let intent_ids = connection
            .prepare("SELECT intent_id FROM choice_markdown_render_intent")?
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        let mut resumable_receipted_cleanup = false;
        for intent_id in intent_ids {
            let stored = load_markdown_render_intent(connection, authority, &intent_id)?
                .ok_or(StoreError::ChoiceLoopStateConflict)?;
            if stored.intent.choice_session_id == current.session.id
                && stored.plaintext_body.is_some()
                && load_markdown_render_receipt(connection, authority, &intent_id)?.is_some()
            {
                resumable_receipted_cleanup = true;
            }
        }
        if !resumable_receipted_cleanup {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
    }
    verify_choice_private_body_retirements(connection, authority)
}

/// Verifies every command-owned Markdown journal row and the exact journal
/// required by an awaiting/executing foreground confirmation. This remains a
/// bounded continuity check: it reads no filesystem path and grants no effect.
#[allow(clippy::too_many_lines)] // One verifier keeps row/audit and foreground journal relationships in a single fail-closed boundary.
fn verify_choice_markdown_bindings(
    connection: &Connection,
    authority: &LocalAuthority,
    snapshot: Option<&ChoiceLoopSnapshot>,
) -> Result<(), StoreError> {
    verify_audited_entities_exist(connection, CHOICE_MARKDOWN_RENDER_INTENT_ACTION)?;
    verify_audited_entities_exist(connection, CHOICE_MARKDOWN_RENDER_RECEIPT_ACTION)?;

    let intent_ids = connection
        .prepare("SELECT intent_id FROM choice_markdown_render_intent ORDER BY intent_id")?
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    let mut journals = Vec::with_capacity(intent_ids.len());
    for intent_id in intent_ids {
        let stored = load_markdown_render_intent(connection, authority, &intent_id)?
            .ok_or(StoreError::ChoiceLoopStateConflict)?;
        let receipt = load_markdown_render_receipt(connection, authority, &intent_id)?;
        if receipt.as_ref().is_some_and(|receipt| {
            receipt.final_entry != stored.intent.entry
                || receipt.displaced_base != stored.intent.expected_base
                || receipt.committed_at_ms < stored.intent.created_at_ms
        }) {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        journals.push((stored, receipt));
    }

    // An orphan receipt cannot be loaded through the intent-bound AAD join.
    // Check the receipt table independently so a direct row delete never hides
    // a previously audited proof from foreground continuity.
    let receipt_ids = connection
        .prepare("SELECT intent_id FROM choice_markdown_render_receipt ORDER BY intent_id")?
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    for intent_id in receipt_ids {
        load_markdown_render_receipt(connection, authority, &intent_id)?
            .ok_or(StoreError::ChoiceLoopStateConflict)?;
    }

    let Some(current) = snapshot else {
        return Ok(());
    };
    if !matches!(
        current.session.state,
        ChoiceSessionState::AwaitingConfirmation | ChoiceSessionState::Executing
    ) {
        return Ok(());
    }
    let confirmation = current
        .confirmation
        .as_ref()
        .ok_or(StoreError::ChoiceLoopStateConflict)?;
    let matching = journals
        .iter()
        .filter(|(stored, _)| markdown_intent_matches_confirmation(&stored.intent, confirmation))
        .collect::<Vec<_>>();
    if matching.len() != 1 {
        return Err(StoreError::ChoiceLoopStateConflict);
    }
    let (stored, receipt) = matching[0];
    match current.session.state {
        ChoiceSessionState::AwaitingConfirmation => {
            if current.session.revision != stored.intent.expected_session_revision
                || stored.plaintext_body.is_none()
                || stored.reconciliation.is_some()
            {
                return Err(StoreError::ChoiceLoopStateConflict);
            }
        }
        ChoiceSessionState::Executing => {
            if current.session.revision
                != stored
                    .intent
                    .expected_session_revision
                    .checked_add(1)
                    .ok_or(StoreError::ChoiceLoopStateConflict)?
                || stored.plaintext_body.is_some()
                || stored.reconciliation.is_some()
                || receipt.is_none()
            {
                return Err(StoreError::ChoiceLoopStateConflict);
            }
        }
        _ => unreachable!("foreground journal state was filtered above"),
    }
    Ok(())
}

fn require_current_markdown_render_authority(
    connection: &Connection,
    authority: &LocalAuthority,
    trusted_broker: Option<&TrustedBrokerEnrollment>,
    intent: &MarkdownRenderIntent,
) -> Result<(), StoreError> {
    let runtime = load_runtime_control(connection, authority, trusted_broker)?;
    let current = load_choice_loop_snapshot(connection, authority)?
        .ok_or(StoreError::ChoiceLoopStateConflict)?;
    let confirmation = current
        .confirmation
        .as_ref()
        .ok_or(StoreError::ChoiceLoopStateConflict)?;
    if !runtime.enabled
        || runtime.revision != intent.expected_generation
        || current.session.id != intent.choice_session_id
        || current.session.revision != intent.expected_session_revision
        || current.session.state != ChoiceSessionState::AwaitingConfirmation
        || current.session.pending_confirmation_id.as_deref() != Some(&confirmation.id)
        || !markdown_intent_matches_confirmation(intent, confirmation)
    {
        return Err(StoreError::ChoiceLoopStateConflict);
    }
    Ok(())
}

fn markdown_intent_matches_confirmation(
    intent: &MarkdownRenderIntent,
    confirmation: &ChoiceConsolidatedConfirmation,
) -> bool {
    confirmation.choice_session_id == intent.choice_session_id
        && confirmation.markdown_entry == intent.entry
        && confirmation.markdown_expected_base == intent.expected_base
        && markdown_render_intent_id(
            confirmation,
            intent.expected_session_revision,
            intent.expected_generation,
            &intent.content_digest,
        ) == intent.id
}

fn require_current_markdown_cleanup_authority(
    connection: &Connection,
    authority: &LocalAuthority,
    trusted_broker: Option<&TrustedBrokerEnrollment>,
    intent: &MarkdownRenderIntent,
    receipt: &MarkdownRenderReceipt,
) -> Result<(), StoreError> {
    // Cleanup is deliberately allowed after protected Off, but it must still
    // verify the signed runtime row and every receipt/session identity before
    // deleting the retained Owner base.
    let runtime = load_runtime_control(connection, authority, trusted_broker)?;
    let current = load_choice_loop_snapshot(connection, authority)?
        .ok_or(StoreError::ChoiceLoopStateConflict)?;
    let confirmation_matches = current
        .confirmation
        .as_ref()
        .is_some_and(|confirmation| markdown_intent_matches_confirmation(intent, confirmation));
    let pre_retirement = current.session.id == intent.choice_session_id
        && current.session.revision == intent.expected_session_revision
        && current.session.state == ChoiceSessionState::AwaitingConfirmation
        && current.confirmation.is_some()
        && current.session.pending_confirmation_id.is_some();
    let exact_replay = current.session.id == intent.choice_session_id
        && current.session.revision
            == intent
                .expected_session_revision
                .checked_add(1)
                .ok_or(StoreError::ChoiceLoopStateConflict)?
        && current.session.state == ChoiceSessionState::Executing;
    // Global Off can cancel the session after this exact receipt has become
    // durable but before the retained base is removed. That successor may
    // finish only receipt-verified deletion; it never re-enters publication.
    let cancelled_after_off = current.session.id == intent.choice_session_id
        && current.session.revision
            == intent
                .expected_session_revision
                .checked_add(1)
                .ok_or(StoreError::ChoiceLoopStateConflict)?
        && current.session.state == ChoiceSessionState::Cancelled
        && !runtime.enabled;
    if (!pre_retirement && !exact_replay && !cancelled_after_off)
        || (current.confirmation.is_some() && !confirmation_matches)
        || receipt.intent_id != intent.id
        || receipt.final_entry != intent.entry
        || receipt.displaced_base != intent.expected_base
        || receipt.committed_at_ms < intent.created_at_ms
    {
        return Err(StoreError::ChoiceLoopStateConflict);
    }
    Ok(())
}

fn verify_audited_state_entities_exist(
    connection: &Connection,
    authority: &LocalAuthority,
) -> Result<(), StoreError> {
    verify_audited_entities_exist(connection, MISSION_COMMAND_ACTION)?;
    verify_audited_entities_exist(connection, RECEIPT_COMMIT_ACTION)?;
    verify_audited_entities_exist(connection, EFFECT_AUTHORIZATION_ACTION)?;
    verify_audited_entities_exist(connection, EFFECT_RECEIPT_ACTION)?;
    verify_audited_entities_exist(connection, EFFECT_NONCOMMIT_ACTION)?;
    verify_audited_entities_exist(connection, CHANNEL_PAIRING_ACTION)?;
    verify_audited_entities_exist(connection, CHANNEL_OBSERVATION_ACTION)?;
    verify_audited_entities_exist(connection, CHANNEL_CURSOR_ACTION)?;
    verify_audited_entities_exist(connection, CHANNEL_MODEL_QUEUED_ACTION)?;
    verify_audited_entities_exist(connection, CHANNEL_MODEL_ACTION)?;
    verify_audited_entities_exist(connection, CHANNEL_MODEL_FAILED_ACTION)?;
    verify_audited_entities_exist(connection, CHANNEL_SUGGESTION_ACTION)?;
    verify_audited_entities_exist(connection, CHANNEL_FAILURE_INCIDENT_ACTION)?;
    verify_audited_entities_exist(connection, CHANNEL_FAILURE_ACK_ACTION)?;
    verify_audited_entities_exist(connection, CHOICE_MODEL_SELECTION_ACTION)?;
    verify_audited_entities_exist(connection, CHOICE_BEGIN_ACTION)?;
    verify_audited_entities_exist(connection, CHOICE_D_INTAKE_ACTION)?;
    verify_audited_entities_exist(connection, CHOICE_REFINEMENT_CONTEXT_ACTION)?;
    verify_audited_entities_exist(connection, CHOICE_REFINEMENT_RESULT_ACTION)?;
    verify_audited_entities_exist(connection, CHOICE_BODY_RETIREMENT_ACTION)?;
    verify_audited_entities_exist(connection, CHOICE_MARKDOWN_RENDER_INTENT_ACTION)?;
    verify_audited_entities_exist(connection, CHOICE_MARKDOWN_RENDER_RECEIPT_ACTION)?;
    verify_audited_entities_exist(connection, CHOICE_REMINDER_SCHEDULE_ACTION)?;
    verify_audited_entities_exist(connection, CHOICE_LOOP_STATE_ACTION)?;
    verify_audited_entities_exist(connection, CHOICE_IDLE_CLOCK_ACTION)?;
    if connection
        .query_row(
            "SELECT 1 FROM choice_idle_clock_anchor WHERE singleton_id = 1",
            [],
            |_| Ok(()),
        )
        .optional()?
        .is_some()
    {
        load_choice_idle_clock_anchor(connection, authority)?
            .ok_or(StoreError::ChoiceClockUncertain)?;
    }
    verify_audited_entities_exist(connection, CHANNEL_ROUTE_SET_ACTION)?;
    verify_audited_entities_exist(connection, CHANNEL_MISSION_EVENT_ACTION)?;
    if connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'channel_mission_origin_legacy')",
        [],
        |row| row.get::<_, bool>(0),
    )? {
        verify_audited_entities_exist(connection, LEGACY_CHANNEL_ORIGIN_ACTION)?;
    }
    verify_audited_entities_exist(connection, CHANNEL_OUTBOUND_ACTION)?;
    verify_audited_entities_exist(connection, CHANNEL_DELIVERY_ACTION)?;
    Ok(())
}

fn verify_all_mission_and_receipt_bindings(
    connection: &Connection,
    authority: &LocalAuthority,
) -> Result<(), StoreError> {
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

fn verify_channel_bindings(
    connection: &Connection,
    authority: &LocalAuthority,
) -> Result<(), StoreError> {
    let pairing_channels = connection
        .prepare("SELECT channel_json FROM channel_pairing")?
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    for encoded in pairing_channels {
        let channel: ChannelKind = serde_json::from_str(&encoded)
            .map_err(|_| StoreError::ChannelStateMismatch(encoded.clone()))?;
        load_channel_pairing(connection, authority, channel)?
            .ok_or_else(|| StoreError::ChannelStateMismatch(encoded))?;
    }

    let observations = connection
        .prepare("SELECT channel_json, source_message_id FROM channel_observation")?
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for (encoded, source_message_id) in observations {
        let channel: ChannelKind = serde_json::from_str(&encoded)
            .map_err(|_| StoreError::ChannelStateMismatch(encoded.clone()))?;
        load_channel_observation(connection, authority, channel, &source_message_id)?
            .ok_or_else(|| StoreError::ChannelStateMismatch(source_message_id))?;
    }

    let cursors = connection
        .prepare("SELECT channel_json, conversation_id FROM channel_cursor")?
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for (encoded, conversation_id) in cursors {
        let channel: ChannelKind = serde_json::from_str(&encoded)
            .map_err(|_| StoreError::ChannelStateMismatch(encoded.clone()))?;
        load_channel_cursor(connection, authority, channel, &conversation_id)?
            .ok_or_else(|| StoreError::ChannelStateMismatch(conversation_id))?;
    }

    let model_dispatches = connection
        .prepare("SELECT channel_json, source_message_id FROM channel_model_dispatch")?
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for (encoded, source_message_id) in model_dispatches {
        let channel: ChannelKind = serde_json::from_str(&encoded)
            .map_err(|_| StoreError::ChannelStateMismatch(encoded.clone()))?;
        load_channel_model_dispatch(connection, authority, channel, &source_message_id)?
            .ok_or_else(|| StoreError::ChannelStateMismatch(source_message_id))?;
    }

    let failure_incidents = connection
        .prepare("SELECT incident_id FROM channel_failure_incident")?
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    for incident_id in failure_incidents {
        load_channel_failure_incident(connection, authority, &incident_id)?
            .ok_or_else(|| StoreError::ChannelFailureIncidentConflict(incident_id))?;
    }

    let route_sets = connection
        .prepare("SELECT mission_id FROM channel_route_set")?
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    for mission_id in route_sets {
        load_channel_route_set(connection, authority, &mission_id)?
            .ok_or_else(|| StoreError::ChannelStateMismatch(mission_id))?;
    }

    let mission_events = connection
        .prepare("SELECT channel_json, source_message_id FROM channel_mission_event")?
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for (encoded, source_message_id) in mission_events {
        let channel: ChannelKind = serde_json::from_str(&encoded)
            .map_err(|_| StoreError::ChannelStateMismatch(encoded.clone()))?;
        load_channel_mission_event(connection, authority, channel, &source_message_id)?
            .ok_or_else(|| StoreError::ChannelStateMismatch(source_message_id))?;
    }

    let outbounds = connection
        .prepare("SELECT outbound_id FROM channel_outbound")?
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    for outbound_id in outbounds {
        load_channel_outbound(connection, authority, &outbound_id)?
            .ok_or_else(|| StoreError::ChannelStateMismatch(outbound_id))?;
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
        CHANNEL_PAIRING_ACTION => "SELECT 1 FROM channel_pairing WHERE channel_json = ?1",
        CHANNEL_OBSERVATION_ACTION => "SELECT 1 FROM channel_observation WHERE entity_id = ?1",
        CHANNEL_CURSOR_ACTION => "SELECT 1 FROM channel_cursor WHERE entity_id = ?1",
        CHANNEL_MODEL_QUEUED_ACTION
        | CHANNEL_MODEL_ACTION
        | CHANNEL_MODEL_FAILED_ACTION
        | CHANNEL_SUGGESTION_ACTION => "SELECT 1 FROM channel_model_dispatch WHERE entity_id = ?1",
        CHANNEL_FAILURE_INCIDENT_ACTION | CHANNEL_FAILURE_ACK_ACTION => {
            "SELECT 1 FROM channel_failure_incident WHERE incident_id = ?1"
        }
        CHOICE_MODEL_SELECTION_ACTION => {
            "SELECT 1 FROM choice_model_selection
             WHERE singleton_id = 1 AND ?1 = 'choice-model-selection'"
        }
        CHOICE_BEGIN_ACTION => {
            "SELECT 1 WHERE EXISTS(SELECT 1 FROM choice_begin_request WHERE request_id = ?1)
             OR EXISTS(SELECT 1 FROM choice_private_body_retirement
                       WHERE source_kind = 'begin' AND entity_id = ?1)"
        }
        CHOICE_D_INTAKE_ACTION => {
            "SELECT 1 WHERE EXISTS(SELECT 1 FROM choice_d_request WHERE request_id = ?1)
             OR EXISTS(SELECT 1 FROM choice_private_body_retirement
                       WHERE source_kind = 'd' AND entity_id = ?1)"
        }
        CHOICE_REFINEMENT_CONTEXT_ACTION => {
            "SELECT 1 WHERE EXISTS(SELECT 1 FROM choice_refinement_context
                                   WHERE operation_id = ?1)
             OR EXISTS(SELECT 1 FROM choice_private_body_retirement
                       WHERE source_kind = 'refinement'
                         AND entity_id = 'context:' || ?1)"
        }
        CHOICE_REFINEMENT_RESULT_ACTION => {
            "SELECT 1 WHERE EXISTS(SELECT 1 FROM choice_refinement_result WHERE operation_id = ?1)
             OR EXISTS(SELECT 1 FROM choice_private_body_retirement
                       WHERE source_kind = 'refinement' AND entity_id = ?1)"
        }
        CHOICE_BODY_RETIREMENT_ACTION => {
            "SELECT 1 FROM choice_private_body_retirement
             WHERE source_kind || ':' || entity_id = ?1"
        }
        CHOICE_MARKDOWN_RENDER_INTENT_ACTION => {
            "SELECT 1 FROM choice_markdown_render_intent WHERE intent_id = ?1"
        }
        CHOICE_MARKDOWN_RENDER_RECEIPT_ACTION => {
            "SELECT 1 FROM choice_markdown_render_receipt WHERE intent_id = ?1"
        }
        CHOICE_REMINDER_SCHEDULE_ACTION => {
            "SELECT 1 FROM choice_reminder_schedule WHERE schedule_id = ?1"
        }
        CHOICE_LOOP_STATE_ACTION => {
            "SELECT 1 FROM choice_loop_state
             WHERE singleton_id = 1 AND ?1 = 'choice-loop'"
        }
        CHOICE_IDLE_CLOCK_ACTION => {
            "SELECT 1 FROM choice_idle_clock_anchor
             WHERE singleton_id = 1 AND ?1 = 'choice-idle-clock'"
        }
        CHANNEL_ROUTE_SET_ACTION => "SELECT 1 FROM channel_route_set WHERE mission_id = ?1",
        CHANNEL_MISSION_EVENT_ACTION => "SELECT 1 FROM channel_mission_event WHERE entity_id = ?1",
        LEGACY_CHANNEL_ORIGIN_ACTION => {
            "SELECT 1 FROM channel_mission_origin_legacy WHERE mission_id = ?1"
        }
        CHANNEL_OUTBOUND_ACTION | CHANNEL_DELIVERY_ACTION => {
            "SELECT 1 FROM channel_outbound WHERE outbound_id = ?1"
        }
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

fn audit_state_hash_for(
    connection: &Connection,
    action: &str,
    entity_id: &str,
) -> Result<String, StoreError> {
    connection
        .query_row(
            "SELECT state_hash FROM audit_ledger
             WHERE action = ?1 AND entity_id = ?2
             ORDER BY sequence DESC LIMIT 1",
            params![action, entity_id],
            |row| row.get(0),
        )
        .optional()?
        .filter(|hash: &String| is_sha256_hex(hash))
        .ok_or(StoreError::ChoiceLoopStateConflict)
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

fn load_choice_model_selection(
    connection: &Connection,
    authority: &LocalAuthority,
) -> Result<Option<ModelSelection>, StoreError> {
    let row: Option<(Vec<u8>, String)> = connection
        .query_row(
            "SELECT encrypted_blob, blob_hash FROM choice_model_selection WHERE singleton_id = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    row.map(|(blob, stored_hash)| {
        if stored_hash != blob_hash(&blob) {
            return Err(StoreError::ChoiceModelSelectionConflict);
        }
        verify_blob_binding(
            connection,
            CHOICE_MODEL_SELECTION_ACTION,
            "choice-model-selection",
            "choice:model_selection",
            &blob,
        )?;
        let selection: ModelSelection =
            authority.decrypt_json(&blob, choice_model_selection_aad().as_bytes())?;
        if !selection.is_valid() {
            return Err(StoreError::ChoiceModelSelectionConflict);
        }
        Ok(selection)
    })
    .transpose()
}

fn choice_begin_matches_snapshot(
    record: &ChoiceBeginRecord,
    snapshot: &ChoiceLoopSnapshot,
) -> bool {
    snapshot.session.id == record.accepted.choice_session_id
        && snapshot.session.state == ChoiceSessionState::Interpreting
        && snapshot.session.revision == record.accepted.accepted_session_revision
        && snapshot.session.model_selection_state
            == ModelSelectionState::Selected {
                model_provenance_ref: record.model_selection.id.clone(),
            }
        && snapshot.session.primary_delivery_binding_id.as_deref()
            == Some(&record.source_envelope.delivery_binding_id)
        && snapshot.session.opened_at_ms == record.accepted_at_ms
        && snapshot.session.last_input_at_ms == record.accepted_at_ms
        && snapshot.active_batch.as_ref() == Some(&record.batch)
        && snapshot.interpretation.is_none()
        && snapshot.active_choice_set.is_none()
        && snapshot.last_selection.is_none()
        && snapshot.confirmation.is_none()
        && snapshot.document_manifest == record.source_manifest
}

type ChoicePrivateRequestRow = (String, String, String, String, String, Vec<u8>, String, i64);
type ChoiceRefinementContextRow = (String, String, String, String, Vec<u8>, String, i64);
type ChoiceRefinementResultRow = (String, String, String, String, String, Vec<u8>, String, i64);

fn load_choice_begin_record(
    connection: &Connection,
    authority: &LocalAuthority,
    request_id: &str,
) -> Result<Option<ChoiceBeginRecord>, StoreError> {
    let row: Option<ChoicePrivateRequestRow> = connection
        .query_row(
            "SELECT request_digest, choice_session_id, operation_id, source_envelope_id,
                    conversation_turn_batch_id, encrypted_blob, blob_hash, accepted_at_ms
             FROM choice_begin_request
             WHERE request_id = ?1",
            params![request_id],
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
                ))
            },
        )
        .optional()?;
    row.map(
        |(
            request_digest,
            choice_session_id,
            operation_id,
            source_envelope_id,
            conversation_turn_batch_id,
            blob,
            stored_hash,
            accepted_at_ms,
        )| {
            if stored_hash != blob_hash(&blob) {
                return Err(StoreError::ChoiceBeginConflict);
            }
            verify_blob_binding(
                connection,
                CHOICE_BEGIN_ACTION,
                request_id,
                "choice:begin",
                &blob,
            )?;
            let record: ChoiceBeginRecord = authority.decrypt_json(
                &blob,
                choice_begin_request_aad(
                    request_id,
                    &choice_session_id,
                    &operation_id,
                    &source_envelope_id,
                    &conversation_turn_batch_id,
                    accepted_at_ms,
                )
                .as_bytes(),
            )?;
            if !record.is_valid()
                || record.accepted.request_id != request_id
                || record.accepted.choice_session_id != choice_session_id
                || record.accepted.operation_id != operation_id
                || record.accepted.source_envelope_id != source_envelope_id
                || record.accepted.conversation_turn_batch_id != conversation_turn_batch_id
                || record.source_envelope.id != source_envelope_id
                || record.batch.id != conversation_turn_batch_id
                || record.request_digest != request_digest
                || record.accepted_at_ms != accepted_at_ms
            {
                return Err(StoreError::ChoiceBeginConflict);
            }
            Ok(record)
        },
    )
    .transpose()
}

fn load_choice_begin_record_by_operation(
    connection: &Connection,
    authority: &LocalAuthority,
    operation_id: &str,
) -> Result<Option<ChoiceBeginRecord>, StoreError> {
    let request_id: Option<String> = connection
        .query_row(
            "SELECT request_id FROM choice_begin_request WHERE operation_id = ?1",
            [operation_id],
            |row| row.get(0),
        )
        .optional()?;
    request_id
        .map(|request_id| load_choice_begin_record(connection, authority, &request_id))
        .transpose()
        .map(Option::flatten)
}

/// Returns the sole encrypted initial-intake record for a session. The
/// session column is unique, so this is a lookup of an already authenticated
/// Host record rather than a caller-controlled source selection.
fn load_choice_begin_record_by_session(
    connection: &Connection,
    authority: &LocalAuthority,
    choice_session_id: &str,
) -> Result<Option<ChoiceBeginRecord>, StoreError> {
    let request_id: Option<String> = connection
        .query_row(
            "SELECT request_id FROM choice_begin_request WHERE choice_session_id = ?1",
            [choice_session_id],
            |row| row.get(0),
        )
        .optional()?;
    request_id
        .map(|request_id| load_choice_begin_record(connection, authority, &request_id))
        .transpose()
        .map(Option::flatten)
}

fn choice_imessage_reply_aad(
    reply_id: &str,
    source_message_id: &str,
    choice_session_id: &str,
    choice_set_id: &str,
    preview_revision: u64,
    confirmation_digest: &str,
) -> String {
    let identity = serde_json::to_vec(&json!({
        "replyId": reply_id,
        "sourceMessageId": source_message_id,
        "choiceSessionId": choice_session_id,
        "choiceSetId": choice_set_id,
        "previewRevision": preview_revision,
        "confirmationDigest": confirmation_digest,
    }))
    .expect("bounded reply identity serializes");
    format!("openopen-choice-imessage-reply-v2:{}", blob_hash(&identity))
}

fn load_choice_imessage_reply_by_id(
    connection: &Connection,
    authority: &LocalAuthority,
    reply_id: &str,
) -> Result<Option<(ChoiceIMessageReplyIntent, String, Option<String>)>, StoreError> {
    let mut statement = connection.prepare(
        "SELECT reply_id, source_message_id, choice_session_id, choice_set_id,
                preview_revision, confirmation_digest, status_json,
                encrypted_blob, blob_hash, provider_message_id
         FROM choice_imessage_reply WHERE reply_id = ?1",
    )?;
    let row = statement
        .query_map([reply_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, Vec<u8>>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, Option<String>>(9)?,
            ))
        })?
        .next()
        .transpose()?;
    row.map(
        |(
            indexed_reply_id,
            source_message_id,
            choice_session_id,
            choice_set_id,
            preview_revision,
            confirmation_digest,
            status,
            blob,
            stored_hash,
            provider,
        )| {
            let preview_revision =
                u64::try_from(preview_revision).map_err(|_| StoreError::ChannelOutboundConflict)?;
            if stored_hash != blob_hash(&blob) {
                return Err(StoreError::ChannelOutboundConflict);
            }
            if status != "prepared" && status != "authorized" && status != "delivered" {
                return Err(StoreError::ChannelOutboundConflict);
            }
            let action = if status == "prepared" {
                CHOICE_IMESSAGE_REPLY_PREPARED_ACTION
            } else {
                CHOICE_IMESSAGE_REPLY_AUTHORIZED_ACTION
            };
            verify_blob_binding(connection, action, reply_id, "choice:imessage_reply", &blob)?;
            let intent: ChoiceIMessageReplyIntent = authority.decrypt_json(
                &blob,
                choice_imessage_reply_aad(
                    &indexed_reply_id,
                    &source_message_id,
                    &choice_session_id,
                    &choice_set_id,
                    preview_revision,
                    &confirmation_digest,
                )
                .as_bytes(),
            )?;
            if !intent.is_valid()
                || indexed_reply_id != reply_id
                || intent.preview.reply_id != reply_id
                || intent.source_message_id != source_message_id
                || intent.choice_session_id != choice_session_id
                || intent.choice_set_id != choice_set_id
                || intent.preview.preview_revision != preview_revision
                || intent.preview.confirmation_digest != confirmation_digest
                || (status == "prepared" && intent.approved_at_ms.is_some())
                || ((status == "authorized" || status == "delivered")
                    && intent.approved_at_ms.is_none())
                || (status == "delivered") != provider.is_some()
            {
                return Err(StoreError::ChannelOutboundConflict);
            }
            if let Some(provider_message_id) = provider.as_deref() {
                let audit_binding: Option<(String, String)> = connection
                    .query_row(
                        "SELECT state_kind, state_hash FROM audit_ledger
                     WHERE action = ?1 AND entity_id = ?2
                     ORDER BY sequence DESC LIMIT 1",
                        params![CHOICE_IMESSAGE_REPLY_DELIVERED_ACTION, reply_id],
                        |row| Ok((row.get(0)?, row.get(1)?)),
                    )
                    .optional()?;
                if audit_binding
                    != Some((
                        "choice:imessage_reply".to_owned(),
                        blob_hash(provider_message_id.as_bytes()),
                    ))
                {
                    return Err(StoreError::ChannelOutboundConflict);
                }
            }
            Ok((intent, status, provider))
        },
    )
    .transpose()
}

fn load_choice_imessage_reply_by_source(
    connection: &Connection,
    authority: &LocalAuthority,
    source_message_id: &str,
) -> Result<Option<(ChoiceIMessageReplyIntent, String, Option<String>)>, StoreError> {
    let reply_id: Option<String> = connection
        .query_row(
            "SELECT reply_id FROM choice_imessage_reply WHERE source_message_id = ?1",
            [source_message_id],
            |row| row.get(0),
        )
        .optional()?;
    reply_id
        .map(|reply_id| load_choice_imessage_reply_by_id(connection, authority, &reply_id))
        .transpose()
        .map(Option::flatten)
}

fn validate_choice_imessage_reply_current(
    connection: &Connection,
    authority: &LocalAuthority,
    intent: &ChoiceIMessageReplyIntent,
) -> Result<(), StoreError> {
    let snapshot = load_choice_loop_snapshot(connection, authority)?
        .ok_or(StoreError::ChannelOutboundConflict)?;
    let choice_set = snapshot
        .active_choice_set
        .as_ref()
        .ok_or(StoreError::ChannelOutboundConflict)?;
    let begin =
        load_choice_begin_record_by_session(connection, authority, &intent.choice_session_id)?
            .ok_or(StoreError::ChannelOutboundConflict)?;
    let pairing = load_channel_pairing(connection, authority, ChannelKind::IMessage)?
        .ok_or(StoreError::ChannelOutboundConflict)?;
    if snapshot.session.state != ChoiceSessionState::Active
        || snapshot.session.id != intent.choice_session_id
        || snapshot.session.revision != intent.session_revision
        || snapshot.session.active_choice_set_id.as_deref() != Some(intent.choice_set_id.as_str())
        || choice_set.id != intent.choice_set_id
        || canonical_choice_set_digest(choice_set).as_deref()
            != Some(intent.choice_set_digest.as_str())
        || choice_set.persona_revision != intent.persona_revision
        || choice_set.source_manifest_digest != intent.source_manifest_digest
        || choice_set.model_provenance != intent.model_provenance
        || snapshot.document_manifest.aggregate_digest != intent.source_manifest_digest
        || pairing != intent.pairing
        || begin.source_envelope.surface != "imessage-self-chat"
        || begin.source_envelope.provider_message_id.as_deref()
            != Some(intent.source_message_id.as_str())
        || begin.source_envelope.delivery_binding_id != intent.delivery_binding_id
        || snapshot.session.primary_delivery_binding_id.as_deref()
            != Some(intent.delivery_binding_id.as_str())
    {
        return Err(StoreError::ChannelOutboundConflict);
    }
    Ok(())
}

fn load_choice_d_record(
    connection: &Connection,
    authority: &LocalAuthority,
    request_id: &str,
) -> Result<Option<ChoiceDIntakeRecord>, StoreError> {
    let row: Option<ChoicePrivateRequestRow> = connection
        .query_row(
            "SELECT request_digest, operation_id, choice_session_id, source_envelope_id,
                    conversation_turn_batch_id, encrypted_blob, blob_hash, accepted_at_ms
             FROM choice_d_request WHERE request_id = ?1",
            [request_id],
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
                ))
            },
        )
        .optional()?;
    row.map(
        |(
            request_digest,
            operation_id,
            choice_session_id,
            source_envelope_id,
            conversation_turn_batch_id,
            blob,
            stored_hash,
            accepted_at_ms,
        )| {
            if stored_hash != blob_hash(&blob) {
                return Err(StoreError::ChoiceLoopStateConflict);
            }
            verify_blob_binding(
                connection,
                CHOICE_D_INTAKE_ACTION,
                request_id,
                "choice:d_intake",
                &blob,
            )?;
            let record: ChoiceDIntakeRecord = authority.decrypt_json(
                &blob,
                choice_d_request_aad(
                    request_id,
                    &choice_session_id,
                    &operation_id,
                    &source_envelope_id,
                    &conversation_turn_batch_id,
                    accepted_at_ms,
                )
                .as_bytes(),
            )?;
            if !record.is_valid()
                || record.input.request_id != request_id
                || record.input.choice_session_id != choice_session_id
                || record.source_envelope.id != source_envelope_id
                || record.batch.id != conversation_turn_batch_id
                || record.selection.id
                    != operation_id.strip_prefix("refinement-").unwrap_or_default()
                || record.request_digest != request_digest
                || record.input.submitted_at_ms != accepted_at_ms
            {
                return Err(StoreError::ChoiceLoopStateConflict);
            }
            Ok(record)
        },
    )
    .transpose()
}

fn load_choice_refinement_context(
    connection: &Connection,
    authority: &LocalAuthority,
    operation_id: &str,
) -> Result<Option<ChoiceRefinementContext>, StoreError> {
    let row: Option<ChoiceRefinementContextRow> = connection
        .query_row(
            "SELECT selection_id, choice_session_id, source_envelope_id, conversation_turn_batch_id,
                    encrypted_blob, blob_hash, created_at_ms
             FROM choice_refinement_context WHERE operation_id = ?1",
            [operation_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                ))
            },
        )
        .optional()?;
    row.map(
        |(
            selection_id,
            choice_session_id,
            source_envelope_id,
            conversation_turn_batch_id,
            blob,
            stored_hash,
            created_at_ms,
        )| {
            if stored_hash != blob_hash(&blob) {
                return Err(StoreError::ChoiceLoopStateConflict);
            }
            verify_blob_binding(
                connection,
                CHOICE_REFINEMENT_CONTEXT_ACTION,
                operation_id,
                "choice:refinement_context",
                &blob,
            )?;
            let context: ChoiceRefinementContext = authority.decrypt_json(
                &blob,
                choice_refinement_context_aad(
                    operation_id,
                    &selection_id,
                    &choice_session_id,
                    &source_envelope_id,
                    &conversation_turn_batch_id,
                    created_at_ms,
                )
                .as_bytes(),
            )?;
            if !context.is_valid()
                || context.operation_id != operation_id
                || context.selection_id != selection_id
                || context.choice_session_id != choice_session_id
                || context.source_envelope_id != source_envelope_id
                || context.conversation_turn_batch_id != conversation_turn_batch_id
            {
                return Err(StoreError::ChoiceLoopStateConflict);
            }
            Ok(context)
        },
    )
    .transpose()
}

fn choice_private_body_tombstone_exists(
    connection: &Connection,
    authority: &LocalAuthority,
    source_kind: &str,
    request_id: &str,
) -> Result<bool, StoreError> {
    load_choice_private_body_retirement(connection, authority, source_kind, request_id)
        .map(|record| record.is_some())
}

fn load_choice_private_body_retirement(
    connection: &Connection,
    authority: &LocalAuthority,
    source_kind: &str,
    entity_id: &str,
) -> Result<Option<ChoicePrivateBodyRetirement>, StoreError> {
    let row: Option<(Vec<u8>, String)> = connection
        .query_row(
            "SELECT encrypted_blob, blob_hash FROM choice_private_body_retirement
             WHERE source_kind = ?1 AND entity_id = ?2",
            params![source_kind, entity_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    row.map(|(blob, stored_hash)| {
        if stored_hash != blob_hash(&blob) {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        verify_blob_binding(
            connection,
            CHOICE_BODY_RETIREMENT_ACTION,
            &format!("{source_kind}:{entity_id}"),
            "choice:private_body_retirement",
            &blob,
        )?;
        let record: ChoicePrivateBodyRetirement = authority.decrypt_json(
            &blob,
            choice_private_body_retirement_aad(source_kind, entity_id).as_bytes(),
        )?;
        if !record.is_valid() || record.source_kind != source_kind || record.entity_id != entity_id
        {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        if record.legacy_blocked {
            let marker =
                legacy_private_body_marker_digest_for_row(connection, source_kind, entity_id)?
                    .ok_or(StoreError::ChoiceLoopStateConflict)?;
            if record.request_digest.is_some()
                || record.choice_session_id != "legacy-retirement-blocked"
                || record.body_digest != marker
                || record.source_blob_hash != marker
            {
                return Err(StoreError::ChoiceLoopStateConflict);
            }
        } else {
            let (action, source_entity_id) = match source_kind {
                "begin" => (CHOICE_BEGIN_ACTION, entity_id),
                "d" => (CHOICE_D_INTAKE_ACTION, entity_id),
                "refinement" if entity_id.starts_with("context:") => (
                    CHOICE_REFINEMENT_CONTEXT_ACTION,
                    entity_id
                        .strip_prefix("context:")
                        .ok_or(StoreError::ChoiceLoopStateConflict)?,
                ),
                "refinement" => (CHOICE_REFINEMENT_RESULT_ACTION, entity_id),
                _ => return Err(StoreError::ChoiceLoopStateConflict),
            };
            if audit_state_hash_for(connection, action, source_entity_id)?
                != record.source_blob_hash
            {
                return Err(StoreError::ChoiceLoopStateConflict);
            }
        }
        Ok(record)
    })
    .transpose()
}

fn legacy_private_body_marker_digest(value: &serde_json::Value) -> String {
    // `serde_json::Map` is ordered, and every migration call supplies a
    // fixed literal key sequence. This is a body-free compatibility marker,
    // not a reconstruction or a new attestation of legacy metadata.
    sha256_hex(value.to_string().as_bytes())
}

fn retirement_audit_exists(
    transaction: &Transaction<'_>,
    source_kind: &str,
    entity_id: &str,
) -> Result<bool, StoreError> {
    transaction
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM audit_ledger
             WHERE action = ?1 AND entity_id = ?2)",
            params![
                CHOICE_BODY_RETIREMENT_ACTION,
                format!("{source_kind}:{entity_id}")
            ],
            |row| row.get(0),
        )
        .map_err(Into::into)
}

fn legacy_private_body_marker_digest_for_row(
    connection: &Connection,
    source_kind: &str,
    entity_id: &str,
) -> Result<Option<String>, StoreError> {
    match source_kind {
        "begin" | "d" => connection
            .query_row(
                "SELECT source_kind, request_id, request_digest, body_digest,
                        choice_session_id, retired_at_ms
                 FROM choice_private_body_tombstone
                 WHERE source_kind = ?1 AND request_id = ?2",
                params![source_kind, entity_id],
                |row| {
                    Ok(legacy_private_body_marker_digest(&json!({
                        "sourceKind": row.get::<_, String>(0)?,
                        "entityId": row.get::<_, String>(1)?,
                        "requestDigest": row.get::<_, String>(2)?,
                        "bodyDigest": row.get::<_, String>(3)?,
                        "choiceSessionId": row.get::<_, String>(4)?,
                        "retiredAtMs": row.get::<_, i64>(5)?,
                    })))
                },
            )
            .optional()
            .map_err(Into::into),
        "refinement" => connection
            .query_row(
                "SELECT operation_id, result_digest, choice_session_id, retired_at_ms
                 FROM choice_refinement_body_tombstone WHERE operation_id = ?1",
                [entity_id],
                |row| {
                    Ok(legacy_private_body_marker_digest(&json!({
                        "sourceKind": "refinement",
                        "entityId": row.get::<_, String>(0)?,
                        "bodyDigest": row.get::<_, String>(1)?,
                        "choiceSessionId": row.get::<_, String>(2)?,
                        "retiredAtMs": row.get::<_, i64>(3)?,
                    })))
                },
            )
            .optional()
            .map_err(Into::into),
        _ => Ok(None),
    }
}

fn verify_choice_private_body_retirements(
    connection: &Connection,
    authority: &LocalAuthority,
) -> Result<(), StoreError> {
    let identifiers = connection
        .prepare("SELECT source_kind, entity_id FROM choice_private_body_retirement")?
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for (source_kind, entity_id) in identifiers {
        let _ =
            load_choice_private_body_retirement(connection, authority, &source_kind, &entity_id)?
                .ok_or(StoreError::ChoiceLoopStateConflict)?;
    }
    Ok(())
}

#[allow(clippy::too_many_lines)] // One transaction keeps both intake kinds verified before retirement.
fn purge_choice_private_bodies(
    transaction: &Transaction<'_>,
    authority: &LocalAuthority,
    choice_session_id: &str,
    retired_at_ms: i64,
) -> Result<(), StoreError> {
    let begins = transaction
        .prepare(
            "SELECT request_id, request_digest, operation_id, source_envelope_id,
                    conversation_turn_batch_id, encrypted_blob, blob_hash, accepted_at_ms
             FROM choice_begin_request WHERE choice_session_id = ?1",
        )?
        .query_map([choice_session_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, Vec<u8>>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, i64>(7)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for (
        request_id,
        request_digest,
        operation_id,
        source_envelope_id,
        conversation_turn_batch_id,
        blob,
        stored_hash,
        accepted_at_ms,
    ) in begins
    {
        if blob_hash(&blob) != stored_hash {
            return Err(StoreError::ChoiceBeginConflict);
        }
        let record: ChoiceBeginRecord = authority.decrypt_json(
            &blob,
            choice_begin_request_aad(
                &request_id,
                choice_session_id,
                &operation_id,
                &source_envelope_id,
                &conversation_turn_batch_id,
                accepted_at_ms,
            )
            .as_bytes(),
        )?;
        if !record.is_valid()
            || record.accepted.choice_session_id != choice_session_id
            || record.accepted.operation_id != operation_id
            || record.source_envelope.id != source_envelope_id
            || record.batch.id != conversation_turn_batch_id
        {
            return Err(StoreError::ChoiceBeginConflict);
        }
        insert_choice_private_body_tombstone(
            transaction,
            authority,
            &ChoicePrivateBodyTombstoneArgs {
                source_kind: "begin",
                request_id: &request_id,
                request_digest: &request_digest,
                body_digest: &record.source_envelope.body_digest,
                choice_session_id,
                source_blob_hash: &stored_hash,
                retired_at_ms,
            },
        )?;
        transaction.execute(
            "DELETE FROM choice_begin_request WHERE request_id = ?1",
            [&request_id],
        )?;
    }

    let d_records = transaction
        .prepare(
            "SELECT request_id, request_digest, operation_id, choice_session_id,
                    source_envelope_id, conversation_turn_batch_id, encrypted_blob, blob_hash,
                    accepted_at_ms
             FROM choice_d_request",
        )?
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, Vec<u8>>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, i64>(8)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for (
        request_id,
        request_digest,
        operation_id,
        stored_choice_session_id,
        source_envelope_id,
        conversation_turn_batch_id,
        blob,
        stored_hash,
        accepted_at_ms,
    ) in d_records
    {
        if blob_hash(&blob) != stored_hash {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        let record: ChoiceDIntakeRecord = authority.decrypt_json(
            &blob,
            choice_d_request_aad(
                &request_id,
                &stored_choice_session_id,
                &operation_id,
                &source_envelope_id,
                &conversation_turn_batch_id,
                accepted_at_ms,
            )
            .as_bytes(),
        )?;
        if !record.is_valid()
            || record.input.choice_session_id != stored_choice_session_id
            || record.source_envelope.id != source_envelope_id
            || record.batch.id != conversation_turn_batch_id
        {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        if record.input.choice_session_id != choice_session_id {
            continue;
        }
        insert_choice_private_body_tombstone(
            transaction,
            authority,
            &ChoicePrivateBodyTombstoneArgs {
                source_kind: "d",
                request_id: &request_id,
                request_digest: &request_digest,
                body_digest: &sha256_hex(record.input.bounded_text.as_bytes()),
                choice_session_id,
                source_blob_hash: &stored_hash,
                retired_at_ms,
            },
        )?;
        transaction.execute(
            "DELETE FROM choice_d_request WHERE request_id = ?1",
            [&request_id],
        )?;
    }
    Ok(())
}

fn retire_choice_refinement_results(
    transaction: &Transaction<'_>,
    authority: &LocalAuthority,
    choice_session_id: &str,
    retired_at_ms: i64,
) -> Result<(), StoreError> {
    let rows = transaction
        .prepare(
            "SELECT operation_id, selection_id, choice_session_id, source_envelope_id,
                    conversation_turn_batch_id, result_digest, encrypted_blob, blob_hash, completed_at_ms
             FROM choice_refinement_result",
        )?
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, Vec<u8>>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, i64>(8)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for (
        operation_id,
        selection_id,
        stored_choice_session_id,
        source_envelope_id,
        conversation_turn_batch_id,
        result_digest,
        blob,
        stored_hash,
        completed_at_ms,
    ) in rows
    {
        if blob_hash(&blob) != stored_hash {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        let result: ChoiceRefinementResult = authority.decrypt_json(
            &blob,
            choice_refinement_result_aad(
                &operation_id,
                &selection_id,
                &stored_choice_session_id,
                &source_envelope_id,
                &conversation_turn_batch_id,
                &result_digest,
                completed_at_ms,
            )
            .as_bytes(),
        )?;
        if !result.is_valid()
            || result.operation_id != operation_id
            || result.selection_id != selection_id
            || result.interpretation.choice_session_id != stored_choice_session_id
            || result.source_envelope_id != source_envelope_id
            || result.conversation_turn_batch_id != conversation_turn_batch_id
            || result.result_digest != result_digest
        {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        if result.interpretation.choice_session_id != choice_session_id {
            continue;
        }
        transaction.execute(
            "INSERT INTO choice_refinement_body_tombstone
             (operation_id, result_digest, choice_session_id, retired_at_ms)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                operation_id,
                result_digest,
                choice_session_id,
                retired_at_ms
            ],
        )?;
        persist_choice_private_body_retirement(
            transaction,
            authority,
            &ChoicePrivateBodyRetirement {
                source_kind: "refinement".to_owned(),
                entity_id: operation_id.clone(),
                request_digest: None,
                body_digest: result_digest.clone(),
                choice_session_id: choice_session_id.to_owned(),
                source_blob_hash: stored_hash,
                retired_at_ms,
                legacy_blocked: false,
            },
        )?;
        transaction.execute(
            "DELETE FROM choice_refinement_result WHERE operation_id = ?1",
            [&operation_id],
        )?;
    }
    Ok(())
}

/// Retires the encrypted semantic worker input once it can no longer be
/// consumed by a pending refinement. The body-free retirement reuses the
/// refinement namespace with a disjoint `context:` identity so the original
/// context audit remains verifiable without retaining the derived frame or
/// selected option.
fn retire_choice_refinement_contexts(
    transaction: &Transaction<'_>,
    authority: &LocalAuthority,
    choice_session_id: &str,
    retired_at_ms: i64,
) -> Result<(), StoreError> {
    let rows = transaction
        .prepare(
            "SELECT operation_id, selection_id, choice_session_id, source_envelope_id,
                    conversation_turn_batch_id, encrypted_blob, blob_hash, created_at_ms
             FROM choice_refinement_context",
        )?
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, Vec<u8>>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, i64>(7)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for (
        operation_id,
        selection_id,
        stored_session_id,
        source_envelope_id,
        conversation_turn_batch_id,
        blob,
        stored_hash,
        created_at_ms,
    ) in rows
    {
        if blob_hash(&blob) != stored_hash {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        let context: ChoiceRefinementContext = authority.decrypt_json(
            &blob,
            choice_refinement_context_aad(
                &operation_id,
                &selection_id,
                &stored_session_id,
                &source_envelope_id,
                &conversation_turn_batch_id,
                created_at_ms,
            )
            .as_bytes(),
        )?;
        if !context.is_valid()
            || context.operation_id != operation_id
            || context.selection_id != selection_id
            || context.choice_session_id != stored_session_id
            || context.source_envelope_id != source_envelope_id
            || context.conversation_turn_batch_id != conversation_turn_batch_id
        {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        if stored_session_id != choice_session_id {
            continue;
        }
        let context_digest = sha256_hex(
            &serde_json::to_vec(&context)
                .map_err(|error| CryptoError::Serialization(error.to_string()))?,
        );
        persist_choice_private_body_retirement(
            transaction,
            authority,
            &ChoicePrivateBodyRetirement {
                source_kind: "refinement".to_owned(),
                entity_id: format!("context:{operation_id}"),
                request_digest: None,
                body_digest: context_digest,
                choice_session_id: choice_session_id.to_owned(),
                source_blob_hash: stored_hash,
                retired_at_ms,
                legacy_blocked: false,
            },
        )?;
        transaction.execute(
            "DELETE FROM choice_refinement_context WHERE operation_id = ?1",
            [&operation_id],
        )?;
    }
    Ok(())
}

/// Retires a cancelled session's Markdown body without claiming that a render
/// receipt exists.  The intent metadata remains auditable so restart recovery
/// can report the cancelled journal, but no body is retained after cancellation.
fn retire_choice_markdown_intents_on_cancel(
    transaction: &Transaction<'_>,
    authority: &LocalAuthority,
    choice_session_id: &str,
    retired_at_ms: i64,
) -> Result<(), StoreError> {
    let intent_ids = transaction
        .prepare("SELECT intent_id FROM choice_markdown_render_intent")?
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    for intent_id in intent_ids {
        let stored = load_markdown_render_intent(transaction, authority, &intent_id)?
            .ok_or(StoreError::ChoiceLoopStateConflict)?;
        if stored.intent.choice_session_id != choice_session_id {
            continue;
        }
        if stored.plaintext_body.is_some() {
            // The retained encrypted intent metadata and its body-retired
            // audit record are the body-free tombstone for a cancelled
            // journal. Unlike begin/D, a render intent must remain present
            // so restart can explain reconciliation without recreating text.
            persist_retired_markdown_render_intent(
                transaction,
                authority,
                &stored.intent,
                retired_at_ms,
            )?;
        }
    }
    Ok(())
}

fn insert_choice_private_body_tombstone(
    transaction: &Transaction<'_>,
    authority: &LocalAuthority,
    args: &ChoicePrivateBodyTombstoneArgs<'_>,
) -> Result<(), StoreError> {
    transaction.execute(
        "INSERT INTO choice_private_body_tombstone
         (source_kind, request_id, request_digest, body_digest, choice_session_id, retired_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            args.source_kind,
            args.request_id,
            args.request_digest,
            args.body_digest,
            args.choice_session_id,
            args.retired_at_ms
        ],
    )?;
    persist_choice_private_body_retirement(
        transaction,
        authority,
        &ChoicePrivateBodyRetirement {
            source_kind: args.source_kind.to_owned(),
            entity_id: args.request_id.to_owned(),
            request_digest: Some(args.request_digest.to_owned()),
            body_digest: args.body_digest.to_owned(),
            choice_session_id: args.choice_session_id.to_owned(),
            source_blob_hash: args.source_blob_hash.to_owned(),
            retired_at_ms: args.retired_at_ms,
            legacy_blocked: false,
        },
    )?;
    Ok(())
}

fn persist_choice_private_body_retirement(
    transaction: &Transaction<'_>,
    authority: &LocalAuthority,
    record: &ChoicePrivateBodyRetirement,
) -> Result<(), StoreError> {
    if !record.is_valid() {
        return Err(StoreError::ChoiceLoopStateConflict);
    }
    let blob = authority.encrypt_json(
        &record,
        choice_private_body_retirement_aad(&record.source_kind, &record.entity_id).as_bytes(),
    )?;
    let state_hash = blob_hash(&blob);
    transaction.execute(
        "INSERT INTO choice_private_body_retirement
         (source_kind, entity_id, encrypted_blob, blob_hash, retired_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            &record.source_kind,
            &record.entity_id,
            blob,
            &state_hash,
            record.retired_at_ms,
        ],
    )?;
    append_audit(
        transaction,
        authority,
        &AuditRecord {
            id: &format!(
                "choice-body-retired-{}-{}",
                record.source_kind, record.entity_id
            ),
            command_id: &record.entity_id,
            command_hash: &record.body_digest,
            actor: "openopen-host",
            action: CHOICE_BODY_RETIREMENT_ACTION,
            entity_id: &format!("{}:{}", record.source_kind, record.entity_id),
            created_at_ms: record.retired_at_ms,
            state_kind: "choice:private_body_retirement",
            state_hash: &state_hash,
        },
    )?;
    Ok(())
}

fn model_provenance_matches_selection(
    provenance: &openopen_protocol::ModelProvenance,
    selection: &ModelSelection,
) -> bool {
    provenance.model_id == selection.model_id
        && provenance.requested_effort == selection.requested_effort
        && provenance.actual_effort == selection.actual_effort
        && provenance.catalog_fingerprint == selection.catalog_fingerprint
        && provenance.catalog_revision == selection.catalog_revision
        && provenance.account_display_class == selection.account_display_class
        && provenance.protocol_schema_revision == selection.protocol_schema_revision
}

fn choice_selection_id(selection: &Selection) -> &str {
    match selection {
        Selection::OptionSelection(value) => &value.id,
        Selection::NaturalConversationSelection(value) => &value.id,
    }
}

fn persist_choice_begin_record(
    transaction: &Transaction<'_>,
    authority: &LocalAuthority,
    record: &ChoiceBeginRecord,
) -> Result<(), StoreError> {
    let request_id = &record.accepted.request_id;
    let blob = authority.encrypt_json(
        record,
        choice_begin_request_aad(
            request_id,
            &record.accepted.choice_session_id,
            &record.accepted.operation_id,
            &record.accepted.source_envelope_id,
            &record.accepted.conversation_turn_batch_id,
            record.accepted_at_ms,
        )
        .as_bytes(),
    )?;
    let encrypted_blob_hash = blob_hash(&blob);
    transaction.execute(
        "INSERT INTO choice_begin_request (
            request_id, request_digest, choice_session_id, operation_id, source_envelope_id,
            conversation_turn_batch_id, encrypted_blob, blob_hash, accepted_at_ms
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            request_id,
            &record.request_digest,
            &record.accepted.choice_session_id,
            &record.accepted.operation_id,
            &record.accepted.source_envelope_id,
            &record.accepted.conversation_turn_batch_id,
            blob,
            encrypted_blob_hash,
            record.accepted_at_ms,
        ],
    )?;
    append_audit(
        transaction,
        authority,
        &AuditRecord {
            id: &format!("choice-begin-{}", record.accepted.operation_id),
            command_id: request_id,
            command_hash: &record.request_digest,
            actor: "owner",
            action: CHOICE_BEGIN_ACTION,
            entity_id: request_id,
            created_at_ms: record.accepted_at_ms,
            state_kind: "choice:begin",
            state_hash: &encrypted_blob_hash,
        },
    )?;
    Ok(())
}

fn persist_choice_d_record(
    transaction: &Transaction<'_>,
    authority: &LocalAuthority,
    record: &ChoiceDIntakeRecord,
    operation: &ChoiceRefinementOperation,
) -> Result<(), StoreError> {
    let blob = authority.encrypt_json(
        record,
        choice_d_request_aad(
            &record.input.request_id,
            &record.input.choice_session_id,
            &operation.id,
            &record.source_envelope.id,
            &record.batch.id,
            record.input.submitted_at_ms,
        )
        .as_bytes(),
    )?;
    let encrypted_blob_hash = blob_hash(&blob);
    transaction.execute(
        "INSERT INTO choice_d_request (
            request_id, request_digest, operation_id, choice_session_id, source_envelope_id,
            conversation_turn_batch_id, encrypted_blob, blob_hash, accepted_at_ms
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            &record.input.request_id,
            &record.request_digest,
            &operation.id,
            &record.input.choice_session_id,
            &record.source_envelope.id,
            &record.batch.id,
            blob,
            encrypted_blob_hash,
            record.input.submitted_at_ms,
        ],
    )?;
    append_audit(
        transaction,
        authority,
        &AuditRecord {
            id: &format!("choice-d-intake-{}", record.input.request_id),
            command_id: &record.input.request_id,
            command_hash: &record.request_digest,
            actor: "owner",
            action: CHOICE_D_INTAKE_ACTION,
            entity_id: &record.input.request_id,
            created_at_ms: record.input.submitted_at_ms,
            state_kind: "choice:d_intake",
            state_hash: &encrypted_blob_hash,
        },
    )?;
    Ok(())
}

fn persist_choice_refinement_context(
    transaction: &Transaction<'_>,
    authority: &LocalAuthority,
    context: &ChoiceRefinementContext,
    created_at_ms: i64,
) -> Result<(), StoreError> {
    if !context.is_valid() || created_at_ms < 0 {
        return Err(StoreError::ChoiceLoopStateConflict);
    }
    let blob = authority.encrypt_json(
        context,
        choice_refinement_context_aad(
            &context.operation_id,
            &context.selection_id,
            &context.choice_session_id,
            &context.source_envelope_id,
            &context.conversation_turn_batch_id,
            created_at_ms,
        )
        .as_bytes(),
    )?;
    let hash = blob_hash(&blob);
    transaction.execute(
        "INSERT INTO choice_refinement_context
         (operation_id, selection_id, choice_session_id, source_envelope_id,
          conversation_turn_batch_id, encrypted_blob, blob_hash, created_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            &context.operation_id,
            &context.selection_id,
            &context.choice_session_id,
            &context.source_envelope_id,
            &context.conversation_turn_batch_id,
            blob,
            hash,
            created_at_ms,
        ],
    )?;
    append_audit(
        transaction,
        authority,
        &AuditRecord {
            id: &format!("choice-refinement-context-{}", context.operation_id),
            command_id: &context.operation_id,
            command_hash: &format!(
                "{:x}",
                Sha256::digest(
                    serde_json::to_vec(context)
                        .map_err(|error| CryptoError::Serialization(error.to_string()))?
                )
            ),
            actor: "host",
            action: CHOICE_REFINEMENT_CONTEXT_ACTION,
            entity_id: &context.operation_id,
            created_at_ms,
            state_kind: "choice:refinement_context",
            state_hash: &hash,
        },
    )?;
    Ok(())
}

fn load_choice_refinement_result(
    connection: &Connection,
    authority: &LocalAuthority,
    operation_id: &str,
) -> Result<Option<ChoiceRefinementResult>, StoreError> {
    let row: Option<ChoiceRefinementResultRow> = connection
        .query_row(
            "SELECT selection_id, choice_session_id, source_envelope_id,
                    conversation_turn_batch_id, result_digest, encrypted_blob, blob_hash,
                    completed_at_ms
             FROM choice_refinement_result WHERE operation_id = ?1",
            [operation_id],
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
                ))
            },
        )
        .optional()?;
    row.map(
        |(
            selection_id,
            choice_session_id,
            source_envelope_id,
            conversation_turn_batch_id,
            result_digest,
            blob,
            stored_hash,
            completed_at_ms,
        )| {
            if stored_hash != blob_hash(&blob) {
                return Err(StoreError::ChoiceLoopStateConflict);
            }
            verify_blob_binding(
                connection,
                CHOICE_REFINEMENT_RESULT_ACTION,
                operation_id,
                "choice:refinement_result",
                &blob,
            )?;
            let result: ChoiceRefinementResult = authority.decrypt_json(
                &blob,
                choice_refinement_result_aad(
                    operation_id,
                    &selection_id,
                    &choice_session_id,
                    &source_envelope_id,
                    &conversation_turn_batch_id,
                    &result_digest,
                    completed_at_ms,
                )
                .as_bytes(),
            )?;
            if !result.is_valid()
                || result.operation_id != operation_id
                || result.selection_id != selection_id
                || result.interpretation.choice_session_id != choice_session_id
                || result.source_envelope_id != source_envelope_id
                || result.conversation_turn_batch_id != conversation_turn_batch_id
                || result.result_digest != result_digest
                || result.completed_at_ms != completed_at_ms
            {
                return Err(StoreError::ChoiceLoopStateConflict);
            }
            Ok(result)
        },
    )
    .transpose()
}

fn persist_choice_refinement_result(
    transaction: &Transaction<'_>,
    authority: &LocalAuthority,
    result: &ChoiceRefinementResult,
) -> Result<(), StoreError> {
    let blob = authority.encrypt_json(
        result,
        choice_refinement_result_aad(
            &result.operation_id,
            &result.selection_id,
            &result.interpretation.choice_session_id,
            &result.source_envelope_id,
            &result.conversation_turn_batch_id,
            &result.result_digest,
            result.completed_at_ms,
        )
        .as_bytes(),
    )?;
    let encrypted_blob_hash = blob_hash(&blob);
    transaction.execute(
        "INSERT INTO choice_refinement_result
         (operation_id, selection_id, choice_session_id, source_envelope_id,
          conversation_turn_batch_id, result_digest, encrypted_blob, blob_hash, completed_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            &result.operation_id,
            &result.selection_id,
            &result.interpretation.choice_session_id,
            &result.source_envelope_id,
            &result.conversation_turn_batch_id,
            &result.result_digest,
            &blob,
            &encrypted_blob_hash,
            result.completed_at_ms,
        ],
    )?;
    append_audit(
        transaction,
        authority,
        &AuditRecord {
            id: &format!("choice-refinement-result-{}", result.operation_id),
            command_id: &result.operation_id,
            command_hash: &result.result_digest,
            actor: "host",
            action: CHOICE_REFINEMENT_RESULT_ACTION,
            entity_id: &result.operation_id,
            created_at_ms: result.completed_at_ms,
            state_kind: "choice:refinement_result",
            state_hash: &encrypted_blob_hash,
        },
    )?;
    Ok(())
}

fn load_markdown_render_intent(
    connection: &Connection,
    authority: &LocalAuthority,
    intent_id: &str,
) -> Result<Option<StoredMarkdownRenderIntent>, StoreError> {
    let row: Option<(String, Vec<u8>, String)> = connection
        .query_row(
            "SELECT intent_digest, encrypted_blob, blob_hash
             FROM choice_markdown_render_intent WHERE intent_id = ?1",
            [intent_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    row.map(|(intent_digest, blob, stored_hash)| {
        if stored_hash != blob_hash(&blob) {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        verify_blob_binding(
            connection,
            CHOICE_MARKDOWN_RENDER_INTENT_ACTION,
            intent_id,
            "choice:markdown_render_intent",
            &blob,
        )?;
        let record: StoredMarkdownRenderIntent = authority.decrypt_json(
            &blob,
            markdown_render_intent_aad(intent_id, &intent_digest).as_bytes(),
        )?;
        let computed_digest = markdown_render_intent_digest(&record.intent)?;
        let body_is_valid = record.plaintext_body.as_ref().is_none_or(|body| {
            body.len() as u64 == record.intent.entry.byte_length
                && sha256_hex(body.as_bytes()) == record.intent.content_digest
        });
        if !record.intent.is_valid()
            || record.intent.id != intent_id
            || !body_is_valid
            || record
                .reconciliation
                .as_ref()
                .is_some_and(|value| !value.is_valid())
            || computed_digest != intent_digest
        {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        Ok(record)
    })
    .transpose()
}

fn persist_markdown_render_intent(
    transaction: &Transaction<'_>,
    authority: &LocalAuthority,
    record: &StoredMarkdownRenderIntent,
) -> Result<(), StoreError> {
    let intent_digest = markdown_render_intent_digest(&record.intent)?;
    let blob = authority.encrypt_json(
        record,
        markdown_render_intent_aad(&record.intent.id, &intent_digest).as_bytes(),
    )?;
    let state_hash = blob_hash(&blob);
    transaction.execute(
        "INSERT INTO choice_markdown_render_intent
         (intent_id, intent_digest, encrypted_blob, blob_hash, created_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            &record.intent.id,
            intent_digest,
            blob,
            state_hash,
            record.intent.created_at_ms,
        ],
    )?;
    append_audit(
        transaction,
        authority,
        &AuditRecord {
            id: &format!("{}:intent", record.intent.id),
            command_id: &record.intent.id,
            command_hash: &markdown_render_intent_digest(&record.intent)?,
            actor: "openopen-host",
            action: CHOICE_MARKDOWN_RENDER_INTENT_ACTION,
            entity_id: &record.intent.id,
            created_at_ms: record.intent.created_at_ms,
            state_kind: "choice:markdown_render_intent",
            state_hash: &state_hash,
        },
    )?;
    Ok(())
}

fn update_markdown_render_intent(
    transaction: &Transaction<'_>,
    authority: &LocalAuthority,
    record: &StoredMarkdownRenderIntent,
    audit_suffix: &str,
    updated_at_ms: i64,
) -> Result<(), StoreError> {
    let intent_digest = markdown_render_intent_digest(&record.intent)?;
    let blob = authority.encrypt_json(
        record,
        markdown_render_intent_aad(&record.intent.id, &intent_digest).as_bytes(),
    )?;
    let state_hash = blob_hash(&blob);
    if transaction.execute(
        "UPDATE choice_markdown_render_intent
         SET encrypted_blob = ?2, blob_hash = ?3
         WHERE intent_id = ?1 AND intent_digest = ?4",
        params![&record.intent.id, blob, state_hash, intent_digest],
    )? != 1
    {
        return Err(StoreError::ChoiceLoopStateConflict);
    }
    append_audit(
        transaction,
        authority,
        &AuditRecord {
            id: &format!("{}:{audit_suffix}", record.intent.id),
            command_id: &format!("{}:{audit_suffix}", record.intent.id),
            command_hash: &markdown_render_intent_digest(&record.intent)?,
            actor: "openopen-host",
            action: CHOICE_MARKDOWN_RENDER_INTENT_ACTION,
            entity_id: &record.intent.id,
            created_at_ms: updated_at_ms,
            state_kind: "choice:markdown_render_intent",
            state_hash: &state_hash,
        },
    )?;
    Ok(())
}

fn persist_retired_markdown_render_intent(
    transaction: &Transaction<'_>,
    authority: &LocalAuthority,
    intent: &MarkdownRenderIntent,
    retired_at_ms: i64,
) -> Result<(), StoreError> {
    let intent_digest = markdown_render_intent_digest(intent)?;
    let retired = StoredMarkdownRenderIntent {
        intent: intent.clone(),
        plaintext_body: None,
        reconciliation: None,
    };
    let blob = authority.encrypt_json(
        &retired,
        markdown_render_intent_aad(&intent.id, &intent_digest).as_bytes(),
    )?;
    let state_hash = blob_hash(&blob);
    transaction.execute(
        "UPDATE choice_markdown_render_intent
         SET encrypted_blob = ?2, blob_hash = ?3
         WHERE intent_id = ?1 AND intent_digest = ?4",
        params![&intent.id, blob, state_hash, intent_digest],
    )?;
    append_audit(
        transaction,
        authority,
        &AuditRecord {
            id: &format!("{}:body-retired", intent.id),
            command_id: &format!("{}:body-retired", intent.id),
            command_hash: &markdown_render_intent_digest(intent)?,
            actor: "openopen-host",
            action: CHOICE_MARKDOWN_RENDER_INTENT_ACTION,
            entity_id: &intent.id,
            created_at_ms: retired_at_ms,
            state_kind: "choice:markdown_render_intent",
            state_hash: &state_hash,
        },
    )?;
    Ok(())
}

fn retire_reconciled_markdown_intents_for_session(
    transaction: &Transaction<'_>,
    authority: &LocalAuthority,
    choice_session_id: &str,
    retired_at_ms: i64,
) -> Result<(), StoreError> {
    let intent_ids = transaction
        .prepare("SELECT intent_id FROM choice_markdown_render_intent")?
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    for intent_id in intent_ids {
        let stored = load_markdown_render_intent(transaction, authority, &intent_id)?
            .ok_or(StoreError::ChoiceLoopStateConflict)?;
        if stored.intent.choice_session_id != choice_session_id || stored.plaintext_body.is_none() {
            continue;
        }
        if stored.reconciliation.is_none()
            || load_markdown_render_receipt(transaction, authority, &intent_id)?.is_some()
        {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        persist_retired_markdown_render_intent(
            transaction,
            authority,
            &stored.intent,
            retired_at_ms,
        )?;
    }
    Ok(())
}

fn load_markdown_render_receipt(
    connection: &Connection,
    authority: &LocalAuthority,
    intent_id: &str,
) -> Result<Option<MarkdownRenderReceipt>, StoreError> {
    let row: Option<(String, Vec<u8>, String)> = connection
        .query_row(
            "SELECT choice_markdown_render_intent.intent_digest,
                    choice_markdown_render_receipt.encrypted_blob,
                    choice_markdown_render_receipt.blob_hash
             FROM choice_markdown_render_intent
             JOIN choice_markdown_render_receipt USING(intent_id)
             WHERE intent_id = ?1",
            [intent_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    row.map(|(intent_digest, blob, stored_hash)| {
        if stored_hash != blob_hash(&blob) {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        verify_blob_binding(
            connection,
            CHOICE_MARKDOWN_RENDER_RECEIPT_ACTION,
            intent_id,
            "choice:markdown_render_receipt",
            &blob,
        )?;
        let receipt: MarkdownRenderReceipt = authority.decrypt_json(
            &blob,
            markdown_render_receipt_aad(intent_id, &intent_digest).as_bytes(),
        )?;
        if !receipt.is_valid() || receipt.intent_id != intent_id {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        Ok(receipt)
    })
    .transpose()
}

fn persist_markdown_render_receipt(
    transaction: &Transaction<'_>,
    authority: &LocalAuthority,
    intent: &MarkdownRenderIntent,
    receipt: &MarkdownRenderReceipt,
) -> Result<(), StoreError> {
    let intent_digest = markdown_render_intent_digest(intent)?;
    let blob = authority.encrypt_json(
        receipt,
        markdown_render_receipt_aad(&receipt.intent_id, &intent_digest).as_bytes(),
    )?;
    let state_hash = blob_hash(&blob);
    transaction.execute(
        "INSERT INTO choice_markdown_render_receipt
         (intent_id, encrypted_blob, blob_hash, committed_at_ms)
         VALUES (?1, ?2, ?3, ?4)",
        params![
            &receipt.intent_id,
            blob,
            state_hash,
            receipt.committed_at_ms
        ],
    )?;
    append_audit(
        transaction,
        authority,
        &AuditRecord {
            id: &format!("{}:receipt", receipt.intent_id),
            command_id: &format!("{}:receipt", receipt.intent_id),
            command_hash: &markdown_render_receipt_digest(receipt)?,
            actor: "openopen-host",
            action: CHOICE_MARKDOWN_RENDER_RECEIPT_ACTION,
            entity_id: &receipt.intent_id,
            created_at_ms: receipt.committed_at_ms,
            state_kind: "choice:markdown_render_receipt",
            state_hash: &state_hash,
        },
    )?;
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ChoiceIdleClockContinuity {
    Calibrate { trusted_now_ms: i64 },
    Continuous { trusted_now_ms: i64 },
    Uncertain,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct ChoiceIdleClockAnchor {
    evidence: ChoiceIdleClockEvidence,
    trusted_now_ms: i64,
}

impl ChoiceIdleClockAnchor {
    fn is_valid(&self) -> bool {
        self.evidence.is_valid() && self.trusted_now_ms >= 0
    }
}

fn classify_choice_idle_clock(
    connection: &Connection,
    authority: &LocalAuthority,
    next: &ChoiceIdleClockEvidence,
) -> Result<ChoiceIdleClockContinuity, StoreError> {
    let previous = load_choice_idle_clock_anchor(connection, authority)?;
    let Some(previous) = previous else {
        return Ok(ChoiceIdleClockContinuity::Calibrate {
            trusted_now_ms: next.wall_clock_ms,
        });
    };
    if next.boot_id != previous.evidence.boot_id {
        // A changed boot has no comparable monotonic clock. It must be
        // calibrated by this valid Host-owned wake and cannot authorize an
        // idle/stale transition until a later same-boot sample.
        return Ok(if next.wall_clock_ms >= previous.evidence.wall_clock_ms {
            ChoiceIdleClockContinuity::Calibrate {
                trusted_now_ms: previous.trusted_now_ms,
            }
        } else {
            ChoiceIdleClockContinuity::Uncertain
        });
    }
    let Some(wall_delta) = next
        .wall_clock_ms
        .checked_sub(previous.evidence.wall_clock_ms)
    else {
        return Ok(ChoiceIdleClockContinuity::Uncertain);
    };
    let Some(monotonic_delta) = next
        .monotonic_ms
        .checked_sub(previous.evidence.monotonic_ms)
    else {
        return Ok(ChoiceIdleClockContinuity::Uncertain);
    };
    // An exact repeated sample is an idempotent no-progress observation.
    // Rollback of either clock is uncertainty and leaves the last good anchor
    // intact. A positive sleep-shaped skew is handled separately below as a
    // non-authorizing recalibration point.
    if wall_delta == 0 && monotonic_delta == 0 {
        return Ok(ChoiceIdleClockContinuity::Continuous {
            trusted_now_ms: previous.trusted_now_ms,
        });
    }
    if wall_delta <= 0 || monotonic_delta <= 0 {
        return Ok(ChoiceIdleClockContinuity::Uncertain);
    }
    if wall_delta.abs_diff(monotonic_delta) > 60_000 {
        // A sleep-shaped or forward-adjusted wall sample is ambiguous and may
        // not consume a deadline. Persist it only as a new calibration point;
        // the caller returns typed clock uncertainty and requires a later
        // same-boot sample before any Choice transition can occur. This makes
        // uncertainty recoverable without treating unknown elapsed time as
        // authority.
        return Ok(ChoiceIdleClockContinuity::Calibrate {
            trusted_now_ms: previous.trusted_now_ms,
        });
    }
    let trusted_now_ms = previous
        .trusted_now_ms
        .checked_add(monotonic_delta)
        .ok_or(StoreError::ChoiceClockUncertain)?;
    Ok(ChoiceIdleClockContinuity::Continuous { trusted_now_ms })
}

/// Persists a non-authorizing clock calibration.  If an old active `ChoiceSet`
/// exists, retire it in the same transaction: an ambiguous first/rebooted
/// clock sample cannot leave model-derived authority consumable on a retry.
fn calibrate_choice_idle_state(
    transaction: Transaction<'_>,
    authority: &LocalAuthority,
    current: ChoiceLoopSnapshot,
    clock: &ChoiceIdleClockEvidence,
    trusted_now_ms: i64,
) -> Result<ChoiceIdleAdvance, StoreError> {
    persist_choice_idle_clock_anchor(&transaction, authority, clock, trusted_now_ms)?;
    if current.session.state != ChoiceSessionState::Active {
        transaction.commit()?;
        return Ok(ChoiceIdleAdvance::Calibrated(current));
    }

    let mut next = current.clone();
    next.session.revision = current
        .session
        .revision
        .checked_add(1)
        .ok_or(StoreError::ChoiceLoopStateConflict)?;
    next.session.state = ChoiceSessionState::SoftIdle;
    next.session.active_choice_set_id = None;
    next.active_choice_set = None;
    if !next.is_permitted_successor_of(&current) {
        return Err(StoreError::ChoiceLoopStateConflict);
    }
    let retirement_at_ms = trusted_now_ms
        .max(current.session.last_input_at_ms)
        .max(current.document_manifest.generated_at_ms);
    persist_choice_loop_snapshot(&transaction, authority, &next, retirement_at_ms)?;
    transaction.commit()?;
    Ok(ChoiceIdleAdvance::Transitioned(next))
}

fn load_choice_idle_clock_anchor(
    connection: &Connection,
    authority: &LocalAuthority,
) -> Result<Option<ChoiceIdleClockAnchor>, StoreError> {
    connection
        .query_row(
            "SELECT encrypted_blob, blob_hash
             FROM choice_idle_clock_anchor WHERE singleton_id = 1",
            [],
            |row| Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?
        .map(|(blob, stored_hash)| {
            if stored_hash != blob_hash(&blob) {
                return Err(StoreError::ChoiceClockUncertain);
            }
            verify_blob_binding(
                connection,
                CHOICE_IDLE_CLOCK_ACTION,
                "choice-idle-clock",
                "choice:idle_clock_anchor",
                &blob,
            )?;
            let anchor: ChoiceIdleClockAnchor = authority
                .decrypt_json(&blob, choice_idle_clock_aad().as_bytes())
                .map_err(StoreError::Crypto)?;
            anchor
                .is_valid()
                .then_some(anchor)
                .ok_or(StoreError::ChoiceClockUncertain)
        })
        .transpose()
}

fn persist_choice_idle_clock_anchor(
    transaction: &Transaction<'_>,
    authority: &LocalAuthority,
    clock: &ChoiceIdleClockEvidence,
    trusted_now_ms: i64,
) -> Result<(), StoreError> {
    let anchor = ChoiceIdleClockAnchor {
        evidence: clock.clone(),
        trusted_now_ms,
    };
    if !anchor.is_valid() {
        return Err(StoreError::ChoiceClockUncertain);
    }
    let command_hash = serde_json::to_vec(&anchor)
        .map(|bytes| format!("{:x}", Sha256::digest(bytes)))
        .map_err(|error| StoreError::Crypto(CryptoError::Serialization(error.to_string())))?;
    let blob = authority.encrypt_json(&anchor, choice_idle_clock_aad().as_bytes())?;
    let state_hash = blob_hash(&blob);
    transaction.execute(
        "INSERT INTO choice_idle_clock_anchor (singleton_id, encrypted_blob, blob_hash)
         VALUES (1, ?1, ?2)
         ON CONFLICT(singleton_id) DO UPDATE SET encrypted_blob = excluded.encrypted_blob,
             blob_hash = excluded.blob_hash",
        params![blob, state_hash],
    )?;
    append_audit(
        transaction,
        authority,
        &AuditRecord {
            id: &format!("choice-idle-clock:{command_hash}"),
            command_id: &format!("choice-idle-clock:{command_hash}"),
            command_hash: &command_hash,
            actor: "openopen-host",
            action: CHOICE_IDLE_CLOCK_ACTION,
            entity_id: "choice-idle-clock",
            created_at_ms: clock.wall_clock_ms,
            state_kind: "choice:idle_clock_anchor",
            state_hash: &state_hash,
        },
    )?;
    Ok(())
}

fn markdown_render_intent_digest(intent: &MarkdownRenderIntent) -> Result<String, StoreError> {
    serde_json::to_vec(intent)
        .map(|bytes| format!("{:x}", Sha256::digest(bytes)))
        .map_err(|error| StoreError::Crypto(CryptoError::Serialization(error.to_string())))
}

fn markdown_render_receipt_digest(receipt: &MarkdownRenderReceipt) -> Result<String, StoreError> {
    serde_json::to_vec(receipt)
        .map(|bytes| format!("{:x}", Sha256::digest(bytes)))
        .map_err(|error| StoreError::Crypto(CryptoError::Serialization(error.to_string())))
}

fn persist_choice_loop_snapshot(
    transaction: &Transaction<'_>,
    authority: &LocalAuthority,
    snapshot: &ChoiceLoopSnapshot,
    updated_at_ms: i64,
) -> Result<(), StoreError> {
    let blob = authority.encrypt_json(snapshot, choice_loop_state_aad().as_bytes())?;
    let snapshot_hash = format!(
        "{:x}",
        Sha256::digest(
            serde_json::to_vec(snapshot)
                .map_err(|error| CryptoError::Serialization(error.to_string()))?
        )
    );
    let encrypted_blob_hash = blob_hash(&blob);
    let audit_id = format!("choice-loop-state-{snapshot_hash}-{updated_at_ms}");
    transaction.execute(
        "INSERT INTO choice_loop_state (singleton_id, encrypted_blob, blob_hash, updated_at_ms)
         VALUES (1, ?1, ?2, ?3)
         ON CONFLICT(singleton_id) DO UPDATE SET encrypted_blob = excluded.encrypted_blob,
             blob_hash = excluded.blob_hash, updated_at_ms = excluded.updated_at_ms",
        params![blob, encrypted_blob_hash, updated_at_ms],
    )?;
    append_audit(
        transaction,
        authority,
        &AuditRecord {
            id: &audit_id,
            command_id: &audit_id,
            command_hash: &snapshot_hash,
            actor: "owner",
            action: CHOICE_LOOP_STATE_ACTION,
            entity_id: "choice-loop",
            created_at_ms: updated_at_ms,
            state_kind: "choice:loop_state",
            state_hash: &encrypted_blob_hash,
        },
    )?;
    Ok(())
}

fn valid_choice_reminder_time_zone(value: &str) -> bool {
    value.parse::<chrono_tz::Tz>().is_ok()
}

/// Binds every plaintext schedule index to the encrypted record.  These
/// columns are used for replay and current-revision lookup, so accepting a
/// record with only its ciphertext authenticated would let a direct row edit
/// alter idempotency or ordering without changing the blob itself.
fn choice_reminder_schedule_aad(
    schedule_id: &str,
    request_id: &str,
    choice_session_id: &str,
    revision: u64,
    accepted_at_ms: i64,
) -> Vec<u8> {
    let mut aad = b"openopen:choice-reminder-schedule:v2".to_vec();
    for value in [schedule_id, request_id, choice_session_id] {
        aad.extend_from_slice(&(value.len() as u64).to_be_bytes());
        aad.extend_from_slice(value.as_bytes());
    }
    aad.extend_from_slice(&8_u64.to_be_bytes());
    aad.extend_from_slice(&revision.to_be_bytes());
    aad.extend_from_slice(&8_u64.to_be_bytes());
    aad.extend_from_slice(&accepted_at_ms.to_be_bytes());
    aad
}

fn persist_choice_reminder_schedule(
    transaction: &Transaction<'_>,
    authority: &LocalAuthority,
    schedule: &ChoiceReminderSchedule,
) -> Result<(), StoreError> {
    if !schedule.is_valid() || !valid_choice_reminder_time_zone(&schedule.input.time_zone) {
        return Err(StoreError::ChoiceLoopStateConflict);
    }
    let blob = authority.encrypt_json(
        schedule,
        &choice_reminder_schedule_aad(
            &schedule.id,
            &schedule.input.request_id,
            &schedule.input.choice_session_id,
            schedule.revision,
            schedule.accepted_at_ms,
        ),
    )?;
    let hash = blob_hash(&blob);
    transaction.execute(
        "INSERT INTO choice_reminder_schedule
         (schedule_id, request_id, choice_session_id, revision, encrypted_blob, blob_hash, accepted_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![schedule.id, schedule.input.request_id, schedule.input.choice_session_id,
            i64::try_from(schedule.revision).map_err(|_| StoreError::ChoiceLoopStateConflict)?,
            blob, hash, schedule.accepted_at_ms],
    )?;
    append_audit(
        transaction,
        authority,
        &AuditRecord {
            id: &format!("choice-reminder-schedule-{}", schedule.id),
            command_id: &schedule.input.request_id,
            command_hash: &format!(
                "{:x}",
                Sha256::digest(
                    serde_json::to_vec(&schedule.input)
                        .map_err(|error| CryptoError::Serialization(error.to_string()))?
                )
            ),
            actor: "owner",
            action: CHOICE_REMINDER_SCHEDULE_ACTION,
            entity_id: &schedule.id,
            created_at_ms: schedule.accepted_at_ms,
            state_kind: "choice:reminder_schedule",
            state_hash: &hash,
        },
    )?;
    Ok(())
}

fn load_choice_reminder_schedule_by_request(
    connection: &Connection,
    authority: &LocalAuthority,
    request_id: &str,
) -> Result<Option<ChoiceReminderSchedule>, StoreError> {
    let id: Option<String> = connection
        .query_row(
            "SELECT schedule_id FROM choice_reminder_schedule WHERE request_id = ?1",
            [request_id],
            |row| row.get(0),
        )
        .optional()?;
    id.map(|id| load_choice_reminder_schedule(connection, authority, &id))
        .transpose()
}

fn load_current_choice_reminder_schedule(
    connection: &Connection,
    authority: &LocalAuthority,
    session_id: &str,
) -> Result<Option<ChoiceReminderSchedule>, StoreError> {
    let id: Option<String> = connection.query_row(
        "SELECT schedule_id FROM choice_reminder_schedule WHERE choice_session_id = ?1 ORDER BY revision DESC LIMIT 1",
        [session_id], |row| row.get(0),
    ).optional()?;
    id.map(|id| load_choice_reminder_schedule(connection, authority, &id))
        .transpose()
}

fn load_current_choice_reminder_schedule_for_revision(
    connection: &Connection,
    authority: &LocalAuthority,
    session_id: &str,
    expected_session_revision: u64,
) -> Result<Option<ChoiceReminderSchedule>, StoreError> {
    let id: Option<String> = connection
        .query_row(
            "SELECT schedule_id FROM choice_reminder_schedule
             WHERE choice_session_id = ?1 ORDER BY revision DESC LIMIT 1",
            [session_id],
            |row| row.get(0),
        )
        .optional()?;
    let Some(id) = id else {
        return Ok(None);
    };
    let schedule = load_choice_reminder_schedule(connection, authority, &id)?;
    Ok((schedule.input.expected_session_revision == expected_session_revision).then_some(schedule))
}

fn load_choice_reminder_schedule(
    connection: &Connection,
    authority: &LocalAuthority,
    schedule_id: &str,
) -> Result<ChoiceReminderSchedule, StoreError> {
    let (request_id, choice_session_id, revision, blob, hash, accepted_at_ms): (
        String,
        String,
        i64,
        Vec<u8>,
        String,
        i64,
    ) = connection.query_row(
        "SELECT request_id, choice_session_id, revision, encrypted_blob, blob_hash, accepted_at_ms
         FROM choice_reminder_schedule WHERE schedule_id = ?1",
        [schedule_id],
        |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
            ))
        },
    )?;
    let revision = u64::try_from(revision).map_err(|_| StoreError::ChoiceLoopStateConflict)?;
    if accepted_at_ms < 0 {
        return Err(StoreError::ChoiceLoopStateConflict);
    }
    if hash != blob_hash(&blob) {
        return Err(StoreError::ChoiceLoopStateConflict);
    }
    verify_blob_binding(
        connection,
        CHOICE_REMINDER_SCHEDULE_ACTION,
        schedule_id,
        "choice:reminder_schedule",
        &blob,
    )?;
    let schedule: ChoiceReminderSchedule = authority.decrypt_json(
        &blob,
        &choice_reminder_schedule_aad(
            schedule_id,
            &request_id,
            &choice_session_id,
            revision,
            accepted_at_ms,
        ),
    )?;
    if !schedule.is_valid()
        || !valid_choice_reminder_time_zone(&schedule.input.time_zone)
        || schedule.id != schedule_id
        || schedule.input.request_id != request_id
        || schedule.input.choice_session_id != choice_session_id
        || schedule.revision != revision
        || schedule.accepted_at_ms != accepted_at_ms
    {
        return Err(StoreError::ChoiceLoopStateConflict);
    }
    Ok(schedule)
}

fn confirmation_delivery_is_bound(
    confirmation: &ChoiceConsolidatedConfirmation,
    session: &openopen_protocol::ChoiceSession,
) -> bool {
    match (
        confirmation.delivery_binding_id.as_deref(),
        confirmation.recipient.as_deref(),
        confirmation.delivery_scope.as_deref(),
        session.primary_delivery_binding_id.as_deref(),
    ) {
        (None, None, None, None) => true,
        (Some(binding), Some(_), Some(_), Some(primary)) => binding == primary,
        _ => false,
    }
}

fn session_model_matches_choice_set(session: &ChoiceSession, choice_set: &ChoiceSet) -> bool {
    matches!(
        &session.model_selection_state,
        ModelSelectionState::Selected {
            model_provenance_ref,
        } if model_provenance_ref == &choice_set.model_provenance.id
    )
}

fn selected_model_matches_choice_set(selection: &ModelSelection, choice_set: &ChoiceSet) -> bool {
    selection.model_id == choice_set.model_provenance.model_id
        && selection.requested_effort == choice_set.model_provenance.requested_effort
        && selection.actual_effort == choice_set.model_provenance.actual_effort
        && selection.catalog_fingerprint == choice_set.model_provenance.catalog_fingerprint
        && selection.catalog_revision == choice_set.model_provenance.catalog_revision
        && selection.account_display_class == choice_set.model_provenance.account_display_class
        && selection.protocol_schema_revision
            == choice_set.model_provenance.protocol_schema_revision
}

fn raw_choice_loop_batch_lacks_delivery_binding(raw: &serde_json::Value) -> bool {
    raw.get("activeBatch")
        .and_then(serde_json::Value::as_object)
        .is_some_and(|batch| !batch.contains_key("deliveryBindingId"))
}

fn load_raw_choice_loop_snapshot(
    connection: &Connection,
    authority: &LocalAuthority,
) -> Result<Option<serde_json::Value>, StoreError> {
    let row: Option<(Vec<u8>, String)> = connection
        .query_row(
            "SELECT encrypted_blob, blob_hash FROM choice_loop_state WHERE singleton_id = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    row.map(|(blob, stored_hash)| {
        if stored_hash != blob_hash(&blob) {
            return Err(StoreError::ChoiceLoopStateConflict);
        }
        verify_blob_binding(
            connection,
            CHOICE_LOOP_STATE_ACTION,
            "choice-loop",
            "choice:loop_state",
            &blob,
        )?;
        authority
            .decrypt_json(&blob, choice_loop_state_aad().as_bytes())
            .map_err(StoreError::Crypto)
    })
    .transpose()
}

fn load_choice_loop_snapshot(
    connection: &Connection,
    authority: &LocalAuthority,
) -> Result<Option<ChoiceLoopSnapshot>, StoreError> {
    load_raw_choice_loop_snapshot(connection, authority)?
        .map(|mut raw| {
            let historical_batch_without_binding = raw
                .get("activeBatch")
                .and_then(serde_json::Value::as_object)
                .is_some_and(|batch| !batch.contains_key("deliveryBindingId"));
            let object = raw
                .as_object_mut()
                .ok_or(StoreError::ChoiceLoopStateConflict)?;
            // These fields were added after the first Choice-loop persistence
            // shape. Their absence is a neutral empty value, unlike a missing
            // authority binding below.
            object
                .entry("lastSelection")
                .or_insert(serde_json::Value::Null);
            object
                .entry("confirmation")
                .or_insert(serde_json::Value::Null);
            if historical_batch_without_binding {
                // Never infer authority from owner/provider/body/time. Expose the
                // legacy batch as a typed blocked recovery state until a fresh,
                // Host-derived batch is accepted through the normal command path.
                if let Some(batch) = object
                    .get_mut("activeBatch")
                    .and_then(serde_json::Value::as_object_mut)
                {
                    batch.insert(
                        "deliveryBindingId".to_owned(),
                        serde_json::Value::String("blocked-missing-binding".to_owned()),
                    );
                }
                if let Some(session) = object
                    .get_mut("session")
                    .and_then(serde_json::Value::as_object_mut)
                {
                    session.insert(
                        "state".to_owned(),
                        serde_json::Value::String("blocked".to_owned()),
                    );
                    session.insert("activeChoiceSetId".to_owned(), serde_json::Value::Null);
                    session.insert("pendingConfirmationId".to_owned(), serde_json::Value::Null);
                }
                object.insert("activeChoiceSet".to_owned(), serde_json::Value::Null);
                object.insert("confirmation".to_owned(), serde_json::Value::Null);
            }
            let snapshot: ChoiceLoopSnapshot = serde_json::from_value(raw).map_err(|error| {
                StoreError::Crypto(CryptoError::Serialization(error.to_string()))
            })?;
            if !snapshot.is_valid() {
                return Err(StoreError::ChoiceLoopStateConflict);
            }
            Ok(snapshot)
        })
        .transpose()
}

fn channel_json(channel: ChannelKind) -> Result<String, StoreError> {
    serde_json::to_string(&channel)
        .map_err(|error| CryptoError::Serialization(error.to_string()).into())
}

fn channel_observation_entity(channel: ChannelKind, source_message_id: &str) -> String {
    format!(
        "channel-observation-{:x}",
        Sha256::digest(
            serde_json::to_vec(&serde_json::json!({
                "channel": channel,
                "sourceMessageId": source_message_id,
                "version": 1,
            }))
            .expect("channel observation identity is infallibly serializable")
        )
    )
}

fn channel_failure_incident_id(
    channel: ChannelKind,
    source_message_id: &str,
    dispatch_state_hash: &str,
    failure_class: ChannelFailureClass,
) -> String {
    format!(
        "channel-failure-{:x}",
        Sha256::digest(
            serde_json::to_vec(&serde_json::json!({
                "channel": channel,
                "dispatchStateHash": dispatch_state_hash,
                "failureClass": failure_class,
                "sourceMessageId": source_message_id,
                "version": 1,
            }))
            .expect("channel failure identity is infallibly serializable")
        )
    )
}

const fn channel_failure_class_name(failure_class: ChannelFailureClass) -> &'static str {
    match failure_class {
        ChannelFailureClass::ModelResultUnavailable => "modelResultUnavailable",
    }
}

fn is_canonical_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn audit_anchor_for_action(
    connection: &Connection,
    action: &str,
    entity_id: &str,
) -> Result<AuditAnchor, StoreError> {
    connection
        .query_row(
            "SELECT sequence, entry_hash, signature_hex FROM audit_ledger
             WHERE action = ?1 AND entity_id = ?2 ORDER BY sequence DESC LIMIT 1",
            params![action, entity_id],
            |row| {
                Ok(AuditAnchor {
                    sequence: row.get(0)?,
                    entry_hash: row.get(1)?,
                    signature_hex: row.get(2)?,
                })
            },
        )
        .optional()?
        .ok_or_else(|| StoreError::StateBindingMismatch(entity_id.to_owned()))
}

fn channel_cursor_entity(channel: ChannelKind, conversation_id: &str) -> String {
    format!(
        "channel-cursor-{:x}",
        Sha256::digest(
            serde_json::to_vec(&serde_json::json!({
                "channel": channel,
                "conversationId": conversation_id,
                "version": 1,
            }))
            .expect("channel cursor identity is infallibly serializable")
        )
    )
}

fn load_channel_pairing(
    connection: &Connection,
    authority: &LocalAuthority,
    channel: ChannelKind,
) -> Result<Option<ChannelPairing>, StoreError> {
    let channel_json = channel_json(channel)?;
    let row = connection
        .query_row(
            "SELECT encrypted_blob, paired_at_ms, blob_hash
             FROM channel_pairing WHERE channel_json = ?1",
            [&channel_json],
            |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .optional()?;
    row.map(|(blob, paired_at_ms, stored_hash)| {
        let mismatch = || StoreError::ChannelStateMismatch(channel_json.clone());
        if blob_hash(&blob) != stored_hash {
            return Err(mismatch());
        }
        verify_blob_binding(
            connection,
            CHANNEL_PAIRING_ACTION,
            &channel_json,
            "channelPairing",
            &blob,
        )
        .map_err(|_| mismatch())?;
        let pairing: ChannelPairing = authority
            .decrypt_json(&blob, channel_pairing_aad(&channel_json).as_bytes())
            .map_err(|_| mismatch())?;
        validate_pairing(&pairing).map_err(|_| mismatch())?;
        if pairing.channel != channel || pairing.paired_at_ms != paired_at_ms {
            return Err(mismatch());
        }
        Ok(pairing)
    })
    .transpose()
}

fn load_channel_cursor(
    connection: &Connection,
    authority: &LocalAuthority,
    channel: ChannelKind,
    conversation_id: &str,
) -> Result<Option<ChannelCursor>, StoreError> {
    let channel_json = channel_json(channel)?;
    let entity_id = channel_cursor_entity(channel, conversation_id);
    let row = connection
        .query_row(
            "SELECT cursor_order, encrypted_blob, blob_hash, entity_id
             FROM channel_cursor WHERE channel_json = ?1 AND conversation_id = ?2",
            params![channel_json, conversation_id],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            },
        )
        .optional()?;
    row.map(|(order, blob, stored_hash, stored_entity)| {
        let mismatch = || StoreError::ChannelStateMismatch(entity_id.clone());
        if order < 0 || stored_entity != entity_id || blob_hash(&blob) != stored_hash {
            return Err(mismatch());
        }
        verify_blob_binding(
            connection,
            CHANNEL_CURSOR_ACTION,
            &entity_id,
            "channelCursor",
            &blob,
        )
        .map_err(|_| mismatch())?;
        let cursor: ChannelCursor = authority
            .decrypt_json(&blob, channel_cursor_aad(&entity_id).as_bytes())
            .map_err(|_| mismatch())?;
        validate_cursor(&cursor).map_err(|_| mismatch())?;
        if cursor.channel != channel
            || cursor.conversation_id != conversation_id
            || cursor.order != u64::try_from(order).map_err(|_| mismatch())?
        {
            return Err(mismatch());
        }
        Ok(cursor)
    })
    .transpose()
}

fn load_channel_observation(
    connection: &Connection,
    authority: &LocalAuthority,
    channel: ChannelKind,
    source_message_id: &str,
) -> Result<Option<StoredChannelObservation>, StoreError> {
    let channel_json = channel_json(channel)?;
    let entity_id = channel_observation_entity(channel, source_message_id);
    let row = connection
        .query_row(
            "SELECT conversation_id, cursor_order, decision_json, encrypted_blob,
                    blob_hash, entity_id
             FROM channel_observation
             WHERE channel_json = ?1 AND source_message_id = ?2",
            params![channel_json, source_message_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Vec<u8>>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            },
        )
        .optional()?;
    row.map(
        |(conversation, cursor_order, decision_json, blob, stored_hash, stored_entity)| {
            let mismatch = || StoreError::ChannelStateMismatch(entity_id.clone());
            if cursor_order < 0 || stored_entity != entity_id || blob_hash(&blob) != stored_hash {
                return Err(mismatch());
            }
            verify_blob_binding(
                connection,
                CHANNEL_OBSERVATION_ACTION,
                &entity_id,
                "channelObservation",
                &blob,
            )
            .map_err(|_| mismatch())?;
            let stored: StoredChannelObservation = authority
                .decrypt_json(&blob, channel_observation_aad(&entity_id).as_bytes())
                .map_err(|_| mismatch())?;
            validate_observation(&stored.observation).map_err(|_| mismatch())?;
            let encoded_decision =
                serde_json::to_string(&stored.decision).map_err(|_| mismatch())?;
            let accepted_content_is_valid = if stored.decision == ChannelInboundDecision::Accepted {
                stored.accepted_content.as_ref().is_some_and(|content| {
                    !content.is_empty()
                        && content.trim() == content
                        && content.len() <= MAX_CHANNEL_CONTENT_BYTES
                        && !content.as_bytes().contains(&0)
                        && format!("{:x}", Sha256::digest(content.as_bytes()))
                            == stored.observation.envelope.content_sha256
                })
            } else {
                stored.accepted_content.is_none()
            };
            if stored.observation.envelope.channel != channel
                || stored.observation.envelope.source_message_id != source_message_id
                || stored.observation.envelope.conversation_id != conversation
                || stored.observation.cursor.order
                    != u64::try_from(cursor_order).map_err(|_| mismatch())?
                || encoded_decision != decision_json
                || !accepted_content_is_valid
            {
                return Err(mismatch());
            }
            Ok(stored)
        },
    )
    .transpose()
}

fn write_channel_observation(
    transaction: &Transaction<'_>,
    authority: &LocalAuthority,
    observation: &ChannelObservation,
    decision: ChannelInboundDecision,
    content: &str,
) -> Result<(), StoreError> {
    let channel = channel_json(observation.envelope.channel)?;
    let entity_id = channel_observation_entity(
        observation.envelope.channel,
        &observation.envelope.source_message_id,
    );
    let stored = StoredChannelObservation {
        observation: observation.clone(),
        decision,
        accepted_content: (decision == ChannelInboundDecision::Accepted)
            .then(|| content.to_owned()),
    };
    let blob = authority.encrypt_json(&stored, channel_observation_aad(&entity_id).as_bytes())?;
    let state_hash = blob_hash(&blob);
    let decision_json = serde_json::to_string(&decision)
        .map_err(|error| CryptoError::Serialization(error.to_string()))?;
    append_audit(
        transaction,
        authority,
        &AuditRecord {
            id: &format!("{entity_id}:observed"),
            command_id: &entity_id,
            command_hash: &state_hash,
            actor: &observation.envelope.sender_id,
            action: CHANNEL_OBSERVATION_ACTION,
            entity_id: &entity_id,
            created_at_ms: observation.envelope.received_at_ms,
            state_kind: "channelObservation",
            state_hash: &state_hash,
        },
    )?;
    let cursor_order = i64::try_from(observation.cursor.order)
        .map_err(|_| StoreError::ChannelObservationConflict)?;
    transaction.execute(
        "INSERT INTO channel_observation
            (channel_json, source_message_id, entity_id, conversation_id,
             cursor_order, decision_json, encrypted_blob, blob_hash)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            channel,
            observation.envelope.source_message_id,
            entity_id,
            observation.envelope.conversation_id,
            cursor_order,
            decision_json,
            blob,
            state_hash,
        ],
    )?;
    Ok(())
}

fn write_channel_cursor(
    transaction: &Transaction<'_>,
    authority: &LocalAuthority,
    cursor: &ChannelCursor,
) -> Result<(), StoreError> {
    let channel = channel_json(cursor.channel)?;
    let entity_id = channel_cursor_entity(cursor.channel, &cursor.conversation_id);
    let blob = authority.encrypt_json(cursor, channel_cursor_aad(&entity_id).as_bytes())?;
    let state_hash = blob_hash(&blob);
    append_audit(
        transaction,
        authority,
        &AuditRecord {
            id: &format!("{entity_id}:{}", cursor.order),
            command_id: &format!("{entity_id}-{}", cursor.order),
            command_hash: &state_hash,
            actor: "channel-adapter",
            action: CHANNEL_CURSOR_ACTION,
            entity_id: &entity_id,
            created_at_ms: cursor.observed_at_ms,
            state_kind: "channelCursor",
            state_hash: &state_hash,
        },
    )?;
    let cursor_order =
        i64::try_from(cursor.order).map_err(|_| StoreError::ChannelObservationConflict)?;
    transaction.execute(
        "INSERT INTO channel_cursor
            (channel_json, conversation_id, entity_id, cursor_order, encrypted_blob, blob_hash)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(channel_json, conversation_id) DO UPDATE SET
            entity_id = excluded.entity_id,
            cursor_order = excluded.cursor_order,
            encrypted_blob = excluded.encrypted_blob,
            blob_hash = excluded.blob_hash",
        params![
            channel,
            cursor.conversation_id,
            entity_id,
            cursor_order,
            blob,
            state_hash,
        ],
    )?;
    Ok(())
}

fn load_channel_model_dispatch(
    connection: &Connection,
    authority: &LocalAuthority,
    channel: ChannelKind,
    source_message_id: &str,
) -> Result<Option<StoredChannelModelDispatch>, StoreError> {
    let encoded_channel = channel_json(channel)?;
    let entity_id = channel_observation_entity(channel, source_message_id);
    let row = connection
        .query_row(
            "SELECT entity_id, status_json, suggestion_id, encrypted_blob, blob_hash
             FROM channel_model_dispatch
             WHERE channel_json = ?1 AND source_message_id = ?2",
            params![encoded_channel, source_message_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Vec<u8>>(3)?,
                    row.get::<_, String>(4)?,
                ))
            },
        )
        .optional()?;
    row.map(
        |(stored_entity, status, suggestion_id, blob, stored_hash)| {
            let mismatch = || StoreError::ChannelStateMismatch(entity_id.clone());
            if stored_entity != entity_id || blob_hash(&blob) != stored_hash {
                return Err(mismatch());
            }
            let (action, state_kind, state) = match status.as_str() {
                "queued" if suggestion_id.is_none() => (
                    CHANNEL_MODEL_QUEUED_ACTION,
                    "channelModelQueued",
                    StoredChannelModelState::Queued,
                ),
                "started" if suggestion_id.is_none() => (
                    CHANNEL_MODEL_ACTION,
                    "channelModelStarted",
                    StoredChannelModelState::Started,
                ),
                "ready" if suggestion_id.is_some() => (
                    CHANNEL_SUGGESTION_ACTION,
                    "channelSuggestionReady",
                    StoredChannelModelState::Ready,
                ),
                "failed" if suggestion_id.is_none() => (
                    CHANNEL_MODEL_FAILED_ACTION,
                    "channelModelFailed",
                    StoredChannelModelState::Failed,
                ),
                _ => return Err(mismatch()),
            };
            verify_blob_binding(connection, action, &entity_id, state_kind, &blob)
                .map_err(|_| mismatch())?;
            let stored: StoredChannelModelDispatch = authority
                .decrypt_json(&blob, channel_model_aad(&entity_id).as_bytes())
                .map_err(|_| mismatch())?;
            if stored.channel != channel
                || stored.source_message_id != source_message_id
                || stored.state != state
                || stored.suggestion.as_ref().map(|value| &value.id) != suggestion_id.as_ref()
                || stored
                    .suggestion
                    .as_ref()
                    .is_some_and(|value| !valid_channel_suggestion(value))
            {
                return Err(mismatch());
            }
            Ok(stored)
        },
    )
    .transpose()
}

fn channel_failure_source_anchor(
    connection: &Connection,
    channel: ChannelKind,
    source_message_id: &str,
    dispatch_state_hash: &str,
) -> Result<(AuditAnchor, i64), StoreError> {
    let entity_id = channel_observation_entity(channel, source_message_id);
    connection
        .query_row(
            "SELECT sequence, entry_hash, signature_hex, created_at_ms
             FROM audit_ledger
             WHERE action = ?1 AND entity_id = ?2 AND state_kind = 'channelModelFailed'
               AND state_hash = ?3
             ORDER BY sequence DESC LIMIT 1",
            params![CHANNEL_MODEL_FAILED_ACTION, entity_id, dispatch_state_hash],
            |row| {
                Ok((
                    AuditAnchor {
                        sequence: row.get(0)?,
                        entry_hash: row.get(1)?,
                        signature_hex: row.get(2)?,
                    },
                    row.get(3)?,
                ))
            },
        )
        .optional()?
        .ok_or_else(|| StoreError::ChannelFailureIncidentConflict(source_message_id.to_owned()))
}

fn verify_channel_failure_incident_source(
    connection: &Connection,
    authority: &LocalAuthority,
    incident_id: &str,
    encoded_channel: &str,
    stored: &StoredChannelFailureIncident,
) -> Result<(), StoreError> {
    let mismatch = || StoreError::ChannelFailureIncidentConflict(incident_id.to_owned());
    let dispatch = load_channel_model_dispatch(
        connection,
        authority,
        stored.channel,
        &stored.source_message_id,
    )?
    .ok_or_else(mismatch)?;
    if dispatch.state != StoredChannelModelState::Failed {
        return Err(mismatch());
    }
    let dispatch_hash: String = connection.query_row(
        "SELECT blob_hash FROM channel_model_dispatch
         WHERE channel_json = ?1 AND source_message_id = ?2",
        params![encoded_channel, &stored.source_message_id],
        |row| row.get(0),
    )?;
    if dispatch_hash != stored.dispatch_state_hash {
        return Err(mismatch());
    }
    let (source_anchor, occurred_at_ms) = channel_failure_source_anchor(
        connection,
        stored.channel,
        &stored.source_message_id,
        &stored.dispatch_state_hash,
    )?;
    if source_anchor != stored.source_audit_anchor || occurred_at_ms != stored.occurred_at_ms {
        return Err(mismatch());
    }
    Ok(())
}

fn verify_channel_failure_incident_audit(
    connection: &Connection,
    incident_id: &str,
    acknowledged: bool,
    blob: &[u8],
    stored: &StoredChannelFailureIncident,
) -> Result<(AuditAnchor, Option<AuditAnchor>), StoreError> {
    let mismatch = || StoreError::ChannelFailureIncidentConflict(incident_id.to_owned());
    let incident_anchor =
        audit_anchor_for_action(connection, CHANNEL_FAILURE_INCIDENT_ACTION, incident_id)?;
    let acknowledgement_anchor = if acknowledged {
        verify_blob_binding(
            connection,
            CHANNEL_FAILURE_ACK_ACTION,
            incident_id,
            "channelFailureIncidentAcknowledged",
            blob,
        )?;
        Some(audit_anchor_for_action(
            connection,
            CHANNEL_FAILURE_ACK_ACTION,
            incident_id,
        )?)
    } else {
        verify_blob_binding(
            connection,
            CHANNEL_FAILURE_INCIDENT_ACTION,
            incident_id,
            "channelFailureIncident",
            blob,
        )?;
        None
    };
    if stored.acknowledgement.as_ref().is_some_and(|ack| {
        ack.acknowledged_at_ms < stored.occurred_at_ms
            || ack.runtime_revision < stored.runtime_revision
            || ack.incident_anchor != incident_anchor
    }) {
        return Err(mismatch());
    }
    Ok((incident_anchor, acknowledgement_anchor))
}

fn load_channel_failure_incident(
    connection: &Connection,
    authority: &LocalAuthority,
    incident_id: &str,
) -> Result<Option<LoadedChannelFailureIncident>, StoreError> {
    let row = connection
        .query_row(
            "SELECT channel_json, source_message_id, failure_class, acknowledged,
                    encrypted_blob, blob_hash
             FROM channel_failure_incident WHERE incident_id = ?1",
            [incident_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, Vec<u8>>(4)?,
                    row.get::<_, String>(5)?,
                ))
            },
        )
        .optional()?;
    row.map(
        |(encoded_channel, source_message_id, failure_class, acknowledged, blob, stored_hash)| {
            let mismatch = || StoreError::ChannelFailureIncidentConflict(incident_id.to_owned());
            if !matches!(acknowledged, 0 | 1) || blob_hash(&blob) != stored_hash {
                return Err(mismatch());
            }
            let channel: ChannelKind =
                serde_json::from_str(&encoded_channel).map_err(|_| mismatch())?;
            let stored: StoredChannelFailureIncident = authority
                .decrypt_json(&blob, channel_failure_incident_aad(incident_id).as_bytes())
                .map_err(|_| mismatch())?;
            let expected_class = channel_failure_class_name(stored.failure_class);
            if stored.incident_id != incident_id
                || stored.channel != channel
                || stored.source_message_id != source_message_id
                || failure_class != expected_class
                || (stored.acknowledgement.is_some()) != (acknowledged == 1)
                || stored.occurred_at_ms < 0
                || !is_canonical_sha256(&stored.dispatch_state_hash)
            {
                return Err(mismatch());
            }
            let expected_id = channel_failure_incident_id(
                stored.channel,
                &stored.source_message_id,
                &stored.dispatch_state_hash,
                stored.failure_class,
            );
            if expected_id != stored.incident_id {
                return Err(mismatch());
            }
            verify_channel_failure_incident_source(
                connection,
                authority,
                incident_id,
                &encoded_channel,
                &stored,
            )?;
            let (incident_anchor, acknowledgement_anchor) = verify_channel_failure_incident_audit(
                connection,
                incident_id,
                acknowledged == 1,
                &blob,
                &stored,
            )?;
            Ok(LoadedChannelFailureIncident {
                stored,
                incident_anchor,
                acknowledgement_anchor,
            })
        },
    )
    .transpose()
}

fn write_channel_failure_incident(
    transaction: &Transaction<'_>,
    authority: &LocalAuthority,
    stored: &StoredChannelFailureIncident,
) -> Result<(), StoreError> {
    if stored.acknowledgement.is_some()
        || stored.occurred_at_ms < 0
        || !is_canonical_sha256(&stored.dispatch_state_hash)
        || stored.incident_id
            != channel_failure_incident_id(
                stored.channel,
                &stored.source_message_id,
                &stored.dispatch_state_hash,
                stored.failure_class,
            )
    {
        return Err(StoreError::ChannelFailureIncidentConflict(
            stored.incident_id.clone(),
        ));
    }
    let (source_anchor, occurred_at_ms) = channel_failure_source_anchor(
        transaction,
        stored.channel,
        &stored.source_message_id,
        &stored.dispatch_state_hash,
    )?;
    if source_anchor != stored.source_audit_anchor || occurred_at_ms != stored.occurred_at_ms {
        return Err(StoreError::ChannelFailureIncidentConflict(
            stored.incident_id.clone(),
        ));
    }
    let blob = authority.encrypt_json(
        stored,
        channel_failure_incident_aad(&stored.incident_id).as_bytes(),
    )?;
    let state_hash = blob_hash(&blob);
    append_audit(
        transaction,
        authority,
        &AuditRecord {
            id: &format!("{}:recorded", stored.incident_id),
            command_id: &format!("{}-record", stored.incident_id),
            command_hash: &state_hash,
            actor: "openopen-model-runtime",
            action: CHANNEL_FAILURE_INCIDENT_ACTION,
            entity_id: &stored.incident_id,
            created_at_ms: stored.occurred_at_ms,
            state_kind: "channelFailureIncident",
            state_hash: &state_hash,
        },
    )?;
    transaction.execute(
        "INSERT INTO channel_failure_incident
            (incident_id, channel_json, source_message_id, failure_class,
             acknowledged, encrypted_blob, blob_hash)
         VALUES (?1, ?2, ?3, ?4, 0, ?5, ?6)",
        params![
            stored.incident_id,
            channel_json(stored.channel)?,
            stored.source_message_id,
            channel_failure_class_name(stored.failure_class),
            blob,
            state_hash,
        ],
    )?;
    Ok(())
}

fn update_channel_failure_acknowledgement(
    transaction: &Transaction<'_>,
    authority: &LocalAuthority,
    stored: &StoredChannelFailureIncident,
) -> Result<(), StoreError> {
    let Some(acknowledgement) = stored.acknowledgement.as_ref() else {
        return Err(StoreError::ChannelFailureIncidentConflict(
            stored.incident_id.clone(),
        ));
    };
    let blob = authority.encrypt_json(
        stored,
        channel_failure_incident_aad(&stored.incident_id).as_bytes(),
    )?;
    let state_hash = blob_hash(&blob);
    append_audit(
        transaction,
        authority,
        &AuditRecord {
            id: &format!("{}:acknowledged", stored.incident_id),
            command_id: &format!("{}-acknowledge", stored.incident_id),
            command_hash: &state_hash,
            actor: "openopen-local-owner",
            action: CHANNEL_FAILURE_ACK_ACTION,
            entity_id: &stored.incident_id,
            created_at_ms: acknowledgement.acknowledged_at_ms,
            state_kind: "channelFailureIncidentAcknowledged",
            state_hash: &state_hash,
        },
    )?;
    if transaction.execute(
        "UPDATE channel_failure_incident
         SET acknowledged = 1, encrypted_blob = ?1, blob_hash = ?2
         WHERE incident_id = ?3 AND acknowledged = 0",
        params![blob, state_hash, stored.incident_id],
    )? != 1
    {
        return Err(StoreError::ChannelFailureIncidentConflict(
            stored.incident_id.clone(),
        ));
    }
    Ok(())
}

fn channel_failure_incident_public(
    loaded: LoadedChannelFailureIncident,
) -> Result<ChannelFailureIncident, StoreError> {
    let acknowledgement = match (loaded.stored.acknowledgement, loaded.acknowledgement_anchor) {
        (Some(stored), Some(anchor)) => Some(ChannelFailureAcknowledgement {
            acknowledged_at_ms: stored.acknowledged_at_ms,
            runtime_revision: stored.runtime_revision,
            audit_anchor: effect_anchor(&anchor),
        }),
        (None, None) => None,
        _ => {
            return Err(StoreError::ChannelFailureIncidentConflict(
                loaded.stored.incident_id,
            ));
        }
    };
    Ok(ChannelFailureIncident {
        incident_id: loaded.stored.incident_id,
        channel: loaded.stored.channel,
        failure_class: loaded.stored.failure_class,
        occurred_at_ms: loaded.stored.occurred_at_ms,
        runtime_revision: loaded.stored.runtime_revision,
        dispatch_state_hash: loaded.stored.dispatch_state_hash,
        source_audit_anchor: effect_anchor(&loaded.stored.source_audit_anchor),
        incident_audit_anchor: effect_anchor(&loaded.incident_anchor),
        acknowledgement,
    })
}

fn write_channel_model_dispatch(
    transaction: &Transaction<'_>,
    authority: &LocalAuthority,
    stored: &StoredChannelModelDispatch,
    observed_at_ms: i64,
) -> Result<(), StoreError> {
    if stored.state != StoredChannelModelState::Queued || stored.suggestion.is_some() {
        return Err(StoreError::ChannelObservationConflict);
    }
    let entity_id = channel_observation_entity(stored.channel, &stored.source_message_id);
    let blob = authority.encrypt_json(stored, channel_model_aad(&entity_id).as_bytes())?;
    let state_hash = blob_hash(&blob);
    append_audit(
        transaction,
        authority,
        &AuditRecord {
            id: &format!("{entity_id}:model-queued"),
            command_id: &format!("{entity_id}-model-queued"),
            command_hash: &state_hash,
            actor: "channel-adapter",
            action: CHANNEL_MODEL_QUEUED_ACTION,
            entity_id: &entity_id,
            created_at_ms: observed_at_ms,
            state_kind: "channelModelQueued",
            state_hash: &state_hash,
        },
    )?;
    transaction.execute(
        "INSERT INTO channel_model_dispatch
            (entity_id, channel_json, source_message_id, status_json,
             suggestion_id, encrypted_blob, blob_hash)
         VALUES (?1, ?2, ?3, 'queued', NULL, ?4, ?5)",
        params![
            entity_id,
            channel_json(stored.channel)?,
            stored.source_message_id,
            blob,
            state_hash,
        ],
    )?;
    Ok(())
}

fn update_channel_model_started(
    transaction: &Transaction<'_>,
    authority: &LocalAuthority,
    stored: &StoredChannelModelDispatch,
    observed_at_ms: i64,
) -> Result<(), StoreError> {
    if stored.state != StoredChannelModelState::Started || stored.suggestion.is_some() {
        return Err(StoreError::ChannelObservationConflict);
    }
    let entity_id = channel_observation_entity(stored.channel, &stored.source_message_id);
    let blob = authority.encrypt_json(stored, channel_model_aad(&entity_id).as_bytes())?;
    let state_hash = blob_hash(&blob);
    append_audit(
        transaction,
        authority,
        &AuditRecord {
            id: &format!("{entity_id}:model-started"),
            command_id: &format!("{entity_id}-model-started"),
            command_hash: &state_hash,
            actor: "channel-adapter",
            action: CHANNEL_MODEL_ACTION,
            entity_id: &entity_id,
            created_at_ms: observed_at_ms,
            state_kind: "channelModelStarted",
            state_hash: &state_hash,
        },
    )?;
    if transaction.execute(
        "UPDATE channel_model_dispatch
         SET status_json = 'started', encrypted_blob = ?1, blob_hash = ?2
         WHERE entity_id = ?3 AND status_json = 'queued' AND suggestion_id IS NULL",
        params![blob, state_hash, entity_id],
    )? != 1
    {
        return Err(StoreError::ChannelObservationConflict);
    }
    Ok(())
}

fn update_channel_model_failed(
    transaction: &Transaction<'_>,
    authority: &LocalAuthority,
    stored: &StoredChannelModelDispatch,
    observed_at_ms: i64,
) -> Result<(String, AuditAnchor), StoreError> {
    if stored.state != StoredChannelModelState::Failed || stored.suggestion.is_some() {
        return Err(StoreError::ChannelObservationConflict);
    }
    let entity_id = channel_observation_entity(stored.channel, &stored.source_message_id);
    let blob = authority.encrypt_json(stored, channel_model_aad(&entity_id).as_bytes())?;
    let state_hash = blob_hash(&blob);
    let anchor = append_audit(
        transaction,
        authority,
        &AuditRecord {
            id: &format!("{entity_id}:model-failed"),
            command_id: &format!("{entity_id}-model-failed"),
            command_hash: &state_hash,
            actor: "openopen-model-runtime",
            action: CHANNEL_MODEL_FAILED_ACTION,
            entity_id: &entity_id,
            created_at_ms: observed_at_ms,
            state_kind: "channelModelFailed",
            state_hash: &state_hash,
        },
    )?;
    if transaction.execute(
        "UPDATE channel_model_dispatch
         SET status_json = 'failed', encrypted_blob = ?1, blob_hash = ?2
         WHERE entity_id = ?3 AND status_json = 'started' AND suggestion_id IS NULL",
        params![blob, state_hash, entity_id],
    )? != 1
    {
        return Err(StoreError::ChannelObservationConflict);
    }
    Ok((state_hash, anchor))
}

fn update_channel_model_suggestion(
    transaction: &Transaction<'_>,
    authority: &LocalAuthority,
    stored: &StoredChannelModelDispatch,
    observed_at_ms: i64,
) -> Result<(), StoreError> {
    let suggestion = stored
        .suggestion
        .as_ref()
        .ok_or(StoreError::ChannelObservationConflict)?;
    if stored.state != StoredChannelModelState::Ready {
        return Err(StoreError::ChannelObservationConflict);
    }
    let entity_id = channel_observation_entity(stored.channel, &stored.source_message_id);
    let blob = authority.encrypt_json(stored, channel_model_aad(&entity_id).as_bytes())?;
    let state_hash = blob_hash(&blob);
    append_audit(
        transaction,
        authority,
        &AuditRecord {
            id: &format!("{entity_id}:suggestion-ready"),
            command_id: &format!("{entity_id}-suggestion-ready"),
            command_hash: &state_hash,
            actor: "openopen-local-owner",
            action: CHANNEL_SUGGESTION_ACTION,
            entity_id: &entity_id,
            created_at_ms: observed_at_ms,
            state_kind: "channelSuggestionReady",
            state_hash: &state_hash,
        },
    )?;
    if transaction.execute(
        "UPDATE channel_model_dispatch
         SET status_json = 'ready', suggestion_id = ?1,
             encrypted_blob = ?2, blob_hash = ?3
         WHERE entity_id = ?4 AND status_json = 'started' AND suggestion_id IS NULL",
        params![suggestion.id, blob, state_hash, entity_id],
    )? != 1
    {
        return Err(StoreError::ChannelObservationConflict);
    }
    Ok(())
}

fn valid_channel_suggestion(suggestion: &OutcomeSuggestion) -> bool {
    !suggestion.id.is_empty()
        && suggestion.id.len() <= 256
        && !suggestion.title.trim().is_empty()
        && suggestion.title.trim() == suggestion.title
        && suggestion.title.len() <= 1_024
        && !suggestion.why_now.trim().is_empty()
        && suggestion.why_now.trim() == suggestion.why_now
        && suggestion.why_now.len() <= 4_096
        && !suggestion.proposed_steps.is_empty()
        && suggestion.proposed_steps.len() <= 16
        && suggestion
            .proposed_steps
            .iter()
            .all(|step| !step.trim().is_empty() && step.trim() == step && step.len() <= 1_024)
        && suggestion.source_refs.len() <= 32
        && suggestion
            .source_refs
            .iter()
            .all(|value| !value.trim().is_empty() && value.trim() == value && value.len() <= 2_048)
}

fn explicit_channel_correction(content: &str) -> bool {
    let Some(prefix) = content.get(..CHANNEL_CORRECTION_PREFIX.len()) else {
        return false;
    };
    prefix.eq_ignore_ascii_case(CHANNEL_CORRECTION_PREFIX)
        && content
            .get(CHANNEL_CORRECTION_PREFIX.len()..)
            .is_some_and(|remainder| !remainder.trim().is_empty())
}

fn qualified_channel_correction_predecessor(
    connection: &Connection,
    authority: &LocalAuthority,
    channel: ChannelKind,
    current: &StoredChannelObservation,
) -> Result<Option<String>, StoreError> {
    let encoded_channel = channel_json(channel)?;
    let accepted_decision = serde_json::to_string(&ChannelInboundDecision::Accepted)
        .map_err(|error| CryptoError::Serialization(error.to_string()))?;
    let prior_source_id = connection
        .query_row(
            "SELECT source_message_id
             FROM channel_observation
             WHERE channel_json = ?1
               AND conversation_id = ?2
               AND decision_json = ?3
               AND cursor_order < ?4
             ORDER BY cursor_order DESC, source_message_id DESC
             LIMIT 1",
            params![
                encoded_channel,
                current.observation.envelope.conversation_id,
                accepted_decision,
                i64::try_from(current.observation.cursor.order)
                    .map_err(|_| StoreError::ChannelObservationConflict)?,
            ],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    let Some(prior_source_id) = prior_source_id else {
        return Ok(None);
    };
    let prior_dispatch =
        load_channel_model_dispatch(connection, authority, channel, &prior_source_id)?
            .ok_or(StoreError::ChannelObservationConflict)?;
    if prior_dispatch.state == StoredChannelModelState::Failed {
        return Ok(Some(prior_source_id));
    }
    if prior_dispatch.state != StoredChannelModelState::Ready {
        return Ok(None);
    }
    let current_entity =
        channel_observation_entity(channel, &current.observation.envelope.source_message_id);
    let prior_entity = channel_observation_entity(channel, &prior_source_id);
    let audit_order_qualifies = connection.query_row(
        "SELECT EXISTS(
             SELECT 1
             FROM audit_ledger AS suggestion_audit
             JOIN audit_ledger AS current_observation_audit
               ON current_observation_audit.entity_id = ?1
              AND current_observation_audit.action = ?2
             WHERE suggestion_audit.entity_id = ?3
               AND suggestion_audit.action = ?4
               AND suggestion_audit.sequence > current_observation_audit.sequence
         )",
        params![
            current_entity,
            CHANNEL_OBSERVATION_ACTION,
            prior_entity,
            CHANNEL_SUGGESTION_ACTION,
        ],
        |row| row.get::<_, bool>(0),
    )?;
    Ok(audit_order_qualifies.then_some(prior_source_id))
}

fn primary_channel_route_set(
    mission: &Mission,
    pairing: &ChannelPairing,
    envelope: &ChannelEnvelope,
    bound_at_ms: i64,
) -> Result<ChannelRouteSet, StoreError> {
    let approval_id = mission
        .approvals
        .iter()
        .find(|approval| {
            approval.kind == ApprovalKind::MissionScope
                && approval.status == ApprovalStatus::Approved
                && approval.decided_by_id.as_deref() == Some(mission.owner_id.as_str())
        })
        .map(|approval| approval.id.clone())
        .ok_or(StoreError::ChannelRouteConflict)?;
    let route_id = channel_route_id(
        &mission.id,
        envelope.channel,
        &envelope.conversation_id,
        &envelope.sender_id,
    );
    Ok(ChannelRouteSet {
        mission_id: mission.id.clone(),
        revision: 1,
        primary_route_id: route_id.clone(),
        routes: vec![ChannelRoute {
            route_id,
            role: ChannelRouteRole::Primary,
            channel: envelope.channel,
            conversation_id: envelope.conversation_id.clone(),
            owner_sender_id: envelope.sender_id.clone(),
            provider_identity: pairing_provider_identity(pairing),
            source_message_id: Some(envelope.source_message_id.clone()),
            allowed_inbound_classes: vec![
                ChannelInboundMessageClass::MissionParticipation,
                ChannelInboundMessageClass::NeedYouResponse,
            ],
            allowed_outbound_classes: vec![
                ChannelMessageKind::NeedYou,
                ChannelMessageKind::Progress,
                ChannelMessageKind::Receipt,
            ],
            revision: 1,
            approval_id,
            audit_id: format!("channel-route-{}-1", mission.id),
            bound_at_ms,
            updated_at_ms: bound_at_ms,
        }],
    })
}

fn write_channel_route_set(
    transaction: &Transaction<'_>,
    authority: &LocalAuthority,
    route_set: &ChannelRouteSet,
    actor: &str,
) -> Result<(), StoreError> {
    validate_route_set(route_set)?;
    if let Some(existing) = load_channel_route_set(transaction, authority, &route_set.mission_id)?
        && (route_set.revision != existing.revision.saturating_add(1)
            || route_set.primary_route_id != existing.primary_route_id
            || !route_set.routes.starts_with(&existing.routes)
            || route_set.routes.len() != existing.routes.len() + 1)
    {
        return Err(StoreError::ChannelRouteConflict);
    }
    let changed_route = route_set
        .routes
        .last()
        .ok_or(StoreError::ChannelRouteConflict)?;
    let blob = authority.encrypt_json(
        route_set,
        channel_route_set_aad(&route_set.mission_id).as_bytes(),
    )?;
    let state_hash = blob_hash(&blob);
    append_audit(
        transaction,
        authority,
        &AuditRecord {
            id: &changed_route.audit_id,
            command_id: &changed_route.approval_id,
            command_hash: &state_hash,
            actor,
            action: CHANNEL_ROUTE_SET_ACTION,
            entity_id: &route_set.mission_id,
            created_at_ms: changed_route.updated_at_ms,
            state_kind: "channelRouteSet",
            state_hash: &state_hash,
        },
    )?;
    transaction.execute(
        "INSERT INTO channel_route_set
            (mission_id, revision, primary_route_id, encrypted_blob, blob_hash)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(mission_id) DO UPDATE SET
            revision = excluded.revision,
            primary_route_id = excluded.primary_route_id,
            encrypted_blob = excluded.encrypted_blob,
            blob_hash = excluded.blob_hash",
        params![
            route_set.mission_id,
            i64::try_from(route_set.revision).map_err(|_| StoreError::ChannelRouteConflict)?,
            route_set.primary_route_id,
            blob,
            state_hash,
        ],
    )?;
    Ok(())
}

fn load_channel_route_set(
    connection: &Connection,
    authority: &LocalAuthority,
    mission_id: &str,
) -> Result<Option<ChannelRouteSet>, StoreError> {
    let row = connection
        .query_row(
            "SELECT revision, primary_route_id, encrypted_blob, blob_hash
             FROM channel_route_set WHERE mission_id = ?1",
            [mission_id],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                    row.get::<_, String>(3)?,
                ))
            },
        )
        .optional()?;
    row.map(|(revision, primary_route_id, blob, stored_hash)| {
        let mismatch = || StoreError::ChannelStateMismatch(mission_id.to_owned());
        if blob_hash(&blob) != stored_hash {
            return Err(mismatch());
        }
        verify_blob_binding(
            connection,
            CHANNEL_ROUTE_SET_ACTION,
            mission_id,
            "channelRouteSet",
            &blob,
        )
        .map_err(|_| mismatch())?;
        let route_set: ChannelRouteSet = authority
            .decrypt_json(&blob, channel_route_set_aad(mission_id).as_bytes())
            .map_err(|_| mismatch())?;
        validate_route_set(&route_set).map_err(|_| mismatch())?;
        if route_set.mission_id != mission_id
            || i64::try_from(route_set.revision).map_err(|_| mismatch())? != revision
            || route_set.primary_route_id != primary_route_id
        {
            return Err(mismatch());
        }
        if load_mission_for_update(connection, authority, mission_id)?.is_none() {
            return Err(mismatch());
        }
        for route in &route_set.routes {
            let pairing = load_channel_pairing(connection, authority, route.channel)?
                .ok_or_else(&mismatch)?;
            if pairing.conversation_id != route.conversation_id
                || pairing.owner_sender_id != route.owner_sender_id
                || pairing_provider_identity(&pairing) != route.provider_identity
            {
                return Err(mismatch());
            }
        }
        Ok(route_set)
    })
    .transpose()
}

fn load_legacy_channel_origin(
    connection: &Connection,
    authority: &LocalAuthority,
    mission_id: &str,
) -> Result<Option<LegacyChannelMissionOrigin>, StoreError> {
    let row = connection
        .query_row(
            "SELECT channel_json, conversation_id, source_message_id, encrypted_blob, blob_hash
             FROM channel_mission_origin WHERE mission_id = ?1",
            [mission_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Vec<u8>>(3)?,
                    row.get::<_, String>(4)?,
                ))
            },
        )
        .optional()?;
    row.map(
        |(channel, conversation, source_message, blob, stored_hash)| {
            let mismatch = || StoreError::ChannelStateMismatch(mission_id.to_owned());
            if blob_hash(&blob) != stored_hash {
                return Err(mismatch());
            }
            verify_blob_binding(
                connection,
                LEGACY_CHANNEL_ORIGIN_ACTION,
                mission_id,
                "channelMissionOrigin",
                &blob,
            )
            .map_err(|_| mismatch())?;
            let origin: LegacyChannelMissionOrigin = authority
                .decrypt_json(&blob, channel_origin_aad(mission_id).as_bytes())
                .map_err(|_| mismatch())?;
            if origin.mission_id != mission_id
                || channel_json(origin.channel).map_err(|_| mismatch())? != channel
                || origin.conversation_id != conversation
                || origin.source_message_id != source_message
                || origin.bound_at_ms < 0
            {
                return Err(mismatch());
            }
            Ok(origin)
        },
    )
    .transpose()
}

fn write_channel_mission_event(
    transaction: &Transaction<'_>,
    authority: &LocalAuthority,
    event: &ChannelMissionEvent,
) -> Result<(), StoreError> {
    validate_mission_event(event)?;
    if load_channel_mission_event(
        transaction,
        authority,
        event.channel,
        &event.source_message_id,
    )?
    .is_some()
    {
        return Err(StoreError::ChannelObservationConflict);
    }
    let blob =
        authority.encrypt_json(event, channel_mission_event_aad(&event.event_id).as_bytes())?;
    let state_hash = blob_hash(&blob);
    append_audit(
        transaction,
        authority,
        &AuditRecord {
            id: &format!("{}:audit", event.event_id),
            command_id: &event.event_id,
            command_hash: &state_hash,
            actor: "channel-adapter",
            action: CHANNEL_MISSION_EVENT_ACTION,
            entity_id: &event.event_id,
            created_at_ms: event.recorded_at_ms,
            state_kind: "channelMissionEvent",
            state_hash: &state_hash,
        },
    )?;
    transaction.execute(
        "INSERT INTO channel_mission_event
            (entity_id, mission_id, route_id, route_set_revision, mission_revision,
             channel_json, source_message_id, encrypted_blob, blob_hash)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            event.event_id,
            event.mission_id,
            event.route_id,
            i64::try_from(event.route_set_revision)
                .map_err(|_| StoreError::ChannelRouteConflict)?,
            event.mission_revision,
            channel_json(event.channel)?,
            event.source_message_id,
            blob,
            state_hash,
        ],
    )?;
    Ok(())
}

fn load_channel_mission_event(
    connection: &Connection,
    authority: &LocalAuthority,
    channel: ChannelKind,
    source_message_id: &str,
) -> Result<Option<ChannelMissionEvent>, StoreError> {
    let row = connection
        .query_row(
            "SELECT entity_id, mission_id, route_id, route_set_revision, mission_revision,
                    encrypted_blob, blob_hash
             FROM channel_mission_event
             WHERE channel_json = ?1 AND source_message_id = ?2",
            params![channel_json(channel)?, source_message_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, Vec<u8>>(5)?,
                    row.get::<_, String>(6)?,
                ))
            },
        )
        .optional()?;
    row.map(
        |(entity_id, mission_id, route_id, route_revision, mission_revision, blob, stored_hash)| {
            let mismatch = || StoreError::ChannelStateMismatch(entity_id.clone());
            if blob_hash(&blob) != stored_hash {
                return Err(mismatch());
            }
            verify_blob_binding(
                connection,
                CHANNEL_MISSION_EVENT_ACTION,
                &entity_id,
                "channelMissionEvent",
                &blob,
            )
            .map_err(|_| mismatch())?;
            let event: ChannelMissionEvent = authority
                .decrypt_json(&blob, channel_mission_event_aad(&entity_id).as_bytes())
                .map_err(|_| mismatch())?;
            validate_mission_event(&event).map_err(|_| mismatch())?;
            if event.event_id != entity_id
                || event.mission_id != mission_id
                || event.route_id != route_id
                || i64::try_from(event.route_set_revision).map_err(|_| mismatch())?
                    != route_revision
                || event.mission_revision != mission_revision
                || event.channel != channel
                || event.source_message_id != source_message_id
            {
                return Err(mismatch());
            }
            let route_set = load_channel_route_set(connection, authority, &event.mission_id)?
                .ok_or_else(mismatch)?;
            if event.route_set_revision > route_set.revision
                || !route_set.routes.iter().any(|route| {
                    route.route_id == event.route_id
                        && route.revision <= event.route_set_revision
                        && route.channel == event.channel
                })
            {
                return Err(mismatch());
            }
            let mission_binding: Option<String> = connection
                .query_row(
                    "SELECT entry_hash FROM audit_ledger
                     WHERE sequence = ?1 AND action = ?2 AND entity_id = ?3",
                    params![
                        event.mission_revision,
                        MISSION_COMMAND_ACTION,
                        event.mission_id
                    ],
                    |row| row.get(0),
                )
                .optional()?;
            if mission_binding.as_deref() != Some(event.mission_anchor_hash.as_str()) {
                return Err(mismatch());
            }
            let observation = load_channel_observation(
                connection,
                authority,
                event.channel,
                &event.source_message_id,
            )?
            .ok_or_else(mismatch)?;
            if observation.decision != ChannelInboundDecision::AcceptedMissionUpdate
                || observation.observation.envelope.content_sha256 != event.content_sha256
            {
                return Err(mismatch());
            }
            Ok(event)
        },
    )
    .transpose()
}

fn load_channel_outbound(
    connection: &Connection,
    authority: &LocalAuthority,
    outbound_id: &str,
) -> Result<Option<StoredChannelOutbound>, StoreError> {
    let row = connection
        .query_row(
            "SELECT mission_id, channel_json, conversation_id, content_sha256,
                    status_json, provider_message_id, encrypted_blob, blob_hash
             FROM channel_outbound WHERE outbound_id = ?1",
            [outbound_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, Vec<u8>>(6)?,
                    row.get::<_, String>(7)?,
                ))
            },
        )
        .optional()?;
    row.map(
        |(mission, channel, conversation, content_hash, status, provider, blob, stored_hash)| {
            let mismatch = || StoreError::ChannelStateMismatch(outbound_id.to_owned());
            if blob_hash(&blob) != stored_hash {
                return Err(mismatch());
            }
            let (action, state_kind) = match status.as_str() {
                "started" if provider.is_none() => {
                    (CHANNEL_OUTBOUND_ACTION, "channelOutboundStarted")
                }
                "delivered" if provider.is_some() => {
                    (CHANNEL_DELIVERY_ACTION, "channelOutboundDelivered")
                }
                _ => return Err(mismatch()),
            };
            verify_blob_binding(connection, action, outbound_id, state_kind, &blob)
                .map_err(|_| mismatch())?;
            let stored: StoredChannelOutbound = authority
                .decrypt_json(&blob, channel_outbound_aad(outbound_id).as_bytes())
                .map_err(|_| mismatch())?;
            validate_outbound(&stored.intent).map_err(|_| mismatch())?;
            if let Some(delivery) = &stored.delivery {
                validate_delivery(delivery).map_err(|_| mismatch())?;
            }
            if stored.intent.outbound_id != outbound_id
                || stored.intent.mission_id != mission
                || channel_json(stored.intent.channel).map_err(|_| mismatch())? != channel
                || stored.intent.conversation_id != conversation
                || stored.intent.content_sha256 != content_hash
                || stored
                    .delivery
                    .as_ref()
                    .map(|value| &value.provider_message_id)
                    != provider.as_ref()
                || (stored.delivery.is_some()) != (status == "delivered")
            {
                return Err(mismatch());
            }
            Ok(stored)
        },
    )
    .transpose()
}

fn write_channel_outbound(
    transaction: &Transaction<'_>,
    authority: &LocalAuthority,
    stored: &StoredChannelOutbound,
) -> Result<(), StoreError> {
    let intent = &stored.intent;
    let blob =
        authority.encrypt_json(stored, channel_outbound_aad(&intent.outbound_id).as_bytes())?;
    let state_hash = blob_hash(&blob);
    append_audit(
        transaction,
        authority,
        &AuditRecord {
            id: &format!("channel:{}:started", intent.outbound_id),
            command_id: &intent.outbound_id,
            command_hash: &state_hash,
            actor: &intent.recipient_id,
            action: CHANNEL_OUTBOUND_ACTION,
            entity_id: &intent.outbound_id,
            created_at_ms: intent.created_at_ms,
            state_kind: "channelOutboundStarted",
            state_hash: &state_hash,
        },
    )?;
    transaction.execute(
        "INSERT INTO channel_outbound
            (outbound_id, mission_id, channel_json, conversation_id, content_sha256,
             status_json, provider_message_id, encrypted_blob, blob_hash)
         VALUES (?1, ?2, ?3, ?4, ?5, 'started', NULL, ?6, ?7)",
        params![
            intent.outbound_id,
            intent.mission_id,
            channel_json(intent.channel)?,
            intent.conversation_id,
            intent.content_sha256,
            blob,
            state_hash,
        ],
    )?;
    Ok(())
}

fn update_channel_outbound_delivery(
    transaction: &Transaction<'_>,
    authority: &LocalAuthority,
    stored: &StoredChannelOutbound,
) -> Result<(), StoreError> {
    let delivery = stored
        .delivery
        .as_ref()
        .ok_or(StoreError::ChannelOutboundConflict)?;
    let blob = authority.encrypt_json(
        stored,
        channel_outbound_aad(&stored.intent.outbound_id).as_bytes(),
    )?;
    let state_hash = blob_hash(&blob);
    append_audit(
        transaction,
        authority,
        &AuditRecord {
            id: &format!("channel:{}:delivered", stored.intent.outbound_id),
            command_id: &stored.intent.outbound_id,
            command_hash: &state_hash,
            actor: "channel-adapter",
            action: CHANNEL_DELIVERY_ACTION,
            entity_id: &stored.intent.outbound_id,
            created_at_ms: delivery.delivered_at_ms,
            state_kind: "channelOutboundDelivered",
            state_hash: &state_hash,
        },
    )?;
    if transaction.execute(
        "UPDATE channel_outbound
         SET status_json = 'delivered', provider_message_id = ?1,
             encrypted_blob = ?2, blob_hash = ?3
         WHERE outbound_id = ?4 AND status_json = 'started' AND provider_message_id IS NULL",
        params![
            delivery.provider_message_id,
            blob,
            state_hash,
            stored.intent.outbound_id,
        ],
    )? != 1
    {
        return Err(StoreError::ChannelOutboundConflict);
    }
    Ok(())
}

enum InboundRouteResolution {
    Unbound,
    Active(Box<ActiveInboundRoute>),
}

struct ActiveInboundRoute {
    route_set: ChannelRouteSet,
    route: ChannelRoute,
    mission: Mission,
    mission_anchor: AuditAnchor,
}

fn ignored_channel_inbound(
    decision: ChannelInboundDecision,
    cursor: ChannelCursor,
) -> ChannelInboundResult {
    ChannelInboundResult {
        decision,
        cursor,
        mission_event: None,
    }
}

fn validate_channel_content(
    observation: &ChannelObservation,
    content: &str,
) -> Result<(), StoreError> {
    if content.is_empty()
        || content.trim() != content
        || content.len() > MAX_CHANNEL_CONTENT_BYTES
        || content.as_bytes().contains(&0)
        || format!("{:x}", Sha256::digest(content.as_bytes()))
            != observation.envelope.content_sha256
    {
        return Err(StoreError::ChannelObservationConflict);
    }
    Ok(())
}

fn classify_mission_bound_inbound(
    connection: &Connection,
    authority: &LocalAuthority,
    observation: &ChannelObservation,
) -> Result<(ChannelInboundDecision, Option<ChannelMissionEvent>), StoreError> {
    let active = match resolve_channel_route_for_inbound(
        connection,
        authority,
        observation.envelope.channel,
        &observation.envelope.conversation_id,
        &observation.envelope.sender_id,
    )? {
        InboundRouteResolution::Unbound => {
            return Ok((ChannelInboundDecision::Accepted, None));
        }
        InboundRouteResolution::Active(active) => active,
    };
    let message_class = if active.mission.status == openopen_protocol::MissionStatus::NeedsMe {
        ChannelInboundMessageClass::NeedYouResponse
    } else {
        ChannelInboundMessageClass::MissionParticipation
    };
    if !active
        .route
        .allowed_inbound_classes
        .contains(&message_class)
    {
        return Ok((ChannelInboundDecision::IgnoredMessageClass, None));
    }
    Ok((
        ChannelInboundDecision::AcceptedMissionUpdate,
        Some(ChannelMissionEvent {
            event_id: channel_mission_event_id(
                observation.envelope.channel,
                &observation.envelope.source_message_id,
            ),
            mission_id: active.mission.id,
            mission_revision: active.mission_anchor.sequence,
            mission_anchor_hash: active.mission_anchor.entry_hash,
            route_id: active.route.route_id,
            route_set_revision: active.route_set.revision,
            message_class,
            channel: observation.envelope.channel,
            source_message_id: observation.envelope.source_message_id.clone(),
            content_sha256: observation.envelope.content_sha256.clone(),
            recorded_at_ms: observation.envelope.received_at_ms,
        }),
    ))
}

fn classify_channel_inbound(
    connection: &Connection,
    authority: &LocalAuthority,
    pairing: &ChannelPairing,
    observation: &ChannelObservation,
) -> Result<(ChannelInboundDecision, Option<ChannelMissionEvent>), StoreError> {
    if observation.is_bot {
        return Ok((ChannelInboundDecision::IgnoredBot, None));
    }
    if observation.envelope.sender_id != pairing.owner_sender_id {
        return Ok((ChannelInboundDecision::IgnoredSender, None));
    }
    if pairing.require_explicit_address && !observation.explicitly_addressed {
        return Ok((ChannelInboundDecision::IgnoredNotAddressed, None));
    }
    classify_mission_bound_inbound(connection, authority, observation)
}

fn resolve_channel_route_for_inbound(
    connection: &Connection,
    authority: &LocalAuthority,
    channel: ChannelKind,
    conversation_id: &str,
    owner_sender_id: &str,
) -> Result<InboundRouteResolution, StoreError> {
    let mission_ids = connection
        .prepare("SELECT mission_id FROM channel_route_set ORDER BY mission_id")?
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    let mut active = None;
    for mission_id in mission_ids {
        let route_set = load_channel_route_set(connection, authority, &mission_id)?
            .ok_or_else(|| StoreError::ChannelStateMismatch(mission_id.clone()))?;
        let Some(route) = route_set
            .routes
            .iter()
            .find(|route| {
                route.channel == channel
                    && route.conversation_id == conversation_id
                    && route.owner_sender_id == owner_sender_id
            })
            .cloned()
        else {
            continue;
        };
        let mission = load_mission_for_update(connection, authority, &mission_id)?
            .ok_or(StoreError::MissionNotFound)?;
        if mission.status.is_terminal() {
            continue;
        }
        if active.is_some() {
            return Err(StoreError::ChannelRouteConflict);
        }
        let mission_anchor = mission_audit_anchor(connection, &mission_id)?;
        active = Some(InboundRouteResolution::Active(Box::new(
            ActiveInboundRoute {
                route_set,
                route,
                mission,
                mission_anchor,
            },
        )));
    }
    Ok(active.unwrap_or(InboundRouteResolution::Unbound))
}

fn require_route_boundary_available(
    connection: &Connection,
    authority: &LocalAuthority,
    mission_id: &str,
    channel: ChannelKind,
    conversation_id: &str,
    owner_sender_id: &str,
) -> Result<(), StoreError> {
    match resolve_channel_route_for_inbound(
        connection,
        authority,
        channel,
        conversation_id,
        owner_sender_id,
    )? {
        InboundRouteResolution::Active(active) if active.mission.id != mission_id => {
            Err(StoreError::ChannelRouteConflict)
        }
        _ => Ok(()),
    }
}

fn mission_audit_anchor(
    connection: &Connection,
    mission_id: &str,
) -> Result<AuditAnchor, StoreError> {
    connection
        .query_row(
            "SELECT sequence, entry_hash, signature_hex FROM audit_ledger
             WHERE action = ?1 AND entity_id = ?2 ORDER BY sequence DESC LIMIT 1",
            params![MISSION_COMMAND_ACTION, mission_id],
            |row| {
                Ok(AuditAnchor {
                    sequence: row.get(0)?,
                    entry_hash: row.get(1)?,
                    signature_hex: row.get(2)?,
                })
            },
        )
        .optional()?
        .ok_or(StoreError::MissionStateMismatch)
}

fn channel_route_id(
    mission_id: &str,
    channel: ChannelKind,
    conversation_id: &str,
    owner_sender_id: &str,
) -> String {
    format!(
        "channel-route-{:x}",
        Sha256::digest(
            serde_json::to_vec(&serde_json::json!({
                "channel": channel,
                "conversationId": conversation_id,
                "missionId": mission_id,
                "ownerSenderId": owner_sender_id,
                "version": 1,
            }))
            .expect("channel route identity fields are infallibly serializable")
        )
    )
}

fn channel_mission_event_id(channel: ChannelKind, source_message_id: &str) -> String {
    format!(
        "channel-event-{:x}",
        Sha256::digest(
            serde_json::to_vec(&serde_json::json!({
                "channel": channel,
                "sourceMessageId": source_message_id,
                "version": 1,
            }))
            .expect("channel event identity fields are infallibly serializable")
        )
    )
}

fn pairing_provider_identity(pairing: &ChannelPairing) -> Option<String> {
    pairing
        .discord
        .as_ref()
        .map(|discord| discord.application_id.clone())
}

fn pairing_source_identity(pairing: &ChannelPairing) -> Option<String> {
    pairing
        .discord
        .as_ref()
        .map(|discord| discord.setup_source_message_id.clone())
}

fn channel_pairing_aad(channel_json: &str) -> String {
    format!("openopen:channel-pairing:v1:{channel_json}")
}

fn choice_model_selection_aad() -> &'static str {
    "openopen:choice-model-selection:v1"
}

fn choice_loop_state_aad() -> &'static str {
    "openopen:choice-loop-state:v1"
}

fn choice_idle_clock_aad() -> &'static str {
    "openopen:choice-idle-clock-anchor:v1"
}

fn choice_begin_request_aad(
    request_id: &str,
    choice_session_id: &str,
    operation_id: &str,
    source_envelope_id: &str,
    conversation_turn_batch_id: &str,
    accepted_at_ms: i64,
) -> String {
    let accepted_at_ms = accepted_at_ms.to_string();
    choice_identity_aad(
        "openopen:choice-begin-request:v3",
        &[
            request_id,
            choice_session_id,
            operation_id,
            source_envelope_id,
            conversation_turn_batch_id,
            &accepted_at_ms,
        ],
    )
}

fn choice_d_request_aad(
    request_id: &str,
    choice_session_id: &str,
    operation_id: &str,
    source_envelope_id: &str,
    conversation_turn_batch_id: &str,
    accepted_at_ms: i64,
) -> String {
    let accepted_at_ms = accepted_at_ms.to_string();
    choice_identity_aad(
        "openopen:choice-d-request:v3",
        &[
            request_id,
            choice_session_id,
            operation_id,
            source_envelope_id,
            conversation_turn_batch_id,
            &accepted_at_ms,
        ],
    )
}

fn choice_refinement_context_aad(
    operation_id: &str,
    selection_id: &str,
    choice_session_id: &str,
    source_envelope_id: &str,
    conversation_turn_batch_id: &str,
    created_at_ms: i64,
) -> String {
    let created_at_ms = created_at_ms.to_string();
    choice_identity_aad(
        "openopen:choice-refinement-context:v3",
        &[
            operation_id,
            selection_id,
            choice_session_id,
            source_envelope_id,
            conversation_turn_batch_id,
            &created_at_ms,
        ],
    )
}

fn choice_refinement_result_aad(
    operation_id: &str,
    selection_id: &str,
    choice_session_id: &str,
    source_envelope_id: &str,
    conversation_turn_batch_id: &str,
    result_digest: &str,
    completed_at_ms: i64,
) -> String {
    let completed_at_ms = completed_at_ms.to_string();
    choice_identity_aad(
        "openopen:choice-refinement-result:v4",
        &[
            operation_id,
            selection_id,
            choice_session_id,
            source_envelope_id,
            conversation_turn_batch_id,
            result_digest,
            &completed_at_ms,
        ],
    )
}

fn choice_identity_aad(domain: &str, fields: &[&str]) -> String {
    let mut aad = String::from(domain);
    for field in fields {
        aad.push('|');
        aad.push_str(&field.len().to_string());
        aad.push(':');
        aad.push_str(field);
    }
    aad
}

fn choice_private_body_retirement_aad(source_kind: &str, entity_id: &str) -> String {
    format!("openopen:choice-private-body-retirement:v1:{source_kind}:{entity_id}")
}

fn markdown_render_intent_aad(intent_id: &str, intent_digest: &str) -> String {
    format!("openopen:choice-markdown-render-intent:v2:{intent_id}:{intent_digest}")
}

fn markdown_render_receipt_aad(intent_id: &str, intent_digest: &str) -> String {
    format!("openopen:choice-markdown-render-receipt:v2:{intent_id}:{intent_digest}")
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn channel_observation_aad(entity_id: &str) -> String {
    format!("openopen:channel-observation:v1:{entity_id}")
}

fn channel_cursor_aad(entity_id: &str) -> String {
    format!("openopen:channel-cursor:v1:{entity_id}")
}

fn channel_model_aad(entity_id: &str) -> String {
    format!("openopen:channel-model:v1:{entity_id}")
}

fn channel_failure_incident_aad(incident_id: &str) -> String {
    format!("openopen:channel-failure-incident:v1:{incident_id}")
}

fn channel_origin_aad(mission_id: &str) -> String {
    format!("openopen:channel-origin:v1:{mission_id}")
}

fn channel_route_set_aad(mission_id: &str) -> String {
    format!("openopen:channel-route-set:v1:{mission_id}")
}

fn channel_mission_event_aad(event_id: &str) -> String {
    format!("openopen:channel-mission-event:v1:{event_id}")
}

fn channel_outbound_aad(outbound_id: &str) -> String {
    format!("openopen:channel-outbound:v1:{outbound_id}")
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

#[cfg(test)]
mod correction_directive_tests {
    use super::explicit_channel_correction;

    #[test]
    fn only_an_explicit_nonempty_previous_message_correction_is_authoritative() {
        assert!(explicit_channel_correction(
            "Correction to previous: prepare only the revised draft"
        ));
        assert!(explicit_channel_correction(
            "CORRECTION TO PREVIOUS: prepare only the revised draft"
        ));
        assert!(!explicit_channel_correction("Correction to previous:"));
        assert!(!explicit_channel_correction("correction: revise the draft"));
        assert!(!explicit_channel_correction("update: revise the draft"));
        assert!(!explicit_channel_correction("book dinner"));
        assert!(!explicit_channel_correction(
            "Please treat this as a correction to previous: book dinner"
        ));
    }
}

#[cfg(test)]
mod choice_model_selection_tests {
    use super::*;

    fn selection(model_id: &str, catalog_fingerprint: &str) -> ModelSelection {
        ModelSelection {
            id: format!("selection-{model_id}"),
            model_id: model_id.to_owned(),
            requested_effort: "not_applicable".to_owned(),
            actual_effort: "not_applicable".to_owned(),
            catalog_fingerprint: catalog_fingerprint.to_owned(),
            catalog_revision: 1,
            account_display_class: "chatgpt:test".to_owned(),
            protocol_schema_revision: 1,
        }
    }

    #[test]
    fn explicit_model_selection_is_audited_idempotent_and_tamper_evident() {
        let authority = LocalAuthority::from_master("choice-model-selection", [91_u8; 32]);
        let mut store = Store::open_in_memory(authority).expect("open store");
        let first = selection("gpt-test-a", &"a".repeat(64));

        assert_eq!(
            store
                .select_model_selection(&first, 1)
                .expect("persist first selection"),
            first
        );
        assert_eq!(
            store
                .selected_model_selection()
                .expect("load first selection"),
            Some(first.clone())
        );

        assert_eq!(
            store
                .select_model_selection(&first, 2)
                .expect("idempotent selection"),
            first
        );
        let audit_count: i64 = store
            .connection
            .query_row(
                "SELECT COUNT(*) FROM audit_ledger WHERE action = ?1",
                [CHOICE_MODEL_SELECTION_ACTION],
                |row| row.get(0),
            )
            .expect("audit count");
        assert_eq!(audit_count, 1);

        let second = selection("gpt-test-b", &"b".repeat(64));
        store
            .select_model_selection(&second, 3)
            .expect("persist changed selection");
        assert_eq!(
            store
                .selected_model_selection()
                .expect("load changed selection"),
            Some(second)
        );
        store
            .connection
            .execute(
                "UPDATE choice_model_selection SET blob_hash = ?1 WHERE singleton_id = 1",
                ["00".repeat(32)],
            )
            .expect("tamper selection hash");
        assert!(matches!(
            store.selected_model_selection(),
            Err(StoreError::ChoiceModelSelectionConflict)
        ));
    }
}

#[cfg(test)]
mod choice_begin_tests {
    use super::*;
    use crate::{BrokerEnrollmentRecord, broker_enrollment_signing_bytes};
    use ed25519_dalek::{Signer, SigningKey};
    use openopen_protocol::{
        BatchSealReason, ChoiceBeginAccepted, ChoiceBeginRecord, ChoiceDInput, ChoiceDIntakeRecord,
        ChoiceInitialResult, ChoiceLoopSnapshot, ChoiceOption, ChoiceResumeResult, ChoiceSession,
        ChoiceSessionState, ChoiceSet, ConversationTurnBatch, DocumentManifest,
        DocumentManifestEntry, EFFECT_PROTOCOL_VERSION, InterpretationFrame, MarkdownRenderIntent,
        MarkdownRenderReceipt, ModelProvenance, ModelSelection, ModelSelectionState,
        NaturalConversationSelection, OptionSelection, PersonaRevisionRef, RuntimeControlReceipt,
        SourceEnvelope, canonical_document_manifest_digest, runtime_control_authorization_hash,
        runtime_control_receipt_signing_bytes,
    };

    fn commit_runtime_revision(store: &mut Store, enabled: bool, updated_at_ms: i64) {
        let broker = SigningKey::from_bytes(&[94_u8; 32]);
        let broker_key = broker.verifying_key().to_bytes();
        let authorization = store
            .prepare_runtime_control(enabled, updated_at_ms)
            .unwrap();
        let mut receipt = RuntimeControlReceipt {
            protocol_version: EFFECT_PROTOCOL_VERSION,
            authorization_hash: runtime_control_authorization_hash(&authorization).unwrap(),
            checkpoint_nonce: "90".repeat(32),
            request_nonce: None,
            broker_key_id: format!("{:x}", Sha256::digest(broker_key)),
            broker_signature_hex: String::new(),
        };
        receipt.broker_signature_hex = hex::encode(
            broker
                .sign(&runtime_control_receipt_signing_bytes(&receipt).unwrap())
                .to_bytes(),
        );
        store
            .commit_runtime_control(&authorization, &receipt)
            .unwrap();
    }

    pub(super) fn enable_runtime(store: &mut Store, authority: &LocalAuthority) {
        let broker = SigningKey::from_bytes(&[94_u8; 32]);
        let broker_key = broker.verifying_key().to_bytes();
        let mut enrollment = BrokerEnrollmentRecord {
            version: 1,
            broker_key_id: format!("{:x}", Sha256::digest(broker_key)),
            broker_verifying_key_hex: hex::encode(broker_key),
            helper_designated_requirement_digest: "cd".repeat(32),
            installed_at_ms: 1,
            core_key_id: authority.effect_key_id(),
            core_authorization_signature_hex: String::new(),
        };
        let mut derivation = b"openopen-effect-authorizer-v1".to_vec();
        derivation.extend([93_u8; 32]);
        let core_signing_key = SigningKey::from_bytes(&Sha256::digest(derivation).into());
        enrollment.core_authorization_signature_hex = hex::encode(
            core_signing_key
                .sign(&broker_enrollment_signing_bytes(&enrollment).unwrap())
                .to_bytes(),
        );
        store.install_trusted_broker(&enrollment).unwrap();
        commit_runtime_revision(store, true, 1);
    }

    pub(super) fn selection() -> ModelSelection {
        ModelSelection {
            id: "model-selection-1".to_owned(),
            model_id: "gpt-test-model".to_owned(),
            requested_effort: "not_applicable".to_owned(),
            actual_effort: "not_applicable".to_owned(),
            catalog_fingerprint: "a".repeat(64),
            catalog_revision: 1,
            account_display_class: "chatgpt:test".to_owned(),
            protocol_schema_revision: 1,
        }
    }

    pub(super) fn persona_revision() -> PersonaRevisionRef {
        PersonaRevisionRef {
            persona_id: "openopen.nondev.default".to_owned(),
            revision: "draft-03-en".to_owned(),
            aggregate_digest: "f".repeat(64),
            instructions_digest: "e".repeat(64),
        }
    }

    pub(super) fn begin_state(
        question: &str,
        request_id: &str,
    ) -> (ChoiceBeginRecord, ChoiceLoopSnapshot) {
        let digest = format!("{:x}", Sha256::digest(question.as_bytes()));
        let manifest_entry = DocumentManifestEntry {
            relative_path: "sessions/session-1/SESSION.md".to_owned(),
            sha256: digest.clone(),
            byte_length: u64::try_from(question.len()).unwrap(),
            mode: 0o600,
        };
        let source_manifest = DocumentManifest {
            root_version: 1,
            entries: vec![manifest_entry],
            aggregate_digest: canonical_document_manifest_digest(&[DocumentManifestEntry {
                relative_path: "sessions/session-1/SESSION.md".to_owned(),
                sha256: digest.clone(),
                byte_length: u64::try_from(question.len()).unwrap(),
                mode: 0o600,
            }])
            .unwrap(),
            generated_at_ms: 10,
        };
        let source_envelope = SourceEnvelope {
            id: "source-1".to_owned(),
            surface: "mac".to_owned(),
            delivery_binding_id: "mac-local-owner".to_owned(),
            provider_message_id: None,
            owner_id: "openopen-local-owner".to_owned(),
            received_at_ms: 10,
            monotonic_sequence: 1,
            body_digest: digest,
            attachment_manifest: None,
            third_party_data: false,
            session_hint: None,
            schema_version: 1,
        };
        let batch = ConversationTurnBatch {
            id: "batch-1".to_owned(),
            choice_session_id: "session-1".to_owned(),
            delivery_binding_id: "mac-local-owner".to_owned(),
            source_envelope_ids: vec!["source-1".to_owned()],
            opened_at_ms: 10,
            quiet_deadline_ms: 2_510,
            hard_deadline_ms: 8_010,
            sealed_at_ms: Some(10),
            seal_reason: Some(BatchSealReason::InitialIntake),
            revision: 1,
        };
        let accepted = ChoiceBeginAccepted {
            request_id: request_id.to_owned(),
            operation_id: "operation-1".to_owned(),
            choice_session_id: "session-1".to_owned(),
            accepted_session_revision: 1,
            source_envelope_id: "source-1".to_owned(),
            conversation_turn_batch_id: "batch-1".to_owned(),
            state: ChoiceSessionState::Interpreting,
        };
        let record = ChoiceBeginRecord {
            accepted: accepted.clone(),
            request_digest: "b".repeat(64),
            bounded_local_question: question.to_owned(),
            source_envelope,
            batch: batch.clone(),
            model_selection: selection(),
            source_manifest: source_manifest.clone(),
            persona_revision: persona_revision(),
            runtime_revision: 1,
            accepted_at_ms: 10,
        };
        let snapshot = ChoiceLoopSnapshot {
            session: ChoiceSession {
                id: "session-1".to_owned(),
                state: ChoiceSessionState::Interpreting,
                revision: 1,
                model_selection_state: ModelSelectionState::Selected {
                    model_provenance_ref: "model-selection-1".to_owned(),
                },
                communication_profile_revision: 0,
                active_choice_set_id: None,
                active_interpretation_revision: None,
                opened_at_ms: 10,
                last_input_at_ms: 10,
                soft_idle_at_ms: 1_800_010,
                stale_review_at_ms: 86_400_010,
                primary_delivery_binding_id: Some("mac-local-owner".to_owned()),
                pending_confirmation_id: None,
                background_mission_ids: vec![],
            },
            active_batch: Some(batch),
            interpretation: None,
            active_choice_set: None,
            last_selection: None,
            pending_refinement_operation: None,
            confirmation: None,
            document_manifest: source_manifest,
        };
        (record, snapshot)
    }

    pub(super) fn initial_result(record: &ChoiceBeginRecord) -> ChoiceInitialResult {
        let provenance = ModelProvenance {
            id: "turn-provenance-1".to_owned(),
            model_id: record.model_selection.model_id.clone(),
            requested_effort: record.model_selection.requested_effort.clone(),
            actual_effort: record.model_selection.actual_effort.clone(),
            catalog_fingerprint: record.model_selection.catalog_fingerprint.clone(),
            catalog_revision: record.model_selection.catalog_revision,
            account_display_class: record.model_selection.account_display_class.clone(),
            protocol_schema_revision: record.model_selection.protocol_schema_revision,
            turn_id: "turn-1".to_owned(),
        };
        let interpretation = InterpretationFrame {
            choice_session_id: record.accepted.choice_session_id.clone(),
            revision: 1,
            understood_goal: "Plan one bounded task".to_owned(),
            current_context: "A local first question was accepted".to_owned(),
            assumptions: vec![],
            constraints: vec![],
            uncertainties: vec![],
            what_to_avoid: vec![],
            source_manifest_digest: record.source_manifest.aggregate_digest.clone(),
        };
        ChoiceInitialResult {
            operation_id: record.accepted.operation_id.clone(),
            expected_session_revision: record.accepted.accepted_session_revision,
            expected_generation: record.runtime_revision,
            model_provenance: provenance.clone(),
            source_manifest_digest: record.source_manifest.aggregate_digest.clone(),
            persona_revision: record.persona_revision.clone(),
            interpretation: interpretation.clone(),
            choice_set: ChoiceSet {
                id: "choice-set-1".to_owned(),
                choice_session_id: record.accepted.choice_session_id.clone(),
                session_revision: record.accepted.accepted_session_revision + 1,
                interpretation_revision: interpretation.revision,
                generated_at_ms: 11,
                expires_on_revision: record.accepted.accepted_session_revision + 1,
                options: vec![
                    ChoiceOption {
                        id: "option-1".to_owned(),
                        position: 1,
                        direction: "Review the first step".to_owned(),
                        rationale: "Keeps the work bounded".to_owned(),
                        expected_result: "A clear next step".to_owned(),
                        information_needed: vec![],
                        external_effects_preview: vec![],
                        source_categories: vec!["ownerInput".to_owned()],
                    },
                    ChoiceOption {
                        id: "option-2".to_owned(),
                        position: 2,
                        direction: "Narrow the task".to_owned(),
                        rationale: "Reduces uncertainty".to_owned(),
                        expected_result: "A smaller plan".to_owned(),
                        information_needed: vec![],
                        external_effects_preview: vec![],
                        source_categories: vec!["ownerInput".to_owned()],
                    },
                    ChoiceOption {
                        id: "option-3".to_owned(),
                        position: 3,
                        direction: "Prepare an alternative".to_owned(),
                        rationale: "Keeps one safe backup".to_owned(),
                        expected_result: "A bounded alternative".to_owned(),
                        information_needed: vec![],
                        external_effects_preview: vec![],
                        source_categories: vec!["ownerInput".to_owned()],
                    },
                ],
                d_available: true,
                source_manifest_digest: record.source_manifest.aggregate_digest.clone(),
                model_provenance: provenance,
                persona_revision: record.persona_revision.clone(),
            },
            completed_at_ms: 11,
        }
    }

    fn d_intake_record(active: &ChoiceLoopSnapshot) -> ChoiceDIntakeRecord {
        let choice_set = active
            .active_choice_set
            .as_ref()
            .expect("active choice set");
        let input = ChoiceDInput {
            request_id: "choice-d-request-1".to_owned(),
            bounded_text: "Refine the bounded local task".to_owned(),
            choice_session_id: active.session.id.clone(),
            choice_set_id: choice_set.id.clone(),
            expected_session_revision: active.session.revision,
            submitted_at_ms: 20,
        };
        let request_digest = input.request_digest().expect("valid D request digest");
        let source_envelope = SourceEnvelope {
            id: "choice-d-source-1".to_owned(),
            surface: "mac".to_owned(),
            delivery_binding_id: active
                .session
                .primary_delivery_binding_id
                .clone()
                .expect("bound local delivery"),
            provider_message_id: None,
            owner_id: "openopen-local-owner".to_owned(),
            received_at_ms: input.submitted_at_ms,
            monotonic_sequence: active.session.revision,
            body_digest: format!("{:x}", Sha256::digest(input.bounded_text.as_bytes())),
            attachment_manifest: None,
            third_party_data: false,
            session_hint: Some(active.session.id.clone()),
            schema_version: choice_set.model_provenance.protocol_schema_revision,
        };
        let batch = ConversationTurnBatch {
            id: "choice-d-batch-1".to_owned(),
            choice_session_id: active.session.id.clone(),
            delivery_binding_id: source_envelope.delivery_binding_id.clone(),
            source_envelope_ids: vec![source_envelope.id.clone()],
            opened_at_ms: input.submitted_at_ms,
            quiet_deadline_ms: input.submitted_at_ms + 2_500,
            hard_deadline_ms: input.submitted_at_ms + 8_000,
            sealed_at_ms: Some(input.submitted_at_ms),
            seal_reason: Some(BatchSealReason::ImmediateRefinement),
            revision: active.session.revision,
        };
        ChoiceDIntakeRecord {
            input: input.clone(),
            request_digest,
            source_envelope,
            batch: batch.clone(),
            selection: NaturalConversationSelection {
                id: "choice-d-selection-1".to_owned(),
                choice_session_id: input.choice_session_id.clone(),
                choice_set_id: input.choice_set_id.clone(),
                d_input_batch_id: batch.id,
                expected_session_revision: input.expected_session_revision,
                selected_at_ms: input.submitted_at_ms,
            },
        }
    }

    pub(super) fn refining_d_store(
        label: &str,
    ) -> (
        Store,
        ChoiceDIntakeRecord,
        ChoiceRefinementOperation,
        ChoiceLoopSnapshot,
        ChoiceSet,
    ) {
        let authority = LocalAuthority::from_master(label, [93_u8; 32]);
        let mut store = Store::open_in_memory(authority.clone()).expect("open Store");
        enable_runtime(&mut store, &authority);
        store
            .select_model_selection(&selection(), 1)
            .expect("persist model selection");
        let (begin, snapshot) = begin_state("Plan one bounded task", "request-private-row");
        store
            .begin_choice_session(&begin, &snapshot)
            .expect("begin Choice");
        let active = store
            .commit_initial_choice_result(&initial_result(&begin))
            .expect("activate Choice");
        let choices = active.active_choice_set.clone().expect("active choices");
        let d_record = d_intake_record(&active);
        let refining = store
            .commit_choice_d_selection(&d_record, 1)
            .expect("commit D intake");
        let operation = refining
            .pending_refinement_operation
            .clone()
            .expect("pending refinement");
        (store, d_record, operation, refining, choices)
    }

    pub(super) fn refinement_result_for(
        refining: &ChoiceLoopSnapshot,
        operation: &ChoiceRefinementOperation,
        choices: &ChoiceSet,
    ) -> ChoiceRefinementResult {
        let interpretation = InterpretationFrame {
            choice_session_id: refining.session.id.clone(),
            revision: refining.session.revision + 1,
            understood_goal: "Refined private task".to_owned(),
            current_context: "The owner supplied a bound direction".to_owned(),
            assumptions: vec![],
            constraints: vec![],
            uncertainties: vec![],
            what_to_avoid: vec![],
            source_manifest_digest: refining.document_manifest.aggregate_digest.clone(),
        };
        let mut result = ChoiceRefinementResult {
            operation_id: operation.id.clone(),
            selection_id: operation.selection_id.clone(),
            source_envelope_id: operation.source_envelope_id.clone(),
            conversation_turn_batch_id: operation.conversation_turn_batch_id.clone(),
            expected_session_revision: operation.expected_session_revision,
            expected_generation: operation.expected_generation,
            model_provenance: operation.model_provenance.clone(),
            source_manifest_digest: refining.document_manifest.aggregate_digest.clone(),
            persona_revision: operation.persona_revision.clone(),
            interpretation: interpretation.clone(),
            choice_set: ChoiceSet {
                id: "choices-private-refined".to_owned(),
                choice_session_id: refining.session.id.clone(),
                session_revision: refining.session.revision + 1,
                interpretation_revision: interpretation.revision,
                generated_at_ms: 21,
                expires_on_revision: refining.session.revision + 1,
                options: choices.options.clone(),
                d_available: true,
                source_manifest_digest: refining.document_manifest.aggregate_digest.clone(),
                model_provenance: operation.model_provenance.clone(),
                persona_revision: operation.persona_revision.clone(),
            },
            result_digest: String::new(),
            completed_at_ms: 21,
        };
        result.result_digest = result
            .canonical_result_digest()
            .expect("canonical refinement result");
        result
    }

    #[test]
    fn choice_begin_is_atomic_idempotent_and_rejects_changed_or_active_replays() {
        let authority = LocalAuthority::from_master("choice-begin", [93_u8; 32]);
        let mut store = Store::open_in_memory(authority.clone()).unwrap();
        enable_runtime(&mut store, &authority);
        store.select_model_selection(&selection(), 1).unwrap();
        let (record, snapshot) = begin_state("Plan one bounded task", "request-1");

        assert_eq!(
            store.begin_choice_session(&record, &snapshot).unwrap(),
            record.accepted
        );
        assert_eq!(
            store.begin_choice_session(&record, &snapshot).unwrap(),
            record.accepted
        );
        let begin_rows: i64 = store
            .connection
            .query_row("SELECT COUNT(*) FROM choice_begin_request", [], |row| {
                row.get(0)
            })
            .unwrap();
        let state_rows: i64 = store
            .connection
            .query_row("SELECT COUNT(*) FROM choice_loop_state", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!((begin_rows, state_rows), (1, 1));

        let (mut changed, changed_snapshot) = begin_state("Changed question", "request-1");
        changed.request_digest = "c".repeat(64);
        assert!(matches!(
            store.begin_choice_session(&changed, &changed_snapshot),
            Err(StoreError::ChoiceBeginConflict)
        ));
        let (other, other_snapshot) = begin_state("Another question", "request-2");
        assert!(matches!(
            store.begin_choice_session(&other, &other_snapshot),
            Err(StoreError::ChoiceBeginConflict)
        ));
        assert_eq!(
            store.choice_loop_snapshot().unwrap().unwrap().session.state,
            ChoiceSessionState::Interpreting
        );
    }

    #[test]
    #[allow(clippy::too_many_lines)] // One tamper corpus keeps every active private-row identity together.
    fn active_private_choice_rows_bind_every_clear_identity_and_fail_closed_on_tamper() {
        let root = tempfile::tempdir().expect("temporary Store root");
        let path = root.path().join("choice.sqlite3");
        let authority = LocalAuthority::from_master("choice-private-index-restart", [93_u8; 32]);
        let (begin, snapshot) = begin_state("Plan one bounded task", "request-index-restart");
        {
            let mut store = Store::open(&path, authority.clone()).expect("open persistent Store");
            enable_runtime(&mut store, &authority);
            store
                .select_model_selection(&selection(), 1)
                .expect("persist model selection");
            store
                .begin_choice_session(&begin, &snapshot)
                .expect("persist begin");
            store
                .connection
                .execute(
                    "UPDATE choice_begin_request SET source_envelope_id = 'tampered-envelope'
                     WHERE request_id = ?1",
                    [&begin.accepted.request_id],
                )
                .expect("tamper begin clear identity");
        }
        let reopened = Store::open(&path, authority).expect("reopen persistent Store");
        assert!(matches!(
            reopened.choice_begin_replay(&begin.accepted.request_id, &begin.request_digest),
            Err(StoreError::Crypto(_) | StoreError::ChoiceBeginConflict)
        ));
        assert!(
            reopened.choice_loop_snapshot().is_err(),
            "restart continuity must not present a healthy session over a tampered begin row"
        );

        let delete_authority =
            LocalAuthority::from_master("choice-private-row-delete", [93_u8; 32]);
        let mut deleted_store = Store::open_in_memory(delete_authority.clone())
            .expect("open private-row deletion Store");
        enable_runtime(&mut deleted_store, &delete_authority);
        deleted_store
            .select_model_selection(&selection(), 1)
            .expect("persist model selection");
        let (deleted_begin, deleted_snapshot) =
            begin_state("Plan one bounded task", "request-row-delete");
        deleted_store
            .begin_choice_session(&deleted_begin, &deleted_snapshot)
            .expect("persist audited begin row");
        deleted_store
            .connection
            .execute(
                "DELETE FROM choice_begin_request WHERE request_id = ?1",
                [&deleted_begin.accepted.request_id],
            )
            .expect("delete audited private row");
        assert!(
            deleted_store.choice_loop_snapshot().is_err(),
            "an audit-linked private row deletion must fail continuity closed"
        );

        let (mut d_store, _d_record, d_operation, _refining, _choices) =
            refining_d_store("choice-d-index-tamper");
        d_store
            .connection
            .execute(
                "UPDATE choice_d_request SET conversation_turn_batch_id = 'tampered-batch'
                 WHERE operation_id = ?1",
                [&d_operation.id],
            )
            .expect("tamper D clear identity");
        assert!(matches!(
            d_store.choice_d_intake_for_refinement(&d_operation),
            Err(StoreError::Crypto(_) | StoreError::ChoiceLoopStateConflict)
        ));
        assert!(d_store.choice_loop_snapshot().is_err());
        assert!(matches!(
            d_store.cancel_choice_session(1, 22),
            Err(StoreError::Crypto(_) | StoreError::ChoiceLoopStateConflict)
        ));
        let retained_d_rows: i64 = d_store
            .connection
            .query_row("SELECT COUNT(*) FROM choice_d_request", [], |row| {
                row.get(0)
            })
            .expect("D body remains until a valid retirement");
        let false_d_tombstones: i64 = d_store
            .connection
            .query_row(
                "SELECT COUNT(*) FROM choice_private_body_tombstone WHERE source_kind = 'd'",
                [],
                |row| row.get(0),
            )
            .expect("no false D retirement");
        assert_eq!((retained_d_rows, false_d_tombstones), (1, 0));

        let (context_store, _d_record, context_operation, _refining, _choices) =
            refining_d_store("choice-context-index-tamper");
        context_store
            .connection
            .execute(
                "UPDATE choice_refinement_context SET selection_id = 'tampered-selection'
                 WHERE operation_id = ?1",
                [&context_operation.id],
            )
            .expect("tamper context clear identity");
        assert!(matches!(
            context_store.choice_refinement_context(&context_operation),
            Err(StoreError::Crypto(_) | StoreError::ChoiceLoopStateConflict)
        ));
        assert!(context_store.choice_loop_snapshot().is_err());

        let (source_context_store, _d_record, source_context_operation, _refining, _choices) =
            refining_d_store("choice-context-source-index-tamper");
        source_context_store
            .connection
            .execute(
                "UPDATE choice_refinement_context SET source_envelope_id = 'tampered-envelope'
                 WHERE operation_id = ?1",
                [&source_context_operation.id],
            )
            .expect("tamper context source identity");
        assert!(matches!(
            source_context_store.choice_refinement_context(&source_context_operation),
            Err(StoreError::Crypto(_) | StoreError::ChoiceLoopStateConflict)
        ));
        assert!(source_context_store.choice_loop_snapshot().is_err());

        let (timing_store, _d_record, timing_operation, _refining, _choices) =
            refining_d_store("choice-context-time-tamper");
        timing_store
            .connection
            .execute(
                "UPDATE choice_refinement_context SET created_at_ms = created_at_ms + 1
                 WHERE operation_id = ?1",
                [&timing_operation.id],
            )
            .expect("tamper context timing index");
        assert!(matches!(
            timing_store.choice_refinement_context(&timing_operation),
            Err(StoreError::Crypto(_) | StoreError::ChoiceLoopStateConflict)
        ));
        assert!(timing_store.choice_loop_snapshot().is_err());

        let (mut result_store, _d_record, result_operation, refining, choices) =
            refining_d_store("choice-result-index-tamper");
        let result = refinement_result_for(&refining, &result_operation, &choices);
        result_store
            .commit_choice_refinement_result(&result)
            .expect("persist exact result");
        result_store
            .connection
            .execute(
                "UPDATE choice_refinement_result SET choice_session_id = 'tampered-session'
                 WHERE operation_id = ?1",
                [&result_operation.id],
            )
            .expect("tamper result clear identity");
        assert!(matches!(
            result_store.commit_choice_refinement_result(&result),
            Err(StoreError::Crypto(_) | StoreError::ChoiceLoopStateConflict)
        ));
        assert!(result_store.choice_loop_snapshot().is_err());

        let (mut source_result_store, _d_record, source_result_operation, refining, choices) =
            refining_d_store("choice-result-source-index-tamper");
        let result = refinement_result_for(&refining, &source_result_operation, &choices);
        source_result_store
            .commit_choice_refinement_result(&result)
            .expect("persist source-bound result");
        source_result_store
            .connection
            .execute(
                "UPDATE choice_refinement_result
                 SET conversation_turn_batch_id = 'tampered-batch' WHERE operation_id = ?1",
                [&source_result_operation.id],
            )
            .expect("tamper result batch identity");
        assert!(matches!(
            source_result_store.commit_choice_refinement_result(&result),
            Err(StoreError::Crypto(_) | StoreError::ChoiceLoopStateConflict)
        ));
        assert!(source_result_store.choice_loop_snapshot().is_err());
    }

    #[test]
    fn a_new_explicit_question_supersedes_only_a_local_executing_journal() {
        let authority = LocalAuthority::from_master("choice-begin-after-markdown", [93_u8; 32]);
        let mut store = Store::open_in_memory(authority.clone()).expect("open store");
        enable_runtime(&mut store, &authority);
        store
            .select_model_selection(&selection(), 1)
            .expect("persist selection");

        // This is the post-receipt shape: the local journal is durable, but
        // no Mission, Reminder, delivery, or other effect exists.
        let (_, mut executing) = begin_state("Earlier local question", "request-earlier");
        executing.session.state = ChoiceSessionState::Executing;
        executing.active_batch = None;
        store
            .save_choice_loop_snapshot(&executing, 10)
            .expect("seed an effect-free completed local journal");

        let (mut record, mut next) = begin_state("A different local question", "request-next");
        let next_revision = executing.session.revision + 1;
        record.accepted.operation_id = "operation-next".to_owned();
        record.accepted.choice_session_id = "session-next".to_owned();
        record.accepted.accepted_session_revision = next_revision;
        record.accepted.source_envelope_id = "source-next".to_owned();
        record.accepted.conversation_turn_batch_id = "batch-next".to_owned();
        record.source_envelope.id = record.accepted.source_envelope_id.clone();
        record.batch.id = record.accepted.conversation_turn_batch_id.clone();
        record.batch.choice_session_id = record.accepted.choice_session_id.clone();
        record.batch.source_envelope_ids = vec![record.source_envelope.id.clone()];
        record.batch.revision = next_revision;
        next.session.id = record.accepted.choice_session_id.clone();
        next.session.revision = next_revision;
        let batch = next.active_batch.as_mut().expect("initial sealed batch");
        batch.id = record.batch.id.clone();
        batch.choice_session_id = record.batch.choice_session_id.clone();
        batch.source_envelope_ids = record.batch.source_envelope_ids.clone();
        batch.revision = next_revision;

        assert_eq!(
            store
                .begin_choice_session(&record, &next)
                .expect("new explicit question supersedes only the safe local state"),
            record.accepted
        );
        assert_eq!(
            store
                .choice_loop_snapshot()
                .expect("read durable successor")
                .expect("current snapshot")
                .session
                .state,
            ChoiceSessionState::Interpreting
        );
        assert_eq!(
            store
                .connection
                .query_row("SELECT COUNT(*) FROM mission_state", [], |row| row
                    .get::<_, i64>(0))
                .expect("no Mission exists"),
            0
        );
    }

    #[test]
    fn initial_choice_result_is_operation_generation_and_provenance_bound() {
        let authority = LocalAuthority::from_master("choice-initial-result", [93_u8; 32]);
        let mut store = Store::open_in_memory(authority.clone()).unwrap();
        enable_runtime(&mut store, &authority);
        store.select_model_selection(&selection(), 1).unwrap();
        let (record, snapshot) = begin_state("Plan one bounded task", "request-result-1");
        store.begin_choice_session(&record, &snapshot).unwrap();
        let result = initial_result(&record);

        let mut wrong_persona = result.clone();
        let forged_persona = PersonaRevisionRef {
            persona_id: record.persona_revision.persona_id.clone(),
            revision: "draft-04-en".to_owned(),
            aggregate_digest: "a".repeat(64),
            instructions_digest: "b".repeat(64),
        };
        wrong_persona.persona_revision = forged_persona.clone();
        wrong_persona.choice_set.persona_revision = forged_persona;
        assert!(matches!(
            store.commit_initial_choice_result(&wrong_persona),
            Err(StoreError::ChoiceLoopStateConflict)
        ));

        let committed = store.commit_initial_choice_result(&result).unwrap();
        assert_eq!(committed.session.state, ChoiceSessionState::Active);
        assert_eq!(committed.session.revision, 2);
        assert_eq!(committed.active_choice_set, Some(result.choice_set.clone()));
        assert_eq!(
            store.commit_initial_choice_result(&result).unwrap(),
            committed
        );

        let mut changed = result.clone();
        changed.choice_set.options[0].direction = "Changed replay".to_owned();
        assert!(matches!(
            store.commit_initial_choice_result(&changed),
            Err(StoreError::ChoiceLoopStateConflict)
        ));
        let mut stale = result;
        stale.expected_generation = 2;
        assert!(matches!(
            store.commit_initial_choice_result(&stale),
            Err(StoreError::ChoiceLoopStateConflict)
        ));
        let mission_count: i64 = store
            .connection
            .query_row("SELECT COUNT(*) FROM mission_state", [], |row| row.get(0))
            .unwrap();
        let receipt_count: i64 = store
            .connection
            .query_row("SELECT COUNT(*) FROM receipt_state", [], |row| row.get(0))
            .unwrap();
        assert_eq!((mission_count, receipt_count), (0, 0));
    }

    #[test]
    fn initial_choice_failure_is_durable_idempotent_and_never_reopens_the_batch() {
        let authority = LocalAuthority::from_master("choice-initial-block", [93_u8; 32]);
        let mut store = Store::open_in_memory(authority.clone()).unwrap();
        enable_runtime(&mut store, &authority);
        store.select_model_selection(&selection(), 1).unwrap();
        let (record, snapshot) = begin_state("Plan one bounded task", "request-blocked-1");
        store.begin_choice_session(&record, &snapshot).unwrap();

        let blocked = store
            .block_initial_choice_operation(
                &record.accepted.operation_id,
                record.runtime_revision,
                record.accepted_at_ms + 1,
            )
            .unwrap();
        assert_eq!(blocked.session.state, ChoiceSessionState::Blocked);
        assert_eq!(blocked.session.revision, snapshot.session.revision + 1);
        assert!(blocked.active_batch.is_none());
        assert_eq!(
            store
                .block_initial_choice_operation(
                    &record.accepted.operation_id,
                    record.runtime_revision,
                    record.accepted_at_ms + 2,
                )
                .unwrap(),
            blocked
        );
        assert!(matches!(
            store.commit_initial_choice_result(&initial_result(&record)),
            Err(StoreError::ChoiceLoopStateConflict)
        ));
    }

    #[test]
    fn host_restart_recovery_blocks_only_the_interrupted_choice_worker_without_retrying() {
        let authority = LocalAuthority::from_master("choice-restart-recovery", [93_u8; 32]);
        let mut store = Store::open_in_memory(authority.clone()).unwrap();
        enable_runtime(&mut store, &authority);
        store.select_model_selection(&selection(), 1).unwrap();
        let (record, snapshot) = begin_state("Plan one bounded task", "request-restart-1");
        store.begin_choice_session(&record, &snapshot).unwrap();

        let recovered = store
            .recover_interrupted_choice_operation(
                record.runtime_revision,
                record.accepted_at_ms + 1,
            )
            .unwrap()
            .expect("interpreting state is recovered");
        assert_eq!(recovered.session.state, ChoiceSessionState::Blocked);
        assert!(recovered.active_batch.is_none());
        assert_eq!(
            store
                .recover_interrupted_choice_operation(
                    record.runtime_revision,
                    record.accepted_at_ms + 2,
                )
                .unwrap(),
            None
        );
        assert!(matches!(
            store.commit_initial_choice_result(&initial_result(&record)),
            Err(StoreError::ChoiceLoopStateConflict)
        ));

        let (mut refining_store, _, operation, _, _) = refining_d_store("choice-restart-refining");
        let recovered = refining_store
            .recover_interrupted_choice_operation(
                operation.expected_generation,
                operation.created_at_ms + 1,
            )
            .unwrap()
            .expect("refining state is recovered");
        assert_eq!(recovered.session.state, ChoiceSessionState::Blocked);
        assert!(recovered.pending_refinement_operation.is_none());
        assert_eq!(
            refining_store
                .recover_interrupted_choice_operation(
                    operation.expected_generation,
                    operation.created_at_ms + 2,
                )
                .unwrap(),
            None
        );
    }

    #[test]
    fn choice_cancellation_is_terminal_and_rejects_late_initial_results() {
        let authority = LocalAuthority::from_master("choice-cancel", [93_u8; 32]);
        let mut store = Store::open_in_memory(authority.clone()).unwrap();
        enable_runtime(&mut store, &authority);
        store.select_model_selection(&selection(), 1).unwrap();
        let (record, snapshot) = begin_state("Plan one bounded task", "request-cancel-1");
        store.begin_choice_session(&record, &snapshot).unwrap();

        let cancelled = store
            .cancel_choice_session(record.runtime_revision, record.accepted_at_ms + 1)
            .unwrap();
        assert_eq!(cancelled.session.state, ChoiceSessionState::Cancelled);
        assert_eq!(cancelled.session.revision, snapshot.session.revision + 1);
        assert!(cancelled.active_batch.is_none());
        assert!(cancelled.active_choice_set.is_none());
        let retained_bodies: i64 = store
            .connection
            .query_row("SELECT COUNT(*) FROM choice_begin_request", [], |row| {
                row.get(0)
            })
            .unwrap();
        let tombstones: i64 = store
            .connection
            .query_row(
                "SELECT COUNT(*) FROM choice_private_body_tombstone WHERE source_kind = 'begin'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!((retained_bodies, tombstones), (0, 1));
        assert!(matches!(
            store.choice_begin_replay(&record.accepted.request_id, &record.request_digest),
            Err(StoreError::ChoiceBeginConflict)
        ));
        assert!(matches!(
            store.commit_initial_choice_result(&initial_result(&record)),
            Err(StoreError::ChoiceLoopStateConflict)
        ));
    }

    #[test]
    fn private_body_retirement_is_authenticated_and_direct_row_tampering_fails_closed() {
        let authority = LocalAuthority::from_master("choice-retirement-tamper", [93_u8; 32]);
        let mut store = Store::open_in_memory(authority.clone()).unwrap();
        enable_runtime(&mut store, &authority);
        store.select_model_selection(&selection(), 1).unwrap();
        let (record, snapshot) = begin_state("Plan one bounded task", "request-retirement-tamper");
        store.begin_choice_session(&record, &snapshot).unwrap();
        store
            .cancel_choice_session(record.runtime_revision, record.accepted_at_ms + 1)
            .unwrap();

        assert!(matches!(
            store.choice_begin_replay(&record.accepted.request_id, &record.request_digest),
            Err(StoreError::ChoiceBeginConflict)
        ));
        store
            .connection
            .execute(
                "UPDATE choice_private_body_retirement SET blob_hash = '0' WHERE source_kind = 'begin' AND entity_id = ?1",
                [&record.accepted.request_id],
            )
            .unwrap();
        assert!(matches!(
            store.choice_begin_replay(&record.accepted.request_id, &record.request_digest),
            Err(StoreError::ChoiceLoopStateConflict)
        ));
    }

    #[test]
    fn cancelling_a_d_refinement_tombstones_and_removes_its_encrypted_body() {
        let authority = LocalAuthority::from_master("choice-d-private-body", [93_u8; 32]);
        let mut store = Store::open_in_memory(authority.clone()).unwrap();
        enable_runtime(&mut store, &authority);
        store.select_model_selection(&selection(), 1).unwrap();
        let (begin, snapshot) = begin_state("Plan one bounded task", "request-d-private");
        store.begin_choice_session(&begin, &snapshot).unwrap();
        let active = store
            .commit_initial_choice_result(&initial_result(&begin))
            .unwrap();
        let choice_set = active.active_choice_set.clone().unwrap();
        let d_record = d_intake_record(&active);
        let refining = store.commit_choice_d_selection(&d_record, 1).unwrap();
        assert_eq!(refining.session.state, ChoiceSessionState::Refining);
        let operation = refining.pending_refinement_operation.clone().unwrap();
        let interpretation = InterpretationFrame {
            choice_session_id: refining.session.id.clone(),
            revision: 2,
            understood_goal: "Refined private task".to_owned(),
            current_context: "The owner supplied D input".to_owned(),
            assumptions: vec![],
            constraints: vec![],
            uncertainties: vec![],
            what_to_avoid: vec![],
            source_manifest_digest: refining.document_manifest.aggregate_digest.clone(),
        };
        let mut result = ChoiceRefinementResult {
            operation_id: operation.id.clone(),
            selection_id: operation.selection_id.clone(),
            source_envelope_id: operation.source_envelope_id.clone(),
            conversation_turn_batch_id: operation.conversation_turn_batch_id.clone(),
            expected_session_revision: operation.expected_session_revision,
            expected_generation: operation.expected_generation,
            model_provenance: operation.model_provenance.clone(),
            source_manifest_digest: refining.document_manifest.aggregate_digest.clone(),
            persona_revision: operation.persona_revision.clone(),
            interpretation: interpretation.clone(),
            choice_set: ChoiceSet {
                id: "choices-d-private-refined".to_owned(),
                choice_session_id: refining.session.id.clone(),
                session_revision: refining.session.revision + 1,
                interpretation_revision: interpretation.revision,
                generated_at_ms: 20,
                expires_on_revision: refining.session.revision + 1,
                options: choice_set.options,
                d_available: true,
                source_manifest_digest: refining.document_manifest.aggregate_digest.clone(),
                model_provenance: operation.model_provenance,
                persona_revision: operation.persona_revision.clone(),
            },
            result_digest: String::new(),
            completed_at_ms: 20,
        };
        result.result_digest = result.canonical_result_digest().unwrap();
        store.commit_choice_refinement_result(&result).unwrap();

        store.cancel_choice_session(1, 21).unwrap();
        let retained_bodies: i64 = store
            .connection
            .query_row("SELECT COUNT(*) FROM choice_d_request", [], |row| {
                row.get(0)
            })
            .unwrap();
        let tombstones: i64 = store
            .connection
            .query_row(
                "SELECT COUNT(*) FROM choice_private_body_tombstone
                 WHERE source_kind = 'd' AND request_id = 'choice-d-request-1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!((retained_bodies, tombstones), (0, 1));
        let retained_results: i64 = store
            .connection
            .query_row("SELECT COUNT(*) FROM choice_refinement_result", [], |row| {
                row.get(0)
            })
            .unwrap();
        let result_tombstones: i64 = store
            .connection
            .query_row(
                "SELECT COUNT(*) FROM choice_refinement_body_tombstone WHERE operation_id = ?1",
                [&operation.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!((retained_results, result_tombstones), (0, 1));
        assert!(matches!(
            store.commit_choice_d_selection(&d_record, 1),
            Err(StoreError::ChoiceLoopStateConflict)
        ));
        store
            .current_verified_audit_anchor()
            .expect("body-free tombstone keeps audit valid");
        store
            .choice_loop_snapshot()
            .expect("cancelled snapshot stays readable after raw-body removal");
    }

    #[test]
    #[allow(clippy::too_many_lines)] // Retains the ordered receipt/retirement assertions in one transaction test.
    fn markdown_render_intent_and_receipt_are_atomic_replay_safe_and_effect_free() {
        let authority = LocalAuthority::from_master("markdown-render", [93_u8; 32]);
        let mut store = Store::open_in_memory(authority.clone()).expect("open store");
        enable_runtime(&mut store, &authority);
        store
            .select_model_selection(&selection(), 1)
            .expect("persist selection");
        let (record, snapshot) = begin_state("Plan a local task", "request-markdown");
        store
            .begin_choice_session(&record, &snapshot)
            .expect("persist begin state");
        let active = store
            .commit_initial_choice_result(&initial_result(&record))
            .expect("commit active choice state");
        let body = "# Local continuity\n";
        let entry = DocumentManifestEntry {
            relative_path: "tasks/session-1/STATE.md".to_owned(),
            sha256: format!("{:x}", Sha256::digest(body.as_bytes())),
            byte_length: body.len() as u64,
            mode: 0o600,
        };
        let intent = MarkdownRenderIntent {
            id: "markdown-intent-1".to_owned(),
            choice_session_id: active.session.id.clone(),
            expected_session_revision: active.session.revision,
            expected_generation: 1,
            entry: entry.clone(),
            expected_base: None,
            content_digest: entry.sha256.clone(),
            created_at_ms: 22,
        };
        assert_eq!(
            store
                .begin_markdown_render_for_test(&intent, body)
                .expect("persist intent"),
            intent
        );
        assert_eq!(
            store
                .begin_markdown_render_for_test(&intent, body)
                .expect("exact replay"),
            intent
        );
        store
            .record_markdown_reconciliation(&intent.id, 22)
            .expect("persist body-free reconciliation marker");
        assert!(matches!(
            store.private_markdown_body_available_for_test(&intent.id, false),
            Err(StoreError::ChoiceLoopStateConflict)
        ));
        assert!(
            store
                .private_markdown_body_available_for_test(&intent.id, true)
                .unwrap()
        );
        let original_intent_blob: (Vec<u8>, String) = store
            .connection
            .query_row(
                "SELECT encrypted_blob, blob_hash FROM choice_markdown_render_intent WHERE intent_id = ?1",
                [&intent.id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert!(matches!(
            store.begin_markdown_render_for_test(&intent, "# changed\n"),
            Err(StoreError::ChoiceLoopStateConflict)
        ));
        assert!(
            store
                .retire_choice_private_bodies_after_render(&intent.id, 23)
                .is_err()
        );
        let retained_before_receipt: i64 = store
            .connection
            .query_row("SELECT COUNT(*) FROM choice_begin_request", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(retained_before_receipt, 1);
        let receipt = MarkdownRenderReceipt {
            intent_id: intent.id.clone(),
            final_entry: entry,
            final_device: 1,
            final_inode: 2,
            displaced_base: None,
            committed_at_ms: 23,
        };
        assert_eq!(
            store
                .commit_markdown_render_receipt_for_test(&receipt)
                .expect("commit receipt"),
            receipt
        );
        let retained_before_retirement: i64 = store
            .connection
            .query_row("SELECT COUNT(*) FROM choice_begin_request", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(retained_before_retirement, 1);
        store
            .retire_choice_private_bodies_after_render(&intent.id, 24)
            .expect("durable receipt permits body retirement");
        let retained_bodies: i64 = store
            .connection
            .query_row("SELECT COUNT(*) FROM choice_begin_request", [], |row| {
                row.get(0)
            })
            .unwrap();
        let tombstones: i64 = store
            .connection
            .query_row(
                "SELECT COUNT(*) FROM choice_private_body_tombstone WHERE source_kind = 'begin'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!((retained_bodies, tombstones), (0, 1));
        let retired_intent_blob: (Vec<u8>, String) = store
            .connection
            .query_row(
                "SELECT encrypted_blob, blob_hash FROM choice_markdown_render_intent WHERE intent_id = ?1",
                [&intent.id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        store
            .connection
            .execute(
                "UPDATE choice_markdown_render_intent SET encrypted_blob = ?2, blob_hash = ?3 WHERE intent_id = ?1",
                params![&intent.id, original_intent_blob.0, original_intent_blob.1],
            )
            .unwrap();
        assert!(matches!(
            store.markdown_render_intent(&intent.id),
            Err(StoreError::StateBindingMismatch(_))
        ));
        store
            .connection
            .execute(
                "UPDATE choice_markdown_render_intent SET encrypted_blob = ?2, blob_hash = ?3 WHERE intent_id = ?1",
                params![&intent.id, retired_intent_blob.0, retired_intent_blob.1],
            )
            .unwrap();
        assert_eq!(
            store
                .commit_markdown_render_receipt_for_test(&receipt)
                .expect("exact receipt replay"),
            receipt
        );
        let mut changed_receipt = receipt;
        changed_receipt.final_inode = 3;
        assert!(matches!(
            store.commit_markdown_render_receipt_for_test(&changed_receipt),
            Err(StoreError::ChoiceLoopStateConflict)
        ));
    }

    #[test]
    fn cancellation_retires_pending_markdown_body_without_fabricating_a_receipt() {
        let authority = LocalAuthority::from_master("markdown-cancel", [93_u8; 32]);
        let mut store = Store::open_in_memory(authority.clone()).unwrap();
        enable_runtime(&mut store, &authority);
        store.select_model_selection(&selection(), 1).unwrap();
        let (record, snapshot) = begin_state("Plan a local task", "request-markdown-cancel");
        store.begin_choice_session(&record, &snapshot).unwrap();
        let active = store
            .commit_initial_choice_result(&initial_result(&record))
            .unwrap();
        let body = "# Local continuity\n";
        let entry = DocumentManifestEntry {
            relative_path: "tasks/session-1/STATE.md".to_owned(),
            sha256: format!("{:x}", Sha256::digest(body.as_bytes())),
            byte_length: body.len() as u64,
            mode: 0o600,
        };
        let intent = MarkdownRenderIntent {
            id: "markdown-intent-1".to_owned(),
            choice_session_id: active.session.id.clone(),
            expected_session_revision: active.session.revision,
            expected_generation: 1,
            entry: entry.clone(),
            expected_base: None,
            content_digest: entry.sha256,
            created_at_ms: 22,
        };
        store.begin_markdown_render_for_test(&intent, body).unwrap();
        store.cancel_choice_session(1, 23).unwrap();
        assert!(matches!(
            store.private_markdown_body_available_for_test(&intent.id, false),
            Err(StoreError::ChoiceLoopStateConflict)
        ));
        assert!(store.markdown_render_receipt(&intent.id).unwrap().is_none());
        store.current_verified_audit_anchor().unwrap();
    }

    #[test]
    fn cancelled_receipted_journal_keeps_only_the_verified_off_cleanup_path() {
        let authority = LocalAuthority::from_master("markdown-receipt-off", [93_u8; 32]);
        let mut store = Store::open_in_memory(authority.clone()).unwrap();
        enable_runtime(&mut store, &authority);
        store.select_model_selection(&selection(), 1).unwrap();
        let (record, snapshot) = begin_state("Plan a local task", "request-markdown-receipt-off");
        store.begin_choice_session(&record, &snapshot).unwrap();
        let active = store
            .commit_initial_choice_result(&initial_result(&record))
            .unwrap();
        let body = "# Local continuity\n";
        let entry = DocumentManifestEntry {
            relative_path: "tasks/session-1/STATE.md".to_owned(),
            sha256: format!("{:x}", Sha256::digest(body.as_bytes())),
            byte_length: body.len() as u64,
            mode: 0o600,
        };
        let intent = MarkdownRenderIntent {
            id: "markdown-receipt-off-intent".to_owned(),
            choice_session_id: active.session.id.clone(),
            expected_session_revision: active.session.revision,
            expected_generation: 1,
            entry: entry.clone(),
            expected_base: None,
            content_digest: entry.sha256.clone(),
            created_at_ms: 22,
        };
        store.begin_markdown_render_for_test(&intent, body).unwrap();
        store
            .commit_markdown_render_receipt_for_test(&MarkdownRenderReceipt {
                intent_id: intent.id.clone(),
                final_entry: entry,
                final_device: 1,
                final_inode: 2,
                displaced_base: None,
                committed_at_ms: 23,
            })
            .unwrap();
        store.cancel_choice_session(1, 24).unwrap();

        let broker = SigningKey::from_bytes(&[94_u8; 32]);
        let authorization = store.prepare_runtime_control(false, 25).unwrap();
        let mut off_receipt = RuntimeControlReceipt {
            protocol_version: EFFECT_PROTOCOL_VERSION,
            authorization_hash: runtime_control_authorization_hash(&authorization).unwrap(),
            checkpoint_nonce: "91".repeat(32),
            request_nonce: None,
            broker_key_id: format!("{:x}", Sha256::digest(broker.verifying_key().to_bytes())),
            broker_signature_hex: String::new(),
        };
        off_receipt.broker_signature_hex = hex::encode(
            broker
                .sign(&runtime_control_receipt_signing_bytes(&off_receipt).unwrap())
                .to_bytes(),
        );
        store
            .commit_runtime_control(&authorization, &off_receipt)
            .unwrap();

        let stored = load_markdown_render_intent(&store.connection, &authority, &intent.id)
            .unwrap()
            .unwrap();
        let receipt = load_markdown_render_receipt(&store.connection, &authority, &intent.id)
            .unwrap()
            .unwrap();
        require_current_markdown_cleanup_authority(
            &store.connection,
            &authority,
            store.trusted_broker.as_ref(),
            &stored.intent,
            &receipt,
        )
        .expect("Off permits only verified retained-base cleanup");
        assert!(matches!(
            store.private_markdown_body_available_for_test(&intent.id, false),
            Err(StoreError::ChoiceLoopStateConflict)
        ));
    }

    #[test]
    fn direct_markdown_publication_rejects_a_durable_global_off() {
        use std::os::unix::fs::PermissionsExt;

        let authority = LocalAuthority::from_master("markdown-off-fence", [93_u8; 32]);
        let mut store = Store::open_in_memory(authority.clone()).unwrap();
        enable_runtime(&mut store, &authority);
        let home = tempfile::tempdir().unwrap();
        let documents = home.path().join("Documents");
        let root = documents.join("OpenOpen");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::set_permissions(&documents, std::fs::Permissions::from_mode(0o700)).unwrap();
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
        store.bind_choice_markdown_root(home.path()).unwrap();
        store.select_model_selection(&selection(), 1).unwrap();
        let (record, snapshot) = begin_state("Plan a local task", "request-markdown-off");
        store.begin_choice_session(&record, &snapshot).unwrap();
        let active = store
            .commit_initial_choice_result(&initial_result(&record))
            .unwrap();
        let body = "# Local continuity\n";
        let entry = DocumentManifestEntry {
            relative_path: "tasks/session-1/STATE.md".to_owned(),
            sha256: format!("{:x}", Sha256::digest(body.as_bytes())),
            byte_length: body.len() as u64,
            mode: 0o600,
        };
        let intent = MarkdownRenderIntent {
            id: "markdown-off-intent".to_owned(),
            choice_session_id: active.session.id,
            expected_session_revision: active.session.revision,
            expected_generation: 1,
            entry: entry.clone(),
            expected_base: None,
            content_digest: entry.sha256,
            created_at_ms: 22,
        };
        store.begin_markdown_render_for_test(&intent, body).unwrap();

        let broker = SigningKey::from_bytes(&[94_u8; 32]);
        let authorization = store.prepare_runtime_control(false, 2).unwrap();
        let mut receipt = RuntimeControlReceipt {
            protocol_version: EFFECT_PROTOCOL_VERSION,
            authorization_hash: runtime_control_authorization_hash(&authorization).unwrap(),
            checkpoint_nonce: "91".repeat(32),
            request_nonce: None,
            broker_key_id: format!("{:x}", Sha256::digest(broker.verifying_key().to_bytes())),
            broker_signature_hex: String::new(),
        };
        receipt.broker_signature_hex = hex::encode(
            broker
                .sign(&runtime_control_receipt_signing_bytes(&receipt).unwrap())
                .to_bytes(),
        );
        store
            .commit_runtime_control(&authorization, &receipt)
            .unwrap();

        assert!(matches!(
            store.publish_markdown_render_intent(&intent.id, false, 23),
            Err(StoreError::ChoiceLoopStateConflict)
        ));
        assert!(!root.join(&intent.entry.relative_path).try_exists().unwrap());
    }

    #[test]
    #[allow(clippy::too_many_lines)] // One ordered clock corpus proves calibration through stale transition.
    fn host_owned_idle_transition_requires_continuous_clock_and_retires_stale_choices() {
        let authority = LocalAuthority::from_master("choice-idle", [93_u8; 32]);
        let mut store = Store::open_in_memory(authority.clone()).expect("open store");
        enable_runtime(&mut store, &authority);
        store
            .select_model_selection(&selection(), 1)
            .expect("persist selection");
        let (record, snapshot) = begin_state("Plan a local task", "request-idle");
        store
            .begin_choice_session(&record, &snapshot)
            .expect("begin session");
        let active = store
            .commit_initial_choice_result(&initial_result(&record))
            .expect("active session");
        let active_choices = active.active_choice_set.clone().expect("initial ChoiceSet");
        let calibration = ChoiceIdleClockEvidence {
            boot_id: "test-boot".to_owned(),
            wall_clock_ms: active.session.opened_at_ms,
            monotonic_ms: active.session.opened_at_ms,
        };
        let calibrated = store
            .advance_choice_idle_state(&active.session.id, active.session.revision, 1, &calibration)
            .expect("seed clock anchor");
        assert_eq!(calibrated, active);
        assert!(matches!(
            store.advance_choice_idle_state_classified(
                &active.session.id,
                active.session.revision,
                1,
                &calibration,
            ),
            Ok(ChoiceIdleAdvance::Unchanged(snapshot)) if snapshot == active
        ));
        assert!(matches!(
            store.advance_choice_idle_state(
                &active.session.id,
                active.session.revision,
                1,
                &ChoiceIdleClockEvidence {
                    boot_id: "test-boot".to_owned(),
                    wall_clock_ms: calibration.wall_clock_ms + 1,
                    monotonic_ms: calibration.monotonic_ms - 1,
                },
            ),
            Err(StoreError::ChoiceClockUncertain)
        ));
        assert!(matches!(
            store.advance_choice_idle_state(
                &active.session.id,
                active.session.revision,
                1,
                &ChoiceIdleClockEvidence {
                    boot_id: "test-boot".to_owned(),
                    wall_clock_ms: calibration.wall_clock_ms - 1,
                    monotonic_ms: calibration.monotonic_ms + 1,
                },
            ),
            Err(StoreError::ChoiceClockUncertain)
        ));
        let soft_clock = ChoiceIdleClockEvidence {
            boot_id: "test-boot".to_owned(),
            wall_clock_ms: active.session.soft_idle_at_ms,
            monotonic_ms: active.session.soft_idle_at_ms,
        };
        let soft = store
            .advance_choice_idle_state(&active.session.id, active.session.revision, 1, &soft_clock)
            .expect("soft idle after continuous clock evidence");
        assert_eq!(soft.session.state, ChoiceSessionState::SoftIdle);
        assert!(soft.active_choice_set.is_none());
        assert!(soft.session.active_choice_set_id.is_none());
        let soft_selection = Selection::OptionSelection(OptionSelection {
            id: "selection-during-soft-idle".to_owned(),
            choice_session_id: soft.session.id.clone(),
            choice_set_id: active_choices.id.clone(),
            selected_option_id: active_choices.options[0].id.clone(),
            expected_session_revision: soft.session.revision,
            selected_at_ms: soft.session.soft_idle_at_ms + 1,
        });
        assert!(matches!(
            store.commit_choice_selection(&soft_selection, 1, soft.session.soft_idle_at_ms + 1,),
            Err(StoreError::ChoiceLoopStateConflict)
        ));
        let sleep_shaped_clock = ChoiceIdleClockEvidence {
            boot_id: "test-boot".to_owned(),
            wall_clock_ms: soft.session.stale_review_at_ms - 1,
            monotonic_ms: soft_clock.monotonic_ms + 1,
        };
        assert!(matches!(
            store.advance_choice_idle_state_classified(
                &soft.session.id,
                soft.session.revision,
                1,
                &sleep_shaped_clock,
            ),
            Ok(ChoiceIdleAdvance::Calibrated(snapshot)) if snapshot == soft
        ));
        // The ambiguous sample updates only the authenticated clock anchor. It
        // consumes no deadline, revision, ChoiceSet, model, or effect authority.
        assert_eq!(store.choice_loop_snapshot().unwrap(), Some(soft.clone()));
        let audit_count_after_recalibration: i64 = store
            .connection
            .query_row("SELECT COUNT(*) FROM audit_ledger", [], |row| row.get(0))
            .unwrap();
        assert!(matches!(
            store.advance_choice_idle_state_classified(
                &soft.session.id,
                soft.session.revision,
                1,
                &sleep_shaped_clock,
            ),
            Ok(ChoiceIdleAdvance::Unchanged(snapshot)) if snapshot == soft
        ));
        assert_eq!(
            store
                .connection
                .query_row("SELECT COUNT(*) FROM audit_ledger", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            audit_count_after_recalibration,
            "the later continuity proof creates neither a revision nor an audit"
        );
        let reboot_clock = ChoiceIdleClockEvidence {
            boot_id: "boot-2".to_owned(),
            wall_clock_ms: sleep_shaped_clock.wall_clock_ms + 1_000,
            monotonic_ms: 1,
        };
        let recalibrated = store
            .advance_choice_idle_state(&soft.session.id, soft.session.revision, 1, &reboot_clock)
            .expect("reboot clock recalibration");
        assert_eq!(recalibrated, soft);
        assert!(matches!(
            store.advance_choice_idle_state_classified(
                &soft.session.id,
                soft.session.revision,
                1,
                &reboot_clock,
            ),
            Ok(ChoiceIdleAdvance::Unchanged(snapshot)) if snapshot == soft
        ));
        let stale_delta = soft
            .session
            .stale_review_at_ms
            .checked_sub(soft_clock.wall_clock_ms)
            .expect("stale deadline follows soft-idle deadline");
        let stale = store
            .advance_choice_idle_state(
                &soft.session.id,
                soft.session.revision,
                1,
                &ChoiceIdleClockEvidence {
                    boot_id: "boot-2".to_owned(),
                    wall_clock_ms: reboot_clock.wall_clock_ms + stale_delta,
                    monotonic_ms: reboot_clock.monotonic_ms + stale_delta,
                },
            )
            .expect("reboot-anchor stale review after later continuous sample");
        assert_eq!(stale.session.state, ChoiceSessionState::StaleReview);
        // Stale review retires the old menu. A timer can never manufacture a
        // fresh authenticated ChoiceSet, and the prior choices must no longer
        // remain selectable after the 24-hour boundary.
        assert!(stale.active_choice_set.is_none());
        assert!(stale.session.active_choice_set_id.is_none());
        let stale_selection = Selection::OptionSelection(OptionSelection {
            id: "selection-after-stale".to_owned(),
            choice_session_id: stale.session.id.clone(),
            choice_set_id: active_choices.id.clone(),
            selected_option_id: active_choices.options[0].id.clone(),
            expected_session_revision: stale.session.revision,
            selected_at_ms: stale.session.stale_review_at_ms + 1,
        });
        assert!(matches!(
            store.commit_choice_selection(
                &stale_selection,
                1,
                stale.session.stale_review_at_ms + 1,
            ),
            Err(StoreError::ChoiceLoopStateConflict)
        ));
        assert!(matches!(
            store.advance_choice_idle_state(
                &stale.session.id,
                stale.session.revision,
                1,
                &ChoiceIdleClockEvidence {
                    boot_id: "boot-2".to_owned(),
                    wall_clock_ms: stale.session.stale_review_at_ms,
                    monotonic_ms: 2,
                },
            ),
            Err(StoreError::ChoiceLoopStateConflict)
        ));
    }

    #[test]
    fn active_choice_clock_discontinuity_retires_authority_before_every_retry() {
        for (label, discontinuity) in [
            (
                "sleep",
                ChoiceIdleClockEvidence {
                    boot_id: "test-boot".to_owned(),
                    wall_clock_ms: 1_800_011,
                    monotonic_ms: 11,
                },
            ),
            (
                "reboot",
                ChoiceIdleClockEvidence {
                    boot_id: "boot-2".to_owned(),
                    wall_clock_ms: 86_400_011,
                    monotonic_ms: 1,
                },
            ),
        ] {
            let authority = LocalAuthority::from_master(
                format!("choice-clock-discontinuity-{label}"),
                [93_u8; 32],
            );
            let mut store = Store::open_in_memory(authority.clone()).expect("open store");
            enable_runtime(&mut store, &authority);
            store
                .select_model_selection(&selection(), 1)
                .expect("persist selection");
            let (record, snapshot) = begin_state("Plan a local task", "request-discontinuity");
            store
                .begin_choice_session(&record, &snapshot)
                .expect("begin session");
            let active = store
                .commit_initial_choice_result(&initial_result(&record))
                .expect("active session");
            let old_choice_set = active.active_choice_set.clone().expect("active ChoiceSet");

            let retired = match store
                .advance_choice_idle_state_classified(
                    &active.session.id,
                    active.session.revision,
                    1,
                    &discontinuity,
                )
                .expect("discontinuity fails closed by retiring old authority")
            {
                ChoiceIdleAdvance::Transitioned(snapshot) => snapshot,
                other => panic!("expected a durable retirement transition, got {other:?}"),
            };
            assert_eq!(retired.session.state, ChoiceSessionState::SoftIdle);
            assert!(retired.session.active_choice_set_id.is_none());
            assert!(retired.active_choice_set.is_none());

            let audit_count_after_retirement: i64 = store
                .connection
                .query_row("SELECT COUNT(*) FROM audit_ledger", [], |row| row.get(0))
                .unwrap();
            assert!(matches!(
                store.advance_choice_idle_state_classified(
                    &retired.session.id,
                    retired.session.revision,
                    1,
                    &discontinuity,
                ),
                Ok(ChoiceIdleAdvance::Unchanged(snapshot)) if snapshot == retired
            ));
            assert_eq!(
                store
                    .connection
                    .query_row("SELECT COUNT(*) FROM audit_ledger", [], |row| row
                        .get::<_, i64>(0))
                    .unwrap(),
                audit_count_after_retirement,
                "a retry cannot mint another revision or audit"
            );

            let stale_selection = Selection::OptionSelection(OptionSelection {
                id: format!("selection-after-{label}"),
                choice_session_id: retired.session.id.clone(),
                choice_set_id: old_choice_set.id.clone(),
                selected_option_id: old_choice_set.options[0].id.clone(),
                expected_session_revision: retired.session.revision,
                selected_at_ms: discontinuity.wall_clock_ms,
            });
            for _ in 0..2 {
                assert!(matches!(
                    store
                        .commit_choice_selection(&stale_selection, 1, discontinuity.wall_clock_ms,),
                    Err(StoreError::ChoiceLoopStateConflict)
                ));
            }
        }
    }

    #[test]
    fn idle_clock_anchor_survives_core_restart_without_using_wall_time_alone() {
        let root = tempfile::tempdir().expect("temporary Store root");
        let path = root.path().join("idle-restart.sqlite3");
        let authority = LocalAuthority::from_master("choice-idle-restart", [93_u8; 32]);
        let (session_id, session_revision, soft_idle_at_ms, trusted_broker) = {
            let mut store = Store::open(&path, authority.clone()).expect("open persistent Store");
            enable_runtime(&mut store, &authority);
            store
                .select_model_selection(&selection(), 1)
                .expect("persist selection");
            let (record, snapshot) = begin_state("Plan a local task", "request-idle-restart");
            store
                .begin_choice_session(&record, &snapshot)
                .expect("begin session");
            let active = store
                .commit_initial_choice_result(&initial_result(&record))
                .expect("active session");
            let calibration = ChoiceIdleClockEvidence {
                boot_id: "stable-boot".to_owned(),
                wall_clock_ms: active.session.soft_idle_at_ms - 1_000,
                monotonic_ms: 5_000,
            };
            let calibrated = store
                .advance_choice_idle_state(
                    &active.session.id,
                    active.session.revision,
                    1,
                    &calibration,
                )
                .expect("persist calibration");
            // A first clock sample is not elapsed-time authority.  The
            // fail-closed calibration therefore retires the old active menu;
            // it must not leave it usable until a later authenticated return
            // creates a new ChoiceSet.
            assert_eq!(calibrated.session.state, ChoiceSessionState::SoftIdle);
            assert!(calibrated.active_choice_set.is_none());
            (
                calibrated.session.id,
                calibrated.session.revision,
                calibrated.session.soft_idle_at_ms,
                store.trusted_broker.clone().expect("trusted broker"),
            )
        };
        let mut reopened = Store::open_with_trusted_broker(&path, authority, trusted_broker)
            .expect("reopen with exact broker");
        let first_continuous = reopened
            .advance_choice_idle_state(
                &session_id,
                session_revision,
                1,
                &ChoiceIdleClockEvidence {
                    boot_id: "stable-boot".to_owned(),
                    wall_clock_ms: soft_idle_at_ms,
                    monotonic_ms: 6_000,
                },
            )
            .expect("same-boot monotonic evidence survives Core restart");
        // The persisted calibration survives the Core restart. This first
        // continuous same-boot sample has no new timer authority to mint a
        // second revision or recap from the retired ChoiceSet.
        assert_eq!(first_continuous.session.state, ChoiceSessionState::SoftIdle);
        assert!(first_continuous.active_choice_set.is_none());
        assert_eq!(
            first_continuous.session.revision, session_revision,
            "a continuous retry cannot mint a second retirement revision"
        );
        let wall_jump_only = reopened
            .advance_choice_idle_state(
                &session_id,
                first_continuous.session.revision,
                1,
                &ChoiceIdleClockEvidence {
                    boot_id: "stable-boot".to_owned(),
                    wall_clock_ms: soft_idle_at_ms + 86_400_000,
                    monotonic_ms: 6_001,
                },
            )
            .expect("a wall-only jump is a non-authorizing recalibration");
        assert_eq!(wall_jump_only, first_continuous);
    }

    #[test]
    fn authenticated_idle_resume_is_atomic_replay_safe_and_failure_returns_to_idle() {
        let authority = LocalAuthority::from_master("choice-resume", [93_u8; 32]);
        let mut store = Store::open_in_memory(authority.clone()).unwrap();
        enable_runtime(&mut store, &authority);
        store.select_model_selection(&selection(), 1).unwrap();
        let (record, snapshot) = begin_state("Plan a local task", "request-resume");
        store.begin_choice_session(&record, &snapshot).unwrap();
        let active = store
            .commit_initial_choice_result(&initial_result(&record))
            .unwrap();
        let calibration = ChoiceIdleClockEvidence {
            boot_id: "resume-boot".to_owned(),
            wall_clock_ms: active.session.soft_idle_at_ms - 1_000,
            monotonic_ms: 5_000,
        };
        let soft = store
            .advance_choice_idle_state(&active.session.id, active.session.revision, 1, &calibration)
            .unwrap();
        assert_eq!(soft.session.state, ChoiceSessionState::SoftIdle);
        let resumed = store
            .begin_choice_resume(1, soft.session.soft_idle_at_ms + 1)
            .unwrap();
        let operation = resumed.pending_refinement_operation.clone().unwrap();
        assert_eq!(resumed.session.state, ChoiceSessionState::Refining);
        assert!(operation.is_owner_resume());
        assert_eq!(
            store
                .begin_choice_resume(1, soft.session.soft_idle_at_ms + 2)
                .unwrap(),
            resumed
        );
        let returned = store
            .block_choice_refinement_operation(
                &operation.id,
                operation.expected_generation,
                soft.session.soft_idle_at_ms + 3,
            )
            .unwrap();
        assert_eq!(returned.session.state, ChoiceSessionState::SoftIdle);
        assert!(returned.active_choice_set.is_none());
        assert!(returned.pending_refinement_operation.is_none());
        assert!(returned.interpretation.is_some());
        let retried = store
            .begin_choice_resume(1, soft.session.soft_idle_at_ms + 4)
            .expect("a new authenticated return can retry after the blocked resume");
        assert!(
            retried
                .pending_refinement_operation
                .as_ref()
                .is_some_and(ChoiceRefinementOperation::is_owner_resume)
        );
    }

    #[test]
    fn off_then_on_generation_retires_an_interrupted_owner_resume_without_retrying() {
        let authority = LocalAuthority::from_master("choice-resume-off-on", [93_u8; 32]);
        let mut store = Store::open_in_memory(authority.clone()).unwrap();
        enable_runtime(&mut store, &authority);
        store.select_model_selection(&selection(), 1).unwrap();
        let (record, snapshot) = begin_state("Plan a local task", "request-resume-off-on");
        store.begin_choice_session(&record, &snapshot).unwrap();
        let active = store
            .commit_initial_choice_result(&initial_result(&record))
            .unwrap();
        let soft = store
            .advance_choice_idle_state(
                &active.session.id,
                active.session.revision,
                1,
                &ChoiceIdleClockEvidence {
                    boot_id: "resume-off-on-boot".to_owned(),
                    wall_clock_ms: active.session.soft_idle_at_ms - 1_000,
                    monotonic_ms: 5_000,
                },
            )
            .unwrap();
        let pending = store
            .begin_choice_resume(1, soft.session.soft_idle_at_ms + 1)
            .unwrap();
        let operation = pending.pending_refinement_operation.clone().unwrap();
        commit_runtime_revision(&mut store, false, 2);
        commit_runtime_revision(&mut store, true, 3);

        let recovered = store
            .recover_interrupted_choice_operation(3, soft.session.soft_idle_at_ms + 2)
            .unwrap()
            .expect("replacement generation retires the old resume worker");
        assert_eq!(recovered.session.state, ChoiceSessionState::SoftIdle);
        assert!(recovered.pending_refinement_operation.is_none());
        assert!(recovered.active_choice_set.is_none());
        assert!(recovered.session.revision > pending.session.revision);
        assert!(matches!(
            store.begin_choice_resume(3, soft.session.soft_idle_at_ms + 3),
            Ok(next) if next.pending_refinement_operation.as_ref().is_some_and(ChoiceRefinementOperation::is_owner_resume)
        ));
        assert_ne!(operation.expected_generation, 3);
    }

    fn pending_choice_resume(
        label: &str,
    ) -> (
        Store,
        ChoiceLoopSnapshot,
        ChoiceRefinementOperation,
        ChoiceSet,
    ) {
        let authority = LocalAuthority::from_master(label, [93_u8; 32]);
        let mut store = Store::open_in_memory(authority.clone()).expect("open Store");
        enable_runtime(&mut store, &authority);
        store
            .select_model_selection(&selection(), 1)
            .expect("persist selection");
        let (record, snapshot) = begin_state("Plan a local task", label);
        store
            .begin_choice_session(&record, &snapshot)
            .expect("begin Choice");
        let active = store
            .commit_initial_choice_result(&initial_result(&record))
            .expect("activate Choice");
        let calibration = ChoiceIdleClockEvidence {
            boot_id: "resume-result-boot".to_owned(),
            wall_clock_ms: active.session.soft_idle_at_ms - 1_000,
            monotonic_ms: 5_000,
        };
        let soft = store
            .advance_choice_idle_state(&active.session.id, active.session.revision, 1, &calibration)
            .expect("retire ChoiceSet for idle");
        let resumed = store
            .begin_choice_resume(1, soft.session.soft_idle_at_ms + 1)
            .expect("begin exact owner resume");
        let operation = resumed
            .pending_refinement_operation
            .clone()
            .expect("pending owner resume");
        (
            store,
            resumed,
            operation,
            active.active_choice_set.expect("prior choices"),
        )
    }

    #[test]
    fn choice_resume_result_is_typed_and_exactly_replay_safe() {
        let (mut store, resumed, operation, choices) = pending_choice_resume("resume-result-valid");
        let result = ChoiceResumeResult {
            result: refinement_result_for(&resumed, &operation, &choices),
        };
        let active = store
            .commit_choice_resume_result(&result)
            .expect("commit exact typed owner-resume result");
        assert_eq!(active.session.state, ChoiceSessionState::Active);
        assert!(active.pending_refinement_operation.is_none());
        assert_eq!(
            store
                .commit_choice_resume_result(&result)
                .expect("exact typed replay"),
            active
        );
    }

    #[test]
    fn choice_resume_result_rejects_a_stale_generation() {
        let (mut stale_store, stale, stale_operation, stale_choices) =
            pending_choice_resume("resume-result-stale");
        let mut stale_result = ChoiceResumeResult {
            result: refinement_result_for(&stale, &stale_operation, &stale_choices),
        };
        stale_result.result.expected_generation += 1;
        stale_result.result.result_digest = stale_result
            .result
            .canonical_result_digest()
            .expect("re-sign stale test result");
        assert!(matches!(
            stale_store.commit_choice_resume_result(&stale_result),
            Err(StoreError::ChoiceLoopStateConflict)
        ));
    }

    #[test]
    fn choice_resume_result_rejects_cancelled_work() {
        let (mut cancelled_store, cancelled, cancelled_operation, cancelled_choices) =
            pending_choice_resume("resume-result-cancelled");
        let cancelled_result = ChoiceResumeResult {
            result: refinement_result_for(&cancelled, &cancelled_operation, &cancelled_choices),
        };
        cancelled_store
            .cancel_choice_session(1, cancelled.session.last_input_at_ms + 1)
            .expect("cancel pending owner resume");
        assert!(matches!(
            cancelled_store.commit_choice_resume_result(&cancelled_result),
            Err(StoreError::ChoiceLoopStateConflict)
        ));
    }

    #[test]
    fn choice_resume_result_rejects_a_wrong_operation_marker() {
        let (mut wrong_marker_store, wrong_marker, wrong_operation, wrong_choices) =
            pending_choice_resume("resume-result-marker");
        let mut wrong_marker_result = ChoiceResumeResult {
            result: refinement_result_for(&wrong_marker, &wrong_operation, &wrong_choices),
        };
        wrong_marker_result.result.operation_id = "resume-operation-other".to_owned();
        wrong_marker_result.result.result_digest = wrong_marker_result
            .result
            .canonical_result_digest()
            .expect("re-sign wrong-marker result");
        assert!(matches!(
            wrong_marker_store.commit_choice_resume_result(&wrong_marker_result),
            Err(StoreError::ChoiceLoopStateConflict)
        ));
    }

    #[test]
    fn idle_clock_anchor_is_authenticated_and_deletion_never_recalibrates() {
        fn seeded_store(label: &str) -> (Store, ChoiceLoopSnapshot, ChoiceIdleClockEvidence) {
            let authority = LocalAuthority::from_master(label, [93_u8; 32]);
            let mut store = Store::open_in_memory(authority.clone()).unwrap();
            enable_runtime(&mut store, &authority);
            store.select_model_selection(&selection(), 1).unwrap();
            let (record, snapshot) = begin_state("Plan a local task", label);
            store.begin_choice_session(&record, &snapshot).unwrap();
            let active = store
                .commit_initial_choice_result(&initial_result(&record))
                .unwrap();
            let calibration = ChoiceIdleClockEvidence {
                boot_id: "stable-boot".to_owned(),
                wall_clock_ms: active.session.soft_idle_at_ms - 1_000,
                monotonic_ms: 5_000,
            };
            store
                .advance_choice_idle_state(
                    &active.session.id,
                    active.session.revision,
                    1,
                    &calibration,
                )
                .unwrap();
            (store, active, calibration)
        }

        let (mut tampered, active, calibration) = seeded_store("clock-anchor-tamper");
        tampered
            .connection
            .execute(
                "UPDATE choice_idle_clock_anchor SET blob_hash = ?1 WHERE singleton_id = 1",
                ["0".repeat(64)],
            )
            .unwrap();
        assert!(
            tampered
                .advance_choice_idle_state(
                    &active.session.id,
                    active.session.revision,
                    1,
                    &ChoiceIdleClockEvidence {
                        wall_clock_ms: calibration.wall_clock_ms + 1,
                        monotonic_ms: calibration.monotonic_ms + 1,
                        ..calibration.clone()
                    },
                )
                .is_err()
        );

        let (mut deleted, active, calibration) = seeded_store("clock-anchor-delete");
        deleted
            .connection
            .execute("DELETE FROM choice_idle_clock_anchor", [])
            .unwrap();
        assert!(
            deleted
                .advance_choice_idle_state(
                    &active.session.id,
                    active.session.revision,
                    1,
                    &ChoiceIdleClockEvidence {
                        wall_clock_ms: calibration.wall_clock_ms + 1,
                        monotonic_ms: calibration.monotonic_ms + 1,
                        ..calibration
                    },
                )
                .is_err(),
            "a missing audited anchor is corruption, not a fresh calibration"
        );
    }
}

#[cfg(test)]
mod choice_loop_state_tests {
    use super::choice_begin_tests::{
        begin_state, enable_runtime, initial_result, persona_revision, refinement_result_for,
        refining_d_store, selection,
    };
    use super::*;
    use openopen_protocol::{
        CHOICE_SESSION_SOFT_IDLE_MS, CHOICE_SESSION_STALE_REVIEW_MS, ChoiceBeginRecord,
        ChoiceConsolidatedConfirmation, ChoiceLoopSnapshot, ChoiceOption,
        ChoiceReminderScheduleInput, ChoiceSession, ChoiceSessionState, ChoiceSet,
        ConversationTurnBatch, DocumentManifest, DocumentManifestEntry, InterpretationFrame,
        ModelProvenance, ModelSelection, ModelSelectionState, OptionSelection, PersonaRevisionRef,
        Selection, canonical_document_manifest_digest,
    };
    use tempfile::tempdir;

    fn manifest(entries: Vec<DocumentManifestEntry>, generated_at_ms: i64) -> DocumentManifest {
        let aggregate_digest = canonical_document_manifest_digest(&entries)
            .expect("test entries satisfy canonical manifest policy");
        DocumentManifest {
            root_version: 1,
            entries,
            aggregate_digest,
            generated_at_ms,
        }
    }

    fn snapshot() -> ChoiceLoopSnapshot {
        ChoiceLoopSnapshot {
            session: ChoiceSession {
                id: "session-1".to_owned(),
                state: ChoiceSessionState::Active,
                revision: 1,
                model_selection_state: ModelSelectionState::Unselected,
                communication_profile_revision: 0,
                active_choice_set_id: None,
                active_interpretation_revision: None,
                opened_at_ms: 10,
                last_input_at_ms: 20,
                soft_idle_at_ms: 20 + CHOICE_SESSION_SOFT_IDLE_MS,
                stale_review_at_ms: 20 + CHOICE_SESSION_STALE_REVIEW_MS,
                primary_delivery_binding_id: None,
                pending_confirmation_id: None,
                background_mission_ids: vec![],
            },
            active_batch: None,
            interpretation: None,
            active_choice_set: None,
            last_selection: None,
            pending_refinement_operation: None,
            confirmation: None,
            document_manifest: manifest(
                vec![DocumentManifestEntry {
                    relative_path: "sessions/session-1/SESSION.md".to_owned(),
                    sha256: "a".repeat(64),
                    byte_length: 64,
                    mode: 0o600,
                }],
                20,
            ),
        }
    }

    fn advance_snapshot(
        previous: &ChoiceLoopSnapshot,
        last_input_at_ms: i64,
    ) -> ChoiceLoopSnapshot {
        let mut next = previous.clone();
        next.session.revision += 1;
        next.session.last_input_at_ms = last_input_at_ms;
        next.session.soft_idle_at_ms = last_input_at_ms + CHOICE_SESSION_SOFT_IDLE_MS;
        next.session.stale_review_at_ms = last_input_at_ms + CHOICE_SESSION_STALE_REVIEW_MS;
        next.document_manifest.generated_at_ms = last_input_at_ms;
        next
    }

    fn choice_set_snapshot(
        previous: &ChoiceLoopSnapshot,
        last_input_at_ms: i64,
    ) -> ChoiceLoopSnapshot {
        let mut next = advance_snapshot(previous, last_input_at_ms);
        let source_manifest_digest = next.document_manifest.aggregate_digest.clone();
        next.interpretation = Some(InterpretationFrame {
            choice_session_id: next.session.id.clone(),
            revision: 1,
            understood_goal: "Prepare the next bounded step".to_owned(),
            current_context: "The user has one active local session".to_owned(),
            assumptions: vec![],
            constraints: vec![],
            uncertainties: vec![],
            what_to_avoid: vec![],
            source_manifest_digest: source_manifest_digest.clone(),
        });
        next.session.active_interpretation_revision = Some(1);
        next.active_choice_set = Some(ChoiceSet {
            id: "choices-1".to_owned(),
            choice_session_id: next.session.id.clone(),
            session_revision: next.session.revision,
            interpretation_revision: 1,
            generated_at_ms: last_input_at_ms,
            expires_on_revision: next.session.revision,
            options: vec![
                ChoiceOption {
                    id: "option-1".to_owned(),
                    position: 1,
                    direction: "Review the current plan".to_owned(),
                    rationale: "Keeps the next step bounded".to_owned(),
                    expected_result: "A clear local next step".to_owned(),
                    information_needed: vec![],
                    external_effects_preview: vec![],
                    source_categories: vec!["ownerInput".to_owned()],
                },
                ChoiceOption {
                    id: "option-2".to_owned(),
                    position: 2,
                    direction: "Narrow the next action".to_owned(),
                    rationale: "Reduces uncertainty before confirmation".to_owned(),
                    expected_result: "A smaller decision".to_owned(),
                    information_needed: vec![],
                    external_effects_preview: vec![],
                    source_categories: vec!["ownerInput".to_owned()],
                },
                ChoiceOption {
                    id: "option-3".to_owned(),
                    position: 3,
                    direction: "Prepare a safe alternative".to_owned(),
                    rationale: "Keeps an alternate path visible".to_owned(),
                    expected_result: "One bounded alternative".to_owned(),
                    information_needed: vec![],
                    external_effects_preview: vec![],
                    source_categories: vec!["ownerInput".to_owned()],
                },
            ],
            d_available: true,
            source_manifest_digest,
            model_provenance: ModelProvenance {
                id: "provenance-1".to_owned(),
                model_id: "gpt-test-model".to_owned(),
                requested_effort: "not_applicable".to_owned(),
                actual_effort: "not_applicable".to_owned(),
                catalog_fingerprint: "c".repeat(64),
                catalog_revision: 1,
                account_display_class: "ChatGPT account".to_owned(),
                protocol_schema_revision: 1,
                turn_id: "turn-1".to_owned(),
            },
            persona_revision: persona_revision(),
        });
        next.session.model_selection_state = ModelSelectionState::Selected {
            model_provenance_ref: "provenance-1".to_owned(),
        };
        next.session.active_choice_set_id = Some("choices-1".to_owned());
        next
    }

    fn batch_snapshot(previous: &ChoiceLoopSnapshot, last_input_at_ms: i64) -> ChoiceLoopSnapshot {
        let mut next = advance_snapshot(previous, last_input_at_ms);
        next.session.primary_delivery_binding_id = Some("binding-1".to_owned());
        next.active_batch = Some(ConversationTurnBatch {
            id: "batch-1".to_owned(),
            choice_session_id: next.session.id.clone(),
            delivery_binding_id: "binding-1".to_owned(),
            source_envelope_ids: vec!["envelope-1".to_owned()],
            opened_at_ms: last_input_at_ms,
            quiet_deadline_ms: last_input_at_ms + 2_500,
            hard_deadline_ms: last_input_at_ms + 8_000,
            sealed_at_ms: None,
            seal_reason: None,
            revision: next.session.revision,
        });
        next
    }

    fn active_choice_from_authenticated_begin(
        store: &mut Store,
        request_id: &str,
    ) -> (ChoiceBeginRecord, ChoiceLoopSnapshot, ChoiceSet) {
        store
            .select_model_selection(&selection(), 20)
            .expect("persist selected model for authenticated begin");
        let (begin, snapshot) = begin_state("Plan one bounded task", request_id);
        store
            .begin_choice_session(&begin, &snapshot)
            .expect("persist authenticated begin");
        let active = store
            .commit_initial_choice_result(&initial_result(&begin))
            .expect("commit authenticated initial result");
        let choice_set = active.active_choice_set.clone().expect("active ChoiceSet");
        (begin, active, choice_set)
    }

    fn choice_set_with_bound_batch_snapshot(
        previous: &ChoiceLoopSnapshot,
        last_input_at_ms: i64,
    ) -> ChoiceLoopSnapshot {
        let mut next = choice_set_snapshot(previous, last_input_at_ms);
        next.session.primary_delivery_binding_id = Some("binding-1".to_owned());
        next.active_batch = Some(ConversationTurnBatch {
            id: "batch-1".to_owned(),
            choice_session_id: next.session.id.clone(),
            delivery_binding_id: "binding-1".to_owned(),
            source_envelope_ids: vec!["envelope-1".to_owned()],
            opened_at_ms: last_input_at_ms,
            quiet_deadline_ms: last_input_at_ms + 2_500,
            hard_deadline_ms: last_input_at_ms + 8_000,
            sealed_at_ms: None,
            seal_reason: None,
            revision: next.session.revision,
        });
        next
    }

    fn matching_model_selection(choice_set: &ChoiceSet) -> ModelSelection {
        ModelSelection {
            id: "model-selection-1".to_owned(),
            model_id: choice_set.model_provenance.model_id.clone(),
            requested_effort: choice_set.model_provenance.requested_effort.clone(),
            actual_effort: choice_set.model_provenance.actual_effort.clone(),
            catalog_fingerprint: choice_set.model_provenance.catalog_fingerprint.clone(),
            catalog_revision: choice_set.model_provenance.catalog_revision,
            account_display_class: choice_set.model_provenance.account_display_class.clone(),
            protocol_schema_revision: choice_set.model_provenance.protocol_schema_revision,
        }
    }

    fn confirmation_for(
        choices: &ChoiceLoopSnapshot,
        choice_set: &ChoiceSet,
        confirmed_at_ms: i64,
    ) -> ChoiceConsolidatedConfirmation {
        let mut confirmation = ChoiceConsolidatedConfirmation {
            id: "confirmation-1".to_owned(),
            choice_session_id: choices.session.id.clone(),
            choice_set_id: choice_set.id.clone(),
            selection_id: choices.last_selection.as_ref().map_or_else(
                || "selection-1".to_owned(),
                |selection| selection.id().to_owned(),
            ),
            expected_session_revision: choices.session.revision,
            interpretation_revision: choice_set.interpretation_revision,
            payload_revision: 0,
            payload_digest: String::new(),
            goal: "Prepare one bounded local plan".to_owned(),
            steps: vec!["Review the prepared plan".to_owned()],
            markdown_entry: DocumentManifestEntry {
                relative_path: format!("sessions/{}/CHOICE.md", choices.session.id),
                sha256: "b".repeat(64),
                byte_length: 1,
                mode: 0o600,
            },
            markdown_expected_base: None,
            markdown_manifest_digests: vec![choices.document_manifest.aggregate_digest.clone()],
            document_diff_digest: "d".repeat(64),
            model_provenance: choice_set.model_provenance.clone(),
            persona_revision: choice_set.persona_revision.clone(),
            reminder_list_id: "reminder-list-1".to_owned(),
            reminder_items: vec![openopen_protocol::ChoiceReminderItem {
                id: "reminder-1".to_owned(),
                text: "Review the prepared plan".to_owned(),
                due_at_ms: confirmed_at_ms,
                time_zone: "Etc/UTC".to_owned(),
                evidence_intent: "reminder-readback".to_owned(),
            }],
            reminder_count: 1,
            reminder_payload_digest: String::new(),
            evidence_requirements: vec!["Reminder readback is required before Done".to_owned()],
            delivery_binding_id: None,
            recipient: None,
            delivery_scope: None,
            data_categories: vec!["local task state".to_owned()],
            retention: "Local until the user deletes it".to_owned(),
            permissions: vec![],
            effect_classes: vec!["reminder".to_owned()],
            confirmed_at_ms,
        };
        let markdown_body = confirmed_choice_markdown_body(&confirmation);
        confirmation.markdown_entry.sha256 = sha256_hex(markdown_body.as_bytes());
        confirmation.markdown_entry.byte_length =
            u64::try_from(markdown_body.len()).expect("bounded fixture Markdown length");
        confirmation.markdown_manifest_digests.push(
            canonical_document_manifest_digest(std::slice::from_ref(&confirmation.markdown_entry))
                .expect("canonical desired Markdown manifest"),
        );
        confirmation.reminder_payload_digest = confirmation
            .canonical_reminder_payload_digest()
            .expect("canonical reminder digest");
        confirmation.payload_revision = confirmation
            .canonical_payload_revision(1)
            .expect("canonical confirmation revision");
        confirmation.payload_digest = confirmation
            .canonical_payload_digest()
            .expect("canonical confirmation digest");
        confirmation
    }

    fn record_schedule_for(
        store: &mut Store,
        choices: &ChoiceLoopSnapshot,
        confirmation: &ChoiceConsolidatedConfirmation,
        request_id: &str,
        accepted_at_ms: i64,
    ) -> ChoiceReminderSchedule {
        store
            .record_choice_reminder_schedule(
                &ChoiceReminderScheduleInput {
                    request_id: request_id.to_owned(),
                    choice_session_id: choices.session.id.clone(),
                    expected_session_revision: choices.session.revision,
                    reminder_list_id: confirmation.reminder_list_id.clone(),
                    reminder_count: confirmation.reminder_count,
                    due_at_ms: confirmation.reminder_items[0].due_at_ms,
                    time_zone: confirmation.reminder_items[0].time_zone.clone(),
                },
                1,
                accepted_at_ms,
            )
            .expect("record effect-free schedule")
    }

    fn store_with_executing_markdown_receipt(
        authority_id: &str,
    ) -> (
        Store,
        ChoiceConsolidatedConfirmation,
        MarkdownRenderIntent,
        ChoiceLoopSnapshot,
    ) {
        let authority = LocalAuthority::from_master(authority_id, [93_u8; 32]);
        let mut store = Store::open_in_memory(authority.clone()).expect("open store");
        enable_runtime(&mut store, &authority);
        let first = snapshot();
        store.save_choice_loop_snapshot(&first, 20).unwrap();
        let mut choices = choice_set_snapshot(&first, 21);
        let choice_set = choices.active_choice_set.clone().unwrap();
        choices.last_selection = Some(Selection::OptionSelection(OptionSelection {
            id: "selection-1".to_owned(),
            choice_session_id: choices.session.id.clone(),
            choice_set_id: choice_set.id.clone(),
            selected_option_id: "option-1".to_owned(),
            expected_session_revision: choices.session.revision - 1,
            selected_at_ms: 21,
        }));
        store.save_choice_loop_snapshot(&choices, 21).unwrap();
        store
            .select_model_selection(&matching_model_selection(&choice_set), 21)
            .unwrap();
        let confirmation = confirmation_for(&choices, &choice_set, 22);
        record_schedule_for(
            &mut store,
            &choices,
            &confirmation,
            "schedule-request-render",
            21,
        );
        let (committed, intent) = store
            .commit_choice_confirmation_and_render_intent(&confirmation, 1, 22)
            .unwrap();
        store
            .commit_markdown_render_receipt_for_test(&MarkdownRenderReceipt {
                intent_id: intent.id.clone(),
                final_entry: intent.entry.clone(),
                final_device: 1,
                final_inode: 2,
                displaced_base: None,
                committed_at_ms: 24,
            })
            .unwrap();
        let next = store
            .retire_choice_private_bodies_after_render(&intent.id, 25)
            .unwrap();
        assert_eq!(next.session.revision, committed.session.revision + 1);
        (store, confirmation, intent, next)
    }

    fn store_with_reminder_schedule_for_tamper() -> (Store, ChoiceReminderSchedule) {
        let authority = LocalAuthority::from_master("choice-reminder-index-tamper", [93_u8; 32]);
        let mut store = Store::open_in_memory(authority.clone()).expect("open store");
        enable_runtime(&mut store, &authority);
        let first = snapshot();
        store.save_choice_loop_snapshot(&first, 20).expect("base");
        let mut choices = choice_set_snapshot(&first, 21);
        let choice_set = choices.active_choice_set.clone().expect("choice set");
        choices.last_selection = Some(Selection::OptionSelection(OptionSelection {
            id: "selection-1".to_owned(),
            choice_session_id: choices.session.id.clone(),
            choice_set_id: choice_set.id.clone(),
            selected_option_id: "option-1".to_owned(),
            expected_session_revision: choices.session.revision - 1,
            selected_at_ms: 21,
        }));
        store
            .save_choice_loop_snapshot(&choices, 21)
            .expect("choices");
        store
            .select_model_selection(&matching_model_selection(&choice_set), 21)
            .expect("model");
        let confirmation = confirmation_for(&choices, &choice_set, 22);
        let schedule = record_schedule_for(
            &mut store,
            &choices,
            &confirmation,
            "schedule-index-tamper-request",
            21,
        );
        (store, schedule)
    }

    #[test]
    fn choice_select_is_command_owned_atomic_and_replay_safe() {
        let authority = LocalAuthority::from_master("choice-select", [93_u8; 32]);
        let mut store = Store::open_in_memory(authority).expect("open store");
        enable_runtime(
            &mut store,
            &LocalAuthority::from_master("choice-select", [93_u8; 32]),
        );
        let (_begin, choices, choice_set) =
            active_choice_from_authenticated_begin(&mut store, "request-choice-select");
        let selection = Selection::OptionSelection(OptionSelection {
            id: "resume-soft-idle-forged".to_owned(),
            choice_session_id: choices.session.id.clone(),
            choice_set_id: choice_set.id.clone(),
            selected_option_id: "option-1".to_owned(),
            expected_session_revision: choices.session.revision,
            selected_at_ms: 22,
        });
        let committed = store
            .commit_choice_selection(&selection, 1, 22)
            .expect("commit selection");
        assert_eq!(committed.session.revision, 3);
        assert_eq!(committed.session.state, ChoiceSessionState::Refining);
        assert!(committed.active_choice_set.is_none());
        assert_eq!(committed.last_selection, Some(selection.clone()));
        let operation = committed
            .pending_refinement_operation
            .as_ref()
            .expect("selection owns a pending refinement operation");
        assert_eq!(operation.selection_id, "resume-soft-idle-forged");
        assert_eq!(operation.expected_generation, 1);
        assert!(
            !operation.is_owner_resume(),
            "a caller-supplied OptionSelection id cannot mint owner-return authority"
        );
        assert_eq!(
            store
                .commit_choice_selection(&selection, 1, 22)
                .expect("exact replay"),
            committed
        );
        assert!(matches!(
            store.commit_choice_selection(&selection, 1, 23),
            Err(StoreError::ChoiceLoopStateConflict)
        ));
        let mut stale = selection.clone();
        if let Selection::OptionSelection(value) = &mut stale {
            value.id = "selection-2".to_owned();
        }
        assert!(matches!(
            store.commit_choice_selection(&stale, 1, 24),
            Err(StoreError::ChoiceLoopStateConflict)
        ));

        let Selection::OptionSelection(mut caller_retry) = selection else {
            unreachable!("test constructs an option selection");
        };
        caller_retry.selected_at_ms = i64::MAX;
        assert_eq!(
            store
                .choice_option_selection_replay(&caller_retry)
                .expect("untrusted caller time is not replay authority"),
            Some(committed)
        );
    }

    #[test]
    fn choice_select_accepts_each_current_option_and_only_the_current_d_binding() {
        for (index, option_id) in ["option-1", "option-2", "option-3"].iter().enumerate() {
            let authority =
                LocalAuthority::from_master(format!("choice-select-option-{index}"), [93_u8; 32]);
            let mut store = Store::open_in_memory(authority.clone()).expect("open option store");
            enable_runtime(&mut store, &authority);
            let (_begin, choices, choice_set) = active_choice_from_authenticated_begin(
                &mut store,
                &format!("request-choice-select-option-{index}"),
            );
            let selection = Selection::OptionSelection(OptionSelection {
                id: format!("selection-option-{index}"),
                choice_session_id: choices.session.id.clone(),
                choice_set_id: choice_set.id.clone(),
                selected_option_id: (*option_id).to_owned(),
                expected_session_revision: choices.session.revision,
                selected_at_ms: 22,
            });
            assert_eq!(
                store
                    .commit_choice_selection(&selection, 1, 22)
                    .expect("commit current option")
                    .last_selection,
                Some(selection)
            );
        }

        let authority = LocalAuthority::from_master("choice-select-d", [93_u8; 32]);
        let mut store = Store::open_in_memory(authority.clone()).expect("open D store");
        enable_runtime(&mut store, &authority);
        let first = snapshot();
        store
            .save_choice_loop_snapshot(&first, 20)
            .expect("persist D base");
        let choices = choice_set_with_bound_batch_snapshot(&first, 21);
        assert!(choices.is_valid());
        let choice_set = choices.active_choice_set.clone().expect("D choice set");
        store
            .save_choice_loop_snapshot(&choices, 21)
            .expect("persist D choices");
        store
            .select_model_selection(&matching_model_selection(&choice_set), 21)
            .expect("persist D model provenance");
        let selection = Selection::NaturalConversationSelection(
            openopen_protocol::NaturalConversationSelection {
                id: "selection-d".to_owned(),
                choice_session_id: "session-1".to_owned(),
                choice_set_id: "choices-1".to_owned(),
                d_input_batch_id: "batch-1".to_owned(),
                expected_session_revision: 2,
                selected_at_ms: 22,
            },
        );
        let mut wrong_binding = selection.clone();
        if let Selection::NaturalConversationSelection(value) = &mut wrong_binding {
            value.d_input_batch_id = "batch-not-current".to_owned();
        }
        assert!(matches!(
            store.commit_choice_selection(&wrong_binding, 1, 22),
            Err(StoreError::ChoiceLoopStateConflict)
        ));
        // The historical natural-selection shape carries a persisted batch
        // identity, so it is never a public write route.  D must arrive as a
        // Host-derived encrypted intake record instead.
        assert!(matches!(
            store.commit_choice_selection(&selection, 1, 22),
            Err(StoreError::ChoiceLoopStateConflict)
        ));
    }

    #[test]
    #[allow(clippy::too_many_lines)] // Covers result replay, tamper, and visibility in one ordered transaction trace.
    fn refinement_result_is_selection_bound_replay_safe_and_never_partially_visible() {
        let authority = LocalAuthority::from_master("choice-refinement", [93_u8; 32]);
        let mut store = Store::open_in_memory(authority.clone()).expect("open store");
        enable_runtime(&mut store, &authority);
        store
            .select_model_selection(&selection(), 20)
            .expect("model");
        let (begin, snapshot) = begin_state("Plan one bounded task", "request-refinement");
        store
            .begin_choice_session(&begin, &snapshot)
            .expect("begin source-bound Choice");
        let choices = store
            .commit_initial_choice_result(&initial_result(&begin))
            .expect("activate source-bound Choice");
        let choice_set = choices.active_choice_set.clone().expect("choice set");
        let selection = Selection::OptionSelection(OptionSelection {
            id: "selection-refinement-1".to_owned(),
            choice_session_id: choices.session.id.clone(),
            choice_set_id: choice_set.id.clone(),
            selected_option_id: "option-1".to_owned(),
            expected_session_revision: choices.session.revision,
            selected_at_ms: 22,
        });
        let refining = store
            .commit_choice_selection(&selection, 1, 22)
            .expect("pending refinement");
        let operation = refining
            .pending_refinement_operation
            .clone()
            .expect("operation");
        assert_eq!(
            operation.source_envelope_id,
            begin.accepted.source_envelope_id
        );
        assert_eq!(
            operation.conversation_turn_batch_id,
            begin.accepted.conversation_turn_batch_id
        );
        let interpretation = InterpretationFrame {
            choice_session_id: refining.session.id.clone(),
            revision: 2,
            understood_goal: "Refined bounded local plan".to_owned(),
            current_context: "The owner selected one direction".to_owned(),
            assumptions: vec![],
            constraints: vec![],
            uncertainties: vec![],
            what_to_avoid: vec![],
            source_manifest_digest: refining.document_manifest.aggregate_digest.clone(),
        };
        let mut result = ChoiceRefinementResult {
            operation_id: operation.id.clone(),
            selection_id: operation.selection_id.clone(),
            source_envelope_id: operation.source_envelope_id.clone(),
            conversation_turn_batch_id: operation.conversation_turn_batch_id.clone(),
            expected_session_revision: operation.expected_session_revision,
            expected_generation: operation.expected_generation,
            model_provenance: operation.model_provenance.clone(),
            source_manifest_digest: refining.document_manifest.aggregate_digest.clone(),
            persona_revision: operation.persona_revision.clone(),
            interpretation: interpretation.clone(),
            choice_set: ChoiceSet {
                id: "choices-refined-1".to_owned(),
                choice_session_id: refining.session.id.clone(),
                session_revision: refining.session.revision + 1,
                interpretation_revision: interpretation.revision,
                generated_at_ms: 23,
                expires_on_revision: refining.session.revision + 1,
                options: choice_set.options.clone(),
                d_available: true,
                source_manifest_digest: refining.document_manifest.aggregate_digest.clone(),
                model_provenance: operation.model_provenance.clone(),
                persona_revision: operation.persona_revision.clone(),
            },
            result_digest: String::new(),
            completed_at_ms: 23,
        };
        result.result_digest = result
            .canonical_result_digest()
            .expect("canonical refinement digest");
        let mut wrong_persona = result.clone();
        let forged_persona = PersonaRevisionRef {
            persona_id: operation.persona_revision.persona_id.clone(),
            revision: "draft-04-en".to_owned(),
            aggregate_digest: "a".repeat(64),
            instructions_digest: "b".repeat(64),
        };
        wrong_persona.persona_revision = forged_persona.clone();
        wrong_persona.choice_set.persona_revision = forged_persona;
        wrong_persona.result_digest = wrong_persona
            .canonical_result_digest()
            .expect("forged but self-consistent digest");
        assert!(matches!(
            store.commit_choice_refinement_result(&wrong_persona),
            Err(StoreError::ChoiceLoopStateConflict)
        ));
        let active = store
            .commit_choice_refinement_result(&result)
            .expect("commit exact result");
        assert_eq!(active.session.state, ChoiceSessionState::Active);
        assert!(active.pending_refinement_operation.is_none());
        assert_eq!(
            store
                .commit_choice_refinement_result(&result)
                .expect("exact result replay"),
            active
        );
        let mut changed = result.clone();
        changed.selection_id = "selection-other".to_owned();
        changed.result_digest = changed.canonical_result_digest().expect("changed digest");
        assert!(matches!(
            store.commit_choice_refinement_result(&changed),
            Err(StoreError::ChoiceLoopStateConflict)
        ));
        store
            .connection
            .execute(
                "UPDATE choice_refinement_result SET blob_hash = ?1 WHERE operation_id = ?2",
                ["00".repeat(32), operation.id.clone()],
            )
            .expect("tamper retained result");
        // Retained private results are audited inputs until their typed body
        // retirement. A later unrelated transition must fail closed too.
        assert!(matches!(
            store.cancel_choice_session(1, 24),
            Err(StoreError::ChoiceLoopStateConflict)
        ));
    }

    #[test]
    fn pending_refinement_keeps_the_selected_semantic_context_private_and_bound() {
        let authority = LocalAuthority::from_master("choice-refinement-context", [93_u8; 32]);
        let mut store = Store::open_in_memory(authority.clone()).expect("open store");
        enable_runtime(&mut store, &authority);
        let (_begin, choices, choice_set) =
            active_choice_from_authenticated_begin(&mut store, "request-refinement-context");
        let selection = Selection::OptionSelection(OptionSelection {
            id: "selection-context-1".to_owned(),
            choice_session_id: choices.session.id.clone(),
            choice_set_id: choice_set.id.clone(),
            selected_option_id: "option-2".to_owned(),
            expected_session_revision: choices.session.revision,
            selected_at_ms: 22,
        });
        let refining = store
            .commit_choice_selection(&selection, 1, 22)
            .expect("selection and context commit atomically");
        let operation = refining.pending_refinement_operation.expect("operation");
        let context = store
            .choice_refinement_context(&operation)
            .expect("private context");
        assert_eq!(
            context.interpretation,
            choices.interpretation.expect("interpretation")
        );
        assert_eq!(
            context.selected_option.expect("selected option").direction,
            "Narrow the task"
        );
        assert!(
            store
                .choice_d_intake_for_refinement(&operation)
                .expect("A/B/C has no D input")
                .is_none()
        );
        store
            .connection
            .execute(
                "UPDATE choice_refinement_context SET blob_hash = ?1 WHERE operation_id = ?2",
                ["00".repeat(32), operation.id.clone()],
            )
            .expect("tamper context");
        assert!(matches!(
            store.choice_refinement_context(&operation),
            Err(StoreError::ChoiceLoopStateConflict)
        ));
        // A private worker row must also fence unrelated command-owned
        // transitions. Otherwise a later command could advance durable state
        // while a tampered semantic context remained live in the Store.
        assert!(matches!(
            store.cancel_choice_session(1, 23),
            Err(StoreError::ChoiceLoopStateConflict)
        ));
    }

    #[test]
    fn stale_reminder_schedule_is_history_not_current_choice_continuity() {
        let authority = LocalAuthority::from_master("choice-stale-schedule", [93_u8; 32]);
        let mut store = Store::open_in_memory(authority.clone()).expect("open store");
        enable_runtime(&mut store, &authority);
        let (_begin, initial, initial_choice_set) =
            active_choice_from_authenticated_begin(&mut store, "request-stale-schedule");
        let first_selection = Selection::OptionSelection(OptionSelection {
            id: "selection-before-schedule".to_owned(),
            choice_session_id: initial.session.id.clone(),
            choice_set_id: initial_choice_set.id.clone(),
            selected_option_id: "option-3".to_owned(),
            expected_session_revision: initial.session.revision,
            selected_at_ms: initial.session.last_input_at_ms,
        });
        let refining = store
            .commit_choice_selection(&first_selection, 1, initial.session.last_input_at_ms)
            .expect("commit first selected direction");
        let operation = refining
            .pending_refinement_operation
            .clone()
            .expect("pending refinement");
        let refinement = refinement_result_for(&refining, &operation, &initial_choice_set);
        let choices = store
            .commit_choice_refinement_result(&refinement)
            .expect("activate refined selected direction");
        let choice_set = choices
            .active_choice_set
            .clone()
            .expect("refined choice set");
        let confirmation = confirmation_for(&choices, &choice_set, 100_000);
        let schedule = record_schedule_for(
            &mut store,
            &choices,
            &confirmation,
            "stale-schedule-request",
            22,
        );
        assert_eq!(
            store
                .current_choice_reminder_schedule_for_revision(
                    &choices.session.id,
                    choices.session.revision,
                )
                .expect("current schedule"),
            Some(schedule)
        );
        let selection = Selection::OptionSelection(OptionSelection {
            id: "selection-after-schedule".to_owned(),
            choice_session_id: choices.session.id.clone(),
            choice_set_id: choice_set.id.clone(),
            selected_option_id: "option-1".to_owned(),
            expected_session_revision: choices.session.revision,
            selected_at_ms: 23,
        });
        let refining = store
            .commit_choice_selection(&selection, 1, 23)
            .expect("new Choice revision");
        assert!(
            store
                .current_choice_reminder_schedule_for_revision(
                    &refining.session.id,
                    refining.session.revision,
                )
                .expect("stale schedule lookup")
                .is_none()
        );
        assert!(
            store
                .current_choice_reminder_schedule(&refining.session.id)
                .expect("history remains auditable")
                .is_some()
        );
    }

    #[test]
    #[allow(clippy::too_many_lines)] // One ordered regression proves atomic bind, exact replay, drift rejection, and zero effects.
    fn choice_confirm_binds_the_exact_payload_without_effects() {
        let authority = LocalAuthority::from_master("choice-confirm", [93_u8; 32]);
        let directory = tempdir().expect("temporary Store root");
        let database = directory.path().join("choice-confirm.sqlite");
        let mut store = Store::open(&database, authority.clone()).expect("open store");
        enable_runtime(&mut store, &authority);
        let first = snapshot();
        store
            .save_choice_loop_snapshot(&first, 20)
            .expect("persist base");
        let mut choices = choice_set_snapshot(&first, 21);
        choices.last_selection = Some(Selection::OptionSelection(OptionSelection {
            id: "selection-1".to_owned(),
            choice_session_id: choices.session.id.clone(),
            choice_set_id: choices
                .active_choice_set
                .as_ref()
                .expect("choice set")
                .id
                .clone(),
            selected_option_id: "option-1".to_owned(),
            expected_session_revision: choices.session.revision - 1,
            selected_at_ms: 21,
        }));
        let choice_set = choices.active_choice_set.clone().expect("choice set");
        store
            .save_choice_loop_snapshot(&choices, 21)
            .expect("persist choices");
        store
            .select_model_selection(&matching_model_selection(&choice_set), 21)
            .expect("persist exact model provenance");
        let confirmation = confirmation_for(&choices, &choice_set, 22);
        let schedule = record_schedule_for(
            &mut store,
            &choices,
            &confirmation,
            "schedule-request-1",
            21,
        );
        assert_eq!(schedule.revision, 1);
        let mut unbound_delivery = confirmation.clone();
        unbound_delivery.delivery_binding_id = Some("binding-not-current".to_owned());
        unbound_delivery.recipient = Some("recipient-1".to_owned());
        unbound_delivery.delivery_scope = Some("same-surface only".to_owned());
        unbound_delivery.payload_digest = unbound_delivery
            .canonical_payload_digest()
            .expect("canonical unbound delivery digest");
        assert!(matches!(
            store.commit_choice_confirmation(&unbound_delivery, 1, 22),
            Err(StoreError::ChoiceLoopStateConflict)
        ));
        let committed = store
            .commit_choice_confirmation(&confirmation, 1, 22)
            .expect("commit confirmation");
        assert_eq!(
            committed.session.state,
            ChoiceSessionState::AwaitingConfirmation
        );
        assert_eq!(
            committed.session.pending_confirmation_id.as_deref(),
            Some("confirmation-1")
        );
        assert_eq!(committed.confirmation, Some(confirmation.clone()));
        assert_eq!(
            store
                .commit_choice_confirmation(&confirmation, 1, 22)
                .expect("exact confirmation replay"),
            committed
        );
        let pending_intent = store
            .pending_markdown_render_intent_for_session(&committed.session.id)
            .expect("read atomic confirmation journal")
            .expect("confirmation and render intent commit together");
        assert!(markdown_intent_matches_confirmation(
            &pending_intent,
            &confirmation
        ));
        let intent_rows: i64 = store
            .connection
            .query_row(
                "SELECT COUNT(*) FROM choice_markdown_render_intent",
                [],
                |row| row.get(0),
            )
            .expect("render intent count");
        let intent_audits: i64 = store
            .connection
            .query_row(
                "SELECT COUNT(*) FROM audit_ledger WHERE action = ?1",
                [CHOICE_MARKDOWN_RENDER_INTENT_ACTION],
                |row| row.get(0),
            )
            .expect("render intent audit count");
        assert_eq!((intent_rows, intent_audits), (1, 1));
        let mission_count: i64 = store
            .connection
            .query_row("SELECT COUNT(*) FROM mission_state", [], |row| row.get(0))
            .expect("mission count");
        assert_eq!(mission_count, 0);
        let mut drifted = confirmation.clone();
        drifted.goal = "A changed goal".to_owned();
        assert!(matches!(
            store.commit_choice_confirmation(&drifted, 1, 22),
            Err(StoreError::ChoiceLoopStateConflict)
        ));
        let mut reminder_drifted = confirmation.clone();
        reminder_drifted.reminder_items[0].due_at_ms += 1;
        reminder_drifted.reminder_payload_digest = reminder_drifted
            .canonical_reminder_payload_digest()
            .expect("canonical changed reminder digest");
        reminder_drifted.payload_digest = reminder_drifted
            .canonical_payload_digest()
            .expect("canonical changed confirmation digest");
        assert!(matches!(
            store.commit_choice_confirmation(&reminder_drifted, 1, 22),
            Err(StoreError::ChoiceLoopStateConflict)
        ));
        let mut persona_drifted = confirmation.clone();
        persona_drifted.persona_revision.aggregate_digest = "a".repeat(64);
        persona_drifted.payload_digest = persona_drifted
            .canonical_payload_digest()
            .expect("canonical changed Persona digest");
        assert!(matches!(
            store.commit_choice_confirmation(&persona_drifted, 1, 22),
            Err(StoreError::ChoiceLoopStateConflict)
        ));
        drop(store);
        let mut restarted = Store::open(&database, authority).expect("restart Store");
        assert_eq!(
            restarted
                .commit_choice_confirmation(&confirmation, 1, 23)
                .expect("exact immutable confirmation replay after restart"),
            committed
        );
    }

    #[test]
    fn choice_reminder_schedule_is_future_bound_revisioned_and_reconfirmation_gated() {
        let authority = LocalAuthority::from_master("choice-reminder-schedule", [93_u8; 32]);
        let mut store = Store::open_in_memory(authority.clone()).expect("open store");
        enable_runtime(&mut store, &authority);
        let first = snapshot();
        store.save_choice_loop_snapshot(&first, 20).expect("base");
        let mut choices = choice_set_snapshot(&first, 21);
        let choice_set = choices.active_choice_set.clone().expect("choice set");
        choices.last_selection = Some(Selection::OptionSelection(OptionSelection {
            id: "selection-1".to_owned(),
            choice_session_id: choices.session.id.clone(),
            choice_set_id: choice_set.id.clone(),
            selected_option_id: "option-1".to_owned(),
            expected_session_revision: choices.session.revision - 1,
            selected_at_ms: 21,
        }));
        store
            .save_choice_loop_snapshot(&choices, 21)
            .expect("choices");
        store
            .select_model_selection(&matching_model_selection(&choice_set), 21)
            .expect("model");
        let confirmation = confirmation_for(&choices, &choice_set, 22);
        let first_schedule = record_schedule_for(
            &mut store,
            &choices,
            &confirmation,
            "schedule-request-1",
            21,
        );
        assert_eq!(
            record_schedule_for(
                &mut store,
                &choices,
                &confirmation,
                "schedule-request-1",
                21,
            ),
            first_schedule,
            "exact schedule request is replay-safe"
        );
        assert_eq!(
            store
                .record_choice_reminder_schedule(&first_schedule.input, 1, 100)
                .expect("a lost response may replay its exact durable proposal"),
            first_schedule,
            "an expired proposal still cannot be recreated or mutated by retry"
        );
        let mut changed_input = first_schedule.input.clone();
        changed_input.due_at_ms = 23;
        changed_input.reminder_count = 2;
        assert!(matches!(
            store.record_choice_reminder_schedule(&changed_input, 1, 21),
            Err(StoreError::ChoiceLoopStateConflict)
        ));
        changed_input.request_id = "schedule-request-2".to_owned();
        let second_schedule = store
            .record_choice_reminder_schedule(&changed_input, 1, 21)
            .expect("edited schedule creates a new revision");
        assert_eq!(second_schedule.revision, 2);
        assert_eq!(second_schedule.input.reminder_count, 2);
        assert!(matches!(
            store.commit_choice_confirmation(&confirmation, 1, 22),
            Err(StoreError::ChoiceLoopStateConflict)
        ));
        let mut reconfirmed = confirmation.clone();
        reconfirmed.reminder_items[0].due_at_ms = second_schedule.input.due_at_ms;
        let mut second_item = reconfirmed.reminder_items[0].clone();
        second_item.id = "reminder-item-2".to_owned();
        reconfirmed.reminder_items.push(second_item);
        reconfirmed.reminder_count = second_schedule.input.reminder_count;
        reconfirmed.reminder_payload_digest = reconfirmed
            .canonical_reminder_payload_digest()
            .expect("updated reminder digest");
        reconfirmed.payload_revision = reconfirmed
            .canonical_payload_revision(second_schedule.revision)
            .expect("updated confirmation revision");
        reconfirmed.payload_digest = reconfirmed
            .canonical_payload_digest()
            .expect("updated confirmation digest");
        assert_eq!(
            store
                .commit_choice_confirmation(&reconfirmed, 1, 22)
                .expect("new exact confirmation")
                .confirmation,
            Some(reconfirmed)
        );
        let mission_count: i64 = store
            .connection
            .query_row("SELECT COUNT(*) FROM mission_state", [], |row| row.get(0))
            .expect("mission count");
        assert_eq!(
            mission_count, 0,
            "schedule and confirmation have no effect authority"
        );
    }

    #[test]
    fn choice_reminder_schedule_survives_restart_without_recreating_authority() {
        let directory = tempdir().expect("temporary store directory");
        let path = directory.path().join("choice-reminder-schedule.sqlite3");
        let authority = LocalAuthority::from_master("choice-reminder-restart", [93_u8; 32]);
        let expected = {
            let mut store = Store::open(&path, authority.clone()).expect("open persistent store");
            enable_runtime(&mut store, &authority);
            let first = snapshot();
            store.save_choice_loop_snapshot(&first, 20).expect("base");
            let mut choices = choice_set_snapshot(&first, 21);
            let choice_set = choices.active_choice_set.clone().expect("choice set");
            choices.last_selection = Some(Selection::OptionSelection(OptionSelection {
                id: "selection-1".to_owned(),
                choice_session_id: choices.session.id.clone(),
                choice_set_id: choice_set.id.clone(),
                selected_option_id: "option-1".to_owned(),
                expected_session_revision: choices.session.revision - 1,
                selected_at_ms: 21,
            }));
            store
                .save_choice_loop_snapshot(&choices, 21)
                .expect("choices");
            store
                .select_model_selection(&matching_model_selection(&choice_set), 21)
                .expect("model");
            let confirmation = confirmation_for(&choices, &choice_set, 22);
            record_schedule_for(
                &mut store,
                &choices,
                &confirmation,
                "schedule-request-restart",
                21,
            )
        };
        let reopened = Store::open(&path, authority).expect("reopen schedule store");
        assert_eq!(
            reopened
                .current_choice_reminder_schedule("session-1")
                .expect("read persisted schedule"),
            Some(expected)
        );
    }

    #[test]
    fn choice_reminder_schedule_indexes_are_authenticated_and_store_rejects_invalid_time_zone() {
        for (column, value) in [
            ("request_id", "tampered-request"),
            ("choice_session_id", "tampered-session"),
            ("revision", "99"),
            ("accepted_at_ms", "99"),
        ] {
            let (mut store, schedule) = store_with_reminder_schedule_for_tamper();
            let statement =
                format!("UPDATE choice_reminder_schedule SET {column} = ?1 WHERE schedule_id = ?2");
            store
                .connection
                .execute(&statement, params![value, schedule.id])
                .expect("tamper plaintext schedule index");
            assert!(
                load_choice_reminder_schedule(&store.connection, &store.authority, &schedule.id)
                    .is_err(),
                "{column} must be authenticated before index lookup can trust it"
            );
            assert!(
                store
                    .record_choice_reminder_schedule(&schedule.input, 1, 100)
                    .is_err(),
                "tampering must not permit a duplicate idempotent schedule write"
            );
            let count: i64 = store
                .connection
                .query_row("SELECT COUNT(*) FROM choice_reminder_schedule", [], |row| {
                    row.get(0)
                })
                .expect("schedule count");
            assert_eq!(count, 1, "tampering never creates a second schedule");
        }

        let (mut store, schedule) = store_with_reminder_schedule_for_tamper();
        let mut invalid = schedule.input.clone();
        invalid.request_id = "schedule-invalid-zone".to_owned();
        invalid.time_zone = "Invalid/Zone".to_owned();
        assert!(
            invalid.is_valid(),
            "protocol shape alone permits a bounded string"
        );
        assert!(matches!(
            store.record_choice_reminder_schedule(&invalid, 1, 21),
            Err(StoreError::ChoiceLoopStateConflict)
        ));
    }

    #[test]
    fn confirmed_markdown_receipt_reaches_a_durable_effect_free_next_state() {
        let (store, confirmation, _intent, next) =
            store_with_executing_markdown_receipt("choice-confirm-render");
        assert_eq!(next.session.state, ChoiceSessionState::SoftIdle);
        assert_eq!(next.confirmation.as_ref(), Some(&confirmation));
        assert_eq!(
            next.document_manifest.aggregate_digest,
            confirmation.markdown_manifest_digests[1]
        );
        assert_eq!(
            next.document_manifest.entries,
            vec![confirmation.markdown_entry.clone()]
        );
        assert_eq!(
            next.session.pending_confirmation_id.as_deref(),
            Some(confirmation.id.as_str())
        );
        assert_eq!(
            store.choice_loop_snapshot().unwrap(),
            Some(next),
            "restart observes the reachable post-render state without repeating work"
        );
    }

    #[test]
    fn executing_choice_continuity_rejects_missing_markdown_proof_rows() {
        let (store, _, intent, _) =
            store_with_executing_markdown_receipt("choice-confirm-missing-receipt");
        store
            .connection
            .execute(
                "DELETE FROM choice_markdown_render_receipt WHERE intent_id = ?1",
                [&intent.id],
            )
            .unwrap();
        assert!(matches!(
            store.choice_loop_snapshot(),
            Err(StoreError::StateBindingMismatch(_) | StoreError::ChoiceLoopStateConflict)
        ));

        let (store, _, intent, _) =
            store_with_executing_markdown_receipt("choice-confirm-missing-journal");
        store
            .connection
            .execute(
                "DELETE FROM choice_markdown_render_receipt WHERE intent_id = ?1",
                [&intent.id],
            )
            .unwrap();
        store
            .connection
            .execute(
                "DELETE FROM choice_markdown_render_intent WHERE intent_id = ?1",
                [&intent.id],
            )
            .unwrap();
        assert!(matches!(
            store.choice_loop_snapshot(),
            Err(StoreError::StateBindingMismatch(_) | StoreError::ChoiceLoopStateConflict)
        ));
    }

    #[test]
    fn markdown_reconciliation_returns_to_reconfirm_and_retires_the_old_journal() {
        let authority = LocalAuthority::from_master("choice-confirm-reconcile", [93_u8; 32]);
        let mut store = Store::open_in_memory(authority.clone()).expect("open store");
        enable_runtime(&mut store, &authority);
        let first = snapshot();
        store.save_choice_loop_snapshot(&first, 20).unwrap();
        let mut choices = choice_set_snapshot(&first, 21);
        let choice_set = choices.active_choice_set.clone().unwrap();
        choices.last_selection = Some(Selection::OptionSelection(OptionSelection {
            id: "selection-1".to_owned(),
            choice_session_id: choices.session.id.clone(),
            choice_set_id: choice_set.id.clone(),
            selected_option_id: "option-1".to_owned(),
            expected_session_revision: choices.session.revision - 1,
            selected_at_ms: 21,
        }));
        store.save_choice_loop_snapshot(&choices, 21).unwrap();
        store
            .select_model_selection(&matching_model_selection(&choice_set), 21)
            .unwrap();
        let first_confirmation = confirmation_for(&choices, &choice_set, 10_000);
        record_schedule_for(
            &mut store,
            &choices,
            &first_confirmation,
            "schedule-request-reconcile-1",
            21,
        );
        let (awaiting, first_intent) = store
            .commit_choice_confirmation_and_render_intent(&first_confirmation, 1, 22)
            .unwrap();
        assert_eq!(
            awaiting.session.state,
            ChoiceSessionState::AwaitingConfirmation
        );

        store
            .record_markdown_reconciliation(&first_intent.id, 23)
            .expect("descriptor conflict becomes durable re-review state");
        let recovered = store
            .choice_loop_snapshot()
            .unwrap()
            .expect("recovered Choice snapshot");
        assert_eq!(recovered.session.state, ChoiceSessionState::Active);
        assert_eq!(recovered.session.pending_confirmation_id, None);
        assert_eq!(recovered.confirmation, None);
        let recovery_set = recovered
            .active_choice_set
            .clone()
            .expect("exact alternatives remain reachable for re-review");
        assert_eq!(recovery_set.session_revision, recovered.session.revision);

        let mut replacement = first_confirmation.clone();
        replacement.id = "confirmation-2".to_owned();
        replacement.choice_set_id = recovery_set.id.clone();
        replacement.expected_session_revision = recovered.session.revision;
        replacement.confirmed_at_ms = 20_000;
        replacement.reminder_items[0].due_at_ms = 20_000;
        replacement.reminder_payload_digest = replacement
            .canonical_reminder_payload_digest()
            .expect("replacement reminder digest");
        let schedule = record_schedule_for(
            &mut store,
            &recovered,
            &replacement,
            "schedule-request-reconcile-2",
            24,
        );
        replacement.payload_revision = replacement
            .canonical_payload_revision(schedule.revision)
            .expect("replacement confirmation revision");
        replacement.payload_digest = replacement
            .canonical_payload_digest()
            .expect("replacement confirmation digest");

        let (_, replacement_intent) = store
            .commit_choice_confirmation_and_render_intent(&replacement, 1, 25)
            .expect("explicit re-confirmation creates one replacement journal");
        assert_ne!(replacement_intent.id, first_intent.id);
        assert_eq!(
            store
                .pending_markdown_render_intent_for_session(&recovered.session.id)
                .unwrap(),
            Some(replacement_intent)
        );
        assert!(
            load_markdown_render_intent(&store.connection, &store.authority, &first_intent.id)
                .unwrap()
                .is_some_and(|record| record.plaintext_body.is_none()),
            "the reconciled journal keeps only authenticated body-free metadata"
        );
        assert_eq!(
            store
                .connection
                .query_row("SELECT COUNT(*) FROM mission_state", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }

    #[test]
    fn markdown_reconciliation_reverifies_live_private_rows_before_state_advance() {
        let (mut store, d_record, operation, refining, choices) =
            refining_d_store("choice-reconcile-private-row");
        let result = refinement_result_for(&refining, &operation, &choices);
        let active = store
            .commit_choice_refinement_result(&result)
            .expect("commit bound refinement result");
        let choice_set = active.active_choice_set.clone().unwrap();
        let mut confirmation = confirmation_for(&active, &choice_set, 10_000);
        // The D batch is bound to the interactive owner surface, so its
        // confirmation fixture must carry that exact complete delivery tuple.
        confirmation.delivery_binding_id = active.session.primary_delivery_binding_id.clone();
        confirmation.recipient = Some("owner-self".to_owned());
        confirmation.delivery_scope = Some("same-surface only".to_owned());
        confirmation.payload_revision = confirmation
            .canonical_payload_revision(1)
            .expect("canonical bound confirmation revision");
        confirmation.payload_digest = confirmation
            .canonical_payload_digest()
            .expect("canonical bound confirmation digest");
        record_schedule_for(
            &mut store,
            &active,
            &confirmation,
            "schedule-request-private-row",
            22,
        );
        let (awaiting, intent) = store
            .commit_choice_confirmation_and_render_intent(&confirmation, 1, 23)
            .expect("commit confirmation and journal");
        let audit_count: i64 = store
            .connection
            .query_row("SELECT COUNT(*) FROM audit_ledger", [], |row| row.get(0))
            .unwrap();
        store
            .connection
            .execute(
                "DELETE FROM choice_d_request WHERE request_id = ?1",
                [&d_record.input.request_id],
            )
            .expect("delete one audited live private row");

        assert!(
            store
                .record_markdown_reconciliation(&intent.id, 24)
                .is_err()
        );
        assert_eq!(
            load_choice_loop_snapshot(&store.connection, &store.authority).unwrap(),
            Some(awaiting),
            "failed reconciliation cannot advance the raw durable snapshot"
        );
        assert!(
            load_markdown_render_intent(&store.connection, &store.authority, &intent.id)
                .unwrap()
                .is_some_and(|record| record.reconciliation.is_none()),
            "failed reconciliation cannot persist a false marker"
        );
        assert_eq!(
            store
                .connection
                .query_row("SELECT COUNT(*) FROM audit_ledger", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            audit_count,
            "failed reconciliation appends no audit entry"
        );
        assert!(store.choice_loop_snapshot().is_err());
    }

    #[test]
    fn choice_confirmation_rejects_current_catalog_drift_without_partial_commit() {
        let authority = LocalAuthority::from_master("choice-confirm-catalog", [93_u8; 32]);
        let mut store = Store::open_in_memory(authority.clone()).expect("open store");
        enable_runtime(&mut store, &authority);
        let first = snapshot();
        store
            .save_choice_loop_snapshot(&first, 20)
            .expect("persist base");
        let choices = choice_set_snapshot(&first, 21);
        let choice_set = choices.active_choice_set.clone().expect("choice set");
        store
            .save_choice_loop_snapshot(&choices, 21)
            .expect("persist choices");
        store
            .select_model_selection(&matching_model_selection(&choice_set), 21)
            .expect("persist exact model provenance");

        let mut drifted_selection = matching_model_selection(&choice_set);
        drifted_selection.catalog_fingerprint = "f".repeat(64);
        drifted_selection.catalog_revision = 2;
        store
            .select_model_selection(&drifted_selection, 22)
            .expect("persist catalog drift for test");
        let confirmation = confirmation_for(&choices, &choice_set, 23);
        assert!(matches!(
            store.commit_choice_confirmation(&confirmation, 1, 23),
            Err(StoreError::ChoiceLoopStateConflict)
        ));
        assert_eq!(
            store
                .choice_loop_snapshot()
                .expect("read unchanged choice state"),
            Some(choices)
        );
    }

    #[test]
    fn historical_batch_without_binding_migrates_once_to_durable_blocked_recovery() {
        let directory = tempdir().expect("temporary store directory");
        let path = directory.path().join("choice-loop-legacy.sqlite3");
        let authority = LocalAuthority::from_master("choice-loop-legacy", [111_u8; 32]);
        let legacy_snapshot = batch_snapshot(&snapshot(), 21);
        assert!(legacy_snapshot.is_valid());
        {
            let mut store = Store::open(&path, authority.clone()).expect("open persistent store");
            store
                .save_choice_loop_snapshot(&legacy_snapshot, 21)
                .expect("persist bound batch before historical projection");
            let mut historical = serde_json::to_value(&legacy_snapshot)
                .expect("serialize historical choice snapshot");
            historical
                .get_mut("activeBatch")
                .and_then(serde_json::Value::as_object_mut)
                .expect("historical active batch")
                .remove("deliveryBindingId");
            let blob = store
                .authority
                .encrypt_json(&historical, choice_loop_state_aad().as_bytes())
                .expect("encrypt historical snapshot");
            let encrypted_blob_hash = blob_hash(&blob);
            let command_hash = format!(
                "{:x}",
                Sha256::digest(
                    serde_json::to_vec(&historical).expect("serialize historical command hash")
                )
            );
            let transaction = store
                .connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .expect("begin historical projection transaction");
            transaction
                .execute(
                    "UPDATE choice_loop_state SET encrypted_blob = ?1, blob_hash = ?2, updated_at_ms = 22 WHERE singleton_id = 1",
                    params![blob, encrypted_blob_hash],
                )
                .expect("replace with historical shape");
            append_audit(
                &transaction,
                &store.authority,
                &AuditRecord {
                    id: "choice-loop-state-legacy-missing-binding-22",
                    command_id: "choice-loop-state-legacy-missing-binding-22",
                    command_hash: &command_hash,
                    actor: "owner",
                    action: CHOICE_LOOP_STATE_ACTION,
                    entity_id: "choice-loop",
                    created_at_ms: 22,
                    state_kind: "choice:loop_state",
                    state_hash: &encrypted_blob_hash,
                },
            )
            .expect("audit historical shape");
            transaction.commit().expect("commit historical shape");
        }

        let reopened = Store::open(&path, authority).expect("migrate legacy batch at open");
        let migrated = reopened
            .choice_loop_snapshot()
            .expect("load migrated snapshot")
            .expect("migrated snapshot exists");
        assert_eq!(migrated.session.state, ChoiceSessionState::Blocked);
        assert_eq!(
            migrated
                .active_batch
                .as_ref()
                .map(|batch| batch.delivery_binding_id.as_str()),
            Some("blocked-missing-binding")
        );
        let raw = load_raw_choice_loop_snapshot(&reopened.connection, &reopened.authority)
            .expect("load durable migration projection")
            .expect("durable migrated raw snapshot");
        assert!(!raw_choice_loop_batch_lacks_delivery_binding(&raw));
    }

    #[test]
    fn choice_loop_snapshot_is_atomic_audited_idempotent_and_tamper_evident() {
        let authority = LocalAuthority::from_master("choice-loop", [92_u8; 32]);
        let mut store = Store::open_in_memory(authority).expect("open store");
        let first = snapshot();
        assert_eq!(
            store
                .save_choice_loop_snapshot(&first, 20)
                .expect("persist choice loop"),
            first
        );
        assert_eq!(
            store.choice_loop_snapshot().expect("load snapshot"),
            Some(first.clone())
        );
        assert_eq!(
            store
                .save_choice_loop_snapshot(&first, 21)
                .expect("idempotent retry"),
            first
        );
        let audit_count: i64 = store
            .connection
            .query_row(
                "SELECT COUNT(*) FROM audit_ledger WHERE action = ?1",
                [CHOICE_LOOP_STATE_ACTION],
                |row| row.get(0),
            )
            .expect("audit count");
        assert_eq!(audit_count, 1);

        let mut changed = first.clone();
        changed.session.revision = 2;
        changed.session.last_input_at_ms = 21;
        changed.session.soft_idle_at_ms = 21 + CHOICE_SESSION_SOFT_IDLE_MS;
        changed.session.stale_review_at_ms = 21 + CHOICE_SESSION_STALE_REVIEW_MS;
        store
            .save_choice_loop_snapshot(&changed, 21)
            .expect("persist revision advance");
        assert_eq!(
            store.choice_loop_snapshot().expect("load changed"),
            Some(changed)
        );

        store
            .connection
            .execute(
                "UPDATE choice_loop_state SET blob_hash = ?1 WHERE singleton_id = 1",
                ["00".repeat(32)],
            )
            .expect("tamper snapshot hash");
        assert!(matches!(
            store.choice_loop_snapshot(),
            Err(StoreError::ChoiceLoopStateConflict)
        ));
    }

    #[test]
    fn choice_loop_rejects_stale_choice_set_and_never_persists_partial_state() {
        let authority = LocalAuthority::from_master("choice-loop-stale", [93_u8; 32]);
        let mut store = Store::open_in_memory(authority).expect("open store");
        let mut invalid = snapshot();
        invalid.session.active_choice_set_id = Some("choices-1".to_owned());
        assert!(matches!(
            store.save_choice_loop_snapshot(&invalid, 20),
            Err(StoreError::ChoiceLoopStateConflict)
        ));
        assert_eq!(
            store.choice_loop_snapshot().expect("no partial state"),
            None
        );
    }

    #[test]
    fn choice_loop_rejects_rollback_equal_revision_and_timestamp_rewrites_atomically() {
        let authority = LocalAuthority::from_master("choice-loop-monotonic", [94_u8; 32]);
        let mut store = Store::open_in_memory(authority).expect("open store");
        let first = snapshot();
        store
            .save_choice_loop_snapshot(&first, 20)
            .expect("persist first snapshot");
        let second = advance_snapshot(&first, 21);
        store
            .save_choice_loop_snapshot(&second, 21)
            .expect("persist exact successor");

        assert!(matches!(
            store.save_choice_loop_snapshot(&first, 22),
            Err(StoreError::ChoiceLoopStateConflict)
        ));
        assert_eq!(
            store
                .choice_loop_snapshot()
                .expect("rollback left state intact"),
            Some(second.clone())
        );

        let mut same_revision_mutation = second.clone();
        same_revision_mutation.session.last_input_at_ms = 22;
        same_revision_mutation.session.soft_idle_at_ms = 22 + CHOICE_SESSION_SOFT_IDLE_MS;
        same_revision_mutation.session.stale_review_at_ms = 22 + CHOICE_SESSION_STALE_REVIEW_MS;
        assert!(same_revision_mutation.is_valid());
        assert!(matches!(
            store.save_choice_loop_snapshot(&same_revision_mutation, 22),
            Err(StoreError::ChoiceLoopStateConflict)
        ));

        let mut timestamp_regression = advance_snapshot(&second, 22);
        timestamp_regression.document_manifest.generated_at_ms = 20;
        assert!(timestamp_regression.is_valid());
        assert!(matches!(
            store.save_choice_loop_snapshot(&timestamp_regression, 22),
            Err(StoreError::ChoiceLoopStateConflict)
        ));
        assert_eq!(
            store
                .choice_loop_snapshot()
                .expect("all rejected writes are atomic"),
            Some(second)
        );
    }

    #[test]
    fn choice_loop_retires_old_choice_sets_instead_of_rebinding_them_to_a_new_revision() {
        let authority = LocalAuthority::from_master("choice-loop-choices", [95_u8; 32]);
        let mut store = Store::open_in_memory(authority).expect("open store");
        let first = snapshot();
        store
            .save_choice_loop_snapshot(&first, 20)
            .expect("persist first snapshot");
        let choices = choice_set_snapshot(&first, 21);
        store
            .save_choice_loop_snapshot(&choices, 21)
            .expect("persist choice set");

        let mut resurrected = choice_set_snapshot(&choices, 22);
        let choice_set = resurrected
            .active_choice_set
            .as_mut()
            .expect("active choice set");
        choice_set.session_revision = resurrected.session.revision;
        choice_set.generated_at_ms = 22;
        choice_set.expires_on_revision = resurrected.session.revision;
        assert!(resurrected.is_valid());
        assert!(matches!(
            store.save_choice_loop_snapshot(&resurrected, 22),
            Err(StoreError::ChoiceLoopStateConflict)
        ));
        assert_eq!(
            store
                .choice_loop_snapshot()
                .expect("stale choice set not rebound"),
            Some(choices)
        );
    }

    #[test]
    fn choice_loop_rejects_replayed_snapshot_after_restart() {
        let directory = tempdir().expect("temporary store directory");
        let path = directory.path().join("choice-loop.sqlite3");
        let authority = LocalAuthority::from_master("choice-loop-restart", [96_u8; 32]);
        let first = snapshot();
        let second = advance_snapshot(&first, 21);
        {
            let mut store = Store::open(&path, authority.clone()).expect("open persistent store");
            store
                .save_choice_loop_snapshot(&first, 20)
                .expect("persist first snapshot");
            store
                .save_choice_loop_snapshot(&second, 21)
                .expect("persist successor");
        }
        let mut reopened = Store::open(&path, authority).expect("reopen persistent store");
        assert_eq!(
            reopened
                .choice_loop_snapshot()
                .expect("load latest snapshot"),
            Some(second.clone())
        );
        assert!(matches!(
            reopened.save_choice_loop_snapshot(&first, 22),
            Err(StoreError::ChoiceLoopStateConflict)
        ));
        assert_eq!(
            reopened
                .choice_loop_snapshot()
                .expect("replay leaves latest intact"),
            Some(second)
        );
    }
}

#[cfg(test)]
mod channel_model_mission_recovery_tests {
    use super::*;
    use crate::{
        ApprovalDecision, BrokerEnrollmentRecord, CreateMission, CreateWorkItem,
        TrustedBrokerEnrollment, broker_enrollment_signing_bytes,
    };
    use ed25519_dalek::{Signer, SigningKey};
    use openopen_protocol::{
        ChannelCursor, ChannelEnvelope, ChannelModelDisposition, ChannelObservation,
        ChannelPairing, EFFECT_PROTOCOL_VERSION, RuntimeControlReceipt,
        runtime_control_authorization_hash, runtime_control_receipt_signing_bytes,
    };

    fn test_authority() -> LocalAuthority {
        LocalAuthority::from_master("openopen-core", [55_u8; 32])
    }

    fn broker_signing_key() -> SigningKey {
        SigningKey::from_bytes(&[56_u8; 32])
    }

    fn trusted_broker(authority: &LocalAuthority) -> TrustedBrokerEnrollment {
        let broker_key = broker_signing_key().verifying_key().to_bytes();
        let mut record = BrokerEnrollmentRecord {
            version: 1,
            broker_key_id: format!("{:x}", Sha256::digest(broker_key)),
            broker_verifying_key_hex: hex::encode(broker_key),
            helper_designated_requirement_digest: "cd".repeat(32),
            installed_at_ms: 1,
            core_key_id: authority.effect_key_id(),
            core_authorization_signature_hex: String::new(),
        };
        let mut derivation = b"openopen-effect-authorizer-v1".to_vec();
        derivation.extend([55_u8; 32]);
        let signing_key = SigningKey::from_bytes(&Sha256::digest(derivation).into());
        record.core_authorization_signature_hex = hex::encode(
            signing_key
                .sign(&broker_enrollment_signing_bytes(&record).unwrap())
                .to_bytes(),
        );
        TrustedBrokerEnrollment::from_signed_install_record(authority, &record).unwrap()
    }

    fn enable_runtime(store: &mut Store) {
        let authorization = store.prepare_runtime_control(true, 1).unwrap();
        let broker = broker_signing_key();
        let mut receipt = RuntimeControlReceipt {
            protocol_version: EFFECT_PROTOCOL_VERSION,
            authorization_hash: runtime_control_authorization_hash(&authorization).unwrap(),
            checkpoint_nonce: "90".repeat(32),
            request_nonce: None,
            broker_key_id: format!("{:x}", Sha256::digest(broker.verifying_key().to_bytes())),
            broker_signature_hex: String::new(),
        };
        receipt.broker_signature_hex = hex::encode(
            broker
                .sign(&runtime_control_receipt_signing_bytes(&receipt).unwrap())
                .to_bytes(),
        );
        store
            .commit_runtime_control(&authorization, &receipt)
            .unwrap();
    }

    fn execute(store: &mut Store, command_id: &str, command: MissionCommand) {
        let expected_anchor = store.current_verified_audit_anchor().unwrap();
        store
            .execute_mission_command(&MissionCommandEnvelope {
                command_id: command_id.into(),
                expected_anchor,
                command,
            })
            .unwrap();
    }

    fn seed_active_mission(store: &mut Store) {
        execute(
            store,
            "legacy-active-create",
            MissionCommand::Create {
                input: CreateMission {
                    mission_id: "mission-legacy-active".into(),
                    title: "Legacy active Mission".into(),
                    outcome: "Preserve recovery order".into(),
                    owner_id: "owner-legacy".into(),
                    scope_digest: "scope-legacy-active".into(),
                    scope_approval_id: "scope-legacy-active".into(),
                    scope_approval_prompt: "Approve this Mission?".into(),
                    work_items: vec![CreateWorkItem {
                        id: "work-legacy-active".into(),
                        title: "Recover without replay".into(),
                    }],
                    now_ms: 2,
                },
            },
        );
        execute(
            store,
            "legacy-active-confirm",
            MissionCommand::BeginConfirmation {
                mission_id: "mission-legacy-active".into(),
                now_ms: 3,
            },
        );
        execute(
            store,
            "legacy-active-approve",
            MissionCommand::DecideApproval {
                mission_id: "mission-legacy-active".into(),
                approval_id: "scope-legacy-active".into(),
                actor_id: "owner-legacy".into(),
                decision: ApprovalDecision::Approve,
                now_ms: 4,
            },
        );
        execute(
            store,
            "legacy-active-activate",
            MissionCommand::Activate {
                mission_id: "mission-legacy-active".into(),
                now_ms: 5,
            },
        );
    }

    fn observation(id: u64) -> (ChannelObservation, String) {
        let content = format!("@OpenOpen legacy message {id}");
        (
            ChannelObservation {
                envelope: ChannelEnvelope {
                    channel: ChannelKind::IMessage,
                    source_message_id: format!("legacy-message-{id}"),
                    sender_id: "owner-imessage".into(),
                    conversation_id: "chat-imessage".into(),
                    content_sha256: format!("{:x}", Sha256::digest(content.as_bytes())),
                    received_at_ms: i64::try_from(id + 5).unwrap(),
                },
                cursor: ChannelCursor {
                    channel: ChannelKind::IMessage,
                    conversation_id: "chat-imessage".into(),
                    opaque_value: format!("legacy-cursor-{id}"),
                    order: id,
                    observed_at_ms: i64::try_from(id + 5).unwrap(),
                },
                is_bot: false,
                explicitly_addressed: true,
            },
            content,
        )
    }

    #[test]
    fn legacy_started_dispatch_recovers_before_nonterminal_mission_gate() {
        let authority = test_authority();
        let enrollment = trusted_broker(&authority);
        let mut store = Store::open_in_memory_with_trusted_broker(authority, enrollment).unwrap();
        enable_runtime(&mut store);
        seed_active_mission(&mut store);
        store
            .pair_channel(&ChannelPairing {
                channel: ChannelKind::IMessage,
                owner_sender_id: "owner-imessage".into(),
                conversation_id: "chat-imessage".into(),
                require_explicit_address: true,
                imessage: None,
                discord: None,
                paired_at_ms: 6,
            })
            .unwrap();
        let (first, first_content) = observation(1);
        store
            .ingest_channel_message(&first, &first_content)
            .unwrap();

        // Recreate the exact state produced by the historical activation race:
        // a queued dispatch was already claimed before the Mission existed.
        let transaction = store
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        let queued = load_channel_model_dispatch(
            &transaction,
            &store.authority,
            ChannelKind::IMessage,
            "legacy-message-1",
        )
        .unwrap()
        .unwrap();
        update_channel_model_started(
            &transaction,
            &store.authority,
            &StoredChannelModelDispatch {
                state: StoredChannelModelState::Started,
                ..queued
            },
            7,
        )
        .unwrap();
        transaction.commit().unwrap();

        assert_eq!(
            store
                .begin_channel_model(ChannelKind::IMessage, "legacy-message-1")
                .unwrap()
                .disposition,
            ChannelModelDisposition::RecoverOnly
        );
        store
            .fail_channel_model(ChannelKind::IMessage, "legacy-message-1", 8)
            .unwrap();
        assert_eq!(
            store
                .channel_failure_incidents(Some(ChannelKind::IMessage))
                .unwrap()
                .len(),
            1
        );

        let (second, second_content) = observation(2);
        store
            .ingest_channel_message(&second, &second_content)
            .unwrap();
        assert!(matches!(
            store.begin_channel_model(ChannelKind::IMessage, "legacy-message-2"),
            Err(StoreError::ChannelModelDeferredByMission)
        ));
    }
}

#[cfg(test)]
mod migration_tests {
    use super::*;
    use crate::{CreateMission, CreateWorkItem};
    use tempfile::tempdir;

    const LEGACY_ORIGIN_SCHEMA: &str = "CREATE TABLE channel_mission_origin (
        mission_id TEXT PRIMARY KEY, channel_json TEXT NOT NULL,
        conversation_id TEXT NOT NULL, source_message_id TEXT NOT NULL,
        encrypted_blob BLOB NOT NULL, blob_hash TEXT NOT NULL
    );";

    fn test_authority() -> LocalAuthority {
        LocalAuthority::from_master("openopen-core", [73_u8; 32])
    }

    fn seed_legacy_origin(path: &Path) {
        let authority = test_authority();
        let mut store = Store::open(path, authority).expect("open current store");
        store
            .execute_mission_command(&MissionCommandEnvelope {
                command_id: "legacy-create".into(),
                expected_anchor: None,
                command: MissionCommand::Create {
                    input: CreateMission {
                        mission_id: "mission-legacy".into(),
                        title: "Legacy Mission".into(),
                        outcome: "Migrate exactly once".into(),
                        owner_id: "owner-legacy".into(),
                        scope_digest: "scope-legacy".into(),
                        scope_approval_id: "scope-mission-legacy".into(),
                        scope_approval_prompt: "Approve?".into(),
                        work_items: vec![CreateWorkItem {
                            id: "work-legacy".into(),
                            title: "Migrate".into(),
                        }],
                        now_ms: 1,
                    },
                },
            })
            .expect("seed Mission");
        store
            .pair_channel(&ChannelPairing {
                channel: ChannelKind::IMessage,
                owner_sender_id: "owner-legacy".into(),
                conversation_id: "chat-legacy".into(),
                require_explicit_address: true,
                imessage: None,
                discord: None,
                paired_at_ms: 2,
            })
            .expect("seed pairing");
        store
            .connection
            .execute_batch(LEGACY_ORIGIN_SCHEMA)
            .expect("create legacy origin table");
        let legacy = LegacyChannelMissionOrigin {
            mission_id: "mission-legacy".into(),
            channel: ChannelKind::IMessage,
            conversation_id: "chat-legacy".into(),
            owner_sender_id: "owner-legacy".into(),
            source_message_id: "message-legacy".into(),
            bound_at_ms: 3,
        };
        let blob = store
            .authority
            .encrypt_json(&legacy, channel_origin_aad(&legacy.mission_id).as_bytes())
            .expect("encrypt legacy origin");
        let state_hash = blob_hash(&blob);
        let transaction = store.connection.transaction().expect("begin legacy seed");
        append_audit(
            &transaction,
            &store.authority,
            &AuditRecord {
                id: "channel:mission-legacy:origin",
                command_id: "channel-origin-mission-legacy",
                command_hash: &state_hash,
                actor: "owner-legacy",
                action: LEGACY_CHANNEL_ORIGIN_ACTION,
                entity_id: "mission-legacy",
                created_at_ms: 3,
                state_kind: "channelMissionOrigin",
                state_hash: &state_hash,
            },
        )
        .expect("audit legacy origin");
        transaction
            .execute(
                "INSERT INTO channel_mission_origin
                 (mission_id, channel_json, conversation_id, source_message_id,
                  encrypted_blob, blob_hash)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    legacy.mission_id,
                    channel_json(legacy.channel).expect("channel JSON"),
                    legacy.conversation_id,
                    legacy.source_message_id,
                    blob,
                    state_hash,
                ],
            )
            .expect("insert legacy origin");
        transaction.commit().expect("commit legacy seed");
    }

    fn table_exists(store: &Store, table: &str) -> bool {
        store
            .connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
                [table],
                |row| row.get(0),
            )
            .expect("table lookup")
    }

    #[test]
    fn empty_legacy_origin_table_is_removed_without_a_compatibility_reader() {
        let directory = tempdir().expect("temp directory");
        let path = directory.path().join("empty-legacy.sqlite3");
        let connection = Connection::open(&path).expect("open seed database");
        connection.execute_batch(STORE_SCHEMA).expect("seed schema");
        connection
            .execute_batch(LEGACY_ORIGIN_SCHEMA)
            .expect("seed empty legacy table");
        drop(connection);

        let store = Store::open(&path, test_authority()).expect("migrate empty legacy table");
        assert!(!table_exists(&store, "channel_mission_origin"));
        assert!(!table_exists(&store, "channel_mission_origin_legacy"));
    }

    #[test]
    fn one_legacy_origin_migrates_to_one_primary_route_and_archives_once() {
        let directory = tempdir().expect("temp directory");
        let path = directory.path().join("one-legacy.sqlite3");
        seed_legacy_origin(&path);

        let store = Store::open(&path, test_authority()).expect("migrate legacy origin");
        let route_set = store
            .channel_route_set("mission-legacy")
            .expect("read route set")
            .expect("route set exists");
        assert_eq!(route_set.revision, 1);
        assert_eq!(route_set.routes.len(), 1);
        assert_eq!(route_set.routes[0].role, ChannelRouteRole::Primary);
        assert_eq!(route_set.routes[0].channel, ChannelKind::IMessage);
        assert_eq!(
            route_set.routes[0].source_message_id.as_deref(),
            Some("message-legacy")
        );
        assert!(!table_exists(&store, "channel_mission_origin"));
        assert!(table_exists(&store, "channel_mission_origin_legacy"));
        drop(store);

        let reopened = Store::open(&path, test_authority()).expect("reopen migrated store");
        assert_eq!(
            reopened
                .channel_route_set("mission-legacy")
                .expect("read reopened route set")
                .expect("reopened route set")
                .routes
                .len(),
            1
        );
    }

    #[test]
    fn invalid_legacy_origin_fails_closed_without_partial_migration() {
        let directory = tempdir().expect("temp directory");
        let path = directory.path().join("invalid-legacy.sqlite3");
        seed_legacy_origin(&path);
        let connection = Connection::open(&path).expect("open corrupt database");
        connection
            .execute(
                "UPDATE channel_mission_origin SET blob_hash = ?1 WHERE mission_id = ?2",
                params!["00".repeat(32), "mission-legacy"],
            )
            .expect("corrupt legacy hash");
        drop(connection);

        assert!(matches!(
            Store::open(&path, test_authority()),
            Err(StoreError::ChannelStateMismatch(value)) if value == "mission-legacy"
        ));
        let connection = Connection::open(&path).expect("inspect failed migration");
        let legacy_exists: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'channel_mission_origin')",
                [],
                |row| row.get(0),
            )
            .expect("legacy table lookup");
        let route_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM channel_route_set", [], |row| {
                row.get(0)
            })
            .expect("route count");
        assert!(legacy_exists);
        assert_eq!(route_count, 0);
    }

    #[test]
    fn legacy_plaintext_body_retirement_migrates_to_a_typed_blocked_marker() {
        let directory = tempdir().expect("temp directory");
        let path = directory.path().join("legacy-private-retirement.sqlite3");
        let store = Store::open(&path, test_authority()).expect("create current schema");
        store
            .connection
            .execute(
                "INSERT INTO choice_private_body_tombstone
                 (source_kind, request_id, request_digest, body_digest, choice_session_id, retired_at_ms)
                 VALUES ('begin', 'legacy-request', ?1, ?2, 'legacy-session', 1)",
                params!["a".repeat(64), "b".repeat(64)],
            )
            .expect("seed untrusted legacy marker");
        drop(store);

        let reopened = Store::open(&path, test_authority())
            .expect("legacy tombstone must not make the whole Store unavailable");
        assert!(
            choice_private_body_tombstone_exists(
                &reopened.connection,
                &reopened.authority,
                "begin",
                "legacy-request",
            )
            .expect("load authenticated blocked marker"),
            "the old request remains fail-closed rather than becoming replay authority"
        );
        let connection = Connection::open(&path).expect("inspect failed migration");
        let authoritative_rows: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM choice_private_body_retirement",
                [],
                |row| row.get(0),
            )
            .expect("authoritative retirement count");
        assert_eq!(authoritative_rows, 1, "migration seals one blocked marker");
        // Deleting the authenticated record must not make the mutable legacy
        // row a fresh migration source. The durable retirement audit means a
        // missing record is tampering, not an invitation to seal whatever
        // happens to be in the old table now.
        connection
            .execute(
                "DELETE FROM choice_private_body_retirement WHERE source_kind = 'begin' AND entity_id = 'legacy-request'",
                [],
            )
            .expect("remove authoritative marker to simulate row tampering");
        connection
            .execute(
                "UPDATE choice_private_body_tombstone SET body_digest = ?1 WHERE request_id = 'legacy-request'",
                ["c".repeat(64)],
            )
            .expect("tamper legacy row");
        drop(connection);
        assert!(
            Store::open(&path, test_authority()).is_err(),
            "a changed legacy row cannot be silently accepted after migration"
        );
    }

    #[test]
    fn pre_identity_private_tables_upgrade_only_when_no_unbound_rows_exist() {
        const OLD_PRIVATE_SCHEMA: &str = "
CREATE TABLE choice_begin_request (
 request_id TEXT PRIMARY KEY, request_digest TEXT NOT NULL, choice_session_id TEXT NOT NULL UNIQUE,
 operation_id TEXT NOT NULL UNIQUE, encrypted_blob BLOB NOT NULL, blob_hash TEXT NOT NULL,
 accepted_at_ms INTEGER NOT NULL
);
CREATE TABLE choice_d_request (
 request_id TEXT PRIMARY KEY, request_digest TEXT NOT NULL,
 operation_id TEXT NOT NULL UNIQUE, encrypted_blob BLOB NOT NULL,
 blob_hash TEXT NOT NULL, accepted_at_ms INTEGER NOT NULL
);
CREATE TABLE choice_refinement_result (
 operation_id TEXT PRIMARY KEY, selection_id TEXT NOT NULL, result_digest TEXT NOT NULL,
 encrypted_blob BLOB NOT NULL, blob_hash TEXT NOT NULL, completed_at_ms INTEGER NOT NULL
);";
        let empty_directory = tempdir().expect("empty migration directory");
        let empty_path = empty_directory.path().join("empty-private.sqlite3");
        let connection = Connection::open(&empty_path).expect("open empty legacy database");
        connection
            .execute_batch(OLD_PRIVATE_SCHEMA)
            .expect("seed empty pre-identity tables");
        drop(connection);
        let upgraded = Store::open(&empty_path, test_authority())
            .expect("empty tables may acquire stronger identity columns");
        for (table, column) in [
            ("choice_begin_request", "source_envelope_id"),
            ("choice_begin_request", "conversation_turn_batch_id"),
            ("choice_d_request", "choice_session_id"),
            ("choice_d_request", "source_envelope_id"),
            ("choice_d_request", "conversation_turn_batch_id"),
            ("choice_refinement_result", "choice_session_id"),
        ] {
            let columns = upgraded
                .connection
                .prepare(&format!("PRAGMA table_info({table})"))
                .expect("inspect upgraded table")
                .query_map([], |row| row.get::<_, String>(1))
                .expect("read upgraded columns")
                .collect::<Result<Vec<_>, _>>()
                .expect("collect upgraded columns");
            assert!(columns.iter().any(|value| value == column));
        }

        let occupied_directory = tempdir().expect("occupied migration directory");
        let occupied_path = occupied_directory.path().join("occupied-private.sqlite3");
        let connection = Connection::open(&occupied_path).expect("open occupied database");
        connection
            .execute_batch(OLD_PRIVATE_SCHEMA)
            .expect("seed occupied pre-identity tables");
        connection
            .execute(
                "INSERT INTO choice_begin_request
                 (request_id, request_digest, choice_session_id, operation_id, encrypted_blob,
                  blob_hash, accepted_at_ms)
                 VALUES ('request-1', ?1, 'session-1', 'operation-1', X'00', ?2, 1)",
                params!["a".repeat(64), "b".repeat(64)],
            )
            .expect("seed an unauthenticated pre-identity row");
        drop(connection);
        assert!(matches!(
            Store::open(&occupied_path, test_authority()),
            Err(StoreError::ChoiceLoopStateConflict)
        ));
    }

    #[test]
    fn legacy_plaintext_idle_clock_is_never_promoted_into_time_authority() {
        const OLD_CLOCK_SCHEMA: &str = "CREATE TABLE choice_idle_clock_anchor (
            singleton_id INTEGER PRIMARY KEY CHECK (singleton_id = 1),
            boot_id TEXT NOT NULL, wall_clock_ms INTEGER NOT NULL,
            monotonic_ms INTEGER NOT NULL
        );";

        let empty_directory = tempdir().expect("empty clock migration directory");
        let empty_path = empty_directory.path().join("empty-clock.sqlite3");
        Connection::open(&empty_path)
            .unwrap()
            .execute_batch(OLD_CLOCK_SCHEMA)
            .unwrap();
        let upgraded = Store::open(&empty_path, test_authority())
            .expect("empty unshipped clock table upgrades mechanically");
        let columns = upgraded
            .connection
            .prepare("PRAGMA table_info(choice_idle_clock_anchor)")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert!(columns.iter().any(|name| name == "encrypted_blob"));
        assert!(columns.iter().any(|name| name == "blob_hash"));
        assert!(!columns.iter().any(|name| name == "boot_id"));

        let occupied_directory = tempdir().expect("occupied clock migration directory");
        let occupied_path = occupied_directory.path().join("occupied-clock.sqlite3");
        let connection = Connection::open(&occupied_path).unwrap();
        connection.execute_batch(OLD_CLOCK_SCHEMA).unwrap();
        connection
            .execute(
                "INSERT INTO choice_idle_clock_anchor
                 (singleton_id, boot_id, wall_clock_ms, monotonic_ms)
                 VALUES (1, 'legacy-boot', 100, 100)",
                [],
            )
            .unwrap();
        drop(connection);
        assert!(matches!(
            Store::open(&occupied_path, test_authority()),
            Err(StoreError::ChoiceClockUncertain)
        ));
    }
}
