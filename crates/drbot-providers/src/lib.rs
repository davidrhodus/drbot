//! AI provider trait abstraction for drbot.
//!
//! This crate defines the `Provider` trait that all AI backends must implement.

pub mod cli;
pub use cli::CliProvider;

use async_trait::async_trait;
use drbot_core::message::Message;
use drbot_core::Result;
use serde::{Deserialize, Serialize};
use std::pin::Pin;
use tokio_stream::Stream;

/// Tool definition for providers that support function/tool calling.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    /// JSON Schema for the tool input.
    pub parameters: serde_json::Value,
}

/// A tool-use request returned by the model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolUse {
    pub id: String,
    pub name: String,
    pub input: serde_json::Value,
}

/// Information about an AI model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub context_window: usize,
    pub max_output_tokens: Option<usize>,
}

/// Options for chat completion requests.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChatOptions {
    pub model: Option<String>,
    pub max_tokens: Option<usize>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub stop_sequences: Option<Vec<String>>,
    pub system_prompt: Option<String>,
    /// Optional tool definitions to enable native tool calling (provider-dependent).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ToolDefinition>>,
}

/// Response from a chat completion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    pub content: String,
    pub model: String,
    pub usage: Option<Usage>,
    pub stop_reason: Option<String>,
    /// Native tool calls requested by the model (if supported).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_uses: Vec<ToolUse>,
}

/// Token usage information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    pub input_tokens: usize,
    pub output_tokens: usize,
}

/// Events emitted during streaming.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StreamEvent {
    /// Stream started with model info.
    Start { model: String },
    /// Text content delta.
    Delta { content: String },
    /// Tool use request from the model.
    ToolUse {
        /// Tool use ID.
        id: String,
        /// Tool name.
        name: String,
        /// Tool input as JSON.
        input: serde_json::Value,
    },
    /// Stream stopped.
    Stop {
        reason: String,
        usage: Option<Usage>,
    },
    /// Error occurred.
    Error { message: String },
}

/// Trait for AI providers.
#[async_trait]
pub trait Provider: Send + Sync {
    /// Send a chat completion request.
    async fn chat(&self, messages: &[Message], options: ChatOptions) -> Result<ChatResponse>;

    /// Stream a chat completion response.
    async fn stream(
        &self,
        messages: &[Message],
        options: ChatOptions,
    ) -> Result<Pin<Box<dyn Stream<Item = StreamEvent> + Send>>>;

    /// Get the list of available models.
    fn models(&self) -> Vec<ModelInfo>;

    /// Get the provider name.
    fn name(&self) -> &str;
}
