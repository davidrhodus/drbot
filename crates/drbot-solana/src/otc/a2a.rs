//! A2A transport glue for OTC negotiation.
//!
//! This module bridges `drbot-a2a` messages with the Solana OTC protocol envelope.

use super::protocol::{OTCEnvelope, OTC_CAPABILITY};
use drbot_a2a::{A2AMessage, MessageType};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Payload wrapper used for A2A OTC notifications.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OtcA2aPayload {
    pub capability: String,
    pub envelope: OTCEnvelope,
}

impl OtcA2aPayload {
    pub fn new(envelope: OTCEnvelope) -> Self {
        Self {
            capability: OTC_CAPABILITY.to_string(),
            envelope,
        }
    }
}

/// Create an A2A notification containing an OTC envelope.
pub fn otc_notification(from: Uuid, to: Uuid, envelope: OTCEnvelope) -> A2AMessage {
    A2AMessage::new(
        from,
        to,
        MessageType::Notification,
        serde_json::to_value(OtcA2aPayload::new(envelope)).unwrap_or_else(
            |_| serde_json::json!({"capability": OTC_CAPABILITY, "error": "serialize_failed"}),
        ),
    )
}

/// Try to parse an OTC envelope from an A2A message.
pub fn parse_otc_envelope(message: &A2AMessage) -> Option<OTCEnvelope> {
    let payload: OtcA2aPayload = serde_json::from_value(message.payload.clone()).ok()?;
    if payload.capability != OTC_CAPABILITY {
        return None;
    }
    // Bind the envelope `sender` to the A2A `from` field to prevent spoofing.
    if payload.envelope.sender != message.from.to_string() {
        return None;
    }
    Some(payload.envelope)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::otc::protocol::{OTCEnvelope, OTCMessage, TradeDirection};
    use solana_sdk::pubkey::Pubkey;

    #[test]
    fn test_parse_otc_envelope_requires_sender_binding() {
        let from = Uuid::new_v4();
        let to = Uuid::new_v4();

        let rfq = OTCMessage::rfq(
            "SOL",
            Pubkey::new_unique(),
            TradeDirection::Buy,
            1_000,
            60,
            Pubkey::new_unique(),
        );

        let good_env = OTCEnvelope::new(from.to_string(), rfq.clone());
        let good_msg = otc_notification(from, to, good_env);
        assert!(parse_otc_envelope(&good_msg).is_some());

        let bad_env = OTCEnvelope::new("not-the-sender", rfq);
        let bad_msg = otc_notification(from, to, bad_env);
        assert!(parse_otc_envelope(&bad_msg).is_none());
    }
}
