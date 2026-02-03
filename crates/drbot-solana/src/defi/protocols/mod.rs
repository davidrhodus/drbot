//! DeFi protocol clients.
//!
//! Provides a unified interface for interacting with various Solana DeFi protocols.

mod jito;
mod kamino;
mod marginfi;
mod marinade;
mod solend;

pub use jito::*;
pub use kamino::*;
pub use marginfi::*;
pub use marinade::*;
pub use solend::*;

use crate::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;

/// Common trait for DeFi protocol clients.
#[async_trait]
pub trait DeFiProtocol: Send + Sync {
    /// Get the protocol name.
    fn name(&self) -> &str;

    /// Get the protocol type.
    fn protocol_type(&self) -> ProtocolType;

    /// Get available yield opportunities from this protocol.
    async fn get_opportunities(&self) -> Result<Vec<YieldOpportunity>>;

    /// Deposit assets into the protocol.
    async fn deposit(&self, params: DepositParams) -> Result<TransactionResult>;

    /// Withdraw assets from the protocol.
    async fn withdraw(&self, params: WithdrawParams) -> Result<TransactionResult>;

    /// Get user's current positions in this protocol.
    async fn get_positions(&self, user: &Pubkey) -> Result<Vec<Position>>;
}

/// Protocol types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolType {
    /// Lending protocol (Solend, Marginfi).
    Lending,
    /// Vault/yield aggregator (Kamino).
    Vault,
    /// Liquid staking (Marinade, Jito).
    LiquidStaking,
}

/// A yield opportunity from a DeFi protocol.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YieldOpportunity {
    /// Protocol name.
    pub protocol: String,
    /// Opportunity identifier.
    pub id: String,
    /// Asset name.
    pub asset: String,
    /// Asset mint address.
    pub asset_mint: Pubkey,
    /// Annual percentage yield.
    pub apy: f64,
    /// Total value locked in USD.
    pub tvl_usd: f64,
    /// Risk score (1-10, where 10 is highest risk).
    pub risk_score: u8,
    /// Additional metadata.
    #[serde(default)]
    pub metadata: serde_json::Value,
}

impl YieldOpportunity {
    /// Create a new yield opportunity.
    pub fn new(
        protocol: impl Into<String>,
        id: impl Into<String>,
        asset: impl Into<String>,
        asset_mint: Pubkey,
        apy: f64,
        tvl_usd: f64,
        risk_score: u8,
    ) -> Self {
        Self {
            protocol: protocol.into(),
            id: id.into(),
            asset: asset.into(),
            asset_mint,
            apy,
            tvl_usd,
            risk_score,
            metadata: serde_json::Value::Null,
        }
    }

    /// Set metadata.
    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = metadata;
        self
    }
}

/// Parameters for depositing into a DeFi protocol.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepositParams {
    /// Asset mint to deposit.
    pub asset_mint: Pubkey,
    /// Amount to deposit (in smallest units).
    pub amount: u64,
    /// Target pool/vault/market identifier.
    pub target_id: String,
    /// Maximum acceptable slippage in basis points.
    pub max_slippage_bps: u16,
}

impl DepositParams {
    /// Create new deposit parameters.
    pub fn new(asset_mint: Pubkey, amount: u64, target_id: impl Into<String>) -> Self {
        Self {
            asset_mint,
            amount,
            target_id: target_id.into(),
            max_slippage_bps: 50,
        }
    }

    /// Set max slippage.
    pub fn with_slippage(mut self, bps: u16) -> Self {
        self.max_slippage_bps = bps;
        self
    }
}

/// Parameters for withdrawing from a DeFi protocol.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WithdrawParams {
    /// Asset mint to withdraw.
    pub asset_mint: Pubkey,
    /// Amount to withdraw (in smallest units). Use u64::MAX for max.
    pub amount: u64,
    /// Source pool/vault/market identifier.
    pub source_id: String,
    /// Maximum acceptable slippage in basis points.
    pub max_slippage_bps: u16,
}

impl WithdrawParams {
    /// Create new withdraw parameters.
    pub fn new(asset_mint: Pubkey, amount: u64, source_id: impl Into<String>) -> Self {
        Self {
            asset_mint,
            amount,
            source_id: source_id.into(),
            max_slippage_bps: 50,
        }
    }

    /// Withdraw maximum available.
    pub fn max(asset_mint: Pubkey, source_id: impl Into<String>) -> Self {
        Self {
            asset_mint,
            amount: u64::MAX,
            source_id: source_id.into(),
            max_slippage_bps: 50,
        }
    }

    /// Set max slippage.
    pub fn with_slippage(mut self, bps: u16) -> Self {
        self.max_slippage_bps = bps;
        self
    }
}

/// Result of a DeFi transaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionResult {
    /// Transaction signature.
    pub signature: String,
    /// Protocol name.
    pub protocol: String,
    /// Action performed.
    pub action: DeFiAction,
    /// Asset involved.
    pub asset_mint: Pubkey,
    /// Amount involved.
    pub amount: u64,
    /// USD value at time of transaction.
    pub usd_value: Option<f64>,
    /// Block explorer URL.
    pub explorer_url: String,
}

impl TransactionResult {
    /// Create a new transaction result.
    pub fn new(
        signature: impl Into<String>,
        protocol: impl Into<String>,
        action: DeFiAction,
        asset_mint: Pubkey,
        amount: u64,
    ) -> Self {
        let sig = signature.into();
        Self {
            explorer_url: format!("https://solscan.io/tx/{}", sig),
            signature: sig,
            protocol: protocol.into(),
            action,
            asset_mint,
            amount,
            usd_value: None,
        }
    }

    /// Set USD value.
    pub fn with_usd_value(mut self, value: f64) -> Self {
        self.usd_value = Some(value);
        self
    }
}

/// DeFi action types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeFiAction {
    /// Deposit/supply assets.
    Deposit,
    /// Withdraw assets.
    Withdraw,
    /// Stake assets.
    Stake,
    /// Unstake assets.
    Unstake,
    /// Borrow assets.
    Borrow,
    /// Repay borrowed assets.
    Repay,
    /// Claim rewards.
    ClaimRewards,
}

/// A user's position in a DeFi protocol.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    /// Protocol name.
    pub protocol: String,
    /// Position identifier.
    pub id: String,
    /// Position type.
    pub position_type: PositionType,
    /// Asset mint.
    pub asset_mint: Pubkey,
    /// Asset symbol.
    pub asset_symbol: String,
    /// Amount deposited/staked (in smallest units).
    pub amount: u64,
    /// Current USD value.
    pub usd_value: f64,
    /// Current APY being earned.
    pub current_apy: f64,
    /// Unclaimed rewards (if any).
    #[serde(default)]
    pub unclaimed_rewards: Vec<Reward>,
}

/// Position types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PositionType {
    /// Lending/supply position.
    Supply,
    /// Borrow position.
    Borrow,
    /// Liquidity provision.
    Liquidity,
    /// Staking position.
    Stake,
    /// Vault deposit.
    Vault,
}

/// Unclaimed reward.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reward {
    /// Reward token mint.
    pub mint: Pubkey,
    /// Reward token symbol.
    pub symbol: String,
    /// Amount claimable.
    pub amount: u64,
    /// USD value.
    pub usd_value: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_yield_opportunity() {
        let opp = YieldOpportunity::new(
            "Solend",
            "usdc-main",
            "USDC",
            Pubkey::new_unique(),
            0.05,
            1_000_000.0,
            3,
        );

        assert_eq!(opp.protocol, "Solend");
        assert_eq!(opp.risk_score, 3);
    }

    #[test]
    fn test_deposit_params() {
        let params = DepositParams::new(Pubkey::new_unique(), 1000, "main-pool").with_slippage(100);

        assert_eq!(params.max_slippage_bps, 100);
    }

    #[test]
    fn test_transaction_result() {
        let result = TransactionResult::new(
            "5abc...",
            "Marinade",
            DeFiAction::Stake,
            Pubkey::new_unique(),
            1_000_000_000,
        );

        assert!(result.explorer_url.contains("5abc"));
    }
}
