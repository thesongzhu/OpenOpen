//! Production Rust host for `OpenOpen`'s local JSON-RPC surface.

use openopen_codex_client::{
    ChatGptLogin, CodexClient, CodexError, CodexRuntimeConfig, OutcomeRequest,
};
use openopen_core::{
    BrokerEnrollmentRecord, LocalAuthority, Store, StoreError, TrustedBrokerEnrollment,
    authorize_broker_enrollment, verify_core_instance_lease,
};
use openopen_protocol::{
    CoreInstanceLease, OutcomeSuggestion, RpcError, RpcRequest, RpcResponse,
    RuntimeControlAuthorization, RuntimeControlReceipt,
};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
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
        let response = self.store.runtime_control().map_or_else(
            |error| host_failure(request.id, &error),
            |runtime| {
                let suggestion = self
                    .operations
                    .suggestion
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .clone();
                RpcResponse::success(
                    request.id,
                    json!({
                        "activeCards": [],
                        "microphone": {"available": false, "reason": "Microphone unavailable until Voice setup"},
                        "runtime": runtime,
                        "suggestion": suggestion
                    }),
                )
            },
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
                let encoded = serde_json::to_vec(&value).map_err(|_| HostCallError::Internal)?;
                let suggestion = OutcomeSuggestion {
                    id: format!("suggestion-{}", &hex::encode(Sha256::digest(encoded))[..24]),
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
        BrokerEnrollmentRecord, TrustedBrokerEnrollment, broker_enrollment_signing_bytes,
    };
    use openopen_protocol::{
        CoreInstanceLease, EFFECT_PROTOCOL_VERSION, RpcResponse, RuntimeControlAuthorization,
        RuntimeControlReceipt, core_instance_lease_signing_bytes,
        runtime_control_authorization_hash, runtime_control_receipt_signing_bytes,
    };
    use rusqlite::Connection;
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
