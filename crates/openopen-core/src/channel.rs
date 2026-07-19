use crate::mission::is_canonical_mission_id;
use openopen_protocol::{
    ChannelCursor, ChannelDeliveryReceipt, ChannelEnvelope, ChannelKind, ChannelMissionEvent,
    ChannelObservation, ChannelOutboundIntent, ChannelPairing, ChannelRoute, ChannelRouteApproval,
    ChannelRouteRole, ChannelRouteSet, NeedsMe, Receipt,
};
use std::collections::HashSet;
use thiserror::Error;

const MAX_PROVIDER_IDENTIFIER_BYTES: usize = 512;

#[must_use]
pub fn channel_need_you_content(needs_me: &NeedsMe) -> String {
    format!("Need you: {}", needs_me.prompt)
}

#[must_use]
pub fn channel_receipt_content(receipt: &Receipt) -> String {
    let count = receipt.evidence_ids.len();
    format!(
        "Done: {}\nEvidence: {count} verified completion{}\nModel: {}",
        receipt.summary,
        if count == 1 { "" } else { "s" },
        receipt.actual_model
    )
}

#[must_use]
pub fn channel_message_payload(channel: ChannelKind, content: &str) -> Vec<u8> {
    match channel {
        ChannelKind::IMessage => format!("OpenOpen · AI\n{content}").into_bytes(),
        ChannelKind::Discord => content.as_bytes().to_vec(),
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum ChannelError {
    #[error("channel record is malformed")]
    InvalidRecord,
}

pub(crate) fn validate_pairing(pairing: &ChannelPairing) -> Result<(), ChannelError> {
    if !valid_provider_identifier(&pairing.owner_sender_id)
        || !valid_provider_identifier(&pairing.conversation_id)
        || !pairing.require_explicit_address
        || pairing.paired_at_ms < 0
        || match (pairing.channel, pairing.discord.as_ref()) {
            (ChannelKind::IMessage, None) => false,
            (ChannelKind::Discord, Some(discord)) => {
                !valid_discord_snowflake(&discord.guild_id)
                    || !valid_discord_snowflake(&discord.bot_user_id)
                    || !valid_discord_snowflake(&discord.application_id)
                    || !valid_discord_snowflake(&discord.setup_source_message_id)
                    || !discord.setup_candidate_id.starts_with("discord-pair-")
                    || !is_lower_hex(&discord.setup_candidate_id[13..], 64)
            }
            _ => true,
        }
    {
        return Err(ChannelError::InvalidRecord);
    }
    Ok(())
}

pub(crate) fn validate_envelope(envelope: &ChannelEnvelope) -> Result<(), ChannelError> {
    if !valid_provider_identifier(&envelope.source_message_id)
        || !valid_provider_identifier(&envelope.sender_id)
        || !valid_provider_identifier(&envelope.conversation_id)
        || !is_lower_hex(&envelope.content_sha256, 64)
        || envelope.received_at_ms < 0
    {
        return Err(ChannelError::InvalidRecord);
    }
    Ok(())
}

pub(crate) fn validate_cursor(cursor: &ChannelCursor) -> Result<(), ChannelError> {
    if !valid_provider_identifier(&cursor.conversation_id)
        || !valid_provider_identifier(&cursor.opaque_value)
        || cursor.observed_at_ms < 0
    {
        return Err(ChannelError::InvalidRecord);
    }
    Ok(())
}

pub(crate) fn validate_observation(observation: &ChannelObservation) -> Result<(), ChannelError> {
    validate_envelope(&observation.envelope)?;
    validate_cursor(&observation.cursor)?;
    if observation.envelope.channel != observation.cursor.channel
        || observation.envelope.conversation_id != observation.cursor.conversation_id
        || observation.envelope.received_at_ms > observation.cursor.observed_at_ms
    {
        return Err(ChannelError::InvalidRecord);
    }
    Ok(())
}

pub(crate) fn validate_route(route: &ChannelRoute) -> Result<(), ChannelError> {
    if !is_canonical_effect_identifier(&route.route_id)
        || !valid_provider_identifier(&route.conversation_id)
        || !valid_provider_identifier(&route.owner_sender_id)
        || route
            .provider_identity
            .as_deref()
            .is_some_and(|value| !valid_provider_identifier(value))
        || route
            .source_message_id
            .as_deref()
            .is_some_and(|value| !valid_provider_identifier(value))
        || route.allowed_inbound_classes.is_empty()
        || !strictly_sorted(&route.allowed_inbound_classes)
        || !strictly_sorted(&route.allowed_outbound_classes)
        || route.revision == 0
        || !is_canonical_effect_identifier(&route.approval_id)
        || !is_canonical_effect_identifier(&route.audit_id)
        || route.bound_at_ms < 0
        || route.updated_at_ms < route.bound_at_ms
        || (route.role == ChannelRouteRole::Primary && route.source_message_id.is_none())
    {
        return Err(ChannelError::InvalidRecord);
    }
    Ok(())
}

pub(crate) fn validate_route_set(route_set: &ChannelRouteSet) -> Result<(), ChannelError> {
    if !is_canonical_mission_id(&route_set.mission_id)
        || route_set.revision == 0
        || route_set.routes.is_empty()
        || route_set.routes.len() > 8
        || route_set.routes[0].role != ChannelRouteRole::Primary
        || route_set.routes[0].route_id != route_set.primary_route_id
        || route_set
            .routes
            .iter()
            .filter(|route| route.role == ChannelRouteRole::Primary)
            .count()
            != 1
        || route_set
            .routes
            .iter()
            .any(|route| validate_route(route).is_err() || route.revision > route_set.revision)
    {
        return Err(ChannelError::InvalidRecord);
    }
    let mut route_ids = HashSet::new();
    let mut boundaries = HashSet::new();
    if route_set.routes.iter().any(|route| {
        !route_ids.insert(route.route_id.as_str())
            || !boundaries.insert((
                route.channel,
                route.conversation_id.as_str(),
                route.owner_sender_id.as_str(),
            ))
    }) {
        return Err(ChannelError::InvalidRecord);
    }
    Ok(())
}

pub(crate) fn validate_route_approval(approval: &ChannelRouteApproval) -> Result<(), ChannelError> {
    if !is_canonical_effect_identifier(&approval.approval_id)
        || !is_canonical_mission_id(&approval.mission_id)
        || approval.expected_route_set_revision == 0
        || !valid_provider_identifier(&approval.conversation_id)
        || !valid_provider_identifier(&approval.owner_sender_id)
        || approval
            .provider_identity
            .as_deref()
            .is_some_and(|value| !valid_provider_identifier(value))
        || approval.allowed_inbound_classes.is_empty()
        || !strictly_sorted(&approval.allowed_inbound_classes)
        || !strictly_sorted(&approval.allowed_outbound_classes)
        || !valid_provider_identifier(&approval.actor_id)
        || approval.decided_at_ms < 0
    {
        return Err(ChannelError::InvalidRecord);
    }
    Ok(())
}

pub(crate) fn validate_mission_event(event: &ChannelMissionEvent) -> Result<(), ChannelError> {
    if !is_canonical_effect_identifier(&event.event_id)
        || !is_canonical_mission_id(&event.mission_id)
        || event.mission_revision <= 0
        || !is_lower_hex(&event.mission_anchor_hash, 64)
        || !is_canonical_effect_identifier(&event.route_id)
        || event.route_set_revision == 0
        || !valid_provider_identifier(&event.source_message_id)
        || !is_lower_hex(&event.content_sha256, 64)
        || event.recorded_at_ms < 0
    {
        return Err(ChannelError::InvalidRecord);
    }
    Ok(())
}

pub(crate) fn validate_outbound(intent: &ChannelOutboundIntent) -> Result<(), ChannelError> {
    if !is_canonical_effect_identifier(&intent.outbound_id)
        || !is_canonical_mission_id(&intent.mission_id)
        || !is_canonical_effect_identifier(&intent.route_id)
        || intent.route_set_revision == 0
        || !valid_provider_identifier(&intent.conversation_id)
        || !valid_provider_identifier(&intent.recipient_id)
        || !is_lower_hex(&intent.content_sha256, 64)
        || intent.created_at_ms < 0
        || intent.recovery_cursor.as_ref().is_some_and(|cursor| {
            validate_cursor(cursor).is_err()
                || cursor.channel != intent.channel
                || cursor.conversation_id != intent.conversation_id
                || cursor.observed_at_ms > intent.created_at_ms
        })
    {
        return Err(ChannelError::InvalidRecord);
    }
    Ok(())
}

fn strictly_sorted<T: Ord>(values: &[T]) -> bool {
    values.windows(2).all(|pair| pair[0] < pair[1])
}

pub(crate) fn validate_delivery(receipt: &ChannelDeliveryReceipt) -> Result<(), ChannelError> {
    if !is_canonical_effect_identifier(&receipt.outbound_id)
        || !valid_provider_identifier(&receipt.provider_message_id)
        || receipt.delivered_at_ms < 0
    {
        return Err(ChannelError::InvalidRecord);
    }
    Ok(())
}

fn valid_provider_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.trim() == value
        && value.len() <= MAX_PROVIDER_IDENTIFIER_BYTES
        && !value
            .bytes()
            .any(|byte| byte == 0 || byte.is_ascii_control())
}

fn valid_discord_snowflake(value: &str) -> bool {
    value.parse::<u64>().is_ok_and(|value| value != 0)
}

fn is_canonical_effect_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
        })
}

fn is_lower_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
mod tests {
    use super::*;
    use openopen_protocol::{ChannelKind, ChannelMessageKind};

    #[test]
    fn v1_pairing_requires_explicit_addressing() {
        let pairing = ChannelPairing {
            channel: ChannelKind::Discord,
            owner_sender_id: "owner".into(),
            conversation_id: "channel".into(),
            require_explicit_address: false,
            discord: None,
            paired_at_ms: 1,
        };
        assert_eq!(validate_pairing(&pairing), Err(ChannelError::InvalidRecord));
    }

    #[test]
    fn outbound_ids_and_hashes_are_canonical() {
        let intent = ChannelOutboundIntent {
            outbound_id: "receipt-1".into(),
            mission_id: "mission-1".into(),
            route_id: "channel-route-1".into(),
            route_set_revision: 1,
            channel: ChannelKind::IMessage,
            conversation_id: "chat-1".into(),
            recipient_id: "owner-1".into(),
            kind: ChannelMessageKind::Receipt,
            content_sha256: "a".repeat(64),
            created_at_ms: 1,
            recovery_cursor: None,
        };
        assert_eq!(validate_outbound(&intent), Ok(()));
    }
}
