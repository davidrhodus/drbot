//! Swap tool for agent use.

use super::compat::{
    Tool, ToolDefinition, ToolError, ToolExecutor, ToolInput, ToolOutput, ToolParameter, ToolResult,
};
use crate::skills::SwapSkill;
use crate::trading::JupiterClient;
use crate::wallet::KeypairManager;
use async_trait::async_trait;
use solana_client::nonblocking::rpc_client::RpcClient;
use std::sync::Arc;

/// Swap tool for Jupiter DEX operations.
pub struct SwapTool {
    skill: SwapSkill,
}

impl SwapTool {
    /// Create a new swap tool.
    pub fn new(
        rpc_client: Arc<RpcClient>,
        jupiter: JupiterClient,
        keypair_manager: Option<KeypairManager>,
    ) -> Self {
        Self {
            skill: SwapSkill::new(rpc_client, jupiter, keypair_manager),
        }
    }
}

impl Tool for SwapTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "solana_swap".to_string(),
            description: "Get quotes and execute token swaps on Solana via Jupiter DEX aggregator. Use 'quote' to get a price quote, 'execute' to perform the swap.".to_string(),
            parameters: vec![
                ToolParameter {
                    name: "action".to_string(),
                    description: "Action to perform: 'quote' for price quote, 'execute' to swap".to_string(),
                    param_type: "string".to_string(),
                    required: true,
                    enum_values: Some(vec!["quote".to_string(), "execute".to_string()]),
                    default: None,
                },
                ToolParameter {
                    name: "input_mint".to_string(),
                    description: "Input token: use 'SOL', 'USDC', 'USDT' for common tokens, or the mint address".to_string(),
                    param_type: "string".to_string(),
                    required: true,
                    enum_values: None,
                    default: None,
                },
                ToolParameter {
                    name: "output_mint".to_string(),
                    description: "Output token: use 'SOL', 'USDC', 'USDT' for common tokens, or the mint address".to_string(),
                    param_type: "string".to_string(),
                    required: true,
                    enum_values: None,
                    default: None,
                },
                ToolParameter {
                    name: "amount".to_string(),
                    description: "Amount to swap in UI units (e.g., 1.5 for 1.5 SOL)".to_string(),
                    param_type: "number".to_string(),
                    required: true,
                    enum_values: None,
                    default: None,
                },
                ToolParameter {
                    name: "slippage_bps".to_string(),
                    description: "Slippage tolerance in basis points (50 = 0.5%, default)".to_string(),
                    param_type: "number".to_string(),
                    required: false,
                    enum_values: None,
                    default: Some(serde_json::json!(50)),
                },
            ],
            category: Some("defi".to_string()),
            examples: Some(vec![
                r#"{"action": "quote", "input_mint": "SOL", "output_mint": "USDC", "amount": 1.0}"#.to_string(),
                r#"{"action": "execute", "input_mint": "SOL", "output_mint": "USDC", "amount": 0.5, "slippage_bps": 100}"#.to_string(),
            ]),
        }
    }
}

#[async_trait]
impl ToolExecutor for SwapTool {
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
    fn test_swap_tool_definition() {
        let rpc = Arc::new(RpcClient::new("https://api.devnet.solana.com".to_string()));
        let jupiter = JupiterClient::new("https://quote-api.jup.ag/v6".to_string());
        let tool = SwapTool::new(rpc, jupiter, None);
        let def = tool.definition();

        assert_eq!(def.name, "solana_swap");
        assert!(def.parameters.iter().any(|p| p.name == "input_mint"));
        assert!(def.parameters.iter().any(|p| p.name == "output_mint"));
    }
}
