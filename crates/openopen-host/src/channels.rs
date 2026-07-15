use openopen_discord_adapter::{
    BotToken, ConnectionStatus as DiscordConnectionStatus, DeliveryRecoveryRequest, DiscordAdapter,
    DiscordPairing, DiscordPairingCandidate, DiscordSetupAdapter, DiscordSetupStart,
    InboundEnvelope as DiscordInbound, OutboundRequest, RecoveryBatch, RecoveryRequest,
};
use openopen_imsg_adapter::{
    AdapterConfig as ImsgConfig, Chat, ChatId, ImsgAdapter, ImsgEvent, InboundPairing,
    InboundRejection, ListChatsRequest, MessageCursor, OPENOPEN_IMESSAGE_PREFIX,
    OutboundRecoveryRequest, SendRequest, SubscriptionId, WatchRequest, normalize_inbound,
};
use openopen_protocol::{ChannelCursor, ChannelKind, ChannelPairing};
use serde::Serialize;
use std::collections::VecDeque;
use std::path::Path;
use std::sync::Arc;
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
}

struct DiscordSession {
    adapter: DiscordAdapter,
    inbound: tokio::sync::mpsc::Receiver<DiscordInbound>,
    recovery: Option<Receiver<Result<RecoveryBatch, ()>>>,
    recovered: VecDeque<TransportEvent>,
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
}

struct PreparedImsgSession {
    adapter: Arc<ImsgAdapter>,
    pairing: InboundPairing,
    since_rowid: Option<MessageCursor>,
    conversation_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ImessageChat {
    pub chat_id: String,
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
                let _ = adapter.recover_outbound(&OutboundRecoveryRequest {
                    chat_id: *chat_id,
                    body: match imessage_adapter_body(content) {
                        Some(body) => body.to_owned(),
                        None => return ChannelSendResult::Uncertain,
                    },
                    after_rowid,
                    history_limit: openopen_imsg_adapter::MAX_HISTORY_MESSAGES,
                });
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
        })
    }

    pub(crate) fn start_discord(
        &mut self,
        pairing: &ChannelPairing,
        token: Zeroizing<String>,
        cursor: Option<&ChannelCursor>,
    ) -> Result<ChannelConnectionStatus, ChannelRuntimeError> {
        if self.discord.is_some() {
            return Err(ChannelRuntimeError::AlreadyRunning);
        }
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
        let (adapter, inbound) = DiscordAdapter::new(adapter_pairing, DISCORD_INBOUND_CAPACITY)
            .map_err(|_| ChannelRuntimeError::Adapter)?;
        BotToken::new(token.as_str()).map_err(|_| ChannelRuntimeError::Adapter)?;
        let recovery = if let Some(cursor) = cursor {
            let (sender, receiver) = mpsc::sync_channel(1);
            let recover_adapter = adapter.clone();
            let request = RecoveryRequest {
                conversation_id: pairing.conversation_id.clone(),
                after_message_id: cursor.opaque_value.clone(),
                max_pages: DISCORD_RECOVERY_PAGES,
            };
            self.runtime.spawn(async move {
                let result = async {
                    let token_ref = BotToken::new(token.as_str()).map_err(|_| ())?;
                    recover_adapter.start(token_ref).await.map_err(|_| ())?;
                    recover_adapter
                        .recover_after(&request)
                        .await
                        .map_err(|_| ())
                }
                .await;
                let _ = sender.send(result);
            });
            Some(receiver)
        } else {
            let start_adapter = adapter.clone();
            self.runtime.spawn(async move {
                let Ok(token_ref) = BotToken::new(token.as_str()) else {
                    return;
                };
                let _ = start_adapter.start(token_ref).await;
            });
            None
        };
        self.discord = Some(DiscordSession {
            adapter,
            inbound,
            recovery,
            recovered: VecDeque::new(),
            conversation_id: pairing.conversation_id.clone(),
        });
        Ok(ChannelConnectionStatus::Connecting)
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
        if pairing.channel != ChannelKind::IMessage || !pairing.require_explicit_address {
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
        let adapter = Arc::new(
            ImsgAdapter::spawn(&ImsgConfig::new(executable))
                .map_err(|_| ChannelRuntimeError::Adapter)?,
        );
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
            .and_then(validate_imessage_chats);
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
        });
        Ok(ChannelConnectionStatus::Connected)
    }

    pub(crate) fn status(&self, channel: ChannelKind) -> ChannelConnectionStatus {
        match channel {
            ChannelKind::Discord => {
                self.discord
                    .as_ref()
                    .map_or(
                        ChannelConnectionStatus::Disconnected,
                        |session| match session.adapter.status() {
                            DiscordConnectionStatus::Disconnected
                            | DiscordConnectionStatus::Stopping => {
                                ChannelConnectionStatus::Disconnected
                            }
                            DiscordConnectionStatus::Connecting => {
                                ChannelConnectionStatus::Connecting
                            }
                            DiscordConnectionStatus::Connected => {
                                ChannelConnectionStatus::Connected
                            }
                            DiscordConnectionStatus::Reconnecting => {
                                ChannelConnectionStatus::Reconnecting
                            }
                            DiscordConnectionStatus::Faulted => ChannelConnectionStatus::Faulted,
                        },
                    )
            }
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
        let Some(session) = self.discord.as_mut() else {
            return Ok(None);
        };
        if let Some(event) = session.recovered.pop_front() {
            return Ok(Some(event));
        }
        if let Some(recovery) = session.recovery.as_ref() {
            match recovery.try_recv() {
                Ok(Ok(batch)) => {
                    for envelope in batch.envelopes {
                        session
                            .recovered
                            .push_back(TransportEvent::Inbound(discord_inbound(envelope)?));
                    }
                    let order = batch
                        .high_water_message_id
                        .parse()
                        .map_err(|_| ChannelRuntimeError::Recovery)?;
                    session
                        .recovered
                        .push_back(TransportEvent::Cursor(ChannelCursor {
                            channel: ChannelKind::Discord,
                            conversation_id: session.conversation_id.clone(),
                            opaque_value: batch.high_water_message_id,
                            order,
                            observed_at_ms,
                        }));
                    session.recovery = None;
                    return Ok(session.recovered.pop_front());
                }
                Ok(Err(())) | Err(TryRecvError::Disconnected) => {
                    return Err(ChannelRuntimeError::Recovery);
                }
                Err(TryRecvError::Empty) => return Ok(None),
            }
        }
        match session.inbound.try_recv() {
            Ok(envelope) => Ok(Some(TransportEvent::Inbound(discord_inbound(envelope)?))),
            Err(TokioTryRecvError::Empty) => Ok(None),
            Err(TokioTryRecvError::Disconnected) => Err(ChannelRuntimeError::Adapter),
        }
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
                match normalize_inbound(&session.pairing, &message) {
                    Ok(value) => Ok(Some(TransportEvent::Inbound(TransportInbound {
                        channel: ChannelKind::IMessage,
                        source_message_id: value.source_guid,
                        sender_id: value.sender,
                        conversation_id: session.conversation_id.clone(),
                        content: value.body,
                        cursor_opaque_value: value.source_rowid.to_string(),
                        cursor_order: u64::try_from(value.source_rowid)
                            .map_err(|_| ChannelRuntimeError::Adapter)?,
                        received_at_ms: observed_at_ms,
                    }))),
                    Err(
                        InboundRejection::SenderNotOwner
                        | InboundRejection::FromLocalUser
                        | InboundRejection::NotAddressed
                        | InboundRejection::EmptyBody
                        | InboundRejection::BodyTooLarge,
                    ) if message.id > 0 => Ok(Some(TransportEvent::Cursor(ChannelCursor {
                        channel: ChannelKind::IMessage,
                        conversation_id: session.conversation_id.clone(),
                        opaque_value: message.id.to_string(),
                        order: u64::try_from(message.id)
                            .map_err(|_| ChannelRuntimeError::Adapter)?,
                        observed_at_ms,
                    }))),
                    Err(
                        InboundRejection::InvalidPairing
                        | InboundRejection::ChatNotPaired
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
        match channel {
            ChannelKind::Discord => {
                self.discord
                    .as_ref()
                    .map(|session| ChannelSendHandle::Discord {
                        adapter: session.adapter.clone(),
                        runtime: self.runtime.handle().clone(),
                        conversation_id: session.conversation_id.clone(),
                    })
            }
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

    pub(crate) fn stop_all(&mut self) {
        self.stop(ChannelKind::Discord);
        self.stop(ChannelKind::IMessage);
    }
}

fn validate_imessage_chats(chats: Vec<Chat>) -> Result<Vec<ImessageChat>, ChannelRuntimeError> {
    let mut result = Vec::with_capacity(chats.len());
    for chat in chats {
        if chat.service != "iMessage" {
            continue;
        }
        if !valid_imessage_chat_field(&chat.name, false)
            || chat.participants.is_empty()
            || chat.participants.len() > MAX_IMESSAGE_CHAT_PARTICIPANTS
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
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::thread;

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
                "[]"
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
        emit({{'jsonrpc':'2.0','id':request_id,'result':{{'messages':[{{'id':101,'chat_id':42,'chat_identifier':'first','chat_guid':'iMessage;+;first','chat_name':'First','participants':['+15550000001'],'is_group':False,'guid':'same-text-guid','sender':'','is_from_me':True,'text':'OpenOpen · AI\nWorking on it','created_at':'2026-07-15T00:02:00Z'}}]}}}})
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
            vec![
                ImessageChat {
                    chat_id: "42".into(),
                    name: "First".into(),
                    service: "iMessage".into(),
                    participants: vec!["+15550000001".into(), "+15550000002".into()],
                },
                ImessageChat {
                    chat_id: "43".into(),
                    name: "Second".into(),
                    service: "iMessage".into(),
                    participants: vec!["+15550000003".into()],
                },
            ]
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
}
