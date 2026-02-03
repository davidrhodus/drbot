//! Hedge opportunity finder.
//!
//! Discovers and recommends hedge positions to reduce portfolio delta.

use super::delta_calculator::{AssetClass, DeltaCalculator, PortfolioDelta, PositionDelta};
use crate::defi::{Position, YieldOpportunity};
use crate::otc::TradeDirection;
use serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;
use std::collections::HashMap;

/// A recommendation for hedging a position.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HedgeRecommendation {
    /// Position being hedged.
    pub target_position: PositionDelta,
    /// Recommended hedge asset.
    pub hedge_asset: String,
    /// Hedge asset mint.
    pub hedge_mint: Pubkey,
    /// Direction of hedge trade.
    pub hedge_direction: TradeDirection,
    /// Amount to hedge (in smallest units).
    pub hedge_amount: u64,
    /// Amount in USD terms.
    pub hedge_amount_usd: f64,
    /// Expected delta reduction.
    pub expected_delta_reduction: f64,
    /// Estimated cost/slippage.
    pub cost_estimate_usd: f64,
    /// Hedge effectiveness (0-1).
    pub effectiveness: f64,
    /// Hedge method.
    pub method: HedgeMethod,
}

/// Methods for implementing a hedge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HedgeMethod {
    /// Spot trade (buy/sell the asset).
    Spot,
    /// Short via lending protocol.
    LendingShort,
    /// Perpetual futures.
    Perpetual,
    /// Options.
    Options,
    /// Inverse position in correlated asset.
    CorrelatedAsset,
}

/// A complete hedge plan for achieving market neutrality.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HedgePlan {
    /// Current portfolio delta.
    pub current_delta: PortfolioDelta,
    /// Target delta (usually 0 for market neutral).
    pub target_delta: f64,
    /// Recommended hedges.
    pub recommendations: Vec<HedgeRecommendation>,
    /// Total estimated cost.
    pub total_cost_usd: f64,
    /// Expected final delta after hedges.
    pub expected_final_delta: f64,
    /// Confidence in achieving target.
    pub confidence: f64,
}

impl HedgePlan {
    /// Check if the plan achieves market neutrality.
    pub fn achieves_neutrality(&self, threshold: f64) -> bool {
        self.expected_final_delta.abs() < threshold
    }

    /// Get summary of the plan.
    pub fn summary(&self) -> String {
        format!(
            "Hedge {} positions, reduce delta from {:.2} to {:.2}, cost ~${:.2}",
            self.recommendations.len(),
            self.current_delta.total_delta,
            self.expected_final_delta,
            self.total_cost_usd
        )
    }
}

/// Hedge finder for discovering hedging opportunities.
pub struct HedgeFinder {
    /// Known hedging assets.
    hedge_assets: HashMap<AssetClass, Vec<HedgeAsset>>,
    /// Delta calculator.
    delta_calculator: DeltaCalculator,
    /// Configuration.
    config: HedgeFinderConfig,
}

/// Configuration for hedge finding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HedgeFinderConfig {
    /// Minimum hedge size in USD.
    pub min_hedge_size_usd: f64,
    /// Maximum slippage tolerance.
    pub max_slippage_pct: f64,
    /// Preferred hedge methods.
    pub preferred_methods: Vec<HedgeMethod>,
    /// Whether to consider correlated assets.
    pub use_correlated_assets: bool,
}

impl Default for HedgeFinderConfig {
    fn default() -> Self {
        Self {
            min_hedge_size_usd: 50.0,
            max_slippage_pct: 1.0,
            preferred_methods: vec![HedgeMethod::Spot, HedgeMethod::LendingShort],
            use_correlated_assets: true,
        }
    }
}

/// A known hedging asset.
#[derive(Debug, Clone)]
struct HedgeAsset {
    symbol: String,
    mint: Pubkey,
    beta_to_class: f64,
    available_methods: Vec<HedgeMethod>,
    estimated_liquidity_usd: f64,
}

impl HedgeFinder {
    /// Create a new hedge finder.
    pub fn new(config: HedgeFinderConfig) -> Self {
        let mut hedge_assets: HashMap<AssetClass, Vec<HedgeAsset>> = HashMap::new();

        // SOL class hedges
        hedge_assets.insert(
            AssetClass::Sol,
            vec![
                HedgeAsset {
                    symbol: "SOL".to_string(),
                    mint: Pubkey::default(), // Would be actual mint
                    beta_to_class: 1.0,
                    available_methods: vec![HedgeMethod::Spot, HedgeMethod::LendingShort],
                    estimated_liquidity_usd: 100_000_000.0,
                },
                HedgeAsset {
                    symbol: "mSOL".to_string(),
                    mint: Pubkey::default(),
                    beta_to_class: 1.0,
                    available_methods: vec![HedgeMethod::Spot, HedgeMethod::LendingShort],
                    estimated_liquidity_usd: 50_000_000.0,
                },
            ],
        );

        // Stablecoin for delta neutralization
        hedge_assets.insert(
            AssetClass::Stablecoin,
            vec![HedgeAsset {
                symbol: "USDC".to_string(),
                mint: Pubkey::default(),
                beta_to_class: 0.0,
                available_methods: vec![HedgeMethod::Spot],
                estimated_liquidity_usd: 500_000_000.0,
            }],
        );

        Self {
            hedge_assets,
            delta_calculator: DeltaCalculator::new(),
            config,
        }
    }

    /// Find hedges to achieve market neutrality.
    pub fn find_hedges(&self, positions: &[Position]) -> HedgePlan {
        let current_delta = self.delta_calculator.calculate(positions);
        self.create_hedge_plan(&current_delta, 0.0)
    }

    /// Find hedges to achieve a specific target delta.
    pub fn find_hedges_for_target(&self, positions: &[Position], target_delta: f64) -> HedgePlan {
        let current_delta = self.delta_calculator.calculate(positions);
        self.create_hedge_plan(&current_delta, target_delta)
    }

    /// Create a hedge plan to move from current to target delta.
    fn create_hedge_plan(&self, current: &PortfolioDelta, target: f64) -> HedgePlan {
        let delta_to_hedge = current.total_delta - target;
        let mut recommendations = Vec::new();
        let mut remaining_delta = delta_to_hedge;

        // If we need to reduce positive delta, we need to short/sell
        // If we need to reduce negative delta, we need to long/buy

        if delta_to_hedge.abs() < self.config.min_hedge_size_usd {
            // Already close enough to target
            return HedgePlan {
                current_delta: current.clone(),
                target_delta: target,
                recommendations: vec![],
                total_cost_usd: 0.0,
                expected_final_delta: current.total_delta,
                confidence: 1.0,
            };
        }

        // Find the best hedge for each significant position
        for pos_delta in &current.position_deltas {
            if pos_delta.delta.abs() < self.config.min_hedge_size_usd {
                continue;
            }

            // Skip if this position is in the wrong direction
            if (remaining_delta > 0.0 && pos_delta.delta < 0.0)
                || (remaining_delta < 0.0 && pos_delta.delta > 0.0)
            {
                continue;
            }

            // Find a hedge for this position
            if let Some(rec) = self.find_hedge_for_position(pos_delta, remaining_delta) {
                remaining_delta -= rec.expected_delta_reduction;
                recommendations.push(rec);

                // Stop if we've hedged enough
                if remaining_delta.abs() < self.config.min_hedge_size_usd {
                    break;
                }
            }
        }

        // If we still have significant delta, add a general hedge
        if remaining_delta.abs() >= self.config.min_hedge_size_usd {
            if let Some(rec) = self.create_general_hedge(remaining_delta) {
                recommendations.push(rec);
            }
        }

        let total_cost: f64 = recommendations.iter().map(|r| r.cost_estimate_usd).sum();
        let total_reduction: f64 = recommendations
            .iter()
            .map(|r| r.expected_delta_reduction)
            .sum();
        let expected_final = current.total_delta - total_reduction;

        let confidence = if delta_to_hedge != 0.0 {
            (total_reduction / delta_to_hedge).min(1.0)
        } else {
            1.0
        };

        HedgePlan {
            current_delta: current.clone(),
            target_delta: target,
            recommendations,
            total_cost_usd: total_cost,
            expected_final_delta: expected_final,
            confidence,
        }
    }

    /// Find a hedge for a specific position.
    fn find_hedge_for_position(
        &self,
        pos: &PositionDelta,
        remaining_delta: f64,
    ) -> Option<HedgeRecommendation> {
        // Determine hedge direction
        let direction = if pos.delta > 0.0 {
            TradeDirection::Sell
        } else {
            TradeDirection::Buy
        };

        // Find the best hedge asset
        let hedge_assets = self.hedge_assets.get(&pos.asset_class)?;
        let hedge_asset = hedge_assets.first()?;

        // Calculate hedge amount
        let hedge_amount_usd = pos.delta.abs().min(remaining_delta.abs());
        let hedge_amount = (hedge_amount_usd * 1e9) as u64; // Assuming 9 decimals

        // Estimate cost (slippage + fees)
        let cost_estimate = hedge_amount_usd * 0.003; // 0.3% estimated cost

        Some(HedgeRecommendation {
            target_position: pos.clone(),
            hedge_asset: hedge_asset.symbol.clone(),
            hedge_mint: hedge_asset.mint,
            hedge_direction: direction,
            hedge_amount,
            hedge_amount_usd,
            expected_delta_reduction: hedge_amount_usd * if pos.delta > 0.0 { 1.0 } else { -1.0 },
            cost_estimate_usd: cost_estimate,
            effectiveness: hedge_asset.beta_to_class,
            method: hedge_asset.available_methods[0],
        })
    }

    /// Create a general hedge for remaining delta.
    fn create_general_hedge(&self, remaining_delta: f64) -> Option<HedgeRecommendation> {
        // Use SOL or stablecoin for general hedging
        let direction = if remaining_delta > 0.0 {
            TradeDirection::Sell
        } else {
            TradeDirection::Buy
        };

        let hedge_amount_usd = remaining_delta.abs();
        let hedge_amount = (hedge_amount_usd * 1e9) as u64;
        let cost_estimate = hedge_amount_usd * 0.003;

        Some(HedgeRecommendation {
            target_position: PositionDelta {
                position_id: "general".to_string(),
                asset: "Portfolio".to_string(),
                asset_mint: Pubkey::default(),
                value_usd: hedge_amount_usd,
                delta: remaining_delta,
                effective_delta: remaining_delta,
                direction: if remaining_delta > 0.0 {
                    super::delta_calculator::PositionDirection::Long
                } else {
                    super::delta_calculator::PositionDirection::Short
                },
                asset_class: AssetClass::Sol,
                beta_sol: 1.0,
            },
            hedge_asset: "SOL".to_string(),
            hedge_mint: Pubkey::default(),
            hedge_direction: direction,
            hedge_amount,
            hedge_amount_usd,
            expected_delta_reduction: remaining_delta,
            cost_estimate_usd: cost_estimate,
            effectiveness: 1.0,
            method: HedgeMethod::Spot,
        })
    }

    /// Get configuration.
    pub fn config(&self) -> &HedgeFinderConfig {
        &self.config
    }
}

impl Default for HedgeFinder {
    fn default() -> Self {
        Self::new(HedgeFinderConfig::default())
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
            asset_mint: Pubkey::new_unique(),
            asset_symbol: symbol.to_string(),
            amount: (value * 1e9) as u64,
            usd_value: value,
            current_apy: 0.05,
            unclaimed_rewards: vec![],
        }
    }

    #[test]
    fn test_hedge_plan_for_long() {
        let finder = HedgeFinder::new(HedgeFinderConfig::default());

        let positions = vec![test_position("SOL", 1000.0, PositionType::Stake)];

        let plan = finder.find_hedges(&positions);

        assert!(!plan.recommendations.is_empty());
        assert!(plan.expected_final_delta.abs() < plan.current_delta.total_delta.abs());
    }

    #[test]
    fn test_already_neutral() {
        let finder = HedgeFinder::new(HedgeFinderConfig {
            min_hedge_size_usd: 100.0,
            ..Default::default()
        });

        // Small position that doesn't need hedging
        let positions = vec![test_position("SOL", 10.0, PositionType::Stake)];

        let plan = finder.find_hedges(&positions);

        assert!(plan.recommendations.is_empty());
    }

    #[test]
    fn test_hedge_plan_summary() {
        let finder = HedgeFinder::new(HedgeFinderConfig::default());
        let positions = vec![test_position("SOL", 1000.0, PositionType::Stake)];

        let plan = finder.find_hedges(&positions);
        let summary = plan.summary();

        assert!(summary.contains("delta"));
    }
}
