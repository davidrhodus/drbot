//! Solend lending protocol client.
//!
//! Solend is the leading lending protocol on Solana, allowing users to
//! deposit assets to earn yield and borrow against collateral.

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

/// Solend program ID.
pub const SOLEND_PROGRAM_ID: &str = "So1endDq2YkqhipRh3WViPa8hdiSpxWy6z3Z6tMCpAo";

/// Solend API base URL.
const SOLEND_API_URL: &str = "https://api.solend.fi";

/// Solend protocol client.
pub struct SolendClient {
    rpc_client: Arc<RpcClient>,
    http_client: Client,
    program_id: Pubkey,
}

impl SolendClient {
    /// Create a new Solend client.
    pub fn new(rpc_client: Arc<RpcClient>) -> Self {
        Self {
            rpc_client,
            http_client: Client::new(),
            program_id: Pubkey::from_str(SOLEND_PROGRAM_ID).unwrap(),
        }
    }

    /// Get the program ID.
    pub fn program_id(&self) -> &Pubkey {
        &self.program_id
    }

    /// Fetch market data from Solend API.
    async fn fetch_markets(&self) -> Result<Vec<SolendMarket>> {
        let url = format!("{}/v1/markets", SOLEND_API_URL);

        trace!("Fetching Solend markets");

        let response = self.http_client.get(&url).send().await?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(SolanaError::DexScreenerError(format!(
                "Solend API error: {}",
                error_text
            )));
        }

        let markets: SolendMarketsResponse = response.json().await?;

        debug!(count = markets.results.len(), "Fetched Solend markets");

        Ok(markets.results)
    }

    /// Fetch user's obligations (positions).
    async fn fetch_obligations(&self, user: &Pubkey) -> Result<Vec<SolendObligation>> {
        let url = format!("{}/v1/obligations?wallet={}", SOLEND_API_URL, user);

        trace!(user = %user, "Fetching Solend obligations");

        let response = self.http_client.get(&url).send().await?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(SolanaError::DexScreenerError(format!(
                "Solend API error: {}",
                error_text
            )));
        }

        let obligations: SolendObligationsResponse = response.json().await?;

        debug!(
            count = obligations.results.len(),
            "Fetched Solend obligations"
        );

        Ok(obligations.results)
    }

    /// Calculate risk score based on market characteristics.
    fn calculate_risk_score(&self, market: &SolendMarket) -> u8 {
        let mut score = 3u8; // Base score

        // Higher utilization = higher risk
        if market.utilization_rate > 0.9 {
            score += 3;
        } else if market.utilization_rate > 0.8 {
            score += 2;
        } else if market.utilization_rate > 0.7 {
            score += 1;
        }

        // Lower TVL = higher risk
        if market.total_supply_usd < 1_000_000.0 {
            score += 2;
        } else if market.total_supply_usd < 10_000_000.0 {
            score += 1;
        }

        score.min(10)
    }
}

#[async_trait]
impl DeFiProtocol for SolendClient {
    fn name(&self) -> &str {
        "Solend"
    }

    fn protocol_type(&self) -> ProtocolType {
        ProtocolType::Lending
    }

    async fn get_opportunities(&self) -> Result<Vec<YieldOpportunity>> {
        let markets = self.fetch_markets().await?;

        let opportunities = markets
            .into_iter()
            .filter(|m| m.total_supply_usd > 100_000.0) // Filter out tiny markets
            .map(|market| {
                let risk_score = self.calculate_risk_score(&market);
                YieldOpportunity::new(
                    "Solend",
                    &market.reserve_id,
                    &market.symbol,
                    Pubkey::from_str(&market.mint_address).unwrap_or_default(),
                    market.supply_apy,
                    market.total_supply_usd,
                    risk_score,
                )
                .with_metadata(serde_json::json!({
                    "borrow_apy": market.borrow_apy,
                    "utilization_rate": market.utilization_rate,
                    "ltv": market.loan_to_value,
                }))
            })
            .collect();

        Ok(opportunities)
    }

    async fn deposit(&self, params: DepositParams) -> Result<TransactionResult> {
        // In a real implementation, this would:
        // 1. Fetch the reserve account for the asset
        // 2. Build the deposit instruction
        // 3. Sign and send the transaction

        debug!(
            asset = %params.asset_mint,
            amount = params.amount,
            target = %params.target_id,
            "Solend deposit requested"
        );

        // Placeholder - actual implementation would interact with Solend program
        Err(SolanaError::TransactionError(
            "Solend deposit not yet implemented - requires on-chain interaction".to_string(),
        ))
    }

    async fn withdraw(&self, params: WithdrawParams) -> Result<TransactionResult> {
        debug!(
            asset = %params.asset_mint,
            amount = params.amount,
            source = %params.source_id,
            "Solend withdraw requested"
        );

        // Placeholder - actual implementation would interact with Solend program
        Err(SolanaError::TransactionError(
            "Solend withdraw not yet implemented - requires on-chain interaction".to_string(),
        ))
    }

    async fn get_positions(&self, user: &Pubkey) -> Result<Vec<Position>> {
        let obligations = self.fetch_obligations(user).await?;

        let positions = obligations
            .into_iter()
            .flat_map(|ob| {
                let mut positions = Vec::new();

                // Add deposit positions
                for deposit in ob.deposits {
                    positions.push(Position {
                        protocol: "Solend".to_string(),
                        id: format!("{}:{}", ob.obligation_address, deposit.reserve),
                        position_type: PositionType::Supply,
                        asset_mint: Pubkey::from_str(&deposit.mint_address).unwrap_or_default(),
                        asset_symbol: deposit.symbol.clone(),
                        amount: deposit.amount,
                        usd_value: deposit.usd_value,
                        current_apy: deposit.supply_apy,
                        unclaimed_rewards: vec![],
                    });
                }

                // Add borrow positions
                for borrow in ob.borrows {
                    positions.push(Position {
                        protocol: "Solend".to_string(),
                        id: format!("{}:{}", ob.obligation_address, borrow.reserve),
                        position_type: PositionType::Borrow,
                        asset_mint: Pubkey::from_str(&borrow.mint_address).unwrap_or_default(),
                        asset_symbol: borrow.symbol.clone(),
                        amount: borrow.amount,
                        usd_value: borrow.usd_value,
                        current_apy: borrow.borrow_apy,
                        unclaimed_rewards: vec![],
                    });
                }

                positions
            })
            .collect();

        Ok(positions)
    }
}

/// Solend markets API response.
#[derive(Debug, Deserialize)]
struct SolendMarketsResponse {
    results: Vec<SolendMarket>,
}

/// Solend market data.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SolendMarket {
    reserve_id: String,
    symbol: String,
    mint_address: String,
    supply_apy: f64,
    borrow_apy: f64,
    total_supply_usd: f64,
    total_borrow_usd: f64,
    utilization_rate: f64,
    loan_to_value: f64,
}

/// Solend obligations API response.
#[derive(Debug, Deserialize)]
struct SolendObligationsResponse {
    results: Vec<SolendObligation>,
}

/// Solend obligation (user position).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SolendObligation {
    obligation_address: String,
    deposits: Vec<SolendDeposit>,
    borrows: Vec<SolendBorrow>,
}

/// Solend deposit position.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SolendDeposit {
    reserve: String,
    symbol: String,
    mint_address: String,
    amount: u64,
    usd_value: f64,
    supply_apy: f64,
}

/// Solend borrow position.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SolendBorrow {
    reserve: String,
    symbol: String,
    mint_address: String,
    amount: u64,
    usd_value: f64,
    borrow_apy: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_solend_program_id() {
        let client = SolendClient::new(Arc::new(RpcClient::new(
            "https://api.devnet.solana.com".to_string(),
        )));
        assert_eq!(client.program_id().to_string(), SOLEND_PROGRAM_ID);
    }

    #[test]
    fn test_risk_score_calculation() {
        let client = SolendClient::new(Arc::new(RpcClient::new(
            "https://api.devnet.solana.com".to_string(),
        )));

        // Low risk market
        let low_risk = SolendMarket {
            reserve_id: "usdc".to_string(),
            symbol: "USDC".to_string(),
            mint_address: Pubkey::new_unique().to_string(),
            supply_apy: 0.05,
            borrow_apy: 0.08,
            total_supply_usd: 100_000_000.0,
            total_borrow_usd: 50_000_000.0,
            utilization_rate: 0.5,
            loan_to_value: 0.8,
        };
        assert!(client.calculate_risk_score(&low_risk) <= 5);

        // High risk market
        let high_risk = SolendMarket {
            reserve_id: "small".to_string(),
            symbol: "SMALL".to_string(),
            mint_address: Pubkey::new_unique().to_string(),
            supply_apy: 0.20,
            borrow_apy: 0.30,
            total_supply_usd: 500_000.0,
            total_borrow_usd: 450_000.0,
            utilization_rate: 0.95,
            loan_to_value: 0.5,
        };
        assert!(client.calculate_risk_score(&high_risk) >= 6);
    }
}
