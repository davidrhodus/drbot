//! Risk analysis tool for agent use.

use super::compat::{
    Tool, ToolDefinition, ToolError, ToolExecutor, ToolInput, ToolOutput, ToolParameter, ToolResult,
};
use crate::skills::RiskSkill;
use async_trait::async_trait;
use solana_client::nonblocking::rpc_client::RpcClient;
use std::sync::Arc;

/// Risk analysis tool for agent use.
pub struct RiskTool {
    skill: RiskSkill,
}

impl RiskTool {
    /// Create a new risk tool.
    pub fn new(rpc_client: Arc<RpcClient>) -> Self {
        Self {
            skill: RiskSkill::new(rpc_client),
        }
    }
}

impl Tool for RiskTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "solana_risk".to_string(),
            description: "Analyze portfolio risk, correlations, and protocol dependencies"
                .to_string(),
            parameters: vec![
                ToolParameter {
                    name: "action".to_string(),
                    description: "Action to perform".to_string(),
                    param_type: "string".to_string(),
                    required: true,
                    enum_values: Some(vec![
                        "analyze".to_string(),
                        "correlations".to_string(),
                        "dependencies".to_string(),
                        "alerts".to_string(),
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
                    name: "protocols".to_string(),
                    description: "Protocols to analyze (comma-separated)".to_string(),
                    param_type: "string".to_string(),
                    required: false,
                    enum_values: None,
                    default: None,
                },
            ],
            category: Some("risk".to_string()),
            examples: Some(vec![
                r#"{"action": "analyze", "wallet": "..."}"#.to_string(),
                r#"{"action": "dependencies", "protocols": "Solend,Marginfi"}"#.to_string(),
            ]),
        }
    }
}

#[async_trait]
impl ToolExecutor for RiskTool {
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
    fn test_risk_tool_definition() {
        let rpc = Arc::new(RpcClient::new("https://api.devnet.solana.com".to_string()));
        let tool = RiskTool::new(rpc);
        let def = tool.definition();

        assert_eq!(def.name, "solana_risk");
    }
}
