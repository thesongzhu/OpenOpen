use ed25519_dalek::{Signer, SigningKey};
use openopen_core::{
    ActionProposal, ActionTarget, ApprovalDecision, AuditAnchor, BrokerEnrollmentRecord,
    CreateMission, CreateWorkItem, EffectKind, EnvelopeInsert, EvidenceClaims, GateDecision,
    LocalAuthority, MissionCommand, MissionCommandEnvelope, MissionCommandResult, MissionError,
    NewBoundaryApproval, NewReceipt, RuntimeControl, Store, StoreError, TrustedBrokerEnrollment,
    broker_enrollment_signing_bytes,
};
use openopen_protocol::{
    ApprovalKind, ChannelEnvelope, ChannelKind, EFFECT_PROTOCOL_VERSION, EffectBrokerSession,
    EffectNonCommit, EffectPermit, EffectPermitPurpose, EffectReceipt, EvidenceKind,
    MissionFileEffect, MissionStatus, RuntimeControlAuthorization, RuntimeControlReceipt,
    WorkItemStatus, effect_noncommit_signing_bytes, effect_permit_hash,
    effect_receipt_signing_bytes, runtime_control_authorization_hash,
    runtime_control_receipt_signing_bytes,
};
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};

fn authority() -> LocalAuthority {
    LocalAuthority::from_master("openopen-core", [9_u8; 32])
}

fn create_command(mission_id: &str, now_ms: i64) -> MissionCommand {
    MissionCommand::Create {
        input: CreateMission {
            mission_id: mission_id.into(),
            title: "encrypted title".into(),
            outcome: "encrypted outcome".into(),
            owner_id: "owner-1".into(),
            scope_digest: "scope-v1".into(),
            scope_approval_id: format!("scope-{mission_id}"),
            scope_approval_prompt: "Approve this Mission?".into(),
            work_items: vec![CreateWorkItem {
                id: "work-1".into(),
                title: "Build workbook".into(),
            }],
            now_ms,
        },
    }
}

fn execute(
    store: &mut Store,
    command_id: &str,
    expected_anchor: Option<AuditAnchor>,
    command: MissionCommand,
) -> Result<MissionCommandResult, StoreError> {
    store.execute_mission_command(&MissionCommandEnvelope {
        command_id: command_id.into(),
        expected_anchor,
        command,
    })
}

fn command_hash(command: &MissionCommand) -> String {
    let bytes = serde_json::to_vec(&serde_json::json!({
        "command": command,
        "version": 1,
    }))
    .unwrap();
    format!("{:x}", Sha256::digest(bytes))
}

fn permit_anchor(permit: &EffectPermit) -> AuditAnchor {
    AuditAnchor {
        sequence: permit.authorization_anchor.sequence,
        entry_hash: permit.authorization_anchor.entry_hash.clone(),
        signature_hex: permit.authorization_anchor.signature_hex.clone(),
    }
}

fn advance(
    store: &mut Store,
    command_id: &str,
    previous: &MissionCommandResult,
    command: MissionCommand,
) -> MissionCommandResult {
    execute(store, command_id, Some(previous.anchor.clone()), command).unwrap()
}

fn persist_active(store: &mut Store, mission_id: &str) -> MissionCommandResult {
    let created = execute(store, "seed-1", None, create_command(mission_id, 1)).unwrap();
    let awaiting = advance(
        store,
        "seed-2",
        &created,
        MissionCommand::BeginConfirmation {
            mission_id: mission_id.into(),
            now_ms: 2,
        },
    );
    let approved = advance(
        store,
        "seed-3",
        &awaiting,
        MissionCommand::DecideApproval {
            mission_id: mission_id.into(),
            approval_id: format!("scope-{mission_id}"),
            actor_id: "owner-1".into(),
            decision: ApprovalDecision::Approve,
            now_ms: 3,
        },
    );
    let active = advance(
        store,
        "seed-4",
        &approved,
        MissionCommand::Activate {
            mission_id: mission_id.into(),
            now_ms: 4,
        },
    );
    advance(
        store,
        "seed-5",
        &active,
        MissionCommand::TransitionWorkItem {
            mission_id: mission_id.into(),
            work_item_id: "work-1".into(),
            next: WorkItemStatus::Active,
            evidence_ids: Vec::new(),
            now_ms: 5,
        },
    )
}

fn file_proposal(relative_path: &str) -> ActionProposal {
    ActionProposal {
        effect: EffectKind::FileWrite,
        mission_id: "mission-1".into(),
        mission_scope_digest: "scope-v1".into(),
        target: ActionTarget::MissionFile {
            relative_path: relative_path.into(),
        },
        estimated_cost_micros: None,
    }
}

fn broker_session() -> EffectBrokerSession {
    let verifying_key = broker_signing_key().verifying_key();
    let key_bytes = verifying_key.to_bytes();
    let now_ms = i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis(),
    )
    .unwrap();
    EffectBrokerSession {
        protocol_version: EFFECT_PROTOCOL_VERSION,
        session_nonce: "ab".repeat(32),
        broker_key_id: format!("{:x}", Sha256::digest(key_bytes)),
        broker_verifying_key_hex: hex::encode(key_bytes),
        expires_at_ms: now_ms + 60_000,
    }
}

fn broker_signing_key() -> SigningKey {
    SigningKey::from_bytes(&[41_u8; 32])
}

fn broker_enrollment(authority: &LocalAuthority) -> TrustedBrokerEnrollment {
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
    derivation.extend([9_u8; 32]);
    let signing_key = SigningKey::from_bytes(&Sha256::digest(derivation).into());
    record.core_authorization_signature_hex = hex::encode(
        signing_key
            .sign(&broker_enrollment_signing_bytes(&record).unwrap())
            .to_bytes(),
    );
    TrustedBrokerEnrollment::from_signed_install_record(authority, &record).unwrap()
}

fn effect_store_in_memory(authority: LocalAuthority) -> Store {
    let enrollment = broker_enrollment(&authority);
    let mut store = Store::open_in_memory_with_trusted_broker(authority, enrollment).unwrap();
    commit_runtime(&mut store, true, 1);
    store
}

fn commit_runtime(store: &mut Store, enabled: bool, updated_at_ms: i64) -> RuntimeControl {
    let authorization = store
        .prepare_runtime_control(enabled, updated_at_ms)
        .unwrap();
    let receipt = signed_runtime_control_receipt(&authorization);
    store
        .commit_runtime_control(&authorization, &receipt)
        .unwrap()
}

fn signed_runtime_control_receipt(
    authorization: &RuntimeControlAuthorization,
) -> RuntimeControlReceipt {
    let key = broker_signing_key();
    let mut receipt = RuntimeControlReceipt {
        protocol_version: EFFECT_PROTOCOL_VERSION,
        authorization_hash: runtime_control_authorization_hash(authorization).unwrap(),
        checkpoint_nonce: "90".repeat(32),
        request_nonce: None,
        broker_key_id: format!("{:x}", Sha256::digest(key.verifying_key().to_bytes())),
        broker_signature_hex: String::new(),
    };
    receipt.broker_signature_hex = hex::encode(
        key.sign(&runtime_control_receipt_signing_bytes(&receipt).unwrap())
            .to_bytes(),
    );
    receipt
}

fn signed_effect_receipt(permit: &EffectPermit) -> EffectReceipt {
    let MissionFileEffect::PutFile {
        path_components,
        payload,
        ..
    } = &permit.command.effect;
    let mut receipt = EffectReceipt {
        protocol_version: EFFECT_PROTOCOL_VERSION,
        effect_id: permit.command.effect_id.clone(),
        stable_effect_hash: permit.stable_effect_hash.clone(),
        permit_hash: effect_permit_hash(permit).unwrap(),
        mission_id: permit.command.mission_id.clone(),
        path_components: path_components.clone(),
        payload_sha256: payload.sha256.clone(),
        payload_byte_len: payload.byte_len,
        broker_session_nonce: permit.broker_session_nonce.clone(),
        committed_at_ms: permit.issued_at_ms,
        attested_at_ms: permit.issued_at_ms,
        broker_key_id: broker_session().broker_key_id,
        broker_signature_hex: String::new(),
    };
    let signature = broker_signing_key().sign(&effect_receipt_signing_bytes(&receipt).unwrap());
    receipt.broker_signature_hex = hex::encode(signature.to_bytes());
    receipt
}

fn signed_effect_noncommit(permit: &EffectPermit) -> EffectNonCommit {
    let mut attestation = EffectNonCommit {
        protocol_version: EFFECT_PROTOCOL_VERSION,
        effect_id: permit.command.effect_id.clone(),
        stable_effect_hash: permit.stable_effect_hash.clone(),
        permit_hash: effect_permit_hash(permit).unwrap(),
        mission_id: permit.command.mission_id.clone(),
        broker_session_nonce: permit.broker_session_nonce.clone(),
        reconciled_at_ms: permit.issued_at_ms,
        broker_key_id: broker_session().broker_key_id,
        broker_signature_hex: String::new(),
    };
    attestation.broker_signature_hex = hex::encode(
        broker_signing_key()
            .sign(&effect_noncommit_signing_bytes(&attestation).unwrap())
            .to_bytes(),
    );
    attestation
}

fn persist_file_write_approval(
    store: &mut Store,
    active: &MissionCommandResult,
    proposal: &ActionProposal,
    payload: &[u8],
) -> MissionCommandResult {
    let digest = proposal
        .approval_digest(ApprovalKind::NewExternalWrite, Some(payload))
        .unwrap();
    let requested = advance(
        store,
        "effect-approval-request",
        active,
        MissionCommand::RequestScopeChange {
            mission_id: "mission-1".into(),
            approval: NewBoundaryApproval {
                id: "effect-write-approval".into(),
                kind: ApprovalKind::NewExternalWrite,
                prompt: "Write this exact Mission file?".into(),
                scope_digest: digest,
            },
            needs_me_id: "effect-write-needs-me".into(),
            now_ms: 6,
        },
    );
    let approved = advance(
        store,
        "effect-approval-decision",
        &requested,
        MissionCommand::DecideApproval {
            mission_id: "mission-1".into(),
            approval_id: "effect-write-approval".into(),
            actor_id: "owner-1".into(),
            decision: ApprovalDecision::Approve,
            now_ms: 7,
        },
    );
    advance(
        store,
        "effect-approval-resume",
        &approved,
        MissionCommand::Resume {
            mission_id: "mission-1".into(),
            now_ms: 8,
        },
    )
}

fn persist_completed(store: &mut Store, security: &LocalAuthority) -> MissionCommandResult {
    let active = persist_active(store, "mission-1");
    let evidence = security.sign_evidence(EvidenceClaims {
        id: "xlsx-1".into(),
        mission_id: "mission-1".into(),
        work_item_id: "work-1".into(),
        kind: EvidenceKind::XlsxVerified,
        source_id: "workbook-1".into(),
        sha256: Some("hash-1".into()),
        observed_at_ms: 6,
    });
    let evidenced = advance(
        store,
        "seed-6",
        &active,
        MissionCommand::AttachEvidence {
            mission_id: "mission-1".into(),
            evidence,
            now_ms: 6,
        },
    );
    let work_done = advance(
        store,
        "seed-7",
        &evidenced,
        MissionCommand::TransitionWorkItem {
            mission_id: "mission-1".into(),
            work_item_id: "work-1".into(),
            next: WorkItemStatus::Completed,
            evidence_ids: vec!["xlsx-1".into()],
            now_ms: 7,
        },
    );
    advance(
        store,
        "seed-8",
        &work_done,
        MissionCommand::Complete {
            mission_id: "mission-1".into(),
            receipt: NewReceipt {
                id: "receipt-1".into(),
                summary: "Workbook verified".into(),
                actual_model: "gpt-5.6-sol".into(),
                output_hashes: vec!["hash-1".into()],
                completed_at_ms: 8,
            },
            now_ms: 8,
        },
    )
}

fn envelope(id: i64, hash: &str) -> ChannelEnvelope {
    ChannelEnvelope {
        channel: ChannelKind::Discord,
        source_message_id: format!("message-{id}"),
        sender_id: "sender-1".into(),
        conversation_id: "channel-1".into(),
        content_sha256: hash.into(),
        received_at_ms: id,
    }
}

#[test]
fn every_typed_command_family_is_persistable_through_the_domain_machine() {
    let mut store = Store::open_in_memory(authority()).unwrap();
    let active = persist_active(&mut store, "mission-1");
    let paused = advance(
        &mut store,
        "pause-1",
        &active,
        MissionCommand::Pause {
            mission_id: "mission-1".into(),
            now_ms: 6,
        },
    );
    let resumed = advance(
        &mut store,
        "resume-1",
        &paused,
        MissionCommand::Resume {
            mission_id: "mission-1".into(),
            now_ms: 7,
        },
    );
    let work_needs = advance(
        &mut store,
        "work-needs-1",
        &resumed,
        MissionCommand::RequestWorkItemBoundary {
            mission_id: "mission-1".into(),
            work_item_id: "work-1".into(),
            approval: NewBoundaryApproval {
                id: "work-boundary".into(),
                kind: ApprovalKind::ExpandedScope,
                prompt: "Expand this work item?".into(),
                scope_digest: "work-v2".into(),
            },
            now_ms: 8,
        },
    );
    let work_rejected = advance(
        &mut store,
        "work-reject-1",
        &work_needs,
        MissionCommand::DecideApproval {
            mission_id: "mission-1".into(),
            approval_id: "work-boundary".into(),
            actor_id: "owner-1".into(),
            decision: ApprovalDecision::Reject,
            now_ms: 9,
        },
    );
    assert_eq!(
        work_rejected.mission.work_items[0].status,
        WorkItemStatus::Active
    );
    let mission_needs = advance(
        &mut store,
        "mission-needs-1",
        &work_rejected,
        MissionCommand::RequestScopeChange {
            mission_id: "mission-1".into(),
            approval: NewBoundaryApproval {
                id: "write-boundary".into(),
                kind: ApprovalKind::NewExternalWrite,
                prompt: "Write this output?".into(),
                scope_digest: "write-v1".into(),
            },
            needs_me_id: "needs-write".into(),
            now_ms: 10,
        },
    );
    let mission_approved = advance(
        &mut store,
        "mission-approve-1",
        &mission_needs,
        MissionCommand::DecideApproval {
            mission_id: "mission-1".into(),
            approval_id: "write-boundary".into(),
            actor_id: "owner-1".into(),
            decision: ApprovalDecision::Approve,
            now_ms: 11,
        },
    );
    let active_again = advance(
        &mut store,
        "mission-resume-1",
        &mission_approved,
        MissionCommand::Resume {
            mission_id: "mission-1".into(),
            now_ms: 12,
        },
    );
    assert_eq!(active_again.mission.status, MissionStatus::Active);
}

#[test]
fn fail_and_cancel_commands_are_persistable_and_terminal() {
    let security = authority();
    let mut cancel_store = Store::open_in_memory(security.clone()).unwrap();
    let cancel_created = execute(
        &mut cancel_store,
        "cancel-seed",
        None,
        create_command("mission-cancel", 1),
    )
    .unwrap();
    let cancel_result = advance(
        &mut cancel_store,
        "cancel-1",
        &cancel_created,
        MissionCommand::Cancel {
            mission_id: "mission-cancel".into(),
            now_ms: 2,
        },
    );
    assert_eq!(cancel_result.mission.status, MissionStatus::Cancelled);

    let mut fail_store = Store::open_in_memory(security).unwrap();
    let fail_active = persist_active(&mut fail_store, "mission-fail");
    let failed = advance(
        &mut fail_store,
        "fail-1",
        &fail_active,
        MissionCommand::Fail {
            mission_id: "mission-fail".into(),
            now_ms: 6,
        },
    );
    assert_eq!(failed.mission.status, MissionStatus::Failed);
}

#[test]
fn illegal_commands_leave_state_and_audit_unchanged() {
    let security = authority();
    let mut store = Store::open_in_memory(security).unwrap();
    let created = execute(&mut store, "create-1", None, create_command("mission-1", 1)).unwrap();
    let illegal = execute(
        &mut store,
        "illegal-activate",
        Some(created.anchor.clone()),
        MissionCommand::Activate {
            mission_id: "mission-1".into(),
            now_ms: 2,
        },
    );
    assert!(matches!(
        illegal,
        Err(StoreError::Domain(MissionError::InvalidTransition { .. }))
    ));
    assert_eq!(
        store.get_mission("mission-1", &created.anchor).unwrap(),
        Some(created.mission)
    );
}

#[test]
fn create_command_constructs_the_only_clean_genesis_shape() {
    let mut store = Store::open_in_memory(authority()).unwrap();
    let created = execute(
        &mut store,
        "create-clean",
        None,
        create_command("mission-1", 1),
    )
    .unwrap();
    assert_eq!(created.mission.status, MissionStatus::Proposed);
    assert!(created.mission.evidence.is_empty());
    assert!(created.mission.needs_me.is_none());
    assert_eq!(created.mission.approvals.len(), 1);
    assert_eq!(
        created.mission.approvals[0].status,
        openopen_protocol::ApprovalStatus::Pending
    );
    assert!(
        created
            .mission
            .work_items
            .iter()
            .all(|item| item.status == WorkItemStatus::Pending
                && item.evidence_ids.is_empty()
                && item.pending_approval_id.is_none())
    );
}

#[test]
fn invalid_mission_path_components_never_persist() {
    let mut store = Store::open_in_memory(authority()).unwrap();
    for (index, mission_id) in [
        "../mission-2",
        "a/b",
        "mission/",
        "mission//child",
        "mission/.",
        ".",
        "..",
        "",
        "mission\0alias",
        "MISSION-A",
        "mission_1",
        "-mission",
        "mission-",
        "mïssion",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    ]
    .into_iter()
    .enumerate()
    {
        assert!(matches!(
            execute(
                &mut store,
                &format!("invalid-id-{index}"),
                None,
                create_command(mission_id, 1)
            ),
            Err(StoreError::Domain(MissionError::InvalidMissionId))
        ));
    }
}

#[test]
fn rejected_scope_confirmation_cancels_without_starting_work() {
    let mut store = Store::open_in_memory(authority()).unwrap();
    let created = execute(&mut store, "reject-1", None, create_command("mission-1", 1)).unwrap();
    let awaiting = advance(
        &mut store,
        "reject-2",
        &created,
        MissionCommand::BeginConfirmation {
            mission_id: "mission-1".into(),
            now_ms: 2,
        },
    );
    let rejected = advance(
        &mut store,
        "reject-3",
        &awaiting,
        MissionCommand::DecideApproval {
            mission_id: "mission-1".into(),
            approval_id: "scope-mission-1".into(),
            actor_id: "owner-1".into(),
            decision: ApprovalDecision::Reject,
            now_ms: 3,
        },
    );
    assert_eq!(rejected.mission.status, MissionStatus::Cancelled);
    assert_eq!(
        rejected.mission.work_items[0].status,
        WorkItemStatus::Cancelled
    );
}

#[test]
fn non_owner_decision_and_invalid_work_boundary_leave_no_write() {
    let mut store = Store::open_in_memory(authority()).unwrap();
    let active = persist_active(&mut store, "mission-1");
    let non_owner = execute(
        &mut store,
        "non-owner",
        Some(active.anchor.clone()),
        MissionCommand::DecideApproval {
            mission_id: "mission-1".into(),
            approval_id: "scope-mission-1".into(),
            actor_id: "participant".into(),
            decision: ApprovalDecision::Approve,
            now_ms: 6,
        },
    );
    assert!(matches!(
        non_owner,
        Err(StoreError::Domain(MissionError::NotMissionOwner))
    ));
    let invalid_boundary = execute(
        &mut store,
        "invalid-boundary",
        Some(active.anchor.clone()),
        MissionCommand::RequestWorkItemBoundary {
            mission_id: "mission-1".into(),
            work_item_id: "work-1".into(),
            approval: NewBoundaryApproval {
                id: "bad-boundary".into(),
                kind: ApprovalKind::MissionScope,
                prompt: "Invalid work approval".into(),
                scope_digest: "bad".into(),
            },
            now_ms: 6,
        },
    );
    assert!(matches!(
        invalid_boundary,
        Err(StoreError::Domain(MissionError::WorkItemApprovalRequired))
    ));
    assert_eq!(
        store.get_mission("mission-1", &active.anchor).unwrap(),
        Some(active.mission)
    );
}

#[test]
fn completion_command_cannot_fabricate_receipt_before_evidence() {
    let mut store = Store::open_in_memory(authority()).unwrap();
    let active = persist_active(&mut store, "mission-1");
    let fabricated = execute(
        &mut store,
        "fake-complete",
        Some(active.anchor.clone()),
        MissionCommand::Complete {
            mission_id: "mission-1".into(),
            receipt: NewReceipt {
                id: "receipt-fake".into(),
                summary: "Not actually done".into(),
                actual_model: "gpt-5.6-sol".into(),
                output_hashes: Vec::new(),
                completed_at_ms: 6,
            },
            now_ms: 6,
        },
    );
    assert!(matches!(
        fabricated,
        Err(StoreError::Domain(MissionError::IncompleteWorkItems))
    ));
    assert_eq!(
        store.get_mission("mission-1", &active.anchor).unwrap(),
        Some(active.mission)
    );
}

#[test]
fn future_evidence_cannot_be_attached_or_advance_the_audit_anchor() {
    let security = authority();
    let mut store = Store::open_in_memory(security.clone()).unwrap();
    let active = persist_active(&mut store, "mission-1");
    let evidence = security.sign_evidence(EvidenceClaims {
        id: "future-xlsx".into(),
        mission_id: "mission-1".into(),
        work_item_id: "work-1".into(),
        kind: EvidenceKind::XlsxVerified,
        source_id: "future-workbook".into(),
        sha256: Some("future-hash".into()),
        observed_at_ms: 10_000,
    });
    assert!(matches!(
        execute(
            &mut store,
            "future-evidence",
            Some(active.anchor.clone()),
            MissionCommand::AttachEvidence {
                mission_id: "mission-1".into(),
                evidence,
                now_ms: 6,
            }
        ),
        Err(StoreError::Domain(MissionError::EvidenceTimeMismatch))
    ));
    assert_eq!(
        store.get_mission("mission-1", &active.anchor).unwrap(),
        Some(active.mission)
    );
}

#[test]
fn duplicate_command_id_is_exactly_idempotent_and_conflicts_fail_closed() {
    let mut store = Store::open_in_memory(authority()).unwrap();
    let envelope = MissionCommandEnvelope {
        command_id: "create-once".into(),
        expected_anchor: None,
        command: create_command("mission-1", 1),
    };
    let first = store.execute_mission_command(&envelope).unwrap();
    let duplicate = store.execute_mission_command(&envelope).unwrap();
    assert_eq!(duplicate, first);

    let mut conflict = envelope;
    conflict.command = create_command("mission-2", 1);
    assert!(matches!(
        store.execute_mission_command(&conflict),
        Err(StoreError::CommandConflict)
    ));
    assert_eq!(
        store.get_mission("mission-1", &first.anchor).unwrap(),
        Some(first.mission)
    );
}

#[test]
fn rewritten_command_hash_cannot_convert_a_conflict_into_a_retry() {
    let temp = tempfile::NamedTempFile::new().unwrap();
    let path = temp.path().to_path_buf();
    let security = authority();
    let enrollment = broker_enrollment(&security);
    let mut store = Store::open_with_trusted_broker(&path, security.clone(), enrollment).unwrap();
    let original = MissionCommandEnvelope {
        command_id: "same-id".into(),
        expected_anchor: None,
        command: create_command("mission-1", 1),
    };
    let first = store.execute_mission_command(&original).unwrap();
    drop(store);

    let conflicting = create_command("mission-2", 1);
    Connection::open(&path)
        .unwrap()
        .execute(
            "UPDATE mission_command_result SET command_hash = ?1 WHERE command_id = 'same-id'",
            [command_hash(&conflicting)],
        )
        .unwrap();
    let mut reopened = Store::open(&path, security).unwrap();
    assert!(matches!(
        execute(&mut reopened, "same-id", Some(first.anchor), conflicting),
        Err(StoreError::StateBindingMismatch(_))
    ));
}

#[test]
fn rewritten_command_mission_id_fails_signed_audit_reconciliation() {
    let temp = tempfile::NamedTempFile::new().unwrap();
    let path = temp.path().to_path_buf();
    let security = authority();
    let enrollment = broker_enrollment(&security);
    let mut store = Store::open_with_trusted_broker(&path, security.clone(), enrollment).unwrap();
    let created = execute(
        &mut store,
        "create-bound-mission",
        None,
        create_command("mission-1", 1),
    )
    .unwrap();
    drop(store);

    Connection::open(&path)
        .unwrap()
        .execute(
            "UPDATE mission_command_result SET mission_id = 'other-mission' \
             WHERE command_id = 'create-bound-mission'",
            [],
        )
        .unwrap();
    let mut reopened = Store::open(&path, security).unwrap();
    assert!(matches!(
        reopened.verify_audit_chain(&created.anchor),
        Err(StoreError::StateBindingMismatch(_))
    ));
    assert!(matches!(
        reopened.get_mission("mission-1", &created.anchor),
        Err(StoreError::StateBindingMismatch(_))
    ));
    assert!(matches!(
        execute(
            &mut reopened,
            "next-command",
            Some(created.anchor),
            MissionCommand::BeginConfirmation {
                mission_id: "mission-1".into(),
                now_ms: 2,
            }
        ),
        Err(StoreError::StateBindingMismatch(_))
    ));
}

#[test]
fn rewritten_command_result_and_matching_hash_fail_every_route_closed() {
    let temp = tempfile::NamedTempFile::new().unwrap();
    let path = temp.path().to_path_buf();
    let security = authority();
    let mut store = Store::open(&path, security.clone()).unwrap();
    let active = persist_active(&mut store, "mission-1");
    drop(store);

    let replacement = vec![0_u8, 1, 2, 3];
    let replacement_hash = format!("{:x}", Sha256::digest(&replacement));
    Connection::open(&path)
        .unwrap()
        .execute(
            "UPDATE mission_command_result
             SET encrypted_result = ?1, result_hash = ?2
             WHERE command_id = 'seed-3'",
            rusqlite::params![replacement, replacement_hash],
        )
        .unwrap();
    let mut reopened = Store::open(&path, security).unwrap();
    assert!(reopened.verify_audit_chain(&active.anchor).is_err());
    assert!(reopened.get_mission("mission-1", &active.anchor).is_err());
    assert!(
        execute(
            &mut reopened,
            "after-result-rewrite",
            Some(active.anchor),
            MissionCommand::Pause {
                mission_id: "mission-1".into(),
                now_ms: 6,
            }
        )
        .is_err()
    );
}

#[test]
fn encrypted_command_result_and_receipt_survive_restart() {
    let temp = tempfile::NamedTempFile::new().unwrap();
    let path = temp.path().to_path_buf();
    let security = authority();
    let mut store = Store::open(&path, security.clone()).unwrap();
    let completed = persist_completed(&mut store, &security);
    store.verify_audit_chain(&completed.anchor).unwrap();
    drop(store);

    let mut reopened = Store::open(&path, security).unwrap();
    assert_eq!(
        reopened
            .get_mission("mission-1", &completed.anchor)
            .unwrap(),
        Some(completed.mission.clone())
    );
    assert_eq!(
        reopened
            .get_receipt("receipt-1", &completed.anchor)
            .unwrap(),
        completed.receipt
    );
    assert!(matches!(
        execute(
            &mut reopened,
            "terminal-rewrite",
            Some(completed.anchor),
            MissionCommand::Pause {
                mission_id: "mission-1".into(),
                now_ms: 9,
            }
        ),
        Err(StoreError::Domain(MissionError::InvalidTransition { .. }))
    ));
}

#[test]
fn failed_audit_insert_rolls_back_state_and_reopens_cleanly() {
    let temp = tempfile::NamedTempFile::new().unwrap();
    let path = temp.path().to_path_buf();
    let security = authority();
    let mut store = Store::open(&path, security.clone()).unwrap();
    let active = persist_active(&mut store, "mission-1");
    Connection::open(&path)
        .unwrap()
        .execute_batch(
            "CREATE TRIGGER force_audit_rollback BEFORE INSERT ON audit_ledger
             BEGIN SELECT RAISE(ABORT, 'forced rollback'); END;",
        )
        .unwrap();
    let pause = MissionCommand::Pause {
        mission_id: "mission-1".into(),
        now_ms: 6,
    };
    assert!(matches!(
        execute(
            &mut store,
            "rollback-pause",
            Some(active.anchor.clone()),
            pause.clone()
        ),
        Err(StoreError::Database(_))
    ));
    drop(store);
    Connection::open(&path)
        .unwrap()
        .execute_batch("DROP TRIGGER force_audit_rollback")
        .unwrap();
    let mut reopened = Store::open(&path, security).unwrap();
    assert_eq!(
        reopened.get_mission("mission-1", &active.anchor).unwrap(),
        Some(active.mission)
    );
    execute(&mut reopened, "rollback-pause", Some(active.anchor), pause).unwrap();
}

#[test]
fn wrong_anchor_and_stale_time_fail_without_writes() {
    let mut store = Store::open_in_memory(authority()).unwrap();
    let active = persist_active(&mut store, "mission-1");
    let wrong = AuditAnchor {
        sequence: active.anchor.sequence,
        entry_hash: "wrong".into(),
        signature_hex: active.anchor.signature_hex.clone(),
    };
    assert!(matches!(
        execute(
            &mut store,
            "wrong-anchor",
            Some(wrong),
            MissionCommand::Pause {
                mission_id: "mission-1".into(),
                now_ms: 6,
            }
        ),
        Err(StoreError::AuditAnchorMismatch)
    ));
    assert!(matches!(
        execute(
            &mut store,
            "stale-time",
            Some(active.anchor.clone()),
            MissionCommand::Pause {
                mission_id: "mission-1".into(),
                now_ms: 4,
            }
        ),
        Err(StoreError::Domain(MissionError::StaleCommandTime))
    ));
    assert_eq!(
        store.get_mission("mission-1", &active.anchor).unwrap(),
        Some(active.mission)
    );
}

#[test]
fn deleting_audit_state_or_command_rows_fails_every_route_closed() {
    for mutation in [
        "DELETE FROM audit_ledger WHERE audit_id = 'seed-3:mission'",
        "UPDATE audit_ledger SET observed_at_ms = observed_at_ms + 1 WHERE audit_id = 'seed-3:mission'",
        "DELETE FROM mission_state",
        "DELETE FROM mission_command_result WHERE command_id = 'seed-3'",
    ] {
        let temp = tempfile::NamedTempFile::new().unwrap();
        let path = temp.path().to_path_buf();
        let security = authority();
        let mut store = Store::open(&path, security.clone()).unwrap();
        let active = persist_active(&mut store, "mission-1");
        drop(store);
        Connection::open(&path)
            .unwrap()
            .execute(mutation, [])
            .unwrap();
        let mut reopened = Store::open(&path, security).unwrap();
        assert!(reopened.verify_audit_chain(&active.anchor).is_err());
        assert!(reopened.get_mission("mission-1", &active.anchor).is_err());
        assert!(
            execute(
                &mut reopened,
                "next-pause",
                Some(active.anchor),
                MissionCommand::Pause {
                    mission_id: "mission-1".into(),
                    now_ms: 6,
                }
            )
            .is_err()
        );
    }
}

#[test]
fn nonempty_legacy_audit_without_store_observation_fails_closed() {
    let temp = tempfile::NamedTempFile::new().unwrap();
    let path = temp.path().to_path_buf();
    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch(
            "CREATE TABLE audit_ledger (sequence INTEGER PRIMARY KEY);
             INSERT INTO audit_ledger(sequence) VALUES (1);",
        )
        .unwrap();
    drop(connection);

    assert!(matches!(
        Store::open(&path, authority()),
        Err(StoreError::LegacyAuditObservationMissing)
    ));
}

#[test]
fn deleting_or_rewriting_receipt_fails_closed() {
    for mutation in [
        "DELETE FROM receipt_state",
        "UPDATE receipt_state SET completed_at_ms = completed_at_ms + 1",
    ] {
        let temp = tempfile::NamedTempFile::new().unwrap();
        let path = temp.path().to_path_buf();
        let security = authority();
        let mut store = Store::open(&path, security.clone()).unwrap();
        let completed = persist_completed(&mut store, &security);
        drop(store);
        Connection::open(&path)
            .unwrap()
            .execute(mutation, [])
            .unwrap();
        let reopened = Store::open(&path, security).unwrap();
        assert!(reopened.verify_audit_chain(&completed.anchor).is_err());
        assert!(
            reopened
                .get_receipt("receipt-1", &completed.anchor)
                .is_err()
        );
    }
}

#[test]
fn one_hundred_shuffled_duplicate_envelopes_are_idempotent() {
    let store = Store::open_in_memory(authority()).unwrap();
    for id in (0..100).rev() {
        assert_eq!(
            store
                .record_envelope(&envelope(id, &format!("hash-{id}")))
                .unwrap(),
            EnvelopeInsert::Inserted
        );
    }
    for id in (0..100).step_by(2).chain((1..100).step_by(2)) {
        assert_eq!(
            store
                .record_envelope(&envelope(id, &format!("hash-{id}")))
                .unwrap(),
            EnvelopeInsert::Duplicate
        );
    }
}

#[test]
fn changed_sender_conversation_or_content_fails_closed() {
    let store = Store::open_in_memory(authority()).unwrap();
    store.record_envelope(&envelope(1, "hash-a")).unwrap();
    let mut changed_sender = envelope(1, "hash-a");
    changed_sender.sender_id = "attacker".into();
    let mut changed_conversation = envelope(1, "hash-a");
    changed_conversation.conversation_id = "other-channel".into();
    for changed in [changed_sender, changed_conversation, envelope(1, "hash-b")] {
        assert!(matches!(
            store.record_envelope(&changed),
            Err(StoreError::EnvelopeConflict)
        ));
    }
}

#[test]
fn runtime_control_defaults_off_persists_and_does_not_delete_mission_state() {
    let temp = tempfile::NamedTempFile::new().unwrap();
    let path = temp.path().to_path_buf();
    let security = authority();
    let enrollment = broker_enrollment(&security);
    let mut store = Store::open_with_trusted_broker(&path, security.clone(), enrollment).unwrap();
    assert!(!store.runtime_control().unwrap().enabled);
    assert!(matches!(
        store.require_runtime_enabled(),
        Err(StoreError::RuntimeDisabled)
    ));

    let active = persist_active(&mut store, "mission-1");
    let on = commit_runtime(&mut store, true, 10);
    assert!(on.enabled);
    assert_eq!(on.revision, 1);
    drop(store);

    let enrollment = broker_enrollment(&security);
    let mut reopened = Store::open_with_trusted_broker(&path, security, enrollment).unwrap();
    assert_eq!(reopened.runtime_control().unwrap(), on);
    let off = commit_runtime(&mut reopened, false, 11);
    assert!(!off.enabled);
    assert_eq!(off.revision, 2);
    assert!(
        reopened
            .get_mission("mission-1", &active.anchor)
            .unwrap()
            .is_some()
    );
}

#[test]
fn runtime_control_tamper_or_deletion_fails_closed_after_signed_history_exists() {
    let temp = tempfile::NamedTempFile::new().unwrap();
    let path = temp.path().to_path_buf();
    let security = authority();
    let enrollment = broker_enrollment(&security);
    let mut store = Store::open_with_trusted_broker(&path, security.clone(), enrollment).unwrap();
    commit_runtime(&mut store, true, 10);
    drop(store);

    Connection::open(&path)
        .unwrap()
        .execute(
            "UPDATE runtime_control SET updated_at_ms = 11 WHERE singleton_id = 1",
            [],
        )
        .unwrap();
    let tampered = Store::open(&path, security.clone()).unwrap();
    assert!(matches!(
        tampered.runtime_control(),
        Err(StoreError::RuntimeControlMismatch)
    ));
    assert!(matches!(
        tampered.require_runtime_enabled(),
        Err(StoreError::RuntimeControlMismatch)
    ));
    drop(tampered);

    Connection::open(&path)
        .unwrap()
        .execute("DELETE FROM runtime_control WHERE singleton_id = 1", [])
        .unwrap();
    let deleted = Store::open(&path, security).unwrap();
    assert!(matches!(
        deleted.runtime_control(),
        Err(StoreError::RuntimeControlMismatch)
    ));
}

#[test]
fn valid_old_runtime_row_cannot_be_replayed_over_a_later_off_revision() {
    let temp = tempfile::NamedTempFile::new().unwrap();
    let path = temp.path().to_path_buf();
    let security = authority();
    let enrollment = broker_enrollment(&security);
    let mut store = Store::open_with_trusted_broker(&path, security.clone(), enrollment).unwrap();
    commit_runtime(&mut store, true, 10);
    let old: (i64, i64, i64, String) = Connection::open(&path)
        .unwrap()
        .query_row(
            "SELECT enabled, revision, updated_at_ms, signature_hex FROM runtime_control",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    commit_runtime(&mut store, false, 11);
    drop(store);

    Connection::open(&path)
        .unwrap()
        .execute(
            "UPDATE runtime_control SET enabled = ?1, revision = ?2,
                    updated_at_ms = ?3, signature_hex = ?4",
            rusqlite::params![old.0, old.1, old.2, old.3],
        )
        .unwrap();
    let replayed = Store::open(&path, security).unwrap();
    assert!(matches!(
        replayed.runtime_control(),
        Err(StoreError::RuntimeControlMismatch)
    ));
}

#[test]
fn full_database_rollback_recovers_from_broker_checkpoint_and_stays_off() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("store.sqlite3");
    let snapshot = directory.path().join("revision-1.sqlite3");
    let security = authority();
    let enrollment = broker_enrollment(&security);
    let mut store =
        Store::open_with_trusted_broker(&path, security.clone(), enrollment.clone()).unwrap();
    commit_runtime(&mut store, true, 1);
    drop(store);
    std::fs::copy(&path, &snapshot).unwrap();

    let mut store =
        Store::open_with_trusted_broker(&path, security.clone(), enrollment.clone()).unwrap();
    for revision in 2..=9 {
        commit_runtime(&mut store, true, revision);
    }
    let off_authorization = store.prepare_runtime_control(false, 10).unwrap();
    assert_eq!(off_authorization.revision, 10);
    let off_receipt = signed_runtime_control_receipt(&off_authorization);
    store
        .commit_runtime_control(&off_authorization, &off_receipt)
        .unwrap();
    drop(store);

    std::fs::copy(&snapshot, &path).unwrap();
    let mut rolled_back =
        Store::open_with_trusted_broker(&path, security.clone(), enrollment.clone()).unwrap();
    assert_eq!(rolled_back.runtime_control().unwrap().revision, 1);
    assert!(matches!(
        rolled_back.require_runtime_checkpoint(&off_authorization, &off_receipt),
        Err(StoreError::RuntimeDisabled | StoreError::RuntimeControlMismatch)
    ));
    let recovered = rolled_back
        .recover_runtime_control(&off_authorization, &off_receipt)
        .unwrap();
    assert_eq!(recovered.revision, 10);
    assert!(!recovered.enabled);
    assert!(matches!(
        rolled_back.require_runtime_enabled(),
        Err(StoreError::RuntimeDisabled)
    ));
    let next = rolled_back.prepare_runtime_control(true, 11).unwrap();
    assert_eq!(next.revision, 11);
    drop(rolled_back);

    let reopened = Store::open_with_trusted_broker(&path, security, enrollment).unwrap();
    assert_eq!(reopened.runtime_control().unwrap(), recovered);
    assert!(matches!(
        reopened.require_runtime_enabled(),
        Err(StoreError::RuntimeDisabled)
    ));
}

#[test]
fn invalid_runtime_timestamp_does_not_change_signed_state() {
    let security = authority();
    let enrollment = broker_enrollment(&security);
    let store = Store::open_in_memory_with_trusted_broker(security, enrollment).unwrap();
    assert!(matches!(
        store.prepare_runtime_control(true, -1),
        Err(StoreError::InvalidRuntimeControlTimestamp)
    ));
    assert!(!store.runtime_control().unwrap().enabled);
}

#[test]
fn store_verified_state_issues_a_signed_exact_mission_file_permit() {
    let security = authority();
    let mut store = effect_store_in_memory(security.clone());
    let active = persist_active(&mut store, "mission-1");
    let proposal = file_proposal("reports/output.xlsx");
    let payload = b"verified workbook";
    let approved = persist_file_write_approval(&mut store, &active, &proposal, payload);
    let permit = store
        .prepare_mission_file_put(
            "effect-1",
            &approved.anchor,
            &proposal,
            payload,
            &broker_session(),
        )
        .unwrap();
    assert_eq!(permit.purpose, EffectPermitPurpose::Execute);

    security.verify_effect_permit(&permit).unwrap();
    let mut changed_hash = permit.clone();
    changed_hash.stable_effect_hash = "00".repeat(32);
    assert!(security.verify_effect_permit(&changed_hash).is_err());
    assert_eq!(
        permit.command.source_anchor.sequence,
        approved.anchor.sequence
    );
    assert_eq!(permit.command.approval_ids, ["effect-write-approval"]);
    let MissionFileEffect::PutFile {
        path_components,
        payload: descriptor,
        ..
    } = permit.command.effect;
    assert_eq!(path_components, ["reports", "output.xlsx"]);
    assert_eq!(descriptor.byte_len, payload.len() as u64);
    assert_eq!(descriptor.sha256, format!("{:x}", Sha256::digest(payload)));
}

#[test]
fn store_owned_off_blocks_new_and_unresolved_effects_but_allows_read_only_reattestation() {
    let security = authority();
    let enrollment = broker_enrollment(&security);
    let mut store = Store::open_in_memory_with_trusted_broker(security, enrollment).unwrap();
    let active = persist_active(&mut store, "mission-1");
    let proposal = file_proposal("output.xlsx");
    let payload = b"runtime-owned switch";
    let approved = persist_file_write_approval(&mut store, &active, &proposal, payload);
    let session = broker_session();

    assert!(matches!(
        store.prepare_mission_file_put(
            "effect-runtime-switch",
            &approved.anchor,
            &proposal,
            payload,
            &session,
        ),
        Err(StoreError::RuntimeDisabled)
    ));

    commit_runtime(&mut store, true, 20);
    let permit = store
        .prepare_mission_file_put(
            "effect-runtime-switch",
            &approved.anchor,
            &proposal,
            payload,
            &session,
        )
        .unwrap();
    commit_runtime(&mut store, false, 21);
    assert!(matches!(
        store.prepare_mission_file_put(
            "effect-runtime-switch",
            &permit_anchor(&permit),
            &proposal,
            payload,
            &session,
        ),
        Err(StoreError::RuntimeDisabled)
    ));

    let receipt = signed_effect_receipt(&permit);
    let receipt_anchor = store
        .record_effect_receipt(&permit_anchor(&permit), &session, &permit, &receipt)
        .unwrap();
    let reattest = store
        .prepare_mission_file_put(
            "effect-runtime-switch",
            &receipt_anchor,
            &proposal,
            payload,
            &session,
        )
        .unwrap();
    assert_eq!(reattest.purpose, EffectPermitPurpose::ReattestOnly);
}

#[test]
fn missing_or_caller_selected_broker_trust_cannot_issue_a_permit() {
    let proposal = file_proposal("output.xlsx");
    let payload = b"xlsx";

    let mut unconfigured = Store::open_in_memory(authority()).unwrap();
    let active = persist_active(&mut unconfigured, "mission-1");
    let approved = persist_file_write_approval(&mut unconfigured, &active, &proposal, payload);
    assert!(matches!(
        unconfigured.prepare_mission_file_put(
            "effect-1",
            &approved.anchor,
            &proposal,
            payload,
            &broker_session(),
        ),
        Err(StoreError::MissingTrustedBrokerEnrollment)
    ));
    unconfigured.verify_audit_chain(&approved.anchor).unwrap();

    let mut pinned = effect_store_in_memory(authority());
    let active = persist_active(&mut pinned, "mission-1");
    let approved = persist_file_write_approval(&mut pinned, &active, &proposal, payload);
    let attacker_key = SigningKey::from_bytes(&[99_u8; 32]);
    let attacker_key_bytes = attacker_key.verifying_key().to_bytes();
    let mut attacker_session = broker_session();
    attacker_session.broker_key_id = format!("{:x}", Sha256::digest(attacker_key_bytes));
    attacker_session.broker_verifying_key_hex = hex::encode(attacker_key_bytes);
    assert!(matches!(
        pinned.prepare_mission_file_put(
            "effect-1",
            &approved.anchor,
            &proposal,
            payload,
            &attacker_session,
        ),
        Err(StoreError::EffectProtocol(
            openopen_core::EffectProtocolError::UntrustedBroker
        ))
    ));
    pinned.verify_audit_chain(&approved.anchor).unwrap();
}

#[test]
fn self_consistent_broker_install_record_without_core_authorization_is_rejected() {
    let security = authority();
    let broker_key = broker_signing_key().verifying_key().to_bytes();
    let record = BrokerEnrollmentRecord {
        version: 1,
        broker_key_id: format!("{:x}", Sha256::digest(broker_key)),
        broker_verifying_key_hex: hex::encode(broker_key),
        helper_designated_requirement_digest: "cd".repeat(32),
        installed_at_ms: 1,
        core_key_id: security.effect_key_id(),
        core_authorization_signature_hex: "00".repeat(64),
    };
    assert!(matches!(
        TrustedBrokerEnrollment::from_signed_install_record(&security, &record),
        Err(openopen_core::EffectProtocolError::InvalidEnrollment)
    ));
}

#[test]
fn missing_persisted_approval_or_non_broker_target_cannot_issue_a_permit() {
    let mut store = effect_store_in_memory(authority());
    let active = persist_active(&mut store, "mission-1");
    let proposal = file_proposal("output.xlsx");
    assert!(matches!(
        store.prepare_mission_file_put(
            "effect-1",
            &active.anchor,
            &proposal,
            b"xlsx",
            &broker_session(),
        ),
        Err(StoreError::EffectAuthorization(GateDecision::NeedsMe(
            ApprovalKind::NewExternalWrite
        )))
    ));

    let mut export = proposal;
    export.target = ActionTarget::ApprovedExport {
        absolute_path: "/tmp/report.xlsx".into(),
    };
    assert!(matches!(
        store.prepare_mission_file_put(
            "effect-2",
            &active.anchor,
            &export,
            b"xlsx",
            &broker_session(),
        ),
        Err(StoreError::EffectAuthorization(GateDecision::Denied(
            "privileged broker accepts only Mission-relative file targets"
        )))
    ));
}

#[test]
fn exact_effect_retry_is_idempotent_and_fence_linearizes_receipt_before_pause() {
    let security = authority();
    let mut store = effect_store_in_memory(security.clone());
    let active = persist_active(&mut store, "mission-1");
    let proposal = file_proposal("output.xlsx");
    let payload = b"xlsx-v1";
    let approved = persist_file_write_approval(&mut store, &active, &proposal, payload);
    let session = broker_session();
    let first = store
        .prepare_mission_file_put("effect-1", &approved.anchor, &proposal, payload, &session)
        .unwrap();
    let immediate_retry = store
        .prepare_mission_file_put(
            "effect-1",
            &permit_anchor(&first),
            &proposal,
            payload,
            &session,
        )
        .unwrap();
    assert_eq!(immediate_retry.command, first.command);
    assert_eq!(
        immediate_retry.authorization_anchor,
        first.authorization_anchor
    );
    security.verify_effect_permit(&immediate_retry).unwrap();
    assert!(matches!(
        execute(
            &mut store,
            "pause-while-effect-unresolved",
            Some(permit_anchor(&first)),
            MissionCommand::Pause {
                mission_id: "mission-1".into(),
                now_ms: 9,
            },
        ),
        Err(StoreError::EffectFenceActive(effect_id)) if effect_id == "effect-1"
    ));
    let receipt = signed_effect_receipt(&first);
    let receipt_anchor = store
        .record_effect_receipt(&permit_anchor(&first), &session, &first, &receipt)
        .unwrap();
    let paused = execute(
        &mut store,
        "pause-after-effect-authorization",
        Some(receipt_anchor),
        MissionCommand::Pause {
            mission_id: "mission-1".into(),
            now_ms: 9,
        },
    )
    .unwrap();
    let resumed = advance(
        &mut store,
        "resume-after-effect-authorization",
        &paused,
        MissionCommand::Resume {
            mission_id: "mission-1".into(),
            now_ms: 10,
        },
    );
    let recovery_only = store
        .prepare_mission_file_put("effect-1", &resumed.anchor, &proposal, payload, &session)
        .unwrap();
    assert_eq!(recovery_only.purpose, EffectPermitPurpose::ReattestOnly);
    assert_eq!(
        recovery_only.authorization_anchor,
        first.authorization_anchor
    );
    assert!(matches!(
        store.prepare_mission_file_put(
            "effect-1",
            &approved.anchor,
            &proposal,
            b"xlsx-v2",
            &session,
        ),
        Err(StoreError::EffectConflict)
    ));
}

#[test]
fn tampered_effect_authorization_fails_every_store_route_closed() {
    let temp = tempfile::NamedTempFile::new().unwrap();
    let path = temp.path().to_path_buf();
    let security = authority();
    let mut store =
        Store::open_with_trusted_broker(&path, security.clone(), broker_enrollment(&security))
            .unwrap();
    commit_runtime(&mut store, true, 1);
    let active = persist_active(&mut store, "mission-1");
    let proposal = file_proposal("output.xlsx");
    let payload = b"xlsx";
    let approved = persist_file_write_approval(&mut store, &active, &proposal, payload);
    let permit = store
        .prepare_mission_file_put(
            "effect-1",
            &approved.anchor,
            &proposal,
            payload,
            &broker_session(),
        )
        .unwrap();
    drop(store);
    Connection::open(&path)
        .unwrap()
        .execute(
            "UPDATE effect_authorization SET stable_effect_hash = ?1 WHERE effect_id = 'effect-1'",
            ["00".repeat(32)],
        )
        .unwrap();
    let reopened = Store::open(&path, security).unwrap();
    assert!(matches!(
        reopened.verify_audit_chain(&permit_anchor(&permit)),
        Err(StoreError::EffectAuthorizationMismatch(_))
    ));
}

#[test]
fn deleting_effect_authorization_is_detected_by_its_signed_audit_row() {
    let temp = tempfile::NamedTempFile::new().unwrap();
    let path = temp.path().to_path_buf();
    let security = authority();
    let mut store =
        Store::open_with_trusted_broker(&path, security.clone(), broker_enrollment(&security))
            .unwrap();
    commit_runtime(&mut store, true, 1);
    let active = persist_active(&mut store, "mission-1");
    let proposal = file_proposal("output.xlsx");
    let payload = b"xlsx";
    let approved = persist_file_write_approval(&mut store, &active, &proposal, payload);
    let permit = store
        .prepare_mission_file_put(
            "effect-1",
            &approved.anchor,
            &proposal,
            payload,
            &broker_session(),
        )
        .unwrap();
    let authorization_anchor = permit_anchor(&permit);
    drop(store);
    Connection::open(&path)
        .unwrap()
        .execute(
            "DELETE FROM effect_authorization WHERE effect_id = 'effect-1'",
            [],
        )
        .unwrap();
    let reopened = Store::open(&path, security).unwrap();
    assert!(reopened.verify_audit_chain(&authorization_anchor).is_err());
}

#[test]
fn broker_signed_effect_receipt_is_audited_and_exactly_idempotent() {
    let security = authority();
    let mut store = effect_store_in_memory(security);
    let active = persist_active(&mut store, "mission-1");
    let proposal = file_proposal("reports/output.xlsx");
    let payload = b"verified workbook";
    let approved = persist_file_write_approval(&mut store, &active, &proposal, payload);
    let session = broker_session();
    let permit = store
        .prepare_mission_file_put("effect-1", &approved.anchor, &proposal, payload, &session)
        .unwrap();
    let mut receipt = signed_effect_receipt(&permit);
    receipt.broker_signature_hex = hex::encode(
        broker_signing_key()
            .sign(&effect_receipt_signing_bytes(&receipt).unwrap())
            .to_bytes(),
    );

    let receipt_anchor = store
        .record_effect_receipt(&permit_anchor(&permit), &session, &permit, &receipt)
        .unwrap();
    store.verify_audit_chain(&receipt_anchor).unwrap();
    assert_eq!(
        store
            .record_effect_receipt(&permit_anchor(&permit), &session, &permit, &receipt)
            .unwrap(),
        receipt_anchor
    );

    let mut next_session = broker_session();
    next_session.session_nonce = "cd".repeat(32);
    let next_permit = store
        .prepare_mission_file_put(
            "effect-1",
            &receipt_anchor,
            &proposal,
            payload,
            &next_session,
        )
        .unwrap();
    assert_eq!(next_permit.purpose, EffectPermitPurpose::ReattestOnly);
    let mut reattested = signed_effect_receipt(&next_permit);
    reattested.committed_at_ms = receipt.committed_at_ms;
    reattested.broker_signature_hex = hex::encode(
        broker_signing_key()
            .sign(&effect_receipt_signing_bytes(&reattested).unwrap())
            .to_bytes(),
    );
    assert_eq!(
        store
            .record_effect_receipt(&receipt_anchor, &next_session, &next_permit, &reattested,)
            .unwrap(),
        receipt_anchor
    );

    let mut changed = receipt;
    changed.payload_sha256 = "00".repeat(32);
    changed.broker_signature_hex = hex::encode(
        broker_signing_key()
            .sign(&effect_receipt_signing_bytes(&changed).unwrap())
            .to_bytes(),
    );
    assert!(matches!(
        store.record_effect_receipt(&receipt_anchor, &session, &permit, &changed),
        Err(StoreError::EffectReceiptConflict)
    ));
}

#[test]
fn definitive_noncommit_clears_fence_and_makes_effect_terminal() {
    let mut store = effect_store_in_memory(authority());
    let active = persist_active(&mut store, "mission-1");
    let proposal = file_proposal("output.xlsx");
    let payload = b"never committed";
    let approved = persist_file_write_approval(&mut store, &active, &proposal, payload);
    let session = broker_session();
    let permit = store
        .prepare_mission_file_put(
            "effect-noncommit",
            &approved.anchor,
            &proposal,
            payload,
            &session,
        )
        .unwrap();
    assert!(matches!(
        execute(
            &mut store,
            "pause-before-reconciliation",
            Some(permit_anchor(&permit)),
            MissionCommand::Pause {
                mission_id: "mission-1".into(),
                now_ms: 9,
            },
        ),
        Err(StoreError::EffectFenceActive(effect_id)) if effect_id == "effect-noncommit"
    ));
    let reconciliation_permit = store
        .prepare_effect_reconciliation("effect-noncommit", &permit_anchor(&permit), &session)
        .unwrap();
    assert_eq!(
        reconciliation_permit.purpose,
        EffectPermitPurpose::Reconcile
    );
    let attestation = signed_effect_noncommit(&reconciliation_permit);
    let noncommit_anchor = store
        .record_effect_noncommit(
            &permit_anchor(&permit),
            &session,
            &reconciliation_permit,
            &attestation,
        )
        .unwrap();
    assert_eq!(
        store
            .record_effect_noncommit(
                &permit_anchor(&permit),
                &session,
                &reconciliation_permit,
                &attestation,
            )
            .unwrap(),
        noncommit_anchor
    );
    let paused = execute(
        &mut store,
        "pause-after-definitive-noncommit",
        Some(noncommit_anchor),
        MissionCommand::Pause {
            mission_id: "mission-1".into(),
            now_ms: 9,
        },
    )
    .unwrap();
    store.verify_audit_chain(&paused.anchor).unwrap();
    assert!(matches!(
        store.prepare_mission_file_put(
            "effect-noncommit",
            &paused.anchor,
            &proposal,
            payload,
            &session,
        ),
        Err(StoreError::EffectNotCommitted)
    ));
}

#[test]
fn receipt_for_a_different_exact_permit_is_rejected_even_if_broker_signed() {
    let mut store = effect_store_in_memory(authority());
    let active = persist_active(&mut store, "mission-1");
    let proposal = file_proposal("output.xlsx");
    let payload = b"xlsx";
    let approved = persist_file_write_approval(&mut store, &active, &proposal, payload);
    let session = broker_session();
    let permit = store
        .prepare_mission_file_put(
            "effect-exact-permit",
            &approved.anchor,
            &proposal,
            payload,
            &session,
        )
        .unwrap();
    let mut receipt = signed_effect_receipt(&permit);
    receipt.permit_hash = "00".repeat(32);
    receipt.broker_signature_hex = hex::encode(
        broker_signing_key()
            .sign(&effect_receipt_signing_bytes(&receipt).unwrap())
            .to_bytes(),
    );

    assert!(
        store
            .record_effect_receipt(&permit_anchor(&permit), &session, &permit, &receipt)
            .is_err()
    );
    store.verify_audit_chain(&permit_anchor(&permit)).unwrap();
}

#[test]
fn invalid_effect_receipt_rolls_back_without_advancing_audit() {
    let mut store = effect_store_in_memory(authority());
    let active = persist_active(&mut store, "mission-1");
    let proposal = file_proposal("output.xlsx");
    let payload = b"xlsx";
    let approved = persist_file_write_approval(&mut store, &active, &proposal, payload);
    let session = broker_session();
    let permit = store
        .prepare_mission_file_put("effect-1", &approved.anchor, &proposal, payload, &session)
        .unwrap();
    let mut receipt = signed_effect_receipt(&permit);
    receipt.broker_signature_hex = "00".repeat(64);

    assert!(
        store
            .record_effect_receipt(&permit_anchor(&permit), &session, &permit, &receipt)
            .is_err()
    );
    store.verify_audit_chain(&permit_anchor(&permit)).unwrap();
}

#[test]
fn effect_outcome_audit_failure_rolls_back_fence_clear_and_retries_cleanly() {
    let temp = tempfile::NamedTempFile::new().unwrap();
    let path = temp.path().to_path_buf();
    let security = authority();
    let mut store =
        Store::open_with_trusted_broker(&path, security.clone(), broker_enrollment(&security))
            .unwrap();
    commit_runtime(&mut store, true, 1);
    let active = persist_active(&mut store, "mission-1");
    let proposal = file_proposal("output.xlsx");
    let payload = b"atomic fence outcome";
    let approved = persist_file_write_approval(&mut store, &active, &proposal, payload);
    let session = broker_session();
    let permit = store
        .prepare_mission_file_put(
            "effect-atomic-outcome",
            &approved.anchor,
            &proposal,
            payload,
            &session,
        )
        .unwrap();
    let reconciliation = store
        .prepare_effect_reconciliation("effect-atomic-outcome", &permit_anchor(&permit), &session)
        .unwrap();
    let attestation = signed_effect_noncommit(&reconciliation);
    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch(
            "CREATE TRIGGER reject_effect_noncommit
             BEFORE INSERT ON audit_ledger
             WHEN NEW.action = 'effect.not_committed'
             BEGIN SELECT RAISE(ABORT, 'injected outcome audit failure'); END;",
        )
        .unwrap();
    drop(connection);

    assert!(
        store
            .record_effect_noncommit(
                &permit_anchor(&permit),
                &session,
                &reconciliation,
                &attestation,
            )
            .is_err()
    );
    store.verify_audit_chain(&permit_anchor(&permit)).unwrap();
    assert!(matches!(
        execute(
            &mut store,
            "pause-after-rolled-back-outcome",
            Some(permit_anchor(&permit)),
            MissionCommand::Pause {
                mission_id: "mission-1".into(),
                now_ms: 9,
            },
        ),
        Err(StoreError::EffectFenceActive(effect_id))
            if effect_id == "effect-atomic-outcome"
    ));

    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch("DROP TRIGGER reject_effect_noncommit")
        .unwrap();
    drop(connection);
    let anchor = store
        .record_effect_noncommit(
            &permit_anchor(&permit),
            &session,
            &reconciliation,
            &attestation,
        )
        .unwrap();
    store.verify_audit_chain(&anchor).unwrap();
}

#[test]
fn deleting_fence_or_rewriting_noncommit_is_detected() {
    for mutation in [
        "DELETE FROM effect_fence WHERE effect_id = 'effect-bound-outcome'",
        "UPDATE effect_noncommit SET mission_id = 'mission-attacker' WHERE effect_id = 'effect-bound-outcome'",
    ] {
        let temp = tempfile::NamedTempFile::new().unwrap();
        let path = temp.path().to_path_buf();
        let security = authority();
        let enrollment = broker_enrollment(&security);
        let mut store =
            Store::open_with_trusted_broker(&path, security.clone(), enrollment.clone()).unwrap();
        commit_runtime(&mut store, true, 1);
        let active = persist_active(&mut store, "mission-1");
        let proposal = file_proposal("output.xlsx");
        let payload = b"bound outcome";
        let approved = persist_file_write_approval(&mut store, &active, &proposal, payload);
        let session = broker_session();
        let permit = store
            .prepare_mission_file_put(
                "effect-bound-outcome",
                &approved.anchor,
                &proposal,
                payload,
                &session,
            )
            .unwrap();
        if mutation.starts_with("DELETE FROM effect_fence") {
            drop(store);
            Connection::open(&path)
                .unwrap()
                .execute(mutation, [])
                .unwrap();
            let reopened = Store::open_with_trusted_broker(&path, security, enrollment).unwrap();
            assert!(
                reopened
                    .verify_audit_chain(&permit_anchor(&permit))
                    .is_err()
            );
            continue;
        }
        let reconciliation = store
            .prepare_effect_reconciliation(
                "effect-bound-outcome",
                &permit_anchor(&permit),
                &session,
            )
            .unwrap();
        let attestation = signed_effect_noncommit(&reconciliation);
        let outcome_anchor = store
            .record_effect_noncommit(
                &permit_anchor(&permit),
                &session,
                &reconciliation,
                &attestation,
            )
            .unwrap();
        drop(store);
        Connection::open(&path)
            .unwrap()
            .execute(mutation, [])
            .unwrap();
        let reopened = Store::open_with_trusted_broker(&path, security, enrollment).unwrap();
        assert!(reopened.verify_audit_chain(&outcome_anchor).is_err());
    }
}

#[test]
fn tampered_effect_receipt_state_or_audit_fails_closed() {
    let mutations = [
        "UPDATE effect_receipt SET mission_id = 'mission-attacker' WHERE effect_id = 'effect-1'",
        "UPDATE effect_receipt SET local_signature_hex = '00' WHERE effect_id = 'effect-1'",
        "DELETE FROM audit_ledger WHERE action = 'effect.committed'",
    ];

    for mutation in mutations {
        let temp = tempfile::NamedTempFile::new().unwrap();
        let path = temp.path().to_path_buf();
        let security = authority();
        let mut store =
            Store::open_with_trusted_broker(&path, security.clone(), broker_enrollment(&security))
                .unwrap();
        commit_runtime(&mut store, true, 1);
        let active = persist_active(&mut store, "mission-1");
        let proposal = file_proposal("output.xlsx");
        let payload = b"xlsx";
        let approved = persist_file_write_approval(&mut store, &active, &proposal, payload);
        let session = broker_session();
        let permit = store
            .prepare_mission_file_put("effect-1", &approved.anchor, &proposal, payload, &session)
            .unwrap();
        let receipt = signed_effect_receipt(&permit);
        let receipt_anchor = store
            .record_effect_receipt(&permit_anchor(&permit), &session, &permit, &receipt)
            .unwrap();
        drop(store);

        Connection::open(&path)
            .unwrap()
            .execute(mutation, [])
            .unwrap();
        let enrollment = broker_enrollment(&security);
        let reopened = Store::open_with_trusted_broker(&path, security, enrollment).unwrap();
        assert!(reopened.verify_audit_chain(&receipt_anchor).is_err());
    }
}
