//! Production Rust host for `OpenOpen`'s local JSON-RPC surface.

mod channels;

use channels::{
    ChannelConnectionStatus, ChannelRuntime, ChannelRuntimeError, ChannelSendResult,
    TransportEvent, TransportInbound,
};

use openopen_codex_client::{
    ChatGptLogin, CodexClient, CodexError, CodexRuntimeConfig, OutcomeRequest, REQUIRED_MODEL,
};
use openopen_core::{
    ActionGate, ActionProposal, ActionTarget, ApprovalDecision, AuditAnchor,
    BrokerEnrollmentRecord, CreateMission, CreateWorkItem, EffectKind, EvidenceClaims,
    GateDecision, LocalAuthority, MissionCommand, MissionCommandEnvelope, NewBoundaryApproval,
    NewReceipt, Store, StoreError, TrustedBrokerEnrollment, authorize_broker_enrollment,
    channel_message_payload, channel_need_you_content, channel_receipt_content,
    verify_core_instance_lease,
};
use openopen_protocol::{
    ApprovalKind, ApprovalStatus, ApprovalTarget, ChannelDeliveryReceipt, ChannelEnvelope,
    ChannelInboundDecision, ChannelInboundResult, ChannelKind, ChannelMessageKind,
    ChannelModelDisposition, ChannelModelStart, ChannelObservation, ChannelOutboundDisposition,
    ChannelOutboundIntent, ChannelPairing, ChannelRouteApproval, ChannelRouteSet,
    CoreInstanceLease, DiscordPairingMetadata, EffectAuditAnchor, EvidenceKind, Mission,
    MissionStatus, OutcomeSuggestion, Receipt, RpcError, RpcRequest, RpcResponse,
    RuntimeControlAuthorization, RuntimeControlReceipt, WorkItem, WorkItemStatus,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::io::{self, Read};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc::SyncSender as Sender,
};
use std::time::{SystemTime, UNIX_EPOCH};
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
}

#[derive(Clone, Debug)]
pub struct HostPaths {
    pub store: PathBuf,
    pub codex_runtime: PathBuf,
    pub codex_home: PathBuf,
    pub synthetic_home: PathBuf,
    pub model_input_root: PathBuf,
    pub imsg_runtime: PathBuf,
    pub user_home: PathBuf,
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
            user_home: home,
        })
    }
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

#[derive(Clone, Default)]
struct OperationState {
    gate: Arc<Mutex<OperationGate>>,
    login: Arc<Mutex<Option<LoginSession>>>,
    suggestion: Arc<Mutex<Option<OutcomeSuggestion>>>,
    runtime_challenge: Arc<Mutex<Option<String>>>,
    codex: Arc<Mutex<Option<CodexClient>>>,
    codex_pid: Arc<Mutex<Option<i32>>>,
    codex_cancel: Arc<AtomicBool>,
}

impl OperationState {
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
        match (gate.active.as_ref(), login.as_ref()) {
            (None, None) => true,
            (Some(token), Some(_)) => {
                token.store(true, Ordering::Release);
                *login = None;
                gate.active = None;
                true
            }
            _ => false,
        }
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

    fn cancel_active(&self) {
        let mut gate = self
            .gate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        gate.runtime = RuntimeAuthorityState::OffLatched {
            minimum_on_revision: None,
        };
        self.codex_cancel.store(true, Ordering::Release);
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
    instance_lease: Arc<Mutex<Option<CoreInstanceLease>>>,
    channels: Mutex<ChannelRuntime>,
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
        let store = Store::open(&paths.store, authority.clone())?;
        Ok(Self {
            store,
            authority,
            paths,
            operations: OperationState::default(),
            instance_nonce: hex::encode(nonce),
            instance_lease: Arc::new(Mutex::new(None)),
            channels: Mutex::new(ChannelRuntime::new().map_err(|_| HostError::ChannelRuntime)?),
        })
    }

    pub fn handle_line(&mut self, line: &str, responses: &Sender<RpcResponse>) {
        let Some(request) = parse_request(line, responses) else {
            return;
        };
        if request.jsonrpc != "2.0" {
            let _ = responses.send(invalid_request(Some(request.id)));
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
            "outcome.propose" => self.start_outcome(&request, responses),
            "mission.confirm" => self.confirm_mission(&request, responses),
            "mission.cancel" => self.cancel_mission(&request, responses),
            "mission.reminders.begin" => self.begin_reminder_dispatch(&request, responses),
            "mission.reminders.record" => self.record_reminder_mirror(&request, responses),
            "mission.reminders.complete" => self.complete_reminders(&request, responses),
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

    fn prepare_runtime(&self, request: &RpcRequest, responses: &Sender<RpcResponse>) {
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

    fn execute_channel_model(
        &self,
        request: &RpcRequest,
        responses: &Sender<RpcResponse>,
        params: &PollChannel,
        operation: Arc<AtomicBool>,
        start: ChannelModelStart,
    ) {
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
                        let incidents = store
                            .channel_failure_incident_projection(Some(start.envelope.channel))?;
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

    fn start_outcome(&self, request: &RpcRequest, responses: &Sender<RpcResponse>) {
        let Ok(params) = decode_params::<OutcomeWithProof>(request) else {
            let _ = responses.send(invalid_params(request.id));
            return;
        };
        let outcome = params.outcome();
        if let Err(error) = outcome.validate() {
            let _ = responses.send(call_failure(request.id, &HostCallError::Codex(error)));
            return;
        }
        let Some(token) = self.begin_operation(request.id, &params.proof(), responses) else {
            return;
        };
        let context = self.background_context(params.proof());
        let suggestion_slot = self.operations.suggestion.clone();
        let responses = responses.clone();
        let request_id = request.id;
        std::thread::spawn(move || {
            let result = (|| -> Result<OutcomeSuggestion, HostCallError> {
                let workspace = ModelWorkspace::create(&context.paths.model_input_root)?;
                let value = context.with_client(|client| {
                    client.run_structured_outcome_in_workspace(&outcome, &workspace.path)
                })?;
                context.require_enabled()?;
                if token.load(Ordering::Acquire) {
                    return Err(HostCallError::Codex(CodexError::Cancelled));
                }
                let mut suggestion_nonce = [0_u8; 16];
                getrandom::fill(&mut suggestion_nonce).map_err(|_| HostCallError::Internal)?;
                let suggested_at_ms = now_ms()?;
                let suggestion = OutcomeSuggestion {
                    id: format!(
                        "suggestion-{suggested_at_ms}-{}",
                        hex::encode(suggestion_nonce)
                    ),
                    title: value.title,
                    why_now: value.why_now,
                    proposed_steps: value.proposed_steps,
                    source_refs: value.source_refs,
                };
                *suggestion_slot
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(suggestion.clone());
                Ok(suggestion)
            })();
            context.finish_operation(&token);
            let _ = responses.send(result.map_or_else(
                |error| call_failure(request_id, &error),
                |value| success(request_id, value),
            ));
        });
    }

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
                evidence: self.authority.sign_evidence(EvidenceClaims {
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
        let mut receipt_completed_at_ms = completed_at_ms;
        let mut receipt = new_reminder_receipt(mission, &evidence_ids, completed_at_ms)?;
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
        }
    }

    fn retire_codex_runtime(&self) {
        self.operations.codex_cancel.store(true, Ordering::Release);
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
}

impl BackgroundContext {
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
    #[error("channel listener is unavailable")]
    ChannelUnavailable,
    #[error("channel boundary verification failed")]
    ChannelIntegrity,
    #[error("another Mission still needs the owner")]
    MissionAlreadyInProgress,
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
struct OutcomeWithProof {
    prompt: String,
    allowed_source_refs: Vec<String>,
    authorization: RuntimeControlAuthorization,
    broker_receipt: RuntimeControlReceipt,
}

impl OutcomeWithProof {
    fn proof(&self) -> RuntimeProof {
        RuntimeProof {
            authorization: self.authorization.clone(),
            broker_receipt: self.broker_receipt.clone(),
        }
    }

    fn outcome(&self) -> OutcomeRequest {
        OutcomeRequest {
            prompt: self.prompt.clone(),
            allowed_source_refs: self.allowed_source_refs.clone(),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ConfirmMission {
    suggestion_id: String,
    reminder_target: ReminderTarget,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CancelMission {
    mission_id: String,
    authorization: RuntimeControlAuthorization,
    broker_receipt: RuntimeControlReceipt,
}

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
struct CompleteReminders {
    mission_id: String,
    completions: Vec<ReminderCompletion>,
    receipt_return_approved_at_ms: Option<i64>,
    receipt_return_route_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BeginReminderDispatch {
    mission_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
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
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReminderDispatchStart {
    mission: ConfirmedMission,
    execute_now: bool,
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
        "outcome.",
        "mission.",
        "channel.",
        "receipt.",
        "workflow.",
        "skill.",
    ]
    .iter()
    .any(|prefix| method.starts_with(prefix))
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
    let outcome = OutcomeRequest {
        prompt,
        allowed_source_refs,
    };
    outcome.validate()?;
    Ok(outcome)
}

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

fn validated_reminder_completions(
    mission: &Mission,
    authority: &LocalAuthority,
    completions: Vec<ReminderCompletion>,
    observed_now_ms: i64,
    require_pending: bool,
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
    let authorization =
        reminder_authorization_from_mission(mission, ReminderWriteDisposition::RecoverOnly)
            .ok()??;
    let dispatch = reminder_dispatch_tokens(mission, &authorization, authority).ok()?;
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
        actual_model: REQUIRED_MODEL.to_owned(),
        output_hashes: Vec::new(),
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

fn serialized_sha256(value: &impl serde::Serialize) -> Result<String, HostCallError> {
    let encoded = serde_json::to_vec(value).map_err(|_| HostCallError::Internal)?;
    Ok(format!("{:x}", Sha256::digest(encoded)))
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
            "GPT-5.6 Sol with high reasoning is unavailable",
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
        HostCallError::MissionAlreadyInProgress => failure(
            Some(id),
            -32_022,
            "Finish the current Mission before confirming another",
        ),
        _ => failure(Some(id), -32_000, "Local operation failed closed"),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BOOTSTRAP_MAGIC, ChannelConnectionStatus, ChannelSendResult, ChatGptLogin, CodexError,
        Host, HostCallError, HostPaths, OperationState, PollChannel, ReminderTarget, RpcRequest,
        SendChannelMessage, TransportEvent, TransportInbound, channel_outcome_request,
        decode_params, mission_command_batch, read_bootstrap,
    };
    use ed25519_dalek::{Signer, SigningKey};
    use openopen_core::{
        ActionGate, ActionProposal, ActionTarget, BrokerEnrollmentRecord, CreateMission,
        CreateWorkItem, EffectKind, GateDecision, MissionCommand, NewBoundaryApproval,
        TrustedBrokerEnrollment, broker_enrollment_signing_bytes,
    };
    use openopen_discord_adapter::{InboundEnvelope as DiscordInbound, RecoveryBatch};
    use openopen_protocol::{
        ApprovalKind, ApprovalStatus, ApprovalTarget, ChannelCursor, ChannelEnvelope,
        ChannelFailureIncident, ChannelInboundMessageClass, ChannelKind, ChannelMessageKind,
        ChannelModelDisposition, ChannelModelStart, ChannelObservation, ChannelOutboundDisposition,
        ChannelPairing, ChannelRouteApproval, ChannelRouteApprovalDecision, CoreInstanceLease,
        EFFECT_PROTOCOL_VERSION, EvidenceKind, MissionStatus, OutcomeSuggestion, Receipt,
        RpcResponse, RuntimeControlAuthorization, RuntimeControlReceipt, WorkItemStatus,
        core_instance_lease_signing_bytes, runtime_control_authorization_hash,
        runtime_control_receipt_signing_bytes,
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

    fn fixture() -> (tempfile::TempDir, Host) {
        let root = tempfile::tempdir().unwrap();
        let root_path = std::fs::canonicalize(root.path()).unwrap();
        let support = root_path.join("support");
        std::fs::create_dir(&support).unwrap();
        std::fs::set_permissions(&support, std::fs::Permissions::from_mode(0o700)).unwrap();
        let host = Host::open(
            HostPaths {
                store: support.join("store.sqlite3"),
                codex_runtime: root_path.join("missing-codex"),
                codex_home: support.join("codex-home"),
                synthetic_home: support.join("synthetic-home"),
                model_input_root: support.join("model-input"),
                imsg_runtime: root_path.join("missing-imsg"),
                user_home: root_path.clone(),
            },
            [7_u8; 32],
        )
        .unwrap();
        (root, host)
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
",
        )
        .unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
        executable
    }

    fn request(host: &mut Host, line: &str) -> RpcResponse {
        let (send, receive) = mpsc::sync_channel(32);
        host.handle_line(line, &send);
        receive.recv().unwrap()
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

    fn confirm_hero_mission(host: &mut Host) -> Value {
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
        let cancel_thread = std::thread::spawn(move || cancelling.cancel_active());
        for _ in 0..1_000 {
            if racing.codex_cancel.load(Ordering::Acquire) {
                break;
            }
            std::thread::yield_now();
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
    fn off_blocks_codex_before_any_runtime_spawn() {
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
            &outcome_request(1, "Plan today", Some(&challenge)),
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
        assert_eq!(receipt.actual_model, "gpt-5.6-sol");
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
        assert_eq!(receipt.result.unwrap()["actualModel"], "gpt-5.6-sol");
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
    fn invalid_prompt_is_rejected_before_a_codex_process_is_touched() {
        let (_root, mut host) = fixture();
        let response = request(&mut host, &outcome_request(2, "   ", None));
        assert_eq!(response.error.unwrap().code, -32_000);
        assert!(
            std::fs::read_dir(&host.paths.model_input_root)
                .unwrap()
                .next()
                .is_none()
        );
    }

    fn outcome_request(id: u64, prompt: &str, request_nonce: Option<&str>) -> String {
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "outcome.propose",
            "params": {
                "prompt": prompt,
                "allowedSourceRefs": [],
                "authorization": {
                    "protocolVersion": 1,
                    "enabled": false,
                    "revision": 1,
                    "updatedAtMs": 1,
                    "coreKeyId": "00".repeat(32),
                    "authorizationSignatureHex": "00".repeat(64)
                },
                "brokerReceipt": {
                    "protocolVersion": 1,
                    "authorizationHash": "00".repeat(32),
                    "checkpointNonce": "00".repeat(32),
                    "requestNonce": request_nonce,
                    "brokerKeyId": "00".repeat(32),
                    "brokerSignatureHex": "00".repeat(64)
                }
            }
        })
        .to_string()
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
            (
                103,
                "outcome.propose",
                serde_json::json!({"allowedSourceRefs": [], "prompt": "Plan today"}),
            ),
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
    fn model_forbidden_poll_ingests_inbound_without_starting_a_dispatch() {
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
        let response = receive.recv().unwrap().result.unwrap();

        assert_eq!(response["eventStatus"], "recovering");
        assert!(response["suggestion"].is_null());
        assert_eq!(
            host.store
                .next_queued_channel_model(ChannelKind::IMessage)
                .unwrap()
                .as_deref(),
            Some("imessage-awaiting-account")
        );
        assert!(
            host.store
                .started_channel_model(ChannelKind::IMessage)
                .unwrap()
                .is_none(),
            "a model-forbidden poll may persist/dedupe input but cannot start its dispatch"
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
        host.store
            .pair_channel(&ChannelPairing {
                channel: ChannelKind::Discord,
                owner_sender_id: "1001".into(),
                conversation_id: "2002".into(),
                require_explicit_address: true,
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
