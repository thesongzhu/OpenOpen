//! Fail-closed client for the pinned Codex app-server stable method subset.

mod contracts;
mod process;
mod wire;

pub use contracts::{
    AccountState, ChatGptLogin, ChoiceGenerationRequest, GptModel,
    MEMORY_CANDIDATE_DEVELOPER_INSTRUCTIONS, MemoryCandidateGenerationRequest, OutcomeRequest,
    SelectedModel, StructuredChoiceGeneration, StructuredChoiceOption, StructuredMemoryCandidate,
    StructuredMemoryCandidateGeneration, StructuredOutcome,
};
pub use process::{
    CODEX_BINARY_SHA256, CODEX_CODE_MODE_HOST_SHA256, CODEX_PACKAGE_SHA256, CODEX_RG_SHA256,
    CODEX_VERSION, CodexRuntimeConfig,
};

use contracts::{
    MAX_MODEL_CATALOG_BYTES, MAX_MODEL_CURSOR_BYTES, MAX_MODEL_PAGES, MAX_MODELS, model_from_value,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};
use thiserror::Error;
use wire::{Incoming, Transport};

trait StructuredRequest {
    fn validate_request(&self) -> Result<(), CodexError>;
    fn prompt(&self) -> &str;
    fn schema(&self) -> Value;
    fn selected_model(&self) -> Option<&SelectedModel>;
    fn allowed_source_refs(&self) -> &[String];
    fn developer_instructions(&self) -> &str;
}

trait StructuredResponse: Sized {
    fn parse_response(text: &str, allowed_source_refs: &[String]) -> Result<Self, CodexError>;
}

impl StructuredRequest for OutcomeRequest {
    fn validate_request(&self) -> Result<(), CodexError> {
        self.validate()
    }
    fn prompt(&self) -> &str {
        &self.prompt
    }
    fn schema(&self) -> Value {
        self.output_schema()
    }
    fn selected_model(&self) -> Option<&SelectedModel> {
        self.selected_model.as_ref()
    }
    fn allowed_source_refs(&self) -> &[String] {
        &self.allowed_source_refs
    }
    fn developer_instructions(&self) -> &str {
        &self.developer_instructions
    }
}

impl StructuredRequest for ChoiceGenerationRequest {
    fn validate_request(&self) -> Result<(), CodexError> {
        self.validate()
    }
    fn prompt(&self) -> &str {
        &self.prompt
    }
    fn schema(&self) -> Value {
        self.output_schema()
    }
    fn selected_model(&self) -> Option<&SelectedModel> {
        self.selected_model.as_ref()
    }
    fn allowed_source_refs(&self) -> &[String] {
        &self.allowed_source_refs
    }
    fn developer_instructions(&self) -> &str {
        &self.developer_instructions
    }
}

impl StructuredRequest for MemoryCandidateGenerationRequest {
    fn validate_request(&self) -> Result<(), CodexError> {
        self.validate()
    }
    fn prompt(&self) -> &str {
        &self.prompt
    }
    fn schema(&self) -> Value {
        self.output_schema()
    }
    fn selected_model(&self) -> Option<&SelectedModel> {
        Some(&self.selected_model)
    }
    fn allowed_source_refs(&self) -> &[String] {
        &self.allowed_source_refs
    }
    fn developer_instructions(&self) -> &str {
        &self.developer_instructions
    }
}

impl StructuredResponse for StructuredOutcome {
    fn parse_response(text: &str, allowed_source_refs: &[String]) -> Result<Self, CodexError> {
        Self::parse_and_validate(text, allowed_source_refs)
    }
}

impl StructuredResponse for StructuredChoiceGeneration {
    fn parse_response(text: &str, allowed_source_refs: &[String]) -> Result<Self, CodexError> {
        Self::parse_and_validate(text, allowed_source_refs)
    }
}

impl StructuredResponse for StructuredMemoryCandidateGeneration {
    fn parse_response(text: &str, allowed_source_refs: &[String]) -> Result<Self, CodexError> {
        Self::parse_and_validate(text, allowed_source_refs)
    }
}

const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
const LOGIN_TIMEOUT: Duration = Duration::from_secs(600);
const TURN_TIMEOUT: Duration = Duration::from_secs(180);
const CANCEL_POLL_INTERVAL: Duration = Duration::from_millis(200);
const MAX_EARLY_TURN_NOTIFICATIONS: usize = 128;
const MAX_EARLY_TURN_NOTIFICATION_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum CodexError {
    #[error("Codex app-server protocol failed: {0}")]
    Protocol(&'static str),
    #[error("Codex app-server process failed: {0}")]
    Process(&'static str),
    #[error("Codex request timed out")]
    Timeout,
    #[error("Codex request was cancelled by the host")]
    Cancelled,
    #[error("pinned Codex runtime did not match its manifest")]
    RuntimeMismatch,
    #[error("Codex runtime path is not an exact canonical file or directory")]
    InvalidPath,
    #[error("the required macOS process sandbox is unavailable")]
    SandboxUnavailable,
    #[error("app-specific CODEX_HOME config does not match the security contract")]
    ConfigMismatch,
    #[error("app-specific CODEX_HOME contains auth.json; keyring-only mode requires its absence")]
    CredentialFilePresent,
    #[error("Codex returned unsupported account credentials")]
    UnsupportedAccount,
    #[error("Codex runtime purpose does not authorize this route")]
    WrongRuntimePurpose,
    #[error("required GPT model or reasoning effort is unavailable")]
    RequiredModelUnavailable,
    #[error("Codex requested client-side authority: {0}")]
    AuthorityRequest(String),
    #[error("structured contract failed: {0}")]
    InvalidContract(&'static str),
    #[error("Codex remote error {code}: {message}")]
    Remote { code: i64, message: String },
    #[error("Codex turn failed: {0}")]
    TurnFailed(&'static str),
    #[error("local I/O failed")]
    Io(#[source] std::io::Error),
}

pub struct CodexClient {
    transport: Transport,
    next_id: i64,
    codex_home: PathBuf,
    model_workspace: PathBuf,
    cancel: Arc<AtomicBool>,
    early_turn_notifications: VecDeque<(String, Value, usize)>,
    early_turn_notification_bytes: usize,
    process_identifier: i32,
    protocol_initialized: bool,
    initialized: bool,
    purpose: RuntimePurpose,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RuntimePurpose {
    Model,
    LoginOnly,
}

impl CodexClient {
    /// Spawns the exact pinned runtime under the outer macOS sandbox and
    /// completes the stable initialize/initialized handshake.
    ///
    /// # Errors
    ///
    /// Returns an error for any runtime, sandbox, environment, or protocol
    /// mismatch and terminates the child.
    pub fn spawn(config: &CodexRuntimeConfig) -> Result<Self, CodexError> {
        Self::spawn_with_cancel(config, Arc::new(AtomicBool::new(false)))
    }

    /// Spawns the pinned runtime with a host-owned cancellation signal. The
    /// client polls the signal while waiting on app-server so global Off can
    /// terminate an in-flight model operation.
    ///
    /// # Errors
    ///
    /// Returns the same fail-closed errors as [`Self::spawn`].
    pub fn spawn_with_cancel(
        config: &CodexRuntimeConfig,
        cancel: Arc<AtomicBool>,
    ) -> Result<Self, CodexError> {
        let mut client = Self::spawn_uninitialized_with_cancel(config, cancel)?;
        if let Err(error) = client.complete_initialize() {
            client.transport.terminate();
            return Err(error);
        }
        Ok(client)
    }

    /// Spawns the exact pinned sandboxed process without sending any app-server
    /// request. The caller must first bind its process incarnation into the
    /// protected broker lease, then call [`Self::complete_initialize`].
    ///
    /// # Errors
    ///
    /// Returns an error for any runtime, sandbox, environment, or cancellation
    /// mismatch and never performs model or account work.
    pub fn spawn_uninitialized_with_cancel(
        config: &CodexRuntimeConfig,
        cancel: Arc<AtomicBool>,
    ) -> Result<Self, CodexError> {
        Self::spawn_uninitialized_for_purpose(config, cancel, RuntimePurpose::Model)
    }

    /// Spawns a short-lived login-only runtime. Its outer sandbox adds write
    /// access only to the exact canonical login Keychain database. This
    /// process must never be reused for account reads, model discovery, or
    /// model turns.
    ///
    /// # Errors
    ///
    /// Returns an error for the same fail-closed preconditions as the normal
    /// uninitialized runtime.
    pub fn spawn_login_uninitialized_with_cancel(
        config: &CodexRuntimeConfig,
        cancel: Arc<AtomicBool>,
    ) -> Result<Self, CodexError> {
        Self::spawn_uninitialized_for_purpose(config, cancel, RuntimePurpose::LoginOnly)
    }

    fn spawn_uninitialized_for_purpose(
        config: &CodexRuntimeConfig,
        cancel: Arc<AtomicBool>,
        purpose: RuntimePurpose,
    ) -> Result<Self, CodexError> {
        if cancel.load(Ordering::Acquire) {
            return Err(CodexError::Cancelled);
        }
        let codex_home = config.codex_home.clone();
        let model_workspace = config.model_workspace.clone();
        let keychain_access = match purpose {
            RuntimePurpose::Model => process::LoginKeychainAccess::ReadOnly,
            RuntimePurpose::LoginOnly => process::LoginKeychainAccess::LoginWriteOnly,
        };
        let (transport, process_identifier) = process::spawn(config, keychain_access)?;
        Ok(Self {
            transport,
            next_id: 1,
            codex_home,
            model_workspace,
            cancel,
            early_turn_notifications: VecDeque::new(),
            early_turn_notification_bytes: 0,
            process_identifier,
            protocol_initialized: false,
            initialized: false,
            purpose,
        })
    }

    #[must_use]
    pub const fn process_identifier(&self) -> i32 {
        self.process_identifier
    }

    #[must_use]
    pub const fn is_initialized(&self) -> bool {
        self.initialized
    }

    #[must_use]
    pub const fn is_login_only(&self) -> bool {
        matches!(self.purpose, RuntimePurpose::LoginOnly)
    }

    /// Permanently transfers signal authority for this exact process
    /// incarnation to the protected broker lease. After this call, client
    /// failure and drop close stdin and reap asynchronously but never signal a
    /// numeric PID.
    pub fn mark_process_lease_bound(&mut self) {
        self.transport.mark_process_lease_bound();
    }

    /// Completes the stable initialize/initialized handshake after the caller
    /// has installed the exact protected process lease.
    ///
    /// # Errors
    ///
    /// Returns an error for duplicate initialization, cancellation, transport,
    /// or protocol mismatches and terminates no unrelated process.
    pub fn complete_initialize(&mut self) -> Result<(), CodexError> {
        if self.initialized {
            return Err(CodexError::Protocol("Codex already initialized"));
        }
        let result = self.request(
            "initialize",
            &json!({
                "capabilities": {
                    "experimentalApi": false,
                    "mcpServerOpenaiFormElicitation": false,
                    "requestAttestation": false
                },
                "clientInfo": {"name": "OpenOpen", "title": "OpenOpen", "version": "0.1.0"}
            }),
            REQUEST_TIMEOUT,
        )?;
        let expected_home = self.codex_home.to_str().ok_or(CodexError::InvalidPath)?;
        if result.get("codexHome").and_then(Value::as_str) != Some(expected_home)
            || result.get("platformFamily").and_then(Value::as_str) != Some("unix")
            || result.get("platformOs").and_then(Value::as_str) != Some("macos")
        {
            return Err(CodexError::Protocol("initialize response mismatch"));
        }
        self.transport.send_notification("initialized", None)?;
        self.protocol_initialized = true;
        let verification = (|| {
            let effective = self.request(
                "config/read",
                &json!({"includeLayers": false}),
                REQUEST_TIMEOUT,
            )?;
            validate_effective_security_config(&effective)?;
            process::ensure_credential_file_absent(&self.codex_home)
        })();
        if let Err(error) = verification {
            self.protocol_initialized = false;
            self.initialized = false;
            return self.fail(error);
        }
        self.initialized = true;
        Ok(())
    }

    /// Begins managed `ChatGPT` OAuth. No token-bearing login variant is exposed.
    ///
    /// # Errors
    ///
    /// Returns an error for a non-ChatGPT or malformed response.
    pub fn begin_chatgpt_login(&mut self) -> Result<ChatGptLogin, CodexError> {
        self.require_purpose(RuntimePurpose::LoginOnly)?;
        let result = self.request(
            "account/login/start",
            &json!({"type": "chatgpt"}),
            REQUEST_TIMEOUT,
        )?;
        if result.get("type").and_then(Value::as_str) != Some("chatgpt") {
            return Err(CodexError::UnsupportedAccount);
        }
        let auth_url = result
            .get("authUrl")
            .and_then(Value::as_str)
            .filter(|url| url.starts_with("https://"))
            .ok_or(CodexError::Protocol("invalid ChatGPT auth URL"))?;
        let login_id = result
            .get("loginId")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty() && id.len() <= 256)
            .ok_or(CodexError::Protocol("invalid ChatGPT login id"))?;
        Ok(ChatGptLogin {
            auth_url: auth_url.to_owned(),
            login_id: login_id.to_owned(),
        })
    }

    /// Waits only for the matching managed-login completion. Account state is
    /// deliberately not read in this login-only process; the caller must
    /// destroy it and use a fresh read-only model runtime.
    ///
    /// # Errors
    ///
    /// Returns an error for mismatched login IDs, unsuccessful login, server
    /// authority requests, timeout, or a non-ChatGPT account.
    pub fn await_chatgpt_login(&mut self, login_id: &str) -> Result<(), CodexError> {
        self.require_purpose(RuntimePurpose::LoginOnly)?;
        let deadline = Instant::now() + LOGIN_TIMEOUT;
        loop {
            match self.recv_until(deadline)? {
                Incoming::Notification { method, params }
                    if method == "account/login/completed" =>
                {
                    if params.get("loginId").and_then(Value::as_str) != Some(login_id)
                        || params.get("success").and_then(Value::as_bool) != Some(true)
                    {
                        return self.fail(CodexError::Protocol("login completion mismatch"));
                    }
                    return Ok(());
                }
                Incoming::Notification { .. } => {}
                Incoming::Request { id, method } => {
                    self.transport.send_server_rejection(&id, &method)?;
                    return self.fail(CodexError::AuthorityRequest(method));
                }
                Incoming::Response { .. } => {
                    return self.fail(CodexError::Protocol("unexpected response during login"));
                }
            }
        }
    }

    /// Reads sanitized account state without exposing credentials or tokens.
    ///
    /// # Errors
    ///
    /// Returns an error if app-server reports API-key, Bedrock, or malformed
    /// account state.
    pub fn read_account(&mut self) -> Result<AccountState, CodexError> {
        self.require_purpose(RuntimePurpose::Model)?;
        self.read_account_inner()
    }

    fn read_account_inner(&mut self) -> Result<AccountState, CodexError> {
        let result = self.request("account/read", &json!({}), REQUEST_TIMEOUT)?;
        let Some(account) = result.get("account").filter(|value| !value.is_null()) else {
            return Ok(AccountState::NotConnected);
        };
        if account.get("type").and_then(Value::as_str) != Some("chatgpt") {
            return Err(CodexError::UnsupportedAccount);
        }
        let email = account
            .get("email")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .chars()
            .take(320)
            .collect::<String>();
        let plan_type = account
            .get("planType")
            .and_then(Value::as_str)
            .filter(|plan| !plan.is_empty() && plan.len() <= 128)
            .ok_or(CodexError::Protocol("invalid ChatGPT plan"))?;
        Ok(AccountState::ChatGpt {
            email,
            plan_type: plan_type.to_owned(),
        })
    }

    /// Lists every non-hidden GPT model through bounded stable pagination.
    ///
    /// # Errors
    ///
    /// Returns an error for partial pagination, cursor cycles, duplicate model
    /// conflicts, or an oversized catalog.
    pub fn list_gpt_models(&mut self) -> Result<Vec<GptModel>, CodexError> {
        self.require_purpose(RuntimePurpose::Model)?;
        let mut cursor: Option<String> = None;
        let mut seen_cursors = HashSet::new();
        let mut models = HashMap::<String, GptModel>::new();
        let mut catalog_bytes = 0_usize;
        for _ in 0..MAX_MODEL_PAGES {
            let mut params = json!({"includeHidden": false, "limit": 100});
            if let Some(cursor) = &cursor {
                params["cursor"] = Value::String(cursor.clone());
            }
            let result = self.request("model/list", &params, REQUEST_TIMEOUT)?;
            let data = result
                .get("data")
                .and_then(Value::as_array)
                .ok_or(CodexError::Protocol("model list data missing"))?;
            for entry in data {
                if let Some(model) = model_from_value(entry)? {
                    match models.get(&model.id) {
                        Some(existing) if existing != &model => {
                            return Err(CodexError::Protocol("conflicting duplicate model"));
                        }
                        Some(_) => {}
                        None => {
                            let model_bytes = model
                                .id
                                .len()
                                .checked_add(model.display_name.len())
                                .and_then(|total| {
                                    model
                                        .supported_reasoning_efforts
                                        .iter()
                                        .try_fold(total, |total, effort| {
                                            total.checked_add(effort.len())
                                        })
                                })
                                .ok_or(CodexError::Protocol("model catalog too large"))?;
                            catalog_bytes = catalog_bytes
                                .checked_add(model_bytes)
                                .filter(|total| *total <= MAX_MODEL_CATALOG_BYTES)
                                .ok_or(CodexError::Protocol("model catalog too large"))?;
                            models.insert(model.id.clone(), model);
                            if models.len() > MAX_MODELS {
                                return Err(CodexError::Protocol("model catalog too large"));
                            }
                        }
                    }
                }
            }
            cursor = result
                .get("nextCursor")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned);
            let Some(next) = &cursor else {
                let mut values = models.into_values().collect::<Vec<_>>();
                values.sort_by(|left, right| left.id.cmp(&right.id));
                return Ok(values);
            };
            if next.is_empty() || next.len() > MAX_MODEL_CURSOR_BYTES {
                return Err(CodexError::Protocol("invalid model cursor"));
            }
            if !seen_cursors.insert(next.clone()) {
                return Err(CodexError::Protocol("model cursor cycle"));
            }
        }
        Err(CodexError::Protocol("model pagination exceeded limit"))
    }

    /// Returns the stable content binding for the complete, sorted compatible
    /// catalog returned by [`Self::list_gpt_models`]. Host persists this value
    /// with the explicit owner selection and rejects catalog drift before a
    /// turn can begin.
    ///
    /// # Errors
    ///
    /// Returns an error when the complete catalog cannot be serialized into
    /// the pinned canonical binding.
    pub fn model_catalog_fingerprint(models: &[GptModel]) -> Result<String, CodexError> {
        let encoded = serde_json::to_vec(models)
            .map_err(|_| CodexError::Protocol("invalid model catalog"))?;
        Ok(format!("{:x}", Sha256::digest(encoded)))
    }

    /// A deterministic numeric projection of the full catalog fingerprint.
    /// The fingerprint remains the authoritative binding; the revision is a
    /// stable display/storage version for the exact same catalog bytes.
    ///
    /// # Errors
    ///
    /// Returns an error when the supplied catalog fingerprint has no valid
    /// hexadecimal revision prefix.
    pub fn model_catalog_revision(fingerprint: &str) -> Result<u64, CodexError> {
        let prefix = fingerprint
            .get(..16)
            .ok_or(CodexError::Protocol("invalid model catalog fingerprint"))?;
        let revision = u64::from_str_radix(prefix, 16)
            .map_err(|_| CodexError::Protocol("invalid model catalog fingerprint"))?;
        Ok(revision.max(1))
    }

    /// Runs the sealed outcome contract with Host's explicit catalog-bound
    /// model selection. Any tool/action item, reroute, scope mismatch, or
    /// malformed output terminates the child and returns no result.
    ///
    /// # Errors
    ///
    /// Returns an error for unavailable model access, containment mismatch,
    /// action/tool requests, failed turns, or schema-invalid output.
    pub fn run_structured_outcome(
        &mut self,
        request: &OutcomeRequest,
    ) -> Result<StructuredOutcome, CodexError> {
        let workspace = self.model_workspace.clone();
        self.run_structured_in_workspace(request, &workspace)
    }

    /// Runs the sealed Choice-generation contract with the exact same
    /// read-only/tool-free containment as an Outcome, but returns only
    /// bounded understanding and three directions rather than effect authority.
    ///
    /// # Errors
    ///
    /// Returns an error for unavailable model access, containment mismatch,
    /// tool/action requests, failed turns, or schema-invalid output.
    pub fn run_structured_choice_generation(
        &mut self,
        request: &ChoiceGenerationRequest,
    ) -> Result<StructuredChoiceGeneration, CodexError> {
        let workspace = self.model_workspace.clone();
        self.run_structured_in_workspace(request, &workspace)
    }

    /// Runs the sealed Memory-candidate contract with the selected catalog-
    /// bound model. The turn remains read-only, network-disabled, tool-free,
    /// and can return only one to three source-bound candidate records.
    ///
    /// # Errors
    ///
    /// Returns an error for unavailable selected-model access, containment
    /// mismatch, action/tool requests, failed turns, or invalid output.
    pub fn run_structured_memory_candidate_generation(
        &mut self,
        request: &MemoryCandidateGenerationRequest,
    ) -> Result<StructuredMemoryCandidateGeneration, CodexError> {
        let workspace = self.model_workspace.clone();
        self.run_structured_in_workspace(request, &workspace)
    }

    /// Runs the sealed Memory-candidate contract in an already-contained
    /// read-only workspace.
    ///
    /// # Errors
    ///
    /// Returns an error when the workspace escapes containment or the request,
    /// selected model, or structured result violates the sealed contract.
    pub fn run_structured_memory_candidate_generation_in_workspace(
        &mut self,
        request: &MemoryCandidateGenerationRequest,
        workspace: &Path,
    ) -> Result<StructuredMemoryCandidateGeneration, CodexError> {
        self.run_structured_in_workspace(request, workspace)
    }

    /// # Errors
    ///
    /// Returns an error when the supplied workspace or the sealed Outcome
    /// request violates the same bounded model contract as the default root.
    pub fn run_structured_outcome_in_workspace(
        &mut self,
        request: &OutcomeRequest,
        workspace: &Path,
    ) -> Result<StructuredOutcome, CodexError> {
        self.run_structured_in_workspace(request, workspace)
    }

    /// # Errors
    ///
    /// Returns an error when the supplied workspace or the sealed Choice
    /// generation request violates the bounded model contract.
    pub fn run_structured_choice_generation_in_workspace(
        &mut self,
        request: &ChoiceGenerationRequest,
        workspace: &Path,
    ) -> Result<StructuredChoiceGeneration, CodexError> {
        self.run_structured_in_workspace(request, workspace)
    }

    /// Runs one structured outcome in an exact subdirectory already contained
    /// by the immutable outer sandbox root.
    ///
    /// # Errors
    ///
    /// Returns an error when the workspace escapes the immutable sandbox root,
    /// required model access is unavailable, the runtime violates containment,
    /// or the structured result fails the sealed outcome contract.
    fn run_structured_in_workspace<R, T>(
        &mut self,
        request: &R,
        workspace: &Path,
    ) -> Result<T, CodexError>
    where
        R: StructuredRequest,
        T: StructuredResponse,
    {
        self.require_purpose(RuntimePurpose::Model)?;
        request.validate_request()?;
        let workspace = std::fs::canonicalize(workspace).map_err(CodexError::Io)?;
        if !workspace.starts_with(&self.model_workspace) {
            return Err(CodexError::InvalidPath);
        }
        let models = self.list_gpt_models()?;
        let selected_model = request
            .selected_model()
            .ok_or(CodexError::RequiredModelUnavailable)?;
        let selected = models
            .iter()
            .find(|model| model.id == selected_model.model_id)
            .ok_or(CodexError::RequiredModelUnavailable)?;
        let catalog_fingerprint = Self::model_catalog_fingerprint(&models)?;
        if catalog_fingerprint != selected_model.catalog_fingerprint
            || Self::model_catalog_revision(&catalog_fingerprint)?
                != selected_model.catalog_revision
            || !selected_model_matches_catalog(selected, selected_model)
        {
            return Err(CodexError::RequiredModelUnavailable);
        }
        let cwd = workspace
            .to_str()
            .ok_or(CodexError::InvalidPath)?
            .to_owned();
        let thread = self.request(
            "thread/start",
            &json!({
                "approvalPolicy": "never",
                "config": {"web_search": "disabled"},
                "cwd": cwd,
                "developerInstructions": request.developer_instructions(),
                "ephemeral": true,
                "model": selected_model.model_id,
                "sandbox": "read-only"
            }),
            REQUEST_TIMEOUT,
        )?;
        validate_thread(&thread, &cwd, &selected_model.model_id)?;
        let thread_id = thread
            .pointer("/thread/id")
            .and_then(Value::as_str)
            .ok_or(CodexError::Protocol("thread id missing"))?
            .to_owned();

        self.start_turn_and_collect(&thread_id, &cwd, request)
    }

    fn require_purpose(&self, purpose: RuntimePurpose) -> Result<(), CodexError> {
        if self.purpose == purpose {
            Ok(())
        } else {
            Err(CodexError::WrongRuntimePurpose)
        }
    }

    fn start_turn_and_collect<R, T>(
        &mut self,
        thread_id: &str,
        cwd: &str,
        request: &R,
    ) -> Result<T, CodexError>
    where
        R: StructuredRequest,
        T: StructuredResponse,
    {
        let result = self.start_turn_and_collect_inner(thread_id, cwd, request);
        match result {
            Ok(outcome) => Ok(outcome),
            Err(error) => self.fail(error),
        }
    }

    fn start_turn_and_collect_inner<R, T>(
        &mut self,
        thread_id: &str,
        cwd: &str,
        request: &R,
    ) -> Result<T, CodexError>
    where
        R: StructuredRequest,
        T: StructuredResponse,
    {
        let selected_model = request
            .selected_model()
            .ok_or(CodexError::RequiredModelUnavailable)?;
        let mut turn_params = json!({
            "approvalPolicy": "never",
            "cwd": cwd,
            "input": [{"text": request.prompt(), "type": "text"}],
            "model": selected_model.model_id,
            "outputSchema": request.schema(),
            "sandboxPolicy": {"networkAccess": false, "type": "readOnly"},
            "threadId": thread_id
        });
        if let Some(effort) = &selected_model.reasoning_effort {
            turn_params["effort"] = Value::String(effort.clone());
        }
        let response = self.request("turn/start", &turn_params, REQUEST_TIMEOUT)?;
        validate_exact_object_keys(&response, &["turn"], "invalid turn start response")?;
        let validated = validate_turn(
            response
                .get("turn")
                .ok_or(CodexError::Protocol("turn start response missing"))?,
            None,
            TurnValidationPhase::StartResponse,
        )?;
        self.collect_structured_inner(thread_id, &validated.id, request.allowed_source_refs())
    }

    #[cfg(test)]
    fn collect_outcome(
        &mut self,
        thread_id: &str,
        turn_id: &str,
        allowed_source_refs: &[String],
    ) -> Result<StructuredOutcome, CodexError> {
        let result = self.collect_structured_inner(thread_id, turn_id, allowed_source_refs);
        match result {
            Ok(outcome) => Ok(outcome),
            Err(error) => self.fail(error),
        }
    }

    fn collect_structured_inner<T: StructuredResponse>(
        &mut self,
        thread_id: &str,
        turn_id: &str,
        allowed_source_refs: &[String],
    ) -> Result<T, CodexError> {
        let deadline = Instant::now() + TURN_TIMEOUT;
        let mut turn = TurnAccumulator::default();
        loop {
            match self.recv_turn_until(deadline)? {
                Incoming::Request { id, method } => {
                    self.transport.send_server_rejection(&id, &method)?;
                    return self.fail(CodexError::AuthorityRequest(method));
                }
                Incoming::Response { .. } => {
                    return self.fail(CodexError::Protocol("unexpected response during turn"));
                }
                Incoming::Notification { method, params } => match method.as_str() {
                    "model/rerouted" => {
                        return self.fail(CodexError::Protocol("model rerouted"));
                    }
                    "item/started" | "item/completed" => {
                        validate_item_lifecycle_notification(
                            &method, &params, thread_id, turn_id, &mut turn,
                        )?;
                    }
                    "item/agentMessage/delta"
                    | "item/reasoning/summaryTextDelta"
                    | "item/reasoning/summaryPartAdded"
                    | "item/reasoning/textDelta"
                    | "thread/tokenUsage/updated"
                    | "account/rateLimits/updated"
                    | "model/verification"
                    | "model/safetyBuffering/updated"
                    | "turn/started"
                    | "thread/status/changed" => {
                        validate_passive_turn_notification(
                            &method, &params, thread_id, turn_id, &mut turn,
                        )?;
                    }
                    "turn/completed" => {
                        validate_exact_object_keys(
                            &params,
                            &["threadId", "turn"],
                            "invalid turn completed notification",
                        )?;
                        if params.get("threadId").and_then(Value::as_str) != Some(thread_id) {
                            return self.fail(CodexError::Protocol("turn completion mismatch"));
                        }
                        turn = validate_turn(
                            params
                                .get("turn")
                                .ok_or(CodexError::Protocol("completed turn missing"))?,
                            Some(turn_id),
                            TurnValidationPhase::CompletedNotification,
                        )?
                        .items;
                        let text = match turn.final_messages.as_slice() {
                            [only] => only.as_str(),
                            [] => turn
                                .phase_unknown
                                .last()
                                .map(String::as_str)
                                .ok_or(CodexError::Protocol("final answer missing"))?,
                            _ => {
                                return self.fail(CodexError::Protocol("multiple final answers"));
                            }
                        };
                        return T::parse_response(text, allowed_source_refs);
                    }
                    "error" => {
                        validate_turn_identity(&params, thread_id, turn_id)?;
                        let class =
                            sanitized_turn_error_class(params.pointer("/error/codexErrorInfo"));
                        return self.fail(CodexError::TurnFailed(class));
                    }
                    _ => {
                        return self.fail(CodexError::Protocol("unexpected turn notification"));
                    }
                },
            }
        }
    }

    fn request(
        &mut self,
        method: &str,
        params: &Value,
        timeout: Duration,
    ) -> Result<Value, CodexError> {
        if !request_allowed(self.protocol_initialized, self.initialized, method) {
            return Err(CodexError::Protocol("Codex lease is not initialized"));
        }
        if self.cancel.load(Ordering::Acquire) {
            return self.fail(CodexError::Cancelled);
        }
        process::ensure_credential_file_absent(&self.codex_home)?;
        let id = self.next_id;
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or(CodexError::Protocol("request id overflow"))?;
        self.transport.send_request(id, method, params)?;
        let deadline = Instant::now() + timeout;
        loop {
            match self.recv_until(deadline)? {
                Incoming::Response {
                    id: response_id,
                    result,
                    error,
                } if response_id == id => {
                    process::ensure_credential_file_absent(&self.codex_home)?;
                    if let Some(error) = error {
                        return Err(CodexError::Remote {
                            code: error.code,
                            message: error.message,
                        });
                    }
                    return result.ok_or(CodexError::Protocol("response result missing"));
                }
                Incoming::Response { .. } => {
                    return self.fail(CodexError::Protocol("unknown response id"));
                }
                Incoming::Request { id, method } => {
                    self.transport.send_server_rejection(&id, &method)?;
                    return self.fail(CodexError::AuthorityRequest(method));
                }
                Incoming::Notification {
                    method: notification_method,
                    params,
                } if method == "turn/start" => {
                    let bytes = notification_method
                        .len()
                        .checked_add(
                            serde_json::to_vec(&params)
                                .map_err(|_| CodexError::Protocol("notification encoding failed"))?
                                .len(),
                        )
                        .ok_or(CodexError::Protocol("early notification overflow"))?;
                    let next_total = self
                        .early_turn_notification_bytes
                        .checked_add(bytes)
                        .ok_or(CodexError::Protocol("early notification overflow"))?;
                    if self.early_turn_notifications.len() >= MAX_EARLY_TURN_NOTIFICATIONS
                        || next_total > MAX_EARLY_TURN_NOTIFICATION_BYTES
                    {
                        return self
                            .fail(CodexError::Protocol("too many early turn notifications"));
                    }
                    self.early_turn_notification_bytes = next_total;
                    self.early_turn_notifications
                        .push_back((notification_method, params, bytes));
                }
                Incoming::Notification { .. } => {}
            }
        }
    }

    fn fail<T>(&mut self, error: CodexError) -> Result<T, CodexError> {
        self.transport.terminate();
        Err(error)
    }

    fn recv_until(&mut self, deadline: Instant) -> Result<Incoming, CodexError> {
        loop {
            if self.cancel.load(Ordering::Acquire) {
                return self.fail(CodexError::Cancelled);
            }
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .ok_or(CodexError::Timeout)?;
            process::ensure_credential_file_absent(&self.codex_home)?;
            let result = self.transport.recv(remaining.min(CANCEL_POLL_INTERVAL));
            process::ensure_credential_file_absent(&self.codex_home)?;
            if !matches!(result, Err(CodexError::Timeout)) || Instant::now() >= deadline {
                return result;
            }
        }
    }

    fn recv_turn_until(&mut self, deadline: Instant) -> Result<Incoming, CodexError> {
        if let Some((method, params, bytes)) = self.early_turn_notifications.pop_front() {
            self.early_turn_notification_bytes =
                self.early_turn_notification_bytes.saturating_sub(bytes);
            return Ok(Incoming::Notification { method, params });
        }
        self.recv_until(deadline)
    }
}

fn validate_effective_security_config(result: &Value) -> Result<(), CodexError> {
    let config = result
        .get("config")
        .and_then(Value::as_object)
        .ok_or(CodexError::Protocol("effective config missing"))?;
    for (key, expected) in [
        ("forced_login_method", "chatgpt"),
        ("cli_auth_credentials_store", "keyring"),
        ("mcp_oauth_credentials_store", "keyring"),
    ] {
        if config.get(key).and_then(Value::as_str) != Some(expected) {
            return Err(CodexError::ConfigMismatch);
        }
    }
    if config
        .get("features")
        .and_then(Value::as_object)
        .and_then(|features| features.get("secret_auth_storage"))
        .and_then(Value::as_bool)
        != Some(false)
    {
        return Err(CodexError::ConfigMismatch);
    }
    Ok(())
}

fn request_allowed(
    protocol_initialized: bool,
    security_config_verified: bool,
    method: &str,
) -> bool {
    if !protocol_initialized {
        return method == "initialize";
    }
    security_config_verified || method == "config/read"
}

fn validate_thread(
    result: &Value,
    expected_cwd: &str,
    expected_model: &str,
) -> Result<(), CodexError> {
    if result.get("model").and_then(Value::as_str) != Some(expected_model)
        || result.get("modelProvider").and_then(Value::as_str) != Some("openai")
        || result.get("cwd").and_then(Value::as_str) != Some(expected_cwd)
        || result.get("approvalPolicy").and_then(Value::as_str) != Some("never")
        || result.pointer("/sandbox/type").and_then(Value::as_str) != Some("readOnly")
        || result
            .pointer("/sandbox/networkAccess")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    {
        return Err(CodexError::Protocol("thread containment mismatch"));
    }
    if result
        .get("instructionSources")
        .and_then(Value::as_array)
        .is_some_and(|sources| !sources.is_empty())
    {
        return Err(CodexError::Protocol("unexpected instruction source"));
    }
    Ok(())
}

fn validate_turn_identity(
    params: &Value,
    thread_id: &str,
    turn_id: &str,
) -> Result<(), CodexError> {
    if params.get("threadId").and_then(Value::as_str) == Some(thread_id)
        && params.get("turnId").and_then(Value::as_str) == Some(turn_id)
    {
        Ok(())
    } else {
        Err(CodexError::Protocol("turn item identity mismatch"))
    }
}

fn validate_item_lifecycle_notification(
    method: &str,
    params: &Value,
    thread_id: &str,
    turn_id: &str,
    turn: &mut TurnAccumulator,
) -> Result<(), CodexError> {
    validate_turn_identity(params, thread_id, turn_id)?;
    let timestamp = match method {
        "item/started" => "startedAtMs",
        "item/completed" => "completedAtMs",
        _ => return Err(CodexError::Protocol("unexpected item lifecycle method")),
    };
    if params.get(timestamp).and_then(Value::as_i64).is_none() {
        return Err(CodexError::Protocol("item lifecycle timestamp missing"));
    }
    inspect_item(
        params
            .get("item")
            .ok_or(CodexError::Protocol("turn item missing"))?,
        turn,
    )
}

const MAX_TURN_ITEMS: usize = 128;
const MAX_TURN_ACCUMULATED_TEXT_BYTES: usize = 1024 * 1024;
const MAX_PASSIVE_TURN_NOTIFICATIONS: usize = 4096;
const MAX_PASSIVE_TURN_NOTIFICATION_BYTES: usize = 1024 * 1024;
const MAX_PASSIVE_DELTA_BYTES: usize = 64 * 1024;
const MAX_TURN_ITEM_ID_BYTES: usize = 256;
const MAX_TURN_STREAM_INDEX: u64 = 4096;

#[derive(Default)]
struct TurnAccumulator {
    final_messages: Vec<String>,
    phase_unknown: Vec<String>,
    item_count: usize,
    text_bytes: usize,
    passive_notification_count: usize,
    passive_notification_bytes: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TurnValidationPhase {
    StartResponse,
    StartedNotification,
    CompletedNotification,
}

struct ValidatedTurn {
    id: String,
    items: TurnAccumulator,
}

fn validate_passive_turn_notification(
    method: &str,
    params: &Value,
    thread_id: &str,
    turn_id: &str,
    turn: &mut TurnAccumulator,
) -> Result<(), CodexError> {
    record_passive_turn_notification(params, turn)?;
    validate_passive_turn_scope(method, params, thread_id, turn_id)?;
    validate_passive_turn_payload(method, params)
}

fn record_passive_turn_notification(
    params: &Value,
    turn: &mut TurnAccumulator,
) -> Result<(), CodexError> {
    let encoded_len = serde_json::to_vec(params)
        .map_err(|_| CodexError::Protocol("notification encoding failed"))?
        .len();
    turn.passive_notification_count = turn
        .passive_notification_count
        .checked_add(1)
        .ok_or(CodexError::Protocol("passive notification count overflow"))?;
    turn.passive_notification_bytes = turn
        .passive_notification_bytes
        .checked_add(encoded_len)
        .ok_or(CodexError::Protocol("passive notification size overflow"))?;
    if turn.passive_notification_count > MAX_PASSIVE_TURN_NOTIFICATIONS
        || turn.passive_notification_bytes > MAX_PASSIVE_TURN_NOTIFICATION_BYTES
    {
        return Err(CodexError::Protocol("passive notification limit exceeded"));
    }
    Ok(())
}

fn validate_passive_turn_scope(
    method: &str,
    params: &Value,
    thread_id: &str,
    turn_id: &str,
) -> Result<(), CodexError> {
    match method {
        "item/agentMessage/delta"
        | "item/reasoning/summaryTextDelta"
        | "item/reasoning/summaryPartAdded"
        | "item/reasoning/textDelta"
        | "thread/tokenUsage/updated"
        | "model/verification"
        | "model/safetyBuffering/updated" => {
            validate_turn_identity(params, thread_id, turn_id)?;
        }
        "turn/started" => {
            validate_exact_object_keys(
                params,
                &["threadId", "turn"],
                "invalid turn started notification",
            )?;
            if params.get("threadId").and_then(Value::as_str) != Some(thread_id) {
                return Err(CodexError::Protocol("turn start identity mismatch"));
            }
            validate_turn(
                params
                    .get("turn")
                    .ok_or(CodexError::Protocol("turn start missing"))?,
                Some(turn_id),
                TurnValidationPhase::StartedNotification,
            )?;
        }
        "thread/status/changed" => {
            if params.get("threadId").and_then(Value::as_str) != Some(thread_id) {
                return Err(CodexError::Protocol("thread status identity mismatch"));
            }
            validate_thread_status(params)?;
        }
        "account/rateLimits/updated" => {
            validate_rate_limits_notification(params)?;
        }
        _ => return Err(CodexError::Protocol("unexpected passive notification")),
    }
    Ok(())
}

fn validate_thread_status(params: &Value) -> Result<(), CodexError> {
    let status = params
        .get("status")
        .and_then(Value::as_object)
        .ok_or(CodexError::Protocol("invalid thread status"))?;
    match status.get("type").and_then(Value::as_str) {
        Some("active") => {
            let flags = status
                .get("activeFlags")
                .and_then(Value::as_array)
                .ok_or(CodexError::Protocol("invalid thread active flags"))?;
            if flags.iter().any(|flag| {
                !matches!(
                    flag.as_str(),
                    Some("waitingOnApproval" | "waitingOnUserInput")
                )
            }) {
                return Err(CodexError::Protocol("invalid thread active flags"));
            }
            if !flags.is_empty() {
                return Err(CodexError::Protocol("turn waiting for authority"));
            }
        }
        Some("idle") => {
            if status.contains_key("activeFlags") {
                return Err(CodexError::Protocol("invalid thread status"));
            }
        }
        Some("systemError") => return Err(CodexError::TurnFailed("system_error")),
        _ => return Err(CodexError::Protocol("invalid thread status")),
    }
    Ok(())
}

fn validate_rate_limits_notification(params: &Value) -> Result<(), CodexError> {
    let snapshot = params
        .get("rateLimits")
        .and_then(Value::as_object)
        .ok_or(CodexError::Protocol("invalid rate limit notification"))?;
    let valid = optional_nullable(snapshot, "limitId", Value::is_string)
        && optional_nullable(snapshot, "limitName", Value::is_string)
        && optional_nullable(snapshot, "primary", valid_rate_limit_window)
        && optional_nullable(snapshot, "secondary", valid_rate_limit_window)
        && optional_nullable(snapshot, "credits", valid_credits_snapshot)
        && optional_nullable(
            snapshot,
            "individualLimit",
            valid_spend_control_limit_snapshot,
        )
        && optional_nullable(snapshot, "planType", valid_plan_type)
        && optional_nullable(
            snapshot,
            "rateLimitReachedType",
            valid_rate_limit_reached_type,
        );
    if valid {
        Ok(())
    } else {
        Err(CodexError::Protocol("invalid rate limit notification"))
    }
}

fn optional_nullable(
    object: &serde_json::Map<String, Value>,
    key: &str,
    predicate: fn(&Value) -> bool,
) -> bool {
    object
        .get(key)
        .is_none_or(|value| value.is_null() || predicate(value))
}

fn valid_i32(value: &Value) -> bool {
    value
        .as_i64()
        .is_some_and(|number| i32::try_from(number).is_ok())
}

fn valid_optional_i64(object: &serde_json::Map<String, Value>, key: &str) -> bool {
    optional_nullable(object, key, |value| value.as_i64().is_some())
}

fn valid_rate_limit_window(value: &Value) -> bool {
    value.as_object().is_some_and(|window| {
        window.get("usedPercent").is_some_and(valid_i32)
            && valid_optional_i64(window, "windowDurationMins")
            && valid_optional_i64(window, "resetsAt")
    })
}

fn valid_credits_snapshot(value: &Value) -> bool {
    value.as_object().is_some_and(|credits| {
        credits.get("hasCredits").is_some_and(Value::is_boolean)
            && credits.get("unlimited").is_some_and(Value::is_boolean)
            && optional_nullable(credits, "balance", Value::is_string)
    })
}

fn valid_spend_control_limit_snapshot(value: &Value) -> bool {
    value.as_object().is_some_and(|limit| {
        limit.get("limit").is_some_and(Value::is_string)
            && limit.get("used").is_some_and(Value::is_string)
            && limit.get("remainingPercent").is_some_and(valid_i32)
            && limit.get("resetsAt").and_then(Value::as_i64).is_some()
    })
}

fn valid_plan_type(value: &Value) -> bool {
    matches!(
        value.as_str(),
        Some(
            "free"
                | "go"
                | "plus"
                | "pro"
                | "prolite"
                | "team"
                | "self_serve_business_usage_based"
                | "business"
                | "enterprise_cbp_usage_based"
                | "enterprise"
                | "edu"
                | "unknown"
        )
    )
}

fn valid_rate_limit_reached_type(value: &Value) -> bool {
    matches!(
        value.as_str(),
        Some(
            "rate_limit_reached"
                | "workspace_owner_credits_depleted"
                | "workspace_member_credits_depleted"
                | "workspace_owner_usage_limit_reached"
                | "workspace_member_usage_limit_reached"
        )
    )
}

fn validate_passive_turn_payload(method: &str, params: &Value) -> Result<(), CodexError> {
    match method {
        "item/agentMessage/delta"
        | "item/reasoning/summaryTextDelta"
        | "item/reasoning/textDelta" => {
            validate_stream_item_id(params)?;
            let delta = params
                .get("delta")
                .and_then(Value::as_str)
                .ok_or(CodexError::Protocol("stream delta missing"))?;
            if delta.len() > MAX_PASSIVE_DELTA_BYTES {
                return Err(CodexError::Protocol("stream delta limit exceeded"));
            }
        }
        "item/reasoning/summaryPartAdded" => validate_stream_item_id(params)?,
        "thread/tokenUsage/updated" => validate_token_usage(params)?,
        "model/verification" => {
            let verifications = params
                .get("verifications")
                .and_then(Value::as_array)
                .ok_or(CodexError::Protocol("model verifications missing"))?;
            if verifications.len() > 8
                || verifications
                    .iter()
                    .any(|entry| entry.as_str() != Some("trustedAccessForCyber"))
            {
                return Err(CodexError::Protocol("invalid model verification"));
            }
        }
        "model/safetyBuffering/updated" => validate_safety_buffering(params)?,
        _ => {}
    }

    match method {
        "item/reasoning/summaryTextDelta" | "item/reasoning/summaryPartAdded" => {
            validate_stream_index(params, "summaryIndex")?;
        }
        "item/reasoning/textDelta" => validate_stream_index(params, "contentIndex")?,
        _ => {}
    }
    Ok(())
}

fn validate_safety_buffering(params: &Value) -> Result<(), CodexError> {
    if !params
        .get("model")
        .and_then(Value::as_str)
        .is_some_and(valid_model_identifier)
        || params
            .get("showBufferingUi")
            .and_then(Value::as_bool)
            .is_none()
        || !bounded_string_array(params.get("useCases"), 16, 128)
        || !bounded_string_array(params.get("reasons"), 16, 256)
        || params.get("fasterModel").is_some_and(|value| {
            !value.is_null()
                && value
                    .as_str()
                    .is_none_or(|model| model.is_empty() || model.len() > 128 || !model.is_ascii())
        })
    {
        return Err(CodexError::Protocol(
            "invalid safety buffering notification",
        ));
    }
    Ok(())
}

fn selected_model_matches_catalog(model: &GptModel, selected: &SelectedModel) -> bool {
    match &selected.reasoning_effort {
        Some(effort) => {
            !model.supported_reasoning_efforts.is_empty()
                && model
                    .supported_reasoning_efforts
                    .iter()
                    .any(|candidate| candidate == effort)
        }
        None => model.supported_reasoning_efforts.is_empty(),
    }
}

fn valid_model_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn validate_stream_item_id(params: &Value) -> Result<(), CodexError> {
    let item_id = params
        .get("itemId")
        .and_then(Value::as_str)
        .ok_or(CodexError::Protocol("stream item id missing"))?;
    if item_id.is_empty() || item_id.len() > MAX_TURN_ITEM_ID_BYTES || item_id.contains('\0') {
        return Err(CodexError::Protocol("invalid stream item id"));
    }
    Ok(())
}

fn validate_stream_index(params: &Value, key: &str) -> Result<(), CodexError> {
    if params
        .get(key)
        .and_then(Value::as_u64)
        .is_none_or(|index| index > MAX_TURN_STREAM_INDEX)
    {
        return Err(CodexError::Protocol("invalid stream index"));
    }
    Ok(())
}

fn validate_token_usage(params: &Value) -> Result<(), CodexError> {
    for bucket in ["last", "total"] {
        let usage = params
            .pointer(&format!("/tokenUsage/{bucket}"))
            .and_then(Value::as_object)
            .ok_or(CodexError::Protocol("token usage bucket missing"))?;
        for key in [
            "cachedInputTokens",
            "inputTokens",
            "outputTokens",
            "reasoningOutputTokens",
            "totalTokens",
        ] {
            if usage
                .get(key)
                .and_then(Value::as_i64)
                .is_none_or(|value| value < 0)
            {
                return Err(CodexError::Protocol("invalid token usage"));
            }
        }
    }
    if params
        .pointer("/tokenUsage/modelContextWindow")
        .is_some_and(|value| !value.is_null() && value.as_i64().is_none_or(|size| size <= 0))
    {
        return Err(CodexError::Protocol("invalid model context window"));
    }
    Ok(())
}

fn bounded_string_array(value: Option<&Value>, max_items: usize, max_bytes: usize) -> bool {
    value.and_then(Value::as_array).is_some_and(|items| {
        items.len() <= max_items
            && items
                .iter()
                .all(|item| item.as_str().is_some_and(|text| text.len() <= max_bytes))
    })
}

fn sanitized_turn_error_class(value: Option<&Value>) -> &'static str {
    match value {
        Some(Value::String(class)) => match class.as_str() {
            "contextWindowExceeded" => "context_window_exceeded",
            "sessionBudgetExceeded" => "session_budget_exceeded",
            "usageLimitExceeded" => "usage_limit_exceeded",
            "serverOverloaded" => "server_overloaded",
            "cyberPolicy" => "cyber_policy",
            "internalServerError" => "internal_server_error",
            "unauthorized" => "unauthorized",
            "badRequest" => "bad_request",
            "sandboxError" => "sandbox_error",
            _ => "other",
        },
        Some(Value::Object(class)) if class.contains_key("httpConnectionFailed") => {
            "http_connection_failed"
        }
        Some(Value::Object(class)) if class.contains_key("responseStreamConnectionFailed") => {
            "response_stream_connection_failed"
        }
        Some(Value::Object(class)) if class.contains_key("responseStreamDisconnected") => {
            "response_stream_disconnected"
        }
        Some(Value::Object(class)) if class.contains_key("responseTooManyFailedAttempts") => {
            "response_too_many_failed_attempts"
        }
        _ => "other",
    }
}

fn validate_exact_object_keys(
    value: &Value,
    allowed: &[&str],
    error: &'static str,
) -> Result<(), CodexError> {
    let object = value.as_object().ok_or(CodexError::Protocol(error))?;
    if object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err(CodexError::Protocol(error));
    }
    Ok(())
}

fn validate_turn(
    turn: &Value,
    expected_turn_id: Option<&str>,
    phase: TurnValidationPhase,
) -> Result<ValidatedTurn, CodexError> {
    validate_exact_object_keys(
        turn,
        &[
            "completedAt",
            "durationMs",
            "error",
            "id",
            "items",
            "itemsView",
            "startedAt",
            "status",
        ],
        "invalid turn",
    )?;
    let object = turn
        .as_object()
        .ok_or(CodexError::Protocol("invalid turn"))?;
    let id = object
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty() && id.len() <= MAX_TURN_ITEM_ID_BYTES && !id.contains('\0'))
        .ok_or(CodexError::Protocol("invalid turn id"))?;
    if expected_turn_id.is_some_and(|expected| expected != id) {
        return Err(CodexError::Protocol("turn identity mismatch"));
    }
    if object
        .get("itemsView")
        .is_some_and(|items_view| items_view.as_str() != Some("full"))
    {
        return Err(CodexError::Protocol("incomplete turn items"));
    }
    for key in ["completedAt", "durationMs", "startedAt"] {
        if object
            .get(key)
            .is_some_and(|value| !value.is_null() && value.as_i64().is_none())
        {
            return Err(CodexError::Protocol("invalid turn lifecycle metadata"));
        }
    }
    let status = object
        .get("status")
        .and_then(Value::as_str)
        .ok_or(CodexError::Protocol("turn status missing"))?;
    let items = object
        .get("items")
        .and_then(Value::as_array)
        .ok_or(CodexError::Protocol("turn items missing"))?;
    if items.len() > MAX_TURN_ITEMS {
        return Err(CodexError::Protocol("turn item limit exceeded"));
    }
    let mut validated = TurnAccumulator::default();
    for item in items {
        inspect_item(item, &mut validated)?;
    }
    match phase {
        TurnValidationPhase::StartResponse | TurnValidationPhase::StartedNotification => {
            if status != "inProgress"
                || object
                    .get("completedAt")
                    .is_some_and(|value| !value.is_null())
                || object
                    .get("durationMs")
                    .is_some_and(|value| !value.is_null())
                || object.get("error").is_some_and(|error| !error.is_null())
            {
                return Err(CodexError::Protocol("invalid in-progress turn"));
            }
        }
        TurnValidationPhase::CompletedNotification => match status {
            "completed" => {
                if object.get("error").is_some_and(|error| !error.is_null()) {
                    return Err(CodexError::Protocol("completed turn contains error"));
                }
            }
            "failed" => {
                let error = object
                    .get("error")
                    .filter(|error| !error.is_null())
                    .ok_or(CodexError::Protocol("failed turn error missing"))?;
                validate_turn_error(error)?;
                return Err(CodexError::TurnFailed(sanitized_turn_error_class(
                    error.get("codexErrorInfo"),
                )));
            }
            "interrupted" => {
                if object.get("error").is_some_and(|error| !error.is_null()) {
                    return Err(CodexError::Protocol("interrupted turn contains error"));
                }
                return Err(CodexError::TurnFailed("interrupted"));
            }
            _ => return Err(CodexError::Protocol("invalid completed turn status")),
        },
    }
    Ok(ValidatedTurn {
        id: id.to_owned(),
        items: validated,
    })
}

fn validate_turn_error(error: &Value) -> Result<(), CodexError> {
    validate_exact_object_keys(
        error,
        &["additionalDetails", "codexErrorInfo", "message"],
        "invalid turn error",
    )?;
    let object = error
        .as_object()
        .ok_or(CodexError::Protocol("invalid turn error"))?;
    if object
        .get("message")
        .and_then(Value::as_str)
        .is_none_or(|message| {
            message.len() > MAX_TURN_ACCUMULATED_TEXT_BYTES || message.contains('\0')
        })
        || object.get("additionalDetails").is_some_and(|details| {
            !details.is_null()
                && details.as_str().is_none_or(|details| {
                    details.len() > MAX_TURN_ACCUMULATED_TEXT_BYTES || details.contains('\0')
                })
        })
    {
        return Err(CodexError::Protocol("invalid turn error"));
    }
    if let Some(info) = object.get("codexErrorInfo").filter(|info| !info.is_null()) {
        validate_codex_error_info(info)?;
    }
    Ok(())
}

fn validate_codex_error_info(info: &Value) -> Result<(), CodexError> {
    if matches!(
        info.as_str(),
        Some(
            "contextWindowExceeded"
                | "sessionBudgetExceeded"
                | "usageLimitExceeded"
                | "serverOverloaded"
                | "cyberPolicy"
                | "internalServerError"
                | "unauthorized"
                | "badRequest"
                | "threadRollbackFailed"
                | "sandboxError"
                | "other"
        )
    ) {
        return Ok(());
    }
    let object = info
        .as_object()
        .ok_or(CodexError::Protocol("invalid Codex error info"))?;
    if object.len() != 1 {
        return Err(CodexError::Protocol("invalid Codex error info"));
    }
    let (kind, details) = object
        .iter()
        .next()
        .ok_or(CodexError::Protocol("invalid Codex error info"))?;
    let details = details
        .as_object()
        .ok_or(CodexError::Protocol("invalid Codex error info"))?;
    match kind.as_str() {
        "httpConnectionFailed"
        | "responseStreamConnectionFailed"
        | "responseStreamDisconnected"
        | "responseTooManyFailedAttempts" => {
            if details.keys().any(|key| key != "httpStatusCode")
                || details.get("httpStatusCode").is_some_and(|status| {
                    !status.is_null()
                        && status
                            .as_u64()
                            .is_none_or(|status| status > u64::from(u16::MAX))
                })
            {
                return Err(CodexError::Protocol("invalid Codex error info"));
            }
        }
        "activeTurnNotSteerable" => {
            if details.len() != 1
                || !matches!(
                    details.get("turnKind").and_then(Value::as_str),
                    Some("review" | "compact")
                )
            {
                return Err(CodexError::Protocol("invalid Codex error info"));
            }
        }
        _ => return Err(CodexError::Protocol("invalid Codex error info")),
    }
    Ok(())
}

fn inspect_item(item: &Value, turn: &mut TurnAccumulator) -> Result<(), CodexError> {
    turn.item_count = turn
        .item_count
        .checked_add(1)
        .ok_or(CodexError::Protocol("turn item count overflow"))?;
    if turn.item_count > MAX_TURN_ITEMS {
        return Err(CodexError::Protocol("turn item limit exceeded"));
    }
    let item_id = item
        .get("id")
        .and_then(Value::as_str)
        .ok_or(CodexError::Protocol("turn item id missing"))?;
    if item_id.is_empty() || item_id.len() > MAX_TURN_ITEM_ID_BYTES || item_id.contains('\0') {
        return Err(CodexError::Protocol("invalid turn item id"));
    }
    let item_type = item
        .get("type")
        .and_then(Value::as_str)
        .ok_or(CodexError::Protocol("turn item type missing"))?;
    match item_type {
        "userMessage" => {
            validate_exact_object_keys(
                item,
                &["clientId", "content", "id", "type"],
                "invalid user message item",
            )?;
            validate_user_message_item(item)?;
            Ok(())
        }
        "reasoning" => validate_reasoning_item(item),
        "agentMessage" => validate_agent_message_item(item, turn),
        "commandExecution"
        | "fileChange"
        | "mcpToolCall"
        | "dynamicToolCall"
        | "collabAgentToolCall"
        | "webSearch"
        | "imageView"
        | "imageGeneration"
        | "sleep" => Err(CodexError::Protocol("tool or action item attempted")),
        _ => Err(CodexError::Protocol("unknown turn item type")),
    }
}

fn validate_reasoning_item(item: &Value) -> Result<(), CodexError> {
    validate_exact_object_keys(
        item,
        &["content", "id", "summary", "type"],
        "invalid reasoning item",
    )?;
    for key in ["content", "summary"] {
        if item.get(key).is_some_and(|value| {
            value.as_array().is_none_or(|parts| {
                parts.len() > MAX_TURN_ITEMS
                    || parts.iter().any(|part| {
                        part.as_str().is_none_or(|text| {
                            text.len() > MAX_TURN_ACCUMULATED_TEXT_BYTES || text.contains('\0')
                        })
                    })
            })
        }) {
            return Err(CodexError::Protocol("invalid reasoning item"));
        }
    }
    Ok(())
}

fn validate_agent_message_item(item: &Value, turn: &mut TurnAccumulator) -> Result<(), CodexError> {
    validate_exact_object_keys(
        item,
        &["id", "memoryCitation", "phase", "text", "type"],
        "invalid agent message item",
    )?;
    let text = item
        .get("text")
        .and_then(Value::as_str)
        .ok_or(CodexError::Protocol("agent message text missing"))?;
    if text.contains('\0') {
        return Err(CodexError::Protocol("invalid agent message text"));
    }
    if let Some(citation) = item.get("memoryCitation").filter(|value| !value.is_null()) {
        validate_memory_citation(citation)?;
    }
    match item.get("phase") {
        Some(Value::String(phase)) if phase == "final_answer" => {
            record_agent_message_text(turn, text)?;
            turn.final_messages.push(text.to_owned());
        }
        None | Some(Value::Null) => {
            record_agent_message_text(turn, text)?;
            turn.phase_unknown.push(text.to_owned());
        }
        Some(Value::String(phase)) if phase == "commentary" => {}
        Some(_) => return Err(CodexError::Protocol("unknown agent message phase")),
    }
    Ok(())
}

fn record_agent_message_text(turn: &mut TurnAccumulator, text: &str) -> Result<(), CodexError> {
    turn.text_bytes = turn
        .text_bytes
        .checked_add(text.len())
        .ok_or(CodexError::Protocol("turn text size overflow"))?;
    if turn.text_bytes > MAX_TURN_ACCUMULATED_TEXT_BYTES {
        return Err(CodexError::Protocol("turn text limit exceeded"));
    }
    Ok(())
}

fn validate_memory_citation(citation: &Value) -> Result<(), CodexError> {
    validate_exact_object_keys(
        citation,
        &["entries", "threadIds"],
        "invalid agent message citation",
    )?;
    let citation = citation
        .as_object()
        .ok_or(CodexError::Protocol("invalid agent message citation"))?;
    let entries = citation
        .get("entries")
        .and_then(Value::as_array)
        .filter(|entries| entries.len() <= MAX_TURN_ITEMS)
        .ok_or(CodexError::Protocol("invalid agent message citation"))?;
    let thread_ids = citation
        .get("threadIds")
        .and_then(Value::as_array)
        .filter(|thread_ids| thread_ids.len() <= MAX_TURN_ITEMS)
        .ok_or(CodexError::Protocol("invalid agent message citation"))?;
    for thread_id in thread_ids {
        let thread_id = thread_id
            .as_str()
            .filter(|value| {
                !value.is_empty() && value.len() <= MAX_TURN_ITEM_ID_BYTES && !value.contains('\0')
            })
            .ok_or(CodexError::Protocol("invalid agent message citation"))?;
        let _ = thread_id;
    }
    for entry in entries {
        validate_exact_object_keys(
            entry,
            &["lineEnd", "lineStart", "note", "path"],
            "invalid agent message citation",
        )?;
        let entry = entry
            .as_object()
            .ok_or(CodexError::Protocol("invalid agent message citation"))?;
        for key in ["lineStart", "lineEnd"] {
            if entry
                .get(key)
                .and_then(Value::as_u64)
                .is_none_or(|line| line > u64::from(u32::MAX))
            {
                return Err(CodexError::Protocol("invalid agent message citation"));
            }
        }
        for key in ["note", "path"] {
            if entry.get(key).and_then(Value::as_str).is_none_or(|value| {
                value.len() > MAX_TURN_ACCUMULATED_TEXT_BYTES || value.contains('\0')
            }) {
                return Err(CodexError::Protocol("invalid agent message citation"));
            }
        }
    }
    Ok(())
}

fn validate_user_message_item(item: &Value) -> Result<(), CodexError> {
    if item.get("clientId").is_some_and(|value| {
        !value.is_null()
            && value.as_str().is_none_or(|client_id| {
                client_id.is_empty()
                    || client_id.len() > MAX_TURN_ITEM_ID_BYTES
                    || client_id.contains('\0')
            })
    }) {
        return Err(CodexError::Protocol("invalid user message client id"));
    }
    let content = item
        .get("content")
        .and_then(Value::as_array)
        .ok_or(CodexError::Protocol("user message content missing"))?;
    if content.len() > MAX_TURN_ITEMS {
        return Err(CodexError::Protocol("user message content limit exceeded"));
    }
    for input in content {
        validate_user_input(input)?;
    }
    Ok(())
}

fn validate_user_input(input: &Value) -> Result<(), CodexError> {
    let kind = input
        .get("type")
        .and_then(Value::as_str)
        .ok_or(CodexError::Protocol("user input type missing"))?;
    match kind {
        "text" => {
            validate_exact_object_keys(
                input,
                &["text", "text_elements", "type"],
                "invalid text input",
            )?;
            let text = validate_required_input_string(input, "text")?;
            validate_text_elements(input.get("text_elements"), text)
        }
        "image" => {
            validate_exact_object_keys(input, &["detail", "type", "url"], "invalid image input")?;
            validate_required_input_string(input, "url")?;
            validate_optional_image_detail(input)
        }
        "localImage" => {
            validate_exact_object_keys(
                input,
                &["detail", "path", "type"],
                "invalid local image input",
            )?;
            validate_required_input_string(input, "path")?;
            validate_optional_image_detail(input)
        }
        "skill" | "mention" => {
            validate_exact_object_keys(
                input,
                &["name", "path", "type"],
                "invalid named user input",
            )?;
            validate_required_input_string(input, "name")?;
            validate_required_input_string(input, "path")?;
            Ok(())
        }
        _ => Err(CodexError::Protocol("invalid user input")),
    }
}

fn validate_required_input_string<'a>(input: &'a Value, key: &str) -> Result<&'a str, CodexError> {
    input
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| value.len() <= MAX_TURN_ACCUMULATED_TEXT_BYTES && !value.contains('\0'))
        .ok_or(CodexError::Protocol("invalid user input"))
}

fn validate_text_elements(elements: Option<&Value>, text: &str) -> Result<(), CodexError> {
    let Some(elements) = elements else {
        return Ok(());
    };
    let elements = elements
        .as_array()
        .filter(|values| values.len() <= MAX_TURN_ITEMS)
        .ok_or(CodexError::Protocol("invalid text elements"))?;
    for element in elements {
        validate_exact_object_keys(
            element,
            &["byteRange", "placeholder"],
            "invalid text element",
        )?;
        let range = element
            .get("byteRange")
            .and_then(Value::as_object)
            .ok_or(CodexError::Protocol("invalid text element"))?;
        if range
            .keys()
            .any(|key| !matches!(key.as_str(), "start" | "end"))
            || element.get("placeholder").is_some_and(|value| {
                !value.is_null()
                    && value.as_str().is_none_or(|placeholder| {
                        placeholder.len() > MAX_TURN_ACCUMULATED_TEXT_BYTES
                            || placeholder.contains('\0')
                    })
            })
        {
            return Err(CodexError::Protocol("invalid text element"));
        }
        let start = range
            .get("start")
            .and_then(Value::as_u64)
            .and_then(|value| usize::try_from(value).ok())
            .ok_or(CodexError::Protocol("invalid text element"))?;
        let end = range
            .get("end")
            .and_then(Value::as_u64)
            .and_then(|value| usize::try_from(value).ok())
            .ok_or(CodexError::Protocol("invalid text element"))?;
        if start > end || end > text.len() {
            return Err(CodexError::Protocol("invalid text element range"));
        }
    }
    Ok(())
}

fn validate_optional_image_detail(input: &Value) -> Result<(), CodexError> {
    if input.get("detail").is_some_and(|detail| {
        !detail.is_null() && !matches!(detail.as_str(), Some("auto" | "low" | "high" | "original"))
    }) {
        return Err(CodexError::Protocol("invalid image detail"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        AccountState, CODEX_BINARY_SHA256, CODEX_CODE_MODE_HOST_SHA256, CODEX_PACKAGE_SHA256,
        CODEX_RG_SHA256, CODEX_VERSION, CodexClient, CodexError, CodexRuntimeConfig,
        MAX_PASSIVE_DELTA_BYTES, MAX_PASSIVE_TURN_NOTIFICATIONS, MAX_TURN_ACCUMULATED_TEXT_BYTES,
        MAX_TURN_ITEM_ID_BYTES, MAX_TURN_ITEMS, OutcomeRequest, RuntimePurpose, SelectedModel,
        TurnAccumulator, TurnValidationPhase, inspect_item, request_allowed,
        sanitized_turn_error_class, validate_effective_security_config,
        validate_item_lifecycle_notification, validate_passive_turn_notification, validate_thread,
        validate_turn,
    };
    use crate::wire::Transport;
    use serde_json::{Value, json};
    use sha2::{Digest, Sha256};
    use std::collections::VecDeque;
    use std::path::{Path, PathBuf};
    use std::process::{Command, Stdio};
    use std::sync::{Arc, atomic::AtomicBool};

    #[test]
    fn thread_containment_must_match_exactly() {
        let good = json!({
            "approvalPolicy": "never",
            "cwd": "/tmp/model-input",
            "instructionSources": [],
            "model": "gpt-test-model",
            "modelProvider": "openai",
            "sandbox": {"networkAccess": false, "type": "readOnly"},
            "thread": {"id": "thread-1"}
        });
        validate_thread(&good, "/tmp/model-input", "gpt-test-model").unwrap();
        let mut changed = good;
        changed["sandbox"]["networkAccess"] = json!(true);
        assert!(validate_thread(&changed, "/tmp/model-input", "gpt-test-model").is_err());
    }

    #[test]
    fn uninitialized_codex_accepts_only_the_initialize_handshake() {
        assert!(request_allowed(false, false, "initialize"));
        for method in ["account/read", "model/list", "thread/start", "turn/start"] {
            assert!(!request_allowed(false, false, method));
            assert!(!request_allowed(true, false, method));
            assert!(request_allowed(true, true, method));
        }
        assert!(request_allowed(true, false, "config/read"));
    }

    #[test]
    fn managed_auth_storage_override_fails_before_any_account_route() {
        for (key, value) in [
            ("cli_auth_credentials_store", "auto"),
            ("cli_auth_credentials_store", "file"),
            ("mcp_oauth_credentials_store", "file"),
        ] {
            let mut config = json!({
                "forced_login_method": "chatgpt",
                "cli_auth_credentials_store": "keyring",
                "mcp_oauth_credentials_store": "keyring",
                "features": {"secret_auth_storage": false}
            });
            config[key] = json!(value);
            assert!(validate_effective_security_config(&json!({"config": config})).is_err());
        }
        assert!(
            validate_effective_security_config(&json!({
                "config": {
                    "forced_login_method": "chatgpt",
                    "cli_auth_credentials_store": "keyring",
                    "mcp_oauth_credentials_store": "keyring",
                    "features": {"secret_auth_storage": false}
                }
            }))
            .is_ok()
        );
        assert!(
            validate_effective_security_config(&json!({
                "config": {
                    "forced_login_method": "chatgpt",
                    "cli_auth_credentials_store": "keyring",
                    "mcp_oauth_credentials_store": "keyring",
                    "features": {"secret_auth_storage": true}
                }
            }))
            .is_err()
        );
    }

    #[test]
    fn every_effectful_or_unknown_item_fails_closed() {
        for item_type in [
            "commandExecution",
            "fileChange",
            "mcpToolCall",
            "dynamicToolCall",
            "collabAgentToolCall",
            "webSearch",
            "imageView",
            "imageGeneration",
            "sleep",
            "futureTool",
        ] {
            assert!(matches!(
                inspect_item(
                    &json!({"id": "item-1", "type": item_type}),
                    &mut TurnAccumulator::default(),
                ),
                Err(CodexError::Protocol(_))
            ));
        }
    }

    #[test]
    fn pinned_passive_turn_progress_is_bounded_and_identity_checked() {
        let thread_id = "thread-1";
        let turn_id = "turn-1";
        let mut turn = TurnAccumulator::default();
        for (method, params) in [
            (
                "turn/started",
                json!({
                    "threadId": thread_id,
                    "turn": {"id": turn_id, "items": [], "status": "inProgress"}
                }),
            ),
            (
                "thread/status/changed",
                json!({
                    "status": {"activeFlags": [], "type": "active"},
                    "threadId": thread_id
                }),
            ),
            (
                "item/reasoning/summaryTextDelta",
                json!({
                    "delta": "bounded summary",
                    "itemId": "reasoning-1",
                    "summaryIndex": 0,
                    "threadId": thread_id,
                    "turnId": turn_id
                }),
            ),
            (
                "item/reasoning/summaryPartAdded",
                json!({
                    "itemId": "reasoning-1",
                    "summaryIndex": 1,
                    "threadId": thread_id,
                    "turnId": turn_id
                }),
            ),
            (
                "item/reasoning/textDelta",
                json!({
                    "contentIndex": 0,
                    "delta": "bounded reasoning",
                    "itemId": "reasoning-1",
                    "threadId": thread_id,
                    "turnId": turn_id
                }),
            ),
            (
                "item/agentMessage/delta",
                json!({
                    "delta": "bounded output",
                    "itemId": "message-1",
                    "threadId": thread_id,
                    "turnId": turn_id
                }),
            ),
        ] {
            validate_passive_turn_notification(method, &params, thread_id, turn_id, &mut turn)
                .unwrap();
        }
    }

    #[test]
    fn pinned_passive_turn_metadata_is_bounded_and_schema_shaped() {
        let thread_id = "thread-1";
        let turn_id = "turn-1";
        let mut turn = TurnAccumulator::default();
        for (method, params) in [
            (
                "thread/tokenUsage/updated",
                json!({
                    "threadId": thread_id,
                    "tokenUsage": {
                        "last": {
                            "cachedInputTokens": 0,
                            "inputTokens": 10,
                            "outputTokens": 5,
                            "reasoningOutputTokens": 3,
                            "totalTokens": 15
                        },
                        "modelContextWindow": 100_000,
                        "total": {
                            "cachedInputTokens": 0,
                            "inputTokens": 10,
                            "outputTokens": 5,
                            "reasoningOutputTokens": 3,
                            "totalTokens": 15
                        }
                    },
                    "turnId": turn_id
                }),
            ),
            (
                "account/rateLimits/updated",
                json!({"rateLimits": {"planType": "pro"}}),
            ),
            (
                "model/verification",
                json!({
                    "threadId": thread_id,
                    "turnId": turn_id,
                    "verifications": ["trustedAccessForCyber"]
                }),
            ),
            (
                "model/safetyBuffering/updated",
                json!({
                    "fasterModel": null,
                    "model": "gpt-test-model",
                    "reasons": [],
                    "showBufferingUi": false,
                    "threadId": thread_id,
                    "turnId": turn_id,
                    "useCases": []
                }),
            ),
        ] {
            validate_passive_turn_notification(method, &params, thread_id, turn_id, &mut turn)
                .unwrap();
        }
    }

    #[test]
    fn malformed_pinned_passive_metadata_fails_closed() {
        let thread_id = "thread-1";
        let turn_id = "turn-1";
        for (method, params) in [
            (
                "thread/status/changed",
                json!({"status": {"type": "active"}, "threadId": thread_id}),
            ),
            (
                "thread/status/changed",
                json!({
                    "status": {"activeFlags": "none", "type": "active"},
                    "threadId": thread_id
                }),
            ),
            (
                "thread/status/changed",
                json!({
                    "status": {"activeFlags": ["futureAuthority"], "type": "active"},
                    "threadId": thread_id
                }),
            ),
            (
                "thread/status/changed",
                json!({
                    "status": {"activeFlags": [], "type": "idle"},
                    "threadId": thread_id
                }),
            ),
            (
                "account/rateLimits/updated",
                json!({"rateLimits": "not-an-object"}),
            ),
            (
                "account/rateLimits/updated",
                json!({"rateLimits": {"planType": 7}}),
            ),
            (
                "account/rateLimits/updated",
                json!({"rateLimits": {"primary": {"resetsAt": 1}}}),
            ),
            (
                "account/rateLimits/updated",
                json!({
                    "rateLimits": {"credits": {"hasCredits": "yes", "unlimited": false}}
                }),
            ),
            ("turn/started", json!({"threadId": thread_id})),
            (
                "turn/started",
                json!({
                    "threadId": thread_id,
                    "turn": {"id": turn_id, "items": [], "status": "completed"}
                }),
            ),
            (
                "turn/started",
                json!({
                    "threadId": thread_id,
                    "turn": {"id": 1, "items": [], "status": "inProgress"}
                }),
            ),
            (
                "turn/started",
                json!({
                    "threadId": thread_id,
                    "turn": {"id": turn_id, "status": "inProgress"}
                }),
            ),
            (
                "turn/started",
                json!({
                    "threadId": thread_id,
                    "turn": {"id": turn_id, "items": "not-an-array", "status": "inProgress"}
                }),
            ),
            (
                "turn/started",
                json!({
                    "threadId": thread_id,
                    "turn": {
                        "id": turn_id,
                        "items": [{"id": "tool-1", "type": "commandExecution"}],
                        "status": "inProgress"
                    }
                }),
            ),
        ] {
            assert!(
                validate_passive_turn_notification(
                    method,
                    &params,
                    thread_id,
                    turn_id,
                    &mut TurnAccumulator::default(),
                )
                .is_err(),
                "malformed {method} unexpectedly passed"
            );
        }
    }

    #[test]
    fn passive_progress_rejects_wrong_identity_unbounded_data_and_unknown_methods() {
        let scoped = |delta: String| {
            json!({
                "delta": delta,
                "itemId": "message-1",
                "threadId": "thread-1",
                "turnId": "turn-1"
            })
        };
        for params in [
            json!({
                "delta": "x",
                "itemId": "message-1",
                "threadId": "wrong",
                "turnId": "turn-1"
            }),
            scoped("x".repeat(MAX_PASSIVE_DELTA_BYTES + 1)),
            json!({
                "delta": "x",
                "itemId": "\0",
                "threadId": "thread-1",
                "turnId": "turn-1"
            }),
        ] {
            assert!(
                validate_passive_turn_notification(
                    "item/agentMessage/delta",
                    &params,
                    "thread-1",
                    "turn-1",
                    &mut TurnAccumulator::default(),
                )
                .is_err()
            );
        }
        assert!(
            validate_passive_turn_notification(
                "agentMessage/delta",
                &scoped("x".into()),
                "thread-1",
                "turn-1",
                &mut TurnAccumulator::default(),
            )
            .is_err()
        );
        let mut turn = TurnAccumulator::default();
        for _ in 0..MAX_PASSIVE_TURN_NOTIFICATIONS {
            validate_passive_turn_notification(
                "item/agentMessage/delta",
                &scoped(String::new()),
                "thread-1",
                "turn-1",
                &mut turn,
            )
            .unwrap();
        }
        assert!(
            validate_passive_turn_notification(
                "item/agentMessage/delta",
                &scoped(String::new()),
                "thread-1",
                "turn-1",
                &mut turn,
            )
            .is_err()
        );
    }

    #[test]
    fn turn_failures_are_classified_without_message_or_detail_disclosure() {
        assert_eq!(
            sanitized_turn_error_class(Some(&json!("usageLimitExceeded"))),
            "usage_limit_exceeded"
        );
        assert_eq!(
            sanitized_turn_error_class(Some(&json!({
                "responseStreamDisconnected": {"httpStatusCode": 503}
            }))),
            "response_stream_disconnected"
        );
        assert_eq!(
            sanitized_turn_error_class(Some(&json!({
                "futureFailure": {"secret": "must-not-escape"}
            }))),
            "other"
        );
    }

    #[test]
    fn turn_item_and_accumulated_text_limits_fail_closed() {
        let mut turn = TurnAccumulator::default();
        for _ in 0..MAX_TURN_ITEMS {
            inspect_item(
                &json!({"content": [], "id": "user-1", "type": "userMessage"}),
                &mut turn,
            )
            .unwrap();
        }
        assert!(matches!(
            inspect_item(
                &json!({"content": [], "id": "user-1", "type": "userMessage"}),
                &mut turn,
            ),
            Err(CodexError::Protocol("turn item limit exceeded"))
        ));

        let mut turn = TurnAccumulator::default();
        let oversized = "x".repeat(MAX_TURN_ACCUMULATED_TEXT_BYTES + 1);
        assert!(matches!(
            inspect_item(
                &json!({
                    "id": "message-1",
                    "type": "agentMessage",
                    "phase": "final_answer",
                    "text": oversized
                }),
                &mut turn,
            ),
            Err(CodexError::Protocol("turn text limit exceeded"))
        ));
        assert!(turn.final_messages.is_empty());
    }

    #[test]
    fn pinned_turn_items_require_bounded_ids_and_type_specific_fields() {
        for malformed in [
            json!({"text": "{}", "type": "agentMessage"}),
            json!({"id": "", "text": "{}", "type": "agentMessage"}),
            json!({"id": "bad\0id", "text": "{}", "type": "agentMessage"}),
            json!({
                "id": "x".repeat(MAX_TURN_ITEM_ID_BYTES + 1),
                "text": "{}",
                "type": "agentMessage"
            }),
            json!({"id": "message-1", "type": "agentMessage"}),
            json!({"id": "user-1", "type": "userMessage"}),
            json!({"content": "text", "id": "user-1", "type": "userMessage"}),
            json!({
                "content": [{"type": "text"}],
                "id": "user-1",
                "type": "userMessage"
            }),
            json!({
                "content": [{"detail": "future", "type": "image", "url": "https://invalid.test"}],
                "id": "user-1",
                "type": "userMessage"
            }),
            json!({"content": "text", "id": "reasoning-1", "type": "reasoning"}),
            json!({"id": "reasoning-1", "summary": [1], "type": "reasoning"}),
            json!({"id": "message-1", "phase": 1, "text": "{}", "type": "agentMessage"}),
            json!({
                "id": "message-1",
                "memoryCitation": "invalid",
                "text": "{}",
                "type": "agentMessage"
            }),
        ] {
            assert!(
                inspect_item(&malformed, &mut TurnAccumulator::default()).is_err(),
                "malformed turn item unexpectedly passed"
            );
        }

        for valid in [
            json!({"content": [], "id": "user-1", "type": "userMessage"}),
            json!({
                "content": [{"text": "bounded", "type": "text"}],
                "id": "user-1",
                "type": "userMessage"
            }),
            json!({
                "content": ["bounded reasoning"],
                "id": "reasoning-1",
                "summary": ["bounded summary"],
                "type": "reasoning"
            }),
            json!({
                "id": "message-1",
                "phase": "final_answer",
                "text": "{}",
                "type": "agentMessage"
            }),
        ] {
            inspect_item(&valid, &mut TurnAccumulator::default()).unwrap();
        }
    }

    #[test]
    fn pinned_memory_citation_shape_is_strictly_validated() {
        let item = |citation| {
            json!({
                "id": "message-1",
                "memoryCitation": citation,
                "phase": "final_answer",
                "text": "{}",
                "type": "agentMessage"
            })
        };
        for malformed in [
            json!({}),
            json!({"entries": "invalid", "threadIds": []}),
            json!({"entries": [], "threadIds": "invalid"}),
            json!({"entries": [{}], "threadIds": []}),
            json!({
                "entries": [{"lineEnd": -1, "lineStart": 0, "note": "note", "path": "path"}],
                "threadIds": []
            }),
            json!({
                "entries": [{
                    "lineEnd": u64::from(u32::MAX) + 1,
                    "lineStart": 0,
                    "note": "note",
                    "path": "path"
                }],
                "threadIds": []
            }),
            json!({
                "entries": [{"lineEnd": 2, "lineStart": 1, "note": 7, "path": "path"}],
                "threadIds": []
            }),
            json!({
                "entries": [{"lineEnd": 2, "lineStart": 1, "note": "note", "path": "path"}],
                "threadIds": [""]
            }),
        ] {
            assert!(
                inspect_item(&item(malformed), &mut TurnAccumulator::default()).is_err(),
                "malformed memory citation unexpectedly passed"
            );
        }
        inspect_item(
            &item(json!({
                "entries": [{
                    "lineEnd": 2,
                    "lineStart": 1,
                    "note": "bounded note",
                    "path": "bounded/path"
                }],
                "threadIds": ["thread-1"]
            })),
            &mut TurnAccumulator::default(),
        )
        .unwrap();
    }

    #[test]
    fn completed_turn_requires_full_items_and_consistent_optional_metadata() {
        let turn = |items_view: Option<serde_json::Value>| {
            let mut turn = json!({"id": "turn-1", "items": [], "status": "completed"});
            if let Some(items_view) = items_view {
                turn["itemsView"] = items_view;
            }
            turn
        };
        validate_turn(
            &turn(None),
            Some("turn-1"),
            TurnValidationPhase::CompletedNotification,
        )
        .unwrap();
        validate_turn(
            &turn(Some(json!("full"))),
            Some("turn-1"),
            TurnValidationPhase::CompletedNotification,
        )
        .unwrap();
        for malformed in [
            turn(Some(json!("summary"))),
            turn(Some(json!("notLoaded"))),
            turn(Some(json!(7))),
            json!({
                "id": "turn-1",
                "items": [],
                "status": "completed",
                "completedAt": "now"
            }),
            json!({
                "id": "turn-1",
                "items": [],
                "status": "completed",
                "durationMs": false
            }),
            json!({
                "id": "turn-1",
                "items": [],
                "status": "completed",
                "startedAt": 1.5
            }),
            json!({
                "error": {"message": "must not accompany completed"},
                "id": "turn-1",
                "items": [],
                "status": "completed"
            }),
        ] {
            assert!(
                validate_turn(
                    &malformed,
                    Some("turn-1"),
                    TurnValidationPhase::CompletedNotification,
                )
                .is_err(),
                "malformed completed turn unexpectedly passed"
            );
        }
    }

    #[test]
    fn one_stage_aware_gateway_validates_all_three_turn_routes() {
        for phase in [
            TurnValidationPhase::StartResponse,
            TurnValidationPhase::StartedNotification,
            TurnValidationPhase::CompletedNotification,
        ] {
            let status = if phase == TurnValidationPhase::CompletedNotification {
                "completed"
            } else {
                "inProgress"
            };
            let turn = json!({
                "completedAt": if status == "completed" { json!(3) } else { Value::Null },
                "durationMs": if status == "completed" { json!(2) } else { Value::Null },
                "error": null,
                "id": "turn-1",
                "items": [{
                    "content": [{"text": "bounded", "text_elements": [], "type": "text"}],
                    "id": "user-1",
                    "type": "userMessage"
                }],
                "itemsView": "full",
                "startedAt": 1,
                "status": status
            });
            validate_turn(&turn, Some("turn-1"), phase).unwrap();
        }
    }

    #[test]
    fn shared_malformed_turn_corpus_fails_on_every_route() {
        let cases = [
            json!({"futureAuthority": true}),
            json!({"itemsView": "summary"}),
            json!({"itemsView": "notLoaded"}),
            json!({"startedAt": 1.5}),
            json!({"startedAt": 9_223_372_036_854_775_808_u64}),
            json!({"durationMs": "two"}),
            json!({"error": {"message": "contradiction"}}),
            json!({"items": [{"futureAuthority": true, "id": "message-1", "text": "{}", "type": "agentMessage"}]}),
            json!({"items": [{"id": "tool-1", "type": "commandExecution"}]}),
            json!({"items": [{
                "id": "message-1",
                "memoryCitation": {"entries": [], "futureAuthority": true, "threadIds": []},
                "text": "{}",
                "type": "agentMessage"
            }]}),
        ];
        for phase in [
            TurnValidationPhase::StartResponse,
            TurnValidationPhase::StartedNotification,
            TurnValidationPhase::CompletedNotification,
        ] {
            let status = if phase == TurnValidationPhase::CompletedNotification {
                "completed"
            } else {
                "inProgress"
            };
            for changes in &cases {
                let mut turn = json!({
                    "id": "turn-1",
                    "items": [],
                    "status": status
                });
                for (key, value) in changes.as_object().unwrap() {
                    turn[key] = value.clone();
                }
                assert!(
                    validate_turn(&turn, Some("turn-1"), phase).is_err(),
                    "malformed Turn unexpectedly passed {phase:?}: {turn}"
                );
            }
            for malformed in [
                json!({"id": "turn-1", "status": status}),
                json!({"id": "", "items": [], "status": status}),
                json!({"id": "wrong-turn", "items": [], "status": status}),
            ] {
                assert!(
                    validate_turn(&malformed, Some("turn-1"), phase).is_err(),
                    "required-field/identity corpus unexpectedly passed {phase:?}: {malformed}"
                );
            }
        }
    }

    #[test]
    fn terminal_failure_shape_is_validated_before_sanitized_classification() {
        let failed = json!({
            "error": {
                "additionalDetails": null,
                "codexErrorInfo": {"responseStreamDisconnected": {"httpStatusCode": 503}},
                "message": "sensitive remote detail"
            },
            "id": "turn-1",
            "items": [],
            "status": "failed"
        });
        assert!(matches!(
            validate_turn(
                &failed,
                Some("turn-1"),
                TurnValidationPhase::CompletedNotification
            ),
            Err(CodexError::TurnFailed("response_stream_disconnected"))
        ));
        let mut malformed = failed;
        malformed["error"]["codexErrorInfo"] =
            json!({"responseStreamDisconnected": {"httpStatusCode": 70_000}});
        assert!(matches!(
            validate_turn(
                &malformed,
                Some("turn-1"),
                TurnValidationPhase::CompletedNotification
            ),
            Err(CodexError::Protocol(_))
        ));
        let mut concealed_action = json!({
            "error": {"message": "failed"},
            "id": "turn-1",
            "items": [{"id": "tool-1", "type": "commandExecution"}],
            "status": "failed"
        });
        assert!(matches!(
            validate_turn(
                &concealed_action,
                Some("turn-1"),
                TurnValidationPhase::CompletedNotification
            ),
            Err(CodexError::Protocol("tool or action item attempted"))
        ));
        concealed_action["items"] = json!([]);
        assert!(matches!(
            validate_turn(
                &concealed_action,
                Some("turn-1"),
                TurnValidationPhase::CompletedNotification
            ),
            Err(CodexError::TurnFailed("other"))
        ));
    }

    #[test]
    fn malformed_turn_start_response_terminates_transport_before_reuse() {
        let root = tempfile::tempdir().unwrap();
        let codex_home = root.path().join("codex-home");
        let model_workspace = root.path().join("model-input");
        let model_workspace_string = model_workspace.to_str().unwrap().to_owned();
        std::fs::create_dir(&codex_home).unwrap();
        std::fs::create_dir(&model_workspace).unwrap();
        let child = Command::new("/bin/sh")
            .arg("-c")
            .arg(
                "read _; printf '%s\\n' '{\"id\":1,\"result\":{\"turn\":{\"id\":\"turn-1\",\"items\":[],\"itemsView\":\"summary\",\"status\":\"inProgress\"}}}'; exec /bin/sleep 60",
            )
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let process_identifier = i32::try_from(child.id()).unwrap();
        let transport = Transport::new(child).unwrap();
        let mut client = CodexClient {
            transport,
            next_id: 1,
            codex_home,
            model_workspace,
            cancel: Arc::new(AtomicBool::new(false)),
            early_turn_notifications: VecDeque::new(),
            early_turn_notification_bytes: 0,
            process_identifier,
            protocol_initialized: true,
            initialized: true,
            purpose: RuntimePurpose::Model,
        };
        let developer_instructions = format!(
            "OpenOpen persona openopen.nondev.default / draft-03-en; aggregate={}. Return only the requested JSON.",
            "b".repeat(64)
        );
        let request = OutcomeRequest {
            prompt: "synthetic bounded prompt".to_owned(),
            allowed_source_refs: vec![],
            selected_model: Some(SelectedModel {
                model_id: "gpt-example".to_owned(),
                reasoning_effort: None,
                catalog_fingerprint: "a".repeat(64),
                catalog_revision: 1,
            }),
            persona_revision: openopen_protocol::PersonaRevisionRef {
                persona_id: "openopen.nondev.default".to_owned(),
                revision: "draft-03-en".to_owned(),
                aggregate_digest: "b".repeat(64),
                instructions_digest: format!(
                    "{:x}",
                    Sha256::digest(developer_instructions.as_bytes())
                ),
            },
            developer_instructions,
        };
        let result: Result<super::StructuredOutcome, CodexError> =
            client.start_turn_and_collect("thread-1", &model_workspace_string, &request);
        assert!(matches!(
            result,
            Err(CodexError::Protocol("incomplete turn items"))
        ));
        assert!(matches!(
            client.transport.send_notification("initialized", None),
            Err(CodexError::Process("app-server stdin closed"))
        ));
    }

    #[test]
    fn malformed_turn_event_terminates_transport_before_client_reuse() {
        let root = tempfile::tempdir().unwrap();
        let codex_home = root.path().join("codex-home");
        let model_workspace = root.path().join("model-input");
        std::fs::create_dir(&codex_home).unwrap();
        std::fs::create_dir(&model_workspace).unwrap();
        let child = Command::new("/bin/sh")
            .arg("-c")
            .arg(
                "printf '%s\\n' '{\"method\":\"item/completed\",\"params\":{\"completedAtMs\":1,\"item\":{\"text\":\"{}\",\"type\":\"agentMessage\"},\"threadId\":\"thread-1\",\"turnId\":\"turn-1\"}}'; exec /bin/sleep 60",
            )
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let process_identifier = i32::try_from(child.id()).unwrap();
        let transport = Transport::new(child).unwrap();
        let mut client = CodexClient {
            transport,
            next_id: 1,
            codex_home,
            model_workspace,
            cancel: Arc::new(AtomicBool::new(false)),
            early_turn_notifications: VecDeque::new(),
            early_turn_notification_bytes: 0,
            process_identifier,
            protocol_initialized: true,
            initialized: true,
            purpose: RuntimePurpose::Model,
        };
        assert!(matches!(
            client.collect_outcome("thread-1", "turn-1", &[]),
            Err(CodexError::Protocol("turn item id missing"))
        ));
        assert!(matches!(
            client.transport.send_notification("initialized", None),
            Err(CodexError::Process("app-server stdin closed"))
        ));
    }

    #[test]
    fn summary_completion_terminates_transport_before_client_reuse() {
        let root = tempfile::tempdir().unwrap();
        let codex_home = root.path().join("codex-home");
        let model_workspace = root.path().join("model-input");
        std::fs::create_dir(&codex_home).unwrap();
        std::fs::create_dir(&model_workspace).unwrap();
        let child = Command::new("/bin/sh")
            .arg("-c")
            .arg(
                "printf '%s\\n' '{\"method\":\"turn/completed\",\"params\":{\"threadId\":\"thread-1\",\"turn\":{\"id\":\"turn-1\",\"items\":[{\"id\":\"message-1\",\"phase\":\"final_answer\",\"text\":\"{}\",\"type\":\"agentMessage\"}],\"itemsView\":\"summary\",\"status\":\"completed\"}}}'; exec /bin/sleep 60",
            )
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let process_identifier = i32::try_from(child.id()).unwrap();
        let transport = Transport::new(child).unwrap();
        let mut client = CodexClient {
            transport,
            next_id: 1,
            codex_home,
            model_workspace,
            cancel: Arc::new(AtomicBool::new(false)),
            early_turn_notifications: VecDeque::new(),
            early_turn_notification_bytes: 0,
            process_identifier,
            protocol_initialized: true,
            initialized: true,
            purpose: RuntimePurpose::Model,
        };
        assert!(matches!(
            client.collect_outcome("thread-1", "turn-1", &[]),
            Err(CodexError::Protocol("incomplete turn items"))
        ));
        assert!(matches!(
            client.transport.send_notification("initialized", None),
            Err(CodexError::Process("app-server stdin closed"))
        ));
    }

    #[test]
    fn pinned_item_lifecycle_requires_exact_identity_timestamp_and_item_shape() {
        for (method, params) in [
            (
                "item/started",
                json!({
                    "item": {"content": [], "id": "user-1", "type": "userMessage"},
                    "threadId": "thread-1",
                    "turnId": "turn-1"
                }),
            ),
            (
                "item/completed",
                json!({
                    "completedAtMs": "now",
                    "item": {"id": "message-1", "text": "{}", "type": "agentMessage"},
                    "threadId": "thread-1",
                    "turnId": "turn-1"
                }),
            ),
            (
                "item/completed",
                json!({
                    "completedAtMs": 1,
                    "item": {"text": "{}", "type": "agentMessage"},
                    "threadId": "thread-1",
                    "turnId": "turn-1"
                }),
            ),
        ] {
            assert!(
                validate_item_lifecycle_notification(
                    method,
                    &params,
                    "thread-1",
                    "turn-1",
                    &mut TurnAccumulator::default(),
                )
                .is_err(),
                "malformed {method} unexpectedly passed"
            );
        }

        for (method, params) in [
            (
                "item/started",
                json!({
                    "item": {"content": [], "id": "user-1", "type": "userMessage"},
                    "startedAtMs": 1,
                    "threadId": "thread-1",
                    "turnId": "turn-1"
                }),
            ),
            (
                "item/completed",
                json!({
                    "completedAtMs": 2,
                    "item": {"id": "message-1", "text": "{}", "type": "agentMessage"},
                    "threadId": "thread-1",
                    "turnId": "turn-1"
                }),
            ),
        ] {
            validate_item_lifecycle_notification(
                method,
                &params,
                "thread-1",
                "turn-1",
                &mut TurnAccumulator::default(),
            )
            .unwrap();
        }
    }

    #[test]
    fn pre_cancelled_client_never_touches_the_runtime_path() {
        let root = tempfile::tempdir().unwrap();
        let config = CodexRuntimeConfig {
            runtime: root.path().join("missing-runtime"),
            codex_home: root.path().join("codex-home"),
            synthetic_home: root.path().join("synthetic-home"),
            model_workspace: root.path().join("model-input"),
            user_home: root.path().to_path_buf(),
        };
        let cancel = Arc::new(AtomicBool::new(true));
        assert!(matches!(
            CodexClient::spawn_with_cancel(&config, cancel),
            Err(CodexError::Cancelled)
        ));
    }

    #[test]
    fn tracked_manifest_matches_every_runtime_pin() {
        let manifest: serde_json::Value = serde_json::from_str(include_str!(
            "../../../protocol/codex/0.144.0/manifest.json"
        ))
        .unwrap();
        assert_eq!(manifest["codexVersion"], CODEX_VERSION);
        assert_eq!(manifest["binarySha256"], CODEX_BINARY_SHA256);
        assert_eq!(manifest["standalonePackageSha256"], CODEX_PACKAGE_SHA256);
        assert_eq!(manifest["codeModeHostSha256"], CODEX_CODE_MODE_HOST_SHA256);
        assert_eq!(manifest["standaloneRgSha256"], CODEX_RG_SHA256);
        assert_eq!(manifest["experimentalApi"], false);
        assert_eq!(manifest["schemaDigestAlgorithm"], "openopen-schema-set-v1");
        let schema_root =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../protocol/codex/0.144.0/schema");
        let mut files = Vec::new();
        collect_schema_files(&schema_root, &mut files);
        files.sort();
        assert_eq!(
            files.len(),
            usize::try_from(manifest["schemaFileCount"].as_u64().unwrap()).unwrap()
        );
        let mut digest = Sha256::new();
        digest.update(b"openopen-codex-schema-set-v1\0");
        for path in files {
            let relative = path.strip_prefix(&schema_root).unwrap();
            let relative = relative.to_str().unwrap();
            let contents = std::fs::read(&path).unwrap();
            digest.update(relative.as_bytes());
            digest.update([0]);
            digest.update(u64::try_from(contents.len()).unwrap().to_be_bytes());
            digest.update(contents);
        }
        assert_eq!(manifest["schemaSetSha256"], hex::encode(digest.finalize()));
    }

    fn collect_schema_files(root: &Path, files: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(root).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                collect_schema_files(&path, files);
            } else if path.extension().and_then(|value| value.to_str()) == Some("json") {
                files.push(path);
            }
        }
    }

    #[test]
    #[ignore = "requires OPENOPEN_TEST_CODEX_RUNTIME pointing to the exact pinned macOS binary"]
    fn real_pinned_runtime_initializes_inside_outer_sandbox() {
        let runtime = std::env::var_os("OPENOPEN_TEST_CODEX_RUNTIME")
            .expect("OPENOPEN_TEST_CODEX_RUNTIME is required for this explicit diagnostic");
        let root = tempfile::tempdir().unwrap();
        let canonical_root = std::fs::canonicalize(root.path()).unwrap();
        let codex_home = std::env::var_os("OPENOPEN_TEST_CODEX_HOME")
            .expect("OPENOPEN_TEST_CODEX_HOME is required")
            .into();
        let credential_file = PathBuf::from(&codex_home).join("auth.json");
        let synthetic_home = canonical_root.join("synthetic-home");
        let model_workspace = canonical_root.join("model-input");
        std::fs::create_dir(&model_workspace).unwrap();
        let mut client = CodexClient::spawn(&CodexRuntimeConfig {
            runtime: runtime.into(),
            codex_home,
            synthetic_home,
            model_workspace,
            user_home: std::env::var_os("OPENOPEN_TEST_USER_HOME")
                .expect("OPENOPEN_TEST_USER_HOME is required")
                .into(),
        })
        .unwrap();
        assert!(matches!(
            client.read_account().unwrap(),
            AccountState::ChatGpt { .. }
        ));
        assert!(!credential_file.exists());
    }

    #[test]
    #[ignore = "requires OPENOPEN_TEST_CODEX_RUNTIME pointing to the exact pinned macOS binary"]
    fn real_pinned_runtime_begins_managed_login_inside_outer_sandbox() {
        let runtime = std::env::var_os("OPENOPEN_TEST_CODEX_RUNTIME")
            .expect("OPENOPEN_TEST_CODEX_RUNTIME is required for this explicit diagnostic");
        let root = tempfile::tempdir().unwrap();
        let canonical_root = std::fs::canonicalize(root.path()).unwrap();
        let codex_home: PathBuf = std::env::var_os("OPENOPEN_TEST_CODEX_HOME")
            .expect("OPENOPEN_TEST_CODEX_HOME is required")
            .into();
        let synthetic_home = codex_home.join("SyntheticHome");
        let model_workspace = canonical_root.join("model-input");
        std::fs::create_dir(&model_workspace).unwrap();
        let mut client = CodexClient::spawn_login_uninitialized_with_cancel(
            &CodexRuntimeConfig {
                runtime: runtime.into(),
                codex_home: codex_home.clone(),
                synthetic_home,
                model_workspace,
                user_home: std::env::var_os("OPENOPEN_TEST_USER_HOME")
                    .expect("OPENOPEN_TEST_USER_HOME is required")
                    .into(),
            },
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
        client.complete_initialize().unwrap();
        assert!(matches!(
            client.read_account(),
            Err(CodexError::WrongRuntimePurpose)
        ));
        assert!(matches!(
            client.list_gpt_models(),
            Err(CodexError::WrongRuntimePurpose)
        ));
        let login = client.begin_chatgpt_login().unwrap();
        assert!(login.auth_url.starts_with("https://auth.openai.com/"));
        assert!(!login.login_id.is_empty());
        assert!(!codex_home.join("auth.json").exists());
    }
}
