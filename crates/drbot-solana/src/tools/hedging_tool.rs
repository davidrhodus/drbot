//! Hedging tool for agent use.

use super::compat::{
    Tool, ToolDefinition, ToolError, ToolExecutor, ToolInput, ToolOutput, ToolParameter, ToolResult,
};
use crate::skills::HedgingSkill;
use async_trait::async_trait;
use solana_client::nonblocking::rpc_client::RpcClient;
use std::sync::Arc;

/// Hedging tool for agent use.
pub struct HedgingTool {
    skill: HedgingSkill,
}

impl HedgingTool {
    /// Create a new hedging tool.
    pub fn new(rpc_client: Arc<RpcClient>) -> Self {
        Self {
            skill: HedgingSkill::new(rpc_client),
        }
    }
}

impl Tool for HedgingTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "solana_hedging".to_string(),
            description:
                "Market neutral hedging: calculate delta, create hedge plans, rebalance portfolio"
                    .to_string(),
            parameters: vec![
                ToolParameter {
                    name: "action".to_string(),
                    description: "Action to perform".to_string(),
                    param_type: "string".to_string(),
                    required: true,
                    enum_values: Some(vec![
                        "delta".to_string(),
                        "hedge_plan".to_string(),
                        "rebalance".to_string(),
                        "market_neutral".to_string(),
                        "execute".to_string(),
                    ]),
                    default: None,
                },
                ToolParameter {
                    name: "wallet".to_string(),
                    description: "Wallet address to analyze".to_string(),
                    param_type: "string".to_string(),
                    required: false,
                    enum_values: None,
                    default: None,
                },
                ToolParameter {
                    name: "target_delta".to_string(),
                    description: "Target delta (0 for market neutral)".to_string(),
                    param_type: "number".to_string(),
                    required: false,
                    enum_values: None,
                    default: None,
                },
                ToolParameter {
                    name: "plan_id".to_string(),
                    description: "Hedge plan ID to execute".to_string(),
                    param_type: "string".to_string(),
                    required: false,
                    enum_values: None,
                    default: None,
                },
                ToolParameter {
                    name: "auto_execute".to_string(),
                    description: "Auto-execute the plan".to_string(),
                    param_type: "boolean".to_string(),
                    required: false,
                    enum_values: None,
                    default: None,
                },
            ],
            category: Some("risk".to_string()),
            examples: Some(vec![
                r#"{"action": "delta", "wallet": "..."}"#.to_string(),
                r#"{"action": "hedge_plan", "wallet": "...", "target_delta": 0}"#.to_string(),
                r#"{"action": "market_neutral", "wallet": "..."}"#.to_string(),
            ]),
        }
    }
}

#[async_trait]
impl ToolExecutor for HedgingTool {
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
    fn test_hedging_tool_definition() {
        let rpc = Arc::new(RpcClient::new("https://api.devnet.solana.com".to_string()));
        let tool = HedgingTool::new(rpc);
        let def = tool.definition();

        assert_eq!(def.name, "solana_hedging");
    }
}
