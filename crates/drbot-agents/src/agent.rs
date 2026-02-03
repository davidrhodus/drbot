//! Core agent implementation.

use crate::executor::Executor;
use crate::planner::Planner;
use crate::tools::AgentTool;
use crate::{AgentError, AgentEvent, AgentMessage, AgentRole, Result, ToolCall, ToolResult};
use drbot_providers::Provider;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};
use uuid::Uuid;

/// Agent configuration.
#[derive(Debug, Clone)]
pub struct AgentConfig {
    /// Maximum iterations before stopping.
    pub max_iterations: usize,
    /// System prompt for the agent.
    pub system_prompt: String,
    /// Whether to use planning.
    pub use_planning: bool,
    /// Timeout per iteration in seconds.
    pub iteration_timeout_secs: u64,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            max_iterations: 10,
            system_prompt:
                "You are a helpful AI assistant that can use tools to accomplish tasks. \
                           Think step by step and use tools when needed."
                    .to_string(),
            use_planning: true,
            iteration_timeout_secs: 60,
        }
    }
}

/// Agent state.
#[derive(Debug, Clone)]
pub enum AgentState {
    Idle,
    Thinking,
    ExecutingTool(String),
    Complete,
    Failed(String),
}

/// An autonomous agent.
pub struct Agent {
    id: Uuid,
    config: AgentConfig,
    provider: Arc<dyn Provider>,
    tools: HashMap<String, Arc<dyn AgentTool>>,
    messages: Vec<AgentMessage>,
    state: AgentState,
}

impl Agent {
    /// Create a new agent.
    pub fn new(provider: Arc<dyn Provider>, config: AgentConfig) -> Self {
        let mut agent = Self {
            id: Uuid::new_v4(),
            config,
            provider,
            tools: HashMap::new(),
            messages: Vec::new(),
            state: AgentState::Idle,
        };

        // Add system message
        agent.messages.push(AgentMessage {
            role: AgentRole::System,
            content: agent.config.system_prompt.clone(),
            tool_calls: None,
            tool_result: None,
        });

        agent
    }

    /// Register a tool.
    pub fn register_tool(&mut self, tool: Arc<dyn AgentTool>) {
        self.tools.insert(tool.name().to_string(), tool);
    }

    /// Append a message to the agent's conversation history.
    ///
    /// Useful for seeding context (e.g. loading a prior session transcript).
    pub fn push_message(&mut self, message: AgentMessage) {
        self.messages.push(message);
    }

    /// Get agent ID.
    pub fn id(&self) -> Uuid {
        self.id
    }

    /// Get current state.
    pub fn state(&self) -> &AgentState {
        &self.state
    }

    /// Run the agent with a task.
    pub async fn run(&mut self, task: &str) -> Result<String> {
        let (tx, _rx) = mpsc::channel(100);
        self.run_with_events(task, tx).await
    }

    /// Run the agent with event streaming.
    pub async fn run_with_events(
        &mut self,
        task: &str,
        events: mpsc::Sender<AgentEvent>,
    ) -> Result<String> {
        info!(agent_id = %self.id, "Starting agent with task");

        // Add user task
        self.messages.push(AgentMessage {
            role: AgentRole::User,
            content: task.to_string(),
            tool_calls: None,
            tool_result: None,
        });

        let mut iterations = 0;
        let mut final_output = String::new();

        while iterations < self.config.max_iterations {
            iterations += 1;
            debug!(iteration = iterations, "Agent iteration");

            self.state = AgentState::Thinking;
            let _ = events.send(AgentEvent::ThinkingStart).await;

            // Call the LLM
            let response = self.call_llm().await?;

            // Check for tool calls
            if let Some(tool_calls) = &response.tool_calls {
                for tool_call in tool_calls {
                    self.state = AgentState::ExecutingTool(tool_call.name.clone());
                    let _ = events
                        .send(AgentEvent::ToolCall {
                            tool: tool_call.name.clone(),
                            args: tool_call.arguments.clone(),
                        })
                        .await;

                    // Execute tool
                    let result = self.execute_tool(tool_call).await;

                    let _ = events
                        .send(AgentEvent::ToolResult {
                            tool: tool_call.name.clone(),
                            result: result.content.clone(),
                            is_error: result.is_error,
                        })
                        .await;

                    // Add tool result to messages
                    self.messages.push(AgentMessage {
                        role: AgentRole::Tool,
                        content: result.content.clone(),
                        tool_calls: None,
                        tool_result: Some(result),
                    });
                }

                // Add assistant message with tool calls
                self.messages.push(response);
            } else {
                // No tool calls - agent is done
                final_output = response.content.clone();
                self.messages.push(response);

                let _ = events
                    .send(AgentEvent::Output {
                        content: final_output.clone(),
                    })
                    .await;

                break;
            }
        }

        if iterations >= self.config.max_iterations {
            self.state = AgentState::Failed("Max iterations exceeded".to_string());
            return Err(AgentError::MaxIterationsExceeded);
        }

        self.state = AgentState::Complete;
        let _ = events.send(AgentEvent::Complete { iterations }).await;

        Ok(final_output)
    }

    /// Call the LLM.
    async fn call_llm(&self) -> Result<AgentMessage> {
        use drbot_core::message::Message;
        use drbot_providers::ChatOptions;

        // Convert agent messages to provider messages
        let messages: Vec<Message> = self
            .messages
            .iter()
            .filter_map(|m| match m.role {
                AgentRole::System => Some(Message::system(&m.content)),
                AgentRole::User => Some(Message::user(&m.content)),
                AgentRole::Assistant => Some(Message::assistant(&m.content)),
                AgentRole::Tool => Some(Message::user(&format!("Tool result: {}", m.content))),
            })
            .collect();

        // Build tool definitions for the prompt
        let tools_desc = self.get_tools_description();
        let system_with_tools = format!(
            "{}\n\nYou have access to the following tools:\n{}\n\n\
             To use a tool, respond with a JSON object like: {{\"tool\": \"tool_name\", \"args\": {{...}}}}\n\
             OpenClaw-style actions are also accepted: {{\"action\": \"tool_name\", \"params\": {{...}}}} (or {{\"action\": \"tool_name\", ...}})\n\
             When you have the final answer, respond normally without using tools.",
            self.config.system_prompt,
            tools_desc
        );

        let options = ChatOptions {
            model: None,
            max_tokens: Some(4096),
            temperature: Some(0.7),
            top_p: None,
            stop_sequences: None,
            system_prompt: Some(system_with_tools),
        };

        let response = self
            .provider
            .chat(&messages, options)
            .await
            .map_err(|e| AgentError::ExecutionFailed(e.to_string()))?;

        // Parse response for tool calls
        let tool_calls = self.parse_tool_calls(&response.content);

        Ok(AgentMessage {
            role: AgentRole::Assistant,
            content: response.content,
            tool_calls,
            tool_result: None,
        })
    }

    /// Get tools description for prompt.
    fn get_tools_description(&self) -> String {
        self.tools
            .values()
            .map(|t| {
                format!(
                    "- {}: {}\n  Arguments: {}",
                    t.name(),
                    t.description(),
                    t.parameters()
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Parse tool calls from response.
    fn parse_tool_calls(&self, content: &str) -> Option<Vec<ToolCall>> {
        // Extract the first JSON object in the response that looks like:
        // {"tool":"...", "args":{...}}
        //
        // We intentionally support whitespace and nested objects by scanning for
        // balanced braces rather than naively taking the first '}'.
        fn extract_json_object_bounds(s: &str, start: usize) -> Option<(usize, usize)> {
            let slice = s.get(start..)?;
            let mut depth: i64 = 0;
            let mut in_string = false;
            let mut escape = false;

            for (off, ch) in slice.char_indices() {
                if in_string {
                    if escape {
                        escape = false;
                        continue;
                    }
                    match ch {
                        '\\' => {
                            escape = true;
                        }
                        '"' => {
                            in_string = false;
                        }
                        _ => {}
                    }
                    continue;
                }

                match ch {
                    '"' => in_string = true,
                    '{' => depth += 1,
                    '}' => {
                        depth -= 1;
                        if depth == 0 {
                            return Some((start, start + off + ch.len_utf8()));
                        }
                    }
                    _ => {}
                }
            }

            None
        }

        fn parse_tool_call(value: &serde_json::Value) -> Option<ToolCall> {
            let tool = value
                .get("tool")
                .and_then(|v| v.as_str())
                .or_else(|| value.get("action").and_then(|v| v.as_str()))?
                .trim();
            if tool.is_empty() {
                return None;
            }
            // Accept either "args" or "arguments" (drbot) or "params" (OpenClaw actions).
            let args = value
                .get("args")
                .or_else(|| value.get("arguments"))
                .or_else(|| value.get("params"))
                .cloned()
                .unwrap_or_else(|| {
                    // Also accept `{ "action": "...", ... }` where the remaining
                    // keys are treated as args.
                    if let Some(obj) = value.as_object() {
                        let mut out = serde_json::Map::new();
                        for (k, v) in obj {
                            if matches!(
                                k.as_str(),
                                "tool" | "args" | "arguments" | "action" | "params"
                            ) {
                                continue;
                            }
                            out.insert(k.clone(), v.clone());
                        }
                        serde_json::Value::Object(out)
                    } else {
                        serde_json::Value::Object(serde_json::Map::new())
                    }
                });
            Some(ToolCall {
                id: Uuid::new_v4().to_string(),
                name: tool.to_string(),
                arguments: args,
            })
        }

        let bytes = content.as_bytes();
        let mut i = 0usize;
        while i < bytes.len() {
            if bytes[i] != b'{' {
                i += 1;
                continue;
            }

            let Some((start, end)) = extract_json_object_bounds(content, i) else {
                i += 1;
                continue;
            };
            let json_str = &content[start..end];
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(json_str) {
                if let Some(call) = parse_tool_call(&value) {
                    return Some(vec![call]);
                }
                if let Some(arr) = value.as_array() {
                    for item in arr {
                        if let Some(call) = parse_tool_call(item) {
                            return Some(vec![call]);
                        }
                    }
                }
            }

            i = end;
        }

        None
    }

    /// Execute a tool.
    async fn execute_tool(&self, call: &ToolCall) -> ToolResult {
        match self.tools.get(&call.name) {
            Some(tool) => match tool.execute(call.arguments.clone()).await {
                Ok(result) => ToolResult {
                    tool_call_id: call.id.clone(),
                    content: result,
                    is_error: false,
                },
                Err(e) => ToolResult {
                    tool_call_id: call.id.clone(),
                    content: format!("Error: {}", e),
                    is_error: true,
                },
            },
            None => ToolResult {
                tool_call_id: call.id.clone(),
                content: format!("Tool not found: {}", call.name),
                is_error: true,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_config_default() {
        let config = AgentConfig::default();
        assert_eq!(config.max_iterations, 10);
        assert!(config.use_planning);
    }
}
