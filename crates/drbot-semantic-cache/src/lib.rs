//! Semantic caching for similar queries - massive cost savings.
//!
//! This crate provides:
//! - Semantic similarity matching
//! - Response caching
//! - Cache invalidation
//! - Cost tracking

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Cache errors.
#[derive(Debug, Error)]
pub enum CacheError {
    #[error("Cache operation failed: {0}")]
    OperationFailed(String),

    #[error("Embedding failed: {0}")]
    EmbeddingFailed(String),

    #[error("Entry not found: {0}")]
    NotFound(String),
}

/// Result type for cache operations.
pub type Result<T> = std::result::Result<T, CacheError>;

/// A cached entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry {
    /// Entry identifier.
    pub id: String,
    /// Original query.
    pub query: String,
    /// Query embedding.
    pub embedding: Vec<f32>,
    /// Cached response.
    pub response: String,
    /// Model used.
    pub model: String,
    /// Created timestamp.
    pub created_at: DateTime<Utc>,
    /// Last accessed.
    pub last_accessed: DateTime<Utc>,
    /// Access count.
    pub access_count: usize,
    /// Time-to-live in seconds.
    pub ttl_secs: u64,
    /// Metadata.
    pub metadata: HashMap<String, String>,
}

/// Cache lookup result.
#[derive(Debug, Clone)]
pub enum CacheLookup {
    /// Exact match found.
    Hit(CacheEntry),
    /// Semantically similar match found.
    SimilarHit { entry: CacheEntry, similarity: f64 },
    /// No match found.
    Miss,
}

/// Cache configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    /// Maximum entries.
    pub max_entries: usize,
    /// Default TTL in seconds.
    pub default_ttl_secs: u64,
    /// Similarity threshold (0-1).
    pub similarity_threshold: f64,
    /// Enable semantic matching.
    pub semantic_matching: bool,
    /// Eviction policy.
    pub eviction_policy: EvictionPolicy,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            max_entries: 10000,
            default_ttl_secs: 3600,
            similarity_threshold: 0.92,
            semantic_matching: true,
            eviction_policy: EvictionPolicy::LRU,
        }
    }
}

/// Eviction policies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EvictionPolicy {
    /// Least Recently Used.
    LRU,
    /// Least Frequently Used.
    LFU,
    /// First In First Out.
    FIFO,
    /// Time-based expiry only.
    TTL,
}

/// Provider for embeddings.
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    /// Generate embedding for text.
    async fn embed(&self, text: &str) -> Result<Vec<f32>>;

    /// Generate embeddings for multiple texts.
    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>>;
}

/// Cache statistics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CacheStats {
    /// Total lookups.
    pub total_lookups: usize,
    /// Exact hits.
    pub exact_hits: usize,
    /// Semantic hits.
    pub semantic_hits: usize,
    /// Misses.
    pub misses: usize,
    /// Current entry count.
    pub entry_count: usize,
    /// Estimated cost savings.
    pub cost_savings: f64,
    /// Hit rate.
    pub hit_rate: f64,
}

/// The semantic cache.
pub struct SemanticCache {
    /// Embedding provider.
    embedder: Arc<dyn EmbeddingProvider>,
    /// Cache entries.
    entries: Arc<RwLock<HashMap<String, CacheEntry>>>,
    /// Configuration.
    config: CacheConfig,
    /// Statistics.
    stats: Arc<RwLock<CacheStats>>,
    /// Cost per query (for savings calculation).
    cost_per_query: f64,
}

impl SemanticCache {
    /// Create a new semantic cache.
    pub fn new(embedder: Arc<dyn EmbeddingProvider>, config: CacheConfig) -> Self {
        Self {
            embedder,
            entries: Arc::new(RwLock::new(HashMap::new())),
            config,
            stats: Arc::new(RwLock::new(CacheStats::default())),
            cost_per_query: 0.01, // Default $0.01 per query
        }
    }

    /// Set cost per query for savings calculation.
    pub fn with_cost_per_query(mut self, cost: f64) -> Self {
        self.cost_per_query = cost;
        self
    }

    /// Look up a query in the cache.
    pub async fn lookup(&self, query: &str) -> Result<CacheLookup> {
        let mut stats = self.stats.write().await;
        stats.total_lookups += 1;

        let entries = self.entries.read().await;
        let now = Utc::now();

        // Check for exact match first
        let exact_match = entries
            .values()
            .find(|entry| {
                if entry.query == query {
                    let expires_at = entry.created_at + Duration::seconds(entry.ttl_secs as i64);
                    now < expires_at
                } else {
                    false
                }
            })
            .cloned();

        if let Some(entry) = exact_match {
            stats.exact_hits += 1;
            stats.cost_savings += self.cost_per_query;
            stats.hit_rate =
                (stats.exact_hits + stats.semantic_hits) as f64 / stats.total_lookups as f64;
            drop(stats);
            drop(entries);

            // Update access time
            self.touch(&entry.id).await;
            return Ok(CacheLookup::Hit(entry));
        }
        drop(entries);

        // Semantic matching if enabled
        if self.config.semantic_matching {
            let query_embedding = self.embedder.embed(query).await?;
            let entries = self.entries.read().await;

            let mut best_match: Option<(CacheEntry, f64)> = None;

            for entry in entries.values() {
                let expires_at = entry.created_at + Duration::seconds(entry.ttl_secs as i64);
                if now >= expires_at {
                    continue;
                }

                let similarity = cosine_similarity(&query_embedding, &entry.embedding);
                if similarity >= self.config.similarity_threshold {
                    if best_match.is_none() || similarity > best_match.as_ref().unwrap().1 {
                        best_match = Some((entry.clone(), similarity));
                    }
                }
            }
            drop(entries);

            if let Some((entry, similarity)) = best_match {
                stats.semantic_hits += 1;
                stats.cost_savings += self.cost_per_query;
                stats.hit_rate =
                    (stats.exact_hits + stats.semantic_hits) as f64 / stats.total_lookups as f64;
                drop(stats);

                self.touch(&entry.id).await;
                return Ok(CacheLookup::SimilarHit { entry, similarity });
            }
        }

        stats.misses += 1;
        stats.hit_rate =
            (stats.exact_hits + stats.semantic_hits) as f64 / stats.total_lookups as f64;

        Ok(CacheLookup::Miss)
    }

    /// Store a response in the cache.
    pub async fn store(
        &self,
        query: &str,
        response: &str,
        model: &str,
        metadata: Option<HashMap<String, String>>,
    ) -> Result<String> {
        let embedding = self.embedder.embed(query).await?;

        let entry = CacheEntry {
            id: Uuid::new_v4().to_string(),
            query: query.to_string(),
            embedding,
            response: response.to_string(),
            model: model.to_string(),
            created_at: Utc::now(),
            last_accessed: Utc::now(),
            access_count: 0,
            ttl_secs: self.config.default_ttl_secs,
            metadata: metadata.unwrap_or_default(),
        };

        let id = entry.id.clone();

        let mut entries = self.entries.write().await;

        // Evict if necessary
        if entries.len() >= self.config.max_entries {
            self.evict_one(&mut entries).await;
        }

        entries.insert(id.clone(), entry);

        // Update stats
        let mut stats = self.stats.write().await;
        stats.entry_count = entries.len();

        Ok(id)
    }

    /// Update access time for an entry.
    async fn touch(&self, id: &str) {
        let mut entries = self.entries.write().await;
        if let Some(entry) = entries.get_mut(id) {
            entry.last_accessed = Utc::now();
            entry.access_count += 1;
        }
    }

    /// Evict one entry based on policy.
    async fn evict_one(&self, entries: &mut HashMap<String, CacheEntry>) {
        let to_remove = match self.config.eviction_policy {
            EvictionPolicy::LRU => entries
                .iter()
                .min_by_key(|(_, e)| e.last_accessed)
                .map(|(id, _)| id.clone()),
            EvictionPolicy::LFU => entries
                .iter()
                .min_by_key(|(_, e)| e.access_count)
                .map(|(id, _)| id.clone()),
            EvictionPolicy::FIFO => entries
                .iter()
                .min_by_key(|(_, e)| e.created_at)
                .map(|(id, _)| id.clone()),
            EvictionPolicy::TTL => {
                let now = Utc::now();
                entries
                    .iter()
                    .filter(|(_, e)| e.created_at + Duration::seconds(e.ttl_secs as i64) < now)
                    .map(|(id, _)| id.clone())
                    .next()
            }
        };

        if let Some(id) = to_remove {
            entries.remove(&id);
        }
    }

    /// Invalidate an entry.
    pub async fn invalidate(&self, id: &str) -> Result<()> {
        let mut entries = self.entries.write().await;
        entries.remove(id);

        let mut stats = self.stats.write().await;
        stats.entry_count = entries.len();

        Ok(())
    }

    /// Invalidate entries matching a pattern.
    pub async fn invalidate_pattern(&self, pattern: &str) -> Result<usize> {
        let mut entries = self.entries.write().await;
        let pattern_lower = pattern.to_lowercase();

        let to_remove: Vec<_> = entries
            .iter()
            .filter(|(_, e)| e.query.to_lowercase().contains(&pattern_lower))
            .map(|(id, _)| id.clone())
            .collect();

        let count = to_remove.len();
        for id in to_remove {
            entries.remove(&id);
        }

        let mut stats = self.stats.write().await;
        stats.entry_count = entries.len();

        Ok(count)
    }

    /// Clear all entries.
    pub async fn clear(&self) {
        let mut entries = self.entries.write().await;
        entries.clear();

        let mut stats = self.stats.write().await;
        stats.entry_count = 0;
    }

    /// Get statistics.
    pub async fn stats(&self) -> CacheStats {
        let stats = self.stats.read().await;
        stats.clone()
    }

    /// Cleanup expired entries.
    pub async fn cleanup_expired(&self) -> usize {
        let mut entries = self.entries.write().await;
        let now = Utc::now();

        let expired: Vec<_> = entries
            .iter()
            .filter(|(_, e)| e.created_at + Duration::seconds(e.ttl_secs as i64) < now)
            .map(|(id, _)| id.clone())
            .collect();

        let count = expired.len();
        for id in expired {
            entries.remove(&id);
        }

        let mut stats = self.stats.write().await;
        stats.entry_count = entries.len();

        count
    }
}

/// Calculate cosine similarity between two vectors.
fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
    if a.len() != b.len() {
        return 0.0;
    }

    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let mag_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let mag_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if mag_a == 0.0 || mag_b == 0.0 {
        return 0.0;
    }

    (dot / (mag_a * mag_b)) as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockEmbedder;

    #[async_trait]
    impl EmbeddingProvider for MockEmbedder {
        async fn embed(&self, text: &str) -> Result<Vec<f32>> {
            // Simple hash-based mock embedding
            let hash = text.bytes().fold(0u32, |acc, b| acc.wrapping_add(b as u32));
            Ok(vec![
                (hash % 100) as f32 / 100.0,
                ((hash >> 8) % 100) as f32 / 100.0,
                ((hash >> 16) % 100) as f32 / 100.0,
                ((hash >> 24) % 100) as f32 / 100.0,
            ])
        }

        async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
            let mut embeddings = Vec::new();
            for text in texts {
                embeddings.push(self.embed(text).await?);
            }
            Ok(embeddings)
        }
    }

    #[tokio::test]
    async fn test_store_and_lookup() {
        let embedder = Arc::new(MockEmbedder);
        let cache = SemanticCache::new(embedder, CacheConfig::default());

        cache
            .store(
                "What is Rust?",
                "Rust is a systems programming language.",
                "gpt-4",
                None,
            )
            .await
            .unwrap();

        let result = cache.lookup("What is Rust?").await.unwrap();
        assert!(matches!(result, CacheLookup::Hit(_)));
    }

    #[tokio::test]
    async fn test_cache_miss() {
        let embedder = Arc::new(MockEmbedder);
        let cache = SemanticCache::new(embedder, CacheConfig::default());

        let result = cache.lookup("Unknown query").await.unwrap();
        assert!(matches!(result, CacheLookup::Miss));
    }

    #[tokio::test]
    async fn test_invalidate() {
        let embedder = Arc::new(MockEmbedder);
        let cache = SemanticCache::new(embedder, CacheConfig::default());

        let id = cache
            .store("Test query", "Test response", "gpt-4", None)
            .await
            .unwrap();
        cache.invalidate(&id).await.unwrap();

        let result = cache.lookup("Test query").await.unwrap();
        assert!(matches!(result, CacheLookup::Miss));
    }

    #[tokio::test]
    async fn test_stats() {
        let embedder = Arc::new(MockEmbedder);
        let cache = SemanticCache::new(embedder, CacheConfig::default());

        cache
            .store("Query 1", "Response 1", "gpt-4", None)
            .await
            .unwrap();
        cache.lookup("Query 1").await.unwrap();
        cache.lookup("Query 2").await.unwrap();

        let stats = cache.stats().await;
        assert_eq!(stats.total_lookups, 2);
        assert!(stats.exact_hits >= 1);
    }

    #[tokio::test]
    async fn test_clear() {
        let embedder = Arc::new(MockEmbedder);
        let cache = SemanticCache::new(embedder, CacheConfig::default());

        cache
            .store("Query", "Response", "gpt-4", None)
            .await
            .unwrap();
        cache.clear().await;

        let stats = cache.stats().await;
        assert_eq!(stats.entry_count, 0);
    }

    #[test]
    fn test_cosine_similarity() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 0.001);

        let c = vec![0.0, 1.0, 0.0];
        assert!(cosine_similarity(&a, &c).abs() < 0.001);
    }
}
