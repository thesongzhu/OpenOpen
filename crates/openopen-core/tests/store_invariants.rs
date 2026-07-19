use ed25519_dalek::{Signer, SigningKey};
use openopen_core::{
    ActionProposal, ActionTarget, ApprovalDecision, AuditAnchor, BrokerEnrollmentRecord,
    CreateMission, CreateWorkItem, EffectKind, EvidenceClaims, GateDecision, LocalAuthority,
    MissionCommand, MissionCommandEnvelope, MissionCommandResult, MissionError,
    NewBoundaryApproval, NewReceipt, RuntimeControl, Store, StoreError, TrustedBrokerEnrollment,
    broker_enrollment_signing_bytes, channel_message_payload, channel_need_you_content,
    channel_receipt_content,
};
use openopen_protocol::{
    ApprovalKind, ApprovalStatus, ChannelCursor, ChannelDeliveryReceipt, ChannelEnvelope,
    ChannelInboundDecision, ChannelInboundMessageClass, ChannelKind, ChannelMessageKind,
    ChannelMissionEvent, ChannelModelDisposition, ChannelObservation, ChannelOutboundDisposition,
    ChannelOutboundIntent, ChannelPairing, ChannelRouteApproval, ChannelRouteApprovalDecision,
    ChannelRouteRole, EFFECT_PROTOCOL_VERSION, EffectBrokerSession, EffectNonCommit, EffectPermit,
    EffectPermitPurpose, EffectReceipt, EvidenceKind, MissionFileEffect, MissionStatus,
    OutcomeSuggestion, RuntimeControlAuthorization, RuntimeControlReceipt, WorkItemStatus,
    effect_noncommit_signing_bytes, effect_permit_hash, effect_receipt_signing_bytes,
    runtime_control_authorization_hash, runtime_control_receipt_signing_bytes,
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

fn broker_enrollment_record(authority: &LocalAuthority) -> BrokerEnrollmentRecord {
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
    record
}

fn broker_enrollment(authority: &LocalAuthority) -> TrustedBrokerEnrollment {
    TrustedBrokerEnrollment::from_signed_install_record(
        authority,
        &broker_enrollment_record(authority),
    )
    .unwrap()
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
                target: None,
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

fn persist_channel_send_approvals(
    store: &mut Store,
    active: &MissionCommandResult,
    payload: &[u8],
) -> MissionCommandResult {
    let proposal = ActionProposal {
        effect: EffectKind::ChannelSend,
        mission_id: "mission-1".into(),
        mission_scope_digest: "scope-v1".into(),
        target: ActionTarget::Channel {
            channel: ChannelKind::Discord,
            conversation_id: "channel-1".into(),
            recipient_ids: vec!["owner-1".into()],
        },
        estimated_cost_micros: None,
    };
    let recipient_digest = proposal
        .approval_digest(ApprovalKind::NewRecipient, Some(payload))
        .unwrap();
    let requested_recipient = advance(
        store,
        "channel-recipient-request",
        active,
        MissionCommand::RequestScopeChange {
            mission_id: "mission-1".into(),
            approval: NewBoundaryApproval {
                id: "channel-recipient-approval".into(),
                kind: ApprovalKind::NewRecipient,
                prompt: "Return this exact update to the originating conversation?".into(),
                scope_digest: recipient_digest,
                target: None,
            },
            needs_me_id: "channel-recipient-needs-me".into(),
            now_ms: 6,
        },
    );
    let approved_recipient = advance(
        store,
        "channel-recipient-decision",
        &requested_recipient,
        MissionCommand::DecideApproval {
            mission_id: "mission-1".into(),
            approval_id: "channel-recipient-approval".into(),
            actor_id: "owner-1".into(),
            decision: ApprovalDecision::Approve,
            now_ms: 7,
        },
    );
    let resumed = advance(
        store,
        "channel-recipient-resume",
        &approved_recipient,
        MissionCommand::Resume {
            mission_id: "mission-1".into(),
            now_ms: 8,
        },
    );
    let disclosure_digest = proposal
        .approval_digest(ApprovalKind::NewDataShare, Some(payload))
        .unwrap();
    let requested_disclosure = advance(
        store,
        "channel-disclosure-request",
        &resumed,
        MissionCommand::RequestScopeChange {
            mission_id: "mission-1".into(),
            approval: NewBoundaryApproval {
                id: "channel-disclosure-approval".into(),
                kind: ApprovalKind::NewDataShare,
                prompt: "Share these exact bytes with the originating conversation?".into(),
                scope_digest: disclosure_digest,
                target: None,
            },
            needs_me_id: "channel-disclosure-needs-me".into(),
            now_ms: 9,
        },
    );
    let approved_disclosure = advance(
        store,
        "channel-disclosure-decision",
        &requested_disclosure,
        MissionCommand::DecideApproval {
            mission_id: "mission-1".into(),
            approval_id: "channel-disclosure-approval".into(),
            actor_id: "owner-1".into(),
            decision: ApprovalDecision::Approve,
            now_ms: 10,
        },
    );
    advance(
        store,
        "channel-disclosure-resume",
        &approved_disclosure,
        MissionCommand::Resume {
            mission_id: "mission-1".into(),
            now_ms: 11,
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

fn channel_pairing() -> ChannelPairing {
    ChannelPairing {
        channel: ChannelKind::Discord,
        owner_sender_id: "owner-1".into(),
        conversation_id: "channel-1".into(),
        require_explicit_address: true,
        discord: Some(openopen_protocol::DiscordPairingMetadata {
            guild_id: "101".into(),
            bot_user_id: "102".into(),
            application_id: "103".into(),
            setup_source_message_id: "104".into(),
            setup_candidate_id: format!("discord-pair-{}", "a".repeat(64)),
        }),
        paired_at_ms: 1,
    }
}

fn message_body(id: u64) -> String {
    format!("body-{id}")
}

fn observation(id: u64) -> ChannelObservation {
    let received_at_ms = i64::try_from(id).unwrap();
    ChannelObservation {
        envelope: ChannelEnvelope {
            channel: ChannelKind::Discord,
            source_message_id: format!("message-{id}"),
            sender_id: "owner-1".into(),
            conversation_id: "channel-1".into(),
            content_sha256: format!("{:x}", Sha256::digest(message_body(id))),
            received_at_ms,
        },
        cursor: ChannelCursor {
            channel: ChannelKind::Discord,
            conversation_id: "channel-1".into(),
            opaque_value: format!("cursor-{id}"),
            order: id,
            observed_at_ms: received_at_ms,
        },
        is_bot: false,
        explicitly_addressed: true,
    }
}

fn imessage_pairing() -> ChannelPairing {
    ChannelPairing {
        channel: ChannelKind::IMessage,
        owner_sender_id: "owner-imessage".into(),
        conversation_id: "chat-imessage".into(),
        require_explicit_address: true,
        discord: None,
        paired_at_ms: 6,
    }
}

fn imessage_observation(id: u64) -> ChannelObservation {
    let content = format!("imessage-body-{id}");
    ChannelObservation {
        envelope: ChannelEnvelope {
            channel: ChannelKind::IMessage,
            source_message_id: format!("imessage-{id}"),
            sender_id: "owner-imessage".into(),
            conversation_id: "chat-imessage".into(),
            content_sha256: format!("{:x}", Sha256::digest(&content)),
            received_at_ms: i64::try_from(id + 10).unwrap(),
        },
        cursor: ChannelCursor {
            channel: ChannelKind::IMessage,
            conversation_id: "chat-imessage".into(),
            opaque_value: format!("imessage-cursor-{id}"),
            order: id,
            observed_at_ms: i64::try_from(id + 10).unwrap(),
        },
        is_bot: false,
        explicitly_addressed: true,
    }
}

fn imessage_body(id: u64) -> String {
    format!("imessage-body-{id}")
}

fn persist_channel_active(store: &mut Store) -> MissionCommandResult {
    store.pair_channel(&channel_pairing()).unwrap();
    store
        .ingest_channel_message(&observation(1), &message_body(1))
        .unwrap();
    assert_eq!(
        store
            .begin_channel_model(ChannelKind::Discord, "message-1")
            .unwrap()
            .disposition,
        ChannelModelDisposition::ExecuteNow
    );
    let suggestion = OutcomeSuggestion {
        id: "suggestion-channel-1".into(),
        title: "encrypted title".into(),
        why_now: "encrypted outcome".into(),
        proposed_steps: vec!["Build workbook".into()],
        source_refs: vec!["discord:message-1".into()],
    };
    store
        .record_channel_suggestion(ChannelKind::Discord, "message-1", &suggestion, 2)
        .unwrap();
    let commands = vec![
        create_command("mission-1", 1),
        MissionCommand::BeginConfirmation {
            mission_id: "mission-1".into(),
            now_ms: 2,
        },
        MissionCommand::DecideApproval {
            mission_id: "mission-1".into(),
            approval_id: "scope-mission-1".into(),
            actor_id: "owner-1".into(),
            decision: ApprovalDecision::Approve,
            now_ms: 3,
        },
        MissionCommand::Activate {
            mission_id: "mission-1".into(),
            now_ms: 4,
        },
        MissionCommand::TransitionWorkItem {
            mission_id: "mission-1".into(),
            work_item_id: "work-1".into(),
            next: WorkItemStatus::Active,
            evidence_ids: Vec::new(),
            now_ms: 5,
        },
    ];
    let expected_anchor = store.current_verified_audit_anchor().unwrap();
    let envelopes = commands
        .into_iter()
        .enumerate()
        .map(|(index, command)| MissionCommandEnvelope {
            command_id: format!("channel-seed-{index}"),
            expected_anchor: (index == 0).then(|| expected_anchor.clone()).flatten(),
            command,
        })
        .collect::<Vec<_>>();
    let mut active = store
        .execute_mission_command_batch_with_primary_channel_route(
            &envelopes,
            ChannelKind::Discord,
            "message-1",
            &suggestion.id,
            5,
        )
        .unwrap()
        .pop()
        .unwrap();
    active.anchor = store.current_verified_audit_anchor().unwrap().unwrap();
    active
}

fn primary_route(store: &Store) -> (String, u64) {
    let route_set = store.channel_route_set("mission-1").unwrap().unwrap();
    (route_set.primary_route_id, route_set.revision)
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
                target: None,
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
                target: None,
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
                target: None,
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
    let mut store = effect_store_in_memory(authority());
    store.pair_channel(&channel_pairing()).unwrap();
    for id in 1..=100 {
        assert_eq!(
            store
                .ingest_channel_message(&observation(id), &message_body(id))
                .unwrap()
                .decision,
            ChannelInboundDecision::Accepted
        );
    }
    for id in (1..=100).step_by(2).chain((2..=100).step_by(2)) {
        assert_eq!(
            store
                .ingest_channel_message(&observation(id), &message_body(id))
                .unwrap()
                .decision,
            ChannelInboundDecision::Duplicate
        );
    }
}

#[test]
fn changed_channel_provenance_for_one_message_id_fails_closed() {
    let mut store = effect_store_in_memory(authority());
    store.pair_channel(&channel_pairing()).unwrap();
    store
        .ingest_channel_message(&observation(1), &message_body(1))
        .unwrap();
    let mut changed_sender = observation(1);
    changed_sender.envelope.sender_id = "attacker".into();
    let mut changed_content = observation(1);
    changed_content.envelope.content_sha256 = "b".repeat(64);
    let mut changed_cursor = observation(1);
    changed_cursor.cursor.opaque_value = "different-cursor".into();
    for changed in [changed_sender, changed_content, changed_cursor] {
        assert!(matches!(
            store.ingest_channel_message(&changed, &message_body(1)),
            Err(StoreError::ChannelObservationConflict)
        ));
    }
}

#[test]
fn pairing_filters_before_model_acceptance_and_cursor_never_moves_backward() {
    let mut store = effect_store_in_memory(authority());
    store.pair_channel(&channel_pairing()).unwrap();

    let mut attacker = observation(1);
    attacker.envelope.sender_id = "attacker".into();
    assert_eq!(
        store
            .ingest_channel_message(&attacker, &message_body(1))
            .unwrap()
            .decision,
        ChannelInboundDecision::IgnoredSender
    );
    let mut unaddressed = observation(2);
    unaddressed.explicitly_addressed = false;
    assert_eq!(
        store
            .ingest_channel_message(&unaddressed, &message_body(2))
            .unwrap()
            .decision,
        ChannelInboundDecision::IgnoredNotAddressed
    );
    let mut bot = observation(3);
    bot.is_bot = true;
    assert_eq!(
        store
            .ingest_channel_message(&bot, &message_body(3))
            .unwrap()
            .decision,
        ChannelInboundDecision::IgnoredBot
    );
    assert_eq!(
        store
            .ingest_channel_message(&observation(4), &message_body(4))
            .unwrap()
            .decision,
        ChannelInboundDecision::Accepted
    );
    let mut stale = observation(3);
    stale.envelope.source_message_id = "different-stale-message".into();
    assert_eq!(
        store
            .ingest_channel_message(&stale, &message_body(3))
            .unwrap()
            .decision,
        ChannelInboundDecision::IgnoredStaleCursor
    );

    let mut other_conversation = observation(5);
    other_conversation.envelope.conversation_id = "unapproved-channel".into();
    other_conversation.cursor.conversation_id = "unapproved-channel".into();
    assert_eq!(
        store
            .ingest_channel_message(&other_conversation, &message_body(5))
            .unwrap()
            .decision,
        ChannelInboundDecision::IgnoredConversation
    );
}

#[test]
fn provider_recovery_high_water_advances_once_and_never_skips_backward() {
    let mut store = effect_store_in_memory(authority());
    store.pair_channel(&channel_pairing()).unwrap();
    store
        .ingest_channel_message(&observation(4), &message_body(4))
        .unwrap();
    let high_water = ChannelCursor {
        channel: ChannelKind::Discord,
        conversation_id: "channel-1".into(),
        opaque_value: "cursor-10".into(),
        order: 10,
        observed_at_ms: 10,
    };
    store.advance_channel_cursor(&high_water).unwrap();
    assert_eq!(
        store
            .channel_cursor(ChannelKind::Discord, "channel-1")
            .unwrap(),
        Some(high_water.clone())
    );
    let mut exact_retry = high_water.clone();
    exact_retry.observed_at_ms = 11;
    store.advance_channel_cursor(&exact_retry).unwrap();
    assert!(matches!(
        store.advance_channel_cursor(&observation(9).cursor),
        Err(StoreError::ChannelObservationConflict)
    ));
    let mut other = high_water;
    other.conversation_id = "unapproved-channel".into();
    assert!(matches!(
        store.advance_channel_cursor(&other),
        Err(StoreError::ChannelObservationConflict)
    ));
}

#[test]
fn global_off_rejects_observation_without_advancing_durable_dedupe() {
    let security = authority();
    let enrollment = broker_enrollment(&security);
    let mut store = Store::open_in_memory_with_trusted_broker(security, enrollment).unwrap();
    store.pair_channel(&channel_pairing()).unwrap();
    assert!(matches!(
        store.ingest_channel_message(&observation(1), &message_body(1)),
        Err(StoreError::RuntimeDisabled)
    ));
    commit_runtime(&mut store, true, 1);
    assert_eq!(
        store
            .ingest_channel_message(&observation(1), &message_body(1))
            .unwrap()
            .decision,
        ChannelInboundDecision::Accepted
    );
}

#[test]
fn accepted_channel_model_dispatch_is_once_only_and_result_cannot_cross_global_off() {
    let temp = tempfile::NamedTempFile::new().unwrap();
    let path = temp.path().to_path_buf();
    let security = authority();
    let enrollment = broker_enrollment(&security);
    let mut store =
        Store::open_with_trusted_broker(&path, security.clone(), enrollment.clone()).unwrap();
    commit_runtime(&mut store, true, 1);
    store.pair_channel(&channel_pairing()).unwrap();
    store
        .ingest_channel_message(&observation(1), &message_body(1))
        .unwrap();
    let first = store
        .begin_channel_model(ChannelKind::Discord, "message-1")
        .unwrap();
    assert_eq!(first.disposition, ChannelModelDisposition::ExecuteNow);
    assert_eq!(first.content, "body-1");
    drop(store);

    let mut reopened = Store::open_with_trusted_broker(&path, security, enrollment).unwrap();
    assert_eq!(
        reopened
            .begin_channel_model(ChannelKind::Discord, "message-1")
            .unwrap()
            .disposition,
        ChannelModelDisposition::RecoverOnly
    );
    let suggestion = OutcomeSuggestion {
        id: "suggestion-channel-1".into(),
        title: "Close the loop".into(),
        why_now: "The owner explicitly asked.".into(),
        proposed_steps: vec!["Create the exact reminder".into()],
        source_refs: vec!["discord:message-1".into()],
    };
    commit_runtime(&mut reopened, false, 2);
    assert!(matches!(
        reopened.record_channel_suggestion(ChannelKind::Discord, "message-1", &suggestion, 2),
        Err(StoreError::RuntimeDisabled)
    ));
    reopened
        .fail_channel_model(ChannelKind::Discord, "message-1", 3)
        .unwrap();
    assert!(matches!(
        reopened.begin_channel_model(ChannelKind::Discord, "message-1"),
        Err(StoreError::RuntimeDisabled)
    ));
    commit_runtime(&mut reopened, true, 3);
    let failed = reopened
        .begin_channel_model(ChannelKind::Discord, "message-1")
        .unwrap();
    assert_eq!(failed.disposition, ChannelModelDisposition::RecoverOnly);
    assert!(failed.suggestion.is_none());
    assert_eq!(
        reopened
            .latest_failed_channel_model(ChannelKind::Discord)
            .unwrap()
            .as_deref(),
        Some("message-1")
    );
    assert!(reopened.latest_channel_suggestion().unwrap().is_none());
    assert!(matches!(
        reopened.record_channel_suggestion(ChannelKind::Discord, "message-1", &suggestion, 4),
        Err(StoreError::ChannelObservationConflict)
    ));
    let correction = "Correction to previous: body-2";
    let mut correction_observation = observation(2);
    correction_observation.envelope.content_sha256 =
        format!("{:x}", Sha256::digest(correction.as_bytes()));
    assert_eq!(
        reopened
            .ingest_channel_message(&correction_observation, correction)
            .unwrap()
            .decision,
        ChannelInboundDecision::Accepted
    );
    assert!(
        reopened
            .latest_failed_channel_model(ChannelKind::Discord)
            .unwrap()
            .is_none()
    );
    assert_eq!(
        reopened
            .next_queued_channel_model(ChannelKind::Discord)
            .unwrap()
            .as_deref(),
        Some("message-2"),
        "a later explicit correction must supersede persistent Need you without replaying the failed dispatch"
    );
}

#[test]
fn terminal_channel_incident_is_stable_across_one_hundred_reads_and_acknowledges_once() {
    let temp = tempfile::NamedTempFile::new().unwrap();
    let path = temp.path().to_path_buf();
    let security = authority();
    let enrollment = broker_enrollment(&security);
    let mut store =
        Store::open_with_trusted_broker(&path, security.clone(), enrollment.clone()).unwrap();
    let runtime = commit_runtime(&mut store, true, 1);
    store.pair_channel(&channel_pairing()).unwrap();
    store
        .ingest_channel_message(&observation(1), &message_body(1))
        .unwrap();
    assert_eq!(
        store
            .begin_channel_model(ChannelKind::Discord, "message-1")
            .unwrap()
            .disposition,
        ChannelModelDisposition::ExecuteNow
    );
    store
        .fail_channel_model(ChannelKind::Discord, "message-1", 3)
        .unwrap();
    let expected = store
        .channel_failure_incidents(Some(ChannelKind::Discord))
        .unwrap();
    assert_eq!(expected.len(), 1);
    for _ in 0..100 {
        assert_eq!(
            store
                .channel_failure_incidents(Some(ChannelKind::Discord))
                .unwrap(),
            expected
        );
    }
    let anchor = AuditAnchor {
        sequence: expected[0].incident_audit_anchor.sequence,
        entry_hash: expected[0].incident_audit_anchor.entry_hash.clone(),
        signature_hex: expected[0].incident_audit_anchor.signature_hex.clone(),
    };
    let acknowledged = store
        .acknowledge_channel_failure_incident(
            &expected[0].incident_id,
            &anchor,
            runtime.revision,
            4,
        )
        .unwrap();
    assert!(acknowledged.acknowledgement.is_some());
    let response_loss_retry = store
        .acknowledge_channel_failure_incident(
            &expected[0].incident_id,
            &anchor,
            runtime.revision,
            5,
        )
        .unwrap();
    assert_eq!(response_loss_retry, acknowledged);
    drop(store);
    let connection = Connection::open(path).unwrap();
    let acknowledgement_audits: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM audit_ledger
             WHERE action = 'channel.failure_incident_acknowledged'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(acknowledgement_audits, 1);
}

#[test]
fn incident_projection_reveals_all_unacknowledged_history_without_overbounding_the_ui() {
    let temp = tempfile::NamedTempFile::new().unwrap();
    let path = temp.path().to_path_buf();
    let security = authority();
    let enrollment = broker_enrollment(&security);
    let mut store =
        Store::open_with_trusted_broker(&path, security.clone(), enrollment.clone()).unwrap();
    let runtime = commit_runtime(&mut store, true, 1);
    store.pair_channel(&channel_pairing()).unwrap();
    store.pair_channel(&imessage_pairing()).unwrap();

    for id in 1..=129 {
        let (channel, source_message_id) = if id % 2 == 0 {
            store
                .ingest_channel_message(&imessage_observation(id), &imessage_body(id))
                .unwrap();
            (ChannelKind::IMessage, format!("imessage-{id}"))
        } else {
            store
                .ingest_channel_message(&observation(id), &message_body(id))
                .unwrap();
            (ChannelKind::Discord, format!("message-{id}"))
        };
        assert_eq!(
            store
                .begin_channel_model(channel, &source_message_id)
                .unwrap()
                .disposition,
            ChannelModelDisposition::ExecuteNow
        );
        store
            .fail_channel_model(
                channel,
                &source_message_id,
                1_000 + i64::try_from(id).unwrap(),
            )
            .unwrap();
    }

    let complete = store.channel_failure_incidents(None).unwrap();
    assert_eq!(
        complete.len(),
        129,
        "durable history must never be truncated"
    );
    let first_page = store.channel_failure_incident_projection(None).unwrap();
    assert_eq!(first_page.len(), 128);
    assert_eq!(first_page, complete[..128]);

    let first = &first_page[0];
    let first_anchor = AuditAnchor {
        sequence: first.incident_audit_anchor.sequence,
        entry_hash: first.incident_audit_anchor.entry_hash.clone(),
        signature_hex: first.incident_audit_anchor.signature_hex.clone(),
    };
    store
        .acknowledge_channel_failure_incident(
            &first.incident_id,
            &first_anchor,
            runtime.revision,
            2_000,
        )
        .unwrap();
    let next_page = store.channel_failure_incident_projection(None).unwrap();
    assert_eq!(next_page.len(), 128);
    assert!(
        !next_page
            .iter()
            .any(|value| value.incident_id == first.incident_id)
    );
    assert!(
        next_page
            .iter()
            .any(|value| value.incident_id == complete[128].incident_id),
        "acknowledging the oldest row must reveal the next durable incident"
    );

    drop(store);
    let reopened = Store::open_with_trusted_broker(&path, security, enrollment).unwrap();
    assert_eq!(
        reopened.channel_failure_incident_projection(None).unwrap(),
        next_page,
        "the bounded recoverable projection must survive restart"
    );
    assert_eq!(reopened.channel_failure_incidents(None).unwrap().len(), 129);
}

#[test]
fn terminal_incident_and_acknowledgement_failures_roll_back_atomically() {
    let temp = tempfile::NamedTempFile::new().unwrap();
    let path = temp.path().to_path_buf();
    let security = authority();
    let enrollment = broker_enrollment(&security);
    let mut store =
        Store::open_with_trusted_broker(&path, security.clone(), enrollment.clone()).unwrap();
    let runtime = commit_runtime(&mut store, true, 1);
    store.pair_channel(&channel_pairing()).unwrap();
    store
        .ingest_channel_message(&observation(1), &message_body(1))
        .unwrap();
    store
        .begin_channel_model(ChannelKind::Discord, "message-1")
        .unwrap();
    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch(
            "CREATE TRIGGER reject_channel_failure_incident
             BEFORE INSERT ON channel_failure_incident
             BEGIN SELECT RAISE(ABORT, 'injected incident failure'); END;",
        )
        .unwrap();
    drop(connection);
    assert!(
        store
            .fail_channel_model(ChannelKind::Discord, "message-1", 3)
            .is_err()
    );
    assert_eq!(
        store
            .started_channel_model(ChannelKind::Discord)
            .unwrap()
            .as_deref(),
        Some("message-1")
    );
    assert!(store.channel_failure_incidents(None).unwrap().is_empty());
    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch("DROP TRIGGER reject_channel_failure_incident")
        .unwrap();
    drop(connection);
    store
        .fail_channel_model(ChannelKind::Discord, "message-1", 4)
        .unwrap();
    let incident = store.channel_failure_incidents(None).unwrap().remove(0);
    let anchor = AuditAnchor {
        sequence: incident.incident_audit_anchor.sequence,
        entry_hash: incident.incident_audit_anchor.entry_hash.clone(),
        signature_hex: incident.incident_audit_anchor.signature_hex.clone(),
    };
    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch(
            "CREATE TRIGGER reject_channel_failure_ack
             BEFORE INSERT ON audit_ledger
             WHEN NEW.action = 'channel.failure_incident_acknowledged'
             BEGIN SELECT RAISE(ABORT, 'injected acknowledgement failure'); END;",
        )
        .unwrap();
    drop(connection);
    assert!(
        store
            .acknowledge_channel_failure_incident(
                &incident.incident_id,
                &anchor,
                runtime.revision,
                5,
            )
            .is_err()
    );
    assert!(
        store.channel_failure_incidents(None).unwrap()[0]
            .acknowledgement
            .is_none()
    );
}

#[test]
fn legacy_failed_dispatch_backfills_only_after_trusted_broker_install() {
    let temp = tempfile::NamedTempFile::new().unwrap();
    let path = temp.path().to_path_buf();
    let security = authority();
    let enrollment_record = broker_enrollment_record(&security);
    let enrollment = broker_enrollment(&security);
    let mut store =
        Store::open_with_trusted_broker(&path, security.clone(), enrollment.clone()).unwrap();
    for revision in 1..=33 {
        commit_runtime(&mut store, revision % 2 == 1, revision);
    }
    store.pair_channel(&channel_pairing()).unwrap();
    store
        .ingest_channel_message(&observation(1), &message_body(1))
        .unwrap();
    store
        .begin_channel_model(ChannelKind::Discord, "message-1")
        .unwrap();
    store
        .fail_channel_model(ChannelKind::Discord, "message-1", 34)
        .unwrap();
    drop(store);
    let connection = Connection::open(&path).unwrap();
    connection
        .execute("DELETE FROM channel_failure_incident", [])
        .unwrap();
    connection
        .execute(
            "DELETE FROM audit_ledger
             WHERE action = 'channel.failure_incident_recorded'",
            [],
        )
        .unwrap();
    drop(connection);

    let mut trustless = Store::open(&path, security).unwrap();
    assert!(trustless.trusted_broker_enrollment().is_none());
    trustless
        .install_trusted_broker(&enrollment_record)
        .unwrap();
    let incidents = trustless.channel_failure_incidents(None).unwrap();
    assert_eq!(incidents.len(), 1);
    assert_eq!(incidents[0].runtime_revision, 33);
    assert!(incidents[0].acknowledgement.is_none());
}

#[test]
fn started_channel_model_is_recovery_visible_then_terminally_unblocks_later_queue() {
    let temp = tempfile::NamedTempFile::new().unwrap();
    let path = temp.path().to_path_buf();
    let security = authority();
    let enrollment = broker_enrollment(&security);
    let mut store =
        Store::open_with_trusted_broker(&path, security.clone(), enrollment.clone()).unwrap();
    commit_runtime(&mut store, true, 1);
    store.pair_channel(&channel_pairing()).unwrap();
    for id in [1, 2] {
        store
            .ingest_channel_message(&observation(id), &message_body(id))
            .unwrap();
    }
    assert_eq!(
        store
            .begin_channel_model(ChannelKind::Discord, "message-1")
            .unwrap()
            .disposition,
        ChannelModelDisposition::ExecuteNow
    );
    drop(store);

    let mut reopened = Store::open_with_trusted_broker(&path, security, enrollment).unwrap();
    assert_eq!(
        reopened
            .started_channel_model(ChannelKind::Discord)
            .unwrap()
            .as_deref(),
        Some("message-1")
    );
    assert_eq!(
        reopened
            .next_queued_channel_model(ChannelKind::Discord)
            .unwrap(),
        None,
        "later queued work must remain closed while recovery surfaces the exact started row"
    );
    assert_eq!(
        reopened
            .begin_channel_model(ChannelKind::Discord, "message-1")
            .unwrap()
            .disposition,
        ChannelModelDisposition::RecoverOnly
    );
    reopened
        .fail_channel_model(ChannelKind::Discord, "message-1", 12)
        .unwrap();
    assert!(
        reopened
            .started_channel_model(ChannelKind::Discord)
            .unwrap()
            .is_none()
    );
    assert_eq!(
        reopened
            .next_queued_channel_model(ChannelKind::Discord)
            .unwrap()
            .as_deref(),
        Some("message-2")
    );
    assert_eq!(
        reopened
            .begin_channel_model(ChannelKind::Discord, "message-2")
            .unwrap()
            .disposition,
        ChannelModelDisposition::ExecuteNow
    );
}

#[test]
fn explicit_model_failure_is_atomic_idempotent_and_never_becomes_a_suggestion() {
    let temp = tempfile::NamedTempFile::new().unwrap();
    let path = temp.path().to_path_buf();
    let security = authority();
    let enrollment = broker_enrollment(&security);
    let mut store = Store::open_with_trusted_broker(&path, security, enrollment).unwrap();
    commit_runtime(&mut store, true, 1);
    store.pair_channel(&channel_pairing()).unwrap();
    store
        .ingest_channel_message(&observation(1), &message_body(1))
        .unwrap();
    assert_eq!(
        store
            .begin_channel_model(ChannelKind::Discord, "message-1")
            .unwrap()
            .disposition,
        ChannelModelDisposition::ExecuteNow
    );
    store
        .fail_channel_model(ChannelKind::Discord, "message-1", 3)
        .unwrap();
    store
        .fail_channel_model(ChannelKind::Discord, "message-1", 4)
        .unwrap();
    let correction = "Correction to previous: keep the same intent but use a three-step checklist";
    let mut correction_observation = observation(2);
    correction_observation.envelope.content_sha256 =
        format!("{:x}", Sha256::digest(correction.as_bytes()));
    store
        .ingest_channel_message(&correction_observation, correction)
        .unwrap();
    assert!(
        store
            .started_channel_model(ChannelKind::Discord)
            .unwrap()
            .is_none()
    );
    assert_eq!(
        store
            .next_queued_channel_model(ChannelKind::Discord)
            .unwrap()
            .as_deref(),
        Some("message-2")
    );
    assert_eq!(
        store
            .begin_channel_model(ChannelKind::Discord, "message-2")
            .unwrap()
            .disposition,
        ChannelModelDisposition::ExecuteNow
    );
    let context = store
        .channel_model_context(ChannelKind::Discord, "message-2")
        .unwrap();
    assert_eq!(context.len(), 2);
    assert_eq!(context[0].0.source_message_id, "message-1");
    assert_eq!(context[1].1, correction);
    assert!(matches!(
        store.record_channel_suggestion(
            ChannelKind::Discord,
            "message-1",
            &OutcomeSuggestion {
                id: "suggestion-must-not-appear".into(),
                title: "Invalid retry".into(),
                why_now: "The consumed call failed.".into(),
                proposed_steps: vec!["Do not publish".into()],
                source_refs: vec!["channel:failed".into()],
            },
            5,
        ),
        Err(StoreError::ChannelObservationConflict)
    ));
    drop(store);
    let failed_audits = Connection::open(&path)
        .unwrap()
        .query_row(
            "SELECT COUNT(*) FROM audit_ledger WHERE action = 'channel.model_failed'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap();
    assert_eq!(failed_audits, 1);
}

#[test]
fn overlapping_unrelated_channel_intents_never_share_model_context() {
    fn custom_observation(id: u64, content: &str) -> ChannelObservation {
        let mut value = observation(id);
        value.envelope.content_sha256 = format!("{:x}", Sha256::digest(content));
        value
    }

    let security = authority();
    let enrollment = broker_enrollment(&security);
    let mut store = Store::open_in_memory_with_trusted_broker(security, enrollment).unwrap();
    commit_runtime(&mut store, true, 1);
    store.pair_channel(&channel_pairing()).unwrap();

    let first_content = "prepare a tax letter";
    let unrelated_content = "book dinner";
    store
        .ingest_channel_message(&custom_observation(1, first_content), first_content)
        .unwrap();
    store
        .ingest_channel_message(&custom_observation(2, unrelated_content), unrelated_content)
        .unwrap();

    assert_eq!(
        store
            .begin_channel_model(ChannelKind::Discord, "message-1")
            .unwrap()
            .disposition,
        ChannelModelDisposition::ExecuteNow
    );
    let first = OutcomeSuggestion {
        id: "suggestion-unrelated-first".into(),
        title: "Prepare the tax letter".into(),
        why_now: "The owner requested it.".into(),
        proposed_steps: vec!["Draft the letter".into()],
        source_refs: vec!["channel:first".into()],
    };
    store
        .record_channel_suggestion(ChannelKind::Discord, "message-1", &first, 3)
        .unwrap();
    assert_eq!(
        store
            .begin_channel_model(ChannelKind::Discord, "message-2")
            .unwrap()
            .disposition,
        ChannelModelDisposition::ExecuteNow
    );
    let context = store
        .channel_model_context(ChannelKind::Discord, "message-2")
        .unwrap();
    assert_eq!(context.len(), 1);
    assert_eq!(context[0].0.source_message_id, "message-2");
    assert_eq!(context[0].1, unrelated_content);

    let second = OutcomeSuggestion {
        id: "suggestion-unrelated-second".into(),
        title: "Book dinner".into(),
        why_now: "The owner requested it separately.".into(),
        proposed_steps: vec!["Choose a restaurant".into()],
        source_refs: vec!["channel:second".into()],
    };
    store
        .record_channel_suggestion(ChannelKind::Discord, "message-2", &second, 4)
        .unwrap();
    let late_correction = "Correction to previous: choose a quieter restaurant";
    store
        .ingest_channel_message(&custom_observation(3, late_correction), late_correction)
        .unwrap();
    assert_eq!(
        store
            .begin_channel_model(ChannelKind::Discord, "message-3")
            .unwrap()
            .disposition,
        ChannelModelDisposition::ExecuteNow
    );
    let late_context = store
        .channel_model_context(ChannelKind::Discord, "message-3")
        .unwrap();
    assert_eq!(late_context.len(), 1);
    assert_eq!(late_context[0].0.source_message_id, "message-3");
    assert_eq!(late_context[0].1, late_correction);
}

#[test]
#[allow(clippy::too_many_lines)] // One chronological serial-dispatch invariant story.
fn channel_model_dispatch_is_serial_and_correction_binds_immediate_predecessor() {
    fn custom_observation(id: u64, content: &str) -> ChannelObservation {
        let mut value = observation(id);
        value.envelope.content_sha256 = format!("{:x}", Sha256::digest(content));
        value
    }

    let security = authority();
    let enrollment = broker_enrollment(&security);
    let mut store = Store::open_in_memory_with_trusted_broker(security, enrollment).unwrap();
    commit_runtime(&mut store, true, 1);
    store.pair_channel(&channel_pairing()).unwrap();

    let first_content = "prepare a tax letter";
    let immediate_content = "book dinner";
    let correction_content = "Correction to previous: choose a quiet restaurant";
    for (id, content) in [
        (1, first_content),
        (2, immediate_content),
        (3, correction_content),
    ] {
        store
            .ingest_channel_message(&custom_observation(id, content), content)
            .unwrap();
    }

    assert_eq!(
        store
            .begin_channel_model(ChannelKind::Discord, "message-1")
            .unwrap()
            .disposition,
        ChannelModelDisposition::ExecuteNow
    );
    store
        .record_channel_suggestion(
            ChannelKind::Discord,
            "message-1",
            &OutcomeSuggestion {
                id: "suggestion-serial-first".into(),
                title: "Prepare a tax letter".into(),
                why_now: "The owner requested it.".into(),
                proposed_steps: vec!["Draft the letter".into()],
                source_refs: vec!["channel:serial-first".into()],
            },
            4,
        )
        .unwrap();
    assert_eq!(
        store
            .begin_channel_model(ChannelKind::Discord, "message-2")
            .unwrap()
            .disposition,
        ChannelModelDisposition::ExecuteNow
    );
    assert_eq!(
        store
            .next_queued_channel_model(ChannelKind::Discord)
            .unwrap(),
        None,
        "a started dispatch must serialize all later work on the channel"
    );
    assert!(matches!(
        store.begin_channel_model(ChannelKind::Discord, "message-3"),
        Err(StoreError::ChannelObservationConflict)
    ));

    store
        .record_channel_suggestion(
            ChannelKind::Discord,
            "message-2",
            &OutcomeSuggestion {
                id: "suggestion-serial-immediate".into(),
                title: "Book dinner".into(),
                why_now: "The owner requested it separately.".into(),
                proposed_steps: vec!["Choose a restaurant".into()],
                source_refs: vec!["channel:serial-immediate".into()],
            },
            5,
        )
        .unwrap();
    assert_eq!(
        store
            .next_queued_channel_model(ChannelKind::Discord)
            .unwrap()
            .as_deref(),
        Some("message-3")
    );
    assert_eq!(
        store
            .begin_channel_model(ChannelKind::Discord, "message-3")
            .unwrap()
            .disposition,
        ChannelModelDisposition::ExecuteNow
    );
    let context = store
        .channel_model_context(ChannelKind::Discord, "message-3")
        .unwrap();
    assert_eq!(
        context
            .iter()
            .map(|(envelope, content)| (envelope.source_message_id.as_str(), content.as_str()))
            .collect::<Vec<_>>(),
        vec![
            ("message-2", immediate_content),
            ("message-3", correction_content),
        ]
    );
}

#[test]
fn accepted_observation_cursor_and_model_queue_commit_atomically_before_claim() {
    let temp = tempfile::NamedTempFile::new().unwrap();
    let path = temp.path().to_path_buf();
    let security = authority();
    let enrollment = broker_enrollment(&security);
    let mut store =
        Store::open_with_trusted_broker(&path, security.clone(), enrollment.clone()).unwrap();
    commit_runtime(&mut store, true, 1);
    store.pair_channel(&channel_pairing()).unwrap();
    store
        .ingest_channel_message(&observation(1), &message_body(1))
        .unwrap();
    assert_eq!(
        store
            .next_queued_channel_model(ChannelKind::Discord)
            .unwrap()
            .as_deref(),
        Some("message-1")
    );
    drop(store);

    let mut reopened = Store::open_with_trusted_broker(&path, security, enrollment).unwrap();
    assert_eq!(
        reopened
            .channel_cursor(ChannelKind::Discord, "channel-1")
            .unwrap(),
        Some(observation(1).cursor)
    );
    assert_eq!(
        reopened
            .next_queued_channel_model(ChannelKind::Discord)
            .unwrap()
            .as_deref(),
        Some("message-1")
    );
    assert_eq!(
        reopened
            .begin_channel_model(ChannelKind::Discord, "message-1")
            .unwrap()
            .disposition,
        ChannelModelDisposition::ExecuteNow
    );
    assert_eq!(
        reopened
            .next_queued_channel_model(ChannelKind::Discord)
            .unwrap(),
        None
    );
    assert_eq!(
        reopened
            .begin_channel_model(ChannelKind::Discord, "message-1")
            .unwrap()
            .disposition,
        ChannelModelDisposition::RecoverOnly
    );
}

#[test]
fn mission_and_channel_model_claim_race_has_exactly_one_transactional_winner() {
    let mut mission_first = effect_store_in_memory(authority());
    let active = persist_active(&mut mission_first, "mission-1");
    mission_first.pair_channel(&channel_pairing()).unwrap();
    mission_first
        .ingest_channel_message(&observation(1), &message_body(1))
        .unwrap();
    let before_deferred_claim = mission_first
        .current_verified_audit_anchor()
        .unwrap()
        .unwrap();
    assert!(matches!(
        mission_first.begin_channel_model(ChannelKind::Discord, "message-1"),
        Err(StoreError::ChannelModelDeferredByMission)
    ));
    assert_eq!(
        mission_first
            .current_verified_audit_anchor()
            .unwrap()
            .unwrap(),
        before_deferred_claim
    );
    let cancelled = execute(
        &mut mission_first,
        "cancel-mission-before-queued-claim",
        Some(before_deferred_claim),
        MissionCommand::Cancel {
            mission_id: active.mission.id,
            now_ms: 6,
        },
    )
    .unwrap();
    assert_eq!(cancelled.mission.status, MissionStatus::Cancelled);
    assert_eq!(
        mission_first
            .begin_channel_model(ChannelKind::Discord, "message-1")
            .unwrap()
            .disposition,
        ChannelModelDisposition::ExecuteNow
    );

    let mut claim_first = effect_store_in_memory(authority());
    claim_first.pair_channel(&channel_pairing()).unwrap();
    claim_first
        .ingest_channel_message(&observation(1), &message_body(1))
        .unwrap();
    assert_eq!(
        claim_first
            .begin_channel_model(ChannelKind::Discord, "message-1")
            .unwrap()
            .disposition,
        ChannelModelDisposition::ExecuteNow
    );
    let before_rejected_mission = claim_first
        .current_verified_audit_anchor()
        .unwrap()
        .unwrap();
    assert!(matches!(
        execute(
            &mut claim_first,
            "mission-create-during-started-channel-model",
            Some(before_rejected_mission.clone()),
            create_command("mission-2", 2),
        ),
        Err(StoreError::MissionModelInFlight)
    ));
    assert_eq!(
        claim_first
            .current_verified_audit_anchor()
            .unwrap()
            .unwrap(),
        before_rejected_mission
    );
    claim_first
        .fail_channel_model(ChannelKind::Discord, "message-1", 3)
        .unwrap();
    let after_terminal_dispatch = claim_first.current_verified_audit_anchor().unwrap();
    assert_eq!(
        execute(
            &mut claim_first,
            "mission-create-after-terminal-channel-model",
            after_terminal_dispatch,
            create_command("mission-2", 4),
        )
        .unwrap()
        .mission
        .status,
        MissionStatus::Proposed
    );
}

#[test]
fn mission_bound_inbound_is_a_typed_participation_and_never_a_free_outcome() {
    let mut store = effect_store_in_memory(authority());
    persist_channel_active(&mut store);
    let accepted = store
        .ingest_channel_message(&observation(2), &message_body(2))
        .unwrap();
    assert_eq!(
        accepted.decision,
        ChannelInboundDecision::AcceptedMissionUpdate
    );
    let event = accepted.mission_event.unwrap();
    assert_eq!(event.mission_id, "mission-1");
    assert_eq!(
        event.message_class,
        ChannelInboundMessageClass::MissionParticipation
    );
    assert!(event.mission_revision > 0);
    assert_eq!(event.route_set_revision, 1);
    let anchor = store.current_verified_audit_anchor().unwrap().unwrap();
    let mission = store.get_mission("mission-1", &anchor).unwrap().unwrap();
    assert_eq!(mission.status, MissionStatus::Active);
    assert!(mission.evidence.is_empty());
    assert_eq!(mission.updated_at_ms, 5);
    assert_eq!(
        store
            .next_queued_channel_model(ChannelKind::Discord)
            .unwrap(),
        None
    );
    assert_eq!(
        store
            .channel_mission_event(ChannelKind::Discord, "message-2")
            .unwrap(),
        Some(event.clone())
    );
    let duplicate = store
        .ingest_channel_message(&observation(2), &message_body(2))
        .unwrap();
    assert_eq!(duplicate.decision, ChannelInboundDecision::Duplicate);
    assert_eq!(duplicate.mission_event, Some(event));
}

#[test]
fn need_you_route_event_advances_only_the_mission_revision_without_granting_authority() {
    let mut store = effect_store_in_memory(authority());
    let active = persist_channel_active(&mut store);
    let needs = execute(
        &mut store,
        "route-need-you",
        Some(active.anchor),
        MissionCommand::RequestScopeChange {
            mission_id: "mission-1".into(),
            approval: NewBoundaryApproval {
                id: "route-boundary".into(),
                kind: ApprovalKind::NewRecipient,
                prompt: "Approve the exact recipient?".into(),
                scope_digest: "recipient-v1".into(),
                target: None,
            },
            needs_me_id: "route-needs-you".into(),
            now_ms: 6,
        },
    )
    .unwrap();
    let before = needs.anchor;
    let accepted = store
        .ingest_channel_message(&observation(7), &message_body(7))
        .unwrap();
    let event = accepted.mission_event.unwrap();
    assert_eq!(
        event.message_class,
        ChannelInboundMessageClass::NeedYouResponse
    );
    let after = store.current_verified_audit_anchor().unwrap().unwrap();
    assert!(after.sequence > before.sequence);
    let mission = store.get_mission("mission-1", &after).unwrap().unwrap();
    assert_eq!(mission.status, MissionStatus::NeedsMe);
    assert_eq!(mission.updated_at_ms, 7);
    assert!(mission.evidence.is_empty());
    let approval = mission
        .approvals
        .iter()
        .find(|approval| approval.id == "route-boundary")
        .unwrap();
    assert_eq!(approval.status, ApprovalStatus::Pending);
    assert_eq!(approval.decided_by_id, None);
}

#[test]
fn terminal_mission_route_releases_the_conversation_for_a_new_explicit_outcome() {
    let mut store = effect_store_in_memory(authority());
    let active = persist_channel_active(&mut store);
    let cancelled = execute(
        &mut store,
        "cancel-routed-mission",
        Some(active.anchor),
        MissionCommand::Cancel {
            mission_id: "mission-1".into(),
            now_ms: 6,
        },
    )
    .unwrap();
    assert_eq!(cancelled.mission.status, MissionStatus::Cancelled);
    let accepted = store
        .ingest_channel_message(&observation(7), &message_body(7))
        .unwrap();
    assert_eq!(accepted.decision, ChannelInboundDecision::Accepted);
    assert_eq!(accepted.mission_event, None);
    assert_eq!(
        store
            .begin_channel_model(ChannelKind::Discord, "message-7")
            .unwrap()
            .disposition,
        ChannelModelDisposition::ExecuteNow
    );
}

#[test]
fn caller_cannot_fabricate_a_channel_participation_command_without_a_durable_event() {
    let mut store = effect_store_in_memory(authority());
    let active = persist_channel_active(&mut store);
    let event = ChannelMissionEvent {
        event_id: "channel-event-forged".into(),
        mission_id: "mission-1".into(),
        mission_revision: active.anchor.sequence,
        mission_anchor_hash: active.anchor.entry_hash.clone(),
        route_id: primary_route(&store).0,
        route_set_revision: 1,
        message_class: ChannelInboundMessageClass::MissionParticipation,
        channel: ChannelKind::Discord,
        source_message_id: "missing-message".into(),
        content_sha256: "aa".repeat(32),
        recorded_at_ms: 6,
    };
    let before = store.current_verified_audit_anchor().unwrap().unwrap();
    assert!(matches!(
        execute(
            &mut store,
            "channel-event-forged:mission",
            Some(before.clone()),
            MissionCommand::RecordChannelParticipation {
                mission_id: "mission-1".into(),
                event,
            },
        ),
        Err(StoreError::ChannelRouteConflict)
    ));
    assert_eq!(store.current_verified_audit_anchor().unwrap(), Some(before));
}

fn ingest_route_event_twice(
    store: &mut Store,
    observation: &ChannelObservation,
    content: &str,
) -> bool {
    let first = store.ingest_channel_message(observation, content).unwrap();
    let retry = store.ingest_channel_message(observation, content).unwrap();
    if first.decision == ChannelInboundDecision::AcceptedMissionUpdate {
        assert_eq!(retry.decision, ChannelInboundDecision::Duplicate);
        true
    } else {
        assert_eq!(first.decision, ChannelInboundDecision::IgnoredStaleCursor);
        assert_eq!(retry.decision, ChannelInboundDecision::IgnoredStaleCursor);
        false
    }
}

fn bind_imessage_stress_route(store: &mut Store) {
    store.pair_channel(&imessage_pairing()).unwrap();
    store
        .bind_additional_channel_route(&ChannelRouteApproval {
            approval_id: "route-approval-imessage-stress".into(),
            mission_id: "mission-1".into(),
            expected_route_set_revision: 1,
            channel: ChannelKind::IMessage,
            conversation_id: "chat-imessage".into(),
            owner_sender_id: "owner-imessage".into(),
            provider_identity: None,
            allowed_inbound_classes: vec![
                ChannelInboundMessageClass::MissionParticipation,
                ChannelInboundMessageClass::NeedYouResponse,
            ],
            allowed_outbound_classes: Vec::new(),
            actor_id: "owner-1".into(),
            decision: ChannelRouteApprovalDecision::Approve,
            decided_at_ms: 7,
        })
        .unwrap();
}

fn persist_numbered_active_mission(store: &mut Store, index: usize) {
    let mission_id = format!("mission-{index}");
    let commands = vec![
        create_command(&mission_id, 20 + i64::try_from(index).unwrap()),
        MissionCommand::BeginConfirmation {
            mission_id: mission_id.clone(),
            now_ms: 40 + i64::try_from(index).unwrap(),
        },
        MissionCommand::DecideApproval {
            mission_id: mission_id.clone(),
            approval_id: format!("scope-{mission_id}"),
            actor_id: "owner-1".into(),
            decision: ApprovalDecision::Approve,
            now_ms: 60 + i64::try_from(index).unwrap(),
        },
        MissionCommand::Activate {
            mission_id: mission_id.clone(),
            now_ms: 80 + i64::try_from(index).unwrap(),
        },
        MissionCommand::TransitionWorkItem {
            mission_id,
            work_item_id: "work-1".into(),
            next: WorkItemStatus::Active,
            evidence_ids: Vec::new(),
            now_ms: 100 + i64::try_from(index).unwrap(),
        },
    ];
    let expected_anchor = store.current_verified_audit_anchor().unwrap();
    let envelopes = commands
        .into_iter()
        .enumerate()
        .map(|(command_index, command)| MissionCommandEnvelope {
            command_id: format!("mission-{index}-active-{command_index}"),
            expected_anchor: (command_index == 0)
                .then(|| expected_anchor.clone())
                .flatten(),
            command,
        })
        .collect::<Vec<_>>();
    let results = store.execute_mission_command_batch(&envelopes).unwrap();
    assert_eq!(
        results.last().unwrap().mission.status,
        MissionStatus::Active
    );
}

#[test]
fn exact_create_batch_retry_precedes_a_later_started_channel_model_gate() {
    let mut store = effect_store_in_memory(authority());
    let create = MissionCommandEnvelope {
        command_id: "mission-create-before-later-dispatch".into(),
        expected_anchor: store.current_verified_audit_anchor().unwrap(),
        command: create_command("mission-retry", 2),
    };
    let original = store
        .execute_mission_command_batch(std::slice::from_ref(&create))
        .unwrap();
    let cancelled = execute(
        &mut store,
        "cancel-before-later-dispatch",
        Some(original[0].anchor.clone()),
        MissionCommand::Cancel {
            mission_id: "mission-retry".into(),
            now_ms: 3,
        },
    )
    .unwrap();
    assert_eq!(cancelled.mission.status, MissionStatus::Cancelled);

    store.pair_channel(&channel_pairing()).unwrap();
    store
        .ingest_channel_message(&observation(1), &message_body(1))
        .unwrap();
    assert_eq!(
        store
            .begin_channel_model(ChannelKind::Discord, "message-1")
            .unwrap()
            .disposition,
        ChannelModelDisposition::ExecuteNow
    );
    let before_retry = store.current_verified_audit_anchor().unwrap().unwrap();

    assert_eq!(
        store
            .execute_mission_command_batch(std::slice::from_ref(&create))
            .unwrap(),
        original,
        "an exact persisted response-loss retry must win before the new-creation race gate"
    );
    assert_eq!(
        store.current_verified_audit_anchor().unwrap().unwrap(),
        before_retry,
        "an exact retry must remain read-only"
    );
    assert_eq!(
        store
            .begin_channel_model(ChannelKind::Discord, "message-1")
            .unwrap()
            .disposition,
        ChannelModelDisposition::RecoverOnly,
        "the later started dispatch must remain untouched"
    );
}

#[test]
fn ten_concurrent_missions_reject_cross_route_recipient_and_revision_leakage() {
    let mut store = effect_store_in_memory(authority());
    persist_channel_active(&mut store);
    bind_imessage_stress_route(&mut store);
    for index in 2..=10 {
        persist_numbered_active_mission(&mut store, index);
    }

    let routed = store.channel_route_set("mission-1").unwrap().unwrap();
    assert_eq!(routed.revision, 2);
    assert_eq!(routed.routes.len(), 2);
    for index in 2..=10 {
        let mission_id = format!("mission-{index}");
        let before = store.current_verified_audit_anchor().unwrap().unwrap();
        let rejected = store.bind_additional_channel_route(&ChannelRouteApproval {
            approval_id: format!("cross-route-approval-{index}"),
            mission_id: mission_id.clone(),
            expected_route_set_revision: routed.revision,
            channel: ChannelKind::IMessage,
            conversation_id: "chat-imessage".into(),
            owner_sender_id: "owner-imessage".into(),
            provider_identity: None,
            allowed_inbound_classes: vec![ChannelInboundMessageClass::MissionParticipation],
            allowed_outbound_classes: vec![ChannelMessageKind::Receipt],
            actor_id: "owner-1".into(),
            decision: ChannelRouteApprovalDecision::Approve,
            decided_at_ms: 200 + i64::from(index),
        });
        assert!(matches!(rejected, Err(StoreError::ChannelRouteConflict)));
        assert_eq!(store.current_verified_audit_anchor().unwrap(), Some(before));
        assert_eq!(store.channel_route_set(&mission_id).unwrap(), None);
    }

    let before_wrong_owner = store.current_verified_audit_anchor().unwrap().unwrap();
    let wrong_owner = store.bind_additional_channel_route(&ChannelRouteApproval {
        approval_id: "wrong-owner-route-approval".into(),
        mission_id: "mission-1".into(),
        expected_route_set_revision: routed.revision,
        channel: ChannelKind::IMessage,
        conversation_id: "chat-imessage".into(),
        owner_sender_id: "owner-imessage".into(),
        provider_identity: None,
        allowed_inbound_classes: vec![ChannelInboundMessageClass::MissionParticipation],
        allowed_outbound_classes: Vec::new(),
        actor_id: "owner-2".into(),
        decision: ChannelRouteApprovalDecision::Approve,
        decided_at_ms: 220,
    });
    assert!(matches!(wrong_owner, Err(StoreError::ChannelRouteConflict)));
    assert_eq!(
        store.current_verified_audit_anchor().unwrap(),
        Some(before_wrong_owner)
    );

    let discord = store
        .ingest_channel_message(&observation(500), &message_body(500))
        .unwrap();
    let imessage = store
        .ingest_channel_message(&imessage_observation(500), &imessage_body(500))
        .unwrap();
    assert_eq!(
        discord.decision,
        ChannelInboundDecision::AcceptedMissionUpdate
    );
    assert_eq!(
        imessage.decision,
        ChannelInboundDecision::AcceptedMissionUpdate
    );
    assert_eq!(discord.mission_event.unwrap().mission_id, "mission-1");
    assert_eq!(imessage.mission_event.unwrap().mission_id, "mission-1");

    let mut crossed_recipient = imessage_observation(501);
    crossed_recipient.envelope.conversation_id = "channel-1".into();
    crossed_recipient.cursor.conversation_id = "channel-1".into();
    let before_crossed_recipient = store.current_verified_audit_anchor().unwrap().unwrap();
    let ignored = store
        .ingest_channel_message(&crossed_recipient, &imessage_body(501))
        .unwrap();
    assert_eq!(
        ignored.decision,
        ChannelInboundDecision::IgnoredConversation
    );
    assert_eq!(ignored.mission_event, None);
    assert_eq!(
        store.current_verified_audit_anchor().unwrap(),
        Some(before_crossed_recipient)
    );

    let final_anchor = store.current_verified_audit_anchor().unwrap().unwrap();
    let missions = store.list_missions(&final_anchor).unwrap();
    assert_eq!(missions.len(), 10);
    assert!(
        missions.iter().all(|mission| {
            mission.status == MissionStatus::Active && mission.evidence.is_empty()
        })
    );
    assert!(store.list_receipts(&final_anchor).unwrap().is_empty());
    assert_eq!(store.channel_route_set("mission-1").unwrap(), Some(routed));
}

#[test]
fn one_hundred_out_of_order_duplicate_events_across_both_routes_survive_restart() {
    let database = tempfile::NamedTempFile::new().unwrap();
    let security = authority();
    let enrollment = broker_enrollment(&security);
    let mut store =
        Store::open_with_trusted_broker(database.path(), security.clone(), enrollment.clone())
            .unwrap();
    commit_runtime(&mut store, true, 1);
    persist_channel_active(&mut store);
    bind_imessage_stress_route(&mut store);

    let mut discord_orders = (2_u64..=51).collect::<Vec<_>>();
    let mut imessage_orders = (1_u64..=50).collect::<Vec<_>>();
    for pair in discord_orders.chunks_exact_mut(2) {
        pair.swap(0, 1);
    }
    for pair in imessage_orders.chunks_exact_mut(2) {
        pair.swap(0, 1);
    }
    let mut accepted = Vec::new();
    for index in 0..50 {
        let discord_id = discord_orders[index];
        if ingest_route_event_twice(
            &mut store,
            &observation(discord_id),
            &message_body(discord_id),
        ) {
            accepted.push((ChannelKind::Discord, format!("message-{discord_id}")));
        }

        let imessage_id = imessage_orders[index];
        if ingest_route_event_twice(
            &mut store,
            &imessage_observation(imessage_id),
            &imessage_body(imessage_id),
        ) {
            accepted.push((ChannelKind::IMessage, format!("imessage-{imessage_id}")));
        }
    }
    assert_eq!(accepted.len(), 50);
    assert_eq!(
        store
            .next_queued_channel_model(ChannelKind::Discord)
            .unwrap(),
        None
    );
    assert_eq!(
        store
            .next_queued_channel_model(ChannelKind::IMessage)
            .unwrap(),
        None
    );
    let before_restart = store.current_verified_audit_anchor().unwrap().unwrap();
    drop(store);

    let mut restarted =
        Store::open_with_trusted_broker(database.path(), security, enrollment).unwrap();
    for (channel, source_message_id) in accepted {
        let (observation, content) = match channel {
            ChannelKind::Discord => {
                let id = source_message_id
                    .strip_prefix("message-")
                    .unwrap()
                    .parse::<u64>()
                    .unwrap();
                (observation(id), message_body(id))
            }
            ChannelKind::IMessage => {
                let id = source_message_id
                    .strip_prefix("imessage-")
                    .unwrap()
                    .parse::<u64>()
                    .unwrap();
                (imessage_observation(id), imessage_body(id))
            }
        };
        let duplicate = restarted
            .ingest_channel_message(&observation, &content)
            .unwrap();
        assert_eq!(duplicate.decision, ChannelInboundDecision::Duplicate);
        assert_eq!(
            duplicate.mission_event.unwrap().source_message_id,
            source_message_id
        );
    }
    assert_eq!(
        restarted.current_verified_audit_anchor().unwrap().unwrap(),
        before_restart
    );
}

#[test]
fn additional_route_requires_exact_owner_pairing_revision_and_defaults_outbound_off() {
    let mut store = effect_store_in_memory(authority());
    persist_channel_active(&mut store);
    store.pair_channel(&imessage_pairing()).unwrap();
    let approval = ChannelRouteApproval {
        approval_id: "route-approval-imessage-1".into(),
        mission_id: "mission-1".into(),
        expected_route_set_revision: 1,
        channel: ChannelKind::IMessage,
        conversation_id: "chat-imessage".into(),
        owner_sender_id: "owner-imessage".into(),
        provider_identity: None,
        allowed_inbound_classes: vec![
            ChannelInboundMessageClass::MissionParticipation,
            ChannelInboundMessageClass::NeedYouResponse,
        ],
        allowed_outbound_classes: Vec::new(),
        actor_id: "owner-1".into(),
        decision: ChannelRouteApprovalDecision::Approve,
        decided_at_ms: 7,
    };
    let route_set = store.bind_additional_channel_route(&approval).unwrap();
    assert_eq!(route_set.revision, 2);
    assert_eq!(route_set.routes.len(), 2);
    assert_eq!(route_set.routes[0].role, ChannelRouteRole::Primary);
    let additional = &route_set.routes[1];
    assert_eq!(additional.role, ChannelRouteRole::Additional);
    assert!(additional.allowed_outbound_classes.is_empty());
    assert_eq!(
        store.bind_additional_channel_route(&approval).unwrap(),
        route_set
    );

    let accepted = store
        .ingest_channel_message(&imessage_observation(1), &imessage_body(1))
        .unwrap();
    assert_eq!(
        accepted.decision,
        ChannelInboundDecision::AcceptedMissionUpdate
    );
    assert_eq!(
        accepted.mission_event.unwrap().route_id,
        additional.route_id
    );
    assert_eq!(
        store
            .next_queued_channel_model(ChannelKind::IMessage)
            .unwrap(),
        None
    );

    let anchor = store.current_verified_audit_anchor().unwrap().unwrap();
    let mut changed = approval.clone();
    changed.allowed_outbound_classes = vec![ChannelMessageKind::Progress];
    assert!(matches!(
        store.bind_additional_channel_route(&changed),
        Err(StoreError::ChannelRouteConflict)
    ));
    assert_eq!(
        store.current_verified_audit_anchor().unwrap().unwrap(),
        anchor
    );
}

#[test]
fn stale_revision_changed_recipient_wrong_owner_and_off_route_bind_fail_closed() {
    let mut store = effect_store_in_memory(authority());
    persist_channel_active(&mut store);
    store.pair_channel(&imessage_pairing()).unwrap();
    let base = ChannelRouteApproval {
        approval_id: "route-approval-imessage-1".into(),
        mission_id: "mission-1".into(),
        expected_route_set_revision: 1,
        channel: ChannelKind::IMessage,
        conversation_id: "chat-imessage".into(),
        owner_sender_id: "owner-imessage".into(),
        provider_identity: None,
        allowed_inbound_classes: vec![ChannelInboundMessageClass::MissionParticipation],
        allowed_outbound_classes: Vec::new(),
        actor_id: "owner-1".into(),
        decision: ChannelRouteApprovalDecision::Approve,
        decided_at_ms: 7,
    };
    let anchor = store.current_verified_audit_anchor().unwrap().unwrap();
    let mut wrong_owner = base.clone();
    wrong_owner.actor_id = "attacker".into();
    assert!(matches!(
        store.bind_additional_channel_route(&wrong_owner),
        Err(StoreError::ChannelRouteConflict)
    ));
    assert_eq!(
        store.current_verified_audit_anchor().unwrap().unwrap(),
        anchor
    );
    let mut stale_revision = base.clone();
    stale_revision.expected_route_set_revision = 2;
    assert!(matches!(
        store.bind_additional_channel_route(&stale_revision),
        Err(StoreError::ChannelRouteConflict)
    ));
    assert_eq!(
        store.current_verified_audit_anchor().unwrap().unwrap(),
        anchor
    );
    let mut changed_recipient = base.clone();
    changed_recipient.owner_sender_id = "other-recipient".into();
    assert!(matches!(
        store.bind_additional_channel_route(&changed_recipient),
        Err(StoreError::ChannelRouteConflict)
    ));
    assert_eq!(
        store.current_verified_audit_anchor().unwrap().unwrap(),
        anchor
    );
    let mut rejected = base.clone();
    rejected.decision = ChannelRouteApprovalDecision::Reject;
    assert!(matches!(
        store.bind_additional_channel_route(&rejected),
        Err(StoreError::ChannelRouteConflict)
    ));
    assert_eq!(
        store.current_verified_audit_anchor().unwrap().unwrap(),
        anchor
    );
    commit_runtime(&mut store, false, 8);
    let mut approved = rejected;
    approved.decision = ChannelRouteApprovalDecision::Approve;
    assert!(matches!(
        store.bind_additional_channel_route(&approved),
        Err(StoreError::RuntimeDisabled)
    ));
}

#[test]
fn outbound_authority_is_consumed_once_and_restart_is_recovery_only() {
    let temp = tempfile::NamedTempFile::new().unwrap();
    let path = temp.path().to_path_buf();
    let security = authority();
    let enrollment = broker_enrollment(&security);
    let content = "OpenOpen · AI\nMission progress is ready.".as_bytes();
    let mut store =
        Store::open_with_trusted_broker(&path, security.clone(), enrollment.clone()).unwrap();
    commit_runtime(&mut store, true, 1);
    let active = persist_channel_active(&mut store);
    persist_channel_send_approvals(&mut store, &active, content);
    let (route_id, route_set_revision) = primary_route(&store);
    let intent = ChannelOutboundIntent {
        outbound_id: "channel-send-1".into(),
        mission_id: "mission-1".into(),
        route_id,
        route_set_revision,
        channel: ChannelKind::Discord,
        conversation_id: "channel-1".into(),
        recipient_id: "owner-1".into(),
        kind: ChannelMessageKind::Progress,
        content_sha256: format!("{:x}", Sha256::digest(content)),
        created_at_ms: 13,
        recovery_cursor: Some(observation(1).cursor),
    };
    let before_wrong_recipient = store.current_verified_audit_anchor().unwrap().unwrap();
    let mut wrong_recipient = intent.clone();
    wrong_recipient.recipient_id = "other-recipient".into();
    assert!(matches!(
        store.begin_channel_outbound(&wrong_recipient, content),
        Err(StoreError::ChannelRouteConflict)
    ));
    assert_eq!(
        store.current_verified_audit_anchor().unwrap().unwrap(),
        before_wrong_recipient
    );
    assert_eq!(
        store
            .begin_channel_outbound(&intent, content)
            .unwrap()
            .disposition,
        ChannelOutboundDisposition::ExecuteNow
    );
    drop(store);

    let mut reopened = Store::open_with_trusted_broker(&path, security, enrollment).unwrap();
    assert_eq!(
        reopened
            .begin_channel_outbound(&intent, content)
            .unwrap()
            .disposition,
        ChannelOutboundDisposition::RecoverOnly
    );
    commit_runtime(&mut reopened, false, 2);
    assert!(matches!(
        reopened.begin_channel_outbound(&intent, content),
        Err(StoreError::RuntimeDisabled)
    ));
    let delivery = ChannelDeliveryReceipt {
        outbound_id: intent.outbound_id.clone(),
        provider_message_id: "provider-message-1".into(),
        delivered_at_ms: 14,
    };
    assert_eq!(
        reopened
            .record_channel_delivery(&delivery)
            .unwrap()
            .disposition,
        ChannelOutboundDisposition::AlreadySent
    );
    assert_eq!(
        reopened
            .record_channel_delivery(&delivery)
            .unwrap()
            .disposition,
        ChannelOutboundDisposition::AlreadySent
    );
    commit_runtime(&mut reopened, true, 3);
    let already_sent = reopened.begin_channel_outbound(&intent, content).unwrap();
    assert_eq!(
        already_sent.disposition,
        ChannelOutboundDisposition::AlreadySent
    );
    assert_eq!(
        already_sent.provider_message_id.as_deref(),
        Some("provider-message-1")
    );
}

#[test]
fn outbound_response_loss_survives_unrelated_route_set_append_without_duplicate_authority() {
    let mut store = effect_store_in_memory(authority());
    let content = "OpenOpen · AI\nMission progress is ready.".as_bytes();
    let active = persist_channel_active(&mut store);
    persist_channel_send_approvals(&mut store, &active, content);
    let (route_id, route_set_revision) = primary_route(&store);
    let original = ChannelOutboundIntent {
        outbound_id: "channel-send-before-route-append".into(),
        mission_id: "mission-1".into(),
        route_id,
        route_set_revision,
        channel: ChannelKind::Discord,
        conversation_id: "channel-1".into(),
        recipient_id: "owner-1".into(),
        kind: ChannelMessageKind::Progress,
        content_sha256: format!("{:x}", Sha256::digest(content)),
        created_at_ms: 13,
        recovery_cursor: Some(observation(1).cursor),
    };
    assert_eq!(
        store
            .begin_channel_outbound(&original, content)
            .unwrap()
            .disposition,
        ChannelOutboundDisposition::ExecuteNow
    );

    store.pair_channel(&imessage_pairing()).unwrap();
    let appended = store
        .bind_additional_channel_route(&ChannelRouteApproval {
            approval_id: "route-approval-after-response-loss".into(),
            mission_id: "mission-1".into(),
            expected_route_set_revision: 1,
            channel: ChannelKind::IMessage,
            conversation_id: "chat-imessage".into(),
            owner_sender_id: "owner-imessage".into(),
            provider_identity: None,
            allowed_inbound_classes: vec![ChannelInboundMessageClass::MissionParticipation],
            allowed_outbound_classes: vec![ChannelMessageKind::Progress],
            actor_id: "owner-1".into(),
            decision: ChannelRouteApprovalDecision::Approve,
            decided_at_ms: 14,
        })
        .unwrap();
    assert_eq!(appended.revision, 2);

    let mut retried = original.clone();
    retried.outbound_id = "fresh-id-from-current-route-set-revision".into();
    retried.route_set_revision = appended.revision;
    retried.created_at_ms = 15;
    retried.recovery_cursor = Some(observation(2).cursor);
    let recovered = store.begin_channel_outbound(&retried, content).unwrap();
    assert_eq!(
        recovered.disposition,
        ChannelOutboundDisposition::RecoverOnly
    );
    assert_eq!(recovered.intent, original);

    let additional = appended
        .routes
        .iter()
        .find(|route| route.channel == ChannelKind::IMessage)
        .unwrap();
    let different_route = ChannelOutboundIntent {
        outbound_id: "genuinely-different-route".into(),
        mission_id: "mission-1".into(),
        route_id: additional.route_id.clone(),
        route_set_revision: appended.revision,
        channel: ChannelKind::IMessage,
        conversation_id: additional.conversation_id.clone(),
        recipient_id: additional.owner_sender_id.clone(),
        kind: ChannelMessageKind::Progress,
        content_sha256: format!("{:x}", Sha256::digest(content)),
        created_at_ms: 15,
        recovery_cursor: None,
    };
    assert_eq!(
        store
            .recover_channel_outbound(&different_route, content)
            .unwrap(),
        None
    );
}

#[test]
fn outbound_changed_payload_target_or_provider_result_conflicts() {
    let mut store = effect_store_in_memory(authority());
    let content = "OpenOpen · AI\nExact progress".as_bytes();
    let active = persist_channel_active(&mut store);
    persist_channel_send_approvals(&mut store, &active, content);
    let (route_id, route_set_revision) = primary_route(&store);
    let intent = ChannelOutboundIntent {
        outbound_id: "channel-send-1".into(),
        mission_id: "mission-1".into(),
        route_id,
        route_set_revision,
        channel: ChannelKind::Discord,
        conversation_id: "channel-1".into(),
        recipient_id: "owner-1".into(),
        kind: ChannelMessageKind::Progress,
        content_sha256: format!("{:x}", Sha256::digest(content)),
        created_at_ms: 13,
        recovery_cursor: Some(observation(1).cursor),
    };
    store.begin_channel_outbound(&intent, content).unwrap();
    let mut changed = intent.clone();
    changed.conversation_id = "other-channel".into();
    changed.recovery_cursor.as_mut().unwrap().conversation_id = "other-channel".into();
    let changed_result = store.begin_channel_outbound(&changed, content);
    assert!(
        matches!(changed_result, Err(StoreError::ChannelOutboundConflict)),
        "unexpected changed-target result: {changed_result:?}"
    );
    let delivery = ChannelDeliveryReceipt {
        outbound_id: intent.outbound_id.clone(),
        provider_message_id: "provider-1".into(),
        delivered_at_ms: 14,
    };
    store.record_channel_delivery(&delivery).unwrap();
    let mut conflicting_delivery = delivery;
    conflicting_delivery.provider_message_id = "provider-2".into();
    assert!(matches!(
        store.record_channel_delivery(&conflicting_delivery),
        Err(StoreError::ChannelOutboundConflict)
    ));
}

#[test]
fn need_you_outbound_accepts_only_the_current_exact_boundary_prompt() {
    let mut store = effect_store_in_memory(authority());
    let active = persist_channel_active(&mut store);
    let requested = advance(
        &mut store,
        "need-you-request",
        &active,
        MissionCommand::RequestScopeChange {
            mission_id: "mission-1".into(),
            approval: NewBoundaryApproval {
                id: "need-you-approval".into(),
                kind: ApprovalKind::ExpandedScope,
                prompt: "Choose the one approved destination.".into(),
                scope_digest: "expanded-scope".into(),
                target: None,
            },
            needs_me_id: "need-you-1".into(),
            now_ms: 6,
        },
    );
    let needs_me = requested.mission.needs_me.as_ref().unwrap();
    let content =
        channel_message_payload(ChannelKind::Discord, &channel_need_you_content(needs_me));
    let (route_id, route_set_revision) = primary_route(&store);
    let intent = ChannelOutboundIntent {
        outbound_id: "need-you-send-1".into(),
        mission_id: "mission-1".into(),
        route_id,
        route_set_revision,
        channel: ChannelKind::Discord,
        conversation_id: "channel-1".into(),
        recipient_id: "owner-1".into(),
        kind: ChannelMessageKind::NeedYou,
        content_sha256: format!("{:x}", Sha256::digest(&content)),
        created_at_ms: 6,
        recovery_cursor: Some(observation(1).cursor),
    };
    assert_eq!(
        store
            .begin_channel_outbound(&intent, &content)
            .unwrap()
            .disposition,
        ChannelOutboundDisposition::ExecuteNow
    );
    let changed = channel_message_payload(ChannelKind::Discord, "Need you: changed");
    let mut changed_intent = intent;
    changed_intent.outbound_id = "need-you-send-2".into();
    changed_intent.content_sha256 = format!("{:x}", Sha256::digest(&changed));
    assert!(matches!(
        store.begin_channel_outbound(&changed_intent, &changed),
        Err(StoreError::ChannelAuthorization(GateDecision::Denied(_)))
    ));
}

#[test]
fn receipt_outbound_requires_completed_evidence_receipt_and_prior_exact_approval() {
    let security = authority();
    let mut store = effect_store_in_memory(security.clone());
    let active = persist_channel_active(&mut store);
    let receipt = openopen_protocol::Receipt {
        id: "receipt-1".into(),
        mission_id: "mission-1".into(),
        summary: "Workbook verified".into(),
        actual_model: "gpt-5.6-sol".into(),
        evidence_ids: vec!["xlsx-1".into()],
        output_hashes: vec!["hash-1".into()],
        completed_at_ms: 14,
    };
    let content = channel_message_payload(ChannelKind::Discord, &channel_receipt_content(&receipt));
    let approved = persist_channel_send_approvals(&mut store, &active, &content);
    let evidence = security.sign_evidence(EvidenceClaims {
        id: "xlsx-1".into(),
        mission_id: "mission-1".into(),
        work_item_id: "work-1".into(),
        kind: EvidenceKind::XlsxVerified,
        source_id: "workbook-1".into(),
        sha256: Some("hash-1".into()),
        observed_at_ms: 12,
    });
    let evidenced = advance(
        &mut store,
        "receipt-evidence",
        &approved,
        MissionCommand::AttachEvidence {
            mission_id: "mission-1".into(),
            evidence,
            now_ms: 12,
        },
    );
    let work_done = advance(
        &mut store,
        "receipt-work-done",
        &evidenced,
        MissionCommand::TransitionWorkItem {
            mission_id: "mission-1".into(),
            work_item_id: "work-1".into(),
            next: WorkItemStatus::Completed,
            evidence_ids: vec!["xlsx-1".into()],
            now_ms: 13,
        },
    );
    let completed = advance(
        &mut store,
        "receipt-complete",
        &work_done,
        MissionCommand::Complete {
            mission_id: "mission-1".into(),
            receipt: NewReceipt {
                id: receipt.id.clone(),
                summary: receipt.summary.clone(),
                actual_model: receipt.actual_model.clone(),
                output_hashes: receipt.output_hashes.clone(),
                completed_at_ms: receipt.completed_at_ms,
            },
            now_ms: receipt.completed_at_ms,
        },
    );
    assert_eq!(completed.receipt.as_ref(), Some(&receipt));
    let (route_id, route_set_revision) = primary_route(&store);
    let intent = ChannelOutboundIntent {
        outbound_id: "receipt-send-1".into(),
        mission_id: "mission-1".into(),
        route_id,
        route_set_revision,
        channel: ChannelKind::Discord,
        conversation_id: "channel-1".into(),
        recipient_id: "owner-1".into(),
        kind: ChannelMessageKind::Receipt,
        content_sha256: format!("{:x}", Sha256::digest(&content)),
        created_at_ms: 15,
        recovery_cursor: Some(observation(1).cursor),
    };
    assert_eq!(
        store
            .begin_channel_outbound(&intent, &content)
            .unwrap()
            .disposition,
        ChannelOutboundDisposition::ExecuteNow
    );
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
