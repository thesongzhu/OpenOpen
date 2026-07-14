use crate::{CryptoError, LocalAuthority};
use openopen_protocol::{
    ApprovalKind, ApprovalRequest, ApprovalStatus, EvidenceKind, EvidenceRef, Mission,
    MissionStatus, NeedsMe, Receipt, WorkItem, WorkItemStatus,
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use thiserror::Error;

const MAX_MISSION_ID_BYTES: usize = 64;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ApprovalDecision {
    Approve,
    Reject,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateWorkItem {
    pub id: String,
    pub title: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateMission {
    pub mission_id: String,
    pub title: String,
    pub outcome: String,
    pub owner_id: String,
    pub scope_digest: String,
    pub scope_approval_id: String,
    pub scope_approval_prompt: String,
    pub work_items: Vec<CreateWorkItem>,
    pub now_ms: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NewBoundaryApproval {
    pub id: String,
    pub kind: ApprovalKind,
    pub prompt: String,
    pub scope_digest: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NewReceipt {
    pub id: String,
    pub summary: String,
    pub actual_model: String,
    pub output_hashes: Vec<String>,
    pub completed_at_ms: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum MissionCommand {
    Create {
        input: CreateMission,
    },
    BeginConfirmation {
        mission_id: String,
        now_ms: i64,
    },
    DecideApproval {
        mission_id: String,
        approval_id: String,
        actor_id: String,
        decision: ApprovalDecision,
        now_ms: i64,
    },
    Activate {
        mission_id: String,
        now_ms: i64,
    },
    RequestScopeChange {
        mission_id: String,
        approval: NewBoundaryApproval,
        needs_me_id: String,
        now_ms: i64,
    },
    Pause {
        mission_id: String,
        now_ms: i64,
    },
    Resume {
        mission_id: String,
        now_ms: i64,
    },
    Fail {
        mission_id: String,
        now_ms: i64,
    },
    Cancel {
        mission_id: String,
        now_ms: i64,
    },
    TransitionWorkItem {
        mission_id: String,
        work_item_id: String,
        next: WorkItemStatus,
        evidence_ids: Vec<String>,
        now_ms: i64,
    },
    RequestWorkItemBoundary {
        mission_id: String,
        work_item_id: String,
        approval: NewBoundaryApproval,
        now_ms: i64,
    },
    AttachEvidence {
        mission_id: String,
        evidence: EvidenceRef,
        now_ms: i64,
    },
    Complete {
        mission_id: String,
        receipt: NewReceipt,
        now_ms: i64,
    },
}

impl MissionCommand {
    #[must_use]
    pub fn mission_id(&self) -> &str {
        match self {
            Self::Create { input } => &input.mission_id,
            Self::BeginConfirmation { mission_id, .. }
            | Self::DecideApproval { mission_id, .. }
            | Self::Activate { mission_id, .. }
            | Self::RequestScopeChange { mission_id, .. }
            | Self::Pause { mission_id, .. }
            | Self::Resume { mission_id, .. }
            | Self::Fail { mission_id, .. }
            | Self::Cancel { mission_id, .. }
            | Self::TransitionWorkItem { mission_id, .. }
            | Self::RequestWorkItemBoundary { mission_id, .. }
            | Self::AttachEvidence { mission_id, .. }
            | Self::Complete { mission_id, .. } => mission_id,
        }
    }

    #[must_use]
    pub const fn action(&self) -> &'static str {
        match self {
            Self::Create { .. } => "mission.command.create",
            Self::BeginConfirmation { .. } => "mission.command.begin_confirmation",
            Self::DecideApproval { .. } => "mission.command.decide_approval",
            Self::Activate { .. } => "mission.command.activate",
            Self::RequestScopeChange { .. } => "mission.command.request_scope_change",
            Self::Pause { .. } => "mission.command.pause",
            Self::Resume { .. } => "mission.command.resume",
            Self::Fail { .. } => "mission.command.fail",
            Self::Cancel { .. } => "mission.command.cancel",
            Self::TransitionWorkItem { .. } => "mission.command.transition_work_item",
            Self::RequestWorkItemBoundary { .. } => "mission.command.request_work_item_boundary",
            Self::AttachEvidence { .. } => "mission.command.attach_evidence",
            Self::Complete { .. } => "mission.command.complete",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AppliedMissionCommand {
    pub mission: Mission,
    pub receipt: Option<Receipt>,
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum MissionError {
    #[error("invalid mission transition: {from:?} -> {to:?}")]
    InvalidTransition {
        from: MissionStatus,
        to: MissionStatus,
    },
    #[error("invalid work item transition: {from:?} -> {to:?}")]
    InvalidWorkItemTransition {
        from: WorkItemStatus,
        to: WorkItemStatus,
    },
    #[error("mission completion requires verified outcome evidence for every work item")]
    MissingOutcomeEvidence,
    #[error("mission completion requires every work item to be completed")]
    IncompleteWorkItems,
    #[error("mission completion requires at least one work item")]
    MissingWorkItems,
    #[error("scope confirmation is missing or does not match the current scope")]
    MissingScopeApproval,
    #[error("the pending Need you approval is not owner-approved")]
    PendingApproval,
    #[error("only the Mission owner can approve this request")]
    NotMissionOwner,
    #[error("approval request was not found or is no longer pending")]
    ApprovalNotPending,
    #[error("approval cannot be decided outside its exact Need you boundary")]
    ApprovalDecisionNotAllowed,
    #[error("approval request id must be unique inside a Mission")]
    DuplicateApprovalId,
    #[error("scope change requires an active or paused Mission")]
    ScopeChangeNotAllowed,
    #[error("scope change approval must be pending and requested by the Mission owner")]
    InvalidScopeApproval,
    #[error("work item was not found")]
    WorkItemNotFound,
    #[error("evidence id was reused with different signed claims")]
    EvidenceConflict,
    #[error("evidence signature failed verification")]
    InvalidEvidence,
    #[error("evidence is not bound to this Mission and WorkItem")]
    EvidenceScopeMismatch,
    #[error("evidence observation cannot occur after attachment or completion")]
    EvidenceTimeMismatch,
    #[error("WorkItem NeedsMe requires an exact owner approval")]
    WorkItemApprovalRequired,
    #[error("WorkItem transitions require an active parent Mission")]
    ParentMissionNotActive,
    #[error("Mission contains an invalid approval record")]
    InvalidApproval,
    #[error("Mission snapshot invariant failed: {0}")]
    InvalidMissionSnapshot(&'static str),
    #[error("Receipt requires a completed Mission")]
    MissionNotCompleted,
    #[error("Receipt requires the actual non-empty model id")]
    MissingActualModel,
    #[error("Receipt does not match the completed Mission")]
    ReceiptMismatch,
    #[error("Mission command requires an existing Mission")]
    MissionNotFound,
    #[error("CreateMission cannot replace an existing Mission")]
    MissionAlreadyExists,
    #[error("Mission command id does not match the loaded Mission")]
    CommandMissionMismatch,
    #[error("Mission command time cannot move backwards")]
    StaleCommandTime,
    #[error("Mission id must use the canonical lowercase ASCII identifier syntax")]
    InvalidMissionId,
}

pub(crate) fn apply_mission_command(
    current: Option<Mission>,
    command: &MissionCommand,
    authority: &LocalAuthority,
) -> Result<AppliedMissionCommand, MissionError> {
    if !is_canonical_mission_id(command.mission_id()) {
        return Err(MissionError::InvalidMissionId);
    }
    let (mission, receipt) = if let MissionCommand::Create { input } = command {
        if current.is_some() {
            return Err(MissionError::MissionAlreadyExists);
        }
        (create_mission(input)?, None)
    } else {
        let mut mission = current.ok_or(MissionError::MissionNotFound)?;
        if mission.id != command.mission_id() {
            return Err(MissionError::CommandMissionMismatch);
        }
        let now_ms = command_time(command);
        if now_ms < mission.updated_at_ms {
            return Err(MissionError::StaleCommandTime);
        }
        let receipt = apply_existing_command(&mut mission, command, authority)?;
        (mission, receipt)
    };
    validate_mission_snapshot(&mission, authority)?;
    if let Some(receipt) = receipt.as_ref() {
        validate_receipt(&mission, receipt, authority)?;
    }
    Ok(AppliedMissionCommand { mission, receipt })
}

fn create_mission(input: &CreateMission) -> Result<Mission, MissionError> {
    if input.work_items.is_empty() {
        return Err(MissionError::MissingWorkItems);
    }
    let mission = Mission {
        id: input.mission_id.clone(),
        title: input.title.clone(),
        outcome: input.outcome.clone(),
        owner_id: input.owner_id.clone(),
        scope_digest: input.scope_digest.clone(),
        status: MissionStatus::Proposed,
        work_items: input
            .work_items
            .iter()
            .map(|item| WorkItem {
                id: item.id.clone(),
                title: item.title.clone(),
                status: WorkItemStatus::Pending,
                evidence_ids: Vec::new(),
                pending_approval_id: None,
            })
            .collect(),
        approvals: vec![ApprovalRequest {
            id: input.scope_approval_id.clone(),
            work_item_id: None,
            kind: ApprovalKind::MissionScope,
            prompt: input.scope_approval_prompt.clone(),
            scope_digest: input.scope_digest.clone(),
            status: ApprovalStatus::Pending,
            requested_by_id: input.owner_id.clone(),
            decided_by_id: None,
            requested_at_ms: input.now_ms,
            decided_at_ms: None,
        }],
        needs_me: None,
        evidence: Vec::new(),
        created_at_ms: input.now_ms,
        updated_at_ms: input.now_ms,
    };
    validate_snapshot_identity_and_uniqueness(&mission)?;
    validate_snapshot_approvals(&mission)?;
    Ok(mission)
}

fn apply_existing_command(
    mission: &mut Mission,
    command: &MissionCommand,
    authority: &LocalAuthority,
) -> Result<Option<Receipt>, MissionError> {
    match command {
        MissionCommand::Create { .. } => unreachable!("Create handled before loading a Mission"),
        MissionCommand::BeginConfirmation { now_ms, .. } => {
            transition_mission(
                mission,
                MissionStatus::AwaitingConfirmation,
                *now_ms,
                authority,
            )?;
        }
        MissionCommand::DecideApproval {
            approval_id,
            actor_id,
            decision,
            now_ms,
            ..
        } => decide_request(mission, approval_id, actor_id, *decision, *now_ms)?,
        MissionCommand::Activate { now_ms, .. } | MissionCommand::Resume { now_ms, .. } => {
            transition_mission(mission, MissionStatus::Active, *now_ms, authority)?;
        }
        MissionCommand::Pause { now_ms, .. } => {
            transition_mission(mission, MissionStatus::Paused, *now_ms, authority)?;
        }
        MissionCommand::Fail { now_ms, .. } => {
            transition_mission(mission, MissionStatus::Failed, *now_ms, authority)?;
        }
        MissionCommand::Cancel { now_ms, .. } => {
            transition_mission(mission, MissionStatus::Cancelled, *now_ms, authority)?;
        }
        MissionCommand::RequestScopeChange {
            approval,
            needs_me_id,
            now_ms,
            ..
        } => {
            let request = new_approval(mission, approval, None, *now_ms);
            request_scope_change(
                mission,
                request,
                NeedsMe {
                    id: needs_me_id.clone(),
                    prompt: approval.prompt.clone(),
                    approval_id: Some(approval.id.clone()),
                    created_at_ms: *now_ms,
                },
                *now_ms,
            )?;
        }
        MissionCommand::TransitionWorkItem {
            work_item_id,
            next,
            evidence_ids,
            now_ms,
            ..
        } => transition_work_item(
            mission,
            work_item_id,
            *next,
            evidence_ids,
            None,
            *now_ms,
            authority,
        )?,
        MissionCommand::RequestWorkItemBoundary {
            work_item_id,
            approval,
            now_ms,
            ..
        } => {
            let request = new_approval(mission, approval, Some(work_item_id.clone()), *now_ms);
            if mission.approvals.iter().any(|item| item.id == request.id) {
                return Err(MissionError::DuplicateApprovalId);
            }
            mission.approvals.push(request);
            transition_work_item(
                mission,
                work_item_id,
                WorkItemStatus::NeedsMe,
                &[],
                Some(&approval.id),
                *now_ms,
                authority,
            )?;
        }
        MissionCommand::AttachEvidence {
            evidence, now_ms, ..
        } => add_evidence(mission, evidence.clone(), *now_ms, authority)?,
        MissionCommand::Complete {
            receipt, now_ms, ..
        } => return complete_mission(mission, receipt, *now_ms, authority).map(Some),
    }
    Ok(None)
}

fn complete_mission(
    mission: &mut Mission,
    receipt: &NewReceipt,
    now_ms: i64,
    authority: &LocalAuthority,
) -> Result<Receipt, MissionError> {
    transition_mission(mission, MissionStatus::Completed, now_ms, authority)?;
    issue_receipt(
        mission,
        receipt.id.clone(),
        receipt.summary.clone(),
        receipt.actual_model.clone(),
        receipt.output_hashes.clone(),
        receipt.completed_at_ms,
        authority,
    )
}

fn new_approval(
    mission: &Mission,
    input: &NewBoundaryApproval,
    work_item_id: Option<String>,
    now_ms: i64,
) -> ApprovalRequest {
    ApprovalRequest {
        id: input.id.clone(),
        work_item_id,
        kind: input.kind,
        prompt: input.prompt.clone(),
        scope_digest: input.scope_digest.clone(),
        status: ApprovalStatus::Pending,
        requested_by_id: mission.owner_id.clone(),
        decided_by_id: None,
        requested_at_ms: now_ms,
        decided_at_ms: None,
    }
}

const fn command_time(command: &MissionCommand) -> i64 {
    match command {
        MissionCommand::Create { input } => input.now_ms,
        MissionCommand::BeginConfirmation { now_ms, .. }
        | MissionCommand::DecideApproval { now_ms, .. }
        | MissionCommand::Activate { now_ms, .. }
        | MissionCommand::RequestScopeChange { now_ms, .. }
        | MissionCommand::Pause { now_ms, .. }
        | MissionCommand::Resume { now_ms, .. }
        | MissionCommand::Fail { now_ms, .. }
        | MissionCommand::Cancel { now_ms, .. }
        | MissionCommand::TransitionWorkItem { now_ms, .. }
        | MissionCommand::RequestWorkItemBoundary { now_ms, .. }
        | MissionCommand::AttachEvidence { now_ms, .. }
        | MissionCommand::Complete { now_ms, .. } => *now_ms,
    }
}

/// Advances a Mission only through the fixed lifecycle and revalidates proof at
/// the completion boundary.
///
/// # Errors
///
/// Returns an error for an illegal edge, missing owner approval, incomplete
/// work, or evidence that is absent, non-outcome, or signature-invalid.
pub fn transition_mission(
    mission: &mut Mission,
    next: MissionStatus,
    now_ms: i64,
    authority: &LocalAuthority,
) -> Result<(), MissionError> {
    ensure_monotonic_time(mission, now_ms)?;
    if !can_transition(mission.status, next) {
        return Err(MissionError::InvalidTransition {
            from: mission.status,
            to: next,
        });
    }

    if next == MissionStatus::Active {
        match mission.status {
            MissionStatus::AwaitingConfirmation => {
                if !has_owner_approval(mission, ApprovalKind::MissionScope, &mission.scope_digest) {
                    return Err(MissionError::MissingScopeApproval);
                }
            }
            MissionStatus::NeedsMe => validate_mission_need_you(mission, true)?,
            MissionStatus::Paused if mission.needs_me.is_some() => {
                validate_mission_need_you(mission, true)?;
            }
            _ => {}
        }
    }

    if next == MissionStatus::Completed {
        validate_completion(mission, authority)?;
    }

    if matches!(next, MissionStatus::Failed | MissionStatus::Cancelled) {
        let child_status = if next == MissionStatus::Failed {
            WorkItemStatus::Failed
        } else {
            WorkItemStatus::Cancelled
        };
        for item in &mut mission.work_items {
            if !matches!(
                item.status,
                WorkItemStatus::Completed | WorkItemStatus::Failed | WorkItemStatus::Cancelled
            ) {
                item.status = child_status;
                item.pending_approval_id = None;
            }
        }
    }

    mission.status = next;
    mission.updated_at_ms = now_ms;
    if matches!(
        next,
        MissionStatus::Active
            | MissionStatus::Completed
            | MissionStatus::Failed
            | MissionStatus::Cancelled
    ) {
        mission.needs_me = None;
    }
    Ok(())
}

/// Adds only evidence signed by the local Rust authority. Identical retries are
/// idempotent; reusing an id with different claims fails closed.
///
/// # Errors
///
/// Returns an error when the signature is invalid or an existing evidence id
/// has different contents.
pub fn add_evidence(
    mission: &mut Mission,
    evidence: EvidenceRef,
    now_ms: i64,
    authority: &LocalAuthority,
) -> Result<(), MissionError> {
    ensure_monotonic_time(mission, now_ms)?;
    authority
        .verify_evidence(&evidence)
        .map_err(|_| MissionError::InvalidEvidence)?;
    if evidence.observed_at_ms > now_ms {
        return Err(MissionError::EvidenceTimeMismatch);
    }
    if evidence.mission_id != mission.id
        || !mission
            .work_items
            .iter()
            .any(|item| item.id == evidence.work_item_id)
    {
        return Err(MissionError::EvidenceScopeMismatch);
    }
    if let Some(existing) = mission
        .evidence
        .iter()
        .find(|existing| existing.id == evidence.id)
    {
        return if existing == &evidence {
            Ok(())
        } else {
            Err(MissionError::EvidenceConflict)
        };
    }
    mission.evidence.push(evidence);
    mission.updated_at_ms = now_ms;
    Ok(())
}

/// Changes a pending approval to approved, binding the decision to the Mission
/// owner identity.
///
/// # Errors
///
/// Returns an error for a non-owner actor or an unknown/already-decided request.
pub fn approve_request(
    mission: &mut Mission,
    approval_id: &str,
    actor_id: &str,
    now_ms: i64,
) -> Result<(), MissionError> {
    decide_request(
        mission,
        approval_id,
        actor_id,
        ApprovalDecision::Approve,
        now_ms,
    )
}

fn decide_request(
    mission: &mut Mission,
    approval_id: &str,
    actor_id: &str,
    decision: ApprovalDecision,
    now_ms: i64,
) -> Result<(), MissionError> {
    ensure_monotonic_time(mission, now_ms)?;
    if actor_id != mission.owner_id {
        return Err(MissionError::NotMissionOwner);
    }
    let matching_count = mission
        .approvals
        .iter()
        .filter(|approval| approval.id == approval_id)
        .count();
    if matching_count == 0 {
        return Err(MissionError::ApprovalNotPending);
    }
    if matching_count > 1 {
        return Err(MissionError::DuplicateApprovalId);
    }
    let approval_index = mission
        .approvals
        .iter()
        .position(|approval| {
            approval.id == approval_id && approval.status == ApprovalStatus::Pending
        })
        .ok_or(MissionError::ApprovalNotPending)?;
    if !approval_is_decidable(mission, &mission.approvals[approval_index]) {
        return Err(MissionError::ApprovalDecisionNotAllowed);
    }
    let kind = mission.approvals[approval_index].kind;
    let work_item_id = mission.approvals[approval_index].work_item_id.clone();
    let approval = &mut mission.approvals[approval_index];
    approval.status = match decision {
        ApprovalDecision::Approve => ApprovalStatus::Approved,
        ApprovalDecision::Reject => ApprovalStatus::Rejected,
    };
    approval.decided_by_id = Some(actor_id.to_owned());
    approval.decided_at_ms = Some(now_ms);
    if decision == ApprovalDecision::Reject {
        if let Some(work_item_id) = work_item_id {
            let item = mission
                .work_items
                .iter_mut()
                .find(|item| item.id == work_item_id)
                .ok_or(MissionError::WorkItemNotFound)?;
            item.status = WorkItemStatus::Active;
            item.pending_approval_id = None;
        } else if kind == ApprovalKind::MissionScope {
            for item in &mut mission.work_items {
                item.status = WorkItemStatus::Cancelled;
                item.pending_approval_id = None;
            }
            mission.status = MissionStatus::Cancelled;
            mission.needs_me = None;
        } else {
            mission.status = MissionStatus::Active;
            mission.needs_me = None;
        }
    }
    mission.updated_at_ms = now_ms;
    Ok(())
}

/// Moves an active/paused Mission to `NeedsMe` for a fixed boundary change.
///
/// # Errors
///
/// Returns an error when the Mission is outside an executable state or the
/// request is not a pending owner-originated boundary approval.
pub fn request_scope_change(
    mission: &mut Mission,
    approval: ApprovalRequest,
    needs_me: NeedsMe,
    now_ms: i64,
) -> Result<(), MissionError> {
    ensure_monotonic_time(mission, now_ms)?;
    if mission.needs_me.is_some()
        || !matches!(
            mission.status,
            MissionStatus::Active | MissionStatus::Paused
        )
        || !matches!(
            approval.kind,
            ApprovalKind::ExpandedScope
                | ApprovalKind::NewRecipient
                | ApprovalKind::NewDataShare
                | ApprovalKind::NewExternalWrite
                | ApprovalKind::Cost
                | ApprovalKind::DeleteOrIrreversible
        )
    {
        return Err(MissionError::ScopeChangeNotAllowed);
    }
    if approval.status != ApprovalStatus::Pending
        || approval.requested_by_id != mission.owner_id
        || approval.work_item_id.is_some()
        || approval.decided_by_id.is_some()
        || needs_me.approval_id.as_deref() != Some(approval.id.as_str())
    {
        return Err(MissionError::InvalidScopeApproval);
    }
    if mission
        .approvals
        .iter()
        .any(|existing| existing.id == approval.id)
    {
        return Err(MissionError::DuplicateApprovalId);
    }
    mission.approvals.push(approval);
    mission.needs_me = Some(needs_me);
    mission.status = MissionStatus::NeedsMe;
    mission.updated_at_ms = now_ms;
    Ok(())
}

/// Advances a `WorkItem` through its controlled lifecycle. Completion requires
/// at least one signed outcome Evidence reference attached to that `WorkItem`.
///
/// # Errors
///
/// Returns an error for a missing item, illegal edge, or invalid completion
/// evidence.
pub fn transition_work_item(
    mission: &mut Mission,
    work_item_id: &str,
    next: WorkItemStatus,
    evidence_ids: &[String],
    approval_id: Option<&str>,
    now_ms: i64,
    authority: &LocalAuthority,
) -> Result<(), MissionError> {
    ensure_monotonic_time(mission, now_ms)?;
    let index = mission
        .work_items
        .iter()
        .position(|item| item.id == work_item_id)
        .ok_or(MissionError::WorkItemNotFound)?;
    let current = mission.work_items[index].status;
    if mission.status != MissionStatus::Active {
        return Err(MissionError::ParentMissionNotActive);
    }
    if !can_transition_work_item(current, next) {
        return Err(MissionError::InvalidWorkItemTransition {
            from: current,
            to: next,
        });
    }
    if current == WorkItemStatus::Active && next == WorkItemStatus::NeedsMe {
        let approval_id = approval_id.ok_or(MissionError::WorkItemApprovalRequired)?;
        let matching: Vec<_> = mission
            .approvals
            .iter()
            .filter(|approval| approval.id == approval_id)
            .collect();
        if matching.len() != 1
            || matching[0].status != ApprovalStatus::Pending
            || matching[0].requested_by_id != mission.owner_id
            || matching[0].work_item_id.as_deref() != Some(work_item_id)
            || !is_work_item_boundary_kind(matching[0].kind)
            || matching[0].scope_digest.trim().is_empty()
            || matching[0].decided_by_id.is_some()
        {
            return Err(MissionError::WorkItemApprovalRequired);
        }
    }
    if current == WorkItemStatus::NeedsMe && next == WorkItemStatus::Active {
        let pending_id = mission.work_items[index]
            .pending_approval_id
            .as_deref()
            .ok_or(MissionError::WorkItemApprovalRequired)?;
        let matching: Vec<_> = mission
            .approvals
            .iter()
            .filter(|approval| approval.id == pending_id)
            .collect();
        if matching.len() != 1
            || matching[0].status != ApprovalStatus::Approved
            || matching[0].work_item_id.as_deref() != Some(work_item_id)
            || !is_work_item_boundary_kind(matching[0].kind)
            || matching[0].scope_digest.trim().is_empty()
            || matching[0].decided_by_id.as_deref() != Some(mission.owner_id.as_str())
        {
            return Err(MissionError::WorkItemApprovalRequired);
        }
    }
    if next == WorkItemStatus::Completed {
        validate_evidence_ids(mission, work_item_id, evidence_ids, now_ms, authority)?;
    }
    let item = &mut mission.work_items[index];
    item.status = next;
    item.pending_approval_id = if next == WorkItemStatus::NeedsMe {
        approval_id.map(ToOwned::to_owned)
    } else {
        None
    };
    if next == WorkItemStatus::Completed {
        item.evidence_ids = evidence_ids.to_vec();
    }
    mission.updated_at_ms = now_ms;
    Ok(())
}

/// Issues a Receipt only after all Mission completion invariants still verify.
///
/// # Errors
///
/// Returns an error when the Mission is not completed, its proof no longer
/// verifies, or the actual model id is empty.
pub fn issue_receipt(
    mission: &Mission,
    receipt_id: impl Into<String>,
    summary: impl Into<String>,
    actual_model: impl Into<String>,
    output_hashes: Vec<String>,
    completed_at_ms: i64,
    authority: &LocalAuthority,
) -> Result<Receipt, MissionError> {
    if mission.status != MissionStatus::Completed {
        return Err(MissionError::MissionNotCompleted);
    }
    validate_completion(mission, authority)?;
    let actual_model = actual_model.into();
    if actual_model.trim().is_empty() {
        return Err(MissionError::MissingActualModel);
    }
    let evidence_ids = mission
        .work_items
        .iter()
        .flat_map(|item| item.evidence_ids.iter().cloned())
        .collect();
    let receipt = Receipt {
        id: receipt_id.into(),
        mission_id: mission.id.clone(),
        summary: summary.into(),
        actual_model,
        evidence_ids,
        output_hashes,
        completed_at_ms,
    };
    validate_receipt(mission, &receipt, authority)?;
    Ok(receipt)
}

pub(crate) fn validate_mission_snapshot(
    mission: &Mission,
    authority: &LocalAuthority,
) -> Result<(), MissionError> {
    validate_snapshot_identity_and_uniqueness(mission)?;
    validate_snapshot_approvals(mission)?;
    validate_snapshot_evidence(mission, authority)?;
    validate_snapshot_work_items(mission, authority)?;
    validate_snapshot_status(mission, authority)
}

fn validate_snapshot_identity_and_uniqueness(mission: &Mission) -> Result<(), MissionError> {
    if !is_canonical_mission_id(&mission.id) {
        return Err(MissionError::InvalidMissionId);
    }
    if mission.title.trim().is_empty()
        || mission.outcome.trim().is_empty()
        || mission.owner_id.trim().is_empty()
        || mission.scope_digest.trim().is_empty()
        || mission.created_at_ms > mission.updated_at_ms
    {
        return Err(MissionError::InvalidMissionSnapshot(
            "identity, outcome, owner, scope, and monotonic timestamps are required",
        ));
    }
    if !matches!(
        mission.status,
        MissionStatus::Proposed | MissionStatus::Cancelled
    ) && mission.work_items.is_empty()
    {
        return Err(MissionError::MissingWorkItems);
    }
    if !all_unique(mission.work_items.iter().map(|item| item.id.as_str())) {
        return Err(MissionError::InvalidMissionSnapshot(
            "WorkItem ids must be unique",
        ));
    }
    if !all_unique(
        mission
            .approvals
            .iter()
            .map(|approval| approval.id.as_str()),
    ) {
        return Err(MissionError::DuplicateApprovalId);
    }
    if !all_unique(mission.evidence.iter().map(|evidence| evidence.id.as_str())) {
        return Err(MissionError::EvidenceConflict);
    }
    Ok(())
}

fn validate_snapshot_approvals(mission: &Mission) -> Result<(), MissionError> {
    for approval in &mission.approvals {
        let decision_is_valid = match approval.status {
            ApprovalStatus::Pending => {
                approval.decided_by_id.is_none() && approval.decided_at_ms.is_none()
            }
            ApprovalStatus::Approved | ApprovalStatus::Rejected => {
                approval.decided_by_id.as_deref() == Some(mission.owner_id.as_str())
                    && approval.decided_at_ms.is_some()
            }
        };
        if approval.id.trim().is_empty()
            || approval.prompt.trim().is_empty()
            || approval.scope_digest.trim().is_empty()
            || approval.requested_by_id != mission.owner_id
            || !decision_is_valid
            || approval.work_item_id.as_ref().is_some_and(|work_item_id| {
                !mission
                    .work_items
                    .iter()
                    .any(|item| item.id == *work_item_id)
            })
            || (approval.work_item_id.is_some() && !is_work_item_boundary_kind(approval.kind))
        {
            return Err(MissionError::InvalidApproval);
        }
    }
    Ok(())
}

fn validate_snapshot_evidence(
    mission: &Mission,
    authority: &LocalAuthority,
) -> Result<(), MissionError> {
    for evidence in &mission.evidence {
        authority
            .verify_evidence(evidence)
            .map_err(|_| MissionError::InvalidEvidence)?;
        if evidence.mission_id != mission.id
            || !mission
                .work_items
                .iter()
                .any(|item| item.id == evidence.work_item_id)
        {
            return Err(MissionError::EvidenceScopeMismatch);
        }
        if evidence.observed_at_ms > mission.updated_at_ms {
            return Err(MissionError::EvidenceTimeMismatch);
        }
    }
    Ok(())
}

fn validate_snapshot_work_items(
    mission: &Mission,
    authority: &LocalAuthority,
) -> Result<(), MissionError> {
    for item in &mission.work_items {
        if item.id.trim().is_empty() || item.title.trim().is_empty() {
            return Err(MissionError::InvalidMissionSnapshot(
                "WorkItem identity and title are required",
            ));
        }
        match item.status {
            WorkItemStatus::NeedsMe => {
                let approval_id = item
                    .pending_approval_id
                    .as_deref()
                    .ok_or(MissionError::WorkItemApprovalRequired)?;
                validate_work_item_approval(mission, &item.id, approval_id, false)?;
            }
            _ if item.pending_approval_id.is_some() => {
                return Err(MissionError::InvalidMissionSnapshot(
                    "only a NeedsMe WorkItem may retain a pending approval",
                ));
            }
            _ => {}
        }
        if item.status == WorkItemStatus::Completed {
            validate_evidence_ids(
                mission,
                &item.id,
                &item.evidence_ids,
                mission.updated_at_ms,
                authority,
            )?;
        }
    }
    Ok(())
}

fn validate_snapshot_status(
    mission: &Mission,
    authority: &LocalAuthority,
) -> Result<(), MissionError> {
    if matches!(
        mission.status,
        MissionStatus::Proposed | MissionStatus::AwaitingConfirmation
    ) && mission
        .work_items
        .iter()
        .any(|item| item.status != WorkItemStatus::Pending)
    {
        return Err(MissionError::InvalidMissionSnapshot(
            "unconfirmed Mission contains a started WorkItem",
        ));
    }
    if matches!(
        mission.status,
        MissionStatus::Active
            | MissionStatus::NeedsMe
            | MissionStatus::Paused
            | MissionStatus::Completed
    ) && !has_owner_approval(mission, ApprovalKind::MissionScope, &mission.scope_digest)
    {
        return Err(MissionError::MissingScopeApproval);
    }

    match mission.status {
        MissionStatus::NeedsMe => validate_mission_need_you(mission, false)?,
        MissionStatus::Paused if mission.needs_me.is_some() => {
            validate_mission_need_you(mission, false)?;
        }
        MissionStatus::Proposed
        | MissionStatus::AwaitingConfirmation
        | MissionStatus::Active
        | MissionStatus::Completed
        | MissionStatus::Failed
        | MissionStatus::Cancelled
            if mission.needs_me.is_some() =>
        {
            return Err(MissionError::InvalidMissionSnapshot(
                "Need you context is only valid while needsMe or paused",
            ));
        }
        _ => {}
    }

    if matches!(
        mission.status,
        MissionStatus::Failed | MissionStatus::Cancelled
    ) && mission.work_items.iter().any(|item| {
        !matches!(
            item.status,
            WorkItemStatus::Completed | WorkItemStatus::Failed | WorkItemStatus::Cancelled
        )
    }) {
        return Err(MissionError::InvalidMissionSnapshot(
            "terminal Mission contains a runnable WorkItem",
        ));
    }
    if mission.status == MissionStatus::Completed {
        validate_completion(mission, authority)?;
    }
    Ok(())
}

pub(crate) fn validate_receipt(
    mission: &Mission,
    receipt: &Receipt,
    authority: &LocalAuthority,
) -> Result<(), MissionError> {
    if mission.status != MissionStatus::Completed {
        return Err(MissionError::MissionNotCompleted);
    }
    validate_completion(mission, authority)?;
    if receipt.id.trim().is_empty()
        || receipt.summary.trim().is_empty()
        || receipt.mission_id != mission.id
        || receipt.actual_model.trim().is_empty()
        || receipt.completed_at_ms < mission.updated_at_ms
        || mission
            .evidence
            .iter()
            .any(|evidence| evidence.observed_at_ms > receipt.completed_at_ms)
        || receipt.evidence_ids
            != mission
                .work_items
                .iter()
                .flat_map(|item| item.evidence_ids.iter().cloned())
                .collect::<Vec<_>>()
    {
        return Err(MissionError::ReceiptMismatch);
    }
    Ok(())
}

fn validate_completion(mission: &Mission, authority: &LocalAuthority) -> Result<(), MissionError> {
    if mission.work_items.is_empty() {
        return Err(MissionError::MissingWorkItems);
    }
    if mission
        .work_items
        .iter()
        .any(|item| item.status != WorkItemStatus::Completed)
    {
        return Err(MissionError::IncompleteWorkItems);
    }
    for item in &mission.work_items {
        validate_evidence_ids(
            mission,
            &item.id,
            &item.evidence_ids,
            mission.updated_at_ms,
            authority,
        )?;
    }
    Ok(())
}

fn validate_evidence_ids(
    mission: &Mission,
    work_item_id: &str,
    evidence_ids: &[String],
    not_after_ms: i64,
    authority: &LocalAuthority,
) -> Result<(), MissionError> {
    if evidence_ids.is_empty() {
        return Err(MissionError::MissingOutcomeEvidence);
    }
    let mut has_outcome_evidence = false;
    for evidence_id in evidence_ids {
        let evidence = mission
            .evidence
            .iter()
            .find(|evidence| &evidence.id == evidence_id)
            .ok_or(MissionError::MissingOutcomeEvidence)?;
        if evidence.mission_id != mission.id || evidence.work_item_id != work_item_id {
            return Err(MissionError::EvidenceScopeMismatch);
        }
        authority
            .verify_evidence(evidence)
            .map_err(|_| MissionError::InvalidEvidence)?;
        if evidence.observed_at_ms > not_after_ms {
            return Err(MissionError::EvidenceTimeMismatch);
        }
        has_outcome_evidence |= is_outcome_evidence(evidence.kind);
    }
    if !has_outcome_evidence {
        return Err(MissionError::MissingOutcomeEvidence);
    }
    Ok(())
}

const fn is_outcome_evidence(kind: EvidenceKind) -> bool {
    matches!(
        kind,
        EvidenceKind::ReminderCompleted
            | EvidenceKind::AvailabilityDecisionPublished
            | EvidenceKind::XlsxVerified
    )
}

fn has_owner_approval(mission: &Mission, kind: ApprovalKind, digest: &str) -> bool {
    mission.approvals.iter().any(|approval| {
        approval.kind == kind
            && approval.work_item_id.is_none()
            && approval.status == ApprovalStatus::Approved
            && approval.scope_digest == digest
            && approval.decided_by_id.as_deref() == Some(mission.owner_id.as_str())
    })
}

const fn can_transition(from: MissionStatus, to: MissionStatus) -> bool {
    use MissionStatus::{
        Active, AwaitingConfirmation, Cancelled, Completed, Failed, NeedsMe, Paused, Proposed,
    };
    matches!(
        (from, to),
        (Proposed, AwaitingConfirmation | Cancelled)
            | (AwaitingConfirmation, Active | Cancelled)
            | (Active, Paused | Completed | Failed | Cancelled)
            | (NeedsMe, Active | Paused | Failed | Cancelled)
            | (Paused, Active | Failed | Cancelled)
    )
}

const fn can_transition_work_item(from: WorkItemStatus, to: WorkItemStatus) -> bool {
    use WorkItemStatus::{Active, Cancelled, Completed, Failed, NeedsMe, Pending};
    matches!(
        (from, to),
        (Pending, Active | Cancelled)
            | (Active, NeedsMe | Completed | Failed | Cancelled)
            | (NeedsMe, Active | Failed | Cancelled)
    )
}

fn validate_mission_need_you(
    mission: &Mission,
    require_approved: bool,
) -> Result<(), MissionError> {
    let approval_id = mission
        .needs_me
        .as_ref()
        .and_then(|needs_me| needs_me.approval_id.as_deref())
        .ok_or(MissionError::PendingApproval)?;
    let matching: Vec<_> = mission
        .approvals
        .iter()
        .filter(|approval| approval.id == approval_id)
        .collect();
    let status_is_valid = if require_approved {
        matching
            .first()
            .is_some_and(|approval| approval.status == ApprovalStatus::Approved)
    } else {
        matching.first().is_some_and(|approval| {
            matches!(
                approval.status,
                ApprovalStatus::Pending | ApprovalStatus::Approved
            )
        })
    };
    if matching.len() != 1
        || !status_is_valid
        || matching[0].work_item_id.is_some()
        || matching[0].requested_by_id != mission.owner_id
        || (matching[0].status == ApprovalStatus::Approved
            && matching[0].decided_by_id.as_deref() != Some(mission.owner_id.as_str()))
    {
        return Err(MissionError::PendingApproval);
    }
    Ok(())
}

fn validate_work_item_approval(
    mission: &Mission,
    work_item_id: &str,
    approval_id: &str,
    require_approved: bool,
) -> Result<(), MissionError> {
    let matching: Vec<_> = mission
        .approvals
        .iter()
        .filter(|approval| approval.id == approval_id)
        .collect();
    let status_is_valid = if require_approved {
        matching
            .first()
            .is_some_and(|approval| approval.status == ApprovalStatus::Approved)
    } else {
        matching.first().is_some_and(|approval| {
            matches!(
                approval.status,
                ApprovalStatus::Pending | ApprovalStatus::Approved
            )
        })
    };
    if matching.len() != 1
        || !status_is_valid
        || matching[0].work_item_id.as_deref() != Some(work_item_id)
        || !is_work_item_boundary_kind(matching[0].kind)
        || matching[0].scope_digest.trim().is_empty()
        || matching[0].requested_by_id != mission.owner_id
        || (matching[0].status == ApprovalStatus::Approved
            && matching[0].decided_by_id.as_deref() != Some(mission.owner_id.as_str()))
    {
        return Err(MissionError::WorkItemApprovalRequired);
    }
    Ok(())
}

fn approval_is_decidable(mission: &Mission, approval: &ApprovalRequest) -> bool {
    if let Some(work_item_id) = approval.work_item_id.as_deref() {
        return mission.status == MissionStatus::Active
            && mission.work_items.iter().any(|item| {
                item.id == work_item_id
                    && item.status == WorkItemStatus::NeedsMe
                    && item.pending_approval_id.as_deref() == Some(approval.id.as_str())
            });
    }
    if approval.kind == ApprovalKind::MissionScope {
        return mission.status == MissionStatus::AwaitingConfirmation
            && approval.scope_digest == mission.scope_digest;
    }
    matches!(
        mission.status,
        MissionStatus::NeedsMe | MissionStatus::Paused
    ) && mission
        .needs_me
        .as_ref()
        .and_then(|needs_me| needs_me.approval_id.as_deref())
        == Some(approval.id.as_str())
}

fn all_unique<'a>(values: impl Iterator<Item = &'a str>) -> bool {
    let mut seen = HashSet::new();
    values.into_iter().all(|value| seen.insert(value))
}

fn ensure_monotonic_time(mission: &Mission, now_ms: i64) -> Result<(), MissionError> {
    if now_ms < mission.updated_at_ms {
        Err(MissionError::StaleCommandTime)
    } else {
        Ok(())
    }
}

pub(crate) fn is_canonical_mission_id(value: &str) -> bool {
    let bytes = value.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= MAX_MISSION_ID_BYTES
        && bytes.first().is_some_and(u8::is_ascii_alphanumeric)
        && bytes.last().is_some_and(u8::is_ascii_alphanumeric)
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
}

const fn is_work_item_boundary_kind(kind: ApprovalKind) -> bool {
    matches!(
        kind,
        ApprovalKind::ExpandedScope
            | ApprovalKind::NewRecipient
            | ApprovalKind::NewDataShare
            | ApprovalKind::NewExternalWrite
            | ApprovalKind::Cost
            | ApprovalKind::DeleteOrIrreversible
    )
}

impl From<CryptoError> for MissionError {
    fn from(_: CryptoError) -> Self {
        Self::InvalidEvidence
    }
}
