//! Human-in-the-loop approval flow for DeFi transactions.
//!
//! Requires confirmation for transactions above a configurable threshold,
//! using approval codes for security.

use crate::{Result, SolanaError};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};
use uuid::Uuid;

use super::protocols::{DeFiAction, TransactionResult, YieldOpportunity};

/// Configuration for DeFi approval flow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalConfig {
    /// Threshold in USD above which approval is required.
    pub threshold_usd: f64,
    /// Approval code expiration in seconds.
    pub expiration_secs: u64,
    /// Whether to require approval for all transactions.
    pub require_all: bool,
    /// Actions that always require approval regardless of amount.
    pub always_require: Vec<DeFiAction>,
}

impl Default for ApprovalConfig {
    fn default() -> Self {
        Self {
            threshold_usd: 100.0,
            expiration_secs: 300, // 5 minutes
            require_all: false,
            always_require: vec![DeFiAction::Withdraw],
        }
    }
}

impl ApprovalConfig {
    /// Create a strict config requiring approval for all transactions.
    pub fn strict() -> Self {
        Self {
            threshold_usd: 0.0,
            require_all: true,
            ..Default::default()
        }
    }

    /// Set the threshold.
    pub fn with_threshold(mut self, threshold: f64) -> Self {
        self.threshold_usd = threshold;
        self
    }

    /// Set expiration time.
    pub fn with_expiration(mut self, secs: u64) -> Self {
        self.expiration_secs = secs;
        self
    }
}

/// A pending DeFi transaction awaiting approval.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingTransaction {
    /// Unique identifier.
    pub id: Uuid,
    /// The yield opportunity being acted upon.
    pub opportunity: YieldOpportunity,
    /// Action to perform.
    pub action: DeFiAction,
    /// Amount in smallest units.
    pub amount: u64,
    /// Estimated USD value.
    pub amount_usd: f64,
    /// 6-digit approval code.
    pub approval_code: String,
    /// When this approval request expires.
    pub expires_at: DateTime<Utc>,
    /// Creation time.
    pub created_at: DateTime<Utc>,
    /// Status of the pending transaction.
    pub status: PendingStatus,
}

/// Status of a pending transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PendingStatus {
    /// Awaiting approval.
    Pending,
    /// Approved and ready to execute.
    Approved,
    /// Rejected by user.
    Rejected,
    /// Expired without action.
    Expired,
    /// Executed successfully.
    Executed,
}

/// Manages DeFi transaction approvals.
pub struct DeFiApprovalManager {
    config: ApprovalConfig,
    pending: Arc<RwLock<HashMap<Uuid, PendingTransaction>>>,
}

impl DeFiApprovalManager {
    /// Create a new approval manager.
    pub fn new(config: ApprovalConfig) -> Self {
        Self {
            config,
            pending: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Check if approval is required for a transaction.
    pub fn requires_approval(&self, action: DeFiAction, amount_usd: f64) -> bool {
        if self.config.require_all {
            return true;
        }

        if self.config.always_require.contains(&action) {
            return true;
        }

        amount_usd >= self.config.threshold_usd
    }

    /// Request approval for a transaction.
    pub async fn request_approval(
        &self,
        opportunity: YieldOpportunity,
        action: DeFiAction,
        amount: u64,
        amount_usd: f64,
    ) -> Result<PendingTransaction> {
        let id = Uuid::new_v4();
        let approval_code = generate_approval_code();
        let now = Utc::now();
        let expires_at = now + Duration::seconds(self.config.expiration_secs as i64);

        let pending = PendingTransaction {
            id,
            opportunity,
            action,
            amount,
            amount_usd,
            approval_code: approval_code.clone(),
            expires_at,
            created_at: now,
            status: PendingStatus::Pending,
        };

        info!(
            id = %id,
            action = ?action,
            amount_usd = amount_usd,
            expires_at = %expires_at,
            "Created pending transaction approval request"
        );

        self.pending.write().await.insert(id, pending.clone());

        Ok(pending)
    }

    /// Approve a pending transaction with the approval code.
    pub async fn approve(&self, id: Uuid, code: &str) -> Result<PendingTransaction> {
        let mut pending = self.pending.write().await;

        let tx = pending.get_mut(&id).ok_or_else(|| {
            SolanaError::TransactionError("Pending transaction not found".to_string())
        })?;

        // Check expiration
        if Utc::now() > tx.expires_at {
            tx.status = PendingStatus::Expired;
            return Err(SolanaError::TransactionError(
                "Approval request expired".to_string(),
            ));
        }

        // Verify code
        if tx.approval_code != code {
            warn!(id = %id, "Invalid approval code provided");
            return Err(SolanaError::TransactionError(
                "Invalid approval code".to_string(),
            ));
        }

        // Check current status
        if tx.status != PendingStatus::Pending {
            return Err(SolanaError::TransactionError(format!(
                "Transaction is not pending: {:?}",
                tx.status
            )));
        }

        tx.status = PendingStatus::Approved;
        info!(id = %id, "Transaction approved");

        Ok(tx.clone())
    }

    /// Reject a pending transaction.
    pub async fn reject(&self, id: Uuid) -> Result<PendingTransaction> {
        let mut pending = self.pending.write().await;

        let tx = pending.get_mut(&id).ok_or_else(|| {
            SolanaError::TransactionError("Pending transaction not found".to_string())
        })?;

        if tx.status != PendingStatus::Pending {
            return Err(SolanaError::TransactionError(format!(
                "Transaction is not pending: {:?}",
                tx.status
            )));
        }

        tx.status = PendingStatus::Rejected;
        info!(id = %id, "Transaction rejected");

        Ok(tx.clone())
    }

    /// Mark a transaction as executed.
    pub async fn mark_executed(&self, id: Uuid) -> Result<()> {
        let mut pending = self.pending.write().await;

        let tx = pending.get_mut(&id).ok_or_else(|| {
            SolanaError::TransactionError("Pending transaction not found".to_string())
        })?;

        if tx.status != PendingStatus::Approved {
            return Err(SolanaError::TransactionError(
                "Transaction must be approved before execution".to_string(),
            ));
        }

        tx.status = PendingStatus::Executed;
        debug!(id = %id, "Transaction marked as executed");

        Ok(())
    }

    /// Get a pending transaction by ID.
    pub async fn get(&self, id: Uuid) -> Option<PendingTransaction> {
        self.pending.read().await.get(&id).cloned()
    }

    /// Get all pending transactions.
    pub async fn get_pending(&self) -> Vec<PendingTransaction> {
        self.pending
            .read()
            .await
            .values()
            .filter(|tx| tx.status == PendingStatus::Pending && Utc::now() <= tx.expires_at)
            .cloned()
            .collect()
    }

    /// Clean up expired transactions.
    pub async fn cleanup_expired(&self) -> usize {
        let mut pending = self.pending.write().await;
        let now = Utc::now();

        let expired: Vec<Uuid> = pending
            .iter()
            .filter(|(_, tx)| tx.status == PendingStatus::Pending && now > tx.expires_at)
            .map(|(id, _)| *id)
            .collect();

        let count = expired.len();
        for id in expired {
            if let Some(tx) = pending.get_mut(&id) {
                tx.status = PendingStatus::Expired;
            }
        }

        if count > 0 {
            debug!(count = count, "Cleaned up expired transactions");
        }

        count
    }

    /// Get configuration.
    pub fn config(&self) -> &ApprovalConfig {
        &self.config
    }
}

/// Generate a 6-digit approval code.
fn generate_approval_code() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    format!("{:06}", rng.gen_range(0..1_000_000))
}

/// Approval request for display to user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRequest {
    /// Transaction ID.
    pub id: String,
    /// Protocol name.
    pub protocol: String,
    /// Action description.
    pub action: String,
    /// Asset involved.
    pub asset: String,
    /// Amount in human-readable form.
    pub amount_display: String,
    /// USD value.
    pub amount_usd: f64,
    /// Expiration time.
    pub expires_at: String,
    /// Instructions for user.
    pub instructions: String,
}

impl From<&PendingTransaction> for ApprovalRequest {
    fn from(tx: &PendingTransaction) -> Self {
        Self {
            id: tx.id.to_string(),
            protocol: tx.opportunity.protocol.clone(),
            action: format!("{:?}", tx.action),
            asset: tx.opportunity.asset.clone(),
            amount_display: format_amount(tx.amount),
            amount_usd: tx.amount_usd,
            expires_at: tx.expires_at.format("%Y-%m-%d %H:%M:%S UTC").to_string(),
            instructions: format!(
                "To approve this transaction, use the approval code: {}",
                tx.approval_code
            ),
        }
    }
}

/// Format amount for display.
fn format_amount(amount: u64) -> String {
    let ui_amount = amount as f64 / 1e9; // Assuming 9 decimals (SOL standard)
    if ui_amount >= 1000.0 {
        format!("{:.2}K", ui_amount / 1000.0)
    } else if ui_amount >= 1.0 {
        format!("{:.4}", ui_amount)
    } else {
        format!("{:.6}", ui_amount)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use solana_sdk::pubkey::Pubkey;

    fn test_opportunity() -> YieldOpportunity {
        YieldOpportunity::new(
            "Marinade",
            "msol-stake",
            "SOL",
            Pubkey::new_unique(),
            0.07,
            100_000_000.0,
            2,
        )
    }

    #[tokio::test]
    async fn test_approval_required() {
        let manager = DeFiApprovalManager::new(ApprovalConfig::default());

        // Under threshold
        assert!(!manager.requires_approval(DeFiAction::Deposit, 50.0));

        // Over threshold
        assert!(manager.requires_approval(DeFiAction::Deposit, 150.0));

        // Withdraw always requires
        assert!(manager.requires_approval(DeFiAction::Withdraw, 10.0));
    }

    #[tokio::test]
    async fn test_approval_flow() {
        let manager = DeFiApprovalManager::new(ApprovalConfig::default());
        let opp = test_opportunity();

        // Request approval
        let pending = manager
            .request_approval(opp, DeFiAction::Deposit, 1_000_000_000, 150.0)
            .await
            .unwrap();

        assert_eq!(pending.status, PendingStatus::Pending);
        assert_eq!(pending.approval_code.len(), 6);

        // Approve with correct code
        let approved = manager
            .approve(pending.id, &pending.approval_code)
            .await
            .unwrap();

        assert_eq!(approved.status, PendingStatus::Approved);
    }

    #[tokio::test]
    async fn test_invalid_approval_code() {
        let manager = DeFiApprovalManager::new(ApprovalConfig::default());
        let opp = test_opportunity();

        let pending = manager
            .request_approval(opp, DeFiAction::Deposit, 1_000_000_000, 150.0)
            .await
            .unwrap();

        // Try wrong code
        let result = manager.approve(pending.id, "000000").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_reject() {
        let manager = DeFiApprovalManager::new(ApprovalConfig::default());
        let opp = test_opportunity();

        let pending = manager
            .request_approval(opp, DeFiAction::Deposit, 1_000_000_000, 150.0)
            .await
            .unwrap();

        let rejected = manager.reject(pending.id).await.unwrap();
        assert_eq!(rejected.status, PendingStatus::Rejected);
    }

    #[test]
    fn test_approval_code_format() {
        let code = generate_approval_code();
        assert_eq!(code.len(), 6);
        assert!(code.chars().all(|c| c.is_ascii_digit()));
    }
}
