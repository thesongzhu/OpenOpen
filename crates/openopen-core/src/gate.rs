use crate::mission::is_canonical_mission_id;
use openopen_protocol::{ApprovalKind, ApprovalStatus, ChannelKind, Mission, MissionStatus};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::path::Path;

const MAX_EFFECT_PATH_COMPONENTS: usize = 16;
const MAX_EFFECT_PATH_COMPONENT_BYTES: usize = 128;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum EffectKind {
    ModelCall,
    ChannelListen,
    WorkflowTrigger,
    ChannelSend,
    ReminderWrite,
    FileWrite,
    FileDelete,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum ActionTarget {
    Model {
        model_id: String,
    },
    Channel {
        channel: ChannelKind,
        conversation_id: String,
        recipient_ids: Vec<String>,
    },
    Workflow {
        workflow_id: String,
    },
    ReminderList {
        list_id: String,
    },
    MissionFile {
        relative_path: String,
    },
    ApprovedExport {
        absolute_path: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionProposal {
    pub effect: EffectKind,
    pub mission_id: String,
    pub mission_scope_digest: String,
    pub target: ActionTarget,
    pub estimated_cost_micros: Option<u64>,
}

impl ActionProposal {
    /// Computes a boundary-specific approval digest in Rust. Content-bearing
    /// boundaries include a SHA-256 derived from the actual bytes supplied to
    /// the gate; callers cannot omit or substitute a disclosure claim.
    ///
    /// # Errors
    ///
    /// Returns a serialization error instead of silently authorizing an action
    /// if the canonical proposal cannot be encoded.
    pub fn approval_digest(
        &self,
        kind: ApprovalKind,
        payload: Option<&[u8]>,
    ) -> Result<String, serde_json::Error> {
        let payload_sha256 = payload.map(|bytes| format!("{:x}", Sha256::digest(bytes)));
        let canonical = match kind {
            ApprovalKind::MissionScope | ApprovalKind::ExpandedScope => serde_json::json!({
                "effect": self.effect,
                "missionId": self.mission_id,
                "missionScopeDigest": self.mission_scope_digest,
                "target": self.target,
                "version": 1,
            }),
            ApprovalKind::NewRecipient => serde_json::json!({
                "effect": self.effect,
                "missionId": self.mission_id,
                "target": self.target,
                "version": 1,
            }),
            ApprovalKind::NewDataShare | ApprovalKind::NewExternalWrite => serde_json::json!({
                "effect": self.effect,
                "missionId": self.mission_id,
                "payloadSha256": payload_sha256,
                "target": self.target,
                "version": 1,
            }),
            ApprovalKind::Cost => serde_json::json!({
                "effect": self.effect,
                "estimatedCostMicros": self.estimated_cost_micros,
                "missionId": self.mission_id,
                "target": self.target,
                "version": 1,
            }),
            ApprovalKind::DeleteOrIrreversible
            | ApprovalKind::FinalDecision
            | ApprovalKind::WorkflowEnable
            | ApprovalKind::SkillPromotion => serde_json::json!({
                "approvalKind": kind,
                "effect": self.effect,
                "missionId": self.mission_id,
                "target": self.target,
                "version": 1,
            }),
        };
        let canonical = serde_json::to_vec(&canonical)?;
        Ok(format!("{:x}", Sha256::digest(canonical)))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GateDecision {
    Allowed,
    Denied(&'static str),
    NeedsMe(ApprovalKind),
}

/// A pure policy gate. It deliberately owns neither runtime enablement nor a
/// Mission root and exposes no executor. The `Store` is the only production
/// authority for the persistent global switch and effect-permit issuance.
#[derive(Clone, Copy, Debug, Default)]
pub struct ActionGate;

impl ActionGate {
    #[must_use]
    pub fn authorize(
        &self,
        mission: &Mission,
        proposal: &ActionProposal,
        payload: Option<&[u8]>,
    ) -> GateDecision {
        if proposal.mission_id != mission.id {
            return GateDecision::Denied("proposal mission mismatch");
        }
        if !is_canonical_mission_id(&mission.id) {
            return GateDecision::Denied("invalid canonical Mission id");
        }
        if mission.status != MissionStatus::Active {
            return GateDecision::Denied("mission is not active");
        }
        if proposal.mission_scope_digest != mission.scope_digest {
            return GateDecision::NeedsMe(ApprovalKind::ExpandedScope);
        }
        if !target_matches_effect(proposal.effect, &proposal.target) {
            return GateDecision::Denied("proposal target does not match effect");
        }
        if !file_target_is_safe(&proposal.target) {
            return GateDecision::Denied("file target escapes Mission workspace");
        }
        if effect_requires_payload(proposal.effect) && payload.is_none() {
            return GateDecision::Denied("content-bearing action is missing Rust-observed payload");
        }

        if proposal.estimated_cost_micros.unwrap_or_default() > 0 {
            let Ok(digest) = proposal.approval_digest(ApprovalKind::Cost, payload) else {
                return GateDecision::Denied("proposal canonicalization failed");
            };
            if approved_owner_approval_id(mission, ApprovalKind::Cost, &digest).is_none() {
                return GateDecision::NeedsMe(ApprovalKind::Cost);
            }
        }

        let required = match proposal.effect {
            EffectKind::ChannelSend => Some(ApprovalKind::NewRecipient),
            EffectKind::ReminderWrite | EffectKind::FileWrite => {
                Some(ApprovalKind::NewExternalWrite)
            }
            EffectKind::FileDelete => Some(ApprovalKind::DeleteOrIrreversible),
            EffectKind::ModelCall | EffectKind::ChannelListen | EffectKind::WorkflowTrigger => {
                let Ok(digest) = proposal.approval_digest(ApprovalKind::ExpandedScope, payload)
                else {
                    return GateDecision::Denied("proposal canonicalization failed");
                };
                if has_action_scope_approval(mission, &digest) {
                    None
                } else {
                    return GateDecision::NeedsMe(ApprovalKind::ExpandedScope);
                }
            }
        };
        if let Some(kind) = required {
            let Ok(digest) = proposal.approval_digest(kind, payload) else {
                return GateDecision::Denied("proposal canonicalization failed");
            };
            if approved_owner_approval_id(mission, kind, &digest).is_none() {
                return GateDecision::NeedsMe(kind);
            }
        }

        if matches!(
            proposal.effect,
            EffectKind::ModelCall | EffectKind::ChannelSend
        ) {
            let Ok(digest) = proposal.approval_digest(ApprovalKind::NewDataShare, payload) else {
                return GateDecision::Denied("proposal canonicalization failed");
            };
            if approved_owner_approval_id(mission, ApprovalKind::NewDataShare, &digest).is_none() {
                return GateDecision::NeedsMe(ApprovalKind::NewDataShare);
            }
        }

        GateDecision::Allowed
    }
}

pub(crate) fn approved_owner_approval_id(
    mission: &Mission,
    kind: ApprovalKind,
    digest: &str,
) -> Option<String> {
    mission.approvals.iter().find_map(|approval| {
        (approval.kind == kind
            && approval.work_item_id.is_none()
            && approval.status == ApprovalStatus::Approved
            && approval.scope_digest == digest
            && approval.decided_by_id.as_deref() == Some(mission.owner_id.as_str()))
        .then(|| approval.id.clone())
    })
}

const fn target_matches_effect(effect: EffectKind, target: &ActionTarget) -> bool {
    matches!(
        (effect, target),
        (EffectKind::ModelCall, ActionTarget::Model { .. })
            | (
                EffectKind::ChannelListen | EffectKind::ChannelSend,
                ActionTarget::Channel { .. }
            )
            | (EffectKind::WorkflowTrigger, ActionTarget::Workflow { .. })
            | (EffectKind::ReminderWrite, ActionTarget::ReminderList { .. })
            | (
                EffectKind::FileWrite | EffectKind::FileDelete,
                ActionTarget::MissionFile { .. } | ActionTarget::ApprovedExport { .. }
            )
    )
}

pub(crate) fn mission_file_path_components(value: &str) -> Option<Vec<String>> {
    if value.is_empty() || value.as_bytes().contains(&0) || Path::new(value).is_absolute() {
        return None;
    }
    let components = value.split('/').collect::<Vec<_>>();
    if components.is_empty() || components.len() > MAX_EFFECT_PATH_COMPONENTS {
        return None;
    }
    components
        .iter()
        .all(|component| is_canonical_effect_path_component(component))
        .then(|| components.into_iter().map(ToOwned::to_owned).collect())
}

fn is_canonical_effect_path_component(value: &str) -> bool {
    !value.is_empty()
        && value != "."
        && value != ".."
        && value.len() <= MAX_EFFECT_PATH_COMPONENT_BYTES
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"._-".contains(&byte)
        })
}

fn has_action_scope_approval(mission: &Mission, digest: &str) -> bool {
    approved_owner_approval_id(mission, ApprovalKind::MissionScope, digest).is_some()
        || approved_owner_approval_id(mission, ApprovalKind::ExpandedScope, digest).is_some()
}

fn file_target_is_safe(target: &ActionTarget) -> bool {
    match target {
        ActionTarget::MissionFile { relative_path } => {
            mission_file_path_components(relative_path).is_some()
        }
        ActionTarget::ApprovedExport { absolute_path } => Path::new(absolute_path).is_absolute(),
        ActionTarget::Model { .. }
        | ActionTarget::Channel { .. }
        | ActionTarget::Workflow { .. }
        | ActionTarget::ReminderList { .. } => true,
    }
}

const fn effect_requires_payload(effect: EffectKind) -> bool {
    matches!(
        effect,
        EffectKind::ModelCall
            | EffectKind::ChannelSend
            | EffectKind::ReminderWrite
            | EffectKind::FileWrite
    )
}
