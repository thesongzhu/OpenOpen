//! These tests run under one local UID. They prove protocol, journal, and
//! descriptor-relative filesystem logic only; they are not cross-UID boundary
//! proof and are deliberately named `same_uid_contract_*`.

use super::*;
use ed25519_dalek::Signer;
use openopen_core::{
    ActionGate, ActionProposal, ActionTarget, ApprovalDecision, AuditAnchor,
    BrokerEnrollmentRecord, CreateMission, CreateWorkItem, EffectKind, LocalAuthority,
    MissionCommand, MissionCommandEnvelope, MissionCommandResult, NewBoundaryApproval, Store,
    StoreError, TrustedBrokerEnrollment, broker_enrollment_signing_bytes,
};
use openopen_protocol::{ApprovalKind, EffectPermitPurpose, WorkItemStatus};
use openopen_protocol::{EffectAuditAnchor, effect_receipt_signing_bytes};
use std::fs::Permissions;
use std::io::{Cursor, Read};
use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

struct Fixture {
    root: tempfile::TempDir,
    core_signing_key: SigningKey,
    config: BrokerConfig,
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        std::fs::set_permissions(root.path(), Permissions::from_mode(0o700)).unwrap();
        let now_ms = current_unix_ms().unwrap();
        let core_signing_key = SigningKey::from_bytes(&[0x11; 32]);
        let config = BrokerConfig {
            protected_root: std::fs::canonicalize(root.path()).unwrap(),
            authenticated_audit_euid: rustix::process::geteuid().as_raw(),
            enrolled_core_verifying_key: core_signing_key.verifying_key().to_bytes(),
            broker_signing_seed: [0x22; 32],
            session_nonce: "ab".repeat(32),
            session_expires_at_ms: now_ms + 120_000,
        };
        Self {
            root,
            core_signing_key,
            config,
        }
    }

    fn engine(&self) -> BrokerEngine {
        BrokerEngine::open_internal(self.config.clone()).unwrap()
    }

    fn permit(&self, effect_id: &str, path: &[&str], payload: &[u8]) -> EffectPermit {
        self.permit_at(effect_id, path, payload, current_unix_ms().unwrap())
    }

    fn permit_at(
        &self,
        effect_id: &str,
        path: &[&str],
        payload: &[u8],
        now_ms: i64,
    ) -> EffectPermit {
        let command = EffectCommand {
            protocol_version: EFFECT_PROTOCOL_VERSION,
            effect_id: effect_id.to_owned(),
            mission_id: "mission-1".into(),
            mission_updated_at_ms: now_ms - 1,
            mission_scope_digest: "scope-v1".into(),
            source_anchor: EffectAuditAnchor {
                sequence: 7,
                entry_hash: "a1".repeat(32),
                signature_hex: "b2".repeat(64),
            },
            approval_ids: vec!["approval-1".into()],
            effect: MissionFileEffect::PutFile {
                path_components: path.iter().map(ToString::to_string).collect(),
                payload: PayloadDescriptor {
                    sha256: sha256_hex(payload),
                    byte_len: u64::try_from(payload.len()).unwrap(),
                },
                action_digest: "c3".repeat(32),
            },
        };
        let stable_effect_hash = sha256_hex(&effect_command_signing_bytes(&command).unwrap());
        let authorization_anchor = EffectAuditAnchor {
            sequence: command.source_anchor.sequence + 1,
            entry_hash: "d4".repeat(32),
            signature_hex: "e5".repeat(64),
        };
        let mut permit = EffectPermit {
            command,
            stable_effect_hash,
            authorization_anchor,
            purpose: EffectPermitPurpose::Execute,
            broker_session_nonce: self.config.session_nonce.clone(),
            issued_at_ms: now_ms,
            expires_at_ms: now_ms + 30_000,
            core_key_id: sha256_hex(self.core_signing_key.verifying_key().as_bytes()),
            authorization_signature_hex: String::new(),
        };
        resign(&self.core_signing_key, &mut permit);
        permit
    }
}

fn resign(key: &SigningKey, permit: &mut EffectPermit) {
    permit.authorization_signature_hex = String::new();
    let bytes = effect_permit_signing_bytes(permit).unwrap();
    permit.authorization_signature_hex = hex::encode(key.sign(&bytes).to_bytes());
}

#[test]
fn production_constructor_rejects_same_euid_boundary() {
    let fixture = Fixture::new();
    assert!(matches!(
        BrokerEngine::open(fixture.config),
        Err(BrokerError::InvalidSecurityBoundary)
    ));
}

#[test]
fn same_uid_contract_independent_workers_share_one_cross_process_operation_lock() {
    let fixture = Fixture::new();
    let first = fixture.engine();
    assert!(matches!(
        BrokerEngine::open_internal(fixture.config.clone()),
        Err(BrokerError::OperationBusy)
    ));
    drop(first);
    BrokerEngine::open_internal(fixture.config.clone()).unwrap();
}

#[test]
fn same_uid_contract_noncommit_tombstone_rejects_old_execute_forever() {
    let fixture = Fixture::new();
    let mut engine = fixture.engine();
    let payload = b"must remain absent";
    let execute = fixture.permit("effect-terminal", &["absent.bin"], payload);
    let mut reconcile = execute.clone();
    reconcile.purpose = EffectPermitPurpose::Reconcile;
    resign(&fixture.core_signing_key, &mut reconcile);
    let now_ms = reconcile.issued_at_ms;
    let first = engine
        .reconcile_effect_with_clock(&reconcile, now_ms, || Ok(now_ms))
        .unwrap();
    let duplicate = engine
        .reconcile_effect_with_clock(&reconcile, now_ms, || Ok(now_ms))
        .unwrap();
    assert_eq!(duplicate, first);
    assert!(matches!(
        first,
        openopen_protocol::EffectReconciliation::NotCommitted { .. }
    ));
    assert!(matches!(
        engine.put_file_at(&execute, Cursor::new(payload), now_ms),
        Err(BrokerError::EffectNotCommitted)
    ));
    assert!(!fixture.root.path().join("mission-1").exists());
}

#[test]
fn same_uid_contract_lost_noncommit_response_recovers_under_fresh_session() {
    let root = tempfile::tempdir().unwrap();
    std::fs::set_permissions(root.path(), Permissions::from_mode(0o700)).unwrap();
    let store_dir = tempfile::tempdir().unwrap();
    let store_path = store_dir.path().join("core.sqlite3");
    let (authority, enrollment, first_config) = core_broker_setup(root.path(), 0x51, 0x52, "ab");
    let mut first_engine = BrokerEngine::open_internal(first_config.clone()).unwrap();
    let first_session = first_engine.broker_session();
    let mut store =
        Store::open_with_trusted_broker(&store_path, authority.clone(), enrollment.clone())
            .unwrap();
    let working = activate_core_mission(&mut store);
    let proposal = ActionProposal {
        effect: EffectKind::FileWrite,
        mission_id: "mission-1".into(),
        mission_scope_digest: "scope-v1".into(),
        target: ActionTarget::MissionFile {
            relative_path: "reports/absent.bin".into(),
        },
        estimated_cost_micros: None,
    };
    let payload = b"must never be written";
    let resumed = approve_core_write(&mut store, working, &proposal, payload);
    let mut gate = ActionGate::default();
    gate.set_enabled(true);
    let execute = store
        .prepare_mission_file_put(
            &gate,
            "lost-noncommit-effect",
            &resumed.anchor,
            &proposal,
            payload,
            &first_session,
        )
        .unwrap();
    let authorization_anchor = AuditAnchor {
        sequence: execute.authorization_anchor.sequence,
        entry_hash: execute.authorization_anchor.entry_hash.clone(),
        signature_hex: execute.authorization_anchor.signature_hex.clone(),
    };
    let first_reconcile = store
        .prepare_effect_reconciliation(
            &execute.command.effect_id,
            &authorization_anchor,
            &first_session,
        )
        .unwrap();
    assert!(matches!(
        first_engine.reconcile_effect(&first_reconcile).unwrap(),
        openopen_protocol::EffectReconciliation::NotCommitted { .. }
    ));
    drop(first_engine);
    drop(store);

    let mut second_config = first_config.clone();
    second_config.session_nonce = "ef".repeat(32);
    let mut second_engine = BrokerEngine::open_internal(second_config).unwrap();
    let second_session = second_engine.broker_session();
    let mut reopened_store =
        Store::open_with_trusted_broker(&store_path, authority, enrollment).unwrap();
    let second_reconcile = reopened_store
        .prepare_effect_reconciliation(
            &execute.command.effect_id,
            &authorization_anchor,
            &second_session,
        )
        .unwrap();
    assert_eq!(second_reconcile.broker_session_nonce, "ef".repeat(32));
    let recovered = second_engine.reconcile_effect(&second_reconcile).unwrap();
    assert_eq!(
        second_engine.reconcile_effect(&second_reconcile).unwrap(),
        recovered
    );
    let openopen_protocol::EffectReconciliation::NotCommitted { attestation } = recovered else {
        panic!("permanent tombstone must reattest noncommit under the fresh permit");
    };
    let noncommit_anchor = reopened_store
        .record_effect_noncommit(
            &authorization_anchor,
            &second_session,
            &second_reconcile,
            &attestation,
        )
        .unwrap();
    reopened_store
        .verify_audit_chain(&noncommit_anchor)
        .unwrap();
    reopened_store
        .execute_mission_command(&pause_envelope(
            "pause-after-lost-noncommit",
            noncommit_anchor,
            9,
        ))
        .unwrap();
    drop(second_engine);

    let mut first_session_engine = BrokerEngine::open_internal(first_config).unwrap();
    assert!(matches!(
        first_session_engine.put_file(&execute, Cursor::new(payload)),
        Err(BrokerError::EffectNotCommitted)
    ));
    assert!(!root.path().join("mission-1/reports/absent.bin").exists());
}

#[test]
fn same_uid_contract_reconciliation_never_mistakes_same_hash_wrong_inode_for_commit() {
    let fixture = Fixture::new();
    let mut engine = fixture.engine();
    let payload = b"same bytes wrong inode";
    let execute = fixture.permit("effect-inode-proof", &["output.bin"], payload);
    let validated = engine
        .validate_permit(&execute, execute.issued_at_ms)
        .unwrap();
    let entry = engine
        .journal
        .accept(
            &execute.command,
            &execute.stable_effect_hash,
            &validated.payload,
            &validated.path_components,
        )
        .unwrap();
    engine.bind_workspace(&execute.command.mission_id).unwrap();
    let staged = engine
        .workspace
        .prepare_stage(
            &execute.command.mission_id,
            &validated.path_components,
            &entry.stage_name,
        )
        .unwrap();
    engine
        .journal
        .mark_stage_identity(
            &execute.command.effect_id,
            &execute.stable_effect_hash,
            staged.device,
            staged.inode,
        )
        .unwrap();
    let final_path = fixture.root.path().join("mission-1/output.bin");
    std::fs::write(&final_path, payload).unwrap();
    std::fs::set_permissions(&final_path, Permissions::from_mode(0o600)).unwrap();
    let mut reconcile = execute;
    reconcile.purpose = EffectPermitPurpose::Reconcile;
    resign(&fixture.core_signing_key, &mut reconcile);
    assert!(matches!(
        engine.reconcile_effect_with_clock(&reconcile, reconcile.issued_at_ms, || Ok(
            reconcile.issued_at_ms
        ),),
        Err(BrokerError::WorkspaceBoundary)
    ));
    assert_eq!(std::fs::read(final_path).unwrap(), payload);
    assert_eq!(
        engine
            .journal
            .load("effect-inode-proof")
            .unwrap()
            .unwrap()
            .state,
        EffectState::Accepted
    );
}

#[test]
fn same_uid_contract_reattest_only_is_read_only_for_stage_and_committed_states() {
    let fixture = Fixture::new();
    let mut engine = fixture.engine();
    let payload = b"read-only reattestation";
    let execute = fixture.permit("effect-read-only", &["output.bin"], payload);
    let validated = engine
        .validate_permit(&execute, execute.issued_at_ms)
        .unwrap();
    let accepted = engine
        .journal
        .accept(
            &execute.command,
            &execute.stable_effect_hash,
            &validated.payload,
            &validated.path_components,
        )
        .unwrap();
    engine.bind_workspace(&execute.command.mission_id).unwrap();
    let staged = engine
        .workspace
        .prepare_stage(
            &execute.command.mission_id,
            &validated.path_components,
            &accepted.stage_name,
        )
        .unwrap();
    engine
        .journal
        .mark_stage_identity(
            &execute.command.effect_id,
            &execute.stable_effect_hash,
            staged.device,
            staged.inode,
        )
        .unwrap();
    let accepted_before = engine.journal.load("effect-read-only").unwrap().unwrap();
    let stage_path = fixture
        .root
        .path()
        .join("mission-1")
        .join(&accepted_before.stage_name)
        .join(".payload");
    let mut reattest = execute.clone();
    reattest.purpose = EffectPermitPurpose::ReattestOnly;
    resign(&fixture.core_signing_key, &mut reattest);
    assert!(matches!(
        engine.put_file_at(&reattest, Cursor::new(payload), reattest.issued_at_ms),
        Err(BrokerError::EffectNotCommitted)
    ));
    assert_eq!(
        engine.journal.load("effect-read-only").unwrap().unwrap(),
        accepted_before
    );
    assert!(stage_path.exists());
    assert_eq!(std::fs::read(&stage_path).unwrap(), b"");

    let mut reconcile = execute;
    reconcile.purpose = EffectPermitPurpose::Reconcile;
    resign(&fixture.core_signing_key, &mut reconcile);
    assert!(matches!(
        engine
            .reconcile_effect_with_clock(&reconcile, reconcile.issued_at_ms, || Ok(
                reconcile.issued_at_ms
            ),)
            .unwrap(),
        openopen_protocol::EffectReconciliation::NotCommitted { .. }
    ));

    let committed_execute = fixture.permit("effect-committed-read-only", &["done.bin"], payload);
    engine
        .put_file_at(
            &committed_execute,
            Cursor::new(payload),
            committed_execute.issued_at_ms,
        )
        .unwrap();
    let committed_before = engine
        .journal
        .load("effect-committed-read-only")
        .unwrap()
        .unwrap();
    let mut committed_reattest = committed_execute;
    committed_reattest.purpose = EffectPermitPurpose::ReattestOnly;
    committed_reattest.issued_at_ms = committed_before.completed_at_ms.unwrap() + 1;
    committed_reattest.expires_at_ms = committed_reattest.issued_at_ms + 20_000;
    resign(&fixture.core_signing_key, &mut committed_reattest);
    engine
        .put_file_at(
            &committed_reattest,
            Cursor::new(payload),
            committed_reattest.issued_at_ms,
        )
        .unwrap();
    assert_eq!(
        engine
            .journal
            .load("effect-committed-read-only")
            .unwrap()
            .unwrap(),
        committed_before
    );
}

#[test]
fn same_uid_contract_put_signs_receipt_and_exact_retry_returns_original() {
    let fixture = Fixture::new();
    let mut engine = fixture.engine();
    let payload = b"verified workbook";
    let permit = fixture.permit("effect-1", &["output.xlsx"], payload);

    let receipt = engine.put_file(&permit, Cursor::new(payload)).unwrap();
    let duplicate = engine.put_file(&permit, Cursor::new(payload)).unwrap();

    assert_eq!(duplicate, receipt);
    assert_eq!(
        std::fs::read(fixture.root.path().join("mission-1/output.xlsx")).unwrap(),
        payload
    );
    assert_eq!(
        std::fs::metadata(fixture.root.path().join("mission-1/output.xlsx"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    let bytes = effect_receipt_signing_bytes(&receipt).unwrap();
    let signature =
        Signature::from_slice(&hex::decode(&receipt.broker_signature_hex).unwrap()).unwrap();
    engine
        .broker_signing_key
        .verifying_key()
        .verify(&bytes, &signature)
        .unwrap();
}

struct CountingReader {
    bytes: Cursor<Vec<u8>>,
    consumed: Arc<AtomicUsize>,
}

impl Read for CountingReader {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let count = self.bytes.read(buffer)?;
        self.consumed.fetch_add(count, Ordering::SeqCst);
        Ok(count)
    }
}

fn counting_reader(bytes: Vec<u8>) -> (CountingReader, Arc<AtomicUsize>) {
    let consumed = Arc::new(AtomicUsize::new(0));
    (
        CountingReader {
            bytes: Cursor::new(bytes),
            consumed: Arc::clone(&consumed),
        },
        consumed,
    )
}

#[test]
fn same_uid_contract_committed_retry_consumes_and_validates_payload() {
    let fixture = Fixture::new();
    let mut engine = fixture.engine();
    let payload = vec![0x41; 256 * 1024];
    let permit = fixture.permit("effect-consumed-retry", &["output.bin"], &payload);
    let original = engine.put_file(&permit, Cursor::new(&payload)).unwrap();

    let (matching, matching_count) = counting_reader(payload.clone());
    assert_eq!(engine.put_file(&permit, matching).unwrap(), original);
    assert_eq!(matching_count.load(Ordering::SeqCst), payload.len());

    let changed = vec![0x42; payload.len()];
    let (mismatching, mismatching_count) = counting_reader(changed);
    assert!(matches!(
        engine.put_file(&permit, mismatching),
        Err(BrokerError::PayloadMismatch)
    ));
    assert_eq!(mismatching_count.load(Ordering::SeqCst), payload.len());
    assert_eq!(
        std::fs::read(fixture.root.path().join("mission-1/output.bin")).unwrap(),
        payload
    );
}

#[test]
fn same_uid_contract_retry_rechecks_expiry_after_payload_and_output_validation() {
    let fixture = Fixture::new();
    let mut engine = fixture.engine();
    let payload = b"completion-time expiry";
    let now_ms = current_unix_ms().unwrap();
    let permit = fixture.permit_at("effect-expired-retry", &["output.bin"], payload, now_ms);
    engine
        .put_file_at(&permit, Cursor::new(payload), now_ms)
        .unwrap();

    assert!(matches!(
        engine.put_file_with_completion_clock(&permit, Cursor::new(payload), now_ms + 1, || Ok(
            permit.expires_at_ms
        ),),
        Err(BrokerError::InvalidPermit)
    ));
}

#[test]
fn same_uid_contract_reattest_only_never_creates_a_missing_effect_or_workspace() {
    let fixture = Fixture::new();
    let mut engine = fixture.engine();
    let payload = b"must already exist";
    let mut permit = fixture.permit("effect-missing-recovery", &["output.bin"], payload);
    permit.purpose = EffectPermitPurpose::ReattestOnly;
    resign(&fixture.core_signing_key, &mut permit);

    assert!(matches!(
        engine.put_file(&permit, Cursor::new(payload)),
        Err(BrokerError::EffectNotCommitted)
    ));
    assert!(!fixture.root.path().join("mission-1").exists());
}

#[test]
fn same_uid_contract_receipt_binds_the_exact_same_session_permit() {
    let fixture = Fixture::new();
    let mut engine = fixture.engine();
    let payload = b"exact permit binding";
    let first_now = current_unix_ms().unwrap();
    let first_permit =
        fixture.permit_at("effect-exact-permit", &["output.bin"], payload, first_now);
    let first = engine
        .put_file_at(&first_permit, Cursor::new(payload), first_now)
        .unwrap();

    let mut second_permit = first_permit;
    second_permit.purpose = EffectPermitPurpose::ReattestOnly;
    second_permit.issued_at_ms = first.committed_at_ms + 1;
    second_permit.expires_at_ms = second_permit.issued_at_ms + 20_000;
    resign(&fixture.core_signing_key, &mut second_permit);
    let second = engine
        .put_file_at(
            &second_permit,
            Cursor::new(payload),
            second_permit.issued_at_ms,
        )
        .unwrap();
    let exact_retry = engine
        .put_file_at(
            &second_permit,
            Cursor::new(payload),
            second_permit.issued_at_ms + 1,
        )
        .unwrap();

    assert_ne!(first.permit_hash, second.permit_hash);
    assert_ne!(first, second);
    assert_eq!(first.committed_at_ms, second.committed_at_ms);
    assert_eq!(second, exact_retry);
}

#[test]
fn same_uid_contract_expiry_after_rename_requires_reconciliation_proof() {
    let fixture = Fixture::new();
    let mut engine = fixture.engine();
    let payload = b"must not commit stale";
    let now_ms = current_unix_ms().unwrap();
    let permit = fixture.permit_at(
        "effect-expire-during-stream",
        &["output.bin"],
        payload,
        now_ms,
    );
    let validated = engine.validate_permit(&permit, now_ms).unwrap();
    let entry = engine
        .journal
        .accept(
            &permit.command,
            &permit.stable_effect_hash,
            &validated.payload,
            &validated.path_components,
        )
        .unwrap();
    engine.bind_workspace(&permit.command.mission_id).unwrap();

    assert!(matches!(
        engine.commit_accepted_write(&entry, &permit, &validated, Cursor::new(payload), || Ok(
            permit.expires_at_ms
        ),),
        Err(BrokerError::InvalidPermit)
    ));
    assert_eq!(
        std::fs::read(fixture.root.path().join("mission-1/output.bin")).unwrap(),
        payload
    );
    let reconcile_now = now_ms + 1;
    let mut reconcile = permit;
    reconcile.purpose = EffectPermitPurpose::Reconcile;
    reconcile.issued_at_ms = reconcile_now;
    reconcile.expires_at_ms = reconcile_now + 30_000;
    resign(&fixture.core_signing_key, &mut reconcile);
    assert!(matches!(
        engine
            .reconcile_effect_with_clock(&reconcile, reconcile_now, || Ok(reconcile_now))
            .unwrap(),
        openopen_protocol::EffectReconciliation::Committed { .. }
    ));
}

#[test]
fn same_uid_contract_committed_retry_gets_fresh_session_attestation() {
    let fixture = Fixture::new();
    let first_now = current_unix_ms().unwrap();
    let mut engine = fixture.engine();
    let payload = b"session recovery";
    let first_permit =
        fixture.permit_at("effect-session-retry", &["output.bin"], payload, first_now);
    let first_receipt = engine
        .put_file_at(&first_permit, Cursor::new(payload), first_now)
        .unwrap();
    drop(engine);

    let retry_now = first_now + 1_000;
    let mut retry_config = fixture.config.clone();
    retry_config.session_nonce = "cd".repeat(32);
    retry_config.session_expires_at_ms = retry_now + 120_000;
    let mut retry_permit = first_permit;
    retry_permit.purpose = EffectPermitPurpose::ReattestOnly;
    retry_permit.broker_session_nonce = retry_config.session_nonce.clone();
    retry_permit.issued_at_ms = retry_now;
    retry_permit.expires_at_ms = retry_now + 30_000;
    resign(&fixture.core_signing_key, &mut retry_permit);
    let mut reopened = BrokerEngine::open_internal(retry_config).unwrap();
    let recovered = reopened
        .put_file_at(&retry_permit, Cursor::new(payload), retry_now)
        .unwrap();
    let exact_retry = reopened
        .put_file_at(&retry_permit, Cursor::new(payload), retry_now + 1)
        .unwrap();

    assert_eq!(recovered.committed_at_ms, first_receipt.committed_at_ms);
    assert_eq!(recovered.attested_at_ms, retry_now);
    assert_eq!(recovered.broker_session_nonce, "cd".repeat(32));
    assert_ne!(recovered, first_receipt);
    assert_eq!(exact_retry, recovered);
    assert_eq!(
        std::fs::read(fixture.root.path().join("mission-1/output.bin")).unwrap(),
        payload
    );
}

#[test]
fn same_uid_contract_recovery_only_fresh_session_attests_lost_response() {
    let fixture = Fixture::new();
    let payload = b"rename succeeded response lost";
    let first_permit = fixture.permit("effect-lost-response", &["recovered.bin"], payload);
    let mut engine = fixture.engine();
    let committed_at_ms = commit_without_receipt(&mut engine, &first_permit, payload, false);
    drop(engine);

    let retry_now = committed_at_ms + 1_000;
    let mut retry_config = fixture.config.clone();
    retry_config.session_nonce = "cd".repeat(32);
    retry_config.session_expires_at_ms = retry_now + 120_000;
    let mut recovery_permit = first_permit;
    recovery_permit.purpose = EffectPermitPurpose::Reconcile;
    recovery_permit.broker_session_nonce = retry_config.session_nonce.clone();
    recovery_permit.issued_at_ms = retry_now;
    recovery_permit.expires_at_ms = retry_now + 30_000;
    resign(&fixture.core_signing_key, &mut recovery_permit);

    let mut reopened = BrokerEngine::open_internal(retry_config).unwrap();
    let reconciliation = reopened
        .reconcile_effect_with_clock(&recovery_permit, retry_now, || Ok(retry_now))
        .unwrap();
    let openopen_protocol::EffectReconciliation::Committed { receipt } = reconciliation else {
        panic!("post-rename inode must reconcile as committed");
    };
    assert_eq!(receipt.committed_at_ms, retry_now);
    assert_eq!(receipt.attested_at_ms, retry_now);
    assert_eq!(receipt.broker_session_nonce, "cd".repeat(32));
    assert_eq!(
        std::fs::read(fixture.root.path().join("mission-1/recovered.bin")).unwrap(),
        payload
    );
}

#[test]
fn same_uid_contract_changed_effect_id_payload_conflicts() {
    let fixture = Fixture::new();
    let mut engine = fixture.engine();
    let first = fixture.permit("effect-2", &["result.bin"], b"first");
    engine.put_file(&first, Cursor::new(b"first")).unwrap();
    let changed = fixture.permit("effect-2", &["result.bin"], b"changed");
    assert!(matches!(
        engine.put_file(&changed, Cursor::new(b"changed")),
        Err(BrokerError::EffectConflict)
    ));
    assert_eq!(
        std::fs::read(fixture.root.path().join("mission-1/result.bin")).unwrap(),
        b"first"
    );
}

#[test]
fn same_uid_contract_tampered_wrong_key_session_stale_and_payload_fail_closed() {
    let fixture = Fixture::new();
    let mut engine = fixture.engine();
    let now_ms = current_unix_ms().unwrap();

    let mut tampered = fixture.permit_at("tampered-1", &["a.bin"], b"a", now_ms);
    tampered.command.mission_scope_digest = "changed".into();
    assert!(matches!(
        engine.put_file_at(&tampered, Cursor::new(b"a"), now_ms),
        Err(BrokerError::InvalidPermit)
    ));

    let wrong_key = SigningKey::from_bytes(&[0x44; 32]);
    let mut wrong = fixture.permit_at("wrong-key-1", &["b.bin"], b"b", now_ms);
    wrong.core_key_id = sha256_hex(wrong_key.verifying_key().as_bytes());
    resign(&wrong_key, &mut wrong);
    assert!(matches!(
        engine.put_file_at(&wrong, Cursor::new(b"b"), now_ms),
        Err(BrokerError::InvalidPermit)
    ));

    let mut wrong_session = fixture.permit_at("wrong-session-1", &["c.bin"], b"c", now_ms);
    wrong_session.broker_session_nonce = "cd".repeat(32);
    resign(&fixture.core_signing_key, &mut wrong_session);
    assert!(matches!(
        engine.put_file_at(&wrong_session, Cursor::new(b"c"), now_ms),
        Err(BrokerError::InvalidPermit)
    ));

    let stale = fixture.permit_at("stale-1", &["d.bin"], b"d", now_ms - 31_000);
    assert!(matches!(
        engine.put_file_at(&stale, Cursor::new(b"d"), now_ms),
        Err(BrokerError::InvalidPermit)
    ));

    let payload = fixture.permit_at("payload-1", &["e.bin"], b"expected", now_ms);
    assert!(matches!(
        engine.put_file_at(&payload, Cursor::new(b"changed"), now_ms),
        Err(BrokerError::PayloadMismatch)
    ));
    assert!(!fixture.root.path().join("mission-1/e.bin").exists());
    let mut reconcile = payload.clone();
    reconcile.purpose = EffectPermitPurpose::Reconcile;
    resign(&fixture.core_signing_key, &mut reconcile);
    assert!(matches!(
        engine
            .reconcile_effect_with_clock(&reconcile, now_ms, || Ok(now_ms))
            .unwrap(),
        openopen_protocol::EffectReconciliation::NotCommitted { .. }
    ));
    assert!(matches!(
        engine.put_file_at(&payload, Cursor::new(b"expected"), now_ms),
        Err(BrokerError::EffectNotCommitted)
    ));
}

#[test]
fn same_uid_contract_traversal_and_nested_symlink_fail_closed() {
    let fixture = Fixture::new();
    let mut engine = fixture.engine();
    let traversal = fixture.permit("traversal-1", &["..", "escape"], b"x");
    assert!(matches!(
        engine.put_file(&traversal, Cursor::new(b"x")),
        Err(BrokerError::InvalidCommand)
    ));

    let seed = fixture.permit("seed-1", &["seed.bin"], b"seed");
    engine.put_file(&seed, Cursor::new(b"seed")).unwrap();
    let outside = tempfile::tempdir().unwrap();
    symlink(outside.path(), fixture.root.path().join("mission-1/escape")).unwrap();
    let nested = fixture.permit("symlink-1", &["escape", "canary"], b"secret");
    assert!(matches!(
        engine.put_file(&nested, Cursor::new(b"secret")),
        Err(BrokerError::WorkspaceBoundary)
    ));
    assert!(!outside.path().join("canary").exists());
}

#[test]
fn same_uid_contract_nested_directory_write_succeeds() {
    let fixture = Fixture::new();
    let mut engine = fixture.engine();
    let permit = fixture.permit("nested-success", &["reports", "output.xlsx"], b"nested");
    engine.put_file(&permit, Cursor::new(b"nested")).unwrap();
    assert_eq!(
        std::fs::read(fixture.root.path().join("mission-1/reports/output.xlsx")).unwrap(),
        b"nested"
    );
}

#[test]
fn same_uid_contract_atomic_replace_never_truncates_outside_hardlink() {
    let fixture = Fixture::new();
    let mut engine = fixture.engine();
    let seed = fixture.permit("seed-hardlink", &["seed.bin"], b"seed");
    engine.put_file(&seed, Cursor::new(b"seed")).unwrap();
    let outside = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(outside.path(), b"outside-original").unwrap();
    let target = fixture.root.path().join("mission-1/output.bin");
    std::fs::hard_link(outside.path(), &target).unwrap();

    let permit = fixture.permit("hardlink-replace", &["output.bin"], b"broker-output");
    engine
        .put_file(&permit, Cursor::new(b"broker-output"))
        .unwrap();

    assert_eq!(std::fs::read(outside.path()).unwrap(), b"outside-original");
    assert_eq!(std::fs::read(&target).unwrap(), b"broker-output");
    assert_ne!(
        std::fs::metadata(outside.path()).unwrap().ino(),
        std::fs::metadata(&target).unwrap().ino()
    );
}

struct BlockingReader {
    bytes: Cursor<Vec<u8>>,
    announced: bool,
    reached: Sender<()>,
    resume: Receiver<()>,
}

impl Read for BlockingReader {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let count = self.bytes.read(buffer)?;
        if count > 0 && !self.announced {
            self.announced = true;
            self.reached.send(()).unwrap();
            self.resume.recv().unwrap();
        }
        Ok(count)
    }
}

fn blocking_reader(bytes: Vec<u8>) -> (BlockingReader, Receiver<()>, Sender<()>) {
    let (reached_tx, reached_rx) = channel();
    let (resume_tx, resume_rx) = channel();
    (
        BlockingReader {
            bytes: Cursor::new(bytes),
            announced: false,
            reached: reached_tx,
            resume: resume_rx,
        },
        reached_rx,
        resume_tx,
    )
}

#[test]
fn same_uid_contract_external_staging_hardlink_is_detected_and_scrubbed() {
    let fixture = Fixture::new();
    let engine = fixture.engine();
    let payload = vec![0x5a; 128 * 1024];
    let permit = fixture.permit("stage-hardlink", &["private.bin"], &payload);
    let (reader, reached, resume) = blocking_reader(payload);
    let handle = std::thread::spawn(move || {
        let mut engine = engine;
        engine.put_file(&permit, reader)
    });
    reached.recv().unwrap();
    let mission = fixture.root.path().join("mission-1");
    let stage = std::fs::read_dir(&mission)
        .unwrap()
        .find_map(|entry| {
            let entry = entry.unwrap();
            entry
                .file_name()
                .to_string_lossy()
                .starts_with(".openopen-stage-")
                .then(|| entry.path())
        })
        .unwrap();
    let leaked = fixture.root.path().join("leaked.bin");
    std::fs::hard_link(stage.join(".payload"), &leaked).unwrap();
    resume.send(()).unwrap();
    assert!(matches!(
        handle.join().unwrap(),
        Err(BrokerError::WorkspaceBoundary)
    ));
    assert_eq!(std::fs::metadata(&leaked).unwrap().len(), 0);
    assert!(!mission.join("private.bin").exists());
}

#[test]
fn same_uid_contract_workspace_relocation_during_stream_fails_and_scrubs() {
    let fixture = Fixture::new();
    let engine = fixture.engine();
    let payload = vec![0x6b; 128 * 1024];
    let permit = fixture.permit("move-workspace", &["result.bin"], &payload);
    let (reader, reached, resume) = blocking_reader(payload);
    let handle = std::thread::spawn(move || {
        let mut engine = engine;
        engine.put_file(&permit, reader)
    });
    reached.recv().unwrap();
    let mission = fixture.root.path().join("mission-1");
    let outside = tempfile::tempdir().unwrap();
    let moved = outside.path().join("moved-mission");
    std::fs::rename(&mission, &moved).unwrap();
    resume.send(()).unwrap();
    assert!(matches!(
        handle.join().unwrap(),
        Err(BrokerError::WorkspaceBoundary)
    ));
    assert!(!moved.join("result.bin").exists());
    assert!(std::fs::read_dir(&moved).unwrap().next().is_none());
}

fn commit_without_receipt(
    engine: &mut BrokerEngine,
    permit: &EffectPermit,
    payload: &[u8],
    mark_filesystem: bool,
) -> i64 {
    let now_ms = current_unix_ms().unwrap();
    let validated = engine.validate_permit(permit, now_ms).unwrap();
    let entry = engine
        .journal
        .accept(
            &permit.command,
            &permit.stable_effect_hash,
            &validated.payload,
            &validated.path_components,
        )
        .unwrap();
    let identity = engine
        .workspace
        .ensure_workspace(&permit.command.mission_id)
        .unwrap();
    engine
        .journal
        .bind_workspace(&permit.command.mission_id, identity.device, identity.inode)
        .unwrap();
    let staged_identity = engine
        .workspace
        .prepare_stage(
            &permit.command.mission_id,
            &validated.path_components,
            &entry.stage_name,
        )
        .unwrap();
    engine
        .journal
        .mark_stage_identity(
            &permit.command.effect_id,
            &permit.stable_effect_hash,
            staged_identity.device,
            staged_identity.inode,
        )
        .unwrap();
    engine
        .journal
        .mark_commit_intent(
            &permit.command.effect_id,
            &permit.stable_effect_hash,
            now_ms,
            &permit.broker_session_nonce,
        )
        .unwrap();
    engine
        .workspace
        .write_atomically(
            &permit.command.mission_id,
            &validated.path_components,
            &validated.payload,
            &entry.stage_name,
            Cursor::new(payload),
            || Ok(()),
        )
        .unwrap();
    if mark_filesystem {
        engine
            .journal
            .mark_filesystem_committed(
                &permit.command.effect_id,
                &permit.stable_effect_hash,
                now_ms,
                &permit.broker_session_nonce,
            )
            .unwrap();
    }
    now_ms
}

#[test]
fn same_uid_contract_recovery_closes_both_commit_windows() {
    for mark_filesystem in [false, true] {
        let fixture = Fixture::new();
        let payload = b"recoverable";
        let permit = fixture.permit(
            if mark_filesystem {
                "recover-after-fs"
            } else {
                "recover-after-rename"
            },
            &["recovered.bin"],
            payload,
        );
        let mut engine = fixture.engine();
        let original_commit_intent =
            commit_without_receipt(&mut engine, &permit, payload, mark_filesystem);
        drop(engine);

        let retry_now = original_commit_intent + 1_000;
        let mut reconcile_permit = permit.clone();
        reconcile_permit.purpose = EffectPermitPurpose::Reconcile;
        reconcile_permit.issued_at_ms = retry_now;
        reconcile_permit.expires_at_ms = retry_now + 30_000;
        resign(&fixture.core_signing_key, &mut reconcile_permit);
        let mut reopened = fixture.engine();
        let reconciliation = reopened
            .reconcile_effect_with_clock(&reconcile_permit, retry_now, || Ok(retry_now))
            .unwrap();
        let openopen_protocol::EffectReconciliation::Committed { receipt } = reconciliation else {
            panic!("durable staged inode must reconcile as committed");
        };
        let duplicate = reopened
            .reconcile_effect_with_clock(&reconcile_permit, retry_now, || Ok(retry_now))
            .unwrap();
        assert_eq!(
            duplicate,
            openopen_protocol::EffectReconciliation::Committed {
                receipt: receipt.clone()
            }
        );
        assert_eq!(
            receipt.committed_at_ms,
            if mark_filesystem {
                original_commit_intent
            } else {
                retry_now
            }
        );
        assert_eq!(
            std::fs::read(fixture.root.path().join("mission-1/recovered.bin")).unwrap(),
            payload
        );
    }
}

#[test]
fn same_uid_contract_recovery_recognizes_empty_stage_after_rename() {
    let fixture = Fixture::new();
    let payload = b"renamed-before-stage-cleanup";
    let permit = fixture.permit("recover-empty-stage", &["result.bin"], payload);
    let mut engine = fixture.engine();
    let _ = commit_without_receipt(&mut engine, &permit, payload, false);
    let entry = engine
        .journal
        .load(&permit.command.effect_id)
        .unwrap()
        .unwrap();
    let empty_stage = fixture
        .root
        .path()
        .join("mission-1")
        .join(&entry.stage_name);
    std::fs::create_dir(&empty_stage).unwrap();
    std::fs::set_permissions(&empty_stage, Permissions::from_mode(0o700)).unwrap();
    drop(engine);

    let retry_now = current_unix_ms().unwrap();
    let mut reconcile_permit = permit;
    reconcile_permit.purpose = EffectPermitPurpose::Reconcile;
    reconcile_permit.issued_at_ms = retry_now;
    reconcile_permit.expires_at_ms = retry_now + 30_000;
    resign(&fixture.core_signing_key, &mut reconcile_permit);
    let mut reopened = fixture.engine();
    assert!(matches!(
        reopened
            .reconcile_effect_with_clock(&reconcile_permit, retry_now, || Ok(retry_now))
            .unwrap(),
        openopen_protocol::EffectReconciliation::Committed { .. }
    ));
    assert!(
        std::fs::read_dir(fixture.root.path().join("mission-1"))
            .unwrap()
            .all(|entry| !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".openopen-stage-"))
    );
}

#[test]
fn same_uid_contract_recovery_finishes_noncommit_after_stage_cleanup_crash() {
    let fixture = Fixture::new();
    let payload = b"cleanup-before-tombstone";
    let execute = fixture.permit("recover-cleanup-crash", &["result.bin"], payload);
    let mut engine = fixture.engine();
    let now_ms = current_unix_ms().unwrap();
    let validated = engine.validate_permit(&execute, now_ms).unwrap();
    let entry = engine
        .journal
        .accept(
            &execute.command,
            &execute.stable_effect_hash,
            &validated.payload,
            &validated.path_components,
        )
        .unwrap();
    let workspace_identity = engine
        .workspace
        .ensure_workspace(&execute.command.mission_id)
        .unwrap();
    engine
        .journal
        .bind_workspace(
            &execute.command.mission_id,
            workspace_identity.device,
            workspace_identity.inode,
        )
        .unwrap();
    let staged_identity = engine
        .workspace
        .prepare_stage(
            &execute.command.mission_id,
            &validated.path_components,
            &entry.stage_name,
        )
        .unwrap();
    engine
        .journal
        .mark_stage_identity(
            &execute.command.effect_id,
            &execute.stable_effect_hash,
            staged_identity.device,
            staged_identity.inode,
        )
        .unwrap();
    engine
        .journal
        .mark_commit_intent(
            &execute.command.effect_id,
            &execute.stable_effect_hash,
            now_ms,
            &execute.broker_session_nonce,
        )
        .unwrap();
    engine
        .workspace
        .discard_owned_stage(
            &execute.command.mission_id,
            &validated.path_components,
            &entry.stage_name,
            Some(staged_identity),
        )
        .unwrap();
    drop(engine);

    let retry_now = now_ms + 1;
    let mut reconcile = execute.clone();
    reconcile.purpose = EffectPermitPurpose::Reconcile;
    reconcile.issued_at_ms = retry_now;
    reconcile.expires_at_ms = retry_now + 30_000;
    resign(&fixture.core_signing_key, &mut reconcile);
    let mut reopened = fixture.engine();
    assert!(matches!(
        reopened
            .reconcile_effect_with_clock(&reconcile, retry_now, || Ok(retry_now))
            .unwrap(),
        openopen_protocol::EffectReconciliation::NotCommitted { .. }
    ));
    assert!(matches!(
        reopened.put_file_at(&execute, Cursor::new(payload), retry_now),
        Err(BrokerError::EffectNotCommitted)
    ));
    assert!(!fixture.root.path().join("mission-1/result.bin").exists());
}

#[test]
fn same_uid_contract_recovery_cleans_stage_created_before_started_marker() {
    let fixture = Fixture::new();
    let payload = b"stage-before-marker";
    let permit = fixture.permit("recover-before-started", &["result.bin"], payload);
    let mut engine = fixture.engine();
    let now_ms = current_unix_ms().unwrap();
    let validated = engine.validate_permit(&permit, now_ms).unwrap();
    let entry = engine
        .journal
        .accept(
            &permit.command,
            &permit.stable_effect_hash,
            &validated.payload,
            &validated.path_components,
        )
        .unwrap();
    let identity = engine
        .workspace
        .ensure_workspace(&permit.command.mission_id)
        .unwrap();
    engine
        .journal
        .bind_workspace(&permit.command.mission_id, identity.device, identity.inode)
        .unwrap();
    engine
        .workspace
        .prepare_stage(
            &permit.command.mission_id,
            &validated.path_components,
            &entry.stage_name,
        )
        .unwrap();
    drop(engine);

    let mut reopened = fixture.engine();
    reopened.put_file(&permit, Cursor::new(payload)).unwrap();
    assert_eq!(
        std::fs::read(fixture.root.path().join("mission-1/result.bin")).unwrap(),
        payload
    );
}

#[test]
fn same_uid_contract_journal_pins_enrolled_core_and_broker_keys() {
    let fixture = Fixture::new();
    let engine = fixture.engine();
    drop(engine);

    let mut changed_broker = fixture.config.clone();
    changed_broker.broker_signing_seed = [0x55; 32];
    assert!(matches!(
        BrokerEngine::open_internal(changed_broker),
        Err(BrokerError::JournalMismatch)
    ));

    let mut changed_core = fixture.config.clone();
    changed_core.enrolled_core_verifying_key = SigningKey::from_bytes(&[0x66; 32])
        .verifying_key()
        .to_bytes();
    assert!(matches!(
        BrokerEngine::open_internal(changed_core),
        Err(BrokerError::JournalMismatch)
    ));
}

#[test]
fn same_uid_contract_workspace_replacement_is_rejected_after_restart() {
    let fixture = Fixture::new();
    let first = fixture.permit("pin-workspace", &["first.bin"], b"first");
    let mut engine = fixture.engine();
    engine.put_file(&first, Cursor::new(b"first")).unwrap();
    drop(engine);

    let original = fixture.root.path().join("mission-original");
    std::fs::rename(fixture.root.path().join("mission-1"), &original).unwrap();
    std::fs::create_dir(fixture.root.path().join("mission-1")).unwrap();
    std::fs::set_permissions(
        fixture.root.path().join("mission-1"),
        Permissions::from_mode(0o700),
    )
    .unwrap();
    let second = fixture.permit("replacement-write", &["second.bin"], b"second");
    let mut reopened = fixture.engine();
    assert!(matches!(
        reopened.put_file(&first, Cursor::new(b"first")),
        Err(BrokerError::WorkspaceBoundary)
    ));
    assert!(matches!(
        reopened.put_file(&second, Cursor::new(b"second")),
        Err(BrokerError::WorkspaceBoundary)
    ));
    assert!(!fixture.root.path().join("mission-1/second.bin").exists());
}

fn execute_core_command(
    store: &mut Store,
    command_id: &str,
    anchor: Option<AuditAnchor>,
    command: MissionCommand,
) -> MissionCommandResult {
    store
        .execute_mission_command(&MissionCommandEnvelope {
            command_id: command_id.into(),
            expected_anchor: anchor,
            command,
        })
        .unwrap()
}

fn pause_envelope(command_id: &str, anchor: AuditAnchor, now_ms: i64) -> MissionCommandEnvelope {
    MissionCommandEnvelope {
        command_id: command_id.into(),
        expected_anchor: Some(anchor),
        command: MissionCommand::Pause {
            mission_id: "mission-1".into(),
            now_ms,
        },
    }
}

fn activate_core_mission(store: &mut Store) -> MissionCommandResult {
    let created = execute_core_command(
        store,
        "roundtrip-1",
        None,
        MissionCommand::Create {
            input: CreateMission {
                mission_id: "mission-1".into(),
                title: "Broker round trip".into(),
                outcome: "Persist one exact file".into(),
                owner_id: "owner-1".into(),
                scope_digest: "scope-v1".into(),
                scope_approval_id: "scope-1".into(),
                scope_approval_prompt: "Approve?".into(),
                work_items: vec![CreateWorkItem {
                    id: "work-1".into(),
                    title: "Write output".into(),
                }],
                now_ms: 1,
            },
        },
    );
    let confirming = execute_core_command(
        store,
        "roundtrip-2",
        Some(created.anchor),
        MissionCommand::BeginConfirmation {
            mission_id: "mission-1".into(),
            now_ms: 2,
        },
    );
    let approved = execute_core_command(
        store,
        "roundtrip-3",
        Some(confirming.anchor),
        MissionCommand::DecideApproval {
            mission_id: "mission-1".into(),
            approval_id: "scope-1".into(),
            actor_id: "owner-1".into(),
            decision: ApprovalDecision::Approve,
            now_ms: 3,
        },
    );
    let active = execute_core_command(
        store,
        "roundtrip-4",
        Some(approved.anchor),
        MissionCommand::Activate {
            mission_id: "mission-1".into(),
            now_ms: 4,
        },
    );
    execute_core_command(
        store,
        "roundtrip-5",
        Some(active.anchor),
        MissionCommand::TransitionWorkItem {
            mission_id: "mission-1".into(),
            work_item_id: "work-1".into(),
            next: WorkItemStatus::Active,
            evidence_ids: Vec::new(),
            now_ms: 5,
        },
    )
}

fn approve_core_write(
    store: &mut Store,
    working: MissionCommandResult,
    proposal: &ActionProposal,
    payload: &[u8],
) -> MissionCommandResult {
    let action_digest = proposal
        .approval_digest(ApprovalKind::NewExternalWrite, Some(payload))
        .unwrap();
    let requested = execute_core_command(
        store,
        "roundtrip-6",
        Some(working.anchor),
        MissionCommand::RequestScopeChange {
            mission_id: "mission-1".into(),
            approval: NewBoundaryApproval {
                id: "write-approval".into(),
                kind: ApprovalKind::NewExternalWrite,
                prompt: "Write exact file?".into(),
                scope_digest: action_digest,
            },
            needs_me_id: "write-needs-me".into(),
            now_ms: 6,
        },
    );
    let approved = execute_core_command(
        store,
        "roundtrip-7",
        Some(requested.anchor),
        MissionCommand::DecideApproval {
            mission_id: "mission-1".into(),
            approval_id: "write-approval".into(),
            actor_id: "owner-1".into(),
            decision: ApprovalDecision::Approve,
            now_ms: 7,
        },
    );
    execute_core_command(
        store,
        "roundtrip-8",
        Some(approved.anchor),
        MissionCommand::Resume {
            mission_id: "mission-1".into(),
            now_ms: 8,
        },
    )
}

fn core_broker_setup(
    root: &std::path::Path,
    core_byte: u8,
    broker_byte: u8,
    session_prefix: &str,
) -> (LocalAuthority, TrustedBrokerEnrollment, BrokerConfig) {
    let core_master = [core_byte; 32];
    let broker_seed = [broker_byte; 32];
    let authority = LocalAuthority::from_master("openopen-core", core_master);
    let broker_key = SigningKey::from_bytes(&broker_seed)
        .verifying_key()
        .to_bytes();
    let mut enrollment_record = BrokerEnrollmentRecord {
        version: 1,
        broker_key_id: sha256_hex(&broker_key),
        broker_verifying_key_hex: hex::encode(broker_key),
        helper_designated_requirement_digest: "cd".repeat(32),
        installed_at_ms: 1,
        core_key_id: authority.effect_key_id(),
        core_authorization_signature_hex: String::new(),
    };
    let mut derivation = b"openopen-effect-authorizer-v1".to_vec();
    derivation.extend(core_master);
    let enrollment_signing_key = SigningKey::from_bytes(&Sha256::digest(derivation).into());
    enrollment_record.core_authorization_signature_hex = hex::encode(
        enrollment_signing_key
            .sign(&broker_enrollment_signing_bytes(&enrollment_record).unwrap())
            .to_bytes(),
    );
    let enrollment =
        TrustedBrokerEnrollment::from_signed_install_record(&authority, &enrollment_record)
            .unwrap();
    let config = BrokerConfig {
        protected_root: std::fs::canonicalize(root).unwrap(),
        authenticated_audit_euid: rustix::process::geteuid().as_raw(),
        enrolled_core_verifying_key: hex::decode(authority.effect_verifying_key_hex())
            .unwrap()
            .try_into()
            .unwrap(),
        broker_signing_seed: broker_seed,
        session_nonce: session_prefix.repeat(32),
        session_expires_at_ms: current_unix_ms().unwrap() + 120_000,
    };
    (authority, enrollment, config)
}

#[test]
fn same_uid_contract_core_store_broker_receipt_round_trip() {
    let root = tempfile::tempdir().unwrap();
    std::fs::set_permissions(root.path(), Permissions::from_mode(0o700)).unwrap();
    let (authority, enrollment, config) = core_broker_setup(root.path(), 0x31, 0x32, "ab");
    let mut engine = BrokerEngine::open_internal(config).unwrap();
    let session = engine.broker_session();
    let mut store = Store::open_in_memory_with_trusted_broker(authority, enrollment).unwrap();

    let working = activate_core_mission(&mut store);
    let proposal = ActionProposal {
        effect: EffectKind::FileWrite,
        mission_id: "mission-1".into(),
        mission_scope_digest: "scope-v1".into(),
        target: ActionTarget::MissionFile {
            relative_path: "reports/output.xlsx".into(),
        },
        estimated_cost_micros: None,
    };
    let payload = b"verified workbook";
    let resumed = approve_core_write(&mut store, working, &proposal, payload);
    let mut gate = ActionGate::default();
    gate.set_enabled(true);
    let permit = store
        .prepare_mission_file_put(
            &gate,
            "roundtrip-effect",
            &resumed.anchor,
            &proposal,
            payload,
            &session,
        )
        .unwrap();
    let receipt = engine.put_file(&permit, Cursor::new(payload)).unwrap();
    let authorization_anchor = openopen_core::AuditAnchor {
        sequence: permit.authorization_anchor.sequence,
        entry_hash: permit.authorization_anchor.entry_hash.clone(),
        signature_hex: permit.authorization_anchor.signature_hex.clone(),
    };
    let receipt_anchor = store
        .record_effect_receipt(&authorization_anchor, &session, &permit, &receipt)
        .unwrap();

    store.verify_audit_chain(&receipt_anchor).unwrap();
    assert_eq!(
        std::fs::read(root.path().join("mission-1/reports/output.xlsx")).unwrap(),
        payload
    );
}

#[test]
fn same_uid_contract_streaming_effect_fence_orders_outcome_before_pause() {
    let root = tempfile::tempdir().unwrap();
    std::fs::set_permissions(root.path(), Permissions::from_mode(0o700)).unwrap();
    let authority = LocalAuthority::from_master("openopen-core", [0x41; 32]);
    let broker_seed = [0x42; 32];
    let broker_key = SigningKey::from_bytes(&broker_seed)
        .verifying_key()
        .to_bytes();
    let mut enrollment_record = BrokerEnrollmentRecord {
        version: 1,
        broker_key_id: sha256_hex(&broker_key),
        broker_verifying_key_hex: hex::encode(broker_key),
        helper_designated_requirement_digest: "cd".repeat(32),
        installed_at_ms: 1,
        core_key_id: authority.effect_key_id(),
        core_authorization_signature_hex: String::new(),
    };
    let mut derivation = b"openopen-effect-authorizer-v1".to_vec();
    derivation.extend([0x41; 32]);
    let enrollment_signing_key = SigningKey::from_bytes(&Sha256::digest(derivation).into());
    enrollment_record.core_authorization_signature_hex = hex::encode(
        enrollment_signing_key
            .sign(&broker_enrollment_signing_bytes(&enrollment_record).unwrap())
            .to_bytes(),
    );
    let enrollment =
        TrustedBrokerEnrollment::from_signed_install_record(&authority, &enrollment_record)
            .unwrap();
    let now_ms = current_unix_ms().unwrap();
    let config = BrokerConfig {
        protected_root: std::fs::canonicalize(root.path()).unwrap(),
        authenticated_audit_euid: rustix::process::geteuid().as_raw(),
        enrolled_core_verifying_key: hex::decode(authority.effect_verifying_key_hex())
            .unwrap()
            .try_into()
            .unwrap(),
        broker_signing_seed: broker_seed,
        session_nonce: "ab".repeat(32),
        session_expires_at_ms: now_ms + 120_000,
    };
    let engine = BrokerEngine::open_internal(config).unwrap();
    let session = engine.broker_session();
    let mut store = Store::open_in_memory_with_trusted_broker(authority, enrollment).unwrap();
    let working = activate_core_mission(&mut store);
    let proposal = ActionProposal {
        effect: EffectKind::FileWrite,
        mission_id: "mission-1".into(),
        mission_scope_digest: "scope-v1".into(),
        target: ActionTarget::MissionFile {
            relative_path: "reports/streamed.bin".into(),
        },
        estimated_cost_micros: None,
    };
    let payload = vec![0x5a; 256 * 1024];
    let resumed = approve_core_write(&mut store, working, &proposal, &payload);
    let mut gate = ActionGate::default();
    gate.set_enabled(true);
    let permit = store
        .prepare_mission_file_put(
            &gate,
            "streaming-fence-effect",
            &resumed.anchor,
            &proposal,
            &payload,
            &session,
        )
        .unwrap();
    let authorization_anchor = AuditAnchor {
        sequence: permit.authorization_anchor.sequence,
        entry_hash: permit.authorization_anchor.entry_hash.clone(),
        signature_hex: permit.authorization_anchor.signature_hex.clone(),
    };
    let (reader, reached, resume_stream) = blocking_reader(payload.clone());
    let worker_permit = permit.clone();
    let worker = std::thread::spawn(move || {
        let mut engine = engine;
        engine.put_file(&worker_permit, reader)
    });
    reached.recv().unwrap();
    assert!(matches!(
        store.execute_mission_command(&pause_envelope(
            "pause-during-stream",
            authorization_anchor.clone(),
            9,
        )),
        Err(StoreError::EffectFenceActive(effect_id))
            if effect_id == "streaming-fence-effect"
    ));
    resume_stream.send(()).unwrap();
    let receipt = worker.join().unwrap().unwrap();
    let receipt_anchor = store
        .record_effect_receipt(&authorization_anchor, &session, &permit, &receipt)
        .unwrap();
    let paused = store
        .execute_mission_command(&pause_envelope("pause-after-stream", receipt_anchor, 10))
        .unwrap();
    store.verify_audit_chain(&paused.anchor).unwrap();
}
