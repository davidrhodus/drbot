//! Context window management.

use crate::{ContextItem, Result};
use std::collections::VecDeque;

/// Configuration for context window.
#[derive(Debug, Clone)]
pub struct WindowConfig {
    /// Maximum tokens.
    pub max_tokens: usize,
    /// Reserved for response.
    pub reserved_response: usize,
    /// Sliding window overlap.
    pub overlap_tokens: usize,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            max_tokens: 100000,
            reserved_response: 4096,
            overlap_tokens: 1000,
        }
    }
}

/// A sliding context window.
pub struct ContextWindow {
    config: WindowConfig,
    items: VecDeque<ContextItem>,
    total_tokens: usize,
}

impl ContextWindow {
    /// Create a new context window.
    pub fn new(config: WindowConfig) -> Self {
        Self {
            config,
            items: VecDeque::new(),
            total_tokens: 0,
        }
    }

    /// Get available tokens.
    pub fn available_tokens(&self) -> usize {
        self.config
            .max_tokens
            .saturating_sub(self.total_tokens)
            .saturating_sub(self.config.reserved_response)
    }

    /// Check if an item can fit.
    pub fn can_fit(&self, tokens: usize) -> bool {
        tokens <= self.available_tokens()
    }

    /// Add an item to the window.
    pub fn push(&mut self, item: ContextItem) -> Result<()> {
        if !self.can_fit(item.tokens) {
            self.make_room(item.tokens)?;
        }

        self.total_tokens += item.tokens;
        self.items.push_back(item);
        Ok(())
    }

    /// Make room for new tokens by removing old items.
    fn make_room(&mut self, needed: usize) -> Result<()> {
        let target = self
            .config
            .max_tokens
            .saturating_sub(self.config.reserved_response)
            .saturating_sub(needed);

        while self.total_tokens > target && !self.items.is_empty() {
            if let Some(item) = self.items.pop_front() {
                self.total_tokens = self.total_tokens.saturating_sub(item.tokens);
            }
        }

        Ok(())
    }

    /// Get all items.
    pub fn items(&self) -> &VecDeque<ContextItem> {
        &self.items
    }

    /// Get total tokens.
    pub fn total_tokens(&self) -> usize {
        self.total_tokens
    }

    /// Get item count.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Clear the window.
    pub fn clear(&mut self) {
        self.items.clear();
        self.total_tokens = 0;
    }

    /// Get a slice of the window as a new window.
    pub fn slice(&self, start: usize, end: usize) -> ContextWindow {
        let mut window = ContextWindow::new(self.config.clone());

        for (i, item) in self.items.iter().enumerate() {
            if i >= start && i < end {
                window.total_tokens += item.tokens;
                window.items.push_back(item.clone());
            }
        }

        window
    }

    /// Create overlapping windows for processing long contexts.
    pub fn create_chunks(&self, chunk_size: usize) -> Vec<ContextWindow> {
        let mut chunks = Vec::new();
        let mut start = 0;

        while start < self.items.len() {
            let end = (start + chunk_size).min(self.items.len());
            chunks.push(self.slice(start, end));

            // Move start with overlap
            start = end.saturating_sub(self.config.overlap_tokens / 100); // Rough estimate
            if start >= end {
                break;
            }
        }

        chunks
    }
}

impl Default for ContextWindow {
    fn default() -> Self {
        Self::new(WindowConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ContextItemType;
    use chrono::Utc;

    fn test_item(tokens: usize) -> ContextItem {
        ContextItem {
            item_type: ContextItemType::UserMessage,
            content: "test".to_string(),
            tokens,
            priority: 0.5,
            timestamp: Utc::now(),
        }
    }

    #[test]
    fn test_window_basic() {
        let mut window = ContextWindow::new(WindowConfig {
            max_tokens: 1000,
            reserved_response: 100,
            overlap_tokens: 50,
        });

        window.push(test_item(100)).unwrap();
        window.push(test_item(200)).unwrap();

        assert_eq!(window.total_tokens(), 300);
        assert_eq!(window.len(), 2);
    }

    #[test]
    fn test_window_overflow() {
        let mut window = ContextWindow::new(WindowConfig {
            max_tokens: 500,
            reserved_response: 100,
            overlap_tokens: 50,
        });

        window.push(test_item(200)).unwrap();
        window.push(test_item(200)).unwrap();
        window.push(test_item(200)).unwrap(); // Should push out first item

        assert!(window.total_tokens() <= 400);
    }
}
