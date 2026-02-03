//! Risk analysis skill for portfolio risk management.

use crate::defi::YieldAggregator;
use crate::risk::{
    AlertManager, CorrelationMatrix, PortfolioAnalyzer, PortfolioRisk, ProtocolDependencyGraph,
    RiskAlert, RiskConfig,
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

/// Risk analysis skill for portfolio risk management.
pub struct RiskSkill {
    manifest: SkillManifest,
    rpc_client: Arc<RpcClient>,
    analyzer: PortfolioAnalyzer,
    dependency_graph: ProtocolDependencyGraph,
    aggregator: YieldAggregator,
    alert_manager: Arc<tokio::sync::RwLock<AlertManager>>,
}

impl RiskSkill {
    /// Create a new risk skill.
    pub fn new(rpc_client: Arc<RpcClient>) -> Self {
        let manifest = SkillManifest {
            name: "risk".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            description: "Analyze portfolio risk, correlations, and protocol dependencies"
                .to_string(),
            author: Some("drbot".to_string()),
            license: Some("MIT".to_string()),
            homepage: None,
            repository: None,
            tags: vec![
                "solana".to_string(),
                "risk".to_string(),
                "portfolio".to_string(),
            ],
            inputs: vec![
                ManifestInput {
                    name: "action".to_string(),
                    description: "Action: analyze, correlations, dependencies, alerts".to_string(),
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
                    name: "protocols".to_string(),
                    description: "Protocols to analyze (comma-separated)".to_string(),
                    param_type: "string".to_string(),
                    required: false,
                    default: None,
                    pattern: None,
                    enum_values: vec![],
                },
            ],
            outputs: vec![ManifestOutput {
                name: "result".to_string(),
                description: "Risk analysis result".to_string(),
                output_type: "object".to_string(),
            }],
            capabilities: vec![
                ManifestCapability::required("risk"),
                ManifestCapability::required("analysis"),
            ],
            entry_point: None,
            runtime: None,
        };

        Self {
            manifest,
            rpc_client: rpc_client.clone(),
            analyzer: PortfolioAnalyzer::new(RiskConfig::default()),
            dependency_graph: ProtocolDependencyGraph::solana_defaults(),
            aggregator: YieldAggregator::new(rpc_client),
            alert_manager: Arc::new(tokio::sync::RwLock::new(AlertManager::default())),
        }
    }
}

#[async_trait]
impl Skill for RiskSkill {
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
            "analyze" | "correlations" => {
                if input
                    .params
                    .get("wallet")
                    .and_then(|v| v.as_str())
                    .is_none()
                {
                    return Err(drbot_skills::SkillError::ValidationFailed(
                        "Wallet required for analysis".to_string(),
                    ));
                }
                Ok(())
            }
            "dependencies" | "alerts" => Ok(()),
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
            .unwrap_or("analyze");

        match action {
            "analyze" => self.handle_analyze(&input).await,
            "correlations" => self.handle_correlations(&input).await,
            "dependencies" => self.handle_dependencies(&input).await,
            "alerts" => self.handle_alerts().await,
            _ => Err(drbot_skills::SkillError::ExecutionFailed(format!(
                "Action '{}' not implemented",
                action
            ))),
        }
    }
}

impl RiskSkill {
    async fn handle_analyze(&self, input: &SkillInput) -> drbot_skills::Result<SkillOutput> {
        let wallet_str = input
            .params
            .get("wallet")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                drbot_skills::SkillError::ValidationFailed("Wallet required".to_string())
            })?;

        let wallet = Pubkey::from_str(wallet_str)
            .map_err(|e| drbot_skills::SkillError::ValidationFailed(e.to_string()))?;

        let positions = self
            .aggregator
            .get_all_positions(&wallet)
            .await
            .map_err(|e| drbot_skills::SkillError::ExecutionFailed(e.to_string()))?;

        let risk = self.analyzer.analyze(&positions, None);

        let mut alert_manager = self.alert_manager.write().await;
        alert_manager.add_all(risk.alerts.clone());

        Ok(SkillOutput::new(serde_json::json!({
            "total_value_usd": risk.total_value_usd,
            "var_95": risk.var_95,
            "var_99": risk.var_99,
            "risk_score": risk.risk_score(),
            "is_high_risk": risk.is_high_risk(),
            "concentration": {
                "herfindahl_index": risk.concentration.herfindahl_index,
                "largest_position_pct": risk.concentration.largest_position_pct,
                "effective_positions": risk.concentration.effective_positions,
                "is_concentrated": risk.concentration.is_concentrated(),
            },
            "protocol_exposure": risk.protocol_exposure,
            "alerts": risk.alerts.iter().map(|a| serde_json::json!({
                "title": a.title(),
                "severity": format!("{:?}", a.severity()),
            })).collect::<Vec<_>>(),
        })))
    }

    async fn handle_correlations(&self, input: &SkillInput) -> drbot_skills::Result<SkillOutput> {
        let wallet_str = input
            .params
            .get("wallet")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                drbot_skills::SkillError::ValidationFailed("Wallet required".to_string())
            })?;

        let wallet = Pubkey::from_str(wallet_str)
            .map_err(|e| drbot_skills::SkillError::ValidationFailed(e.to_string()))?;

        let positions = self
            .aggregator
            .get_all_positions(&wallet)
            .await
            .map_err(|e| drbot_skills::SkillError::ExecutionFailed(e.to_string()))?;

        let assets: Vec<String> = positions.iter().map(|p| p.asset_symbol.clone()).collect();
        let matrix = CorrelationMatrix::default_for_assets(&assets);

        Ok(SkillOutput::new(serde_json::json!({
            "assets": matrix.assets,
            "average_correlation": matrix.average_correlation(),
            "high_correlation_pairs": matrix.high_correlation_pairs.iter().map(|p| serde_json::json!({
                "asset_a": p.asset_a,
                "asset_b": p.asset_b,
                "correlation": p.correlation,
                "correlation_type": p.correlation_type().to_string(),
            })).collect::<Vec<_>>(),
        })))
    }

    async fn handle_dependencies(&self, input: &SkillInput) -> drbot_skills::Result<SkillOutput> {
        let protocols: Vec<String> =
            if let Some(protocols_str) = input.params.get("protocols").and_then(|v| v.as_str()) {
                protocols_str
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .collect()
            } else {
                vec![
                    "Solend".to_string(),
                    "Marginfi".to_string(),
                    "Marinade".to_string(),
                ]
            };

        let analysis = self.dependency_graph.analyze_systemic_risk(&protocols);

        Ok(SkillOutput::new(serde_json::json!({
            "protocols_analyzed": analysis.protocols_analyzed,
            "systemic_risk_score": analysis.systemic_risk_score,
            "critical_dependencies": analysis.critical_dependencies.iter().map(|d| serde_json::json!({
                "protocol": d.protocol,
                "dependent_count": d.dependent_count,
                "dependents": d.dependents,
                "dependency_type": format!("{:?}", d.dependency_type),
            })).collect::<Vec<_>>(),
        })))
    }

    async fn handle_alerts(&self) -> drbot_skills::Result<SkillOutput> {
        let manager = self.alert_manager.read().await;
        let alerts = manager.active();

        let output: Vec<_> = alerts
            .iter()
            .map(|a| {
                serde_json::json!({
                    "id": a.id.to_string(),
                    "title": a.alert.title(),
                    "description": a.alert.description(),
                    "severity": format!("{:?}", a.alert.severity()),
                    "created_at": a.created_at.to_rfc3339(),
                })
            })
            .collect();

        Ok(SkillOutput::new(output))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_risk_skill_manifest() {
        let rpc = Arc::new(RpcClient::new("https://api.devnet.solana.com".to_string()));
        let skill = RiskSkill::new(rpc);
        let manifest = skill.manifest();

        assert_eq!(manifest.name, "risk");
    }
}
