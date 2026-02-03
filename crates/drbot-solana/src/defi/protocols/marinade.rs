//! Marinade Finance liquid staking protocol client.
//!
//! Marinade is the leading liquid staking protocol on Solana,
//! allowing users to stake SOL and receive mSOL in return.

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

/// Marinade program ID.
pub const MARINADE_PROGRAM_ID: &str = "MarBmsSgKXdrN1egZf5sqe1TMai9K1rChYNDJgjq7aD";

/// mSOL token mint.
pub const MSOL_MINT: &str = "mSoLzYCxHdYgdzU16g5QSh3i5K3z3KZK7ytfqcJm7So";

/// Marinade API base URL.
const MARINADE_API_URL: &str = "https://api.marinade.finance";

/// Marinade protocol client.
pub struct MarinadeClient {
    rpc_client: Arc<RpcClient>,
    http_client: Client,
    program_id: Pubkey,
    msol_mint: Pubkey,
}

impl MarinadeClient {
    /// Create a new Marinade client.
    pub fn new(rpc_client: Arc<RpcClient>) -> Self {
        Self {
            rpc_client,
            http_client: Client::new(),
            program_id: Pubkey::from_str(MARINADE_PROGRAM_ID).unwrap(),
            msol_mint: Pubkey::from_str(MSOL_MINT).unwrap(),
        }
    }

    /// Get the program ID.
    pub fn program_id(&self) -> &Pubkey {
        &self.program_id
    }

    /// Get the mSOL mint.
    pub fn msol_mint(&self) -> &Pubkey {
        &self.msol_mint
    }

    /// Fetch Marinade statistics.
    async fn fetch_stats(&self) -> Result<MarinadeStats> {
        let url = format!("{}/tlv", MARINADE_API_URL);

        trace!("Fetching Marinade stats");

        let response = self.http_client.get(&url).send().await?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(SolanaError::DexScreenerError(format!(
                "Marinade API error: {}",
                error_text
            )));
        }

        let stats: MarinadeStats = response.json().await?;

        debug!(
            tvl = stats.total_sol,
            apy = stats.msol_apy,
            "Fetched Marinade stats"
        );

        Ok(stats)
    }

    /// Fetch mSOL price.
    async fn fetch_msol_price(&self) -> Result<f64> {
        let url = format!("{}/msol/price_sol", MARINADE_API_URL);

        let response = self.http_client.get(&url).send().await?;

        if !response.status().is_success() {
            return Ok(1.0); // Default to 1:1 if API fails
        }

        let price: MsolPrice = response.json().await?;
        Ok(price.msol_price)
    }

    /// Get SOL price in USD.
    async fn fetch_sol_price(&self) -> Result<f64> {
        // Use a simple price feed - in production would use oracle
        let url = "https://api.coingecko.com/api/v3/simple/price?ids=solana&vs_currencies=usd";

        let response = self.http_client.get(url).send().await?;

        if !response.status().is_success() {
            return Ok(100.0); // Default fallback
        }

        let prices: serde_json::Value = response.json().await?;
        Ok(prices["solana"]["usd"].as_f64().unwrap_or(100.0))
    }
}

#[async_trait]
impl DeFiProtocol for MarinadeClient {
    fn name(&self) -> &str {
        "Marinade"
    }

    fn protocol_type(&self) -> ProtocolType {
        ProtocolType::LiquidStaking
    }

    async fn get_opportunities(&self) -> Result<Vec<YieldOpportunity>> {
        let stats = self.fetch_stats().await?;
        let sol_price = self.fetch_sol_price().await.unwrap_or(100.0);

        let tvl_usd = stats.total_sol * sol_price;

        // Marinade is considered low risk - established protocol, liquid token
        let opportunity = YieldOpportunity::new(
            "Marinade",
            "msol-stake",
            "SOL → mSOL",
            self.msol_mint,
            stats.msol_apy / 100.0, // Convert from percentage
            tvl_usd,
            2, // Low risk score
        )
        .with_metadata(serde_json::json!({
            "total_sol_staked": stats.total_sol,
            "validators_count": stats.validators_count,
            "msol_supply": stats.msol_supply,
            "instant_unstake_fee": stats.instant_unstake_fee,
            "delayed_unstake_epochs": 2,
        }));

        Ok(vec![opportunity])
    }

    async fn deposit(&self, params: DepositParams) -> Result<TransactionResult> {
        debug!(
            asset = %params.asset_mint,
            amount = params.amount,
            "Marinade stake requested"
        );

        // In production, this would:
        // 1. Create a stake instruction
        // 2. Sign and send transaction
        // 3. Return mSOL to user

        Err(SolanaError::TransactionError(
            "Marinade stake not yet implemented - requires on-chain interaction".to_string(),
        ))
    }

    async fn withdraw(&self, params: WithdrawParams) -> Result<TransactionResult> {
        debug!(
            asset = %params.asset_mint,
            amount = params.amount,
            "Marinade unstake requested"
        );

        // Marinade supports both instant and delayed unstake
        // Instant: ~0.3% fee
        // Delayed: 2 epochs, no fee

        Err(SolanaError::TransactionError(
            "Marinade unstake not yet implemented - requires on-chain interaction".to_string(),
        ))
    }

    async fn get_positions(&self, user: &Pubkey) -> Result<Vec<Position>> {
        // Check user's mSOL balance
        let msol_account =
            spl_associated_token_account::get_associated_token_address(user, &self.msol_mint);

        let balance = match self
            .rpc_client
            .get_token_account_balance(&msol_account)
            .await
        {
            Ok(balance) => balance.amount.parse::<u64>().unwrap_or(0),
            Err(_) => 0,
        };

        if balance == 0 {
            return Ok(vec![]);
        }

        let msol_price = self.fetch_msol_price().await.unwrap_or(1.0);
        let sol_price = self.fetch_sol_price().await.unwrap_or(100.0);
        let stats = self.fetch_stats().await?;

        let sol_value = (balance as f64 / 1e9) * msol_price;
        let usd_value = sol_value * sol_price;

        let position = Position {
            protocol: "Marinade".to_string(),
            id: msol_account.to_string(),
            position_type: PositionType::Stake,
            asset_mint: self.msol_mint,
            asset_symbol: "mSOL".to_string(),
            amount: balance,
            usd_value,
            current_apy: stats.msol_apy / 100.0,
            unclaimed_rewards: vec![], // mSOL appreciation is automatic
        };

        Ok(vec![position])
    }
}

/// Marinade statistics.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MarinadeStats {
    total_sol: f64,
    msol_supply: f64,
    msol_apy: f64,
    validators_count: u32,
    #[serde(default)]
    instant_unstake_fee: f64,
}

/// mSOL price response.
#[derive(Debug, Deserialize)]
struct MsolPrice {
    msol_price: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_marinade_program_id() {
        let client = MarinadeClient::new(Arc::new(RpcClient::new(
            "https://api.devnet.solana.com".to_string(),
        )));
        assert_eq!(client.program_id().to_string(), MARINADE_PROGRAM_ID);
    }

    #[test]
    fn test_msol_mint() {
        let client = MarinadeClient::new(Arc::new(RpcClient::new(
            "https://api.devnet.solana.com".to_string(),
        )));
        assert_eq!(client.msol_mint().to_string(), MSOL_MINT);
    }
}
