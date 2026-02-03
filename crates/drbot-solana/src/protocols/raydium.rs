//! Raydium AMM integration for liquidity provision.
//!
//! Provides access to Raydium's AMM and concentrated liquidity pools.

use crate::{Result, SolanaError};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;
use std::sync::Arc;
use tracing::{debug, info};

/// Raydium AMM program ID.
pub const RAYDIUM_AMM_PROGRAM_ID: &str = "675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8";

/// Raydium CLMM program ID.
pub const RAYDIUM_CLMM_PROGRAM_ID: &str = "CAMMCzo5YL8w4VFF8KVHrK22GGUsp5VTaW7grrKgrWqK";

/// Raydium API base URL.
const RAYDIUM_API_URL: &str = "https://api.raydium.io/v2";

/// Raydium liquidity pool information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RaydiumPool {
    /// Pool ID (AMM ID).
    pub id: String,
    /// Pool name (e.g., "SOL-USDC").
    pub name: String,
    /// Token A mint address.
    pub token_a: String,
    /// Token A symbol.
    pub token_a_symbol: String,
    /// Token B mint address.
    pub token_b: String,
    /// Token B symbol.
    pub token_b_symbol: String,
    /// Total value locked in USD.
    pub tvl: f64,
    /// Annual percentage yield.
    pub apy: f64,
    /// 24h trading volume in USD.
    pub volume_24h: f64,
    /// Fee rate (e.g., 0.25 for 0.25%).
    pub fee_rate: f64,
}

/// User LP position information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LpPosition {
    /// Pool ID.
    pub pool_id: String,
    /// Pool name.
    pub pool_name: String,
    /// LP token balance.
    pub lp_balance: f64,
    /// Share of pool.
    pub pool_share: f64,
    /// Token A amount.
    pub token_a_amount: f64,
    /// Token B amount.
    pub token_b_amount: f64,
    /// Position value in USD.
    pub value_usd: f64,
}

/// Parameters for adding liquidity.
#[derive(Debug, Clone)]
pub struct AddLiquidityParams {
    /// Pool ID.
    pub pool: String,
    /// Amount of token A.
    pub amount_a: f64,
    /// Amount of token B.
    pub amount_b: f64,
    /// Slippage tolerance (e.g., 0.5 for 0.5%).
    pub slippage: f64,
}

/// Parameters for removing liquidity.
#[derive(Debug, Clone)]
pub struct RemoveLiquidityParams {
    /// Pool ID.
    pub pool: String,
    /// LP token amount to remove.
    pub lp_amount: f64,
    /// Slippage tolerance.
    pub slippage: f64,
}

/// Raydium AMM client.
pub struct RaydiumClient {
    rpc_client: Arc<RpcClient>,
    http_client: Client,
    amm_program_id: Pubkey,
    clmm_program_id: Pubkey,
}

impl RaydiumClient {
    /// Create a new Raydium client.
    pub fn new(rpc_client: Arc<RpcClient>) -> Self {
        Self {
            rpc_client,
            http_client: Client::new(),
            amm_program_id: RAYDIUM_AMM_PROGRAM_ID.parse().unwrap(),
            clmm_program_id: RAYDIUM_CLMM_PROGRAM_ID.parse().unwrap(),
        }
    }

    /// Get the AMM program ID.
    pub fn amm_program_id(&self) -> &Pubkey {
        &self.amm_program_id
    }

    /// Get the CLMM program ID.
    pub fn clmm_program_id(&self) -> &Pubkey {
        &self.clmm_program_id
    }

    /// Get all available pools.
    pub async fn get_pools(&self) -> Result<Vec<RaydiumPool>> {
        debug!("Fetching Raydium pools");

        let url = format!("{}/main/pairs", RAYDIUM_API_URL);

        let response = self.http_client.get(&url).send().await?;

        if !response.status().is_success() {
            // Return some hardcoded popular pools as fallback
            return Ok(self.default_pools());
        }

        let data: Vec<RaydiumApiPool> = response.json().await.unwrap_or_default();

        let pools: Vec<RaydiumPool> = data
            .into_iter()
            .take(100)
            .map(|p| RaydiumPool {
                id: p.amm_id,
                name: p.name,
                token_a: p.base_mint,
                token_a_symbol: p.base_symbol.unwrap_or_default(),
                token_b: p.quote_mint,
                token_b_symbol: p.quote_symbol.unwrap_or_default(),
                tvl: p.liquidity.unwrap_or(0.0),
                apy: p.apr_24h.unwrap_or(0.0),
                volume_24h: p.volume_24h.unwrap_or(0.0),
                fee_rate: 0.25,
            })
            .collect();

        debug!(count = pools.len(), "Fetched Raydium pools");
        Ok(pools)
    }

    /// Get a specific pool by ID.
    pub async fn get_pool(&self, id: &str) -> Result<Option<RaydiumPool>> {
        let pools = self.get_pools().await?;
        Ok(pools.into_iter().find(|p| p.id == id))
    }

    /// Search pools by token.
    pub async fn search_pools(&self, token: &str) -> Result<Vec<RaydiumPool>> {
        let pools = self.get_pools().await?;
        let token_upper = token.to_uppercase();

        Ok(pools
            .into_iter()
            .filter(|p| {
                p.token_a_symbol.to_uppercase().contains(&token_upper)
                    || p.token_b_symbol.to_uppercase().contains(&token_upper)
                    || p.name.to_uppercase().contains(&token_upper)
            })
            .collect())
    }

    /// Get user LP positions.
    pub async fn get_positions(&self, _user: &Pubkey) -> Result<Vec<LpPosition>> {
        // In production, would query on-chain LP token balances
        Ok(vec![])
    }

    /// Get LP balance for a specific pool.
    pub async fn get_lp_balance(&self, _user: &Pubkey, _pool: &str) -> Result<f64> {
        // In production, would query LP token balance
        Ok(0.0)
    }

    /// Add liquidity to a pool.
    pub async fn add_liquidity(&self, params: AddLiquidityParams) -> Result<String> {
        info!(
            pool = %params.pool,
            amount_a = params.amount_a,
            amount_b = params.amount_b,
            "Raydium add liquidity requested"
        );

        Err(SolanaError::DeFiProtocolError(
            "Raydium add_liquidity not yet implemented - requires on-chain interaction".to_string(),
        ))
    }

    /// Remove liquidity from a pool.
    pub async fn remove_liquidity(&self, params: RemoveLiquidityParams) -> Result<String> {
        info!(
            pool = %params.pool,
            lp_amount = params.lp_amount,
            "Raydium remove liquidity requested"
        );

        Err(SolanaError::DeFiProtocolError(
            "Raydium remove_liquidity not yet implemented - requires on-chain interaction"
                .to_string(),
        ))
    }

    /// Get default/popular pools.
    fn default_pools(&self) -> Vec<RaydiumPool> {
        vec![
            RaydiumPool {
                id: "58oQChx4yWmvKdwLLZzBi4ChoCc2fqCUWBkwMihLYQo2".to_string(),
                name: "SOL-USDC".to_string(),
                token_a: "So11111111111111111111111111111111111111112".to_string(),
                token_a_symbol: "SOL".to_string(),
                token_b: "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v".to_string(),
                token_b_symbol: "USDC".to_string(),
                tvl: 0.0,
                apy: 0.0,
                volume_24h: 0.0,
                fee_rate: 0.25,
            },
            RaydiumPool {
                id: "HZtSsGMWKnSAR2S5dqmwkDYvzUhVWVgPCKsHVRBdGRZM".to_string(),
                name: "RAY-USDC".to_string(),
                token_a: "4k3Dyjzvzp8eMZWUXbBCjEvwSkkk59S5iCNLY3QrkX6R".to_string(),
                token_a_symbol: "RAY".to_string(),
                token_b: "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v".to_string(),
                token_b_symbol: "USDC".to_string(),
                tvl: 0.0,
                apy: 0.0,
                volume_24h: 0.0,
                fee_rate: 0.25,
            },
        ]
    }
}

/// Raydium API response types.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RaydiumApiPool {
    amm_id: String,
    name: String,
    base_mint: String,
    quote_mint: String,
    base_symbol: Option<String>,
    quote_symbol: Option<String>,
    liquidity: Option<f64>,
    apr_24h: Option<f64>,
    volume_24h: Option<f64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_program_ids() {
        assert!(RAYDIUM_AMM_PROGRAM_ID.parse::<Pubkey>().is_ok());
        assert!(RAYDIUM_CLMM_PROGRAM_ID.parse::<Pubkey>().is_ok());
    }

    #[test]
    fn test_pool_serialization() {
        let pool = RaydiumPool {
            id: "test".to_string(),
            name: "SOL-USDC".to_string(),
            token_a: "mint_a".to_string(),
            token_a_symbol: "SOL".to_string(),
            token_b: "mint_b".to_string(),
            token_b_symbol: "USDC".to_string(),
            tvl: 1000000.0,
            apy: 15.5,
            volume_24h: 500000.0,
            fee_rate: 0.25,
        };

        let json = serde_json::to_string(&pool);
        assert!(json.is_ok());
    }
}
