//! Hedging skill for market neutral portfolio management.

use crate::defi::YieldAggregator;
use crate::hedging::{
    DeltaCalculator, HedgeFinder, HedgeFinderConfig, HedgePlan, PortfolioDelta, RebalanceConfig,
    Rebalancer,
};
use crate::{Result, SolanaError};
use async_trait::async_trait;
use drbot_skills::{
    ManifestCapability, ManifestInput, ManifestOutput, Skill, SkillContext, SkillInput,
    SkillManifest, SkillOutput,
};
use serde::{Deserialize, Serialize};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;
use std::sync::Arc;

/// Hedging skill for market neutral portfolio management.
pub struct HedgingSkill {
    manifest: SkillManifest,
    rpc_client: Arc<RpcClient>,
    delta_calculator: DeltaCalculator,
    hedge_finder: HedgeFinder,
    rebalancer: Arc<Rebalancer>,
    aggregator: YieldAggregator,
}

impl HedgingSkill {
    /// Create a new hedging skill.
    pub fn new(rpc_client: Arc<RpcClient>) -> Self {
        let manifest = SkillManifest {
            name: "hedging".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            description: "Market neutral hedging and portfolio delta management".to_string(),
            author: Some("drbot".to_string()),
            license: Some("MIT".to_string()),
            homepage: None,
            repository: None,
            tags: vec![
                "solana".to_string(),
                "hedging".to_string(),
                "delta".to_string(),
            ],
            inputs: vec![
                ManifestInput {
                    name: "action".to_string(),
                    description: "Action: delta, hedge_plan, execute, rebalance, market_neutral"
                        .to_string(),
                    param_type: "string".to_string(),
                    required: true,
                    default: None,
                    pattern: None,
                    enum_values: vec![],
                },
                ManifestInput {
                    name: "wallet".to_string(),
                    description: "Wallet address to analyze".to_string(),
                    param_type: "string".to_string(),
                    required: false,
                    default: None,
                    pattern: None,
                    enum_values: vec![],
                },
                ManifestInput {
                    name: "target_delta".to_string(),
                    description: "Target delta for hedging (0 for market neutral)".to_string(),
                    param_type: "number".to_string(),
                    required: false,
                    default: None,
                    pattern: None,
                    enum_values: vec![],
                },
                ManifestInput {
                    name: "plan_id".to_string(),
                    description: "Hedge plan ID to execute".to_string(),
                    param_type: "string".to_string(),
                    required: false,
                    default: None,
                    pattern: None,
                    enum_values: vec![],
                },
                ManifestInput {
                    name: "auto_execute".to_string(),
                    description: "Whether to auto-execute the plan".to_string(),
                    param_type: "boolean".to_string(),
                    required: false,
                    default: None,
                    pattern: None,
                    enum_values: vec![],
                },
            ],
            outputs: vec![ManifestOutput {
                name: "result".to_string(),
                description: "Operation result".to_string(),
                output_type: "object".to_string(),
            }],
            capabilities: vec![
                ManifestCapability::required("hedging"),
                ManifestCapability::required("risk"),
            ],
            entry_point: None,
            runtime: None,
        };

        Self {
            manifest,
            rpc_client: rpc_client.clone(),
            delta_calculator: DeltaCalculator::new(),
            hedge_finder: HedgeFinder::new(HedgeFinderConfig::default()),
            rebalancer: Arc::new(Rebalancer::new(RebalanceConfig::default())),
            aggregator: YieldAggregator::new(rpc_client),
        }
    }

    /// Create with custom rebalance config.
    pub fn with_rebalance_config(mut self, config: RebalanceConfig) -> Self {
        self.rebalancer = Arc::new(Rebalancer::new(config));
        self
    }
}

#[async_trait]
impl Skill for HedgingSkill {
    fn manifest(&self) -> &SkillManifest {
        &self.manifest
    }

    fn validate_input(&self, input: &SkillInput) -> drbot_skills::Result<()> {
        let action = input
            .params
            .get("action")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                drbot_skills::SkillError::ValidationFailed("Missing action".to_string())
            })?;

        match action {
            "delta" | "hedge_plan" | "rebalance" | "market_neutral" => {
                if input
                    .params
                    .get("wallet")
                    .and_then(|v| v.as_str())
                    .is_none()
                {
                    return Err(drbot_skills::SkillError::ValidationFailed(
                        "Wallet required".to_string(),
                    ));
                }
                Ok(())
            }
            "execute" => {
                if input
                    .params
                    .get("plan_id")
                    .and_then(|v| v.as_str())
                    .is_none()
                {
                    return Err(drbot_skills::SkillError::ValidationFailed(
                        "Plan ID required for execute".to_string(),
                    ));
                }
                Ok(())
            }
            _ => Err(drbot_skills::SkillError::ValidationFailed(format!(
                "Unknown action: {}",
                action
            ))),
        }
    }

    async fn execute(
        &self,
        input: SkillInput,
        _context: &SkillContext,
    ) -> drbot_skills::Result<SkillOutput> {
        let action = input
            .params
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("delta");

        match action {
            "delta" => self.handle_delta(&input).await,
            "hedge_plan" => self.handle_hedge_plan(&input).await,
            "rebalance" => self.handle_rebalance(&input).await,
            "market_neutral" => self.handle_market_neutral(&input).await,
            "execute" => self.handle_execute(&input).await,
            _ => Err(drbot_skills::SkillError::ExecutionFailed(format!(
                "Action '{}' not implemented",
                action
            ))),
        }
    }
}

impl HedgingSkill {
    async fn handle_delta(&self, input: &SkillInput) -> drbot_skills::Result<SkillOutput> {
        let wallet_str = input.params.get("wallet").and_then(|v| v.as_str()).unwrap();

        let wallet = Pubkey::from_str(wallet_str)
            .map_err(|e| drbot_skills::SkillError::ValidationFailed(e.to_string()))?;

        let positions = self
            .aggregator
            .get_all_positions(&wallet)
            .await
            .map_err(|e| drbot_skills::SkillError::ExecutionFailed(e.to_string()))?;

        let delta = self.delta_calculator.calculate(&positions);

        let output = DeltaOutput {
            total_delta: delta.total_delta,
            delta_percentage: delta.delta_percentage(),
            beta_to_sol: delta.beta_to_sol,
            beta_to_btc: delta.beta_to_btc,
            is_market_neutral: delta.is_market_neutral,
            long_exposure: delta.long_exposure,
            short_exposure: delta.short_exposure,
            positions: delta
                .position_deltas
                .iter()
                .map(|p| PositionDeltaOutput {
                    asset: p.asset.clone(),
                    value_usd: p.value_usd,
                    delta: p.delta,
                    direction: format!("{:?}", p.direction),
                    beta_sol: p.beta_sol,
                })
                .collect(),
        };

        Ok(SkillOutput::new(serde_json::to_value(output).map_err(
            |e| drbot_skills::SkillError::ExecutionFailed(e.to_string()),
        )?))
    }

    async fn handle_hedge_plan(&self, input: &SkillInput) -> drbot_skills::Result<SkillOutput> {
        let wallet_str = input.params.get("wallet").and_then(|v| v.as_str()).unwrap();

        let wallet = Pubkey::from_str(wallet_str)
            .map_err(|e| drbot_skills::SkillError::ValidationFailed(e.to_string()))?;

        let target_delta = input
            .params
            .get("target_delta")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);

        let positions = self
            .aggregator
            .get_all_positions(&wallet)
            .await
            .map_err(|e| drbot_skills::SkillError::ExecutionFailed(e.to_string()))?;

        let plan = self
            .hedge_finder
            .find_hedges_for_target(&positions, target_delta);

        let output = HedgePlanOutput {
            current_delta: plan.current_delta.total_delta,
            target_delta: plan.target_delta,
            expected_final_delta: plan.expected_final_delta,
            total_cost_usd: plan.total_cost_usd,
            confidence: plan.confidence,
            achieves_neutrality: plan.achieves_neutrality(100.0), // $100 threshold
            recommendations: plan
                .recommendations
                .iter()
                .map(|r| HedgeRecommendationOutput {
                    hedge_asset: r.hedge_asset.clone(),
                    direction: format!("{:?}", r.hedge_direction),
                    amount_usd: r.hedge_amount_usd,
                    delta_reduction: r.expected_delta_reduction,
                    cost_estimate: r.cost_estimate_usd,
                    method: format!("{:?}", r.method),
                })
                .collect(),
            summary: plan.summary(),
        };

        Ok(SkillOutput::new(serde_json::to_value(output).map_err(
            |e| drbot_skills::SkillError::ExecutionFailed(e.to_string()),
        )?))
    }

    async fn handle_rebalance(&self, input: &SkillInput) -> drbot_skills::Result<SkillOutput> {
        let wallet_str = input.params.get("wallet").and_then(|v| v.as_str()).unwrap();

        let wallet = Pubkey::from_str(wallet_str)
            .map_err(|e| drbot_skills::SkillError::ValidationFailed(e.to_string()))?;

        let positions = self
            .aggregator
            .get_all_positions(&wallet)
            .await
            .map_err(|e| drbot_skills::SkillError::ExecutionFailed(e.to_string()))?;

        let needs_rebalance = self.rebalancer.needs_rebalance(&positions).await;

        if !needs_rebalance {
            return Ok(SkillOutput::new(serde_json::json!({
                "needs_rebalance": false,
                "message": "Portfolio is within delta threshold"
            })));
        }

        let plan = self.rebalancer.analyze(&positions).await.ok_or_else(|| {
            drbot_skills::SkillError::ExecutionFailed("Could not create rebalance plan".to_string())
        })?;

        let auto_execute = input
            .params
            .get("auto_execute")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if auto_execute {
            let pending = self
                .rebalancer
                .execute_plan(plan)
                .await
                .map_err(|e| drbot_skills::SkillError::ExecutionFailed(e.to_string()))?;

            Ok(SkillOutput::new(serde_json::json!({
                "needs_rebalance": true,
                "executed": true,
                "plan_id": pending.id.to_string(),
                "status": format!("{:?}", pending.status),
            })))
        } else {
            let output = HedgePlanOutput {
                current_delta: plan.current_delta.total_delta,
                target_delta: plan.target_delta,
                expected_final_delta: plan.expected_final_delta,
                total_cost_usd: plan.total_cost_usd,
                confidence: plan.confidence,
                achieves_neutrality: plan.achieves_neutrality(100.0),
                recommendations: plan
                    .recommendations
                    .iter()
                    .map(|r| HedgeRecommendationOutput {
                        hedge_asset: r.hedge_asset.clone(),
                        direction: format!("{:?}", r.hedge_direction),
                        amount_usd: r.hedge_amount_usd,
                        delta_reduction: r.expected_delta_reduction,
                        cost_estimate: r.cost_estimate_usd,
                        method: format!("{:?}", r.method),
                    })
                    .collect(),
                summary: plan.summary(),
            };

            Ok(SkillOutput::new(serde_json::json!({
                "needs_rebalance": true,
                "executed": false,
                "plan": output,
            })))
        }
    }

    async fn handle_market_neutral(&self, input: &SkillInput) -> drbot_skills::Result<SkillOutput> {
        let wallet_str = input.params.get("wallet").and_then(|v| v.as_str()).unwrap();

        let wallet = Pubkey::from_str(wallet_str)
            .map_err(|e| drbot_skills::SkillError::ValidationFailed(e.to_string()))?;

        let positions = self
            .aggregator
            .get_all_positions(&wallet)
            .await
            .map_err(|e| drbot_skills::SkillError::ExecutionFailed(e.to_string()))?;

        let pending = self
            .rebalancer
            .make_market_neutral(&positions)
            .await
            .map_err(|e| drbot_skills::SkillError::ExecutionFailed(e.to_string()))?;

        Ok(SkillOutput::new(serde_json::json!({
            "status": format!("{:?}", pending.status),
            "plan_id": pending.id.to_string(),
            "message": "Market neutral rebalance initiated"
        })))
    }

    async fn handle_execute(&self, input: &SkillInput) -> drbot_skills::Result<SkillOutput> {
        let plan_id = input
            .params
            .get("plan_id")
            .and_then(|v| v.as_str())
            .unwrap();

        let plan_uuid = uuid::Uuid::parse_str(plan_id)
            .map_err(|e| drbot_skills::SkillError::ValidationFailed(e.to_string()))?;

        let result = self
            .rebalancer
            .execute_pending(plan_uuid)
            .await
            .map_err(|e| drbot_skills::SkillError::ExecutionFailed(e.to_string()))?;

        Ok(SkillOutput::new(serde_json::json!({
            "plan_id": result.id.to_string(),
            "status": format!("{:?}", result.status),
            "results_count": result.results.len(),
            "successful": result.results.iter().filter(|r| r.success).count(),
        })))
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct DeltaOutput {
    total_delta: f64,
    delta_percentage: f64,
    beta_to_sol: f64,
    beta_to_btc: f64,
    is_market_neutral: bool,
    long_exposure: f64,
    short_exposure: f64,
    positions: Vec<PositionDeltaOutput>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PositionDeltaOutput {
    asset: String,
    value_usd: f64,
    delta: f64,
    direction: String,
    beta_sol: f64,
}

#[derive(Debug, Serialize, Deserialize)]
struct HedgePlanOutput {
    current_delta: f64,
    target_delta: f64,
    expected_final_delta: f64,
    total_cost_usd: f64,
    confidence: f64,
    achieves_neutrality: bool,
    recommendations: Vec<HedgeRecommendationOutput>,
    summary: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct HedgeRecommendationOutput {
    hedge_asset: String,
    direction: String,
    amount_usd: f64,
    delta_reduction: f64,
    cost_estimate: f64,
    method: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hedging_skill_manifest() {
        let rpc = Arc::new(RpcClient::new("https://api.devnet.solana.com".to_string()));
        let skill = HedgingSkill::new(rpc);
        let manifest = skill.manifest();

        assert_eq!(manifest.name, "hedging");
    }
}
