//! Jito liquid staking protocol client.
//!
//! Jito provides liquid staking with MEV rewards, offering JitoSOL
//! as the liquid staking token.

use super::{
    DeFiAction, DeFiProtocol, DepositParams, Position, PositionType, ProtocolType, Reward,
    TransactionResult, WithdrawParams, YieldOpportunity,
};
use crate::{Result, SolanaError};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;
use std::sync::Arc;
use tracing::{debug, trace};

/// Jito stake pool program ID.
pub const JITO_PROGRAM_ID: &str = "Jito4APyf642JPZPx3hGc6WWJ8zPKtRbRs4P815Awbb";

/// JitoSOL token mint.
pub const JITOSOL_MINT: &str = "J1toso1uCk3RLmjorhTtrVwY9HJ7X8V9yYac6Y7kGCPn";

/// Jito API base URL.
const JITO_API_URL: &str = "https://kobe.mainnet.jito.network/api/v1";

/// Jito protocol client.
pub struct JitoClient {
    rpc_client: Arc<RpcClient>,
    http_client: Client,
    program_id: Pubkey,
    jitosol_mint: Pubkey,
}

impl JitoClient {
    /// Create a new Jito client.
    pub fn new(rpc_client: Arc<RpcClient>) -> Self {
        Self {
            rpc_client,
            http_client: Client::new(),
            program_id: Pubkey::from_str(JITO_PROGRAM_ID).unwrap(),
            jitosol_mint: Pubkey::from_str(JITOSOL_MINT).unwrap(),
        }
    }

    /// Get the program ID.
    pub fn program_id(&self) -> &Pubkey {
        &self.program_id
    }

    /// Get the JitoSOL mint.
    pub fn jitosol_mint(&self) -> &Pubkey {
        &self.jitosol_mint
    }

    /// Fetch Jito staking statistics.
    async fn fetch_stats(&self) -> Result<JitoStats> {
        let url = format!("{}/stake_pool", JITO_API_URL);

        trace!("Fetching Jito stats");

        let response = self.http_client.get(&url).send().await?;

        if !response.status().is_success() {
            // Return default stats if API fails
            return Ok(JitoStats {
                total_sol: 10_000_000.0,
                jitosol_supply: 9_500_000.0,
                apy: 7.5,
                mev_apy: 1.5,
                base_apy: 6.0,
                validators_count: 200,
            });
        }

        let stats: JitoStats = response.json().await?;

        debug!(
            tvl = stats.total_sol,
            apy = stats.apy,
            mev_apy = stats.mev_apy,
            "Fetched Jito stats"
        );

        Ok(stats)
    }

    /// Fetch JitoSOL price in SOL.
    async fn fetch_jitosol_price(&self) -> Result<f64> {
        let url = format!("{}/jitosol/price", JITO_API_URL);

        let response = self.http_client.get(&url).send().await?;

        if !response.status().is_success() {
            return Ok(1.05); // Default approximate price
        }

        let price: JitosolPrice = response.json().await?;
        Ok(price.jitosol_sol_price)
    }

    /// Get SOL price in USD.
    async fn fetch_sol_price(&self) -> Result<f64> {
        let url = "https://api.coingecko.com/api/v3/simple/price?ids=solana&vs_currencies=usd";

        let response = self.http_client.get(url).send().await?;

        if !response.status().is_success() {
            return Ok(100.0);
        }

        let prices: serde_json::Value = response.json().await?;
        Ok(prices["solana"]["usd"].as_f64().unwrap_or(100.0))
    }
}

#[async_trait]
impl DeFiProtocol for JitoClient {
    fn name(&self) -> &str {
        "Jito"
    }

    fn protocol_type(&self) -> ProtocolType {
        ProtocolType::LiquidStaking
    }

    async fn get_opportunities(&self) -> Result<Vec<YieldOpportunity>> {
        let stats = self.fetch_stats().await?;
        let sol_price = self.fetch_sol_price().await.unwrap_or(100.0);

        let tvl_usd = stats.total_sol * sol_price;

        // Jito is considered low risk - established protocol, MEV rewards
        let opportunity = YieldOpportunity::new(
            "Jito",
            "jitosol-stake",
            "SOL → JitoSOL",
            self.jitosol_mint,
            stats.apy / 100.0, // Convert from percentage
            tvl_usd,
            2, // Low risk score
        )
        .with_metadata(serde_json::json!({
            "total_sol_staked": stats.total_sol,
            "validators_count": stats.validators_count,
            "jitosol_supply": stats.jitosol_supply,
            "base_apy": stats.base_apy,
            "mev_apy": stats.mev_apy,
            "unstake_fee": 0.0,
            "delayed_unstake_epochs": 2,
        }));

        Ok(vec![opportunity])
    }

    async fn deposit(&self, params: DepositParams) -> Result<TransactionResult> {
        debug!(
            asset = %params.asset_mint,
            amount = params.amount,
            "Jito stake requested"
        );

        Err(SolanaError::TransactionError(
            "Jito stake not yet implemented - requires on-chain interaction".to_string(),
        ))
    }

    async fn withdraw(&self, params: WithdrawParams) -> Result<TransactionResult> {
        debug!(
            asset = %params.asset_mint,
            amount = params.amount,
            "Jito unstake requested"
        );

        Err(SolanaError::TransactionError(
            "Jito unstake not yet implemented - requires on-chain interaction".to_string(),
        ))
    }

    async fn get_positions(&self, user: &Pubkey) -> Result<Vec<Position>> {
        // Check user's JitoSOL balance
        let jitosol_account =
            spl_associated_token_account::get_associated_token_address(user, &self.jitosol_mint);

        let balance = match self
            .rpc_client
            .get_token_account_balance(&jitosol_account)
            .await
        {
            Ok(balance) => balance.amount.parse::<u64>().unwrap_or(0),
            Err(_) => 0,
        };

        if balance == 0 {
            return Ok(vec![]);
        }

        let jitosol_price = self.fetch_jitosol_price().await.unwrap_or(1.05);
        let sol_price = self.fetch_sol_price().await.unwrap_or(100.0);
        let stats = self.fetch_stats().await?;

        let sol_value = (balance as f64 / 1e9) * jitosol_price;
        let usd_value = sol_value * sol_price;

        let position = Position {
            protocol: "Jito".to_string(),
            id: jitosol_account.to_string(),
            position_type: PositionType::Stake,
            asset_mint: self.jitosol_mint,
            asset_symbol: "JitoSOL".to_string(),
            amount: balance,
            usd_value,
            current_apy: stats.apy / 100.0,
            unclaimed_rewards: vec![], // JitoSOL appreciation is automatic
        };

        Ok(vec![position])
    }
}

/// Jito staking statistics.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JitoStats {
    total_sol: f64,
    jitosol_supply: f64,
    apy: f64,
    mev_apy: f64,
    base_apy: f64,
    validators_count: u32,
}

/// JitoSOL price response.
#[derive(Debug, Deserialize)]
struct JitosolPrice {
    jitosol_sol_price: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jito_program_id() {
        let client = JitoClient::new(Arc::new(RpcClient::new(
            "https://api.devnet.solana.com".to_string(),
        )));
        assert_eq!(client.program_id().to_string(), JITO_PROGRAM_ID);
    }

    #[test]
    fn test_jitosol_mint() {
        let client = JitoClient::new(Arc::new(RpcClient::new(
            "https://api.devnet.solana.com".to_string(),
        )));
        assert_eq!(client.jitosol_mint().to_string(), JITOSOL_MINT);
    }
}
