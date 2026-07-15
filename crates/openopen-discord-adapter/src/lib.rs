//! Narrow Discord Bot adapter for `OpenOpen`'s approved-conversation boundary.
//!
//! The host owns Keychain access and durable authorization/dedupe state. This
//! crate accepts a borrowed bot credential only while establishing the
//! official Discord Bot Gateway/HTTP client; it never serializes or logs it.

use std::collections::HashSet;
use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::{Deserialize, Serialize};
use serenity::all::{
    ApplicationId, Channel, ChannelId, ConnectionStage, Context, CreateAllowedMentions,
    CreateBotAuthParameters, CreateMessage, EventHandler, GatewayIntents, GuildId, Message,
    MessageId, Nonce, Permissions, Ready, ResumedEvent, Scope, ShardStageUpdateEvent, UserId,
};
use serenity::client::Client;
use serenity::gateway::ShardManager;
use serenity::http::{Http, MessagePagination};
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::sync::{Mutex as AsyncMutex, mpsc, watch};
use tokio::task::JoinHandle;

/// Discord's documented maximum message-content length.
pub const DISCORD_MAX_CONTENT_CHARS: usize = 2_000;

/// Capacity is deliberately bounded so the gateway cannot create unbounded memory work.
pub const DEFAULT_INBOUND_CAPACITY: usize = 32;

/// Discord's documented maximum result count for one channel-history request.
pub const DISCORD_HISTORY_PAGE_SIZE: u8 = 100;

/// Hard upper bound for one restart-recovery operation.
pub const MAX_RECOVERY_PAGES: u8 = 10;

/// Exact `OAuth2` permission bits requested by the local install wizard.
///
/// This is `View Channel | Send Messages | Attach Files | Read Message History`.
pub const DISCORD_INSTALL_PERMISSION_BITS: u64 = 101_376;

/// A setup pairing code is a host-generated 128-bit value encoded as lowercase hex.
pub const DISCORD_PAIRING_CODE_LENGTH: usize = 32;

const MAX_OUTBOUND_ID_BYTES: usize = 256;

const STOP_TIMEOUT: Duration = Duration::from_secs(5);

/// A borrowed credential supplied by the host's Keychain boundary.
///
/// This type intentionally implements neither `Clone` nor serialization. Its
/// `Debug` representation is always redacted.
pub struct BotToken<'a>(&'a str);

impl<'a> BotToken<'a> {
    /// Validates a non-empty token without retaining it.
    ///
    /// # Errors
    ///
    /// Returns [`AdapterError::MissingBotToken`] for an empty value.
    pub fn new(value: &'a str) -> Result<Self, AdapterError> {
        if value.trim().is_empty() {
            return Err(AdapterError::MissingBotToken);
        }
        Ok(Self(value))
    }

    fn expose(&self) -> &'a str {
        self.0
    }
}

impl fmt::Debug for BotToken<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("BotToken(<redacted>)")
    }
}

/// Non-secret identity inferred from the official bot token.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscordBotIdentity {
    pub bot_user_id: u64,
    pub application_id: u64,
    pub bot_name: String,
}

impl DiscordBotIdentity {
    fn validate(&self) -> Result<(), AdapterError> {
        if self.bot_user_id == 0 || self.application_id == 0 || !valid_display_name(&self.bot_name)
        {
            return Err(AdapterError::BotIdentityMismatch);
        }
        Ok(())
    }
}

/// Live effective permissions and readback result for one candidate channel.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DiscordProbeStatus {
    Passed,
    Missing,
}

impl From<bool> for DiscordProbeStatus {
    fn from(value: bool) -> Self {
        if value { Self::Passed } else { Self::Missing }
    }
}

/// Live effective permissions and readback result for one candidate channel.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscordPermissionProbe {
    pub view_channel: DiscordProbeStatus,
    pub send_messages: DiscordProbeStatus,
    pub read_message_history: DiscordProbeStatus,
    pub attach_files: DiscordProbeStatus,
    pub history_readback: DiscordProbeStatus,
    pub effective_permission_bits: u64,
}

impl DiscordPermissionProbe {
    /// True only when all setup permissions and a real history readback succeeded.
    #[must_use]
    pub fn complete(&self) -> bool {
        self.view_channel == DiscordProbeStatus::Passed
            && self.send_messages == DiscordProbeStatus::Passed
            && self.read_message_history == DiscordProbeStatus::Passed
            && self.attach_files == DiscordProbeStatus::Passed
            && self.history_readback == DiscordProbeStatus::Passed
    }
}

/// Setup-only provider data. It never carries message content into the model boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiscordChannelProbe {
    pub guild_name: String,
    pub channel_name: String,
    pub permissions: DiscordPermissionProbe,
}

/// REST boundary used by setup. Tests inject a deterministic implementation.
#[serenity::async_trait]
pub trait DiscordSetupProvider: Send + Sync {
    /// Resolves the official bot and application identity from the borrowed credential.
    async fn current_identity(&self) -> Result<DiscordBotIdentity, AdapterError>;

    /// Reads the exact guild/channel/member state and proves bounded history access.
    async fn probe_channel(
        &self,
        guild_id: u64,
        channel_id: u64,
        bot_user_id: u64,
    ) -> Result<DiscordChannelProbe, AdapterError>;
}

struct SerenitySetupProvider {
    http: Arc<Http>,
}

#[serenity::async_trait]
impl DiscordSetupProvider for SerenitySetupProvider {
    async fn current_identity(&self) -> Result<DiscordBotIdentity, AdapterError> {
        let user = self
            .http
            .get_current_user()
            .await
            .map_err(|error| AdapterError::Serenity(Box::new(error)))?;
        let application = self
            .http
            .get_current_application_info()
            .await
            .map_err(|error| AdapterError::Serenity(Box::new(error)))?;
        let identity = DiscordBotIdentity {
            bot_user_id: user.id.get(),
            application_id: application.id.get(),
            bot_name: user.name.clone(),
        };
        if !user.bot {
            return Err(AdapterError::BotIdentityMismatch);
        }
        identity.validate()?;
        Ok(identity)
    }

    async fn probe_channel(
        &self,
        guild_id: u64,
        channel_id: u64,
        bot_user_id: u64,
    ) -> Result<DiscordChannelProbe, AdapterError> {
        let channel = self
            .http
            .get_channel(ChannelId::new(channel_id))
            .await
            .map_err(|error| AdapterError::Serenity(Box::new(error)))?;
        let Channel::Guild(channel) = channel else {
            return Err(AdapterError::SetupMessageNotGuildChannel);
        };
        if channel.guild_id.get() != guild_id {
            return Err(AdapterError::SetupChannelMismatch);
        }
        let guild = self
            .http
            .get_guild(GuildId::new(guild_id))
            .await
            .map_err(|error| AdapterError::Serenity(Box::new(error)))?;
        let member = self
            .http
            .get_member(GuildId::new(guild_id), UserId::new(bot_user_id))
            .await
            .map_err(|error| AdapterError::Serenity(Box::new(error)))?;
        if member.user.id.get() != bot_user_id || !member.user.bot {
            return Err(AdapterError::BotIdentityMismatch);
        }
        let permissions = guild.user_permissions_in(&channel, &member);
        let read_message_history = permissions.contains(Permissions::READ_MESSAGE_HISTORY);
        let history_readback = if read_message_history {
            self.http
                .get_messages(ChannelId::new(channel_id), None, Some(1))
                .await
                .map_err(|error| AdapterError::Serenity(Box::new(error)))?;
            true
        } else {
            false
        };
        Ok(DiscordChannelProbe {
            guild_name: guild.name,
            channel_name: channel.name,
            permissions: DiscordPermissionProbe {
                view_channel: permissions.contains(Permissions::VIEW_CHANNEL).into(),
                send_messages: permissions.contains(Permissions::SEND_MESSAGES).into(),
                read_message_history: read_message_history.into(),
                attach_files: permissions.contains(Permissions::ATTACH_FILES).into(),
                history_readback: history_readback.into(),
                effective_permission_bits: permissions.bits(),
            },
        })
    }
}

async fn current_identity_unless_disabled(
    provider: &dyn DiscordSetupProvider,
    cancellation: &mut watch::Receiver<u64>,
) -> Result<DiscordBotIdentity, AdapterError> {
    tokio::select! {
        biased;
        changed = cancellation.changed() => {
            let _ = changed;
            Err(AdapterError::Disabled)
        }
        result = provider.current_identity() => result,
    }
}

/// Minimal setup message fields. This type is deliberately distinct from [`GatewayMessage`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiscordSetupMessage {
    pub source_message_id: u64,
    pub guild_id: Option<u64>,
    pub channel_id: u64,
    pub owner_user_id: u64,
    pub owner_is_bot: bool,
    pub owner_name: String,
    pub mentioned_user_ids: Vec<u64>,
    pub content: String,
    pub received_at_ms: i64,
}

impl From<&Message> for DiscordSetupMessage {
    fn from(message: &Message) -> Self {
        Self {
            source_message_id: message.id.get(),
            guild_id: message.guild_id.map(GuildId::get),
            channel_id: message.channel_id.get(),
            owner_user_id: message.author.id.get(),
            owner_is_bot: message.author.bot,
            owner_name: message.author.name.clone(),
            mentioned_user_ids: message.mentions.iter().map(|user| user.id.get()).collect(),
            content: message.content.clone(),
            received_at_ms: message.timestamp.unix_timestamp().saturating_mul(1_000),
        }
    }
}

/// One setup-only candidate. The Host must require owner confirmation before persistence.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscordPairingCandidate {
    pub candidate_id: String,
    pub source_message_id: String,
    pub guild_id: String,
    pub guild_name: String,
    pub channel_id: String,
    pub channel_name: String,
    pub owner_user_id: String,
    pub owner_name: String,
    pub bot_user_id: String,
    pub application_id: String,
    pub received_at_ms: i64,
    pub message_content_intent_ready: bool,
    pub permissions: DiscordPermissionProbe,
}

impl DiscordPairingCandidate {
    /// True only after the explicit Ready gate and every live permission probe passed.
    #[must_use]
    pub fn confirmable(&self) -> bool {
        self.message_content_intent_ready && self.permissions.complete()
    }
}

/// Non-secret values shown by step one of the local setup wizard.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscordSetupStart {
    pub identity: DiscordBotIdentity,
    pub install_url: String,
    pub pairing_code: String,
    pub status: ConnectionStatus,
}

fn required_install_permissions() -> Permissions {
    Permissions::VIEW_CHANNEL
        | Permissions::SEND_MESSAGES
        | Permissions::ATTACH_FILES
        | Permissions::READ_MESSAGE_HISTORY
}

fn gateway_intents() -> GatewayIntents {
    GatewayIntents::GUILDS | GatewayIntents::GUILD_MESSAGES | GatewayIntents::MESSAGE_CONTENT
}

fn validate_pairing_code(value: &str) -> Result<(), AdapterError> {
    if value.len() != DISCORD_PAIRING_CODE_LENGTH
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(AdapterError::InvalidPairingCode);
    }
    Ok(())
}

fn valid_display_name(value: &str) -> bool {
    !value.is_empty() && value.chars().count() <= 100 && !value.chars().any(char::is_control)
}

/// Builds the exact official `OAuth2` bot installation URL.
///
/// # Errors
///
/// Returns an error for a zero application identity or any unexpected permission-bit drift.
pub fn discord_install_url(application_id: u64) -> Result<String, AdapterError> {
    if application_id == 0
        || required_install_permissions().bits() != DISCORD_INSTALL_PERMISSION_BITS
    {
        return Err(AdapterError::InvalidInstallParameters);
    }
    Ok(CreateBotAuthParameters::new()
        .client_id(ApplicationId::new(application_id))
        .scopes(&[Scope::Bot])
        .permissions(required_install_permissions())
        .build())
}

async fn pairing_candidate<P: DiscordSetupProvider + ?Sized>(
    provider: &P,
    identity: &DiscordBotIdentity,
    pairing_code: &str,
    message: &DiscordSetupMessage,
) -> Result<Option<DiscordPairingCandidate>, AdapterError> {
    identity.validate()?;
    validate_pairing_code(pairing_code)?;
    if message.source_message_id == 0
        || message.channel_id == 0
        || message.owner_user_id == 0
        || message.received_at_ms < 0
        || !valid_display_name(&message.owner_name)
    {
        return Err(AdapterError::InvalidSetupMessage);
    }
    let Some(guild_id) = message.guild_id.filter(|value| *value != 0) else {
        return Ok(None);
    };
    if message.owner_is_bot
        || !message.mentioned_user_ids.contains(&identity.bot_user_id)
        || strip_bot_mention(&message.content, identity.bot_user_id)
            != format!("pair {pairing_code}")
    {
        return Ok(None);
    }
    let channel = provider
        .probe_channel(guild_id, message.channel_id, identity.bot_user_id)
        .await?;
    if !valid_display_name(&channel.guild_name) || !valid_display_name(&channel.channel_name) {
        return Err(AdapterError::InvalidSetupMessage);
    }
    let candidate_id = format!(
        "discord-pair-{:x}",
        Sha256::digest(
            format!(
                "{}:{}:{}:{}:{}:{}:{}",
                identity.application_id,
                identity.bot_user_id,
                guild_id,
                message.channel_id,
                message.owner_user_id,
                message.source_message_id,
                pairing_code,
            )
            .as_bytes()
        )
    );
    Ok(Some(DiscordPairingCandidate {
        candidate_id,
        source_message_id: message.source_message_id.to_string(),
        guild_id: guild_id.to_string(),
        guild_name: channel.guild_name,
        channel_id: message.channel_id.to_string(),
        channel_name: channel.channel_name,
        owner_user_id: message.owner_user_id.to_string(),
        owner_name: message.owner_name.clone(),
        bot_user_id: identity.bot_user_id.to_string(),
        application_id: identity.application_id.to_string(),
        received_at_ms: message.received_at_ms,
        message_content_intent_ready: true,
        permissions: channel.permissions,
    }))
}

/// The one-owner, one-channel V1 pairing selected by the user.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscordPairing {
    pub owner_user_id: u64,
    pub approved_channel_id: u64,
    pub bot_user_id: u64,
    pub application_id: u64,
    pub max_content_chars: usize,
}

impl DiscordPairing {
    /// Constructs the fixed V1 pairing with the Discord content bound.
    ///
    /// # Errors
    ///
    /// Returns an error for zero IDs or an invalid content bound.
    pub fn new(
        owner_user_id: u64,
        approved_channel_id: u64,
        bot_user_id: u64,
    ) -> Result<Self, AdapterError> {
        Self::new_with_application(owner_user_id, approved_channel_id, bot_user_id, bot_user_id)
    }

    /// Constructs a pairing that binds both the bot user and OAuth application identities.
    ///
    /// # Errors
    ///
    /// Returns an error for zero IDs or an invalid content bound.
    pub fn new_with_application(
        owner_user_id: u64,
        approved_channel_id: u64,
        bot_user_id: u64,
        application_id: u64,
    ) -> Result<Self, AdapterError> {
        let pairing = Self {
            owner_user_id,
            approved_channel_id,
            bot_user_id,
            application_id,
            max_content_chars: DISCORD_MAX_CONTENT_CHARS,
        };
        pairing.validate()?;
        Ok(pairing)
    }

    /// Validates persisted or decoded configuration before use.
    ///
    /// # Errors
    ///
    /// Returns an error for zero IDs or an invalid content bound.
    pub fn validate(&self) -> Result<(), AdapterError> {
        if self.owner_user_id == 0 {
            return Err(AdapterError::InvalidPairing(
                "owner user id must be nonzero",
            ));
        }
        if self.approved_channel_id == 0 {
            return Err(AdapterError::InvalidPairing(
                "approved channel id must be nonzero",
            ));
        }
        if self.bot_user_id == 0 {
            return Err(AdapterError::InvalidPairing("bot user id must be nonzero"));
        }
        if self.application_id == 0 {
            return Err(AdapterError::InvalidPairing(
                "application id must be nonzero",
            ));
        }
        if self.max_content_chars == 0 || self.max_content_chars > DISCORD_MAX_CONTENT_CHARS {
            return Err(AdapterError::InvalidPairing(
                "content bound must be within Discord's limit",
            ));
        }
        Ok(())
    }

    /// The least-privilege Gateway intents for one approved guild channel.
    #[must_use]
    pub fn gateway_intents(&self) -> GatewayIntents {
        gateway_intents()
    }

    /// Checks that a freshly probed credential still resolves to the approved
    /// bot and application identities.
    #[must_use]
    pub fn matches_identity(&self, identity: &DiscordBotIdentity) -> bool {
        identity.bot_user_id == self.bot_user_id
            && identity.application_id == self.application_id
            && identity.validate().is_ok()
    }
}

/// Platform-neutral inbound fields consumed by the shared channel boundary.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InboundEnvelope {
    pub source_message_id: String,
    pub sender_id: String,
    pub conversation_id: String,
    pub content: String,
    pub received_at_ms: i64,
}

/// Minimal Discord message fields required before shared persistence/model entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GatewayMessage {
    pub message_id: u64,
    pub channel_id: u64,
    pub author_id: u64,
    pub author_is_bot: bool,
    pub mentioned_user_ids: Vec<u64>,
    pub provider_nonce: Option<String>,
    pub content: String,
    pub received_at_ms: i64,
}

impl From<&Message> for GatewayMessage {
    fn from(message: &Message) -> Self {
        Self {
            message_id: message.id.get(),
            channel_id: message.channel_id.get(),
            author_id: message.author.id.get(),
            author_is_bot: message.author.bot,
            mentioned_user_ids: message.mentions.iter().map(|user| user.id.get()).collect(),
            provider_nonce: message.nonce.as_ref().map(|nonce| match nonce {
                Nonce::String(value) => value.clone(),
                Nonce::Number(value) => value.to_string(),
            }),
            content: message.content.clone(),
            received_at_ms: message.timestamp.unix_timestamp().saturating_mul(1_000),
        }
    }
}

/// Filters and normalizes an inbound message before it can reach shared state.
///
/// Bot messages, unpaired users, other channels, and messages without an
/// explicit bot mention are ignored before model access.
///
/// # Errors
///
/// Returns an error for malformed IDs, empty addressed content, or content
/// exceeding the configured bound.
pub fn normalize_inbound(
    pairing: &DiscordPairing,
    message: &GatewayMessage,
) -> Result<Option<InboundEnvelope>, AdapterError> {
    pairing.validate()?;
    if message.message_id == 0
        || message.channel_id == 0
        || message.author_id == 0
        || message.received_at_ms < 0
    {
        return Err(AdapterError::InvalidMessage(
            "message ids must be nonzero and timestamp nonnegative",
        ));
    }
    if message.author_is_bot
        || message.author_id != pairing.owner_user_id
        || message.channel_id != pairing.approved_channel_id
        || !message.mentioned_user_ids.contains(&pairing.bot_user_id)
    {
        return Ok(None);
    }

    let content = strip_bot_mention(&message.content, pairing.bot_user_id);
    if content.is_empty() {
        return Err(AdapterError::EmptyAddressedContent);
    }
    let content_chars = content.chars().count();
    if content_chars > pairing.max_content_chars {
        return Err(AdapterError::ContentTooLong {
            actual: content_chars,
            maximum: pairing.max_content_chars,
        });
    }

    Ok(Some(InboundEnvelope {
        source_message_id: message.message_id.to_string(),
        sender_id: message.author_id.to_string(),
        conversation_id: message.channel_id.to_string(),
        content,
        received_at_ms: message.received_at_ms,
    }))
}

/// Removes exact canonical Discord bot mentions and trims the addressed text.
#[must_use]
pub fn strip_bot_mention(content: &str, bot_user_id: u64) -> String {
    let standard = format!("<@{bot_user_id}>");
    let nickname = format!("<@!{bot_user_id}>");
    content
        .replace(&standard, " ")
        .replace(&nickname, " ")
        .trim()
        .to_owned()
}

/// Outbound content already authorized and durably claimed by the shared boundary.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutboundRequest {
    /// Durable shared-boundary identity; used to derive Discord's provider nonce.
    pub outbound_id: String,
    pub conversation_id: String,
    pub content: String,
    pub reply_to_message_id: Option<String>,
}

/// A successful Discord delivery result. It is delivery evidence, never Mission completion Evidence.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutboundDelivery {
    pub source_message_id: String,
    pub conversation_id: String,
}

/// Bounded restart-recovery request anchored at the last durable Discord snowflake.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryRequest {
    pub conversation_id: String,
    pub after_message_id: String,
    pub max_pages: u8,
}

/// Chronological accepted inbound messages plus the raw high-water snowflake.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryBatch {
    pub envelopes: Vec<InboundEnvelope>,
    pub high_water_message_id: String,
    pub pages_fetched: u8,
}

/// Recover-only lookup for an ambiguous durable outbound intent.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeliveryRecoveryRequest {
    pub outbound_id: String,
    pub conversation_id: String,
    pub after_message_id: String,
    pub max_pages: u8,
}

fn validate_outbound_id(outbound_id: &str) -> Result<(), AdapterError> {
    if outbound_id.is_empty() || outbound_id.len() > MAX_OUTBOUND_ID_BYTES {
        return Err(AdapterError::InvalidOutboundId);
    }
    Ok(())
}

/// Derives a stable 25-character Discord nonce from the durable outbound identity.
///
/// The `o` prefix forces Discord's string nonce representation. The remaining
/// 24 URL-safe characters bind the first 144 bits of SHA-256.
///
/// # Errors
///
/// Returns an error for an empty or excessively large outbound identity.
pub fn outbound_nonce(outbound_id: &str) -> Result<String, AdapterError> {
    validate_outbound_id(outbound_id)?;
    let digest = Sha256::digest(outbound_id.as_bytes());
    let encoded = URL_SAFE_NO_PAD.encode(&digest[..18]);
    Ok(format!("o{encoded}"))
}

fn parse_snowflake(value: &str, field: &'static str) -> Result<u64, AdapterError> {
    let parsed = value
        .parse::<u64>()
        .map_err(|_| AdapterError::InvalidSnowflake(field))?;
    if parsed == 0 {
        return Err(AdapterError::InvalidSnowflake(field));
    }
    Ok(parsed)
}

fn validate_outbound(
    pairing: &DiscordPairing,
    request: &OutboundRequest,
) -> Result<u64, AdapterError> {
    validate_outbound_id(&request.outbound_id)?;
    let channel_id = parse_snowflake(&request.conversation_id, "conversation id")?;
    if channel_id != pairing.approved_channel_id {
        return Err(AdapterError::UnauthorizedChannel);
    }
    let content_chars = request.content.chars().count();
    if request.content.trim().is_empty() {
        return Err(AdapterError::EmptyOutboundContent);
    }
    if content_chars > pairing.max_content_chars {
        return Err(AdapterError::ContentTooLong {
            actual: content_chars,
            maximum: pairing.max_content_chars,
        });
    }
    if let Some(reply_id) = &request.reply_to_message_id {
        parse_snowflake(reply_id, "reply message id")?;
    }
    Ok(channel_id)
}

fn build_message(request: &OutboundRequest) -> CreateMessage {
    let nonce = outbound_nonce(&request.outbound_id).expect("validated outbound id");
    let mut builder = CreateMessage::new()
        .content(request.content.clone())
        .nonce(Nonce::String(nonce))
        .enforce_nonce(true)
        .allowed_mentions(
            CreateAllowedMentions::new()
                .all_users(false)
                .all_roles(false)
                .everyone(false)
                .empty_users()
                .empty_roles()
                .replied_user(false),
        );
    if let Some(reply_id) = &request.reply_to_message_id {
        let message_id =
            parse_snowflake(reply_id, "reply message id").expect("validated outbound reply id");
        let channel_id = parse_snowflake(&request.conversation_id, "conversation id")
            .expect("validated outbound channel id");
        builder =
            builder.reference_message((ChannelId::new(channel_id), MessageId::new(message_id)));
    }
    builder
}

fn validate_recovery_request(
    pairing: &DiscordPairing,
    conversation_id: &str,
    after_message_id: &str,
    max_pages: u8,
) -> Result<(u64, u64), AdapterError> {
    let channel_id = parse_snowflake(conversation_id, "conversation id")?;
    if channel_id != pairing.approved_channel_id {
        return Err(AdapterError::UnauthorizedChannel);
    }
    let after = parse_snowflake(after_message_id, "recovery cursor")?;
    if max_pages == 0 || max_pages > MAX_RECOVERY_PAGES {
        return Err(AdapterError::InvalidRecoveryBound);
    }
    Ok((channel_id, after))
}

#[derive(Debug)]
struct ValidatedRecovery {
    messages: Vec<GatewayMessage>,
    high_water_message_id: u64,
    pages_fetched: u8,
}

fn validate_recovery_pages(
    after_message_id: u64,
    pages: Vec<Vec<GatewayMessage>>,
    max_pages: u8,
) -> Result<ValidatedRecovery, AdapterError> {
    if pages.is_empty() || pages.len() > usize::from(max_pages) {
        return Err(AdapterError::InvalidRecoveryPage);
    }
    let mut seen = HashSet::new();
    let mut messages = Vec::new();
    let mut previous_boundary = None;
    let mut complete = false;
    let pages_fetched = u8::try_from(pages.len()).map_err(|_| AdapterError::InvalidRecoveryPage)?;

    for (index, page) in pages.into_iter().enumerate() {
        if page.len() > usize::from(DISCORD_HISTORY_PAGE_SIZE) || complete {
            return Err(AdapterError::InvalidRecoveryPage);
        }
        if index == 0
            && page
                .iter()
                .any(|message| message.message_id <= after_message_id)
        {
            return Err(AdapterError::InvalidRecoveryPage);
        }
        if let Some(boundary) = previous_boundary
            && page.iter().any(|message| message.message_id >= boundary)
        {
            return Err(AdapterError::InvalidRecoveryPage);
        }
        for message in &page {
            if message.message_id == 0 || !seen.insert(message.message_id) {
                return Err(AdapterError::InvalidRecoveryPage);
            }
            if message.message_id > after_message_id {
                messages.push(message.clone());
            }
        }
        let crossed_cursor = page
            .iter()
            .any(|message| message.message_id <= after_message_id);
        complete = page.len() < usize::from(DISCORD_HISTORY_PAGE_SIZE) || crossed_cursor;
        previous_boundary = page.iter().map(|message| message.message_id).min();
    }

    if !complete {
        return Err(AdapterError::RecoveryWindowExceeded);
    }
    messages.sort_by_key(|message| message.message_id);
    let high_water_message_id = messages
        .last()
        .map_or(after_message_id, |message| message.message_id);
    Ok(ValidatedRecovery {
        messages,
        high_water_message_id,
        pages_fetched,
    })
}

fn recovery_batch_from_pages(
    pairing: &DiscordPairing,
    after_message_id: u64,
    pages: Vec<Vec<GatewayMessage>>,
    max_pages: u8,
) -> Result<RecoveryBatch, AdapterError> {
    let validated = validate_recovery_pages(after_message_id, pages, max_pages)?;
    let envelopes = validated
        .messages
        .iter()
        .map(|message| normalize_inbound(pairing, message))
        .filter_map(Result::transpose)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(RecoveryBatch {
        envelopes,
        high_water_message_id: validated.high_water_message_id.to_string(),
        pages_fetched: validated.pages_fetched,
    })
}

fn recovered_delivery_from_pages(
    pairing: &DiscordPairing,
    outbound_id: &str,
    after_message_id: u64,
    pages: Vec<Vec<GatewayMessage>>,
    max_pages: u8,
) -> Result<Option<OutboundDelivery>, AdapterError> {
    let expected_nonce = outbound_nonce(outbound_id)?;
    let validated = validate_recovery_pages(after_message_id, pages, max_pages)?;
    let matches = validated
        .messages
        .iter()
        .filter(|message| {
            message.author_is_bot
                && message.author_id == pairing.bot_user_id
                && message.channel_id == pairing.approved_channel_id
                && message.provider_nonce.as_deref() == Some(expected_nonce.as_str())
        })
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [] => Ok(None),
        [message] => Ok(Some(OutboundDelivery {
            source_message_id: message.message_id.to_string(),
            conversation_id: message.channel_id.to_string(),
        })),
        _ => Err(AdapterError::ProviderNonceConflict),
    }
}

/// Observable adapter lifecycle. `Faulted` never aliases `Disconnected`.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ConnectionStatus {
    Disconnected,
    Connecting,
    Connected,
    Reconnecting,
    Stopping,
    Faulted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RunLease(u64);

#[derive(Debug)]
struct LifecycleState {
    status: ConnectionStatus,
    generation: u64,
    enabled: bool,
}

#[derive(Debug)]
struct Lifecycle {
    state: Mutex<LifecycleState>,
    generation_tx: watch::Sender<u64>,
}

impl Lifecycle {
    fn new() -> Self {
        let (generation_tx, _) = watch::channel(0);
        Self {
            state: Mutex::new(LifecycleState {
                status: ConnectionStatus::Disconnected,
                generation: 0,
                enabled: false,
            }),
            generation_tx,
        }
    }

    fn lock(&self) -> MutexGuard<'_, LifecycleState> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn begin_connect(&self) -> Result<RunLease, AdapterError> {
        let mut state = self.lock();
        if state.enabled {
            return Err(AdapterError::AlreadyRunning);
        }
        state.generation = state.generation.wrapping_add(1);
        state.enabled = true;
        state.status = ConnectionStatus::Connecting;
        self.generation_tx.send_replace(state.generation);
        Ok(RunLease(state.generation))
    }

    fn update(&self, lease: RunLease, status: ConnectionStatus) {
        let mut state = self.lock();
        if state.enabled && state.generation == lease.0 {
            state.status = status;
        }
    }

    fn terminate(&self, lease: RunLease, status: ConnectionStatus) {
        let mut state = self.lock();
        if state.generation == lease.0 {
            state.enabled = false;
            state.status = status;
            state.generation = state.generation.wrapping_add(1);
            self.generation_tx.send_replace(state.generation);
        }
    }

    fn disable(&self) {
        let mut state = self.lock();
        state.enabled = false;
        state.status = ConnectionStatus::Stopping;
        state.generation = state.generation.wrapping_add(1);
        self.generation_tx.send_replace(state.generation);
    }

    fn disconnected(&self) {
        self.lock().status = ConnectionStatus::Disconnected;
    }

    fn status(&self) -> ConnectionStatus {
        self.lock().status
    }

    fn generation(&self) -> u64 {
        self.lock().generation
    }

    fn permits(&self, lease: RunLease) -> bool {
        let state = self.lock();
        state.enabled && state.generation == lease.0
    }

    fn permits_connected(&self, lease: RunLease) -> bool {
        let state = self.lock();
        state.enabled && state.generation == lease.0 && state.status == ConnectionStatus::Connected
    }

    fn subscribe(&self) -> watch::Receiver<u64> {
        self.generation_tx.subscribe()
    }
}

struct DiscordSetupHandler {
    identity: DiscordBotIdentity,
    pairing_code: String,
    lifecycle: Arc<Lifecycle>,
    lease: RunLease,
    provider: Arc<dyn DiscordSetupProvider>,
    candidate_tx: mpsc::Sender<DiscordPairingCandidate>,
}

impl fmt::Debug for DiscordSetupHandler {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DiscordSetupHandler")
            .field("identity", &self.identity)
            .field("status", &self.lifecycle.status())
            .finish_non_exhaustive()
    }
}

#[serenity::async_trait]
impl EventHandler for DiscordSetupHandler {
    async fn message(&self, _context: Context, message: Message) {
        if !self.lifecycle.permits_connected(self.lease) {
            return;
        }
        let candidate = match pairing_candidate(
            self.provider.as_ref(),
            &self.identity,
            &self.pairing_code,
            &DiscordSetupMessage::from(&message),
        )
        .await
        {
            Ok(Some(candidate)) => candidate,
            Ok(None) => return,
            Err(_) => {
                self.lifecycle
                    .terminate(self.lease, ConnectionStatus::Faulted);
                return;
            }
        };
        let mut generation = self.lifecycle.subscribe();
        tokio::select! {
            biased;
            changed = generation.changed() => {
                let _ = changed;
            }
            result = self.candidate_tx.send(candidate) => {
                if result.is_err() {
                    self.lifecycle
                        .terminate(self.lease, ConnectionStatus::Faulted);
                }
            }
        }
    }

    async fn ready(&self, _context: Context, ready: Ready) {
        if !ready.user.bot
            || ready.user.id.get() != self.identity.bot_user_id
            || ready.application.id.get() != self.identity.application_id
        {
            self.lifecycle
                .terminate(self.lease, ConnectionStatus::Faulted);
            return;
        }
        // This Ready is received only after Discord accepts our explicit
        // MESSAGE_CONTENT identify request. A 4014 disallowed-intent close
        // never reaches this state.
        self.lifecycle
            .update(self.lease, ConnectionStatus::Connected);
    }

    async fn resume(&self, _context: Context, _event: ResumedEvent) {
        self.lifecycle
            .update(self.lease, ConnectionStatus::Connected);
    }

    async fn shard_stage_update(&self, _context: Context, event: ShardStageUpdateEvent) {
        let status = match event.new {
            // A transport-level connection is not setup authority. Only the
            // Ready/Resume callbacks above can publish Connected after Discord
            // accepts the requested intents and exact identity.
            ConnectionStage::Connected
            | ConnectionStage::Disconnected
            | ConnectionStage::Connecting
            | ConnectionStage::Handshake
            | ConnectionStage::Identifying
            | ConnectionStage::Resuming => ConnectionStatus::Reconnecting,
            _ => ConnectionStatus::Faulted,
        };
        self.lifecycle.update(self.lease, status);
    }
}

struct SetupInner {
    identity: DiscordBotIdentity,
    pairing_code: String,
    lifecycle: Arc<Lifecycle>,
    shard_manager: AsyncMutex<Option<Arc<ShardManager>>>,
    gateway_task: AsyncMutex<Option<JoinHandle<()>>>,
    operation: AsyncMutex<()>,
}

/// Setup-only Discord session. Its events can produce pairing candidates but
/// never operational [`InboundEnvelope`] values.
#[derive(Clone)]
pub struct DiscordSetupAdapter {
    inner: Arc<SetupInner>,
}

impl fmt::Debug for DiscordSetupAdapter {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DiscordSetupAdapter")
            .field("identity", &self.inner.identity)
            .field("status", &self.status())
            .finish_non_exhaustive()
    }
}

impl DiscordSetupAdapter {
    /// Probes the borrowed bot credential, builds the exact official install
    /// link, and starts an isolated setup Gateway listener.
    ///
    /// The caller supplies a fresh random 128-bit lowercase-hex pairing code.
    /// Setup messages are returned only as candidates and require a later owner
    /// confirmation in the Host before durable pairing.
    ///
    /// # Errors
    ///
    /// Returns an error for malformed setup input, credential/identity failure,
    /// client construction failure, or zero candidate capacity.
    pub async fn start(
        token: BotToken<'_>,
        pairing_code: String,
        candidate_capacity: usize,
    ) -> Result<
        (
            Self,
            DiscordSetupStart,
            mpsc::Receiver<DiscordPairingCandidate>,
        ),
        AdapterError,
    > {
        validate_pairing_code(&pairing_code)?;
        if candidate_capacity == 0 {
            return Err(AdapterError::InvalidInboundCapacity);
        }
        let lifecycle = Arc::new(Lifecycle::new());
        let lease = lifecycle.begin_connect()?;
        let provider: Arc<dyn DiscordSetupProvider> = Arc::new(SerenitySetupProvider {
            http: Arc::new(Http::new(token.expose())),
        });
        let identity = match provider.current_identity().await {
            Ok(identity) => identity,
            Err(error) => {
                lifecycle.terminate(lease, ConnectionStatus::Faulted);
                return Err(error);
            }
        };
        let install_url = discord_install_url(identity.application_id)?;
        let (candidate_tx, candidate_rx) = mpsc::channel(candidate_capacity);
        let handler = DiscordSetupHandler {
            identity: identity.clone(),
            pairing_code: pairing_code.clone(),
            lifecycle: Arc::clone(&lifecycle),
            lease,
            provider,
            candidate_tx,
        };
        let mut client = match Client::builder(token.expose(), gateway_intents())
            .event_handler(handler)
            .await
        {
            Ok(client) => client,
            Err(error) => {
                lifecycle.terminate(lease, ConnectionStatus::Faulted);
                return Err(AdapterError::Serenity(Box::new(error)));
            }
        };
        let shard_manager = Arc::clone(&client.shard_manager);
        let task_lifecycle = Arc::clone(&lifecycle);
        let gateway_task = tokio::spawn(async move {
            let result = client.start().await;
            if task_lifecycle.permits(lease) {
                task_lifecycle.terminate(
                    lease,
                    if result.is_ok() {
                        ConnectionStatus::Disconnected
                    } else {
                        ConnectionStatus::Faulted
                    },
                );
            }
        });
        let adapter = Self {
            inner: Arc::new(SetupInner {
                identity: identity.clone(),
                pairing_code: pairing_code.clone(),
                lifecycle,
                shard_manager: AsyncMutex::new(Some(shard_manager)),
                gateway_task: AsyncMutex::new(Some(gateway_task)),
                operation: AsyncMutex::new(()),
            }),
        };
        let start = DiscordSetupStart {
            identity,
            install_url,
            pairing_code,
            status: adapter.status(),
        };
        Ok((adapter, start, candidate_rx))
    }

    /// Returns the current non-secret setup connection state.
    #[must_use]
    pub fn status(&self) -> ConnectionStatus {
        self.inner.lifecycle.status()
    }

    /// Returns the exact token-derived identity for Host persistence after confirmation.
    #[must_use]
    pub fn identity(&self) -> DiscordBotIdentity {
        self.inner.identity.clone()
    }

    /// Returns the Host-generated setup code for exact candidate confirmation.
    #[must_use]
    pub fn pairing_code(&self) -> &str {
        &self.inner.pairing_code
    }

    /// Invalidates setup events before awaiting Gateway shutdown.
    pub async fn stop(&self) {
        self.inner.lifecycle.disable();
        let _operation = self.inner.operation.lock().await;
        if let Some(manager) = self.inner.shard_manager.lock().await.take() {
            manager.shutdown_all().await;
        }
        if let Some(mut task) = self.inner.gateway_task.lock().await.take()
            && tokio::time::timeout(STOP_TIMEOUT, &mut task).await.is_err()
        {
            task.abort();
            let _ = task.await;
        }
        self.inner.lifecycle.disconnected();
    }
}

#[derive(Debug)]
struct DiscordHandler {
    pairing: DiscordPairing,
    lifecycle: Arc<Lifecycle>,
    lease: RunLease,
    inbound_tx: mpsc::Sender<InboundEnvelope>,
}

#[serenity::async_trait]
impl EventHandler for DiscordHandler {
    async fn message(&self, _context: Context, message: Message) {
        if !self.lifecycle.permits(self.lease) {
            return;
        }
        let envelope = match normalize_inbound(&self.pairing, &GatewayMessage::from(&message)) {
            Ok(Some(envelope)) => envelope,
            Ok(None) => return,
            Err(_) => {
                self.lifecycle
                    .terminate(self.lease, ConnectionStatus::Faulted);
                return;
            }
        };
        let mut generation = self.lifecycle.subscribe();
        tokio::select! {
            biased;
            changed = generation.changed() => {
                let _ = changed;
            }
            result = self.inbound_tx.send(envelope) => {
                if result.is_err() {
                    self.lifecycle
                        .terminate(self.lease, ConnectionStatus::Faulted);
                }
            }
        }
    }

    async fn ready(&self, _context: Context, ready: Ready) {
        if !ready.user.bot
            || !self.pairing.matches_identity(&DiscordBotIdentity {
                bot_user_id: ready.user.id.get(),
                application_id: ready.application.id.get(),
                bot_name: ready.user.name.clone(),
            })
        {
            self.lifecycle
                .terminate(self.lease, ConnectionStatus::Faulted);
            return;
        }
        self.lifecycle
            .update(self.lease, ConnectionStatus::Connected);
    }

    async fn resume(&self, _context: Context, _event: ResumedEvent) {
        self.lifecycle
            .update(self.lease, ConnectionStatus::Connected);
    }

    async fn shard_stage_update(&self, _context: Context, event: ShardStageUpdateEvent) {
        let status = match event.new {
            // Ready/Resume, not the lower transport stage, proves the exact
            // bot/application identity for this run.
            ConnectionStage::Connected
            | ConnectionStage::Disconnected
            | ConnectionStage::Connecting
            | ConnectionStage::Handshake
            | ConnectionStage::Identifying
            | ConnectionStage::Resuming => ConnectionStatus::Reconnecting,
            _ => ConnectionStatus::Faulted,
        };
        self.lifecycle.update(self.lease, status);
    }
}

struct Inner {
    pairing: DiscordPairing,
    lifecycle: Arc<Lifecycle>,
    inbound_tx: mpsc::Sender<InboundEnvelope>,
    http: AsyncMutex<Option<Arc<Http>>>,
    shard_manager: AsyncMutex<Option<Arc<ShardManager>>>,
    gateway_task: AsyncMutex<Option<JoinHandle<()>>>,
    operation: AsyncMutex<()>,
}

/// Official Discord Bot Gateway/HTTP adapter backed by pinned serenity.
#[derive(Clone)]
pub struct DiscordAdapter {
    inner: Arc<Inner>,
}

impl fmt::Debug for DiscordAdapter {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DiscordAdapter")
            .field("pairing", &self.inner.pairing)
            .field("status", &self.status())
            .finish_non_exhaustive()
    }
}

impl DiscordAdapter {
    /// Creates a stopped adapter and its bounded inbound receiver.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid pairing or zero queue capacity.
    pub fn new(
        pairing: DiscordPairing,
        inbound_capacity: usize,
    ) -> Result<(Self, mpsc::Receiver<InboundEnvelope>), AdapterError> {
        pairing.validate()?;
        if inbound_capacity == 0 {
            return Err(AdapterError::InvalidInboundCapacity);
        }
        let (inbound_tx, inbound_rx) = mpsc::channel(inbound_capacity);
        Ok((
            Self {
                inner: Arc::new(Inner {
                    pairing,
                    lifecycle: Arc::new(Lifecycle::new()),
                    inbound_tx,
                    http: AsyncMutex::new(None),
                    shard_manager: AsyncMutex::new(None),
                    gateway_task: AsyncMutex::new(None),
                    operation: AsyncMutex::new(()),
                }),
            },
            inbound_rx,
        ))
    }

    /// Returns the current non-secret connection state.
    #[must_use]
    pub fn status(&self) -> ConnectionStatus {
        self.inner.lifecycle.status()
    }

    async fn active_http(&self) -> Result<(RunLease, Arc<Http>), AdapterError> {
        let lease = {
            let state = self.inner.lifecycle.lock();
            if !state.enabled {
                return Err(AdapterError::Disabled);
            }
            RunLease(state.generation)
        };
        let http = self
            .inner
            .http
            .lock()
            .await
            .clone()
            .ok_or(AdapterError::Disabled)?;
        if !self.inner.lifecycle.permits(lease) {
            return Err(AdapterError::Disabled);
        }
        Ok((lease, http))
    }

    async fn fetch_history_page(
        &self,
        lease: RunLease,
        http: &Http,
        channel_id: u64,
        pagination: MessagePagination,
    ) -> Result<Vec<GatewayMessage>, AdapterError> {
        let mut generation = self.inner.lifecycle.subscribe();
        let messages = tokio::select! {
            biased;
            changed = generation.changed() => {
                let _ = changed;
                return Err(AdapterError::Disabled);
            }
            result = http.get_messages(
                ChannelId::new(channel_id),
                Some(pagination),
                Some(DISCORD_HISTORY_PAGE_SIZE),
            ) => result.map_err(|error| AdapterError::Serenity(Box::new(error)))?,
        };
        if !self.inner.lifecycle.permits(lease) {
            return Err(AdapterError::Disabled);
        }
        Ok(messages.iter().map(GatewayMessage::from).collect())
    }

    async fn fetch_history_pages(
        &self,
        channel_id: u64,
        after_message_id: u64,
        max_pages: u8,
    ) -> Result<Vec<Vec<GatewayMessage>>, AdapterError> {
        let (lease, http) = self.active_http().await?;
        let first = self
            .fetch_history_page(
                lease,
                &http,
                channel_id,
                MessagePagination::After(MessageId::new(after_message_id)),
            )
            .await?;
        let mut pages = vec![first];
        while pages
            .last()
            .is_some_and(|page| page.len() == usize::from(DISCORD_HISTORY_PAGE_SIZE))
            && pages.len() < usize::from(max_pages)
        {
            let before = pages
                .last()
                .and_then(|page| page.iter().map(|message| message.message_id).min())
                .ok_or(AdapterError::InvalidRecoveryPage)?;
            let page = self
                .fetch_history_page(
                    lease,
                    &http,
                    channel_id,
                    MessagePagination::Before(MessageId::new(before)),
                )
                .await?;
            let complete = page.len() < usize::from(DISCORD_HISTORY_PAGE_SIZE)
                || page
                    .iter()
                    .any(|message| message.message_id <= after_message_id);
            pages.push(page);
            if complete {
                break;
            }
        }
        Ok(pages)
    }

    /// Reads a complete bounded restart window after a durable Discord cursor.
    ///
    /// Discord returns history newest-first. If the first 100-message page is
    /// full, this walks backward until it crosses the durable cursor. Hitting
    /// `max_pages` before crossing fails closed so the host cannot skip a gap.
    /// Returned accepted envelopes are chronological; the high-water mark
    /// covers every observed message, including ignored bot/unauthorized ones.
    ///
    /// # Errors
    ///
    /// Returns an error for Off, wrong channel, invalid bounds, malformed or
    /// gapped provider pages, an oversized accepted message, or HTTP failure.
    pub async fn recover_after(
        &self,
        request: &RecoveryRequest,
    ) -> Result<RecoveryBatch, AdapterError> {
        let (channel_id, after_message_id) = validate_recovery_request(
            &self.inner.pairing,
            &request.conversation_id,
            &request.after_message_id,
            request.max_pages,
        )?;
        let pages = self
            .fetch_history_pages(channel_id, after_message_id, request.max_pages)
            .await?;
        recovery_batch_from_pages(
            &self.inner.pairing,
            after_message_id,
            pages,
            request.max_pages,
        )
    }

    /// Recovers an ambiguous provider delivery by its deterministic nonce.
    ///
    /// This method performs readback only. If no matching bot-authored message
    /// is found, it returns `None`; callers must keep the durable intent in
    /// recover-only state and must not call [`Self::send`] again.
    ///
    /// # Errors
    ///
    /// Returns an error for Off, invalid identity/bounds, incomplete history,
    /// multiple provider messages with one nonce, or HTTP failure.
    pub async fn recover_delivery(
        &self,
        request: &DeliveryRecoveryRequest,
    ) -> Result<Option<OutboundDelivery>, AdapterError> {
        validate_outbound_id(&request.outbound_id)?;
        let (channel_id, after_message_id) = validate_recovery_request(
            &self.inner.pairing,
            &request.conversation_id,
            &request.after_message_id,
            request.max_pages,
        )?;
        let pages = self
            .fetch_history_pages(channel_id, after_message_id, request.max_pages)
            .await?;
        recovered_delivery_from_pages(
            &self.inner.pairing,
            &request.outbound_id,
            after_message_id,
            pages,
            request.max_pages,
        )
    }

    /// Connects with the official bot identity and starts one Gateway shard.
    ///
    /// The supplied credential is first probed through Discord's official Bot
    /// HTTP surface and rejected if it does not resolve to a bot account.
    /// Serenity owns reconnect/backoff after the shard starts.
    ///
    /// # Errors
    ///
    /// Returns an error for duplicate starts, authentication, non-bot identity,
    /// or client construction failure.
    pub async fn start(&self, token: BotToken<'_>) -> Result<(), AdapterError> {
        let requested_generation = self.inner.lifecycle.generation();
        let _operation = self.inner.operation.lock().await;
        if self.inner.lifecycle.generation() != requested_generation {
            return Err(AdapterError::Disabled);
        }
        let lease = self.inner.lifecycle.begin_connect()?;
        let mut cancellation = self.inner.lifecycle.subscribe();
        let handler = DiscordHandler {
            pairing: self.inner.pairing.clone(),
            lifecycle: Arc::clone(&self.inner.lifecycle),
            lease,
            inbound_tx: self.inner.inbound_tx.clone(),
        };
        let client_result = tokio::select! {
            biased;
            changed = cancellation.changed() => {
                let _ = changed;
                return Err(AdapterError::Disabled);
            }
            result = Client::builder(token.expose(), self.inner.pairing.gateway_intents())
                .event_handler(handler) => result,
        };
        let mut client = match client_result {
            Ok(client) => client,
            Err(error) => {
                self.inner
                    .lifecycle
                    .terminate(lease, ConnectionStatus::Faulted);
                return Err(AdapterError::Serenity(Box::new(error)));
            }
        };

        let identity_provider = SerenitySetupProvider {
            http: Arc::clone(&client.http),
        };
        let identity =
            match current_identity_unless_disabled(&identity_provider, &mut cancellation).await {
                Ok(identity) => identity,
                Err(AdapterError::Disabled) => return Err(AdapterError::Disabled),
                Err(error) => {
                    self.inner
                        .lifecycle
                        .terminate(lease, ConnectionStatus::Faulted);
                    return Err(error);
                }
            };
        if !self.inner.pairing.matches_identity(&identity) {
            self.inner
                .lifecycle
                .terminate(lease, ConnectionStatus::Faulted);
            return Err(AdapterError::BotIdentityMismatch);
        }
        if !self.inner.lifecycle.permits(lease) {
            return Err(AdapterError::Disabled);
        }

        let shard_manager = Arc::clone(&client.shard_manager);
        let http = Arc::clone(&client.http);
        *self.inner.http.lock().await = Some(http);
        *self.inner.shard_manager.lock().await = Some(Arc::clone(&shard_manager));

        let lifecycle = Arc::clone(&self.inner.lifecycle);
        let task = tokio::spawn(async move {
            let result = client.start().await;
            if lifecycle.permits(lease) {
                lifecycle.terminate(
                    lease,
                    if result.is_ok() {
                        ConnectionStatus::Disconnected
                    } else {
                        ConnectionStatus::Faulted
                    },
                );
            }
        });
        *self.inner.gateway_task.lock().await = Some(task);
        Ok(())
    }

    /// Sends only to the paired channel and suppresses every Discord mention.
    ///
    /// Global Off invalidates the run generation and cancels the in-flight HTTP
    /// future. A cancellation is reported as delivery-uncertain and must be
    /// reconciled by the shared durable outbound boundary, never blindly retried.
    ///
    /// # Errors
    ///
    /// Returns an error for Off/disconnected state, wrong channel, invalid
    /// content, transport failure, or cancellation during delivery.
    pub async fn send(&self, request: &OutboundRequest) -> Result<OutboundDelivery, AdapterError> {
        let channel_id = validate_outbound(&self.inner.pairing, request)?;
        let expected_nonce = outbound_nonce(&request.outbound_id)?;
        let lease = {
            let state = self.inner.lifecycle.lock();
            if !state.enabled || state.status != ConnectionStatus::Connected {
                return Err(AdapterError::Disabled);
            }
            RunLease(state.generation)
        };
        let http = self
            .inner
            .http
            .lock()
            .await
            .clone()
            .ok_or(AdapterError::Disabled)?;
        let mut generation = self.inner.lifecycle.subscribe();
        if !self.inner.lifecycle.permits(lease) {
            return Err(AdapterError::Disabled);
        }

        let send = ChannelId::new(channel_id).send_message(&http, build_message(request));
        let message = tokio::select! {
            biased;
            changed = generation.changed() => {
                let _ = changed;
                return Err(AdapterError::DeliveryUncertainAfterOff);
            }
            result = send => result.map_err(|error| AdapterError::Serenity(Box::new(error)))?,
        };
        if !self.inner.lifecycle.permits(lease) {
            return Err(AdapterError::DeliveryUncertainAfterOff);
        }
        if message.channel_id.get() != channel_id
            || message.id.get() == 0
            || !matches!(
                message.nonce.as_ref(),
                Some(Nonce::String(value)) if value == &expected_nonce
            )
        {
            return Err(AdapterError::ProviderNonceMismatch);
        }
        Ok(OutboundDelivery {
            source_message_id: message.id.to_string(),
            conversation_id: message.channel_id.to_string(),
        })
    }

    /// Stops listeners and invalidates model/outbound entry before awaiting Gateway shutdown.
    ///
    /// Calling `start` again requires the host to fetch and re-inject the
    /// Keychain credential; the adapter never retains a reconnect credential.
    pub async fn stop(&self) {
        self.inner.lifecycle.disable();
        let _operation = self.inner.operation.lock().await;
        self.inner.http.lock().await.take();
        if let Some(manager) = self.inner.shard_manager.lock().await.take() {
            manager.shutdown_all().await;
        }
        if let Some(mut task) = self.inner.gateway_task.lock().await.take()
            && tokio::time::timeout(STOP_TIMEOUT, &mut task).await.is_err()
        {
            task.abort();
            let _ = task.await;
        }
        self.inner.lifecycle.disconnected();
    }
}

/// Fail-closed Discord adapter errors. None contain credential values.
#[derive(Debug, Error)]
pub enum AdapterError {
    #[error("Discord bot token is missing")]
    MissingBotToken,
    #[error("Discord pairing is invalid: {0}")]
    InvalidPairing(&'static str),
    #[error("Discord inbound capacity must be nonzero")]
    InvalidInboundCapacity,
    #[error("Discord message is invalid: {0}")]
    InvalidMessage(&'static str),
    #[error("addressed Discord content is empty")]
    EmptyAddressedContent,
    #[error("outbound Discord content is empty")]
    EmptyOutboundContent,
    #[error("Discord content has {actual} characters; maximum is {maximum}")]
    ContentTooLong { actual: usize, maximum: usize },
    #[error("invalid Discord {0}")]
    InvalidSnowflake(&'static str),
    #[error("durable Discord outbound id is empty or too large")]
    InvalidOutboundId,
    #[error("Discord channel is not the approved paired channel")]
    UnauthorizedChannel,
    #[error("Discord recovery page bound must be between 1 and 10")]
    InvalidRecoveryBound,
    #[error("Discord returned a malformed or noncontiguous recovery page")]
    InvalidRecoveryPage,
    #[error("Discord recovery exceeded the bounded history window")]
    RecoveryWindowExceeded,
    #[error("Discord adapter is already running")]
    AlreadyRunning,
    #[error("Discord adapter is Off or disconnected")]
    Disabled,
    #[error("Discord credential did not resolve to the paired bot identity")]
    BotIdentityMismatch,
    #[error("Discord setup pairing code must be 128-bit lowercase hex")]
    InvalidPairingCode,
    #[error("Discord setup install parameters are invalid")]
    InvalidInstallParameters,
    #[error("Discord setup message is malformed")]
    InvalidSetupMessage,
    #[error("Discord setup message is not in a guild channel")]
    SetupMessageNotGuildChannel,
    #[error("Discord setup channel does not match the message guild")]
    SetupChannelMismatch,
    #[error("Discord delivery became uncertain while global Off was applied")]
    DeliveryUncertainAfterOff,
    #[error("Discord response did not bind the expected outbound nonce")]
    ProviderNonceMismatch,
    #[error("Discord history contained multiple messages for one outbound nonce")]
    ProviderNonceConflict,
    #[error("Discord transport failed")]
    Serenity(#[source] Box<serenity::Error>),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct MockSetupProvider {
        identity: DiscordBotIdentity,
        probe: DiscordChannelProbe,
        probe_calls: Mutex<Vec<(u64, u64, u64)>>,
    }

    #[serenity::async_trait]
    impl DiscordSetupProvider for MockSetupProvider {
        async fn current_identity(&self) -> Result<DiscordBotIdentity, AdapterError> {
            Ok(self.identity.clone())
        }

        async fn probe_channel(
            &self,
            guild_id: u64,
            channel_id: u64,
            bot_user_id: u64,
        ) -> Result<DiscordChannelProbe, AdapterError> {
            self.probe_calls
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push((guild_id, channel_id, bot_user_id));
            Ok(self.probe.clone())
        }
    }

    fn complete_permission_probe() -> DiscordPermissionProbe {
        DiscordPermissionProbe {
            view_channel: DiscordProbeStatus::Passed,
            send_messages: DiscordProbeStatus::Passed,
            read_message_history: DiscordProbeStatus::Passed,
            attach_files: DiscordProbeStatus::Passed,
            history_readback: DiscordProbeStatus::Passed,
            effective_permission_bits: DISCORD_INSTALL_PERMISSION_BITS,
        }
    }

    fn setup_provider() -> MockSetupProvider {
        MockSetupProvider {
            identity: DiscordBotIdentity {
                bot_user_id: 33,
                application_id: 34,
                bot_name: "OpenOpen".into(),
            },
            probe: DiscordChannelProbe {
                guild_name: "Owner Guild".into(),
                channel_name: "approved-channel".into(),
                permissions: complete_permission_probe(),
            },
            probe_calls: Mutex::new(Vec::new()),
        }
    }

    fn setup_message(content: &str) -> DiscordSetupMessage {
        DiscordSetupMessage {
            source_message_id: 44,
            guild_id: Some(55),
            channel_id: 22,
            owner_user_id: 11,
            owner_is_bot: false,
            owner_name: "Owner".into(),
            mentioned_user_ids: vec![33],
            content: content.into(),
            received_at_ms: 66,
        }
    }

    fn pairing() -> DiscordPairing {
        DiscordPairing::new(11, 22, 33).unwrap()
    }

    fn message(content: &str) -> GatewayMessage {
        GatewayMessage {
            message_id: 44,
            channel_id: 22,
            author_id: 11,
            author_is_bot: false,
            mentioned_user_ids: vec![33],
            provider_nonce: None,
            content: content.to_owned(),
            received_at_ms: 55,
        }
    }

    #[test]
    fn bot_token_debug_is_redacted_and_empty_token_fails() {
        let token = BotToken::new("x").unwrap();
        assert_eq!(format!("{token:?}"), "BotToken(<redacted>)");
        assert!(matches!(
            BotToken::new("  "),
            Err(AdapterError::MissingBotToken)
        ));
    }

    #[test]
    fn official_install_url_has_only_the_exact_bot_scope_and_permissions() {
        assert_eq!(
            required_install_permissions().bits(),
            DISCORD_INSTALL_PERMISSION_BITS
        );
        assert_eq!(
            discord_install_url(34).unwrap(),
            "https://discord.com/api/oauth2/authorize?client_id=34&scope=bot&permissions=101376"
        );
        assert!(matches!(
            discord_install_url(0),
            Err(AdapterError::InvalidInstallParameters)
        ));
    }

    #[test]
    fn pairing_code_is_exact_lowercase_128_bit_hex() {
        assert!(validate_pairing_code(&"ab".repeat(16)).is_ok());
        for invalid in ["ab".repeat(15), "AB".repeat(16), "gg".repeat(16)] {
            assert!(matches!(
                validate_pairing_code(&invalid),
                Err(AdapterError::InvalidPairingCode)
            ));
        }
    }

    #[test]
    fn every_permission_and_real_history_readback_is_required() {
        let complete = complete_permission_probe();
        assert!(complete.complete());
        for incomplete in [
            DiscordPermissionProbe {
                view_channel: DiscordProbeStatus::Missing,
                ..complete.clone()
            },
            DiscordPermissionProbe {
                send_messages: DiscordProbeStatus::Missing,
                ..complete.clone()
            },
            DiscordPermissionProbe {
                read_message_history: DiscordProbeStatus::Missing,
                ..complete.clone()
            },
            DiscordPermissionProbe {
                attach_files: DiscordProbeStatus::Missing,
                ..complete.clone()
            },
            DiscordPermissionProbe {
                history_readback: DiscordProbeStatus::Missing,
                ..complete
            },
        ] {
            assert!(!incomplete.complete());
        }
    }

    #[tokio::test]
    async fn setup_candidate_binds_explicit_message_and_live_probe_without_model_envelope() {
        let provider = setup_provider();
        assert_eq!(
            provider.current_identity().await.unwrap(),
            provider.identity
        );
        let code = "ab".repeat(16);
        let candidate = pairing_candidate(
            &provider,
            &provider.identity,
            &code,
            &setup_message(&format!("<@33> pair {code}")),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(candidate.candidate_id.starts_with("discord-pair-"));
        assert_eq!(candidate.source_message_id, "44");
        assert_eq!(candidate.guild_id, "55");
        assert_eq!(candidate.channel_id, "22");
        assert_eq!(candidate.owner_user_id, "11");
        assert_eq!(candidate.bot_user_id, "33");
        assert_eq!(candidate.application_id, "34");
        assert!(candidate.message_content_intent_ready);
        assert!(candidate.permissions.complete());
        assert!(candidate.confirmable());
        assert_eq!(
            *provider
                .probe_calls
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
            vec![(55, 22, 33)]
        );
    }

    #[tokio::test]
    async fn setup_rejects_dm_bot_wrong_mention_and_wrong_code_before_provider_access() {
        let provider = setup_provider();
        let code = "ab".repeat(16);
        let mut values = Vec::new();
        let mut dm = setup_message(&format!("<@33> pair {code}"));
        dm.guild_id = None;
        values.push(dm);
        let mut bot = setup_message(&format!("<@33> pair {code}"));
        bot.owner_is_bot = true;
        values.push(bot);
        let mut unmentioned = setup_message(&format!("<@33> pair {code}"));
        unmentioned.mentioned_user_ids.clear();
        values.push(unmentioned);
        values.push(setup_message("<@33> pair 00000000000000000000000000000000"));
        for value in values {
            assert_eq!(
                pairing_candidate(&provider, &provider.identity, &code, &value)
                    .await
                    .unwrap(),
                None
            );
        }
        assert!(
            provider
                .probe_calls
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .is_empty()
        );
    }

    #[test]
    fn operational_pairing_rejects_bot_or_application_identity_drift() {
        let pairing = DiscordPairing::new_with_application(11, 22, 33, 34).unwrap();
        let exact = DiscordBotIdentity {
            bot_user_id: 33,
            application_id: 34,
            bot_name: "OpenOpen".into(),
        };
        assert!(pairing.matches_identity(&exact));
        assert!(!pairing.matches_identity(&DiscordBotIdentity {
            bot_user_id: 35,
            ..exact.clone()
        }));
        assert!(!pairing.matches_identity(&DiscordBotIdentity {
            application_id: 36,
            ..exact
        }));
    }

    #[test]
    fn pairing_is_exact_and_uses_least_privilege_intents() {
        let pairing = pairing();
        assert_eq!(
            pairing.gateway_intents(),
            GatewayIntents::GUILDS
                | GatewayIntents::GUILD_MESSAGES
                | GatewayIntents::MESSAGE_CONTENT
        );
        assert!(DiscordPairing::new(0, 22, 33).is_err());
        assert!(DiscordPairing::new(11, 0, 33).is_err());
        assert!(DiscordPairing::new(11, 22, 0).is_err());
    }

    #[test]
    fn inbound_requires_owner_channel_and_explicit_mention() {
        let pairing = pairing();
        let accepted = normalize_inbound(&pairing, &message("<@33> plan this")).unwrap();
        assert_eq!(
            accepted,
            Some(InboundEnvelope {
                source_message_id: "44".into(),
                sender_id: "11".into(),
                conversation_id: "22".into(),
                content: "plan this".into(),
                received_at_ms: 55,
            })
        );

        let mut unauthorized = message("<@33> plan this");
        unauthorized.author_id = 12;
        assert_eq!(normalize_inbound(&pairing, &unauthorized).unwrap(), None);
        unauthorized = message("<@33> plan this");
        unauthorized.channel_id = 23;
        assert_eq!(normalize_inbound(&pairing, &unauthorized).unwrap(), None);
        unauthorized = message("plan this");
        unauthorized.mentioned_user_ids.clear();
        assert_eq!(normalize_inbound(&pairing, &unauthorized).unwrap(), None);
    }

    #[test]
    fn bots_are_ignored_before_model_entry() {
        let mut value = message("<@33> plan this");
        value.author_is_bot = true;
        assert_eq!(normalize_inbound(&pairing(), &value).unwrap(), None);
    }

    #[test]
    fn exact_standard_and_nickname_mentions_are_stripped() {
        assert_eq!(strip_bot_mention(" <@33> approve <@!33> ", 33), "approve");
        assert_eq!(strip_bot_mention("<@330> keep", 33), "<@330> keep");
    }

    #[test]
    fn inbound_content_is_bounded_after_mention_stripping() {
        let mut value = message(&format!("<@33> {}", "x".repeat(2_001)));
        assert!(matches!(
            normalize_inbound(&pairing(), &value),
            Err(AdapterError::ContentTooLong {
                actual: 2_001,
                maximum: 2_000
            })
        ));
        value.content = "<@33>".into();
        assert!(matches!(
            normalize_inbound(&pairing(), &value),
            Err(AdapterError::EmptyAddressedContent)
        ));
    }

    #[test]
    fn outbound_is_exact_channel_bounded_and_disables_mentions() {
        let request = OutboundRequest {
            outbound_id: "outbound-1".into(),
            conversation_id: "22".into(),
            content: "OpenOpen · AI: Working on it <@11> @everyone".into(),
            reply_to_message_id: None,
        };
        assert_eq!(validate_outbound(&pairing(), &request).unwrap(), 22);
        let json = serde_json::to_value(build_message(&request)).unwrap();
        assert_eq!(json["allowed_mentions"]["parse"], serde_json::json!([]));
        assert_eq!(json["allowed_mentions"]["users"], serde_json::json!([]));
        assert_eq!(json["allowed_mentions"]["roles"], serde_json::json!([]));
        assert_eq!(json["allowed_mentions"]["replied_user"], false);
        assert_eq!(
            json["nonce"],
            outbound_nonce("outbound-1").unwrap().as_str()
        );
        assert_eq!(json["enforce_nonce"], true);

        let reply = OutboundRequest {
            reply_to_message_id: Some("44".into()),
            ..request.clone()
        };
        let reply_json = serde_json::to_value(build_message(&reply)).unwrap();
        assert_eq!(reply_json["message_reference"]["channel_id"], "22");
        assert_eq!(reply_json["message_reference"]["message_id"], "44");

        let mut wrong = request;
        wrong.conversation_id = "23".into();
        assert!(matches!(
            validate_outbound(&pairing(), &wrong),
            Err(AdapterError::UnauthorizedChannel)
        ));
    }

    #[test]
    fn outbound_nonce_is_stable_bounded_and_identity_specific() {
        let first = outbound_nonce("outbound-1").unwrap();
        assert_eq!(first, outbound_nonce("outbound-1").unwrap());
        assert_ne!(first, outbound_nonce("outbound-2").unwrap());
        assert_eq!(first.len(), 25);
        assert!(first.starts_with('o'));
        assert!(matches!(
            outbound_nonce(""),
            Err(AdapterError::InvalidOutboundId)
        ));
    }

    #[test]
    fn restart_recovery_sorts_accepted_messages_and_advances_raw_high_water() {
        let mut accepted = message("<@33> accepted");
        accepted.message_id = 101;
        let mut ignored_owner = message("<@33> ignored");
        ignored_owner.message_id = 102;
        ignored_owner.author_id = 12;
        let mut ignored_bot = message("<@33> ignored bot");
        ignored_bot.message_id = 103;
        ignored_bot.author_id = 33;
        ignored_bot.author_is_bot = true;
        let batch = recovery_batch_from_pages(
            &pairing(),
            100,
            vec![vec![ignored_bot, accepted, ignored_owner]],
            1,
        )
        .unwrap();
        assert_eq!(batch.envelopes.len(), 1);
        assert_eq!(batch.envelopes[0].source_message_id, "101");
        assert_eq!(batch.high_water_message_id, "103");
        assert_eq!(batch.pages_fetched, 1);
    }

    #[test]
    fn full_recovery_window_never_skips_a_cursor_gap() {
        let page = (101..=200)
            .rev()
            .map(|id| {
                let mut value = message("<@33> recovered");
                value.message_id = id;
                value
            })
            .collect::<Vec<_>>();
        assert!(matches!(
            recovery_batch_from_pages(&pairing(), 100, vec![page.clone()], 1),
            Err(AdapterError::RecoveryWindowExceeded)
        ));

        let mut cursor = message("old cursor");
        cursor.message_id = 100;
        cursor.mentioned_user_ids.clear();
        let batch =
            recovery_batch_from_pages(&pairing(), 100, vec![page, vec![cursor]], 2).unwrap();
        assert_eq!(batch.envelopes.len(), 100);
        assert_eq!(batch.envelopes[0].source_message_id, "101");
        assert_eq!(batch.envelopes[99].source_message_id, "200");
    }

    #[test]
    fn ambiguous_outbound_is_recovered_by_nonce_without_resend() {
        let expected_nonce = outbound_nonce("outbound-1").unwrap();
        let mut delivered = message("provider echo");
        delivered.message_id = 150;
        delivered.author_id = 33;
        delivered.author_is_bot = true;
        delivered.provider_nonce = Some(expected_nonce.clone());
        let recovered = recovered_delivery_from_pages(
            &pairing(),
            "outbound-1",
            100,
            vec![vec![delivered.clone()]],
            1,
        )
        .unwrap();
        assert_eq!(
            recovered,
            Some(OutboundDelivery {
                source_message_id: "150".into(),
                conversation_id: "22".into(),
            })
        );

        delivered.message_id = 151;
        assert!(matches!(
            recovered_delivery_from_pages(
                &pairing(),
                "outbound-1",
                100,
                vec![vec![
                    delivered,
                    GatewayMessage {
                        message_id: 150,
                        channel_id: 22,
                        author_id: 33,
                        author_is_bot: true,
                        mentioned_user_ids: Vec::new(),
                        provider_nonce: Some(expected_nonce),
                        content: String::new(),
                        received_at_ms: 55,
                    }
                ]],
                1,
            ),
            Err(AdapterError::ProviderNonceConflict)
        ));
    }

    #[test]
    fn lifecycle_generation_makes_off_fail_closed() {
        let lifecycle = Lifecycle::new();
        let first = lifecycle.begin_connect().unwrap();
        lifecycle.update(first, ConnectionStatus::Connected);
        assert!(lifecycle.permits(first));
        lifecycle.disable();
        assert!(!lifecycle.permits(first));
        assert_eq!(lifecycle.status(), ConnectionStatus::Stopping);
        lifecycle.disconnected();
        let second = lifecycle.begin_connect().unwrap();
        assert_ne!(first, second);
        assert!(!lifecycle.permits(first));
        assert!(lifecycle.permits(second));
        lifecycle.terminate(second, ConnectionStatus::Faulted);
        assert_eq!(lifecycle.status(), ConnectionStatus::Faulted);
        assert!(lifecycle.begin_connect().is_ok());
    }

    #[tokio::test]
    async fn adapter_starts_off_and_stop_invalidates_without_network() {
        let (adapter, _inbound) = DiscordAdapter::new(pairing(), 1).unwrap();
        assert_eq!(adapter.status(), ConnectionStatus::Disconnected);
        adapter.stop().await;
        assert_eq!(adapter.status(), ConnectionStatus::Disconnected);
        let request = OutboundRequest {
            outbound_id: "outbound-1".into(),
            conversation_id: "22".into(),
            content: "status".into(),
            reply_to_message_id: None,
        };
        assert!(matches!(
            adapter.send(&request).await,
            Err(AdapterError::Disabled)
        ));
    }
}
