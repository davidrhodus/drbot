//! Validator intelligence tool for agent use.

use super::compat::{
    Tool, ToolDefinition, ToolError, ToolExecutor, ToolInput, ToolOutput, ToolParameter, ToolResult,
};
use crate::skills::ValidatorIntelSkill;
use async_trait::async_trait;
use solana_client::nonblocking::rpc_client::RpcClient;
use std::sync::Arc;

/// Validator intelligence tool for agent use.
pub struct ValidatorIntelTool {
    skill: ValidatorIntelSkill,
}

impl ValidatorIntelTool {
    /// Create a new validator intel tool.
    pub fn new(rpc_client: Arc<RpcClient>) -> Self {
        Self {
            skill: ValidatorIntelSkill::new(rpc_client),
        }
    }
}

impl Tool for ValidatorIntelTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "solana_validator_intel".to_string(),
            description: "Fetch and rank Solana validators (stake, commission, delinquency, node contact info, optional skip-rate performance).".to_string(),
            parameters: vec![
                ToolParameter {
                    name: "action".to_string(),
                    description: "Action to perform".to_string(),
                    param_type: "string".to_string(),
                    required: true,
                    enum_values: Some(vec![
                        "validators".to_string(),
                        "overview".to_string(),
                        "blocks_compare".to_string(),
                        "sfdp_overview".to_string(),
                        "list".to_string(),
                        "get".to_string(),
                        "top".to_string(),
                        "summary".to_string(),
                    ]),
                    default: None,
                },
                ToolParameter {
                    name: "validator".to_string(),
                    description: "Validator identity/vote pubkey, or alias for 'get' (e.g., 'jito')".to_string(),
                    param_type: "string".to_string(),
                    required: false,
                    enum_values: None,
                    default: None,
                },
                ToolParameter {
                    name: "client_type".to_string(),
                    description: "Filter for 'validators' action (e.g., 'Harmonix', 'Jito Classic')".to_string(),
                    param_type: "string".to_string(),
                    required: false,
                    enum_values: None,
                    default: None,
                },
                ToolParameter {
                    name: "include_delinquent".to_string(),
                    description: "Include delinquent validators (default: false)".to_string(),
                    param_type: "boolean".to_string(),
                    required: false,
                    enum_values: None,
                    default: Some(serde_json::json!(false)),
                },
                ToolParameter {
                    name: "with_performance".to_string(),
                    description: "Fetch block production and compute skip rate (default: false)".to_string(),
                    param_type: "boolean".to_string(),
                    required: false,
                    enum_values: None,
                    default: Some(serde_json::json!(false)),
                },
                ToolParameter {
                    name: "max_commission".to_string(),
                    description: "Filter: maximum commission percent".to_string(),
                    param_type: "number".to_string(),
                    required: false,
                    enum_values: None,
                    default: None,
                },
                ToolParameter {
                    name: "min_stake_sol".to_string(),
                    description: "Filter: minimum activated stake in SOL".to_string(),
                    param_type: "number".to_string(),
                    required: false,
                    enum_values: None,
                    default: None,
                },
                ToolParameter {
                    name: "sort".to_string(),
                    description: "Sort: stake_desc, commission_asc, score_desc, skip_rate_asc".to_string(),
                    param_type: "string".to_string(),
                    required: false,
                    enum_values: Some(vec![
                        "stake_desc".to_string(),
                        "commission_asc".to_string(),
                        "score_desc".to_string(),
                        "skip_rate_asc".to_string(),
                    ]),
                    default: Some(serde_json::json!("stake_desc")),
                },
                ToolParameter {
                    name: "limit".to_string(),
                    description: "Limit number of results".to_string(),
                    param_type: "number".to_string(),
                    required: false,
                    enum_values: None,
                    default: None,
                },
                ToolParameter {
                    name: "offset".to_string(),
                    description: "Offset for pagination (validators action)".to_string(),
                    param_type: "number".to_string(),
                    required: false,
                    enum_values: None,
                    default: None,
                },
            ],
            category: Some("monitoring".to_string()),
            examples: Some(vec![
                r#"{"action":"overview"}"#.to_string(),
                r#"{"action":"validators","client_type":"Harmonix","limit":50,"offset":0}"#.to_string(),
                r#"{"action":"blocks_compare"}"#.to_string(),
                r#"{"action":"sfdp_overview"}"#.to_string(),
                r#"{"action":"summary"}"#.to_string(),
                r#"{"action":"top","limit":10,"max_commission":10}"#.to_string(),
                r#"{"action":"get","validator":"jito","with_performance":true}"#.to_string(),
                r#"{"action":"list","sort":"skip_rate_asc","with_performance":true,"limit":25}"#.to_string(),
            ]),
        }
    }
}

#[async_trait]
impl ToolExecutor for ValidatorIntelTool {
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
    fn test_validator_intel_tool_definition() {
        let rpc = Arc::new(RpcClient::new("https://api.devnet.solana.com".to_string()));
        let tool = ValidatorIntelTool::new(rpc);
        let def = tool.definition();

        assert_eq!(def.name, "solana_validator_intel");
        assert!(def.parameters.iter().any(|p| p.name == "action"));
    }
}
