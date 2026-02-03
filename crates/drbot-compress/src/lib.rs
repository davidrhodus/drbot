//! Smart context compression for drbot.
//!
//! Intelligently compress context to fit within model limits.
//!
//! # Features
//!
//! - Relevance-based pruning
//! - Intelligent summarization
//! - Token counting and budgeting
//! - Conversation compression
//! - Document chunking with overlap

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Compression result type.
pub type Result<T> = std::result::Result<T, CompressError>;

/// Compression errors.
#[derive(Debug, thiserror::Error)]
pub enum CompressError {
    #[error("Cannot compress below minimum: {0}")]
    BelowMinimum(usize),
    #[error("Summarization failed: {0}")]
    SummarizationFailed(String),
    #[error("Token counting failed: {0}")]
    TokenCountFailed(String),
}

/// A piece of context that can be compressed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextItem {
    /// Item ID.
    pub id: Uuid,
    /// Content.
    pub content: String,
    /// Compressed content (if available).
    pub compressed: Option<String>,
    /// Token count.
    pub tokens: usize,
    /// Compressed token count.
    pub compressed_tokens: Option<usize>,
    /// Relevance score (0-1).
    pub relevance: f32,
    /// Recency score (0-1).
    pub recency: f32,
    /// Importance score (0-1).
    pub importance: f32,
    /// Source type.
    pub source: ContextSource,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
}

impl ContextItem {
    /// Create a new context item.
    pub fn new(content: &str, source: ContextSource) -> Self {
        Self {
            id: Uuid::new_v4(),
            content: content.to_string(),
            compressed: None,
            tokens: estimate_tokens(content),
            compressed_tokens: None,
            relevance: 0.5,
            recency: 1.0,
            importance: 0.5,
            source,
            timestamp: Utc::now(),
        }
    }

    /// Set relevance.
    pub fn with_relevance(mut self, relevance: f32) -> Self {
        self.relevance = relevance.clamp(0.0, 1.0);
        self
    }

    /// Set importance.
    pub fn with_importance(mut self, importance: f32) -> Self {
        self.importance = importance.clamp(0.0, 1.0);
        self
    }

    /// Combined score for prioritization.
    pub fn combined_score(&self) -> f32 {
        self.relevance * 0.4 + self.recency * 0.3 + self.importance * 0.3
    }

    /// Get effective content (compressed if available).
    pub fn effective_content(&self) -> &str {
        self.compressed.as_ref().unwrap_or(&self.content)
    }

    /// Get effective token count.
    pub fn effective_tokens(&self) -> usize {
        self.compressed_tokens.unwrap_or(self.tokens)
    }
}

/// Context source types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextSource {
    /// Conversation message.
    Conversation,
    /// Retrieved document.
    Document,
    /// System context.
    System,
    /// Tool result.
    ToolResult,
    /// Memory.
    Memory,
    /// User instruction.
    Instruction,
}

/// Estimate tokens for text (rough approximation).
fn estimate_tokens(text: &str) -> usize {
    // Rough estimate: 4 characters per token
    text.len() / 4 + 1
}

/// Compression strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompressionStrategy {
    /// Remove least relevant items first.
    RelevancePruning,
    /// Summarize all items.
    Summarization,
    /// Summarize older items, keep recent verbatim.
    HybridRecency,
    /// Keep first and last, summarize middle.
    HeadTail,
    /// Semantic deduplication.
    Deduplication,
}

/// Compression configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressConfig {
    /// Target token budget.
    pub target_tokens: usize,
    /// Minimum tokens to preserve.
    pub min_tokens: usize,
    /// Compression strategy.
    pub strategy: CompressionStrategy,
    /// Preserve system messages.
    pub preserve_system: bool,
    /// Preserve recent count.
    pub preserve_recent: usize,
    /// Summarization ratio (target summary / original).
    pub summary_ratio: f32,
}

impl Default for CompressConfig {
    fn default() -> Self {
        Self {
            target_tokens: 32000,
            min_tokens: 1000,
            strategy: CompressionStrategy::HybridRecency,
            preserve_system: true,
            preserve_recent: 5,
            summary_ratio: 0.25,
        }
    }
}

/// Compression result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressResult {
    /// Compressed items.
    pub items: Vec<ContextItem>,
    /// Original token count.
    pub original_tokens: usize,
    /// Compressed token count.
    pub compressed_tokens: usize,
    /// Compression ratio.
    pub ratio: f32,
    /// Items removed.
    pub items_removed: usize,
    /// Items summarized.
    pub items_summarized: usize,
}

/// Trait for compression providers.
#[async_trait]
pub trait CompressionProvider: Send + Sync {
    /// Summarize text.
    async fn summarize(&self, text: &str, target_tokens: usize) -> Result<String>;

    /// Check semantic similarity.
    async fn similarity(&self, a: &str, b: &str) -> f32;
}

/// Context compressor.
pub struct ContextCompressor<P: CompressionProvider> {
    config: CompressConfig,
    provider: P,
}

impl<P: CompressionProvider> ContextCompressor<P> {
    /// Create a new compressor.
    pub fn new(config: CompressConfig, provider: P) -> Self {
        Self { config, provider }
    }

    /// Compress context items to fit within budget.
    pub async fn compress(&self, mut items: Vec<ContextItem>) -> Result<CompressResult> {
        let original_tokens: usize = items.iter().map(|i| i.tokens).sum();

        if original_tokens <= self.config.target_tokens {
            return Ok(CompressResult {
                items,
                original_tokens,
                compressed_tokens: original_tokens,
                ratio: 1.0,
                items_removed: 0,
                items_summarized: 0,
            });
        }

        match self.config.strategy {
            CompressionStrategy::RelevancePruning => self.prune_by_relevance(&mut items).await,
            CompressionStrategy::Summarization => self.summarize_all(&mut items).await,
            CompressionStrategy::HybridRecency => self.hybrid_compress(&mut items).await,
            CompressionStrategy::HeadTail => self.head_tail_compress(&mut items).await,
            CompressionStrategy::Deduplication => self.deduplicate(&mut items).await,
        }
    }

    async fn prune_by_relevance(&self, items: &mut Vec<ContextItem>) -> Result<CompressResult> {
        let original_tokens: usize = items.iter().map(|i| i.tokens).sum();
        let mut items_removed = 0;

        // Sort by score (descending)
        items.sort_by(|a, b| {
            b.combined_score()
                .partial_cmp(&a.combined_score())
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Remove items until under budget
        let mut current_tokens: usize = items.iter().map(|i| i.tokens).sum();

        while current_tokens > self.config.target_tokens && items.len() > 1 {
            // Remove last (lowest score) item unless it's a system message
            if let Some(item) = items.last() {
                if self.config.preserve_system && item.source == ContextSource::System {
                    break;
                }
            }

            if let Some(removed) = items.pop() {
                current_tokens -= removed.tokens;
                items_removed += 1;
            }
        }

        Ok(CompressResult {
            items: items.clone(),
            original_tokens,
            compressed_tokens: current_tokens,
            ratio: current_tokens as f32 / original_tokens as f32,
            items_removed,
            items_summarized: 0,
        })
    }

    async fn summarize_all(&self, items: &mut Vec<ContextItem>) -> Result<CompressResult> {
        let original_tokens: usize = items.iter().map(|i| i.tokens).sum();
        let mut items_summarized = 0;

        for item in items.iter_mut() {
            if item.source == ContextSource::System && self.config.preserve_system {
                continue;
            }

            let target = (item.tokens as f32 * self.config.summary_ratio) as usize;
            if target < item.tokens && target >= 10 {
                if let Ok(summary) = self.provider.summarize(&item.content, target).await {
                    item.compressed = Some(summary.clone());
                    item.compressed_tokens = Some(estimate_tokens(&summary));
                    items_summarized += 1;
                }
            }
        }

        let compressed_tokens: usize = items.iter().map(|i| i.effective_tokens()).sum();

        Ok(CompressResult {
            items: items.clone(),
            original_tokens,
            compressed_tokens,
            ratio: compressed_tokens as f32 / original_tokens as f32,
            items_removed: 0,
            items_summarized,
        })
    }

    async fn hybrid_compress(&self, items: &mut Vec<ContextItem>) -> Result<CompressResult> {
        let original_tokens: usize = items.iter().map(|i| i.tokens).sum();
        let mut items_summarized = 0;

        // Sort by timestamp (newest first)
        items.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

        // Keep recent items verbatim
        let preserve_count = self.config.preserve_recent.min(items.len());

        // Summarize older items
        for item in items.iter_mut().skip(preserve_count) {
            if item.source == ContextSource::System && self.config.preserve_system {
                continue;
            }

            let target = (item.tokens as f32 * self.config.summary_ratio) as usize;
            if target < item.tokens && target >= 10 {
                if let Ok(summary) = self.provider.summarize(&item.content, target).await {
                    item.compressed = Some(summary.clone());
                    item.compressed_tokens = Some(estimate_tokens(&summary));
                    items_summarized += 1;
                }
            }
        }

        // If still over budget, prune
        let mut current_tokens: usize = items.iter().map(|i| i.effective_tokens()).sum();
        let mut items_removed = 0;

        while current_tokens > self.config.target_tokens && items.len() > preserve_count {
            if let Some(removed) = items.pop() {
                current_tokens -= removed.effective_tokens();
                items_removed += 1;
            }
        }

        Ok(CompressResult {
            items: items.clone(),
            original_tokens,
            compressed_tokens: current_tokens,
            ratio: current_tokens as f32 / original_tokens as f32,
            items_removed,
            items_summarized,
        })
    }

    async fn head_tail_compress(&self, items: &mut Vec<ContextItem>) -> Result<CompressResult> {
        let original_tokens: usize = items.iter().map(|i| i.tokens).sum();

        if items.len() <= 4 {
            return Ok(CompressResult {
                items: items.clone(),
                original_tokens,
                compressed_tokens: original_tokens,
                ratio: 1.0,
                items_removed: 0,
                items_summarized: 0,
            });
        }

        // Keep first 2 and last 2, summarize middle
        let head: Vec<_> = items.iter().take(2).cloned().collect();
        let tail: Vec<_> = items.iter().rev().take(2).cloned().collect();
        let middle: Vec<_> = items
            .iter()
            .skip(2)
            .take(items.len() - 4)
            .cloned()
            .collect();

        // Summarize middle into one item
        let middle_content: String = middle
            .iter()
            .map(|i| i.content.clone())
            .collect::<Vec<_>>()
            .join("\n\n");

        let middle_tokens: usize = middle.iter().map(|i| i.tokens).sum();
        let target = (middle_tokens as f32 * self.config.summary_ratio) as usize;

        let summary = self.provider.summarize(&middle_content, target).await?;

        let summary_item = ContextItem {
            id: Uuid::new_v4(),
            content: middle_content,
            compressed: Some(summary.clone()),
            tokens: middle_tokens,
            compressed_tokens: Some(estimate_tokens(&summary)),
            relevance: 0.5,
            recency: 0.5,
            importance: 0.5,
            source: ContextSource::Conversation,
            timestamp: Utc::now(),
        };

        let mut result = head;
        result.push(summary_item);
        result.extend(tail.into_iter().rev());

        let compressed_tokens: usize = result.iter().map(|i| i.effective_tokens()).sum();

        Ok(CompressResult {
            items: result,
            original_tokens,
            compressed_tokens,
            ratio: compressed_tokens as f32 / original_tokens as f32,
            items_removed: middle.len() - 1,
            items_summarized: 1,
        })
    }

    async fn deduplicate(&self, items: &mut Vec<ContextItem>) -> Result<CompressResult> {
        let original_tokens: usize = items.iter().map(|i| i.tokens).sum();
        let original_count = items.len();

        // Find and remove semantically similar items
        let mut i = 0;
        while i < items.len() {
            let mut j = i + 1;
            while j < items.len() {
                let similarity = self
                    .provider
                    .similarity(&items[i].content, &items[j].content)
                    .await;

                if similarity > 0.9 {
                    // Keep the more important/recent one
                    if items[i].combined_score() >= items[j].combined_score() {
                        items.remove(j);
                    } else {
                        items.remove(i);
                        break;
                    }
                } else {
                    j += 1;
                }
            }
            i += 1;
        }

        let compressed_tokens: usize = items.iter().map(|i| i.tokens).sum();

        Ok(CompressResult {
            items: items.clone(),
            original_tokens,
            compressed_tokens,
            ratio: compressed_tokens as f32 / original_tokens as f32,
            items_removed: original_count - items.len(),
            items_summarized: 0,
        })
    }
}

/// Simple compression provider (no-op for testing).
pub struct SimpleCompressionProvider;

#[async_trait]
impl CompressionProvider for SimpleCompressionProvider {
    async fn summarize(&self, text: &str, target_tokens: usize) -> Result<String> {
        // Simple truncation as fallback
        let target_chars = target_tokens * 4;
        if text.len() <= target_chars {
            Ok(text.to_string())
        } else {
            Ok(format!("{}...", &text[..target_chars.min(text.len())]))
        }
    }

    async fn similarity(&self, a: &str, b: &str) -> f32 {
        // Simple Jaccard similarity
        let words_a: std::collections::HashSet<_> = a.split_whitespace().collect();
        let words_b: std::collections::HashSet<_> = b.split_whitespace().collect();

        let intersection = words_a.intersection(&words_b).count();
        let union = words_a.union(&words_b).count();

        if union == 0 {
            0.0
        } else {
            intersection as f32 / union as f32
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_context_compression() {
        let config = CompressConfig {
            target_tokens: 100,
            strategy: CompressionStrategy::RelevancePruning,
            ..Default::default()
        };

        let provider = SimpleCompressionProvider;
        let compressor = ContextCompressor::new(config, provider);

        let items: Vec<ContextItem> = (0..10)
            .map(|i| {
                ContextItem::new(
                    &format!("This is context item number {} with some content.", i),
                    ContextSource::Conversation,
                )
                .with_relevance(i as f32 / 10.0)
            })
            .collect();

        let result = compressor.compress(items).await.unwrap();
        assert!(result.compressed_tokens <= 100 || result.items.len() == 1);
    }

    #[test]
    fn test_token_estimation() {
        let text = "Hello, this is a test message with some content.";
        let tokens = estimate_tokens(text);
        assert!(tokens > 0);
        assert!(tokens < text.len());
    }

    #[test]
    fn test_combined_score() {
        let item = ContextItem::new("Test", ContextSource::Conversation)
            .with_relevance(0.8)
            .with_importance(0.9);

        let score = item.combined_score();
        assert!(score > 0.5);
    }
}
