//! OTC negotiation state machine.
//!
//! Manages the lifecycle of OTC negotiations between agents.

use super::escrow::EscrowManager;
use super::protocol::{EscrowParty, OTCEnvelope, OTCMessage, TradeDirection};
use crate::{Result, SolanaError};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::program_pack::Pack;
use solana_sdk::pubkey::Pubkey;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::sync::OnceLock;
use tokio::sync::Notify;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tracing::{debug, info, warn};
use uuid::Uuid;

/// OTC negotiation manager.
pub struct OTCNegotiationManager {
    rpc_client: Arc<RpcClient>,
    escrow_manager: EscrowManager,
    negotiations: Arc<RwLock<HashMap<Uuid, Negotiation>>>,
    our_wallet: Option<Pubkey>,
    persistence: OnceLock<NegotiationPersistence>,
}

#[derive(Clone)]
struct NegotiationPersistence {
    path: PathBuf,
    dirty: Arc<AtomicBool>,
    notify: Arc<Notify>,
}

/// RFQ + Quote context for building escrow terms.
#[derive(Debug, Clone)]
pub struct QuoteContext {
    pub negotiation_id: Uuid,
    pub rfq: OTCMessage,
    pub quote: OTCMessage,
}

impl OTCNegotiationManager {
    /// Create a new negotiation manager.
    pub fn new(rpc_client: Arc<RpcClient>) -> Self {
        Self {
            rpc_client: rpc_client.clone(),
            escrow_manager: EscrowManager::new(rpc_client),
            negotiations: Arc::new(RwLock::new(HashMap::new())),
            our_wallet: None,
            persistence: OnceLock::new(),
        }
    }

    /// Set our wallet for negotiations.
    pub fn with_wallet(mut self, wallet: Pubkey) -> Self {
        self.our_wallet = Some(wallet);
        self
    }

    /// Enable on-disk persistence for negotiations (crash/restart safety).
    ///
    /// - Loads existing state from `path` (if it exists)
    /// - Spawns an autosave task that flushes whenever state changes
    pub async fn enable_persistence(
        self: &Arc<Self>,
        path: impl Into<PathBuf>,
        flush_interval: std::time::Duration,
    ) -> Result<JoinHandle<()>> {
        let path = path.into();

        let persistence = NegotiationPersistence {
            path: path.clone(),
            dirty: Arc::new(AtomicBool::new(false)),
            notify: Arc::new(Notify::new()),
        };

        if self.persistence.set(persistence).is_err() {
            return Err(SolanaError::ConfigError(
                "Negotiation persistence already enabled".to_string(),
            ));
        }

        // Best-effort load.
        let _ = self.load_from_file(&path).await?;

        let manager = self.clone();
        Ok(tokio::spawn(async move {
            manager.run_autosave(flush_interval).await;
        }))
    }

    /// Best-effort load. Returns `Ok(true)` if a file was loaded, `Ok(false)` if missing.
    pub async fn load_from_file(&self, path: &Path) -> Result<bool> {
        let bytes = match tokio::fs::read(path).await {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(e) => return Err(e.into()),
        };

        let file: NegotiationStoreFile = serde_json::from_slice(&bytes)?;
        if file.version != NegotiationStoreFile::VERSION {
            return Err(SolanaError::OTCError(format!(
                "Unsupported negotiation store version {}",
                file.version
            )));
        }

        let mut map = HashMap::new();
        for n in file.negotiations {
            map.insert(n.id, n);
        }

        *self.negotiations.write().await = map;
        Ok(true)
    }

    pub async fn save_to_file(&self, path: &Path) -> Result<()> {
        let negotiations = self.negotiations.read().await;
        let mut list: Vec<Negotiation> = negotiations.values().cloned().collect();
        list.sort_by_key(|n| n.id);

        let file = NegotiationStoreFile {
            version: NegotiationStoreFile::VERSION,
            negotiations: list,
        };

        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let tmp_path = temp_path_for(path);
        tokio::fs::write(&tmp_path, serde_json::to_vec_pretty(&file)?).await?;
        if tokio::fs::rename(&tmp_path, path).await.is_err() {
            let _ = tokio::fs::remove_file(path).await;
            tokio::fs::rename(&tmp_path, path).await?;
        }
        Ok(())
    }

    /// Record that an escrow address was created/derived for this negotiation.
    pub async fn record_escrow_created(&self, negotiation_id: Uuid, escrow_address: Pubkey) {
        let mut negotiations = self.negotiations.write().await;
        let Some(neg) = negotiations.get_mut(&negotiation_id) else {
            return;
        };

        neg.escrow_address = Some(escrow_address);
        neg.updated_at = Utc::now();
        neg.history
            .push(NegotiationEvent::EscrowCreated { escrow_address });
        self.mark_dirty();
    }

    /// Record that a funding tx occurred (local observation).
    pub async fn record_escrow_funded_local(
        &self,
        negotiation_id: Uuid,
        party: EscrowParty,
        signature: String,
    ) {
        let mut negotiations = self.negotiations.write().await;
        let Some(neg) = negotiations.get_mut(&negotiation_id) else {
            return;
        };

        neg.state = NegotiationState::EscrowFunding;
        neg.updated_at = Utc::now();
        neg.history
            .push(NegotiationEvent::EscrowFunded { party, signature });
        self.mark_dirty();
    }

    fn mark_dirty(&self) {
        let Some(p) = self.persistence.get() else {
            return;
        };
        p.dirty.store(true, Ordering::Release);
        p.notify.notify_one();
    }

    async fn run_autosave(&self, flush_interval: std::time::Duration) {
        let Some(p) = self.persistence.get() else {
            return;
        };

        let mut tick = tokio::time::interval(flush_interval);
        loop {
            tokio::select! {
                _ = tick.tick() => {}
                _ = p.notify.notified() => {}
            }

            if !p.dirty.swap(false, Ordering::AcqRel) {
                continue;
            }

            if let Err(e) = self.save_to_file(&p.path).await {
                // Keep dirty so we retry next tick.
                p.dirty.store(true, Ordering::Release);
                tracing::warn!(error = %e, path = %p.path.display(), "Failed to persist OTC negotiations");
            }
        }
    }

    /// Handle an incoming OTC message.
    pub async fn handle_message(&self, envelope: OTCEnvelope) -> Result<Option<OTCMessage>> {
        match &envelope.message {
            OTCMessage::Rfq { id, .. } => self.handle_rfq(&envelope).await,
            OTCMessage::Quote { rfq_id, .. } => self.handle_quote(&envelope).await,
            OTCMessage::CounterOffer { quote_id, .. } => self.handle_counter_offer(&envelope).await,
            OTCMessage::Accept { quote_id, .. } => self.handle_accept(&envelope).await,
            OTCMessage::Reject { quote_id, .. } => self.handle_reject(&envelope).await,
            OTCMessage::EscrowFunded { negotiation_id, .. } => {
                self.handle_escrow_funded(&envelope).await
            }
            OTCMessage::Settled { negotiation_id, .. } => self.handle_settled(&envelope).await,
            OTCMessage::Cancel { negotiation_id, .. } => self.handle_cancel(&envelope).await,
        }
    }

    /// Create and broadcast an RFQ.
    pub async fn create_rfq(
        &self,
        asset: impl Into<String>,
        asset_mint: Pubkey,
        direction: TradeDirection,
        amount: u64,
    ) -> Result<Negotiation> {
        let rfq = OTCMessage::rfq(
            asset.into(),
            asset_mint,
            direction,
            amount,
            300,
            self.our_wallet.unwrap_or_default(),
        );

        let rfq_id = match &rfq {
            OTCMessage::Rfq { id, .. } => *id,
            _ => unreachable!(),
        };

        let negotiation = Negotiation {
            id: rfq_id,
            state: NegotiationState::RfqSent,
            our_role: NegotiationRole::Initiator,
            counterparty: None,
            rfq: Some(rfq),
            quotes: Vec::new(),
            selected_quote: None,
            escrow_address: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            history: vec![NegotiationEvent::RfqCreated { id: rfq_id }],
        };

        self.negotiations
            .write()
            .await
            .insert(rfq_id, negotiation.clone());
        self.mark_dirty();

        info!(id = %rfq_id, "Created RFQ");

        Ok(negotiation)
    }

    /// Respond to an RFQ with a quote.
    pub async fn create_quote(
        &self,
        rfq_id: Uuid,
        price: f64,
        quantity: u64,
        settlement_asset: impl Into<String>,
        settlement_mint: Pubkey,
        valid_secs: u64,
    ) -> Result<OTCMessage> {
        let maker_wallet = self.our_wallet.unwrap_or_default();

        let (asset_mint, maker_receives_settlement) = {
            let negotiations = self.negotiations.read().await;
            let rfq = negotiations
                .get(&rfq_id)
                .and_then(|n| n.rfq.as_ref())
                .and_then(|m| match m {
                    OTCMessage::Rfq {
                        asset_mint,
                        direction,
                        ..
                    } => Some((*asset_mint, *direction == TradeDirection::Buy)),
                    _ => None,
                })
                .unwrap_or_default();
            rfq
        };

        let asset_decimals = self.best_effort_decimals(&asset_mint).await.unwrap_or(9);
        let settlement_decimals = self
            .best_effort_decimals(&settlement_mint)
            .await
            .unwrap_or(6);
        let settlement_amount = compute_settlement_amount(
            price,
            quantity,
            asset_decimals,
            settlement_decimals,
            maker_receives_settlement,
        )?;

        let quote = OTCMessage::quote(
            rfq_id,
            price,
            quantity,
            settlement_amount,
            settlement_asset.into(),
            settlement_mint,
            maker_wallet,
            valid_secs,
        );

        let quote_id = match &quote {
            OTCMessage::Quote { id, .. } => *id,
            _ => unreachable!(),
        };

        // Create or update negotiation
        let mut negotiations = self.negotiations.write().await;

        if let Some(neg) = negotiations.get_mut(&rfq_id) {
            neg.quotes.push(quote.clone());
            neg.updated_at = Utc::now();
            neg.history.push(NegotiationEvent::QuoteSent { quote_id });
        } else {
            // We're responding to someone else's RFQ
            let negotiation = Negotiation {
                id: rfq_id,
                state: NegotiationState::QuoteSent,
                our_role: NegotiationRole::Responder,
                counterparty: None,
                rfq: None,
                quotes: vec![quote.clone()],
                selected_quote: None,
                escrow_address: None,
                created_at: Utc::now(),
                updated_at: Utc::now(),
                history: vec![NegotiationEvent::QuoteSent { quote_id }],
            };
            negotiations.insert(rfq_id, negotiation);
        }
        drop(negotiations);
        self.mark_dirty();

        info!(rfq_id = %rfq_id, quote_id = %quote_id, price = price, "Created quote");

        Ok(quote)
    }

    /// Accept a quote.
    pub async fn accept_quote(&self, quote_id: Uuid) -> Result<OTCMessage> {
        let wallet = self
            .our_wallet
            .ok_or_else(|| SolanaError::ConfigError("Wallet not configured for OTC".to_string()))?;

        let accept = OTCMessage::Accept {
            quote_id,
            accepting_wallet: wallet,
        };

        // Find and update the negotiation
        let mut negotiations = self.negotiations.write().await;

        for (_, neg) in negotiations.iter_mut() {
            if neg.quotes.iter().any(|q| match q {
                OTCMessage::Quote { id, .. } => *id == quote_id,
                _ => false,
            }) {
                neg.state = NegotiationState::Accepted;
                neg.selected_quote = Some(quote_id);
                neg.updated_at = Utc::now();
                neg.history
                    .push(NegotiationEvent::QuoteAccepted { quote_id });

                info!(quote_id = %quote_id, "Accepted quote");
                break;
            }
        }
        drop(negotiations);
        self.mark_dirty();

        Ok(accept)
    }

    /// Reject a quote.
    pub async fn reject_quote(&self, quote_id: Uuid, reason: Option<String>) -> Result<OTCMessage> {
        let reject = OTCMessage::Reject { quote_id, reason };

        // Update negotiation
        let mut negotiations = self.negotiations.write().await;

        for (_, neg) in negotiations.iter_mut() {
            if neg.quotes.iter().any(|q| match q {
                OTCMessage::Quote { id, .. } => *id == quote_id,
                _ => false,
            }) {
                neg.history
                    .push(NegotiationEvent::QuoteRejected { quote_id });
                neg.updated_at = Utc::now();
                break;
            }
        }
        drop(negotiations);
        self.mark_dirty();

        info!(quote_id = %quote_id, "Rejected quote");

        Ok(reject)
    }

    /// Get a negotiation by ID.
    pub async fn get_negotiation(&self, id: Uuid) -> Option<Negotiation> {
        self.negotiations.read().await.get(&id).cloned()
    }

    /// Find the RFQ + Quote for a given quote id.
    pub async fn quote_context(&self, quote_id: Uuid) -> Result<QuoteContext> {
        let negotiations = self.negotiations.read().await;

        for (negotiation_id, negotiation) in negotiations.iter() {
            let Some(rfq) = negotiation.rfq.as_ref() else {
                continue;
            };

            let Some(quote) = negotiation.quotes.iter().find(|msg| match msg {
                OTCMessage::Quote { id, .. } => *id == quote_id,
                _ => false,
            }) else {
                continue;
            };

            return Ok(QuoteContext {
                negotiation_id: *negotiation_id,
                rfq: rfq.clone(),
                quote: quote.clone(),
            });
        }

        Err(SolanaError::OTCError(format!(
            "Quote not found: {quote_id}"
        )))
    }

    /// Get all active negotiations.
    pub async fn get_active_negotiations(&self) -> Vec<Negotiation> {
        self.negotiations
            .read()
            .await
            .values()
            .filter(|n| !n.state.is_terminal())
            .cloned()
            .collect()
    }

    /// Get negotiation history.
    pub async fn get_history(&self) -> Vec<Negotiation> {
        self.negotiations.read().await.values().cloned().collect()
    }

    // Private handlers

    async fn handle_rfq(&self, envelope: &OTCEnvelope) -> Result<Option<OTCMessage>> {
        if let OTCMessage::Rfq {
            id,
            asset,
            direction,
            amount,
            ..
        } = &envelope.message
        {
            debug!(
                id = %id,
                asset = %asset,
                direction = ?direction,
                amount = amount,
                from = %envelope.sender,
                "Received RFQ"
            );

            // Store the RFQ for potential response
            let negotiation = Negotiation {
                id: *id,
                state: NegotiationState::RfqReceived,
                our_role: NegotiationRole::Responder,
                counterparty: Some(envelope.sender.clone()),
                rfq: Some(envelope.message.clone()),
                quotes: Vec::new(),
                selected_quote: None,
                escrow_address: None,
                created_at: Utc::now(),
                updated_at: Utc::now(),
                history: vec![NegotiationEvent::RfqReceived {
                    id: *id,
                    from: envelope.sender.clone(),
                }],
            };

            self.negotiations.write().await.insert(*id, negotiation);
            self.mark_dirty();
        }

        Ok(None) // No automatic response
    }

    async fn handle_quote(&self, envelope: &OTCEnvelope) -> Result<Option<OTCMessage>> {
        if let OTCMessage::Quote {
            id,
            rfq_id,
            price,
            quantity,
            ..
        } = &envelope.message
        {
            debug!(
                quote_id = %id,
                rfq_id = %rfq_id,
                price = price,
                quantity = quantity,
                from = %envelope.sender,
                "Received quote"
            );

            let mut negotiations = self.negotiations.write().await;

            if let Some(neg) = negotiations.get_mut(rfq_id) {
                neg.quotes.push(envelope.message.clone());
                neg.state = NegotiationState::QuoteReceived;
                neg.counterparty = Some(envelope.sender.clone());
                neg.updated_at = Utc::now();
                neg.history.push(NegotiationEvent::QuoteReceived {
                    quote_id: *id,
                    from: envelope.sender.clone(),
                });
                drop(negotiations);
                self.mark_dirty();
            }
        }

        Ok(None)
    }

    async fn handle_counter_offer(&self, envelope: &OTCEnvelope) -> Result<Option<OTCMessage>> {
        if let OTCMessage::CounterOffer {
            id,
            quote_id,
            new_price,
            ..
        } = &envelope.message
        {
            debug!(
                counter_id = %id,
                quote_id = %quote_id,
                new_price = new_price,
                from = %envelope.sender,
                "Received counter-offer"
            );

            // Update negotiation state
            let mut negotiations = self.negotiations.write().await;

            for (_, neg) in negotiations.iter_mut() {
                if neg.quotes.iter().any(|q| match q {
                    OTCMessage::Quote { id, .. } => id == quote_id,
                    _ => false,
                }) {
                    neg.state = NegotiationState::CounterOfferReceived;
                    neg.quotes.push(envelope.message.clone());
                    neg.updated_at = Utc::now();
                    neg.history.push(NegotiationEvent::CounterOfferReceived {
                        counter_id: *id,
                        from: envelope.sender.clone(),
                    });
                    break;
                }
            }
            drop(negotiations);
            self.mark_dirty();
        }

        Ok(None)
    }

    async fn handle_accept(&self, envelope: &OTCEnvelope) -> Result<Option<OTCMessage>> {
        if let OTCMessage::Accept {
            quote_id,
            accepting_wallet,
        } = &envelope.message
        {
            debug!(
                quote_id = %quote_id,
                wallet = %accepting_wallet,
                from = %envelope.sender,
                "Quote accepted"
            );

            let mut negotiations = self.negotiations.write().await;

            for (_, neg) in negotiations.iter_mut() {
                if neg.quotes.iter().any(|q| match q {
                    OTCMessage::Quote { id, .. } => id == quote_id,
                    _ => false,
                }) {
                    neg.state = NegotiationState::Accepted;
                    neg.selected_quote = Some(*quote_id);
                    neg.updated_at = Utc::now();
                    neg.history.push(NegotiationEvent::QuoteAccepted {
                        quote_id: *quote_id,
                    });
                    break;
                }
            }
            drop(negotiations);
            self.mark_dirty();
        }

        Ok(None)
    }

    async fn handle_reject(&self, envelope: &OTCEnvelope) -> Result<Option<OTCMessage>> {
        if let OTCMessage::Reject { quote_id, reason } = &envelope.message {
            debug!(
                quote_id = %quote_id,
                reason = ?reason,
                from = %envelope.sender,
                "Quote rejected"
            );

            let mut negotiations = self.negotiations.write().await;

            for (_, neg) in negotiations.iter_mut() {
                if neg.quotes.iter().any(|q| match q {
                    OTCMessage::Quote { id, .. } => id == quote_id,
                    _ => false,
                }) {
                    neg.history.push(NegotiationEvent::QuoteRejected {
                        quote_id: *quote_id,
                    });
                    neg.updated_at = Utc::now();
                    break;
                }
            }
            drop(negotiations);
            self.mark_dirty();
        }

        Ok(None)
    }

    async fn handle_escrow_funded(&self, envelope: &OTCEnvelope) -> Result<Option<OTCMessage>> {
        if let OTCMessage::EscrowFunded {
            negotiation_id,
            escrow_address,
            signature,
            funded_by,
            ..
        } = &envelope.message
        {
            debug!(
                negotiation_id = %negotiation_id,
                escrow = %escrow_address,
                signature = %signature,
                party = ?funded_by,
                "Escrow funded"
            );

            let mut negotiations = self.negotiations.write().await;

            if let Some(neg) = negotiations.get_mut(negotiation_id) {
                neg.state = NegotiationState::EscrowFunding;
                neg.escrow_address = Some(*escrow_address);
                neg.updated_at = Utc::now();
                neg.history.push(NegotiationEvent::EscrowFunded {
                    party: *funded_by,
                    signature: signature.clone(),
                });
                drop(negotiations);
                self.mark_dirty();
            }
        }

        Ok(None)
    }

    async fn handle_settled(&self, envelope: &OTCEnvelope) -> Result<Option<OTCMessage>> {
        if let OTCMessage::Settled {
            negotiation_id,
            signature,
            final_price,
            final_quantity,
            ..
        } = &envelope.message
        {
            info!(
                negotiation_id = %negotiation_id,
                signature = %signature,
                price = final_price,
                quantity = final_quantity,
                "Trade settled"
            );

            let mut negotiations = self.negotiations.write().await;

            if let Some(neg) = negotiations.get_mut(negotiation_id) {
                neg.state = NegotiationState::Settled;
                neg.updated_at = Utc::now();
                neg.history.push(NegotiationEvent::Settled {
                    signature: signature.clone(),
                });
                drop(negotiations);
                self.mark_dirty();
            }
        }

        Ok(None)
    }

    async fn handle_cancel(&self, envelope: &OTCEnvelope) -> Result<Option<OTCMessage>> {
        if let OTCMessage::Cancel {
            negotiation_id,
            reason,
        } = &envelope.message
        {
            warn!(
                negotiation_id = %negotiation_id,
                reason = %reason,
                "Negotiation cancelled"
            );

            let mut negotiations = self.negotiations.write().await;

            if let Some(neg) = negotiations.get_mut(negotiation_id) {
                neg.state = NegotiationState::Cancelled;
                neg.updated_at = Utc::now();
                neg.history.push(NegotiationEvent::Cancelled {
                    reason: reason.clone(),
                });
                drop(negotiations);
                self.mark_dirty();
            }
        }

        Ok(None)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NegotiationStoreFile {
    version: u32,
    negotiations: Vec<Negotiation>,
}

impl NegotiationStoreFile {
    const VERSION: u32 = 1;
}

fn temp_path_for(path: &Path) -> PathBuf {
    let file_name = path.file_name().unwrap_or_default().to_string_lossy();
    path.with_file_name(format!("{file_name}.tmp"))
}

impl OTCNegotiationManager {
    async fn best_effort_decimals(&self, mint: &Pubkey) -> Option<u8> {
        // Well-known defaults.
        if mint.to_string() == "So11111111111111111111111111111111111111112" {
            return Some(9);
        }
        if mint.to_string() == "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v" {
            return Some(6);
        }

        let account = self.rpc_client.get_account(mint).await.ok()?;
        if account.owner != spl_token::id() {
            return None;
        }

        let mint_state = spl_token::state::Mint::unpack(&account.data).ok()?;
        Some(mint_state.decimals)
    }
}

fn pow10_u128(decimals: u8) -> Result<u128> {
    if decimals > 38 {
        return Err(SolanaError::OTCError(format!(
            "Unsupported token decimals {decimals}"
        )));
    }
    Ok(10u128.pow(decimals as u32))
}

fn compute_settlement_amount(
    price: f64,
    quantity: u64,
    asset_decimals: u8,
    settlement_decimals: u8,
    maker_receives_settlement: bool,
) -> Result<u64> {
    if !price.is_finite() || price <= 0.0 {
        return Err(SolanaError::OTCError("Invalid quote price".to_string()));
    }

    let settlement_scale = pow10_u128(settlement_decimals)?;
    let asset_scale = pow10_u128(asset_decimals)?;

    let price_scaled = (price * settlement_scale as f64).round();
    if !price_scaled.is_finite() || price_scaled < 0.0 {
        return Err(SolanaError::OTCError("Invalid quote price".to_string()));
    }

    let price_scaled = price_scaled as u128;
    let numerator = price_scaled
        .checked_mul(quantity as u128)
        .ok_or_else(|| SolanaError::OTCError("Settlement amount overflow".to_string()))?;
    let mut amount = numerator / asset_scale;
    let remainder = numerator % asset_scale;

    if maker_receives_settlement && remainder != 0 {
        amount = amount
            .checked_add(1)
            .ok_or_else(|| SolanaError::OTCError("Settlement amount overflow".to_string()))?;
    }

    u64::try_from(amount)
        .map_err(|_| SolanaError::OTCError("Settlement amount overflow".to_string()))
}

/// An OTC negotiation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Negotiation {
    /// Negotiation ID (same as RFQ ID).
    pub id: Uuid,
    /// Current state.
    pub state: NegotiationState,
    /// Our role in this negotiation.
    pub our_role: NegotiationRole,
    /// Counterparty identifier.
    pub counterparty: Option<String>,
    /// Original RFQ.
    pub rfq: Option<OTCMessage>,
    /// Received quotes.
    pub quotes: Vec<OTCMessage>,
    /// Selected quote ID.
    pub selected_quote: Option<Uuid>,
    /// Escrow PDA address (if created).
    pub escrow_address: Option<Pubkey>,
    /// Creation time.
    pub created_at: DateTime<Utc>,
    /// Last update time.
    pub updated_at: DateTime<Utc>,
    /// Event history.
    pub history: Vec<NegotiationEvent>,
}

/// Negotiation state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NegotiationState {
    /// RFQ sent, waiting for quotes.
    RfqSent,
    /// RFQ received from counterparty.
    RfqReceived,
    /// Quote sent, waiting for response.
    QuoteSent,
    /// Quote received, considering.
    QuoteReceived,
    /// Counter-offer sent.
    CounterOfferSent,
    /// Counter-offer received.
    CounterOfferReceived,
    /// Quote accepted, proceeding to escrow.
    Accepted,
    /// Escrow being funded.
    EscrowFunding,
    /// Escrow fully funded.
    EscrowFunded,
    /// Trade settled successfully.
    Settled,
    /// Negotiation cancelled.
    Cancelled,
    /// Negotiation expired.
    Expired,
}

impl NegotiationState {
    /// Check if this is a terminal state.
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Settled | Self::Cancelled | Self::Expired)
    }
}

/// Our role in a negotiation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NegotiationRole {
    /// We initiated the RFQ.
    Initiator,
    /// We're responding to someone's RFQ.
    Responder,
}

/// Negotiation event for history tracking.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum NegotiationEvent {
    RfqCreated {
        id: Uuid,
    },
    RfqReceived {
        id: Uuid,
        from: String,
    },
    QuoteSent {
        quote_id: Uuid,
    },
    QuoteReceived {
        quote_id: Uuid,
        from: String,
    },
    CounterOfferSent {
        counter_id: Uuid,
    },
    CounterOfferReceived {
        counter_id: Uuid,
        from: String,
    },
    QuoteAccepted {
        quote_id: Uuid,
    },
    QuoteRejected {
        quote_id: Uuid,
    },
    EscrowCreated {
        escrow_address: Pubkey,
    },
    EscrowFunded {
        party: EscrowParty,
        signature: String,
    },
    Settled {
        signature: String,
    },
    Cancelled {
        reason: String,
    },
    Expired,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_rfq() {
        let rpc = Arc::new(RpcClient::new("https://api.devnet.solana.com".to_string()));
        let manager = OTCNegotiationManager::new(rpc);

        let negotiation = manager
            .create_rfq(
                "SOL",
                Pubkey::new_unique(),
                TradeDirection::Buy,
                1_000_000_000,
            )
            .await
            .unwrap();

        assert_eq!(negotiation.state, NegotiationState::RfqSent);
        assert_eq!(negotiation.our_role, NegotiationRole::Initiator);
    }

    #[tokio::test]
    async fn test_negotiation_state_terminal() {
        assert!(NegotiationState::Settled.is_terminal());
        assert!(NegotiationState::Cancelled.is_terminal());
        assert!(!NegotiationState::RfqSent.is_terminal());
        assert!(!NegotiationState::QuoteReceived.is_terminal());
    }

    #[tokio::test]
    async fn test_persistence_roundtrip() {
        // Use a mock RPC client to avoid network dependency in tests.
        let rpc = Arc::new(RpcClient::new_mock("succeeds".to_string()));
        let manager = Arc::new(OTCNegotiationManager::new(rpc).with_wallet(Pubkey::new_unique()));

        let negotiation = manager
            .create_rfq(
                "SOL",
                Pubkey::new_unique(),
                TradeDirection::Buy,
                1_000_000_000,
            )
            .await
            .unwrap();

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("otc_state.json");
        manager.save_to_file(&path).await.unwrap();

        let rpc2 = Arc::new(RpcClient::new_mock("succeeds".to_string()));
        let manager2 = OTCNegotiationManager::new(rpc2).with_wallet(Pubkey::new_unique());
        assert!(manager2.load_from_file(&path).await.unwrap());

        let loaded = manager2.get_negotiation(negotiation.id).await;
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().id, negotiation.id);
    }

    #[tokio::test]
    async fn test_load_missing_persistence_file_is_ok() {
        let rpc = Arc::new(RpcClient::new_mock("succeeds".to_string()));
        let manager = OTCNegotiationManager::new(rpc);

        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("does_not_exist.json");
        assert!(!manager.load_from_file(&missing).await.unwrap());
    }
}
