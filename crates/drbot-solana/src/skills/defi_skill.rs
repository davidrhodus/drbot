//! DeFi skill for yield discovery and protocol interactions.

use crate::defi::{
    ApprovalConfig, DeFiApprovalManager, DepositParams, Position, WithdrawParams, YieldAggregator,
    YieldFilter, YieldOpportunity,
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

/// DeFi skill for yield discovery and protocol interactions.
pub struct DeFiSkill {
    manifest: SkillManifest,
    aggregator: YieldAggregator,
    approval_manager: DeFiApprovalManager,
}

impl DeFiSkill {
    /// Create a new DeFi skill.
    pub fn new(rpc_client: Arc<RpcClient>) -> Self {
        let manifest = SkillManifest {
            name: "defi".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            description:
                "Discover DeFi yield opportunities and manage positions across Solana protocols"
                    .to_string(),
            author: Some("drbot".to_string()),
            license: Some("MIT".to_string()),
            homepage: None,
            repository: None,
            tags: vec![
                "solana".to_string(),
                "defi".to_string(),
                "yield".to_string(),
            ],
            inputs: vec![
                ManifestInput {
                    name: "action".to_string(),
                    description: "Action: discover, positions, pending, approve".to_string(),
                    param_type: "string".to_string(),
                    required: true,
                    default: None,
                    pattern: None,
                    enum_values: vec![],
                },
                ManifestInput {
                    name: "protocol".to_string(),
                    description: "Protocol filter (Solend, Marginfi, Kamino, Marinade, Jito)"
                        .to_string(),
                    param_type: "string".to_string(),
                    required: false,
                    default: None,
                    pattern: None,
                    enum_values: vec![],
                },
                ManifestInput {
                    name: "min_apy".to_string(),
                    description: "Minimum APY filter (decimal)".to_string(),
                    param_type: "number".to_string(),
                    required: false,
                    default: None,
                    pattern: None,
                    enum_values: vec![],
                },
                ManifestInput {
                    name: "max_risk".to_string(),
                    description: "Maximum risk score (1-10)".to_string(),
                    param_type: "number".to_string(),
                    required: false,
                    default: None,
                    pattern: None,
                    enum_values: vec![],
                },
                ManifestInput {
                    name: "wallet".to_string(),
                    description: "Wallet address for positions query".to_string(),
                    param_type: "string".to_string(),
                    required: false,
                    default: None,
                    pattern: None,
                    enum_values: vec![],
                },
                ManifestInput {
                    name: "limit".to_string(),
                    description: "Maximum results".to_string(),
                    param_type: "number".to_string(),
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
                ManifestCapability::required("defi"),
                ManifestCapability::required("yield"),
            ],
            entry_point: None,
            runtime: None,
        };

        Self {
            manifest,
            aggregator: YieldAggregator::new(rpc_client),
            approval_manager: DeFiApprovalManager::new(ApprovalConfig::default()),
        }
    }
}

#[async_trait]
impl Skill for DeFiSkill {
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
            "discover" | "positions" | "pending" | "approve" => Ok(()),
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
            .unwrap_or("discover");

        match action {
            "discover" => self.handle_discover(&input).await,
            "positions" => self.handle_positions(&input).await,
            "pending" => self.handle_pending().await,
            "approve" => self.handle_approve(&input).await,
            _ => Err(drbot_skills::SkillError::ExecutionFailed(format!(
                "Unknown action: {}",
                action
            ))),
        }
    }
}

impl DeFiSkill {
    async fn handle_discover(&self, input: &SkillInput) -> drbot_skills::Result<SkillOutput> {
        let mut filter = YieldFilter::default();

        if let Some(min_apy) = input.params.get("min_apy").and_then(|v| v.as_f64()) {
            filter = filter.with_min_apy(min_apy);
        }

        if let Some(max_risk) = input.params.get("max_risk").and_then(|v| v.as_u64()) {
            filter = filter.with_max_risk(max_risk as u8);
        }

        if let Some(protocol) = input.params.get("protocol").and_then(|v| v.as_str()) {
            filter = filter.with_protocols(vec![protocol.to_string()]);
        }

        if let Some(limit) = input.params.get("limit").and_then(|v| v.as_u64()) {
            filter = filter.with_limit(limit as usize);
        }

        let opportunities = self
            .aggregator
            .discover(&filter)
            .await
            .map_err(|e| drbot_skills::SkillError::ExecutionFailed(e.to_string()))?;

        let output: Vec<_> = opportunities
            .into_iter()
            .map(|o| {
                serde_json::json!({
                    "protocol": o.protocol,
                    "id": o.id,
                    "asset": o.asset,
                    "apy": o.apy,
                    "apy_display": format!("{:.2}%", o.apy * 100.0),
                    "tvl_usd": o.tvl_usd,
                    "risk_score": o.risk_score,
                })
            })
            .collect();

        Ok(SkillOutput::new(output))
    }

    async fn handle_positions(&self, input: &SkillInput) -> drbot_skills::Result<SkillOutput> {
        let wallet_str = input
            .params
            .get("wallet")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                drbot_skills::SkillError::ValidationFailed("Wallet address required".to_string())
            })?;

        let wallet = Pubkey::from_str(wallet_str)
            .map_err(|e| drbot_skills::SkillError::ValidationFailed(e.to_string()))?;

        let summary = self
            .aggregator
            .get_portfolio_value(&wallet)
            .await
            .map_err(|e| drbot_skills::SkillError::ExecutionFailed(e.to_string()))?;

        Ok(SkillOutput::new(serde_json::json!({
            "total_value_usd": summary.total_value_usd,
            "weighted_avg_apy": summary.weighted_avg_apy,
            "position_count": summary.position_count,
        })))
    }

    async fn handle_pending(&self) -> drbot_skills::Result<SkillOutput> {
        let pending = self.approval_manager.get_pending().await;

        let output: Vec<_> = pending
            .into_iter()
            .map(|p| {
                serde_json::json!({
                    "id": p.id.to_string(),
                    "protocol": p.opportunity.protocol,
                    "action": format!("{:?}", p.action),
                    "amount_usd": p.amount_usd,
                    "expires_at": p.expires_at.to_rfc3339(),
                })
            })
            .collect();

        Ok(SkillOutput::new(output))
    }

    async fn handle_approve(&self, input: &SkillInput) -> drbot_skills::Result<SkillOutput> {
        let tx_id = input
            .params
            .get("transaction_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                drbot_skills::SkillError::ValidationFailed("Transaction ID required".to_string())
            })?;

        let code = input
            .params
            .get("approval_code")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                drbot_skills::SkillError::ValidationFailed("Approval code required".to_string())
            })?;

        let id = uuid::Uuid::parse_str(tx_id)
            .map_err(|e| drbot_skills::SkillError::ValidationFailed(e.to_string()))?;

        let approved = self
            .approval_manager
            .approve(id, code)
            .await
            .map_err(|e| drbot_skills::SkillError::ExecutionFailed(e.to_string()))?;

        Ok(SkillOutput::new(serde_json::json!({
            "status": "approved",
            "transaction_id": approved.id.to_string(),
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_defi_skill_manifest() {
        let rpc = Arc::new(RpcClient::new("https://api.devnet.solana.com".to_string()));
        let skill = DeFiSkill::new(rpc);
        let manifest = skill.manifest();

        assert_eq!(manifest.name, "defi");
        assert!(manifest.inputs.iter().any(|i| i.name == "action"));
    }
}
