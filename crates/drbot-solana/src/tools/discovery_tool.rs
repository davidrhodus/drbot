//! Discovery tool for agent use.

use super::compat::{
    Tool, ToolDefinition, ToolError, ToolExecutor, ToolInput, ToolOutput, ToolParameter, ToolResult,
};
use crate::discovery::{DexScreenerClient, GeckoTerminalClient};
use crate::skills::DiscoverySkill;
use async_trait::async_trait;

/// Discovery tool for finding trading opportunities.
pub struct DiscoveryTool {
    skill: DiscoverySkill,
}

impl DiscoveryTool {
    /// Create a new discovery tool.
    pub fn new(dexscreener: DexScreenerClient, geckoterminal: GeckoTerminalClient) -> Self {
        Self {
            skill: DiscoverySkill::new(dexscreener, geckoterminal),
        }
    }
}

impl Tool for DiscoveryTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "solana_discover".to_string(),
            description: "Find trading opportunities on Solana using DexScreener and GeckoTerminal. Search for tokens, find trending/new tokens, and filter by liquidity, volume, and age.".to_string(),
            parameters: vec![
                ToolParameter {
                    name: "source".to_string(),
                    description: "Data source: 'dexscreener', 'geckoterminal', or 'both' (default)".to_string(),
                    param_type: "string".to_string(),
                    required: false,
                    enum_values: Some(vec![
                        "dexscreener".to_string(),
                        "geckoterminal".to_string(),
                        "both".to_string(),
                    ]),
                    default: Some(serde_json::json!("both")),
                },
                ToolParameter {
                    name: "filter".to_string(),
                    description: "Filter preset: 'new_tokens' (< 24h old), 'established' (> 1 week, high liquidity), 'high_momentum' (> 10% gain), or 'custom'".to_string(),
                    param_type: "string".to_string(),
                    required: false,
                    enum_values: Some(vec![
                        "new_tokens".to_string(),
                        "established".to_string(),
                        "high_momentum".to_string(),
                        "custom".to_string(),
                    ]),
                    default: Some(serde_json::json!("new_tokens")),
                },
                ToolParameter {
                    name: "query".to_string(),
                    description: "Search query for finding specific tokens by name or symbol".to_string(),
                    param_type: "string".to_string(),
                    required: false,
                    enum_values: None,
                    default: None,
                },
                ToolParameter {
                    name: "min_liquidity_usd".to_string(),
                    description: "Minimum liquidity in USD".to_string(),
                    param_type: "number".to_string(),
                    required: false,
                    enum_values: None,
                    default: None,
                },
                ToolParameter {
                    name: "min_volume_24h".to_string(),
                    description: "Minimum 24h trading volume in USD".to_string(),
                    param_type: "number".to_string(),
                    required: false,
                    enum_values: None,
                    default: None,
                },
                ToolParameter {
                    name: "max_age_hours".to_string(),
                    description: "Maximum token/pool age in hours".to_string(),
                    param_type: "number".to_string(),
                    required: false,
                    enum_values: None,
                    default: None,
                },
                ToolParameter {
                    name: "limit".to_string(),
                    description: "Maximum number of results to return".to_string(),
                    param_type: "number".to_string(),
                    required: false,
                    enum_values: None,
                    default: Some(serde_json::json!(20)),
                },
            ],
            category: Some("research".to_string()),
            examples: Some(vec![
                r#"{"source": "both", "filter": "new_tokens"}"#.to_string(),
                r#"{"query": "bonk", "min_liquidity_usd": 10000}"#.to_string(),
                r#"{"filter": "high_momentum", "min_volume_24h": 50000, "limit": 10}"#.to_string(),
            ]),
        }
    }
}

#[async_trait]
impl ToolExecutor for DiscoveryTool {
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
    fn test_discovery_tool_definition() {
        let dexscreener = DexScreenerClient::new("https://api.dexscreener.com".to_string());
        let geckoterminal =
            GeckoTerminalClient::new("https://api.geckoterminal.com/api/v2".to_string());
        let tool = DiscoveryTool::new(dexscreener, geckoterminal);
        let def = tool.definition();

        assert_eq!(def.name, "solana_discover");
        assert!(def.parameters.iter().any(|p| p.name == "source"));
        assert!(def.parameters.iter().any(|p| p.name == "filter"));
    }
}
