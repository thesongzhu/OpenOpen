//! Fail-closed client for the pinned Codex app-server stable method subset.

mod contracts;
mod process;
mod wire;

pub use contracts::{
    AccountState, ChatGptLogin, GptModel, OutcomeRequest, REQUIRED_MODEL,
    REQUIRED_REASONING_EFFORT, StructuredOutcome,
};
pub use process::{
    CODEX_BINARY_SHA256, CODEX_CODE_MODE_HOST_SHA256, CODEX_PACKAGE_SHA256, CODEX_RG_SHA256,
    CODEX_VERSION, CodexRuntimeConfig,
};

use contracts::{
    MAX_MODEL_CATALOG_BYTES, MAX_MODEL_CURSOR_BYTES, MAX_MODEL_PAGES, MAX_MODELS, model_from_value,
};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};
use thiserror::Error;
use wire::{Incoming, Transport};

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
    #[error("required GPT model or reasoning effort is unavailable")]
    RequiredModelUnavailable,
    #[error("Codex requested client-side authority: {0}")]
    AuthorityRequest(String),
    #[error("structured contract failed: {0}")]
    InvalidContract(&'static str),
    #[error("Codex remote error {code}: {message}")]
    Remote { code: i64, message: String },
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
        if cancel.load(Ordering::Acquire) {
            return Err(CodexError::Cancelled);
        }
        let codex_home = config.codex_home.clone();
        let model_workspace = config.model_workspace.clone();
        let (transport, process_identifier) = process::spawn(config)?;
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

    /// Waits only for the matching managed-login completion, then re-reads the
    /// account through the stable API.
    ///
    /// # Errors
    ///
    /// Returns an error for mismatched login IDs, unsuccessful login, server
    /// authority requests, timeout, or a non-ChatGPT account.
    pub fn await_chatgpt_login(&mut self, login_id: &str) -> Result<AccountState, CodexError> {
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
                    return self.read_account();
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

    /// Runs the sealed outcome contract with the required model and high
    /// reasoning. Any tool/action item, reroute, scope mismatch, or malformed
    /// output terminates the child and returns no result.
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
        self.run_structured_outcome_in_workspace(request, &workspace)
    }

    /// Runs one structured outcome in an exact subdirectory already contained
    /// by the immutable outer sandbox root.
    ///
    /// # Errors
    ///
    /// Returns an error when the workspace escapes the immutable sandbox root,
    /// required model access is unavailable, the runtime violates containment,
    /// or the structured result fails the sealed outcome contract.
    pub fn run_structured_outcome_in_workspace(
        &mut self,
        request: &OutcomeRequest,
        workspace: &Path,
    ) -> Result<StructuredOutcome, CodexError> {
        request.validate()?;
        let workspace = std::fs::canonicalize(workspace).map_err(CodexError::Io)?;
        if !workspace.starts_with(&self.model_workspace) {
            return Err(CodexError::InvalidPath);
        }
        let models = self.list_gpt_models()?;
        let required = models
            .iter()
            .find(|model| model.id == REQUIRED_MODEL)
            .ok_or(CodexError::RequiredModelUnavailable)?;
        if !required
            .supported_reasoning_efforts
            .iter()
            .any(|effort| effort == REQUIRED_REASONING_EFFORT)
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
                "developerInstructions": "Return only the requested JSON. Do not invoke tools.",
                "ephemeral": true,
                "model": REQUIRED_MODEL,
                "sandbox": "read-only"
            }),
            REQUEST_TIMEOUT,
        )?;
        validate_thread(&thread, &cwd)?;
        let thread_id = thread
            .pointer("/thread/id")
            .and_then(Value::as_str)
            .ok_or(CodexError::Protocol("thread id missing"))?
            .to_owned();

        let turn = self.request(
            "turn/start",
            &json!({
                "approvalPolicy": "never",
                "cwd": cwd,
                "effort": REQUIRED_REASONING_EFFORT,
                "input": [{"text": request.prompt, "type": "text"}],
                "model": REQUIRED_MODEL,
                "outputSchema": request.output_schema(),
                "sandboxPolicy": {"networkAccess": false, "type": "readOnly"},
                "threadId": thread_id
            }),
            REQUEST_TIMEOUT,
        )?;
        let turn_id = turn
            .pointer("/turn/id")
            .and_then(Value::as_str)
            .ok_or(CodexError::Protocol("turn id missing"))?
            .to_owned();
        self.collect_outcome(&thread_id, &turn_id, &request.allowed_source_refs)
    }

    fn collect_outcome(
        &mut self,
        thread_id: &str,
        turn_id: &str,
        allowed_source_refs: &[String],
    ) -> Result<StructuredOutcome, CodexError> {
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
                        validate_turn_identity(&params, thread_id, turn_id)?;
                        inspect_item(
                            params
                                .get("item")
                                .ok_or(CodexError::Protocol("turn item missing"))?,
                            &mut turn,
                        )?;
                    }
                    "agentMessage/delta" | "turn/started" | "thread/status/changed" => {}
                    "turn/completed" => {
                        if params.get("threadId").and_then(Value::as_str) != Some(thread_id)
                            || params.pointer("/turn/id").and_then(Value::as_str) != Some(turn_id)
                            || params.pointer("/turn/status").and_then(Value::as_str)
                                != Some("completed")
                        {
                            return self.fail(CodexError::Protocol("turn completion mismatch"));
                        }
                        let items = params
                            .pointer("/turn/items")
                            .and_then(Value::as_array)
                            .ok_or(CodexError::Protocol("completed turn items missing"))?;
                        turn = TurnAccumulator::default();
                        for item in items {
                            inspect_item(item, &mut turn)?;
                        }
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
                        return StructuredOutcome::parse_and_validate(text, allowed_source_refs);
                    }
                    "error" => return self.fail(CodexError::Protocol("turn error notification")),
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

fn validate_thread(result: &Value, expected_cwd: &str) -> Result<(), CodexError> {
    if result.get("model").and_then(Value::as_str) != Some(REQUIRED_MODEL)
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

const MAX_TURN_ITEMS: usize = 128;
const MAX_TURN_ACCUMULATED_TEXT_BYTES: usize = 1024 * 1024;

#[derive(Default)]
struct TurnAccumulator {
    final_messages: Vec<String>,
    phase_unknown: Vec<String>,
    item_count: usize,
    text_bytes: usize,
}

fn inspect_item(item: &Value, turn: &mut TurnAccumulator) -> Result<(), CodexError> {
    turn.item_count = turn
        .item_count
        .checked_add(1)
        .ok_or(CodexError::Protocol("turn item count overflow"))?;
    if turn.item_count > MAX_TURN_ITEMS {
        return Err(CodexError::Protocol("turn item limit exceeded"));
    }
    let item_type = item
        .get("type")
        .and_then(Value::as_str)
        .ok_or(CodexError::Protocol("turn item type missing"))?;
    match item_type {
        "userMessage" | "reasoning" => Ok(()),
        "agentMessage" => {
            let text = item
                .get("text")
                .and_then(Value::as_str)
                .ok_or(CodexError::Protocol("agent message text missing"))?;
            match item.get("phase").and_then(Value::as_str) {
                Some("final_answer") | None => {
                    turn.text_bytes = turn
                        .text_bytes
                        .checked_add(text.len())
                        .ok_or(CodexError::Protocol("turn text size overflow"))?;
                    if turn.text_bytes > MAX_TURN_ACCUMULATED_TEXT_BYTES {
                        return Err(CodexError::Protocol("turn text limit exceeded"));
                    }
                    if item.get("phase").is_none() {
                        turn.phase_unknown.push(text.to_owned());
                    } else {
                        turn.final_messages.push(text.to_owned());
                    }
                }
                Some("commentary") => {}
                Some(_) => return Err(CodexError::Protocol("unknown agent message phase")),
            }
            Ok(())
        }
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

#[cfg(test)]
mod tests {
    use super::{
        CODEX_BINARY_SHA256, CODEX_CODE_MODE_HOST_SHA256, CODEX_PACKAGE_SHA256, CODEX_RG_SHA256,
        CODEX_VERSION, CodexClient, CodexError, CodexRuntimeConfig,
        MAX_TURN_ACCUMULATED_TEXT_BYTES, MAX_TURN_ITEMS, TurnAccumulator, inspect_item,
        request_allowed, validate_effective_security_config, validate_thread,
    };
    use serde_json::json;
    use sha2::{Digest, Sha256};
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, atomic::AtomicBool};

    #[test]
    fn thread_containment_must_match_exactly() {
        let good = json!({
            "approvalPolicy": "never",
            "cwd": "/tmp/model-input",
            "instructionSources": [],
            "model": "gpt-5.6-sol",
            "modelProvider": "openai",
            "sandbox": {"networkAccess": false, "type": "readOnly"},
            "thread": {"id": "thread-1"}
        });
        validate_thread(&good, "/tmp/model-input").unwrap();
        let mut changed = good;
        changed["sandbox"]["networkAccess"] = json!(true);
        assert!(validate_thread(&changed, "/tmp/model-input").is_err());
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
                "mcp_oauth_credentials_store": "keyring"
            });
            config[key] = json!(value);
            assert!(validate_effective_security_config(&json!({"config": config})).is_err());
        }
        assert!(
            validate_effective_security_config(&json!({
                "config": {
                    "forced_login_method": "chatgpt",
                    "cli_auth_credentials_store": "keyring",
                    "mcp_oauth_credentials_store": "keyring"
                }
            }))
            .is_ok()
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
    fn turn_item_and_accumulated_text_limits_fail_closed() {
        let mut turn = TurnAccumulator::default();
        for _ in 0..MAX_TURN_ITEMS {
            inspect_item(&json!({"type": "userMessage"}), &mut turn).unwrap();
        }
        assert!(matches!(
            inspect_item(&json!({"type": "userMessage"}), &mut turn),
            Err(CodexError::Protocol("turn item limit exceeded"))
        ));

        let mut turn = TurnAccumulator::default();
        let oversized = "x".repeat(MAX_TURN_ACCUMULATED_TEXT_BYTES + 1);
        assert!(matches!(
            inspect_item(
                &json!({"type": "agentMessage", "phase": "final_answer", "text": oversized}),
                &mut turn,
            ),
            Err(CodexError::Protocol("turn text limit exceeded"))
        ));
        assert!(turn.final_messages.is_empty());
    }

    #[test]
    fn pre_cancelled_client_never_touches_the_runtime_path() {
        let root = tempfile::tempdir().unwrap();
        let config = CodexRuntimeConfig {
            runtime: root.path().join("missing-runtime"),
            codex_home: root.path().join("codex-home"),
            synthetic_home: root.path().join("synthetic-home"),
            model_workspace: root.path().join("model-input"),
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
        let codex_home = canonical_root.join("codex-home");
        let synthetic_home = canonical_root.join("synthetic-home");
        let model_workspace = canonical_root.join("model-input");
        std::fs::create_dir(&model_workspace).unwrap();
        let mut client = CodexClient::spawn(&CodexRuntimeConfig {
            runtime: runtime.into(),
            codex_home,
            synthetic_home,
            model_workspace,
        })
        .unwrap();
        let _ = client.read_account().unwrap();
    }

    #[test]
    #[ignore = "requires OPENOPEN_TEST_CODEX_RUNTIME pointing to the exact pinned macOS binary"]
    fn real_pinned_runtime_begins_managed_login_inside_outer_sandbox() {
        let runtime = std::env::var_os("OPENOPEN_TEST_CODEX_RUNTIME")
            .expect("OPENOPEN_TEST_CODEX_RUNTIME is required for this explicit diagnostic");
        let root = tempfile::tempdir().unwrap();
        let canonical_root = std::fs::canonicalize(root.path()).unwrap();
        let codex_home = canonical_root.join("codex-home");
        let synthetic_home = canonical_root.join("synthetic-home");
        let model_workspace = canonical_root.join("model-input");
        std::fs::create_dir(&model_workspace).unwrap();
        let mut client = CodexClient::spawn(&CodexRuntimeConfig {
            runtime: runtime.into(),
            codex_home,
            synthetic_home,
            model_workspace,
        })
        .unwrap();
        let login = client.begin_chatgpt_login().unwrap();
        assert!(login.auth_url.starts_with("https://auth.openai.com/"));
        assert!(!login.login_id.is_empty());
    }
}
