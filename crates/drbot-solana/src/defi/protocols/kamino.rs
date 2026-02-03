//! Kamino Finance vault protocol client.
//!
//! Kamino provides automated liquidity vaults that optimize yield
//! through concentrated liquidity management on Solana DEXes.

use super::{
    DeFiAction, DeFiProtocol, DepositParams, Position, PositionType, ProtocolType,
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

/// Kamino program ID.
pub const KAMINO_PROGRAM_ID: &str = "KLend2g3cP87ber41GFZMoD7yPu45R8LRFH9Wb9fjXp";

/// Kamino API base URL.
const KAMINO_API_URL: &str = "https://api.kamino.finance";

/// Kamino protocol client.
pub struct KaminoClient {
    rpc_client: Arc<RpcClient>,
    http_client: Client,
    program_id: Pubkey,
}

impl KaminoClient {
    /// Create a new Kamino client.
    pub fn new(rpc_client: Arc<RpcClient>) -> Self {
        Self {
            rpc_client,
            http_client: Client::new(),
            program_id: Pubkey::from_str(KAMINO_PROGRAM_ID).unwrap(),
        }
    }

    /// Get the program ID.
    pub fn program_id(&self) -> &Pubkey {
        &self.program_id
    }

    /// Fetch vault data from Kamino.
    async fn fetch_vaults(&self) -> Result<Vec<KaminoVault>> {
        let url = format!("{}/strategies", KAMINO_API_URL);

        trace!("Fetching Kamino vaults");

        let response = self.http_client.get(&url).send().await?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(SolanaError::DexScreenerError(format!(
                "Kamino API error: {}",
                error_text
            )));
        }

        let vaults: Vec<KaminoVault> = response.json().await?;

        debug!(count = vaults.len(), "Fetched Kamino vaults");

        Ok(vaults)
    }

    /// Fetch user's vault positions.
    async fn fetch_positions(&self, user: &Pubkey) -> Result<Vec<KaminoPosition>> {
        let url = format!("{}/positions?wallet={}", KAMINO_API_URL, user);

        trace!(user = %user, "Fetching Kamino positions");

        let response = self.http_client.get(&url).send().await?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(SolanaError::DexScreenerError(format!(
                "Kamino API error: {}",
                error_text
            )));
        }

        let positions: Vec<KaminoPosition> = response.json().await?;

        debug!(count = positions.len(), "Fetched Kamino positions");

        Ok(positions)
    }

    /// Calculate risk score for a vault.
    fn calculate_risk_score(&self, vault: &KaminoVault) -> u8 {
        let mut score = 4u8; // Base score slightly higher for complex strategies

        // Lower TVL = higher risk
        if vault.tvl_usd < 500_000.0 {
            score += 2;
        } else if vault.tvl_usd < 5_000_000.0 {
            score += 1;
        }

        // Volatile pairs = higher risk
        if vault.token_a_symbol != "USDC"
            && vault.token_a_symbol != "USDT"
            && vault.token_b_symbol != "USDC"
            && vault.token_b_symbol != "USDT"
        {
            score += 2;
        }

        // Wider range = lower risk (less impermanent loss)
        if vault.range_width < 0.1 {
            score += 1;
        }

        score.min(10)
    }
}

#[async_trait]
impl DeFiProtocol for KaminoClient {
    fn name(&self) -> &str {
        "Kamino"
    }

    fn protocol_type(&self) -> ProtocolType {
        ProtocolType::Vault
    }

    async fn get_opportunities(&self) -> Result<Vec<YieldOpportunity>> {
        let vaults = self.fetch_vaults().await?;

        let opportunities = vaults
            .into_iter()
            .filter(|v| v.tvl_usd > 100_000.0 && v.status == "active")
            .map(|vault| {
                let risk_score = self.calculate_risk_score(&vault);
                let asset_name = format!("{}-{}", vault.token_a_symbol, vault.token_b_symbol);

                YieldOpportunity::new(
                    "Kamino",
                    &vault.address,
                    &asset_name,
                    Pubkey::from_str(&vault.share_mint).unwrap_or_default(),
                    vault.apy,
                    vault.tvl_usd,
                    risk_score,
                )
                .with_metadata(serde_json::json!({
                    "token_a": vault.token_a_symbol,
                    "token_b": vault.token_b_symbol,
                    "dex": vault.dex,
                    "range_width": vault.range_width,
                    "fees_apy": vault.fees_apy,
                    "rewards_apy": vault.rewards_apy,
                }))
            })
            .collect();

        Ok(opportunities)
    }

    async fn deposit(&self, params: DepositParams) -> Result<TransactionResult> {
        debug!(
            asset = %params.asset_mint,
            amount = params.amount,
            target = %params.target_id,
            "Kamino deposit requested"
        );

        // Placeholder - actual implementation would interact with Kamino program
        Err(SolanaError::TransactionError(
            "Kamino deposit not yet implemented - requires on-chain interaction".to_string(),
        ))
    }

    async fn withdraw(&self, params: WithdrawParams) -> Result<TransactionResult> {
        debug!(
            asset = %params.asset_mint,
            amount = params.amount,
            source = %params.source_id,
            "Kamino withdraw requested"
        );

        // Placeholder - actual implementation would interact with Kamino program
        Err(SolanaError::TransactionError(
            "Kamino withdraw not yet implemented - requires on-chain interaction".to_string(),
        ))
    }

    async fn get_positions(&self, user: &Pubkey) -> Result<Vec<Position>> {
        let positions = self.fetch_positions(user).await?;

        let result = positions
            .into_iter()
            .map(|pos| {
                let asset_name = format!("{}-{}", pos.token_a_symbol, pos.token_b_symbol);
                Position {
                    protocol: "Kamino".to_string(),
                    id: pos.position_address.clone(),
                    position_type: PositionType::Vault,
                    asset_mint: Pubkey::from_str(&pos.share_mint).unwrap_or_default(),
                    asset_symbol: asset_name,
                    amount: pos.shares,
                    usd_value: pos.usd_value,
                    current_apy: pos.current_apy,
                    unclaimed_rewards: vec![],
                }
            })
            .collect();

        Ok(result)
    }
}

/// Kamino vault data.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct KaminoVault {
    address: String,
    share_mint: String,
    token_a_mint: String,
    token_a_symbol: String,
    token_b_mint: String,
    token_b_symbol: String,
    dex: String,
    apy: f64,
    fees_apy: f64,
    rewards_apy: f64,
    tvl_usd: f64,
    range_width: f64,
    status: String,
}

/// Kamino user position.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct KaminoPosition {
    position_address: String,
    strategy_address: String,
    share_mint: String,
    token_a_symbol: String,
    token_b_symbol: String,
    shares: u64,
    usd_value: f64,
    current_apy: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kamino_program_id() {
        let client = KaminoClient::new(Arc::new(RpcClient::new(
            "https://api.devnet.solana.com".to_string(),
        )));
        assert_eq!(client.program_id().to_string(), KAMINO_PROGRAM_ID);
    }
}
