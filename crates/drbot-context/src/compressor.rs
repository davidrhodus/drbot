//! Context compression strategies.

use crate::{ContextItem, ContextItemType, Result};

/// Compression strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressionStrategy {
    /// Remove oldest messages first.
    RemoveOldest,
    /// Remove lowest priority items.
    RemoveLowestPriority,
    /// Summarize old messages.
    Summarize,
    /// Hybrid: summarize then remove.
    Hybrid,
}

/// Context compressor.
pub struct ContextCompressor {
    strategy: CompressionStrategy,
}

impl ContextCompressor {
    /// Create a new compressor.
    pub fn new() -> Self {
        Self {
            strategy: CompressionStrategy::RemoveLowestPriority,
        }
    }

    /// Set compression strategy.
    pub fn with_strategy(mut self, strategy: CompressionStrategy) -> Self {
        self.strategy = strategy;
        self
    }

    /// Compress items to fit within target tokens.
    pub fn compress(&self, items: Vec<ContextItem>, target_tokens: usize) -> Vec<ContextItem> {
        match self.strategy {
            CompressionStrategy::RemoveOldest => self.compress_remove_oldest(items, target_tokens),
            CompressionStrategy::RemoveLowestPriority => {
                self.compress_remove_lowest_priority(items, target_tokens)
            }
            CompressionStrategy::Summarize => self.compress_summarize(items, target_tokens),
            CompressionStrategy::Hybrid => self.compress_hybrid(items, target_tokens),
        }
    }

    /// Remove oldest items until target is reached.
    fn compress_remove_oldest(
        &self,
        items: Vec<ContextItem>,
        target_tokens: usize,
    ) -> Vec<ContextItem> {
        let mut result: Vec<ContextItem> = Vec::new();
        let mut total = 0;

        // Keep system prompt always
        let (system_items, other_items): (Vec<_>, Vec<_>) = items
            .into_iter()
            .partition(|i| i.item_type == ContextItemType::SystemPrompt);

        for item in system_items {
            total += item.tokens;
            result.push(item);
        }

        // Add from newest to oldest (reverse order)
        for item in other_items.into_iter().rev() {
            if total + item.tokens <= target_tokens {
                total += item.tokens;
                // Insert after system prompt if present, otherwise at front
                let insert_pos = if result.is_empty() { 0 } else { 1 };
                result.insert(insert_pos, item);
            }
        }

        result
    }

    /// Remove lowest priority items until target is reached.
    fn compress_remove_lowest_priority(
        &self,
        mut items: Vec<ContextItem>,
        target_tokens: usize,
    ) -> Vec<ContextItem> {
        // Sort by priority (highest first)
        items.sort_by(|a, b| b.priority.partial_cmp(&a.priority).unwrap());

        let mut result = Vec::new();
        let mut total = 0;

        for item in items {
            if total + item.tokens <= target_tokens {
                total += item.tokens;
                result.push(item);
            }
        }

        // Re-sort by timestamp to maintain order
        result.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
        result
    }

    /// Summarize old items (placeholder - actual summarization happens in manager).
    fn compress_summarize(
        &self,
        items: Vec<ContextItem>,
        target_tokens: usize,
    ) -> Vec<ContextItem> {
        // For now, fall back to remove oldest
        self.compress_remove_oldest(items, target_tokens)
    }

    /// Hybrid compression.
    fn compress_hybrid(&self, items: Vec<ContextItem>, target_tokens: usize) -> Vec<ContextItem> {
        // First try priority-based
        let result = self.compress_remove_lowest_priority(items.clone(), target_tokens);

        let total: usize = result.iter().map(|i| i.tokens).sum();
        if total <= target_tokens {
            return result;
        }

        // If still too large, use oldest-based
        self.compress_remove_oldest(items, target_tokens)
    }

    /// Calculate how many tokens to remove.
    pub fn tokens_to_remove(&self, current: usize, target: usize) -> usize {
        current.saturating_sub(target)
    }

    /// Estimate compression ratio.
    pub fn estimate_ratio(&self, items: &[ContextItem], target_tokens: usize) -> f32 {
        let current: usize = items.iter().map(|i| i.tokens).sum();
        if current == 0 {
            return 1.0;
        }
        target_tokens as f32 / current as f32
    }
}

impl Default for ContextCompressor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn test_item(tokens: usize, priority: f32) -> ContextItem {
        ContextItem {
            item_type: ContextItemType::UserMessage,
            content: "test".to_string(),
            tokens,
            priority,
            timestamp: Utc::now(),
        }
    }

    #[test]
    fn test_compress_remove_oldest() {
        let compressor = ContextCompressor::new().with_strategy(CompressionStrategy::RemoveOldest);

        let items = vec![
            test_item(100, 0.5),
            test_item(100, 0.5),
            test_item(100, 0.5),
        ];

        let result = compressor.compress(items, 200);
        assert!(result.iter().map(|i| i.tokens).sum::<usize>() <= 200);
    }

    #[test]
    fn test_compress_remove_lowest_priority() {
        let compressor =
            ContextCompressor::new().with_strategy(CompressionStrategy::RemoveLowestPriority);

        let items = vec![
            test_item(100, 0.3), // Low priority
            test_item(100, 0.9), // High priority
            test_item(100, 0.5), // Medium priority
        ];

        let result = compressor.compress(items, 200);

        // Should keep high and medium priority
        assert_eq!(result.len(), 2);
        assert!(result.iter().all(|i| i.priority >= 0.5));
    }
}
