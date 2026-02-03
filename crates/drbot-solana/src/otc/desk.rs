//! OTC desk agent (A2A responder).
//!
//! This is a lightweight “desk” that listens for RFQs over the in-process A2A hub
//! and responds with quotes using a pluggable quote engine.

use super::a2a::{otc_notification, parse_otc_envelope};
use super::negotiation::OTCNegotiationManager;
use super::protocol::{OTCEnvelope, OTCMessage, TradeDirection, OTC_CAPABILITY};
use crate::Result;
use async_trait::async_trait;
use drbot_a2a::{A2AError, A2AHub, A2AMessage, Agent, Capability, MessageHandler, MessageType};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signer::keypair::Keypair;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

/// Wrapped SOL (wSOL) mint address.
pub fn wsol_mint() -> Pubkey {
    "So11111111111111111111111111111111111111112"
        .parse()
        .expect("valid wSOL mint")
}

/// USDC mint address.
pub fn usdc_mint() -> Pubkey {
    "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"
        .parse()
        .expect("valid USDC mint")
}

/// Quote produced by a desk.
#[derive(Debug, Clone)]
pub struct DeskQuote {
    pub price: f64,
    pub quantity: u64,
    pub settlement_asset: String,
    pub settlement_mint: Pubkey,
    pub valid_secs: u64,
}

/// A pluggable quoting strategy for an OTC desk.
#[async_trait]
pub trait QuoteEngine: Send + Sync {
    async fn quote(&self, rfq: &OTCMessage) -> Result<Option<DeskQuote>>;
}

/// Simple fixed-price quote engine (good for tests / demos).
pub struct FixedQuoteEngine {
    pub price: f64,
    pub settlement_asset: String,
    pub settlement_mint: Pubkey,
    pub valid_secs: u64,
}

#[async_trait]
impl QuoteEngine for FixedQuoteEngine {
    async fn quote(&self, rfq: &OTCMessage) -> Result<Option<DeskQuote>> {
        let OTCMessage::Rfq { amount, .. } = rfq else {
            return Ok(None);
        };

        if *amount == 0 {
            return Ok(None);
        }

        Ok(Some(DeskQuote {
            price: self.price,
            quantity: *amount,
            settlement_asset: self.settlement_asset.clone(),
            settlement_mint: self.settlement_mint,
            valid_secs: self.valid_secs,
        }))
    }
}

/// A spread-based quote engine with a fixed mid price per asset.
///
/// Quotes are produced around `mid_price` using a symmetric spread:
/// - RFQ direction `Buy` => desk quotes *ask* = mid * (1 + spread/2)
/// - RFQ direction `Sell` => desk quotes *bid* = mid * (1 - spread/2)
pub struct SpreadQuoteEngine {
    default_settlement_asset: String,
    default_settlement_mint: Pubkey,
    default_spread_bps: u16,
    valid_secs: u64,
    markets_by_mint: HashMap<Pubkey, SpreadMarket>,
    markets_by_symbol: HashMap<String, SpreadMarket>,
}

#[derive(Debug, Clone)]
pub struct SpreadMarket {
    pub mid_price: f64,
    pub spread_bps: u16,
    pub settlement_asset: Option<String>,
    pub settlement_mint: Option<Pubkey>,
}

impl SpreadQuoteEngine {
    pub fn new(
        default_settlement_asset: impl Into<String>,
        default_settlement_mint: Pubkey,
    ) -> Self {
        Self {
            default_settlement_asset: default_settlement_asset.into(),
            default_settlement_mint,
            default_spread_bps: 100, // 1.00%
            valid_secs: 60,
            markets_by_mint: HashMap::new(),
            markets_by_symbol: HashMap::new(),
        }
    }

    pub fn with_default_spread_bps(mut self, spread_bps: u16) -> Self {
        self.default_spread_bps = spread_bps;
        self
    }

    pub fn with_valid_secs(mut self, valid_secs: u64) -> Self {
        self.valid_secs = valid_secs;
        self
    }

    pub fn insert_market_mint(&mut self, mint: Pubkey, market: SpreadMarket) {
        self.markets_by_mint.insert(mint, market);
    }

    pub fn insert_market_symbol(&mut self, symbol: impl Into<String>, market: SpreadMarket) {
        self.markets_by_symbol
            .insert(symbol.into().to_lowercase(), market);
    }

    /// Convenience helper to configure a SOL/USDC market.
    ///
    /// - matches by symbol `"SOL"` and by wSOL mint
    /// - quotes in USDC
    pub fn with_sol_usdc_market(mut self, mid_price: f64, spread_bps: u16) -> Self {
        let usdc = usdc_mint();

        let market = SpreadMarket {
            mid_price,
            spread_bps,
            settlement_asset: Some("USDC".to_string()),
            settlement_mint: Some(usdc),
        };

        self.insert_market_symbol("SOL", market.clone());
        self.insert_market_mint(wsol_mint(), market);
        self
    }

    fn resolve_market<'a>(&'a self, asset: &str, asset_mint: &Pubkey) -> Option<&'a SpreadMarket> {
        if *asset_mint != Pubkey::default() {
            if let Some(market) = self.markets_by_mint.get(asset_mint) {
                return Some(market);
            }
        }
        self.markets_by_symbol.get(&asset.to_lowercase())
    }
}

#[async_trait]
impl QuoteEngine for SpreadQuoteEngine {
    async fn quote(&self, rfq: &OTCMessage) -> Result<Option<DeskQuote>> {
        use chrono::Utc;

        let OTCMessage::Rfq {
            asset,
            asset_mint,
            direction,
            amount,
            price_limit,
            expires_at,
            ..
        } = rfq
        else {
            return Ok(None);
        };

        if *amount == 0 {
            return Ok(None);
        }

        if Utc::now() >= *expires_at {
            return Ok(None);
        }

        let Some(market) = self.resolve_market(asset, asset_mint) else {
            return Ok(None);
        };
        if market.mid_price <= 0.0 {
            return Ok(None);
        }

        let spread_bps = if market.spread_bps == 0 {
            self.default_spread_bps
        } else {
            market.spread_bps
        };

        let half_spread = (spread_bps as f64) / 20_000.0;
        let mid = market.mid_price;

        let price = match direction {
            TradeDirection::Buy => mid * (1.0 + half_spread),
            TradeDirection::Sell => mid * (1.0 - half_spread),
        };

        if let Some(limit) = price_limit {
            match direction {
                TradeDirection::Buy => {
                    if price > *limit {
                        return Ok(None);
                    }
                }
                TradeDirection::Sell => {
                    if price < *limit {
                        return Ok(None);
                    }
                }
            }
        }

        let settlement_asset = market
            .settlement_asset
            .clone()
            .unwrap_or_else(|| self.default_settlement_asset.clone());
        let settlement_mint = market
            .settlement_mint
            .unwrap_or(self.default_settlement_mint);

        Ok(Some(DeskQuote {
            price,
            quantity: *amount,
            settlement_asset,
            settlement_mint,
            valid_secs: self.valid_secs,
        }))
    }
}

/// OTC desk configuration.
#[derive(Debug, Clone)]
pub struct OtcDeskConfig {
    pub name: String,
    pub agent_type: String,
    pub auto_quote: bool,
    /// Desk settlement wallet (used for maker wallet in quotes).
    pub wallet: Option<Pubkey>,
}

impl Default for OtcDeskConfig {
    fn default() -> Self {
        Self {
            name: "otc-desk".to_string(),
            agent_type: "otc_desk".to_string(),
            auto_quote: true,
            wallet: None,
        }
    }
}

/// OTC desk agent that responds to RFQs over A2A.
pub struct OtcDeskAgent {
    pub agent: Agent,
    manager: Arc<OTCNegotiationManager>,
    quote_engine: Arc<dyn QuoteEngine>,
    auto_quote: bool,
    signing_keypair: Option<Arc<Mutex<Keypair>>>,
}

impl OtcDeskAgent {
    /// Create a new desk agent.
    pub fn new(
        rpc_client: Arc<RpcClient>,
        config: OtcDeskConfig,
        quote_engine: Arc<dyn QuoteEngine>,
    ) -> Self {
        let OtcDeskConfig {
            name,
            agent_type,
            auto_quote,
            wallet,
        } = config;

        let agent = Agent::new(&name, &agent_type).with_capability(Capability::new(
            OTC_CAPABILITY,
            "Agent-to-agent Solana OTC negotiation (RFQ/Quote)",
        ));

        let mut manager = OTCNegotiationManager::new(rpc_client);
        if let Some(wallet) = wallet {
            manager = manager.with_wallet(wallet);
        }
        let manager = Arc::new(manager);

        Self {
            agent,
            manager,
            quote_engine,
            auto_quote,
            signing_keypair: None,
        }
    }

    /// Configure the desk to sign outgoing RFQ/Quote/Accept envelopes.
    pub fn with_signing_keypair(mut self, keypair: Keypair) -> Self {
        self.signing_keypair = Some(Arc::new(Mutex::new(keypair)));
        self
    }

    /// Access the underlying negotiation manager.
    pub fn negotiation_manager(&self) -> Arc<OTCNegotiationManager> {
        self.manager.clone()
    }

    /// Convenience constructor for a fixed-spread SOL/USDC desk.
    ///
    /// `mid_price` is in USDC per SOL and `spread_bps` is the total (bid/ask) spread in basis points.
    pub fn new_sol_usdc_spread(
        rpc_client: Arc<RpcClient>,
        name: impl Into<String>,
        mid_price: f64,
        spread_bps: u16,
    ) -> Self {
        let quote_engine: Arc<dyn QuoteEngine> = Arc::new(
            SpreadQuoteEngine::new("USDC", usdc_mint()).with_sol_usdc_market(mid_price, spread_bps),
        );

        Self::new(
            rpc_client,
            OtcDeskConfig {
                name: name.into(),
                ..Default::default()
            },
            quote_engine,
        )
    }

    /// Same as [`Self::new_sol_usdc_spread`] but configures the desk's settlement wallet.
    pub fn new_sol_usdc_spread_with_wallet(
        rpc_client: Arc<RpcClient>,
        name: impl Into<String>,
        mid_price: f64,
        spread_bps: u16,
        wallet: Pubkey,
    ) -> Self {
        let quote_engine: Arc<dyn QuoteEngine> = Arc::new(
            SpreadQuoteEngine::new("USDC", usdc_mint()).with_sol_usdc_market(mid_price, spread_bps),
        );

        Self::new(
            rpc_client,
            OtcDeskConfig {
                name: name.into(),
                wallet: Some(wallet),
                ..Default::default()
            },
            quote_engine,
        )
    }

    /// Spawn the desk listener loop.
    pub async fn spawn(self, hub: Arc<A2AHub>) -> JoinHandle<()> {
        hub.register_agent(self.agent.clone()).await;

        let mut rx = hub.subscribe();
        let desk_id = self.agent.id;

        let desk = Arc::new(self);

        tokio::spawn(async move {
            loop {
                let msg = match rx.recv().await {
                    Ok(m) => m,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                };

                if msg.to != desk_id {
                    continue;
                }

                let response = match desk.handle(msg).await {
                    Ok(r) => r,
                    Err(e) => {
                        tracing::warn!(error = %e, "OTC desk handler error");
                        None
                    }
                };

                if let Some(reply) = response {
                    let _ = hub.send(reply).await;
                }
            }
        })
    }
}

#[async_trait]
impl MessageHandler for OtcDeskAgent {
    async fn handle(&self, message: A2AMessage) -> drbot_a2a::Result<Option<A2AMessage>> {
        if message.message_type != MessageType::Notification {
            return Ok(None);
        }

        let Some(envelope) = parse_otc_envelope(&message) else {
            return Ok(None);
        };

        // In "real settlement" mode, require signed messages for message types that
        // define an expected signing wallet (RFQ / Quote / Accept).
        if self.signing_keypair.is_some() && envelope.message.signer_wallet().is_some() {
            match envelope.verify_signature() {
                Ok(true) => {}
                Ok(false) => return Ok(None),
                Err(e) => {
                    tracing::warn!(error = %e, "Invalid OTC envelope signature");
                    return Ok(None);
                }
            }
        }

        // Update local negotiation state.
        self.manager
            .handle_message(envelope.clone())
            .await
            .map_err(|e| A2AError::CommunicationFailed(e.to_string()))?;

        // Auto-quote RFQs.
        if !self.auto_quote {
            return Ok(None);
        }

        let OTCMessage::Rfq { id: rfq_id, .. } = &envelope.message else {
            return Ok(None);
        };

        let quote = self
            .quote_engine
            .quote(&envelope.message)
            .await
            .map_err(|e| A2AError::CommunicationFailed(e.to_string()))?;

        let Some(quote) = quote else {
            return Ok(None);
        };

        let quote_msg = self
            .manager
            .create_quote(
                *rfq_id,
                quote.price,
                quote.quantity,
                quote.settlement_asset,
                quote.settlement_mint,
                quote.valid_secs,
            )
            .await
            .map_err(|e| A2AError::CommunicationFailed(e.to_string()))?;

        let mut reply_envelope = OTCEnvelope::new(self.agent.id.to_string(), quote_msg);
        if let Some(signer) = self.signing_keypair.as_ref() {
            let kp = signer.lock().await;
            reply_envelope = reply_envelope
                .sign_with(&kp)
                .map_err(|e| A2AError::CommunicationFailed(e.to_string()))?;
        }
        Ok(Some(otc_notification(
            self.agent.id,
            message.from,
            reply_envelope,
        )))
    }
}

/// Helper to create an RFQ message with a best-effort asset mint.
///
/// For now this expects callers to pass the mint explicitly. The `asset` string is used
/// for display only.
pub fn make_rfq(
    asset: impl Into<String>,
    asset_mint: Pubkey,
    direction: TradeDirection,
    amount: u64,
    expires_in_secs: u64,
    initiator_wallet: Pubkey,
) -> OTCMessage {
    OTCMessage::rfq(
        asset,
        asset_mint,
        direction,
        amount,
        expires_in_secs,
        initiator_wallet,
    )
}

/// Convenience helper for SOL RFQs using the canonical wSOL mint.
pub fn make_sol_rfq(
    direction: TradeDirection,
    amount_lamports: u64,
    expires_in_secs: u64,
    initiator_wallet: Pubkey,
) -> OTCMessage {
    make_rfq(
        "SOL",
        wsol_mint(),
        direction,
        amount_lamports,
        expires_in_secs,
        initiator_wallet,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::otc::trader::OtcTraderClient;
    use drbot_a2a::{A2AConfig, A2AHub};
    use solana_client::nonblocking::rpc_client::RpcClient;
    use solana_sdk::pubkey::Pubkey;
    use solana_sdk::signer::keypair::Keypair;
    use solana_sdk::signer::Signer;
    use std::sync::Arc;
    use tokio::time::Duration;

    #[tokio::test]
    async fn test_otc_desk_quotes_rfq_over_a2a() {
        // Use a mock RPC client to avoid platform-specific system proxy panics in tests.
        let rpc = Arc::new(RpcClient::new_mock("succeeds".to_string()));

        let hub_agent = Agent::new("hub", "coordinator");
        let hub = Arc::new(A2AHub::new(A2AConfig::default(), hub_agent));

        // Give the hub a moment to register its local agent.
        tokio::time::sleep(Duration::from_millis(10)).await;

        let desk = OtcDeskAgent::new_sol_usdc_spread(rpc, "desk-1", 100.0, 100);
        let _desk_task = desk.spawn(hub.clone()).await;

        let trader = OtcTraderClient::new(hub.clone(), "trader-1").await;
        let rfq = make_sol_rfq(TradeDirection::Buy, 1_000_000_000, 120, Pubkey::default());

        let quotes = trader
            .broadcast_rfq(rfq, 5, Duration::from_millis(500))
            .await
            .unwrap();

        assert_eq!(quotes.len(), 1);
        let (_from, env) = &quotes[0];
        match &env.message {
            OTCMessage::Quote { price, .. } => {
                // mid=100, spread=1% => ask=100.5 for RFQ buy (desk sells).
                assert!((*price - 100.5).abs() < 1e-9);
            }
            _ => panic!("expected quote"),
        }
    }

    #[tokio::test]
    async fn test_otc_desk_requires_signed_rfq_when_signing_enabled() {
        let rpc = Arc::new(RpcClient::new_mock("succeeds".to_string()));

        let hub_agent = Agent::new("hub", "coordinator");
        let hub = Arc::new(A2AHub::new(A2AConfig::default(), hub_agent));

        tokio::time::sleep(Duration::from_millis(10)).await;

        let desk_keypair = Keypair::new();
        let desk = OtcDeskAgent::new_sol_usdc_spread_with_wallet(
            rpc,
            "desk-1",
            100.0,
            100,
            desk_keypair.pubkey(),
        )
        .with_signing_keypair(desk_keypair);
        let _desk_task = desk.spawn(hub.clone()).await;

        let trader = OtcTraderClient::new(hub.clone(), "trader-1").await;

        let trader_keypair = Keypair::new();
        let rfq = make_sol_rfq(
            TradeDirection::Buy,
            1_000_000_000,
            120,
            trader_keypair.pubkey(),
        );

        // Unsigned RFQ should be ignored by the desk.
        let quotes = trader
            .broadcast_rfq(rfq.clone(), 5, Duration::from_millis(300))
            .await
            .unwrap();
        assert_eq!(quotes.len(), 0);

        // Signed RFQ should produce a signed quote.
        let quotes = trader
            .broadcast_rfq_signed(rfq, &trader_keypair, 5, Duration::from_millis(300))
            .await
            .unwrap();
        assert_eq!(quotes.len(), 1);
        let (_from, env) = &quotes[0];
        assert_eq!(env.verify_signature().unwrap(), true);
    }

    #[tokio::test]
    async fn test_spread_engine_bid_ask() {
        let engine = SpreadQuoteEngine::new("USDC", usdc_mint())
            .with_default_spread_bps(200)
            .with_sol_usdc_market(100.0, 200);

        let rfq_buy = make_rfq(
            "SOL",
            Pubkey::default(),
            TradeDirection::Buy,
            1_000,
            120,
            Pubkey::default(),
        );
        let ask = engine.quote(&rfq_buy).await.unwrap().unwrap().price;
        assert!((ask - 101.0).abs() < 1e-9);

        let rfq_sell = make_rfq(
            "SOL",
            Pubkey::default(),
            TradeDirection::Sell,
            1_000,
            120,
            Pubkey::default(),
        );
        let bid = engine.quote(&rfq_sell).await.unwrap().unwrap().price;
        assert!((bid - 99.0).abs() < 1e-9);
    }
}
