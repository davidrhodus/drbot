//! Context summarization.

use crate::{ContextError, Result};
use drbot_core::message::Message;
use drbot_providers::{ChatOptions, Provider};
use std::sync::Arc;

/// A summary of context.
#[derive(Debug, Clone)]
pub struct Summary {
    /// Summary text.
    pub text: String,
    /// Number of messages summarized.
    pub message_count: usize,
    /// Original token count.
    pub original_tokens: usize,
    /// Summary token count.
    pub summary_tokens: usize,
    /// Compression ratio.
    pub compression_ratio: f32,
}

impl Summary {
    /// Create a new summary.
    pub fn new(text: &str, message_count: usize, original_tokens: usize) -> Self {
        let summary_tokens = (text.len() + 3) / 4; // Rough estimate
        let compression_ratio = if original_tokens > 0 {
            summary_tokens as f32 / original_tokens as f32
        } else {
            1.0
        };

        Self {
            text: text.to_string(),
            message_count,
            original_tokens,
            summary_tokens,
            compression_ratio,
        }
    }
}

/// Summarizer for compressing context.
pub struct Summarizer {
    provider: Arc<dyn Provider>,
    max_summary_tokens: usize,
}

impl Summarizer {
    /// Create a new summarizer.
    pub fn new(provider: Arc<dyn Provider>) -> Self {
        Self {
            provider,
            max_summary_tokens: 500,
        }
    }

    /// Set max summary tokens.
    pub fn with_max_tokens(mut self, tokens: usize) -> Self {
        self.max_summary_tokens = tokens;
        self
    }

    /// Summarize a conversation.
    pub async fn summarize(&self, messages: &[Message]) -> Result<Summary> {
        if messages.is_empty() {
            return Ok(Summary::new("", 0, 0));
        }

        // Build conversation text
        let conversation: Vec<String> = messages
            .iter()
            .map(|m| {
                let content = m.text_content();
                let role = match m.role {
                    drbot_core::message::Role::User => "User",
                    drbot_core::message::Role::Assistant => "Assistant",
                    drbot_core::message::Role::System => "System",
                };
                format!("{}: {}", role, content)
            })
            .collect();

        let conversation_text = conversation.join("\n");
        let original_tokens = (conversation_text.len() + 3) / 4;

        // Create summarization prompt
        let prompt = format!(
            "Please provide a concise summary of the following conversation, \
             capturing the key points, decisions, and any important context. \
             Keep the summary under {} tokens.\n\n\
             Conversation:\n{}\n\n\
             Summary:",
            self.max_summary_tokens, conversation_text
        );

        let request_messages = vec![Message::user(&prompt)];
        let options = ChatOptions {
            model: None,
            max_tokens: Some(self.max_summary_tokens),
            temperature: Some(0.3),
            top_p: None,
            stop_sequences: None,
            system_prompt: Some(
                "You are a helpful assistant that creates concise conversation summaries."
                    .to_string(),
            ),
            tools: None,
        };

        let response = self
            .provider
            .chat(&request_messages, options)
            .await
            .map_err(|e| ContextError::SummarizationFailed(e.to_string()))?;

        Ok(Summary::new(
            &response.content,
            messages.len(),
            original_tokens,
        ))
    }

    /// Summarize incrementally (for rolling summaries).
    pub async fn summarize_incremental(
        &self,
        previous_summary: Option<&Summary>,
        new_messages: &[Message],
    ) -> Result<Summary> {
        if new_messages.is_empty() {
            return previous_summary.cloned().ok_or_else(|| {
                ContextError::SummarizationFailed("No content to summarize".to_string())
            });
        }

        let new_conversation: Vec<String> = new_messages
            .iter()
            .map(|m| {
                let content = m.text_content();
                let role = match m.role {
                    drbot_core::message::Role::User => "User",
                    drbot_core::message::Role::Assistant => "Assistant",
                    drbot_core::message::Role::System => "System",
                };
                format!("{}: {}", role, content)
            })
            .collect();

        let new_text = new_conversation.join("\n");
        let total_messages =
            new_messages.len() + previous_summary.map(|s| s.message_count).unwrap_or(0);
        let original_tokens =
            (new_text.len() + 3) / 4 + previous_summary.map(|s| s.original_tokens).unwrap_or(0);

        let prompt = if let Some(prev) = previous_summary {
            format!(
                "You have a previous conversation summary and new messages. \
                 Create an updated summary that incorporates both.\n\n\
                 Previous Summary:\n{}\n\n\
                 New Messages:\n{}\n\n\
                 Updated Summary:",
                prev.text, new_text
            )
        } else {
            format!(
                "Summarize this conversation concisely:\n\n{}\n\nSummary:",
                new_text
            )
        };

        let request_messages = vec![Message::user(&prompt)];
        let options = ChatOptions {
            model: None,
            max_tokens: Some(self.max_summary_tokens),
            temperature: Some(0.3),
            top_p: None,
            stop_sequences: None,
            system_prompt: Some(
                "You are a helpful assistant that creates concise conversation summaries."
                    .to_string(),
            ),
            tools: None,
        };

        let response = self
            .provider
            .chat(&request_messages, options)
            .await
            .map_err(|e| ContextError::SummarizationFailed(e.to_string()))?;

        Ok(Summary::new(
            &response.content,
            total_messages,
            original_tokens,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_summary_creation() {
        let summary = Summary::new("This is a test summary", 5, 1000);

        assert_eq!(summary.message_count, 5);
        assert_eq!(summary.original_tokens, 1000);
        assert!(summary.compression_ratio < 1.0);
    }
}
