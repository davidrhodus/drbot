//! Context manager for intelligent context handling.

use crate::compressor::ContextCompressor;
use crate::summarizer::Summarizer;
use crate::window::ContextWindow;
use crate::{ContextConfig, ContextError, ContextItem, ContextItemType, Result};
use drbot_core::message::Message;
use std::collections::VecDeque;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Current state of the context.
#[derive(Debug, Clone)]
pub struct ContextState {
    /// Total tokens used.
    pub total_tokens: usize,
    /// Available tokens.
    pub available_tokens: usize,
    /// Number of messages.
    pub message_count: usize,
    /// Whether compression is needed.
    pub needs_compression: bool,
    /// Compression ratio if compressed.
    pub compression_ratio: Option<f32>,
}

/// Context manager that handles intelligent context window management.
pub struct ContextManager {
    config: ContextConfig,
    items: VecDeque<ContextItem>,
    summarizer: Option<Arc<Summarizer>>,
    compressor: ContextCompressor,
    total_tokens: usize,
}

impl ContextManager {
    /// Create a new context manager.
    pub fn new(config: ContextConfig) -> Self {
        Self {
            config,
            items: VecDeque::new(),
            summarizer: None,
            compressor: ContextCompressor::new(),
            total_tokens: 0,
        }
    }

    /// Set the summarizer.
    pub fn with_summarizer(mut self, summarizer: Arc<Summarizer>) -> Self {
        self.summarizer = Some(summarizer);
        self
    }

    /// Add a message to the context.
    pub fn add_message(&mut self, message: &Message) -> Result<()> {
        let content = message.text_content();
        let tokens = count_tokens(&content);

        let item_type = match message.role {
            drbot_core::message::Role::User => ContextItemType::UserMessage,
            drbot_core::message::Role::Assistant => ContextItemType::AssistantMessage,
            drbot_core::message::Role::System => ContextItemType::SystemPrompt,
        };

        let item = ContextItem {
            item_type,
            content,
            tokens,
            priority: self.calculate_priority(item_type),
            timestamp: chrono::Utc::now(),
        };

        self.items.push_back(item);
        self.total_tokens += tokens;

        // Check if we need to compress
        if self.needs_compression() {
            self.compress()?;
        }

        Ok(())
    }

    /// Add a knowledge chunk to context.
    pub fn add_knowledge(&mut self, content: &str, priority: f32) -> Result<()> {
        let tokens = count_tokens(content);

        let item = ContextItem {
            item_type: ContextItemType::KnowledgeChunk,
            content: content.to_string(),
            tokens,
            priority,
            timestamp: chrono::Utc::now(),
        };

        self.items.push_back(item);
        self.total_tokens += tokens;

        if self.needs_compression() {
            self.compress()?;
        }

        Ok(())
    }

    /// Get current context state.
    pub fn state(&self) -> ContextState {
        let available = self
            .config
            .max_tokens
            .saturating_sub(self.total_tokens)
            .saturating_sub(self.config.reserved_for_response);

        ContextState {
            total_tokens: self.total_tokens,
            available_tokens: available,
            message_count: self.items.len(),
            needs_compression: self.needs_compression(),
            compression_ratio: None,
        }
    }

    /// Check if compression is needed.
    fn needs_compression(&self) -> bool {
        let threshold =
            (self.config.max_tokens as f32 * self.config.compression_threshold) as usize;
        self.total_tokens > threshold
    }

    /// Compress the context.
    fn compress(&mut self) -> Result<()> {
        if self.items.len() <= self.config.min_messages {
            return Ok(());
        }

        info!(
            "Compressing context: {} tokens -> target {}",
            self.total_tokens,
            self.config.max_tokens / 2
        );

        // Strategy: Remove lowest priority items first
        let mut items: Vec<_> = self.items.drain(..).collect();
        items.sort_by(|a, b| a.priority.partial_cmp(&b.priority).unwrap());

        // Keep high priority items
        let mut kept = VecDeque::new();
        let mut kept_tokens = 0;
        let target_tokens = self.config.max_tokens / 2;

        // Always keep system prompt
        if let Some(pos) = items
            .iter()
            .position(|i| i.item_type == ContextItemType::SystemPrompt)
        {
            let item = items.remove(pos);
            kept_tokens += item.tokens;
            kept.push_back(item);
        }

        // Keep recent messages (highest priority last)
        items.reverse();
        for item in items {
            if kept_tokens + item.tokens <= target_tokens {
                kept_tokens += item.tokens;
                kept.push_front(item);
            }
        }

        self.items = kept;
        self.total_tokens = kept_tokens;

        debug!(
            "Context compressed: {} items, {} tokens",
            self.items.len(),
            self.total_tokens
        );

        Ok(())
    }

    /// Calculate priority for an item type.
    fn calculate_priority(&self, item_type: ContextItemType) -> f32 {
        match item_type {
            ContextItemType::SystemPrompt => 1.0,
            ContextItemType::UserMessage => 0.8,
            ContextItemType::AssistantMessage => 0.7,
            ContextItemType::ToolResult => 0.6,
            ContextItemType::Summary => 0.5,
            ContextItemType::KnowledgeChunk => 0.4,
        }
    }

    /// Build messages for the LLM.
    pub fn build_messages(&self) -> Vec<Message> {
        self.items
            .iter()
            .filter_map(|item| match item.item_type {
                ContextItemType::SystemPrompt => Some(Message::system(&item.content)),
                ContextItemType::UserMessage => Some(Message::user(&item.content)),
                ContextItemType::AssistantMessage => Some(Message::assistant(&item.content)),
                ContextItemType::Summary => {
                    Some(Message::system(&format!("[Summary] {}", item.content)))
                }
                ContextItemType::KnowledgeChunk => {
                    Some(Message::system(&format!("[Context] {}", item.content)))
                }
                ContextItemType::ToolResult => {
                    Some(Message::user(&format!("[Tool Result] {}", item.content)))
                }
            })
            .collect()
    }

    /// Clear all context.
    pub fn clear(&mut self) {
        self.items.clear();
        self.total_tokens = 0;
    }

    /// Get token count.
    pub fn token_count(&self) -> usize {
        self.total_tokens
    }
}

/// Simple token counter (approximation).
fn count_tokens(text: &str) -> usize {
    // Rough approximation: ~4 characters per token
    // In production, use tiktoken
    (text.len() + 3) / 4
}

impl Default for ContextManager {
    fn default() -> Self {
        Self::new(ContextConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_manager() {
        let mut manager = ContextManager::new(ContextConfig::default());

        manager.add_message(&Message::user("Hello")).unwrap();
        manager
            .add_message(&Message::assistant("Hi there!"))
            .unwrap();

        let state = manager.state();
        assert_eq!(state.message_count, 2);
        assert!(state.total_tokens > 0);
    }

    #[test]
    fn test_token_counting() {
        assert_eq!(count_tokens("Hello"), 2); // 5 chars -> ~2 tokens
        assert_eq!(count_tokens("Hello, world!"), 4); // 13 chars -> ~4 tokens
    }
}
