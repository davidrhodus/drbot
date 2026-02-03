//! Meteora DLMM integration for dynamic liquidity market making.
//!
//! Provides access to Meteora's DLMM (Dynamic Liquidity Market Maker) pools.

use crate::{Result, SolanaError};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;
use std::sync::Arc;
use tracing::{debug, info};

/// Meteora DLMM program ID.
pub const METEORA_DLMM_PROGRAM_ID: &str = "LBUZKhRxPF3XUpBCjp4YzTKgLccjZhTSDM9YuVaPwxo";

/// Meteora API base URL.
const METEORA_API_URL: &str = "https://dlmm-api.meteora.ag";

/// Meteora DLMM pool information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeteoraPool {
    /// Pool address.
    pub address: String,
    /// Pool name (e.g., "SOL-USDC").
    pub name: String,
    /// Token X (base) mint.
    pub token_x: String,
    /// Token X symbol.
    pub token_x_symbol: String,
    /// Token Y (quote) mint.
    pub token_y: String,
    /// Token Y symbol.
    pub token_y_symbol: String,
    /// Total value locked in USD.
    pub tvl: f64,
    /// Annual percentage yield.
    pub apy: f64,
    /// Base fee percentage.
    pub fee_rate: f64,
    /// Current bin step (price tick).
    pub bin_step: u16,
    /// Active bin ID.
    pub active_bin_id: i32,
}

/// DLMM position (liquidity bin range).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DlmmPosition {
    /// Position address.
    pub address: String,
    /// Pool address.
    pub pool: String,
    /// Pool name.
    pub pool_name: String,
    /// Total liquidity in position.
    pub liquidity: f64,
    /// Lower bin ID.
    pub lower_bin_id: i32,
    /// Upper bin ID.
    pub upper_bin_id: i32,
    /// Token X amount.
    pub token_x_amount: f64,
    /// Token Y amount.
    pub token_y_amount: f64,
    /// Fees earned in token X.
    pub fees_x: f64,
    /// Fees earned in token Y.
    pub fees_y: f64,
    /// Position value in USD.
    pub value_usd: f64,
}

/// Parameters for adding liquidity.
#[derive(Debug, Clone)]
pub struct AddLiquidityParams {
    /// Pool address.
    pub pool: String,
    /// Amount of token X.
    pub amount_x: f64,
    /// Amount of token Y.
    pub amount_y: f64,
    /// Lower price bound.
    pub lower_price: Option<f64>,
    /// Upper price bound.
    pub upper_price: Option<f64>,
    /// Distribution strategy.
    pub strategy: LiquidityStrategy,
}

/// Liquidity distribution strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LiquidityStrategy {
    /// Uniform distribution across bins.
    Uniform,
    /// Concentrated at current price.
    Spot,
    /// Bid-ask spread pattern.
    BidAsk,
    /// Custom curve.
    Curve,
}

impl Default for LiquidityStrategy {
    fn default() -> Self {
        Self::Uniform
    }
}

/// Meteora DLMM client.
pub struct MeteoraClient {
    rpc_client: Arc<RpcClient>,
    http_client: Client,
    program_id: Pubkey,
}

impl MeteoraClient {
    /// Create a new Meteora client.
    pub fn new(rpc_client: Arc<RpcClient>) -> Self {
        Self {
            rpc_client,
            http_client: Client::new(),
            program_id: METEORA_DLMM_PROGRAM_ID.parse().unwrap(),
        }
    }

    /// Get the program ID.
    pub fn program_id(&self) -> &Pubkey {
        &self.program_id
    }

    /// Get all DLMM pools.
    pub async fn get_pools(&self) -> Result<Vec<MeteoraPool>> {
        debug!("Fetching Meteora pools");

        let url = format!("{}/pair/all", METEORA_API_URL);

        let response = self.http_client.get(&url).send().await?;

        if !response.status().is_success() {
            return Ok(self.default_pools());
        }

        let data: Vec<MeteoraApiPool> = response.json().await.unwrap_or_default();

        let pools: Vec<MeteoraPool> = data
            .into_iter()
            .take(100)
            .map(|p| MeteoraPool {
                address: p.address,
                name: p.name,
                token_x: p.mint_x,
                token_x_symbol: p.symbol_x.unwrap_or_default(),
                token_y: p.mint_y,
                token_y_symbol: p.symbol_y.unwrap_or_default(),
                tvl: p.liquidity.unwrap_or(0.0),
                apy: p.apr.unwrap_or(0.0),
                fee_rate: p.base_fee_percentage.unwrap_or(0.0),
                bin_step: p.bin_step.unwrap_or(1),
                active_bin_id: p.active_id.unwrap_or(0),
            })
            .collect();

        debug!(count = pools.len(), "Fetched Meteora pools");
        Ok(pools)
    }

    /// Get a specific pool by address.
    pub async fn get_pool(&self, address: &str) -> Result<Option<MeteoraPool>> {
        let pools = self.get_pools().await?;
        Ok(pools.into_iter().find(|p| p.address == address))
    }

    /// Search pools by token.
    pub async fn search_pools(&self, token: &str) -> Result<Vec<MeteoraPool>> {
        let pools = self.get_pools().await?;
        let token_upper = token.to_uppercase();

        Ok(pools
            .into_iter()
            .filter(|p| {
                p.token_x_symbol.to_uppercase().contains(&token_upper)
                    || p.token_y_symbol.to_uppercase().contains(&token_upper)
                    || p.name.to_uppercase().contains(&token_upper)
            })
            .collect())
    }

    /// Get user DLMM positions.
    pub async fn get_positions(&self, _user: &Pubkey) -> Result<Vec<DlmmPosition>> {
        // In production, would query on-chain position accounts
        Ok(vec![])
    }

    /// Add liquidity to a pool.
    pub async fn add_liquidity(&self, params: AddLiquidityParams) -> Result<String> {
        info!(
            pool = %params.pool,
            amount_x = params.amount_x,
            amount_y = params.amount_y,
            strategy = ?params.strategy,
            "Meteora add liquidity requested"
        );

        Err(SolanaError::DeFiProtocolError(
            "Meteora add_liquidity not yet implemented - requires on-chain interaction".to_string(),
        ))
    }

    /// Remove liquidity from a position.
    pub async fn remove_liquidity(&self, position_address: &str) -> Result<String> {
        info!(position = %position_address, "Meteora remove liquidity requested");

        Err(SolanaError::DeFiProtocolError(
            "Meteora remove_liquidity not yet implemented - requires on-chain interaction"
                .to_string(),
        ))
    }

    /// Claim fees from a position.
    pub async fn claim_fees(&self, position_address: &str) -> Result<String> {
        info!(position = %position_address, "Meteora claim fees requested");

        Err(SolanaError::DeFiProtocolError(
            "Meteora claim_fees not yet implemented - requires on-chain interaction".to_string(),
        ))
    }

    /// Get default/popular pools.
    fn default_pools(&self) -> Vec<MeteoraPool> {
        vec![MeteoraPool {
            address: "ARwi1S4DaiTG5DX7S4M4ZsrXqpMD1MrTmbu9ue2tpmEq".to_string(),
            name: "SOL-USDC".to_string(),
            token_x: "So11111111111111111111111111111111111111112".to_string(),
            token_x_symbol: "SOL".to_string(),
            token_y: "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v".to_string(),
            token_y_symbol: "USDC".to_string(),
            tvl: 0.0,
            apy: 0.0,
            fee_rate: 0.25,
            bin_step: 10,
            active_bin_id: 0,
        }]
    }
}

/// Meteora API response types.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct MeteoraApiPool {
    address: String,
    name: String,
    mint_x: String,
    mint_y: String,
    symbol_x: Option<String>,
    symbol_y: Option<String>,
    liquidity: Option<f64>,
    apr: Option<f64>,
    base_fee_percentage: Option<f64>,
    bin_step: Option<u16>,
    active_id: Option<i32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_program_id() {
        assert!(METEORA_DLMM_PROGRAM_ID.parse::<Pubkey>().is_ok());
    }

    #[test]
    fn test_liquidity_strategy_default() {
        assert_eq!(LiquidityStrategy::default(), LiquidityStrategy::Uniform);
    }

    #[test]
    fn test_pool_serialization() {
        let pool = MeteoraPool {
            address: "test".to_string(),
            name: "SOL-USDC".to_string(),
            token_x: "mint_x".to_string(),
            token_x_symbol: "SOL".to_string(),
            token_y: "mint_y".to_string(),
            token_y_symbol: "USDC".to_string(),
            tvl: 1000000.0,
            apy: 25.5,
            fee_rate: 0.25,
            bin_step: 10,
            active_bin_id: 8388608,
        };

        let json = serde_json::to_string(&pool);
        assert!(json.is_ok());
    }
}
