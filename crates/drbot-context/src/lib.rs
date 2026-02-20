//! Smart context management for drbot.
//!
//! Handles context window optimization, summarization, and memory management.

mod compressor;
mod manager;
mod summarizer;
mod window;

pub use compressor::{CompressionStrategy, ContextCompressor};
pub use manager::{ContextManager, ContextState};
pub use summarizer::{Summarizer, Summary};
pub use window::{ContextWindow, WindowConfig};

use serde::{Deserialize, Serialize};

/// Context result.
pub type Result<T> = std::result::Result<T, ContextError>;

/// Context errors.
#[derive(Debug, thiserror::Error)]
pub enum ContextError {
    #[error("Context overflow: {0}")]
    Overflow(String),
    #[error("Summarization failed: {0}")]
    SummarizationFailed(String),
    #[error("Invalid context: {0}")]
    InvalidContext(String),
    #[error("Token counting error: {0}")]
    TokenError(String),
}

/// Context configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextConfig {
    /// Maximum tokens in context window.
    pub max_tokens: usize,
    /// Reserved tokens for response.
    pub reserved_for_response: usize,
    /// When to start compression (percentage of max).
    pub compression_threshold: f32,
    /// Minimum messages to keep.
    pub min_messages: usize,
    /// Whether to auto-summarize old messages.
    pub auto_summarize: bool,
}

impl Default for ContextConfig {
    fn default() -> Self {
        Self {
            max_tokens: 100000,
            reserved_for_response: 4096,
            compression_threshold: 0.8,
            min_messages: 5,
            auto_summarize: true,
        }
    }
}

/// A context item that can be included in the context window.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextItem {
    /// Item type.
    pub item_type: ContextItemType,
    /// Content.
    pub content: String,
    /// Token count.
    pub tokens: usize,
    /// Priority (higher = more important to keep).
    pub priority: f32,
    /// Timestamp.
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Types of context items.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextItemType {
    SystemPrompt,
    UserMessage,
    AssistantMessage,
    ToolResult,
    Summary,
    KnowledgeChunk,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_config_default() {
        let config = ContextConfig::default();
        assert_eq!(config.max_tokens, 100000);
        assert!(config.auto_summarize);
    }
}
