//! Local, fail-closed `OpenOpen` product core.

mod channel;
mod crypto;
mod effect;
mod gate;
mod markdown;
mod mission;
mod store;

pub use channel::ChannelError;
pub use channel::{channel_message_payload, channel_need_you_content, channel_receipt_content};
pub use crypto::{CryptoError, EvidenceClaims, LocalAuthority};
pub use effect::{
    BrokerEnrollmentRecord, EffectProtocolError, TrustedBrokerEnrollment,
    authorize_broker_enrollment, broker_enrollment_signing_bytes, verify_core_instance_lease,
    verify_effect_noncommit, verify_effect_receipt,
};
pub use gate::{ActionGate, ActionProposal, ActionTarget, EffectKind, GateDecision};
pub use mission::{
    ApprovalDecision, CreateMission, CreateWorkItem, MissionCommand, MissionError,
    NewBoundaryApproval, NewReceipt, add_evidence, approve_request, issue_receipt,
    request_scope_change, transition_mission, transition_work_item,
};
pub use store::{
    AuditAnchor, B2MemoryPreparedSourceRecord, ChoiceIdleAdvance, ChoiceIdleClockEvidence,
    MarkdownRenderCleanup, MarkdownRenderPublication, MissionCommandEnvelope, MissionCommandResult,
    RuntimeControl, Store, StoreError,
};
