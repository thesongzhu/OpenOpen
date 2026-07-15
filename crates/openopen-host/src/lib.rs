//! Production Rust host for `OpenOpen`'s local JSON-RPC surface.

use openopen_codex_client::{
    ChatGptLogin, CodexClient, CodexError, CodexRuntimeConfig, OutcomeRequest, REQUIRED_MODEL,
};
use openopen_core::{
    ActionGate, ActionProposal, ActionTarget, ApprovalDecision, AuditAnchor,
    BrokerEnrollmentRecord, CreateMission, CreateWorkItem, EffectKind, EvidenceClaims,
    GateDecision, LocalAuthority, MissionCommand, MissionCommandEnvelope, NewBoundaryApproval,
    NewReceipt, Store, StoreError, TrustedBrokerEnrollment, authorize_broker_enrollment,
    verify_core_instance_lease,
};
use openopen_protocol::{
    ApprovalKind, ApprovalStatus, ApprovalTarget, CoreInstanceLease, EvidenceKind, Mission,
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
}

#[derive(Clone, Debug)]
pub struct HostPaths {
    pub store: PathBuf,
    pub codex_runtime: PathBuf,
    pub codex_home: PathBuf,
    pub synthetic_home: PathBuf,
    pub model_input_root: PathBuf,
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
        Ok(Self {
            store: support.join("openopen.sqlite3"),
            codex_runtime: contents.join("Resources/Codex/0.144.0/bin/codex"),
            codex_home: support.join("CodexHome"),
            synthetic_home: support.join("CodexSyntheticHome"),
            model_input_root: support.join("ModelInput"),
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
}

impl Host {
    /// Opens the signed persistent Store with Keychain-sourced master material.
    ///
    /// # Errors
    ///
    /// Returns an error for unsafe paths or Store initialization failure.
    pub fn open(paths: HostPaths, master: [u8; 32]) -> Result<Self, HostError> {
        let master = Zeroizing::new(master);
        for directory in [
            &paths.codex_home,
            &paths.synthetic_home,
            &paths.model_input_root,
        ] {
            create_exact_private_directory(directory)?;
        }
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
            "broker.codex.initialize" => self.initialize_codex_runtime(&request, responses),
            "broker.codex.abort" => self.abort_codex_candidate(&request, responses),
            "broker.lease.install" => self.install_core_lease(&request, responses),
            "mission.dashboard.read" => self.read_dashboard(&request, responses),
            "account.read" => self.start_account_read(&request, responses),
            "account.login.start" => self.start_login(&request, responses),
            "account.login.await" => self.await_login(&request, responses),
            "models.list" => self.start_model_list(&request, responses),
            "outcome.propose" => self.start_outcome(&request, responses),
            "mission.confirm" => self.confirm_mission(&request, responses),
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
                return Ok(client.process_identifier());
            }
            self.operations.codex_cancel.store(false, Ordering::Release);
            let client = CodexClient::spawn_uninitialized_with_cancel(
                &CodexRuntimeConfig {
                    runtime: self.paths.codex_runtime.clone(),
                    codex_home: self.paths.codex_home.clone(),
                    synthetic_home: self.paths.synthetic_home.clone(),
                    model_workspace: self.paths.model_input_root.clone(),
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

    fn abort_codex_candidate(&self, request: &RpcRequest, responses: &Sender<RpcResponse>) {
        if decode_params::<NoParams>(request).is_err() || self.has_instance_lease() {
            let _ = responses.send(invalid_params(request.id));
            return;
        }
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
        drop(client);
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
        let result = (|| -> Result<Value, HostCallError> {
            let runtime = self.store.runtime_control()?;
            let suggestion = self
                .operations
                .suggestion
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone();
            let Some(anchor) = self.store.current_verified_audit_anchor()? else {
                return Ok(json!({
                    "activeCards": [],
                    "confirmedMission": null,
                    "microphone": {"available": false, "reason": "Microphone unavailable until Voice setup"},
                    "receipt": null,
                    "runtime": runtime,
                    "suggestion": suggestion
                }));
            };
            let mut confirmed = Vec::new();
            for mission in self.store.list_missions(&anchor)? {
                if let Some(value) = confirmed_mission_from_mission(
                    &mission,
                    &self.authority,
                    ReminderWriteDisposition::RecoverOnly,
                )? {
                    confirmed.push(value);
                }
            }
            let receipt = self.store.list_receipts(&anchor)?.into_iter().next();
            let active_cards = confirmed
                .iter()
                .take(3)
                .map(|mission| {
                    json!({"id": mission.mission_id, "state": "working", "title": mission.title})
                })
                .collect::<Vec<_>>();
            Ok(json!({
                "activeCards": active_cards,
                "confirmedMission": confirmed.first(),
                "microphone": {"available": false, "reason": "Microphone unavailable until Voice setup"},
                "receipt": receipt,
                "runtime": runtime,
                "suggestion": suggestion
            }))
        })();
        let response = result.map_or_else(
            |error| call_failure(request.id, &error),
            |dashboard| success(request.id, dashboard),
        );
        let _ = responses.send(response);
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
            let _ = responses.send(result.map_or_else(
                |error| call_failure(request_id, &error),
                |value| success(request_id, value),
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
        let expected_anchor = self.store.current_verified_audit_anchor()?;
        let envelopes = mission_command_batch(expected_anchor.as_ref(), &mission_id, commands)?;
        let mission = self
            .store
            .execute_mission_command_batch(&envelopes)?
            .pop()
            .ok_or(HostCallError::Internal)?
            .mission;
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
        let Some(completions) = validated_reminder_completions(
            &mission,
            &self.authority,
            params.completions,
            observed_now,
            mission.status == MissionStatus::Active,
        ) else {
            let _ = responses.send(invalid_params(request.id));
            return;
        };
        let result = match mission.status {
            MissionStatus::Active => {
                self.persist_reminder_completion(&mission, &anchor, &completions, observed_now)
            }
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
    ) -> Result<Receipt, HostCallError> {
        let mut commands = Vec::with_capacity(mission.work_items.len() * 3 + 1);
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
        let receipt_id = hashed_identifier(
            "receipt",
            &json!({"evidenceIds": evidence_ids, "missionId": mission.id}),
        )?;
        commands.push(MissionCommand::Complete {
            mission_id: mission.id.clone(),
            receipt: NewReceipt {
                id: receipt_id,
                summary: format!(
                    "Completed {} with {} verified Reminders.",
                    mission.title,
                    mission.work_items.len()
                ),
                actual_model: REQUIRED_MODEL.to_owned(),
                output_hashes: Vec::new(),
                completed_at_ms,
            },
            now_ms: completed_at_ms,
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
        }
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
}

impl BackgroundContext {
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
    #[error("internal failure")]
    Internal,
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
            || (require_pending && completion.completed_at_ms < mission.updated_at_ms)
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
        _ => failure(Some(id), -32_000, "Local operation failed closed"),
    }
}

#[cfg(test)]
mod tests {
    use super::{BOOTSTRAP_MAGIC, ChatGptLogin, Host, HostPaths, OperationState, read_bootstrap};
    use ed25519_dalek::{Signer, SigningKey};
    use openopen_core::{
        ActionGate, ActionProposal, ActionTarget, BrokerEnrollmentRecord, CreateMission,
        CreateWorkItem, EffectKind, GateDecision, MissionCommand, TrustedBrokerEnrollment,
        broker_enrollment_signing_bytes,
    };
    use openopen_protocol::{
        ApprovalKind, ApprovalStatus, ApprovalTarget, CoreInstanceLease, EFFECT_PROTOCOL_VERSION,
        EvidenceKind, MissionStatus, OutcomeSuggestion, Receipt, RpcResponse,
        RuntimeControlAuthorization, RuntimeControlReceipt, WorkItemStatus,
        core_instance_lease_signing_bytes, runtime_control_authorization_hash,
        runtime_control_receipt_signing_bytes,
    };
    use rusqlite::Connection;
    use serde_json::{Value, json};
    use sha2::{Digest, Sha256};
    use std::io::Cursor;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::atomic::Ordering;
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
            },
            [7_u8; 32],
        )
        .unwrap();
        (root, host)
    }

    fn request(host: &mut Host, line: &str) -> RpcResponse {
        let (send, receive) = mpsc::sync_channel(32);
        host.handle_line(line, &send);
        receive.recv().unwrap()
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
    fn identical_outcomes_create_distinct_missions_but_exact_suggestion_retry_is_idempotent() {
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
        let second = request(
            &mut host,
            &json!({
                "jsonrpc": "2.0",
                "id": 306,
                "method": "mission.confirm",
                "params": {"suggestionId": second_id, "reminderTarget": {"sourceIdentifier": "source-1", "calendarIdentifier": "calendar-1"}}
            })
            .to_string(),
        )
        .result
        .unwrap();
        assert_ne!(second["missionId"], first_mission_id);
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

        assert_dashboard_recovers_receipt_and_new_mission(&mut reopened, &receipt);
    }

    fn assert_dashboard_recovers_receipt_and_new_mission(reopened: &mut Host, receipt: &Value) {
        let second_id = "suggestion-1700000000000-89abcdef0123456789abcdef01234567";
        *reopened.operations.suggestion.lock().unwrap() = Some(hero_suggestion_with_id(second_id));
        let second = request(
            reopened,
            &json!({
                "jsonrpc": "2.0",
                "id": 311,
                "method": "mission.confirm",
                "params": {"suggestionId": second_id, "reminderTarget": {"sourceIdentifier": "source-1", "calendarIdentifier": "calendar-1"}}
            })
            .to_string(),
        )
        .result
        .unwrap();
        let dashboard = request(
            reopened,
            r#"{"jsonrpc":"2.0","id":312,"method":"mission.dashboard.read","params":{}}"#,
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
        assert_eq!(&dashboard["receipt"], receipt);
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
