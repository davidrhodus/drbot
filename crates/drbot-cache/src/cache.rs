//! Main semantic cache implementation.

use crate::embedding::{Embedding, EmbeddingProvider, LocalEmbedding};
use crate::similarity::{CosineSimilarity, SimilarityMetric};
use crate::storage::{CacheStorage, MemoryStorage};
use crate::{CacheError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info};

/// Cache configuration.
#[derive(Debug, Clone)]
pub struct CacheConfig {
    /// Maximum number of entries.
    pub max_entries: usize,
    /// Default TTL in seconds.
    pub default_ttl_secs: Option<u64>,
    /// Similarity threshold for cache hits (0.0 - 1.0).
    pub similarity_threshold: f32,
    /// Enable automatic eviction.
    pub auto_evict: bool,
    /// Eviction interval in seconds.
    pub eviction_interval_secs: u64,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            max_entries: 10000,
            default_ttl_secs: Some(3600), // 1 hour
            similarity_threshold: 0.85,
            auto_evict: true,
            eviction_interval_secs: 300, // 5 minutes
        }
    }
}

/// A cached entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry {
    /// Original query.
    pub query: String,
    /// Cached response content.
    pub content: String,
    /// Embedding vector.
    pub embedding: Embedding,
    /// When the entry was created.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// When the entry expires.
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Number of times this entry was hit.
    pub hits: u64,
    /// Additional metadata.
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Cache statistics.
#[derive(Debug, Clone, Default)]
pub struct CacheStats {
    /// Total number of lookups.
    pub total_lookups: u64,
    /// Number of exact hits.
    pub exact_hits: u64,
    /// Number of similar hits.
    pub similar_hits: u64,
    /// Number of misses.
    pub misses: u64,
    /// Number of entries.
    pub entries: usize,
    /// Number of evictions.
    pub evictions: u64,
}

impl CacheStats {
    /// Calculate hit rate.
    pub fn hit_rate(&self) -> f32 {
        if self.total_lookups == 0 {
            return 0.0;
        }
        (self.exact_hits + self.similar_hits) as f32 / self.total_lookups as f32
    }
}

/// Semantic cache with embedding-based similarity search.
pub struct SemanticCache {
    config: CacheConfig,
    storage: Arc<dyn CacheStorage>,
    embedding_provider: Arc<dyn EmbeddingProvider>,
    similarity_metric: Arc<dyn SimilarityMetric>,
    stats: Arc<RwLock<CacheStats>>,
}

impl SemanticCache {
    /// Create a new semantic cache with default providers.
    pub async fn new(config: CacheConfig) -> Result<Self> {
        let storage = Arc::new(MemoryStorage::new());
        let embedding_provider = Arc::new(LocalEmbedding::default());
        let similarity_metric = Arc::new(CosineSimilarity);

        Ok(Self {
            config,
            storage,
            embedding_provider,
            similarity_metric,
            stats: Arc::new(RwLock::new(CacheStats::default())),
        })
    }

    /// Create with custom providers.
    pub fn with_providers(
        config: CacheConfig,
        storage: Arc<dyn CacheStorage>,
        embedding_provider: Arc<dyn EmbeddingProvider>,
        similarity_metric: Arc<dyn SimilarityMetric>,
    ) -> Self {
        Self {
            config,
            storage,
            embedding_provider,
            similarity_metric,
            stats: Arc::new(RwLock::new(CacheStats::default())),
        }
    }

    /// Store a query-response pair.
    pub async fn set(&self, query: &str, content: &str) -> Result<()> {
        self.set_with_metadata(query, content, HashMap::new()).await
    }

    /// Store a query-response pair with metadata.
    pub async fn set_with_metadata(
        &self,
        query: &str,
        content: &str,
        metadata: HashMap<String, serde_json::Value>,
    ) -> Result<()> {
        // Check capacity
        if self.storage.len().await? >= self.config.max_entries {
            if self.config.auto_evict {
                self.storage.evict_expired().await?;
            }
            if self.storage.len().await? >= self.config.max_entries {
                return Err(CacheError::CacheFull);
            }
        }

        // Generate embedding
        let embedding = self.embedding_provider.embed(query).await?;

        // Calculate expiration
        let expires_at = self
            .config
            .default_ttl_secs
            .map(|ttl| chrono::Utc::now() + chrono::Duration::seconds(ttl as i64));

        let entry = CacheEntry {
            query: query.to_string(),
            content: content.to_string(),
            embedding,
            created_at: chrono::Utc::now(),
            expires_at,
            hits: 0,
            metadata,
        };

        self.storage.store(query, entry).await?;
        debug!("Cached response for query: {}", query);

        Ok(())
    }

    /// Get exact match.
    pub async fn get_exact(&self, query: &str) -> Option<CacheEntry> {
        let mut stats = self.stats.write().await;
        stats.total_lookups += 1;

        if let Ok(Some(entry)) = self.storage.get(query).await {
            // Check expiration
            if let Some(expires_at) = entry.expires_at {
                if expires_at < chrono::Utc::now() {
                    let _ = self.storage.delete(query).await;
                    stats.misses += 1;
                    return None;
                }
            }
            stats.exact_hits += 1;
            return Some(entry);
        }

        stats.misses += 1;
        None
    }

    /// Get similar match above threshold.
    pub async fn get_similar(&self, query: &str, threshold: f32) -> Option<CacheEntry> {
        let mut stats = self.stats.write().await;
        stats.total_lookups += 1;

        // Generate query embedding
        let query_embedding = match self.embedding_provider.embed(query).await {
            Ok(e) => e,
            Err(_) => {
                stats.misses += 1;
                return None;
            }
        };

        // Get all entries and find best match
        let entries = match self.storage.get_all_with_embeddings().await {
            Ok(e) => e,
            Err(_) => {
                stats.misses += 1;
                return None;
            }
        };

        let now = chrono::Utc::now();
        let mut best_match: Option<(f32, CacheEntry)> = None;

        for (_key, embedding, entry) in entries {
            // Check expiration
            if let Some(expires_at) = entry.expires_at {
                if expires_at < now {
                    continue;
                }
            }

            let similarity = self
                .similarity_metric
                .similarity(&query_embedding, &embedding);

            if similarity >= threshold {
                if best_match.is_none() || similarity > best_match.as_ref().unwrap().0 {
                    best_match = Some((similarity, entry));
                }
            }
        }

        if let Some((similarity, entry)) = best_match {
            debug!(
                "Similar cache hit ({}): {} -> {}",
                similarity, query, entry.query
            );
            stats.similar_hits += 1;
            return Some(entry);
        }

        stats.misses += 1;
        None
    }

    /// Get with automatic similarity fallback.
    pub async fn get(&self, query: &str) -> Option<CacheEntry> {
        // Try exact match first
        if let Some(entry) = self.get_exact(query).await {
            return Some(entry);
        }

        // Fall back to similar match
        self.get_similar(query, self.config.similarity_threshold)
            .await
    }

    /// Delete an entry.
    pub async fn delete(&self, query: &str) -> Result<()> {
        self.storage.delete(query).await
    }

    /// Clear all entries.
    pub async fn clear(&self) -> Result<()> {
        self.storage.clear().await
    }

    /// Get cache statistics.
    pub async fn stats(&self) -> CacheStats {
        let mut stats = self.stats.read().await.clone();
        stats.entries = self.storage.len().await.unwrap_or(0);
        stats
    }

    /// Manually trigger eviction.
    pub async fn evict_expired(&self) -> Result<usize> {
        let evicted = self.storage.evict_expired().await?;
        let mut stats = self.stats.write().await;
        stats.evictions += evicted as u64;
        Ok(evicted)
    }

    /// Warm the cache with pre-computed entries.
    pub async fn warm(&self, entries: Vec<(String, String)>) -> Result<usize> {
        let mut added = 0;
        for (query, content) in entries {
            if self.set(&query, &content).await.is_ok() {
                added += 1;
            }
        }
        info!("Warmed cache with {} entries", added);
        Ok(added)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_set_and_get_exact() {
        let cache = SemanticCache::new(CacheConfig::default()).await.unwrap();

        cache.set("hello world", "Hi there!").await.unwrap();

        let entry = cache.get_exact("hello world").await.unwrap();
        assert_eq!(entry.content, "Hi there!");
    }

    #[tokio::test]
    async fn test_similar_queries() {
        let cache = SemanticCache::new(CacheConfig::default()).await.unwrap();

        cache
            .set("what is the weather", "It's sunny")
            .await
            .unwrap();

        // Should find similar match
        let entry = cache.get_similar("what's the weather like", 0.5).await;
        assert!(entry.is_some());
    }

    #[tokio::test]
    async fn test_cache_stats() {
        let cache = SemanticCache::new(CacheConfig::default()).await.unwrap();

        cache.set("test", "response").await.unwrap();
        cache.get_exact("test").await;
        cache.get_exact("nonexistent").await;

        let stats = cache.stats().await;
        assert_eq!(stats.exact_hits, 1);
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.entries, 1);
    }

    #[tokio::test]
    async fn test_cache_warming() {
        let cache = SemanticCache::new(CacheConfig::default()).await.unwrap();

        let entries = vec![
            ("q1".to_string(), "r1".to_string()),
            ("q2".to_string(), "r2".to_string()),
            ("q3".to_string(), "r3".to_string()),
        ];

        let added = cache.warm(entries).await.unwrap();
        assert_eq!(added, 3);
        assert_eq!(cache.stats().await.entries, 3);
    }
}
