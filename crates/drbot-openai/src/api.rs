//! OpenAI API types.

use serde::{Deserialize, Serialize};

/// Role in a chat message.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

/// Chat message for the API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    /// Role of the message sender.
    pub role: Role,
    /// Content of the message.
    pub content: Option<String>,
    /// Tool calls (for assistant messages).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    /// Tool call ID (for tool messages).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl ChatMessage {
    /// Create a system message.
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: Role::System,
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    /// Create a user message.
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    /// Create an assistant message.
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: None,
        }
    }
}

/// Tool call in assistant response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    /// Tool call ID.
    pub id: String,
    /// Type of tool (always "function").
    #[serde(rename = "type")]
    pub tool_type: String,
    /// Function call details.
    pub function: FunctionCall,
}

/// Function call details.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    /// Function name.
    pub name: String,
    /// Arguments as JSON string.
    pub arguments: String,
}

/// Chat completion request.
#[derive(Debug, Clone, Serialize)]
pub struct ChatCompletionRequest {
    /// Model ID.
    pub model: String,
    /// Messages in the conversation.
    pub messages: Vec<ChatMessage>,
    /// Maximum tokens to generate.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    /// Temperature for sampling.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// Top-p sampling.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    /// Stop sequences.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<Vec<String>>,
    /// Whether to stream the response.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    /// Stream options (for streaming).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream_options: Option<StreamOptions>,
}

/// Stream options for chat completion.
#[derive(Debug, Clone, Serialize)]
pub struct StreamOptions {
    /// Include usage in stream.
    pub include_usage: bool,
}

/// Chat completion response.
#[derive(Debug, Clone, Deserialize)]
pub struct ChatCompletionResponse {
    /// Response ID.
    pub id: String,
    /// Object type.
    pub object: String,
    /// Creation timestamp.
    pub created: i64,
    /// Model used.
    pub model: String,
    /// Response choices.
    pub choices: Vec<Choice>,
    /// Token usage.
    pub usage: Option<Usage>,
}

/// Response choice.
#[derive(Debug, Clone, Deserialize)]
pub struct Choice {
    /// Choice index.
    pub index: usize,
    /// Generated message.
    pub message: Option<ChatMessage>,
    /// Delta for streaming.
    pub delta: Option<Delta>,
    /// Finish reason.
    pub finish_reason: Option<String>,
}

/// Delta content for streaming.
#[derive(Debug, Clone, Deserialize)]
pub struct Delta {
    /// Role (only in first delta).
    pub role: Option<Role>,
    /// Content chunk.
    pub content: Option<String>,
    /// Tool calls delta.
    pub tool_calls: Option<Vec<ToolCallDelta>>,
}

/// Tool call delta for streaming.
#[derive(Debug, Clone, Deserialize)]
pub struct ToolCallDelta {
    /// Index of the tool call.
    pub index: usize,
    /// Tool call ID (only in first delta).
    pub id: Option<String>,
    /// Type (only in first delta).
    #[serde(rename = "type")]
    pub tool_type: Option<String>,
    /// Function delta.
    pub function: Option<FunctionDelta>,
}

/// Function delta for streaming.
#[derive(Debug, Clone, Deserialize)]
pub struct FunctionDelta {
    /// Function name (only in first delta).
    pub name: Option<String>,
    /// Arguments chunk.
    pub arguments: Option<String>,
}

/// Token usage statistics.
#[derive(Debug, Clone, Deserialize)]
pub struct Usage {
    /// Prompt tokens.
    pub prompt_tokens: u32,
    /// Completion tokens.
    pub completion_tokens: u32,
    /// Total tokens.
    pub total_tokens: u32,
}

/// Streaming chunk response.
#[derive(Debug, Clone, Deserialize)]
pub struct ChatCompletionChunk {
    /// Chunk ID.
    pub id: String,
    /// Object type.
    pub object: String,
    /// Creation timestamp.
    pub created: i64,
    /// Model used.
    pub model: String,
    /// Choices in chunk.
    pub choices: Vec<Choice>,
    /// Usage (only in final chunk with stream_options).
    pub usage: Option<Usage>,
}

/// Error response from the API.
#[derive(Debug, Clone, Deserialize)]
pub struct ErrorResponse {
    /// Error details.
    pub error: ErrorDetails,
}

/// Error details.
#[derive(Debug, Clone, Deserialize)]
pub struct ErrorDetails {
    /// Error message.
    pub message: String,
    /// Error type.
    #[serde(rename = "type")]
    pub error_type: String,
    /// Parameter that caused the error.
    pub param: Option<String>,
    /// Error code.
    pub code: Option<String>,
}
