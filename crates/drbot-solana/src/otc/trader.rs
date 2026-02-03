//! OTC trader client (A2A initiator).
//!
//! This helper can broadcast an RFQ to any registered A2A agents that advertise
//! the OTC capability, and then collect quotes.

use super::a2a::{otc_notification, parse_otc_envelope};
use super::protocol::{OTCEnvelope, OTCMessage, OTC_CAPABILITY};
use crate::{Result, SolanaError};
use drbot_a2a::{A2AHub, Agent, Capability, MessageType};
use solana_sdk::signer::keypair::Keypair;
use std::sync::Arc;
use tokio::time::{timeout, Duration, Instant};
use uuid::Uuid;

/// Simple OTC trader client that can send RFQs and collect quotes.
pub struct OtcTraderClient {
    pub agent: Agent,
    hub: Arc<A2AHub>,
}

impl OtcTraderClient {
    /// Create and register a new trader client.
    pub async fn new(hub: Arc<A2AHub>, name: &str) -> Self {
        let agent = Agent::new(name, "otc_trader").with_capability(Capability::new(
            OTC_CAPABILITY,
            "Solana OTC negotiation client",
        ));
        hub.register_agent(agent.clone()).await;
        Self { agent, hub }
    }

    /// Broadcast an RFQ to all desks (agents advertising the OTC capability) and collect quotes.
    pub async fn broadcast_rfq(
        &self,
        rfq: OTCMessage,
        max_quotes: usize,
        timeout_duration: Duration,
    ) -> Result<Vec<(Uuid, OTCEnvelope)>> {
        self.broadcast_rfq_inner(rfq, None, max_quotes, timeout_duration)
            .await
    }

    /// Broadcast a signed RFQ (signature is verified by desks in settlement mode).
    pub async fn broadcast_rfq_signed(
        &self,
        rfq: OTCMessage,
        signer: &Keypair,
        max_quotes: usize,
        timeout_duration: Duration,
    ) -> Result<Vec<(Uuid, OTCEnvelope)>> {
        self.broadcast_rfq_inner(rfq, Some(signer), max_quotes, timeout_duration)
            .await
    }

    async fn broadcast_rfq_inner(
        &self,
        rfq: OTCMessage,
        signer: Option<&Keypair>,
        max_quotes: usize,
        timeout_duration: Duration,
    ) -> Result<Vec<(Uuid, OTCEnvelope)>> {
        let require_signed_quotes = signer.is_some();

        let rfq_id = match &rfq {
            OTCMessage::Rfq { id, .. } => *id,
            _ => return Ok(vec![]),
        };

        // Discover desks
        let desks = self.hub.discover(OTC_CAPABILITY).await;

        // Subscribe before sending to avoid missing fast quotes.
        let mut rx = self.hub.subscribe();

        // Send RFQ to each desk
        let mut rfq_env = OTCEnvelope::new(self.agent.id.to_string(), rfq.clone());
        if let Some(signer) = signer {
            rfq_env = rfq_env
                .sign_with(signer)
                .map_err(|e| SolanaError::OTCError(e.to_string()))?;
        }

        for desk in desks.iter().filter(|a| a.id != self.agent.id) {
            let msg = otc_notification(self.agent.id, desk.id, rfq_env.clone());
            self.hub
                .send(msg)
                .await
                .map_err(|e| SolanaError::OTCError(e.to_string()))?;
        }

        // Collect quotes
        let mut quotes: Vec<(Uuid, OTCEnvelope)> = Vec::new();
        let deadline = Instant::now() + timeout_duration;

        loop {
            if quotes.len() >= max_quotes {
                break;
            }

            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }

            let msg = match timeout(remaining, rx.recv()).await {
                Ok(Ok(m)) => m,
                Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(_))) => continue,
                _ => break,
            };

            if msg.to != self.agent.id || msg.message_type != MessageType::Notification {
                continue;
            }

            let Some(envelope) = parse_otc_envelope(&msg) else {
                continue;
            };

            if matches!(
                envelope.message,
                OTCMessage::Quote { rfq_id: q_rfq, .. } if q_rfq == rfq_id
            ) {
                if require_signed_quotes && envelope.verify_signature().unwrap_or(false) == false {
                    continue;
                }
                quotes.push((msg.from, envelope));
            }
        }

        Ok(quotes)
    }
}
