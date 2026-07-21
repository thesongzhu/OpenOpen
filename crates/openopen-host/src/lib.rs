//! Production Rust host for `OpenOpen`'s local JSON-RPC surface.

mod channels;

use channels::{
    ChannelConnectionStatus, ChannelRuntime, ChannelRuntimeError, ChannelSendResult,
    TransportEvent, TransportInbound,
};

#[cfg(test)]
use openopen_codex_client::OutcomeRequest;
use openopen_codex_client::{
    AccountState, ChatGptLogin, ChoiceGenerationRequest, CodexClient, CodexError,
    CodexRuntimeConfig, GptModel, MEMORY_CANDIDATE_DEVELOPER_INSTRUCTIONS,
    MemoryCandidateGenerationRequest, SelectedModel, StructuredChoiceGeneration,
};
use openopen_core::{
    ActionGate, ActionProposal, ActionTarget, ApprovalDecision, AuditAnchor,
    B2MemoryPreparedSourceRecord, BrokerEnrollmentRecord, ChoiceIdleAdvance,
    ChoiceIdleClockEvidence, CreateMission, CreateWorkItem, EffectKind, EvidenceClaims,
    GateDecision, LocalAuthority, MarkdownRenderCleanup, MarkdownRenderPublication, MissionCommand,
    MissionCommandEnvelope, NewBoundaryApproval, NewReceipt, Store, StoreError,
    TrustedBrokerEnrollment, authorize_broker_enrollment, channel_message_payload,
    channel_need_you_content, channel_receipt_content, verify_core_instance_lease,
};
use openopen_deep_zip_worker::{DeepZipMemoryContext, DeepZipSupervisor};
use openopen_persona::{PersonaError, PersonaManager, PersonaStatus};
use openopen_protocol::ChannelRouteSet;
use openopen_protocol::{
    ApprovalKind, ApprovalStatus, ApprovalTarget, B2MemoryCandidateCard, B2MemoryCommand,
    B2MemoryCommandReceipt, B2MemoryDemoState, B2MemoryImportSeal, B2MemoryPrepareSourceRequest,
    B2MemoryPreparedSource, B2MemoryProcessingConsent, B2MemoryProcessingResult, BatchSealReason,
    C2SkillDemoCommand, C2SkillDemoState, ChannelCursor, ChannelDeliveryReceipt, ChannelEnvelope,
    ChannelInboundDecision, ChannelInboundResult, ChannelKind, ChannelMessageKind,
    ChannelModelDisposition, ChannelModelStart, ChannelObservation, ChannelOutboundDisposition,
    ChannelOutboundIntent, ChannelPairing, ChannelRouteApproval, ChoiceBeginAccepted,
    ChoiceBeginRecord, ChoiceBeginRequest, ChoiceConsolidatedConfirmation, ChoiceDInput,
    ChoiceDIntakeRecord, ChoiceIMessageReplyDisposition, ChoiceIMessageReplyIntent,
    ChoiceIMessageReplyPreview, ChoiceInitialResult, ChoiceLoopSnapshot, ChoiceOption,
    ChoiceRefinementOperation, ChoiceRefinementResult, ChoiceReminderItem, ChoiceReminderSchedule,
    ChoiceReminderScheduleInput, ChoiceResumeResult, ChoiceSession, ChoiceSessionState,
    ConversationTurnBatch, CoreInstanceLease, DiscordPairingMetadata, DocumentManifest,
    DocumentManifestEntry, EffectAuditAnchor, EvidenceKind, InterpretationFrame,
    MarkdownBaseIdentity, Mission, MissionStatus, ModelSelection, ModelSelectionState,
    OutcomeSuggestion, Receipt, RpcError, RpcRequest, RpcResponse, RuntimeControlAuthorization,
    RuntimeControlReceipt, Selection, SourceEnvelope, WorkItem, WorkItemStatus,
    canonical_choice_set_digest, canonical_document_manifest_digest,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};
use std::fs::{self, OpenOptions};
use std::io::{self, Read};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc::SyncSender as Sender,
};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use thiserror::Error;
use zeroize::{Zeroize, Zeroizing};

pub const BOOTSTRAP_MAGIC: &[u8] = b"OPENOPEN_BOOTSTRAP_V1\0";
const ISSUER_ID: &str = "openopen-local-owner";
const APP_SUPPORT_COMPONENT: &str = "com.thesongzhu.OpenOpen";
const CORE_LEASE_INSTALL_WINDOW_MS: i64 = 60_000;
const DEFAULT_REMINDERS_LIST_ID: &str = "openopen.default-reminders";
const REMINDER_WRITE_PAYLOAD_PREFIX: &[u8] = b"OPENOPEN_REMINDER_WRITE_V2\0";

#[derive(Debug, Error)]
pub enum HostError {
    #[error("production bundle layout is invalid")]
    InvalidBundle,
    #[error("application support path is invalid")]
    InvalidSupportPath,
    #[error("Core instance nonce generation failed")]
    InstanceNonce,
    #[error("host I/O failed")]
    Io(#[from] io::Error),
    #[error("persistent Store failed")]
    Store(#[from] StoreError),
    #[error("channel runtime failed to initialize")]
    ChannelRuntime,
    #[error("persona runtime failed to initialize")]
    Persona(#[from] PersonaError),
}

#[derive(Clone, Debug)]
pub struct HostPaths {
    pub store: PathBuf,
    pub codex_runtime: PathBuf,
    pub codex_home: PathBuf,
    pub synthetic_home: PathBuf,
    pub model_input_root: PathBuf,
    pub imsg_runtime: PathBuf,
    pub deep_zip_runtime: PathBuf,
    pub user_home: PathBuf,
    pub persona_root: PathBuf,
    pub persona_team_identifier: String,
}

impl HostPaths {
    /// Resolves production paths from the app bundle and user home. The Codex
    /// executable has no PATH, argument, or environment fallback.
    ///
    /// # Errors
    ///
    /// Returns an error if the bundle or app-support path is not exact.
    pub fn production() -> Result<Self, HostError> {
        let executable = exact_canonical_file(&std::env::current_exe()?)?;
        if executable.file_name().and_then(|value| value.to_str()) != Some("OpenOpenCore") {
            return Err(HostError::InvalidBundle);
        }
        let macos = executable.parent().ok_or(HostError::InvalidBundle)?;
        let contents = macos.parent().ok_or(HostError::InvalidBundle)?;
        let app = contents.parent().ok_or(HostError::InvalidBundle)?;
        if macos.file_name().and_then(|value| value.to_str()) != Some("MacOS")
            || contents.file_name().and_then(|value| value.to_str()) != Some("Contents")
            || app.extension().and_then(|value| value.to_str()) != Some("app")
        {
            return Err(HostError::InvalidBundle);
        }
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or(HostError::InvalidSupportPath)?;
        let home = exact_canonical_directory(&home)?;
        let support_parent = exact_canonical_directory(&home.join("Library/Application Support"))?;
        let support = support_parent.join(APP_SUPPORT_COMPONENT);
        create_exact_private_directory(&support)?;
        let runtime_home =
            PathBuf::from("/Library/Application Support/com.thesongzhu.OpenOpenRuntime/users")
                .join(rustix::process::geteuid().as_raw().to_string())
                .join("CodexHome");
        Ok(Self {
            store: support.join("openopen.sqlite3"),
            codex_runtime: contents.join("Resources/Codex/0.144.0/bin/codex"),
            codex_home: runtime_home.clone(),
            synthetic_home: runtime_home.join("SyntheticHome"),
            model_input_root: support.join("ModelInput"),
            imsg_runtime: contents.join("Resources/iMessage/0.13.0/bin/imsg"),
            deep_zip_runtime: contents.join("Resources/DeepZip/openopen-deep-zip-worker"),
            user_home: home,
            persona_root: support.join("Persona"),
            persona_team_identifier: current_executable_team_identifier().unwrap_or_default(),
        })
    }
}

fn current_executable_team_identifier() -> Option<String> {
    let executable = std::env::current_exe().ok()?;
    let output = Command::new("/usr/bin/codesign")
        .args(["-d", "--verbose=4"])
        .arg(executable)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let details = String::from_utf8_lossy(&output.stderr);
    details.lines().find_map(|line| {
        let (key, value) = line.split_once('=')?;
        (key == "TeamIdentifier"
            && value.len() == 10
            && value
                .bytes()
                .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit()))
        .then(|| value.to_owned())
    })
}

struct LoginSession {
    login: ChatGptLogin,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RuntimeAuthorityState {
    StartupUnknown,
    Enabled,
    OffLatched { minimum_on_revision: Option<u64> },
}

struct OperationGate {
    active: Option<Arc<AtomicBool>>,
    runtime: RuntimeAuthorityState,
}

impl Default for OperationGate {
    fn default() -> Self {
        Self {
            active: None,
            runtime: RuntimeAuthorityState::StartupUnknown,
        }
    }
}

const MODEL_CATALOG_SNAPSHOT_TTL_MS: i64 = 5 * 60 * 1_000;

struct ModelCatalogRequest<'a> {
    snapshot_id: &'a str,
    catalog_fingerprint: &'a str,
    catalog_revision: u64,
    runtime_revision: u64,
    now: i64,
}

/// A Host-owned, short-lived catalog result. The App receives only its opaque
/// identity and cannot manufacture a selection from catalog data of its own.
/// It is deliberately volatile: a Core restart, protected Off, or expiry
/// requires a fresh account/catalog scan before any selection can be saved.
#[derive(Clone)]
struct ModelCatalogSnapshot {
    id: String,
    account: AccountState,
    models: Vec<GptModel>,
    catalog_fingerprint: String,
    catalog_revision: u64,
    runtime_revision: u64,
    issued_at_ms: i64,
}

impl ModelCatalogSnapshot {
    fn matches_request(&self, request: &ModelCatalogRequest<'_>) -> bool {
        self.id == request.snapshot_id
            && self.catalog_fingerprint == request.catalog_fingerprint
            && self.catalog_revision == request.catalog_revision
            && self.runtime_revision == request.runtime_revision
            && request.now >= self.issued_at_ms
            && request.now.saturating_sub(self.issued_at_ms) <= MODEL_CATALOG_SNAPSHOT_TTL_MS
    }
}

#[derive(Clone, Default)]
struct OperationState {
    gate: Arc<Mutex<OperationGate>>,
    login: Arc<Mutex<Option<LoginSession>>>,
    suggestion: Arc<Mutex<Option<OutcomeSuggestion>>>,
    model_catalog_snapshot: Arc<Mutex<Option<ModelCatalogSnapshot>>>,
    runtime_challenge: Arc<Mutex<Option<String>>>,
    codex: Arc<Mutex<Option<CodexClient>>>,
    codex_pid: Arc<Mutex<Option<i32>>>,
    codex_cancel: Arc<AtomicBool>,
}

impl OperationState {
    fn has_active_operation(&self) -> bool {
        self.gate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .active
            .is_some()
    }

    fn codex_replacement_allowed(&self) -> bool {
        let gate = self
            .gate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let no_login = self
            .login
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_none();
        matches!(gate.runtime, RuntimeAuthorityState::Enabled) && gate.active.is_none() && no_login
    }

    fn authorize_codex_abort(&self) -> bool {
        let mut gate = self
            .gate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut login = self
            .login
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let authorized = match (gate.active.as_ref(), login.as_ref()) {
            (None, None) => true,
            (Some(token), Some(_)) => {
                token.store(true, Ordering::Release);
                *login = None;
                gate.active = None;
                true
            }
            _ => false,
        };
        drop(login);
        drop(gate);
        if authorized {
            self.clear_model_catalog_snapshot();
        }
        authorized
    }

    fn begin_operation(&self) -> Option<Arc<AtomicBool>> {
        let mut gate = self
            .gate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !matches!(gate.runtime, RuntimeAuthorityState::Enabled) || gate.active.is_some() {
            return None;
        }
        self.codex_cancel.store(false, Ordering::Release);
        let token = Arc::new(AtomicBool::new(false));
        gate.active = Some(token.clone());
        Some(token)
    }

    /// Serializes one irreversible provider initiation with protected Off.
    /// Holding the operation gate through the initiation means Off either
    /// wins before this closure (and no effect starts) or observes the effect
    /// as already initiated before it can latch the runtime Off.
    fn start_irreversible<T>(
        &self,
        token: &Arc<AtomicBool>,
        start: impl FnOnce() -> T,
    ) -> Option<T> {
        let gate = self
            .gate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if token.load(Ordering::Acquire)
            || !matches!(gate.runtime, RuntimeAuthorityState::Enabled)
            || !gate
                .active
                .as_ref()
                .is_some_and(|current| Arc::ptr_eq(current, token))
        {
            return None;
        }
        Some(start())
    }

    fn cancel_active(&self) {
        // A protected Off/cancel must be observable even if another path is
        // temporarily holding a secondary operation lock (for example while
        // installing a browser-login session). The worker checks this flag
        // before any later state publication.
        self.codex_cancel.store(true, Ordering::Release);
        let mut gate = self
            .gate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        gate.runtime = RuntimeAuthorityState::OffLatched {
            minimum_on_revision: None,
        };
        if let Some(token) = gate.active.as_ref() {
            token.store(true, Ordering::Release);
        }
        let pending_login = self
            .login
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
            .is_some();
        if pending_login {
            gate.active = None;
        }
        drop(gate);
        self.clear_model_catalog_snapshot();
    }

    /// Retires only the current model operation. Unlike protected Global Off,
    /// this leaves the signed runtime enabled so the caller can persist a
    /// terminal Choice cancellation and immediately regain a local next step.
    fn cancel_active_model_operation(&self) {
        let mut gate = self
            .gate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(token) = gate.active.take() {
            token.store(true, Ordering::Release);
        }
        self.codex_cancel.store(true, Ordering::Release);
    }

    fn latch_prepared_off(&self, revision: u64) {
        let mut gate = self
            .gate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let minimum_on_revision = match gate.runtime {
            RuntimeAuthorityState::OffLatched {
                minimum_on_revision: Some(current),
            } => current.max(revision),
            _ => revision,
        };
        gate.runtime = RuntimeAuthorityState::OffLatched {
            minimum_on_revision: Some(minimum_on_revision),
        };
    }

    fn accept_committed_runtime(&self, enabled: bool, revision: u64) {
        let mut gate = self
            .gate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if enabled {
            if runtime_on_may_release(gate.runtime, revision) {
                gate.runtime = RuntimeAuthorityState::Enabled;
            }
        } else {
            gate.runtime = RuntimeAuthorityState::OffLatched {
                minimum_on_revision: revision.checked_add(1),
            };
            self.codex_cancel.store(true, Ordering::Release);
            if let Some(token) = gate.active.as_ref() {
                token.store(true, Ordering::Release);
            }
        }
        drop(gate);
        if !enabled {
            self.clear_model_catalog_snapshot();
        }
    }

    fn accept_recovered_runtime(&self, enabled: bool, revision: u64) {
        let mut gate = self
            .gate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match (gate.runtime, enabled) {
            (RuntimeAuthorityState::StartupUnknown, true) => {
                gate.runtime = RuntimeAuthorityState::Enabled;
            }
            (_, false) => {
                gate.runtime = RuntimeAuthorityState::OffLatched {
                    minimum_on_revision: revision.checked_add(1),
                };
                self.codex_cancel.store(true, Ordering::Release);
                if let Some(token) = gate.active.as_ref() {
                    token.store(true, Ordering::Release);
                }
            }
            (RuntimeAuthorityState::OffLatched { .. }, true)
                if runtime_on_may_release(gate.runtime, revision) =>
            {
                gate.runtime = RuntimeAuthorityState::Enabled;
            }
            (RuntimeAuthorityState::Enabled | RuntimeAuthorityState::OffLatched { .. }, true) => {}
        }
        drop(gate);
        if !enabled {
            self.clear_model_catalog_snapshot();
        }
    }

    fn install_login(&self, token: &Arc<AtomicBool>, login: ChatGptLogin) -> bool {
        let gate = self
            .gate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if token.load(Ordering::Acquire)
            || !gate
                .active
                .as_ref()
                .is_some_and(|current| Arc::ptr_eq(current, token))
        {
            return false;
        }
        let mut pending = self
            .login
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *pending = Some(LoginSession { login });
        true
    }

    fn take_login_operation(&self) -> Option<(LoginSession, Arc<AtomicBool>)> {
        let gate = self
            .gate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let session = self
            .login
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()?;
        let token = gate.active.clone()?;
        Some((session, token))
    }

    fn finish_operation(&self, token: &Arc<AtomicBool>) {
        let mut gate = self
            .gate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if gate
            .active
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, token))
        {
            gate.active = None;
        }
    }

    fn reconcile_active<T>(
        &self,
        token: &Arc<AtomicBool>,
        reconcile: impl FnOnce() -> Result<T, HostCallError>,
    ) -> Result<T, HostCallError> {
        let gate = self
            .gate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if token.load(Ordering::Acquire)
            || !matches!(gate.runtime, RuntimeAuthorityState::Enabled)
            || !gate
                .active
                .as_ref()
                .is_some_and(|current| Arc::ptr_eq(current, token))
        {
            return Err(HostCallError::Codex(CodexError::Cancelled));
        }
        // Hold the operation gate through the Store transaction and volatile
        // publication. Global Off must acquire this same gate before it can
        // cancel/latch the operation, while Store independently verifies the
        // signed On row inside its immediate write transaction.
        reconcile()
    }

    fn publish_model_catalog_snapshot(
        &self,
        token: &Arc<AtomicBool>,
        snapshot: ModelCatalogSnapshot,
    ) -> bool {
        let gate = self
            .gate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if token.load(Ordering::Acquire)
            || !matches!(gate.runtime, RuntimeAuthorityState::Enabled)
            || !gate
                .active
                .as_ref()
                .is_some_and(|current| Arc::ptr_eq(current, token))
        {
            return false;
        }
        *self
            .model_catalog_snapshot
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(snapshot);
        true
    }

    fn reconcile_active_model_catalog<T>(
        &self,
        token: &Arc<AtomicBool>,
        request: &ModelCatalogRequest<'_>,
        reconcile: impl FnOnce(&ModelCatalogSnapshot) -> Result<T, HostCallError>,
    ) -> Result<T, HostCallError> {
        let gate = self
            .gate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if token.load(Ordering::Acquire)
            || !matches!(gate.runtime, RuntimeAuthorityState::Enabled)
            || !gate
                .active
                .as_ref()
                .is_some_and(|current| Arc::ptr_eq(current, token))
        {
            return Err(HostCallError::Codex(CodexError::Cancelled));
        }
        let snapshot = self
            .model_catalog_snapshot
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !snapshot
            .as_ref()
            .is_some_and(|value| value.matches_request(request))
        {
            return Err(HostCallError::Codex(CodexError::RequiredModelUnavailable));
        }
        // The operation gate remains held through the Store transaction, so
        // protected Off cannot race a stale snapshot into durable readiness.
        reconcile(snapshot.as_ref().expect("validated model catalog snapshot"))
    }

    /// Holds the same operation gate used by catalog selection while a
    /// first-local-question intake verifies its persisted model binding and
    /// commits the initial session. The caller cannot race protected Off or a
    /// catalog/account drift into durable model readiness.
    fn reconcile_active_model_selection<T>(
        &self,
        token: &Arc<AtomicBool>,
        selection: &ModelSelection,
        runtime_revision: u64,
        now: i64,
        reconcile: impl FnOnce() -> Result<T, HostCallError>,
    ) -> Result<T, HostCallError> {
        let gate = self
            .gate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if token.load(Ordering::Acquire)
            || !matches!(gate.runtime, RuntimeAuthorityState::Enabled)
            || !gate
                .active
                .as_ref()
                .is_some_and(|current| Arc::ptr_eq(current, token))
        {
            return Err(HostCallError::Codex(CodexError::Cancelled));
        }
        let snapshot = self
            .model_catalog_snapshot
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let current = snapshot.as_ref().is_some_and(|value| {
            value.runtime_revision == runtime_revision
                && now >= value.issued_at_ms
                && now.saturating_sub(value.issued_at_ms) <= MODEL_CATALOG_SNAPSHOT_TTL_MS
                && model_selection_status(
                    &value.account,
                    &value.models,
                    Some(selection),
                    &value.catalog_fingerprint,
                    value.catalog_revision,
                ) == ModelSelectionStatus::Current
        });
        if !current {
            return Err(HostCallError::Codex(CodexError::RequiredModelUnavailable));
        }
        reconcile()
    }

    fn clear_model_catalog_snapshot(&self) {
        self.model_catalog_snapshot
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
    }
}

fn runtime_on_may_release(state: RuntimeAuthorityState, revision: u64) -> bool {
    match state {
        RuntimeAuthorityState::StartupUnknown | RuntimeAuthorityState::Enabled => true,
        RuntimeAuthorityState::OffLatched {
            minimum_on_revision: Some(minimum),
        } => revision >= minimum,
        RuntimeAuthorityState::OffLatched {
            minimum_on_revision: None,
        } => false,
    }
}

pub struct Host {
    store: Store,
    authority: LocalAuthority,
    paths: HostPaths,
    operations: OperationState,
    instance_nonce: String,
    idle_boot_id: String,
    instance_lease: Arc<Mutex<Option<CoreInstanceLease>>>,
    channels: Mutex<ChannelRuntime>,
    persona: Arc<Mutex<PersonaManager>>,
    // Unit fixtures retain direct coverage of historical recovery records.
    // This is never enabled by `Host::open`, so a shipped Host cannot expose
    // a deferred channel route during the PR1 local-Choice stage.
    allow_pr1_deferred_channel_routes_for_tests: bool,
}

impl Host {
    /// Opens the signed persistent Store with Keychain-sourced master material.
    ///
    /// # Errors
    ///
    /// Returns an error for unsafe paths or Store initialization failure.
    pub fn open(paths: HostPaths, master: [u8; 32]) -> Result<Self, HostError> {
        let master = Zeroizing::new(master);
        // The root broker owns creation of the fixed tmpfs Codex home. Core
        // must remain able to open its Store and report protected Off while
        // that broker-managed mount is absent. The Codex client validates the
        // exact mount and only then creates the nested synthetic home during
        // `broker.codex.prepare`.
        create_exact_private_directory(&paths.model_input_root)?;
        let mut nonce = [0_u8; 32];
        getrandom::fill(&mut nonce).map_err(|_| HostError::InstanceNonce)?;
        let authority = LocalAuthority::from_master(ISSUER_ID, *master);
        let mut store = Store::open(&paths.store, authority.clone())?;
        store.bind_choice_markdown_root(&paths.user_home)?;
        let idle_boot_id = stable_idle_boot_id()?;
        let persona = PersonaManager::open_default_only(
            &paths.persona_root,
            1,
            paths.persona_team_identifier.clone(),
        )?;
        Ok(Self {
            store,
            authority,
            paths,
            operations: OperationState::default(),
            instance_nonce: hex::encode(nonce),
            idle_boot_id,
            instance_lease: Arc::new(Mutex::new(None)),
            channels: Mutex::new(ChannelRuntime::new().map_err(|_| HostError::ChannelRuntime)?),
            persona: Arc::new(Mutex::new(persona)),
            allow_pr1_deferred_channel_routes_for_tests: false,
        })
    }

    #[allow(clippy::too_many_lines)] // The RPC dispatch table is deliberately auditable in one place.
    pub fn handle_line(&mut self, line: &str, responses: &Sender<RpcResponse>) {
        let Some(request) = parse_request(line, responses) else {
            return;
        };
        if request.jsonrpc != "2.0" {
            let _ = responses.send(invalid_request(Some(request.id)));
            return;
        }
        // PR1 owns the local Choice Core only. Channel pairing/setup, polling,
        // outbound, and route binding begin in later independently reviewed
        // stages; they are not a compatibility path around Choice authority.
        // Unit fixtures retain direct coverage of historical Store recovery,
        // but the production Host never exposes these mutating routes.
        if is_pr1_deferred_channel_route(&request.method)
            && !self.allow_pr1_deferred_channel_routes_for_tests
        {
            let _ = responses.send(not_ready(request.id));
            return;
        }
        match request.method.as_str() {
            "mission.runtime.read" => {
                let response = self.store.runtime_control().map_or_else(
                    |error| host_failure(request.id, &error),
                    |value| success(request.id, value),
                );
                let _ = responses.send(response);
            }
            "mission.runtime.challenge" => self.issue_runtime_challenge(&request, responses),
            "mission.runtime.prepare" => self.prepare_runtime(&request, responses),
            "mission.runtime.commit" => self.commit_runtime(&request, responses),
            "mission.runtime.recover" => self.recover_runtime(&request, responses),
            "broker.identity.read" => self.read_broker_identity(&request, responses),
            "broker.enrollment.sign" => self.sign_broker_enrollment(&request, responses),
            "broker.enrollment.install" => self.install_broker(&request, responses),
            "broker.codex.prepare" => self.prepare_codex_runtime(&request, responses),
            "broker.codex.login.prepare" => {
                self.prepare_login_codex_runtime(&request, responses);
            }
            "broker.codex.candidate.bind" => {
                self.bind_codex_candidate_for_broker(&request, responses);
            }
            "broker.codex.initialize" => self.initialize_codex_runtime(&request, responses),
            "broker.codex.abort" => self.abort_codex_candidate(&request, responses),
            "broker.lease.install" => self.install_core_lease(&request, responses),
            "mission.dashboard.read" => self.read_dashboard(&request, responses),
            "channel.pair" => self.pair_channel(&request, responses),
            "channel.pairing.read" => self.read_channel_pairing(&request, responses),
            "channel.route.bind" => self.bind_channel_route(&request, responses),
            "channel.discord.setup.start" => self.start_discord_setup(&request, responses),
            "channel.discord.setup.poll" => self.poll_discord_setup(&request, responses),
            "channel.discord.setup.confirm" => self.confirm_discord_setup(&request, responses),
            "channel.discord.start" => self.start_discord(&request, responses),
            "channel.imessage.chats.prepare" => {
                self.prepare_imessage_chat_discovery(&request, responses);
            }
            "channel.imessage.chats.list" => self.list_imessage_chat_discovery(&request, responses),
            "channel.imessage.prepare" => self.prepare_imessage(&request, responses),
            "channel.imessage.activate" => self.activate_imessage(&request, responses),
            "channel.poll" => self.poll_channel(&request, responses),
            "channel.failure.acknowledge" => {
                self.acknowledge_channel_failure(&request, responses);
            }
            "channel.status" => self.channel_status(&request, responses),
            "channel.stop" => self.stop_channel(&request, responses),
            "channel.outbound.send" => self.send_channel_message(&request, responses),
            "account.read" => self.start_account_read(&request, responses),
            "account.login.start" => self.start_login(&request, responses),
            "account.login.await" => self.await_login(&request, responses),
            "models.list" => self.start_model_list(&request, responses),
            "models.setup.read" => self.read_model_setup(&request, responses),
            "models.selection.read" => self.read_selected_model(&request, responses),
            "models.select" => self.select_model(&request, responses),
            "persona.status" => self.read_persona_status(&request, responses),
            "skill.demo.read" => self.read_c2_skill_demo(&request, responses),
            "skill.demo.command" => self.command_c2_skill_demo(&request, responses),
            "memory.demo.read" => self.read_b2_memory_demo(&request, responses),
            "memory.demo.command" => self.command_b2_memory_demo(&request, responses),
            "memory.demo.source.prepare" => {
                self.prepare_b2_memory_source(&request, responses);
            }
            "memory.demo.source.process" => {
                self.process_b2_memory_source(&request, responses);
            }
            "choice.loop.read" => self.read_choice_loop(&request, responses),
            "choice.reminder.schedule.read" => {
                self.read_choice_reminder_schedule(&request, responses);
            }
            "choice.begin" => self.begin_choice(&request, responses),
            "choice.cancel" => self.cancel_choice(&request, responses),
            "choice.select" => self.select_choice(&request, responses),
            "choice.resume" => self.resume_choice(&request, responses),
            "choice.markdown.reconcile" => self.reconcile_choice_markdown(&request, responses),
            "choice.markdown.receipt.cleanup" => {
                self.cleanup_choice_markdown_receipt(&request, responses);
            }
            "choice.markdown.receipt.cleanup.available" => {
                self.read_choice_markdown_receipt_cleanup_availability(&request, responses);
            }
            "choice.reminder.schedule" => self.record_choice_reminder_schedule(&request, responses),
            "choice.confirm.prepare" => self.prepare_choice_confirmation(&request, responses),
            "choice.confirm" => self.confirm_choice(&request, responses),
            "choice.imessage.reply.prepare" => {
                self.prepare_choice_imessage_reply(&request, responses);
            }
            "choice.imessage.reply.authorize" => {
                self.authorize_choice_imessage_reply(&request, responses);
            }
            "choice.reminders.authorize" => {
                self.authorize_choice_reminders(&request, responses);
            }
            "choice.reminders.begin" => self.begin_choice_reminder_dispatch(&request, responses),
            "choice.reminders.abort-before-commit" => {
                self.abort_choice_reminder_dispatch_before_commit(&request, responses);
            }
            "choice.reminders.record" => self.record_choice_reminder_mirror(&request, responses),
            "choice.reminders.complete" => self.complete_choice_reminders(&request, responses),
            // Choice confirmation is the only current foreground authority.
            // PR1 may read a historical Mission/Receipt dashboard, but no
            // public Mission command can create, cancel, complete, record
            // Evidence, begin Reminder work, or send a route effect.
            "mission.confirm"
            | "mission.cancel"
            | "mission.reminders.begin"
            | "mission.reminders.record"
            | "mission.reminders.complete" => {
                let _ = responses.send(not_ready(request.id));
            }
            method if is_public_family(method) => {
                let _ = responses.send(not_ready(request.id));
            }
            _ => {
                let _ = responses.send(failure(
                    Some(request.id),
                    -32_601,
                    "Unknown OpenOpen RPC method",
                ));
            }
        }
    }

    fn prepare_runtime(&mut self, request: &RpcRequest, responses: &Sender<RpcResponse>) {
        let Ok(params) = decode_params::<SetEnabled>(request) else {
            let _ = responses.send(invalid_params(request.id));
            return;
        };
        if params.enabled && !self.has_instance_lease() {
            let _ = responses.send(failure(
                Some(request.id),
                -32_015,
                "Core has no protected instance lease",
            ));
            return;
        }
        if !params.enabled {
            self.channels
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .stop_all();
            *self
                .operations
                .runtime_challenge
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
            self.cancel_active();
        }
        let result = now_ms().and_then(|now| {
            if !params.enabled {
                // Off is an immediate cancellation boundary for an unreceipted
                // local Choice journal. Retire the encrypted body while the
                // current signed On revision is still authoritative; leaving
                // it behind would strand it after Core quiesces with no legal
                // publication or cancellation route.
                if let Some(snapshot) = self.store.choice_loop_snapshot()?
                    && !matches!(
                        snapshot.session.state,
                        ChoiceSessionState::Completed | ChoiceSessionState::Cancelled
                    )
                {
                    let runtime = self.store.runtime_control()?;
                    self.store.cancel_choice_session(runtime.revision, now)?;
                }
            }
            self.store
                .prepare_runtime_control(params.enabled, now)
                .map_err(HostCallError::Store)
        });
        if let Ok(authorization) = &result
            && !authorization.enabled
        {
            self.operations.latch_prepared_off(authorization.revision);
        }
        let response = result.map_or_else(
            |error| call_failure(request.id, &error),
            |value| success(request.id, value),
        );
        let _ = responses.send(response);
    }

    fn read_broker_identity(&self, request: &RpcRequest, responses: &Sender<RpcResponse>) {
        if decode_params::<NoParams>(request).is_err() {
            let _ = responses.send(invalid_params(request.id));
            return;
        }
        let _ = responses.send(success(
            request.id,
            json!({
                "coreKeyId": self.authority.effect_key_id(),
                "coreVerifyingKeyHex": self.authority.effect_verifying_key_hex(),
                "coreInstanceNonce": self.instance_nonce,
                "corePid": std::process::id(),
            }),
        ));
    }

    fn prepare_codex_runtime(&self, request: &RpcRequest, responses: &Sender<RpcResponse>) {
        if decode_params::<NoParams>(request).is_err()
            || self.store.trusted_broker_enrollment().is_none()
        {
            let _ = responses.send(invalid_params(request.id));
            return;
        }
        let result = (|| -> Result<i32, HostCallError> {
            let mut slot = self
                .operations
                .codex
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(client) = slot.as_ref() {
                if client.is_login_only() {
                    return Err(HostCallError::Internal);
                }
                return Ok(client.process_identifier());
            }
            self.operations.codex_cancel.store(false, Ordering::Release);
            let client = CodexClient::spawn_uninitialized_with_cancel(
                &CodexRuntimeConfig {
                    runtime: self.paths.codex_runtime.clone(),
                    codex_home: self.paths.codex_home.clone(),
                    synthetic_home: self.paths.synthetic_home.clone(),
                    model_workspace: self.paths.model_input_root.clone(),
                    user_home: self.paths.user_home.clone(),
                },
                self.operations.codex_cancel.clone(),
            )
            .map_err(HostCallError::Codex)?;
            let pid = client.process_identifier();
            *slot = Some(client);
            *self
                .operations
                .codex_pid
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(pid);
            Ok(pid)
        })();
        let _ = responses.send(result.map_or_else(
            |error| call_failure(request.id, &error),
            |codex_pid| success(request.id, json!({"codexPid": codex_pid})),
        ));
    }

    fn prepare_login_codex_runtime(&self, request: &RpcRequest, responses: &Sender<RpcResponse>) {
        if decode_params::<NoParams>(request).is_err()
            || self.store.trusted_broker_enrollment().is_none()
            || !self.operations.codex_replacement_allowed()
        {
            let _ = responses.send(invalid_params(request.id));
            return;
        }
        let result = (|| -> Result<i32, HostCallError> {
            // A failed, cancelled, or completed login deliberately retires the
            // local lease before a fresh model/login process is prepared. The
            // protected broker still owns the durable old audit-token lease;
            // spawning this uninitialized candidate lets App ask the broker to
            // terminate/release that exact old incarnation and issue the new
            // lease. No account, model, or login request can run before the new
            // signed lease is installed and initialization completes.
            self.retire_codex_runtime();
            self.operations.codex_cancel.store(false, Ordering::Release);
            let client = CodexClient::spawn_login_uninitialized_with_cancel(
                &CodexRuntimeConfig {
                    runtime: self.paths.codex_runtime.clone(),
                    codex_home: self.paths.codex_home.clone(),
                    synthetic_home: self.paths.synthetic_home.clone(),
                    model_workspace: self.paths.model_input_root.clone(),
                    user_home: self.paths.user_home.clone(),
                },
                self.operations.codex_cancel.clone(),
            )
            .map_err(HostCallError::Codex)?;
            let pid = client.process_identifier();
            *self
                .operations
                .codex
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(client);
            *self
                .operations
                .codex_pid
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(pid);
            Ok(pid)
        })();
        let _ = responses.send(result.map_or_else(
            |error| call_failure(request.id, &error),
            |codex_pid| success(request.id, json!({"codexPid": codex_pid})),
        ));
    }

    fn initialize_codex_runtime(&self, request: &RpcRequest, responses: &Sender<RpcResponse>) {
        if decode_params::<NoParams>(request).is_err() || !self.has_instance_lease() {
            let _ = responses.send(invalid_params(request.id));
            return;
        }
        let result = self
            .operations
            .codex
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_mut()
            .ok_or(HostCallError::Internal)
            .and_then(|client| {
                if client.is_initialized() {
                    Ok(())
                } else {
                    client.complete_initialize().map_err(HostCallError::Codex)
                }
            });
        let _ = responses.send(result.map_or_else(
            |error| call_failure(request.id, &error),
            |()| success(request.id, json!({"status": "initialized"})),
        ));
    }

    fn bind_codex_candidate_for_broker(
        &self,
        request: &RpcRequest,
        responses: &Sender<RpcResponse>,
    ) {
        if decode_params::<NoParams>(request).is_err()
            || self.store.trusted_broker_enrollment().is_none()
        {
            let _ = responses.send(invalid_params(request.id));
            return;
        }
        let result = (|| -> Result<(), HostCallError> {
            let mut codex = self
                .operations
                .codex
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let client = codex.as_mut().ok_or(HostCallError::Internal)?;
            let installed = self
                .instance_lease
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if client.is_initialized()
                && installed
                    .as_ref()
                    .is_none_or(|lease| lease.codex_pid != client.process_identifier())
            {
                return Err(HostCallError::Internal);
            }
            // This one-way handoff occurs before App may ask the broker to
            // durably acquire the exact audit-token lease. Therefore broker
            // response loss and the later Core-install request/response loss
            // can never restore numeric-PID signal authority: abort/drop is
            // pipe-close plus wait-only, and broker recovery owns termination.
            client.mark_process_lease_bound();
            Ok(())
        })();
        let _ = responses.send(result.map_or_else(
            |_| failure(Some(request.id), -32_015, "Codex broker handoff rejected"),
            |()| success(request.id, json!({"status": "bound"})),
        ));
    }

    fn abort_codex_candidate(&self, request: &RpcRequest, responses: &Sender<RpcResponse>) {
        if decode_params::<NoParams>(request).is_err() || !self.operations.authorize_codex_abort() {
            let _ = responses.send(invalid_params(request.id));
            return;
        }
        self.retire_codex_runtime();
        let _ = responses.send(success(request.id, json!({"status": "aborted"})));
    }

    fn sign_broker_enrollment(&self, request: &RpcRequest, responses: &Sender<RpcResponse>) {
        let Ok(params) = decode_params::<SignBrokerEnrollment>(request) else {
            let _ = responses.send(invalid_params(request.id));
            return;
        };
        let response = authorize_broker_enrollment(
            &self.authority,
            params.broker_key_id,
            params.broker_verifying_key_hex,
            params.helper_designated_requirement_digest,
            params.installed_at_ms,
        )
        .map_or_else(
            |_| {
                failure(
                    Some(request.id),
                    -32_014,
                    "Broker enrollment signing failed",
                )
            },
            |record| success(request.id, record),
        );
        let _ = responses.send(response);
    }

    fn issue_runtime_challenge(&self, request: &RpcRequest, responses: &Sender<RpcResponse>) {
        if decode_params::<NoParams>(request).is_err() {
            let _ = responses.send(invalid_params(request.id));
            return;
        }
        let mut bytes = [0_u8; 32];
        let response = if getrandom::fill(&mut bytes).is_ok() {
            let challenge = hex::encode(bytes);
            *self
                .operations
                .runtime_challenge
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(challenge.clone());
            success(request.id, json!({"challenge": challenge}))
        } else {
            failure(
                Some(request.id),
                -32_014,
                "Runtime challenge generation failed",
            )
        };
        let _ = responses.send(response);
    }

    fn commit_runtime(&mut self, request: &RpcRequest, responses: &Sender<RpcResponse>) {
        let Ok(params) = decode_params::<CommitRuntime>(request) else {
            let _ = responses.send(invalid_params(request.id));
            return;
        };
        if params.authorization.enabled && !self.has_instance_lease() {
            let _ = responses.send(failure(
                Some(request.id),
                -32_015,
                "Core has no protected instance lease",
            ));
            return;
        }
        let response = match self
            .store
            .commit_runtime_control(&params.authorization, &params.broker_receipt)
        {
            Ok(value) => {
                self.operations
                    .accept_committed_runtime(value.enabled, value.revision);
                success(request.id, value)
            }
            Err(error) => host_failure(request.id, &error),
        };
        let _ = responses.send(response);
    }

    fn recover_runtime(&mut self, request: &RpcRequest, responses: &Sender<RpcResponse>) {
        let Ok(params) = decode_params::<CommitRuntime>(request) else {
            let _ = responses.send(invalid_params(request.id));
            return;
        };
        if params.authorization.enabled && !self.has_instance_lease() {
            let _ = responses.send(failure(
                Some(request.id),
                -32_015,
                "Core has no protected instance lease",
            ));
            return;
        }
        let response = match self
            .store
            .recover_runtime_control(&params.authorization, &params.broker_receipt)
        {
            Ok(value) => {
                self.operations
                    .accept_recovered_runtime(value.enabled, value.revision);
                success(request.id, value)
            }
            Err(error) => host_failure(request.id, &error),
        };
        let _ = responses.send(response);
    }

    fn install_broker(&mut self, request: &RpcRequest, responses: &Sender<RpcResponse>) {
        let Ok(params) = decode_params::<InstallBroker>(request) else {
            let _ = responses.send(invalid_params(request.id));
            return;
        };
        let response = self
            .store
            .install_trusted_broker(&params.record)
            .map_or_else(
                |error| host_failure(request.id, &error),
                |()| success(request.id, json!({"status": "installed"})),
            );
        let _ = responses.send(response);
    }

    fn install_core_lease(&self, request: &RpcRequest, responses: &Sender<RpcResponse>) {
        let Ok(params) = decode_params::<InstallCoreLease>(request) else {
            let _ = responses.send(invalid_params(request.id));
            return;
        };
        let result = (|| -> Result<(), HostCallError> {
            let enrollment = self
                .store
                .trusted_broker_enrollment()
                .ok_or(StoreError::MissingTrustedBrokerEnrollment)?;
            verify_core_instance_lease(enrollment, &params.lease)
                .map_err(|_| HostCallError::Internal)?;
            let now = now_ms()?;
            let current_pid =
                i32::try_from(std::process::id()).map_err(|_| HostCallError::Internal)?;
            if params.lease.audit_euid != rustix::process::geteuid().as_raw()
                || params.lease.core_pid != current_pid
                || self
                    .operations
                    .codex_pid
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .is_none_or(|pid| pid != params.lease.codex_pid)
                || params.lease.core_instance_nonce != self.instance_nonce
                || params.lease.issued_at_ms > now
                || now.saturating_sub(params.lease.issued_at_ms) > CORE_LEASE_INSTALL_WINDOW_MS
            {
                return Err(HostCallError::Internal);
            }
            let mut codex = self
                .operations
                .codex
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(client) = codex.as_mut() {
                if client.process_identifier() != params.lease.codex_pid {
                    return Err(HostCallError::Internal);
                }
                // The broker has already issued a lease for this exact Codex
                // audit-token incarnation. Remove Core's numeric-signal
                // authority before publishing the lease locally; every later
                // success, failure, cancellation, Global Off, or drop is
                // pipe-close/reap only and the broker performs any required
                // exact termination. A vacant slot has no process authority
                // to revoke and remains fail-closed for every Codex route.
                client.mark_process_lease_bound();
            }
            let mut installed = self
                .instance_lease
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if installed
                .as_ref()
                .is_some_and(|existing| existing != &params.lease)
            {
                return Err(HostCallError::Internal);
            }
            *installed = Some(params.lease);
            Ok(())
        })();
        let _ = responses.send(result.map_or_else(
            |_| failure(Some(request.id), -32_015, "Protected Core lease rejected"),
            |()| success(request.id, json!({"status": "installed"})),
        ));
    }

    fn read_dashboard(&self, request: &RpcRequest, responses: &Sender<RpcResponse>) {
        if decode_params::<NoParams>(request).is_err() {
            let _ = responses.send(invalid_params(request.id));
            return;
        }
        let result = self.dashboard_value();
        let response = result.map_or_else(
            |error| call_failure(request.id, &error),
            |dashboard| success(request.id, dashboard),
        );
        let _ = responses.send(response);
    }

    fn dashboard_value(&self) -> Result<Value, HostCallError> {
        let runtime = self.store.runtime_control()?;
        let mut suggestion = self.visible_dashboard_suggestion()?;
        let incidents = self.store.channel_failure_incident_projection(None)?;
        let Some(anchor) = self.store.current_verified_audit_anchor()? else {
            return Ok(empty_dashboard(runtime, suggestion.as_ref(), incidents));
        };
        let missions = self.store.list_missions(&anchor)?;
        let confirmed = self.dashboard_confirmed_missions(&missions)?;
        if suggestion.as_ref().is_some_and(|candidate| {
            mission_id_for_suggestion_id(&candidate.id).is_ok_and(|mission_id| {
                confirmed
                    .iter()
                    .any(|mission| mission.mission_id == mission_id)
            })
        }) {
            suggestion = None;
        }
        let nonterminal = missions
            .iter()
            .filter(|mission| !mission.status.is_terminal())
            .collect::<Vec<_>>();
        if !nonterminal.is_empty() {
            suggestion = None;
        }
        let focus = dashboard_focus(&nonterminal, &confirmed);
        let focus_id = focus.map(|mission| mission.id.as_str());
        let confirmed_mission = focus_id.and_then(|mission_id| {
            confirmed
                .iter()
                .find(|candidate| candidate.mission_id == mission_id)
        });
        let receipt = (nonterminal.is_empty() && suggestion.is_none())
            .then(|| self.store.list_receipts(&anchor))
            .transpose()?
            .and_then(|receipts| receipts.into_iter().next());
        let route_mission_id =
            focus_id.or_else(|| receipt.as_ref().map(|value| value.mission_id.as_str()));
        let channel_route_set = route_mission_id
            .map(|mission_id| self.store.channel_route_set(mission_id))
            .transpose()?
            .flatten();
        Ok(json!({
            "activeCards": dashboard_active_cards(&nonterminal, focus),
            "channelFailureIncidents": incidents,
            "channelRouteSet": channel_route_set,
            "confirmedMission": confirmed_mission,
            "microphone": {"available": false, "reason": "Microphone unavailable until Voice setup"},
            "needsYou": dashboard_needs_you(focus),
            "receipt": receipt,
            "runtime": runtime,
            "suggestion": suggestion
        }))
    }

    fn dashboard_confirmed_missions(
        &self,
        missions: &[Mission],
    ) -> Result<Vec<ConfirmedMission>, HostCallError> {
        missions
            .iter()
            .map(|mission| {
                confirmed_mission_from_mission(
                    mission,
                    &self.authority,
                    ReminderWriteDisposition::RecoverOnly,
                )
            })
            .filter_map(Result::transpose)
            .collect()
    }

    fn pair_channel(&mut self, request: &RpcRequest, responses: &Sender<RpcResponse>) {
        let Ok(params) = decode_params::<PairChannel>(request) else {
            let _ = responses.send(invalid_params(request.id));
            return;
        };
        let result = if params.pairing.channel != ChannelKind::IMessage
            || params.pairing.require_explicit_address
            || params.pairing.imessage.is_none()
            || params.pairing.discord.is_some()
        {
            Err(StoreError::ChannelPairingConflict)
        } else {
            self.store
                .require_runtime_checkpoint(&params.authorization, &params.broker_receipt)
                .and_then(|()| self.store.pair_channel(&params.pairing))
        };
        let _ = responses.send(result.map_or_else(
            |error| host_failure(request.id, &error),
            |()| success(request.id, json!({"status": "paired"})),
        ));
    }

    fn read_c2_skill_demo(&mut self, request: &RpcRequest, responses: &Sender<RpcResponse>) {
        let Ok(proof) = decode_params::<RuntimeProof>(request) else {
            let _ = responses.send(invalid_params(request.id));
            return;
        };
        if !self.validate_store_control_proof(request.id, &proof, responses) {
            return;
        }
        let result = self.store.c2_skill_demo_state();
        let _ = responses.send(result.map_or_else(
            |error| host_failure(request.id, &error),
            |state| success(request.id, C2SkillDemoView { state }),
        ));
    }

    fn command_c2_skill_demo(&mut self, request: &RpcRequest, responses: &Sender<RpcResponse>) {
        let Ok(params) = decode_params::<ApplyC2SkillDemo>(request) else {
            let _ = responses.send(invalid_params(request.id));
            return;
        };
        if !params.command.is_valid() {
            let _ = responses.send(invalid_params(request.id));
            return;
        }
        if !self.validate_store_control_proof(request.id, &params.proof(), responses) {
            return;
        }
        let result = self.store.apply_c2_skill_demo_command(
            &params.command,
            &params.authorization,
            &params.broker_receipt,
        );
        let _ = responses.send(result.map_or_else(
            |error| host_failure(request.id, &error),
            |(state, receipt)| success(request.id, json!({"state": state, "receipt": receipt})),
        ));
    }

    fn read_b2_memory_demo(&mut self, request: &RpcRequest, responses: &Sender<RpcResponse>) {
        let Ok(proof) = decode_params::<RuntimeProof>(request) else {
            let _ = responses.send(invalid_params(request.id));
            return;
        };
        if !self.validate_store_control_proof(request.id, &proof, responses) {
            return;
        }
        let result = self.store.b2_memory_demo_state();
        let _ = responses.send(result.map_or_else(
            |error| host_failure(request.id, &error),
            |state| success(request.id, B2MemoryDemoView { state }),
        ));
    }

    fn command_b2_memory_demo(&mut self, request: &RpcRequest, responses: &Sender<RpcResponse>) {
        let Ok(params) = decode_params::<ApplyB2MemoryDemo>(request) else {
            let _ = responses.send(invalid_params(request.id));
            return;
        };
        if !params.command.is_valid() {
            let _ = responses.send(invalid_params(request.id));
            return;
        }
        if !self.validate_store_control_proof(request.id, &params.proof(), responses) {
            return;
        }
        let result = self.store.apply_b2_memory_command(
            &params.command,
            &params.authorization,
            &params.broker_receipt,
        );
        let _ = responses.send(result.map_or_else(
            |error| host_failure(request.id, &error),
            |(state, receipt)| success(request.id, json!({"state": state, "receipt": receipt})),
        ));
    }

    fn prepare_b2_memory_source(&mut self, request: &RpcRequest, responses: &Sender<RpcResponse>) {
        let Ok(params) = decode_params::<PrepareB2MemorySource>(request) else {
            let _ = responses.send(invalid_params(request.id));
            return;
        };
        if !params.request.is_valid() {
            let _ = responses.send(invalid_params(request.id));
            return;
        }
        let Some(operation) = self.begin_operation(request.id, &params.proof(), responses) else {
            return;
        };
        let context = self.background_context(params.proof());
        let responses = responses.clone();
        let request_id = request.id;
        std::thread::spawn(move || {
            let result = context.prepare_b2_memory_source(&operation, &params.request);
            context.finish_operation(&operation);
            let _ = responses.send(result.map_or_else(
                |error| call_failure(request_id, &error),
                |(state, receipt)| success(request_id, json!({"state": state, "receipt": receipt})),
            ));
        });
    }

    fn process_b2_memory_source(&mut self, request: &RpcRequest, responses: &Sender<RpcResponse>) {
        let Ok(params) = decode_params::<ProcessB2MemorySource>(request) else {
            let _ = responses.send(invalid_params(request.id));
            return;
        };
        if !params.consent.is_valid() {
            let _ = responses.send(invalid_params(request.id));
            return;
        }
        let Some(operation) = self.begin_operation(request.id, &params.proof(), responses) else {
            return;
        };
        let selection = self.store.selected_model_selection();
        let current_model = selection
            .map_err(HostCallError::Store)
            .and_then(|selection| selection.ok_or(HostCallError::Internal))
            .and_then(|selection| {
                self.operations.reconcile_active_model_selection(
                    &operation,
                    &selection,
                    params.authorization.revision,
                    now_ms()?,
                    || Ok(()),
                )
            });
        if let Err(error) = current_model {
            self.finish_operation(&operation);
            let _ = responses.send(call_failure(request.id, &error));
            return;
        }
        let context = self.background_context(params.proof());
        let responses = responses.clone();
        let request_id = request.id;
        std::thread::spawn(move || {
            let result = context.process_b2_memory_source(&operation, &params.consent);
            context.finish_operation(&operation);
            let _ = responses.send(result.map_or_else(
                |error| call_failure(request_id, &error),
                |(state, receipt)| success(request_id, json!({"state": state, "receipt": receipt})),
            ));
        });
    }

    fn bind_channel_route(&mut self, request: &RpcRequest, responses: &Sender<RpcResponse>) {
        let Ok(params) = decode_params::<BindChannelRoute>(request) else {
            let _ = responses.send(invalid_params(request.id));
            return;
        };
        let result = self
            .store
            .require_runtime_checkpoint(&params.authorization, &params.broker_receipt)
            .and_then(|()| self.store.bind_additional_channel_route(&params.approval));
        let _ = responses.send(result.map_or_else(
            |error| host_failure(request.id, &error),
            |route_set| success(request.id, route_set),
        ));
    }

    fn start_discord_setup(&self, request: &RpcRequest, responses: &Sender<RpcResponse>) {
        let Ok(mut params) = decode_params::<StartDiscord>(request) else {
            let _ = responses.send(invalid_params(request.id));
            return;
        };
        let Some(operation) = self.begin_operation(request.id, &params.proof(), responses) else {
            params.bot_token.zeroize();
            return;
        };
        let token = Zeroizing::new(std::mem::take(&mut params.bot_token));
        let result = self
            .channels
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .start_discord_setup(&token)
            .map_err(|_| HostCallError::Internal);
        self.finish_operation(&operation);
        let _ = responses.send(result.map_or_else(
            |error| call_failure(request.id, &error),
            |start| success(request.id, start),
        ));
    }

    fn poll_discord_setup(&self, request: &RpcRequest, responses: &Sender<RpcResponse>) {
        let Ok(params) = decode_params::<RuntimeProof>(request) else {
            let _ = responses.send(invalid_params(request.id));
            return;
        };
        let Some(operation) = self.begin_operation(request.id, &params, responses) else {
            return;
        };
        let result = self
            .channels
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .poll_discord_setup()
            .map(|(status, candidate)| json!({"status": status, "candidate": candidate}))
            .map_err(|_| HostCallError::Internal);
        self.finish_operation(&operation);
        let _ = responses.send(result.map_or_else(
            |error| call_failure(request.id, &error),
            |value| success(request.id, value),
        ));
    }

    fn confirm_discord_setup(&mut self, request: &RpcRequest, responses: &Sender<RpcResponse>) {
        let Ok(params) = decode_params::<ConfirmDiscordSetup>(request) else {
            let _ = responses.send(invalid_params(request.id));
            return;
        };
        let proof = params.proof();
        let Some(operation) = self.begin_operation(request.id, &proof, responses) else {
            return;
        };
        let result = (|| -> Result<(), HostCallError> {
            let now = now_ms()?;
            let candidate = self
                .channels
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .confirm_discord_setup(&params.candidate_id)
                .map_err(|_| HostCallError::Internal)?;
            if params.confirmed_at_ms < candidate.received_at_ms || params.confirmed_at_ms > now {
                return Err(HostCallError::Internal);
            }
            self.store
                .require_runtime_checkpoint(&params.authorization, &params.broker_receipt)?;
            self.store.pair_channel(&ChannelPairing {
                channel: ChannelKind::Discord,
                owner_sender_id: candidate.owner_user_id,
                conversation_id: candidate.channel_id,
                require_explicit_address: true,
                imessage: None,
                discord: Some(DiscordPairingMetadata {
                    guild_id: candidate.guild_id,
                    bot_user_id: candidate.bot_user_id,
                    application_id: candidate.application_id,
                    setup_source_message_id: candidate.source_message_id,
                    setup_candidate_id: candidate.candidate_id,
                }),
                paired_at_ms: params.confirmed_at_ms,
            })?;
            self.channels
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .stop_discord_setup();
            Ok(())
        })();
        self.finish_operation(&operation);
        let _ = responses.send(result.map_or_else(
            |error| call_failure(request.id, &error),
            |()| success(request.id, json!({"status": "paired"})),
        ));
    }

    fn read_channel_pairing(&self, request: &RpcRequest, responses: &Sender<RpcResponse>) {
        let Ok(params) = decode_params::<ReadChannelPairing>(request) else {
            let _ = responses.send(invalid_params(request.id));
            return;
        };
        let _ = responses.send(self.store.channel_pairing(params.channel).map_or_else(
            |error| host_failure(request.id, &error),
            |value| success(request.id, value),
        ));
    }

    fn start_discord(&self, request: &RpcRequest, responses: &Sender<RpcResponse>) {
        let Ok(mut params) = decode_params::<StartDiscord>(request) else {
            let _ = responses.send(invalid_params(request.id));
            return;
        };
        let Some(operation) = self.begin_operation(request.id, &params.proof(), responses) else {
            params.bot_token.zeroize();
            return;
        };
        let token = Zeroizing::new(std::mem::take(&mut params.bot_token));
        let result = (|| -> Result<ChannelConnectionStatus, HostCallError> {
            let pairing = self
                .store
                .channel_pairing(ChannelKind::Discord)?
                .ok_or(HostCallError::Internal)?;
            let cursor = self
                .store
                .channel_cursor(ChannelKind::Discord, &pairing.conversation_id)?;
            self.channels
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .start_discord(&pairing, token, cursor.as_ref())
                .map_err(|error| channel_runtime_failure(&error))
        })();
        self.finish_operation(&operation);
        let _ = responses.send(result.map_or_else(
            |error| call_failure(request.id, &error),
            |status| success(request.id, json!({"status": status})),
        ));
    }

    fn prepare_imessage(&self, request: &RpcRequest, responses: &Sender<RpcResponse>) {
        let Ok(params) = decode_params::<RuntimeProof>(request) else {
            let _ = responses.send(invalid_params(request.id));
            return;
        };
        let Some(operation) = self.begin_operation(request.id, &params, responses) else {
            return;
        };
        let result = (|| -> Result<u32, HostCallError> {
            let pairing = self
                .store
                .channel_pairing(ChannelKind::IMessage)?
                .ok_or(HostCallError::Internal)?;
            let cursor = self
                .store
                .channel_cursor(ChannelKind::IMessage, &pairing.conversation_id)?;
            self.channels
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .prepare_imessage(&self.paths.imsg_runtime, &pairing, cursor.as_ref())
                .map_err(|error| channel_runtime_failure(&error))
        })();
        self.finish_operation(&operation);
        let _ = responses.send(result.map_or_else(
            |error| call_failure(request.id, &error),
            |process_identifier| {
                success(request.id, json!({"processIdentifier": process_identifier}))
            },
        ));
    }

    fn prepare_imessage_chat_discovery(
        &self,
        request: &RpcRequest,
        responses: &Sender<RpcResponse>,
    ) {
        let Ok(params) = decode_params::<RuntimeProof>(request) else {
            let _ = responses.send(invalid_params(request.id));
            return;
        };
        let Some(operation) = self.begin_operation(request.id, &params, responses) else {
            return;
        };
        let result = self
            .channels
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .prepare_imessage_discovery(&self.paths.imsg_runtime)
            .map_err(|_| HostCallError::Internal);
        self.finish_operation(&operation);
        let _ = responses.send(result.map_or_else(
            |error| call_failure(request.id, &error),
            |process_identifier| {
                success(request.id, json!({"processIdentifier": process_identifier}))
            },
        ));
    }

    fn list_imessage_chat_discovery(&self, request: &RpcRequest, responses: &Sender<RpcResponse>) {
        let Ok(params) = decode_params::<RuntimeProof>(request) else {
            let _ = responses.send(invalid_params(request.id));
            return;
        };
        let Some(operation) = self.begin_operation(request.id, &params, responses) else {
            return;
        };
        let result = self
            .channels
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .list_imessage_chats()
            .map_err(|_| HostCallError::Internal);
        self.finish_operation(&operation);
        let _ = responses.send(result.map_or_else(
            |error| call_failure(request.id, &error),
            |chats| success(request.id, json!({"chats": chats})),
        ));
    }

    fn activate_imessage(&self, request: &RpcRequest, responses: &Sender<RpcResponse>) {
        let Ok(params) = decode_params::<RuntimeProof>(request) else {
            let _ = responses.send(invalid_params(request.id));
            return;
        };
        let Some(operation) = self.begin_operation(request.id, &params, responses) else {
            return;
        };
        let result = self
            .channels
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .activate_imessage()
            .map_err(|error| channel_runtime_failure(&error));
        self.finish_operation(&operation);
        let _ = responses.send(result.map_or_else(
            |error| call_failure(request.id, &error),
            |status| success(request.id, json!({"status": status})),
        ));
    }

    fn prepare_choice_imessage_reply(
        &mut self,
        request: &RpcRequest,
        responses: &Sender<RpcResponse>,
    ) {
        let Ok(params) = decode_params::<RuntimeProof>(request) else {
            let _ = responses.send(invalid_params(request.id));
            return;
        };
        let Some(operation) = self.begin_operation(request.id, &params, responses) else {
            return;
        };
        let result = (|| -> Result<(ChoiceIMessageReplyPreview, String), HostCallError> {
            let (snapshot, pairing, source_message_id, delivery_binding_id) = self
                .store
                .current_choice_imessage_reply_context()?
                .ok_or(HostCallError::ChoiceRefreshRequired)?;
            let choice_set = snapshot
                .active_choice_set
                .as_ref()
                .ok_or(HostCallError::ChoiceRefreshRequired)?;
            if snapshot.session.state != ChoiceSessionState::Active
                || snapshot.session.active_choice_set_id.as_deref() != Some(choice_set.id.as_str())
            {
                return Err(HostCallError::ChoiceRefreshRequired);
            }
            let visible_body = render_choice_imessage_reply(choice_set)?;
            let choice_set_digest =
                canonical_choice_set_digest(choice_set).ok_or(HostCallError::Internal)?;
            let reply_id = hashed_identifier(
                "choice-imessage-reply",
                &json!({
                    "sourceMessageId": source_message_id,
                    "choiceSetDigest": choice_set_digest,
                    "conversationId": pairing.conversation_id,
                }),
            )?;
            let outbound_id =
                hashed_identifier("choice-imessage-outbound", &json!({"replyId": reply_id}))?;
            let created_at_ms = now_ms()?;
            let payload_digest = format!("{:x}", Sha256::digest(visible_body.as_bytes()));
            let mut intent = ChoiceIMessageReplyIntent {
                preview: ChoiceIMessageReplyPreview {
                    reply_id,
                    preview_revision: snapshot.session.revision,
                    destination: "Your selected iMessage self-chat".to_owned(),
                    visible_body,
                    confirmation_digest: "0".repeat(64),
                },
                outbound_id,
                choice_session_id: snapshot.session.id.clone(),
                session_revision: snapshot.session.revision,
                choice_set_id: choice_set.id.clone(),
                choice_set_digest,
                source_message_id,
                delivery_binding_id,
                pairing,
                persona_revision: choice_set.persona_revision.clone(),
                source_manifest_digest: choice_set.source_manifest_digest.clone(),
                model_provenance: choice_set.model_provenance.clone(),
                canonical_payload_sha256: payload_digest,
                created_at_ms,
                approved_at_ms: None,
                recovery_cursor: None,
            };
            intent.preview.confirmation_digest = intent
                .expected_confirmation_digest()
                .ok_or(HostCallError::Internal)?;
            self.store
                .prepare_choice_imessage_reply(&intent)
                .map_err(HostCallError::Store)
        })();
        self.finish_operation(&operation);
        let _ = responses.send(result.map_or_else(
            |error| call_failure(request.id, &error),
            |(preview, status)| success(request.id, json!({"preview": preview, "status": status})),
        ));
    }

    fn authorize_choice_imessage_reply(
        &mut self,
        request: &RpcRequest,
        responses: &Sender<RpcResponse>,
    ) {
        let Ok(params) = decode_params::<AuthorizeChoiceIMessageReply>(request) else {
            let _ = responses.send(invalid_params(request.id));
            return;
        };
        let Some(operation) = self.begin_operation(request.id, &params.proof(), responses) else {
            return;
        };
        let started = self.store.authorize_choice_imessage_reply(
            &params.reply_id,
            params.preview_revision,
            &params.confirmation_digest,
            params.explicitly_approved,
            now_ms().unwrap_or(-1),
        );
        let start = match started {
            Ok(value) => value,
            Err(error) => {
                self.finish_operation(&operation);
                let _ = responses.send(host_failure(request.id, &error));
                return;
            }
        };
        if start.disposition == ChoiceIMessageReplyDisposition::AlreadySent {
            self.finish_operation(&operation);
            let _ = responses.send(success(request.id, json!({"status": "sent"})));
            return;
        }
        let handle = self
            .channels
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .send_handle(ChannelKind::IMessage);
        let Some(handle) = handle else {
            self.finish_operation(&operation);
            let _ = responses.send(call_failure(request.id, &HostCallError::ChannelUnavailable));
            return;
        };
        let context = self.background_context(params.proof());
        let responses = responses.clone();
        let request_id = request.id;
        std::thread::spawn(move || {
            let result = match start.disposition {
                ChoiceIMessageReplyDisposition::ExecuteNow => context
                    .operations
                    .start_irreversible(&operation, || {
                        handle.send(
                            &start.intent.outbound_id,
                            &start.intent.preview.visible_body,
                        )
                    })
                    .unwrap_or(ChannelSendResult::Uncertain),
                ChoiceIMessageReplyDisposition::RecoverOnly => start
                    .intent
                    .recovery_cursor
                    .as_ref()
                    .map_or(ChannelSendResult::Uncertain, |cursor| {
                        handle.recover(
                            &start.intent.outbound_id,
                            &start.intent.preview.visible_body,
                            cursor,
                        )
                    }),
                ChoiceIMessageReplyDisposition::AlreadySent => unreachable!(),
            };
            let response = match result {
                ChannelSendResult::Accepted {
                    provider_message_id,
                } => {
                    let recorded = (|| -> Result<Value, HostCallError> {
                        let broker = context
                            .trusted_broker
                            .clone()
                            .ok_or(StoreError::MissingTrustedBrokerEnrollment)?;
                        let mut store = Store::open_with_trusted_broker(
                            &context.paths.store,
                            context.authority.clone(),
                            broker,
                        )?;
                        store.record_choice_imessage_reply_delivery(
                            &start.intent.preview.reply_id,
                            &provider_message_id,
                            now_ms()?,
                        )?;
                        Ok(json!({"status": "sent"}))
                    })();
                    recorded.map_or_else(
                        |error| call_failure(request_id, &error),
                        |value| success(request_id, value),
                    )
                }
                ChannelSendResult::Uncertain => success(
                    request_id,
                    json!({"status": "needYou", "recoveryOnly": true}),
                ),
            };
            context.finish_operation(&operation);
            let _ = responses.send(response);
        });
    }

    fn channel_status(&self, request: &RpcRequest, responses: &Sender<RpcResponse>) {
        let Ok(params) = decode_params::<ChannelSelection>(request) else {
            let _ = responses.send(invalid_params(request.id));
            return;
        };
        let status = self
            .channels
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .status(params.channel);
        let _ = responses.send(success(request.id, json!({"status": status})));
    }

    fn stop_channel(&self, request: &RpcRequest, responses: &Sender<RpcResponse>) {
        let Ok(params) = decode_params::<ChannelSelection>(request) else {
            let _ = responses.send(invalid_params(request.id));
            return;
        };
        self.channels
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .stop(params.channel);
        let _ = responses.send(success(
            request.id,
            json!({"status": ChannelConnectionStatus::Disconnected}),
        ));
    }

    fn send_channel_message(&mut self, request: &RpcRequest, responses: &Sender<RpcResponse>) {
        let Ok(params) = decode_params::<SendChannelMessage>(request) else {
            let _ = responses.send(invalid_params(request.id));
            return;
        };
        if params.content.is_empty()
            || params.content.trim() != params.content
            || params.content.as_bytes().contains(&0)
            || params.content.chars().count() > 2_000
            || params.approved_at_ms < 0
        {
            let _ = responses.send(invalid_params(request.id));
            return;
        }
        let Some(operation) = self.begin_operation(request.id, &params.proof(), responses) else {
            return;
        };
        let prepared = self.prepare_channel_send(&params);
        let (start, message_body, handle) = match prepared {
            Ok(value) => value,
            Err(error) => {
                self.finish_operation(&operation);
                let _ = responses.send(call_failure(request.id, &error));
                return;
            }
        };
        if start.disposition == ChannelOutboundDisposition::AlreadySent {
            self.finish_operation(&operation);
            let _ = responses.send(success(
                request.id,
                json!({
                    "status": "sent",
                    "providerMessageId": start.provider_message_id,
                }),
            ));
            return;
        }
        let context = self.background_context(params.proof());
        let responses = responses.clone();
        let request_id = request.id;
        std::thread::spawn(move || {
            let transport_result = match start.disposition {
                ChannelOutboundDisposition::ExecuteNow => {
                    handle.send(&start.intent.outbound_id, &message_body)
                }
                ChannelOutboundDisposition::RecoverOnly => start
                    .intent
                    .recovery_cursor
                    .as_ref()
                    .map_or(ChannelSendResult::Uncertain, |cursor| {
                        handle.recover(&start.intent.outbound_id, &message_body, cursor)
                    }),
                ChannelOutboundDisposition::AlreadySent => unreachable!(),
            };
            let result = match transport_result {
                ChannelSendResult::Accepted {
                    provider_message_id,
                } => (|| -> Result<Value, HostCallError> {
                    let broker = context
                        .trusted_broker
                        .clone()
                        .ok_or(StoreError::MissingTrustedBrokerEnrollment)?;
                    let mut store = Store::open_with_trusted_broker(
                        &context.paths.store,
                        context.authority.clone(),
                        broker,
                    )?;
                    let reconciled = store.record_channel_delivery(&ChannelDeliveryReceipt {
                        outbound_id: start.intent.outbound_id.clone(),
                        provider_message_id,
                        delivered_at_ms: now_ms()?,
                    })?;
                    Ok(json!({
                        "status": "sent",
                        "providerMessageId": reconciled.provider_message_id,
                    }))
                })(),
                ChannelSendResult::Uncertain => Ok(json!({
                    "status": "needYou",
                    "providerMessageId": null,
                })),
            };
            context.finish_operation(&operation);
            let _ = responses.send(result.map_or_else(
                |error| call_failure(request_id, &error),
                |value| success(request_id, value),
            ));
        });
    }

    #[allow(clippy::too_many_lines, clippy::type_complexity)]
    fn prepare_channel_send(
        &mut self,
        params: &SendChannelMessage,
    ) -> Result<
        (
            openopen_protocol::ChannelOutboundStart,
            String,
            channels::ChannelSendHandle,
        ),
        HostCallError,
    > {
        let anchor = self
            .store
            .current_verified_audit_anchor()?
            .ok_or(HostCallError::Internal)?;
        let mission = self
            .store
            .get_mission(&params.mission_id, &anchor)?
            .ok_or(HostCallError::Internal)?;
        let route_set = self
            .store
            .channel_route_set(&params.mission_id)?
            .ok_or(HostCallError::Internal)?;
        let route = route_set
            .routes
            .iter()
            .find(|route| route.route_id == params.route_id)
            .cloned()
            .ok_or(HostCallError::Internal)?;
        if route.revision > route_set.revision
            || !route.allowed_outbound_classes.contains(&params.kind)
        {
            return Err(HostCallError::Internal);
        }
        let handle = self
            .channels
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .send_handle(route.channel)
            .ok_or(HostCallError::Internal)?;
        let payload = external_channel_payload(route.channel, &params.content)?;
        let proposal = ActionProposal {
            effect: EffectKind::ChannelSend,
            mission_id: mission.id.clone(),
            mission_scope_digest: mission.scope_digest.clone(),
            target: ActionTarget::Channel {
                channel: route.channel,
                conversation_id: route.conversation_id.clone(),
                recipient_ids: vec![route.owner_sender_id.clone()],
            },
            estimated_cost_micros: None,
        };
        let outbound_id = hashed_identifier(
            "channel-send",
            &json!({
                "channel": route.channel,
                "contentSha256": format!("{:x}", Sha256::digest(&payload)),
                "kind": params.kind,
                "missionId": mission.id,
                "routeId": route.route_id,
                "routeSetRevision": route_set.revision,
            }),
        )?;
        let recovery_cursor = self
            .store
            .channel_cursor(route.channel, &route.conversation_id)?;
        let intent = ChannelOutboundIntent {
            outbound_id: outbound_id.clone(),
            mission_id: mission.id.clone(),
            route_id: route.route_id.clone(),
            route_set_revision: route_set.revision,
            channel: route.channel,
            conversation_id: route.conversation_id.clone(),
            recipient_id: route.owner_sender_id.clone(),
            kind: params.kind,
            content_sha256: format!("{:x}", Sha256::digest(&payload)),
            created_at_ms: params.approved_at_ms,
            recovery_cursor,
        };
        if let Some(start) = self.store.recover_channel_outbound(&intent, &payload)? {
            let message_body = String::from_utf8(payload).map_err(|_| HostCallError::Internal)?;
            return Ok((start, message_body, handle));
        }
        match params.kind {
            ChannelMessageKind::Progress => {
                if mission.status != MissionStatus::Active
                    || params.approved_at_ms < mission.updated_at_ms
                {
                    return Err(HostCallError::Internal);
                }
                let commands = channel_send_approval_commands(
                    &mission,
                    &proposal,
                    &payload,
                    &outbound_id,
                    params.approved_at_ms,
                )?;
                let envelopes = mission_command_batch(Some(&anchor), &mission.id, commands)?;
                self.store.execute_mission_command_batch(&envelopes)?;
            }
            ChannelMessageKind::NeedYou => {
                let needs_me = mission.needs_me.as_ref().ok_or(HostCallError::Internal)?;
                if mission.status != MissionStatus::NeedsMe
                    || params.approved_at_ms < needs_me.created_at_ms
                    || params.content != channel_need_you_content(needs_me)
                {
                    return Err(HostCallError::Internal);
                }
            }
            ChannelMessageKind::Receipt => {
                let receipt = self
                    .store
                    .list_receipts(&anchor)?
                    .into_iter()
                    .find(|receipt| receipt.mission_id == mission.id)
                    .ok_or(HostCallError::Internal)?;
                if mission.status != MissionStatus::Completed
                    || params.approved_at_ms < receipt.completed_at_ms
                    || params.content != channel_receipt_content(&receipt)
                {
                    return Err(HostCallError::Internal);
                }
            }
        }
        let start = self.store.begin_channel_outbound(&intent, &payload)?;
        let message_body = String::from_utf8(payload).map_err(|_| HostCallError::Internal)?;
        Ok((start, message_body, handle))
    }

    fn next_ready_channel_model(
        &mut self,
        channel: ChannelKind,
        observed_at_ms: i64,
    ) -> Result<Option<String>, StoreError> {
        let model_work_ready = self
            .channels
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .model_work_ready(channel);
        if model_work_ready {
            if let Some(source_message_id) = self.store.started_channel_model(channel)? {
                self.store
                    .fail_channel_model(channel, &source_message_id, observed_at_ms)?;
                return Ok(Some(source_message_id));
            }
            if self.store.has_nonterminal_mission()? {
                return Ok(None);
            }
            if let Some(source_message_id) = self.store.next_queued_channel_model(channel)? {
                return Ok(Some(source_message_id));
            }
            Ok(None)
        } else {
            Ok(None)
        }
    }

    fn channel_suggestion_is_current(
        &self,
        suggestion: &OutcomeSuggestion,
    ) -> Result<bool, HostCallError> {
        if self.store.has_nonterminal_mission()? {
            return Ok(false);
        }
        let Some(source) = self.store.channel_source_for_suggestion(&suggestion.id)? else {
            return Ok(true);
        };
        if self
            .store
            .latest_failed_channel_model(source.channel)?
            .is_some()
        {
            return Ok(false);
        }
        let model_work_ready = self
            .channels
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .model_work_ready(source.channel);
        if !model_work_ready || self.store.channel_model_work_pending(source.channel)? {
            return Ok(false);
        }
        Ok(self
            .store
            .latest_channel_suggestion_for(source.channel)?
            .is_some_and(|latest| latest.id == suggestion.id))
    }

    fn visible_dashboard_suggestion(&self) -> Result<Option<OutcomeSuggestion>, HostCallError> {
        let memory_suggestion = self
            .operations
            .suggestion
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        if let Some(candidate) = memory_suggestion {
            if self.channel_suggestion_is_current(&candidate)? {
                return Ok(Some(candidate));
            }
            let mut slot = self
                .operations
                .suggestion
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if slot.as_ref().is_some_and(|value| value.id == candidate.id) {
                *slot = None;
            }
        }
        let Some(candidate) = self.store.latest_channel_suggestion()? else {
            return Ok(None);
        };
        if !self.channel_suggestion_is_current(&candidate)? {
            return Ok(None);
        }
        *self
            .operations
            .suggestion
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(candidate.clone());
        Ok(Some(candidate))
    }

    fn restore_channel_suggestion(
        &self,
        channel: ChannelKind,
    ) -> Result<Option<OutcomeSuggestion>, HostCallError> {
        if self.store.has_nonterminal_mission()? {
            return Ok(None);
        }
        if self.store.channel_model_work_pending(channel)? {
            return Ok(None);
        }
        let Some(candidate) = self.store.latest_channel_suggestion_for(channel)? else {
            return Ok(None);
        };
        if !self.channel_suggestion_is_current(&candidate)? {
            return Ok(None);
        }
        let existing = self
            .operations
            .suggestion
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        if existing
            .as_ref()
            .is_some_and(|value| value.id == candidate.id)
        {
            return Ok(None);
        }
        if let Some(existing) = existing
            && self
                .store
                .channel_source_for_suggestion(&existing.id)?
                .is_none()
        {
            return Ok(None);
        }
        let mut slot = self
            .operations
            .suggestion
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *slot = Some(candidate.clone());
        Ok(Some(candidate))
    }

    fn respond_with_restored_channel_suggestion(
        &self,
        request: &RpcRequest,
        responses: &Sender<RpcResponse>,
        params: &PollChannel,
        operation: &Arc<AtomicBool>,
    ) -> bool {
        match self.restore_channel_suggestion(params.channel) {
            Ok(Some(suggestion)) => {
                self.finish_operation(operation);
                let _ = responses.send(success(
                    request.id,
                    self.channel_poll_value(params.channel, "ready", Some(&suggestion)),
                ));
                true
            }
            Ok(None) => false,
            Err(error) => {
                self.finish_operation(operation);
                let _ = responses.send(call_failure(request.id, &error));
                true
            }
        }
    }

    #[allow(clippy::too_many_lines)] // Keeps the ordered transport/cursor/echo gates auditable.
    fn poll_channel(&mut self, request: &RpcRequest, responses: &Sender<RpcResponse>) {
        let Ok(params) = decode_params::<PollChannel>(request) else {
            let _ = responses.send(invalid_params(request.id));
            return;
        };
        let Some(operation) = self.begin_operation(request.id, &params.proof(), responses) else {
            return;
        };
        let observed_at_ms = match now_ms() {
            Ok(value) => value,
            Err(error) => {
                self.finish_operation(&operation);
                let _ = responses.send(call_failure(request.id, &error));
                return;
            }
        };
        if params.model_work_allowed {
            match self.next_ready_channel_model(params.channel, observed_at_ms) {
                Ok(Some(source_message_id)) => {
                    match self
                        .store
                        .begin_channel_model(params.channel, &source_message_id)
                    {
                        Ok(start) => {
                            self.process_channel_model_start(
                                request, responses, &params, operation, start,
                            );
                        }
                        Err(error) => {
                            self.finish_operation(&operation);
                            let _ = responses.send(host_failure(request.id, &error));
                        }
                    }
                    return;
                }
                Ok(None) => {}
                Err(error) => {
                    self.finish_operation(&operation);
                    let _ = responses.send(host_failure(request.id, &error));
                    return;
                }
            }
            if self
                .respond_with_restored_channel_suggestion(request, responses, &params, &operation)
            {
                return;
            }
        }
        let event = self
            .channels
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .poll(params.channel, observed_at_ms);
        let Ok(event) = event else {
            self.finish_operation(&operation);
            let _ = responses.send(failure(
                Some(request.id),
                -32_000,
                "Channel transport failed closed",
            ));
            return;
        };
        let Some(event) = event else {
            self.respond_when_channel_idle(request, responses, &params, &operation);
            return;
        };
        if let TransportEvent::IMessageEcho {
            provider_message_id,
            cursor,
        } = event
        {
            let result = self
                .store
                .verify_choice_imessage_reply_echo(&cursor.conversation_id, &provider_message_id)
                .and_then(|choice_verified| {
                    if choice_verified {
                        Ok(true)
                    } else {
                        self.store
                            .verify_imessage_echo(&cursor.conversation_id, &provider_message_id)
                    }
                })
                .and_then(|verified| {
                    if !verified {
                        return Err(StoreError::ChannelOutboundConflict);
                    }
                    self.store.advance_channel_cursor(&cursor)
                });
            self.finish_operation(&operation);
            let _ = responses.send(result.map_or_else(
                |error| host_failure(request.id, &error),
                |()| {
                    success(
                        request.id,
                        self.channel_poll_value(params.channel, "echo", None),
                    )
                },
            ));
            return;
        }
        if let TransportEvent::Cursor(cursor) = event {
            match self.store.advance_channel_cursor(&cursor) {
                Ok(()) => {
                    let acknowledged = self
                        .channels
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .acknowledge_recovery(&TransportEvent::Cursor(cursor));
                    self.finish_operation(&operation);
                    let _ = responses.send(acknowledged.map_or_else(
                        |_| failure(Some(request.id), -32_000, "Channel transport failed closed"),
                        |()| {
                            success(
                                request.id,
                                self.channel_poll_value(params.channel, "recovered", None),
                            )
                        },
                    ));
                }
                Err(error) => {
                    self.finish_operation(&operation);
                    let _ = responses.send(host_failure(request.id, &error));
                }
            }
            return;
        }
        let TransportEvent::Inbound(inbound) = event else {
            unreachable!();
        };
        self.process_channel_inbound(request, responses, &params, operation, &inbound);
    }

    fn acknowledge_channel_failure(
        &mut self,
        request: &RpcRequest,
        responses: &Sender<RpcResponse>,
    ) {
        let Ok(params) = decode_params::<AcknowledgeChannelFailure>(request) else {
            let _ = responses.send(invalid_params(request.id));
            return;
        };
        if !self.validate_store_control_proof(request.id, &params.proof(), responses) {
            return;
        }
        let expected_anchor = AuditAnchor {
            sequence: params.expected_incident_audit_anchor.sequence,
            entry_hash: params.expected_incident_audit_anchor.entry_hash,
            signature_hex: params.expected_incident_audit_anchor.signature_hex,
        };
        let result = self.store.acknowledge_channel_failure_incident(
            &params.incident_id,
            &expected_anchor,
            params.authorization.revision,
            params.acknowledged_at_ms,
        );
        let _ = responses.send(result.map_or_else(
            |error| host_failure(request.id, &error),
            |incident| success(request.id, json!(incident)),
        ));
    }

    fn process_channel_inbound(
        &mut self,
        request: &RpcRequest,
        responses: &Sender<RpcResponse>,
        params: &PollChannel,
        operation: Arc<AtomicBool>,
        inbound: &TransportInbound,
    ) {
        if inbound.channel == ChannelKind::IMessage {
            self.process_imessage_choice_begin(request, responses, params, operation, inbound);
            return;
        }
        let observation = channel_observation(inbound);
        let ingested = match self
            .store
            .ingest_channel_message(&observation, &inbound.content)
        {
            Ok(value) => value,
            Err(error) => {
                self.finish_operation(&operation);
                let _ = responses.send(host_failure(request.id, &error));
                return;
            }
        };
        let model_work_ready = {
            let mut channels = self
                .channels
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if channels
                .acknowledge_recovery(&TransportEvent::Inbound(inbound.clone()))
                .is_err()
            {
                None
            } else {
                Some(params.model_work_allowed && channels.model_work_ready(params.channel))
            }
        };
        let Some(model_work_ready) = model_work_ready else {
            self.finish_operation(&operation);
            let _ = responses.send(failure(
                Some(request.id),
                -32_000,
                "Channel transport failed closed",
            ));
            return;
        };
        self.process_durable_channel_inbound(
            request,
            responses,
            operation,
            &DurableChannelInbound {
                params,
                observation: &observation,
                ingested: &ingested,
                model_work_ready,
            },
        );
    }

    /// Converts one authenticated self-chat row into the same Host-owned
    /// Choice intake used by the Mac, with a channel-specific immutable source
    /// envelope. It never uses the retired Outcome path or grants a send.
    #[allow(clippy::too_many_lines)] // Keeps the single Host-owned self-chat intake transaction ordered.
    fn process_imessage_choice_begin(
        &mut self,
        request: &RpcRequest,
        responses: &Sender<RpcResponse>,
        params: &PollChannel,
        operation: Arc<AtomicBool>,
        inbound: &TransportInbound,
    ) {
        let result =
            (|| -> Result<(ChoiceBeginAccepted, Option<ChoiceBeginRecord>), HostCallError> {
                if !params.model_work_allowed {
                    return Err(HostCallError::Codex(CodexError::RequiredModelUnavailable));
                }
                let selection = self
                    .store
                    .selected_model_selection()?
                    .ok_or(HostCallError::Codex(CodexError::RequiredModelUnavailable))?;
                let input = ChoiceBeginRequest {
                    request_id: hashed_identifier(
                        "imessage-choice-begin",
                        &json!({
                            "conversationId": inbound.conversation_id,
                            "sourceMessageId": inbound.source_message_id,
                        }),
                    )?,
                    bounded_local_question: inbound.content.clone(),
                    expected_model_provenance_ref: selection.id.clone(),
                    expected_catalog_fingerprint: selection.catalog_fingerprint.clone(),
                    expected_catalog_revision: selection.catalog_revision,
                    expected_protocol_revision: selection.protocol_schema_revision,
                };
                let input_digest = input.request_digest().ok_or(HostCallError::Internal)?;
                let cursor = ChannelCursor {
                    channel: inbound.channel,
                    conversation_id: inbound.conversation_id.clone(),
                    opaque_value: inbound.cursor_opaque_value.clone(),
                    order: inbound.cursor_order,
                    observed_at_ms: inbound.received_at_ms,
                };
                let request_digest = format!(
                    "{:x}",
                    Sha256::digest(
                        serde_json::to_vec(&json!({
                            "inputDigest": input_digest,
                            "cursor": cursor,
                        }))
                        .map_err(|_| HostCallError::Internal)?
                    )
                );
                if let Some((record, requires_worker)) =
                    self.store.replay_imessage_choice_begin_with_cursor(
                        &input.request_id,
                        &request_digest,
                        &cursor,
                    )?
                {
                    let accepted = record.accepted.clone();
                    return Ok((accepted, requires_worker.then_some(record)));
                }
                let prior = self.store.choice_loop_snapshot()?;
                let session_revision = match prior.as_ref() {
                    None => 1,
                    Some(snapshot)
                        if matches!(
                            snapshot.session.state,
                            ChoiceSessionState::Completed
                                | ChoiceSessionState::Cancelled
                                | ChoiceSessionState::Executing
                        ) =>
                    {
                        snapshot
                            .session
                            .revision
                            .checked_add(1)
                            .ok_or(HostCallError::Internal)?
                    }
                    Some(_) => return Err(HostCallError::ChoiceRefreshRequired),
                };
                let now = now_ms()?;
                let persona_revision = self
                    .persona
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .accept_turn();
                let delivery_binding_id = hashed_identifier(
                    "imessage-self-chat-binding",
                    &json!({"conversationId": inbound.conversation_id}),
                )?;
                let (mut record, snapshot) = new_choice_begin_state_for_source(
                    &input,
                    &selection,
                    session_revision,
                    params.authorization.revision,
                    &persona_revision,
                    now,
                    "imessage-self-chat",
                    delivery_binding_id,
                    Some(inbound.source_message_id.clone()),
                )?;
                record.request_digest.clone_from(&request_digest);
                let clock = ChoiceIdleClockEvidence {
                    boot_id: self.idle_boot_id.clone(),
                    wall_clock_ms: now,
                    monotonic_ms: boot_scoped_monotonic_ms()?,
                };
                let accepted = self.operations.reconcile_active_model_selection(
                    &operation,
                    &selection,
                    params.authorization.revision,
                    now,
                    || {
                        let (accepted, requires_worker) =
                            self.store.begin_imessage_choice_session_with_clock(
                                &record, &snapshot, &clock, &cursor,
                            )?;
                        (accepted == record.accepted)
                            .then_some((accepted, requires_worker))
                            .ok_or(HostCallError::Internal)
                    },
                )?;
                Ok((accepted.0, accepted.1.then_some(record)))
            })();
        match result {
            Err(error) => {
                self.finish_operation(&operation);
                let _ = responses.send(call_failure(request.id, &error));
            }
            Ok((_accepted, None)) => {
                self.finish_operation(&operation);
                let _ = responses.send(success(
                    request.id,
                    self.channel_poll_value(params.channel, "deferred", None),
                ));
            }
            Ok((_accepted, Some(record))) => {
                let context = self.background_context(params.proof());
                let _ = responses.send(success(
                    request.id,
                    self.channel_poll_value(params.channel, "deferred", None),
                ));
                std::thread::spawn(move || {
                    let result = run_initial_choice_generation(&context, &operation, &record);
                    if result.is_err() {
                        let _ = context.block_initial_choice_operation(
                            &operation,
                            &record.accepted.operation_id,
                            record.runtime_revision,
                        );
                    }
                    context.finish_operation(&operation);
                });
            }
        }
    }

    fn process_durable_channel_inbound(
        &mut self,
        request: &RpcRequest,
        responses: &Sender<RpcResponse>,
        operation: Arc<AtomicBool>,
        context: &DurableChannelInbound<'_>,
    ) {
        if matches!(
            context.ingested.decision,
            ChannelInboundDecision::AcceptedMissionUpdate | ChannelInboundDecision::Duplicate
        ) && context.ingested.mission_event.is_some()
        {
            self.finish_operation(&operation);
            let _ = responses.send(success(
                request.id,
                json!({
                    "connectionStatus": self.channels
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .status(context.observation.envelope.channel),
                    "eventStatus": if context.ingested.decision
                        == ChannelInboundDecision::AcceptedMissionUpdate
                    {
                        "missionUpdated"
                    } else {
                        "missionUpdateRecovered"
                    },
                    "missionEvent": context.ingested.mission_event,
                    "suggestion": null,
                }),
            ));
            return;
        }
        if context.ingested.decision != ChannelInboundDecision::Accepted {
            self.finish_operation(&operation);
            let _ = responses.send(success(
                request.id,
                self.channel_poll_value(context.observation.envelope.channel, "ignored", None),
            ));
            return;
        }
        if !context.model_work_ready {
            self.finish_operation(&operation);
            let _ = responses.send(success(
                request.id,
                self.channel_poll_value(context.observation.envelope.channel, "recovering", None),
            ));
            return;
        }
        match self.store.has_nonterminal_mission() {
            Ok(true) => {
                self.finish_operation(&operation);
                let _ = responses.send(success(
                    request.id,
                    self.channel_poll_value(context.observation.envelope.channel, "deferred", None),
                ));
                return;
            }
            Ok(false) => {}
            Err(error) => {
                self.finish_operation(&operation);
                let _ = responses.send(host_failure(request.id, &error));
                return;
            }
        }
        let start = match self.store.begin_channel_model(
            context.observation.envelope.channel,
            &context.observation.envelope.source_message_id,
        ) {
            Ok(value) => value,
            Err(StoreError::ChannelModelDeferredByMission) => {
                self.finish_operation(&operation);
                let _ = responses.send(success(
                    request.id,
                    self.channel_poll_value(context.observation.envelope.channel, "deferred", None),
                ));
                return;
            }
            Err(error) => {
                self.finish_operation(&operation);
                let _ = responses.send(host_failure(request.id, &error));
                return;
            }
        };
        self.process_channel_model_start(request, responses, context.params, operation, start);
    }

    fn process_channel_model_start(
        &self,
        request: &RpcRequest,
        responses: &Sender<RpcResponse>,
        params: &PollChannel,
        operation: Arc<AtomicBool>,
        start: openopen_protocol::ChannelModelStart,
    ) {
        if !params.model_work_allowed {
            self.finish_operation(&operation);
            let _ = responses.send(success(
                request.id,
                self.channel_poll_value(params.channel, "recovering", None),
            ));
            return;
        }
        match start.disposition {
            ChannelModelDisposition::SuggestionReady => {
                let suggestion = match start.suggestion.as_ref() {
                    Some(candidate) => match self.channel_suggestion_is_current(candidate) {
                        Ok(true) => Some(candidate),
                        Ok(false) => None,
                        Err(error) => {
                            self.finish_operation(&operation);
                            let _ = responses.send(call_failure(request.id, &error));
                            return;
                        }
                    },
                    None => None,
                };
                if let Some(suggestion) = suggestion {
                    *self
                        .operations
                        .suggestion
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner) =
                        Some(suggestion.clone());
                }
                self.finish_operation(&operation);
                let _ = responses.send(success(
                    request.id,
                    self.channel_poll_value(
                        params.channel,
                        if suggestion.is_some() {
                            "ready"
                        } else {
                            "superseded"
                        },
                        suggestion,
                    ),
                ));
            }
            ChannelModelDisposition::RecoverOnly => {
                self.finish_operation(&operation);
                let response = self
                    .failed_channel_poll_value(params.channel)
                    .and_then(|value| {
                        value.ok_or(HostCallError::Store(StoreError::ChannelObservationConflict))
                    });
                let _ = responses.send(response.map_or_else(
                    |error| call_failure(request.id, &error),
                    |value| success(request.id, value),
                ));
            }
            ChannelModelDisposition::ExecuteNow => {
                self.execute_channel_model(request, responses, params, operation, start);
            }
        }
    }

    #[allow(clippy::too_many_lines)] // Historical channel recovery remains test-only in PR1.
    #[cfg_attr(
        not(test),
        allow(clippy::needless_pass_by_value, clippy::needless_return,)
    )]
    fn execute_channel_model(
        &self,
        request: &RpcRequest,
        responses: &Sender<RpcResponse>,
        params: &PollChannel,
        operation: Arc<AtomicBool>,
        start: ChannelModelStart,
    ) {
        #[cfg(not(test))]
        {
            // Channel model work is not a PR1 compatibility fallback. The
            // public dispatcher rejects channel.poll before this point; this
            // second fence makes an accidental internal invocation fail
            // closed without creating a workspace, model turn, or
            // OutcomeSuggestion.
            let _ = params;
            let _ = start;
            self.finish_operation(&operation);
            let _ = responses.send(not_ready(request.id));
            return;
        }

        #[cfg(test)]
        {
            let model_context = match self
                .store
                .channel_model_context(start.envelope.channel, &start.envelope.source_message_id)
            {
                Ok(value) => value,
                Err(error) => {
                    self.finish_operation(&operation);
                    let _ = responses.send(host_failure(request.id, &error));
                    return;
                }
            };
            let connection_status = self
                .channels
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .status(params.channel);
            let context = self.background_context(params.proof());
            let responses = responses.clone();
            let request_id = request.id;
            std::thread::spawn(move || {
                let result =
                    run_channel_outcome(&context, &start, &model_context).and_then(|suggestion| {
                        context.reconcile_channel_suggestion(&operation, &start, suggestion)
                    });
                let response = if let Ok((suggestion, is_current)) = result {
                    success(
                        request_id,
                        json!({
                            "connectionStatus": connection_status,
                            "eventStatus": if is_current { "ready" } else { "superseded" },
                            "suggestion": is_current.then_some(suggestion),
                        }),
                    )
                } else {
                    let recorded = context
                        .trusted_broker
                        .clone()
                        .ok_or(StoreError::MissingTrustedBrokerEnrollment)
                        .and_then(|broker| {
                            let mut store = Store::open_with_trusted_broker(
                                &context.paths.store,
                                context.authority.clone(),
                                broker,
                            )?;
                            store.fail_channel_model(
                                start.envelope.channel,
                                &start.envelope.source_message_id,
                                now_ms().map_err(|_| StoreError::ChannelObservationConflict)?,
                            )?;
                            let invalidated = store
                                .latest_channel_suggestion_for(start.envelope.channel)?
                                .map(|candidate| candidate.id);
                            let incidents = store.channel_failure_incident_projection(Some(
                                start.envelope.channel,
                            ))?;
                            if incidents.is_empty() {
                                return Err(StoreError::ChannelObservationConflict);
                            }
                            Ok((invalidated, incidents))
                        });
                    match recorded {
                        Ok((invalidated, incidents)) => {
                            if let Some(invalidated_id) = invalidated.as_deref() {
                                let mut slot = context
                                    .operations
                                    .suggestion
                                    .lock()
                                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                                if slot
                                    .as_ref()
                                    .is_some_and(|candidate| candidate.id == invalidated_id)
                                {
                                    *slot = None;
                                }
                            }
                            success(
                                request_id,
                                json!({
                                    "connectionStatus": connection_status,
                                    "eventStatus": "needYou",
                                    "suggestion": null,
                                    "invalidateSuggestionId": invalidated,
                                    "failureIncidents": incidents,
                                }),
                            )
                        }
                        Err(error) => host_failure(request_id, &error),
                    }
                };
                context.finish_operation(&operation);
                let _ = responses.send(response);
            });
        }
    }

    fn channel_poll_value(
        &self,
        channel: ChannelKind,
        event_status: &str,
        suggestion: Option<&OutcomeSuggestion>,
    ) -> Value {
        let connection_status = self
            .channels
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .status(channel);
        json!({
            "connectionStatus": connection_status,
            "eventStatus": event_status,
            "suggestion": suggestion,
        })
    }

    fn respond_when_channel_idle(
        &self,
        request: &RpcRequest,
        responses: &Sender<RpcResponse>,
        params: &PollChannel,
        operation: &Arc<AtomicBool>,
    ) {
        let poll_value = self.failed_channel_poll_value(params.channel).map(|value| {
            value.unwrap_or_else(|| self.channel_poll_value(params.channel, "idle", None))
        });
        self.finish_operation(operation);
        let _ = responses.send(poll_value.map_or_else(
            |error| call_failure(request.id, &error),
            |value| success(request.id, value),
        ));
    }

    fn failed_channel_poll_value(
        &self,
        channel: ChannelKind,
    ) -> Result<Option<Value>, HostCallError> {
        if self.store.latest_failed_channel_model(channel)?.is_none() {
            return Ok(None);
        }
        let invalidated = self
            .store
            .latest_channel_suggestion_for(channel)?
            .map(|candidate| candidate.id);
        if let Some(invalidated_id) = invalidated.as_deref() {
            let mut slot = self
                .operations
                .suggestion
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if slot
                .as_ref()
                .is_some_and(|candidate| candidate.id == invalidated_id)
            {
                *slot = None;
            }
        }
        let mut value = self.channel_poll_value(channel, "needYou", None);
        value["invalidateSuggestionId"] = json!(invalidated);
        value["failureIncidents"] = json!(
            self.store
                .channel_failure_incident_projection(Some(channel))?
        );
        Ok(Some(value))
    }

    fn start_account_read(&self, request: &RpcRequest, responses: &Sender<RpcResponse>) {
        let Ok(proof) = decode_params::<RuntimeProof>(request) else {
            let _ = responses.send(invalid_params(request.id));
            return;
        };
        let Some(token) = self.begin_operation(request.id, &proof, responses) else {
            return;
        };
        let context = self.background_context(proof);
        let responses = responses.clone();
        let request_id = request.id;
        std::thread::spawn(move || {
            let result = context.with_client(CodexClient::read_account);
            context.finish_operation(&token);
            let _ = responses.send(result.map_or_else(
                |error| call_failure(request_id, &error),
                |value| success(request_id, value),
            ));
        });
    }

    fn start_login(&self, request: &RpcRequest, responses: &Sender<RpcResponse>) {
        let Ok(proof) = decode_params::<RuntimeProof>(request) else {
            let _ = responses.send(invalid_params(request.id));
            return;
        };
        let Some(token) = self.begin_operation(request.id, &proof, responses) else {
            return;
        };
        let context = self.background_context(proof);
        let operations = self.operations.clone();
        let responses = responses.clone();
        let request_id = request.id;
        std::thread::spawn(move || {
            let result = context
                .with_client(CodexClient::begin_chatgpt_login)
                .and_then(|login| {
                    if !operations.install_login(&token, login.clone()) {
                        return Err(HostCallError::Codex(CodexError::Cancelled));
                    }
                    Ok(login)
                });
            if result.is_err() {
                context.finish_operation(&token);
                context.retire_codex_runtime();
            }
            let _ = responses.send(result.map_or_else(
                |error| call_failure(request_id, &error),
                |value| success(request_id, value),
            ));
        });
    }

    fn await_login(&self, request: &RpcRequest, responses: &Sender<RpcResponse>) {
        let Ok(params) = decode_params::<AwaitLogin>(request) else {
            let _ = responses.send(invalid_params(request.id));
            return;
        };
        let Some((session, token)) = self.operations.take_login_operation() else {
            let _ = responses.send(failure(
                Some(request.id),
                -32_012,
                "No managed ChatGPT login is pending",
            ));
            return;
        };
        if params.login_id != session.login.login_id {
            self.finish_operation(&token);
            let _ = responses.send(invalid_params(request.id));
            return;
        }
        if let Err(error) = self
            .store
            .require_runtime_checkpoint(&params.authorization, &params.broker_receipt)
        {
            self.finish_operation(&token);
            let _ = responses.send(host_failure(request.id, &error));
            return;
        }
        if !self.consume_runtime_challenge(&params.broker_receipt) {
            self.finish_operation(&token);
            let _ = responses.send(invalid_params(request.id));
            return;
        }
        let context = self.background_context(params.proof());
        let responses = responses.clone();
        let request_id = request.id;
        std::thread::spawn(move || {
            let result = context.with_client(|client| client.await_chatgpt_login(&params.login_id));
            context.finish_operation(&token);
            context.retire_codex_runtime();
            let _ = responses.send(result.map_or_else(
                |error| call_failure(request_id, &error),
                |()| success(request_id, json!({"status": "completed"})),
            ));
        });
    }

    fn start_model_list(&self, request: &RpcRequest, responses: &Sender<RpcResponse>) {
        let Ok(proof) = decode_params::<RuntimeProof>(request) else {
            let _ = responses.send(invalid_params(request.id));
            return;
        };
        let Some(token) = self.begin_operation(request.id, &proof, responses) else {
            return;
        };
        let context = self.background_context(proof);
        let responses = responses.clone();
        let request_id = request.id;
        std::thread::spawn(move || {
            let result = context.with_client(CodexClient::list_gpt_models);
            context.finish_operation(&token);
            let _ = responses.send(result.map_or_else(
                |error| call_failure(request_id, &error),
                |value| success(request_id, value),
            ));
        });
    }

    /// Reads one capability-bound account/catalog/selection snapshot. The
    /// App must not compose these values from independent RPCs: a same-ID
    /// catalog drift would otherwise make an old persisted selection look
    /// ready until the next model turn fails closed.
    fn read_model_setup(&self, request: &RpcRequest, responses: &Sender<RpcResponse>) {
        let Ok(proof) = decode_params::<RuntimeProof>(request) else {
            let _ = responses.send(invalid_params(request.id));
            return;
        };
        let Some(token) = self.begin_operation(request.id, &proof, responses) else {
            return;
        };
        // The persisted selection is a local read. All Codex/account/catalog
        // I/O stays below on the background worker so stdin can immediately
        // accept protected Off or cancellation.
        let selection = match self.store.selected_model_selection() {
            Ok(value) => value,
            Err(error) => {
                self.finish_operation(&token);
                let _ = responses.send(host_failure(request.id, &error));
                return;
            }
        };
        let context = self.background_context(proof);
        let operations = self.operations.clone();
        let runtime_revision = context.proof.authorization.revision;
        let responses = responses.clone();
        let request_id = request.id;
        std::thread::spawn(move || {
            let result = (|| -> Result<ModelSetup, HostCallError> {
                let (account, models) = context.with_client(|client| {
                    Ok((client.read_account()?, client.list_gpt_models()?))
                })?;
                context.require_enabled()?;
                if token.load(Ordering::Acquire) {
                    return Err(HostCallError::Codex(CodexError::Cancelled));
                }
                let catalog_fingerprint = CodexClient::model_catalog_fingerprint(&models)?;
                let catalog_revision = CodexClient::model_catalog_revision(&catalog_fingerprint)?;
                let issued_at_ms = now_ms()?;
                let catalog_snapshot_id = model_catalog_snapshot_id(
                    &account,
                    &catalog_fingerprint,
                    catalog_revision,
                    runtime_revision,
                    issued_at_ms,
                )?;
                let snapshot = ModelCatalogSnapshot {
                    id: catalog_snapshot_id.clone(),
                    account: account.clone(),
                    models: models.clone(),
                    catalog_fingerprint: catalog_fingerprint.clone(),
                    catalog_revision,
                    runtime_revision,
                    issued_at_ms,
                };
                if !operations.publish_model_catalog_snapshot(&token, snapshot) {
                    return Err(HostCallError::Codex(CodexError::Cancelled));
                }
                let selection_status = model_selection_status(
                    &account,
                    &models,
                    selection.as_ref(),
                    &catalog_fingerprint,
                    catalog_revision,
                );
                Ok(ModelSetup {
                    account,
                    models,
                    selection,
                    selection_status,
                    catalog_snapshot_id,
                    catalog_fingerprint,
                    catalog_revision,
                })
            })();
            context.finish_operation(&token);
            let _ = responses.send(result.map_or_else(
                |error| call_failure(request_id, &error),
                |value| success(request_id, value),
            ));
        });
    }

    fn select_model(&mut self, request: &RpcRequest, responses: &Sender<RpcResponse>) {
        let Ok(params) = decode_params::<SelectModel>(request) else {
            let _ = responses.send(invalid_params(request.id));
            return;
        };
        if !valid_model_selection_request(&params) {
            let _ = responses.send(invalid_params(request.id));
            return;
        }
        let proof = params.proof();
        let Some(token) = self.begin_operation(request.id, &proof, responses) else {
            return;
        };
        // Selection never refreshes the provider synchronously (or at all).
        // It can consume only the exact, short-lived Host snapshot returned by
        // a preceding background `models.setup.read` operation.
        let result = now_ms().and_then(|now| {
            self.operations.reconcile_active_model_catalog(
                &token,
                &ModelCatalogRequest {
                    snapshot_id: &params.catalog_snapshot_id,
                    catalog_fingerprint: &params.catalog_fingerprint,
                    catalog_revision: params.catalog_revision,
                    runtime_revision: params.authorization.revision,
                    now,
                },
                |snapshot| {
                    let selection = bind_model_selection(snapshot, &params)?;
                    self.store
                        .select_model_selection(&selection, now)
                        .map_err(HostCallError::Store)
                },
            )
        });
        self.finish_operation(&token);
        let response = result.map_or_else(
            |error| call_failure(request.id, &error),
            |value| success(request.id, value),
        );
        let _ = responses.send(response);
    }

    fn read_selected_model(&self, request: &RpcRequest, responses: &Sender<RpcResponse>) {
        let Ok(proof) = decode_params::<RuntimeProof>(request) else {
            let _ = responses.send(invalid_params(request.id));
            return;
        };
        if !self.validate_store_control_proof(request.id, &proof, responses) {
            return;
        }
        let response = self.store.selected_model_selection().map_or_else(
            |error| host_failure(request.id, &error),
            |value| success(request.id, value),
        );
        let _ = responses.send(response);
    }

    fn read_persona_status(&self, request: &RpcRequest, responses: &Sender<RpcResponse>) {
        if decode_params::<NoParams>(request).is_err() {
            let _ = responses.send(invalid_params(request.id));
            return;
        }
        let persona = self
            .persona
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let status = persona.status();
        let _ = responses.send(success(
            request.id,
            PersonaStatusView {
                status,
                change_note: None,
            },
        ));
    }

    /// Continuity state is readable while protected Off so the Mac can show
    /// honest local history and a reachable recovery path. Reading it never
    /// starts a batch, model turn, or external effect.
    fn read_choice_loop(&mut self, request: &RpcRequest, responses: &Sender<RpcResponse>) {
        if decode_params::<NoParams>(request).is_err() {
            let _ = responses.send(invalid_params(request.id));
            return;
        }
        // A read is only a scheduler wake hint. It carries no time, state, or
        // authority fields; when a persisted deadline is due, the Host derives
        // both clock values and the Store performs the fenced transition.
        let result = (|| -> Result<Option<ChoiceLoopSnapshot>, HostCallError> {
            let snapshot = self.store.choice_loop_snapshot()?;
            let Some(snapshot) = snapshot else {
                return Ok(None);
            };
            let runtime = self.store.runtime_control()?;
            // A restart cannot retain an in-process private worker. If a
            // durable session is still interpreting/refining and this Host
            // owns no worker, atomically block only that exact operation. The
            // recovery is fail-closed and cannot invoke or retry a model.
            if runtime.enabled && !self.operations.has_active_operation() {
                // An authenticated `choice.resume` is the sole recovery path
                // for the exact Store-minted owner-return operation. Reads are
                // deliberately non-authorizing, so they leave it intact for a
                // foreground Mac re-entry while still fail-closing every other
                // interrupted worker.
                let is_pending_owner_resume =
                    snapshot.session.state == ChoiceSessionState::Refining
                        && snapshot.pending_refinement_operation.as_ref().is_some_and(
                            |operation| {
                                operation.is_owner_resume()
                                    && operation.expected_generation == runtime.revision
                            },
                        );
                if !is_pending_owner_resume
                    && let Some(recovered) = self
                        .store
                        .recover_interrupted_choice_operation(runtime.revision, now_ms()?)?
                {
                    return Ok(Some(recovered));
                }
            }
            if runtime.enabled
                && matches!(
                    snapshot.session.state,
                    ChoiceSessionState::Active | ChoiceSessionState::SoftIdle
                )
            {
                return self
                    .advance_choice_idle_from_scheduler(
                        &snapshot.session.id,
                        snapshot.session.revision,
                        runtime.revision,
                    )
                    .and_then(|outcome| match outcome {
                        ChoiceIdleAdvance::Unchanged(snapshot)
                        | ChoiceIdleAdvance::Transitioned(snapshot) => Ok(Some(snapshot)),
                        ChoiceIdleAdvance::Calibrated(_) => {
                            Err(HostCallError::ChoiceClockUncertain)
                        }
                    });
            }
            Ok(Some(snapshot))
        })();
        let response = result.map_or_else(
            |error| call_failure(request.id, &error),
            |value| success(request.id, value),
        );
        let _ = responses.send(response);
    }

    /// Applies a due Host-owned idle/stale transition before an operation that
    /// could otherwise consume an old `ChoiceSet`.  The caller supplies no
    /// clock/state authority: if the deadline is due, this returns an error
    /// after the Store's fenced transition so the Mac must refresh its typed
    /// continuity rather than confirm a stale preview.
    fn reject_due_choice_deadline(
        &mut self,
        expected_generation: u64,
    ) -> Result<(), HostCallError> {
        let clock = ChoiceIdleClockEvidence {
            boot_id: self.idle_boot_id.clone(),
            wall_clock_ms: now_ms()?,
            monotonic_ms: boot_scoped_monotonic_ms()?,
        };
        self.reject_due_choice_deadline_with_clock(expected_generation, &clock)
    }

    fn reject_due_choice_deadline_with_clock(
        &mut self,
        expected_generation: u64,
        clock: &ChoiceIdleClockEvidence,
    ) -> Result<(), HostCallError> {
        let snapshot = self
            .store
            .choice_loop_snapshot()?
            .ok_or(HostCallError::Internal)?;
        if !matches!(
            snapshot.session.state,
            ChoiceSessionState::Active | ChoiceSessionState::SoftIdle
        ) {
            return Ok(());
        }
        let outcome = self.advance_choice_idle_with_clock(
            &snapshot.session.id,
            snapshot.session.revision,
            expected_generation,
            clock,
        )?;
        match outcome {
            // Only a later same-boot continuous sample that leaves the exact
            // session unchanged may authorize consumption. A first sample is
            // calibration rather than continuity proof, while a transition
            // requires the Mac to refresh its revision-bound ChoiceSet.
            ChoiceIdleAdvance::Unchanged(_) => Ok(()),
            ChoiceIdleAdvance::Calibrated(_) => Err(HostCallError::ChoiceClockUncertain),
            ChoiceIdleAdvance::Transitioned(_) => Err(HostCallError::ChoiceRefreshRequired),
        }
    }

    /// Converts an already-applied idle/stale transition into the same typed
    /// refresh result as a transition performed by this request. Wrong-session
    /// input remains a generic fail-closed contract error; only a stale
    /// revision of the current foreground session receives this classification.
    fn require_current_choice_revision(
        &self,
        choice_session_id: &str,
        expected_session_revision: u64,
    ) -> Result<(), HostCallError> {
        let snapshot = self
            .store
            .choice_loop_snapshot()?
            .ok_or(HostCallError::Internal)?;
        if snapshot.session.id == choice_session_id
            && snapshot.session.revision != expected_session_revision
        {
            return Err(HostCallError::ChoiceRefreshRequired);
        }
        Ok(())
    }

    /// The sole public intake for a first local question. It first atomically
    /// commits an interpreting session/audit record, then only the retained
    /// Host operation may start the sealed read-only model turn.
    #[allow(clippy::too_many_lines)] // Keeps the single public intake's replay and runtime fences auditable together.
    fn begin_choice(&mut self, request: &RpcRequest, responses: &Sender<RpcResponse>) {
        let Ok(params) = decode_params::<BeginChoice>(request) else {
            let _ = responses.send(invalid_params(request.id));
            return;
        };
        if !params.input.is_valid() {
            let _ = responses.send(invalid_params(request.id));
            return;
        }
        // Preserve the protected-Off boundary before consuming a one-time
        // proof. A replay is readable only through a live, protected Host;
        // otherwise the caller receives the same lease failure as every other
        // model-entry route.
        if !self.has_instance_lease() {
            let _ = responses.send(failure(
                Some(request.id),
                -32_015,
                "Core has no protected instance lease",
            ));
            return;
        }
        // An exact lost-response retry must resolve before this RPC reserves
        // the exclusive worker slot. It is a Store-owned read, never a second
        // model operation.
        if !self.validate_store_control_proof(request.id, &params.proof(), responses) {
            return;
        }
        let Some(request_digest) = params.input.request_digest() else {
            let _ = responses.send(invalid_params(request.id));
            return;
        };
        match self
            .store
            .choice_begin_replay(&params.input.request_id, &request_digest)
        {
            Ok(Some(accepted)) => {
                let _ = responses.send(success(request.id, accepted));
                return;
            }
            Ok(None) => {}
            Err(error) => {
                let _ = responses.send(host_failure(request.id, &error));
                return;
            }
        }
        let Some(token) = self.begin_operation_after_validated_proof(request.id, responses) else {
            return;
        };
        let persona_revision = self
            .persona
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .accept_turn();
        let result =
            (|| -> Result<(ChoiceBeginAccepted, Option<ChoiceBeginRecord>), HostCallError> {
                let now = now_ms()?;
                let selection = self
                    .store
                    .selected_model_selection()?
                    .ok_or(HostCallError::Codex(CodexError::RequiredModelUnavailable))?;
                if !choice_begin_request_matches_selection(&params.input, &selection) {
                    return Err(HostCallError::Codex(CodexError::RequiredModelUnavailable));
                }
                let prior = self.store.choice_loop_snapshot()?;
                let session_revision = match prior.as_ref() {
                    None => 1,
                    Some(snapshot)
                        if matches!(
                            snapshot.session.state,
                            ChoiceSessionState::Completed
                                | ChoiceSessionState::Cancelled
                                | ChoiceSessionState::Executing
                        ) =>
                    {
                        snapshot
                            .session
                            .revision
                            .checked_add(1)
                            .ok_or(HostCallError::Internal)?
                    }
                    Some(_) => return Err(HostCallError::Internal),
                };
                let (record, snapshot) = new_choice_begin_state(
                    &params.input,
                    &selection,
                    session_revision,
                    params.authorization.revision,
                    &persona_revision,
                    now,
                )?;
                let clock = ChoiceIdleClockEvidence {
                    boot_id: self.idle_boot_id.clone(),
                    wall_clock_ms: now,
                    monotonic_ms: boot_scoped_monotonic_ms()?,
                };
                self.operations.reconcile_active_model_selection(
                    &token,
                    &selection,
                    params.authorization.revision,
                    now,
                    || {
                        let accepted = self
                            .store
                            .begin_choice_session_with_clock(&record, &snapshot, &clock)?;
                        (accepted == record.accepted)
                            .then_some((accepted, Some(record.clone())))
                            .ok_or(HostCallError::Internal)
                    },
                )
            })();
        match result {
            Err(error) => {
                self.finish_operation(&token);
                let _ = responses.send(call_failure(request.id, &error));
            }
            Ok((accepted, None)) => {
                self.finish_operation(&token);
                let _ = responses.send(success(request.id, accepted));
            }
            Ok((accepted, Some(record))) => {
                let context = self.background_context(params.proof());
                let _ = responses.send(success(request.id, accepted));
                std::thread::spawn(move || {
                    let result = run_initial_choice_generation(&context, &token, &record);
                    if result.is_err() {
                        // The durable blocked transition is a non-retry,
                        // non-effect recovery path. It is itself fenced by
                        // the protected runtime and active operation token.
                        let _ = context.block_initial_choice_operation(
                            &token,
                            &record.accepted.operation_id,
                            record.runtime_revision,
                        );
                    }
                    context.finish_operation(&token);
                });
            }
        }
    }

    #[allow(clippy::too_many_lines)] // Replay preflight must remain ordered before token acquisition.
    fn select_choice(&mut self, request: &RpcRequest, responses: &Sender<RpcResponse>) {
        let Ok(params) = decode_params::<SelectChoice>(request) else {
            let _ = responses.send(invalid_params(request.id));
            return;
        };
        if !self.validate_store_control_proof(request.id, &params.proof(), responses) {
            return;
        }
        if let (Some(Selection::OptionSelection(selection)), None) =
            (&params.selection, &params.d_input)
        {
            match self.store.choice_option_selection_replay(selection) {
                Ok(Some(snapshot)) => {
                    let _ = responses.send(success(request.id, snapshot));
                    return;
                }
                Ok(None) => {}
                Err(error) => {
                    let _ = responses.send(host_failure(request.id, &error));
                    return;
                }
            }
        }
        // An exact D retry is a Store-owned idempotency read, not a new
        // operation.  It must win over an active refinement token so a lost
        // transport response cannot strand the Mac or start model work twice.
        if let (None, Some(input)) = (&params.selection, &params.d_input) {
            if !input.is_valid() {
                let _ = responses.send(invalid_params(request.id));
                return;
            }
            let replay = input
                .request_digest()
                .ok_or(HostCallError::Internal)
                .and_then(|digest| {
                    self.store
                        .choice_d_replay(&input.request_id, &digest)
                        .map_err(HostCallError::Store)
                });
            match replay {
                Ok(Some(snapshot)) => {
                    let _ = responses.send(success(request.id, snapshot));
                    return;
                }
                Ok(None) => {}
                Err(error) => {
                    let _ = responses.send(call_failure(request.id, &error));
                    return;
                }
            }
        }
        if let Err(error) = self.reject_due_choice_deadline(params.authorization.revision) {
            let _ = responses.send(call_failure(request.id, &error));
            return;
        }
        let requested_fence = match (&params.selection, &params.d_input) {
            (Some(Selection::OptionSelection(selection)), None) => Some((
                selection.choice_session_id.as_str(),
                selection.expected_session_revision,
            )),
            (None, Some(input)) => Some((
                input.choice_session_id.as_str(),
                input.expected_session_revision,
            )),
            _ => None,
        };
        if let Some((choice_session_id, expected_session_revision)) = requested_fence
            && let Err(error) =
                self.require_current_choice_revision(choice_session_id, expected_session_revision)
        {
            let _ = responses.send(call_failure(request.id, &error));
            return;
        }
        let now = match now_ms() {
            Ok(value) => value,
            Err(error) => {
                let _ = responses.send(call_failure(request.id, &error));
                return;
            }
        };
        // The OptionSelection timestamp is a legacy caller-side transport
        // field, not a clock authority.  Preserve its identity fields for
        // replay, but record the Host acceptance time in every durable state
        // transition and deadline calculation.
        let accepted_option = match (&params.selection, &params.d_input) {
            (Some(Selection::OptionSelection(selection)), None) => {
                let mut accepted = selection.clone();
                accepted.selected_at_ms = now;
                Some(accepted)
            }
            _ => None,
        };
        let selection = match self.store.selected_model_selection() {
            Ok(Some(value)) => value,
            Ok(None) => {
                let _ = responses.send(call_failure(
                    request.id,
                    &HostCallError::Codex(CodexError::RequiredModelUnavailable),
                ));
                return;
            }
            Err(error) => {
                let _ = responses.send(host_failure(request.id, &error));
                return;
            }
        };
        let Some(token) = self.begin_operation_after_validated_proof(request.id, responses) else {
            return;
        };
        // D has its own bounded input shape.  In particular, the historical
        // `NaturalConversationSelection` (which contains a persisted,
        // Host-derived batch id) is never accepted from RPC callers.
        let result = self.operations.reconcile_active_model_selection(
            &token,
            &selection,
            params.authorization.revision,
            now,
            || match (&accepted_option, &params.d_input) {
                (Some(selection), None) => self
                    .store
                    .commit_choice_selection(
                        &Selection::OptionSelection(selection.clone()),
                        params.authorization.revision,
                        now,
                    )
                    .map_err(HostCallError::Store),
                (None, Some(input)) if input.is_valid() => now_ms().and_then(|now| {
                    let snapshot = self
                        .store
                        .choice_loop_snapshot()?
                        .ok_or(HostCallError::Internal)?;
                    let record = new_choice_d_intake_record(input, &snapshot, now)?;
                    self.store
                        .commit_choice_d_selection(&record, params.authorization.revision)
                        .map_err(HostCallError::Store)
                }),
                _ => Err(HostCallError::Internal),
            },
        );
        match result {
            Err(error) => {
                self.finish_operation(&token);
                let _ = responses.send(call_failure(request.id, &error));
            }
            Ok(snapshot) => {
                let Some(operation) = snapshot.pending_refinement_operation.clone() else {
                    // A concurrent exact retry may have committed or finished
                    // between the replay preflight and the write transaction.
                    // It is a durable success, never an internal error.
                    self.finish_operation(&token);
                    let _ = responses.send(success(request.id, snapshot));
                    return;
                };
                if let Some(input) = params.d_input.as_ref()
                    && operation.d_request_id.as_deref() != Some(&input.request_id)
                {
                    self.finish_operation(&token);
                    let _ = responses.send(call_failure(request.id, &HostCallError::Internal));
                    return;
                }
                let context = self.background_context(params.proof());
                let _ = responses.send(success(request.id, snapshot));
                std::thread::spawn(move || {
                    // The worker receives only Host-derived operation metadata;
                    // no RPC caller can supply a result or reopen a stale
                    // refinement.  A transport/model failure durably blocks
                    // the exact operation rather than leaving `Refining`
                    // visible forever or retrying it.
                    if run_private_refinement(&context, &token, &operation).is_err() {
                        let _ = context.block_choice_refinement_operation(
                            &token,
                            &operation.id,
                            operation.expected_generation,
                        );
                    }
                    context.finish_operation(&token);
                });
            }
        }
    }

    /// Authenticated foreground re-entry for an already durable idle Choice
    /// session. The caller supplies only the normal protected-runtime proof;
    /// all session, timing, provenance, manifest, persona and operation
    /// binding is derived and persisted by the Store.
    #[allow(clippy::too_many_lines)] // Replay must precede token acquisition and remain auditable.
    fn resume_choice(&mut self, request: &RpcRequest, responses: &Sender<RpcResponse>) {
        let Ok(proof) = decode_params::<RuntimeProof>(request) else {
            let _ = responses.send(invalid_params(request.id));
            return;
        };
        if !self.validate_store_control_proof(request.id, &proof, responses) {
            return;
        }
        let selection = match self.store.selected_model_selection() {
            Ok(Some(value)) => value,
            Ok(None) => {
                let _ = responses.send(call_failure(
                    request.id,
                    &HostCallError::Codex(CodexError::RequiredModelUnavailable),
                ));
                return;
            }
            Err(error) => {
                let _ = responses.send(host_failure(request.id, &error));
                return;
            }
        };
        let now = match now_ms() {
            Ok(value) => value,
            Err(error) => {
                let _ = responses.send(call_failure(request.id, &error));
                return;
            }
        };
        let Some(token) = self.begin_operation_after_validated_proof(request.id, responses) else {
            return;
        };
        let result = self.operations.reconcile_active_model_selection(
            &token,
            &selection,
            proof.authorization.revision,
            now,
            || {
                self.store
                    .begin_choice_resume(proof.authorization.revision, now)
                    .map_err(HostCallError::Store)
            },
        );
        match result {
            Err(error) => {
                self.finish_operation(&token);
                let _ = responses.send(call_failure(request.id, &error));
            }
            Ok(snapshot) => {
                let Some(operation) = snapshot.pending_refinement_operation.clone() else {
                    self.finish_operation(&token);
                    let _ = responses.send(call_failure(request.id, &HostCallError::Internal));
                    return;
                };
                if !operation.is_owner_resume() {
                    self.finish_operation(&token);
                    let _ = responses.send(call_failure(request.id, &HostCallError::Internal));
                    return;
                }
                let context = self.background_context(proof);
                let _ = responses.send(success(request.id, snapshot));
                std::thread::spawn(move || {
                    if run_private_refinement(&context, &token, &operation).is_err() {
                        let _ = context.block_choice_refinement_operation(
                            &token,
                            &operation.id,
                            operation.expected_generation,
                        );
                    }
                    context.finish_operation(&token);
                });
            }
        }
    }

    /// Cancels the foreground Choice path without aliasing Mission
    /// cancellation or granting an external effect. Retiring the operation
    /// first rejects every late private model result before Store writes its
    /// terminal successor in one transaction.
    fn cancel_choice(&mut self, request: &RpcRequest, responses: &Sender<RpcResponse>) {
        let Ok(proof) = decode_params::<RuntimeProof>(request) else {
            let _ = responses.send(invalid_params(request.id));
            return;
        };
        if !self.validate_store_control_proof(request.id, &proof, responses) {
            return;
        }
        self.operations.cancel_active_model_operation();
        let response = now_ms()
            .and_then(|cancelled_at_ms| {
                self.store
                    .cancel_choice_session(proof.authorization.revision, cancelled_at_ms)
                    .map_err(Into::into)
            })
            .map_or_else(
                |error| call_failure(request.id, &error),
                |value| success(request.id, value),
            );
        let _ = responses.send(response);
    }

    /// Resumes only the one durable, Host-created Markdown journal. This
    /// accepts no content, path, manifest, receipt, or effect authority from
    /// the caller. A receipt-backed executing journal is included so a crash
    /// after receipt durability but before cleanup/body retirement remains
    /// recoverable without publishing anything new.
    fn reconcile_choice_markdown(&mut self, request: &RpcRequest, responses: &Sender<RpcResponse>) {
        let Ok(proof) = decode_params::<RuntimeProof>(request) else {
            let _ = responses.send(invalid_params(request.id));
            return;
        };
        if !self.validate_store_control_proof(request.id, &proof, responses) {
            return;
        }
        let snapshot = match self.store.choice_loop_snapshot() {
            Ok(Some(snapshot))
                if matches!(
                    snapshot.session.state,
                    ChoiceSessionState::AwaitingConfirmation
                        | ChoiceSessionState::Executing
                        | ChoiceSessionState::Cancelled
                ) =>
            {
                snapshot
            }
            Ok(_) => {
                let _ = responses.send(call_failure(
                    request.id,
                    &HostCallError::MarkdownReconciliationRequired,
                ));
                return;
            }
            Err(error) => {
                let _ = responses.send(host_failure(request.id, &error));
                return;
            }
        };
        if snapshot.session.state == ChoiceSessionState::AwaitingConfirmation {
            let Some(confirmation) = snapshot.confirmation.as_ref() else {
                let _ = responses.send(call_failure(
                    request.id,
                    &HostCallError::MarkdownReconciliationRequired,
                ));
                return;
            };
            let receipt_is_durable = (|| -> Result<bool, HostCallError> {
                let anchor = self
                    .store
                    .current_verified_audit_anchor()?
                    .ok_or(HostCallError::Internal)?;
                let mission_id = choice_mission_id(confirmation)?;
                let mission = self
                    .store
                    .get_mission(&mission_id, &anchor)?
                    .ok_or(HostCallError::Internal)?;
                Ok(mission.status == MissionStatus::Completed
                    && self.store.list_receipts(&anchor)?.iter().any(|receipt| {
                        receipt.mission_id == mission_id
                            && receipt.output_hashes.contains(&confirmation.payload_digest)
                    }))
            })();
            if !matches!(receipt_is_durable, Ok(true)) {
                let _ = responses.send(call_failure(
                    request.id,
                    &HostCallError::MarkdownReconciliationRequired,
                ));
                return;
            }
        }
        let intent = match self
            .store
            .pending_markdown_render_intent_for_session(&snapshot.session.id)
        {
            Ok(Some(intent)) => intent,
            Ok(None) => {
                // Retirement keeps a body-free journal row. Missing metadata
                // is therefore never a completed state: it is storage loss or
                // tampering and must not make the foreground session look
                // healthy or skip final/receipt verification.
                let _ = responses.send(call_failure(
                    request.id,
                    &HostCallError::MarkdownReconciliationRequired,
                ));
                return;
            }
            Err(error) => {
                let _ = responses.send(host_failure(request.id, &error));
                return;
            }
        };
        let Some(operation) = self.begin_operation_after_validated_proof(request.id, responses)
        else {
            return;
        };
        let context = self.background_context(proof);
        let responses = responses.clone();
        let request_id = request.id;
        std::thread::spawn(move || {
            let result = context.complete_markdown_render(&operation, &intent.id, true);
            context.finish_operation(&operation);
            let response = result.map_or_else(
                |error| call_failure(request_id, &error),
                |snapshot| success(request_id, snapshot),
            );
            let _ = responses.send(response);
        });
    }

    /// Completes only post-receipt local cleanup after protected Off. The
    /// request has no parameters: Host derives the sole executing session and
    /// its sole render intent, verifies the immutable receipt and filesystem,
    /// then retires encrypted private bodies. It cannot publish Markdown,
    /// start a model, or authorize an effect.
    fn cleanup_choice_markdown_receipt(
        &mut self,
        request: &RpcRequest,
        responses: &Sender<RpcResponse>,
    ) {
        if decode_params::<EmptyChoiceMarkdownReceiptCleanup>(request).is_err() {
            let _ = responses.send(invalid_params(request.id));
            return;
        }
        let snapshot = match self.store.choice_loop_snapshot() {
            Ok(Some(snapshot))
                if matches!(
                    snapshot.session.state,
                    ChoiceSessionState::AwaitingConfirmation
                        | ChoiceSessionState::Executing
                        | ChoiceSessionState::Cancelled
                ) =>
            {
                snapshot
            }
            Ok(_) => {
                let _ = responses.send(call_failure(request.id, &HostCallError::Internal));
                return;
            }
            Err(error) => {
                let _ = responses.send(host_failure(request.id, &error));
                return;
            }
        };
        let intent = match self
            .store
            .pending_markdown_render_intent_for_session(&snapshot.session.id)
        {
            Ok(Some(intent)) => intent,
            Ok(None) => {
                let _ = responses.send(call_failure(
                    request.id,
                    &HostCallError::MarkdownReconciliationRequired,
                ));
                return;
            }
            Err(error) => {
                let _ = responses.send(host_failure(request.id, &error));
                return;
            }
        };
        // `AwaitingConfirmation` and the terminal Off successor are admitted
        // only for the crash window after the exact receipt is durable and
        // before retained-base cleanup advances the journal. This is still a
        // receipt-verified deletion-only route; unreceipted journals reject.
        if matches!(
            snapshot.session.state,
            ChoiceSessionState::AwaitingConfirmation | ChoiceSessionState::Cancelled
        ) {
            match self.store.markdown_render_receipt(&intent.id) {
                Ok(Some(_)) => {}
                Ok(None) => {
                    let _ = responses.send(call_failure(request.id, &HostCallError::Internal));
                    return;
                }
                Err(error) => {
                    let _ = responses.send(host_failure(request.id, &error));
                    return;
                }
            }
        }
        let authority = self.authority.clone();
        let paths = self.paths.clone();
        let broker = self.store.trusted_broker_enrollment().cloned();
        let responses = responses.clone();
        let request_id = request.id;
        std::thread::spawn(move || {
            let result = complete_markdown_receipt_cleanup(&authority, &paths, broker, &intent.id);
            let response = result.map_or_else(
                |error| call_failure(request_id, &error),
                |snapshot| success(request_id, snapshot),
            );
            let _ = responses.send(response);
        });
    }

    /// Reads whether the one post-Off deletion-only cleanup route is available
    /// for the current terminal Choice. It exposes no body, path, receipt, or
    /// mutation authority; the cleanup RPC revalidates the same state again.
    fn read_choice_markdown_receipt_cleanup_availability(
        &self,
        request: &RpcRequest,
        responses: &Sender<RpcResponse>,
    ) {
        if decode_params::<EmptyChoiceMarkdownReceiptCleanup>(request).is_err() {
            let _ = responses.send(invalid_params(request.id));
            return;
        }
        let result = (|| -> Result<ChoiceMarkdownReceiptCleanupAvailability, HostCallError> {
            let Some(snapshot) = self.store.choice_loop_snapshot()? else {
                return Ok(ChoiceMarkdownReceiptCleanupAvailability { available: false });
            };
            let runtime = self.store.runtime_control()?;
            // The deletion-only cleanup authority is the one exact successor
            // of a protected-Off cancellation. A user cancellation while On
            // must never advertise the post-Off route: Store would correctly
            // reject it, and the UI must not offer a false recovery action.
            if runtime.enabled || snapshot.session.state != ChoiceSessionState::Cancelled {
                return Ok(ChoiceMarkdownReceiptCleanupAvailability { available: false });
            }
            let available = self
                .store
                .pending_markdown_render_intent_for_session(&snapshot.session.id)?
                .map(|intent| {
                    let expected_cancelled_revision = intent
                        .expected_session_revision
                        .checked_add(1)
                        .ok_or(StoreError::ChoiceLoopStateConflict)?;
                    self.store
                        .markdown_render_receipt(&intent.id)
                        .map(|receipt| {
                            receipt.is_some_and(|receipt| {
                                snapshot.session.revision == expected_cancelled_revision
                                    && receipt.intent_id == intent.id
                                    && receipt.final_entry == intent.entry
                                    && receipt.displaced_base == intent.expected_base
                                    && receipt.committed_at_ms >= intent.created_at_ms
                            })
                        })
                })
                .transpose()?
                .unwrap_or(false);
            Ok(ChoiceMarkdownReceiptCleanupAvailability { available })
        })();
        let response = result.map_or_else(
            |error| call_failure(request.id, &error),
            |value| success(request.id, value),
        );
        let _ = responses.send(response);
    }

    // The RPC boundary keeps all response/error branches together so no
    // caller-supplied confirmation can bypass a replay or runtime fence.
    #[allow(clippy::too_many_lines)]
    fn confirm_choice(&mut self, request: &RpcRequest, responses: &Sender<RpcResponse>) {
        let Ok(params) = decode_params::<ConfirmChoice>(request) else {
            let _ = responses.send(invalid_params(request.id));
            return;
        };
        if !self.validate_store_control_proof(request.id, &params.proof(), responses) {
            return;
        }
        match self.store.choice_loop_snapshot() {
            Ok(Some(snapshot))
                if matches!(
                    snapshot.session.state,
                    ChoiceSessionState::AwaitingConfirmation | ChoiceSessionState::Executing
                ) && snapshot.confirmation.as_ref() == Some(&params.confirmation) =>
            {
                // The sealed confirmation id is the durable request identity.
                // A lost RPC response therefore returns the committed state
                // without minting a second confirmation. Confirmation and
                // render journal are one Store transaction, so a missing
                // intent is corruption rather than a replay-time write path.
                if snapshot.session.state == ChoiceSessionState::Executing {
                    let _ = responses.send(success(request.id, snapshot));
                    return;
                }
                let replay = now_ms().and_then(|replayed_at_ms| {
                    self.store
                        .commit_choice_confirmation_and_render_intent(
                            &params.confirmation,
                            params.authorization.revision,
                            replayed_at_ms,
                        )
                        .map_err(HostCallError::Store)
                });
                let response = match replay {
                    Ok((replayed, _)) if replayed == snapshot => success(request.id, snapshot),
                    Ok(_) => call_failure(request.id, &HostCallError::Internal),
                    Err(error) => call_failure(request.id, &error),
                };
                let _ = responses.send(response);
                return;
            }
            Ok(Some(snapshot)) if snapshot.confirmation.is_some() => {
                let _ = responses.send(call_failure(request.id, &HostCallError::Internal));
                return;
            }
            Ok(_) => {}
            Err(error) => {
                let _ = responses.send(host_failure(request.id, &error));
                return;
            }
        }
        if let Err(error) = self.reject_due_choice_deadline(params.authorization.revision) {
            let _ = responses.send(call_failure(request.id, &error));
            return;
        }
        if let Err(error) = self.require_current_choice_revision(
            &params.confirmation.choice_session_id,
            params.confirmation.expected_session_revision,
        ) {
            let _ = responses.send(call_failure(request.id, &error));
            return;
        }
        let snapshot = match self.store.choice_loop_snapshot() {
            Ok(Some(snapshot)) => snapshot,
            Ok(None) => {
                let _ = responses.send(call_failure(request.id, &HostCallError::Internal));
                return;
            }
            Err(error) => {
                let _ = responses.send(host_failure(request.id, &error));
                return;
            }
        };
        // A client may replay a Host-derived seal after a lost response, but
        // it never supplies authority-bearing confirmation fields. Rebuild
        // the entire value from the active Store snapshot and reject every
        // self-consistent but caller-minted variation before any write.
        let schedule = match self.store.current_choice_reminder_schedule_for_revision(
            &snapshot.session.id,
            snapshot.session.revision,
        ) {
            Ok(Some(value)) => value,
            Ok(None) => {
                let _ = responses.send(call_failure(
                    request.id,
                    &HostCallError::ReminderScheduleRequired,
                ));
                return;
            }
            Err(error) => {
                let _ = responses.send(host_failure(request.id, &error));
                return;
            }
        };
        let host_now = match now_ms() {
            Ok(value) => value,
            Err(error) => {
                let _ = responses.send(call_failure(request.id, &error));
                return;
            }
        };
        let expected = match self.derive_choice_confirmation(&snapshot, &schedule, host_now) {
            Ok(value) if value == params.confirmation => value,
            _ => {
                let _ = responses.send(call_failure(request.id, &HostCallError::Internal));
                return;
            }
        };
        let selection = match self.store.selected_model_selection() {
            Ok(Some(value)) => value,
            Ok(None) => {
                let _ = responses.send(call_failure(
                    request.id,
                    &HostCallError::Codex(CodexError::RequiredModelUnavailable),
                ));
                return;
            }
            Err(error) => {
                let _ = responses.send(host_failure(request.id, &error));
                return;
            }
        };
        let Some(operation) = self.begin_operation_after_validated_proof(request.id, responses)
        else {
            return;
        };
        let operations = self.operations.clone();
        let committed = operations.reconcile_active_model_selection(
            &operation,
            &selection,
            params.authorization.revision,
            host_now,
            || {
                self.store
                    .commit_choice_confirmation_and_render_intent(
                        &expected,
                        params.authorization.revision,
                        host_now,
                    )
                    .map_err(HostCallError::Store)
            },
        );
        let committed = match committed {
            Ok((snapshot, _)) => snapshot,
            Err(error) => {
                self.finish_operation(&operation);
                let _ = responses.send(call_failure(request.id, &error));
                return;
            }
        };
        self.finish_operation(&operation);
        // Choice confirmation seals the exact later Markdown journal but does
        // not publish it. The independently authorized Reminder write,
        // readback Evidence, and Receipt must complete first.
        let _ = responses.send(success(request.id, committed));
    }

    fn current_choice_confirmation(
        &self,
        confirmation_id: &str,
    ) -> Result<ChoiceConsolidatedConfirmation, HostCallError> {
        let snapshot = self
            .store
            .choice_loop_snapshot()?
            .ok_or(HostCallError::Internal)?;
        let confirmation = snapshot
            .confirmation
            .as_ref()
            .filter(|confirmation| confirmation.id == confirmation_id)
            .cloned()
            .ok_or(HostCallError::Internal)?;
        if !matches!(
            snapshot.session.state,
            ChoiceSessionState::AwaitingConfirmation | ChoiceSessionState::Executing
        ) || confirmation.choice_session_id != snapshot.session.id
        {
            return Err(HostCallError::Internal);
        }
        let selection = self
            .store
            .selected_model_selection()?
            .ok_or(HostCallError::Codex(CodexError::RequiredModelUnavailable))?;
        let provenance = &confirmation.model_provenance;
        if provenance.model_id != selection.model_id
            || provenance.requested_effort != selection.requested_effort
            || provenance.actual_effort != selection.actual_effort
            || provenance.catalog_fingerprint != selection.catalog_fingerprint
            || provenance.catalog_revision != selection.catalog_revision
            || provenance.account_display_class != selection.account_display_class
            || provenance.protocol_schema_revision != selection.protocol_schema_revision
        {
            return Err(HostCallError::Internal);
        }
        Ok(confirmation)
    }

    fn choice_confirmed_mission(
        &self,
        confirmation: &ChoiceConsolidatedConfirmation,
        disposition: ReminderWriteDisposition,
    ) -> Result<Option<ConfirmedMission>, HostCallError> {
        let Some(anchor) = self.store.current_verified_audit_anchor()? else {
            return Ok(None);
        };
        let mission_id = choice_mission_id(confirmation)?;
        self.store
            .get_mission(&mission_id, &anchor)?
            .map(|mission| {
                confirmed_choice_mission_from_mission(
                    &mission,
                    confirmation,
                    &self.authority,
                    disposition,
                )
            })
            .transpose()
            .map(Option::flatten)
    }

    #[allow(clippy::too_many_lines)]
    fn authorize_choice_reminders(
        &mut self,
        request: &RpcRequest,
        responses: &Sender<RpcResponse>,
    ) {
        let Ok(params) = decode_params::<AuthorizeChoiceReminders>(request) else {
            let _ = responses.send(invalid_params(request.id));
            return;
        };
        if !self.validate_store_control_proof(request.id, &params.proof(), responses) {
            return;
        }
        let result = (|| -> Result<ConfirmedMission, HostCallError> {
            if !valid_reminder_target(&params.reminder_target) {
                return Err(HostCallError::Internal);
            }
            let confirmation = self.current_choice_confirmation(&params.confirmation_id)?;
            if let Some(existing) =
                self.choice_confirmed_mission(&confirmation, ReminderWriteDisposition::RecoverOnly)?
            {
                if existing.reminder_authorization.target == params.reminder_target {
                    return Ok(existing);
                }
                return Err(HostCallError::Internal);
            }
            let mission_id = choice_mission_id(&confirmation)?;
            if self.another_mission_needs_owner(&mission_id)? {
                return Err(HostCallError::MissionAlreadyInProgress);
            }
            let clicked_at_ms = now_ms()?;
            let scope_digest = confirmation.payload_digest.clone();
            let scope_approval_id = hashed_identifier(
                "choice-scope-approval",
                &json!({"confirmationId": confirmation.id, "missionId": mission_id}),
            )?;
            let work_items = confirmation
                .reminder_items
                .iter()
                .map(|item| CreateWorkItem {
                    id: item.id.clone(),
                    title: item.text.clone(),
                })
                .collect::<Vec<_>>();
            let payload =
                choice_reminder_write_payload(&mission_id, &confirmation, &params.reminder_target)?;
            let proposal = reminder_write_proposal(&mission_id, &scope_digest);
            let reminder_digest = proposal
                .approval_digest(ApprovalKind::NewExternalWrite, Some(&payload))
                .map_err(|_| HostCallError::Internal)?;
            let reminder_approval_id = hashed_identifier(
                "choice-reminder-approval",
                &json!({"digest": reminder_digest, "missionId": mission_id}),
            )?;
            let needs_me_id = hashed_identifier(
                "choice-reminder-needs-me",
                &json!({"approvalId": reminder_approval_id, "missionId": mission_id}),
            )?;
            let commands = vec![
                MissionCommand::Create {
                    input: CreateMission {
                        mission_id: mission_id.clone(),
                        title: confirmation.goal.clone(),
                        outcome:
                            "Complete the exact confirmed Choice with verified Reminder Evidence."
                                .to_owned(),
                        owner_id: ISSUER_ID.to_owned(),
                        scope_digest: scope_digest.clone(),
                        scope_approval_id: scope_approval_id.clone(),
                        scope_approval_prompt: "Confirm the exact prepared Choice.".to_owned(),
                        work_items,
                        now_ms: clicked_at_ms,
                    },
                },
                MissionCommand::BeginConfirmation {
                    mission_id: mission_id.clone(),
                    now_ms: clicked_at_ms,
                },
                MissionCommand::DecideApproval {
                    mission_id: mission_id.clone(),
                    approval_id: scope_approval_id,
                    actor_id: ISSUER_ID.to_owned(),
                    decision: ApprovalDecision::Approve,
                    now_ms: clicked_at_ms,
                },
                MissionCommand::Activate {
                    mission_id: mission_id.clone(),
                    now_ms: clicked_at_ms,
                },
                MissionCommand::RequestScopeChange {
                    mission_id: mission_id.clone(),
                    approval: NewBoundaryApproval {
                        id: reminder_approval_id.clone(),
                        kind: ApprovalKind::NewExternalWrite,
                        prompt: "Add the exact confirmed item to Reminders.".to_owned(),
                        scope_digest: reminder_digest,
                        target: Some(ApprovalTarget::ReminderList {
                            logical_list_id: confirmation.reminder_list_id.clone(),
                            source_identifier: params.reminder_target.source_identifier.clone(),
                            calendar_identifier: params.reminder_target.calendar_identifier.clone(),
                        }),
                    },
                    needs_me_id,
                    now_ms: clicked_at_ms,
                },
                MissionCommand::DecideApproval {
                    mission_id: mission_id.clone(),
                    approval_id: reminder_approval_id,
                    actor_id: ISSUER_ID.to_owned(),
                    decision: ApprovalDecision::Approve,
                    now_ms: clicked_at_ms,
                },
                MissionCommand::Resume {
                    mission_id: mission_id.clone(),
                    now_ms: clicked_at_ms,
                },
            ];
            let anchor = self.store.current_verified_audit_anchor()?;
            let mission = self
                .store
                .execute_mission_command_batch(&mission_command_batch(
                    anchor.as_ref(),
                    &mission_id,
                    commands,
                )?)?
                .pop()
                .ok_or(HostCallError::Internal)?
                .mission;
            confirmed_choice_mission_from_mission(
                &mission,
                &confirmation,
                &self.authority,
                ReminderWriteDisposition::CreateOnce,
            )?
            .ok_or(HostCallError::Internal)
        })();
        let _ = responses.send(result.map_or_else(
            |error| call_failure(request.id, &error),
            |mission| success(request.id, mission),
        ));
    }

    fn begin_choice_reminder_dispatch(
        &mut self,
        request: &RpcRequest,
        responses: &Sender<RpcResponse>,
    ) {
        let Ok(params) = decode_params::<ChoiceReminderRequest>(request) else {
            let _ = responses.send(invalid_params(request.id));
            return;
        };
        if !self.validate_store_control_proof(request.id, &params.proof(), responses) {
            return;
        }
        let result = (|| -> Result<ReminderDispatchStart, HostCallError> {
            let confirmation = self.current_choice_confirmation(&params.confirmation_id)?;
            let anchor = self
                .store
                .current_verified_audit_anchor()?
                .ok_or(HostCallError::Internal)?;
            let mission_id = choice_mission_id(&confirmation)?;
            let mission = self
                .store
                .get_mission(&mission_id, &anchor)?
                .ok_or(HostCallError::Internal)?;
            let confirmed = confirmed_choice_mission_from_mission(
                &mission,
                &confirmation,
                &self.authority,
                ReminderWriteDisposition::RecoverOnly,
            )?
            .ok_or(HostCallError::Internal)?;
            let attempt = reminder_dispatch_attempt_state(
                &mission,
                &confirmed.reminder_dispatch,
                &self.authority,
            )?;
            if !confirmed.reminder_links.is_empty()
                || (!confirmed.reminder_dispatch.is_empty()
                    && attempt.is_some_and(|attempt| !attempt.aborted))
            {
                return Ok(ReminderDispatchStart {
                    mission: confirmed,
                    execute_now: false,
                });
            }
            if let Some(attempt) = attempt.filter(|attempt| attempt.aborted) {
                let next_attempt = attempt
                    .index
                    .checked_add(1)
                    .filter(|attempt| *attempt <= MAX_REMINDER_DISPATCH_ATTEMPTS)
                    .ok_or(HostCallError::Internal)?;
                let mission = self.persist_choice_reminder_retry_attempt(
                    &mission,
                    &confirmation,
                    &confirmed.reminder_dispatch,
                    &anchor,
                    next_attempt,
                )?;
                return Ok(ReminderDispatchStart {
                    mission,
                    execute_now: true,
                });
            }
            let observed_at_ms = now_ms()?;
            let mut commands = Vec::with_capacity(mission.work_items.len());
            for item in &mission.work_items {
                let (token, sha256) =
                    reminder_dispatch_claim(&mission, item, &confirmed.reminder_authorization)?;
                commands.push(MissionCommand::AttachEvidence {
                    mission_id: mission.id.clone(),
                    evidence: self.authority.sign_evidence(EvidenceClaims {
                        id: hashed_identifier("evidence", &json!({"kind":"reminderDispatchStarted","missionId":mission.id,"sha256":sha256,"sourceId":token,"workItemId":item.id}))?,
                        mission_id: mission.id.clone(), work_item_id: item.id.clone(),
                        kind: EvidenceKind::ReminderDispatchStarted, source_id: token,
                        sha256: Some(sha256), observed_at_ms,
                    }), now_ms: observed_at_ms,
                });
            }
            let persisted = self
                .store
                .execute_mission_command_batch(&mission_command_batch(
                    Some(&anchor),
                    &mission.id,
                    commands,
                )?)?
                .pop()
                .ok_or(HostCallError::Internal)?
                .mission;
            let mission = confirmed_choice_mission_from_mission(
                &persisted,
                &confirmation,
                &self.authority,
                ReminderWriteDisposition::RecoverOnly,
            )?
            .ok_or(HostCallError::Internal)?;
            Ok(ReminderDispatchStart {
                mission,
                execute_now: true,
            })
        })();
        let _ = responses.send(result.map_or_else(
            |error| call_failure(request.id, &error),
            |start| success(request.id, start),
        ));
    }

    fn persist_choice_reminder_retry_attempt(
        &mut self,
        mission: &Mission,
        confirmation: &ChoiceConsolidatedConfirmation,
        dispatch: &[ConfirmedReminderDispatch],
        anchor: &AuditAnchor,
        attempt: u32,
    ) -> Result<ConfirmedMission, HostCallError> {
        let observed_at_ms = now_ms()?;
        let commands = dispatch
            .iter()
            .map(|dispatch| {
                let digest = reminder_dispatch_retry_digest(&mission.id, dispatch, attempt)?;
                Ok(MissionCommand::AttachEvidence {
                    mission_id: mission.id.clone(),
                    evidence: self.authority.sign_evidence(EvidenceClaims {
                        id: hashed_identifier(
                            "evidence",
                            &json!({
                                "attempt": attempt,
                                "kind": "reminderDispatchRetryStarted",
                                "missionId": mission.id,
                                "sha256": digest,
                                "sourceId": dispatch.token,
                                "workItemId": dispatch.work_item_id,
                            }),
                        )?,
                        mission_id: mission.id.clone(),
                        work_item_id: dispatch.work_item_id.clone(),
                        kind: EvidenceKind::ReminderDispatchRetryStarted,
                        source_id: dispatch.token.clone(),
                        sha256: Some(digest),
                        observed_at_ms,
                    }),
                    now_ms: observed_at_ms,
                })
            })
            .collect::<Result<Vec<_>, HostCallError>>()?;
        let persisted = self
            .store
            .execute_mission_command_batch(&mission_command_batch(
                Some(anchor),
                &mission.id,
                commands,
            )?)?
            .pop()
            .ok_or(HostCallError::Internal)?
            .mission;
        confirmed_choice_mission_from_mission(
            &persisted,
            confirmation,
            &self.authority,
            ReminderWriteDisposition::RecoverOnly,
        )?
        .ok_or(HostCallError::Internal)
    }

    fn abort_choice_reminder_dispatch_before_commit(
        &mut self,
        request: &RpcRequest,
        responses: &Sender<RpcResponse>,
    ) {
        let Ok(params) = decode_params::<ChoiceReminderRequest>(request) else {
            let _ = responses.send(invalid_params(request.id));
            return;
        };
        if !self.validate_store_control_proof(request.id, &params.proof(), responses) {
            return;
        }
        let result = (|| -> Result<ConfirmedMission, HostCallError> {
            let confirmation = self.current_choice_confirmation(&params.confirmation_id)?;
            let anchor = self
                .store
                .current_verified_audit_anchor()?
                .ok_or(HostCallError::Internal)?;
            let mission_id = choice_mission_id(&confirmation)?;
            let mission = self
                .store
                .get_mission(&mission_id, &anchor)?
                .ok_or(HostCallError::Internal)?;
            let confirmed = confirmed_choice_mission_from_mission(
                &mission,
                &confirmation,
                &self.authority,
                ReminderWriteDisposition::RecoverOnly,
            )?
            .ok_or(HostCallError::Internal)?;
            if !confirmed.reminder_links.is_empty() || confirmed.reminder_dispatch.is_empty() {
                return Err(HostCallError::Internal);
            }
            let attempt = reminder_dispatch_attempt_state(
                &mission,
                &confirmed.reminder_dispatch,
                &self.authority,
            )?
            .ok_or(HostCallError::Internal)?;
            if attempt.aborted {
                return Ok(confirmed);
            }
            let observed_at_ms = now_ms()?;
            let commands = confirmed
                .reminder_dispatch
                .iter()
                .map(|dispatch| {
                    let digest = reminder_dispatch_abort_digest(
                        &mission.id,
                        dispatch,
                        attempt.index,
                    )?;
                    Ok(MissionCommand::AttachEvidence {
                        mission_id: mission.id.clone(),
                        evidence: self.authority.sign_evidence(EvidenceClaims {
                            id: hashed_identifier("evidence", &json!({"attempt":attempt.index,"kind":"reminderDispatchAbortedBeforeCommit","missionId":mission.id,"sha256":digest,"sourceId":dispatch.token,"workItemId":dispatch.work_item_id}))?,
                            mission_id: mission.id.clone(),
                            work_item_id: dispatch.work_item_id.clone(),
                            kind: EvidenceKind::ReminderDispatchAbortedBeforeCommit,
                            source_id: dispatch.token.clone(),
                            sha256: Some(digest),
                            observed_at_ms,
                        }),
                        now_ms: observed_at_ms,
                    })
                })
                .collect::<Result<Vec<_>, HostCallError>>()?;
            let persisted = self
                .store
                .execute_mission_command_batch(&mission_command_batch(
                    Some(&anchor),
                    &mission.id,
                    commands,
                )?)?
                .pop()
                .ok_or(HostCallError::Internal)?
                .mission;
            confirmed_choice_mission_from_mission(
                &persisted,
                &confirmation,
                &self.authority,
                ReminderWriteDisposition::RecoverOnly,
            )?
            .ok_or(HostCallError::Internal)
        })();
        let _ = responses.send(result.map_or_else(
            |error| call_failure(request.id, &error),
            |mission| success(request.id, mission),
        ));
    }

    fn record_choice_reminder_mirror(
        &mut self,
        request: &RpcRequest,
        responses: &Sender<RpcResponse>,
    ) {
        let Ok(params) = decode_params::<RecordChoiceReminderMirror>(request) else {
            let _ = responses.send(invalid_params(request.id));
            return;
        };
        if !self.validate_store_control_proof(request.id, &params.proof(), responses) {
            return;
        }
        let result = (|| -> Result<ConfirmedMission, HostCallError> {
            let confirmation = self.current_choice_confirmation(&params.confirmation_id)?;
            let anchor = self
                .store
                .current_verified_audit_anchor()?
                .ok_or(HostCallError::Internal)?;
            let mission_id = choice_mission_id(&confirmation)?;
            let mission = self
                .store
                .get_mission(&mission_id, &anchor)?
                .ok_or(HostCallError::Internal)?;
            let confirmed = confirmed_choice_mission_from_mission(
                &mission,
                &confirmation,
                &self.authority,
                ReminderWriteDisposition::RecoverOnly,
            )?
            .ok_or(HostCallError::Internal)?;
            let links = validated_reminder_links(
                &mission,
                &confirmed.reminder_authorization.target,
                &confirmed.reminder_dispatch,
                params.links,
            )
            .ok_or(HostCallError::Internal)?;
            if !confirmed.reminder_links.is_empty() {
                return (confirmed.reminder_links == links)
                    .then_some(confirmed)
                    .ok_or(HostCallError::Internal);
            }
            let observed_at_ms = now_ms()?;
            let mut commands = Vec::with_capacity(links.len());
            for link in &links {
                let sha256 =
                    reminder_mirror_digest(&confirmed.reminder_authorization.target, link)?;
                commands.push(MissionCommand::AttachEvidence {
                    mission_id: mission.id.clone(),
                    evidence: self.authority.sign_evidence(EvidenceClaims {
                        id: hashed_identifier("evidence", &json!({"kind":"reminderMirrored","missionId":mission.id,"sha256":sha256,"sourceId":link.calendar_item_identifier,"workItemId":link.work_item_id}))?,
                        mission_id: mission.id.clone(), work_item_id: link.work_item_id.clone(),
                        kind: EvidenceKind::ReminderMirrored,
                        source_id: link.calendar_item_identifier.clone(), sha256: Some(sha256),
                        observed_at_ms,
                    }), now_ms: observed_at_ms,
                });
            }
            let persisted = self
                .store
                .execute_mission_command_batch(&mission_command_batch(
                    Some(&anchor),
                    &mission.id,
                    commands,
                )?)?
                .pop()
                .ok_or(HostCallError::Internal)?
                .mission;
            confirmed_choice_mission_from_mission(
                &persisted,
                &confirmation,
                &self.authority,
                ReminderWriteDisposition::RecoverOnly,
            )?
            .ok_or(HostCallError::Internal)
        })();
        let _ = responses.send(result.map_or_else(
            |error| call_failure(request.id, &error),
            |mission| success(request.id, mission),
        ));
    }

    fn complete_choice_reminders(&mut self, request: &RpcRequest, responses: &Sender<RpcResponse>) {
        let Ok(params) = decode_params::<CompleteChoiceReminders>(request) else {
            let _ = responses.send(invalid_params(request.id));
            return;
        };
        if !self.validate_store_control_proof(request.id, &params.proof(), responses) {
            return;
        }
        let confirmation = match self.current_choice_confirmation(&params.confirmation_id) {
            Ok(value) => value,
            Err(error) => {
                let _ = responses.send(call_failure(request.id, &error));
                return;
            }
        };
        let result = (|| -> Result<(Receipt, String), HostCallError> {
            let anchor = self
                .store
                .current_verified_audit_anchor()?
                .ok_or(HostCallError::Internal)?;
            let mission_id = choice_mission_id(&confirmation)?;
            let mission = self
                .store
                .get_mission(&mission_id, &anchor)?
                .ok_or(HostCallError::Internal)?;
            let confirmed = confirmed_choice_mission_from_mission(
                &mission,
                &confirmation,
                &self.authority,
                ReminderWriteDisposition::RecoverOnly,
            )?
            .ok_or(HostCallError::Internal)?;
            let completed_at_ms = now_ms()?;
            let completions = validated_reminder_completions_with_authorization(
                &mission,
                &self.authority,
                params.completions.clone(),
                completed_at_ms,
                mission.status == MissionStatus::Active,
                &confirmed.reminder_authorization,
            )
            .ok_or(HostCallError::Internal)?;
            let receipt = match mission.status {
                MissionStatus::Active => self.persist_reminder_completion(
                    &mission,
                    &anchor,
                    &completions,
                    completed_at_ms,
                    None,
                    None,
                )?,
                MissionStatus::Completed => self
                    .existing_receipt_for_completions(&mission, &anchor, &completions)?
                    .ok_or(HostCallError::Internal)?,
                _ => return Err(HostCallError::Internal),
            };
            if receipt.actual_model != confirmation.model_provenance.model_id
                || !receipt.output_hashes.contains(&confirmation.payload_digest)
            {
                return Err(HostCallError::Internal);
            }
            let intent = self
                .store
                .pending_markdown_render_intent_for_session(&confirmation.choice_session_id)?
                .ok_or(HostCallError::Internal)?;
            Ok((receipt, intent.id))
        })();
        let (receipt, intent_id) = match result {
            Ok(value) => value,
            Err(error) => {
                let _ = responses.send(call_failure(request.id, &error));
                return;
            }
        };
        let Some(operation) = self.begin_operation_after_validated_proof(request.id, responses)
        else {
            return;
        };
        let context = self.background_context(params.proof());
        let responses = responses.clone();
        let request_id = request.id;
        std::thread::spawn(move || {
            let result = context
                .complete_markdown_render(&operation, &intent_id, false)
                .map(|choice_loop| ChoiceReminderCompletion {
                    receipt,
                    choice_loop,
                });
            context.finish_operation(&operation);
            let _ = responses.send(result.map_or_else(
                |error| call_failure(request_id, &error),
                |completion| success(request_id, completion),
            ));
        });
    }

    /// Builds the only confirmation preview from the current Host-owned
    /// Choice snapshot. The Mac can display and explicitly confirm this exact
    /// value, but cannot mint a recipient, scope, Markdown binding, model
    /// provenance, or reminder/evidence payload by composing fields itself.
    fn record_choice_reminder_schedule(
        &mut self,
        request: &RpcRequest,
        responses: &Sender<RpcResponse>,
    ) {
        let Ok(params) = decode_params::<RecordChoiceReminderSchedule>(request) else {
            let _ = responses.send(invalid_params(request.id));
            return;
        };
        if !self.validate_store_control_proof(request.id, &params.proof(), responses) {
            return;
        }
        let result = (|| -> Result<ChoiceReminderSchedule, HostCallError> {
            if !valid_choice_reminder_time_zone(&params.input.time_zone) {
                return Err(HostCallError::ReminderScheduleRequired);
            }
            if let Some(existing) = self.store.choice_reminder_schedule_replay(&params.input)? {
                return Ok(existing);
            }
            self.reject_due_choice_deadline(params.authorization.revision)?;
            let accepted_at_ms = now_ms()?;
            self.store
                .record_choice_reminder_schedule(
                    &params.input,
                    params.authorization.revision,
                    accepted_at_ms,
                )
                .map_err(|error| {
                    // All schedule-shape, expiry, and current-session
                    // conflicts are recoverable only by a new explicit local
                    // schedule choice. Do not hide that known recovery under
                    // the generic fail-closed transport message.
                    if matches!(error, StoreError::ChoiceLoopStateConflict) {
                        HostCallError::ReminderScheduleRequired
                    } else {
                        HostCallError::Store(error)
                    }
                })
        })();
        let response = result.map_or_else(
            |error| call_failure(request.id, &error),
            |value| success(request.id, value),
        );
        let _ = responses.send(response);
    }

    /// Reads only the current session's sealed local schedule so the Mac can
    /// display and explicitly revise it after a restart. It neither accepts a
    /// caller session id nor creates effect authority.
    fn read_choice_reminder_schedule(
        &mut self,
        request: &RpcRequest,
        responses: &Sender<RpcResponse>,
    ) {
        if decode_params::<NoParams>(request).is_err() {
            let _ = responses.send(invalid_params(request.id));
            return;
        }
        let result = (|| -> Result<Option<ChoiceReminderSchedule>, HostCallError> {
            let runtime = self.store.runtime_control()?;
            if runtime.enabled {
                self.reject_due_choice_deadline(runtime.revision)?;
            }
            let Some(snapshot) = self.store.choice_loop_snapshot()? else {
                return Ok(None);
            };
            self.store
                .current_choice_reminder_schedule_for_revision(
                    &snapshot.session.id,
                    snapshot.session.revision,
                )
                .map_err(HostCallError::Store)
        })();
        let response = result.map_or_else(
            |error| call_failure(request.id, &error),
            |value| success(request.id, value),
        );
        let _ = responses.send(response);
    }

    fn prepare_choice_confirmation(
        &mut self,
        request: &RpcRequest,
        responses: &Sender<RpcResponse>,
    ) {
        let Ok(proof) = decode_params::<RuntimeProof>(request) else {
            let _ = responses.send(invalid_params(request.id));
            return;
        };
        if !self.validate_store_control_proof(request.id, &proof, responses) {
            return;
        }
        let result = (|| -> Result<ChoiceConsolidatedConfirmation, HostCallError> {
            self.reject_due_choice_deadline(proof.authorization.revision)?;
            let snapshot = self
                .store
                .choice_loop_snapshot()?
                .ok_or(HostCallError::Internal)?;
            let schedule = self
                .store
                .current_choice_reminder_schedule_for_revision(
                    &snapshot.session.id,
                    snapshot.session.revision,
                )?
                .ok_or(HostCallError::ReminderScheduleRequired)?;
            self.derive_choice_confirmation(&snapshot, &schedule, now_ms()?)
        })();
        let response = result.map_or_else(
            |error| call_failure(request.id, &error),
            |value| success(request.id, value),
        );
        let _ = responses.send(response);
    }

    /// Derives the only confirmation payload accepted by `choice.confirm`.
    /// It deliberately rebuilds all user-visible effect preparation from
    /// durable Host state so a decoded RPC object is never a write authority.
    fn derive_choice_confirmation(
        &self,
        snapshot: &ChoiceLoopSnapshot,
        schedule: &ChoiceReminderSchedule,
        host_now_ms: i64,
    ) -> Result<ChoiceConsolidatedConfirmation, HostCallError> {
        let choice_set = snapshot
            .active_choice_set
            .as_ref()
            .ok_or(HostCallError::Internal)?;
        let interpretation = snapshot
            .interpretation
            .as_ref()
            .ok_or(HostCallError::Internal)?;
        if snapshot.session.state != ChoiceSessionState::Active
            || choice_set.choice_session_id != snapshot.session.id
            || choice_set.session_revision != snapshot.session.revision
            || interpretation.revision != choice_set.interpretation_revision
        {
            return Err(HostCallError::Internal);
        }
        let selection = snapshot
            .last_selection
            .as_ref()
            .ok_or(HostCallError::Internal)?;
        if schedule.input.choice_session_id != snapshot.session.id
            || schedule.input.expected_session_revision != snapshot.session.revision
            || schedule.accepted_at_ms < snapshot.session.last_input_at_ms
            || schedule.input.due_at_ms <= host_now_ms
            || !valid_choice_reminder_time_zone(&schedule.input.time_zone)
        {
            return Err(HostCallError::ReminderScheduleRequired);
        }

        // Alternatives in the current ChoiceSet are not a Reminder
        // payload. The accepted refinement is represented by the current
        // Host-owned interpretation, which is one bounded local step.
        let steps = vec![interpretation.understood_goal.clone()];
        let (reminder_list_id, reminder_items, reminder_count) =
            choice_confirmation_reminders(&steps, selection, &schedule.input)?;
        let (delivery_binding_id, recipient, delivery_scope) =
            choice_confirmation_delivery(&snapshot.session);
        let (
            markdown_entry,
            markdown_expected_base,
            markdown_manifest_digest,
            markdown_semantic_diff_digest,
        ) = self.choice_confirmation_markdown_binding(
            &snapshot.session.id,
            &interpretation.understood_goal,
            &steps,
        )?;
        let source_manifest_digest = snapshot.document_manifest.aggregate_digest.clone();
        let mut confirmation = ChoiceConsolidatedConfirmation {
            id: String::new(),
            choice_session_id: snapshot.session.id.clone(),
            choice_set_id: choice_set.id.clone(),
            selection_id: selection.id().to_owned(),
            expected_session_revision: snapshot.session.revision,
            interpretation_revision: interpretation.revision,
            payload_revision: 0,
            payload_digest: String::new(),
            goal: interpretation.understood_goal.clone(),
            steps,
            markdown_entry,
            markdown_expected_base,
            markdown_manifest_digests: vec![source_manifest_digest, markdown_manifest_digest],
            document_diff_digest: markdown_semantic_diff_digest,
            model_provenance: choice_set.model_provenance.clone(),
            persona_revision: choice_set.persona_revision.clone(),
            reminder_list_id,
            reminder_items,
            reminder_count,
            reminder_payload_digest: String::new(),
            evidence_requirements: vec!["reminder-readback".to_owned()],
            delivery_binding_id,
            recipient,
            delivery_scope,
            data_categories: vec!["local-typed-state".to_owned()],
            retention: "local-confirmation-bound".to_owned(),
            permissions: vec!["reminders-pending-confirmation".to_owned()],
            effect_classes: vec!["reminders".to_owned()],
            // This is a stable, durable proposal timestamp, not the moment
            // an ambiguous confirmation RPC happens to arrive.  `host_now_ms`
            // above independently proves the selected instant is still future.
            confirmed_at_ms: schedule.accepted_at_ms,
        };
        confirmation.reminder_payload_digest = confirmation
            .canonical_reminder_payload_digest()
            .ok_or(HostCallError::Internal)?;
        let revision_material_digest = confirmation
            .canonical_revision_material_digest(schedule.revision)
            .ok_or(HostCallError::Internal)?;
        confirmation.payload_revision = confirmation
            .canonical_payload_revision(schedule.revision)
            .ok_or(HostCallError::Internal)?;
        confirmation.id = hashed_identifier(
            "choice-confirmation",
            &json!({
                "scheduleId": schedule.id,
                "revisionMaterialDigest": revision_material_digest,
            }),
        )?;
        confirmation.payload_digest = confirmation
            .canonical_payload_digest()
            .ok_or(HostCallError::Internal)?;
        confirmation
            .is_valid()
            .then_some(confirmation)
            .ok_or(HostCallError::Internal)
    }

    fn choice_confirmation_markdown_binding(
        &self,
        choice_session_id: &str,
        goal: &str,
        steps: &[String],
    ) -> Result<
        (
            DocumentManifestEntry,
            Option<MarkdownBaseIdentity>,
            String,
            String,
        ),
        HostCallError,
    > {
        let markdown_path = format!("sessions/{choice_session_id}/CHOICE.md");
        let expected_base = self
            .store
            .observe_choice_markdown_base(&markdown_path)
            .map_err(HostCallError::Store)?;
        let body = choice_confirmation_markdown_body(goal, steps);
        let entry = DocumentManifestEntry {
            relative_path: markdown_path,
            sha256: format!("{:x}", Sha256::digest(body.as_bytes())),
            byte_length: u64::try_from(body.len()).map_err(|_| HostCallError::Internal)?,
            mode: 0o600,
        };
        let manifest_digest = canonical_document_manifest_digest(std::slice::from_ref(&entry))
            .ok_or(HostCallError::Internal)?;
        let semantic_diff_digest = format!(
            "{:x}",
            Sha256::digest(
                serde_json::to_vec(&json!({
                    "entry": &entry,
                    "expectedBase": &expected_base,
                    "goal": goal,
                    "steps": steps,
                }))
                .map_err(|_| HostCallError::Internal)?,
            )
        );
        Ok((entry, expected_base, manifest_digest, semantic_diff_digest))
    }

    #[cfg(test)]
    fn confirm_mission(&mut self, request: &RpcRequest, responses: &Sender<RpcResponse>) {
        let Ok(params) = decode_params::<ConfirmMission>(request) else {
            let _ = responses.send(invalid_params(request.id));
            return;
        };
        if !valid_suggestion_id(&params.suggestion_id)
            || !valid_reminder_target(&params.reminder_target)
        {
            let _ = responses.send(invalid_params(request.id));
            return;
        }
        let existing = self.confirmed_mission_for_suggestion_id(&params.suggestion_id);
        let result = match existing {
            Ok(Some(confirmed))
                if confirmed.reminder_authorization.target == params.reminder_target =>
            {
                Ok(confirmed)
            }
            Ok(Some(_)) => {
                let _ = responses.send(invalid_params(request.id));
                return;
            }
            Ok(None) => {
                let mission_id = match mission_id_for_suggestion_id(&params.suggestion_id) {
                    Ok(value) => value,
                    Err(error) => {
                        let _ = responses.send(call_failure(request.id, &error));
                        return;
                    }
                };
                match self.another_mission_needs_owner(&mission_id) {
                    Ok(false) => {}
                    Ok(true) => {
                        let _ = responses.send(call_failure(
                            request.id,
                            &HostCallError::MissionAlreadyInProgress,
                        ));
                        return;
                    }
                    Err(error) => {
                        let _ = responses.send(call_failure(request.id, &error));
                        return;
                    }
                }
                let suggestion = self
                    .operations
                    .suggestion
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .as_ref()
                    .filter(|suggestion| {
                        suggestion.id == params.suggestion_id
                            && valid_outcome_suggestion(suggestion)
                    })
                    .cloned();
                let Some(suggestion) = suggestion else {
                    let _ = responses.send(invalid_params(request.id));
                    return;
                };
                match self.channel_suggestion_is_current(&suggestion) {
                    Ok(true) => {}
                    Ok(false) => {
                        let _ = responses.send(invalid_params(request.id));
                        return;
                    }
                    Err(error) => {
                        let _ = responses.send(call_failure(request.id, &error));
                        return;
                    }
                }
                now_ms().and_then(|clicked_at_ms| {
                    self.persist_confirmed_mission(
                        &suggestion,
                        &params.reminder_target,
                        clicked_at_ms,
                    )
                })
            }
            Err(error) => Err(error),
        };
        if result.is_ok() {
            let mut slot = self
                .operations
                .suggestion
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if slot
                .as_ref()
                .is_some_and(|suggestion| suggestion.id == params.suggestion_id)
            {
                *slot = None;
            }
        }
        let response = result.map_or_else(
            |error| call_failure(request.id, &error),
            |confirmed| success(request.id, confirmed),
        );
        let _ = responses.send(response);
    }

    #[cfg(test)]
    #[allow(dead_code)] // Historical recovery fixtures only; production dispatch is absent.
    fn cancel_mission(&mut self, request: &RpcRequest, responses: &Sender<RpcResponse>) {
        let Ok(params) = decode_params::<CancelMission>(request) else {
            let _ = responses.send(invalid_params(request.id));
            return;
        };
        if !self.validate_store_control_proof(request.id, &params.proof(), responses) {
            return;
        }
        let result = (|| -> Result<MissionCancellation, HostCallError> {
            let Some(anchor) = self.store.current_verified_audit_anchor()? else {
                return Err(HostCallError::Internal);
            };
            let Some(mission) = self.store.get_mission(&params.mission_id, &anchor)? else {
                return Err(HostCallError::Internal);
            };
            if mission.status.is_terminal() {
                return Err(HostCallError::Internal);
            }
            let cancelled_at_ms = now_ms()?;
            let mut results = self
                .store
                .execute_mission_command_batch(&mission_command_batch(
                    Some(&anchor),
                    &mission.id,
                    vec![MissionCommand::Cancel {
                        mission_id: mission.id.clone(),
                        now_ms: cancelled_at_ms,
                    }],
                )?)?;
            let cancelled = results.pop().ok_or(HostCallError::Internal)?;
            if cancelled.mission.id != mission.id
                || cancelled.mission.status != MissionStatus::Cancelled
                || cancelled.receipt.is_some()
            {
                return Err(HostCallError::Internal);
            }
            Ok(MissionCancellation {
                mission_id: cancelled.mission.id,
                status: cancelled.mission.status,
                audit_anchor: cancelled.anchor,
            })
        })();
        let _ = responses.send(result.map_or_else(
            |error| call_failure(request.id, &error),
            |cancelled| success(request.id, cancelled),
        ));
    }

    #[cfg(test)]
    fn confirmed_mission_for_suggestion_id(
        &self,
        suggestion_id: &str,
    ) -> Result<Option<ConfirmedMission>, HostCallError> {
        let Some(anchor) = self.store.current_verified_audit_anchor()? else {
            return Ok(None);
        };
        let mission_id = mission_id_for_suggestion_id(suggestion_id)?;
        self.store
            .get_mission(&mission_id, &anchor)?
            .map(|mission| {
                confirmed_mission_from_mission(
                    &mission,
                    &self.authority,
                    ReminderWriteDisposition::RecoverOnly,
                )
            })
            .transpose()
            .map(Option::flatten)
    }

    fn another_mission_needs_owner(
        &self,
        expected_mission_id: &str,
    ) -> Result<bool, HostCallError> {
        let Some(anchor) = self.store.current_verified_audit_anchor()? else {
            return Ok(false);
        };
        Ok(self
            .store
            .list_missions(&anchor)?
            .into_iter()
            .any(|mission| {
                mission.id != expected_mission_id && mission.status == MissionStatus::NeedsMe
            }))
    }

    #[cfg(test)]
    #[allow(clippy::too_many_lines)]
    fn persist_confirmed_mission(
        &mut self,
        suggestion: &OutcomeSuggestion,
        reminder_target: &ReminderTarget,
        clicked_at_ms: i64,
    ) -> Result<ConfirmedMission, HostCallError> {
        if !valid_reminder_target(reminder_target) {
            return Err(HostCallError::Internal);
        }
        let mission_id = mission_id_for_suggestion_id(&suggestion.id)?;
        let scope_digest = serialized_sha256(suggestion)?;
        let scope_approval_id = hashed_identifier(
            "approval",
            &json!({"missionId": mission_id, "scopeDigest": scope_digest}),
        )?;
        let work_items = suggestion
            .proposed_steps
            .iter()
            .enumerate()
            .map(|(index, title)| {
                Ok(CreateWorkItem {
                    id: hashed_identifier(
                        "work",
                        &json!({"index": index, "missionId": mission_id, "title": title}),
                    )?,
                    title: title.clone(),
                })
            })
            .collect::<Result<Vec<_>, HostCallError>>()?;
        let reminder_payload = reminder_write_payload(
            &mission_id,
            reminder_target,
            work_items
                .iter()
                .map(|item| (item.id.as_str(), item.title.as_str())),
        )?;
        let reminder_proposal = reminder_write_proposal(&mission_id, &scope_digest);
        let reminder_digest = reminder_proposal
            .approval_digest(ApprovalKind::NewExternalWrite, Some(&reminder_payload))
            .map_err(|_| HostCallError::Internal)?;
        let reminder_approval_id = hashed_identifier(
            "approval",
            &json!({"digest": reminder_digest, "missionId": mission_id}),
        )?;
        let needs_me_id = hashed_identifier(
            "needsme",
            &json!({"approvalId": reminder_approval_id, "missionId": mission_id}),
        )?;
        let commands = vec![
            MissionCommand::Create {
                input: CreateMission {
                    mission_id: mission_id.clone(),
                    title: suggestion.title.clone(),
                    outcome: suggestion.why_now.clone(),
                    owner_id: ISSUER_ID.to_owned(),
                    scope_digest: scope_digest.clone(),
                    scope_approval_id: scope_approval_id.clone(),
                    scope_approval_prompt: format!(
                        "Confirm this Mission and create its steps in OpenOpen Reminders: {}",
                        suggestion.title
                    ),
                    work_items,
                    now_ms: clicked_at_ms,
                },
            },
            MissionCommand::BeginConfirmation {
                mission_id: mission_id.clone(),
                now_ms: clicked_at_ms,
            },
            MissionCommand::DecideApproval {
                mission_id: mission_id.clone(),
                approval_id: scope_approval_id,
                actor_id: ISSUER_ID.to_owned(),
                decision: ApprovalDecision::Approve,
                now_ms: clicked_at_ms,
            },
            MissionCommand::Activate {
                mission_id: mission_id.clone(),
                now_ms: clicked_at_ms,
            },
            MissionCommand::RequestScopeChange {
                mission_id: mission_id.clone(),
                approval: NewBoundaryApproval {
                    id: reminder_approval_id.clone(),
                    kind: ApprovalKind::NewExternalWrite,
                    prompt: format!(
                        "Create {} steps in the OpenOpen Reminders list.",
                        suggestion.proposed_steps.len()
                    ),
                    scope_digest: reminder_digest,
                    target: Some(reminder_target.approval_target()),
                },
                needs_me_id,
                now_ms: clicked_at_ms,
            },
            MissionCommand::DecideApproval {
                mission_id: mission_id.clone(),
                approval_id: reminder_approval_id,
                actor_id: ISSUER_ID.to_owned(),
                decision: ApprovalDecision::Approve,
                now_ms: clicked_at_ms,
            },
            MissionCommand::Resume {
                mission_id: mission_id.clone(),
                now_ms: clicked_at_ms,
            },
        ];
        let channel_source = self.store.channel_source_for_suggestion(&suggestion.id)?;
        let expected_anchor = self.store.current_verified_audit_anchor()?;
        let envelopes = mission_command_batch(expected_anchor.as_ref(), &mission_id, commands)?;
        let mut results = if let Some(source) = channel_source {
            self.store
                .execute_mission_command_batch_with_primary_channel_route(
                    &envelopes,
                    source.channel,
                    &source.source_message_id,
                    &suggestion.id,
                    clicked_at_ms,
                )?
        } else {
            self.store.execute_mission_command_batch(&envelopes)?
        };
        let mission = results.pop().ok_or(HostCallError::Internal)?.mission;
        confirmed_mission_from_mission(
            &mission,
            &self.authority,
            ReminderWriteDisposition::CreateOnce,
        )?
        .ok_or(HostCallError::Internal)
    }

    #[cfg(test)]
    #[allow(dead_code)] // Historical recovery fixtures only; production dispatch is absent.
    fn record_reminder_mirror(&mut self, request: &RpcRequest, responses: &Sender<RpcResponse>) {
        let Ok(params) = decode_params::<RecordReminderMirror>(request) else {
            let _ = responses.send(invalid_params(request.id));
            return;
        };
        let result = (|| -> Result<Option<ConfirmedMission>, HostCallError> {
            let Some(anchor) = self.store.current_verified_audit_anchor()? else {
                return Ok(None);
            };
            let Some(mission) = self.store.get_mission(&params.mission_id, &anchor)? else {
                return Ok(None);
            };
            if mission.status != MissionStatus::Active {
                return Ok(None);
            }
            let Some(confirmed) = confirmed_mission_from_mission(
                &mission,
                &self.authority,
                ReminderWriteDisposition::RecoverOnly,
            )?
            else {
                return Ok(None);
            };
            let Some(links) = validated_reminder_links(
                &mission,
                &confirmed.reminder_authorization.target,
                &confirmed.reminder_dispatch,
                params.links,
            ) else {
                return Ok(None);
            };
            if !confirmed.reminder_links.is_empty() {
                return Ok((confirmed.reminder_links == links).then_some(confirmed));
            }
            let observed_at_ms = now_ms()?;
            let mut commands = Vec::with_capacity(links.len());
            for link in &links {
                let sha256 =
                    reminder_mirror_digest(&confirmed.reminder_authorization.target, link)?;
                commands.push(MissionCommand::AttachEvidence {
                    mission_id: mission.id.clone(),
                    evidence: self.authority.sign_evidence(EvidenceClaims {
                        id: hashed_identifier(
                            "evidence",
                            &json!({
                                "kind": "reminderMirrored",
                                "missionId": mission.id,
                                "sha256": sha256,
                                "sourceId": link.calendar_item_identifier,
                                "workItemId": link.work_item_id,
                            }),
                        )?,
                        mission_id: mission.id.clone(),
                        work_item_id: link.work_item_id.clone(),
                        kind: EvidenceKind::ReminderMirrored,
                        source_id: link.calendar_item_identifier.clone(),
                        sha256: Some(sha256),
                        observed_at_ms,
                    }),
                    now_ms: observed_at_ms,
                });
            }
            let envelopes = mission_command_batch(Some(&anchor), &mission.id, commands)?;
            let persisted = self
                .store
                .execute_mission_command_batch(&envelopes)?
                .pop()
                .ok_or(HostCallError::Internal)?
                .mission;
            confirmed_mission_from_mission(
                &persisted,
                &self.authority,
                ReminderWriteDisposition::RecoverOnly,
            )
        })();
        let response = match result {
            Ok(Some(confirmed)) => success(request.id, confirmed),
            Ok(None) => invalid_params(request.id),
            Err(error) => call_failure(request.id, &error),
        };
        let _ = responses.send(response);
    }

    #[cfg(test)]
    fn begin_reminder_dispatch(&mut self, request: &RpcRequest, responses: &Sender<RpcResponse>) {
        let Ok(params) = decode_params::<BeginReminderDispatch>(request) else {
            let _ = responses.send(invalid_params(request.id));
            return;
        };
        let result = (|| -> Result<Option<ReminderDispatchStart>, HostCallError> {
            let Some(anchor) = self.store.current_verified_audit_anchor()? else {
                return Ok(None);
            };
            let Some(mission) = self.store.get_mission(&params.mission_id, &anchor)? else {
                return Ok(None);
            };
            let Some(confirmed) = confirmed_mission_from_mission(
                &mission,
                &self.authority,
                ReminderWriteDisposition::RecoverOnly,
            )?
            else {
                return Ok(None);
            };
            if !confirmed.reminder_links.is_empty() {
                return Ok(Some(ReminderDispatchStart {
                    mission: confirmed,
                    execute_now: false,
                }));
            }
            if !confirmed.reminder_dispatch.is_empty() {
                return Ok(Some(ReminderDispatchStart {
                    mission: confirmed,
                    execute_now: false,
                }));
            }
            let observed_at_ms = now_ms()?;
            let mut commands = Vec::with_capacity(mission.work_items.len());
            for item in &mission.work_items {
                let (token, sha256) =
                    reminder_dispatch_claim(&mission, item, &confirmed.reminder_authorization)?;
                commands.push(MissionCommand::AttachEvidence {
                    mission_id: mission.id.clone(),
                    evidence: self.authority.sign_evidence(EvidenceClaims {
                        id: hashed_identifier(
                            "evidence",
                            &json!({
                                "kind": "reminderDispatchStarted",
                                "missionId": mission.id,
                                "sha256": sha256,
                                "sourceId": token,
                                "workItemId": item.id,
                            }),
                        )?,
                        mission_id: mission.id.clone(),
                        work_item_id: item.id.clone(),
                        kind: EvidenceKind::ReminderDispatchStarted,
                        source_id: token,
                        sha256: Some(sha256),
                        observed_at_ms,
                    }),
                    now_ms: observed_at_ms,
                });
            }
            let envelopes = mission_command_batch(Some(&anchor), &mission.id, commands)?;
            let persisted = self
                .store
                .execute_mission_command_batch(&envelopes)?
                .pop()
                .ok_or(HostCallError::Internal)?
                .mission;
            let mission = confirmed_mission_from_mission(
                &persisted,
                &self.authority,
                ReminderWriteDisposition::RecoverOnly,
            )?
            .ok_or(HostCallError::Internal)?;
            if mission.reminder_dispatch.len() != mission.work_items.len() {
                return Err(HostCallError::Internal);
            }
            Ok(Some(ReminderDispatchStart {
                mission,
                execute_now: true,
            }))
        })();
        let response = match result {
            Ok(Some(start)) => success(request.id, start),
            Ok(None) => invalid_params(request.id),
            Err(error) => call_failure(request.id, &error),
        };
        let _ = responses.send(response);
    }

    #[cfg(test)]
    #[allow(dead_code)] // Historical recovery fixtures only; production dispatch is absent.
    fn complete_reminders(&mut self, request: &RpcRequest, responses: &Sender<RpcResponse>) {
        let Ok(params) = decode_params::<CompleteReminders>(request) else {
            let _ = responses.send(invalid_params(request.id));
            return;
        };
        let anchor = match self.store.current_verified_audit_anchor() {
            Ok(Some(anchor)) => anchor,
            Ok(None) => {
                let _ = responses.send(invalid_params(request.id));
                return;
            }
            Err(error) => {
                let _ = responses.send(host_failure(request.id, &error));
                return;
            }
        };
        let mission = match self.store.get_mission(&params.mission_id, &anchor) {
            Ok(Some(mission)) => mission,
            Ok(None) => {
                let _ = responses.send(invalid_params(request.id));
                return;
            }
            Err(error) => {
                let _ = responses.send(host_failure(request.id, &error));
                return;
            }
        };
        let observed_now = match now_ms() {
            Ok(now) => now,
            Err(error) => {
                let _ = responses.send(call_failure(request.id, &error));
                return;
            }
        };
        let channel_route_set = match self.store.channel_route_set(&mission.id) {
            Ok(value) => value,
            Err(error) => {
                let _ = responses.send(host_failure(request.id, &error));
                return;
            }
        };
        let Some(completion_time) = receipt_completion_time(
            channel_route_set.as_ref(),
            params.receipt_return_approved_at_ms,
            params.receipt_return_route_id.as_deref(),
            mission.status,
            mission.updated_at_ms,
            observed_now,
        ) else {
            let _ = responses.send(invalid_params(request.id));
            return;
        };
        let Some(completions) = validated_reminder_completions(
            &mission,
            &self.authority,
            params.completions,
            completion_time,
            mission.status == MissionStatus::Active,
        ) else {
            let _ = responses.send(invalid_params(request.id));
            return;
        };
        let result = match mission.status {
            MissionStatus::Active => self.persist_reminder_completion(
                &mission,
                &anchor,
                &completions,
                completion_time,
                channel_route_set.as_ref(),
                params.receipt_return_route_id.as_deref(),
            ),
            MissionStatus::Completed => {
                match self.existing_receipt_for_completions(&mission, &anchor, &completions) {
                    Ok(Some(receipt)) => Ok(receipt),
                    Ok(None) => {
                        let _ = responses.send(invalid_params(request.id));
                        return;
                    }
                    Err(error) => Err(error),
                }
            }
            _ => {
                let _ = responses.send(invalid_params(request.id));
                return;
            }
        };
        let response = result.map_or_else(
            |error| call_failure(request.id, &error),
            |receipt| success(request.id, receipt),
        );
        let _ = responses.send(response);
    }

    fn persist_reminder_completion(
        &mut self,
        mission: &Mission,
        anchor: &AuditAnchor,
        completions: &BTreeMap<String, ReminderCompletion>,
        completed_at_ms: i64,
        channel_route_set: Option<&ChannelRouteSet>,
        receipt_return_route_id: Option<&str>,
    ) -> Result<Receipt, HostCallError> {
        let selection = self
            .store
            .selected_model_selection()?
            .ok_or(HostCallError::Codex(CodexError::RequiredModelUnavailable))?;
        let (mut commands, evidence_ids) = Self::reminder_completion_commands(
            mission,
            completions,
            completed_at_ms,
            &self.authority,
        )?;
        let mut receipt_completed_at_ms = completed_at_ms;
        let mut receipt =
            new_reminder_receipt(mission, &selection, &evidence_ids, completed_at_ms)?;
        if let (Some(route_set), Some(route_id)) = (channel_route_set, receipt_return_route_id) {
            let route = approved_receipt_route(route_set, Some(route_id))?;
            receipt_completed_at_ms = completed_at_ms
                .checked_add(6)
                .ok_or(HostCallError::Internal)?;
            receipt.completed_at_ms = receipt_completed_at_ms;
            let receipt_value = Receipt {
                id: receipt.id.clone(),
                mission_id: mission.id.clone(),
                summary: receipt.summary.clone(),
                actual_model: receipt.actual_model.clone(),
                evidence_ids: evidence_ids.clone(),
                output_hashes: receipt.output_hashes.clone(),
                completed_at_ms: receipt_completed_at_ms,
            };
            let content = channel_receipt_content(&receipt_value);
            let payload = external_channel_payload(route.channel, &content)?;
            let proposal = ActionProposal {
                effect: EffectKind::ChannelSend,
                mission_id: mission.id.clone(),
                mission_scope_digest: mission.scope_digest.clone(),
                target: ActionTarget::Channel {
                    channel: route.channel,
                    conversation_id: route.conversation_id.clone(),
                    recipient_ids: vec![route.owner_sender_id.clone()],
                },
                estimated_cost_micros: None,
            };
            let outbound_id = hashed_identifier(
                "channel-send",
                &json!({
                    "channel": route.channel,
                    "contentSha256": format!("{:x}", Sha256::digest(&payload)),
                    "kind": ChannelMessageKind::Receipt,
                    "missionId": mission.id,
                    "routeId": route.route_id,
                    "routeSetRevision": route_set.revision,
                }),
            )?;
            commands.extend(channel_send_approval_commands(
                mission,
                &proposal,
                &payload,
                &outbound_id,
                completed_at_ms,
            )?);
        }
        commands.push(MissionCommand::Complete {
            mission_id: mission.id.clone(),
            receipt,
            now_ms: receipt_completed_at_ms,
        });
        let envelopes = mission_command_batch(Some(anchor), &mission.id, commands)?;
        self.store
            .execute_mission_command_batch(&envelopes)?
            .pop()
            .and_then(|result| result.receipt)
            .ok_or(HostCallError::Internal)
    }

    fn reminder_completion_commands(
        mission: &Mission,
        completions: &BTreeMap<String, ReminderCompletion>,
        completed_at_ms: i64,
        authority: &LocalAuthority,
    ) -> Result<(Vec<MissionCommand>, Vec<String>), HostCallError> {
        let mut commands = Vec::with_capacity(mission.work_items.len() * 3 + 7);
        let mut evidence_ids = Vec::with_capacity(mission.work_items.len());
        for item in &mission.work_items {
            let completion = completions
                .get(&item.id)
                .expect("validated completion covers every WorkItem");
            commands.push(MissionCommand::TransitionWorkItem {
                mission_id: mission.id.clone(),
                work_item_id: item.id.clone(),
                next: WorkItemStatus::Active,
                evidence_ids: Vec::new(),
                now_ms: completed_at_ms,
            });
            let evidence_id = reminder_evidence_id(&mission.id, &item.id, completion)?;
            commands.push(MissionCommand::AttachEvidence {
                mission_id: mission.id.clone(),
                evidence: authority.sign_evidence(EvidenceClaims {
                    id: evidence_id.clone(),
                    mission_id: mission.id.clone(),
                    work_item_id: item.id.clone(),
                    kind: EvidenceKind::ReminderCompleted,
                    source_id: completion.source_id.clone(),
                    sha256: None,
                    observed_at_ms: completion.completed_at_ms,
                }),
                now_ms: completed_at_ms,
            });
            commands.push(MissionCommand::TransitionWorkItem {
                mission_id: mission.id.clone(),
                work_item_id: item.id.clone(),
                next: WorkItemStatus::Completed,
                evidence_ids: vec![evidence_id.clone()],
                now_ms: completed_at_ms,
            });
            evidence_ids.push(evidence_id);
        }
        Ok((commands, evidence_ids))
    }

    fn existing_receipt_for_completions(
        &self,
        mission: &Mission,
        anchor: &AuditAnchor,
        completions: &BTreeMap<String, ReminderCompletion>,
    ) -> Result<Option<Receipt>, HostCallError> {
        let mut evidence_ids = Vec::with_capacity(mission.work_items.len());
        for item in &mission.work_items {
            let completion = completions
                .get(&item.id)
                .expect("validated completion covers every WorkItem");
            let evidence_id = reminder_evidence_id(&mission.id, &item.id, completion)?;
            let Some(evidence) = mission
                .evidence
                .iter()
                .find(|evidence| evidence.id == evidence_id)
            else {
                return Ok(None);
            };
            if item.evidence_ids != [evidence_id.clone()]
                || evidence.mission_id != mission.id
                || evidence.work_item_id != item.id
                || evidence.kind != EvidenceKind::ReminderCompleted
                || evidence.source_id != completion.source_id
                || evidence.observed_at_ms != completion.completed_at_ms
                || self.authority.verify_evidence(evidence).is_err()
            {
                return Ok(None);
            }
            evidence_ids.push(evidence_id);
        }
        let receipt_id = hashed_identifier(
            "receipt",
            &json!({"evidenceIds": evidence_ids, "missionId": mission.id}),
        )?;
        self.store
            .get_receipt(&receipt_id, anchor)
            .map_err(HostCallError::Store)
    }

    fn begin_operation(
        &self,
        request_id: u64,
        proof: &RuntimeProof,
        responses: &Sender<RpcResponse>,
    ) -> Option<Arc<AtomicBool>> {
        if !self.has_instance_lease() {
            let _ = responses.send(failure(
                Some(request_id),
                -32_015,
                "Core has no protected instance lease",
            ));
            return None;
        }
        if !self.consume_runtime_challenge(&proof.broker_receipt) {
            let _ = responses.send(invalid_params(request_id));
            return None;
        }
        if let Err(error) = self
            .store
            .require_runtime_checkpoint(&proof.authorization, &proof.broker_receipt)
        {
            let _ = responses.send(host_failure(request_id, &error));
            return None;
        }
        let Some(token) = self.operations.begin_operation() else {
            let _ = responses.send(failure(
                Some(request_id),
                -32_011,
                "Another Codex operation is active",
            ));
            return None;
        };
        Some(token)
    }

    fn validate_store_control_proof(
        &self,
        request_id: u64,
        proof: &RuntimeProof,
        responses: &Sender<RpcResponse>,
    ) -> bool {
        if !self.consume_runtime_challenge(&proof.broker_receipt) {
            let _ = responses.send(invalid_params(request_id));
            return false;
        }
        if let Err(error) = self
            .store
            .require_runtime_checkpoint(&proof.authorization, &proof.broker_receipt)
        {
            let _ = responses.send(host_failure(request_id, &error));
            return false;
        }
        true
    }

    /// Reserves the exclusive model-operation slot after this exact RPC has
    /// already consumed and verified its single runtime proof. Calling the
    /// normal proof-consuming entrypoint again would turn a valid Choice
    /// operation into a false invalid-parameters failure.
    fn begin_operation_after_validated_proof(
        &self,
        request_id: u64,
        responses: &Sender<RpcResponse>,
    ) -> Option<Arc<AtomicBool>> {
        if !self.has_instance_lease() {
            let _ = responses.send(failure(
                Some(request_id),
                -32_015,
                "Core has no protected instance lease",
            ));
            return None;
        }
        let Some(token) = self.operations.begin_operation() else {
            let _ = responses.send(failure(
                Some(request_id),
                -32_011,
                "Another Codex operation is active",
            ));
            return None;
        };
        Some(token)
    }

    fn consume_runtime_challenge(&self, receipt: &RuntimeControlReceipt) -> bool {
        let expected = self
            .operations
            .runtime_challenge
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        expected.as_deref() == receipt.request_nonce.as_deref() && expected.is_some()
    }

    fn cancel_active(&self) {
        self.operations.cancel_active();
    }

    fn finish_operation(&self, token: &Arc<AtomicBool>) {
        self.operations.finish_operation(token);
    }

    fn background_context(&self, proof: RuntimeProof) -> BackgroundContext {
        BackgroundContext {
            authority: self.authority.clone(),
            paths: self.paths.clone(),
            operations: self.operations.clone(),
            trusted_broker: self.store.trusted_broker_enrollment().cloned(),
            proof,
            codex: self.operations.codex.clone(),
            codex_pid: self.operations.codex_pid.clone(),
            instance_lease: self.instance_lease.clone(),
            persona: self.persona.clone(),
        }
    }

    fn retire_codex_runtime(&self) {
        self.operations.codex_cancel.store(true, Ordering::Release);
        self.operations.clear_model_catalog_snapshot();
        let client = self
            .operations
            .codex
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        *self
            .operations
            .codex_pid
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
        *self
            .instance_lease
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
        drop(client);
    }

    fn has_instance_lease(&self) -> bool {
        self.instance_lease
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_some()
    }

    /// Scheduler-only wake handling. No RPC exposes this transition: the Host
    /// derives both clocks and the stable OS boot identity, while the Store
    /// atomically checks the protected generation and persisted deadline.
    fn advance_choice_idle_from_scheduler(
        &mut self,
        choice_session_id: &str,
        expected_session_revision: u64,
        expected_generation: u64,
    ) -> Result<ChoiceIdleAdvance, HostCallError> {
        let clock = ChoiceIdleClockEvidence {
            boot_id: self.idle_boot_id.clone(),
            wall_clock_ms: now_ms()?,
            monotonic_ms: boot_scoped_monotonic_ms()?,
        };
        self.advance_choice_idle_with_clock(
            choice_session_id,
            expected_session_revision,
            expected_generation,
            &clock,
        )
    }

    fn advance_choice_idle_with_clock(
        &mut self,
        choice_session_id: &str,
        expected_session_revision: u64,
        expected_generation: u64,
        clock: &ChoiceIdleClockEvidence,
    ) -> Result<ChoiceIdleAdvance, HostCallError> {
        self.store
            .advance_choice_idle_state_classified(
                choice_session_id,
                expected_session_revision,
                expected_generation,
                clock,
            )
            .map_err(Into::into)
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        self.channels
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .stop_all();
        self.cancel_active();
    }
}

/// Deletes only already-receipted private render material. This deliberately
/// has no operation token or runtime proof: protected Off must not strand a
/// verified local cleanup, while the absence of a body/path/receipt parameter
/// prevents it from becoming an alternate render route.
fn complete_markdown_receipt_cleanup(
    authority: &LocalAuthority,
    paths: &HostPaths,
    trusted_broker: Option<TrustedBrokerEnrollment>,
    intent_id: &str,
) -> Result<ChoiceLoopSnapshot, HostCallError> {
    let broker = trusted_broker.ok_or(StoreError::MissingTrustedBrokerEnrollment)?;
    let mut store = Store::open_with_trusted_broker(&paths.store, authority.clone(), broker)?;
    store.bind_choice_markdown_root(&paths.user_home)?;
    let intent = store
        .markdown_render_intent(intent_id)?
        .ok_or(StoreError::ChoiceLoopStateConflict)?;
    prepare_markdown_render_root(&paths.user_home, &intent.entry)
        .map_err(|_| StoreError::ChoiceLoopStateConflict)?;
    match store.complete_verified_markdown_render_cleanup(intent_id, now_ms()?)? {
        MarkdownRenderCleanup::Retired(snapshot) => Ok(*snapshot),
        MarkdownRenderCleanup::ReconciliationRequired => {
            Err(HostCallError::MarkdownReconciliationRequired)
        }
    }
}

#[derive(Clone)]
struct BackgroundContext {
    authority: LocalAuthority,
    paths: HostPaths,
    operations: OperationState,
    trusted_broker: Option<TrustedBrokerEnrollment>,
    proof: RuntimeProof,
    codex: Arc<Mutex<Option<CodexClient>>>,
    codex_pid: Arc<Mutex<Option<i32>>>,
    instance_lease: Arc<Mutex<Option<CoreInstanceLease>>>,
    persona: Arc<Mutex<PersonaManager>>,
}

impl BackgroundContext {
    fn prepare_b2_memory_source(
        &self,
        operation: &Arc<AtomicBool>,
        request: &B2MemoryPrepareSourceRequest,
    ) -> Result<(B2MemoryDemoState, B2MemoryCommandReceipt), HostCallError> {
        self.require_enabled()?;
        self.operations.reconcile_active(operation, || {
            let broker = self
                .trusted_broker
                .clone()
                .ok_or(StoreError::MissingTrustedBrokerEnrollment)?;
            let mut store =
                Store::open_with_trusted_broker(&self.paths.store, self.authority.clone(), broker)?;
            let selected_path = PathBuf::from(&request.selected_path);
            let record = match store.b2_memory_prepared_source()? {
                Some(existing)
                    if existing.source.request_id == request.request_id
                        && existing.selected_path == selected_path =>
                {
                    existing
                }
                Some(_) => return Err(StoreError::B2MemoryDemoConflict.into()),
                None => pin_b2_memory_source(&selected_path, &request.request_id, now_ms()?)?,
            };
            store
                .prepare_b2_memory_source(
                    &record,
                    &self.proof.authorization,
                    &self.proof.broker_receipt,
                )
                .map_err(Into::into)
        })
    }

    #[allow(clippy::too_many_lines)]
    fn process_b2_memory_source(
        &self,
        operation: &Arc<AtomicBool>,
        consent: &B2MemoryProcessingConsent,
    ) -> Result<(B2MemoryDemoState, B2MemoryCommandReceipt), HostCallError> {
        self.require_enabled()?;
        let broker = self
            .trusted_broker
            .clone()
            .ok_or(StoreError::MissingTrustedBrokerEnrollment)?;
        let store =
            Store::open_with_trusted_broker(&self.paths.store, self.authority.clone(), broker)?;
        if let Some(state) = store.b2_memory_demo_state()?
            && let Some(processing) = state.processing_operation.as_ref()
            && processing.request_id == consent.request_id
        {
            let consent_digest = serialized_sha256(consent)?;
            let consent_matches = state.receipts.iter().any(|receipt| {
                receipt.request_id == consent.request_id
                    && receipt.command_digest == consent_digest
                    && receipt.stage == openopen_protocol::B2MemoryDemoStage::Processing
            });
            if state.stage == openopen_protocol::B2MemoryDemoStage::Candidates
                && consent_matches
                && let Some(receipt) = state
                    .receipts
                    .iter()
                    .find(|receipt| {
                        receipt.request_id == processing.operation_id
                            && receipt.stage == openopen_protocol::B2MemoryDemoStage::Candidates
                    })
                    .cloned()
            {
                return Ok((state, receipt));
            }
            // A crash or restart while model work was in flight is ambiguous.
            // Never run a second model turn from a replayed consent.
            return Err(StoreError::B2MemoryDemoConflict.into());
        }
        let record = store
            .b2_memory_prepared_source()?
            .ok_or(StoreError::B2MemoryDemoConflict)?;
        if record.source.source_identity_digest != consent.source_identity_digest {
            return Err(StoreError::B2MemoryDemoConflict.into());
        }
        let pinned = pin_b2_memory_source(
            &record.selected_path,
            &record.source.request_id,
            record.prepared_at_ms,
        )?;
        if pinned != record {
            return Err(StoreError::B2MemoryDemoConflict.into());
        }
        let selection = store
            .selected_model_selection()?
            .ok_or(StoreError::ChoiceModelSelectionConflict)?;
        drop(store);

        let supervisor = DeepZipSupervisor::new(self.paths.deep_zip_runtime.clone());
        let cancellation = supervisor.cancellation_token();
        let operation_watch = operation.clone();
        let finished = Arc::new(AtomicBool::new(false));
        let finished_watch = finished.clone();
        let watcher = std::thread::spawn(move || {
            while !finished_watch.load(Ordering::Acquire) {
                if operation_watch.load(Ordering::Acquire) {
                    cancellation.cancel();
                    break;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        });
        let memory_context = supervisor
            .scan_memory_context(&record.selected_path)
            .map_err(|_| HostCallError::Internal);
        finished.store(true, Ordering::Release);
        let _ = watcher.join();
        let memory_context = memory_context?;
        let source_digest = hex::encode(memory_context.archive_sha256);
        if source_digest != record.source_digest {
            return Err(StoreError::B2MemoryDemoConflict.into());
        }
        let model_provenance = selection
            .turn_provenance(
                hashed_identifier("b2-provenance", consent)?,
                hashed_identifier("b2-turn", consent)?,
            )
            .ok_or(HostCallError::Internal)?;
        let seal = B2MemoryImportSeal {
            source_digest,
            catalog_digest: hex::encode(memory_context.catalog_digest),
            source_manifest_digest: b2_memory_source_manifest_digest(&memory_context)?,
            model_provenance,
        };
        let processing = self.operations.reconcile_active(operation, || {
            let broker = self
                .trusted_broker
                .clone()
                .ok_or(StoreError::MissingTrustedBrokerEnrollment)?;
            let mut store =
                Store::open_with_trusted_broker(&self.paths.store, self.authority.clone(), broker)?;
            store
                .begin_b2_memory_processing(
                    consent,
                    &seal,
                    &self.proof.authorization,
                    &self.proof.broker_receipt,
                    now_ms()?,
                )
                .map_err(Into::into)
        })?;
        if operation.load(Ordering::Acquire) {
            return Err(HostCallError::Codex(CodexError::Cancelled));
        }
        let (prompt, allowed_source_refs) = b2_memory_candidate_prompt(&memory_context)?;
        let request = MemoryCandidateGenerationRequest {
            prompt,
            allowed_source_refs,
            selected_model: SelectedModel {
                model_id: processing.model_provenance.model_id.clone(),
                reasoning_effort: (processing.model_provenance.actual_effort != "not_applicable")
                    .then(|| processing.model_provenance.actual_effort.clone()),
                catalog_fingerprint: processing.model_provenance.catalog_fingerprint.clone(),
                catalog_revision: processing.model_provenance.catalog_revision,
            },
            developer_instructions: MEMORY_CANDIDATE_DEVELOPER_INSTRUCTIONS.to_owned(),
        };
        request.validate().map_err(HostCallError::Codex)?;
        let workspace = ModelWorkspace::create(&self.paths.model_input_root)?;
        let generated = self.with_client(|client| {
            client
                .run_structured_memory_candidate_generation_in_workspace(&request, &workspace.path)
        })?;
        self.require_enabled()?;
        if operation.load(Ordering::Acquire) {
            return Err(HostCallError::Codex(CodexError::Cancelled));
        }
        let candidates = generated
            .candidates
            .into_iter()
            .map(|candidate| {
                Ok(B2MemoryCandidateCard {
                    id: candidate.id,
                    title: candidate.title,
                    rationale: candidate.rationale,
                    proposed_line: candidate.proposed_markdown_line,
                    source_binding_digest: serialized_sha256(&json!({
                        "operationId": processing.operation_id,
                        "sourceRefs": candidate.source_refs,
                    }))?,
                })
            })
            .collect::<Result<Vec<_>, HostCallError>>()?;
        let completed_at_ms = now_ms()?;
        let mut result = B2MemoryProcessingResult {
            operation_id: processing.operation_id,
            candidates,
            result_digest: String::new(),
            completed_at_ms,
        };
        result.result_digest = result
            .canonical_digest()
            .ok_or(StoreError::B2MemoryDemoConflict)?;
        self.operations.reconcile_active(operation, || {
            let broker = self
                .trusted_broker
                .clone()
                .ok_or(StoreError::MissingTrustedBrokerEnrollment)?;
            let mut store =
                Store::open_with_trusted_broker(&self.paths.store, self.authority.clone(), broker)?;
            store
                .commit_b2_memory_processing_result(
                    &result,
                    &self.proof.authorization,
                    &self.proof.broker_receipt,
                )
                .map_err(Into::into)
        })
    }

    /// Executes the private, already-persisted Markdown journal.  A caller
    /// cannot supply body bytes, a path, a manifest, or a receipt: all are
    /// recovered from the encrypted Store intent and verified by the
    /// descriptor-bound writer before a receipt can be committed.
    fn complete_markdown_render(
        &self,
        operation: &Arc<AtomicBool>,
        intent_id: &str,
        allow_reconciliation: bool,
    ) -> Result<ChoiceLoopSnapshot, HostCallError> {
        self.operations.reconcile_active(operation, || {
            let broker = self
                .trusted_broker
                .clone()
                .ok_or(StoreError::MissingTrustedBrokerEnrollment)?;
            let mut store =
                Store::open_with_trusted_broker(&self.paths.store, self.authority.clone(), broker)?;
            store.bind_choice_markdown_root(&self.paths.user_home)?;
            let prior_receipt = store.markdown_render_receipt(intent_id)?;
            // A durable receipt closes publication. Never re-enter the writer
            // after a crash between retained-base cleanup and body retirement:
            // cleanup is idempotent for an absent exact base, then retirement
            // is the sole remaining journal transition.
            if prior_receipt.is_some() {
                let intent = store
                    .markdown_render_intent(intent_id)?
                    .ok_or(StoreError::ChoiceLoopStateConflict)?;
                prepare_markdown_render_root(&self.paths.user_home, &intent.entry)
                    .map_err(|_| StoreError::ChoiceLoopStateConflict)?;
                return match store
                    .complete_verified_markdown_render_cleanup(intent_id, now_ms()?)?
                {
                    MarkdownRenderCleanup::Retired(snapshot) => Ok(*snapshot),
                    MarkdownRenderCleanup::ReconciliationRequired => {
                        Err(HostCallError::MarkdownReconciliationRequired)
                    }
                };
            }
            // Publishing work requires the current protected On runtime. A
            // receipt-authenticated retirement above is deletion-only and is
            // deliberately resumable after Off.
            self.require_enabled()?;
            let intent = store
                .markdown_render_intent(intent_id)?
                .ok_or(StoreError::ChoiceLoopStateConflict)?;
            prepare_markdown_render_root(&self.paths.user_home, &intent.entry)
                .map_err(|_| StoreError::ChoiceLoopStateConflict)?;
            let committed_at_ms = now_ms().map_err(|_| StoreError::ChoiceLoopStateConflict)?;
            match store.publish_markdown_render_intent(
                intent_id,
                allow_reconciliation,
                committed_at_ms,
            )? {
                Some(MarkdownRenderPublication::Committed(_)) => {}
                Some(MarkdownRenderPublication::ReconciliationRequired) => {
                    return Err(HostCallError::MarkdownReconciliationRequired);
                }
                None => return Err(StoreError::ChoiceLoopStateConflict.into()),
            }
            // The Store receipt is durable before the retained base can be
            // removed. Re-open the published entry before cleanup so an Owner
            // replacement in this narrow post-commit window cannot retire
            // bodies or claim local continuity from a stale final file.
            // If cleanup or subsequent retirement is interrupted, raw bodies
            // remain encrypted and restart enters the same verified recovery
            // path rather than claiming a completed Markdown update.
            match store.complete_verified_markdown_render_cleanup(&intent.id, now_ms()?)? {
                MarkdownRenderCleanup::Retired(snapshot) => Ok(*snapshot),
                MarkdownRenderCleanup::ReconciliationRequired => {
                    Err(HostCallError::MarkdownReconciliationRequired)
                }
            }
        })
    }

    fn commit_initial_choice_result(
        &self,
        operation: &Arc<AtomicBool>,
        result: &ChoiceInitialResult,
    ) -> Result<ChoiceLoopSnapshot, HostCallError> {
        self.require_enabled()?;
        self.operations.reconcile_active(operation, || {
            let broker = self
                .trusted_broker
                .clone()
                .ok_or(StoreError::MissingTrustedBrokerEnrollment)?;
            let mut store =
                Store::open_with_trusted_broker(&self.paths.store, self.authority.clone(), broker)?;
            store
                .commit_initial_choice_result(result)
                .map_err(Into::into)
        })
    }

    fn commit_choice_refinement_result(
        &self,
        operation: &Arc<AtomicBool>,
        result: &ChoiceRefinementResult,
    ) -> Result<ChoiceLoopSnapshot, HostCallError> {
        self.require_enabled()?;
        self.operations.reconcile_active(operation, || {
            let broker = self
                .trusted_broker
                .clone()
                .ok_or(StoreError::MissingTrustedBrokerEnrollment)?;
            let mut store =
                Store::open_with_trusted_broker(&self.paths.store, self.authority.clone(), broker)?;
            store
                .commit_choice_refinement_result(result)
                .map_err(Into::into)
        })
    }

    fn commit_choice_resume_result(
        &self,
        operation: &Arc<AtomicBool>,
        result: &ChoiceResumeResult,
    ) -> Result<ChoiceLoopSnapshot, HostCallError> {
        self.require_enabled()?;
        self.operations.reconcile_active(operation, || {
            let broker = self
                .trusted_broker
                .clone()
                .ok_or(StoreError::MissingTrustedBrokerEnrollment)?;
            let mut store =
                Store::open_with_trusted_broker(&self.paths.store, self.authority.clone(), broker)?;
            store
                .commit_choice_resume_result(result)
                .map_err(Into::into)
        })
    }

    fn block_initial_choice_operation(
        &self,
        operation: &Arc<AtomicBool>,
        operation_id: &str,
        expected_generation: u64,
    ) -> Result<ChoiceLoopSnapshot, HostCallError> {
        self.require_enabled()?;
        self.operations.reconcile_active(operation, || {
            let broker = self
                .trusted_broker
                .clone()
                .ok_or(StoreError::MissingTrustedBrokerEnrollment)?;
            let mut store =
                Store::open_with_trusted_broker(&self.paths.store, self.authority.clone(), broker)?;
            store
                .block_initial_choice_operation(operation_id, expected_generation, now_ms()?)
                .map_err(Into::into)
        })
    }

    fn block_choice_refinement_operation(
        &self,
        operation: &Arc<AtomicBool>,
        operation_id: &str,
        expected_generation: u64,
    ) -> Result<ChoiceLoopSnapshot, HostCallError> {
        self.require_enabled()?;
        self.operations.reconcile_active(operation, || {
            let broker = self
                .trusted_broker
                .clone()
                .ok_or(StoreError::MissingTrustedBrokerEnrollment)?;
            let mut store =
                Store::open_with_trusted_broker(&self.paths.store, self.authority.clone(), broker)?;
            store
                .block_choice_refinement_operation(operation_id, expected_generation, now_ms()?)
                .map_err(Into::into)
        })
    }

    #[cfg(test)]
    fn reconcile_channel_suggestion(
        &self,
        operation: &Arc<AtomicBool>,
        start: &ChannelModelStart,
        suggestion: OutcomeSuggestion,
    ) -> Result<(OutcomeSuggestion, bool), HostCallError> {
        self.require_enabled()?;
        self.operations.reconcile_active(operation, || {
            let broker = self
                .trusted_broker
                .clone()
                .ok_or(StoreError::MissingTrustedBrokerEnrollment)?;
            let mut store =
                Store::open_with_trusted_broker(&self.paths.store, self.authority.clone(), broker)?;
            store.record_channel_suggestion(
                start.envelope.channel,
                &start.envelope.source_message_id,
                &suggestion,
                now_ms()?,
            )?;
            let is_current = !store.channel_model_work_pending(start.envelope.channel)?
                && store
                    .latest_channel_suggestion_for(start.envelope.channel)?
                    .is_some_and(|latest| latest.id == suggestion.id);
            if is_current {
                *self
                    .operations
                    .suggestion
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(suggestion.clone());
            }
            Ok((suggestion, is_current))
        })
    }

    fn with_client<T>(
        &self,
        operation: impl FnOnce(&mut CodexClient) -> Result<T, CodexError>,
    ) -> Result<T, HostCallError> {
        self.require_enabled()?;
        let mut slot = self
            .codex
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let client = slot.as_mut().ok_or(HostCallError::Internal)?;
        operation(client).map_err(HostCallError::Codex)
    }

    fn require_enabled(&self) -> Result<(), HostCallError> {
        let broker = self
            .trusted_broker
            .clone()
            .ok_or(StoreError::MissingTrustedBrokerEnrollment)?;
        Store::open_with_trusted_broker(&self.paths.store, self.authority.clone(), broker)?
            .require_runtime_checkpoint(&self.proof.authorization, &self.proof.broker_receipt)
            .map_err(HostCallError::Store)
    }

    fn finish_operation(&self, token: &Arc<AtomicBool>) {
        self.operations.finish_operation(token);
    }

    fn retire_codex_runtime(&self) {
        self.operations.codex_cancel.store(true, Ordering::Release);
        self.operations.clear_model_catalog_snapshot();
        let client = self
            .codex
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        *self
            .codex_pid
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
        *self
            .instance_lease
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
        drop(client);
    }
}

fn choice_confirmation_reminders(
    steps: &[String],
    selection: &Selection,
    schedule: &ChoiceReminderScheduleInput,
) -> Result<(String, Vec<ChoiceReminderItem>, u32), HostCallError> {
    if steps.is_empty() || schedule.reminder_count == 0 {
        return Err(HostCallError::ReminderScheduleRequired);
    }
    // The accepted direction is the only currently-authoritative local task
    // text. A selected count creates that many ordered payload entries from
    // it; no question timestamp, hidden default, or unselected A/B/C card
    // supplies an item. Later richer accepted step lists retain the same
    // deterministic cycling rule rather than changing an existing proposal.
    let reminder_items = steps
        .iter()
        .cycle()
        .take(usize::try_from(schedule.reminder_count).map_err(|_| HostCallError::Internal)?)
        .enumerate()
        .map(|(index, text)| {
            Ok(ChoiceReminderItem {
                id: hashed_identifier(
                    "choice-reminder-item",
                    &json!({
                        "selection": selection.id(),
                        "index": index,
                        "text": text,
                        "dueAtMs": schedule.due_at_ms,
                    }),
                )?,
                text: text.clone(),
                // The confirmation binds a concrete timestamp rather than
                // inventing a later local-time interpretation. Actual
                // Reminder creation remains separately gated.
                due_at_ms: schedule.due_at_ms,
                time_zone: schedule.time_zone.clone(),
                evidence_intent: "reminder-readback".to_owned(),
            })
        })
        .collect::<Result<Vec<_>, HostCallError>>()?;
    let reminder_count =
        u32::try_from(reminder_items.len()).map_err(|_| HostCallError::Internal)?;
    if reminder_count != schedule.reminder_count {
        return Err(HostCallError::ReminderScheduleRequired);
    }
    Ok((
        schedule.reminder_list_id.clone(),
        reminder_items,
        reminder_count,
    ))
}

/// A reminder proposal carries an absolute instant and a user-selected IANA
/// zone. Parsing the zone at the Host boundary prevents an adapter or UI from
/// persisting a merely ASCII-shaped, but non-interpretable, local-time claim.
fn valid_choice_reminder_time_zone(value: &str) -> bool {
    value.parse::<chrono_tz::Tz>().is_ok()
}

fn choice_confirmation_delivery(
    session: &ChoiceSession,
) -> (Option<String>, Option<String>, Option<String>) {
    session
        .primary_delivery_binding_id
        .as_ref()
        .map_or((None, None, None), |binding| {
            (
                Some(binding.clone()),
                Some("local-owner".to_owned()),
                Some("local-only".to_owned()),
            )
        })
}

fn choice_confirmation_markdown_body(goal: &str, steps: &[String]) -> String {
    let mut body = String::from("# Confirmed choice\n\n## Goal\n\n");
    body.push_str(goal);
    body.push_str("\n\n## Steps\n");
    for step in steps {
        body.push_str("\n- ");
        body.push_str(step);
    }
    body.push('\n');
    body
}

/// Creates only the fixed private directories required by the command-owned
/// render entry.  Every created component is immediately rechecked as an
/// exact `0700` non-symlink directory; the writer subsequently reopens it by
/// descriptor and refuses any replacement.
fn prepare_markdown_render_root(
    user_home: &Path,
    entry: &DocumentManifestEntry,
) -> Result<PathBuf, HostError> {
    let root = user_home.join("Documents").join("OpenOpen");
    create_exact_private_directory(&root)?;
    let mut current = root.clone();
    let components = entry.relative_path.split('/').collect::<Vec<_>>();
    let Some((_file, parents)) = components.split_last() else {
        return Err(HostError::InvalidSupportPath);
    };
    for parent in parents {
        current.push(parent);
        create_exact_private_directory(&current)?;
    }
    Ok(root)
}

struct ModelWorkspace {
    path: PathBuf,
}

impl ModelWorkspace {
    fn create(root: &Path) -> Result<Self, HostCallError> {
        create_exact_private_directory(root)?;
        for _ in 0..16 {
            let mut random = [0_u8; 16];
            getrandom::fill(&mut random).map_err(|_| HostCallError::Internal)?;
            let path = root.join(format!("turn-{}", hex::encode(random)));
            match fs::create_dir(&path) {
                Ok(()) => {
                    fs::set_permissions(&path, fs::Permissions::from_mode(0o700))?;
                    return Ok(Self { path });
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(HostCallError::Io(error)),
            }
        }
        Err(HostCallError::Internal)
    }
}

impl Drop for ModelWorkspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

struct DurableChannelInbound<'a> {
    params: &'a PollChannel,
    observation: &'a ChannelObservation,
    ingested: &'a ChannelInboundResult,
    model_work_ready: bool,
}

fn empty_dashboard(
    runtime: impl Serialize,
    suggestion: Option<&OutcomeSuggestion>,
    incidents: impl Serialize,
) -> Value {
    json!({
        "activeCards": [],
        "channelFailureIncidents": incidents,
        "channelRouteSet": null,
        "confirmedMission": null,
        "microphone": {"available": false, "reason": "Microphone unavailable until Voice setup"},
        "needsYou": null,
        "receipt": null,
        "runtime": runtime,
        "suggestion": suggestion
    })
}

fn dashboard_focus<'a>(
    nonterminal: &[&'a Mission],
    confirmed: &[ConfirmedMission],
) -> Option<&'a Mission> {
    nonterminal
        .iter()
        .copied()
        .find(|mission| mission.status == MissionStatus::NeedsMe)
        .or_else(|| {
            confirmed.first().and_then(|candidate| {
                nonterminal
                    .iter()
                    .copied()
                    .find(|mission| mission.id == candidate.mission_id)
            })
        })
        .or_else(|| nonterminal.first().copied())
}

fn dashboard_active_cards(nonterminal: &[&Mission], focus: Option<&Mission>) -> Vec<Value> {
    let focus_id = focus.map(|mission| mission.id.as_str());
    focus
        .into_iter()
        .chain(nonterminal.iter().copied().filter(|mission| {
            Some(mission.id.as_str()) != focus_id && mission.status == MissionStatus::NeedsMe
        }))
        .chain(nonterminal.iter().copied().filter(|mission| {
            Some(mission.id.as_str()) != focus_id && mission.status != MissionStatus::NeedsMe
        }))
        .take(3)
        .map(|mission| {
            let state = match mission.status {
                MissionStatus::Active => "working",
                MissionStatus::NeedsMe => "Need you",
                MissionStatus::Paused => "Paused",
                MissionStatus::Proposed | MissionStatus::AwaitingConfirmation => {
                    "Awaiting confirmation"
                }
                MissionStatus::Completed | MissionStatus::Failed | MissionStatus::Cancelled => {
                    unreachable!()
                }
            };
            json!({"id": mission.id, "state": state, "title": mission.title})
        })
        .collect()
}

fn dashboard_needs_you(focus: Option<&Mission>) -> Option<MissionNeedsYou> {
    focus.and_then(|mission| {
        (mission.status == MissionStatus::NeedsMe)
            .then_some(mission.needs_me.as_ref())
            .flatten()
            .map(|needs_me| MissionNeedsYou {
                mission_id: mission.id.clone(),
                title: mission.title.clone(),
                prompt: needs_me.prompt.clone(),
                created_at_ms: needs_me.created_at_ms,
            })
    })
}

#[derive(Debug, Error)]
enum HostCallError {
    #[error("Store failed")]
    Store(#[from] StoreError),
    #[error("Codex failed")]
    Codex(#[from] CodexError),
    #[error("I/O failed")]
    Io(#[from] io::Error),
    #[error("host setup failed")]
    Host(#[from] HostError),
    #[error("persona verification failed")]
    Persona(#[from] PersonaError),
    #[error("channel listener is unavailable")]
    ChannelUnavailable,
    #[error("channel boundary verification failed")]
    ChannelIntegrity,
    #[error("another Mission still needs the owner")]
    MissionAlreadyInProgress,
    #[error("local Markdown reconciliation is required")]
    MarkdownReconciliationRequired,
    #[error("Choose a complete future Reminder schedule before review")]
    ReminderScheduleRequired,
    #[error("Choice clock continuity is uncertain")]
    ChoiceClockUncertain,
    #[error("Choice continuity advanced; refresh is required")]
    ChoiceRefreshRequired,
    #[error("internal failure")]
    Internal,
}

fn channel_runtime_failure(error: &ChannelRuntimeError) -> HostCallError {
    match error {
        ChannelRuntimeError::PairingMismatch => HostCallError::ChannelIntegrity,
        ChannelRuntimeError::Runtime(_)
        | ChannelRuntimeError::AlreadyRunning
        | ChannelRuntimeError::Adapter
        | ChannelRuntimeError::Recovery => HostCallError::ChannelUnavailable,
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SetEnabled {
    enabled: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CommitRuntime {
    authorization: RuntimeControlAuthorization,
    broker_receipt: RuntimeControlReceipt,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct InstallBroker {
    record: BrokerEnrollmentRecord,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct InstallCoreLease {
    lease: CoreInstanceLease,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SignBrokerEnrollment {
    broker_key_id: String,
    broker_verifying_key_hex: String,
    helper_designated_requirement_digest: String,
    installed_at_ms: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AwaitLogin {
    login_id: String,
    authorization: RuntimeControlAuthorization,
    broker_receipt: RuntimeControlReceipt,
}

impl AwaitLogin {
    fn proof(&self) -> RuntimeProof {
        RuntimeProof {
            authorization: self.authorization.clone(),
            broker_receipt: self.broker_receipt.clone(),
        }
    }
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RuntimeProof {
    authorization: RuntimeControlAuthorization,
    broker_receipt: RuntimeControlReceipt,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ApplyC2SkillDemo {
    command: C2SkillDemoCommand,
    authorization: RuntimeControlAuthorization,
    broker_receipt: RuntimeControlReceipt,
}

impl ApplyC2SkillDemo {
    fn proof(&self) -> RuntimeProof {
        RuntimeProof {
            authorization: self.authorization.clone(),
            broker_receipt: self.broker_receipt.clone(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct C2SkillDemoView {
    state: Option<C2SkillDemoState>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ApplyB2MemoryDemo {
    command: B2MemoryCommand,
    authorization: RuntimeControlAuthorization,
    broker_receipt: RuntimeControlReceipt,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PrepareB2MemorySource {
    request: B2MemoryPrepareSourceRequest,
    authorization: RuntimeControlAuthorization,
    broker_receipt: RuntimeControlReceipt,
}

impl PrepareB2MemorySource {
    fn proof(&self) -> RuntimeProof {
        RuntimeProof {
            authorization: self.authorization.clone(),
            broker_receipt: self.broker_receipt.clone(),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProcessB2MemorySource {
    consent: B2MemoryProcessingConsent,
    authorization: RuntimeControlAuthorization,
    broker_receipt: RuntimeControlReceipt,
}

impl ProcessB2MemorySource {
    fn proof(&self) -> RuntimeProof {
        RuntimeProof {
            authorization: self.authorization.clone(),
            broker_receipt: self.broker_receipt.clone(),
        }
    }
}

impl ApplyB2MemoryDemo {
    fn proof(&self) -> RuntimeProof {
        RuntimeProof {
            authorization: self.authorization.clone(),
            broker_receipt: self.broker_receipt.clone(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct B2MemoryDemoView {
    state: Option<B2MemoryDemoState>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PersonaStatusView {
    status: PersonaStatus,
    change_note: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StartDiscord {
    bot_token: String,
    authorization: RuntimeControlAuthorization,
    broker_receipt: RuntimeControlReceipt,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ConfirmDiscordSetup {
    candidate_id: String,
    confirmed_at_ms: i64,
    authorization: RuntimeControlAuthorization,
    broker_receipt: RuntimeControlReceipt,
}

impl ConfirmDiscordSetup {
    fn proof(&self) -> RuntimeProof {
        RuntimeProof {
            authorization: self.authorization.clone(),
            broker_receipt: self.broker_receipt.clone(),
        }
    }
}

impl StartDiscord {
    fn proof(&self) -> RuntimeProof {
        RuntimeProof {
            authorization: self.authorization.clone(),
            broker_receipt: self.broker_receipt.clone(),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ChannelSelection {
    channel: ChannelKind,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PollChannel {
    channel: ChannelKind,
    model_work_allowed: bool,
    authorization: RuntimeControlAuthorization,
    broker_receipt: RuntimeControlReceipt,
}

impl PollChannel {
    fn proof(&self) -> RuntimeProof {
        RuntimeProof {
            authorization: self.authorization.clone(),
            broker_receipt: self.broker_receipt.clone(),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AcknowledgeChannelFailure {
    incident_id: String,
    expected_incident_audit_anchor: EffectAuditAnchor,
    acknowledged_at_ms: i64,
    authorization: RuntimeControlAuthorization,
    broker_receipt: RuntimeControlReceipt,
}

impl AcknowledgeChannelFailure {
    fn proof(&self) -> RuntimeProof {
        RuntimeProof {
            authorization: self.authorization.clone(),
            broker_receipt: self.broker_receipt.clone(),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SendChannelMessage {
    mission_id: String,
    route_id: String,
    kind: ChannelMessageKind,
    content: String,
    approved_at_ms: i64,
    authorization: RuntimeControlAuthorization,
    broker_receipt: RuntimeControlReceipt,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AuthorizeChoiceIMessageReply {
    reply_id: String,
    preview_revision: u64,
    confirmation_digest: String,
    explicitly_approved: bool,
    authorization: RuntimeControlAuthorization,
    broker_receipt: RuntimeControlReceipt,
}

impl AuthorizeChoiceIMessageReply {
    fn proof(&self) -> RuntimeProof {
        RuntimeProof {
            authorization: self.authorization.clone(),
            broker_receipt: self.broker_receipt.clone(),
        }
    }
}

impl SendChannelMessage {
    fn proof(&self) -> RuntimeProof {
        RuntimeProof {
            authorization: self.authorization.clone(),
            broker_receipt: self.broker_receipt.clone(),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PairChannel {
    pairing: ChannelPairing,
    authorization: RuntimeControlAuthorization,
    broker_receipt: RuntimeControlReceipt,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BindChannelRoute {
    approval: ChannelRouteApproval,
    authorization: RuntimeControlAuthorization,
    broker_receipt: RuntimeControlReceipt,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReadChannelPairing {
    channel: ChannelKind,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SelectModel {
    model_id: String,
    requested_effort: String,
    catalog_snapshot_id: String,
    catalog_fingerprint: String,
    catalog_revision: u64,
    authorization: RuntimeControlAuthorization,
    broker_receipt: RuntimeControlReceipt,
}

impl SelectModel {
    fn proof(&self) -> RuntimeProof {
        RuntimeProof {
            authorization: self.authorization.clone(),
            broker_receipt: self.broker_receipt.clone(),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SelectChoice {
    #[serde(default)]
    selection: Option<Selection>,
    #[serde(default)]
    d_input: Option<ChoiceDInput>,
    authorization: RuntimeControlAuthorization,
    broker_receipt: RuntimeControlReceipt,
}

impl SelectChoice {
    fn proof(&self) -> RuntimeProof {
        RuntimeProof {
            authorization: self.authorization.clone(),
            broker_receipt: self.broker_receipt.clone(),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BeginChoice {
    #[serde(flatten)]
    input: ChoiceBeginRequest,
    authorization: RuntimeControlAuthorization,
    broker_receipt: RuntimeControlReceipt,
}

impl BeginChoice {
    fn proof(&self) -> RuntimeProof {
        RuntimeProof {
            authorization: self.authorization.clone(),
            broker_receipt: self.broker_receipt.clone(),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ConfirmChoice {
    confirmation: ChoiceConsolidatedConfirmation,
    authorization: RuntimeControlAuthorization,
    broker_receipt: RuntimeControlReceipt,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RecordChoiceReminderSchedule {
    input: ChoiceReminderScheduleInput,
    authorization: RuntimeControlAuthorization,
    broker_receipt: RuntimeControlReceipt,
}

impl RecordChoiceReminderSchedule {
    fn proof(&self) -> RuntimeProof {
        RuntimeProof {
            authorization: self.authorization.clone(),
            broker_receipt: self.broker_receipt.clone(),
        }
    }
}

impl ConfirmChoice {
    fn proof(&self) -> RuntimeProof {
        RuntimeProof {
            authorization: self.authorization.clone(),
            broker_receipt: self.broker_receipt.clone(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ModelSetup {
    account: AccountState,
    models: Vec<GptModel>,
    /// This is retained for local audit/provenance even when it no longer
    /// matches the current account or catalog. `selection_status` is the only
    /// readiness authority exposed to App state.
    selection: Option<ModelSelection>,
    selection_status: ModelSelectionStatus,
    catalog_snapshot_id: String,
    catalog_fingerprint: String,
    catalog_revision: u64,
}

#[derive(PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
enum ModelSelectionStatus {
    Current,
    Unselected,
    Unavailable,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg(test)]
struct ConfirmMission {
    suggestion_id: String,
    reminder_target: ReminderTarget,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct EmptyChoiceMarkdownReceiptCleanup {}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ChoiceMarkdownReceiptCleanupAvailability {
    available: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg(test)]
#[allow(dead_code)] // Historical recovery fixture type only.
struct CancelMission {
    mission_id: String,
    authorization: RuntimeControlAuthorization,
    broker_receipt: RuntimeControlReceipt,
}

#[cfg(test)]
#[allow(dead_code)] // Historical recovery fixture type only.
impl CancelMission {
    fn proof(&self) -> RuntimeProof {
        RuntimeProof {
            authorization: self.authorization.clone(),
            broker_receipt: self.broker_receipt.clone(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg(test)]
#[allow(dead_code)] // Historical recovery fixture type only.
struct MissionCancellation {
    mission_id: String,
    status: MissionStatus,
    audit_anchor: AuditAnchor,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReminderTarget {
    source_identifier: String,
    calendar_identifier: String,
}

impl ReminderTarget {
    #[cfg(test)]
    fn approval_target(&self) -> ApprovalTarget {
        ApprovalTarget::ReminderList {
            logical_list_id: DEFAULT_REMINDERS_LIST_ID.to_owned(),
            source_identifier: self.source_identifier.clone(),
            calendar_identifier: self.calendar_identifier.clone(),
        }
    }
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReminderCompletion {
    work_item_id: String,
    source_id: String,
    completed_at_ms: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg(test)]
struct CompleteReminders {
    mission_id: String,
    completions: Vec<ReminderCompletion>,
    receipt_return_approved_at_ms: Option<i64>,
    receipt_return_route_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg(test)]
struct BeginReminderDispatch {
    mission_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg(test)]
struct RecordReminderMirror {
    mission_id: String,
    links: Vec<ConfirmedReminderLink>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ConfirmedReminderLink {
    mission_id: String,
    work_item_id: String,
    source_identifier: String,
    calendar_identifier: String,
    calendar_item_identifier: String,
    dispatch_token: String,
    title: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct ConfirmedReminderDispatch {
    work_item_id: String,
    token: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
enum ReminderWriteDisposition {
    CreateOnce,
    RecoverOnly,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct ReminderAuthorization {
    mission_id: String,
    list_id: String,
    payload_sha256: String,
    approval_id: String,
    approval_digest: String,
    target: ReminderTarget,
    write_disposition: ReminderWriteDisposition,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct ConfirmedWorkItem {
    id: String,
    title: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct MissionNeedsYou {
    mission_id: String,
    title: String,
    prompt: String,
    created_at_ms: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct ConfirmedMission {
    mission_id: String,
    title: String,
    work_items: Vec<ConfirmedWorkItem>,
    reminder_authorization: ReminderAuthorization,
    reminder_dispatch: Vec<ConfirmedReminderDispatch>,
    reminder_links: Vec<ConfirmedReminderLink>,
    #[serde(skip_serializing_if = "Option::is_none")]
    choice_confirmation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    choice_payload_digest: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    choice_reminder_payload_digest: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    choice_reminder_items: Option<Vec<ChoiceReminderItem>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReminderDispatchStart {
    mission: ConfirmedMission,
    execute_now: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AuthorizeChoiceReminders {
    confirmation_id: String,
    reminder_target: ReminderTarget,
    authorization: RuntimeControlAuthorization,
    broker_receipt: RuntimeControlReceipt,
}

impl AuthorizeChoiceReminders {
    fn proof(&self) -> RuntimeProof {
        RuntimeProof {
            authorization: self.authorization.clone(),
            broker_receipt: self.broker_receipt.clone(),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ChoiceReminderRequest {
    confirmation_id: String,
    authorization: RuntimeControlAuthorization,
    broker_receipt: RuntimeControlReceipt,
}

impl ChoiceReminderRequest {
    fn proof(&self) -> RuntimeProof {
        RuntimeProof {
            authorization: self.authorization.clone(),
            broker_receipt: self.broker_receipt.clone(),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RecordChoiceReminderMirror {
    confirmation_id: String,
    links: Vec<ConfirmedReminderLink>,
    authorization: RuntimeControlAuthorization,
    broker_receipt: RuntimeControlReceipt,
}

impl RecordChoiceReminderMirror {
    fn proof(&self) -> RuntimeProof {
        RuntimeProof {
            authorization: self.authorization.clone(),
            broker_receipt: self.broker_receipt.clone(),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CompleteChoiceReminders {
    confirmation_id: String,
    completions: Vec<ReminderCompletion>,
    authorization: RuntimeControlAuthorization,
    broker_receipt: RuntimeControlReceipt,
}

impl CompleteChoiceReminders {
    fn proof(&self) -> RuntimeProof {
        RuntimeProof {
            authorization: self.authorization.clone(),
            broker_receipt: self.broker_receipt.clone(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ChoiceReminderCompletion {
    receipt: Receipt,
    choice_loop: ChoiceLoopSnapshot,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NoParams {}

/// Reads the fixed binary Keychain bootstrap frame from inherited stdin.
///
/// # Errors
///
/// Returns an error for a short, malformed, or incorrectly terminated frame.
pub fn read_bootstrap(reader: &mut impl Read) -> io::Result<[u8; 32]> {
    let mut magic = vec![0_u8; BOOTSTRAP_MAGIC.len()];
    reader.read_exact(&mut magic)?;
    if magic != BOOTSTRAP_MAGIC {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid private bootstrap frame",
        ));
    }
    let mut master = Zeroizing::new([0_u8; 32]);
    reader.read_exact(&mut *master)?;
    let mut terminator = [0_u8; 1];
    reader.read_exact(&mut terminator)?;
    if terminator != [b'\n'] {
        master.zeroize();
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid private bootstrap terminator",
        ));
    }
    Ok(*master)
}

fn parse_request(line: &str, responses: &Sender<RpcResponse>) -> Option<RpcRequest> {
    let Ok(value) = serde_json::from_str::<Value>(line) else {
        let _ = responses.send(failure(None, -32_700, "Parse error"));
        return None;
    };
    match serde_json::from_value::<RpcRequest>(value.clone()) {
        Ok(request) if params_are_structured(&request.params) => Some(request),
        Ok(request) => {
            let _ = responses.send(invalid_params(request.id));
            None
        }
        Err(_) if is_valid_notification(&value) => None,
        Err(_) => {
            let _ = responses.send(invalid_request(None));
            None
        }
    }
}

fn decode_params<T: for<'de> Deserialize<'de>>(request: &RpcRequest) -> Result<T, ()> {
    serde_json::from_value(request.params.clone()).map_err(|_| ())
}

fn params_are_structured(params: &Value) -> bool {
    params.is_object() || params.is_array() || params.is_null()
}

fn is_valid_notification(value: &Value) -> bool {
    let Some(object) = value.as_object() else {
        return false;
    };
    object.get("jsonrpc").and_then(Value::as_str) == Some("2.0")
        && object
            .get("method")
            .and_then(Value::as_str)
            .is_some_and(|method| !method.is_empty())
        && !object.contains_key("id")
        && object.get("params").is_none_or(params_are_structured)
}

fn is_public_family(method: &str) -> bool {
    [
        "account.",
        "mission.",
        "channel.",
        "receipt.",
        "workflow.",
        "choice.",
        "skill.",
    ]
    .iter()
    .any(|prefix| method.starts_with(prefix))
}

/// Channel setup and delivery are intentionally unavailable during the
/// PR1 Choice Core stage. Read-only status and historical dashboard recovery
/// remain outside this list.
fn is_pr1_deferred_channel_route(method: &str) -> bool {
    matches!(
        method,
        "channel.route.bind"
            | "channel.discord.setup.start"
            | "channel.discord.setup.poll"
            | "channel.discord.setup.confirm"
            | "channel.discord.start"
            | "channel.outbound.send"
    )
}

fn exact_canonical_file(path: &Path) -> Result<PathBuf, HostError> {
    let metadata = fs::symlink_metadata(path)?;
    if !path.is_absolute() || metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(HostError::InvalidBundle);
    }
    fs::canonicalize(path).map_err(HostError::Io)
}

fn exact_canonical_directory(path: &Path) -> Result<PathBuf, HostError> {
    let metadata = fs::symlink_metadata(path)?;
    if !path.is_absolute() || metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(HostError::InvalidSupportPath);
    }
    let canonical = fs::canonicalize(path)?;
    if canonical != path {
        return Err(HostError::InvalidSupportPath);
    }
    Ok(canonical)
}

fn create_exact_private_directory(path: &Path) -> Result<(), HostError> {
    if !path.is_absolute() {
        return Err(HostError::InvalidSupportPath);
    }
    match fs::create_dir(path) {
        Ok(()) => fs::set_permissions(path, fs::Permissions::from_mode(0o700))?,
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(HostError::Io(error)),
    }
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink()
        || !metadata.is_dir()
        || metadata.permissions().mode() & 0o077 != 0
        || fs::canonicalize(path)? != path
    {
        return Err(HostError::InvalidSupportPath);
    }
    Ok(())
}

fn now_ms() -> Result<i64, HostCallError> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| HostCallError::Internal)?;
    i64::try_from(duration.as_millis()).map_err(|_| HostCallError::Internal)
}

/// Returns a stable, OS-derived boot identity for the persisted idle clock.
/// A random Core instance nonce is intentionally not suitable here: it
/// changes on a process restart while the boot-scoped monotonic clock keeps
/// advancing.
/// The kernel boot time is local-only, non-secret, and remains stable for one
/// actual OS boot; its digest keeps the raw value out of diagnostics.
fn stable_idle_boot_id() -> Result<String, HostError> {
    let output = Command::new("/usr/sbin/sysctl")
        .args(["-n", "kern.boottime"])
        .output()
        .map_err(HostError::Io)?;
    if !output.status.success() {
        return Err(HostError::InvalidSupportPath);
    }
    let raw = String::from_utf8(output.stdout).map_err(|_| HostError::InvalidSupportPath)?;
    boot_identity_from_sysctl(&raw)
}

fn boot_identity_from_sysctl(raw: &str) -> Result<String, HostError> {
    let (fields, _) = raw
        .trim_start()
        .strip_prefix('{')
        .and_then(|value| value.split_once('}'))
        .ok_or(HostError::InvalidSupportPath)?;
    let mut seconds = None;
    let mut microseconds = None;
    for field in fields.split(',') {
        let (name, value) = field.split_once('=').ok_or(HostError::InvalidSupportPath)?;
        let name = name.trim();
        let value = value.trim();
        if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(HostError::InvalidSupportPath);
        }
        match name {
            "sec" if seconds.is_none() => {
                seconds = Some(
                    value
                        .parse::<u64>()
                        .map_err(|_| HostError::InvalidSupportPath)?,
                );
            }
            "usec" if microseconds.is_none() => {
                let parsed = value
                    .parse::<u32>()
                    .map_err(|_| HostError::InvalidSupportPath)?;
                if parsed >= 1_000_000 {
                    return Err(HostError::InvalidSupportPath);
                }
                microseconds = Some(parsed);
            }
            _ => return Err(HostError::InvalidSupportPath),
        }
    }
    let (seconds, microseconds) = seconds
        .zip(microseconds)
        .ok_or(HostError::InvalidSupportPath)?;
    let mut canonical = b"openopen:boot-time:v1\0".to_vec();
    canonical.extend_from_slice(&seconds.to_be_bytes());
    canonical.extend_from_slice(&microseconds.to_be_bytes());
    Ok(format!("boot-{:x}", Sha256::digest(canonical)))
}

/// Uses the kernel's boot-scoped monotonic clock. Unlike a process-local
/// `Instant`, this survives Core restart on the same OS boot. Darwin may pause
/// this clock during sleep; the Store compares it with wall progress and
/// classifies a sleep-shaped skew as uncertainty rather than granting time
/// authority. The separately persisted boot identity remains the reboot fence.
fn boot_scoped_monotonic_ms() -> Result<i64, HostCallError> {
    let value = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
    value
        .tv_sec
        .checked_mul(1_000)
        .and_then(|seconds| seconds.checked_add(value.tv_nsec / 1_000_000))
        .filter(|milliseconds| *milliseconds >= 0)
        .ok_or(HostCallError::Internal)
}

fn channel_observation(inbound: &TransportInbound) -> ChannelObservation {
    ChannelObservation {
        envelope: ChannelEnvelope {
            channel: inbound.channel,
            source_message_id: inbound.source_message_id.clone(),
            sender_id: inbound.sender_id.clone(),
            conversation_id: inbound.conversation_id.clone(),
            content_sha256: format!("{:x}", Sha256::digest(inbound.content.as_bytes())),
            received_at_ms: inbound.received_at_ms,
        },
        cursor: openopen_protocol::ChannelCursor {
            channel: inbound.channel,
            conversation_id: inbound.conversation_id.clone(),
            opaque_value: inbound.cursor_opaque_value.clone(),
            order: inbound.cursor_order,
            observed_at_ms: inbound.received_at_ms,
        },
        is_bot: false,
        explicitly_addressed: true,
    }
}

fn external_channel_payload(channel: ChannelKind, content: &str) -> Result<Vec<u8>, HostCallError> {
    let value = channel_message_payload(channel, content);
    if value.is_empty() || value.contains(&0) {
        return Err(HostCallError::Internal);
    }
    Ok(value)
}

fn model_catalog_snapshot_id(
    account: &AccountState,
    catalog_fingerprint: &str,
    catalog_revision: u64,
    runtime_revision: u64,
    issued_at_ms: i64,
) -> Result<String, HostCallError> {
    hashed_identifier(
        "model-catalog-snapshot",
        &json!({
            "account": account,
            "catalogFingerprint": catalog_fingerprint,
            "catalogRevision": catalog_revision,
            "issuedAtMs": issued_at_ms,
            "runtimeRevision": runtime_revision,
        }),
    )
}

fn bind_model_selection(
    snapshot: &ModelCatalogSnapshot,
    request: &SelectModel,
) -> Result<ModelSelection, HostCallError> {
    let AccountState::ChatGpt { plan_type, .. } = &snapshot.account else {
        return Err(HostCallError::Codex(CodexError::RequiredModelUnavailable));
    };
    let selected = snapshot
        .models
        .iter()
        .find(|model| model.id == request.model_id)
        .ok_or(HostCallError::Codex(CodexError::RequiredModelUnavailable))?;
    let actual_effort = if selected.supported_reasoning_efforts.is_empty() {
        if request.requested_effort != "not_applicable" {
            return Err(HostCallError::Codex(CodexError::RequiredModelUnavailable));
        }
        "not_applicable".to_owned()
    } else if request.requested_effort != "not_applicable"
        && selected
            .supported_reasoning_efforts
            .iter()
            .any(|effort| effort == &request.requested_effort)
    {
        request.requested_effort.clone()
    } else {
        return Err(HostCallError::Codex(CodexError::RequiredModelUnavailable));
    };
    let id = hashed_identifier(
        "model-selection",
        &json!({
            "catalogFingerprint": snapshot.catalog_fingerprint,
            "modelId": selected.id,
            "requestedEffort": request.requested_effort,
        }),
    )?;
    let selection = ModelSelection {
        id,
        model_id: selected.id.clone(),
        requested_effort: request.requested_effort.clone(),
        actual_effort,
        catalog_fingerprint: snapshot.catalog_fingerprint.clone(),
        catalog_revision: snapshot.catalog_revision,
        account_display_class: format!("chatgpt:{plan_type}"),
        protocol_schema_revision: 1,
    };
    selection
        .is_valid()
        .then_some(selection)
        .ok_or(HostCallError::Internal)
}

fn choice_begin_request_matches_selection(
    request: &ChoiceBeginRequest,
    selection: &ModelSelection,
) -> bool {
    request.expected_model_provenance_ref == selection.id
        && request.expected_catalog_fingerprint == selection.catalog_fingerprint
        && request.expected_catalog_revision == selection.catalog_revision
        && request.expected_protocol_revision == selection.protocol_schema_revision
}

fn choice_begin_source_manifest(
    question: &str,
    session_id: &str,
    accepted_at_ms: i64,
) -> Result<DocumentManifest, HostCallError> {
    let entry = DocumentManifestEntry {
        relative_path: format!("sessions/{session_id}/SESSION.md"),
        sha256: format!("{:x}", Sha256::digest(question.as_bytes())),
        byte_length: u64::try_from(question.len()).map_err(|_| HostCallError::Internal)?,
        mode: 0o600,
    };
    let aggregate_digest = canonical_document_manifest_digest(std::slice::from_ref(&entry))
        .ok_or(HostCallError::Internal)?;
    Ok(DocumentManifest {
        root_version: 1,
        entries: vec![entry],
        aggregate_digest,
        generated_at_ms: accepted_at_ms,
    })
}

fn new_choice_d_intake_record(
    input: &ChoiceDInput,
    snapshot: &ChoiceLoopSnapshot,
    accepted_at_ms: i64,
) -> Result<ChoiceDIntakeRecord, HostCallError> {
    if !input.is_valid()
        || snapshot.session.id != input.choice_session_id
        || snapshot.session.revision != input.expected_session_revision
        || snapshot.session.state != ChoiceSessionState::Active
    {
        return Err(HostCallError::Internal);
    }
    let choice_set = snapshot
        .active_choice_set
        .as_ref()
        .ok_or(HostCallError::Internal)?;
    if choice_set.id != input.choice_set_id
        || choice_set.session_revision != input.expected_session_revision
        || !choice_set.d_available
    {
        return Err(HostCallError::Internal);
    }
    // The caller-equivalence digest is derived solely from the bounded input
    // supplied by the user. Host acceptance time is recorded independently
    // on the envelope/batch and must not turn an exact retry into a new D
    // request.
    let request_digest = input.request_digest().ok_or(HostCallError::Internal)?;
    let source_envelope_id = hashed_identifier(
        "choice-d-source-envelope",
        &json!({"requestDigest": request_digest, "sessionId": snapshot.session.id}),
    )?;
    let batch_id = hashed_identifier(
        "choice-d-turn-batch",
        &json!({"sourceEnvelopeId": source_envelope_id, "sessionId": snapshot.session.id}),
    )?;
    let selection_id = hashed_identifier(
        "choice-d-selection",
        &json!({"requestDigest": request_digest, "choiceSetId": choice_set.id}),
    )?;
    let source_envelope = SourceEnvelope {
        id: source_envelope_id.clone(),
        surface: "mac".to_owned(),
        delivery_binding_id: snapshot
            .session
            .primary_delivery_binding_id
            .clone()
            .ok_or(HostCallError::Internal)?,
        provider_message_id: None,
        owner_id: ISSUER_ID.to_owned(),
        received_at_ms: accepted_at_ms,
        monotonic_sequence: snapshot.session.revision,
        body_digest: format!("{:x}", Sha256::digest(input.bounded_text.as_bytes())),
        attachment_manifest: None,
        third_party_data: false,
        session_hint: Some(snapshot.session.id.clone()),
        schema_version: choice_set.model_provenance.protocol_schema_revision,
    };
    let batch = ConversationTurnBatch {
        id: batch_id.clone(),
        choice_session_id: snapshot.session.id.clone(),
        delivery_binding_id: source_envelope.delivery_binding_id.clone(),
        source_envelope_ids: vec![source_envelope_id],
        opened_at_ms: accepted_at_ms,
        quiet_deadline_ms: accepted_at_ms + openopen_protocol::CHOICE_BATCH_QUIET_WINDOW_MS,
        hard_deadline_ms: accepted_at_ms + openopen_protocol::CHOICE_BATCH_HARD_WINDOW_MS,
        sealed_at_ms: Some(accepted_at_ms),
        seal_reason: Some(BatchSealReason::ImmediateRefinement),
        revision: snapshot.session.revision,
    };
    let selection = openopen_protocol::NaturalConversationSelection {
        id: selection_id,
        choice_session_id: snapshot.session.id.clone(),
        choice_set_id: choice_set.id.clone(),
        d_input_batch_id: batch_id,
        expected_session_revision: snapshot.session.revision,
        selected_at_ms: accepted_at_ms,
    };
    let record = ChoiceDIntakeRecord {
        input: input.clone(),
        request_digest,
        source_envelope,
        batch,
        selection,
    };
    record
        .is_valid()
        .then_some(record)
        .ok_or(HostCallError::Internal)
}

/// Runs the sealed first Choice generation after, and only after, the Host has
/// committed the interpreting intake transaction. The model controls only
/// bounded understanding text and three directions; every identity, source,
/// provenance, revision, and effect boundary below is Host-derived.
fn run_initial_choice_generation(
    context: &BackgroundContext,
    operation: &Arc<AtomicBool>,
    record: &ChoiceBeginRecord,
) -> Result<ChoiceLoopSnapshot, HostCallError> {
    context.require_enabled()?;
    if operation.load(Ordering::Acquire) {
        return Err(HostCallError::Codex(CodexError::Cancelled));
    }
    let source_ref = format!("local:{}", &record.source_manifest.aggregate_digest[..24]);
    let persona_bundle = context
        .persona
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .bundle_for_ref(&record.persona_revision)?;
    let request = ChoiceGenerationRequest {
        prompt: record.bounded_local_question.clone(),
        allowed_source_refs: vec![source_ref.clone()],
        selected_model: Some(SelectedModel {
            model_id: record.model_selection.model_id.clone(),
            reasoning_effort: (record.model_selection.actual_effort != "not_applicable")
                .then(|| record.model_selection.actual_effort.clone()),
            catalog_fingerprint: record.model_selection.catalog_fingerprint.clone(),
            catalog_revision: record.model_selection.catalog_revision,
        }),
        persona_revision: record.persona_revision.clone(),
        developer_instructions: persona_bundle
            .developer_instructions(true)
            .map_err(|_| HostCallError::Internal)?,
    };
    request.validate().map_err(HostCallError::Codex)?;
    let workspace = ModelWorkspace::create(&context.paths.model_input_root)?;
    let generated = context.with_client(|client| {
        client.run_structured_choice_generation_in_workspace(&request, &workspace.path)
    })?;
    context.require_enabled()?;
    if operation.load(Ordering::Acquire) {
        return Err(HostCallError::Codex(CodexError::Cancelled));
    }
    let result = initial_choice_result_from_generation(record, generated)?;
    context.commit_initial_choice_result(operation, &result)
}

/// Runs only the Host-owned continuation for a committed selection. The
/// operation already contains the exact selection, generation, model/catalog,
/// and manifest binding; no UI or RPC field supplies output authority.
fn run_private_refinement(
    context: &BackgroundContext,
    operation: &Arc<AtomicBool>,
    refinement: &ChoiceRefinementOperation,
) -> Result<ChoiceLoopSnapshot, HostCallError> {
    context.require_enabled()?;
    if operation.load(Ordering::Acquire) {
        return Err(HostCallError::Codex(CodexError::Cancelled));
    }
    // The operation metadata is intentionally body-free. Rehydrate its
    // separately encrypted, audit-bound semantic context only inside this
    // Host worker so a selected A/B/C direction or D text actually refines
    // the accepted owner choice rather than an opaque operation identifier.
    let (refinement_context, d_intake) = {
        let broker = context
            .trusted_broker
            .clone()
            .ok_or(StoreError::MissingTrustedBrokerEnrollment)?;
        let store = Store::open_with_trusted_broker(
            &context.paths.store,
            context.authority.clone(),
            broker,
        )?;
        (
            store.choice_refinement_context(refinement)?,
            store.choice_d_intake_for_refinement(refinement)?,
        )
    };
    let prompt = match (
        refinement_context.selected_option.as_ref(),
        d_intake.as_ref(),
    ) {
        (Some(option), None) => format!(
            "Refine the current local Choice using the selected direction. Current understanding: {}\nSelected direction: {}\nSelected rationale: {}\nReturn only the bounded structured Choice result.",
            refinement_context.interpretation.understood_goal, option.direction, option.rationale,
        ),
        (None, Some(record)) => format!(
            "Refine the current local Choice using the owner's D input. Current understanding: {}\nOwner input: {}\nReturn only the bounded structured Choice result.",
            refinement_context.interpretation.understood_goal, record.input.bounded_text,
        ),
        (None, None) if refinement.is_owner_resume() => format!(
            "Refresh the current local Choice after the owner's return. Current understanding: {}\nKnown context: {}\nReturn only the bounded structured Choice result.",
            refinement_context.interpretation.understood_goal,
            refinement_context.interpretation.current_context,
        ),
        _ => return Err(HostCallError::Internal),
    };
    if operation.load(Ordering::Acquire) {
        return Err(HostCallError::Codex(CodexError::Cancelled));
    }
    let persona_bundle = context
        .persona
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .bundle_for_ref(&refinement.persona_revision)?;
    let request = ChoiceGenerationRequest {
        prompt,
        allowed_source_refs: vec![format!(
            "local:{}",
            &refinement.source_manifest_digest[..24]
        )],
        selected_model: Some(SelectedModel {
            model_id: refinement.model_provenance.model_id.clone(),
            reasoning_effort: (refinement.model_provenance.actual_effort != "not_applicable")
                .then(|| refinement.model_provenance.actual_effort.clone()),
            catalog_fingerprint: refinement.model_provenance.catalog_fingerprint.clone(),
            catalog_revision: refinement.model_provenance.catalog_revision,
        }),
        persona_revision: refinement.persona_revision.clone(),
        developer_instructions: persona_bundle
            .developer_instructions(true)
            .map_err(|_| HostCallError::Internal)?,
    };
    request.validate().map_err(HostCallError::Codex)?;
    let workspace = ModelWorkspace::create(&context.paths.model_input_root)?;
    let generated = context.with_client(|client| {
        client.run_structured_choice_generation_in_workspace(&request, &workspace.path)
    })?;
    context.require_enabled()?;
    if operation.load(Ordering::Acquire) {
        return Err(HostCallError::Codex(CodexError::Cancelled));
    }
    if refinement.is_owner_resume() {
        let result = resume_result_from_generation(refinement, generated)?;
        context.commit_choice_resume_result(operation, &result)
    } else {
        let result = refinement_result_from_generation(refinement, generated)?;
        context.commit_choice_refinement_result(operation, &result)
    }
}

fn resume_result_from_generation(
    refinement: &ChoiceRefinementOperation,
    generated: StructuredChoiceGeneration,
) -> Result<ChoiceResumeResult, HostCallError> {
    let result = refinement_result_from_generation(refinement, generated)?;
    let resume = ChoiceResumeResult { result };
    resume
        .is_valid()
        .then_some(resume)
        .ok_or(HostCallError::Internal)
}

fn refinement_result_from_generation(
    refinement: &ChoiceRefinementOperation,
    generated: StructuredChoiceGeneration,
) -> Result<ChoiceRefinementResult, HostCallError> {
    let completed_at_ms = now_ms()?.max(refinement.created_at_ms);
    let interpretation_revision = refinement
        .expected_session_revision
        .checked_add(1)
        .ok_or(HostCallError::Internal)?;
    let options = generated
        .options
        .into_iter()
        .enumerate()
        .map(|(index, option)| {
            Ok(ChoiceOption {
                id: hashed_identifier(
                    "choice-refinement-option",
                    &json!({"operationId": refinement.id, "position": index + 1, "direction": option.direction}),
                )?,
                position: u8::try_from(index + 1).map_err(|_| HostCallError::Internal)?,
                direction: option.direction,
                rationale: option.rationale,
                expected_result: option.expected_result,
                information_needed: option.information_needed,
                external_effects_preview: option.external_effects_preview,
                source_categories: option.source_categories,
            })
        })
        .collect::<Result<Vec<_>, HostCallError>>()?;
    let choice_set = openopen_protocol::ChoiceSet {
        id: hashed_identifier(
            "choice-refinement-set",
            &json!({"operationId": refinement.id, "sessionRevision": interpretation_revision}),
        )?,
        choice_session_id: refinement.choice_session_id.clone(),
        session_revision: interpretation_revision,
        interpretation_revision,
        generated_at_ms: completed_at_ms,
        expires_on_revision: interpretation_revision,
        options,
        d_available: true,
        source_manifest_digest: refinement.source_manifest_digest.clone(),
        model_provenance: refinement.model_provenance.clone(),
        persona_revision: refinement.persona_revision.clone(),
    };
    let interpretation = InterpretationFrame {
        choice_session_id: refinement.choice_session_id.clone(),
        revision: interpretation_revision,
        understood_goal: generated.understood_goal,
        current_context: generated.current_context,
        assumptions: generated.assumptions,
        constraints: generated.constraints,
        uncertainties: generated.uncertainties,
        what_to_avoid: generated.what_to_avoid,
        source_manifest_digest: refinement.source_manifest_digest.clone(),
    };
    let mut result = ChoiceRefinementResult {
        operation_id: refinement.id.clone(),
        selection_id: refinement.selection_id.clone(),
        source_envelope_id: refinement.source_envelope_id.clone(),
        conversation_turn_batch_id: refinement.conversation_turn_batch_id.clone(),
        expected_session_revision: refinement.expected_session_revision,
        expected_generation: refinement.expected_generation,
        model_provenance: refinement.model_provenance.clone(),
        source_manifest_digest: refinement.source_manifest_digest.clone(),
        persona_revision: refinement.persona_revision.clone(),
        interpretation,
        choice_set,
        result_digest: String::new(),
        completed_at_ms,
    };
    result.result_digest = result
        .canonical_result_digest()
        .ok_or(HostCallError::Internal)?;
    result
        .is_valid()
        .then_some(result)
        .ok_or(HostCallError::Internal)
}

fn initial_choice_result_from_generation(
    record: &ChoiceBeginRecord,
    generated: StructuredChoiceGeneration,
) -> Result<ChoiceInitialResult, HostCallError> {
    let completed_at_ms = now_ms()?.max(record.accepted_at_ms);
    let provenance_id = hashed_identifier(
        "choice-provenance",
        &json!({
            "operationId": record.accepted.operation_id,
            "catalogFingerprint": record.model_selection.catalog_fingerprint,
            "catalogRevision": record.model_selection.catalog_revision,
        }),
    )?;
    let turn_id = hashed_identifier(
        "choice-turn",
        &json!({
            "operationId": record.accepted.operation_id,
            "sourceManifest": record.source_manifest.aggregate_digest,
        }),
    )?;
    let provenance = record
        .model_selection
        .turn_provenance(provenance_id, turn_id)
        .ok_or(HostCallError::Internal)?;
    let interpretation_revision = record
        .accepted
        .accepted_session_revision
        .checked_add(1)
        .ok_or(HostCallError::Internal)?;
    let options = generated
        .options
        .into_iter()
        .enumerate()
        .map(|(index, option)| {
            Ok(ChoiceOption {
                id: hashed_identifier(
                    "choice-option",
                    &json!({
                        "operationId": record.accepted.operation_id,
                        "position": index + 1,
                        "direction": option.direction,
                    }),
                )?,
                position: u8::try_from(index + 1).map_err(|_| HostCallError::Internal)?,
                direction: option.direction,
                rationale: option.rationale,
                expected_result: option.expected_result,
                information_needed: option.information_needed,
                external_effects_preview: option.external_effects_preview,
                source_categories: option.source_categories,
            })
        })
        .collect::<Result<Vec<_>, HostCallError>>()?;
    let choice_set = openopen_protocol::ChoiceSet {
        id: hashed_identifier(
            "choice-set",
            &json!({
                "operationId": record.accepted.operation_id,
                "sessionRevision": interpretation_revision,
            }),
        )?,
        choice_session_id: record.accepted.choice_session_id.clone(),
        session_revision: interpretation_revision,
        interpretation_revision,
        generated_at_ms: completed_at_ms,
        expires_on_revision: interpretation_revision,
        options,
        d_available: true,
        source_manifest_digest: record.source_manifest.aggregate_digest.clone(),
        model_provenance: provenance.clone(),
        persona_revision: record.persona_revision.clone(),
    };
    let interpretation = InterpretationFrame {
        choice_session_id: record.accepted.choice_session_id.clone(),
        revision: interpretation_revision,
        understood_goal: generated.understood_goal,
        current_context: generated.current_context,
        assumptions: generated.assumptions,
        constraints: generated.constraints,
        uncertainties: generated.uncertainties,
        what_to_avoid: generated.what_to_avoid,
        source_manifest_digest: record.source_manifest.aggregate_digest.clone(),
    };
    let result = ChoiceInitialResult {
        operation_id: record.accepted.operation_id.clone(),
        expected_session_revision: record.accepted.accepted_session_revision,
        expected_generation: record.runtime_revision,
        model_provenance: provenance,
        source_manifest_digest: record.source_manifest.aggregate_digest.clone(),
        persona_revision: record.persona_revision.clone(),
        interpretation,
        choice_set,
        completed_at_ms,
    };
    result
        .is_valid()
        .then_some(result)
        .ok_or(HostCallError::Internal)
}

#[allow(clippy::too_many_lines)] // Keeps the complete Host-derived intake seal auditable in one place.
fn new_choice_begin_state(
    request: &ChoiceBeginRequest,
    selection: &ModelSelection,
    session_revision: u64,
    runtime_revision: u64,
    persona_revision: &openopen_protocol::PersonaRevisionRef,
    accepted_at_ms: i64,
) -> Result<(ChoiceBeginRecord, ChoiceLoopSnapshot), HostCallError> {
    new_choice_begin_state_for_source(
        request,
        selection,
        session_revision,
        runtime_revision,
        persona_revision,
        accepted_at_ms,
        "mac",
        "mac-local-owner".to_owned(),
        None,
    )
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn new_choice_begin_state_for_source(
    request: &ChoiceBeginRequest,
    selection: &ModelSelection,
    session_revision: u64,
    runtime_revision: u64,
    persona_revision: &openopen_protocol::PersonaRevisionRef,
    accepted_at_ms: i64,
    surface: &str,
    delivery_binding_id: String,
    provider_message_id: Option<String>,
) -> Result<(ChoiceBeginRecord, ChoiceLoopSnapshot), HostCallError> {
    let request_digest = request.request_digest().ok_or(HostCallError::Internal)?;
    let session_id = hashed_identifier(
        "choice-session",
        &json!({"requestDigest": request_digest, "revision": session_revision}),
    )?;
    let operation_id = hashed_identifier(
        "choice-operation",
        &json!({"requestDigest": request_digest, "sessionId": session_id}),
    )?;
    let source_envelope_id = hashed_identifier(
        "choice-source-envelope",
        &json!({"operationId": operation_id, "requestDigest": request_digest}),
    )?;
    let batch_id = hashed_identifier(
        "choice-turn-batch",
        &json!({"sourceEnvelopeId": source_envelope_id, "sessionId": session_id}),
    )?;
    let question_digest = format!(
        "{:x}",
        Sha256::digest(request.bounded_local_question.as_bytes())
    );
    let source_envelope = SourceEnvelope {
        id: source_envelope_id.clone(),
        surface: surface.to_owned(),
        delivery_binding_id,
        provider_message_id,
        owner_id: ISSUER_ID.to_owned(),
        received_at_ms: accepted_at_ms,
        monotonic_sequence: session_revision,
        body_digest: question_digest.clone(),
        attachment_manifest: None,
        third_party_data: false,
        session_hint: None,
        schema_version: request.expected_protocol_revision,
    };
    let batch = ConversationTurnBatch {
        id: batch_id.clone(),
        choice_session_id: session_id.clone(),
        delivery_binding_id: source_envelope.delivery_binding_id.clone(),
        source_envelope_ids: vec![source_envelope_id.clone()],
        opened_at_ms: accepted_at_ms,
        quiet_deadline_ms: accepted_at_ms + openopen_protocol::CHOICE_BATCH_QUIET_WINDOW_MS,
        hard_deadline_ms: accepted_at_ms + openopen_protocol::CHOICE_BATCH_HARD_WINDOW_MS,
        sealed_at_ms: Some(accepted_at_ms),
        seal_reason: Some(BatchSealReason::InitialIntake),
        revision: session_revision,
    };
    let source_manifest =
        choice_begin_source_manifest(&request.bounded_local_question, &session_id, accepted_at_ms)?;
    let primary_delivery_binding_id = source_envelope.delivery_binding_id.clone();
    let accepted = ChoiceBeginAccepted {
        request_id: request.request_id.clone(),
        operation_id,
        choice_session_id: session_id.clone(),
        accepted_session_revision: session_revision,
        source_envelope_id,
        conversation_turn_batch_id: batch_id,
        state: ChoiceSessionState::Interpreting,
    };
    let record = ChoiceBeginRecord {
        accepted: accepted.clone(),
        request_digest,
        bounded_local_question: request.bounded_local_question.clone(),
        source_envelope,
        batch: batch.clone(),
        model_selection: selection.clone(),
        source_manifest: source_manifest.clone(),
        persona_revision: persona_revision.clone(),
        runtime_revision,
        accepted_at_ms,
    };
    let snapshot = choice_begin_snapshot(
        session_id,
        batch,
        source_manifest,
        selection,
        session_revision,
        accepted_at_ms,
        primary_delivery_binding_id,
    );
    if !record.is_valid() || !snapshot.is_valid() {
        return Err(HostCallError::Internal);
    }
    Ok((record, snapshot))
}

fn choice_begin_snapshot(
    session_id: String,
    batch: ConversationTurnBatch,
    source_manifest: DocumentManifest,
    selection: &ModelSelection,
    session_revision: u64,
    accepted_at_ms: i64,
    delivery_binding_id: String,
) -> ChoiceLoopSnapshot {
    ChoiceLoopSnapshot {
        session: ChoiceSession {
            id: session_id,
            state: ChoiceSessionState::Interpreting,
            revision: session_revision,
            model_selection_state: ModelSelectionState::Selected {
                model_provenance_ref: selection.id.clone(),
            },
            communication_profile_revision: 0,
            active_choice_set_id: None,
            active_interpretation_revision: None,
            opened_at_ms: accepted_at_ms,
            last_input_at_ms: accepted_at_ms,
            soft_idle_at_ms: accepted_at_ms + openopen_protocol::CHOICE_SESSION_SOFT_IDLE_MS,
            stale_review_at_ms: accepted_at_ms + openopen_protocol::CHOICE_SESSION_STALE_REVIEW_MS,
            primary_delivery_binding_id: Some(delivery_binding_id),
            pending_confirmation_id: None,
            background_mission_ids: Vec::new(),
        },
        active_batch: Some(batch),
        interpretation: None,
        active_choice_set: None,
        last_selection: None,
        pending_refinement_operation: None,
        confirmation: None,
        document_manifest: source_manifest,
    }
}

fn model_selection_status(
    account: &AccountState,
    models: &[GptModel],
    selection: Option<&ModelSelection>,
    catalog_fingerprint: &str,
    catalog_revision: u64,
) -> ModelSelectionStatus {
    let Some(selection) = selection else {
        return ModelSelectionStatus::Unselected;
    };
    let AccountState::ChatGpt { plan_type, .. } = account else {
        return ModelSelectionStatus::Unavailable;
    };
    let Some(model) = models.iter().find(|model| model.id == selection.model_id) else {
        return ModelSelectionStatus::Unavailable;
    };
    let effort_matches = if selection.requested_effort == "not_applicable" {
        model.supported_reasoning_efforts.is_empty() && selection.actual_effort == "not_applicable"
    } else {
        selection.actual_effort == selection.requested_effort
            && model
                .supported_reasoning_efforts
                .iter()
                .any(|effort| effort == &selection.requested_effort)
    };
    if selection.is_valid()
        && selection.account_display_class == format!("chatgpt:{plan_type}")
        && selection.catalog_fingerprint == catalog_fingerprint
        && selection.catalog_revision == catalog_revision
        && effort_matches
    {
        ModelSelectionStatus::Current
    } else {
        ModelSelectionStatus::Unavailable
    }
}

fn valid_model_selection_request(request: &SelectModel) -> bool {
    !request.model_id.is_empty()
        && request.model_id.len() <= 128
        && request
            .model_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        && (request.requested_effort == "not_applicable"
            || (!request.requested_effort.is_empty()
                && request.requested_effort.len() <= 32
                && request
                    .requested_effort
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte == b'-')))
        && is_lower_sha256(&request.catalog_snapshot_id)
        && is_lower_sha256(&request.catalog_fingerprint)
        && request.catalog_revision > 0
}

fn is_lower_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

#[cfg(test)]
fn run_channel_outcome(
    context: &BackgroundContext,
    start: &ChannelModelStart,
    model_context: &[(ChannelEnvelope, String)],
) -> Result<OutcomeSuggestion, HostCallError> {
    let outcome = channel_outcome_request(start, model_context)?;
    let workspace = ModelWorkspace::create(&context.paths.model_input_root)?;
    let value = context.with_client(|client| {
        client.run_structured_outcome_in_workspace(&outcome, &workspace.path)
    })?;
    let nonce = &serialized_sha256(&json!({
        "channel": start.envelope.channel,
        "sourceMessageId": start.envelope.source_message_id,
        "contentSha256": start.envelope.content_sha256,
    }))?[..32];
    let suggestion = OutcomeSuggestion {
        id: format!("suggestion-{}-{nonce}", start.envelope.received_at_ms),
        title: value.title,
        why_now: value.why_now,
        proposed_steps: value.proposed_steps,
        source_refs: value.source_refs,
    };
    if !valid_outcome_suggestion(&suggestion) {
        return Err(HostCallError::Internal);
    }
    Ok(suggestion)
}

#[cfg(test)]
fn channel_outcome_request(
    start: &ChannelModelStart,
    model_context: &[(ChannelEnvelope, String)],
) -> Result<OutcomeRequest, HostCallError> {
    let Some((latest_envelope, latest_content)) = model_context.last() else {
        return Err(HostCallError::Internal);
    };
    if model_context.len() > 2 {
        return Err(HostCallError::Internal);
    }
    if latest_envelope != &start.envelope || latest_content != &start.content {
        return Err(HostCallError::Internal);
    }
    if model_context.len() > 1 && !explicit_previous_message_correction(latest_content) {
        return Err(HostCallError::Internal);
    }
    let mut allowed_source_refs = Vec::with_capacity(model_context.len());
    for (envelope, _) in model_context {
        if envelope.channel != start.envelope.channel
            || envelope.sender_id != start.envelope.sender_id
            || envelope.conversation_id != start.envelope.conversation_id
        {
            return Err(HostCallError::Internal);
        }
        let source_digest = serialized_sha256(envelope)?;
        allowed_source_refs.push(format!("channel:{}", &source_digest[..24]));
    }
    let prompt = if model_context.len() == 1 {
        start.content.clone()
    } else {
        let mut prompt = String::from(
            "The following are exactly two chronological instructions from the same approved owner in the same approved conversation. The latest message explicitly begins with `Correction to previous:` and is authorized to revise only the immediately preceding message. Produce one final structured Outcome that honors the explicit correction and preserves only compatible details from that one predecessor.\n",
        );
        for (index, (_, content)) in model_context.iter().enumerate() {
            prompt.push_str("\nMessage ");
            prompt.push_str(&(index + 1).to_string());
            prompt.push_str(":\n");
            prompt.push_str(content);
            prompt.push('\n');
        }
        prompt
    };
    let persona = openopen_persona::PersonaBundle::embedded_default(1)
        .map_err(|_| HostCallError::Internal)?;
    let outcome = OutcomeRequest {
        prompt,
        allowed_source_refs,
        selected_model: None,
        persona_revision: persona.revision_ref.clone(),
        developer_instructions: persona
            .developer_instructions(true)
            .map_err(|_| HostCallError::Internal)?,
    };
    outcome.validate()?;
    Ok(outcome)
}

#[cfg(test)]
fn explicit_previous_message_correction(content: &str) -> bool {
    const PREFIX: &str = "correction to previous:";
    let Some(prefix) = content.get(..PREFIX.len()) else {
        return false;
    };
    prefix.eq_ignore_ascii_case(PREFIX)
        && content
            .get(PREFIX.len()..)
            .is_some_and(|remainder| !remainder.trim().is_empty())
}

#[cfg(test)]
fn valid_outcome_suggestion(suggestion: &OutcomeSuggestion) -> bool {
    let mut source_refs = HashSet::new();
    valid_suggestion_id(&suggestion.id)
        && !suggestion.title.trim().is_empty()
        && suggestion.title.chars().count() <= 120
        && !suggestion.why_now.trim().is_empty()
        && suggestion.why_now.chars().count() <= 300
        && !suggestion.proposed_steps.is_empty()
        && suggestion.proposed_steps.len() <= 8
        && suggestion
            .proposed_steps
            .iter()
            .all(|step| !step.trim().is_empty() && step.chars().count() <= 240)
        && suggestion.source_refs.iter().all(|source_ref| {
            !source_ref.is_empty()
                && source_ref.len() <= 128
                && source_ref.bytes().all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || matches!(byte, b'-' | b'_' | b':' | b'.')
                })
                && source_refs.insert(source_ref)
        })
}

fn valid_suggestion_id(suggestion_id: &str) -> bool {
    let Some((timestamp, nonce)) = suggestion_id
        .strip_prefix("suggestion-")
        .and_then(|value| value.split_once('-'))
    else {
        return false;
    };
    let Ok(timestamp) = timestamp.parse::<i64>() else {
        return false;
    };
    timestamp >= 0
        && nonce.len() == 32
        && nonce
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn mission_id_for_suggestion_id(suggestion_id: &str) -> Result<String, HostCallError> {
    if !valid_suggestion_id(suggestion_id) {
        return Err(HostCallError::Internal);
    }
    hashed_identifier("mission", &suggestion_id)
}

fn validated_reminder_links(
    mission: &Mission,
    target: &ReminderTarget,
    dispatch: &[ConfirmedReminderDispatch],
    links: Vec<ConfirmedReminderLink>,
) -> Option<Vec<ConfirmedReminderLink>> {
    if links.len() != mission.work_items.len() || dispatch.len() != mission.work_items.len() {
        return None;
    }
    let dispatch_by_work_item = dispatch
        .iter()
        .map(|item| (item.work_item_id.as_str(), item.token.as_str()))
        .collect::<BTreeMap<_, _>>();
    let mut by_work_item = links
        .into_iter()
        .map(|link| (link.work_item_id.clone(), link))
        .collect::<BTreeMap<_, _>>();
    if by_work_item.len() != mission.work_items.len() {
        return None;
    }
    let mut reminder_ids = HashSet::new();
    mission
        .work_items
        .iter()
        .map(|item| {
            let link = by_work_item.remove(&item.id)?;
            (link.mission_id == mission.id
                && link.title == item.title
                && link.source_identifier == target.source_identifier
                && link.calendar_identifier == target.calendar_identifier
                && dispatch_by_work_item.get(item.id.as_str())
                    == Some(&link.dispatch_token.as_str())
                && !link.calendar_item_identifier.trim().is_empty()
                && link.calendar_item_identifier.len() <= 512
                && reminder_ids.insert(link.calendar_item_identifier.clone()))
            .then_some(link)
        })
        .collect()
}

fn reminder_dispatch_claim(
    mission: &Mission,
    item: &WorkItem,
    authorization: &ReminderAuthorization,
) -> Result<(String, String), HostCallError> {
    let value = json!({
        "approvalDigest": authorization.approval_digest,
        "approvalId": authorization.approval_id,
        "calendarIdentifier": authorization.target.calendar_identifier,
        "missionId": mission.id,
        "payloadSha256": authorization.payload_sha256,
        "sourceIdentifier": authorization.target.source_identifier,
        "title": item.title,
        "version": 1,
        "workItemId": item.id,
    });
    let sha256 = serialized_sha256(&value)?;
    Ok((format!("dispatch-{}", &sha256[..24]), sha256))
}

fn reminder_dispatch_tokens(
    mission: &Mission,
    authorization: &ReminderAuthorization,
    authority: &LocalAuthority,
) -> Result<Vec<ConfirmedReminderDispatch>, HostCallError> {
    let dispatch = mission
        .evidence
        .iter()
        .filter(|evidence| evidence.kind == EvidenceKind::ReminderDispatchStarted)
        .collect::<Vec<_>>();
    if dispatch.is_empty() {
        return Ok(Vec::new());
    }
    if dispatch.len() != mission.work_items.len() {
        return Err(HostCallError::Internal);
    }
    mission
        .work_items
        .iter()
        .map(|item| {
            let matching = dispatch
                .iter()
                .filter(|evidence| evidence.work_item_id == item.id)
                .collect::<Vec<_>>();
            let [evidence] = matching.as_slice() else {
                return Err(HostCallError::Internal);
            };
            authority
                .verify_evidence(evidence)
                .map_err(|_| HostCallError::Internal)?;
            let (token, digest) = reminder_dispatch_claim(mission, item, authorization)?;
            if evidence.source_id != token || evidence.sha256.as_deref() != Some(digest.as_str()) {
                return Err(HostCallError::Internal);
            }
            Ok(ConfirmedReminderDispatch {
                work_item_id: item.id.clone(),
                token,
            })
        })
        .collect()
}

const MAX_REMINDER_DISPATCH_ATTEMPTS: u32 = 16;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ReminderDispatchAttemptState {
    index: u32,
    aborted: bool,
}

fn reminder_dispatch_retry_digest(
    mission_id: &str,
    dispatch: &ConfirmedReminderDispatch,
    attempt: u32,
) -> Result<String, HostCallError> {
    serialized_sha256(&json!({
        "attempt": attempt,
        "kind": "reminderDispatchRetryStarted",
        "missionId": mission_id,
        "token": dispatch.token,
        "workItemId": dispatch.work_item_id,
    }))
}

fn reminder_dispatch_abort_digest(
    mission_id: &str,
    dispatch: &ConfirmedReminderDispatch,
    attempt: u32,
) -> Result<String, HostCallError> {
    serialized_sha256(&json!({
        "attempt": attempt,
        "kind": "reminderDispatchAbortedBeforeCommit",
        "missionId": mission_id,
        "token": dispatch.token,
        "workItemId": dispatch.work_item_id,
    }))
}

fn legacy_reminder_dispatch_abort_digest(
    mission_id: &str,
    dispatch: &ConfirmedReminderDispatch,
) -> Result<String, HostCallError> {
    serialized_sha256(&json!({
        "kind": "reminderDispatchAbortedBeforeCommit",
        "missionId": mission_id,
        "token": dispatch.token,
        "workItemId": dispatch.work_item_id,
    }))
}

fn reminder_dispatch_attempt_state(
    mission: &Mission,
    dispatch: &[ConfirmedReminderDispatch],
    authority: &LocalAuthority,
) -> Result<Option<ReminderDispatchAttemptState>, HostCallError> {
    if dispatch.is_empty() {
        return Ok(None);
    }
    let retries = mission
        .evidence
        .iter()
        .filter(|evidence| evidence.kind == EvidenceKind::ReminderDispatchRetryStarted)
        .collect::<Vec<_>>();
    let aborted = mission
        .evidence
        .iter()
        .filter(|evidence| evidence.kind == EvidenceKind::ReminderDispatchAbortedBeforeCommit)
        .collect::<Vec<_>>();
    if dispatch.len() != mission.work_items.len()
        || retries.len() % dispatch.len() != 0
        || aborted.len() % dispatch.len() != 0
    {
        return Err(HostCallError::Internal);
    }
    let retry_count = retries.len() / dispatch.len();
    let abort_count = aborted.len() / dispatch.len();
    if retry_count > MAX_REMINDER_DISPATCH_ATTEMPTS as usize
        || (abort_count != retry_count && abort_count != retry_count + 1)
    {
        return Err(HostCallError::Internal);
    }
    for claim in dispatch {
        let matching_retries = retries
            .iter()
            .filter(|evidence| evidence.work_item_id == claim.work_item_id)
            .collect::<Vec<_>>();
        if matching_retries.len() != retry_count {
            return Err(HostCallError::Internal);
        }
        for (offset, evidence) in matching_retries.iter().enumerate() {
            authority
                .verify_evidence(evidence)
                .map_err(|_| HostCallError::Internal)?;
            let attempt = u32::try_from(offset + 1).map_err(|_| HostCallError::Internal)?;
            let digest = reminder_dispatch_retry_digest(&mission.id, claim, attempt)?;
            if evidence.source_id != claim.token
                || evidence.sha256.as_deref() != Some(digest.as_str())
            {
                return Err(HostCallError::Internal);
            }
        }
        let matching_aborts = aborted
            .iter()
            .filter(|evidence| evidence.work_item_id == claim.work_item_id)
            .collect::<Vec<_>>();
        if matching_aborts.len() != abort_count {
            return Err(HostCallError::Internal);
        }
        for (attempt, evidence) in matching_aborts.iter().enumerate() {
            authority
                .verify_evidence(evidence)
                .map_err(|_| HostCallError::Internal)?;
            let attempt = u32::try_from(attempt).map_err(|_| HostCallError::Internal)?;
            let digest = reminder_dispatch_abort_digest(&mission.id, claim, attempt)?;
            let legacy_digest = (attempt == 0)
                .then(|| legacy_reminder_dispatch_abort_digest(&mission.id, claim))
                .transpose()?;
            if evidence.source_id != claim.token
                || (evidence.sha256.as_deref() != Some(digest.as_str())
                    && evidence.sha256.as_deref() != legacy_digest.as_deref())
            {
                return Err(HostCallError::Internal);
            }
        }
    }
    Ok(Some(ReminderDispatchAttemptState {
        index: u32::try_from(retry_count).map_err(|_| HostCallError::Internal)?,
        aborted: abort_count == retry_count + 1,
    }))
}

fn reminder_mirror_digest(
    target: &ReminderTarget,
    link: &ConfirmedReminderLink,
) -> Result<String, HostCallError> {
    serialized_sha256(&json!({
        "calendarIdentifier": target.calendar_identifier,
        "calendarItemIdentifier": link.calendar_item_identifier,
        "dispatchToken": link.dispatch_token,
        "missionId": link.mission_id,
        "sourceIdentifier": target.source_identifier,
        "title": link.title,
        "version": 2,
        "workItemId": link.work_item_id,
    }))
}

fn reminder_mirror_links(
    mission: &Mission,
    target: &ReminderTarget,
    dispatch: &[ConfirmedReminderDispatch],
    authority: &LocalAuthority,
) -> Result<Vec<ConfirmedReminderLink>, HostCallError> {
    let mirrored = mission
        .evidence
        .iter()
        .filter(|evidence| evidence.kind == EvidenceKind::ReminderMirrored)
        .collect::<Vec<_>>();
    if mirrored.is_empty() {
        return Ok(Vec::new());
    }
    if mirrored.len() != mission.work_items.len() {
        return Err(HostCallError::Internal);
    }
    let dispatch_by_work_item = dispatch
        .iter()
        .map(|item| (item.work_item_id.as_str(), item.token.as_str()))
        .collect::<BTreeMap<_, _>>();
    if dispatch_by_work_item.len() != mission.work_items.len() {
        return Err(HostCallError::Internal);
    }
    mission
        .work_items
        .iter()
        .map(|item| {
            let matching = mirrored
                .iter()
                .filter(|evidence| evidence.work_item_id == item.id)
                .collect::<Vec<_>>();
            let [evidence] = matching.as_slice() else {
                return Err(HostCallError::Internal);
            };
            authority
                .verify_evidence(evidence)
                .map_err(|_| HostCallError::Internal)?;
            let link = ConfirmedReminderLink {
                mission_id: mission.id.clone(),
                work_item_id: item.id.clone(),
                source_identifier: target.source_identifier.clone(),
                calendar_identifier: target.calendar_identifier.clone(),
                calendar_item_identifier: evidence.source_id.clone(),
                dispatch_token: dispatch_by_work_item
                    .get(item.id.as_str())
                    .ok_or(HostCallError::Internal)?
                    .to_string(),
                title: item.title.clone(),
            };
            let digest = reminder_mirror_digest(target, &link)?;
            if evidence.sha256.as_deref() != Some(digest.as_str()) {
                return Err(HostCallError::Internal);
            }
            Ok(link)
        })
        .collect()
}

#[cfg(test)]
#[allow(dead_code)] // Historical recovery fixture helper only.
fn receipt_completion_time(
    route_set: Option<&ChannelRouteSet>,
    approved_at_ms: Option<i64>,
    route_id: Option<&str>,
    status: MissionStatus,
    mission_updated_at_ms: i64,
    observed_now: i64,
) -> Option<i64> {
    match (route_set, approved_at_ms, route_id, status) {
        (Some(routes), Some(approved), Some(route), MissionStatus::Active)
            if approved >= mission_updated_at_ms
                && approved <= observed_now
                && routes.routes.iter().any(|candidate| {
                    candidate.route_id == route
                        && candidate
                            .allowed_outbound_classes
                            .contains(&ChannelMessageKind::Receipt)
                }) =>
        {
            Some(approved)
        }
        (Some(routes), Some(_), Some(route), MissionStatus::Completed)
            if routes.routes.iter().any(|candidate| {
                candidate.route_id == route
                    && candidate
                        .allowed_outbound_classes
                        .contains(&ChannelMessageKind::Receipt)
            }) =>
        {
            Some(observed_now)
        }
        (Some(_), None, None, MissionStatus::Active | MissionStatus::Completed) => {
            Some(observed_now)
        }
        (None, None, None, _) => Some(observed_now),
        _ => None,
    }
}

fn approved_receipt_route<'a>(
    route_set: &'a ChannelRouteSet,
    route_id: Option<&str>,
) -> Result<&'a openopen_protocol::ChannelRoute, HostCallError> {
    let route_id = route_id.ok_or(HostCallError::Internal)?;
    route_set
        .routes
        .iter()
        .find(|route| {
            route.route_id == route_id
                && route
                    .allowed_outbound_classes
                    .contains(&ChannelMessageKind::Receipt)
        })
        .ok_or(HostCallError::Internal)
}

#[cfg(test)]
fn validated_reminder_completions(
    mission: &Mission,
    authority: &LocalAuthority,
    completions: Vec<ReminderCompletion>,
    observed_now_ms: i64,
    require_pending: bool,
) -> Option<BTreeMap<String, ReminderCompletion>> {
    let authorization =
        reminder_authorization_from_mission(mission, ReminderWriteDisposition::RecoverOnly)
            .ok()??;
    validated_reminder_completions_with_authorization(
        mission,
        authority,
        completions,
        observed_now_ms,
        require_pending,
        &authorization,
    )
}

fn validated_reminder_completions_with_authorization(
    mission: &Mission,
    authority: &LocalAuthority,
    completions: Vec<ReminderCompletion>,
    observed_now_ms: i64,
    require_pending: bool,
    authorization: &ReminderAuthorization,
) -> Option<BTreeMap<String, ReminderCompletion>> {
    let expected_status = if require_pending {
        WorkItemStatus::Pending
    } else {
        WorkItemStatus::Completed
    };
    if mission
        .work_items
        .iter()
        .any(|item| item.status != expected_status)
        || completions.len() != mission.work_items.len()
    {
        return None;
    }
    let dispatch = reminder_dispatch_tokens(mission, authorization, authority).ok()?;
    let dispatch_started_at_ms = mission
        .evidence
        .iter()
        .filter(|evidence| evidence.kind == EvidenceKind::ReminderDispatchStarted)
        .map(|evidence| evidence.observed_at_ms)
        .max()?;
    let mirror_links =
        reminder_mirror_links(mission, &authorization.target, &dispatch, authority).ok()?;
    if mirror_links.len() != mission.work_items.len() {
        return None;
    }
    let expected_sources = mirror_links
        .into_iter()
        .map(|link| (link.work_item_id, link.calendar_item_identifier))
        .collect::<BTreeMap<_, _>>();
    let mut source_ids = HashSet::new();
    let mut validated = BTreeMap::new();
    for completion in completions {
        if expected_sources.get(&completion.work_item_id) != Some(&completion.source_id)
            || completion.source_id.trim().is_empty()
            || completion.source_id.len() > 512
            || completion.completed_at_ms < dispatch_started_at_ms
            || completion.completed_at_ms < mission.created_at_ms
            || completion.completed_at_ms > observed_now_ms
            || !source_ids.insert(completion.source_id.clone())
            || validated
                .insert(completion.work_item_id.clone(), completion)
                .is_some()
        {
            return None;
        }
    }
    (validated.len() == mission.work_items.len()).then_some(validated)
}

fn mission_command_batch(
    expected_anchor: Option<&AuditAnchor>,
    mission_id: &str,
    commands: Vec<MissionCommand>,
) -> Result<Vec<MissionCommandEnvelope>, HostCallError> {
    commands
        .into_iter()
        .enumerate()
        .map(|(index, command)| {
            Ok(MissionCommandEnvelope {
                command_id: hashed_identifier(
                    "command",
                    &json!({"command": &command, "missionId": mission_id}),
                )?,
                expected_anchor: (index == 0).then(|| expected_anchor.cloned()).flatten(),
                command,
            })
        })
        .collect()
}

fn channel_send_approval_commands(
    mission: &Mission,
    proposal: &ActionProposal,
    payload: &[u8],
    outbound_id: &str,
    approved_at_ms: i64,
) -> Result<Vec<MissionCommand>, HostCallError> {
    let recipient_approval = hashed_identifier(
        "approval",
        &json!({"kind": "newRecipient", "outboundId": outbound_id}),
    )?;
    let disclosure_approval = hashed_identifier(
        "approval",
        &json!({"kind": "newDataShare", "outboundId": outbound_id}),
    )?;
    let times = (0_i64..6)
        .map(|offset| approved_at_ms.checked_add(offset))
        .collect::<Option<Vec<_>>>()
        .ok_or(HostCallError::Internal)?;
    Ok(vec![
        MissionCommand::RequestScopeChange {
            mission_id: mission.id.clone(),
            approval: NewBoundaryApproval {
                id: recipient_approval.clone(),
                kind: ApprovalKind::NewRecipient,
                prompt: "Return this exact update to the originating conversation?".into(),
                scope_digest: proposal
                    .approval_digest(ApprovalKind::NewRecipient, Some(payload))
                    .map_err(|_| HostCallError::Internal)?,
                target: None,
            },
            needs_me_id: hashed_identifier("needs-me", &recipient_approval)?,
            now_ms: times[0],
        },
        MissionCommand::DecideApproval {
            mission_id: mission.id.clone(),
            approval_id: recipient_approval,
            actor_id: mission.owner_id.clone(),
            decision: ApprovalDecision::Approve,
            now_ms: times[1],
        },
        MissionCommand::Resume {
            mission_id: mission.id.clone(),
            now_ms: times[2],
        },
        MissionCommand::RequestScopeChange {
            mission_id: mission.id.clone(),
            approval: NewBoundaryApproval {
                id: disclosure_approval.clone(),
                kind: ApprovalKind::NewDataShare,
                prompt: "Share these exact bytes with the originating conversation?".into(),
                scope_digest: proposal
                    .approval_digest(ApprovalKind::NewDataShare, Some(payload))
                    .map_err(|_| HostCallError::Internal)?,
                target: None,
            },
            needs_me_id: hashed_identifier("needs-me", &disclosure_approval)?,
            now_ms: times[3],
        },
        MissionCommand::DecideApproval {
            mission_id: mission.id.clone(),
            approval_id: disclosure_approval,
            actor_id: mission.owner_id.clone(),
            decision: ApprovalDecision::Approve,
            now_ms: times[4],
        },
        MissionCommand::Resume {
            mission_id: mission.id.clone(),
            now_ms: times[5],
        },
    ])
}

fn reminder_write_proposal(mission_id: &str, scope_digest: &str) -> ActionProposal {
    ActionProposal {
        effect: EffectKind::ReminderWrite,
        mission_id: mission_id.to_owned(),
        mission_scope_digest: scope_digest.to_owned(),
        target: ActionTarget::ReminderList {
            list_id: DEFAULT_REMINDERS_LIST_ID.to_owned(),
        },
        estimated_cost_micros: None,
    }
}

fn reminder_write_payload<'a>(
    mission_id: &str,
    target: &ReminderTarget,
    work_items: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> Result<Vec<u8>, HostCallError> {
    let mut payload = REMINDER_WRITE_PAYLOAD_PREFIX.to_vec();
    append_framed_field(&mut payload, mission_id)?;
    append_framed_field(&mut payload, DEFAULT_REMINDERS_LIST_ID)?;
    append_framed_field(&mut payload, &target.source_identifier)?;
    append_framed_field(&mut payload, &target.calendar_identifier)?;
    for (id, title) in work_items {
        append_framed_field(&mut payload, id)?;
        append_framed_field(&mut payload, title)?;
    }
    Ok(payload)
}

fn choice_reminder_write_payload(
    mission_id: &str,
    confirmation: &ChoiceConsolidatedConfirmation,
    target: &ReminderTarget,
) -> Result<Vec<u8>, HostCallError> {
    let mut payload = REMINDER_WRITE_PAYLOAD_PREFIX.to_vec();
    append_framed_field(&mut payload, mission_id)?;
    append_framed_field(&mut payload, &confirmation.id)?;
    append_framed_field(&mut payload, &confirmation.payload_digest)?;
    append_framed_field(&mut payload, &confirmation.reminder_payload_digest)?;
    append_framed_field(&mut payload, &confirmation.reminder_list_id)?;
    append_framed_field(&mut payload, &target.source_identifier)?;
    append_framed_field(&mut payload, &target.calendar_identifier)?;
    for item in &confirmation.reminder_items {
        append_framed_field(&mut payload, &item.id)?;
        append_framed_field(&mut payload, &item.text)?;
        payload.extend(item.due_at_ms.to_be_bytes());
        append_framed_field(&mut payload, &item.time_zone)?;
        append_framed_field(&mut payload, &item.evidence_intent)?;
    }
    Ok(payload)
}

fn choice_mission_id(
    confirmation: &ChoiceConsolidatedConfirmation,
) -> Result<String, HostCallError> {
    hashed_identifier(
        "choice-mission",
        &json!({
            "confirmationId": confirmation.id,
            "payloadDigest": confirmation.payload_digest,
        }),
    )
}

fn append_framed_field(payload: &mut Vec<u8>, value: &str) -> Result<(), HostCallError> {
    let length = u64::try_from(value.len()).map_err(|_| HostCallError::Internal)?;
    payload.extend(length.to_be_bytes());
    payload.extend(value.as_bytes());
    Ok(())
}

fn confirmed_mission_from_mission(
    mission: &Mission,
    authority: &LocalAuthority,
    write_disposition: ReminderWriteDisposition,
) -> Result<Option<ConfirmedMission>, HostCallError> {
    if mission.status != MissionStatus::Active {
        return Ok(None);
    }
    let Some(reminder_authorization) =
        reminder_authorization_from_mission(mission, write_disposition)?
    else {
        return Ok(None);
    };
    let payload = reminder_write_payload(
        &mission.id,
        &reminder_authorization.target,
        mission
            .work_items
            .iter()
            .map(|item| (item.id.as_str(), item.title.as_str())),
    )?;
    let proposal = reminder_write_proposal(&mission.id, &mission.scope_digest);
    if ActionGate.authorize(mission, &proposal, Some(&payload)) != GateDecision::Allowed {
        return Ok(None);
    }
    let reminder_dispatch = reminder_dispatch_tokens(mission, &reminder_authorization, authority)?;
    let reminder_links = reminder_mirror_links(
        mission,
        &reminder_authorization.target,
        &reminder_dispatch,
        authority,
    )?;
    Ok(Some(ConfirmedMission {
        mission_id: mission.id.clone(),
        title: mission.title.clone(),
        work_items: mission
            .work_items
            .iter()
            .map(|item| ConfirmedWorkItem {
                id: item.id.clone(),
                title: item.title.clone(),
            })
            .collect(),
        reminder_authorization,
        reminder_dispatch,
        reminder_links,
        choice_confirmation_id: None,
        choice_payload_digest: None,
        choice_reminder_payload_digest: None,
        choice_reminder_items: None,
    }))
}

fn confirmed_choice_mission_from_mission(
    mission: &Mission,
    confirmation: &ChoiceConsolidatedConfirmation,
    authority: &LocalAuthority,
    write_disposition: ReminderWriteDisposition,
) -> Result<Option<ConfirmedMission>, HostCallError> {
    if mission.status != MissionStatus::Active
        || mission.id != choice_mission_id(confirmation)?
        || mission.scope_digest != confirmation.payload_digest
        || mission.work_items.len() != confirmation.reminder_items.len()
        || !mission
            .work_items
            .iter()
            .zip(&confirmation.reminder_items)
            .all(|(work, item)| work.id == item.id && work.title == item.text)
    {
        return Ok(None);
    }
    let approvals = mission
        .approvals
        .iter()
        .filter(|approval| {
            approval.kind == ApprovalKind::NewExternalWrite
                && approval.work_item_id.is_none()
                && approval.status == ApprovalStatus::Approved
                && approval.decided_by_id.as_deref() == Some(mission.owner_id.as_str())
        })
        .collect::<Vec<_>>();
    let [approval] = approvals.as_slice() else {
        return Ok(None);
    };
    let Some(ApprovalTarget::ReminderList {
        logical_list_id,
        source_identifier,
        calendar_identifier,
    }) = approval.target.as_ref()
    else {
        return Ok(None);
    };
    if logical_list_id != &confirmation.reminder_list_id {
        return Ok(None);
    }
    let target = ReminderTarget {
        source_identifier: source_identifier.clone(),
        calendar_identifier: calendar_identifier.clone(),
    };
    if !valid_reminder_target(&target) {
        return Err(HostCallError::Internal);
    }
    let payload = choice_reminder_write_payload(&mission.id, confirmation, &target)?;
    let proposal = reminder_write_proposal(&mission.id, &mission.scope_digest);
    let approval_digest = proposal
        .approval_digest(ApprovalKind::NewExternalWrite, Some(&payload))
        .map_err(|_| HostCallError::Internal)?;
    if approval.scope_digest != approval_digest
        || ActionGate.authorize(mission, &proposal, Some(&payload)) != GateDecision::Allowed
    {
        return Ok(None);
    }
    let authorization = ReminderAuthorization {
        mission_id: mission.id.clone(),
        list_id: confirmation.reminder_list_id.clone(),
        payload_sha256: format!("{:x}", Sha256::digest(&payload)),
        approval_id: approval.id.clone(),
        approval_digest,
        target: target.clone(),
        write_disposition,
    };
    let reminder_dispatch = reminder_dispatch_tokens(mission, &authorization, authority)?;
    let reminder_links = reminder_mirror_links(mission, &target, &reminder_dispatch, authority)?;
    Ok(Some(ConfirmedMission {
        mission_id: mission.id.clone(),
        title: mission.title.clone(),
        work_items: mission
            .work_items
            .iter()
            .map(|item| ConfirmedWorkItem {
                id: item.id.clone(),
                title: item.title.clone(),
            })
            .collect(),
        reminder_authorization: authorization,
        reminder_dispatch,
        reminder_links,
        choice_confirmation_id: Some(confirmation.id.clone()),
        choice_payload_digest: Some(confirmation.payload_digest.clone()),
        choice_reminder_payload_digest: Some(confirmation.reminder_payload_digest.clone()),
        choice_reminder_items: Some(confirmation.reminder_items.clone()),
    }))
}

fn reminder_authorization_from_mission(
    mission: &Mission,
    write_disposition: ReminderWriteDisposition,
) -> Result<Option<ReminderAuthorization>, HostCallError> {
    let candidates = mission
        .approvals
        .iter()
        .filter(|approval| {
            approval.kind == ApprovalKind::NewExternalWrite
                && approval.work_item_id.is_none()
                && approval.status == ApprovalStatus::Approved
                && approval.decided_by_id.as_deref() == Some(mission.owner_id.as_str())
        })
        .collect::<Vec<_>>();
    let [approval] = candidates.as_slice() else {
        return Ok(None);
    };
    let Some(ApprovalTarget::ReminderList {
        logical_list_id,
        source_identifier,
        calendar_identifier,
    }) = approval.target.as_ref()
    else {
        return Ok(None);
    };
    if logical_list_id != DEFAULT_REMINDERS_LIST_ID {
        return Ok(None);
    }
    let target = ReminderTarget {
        source_identifier: source_identifier.clone(),
        calendar_identifier: calendar_identifier.clone(),
    };
    if !valid_reminder_target(&target) {
        return Err(HostCallError::Internal);
    }
    let payload = reminder_write_payload(
        &mission.id,
        &target,
        mission
            .work_items
            .iter()
            .map(|item| (item.id.as_str(), item.title.as_str())),
    )?;
    let proposal = reminder_write_proposal(&mission.id, &mission.scope_digest);
    let approval_digest = proposal
        .approval_digest(ApprovalKind::NewExternalWrite, Some(&payload))
        .map_err(|_| HostCallError::Internal)?;
    if approval.scope_digest != approval_digest {
        return Ok(None);
    }
    Ok(Some(ReminderAuthorization {
        mission_id: mission.id.clone(),
        list_id: DEFAULT_REMINDERS_LIST_ID.to_owned(),
        payload_sha256: format!("{:x}", Sha256::digest(&payload)),
        approval_id: approval.id.clone(),
        approval_digest,
        target: target.clone(),
        write_disposition,
    }))
}

fn valid_reminder_target(target: &ReminderTarget) -> bool {
    !target.source_identifier.trim().is_empty()
        && target.source_identifier.len() <= 512
        && target.source_identifier == target.source_identifier.trim()
        && !target.calendar_identifier.trim().is_empty()
        && target.calendar_identifier.len() <= 512
        && target.calendar_identifier == target.calendar_identifier.trim()
}

fn new_reminder_receipt(
    mission: &Mission,
    selection: &ModelSelection,
    evidence_ids: &[String],
    completed_at_ms: i64,
) -> Result<NewReceipt, HostCallError> {
    Ok(NewReceipt {
        id: hashed_identifier(
            "receipt",
            &json!({"evidenceIds": evidence_ids, "missionId": mission.id}),
        )?,
        summary: format!(
            "Completed {} with {} verified Reminders.",
            mission.title,
            mission.work_items.len()
        ),
        actual_model: selection.model_id.clone(),
        output_hashes: vec![mission.scope_digest.clone()],
        completed_at_ms,
    })
}

fn reminder_evidence_id(
    mission_id: &str,
    work_item_id: &str,
    completion: &ReminderCompletion,
) -> Result<String, HostCallError> {
    hashed_identifier(
        "evidence",
        &json!({
            "completedAtMs": completion.completed_at_ms,
            "missionId": mission_id,
            "sourceId": completion.source_id,
            "workItemId": work_item_id
        }),
    )
}

fn hashed_identifier(prefix: &str, value: &impl serde::Serialize) -> Result<String, HostCallError> {
    Ok(format!("{prefix}-{}", &serialized_sha256(value)?[..24]))
}

fn render_choice_imessage_reply(
    choice_set: &openopen_protocol::ChoiceSet,
) -> Result<String, HostCallError> {
    if !choice_set.is_valid() {
        return Err(HostCallError::Internal);
    }
    let labels = ["A", "B", "C"];
    let mut lines = vec!["OpenOpen · AI".to_owned()];
    for (label, option) in labels.into_iter().zip(&choice_set.options) {
        lines.push(format!("{label} — {}", option.direction));
        lines.push(option.rationale.clone());
    }
    lines.push("D — Something else".to_owned());
    lines.push("Describe what these options missed.".to_owned());
    let body = lines.join("\n");
    if body.chars().count() > 2_000 || body.as_bytes().contains(&0) {
        return Err(HostCallError::Internal);
    }
    Ok(body)
}

fn serialized_sha256(value: &impl serde::Serialize) -> Result<String, HostCallError> {
    let encoded = serde_json::to_vec(value).map_err(|_| HostCallError::Internal)?;
    Ok(format!("{:x}", Sha256::digest(encoded)))
}

fn pin_b2_memory_source(
    selected_path: &Path,
    request_id: &str,
    prepared_at_ms: i64,
) -> Result<B2MemoryPreparedSourceRecord, HostCallError> {
    if !selected_path.is_absolute() || prepared_at_ms < 0 {
        return Err(HostCallError::Internal);
    }
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(selected_path)
        .map_err(|_| HostCallError::Internal)?;
    let before = file.metadata().map_err(|_| HostCallError::Internal)?;
    if !before.is_file()
        || before.len() == 0
        || before.len() > openopen_deep_zip_worker::FrozenLimits::MAX_ARCHIVE_BYTES
        || before.dev() == 0
        || before.ino() == 0
    {
        return Err(HostCallError::Internal);
    }
    let mut digest = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1_024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|_| HostCallError::Internal)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    buffer.zeroize();
    let after = file.metadata().map_err(|_| HostCallError::Internal)?;
    if before.dev() != after.dev()
        || before.ino() != after.ino()
        || before.len() != after.len()
        || before.mtime() != after.mtime()
        || before.mtime_nsec() != after.mtime_nsec()
    {
        return Err(HostCallError::Internal);
    }
    let modified_at_ns = before
        .mtime()
        .checked_mul(1_000_000_000)
        .and_then(|value| value.checked_add(before.mtime_nsec()))
        .filter(|value| *value >= 0)
        .ok_or(HostCallError::Internal)?;
    let source_digest = format!("{:x}", digest.finalize());
    let source_identity_digest = serialized_sha256(&json!({
        "deviceId": before.dev(),
        "inode": before.ino(),
        "byteLength": before.len(),
        "modifiedAtNs": modified_at_ns,
        "sourceDigest": source_digest,
    }))?;
    Ok(B2MemoryPreparedSourceRecord {
        source: B2MemoryPreparedSource {
            request_id: request_id.to_owned(),
            source_identity_digest,
            byte_length: before.len(),
        },
        selected_path: selected_path.to_owned(),
        device_id: before.dev(),
        inode: before.ino(),
        modified_at_ns,
        source_digest,
        prepared_at_ms,
    })
}

fn b2_memory_source_manifest_digest(
    context: &DeepZipMemoryContext,
) -> Result<String, HostCallError> {
    serialized_sha256(
        &context
            .conversations
            .iter()
            .map(|conversation| {
                json!({
                    "path": conversation.source_path,
                    "sha256": hex::encode(conversation.source_sha256),
                    "conversationIndex": conversation.conversation_index,
                    "contextDigest": hex::encode(conversation.context_digest),
                })
            })
            .collect::<Vec<_>>(),
    )
}

fn b2_memory_candidate_prompt(
    context: &DeepZipMemoryContext,
) -> Result<(String, Vec<String>), HostCallError> {
    const PROMPT_LIMIT: usize = 15 * 1_024;
    const MIN_CONTEXT_ROOM: usize = 256;
    if !context.is_valid() {
        return Err(HostCallError::Internal);
    }
    let mut prompt = String::from(
        "Review only the bounded ChatGPT conversation excerpts below. Propose one to three useful, durable local Memory lines. Cite at least one exact SOURCE reference per candidate.\n",
    );
    let mut refs = Vec::new();
    for conversation in &context.conversations {
        if PROMPT_LIMIT.saturating_sub(prompt.len()) < MIN_CONTEXT_ROOM {
            break;
        }
        let source_ref = format!("chatgpt:{}", hex::encode(conversation.context_digest));
        let header = format!("\nSOURCE {source_ref}\nTITLE {}\n", conversation.title);
        if prompt.len() + header.len() + 32 > PROMPT_LIMIT {
            break;
        }
        prompt.push_str(&header);
        refs.push(source_ref);
        for message in &conversation.messages {
            let prefix = format!("{}: ", message.role);
            let remaining = PROMPT_LIMIT.saturating_sub(prompt.len());
            if remaining <= prefix.len() + 1 {
                break;
            }
            prompt.push_str(&prefix);
            let content_limit = remaining - prefix.len() - 1;
            let end = utf8_prefix_len(&message.text, content_limit);
            prompt.push_str(&message.text[..end]);
            prompt.push('\n');
            if end != message.text.len() {
                break;
            }
        }
    }
    if refs.is_empty() || prompt.len() > PROMPT_LIMIT {
        return Err(HostCallError::Internal);
    }
    Ok((prompt, refs))
}

fn utf8_prefix_len(value: &str, maximum: usize) -> usize {
    let mut end = maximum.min(value.len());
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    end
}

fn success<T: serde::Serialize>(id: u64, value: T) -> RpcResponse {
    match serde_json::to_value(value) {
        Ok(value) => RpcResponse::success(id, value),
        Err(_) => failure(Some(id), -32_000, "Local serialization failed"),
    }
}

fn failure(id: Option<u64>, code: i64, message: &str) -> RpcResponse {
    RpcResponse::failure(
        id,
        RpcError {
            code,
            message: message.to_owned(),
            data: None,
        },
    )
}

fn invalid_request(id: Option<u64>) -> RpcResponse {
    failure(id, -32_600, "Invalid JSON-RPC request")
}

fn invalid_params(id: u64) -> RpcResponse {
    failure(Some(id), -32_602, "Invalid method parameters")
}

fn not_ready(id: u64) -> RpcResponse {
    failure(Some(id), -32_001, "OpenOpen route is not ready")
}

fn host_failure(id: u64, error: &StoreError) -> RpcResponse {
    match error {
        StoreError::RuntimeDisabled => failure(Some(id), -32_010, "OpenOpen is off"),
        _ => failure(Some(id), -32_000, "Local Store verification failed"),
    }
}

fn call_failure(id: u64, error: &HostCallError) -> RpcResponse {
    match error {
        HostCallError::Store(StoreError::RuntimeDisabled) => {
            failure(Some(id), -32_010, "OpenOpen is off")
        }
        HostCallError::Codex(CodexError::Cancelled) => {
            failure(Some(id), -32_013, "Codex operation was cancelled")
        }
        HostCallError::Codex(CodexError::RequiredModelUnavailable) => failure(
            Some(id),
            -32_014,
            "The selected model or effort is unavailable. Review the current model setup.",
        ),
        HostCallError::Codex(CodexError::UnsupportedAccount) => {
            failure(Some(id), -32_015, "Managed ChatGPT account is required")
        }
        HostCallError::Codex(CodexError::CredentialFilePresent) => failure(
            Some(id),
            -32_016,
            "Keyring-only Codex state rejected an auth file",
        ),
        HostCallError::ChannelUnavailable => {
            failure(Some(id), -32_020, "Channel listener unavailable")
        }
        HostCallError::ChannelIntegrity => {
            failure(Some(id), -32_021, "Channel boundary verification failed")
        }
        #[cfg(test)]
        HostCallError::MissionAlreadyInProgress => failure(
            Some(id),
            -32_022,
            "Finish the current Mission before confirming another",
        ),
        HostCallError::MarkdownReconciliationRequired => failure(
            Some(id),
            -32_023,
            "Local Markdown needs reconciliation. No file was overwritten.",
        ),
        HostCallError::ReminderScheduleRequired => failure(
            Some(id),
            -32_024,
            "Choose a complete future Reminder schedule before review.",
        ),
        HostCallError::Store(StoreError::ChoiceClockUncertain)
        | HostCallError::ChoiceClockUncertain => failure(
            Some(id),
            -32_025,
            "Local clock continuity is uncertain. Refresh before choosing or confirming.",
        ),
        HostCallError::ChoiceRefreshRequired => failure(
            Some(id),
            -32_026,
            "The Choice session advanced. Refresh before choosing or confirming.",
        ),
        _ => failure(Some(id), -32_000, "Local operation failed closed"),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AccountState, BOOTSTRAP_MAGIC, ChannelConnectionStatus, ChannelSendResult, ChatGptLogin,
        CodexClient, CodexError, DEFAULT_REMINDERS_LIST_ID, GptModel, Host, HostCallError,
        HostPaths, ModelCatalogSnapshot, ModelSelectionStatus, OperationState, PollChannel,
        ReminderTarget, RpcRequest, SendChannelMessage, TransportEvent, TransportInbound,
        boot_scoped_monotonic_ms, channel_outcome_request, decode_params,
        initial_choice_result_from_generation, is_lower_sha256, mission_command_batch,
        model_selection_status, new_choice_begin_state, new_choice_begin_state_for_source,
        pin_b2_memory_source, read_bootstrap, render_choice_imessage_reply,
    };
    use ed25519_dalek::{Signer, SigningKey};
    use openopen_codex_client::{StructuredChoiceGeneration, StructuredChoiceOption};
    use openopen_core::{
        ActionGate, ActionProposal, ActionTarget, BrokerEnrollmentRecord, ChoiceIdleClockEvidence,
        CreateMission, CreateWorkItem, EffectKind, GateDecision, MissionCommand,
        NewBoundaryApproval, StoreError, TrustedBrokerEnrollment, broker_enrollment_signing_bytes,
    };
    use openopen_discord_adapter::{InboundEnvelope as DiscordInbound, RecoveryBatch};
    use openopen_protocol::{
        ApprovalKind, ApprovalStatus, ApprovalTarget, ChannelCursor, ChannelEnvelope,
        ChannelFailureIncident, ChannelInboundMessageClass, ChannelKind, ChannelMessageKind,
        ChannelModelDisposition, ChannelModelStart, ChannelObservation, ChannelOutboundDisposition,
        ChannelPairing, ChannelRouteApproval, ChannelRouteApprovalDecision, ChoiceBeginRequest,
        ChoiceIMessageReplyDisposition, ChoiceIMessageReplyIntent, ChoiceIMessageReplyPreview,
        ChoiceRefinementOperation, ChoiceReminderScheduleInput, ChoiceSessionState,
        CoreInstanceLease, EFFECT_PROTOCOL_VERSION, EvidenceKind, IMessagePairingMetadata,
        MissionStatus, ModelSelection, OptionSelection, OutcomeSuggestion, Receipt, RpcResponse,
        RuntimeControlAuthorization, RuntimeControlReceipt, Selection, WorkItemStatus,
        canonical_choice_set_digest, core_instance_lease_signing_bytes,
        runtime_control_authorization_hash, runtime_control_receipt_signing_bytes,
    };
    use rusqlite::Connection;
    use serde_json::{Value, json};
    use sha2::{Digest, Sha256};
    use std::collections::HashSet;
    use std::io::Cursor;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, mpsc};
    use std::time::Duration;

    fn fixture() -> (tempfile::TempDir, Host) {
        let root = tempfile::tempdir().unwrap();
        let root_path = std::fs::canonicalize(root.path()).unwrap();
        let support = root_path.join("support");
        std::fs::create_dir(&support).unwrap();
        std::fs::set_permissions(&support, std::fs::Permissions::from_mode(0o700)).unwrap();
        let mut host = Host::open(
            HostPaths {
                store: support.join("store.sqlite3"),
                codex_runtime: root_path.join("missing-codex"),
                codex_home: support.join("codex-home"),
                synthetic_home: support.join("synthetic-home"),
                model_input_root: support.join("model-input"),
                imsg_runtime: root_path.join("missing-imsg"),
                deep_zip_runtime: root_path.join("missing-deep-zip-worker"),
                user_home: root_path.clone(),
                persona_root: support.join("persona"),
                persona_team_identifier: "A1B2C3D4E5".to_owned(),
            },
            [7_u8; 32],
        )
        .unwrap();
        // Historical recovery fixtures exercise direct Store state. This
        // explicit test-only switch is not constructible through production
        // RPC or `Host::open`.
        host.allow_pr1_deferred_channel_routes_for_tests = true;
        (root, host)
    }

    fn persona_revision() -> openopen_protocol::PersonaRevisionRef {
        openopen_persona::PersonaBundle::embedded_default(1)
            .unwrap()
            .revision_ref
    }

    #[test]
    fn b2_source_pin_accepts_regular_file_and_rejects_symlink() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("synthetic.zip");
        std::fs::write(&source, b"synthetic archive bytes").unwrap();
        let record = pin_b2_memory_source(&source, "b2-source-request", 1).unwrap();
        assert_eq!(record.source.byte_length, 23);
        assert!(record.device_id > 0);
        assert!(record.inode > 0);
        assert_eq!(record.selected_path, source);

        let link = root.path().join("linked.zip");
        std::os::unix::fs::symlink(&record.selected_path, &link).unwrap();
        assert!(pin_b2_memory_source(&link, "b2-linked-request", 2).is_err());
    }

    #[test]
    fn production_host_rejects_discord_and_outbound_routes_deferred_after_pr1() {
        let (_root, mut host) = fixture();
        host.allow_pr1_deferred_channel_routes_for_tests = false;
        for (id, method) in [
            (2, "channel.route.bind"),
            (3, "channel.discord.setup.start"),
            (4, "channel.discord.setup.poll"),
            (5, "channel.discord.setup.confirm"),
            (6, "channel.discord.start"),
            (12, "channel.outbound.send"),
        ] {
            let response = request(
                &mut host,
                &json!({"jsonrpc": "2.0", "id": id, "method": method, "params": {}}).to_string(),
            );
            assert_eq!(
                response.error.expect("deferred route must fail").code,
                -32_001
            );
        }
    }

    #[test]
    fn host_open_leaves_codex_runtime_home_broker_owned() {
        let (_root, mut host) = fixture();
        assert!(!host.paths.codex_home.exists());
        assert!(!host.paths.synthetic_home.exists());
        assert!(host.paths.model_input_root.is_dir());

        let dashboard = request(
            &mut host,
            r#"{"jsonrpc":"2.0","id":1,"method":"mission.dashboard.read","params":{}}"#,
        );
        assert!(dashboard.error.is_none());
    }

    #[test]
    fn choice_loop_history_read_is_available_while_runtime_is_off_and_starts_no_work() {
        let (_root, mut host) = fixture();
        let response = request(
            &mut host,
            r#"{"jsonrpc":"2.0","id":2,"method":"choice.loop.read","params":{}}"#,
        );
        assert_eq!(response.result, Some(Value::Null));
        assert!(host.operations.gate.lock().unwrap().active.is_none());
        assert!(host.operations.codex.lock().unwrap().is_none());
    }

    #[test]
    fn choice_resume_rejects_every_caller_supplied_context_field() {
        let (_root, mut host) = fixture();
        let response = request(
            &mut host,
            r#"{"jsonrpc":"2.0","id":3,"method":"choice.resume","params":{"text":"retry","sessionId":"caller-state"}}"#,
        );
        assert_eq!(response.error.expect("params rejected").code, -32_602);
        assert!(host.operations.gate.lock().unwrap().active.is_none());
        assert!(host.operations.codex.lock().unwrap().is_none());
    }

    #[test]
    #[allow(clippy::too_many_lines)] // End-to-end restart/re-entry owns the complete fenced setup.
    fn read_preserves_a_restartable_owner_resume_for_authenticated_reentry() {
        let (_root, mut host) = fixture();
        let broker = broker_record(&host);
        host.store.install_trusted_broker(&broker).unwrap();
        let on = host.store.prepare_runtime_control(true, 1).unwrap();
        host.store
            .commit_runtime_control(&on, &broker_receipt(&on, None))
            .unwrap();
        host.operations.accept_committed_runtime(true, on.revision);
        install_test_core_lease(&mut host);
        let selection = ModelSelection {
            id: "model-selection-resume".to_owned(),
            model_id: "gpt-test-model".to_owned(),
            requested_effort: "high".to_owned(),
            actual_effort: "high".to_owned(),
            catalog_fingerprint: "a".repeat(64),
            catalog_revision: 7,
            account_display_class: "chatgpt:plus".to_owned(),
            protocol_schema_revision: 1,
        };
        host.store.select_model_selection(&selection, 2).unwrap();
        *host.operations.model_catalog_snapshot.lock().unwrap() = Some(
            test_model_catalog_snapshot(on.revision, super::now_ms().unwrap()),
        );
        let input = ChoiceBeginRequest {
            request_id: "choice-resume-restart".to_owned(),
            bounded_local_question: "Review the already local plan".to_owned(),
            expected_model_provenance_ref: selection.id.clone(),
            expected_catalog_fingerprint: selection.catalog_fingerprint.clone(),
            expected_catalog_revision: selection.catalog_revision,
            expected_protocol_revision: selection.protocol_schema_revision,
        };
        let (record, snapshot) =
            new_choice_begin_state(&input, &selection, 1, on.revision, &persona_revision(), 10)
                .unwrap();
        let initial_clock = ChoiceIdleClockEvidence {
            boot_id: "resume-restart-boot".to_owned(),
            wall_clock_ms: record.accepted_at_ms,
            monotonic_ms: record.accepted_at_ms,
        };
        host.store
            .begin_choice_session_with_clock(&record, &snapshot, &initial_clock)
            .unwrap();
        let generated = StructuredChoiceGeneration {
            understood_goal: "Review the already local plan".to_owned(),
            current_context: "The local intake is sealed.".to_owned(),
            assumptions: vec![],
            constraints: vec![],
            uncertainties: vec![],
            what_to_avoid: vec![],
            options: ["Review", "Narrow", "Prepare"]
                .map(|direction| StructuredChoiceOption {
                    direction: direction.to_owned(),
                    rationale: "Keep the work bounded.".to_owned(),
                    expected_result: "One clear next step.".to_owned(),
                    information_needed: vec![],
                    external_effects_preview: vec![],
                    source_categories: vec!["ownerInput".to_owned()],
                })
                .to_vec(),
            source_refs: vec![format!(
                "local:{}",
                &record.source_manifest.aggregate_digest[..24]
            )],
        };
        let active = host
            .store
            .commit_initial_choice_result(
                &initial_choice_result_from_generation(&record, generated).unwrap(),
            )
            .unwrap();
        let idle = host
            .store
            .advance_choice_idle_state_classified(
                &active.session.id,
                active.session.revision,
                on.revision,
                &ChoiceIdleClockEvidence {
                    boot_id: "resume-restart-boot".to_owned(),
                    wall_clock_ms: active.session.soft_idle_at_ms,
                    monotonic_ms: active.session.soft_idle_at_ms,
                },
            )
            .unwrap()
            .snapshot()
            .clone();
        assert_eq!(idle.session.state, ChoiceSessionState::SoftIdle);
        let pending = host
            .store
            .begin_choice_resume(on.revision, idle.session.soft_idle_at_ms + 1)
            .unwrap();
        assert!(
            pending
                .pending_refinement_operation
                .as_ref()
                .is_some_and(ChoiceRefinementOperation::is_owner_resume)
        );

        // This represents the replacement Host before authenticated Mac
        // re-entry. A non-authorizing continuity read may not block or replay
        // the exact durable owner-resume operation.
        let read = request(
            &mut host,
            r#"{"jsonrpc":"2.0","id":4,"method":"choice.loop.read","params":{}}"#,
        );
        assert_eq!(read.result.unwrap()["session"]["state"], "refining");
        assert_eq!(
            host.store.choice_loop_snapshot().unwrap(),
            Some(pending),
            "read must preserve the exact persisted resume operation"
        );

        let challenge = runtime_challenge(&mut host, 5);
        let recovered = request(
            &mut host,
            &json!({
                "jsonrpc": "2.0",
                "id": 6,
                "method": "choice.resume",
                "params": {
                    "authorization": on,
                    "brokerReceipt": broker_receipt(&on, Some(&challenge)),
                }
            })
            .to_string(),
        );
        assert_eq!(recovered.result.unwrap()["session"]["state"], "refining");
        assert!(host.operations.gate.lock().unwrap().active.is_some());
    }

    #[test]
    #[allow(clippy::too_many_lines)] // One end-to-end Host boundary test intentionally owns setup.
    fn choice_begin_is_host_derived_replay_safe_and_missing_client_blocks_without_effect() {
        let (_root, mut host) = fixture();
        let broker = broker_record(&host);
        host.store.install_trusted_broker(&broker).unwrap();
        let on = host.store.prepare_runtime_control(true, 1).unwrap();
        host.store
            .commit_runtime_control(&on, &broker_receipt(&on, None))
            .unwrap();
        host.operations.accept_committed_runtime(true, on.revision);
        install_test_core_lease(&mut host);
        let selection = ModelSelection {
            id: "model-selection-current".to_owned(),
            model_id: "gpt-test-model".to_owned(),
            requested_effort: "high".to_owned(),
            actual_effort: "high".to_owned(),
            catalog_fingerprint: "a".repeat(64),
            catalog_revision: 7,
            account_display_class: "chatgpt:plus".to_owned(),
            protocol_schema_revision: 1,
        };
        host.store.select_model_selection(&selection, 2).unwrap();
        *host.operations.model_catalog_snapshot.lock().unwrap() = Some(
            test_model_catalog_snapshot(on.revision, super::now_ms().unwrap()),
        );

        let begin = |host: &mut Host, request_id: &str, question: &str, rpc_id: u64| {
            let challenge = runtime_challenge(host, rpc_id);
            request(
                host,
                &json!({
                    "jsonrpc": "2.0",
                    "id": rpc_id + 1,
                    "method": "choice.begin",
                    "params": {
                        "requestId": request_id,
                        "boundedLocalQuestion": question,
                        "expectedModelProvenanceRef": selection.id,
                        "expectedCatalogFingerprint": selection.catalog_fingerprint,
                        "expectedCatalogRevision": selection.catalog_revision,
                        "expectedProtocolRevision": selection.protocol_schema_revision,
                        "authorization": on,
                        "brokerReceipt": broker_receipt(&on, Some(&challenge)),
                    }
                })
                .to_string(),
            )
        };
        let first = begin(&mut host, "choice-request-1", "Plan one bounded task", 800);
        let accepted = first.result.expect("accepted begin");
        assert_eq!(accepted["state"], "interpreting");
        for _ in 0..10_000 {
            if host.operations.gate.lock().unwrap().active.is_none() {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(host.operations.codex.lock().unwrap().is_none());
        assert!(host.operations.gate.lock().unwrap().active.is_none());
        // An exact retry is a durable read even when an unrelated worker slot
        // is occupied. It must not demand a second model turn or report the
        // active slot as a failure.
        let occupied = Arc::new(AtomicBool::new(false));
        host.operations.gate.lock().unwrap().active = Some(occupied.clone());
        let replay = begin(&mut host, "choice-request-1", "Plan one bounded task", 802);
        assert_eq!(replay.result, Some(accepted.clone()));
        assert!(host.operations.gate.lock().unwrap().active.is_some());
        host.operations.gate.lock().unwrap().active = None;
        let changed = begin(&mut host, "choice-request-1", "Changed question", 804);
        assert!(changed.error.is_some());
        let second = begin(&mut host, "choice-request-2", "Another question", 806);
        assert!(second.error.is_some());
        let blocked = host.store.choice_loop_snapshot().unwrap().unwrap();
        assert_eq!(blocked.session.state, ChoiceSessionState::Blocked);
        assert_eq!(blocked.active_batch, None);
        let prepared_off = request(
            &mut host,
            &json!({
                "jsonrpc": "2.0",
                "id": 809,
                "method": "mission.runtime.prepare",
                "params": {
                    "enabled": false,
                }
            })
            .to_string(),
        );
        assert!(prepared_off.error.is_none());
        assert_eq!(prepared_off.result.unwrap()["enabled"], false);
        assert_eq!(
            host.store
                .choice_loop_snapshot()
                .unwrap()
                .unwrap()
                .session
                .state,
            ChoiceSessionState::Cancelled
        );
    }

    #[test]
    fn initial_choice_result_derives_all_authority_from_the_accepted_record() {
        let selection = ModelSelection {
            id: "model-selection-current".to_owned(),
            model_id: "gpt-test-model".to_owned(),
            requested_effort: "high".to_owned(),
            actual_effort: "high".to_owned(),
            catalog_fingerprint: "a".repeat(64),
            catalog_revision: 7,
            account_display_class: "chatgpt:plus".to_owned(),
            protocol_schema_revision: 1,
        };
        let input = ChoiceBeginRequest {
            request_id: "choice-request-result".to_owned(),
            bounded_local_question: "Plan one bounded task".to_owned(),
            expected_model_provenance_ref: selection.id.clone(),
            expected_catalog_fingerprint: selection.catalog_fingerprint.clone(),
            expected_catalog_revision: selection.catalog_revision,
            expected_protocol_revision: selection.protocol_schema_revision,
        };
        let (record, _) =
            new_choice_begin_state(&input, &selection, 1, 9, &persona_revision(), 10).unwrap();
        let generated = StructuredChoiceGeneration {
            understood_goal: "Plan one bounded task".to_owned(),
            current_context: "The local intake is sealed.".to_owned(),
            assumptions: vec![],
            constraints: vec![],
            uncertainties: vec![],
            what_to_avoid: vec![],
            options: ["Review", "Narrow", "Prepare"]
                .map(|direction| StructuredChoiceOption {
                    direction: direction.to_owned(),
                    rationale: "Keep the work bounded.".to_owned(),
                    expected_result: "One clear next step.".to_owned(),
                    information_needed: vec![],
                    external_effects_preview: vec![],
                    source_categories: vec!["ownerInput".to_owned()],
                })
                .to_vec(),
            source_refs: vec![format!(
                "local:{}",
                &record.source_manifest.aggregate_digest[..24]
            )],
        };
        let result = initial_choice_result_from_generation(&record, generated).unwrap();
        assert_eq!(result.operation_id, record.accepted.operation_id);
        assert_eq!(result.expected_generation, record.runtime_revision);
        assert_eq!(
            result.expected_session_revision,
            record.accepted.accepted_session_revision
        );
        assert_eq!(
            result.choice_set.choice_session_id,
            record.accepted.choice_session_id
        );
        assert_eq!(result.choice_set.options.len(), 3);
        assert!(result.is_valid());
    }

    fn fake_imsg_runtime(root: &std::path::Path) -> PathBuf {
        let executable = std::fs::canonicalize(root).unwrap().join("imsg-route-fake");
        std::fs::write(
            &executable,
            r"#!/usr/bin/env python3
import json
import sys

if len(sys.argv) != 2 or sys.argv[1] != 'rpc':
    sys.exit(7)
for line in sys.stdin:
    request = json.loads(line)
    if request['method'] == 'chats.list':
        result = {'chats':[{'id':42,'identifier':'owner','guid':'iMessage;+;owner','name':'Owner','service':'iMessage','last_message_at':'2026-07-15T00:00:00Z','participants':['+15550000001'],'is_group':False}]}
        sys.stdout.write(json.dumps({'jsonrpc':'2.0','id':request['id'],'result':result}, separators=(',', ':')) + '\n')
        sys.stdout.flush()
    elif request['method'] == 'messages.history':
        result = {'messages':[{'id':101,'chat_id':42,'chat_identifier':'owner','chat_guid':'iMessage;+;owner','chat_name':'Owner','participants':['+15550000001'],'is_group':False,'guid':'self-proof-guid','sender':'','is_from_me':True,'text':'Self-chat proof','created_at':'2026-07-15T00:00:00Z','destination_caller_id':'+15550000001'}]}
        sys.stdout.write(json.dumps({'jsonrpc':'2.0','id':request['id'],'result':result}, separators=(',', ':')) + '\n')
        sys.stdout.flush()
",
        )
        .unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
        executable
    }

    /// Drives the production JSON-RPC dispatcher exactly as a product caller
    /// would. Tests for retired routes must use this rather than the legacy
    /// state-fixture helper below.
    fn dispatch_public(host: &mut Host, line: &str) -> RpcResponse {
        let (send, receive) = mpsc::sync_channel(32);
        host.handle_line(line, &send);
        receive.recv().unwrap()
    }

    /// Historical Mission fixtures exercise only the still-readable legacy
    /// Store transition logic. They must not re-open a production JSON-RPC
    /// creation route after Choice became the sole foreground authority.
    fn request(host: &mut Host, line: &str) -> RpcResponse {
        let request: RpcRequest = serde_json::from_str(line).expect("test RPC request");
        let (send, receive) = mpsc::sync_channel(32);
        match request.method.as_str() {
            "mission.confirm" => host.confirm_mission(&request, &send),
            "mission.cancel" => host.cancel_mission(&request, &send),
            "mission.reminders.begin" => host.begin_reminder_dispatch(&request, &send),
            "mission.reminders.record" => host.record_reminder_mirror(&request, &send),
            "mission.reminders.complete" => host.complete_reminders(&request, &send),
            _ => host.handle_line(line, &send),
        }
        receive.recv().expect("test RPC response")
    }

    fn dashboard(host: &mut Host, request_id: u64) -> Value {
        request(
            host,
            &json!({
                "jsonrpc": "2.0",
                "id": request_id,
                "method": "mission.dashboard.read",
                "params": {}
            })
            .to_string(),
        )
        .result
        .unwrap()
    }

    fn runtime_challenge(host: &mut Host, request_id: u64) -> String {
        request(
            host,
            &json!({
                "jsonrpc": "2.0",
                "id": request_id,
                "method": "mission.runtime.challenge",
                "params": {}
            })
            .to_string(),
        )
        .result
        .unwrap()["challenge"]
            .as_str()
            .unwrap()
            .to_owned()
    }

    fn channel_outbound_count(host: &Host) -> i64 {
        Connection::open(&host.paths.store)
            .unwrap()
            .query_row("SELECT COUNT(*) FROM channel_outbound", [], |row| {
                row.get(0)
            })
            .unwrap()
    }

    fn hero_suggestion() -> OutcomeSuggestion {
        hero_suggestion_with_id("suggestion-1700000000000-0123456789abcdef0123456789abcdef")
    }

    fn hero_suggestion_with_id(id: &str) -> OutcomeSuggestion {
        OutcomeSuggestion {
            id: id.to_owned(),
            title: "Prepare the client follow-up".to_owned(),
            why_now: "Close the loop while the meeting is fresh.".to_owned(),
            proposed_steps: vec![
                "Draft the follow-up summary".to_owned(),
                "Send the agreed next steps".to_owned(),
            ],
            source_refs: Vec::new(),
        }
    }

    fn seed_model_selection(host: &mut Host) {
        host.store
            .select_model_selection(
                &ModelSelection {
                    id: "model-selection-test".to_owned(),
                    model_id: "gpt-test-model".to_owned(),
                    requested_effort: "not_applicable".to_owned(),
                    actual_effort: "not_applicable".to_owned(),
                    catalog_fingerprint: "a".repeat(64),
                    catalog_revision: 1,
                    account_display_class: "chatgpt:test".to_owned(),
                    protocol_schema_revision: 1,
                },
                1,
            )
            .unwrap();
    }

    fn test_model_catalog_snapshot(
        runtime_revision: u64,
        issued_at_ms: i64,
    ) -> ModelCatalogSnapshot {
        ModelCatalogSnapshot {
            id: "b".repeat(64),
            account: AccountState::ChatGpt {
                email: "owner@example.com".to_owned(),
                plan_type: "plus".to_owned(),
            },
            models: vec![GptModel {
                id: "gpt-test-model".to_owned(),
                display_name: "Test model".to_owned(),
                supported_reasoning_efforts: vec!["high".to_owned()],
            }],
            catalog_fingerprint: "a".repeat(64),
            catalog_revision: 7,
            runtime_revision,
            issued_at_ms,
        }
    }

    #[test]
    fn current_model_setup_rejects_same_id_catalog_and_account_drift() {
        let account = AccountState::ChatGpt {
            email: "owner@example.com".to_owned(),
            plan_type: "plus".to_owned(),
        };
        let models = vec![GptModel {
            id: "gpt-test-model".to_owned(),
            display_name: "Test model".to_owned(),
            supported_reasoning_efforts: vec!["high".to_owned()],
        }];
        let fingerprint = CodexClient::model_catalog_fingerprint(&models).unwrap();
        let revision = CodexClient::model_catalog_revision(&fingerprint).unwrap();
        let selection = ModelSelection {
            id: "model-selection-current".to_owned(),
            model_id: "gpt-test-model".to_owned(),
            requested_effort: "high".to_owned(),
            actual_effort: "high".to_owned(),
            catalog_fingerprint: fingerprint.clone(),
            catalog_revision: revision,
            account_display_class: "chatgpt:plus".to_owned(),
            protocol_schema_revision: 1,
        };
        assert!(matches!(
            model_selection_status(&account, &models, Some(&selection), &fingerprint, revision),
            ModelSelectionStatus::Current
        ));

        let same_id_drifted_catalog = vec![GptModel {
            id: "gpt-test-model".to_owned(),
            display_name: "Test model changed".to_owned(),
            supported_reasoning_efforts: vec!["medium".to_owned()],
        }];
        let drifted_fingerprint =
            CodexClient::model_catalog_fingerprint(&same_id_drifted_catalog).unwrap();
        let drifted_revision = CodexClient::model_catalog_revision(&drifted_fingerprint).unwrap();
        assert!(matches!(
            model_selection_status(
                &account,
                &same_id_drifted_catalog,
                Some(&selection),
                &drifted_fingerprint,
                drifted_revision
            ),
            ModelSelectionStatus::Unavailable
        ));

        let changed_account = AccountState::ChatGpt {
            email: "owner@example.com".to_owned(),
            plan_type: "free".to_owned(),
        };
        assert!(matches!(
            model_selection_status(
                &changed_account,
                &models,
                Some(&selection),
                &fingerprint,
                revision
            ),
            ModelSelectionStatus::Unavailable
        ));
    }

    #[test]
    fn model_catalog_snapshot_and_digest_contract_reject_non_hex_or_expired_selection() {
        assert!(is_lower_sha256(&"a1".repeat(32)));
        assert!(!is_lower_sha256(&format!("{}g", "a".repeat(63))));

        let operations = OperationState::default();
        operations.accept_recovered_runtime(true, 61);
        let setup = operations.begin_operation().unwrap();
        let snapshot = test_model_catalog_snapshot(61, 1_000);
        assert!(operations.publish_model_catalog_snapshot(&setup, snapshot.clone()));
        operations.finish_operation(&setup);

        let selection = operations.begin_operation().unwrap();
        let wrote_selection = AtomicBool::new(false);
        let result = operations.reconcile_active_model_catalog(
            &selection,
            &super::ModelCatalogRequest {
                snapshot_id: &snapshot.id,
                catalog_fingerprint: &snapshot.catalog_fingerprint,
                catalog_revision: snapshot.catalog_revision,
                runtime_revision: 61,
                now: 1_000 + super::MODEL_CATALOG_SNAPSHOT_TTL_MS + 1,
            },
            |_| {
                wrote_selection.store(true, Ordering::Release);
                Ok(())
            },
        );
        assert!(matches!(
            result,
            Err(HostCallError::Codex(CodexError::RequiredModelUnavailable))
        ));
        assert!(!wrote_selection.load(Ordering::Acquire));
    }

    #[test]
    fn product_owned_sources_do_not_reintroduce_retired_fixed_model_routes() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let sources = [
            "crates/openopen-protocol/src/lib.rs",
            "crates/openopen-codex-client/src/contracts.rs",
            "crates/openopen-codex-client/src/lib.rs",
            "crates/openopen-host/src/lib.rs",
            "macos/EffectBrokerBridge/Sources/OpenOpenAppSupport/AppModel.swift",
            "macos/EffectBrokerBridge/Sources/OpenOpenAppSupport/CoreContracts.swift",
            "macos/EffectBrokerBridge/Sources/OpenOpenAppSupport/CoreProcessClient.swift",
            "macos/EffectBrokerBridge/Sources/OpenOpenAppSupport/OpenOpenViews.swift",
        ]
        .map(|path| std::fs::read_to_string(root.join(path)).expect("read product source"));
        let fixed_model = ["g", "p", "t", "-", "5", ".", "6", "-", "s", "o", "l"].concat();
        let fixed_reasoning = ["sol with", " high reasoning"].concat();
        let auto_route = ["model routing: ", "auto"].concat();
        let retired_host_dispatch = ["\"outcome.propose\"", " =>"].concat();
        let retired_client_dispatch = ["method: \"outcome.", "propose\""].concat();
        let retired_core_entrypoint = ["func ", "propose("].concat();
        for source in sources {
            let lower = source.to_ascii_lowercase();
            assert!(!lower.contains(&fixed_model));
            assert!(!lower.contains(&fixed_reasoning));
            assert!(!lower.contains(&auto_route));
            assert!(
                !source.contains(&retired_host_dispatch),
                "a retired Outcome proposal must not remain in public Host dispatch"
            );
            assert!(
                !source.contains(&retired_client_dispatch),
                "a product client must not expose a retired Outcome proposal RPC"
            );
            assert!(
                !source.contains(&retired_core_entrypoint),
                "a product Core surface must not expose a retired Outcome proposal entrypoint"
            );
        }
    }

    #[test]
    fn markdown_reconciliation_is_a_typed_non_overwrite_failure() {
        let response = super::call_failure(73, &HostCallError::MarkdownReconciliationRequired);
        let error = response.error.expect("typed RPC failure");
        assert_eq!(error.code, -32_023);
        assert_eq!(
            error.message,
            "Local Markdown needs reconciliation. No file was overwritten."
        );
        assert!(response.result.is_none());
    }

    #[test]
    fn reminder_schedule_recovery_is_typed_and_actionable() {
        let response = super::call_failure(74, &HostCallError::ReminderScheduleRequired);
        let error = response.error.expect("typed RPC failure");
        assert_eq!(error.code, -32_024);
        assert_eq!(
            error.message,
            "Choose a complete future Reminder schedule before review."
        );
        assert!(response.result.is_none());
    }

    #[test]
    fn choice_clock_and_refresh_failures_keep_distinct_typed_codes() {
        for error in [
            HostCallError::ChoiceClockUncertain,
            HostCallError::Store(StoreError::ChoiceClockUncertain),
        ] {
            let response = super::call_failure(75, &error);
            assert_eq!(response.error.unwrap().code, -32_025);
        }
        let response = super::call_failure(76, &HostCallError::ChoiceRefreshRequired);
        assert_eq!(response.error.unwrap().code, -32_026);
    }

    #[test]
    fn idle_evidence_uses_stable_boot_and_kernel_monotonic_sources() {
        let boot = super::stable_idle_boot_id().expect("read OS boot identity");
        assert_eq!(
            super::stable_idle_boot_id().expect("re-read OS boot identity"),
            boot
        );
        let first = super::boot_scoped_monotonic_ms().expect("read kernel monotonic clock");
        std::thread::sleep(Duration::from_millis(2));
        let second = super::boot_scoped_monotonic_ms().expect("re-read kernel monotonic clock");
        assert!(second >= first);
    }

    #[test]
    fn boot_identity_uses_only_strict_numeric_kernel_fields() {
        let utc = "{ sec = 1784500000, usec = 123456 } Sun Jul 19 00:00:00 2026\n";
        let pacific = "{ sec = 1784500000, usec = 123456 } Sat Jul 18 17:00:00 2026 PDT\n";
        assert_eq!(
            super::boot_identity_from_sysctl(utc).unwrap(),
            super::boot_identity_from_sysctl(pacific).unwrap(),
            "human-formatted timezone suffixes are not boot identity"
        );
        assert_ne!(
            super::boot_identity_from_sysctl(utc).unwrap(),
            super::boot_identity_from_sysctl(
                "{ sec = 1784500001, usec = 123456 } Sun Jul 19 00:00:01 2026\n"
            )
            .unwrap()
        );
        for malformed in [
            "sec = 1, usec = 2",
            "{ sec = 1, sec = 1, usec = 2 } suffix",
            "{ sec = -1, usec = 2 } suffix",
            "{ sec = 1, usec = 1000000 } suffix",
            "{ sec = 1, other = 2 } suffix",
        ] {
            assert!(
                super::boot_identity_from_sysctl(malformed).is_err(),
                "{malformed}"
            );
        }
    }

    fn confirm_hero_mission(host: &mut Host) -> Value {
        seed_model_selection(host);
        *host.operations.suggestion.lock().unwrap() = Some(hero_suggestion());
        request(
            host,
            r#"{"jsonrpc":"2.0","id":300,"method":"mission.confirm","params":{"suggestionId":"suggestion-1700000000000-0123456789abcdef0123456789abcdef","reminderTarget":{"sourceIdentifier":"source-1","calendarIdentifier":"calendar-1"}}}"#,
        )
        .result
        .unwrap()
    }

    #[test]
    fn a_need_you_mission_blocks_confirmation_of_a_second_mission() {
        let (_root, mut host) = fixture();
        let first = confirm_hero_mission(&mut host);
        let first_mission_id = first["missionId"].as_str().unwrap().to_owned();
        let anchor = host.store.current_verified_audit_anchor().unwrap().unwrap();
        let commands = mission_command_batch(
            Some(&anchor),
            &first_mission_id,
            vec![MissionCommand::RequestScopeChange {
                mission_id: first_mission_id.clone(),
                approval: NewBoundaryApproval {
                    id: "approval-owner-boundary".into(),
                    kind: ApprovalKind::ExpandedScope,
                    prompt: "Finish the exact owner boundary first.".into(),
                    scope_digest: "owner-boundary-scope".into(),
                    target: None,
                },
                needs_me_id: "needs-owner-boundary".into(),
                now_ms: 2_000_000_000_000,
            }],
        )
        .unwrap();
        host.store.execute_mission_command_batch(&commands).unwrap();
        let second_id = "suggestion-1700000000001-fedcba9876543210fedcba9876543210";
        *host
            .operations
            .suggestion
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) =
            Some(hero_suggestion_with_id(second_id));

        let response = request(
            &mut host,
            &json!({
                "jsonrpc": "2.0",
                "id": 399,
                "method": "mission.confirm",
                "params": {
                    "suggestionId": second_id,
                    "reminderTarget": {
                        "sourceIdentifier": "source-1",
                        "calendarIdentifier": "calendar-1"
                    }
                }
            })
            .to_string(),
        );
        assert_eq!(response.error.unwrap().code, -32_022);
        let anchor = host.store.current_verified_audit_anchor().unwrap().unwrap();
        let missions = host.store.list_missions(&anchor).unwrap();
        assert_eq!(missions.len(), 1);
        assert_eq!(missions[0].id, first_mission_id);
        assert_eq!(missions[0].status, MissionStatus::NeedsMe);
        assert!(
            host.operations
                .suggestion
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .as_ref()
                .is_some_and(|value| value.id == second_id),
            "a rejected second confirmation must not consume or mutate its proposal"
        );
    }

    fn record_hero_mirror(host: &mut Host, confirmed: &Value, prefix: &str) -> Value {
        let mission_id = confirmed["missionId"].as_str().unwrap();
        let started = request(
            host,
            &json!({
                "jsonrpc": "2.0",
                "id": 398,
                "method": "mission.reminders.begin",
                "params": {"missionId": mission_id}
            })
            .to_string(),
        )
        .result
        .unwrap();
        let mission = &started["mission"];
        let links = mission["workItems"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
            .map(|(index, item)| {
                json!({
                    "missionId": mission_id,
                    "workItemId": item["id"],
                    "sourceIdentifier": "source-1",
                    "calendarIdentifier": "calendar-1",
                    "calendarItemIdentifier": format!("{prefix}-{index}"),
                    "dispatchToken": mission["reminderDispatch"][index]["token"],
                    "title": item["title"],
                })
            })
            .collect::<Vec<_>>();
        std::thread::sleep(std::time::Duration::from_millis(2));
        request(
            host,
            &json!({
                "jsonrpc": "2.0",
                "id": 399,
                "method": "mission.reminders.record",
                "params": {"missionId": mission_id, "links": links}
            })
            .to_string(),
        )
        .result
        .unwrap()
    }

    #[test]
    fn binary_bootstrap_never_requires_a_text_secret() {
        let mut frame = BOOTSTRAP_MAGIC.to_vec();
        frame.extend([9_u8; 32]);
        frame.push(b'\n');
        assert_eq!(read_bootstrap(&mut Cursor::new(frame)).unwrap(), [9_u8; 32]);
    }

    #[test]
    fn cancelled_operation_identity_cannot_be_reused_or_overtaken() {
        let operations = OperationState::default();
        assert!(operations.begin_operation().is_none());
        operations.accept_recovered_runtime(true, 1);
        let first = operations.begin_operation().unwrap();
        operations.cancel_active();
        operations.latch_prepared_off(2);
        assert!(first.load(Ordering::Acquire));
        assert!(operations.codex_cancel.load(Ordering::Acquire));
        assert!(operations.begin_operation().is_none());

        operations.finish_operation(&first);
        assert!(operations.begin_operation().is_none());
        operations.accept_recovered_runtime(true, 1);
        assert!(operations.begin_operation().is_none());
        operations.accept_committed_runtime(true, 1);
        assert!(operations.begin_operation().is_none());
        operations.accept_committed_runtime(true, 2);
        let second = operations.begin_operation().unwrap();
        assert!(!Arc::ptr_eq(&first, &second));
        assert!(!second.load(Ordering::Acquire));
        assert!(!operations.codex_cancel.load(Ordering::Acquire));

        operations.finish_operation(&first);
        assert!(
            operations
                .gate
                .lock()
                .unwrap()
                .active
                .as_ref()
                .is_some_and(|active| Arc::ptr_eq(active, &second))
        );
        operations.finish_operation(&second);
        assert!(operations.gate.lock().unwrap().active.is_none());
    }

    #[test]
    fn global_off_revokes_channel_model_reconciliation_before_any_publish() {
        let operations = OperationState::default();
        operations.accept_recovered_runtime(true, 1);
        let active = operations.begin_operation().unwrap();
        assert_eq!(
            operations.reconcile_active(&active, || Ok(7_u8)).unwrap(),
            7
        );

        operations.cancel_active();
        let published = AtomicBool::new(false);
        let result = operations.reconcile_active(&active, || {
            published.store(true, Ordering::Release);
            Ok(8_u8)
        });
        assert!(matches!(
            result,
            Err(HostCallError::Codex(CodexError::Cancelled))
        ));
        assert!(!published.load(Ordering::Acquire));
    }

    #[test]
    fn global_off_cancels_stalled_model_setup_without_waiting_for_a_late_catalog() {
        let operations = OperationState::default();
        operations.accept_recovered_runtime(true, 41);
        let token = operations.begin_operation().unwrap();
        let (started_send, started_receive) = mpsc::sync_channel(1);
        let (release_send, release_receive) = mpsc::sync_channel(1);
        let worker_operations = operations.clone();
        let worker_token = token.clone();
        let worker = std::thread::spawn(move || {
            started_send.send(()).unwrap();
            release_receive.recv().unwrap();
            worker_operations.publish_model_catalog_snapshot(
                &worker_token,
                test_model_catalog_snapshot(41, 1_000),
            )
        });

        // The simulated provider worker has not returned. Protected Off only
        // touches the operation gate, so it never waits on that worker.
        started_receive.recv().unwrap();
        operations.cancel_active();
        assert!(token.load(Ordering::Acquire));
        release_send.send(()).unwrap();
        assert!(!worker.join().unwrap());
        assert!(operations.model_catalog_snapshot.lock().unwrap().is_none());
    }

    #[test]
    fn model_selection_rejects_stale_cancelled_and_restarted_catalog_snapshots() {
        let operations = OperationState::default();
        operations.accept_recovered_runtime(true, 51);
        let setup = operations.begin_operation().unwrap();
        let snapshot = test_model_catalog_snapshot(51, 10_000);
        assert!(operations.publish_model_catalog_snapshot(&setup, snapshot.clone()));
        operations.finish_operation(&setup);

        let stale_selection = operations.begin_operation().unwrap();
        let wrote_selection = AtomicBool::new(false);
        let stale = operations.reconcile_active_model_catalog(
            &stale_selection,
            &super::ModelCatalogRequest {
                snapshot_id: &snapshot.id,
                catalog_fingerprint: &snapshot.catalog_fingerprint,
                catalog_revision: snapshot.catalog_revision + 1,
                runtime_revision: 51,
                now: 10_001,
            },
            |_| {
                wrote_selection.store(true, Ordering::Release);
                Ok(())
            },
        );
        assert!(matches!(
            stale,
            Err(HostCallError::Codex(CodexError::RequiredModelUnavailable))
        ));
        assert!(!wrote_selection.load(Ordering::Acquire));
        operations.cancel_active();
        assert!(matches!(
            operations.reconcile_active_model_catalog(
                &stale_selection,
                &super::ModelCatalogRequest {
                    snapshot_id: &snapshot.id,
                    catalog_fingerprint: &snapshot.catalog_fingerprint,
                    catalog_revision: snapshot.catalog_revision,
                    runtime_revision: 51,
                    now: 10_001,
                },
                |_| Ok(())
            ),
            Err(HostCallError::Codex(CodexError::Cancelled))
        ));

        // A fresh Host process owns no volatile catalog snapshot, even if its
        // protected runtime happens to be at the same revision.
        let restarted = OperationState::default();
        restarted.accept_recovered_runtime(true, 51);
        let restart_selection = restarted.begin_operation().unwrap();
        assert!(matches!(
            restarted.reconcile_active_model_catalog(
                &restart_selection,
                &super::ModelCatalogRequest {
                    snapshot_id: &snapshot.id,
                    catalog_fingerprint: &snapshot.catalog_fingerprint,
                    catalog_revision: snapshot.catalog_revision,
                    runtime_revision: 51,
                    now: 10_001,
                },
                |_| Ok(())
            ),
            Err(HostCallError::Codex(CodexError::RequiredModelUnavailable))
        ));
    }

    #[test]
    fn pending_off_latch_rejects_old_on_replay_until_a_fresh_protected_revision() {
        let operations = OperationState::default();
        operations.accept_recovered_runtime(true, 7);
        let active = operations.begin_operation().unwrap();
        assert!(operations.install_login(
            &active,
            ChatGptLogin {
                auth_url: "https://example.invalid".to_owned(),
                login_id: "pending-off".to_owned(),
            }
        ));

        operations.cancel_active();
        operations.latch_prepared_off(8);
        assert!(active.load(Ordering::Acquire));
        assert!(operations.codex_cancel.load(Ordering::Acquire));
        assert!(operations.gate.lock().unwrap().active.is_none());

        operations.accept_committed_runtime(true, 7);
        assert!(operations.begin_operation().is_none());
        operations.accept_recovered_runtime(true, 7);
        assert!(operations.begin_operation().is_none());
        assert!(operations.codex_cancel.load(Ordering::Acquire));

        operations.accept_recovered_runtime(true, 8);
        let replacement = operations.begin_operation().unwrap();
        assert!(!replacement.load(Ordering::Acquire));
        assert!(!operations.codex_cancel.load(Ordering::Acquire));
    }

    #[test]
    fn login_installation_and_cancellation_share_one_ordered_boundary() {
        let operations = OperationState::default();
        operations.accept_recovered_runtime(true, 1);
        let cancelled = operations.begin_operation().unwrap();
        operations.cancel_active();
        operations.latch_prepared_off(1);
        assert!(!operations.install_login(
            &cancelled,
            ChatGptLogin {
                auth_url: "https://example.invalid".to_owned(),
                login_id: "cancelled".to_owned(),
            }
        ));
        assert!(operations.login.lock().unwrap().is_none());
        operations.finish_operation(&cancelled);

        operations.accept_committed_runtime(true, 1);
        let pending = operations.begin_operation().unwrap();
        assert!(operations.install_login(
            &pending,
            ChatGptLogin {
                auth_url: "https://example.invalid".to_owned(),
                login_id: "pending".to_owned(),
            }
        ));
        operations.cancel_active();
        assert!(pending.load(Ordering::Acquire));
        assert!(operations.login.lock().unwrap().is_none());
        assert!(operations.gate.lock().unwrap().active.is_none());
        assert!(operations.begin_operation().is_none());

        let racing = OperationState::default();
        racing.accept_recovered_runtime(true, 1);
        let racing_token = racing.begin_operation().unwrap();
        let login_guard = racing.login.lock().unwrap();
        let cancelling = racing.clone();
        let (started_send, started_receive) = mpsc::sync_channel(1);
        let cancel_thread = std::thread::spawn(move || {
            started_send.send(()).unwrap();
            cancelling.cancel_active();
        });
        started_receive
            .recv_timeout(Duration::from_secs(1))
            .unwrap();
        for _ in 0..100 {
            if racing.codex_cancel.load(Ordering::Acquire) {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(racing.codex_cancel.load(Ordering::Acquire));
        let installing = racing.clone();
        let installing_token = racing_token.clone();
        let install_thread = std::thread::spawn(move || {
            installing.install_login(
                &installing_token,
                ChatGptLogin {
                    auth_url: "https://example.invalid".to_owned(),
                    login_id: "racing".to_owned(),
                },
            )
        });
        drop(login_guard);
        cancel_thread.join().unwrap();
        assert!(!install_thread.join().unwrap());
        assert!(racing_token.load(Ordering::Acquire));
        assert!(racing.login.lock().unwrap().is_none());
        racing.finish_operation(&racing_token);
        assert!(racing.gate.lock().unwrap().active.is_none());

        let aborting = OperationState::default();
        aborting.accept_recovered_runtime(true, 1);
        let login_token = aborting.begin_operation().unwrap();
        assert!(aborting.install_login(
            &login_token,
            ChatGptLogin {
                auth_url: "https://example.invalid".to_owned(),
                login_id: "abort-before-browser".to_owned(),
            }
        ));
        assert!(aborting.authorize_codex_abort());
        assert!(login_token.load(Ordering::Acquire));
        assert!(aborting.login.lock().unwrap().is_none());
        assert!(aborting.gate.lock().unwrap().active.is_none());

        let model_operation = OperationState::default();
        model_operation.accept_recovered_runtime(true, 1);
        let model_token = model_operation.begin_operation().unwrap();
        assert!(!model_operation.authorize_codex_abort());
        assert!(!model_token.load(Ordering::Acquire));
        assert!(model_operation.gate.lock().unwrap().active.is_some());
    }

    #[test]
    fn protected_off_wins_before_an_irreversible_reply_start() {
        let operations = OperationState::default();
        operations.accept_recovered_runtime(true, 1);
        let token = operations.begin_operation().unwrap();
        operations.cancel_active();
        let started = AtomicBool::new(false);
        assert!(
            operations
                .start_irreversible(&token, || started.store(true, Ordering::Release))
                .is_none()
        );
        assert!(!started.load(Ordering::Acquire));
    }

    #[test]
    fn runtime_defaults_off_and_commit_requires_provisioned_broker_receipt() {
        let (_root, mut host) = fixture();
        let initial = request(
            &mut host,
            r#"{"jsonrpc":"2.0","id":1,"method":"mission.runtime.read","params":{}}"#,
        );
        assert_eq!(initial.result.unwrap()["enabled"], false);
        let changed = request(
            &mut host,
            r#"{"jsonrpc":"2.0","id":2,"method":"mission.runtime.prepare","params":{"enabled":true}}"#,
        );
        assert_eq!(changed.error.unwrap().code, -32_015);
        let reread = request(
            &mut host,
            r#"{"jsonrpc":"2.0","id":4,"method":"mission.runtime.read","params":{}}"#,
        );
        assert_eq!(reread.result.unwrap()["enabled"], false);
    }

    #[test]
    fn off_blocks_current_choice_intake_before_any_runtime_spawn() {
        let (_root, mut host) = fixture();
        let challenge = request(
            &mut host,
            r#"{"jsonrpc":"2.0","id":90,"method":"mission.runtime.challenge","params":{}}"#,
        )
        .result
        .unwrap()["challenge"]
            .as_str()
            .unwrap()
            .to_owned();
        let response = request(
            &mut host,
            &json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "choice.begin",
                "params": {
                    "requestId": "choice-off-test",
                    "boundedLocalQuestion": "Plan today",
                    "expectedModelProvenanceRef": "selection-off-test",
                    "expectedCatalogFingerprint": "a".repeat(64),
                    "expectedCatalogRevision": 1,
                    "expectedProtocolRevision": 1,
                    "authorization": {
                        "protocolVersion": 1,
                        "enabled": false,
                        "revision": 1,
                        "updatedAtMs": 1,
                        "coreKeyId": "00".repeat(32),
                        "authorizationSignatureHex": "00".repeat(64),
                    },
                    "brokerReceipt": {
                        "protocolVersion": 1,
                        "authorizationHash": "00".repeat(32),
                        "checkpointNonce": "00".repeat(32),
                        "requestNonce": challenge,
                        "brokerKeyId": "00".repeat(32),
                        "brokerSignatureHex": "00".repeat(64),
                    }
                }
            })
            .to_string(),
        );
        assert_eq!(response.error.unwrap().code, -32_015);
        assert!(
            std::fs::read_dir(&host.paths.model_input_root)
                .unwrap()
                .next()
                .is_none()
        );
    }

    #[test]
    fn dashboard_has_one_suggestion_slot_and_no_more_than_three_cards() {
        let (_root, mut host) = fixture();
        let response = request(
            &mut host,
            r#"{"jsonrpc":"2.0","id":1,"method":"mission.dashboard.read","params":{}}"#,
        );
        let dashboard = response.result.unwrap();
        assert!(dashboard["suggestion"].is_null());
        assert!(dashboard["activeCards"].as_array().unwrap().len() <= 3);
        assert_eq!(dashboard["microphone"]["available"], false);
    }

    #[test]
    fn mission_confirm_accepts_only_the_exact_in_memory_suggestion_id() {
        let (_root, mut host) = fixture();
        *host.operations.suggestion.lock().unwrap() = Some(hero_suggestion());
        let injected = request(
            &mut host,
            r#"{"jsonrpc":"2.0","id":301,"method":"mission.confirm","params":{"suggestionId":"suggestion-1700000000000-0123456789abcdef0123456789abcdef","reminderTarget":{"sourceIdentifier":"source-1","calendarIdentifier":"calendar-1"},"title":"Injected","workItems":[],"status":"completed"}}"#,
        );
        assert_eq!(injected.error.unwrap().code, -32_602);
        assert!(
            host.store
                .current_verified_audit_anchor()
                .unwrap()
                .is_none()
        );
        assert!(host.operations.suggestion.lock().unwrap().is_some());

        let wrong = request(
            &mut host,
            r#"{"jsonrpc":"2.0","id":302,"method":"mission.confirm","params":{"suggestionId":"suggestion-1700000000000-ffffffffffffffffffffffffffffffff","reminderTarget":{"sourceIdentifier":"source-1","calendarIdentifier":"calendar-1"}}}"#,
        );
        assert_eq!(wrong.error.unwrap().code, -32_602);
        assert!(
            host.store
                .current_verified_audit_anchor()
                .unwrap()
                .is_none()
        );
        assert!(host.operations.suggestion.lock().unwrap().is_some());
    }

    #[test]
    fn legacy_mission_confirm_never_accepts_choice_confirmation_authority() {
        let (_root, mut host) = fixture();
        let response = request(
            &mut host,
            r#"{"jsonrpc":"2.0","id":303,"method":"mission.confirm","params":{"confirmation":{"id":"choice-confirmation-1"}}}"#,
        );
        assert_eq!(response.error.expect("invalid legacy route").code, -32_602);
        assert!(
            host.store
                .current_verified_audit_anchor()
                .expect("read audit anchor")
                .is_none()
        );
    }

    #[test]
    fn choice_reminder_schedule_accepts_only_a_future_iana_zone_proposal() {
        assert!(super::valid_choice_reminder_time_zone(
            "America/Los_Angeles"
        ));
        assert!(super::valid_choice_reminder_time_zone("Etc/UTC"));
        assert!(!super::valid_choice_reminder_time_zone("not/a-time-zone"));

        let accepted_at_ms = super::now_ms().unwrap();
        let valid = ChoiceReminderScheduleInput {
            request_id: "schedule-request-1".to_owned(),
            choice_session_id: "choice-session-1".to_owned(),
            expected_session_revision: 1,
            reminder_list_id: "local-reminders".to_owned(),
            reminder_count: 1,
            due_at_ms: accepted_at_ms + 60_000,
            time_zone: "America/Los_Angeles".to_owned(),
        };
        assert!(valid.is_valid());
        assert!(valid.due_at_ms > accepted_at_ms);
        let expired = ChoiceReminderScheduleInput {
            due_at_ms: accepted_at_ms,
            ..valid
        };
        assert!(
            expired.is_valid(),
            "protocol shape alone never proves future time"
        );
        assert!(expired.due_at_ms <= accepted_at_ms);
    }

    #[test]
    #[allow(clippy::too_many_lines)] // One end-to-end boundary test keeps every Host-derived seal visible.
    fn choice_confirmation_binds_the_eventual_markdown_and_revisions_on_owner_edit() {
        let (root, mut host) = fixture();
        let broker = broker_record(&host);
        host.store.install_trusted_broker(&broker).unwrap();
        let on = host.store.prepare_runtime_control(true, 1).unwrap();
        host.store
            .commit_runtime_control(&on, &broker_receipt(&on, None))
            .unwrap();
        let selection = ModelSelection {
            id: "model-selection-confirmation".to_owned(),
            model_id: "gpt-test-model".to_owned(),
            requested_effort: "high".to_owned(),
            actual_effort: "high".to_owned(),
            catalog_fingerprint: "a".repeat(64),
            catalog_revision: 7,
            account_display_class: "chatgpt:plus".to_owned(),
            protocol_schema_revision: 1,
        };
        host.store
            .select_model_selection(&selection, 2)
            .expect("persist explicit model selection");
        let input = ChoiceBeginRequest {
            request_id: "choice-confirmation-begin".to_owned(),
            bounded_local_question: "Prepare one bounded local plan".to_owned(),
            expected_model_provenance_ref: selection.id.clone(),
            expected_catalog_fingerprint: selection.catalog_fingerprint.clone(),
            expected_catalog_revision: selection.catalog_revision,
            expected_protocol_revision: selection.protocol_schema_revision,
        };
        let accepted_at_ms = super::now_ms().unwrap();
        let (record, initial) = new_choice_begin_state(
            &input,
            &selection,
            on.revision,
            1,
            &persona_revision(),
            accepted_at_ms,
        )
        .unwrap();
        host.store
            .begin_choice_session_with_clock(
                &record,
                &initial,
                &ChoiceIdleClockEvidence {
                    boot_id: host.idle_boot_id.clone(),
                    wall_clock_ms: accepted_at_ms,
                    monotonic_ms: boot_scoped_monotonic_ms().unwrap(),
                },
            )
            .expect("commit initial intake");
        let generated = |goal: &str| StructuredChoiceGeneration {
            understood_goal: goal.to_owned(),
            current_context: "The exact local plan is ready for refinement.".to_owned(),
            assumptions: vec![],
            constraints: vec![],
            uncertainties: vec![],
            what_to_avoid: vec![],
            options: ["Review", "Narrow", "Prepare"]
                .map(|direction| StructuredChoiceOption {
                    direction: direction.to_owned(),
                    rationale: "Keep the work bounded.".to_owned(),
                    expected_result: "One clear next step.".to_owned(),
                    information_needed: vec![],
                    external_effects_preview: vec![],
                    source_categories: vec!["ownerInput".to_owned()],
                })
                .to_vec(),
            source_refs: vec![],
        };
        let initial_result =
            initial_choice_result_from_generation(&record, generated("Prepare the local plan"))
                .expect("derive initial Choice result");
        let active = host
            .store
            .commit_initial_choice_result(&initial_result)
            .expect("commit initial Choice result");
        let choice_set = active.active_choice_set.as_ref().expect("active ChoiceSet");
        let selected_at_ms = initial_result.completed_at_ms + 1;
        let selected = Selection::OptionSelection(OptionSelection {
            id: "choice-confirmation-selection".to_owned(),
            choice_session_id: active.session.id.clone(),
            choice_set_id: choice_set.id.clone(),
            selected_option_id: choice_set.options[0].id.clone(),
            expected_session_revision: active.session.revision,
            selected_at_ms,
        });
        let refining = host
            .store
            .commit_choice_selection(&selected, on.revision, selected_at_ms)
            .expect("commit selected direction");
        let operation = refining
            .pending_refinement_operation
            .as_ref()
            .expect("pending refinement");
        let refinement = super::refinement_result_from_generation(
            operation,
            generated("Review the prepared local plan"),
        )
        .expect("derive bound refinement");
        let refined = host
            .store
            .commit_choice_refinement_result(&refinement)
            .expect("commit bound refinement");
        let schedule_at_ms = refinement.completed_at_ms + 1;
        let schedule = host
            .store
            .record_choice_reminder_schedule(
                &ChoiceReminderScheduleInput {
                    request_id: "choice-confirmation-schedule".to_owned(),
                    choice_session_id: refined.session.id.clone(),
                    expected_session_revision: refined.session.revision,
                    reminder_list_id: DEFAULT_REMINDERS_LIST_ID.to_owned(),
                    reminder_count: 1,
                    due_at_ms: schedule_at_ms + 60_000,
                    time_zone: "Etc/UTC".to_owned(),
                },
                on.revision,
                schedule_at_ms,
            )
            .expect("record effect-free schedule");
        let create_preview = host
            .derive_choice_confirmation(&refined, &schedule, schedule_at_ms)
            .expect("derive no-clobber confirmation");
        assert_eq!(
            create_preview.persona_revision,
            refined
                .active_choice_set
                .as_ref()
                .expect("refined ChoiceSet")
                .persona_revision,
            "Host derives confirmation Persona provenance only from the current verified ChoiceSet"
        );
        assert_eq!(
            create_preview.markdown_entry.relative_path,
            format!("sessions/{}/CHOICE.md", refined.session.id)
        );
        assert!(create_preview.markdown_expected_base.is_none());
        assert_eq!(create_preview.markdown_manifest_digests.len(), 2);
        assert_eq!(
            create_preview.markdown_manifest_digests[0],
            refined.document_manifest.aggregate_digest
        );
        assert!(
            !root.path().join("Documents/OpenOpen").exists(),
            "reviewing a confirmation must not create the Markdown root"
        );

        let target = root
            .path()
            .join("Documents/OpenOpen")
            .join(&create_preview.markdown_entry.relative_path);
        let owner_parent = target.parent().expect("owner edit parent");
        std::fs::create_dir_all(owner_parent).expect("create owner edit directories");
        for directory in [
            root.path().join("Documents"),
            root.path().join("Documents/OpenOpen"),
            root.path().join("Documents/OpenOpen/sessions"),
            owner_parent.to_owned(),
        ] {
            std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))
                .expect("protect owner edit directory");
        }
        std::fs::write(&target, b"# Owner edit\n").expect("write owner edit");
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o600))
            .expect("protect owner edit");
        let replacement_preview = host
            .derive_choice_confirmation(&refined, &schedule, schedule_at_ms)
            .expect("derive descriptor-bound replacement confirmation");
        assert!(replacement_preview.markdown_expected_base.is_some());
        assert_ne!(
            replacement_preview.document_diff_digest,
            create_preview.document_diff_digest
        );
        assert_ne!(
            replacement_preview.payload_revision,
            create_preview.payload_revision
        );
        assert_ne!(replacement_preview.id, create_preview.id);
        assert_ne!(
            replacement_preview.payload_digest,
            create_preview.payload_digest
        );
        let (confirmed, intent) = host
            .store
            .commit_choice_confirmation_and_render_intent(
                &replacement_preview,
                on.revision,
                schedule_at_ms,
            )
            .expect("commit only the current descriptor-bound confirmation");
        assert_eq!(
            confirmed.session.state,
            ChoiceSessionState::AwaitingConfirmation
        );
        assert_eq!(confirmed.confirmation, Some(replacement_preview));
        assert!(
            host.store
                .markdown_render_receipt(&intent.id)
                .unwrap()
                .is_none(),
            "Choice confirmation must not publish Markdown before Reminder Evidence and Receipt"
        );
        host.operations.accept_committed_runtime(true, on.revision);
        install_test_core_lease(&mut host);
        let confirmation = confirmed.confirmation.clone().unwrap();
        let challenge = runtime_challenge(&mut host, 3_310);
        let authorized = request(
            &mut host,
            &json!({
                "jsonrpc": "2.0",
                "id": 3_311,
                "method": "choice.reminders.authorize",
                "params": {
                    "confirmationId": confirmation.id,
                    "reminderTarget": {
                        "sourceIdentifier": "eventkit-source",
                        "calendarIdentifier": "eventkit-calendar"
                    },
                    "authorization": on,
                    "brokerReceipt": broker_receipt(&on, Some(&challenge))
                }
            })
            .to_string(),
        );
        assert!(authorized.error.is_none(), "{authorized:?}");
        let authorized = authorized.result.unwrap();
        assert_eq!(authorized["choiceConfirmationId"], confirmation.id);
        assert_eq!(
            authorized["choicePayloadDigest"],
            confirmation.payload_digest
        );
        assert_eq!(
            authorized["choiceReminderPayloadDigest"],
            confirmation.reminder_payload_digest
        );
        assert_eq!(
            authorized["choiceReminderItems"].as_array().unwrap().len(),
            1
        );
        assert!(
            host.store
                .markdown_render_receipt(&intent.id)
                .unwrap()
                .is_none()
        );

        let challenge = runtime_challenge(&mut host, 3_312);
        let started = request(
            &mut host,
            &json!({
                "jsonrpc": "2.0", "id": 3_313,
                "method": "choice.reminders.begin",
                "params": {
                    "confirmationId": confirmation.id,
                    "authorization": on,
                    "brokerReceipt": broker_receipt(&on, Some(&challenge))
                }
            })
            .to_string(),
        );
        assert!(started.error.is_none(), "{started:?}");
        let started = started.result.unwrap();
        assert_eq!(started["executeNow"], true);
        let mission = &started["mission"];
        let mission_id = mission["missionId"].as_str().unwrap();
        let work_item_id = mission["workItems"][0]["id"].as_str().unwrap();
        let title = mission["workItems"][0]["title"].as_str().unwrap();
        let dispatch_token = mission["reminderDispatch"][0]["token"].as_str().unwrap();

        // A signed pre-commit cancellation is durable and permits exactly a
        // later explicit owner retry. It is distinct from an ambiguous
        // post-commit loss, which never calls this RPC.
        let reused_begin_proof = request(
            &mut host,
            &json!({
                "jsonrpc": "2.0", "id": 33_130,
                "method": "choice.reminders.abort-before-commit",
                "params": {
                    "confirmationId": confirmation.id,
                    "authorization": on,
                    "brokerReceipt": broker_receipt(&on, Some(&challenge))
                }
            })
            .to_string(),
        );
        assert_eq!(reused_begin_proof.error.unwrap().code, -32_602);
        let challenge = runtime_challenge(&mut host, 33_131);
        let aborted = request(
            &mut host,
            &json!({
                "jsonrpc": "2.0", "id": 33_132,
                "method": "choice.reminders.abort-before-commit",
                "params": {
                    "confirmationId": confirmation.id,
                    "authorization": on,
                    "brokerReceipt": broker_receipt(&on, Some(&challenge))
                }
            })
            .to_string(),
        );
        assert!(aborted.error.is_none(), "{aborted:?}");
        let challenge = runtime_challenge(&mut host, 33_133);
        let restarted = request(
            &mut host,
            &json!({
                "jsonrpc": "2.0", "id": 33_134,
                "method": "choice.reminders.begin",
                "params": {
                    "confirmationId": confirmation.id,
                    "authorization": on,
                    "brokerReceipt": broker_receipt(&on, Some(&challenge))
                }
            })
            .to_string(),
        );
        assert!(restarted.error.is_none(), "{restarted:?}");
        assert_eq!(restarted.result.unwrap()["executeNow"], true);

        // The retry authority is consumed in the same transaction that
        // records its new started attempt. A lost response, process restart,
        // or second begin can only enter read-only recovery until that exact
        // attempt is itself proven aborted before commit.
        let challenge = runtime_challenge(&mut host, 33_135);
        let replayed_retry = request(
            &mut host,
            &json!({
                "jsonrpc": "2.0", "id": 33_136,
                "method": "choice.reminders.begin",
                "params": {
                    "confirmationId": confirmation.id,
                    "authorization": on,
                    "brokerReceipt": broker_receipt(&on, Some(&challenge))
                }
            })
            .to_string(),
        );
        assert!(replayed_retry.error.is_none(), "{replayed_retry:?}");
        assert_eq!(replayed_retry.result.unwrap()["executeNow"], false);

        let challenge = runtime_challenge(&mut host, 3_314);
        let mirrored = request(
            &mut host,
            &json!({
                "jsonrpc": "2.0", "id": 3_315,
                "method": "choice.reminders.record",
                "params": {
                    "confirmationId": confirmation.id,
                    "links": [{
                        "missionId": mission_id,
                        "workItemId": work_item_id,
                        "sourceIdentifier": "eventkit-source",
                        "calendarIdentifier": "eventkit-calendar",
                        "calendarItemIdentifier": "eventkit-item-1",
                        "dispatchToken": dispatch_token,
                        "title": title
                    }],
                    "authorization": on,
                    "brokerReceipt": broker_receipt(&on, Some(&challenge))
                }
            })
            .to_string(),
        );
        assert!(mirrored.error.is_none(), "{mirrored:?}");

        let challenge = runtime_challenge(&mut host, 3_316);
        let completed = request(
            &mut host,
            &json!({
                "jsonrpc": "2.0", "id": 3_317,
                "method": "choice.reminders.complete",
                "params": {
                    "confirmationId": confirmation.id,
                    "completions": [{
                        "workItemId": work_item_id,
                        "sourceId": "eventkit-item-1",
                        "completedAtMs": super::now_ms().unwrap()
                    }],
                    "authorization": on,
                    "brokerReceipt": broker_receipt(&on, Some(&challenge))
                }
            })
            .to_string(),
        );
        assert!(completed.error.is_none(), "{completed:?}");
        let completed = completed.result.unwrap();
        assert_eq!(completed["choiceLoop"]["session"]["state"], "softIdle");
        assert_eq!(completed["receipt"]["missionId"], mission_id);
        assert!(
            completed["receipt"]["outputHashes"]
                .as_array()
                .unwrap()
                .iter()
                .any(|digest| digest == &json!(confirmation.payload_digest))
        );
        assert_eq!(
            completed["choiceLoop"]["documentManifest"]["aggregateDigest"],
            confirmation.markdown_manifest_digests[1]
        );
        assert!(
            target.exists(),
            "the confirmed exact CHOICE.md is published"
        );

        *host.operations.model_catalog_snapshot.lock().unwrap() = Some(
            test_model_catalog_snapshot(on.revision, super::now_ms().unwrap()),
        );
        let challenge = runtime_challenge(&mut host, 3_318);
        let resumed = request(
            &mut host,
            &json!({
                "jsonrpc": "2.0", "id": 3_319,
                "method": "choice.resume",
                "params": {
                    "authorization": on,
                    "brokerReceipt": broker_receipt(&on, Some(&challenge))
                }
            })
            .to_string(),
        );
        assert!(resumed.error.is_none(), "{resumed:?}");
        let resumed = resumed.result.unwrap();
        assert_eq!(resumed["session"]["state"], "refining");
        assert!(
            resumed["pendingRefinementOperation"]["selectionId"]
                .as_str()
                .is_some_and(|value| value.starts_with("resume-soft-idle-")),
            "the post-Receipt owner return must mint one body-free next-choice operation"
        );
    }

    #[test]
    fn mission_confirm_persists_the_exact_confirmation_lifecycle() {
        let (_root, mut host) = fixture();
        let before_click = super::now_ms().unwrap();
        let confirmed = confirm_hero_mission(&mut host);
        let after_click = super::now_ms().unwrap();
        assert_eq!(confirmed["title"], hero_suggestion().title);
        assert_eq!(confirmed["workItems"].as_array().unwrap().len(), 2);
        assert_eq!(
            confirmed["reminderAuthorization"]["listId"],
            "openopen.default-reminders"
        );
        assert!(host.operations.suggestion.lock().unwrap().is_none());

        let anchor = host.store.current_verified_audit_anchor().unwrap().unwrap();
        let mission = host
            .store
            .get_mission(confirmed["missionId"].as_str().unwrap(), &anchor)
            .unwrap()
            .unwrap();
        assert_eq!(mission.status, MissionStatus::Active);
        assert!(mission.created_at_ms >= before_click);
        assert!(mission.created_at_ms <= after_click);
        assert_ne!(mission.created_at_ms, 1_700_000_000_000);
        assert!(
            mission
                .work_items
                .iter()
                .all(|item| item.status == WorkItemStatus::Pending)
        );
        assert_eq!(mission.approvals.len(), 2);
        assert!(mission.approvals.iter().all(|approval| {
            approval.status == ApprovalStatus::Approved
                && approval.decided_by_id.as_deref() == Some("openopen-local-owner")
                && approval.decided_at_ms == Some(mission.created_at_ms)
        }));
        let reminder_approval = mission
            .approvals
            .iter()
            .find(|approval| approval.kind == ApprovalKind::NewExternalWrite)
            .unwrap();
        assert_eq!(
            reminder_approval.target,
            Some(ApprovalTarget::ReminderList {
                logical_list_id: "openopen.default-reminders".to_owned(),
                source_identifier: "source-1".to_owned(),
                calendar_identifier: "calendar-1".to_owned(),
            })
        );
        let payload = super::reminder_write_payload(
            &mission.id,
            &super::ReminderTarget {
                source_identifier: "source-1".to_owned(),
                calendar_identifier: "calendar-1".to_owned(),
            },
            mission
                .work_items
                .iter()
                .map(|item| (item.id.as_str(), item.title.as_str())),
        )
        .unwrap();
        let proposal = ActionProposal {
            effect: EffectKind::ReminderWrite,
            mission_id: mission.id.clone(),
            mission_scope_digest: mission.scope_digest.clone(),
            target: ActionTarget::ReminderList {
                list_id: "openopen.default-reminders".to_owned(),
            },
            estimated_cost_micros: None,
        };
        assert_eq!(
            ActionGate.authorize(&mission, &proposal, Some(&payload)),
            GateDecision::Allowed
        );

        let connection = Connection::open(&host.paths.store).unwrap();
        let states = connection
            .prepare("SELECT state_kind FROM audit_ledger ORDER BY sequence")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(
            states,
            vec![
                "choice:model_selection",
                "mission:\"proposed\"",
                "mission:\"awaitingConfirmation\"",
                "mission:\"awaitingConfirmation\"",
                "mission:\"active\"",
                "mission:\"needsMe\"",
                "mission:\"needsMe\"",
                "mission:\"active\"",
            ]
        );
    }

    #[test]
    fn reminder_write_payload_has_the_cross_language_fixed_hash() {
        let payload = super::reminder_write_payload(
            "mission-1",
            &super::ReminderTarget {
                source_identifier: "source-1".to_owned(),
                calendar_identifier: "calendar-1".to_owned(),
            },
            [("work-1", "Pick one priority")],
        )
        .unwrap();
        assert_eq!(
            format!("{:x}", Sha256::digest(payload)),
            "188605fc48e5a3bc42efee3820582cb016a84685869bfbb6688daf79b055fab0"
        );
    }

    #[test]
    fn confirmation_retry_cannot_change_the_bound_physical_reminders_target() {
        let (_root, mut host) = fixture();
        let confirmed = confirm_hero_mission(&mut host);
        let anchor = host.store.current_verified_audit_anchor().unwrap().unwrap();
        let changed = request(
            &mut host,
            r#"{"jsonrpc":"2.0","id":304,"method":"mission.confirm","params":{"suggestionId":"suggestion-1700000000000-0123456789abcdef0123456789abcdef","reminderTarget":{"sourceIdentifier":"source-2","calendarIdentifier":"calendar-2"}}}"#,
        );
        assert_eq!(changed.error.unwrap().code, -32_602);
        assert_eq!(
            host.store.current_verified_audit_anchor().unwrap().unwrap(),
            anchor
        );
        let dashboard = request(
            &mut host,
            r#"{"jsonrpc":"2.0","id":305,"method":"mission.dashboard.read","params":{}}"#,
        )
        .result
        .unwrap();
        assert_eq!(
            dashboard["confirmedMission"]["missionId"],
            confirmed["missionId"]
        );
        assert_eq!(
            dashboard["confirmedMission"]["reminderAuthorization"]["writeDisposition"],
            "recoverOnly"
        );
    }

    #[test]
    fn reminder_dispatch_is_durable_at_most_once_across_response_loss_and_restart() {
        let (_root, mut host) = fixture();
        let paths = host.paths.clone();
        let confirmed = confirm_hero_mission(&mut host);
        let begin = json!({
            "jsonrpc": "2.0",
            "id": 306,
            "method": "mission.reminders.begin",
            "params": {"missionId": confirmed["missionId"]}
        });
        let started = request(&mut host, &begin.to_string()).result.unwrap();
        assert_eq!(started["executeNow"], true);
        assert_eq!(
            started["mission"]["reminderDispatch"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert!(
            started["mission"]["reminderLinks"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        let anchor = host.store.current_verified_audit_anchor().unwrap().unwrap();
        drop(host);

        let mut reopened = Host::open(paths, [7_u8; 32]).unwrap();
        let recovered = request(&mut reopened, &begin.to_string()).result.unwrap();
        assert_eq!(recovered["executeNow"], false);
        assert_eq!(
            recovered["mission"]["reminderDispatch"],
            started["mission"]["reminderDispatch"]
        );
        assert_eq!(
            reopened
                .store
                .current_verified_audit_anchor()
                .unwrap()
                .unwrap(),
            anchor
        );
    }

    #[test]
    fn reminder_mirror_record_is_durable_exactly_idempotent_and_changed_retry_fails() {
        let (_root, mut host) = fixture();
        let confirmed = confirm_hero_mission(&mut host);
        let recorded = record_hero_mirror(&mut host, &confirmed, "eventkit-mirror");
        assert_eq!(recorded["reminderLinks"].as_array().unwrap().len(), 2);
        assert_eq!(
            recorded["reminderAuthorization"]["writeDisposition"],
            "recoverOnly"
        );
        let anchor = host.store.current_verified_audit_anchor().unwrap().unwrap();
        let retry = record_hero_mirror(&mut host, &confirmed, "eventkit-mirror");
        assert_eq!(retry, recorded);
        assert_eq!(
            host.store.current_verified_audit_anchor().unwrap().unwrap(),
            anchor
        );

        let mut links = recorded["reminderLinks"].as_array().unwrap().clone();
        links[0]["calendarItemIdentifier"] = json!("changed-reminder");
        let changed = request(
            &mut host,
            &json!({
                "jsonrpc": "2.0",
                "id": 398,
                "method": "mission.reminders.record",
                "params": {"missionId": recorded["missionId"], "links": links}
            })
            .to_string(),
        );
        assert_eq!(changed.error.unwrap().code, -32_602);
        assert_eq!(
            host.store.current_verified_audit_anchor().unwrap().unwrap(),
            anchor
        );
    }

    #[test]
    fn composite_confirmation_and_completion_failures_rollback_every_write() {
        let (_root, mut host) = fixture();
        let mission_id = "mission-atomic-confirm";
        let commands = vec![
            MissionCommand::Create {
                input: CreateMission {
                    mission_id: mission_id.to_owned(),
                    title: "Atomic confirmation".to_owned(),
                    outcome: "Prove rollback".to_owned(),
                    owner_id: "openopen-local-owner".to_owned(),
                    scope_digest: "scope-atomic".to_owned(),
                    scope_approval_id: "approval-atomic".to_owned(),
                    scope_approval_prompt: "Confirm".to_owned(),
                    work_items: vec![CreateWorkItem {
                        id: "work-atomic".to_owned(),
                        title: "One step".to_owned(),
                    }],
                    now_ms: 1,
                },
            },
            MissionCommand::Activate {
                mission_id: mission_id.to_owned(),
                now_ms: 1,
            },
        ];
        let batch = super::mission_command_batch(None, mission_id, commands).unwrap();
        assert!(host.store.execute_mission_command_batch(&batch).is_err());
        assert!(
            host.store
                .current_verified_audit_anchor()
                .unwrap()
                .is_none()
        );

        let confirmed = confirm_hero_mission(&mut host);
        let mission_id = confirmed["missionId"].as_str().unwrap();
        let original_anchor = host.store.current_verified_audit_anchor().unwrap().unwrap();
        let mission = host
            .store
            .get_mission(mission_id, &original_anchor)
            .unwrap()
            .unwrap();
        let commands = vec![
            MissionCommand::TransitionWorkItem {
                mission_id: mission.id.clone(),
                work_item_id: mission.work_items[0].id.clone(),
                next: WorkItemStatus::Active,
                evidence_ids: Vec::new(),
                now_ms: mission.updated_at_ms,
            },
            MissionCommand::TransitionWorkItem {
                mission_id: mission.id.clone(),
                work_item_id: mission.work_items[1].id.clone(),
                next: WorkItemStatus::Completed,
                evidence_ids: Vec::new(),
                now_ms: mission.updated_at_ms,
            },
        ];
        let batch =
            super::mission_command_batch(Some(&original_anchor), &mission.id, commands).unwrap();
        assert!(host.store.execute_mission_command_batch(&batch).is_err());
        assert_eq!(
            host.store.current_verified_audit_anchor().unwrap().unwrap(),
            original_anchor
        );
        let unchanged = host
            .store
            .get_mission(&mission.id, &original_anchor)
            .unwrap()
            .unwrap();
        assert!(
            unchanged
                .work_items
                .iter()
                .all(|item| item.status == WorkItemStatus::Pending)
        );
    }

    #[test]
    fn exact_suggestion_retry_is_idempotent_but_an_active_mission_blocks_a_distinct_suggestion() {
        let (_root, mut host) = fixture();
        let first = confirm_hero_mission(&mut host);
        let first_mission_id = first["missionId"].as_str().unwrap().to_owned();
        let first_anchor = host.store.current_verified_audit_anchor().unwrap().unwrap();

        *host.operations.suggestion.lock().unwrap() = Some(hero_suggestion());
        let retry = request(
            &mut host,
            r#"{"jsonrpc":"2.0","id":305,"method":"mission.confirm","params":{"suggestionId":"suggestion-1700000000000-0123456789abcdef0123456789abcdef","reminderTarget":{"sourceIdentifier":"source-1","calendarIdentifier":"calendar-1"}}}"#,
        )
        .result
        .unwrap();
        assert_eq!(retry["missionId"], first_mission_id);
        assert_eq!(
            host.store.current_verified_audit_anchor().unwrap().unwrap(),
            first_anchor
        );

        let second_id = "suggestion-1700000000000-89abcdef0123456789abcdef01234567";
        *host.operations.suggestion.lock().unwrap() = Some(hero_suggestion_with_id(second_id));
        let rejected = request(
            &mut host,
            &json!({
                "jsonrpc": "2.0",
                "id": 306,
                "method": "mission.confirm",
                "params": {"suggestionId": second_id, "reminderTarget": {"sourceIdentifier": "source-1", "calendarIdentifier": "calendar-1"}}
            })
            .to_string(),
        );
        assert_eq!(rejected.error.unwrap().code, -32_602);
        assert_eq!(
            host.store.current_verified_audit_anchor().unwrap().unwrap(),
            first_anchor
        );
        let missions = host.store.list_missions(&first_anchor).unwrap();
        assert_eq!(missions.len(), 1);
        assert_eq!(missions[0].id, first_mission_id);
        assert!(
            host.operations
                .suggestion
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .as_ref()
                .is_some_and(|value| value.id == second_id),
            "rejecting a distinct suggestion while a Mission is active must not consume it"
        );
    }

    #[test]
    fn confirmation_response_loss_recovers_exactly_after_restart_without_volatile_state() {
        let (_root, mut host) = fixture();
        let paths = host.paths.clone();
        let confirmed = confirm_hero_mission(&mut host);
        let committed_anchor = host.store.current_verified_audit_anchor().unwrap().unwrap();
        drop(host);

        let mut reopened = Host::open(paths, [7_u8; 32]).unwrap();
        assert!(reopened.operations.suggestion.lock().unwrap().is_none());
        let retry = request(
            &mut reopened,
            r#"{"jsonrpc":"2.0","id":307,"method":"mission.confirm","params":{"suggestionId":"suggestion-1700000000000-0123456789abcdef0123456789abcdef","reminderTarget":{"sourceIdentifier":"source-1","calendarIdentifier":"calendar-1"}}}"#,
        )
        .result
        .unwrap();
        assert_eq!(retry["missionId"], confirmed["missionId"]);
        assert_eq!(
            retry["reminderAuthorization"]["writeDisposition"],
            "recoverOnly"
        );
        assert_eq!(
            reopened
                .store
                .current_verified_audit_anchor()
                .unwrap()
                .unwrap(),
            committed_anchor
        );
        let dashboard = request(
            &mut reopened,
            r#"{"jsonrpc":"2.0","id":308,"method":"mission.dashboard.read","params":{}}"#,
        )
        .result
        .unwrap();
        assert_eq!(dashboard["confirmedMission"], retry);
        assert_eq!(dashboard["activeCards"].as_array().unwrap().len(), 1);
        assert!(dashboard["receipt"].is_null());
    }

    #[test]
    fn reminder_completion_rejects_incomplete_mismatched_and_duplicate_batches() {
        let (_root, mut host) = fixture();
        let confirmed = confirm_hero_mission(&mut host);
        let confirmed = record_hero_mirror(&mut host, &confirmed, "eventkit-reminder");
        let mission_id = confirmed["missionId"].as_str().unwrap();
        let work_items = confirmed["workItems"].as_array().unwrap();
        let completed_at_ms = host
            .store
            .get_mission(
                mission_id,
                &host.store.current_verified_audit_anchor().unwrap().unwrap(),
            )
            .unwrap()
            .unwrap()
            .updated_at_ms;
        let original_anchor = host.store.current_verified_audit_anchor().unwrap().unwrap();

        for completions in [
            json!([{
                "workItemId": work_items[0]["id"],
                "sourceId": "reminder-1",
                "completedAtMs": completed_at_ms
            }]),
            json!([
                {
                    "workItemId": work_items[0]["id"],
                    "sourceId": "reminder-1",
                    "completedAtMs": completed_at_ms
                },
                {
                    "workItemId": "work-not-persisted",
                    "sourceId": "reminder-2",
                    "completedAtMs": completed_at_ms
                }
            ]),
            json!([
                {
                    "workItemId": work_items[0]["id"],
                    "sourceId": "reminder-1",
                    "completedAtMs": completed_at_ms
                },
                {
                    "workItemId": work_items[0]["id"],
                    "sourceId": "reminder-2",
                    "completedAtMs": completed_at_ms
                }
            ]),
        ] {
            let response = request(
                &mut host,
                &json!({
                    "jsonrpc": "2.0",
                    "id": 303,
                    "method": "mission.reminders.complete",
                    "params": {"missionId": mission_id, "completions": completions}
                })
                .to_string(),
            );
            assert_eq!(response.error.unwrap().code, -32_602);
            assert_eq!(
                host.store.current_verified_audit_anchor().unwrap().unwrap(),
                original_anchor
            );
        }
    }

    #[test]
    fn verified_reminder_readback_issues_an_evidence_backed_receipt() {
        let (_root, mut host) = fixture();
        let confirmed = confirm_hero_mission(&mut host);
        let confirmed = record_hero_mirror(&mut host, &confirmed, "eventkit-reminder");
        let mission_id = confirmed["missionId"].as_str().unwrap().to_owned();
        let anchor = host.store.current_verified_audit_anchor().unwrap().unwrap();
        let mission = host
            .store
            .get_mission(&mission_id, &anchor)
            .unwrap()
            .unwrap();
        let dispatch_started_at_ms = mission
            .evidence
            .iter()
            .filter(|evidence| evidence.kind == EvidenceKind::ReminderDispatchStarted)
            .map(|evidence| evidence.observed_at_ms)
            .max()
            .unwrap();
        assert!(dispatch_started_at_ms < mission.updated_at_ms);
        let completions = mission
            .work_items
            .iter()
            .enumerate()
            .map(|(index, item)| {
                json!({
                    "workItemId": item.id,
                    "sourceId": confirmed["reminderLinks"][index]["calendarItemIdentifier"],
                    "completedAtMs": dispatch_started_at_ms
                })
            })
            .collect::<Vec<_>>();
        let decoded =
            serde_json::from_value::<Vec<super::ReminderCompletion>>(json!(completions)).unwrap();
        assert!(
            super::validated_reminder_completions(
                &mission,
                &host.authority,
                decoded,
                super::now_ms().unwrap(),
                true,
            )
            .is_some()
        );
        let response = request(
            &mut host,
            &json!({
                "jsonrpc": "2.0",
                "id": 304,
                "method": "mission.reminders.complete",
                "params": {"missionId": mission_id, "completions": completions}
            })
            .to_string(),
        );
        let receipt: Receipt = serde_json::from_value(
            response
                .result
                .unwrap_or_else(|| panic!("completion failed: {:?}", response.error)),
        )
        .unwrap();
        assert_eq!(receipt.actual_model, "gpt-test-model");
        assert_eq!(receipt.evidence_ids.len(), mission.work_items.len());

        let completed_anchor = host.store.current_verified_audit_anchor().unwrap().unwrap();
        let completed = host
            .store
            .get_mission(&mission_id, &completed_anchor)
            .unwrap()
            .unwrap();
        assert_eq!(completed.status, MissionStatus::Completed);
        assert!(
            completed
                .work_items
                .iter()
                .all(|item| item.status == WorkItemStatus::Completed)
        );
        assert_eq!(completed.evidence.len(), completed.work_items.len() * 3);
        for evidence in &completed.evidence {
            host.authority.verify_evidence(evidence).unwrap();
        }
        assert_eq!(
            completed
                .evidence
                .iter()
                .filter(|evidence| evidence.kind == EvidenceKind::ReminderCompleted)
                .count(),
            completed.work_items.len()
        );
        assert_eq!(
            host.store
                .get_receipt(&receipt.id, &completed_anchor)
                .unwrap(),
            Some(receipt)
        );
    }

    #[test]
    fn local_reminder_evidence_can_complete_a_routed_mission_without_channel_outbound() {
        let (_root, mut host) = fixture();
        host.store
            .install_trusted_broker(&broker_record(&host))
            .unwrap();
        let on = host.store.prepare_runtime_control(true, 1).unwrap();
        let broker = broker_receipt(&on, None);
        host.store.commit_runtime_control(&on, &broker).unwrap();
        host.operations.accept_committed_runtime(true, on.revision);
        let mission_id = seed_primary_discord_mission(&mut host);
        let dashboard = request(
            &mut host,
            r#"{"jsonrpc":"2.0","id":320,"method":"mission.dashboard.read","params":{}}"#,
        )
        .result
        .unwrap();
        let confirmed = record_hero_mirror(
            &mut host,
            &dashboard["confirmedMission"],
            "eventkit-local-only",
        );
        let anchor = host.store.current_verified_audit_anchor().unwrap().unwrap();
        let mission = host
            .store
            .get_mission(&mission_id, &anchor)
            .unwrap()
            .unwrap();
        let completions = mission
            .work_items
            .iter()
            .enumerate()
            .map(|(index, item)| {
                json!({
                    "workItemId": item.id,
                    "sourceId": confirmed["reminderLinks"][index]["calendarItemIdentifier"],
                    "completedAtMs": mission.updated_at_ms,
                })
            })
            .collect::<Vec<_>>();
        let receipt = request(
            &mut host,
            &json!({
                "jsonrpc": "2.0",
                "id": 321,
                "method": "mission.reminders.complete",
                "params": {
                    "missionId": mission_id,
                    "completions": completions,
                    "receiptReturnApprovedAtMs": null,
                    "receiptReturnRouteId": null,
                }
            })
            .to_string(),
        );
        assert!(receipt.error.is_none(), "{receipt:?}");
        assert_eq!(receipt.result.unwrap()["actualModel"], "gpt-test-model");
        let outbound_count = Connection::open(&host.paths.store)
            .unwrap()
            .query_row("SELECT COUNT(*) FROM channel_outbound", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap();
        assert_eq!(outbound_count, 0);
    }

    #[test]
    fn typed_mission_cancel_preserves_started_reminder_and_route_audit_without_effects() {
        let (_root, mut host) = fixture();
        host.store
            .install_trusted_broker(&broker_record(&host))
            .unwrap();
        let on = host.store.prepare_runtime_control(true, 1).unwrap();
        let broker = broker_receipt(&on, None);
        host.store.commit_runtime_control(&on, &broker).unwrap();
        host.operations.accept_committed_runtime(true, on.revision);
        let mission_id = seed_primary_discord_mission(&mut host);
        let started = request(
            &mut host,
            &json!({
                "jsonrpc": "2.0",
                "id": 322,
                "method": "mission.reminders.begin",
                "params": {"missionId": mission_id}
            })
            .to_string(),
        );
        assert_eq!(started.result.unwrap()["executeNow"], true);
        let anchor = host.store.current_verified_audit_anchor().unwrap().unwrap();
        let before = host
            .store
            .get_mission(&mission_id, &anchor)
            .unwrap()
            .unwrap();
        let before_evidence = before.evidence.clone();
        let before_routes = host.store.channel_route_set(&mission_id).unwrap().unwrap();
        let challenge = runtime_challenge(&mut host, 323);
        let cancelled = request(
            &mut host,
            &json!({
                "jsonrpc": "2.0",
                "id": 324,
                "method": "mission.cancel",
                "params": {
                    "missionId": mission_id,
                    "authorization": on,
                    "brokerReceipt": broker_receipt(&on, Some(&challenge)),
                }
            })
            .to_string(),
        );
        assert_eq!(cancelled.result.unwrap()["status"], "cancelled");
        let cancelled_anchor = host.store.current_verified_audit_anchor().unwrap().unwrap();
        let cancelled_mission = host
            .store
            .get_mission(&mission_id, &cancelled_anchor)
            .unwrap()
            .unwrap();
        assert_eq!(cancelled_mission.status, MissionStatus::Cancelled);
        assert_eq!(cancelled_mission.evidence, before_evidence);
        assert_eq!(
            host.store.channel_route_set(&mission_id).unwrap().unwrap(),
            before_routes
        );
        assert!(
            host.store
                .list_receipts(&cancelled_anchor)
                .unwrap()
                .is_empty()
        );
        assert_eq!(channel_outbound_count(&host), 0);

        let dashboard = dashboard(&mut host, 325);
        assert!(dashboard["activeCards"].as_array().unwrap().is_empty());
        assert!(dashboard["confirmedMission"].is_null());
        assert!(dashboard["receipt"].is_null());

        let retry_challenge = runtime_challenge(&mut host, 326);
        let terminal_retry = request(
            &mut host,
            &json!({
                "jsonrpc": "2.0",
                "id": 327,
                "method": "mission.cancel",
                "params": {
                    "missionId": mission_id,
                    "authorization": on,
                    "brokerReceipt": broker_receipt(&on, Some(&retry_challenge)),
                }
            })
            .to_string(),
        );
        assert!(terminal_retry.error.is_some());
        assert_eq!(
            host.store.current_verified_audit_anchor().unwrap().unwrap(),
            cancelled_anchor
        );
    }

    #[test]
    fn completion_response_loss_retries_exactly_after_restart_and_dashboard_recovers() {
        let (_root, mut host) = fixture();
        let paths = host.paths.clone();
        let confirmed = confirm_hero_mission(&mut host);
        let confirmed = record_hero_mirror(&mut host, &confirmed, "eventkit-restart");
        let mission_id = confirmed["missionId"].as_str().unwrap().to_owned();
        let anchor = host.store.current_verified_audit_anchor().unwrap().unwrap();
        let mission = host
            .store
            .get_mission(&mission_id, &anchor)
            .unwrap()
            .unwrap();
        let completions = mission
            .work_items
            .iter()
            .enumerate()
            .map(|(index, item)| {
                json!({
                    "workItemId": item.id,
                    "sourceId": confirmed["reminderLinks"][index]["calendarItemIdentifier"],
                    "completedAtMs": mission.updated_at_ms
                })
            })
            .collect::<Vec<_>>();
        let complete_request = json!({
            "jsonrpc": "2.0",
            "id": 309,
            "method": "mission.reminders.complete",
            "params": {"missionId": mission_id, "completions": completions}
        });
        let receipt = request(&mut host, &complete_request.to_string())
            .result
            .unwrap();
        let completed_anchor = host.store.current_verified_audit_anchor().unwrap().unwrap();
        drop(host);

        let mut reopened = Host::open(paths, [7_u8; 32]).unwrap();
        let retry = request(&mut reopened, &complete_request.to_string())
            .result
            .unwrap();
        assert_eq!(retry, receipt);
        assert_eq!(
            reopened
                .store
                .current_verified_audit_anchor()
                .unwrap()
                .unwrap(),
            completed_anchor
        );
        let mut changed = complete_request;
        changed["params"]["completions"][0]["sourceId"] = json!("changed-source");
        assert_eq!(
            request(&mut reopened, &changed.to_string())
                .error
                .unwrap()
                .code,
            -32_602
        );
        assert_eq!(
            reopened
                .store
                .current_verified_audit_anchor()
                .unwrap()
                .unwrap(),
            completed_anchor
        );
        let dashboard = request(
            &mut reopened,
            r#"{"jsonrpc":"2.0","id":310,"method":"mission.dashboard.read","params":{}}"#,
        )
        .result
        .unwrap();
        assert!(dashboard["confirmedMission"].is_null());
        assert!(dashboard["activeCards"].as_array().unwrap().is_empty());
        assert_eq!(dashboard["receipt"], receipt);

        assert_dashboard_hides_historical_receipt_while_new_mission_is_active(
            &mut reopened,
            &receipt,
        );
    }

    fn assert_dashboard_hides_historical_receipt_while_new_mission_is_active(
        reopened: &mut Host,
        historical_receipt: &Value,
    ) {
        let second_id = "suggestion-1700000000000-89abcdef0123456789abcdef01234567";
        *reopened.operations.suggestion.lock().unwrap() = Some(hero_suggestion_with_id(second_id));
        let pending = request(
            reopened,
            r#"{"jsonrpc":"2.0","id":311,"method":"mission.dashboard.read","params":{}}"#,
        )
        .result
        .unwrap();
        assert_eq!(pending["suggestion"]["id"], second_id);
        assert!(pending["confirmedMission"].is_null());
        assert!(pending["receipt"].is_null());
        assert!(pending["channelRouteSet"].is_null());
        assert!(pending["activeCards"].as_array().unwrap().is_empty());
        let second = request(
            reopened,
            &json!({
                "jsonrpc": "2.0",
                "id": 312,
                "method": "mission.confirm",
                "params": {"suggestionId": second_id, "reminderTarget": {"sourceIdentifier": "source-1", "calendarIdentifier": "calendar-1"}}
            })
            .to_string(),
        )
        .result
        .unwrap();
        let dashboard = request(
            reopened,
            r#"{"jsonrpc":"2.0","id":313,"method":"mission.dashboard.read","params":{}}"#,
        )
        .result
        .unwrap();
        assert_eq!(
            dashboard["confirmedMission"]["missionId"],
            second["missionId"]
        );
        assert_eq!(
            dashboard["confirmedMission"]["reminderAuthorization"]["writeDisposition"],
            "recoverOnly"
        );
        assert!(dashboard["receipt"].is_null());
        assert_ne!(
            dashboard["confirmedMission"]["missionId"],
            historical_receipt["missionId"]
        );
        assert_eq!(dashboard["activeCards"].as_array().unwrap().len(), 1);
        assert_eq!(dashboard["activeCards"][0]["id"], second["missionId"]);
        assert_eq!(dashboard["activeCards"][0]["state"], "working");
    }

    #[test]
    fn retired_outcome_proposal_is_not_dispatchable_or_a_model_entry_route() {
        let (_root, mut host) = fixture();
        let response = request(
            &mut host,
            r#"{"jsonrpc":"2.0","id":2,"method":"outcome.propose","params":{}}"#,
        );
        assert_eq!(response.error.unwrap().code, -32_601);
        assert!(
            std::fs::read_dir(&host.paths.model_input_root)
                .unwrap()
                .next()
                .is_none()
        );
    }

    #[test]
    fn persona_default_is_read_only_and_mutable_persona_routes_are_not_public_in_pr1() {
        let (_root, mut host) = fixture();
        let status = request(
            &mut host,
            r#"{"jsonrpc":"2.0","id":18,"method":"persona.status","params":{}}"#,
        )
        .result
        .expect("default Persona status is read-only");
        assert_eq!(
            status["status"]["active"]["personaId"],
            "openopen.nondev.default"
        );
        assert_eq!(status["status"]["active"]["revision"], "draft-03-en");

        for (id, method) in [
            (19, "persona.stage"),
            (20, "persona.activate"),
            (21, "persona.rollback"),
        ] {
            let response = request(
                &mut host,
                &json!({"jsonrpc": "2.0", "id": id, "method": method, "params": {}}).to_string(),
            );
            assert_eq!(response.error.expect("must not dispatch").code, -32_601);
        }
        assert!(
            host.store
                .current_verified_audit_anchor()
                .expect("verify audit")
                .is_none(),
            "Persona status and rejected update routes cannot create an audit event"
        );
    }

    #[test]
    fn receipt_cleanup_accepts_no_caller_body_path_or_receipt_fields() {
        let (_root, mut host) = fixture();
        let response = dispatch_public(
            &mut host,
            r#"{"jsonrpc":"2.0","id":22,"method":"choice.markdown.receipt.cleanup","params":{"body":"injected","path":"CHOICE.md","receipt":{"intentId":"forged"}}}"#,
        );
        assert_eq!(
            response.error.expect("must reject caller material").code,
            -32_602
        );
        assert!(
            host.store
                .current_verified_audit_anchor()
                .expect("verify audit")
                .is_none(),
            "receipt cleanup cannot mint a journal, receipt, or audit event"
        );
    }

    #[test]
    fn receipt_cleanup_availability_is_read_only_and_false_without_a_receipted_cancelled_choice() {
        let (_root, mut host) = fixture();
        let response = dispatch_public(
            &mut host,
            r#"{"jsonrpc":"2.0","id":23,"method":"choice.markdown.receipt.cleanup.available","params":{}}"#,
        );
        assert_eq!(response.error, None);
        assert_eq!(response.result.unwrap()["available"], false);
        assert!(
            host.store
                .current_verified_audit_anchor()
                .expect("verify audit")
                .is_none(),
            "availability reads must not create a cleanup journal or audit event"
        );
    }

    #[test]
    fn retired_mission_and_reminder_mutation_routes_are_not_dispatchable() {
        let (_root, mut host) = fixture();
        for (id, method, params) in [
            (
                3,
                "mission.confirm",
                json!({
                    "suggestionId": "suggestion-1700000000000-0123456789abcdef0123456789abcdef",
                    "reminderTarget": {"sourceIdentifier": "source-1", "calendarIdentifier": "calendar-1"}
                }),
            ),
            (4, "mission.cancel", json!({"missionId": "mission-1"})),
            (
                5,
                "mission.reminders.begin",
                json!({"missionId": "mission-1"}),
            ),
            (
                6,
                "mission.reminders.record",
                json!({"missionId": "mission-1", "links": []}),
            ),
            (
                7,
                "mission.reminders.complete",
                json!({"missionId": "mission-1", "completions": []}),
            ),
        ] {
            let response = dispatch_public(
                &mut host,
                &json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})
                    .to_string(),
            );
            assert_eq!(
                response.error.expect("retired route must fail").code,
                -32_001
            );
        }
        assert!(
            host.store
                .current_verified_audit_anchor()
                .expect("verify audit")
                .is_none(),
            "retired routes cannot mint a Mission or audit event"
        );
    }

    #[test]
    fn primitive_params_and_unknown_routes_fail_closed() {
        let (_root, mut host) = fixture();
        let invalid = request(
            &mut host,
            r#"{"jsonrpc":"2.0","id":7,"method":"account.read","params":"invalid"}"#,
        );
        assert_eq!(invalid.error.unwrap().code, -32_602);
        let unknown = request(
            &mut host,
            r#"{"jsonrpc":"2.0","id":8,"method":"future.route","params":{}}"#,
        );
        assert_eq!(unknown.error.unwrap().code, -32_601);
    }

    #[test]
    fn rust_core_exposes_only_public_identity_and_signs_broker_enrollment() {
        let (_root, mut host) = fixture();
        let identity = request(
            &mut host,
            r#"{"jsonrpc":"2.0","id":20,"method":"broker.identity.read","params":{}}"#,
        )
        .result
        .unwrap();
        assert_eq!(identity["coreKeyId"], host.authority.effect_key_id());
        assert_eq!(
            identity["coreVerifyingKeyHex"],
            host.authority.effect_verifying_key_hex()
        );

        let broker_key = SigningKey::from_bytes(&[9_u8; 32]);
        let broker_verifying = broker_key.verifying_key().to_bytes();
        let broker_key_id = hex::encode(Sha256::digest(broker_verifying));
        let signed = request(
            &mut host,
            &serde_json::json!({
                "jsonrpc": "2.0",
                "id": 21,
                "method": "broker.enrollment.sign",
                "params": {
                    "brokerKeyId": broker_key_id,
                    "brokerVerifyingKeyHex": hex::encode(broker_verifying),
                    "helperDesignatedRequirementDigest": "ab".repeat(32),
                    "installedAtMs": 1
                }
            })
            .to_string(),
        )
        .result
        .unwrap();
        let record: BrokerEnrollmentRecord = serde_json::from_value(signed).unwrap();
        assert!(
            TrustedBrokerEnrollment::from_signed_install_record(&host.authority, &record).is_ok()
        );
    }

    #[test]
    fn model_and_on_routes_require_exact_broker_signed_process_lease() {
        let (_root, mut host) = fixture();
        host.store
            .install_trusted_broker(&broker_record(&host))
            .unwrap();
        let broker_key = SigningKey::from_bytes(&[41_u8; 32]);
        let mut lease = CoreInstanceLease {
            protocol_version: EFFECT_PROTOCOL_VERSION,
            audit_euid: rustix::process::geteuid().as_raw(),
            app_pid: 1,
            app_start_time_us: 1,
            core_pid: i32::try_from(std::process::id()).unwrap(),
            core_start_time_us: 1,
            core_audit_token_hex: "aa".repeat(32),
            codex_pid: 42,
            codex_start_time_us: 2,
            codex_audit_token_hex: "bb".repeat(32),
            core_instance_nonce: host.instance_nonce.clone(),
            issued_at_ms: super::now_ms().unwrap(),
            broker_key_id: format!(
                "{:x}",
                Sha256::digest(broker_key.verifying_key().to_bytes())
            ),
            broker_signature_hex: String::new(),
        };
        lease.broker_signature_hex = hex::encode(
            broker_key
                .sign(&core_instance_lease_signing_bytes(&lease).unwrap())
                .to_bytes(),
        );
        *host
            .operations
            .codex_pid
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(42);
        let installed = request(
            &mut host,
            &serde_json::json!({
                "jsonrpc": "2.0",
                "id": 22,
                "method": "broker.lease.install",
                "params": {"lease": lease}
            })
            .to_string(),
        );
        assert_eq!(installed.result.unwrap()["status"], "installed");
        assert!(
            request(
                &mut host,
                r#"{"jsonrpc":"2.0","id":23,"method":"mission.runtime.prepare","params":{"enabled":true}}"#,
            )
            .result
            .is_some()
        );
        let mut forged = lease;
        forged.core_instance_nonce = "ff".repeat(32);
        assert_eq!(
            request(
                &mut host,
                &serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": 24,
                    "method": "broker.lease.install",
                    "params": {"lease": forged}
                })
                .to_string(),
            )
            .error
            .unwrap()
            .code,
            -32_015
        );
    }

    #[test]
    #[allow(clippy::too_many_lines)] // One end-to-end proof-consumption/child-lifecycle route.
    fn imessage_chat_discovery_requires_two_fresh_proofs_and_clears_the_child() {
        let (root, mut host) = fixture();
        host.paths.imsg_runtime = fake_imsg_runtime(root.path());
        host.store
            .install_trusted_broker(&broker_record(&host))
            .unwrap();
        let broker_key = SigningKey::from_bytes(&[41_u8; 32]);
        let mut lease = CoreInstanceLease {
            protocol_version: EFFECT_PROTOCOL_VERSION,
            audit_euid: rustix::process::geteuid().as_raw(),
            app_pid: 1,
            app_start_time_us: 1,
            core_pid: i32::try_from(std::process::id()).unwrap(),
            core_start_time_us: 1,
            core_audit_token_hex: "aa".repeat(32),
            codex_pid: 42,
            codex_start_time_us: 2,
            codex_audit_token_hex: "bb".repeat(32),
            core_instance_nonce: host.instance_nonce.clone(),
            issued_at_ms: super::now_ms().unwrap(),
            broker_key_id: format!(
                "{:x}",
                Sha256::digest(broker_key.verifying_key().to_bytes())
            ),
            broker_signature_hex: String::new(),
        };
        lease.broker_signature_hex = hex::encode(
            broker_key
                .sign(&core_instance_lease_signing_bytes(&lease).unwrap())
                .to_bytes(),
        );
        *host
            .operations
            .codex_pid
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(42);
        assert_eq!(
            request(
                &mut host,
                &json!({
                    "jsonrpc": "2.0",
                    "id": 500,
                    "method": "broker.lease.install",
                    "params": {"lease": lease}
                })
                .to_string(),
            )
            .result
            .unwrap()["status"],
            "installed"
        );
        let on = host.store.prepare_runtime_control(true, 1).unwrap();
        host.store
            .commit_runtime_control(&on, &broker_receipt(&on, None))
            .unwrap();
        host.operations.accept_committed_runtime(true, on.revision);

        let challenge = request(
            &mut host,
            r#"{"jsonrpc":"2.0","id":501,"method":"mission.runtime.challenge","params":{}}"#,
        )
        .result
        .unwrap()["challenge"]
            .as_str()
            .unwrap()
            .to_owned();
        let prepare_request = json!({
            "jsonrpc": "2.0",
            "id": 502,
            "method": "channel.imessage.chats.prepare",
            "params": {
                "authorization": on,
                "brokerReceipt": broker_receipt(&on, Some(&challenge)),
            }
        })
        .to_string();
        let prepared = request(&mut host, &prepare_request);
        assert!(prepared.error.is_none(), "{prepared:?}");
        assert!(
            prepared.result.unwrap()["processIdentifier"]
                .as_u64()
                .is_some_and(|pid| pid > 0)
        );
        assert_eq!(
            request(&mut host, &prepare_request).error.unwrap().code,
            -32_602
        );

        let challenge = request(
            &mut host,
            r#"{"jsonrpc":"2.0","id":503,"method":"mission.runtime.challenge","params":{}}"#,
        )
        .result
        .unwrap()["challenge"]
            .as_str()
            .unwrap()
            .to_owned();
        let listed = request(
            &mut host,
            &json!({
                "jsonrpc": "2.0",
                "id": 504,
                "method": "channel.imessage.chats.list",
                "params": {
                    "authorization": on,
                    "brokerReceipt": broker_receipt(&on, Some(&challenge)),
                }
            })
            .to_string(),
        )
        .result
        .unwrap();
        assert_eq!(
            listed,
            json!({
                "chats": [{
                    "chatId": "42",
                    "chatGuid": "iMessage;+;owner",
                    "chatIdentifier": "owner",
                    "name": "Owner",
                    "service": "iMessage",
                    "participants": ["+15550000001"],
                }]
            })
        );
        assert_eq!(
            host.channels
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .status(ChannelKind::IMessage),
            ChannelConnectionStatus::Disconnected
        );

        let challenge = request(
            &mut host,
            r#"{"jsonrpc":"2.0","id":505,"method":"mission.runtime.challenge","params":{}}"#,
        )
        .result
        .unwrap()["challenge"]
            .as_str()
            .unwrap()
            .to_owned();
        assert!(
            request(
                &mut host,
                &json!({
                    "jsonrpc": "2.0",
                    "id": 506,
                    "method": "channel.imessage.chats.list",
                    "params": {
                        "authorization": on,
                        "brokerReceipt": broker_receipt(&on, Some(&challenge)),
                    }
                })
                .to_string(),
            )
            .error
            .is_some()
        );
    }

    #[test]
    fn preparing_off_invalidates_an_outstanding_runtime_challenge() {
        let (_root, mut host) = fixture();
        assert!(
            request(
                &mut host,
                r#"{"jsonrpc":"2.0","id":30,"method":"mission.runtime.challenge","params":{}}"#,
            )
            .result
            .is_some()
        );
        assert!(host.operations.runtime_challenge.lock().unwrap().is_some());
        assert!(
            request(
                &mut host,
                r#"{"jsonrpc":"2.0","id":31,"method":"mission.runtime.prepare","params":{"enabled":false}}"#,
            )
            .result
            .is_some()
        );
        assert!(host.operations.runtime_challenge.lock().unwrap().is_none());
    }

    #[test]
    fn replacement_host_requires_exact_broker_reenrollment_before_preparing_off() {
        let (_root, mut host) = fixture();
        let broker = broker_record(&host);
        host.store.install_trusted_broker(&broker).unwrap();
        let on = host.store.prepare_runtime_control(true, 1).unwrap();
        host.store
            .commit_runtime_control(&on, &broker_receipt(&on, None))
            .unwrap();
        let off = host.store.prepare_runtime_control(false, 2).unwrap();
        host.store
            .recover_runtime_control(&off, &broker_receipt(&off, None))
            .unwrap();
        let paths = host.paths.clone();
        drop(host);

        let mut replacement = Host::open(paths, [7_u8; 32]).unwrap();
        let rejected = request(
            &mut replacement,
            r#"{"jsonrpc":"2.0","id":32,"method":"mission.runtime.prepare","params":{"enabled":false}}"#,
        );
        assert_eq!(rejected.error.unwrap().code, -32_000);

        replacement.store.install_trusted_broker(&broker).unwrap();
        let prepared = request(
            &mut replacement,
            r#"{"jsonrpc":"2.0","id":33,"method":"mission.runtime.prepare","params":{"enabled":false}}"#,
        )
        .result
        .unwrap();
        assert_eq!(prepared["enabled"], false);
        assert_eq!(prepared["revision"], 3);
        assert_eq!(replacement.store.runtime_control().unwrap().revision, 2);
    }

    #[test]
    fn rolled_back_runtime_prefix_blocks_every_model_route_with_live_off_checkpoint() {
        let (_root, mut host) = fixture();
        let enrollment = broker_record(&host);
        host.store.install_trusted_broker(&enrollment).unwrap();

        let on = host.store.prepare_runtime_control(true, 1).unwrap();
        let on_receipt = broker_receipt(&on, None);
        host.store.commit_runtime_control(&on, &on_receipt).unwrap();
        let off = host.store.prepare_runtime_control(false, 2).unwrap();
        let off_receipt = broker_receipt(&off, None);
        host.store
            .commit_runtime_control(&off, &off_receipt)
            .unwrap();

        let connection = Connection::open(&host.paths.store).unwrap();
        connection
            .execute("DELETE FROM runtime_control_history WHERE revision = 2", [])
            .unwrap();
        connection
            .execute(
                "UPDATE runtime_control SET enabled = ?1, revision = ?2,
                        updated_at_ms = ?3, signature_hex = ?4 WHERE singleton_id = 1",
                rusqlite::params![
                    i64::from(on.enabled),
                    i64::try_from(on.revision).unwrap(),
                    on.updated_at_ms,
                    on.authorization_signature_hex,
                ],
            )
            .unwrap();
        drop(connection);

        for (id, method, mut extra) in [
            (101, "account.read", serde_json::json!({})),
            (102, "models.list", serde_json::json!({})),
        ] {
            let challenge = request(
                &mut host,
                &serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id + 1_000,
                    "method": "mission.runtime.challenge",
                    "params": {}
                })
                .to_string(),
            )
            .result
            .unwrap()["challenge"]
                .as_str()
                .unwrap()
                .to_owned();
            let receipt = broker_receipt(&off, Some(&challenge));
            let object = extra.as_object_mut().unwrap();
            object.insert("authorization".into(), serde_json::to_value(&off).unwrap());
            object.insert(
                "brokerReceipt".into(),
                serde_json::to_value(receipt).unwrap(),
            );
            let response = request(
                &mut host,
                &serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "method": method,
                    "params": extra
                })
                .to_string(),
            );
            assert!(response.error.is_some(), "{method} must fail closed");
        }
        assert!(
            std::fs::read_dir(&host.paths.model_input_root)
                .unwrap()
                .next()
                .is_none()
        );
    }

    #[test]
    fn route_bind_rpc_adds_only_the_exact_durable_pairing_to_the_same_mission() {
        let (_root, mut host) = fixture();
        host.store
            .install_trusted_broker(&broker_record(&host))
            .unwrap();
        let on = host.store.prepare_runtime_control(true, 1).unwrap();
        let receipt = broker_receipt(&on, None);
        host.store.commit_runtime_control(&on, &receipt).unwrap();
        host.operations.accept_committed_runtime(true, on.revision);
        let mission_id = seed_primary_discord_mission(&mut host);

        host.store
            .pair_channel(&ChannelPairing {
                channel: ChannelKind::IMessage,
                owner_sender_id: "imessage-owner".into(),
                conversation_id: "42".into(),
                require_explicit_address: true,
                imessage: None,
                discord: None,
                paired_at_ms: 4,
            })
            .unwrap();
        let approval = json!({
            "approvalId": "route-approval-host-imessage",
            "missionId": mission_id,
            "expectedRouteSetRevision": 1,
            "channel": "iMessage",
            "conversationId": "42",
            "ownerSenderId": "imessage-owner",
            "providerIdentity": null,
            "allowedInboundClasses": ["missionParticipation", "needYouResponse"],
            "allowedOutboundClasses": [],
            "actorId": super::ISSUER_ID,
            "decision": ChannelRouteApprovalDecision::Approve,
            "decidedAtMs": 5,
        });
        let response = request(
            &mut host,
            &json!({
                "jsonrpc": "2.0",
                "id": 600,
                "method": "channel.route.bind",
                "params": {
                    "approval": approval,
                    "authorization": on,
                    "brokerReceipt": receipt,
                }
            })
            .to_string(),
        );
        assert!(response.error.is_none(), "{response:?}");
        let route_set = response.result.unwrap();
        assert_eq!(route_set["missionId"], mission_id);
        assert_eq!(route_set["revision"], 2);
        assert_eq!(route_set["routes"].as_array().unwrap().len(), 2);
        assert_eq!(route_set["routes"][1]["channel"], "iMessage");
        assert_eq!(route_set["routes"][1]["role"], "additional");
        assert_eq!(route_set["routes"][1]["allowedOutboundClasses"], json!([]));

        let dashboard = request(
            &mut host,
            r#"{"jsonrpc":"2.0","id":601,"method":"mission.dashboard.read","params":{}}"#,
        )
        .result
        .unwrap();
        assert_eq!(dashboard["channelRouteSet"], route_set);
        assert!(dashboard.get("channelOrigin").is_none());
    }

    #[test]
    fn outbound_response_loss_after_unrelated_route_append_never_calls_provider_send_twice() {
        let (_root, mut host) = fixture();
        host.store
            .install_trusted_broker(&broker_record(&host))
            .unwrap();
        let on = host.store.prepare_runtime_control(true, 1).unwrap();
        let receipt = broker_receipt(&on, None);
        host.store.commit_runtime_control(&on, &receipt).unwrap();
        host.operations.accept_committed_runtime(true, on.revision);
        let mission_id = seed_primary_discord_mission(&mut host);
        let route_set = host.store.channel_route_set(&mission_id).unwrap().unwrap();
        let primary = route_set
            .routes
            .iter()
            .find(|route| route.route_id == route_set.primary_route_id)
            .unwrap()
            .clone();
        let (sends, recoveries) = host
            .channels
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .install_test_send_probe(ChannelKind::Discord);
        let content = "Mission progress is ready.";
        let first = SendChannelMessage {
            mission_id: mission_id.clone(),
            route_id: primary.route_id.clone(),
            kind: ChannelMessageKind::Progress,
            content: content.into(),
            approved_at_ms: super::now_ms().unwrap(),
            authorization: on.clone(),
            broker_receipt: receipt.clone(),
        };
        let (started, message_body, handle) = host.prepare_channel_send(&first).unwrap();
        assert_eq!(started.disposition, ChannelOutboundDisposition::ExecuteNow);
        assert_eq!(
            handle.send(&started.intent.outbound_id, &message_body),
            ChannelSendResult::Uncertain
        );
        assert_eq!(sends.load(Ordering::Acquire), 1);

        host.store
            .pair_channel(&ChannelPairing {
                channel: ChannelKind::IMessage,
                owner_sender_id: "imessage-owner".into(),
                conversation_id: "42".into(),
                require_explicit_address: true,
                imessage: None,
                discord: None,
                paired_at_ms: first.approved_at_ms + 1,
            })
            .unwrap();
        let appended = host
            .store
            .bind_additional_channel_route(&ChannelRouteApproval {
                approval_id: "append-unrelated-route-after-response-loss".into(),
                mission_id: mission_id.clone(),
                expected_route_set_revision: route_set.revision,
                channel: ChannelKind::IMessage,
                conversation_id: "42".into(),
                owner_sender_id: "imessage-owner".into(),
                provider_identity: None,
                allowed_inbound_classes: vec![ChannelInboundMessageClass::MissionParticipation],
                allowed_outbound_classes: Vec::new(),
                actor_id: super::ISSUER_ID.into(),
                decision: ChannelRouteApprovalDecision::Approve,
                decided_at_ms: first.approved_at_ms + 1,
            })
            .unwrap();
        assert_eq!(appended.revision, route_set.revision + 1);

        let retry = SendChannelMessage {
            approved_at_ms: first.approved_at_ms + 2,
            ..first
        };
        let (recovered, recovered_body, recovered_handle) =
            host.prepare_channel_send(&retry).unwrap();
        assert_eq!(
            recovered.disposition,
            ChannelOutboundDisposition::RecoverOnly
        );
        assert_eq!(recovered.intent, started.intent);
        let cursor = recovered.intent.recovery_cursor.as_ref().unwrap();
        assert_eq!(
            recovered_handle.recover(&recovered.intent.outbound_id, &recovered_body, cursor,),
            ChannelSendResult::Uncertain
        );
        assert_eq!(sends.load(Ordering::Acquire), 1);
        assert_eq!(recoveries.load(Ordering::Acquire), 1);
    }

    #[test]
    fn dashboard_focus_pins_need_you_confirmed_card_and_route_then_reveals_hidden_work() {
        let (_root, mut host) = fixture();
        host.store
            .install_trusted_broker(&broker_record(&host))
            .unwrap();
        let on = host.store.prepare_runtime_control(true, 1).unwrap();
        host.store
            .commit_runtime_control(&on, &broker_receipt(&on, None))
            .unwrap();
        host.operations.accept_committed_runtime(true, on.revision);
        let routed_mission_id = seed_primary_discord_mission(&mut host);

        let mut other_ids = Vec::new();
        for (index, suffix) in [
            "11111111111111111111111111111111",
            "22222222222222222222222222222222",
            "33333333333333333333333333333333",
        ]
        .into_iter()
        .enumerate()
        {
            let suggestion =
                hero_suggestion_with_id(&format!("suggestion-170000000000{}-{suffix}", index + 1));
            let confirmed = host
                .persist_confirmed_mission(
                    &suggestion,
                    &ReminderTarget {
                        source_identifier: "source-1".into(),
                        calendar_identifier: "calendar-1".into(),
                    },
                    super::now_ms().unwrap(),
                )
                .unwrap();
            other_ids.push(confirmed.mission_id);
        }
        let anchor = host.store.current_verified_audit_anchor().unwrap().unwrap();
        host.store
            .execute_mission_command_batch(
                &mission_command_batch(
                    Some(&anchor),
                    &routed_mission_id,
                    vec![MissionCommand::RequestScopeChange {
                        mission_id: routed_mission_id.clone(),
                        approval: NewBoundaryApproval {
                            id: "dashboard-focus-boundary".into(),
                            kind: ApprovalKind::ExpandedScope,
                            prompt: "Approve the exact bounded change.".into(),
                            scope_digest: "dashboard-focus-scope".into(),
                            target: None,
                        },
                        needs_me_id: "dashboard-focus-needs-you".into(),
                        now_ms: super::now_ms().unwrap(),
                    }],
                )
                .unwrap(),
            )
            .unwrap();

        let initial_dashboard = dashboard(&mut host, 620);
        let cards = initial_dashboard["activeCards"].as_array().unwrap();
        assert_eq!(cards.len(), 3);
        assert_eq!(cards[0]["id"], routed_mission_id);
        assert_eq!(cards[0]["state"], "Need you");
        assert!(initial_dashboard["confirmedMission"].is_null());
        assert_eq!(initial_dashboard["needsYou"]["missionId"], cards[0]["id"]);
        assert_eq!(
            initial_dashboard["channelRouteSet"]["missionId"],
            cards[0]["id"]
        );
        let visible_before = cards
            .iter()
            .filter_map(|card| card["id"].as_str())
            .collect::<HashSet<_>>();
        let hidden_id = other_ids
            .iter()
            .find(|mission_id| !visible_before.contains(mission_id.as_str()))
            .unwrap();

        let anchor = host.store.current_verified_audit_anchor().unwrap().unwrap();
        host.store
            .execute_mission_command_batch(
                &mission_command_batch(
                    Some(&anchor),
                    &routed_mission_id,
                    vec![MissionCommand::Cancel {
                        mission_id: routed_mission_id.clone(),
                        now_ms: super::now_ms().unwrap(),
                    }],
                )
                .unwrap(),
            )
            .unwrap();
        let after_cancel = dashboard(&mut host, 621);
        let cards = after_cancel["activeCards"].as_array().unwrap();
        assert_eq!(cards.len(), 3);
        assert!(cards.iter().all(|card| card["id"] != routed_mission_id));
        assert!(cards.iter().any(|card| card["id"] == hidden_id.as_str()));
        let focused = after_cancel["confirmedMission"]["missionId"]
            .as_str()
            .unwrap();
        assert!(cards.iter().any(|card| card["id"] == focused));
        assert!(after_cancel["needsYou"].is_null());
        assert!(after_cancel["channelRouteSet"].is_null());
    }

    #[test]
    fn poll_contract_requires_an_explicit_model_work_capability() {
        let (_root, mut host) = fixture();
        host.store
            .install_trusted_broker(&broker_record(&host))
            .unwrap();
        let authorization = host.store.prepare_runtime_control(true, 1).unwrap();
        let receipt = broker_receipt(&authorization, None);
        let exact = json!({
            "jsonrpc": "2.0",
            "id": 698,
            "method": "channel.poll",
            "params": {
                "channel": ChannelKind::IMessage,
                "modelWorkAllowed": false,
                "authorization": authorization,
                "brokerReceipt": receipt,
            }
        });
        let request: RpcRequest = serde_json::from_value(exact.clone()).unwrap();
        let decoded = decode_params::<PollChannel>(&request).unwrap();
        assert!(!decoded.model_work_allowed);

        let mut missing = exact.clone();
        missing["params"]
            .as_object_mut()
            .unwrap()
            .remove("modelWorkAllowed");
        let request: RpcRequest = serde_json::from_value(missing).unwrap();
        assert!(decode_params::<PollChannel>(&request).is_err());

        let mut unknown = exact;
        unknown["params"]
            .as_object_mut()
            .unwrap()
            .insert("allowModelFallback".into(), json!(true));
        let request: RpcRequest = serde_json::from_value(unknown).unwrap();
        assert!(decode_params::<PollChannel>(&request).is_err());
    }

    #[test]
    fn model_forbidden_self_chat_poll_keeps_intake_unconsumed_without_choice_work() {
        let (_root, mut host) = fixture();
        host.store
            .install_trusted_broker(&broker_record(&host))
            .unwrap();
        let on = host.store.prepare_runtime_control(true, 1).unwrap();
        let receipt = broker_receipt(&on, None);
        host.store.commit_runtime_control(&on, &receipt).unwrap();
        host.operations.accept_committed_runtime(true, on.revision);
        host.store
            .pair_channel(&ChannelPairing {
                channel: ChannelKind::IMessage,
                owner_sender_id: "owner@example.invalid".into(),
                conversation_id: "42".into(),
                require_explicit_address: true,
                imessage: None,
                discord: None,
                paired_at_ms: 1,
            })
            .unwrap();

        let params = PollChannel {
            channel: ChannelKind::IMessage,
            model_work_allowed: false,
            authorization: on,
            broker_receipt: receipt,
        };
        let inbound = TransportInbound {
            channel: ChannelKind::IMessage,
            source_message_id: "imessage-awaiting-account".into(),
            sender_id: "owner@example.invalid".into(),
            conversation_id: "42".into(),
            content: "Keep this queued until managed ChatGPT is ready.".into(),
            cursor_opaque_value: "imessage-awaiting-account-cursor".into(),
            cursor_order: 1,
            received_at_ms: 2,
        };
        let request: RpcRequest = serde_json::from_value(json!({
            "jsonrpc": "2.0",
            "id": 699,
            "method": "channel.poll",
            "params": {}
        }))
        .unwrap();
        let operation = host.operations.begin_operation().unwrap();
        let (send, receive) = mpsc::sync_channel(1);
        host.process_channel_inbound(&request, &send, &params, operation, &inbound);
        let response = receive.recv().unwrap();
        assert!(response.result.is_none());
        assert!(response.error.is_some());
        assert!(host.store.choice_loop_snapshot().unwrap().is_none());
        assert!(
            host.store
                .started_channel_model(ChannelKind::IMessage)
                .unwrap()
                .is_none(),
            "a model-forbidden self-chat poll cannot start a Choice or legacy dispatch"
        );
        assert!(
            host.operations
                .suggestion
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .is_none()
        );
    }

    #[test]
    fn self_chat_poll_creates_one_host_owned_choice_and_exact_replay_advances_no_revision() {
        let (_root, mut host) = fixture();
        host.store
            .install_trusted_broker(&broker_record(&host))
            .unwrap();
        let on = host.store.prepare_runtime_control(true, 1).unwrap();
        let receipt = broker_receipt(&on, None);
        host.store.commit_runtime_control(&on, &receipt).unwrap();
        host.operations.accept_committed_runtime(true, on.revision);
        let selection = ModelSelection {
            id: "model-selection-imessage".into(),
            model_id: "gpt-test-model".into(),
            requested_effort: "high".into(),
            actual_effort: "high".into(),
            catalog_fingerprint: "a".repeat(64),
            catalog_revision: 7,
            account_display_class: "chatgpt:plus".into(),
            protocol_schema_revision: 1,
        };
        host.store.select_model_selection(&selection, 2).unwrap();
        *host.operations.model_catalog_snapshot.lock().unwrap() = Some(
            test_model_catalog_snapshot(on.revision, super::now_ms().unwrap()),
        );
        host.store
            .pair_channel(&ChannelPairing {
                channel: ChannelKind::IMessage,
                owner_sender_id: "owner@example.invalid".into(),
                conversation_id: "42".into(),
                require_explicit_address: false,
                imessage: Some(IMessagePairingMetadata {
                    chat_guid: "iMessage;+;self".into(),
                    chat_identifier: "self".into(),
                    service: "iMessage".into(),
                    participant_ids: vec!["owner@example.invalid".into()],
                }),
                discord: None,
                paired_at_ms: 1,
            })
            .unwrap();
        let params = PollChannel {
            channel: ChannelKind::IMessage,
            model_work_allowed: true,
            authorization: on,
            broker_receipt: receipt,
        };
        let inbound = TransportInbound {
            channel: ChannelKind::IMessage,
            source_message_id: "imessage-choice-1".into(),
            sender_id: "owner@example.invalid".into(),
            conversation_id: "42".into(),
            content: "Help me prepare tomorrow morning.".into(),
            cursor_opaque_value: "101".into(),
            cursor_order: 101,
            received_at_ms: 2,
        };
        let request: RpcRequest = serde_json::from_value(json!({
            "jsonrpc": "2.0", "id": 700, "method": "channel.poll", "params": {}
        }))
        .unwrap();
        let operation = host.operations.begin_operation().unwrap();
        let (send, receive) = mpsc::sync_channel(1);
        host.process_channel_inbound(&request, &send, &params, operation, &inbound);
        let response = receive.recv().unwrap();
        assert!(response.error.is_none(), "{response:?}");
        assert_eq!(response.result.unwrap()["eventStatus"], "deferred");
        let first = host.store.choice_loop_snapshot().unwrap().unwrap();
        assert_eq!(first.session.revision, 1);
        assert_eq!(
            first.active_batch.as_ref().unwrap().delivery_binding_id,
            first.session.primary_delivery_binding_id.clone().unwrap()
        );
        assert_eq!(
            host.store
                .channel_cursor(ChannelKind::IMessage, "42")
                .unwrap()
                .unwrap()
                .order,
            101
        );
        for _ in 0..100 {
            if host.operations.gate.lock().unwrap().active.is_none() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let durable_before_replay = host.store.choice_loop_snapshot().unwrap().unwrap();
        let operation = host.operations.begin_operation().unwrap();
        let (send, receive) = mpsc::sync_channel(1);
        host.process_channel_inbound(&request, &send, &params, operation, &inbound);
        let response = receive.recv().unwrap();
        assert!(response.error.is_none(), "{response:?}");
        assert_eq!(response.result.unwrap()["eventStatus"], "deferred");
        assert_eq!(
            host.store
                .choice_loop_snapshot()
                .unwrap()
                .unwrap()
                .session
                .revision,
            durable_before_replay.session.revision
        );
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn choice_imessage_reply_consumes_one_exact_authority_and_echo_is_guid_bound() {
        let (_root, mut host) = fixture();
        host.store
            .install_trusted_broker(&broker_record(&host))
            .unwrap();
        let on = host.store.prepare_runtime_control(true, 1).unwrap();
        host.store
            .commit_runtime_control(&on, &broker_receipt(&on, None))
            .unwrap();
        let selection = ModelSelection {
            id: "model-selection-imessage-reply".into(),
            model_id: "gpt-test-model".into(),
            requested_effort: "high".into(),
            actual_effort: "high".into(),
            catalog_fingerprint: "a".repeat(64),
            catalog_revision: 7,
            account_display_class: "chatgpt:plus".into(),
            protocol_schema_revision: 1,
        };
        host.store.select_model_selection(&selection, 2).unwrap();
        let pairing = ChannelPairing {
            channel: ChannelKind::IMessage,
            owner_sender_id: "owner@example.invalid".into(),
            conversation_id: "42".into(),
            require_explicit_address: false,
            imessage: Some(IMessagePairingMetadata {
                chat_guid: "iMessage;+;self".into(),
                chat_identifier: "self".into(),
                service: "iMessage".into(),
                participant_ids: vec!["owner@example.invalid".into()],
            }),
            discord: None,
            paired_at_ms: 1,
        };
        host.store.pair_channel(&pairing).unwrap();
        let input = ChoiceBeginRequest {
            request_id: "choice-imessage-reply-begin".into(),
            bounded_local_question: "Prepare the self-chat demo".into(),
            expected_model_provenance_ref: selection.id.clone(),
            expected_catalog_fingerprint: selection.catalog_fingerprint.clone(),
            expected_catalog_revision: selection.catalog_revision,
            expected_protocol_revision: selection.protocol_schema_revision,
        };
        let (record, initial) = new_choice_begin_state_for_source(
            &input,
            &selection,
            1,
            on.revision,
            &persona_revision(),
            10,
            "imessage-self-chat",
            "self-chat-binding".into(),
            Some("self-chat-source-guid".into()),
        )
        .unwrap();
        let cursor = ChannelCursor {
            channel: ChannelKind::IMessage,
            conversation_id: "42".into(),
            opaque_value: "101".into(),
            order: 101,
            observed_at_ms: 10,
        };
        host.store
            .begin_imessage_choice_session_with_clock(
                &record,
                &initial,
                &ChoiceIdleClockEvidence {
                    boot_id: "reply-test-boot".into(),
                    wall_clock_ms: 10,
                    monotonic_ms: 10,
                },
                &cursor,
            )
            .unwrap();
        let generated = StructuredChoiceGeneration {
            understood_goal: "Prepare the self-chat demo".into(),
            current_context: "One private inbound is accepted.".into(),
            assumptions: vec![],
            constraints: vec![],
            uncertainties: vec![],
            what_to_avoid: vec![],
            options: ["Review", "Narrow", "Rehearse"]
                .map(|direction| StructuredChoiceOption {
                    direction: direction.into(),
                    rationale: "Keep the next action bounded.".into(),
                    expected_result: "One clear next step.".into(),
                    information_needed: vec![],
                    external_effects_preview: vec![],
                    source_categories: vec!["ownerInput".into()],
                })
                .to_vec(),
            source_refs: vec![],
        };
        let active = host
            .store
            .commit_initial_choice_result(
                &initial_choice_result_from_generation(&record, generated).unwrap(),
            )
            .unwrap();
        let choice_set = active.active_choice_set.as_ref().unwrap();
        let visible_body = render_choice_imessage_reply(choice_set).unwrap();
        let mut intent = ChoiceIMessageReplyIntent {
            preview: ChoiceIMessageReplyPreview {
                reply_id: "choice-imessage-reply-1".into(),
                preview_revision: active.session.revision,
                destination: "Your selected iMessage self-chat".into(),
                visible_body: visible_body.clone(),
                confirmation_digest: "0".repeat(64),
            },
            outbound_id: "choice-imessage-outbound-1".into(),
            choice_session_id: active.session.id.clone(),
            session_revision: active.session.revision,
            choice_set_id: choice_set.id.clone(),
            choice_set_digest: canonical_choice_set_digest(choice_set).unwrap(),
            source_message_id: "self-chat-source-guid".into(),
            delivery_binding_id: "self-chat-binding".into(),
            pairing,
            persona_revision: choice_set.persona_revision.clone(),
            source_manifest_digest: choice_set.source_manifest_digest.clone(),
            model_provenance: choice_set.model_provenance.clone(),
            canonical_payload_sha256: format!("{:x}", Sha256::digest(visible_body.as_bytes())),
            created_at_ms: 20,
            approved_at_ms: None,
            recovery_cursor: None,
        };
        intent.preview.confirmation_digest = intent.expected_confirmation_digest().unwrap();
        assert!(intent.is_valid(), "{intent:#?}");
        assert_eq!(
            host.store.prepare_choice_imessage_reply(&intent).unwrap(),
            (intent.preview.clone(), "prepared".to_owned())
        );
        let first = host
            .store
            .authorize_choice_imessage_reply(
                &intent.preview.reply_id,
                intent.preview.preview_revision,
                &intent.preview.confirmation_digest,
                true,
                21,
            )
            .unwrap();
        assert_eq!(
            first.disposition,
            ChoiceIMessageReplyDisposition::ExecuteNow
        );
        let replay = host
            .store
            .authorize_choice_imessage_reply(
                &intent.preview.reply_id,
                intent.preview.preview_revision,
                &intent.preview.confirmation_digest,
                true,
                22,
            )
            .unwrap();
        assert_eq!(
            replay.disposition,
            ChoiceIMessageReplyDisposition::RecoverOnly
        );
        host.store
            .record_choice_imessage_reply_delivery(&intent.preview.reply_id, "provider-guid-1", 23)
            .unwrap();
        assert!(
            host.store
                .verify_choice_imessage_reply_echo("42", "provider-guid-1")
                .unwrap()
        );
        assert!(
            !host
                .store
                .verify_choice_imessage_reply_echo("43", "provider-guid-1")
                .unwrap()
        );
        let delivered = host
            .store
            .authorize_choice_imessage_reply(
                &intent.preview.reply_id,
                intent.preview.preview_revision,
                &intent.preview.confirmation_digest,
                true,
                24,
            )
            .unwrap();
        assert_eq!(
            delivered.disposition,
            ChoiceIMessageReplyDisposition::AlreadySent
        );
        assert!(
            host.store
                .authorize_choice_imessage_reply(
                    &intent.preview.reply_id,
                    intent.preview.preview_revision,
                    &"f".repeat(64),
                    true,
                    25,
                )
                .is_err()
        );
    }

    #[test]
    #[allow(clippy::too_many_lines)] // Chronological two-message recovery proof.
    fn discord_recovery_defers_model_until_the_final_high_water_cursor() {
        let (_root, mut host) = fixture();
        host.store
            .install_trusted_broker(&broker_record(&host))
            .unwrap();
        let on = host.store.prepare_runtime_control(true, 1).unwrap();
        let receipt = broker_receipt(&on, None);
        host.store.commit_runtime_control(&on, &receipt).unwrap();
        host.operations.accept_committed_runtime(true, on.revision);
        host.store
            .pair_channel(&ChannelPairing {
                channel: ChannelKind::Discord,
                owner_sender_id: "1001".into(),
                conversation_id: "2002".into(),
                require_explicit_address: true,
                imessage: None,
                discord: Some(openopen_protocol::DiscordPairingMetadata {
                    guild_id: "3003".into(),
                    bot_user_id: "4004".into(),
                    application_id: "5005".into(),
                    setup_source_message_id: "6006".into(),
                    setup_candidate_id: format!("discord-pair-{}", "a".repeat(64)),
                }),
                paired_at_ms: 1,
            })
            .unwrap();

        let recovery = host
            .channels
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .install_test_discord_recovery();
        recovery
            .send(Ok(RecoveryBatch {
                envelopes: vec![
                    DiscordInbound {
                        source_message_id: "9001".into(),
                        sender_id: "1001".into(),
                        conversation_id: "2002".into(),
                        content: "prepare the first draft".into(),
                        received_at_ms: 10,
                    },
                    DiscordInbound {
                        source_message_id: "9002".into(),
                        sender_id: "1001".into(),
                        conversation_id: "2002".into(),
                        content: "Correction to previous: prepare only the revised draft".into(),
                        received_at_ms: 11,
                    },
                ],
                high_water_message_id: "9003".into(),
                pages_fetched: 1,
            }))
            .unwrap();

        let params = PollChannel {
            channel: ChannelKind::Discord,
            model_work_allowed: false,
            authorization: on,
            broker_receipt: receipt,
        };
        for (request_id, expected_source) in [(700, "9001"), (701, "9002")] {
            let event = host
                .channels
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .poll(ChannelKind::Discord, 42)
                .unwrap()
                .unwrap();
            let TransportEvent::Inbound(inbound) = event else {
                panic!("recovery must preserve both inbound events before its cursor");
            };
            assert_eq!(inbound.source_message_id, expected_source);
            let request: RpcRequest = serde_json::from_value(json!({
                "jsonrpc": "2.0",
                "id": request_id,
                "method": "channel.poll",
                "params": {}
            }))
            .unwrap();
            let operation = host.operations.begin_operation().unwrap();
            let (send, receive) = mpsc::sync_channel(1);
            host.process_channel_inbound(&request, &send, &params, operation, &inbound);
            let response = receive.recv().unwrap().result.unwrap();
            assert_eq!(response["eventStatus"], "recovering");
            assert!(response["suggestion"].is_null());
            assert_eq!(
                host.next_ready_channel_model(ChannelKind::Discord, 42)
                    .unwrap(),
                None
            );
        }

        assert_eq!(
            host.store
                .next_queued_channel_model(ChannelKind::Discord)
                .unwrap()
                .as_deref(),
            Some("9001")
        );
        assert!(
            host.operations
                .suggestion
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .is_none()
        );

        let cursor = host
            .channels
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .poll(ChannelKind::Discord, 42)
            .unwrap()
            .unwrap();
        let TransportEvent::Cursor(cursor) = cursor else {
            panic!("the final recovery event must be the provider high-water cursor");
        };
        host.store.advance_channel_cursor(&cursor).unwrap();
        host.channels
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .acknowledge_recovery(&TransportEvent::Cursor(cursor))
            .unwrap();
        assert_eq!(
            host.next_ready_channel_model(ChannelKind::Discord, 42)
                .unwrap(),
            None,
            "a closed recovery cursor cannot grant model work while Discord is not Connected"
        );
        assert_eq!(
            host.store
                .next_queued_channel_model(ChannelKind::Discord)
                .unwrap()
                .as_deref(),
            Some("9001"),
            "the durable dispatch must remain queued without being begun"
        );
        host.channels
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .mark_test_discord_connected();
        assert_eq!(
            host.next_ready_channel_model(ChannelKind::Discord, 42)
                .unwrap()
                .as_deref(),
            Some("9001"),
            "the exact queued dispatch may begin only after the adapter is Connected"
        );

        let original = OutcomeSuggestion {
            id: "suggestion-10-00000000000000000000000000000001".into(),
            title: "Prepare the original draft".into(),
            why_now: "The first recovered message requested it.".into(),
            proposed_steps: vec!["Draft the original".into()],
            source_refs: vec!["channel:original".into()],
        };
        assert_eq!(
            host.store
                .begin_channel_model(ChannelKind::Discord, "9001")
                .unwrap()
                .disposition,
            ChannelModelDisposition::ExecuteNow
        );
        host.store
            .record_channel_suggestion(ChannelKind::Discord, "9001", &original, 12)
            .unwrap();
        *host
            .operations
            .suggestion
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(original.clone());
        assert!(
            host.store
                .channel_model_work_pending(ChannelKind::Discord)
                .unwrap()
        );
        assert!(!host.channel_suggestion_is_current(&original).unwrap());
        let dashboard = request(
            &mut host,
            r#"{"jsonrpc":"2.0","id":702,"method":"mission.dashboard.read","params":{}}"#,
        )
        .result
        .unwrap();
        assert!(dashboard["suggestion"].is_null());
        assert!(
            request(
                &mut host,
                r#"{"jsonrpc":"2.0","id":703,"method":"mission.confirm","params":{"suggestionId":"suggestion-10-00000000000000000000000000000001","reminderTarget":{"sourceIdentifier":"source-1","calendarIdentifier":"calendar-1"}}}"#,
            )
            .error
            .is_some(),
            "a recovered original must not confirm while its correction is queued"
        );

        let correction = OutcomeSuggestion {
            id: "suggestion-11-00000000000000000000000000000002".into(),
            title: "Prepare only the revised draft".into(),
            why_now: "The later recovered correction supersedes the original.".into(),
            proposed_steps: vec!["Draft only the revision".into()],
            source_refs: vec!["channel:correction".into()],
        };
        assert_eq!(
            host.store
                .begin_channel_model(ChannelKind::Discord, "9002")
                .unwrap()
                .disposition,
            ChannelModelDisposition::ExecuteNow
        );
        let correction_context = host
            .store
            .channel_model_context(ChannelKind::Discord, "9002")
            .unwrap();
        assert_eq!(
            correction_context
                .iter()
                .map(|(envelope, content)| (envelope.source_message_id.as_str(), content.as_str()))
                .collect::<Vec<_>>(),
            vec![
                ("9001", "prepare the first draft"),
                (
                    "9002",
                    "Correction to previous: prepare only the revised draft"
                ),
            ],
            "the final GPT turn must receive the chronological original and correction"
        );
        let final_start = ChannelModelStart {
            envelope: correction_context.last().unwrap().0.clone(),
            content: correction_context.last().unwrap().1.clone(),
            disposition: ChannelModelDisposition::ExecuteNow,
            suggestion: None,
        };
        let final_request = channel_outcome_request(&final_start, &correction_context).unwrap();
        assert!(
            final_request.prompt.find("prepare the first draft")
                < final_request
                    .prompt
                    .find("Correction to previous: prepare only the revised draft")
        );
        assert_eq!(final_request.allowed_source_refs.len(), 2);
        let unrelated_context = vec![
            correction_context[0].clone(),
            (correction_context[1].0.clone(), "book dinner".to_string()),
        ];
        let unrelated_start = ChannelModelStart {
            envelope: unrelated_context.last().unwrap().0.clone(),
            content: unrelated_context.last().unwrap().1.clone(),
            disposition: ChannelModelDisposition::ExecuteNow,
            suggestion: None,
        };
        assert!(
            channel_outcome_request(&unrelated_start, &unrelated_context).is_err(),
            "Host must reject caller-assembled multi-message context without the exact correction directive"
        );
        let mut middle_envelope = correction_context[0].0.clone();
        middle_envelope.source_message_id = "9001-middle".into();
        let overbounded_context = vec![
            correction_context[0].clone(),
            (middle_envelope, "an unauthorized extra predecessor".into()),
            correction_context[1].clone(),
        ];
        let overbounded_start = ChannelModelStart {
            envelope: overbounded_context.last().unwrap().0.clone(),
            content: overbounded_context.last().unwrap().1.clone(),
            disposition: ChannelModelDisposition::ExecuteNow,
            suggestion: None,
        };
        assert!(
            channel_outcome_request(&overbounded_start, &overbounded_context).is_err(),
            "Host must reject a caller-assembled correction context over the exact two-message cap"
        );
        host.store
            .record_channel_suggestion(ChannelKind::Discord, "9002", &correction, 13)
            .unwrap();
        assert!(
            !host
                .store
                .channel_model_work_pending(ChannelKind::Discord)
                .unwrap()
        );
        assert!(host.channel_suggestion_is_current(&correction).unwrap());

        // Emulate loss of the final model response or a Host restart: the
        // volatile slot is empty, while the durable final suggestion remains.
        *host
            .operations
            .suggestion
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
        let dashboard = request(
            &mut host,
            r#"{"jsonrpc":"2.0","id":704,"method":"mission.dashboard.read","params":{}}"#,
        )
        .result
        .unwrap();
        assert_eq!(dashboard["suggestion"]["id"], correction.id);
        assert_eq!(
            host.operations
                .suggestion
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .as_ref()
                .map(|value| value.id.as_str()),
            Some(correction.id.as_str())
        );
        let confirmed = request(
            &mut host,
            r#"{"jsonrpc":"2.0","id":705,"method":"mission.confirm","params":{"suggestionId":"suggestion-11-00000000000000000000000000000002","reminderTarget":{"sourceIdentifier":"source-1","calendarIdentifier":"calendar-1"}}}"#,
        );
        assert!(confirmed.error.is_none(), "{confirmed:?}");
    }

    fn assert_persistent_channel_model_need_you(
        host: &mut Host,
        channel: ChannelKind,
        source_message_id: &str,
        observed_at_ms: i64,
    ) {
        assert!(
            host.next_ready_channel_model(channel, observed_at_ms)
                .unwrap()
                .is_none(),
            "persistent Need you must leave the transport poll reachable"
        );
        let first = host.failed_channel_poll_value(channel).unwrap().unwrap();
        assert_eq!(first["eventStatus"], "needYou");
        assert!(first["invalidateSuggestionId"].is_null());
        assert_eq!(first["failureIncidents"].as_array().unwrap().len(), 1);
        for _ in 0..100 {
            assert_eq!(
                host.failed_channel_poll_value(channel).unwrap().unwrap(),
                first,
                "identical terminal polls must publish one stable durable incident"
            );
        }
        assert_eq!(
            host.store
                .begin_channel_model(channel, source_message_id)
                .unwrap()
                .disposition,
            ChannelModelDisposition::RecoverOnly,
            "persistent Need you must never grant another model execution"
        );
    }

    fn poll_channel_for_test(
        host: &mut Host,
        channel: ChannelKind,
        authorization: &RuntimeControlAuthorization,
        request_id: u64,
    ) -> Value {
        let challenge = request(
            host,
            &json!({
                "jsonrpc": "2.0",
                "id": request_id.saturating_add(100),
                "method": "mission.runtime.challenge",
                "params": {}
            })
            .to_string(),
        )
        .result
        .unwrap()["challenge"]
            .as_str()
            .unwrap()
            .to_owned();
        let receipt = broker_receipt(authorization, Some(&challenge));
        let request: RpcRequest = serde_json::from_value(json!({
            "jsonrpc": "2.0",
            "id": request_id,
            "method": "channel.poll",
            "params": {
                "channel": channel,
                "modelWorkAllowed": true,
                "authorization": authorization,
                "brokerReceipt": receipt,
            }
        }))
        .unwrap();
        let (send, receive) = mpsc::sync_channel(1);
        host.poll_channel(&request, &send);
        let response = receive.recv().unwrap();
        assert!(response.error.is_none(), "{response:?}");
        response.result.unwrap()
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn host_restart_surfaces_started_channel_model_as_need_you_without_reexecution() {
        let (_root, mut host) = fixture();
        let broker = broker_record(&host);
        host.store.install_trusted_broker(&broker).unwrap();
        let on = host.store.prepare_runtime_control(true, 1).unwrap();
        let receipt = broker_receipt(&on, None);
        host.store.commit_runtime_control(&on, &receipt).unwrap();
        host.store
            .pair_channel(&ChannelPairing {
                channel: ChannelKind::IMessage,
                owner_sender_id: "owner@example.invalid".into(),
                conversation_id: "42".into(),
                require_explicit_address: true,
                imessage: None,
                discord: None,
                paired_at_ms: 1,
            })
            .unwrap();
        let content = "@OpenOpen prepare the client follow-up";
        host.store
            .ingest_channel_message(
                &ChannelObservation {
                    envelope: ChannelEnvelope {
                        channel: ChannelKind::IMessage,
                        source_message_id: "imessage-model-started".into(),
                        sender_id: "owner@example.invalid".into(),
                        conversation_id: "42".into(),
                        content_sha256: format!("{:x}", Sha256::digest(content)),
                        received_at_ms: 2,
                    },
                    cursor: ChannelCursor {
                        channel: ChannelKind::IMessage,
                        conversation_id: "42".into(),
                        opaque_value: "cursor-started".into(),
                        order: 1,
                        observed_at_ms: 2,
                    },
                    is_bot: false,
                    explicitly_addressed: true,
                },
                content,
            )
            .unwrap();
        assert_eq!(
            host.store
                .begin_channel_model(ChannelKind::IMessage, "imessage-model-started")
                .unwrap()
                .disposition,
            ChannelModelDisposition::ExecuteNow
        );
        let paths = host.paths.clone();
        drop(host);

        let mut restarted = Host::open(paths, [7_u8; 32]).unwrap();
        restarted.store.install_trusted_broker(&broker).unwrap();
        restarted
            .operations
            .accept_committed_runtime(true, on.revision);
        assert_eq!(
            restarted
                .next_ready_channel_model(ChannelKind::IMessage, 3)
                .unwrap()
                .as_deref(),
            Some("imessage-model-started")
        );
        let start = restarted
            .store
            .begin_channel_model(ChannelKind::IMessage, "imessage-model-started")
            .unwrap();
        assert_eq!(start.disposition, ChannelModelDisposition::RecoverOnly);
        let request: RpcRequest = serde_json::from_value(json!({
            "jsonrpc": "2.0",
            "id": 709,
            "method": "channel.poll",
            "params": {}
        }))
        .unwrap();
        let params = PollChannel {
            channel: ChannelKind::IMessage,
            model_work_allowed: true,
            authorization: on,
            broker_receipt: receipt,
        };
        let operation = restarted.operations.begin_operation().unwrap();
        let (send, receive) = mpsc::sync_channel(1);
        restarted.process_channel_model_start(&request, &send, &params, operation, start);
        let response = receive.recv().unwrap().result.unwrap();
        assert_eq!(response["eventStatus"], "needYou");
        assert!(response["suggestion"].is_null());
        assert!(
            restarted
                .store
                .started_channel_model(ChannelKind::IMessage)
                .unwrap()
                .is_none(),
            "recovery must terminally fail the consumed dispatch without granting a second model call"
        );
        assert_persistent_channel_model_need_you(
            &mut restarted,
            ChannelKind::IMessage,
            "imessage-model-started",
            4,
        );
    }

    fn failed_imessage_incident_fixture(
        host: &mut Host,
    ) -> (
        BrokerEnrollmentRecord,
        RuntimeControlAuthorization,
        ChannelFailureIncident,
    ) {
        let broker = broker_record(host);
        host.store.install_trusted_broker(&broker).unwrap();
        let on = host.store.prepare_runtime_control(true, 1).unwrap();
        host.store
            .commit_runtime_control(&on, &broker_receipt(&on, None))
            .unwrap();
        host.operations.accept_committed_runtime(true, on.revision);
        host.store
            .pair_channel(&ChannelPairing {
                channel: ChannelKind::IMessage,
                owner_sender_id: "owner@example.invalid".into(),
                conversation_id: "42".into(),
                require_explicit_address: true,
                imessage: None,
                discord: None,
                paired_at_ms: 1,
            })
            .unwrap();
        let content = "@OpenOpen prepare the exact checklist";
        host.store
            .ingest_channel_message(
                &ChannelObservation {
                    envelope: ChannelEnvelope {
                        channel: ChannelKind::IMessage,
                        source_message_id: "incident-ack-message".into(),
                        sender_id: "owner@example.invalid".into(),
                        conversation_id: "42".into(),
                        content_sha256: format!("{:x}", Sha256::digest(content)),
                        received_at_ms: 2,
                    },
                    cursor: ChannelCursor {
                        channel: ChannelKind::IMessage,
                        conversation_id: "42".into(),
                        opaque_value: "incident-ack-cursor".into(),
                        order: 1,
                        observed_at_ms: 2,
                    },
                    is_bot: false,
                    explicitly_addressed: true,
                },
                content,
            )
            .unwrap();
        host.store
            .begin_channel_model(ChannelKind::IMessage, "incident-ack-message")
            .unwrap();
        host.store
            .fail_channel_model(ChannelKind::IMessage, "incident-ack-message", 3)
            .unwrap();
        let incident = host
            .store
            .channel_failure_incidents(None)
            .unwrap()
            .remove(0);
        (broker, on, incident)
    }

    fn assert_incident_ack_survives_restart(
        paths: HostPaths,
        broker: &BrokerEnrollmentRecord,
        on: &RuntimeControlAuthorization,
        incident: &ChannelFailureIncident,
    ) {
        let mut restarted = Host::open(paths.clone(), [7_u8; 32]).unwrap();
        restarted.store.install_trusted_broker(broker).unwrap();
        restarted
            .operations
            .accept_committed_runtime(true, on.revision);
        let dashboard = request(
            &mut restarted,
            r#"{"jsonrpc":"2.0","id":714,"method":"mission.dashboard.read","params":{}}"#,
        )
        .result
        .unwrap();
        assert_eq!(
            dashboard["channelFailureIncidents"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert!(dashboard["channelFailureIncidents"][0]["acknowledgement"].is_object());

        let retry_challenge = request(
            &mut restarted,
            r#"{"jsonrpc":"2.0","id":715,"method":"mission.runtime.challenge","params":{}}"#,
        )
        .result
        .unwrap()["challenge"]
            .as_str()
            .unwrap()
            .to_owned();
        let retry = request(
            &mut restarted,
            &json!({
                "jsonrpc": "2.0",
                "id": 716,
                "method": "channel.failure.acknowledge",
                "params": {
                    "incidentId": incident.incident_id.clone(),
                    "expectedIncidentAuditAnchor": incident.incident_audit_anchor.clone(),
                    "acknowledgedAtMs": 5,
                    "authorization": on.clone(),
                    "brokerReceipt": broker_receipt(on, Some(&retry_challenge)),
                }
            })
            .to_string(),
        );
        assert!(retry.error.is_none(), "{retry:?}");
        assert_eq!(
            retry.result.unwrap()["acknowledgement"],
            dashboard["channelFailureIncidents"][0]["acknowledgement"]
        );
        let connection = Connection::open(paths.store).unwrap();
        let acknowledgement_audits: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM audit_ledger
                 WHERE action = 'channel.failure_incident_acknowledged'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(acknowledgement_audits, 1);
        let durable = restarted.store.channel_failure_incidents(None).unwrap();
        assert_eq!(durable.len(), 1);
        assert!(durable[0].acknowledgement.is_some());
        assert_eq!(
            restarted
                .store
                .begin_channel_model(ChannelKind::IMessage, "incident-ack-message")
                .unwrap()
                .disposition,
            ChannelModelDisposition::RecoverOnly
        );
    }

    #[test]
    fn incident_acknowledgement_uses_store_control_proof_without_codex_operation_slot() {
        let (_root, mut host) = fixture();
        let (broker, on, incident) = failed_imessage_incident_fixture(&mut host);
        let occupied_model_slot = host.operations.begin_operation().unwrap();
        let challenge = request(
            &mut host,
            r#"{"jsonrpc":"2.0","id":712,"method":"mission.runtime.challenge","params":{}}"#,
        )
        .result
        .unwrap()["challenge"]
            .as_str()
            .unwrap()
            .to_owned();
        let response = request(
            &mut host,
            &json!({
                "jsonrpc": "2.0",
                "id": 713,
                "method": "channel.failure.acknowledge",
                "params": {
                    "incidentId": incident.incident_id.clone(),
                    "expectedIncidentAuditAnchor": incident.incident_audit_anchor.clone(),
                    "acknowledgedAtMs": 4,
                    "authorization": on.clone(),
                    "brokerReceipt": broker_receipt(&on, Some(&challenge)),
                }
            })
            .to_string(),
        );
        assert!(response.error.is_none(), "{response:?}");
        assert!(response.result.unwrap()["acknowledgement"].is_object());
        assert!(
            host.operations.begin_operation().is_none(),
            "acknowledgement must not consume or release the independently occupied model slot"
        );
        host.finish_operation(&occupied_model_slot);

        // Treat the successful response as lost, then retire and reopen the
        // complete verified Host/Store. The Dashboard must expose the durable
        // acknowledgement and an exact retry must remain idempotent.
        let paths = host.paths.clone();
        drop(host);
        assert_incident_ack_survives_restart(paths, &broker, &on, &incident);
    }

    #[test]
    #[allow(clippy::too_many_lines)] // Chronological failure, recovery, and correction proof.
    fn persistent_need_you_polls_and_persists_a_later_provider_correction() {
        let (_root, mut host) = fixture();
        let broker = broker_record(&host);
        host.store.install_trusted_broker(&broker).unwrap();
        let on = host.store.prepare_runtime_control(true, 1).unwrap();
        let receipt = broker_receipt(&on, None);
        host.store.commit_runtime_control(&on, &receipt).unwrap();
        host.operations.accept_committed_runtime(true, on.revision);
        install_test_core_lease(&mut host);
        host.store
            .pair_channel(&ChannelPairing {
                channel: ChannelKind::Discord,
                owner_sender_id: "1001".into(),
                conversation_id: "2002".into(),
                require_explicit_address: true,
                imessage: None,
                discord: Some(openopen_protocol::DiscordPairingMetadata {
                    guild_id: "3003".into(),
                    bot_user_id: "4004".into(),
                    application_id: "5005".into(),
                    setup_source_message_id: "6006".into(),
                    setup_candidate_id: format!("discord-pair-{}", "a".repeat(64)),
                }),
                paired_at_ms: 1,
            })
            .unwrap();
        let original = "prepare the client follow-up";
        host.store
            .ingest_channel_message(
                &ChannelObservation {
                    envelope: ChannelEnvelope {
                        channel: ChannelKind::Discord,
                        source_message_id: "9001".into(),
                        sender_id: "1001".into(),
                        conversation_id: "2002".into(),
                        content_sha256: format!("{:x}", Sha256::digest(original)),
                        received_at_ms: 2,
                    },
                    cursor: ChannelCursor {
                        channel: ChannelKind::Discord,
                        conversation_id: "2002".into(),
                        opaque_value: "9001".into(),
                        order: 9001,
                        observed_at_ms: 2,
                    },
                    is_bot: false,
                    explicitly_addressed: true,
                },
                original,
            )
            .unwrap();
        host.store
            .begin_channel_model(ChannelKind::Discord, "9001")
            .unwrap();
        host.store
            .fail_channel_model(ChannelKind::Discord, "9001", 3)
            .unwrap();

        let recovery = host
            .channels
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .install_test_discord_recovery();
        recovery
            .send(Ok(RecoveryBatch {
                envelopes: vec![DiscordInbound {
                    source_message_id: "9002".into(),
                    sender_id: "1001".into(),
                    conversation_id: "2002".into(),
                    content: "Correction to previous: prepare only the revised follow-up".into(),
                    received_at_ms: 4,
                }],
                high_water_message_id: "9003".into(),
                pages_fetched: 1,
            }))
            .unwrap();
        let inbound = poll_channel_for_test(&mut host, ChannelKind::Discord, &on, 710);
        assert_eq!(inbound["eventStatus"], "recovering");
        assert!(
            host.store
                .latest_failed_channel_model(ChannelKind::Discord)
                .unwrap()
                .is_none()
        );
        let cursor = poll_channel_for_test(&mut host, ChannelKind::Discord, &on, 711);
        assert_eq!(cursor["eventStatus"], "recovered");
        assert_eq!(
            host.next_ready_channel_model(ChannelKind::Discord, 5)
                .unwrap(),
            None,
            "the correction must not begin while the Discord adapter is not Connected"
        );
        assert_eq!(
            host.store
                .next_queued_channel_model(ChannelKind::Discord)
                .unwrap()
                .as_deref(),
            Some("9002"),
            "persistent Need you must not starve the durable correction queue"
        );
        host.channels
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .mark_test_discord_connected();
        assert_eq!(
            host.next_ready_channel_model(ChannelKind::Discord, 5)
                .unwrap()
                .as_deref(),
            Some("9002"),
            "the durable correction may begin only after the adapter is Connected"
        );
    }

    fn seed_primary_discord_mission(host: &mut Host) -> String {
        seed_model_selection(host);
        host.store
            .pair_channel(&ChannelPairing {
                channel: ChannelKind::Discord,
                owner_sender_id: "1001".into(),
                conversation_id: "2002".into(),
                require_explicit_address: true,
                imessage: None,
                discord: Some(openopen_protocol::DiscordPairingMetadata {
                    guild_id: "3003".into(),
                    bot_user_id: "4004".into(),
                    application_id: "5005".into(),
                    setup_source_message_id: "6006".into(),
                    setup_candidate_id: format!("discord-pair-{}", "a".repeat(64)),
                }),
                paired_at_ms: 1,
            })
            .unwrap();
        let recovery = host
            .channels
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .install_test_discord_recovery();
        recovery
            .send(Ok(RecoveryBatch {
                envelopes: Vec::new(),
                high_water_message_id: "0".into(),
                pages_fetched: 1,
            }))
            .unwrap();
        let recovery_cursor = host
            .channels
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .poll(ChannelKind::Discord, 1)
            .unwrap()
            .unwrap();
        let TransportEvent::Cursor(recovery_cursor) = recovery_cursor else {
            panic!("the seed Discord session must close its recovery cursor");
        };
        host.store.advance_channel_cursor(&recovery_cursor).unwrap();
        host.channels
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .acknowledge_recovery(&TransportEvent::Cursor(recovery_cursor))
            .unwrap();
        host.channels
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .mark_test_discord_connected();
        let content = "@OpenOpen prepare the client follow-up";
        host.store
            .ingest_channel_message(
                &ChannelObservation {
                    envelope: ChannelEnvelope {
                        channel: ChannelKind::Discord,
                        source_message_id: "discord-message-1".into(),
                        sender_id: "1001".into(),
                        conversation_id: "2002".into(),
                        content_sha256: format!("{:x}", Sha256::digest(content)),
                        received_at_ms: 2,
                    },
                    cursor: ChannelCursor {
                        channel: ChannelKind::Discord,
                        conversation_id: "2002".into(),
                        opaque_value: "cursor-1".into(),
                        order: 1,
                        observed_at_ms: 2,
                    },
                    is_bot: false,
                    explicitly_addressed: true,
                },
                content,
            )
            .unwrap();
        host.store
            .begin_channel_model(ChannelKind::Discord, "discord-message-1")
            .unwrap();
        let suggestion = hero_suggestion();
        host.store
            .record_channel_suggestion(ChannelKind::Discord, "discord-message-1", &suggestion, 3)
            .unwrap();
        *host.operations.suggestion.lock().unwrap() = Some(suggestion);
        confirm_hero_mission(host)["missionId"]
            .as_str()
            .unwrap()
            .to_owned()
    }

    fn broker_record(host: &Host) -> BrokerEnrollmentRecord {
        let key = SigningKey::from_bytes(&[41_u8; 32]);
        let verifying_key = key.verifying_key().to_bytes();
        let mut record = BrokerEnrollmentRecord {
            version: 1,
            broker_key_id: format!("{:x}", Sha256::digest(verifying_key)),
            broker_verifying_key_hex: hex::encode(verifying_key),
            helper_designated_requirement_digest: "cd".repeat(32),
            installed_at_ms: 1,
            core_key_id: host.authority.effect_key_id(),
            core_authorization_signature_hex: String::new(),
        };
        let mut derivation = b"openopen-effect-authorizer-v1".to_vec();
        derivation.extend([7_u8; 32]);
        let core_key = SigningKey::from_bytes(&Sha256::digest(derivation).into());
        record.core_authorization_signature_hex = hex::encode(
            core_key
                .sign(&broker_enrollment_signing_bytes(&record).unwrap())
                .to_bytes(),
        );
        record
    }

    fn install_test_core_lease(host: &mut Host) {
        let broker_key = SigningKey::from_bytes(&[41_u8; 32]);
        let mut lease = CoreInstanceLease {
            protocol_version: EFFECT_PROTOCOL_VERSION,
            audit_euid: rustix::process::geteuid().as_raw(),
            app_pid: 1,
            app_start_time_us: 1,
            core_pid: i32::try_from(std::process::id()).unwrap(),
            core_start_time_us: 1,
            core_audit_token_hex: "aa".repeat(32),
            codex_pid: 42,
            codex_start_time_us: 2,
            codex_audit_token_hex: "bb".repeat(32),
            core_instance_nonce: host.instance_nonce.clone(),
            issued_at_ms: super::now_ms().unwrap(),
            broker_key_id: format!(
                "{:x}",
                Sha256::digest(broker_key.verifying_key().to_bytes())
            ),
            broker_signature_hex: String::new(),
        };
        lease.broker_signature_hex = hex::encode(
            broker_key
                .sign(&core_instance_lease_signing_bytes(&lease).unwrap())
                .to_bytes(),
        );
        *host
            .operations
            .codex_pid
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(42);
        let response = request(
            host,
            &json!({
                "jsonrpc": "2.0",
                "id": 712,
                "method": "broker.lease.install",
                "params": {"lease": lease}
            })
            .to_string(),
        );
        assert_eq!(response.result.unwrap()["status"], "installed");
    }

    fn broker_receipt(
        authorization: &RuntimeControlAuthorization,
        request_nonce: Option<&str>,
    ) -> RuntimeControlReceipt {
        let key = SigningKey::from_bytes(&[41_u8; 32]);
        let mut receipt = RuntimeControlReceipt {
            protocol_version: EFFECT_PROTOCOL_VERSION,
            authorization_hash: runtime_control_authorization_hash(authorization).unwrap(),
            checkpoint_nonce: "90".repeat(32),
            request_nonce: request_nonce.map(ToOwned::to_owned),
            broker_key_id: format!("{:x}", Sha256::digest(key.verifying_key().to_bytes())),
            broker_signature_hex: String::new(),
        };
        receipt.broker_signature_hex = hex::encode(
            key.sign(&runtime_control_receipt_signing_bytes(&receipt).unwrap())
                .to_bytes(),
        );
        receipt
    }
}
