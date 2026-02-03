//! Wallet tool for agent use.

use super::compat::{
    Tool, ToolDefinition, ToolError, ToolExecutor, ToolInput, ToolOutput, ToolParameter, ToolResult,
};
use crate::skills::{WalletBalanceOutput, WalletSkill};
use crate::wallet::KeypairManager;
use async_trait::async_trait;
use serde_json::Value;
use solana_client::nonblocking::rpc_client::RpcClient;
use std::sync::Arc;

/// Wallet tool for Solana operations.
pub struct WalletTool {
    skill: WalletSkill,
}

impl WalletTool {
    /// Create a new wallet tool.
    pub fn new(rpc_client: Arc<RpcClient>, keypair_manager: Option<KeypairManager>) -> Self {
        Self {
            skill: WalletSkill::new(rpc_client, keypair_manager),
        }
    }
}

impl Tool for WalletTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "solana_wallet".to_string(),
            description: "Check Solana wallet balances and perform transfers. Use action 'balance' to check balances or 'transfer' to send SOL/tokens.".to_string(),
            parameters: vec![
                ToolParameter {
                    name: "action".to_string(),
                    description: "Action to perform: 'balance' to check balances, 'transfer' to send funds".to_string(),
                    param_type: "string".to_string(),
                    required: true,
                    enum_values: Some(vec!["balance".to_string(), "transfer".to_string()]),
                    default: None,
                },
                ToolParameter {
                    name: "address".to_string(),
                    description: "Wallet address to query (for balance) or destination (for transfer)".to_string(),
                    param_type: "string".to_string(),
                    required: false,
                    enum_values: None,
                    default: None,
                },
                ToolParameter {
                    name: "amount".to_string(),
                    description: "Amount to transfer in SOL (for transfer action)".to_string(),
                    param_type: "number".to_string(),
                    required: false,
                    enum_values: None,
                    default: None,
                },
                ToolParameter {
                    name: "token_mint".to_string(),
                    description: "Token mint address for SPL token transfers (optional)".to_string(),
                    param_type: "string".to_string(),
                    required: false,
                    enum_values: None,
                    default: None,
                },
            ],
            category: Some("blockchain".to_string()),
            examples: Some(vec![
                r#"{"action": "balance", "address": "..."}"#.to_string(),
                r#"{"action": "transfer", "address": "...", "amount": 0.1}"#.to_string(),
            ]),
        }
    }
}

#[async_trait]
impl ToolExecutor for WalletTool {
    async fn execute(&self, input: ToolInput) -> ToolResult {
        use drbot_skills::Skill;

        let skill_input = drbot_skills::SkillInput {
            params: input.to_params(),
            text: None,
            attachments: vec![],
        };

        // Create a minimal context
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
    fn test_wallet_tool_definition() {
        let rpc = Arc::new(RpcClient::new("https://api.devnet.solana.com".to_string()));
        let tool = WalletTool::new(rpc, None);
        let def = tool.definition();

        assert_eq!(def.name, "solana_wallet");
        assert!(def.parameters.iter().any(|p| p.name == "action"));
    }
}
