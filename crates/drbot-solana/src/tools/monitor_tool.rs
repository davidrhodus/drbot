//! Smart contract monitoring tool for agent use.

use super::compat::{
    Tool, ToolDefinition, ToolError, ToolExecutor, ToolInput, ToolOutput, ToolParameter, ToolResult,
};
use crate::skills::MonitorSkill;
use async_trait::async_trait;
use solana_client::nonblocking::rpc_client::RpcClient;
use std::sync::Arc;

/// Smart contract monitoring tool for agent use.
pub struct MonitorTool {
    skill: MonitorSkill,
}

impl MonitorTool {
    /// Create a new monitor tool.
    pub fn new(rpc_client: Arc<RpcClient>) -> Self {
        Self {
            skill: MonitorSkill::new(rpc_client),
        }
    }
}

impl Tool for MonitorTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "solana_monitor".to_string(),
            description: "Monitor smart contract upgrades and detect suspicious changes"
                .to_string(),
            parameters: vec![
                ToolParameter {
                    name: "action".to_string(),
                    description: "Action to perform".to_string(),
                    param_type: "string".to_string(),
                    required: true,
                    enum_values: Some(vec![
                        "watch".to_string(),
                        "unwatch".to_string(),
                        "list".to_string(),
                        "check".to_string(),
                        "events".to_string(),
                        "analyze".to_string(),
                    ]),
                    default: None,
                },
                ToolParameter {
                    name: "program_id".to_string(),
                    description: "Program address to watch/check".to_string(),
                    param_type: "string".to_string(),
                    required: false,
                    enum_values: None,
                    default: None,
                },
                ToolParameter {
                    name: "name".to_string(),
                    description: "Human-readable name for the program".to_string(),
                    param_type: "string".to_string(),
                    required: false,
                    enum_values: None,
                    default: None,
                },
                ToolParameter {
                    name: "limit".to_string(),
                    description: "Maximum events to return".to_string(),
                    param_type: "number".to_string(),
                    required: false,
                    enum_values: None,
                    default: None,
                },
            ],
            category: Some("security".to_string()),
            examples: Some(vec![
                r#"{"action": "list"}"#.to_string(),
                r#"{"action": "watch", "program_id": "...", "name": "MyProtocol"}"#.to_string(),
                r#"{"action": "events", "limit": 10}"#.to_string(),
            ]),
        }
    }
}

#[async_trait]
impl ToolExecutor for MonitorTool {
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
    fn test_monitor_tool_definition() {
        let rpc = Arc::new(RpcClient::new("https://api.devnet.solana.com".to_string()));
        let tool = MonitorTool::new(rpc);
        let def = tool.definition();

        assert_eq!(def.name, "solana_monitor");
    }
}
