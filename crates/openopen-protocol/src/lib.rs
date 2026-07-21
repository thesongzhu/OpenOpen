//! Stable local protocol shared by the Swift host and Rust core.
//!
//! The transport is newline-delimited JSON over stdio. This crate contains data
//! only; it performs no external effects and stores no credentials.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

pub const CHOICE_BATCH_QUIET_WINDOW_MS: i64 = 2_500;
pub const CHOICE_BATCH_HARD_WINDOW_MS: i64 = 8_000;
pub const CHOICE_SESSION_SOFT_IDLE_MS: i64 = 30 * 60 * 1_000;
pub const CHOICE_SESSION_STALE_REVIEW_MS: i64 = 24 * 60 * 60 * 1_000;

/// Exact, immutable conversation behavior used for one accepted turn.
///
/// This reference is provenance only. It grants no model, memory, channel,
/// permission, recipient, confirmation, or effect authority.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PersonaRevisionRef {
    pub persona_id: String,
    pub revision: String,
    pub aggregate_digest: String,
    /// Digest of the exact deterministic developer-instruction payload
    /// compiled from this verified revision. This binds model requests to the
    /// complete compiler output without granting any Persona lifecycle power.
    pub instructions_digest: String,
}

impl PersonaRevisionRef {
    #[must_use]
    pub fn is_valid(&self) -> bool {
        bounded_identifier(&self.persona_id)
            && self.revision.len() <= 64
            && bounded_identifier(&self.revision)
            && sha256_hex(&self.aggregate_digest)
            && sha256_hex(&self.instructions_digest)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ConversationSurface {
    MacApp,
    IMessageSelfChat,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ConversationTurnKind {
    Greeting,
    Question,
    Request,
    Correction,
    EmotionalSupport,
    Permission,
    Confirmation,
    Progress,
    Completion,
    Failure,
    Return,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ConversationAmbiguityClass {
    None,
    SmallReversible,
    MissingRequiredValue,
    MaterialPreview,
    MaterialOutcome,
    ExternalEffect,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ConversationRiskClass {
    Ordinary,
    Sensitive,
    HighStakes,
    Irreversible,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ConversationUrgency {
    Normal,
    TimeSensitive,
    Immediate,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RequestedDetail {
    Concise,
    Standard,
    Detailed,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ActiveTaskState {
    None,
    Running,
    SafelyPaused,
    WaitingForUser,
    Completed,
    Failed,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TransientSupportLevel {
    Ordinary,
    ReducedLoad,
    Emotional,
}

/// Host-owned conversation classification. Model interpretation may advise
/// these values, but only Host constructs the accepted routing context.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConversationContext {
    pub surface: ConversationSurface,
    pub turn_kind: ConversationTurnKind,
    pub ambiguity_class: ConversationAmbiguityClass,
    pub risk_class: ConversationRiskClass,
    pub urgency: ConversationUrgency,
    pub requested_detail: RequestedDetail,
    pub active_task_state: ActiveTaskState,
    pub return_interval_ms: Option<u64>,
    pub transient_support_level: TransientSupportLevel,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ConversationDecision {
    Direct,
    Clarify,
    Choice,
    EditablePreview,
    NeedUser,
    Progress,
    Receipt,
    SafeFailure,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ConversationPreferenceKind {
    Brevity,
    Guidance,
    Register,
    Warmth,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ConversationPreferenceScope {
    SessionOnly,
    Durable,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConversationPreferenceCandidate {
    pub kind: ConversationPreferenceKind,
    pub value: String,
    pub scope: ConversationPreferenceScope,
}

impl ConversationPreferenceCandidate {
    #[must_use]
    pub fn is_valid(&self) -> bool {
        bounded_text(&self.value, 128)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConversationChoice {
    pub id: String,
    pub label: String,
    pub outcome: String,
    pub tradeoff: String,
    pub recommended: bool,
}

impl ConversationChoice {
    #[must_use]
    pub fn is_valid(&self) -> bool {
        bounded_identifier(&self.id)
            && bounded_text(&self.label, 80)
            && bounded_text(&self.outcome, 240)
            && bounded_text(&self.tradeoff, 240)
    }
}

/// Surface-neutral conversational output. iMessage identity is added by the
/// shared renderer, not supplied by a model or adapter.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RenderedMessage {
    pub primary_answer: String,
    pub explanation: Option<String>,
    pub choices: Vec<ConversationChoice>,
    pub next_step: Option<String>,
    pub technical_details: Option<String>,
    pub persona: PersonaRevisionRef,
}

impl RenderedMessage {
    #[must_use]
    pub fn is_valid(&self) -> bool {
        bounded_text(&self.primary_answer, 4_096)
            && self
                .explanation
                .as_ref()
                .is_none_or(|value| bounded_text(value, 4_096))
            && self.choices.len() <= 4
            && self.choices.iter().all(ConversationChoice::is_valid)
            && self
                .next_step
                .as_ref()
                .is_none_or(|value| bounded_text(value, 1_024))
            && self
                .technical_details
                .as_ref()
                .is_none_or(|value| bounded_text(value, 8_192))
            && self.persona.is_valid()
    }
}

/// The bounded foreground session used by the private-agent Choice Loop.
///
/// This type is deliberately separate from a Mission: choosing a direction is
/// conversation state and never grants effect authority.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ChoiceSessionState {
    Interpreting,
    Active,
    Refining,
    SoftIdle,
    StaleReview,
    AwaitingConfirmation,
    Executing,
    Completed,
    Cancelled,
    Blocked,
}

impl ChoiceSessionState {
    #[must_use]
    pub const fn accepts_input(self) -> bool {
        matches!(
            self,
            Self::Active | Self::Refining | Self::SoftIdle | Self::StaleReview
        )
    }
}

/// The only caller-supplied fields accepted when a Mac user starts the first
/// foreground Choice session. Every owner, envelope, batch, session, audit,
/// and operation identity is derived by Host.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChoiceBeginRequest {
    pub request_id: String,
    pub bounded_local_question: String,
    pub expected_model_provenance_ref: String,
    pub expected_catalog_fingerprint: String,
    pub expected_catalog_revision: u64,
    pub expected_protocol_revision: u64,
}

impl ChoiceBeginRequest {
    #[must_use]
    pub fn is_valid(&self) -> bool {
        bounded_identifier(&self.request_id)
            && bounded_text(&self.bounded_local_question, 4_096)
            && bounded_identifier(&self.expected_model_provenance_ref)
            && sha256_hex(&self.expected_catalog_fingerprint)
            && self.expected_catalog_revision > 0
            && self.expected_protocol_revision > 0
    }

    /// A stable idempotency binding over exactly the untrusted input fields.
    #[must_use]
    pub fn request_digest(&self) -> Option<String> {
        self.is_valid()
            .then(|| serde_json::to_vec(self).ok())
            .flatten()
            .map(|bytes| format!("{:x}", Sha256::digest(bytes)))
    }
}

/// The bounded replay-safe acknowledgement for a first local question. It
/// grants neither model execution nor any external effect.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChoiceBeginAccepted {
    pub request_id: String,
    pub operation_id: String,
    pub choice_session_id: String,
    pub accepted_session_revision: u64,
    pub source_envelope_id: String,
    pub conversation_turn_batch_id: String,
    pub state: ChoiceSessionState,
}

impl ChoiceBeginAccepted {
    #[must_use]
    pub fn is_valid(&self) -> bool {
        bounded_identifier(&self.request_id)
            && bounded_identifier(&self.operation_id)
            && bounded_identifier(&self.choice_session_id)
            && self.accepted_session_revision > 0
            && bounded_identifier(&self.source_envelope_id)
            && bounded_identifier(&self.conversation_turn_batch_id)
            && self.state == ChoiceSessionState::Interpreting
    }
}

/// Encrypted Store-only durable material for an accepted initial intake. The
/// Host never returns this record to an adapter or UI. It preserves the exact
/// local question for a later private, provenance-bound result commit.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChoiceBeginRecord {
    pub accepted: ChoiceBeginAccepted,
    pub request_digest: String,
    pub bounded_local_question: String,
    pub source_envelope: SourceEnvelope,
    pub batch: ConversationTurnBatch,
    pub model_selection: ModelSelection,
    pub source_manifest: DocumentManifest,
    /// Conversation behavior accepted with this turn. It is provenance only.
    pub persona_revision: PersonaRevisionRef,
    /// The signed protected runtime revision that admitted this operation.
    /// A later result must not commit after Off/restart has advanced it.
    pub runtime_revision: u64,
    pub accepted_at_ms: i64,
}

impl ChoiceBeginRecord {
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.accepted.is_valid()
            && sha256_hex(&self.request_digest)
            && bounded_text(&self.bounded_local_question, 4_096)
            && self.source_envelope.is_valid()
            && self.batch.is_valid()
            && self.model_selection.is_valid()
            && self.source_manifest.is_valid()
            && self.persona_revision.is_valid()
            && self.runtime_revision > 0
            && self.accepted_at_ms >= 0
            && self.accepted.choice_session_id == self.batch.choice_session_id
            && self.accepted.source_envelope_id == self.source_envelope.id
            && self.accepted.conversation_turn_batch_id == self.batch.id
            && self.batch.source_envelope_ids == [self.source_envelope.id.clone()]
            && self.batch.delivery_binding_id == self.source_envelope.delivery_binding_id
            && self.source_envelope.body_digest
                == format!(
                    "{:x}",
                    Sha256::digest(self.bounded_local_question.as_bytes())
                )
            && self.source_manifest.entries.len() == 1
    }
}

/// A persisted model selection is mandatory before Choice generation can
/// begin. `Unavailable` is a typed recovery state rather than a fallback.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "state", deny_unknown_fields)]
pub enum ModelSelectionState {
    Unselected,
    Selected {
        model_provenance_ref: String,
    },
    Unavailable {
        catalog_revision: u64,
        reason: String,
    },
}

/// A user-confirmed model capability choice. This is persisted before any
/// model work; turn-specific provenance is derived only when Host starts a
/// concrete model turn.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelSelection {
    pub id: String,
    pub model_id: String,
    /// Exact protocol value, or the literal `not_applicable`.
    pub requested_effort: String,
    /// Exact runtime value, or the literal `not_applicable`.
    pub actual_effort: String,
    pub catalog_fingerprint: String,
    pub catalog_revision: u64,
    pub account_display_class: String,
    pub protocol_schema_revision: u64,
}

impl ModelSelection {
    #[must_use]
    pub fn is_valid(&self) -> bool {
        bounded_text(&self.id, 128)
            && valid_model_id(&self.model_id)
            && valid_effort(&self.requested_effort)
            && self.actual_effort == self.requested_effort
            && sha256_hex(&self.catalog_fingerprint)
            && self.catalog_revision > 0
            && bounded_text(&self.account_display_class, 256)
            && self.protocol_schema_revision > 0
    }

    #[must_use]
    pub fn turn_provenance(&self, id: String, turn_id: String) -> Option<ModelProvenance> {
        let provenance = ModelProvenance {
            id,
            model_id: self.model_id.clone(),
            requested_effort: self.requested_effort.clone(),
            actual_effort: self.actual_effort.clone(),
            catalog_fingerprint: self.catalog_fingerprint.clone(),
            catalog_revision: self.catalog_revision,
            account_display_class: self.account_display_class.clone(),
            protocol_schema_revision: self.protocol_schema_revision,
            turn_id,
        };
        provenance.is_valid().then_some(provenance)
    }
}

/// The exact account/catalog/model/effort binding for one model turn.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelProvenance {
    pub id: String,
    pub model_id: String,
    /// Exact protocol value, or the literal `not_applicable`.
    pub requested_effort: String,
    /// Exact protocol value used by the runtime, or `not_applicable`.
    pub actual_effort: String,
    pub catalog_fingerprint: String,
    pub catalog_revision: u64,
    pub account_display_class: String,
    pub protocol_schema_revision: u64,
    pub turn_id: String,
}

impl ModelProvenance {
    #[must_use]
    pub fn is_valid(&self) -> bool {
        bounded_text(&self.id, 128)
            && valid_model_id(&self.model_id)
            && valid_effort(&self.requested_effort)
            && valid_effort(&self.actual_effort)
            && sha256_hex(&self.catalog_fingerprint)
            && self.catalog_revision > 0
            && bounded_text(&self.account_display_class, 256)
            && self.protocol_schema_revision > 0
            && bounded_text(&self.turn_id, 128)
    }
}

/// An authenticated owner or local-Mac input accepted by Host. Adapter input
/// is data only; Host derives the binding and owner fields before persistence.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceEnvelope {
    pub id: String,
    pub surface: String,
    pub delivery_binding_id: String,
    pub provider_message_id: Option<String>,
    pub owner_id: String,
    pub received_at_ms: i64,
    pub monotonic_sequence: u64,
    pub body_digest: String,
    pub attachment_manifest: Option<String>,
    pub third_party_data: bool,
    pub session_hint: Option<String>,
    pub schema_version: u64,
}

impl SourceEnvelope {
    #[must_use]
    pub fn is_valid(&self) -> bool {
        bounded_identifier(&self.id)
            && bounded_text(&self.surface, 64)
            && bounded_identifier(&self.delivery_binding_id)
            && self
                .provider_message_id
                .as_ref()
                .is_none_or(|value| bounded_identifier(value))
            && bounded_identifier(&self.owner_id)
            && self.received_at_ms >= 0
            && self.monotonic_sequence > 0
            && sha256_hex(&self.body_digest)
            && self
                .attachment_manifest
                .as_ref()
                .is_none_or(|value| sha256_hex(value))
            && self
                .session_hint
                .as_ref()
                .is_none_or(|value| bounded_identifier(value))
            && self.schema_version > 0
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum BatchSealReason {
    InitialIntake,
    QuietDeadline,
    HardDeadline,
    AttachmentContinuation,
    ImmediateOff,
    ImmediateCancel,
    ImmediateConfirm,
    ImmediateRefinement,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConversationTurnBatch {
    pub id: String,
    pub choice_session_id: String,
    /// Durable Host-derived authority of the first accepted envelope. Adapters
    /// and message bodies never supply this value directly.
    pub delivery_binding_id: String,
    pub source_envelope_ids: Vec<String>,
    pub opened_at_ms: i64,
    pub quiet_deadline_ms: i64,
    pub hard_deadline_ms: i64,
    pub sealed_at_ms: Option<i64>,
    pub seal_reason: Option<BatchSealReason>,
    pub revision: u64,
}

impl ConversationTurnBatch {
    #[must_use]
    pub fn is_valid(&self) -> bool {
        let source_ids = self
            .source_envelope_ids
            .iter()
            .map(String::as_str)
            .collect::<std::collections::BTreeSet<_>>();
        let seals_together = matches!(
            (self.sealed_at_ms, self.seal_reason),
            (Some(_), Some(_)) | (None, None)
        );
        bounded_identifier(&self.id)
            && bounded_identifier(&self.choice_session_id)
            && bounded_identifier(&self.delivery_binding_id)
            && self.source_envelope_ids.len() <= 64
            && !self.source_envelope_ids.is_empty()
            && source_ids.len() == self.source_envelope_ids.len()
            && self
                .source_envelope_ids
                .iter()
                .all(|id| bounded_identifier(id))
            && self.opened_at_ms >= 0
            && self.quiet_deadline_ms == self.opened_at_ms + CHOICE_BATCH_QUIET_WINDOW_MS
            && self.hard_deadline_ms == self.opened_at_ms + CHOICE_BATCH_HARD_WINDOW_MS
            && self.quiet_deadline_ms <= self.hard_deadline_ms
            && self
                .sealed_at_ms
                .is_none_or(|value| value >= self.opened_at_ms && value <= self.hard_deadline_ms)
            && seals_together
            && batch_seal_is_consistent(
                self.sealed_at_ms,
                self.seal_reason,
                self.quiet_deadline_ms,
                self.hard_deadline_ms,
            )
            && self.revision > 0
    }
}

/// The single global foreground session. Background Missions remain separate
/// and do not create a second interactive session.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChoiceSession {
    pub id: String,
    pub state: ChoiceSessionState,
    pub revision: u64,
    pub model_selection_state: ModelSelectionState,
    pub communication_profile_revision: u64,
    pub active_choice_set_id: Option<String>,
    pub active_interpretation_revision: Option<u64>,
    pub opened_at_ms: i64,
    pub last_input_at_ms: i64,
    pub soft_idle_at_ms: i64,
    pub stale_review_at_ms: i64,
    pub primary_delivery_binding_id: Option<String>,
    pub pending_confirmation_id: Option<String>,
    pub background_mission_ids: Vec<String>,
}

impl ChoiceSession {
    #[must_use]
    pub fn is_valid(&self) -> bool {
        let background_missions = self
            .background_mission_ids
            .iter()
            .map(String::as_str)
            .collect::<std::collections::BTreeSet<_>>();
        bounded_identifier(&self.id)
            && self.revision > 0
            && self.model_selection_state.is_valid()
            && self.opened_at_ms >= 0
            && self.last_input_at_ms >= self.opened_at_ms
            && self.soft_idle_at_ms == self.last_input_at_ms + CHOICE_SESSION_SOFT_IDLE_MS
            && self.stale_review_at_ms == self.last_input_at_ms + CHOICE_SESSION_STALE_REVIEW_MS
            && self
                .active_choice_set_id
                .as_ref()
                .is_none_or(|value| bounded_identifier(value))
            && self
                .active_interpretation_revision
                .is_none_or(|value| value > 0)
            && self
                .primary_delivery_binding_id
                .as_ref()
                .is_none_or(|value| bounded_identifier(value))
            && self
                .pending_confirmation_id
                .as_ref()
                .is_none_or(|value| bounded_identifier(value))
            && self.background_mission_ids.len() <= 32
            && background_missions.len() == self.background_mission_ids.len()
            && self
                .background_mission_ids
                .iter()
                .all(|value| bounded_identifier(value))
    }
}

impl ModelSelectionState {
    #[must_use]
    pub fn is_valid(&self) -> bool {
        match self {
            Self::Unselected => true,
            Self::Selected {
                model_provenance_ref,
            } => bounded_identifier(model_provenance_ref),
            Self::Unavailable {
                catalog_revision,
                reason,
            } => *catalog_revision > 0 && bounded_text(reason, 512),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChoiceOption {
    pub id: String,
    /// One-based fixed presentation position for A/B/C.
    pub position: u8,
    pub direction: String,
    pub rationale: String,
    pub expected_result: String,
    pub information_needed: Vec<String>,
    pub external_effects_preview: Vec<String>,
    pub source_categories: Vec<String>,
}

impl ChoiceOption {
    #[must_use]
    pub fn is_valid(&self) -> bool {
        (1..=3).contains(&self.position)
            && bounded_identifier(&self.id)
            && bounded_text(&self.direction, 512)
            && bounded_text(&self.rationale, 1_024)
            && bounded_text(&self.expected_result, 1_024)
            && bounded_list(&self.information_needed, 16, 512)
            && bounded_list(&self.external_effects_preview, 16, 512)
            && bounded_list(&self.source_categories, 16, 128)
    }
}

/// A model-generated set contains exactly three materially distinct options;
/// D is product-owned and therefore has no model-controlled label or ID.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChoiceSet {
    pub id: String,
    pub choice_session_id: String,
    pub session_revision: u64,
    pub interpretation_revision: u64,
    pub generated_at_ms: i64,
    pub expires_on_revision: u64,
    pub options: Vec<ChoiceOption>,
    pub d_available: bool,
    pub source_manifest_digest: String,
    pub model_provenance: ModelProvenance,
    pub persona_revision: PersonaRevisionRef,
}

impl ChoiceSet {
    #[must_use]
    pub fn is_valid(&self) -> bool {
        if !bounded_identifier(&self.id)
            || !bounded_identifier(&self.choice_session_id)
            || self.session_revision == 0
            || self.interpretation_revision == 0
            || self.generated_at_ms < 0
            || !sha256_hex(&self.source_manifest_digest)
            || self.expires_on_revision < self.session_revision
            || !self.d_available
            || self.options.len() != 3
            || self.options.iter().any(|option| !option.is_valid())
            || !self.persona_revision.is_valid()
        {
            return false;
        }
        let positions = self
            .options
            .iter()
            .map(|option| option.position)
            .collect::<std::collections::BTreeSet<_>>();
        let directions = self
            .options
            .iter()
            .map(|option| option.direction.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        positions.len() == 3 && directions.len() == 3 && self.model_provenance.is_valid()
    }
}

/// Host-internal only result material for the first model turn. There is no
/// public RPC parameter for this shape: the Host binds it to the accepted
/// intake operation, generation, session revision, selected provenance, and
/// source manifest before Store may create the first `ChoiceSet`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChoiceInitialResult {
    pub operation_id: String,
    pub expected_session_revision: u64,
    pub expected_generation: u64,
    pub model_provenance: ModelProvenance,
    pub source_manifest_digest: String,
    pub persona_revision: PersonaRevisionRef,
    pub interpretation: InterpretationFrame,
    pub choice_set: ChoiceSet,
    pub completed_at_ms: i64,
}

impl ChoiceInitialResult {
    #[must_use]
    pub fn is_valid(&self) -> bool {
        bounded_identifier(&self.operation_id)
            && self.expected_session_revision > 0
            && self.expected_generation > 0
            && self.model_provenance.is_valid()
            && sha256_hex(&self.source_manifest_digest)
            && self.persona_revision.is_valid()
            && self.interpretation.is_valid()
            && self.choice_set.is_valid()
            && self.completed_at_ms >= 0
            && self.choice_set.model_provenance == self.model_provenance
            && self.choice_set.source_manifest_digest == self.source_manifest_digest
            && self.choice_set.persona_revision == self.persona_revision
            && self.interpretation.source_manifest_digest == self.source_manifest_digest
            && self.choice_set.choice_session_id == self.interpretation.choice_session_id
            && self.choice_set.interpretation_revision == self.interpretation.revision
    }
}

/// Host-private output for a selected A/B/C/D refinement.  It deliberately
/// cannot be confused with first-intake output: it binds the exact durable
/// Selection as well as the pending operation and stores an independently
/// verifiable result digest.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChoiceRefinementResult {
    pub operation_id: String,
    pub selection_id: String,
    /// Host-derived identity of the accepted source that created the
    /// refinement. It is never caller supplied and remains part of the
    /// encrypted result binding after the active batch has been retired.
    pub source_envelope_id: String,
    /// Host-derived sealed batch identity paired with `source_envelope_id`.
    pub conversation_turn_batch_id: String,
    pub expected_session_revision: u64,
    pub expected_generation: u64,
    pub model_provenance: ModelProvenance,
    pub source_manifest_digest: String,
    pub persona_revision: PersonaRevisionRef,
    pub interpretation: InterpretationFrame,
    pub choice_set: ChoiceSet,
    pub result_digest: String,
    pub completed_at_ms: i64,
}

impl ChoiceRefinementResult {
    #[must_use]
    pub fn is_valid(&self) -> bool {
        bounded_identifier(&self.operation_id)
            && bounded_identifier(&self.selection_id)
            && bounded_identifier(&self.source_envelope_id)
            && bounded_identifier(&self.conversation_turn_batch_id)
            && self.expected_session_revision > 0
            && self.expected_generation > 0
            && self.model_provenance.is_valid()
            && sha256_hex(&self.source_manifest_digest)
            && self.persona_revision.is_valid()
            && self.interpretation.is_valid()
            && self.choice_set.is_valid()
            && sha256_hex(&self.result_digest)
            && self.canonical_result_digest().as_deref() == Some(&self.result_digest)
            && self.completed_at_ms >= 0
            && self.choice_set.model_provenance == self.model_provenance
            && self.choice_set.source_manifest_digest == self.source_manifest_digest
            && self.choice_set.persona_revision == self.persona_revision
            && self.interpretation.source_manifest_digest == self.source_manifest_digest
            && self.choice_set.choice_session_id == self.interpretation.choice_session_id
            && self.choice_set.interpretation_revision == self.interpretation.revision
    }

    #[must_use]
    pub fn canonical_result_digest(&self) -> Option<String> {
        let mut preimage = self.clone();
        preimage.result_digest.clear();
        serde_json::to_vec(&preimage)
            .ok()
            .map(|bytes| format!("{:x}", Sha256::digest(bytes)))
    }
}

/// Host-private output for an authenticated idle/stale owner return. Keeping
/// this distinct from a selection refinement prevents either private worker
/// from being routed through the other's result entrypoint.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChoiceResumeResult {
    pub result: ChoiceRefinementResult,
}

impl ChoiceResumeResult {
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.result.is_valid()
            && (self.result.selection_id.starts_with("resume-soft-idle-")
                || self.result.selection_id.starts_with("resume-stale-review-"))
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OptionSelection {
    pub id: String,
    pub choice_session_id: String,
    pub choice_set_id: String,
    pub selected_option_id: String,
    pub expected_session_revision: u64,
    pub selected_at_ms: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NaturalConversationSelection {
    pub id: String,
    pub choice_session_id: String,
    pub choice_set_id: String,
    pub d_input_batch_id: String,
    pub expected_session_revision: u64,
    pub selected_at_ms: i64,
}

/// The only untrusted input shape for the product-owned D direction.  The
/// caller names the current session/ChoiceSet fence and contributes bounded
/// text plus an idempotency key; it never supplies an envelope, batch, owner,
/// delivery binding, or Selection identity.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChoiceDInput {
    pub request_id: String,
    pub bounded_text: String,
    pub choice_session_id: String,
    pub choice_set_id: String,
    pub expected_session_revision: u64,
    pub submitted_at_ms: i64,
}

impl ChoiceDInput {
    #[must_use]
    pub fn is_valid(&self) -> bool {
        bounded_identifier(&self.request_id)
            && bounded_text(&self.bounded_text, 4_096)
            && bounded_identifier(&self.choice_session_id)
            && bounded_identifier(&self.choice_set_id)
            && self.expected_session_revision > 0
            && self.submitted_at_ms >= 0
    }

    #[must_use]
    pub fn request_digest(&self) -> Option<String> {
        self.is_valid()
            .then(|| serde_json::to_vec(self).ok())
            .flatten()
            .map(|bytes| format!("{:x}", Sha256::digest(bytes)))
    }
}

/// Store-private D intake assembled by Host after authenticating the local
/// caller.  It is never an RPC parameter: only its encrypted representation
/// may be retained for a pending refinement operation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChoiceDIntakeRecord {
    pub input: ChoiceDInput,
    pub request_digest: String,
    pub source_envelope: SourceEnvelope,
    pub batch: ConversationTurnBatch,
    pub selection: NaturalConversationSelection,
}

impl ChoiceDIntakeRecord {
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.input.is_valid()
            && sha256_hex(&self.request_digest)
            && self.source_envelope.is_valid()
            && self.batch.is_valid()
            && bounded_identifier(&self.selection.id)
            && self.selection.choice_session_id == self.input.choice_session_id
            && self.selection.choice_set_id == self.input.choice_set_id
            && self.selection.expected_session_revision == self.input.expected_session_revision
            // `submitted_at_ms` is an untrusted client-side idempotency
            // field.  The Host owns `selected_at_ms` and records its accepted
            // time independently, so normal RPC latency cannot invalidate an
            // otherwise exact D request.
            && self.selection.selected_at_ms >= 0
            && self.selection.d_input_batch_id == self.batch.id
            && self.batch.choice_session_id == self.input.choice_session_id
            && self.batch.delivery_binding_id == self.source_envelope.delivery_binding_id
            && self.batch.source_envelope_ids == [self.source_envelope.id.clone()]
            && self.source_envelope.body_digest
                == format!("{:x}", Sha256::digest(self.input.bounded_text.as_bytes()))
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "type", deny_unknown_fields)]
pub enum Selection {
    OptionSelection(OptionSelection),
    NaturalConversationSelection(NaturalConversationSelection),
}

/// The private, Host-owned continuation created with every accepted Choice
/// selection.  This is deliberately metadata-only: the Store keeps any D
/// intake body encrypted in its command record and never exposes it through a
/// `ChoiceLoopSnapshot`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChoiceRefinementOperation {
    pub id: String,
    pub selection_id: String,
    pub choice_session_id: String,
    /// The original Host-authenticated envelope. Retaining this identity
    /// prevents an operation from being rebound after `active_batch` is
    /// deliberately cleared during the selection transition.
    pub source_envelope_id: String,
    /// The original Host-sealed batch paired with `source_envelope_id`.
    pub conversation_turn_batch_id: String,
    /// The revision after the selection transaction has committed.
    pub expected_session_revision: u64,
    pub expected_generation: u64,
    pub model_provenance: ModelProvenance,
    pub source_manifest_digest: String,
    pub persona_revision: PersonaRevisionRef,
    /// Present only for the D path.  It is the idempotent request identity,
    /// not a caller-provided envelope or batch identity.
    pub d_request_id: Option<String>,
    /// Present only for the D path and contains no plaintext.
    pub d_input_digest: Option<String>,
    pub created_at_ms: i64,
}

impl ChoiceRefinementOperation {
    /// Resume operations are minted only by the Store from a durable idle
    /// state. The marker is intentionally private protocol state rather than
    /// an RPC field; callers cannot choose an operation id or resume state.
    #[must_use]
    pub fn is_owner_resume(&self) -> bool {
        self.id.starts_with("resume-")
            && (self.selection_id.starts_with("resume-soft-idle-")
                || self.selection_id.starts_with("resume-stale-review-"))
    }

    #[must_use]
    pub fn resume_prior_state(&self) -> Option<ChoiceSessionState> {
        if !self.id.starts_with("resume-") {
            return None;
        }
        self.selection_id
            .strip_prefix("resume-soft-idle-")
            .map(|_| ChoiceSessionState::SoftIdle)
            .or_else(|| {
                self.selection_id
                    .strip_prefix("resume-stale-review-")
                    .map(|_| ChoiceSessionState::StaleReview)
            })
    }

    #[must_use]
    pub fn is_valid(&self) -> bool {
        let d_shape_is_complete = matches!(
            (&self.d_request_id, &self.d_input_digest),
            (None, None) | (Some(_), Some(_))
        );
        bounded_identifier(&self.id)
            && bounded_identifier(&self.selection_id)
            && bounded_identifier(&self.choice_session_id)
            && bounded_identifier(&self.source_envelope_id)
            && bounded_identifier(&self.conversation_turn_batch_id)
            && self.expected_session_revision > 0
            && self.expected_generation > 0
            && self.model_provenance.is_valid()
            && sha256_hex(&self.source_manifest_digest)
            && self.persona_revision.is_valid()
            && self
                .d_request_id
                .as_ref()
                .is_none_or(|value| bounded_identifier(value))
            && self
                .d_input_digest
                .as_ref()
                .is_none_or(|value| sha256_hex(value))
            && d_shape_is_complete
            && self.created_at_ms >= 0
    }
}

/// Private, encrypted semantic context for one pending refinement. This is
/// never projected through `ChoiceLoopSnapshot` or accepted from an RPC
/// caller: the Store derives it from the active interpreted Choice state in
/// the same transaction that accepts a Selection.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChoiceRefinementContext {
    pub operation_id: String,
    pub selection_id: String,
    pub choice_session_id: String,
    pub source_envelope_id: String,
    pub conversation_turn_batch_id: String,
    pub expected_session_revision: u64,
    pub interpretation: InterpretationFrame,
    /// Present for A/B/C. D text stays only in its separately encrypted,
    /// request-bound intake record and is loaded by the Host when needed.
    pub selected_option: Option<ChoiceOption>,
}

impl ChoiceRefinementContext {
    #[must_use]
    pub fn is_valid(&self) -> bool {
        bounded_identifier(&self.operation_id)
            && bounded_identifier(&self.selection_id)
            && bounded_identifier(&self.choice_session_id)
            && bounded_identifier(&self.source_envelope_id)
            && bounded_identifier(&self.conversation_turn_batch_id)
            && self.expected_session_revision > 0
            && self.interpretation.is_valid()
            && self.interpretation.choice_session_id == self.choice_session_id
            && self.interpretation.revision > 0
            && self
                .selected_option
                .as_ref()
                .is_none_or(ChoiceOption::is_valid)
    }
}

/// Immutable, reviewable preparation for a later independently-authorized
/// effect. This command is intentionally not a Mission confirmation and does
/// not itself write a Reminder, deliver a message, or grant permission.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChoiceReminderItem {
    pub id: String,
    pub text: String,
    pub due_at_ms: i64,
    pub time_zone: String,
    pub evidence_intent: String,
}

impl ChoiceReminderItem {
    #[must_use]
    pub fn is_valid(&self) -> bool {
        bounded_identifier(&self.id)
            && bounded_text(&self.text, 1_024)
            && self.due_at_ms >= 0
            && bounded_text(&self.time_zone, 128)
            && self.time_zone.is_ascii()
            && bounded_text(&self.evidence_intent, 1_024)
    }
}

/// An explicit, bounded user schedule selection for an otherwise effect-free
/// Choice confirmation. The Host, not a question timestamp, validates and
/// durably records this input before it can appear in a confirmation payload.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChoiceReminderScheduleInput {
    pub request_id: String,
    pub choice_session_id: String,
    pub expected_session_revision: u64,
    pub reminder_list_id: String,
    /// The explicit number of ordered local Reminder payload items. This is
    /// proposal data only; it never authorizes an `EventKit` write.
    pub reminder_count: u32,
    pub due_at_ms: i64,
    pub time_zone: String,
}

impl ChoiceReminderScheduleInput {
    #[must_use]
    pub fn is_valid(&self) -> bool {
        bounded_identifier(&self.request_id)
            && bounded_identifier(&self.choice_session_id)
            && self.expected_session_revision > 0
            && bounded_identifier(&self.reminder_list_id)
            && (1..=16).contains(&self.reminder_count)
            && self.due_at_ms >= 0
            && bounded_text(&self.time_zone, 128)
            && self.time_zone.is_ascii()
    }
}

/// The Host-audited schedule revision used to derive exactly one confirmation
/// preview. It prepares no `EventKit` request and grants no effect authority.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChoiceReminderSchedule {
    pub id: String,
    pub input: ChoiceReminderScheduleInput,
    pub revision: u64,
    pub accepted_at_ms: i64,
}

impl ChoiceReminderSchedule {
    #[must_use]
    pub fn is_valid(&self) -> bool {
        bounded_identifier(&self.id)
            && self.input.is_valid()
            && self.revision > 0
            && self.accepted_at_ms >= 0
            && self.input.due_at_ms > self.accepted_at_ms
    }
}

/// Immutable, reviewable preparation for a later independently-authorized
/// effect. This command is intentionally not a Mission confirmation and does
/// not itself write a Reminder, deliver a message, or grant permission.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChoiceConsolidatedConfirmation {
    pub id: String,
    pub choice_session_id: String,
    pub choice_set_id: String,
    /// The accepted A/B/C/D direction whose private refinement is being
    /// confirmed. This prevents a current three-card `ChoiceSet` from being
    /// mistaken for three simultaneous Reminder steps.
    pub selection_id: String,
    pub expected_session_revision: u64,
    pub interpretation_revision: u64,
    pub payload_revision: u64,
    pub payload_digest: String,
    pub goal: String,
    pub steps: Vec<String>,
    /// The sole eventual Markdown target is sealed before confirmation.  A
    /// later writer may not choose a path or silently observe a new base.
    pub markdown_entry: DocumentManifestEntry,
    /// Descriptor identity observed by Host before confirmation.  `None`
    /// means the sealed operation is no-clobber creation.
    pub markdown_expected_base: Option<MarkdownBaseIdentity>,
    pub markdown_manifest_digests: Vec<String>,
    pub document_diff_digest: String,
    pub model_provenance: ModelProvenance,
    /// Exact verified Persona provenance for the confirmed interpretation.
    /// It supplies no Persona lifecycle or effect authority.
    pub persona_revision: PersonaRevisionRef,
    pub reminder_list_id: String,
    pub reminder_items: Vec<ChoiceReminderItem>,
    pub reminder_count: u32,
    pub reminder_payload_digest: String,
    pub evidence_requirements: Vec<String>,
    pub delivery_binding_id: Option<String>,
    pub recipient: Option<String>,
    pub delivery_scope: Option<String>,
    pub data_categories: Vec<String>,
    pub retention: String,
    pub permissions: Vec<String>,
    pub effect_classes: Vec<String>,
    pub confirmed_at_ms: i64,
}

impl ChoiceConsolidatedConfirmation {
    /// Recomputes the exact Reminder preparation without accepting a caller
    /// supplied digest. This remains effect-free until a later action-time
    /// write gate independently authorizes it.
    #[must_use]
    pub fn canonical_reminder_payload_digest(&self) -> Option<String> {
        let mut bytes = Vec::new();
        append_reminder_digest_text(&mut bytes, &self.reminder_list_id)?;
        bytes.extend_from_slice(&self.reminder_count.to_be_bytes());
        for item in &self.reminder_items {
            append_reminder_digest_text(&mut bytes, &item.id)?;
            append_reminder_digest_text(&mut bytes, &item.text)?;
            bytes.extend_from_slice(&item.due_at_ms.to_be_bytes());
            append_reminder_digest_text(&mut bytes, &item.time_zone)?;
            append_reminder_digest_text(&mut bytes, &item.evidence_intent)?;
        }
        Some(format!("{:x}", Sha256::digest(bytes)))
    }

    /// Recomputes the immutable payload binding without trusting the caller's
    /// claimed digest. The typed, domain-separated byte protocol is shared by
    /// Rust and Swift and deliberately avoids JSON key, null, number, Unicode,
    /// and escaping differences at this security boundary.
    #[must_use]
    pub fn canonical_payload_digest(&self) -> Option<String> {
        self.canonical_payload_preimage()
            .map(|bytes| format!("{:x}", Sha256::digest(bytes)))
    }

    /// Returns the exact cross-language preimage for focused golden-vector
    /// tests. It contains no secret material beyond the already reviewable
    /// confirmation payload and grants no effect authority.
    #[must_use]
    pub fn canonical_payload_preimage(&self) -> Option<Vec<u8>> {
        let mut bytes = b"openopen:choice-consolidated-confirmation:v1\0".to_vec();
        append_typed_object_start(&mut bytes, 29);
        append_typed_field_name(&mut bytes, "id")?;
        append_typed_string(&mut bytes, &self.id)?;
        append_typed_field_name(&mut bytes, "choiceSessionId")?;
        append_typed_string(&mut bytes, &self.choice_session_id)?;
        append_typed_field_name(&mut bytes, "choiceSetId")?;
        append_typed_string(&mut bytes, &self.choice_set_id)?;
        append_typed_field_name(&mut bytes, "selectionId")?;
        append_typed_string(&mut bytes, &self.selection_id)?;
        append_typed_field_name(&mut bytes, "expectedSessionRevision")?;
        append_typed_u64(&mut bytes, self.expected_session_revision);
        append_typed_field_name(&mut bytes, "interpretationRevision")?;
        append_typed_u64(&mut bytes, self.interpretation_revision);
        append_typed_field_name(&mut bytes, "payloadRevision")?;
        append_typed_u64(&mut bytes, self.payload_revision);
        append_typed_field_name(&mut bytes, "payloadDigest")?;
        append_typed_string(&mut bytes, "")?;
        append_typed_field_name(&mut bytes, "goal")?;
        append_typed_string(&mut bytes, &self.goal)?;
        append_typed_field_name(&mut bytes, "steps")?;
        append_typed_string_array(&mut bytes, &self.steps)?;
        append_typed_field_name(&mut bytes, "markdownEntry")?;
        append_typed_manifest_entry(&mut bytes, &self.markdown_entry)?;
        append_typed_field_name(&mut bytes, "markdownExpectedBase")?;
        append_typed_optional_markdown_base(&mut bytes, self.markdown_expected_base.as_ref())?;
        append_typed_field_name(&mut bytes, "markdownManifestDigests")?;
        append_typed_string_array(&mut bytes, &self.markdown_manifest_digests)?;
        append_typed_field_name(&mut bytes, "documentDiffDigest")?;
        append_typed_string(&mut bytes, &self.document_diff_digest)?;
        append_typed_field_name(&mut bytes, "modelProvenance")?;
        append_typed_model_provenance(&mut bytes, &self.model_provenance)?;
        append_typed_field_name(&mut bytes, "personaRevision")?;
        append_typed_persona_revision(&mut bytes, &self.persona_revision)?;
        append_typed_field_name(&mut bytes, "reminderListId")?;
        append_typed_string(&mut bytes, &self.reminder_list_id)?;
        append_typed_field_name(&mut bytes, "reminderItems")?;
        append_typed_reminder_items(&mut bytes, &self.reminder_items)?;
        append_typed_field_name(&mut bytes, "reminderCount")?;
        append_typed_u32(&mut bytes, self.reminder_count);
        append_typed_field_name(&mut bytes, "reminderPayloadDigest")?;
        append_typed_string(&mut bytes, &self.reminder_payload_digest)?;
        append_typed_field_name(&mut bytes, "evidenceRequirements")?;
        append_typed_string_array(&mut bytes, &self.evidence_requirements)?;
        append_typed_field_name(&mut bytes, "deliveryBindingId")?;
        append_typed_optional_string(&mut bytes, self.delivery_binding_id.as_deref())?;
        append_typed_field_name(&mut bytes, "recipient")?;
        append_typed_optional_string(&mut bytes, self.recipient.as_deref())?;
        append_typed_field_name(&mut bytes, "deliveryScope")?;
        append_typed_optional_string(&mut bytes, self.delivery_scope.as_deref())?;
        append_typed_field_name(&mut bytes, "dataCategories")?;
        append_typed_string_array(&mut bytes, &self.data_categories)?;
        append_typed_field_name(&mut bytes, "retention")?;
        append_typed_string(&mut bytes, &self.retention)?;
        append_typed_field_name(&mut bytes, "permissions")?;
        append_typed_string_array(&mut bytes, &self.permissions)?;
        append_typed_field_name(&mut bytes, "effectClasses")?;
        append_typed_string_array(&mut bytes, &self.effect_classes)?;
        append_typed_field_name(&mut bytes, "confirmedAtMs")?;
        append_typed_i64(&mut bytes, self.confirmed_at_ms);
        Some(bytes)
    }

    /// Hashes every immutable payload field together with the Store-owned
    /// Reminder schedule revision while excluding the confirmation ID and its
    /// own revision/digest. This breaks circularity while ensuring any
    /// Markdown, model, delivery, schedule, Reminder, or Evidence drift mints
    /// a distinct confirmation identity.
    #[must_use]
    pub fn canonical_revision_material_digest(&self, schedule_revision: u64) -> Option<String> {
        if schedule_revision == 0 {
            return None;
        }
        let mut material = self.clone();
        material.id.clear();
        material.payload_revision = 0;
        material.payload_digest.clear();
        let preimage = material.canonical_payload_preimage()?;
        let mut bytes = b"openopen:choice-confirmation-revision-material:v1\0".to_vec();
        append_typed_u64(&mut bytes, schedule_revision);
        append_typed_length(&mut bytes, preimage.len())?;
        bytes.extend_from_slice(&preimage);
        Some(format!("{:x}", Sha256::digest(bytes)))
    }

    /// Returns the nonzero typed revision token corresponding to the complete
    /// revision material. The full 256-bit material digest remains part of the
    /// Host-derived ID and the final payload digest remains the authority.
    #[must_use]
    pub fn canonical_payload_revision(&self, schedule_revision: u64) -> Option<u64> {
        let digest = self.canonical_revision_material_digest(schedule_revision)?;
        u64::from_str_radix(digest.get(..16)?, 16)
            .ok()
            .map(|value| value.max(1))
    }

    #[must_use]
    pub fn is_valid(&self) -> bool {
        let delivery_shape_is_complete = matches!(
            (
                self.delivery_binding_id.as_ref(),
                self.recipient.as_ref(),
                self.delivery_scope.as_ref(),
            ),
            (None, None, None) | (Some(_), Some(_), Some(_))
        );
        bounded_identifier(&self.id)
            && bounded_identifier(&self.choice_session_id)
            && bounded_identifier(&self.choice_set_id)
            && bounded_identifier(&self.selection_id)
            && self.expected_session_revision > 0
            && self.interpretation_revision > 0
            && self.payload_revision > 0
            && sha256_hex(&self.payload_digest)
            && self.canonical_payload_digest().as_deref() == Some(&self.payload_digest)
            && bounded_text(&self.goal, 4 * 1024)
            && !self.steps.is_empty()
            && bounded_list(&self.steps, 64, 1_024)
            && document_manifest_entries_are_valid(std::slice::from_ref(&self.markdown_entry))
            && self.markdown_expected_base.as_ref().is_none_or(|base| {
                base.is_valid() && base.entry.relative_path == self.markdown_entry.relative_path
            })
            && self.markdown_manifest_digests.len() == 2
            && self
                .markdown_manifest_digests
                .iter()
                .all(|value| sha256_hex(value))
            && canonical_document_manifest_digest(std::slice::from_ref(&self.markdown_entry))
                .as_deref()
                == self.markdown_manifest_digests.get(1).map(String::as_str)
            && sha256_hex(&self.document_diff_digest)
            && self.model_provenance.is_valid()
            && self.persona_revision.is_valid()
            && bounded_identifier(&self.reminder_list_id)
            && !self.reminder_items.is_empty()
            && self.reminder_items.len() <= 64
            && self.reminder_count == u32::try_from(self.reminder_items.len()).unwrap_or(u32::MAX)
            && self.reminder_items.iter().all(ChoiceReminderItem::is_valid)
            && sha256_hex(&self.reminder_payload_digest)
            && self.canonical_reminder_payload_digest().as_deref()
                == Some(&self.reminder_payload_digest)
            && bounded_list(&self.evidence_requirements, 32, 1_024)
            && self
                .delivery_binding_id
                .as_ref()
                .is_none_or(|value| bounded_identifier(value))
            && self
                .recipient
                .as_ref()
                .is_none_or(|value| bounded_identifier(value))
            && self
                .delivery_scope
                .as_ref()
                .is_none_or(|value| bounded_text(value, 512))
            && delivery_shape_is_complete
            && bounded_list(&self.data_categories, 32, 256)
            && bounded_text(&self.retention, 512)
            && bounded_list(&self.permissions, 32, 256)
            && bounded_list(&self.effect_classes, 32, 256)
            && self.confirmed_at_ms >= 0
    }
}

fn append_typed_length(bytes: &mut Vec<u8>, length: usize) -> Option<()> {
    bytes.extend_from_slice(&u64::try_from(length).ok()?.to_be_bytes());
    Some(())
}

fn append_typed_field_name(bytes: &mut Vec<u8>, name: &str) -> Option<()> {
    append_typed_length(bytes, name.len())?;
    bytes.extend_from_slice(name.as_bytes());
    Some(())
}

fn append_typed_object_start(bytes: &mut Vec<u8>, field_count: u32) {
    bytes.push(0x05);
    bytes.extend_from_slice(&field_count.to_be_bytes());
}

fn append_typed_array_start(bytes: &mut Vec<u8>, item_count: usize) -> Option<()> {
    bytes.push(0x06);
    bytes.extend_from_slice(&u32::try_from(item_count).ok()?.to_be_bytes());
    Some(())
}

fn append_typed_string(bytes: &mut Vec<u8>, value: &str) -> Option<()> {
    bytes.push(0x01);
    append_typed_length(bytes, value.len())?;
    bytes.extend_from_slice(value.as_bytes());
    Some(())
}

fn append_typed_optional_string(bytes: &mut Vec<u8>, value: Option<&str>) -> Option<()> {
    if let Some(value) = value {
        append_typed_string(bytes, value)
    } else {
        bytes.push(0x00);
        Some(())
    }
}

fn append_typed_u64(bytes: &mut Vec<u8>, value: u64) {
    bytes.push(0x02);
    bytes.extend_from_slice(&value.to_be_bytes());
}

fn append_typed_i64(bytes: &mut Vec<u8>, value: i64) {
    bytes.push(0x03);
    bytes.extend_from_slice(&value.to_be_bytes());
}

fn append_typed_u32(bytes: &mut Vec<u8>, value: u32) {
    bytes.push(0x04);
    bytes.extend_from_slice(&value.to_be_bytes());
}

fn append_typed_string_array(bytes: &mut Vec<u8>, values: &[String]) -> Option<()> {
    append_typed_array_start(bytes, values.len())?;
    for value in values {
        append_typed_string(bytes, value)?;
    }
    Some(())
}

fn append_typed_manifest_entry(bytes: &mut Vec<u8>, entry: &DocumentManifestEntry) -> Option<()> {
    append_typed_object_start(bytes, 4);
    append_typed_field_name(bytes, "relativePath")?;
    append_typed_string(bytes, &entry.relative_path)?;
    append_typed_field_name(bytes, "sha256")?;
    append_typed_string(bytes, &entry.sha256)?;
    append_typed_field_name(bytes, "byteLength")?;
    append_typed_u64(bytes, entry.byte_length);
    append_typed_field_name(bytes, "mode")?;
    append_typed_u32(bytes, entry.mode);
    Some(())
}

fn append_typed_optional_markdown_base(
    bytes: &mut Vec<u8>,
    base: Option<&MarkdownBaseIdentity>,
) -> Option<()> {
    let Some(base) = base else {
        bytes.push(0x00);
        return Some(());
    };
    append_typed_object_start(bytes, 3);
    append_typed_field_name(bytes, "entry")?;
    append_typed_manifest_entry(bytes, &base.entry)?;
    append_typed_field_name(bytes, "device")?;
    append_typed_u64(bytes, base.device);
    append_typed_field_name(bytes, "inode")?;
    append_typed_u64(bytes, base.inode);
    Some(())
}

fn append_typed_model_provenance(bytes: &mut Vec<u8>, provenance: &ModelProvenance) -> Option<()> {
    append_typed_object_start(bytes, 9);
    for (name, value) in [
        ("id", provenance.id.as_str()),
        ("modelId", provenance.model_id.as_str()),
        ("requestedEffort", provenance.requested_effort.as_str()),
        ("actualEffort", provenance.actual_effort.as_str()),
        (
            "catalogFingerprint",
            provenance.catalog_fingerprint.as_str(),
        ),
    ] {
        append_typed_field_name(bytes, name)?;
        append_typed_string(bytes, value)?;
    }
    append_typed_field_name(bytes, "catalogRevision")?;
    append_typed_u64(bytes, provenance.catalog_revision);
    append_typed_field_name(bytes, "accountDisplayClass")?;
    append_typed_string(bytes, &provenance.account_display_class)?;
    append_typed_field_name(bytes, "protocolSchemaRevision")?;
    append_typed_u64(bytes, provenance.protocol_schema_revision);
    append_typed_field_name(bytes, "turnId")?;
    append_typed_string(bytes, &provenance.turn_id)?;
    Some(())
}

fn append_typed_persona_revision(bytes: &mut Vec<u8>, persona: &PersonaRevisionRef) -> Option<()> {
    append_typed_object_start(bytes, 4);
    for (name, value) in [
        ("personaId", persona.persona_id.as_str()),
        ("revision", persona.revision.as_str()),
        ("aggregateDigest", persona.aggregate_digest.as_str()),
        ("instructionsDigest", persona.instructions_digest.as_str()),
    ] {
        append_typed_field_name(bytes, name)?;
        append_typed_string(bytes, value)?;
    }
    Some(())
}

fn append_typed_reminder_items(bytes: &mut Vec<u8>, items: &[ChoiceReminderItem]) -> Option<()> {
    append_typed_array_start(bytes, items.len())?;
    for item in items {
        append_typed_object_start(bytes, 5);
        append_typed_field_name(bytes, "id")?;
        append_typed_string(bytes, &item.id)?;
        append_typed_field_name(bytes, "text")?;
        append_typed_string(bytes, &item.text)?;
        append_typed_field_name(bytes, "dueAtMs")?;
        append_typed_i64(bytes, item.due_at_ms);
        append_typed_field_name(bytes, "timeZone")?;
        append_typed_string(bytes, &item.time_zone)?;
        append_typed_field_name(bytes, "evidenceIntent")?;
        append_typed_string(bytes, &item.evidence_intent)?;
    }
    Some(())
}

fn append_reminder_digest_text(bytes: &mut Vec<u8>, value: &str) -> Option<()> {
    let length = u64::try_from(value.len()).ok()?;
    bytes.extend_from_slice(&length.to_be_bytes());
    bytes.extend_from_slice(value.as_bytes());
    Some(())
}

impl Selection {
    #[must_use]
    pub fn id(&self) -> &str {
        match self {
            Self::OptionSelection(selection) => &selection.id,
            Self::NaturalConversationSelection(selection) => &selection.id,
        }
    }

    #[must_use]
    pub fn is_valid(&self) -> bool {
        match self {
            Self::OptionSelection(selection) => {
                bounded_identifier(&selection.id)
                    && bounded_identifier(&selection.choice_session_id)
                    && bounded_identifier(&selection.choice_set_id)
                    && bounded_identifier(&selection.selected_option_id)
                    && selection.expected_session_revision > 0
                    && selection.selected_at_ms >= 0
            }
            Self::NaturalConversationSelection(selection) => {
                bounded_identifier(&selection.id)
                    && bounded_identifier(&selection.choice_session_id)
                    && bounded_identifier(&selection.choice_set_id)
                    && bounded_identifier(&selection.d_input_batch_id)
                    && selection.expected_session_revision > 0
                    && selection.selected_at_ms >= 0
            }
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InterpretationFrame {
    pub choice_session_id: String,
    pub revision: u64,
    pub understood_goal: String,
    pub current_context: String,
    pub assumptions: Vec<String>,
    pub constraints: Vec<String>,
    pub uncertainties: Vec<String>,
    pub what_to_avoid: Vec<String>,
    pub source_manifest_digest: String,
}

impl InterpretationFrame {
    #[must_use]
    pub fn is_valid(&self) -> bool {
        bounded_identifier(&self.choice_session_id)
            && self.revision > 0
            && bounded_text(&self.understood_goal, 4 * 1024)
            && bounded_text(&self.current_context, 8 * 1024)
            && bounded_list(&self.assumptions, 64, 1_024)
            && bounded_list(&self.constraints, 64, 1_024)
            && bounded_list(&self.uncertainties, 64, 1_024)
            && bounded_list(&self.what_to_avoid, 64, 1_024)
            && sha256_hex(&self.source_manifest_digest)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DocumentManifestEntry {
    pub relative_path: String,
    pub sha256: String,
    pub byte_length: u64,
    pub mode: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DocumentManifest {
    pub root_version: u64,
    pub entries: Vec<DocumentManifestEntry>,
    pub aggregate_digest: String,
    pub generated_at_ms: i64,
}

/// A command-owned request to render one already-confirmed plaintext Markdown
/// entry. The body is deliberately not part of this public shape: the Store
/// keeps it encrypted and binds it to this intent before the Host may write.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MarkdownRenderIntent {
    pub id: String,
    pub choice_session_id: String,
    pub expected_session_revision: u64,
    pub expected_generation: u64,
    pub entry: DocumentManifestEntry,
    pub expected_base: Option<MarkdownBaseIdentity>,
    pub content_digest: String,
    pub created_at_ms: i64,
}

impl MarkdownRenderIntent {
    #[must_use]
    pub fn is_valid(&self) -> bool {
        bounded_identifier(&self.id)
            && bounded_identifier(&self.choice_session_id)
            && self.expected_session_revision > 0
            && self.expected_generation > 0
            && document_manifest_entries_are_valid(std::slice::from_ref(&self.entry))
            && self.expected_base.as_ref().is_none_or(|base| {
                base.is_valid() && base.entry.relative_path == self.entry.relative_path
            })
            && self.content_digest == self.entry.sha256
            && sha256_hex(&self.content_digest)
            && self.created_at_ms >= 0
    }
}

/// Exact descriptor identity observed by the Host before replacing an owner
/// file. Digest equality alone is insufficient: a same-content inode swap is
/// still a concurrent owner edit and must enter reconciliation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MarkdownBaseIdentity {
    pub entry: DocumentManifestEntry,
    pub device: u64,
    pub inode: u64,
}

impl MarkdownBaseIdentity {
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.device > 0
            && self.inode > 0
            && document_manifest_entries_are_valid(std::slice::from_ref(&self.entry))
    }
}

/// Durable evidence recorded only after the descriptor-safe render path has
/// synced and re-read the exact final file. Existing or ambiguous targets do
/// not produce this receipt.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MarkdownRenderReceipt {
    pub intent_id: String,
    pub final_entry: DocumentManifestEntry,
    pub final_device: u64,
    pub final_inode: u64,
    pub displaced_base: Option<MarkdownBaseIdentity>,
    pub committed_at_ms: i64,
}

impl MarkdownRenderReceipt {
    #[must_use]
    pub fn is_valid(&self) -> bool {
        bounded_identifier(&self.intent_id)
            && document_manifest_entries_are_valid(std::slice::from_ref(&self.final_entry))
            && self.final_device > 0
            && self.final_inode > 0
            && self.displaced_base.as_ref().is_none_or(|base| {
                base.is_valid() && base.entry.relative_path == self.final_entry.relative_path
            })
            && self.committed_at_ms >= 0
    }
}

/// The Store's one-transaction snapshot of the foreground Choice Loop. It is
/// intentionally local state rather than effect authority: a valid snapshot
/// may prepare context and drafts, but it cannot create a Mission, write a
/// Reminder, or deliver a message without the separate typed gates.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChoiceLoopSnapshot {
    pub session: ChoiceSession,
    pub active_batch: Option<ConversationTurnBatch>,
    pub interpretation: Option<InterpretationFrame>,
    pub active_choice_set: Option<ChoiceSet>,
    pub last_selection: Option<Selection>,
    /// A selection is never durable without its private continuation.  The
    /// result itself may enter only through the matching command-owned Store
    /// transaction.
    pub pending_refinement_operation: Option<ChoiceRefinementOperation>,
    pub confirmation: Option<ChoiceConsolidatedConfirmation>,
    pub document_manifest: DocumentManifest,
}

impl ChoiceLoopSnapshot {
    #[must_use]
    #[allow(clippy::too_many_lines)] // One complete snapshot validator keeps every authority fence visible.
    pub fn is_valid(&self) -> bool {
        if !self.session.is_valid() || !self.document_manifest.is_valid() {
            return false;
        }
        if matches!(
            self.session.state,
            ChoiceSessionState::Completed | ChoiceSessionState::Cancelled
        ) && (self.active_batch.is_some()
            || self.interpretation.is_some()
            || self.active_choice_set.is_some()
            || self.last_selection.is_some()
            || self.pending_refinement_operation.is_some()
            || self.confirmation.is_some()
            || self.session.pending_confirmation_id.is_some())
        {
            return false;
        }
        if let Some(batch) = &self.active_batch
            && (!batch.is_valid()
                || batch.choice_session_id != self.session.id
                || batch.revision != self.session.revision)
        {
            return false;
        }
        if let Some(frame) = &self.interpretation {
            if !frame.is_valid()
                || frame.choice_session_id != self.session.id
                || self.session.active_interpretation_revision != Some(frame.revision)
                || frame.source_manifest_digest != self.document_manifest.aggregate_digest
            {
                return false;
            }
        } else if self.session.active_interpretation_revision.is_some() {
            return false;
        }
        let choice_set_is_valid = match &self.active_choice_set {
            Some(choice_set) => {
                choice_set.is_valid()
                    && choice_set.choice_session_id == self.session.id
                    && choice_set.session_revision == self.session.revision
                    && choice_set.source_manifest_digest == self.document_manifest.aggregate_digest
                    && self.session.active_choice_set_id.as_deref() == Some(&choice_set.id)
                    && self.session.active_interpretation_revision
                        == Some(choice_set.interpretation_revision)
            }
            None => self.session.active_choice_set_id.is_none(),
        };
        let selection_is_bound = self.last_selection.as_ref().is_none_or(|selection| {
            selection.is_valid()
                && match selection {
                    Selection::OptionSelection(value) => {
                        value.choice_session_id == self.session.id
                            && value.expected_session_revision < self.session.revision
                            && value.selected_at_ms <= self.session.last_input_at_ms
                    }
                    Selection::NaturalConversationSelection(value) => {
                        value.choice_session_id == self.session.id
                            && value.expected_session_revision < self.session.revision
                            && value.selected_at_ms <= self.session.last_input_at_ms
                    }
                }
        });
        let pending_refinement_is_bound =
            self.pending_refinement_operation
                .as_ref()
                .is_none_or(|operation| {
                    operation.is_valid()
                        && operation.choice_session_id == self.session.id
                        && operation.expected_session_revision == self.session.revision
                        && operation.source_manifest_digest
                            == self.document_manifest.aggregate_digest
                        && self.session.state == ChoiceSessionState::Refining
                        && (operation.is_owner_resume()
                            || self.last_selection.as_ref().is_some_and(|selection| {
                                selection_id(selection) == operation.selection_id
                            }))
                });
        if self.session.state == ChoiceSessionState::Refining
            && self.pending_refinement_operation.is_none()
        {
            return false;
        }
        choice_set_is_valid
            && selection_is_bound
            && pending_refinement_is_bound
            && self.confirmation.as_ref().is_none_or(|confirmation| {
                confirmation.is_valid()
                    && confirmation.choice_session_id == self.session.id
                    && self.session.pending_confirmation_id.as_deref() == Some(&confirmation.id)
                    && matches!(
                        self.session.state,
                        ChoiceSessionState::AwaitingConfirmation
                            | ChoiceSessionState::Executing
                            | ChoiceSessionState::SoftIdle
                    )
                    && confirmation.delivery_binding_id == self.session.primary_delivery_binding_id
                    && confirmation.expected_session_revision.checked_add(
                        match self.session.state {
                            ChoiceSessionState::AwaitingConfirmation => 1,
                            ChoiceSessionState::Executing | ChoiceSessionState::SoftIdle => 2,
                            _ => 0,
                        },
                    ) == Some(self.session.revision)
            })
    }

    /// Returns whether `self` is the one permitted next durable state after
    /// `previous`. The Store owns this comparison; UI and adapters never get a
    /// raw whole-snapshot replacement capability.
    #[must_use]
    pub fn is_permitted_successor_of(&self, previous: &Self) -> bool {
        if !previous.is_valid()
            || !self.is_valid()
            || previous.session.revision.checked_add(1) != Some(self.session.revision)
            || self.session.opened_at_ms < previous.session.opened_at_ms
            || self.session.last_input_at_ms < previous.session.last_input_at_ms
            || self.document_manifest.generated_at_ms < previous.document_manifest.generated_at_ms
            || self.document_manifest.root_version < previous.document_manifest.root_version
            || self.session.communication_profile_revision
                < previous.session.communication_profile_revision
        {
            return false;
        }

        // `Executing` here means that the local, confirmed Markdown journal
        // has completed. It has not created a Mission, Reminder, delivery, or
        // other effect. A new explicit local question therefore safely starts
        // a fresh foreground Choice session; it never replays or abandons an
        // effect.
        let switches_terminal_session = self.session.id != previous.session.id
            && matches!(
                previous.session.state,
                ChoiceSessionState::Completed
                    | ChoiceSessionState::Cancelled
                    | ChoiceSessionState::Executing
            )
            && matches!(
                self.session.state,
                ChoiceSessionState::Active | ChoiceSessionState::Interpreting
            );
        if self.session.id == previous.session.id {
            if !choice_session_transition_allowed(previous.session.state, self.session.state) {
                return false;
            }
        } else if !switches_terminal_session {
            return false;
        }

        if !batch_transition_allowed(
            previous.active_batch.as_ref(),
            self.active_batch.as_ref(),
            self.session.revision,
        ) || !interpretation_transition_allowed(
            previous.interpretation.as_ref(),
            self.interpretation.as_ref(),
        ) || !choice_set_transition_allowed(
            previous.active_choice_set.as_ref(),
            self.active_choice_set.as_ref(),
        ) {
            return false;
        }

        // A new foreground session begins from clean interaction state. An
        // initial interpreting session may retain exactly its Host-sealed first
        // intake batch; it never inherits interpretation, ChoiceSet, or a
        // pending confirmation from the terminal session it supersedes.
        if switches_terminal_session
            && ((self.session.state != ChoiceSessionState::Interpreting
                && self.active_batch.is_some())
                || self.interpretation.is_some()
                || self.active_choice_set.is_some()
                || self.last_selection.is_some()
                || self.confirmation.is_some()
                || self.session.pending_confirmation_id.is_some())
        {
            return false;
        }
        true
    }
}

fn selection_id(selection: &Selection) -> &str {
    match selection {
        Selection::OptionSelection(value) => &value.id,
        Selection::NaturalConversationSelection(value) => &value.id,
    }
}

impl DocumentManifest {
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.root_version > 0
            && self.generated_at_ms >= 0
            && self.aggregate_digest
                == canonical_document_manifest_digest(&self.entries).unwrap_or_default()
    }
}

/// Computes the only accepted manifest digest. Entries are sorted by their
/// ASCII-safe path, and every field is length-delimited so a caller cannot
/// rebind a valid digest to another arrangement of files.
#[must_use]
pub fn canonical_document_manifest_digest(entries: &[DocumentManifestEntry]) -> Option<String> {
    if entries.is_empty() || entries.len() > 256 || !document_manifest_entries_are_valid(entries) {
        return None;
    }
    let mut ordered = entries.iter().collect::<Vec<_>>();
    ordered.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));

    let mut bytes = b"openopen:document-manifest:v1\0".to_vec();
    for entry in ordered {
        append_length_delimited(&mut bytes, entry.relative_path.as_bytes());
        append_length_delimited(&mut bytes, entry.sha256.as_bytes());
        append_length_delimited(&mut bytes, &entry.byte_length.to_be_bytes());
        append_length_delimited(&mut bytes, &entry.mode.to_be_bytes());
    }
    Some(format!("{:x}", Sha256::digest(bytes)))
}

fn document_manifest_entries_are_valid(entries: &[DocumentManifestEntry]) -> bool {
    let normalized_paths = entries
        .iter()
        .map(|entry| entry.relative_path.to_ascii_lowercase())
        .collect::<std::collections::BTreeSet<_>>();
    normalized_paths.len() == entries.len()
        && entries.iter().all(|entry| {
            safe_document_path(&entry.relative_path)
                && sha256_hex(&entry.sha256)
                && entry.byte_length <= 512 * 1024
                && entry.mode == 0o600
        })
}

fn append_length_delimited(bytes: &mut Vec<u8>, value: &[u8]) {
    bytes.extend_from_slice(&(value.len() as u64).to_be_bytes());
    bytes.extend_from_slice(value);
}

fn choice_session_transition_allowed(
    previous: ChoiceSessionState,
    next: ChoiceSessionState,
) -> bool {
    use ChoiceSessionState::{
        Active, AwaitingConfirmation, Blocked, Cancelled, Completed, Executing, Interpreting,
        Refining, SoftIdle, StaleReview,
    };
    matches!(
        (previous, next),
        (Interpreting, Active | Refining | Blocked | Cancelled)
            | (
                Active | Refining,
                Active
                    | Refining
                    | SoftIdle
                    | StaleReview
                    | AwaitingConfirmation
                    | Blocked
                    | Cancelled
            )
            | (
                SoftIdle | Blocked,
                Active | Refining | SoftIdle | StaleReview | Blocked | Cancelled
            )
            | (
                StaleReview,
                Active | Refining | StaleReview | Blocked | Cancelled
            )
            | (
                AwaitingConfirmation,
                Active | Refining | SoftIdle | Executing | Blocked | Cancelled
            )
            | (Executing, Executing | Completed | Blocked | Cancelled)
            | (Completed, Completed)
            | (Cancelled, Cancelled)
    )
}

fn batch_transition_allowed(
    previous: Option<&ConversationTurnBatch>,
    next: Option<&ConversationTurnBatch>,
    next_session_revision: u64,
) -> bool {
    match (previous, next) {
        (None | Some(_), None) | (None, Some(_)) => true,
        (Some(previous), Some(next)) if previous.id != next.id => {
            previous.sealed_at_ms.is_some()
                && next.opened_at_ms >= previous.sealed_at_ms.unwrap_or_default()
        }
        (Some(previous), Some(next)) => {
            if next.revision != next_session_revision
                || next.opened_at_ms != previous.opened_at_ms
                || next.quiet_deadline_ms != previous.quiet_deadline_ms
                || next.hard_deadline_ms != previous.hard_deadline_ms
                || !next
                    .source_envelope_ids
                    .starts_with(&previous.source_envelope_ids)
                || next.source_envelope_ids.len() == previous.source_envelope_ids.len()
            {
                return false;
            }
            match (
                previous.sealed_at_ms,
                previous.seal_reason,
                next.sealed_at_ms,
                next.seal_reason,
            ) {
                (None, None, None, None) | (None, None, Some(_), Some(_)) => true,
                (Some(old_at), Some(old_reason), Some(new_at), Some(new_reason)) => {
                    old_at == new_at && old_reason == new_reason
                }
                _ => false,
            }
        }
    }
}

const fn batch_seal_is_consistent(
    sealed_at_ms: Option<i64>,
    seal_reason: Option<BatchSealReason>,
    quiet_deadline_ms: i64,
    hard_deadline_ms: i64,
) -> bool {
    match (sealed_at_ms, seal_reason) {
        (None, None) => true,
        (Some(sealed_at_ms), Some(BatchSealReason::QuietDeadline)) => {
            sealed_at_ms >= quiet_deadline_ms && sealed_at_ms <= hard_deadline_ms
        }
        (Some(sealed_at_ms), Some(BatchSealReason::HardDeadline)) => {
            sealed_at_ms == hard_deadline_ms
        }
        (Some(sealed_at_ms), Some(BatchSealReason::InitialIntake)) => {
            sealed_at_ms >= 0 && sealed_at_ms <= hard_deadline_ms
        }
        (Some(sealed_at_ms), Some(_)) => sealed_at_ms >= 0 && sealed_at_ms <= hard_deadline_ms,
        _ => false,
    }
}

fn interpretation_transition_allowed(
    previous: Option<&InterpretationFrame>,
    next: Option<&InterpretationFrame>,
) -> bool {
    match (previous, next) {
        (Some(previous), Some(next)) if previous.revision == next.revision => previous == next,
        (Some(previous), Some(next)) => next.revision > previous.revision,
        _ => true,
    }
}

fn choice_set_transition_allowed(previous: Option<&ChoiceSet>, next: Option<&ChoiceSet>) -> bool {
    !matches!((previous, next), (Some(previous), Some(next)) if previous.id == next.id)
}

fn bounded_text(value: &str, maximum_bytes: usize) -> bool {
    !value.trim().is_empty()
        && value == value.trim()
        && value.len() <= maximum_bytes
        && !value.chars().any(char::is_control)
}

fn bounded_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn bounded_list(values: &[String], maximum_items: usize, maximum_bytes: usize) -> bool {
    values.len() <= maximum_items
        && values
            .iter()
            .all(|value| bounded_text(value, maximum_bytes))
}

fn sha256_hex(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn valid_effort(value: &str) -> bool {
    value == "not_applicable"
        || (!value.is_empty()
            && value.len() <= 32
            && value
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte == b'-'))
}

fn valid_model_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn safe_document_path(path: &str) -> bool {
    let components = path.split('/').collect::<Vec<_>>();
    !path.is_empty()
        && path.len() <= 512
        && !path.starts_with('/')
        && components.iter().all(|component| {
            !component.is_empty()
            && *component != "."
            && *component != ".."
                // The product path grammar is intentionally ASCII-only. This
                // is stricter than normalization and rejects Unicode/case
                // collision tricks before a filesystem implementation sees
                // them; accepted user prose remains file *content*, not a
                // filename authority.
                && component.bytes().all(|byte| {
                    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.')
                })
        })
        && matches_document_manifest_path(&components)
}

fn matches_document_manifest_path(components: &[&str]) -> bool {
    match components {
        ["INDEX.md"] | ["profile", "USER.md" | "COMMUNICATION.md"] | ["sources", "INDEX.md"] => {
            true
        }
        ["sources", name] => dynamic_markdown_name(name),
        [
            "tasks",
            task,
            "OVERVIEW.md" | "STATE.md" | "DECISIONS.md" | "QUESTIONS.md" | "MODEL_BRIEF.md",
        ] => bounded_identifier(task),
        ["tasks", task, "paths" | "updates", name] => {
            bounded_identifier(task) && dynamic_markdown_name(name)
        }
        ["sessions", session, "SESSION.md" | "CHOICE.md"] => bounded_identifier(session),
        ["sessions", session, "choice-sets", name] => {
            bounded_identifier(session) && dynamic_markdown_name(name)
        }
        _ => false,
    }
}

fn dynamic_markdown_name(value: &str) -> bool {
    value
        .strip_suffix(".md")
        .is_some_and(|stem| !stem.is_empty() && bounded_identifier(stem))
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutcomeSuggestion {
    pub id: String,
    pub title: String,
    pub why_now: String,
    pub proposed_steps: Vec<String>,
    pub source_refs: Vec<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
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
#[serde(rename_all = "camelCase", tag = "type")]
pub enum ApprovalTarget {
    ReminderList {
        logical_list_id: String,
        source_identifier: String,
        calendar_identifier: String,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalRequest {
    pub id: String,
    pub work_item_id: Option<String>,
    pub kind: ApprovalKind,
    pub prompt: String,
    pub scope_digest: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<ApprovalTarget>,
    pub status: ApprovalStatus,
    pub requested_by_id: String,
    pub decided_by_id: Option<String>,
    pub requested_at_ms: i64,
    pub decided_at_ms: Option<i64>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum EvidenceKind {
    ReminderDispatchStarted,
    /// A prior signed pre-commit abort was consumed to mint exactly one new
    /// dispatch attempt. An unclosed retry is always recovery-only.
    ReminderDispatchRetryStarted,
    /// The signed local effect client proved cancellation before `EventKit`'s
    /// commit boundary. This permits a later, separately owner-authorized
    /// retry without treating an ambiguous post-commit result as absent.
    ReminderDispatchAbortedBeforeCommit,
    ReminderMirrored,
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

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ChannelKind {
    IMessage,
    Discord,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChannelEnvelope {
    pub channel: ChannelKind,
    pub source_message_id: String,
    pub sender_id: String,
    pub conversation_id: String,
    pub content_sha256: String,
    pub received_at_ms: i64,
}

/// One owner-confirmed channel boundary. V1 deliberately permits exactly one
/// owner and one conversation per channel kind.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChannelPairing {
    pub channel: ChannelKind,
    pub owner_sender_id: String,
    pub conversation_id: String,
    pub require_explicit_address: bool,
    #[serde(default)]
    pub imessage: Option<IMessagePairingMetadata>,
    pub discord: Option<DiscordPairingMetadata>,
    pub paired_at_ms: i64,
}

/// Exact public-imsg identity selected for the dedicated same-account
/// self-chat. The Host revalidates these immutable facts on every row.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IMessagePairingMetadata {
    pub chat_guid: String,
    pub chat_identifier: String,
    pub service: String,
    pub participant_ids: Vec<String>,
}

/// Exact persisted, user-visible preview for one explicitly authorized reply
/// to the dedicated same-account iMessage self-chat.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChoiceIMessageReplyPreview {
    pub reply_id: String,
    pub preview_revision: u64,
    pub destination: String,
    pub visible_body: String,
    pub confirmation_digest: String,
}

impl ChoiceIMessageReplyPreview {
    #[must_use]
    pub fn is_valid(&self) -> bool {
        bounded_identifier(&self.reply_id)
            && self.preview_revision > 0
            && self.destination == "Your selected iMessage self-chat"
            && !self.visible_body.trim().is_empty()
            && self.visible_body == self.visible_body.trim()
            && self.visible_body.len() <= 8_000
            && !self
                .visible_body
                .chars()
                .any(|character| character.is_control() && character != '\n')
            && !self.visible_body.as_bytes().contains(&0)
            && sha256_hex(&self.confirmation_digest)
    }
}

/// Complete Store-private authority binding for one Choice reactive reply.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChoiceIMessageReplyIntent {
    pub preview: ChoiceIMessageReplyPreview,
    pub outbound_id: String,
    pub choice_session_id: String,
    pub session_revision: u64,
    pub choice_set_id: String,
    pub choice_set_digest: String,
    pub source_message_id: String,
    pub delivery_binding_id: String,
    pub pairing: ChannelPairing,
    pub persona_revision: PersonaRevisionRef,
    pub source_manifest_digest: String,
    pub model_provenance: ModelProvenance,
    pub canonical_payload_sha256: String,
    pub created_at_ms: i64,
    pub approved_at_ms: Option<i64>,
    pub recovery_cursor: Option<ChannelCursor>,
}

impl ChoiceIMessageReplyIntent {
    #[must_use]
    pub fn expected_confirmation_digest(&self) -> Option<String> {
        serde_json::to_vec(&serde_json::json!({
            "replyId": self.preview.reply_id,
            "previewRevision": self.preview.preview_revision,
            "destination": self.preview.destination,
            "visibleBody": self.preview.visible_body,
            "outboundId": self.outbound_id,
            "choiceSessionId": self.choice_session_id,
            "sessionRevision": self.session_revision,
            "choiceSetId": self.choice_set_id,
            "choiceSetDigest": self.choice_set_digest,
            "sourceMessageId": self.source_message_id,
            "deliveryBindingId": self.delivery_binding_id,
            "pairing": self.pairing,
            "personaRevision": self.persona_revision,
            "sourceManifestDigest": self.source_manifest_digest,
            "modelProvenance": self.model_provenance,
            "canonicalPayloadSha256": self.canonical_payload_sha256,
        }))
        .ok()
        .map(|bytes| format!("{:x}", Sha256::digest(bytes)))
    }

    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.preview.is_valid()
            && bounded_identifier(&self.outbound_id)
            && bounded_identifier(&self.choice_session_id)
            && self.session_revision > 0
            && bounded_identifier(&self.choice_set_id)
            && sha256_hex(&self.choice_set_digest)
            && bounded_identifier(&self.source_message_id)
            && bounded_identifier(&self.delivery_binding_id)
            && self.pairing.channel == ChannelKind::IMessage
            && !self.pairing.require_explicit_address
            && self.pairing.imessage.is_some()
            && self.pairing.discord.is_none()
            && self.persona_revision.is_valid()
            && sha256_hex(&self.source_manifest_digest)
            && self.model_provenance.is_valid()
            && sha256_hex(&self.canonical_payload_sha256)
            && self.canonical_payload_sha256
                == format!("{:x}", Sha256::digest(self.preview.visible_body.as_bytes()))
            && self.expected_confirmation_digest().as_deref()
                == Some(self.preview.confirmation_digest.as_str())
            && self.created_at_ms >= 0
            && self
                .approved_at_ms
                .is_none_or(|value| value >= self.created_at_ms)
            && self.recovery_cursor.as_ref().is_none_or(|cursor| {
                cursor.channel == ChannelKind::IMessage
                    && cursor.conversation_id == self.pairing.conversation_id
            })
    }
}

#[must_use]
pub fn canonical_choice_set_digest(choice_set: &ChoiceSet) -> Option<String> {
    if !choice_set.is_valid() {
        return None;
    }
    serde_json::to_vec(choice_set)
        .ok()
        .map(|bytes| format!("{:x}", Sha256::digest(bytes)))
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ChoiceIMessageReplyDisposition {
    ExecuteNow,
    RecoverOnly,
    AlreadySent,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChoiceIMessageReplyStart {
    pub intent: ChoiceIMessageReplyIntent,
    pub disposition: ChoiceIMessageReplyDisposition,
}

/// Immutable setup facts proven by the official Discord bot flow before the
/// common channel pairing can be persisted.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiscordPairingMetadata {
    pub guild_id: String,
    pub bot_user_id: String,
    pub application_id: String,
    pub setup_source_message_id: String,
    pub setup_candidate_id: String,
}

/// Adapter-native recovery position plus a monotonically increasing ordering
/// value. The opaque value is persisted for the adapter; Core compares only
/// the numeric order.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChannelCursor {
    pub channel: ChannelKind,
    pub conversation_id: String,
    pub opaque_value: String,
    pub order: u64,
    pub observed_at_ms: i64,
}

/// Metadata observed by an adapter before any message body can reach a model.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChannelObservation {
    pub envelope: ChannelEnvelope,
    pub cursor: ChannelCursor,
    pub is_bot: bool,
    pub explicitly_addressed: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ChannelInboundDecision {
    Accepted,
    AcceptedMissionUpdate,
    Duplicate,
    IgnoredUnpaired,
    IgnoredSender,
    IgnoredConversation,
    IgnoredBot,
    IgnoredNotAddressed,
    IgnoredMessageClass,
    IgnoredInactiveMission,
    IgnoredStaleCursor,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChannelInboundResult {
    pub decision: ChannelInboundDecision,
    pub cursor: ChannelCursor,
    pub mission_event: Option<ChannelMissionEvent>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ChannelModelDisposition {
    ExecuteNow,
    RecoverOnly,
    SuggestionReady,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChannelModelStart {
    pub envelope: ChannelEnvelope,
    pub content: String,
    pub disposition: ChannelModelDisposition,
    pub suggestion: Option<OutcomeSuggestion>,
}

/// Sanitized classification for one terminal channel-model incident. The
/// failed dispatch remains the immutable authority; this type grants no retry
/// or provider effect.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ChannelFailureClass {
    ModelResultUnavailable,
}

/// Durable owner acknowledgement for one exact terminal incident.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChannelFailureAcknowledgement {
    pub acknowledged_at_ms: i64,
    pub runtime_revision: u64,
    pub audit_anchor: EffectAuditAnchor,
}

/// One stable, audited presentation record derived from immutable failed-
/// dispatch correlation. It is informational only and cannot reopen model or
/// provider authority.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChannelFailureIncident {
    pub incident_id: String,
    pub channel: ChannelKind,
    pub failure_class: ChannelFailureClass,
    pub occurred_at_ms: i64,
    pub runtime_revision: u64,
    pub dispatch_state_hash: String,
    pub source_audit_anchor: EffectAuditAnchor,
    pub incident_audit_anchor: EffectAuditAnchor,
    pub acknowledgement: Option<ChannelFailureAcknowledgement>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ChannelInboundMessageClass {
    MissionParticipation,
    NeedYouResponse,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ChannelRouteRole {
    Primary,
    Additional,
}

/// One immutable, owner-confirmed Mission route. The Store derives the route
/// and audit identities and never accepts a caller-assembled route snapshot.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChannelRoute {
    pub route_id: String,
    pub role: ChannelRouteRole,
    pub channel: ChannelKind,
    pub conversation_id: String,
    pub owner_sender_id: String,
    pub provider_identity: Option<String>,
    pub source_message_id: Option<String>,
    pub allowed_inbound_classes: Vec<ChannelInboundMessageClass>,
    pub allowed_outbound_classes: Vec<ChannelMessageKind>,
    pub revision: u64,
    pub approval_id: String,
    pub audit_id: String,
    pub bound_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChannelRouteSet {
    pub mission_id: String,
    pub revision: u64,
    pub primary_route_id: String,
    pub routes: Vec<ChannelRoute>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ChannelRouteApprovalDecision {
    Approve,
    Reject,
}

/// Typed owner decision used only to add one exact durable pairing to an
/// existing Mission route set. Additional outbound classes are explicit and
/// callers must send an empty list to retain the safe default.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChannelRouteApproval {
    pub approval_id: String,
    pub mission_id: String,
    pub expected_route_set_revision: u64,
    pub channel: ChannelKind,
    pub conversation_id: String,
    pub owner_sender_id: String,
    pub provider_identity: Option<String>,
    pub allowed_inbound_classes: Vec<ChannelInboundMessageClass>,
    pub allowed_outbound_classes: Vec<ChannelMessageKind>,
    pub actor_id: String,
    pub decision: ChannelRouteApprovalDecision,
    pub decided_at_ms: i64,
}

/// Durable participation bound to one exact Mission and route revision. This
/// is never completion Evidence and never grants a new Mission or scope.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChannelMissionEvent {
    pub event_id: String,
    pub mission_id: String,
    pub mission_revision: i64,
    pub mission_anchor_hash: String,
    pub route_id: String,
    pub route_set_revision: u64,
    pub message_class: ChannelInboundMessageClass,
    pub channel: ChannelKind,
    pub source_message_id: String,
    pub content_sha256: String,
    pub recorded_at_ms: i64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ChannelMessageKind {
    NeedYou,
    Progress,
    Receipt,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChannelOutboundIntent {
    pub outbound_id: String,
    pub mission_id: String,
    pub route_id: String,
    pub route_set_revision: u64,
    pub channel: ChannelKind,
    pub conversation_id: String,
    pub recipient_id: String,
    pub kind: ChannelMessageKind,
    pub content_sha256: String,
    pub created_at_ms: i64,
    pub recovery_cursor: Option<ChannelCursor>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ChannelOutboundDisposition {
    ExecuteNow,
    RecoverOnly,
    AlreadySent,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChannelOutboundStart {
    pub intent: ChannelOutboundIntent,
    pub disposition: ChannelOutboundDisposition,
    pub provider_message_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChannelDeliveryReceipt {
    pub outbound_id: String,
    pub provider_message_id: String,
    pub delivered_at_ms: i64,
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

pub const C2_INSTRUCTION_ONLY_PERMISSION_DIGEST: &str =
    "3cb2dbae054a787c18b5ba9a60ab0e4541fbe6f9c4c165e9de77f84a7363c298";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum C2SkillDemoStage {
    Candidate,
    Staged,
    Runnable,
    Used,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct C2SkillDemoSeal {
    pub package_id: String,
    pub source_url: String,
    pub commit: String,
    pub package_digest: String,
    pub audit_anchor: String,
    pub permission_digest: String,
    pub license: String,
}

impl C2SkillDemoSeal {
    #[must_use]
    pub fn is_valid(&self) -> bool {
        bounded_identifier(&self.package_id)
            && self.source_url.starts_with("https://github.com/")
            && self.source_url.len() <= 512
            && !self.source_url.chars().any(char::is_control)
            && self.commit.len() == 40
            && self
                .commit
                .bytes()
                .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
            && sha256_hex(&self.package_digest)
            && sha256_hex(&self.audit_anchor)
            && self.permission_digest == C2_INSTRUCTION_ONLY_PERMISSION_DIGEST
            && matches!(self.license.as_str(), "MIT" | "Apache-2.0")
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum C2SkillDemoCommandKind {
    RegisterCandidate,
    StageReviewed,
    EnableRunnable,
    RecordFirstNoEffectUse,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct C2SkillDemoCommand {
    pub request_id: String,
    pub expected_revision: u64,
    pub kind: C2SkillDemoCommandKind,
    pub seal: C2SkillDemoSeal,
    pub actor_id: String,
    pub decision_id: String,
    pub approval_nonce: String,
    pub result_digest: Option<String>,
    pub explicitly_confirmed: bool,
    pub decided_at_ms: i64,
}

impl C2SkillDemoCommand {
    #[must_use]
    pub fn is_valid(&self) -> bool {
        bounded_identifier(&self.request_id)
            && self.seal.is_valid()
            && bounded_identifier(&self.actor_id)
            && bounded_identifier(&self.decision_id)
            && sha256_hex(&self.approval_nonce)
            && self.explicitly_confirmed
            && self.decided_at_ms >= 0
            && match self.kind {
                C2SkillDemoCommandKind::RecordFirstNoEffectUse => {
                    self.result_digest.as_deref().is_some_and(sha256_hex)
                }
                _ => self.result_digest.is_none(),
            }
    }

    #[must_use]
    pub fn digest(&self) -> Option<String> {
        self.is_valid()
            .then(|| serde_json::to_vec(self).ok())
            .flatten()
            .map(|bytes| format!("{:x}", Sha256::digest(bytes)))
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct C2SkillDemoReceipt {
    pub request_id: String,
    pub command_digest: String,
    pub revision: u64,
    pub stage: C2SkillDemoStage,
    pub receipt_digest: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct C2SkillDemoState {
    pub revision: u64,
    pub stage: C2SkillDemoStage,
    pub seal: C2SkillDemoSeal,
    pub consumed_nonces: Vec<String>,
    pub receipts: Vec<C2SkillDemoReceipt>,
    pub first_use_result_digest: Option<String>,
}

pub const B2_MEMORY_MARKDOWN_PATH: &str = "sources/chatgpt.md";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum B2MemoryDemoStage {
    Prepared,
    Candidates,
    Selected,
    DiffReview,
    Confirmed,
    ReadBack,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct B2MemoryCandidateCard {
    pub id: String,
    pub title: String,
    pub rationale: String,
    pub proposed_line: String,
    pub source_binding_digest: String,
}

impl B2MemoryCandidateCard {
    #[must_use]
    pub fn is_valid(&self) -> bool {
        bounded_identifier(&self.id)
            && bounded_text(&self.title, 160)
            && bounded_text(&self.rationale, 512)
            && bounded_text(&self.proposed_line, 4_096)
            && sha256_hex(&self.source_binding_digest)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct B2MemoryImportSeal {
    pub source_digest: String,
    pub catalog_digest: String,
    pub source_manifest_digest: String,
    pub model_provenance: ModelProvenance,
}

impl B2MemoryImportSeal {
    #[must_use]
    pub fn is_valid(&self) -> bool {
        sha256_hex(&self.source_digest)
            && sha256_hex(&self.catalog_digest)
            && sha256_hex(&self.source_manifest_digest)
            && self.model_provenance.is_valid()
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct B2MemoryMarkdownDiff {
    pub revision: u64,
    pub selected_candidate_id: String,
    pub proposed_line: String,
    pub edited_line: String,
    pub expected_base: Option<MarkdownBaseIdentity>,
    pub final_entry: DocumentManifestEntry,
    pub diff_digest: String,
}

impl B2MemoryMarkdownDiff {
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.revision > 0
            && bounded_identifier(&self.selected_candidate_id)
            && bounded_text(&self.proposed_line, 4_096)
            && bounded_text(&self.edited_line, 4_096)
            && self
                .expected_base
                .as_ref()
                .is_none_or(MarkdownBaseIdentity::is_valid)
            && self.final_entry.relative_path == B2_MEMORY_MARKDOWN_PATH
            && document_manifest_entries_are_valid(std::slice::from_ref(&self.final_entry))
            && sha256_hex(&self.diff_digest)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum B2MemoryCommandKind {
    Prepare,
    SelectCandidate,
    EditMarkdown,
    ConfirmDiff,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct B2MemoryCommand {
    pub request_id: String,
    pub expected_revision: u64,
    pub kind: B2MemoryCommandKind,
    pub selected_candidate_id: Option<String>,
    pub edited_line: Option<String>,
    pub expected_diff_digest: Option<String>,
    pub explicitly_confirmed: bool,
    pub decided_at_ms: i64,
}

impl B2MemoryCommand {
    #[must_use]
    pub fn is_valid(&self) -> bool {
        bounded_identifier(&self.request_id)
            && self.decided_at_ms >= 0
            && match self.kind {
                B2MemoryCommandKind::Prepare => {
                    self.expected_revision == 0
                        && self.selected_candidate_id.is_none()
                        && self.edited_line.is_none()
                        && self.expected_diff_digest.is_none()
                        && !self.explicitly_confirmed
                }
                B2MemoryCommandKind::SelectCandidate => {
                    self.expected_revision > 0
                        && self
                            .selected_candidate_id
                            .as_deref()
                            .is_some_and(bounded_identifier)
                        && self.edited_line.is_none()
                        && self.expected_diff_digest.is_none()
                        && self.explicitly_confirmed
                }
                B2MemoryCommandKind::EditMarkdown => {
                    self.expected_revision > 0
                        && self.selected_candidate_id.is_none()
                        && self
                            .edited_line
                            .as_deref()
                            .is_some_and(|line| bounded_text(line, 4_096))
                        && self.expected_diff_digest.as_deref().is_some_and(sha256_hex)
                        && !self.explicitly_confirmed
                }
                B2MemoryCommandKind::ConfirmDiff => {
                    self.expected_revision > 0
                        && self.selected_candidate_id.is_none()
                        && self.edited_line.is_none()
                        && self.expected_diff_digest.as_deref().is_some_and(sha256_hex)
                        && self.explicitly_confirmed
                }
            }
    }

    #[must_use]
    pub fn digest(&self) -> Option<String> {
        self.is_valid()
            .then(|| serde_json::to_vec(self).ok())
            .flatten()
            .map(|bytes| format!("{:x}", Sha256::digest(bytes)))
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct B2MemoryCommandReceipt {
    pub request_id: String,
    pub command_digest: String,
    pub revision: u64,
    pub stage: B2MemoryDemoStage,
    pub receipt_digest: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct B2MemoryReadbackReceipt {
    pub confirmation_digest: String,
    pub render_receipt_digest: String,
    pub receipt_digest: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct B2MemoryDemoState {
    pub revision: u64,
    pub stage: B2MemoryDemoStage,
    pub seal: Option<B2MemoryImportSeal>,
    pub candidates: Vec<B2MemoryCandidateCard>,
    pub selected_candidate: Option<B2MemoryCandidateCard>,
    pub markdown_diff: Option<B2MemoryMarkdownDiff>,
    pub confirmation_digest: Option<String>,
    pub readback_receipt: Option<B2MemoryReadbackReceipt>,
    pub receipts: Vec<B2MemoryCommandReceipt>,
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
mod choice_contract_tests {
    use super::{
        BatchSealReason, CHOICE_BATCH_HARD_WINDOW_MS, CHOICE_BATCH_QUIET_WINDOW_MS,
        CHOICE_SESSION_SOFT_IDLE_MS, CHOICE_SESSION_STALE_REVIEW_MS,
        ChoiceConsolidatedConfirmation, ChoiceOption, ChoiceReminderItem,
        ChoiceReminderScheduleInput, ChoiceSession, ChoiceSessionState, ChoiceSet,
        ConversationTurnBatch, DocumentManifest, DocumentManifestEntry, ModelProvenance,
        ModelSelectionState, canonical_document_manifest_digest, sha256_hex,
    };

    fn provenance() -> ModelProvenance {
        ModelProvenance {
            id: "provenance-1".to_owned(),
            model_id: "gpt-example".to_owned(),
            requested_effort: "not_applicable".to_owned(),
            actual_effort: "not_applicable".to_owned(),
            catalog_fingerprint: "a".repeat(64),
            catalog_revision: 1,
            account_display_class: "ChatGPT account".to_owned(),
            protocol_schema_revision: 1,
            turn_id: "turn-1".to_owned(),
        }
    }

    fn option(position: u8, direction: &str) -> ChoiceOption {
        ChoiceOption {
            id: format!("option-{position}"),
            position,
            direction: direction.to_owned(),
            rationale: "Useful next direction".to_owned(),
            expected_result: "A bounded review result".to_owned(),
            information_needed: vec![],
            external_effects_preview: vec![],
            source_categories: vec!["ownerInput".to_owned()],
        }
    }

    #[test]
    fn reminder_schedule_input_is_bounded_but_never_claims_a_clock_decision() {
        let valid = ChoiceReminderScheduleInput {
            request_id: "schedule-request-1".to_owned(),
            choice_session_id: "session-1".to_owned(),
            expected_session_revision: 1,
            reminder_list_id: "local-reminders".to_owned(),
            reminder_count: 1,
            due_at_ms: 1,
            time_zone: "America/Los_Angeles".to_owned(),
        };
        assert!(valid.is_valid());
        let invalid_hex_like_zone = ChoiceReminderScheduleInput {
            time_zone: "zone\u{0000}".to_owned(),
            ..valid.clone()
        };
        assert!(!invalid_hex_like_zone.is_valid());
        let invalid_list = ChoiceReminderScheduleInput {
            reminder_list_id: "list with spaces".to_owned(),
            ..valid.clone()
        };
        assert!(!invalid_list.is_valid());
        let invalid_count = ChoiceReminderScheduleInput {
            reminder_count: 0,
            ..valid
        };
        assert!(!invalid_count.is_valid());
    }

    #[test]
    fn choice_set_requires_exactly_three_distinct_options_and_product_owned_d() {
        let valid = ChoiceSet {
            id: "choices-1".to_owned(),
            choice_session_id: "session-1".to_owned(),
            session_revision: 3,
            interpretation_revision: 2,
            generated_at_ms: 1,
            expires_on_revision: 3,
            options: vec![
                option(1, "Review the current plan"),
                option(2, "Narrow the next step"),
                option(3, "Prepare a safe alternative"),
            ],
            d_available: true,
            source_manifest_digest: "b".repeat(64),
            model_provenance: provenance(),
            persona_revision: crate::PersonaRevisionRef {
                persona_id: "openopen.nondev.default".to_owned(),
                revision: "draft-03-en".to_owned(),
                aggregate_digest: "f".repeat(64),
                instructions_digest: "e".repeat(64),
            },
        };
        assert!(valid.is_valid());

        let mut duplicate = valid.clone();
        duplicate.options[2].direction = duplicate.options[1].direction.clone();
        assert!(!duplicate.is_valid());

        let mut missing_d = valid;
        missing_d.d_available = false;
        assert!(!missing_d.is_valid());
    }

    #[test]
    fn document_manifest_rejects_escape_and_wrong_mode() {
        let mut manifest = DocumentManifest {
            root_version: 1,
            entries: vec![DocumentManifestEntry {
                relative_path: "tasks/plan/OVERVIEW.md".to_owned(),
                sha256: "c".repeat(64),
                byte_length: 42,
                mode: 0o600,
            }],
            aggregate_digest: String::new(),
            generated_at_ms: 1,
        };
        manifest.aggregate_digest = canonical_document_manifest_digest(&manifest.entries)
            .expect("valid entry has canonical digest");
        assert!(manifest.is_valid());
        manifest.entries[0].relative_path = "../outside.md".to_owned();
        assert!(!manifest.is_valid());
        manifest.entries[0].relative_path = "tasks/plan/OVERVIEW.md".to_owned();
        manifest.entries[0].mode = 0o644;
        assert!(!manifest.is_valid());

        manifest.entries[0].mode = 0o600;
        manifest.entries[0].relative_path = "tasks/plan/UNREVIEWED.md".to_owned();
        assert!(!manifest.is_valid());
        manifest.entries[0].relative_path = "scratch/plan.md".to_owned();
        assert!(!manifest.is_valid());

        manifest.entries[0].relative_path = "tasks/plan/OVERVIEW.md".to_owned();
        manifest.entries.push(DocumentManifestEntry {
            relative_path: "tasks/PLAN/overview.md".to_owned(),
            sha256: "e".repeat(64),
            byte_length: 42,
            mode: 0o600,
        });
        assert!(!manifest.is_valid());

        manifest.entries.pop();
        manifest.entries[0].relative_path = "tasks/plan/é.md".to_owned();
        assert!(!manifest.is_valid());
    }

    #[test]
    fn document_manifest_accepts_only_the_declared_markdown_root_shapes() {
        let accepted = [
            "INDEX.md",
            "profile/USER.md",
            "profile/COMMUNICATION.md",
            "sources/INDEX.md",
            "sources/chatgpt-export.md",
            "tasks/demo/OVERVIEW.md",
            "tasks/demo/STATE.md",
            "tasks/demo/DECISIONS.md",
            "tasks/demo/QUESTIONS.md",
            "tasks/demo/MODEL_BRIEF.md",
            "tasks/demo/paths/backup-plan.md",
            "tasks/demo/updates/owner-update.md",
            "sessions/session-1/SESSION.md",
            "sessions/session-1/choice-sets/choices-1.md",
        ];
        for path in accepted {
            let entries = vec![DocumentManifestEntry {
                relative_path: path.to_owned(),
                sha256: "c".repeat(64),
                byte_length: 42,
                mode: 0o600,
            }];
            assert!(
                canonical_document_manifest_digest(&entries).is_some(),
                "{path}"
            );
        }

        for path in [
            "tasks/demo/UNREVIEWED.md",
            "tasks/demo/paths/backup-plan.txt",
            "tasks/demo/paths/archive.zip",
            "tasks/demo/paths/backup/plan.md",
            "sessions/session-1/choice-sets/choices-1.md/extra",
            "sources/.md",
            "profile/USER.MD",
        ] {
            let entries = vec![DocumentManifestEntry {
                relative_path: path.to_owned(),
                sha256: "c".repeat(64),
                byte_length: 42,
                mode: 0o600,
            }];
            assert!(
                canonical_document_manifest_digest(&entries).is_none(),
                "{path}"
            );
        }
    }

    #[test]
    fn document_manifest_digest_is_canonical_and_cannot_be_caller_supplied() {
        let first = DocumentManifestEntry {
            relative_path: "sessions/session-1/SESSION.md".to_owned(),
            sha256: "a".repeat(64),
            byte_length: 64,
            mode: 0o600,
        };
        let second = DocumentManifestEntry {
            relative_path: "sessions/session-1/choice-sets/choices-1.md".to_owned(),
            sha256: "b".repeat(64),
            byte_length: 128,
            mode: 0o600,
        };
        let digest = canonical_document_manifest_digest(&[first.clone(), second.clone()])
            .expect("canonical digest");
        assert_eq!(
            digest,
            "2556788916fce4a341d7a2a2fbbd81c51d97d2af8da83e2fe890530822f4f8e7"
        );
        assert_eq!(
            canonical_document_manifest_digest(&[second.clone(), first.clone()]),
            Some(digest.clone())
        );
        let manifest = DocumentManifest {
            root_version: 1,
            entries: vec![first, second],
            aggregate_digest: digest,
            generated_at_ms: 1,
        };
        assert!(manifest.is_valid());
        let mut rebound = manifest;
        rebound.aggregate_digest = "f".repeat(64);
        assert!(!rebound.is_valid());
    }

    #[test]
    fn digest_contract_accepts_only_lowercase_hex() {
        assert!(sha256_hex(&"a1".repeat(32)));
        assert!(!sha256_hex(&format!("g{}", "a".repeat(63))));
    }

    #[test]
    fn consolidated_confirmation_has_a_cross_language_typed_golden_vector() {
        let markdown_entry = DocumentManifestEntry {
            relative_path: "sessions/session-1/CHOICE.md".to_owned(),
            sha256: "f".repeat(64),
            byte_length: 64,
            mode: 0o600,
        };
        let mut confirmation = ChoiceConsolidatedConfirmation {
            id: "confirmation-1".to_owned(),
            choice_session_id: "session-1".to_owned(),
            choice_set_id: "choices-1".to_owned(),
            selection_id: "selection-1".to_owned(),
            expected_session_revision: 1,
            interpretation_revision: 1,
            payload_revision: 1,
            payload_digest: String::new(),
            goal: "Prepare a bounded next step".to_owned(),
            steps: vec!["Review the prepared plan".to_owned()],
            markdown_entry: markdown_entry.clone(),
            markdown_expected_base: None,
            markdown_manifest_digests: vec![
                "b".repeat(64),
                canonical_document_manifest_digest(&[markdown_entry])
                    .expect("canonical desired manifest"),
            ],
            document_diff_digest: "c".repeat(64),
            model_provenance: ModelProvenance {
                id: "provenance-1".to_owned(),
                model_id: "gpt-test-model".to_owned(),
                requested_effort: "not_applicable".to_owned(),
                actual_effort: "not_applicable".to_owned(),
                catalog_fingerprint: "d".repeat(64),
                catalog_revision: 1,
                account_display_class: "ChatGPT account".to_owned(),
                protocol_schema_revision: 1,
                turn_id: "turn-1".to_owned(),
            },
            persona_revision: crate::PersonaRevisionRef {
                persona_id: "openopen.nondev.default".to_owned(),
                revision: "draft-03-en".to_owned(),
                aggregate_digest: "e".repeat(64),
                instructions_digest: "f".repeat(64),
            },
            reminder_list_id: "openopen-default-reminders".to_owned(),
            reminder_items: vec![ChoiceReminderItem {
                id: "reminder-1".to_owned(),
                text: "Review the prepared plan".to_owned(),
                due_at_ms: 1,
                time_zone: "Etc/UTC".to_owned(),
                evidence_intent: "reminder-readback".to_owned(),
            }],
            reminder_count: 1,
            reminder_payload_digest: String::new(),
            evidence_requirements: vec!["Reminder readback before Done".to_owned()],
            delivery_binding_id: None,
            recipient: None,
            delivery_scope: None,
            data_categories: vec!["local task state".to_owned()],
            retention: "Local until user deletion".to_owned(),
            permissions: vec![],
            effect_classes: vec!["reminder".to_owned()],
            confirmed_at_ms: 1,
        };
        confirmation.reminder_payload_digest = confirmation
            .canonical_reminder_payload_digest()
            .expect("canonical Reminder payload");
        let preimage = confirmation
            .canonical_payload_preimage()
            .expect("typed confirmation preimage");
        let digest = confirmation
            .canonical_payload_digest()
            .expect("typed confirmation digest");
        assert_eq!(preimage.len(), 2_439);
        assert_eq!(
            digest,
            "5a7b8b6468fc9b773e9605d59c7bb4710c945baea9cb2f2ee155fb8f4626f7f9"
        );
        confirmation.payload_digest = digest;
        assert!(confirmation.is_valid());
    }

    #[test]
    fn choice_session_and_batch_enforce_bounded_time_and_revision_contracts() {
        let session = ChoiceSession {
            id: "session-1".to_owned(),
            state: ChoiceSessionState::Active,
            revision: 1,
            model_selection_state: ModelSelectionState::Unselected,
            communication_profile_revision: 0,
            active_choice_set_id: None,
            active_interpretation_revision: None,
            opened_at_ms: 100,
            last_input_at_ms: 200,
            soft_idle_at_ms: 200 + CHOICE_SESSION_SOFT_IDLE_MS,
            stale_review_at_ms: 200 + CHOICE_SESSION_STALE_REVIEW_MS,
            primary_delivery_binding_id: None,
            pending_confirmation_id: None,
            background_mission_ids: vec![],
        };
        assert!(session.is_valid());
        let mut stale_timer = session.clone();
        stale_timer.soft_idle_at_ms += 1;
        assert!(!stale_timer.is_valid());

        let batch = ConversationTurnBatch {
            id: "batch-1".to_owned(),
            choice_session_id: session.id,
            delivery_binding_id: "binding-1".to_owned(),
            source_envelope_ids: vec!["source-1".to_owned()],
            opened_at_ms: 200,
            quiet_deadline_ms: 200 + CHOICE_BATCH_QUIET_WINDOW_MS,
            hard_deadline_ms: 200 + CHOICE_BATCH_HARD_WINDOW_MS,
            sealed_at_ms: Some(200 + CHOICE_BATCH_QUIET_WINDOW_MS),
            seal_reason: Some(BatchSealReason::QuietDeadline),
            revision: 1,
        };
        assert!(batch.is_valid());
        let mut missing_binding = batch.clone();
        missing_binding.delivery_binding_id.clear();
        assert!(!missing_binding.is_valid());
        let mut invalid_batch = batch.clone();
        invalid_batch.seal_reason = None;
        assert!(!invalid_batch.is_valid());

        let mut early_quiet = batch.clone();
        early_quiet.sealed_at_ms = Some(201);
        assert!(!early_quiet.is_valid());
        let mut early_hard = batch.clone();
        early_hard.seal_reason = Some(BatchSealReason::HardDeadline);
        assert!(!early_hard.is_valid());
        let mut hard = batch.clone();
        hard.sealed_at_ms = Some(hard.hard_deadline_ms);
        hard.seal_reason = Some(BatchSealReason::HardDeadline);
        assert!(hard.is_valid());

        let mut replacement = batch.clone();
        replacement.id = "batch-2".to_owned();
        replacement.opened_at_ms = batch.sealed_at_ms.expect("sealed batch");
        replacement.quiet_deadline_ms = replacement.opened_at_ms + CHOICE_BATCH_QUIET_WINDOW_MS;
        replacement.hard_deadline_ms = replacement.opened_at_ms + CHOICE_BATCH_HARD_WINDOW_MS;
        replacement.sealed_at_ms = None;
        replacement.seal_reason = None;
        replacement.revision = 2;
        assert!(super::batch_transition_allowed(
            Some(&batch),
            Some(&replacement),
            replacement.revision
        ));

        let mut unsealed = batch.clone();
        unsealed.sealed_at_ms = None;
        unsealed.seal_reason = None;
        assert!(!super::batch_transition_allowed(
            Some(&unsealed),
            Some(&replacement),
            replacement.revision
        ));
    }

    #[test]
    fn choice_session_idle_and_stale_states_require_a_safe_recap_path() {
        use ChoiceSessionState::{
            Active, AwaitingConfirmation, Cancelled, Executing, SoftIdle, StaleReview,
        };

        // A soft idle may refresh the existing foreground ChoiceSet, but it
        // cannot jump directly into confirmation or execution.
        assert!(super::choice_session_transition_allowed(Active, SoftIdle));
        assert!(super::choice_session_transition_allowed(SoftIdle, Active));
        assert!(super::choice_session_transition_allowed(
            SoftIdle,
            StaleReview
        ));
        assert!(!super::choice_session_transition_allowed(
            SoftIdle,
            AwaitingConfirmation
        ));
        assert!(!super::choice_session_transition_allowed(
            SoftIdle, Executing
        ));

        // A 24-hour stale review likewise requires a fresh active/refining
        // recap before any confirmation, and terminal cancellation never
        // reopens the old foreground session.
        assert!(super::choice_session_transition_allowed(
            StaleReview,
            Active
        ));
        assert!(!super::choice_session_transition_allowed(
            StaleReview,
            AwaitingConfirmation
        ));
        assert!(!super::choice_session_transition_allowed(
            StaleReview,
            Executing
        ));
        assert!(!super::choice_session_transition_allowed(Cancelled, Active));
    }

    #[test]
    fn terminal_sessions_cannot_retain_replayable_choice_state() {
        let manifest_entries = vec![DocumentManifestEntry {
            relative_path: "sessions/session-1/SESSION.md".to_owned(),
            sha256: "a".repeat(64),
            byte_length: 64,
            mode: 0o600,
        }];
        let manifest = DocumentManifest {
            root_version: 1,
            aggregate_digest: canonical_document_manifest_digest(&manifest_entries)
                .expect("canonical digest"),
            entries: manifest_entries,
            generated_at_ms: 1,
        };
        let session = ChoiceSession {
            id: "session-1".to_owned(),
            state: ChoiceSessionState::Completed,
            revision: 2,
            model_selection_state: ModelSelectionState::Unselected,
            communication_profile_revision: 0,
            active_choice_set_id: Some("choices-1".to_owned()),
            active_interpretation_revision: None,
            opened_at_ms: 0,
            last_input_at_ms: 1,
            soft_idle_at_ms: 1 + CHOICE_SESSION_SOFT_IDLE_MS,
            stale_review_at_ms: 1 + CHOICE_SESSION_STALE_REVIEW_MS,
            primary_delivery_binding_id: None,
            pending_confirmation_id: None,
            background_mission_ids: vec![],
        };
        let snapshot = super::ChoiceLoopSnapshot {
            session,
            active_batch: None,
            interpretation: None,
            active_choice_set: None,
            last_selection: None,
            pending_refinement_operation: None,
            confirmation: None,
            document_manifest: manifest,
        };
        assert!(!snapshot.is_valid());
    }
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
