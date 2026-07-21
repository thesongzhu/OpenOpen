use openopen_discord_adapter::{
    BotToken, ConnectionStatus as DiscordConnectionStatus, DeliveryRecoveryRequest, DiscordAdapter,
    DiscordPairing, DiscordPairingCandidate, DiscordSetupAdapter, DiscordSetupStart,
    InboundEnvelope as DiscordInbound, OutboundRequest, RecoveryBatch, RecoveryRequest,
};
use openopen_imsg_adapter::{
    AdapterConfig as ImsgConfig, Chat, ChatId, HistoryRequest, ImsgAdapter, ImsgEvent,
    InboundPairing, InboundRejection, ListChatsRequest, Message, MessageCursor,
    OPENOPEN_IMESSAGE_PREFIX, OutboundRecovery, OutboundRecoveryRequest, SelfChatMessage,
    SendRequest, SendState, SubscriptionId, WatchRequest, classify_self_chat_message,
};
use openopen_protocol::{ChannelCursor, ChannelKind, ChannelPairing, IMessagePairingMetadata};
use serde::Serialize;
use std::collections::VecDeque;
use std::path::Path;
use std::sync::Arc;
#[cfg(test)]
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::Duration;
use thiserror::Error;
use tokio::runtime::Runtime;
use tokio::sync::mpsc::error::TryRecvError as TokioTryRecvError;
use zeroize::Zeroizing;

const DISCORD_INBOUND_CAPACITY: usize = 32;
const DISCORD_RECOVERY_PAGES: u8 = 10;
const MAX_IMESSAGE_CHAT_FIELD_BYTES: usize = 256;
const MAX_IMESSAGE_CHAT_PARTICIPANTS: usize = 64;
const SELF_CHAT_IDENTITY_HISTORY_LIMIT: u16 = 32;

#[derive(Debug, Error)]
pub(crate) enum ChannelRuntimeError {
    #[error("channel runtime initialization failed")]
    Runtime(#[from] std::io::Error),
    #[error("channel is already running")]
    AlreadyRunning,
    #[error("channel pairing does not match the durable boundary")]
    PairingMismatch,
    #[error("channel adapter failed")]
    Adapter,
    #[error("channel recovery failed")]
    Recovery,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum ChannelConnectionStatus {
    Disconnected,
    Connecting,
    Connected,
    Reconnecting,
    Faulted,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TransportInbound {
    pub channel: ChannelKind,
    pub source_message_id: String,
    pub sender_id: String,
    pub conversation_id: String,
    pub content: String,
    pub cursor_opaque_value: String,
    pub cursor_order: u64,
    pub received_at_ms: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum TransportEvent {
    Inbound(TransportInbound),
    Cursor(ChannelCursor),
    IMessageEcho {
        provider_message_id: String,
        cursor: ChannelCursor,
    },
}

struct DiscordSession {
    adapter: DiscordAdapter,
    #[cfg(test)]
    status_override: Option<DiscordConnectionStatus>,
    inbound: tokio::sync::mpsc::Receiver<DiscordInbound>,
    recovery: Option<Receiver<Result<RecoveryBatch, ()>>>,
    recovered: VecDeque<TransportEvent>,
    recovery_waiting: Option<TransportEvent>,
    recovery_required: bool,
    launch_pending: Arc<AtomicBool>,
    pairing: ChannelPairing,
    conversation_id: String,
}

struct DiscordSetupSession {
    adapter: DiscordSetupAdapter,
    candidates: tokio::sync::mpsc::Receiver<DiscordPairingCandidate>,
    pending_candidate: Option<DiscordPairingCandidate>,
}

struct ImsgSession {
    adapter: Arc<ImsgAdapter>,
    pairing: InboundPairing,
    subscription: SubscriptionId,
    conversation_id: String,
    identity: IMessagePairingMetadata,
}

struct PreparedImsgSession {
    adapter: Arc<ImsgAdapter>,
    pairing: InboundPairing,
    since_rowid: Option<MessageCursor>,
    conversation_id: String,
    identity: IMessagePairingMetadata,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ImessageChat {
    pub chat_id: String,
    pub chat_guid: String,
    pub chat_identifier: String,
    pub name: String,
    pub service: String,
    pub participants: Vec<String>,
}

#[derive(Clone)]
pub(crate) enum ChannelSendHandle {
    Discord {
        adapter: DiscordAdapter,
        runtime: tokio::runtime::Handle,
        conversation_id: String,
    },
    IMessage {
        adapter: Arc<ImsgAdapter>,
        chat_id: ChatId,
    },
    #[cfg(test)]
    Test {
        sends: Arc<AtomicUsize>,
        recoveries: Arc<AtomicUsize>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ChannelSendResult {
    Accepted { provider_message_id: String },
    Uncertain,
}

impl ChannelSendHandle {
    pub(crate) fn send(&self, outbound_id: &str, content: &str) -> ChannelSendResult {
        match self {
            Self::Discord {
                adapter,
                runtime,
                conversation_id,
            } => runtime
                .block_on(adapter.send(&OutboundRequest {
                    outbound_id: outbound_id.to_owned(),
                    conversation_id: conversation_id.clone(),
                    content: content.to_owned(),
                    reply_to_message_id: None,
                }))
                .map_or(ChannelSendResult::Uncertain, |delivery| {
                    ChannelSendResult::Accepted {
                        provider_message_id: delivery.source_message_id,
                    }
                }),
            Self::IMessage { adapter, chat_id } => {
                let Some(body) = imessage_adapter_body(content) else {
                    return ChannelSendResult::Uncertain;
                };
                adapter
                    .execute_send(&SendRequest {
                        chat_id: *chat_id,
                        body: body.to_owned(),
                    })
                    .ok()
                    .and_then(|acceptance| acceptance.guid.or(acceptance.message_id))
                    .map_or(ChannelSendResult::Uncertain, |provider_message_id| {
                        ChannelSendResult::Accepted {
                            provider_message_id,
                        }
                    })
            }
            #[cfg(test)]
            Self::Test { sends, .. } => {
                sends.fetch_add(1, Ordering::AcqRel);
                ChannelSendResult::Uncertain
            }
        }
    }

    pub(crate) fn recover(
        &self,
        outbound_id: &str,
        content: &str,
        cursor: &ChannelCursor,
    ) -> ChannelSendResult {
        match self {
            Self::Discord {
                adapter,
                runtime,
                conversation_id,
            } => runtime
                .block_on(adapter.recover_delivery(&DeliveryRecoveryRequest {
                    outbound_id: outbound_id.to_owned(),
                    conversation_id: conversation_id.clone(),
                    after_message_id: cursor.opaque_value.clone(),
                    max_pages: DISCORD_RECOVERY_PAGES,
                }))
                .ok()
                .flatten()
                .map_or(ChannelSendResult::Uncertain, |delivery| {
                    ChannelSendResult::Accepted {
                        provider_message_id: delivery.source_message_id,
                    }
                }),
            Self::IMessage { adapter, chat_id } => {
                let Ok(after_rowid) = i64::try_from(cursor.order) else {
                    return ChannelSendResult::Uncertain;
                };
                let Ok(after_rowid) = MessageCursor::new(after_rowid) else {
                    return ChannelSendResult::Uncertain;
                };
                let recovery = adapter.recover_outbound(&OutboundRecoveryRequest {
                    chat_id: *chat_id,
                    body: match imessage_adapter_body(content) {
                        Some(body) => body.to_owned(),
                        None => return ChannelSendResult::Uncertain,
                    },
                    after_rowid,
                    history_limit: openopen_imsg_adapter::MAX_HISTORY_MESSAGES,
                });
                let Ok(OutboundRecovery::SingleLocalCandidate(candidate)) = recovery else {
                    return ChannelSendResult::Uncertain;
                };
                let Some(guid) = candidate.guid else {
                    return ChannelSendResult::Uncertain;
                };
                adapter
                    .send_status(&guid)
                    .ok()
                    .and_then(|status| {
                        (status.ok
                            && status.guid == guid
                            && status.service.as_deref() == Some("iMessage")
                            && matches!(status.send_state, SendState::Sent | SendState::Delivered))
                        .then_some(ChannelSendResult::Accepted {
                            provider_message_id: guid,
                        })
                    })
                    .unwrap_or(ChannelSendResult::Uncertain)
            }
            #[cfg(test)]
            Self::Test { recoveries, .. } => {
                recoveries.fetch_add(1, Ordering::AcqRel);
                ChannelSendResult::Uncertain
            }
        }
    }
}

fn imessage_adapter_body(content: &str) -> Option<&str> {
    let body = content
        .strip_prefix(OPENOPEN_IMESSAGE_PREFIX)?
        .strip_prefix('\n')?;
    (!body.is_empty() && !body.starts_with(OPENOPEN_IMESSAGE_PREFIX)).then_some(body)
}

pub(crate) struct ChannelRuntime {
    runtime: Runtime,
    discord_setup: Option<DiscordSetupSession>,
    discord: Option<DiscordSession>,
    imessage_discovery: Option<Arc<ImsgAdapter>>,
    prepared_imessage: Option<PreparedImsgSession>,
    imessage: Option<ImsgSession>,
    #[cfg(test)]
    test_send_handle: Option<(ChannelKind, ChannelSendHandle)>,
}

impl ChannelRuntime {
    pub(crate) fn new() -> Result<Self, ChannelRuntimeError> {
        Ok(Self {
            runtime: Runtime::new()?,
            discord_setup: None,
            discord: None,
            imessage_discovery: None,
            prepared_imessage: None,
            imessage: None,
            #[cfg(test)]
            test_send_handle: None,
        })
    }

    pub(crate) fn start_discord(
        &mut self,
        pairing: &ChannelPairing,
        token: Zeroizing<String>,
        cursor: Option<&ChannelCursor>,
    ) -> Result<ChannelConnectionStatus, ChannelRuntimeError> {
        if pairing.channel != ChannelKind::Discord
            || !pairing.require_explicit_address
            || pairing
                .owner_sender_id
                .parse::<u64>()
                .ok()
                .as_ref()
                .is_none_or(|id| *id == 0)
            || pairing
                .conversation_id
                .parse::<u64>()
                .ok()
                .as_ref()
                .is_none_or(|id| *id == 0)
            || pairing.discord.is_none()
        {
            return Err(ChannelRuntimeError::PairingMismatch);
        }
        let discord = pairing
            .discord
            .as_ref()
            .ok_or(ChannelRuntimeError::PairingMismatch)?;
        let adapter_pairing = DiscordPairing::new_with_application(
            pairing
                .owner_sender_id
                .parse()
                .map_err(|_| ChannelRuntimeError::PairingMismatch)?,
            pairing
                .conversation_id
                .parse()
                .map_err(|_| ChannelRuntimeError::PairingMismatch)?,
            discord
                .bot_user_id
                .parse()
                .map_err(|_| ChannelRuntimeError::PairingMismatch)?,
            discord
                .application_id
                .parse()
                .map_err(|_| ChannelRuntimeError::PairingMismatch)?,
        )
        .map_err(|_| ChannelRuntimeError::PairingMismatch)?;
        BotToken::new(token.as_str()).map_err(|_| ChannelRuntimeError::Adapter)?;
        if let Some(status) = self.existing_discord_start_status(pairing)? {
            return Ok(status);
        }
        let (adapter, inbound) = DiscordAdapter::new(adapter_pairing, DISCORD_INBOUND_CAPACITY)
            .map_err(|_| ChannelRuntimeError::Adapter)?;
        let launch_pending = Arc::new(AtomicBool::new(true));
        let recovery_required = cursor.is_some();
        let recovery = if let Some(cursor) = cursor {
            let (sender, receiver) = mpsc::sync_channel(1);
            let recover_adapter = adapter.clone();
            let pending = Arc::clone(&launch_pending);
            let request = RecoveryRequest {
                conversation_id: pairing.conversation_id.clone(),
                after_message_id: cursor.opaque_value.clone(),
                max_pages: DISCORD_RECOVERY_PAGES,
            };
            self.runtime.spawn(async move {
                let start_result = async {
                    let token_ref = BotToken::new(token.as_str()).map_err(|_| ())?;
                    recover_adapter.start(token_ref).await.map_err(|_| ())
                }
                .await;
                pending.store(false, Ordering::Release);
                let result = match start_result {
                    Ok(()) => recover_adapter
                        .recover_after(&request)
                        .await
                        .map_err(|_| ()),
                    Err(()) => Err(()),
                };
                let _ = sender.send(result);
            });
            Some(receiver)
        } else {
            let start_adapter = adapter.clone();
            let pending = Arc::clone(&launch_pending);
            self.runtime.spawn(async move {
                if let Ok(token_ref) = BotToken::new(token.as_str()) {
                    let _ = start_adapter.start(token_ref).await;
                }
                pending.store(false, Ordering::Release);
            });
            None
        };
        self.discord = Some(DiscordSession {
            adapter,
            #[cfg(test)]
            status_override: None,
            inbound,
            recovery,
            recovered: VecDeque::new(),
            recovery_waiting: None,
            recovery_required,
            launch_pending,
            pairing: pairing.clone(),
            conversation_id: pairing.conversation_id.clone(),
        });
        Ok(ChannelConnectionStatus::Connecting)
    }

    fn existing_discord_start_status(
        &mut self,
        pairing: &ChannelPairing,
    ) -> Result<Option<ChannelConnectionStatus>, ChannelRuntimeError> {
        let Some(session) = self.discord.as_ref() else {
            return Ok(None);
        };
        if session.pairing != *pairing {
            return Err(ChannelRuntimeError::PairingMismatch);
        }
        let status = session.adapter.status();
        let launch_pending = session.launch_pending.load(Ordering::Acquire);
        if discord_session_should_restart(status, launch_pending) {
            let session = self.discord.take().expect("existing Discord session");
            self.runtime.block_on(session.adapter.stop());
            return Ok(None);
        }
        Ok(Some(discord_session_status(session)))
    }

    pub(crate) fn start_discord_setup(
        &mut self,
        token: &Zeroizing<String>,
    ) -> Result<DiscordSetupStart, ChannelRuntimeError> {
        if self.discord_setup.is_some() || self.discord.is_some() {
            return Err(ChannelRuntimeError::AlreadyRunning);
        }
        let mut random = [0_u8; 16];
        getrandom::fill(&mut random).map_err(|_| ChannelRuntimeError::Adapter)?;
        let pairing_code = hex::encode(random);
        let token = BotToken::new(token.as_str()).map_err(|_| ChannelRuntimeError::Adapter)?;
        let (adapter, start, candidates) = self
            .runtime
            .block_on(DiscordSetupAdapter::start(
                token,
                pairing_code,
                DISCORD_INBOUND_CAPACITY,
            ))
            .map_err(|_| ChannelRuntimeError::Adapter)?;
        self.discord_setup = Some(DiscordSetupSession {
            adapter,
            candidates,
            pending_candidate: None,
        });
        Ok(start)
    }

    pub(crate) fn poll_discord_setup(
        &mut self,
    ) -> Result<(ChannelConnectionStatus, Option<DiscordPairingCandidate>), ChannelRuntimeError>
    {
        let Some(session) = self.discord_setup.as_mut() else {
            return Err(ChannelRuntimeError::PairingMismatch);
        };
        let status = discord_status(session.adapter.status());
        match session.candidates.try_recv() {
            Ok(candidate) if candidate.confirmable() => {
                session.pending_candidate = Some(candidate.clone());
                Ok((status, Some(candidate)))
            }
            Ok(_) => Err(ChannelRuntimeError::PairingMismatch),
            Err(TokioTryRecvError::Empty) => Ok((status, session.pending_candidate.clone())),
            Err(TokioTryRecvError::Disconnected) => Err(ChannelRuntimeError::Adapter),
        }
    }

    pub(crate) fn confirm_discord_setup(
        &self,
        candidate_id: &str,
    ) -> Result<DiscordPairingCandidate, ChannelRuntimeError> {
        let candidate = self
            .discord_setup
            .as_ref()
            .and_then(|session| session.pending_candidate.as_ref())
            .filter(|candidate| candidate.candidate_id == candidate_id && candidate.confirmable())
            .ok_or(ChannelRuntimeError::PairingMismatch)?;
        Ok(candidate.clone())
    }

    pub(crate) fn stop_discord_setup(&mut self) {
        if let Some(session) = self.discord_setup.take() {
            self.runtime.block_on(session.adapter.stop());
        }
    }

    pub(crate) fn prepare_imessage(
        &mut self,
        executable: &Path,
        pairing: &ChannelPairing,
        cursor: Option<&ChannelCursor>,
    ) -> Result<u32, ChannelRuntimeError> {
        if self.imessage_discovery.is_some()
            || self.imessage.is_some()
            || self.prepared_imessage.is_some()
        {
            return Err(ChannelRuntimeError::AlreadyRunning);
        }
        if pairing.channel != ChannelKind::IMessage || pairing.require_explicit_address {
            return Err(ChannelRuntimeError::PairingMismatch);
        }
        let chat_id = ChatId::new(
            pairing
                .conversation_id
                .parse()
                .map_err(|_| ChannelRuntimeError::PairingMismatch)?,
        )
        .map_err(|_| ChannelRuntimeError::PairingMismatch)?;
        let inbound_pairing = InboundPairing::new(chat_id, pairing.owner_sender_id.clone())
            .map_err(|_| ChannelRuntimeError::PairingMismatch)?;
        let identity = pairing
            .imessage
            .clone()
            .ok_or(ChannelRuntimeError::PairingMismatch)?;
        if identity.participant_ids.as_slice() != [pairing.owner_sender_id.as_str()] {
            return Err(ChannelRuntimeError::PairingMismatch);
        }
        let adapter = Arc::new(
            ImsgAdapter::spawn(&ImsgConfig::new(executable))
                .map_err(|_| ChannelRuntimeError::Adapter)?,
        );
        let messages = adapter
            .history(HistoryRequest {
                chat_id,
                limit: SELF_CHAT_IDENTITY_HISTORY_LIMIT,
            })
            .map_err(|_| ChannelRuntimeError::Adapter)?;
        validate_self_chat_identity_evidence(
            &ImessageChat {
                chat_id: pairing.conversation_id.clone(),
                chat_guid: identity.chat_guid.clone(),
                chat_identifier: identity.chat_identifier.clone(),
                name: String::new(),
                service: identity.service.clone(),
                participants: identity.participant_ids.clone(),
            },
            &messages,
        )?;
        let since_rowid = cursor
            .map(|value| value.order)
            .map(i64::try_from)
            .transpose()
            .map_err(|_| ChannelRuntimeError::PairingMismatch)?
            .map(MessageCursor::new)
            .transpose()
            .map_err(|_| ChannelRuntimeError::PairingMismatch)?;
        let process_identifier = adapter
            .process_identifier()
            .map_err(|_| ChannelRuntimeError::Adapter)?;
        self.prepared_imessage = Some(PreparedImsgSession {
            adapter,
            pairing: inbound_pairing,
            since_rowid,
            conversation_id: pairing.conversation_id.clone(),
            identity,
        });
        Ok(process_identifier)
    }

    pub(crate) fn prepare_imessage_discovery(
        &mut self,
        executable: &Path,
    ) -> Result<u32, ChannelRuntimeError> {
        if self.imessage_discovery.is_some()
            || self.prepared_imessage.is_some()
            || self.imessage.is_some()
        {
            return Err(ChannelRuntimeError::AlreadyRunning);
        }
        let adapter = Arc::new(
            ImsgAdapter::spawn(&ImsgConfig::new(executable))
                .map_err(|_| ChannelRuntimeError::Adapter)?,
        );
        let process_identifier = adapter
            .process_identifier()
            .map_err(|_| ChannelRuntimeError::Adapter)?;
        self.imessage_discovery = Some(adapter);
        Ok(process_identifier)
    }

    pub(crate) fn list_imessage_chats(&mut self) -> Result<Vec<ImessageChat>, ChannelRuntimeError> {
        let adapter = self
            .imessage_discovery
            .take()
            .ok_or(ChannelRuntimeError::PairingMismatch)?;
        let chats = adapter
            .list_chats(ListChatsRequest {
                limit: openopen_imsg_adapter::MAX_CHAT_LIST_ITEMS,
                unread_only: false,
            })
            .map_err(|_| ChannelRuntimeError::Adapter)
            .and_then(validate_imessage_chats)
            .and_then(|chats| {
                let mut verified = Vec::new();
                for chat in chats {
                    let chat_id = chat
                        .chat_id
                        .parse::<i64>()
                        .map_err(|_| ChannelRuntimeError::PairingMismatch)
                        .and_then(|value| {
                            ChatId::new(value).map_err(|_| ChannelRuntimeError::PairingMismatch)
                        })?;
                    let messages = adapter
                        .history(HistoryRequest {
                            chat_id,
                            limit: SELF_CHAT_IDENTITY_HISTORY_LIMIT,
                        })
                        .map_err(|_| ChannelRuntimeError::Adapter)?;
                    if validate_self_chat_identity_evidence(&chat, &messages).is_ok() {
                        verified.push(chat);
                    }
                }
                Ok(verified)
            });
        let shutdown = adapter.shutdown().map_err(|_| ChannelRuntimeError::Adapter);
        shutdown.and(chats)
    }

    pub(crate) fn activate_imessage(
        &mut self,
    ) -> Result<ChannelConnectionStatus, ChannelRuntimeError> {
        let prepared = self
            .prepared_imessage
            .take()
            .ok_or(ChannelRuntimeError::PairingMismatch)?;
        let Ok(subscription) = prepared.adapter.subscribe(WatchRequest {
            chat_id: prepared.pairing.chat_id,
            since_rowid: prepared.since_rowid,
        }) else {
            let _ = prepared.adapter.shutdown();
            return Err(ChannelRuntimeError::Adapter);
        };
        self.imessage = Some(ImsgSession {
            adapter: prepared.adapter,
            pairing: prepared.pairing,
            subscription,
            conversation_id: prepared.conversation_id,
            identity: prepared.identity,
        });
        Ok(ChannelConnectionStatus::Connected)
    }

    pub(crate) fn status(&self, channel: ChannelKind) -> ChannelConnectionStatus {
        match channel {
            ChannelKind::Discord => self.discord.as_ref().map_or(
                ChannelConnectionStatus::Disconnected,
                discord_session_status,
            ),
            ChannelKind::IMessage => self.imessage.as_ref().map_or_else(
                || {
                    if self.prepared_imessage.is_some() || self.imessage_discovery.is_some() {
                        ChannelConnectionStatus::Connecting
                    } else {
                        ChannelConnectionStatus::Disconnected
                    }
                },
                |_| ChannelConnectionStatus::Connected,
            ),
        }
    }

    pub(crate) fn poll(
        &mut self,
        channel: ChannelKind,
        observed_at_ms: i64,
    ) -> Result<Option<TransportEvent>, ChannelRuntimeError> {
        match channel {
            ChannelKind::Discord => self.poll_discord(observed_at_ms),
            ChannelKind::IMessage => self.poll_imessage(observed_at_ms),
        }
    }

    fn poll_discord(
        &mut self,
        observed_at_ms: i64,
    ) -> Result<Option<TransportEvent>, ChannelRuntimeError> {
        let result = match self.discord.as_mut() {
            Some(session) => poll_discord_session(session, observed_at_ms),
            None => Ok(None),
        };
        if result.is_err()
            && let Some(session) = self.discord.take()
        {
            self.runtime.block_on(session.adapter.stop());
        }
        result
    }

    pub(crate) fn acknowledge_recovery(
        &mut self,
        event: &TransportEvent,
    ) -> Result<(), ChannelRuntimeError> {
        if transport_event_channel(event) != ChannelKind::Discord {
            return Ok(());
        }
        let session = self.discord.as_mut().ok_or(ChannelRuntimeError::Recovery)?;
        if !session.recovery_required {
            return Ok(());
        }
        if session.recovery_waiting.as_ref() != Some(event) {
            return Err(ChannelRuntimeError::Recovery);
        }
        session.recovery_waiting = None;
        if session.recovery.is_none() && session.recovered.is_empty() {
            session.recovery_required = false;
        }
        Ok(())
    }

    fn poll_imessage(
        &mut self,
        observed_at_ms: i64,
    ) -> Result<Option<TransportEvent>, ChannelRuntimeError> {
        let Some(session) = self.imessage.as_ref() else {
            return Ok(None);
        };
        let Some(event) = session
            .adapter
            .recv_event_timeout(Duration::ZERO)
            .map_err(|_| ChannelRuntimeError::Adapter)?
        else {
            return Ok(None);
        };
        match event {
            ImsgEvent::Message {
                subscription,
                message,
            } if subscription == session.subscription => {
                if message.chat_guid != session.identity.chat_guid
                    || message.chat_identifier != session.identity.chat_identifier
                    || message.participants != session.identity.participant_ids
                    || message.destination_caller_id.as_deref()
                        != Some(session.pairing.owner_sender.as_str())
                    || message.is_group
                {
                    return Err(ChannelRuntimeError::PairingMismatch);
                }
                match classify_self_chat_message(&session.pairing, &message) {
                    Ok(SelfChatMessage::UserAuthored(value)) => {
                        Ok(Some(TransportEvent::Inbound(TransportInbound {
                            channel: ChannelKind::IMessage,
                            source_message_id: value.source_guid,
                            sender_id: value.sender,
                            conversation_id: session.conversation_id.clone(),
                            content: value.body,
                            cursor_opaque_value: value.source_rowid.to_string(),
                            cursor_order: u64::try_from(value.source_rowid)
                                .map_err(|_| ChannelRuntimeError::Adapter)?,
                            received_at_ms: observed_at_ms,
                        })))
                    }
                    Ok(SelfChatMessage::OpenOpenEcho(value)) => {
                        Ok(Some(TransportEvent::IMessageEcho {
                            provider_message_id: value.source_guid,
                            cursor: ChannelCursor {
                                channel: ChannelKind::IMessage,
                                conversation_id: session.conversation_id.clone(),
                                opaque_value: value.source_rowid.to_string(),
                                order: u64::try_from(value.source_rowid)
                                    .map_err(|_| ChannelRuntimeError::Adapter)?,
                                observed_at_ms,
                            },
                        }))
                    }
                    Err(InboundRejection::EmptyBody | InboundRejection::BodyTooLarge)
                        if message.id > 0 =>
                    {
                        Ok(Some(TransportEvent::Cursor(ChannelCursor {
                            channel: ChannelKind::IMessage,
                            conversation_id: session.conversation_id.clone(),
                            opaque_value: message.id.to_string(),
                            order: u64::try_from(message.id)
                                .map_err(|_| ChannelRuntimeError::Adapter)?,
                            observed_at_ms,
                        })))
                    }
                    Err(
                        InboundRejection::InvalidPairing
                        | InboundRejection::ChatNotPaired
                        | InboundRejection::GroupChat
                        | InboundRejection::NotFromLocalUser
                        | InboundRejection::AmbiguousProductPrefix
                        | InboundRejection::InvalidSourceIdentity
                        | InboundRejection::SenderNotOwner
                        | InboundRejection::FromLocalUser
                        | InboundRejection::NotAddressed
                        | InboundRejection::EmptyBody
                        | InboundRejection::BodyTooLarge,
                    ) => Err(ChannelRuntimeError::Adapter),
                }
            }
            ImsgEvent::Message { .. } | ImsgEvent::SubscriptionError { .. } => {
                Err(ChannelRuntimeError::Adapter)
            }
        }
    }

    pub(crate) fn stop(&mut self, channel: ChannelKind) {
        match channel {
            ChannelKind::Discord => {
                self.stop_discord_setup();
                if let Some(session) = self.discord.take() {
                    self.runtime.block_on(session.adapter.stop());
                }
            }
            ChannelKind::IMessage => {
                if let Some(adapter) = self.imessage_discovery.take() {
                    let _ = adapter.shutdown();
                }
                if let Some(session) = self.prepared_imessage.take() {
                    let _ = session.adapter.shutdown();
                }
                if let Some(session) = self.imessage.take() {
                    let _ = session.adapter.shutdown();
                }
            }
        }
    }

    pub(crate) fn send_handle(&self, channel: ChannelKind) -> Option<ChannelSendHandle> {
        #[cfg(test)]
        if let Some((expected_channel, handle)) = &self.test_send_handle
            && *expected_channel == channel
        {
            return Some(handle.clone());
        }
        match channel {
            ChannelKind::Discord => self
                .discord
                .as_ref()
                .filter(|session| discord_session_ready_for_send(session))
                .map(|session| ChannelSendHandle::Discord {
                    adapter: session.adapter.clone(),
                    runtime: self.runtime.handle().clone(),
                    conversation_id: session.conversation_id.clone(),
                }),
            ChannelKind::IMessage => {
                self.imessage
                    .as_ref()
                    .map(|session| ChannelSendHandle::IMessage {
                        adapter: Arc::clone(&session.adapter),
                        chat_id: session.pairing.chat_id,
                    })
            }
        }
    }

    pub(crate) fn model_work_ready(&self, channel: ChannelKind) -> bool {
        match channel {
            ChannelKind::Discord => self
                .discord
                .as_ref()
                .is_some_and(discord_session_ready_for_model_work),
            ChannelKind::IMessage => true,
        }
    }

    #[cfg(test)]
    pub(crate) fn install_test_discord_recovery(
        &mut self,
    ) -> std::sync::mpsc::SyncSender<Result<RecoveryBatch, ()>> {
        let pairing = ChannelPairing {
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
        };
        let adapter_pairing = DiscordPairing::new_with_application(1001, 2002, 4004, 5005)
            .expect("valid test pairing");
        let (adapter, inbound) =
            DiscordAdapter::new(adapter_pairing, DISCORD_INBOUND_CAPACITY).unwrap();
        let (sender, recovery) = mpsc::sync_channel(1);
        self.discord = Some(DiscordSession {
            adapter,
            status_override: None,
            inbound,
            recovery: Some(recovery),
            recovered: VecDeque::new(),
            recovery_waiting: None,
            recovery_required: true,
            launch_pending: Arc::new(AtomicBool::new(true)),
            pairing: pairing.clone(),
            conversation_id: pairing.conversation_id,
        });
        sender
    }

    #[cfg(test)]
    pub(crate) fn mark_test_discord_connected(&mut self) {
        self.discord
            .as_mut()
            .expect("test Discord session")
            .status_override = Some(DiscordConnectionStatus::Connected);
    }

    #[cfg(test)]
    pub(crate) fn install_test_send_probe(
        &mut self,
        channel: ChannelKind,
    ) -> (Arc<AtomicUsize>, Arc<AtomicUsize>) {
        let sends = Arc::new(AtomicUsize::new(0));
        let recoveries = Arc::new(AtomicUsize::new(0));
        self.test_send_handle = Some((
            channel,
            ChannelSendHandle::Test {
                sends: Arc::clone(&sends),
                recoveries: Arc::clone(&recoveries),
            },
        ));
        (sends, recoveries)
    }

    pub(crate) fn stop_all(&mut self) {
        self.stop(ChannelKind::Discord);
        self.stop(ChannelKind::IMessage);
    }
}

fn validate_self_chat_identity_evidence(
    chat: &ImessageChat,
    messages: &[Message],
) -> Result<(), ChannelRuntimeError> {
    let [owner_identity] = chat.participants.as_slice() else {
        return Err(ChannelRuntimeError::PairingMismatch);
    };
    let exact_local_identity = messages.iter().any(|message| {
        message.is_from_me
            && !message.is_group
            && message.chat_guid == chat.chat_guid
            && message.chat_identifier == chat.chat_identifier
            && message.participants == chat.participants
            && message.destination_caller_id.as_deref() == Some(owner_identity.as_str())
    });
    exact_local_identity
        .then_some(())
        .ok_or(ChannelRuntimeError::PairingMismatch)
}

fn validate_imessage_chats(chats: Vec<Chat>) -> Result<Vec<ImessageChat>, ChannelRuntimeError> {
    let mut result = Vec::with_capacity(chats.len());
    for chat in chats {
        if chat.service != "iMessage" {
            continue;
        }
        if chat.participants.len() > MAX_IMESSAGE_CHAT_PARTICIPANTS {
            return Err(ChannelRuntimeError::Adapter);
        }
        if chat.is_group || chat.participants.len() != 1 {
            continue;
        }
        if !valid_imessage_chat_field(&chat.name, true)
            || chat
                .participants
                .iter()
                .any(|participant| !valid_imessage_chat_field(participant, false))
        {
            return Err(ChannelRuntimeError::Adapter);
        }
        let mut participants = chat.participants;
        participants.sort();
        if participants.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(ChannelRuntimeError::Adapter);
        }
        result.push(ImessageChat {
            chat_id: chat.id.get().to_string(),
            chat_guid: chat.guid,
            chat_identifier: chat.identifier,
            name: chat.name,
            service: chat.service,
            participants,
        });
    }
    result.sort_by_key(|chat| chat.chat_id.parse::<i64>().unwrap_or_default());
    if result
        .windows(2)
        .any(|pair| pair[0].chat_id == pair[1].chat_id)
    {
        return Err(ChannelRuntimeError::Adapter);
    }
    Ok(result)
}

fn valid_imessage_chat_field(value: &str, allow_empty: bool) -> bool {
    value.len() <= MAX_IMESSAGE_CHAT_FIELD_BYTES
        && !value.as_bytes().contains(&0)
        && value.trim() == value
        && (allow_empty || !value.is_empty())
}

fn poll_discord_session(
    session: &mut DiscordSession,
    observed_at_ms: i64,
) -> Result<Option<TransportEvent>, ChannelRuntimeError> {
    if let Some(event) = session.recovery_waiting.as_ref() {
        return Ok(Some(event.clone()));
    }
    if let Some(event) = session.recovered.pop_front() {
        session.recovery_waiting = Some(event.clone());
        return Ok(Some(event));
    }
    if let Some(recovery) = session.recovery.as_ref() {
        match recovery.try_recv() {
            Ok(Ok(batch)) => {
                let mut recovered = VecDeque::with_capacity(batch.envelopes.len() + 1);
                for envelope in batch.envelopes {
                    recovered.push_back(TransportEvent::Inbound(discord_inbound(envelope)?));
                }
                let order = batch
                    .high_water_message_id
                    .parse()
                    .map_err(|_| ChannelRuntimeError::Recovery)?;
                recovered.push_back(TransportEvent::Cursor(ChannelCursor {
                    channel: ChannelKind::Discord,
                    conversation_id: session.conversation_id.clone(),
                    opaque_value: batch.high_water_message_id,
                    order,
                    observed_at_ms,
                }));
                session.recovery = None;
                session.recovered = recovered;
                let event = session
                    .recovered
                    .pop_front()
                    .ok_or(ChannelRuntimeError::Recovery)?;
                session.recovery_waiting = Some(event.clone());
                return Ok(Some(event));
            }
            Ok(Err(())) | Err(TryRecvError::Disconnected) => {
                return Err(ChannelRuntimeError::Recovery);
            }
            Err(TryRecvError::Empty) => return Ok(None),
        }
    }
    if session.recovery_required {
        return Err(ChannelRuntimeError::Recovery);
    }
    match session.inbound.try_recv() {
        Ok(envelope) => Ok(Some(TransportEvent::Inbound(discord_inbound(envelope)?))),
        Err(TokioTryRecvError::Empty) => Ok(None),
        Err(TokioTryRecvError::Disconnected) => Err(ChannelRuntimeError::Adapter),
    }
}

fn transport_event_channel(event: &TransportEvent) -> ChannelKind {
    match event {
        TransportEvent::Inbound(inbound) => inbound.channel,
        TransportEvent::Cursor(cursor) | TransportEvent::IMessageEcho { cursor, .. } => {
            cursor.channel
        }
    }
}

fn discord_session_status(session: &DiscordSession) -> ChannelConnectionStatus {
    let status = discord_session_adapter_status(session);
    let launch_pending = session.launch_pending.load(Ordering::Acquire);
    match status {
        DiscordConnectionStatus::Faulted => ChannelConnectionStatus::Faulted,
        DiscordConnectionStatus::Disconnected if !launch_pending => {
            ChannelConnectionStatus::Disconnected
        }
        DiscordConnectionStatus::Stopping => ChannelConnectionStatus::Disconnected,
        _ if session.recovery_required => ChannelConnectionStatus::Connecting,
        DiscordConnectionStatus::Disconnected => ChannelConnectionStatus::Connecting,
        _ => discord_status(status),
    }
}

fn discord_session_ready_for_send(session: &DiscordSession) -> bool {
    discord_status_allows_work(
        discord_session_adapter_status(session),
        session.recovery_required,
    )
}

fn discord_session_ready_for_model_work(session: &DiscordSession) -> bool {
    discord_status_allows_work(
        discord_session_adapter_status(session),
        session.recovery_required,
    )
}

fn discord_session_adapter_status(session: &DiscordSession) -> DiscordConnectionStatus {
    #[cfg(test)]
    if let Some(status) = session.status_override {
        return status;
    }
    session.adapter.status()
}

fn discord_status_allows_work(status: DiscordConnectionStatus, recovery_required: bool) -> bool {
    !recovery_required && status == DiscordConnectionStatus::Connected
}

fn discord_status(status: DiscordConnectionStatus) -> ChannelConnectionStatus {
    match status {
        DiscordConnectionStatus::Disconnected | DiscordConnectionStatus::Stopping => {
            ChannelConnectionStatus::Disconnected
        }
        DiscordConnectionStatus::Connecting => ChannelConnectionStatus::Connecting,
        DiscordConnectionStatus::Connected => ChannelConnectionStatus::Connected,
        DiscordConnectionStatus::Reconnecting => ChannelConnectionStatus::Reconnecting,
        DiscordConnectionStatus::Faulted => ChannelConnectionStatus::Faulted,
    }
}

fn discord_session_should_restart(status: DiscordConnectionStatus, launch_pending: bool) -> bool {
    match status {
        DiscordConnectionStatus::Faulted => true,
        DiscordConnectionStatus::Disconnected => !launch_pending,
        DiscordConnectionStatus::Stopping
        | DiscordConnectionStatus::Connecting
        | DiscordConnectionStatus::Connected
        | DiscordConnectionStatus::Reconnecting => false,
    }
}

fn discord_inbound(envelope: DiscordInbound) -> Result<TransportInbound, ChannelRuntimeError> {
    let order = envelope
        .source_message_id
        .parse()
        .map_err(|_| ChannelRuntimeError::Adapter)?;
    Ok(TransportInbound {
        channel: ChannelKind::Discord,
        source_message_id: envelope.source_message_id.clone(),
        sender_id: envelope.sender_id,
        conversation_id: envelope.conversation_id,
        content: envelope.content,
        cursor_opaque_value: envelope.source_message_id,
        cursor_order: order,
        received_at_ms: envelope.received_at_ms,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use openopen_protocol::DiscordPairingMetadata;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::thread;

    fn imessage_chat(id: i64, name: &str, participants: &[&str]) -> Chat {
        Chat {
            id: ChatId::new(id).unwrap(),
            identifier: format!("chat-{id}"),
            guid: format!("iMessage;+;chat-{id}"),
            name: name.into(),
            service: "iMessage".into(),
            last_message_at: "2026-07-16T00:00:00Z".into(),
            participants: participants.iter().map(|value| (*value).into()).collect(),
            is_group: participants.len() > 1,
            contact_name: None,
            unread_count: None,
        }
    }

    fn discord_channel_pairing() -> ChannelPairing {
        ChannelPairing {
            channel: ChannelKind::Discord,
            owner_sender_id: "1001".into(),
            conversation_id: "2002".into(),
            require_explicit_address: true,
            imessage: None,
            discord: Some(DiscordPairingMetadata {
                guild_id: "3003".into(),
                bot_user_id: "4004".into(),
                application_id: "5005".into(),
                setup_source_message_id: "6006".into(),
                setup_candidate_id: format!("discord-pair-{}", "a".repeat(64)),
            }),
            paired_at_ms: 1,
        }
    }

    fn discord_runtime_with_recovery() -> (
        ChannelRuntime,
        std::sync::mpsc::SyncSender<Result<RecoveryBatch, ()>>,
    ) {
        let pairing = discord_channel_pairing();
        let adapter_pairing = DiscordPairing::new_with_application(1001, 2002, 4004, 5005)
            .expect("valid test pairing");
        let (adapter, inbound) =
            DiscordAdapter::new(adapter_pairing, DISCORD_INBOUND_CAPACITY).unwrap();
        let (sender, recovery) = mpsc::sync_channel(1);
        let mut runtime = ChannelRuntime::new().unwrap();
        runtime.discord = Some(DiscordSession {
            adapter,
            status_override: None,
            inbound,
            recovery: Some(recovery),
            recovered: VecDeque::new(),
            recovery_waiting: None,
            recovery_required: true,
            launch_pending: Arc::new(AtomicBool::new(true)),
            pairing: pairing.clone(),
            conversation_id: pairing.conversation_id,
        });
        (runtime, sender)
    }

    #[test]
    fn discord_start_response_loss_retry_reattaches_to_exact_session() {
        let pairing = discord_channel_pairing();
        let adapter_pairing = DiscordPairing::new_with_application(1001, 2002, 4004, 5005)
            .expect("valid test pairing");
        let (adapter, inbound) =
            DiscordAdapter::new(adapter_pairing, DISCORD_INBOUND_CAPACITY).unwrap();
        let mut runtime = ChannelRuntime::new().unwrap();
        let launch_pending = Arc::new(AtomicBool::new(true));
        runtime.discord = Some(DiscordSession {
            adapter,
            status_override: None,
            inbound,
            recovery: None,
            recovered: VecDeque::new(),
            recovery_waiting: None,
            recovery_required: false,
            launch_pending: Arc::clone(&launch_pending),
            pairing: pairing.clone(),
            conversation_id: pairing.conversation_id.clone(),
        });

        assert_eq!(
            runtime
                .start_discord(&pairing, Zeroizing::new("token".into()), None)
                .unwrap(),
            ChannelConnectionStatus::Connecting
        );
        assert!(
            Arc::ptr_eq(
                &runtime.discord.as_ref().unwrap().launch_pending,
                &launch_pending
            ),
            "retry must retain the one already-created provider session"
        );

        let mut changed_pairing = pairing;
        changed_pairing.owner_sender_id = "1002".into();
        assert!(matches!(
            runtime.start_discord(&changed_pairing, Zeroizing::new("token".into()), None),
            Err(ChannelRuntimeError::PairingMismatch)
        ));
    }

    #[test]
    fn discord_terminal_session_retry_replaces_only_the_exact_pairing() {
        let pairing = discord_channel_pairing();
        let adapter_pairing = DiscordPairing::new_with_application(1001, 2002, 4004, 5005)
            .expect("valid test pairing");
        let (adapter, inbound) =
            DiscordAdapter::new(adapter_pairing, DISCORD_INBOUND_CAPACITY).unwrap();
        let previous_launch = Arc::new(AtomicBool::new(false));
        let mut runtime = ChannelRuntime::new().unwrap();
        runtime.discord = Some(DiscordSession {
            adapter,
            status_override: None,
            inbound,
            recovery: None,
            recovered: VecDeque::new(),
            recovery_waiting: None,
            recovery_required: false,
            launch_pending: Arc::clone(&previous_launch),
            pairing: pairing.clone(),
            conversation_id: pairing.conversation_id.clone(),
        });

        assert!(discord_session_should_restart(
            DiscordConnectionStatus::Faulted,
            false
        ));
        assert!(discord_session_should_restart(
            DiscordConnectionStatus::Disconnected,
            false
        ));
        assert!(!discord_session_should_restart(
            DiscordConnectionStatus::Disconnected,
            true
        ));
        assert_eq!(
            runtime
                .start_discord(&pairing, Zeroizing::new("token".into()), None)
                .unwrap(),
            ChannelConnectionStatus::Connecting
        );
        assert!(
            !Arc::ptr_eq(
                &runtime.discord.as_ref().unwrap().launch_pending,
                &previous_launch
            ),
            "terminal retry must replace the stopped provider session exactly once"
        );
    }

    #[test]
    fn discord_recovery_repeats_each_event_until_durable_ack_and_blocks_send() {
        let (mut runtime, recovery) = discord_runtime_with_recovery();
        assert_eq!(
            runtime.status(ChannelKind::Discord),
            ChannelConnectionStatus::Connecting
        );
        assert!(runtime.send_handle(ChannelKind::Discord).is_none());
        assert!(!runtime.model_work_ready(ChannelKind::Discord));
        assert!(!discord_status_allows_work(
            DiscordConnectionStatus::Connected,
            true
        ));

        recovery
            .send(Ok(RecoveryBatch {
                envelopes: vec![
                    DiscordInbound {
                        source_message_id: "9001".into(),
                        sender_id: "1001".into(),
                        conversation_id: "2002".into(),
                        content: "first".into(),
                        received_at_ms: 10,
                    },
                    DiscordInbound {
                        source_message_id: "9002".into(),
                        sender_id: "1001".into(),
                        conversation_id: "2002".into(),
                        content: "second".into(),
                        received_at_ms: 11,
                    },
                ],
                high_water_message_id: "9003".into(),
                pages_fetched: 1,
            }))
            .unwrap();

        let first = runtime.poll(ChannelKind::Discord, 42).unwrap().unwrap();
        assert_eq!(
            runtime.poll(ChannelKind::Discord, 42).unwrap(),
            Some(first.clone())
        );
        runtime.acknowledge_recovery(&first).unwrap();
        assert!(!runtime.model_work_ready(ChannelKind::Discord));

        let second = runtime.poll(ChannelKind::Discord, 42).unwrap().unwrap();
        assert_ne!(second, first);
        assert_eq!(
            runtime.poll(ChannelKind::Discord, 42).unwrap(),
            Some(second.clone())
        );
        runtime.acknowledge_recovery(&second).unwrap();
        assert!(!runtime.model_work_ready(ChannelKind::Discord));

        let cursor = runtime.poll(ChannelKind::Discord, 42).unwrap().unwrap();
        assert!(matches!(
            &cursor,
            TransportEvent::Cursor(value)
                if value.opaque_value == "9003" && value.order == 9003
        ));
        assert_eq!(
            runtime.poll(ChannelKind::Discord, 42).unwrap(),
            Some(cursor.clone())
        );
        assert!(runtime.send_handle(ChannelKind::Discord).is_none());
        runtime.acknowledge_recovery(&cursor).unwrap();

        assert!(!runtime.discord.as_ref().unwrap().recovery_required);
        assert!(!runtime.model_work_ready(ChannelKind::Discord));
        runtime.mark_test_discord_connected();
        assert!(runtime.model_work_ready(ChannelKind::Discord));
        assert!(runtime.send_handle(ChannelKind::Discord).is_some());
        assert!(discord_status_allows_work(
            DiscordConnectionStatus::Connected,
            false
        ));
    }

    #[test]
    fn discord_model_and_send_work_require_exact_connected_after_recovery() {
        for status in [
            DiscordConnectionStatus::Disconnected,
            DiscordConnectionStatus::Connecting,
            DiscordConnectionStatus::Reconnecting,
            DiscordConnectionStatus::Stopping,
            DiscordConnectionStatus::Faulted,
        ] {
            assert!(!discord_status_allows_work(status, false));
        }
        assert!(!discord_status_allows_work(
            DiscordConnectionStatus::Connected,
            true
        ));
        assert!(discord_status_allows_work(
            DiscordConnectionStatus::Connected,
            false
        ));
    }

    #[test]
    fn discord_recovery_failure_stops_the_exact_session_for_clean_retry() {
        let (mut runtime, recovery) = discord_runtime_with_recovery();
        recovery.send(Err(())).unwrap();

        assert!(matches!(
            runtime.poll(ChannelKind::Discord, 42),
            Err(ChannelRuntimeError::Recovery)
        ));
        assert!(runtime.discord.is_none());
        assert_eq!(
            runtime.status(ChannelKind::Discord),
            ChannelConnectionStatus::Disconnected
        );
        assert!(runtime.send_handle(ChannelKind::Discord).is_none());
    }

    #[test]
    fn discord_cursor_store_failure_retains_exact_recovery_event() {
        let (mut runtime, recovery) = discord_runtime_with_recovery();
        recovery
            .send(Ok(RecoveryBatch {
                envelopes: Vec::new(),
                high_water_message_id: "9003".into(),
                pages_fetched: 1,
            }))
            .unwrap();

        let cursor = runtime.poll(ChannelKind::Discord, 42).unwrap().unwrap();
        assert_eq!(
            runtime.poll(ChannelKind::Discord, 99).unwrap(),
            Some(cursor.clone())
        );
        let mut wrong = cursor.clone();
        let TransportEvent::Cursor(wrong_cursor) = &mut wrong else {
            unreachable!();
        };
        wrong_cursor.opaque_value = "9004".into();
        wrong_cursor.order = 9004;
        assert!(matches!(
            runtime.acknowledge_recovery(&wrong),
            Err(ChannelRuntimeError::Recovery)
        ));
        assert_eq!(
            runtime.poll(ChannelKind::Discord, 100).unwrap(),
            Some(cursor.clone())
        );
        runtime.acknowledge_recovery(&cursor).unwrap();
        assert!(!runtime.discord.as_ref().unwrap().recovery_required);
    }

    #[test]
    fn malformed_recovery_batch_is_atomic_and_stops_the_session() {
        let (mut runtime, recovery) = discord_runtime_with_recovery();
        recovery
            .send(Ok(RecoveryBatch {
                envelopes: vec![
                    DiscordInbound {
                        source_message_id: "9001".into(),
                        sender_id: "1001".into(),
                        conversation_id: "2002".into(),
                        content: "valid".into(),
                        received_at_ms: 10,
                    },
                    DiscordInbound {
                        source_message_id: "not-a-snowflake".into(),
                        sender_id: "1001".into(),
                        conversation_id: "2002".into(),
                        content: "invalid".into(),
                        received_at_ms: 11,
                    },
                ],
                high_water_message_id: "9003".into(),
                pages_fetched: 1,
            }))
            .unwrap();

        assert!(matches!(
            runtime.poll(ChannelKind::Discord, 42),
            Err(ChannelRuntimeError::Adapter)
        ));
        assert!(runtime.discord.is_none());
    }

    #[test]
    fn global_off_stops_pending_discord_recovery() {
        let (mut runtime, _recovery) = discord_runtime_with_recovery();
        runtime.stop_all();

        assert!(runtime.discord.is_none());
        assert_eq!(
            runtime.status(ChannelKind::Discord),
            ChannelConnectionStatus::Disconnected
        );
    }

    struct FakeImsg {
        _root: tempfile::TempDir,
        executable: std::path::PathBuf,
        log: std::path::PathBuf,
    }

    impl FakeImsg {
        fn new(invalid_list: bool) -> Self {
            let root = tempfile::tempdir().unwrap();
            let canonical_root = root.path().canonicalize().unwrap();
            let executable = canonical_root.join("imsg-fake");
            let log = canonical_root.join("requests.log");
            let log_json = serde_json::to_string(&log.display().to_string()).unwrap();
            let participants = if invalid_list {
                "['']"
            } else {
                "['+15550000002','+15550000001']"
            };
            let script = format!(
                r"#!/usr/bin/env python3
import json
import sys

LOG = {log_json}

def emit(value):
    sys.stdout.write(json.dumps(value, separators=(',', ':')) + '\n')
    sys.stdout.flush()

if len(sys.argv) != 2 or sys.argv[1] != 'rpc':
    sys.exit(7)

open(LOG, 'w').close()
for line in sys.stdin:
    with open(LOG, 'a') as output:
        output.write(line)
    request = json.loads(line)
    request_id = request['id']
    method = request['method']
    if method == 'chats.list':
        emit({{'jsonrpc':'2.0','id':request_id,'result':{{'chats':[
            {{'id':43,'identifier':'second','guid':'iMessage;+;second','name':'Second','service':'iMessage','last_message_at':'2026-07-15T00:00:00Z','participants':['+15550000003'],'is_group':False}},
            {{'id':42,'identifier':'first','guid':'iMessage;+;first','name':'First','service':'iMessage','last_message_at':'2026-07-14T00:00:00Z','participants':{participants},'is_group':False}}
        ]}}}})
    elif method == 'messages.history':
        chat_id = request['params']['chat_id']
        identifier = 'second' if chat_id == 43 else 'first'
        owner = '+15550000003' if chat_id == 43 else '+15550000001'
        emit({{'jsonrpc':'2.0','id':request_id,'result':{{'messages':[{{'id':101,'chat_id':chat_id,'chat_identifier':identifier,'chat_guid':'iMessage;+;'+identifier,'chat_name':identifier.title(),'participants':[owner],'is_group':False,'guid':'same-text-guid','sender':'','is_from_me':True,'text':'OpenOpen · AI\nWorking on it','created_at':'2026-07-15T00:02:00Z','destination_caller_id':owner}}]}}}})
"
            );
            fs::write(&executable, script).unwrap();
            let mut permissions = fs::metadata(&executable).unwrap().permissions();
            permissions.set_mode(0o700);
            fs::set_permissions(&executable, permissions).unwrap();
            Self {
                _root: root,
                executable,
                log,
            }
        }

        fn wait_for_log(&self) {
            for _ in 0..100 {
                if self.log.exists() {
                    return;
                }
                thread::sleep(Duration::from_millis(10));
            }
            panic!("fake imsg did not start");
        }
    }

    #[test]
    fn imessage_transport_strips_exactly_one_authorized_prefix() {
        assert_eq!(
            imessage_adapter_body("OpenOpen · AI\nWorking on it"),
            Some("Working on it")
        );
        assert_eq!(imessage_adapter_body("Working on it"), None);
        assert_eq!(imessage_adapter_body("OpenOpen · AI\n"), None);
        assert_eq!(
            imessage_adapter_body("OpenOpen · AI\nOpenOpen · AI\nWorking on it"),
            None
        );
    }

    #[test]
    fn imessage_history_recovery_never_becomes_sent() {
        let fake = FakeImsg::new(false);
        let adapter = Arc::new(ImsgAdapter::spawn(&ImsgConfig::new(&fake.executable)).unwrap());
        let handle = ChannelSendHandle::IMessage {
            adapter: Arc::clone(&adapter),
            chat_id: ChatId::new(42).unwrap(),
        };
        assert_eq!(
            handle.recover(
                "outbound-1",
                "OpenOpen · AI\nWorking on it",
                &ChannelCursor {
                    channel: ChannelKind::IMessage,
                    conversation_id: "42".into(),
                    opaque_value: "100".into(),
                    order: 100,
                    observed_at_ms: 1,
                },
            ),
            ChannelSendResult::Uncertain
        );
        let requests = fs::read_to_string(&fake.log).unwrap();
        assert!(requests.contains("messages.history"));
        assert!(!requests.contains(r#"\"method\":\"send\""#));
        adapter.shutdown().unwrap();
    }

    #[test]
    fn imessage_discovery_is_two_phase_bounded_and_always_cleared() {
        let fake = FakeImsg::new(false);
        let mut runtime = ChannelRuntime::new().unwrap();
        assert!(
            runtime
                .prepare_imessage_discovery(&fake.executable)
                .unwrap()
                > 0
        );
        fake.wait_for_log();
        assert_eq!(fs::read_to_string(&fake.log).unwrap(), "");
        assert_eq!(
            runtime.status(ChannelKind::IMessage),
            ChannelConnectionStatus::Connecting
        );
        assert_eq!(
            runtime.list_imessage_chats().unwrap(),
            vec![ImessageChat {
                chat_id: "43".into(),
                chat_guid: "iMessage;+;second".into(),
                chat_identifier: "second".into(),
                name: "Second".into(),
                service: "iMessage".into(),
                participants: vec!["+15550000003".into()],
            }]
        );
        assert_eq!(
            runtime.status(ChannelKind::IMessage),
            ChannelConnectionStatus::Disconnected
        );
        assert!(
            fs::read_to_string(&fake.log)
                .unwrap()
                .contains("chats.list")
        );

        let invalid = FakeImsg::new(true);
        runtime
            .prepare_imessage_discovery(&invalid.executable)
            .unwrap();
        assert!(runtime.list_imessage_chats().is_err());
        assert_eq!(
            runtime.status(ChannelKind::IMessage),
            ChannelConnectionStatus::Disconnected
        );
        runtime
            .prepare_imessage_discovery(&invalid.executable)
            .unwrap();
        runtime.stop(ChannelKind::IMessage);
        assert_eq!(
            runtime.status(ChannelKind::IMessage),
            ChannelConnectionStatus::Disconnected
        );
    }

    #[test]
    fn unnamed_one_to_one_imessage_chat_preserves_empty_name() {
        assert_eq!(
            validate_imessage_chats(vec![imessage_chat(42, "", &["owner@example.invalid"])])
                .unwrap(),
            vec![ImessageChat {
                chat_id: "42".into(),
                chat_guid: "iMessage;+;chat-42".into(),
                chat_identifier: "chat-42".into(),
                name: String::new(),
                service: "iMessage".into(),
                participants: vec!["owner@example.invalid".into()],
            }]
        );
    }

    #[test]
    fn group_chats_are_excluded_while_one_to_one_self_chat_remains() {
        let chats = validate_imessage_chats(vec![
            imessage_chat(
                84,
                "Family",
                &["second@example.invalid", "owner@example.invalid"],
            ),
            imessage_chat(42, "", &["owner@example.invalid"]),
        ])
        .unwrap();
        assert_eq!(
            chats
                .iter()
                .map(|chat| (chat.chat_id.as_str(), chat.name.as_str()))
                .collect::<Vec<_>>(),
            vec![("42", "")]
        );
    }

    #[test]
    fn self_chat_identity_requires_local_destination_equal_to_the_sole_participant() {
        let chat = ImessageChat {
            chat_id: "42".into(),
            chat_guid: "iMessage;+;self".into(),
            chat_identifier: "self".into(),
            name: String::new(),
            service: "iMessage".into(),
            participants: vec!["owner@example.invalid".into()],
        };
        let mut message = Message {
            id: 7,
            chat_id: ChatId::new(42).unwrap(),
            chat_identifier: "self".into(),
            chat_guid: "iMessage;+;self".into(),
            chat_name: String::new(),
            participants: chat.participants.clone(),
            is_group: false,
            guid: "self-proof-guid".into(),
            sender: String::new(),
            sender_name: None,
            is_from_me: true,
            text: "Owner-authored proof".into(),
            created_at: "2026-07-21T00:00:00Z".into(),
            reply_to_guid: None,
            reply_to_text: None,
            reply_to_sender: None,
            destination_caller_id: Some("owner@example.invalid".into()),
            is_read: None,
            date_read: None,
        };
        assert!(validate_self_chat_identity_evidence(&chat, &[message.clone()]).is_ok());
        message.destination_caller_id = Some("different-local-account@example.invalid".into());
        assert!(validate_self_chat_identity_evidence(&chat, &[message]).is_err());
    }

    #[test]
    fn invalid_imessage_names_and_participants_fail_closed() {
        for name in [
            "\0",
            " untrimmed",
            &"n".repeat(MAX_IMESSAGE_CHAT_FIELD_BYTES + 1),
        ] {
            assert!(
                validate_imessage_chats(vec![imessage_chat(42, name, &["owner@example.invalid"])])
                    .is_err()
            );
        }

        let invalid_participant_sets = [
            vec![""],
            vec!["owner@example.invalid\0"],
            vec![" owner@example.invalid"],
        ];
        for participants in invalid_participant_sets {
            assert!(
                validate_imessage_chats(vec![imessage_chat(42, "Owner", &participants)]).is_err()
            );
        }
        assert!(
            validate_imessage_chats(vec![imessage_chat(42, "Owner", &[])])
                .unwrap()
                .is_empty()
        );
        assert!(
            validate_imessage_chats(vec![imessage_chat(
                42,
                "Owner",
                &["owner@example.invalid", "other@example.invalid"]
            )])
            .unwrap()
            .is_empty()
        );
        let oversized_participant = "p".repeat(MAX_IMESSAGE_CHAT_FIELD_BYTES + 1);
        assert!(
            validate_imessage_chats(vec![imessage_chat(
                42,
                "Owner",
                &[oversized_participant.as_str()]
            )])
            .is_err()
        );
    }

    #[test]
    fn duplicate_imessage_chat_id_fails_closed() {
        assert!(
            validate_imessage_chats(vec![
                imessage_chat(42, "First", &["first@example.invalid"]),
                imessage_chat(42, "Second", &["second@example.invalid"]),
            ])
            .is_err()
        );
    }

    #[test]
    fn non_imessage_rows_are_filtered_before_route_validation() {
        let mut sms = imessage_chat(7, "", &[]);
        sms.service = "SMS".into();
        let valid = imessage_chat(42, "", &["owner@example.invalid"]);
        assert_eq!(
            validate_imessage_chats(vec![sms, valid]).unwrap(),
            vec![ImessageChat {
                chat_id: "42".into(),
                chat_guid: "iMessage;+;chat-42".into(),
                chat_identifier: "chat-42".into(),
                name: String::new(),
                service: "iMessage".into(),
                participants: vec!["owner@example.invalid".into()],
            }]
        );
    }
}
