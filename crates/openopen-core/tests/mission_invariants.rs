use openopen_core::{
    ActionGate, ActionProposal, ActionTarget, EffectKind, EvidenceClaims, GateDecision,
    LocalAuthority, MissionError, add_evidence, approve_request, issue_receipt,
    request_scope_change, transition_mission, transition_work_item,
};
use openopen_protocol::{
    ApprovalKind, ApprovalRequest, ApprovalStatus, ChannelKind, EvidenceKind, Mission,
    MissionStatus, NeedsMe, WorkItem, WorkItemStatus,
};

fn authority() -> LocalAuthority {
    LocalAuthority::from_master("openopen-core", [7_u8; 32])
}

fn approval_with_id(
    id: &str,
    kind: ApprovalKind,
    status: ApprovalStatus,
    digest: &str,
) -> ApprovalRequest {
    ApprovalRequest {
        id: id.into(),
        work_item_id: None,
        kind,
        prompt: "Approve this bounded action?".into(),
        scope_digest: digest.into(),
        status,
        requested_by_id: "owner-1".into(),
        decided_by_id: (status != ApprovalStatus::Pending).then(|| "owner-1".into()),
        requested_at_ms: 2,
        decided_at_ms: (status != ApprovalStatus::Pending).then_some(3),
    }
}

fn work_approval_with_id(id: &str, status: ApprovalStatus, digest: &str) -> ApprovalRequest {
    let mut approval = approval_with_id(id, ApprovalKind::ExpandedScope, status, digest);
    approval.work_item_id = Some("work-1".into());
    approval
}

fn approval(kind: ApprovalKind, status: ApprovalStatus, digest: &str) -> ApprovalRequest {
    approval_with_id(&format!("approval-{kind:?}-{digest}"), kind, status, digest)
}

fn mission(status: MissionStatus) -> Mission {
    Mission {
        id: "mission-1".into(),
        title: "Prepare expenses".into(),
        outcome: "A verified workbook".into(),
        owner_id: "owner-1".into(),
        scope_digest: "scope-v1".into(),
        status,
        work_items: vec![WorkItem {
            id: "work-1".into(),
            title: "Build workbook".into(),
            status: if matches!(
                status,
                MissionStatus::Proposed | MissionStatus::AwaitingConfirmation
            ) {
                WorkItemStatus::Pending
            } else {
                WorkItemStatus::Active
            },
            evidence_ids: Vec::new(),
            pending_approval_id: None,
        }],
        approvals: vec![approval(
            ApprovalKind::MissionScope,
            ApprovalStatus::Approved,
            "scope-v1",
        )],
        needs_me: None,
        evidence: Vec::new(),
        created_at_ms: 1,
        updated_at_ms: 1,
    }
}

fn add_outcome_evidence(value: &mut Mission, authority: &LocalAuthority) {
    let evidence = authority.sign_evidence(EvidenceClaims {
        id: "xlsx-1".into(),
        mission_id: value.id.clone(),
        work_item_id: "work-1".into(),
        kind: EvidenceKind::XlsxVerified,
        source_id: "workbook-1".into(),
        sha256: Some("abc".into()),
        observed_at_ms: 2,
    });
    add_evidence(value, evidence, 2, authority).unwrap();
}

fn mission_transition_allowed(from: MissionStatus, to: MissionStatus) -> bool {
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

#[test]
fn every_mission_transition_edge_is_enforced() {
    let authority = authority();
    let states = [
        MissionStatus::Proposed,
        MissionStatus::AwaitingConfirmation,
        MissionStatus::Active,
        MissionStatus::NeedsMe,
        MissionStatus::Paused,
        MissionStatus::Completed,
        MissionStatus::Failed,
        MissionStatus::Cancelled,
    ];
    for from in states {
        for to in states {
            let mut value = mission(from);
            if from == MissionStatus::NeedsMe {
                value.approvals.push(approval_with_id(
                    "needs-approval",
                    ApprovalKind::ExpandedScope,
                    ApprovalStatus::Approved,
                    "scope-change",
                ));
                value.needs_me = Some(NeedsMe {
                    id: "needs-1".into(),
                    prompt: "Approve?".into(),
                    approval_id: Some("needs-approval".into()),
                    created_at_ms: 2,
                });
            }
            if to == MissionStatus::Completed {
                add_outcome_evidence(&mut value, &authority);
                value.work_items[0].status = WorkItemStatus::Completed;
                value.work_items[0].evidence_ids = vec!["xlsx-1".into()];
            }
            let result = transition_mission(&mut value, to, 3, &authority);
            assert_eq!(
                result.is_ok(),
                mission_transition_allowed(from, to),
                "unexpected transition result for {from:?} -> {to:?}: {result:?}"
            );
        }
    }
}

fn work_transition_allowed(from: WorkItemStatus, to: WorkItemStatus) -> bool {
    use WorkItemStatus::{Active, Cancelled, Completed, Failed, NeedsMe, Pending};
    matches!(
        (from, to),
        (Pending, Active | Cancelled)
            | (Active, NeedsMe | Completed | Failed | Cancelled)
            | (NeedsMe, Active | Failed | Cancelled)
    )
}

#[test]
fn every_work_item_transition_edge_is_enforced() {
    let authority = authority();
    let states = [
        WorkItemStatus::Pending,
        WorkItemStatus::Active,
        WorkItemStatus::NeedsMe,
        WorkItemStatus::Completed,
        WorkItemStatus::Failed,
        WorkItemStatus::Cancelled,
    ];
    for from in states {
        for to in states {
            let mut value = mission(MissionStatus::Active);
            value.work_items[0].status = from;
            let mut approval_id = None;
            if from == WorkItemStatus::Active && to == WorkItemStatus::NeedsMe {
                value.approvals.push(work_approval_with_id(
                    "work-approval",
                    ApprovalStatus::Pending,
                    "work-change",
                ));
                approval_id = Some("work-approval");
            }
            if from == WorkItemStatus::NeedsMe {
                value.approvals.push(work_approval_with_id(
                    "work-approval",
                    ApprovalStatus::Approved,
                    "work-change",
                ));
                value.work_items[0].pending_approval_id = Some("work-approval".into());
            }
            let evidence_ids = if to == WorkItemStatus::Completed {
                add_outcome_evidence(&mut value, &authority);
                vec!["xlsx-1".into()]
            } else {
                Vec::new()
            };
            let result = transition_work_item(
                &mut value,
                "work-1",
                to,
                &evidence_ids,
                approval_id,
                3,
                &authority,
            );
            assert_eq!(
                result.is_ok(),
                work_transition_allowed(from, to),
                "unexpected WorkItem transition for {from:?} -> {to:?}: {result:?}"
            );
        }
    }
}

#[test]
fn work_items_cannot_advance_outside_an_active_mission() {
    let authority = authority();
    for parent in [
        MissionStatus::Proposed,
        MissionStatus::AwaitingConfirmation,
        MissionStatus::NeedsMe,
        MissionStatus::Paused,
        MissionStatus::Completed,
        MissionStatus::Failed,
        MissionStatus::Cancelled,
    ] {
        let mut value = mission(parent);
        value.work_items[0].status = WorkItemStatus::Pending;
        assert_eq!(
            transition_work_item(
                &mut value,
                "work-1",
                WorkItemStatus::Active,
                &[],
                None,
                3,
                &authority,
            ),
            Err(MissionError::ParentMissionNotActive),
            "parent {parent:?} must not execute work"
        );
    }
}

#[test]
fn work_item_need_you_rejects_duplicate_or_misbound_approval() {
    let authority = authority();
    let mut duplicate = mission(MissionStatus::Active);
    duplicate.approvals.push(work_approval_with_id(
        "work-approval",
        ApprovalStatus::Approved,
        "old",
    ));
    duplicate.approvals.push(work_approval_with_id(
        "work-approval",
        ApprovalStatus::Pending,
        "new",
    ));
    assert_eq!(
        transition_work_item(
            &mut duplicate,
            "work-1",
            WorkItemStatus::NeedsMe,
            &[],
            Some("work-approval"),
            3,
            &authority,
        ),
        Err(MissionError::WorkItemApprovalRequired)
    );

    let mut misbound = mission(MissionStatus::Active);
    misbound.approvals.push(approval_with_id(
        "mission-approval",
        ApprovalKind::ExpandedScope,
        ApprovalStatus::Pending,
        "work-change",
    ));
    assert_eq!(
        transition_work_item(
            &mut misbound,
            "work-1",
            WorkItemStatus::NeedsMe,
            &[],
            Some("mission-approval"),
            3,
            &authority,
        ),
        Err(MissionError::WorkItemApprovalRequired)
    );

    let mut wrong_kind = mission(MissionStatus::Active);
    let mut approval = approval_with_id(
        "wrong-kind",
        ApprovalKind::MissionScope,
        ApprovalStatus::Pending,
        "work-change",
    );
    approval.work_item_id = Some("work-1".into());
    wrong_kind.approvals.push(approval);
    assert_eq!(
        transition_work_item(
            &mut wrong_kind,
            "work-1",
            WorkItemStatus::NeedsMe,
            &[],
            Some("wrong-kind"),
            3,
            &authority,
        ),
        Err(MissionError::WorkItemApprovalRequired)
    );
}

#[test]
fn need_you_cannot_resume_until_owner_approves_exact_unique_request() {
    let authority = authority();
    let mut value = mission(MissionStatus::Active);
    let request = approval_with_id(
        "new-recipient",
        ApprovalKind::NewRecipient,
        ApprovalStatus::Pending,
        "recipient-action",
    );
    request_scope_change(
        &mut value,
        request.clone(),
        NeedsMe {
            id: "needs-1".into(),
            prompt: "Add this person?".into(),
            approval_id: Some(request.id.clone()),
            created_at_ms: 2,
        },
        2,
    )
    .unwrap();
    assert_eq!(
        transition_mission(&mut value, MissionStatus::Active, 3, &authority),
        Err(MissionError::PendingApproval)
    );
    assert_eq!(
        approve_request(&mut value, &request.id, "participant-2", 3),
        Err(MissionError::NotMissionOwner)
    );
    assert_eq!(
        approve_request(&mut value, "missing", "owner-1", 3),
        Err(MissionError::ApprovalNotPending)
    );
    approve_request(&mut value, &request.id, "owner-1", 4).unwrap();
    transition_mission(&mut value, MissionStatus::Active, 5, &authority).unwrap();

    let mut reused = mission(MissionStatus::Active);
    reused.approvals.push(approval_with_id(
        "reused-id",
        ApprovalKind::ExpandedScope,
        ApprovalStatus::Approved,
        "old",
    ));
    assert_eq!(
        request_scope_change(
            &mut reused,
            approval_with_id(
                "reused-id",
                ApprovalKind::NewRecipient,
                ApprovalStatus::Pending,
                "new"
            ),
            NeedsMe {
                id: "needs-2".into(),
                prompt: "New request".into(),
                approval_id: Some("reused-id".into()),
                created_at_ms: 6,
            },
            6,
        ),
        Err(MissionError::DuplicateApprovalId)
    );
}

#[test]
fn scope_approval_cannot_be_decided_before_confirmation_state() {
    let mut value = mission(MissionStatus::Proposed);
    value.approvals[0] = approval(
        ApprovalKind::MissionScope,
        ApprovalStatus::Pending,
        "scope-v1",
    );
    let approval_id = value.approvals[0].id.clone();
    assert_eq!(
        approve_request(&mut value, &approval_id, "owner-1", 2),
        Err(MissionError::ApprovalDecisionNotAllowed)
    );
}

#[test]
fn paused_need_you_preserves_approval_boundary() {
    let authority = authority();
    let mut value = mission(MissionStatus::Active);
    let request = approval_with_id(
        "scope-change",
        ApprovalKind::ExpandedScope,
        ApprovalStatus::Pending,
        "expanded-scope",
    );
    request_scope_change(
        &mut value,
        request.clone(),
        NeedsMe {
            id: "needs-pause".into(),
            prompt: "Approve the expanded scope?".into(),
            approval_id: Some(request.id.clone()),
            created_at_ms: 2,
        },
        2,
    )
    .unwrap();
    transition_mission(&mut value, MissionStatus::Paused, 3, &authority).unwrap();
    assert!(value.needs_me.is_some());
    assert_eq!(
        transition_mission(&mut value, MissionStatus::Active, 4, &authority),
        Err(MissionError::PendingApproval)
    );
    approve_request(&mut value, &request.id, "owner-1", 5).unwrap();
    transition_mission(&mut value, MissionStatus::Active, 6, &authority).unwrap();
    assert!(value.needs_me.is_none());
}

#[test]
fn empty_mission_cannot_complete() {
    let authority = authority();
    let mut value = mission(MissionStatus::Active);
    value.work_items.clear();
    assert_eq!(
        transition_mission(&mut value, MissionStatus::Completed, 2, &authority),
        Err(MissionError::MissingWorkItems)
    );
}

#[test]
fn evidence_is_bound_to_exact_mission_and_work_item() {
    let authority = authority();
    let mut other = mission(MissionStatus::Active);
    other.id = "mission-2".into();
    let evidence = authority.sign_evidence(EvidenceClaims {
        id: "xlsx-1".into(),
        mission_id: "mission-1".into(),
        work_item_id: "work-1".into(),
        kind: EvidenceKind::XlsxVerified,
        source_id: "workbook-1".into(),
        sha256: Some("abc".into()),
        observed_at_ms: 2,
    });
    assert_eq!(
        add_evidence(&mut other, evidence, 2, &authority),
        Err(MissionError::EvidenceScopeMismatch)
    );

    let mut wrong_item = mission(MissionStatus::Active);
    let evidence = authority.sign_evidence(EvidenceClaims {
        id: "xlsx-2".into(),
        mission_id: "mission-1".into(),
        work_item_id: "work-2".into(),
        kind: EvidenceKind::XlsxVerified,
        source_id: "workbook-2".into(),
        sha256: Some("def".into()),
        observed_at_ms: 2,
    });
    assert_eq!(
        add_evidence(&mut wrong_item, evidence, 2, &authority),
        Err(MissionError::EvidenceScopeMismatch)
    );
}

#[test]
fn evidence_observation_must_precede_attachment_completion_and_receipt() {
    let authority = authority();
    let mut value = mission(MissionStatus::Active);
    let future = authority.sign_evidence(EvidenceClaims {
        id: "future-xlsx".into(),
        mission_id: "mission-1".into(),
        work_item_id: "work-1".into(),
        kind: EvidenceKind::XlsxVerified,
        source_id: "future-workbook".into(),
        sha256: Some("future-hash".into()),
        observed_at_ms: 10,
    });
    assert_eq!(
        add_evidence(&mut value, future, 2, &authority),
        Err(MissionError::EvidenceTimeMismatch)
    );
    assert!(value.evidence.is_empty());

    add_outcome_evidence(&mut value, &authority);
    assert_eq!(
        transition_work_item(
            &mut value,
            "work-1",
            WorkItemStatus::Completed,
            &["xlsx-1".into()],
            None,
            1,
            &authority,
        ),
        Err(MissionError::StaleCommandTime)
    );
    transition_work_item(
        &mut value,
        "work-1",
        WorkItemStatus::Completed,
        &["xlsx-1".into()],
        None,
        3,
        &authority,
    )
    .unwrap();
    transition_mission(&mut value, MissionStatus::Completed, 4, &authority).unwrap();
    assert_eq!(
        issue_receipt(
            &value,
            "receipt-early",
            "Workbook verified",
            "gpt-5.6-sol",
            vec!["abc".into()],
            3,
            &authority,
        ),
        Err(MissionError::ReceiptMismatch)
    );
}

#[test]
fn forged_changed_or_non_outcome_evidence_fails_closed() {
    let authority = authority();
    let mut value = mission(MissionStatus::Active);
    let mut evidence = authority.sign_evidence(EvidenceClaims {
        id: "xlsx-1".into(),
        mission_id: "mission-1".into(),
        work_item_id: "work-1".into(),
        kind: EvidenceKind::XlsxVerified,
        source_id: "workbook-1".into(),
        sha256: Some("abc".into()),
        observed_at_ms: 2,
    });
    evidence.source_id = "changed-after-signing".into();
    assert_eq!(
        add_evidence(&mut value, evidence, 2, &authority),
        Err(MissionError::InvalidEvidence)
    );

    let evidence = authority.sign_evidence(EvidenceClaims {
        id: "delivery-1".into(),
        mission_id: "mission-1".into(),
        work_item_id: "work-1".into(),
        kind: EvidenceKind::ChannelDelivery,
        source_id: "message-1".into(),
        sha256: None,
        observed_at_ms: 2,
    });
    add_evidence(&mut value, evidence, 2, &authority).unwrap();
    assert_eq!(
        transition_work_item(
            &mut value,
            "work-1",
            WorkItemStatus::Completed,
            &["delivery-1".into()],
            None,
            3,
            &authority,
        ),
        Err(MissionError::MissingOutcomeEvidence)
    );
}

#[test]
fn verified_completion_and_receipt_record_actual_model() {
    let authority = authority();
    let mut value = mission(MissionStatus::Active);
    add_outcome_evidence(&mut value, &authority);
    transition_work_item(
        &mut value,
        "work-1",
        WorkItemStatus::Completed,
        &["xlsx-1".into()],
        None,
        3,
        &authority,
    )
    .unwrap();
    transition_mission(&mut value, MissionStatus::Completed, 4, &authority).unwrap();
    assert_eq!(
        issue_receipt(
            &value,
            "receipt-1",
            "Workbook verified",
            "",
            vec!["abc".into()],
            4,
            &authority,
        ),
        Err(MissionError::MissingActualModel)
    );
    let receipt = issue_receipt(
        &value,
        "receipt-1",
        "Workbook verified",
        "gpt-5.6-sol",
        vec!["abc".into()],
        4,
        &authority,
    )
    .unwrap();
    assert_eq!(receipt.actual_model, "gpt-5.6-sol");
}

const SUMMARY_PAYLOAD: &[u8] = b"Meeting summary";

fn channel_proposal(recipients: &[&str]) -> ActionProposal {
    ActionProposal {
        effect: EffectKind::ChannelSend,
        mission_id: "mission-1".into(),
        mission_scope_digest: "scope-v1".into(),
        target: ActionTarget::Channel {
            channel: ChannelKind::Discord,
            conversation_id: "channel-1".into(),
            recipient_ids: recipients.iter().map(ToString::to_string).collect(),
        },
        estimated_cost_micros: None,
    }
}

#[test]
fn pure_gate_rejects_mission_mismatch() {
    let value = mission(MissionStatus::Active);
    let mut proposal = channel_proposal(&["recipient-1"]);
    proposal.mission_id = "mission-2".into();
    assert_eq!(
        ActionGate.authorize(&value, &proposal, Some(SUMMARY_PAYLOAD)),
        GateDecision::Denied("proposal mission mismatch")
    );
}

#[test]
fn content_bearing_action_cannot_omit_rust_observed_payload() {
    let value = mission(MissionStatus::Active);
    assert_eq!(
        ActionGate.authorize(&value, &channel_proposal(&["recipient-1"]), None),
        GateDecision::Denied("content-bearing action is missing Rust-observed payload")
    );
}

#[test]
fn recipient_and_disclosure_approvals_are_exact_and_independent() {
    let mut value = mission(MissionStatus::Active);
    let gate = ActionGate;
    let proposal = channel_proposal(&["recipient-1"]);
    let recipient_digest = proposal
        .approval_digest(ApprovalKind::NewRecipient, Some(SUMMARY_PAYLOAD))
        .unwrap();
    value.approvals.push(approval(
        ApprovalKind::NewRecipient,
        ApprovalStatus::Approved,
        &recipient_digest,
    ));
    assert_eq!(
        gate.authorize(&value, &proposal, Some(SUMMARY_PAYLOAD)),
        GateDecision::NeedsMe(ApprovalKind::NewDataShare)
    );
    let disclosure_digest = proposal
        .approval_digest(ApprovalKind::NewDataShare, Some(SUMMARY_PAYLOAD))
        .unwrap();
    value.approvals.push(approval(
        ApprovalKind::NewDataShare,
        ApprovalStatus::Approved,
        &disclosure_digest,
    ));
    assert_eq!(
        gate.authorize(&value, &proposal, Some(SUMMARY_PAYLOAD)),
        GateDecision::Allowed
    );
    assert_eq!(
        gate.authorize(
            &value,
            &channel_proposal(&["recipient-1", "recipient-2"]),
            Some(SUMMARY_PAYLOAD),
        ),
        GateDecision::NeedsMe(ApprovalKind::NewRecipient)
    );
    assert_eq!(
        gate.authorize(&value, &proposal, Some(b"Changed summary")),
        GateDecision::NeedsMe(ApprovalKind::NewDataShare)
    );
}

#[test]
fn model_and_listener_targets_require_exact_action_scope_and_data_share() {
    let gate = ActionGate;
    let mut value = mission(MissionStatus::Active);
    let model = ActionProposal {
        effect: EffectKind::ModelCall,
        mission_id: value.id.clone(),
        mission_scope_digest: value.scope_digest.clone(),
        target: ActionTarget::Model {
            model_id: "gpt-5.6-sol".into(),
        },
        estimated_cost_micros: None,
    };
    let action_digest = model
        .approval_digest(ApprovalKind::MissionScope, Some(b"receipt fields"))
        .unwrap();
    value.approvals.push(approval(
        ApprovalKind::MissionScope,
        ApprovalStatus::Approved,
        &action_digest,
    ));
    assert_eq!(
        gate.authorize(&value, &model, Some(b"receipt fields")),
        GateDecision::NeedsMe(ApprovalKind::NewDataShare)
    );
    let disclosure_digest = model
        .approval_digest(ApprovalKind::NewDataShare, Some(b"receipt fields"))
        .unwrap();
    value.approvals.push(approval(
        ApprovalKind::NewDataShare,
        ApprovalStatus::Approved,
        &disclosure_digest,
    ));
    assert_eq!(
        gate.authorize(&value, &model, Some(b"receipt fields")),
        GateDecision::Allowed
    );
    assert_eq!(
        gate.authorize(&value, &model, Some(b"raw receipt")),
        GateDecision::NeedsMe(ApprovalKind::NewDataShare)
    );
    let listener = ActionProposal {
        effect: EffectKind::ChannelListen,
        mission_id: value.id.clone(),
        mission_scope_digest: value.scope_digest.clone(),
        target: ActionTarget::Channel {
            channel: ChannelKind::Discord,
            conversation_id: "other-channel".into(),
            recipient_ids: Vec::new(),
        },
        estimated_cost_micros: None,
    };
    assert_eq!(
        gate.authorize(&value, &listener, None),
        GateDecision::NeedsMe(ApprovalKind::ExpandedScope)
    );
}

#[test]
fn cost_and_external_write_approvals_are_both_required() {
    let gate = ActionGate;
    let mut value = mission(MissionStatus::Active);
    let payload = b"Pay the invoice";
    let proposal = ActionProposal {
        effect: EffectKind::ReminderWrite,
        mission_id: value.id.clone(),
        mission_scope_digest: value.scope_digest.clone(),
        target: ActionTarget::ReminderList {
            list_id: "OpenOpen".into(),
        },
        estimated_cost_micros: Some(10),
    };
    let cost_digest = proposal
        .approval_digest(ApprovalKind::Cost, Some(payload))
        .unwrap();
    assert_eq!(
        gate.authorize(&value, &proposal, Some(payload)),
        GateDecision::NeedsMe(ApprovalKind::Cost)
    );
    value.approvals.push(approval(
        ApprovalKind::Cost,
        ApprovalStatus::Approved,
        &cost_digest,
    ));
    assert_eq!(
        gate.authorize(&value, &proposal, Some(payload)),
        GateDecision::NeedsMe(ApprovalKind::NewExternalWrite)
    );
    let write_digest = proposal
        .approval_digest(ApprovalKind::NewExternalWrite, Some(payload))
        .unwrap();
    value.approvals.push(approval(
        ApprovalKind::NewExternalWrite,
        ApprovalStatus::Approved,
        &write_digest,
    ));
    assert_eq!(
        gate.authorize(&value, &proposal, Some(payload)),
        GateDecision::Allowed
    );
}

#[test]
fn mission_file_target_requires_canonical_broker_components() {
    let value = mission(MissionStatus::Active);
    let gate = ActionGate;
    for relative_path in [
        "../canary",
        "",
        "./canary",
        "nested//canary",
        "nested/./canary",
        "nested/",
        "Uppercase.txt",
        "receipt 1.png",
    ] {
        let proposal = ActionProposal {
            effect: EffectKind::FileWrite,
            mission_id: value.id.clone(),
            mission_scope_digest: value.scope_digest.clone(),
            target: ActionTarget::MissionFile {
                relative_path: relative_path.into(),
            },
            estimated_cost_micros: None,
        };
        assert_eq!(
            gate.authorize(&value, &proposal, Some(b"canary")),
            GateDecision::Denied("file target escapes Mission workspace")
        );
    }
}

#[test]
fn outside_export_requires_exact_absolute_path_approval() {
    let destination = tempfile::tempdir().unwrap().path().join("report.xlsx");
    let mut value = mission(MissionStatus::Active);
    let gate = ActionGate;
    let proposal = ActionProposal {
        effect: EffectKind::FileWrite,
        mission_id: value.id.clone(),
        mission_scope_digest: value.scope_digest.clone(),
        target: ActionTarget::ApprovedExport {
            absolute_path: destination.to_string_lossy().into(),
        },
        estimated_cost_micros: None,
    };
    let digest = proposal
        .approval_digest(ApprovalKind::NewExternalWrite, Some(b"xlsx bytes"))
        .unwrap();
    value.approvals.push(approval(
        ApprovalKind::NewExternalWrite,
        ApprovalStatus::Approved,
        &digest,
    ));
    assert_eq!(
        gate.authorize(&value, &proposal, Some(b"xlsx bytes")),
        GateDecision::Allowed
    );
}
