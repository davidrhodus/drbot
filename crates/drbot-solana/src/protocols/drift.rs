//! Drift Protocol integration for perpetual futures trading.
//!
//! Provides access to Drift's perpetual markets for leveraged trading.

use crate::{Result, SolanaError};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;
use std::sync::Arc;
use tracing::{debug, info};

/// Drift program ID on mainnet.
pub const DRIFT_PROGRAM_ID: &str = "dRiftyHA39MWEi3m9aunc5MzRF1JYuBsbn6VPcn33UH";

/// Drift perpetual market information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriftMarket {
    /// Market symbol (e.g., "SOL-PERP").
    pub symbol: String,
    /// Market index.
    pub market_index: u16,
    /// Current mark price.
    pub mark_price: f64,
    /// 24h trading volume in USD.
    pub volume_24h: f64,
    /// Open interest in base asset.
    pub open_interest: f64,
    /// Funding rate (hourly).
    pub funding_rate: f64,
}

/// User position in a perpetual market.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriftPosition {
    /// Market index.
    pub market_index: u16,
    /// Market symbol.
    pub symbol: String,
    /// Position size (positive = long, negative = short).
    pub size: f64,
    /// Average entry price.
    pub entry_price: f64,
    /// Current mark price.
    pub mark_price: f64,
    /// Unrealized PnL in USD.
    pub unrealized_pnl: f64,
    /// Realized PnL in USD.
    pub realized_pnl: f64,
    /// Leverage used.
    pub leverage: f64,
    /// Liquidation price.
    pub liquidation_price: Option<f64>,
}

/// User account information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriftAccount {
    /// Account public key.
    pub address: Pubkey,
    /// USDC collateral deposited.
    pub collateral: f64,
    /// Total unrealized PnL.
    pub total_unrealized_pnl: f64,
    /// Account margin ratio.
    pub margin_ratio: f64,
    /// Account leverage.
    pub leverage: f64,
    /// Active positions.
    pub positions: Vec<DriftPosition>,
}

/// Order side.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OrderSide {
    Long,
    Short,
}

impl std::fmt::Display for OrderSide {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OrderSide::Long => write!(f, "long"),
            OrderSide::Short => write!(f, "short"),
        }
    }
}

/// Order type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OrderType {
    Market,
    Limit,
    StopMarket,
    StopLimit,
    TakeProfitMarket,
    TakeProfitLimit,
}

/// Parameters for opening a position.
#[derive(Debug, Clone)]
pub struct OpenPositionParams {
    /// Market symbol or index.
    pub market: String,
    /// Position size in base asset.
    pub size: f64,
    /// Leverage to use.
    pub leverage: f64,
    /// Order side (long/short).
    pub side: OrderSide,
    /// Order type.
    pub order_type: OrderType,
    /// Limit price (for limit orders).
    pub limit_price: Option<f64>,
    /// Reduce only flag.
    pub reduce_only: bool,
}

impl Default for OpenPositionParams {
    fn default() -> Self {
        Self {
            market: String::new(),
            size: 0.0,
            leverage: 1.0,
            side: OrderSide::Long,
            order_type: OrderType::Market,
            limit_price: None,
            reduce_only: false,
        }
    }
}

/// Parameters for closing a position.
#[derive(Debug, Clone)]
pub struct ClosePositionParams {
    /// Market symbol or index.
    pub market: String,
    /// Size to close (None = close entire position).
    pub size: Option<f64>,
    /// Order type.
    pub order_type: OrderType,
    /// Limit price (for limit orders).
    pub limit_price: Option<f64>,
}

/// Drift protocol client.
pub struct DriftClient {
    rpc_client: Arc<RpcClient>,
    http_client: Client,
    program_id: Pubkey,
}

impl DriftClient {
    /// Create a new Drift client.
    pub fn new(rpc_client: Arc<RpcClient>) -> Self {
        Self {
            rpc_client,
            http_client: Client::new(),
            program_id: DRIFT_PROGRAM_ID.parse().unwrap(),
        }
    }

    /// Get the program ID.
    pub fn program_id(&self) -> &Pubkey {
        &self.program_id
    }

    /// Get all available perpetual markets.
    pub async fn get_markets(&self) -> Result<Vec<DriftMarket>> {
        // Drift markets (hardcoded for now, would fetch from API in production)
        let markets = vec![
            DriftMarket {
                symbol: "SOL-PERP".to_string(),
                market_index: 0,
                mark_price: 0.0,
                volume_24h: 0.0,
                open_interest: 0.0,
                funding_rate: 0.0,
            },
            DriftMarket {
                symbol: "BTC-PERP".to_string(),
                market_index: 1,
                mark_price: 0.0,
                volume_24h: 0.0,
                open_interest: 0.0,
                funding_rate: 0.0,
            },
            DriftMarket {
                symbol: "ETH-PERP".to_string(),
                market_index: 2,
                mark_price: 0.0,
                volume_24h: 0.0,
                open_interest: 0.0,
                funding_rate: 0.0,
            },
            DriftMarket {
                symbol: "APT-PERP".to_string(),
                market_index: 3,
                mark_price: 0.0,
                volume_24h: 0.0,
                open_interest: 0.0,
                funding_rate: 0.0,
            },
            DriftMarket {
                symbol: "ARB-PERP".to_string(),
                market_index: 4,
                mark_price: 0.0,
                volume_24h: 0.0,
                open_interest: 0.0,
                funding_rate: 0.0,
            },
        ];

        Ok(markets)
    }

    /// Get a specific market by symbol.
    pub async fn get_market(&self, symbol: &str) -> Result<Option<DriftMarket>> {
        let markets = self.get_markets().await?;
        Ok(markets.into_iter().find(|m| m.symbol == symbol))
    }

    /// Get user positions.
    pub async fn get_positions(&self, user: &Pubkey) -> Result<Vec<DriftPosition>> {
        debug!(user = %user, "Fetching Drift positions");

        // In production, would query on-chain user account
        // For now, return empty positions
        Ok(vec![])
    }

    /// Get user account information.
    pub async fn get_account(&self, user: &Pubkey) -> Result<Option<DriftAccount>> {
        debug!(user = %user, "Fetching Drift account");

        let positions = self.get_positions(user).await?;

        if positions.is_empty() {
            return Ok(None);
        }

        Ok(Some(DriftAccount {
            address: *user,
            collateral: 0.0,
            total_unrealized_pnl: positions.iter().map(|p| p.unrealized_pnl).sum(),
            margin_ratio: 1.0,
            leverage: 1.0,
            positions,
        }))
    }

    /// Open a perpetual position.
    pub async fn open_position(&self, _params: OpenPositionParams) -> Result<String> {
        // In production:
        // 1. Get market account
        // 2. Calculate margin required
        // 3. Build place order instruction
        // 4. Sign and send transaction

        info!("Drift open position requested");

        Err(SolanaError::DeFiProtocolError(
            "Drift open_position not yet implemented - requires on-chain interaction".to_string(),
        ))
    }

    /// Close a perpetual position.
    pub async fn close_position(&self, _params: ClosePositionParams) -> Result<String> {
        info!("Drift close position requested");

        Err(SolanaError::DeFiProtocolError(
            "Drift close_position not yet implemented - requires on-chain interaction".to_string(),
        ))
    }

    /// Deposit USDC collateral.
    pub async fn deposit(&self, _amount: f64) -> Result<String> {
        Err(SolanaError::DeFiProtocolError(
            "Drift deposit not yet implemented".to_string(),
        ))
    }

    /// Withdraw USDC collateral.
    pub async fn withdraw(&self, _amount: f64) -> Result<String> {
        Err(SolanaError::DeFiProtocolError(
            "Drift withdraw not yet implemented".to_string(),
        ))
    }

    /// Get available markets as symbols.
    pub fn available_markets() -> Vec<&'static str> {
        vec!["SOL-PERP", "BTC-PERP", "ETH-PERP", "APT-PERP", "ARB-PERP"]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_order_side_display() {
        assert_eq!(OrderSide::Long.to_string(), "long");
        assert_eq!(OrderSide::Short.to_string(), "short");
    }

    #[test]
    fn test_available_markets() {
        let markets = DriftClient::available_markets();
        assert!(markets.contains(&"SOL-PERP"));
        assert!(markets.contains(&"BTC-PERP"));
    }

    #[test]
    fn test_open_position_params_default() {
        let params = OpenPositionParams::default();
        assert_eq!(params.leverage, 1.0);
        assert_eq!(params.side, OrderSide::Long);
        assert_eq!(params.order_type, OrderType::Market);
    }
}
