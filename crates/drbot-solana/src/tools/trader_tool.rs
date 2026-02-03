//! Autonomous trader tool for agent use.

use super::compat::{
    Tool, ToolDefinition, ToolError, ToolExecutor, ToolInput, ToolOutput, ToolParameter, ToolResult,
};
use crate::skills::TraderSkill;
use crate::wallet::KeypairManager;
use async_trait::async_trait;
use solana_client::nonblocking::rpc_client::RpcClient;
use std::sync::Arc;

/// Autonomous trader tool for agents.
pub struct TraderTool {
    skill: TraderSkill,
}

impl TraderTool {
    /// Create a new trader tool.
    pub fn new(rpc_client: Arc<RpcClient>, keypair_manager: Option<KeypairManager>) -> Self {
        Self {
            skill: TraderSkill::new(rpc_client, keypair_manager),
        }
    }
}

impl Tool for TraderTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "solana_trader".to_string(),
            description: "Autonomous Solana trading with momentum-based strategy. Supports take profit, stop loss, and trailing stops. Actions: start (begin trading), stop (halt trading), status (check state), positions (view open), history (view closed), summary (performance stats), scan (find opportunities), close (exit position).".to_string(),
            parameters: vec![
                ToolParameter {
                    name: "action".to_string(),
                    description: "Action to perform".to_string(),
                    param_type: "string".to_string(),
                    required: true,
                    enum_values: Some(vec![
                        "start".to_string(),
                        "stop".to_string(),
                        "status".to_string(),
                        "positions".to_string(),
                        "history".to_string(),
                        "summary".to_string(),
                        "scan".to_string(),
                        "close".to_string(),
                    ]),
                    default: None,
                },
                ToolParameter {
                    name: "strategy".to_string(),
                    description: "Strategy preset for 'start': default (+50% TP, -25% SL), aggressive (+100% TP, -30% SL), conservative (+25% TP, -15% SL), scalping (+10% TP, -5% SL)".to_string(),
                    param_type: "string".to_string(),
                    required: false,
                    enum_values: Some(vec![
                        "default".to_string(),
                        "aggressive".to_string(),
                        "conservative".to_string(),
                        "scalping".to_string(),
                    ]),
                    default: Some(serde_json::json!("default")),
                },
                ToolParameter {
                    name: "take_profit_pct".to_string(),
                    description: "Custom take profit percentage (overrides strategy)".to_string(),
                    param_type: "number".to_string(),
                    required: false,
                    enum_values: None,
                    default: None,
                },
                ToolParameter {
                    name: "stop_loss_pct".to_string(),
                    description: "Custom stop loss percentage (overrides strategy)".to_string(),
                    param_type: "number".to_string(),
                    required: false,
                    enum_values: None,
                    default: None,
                },
                ToolParameter {
                    name: "max_position_size_usd".to_string(),
                    description: "Maximum position size in USD".to_string(),
                    param_type: "number".to_string(),
                    required: false,
                    enum_values: None,
                    default: Some(serde_json::json!(100)),
                },
                ToolParameter {
                    name: "position_id".to_string(),
                    description: "Position ID for 'close' action".to_string(),
                    param_type: "string".to_string(),
                    required: false,
                    enum_values: None,
                    default: None,
                },
                ToolParameter {
                    name: "limit".to_string(),
                    description: "Result limit for 'scan' action".to_string(),
                    param_type: "number".to_string(),
                    required: false,
                    enum_values: None,
                    default: Some(serde_json::json!(10)),
                },
            ],
            category: Some("trading".to_string()),
            examples: Some(vec![
                r#"{"action": "start", "strategy": "default"}"#.to_string(),
                r#"{"action": "start", "take_profit_pct": 75, "stop_loss_pct": 20}"#.to_string(),
                r#"{"action": "status"}"#.to_string(),
                r#"{"action": "positions"}"#.to_string(),
                r#"{"action": "scan", "limit": 5}"#.to_string(),
                r#"{"action": "close", "position_id": "..."}"#.to_string(),
                r#"{"action": "stop"}"#.to_string(),
            ]),
        }
    }
}

#[async_trait]
impl ToolExecutor for TraderTool {
    async fn execute(&self, input: ToolInput) -> ToolResult {
        use drbot_skills::Skill;

        let skill_input = drbot_skills::SkillInput {
            params: input.to_params(),
            text: None,
            attachments: vec![],
        };

        let context = drbot_skills::SkillContext::default();

        match self.skill.execute(skill_input, &context).await {
            Ok(output) => Ok(ToolOutput {
                result: output.data,
                metadata: Default::default(),
            }),
            Err(e) => Err(ToolError::ExecutionFailed(e.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trader_tool_definition() {
        let rpc = Arc::new(RpcClient::new("https://api.devnet.solana.com".to_string()));
        let tool = TraderTool::new(rpc, None);
        let def = tool.definition();

        assert_eq!(def.name, "solana_trader");
        assert!(def.parameters.iter().any(|p| p.name == "action"));
        assert!(def.parameters.iter().any(|p| p.name == "strategy"));
    }
}
