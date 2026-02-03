//! Long-term memory for drbot.
//!
//! Persistent memory across months and years.
//!
//! # Features
//!
//! - Memory consolidation
//! - Importance scoring
//! - Decay and reinforcement
//! - Semantic retrieval
//! - Memory compression

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Long memory result type.
pub type Result<T> = std::result::Result<T, MemoryError>;

/// Memory errors.
#[derive(Debug, thiserror::Error)]
pub enum MemoryError {
    #[error("Memory not found: {0}")]
    NotFound(String),
    #[error("Storage error: {0}")]
    StorageError(String),
    #[error("Compression failed: {0}")]
    CompressionFailed(String),
}

/// Memory type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryType {
    Fact,
    Event,
    Preference,
    Skill,
    Relationship,
    Emotion,
    Conversation,
    Custom,
}

/// Memory item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Memory {
    /// Memory ID.
    pub id: Uuid,
    /// Memory type.
    pub memory_type: MemoryType,
    /// Content.
    pub content: String,
    /// Summary (for compressed memories).
    pub summary: Option<String>,
    /// Importance score (0-1).
    pub importance: f64,
    /// Strength (decays over time).
    pub strength: f64,
    /// Access count.
    pub access_count: u64,
    /// Created at.
    pub created_at: DateTime<Utc>,
    /// Last accessed.
    pub last_accessed: DateTime<Utc>,
    /// Last reinforced.
    pub last_reinforced: DateTime<Utc>,
    /// Source.
    pub source: Option<String>,
    /// Tags.
    pub tags: Vec<String>,
    /// Related memories.
    pub related: Vec<Uuid>,
    /// Embedding.
    pub embedding: Option<Vec<f32>>,
    /// Is consolidated.
    pub consolidated: bool,
}

impl Memory {
    /// Create a new memory.
    pub fn new(memory_type: MemoryType, content: &str) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            memory_type,
            content: content.to_string(),
            summary: None,
            importance: 0.5,
            strength: 1.0,
            access_count: 0,
            created_at: now,
            last_accessed: now,
            last_reinforced: now,
            source: None,
            tags: Vec::new(),
            related: Vec::new(),
            embedding: None,
            consolidated: false,
        }
    }

    /// Check if memory should decay.
    pub fn should_decay(&self) -> bool {
        let days_since_access = (Utc::now() - self.last_accessed).num_days();
        days_since_access > 0
    }

    /// Calculate effective strength (with decay).
    pub fn effective_strength(&self) -> f64 {
        let days = (Utc::now() - self.last_reinforced).num_days() as f64;
        // Exponential decay with importance factor
        self.strength * (self.importance + 0.1).powf(0.1) * (-days / 30.0).exp()
    }

    /// Apply decay.
    pub fn apply_decay(&mut self, decay_rate: f64) {
        let days = (Utc::now() - self.last_reinforced).num_days() as f64;
        self.strength *= (-decay_rate * days / 30.0).exp();
        self.strength = self.strength.max(0.01); // Never fully forget
    }

    /// Reinforce memory.
    pub fn reinforce(&mut self, boost: f64) {
        self.strength = (self.strength + boost).min(1.0);
        self.access_count += 1;
        self.last_accessed = Utc::now();
        self.last_reinforced = Utc::now();
    }
}

/// Memory query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryQuery {
    /// Search text.
    pub text: Option<String>,
    /// Memory types.
    pub types: Option<Vec<MemoryType>>,
    /// Tags.
    pub tags: Option<Vec<String>>,
    /// Minimum importance.
    pub min_importance: Option<f64>,
    /// Minimum strength.
    pub min_strength: Option<f64>,
    /// Date range.
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    /// Limit.
    pub limit: usize,
    /// Semantic search.
    pub semantic: bool,
}

impl Default for MemoryQuery {
    fn default() -> Self {
        Self {
            text: None,
            types: None,
            tags: None,
            min_importance: None,
            min_strength: None,
            from: None,
            to: None,
            limit: 20,
            semantic: true,
        }
    }
}

impl MemoryQuery {
    pub fn text(mut self, text: &str) -> Self {
        self.text = Some(text.to_string());
        self
    }

    pub fn types(mut self, types: Vec<MemoryType>) -> Self {
        self.types = Some(types);
        self
    }

    pub fn limit(mut self, limit: usize) -> Self {
        self.limit = limit;
        self
    }
}

/// Consolidated memory (summary of related memories).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsolidatedMemory {
    /// Consolidation ID.
    pub id: Uuid,
    /// Summary.
    pub summary: String,
    /// Source memory IDs.
    pub source_ids: Vec<Uuid>,
    /// Combined importance.
    pub importance: f64,
    /// Created at.
    pub created_at: DateTime<Utc>,
    /// Memory type.
    pub memory_type: MemoryType,
}

/// Memory configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConfig {
    /// Decay rate.
    pub decay_rate: f64,
    /// Consolidation threshold (number of related memories).
    pub consolidation_threshold: usize,
    /// Minimum strength to keep.
    pub min_strength_threshold: f64,
    /// Reinforcement boost.
    pub reinforcement_boost: f64,
    /// Maximum memories.
    pub max_memories: usize,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            decay_rate: 0.05,
            consolidation_threshold: 5,
            min_strength_threshold: 0.05,
            reinforcement_boost: 0.2,
            max_memories: 100_000,
        }
    }
}

/// Trait for memory storage.
#[async_trait]
pub trait MemoryStorage: Send + Sync {
    /// Save memory.
    async fn save(&self, memory: &Memory) -> Result<()>;
    /// Load memory.
    async fn load(&self, id: Uuid) -> Result<Option<Memory>>;
    /// Delete memory.
    async fn delete(&self, id: Uuid) -> Result<()>;
    /// Query memories.
    async fn query(&self, query: &MemoryQuery) -> Result<Vec<Memory>>;
    /// Get all memories.
    async fn get_all(&self) -> Result<Vec<Memory>>;
}

/// Trait for embedding provider.
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    /// Generate embedding.
    async fn embed(&self, text: &str) -> Result<Vec<f32>>;
    /// Calculate similarity.
    fn similarity(&self, a: &[f32], b: &[f32]) -> f32;
}

/// Long-term memory engine.
pub struct LongTermMemory<S: MemoryStorage, E: EmbeddingProvider> {
    config: MemoryConfig,
    storage: S,
    embedder: E,
    cache: Arc<RwLock<HashMap<Uuid, Memory>>>,
    consolidations: Arc<RwLock<Vec<ConsolidatedMemory>>>,
}

impl<S: MemoryStorage, E: EmbeddingProvider> LongTermMemory<S, E> {
    /// Create a new long-term memory.
    pub fn new(config: MemoryConfig, storage: S, embedder: E) -> Self {
        Self {
            config,
            storage,
            embedder,
            cache: Arc::new(RwLock::new(HashMap::new())),
            consolidations: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Store a memory.
    pub async fn remember(&self, mut memory: Memory) -> Result<Uuid> {
        // Generate embedding
        let embedding = self.embedder.embed(&memory.content).await?;
        memory.embedding = Some(embedding);

        // Calculate importance based on content
        memory.importance = self.calculate_importance(&memory);

        // Store
        let id = memory.id;
        self.storage.save(&memory).await?;
        self.cache.write().await.insert(id, memory);

        Ok(id)
    }

    fn calculate_importance(&self, memory: &Memory) -> f64 {
        let mut importance: f64 = 0.5;

        // Longer memories tend to be more important
        let words = memory.content.split_whitespace().count();
        if words > 50 {
            importance += 0.1;
        }

        // Type-based importance
        importance += match memory.memory_type {
            MemoryType::Fact => 0.2,
            MemoryType::Preference => 0.3,
            MemoryType::Relationship => 0.25,
            MemoryType::Skill => 0.2,
            _ => 0.0,
        };

        importance.min(1.0)
    }

    /// Recall memories.
    pub async fn recall(&self, query: MemoryQuery) -> Result<Vec<Memory>> {
        let mut memories = self.storage.query(&query).await?;

        // Semantic search if enabled and embedding is available
        if query.semantic {
            if let Some(ref text) = query.text {
                let query_embedding = self.embedder.embed(text).await?;

                // Score by semantic similarity
                for memory in &mut memories {
                    if let Some(ref mem_embedding) = memory.embedding {
                        let similarity = self.embedder.similarity(&query_embedding, mem_embedding);
                        memory.strength *= similarity as f64;
                    }
                }

                // Re-sort by effective strength
                memories.sort_by(|a, b| {
                    b.effective_strength()
                        .partial_cmp(&a.effective_strength())
                        .unwrap()
                });
            }
        }

        // Reinforce accessed memories
        for memory in &mut memories {
            memory.reinforce(self.config.reinforcement_boost);
            self.storage.save(memory).await?;
        }

        Ok(memories.into_iter().take(query.limit).collect())
    }

    /// Reinforce a specific memory.
    pub async fn reinforce(&self, id: Uuid) -> Result<()> {
        if let Some(mut memory) = self.storage.load(id).await? {
            memory.reinforce(self.config.reinforcement_boost);
            self.storage.save(&memory).await?;
        }
        Ok(())
    }

    /// Forget a memory.
    pub async fn forget(&self, id: Uuid) -> Result<()> {
        self.storage.delete(id).await?;
        self.cache.write().await.remove(&id);
        Ok(())
    }

    /// Apply decay to all memories.
    pub async fn apply_decay(&self) -> Result<usize> {
        let memories = self.storage.get_all().await?;
        let mut decayed = 0;

        for mut memory in memories {
            if memory.should_decay() {
                memory.apply_decay(self.config.decay_rate);

                if memory.strength < self.config.min_strength_threshold {
                    // Archive or delete weak memories
                    self.storage.delete(memory.id).await?;
                } else {
                    self.storage.save(&memory).await?;
                }
                decayed += 1;
            }
        }

        Ok(decayed)
    }

    /// Consolidate related memories.
    pub async fn consolidate(&self) -> Result<Vec<ConsolidatedMemory>> {
        let memories = self.storage.get_all().await?;
        let mut new_consolidations = Vec::new();

        // Group by type
        let mut by_type: HashMap<MemoryType, Vec<&Memory>> = HashMap::new();
        for memory in &memories {
            by_type.entry(memory.memory_type).or_default().push(memory);
        }

        // Find clusters to consolidate
        for (memory_type, type_memories) in by_type {
            if type_memories.len() >= self.config.consolidation_threshold {
                // Find similar memories
                for i in 0..type_memories.len() {
                    let mut cluster = vec![type_memories[i].id];

                    for j in (i + 1)..type_memories.len() {
                        if let (Some(e1), Some(e2)) =
                            (&type_memories[i].embedding, &type_memories[j].embedding)
                        {
                            let sim = self.embedder.similarity(e1, e2);
                            if sim > 0.8 {
                                cluster.push(type_memories[j].id);
                            }
                        }
                    }

                    if cluster.len() >= self.config.consolidation_threshold {
                        // Create consolidated memory
                        let contents: Vec<&str> = cluster
                            .iter()
                            .filter_map(|id| type_memories.iter().find(|m| m.id == *id))
                            .map(|m| m.content.as_str())
                            .collect();

                        let summary = self.create_summary(&contents);
                        let avg_importance = cluster
                            .iter()
                            .filter_map(|id| type_memories.iter().find(|m| m.id == *id))
                            .map(|m| m.importance)
                            .sum::<f64>()
                            / cluster.len() as f64;

                        let consolidated = ConsolidatedMemory {
                            id: Uuid::new_v4(),
                            summary,
                            source_ids: cluster.clone(),
                            importance: avg_importance,
                            created_at: Utc::now(),
                            memory_type,
                        };

                        new_consolidations.push(consolidated);
                    }
                }
            }
        }

        // Store consolidations
        self.consolidations
            .write()
            .await
            .extend(new_consolidations.clone());

        Ok(new_consolidations)
    }

    fn create_summary(&self, contents: &[&str]) -> String {
        // Simple summary: first sentence of each + count
        let first_sentences: Vec<&str> = contents
            .iter()
            .map(|c| c.split('.').next().unwrap_or(c))
            .take(3)
            .collect();

        format!(
            "Combined from {} memories: {}",
            contents.len(),
            first_sentences.join("; ")
        )
    }

    /// Get context for AI.
    pub async fn get_context(&self, topic: &str, limit: usize) -> Result<String> {
        let query = MemoryQuery::default().text(topic).limit(limit);

        let memories = self.recall(query).await?;

        let context = memories
            .iter()
            .map(|m| {
                format!(
                    "- {} (importance: {:.1}, from {})",
                    m.summary.as_ref().unwrap_or(&m.content),
                    m.importance,
                    m.created_at.format("%Y-%m-%d")
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        Ok(if context.is_empty() {
            "No relevant memories found.".to_string()
        } else {
            format!("Relevant memories:\n{}", context)
        })
    }

    /// Get statistics.
    pub async fn stats(&self) -> Result<MemoryStats> {
        let memories = self.storage.get_all().await?;
        let consolidations = self.consolidations.read().await;

        let mut by_type: HashMap<MemoryType, usize> = HashMap::new();
        let mut total_strength = 0.0;

        for memory in &memories {
            *by_type.entry(memory.memory_type).or_insert(0) += 1;
            total_strength += memory.effective_strength();
        }

        let oldest = memories.iter().map(|m| m.created_at).min();
        let newest = memories.iter().map(|m| m.created_at).max();

        Ok(MemoryStats {
            total_memories: memories.len(),
            consolidated: consolidations.len(),
            by_type,
            avg_strength: if memories.is_empty() {
                0.0
            } else {
                total_strength / memories.len() as f64
            },
            oldest_memory: oldest,
            newest_memory: newest,
        })
    }
}

/// Memory statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryStats {
    pub total_memories: usize,
    pub consolidated: usize,
    pub by_type: HashMap<MemoryType, usize>,
    pub avg_strength: f64,
    pub oldest_memory: Option<DateTime<Utc>>,
    pub newest_memory: Option<DateTime<Utc>>,
}

/// In-memory storage for testing.
pub struct InMemoryStorage {
    memories: Arc<RwLock<HashMap<Uuid, Memory>>>,
}

impl InMemoryStorage {
    pub fn new() -> Self {
        Self {
            memories: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl Default for InMemoryStorage {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl MemoryStorage for InMemoryStorage {
    async fn save(&self, memory: &Memory) -> Result<()> {
        self.memories
            .write()
            .await
            .insert(memory.id, memory.clone());
        Ok(())
    }

    async fn load(&self, id: Uuid) -> Result<Option<Memory>> {
        Ok(self.memories.read().await.get(&id).cloned())
    }

    async fn delete(&self, id: Uuid) -> Result<()> {
        self.memories.write().await.remove(&id);
        Ok(())
    }

    async fn query(&self, query: &MemoryQuery) -> Result<Vec<Memory>> {
        let memories = self.memories.read().await;
        let mut results: Vec<_> = memories
            .values()
            .filter(|m| {
                // Type filter
                if let Some(ref types) = query.types {
                    if !types.contains(&m.memory_type) {
                        return false;
                    }
                }
                // Importance filter
                if let Some(min) = query.min_importance {
                    if m.importance < min {
                        return false;
                    }
                }
                // Text filter
                if let Some(ref text) = query.text {
                    if !m.content.to_lowercase().contains(&text.to_lowercase()) {
                        return false;
                    }
                }
                true
            })
            .cloned()
            .collect();

        results.sort_by(|a, b| {
            b.effective_strength()
                .partial_cmp(&a.effective_strength())
                .unwrap()
        });
        Ok(results)
    }

    async fn get_all(&self) -> Result<Vec<Memory>> {
        Ok(self.memories.read().await.values().cloned().collect())
    }
}

/// Simple embedding provider.
pub struct SimpleEmbedder;

#[async_trait]
impl EmbeddingProvider for SimpleEmbedder {
    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let mut embedding = vec![0.0f32; 128];
        for (i, word) in text.to_lowercase().split_whitespace().enumerate() {
            let hash = word.bytes().fold(0u32, |acc, b| acc.wrapping_add(b as u32));
            let idx = (hash as usize) % 128;
            embedding[idx] += 1.0 / (i + 1) as f32;
        }

        // Normalize
        let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for v in &mut embedding {
                *v /= norm;
            }
        }

        Ok(embedding)
    }

    fn similarity(&self, a: &[f32], b: &[f32]) -> f32 {
        a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_remember() {
        let mem = LongTermMemory::new(
            MemoryConfig::default(),
            InMemoryStorage::new(),
            SimpleEmbedder,
        );

        let memory = Memory::new(MemoryType::Fact, "The capital of France is Paris");
        let _id = mem.remember(memory).await.unwrap();

        let stats = mem.stats().await.unwrap();
        assert_eq!(stats.total_memories, 1);
    }

    #[tokio::test]
    async fn test_recall() {
        let mem = LongTermMemory::new(
            MemoryConfig::default(),
            InMemoryStorage::new(),
            SimpleEmbedder,
        );

        mem.remember(Memory::new(MemoryType::Fact, "User likes coffee"))
            .await
            .unwrap();
        mem.remember(Memory::new(MemoryType::Fact, "User prefers tea"))
            .await
            .unwrap();
        mem.remember(Memory::new(MemoryType::Fact, "User works as a developer"))
            .await
            .unwrap();

        let query = MemoryQuery::default().text("coffee");
        let results = mem.recall(query).await.unwrap();

        assert!(!results.is_empty());
        assert!(results[0].content.contains("coffee"));
    }

    #[tokio::test]
    async fn test_reinforcement() {
        let mem = LongTermMemory::new(
            MemoryConfig::default(),
            InMemoryStorage::new(),
            SimpleEmbedder,
        );

        let memory = Memory::new(MemoryType::Preference, "User likes dark mode");
        let id = mem.remember(memory).await.unwrap();

        // Access multiple times
        for _ in 0..3 {
            mem.reinforce(id).await.unwrap();
        }

        let query = MemoryQuery::default().text("dark mode");
        let results = mem.recall(query).await.unwrap();

        assert!(results[0].access_count >= 3);
    }

    #[tokio::test]
    async fn test_forget() {
        let mem = LongTermMemory::new(
            MemoryConfig::default(),
            InMemoryStorage::new(),
            SimpleEmbedder,
        );

        let memory = Memory::new(MemoryType::Event, "Temporary event");
        let id = mem.remember(memory).await.unwrap();

        mem.forget(id).await.unwrap();

        let stats = mem.stats().await.unwrap();
        assert_eq!(stats.total_memories, 0);
    }

    #[tokio::test]
    async fn test_type_filter() {
        let mem = LongTermMemory::new(
            MemoryConfig::default(),
            InMemoryStorage::new(),
            SimpleEmbedder,
        );

        mem.remember(Memory::new(MemoryType::Fact, "Fact 1"))
            .await
            .unwrap();
        mem.remember(Memory::new(MemoryType::Preference, "Preference 1"))
            .await
            .unwrap();
        mem.remember(Memory::new(MemoryType::Event, "Event 1"))
            .await
            .unwrap();

        let query = MemoryQuery::default().types(vec![MemoryType::Fact]);
        let results = mem.recall(query).await.unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].memory_type, MemoryType::Fact);
    }

    #[tokio::test]
    async fn test_context() {
        let mem = LongTermMemory::new(
            MemoryConfig::default(),
            InMemoryStorage::new(),
            SimpleEmbedder,
        );

        mem.remember(Memory::new(
            MemoryType::Fact,
            "Python is a programming language",
        ))
        .await
        .unwrap();
        mem.remember(Memory::new(MemoryType::Skill, "User knows Python"))
            .await
            .unwrap();

        let context = mem.get_context("Python", 5).await.unwrap();
        assert!(context.contains("Python"));
    }
}
