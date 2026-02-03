//! Memory consolidation for drbot.
//!
//! Efficient long-term memory storage through consolidation.
//!
//! # Features
//!
//! - Automatic memory compression
//! - Importance-based retention
//! - Semantic clustering
//! - Progressive summarization

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Consolidation result type.
pub type Result<T> = std::result::Result<T, ConsolidationError>;

/// Consolidation errors.
#[derive(Debug, thiserror::Error)]
pub enum ConsolidationError {
    #[error("Memory not found: {0}")]
    MemoryNotFound(Uuid),
    #[error("Consolidation failed: {0}")]
    Failed(String),
    #[error("Cluster not found: {0}")]
    ClusterNotFound(Uuid),
}

/// A memory to be consolidated.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Memory {
    /// Memory ID.
    pub id: Uuid,
    /// Memory content.
    pub content: String,
    /// Memory type.
    pub memory_type: MemoryType,
    /// Importance score (0-1).
    pub importance: f32,
    /// Access count.
    pub access_count: u64,
    /// Last accessed.
    pub last_accessed: DateTime<Utc>,
    /// Created at.
    pub created_at: DateTime<Utc>,
    /// Embedding (if computed).
    pub embedding: Option<Vec<f32>>,
    /// Source.
    pub source: String,
    /// Tags.
    pub tags: Vec<String>,
    /// Related memory IDs.
    pub related: Vec<Uuid>,
}

impl Memory {
    /// Create a new memory.
    pub fn new(content: &str, memory_type: MemoryType) -> Self {
        Self {
            id: Uuid::new_v4(),
            content: content.to_string(),
            memory_type,
            importance: 0.5,
            access_count: 0,
            last_accessed: Utc::now(),
            created_at: Utc::now(),
            embedding: None,
            source: String::new(),
            tags: Vec::new(),
            related: Vec::new(),
        }
    }

    /// Set importance.
    pub fn with_importance(mut self, importance: f32) -> Self {
        self.importance = importance.clamp(0.0, 1.0);
        self
    }

    /// Add tags.
    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    /// Calculate retention score based on importance and recency.
    pub fn retention_score(&self) -> f32 {
        let age_days = Utc::now().signed_duration_since(self.created_at).num_days() as f32;
        let recency_factor = 1.0 / (1.0 + age_days / 30.0);

        let access_factor = (self.access_count as f32).min(10.0) / 10.0;

        self.importance * 0.5 + recency_factor * 0.3 + access_factor * 0.2
    }

    /// Record access.
    pub fn access(&mut self) {
        self.access_count += 1;
        self.last_accessed = Utc::now();
    }
}

/// Memory types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryType {
    /// Factual information.
    Fact,
    /// User preference.
    Preference,
    /// Conversation summary.
    Summary,
    /// Learned pattern.
    Pattern,
    /// Task result.
    Task,
    /// Error/lesson learned.
    Lesson,
    /// Other.
    Other,
}

/// A cluster of related memories.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryCluster {
    /// Cluster ID.
    pub id: Uuid,
    /// Cluster name/topic.
    pub name: String,
    /// Member memory IDs.
    pub members: Vec<Uuid>,
    /// Consolidated summary.
    pub summary: Option<String>,
    /// Centroid embedding.
    pub centroid: Option<Vec<f32>>,
    /// Importance.
    pub importance: f32,
    /// Created at.
    pub created_at: DateTime<Utc>,
    /// Updated at.
    pub updated_at: DateTime<Utc>,
}

impl MemoryCluster {
    /// Create a new cluster.
    pub fn new(name: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.to_string(),
            members: Vec::new(),
            summary: None,
            centroid: None,
            importance: 0.5,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    /// Add a member.
    pub fn add_member(&mut self, memory_id: Uuid) {
        if !self.members.contains(&memory_id) {
            self.members.push(memory_id);
            self.updated_at = Utc::now();
        }
    }
}

/// Consolidation configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsolidationConfig {
    /// Enable automatic consolidation.
    pub auto_consolidate: bool,
    /// Consolidation interval in hours.
    pub interval_hours: u64,
    /// Minimum memories before consolidation.
    pub min_memories: usize,
    /// Retention threshold (below this gets consolidated).
    pub retention_threshold: f32,
    /// Maximum memories to keep.
    pub max_memories: usize,
    /// Cluster similarity threshold.
    pub cluster_threshold: f32,
}

impl Default for ConsolidationConfig {
    fn default() -> Self {
        Self {
            auto_consolidate: true,
            interval_hours: 24,
            min_memories: 100,
            retention_threshold: 0.3,
            max_memories: 10000,
            cluster_threshold: 0.8,
        }
    }
}

/// Consolidation result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsolidationResult {
    /// Memories consolidated.
    pub memories_consolidated: usize,
    /// Memories removed.
    pub memories_removed: usize,
    /// Clusters created.
    pub clusters_created: usize,
    /// Clusters updated.
    pub clusters_updated: usize,
    /// Space saved (estimated bytes).
    pub space_saved: usize,
    /// Duration in ms.
    pub duration_ms: u64,
}

/// Trait for memory summarizers.
#[async_trait]
pub trait Summarizer: Send + Sync {
    /// Summarize multiple memories into one.
    async fn summarize(&self, memories: &[Memory]) -> Result<String>;
}

/// Trait for embedding providers.
#[async_trait]
pub trait Embedder: Send + Sync {
    /// Generate embedding for text.
    async fn embed(&self, text: &str) -> Result<Vec<f32>>;
}

/// Memory consolidation engine.
pub struct ConsolidationEngine<S: Summarizer, E: Embedder> {
    config: ConsolidationConfig,
    summarizer: S,
    embedder: E,
    memories: Arc<RwLock<HashMap<Uuid, Memory>>>,
    clusters: Arc<RwLock<HashMap<Uuid, MemoryCluster>>>,
    last_consolidation: Arc<RwLock<Option<DateTime<Utc>>>>,
}

impl<S: Summarizer, E: Embedder> ConsolidationEngine<S, E> {
    /// Create a new consolidation engine.
    pub fn new(config: ConsolidationConfig, summarizer: S, embedder: E) -> Self {
        Self {
            config,
            summarizer,
            embedder,
            memories: Arc::new(RwLock::new(HashMap::new())),
            clusters: Arc::new(RwLock::new(HashMap::new())),
            last_consolidation: Arc::new(RwLock::new(None)),
        }
    }

    /// Add a memory.
    pub async fn add_memory(&self, mut memory: Memory) -> Result<Uuid> {
        // Generate embedding
        let embedding = self.embedder.embed(&memory.content).await?;
        memory.embedding = Some(embedding);

        let id = memory.id;
        self.memories.write().await.insert(id, memory);

        // Check if consolidation is needed
        if self.config.auto_consolidate {
            self.maybe_consolidate().await?;
        }

        Ok(id)
    }

    /// Get a memory.
    pub async fn get_memory(&self, id: Uuid) -> Option<Memory> {
        let mut memories = self.memories.write().await;
        if let Some(memory) = memories.get_mut(&id) {
            memory.access();
            return Some(memory.clone());
        }
        None
    }

    /// Run consolidation if needed.
    async fn maybe_consolidate(&self) -> Result<()> {
        let memories = self.memories.read().await;
        if memories.len() < self.config.min_memories {
            return Ok(());
        }

        let last = self.last_consolidation.read().await;
        if let Some(last_time) = *last {
            if Utc::now() - last_time < Duration::hours(self.config.interval_hours as i64) {
                return Ok(());
            }
        }

        drop(memories);
        drop(last);

        self.consolidate().await?;
        Ok(())
    }

    /// Run consolidation.
    pub async fn consolidate(&self) -> Result<ConsolidationResult> {
        let start = std::time::Instant::now();
        let mut memories_consolidated = 0;
        let mut memories_removed = 0;
        let mut clusters_created = 0;
        let mut clusters_updated = 0;
        let mut space_saved = 0;

        let mut memories = self.memories.write().await;

        // Sort by retention score
        let mut sorted: Vec<_> = memories.values().cloned().collect();
        sorted.sort_by(|a, b| {
            a.retention_score()
                .partial_cmp(&b.retention_score())
                .unwrap()
        });

        // Find low-retention memories to consolidate
        let to_consolidate: Vec<_> = sorted
            .iter()
            .filter(|m| m.retention_score() < self.config.retention_threshold)
            .cloned()
            .collect();

        if !to_consolidate.is_empty() {
            // Group by type and summarize
            let mut by_type: HashMap<MemoryType, Vec<Memory>> = HashMap::new();
            for memory in to_consolidate {
                by_type.entry(memory.memory_type).or_default().push(memory);
            }

            for (memory_type, type_memories) in by_type {
                if type_memories.len() >= 3 {
                    // Summarize into a cluster
                    let summary = self.summarizer.summarize(&type_memories).await?;
                    let space_before: usize = type_memories.iter().map(|m| m.content.len()).sum();

                    // Create consolidated memory
                    let consolidated = Memory::new(&summary, memory_type).with_importance(
                        type_memories.iter().map(|m| m.importance).sum::<f32>()
                            / type_memories.len() as f32,
                    );

                    // Remove old memories
                    for old in &type_memories {
                        memories.remove(&old.id);
                        memories_removed += 1;
                    }

                    // Add consolidated memory
                    memories.insert(consolidated.id, consolidated);
                    memories_consolidated += 1;

                    space_saved += space_before.saturating_sub(summary.len());

                    // Create/update cluster
                    let mut cluster = MemoryCluster::new(&format!("{:?} cluster", memory_type));
                    cluster.summary = Some(summary);
                    self.clusters.write().await.insert(cluster.id, cluster);
                    clusters_created += 1;
                }
            }
        }

        // Enforce max memories limit
        while memories.len() > self.config.max_memories {
            if let Some(lowest) = sorted.first() {
                memories.remove(&lowest.id);
                memories_removed += 1;
                sorted.remove(0);
            } else {
                break;
            }
        }

        // Update clusters with semantic grouping
        let all_memories: Vec<_> = memories.values().cloned().collect();
        drop(memories);

        for memory in all_memories {
            if let Some(ref embedding) = memory.embedding {
                let mut clusters = self.clusters.write().await;
                let mut best_cluster: Option<(Uuid, f32)> = None;

                for cluster in clusters.values() {
                    if let Some(ref centroid) = cluster.centroid {
                        let similarity = cosine_similarity(embedding, centroid);
                        if similarity >= self.config.cluster_threshold {
                            if best_cluster.is_none() || similarity > best_cluster.unwrap().1 {
                                best_cluster = Some((cluster.id, similarity));
                            }
                        }
                    }
                }

                if let Some((cluster_id, _)) = best_cluster {
                    if let Some(cluster) = clusters.get_mut(&cluster_id) {
                        cluster.add_member(memory.id);
                        clusters_updated += 1;
                    }
                }
            }
        }

        *self.last_consolidation.write().await = Some(Utc::now());

        Ok(ConsolidationResult {
            memories_consolidated,
            memories_removed,
            clusters_created,
            clusters_updated,
            space_saved,
            duration_ms: start.elapsed().as_millis() as u64,
        })
    }

    /// Get all memories.
    pub async fn list_memories(&self) -> Vec<Memory> {
        self.memories.read().await.values().cloned().collect()
    }

    /// Get all clusters.
    pub async fn list_clusters(&self) -> Vec<MemoryCluster> {
        self.clusters.read().await.values().cloned().collect()
    }

    /// Get consolidation stats.
    pub async fn stats(&self) -> ConsolidationStats {
        let memories = self.memories.read().await;
        let clusters = self.clusters.read().await;

        let total_size: usize = memories.values().map(|m| m.content.len()).sum();
        let avg_importance: f32 = if !memories.is_empty() {
            memories.values().map(|m| m.importance).sum::<f32>() / memories.len() as f32
        } else {
            0.0
        };

        ConsolidationStats {
            total_memories: memories.len(),
            total_clusters: clusters.len(),
            total_size_bytes: total_size,
            avg_importance,
            last_consolidation: *self.last_consolidation.read().await,
        }
    }
}

/// Calculate cosine similarity.
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

/// Consolidation statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsolidationStats {
    pub total_memories: usize,
    pub total_clusters: usize,
    pub total_size_bytes: usize,
    pub avg_importance: f32,
    pub last_consolidation: Option<DateTime<Utc>>,
}

/// Simple summarizer for testing.
pub struct SimpleSummarizer;

#[async_trait]
impl Summarizer for SimpleSummarizer {
    async fn summarize(&self, memories: &[Memory]) -> Result<String> {
        let combined: Vec<_> = memories.iter().map(|m| m.content.as_str()).collect();
        Ok(format!(
            "Summary of {} memories: {}",
            memories.len(),
            combined.join("; ")
        ))
    }
}

/// Simple embedder for testing.
pub struct SimpleEmbedder {
    dim: usize,
}

impl SimpleEmbedder {
    pub fn new(dim: usize) -> Self {
        Self { dim }
    }
}

#[async_trait]
impl Embedder for SimpleEmbedder {
    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let mut embedding = vec![0.0f32; self.dim];
        for (i, c) in text.chars().enumerate() {
            embedding[i % self.dim] += (c as u32 as f32) / 1000.0;
        }
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
    async fn test_memory_creation() {
        let engine = ConsolidationEngine::new(
            ConsolidationConfig::default(),
            SimpleSummarizer,
            SimpleEmbedder::new(64),
        );

        let memory = Memory::new("Test memory content", MemoryType::Fact).with_importance(0.8);
        let id = engine.add_memory(memory).await.unwrap();

        let retrieved = engine.get_memory(id).await.unwrap();
        assert_eq!(retrieved.content, "Test memory content");
        assert_eq!(retrieved.access_count, 1);
    }

    #[test]
    fn test_retention_score() {
        let memory = Memory::new("Test", MemoryType::Fact).with_importance(1.0);
        let score = memory.retention_score();
        assert!(score > 0.5);
    }

    #[tokio::test]
    async fn test_consolidation() {
        let config = ConsolidationConfig {
            min_memories: 3,
            retention_threshold: 0.8,
            ..Default::default()
        };

        let engine = ConsolidationEngine::new(config, SimpleSummarizer, SimpleEmbedder::new(64));

        // Add several low-importance memories
        for i in 0..5 {
            let memory =
                Memory::new(&format!("Memory {}", i), MemoryType::Fact).with_importance(0.1);
            engine.add_memory(memory).await.unwrap();
        }

        let result = engine.consolidate().await.unwrap();
        assert!(result.memories_consolidated > 0 || result.memories_removed > 0);
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
