//! Smart contract monitoring skill.

use crate::monitor::{
    DiffAnalyzer, ProgramEvent, ProgramWatcher, UpgradeDetector, UpgradeDetectorConfig,
    WatchedProgram,
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
use tokio::sync::RwLock;

/// Smart contract monitoring skill.
pub struct MonitorSkill {
    manifest: SkillManifest,
    watcher: Arc<RwLock<ProgramWatcher>>,
    upgrade_detector: Arc<RwLock<UpgradeDetector>>,
}

impl MonitorSkill {
    /// Create a new monitor skill.
    pub fn new(rpc_client: Arc<RpcClient>) -> Self {
        let manifest = SkillManifest {
            name: "monitor".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            description: "Monitor smart contract upgrades and detect suspicious changes"
                .to_string(),
            author: Some("drbot".to_string()),
            license: Some("MIT".to_string()),
            homepage: None,
            repository: None,
            tags: vec![
                "solana".to_string(),
                "monitor".to_string(),
                "upgrade".to_string(),
            ],
            inputs: vec![
                ManifestInput {
                    name: "action".to_string(),
                    description: "Action: watch, unwatch, list, check, events, analyze".to_string(),
                    param_type: "string".to_string(),
                    required: true,
                    default: None,
                    pattern: None,
                    enum_values: vec![],
                },
                ManifestInput {
                    name: "program_id".to_string(),
                    description: "Program address to watch/check".to_string(),
                    param_type: "string".to_string(),
                    required: false,
                    default: None,
                    pattern: None,
                    enum_values: vec![],
                },
                ManifestInput {
                    name: "name".to_string(),
                    description: "Human-readable name for the program".to_string(),
                    param_type: "string".to_string(),
                    required: false,
                    default: None,
                    pattern: None,
                    enum_values: vec![],
                },
                ManifestInput {
                    name: "limit".to_string(),
                    description: "Maximum events to return".to_string(),
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
                ManifestCapability::required("monitor"),
                ManifestCapability::required("security"),
            ],
            entry_point: None,
            runtime: None,
        };

        Self {
            manifest,
            watcher: Arc::new(RwLock::new(ProgramWatcher::new(rpc_client))),
            upgrade_detector: Arc::new(RwLock::new(UpgradeDetector::new(
                UpgradeDetectorConfig::default(),
            ))),
        }
    }

    /// Create with default watch list.
    pub async fn with_defaults(rpc_client: Arc<RpcClient>) -> Result<Self> {
        let mut skill = Self::new(rpc_client.clone());
        let watcher = ProgramWatcher::with_defaults(rpc_client).await?;
        skill.watcher = Arc::new(RwLock::new(watcher));
        Ok(skill)
    }
}

#[async_trait]
impl Skill for MonitorSkill {
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
            "watch" => {
                if input
                    .params
                    .get("program_id")
                    .and_then(|v| v.as_str())
                    .is_none()
                {
                    return Err(drbot_skills::SkillError::ValidationFailed(
                        "Program ID required for watch".to_string(),
                    ));
                }
                Ok(())
            }
            "unwatch" | "check" | "analyze" => {
                if input
                    .params
                    .get("program_id")
                    .and_then(|v| v.as_str())
                    .is_none()
                {
                    return Err(drbot_skills::SkillError::ValidationFailed(
                        "Program ID required".to_string(),
                    ));
                }
                Ok(())
            }
            "list" | "events" => Ok(()),
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
            .unwrap_or("list");

        match action {
            "watch" => self.handle_watch(&input).await,
            "unwatch" => self.handle_unwatch(&input).await,
            "list" => self.handle_list().await,
            "check" => self.handle_check(&input).await,
            "events" => self.handle_events(&input).await,
            "analyze" => self.handle_analyze(&input).await,
            _ => Err(drbot_skills::SkillError::ExecutionFailed(format!(
                "Action '{}' not implemented",
                action
            ))),
        }
    }
}

impl MonitorSkill {
    async fn handle_watch(&self, input: &SkillInput) -> drbot_skills::Result<SkillOutput> {
        let program_id_str = input
            .params
            .get("program_id")
            .and_then(|v| v.as_str())
            .unwrap();

        let program_id = Pubkey::from_str(program_id_str)
            .map_err(|e| drbot_skills::SkillError::ValidationFailed(e.to_string()))?;

        let name = input
            .params
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown")
            .to_string();

        self.watcher
            .write()
            .await
            .watch(program_id, name.clone())
            .await
            .map_err(|e| drbot_skills::SkillError::ExecutionFailed(e.to_string()))?;

        Ok(SkillOutput::new(serde_json::json!({
            "status": "watching",
            "program_id": program_id_str,
            "name": name,
        })))
    }

    async fn handle_unwatch(&self, input: &SkillInput) -> drbot_skills::Result<SkillOutput> {
        let program_id_str = input
            .params
            .get("program_id")
            .and_then(|v| v.as_str())
            .unwrap();

        let program_id = Pubkey::from_str(program_id_str)
            .map_err(|e| drbot_skills::SkillError::ValidationFailed(e.to_string()))?;

        let removed = self.watcher.write().await.unwatch(&program_id).await;

        Ok(SkillOutput::new(serde_json::json!({
            "status": if removed { "removed" } else { "not_found" },
            "program_id": program_id_str,
        })))
    }

    async fn handle_list(&self) -> drbot_skills::Result<SkillOutput> {
        let programs = self.watcher.read().await.get_watched().await;

        let output: Vec<WatchedProgramOutput> = programs
            .into_iter()
            .map(|p| {
                let is_immutable = p.is_immutable();
                WatchedProgramOutput {
                    address: p.address.to_string(),
                    name: p.name,
                    is_upgradeable: p.is_upgradeable,
                    is_immutable,
                    upgrade_authority: p.upgrade_authority.map(|a| a.to_string()),
                    last_deployed_slot: p.last_deployed_slot,
                    last_checked: p.last_checked_at.to_rfc3339(),
                }
            })
            .collect();

        Ok(SkillOutput::new(serde_json::to_value(output).map_err(
            |e| drbot_skills::SkillError::ExecutionFailed(e.to_string()),
        )?))
    }

    async fn handle_check(&self, input: &SkillInput) -> drbot_skills::Result<SkillOutput> {
        let program_id_str = input
            .params
            .get("program_id")
            .and_then(|v| v.as_str())
            .unwrap();

        let program_id = Pubkey::from_str(program_id_str)
            .map_err(|e| drbot_skills::SkillError::ValidationFailed(e.to_string()))?;

        let events = self
            .watcher
            .read()
            .await
            .check_program(&program_id)
            .await
            .map_err(|e| drbot_skills::SkillError::ExecutionFailed(e.to_string()))?;

        // Analyze events for risk
        let mut detector = self.upgrade_detector.write().await;
        let upgrade_events: Vec<_> = events
            .iter()
            .filter_map(|e| detector.analyze_event(e))
            .collect();

        let output = CheckOutput {
            program_id: program_id_str.to_string(),
            events_detected: events.len(),
            events: events.iter().map(EventOutput::from).collect(),
            risk_assessments: upgrade_events
                .iter()
                .map(|e| RiskAssessmentOutput {
                    event_type: format!("{:?}", e.event_type),
                    risk_level: format!("{:?}", e.risk_assessment.level),
                    risk_score: e.risk_assessment.score,
                    factors: e
                        .risk_assessment
                        .factors
                        .iter()
                        .map(|f| f.name.clone())
                        .collect(),
                    recommendations: e.risk_assessment.recommendations.clone(),
                })
                .collect(),
        };

        Ok(SkillOutput::new(serde_json::to_value(output).map_err(
            |e| drbot_skills::SkillError::ExecutionFailed(e.to_string()),
        )?))
    }

    async fn handle_events(&self, input: &SkillInput) -> drbot_skills::Result<SkillOutput> {
        let limit = input
            .params
            .get("limit")
            .and_then(|v| v.as_u64())
            .map(|l| l as usize);

        let events = self.watcher.read().await.get_events(limit).await;

        let output: Vec<EventOutput> = events.iter().map(EventOutput::from).collect();

        Ok(SkillOutput::new(serde_json::to_value(output).map_err(
            |e| drbot_skills::SkillError::ExecutionFailed(e.to_string()),
        )?))
    }

    async fn handle_analyze(&self, input: &SkillInput) -> drbot_skills::Result<SkillOutput> {
        let program_id_str = input
            .params
            .get("program_id")
            .and_then(|v| v.as_str())
            .unwrap();

        let program_id = Pubkey::from_str(program_id_str)
            .map_err(|e| drbot_skills::SkillError::ValidationFailed(e.to_string()))?;

        let program = self
            .watcher
            .read()
            .await
            .get_program(&program_id)
            .await
            .ok_or_else(|| {
                drbot_skills::SkillError::ExecutionFailed("Program not in watch list".to_string())
            })?;

        let analyzer = DiffAnalyzer::new();
        let diff = analyzer.create_minimal_diff(
            program.address,
            program.name.clone(),
            "unknown".to_string(),
            program.data_hash.clone(),
            0,
        );

        let is_immutable = program.is_immutable();
        let output = AnalyzeOutput {
            program_id: program_id_str.to_string(),
            name: program.name,
            is_upgradeable: program.is_upgradeable,
            is_immutable,
            has_risk_indicators: diff.has_risk_indicators(),
            risk_indicators: diff
                .risk_indicators
                .iter()
                .map(|r| RiskIndicatorOutput {
                    name: r.name.clone(),
                    description: r.description.clone(),
                    severity: format!("{:?}", r.severity),
                })
                .collect(),
        };

        Ok(SkillOutput::new(serde_json::to_value(output).map_err(
            |e| drbot_skills::SkillError::ExecutionFailed(e.to_string()),
        )?))
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct WatchedProgramOutput {
    address: String,
    name: String,
    is_upgradeable: bool,
    is_immutable: bool,
    upgrade_authority: Option<String>,
    last_deployed_slot: u64,
    last_checked: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct EventOutput {
    event_type: String,
    program: String,
    timestamp: String,
}

impl From<&ProgramEvent> for EventOutput {
    fn from(event: &ProgramEvent) -> Self {
        Self {
            event_type: event.event_type().to_string(),
            program: event.program_id().to_string(),
            timestamp: event.timestamp().to_rfc3339(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct CheckOutput {
    program_id: String,
    events_detected: usize,
    events: Vec<EventOutput>,
    risk_assessments: Vec<RiskAssessmentOutput>,
}

#[derive(Debug, Serialize, Deserialize)]
struct RiskAssessmentOutput {
    event_type: String,
    risk_level: String,
    risk_score: u8,
    factors: Vec<String>,
    recommendations: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct AnalyzeOutput {
    program_id: String,
    name: String,
    is_upgradeable: bool,
    is_immutable: bool,
    has_risk_indicators: bool,
    risk_indicators: Vec<RiskIndicatorOutput>,
}

#[derive(Debug, Serialize, Deserialize)]
struct RiskIndicatorOutput {
    name: String,
    description: String,
    severity: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_monitor_skill_manifest() {
        let rpc = Arc::new(RpcClient::new("https://api.devnet.solana.com".to_string()));
        let skill = MonitorSkill::new(rpc);
        let manifest = skill.manifest();

        assert_eq!(manifest.name, "monitor");
    }
}
