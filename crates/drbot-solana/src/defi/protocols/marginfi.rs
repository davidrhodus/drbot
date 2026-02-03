//! Marginfi lending protocol client.
//!
//! Marginfi is a decentralized lending protocol on Solana with
//! cross-collateralization and isolated risk pools.

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

/// Marginfi program ID.
pub const MARGINFI_PROGRAM_ID: &str = "MFv2hWf31Z9kbCa1snEPYctwafyhdvnV7FZnsebVacA";

/// Marginfi API base URL.
const MARGINFI_API_URL: &str = "https://api.marginfi.com";

/// Marginfi protocol client.
pub struct MarginfiClient {
    rpc_client: Arc<RpcClient>,
    http_client: Client,
    program_id: Pubkey,
}

impl MarginfiClient {
    /// Create a new Marginfi client.
    pub fn new(rpc_client: Arc<RpcClient>) -> Self {
        Self {
            rpc_client,
            http_client: Client::new(),
            program_id: Pubkey::from_str(MARGINFI_PROGRAM_ID).unwrap(),
        }
    }

    /// Get the program ID.
    pub fn program_id(&self) -> &Pubkey {
        &self.program_id
    }

    /// Fetch bank data from Marginfi.
    async fn fetch_banks(&self) -> Result<Vec<MarginfiBank>> {
        let url = format!("{}/banks", MARGINFI_API_URL);

        trace!("Fetching Marginfi banks");

        let response = self.http_client.get(&url).send().await?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(SolanaError::DexScreenerError(format!(
                "Marginfi API error: {}",
                error_text
            )));
        }

        let banks: Vec<MarginfiBank> = response.json().await?;

        debug!(count = banks.len(), "Fetched Marginfi banks");

        Ok(banks)
    }

    /// Fetch user's marginfi account.
    async fn fetch_account(&self, user: &Pubkey) -> Result<Option<MarginfiAccount>> {
        let url = format!("{}/accounts?wallet={}", MARGINFI_API_URL, user);

        trace!(user = %user, "Fetching Marginfi account");

        let response = self.http_client.get(&url).send().await?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(SolanaError::DexScreenerError(format!(
                "Marginfi API error: {}",
                error_text
            )));
        }

        let accounts: Vec<MarginfiAccount> = response.json().await?;

        Ok(accounts.into_iter().next())
    }

    /// Calculate risk score based on bank characteristics.
    fn calculate_risk_score(&self, bank: &MarginfiBank) -> u8 {
        let mut score = 3u8;

        // Higher utilization = higher risk
        let utilization = if bank.total_deposits > 0.0 {
            bank.total_borrows / bank.total_deposits
        } else {
            0.0
        };

        if utilization > 0.9 {
            score += 3;
        } else if utilization > 0.8 {
            score += 2;
        } else if utilization > 0.7 {
            score += 1;
        }

        // Lower TVL = higher risk
        if bank.total_deposits_usd < 1_000_000.0 {
            score += 2;
        } else if bank.total_deposits_usd < 10_000_000.0 {
            score += 1;
        }

        // Isolated assets are higher risk
        if bank.isolated {
            score += 1;
        }

        score.min(10)
    }
}

#[async_trait]
impl DeFiProtocol for MarginfiClient {
    fn name(&self) -> &str {
        "Marginfi"
    }

    fn protocol_type(&self) -> ProtocolType {
        ProtocolType::Lending
    }

    async fn get_opportunities(&self) -> Result<Vec<YieldOpportunity>> {
        let banks = self.fetch_banks().await?;

        let opportunities = banks
            .into_iter()
            .filter(|b| b.total_deposits_usd > 100_000.0)
            .map(|bank| {
                let risk_score = self.calculate_risk_score(&bank);
                YieldOpportunity::new(
                    "Marginfi",
                    &bank.address,
                    &bank.symbol,
                    Pubkey::from_str(&bank.mint).unwrap_or_default(),
                    bank.lending_rate,
                    bank.total_deposits_usd,
                    risk_score,
                )
                .with_metadata(serde_json::json!({
                    "borrow_rate": bank.borrow_rate,
                    "emissions_rate": bank.emissions_rate,
                    "isolated": bank.isolated,
                    "weight": bank.asset_weight,
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
            "Marginfi deposit requested"
        );

        // Placeholder - actual implementation would interact with Marginfi program
        Err(SolanaError::TransactionError(
            "Marginfi deposit not yet implemented - requires on-chain interaction".to_string(),
        ))
    }

    async fn withdraw(&self, params: WithdrawParams) -> Result<TransactionResult> {
        debug!(
            asset = %params.asset_mint,
            amount = params.amount,
            source = %params.source_id,
            "Marginfi withdraw requested"
        );

        // Placeholder - actual implementation would interact with Marginfi program
        Err(SolanaError::TransactionError(
            "Marginfi withdraw not yet implemented - requires on-chain interaction".to_string(),
        ))
    }

    async fn get_positions(&self, user: &Pubkey) -> Result<Vec<Position>> {
        let account = self.fetch_account(user).await?;

        let positions = match account {
            Some(acc) => acc
                .balances
                .into_iter()
                .map(|bal| Position {
                    protocol: "Marginfi".to_string(),
                    id: format!("{}:{}", acc.address, bal.bank_address),
                    position_type: if bal.amount >= 0.0 {
                        PositionType::Supply
                    } else {
                        PositionType::Borrow
                    },
                    asset_mint: Pubkey::from_str(&bal.mint).unwrap_or_default(),
                    asset_symbol: bal.symbol.clone(),
                    amount: bal.amount.abs() as u64,
                    usd_value: bal.usd_value.abs(),
                    current_apy: if bal.amount >= 0.0 {
                        bal.lending_rate
                    } else {
                        bal.borrow_rate
                    },
                    unclaimed_rewards: vec![],
                })
                .collect(),
            None => vec![],
        };

        Ok(positions)
    }
}

/// Marginfi bank data.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MarginfiBank {
    address: String,
    mint: String,
    symbol: String,
    lending_rate: f64,
    borrow_rate: f64,
    emissions_rate: f64,
    total_deposits: f64,
    total_deposits_usd: f64,
    total_borrows: f64,
    total_borrows_usd: f64,
    asset_weight: f64,
    liability_weight: f64,
    isolated: bool,
}

/// Marginfi user account.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MarginfiAccount {
    address: String,
    authority: String,
    balances: Vec<MarginfiBalance>,
    health_factor: f64,
}

/// Marginfi balance entry.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MarginfiBalance {
    bank_address: String,
    mint: String,
    symbol: String,
    amount: f64,
    usd_value: f64,
    lending_rate: f64,
    borrow_rate: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_marginfi_program_id() {
        let client = MarginfiClient::new(Arc::new(RpcClient::new(
            "https://api.devnet.solana.com".to_string(),
        )));
        assert_eq!(client.program_id().to_string(), MARGINFI_PROGRAM_ID);
    }
}
