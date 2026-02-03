//! OTC message protocol.
//!
//! Defines the message types for OTC negotiation between agents.

use chrono::{DateTime, Utc};
use crate::{Result, SolanaError};
use serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Signature;
use solana_sdk::signer::keypair::Keypair;
use solana_sdk::signer::Signer;
use std::str::FromStr;
use uuid::Uuid;

/// OTC negotiation messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OTCMessage {
    /// Request for Quote - initiates negotiation.
    Rfq {
        /// Unique RFQ identifier.
        id: Uuid,
        /// Asset to trade.
        asset: String,
        /// Asset mint address.
        asset_mint: Pubkey,
        /// Trade direction.
        direction: TradeDirection,
        /// Amount to trade (in smallest units).
        amount: u64,
        /// RFQ initiator's settlement wallet.
        initiator_wallet: Pubkey,
        /// Optional price limit.
        price_limit: Option<f64>,
        /// RFQ expiration.
        expires_at: DateTime<Utc>,
    },

    /// Quote response to an RFQ.
    Quote {
        /// Quote identifier.
        id: Uuid,
        /// Reference to the RFQ.
        rfq_id: Uuid,
        /// Quoted price per unit.
        price: f64,
        /// Valid quantity (may differ from RFQ).
        quantity: u64,
        /// Total settlement amount (in smallest units of `settlement_mint`).
        settlement_amount: u64,
        /// Quote expiration.
        valid_until: DateTime<Utc>,
        /// Settlement asset (what counterparty wants).
        settlement_asset: String,
        /// Settlement asset mint.
        settlement_mint: Pubkey,
        /// Quote maker's settlement wallet (counterparty).
        maker_wallet: Pubkey,
    },

    /// Counter-offer to a quote.
    CounterOffer {
        /// Counter-offer identifier.
        id: Uuid,
        /// Reference to the original quote.
        quote_id: Uuid,
        /// New proposed price.
        new_price: f64,
        /// Optional new quantity.
        new_quantity: Option<u64>,
        /// Counter-offer expiration.
        valid_until: DateTime<Utc>,
    },

    /// Accept a quote or counter-offer.
    Accept {
        /// Reference to the quote being accepted.
        quote_id: Uuid,
        /// Accepting party's wallet.
        accepting_wallet: Pubkey,
    },

    /// Reject a quote or counter-offer.
    Reject {
        /// Reference to the quote being rejected.
        quote_id: Uuid,
        /// Optional reason.
        reason: Option<String>,
    },

    /// Escrow funded notification.
    EscrowFunded {
        /// Negotiation identifier.
        negotiation_id: Uuid,
        /// Escrow account address.
        escrow_address: Pubkey,
        /// Funding transaction signature.
        signature: String,
        /// Party that funded.
        funded_by: EscrowParty,
        /// Wallet that reported this funding (must match envelope signature).
        reporting_wallet: Pubkey,
    },

    /// Trade settled notification.
    Settled {
        /// Negotiation identifier.
        negotiation_id: Uuid,
        /// Settlement transaction signature.
        signature: String,
        /// Final price.
        final_price: f64,
        /// Final quantity.
        final_quantity: u64,
        /// Wallet that reported this settlement (must match envelope signature).
        reporting_wallet: Pubkey,
    },

    /// Cancel negotiation.
    Cancel {
        /// Negotiation identifier.
        negotiation_id: Uuid,
        /// Reason for cancellation.
        reason: String,
    },
}

impl OTCMessage {
    /// Create a new RFQ message.
    pub fn rfq(
        asset: impl Into<String>,
        asset_mint: Pubkey,
        direction: TradeDirection,
        amount: u64,
        expires_in_secs: u64,
        initiator_wallet: Pubkey,
    ) -> Self {
        Self::Rfq {
            id: Uuid::new_v4(),
            asset: asset.into(),
            asset_mint,
            direction,
            amount,
            initiator_wallet,
            price_limit: None,
            expires_at: Utc::now() + chrono::Duration::seconds(expires_in_secs as i64),
        }
    }

    /// Create a quote response.
    pub fn quote(
        rfq_id: Uuid,
        price: f64,
        quantity: u64,
        settlement_amount: u64,
        settlement_asset: impl Into<String>,
        settlement_mint: Pubkey,
        maker_wallet: Pubkey,
        valid_secs: u64,
    ) -> Self {
        Self::Quote {
            id: Uuid::new_v4(),
            rfq_id,
            price,
            quantity,
            settlement_amount,
            valid_until: Utc::now() + chrono::Duration::seconds(valid_secs as i64),
            settlement_asset: settlement_asset.into(),
            settlement_mint,
            maker_wallet,
        }
    }

    /// Get the message type as string.
    pub fn message_type(&self) -> &'static str {
        match self {
            Self::Rfq { .. } => "rfq",
            Self::Quote { .. } => "quote",
            Self::CounterOffer { .. } => "counter_offer",
            Self::Accept { .. } => "accept",
            Self::Reject { .. } => "reject",
            Self::EscrowFunded { .. } => "escrow_funded",
            Self::Settled { .. } => "settled",
            Self::Cancel { .. } => "cancel",
        }
    }

    /// Wallet expected to sign this message type (if applicable).
    pub fn signer_wallet(&self) -> Option<Pubkey> {
        match self {
            Self::Rfq {
                initiator_wallet, ..
            } => Some(*initiator_wallet),
            Self::Quote { maker_wallet, .. } => Some(*maker_wallet),
            Self::Accept {
                accepting_wallet, ..
            } => Some(*accepting_wallet),
            Self::EscrowFunded {
                reporting_wallet, ..
            } => Some(*reporting_wallet),
            Self::Settled {
                reporting_wallet, ..
            } => Some(*reporting_wallet),
            _ => None,
        }
    }
}

/// Trade direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TradeDirection {
    /// Buying the asset.
    Buy,
    /// Selling the asset.
    Sell,
}

impl TradeDirection {
    /// Get the opposite direction.
    pub fn opposite(&self) -> Self {
        match self {
            Self::Buy => Self::Sell,
            Self::Sell => Self::Buy,
        }
    }
}

/// Party in an escrow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EscrowParty {
    /// Party A (RFQ initiator).
    PartyA,
    /// Party B (Quote provider).
    PartyB,
}

/// OTC capability identifier for A2A discovery.
pub const OTC_CAPABILITY: &str = "solana.otc.negotiate";

const OTC_ENVELOPE_DOMAIN: &str = "drbot:otc:envelope:v1";

/// OTC message envelope for A2A transport.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OTCEnvelope {
    /// Sender agent identifier.
    pub sender: String,
    /// Message content.
    pub message: OTCMessage,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
    /// Optional signature for authenticity.
    pub signature: Option<String>,
}

impl OTCEnvelope {
    /// Create a new envelope.
    pub fn new(sender: impl Into<String>, message: OTCMessage) -> Self {
        Self {
            sender: sender.into(),
            message,
            timestamp: Utc::now(),
            signature: None,
        }
    }

    /// Add a signature.
    pub fn with_signature(mut self, sig: impl Into<String>) -> Self {
        self.signature = Some(sig.into());
        self
    }

    fn signable_bytes(&self) -> Result<Vec<u8>> {
        #[derive(Serialize)]
        struct Signable {
            domain: &'static str,
            sender: String,
            timestamp: i64,
            message: SignableMessage,
        }

        #[derive(Serialize)]
        #[serde(tag = "type", rename_all = "snake_case")]
        enum SignableMessage {
            Rfq {
                id: String,
                asset: String,
                asset_mint: String,
                direction: &'static str,
                amount: u64,
                initiator_wallet: String,
                expires_at: i64,
            },
            Quote {
                id: String,
                rfq_id: String,
                quantity: u64,
                settlement_amount: u64,
                settlement_asset: String,
                settlement_mint: String,
                maker_wallet: String,
                valid_until: i64,
            },
            CounterOffer {
                id: String,
                quote_id: String,
                new_quantity: Option<u64>,
                valid_until: i64,
            },
            Accept {
                quote_id: String,
                accepting_wallet: String,
            },
            Reject {
                quote_id: String,
                reason: Option<String>,
            },
            EscrowFunded {
                negotiation_id: String,
                escrow_address: String,
                signature: String,
                funded_by: &'static str,
                reporting_wallet: String,
            },
            Settled {
                negotiation_id: String,
                signature: String,
                final_quantity: u64,
                reporting_wallet: String,
            },
            Cancel {
                negotiation_id: String,
                reason: String,
            },
        }

        fn direction_str(direction: TradeDirection) -> &'static str {
            match direction {
                TradeDirection::Buy => "buy",
                TradeDirection::Sell => "sell",
            }
        }

        fn party_str(party: EscrowParty) -> &'static str {
            match party {
                EscrowParty::PartyA => "party_a",
                EscrowParty::PartyB => "party_b",
            }
        }

        let msg = match &self.message {
            OTCMessage::Rfq {
                id,
                asset,
                asset_mint,
                direction,
                amount,
                initiator_wallet,
                expires_at,
                ..
            } => SignableMessage::Rfq {
                id: id.to_string(),
                asset: asset.clone(),
                asset_mint: asset_mint.to_string(),
                direction: direction_str(*direction),
                amount: *amount,
                initiator_wallet: initiator_wallet.to_string(),
                expires_at: expires_at.timestamp(),
            },
            OTCMessage::Quote {
                id,
                rfq_id,
                quantity,
                settlement_amount,
                settlement_asset,
                settlement_mint,
                maker_wallet,
                valid_until,
                ..
            } => SignableMessage::Quote {
                id: id.to_string(),
                rfq_id: rfq_id.to_string(),
                quantity: *quantity,
                settlement_amount: *settlement_amount,
                settlement_asset: settlement_asset.clone(),
                settlement_mint: settlement_mint.to_string(),
                maker_wallet: maker_wallet.to_string(),
                valid_until: valid_until.timestamp(),
            },
            OTCMessage::CounterOffer {
                id,
                quote_id,
                new_quantity,
                valid_until,
                ..
            } => SignableMessage::CounterOffer {
                id: id.to_string(),
                quote_id: quote_id.to_string(),
                new_quantity: *new_quantity,
                valid_until: valid_until.timestamp(),
            },
            OTCMessage::Accept {
                quote_id,
                accepting_wallet,
            } => SignableMessage::Accept {
                quote_id: quote_id.to_string(),
                accepting_wallet: accepting_wallet.to_string(),
            },
            OTCMessage::Reject { quote_id, reason } => SignableMessage::Reject {
                quote_id: quote_id.to_string(),
                reason: reason.clone(),
            },
            OTCMessage::EscrowFunded {
                negotiation_id,
                escrow_address,
                signature,
                funded_by,
                reporting_wallet,
            } => SignableMessage::EscrowFunded {
                negotiation_id: negotiation_id.to_string(),
                escrow_address: escrow_address.to_string(),
                signature: signature.clone(),
                funded_by: party_str(*funded_by),
                reporting_wallet: reporting_wallet.to_string(),
            },
            OTCMessage::Settled {
                negotiation_id,
                signature,
                final_quantity,
                reporting_wallet,
                ..
            } => SignableMessage::Settled {
                negotiation_id: negotiation_id.to_string(),
                signature: signature.clone(),
                final_quantity: *final_quantity,
                reporting_wallet: reporting_wallet.to_string(),
            },
            OTCMessage::Cancel {
                negotiation_id,
                reason,
            } => SignableMessage::Cancel {
                negotiation_id: negotiation_id.to_string(),
                reason: reason.clone(),
            },
        };

        let bytes = serde_json::to_vec(&Signable {
            domain: OTC_ENVELOPE_DOMAIN,
            sender: self.sender.clone(),
            timestamp: self.timestamp.timestamp(),
            message: msg,
        })?;

        Ok(bytes)
    }

    /// Sign the envelope using a Solana `Keypair`.
    ///
    /// The signature covers `{ domain, sender, timestamp, message }` where `message` is a stable,
    /// canonical subset of the OTC message fields (not the signature field itself).
    pub fn sign_with(mut self, signer: &Keypair) -> Result<Self> {
        if self.message.signer_wallet() != Some(signer.pubkey()) {
            return Err(SolanaError::OTCError(
                "Signer pubkey does not match message wallet".to_string(),
            ));
        }

        let bytes = self.signable_bytes()?;
        let sig = signer.sign_message(&bytes);
        self.signature = Some(sig.to_string());
        Ok(self)
    }

    /// Verify the envelope signature against the wallet implied by the message.
    ///
    /// Returns `Ok(false)` if the message type does not have a signing wallet or if no signature is present.
    pub fn verify_signature(&self) -> Result<bool> {
        let Some(sig_str) = self.signature.as_ref() else {
            return Ok(false);
        };

        let Some(wallet) = self.message.signer_wallet() else {
            return Ok(false);
        };

        let sig = Signature::from_str(sig_str).map_err(|e| {
            SolanaError::OTCError(format!("Invalid signature encoding: {e}"))
        })?;
        let bytes = self.signable_bytes()?;

        Ok(sig.verify(wallet.as_ref(), &bytes))
    }
}

/// RFQ broadcast parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RfqBroadcast {
    /// RFQ message.
    pub rfq: OTCMessage,
    /// Minimum counterparty reputation score.
    pub min_reputation: Option<u32>,
    /// Preferred settlement assets.
    pub preferred_settlement: Vec<String>,
    /// Maximum quotes to receive.
    pub max_quotes: usize,
}

impl RfqBroadcast {
    /// Create a new RFQ broadcast.
    pub fn new(rfq: OTCMessage) -> Self {
        Self {
            rfq,
            min_reputation: None,
            preferred_settlement: vec!["USDC".to_string(), "SOL".to_string()],
            max_quotes: 10,
        }
    }

    /// Set minimum reputation.
    pub fn with_min_reputation(mut self, rep: u32) -> Self {
        self.min_reputation = Some(rep);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rfq_message() {
        let rfq = OTCMessage::rfq(
            "SOL",
            Pubkey::new_unique(),
            TradeDirection::Buy,
            1_000_000_000,
            300,
            Pubkey::new_unique(),
        );

        assert_eq!(rfq.message_type(), "rfq");

        if let OTCMessage::Rfq {
            direction, amount, ..
        } = rfq
        {
            assert_eq!(direction, TradeDirection::Buy);
            assert_eq!(amount, 1_000_000_000);
        }
    }

    #[test]
    fn test_quote_message() {
        let rfq_id = Uuid::new_v4();
        let quote = OTCMessage::quote(
            rfq_id,
            100.5,
            1_000_000_000,
            100_500_000,
            "USDC",
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            60,
        );

        assert_eq!(quote.message_type(), "quote");
    }

    #[test]
    fn test_trade_direction() {
        assert_eq!(TradeDirection::Buy.opposite(), TradeDirection::Sell);
        assert_eq!(TradeDirection::Sell.opposite(), TradeDirection::Buy);
    }

    #[test]
    fn test_envelope() {
        let msg = OTCMessage::rfq(
            "SOL",
            Pubkey::new_unique(),
            TradeDirection::Sell,
            500_000_000,
            120,
            Pubkey::new_unique(),
        );

        let envelope = OTCEnvelope::new("agent-123", msg);
        assert_eq!(envelope.sender, "agent-123");
        assert!(envelope.signature.is_none());
    }

    #[test]
    fn test_envelope_sign_and_verify() {
        let keypair = Keypair::new();
        let wallet = keypair.pubkey();

        let msg = OTCMessage::rfq(
            "SOL",
            Pubkey::new_unique(),
            TradeDirection::Buy,
            1_000_000_000,
            300,
            wallet,
        );

        let env = OTCEnvelope::new("trader-1", msg)
            .sign_with(&keypair)
            .unwrap();
        assert!(env.verify_signature().unwrap());

        let mut tampered = env.clone();
        if let OTCMessage::Rfq { amount, .. } = &mut tampered.message {
            *amount += 1;
        }
        assert!(!tampered.verify_signature().unwrap());
    }
}
