//! Narrow process adapter for the basic `imsg v0.13.0` JSON-RPC/stdio surface.
//!
//! The host owns exactly one adapter. Shutdown closes stdin, waits briefly,
//! then force-reaps the child. No socket, shell, TCP server, `IMCore` bridge, or
//! private helper is opened by this crate.

use serde::de::{DeserializeOwned, Error as DeserializeError};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Component, Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use thiserror::Error;

pub const OPENOPEN_IMESSAGE_PREFIX: &str = "OpenOpen · AI";
pub const OPENOPEN_IMESSAGE_ADDRESS: &str = "@OpenOpen";
pub const MAX_FRAME_BYTES: usize = 1024 * 1024;
pub const MAX_PENDING_REQUESTS: usize = 64;
pub const MAX_OUTBOUND_TEXT_BYTES: usize = 32 * 1024;
pub const MAX_INBOUND_TEXT_BYTES: usize = 32 * 1024;
pub const MAX_HISTORY_MESSAGES: u16 = 200;
pub const MAX_CHAT_LIST_ITEMS: u16 = 200;
pub const SEND_HAS_CALLER_IDEMPOTENCY_KEY: bool = false;
pub const HISTORY_SUPPORTS_SINCE_ROWID: bool = false;
pub const WATCH_CURSOR_IS_EXCLUSIVE: bool = true;
const EVENT_QUEUE_CAPACITY: usize = 128;
const MAX_GUID_BYTES: usize = 256;
const MAX_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone)]
pub struct AdapterConfig {
    executable: PathBuf,
    request_timeout: Duration,
    shutdown_timeout: Duration,
}

impl AdapterConfig {
    pub fn new(executable: impl Into<PathBuf>) -> Self {
        Self {
            executable: executable.into(),
            request_timeout: Duration::from_secs(10),
            shutdown_timeout: Duration::from_secs(2),
        }
    }

    #[must_use]
    pub fn with_request_timeout(mut self, timeout: Duration) -> Self {
        self.request_timeout = timeout;
        self
    }

    #[must_use]
    pub fn with_shutdown_timeout(mut self, timeout: Duration) -> Self {
        self.shutdown_timeout = timeout;
        self
    }

    fn validate(&self) -> Result<(), AdapterError> {
        validate_absolute_regular_file_without_symlinks(&self.executable)?;
        let metadata = self
            .executable
            .symlink_metadata()
            .map_err(|_| AdapterError::InvalidExecutable)?;
        if !metadata.is_file() || metadata.permissions().mode() & 0o111 == 0 {
            return Err(AdapterError::InvalidExecutable);
        }
        if self.request_timeout.is_zero() || self.request_timeout > MAX_REQUEST_TIMEOUT {
            return Err(AdapterError::InvalidTimeout);
        }
        if self.shutdown_timeout.is_zero() || self.shutdown_timeout > MAX_SHUTDOWN_TIMEOUT {
            return Err(AdapterError::InvalidTimeout);
        }
        Ok(())
    }
}

fn validate_absolute_regular_file_without_symlinks(path: &Path) -> Result<(), AdapterError> {
    if !path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::CurDir | Component::ParentDir))
    {
        return Err(AdapterError::InvalidExecutable);
    }

    let mut current = PathBuf::new();
    for component in path.components() {
        match component {
            Component::RootDir | Component::Normal(_) => current.push(component.as_os_str()),
            Component::Prefix(_) | Component::CurDir | Component::ParentDir => {
                return Err(AdapterError::InvalidExecutable);
            }
        }
        let metadata = current
            .symlink_metadata()
            .map_err(|_| AdapterError::InvalidExecutable)?;
        if metadata.file_type().is_symlink() {
            return Err(AdapterError::InvalidExecutable);
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum AdapterError {
    #[error(
        "imsg executable must be an absolute executable regular file with no symlink components"
    )]
    InvalidExecutable,
    #[error("adapter timeout is outside the accepted bounded range")]
    InvalidTimeout,
    #[error("imsg process failed to start")]
    SpawnFailed,
    #[error("imsg child did not expose all required stdio pipes")]
    MissingPipe,
    #[error("imsg adapter is stopped")]
    Stopped,
    #[error("imsg adapter failed closed: {0}")]
    Faulted(String),
    #[error("too many concurrent imsg requests")]
    TooManyPendingRequests,
    #[error("imsg request frame exceeds the protocol bound")]
    RequestFrameTooLarge,
    #[error("imsg response frame exceeds the protocol bound")]
    ResponseFrameTooLarge,
    #[error("imsg request timed out")]
    RequestTimeout,
    #[error("imsg stdio write failed")]
    WriteFailed,
    #[error("imsg child exited unexpectedly")]
    ChildExited,
    #[error("invalid imsg JSON-RPC frame: {0}")]
    InvalidProtocol(String),
    #[error("imsg JSON-RPC error {code}: {message}")]
    Rpc {
        code: i64,
        message: String,
        data: Option<String>,
    },
    #[error("invalid chat identifier")]
    InvalidChatId,
    #[error("invalid subscription identifier")]
    InvalidSubscriptionId,
    #[error("invalid message cursor")]
    InvalidCursor,
    #[error("invalid outbound message")]
    InvalidOutboundMessage,
    #[error("invalid message GUID")]
    InvalidGuid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct ChatId(i64);

impl ChatId {
    /// Creates a positive Messages database chat row identifier.
    ///
    /// # Errors
    ///
    /// Returns [`AdapterError::InvalidChatId`] for zero or negative values.
    pub fn new(value: i64) -> Result<Self, AdapterError> {
        (value > 0)
            .then_some(Self(value))
            .ok_or(AdapterError::InvalidChatId)
    }

    #[must_use]
    pub const fn get(self) -> i64 {
        self.0
    }
}

impl<'de> Deserialize<'de> for ChatId {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::new(i64::deserialize(deserializer)?).map_err(DeserializeError::custom)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct MessageCursor(i64);

impl MessageCursor {
    /// Creates an exclusive Messages row cursor.
    ///
    /// # Errors
    ///
    /// Returns [`AdapterError::InvalidCursor`] for negative values.
    pub fn new(value: i64) -> Result<Self, AdapterError> {
        (value >= 0)
            .then_some(Self(value))
            .ok_or(AdapterError::InvalidCursor)
    }

    #[must_use]
    pub const fn get(self) -> i64 {
        self.0
    }
}

impl<'de> Deserialize<'de> for MessageCursor {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::new(i64::deserialize(deserializer)?).map_err(DeserializeError::custom)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct SubscriptionId(u64);

impl SubscriptionId {
    fn new(value: u64) -> Result<Self, AdapterError> {
        (value > 0)
            .then_some(Self(value))
            .ok_or(AdapterError::InvalidSubscriptionId)
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Chat {
    pub id: ChatId,
    pub identifier: String,
    pub guid: String,
    pub name: String,
    pub service: String,
    pub last_message_at: String,
    pub participants: Vec<String>,
    pub is_group: bool,
    pub contact_name: Option<String>,
    pub unread_count: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Message {
    pub id: i64,
    pub chat_id: ChatId,
    pub chat_identifier: String,
    pub chat_guid: String,
    pub chat_name: String,
    #[serde(default)]
    pub participants: Vec<String>,
    pub is_group: bool,
    pub guid: String,
    pub sender: String,
    pub sender_name: Option<String>,
    pub is_from_me: bool,
    pub text: String,
    pub created_at: String,
    pub reply_to_guid: Option<String>,
    pub reply_to_text: Option<String>,
    pub reply_to_sender: Option<String>,
    pub destination_caller_id: Option<String>,
    pub is_read: Option<bool>,
    pub date_read: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImsgEvent {
    Message {
        subscription: SubscriptionId,
        message: Box<Message>,
    },
    SubscriptionError {
        subscription: SubscriptionId,
        message: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ListChatsRequest {
    pub limit: u16,
    pub unread_only: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HistoryRequest {
    pub chat_id: ChatId,
    pub limit: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WatchRequest {
    pub chat_id: ChatId,
    pub since_rowid: Option<MessageCursor>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SendRequest {
    pub chat_id: ChatId,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutboundRecoveryRequest {
    pub chat_id: ChatId,
    pub body: String,
    pub after_rowid: MessageCursor,
    pub history_limit: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutboundObservation {
    pub rowid: i64,
    pub guid: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutboundRecovery {
    /// One exact local chat.db candidate. This does not prove effect ownership
    /// or remote delivery; use `message.send_status` when the GUID is present.
    SingleLocalCandidate(OutboundObservation),
    /// No candidate was present in the bounded window. This does not prove noncommit.
    NoLocalCandidate,
    /// More than one local row matched, so effect attribution is impossible.
    AmbiguousLocalCandidates(Vec<OutboundObservation>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InboundPairing {
    pub chat_id: ChatId,
    pub owner_sender: String,
}

impl InboundPairing {
    /// Creates the exact chat/sender pair accepted by the V1 inbound parser.
    ///
    /// # Errors
    ///
    /// Returns [`InboundRejection::InvalidPairing`] for an empty, oversized,
    /// or NUL-containing owner sender identifier.
    pub fn new(chat_id: ChatId, owner_sender: impl Into<String>) -> Result<Self, InboundRejection> {
        let owner_sender = owner_sender.into();
        if owner_sender.trim().is_empty()
            || owner_sender.len() > MAX_GUID_BYTES
            || owner_sender.as_bytes().contains(&0)
        {
            return Err(InboundRejection::InvalidPairing);
        }
        Ok(Self {
            chat_id,
            owner_sender,
        })
    }
}

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum InboundRejection {
    #[error("invalid iMessage pairing")]
    InvalidPairing,
    #[error("message came from a different chat")]
    ChatNotPaired,
    #[error("message did not come from the paired owner")]
    SenderNotOwner,
    #[error("outbound echo is not accepted as inbound work")]
    FromLocalUser,
    #[error("message is not explicitly addressed to OpenOpen")]
    NotAddressed,
    #[error("addressed message has no body")]
    EmptyBody,
    #[error("addressed message body exceeds the bound")]
    BodyTooLarge,
    #[error("message has no valid rowid or GUID")]
    InvalidSourceIdentity,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedInbound {
    pub chat_id: ChatId,
    pub sender: String,
    pub source_rowid: i64,
    pub source_guid: String,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SendAcceptance {
    pub ok: bool,
    pub id: Option<i64>,
    pub guid: Option<String>,
    pub message_id: Option<String>,
    pub chat_guid: Option<String>,
    pub service: Option<String>,
    pub transport: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SendState {
    Pending,
    Sent,
    Delivered,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[allow(clippy::struct_excessive_bools)] // Exact pinned imsg send-status wire fields.
pub struct SendStatusFields {
    pub is_sent: bool,
    pub is_delivered: bool,
    pub is_finished: bool,
    pub error: i64,
    pub date_delivered: Option<String>,
    pub date_read: Option<String>,
    pub is_delayed: bool,
    pub is_prepared: bool,
    pub is_pending_satellite_send: bool,
    pub was_downgraded: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SendStatus {
    pub ok: bool,
    pub guid: String,
    pub send_state: SendState,
    pub service: Option<String>,
    pub checked_at: String,
    pub delivered_at: Option<String>,
    pub status_fields: Option<SendStatusFields>,
}

#[derive(Debug)]
enum Lifecycle {
    Running,
    Closing,
    Faulted(AdapterError),
    Stopped,
}

type PendingSender = SyncSender<Result<Value, AdapterError>>;

#[derive(Debug)]
struct SharedState {
    lifecycle: Lifecycle,
    pending: HashMap<u64, PendingSender>,
}

#[derive(Deserialize)]
struct ChatsResult {
    chats: Vec<Chat>,
}

#[derive(Deserialize)]
struct HistoryResult {
    messages: Vec<Message>,
}

#[derive(Deserialize)]
struct SubscriptionResult {
    subscription: u64,
}

#[derive(Deserialize)]
struct OkResult {
    ok: bool,
}

impl SharedState {
    fn new() -> Self {
        Self {
            lifecycle: Lifecycle::Running,
            pending: HashMap::new(),
        }
    }
}

#[derive(Debug)]
pub struct ImsgAdapter {
    request_timeout: Duration,
    shutdown_timeout: Duration,
    next_id: AtomicU64,
    stdin: Mutex<Option<ChildStdin>>,
    child: Mutex<Option<Child>>,
    shared: Arc<Mutex<SharedState>>,
    events: Mutex<Receiver<ImsgEvent>>,
    stdout_thread: Mutex<Option<JoinHandle<()>>>,
    stderr_thread: Mutex<Option<JoinHandle<()>>>,
}

impl ImsgAdapter {
    /// Starts one directly executed `imsg rpc` child with piped stdio.
    ///
    /// # Errors
    ///
    /// Returns an error when configuration is invalid, process creation fails,
    /// or the child does not expose all required stdio pipes.
    pub fn spawn(config: &AdapterConfig) -> Result<Self, AdapterError> {
        config.validate()?;
        let mut child = Command::new(&config.executable)
            .arg("rpc")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|_| AdapterError::SpawnFailed)?;
        let stdin = child.stdin.take().ok_or(AdapterError::MissingPipe)?;
        let stdout = child.stdout.take().ok_or(AdapterError::MissingPipe)?;
        let stderr = child.stderr.take().ok_or(AdapterError::MissingPipe)?;
        let shared = Arc::new(Mutex::new(SharedState::new()));
        let (event_tx, event_rx) = mpsc::sync_channel(EVENT_QUEUE_CAPACITY);
        let stdout_shared = Arc::clone(&shared);
        let stdout_thread = thread::Builder::new()
            .name("openopen-imsg-stdout".into())
            .spawn(move || read_stdout(stdout, &stdout_shared, &event_tx))
            .map_err(|_| AdapterError::SpawnFailed)?;
        let stderr_thread = thread::Builder::new()
            .name("openopen-imsg-stderr".into())
            .spawn(move || drain_stderr(stderr))
            .map_err(|_| AdapterError::SpawnFailed)?;

        Ok(Self {
            request_timeout: config.request_timeout,
            shutdown_timeout: config.shutdown_timeout,
            next_id: AtomicU64::new(1),
            stdin: Mutex::new(Some(stdin)),
            child: Mutex::new(Some(child)),
            shared,
            events: Mutex::new(event_rx),
            stdout_thread: Mutex::new(Some(stdout_thread)),
            stderr_thread: Mutex::new(Some(stderr_thread)),
        })
    }

    /// Returns the exact still-owned child process identifier so the signed
    /// macOS host can validate that running incarnation before sending any RPC
    /// request bytes.
    ///
    /// # Errors
    ///
    /// Returns an error after shutdown or when the child has already exited.
    pub fn process_identifier(&self) -> Result<u32, AdapterError> {
        let mut child = lock(&self.child)?;
        let process = child.as_mut().ok_or(AdapterError::Stopped)?;
        if process
            .try_wait()
            .map_err(|_| AdapterError::ChildExited)?
            .is_some()
        {
            return Err(AdapterError::ChildExited);
        }
        Ok(process.id())
    }

    /// Lists chats for owner pairing.
    ///
    /// # Errors
    ///
    /// Returns an error for a zero limit or any process/protocol failure.
    pub fn list_chats(&self, request: ListChatsRequest) -> Result<Vec<Chat>, AdapterError> {
        if request.limit == 0 || request.limit > MAX_CHAT_LIST_ITEMS {
            return Err(AdapterError::InvalidProtocol(
                "chat-list limit is outside the bound".into(),
            ));
        }
        let result: ChatsResult = self.request(
            "chats.list",
            &json!({"limit": request.limit, "unread_only": request.unread_only}),
        )?;
        if result.chats.len() > usize::from(request.limit) {
            return Err(AdapterError::InvalidProtocol(
                "chat list exceeded the requested bound".into(),
            ));
        }
        Ok(result.chats)
    }

    /// Reads bounded history for one exact paired chat.
    ///
    /// # Errors
    ///
    /// Returns an error for a zero limit or any process/protocol failure.
    pub fn history(&self, request: HistoryRequest) -> Result<Vec<Message>, AdapterError> {
        if request.limit == 0 || request.limit > MAX_HISTORY_MESSAGES {
            return Err(AdapterError::InvalidProtocol(
                "history limit is outside the bound".into(),
            ));
        }
        let result: HistoryResult = self.request(
            "messages.history",
            &json!({
                "chat_id": request.chat_id,
                "limit": request.limit,
                "attachments": false
            }),
        )?;
        if result
            .messages
            .iter()
            .any(|message| message.id <= 0 || message.chat_id != request.chat_id)
        {
            return Err(AdapterError::InvalidProtocol(
                "history returned an invalid or cross-chat message".into(),
            ));
        }
        Ok(result.messages)
    }

    /// Performs a bounded, read-only response-loss recovery scan.
    ///
    /// The pinned imsg `send` method has no caller idempotency key. Therefore
    /// every result remains recovery-only: no result may be converted into
    /// another send. Even one local candidate is not proof of effect ownership
    /// or remote delivery.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid text/limits or a history process/protocol failure.
    pub fn recover_outbound(
        &self,
        request: &OutboundRecoveryRequest,
    ) -> Result<OutboundRecovery, AdapterError> {
        let expected_text = prefixed_outbound_text(&request.body)?;
        let messages = self.history(HistoryRequest {
            chat_id: request.chat_id,
            limit: request.history_limit,
        })?;
        let mut matches = messages
            .into_iter()
            .filter(|message| {
                message.id > request.after_rowid.get()
                    && message.is_from_me
                    && message.text == expected_text
            })
            .map(|message| OutboundObservation {
                rowid: message.id,
                guid: valid_guid(&message.guid).then(|| message.guid.clone()),
                created_at: message.created_at,
            })
            .collect::<Vec<_>>();
        matches.sort_by_key(|observation| observation.rowid);
        match matches.len() {
            0 => Ok(OutboundRecovery::NoLocalCandidate),
            1 => Ok(OutboundRecovery::SingleLocalCandidate(matches.remove(0))),
            _ => Ok(OutboundRecovery::AmbiguousLocalCandidates(matches)),
        }
    }

    /// Starts a watch scoped to one exact paired chat and optional cursor.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid subscription result or process failure.
    pub fn subscribe(&self, request: WatchRequest) -> Result<SubscriptionId, AdapterError> {
        let mut params = json!({
            "chat_id": request.chat_id,
            "attachments": false,
            "include_reactions": false,
            "debounce_ms": 500
        });
        if let Some(cursor) = request.since_rowid {
            params["since_rowid"] = json!(cursor);
        }
        let result: SubscriptionResult = self.request("watch.subscribe", &params)?;
        SubscriptionId::new(result.subscription)
    }

    /// Stops one active watch subscription.
    ///
    /// # Errors
    ///
    /// Returns an error when imsg does not confirm cancellation or the process fails.
    pub fn unsubscribe(&self, subscription: SubscriptionId) -> Result<(), AdapterError> {
        let result: OkResult =
            self.request("watch.unsubscribe", &json!({"subscription": subscription}))?;
        if !result.ok {
            return Err(AdapterError::InvalidProtocol(
                "watch.unsubscribe returned ok=false".into(),
            ));
        }
        Ok(())
    }

    /// Sends prefixed text to one exact chat through basic `AppleScript` transport.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid text, process/protocol failure, or any
    /// response that does not confirm the non-bridge transport.
    pub fn execute_send(&self, request: &SendRequest) -> Result<SendAcceptance, AdapterError> {
        let text = prefixed_outbound_text(&request.body)?;
        let result: SendAcceptance = self.request(
            "send",
            &json!({
                "chat_id": request.chat_id,
                "text": text,
                "service": "auto",
                "transport": "applescript"
            }),
        )?;
        if !result.ok || result.transport != "applescript" {
            return Err(AdapterError::InvalidProtocol(
                "send did not confirm the basic AppleScript transport".into(),
            ));
        }
        if result.id.is_some_and(|id| id <= 0)
            || result.guid.as_deref().is_some_and(|guid| !valid_guid(guid))
            || result
                .message_id
                .as_deref()
                .is_some_and(|guid| !valid_guid(guid))
            || matches!(
                (&result.guid, &result.message_id),
                (Some(guid), Some(message_id)) if guid != message_id
            )
        {
            return Err(AdapterError::InvalidProtocol(
                "send returned an invalid or inconsistent message identity".into(),
            ));
        }
        Ok(result)
    }

    /// Reads transport delivery state for one exact outbound GUID.
    ///
    /// This status is transport information and is never Mission completion Evidence.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid GUID, process/protocol failure, or a
    /// response bound to a different GUID.
    pub fn send_status(&self, guid: &str) -> Result<SendStatus, AdapterError> {
        let guid = guid.trim();
        if guid.is_empty() || guid.len() > MAX_GUID_BYTES || guid.as_bytes().contains(&0) {
            return Err(AdapterError::InvalidGuid);
        }
        let result: SendStatus = self.request("message.send_status", &json!({"guid": guid}))?;
        if !result.ok || result.guid != guid {
            return Err(AdapterError::InvalidProtocol(
                "send status response did not bind the requested GUID".into(),
            ));
        }
        Ok(result)
    }

    /// Waits for one bounded watch event without accumulating an unbounded queue.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid timeout, protocol fault, or child exit.
    pub fn recv_event_timeout(&self, timeout: Duration) -> Result<Option<ImsgEvent>, AdapterError> {
        if timeout > MAX_REQUEST_TIMEOUT {
            return Err(AdapterError::InvalidTimeout);
        }
        let receiver = lock(&self.events)?;
        match receiver.recv_timeout(timeout) {
            Ok(event) => Ok(Some(event)),
            Err(mpsc::RecvTimeoutError::Timeout) => Ok(None),
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                let shared = lock(&self.shared)?;
                match &shared.lifecycle {
                    Lifecycle::Faulted(error) => Err(error.clone()),
                    Lifecycle::Running => Err(AdapterError::ChildExited),
                    Lifecycle::Closing | Lifecycle::Stopped => Err(AdapterError::Stopped),
                }
            }
        }
    }

    /// Closes child stdin and guarantees that the process and drain threads end.
    ///
    /// # Errors
    ///
    /// Returns an error if the child cannot be inspected/reaped or a drain
    /// thread panics.
    pub fn shutdown(&self) -> Result<(), AdapterError> {
        {
            let mut shared = lock(&self.shared)?;
            match shared.lifecycle {
                Lifecycle::Stopped => return Ok(()),
                Lifecycle::Closing => {}
                Lifecycle::Running | Lifecycle::Faulted(_) => {
                    shared.lifecycle = Lifecycle::Closing;
                    fail_pending(&mut shared, &AdapterError::Stopped);
                }
            }
        }
        lock(&self.stdin)?.take();
        let deadline = Instant::now() + self.shutdown_timeout;
        let mut child_guard = lock(&self.child)?;
        if let Some(child) = child_guard.as_mut() {
            loop {
                match child.try_wait() {
                    Ok(Some(_)) => break,
                    Ok(None) if Instant::now() < deadline => {
                        thread::sleep(Duration::from_millis(10));
                    }
                    Ok(None) => {
                        child.kill().map_err(|_| AdapterError::WriteFailed)?;
                        child.wait().map_err(|_| AdapterError::WriteFailed)?;
                        break;
                    }
                    Err(_) => return Err(AdapterError::ChildExited),
                }
            }
        }
        child_guard.take();
        drop(child_guard);
        join_thread(&self.stdout_thread)?;
        join_thread(&self.stderr_thread)?;
        lock(&self.shared)?.lifecycle = Lifecycle::Stopped;
        Ok(())
    }

    fn request<T: DeserializeOwned>(
        &self,
        method: &str,
        params: &Value,
    ) -> Result<T, AdapterError> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        if id == 0 {
            let error = AdapterError::Faulted("request id space exhausted".into());
            mark_fault(&self.shared, &error);
            return Err(error);
        }
        let mut encoded = serde_json::to_vec(&json!({
            "jsonrpc": "2.0", "id": id, "method": method, "params": params
        }))
        .map_err(|_| AdapterError::InvalidProtocol("request serialization failed".into()))?;
        if encoded.len() + 1 > MAX_FRAME_BYTES {
            return Err(AdapterError::RequestFrameTooLarge);
        }
        encoded.push(b'\n');
        let (sender, receiver) = mpsc::sync_channel(1);
        {
            let mut shared = lock(&self.shared)?;
            match &shared.lifecycle {
                Lifecycle::Running => {}
                Lifecycle::Faulted(error) => return Err(error.clone()),
                Lifecycle::Closing | Lifecycle::Stopped => return Err(AdapterError::Stopped),
            }
            if shared.pending.len() >= MAX_PENDING_REQUESTS {
                return Err(AdapterError::TooManyPendingRequests);
            }
            shared.pending.insert(id, sender);
        }
        let write_result = (|| {
            let mut stdin_guard = lock(&self.stdin)?;
            let stdin = stdin_guard.as_mut().ok_or(AdapterError::Stopped)?;
            stdin
                .write_all(&encoded)
                .and_then(|()| stdin.flush())
                .map_err(|_| AdapterError::WriteFailed)
        })();
        if let Err(error) = write_result {
            remove_pending(&self.shared, id);
            mark_fault(&self.shared, &error);
            return Err(error);
        }
        let value = match receiver.recv_timeout(self.request_timeout) {
            Ok(result) => result?,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                remove_pending(&self.shared, id);
                mark_fault(&self.shared, &AdapterError::RequestTimeout);
                return Err(AdapterError::RequestTimeout);
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => return Err(AdapterError::ChildExited),
        };
        serde_json::from_value(value)
            .map_err(|_| AdapterError::InvalidProtocol(format!("invalid {method} result")))
    }
}

/// Applies the one V1 iMessage inbound-addressing rule without side effects.
///
/// # Errors
///
/// Returns a specific rejection when chat, sender, direction, address prefix,
/// body bound, or source identity fails closed.
pub fn normalize_inbound(
    pairing: &InboundPairing,
    message: &Message,
) -> Result<NormalizedInbound, InboundRejection> {
    if message.chat_id != pairing.chat_id {
        return Err(InboundRejection::ChatNotPaired);
    }
    if message.sender != pairing.owner_sender {
        return Err(InboundRejection::SenderNotOwner);
    }
    if message.is_from_me {
        return Err(InboundRejection::FromLocalUser);
    }
    if message.id <= 0 || !valid_guid(&message.guid) {
        return Err(InboundRejection::InvalidSourceIdentity);
    }
    let suffix = message
        .text
        .strip_prefix(OPENOPEN_IMESSAGE_ADDRESS)
        .ok_or(InboundRejection::NotAddressed)?;
    let Some(first) = suffix.chars().next() else {
        return Err(InboundRejection::EmptyBody);
    };
    if !first.is_whitespace() && !matches!(first, ':' | ',') {
        return Err(InboundRejection::NotAddressed);
    }
    let body = suffix
        .trim_start_matches(|character: char| {
            character.is_whitespace() || matches!(character, ':' | ',')
        })
        .trim();
    if body.is_empty() {
        return Err(InboundRejection::EmptyBody);
    }
    if body.len() > MAX_INBOUND_TEXT_BYTES || body.as_bytes().contains(&0) {
        return Err(InboundRejection::BodyTooLarge);
    }
    Ok(NormalizedInbound {
        chat_id: message.chat_id,
        sender: message.sender.clone(),
        source_rowid: message.id,
        source_guid: message.guid.clone(),
        body: body.to_owned(),
    })
}

fn prefixed_outbound_text(body: &str) -> Result<String, AdapterError> {
    let body = body.trim();
    if body.is_empty() || body.as_bytes().contains(&0) || body.starts_with(OPENOPEN_IMESSAGE_PREFIX)
    {
        return Err(AdapterError::InvalidOutboundMessage);
    }
    let text = format!("{OPENOPEN_IMESSAGE_PREFIX}\n{body}");
    if text.len() > MAX_OUTBOUND_TEXT_BYTES {
        return Err(AdapterError::InvalidOutboundMessage);
    }
    Ok(text)
}

fn valid_guid(guid: &str) -> bool {
    !guid.trim().is_empty() && guid.len() <= MAX_GUID_BYTES && !guid.as_bytes().contains(&0)
}

impl Drop for ImsgAdapter {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

#[derive(Debug, Deserialize)]
struct RpcErrorBody {
    code: i64,
    message: String,
    data: Option<String>,
}

fn read_stdout(
    stdout: impl Read,
    shared: &Arc<Mutex<SharedState>>,
    events: &SyncSender<ImsgEvent>,
) {
    let mut reader = BufReader::new(stdout);
    loop {
        let line = match read_bounded_line(&mut reader, MAX_FRAME_BYTES) {
            Ok(Some(line)) => line,
            Ok(None) => {
                let should_fault =
                    lock(shared).is_ok_and(|state| matches!(state.lifecycle, Lifecycle::Running));
                if should_fault {
                    mark_fault(shared, &AdapterError::ChildExited);
                }
                return;
            }
            Err(error) => {
                mark_fault(shared, &error);
                return;
            }
        };
        if let Err(error) = handle_inbound(&line, shared, events) {
            mark_fault(shared, &error);
            return;
        }
    }
}

fn handle_inbound(
    line: &[u8],
    shared: &Arc<Mutex<SharedState>>,
    events: &SyncSender<ImsgEvent>,
) -> Result<(), AdapterError> {
    let value: Value = serde_json::from_slice(line)
        .map_err(|_| AdapterError::InvalidProtocol("stdout was not valid JSON".into()))?;
    let object = value
        .as_object()
        .ok_or_else(|| AdapterError::InvalidProtocol("frame was not an object".into()))?;
    if object.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return Err(AdapterError::InvalidProtocol(
            "jsonrpc must equal 2.0".into(),
        ));
    }
    if let Some(id) = object.get("id") {
        let id = id
            .as_u64()
            .ok_or_else(|| AdapterError::InvalidProtocol("response id was not a u64".into()))?;
        let response = match (object.get("result"), object.get("error")) {
            (Some(result), None) => Ok(result.clone()),
            (None, Some(error)) => {
                let error: RpcErrorBody = serde_json::from_value(error.clone()).map_err(|_| {
                    AdapterError::InvalidProtocol("invalid JSON-RPC error body".into())
                })?;
                Err(AdapterError::Rpc {
                    code: error.code,
                    message: error.message,
                    data: error.data,
                })
            }
            _ => {
                return Err(AdapterError::InvalidProtocol(
                    "response requires exactly one result or error".into(),
                ));
            }
        };
        let sender = lock(shared)?
            .pending
            .remove(&id)
            .ok_or_else(|| AdapterError::InvalidProtocol("unknown response id".into()))?;
        sender
            .send(response)
            .map_err(|_| AdapterError::InvalidProtocol("response receiver vanished".into()))?;
        return Ok(());
    }
    let method = object
        .get("method")
        .and_then(Value::as_str)
        .ok_or_else(|| AdapterError::InvalidProtocol("notification method missing".into()))?;
    let params = object
        .get("params")
        .cloned()
        .ok_or_else(|| AdapterError::InvalidProtocol("notification params missing".into()))?;
    let event = match method {
        "message" => parse_message_event(params)?,
        "error" => parse_error_event(params)?,
        _ => {
            return Err(AdapterError::InvalidProtocol(
                "unsupported notification method".into(),
            ));
        }
    };
    events.try_send(event).map_err(|error| match error {
        TrySendError::Full(_) => AdapterError::Faulted("bounded imsg event queue is full".into()),
        TrySendError::Disconnected(_) => AdapterError::Stopped,
    })
}

fn parse_message_event(params: Value) -> Result<ImsgEvent, AdapterError> {
    #[derive(Deserialize)]
    struct Params {
        subscription: u64,
        message: Message,
    }
    let params: Params = serde_json::from_value(params)
        .map_err(|_| AdapterError::InvalidProtocol("invalid message notification".into()))?;
    Ok(ImsgEvent::Message {
        subscription: SubscriptionId::new(params.subscription)?,
        message: Box::new(params.message),
    })
}

fn parse_error_event(params: Value) -> Result<ImsgEvent, AdapterError> {
    #[derive(Deserialize)]
    struct ErrorValue {
        message: String,
    }
    #[derive(Deserialize)]
    struct Params {
        subscription: u64,
        error: ErrorValue,
    }
    let params: Params = serde_json::from_value(params)
        .map_err(|_| AdapterError::InvalidProtocol("invalid watch error notification".into()))?;
    Ok(ImsgEvent::SubscriptionError {
        subscription: SubscriptionId::new(params.subscription)?,
        message: params.error.message,
    })
}

fn read_bounded_line(
    reader: &mut impl BufRead,
    max_bytes: usize,
) -> Result<Option<Vec<u8>>, AdapterError> {
    let mut output = Vec::with_capacity(4096);
    loop {
        let available = reader.fill_buf().map_err(|_| AdapterError::ChildExited)?;
        if available.is_empty() {
            return if output.is_empty() {
                Ok(None)
            } else {
                Err(AdapterError::InvalidProtocol(
                    "stdout ended mid-frame".into(),
                ))
            };
        }
        let newline = available.iter().position(|byte| *byte == b'\n');
        let consumed = newline.map_or(available.len(), |index| index + 1);
        let payload_len = newline.unwrap_or(available.len());
        if output.len().saturating_add(payload_len) > max_bytes {
            return Err(AdapterError::ResponseFrameTooLarge);
        }
        output.extend_from_slice(&available[..payload_len]);
        reader.consume(consumed);
        if newline.is_some() {
            if output.last() == Some(&b'\r') {
                output.pop();
            }
            if output.is_empty() {
                return Err(AdapterError::InvalidProtocol("empty stdout frame".into()));
            }
            return Ok(Some(output));
        }
    }
}

fn drain_stderr(mut stderr: impl Read) {
    let mut buffer = [0_u8; 4096];
    loop {
        match stderr.read(&mut buffer) {
            Ok(0) | Err(_) => return,
            Ok(_) => {}
        }
    }
}

fn mark_fault(shared: &Arc<Mutex<SharedState>>, error: &AdapterError) {
    if let Ok(mut shared) = shared.lock()
        && matches!(shared.lifecycle, Lifecycle::Running)
    {
        shared.lifecycle = Lifecycle::Faulted(error.clone());
        fail_pending(&mut shared, error);
    }
}

fn fail_pending(shared: &mut SharedState, error: &AdapterError) {
    for (_, sender) in shared.pending.drain() {
        let _ = sender.send(Err(error.clone()));
    }
}

fn remove_pending(shared: &Arc<Mutex<SharedState>>, id: u64) {
    if let Ok(mut shared) = shared.lock() {
        shared.pending.remove(&id);
    }
}

fn lock<T>(mutex: &Mutex<T>) -> Result<MutexGuard<'_, T>, AdapterError> {
    mutex
        .lock()
        .map_err(|_| AdapterError::Faulted("adapter lock poisoned".into()))
}

fn join_thread(thread: &Mutex<Option<JoinHandle<()>>>) -> Result<(), AdapterError> {
    if let Some(thread) = lock(thread)?.take() {
        thread
            .join()
            .map_err(|_| AdapterError::Faulted("adapter worker panicked".into()))?;
    }
    Ok(())
}
