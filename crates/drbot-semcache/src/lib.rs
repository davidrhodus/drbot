//! Semantic caching for drbot.
//!
//! Cache responses by semantic meaning, not exact match.
//!
//! # Features
//!
//! - Embedding-based similarity matching
//! - Configurable similarity thresholds
//! - TTL and size-based eviction
//! - Cache statistics and analytics

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Semantic cache result type.
pub type Result<T> = std::result::Result<T, SemCacheError>;

/// Semantic cache errors.
#[derive(Debug, thiserror::Error)]
pub enum SemCacheError {
    #[error("Cache miss")]
    CacheMiss,
    #[error("Embedding failed: {0}")]
    EmbeddingFailed(String),
    #[error("Cache full")]
    CacheFull,
    #[error("Invalid threshold: {0}")]
    InvalidThreshold(f32),
}

/// Cached entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry {
    /// Entry ID.
    pub id: Uuid,
    /// Original query.
    pub query: String,
    /// Query embedding.
    pub embedding: Vec<f32>,
    /// Cached response.
    pub response: String,
    /// Response metadata.
    pub metadata: CacheMetadata,
    /// Created at.
    pub created_at: DateTime<Utc>,
    /// Last accessed.
    pub last_accessed: DateTime<Utc>,
    /// Access count.
    pub access_count: u64,
    /// TTL in seconds (None = never expires).
    pub ttl_secs: Option<u64>,
}

impl CacheEntry {
    /// Check if entry is expired.
    pub fn is_expired(&self) -> bool {
        if let Some(ttl) = self.ttl_secs {
            let age = Utc::now().signed_duration_since(self.created_at);
            age.num_seconds() as u64 > ttl
        } else {
            false
        }
    }
}

/// Cache metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheMetadata {
    /// Model used.
    pub model: String,
    /// Token count.
    pub tokens: usize,
    /// Generation time in ms.
    pub generation_time_ms: u64,
    /// Cost saved by cache hit.
    pub estimated_cost: f64,
    /// Custom metadata.
    pub custom: HashMap<String, serde_json::Value>,
}

impl Default for CacheMetadata {
    fn default() -> Self {
        Self {
            model: String::new(),
            tokens: 0,
            generation_time_ms: 0,
            estimated_cost: 0.0,
            custom: HashMap::new(),
        }
    }
}

/// Cache lookup result.
#[derive(Debug, Clone)]
pub struct CacheLookup {
    /// The matching entry.
    pub entry: CacheEntry,
    /// Similarity score.
    pub similarity: f32,
    /// Whether this was an exact match.
    pub exact_match: bool,
}

/// Cache statistics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CacheStats {
    /// Total entries.
    pub total_entries: usize,
    /// Cache hits.
    pub hits: u64,
    /// Cache misses.
    pub misses: u64,
    /// Hit rate.
    pub hit_rate: f32,
    /// Total tokens saved.
    pub tokens_saved: u64,
    /// Total cost saved.
    pub cost_saved: f64,
    /// Average similarity on hits.
    pub avg_similarity: f32,
    /// Cache size in bytes (estimated).
    pub size_bytes: usize,
}

/// Semantic cache configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemCacheConfig {
    /// Similarity threshold for cache hit (0-1).
    pub similarity_threshold: f32,
    /// Maximum cache entries.
    pub max_entries: usize,
    /// Default TTL in seconds.
    pub default_ttl_secs: Option<u64>,
    /// Enable exact match fast path.
    pub exact_match_enabled: bool,
    /// Embedding dimension.
    pub embedding_dim: usize,
}

impl Default for SemCacheConfig {
    fn default() -> Self {
        Self {
            similarity_threshold: 0.92,
            max_entries: 10000,
            default_ttl_secs: Some(3600), // 1 hour
            exact_match_enabled: true,
            embedding_dim: 1536,
        }
    }
}

/// Trait for embedding providers.
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    /// Generate embedding for text.
    async fn embed(&self, text: &str) -> Result<Vec<f32>>;
}

/// Semantic cache.
pub struct SemanticCache<E: EmbeddingProvider> {
    config: SemCacheConfig,
    provider: E,
    entries: Arc<RwLock<Vec<CacheEntry>>>,
    exact_match_index: Arc<RwLock<HashMap<String, Uuid>>>,
    stats: Arc<RwLock<CacheStats>>,
}

impl<E: EmbeddingProvider> SemanticCache<E> {
    /// Create a new semantic cache.
    pub fn new(config: SemCacheConfig, provider: E) -> Self {
        Self {
            config,
            provider,
            entries: Arc::new(RwLock::new(Vec::new())),
            exact_match_index: Arc::new(RwLock::new(HashMap::new())),
            stats: Arc::new(RwLock::new(CacheStats::default())),
        }
    }

    /// Look up a query in the cache.
    pub async fn get(&self, query: &str) -> Result<CacheLookup> {
        // Try exact match first
        if self.config.exact_match_enabled {
            let index = self.exact_match_index.read().await;
            if let Some(id) = index.get(query) {
                let entries = self.entries.read().await;
                if let Some(entry) = entries.iter().find(|e| &e.id == id) {
                    if !entry.is_expired() {
                        // Update stats
                        let mut stats = self.stats.write().await;
                        stats.hits += 1;
                        stats.tokens_saved += entry.metadata.tokens as u64;
                        stats.cost_saved += entry.metadata.estimated_cost;
                        stats.hit_rate = stats.hits as f32 / (stats.hits + stats.misses) as f32;

                        return Ok(CacheLookup {
                            entry: entry.clone(),
                            similarity: 1.0,
                            exact_match: true,
                        });
                    }
                }
            }
        }

        // Generate embedding for query
        let query_embedding = self.provider.embed(query).await?;

        // Find best match
        let entries = self.entries.read().await;
        let mut best_match: Option<(usize, f32)> = None;

        for (i, entry) in entries.iter().enumerate() {
            if entry.is_expired() {
                continue;
            }

            let similarity = cosine_similarity(&query_embedding, &entry.embedding);
            if similarity >= self.config.similarity_threshold {
                if best_match.is_none() || similarity > best_match.unwrap().1 {
                    best_match = Some((i, similarity));
                }
            }
        }

        drop(entries);

        if let Some((idx, similarity)) = best_match {
            let mut entries = self.entries.write().await;
            let entry = &mut entries[idx];
            entry.last_accessed = Utc::now();
            entry.access_count += 1;
            let result = entry.clone();

            // Update stats
            let mut stats = self.stats.write().await;
            stats.hits += 1;
            stats.tokens_saved += result.metadata.tokens as u64;
            stats.cost_saved += result.metadata.estimated_cost;
            let total = stats.hits + stats.misses;
            stats.hit_rate = if total > 0 {
                stats.hits as f32 / total as f32
            } else {
                0.0
            };
            stats.avg_similarity =
                (stats.avg_similarity * (stats.hits - 1) as f32 + similarity) / stats.hits as f32;

            Ok(CacheLookup {
                entry: result,
                similarity,
                exact_match: false,
            })
        } else {
            let mut stats = self.stats.write().await;
            stats.misses += 1;
            let total = stats.hits + stats.misses;
            stats.hit_rate = if total > 0 {
                stats.hits as f32 / total as f32
            } else {
                0.0
            };

            Err(SemCacheError::CacheMiss)
        }
    }

    /// Store a response in the cache.
    pub async fn put(&self, query: &str, response: &str, metadata: CacheMetadata) -> Result<Uuid> {
        let embedding = self.provider.embed(query).await?;

        let entry = CacheEntry {
            id: Uuid::new_v4(),
            query: query.to_string(),
            embedding,
            response: response.to_string(),
            metadata,
            created_at: Utc::now(),
            last_accessed: Utc::now(),
            access_count: 0,
            ttl_secs: self.config.default_ttl_secs,
        };

        let id = entry.id;

        // Evict if needed
        self.evict_if_needed().await;

        // Add entry
        self.entries.write().await.push(entry);

        // Update exact match index
        if self.config.exact_match_enabled {
            self.exact_match_index
                .write()
                .await
                .insert(query.to_string(), id);
        }

        // Update stats
        let mut stats = self.stats.write().await;
        stats.total_entries = self.entries.read().await.len();

        Ok(id)
    }

    /// Evict entries if cache is full.
    async fn evict_if_needed(&self) {
        let mut entries = self.entries.write().await;

        // Remove expired entries
        entries.retain(|e| !e.is_expired());

        // If still over limit, remove least recently used
        while entries.len() >= self.config.max_entries {
            if let Some(idx) = entries
                .iter()
                .enumerate()
                .min_by_key(|(_, e)| e.last_accessed)
                .map(|(i, _)| i)
            {
                let removed = entries.remove(idx);
                self.exact_match_index.write().await.remove(&removed.query);
            } else {
                break;
            }
        }
    }

    /// Invalidate a specific entry.
    pub async fn invalidate(&self, id: Uuid) -> bool {
        let mut entries = self.entries.write().await;
        if let Some(idx) = entries.iter().position(|e| e.id == id) {
            let removed = entries.remove(idx);
            self.exact_match_index.write().await.remove(&removed.query);
            true
        } else {
            false
        }
    }

    /// Invalidate entries matching a query pattern.
    pub async fn invalidate_pattern(&self, pattern: &str) -> usize {
        let mut entries = self.entries.write().await;
        let initial_len = entries.len();

        entries.retain(|e| !e.query.contains(pattern));

        let removed = initial_len - entries.len();

        // Rebuild exact match index
        let mut index = self.exact_match_index.write().await;
        index.clear();
        for entry in entries.iter() {
            index.insert(entry.query.clone(), entry.id);
        }

        removed
    }

    /// Clear the entire cache.
    pub async fn clear(&self) {
        self.entries.write().await.clear();
        self.exact_match_index.write().await.clear();
        *self.stats.write().await = CacheStats::default();
    }

    /// Get cache statistics.
    pub async fn stats(&self) -> CacheStats {
        let mut stats = self.stats.read().await.clone();
        stats.total_entries = self.entries.read().await.len();
        stats.size_bytes = self.estimate_size().await;
        stats
    }

    /// Estimate cache size in bytes.
    async fn estimate_size(&self) -> usize {
        let entries = self.entries.read().await;
        entries
            .iter()
            .map(|e| {
                e.query.len() + e.response.len() + e.embedding.len() * 4 + 200 // overhead
            })
            .sum()
    }

    /// Warm up cache with entries.
    pub async fn warm_up(&self, entries: Vec<(String, String, CacheMetadata)>) -> usize {
        let mut count = 0;
        for (query, response, metadata) in entries {
            if self.put(&query, &response, metadata).await.is_ok() {
                count += 1;
            }
        }
        count
    }
}

/// Calculate cosine similarity between two vectors.
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let mag_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let mag_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if mag_a == 0.0 || mag_b == 0.0 {
        0.0
    } else {
        dot / (mag_a * mag_b)
    }
}

/// Simple embedding provider for testing.
pub struct SimpleEmbedding {
    dim: usize,
}

impl SimpleEmbedding {
    /// Create a new simple embedding provider.
    pub fn new(dim: usize) -> Self {
        Self { dim }
    }
}

#[async_trait]
impl EmbeddingProvider for SimpleEmbedding {
    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        // Simple hash-based embedding for testing
        let mut embedding = vec![0.0f32; self.dim];
        for (i, c) in text.chars().enumerate() {
            embedding[i % self.dim] += (c as u32 as f32) / 1000.0;
        }
        // Normalize
        let mag: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        if mag > 0.0 {
            for x in &mut embedding {
                *x /= mag;
            }
        }
        Ok(embedding)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_semantic_cache() {
        let provider = SimpleEmbedding::new(128);
        let cache = SemanticCache::new(SemCacheConfig::default(), provider);

        let metadata = CacheMetadata {
            model: "test".to_string(),
            tokens: 100,
            ..Default::default()
        };

        // Store entry
        cache
            .put("What is the capital of France?", "Paris", metadata)
            .await
            .unwrap();

        // Exact match
        let result = cache.get("What is the capital of France?").await.unwrap();
        assert_eq!(result.entry.response, "Paris");
        assert!(result.exact_match);
    }

    #[tokio::test]
    async fn test_cache_miss() {
        let provider = SimpleEmbedding::new(128);
        let cache = SemanticCache::new(SemCacheConfig::default(), provider);

        let result = cache.get("Unknown query").await;
        assert!(matches!(result, Err(SemCacheError::CacheMiss)));
    }

    #[tokio::test]
    async fn test_cache_stats() {
        let provider = SimpleEmbedding::new(128);
        let cache = SemanticCache::new(SemCacheConfig::default(), provider);

        let metadata = CacheMetadata::default();
        cache
            .put("Query 1", "Response 1", metadata.clone())
            .await
            .unwrap();
        cache.put("Query 2", "Response 2", metadata).await.unwrap();

        let stats = cache.stats().await;
        assert_eq!(stats.total_entries, 2);
    }

    #[test]
    fn test_cosine_similarity() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 0.001);

        let c = vec![0.0, 1.0, 0.0];
        assert!((cosine_similarity(&a, &c)).abs() < 0.001);
    }
}
