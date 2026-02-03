//! AWS Bedrock API types.
//!
//! These types match the Bedrock Converse API format.

use serde::{Deserialize, Serialize};

/// Converse request body.
#[derive(Debug, Clone, Serialize)]
pub struct ConverseRequest {
    /// Model identifier.
    #[serde(rename = "modelId")]
    pub model_id: String,
    /// Messages in the conversation.
    pub messages: Vec<BedrockMessage>,
    /// System prompts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<Vec<SystemContent>>,
    /// Inference configuration.
    #[serde(rename = "inferenceConfig", skip_serializing_if = "Option::is_none")]
    pub inference_config: Option<InferenceConfig>,
}

/// System content block.
#[derive(Debug, Clone, Serialize)]
pub struct SystemContent {
    /// Text content.
    pub text: String,
}

/// Bedrock message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BedrockMessage {
    /// Role (user or assistant).
    pub role: String,
    /// Content blocks.
    pub content: Vec<ContentBlock>,
}

impl BedrockMessage {
    /// Create a user message.
    pub fn user(text: impl Into<String>) -> Self {
        Self {
            role: "user".to_string(),
            content: vec![ContentBlock::Text { text: text.into() }],
        }
    }

    /// Create an assistant message.
    pub fn assistant(text: impl Into<String>) -> Self {
        Self {
            role: "assistant".to_string(),
            content: vec![ContentBlock::Text { text: text.into() }],
        }
    }
}

/// Content block.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ContentBlock {
    /// Text content.
    Text {
        /// Text string.
        text: String,
    },
}

impl ContentBlock {
    /// Get text content if this is a text block.
    pub fn as_text(&self) -> Option<&str> {
        match self {
            ContentBlock::Text { text } => Some(text),
        }
    }
}

/// Inference configuration.
#[derive(Debug, Clone, Serialize, Default)]
pub struct InferenceConfig {
    /// Maximum tokens to generate.
    #[serde(rename = "maxTokens", skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    /// Temperature for sampling.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// Top-p sampling.
    #[serde(rename = "topP", skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    /// Stop sequences.
    #[serde(rename = "stopSequences", skip_serializing_if = "Option::is_none")]
    pub stop_sequences: Option<Vec<String>>,
}

/// Converse response.
#[derive(Debug, Clone, Deserialize)]
pub struct ConverseResponse {
    /// Output content.
    pub output: ConverseOutput,
    /// Stop reason.
    #[serde(rename = "stopReason")]
    pub stop_reason: Option<String>,
    /// Usage information.
    pub usage: Option<BedrockUsage>,
}

/// Converse output.
#[derive(Debug, Clone, Deserialize)]
pub struct ConverseOutput {
    /// Message content.
    pub message: Option<BedrockMessage>,
}

/// Usage information.
#[derive(Debug, Clone, Deserialize)]
pub struct BedrockUsage {
    /// Input tokens.
    #[serde(rename = "inputTokens")]
    pub input_tokens: u32,
    /// Output tokens.
    #[serde(rename = "outputTokens")]
    pub output_tokens: u32,
}

/// Converse stream event.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ConverseStreamEvent {
    /// Message start.
    MessageStart {
        /// Role.
        role: String,
    },
    /// Content block start.
    ContentBlockStart {
        /// Content block index.
        content_block_index: u32,
        /// Start info.
        start: ContentBlockStartInfo,
    },
    /// Content block delta.
    ContentBlockDelta {
        /// Content block index.
        content_block_index: u32,
        /// Delta content.
        delta: ContentBlockDelta,
    },
    /// Content block stop.
    ContentBlockStop {
        /// Content block index.
        content_block_index: u32,
    },
    /// Message stop.
    MessageStop {
        /// Stop reason.
        stop_reason: String,
    },
    /// Metadata.
    Metadata {
        /// Usage.
        usage: Option<BedrockUsage>,
    },
}

/// Content block start info.
#[derive(Debug, Clone, Deserialize)]
pub struct ContentBlockStartInfo {
    /// Text (for text blocks).
    pub text: Option<String>,
}

/// Content block delta.
#[derive(Debug, Clone, Deserialize)]
pub struct ContentBlockDelta {
    /// Text delta.
    pub text: Option<String>,
}

/// AWS error response.
#[derive(Debug, Clone, Deserialize)]
pub struct AwsError {
    /// Error message.
    pub message: Option<String>,
    /// Error type.
    #[serde(rename = "__type")]
    pub error_type: Option<String>,
}
