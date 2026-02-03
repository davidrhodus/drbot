//! Ollama API types.

use serde::{Deserialize, Serialize};

/// Chat message for Ollama API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    /// Role: system, user, or assistant.
    pub role: String,
    /// Message content.
    pub content: String,
}

impl ChatMessage {
    /// Create a system message.
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".to_string(),
            content: content.into(),
        }
    }

    /// Create a user message.
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".to_string(),
            content: content.into(),
        }
    }

    /// Create an assistant message.
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: "assistant".to_string(),
            content: content.into(),
        }
    }
}

/// Chat request to Ollama API.
#[derive(Debug, Clone, Serialize)]
pub struct ChatRequest {
    /// Model name.
    pub model: String,
    /// Messages in the conversation.
    pub messages: Vec<ChatMessage>,
    /// Whether to stream the response.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    /// Generation options.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<GenerationOptions>,
}

/// Generation options for Ollama.
#[derive(Debug, Clone, Serialize, Default)]
pub struct GenerationOptions {
    /// Temperature for sampling.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// Top-p sampling.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    /// Maximum tokens to generate.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub num_predict: Option<usize>,
    /// Stop sequences.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<Vec<String>>,
}

/// Chat response from Ollama API (non-streaming).
#[derive(Debug, Clone, Deserialize)]
pub struct ChatResponse {
    /// Model used.
    pub model: String,
    /// Generated message.
    pub message: ChatMessage,
    /// Whether generation is done.
    pub done: bool,
    /// Total duration in nanoseconds.
    pub total_duration: Option<u64>,
    /// Prompt evaluation count.
    pub prompt_eval_count: Option<u32>,
    /// Evaluation count (output tokens).
    pub eval_count: Option<u32>,
}

/// Streaming chat response chunk.
#[derive(Debug, Clone, Deserialize)]
pub struct ChatStreamChunk {
    /// Model used.
    pub model: String,
    /// Message chunk.
    pub message: ChatMessage,
    /// Whether generation is done.
    pub done: bool,
    /// Total duration (only in final chunk).
    pub total_duration: Option<u64>,
    /// Prompt evaluation count (only in final chunk).
    pub prompt_eval_count: Option<u32>,
    /// Evaluation count (only in final chunk).
    pub eval_count: Option<u32>,
}

/// Response from /api/tags endpoint.
#[derive(Debug, Clone, Deserialize)]
pub struct TagsResponse {
    /// Available models.
    pub models: Vec<ModelInfo>,
}

/// Model information from Ollama.
#[derive(Debug, Clone, Deserialize)]
pub struct ModelInfo {
    /// Model name.
    pub name: String,
    /// Model size in bytes.
    pub size: u64,
    /// Model digest.
    pub digest: String,
    /// Modified timestamp.
    pub modified_at: String,
}

/// Error response from Ollama.
#[derive(Debug, Clone, Deserialize)]
pub struct ErrorResponse {
    /// Error message.
    pub error: String,
}
