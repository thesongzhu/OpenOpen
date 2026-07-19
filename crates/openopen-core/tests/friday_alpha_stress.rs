use openopen_core::{
    ApprovalDecision, AuditAnchor, CreateMission, CreateWorkItem, EvidenceClaims, LocalAuthority,
    MissionCommand, MissionCommandEnvelope, NewReceipt, Store, StoreError,
};
use openopen_protocol::{EvidenceKind, MissionStatus, WorkItemStatus};
use std::path::Path;

fn authority() -> LocalAuthority {
    LocalAuthority::from_master("openopen-core", [29_u8; 32])
}

fn active_mission_batch(
    index: usize,
    expected_anchor: Option<&AuditAnchor>,
) -> Vec<MissionCommandEnvelope> {
    let mission_id = format!("mission-{index}");
    let owner_id = format!("owner-{index}");
    let work_item_id = format!("work-{index}");
    let approval_id = format!("scope-{index}");
    let commands = [
        MissionCommand::Create {
            input: CreateMission {
                mission_id: mission_id.clone(),
                title: format!("Mission {index}"),
                outcome: format!("Bounded outcome {index}"),
                owner_id: owner_id.clone(),
                scope_digest: format!("scope-digest-{index}"),
                scope_approval_id: approval_id.clone(),
                scope_approval_prompt: format!("Approve Mission {index}?"),
                work_items: vec![CreateWorkItem {
                    id: work_item_id.clone(),
                    title: format!("Complete bounded step {index}"),
                }],
                now_ms: 1,
            },
        },
        MissionCommand::BeginConfirmation {
            mission_id: mission_id.clone(),
            now_ms: 2,
        },
        MissionCommand::DecideApproval {
            mission_id: mission_id.clone(),
            approval_id,
            actor_id: owner_id,
            decision: ApprovalDecision::Approve,
            now_ms: 3,
        },
        MissionCommand::Activate {
            mission_id: mission_id.clone(),
            now_ms: 4,
        },
        MissionCommand::TransitionWorkItem {
            mission_id,
            work_item_id,
            next: WorkItemStatus::Active,
            evidence_ids: Vec::new(),
            now_ms: 5,
        },
    ];
    commands
        .into_iter()
        .enumerate()
        .map(|(command_index, command)| MissionCommandEnvelope {
            command_id: format!("mission-{index}-activate-{command_index}"),
            expected_anchor: (command_index == 0)
                .then(|| expected_anchor.cloned())
                .flatten(),
            command,
        })
        .collect()
}

fn close_mission_batch(
    index: usize,
    expected_anchor: &AuditAnchor,
    security: &LocalAuthority,
) -> Vec<MissionCommandEnvelope> {
    let mission_id = format!("mission-{index}");
    let work_item_id = format!("work-{index}");
    let evidence_id = format!("evidence-{index}");
    let evidence = security.sign_evidence(EvidenceClaims {
        id: evidence_id.clone(),
        mission_id: mission_id.clone(),
        work_item_id: work_item_id.clone(),
        kind: EvidenceKind::ReminderCompleted,
        source_id: format!("reminder-{index}"),
        sha256: None,
        observed_at_ms: 6,
    });
    let commands = [
        MissionCommand::AttachEvidence {
            mission_id: mission_id.clone(),
            evidence,
            now_ms: 6,
        },
        MissionCommand::TransitionWorkItem {
            mission_id: mission_id.clone(),
            work_item_id,
            next: WorkItemStatus::Completed,
            evidence_ids: vec![evidence_id],
            now_ms: 7,
        },
        MissionCommand::Complete {
            mission_id,
            receipt: NewReceipt {
                id: format!("receipt-{index}"),
                summary: format!("Completed bounded Mission {index}"),
                actual_model: "gpt-5.6-sol".into(),
                output_hashes: Vec::new(),
                completed_at_ms: 8,
            },
            now_ms: 8,
        },
    ];
    commands
        .into_iter()
        .enumerate()
        .map(|(command_index, command)| MissionCommandEnvelope {
            command_id: format!("mission-{index}-close-{command_index}"),
            expected_anchor: (command_index == 0).then(|| expected_anchor.clone()),
            command,
        })
        .collect()
}

fn reopen(path: &Path, security: &LocalAuthority) -> Store {
    Store::open(path, security.clone()).unwrap()
}

#[test]
fn ten_coexisting_bounded_missions_survive_restarts_without_state_evidence_or_receipt_leakage() {
    let temp = tempfile::NamedTempFile::new().unwrap();
    let path = temp.path().to_path_buf();
    let security = authority();
    let mut store = reopen(&path, &security);

    for index in 1..=10 {
        let expected_anchor = store.current_verified_audit_anchor().unwrap();
        let results = store
            .execute_mission_command_batch(&active_mission_batch(index, expected_anchor.as_ref()))
            .unwrap();
        let mission = &results.last().unwrap().mission;
        assert_eq!(mission.id, format!("mission-{index}"));
        assert_eq!(mission.owner_id, format!("owner-{index}"));
        assert_eq!(mission.scope_digest, format!("scope-digest-{index}"));
        assert_eq!(mission.status, MissionStatus::Active);
        assert_eq!(mission.work_items[0].id, format!("work-{index}"));
        assert!(mission.evidence.is_empty());
    }

    let active_anchor = store.current_verified_audit_anchor().unwrap().unwrap();
    let active = store.list_missions(&active_anchor).unwrap();
    assert_eq!(active.len(), 10);
    assert!(
        active
            .iter()
            .all(|mission| mission.status == MissionStatus::Active)
    );
    assert!(active.iter().all(|mission| mission.evidence.is_empty()));
    assert!(store.list_receipts(&active_anchor).unwrap().is_empty());
    drop(store);

    for index in (1..=10).rev() {
        let mut restarted = reopen(&path, &security);
        let expected_anchor = restarted.current_verified_audit_anchor().unwrap().unwrap();

        let wrong_mission_id = if index == 1 { 10 } else { index - 1 };
        let cross_mission_evidence = security.sign_evidence(EvidenceClaims {
            id: format!("cross-evidence-{index}"),
            mission_id: format!("mission-{wrong_mission_id}"),
            work_item_id: format!("work-{wrong_mission_id}"),
            kind: EvidenceKind::ReminderCompleted,
            source_id: format!("reminder-{wrong_mission_id}"),
            sha256: None,
            observed_at_ms: 6,
        });
        let rejected = restarted.execute_mission_command(&MissionCommandEnvelope {
            command_id: format!("mission-{index}-cross-evidence"),
            expected_anchor: Some(expected_anchor.clone()),
            command: MissionCommand::AttachEvidence {
                mission_id: format!("mission-{index}"),
                evidence: cross_mission_evidence,
                now_ms: 6,
            },
        });
        assert!(matches!(rejected, Err(StoreError::Domain(_))));
        assert_eq!(
            restarted.current_verified_audit_anchor().unwrap(),
            Some(expected_anchor.clone())
        );

        let results = restarted
            .execute_mission_command_batch(&close_mission_batch(index, &expected_anchor, &security))
            .unwrap();
        let closed = results.last().unwrap();
        assert_eq!(closed.mission.id, format!("mission-{index}"));
        assert_eq!(closed.mission.owner_id, format!("owner-{index}"));
        assert_eq!(closed.mission.status, MissionStatus::Completed);
        assert_eq!(closed.mission.evidence.len(), 1);
        assert_eq!(closed.mission.evidence[0].id, format!("evidence-{index}"));
        assert_eq!(
            closed.mission.evidence[0].mission_id,
            format!("mission-{index}")
        );
        assert_eq!(
            closed.mission.evidence[0].source_id,
            format!("reminder-{index}")
        );
        let receipt = closed.receipt.as_ref().unwrap();
        assert_eq!(receipt.id, format!("receipt-{index}"));
        assert_eq!(receipt.mission_id, format!("mission-{index}"));
        assert_eq!(receipt.evidence_ids, [format!("evidence-{index}")]);
    }

    let restarted = reopen(&path, &security);
    let final_anchor = restarted.current_verified_audit_anchor().unwrap().unwrap();
    let missions = restarted.list_missions(&final_anchor).unwrap();
    let receipts = restarted.list_receipts(&final_anchor).unwrap();
    assert_eq!(missions.len(), 10);
    assert_eq!(receipts.len(), 10);
    for index in 1..=10 {
        let mission = missions
            .iter()
            .find(|mission| mission.id == format!("mission-{index}"))
            .unwrap();
        let receipt = receipts
            .iter()
            .find(|receipt| receipt.id == format!("receipt-{index}"))
            .unwrap();
        assert_eq!(mission.status, MissionStatus::Completed);
        assert_eq!(mission.owner_id, format!("owner-{index}"));
        assert_eq!(mission.scope_digest, format!("scope-digest-{index}"));
        assert_eq!(mission.evidence[0].id, format!("evidence-{index}"));
        assert_eq!(receipt.mission_id, mission.id);
        assert_eq!(receipt.evidence_ids, [format!("evidence-{index}")]);
    }
}
