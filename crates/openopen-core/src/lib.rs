//! Local, fail-closed `OpenOpen` product core.

mod crypto;
mod effect;
mod gate;
mod mission;
mod store;

pub use crypto::{CryptoError, EvidenceClaims, LocalAuthority};
pub use effect::{
    BrokerEnrollmentRecord, EffectProtocolError, TrustedBrokerEnrollment,
    broker_enrollment_signing_bytes, verify_effect_noncommit, verify_effect_receipt,
};
pub use gate::{ActionGate, ActionProposal, ActionTarget, EffectKind, GateDecision};
pub use mission::{
    ApprovalDecision, CreateMission, CreateWorkItem, MissionCommand, MissionError,
    NewBoundaryApproval, NewReceipt, add_evidence, approve_request, issue_receipt,
    request_scope_change, transition_mission, transition_work_item,
};
pub use store::{
    AuditAnchor, EnvelopeInsert, MissionCommandEnvelope, MissionCommandResult, Store, StoreError,
};
