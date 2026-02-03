//! DeFi tool for agent use.

use super::compat::{
    Tool, ToolDefinition, ToolError, ToolExecutor, ToolInput, ToolOutput, ToolParameter, ToolResult,
};
use crate::skills::DeFiSkill;
use async_trait::async_trait;
use solana_client::nonblocking::rpc_client::RpcClient;
use std::sync::Arc;

/// DeFi tool for agent use.
pub struct DeFiTool {
    skill: DeFiSkill,
}

impl DeFiTool {
    /// Create a new DeFi tool.
    pub fn new(rpc_client: Arc<RpcClient>) -> Self {
        Self {
            skill: DeFiSkill::new(rpc_client),
        }
    }
}

impl Tool for DeFiTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "solana_defi".to_string(),
            description: "Discover DeFi yield opportunities and manage positions across Solana protocols (Solend, Marginfi, Kamino, Marinade, Jito)".to_string(),
            parameters: vec![
                ToolParameter {
                    name: "action".to_string(),
                    description: "Action to perform".to_string(),
                    param_type: "string".to_string(),
                    required: true,
                    enum_values: Some(vec![
                        "discover".to_string(),
                        "positions".to_string(),
                        "pending".to_string(),
                        "approve".to_string(),
                    ]),
                    default: None,
                },
                ToolParameter {
                    name: "protocol".to_string(),
                    description: "Protocol to filter by".to_string(),
                    param_type: "string".to_string(),
                    required: false,
                    enum_values: Some(vec![
                        "Solend".to_string(),
                        "Marginfi".to_string(),
                        "Kamino".to_string(),
                        "Marinade".to_string(),
                        "Jito".to_string(),
                    ]),
                    default: None,
                },
                ToolParameter {
                    name: "min_apy".to_string(),
                    description: "Minimum APY (decimal, e.g., 0.05 for 5%)".to_string(),
                    param_type: "number".to_string(),
                    required: false,
                    enum_values: None,
                    default: None,
                },
                ToolParameter {
                    name: "max_risk".to_string(),
                    description: "Maximum risk score (1-10)".to_string(),
                    param_type: "number".to_string(),
                    required: false,
                    enum_values: None,
                    default: None,
                },
                ToolParameter {
                    name: "wallet".to_string(),
                    description: "Wallet address for positions query".to_string(),
                    param_type: "string".to_string(),
                    required: false,
                    enum_values: None,
                    default: None,
                },
                ToolParameter {
                    name: "limit".to_string(),
                    description: "Maximum results".to_string(),
                    param_type: "number".to_string(),
                    required: false,
                    enum_values: None,
                    default: None,
                },
            ],
            category: Some("defi".to_string()),
            examples: Some(vec![
                r#"{"action": "discover", "min_apy": 0.05, "max_risk": 5}"#.to_string(),
                r#"{"action": "positions", "wallet": "..."}"#.to_string(),
            ]),
        }
    }
}

#[async_trait]
impl ToolExecutor for DeFiTool {
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
    fn test_defi_tool_definition() {
        let rpc = Arc::new(RpcClient::new("https://api.devnet.solana.com".to_string()));
        let tool = DeFiTool::new(rpc);
        let def = tool.definition();

        assert_eq!(def.name, "solana_defi");
    }
}
