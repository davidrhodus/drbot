//! OTC trading tool for agent use.

use super::compat::{
    Tool, ToolDefinition, ToolError, ToolExecutor, ToolInput, ToolOutput, ToolParameter, ToolResult,
};
use crate::skills::OTCSkill;
use async_trait::async_trait;
use solana_client::nonblocking::rpc_client::RpcClient;
use std::sync::Arc;

/// OTC trading tool for agent use.
pub struct OTCTool {
    skill: OTCSkill,
}

impl OTCTool {
    /// Create a new OTC tool.
    pub fn new(rpc_client: Arc<RpcClient>) -> Self {
        Self {
            skill: OTCSkill::new(rpc_client),
        }
    }
}

impl Tool for OTCTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "solana_otc".to_string(),
            description: "Agent-to-agent OTC trading: send RFQs, receive quotes, negotiate trades"
                .to_string(),
            parameters: vec![
                ToolParameter {
                    name: "action".to_string(),
                    description: "Action to perform".to_string(),
                    param_type: "string".to_string(),
                    required: true,
                    enum_values: Some(vec![
                        "rfq".to_string(),
                        "quote".to_string(),
                        "accept".to_string(),
                        "reject".to_string(),
                        "status".to_string(),
                        "history".to_string(),
                    ]),
                    default: None,
                },
                ToolParameter {
                    name: "asset".to_string(),
                    description: "Asset to trade".to_string(),
                    param_type: "string".to_string(),
                    required: false,
                    enum_values: None,
                    default: None,
                },
                ToolParameter {
                    name: "direction".to_string(),
                    description: "Trade direction".to_string(),
                    param_type: "string".to_string(),
                    required: false,
                    enum_values: Some(vec!["buy".to_string(), "sell".to_string()]),
                    default: None,
                },
                ToolParameter {
                    name: "amount".to_string(),
                    description: "Amount to trade".to_string(),
                    param_type: "number".to_string(),
                    required: false,
                    enum_values: None,
                    default: None,
                },
                ToolParameter {
                    name: "price".to_string(),
                    description: "Price for quote".to_string(),
                    param_type: "number".to_string(),
                    required: false,
                    enum_values: None,
                    default: None,
                },
                ToolParameter {
                    name: "rfq_id".to_string(),
                    description: "RFQ ID to respond to".to_string(),
                    param_type: "string".to_string(),
                    required: false,
                    enum_values: None,
                    default: None,
                },
                ToolParameter {
                    name: "quote_id".to_string(),
                    description: "Quote ID to accept/reject".to_string(),
                    param_type: "string".to_string(),
                    required: false,
                    enum_values: None,
                    default: None,
                },
            ],
            category: Some("trading".to_string()),
            examples: Some(vec![
                r#"{"action": "rfq", "asset": "SOL", "direction": "buy", "amount": 10}"#
                    .to_string(),
                r#"{"action": "status"}"#.to_string(),
            ]),
        }
    }
}

#[async_trait]
impl ToolExecutor for OTCTool {
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
    fn test_otc_tool_definition() {
        let rpc = Arc::new(RpcClient::new("https://api.devnet.solana.com".to_string()));
        let tool = OTCTool::new(rpc);
        let def = tool.definition();

        assert_eq!(def.name, "solana_otc");
    }
}
