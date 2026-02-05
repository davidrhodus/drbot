//! Agent framework for drbot.
//!
//! Provides autonomous agents that can execute multi-step tasks,
//! use tools, and make decisions.
//!
//! # Features
//!
//! - Autonomous task execution with tool use
//! - Planning and step-by-step execution
//! - Background agent execution with checkpoints
//! - Human-in-the-loop approval system
//!
//! # Example
//!
//! ```rust,no_run
//! use drbot_agents::{BackgroundRunner, BackgroundConfig, AgentConfig};
//!
//! async fn example(provider: std::sync::Arc<dyn drbot_providers::Provider>) {
//!     let runner = BackgroundRunner::new(
//!         provider,
//!         BackgroundConfig::default(),
//!         AgentConfig::default(),
//!     );
//!
//!     // Start a background agent
//!     let agent_id = runner.start("Research the latest AI news").await.unwrap();
//!
//!     // Subscribe to events
//!     let mut events = runner.subscribe();
//! }
//! ```

mod agent;
mod background;
mod executor;
mod planner;
mod sandbox;
mod tool_root;
mod tools;
mod unified_diff;

pub use agent::{Agent, AgentConfig, AgentState};
pub use background::{
    BackgroundAgentState, BackgroundConfig, BackgroundEvent, BackgroundRunner, BackgroundStatus,
    BackgroundStorage, Checkpoint, CheckpointAction, MemoryBackgroundStorage,
};
pub use executor::{ExecutionResult, Executor};
pub use planner::{Plan, Planner, Step};
pub use sandbox::{Sandbox, SandboxConfig};
pub use tools::{AgentTool, BuiltinTools};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Agent execution result.
pub type Result<T> = std::result::Result<T, AgentError>;

/// Agent errors.
#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    #[error("Planning failed: {0}")]
    PlanningFailed(String),
    #[error("Execution failed: {0}")]
    ExecutionFailed(String),
    #[error("Tool not found: {0}")]
    ToolNotFound(String),
    #[error("Tool execution failed: {0}")]
    ToolError(String),
    #[error("Max iterations exceeded")]
    MaxIterationsExceeded,
    #[error("Timeout")]
    Timeout,
    #[error("Sandbox error: {0}")]
    SandboxError(String),
}

/// A message in the agent's conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMessage {
    pub role: AgentRole,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_result: Option<ToolResult>,
}

/// Role in agent conversation.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentRole {
    System,
    User,
    Assistant,
    Tool,
}

/// A tool call made by the agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

/// Result of a tool execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub tool_call_id: String,
    pub content: String,
    pub is_error: bool,
}

/// Agent event for streaming updates.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentEvent {
    /// Agent started thinking.
    ThinkingStart,
    /// Agent produced some thought.
    Thought { content: String },
    /// Agent is calling a tool.
    ToolCall {
        tool: String,
        args: serde_json::Value,
    },
    /// Tool returned a result.
    ToolResult {
        tool: String,
        result: String,
        is_error: bool,
    },
    /// Agent produced final output.
    Output { content: String },
    /// Agent finished.
    Complete { iterations: usize },
    /// Error occurred.
    Error { message: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_message_serialization() {
        let msg = AgentMessage {
            role: AgentRole::Assistant,
            content: "Hello".to_string(),
            tool_calls: None,
            tool_result: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("assistant"));
    }
}
