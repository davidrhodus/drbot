//! Semantic caching with embeddings for drbot.
//!
//! Provides intelligent caching that can find similar queries
//! and return cached responses, reducing API calls and latency.
//!
//! # Features
//!
//! - Embedding-based similarity search
//! - TTL and size-based eviction
//! - Hybrid local/distributed caching
//! - Cache warming and preloading
//!
//! # Example
//!
//! ```rust,no_run
//! use drbot_cache::{SemanticCache, CacheConfig};
//!
//! async fn example() {
//!     let cache = SemanticCache::new(CacheConfig::default()).await.unwrap();
//!
//!     // Check for similar cached response
//!     if let Some(response) = cache.get_similar("What's the weather?", 0.9).await {
//!         println!("Cache hit: {}", response.content);
//!     }
//!
//!     // Store new response
//!     cache.set("What's the weather?", "It's sunny and 72°F").await;
//! }
//! ```

mod cache;
mod embedding;
mod similarity;
mod storage;

pub use cache::{CacheConfig, CacheEntry, CacheStats, SemanticCache};
pub use embedding::{EmbeddingProvider, LocalEmbedding};
pub use similarity::{CosineSimilarity, SimilarityMetric};
pub use storage::{CacheStorage, MemoryStorage, SqliteStorage};

/// Result type for cache operations.
pub type Result<T> = std::result::Result<T, CacheError>;

/// Cache errors.
#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    #[error("Embedding generation failed: {0}")]
    EmbeddingFailed(String),
    #[error("Storage error: {0}")]
    StorageError(String),
    #[error("Cache full")]
    CacheFull,
    #[error("Entry not found")]
    NotFound,
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_cache_basic() {
        let cache = SemanticCache::new(CacheConfig::default()).await.unwrap();

        cache.set("test query", "test response").await.unwrap();

        let result = cache.get_exact("test query").await;
        assert!(result.is_some());
        assert_eq!(result.unwrap().content, "test response");
    }
}
