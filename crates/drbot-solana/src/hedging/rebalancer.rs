//! Portfolio rebalancing for maintaining market neutrality.
//!
//! Monitors portfolio delta and triggers rebalancing when thresholds are exceeded.

use super::delta_calculator::{DeltaCalculator, PortfolioDelta};
use super::hedge_finder::{HedgeFinder, HedgePlan, HedgeRecommendation};
use crate::defi::approval::{DeFiApprovalManager, PendingTransaction};
use crate::defi::Position;
use crate::{Result, SolanaError};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};
use uuid::Uuid;

/// Rebalancing configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RebalanceConfig {
    /// Delta threshold percentage that triggers rebalancing.
    pub delta_threshold: f64,
    /// Minimum trade size in USD.
    pub min_trade_size_usd: f64,
    /// Whether to automatically rebalance (vs requiring approval).
    pub auto_rebalance: bool,
    /// Maximum slippage tolerance for trades.
    pub max_slippage_bps: u16,
    /// Cool-down period between rebalances (seconds).
    pub cooldown_secs: u64,
    /// Target delta (usually 0 for market neutral).
    pub target_delta: f64,
}

impl Default for RebalanceConfig {
    fn default() -> Self {
        Self {
            delta_threshold: 10.0, // 10% delta triggers rebalance
            min_trade_size_usd: 50.0,
            auto_rebalance: false, // Require approval by default
            max_slippage_bps: 50,
            cooldown_secs: 300, // 5 minute cooldown
            target_delta: 0.0,
        }
    }
}

impl RebalanceConfig {
    /// Create a config for aggressive rebalancing.
    pub fn aggressive() -> Self {
        Self {
            delta_threshold: 5.0,
            min_trade_size_usd: 25.0,
            auto_rebalance: true,
            ..Default::default()
        }
    }

    /// Create a config for conservative rebalancing.
    pub fn conservative() -> Self {
        Self {
            delta_threshold: 20.0,
            min_trade_size_usd: 100.0,
            auto_rebalance: false,
            cooldown_secs: 600,
            ..Default::default()
        }
    }
}

/// Rebalancer for maintaining market neutral positions.
pub struct Rebalancer {
    config: RebalanceConfig,
    delta_calculator: DeltaCalculator,
    hedge_finder: HedgeFinder,
    last_rebalance: Arc<RwLock<Option<DateTime<Utc>>>>,
    pending_plans: Arc<RwLock<Vec<PendingRebalance>>>,
}

/// A pending rebalance awaiting execution or approval.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingRebalance {
    /// Unique identifier.
    pub id: Uuid,
    /// The hedge plan.
    pub plan: HedgePlan,
    /// When the plan was created.
    pub created_at: DateTime<Utc>,
    /// Status of the rebalance.
    pub status: RebalanceStatus,
    /// Execution results.
    pub results: Vec<RebalanceResult>,
}

/// Status of a rebalance operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RebalanceStatus {
    /// Awaiting approval.
    Pending,
    /// Approved, executing.
    Executing,
    /// Partially executed.
    PartiallyExecuted,
    /// Fully executed.
    Completed,
    /// Cancelled.
    Cancelled,
    /// Failed.
    Failed,
}

/// Result of executing a single hedge.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RebalanceResult {
    /// Recommendation that was executed.
    pub recommendation: HedgeRecommendation,
    /// Whether execution succeeded.
    pub success: bool,
    /// Transaction signature (if successful).
    pub signature: Option<String>,
    /// Error message (if failed).
    pub error: Option<String>,
    /// Actual delta change.
    pub actual_delta_change: f64,
}

impl Rebalancer {
    /// Create a new rebalancer.
    pub fn new(config: RebalanceConfig) -> Self {
        Self {
            config,
            delta_calculator: DeltaCalculator::new(),
            hedge_finder: HedgeFinder::default(),
            last_rebalance: Arc::new(RwLock::new(None)),
            pending_plans: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Check if rebalancing is needed.
    pub async fn needs_rebalance(&self, positions: &[Position]) -> bool {
        // Check cooldown
        if let Some(last) = *self.last_rebalance.read().await {
            let elapsed = (Utc::now() - last).num_seconds() as u64;
            if elapsed < self.config.cooldown_secs {
                return false;
            }
        }

        let delta = self.delta_calculator.calculate(positions);
        delta.has_directional_bias(self.config.delta_threshold)
    }

    /// Analyze portfolio and create rebalance plan if needed.
    pub async fn analyze(&self, positions: &[Position]) -> Option<HedgePlan> {
        let delta = self.delta_calculator.calculate(positions);

        if !delta.has_directional_bias(self.config.delta_threshold) {
            debug!(
                delta = delta.total_delta,
                threshold = self.config.delta_threshold,
                "Portfolio within delta threshold"
            );
            return None;
        }

        info!(
            delta = delta.total_delta,
            percentage = delta.delta_percentage(),
            "Portfolio exceeds delta threshold, creating hedge plan"
        );

        let plan = self
            .hedge_finder
            .find_hedges_for_target(positions, self.config.target_delta);

        Some(plan)
    }

    /// Execute a hedge plan (with optional approval).
    pub async fn execute_plan(&self, plan: HedgePlan) -> Result<PendingRebalance> {
        let id = Uuid::new_v4();

        let pending = PendingRebalance {
            id,
            plan: plan.clone(),
            created_at: Utc::now(),
            status: if self.config.auto_rebalance {
                RebalanceStatus::Executing
            } else {
                RebalanceStatus::Pending
            },
            results: Vec::new(),
        };

        self.pending_plans.write().await.push(pending.clone());

        if self.config.auto_rebalance {
            info!(id = %id, "Auto-executing rebalance plan");
            self.execute_pending(id).await
        } else {
            info!(id = %id, "Rebalance plan pending approval");
            Ok(pending)
        }
    }

    /// Execute a pending rebalance by ID.
    pub async fn execute_pending(&self, id: Uuid) -> Result<PendingRebalance> {
        let mut plans = self.pending_plans.write().await;

        let pending = plans
            .iter_mut()
            .find(|p| p.id == id)
            .ok_or_else(|| SolanaError::ConfigError("Pending rebalance not found".to_string()))?;

        if pending.status != RebalanceStatus::Pending
            && pending.status != RebalanceStatus::Executing
        {
            return Err(SolanaError::ConfigError(format!(
                "Cannot execute rebalance in status {:?}",
                pending.status
            )));
        }

        pending.status = RebalanceStatus::Executing;

        // Execute each recommendation
        for rec in &pending.plan.recommendations {
            let result = self.execute_hedge(rec).await;
            pending.results.push(result);
        }

        // Update status
        let all_success = pending.results.iter().all(|r| r.success);
        let any_success = pending.results.iter().any(|r| r.success);

        pending.status = if all_success {
            RebalanceStatus::Completed
        } else if any_success {
            RebalanceStatus::PartiallyExecuted
        } else {
            RebalanceStatus::Failed
        };

        // Update last rebalance time
        if any_success {
            *self.last_rebalance.write().await = Some(Utc::now());
        }

        Ok(pending.clone())
    }

    /// Execute a single hedge recommendation.
    async fn execute_hedge(&self, rec: &HedgeRecommendation) -> RebalanceResult {
        // In a real implementation, this would:
        // 1. Get a swap quote
        // 2. Execute the swap transaction
        // 3. Verify the result

        info!(
            asset = %rec.hedge_asset,
            amount = rec.hedge_amount_usd,
            direction = ?rec.hedge_direction,
            "Executing hedge"
        );

        // Simulated execution
        RebalanceResult {
            recommendation: rec.clone(),
            success: false, // Would be true after actual execution
            signature: None,
            error: Some("Execution not yet implemented".to_string()),
            actual_delta_change: 0.0,
        }
    }

    /// Cancel a pending rebalance.
    pub async fn cancel(&self, id: Uuid) -> Result<()> {
        let mut plans = self.pending_plans.write().await;

        let pending = plans
            .iter_mut()
            .find(|p| p.id == id)
            .ok_or_else(|| SolanaError::ConfigError("Pending rebalance not found".to_string()))?;

        if pending.status != RebalanceStatus::Pending {
            return Err(SolanaError::ConfigError(
                "Can only cancel pending rebalances".to_string(),
            ));
        }

        pending.status = RebalanceStatus::Cancelled;
        info!(id = %id, "Rebalance cancelled");

        Ok(())
    }

    /// Get pending rebalances.
    pub async fn get_pending(&self) -> Vec<PendingRebalance> {
        self.pending_plans
            .read()
            .await
            .iter()
            .filter(|p| p.status == RebalanceStatus::Pending)
            .cloned()
            .collect()
    }

    /// Get rebalance history.
    pub async fn get_history(&self) -> Vec<PendingRebalance> {
        self.pending_plans.read().await.clone()
    }

    /// Get the current delta for positions.
    pub fn get_delta(&self, positions: &[Position]) -> PortfolioDelta {
        self.delta_calculator.calculate(positions)
    }

    /// Make portfolio market neutral (convenience method).
    pub async fn make_market_neutral(&self, positions: &[Position]) -> Result<PendingRebalance> {
        let plan = self.hedge_finder.find_hedges(positions);

        if plan.recommendations.is_empty() {
            return Err(SolanaError::ConfigError(
                "Portfolio is already market neutral".to_string(),
            ));
        }

        self.execute_plan(plan).await
    }

    /// Get configuration.
    pub fn config(&self) -> &RebalanceConfig {
        &self.config
    }
}

/// Summary of rebalancing activity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RebalanceSummary {
    /// Total rebalances executed.
    pub total_rebalances: usize,
    /// Successful rebalances.
    pub successful: usize,
    /// Failed rebalances.
    pub failed: usize,
    /// Total delta reduction achieved.
    pub total_delta_reduction: f64,
    /// Total cost incurred.
    pub total_cost_usd: f64,
    /// Last rebalance time.
    pub last_rebalance: Option<DateTime<Utc>>,
}

impl From<&[PendingRebalance]> for RebalanceSummary {
    fn from(rebalances: &[PendingRebalance]) -> Self {
        let completed: Vec<_> = rebalances
            .iter()
            .filter(|r| {
                matches!(
                    r.status,
                    RebalanceStatus::Completed | RebalanceStatus::PartiallyExecuted
                )
            })
            .collect();

        let failed = rebalances
            .iter()
            .filter(|r| r.status == RebalanceStatus::Failed)
            .count();

        let total_delta_reduction: f64 = completed
            .iter()
            .flat_map(|r| &r.results)
            .map(|r| r.actual_delta_change)
            .sum();

        let total_cost: f64 = completed.iter().map(|r| r.plan.total_cost_usd).sum();

        let last_rebalance = completed.iter().map(|r| r.created_at).max();

        Self {
            total_rebalances: completed.len(),
            successful: completed.len(),
            failed,
            total_delta_reduction,
            total_cost_usd: total_cost,
            last_rebalance,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::defi::PositionType;

    fn test_position(symbol: &str, value: f64, pos_type: PositionType) -> Position {
        Position {
            protocol: "Test".to_string(),
            id: format!("test-{}", symbol),
            position_type: pos_type,
            asset_mint: solana_sdk::pubkey::Pubkey::new_unique(),
            asset_symbol: symbol.to_string(),
            amount: (value * 1e9) as u64,
            usd_value: value,
            current_apy: 0.05,
            unclaimed_rewards: vec![],
        }
    }

    #[tokio::test]
    async fn test_needs_rebalance() {
        let rebalancer = Rebalancer::new(RebalanceConfig {
            delta_threshold: 10.0,
            ..Default::default()
        });

        // Large SOL position should need rebalancing
        let positions = vec![test_position("SOL", 10000.0, PositionType::Stake)];

        assert!(rebalancer.needs_rebalance(&positions).await);
    }

    #[tokio::test]
    async fn test_no_rebalance_when_neutral() {
        let rebalancer = Rebalancer::new(RebalanceConfig::default());

        // Only stablecoins - no delta
        let positions = vec![test_position("USDC", 10000.0, PositionType::Supply)];

        assert!(!rebalancer.needs_rebalance(&positions).await);
    }

    #[tokio::test]
    async fn test_analyze_creates_plan() {
        let rebalancer = Rebalancer::new(RebalanceConfig::default());

        let positions = vec![test_position("SOL", 5000.0, PositionType::Stake)];

        let plan = rebalancer.analyze(&positions).await;

        assert!(plan.is_some());
        let plan = plan.unwrap();
        assert!(!plan.recommendations.is_empty());
    }

    #[test]
    fn test_config_presets() {
        let aggressive = RebalanceConfig::aggressive();
        let conservative = RebalanceConfig::conservative();

        assert!(aggressive.delta_threshold < conservative.delta_threshold);
        assert!(aggressive.auto_rebalance);
        assert!(!conservative.auto_rebalance);
    }
}
