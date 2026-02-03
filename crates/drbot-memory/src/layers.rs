//! Hierarchical memory layers for drbot.
//!
//! Provides a multi-tier memory system similar to human memory:
//! - Working memory (current context)
//! - Short-term memory (recent conversations)
//! - Long-term memory (persistent knowledge)
//! - Semantic memory (learned patterns and facts)

use async_recursion::async_recursion;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Memory layer type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryLayer {
    /// Active working memory - current conversation context.
    Working,
    /// Short-term memory - recent but not current.
    ShortTerm,
    /// Long-term episodic - specific experiences and events.
    LongTermEpisodic,
    /// Long-term semantic - facts, knowledge, learned patterns.
    LongTermSemantic,
    /// Procedural memory - how to do things, skills.
    Procedural,
}

impl MemoryLayer {
    /// Get the default retention duration for this layer.
    pub fn default_retention(&self) -> Option<Duration> {
        match self {
            MemoryLayer::Working => Some(Duration::hours(1)),
            MemoryLayer::ShortTerm => Some(Duration::days(1)),
            MemoryLayer::LongTermEpisodic => Some(Duration::days(90)),
            MemoryLayer::LongTermSemantic => None, // Permanent
            MemoryLayer::Procedural => None,       // Permanent
        }
    }

    /// Get priority for retrieval (higher = more important).
    pub fn priority(&self) -> u8 {
        match self {
            MemoryLayer::Working => 10,
            MemoryLayer::ShortTerm => 8,
            MemoryLayer::LongTermSemantic => 6,
            MemoryLayer::LongTermEpisodic => 4,
            MemoryLayer::Procedural => 5,
        }
    }
}

/// A memory item that can exist in any layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayeredMemory {
    /// Memory ID.
    pub id: Uuid,
    /// Current layer.
    pub layer: MemoryLayer,
    /// Memory content.
    pub content: String,
    /// Summary (for long-term storage).
    pub summary: Option<String>,
    /// Embedding vector.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding: Option<Vec<f32>>,
    /// Importance score (0-1).
    pub importance: f32,
    /// Access count (for spaced repetition).
    pub access_count: u64,
    /// Last accessed.
    pub last_accessed: DateTime<Utc>,
    /// Created at.
    pub created_at: DateTime<Utc>,
    /// Expires at (if applicable).
    pub expires_at: Option<DateTime<Utc>>,
    /// Related memory IDs.
    pub related: Vec<Uuid>,
    /// Tags for categorization.
    pub tags: Vec<String>,
    /// Source context (session, conversation, etc.).
    pub source: MemorySource,
    /// Metadata.
    pub metadata: HashMap<String, String>,
}

impl LayeredMemory {
    /// Create a new memory in the working layer.
    pub fn new(content: &str, source: MemorySource) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            layer: MemoryLayer::Working,
            content: content.to_string(),
            summary: None,
            embedding: None,
            importance: 0.5,
            access_count: 1,
            last_accessed: now,
            created_at: now,
            expires_at: MemoryLayer::Working.default_retention().map(|d| now + d),
            related: Vec::new(),
            tags: Vec::new(),
            source,
            metadata: HashMap::new(),
        }
    }

    /// Set the importance score.
    pub fn with_importance(mut self, importance: f32) -> Self {
        self.importance = importance.clamp(0.0, 1.0);
        self
    }

    /// Set the embedding.
    pub fn with_embedding(mut self, embedding: Vec<f32>) -> Self {
        self.embedding = Some(embedding);
        self
    }

    /// Add tags.
    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    /// Record an access.
    pub fn record_access(&mut self) {
        self.access_count += 1;
        self.last_accessed = Utc::now();
    }

    /// Calculate decay factor based on time since last access.
    pub fn decay_factor(&self) -> f32 {
        let hours_since_access = (Utc::now() - self.last_accessed).num_hours() as f32;
        // Exponential decay with half-life based on layer
        let half_life = match self.layer {
            MemoryLayer::Working => 1.0,
            MemoryLayer::ShortTerm => 24.0,
            MemoryLayer::LongTermEpisodic => 720.0, // 30 days
            _ => f32::MAX,
        };
        0.5_f32.powf(hours_since_access / half_life)
    }

    /// Calculate effective importance considering decay and access patterns.
    pub fn effective_importance(&self) -> f32 {
        let base = self.importance;
        let decay = self.decay_factor();
        let access_boost = (self.access_count as f32).ln().min(2.0) / 2.0; // Up to +0.5 for frequent access
        (base * decay + access_boost).clamp(0.0, 1.0)
    }

    /// Check if this memory should be promoted to a longer-term layer.
    pub fn should_promote(&self) -> bool {
        match self.layer {
            MemoryLayer::Working => self.importance > 0.7 || self.access_count > 3,
            MemoryLayer::ShortTerm => self.importance > 0.8 || self.access_count > 10,
            _ => false,
        }
    }

    /// Check if this memory has expired.
    pub fn is_expired(&self) -> bool {
        self.expires_at.map(|exp| Utc::now() > exp).unwrap_or(false)
    }

    /// Promote to the next layer.
    pub fn promote(&mut self) {
        let now = Utc::now();
        self.layer = match self.layer {
            MemoryLayer::Working => MemoryLayer::ShortTerm,
            MemoryLayer::ShortTerm => MemoryLayer::LongTermEpisodic,
            other => other,
        };
        self.expires_at = self.layer.default_retention().map(|d| now + d);
    }
}

/// Source context for a memory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemorySource {
    /// Session ID.
    pub session_id: Option<String>,
    /// User ID.
    pub user_id: Option<String>,
    /// Channel ID.
    pub channel_id: Option<String>,
    /// Source type.
    pub source_type: SourceType,
}

impl Default for MemorySource {
    fn default() -> Self {
        Self {
            session_id: None,
            user_id: None,
            channel_id: None,
            source_type: SourceType::Conversation,
        }
    }
}

impl MemorySource {
    /// Create from a session.
    pub fn from_session(session_id: &str) -> Self {
        Self {
            session_id: Some(session_id.to_string()),
            ..Default::default()
        }
    }

    /// Set user ID.
    pub fn with_user(mut self, user_id: &str) -> Self {
        self.user_id = Some(user_id.to_string());
        self
    }

    /// Set channel ID.
    pub fn with_channel(mut self, channel_id: &str) -> Self {
        self.channel_id = Some(channel_id.to_string());
        self
    }
}

/// Source types for memories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceType {
    /// From a conversation.
    Conversation,
    /// From a document.
    Document,
    /// User explicitly saved.
    UserSaved,
    /// Learned from corrections.
    Correction,
    /// Inferred from behavior.
    Inferred,
    /// From external integration.
    Integration,
}

/// Configuration for the hierarchical memory system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HierarchicalMemoryConfig {
    /// Maximum items in working memory.
    pub working_memory_limit: usize,
    /// Maximum items in short-term memory.
    pub short_term_limit: usize,
    /// Importance threshold for promotion.
    pub promotion_threshold: f32,
    /// Enable automatic consolidation.
    pub auto_consolidate: bool,
    /// Consolidation interval in seconds.
    pub consolidation_interval_secs: u64,
    /// Enable memory summarization on promotion.
    pub summarize_on_promotion: bool,
}

impl Default for HierarchicalMemoryConfig {
    fn default() -> Self {
        Self {
            working_memory_limit: 20,
            short_term_limit: 100,
            promotion_threshold: 0.7,
            auto_consolidate: true,
            consolidation_interval_secs: 300, // 5 minutes
            summarize_on_promotion: true,
        }
    }
}

/// Hierarchical memory manager.
pub struct HierarchicalMemory {
    /// Working memory (current context).
    working: Arc<RwLock<VecDeque<LayeredMemory>>>,
    /// Short-term memory.
    short_term: Arc<RwLock<Vec<LayeredMemory>>>,
    /// Long-term episodic memory.
    long_term_episodic: Arc<RwLock<Vec<LayeredMemory>>>,
    /// Long-term semantic memory.
    long_term_semantic: Arc<RwLock<Vec<LayeredMemory>>>,
    /// Procedural memory.
    procedural: Arc<RwLock<Vec<LayeredMemory>>>,
    /// Configuration.
    config: HierarchicalMemoryConfig,
}

impl HierarchicalMemory {
    /// Create a new hierarchical memory system.
    pub fn new(config: HierarchicalMemoryConfig) -> Self {
        Self {
            working: Arc::new(RwLock::new(VecDeque::new())),
            short_term: Arc::new(RwLock::new(Vec::new())),
            long_term_episodic: Arc::new(RwLock::new(Vec::new())),
            long_term_semantic: Arc::new(RwLock::new(Vec::new())),
            procedural: Arc::new(RwLock::new(Vec::new())),
            config,
        }
    }

    /// Add a memory to working memory.
    pub async fn remember(&self, memory: LayeredMemory) {
        let mut working = self.working.write().await;

        // Enforce working memory limit
        while working.len() >= self.config.working_memory_limit {
            if let Some(evicted) = working.pop_front() {
                // Check if evicted memory should be promoted
                if evicted.should_promote() {
                    let mut promoted = evicted;
                    promoted.promote();
                    self.store_in_layer(promoted).await;
                }
            }
        }

        working.push_back(memory);
    }

    /// Store memory in its designated layer.
    #[async_recursion]
    async fn store_in_layer(&self, memory: LayeredMemory) {
        match memory.layer {
            MemoryLayer::Working => {
                self.working.write().await.push_back(memory);
            }
            MemoryLayer::ShortTerm => {
                let mut short_term = self.short_term.write().await;

                // Enforce short-term limit
                if short_term.len() >= self.config.short_term_limit {
                    // Sort by effective importance and remove least important
                    short_term.sort_by(|a, b| {
                        b.effective_importance()
                            .partial_cmp(&a.effective_importance())
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });

                    // Promote top items, remove expired/low importance
                    let mut to_promote = Vec::new();
                    while short_term.len() >= self.config.short_term_limit {
                        if let Some(mut item) = short_term.pop() {
                            if item.should_promote() {
                                item.promote();
                                to_promote.push(item);
                            }
                            // Otherwise, item is dropped
                        }
                    }

                    drop(short_term);
                    for item in to_promote {
                        self.store_in_layer(item).await;
                    }

                    self.short_term.write().await.push(memory);
                } else {
                    short_term.push(memory);
                }
            }
            MemoryLayer::LongTermEpisodic => {
                self.long_term_episodic.write().await.push(memory);
            }
            MemoryLayer::LongTermSemantic => {
                self.long_term_semantic.write().await.push(memory);
            }
            MemoryLayer::Procedural => {
                self.procedural.write().await.push(memory);
            }
        }
    }

    /// Recall memories relevant to a query.
    pub async fn recall(
        &self,
        query_embedding: &[f32],
        options: RecallOptions,
    ) -> Vec<RecallResult> {
        let mut results = Vec::new();

        // Search each layer based on options
        if options.include_working {
            let working = self.working.read().await;
            for memory in working.iter() {
                if let Some(score) = self.similarity(query_embedding, memory) {
                    if score >= options.min_similarity {
                        results.push(RecallResult {
                            memory: memory.clone(),
                            similarity: score,
                            layer_priority: MemoryLayer::Working.priority(),
                        });
                    }
                }
            }
        }

        if options.include_short_term {
            let short_term = self.short_term.read().await;
            for memory in short_term.iter() {
                if let Some(score) = self.similarity(query_embedding, memory) {
                    if score >= options.min_similarity {
                        results.push(RecallResult {
                            memory: memory.clone(),
                            similarity: score,
                            layer_priority: MemoryLayer::ShortTerm.priority(),
                        });
                    }
                }
            }
        }

        if options.include_long_term {
            for memory in self.long_term_episodic.read().await.iter() {
                if let Some(score) = self.similarity(query_embedding, memory) {
                    if score >= options.min_similarity {
                        results.push(RecallResult {
                            memory: memory.clone(),
                            similarity: score,
                            layer_priority: MemoryLayer::LongTermEpisodic.priority(),
                        });
                    }
                }
            }

            for memory in self.long_term_semantic.read().await.iter() {
                if let Some(score) = self.similarity(query_embedding, memory) {
                    if score >= options.min_similarity {
                        results.push(RecallResult {
                            memory: memory.clone(),
                            similarity: score,
                            layer_priority: MemoryLayer::LongTermSemantic.priority(),
                        });
                    }
                }
            }
        }

        if options.include_procedural {
            for memory in self.procedural.read().await.iter() {
                if let Some(score) = self.similarity(query_embedding, memory) {
                    if score >= options.min_similarity {
                        results.push(RecallResult {
                            memory: memory.clone(),
                            similarity: score,
                            layer_priority: MemoryLayer::Procedural.priority(),
                        });
                    }
                }
            }
        }

        // Sort by combined score (similarity + layer priority + importance)
        results.sort_by(|a, b| {
            let score_a = a.combined_score();
            let score_b = b.combined_score();
            score_b
                .partial_cmp(&score_a)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Apply limit
        if let Some(limit) = options.limit {
            results.truncate(limit);
        }

        // Record access for retrieved memories
        for result in &results {
            self.record_access(result.memory.id).await;
        }

        results
    }

    /// Calculate cosine similarity between query and memory.
    fn similarity(&self, query: &[f32], memory: &LayeredMemory) -> Option<f32> {
        memory.embedding.as_ref().map(|emb| {
            let dot: f32 = query.iter().zip(emb.iter()).map(|(a, b)| a * b).sum();
            let mag_q: f32 = query.iter().map(|x| x * x).sum::<f32>().sqrt();
            let mag_m: f32 = emb.iter().map(|x| x * x).sum::<f32>().sqrt();
            if mag_q > 0.0 && mag_m > 0.0 {
                dot / (mag_q * mag_m)
            } else {
                0.0
            }
        })
    }

    /// Record an access to a memory.
    async fn record_access(&self, id: Uuid) {
        // Check each layer and record access
        let mut working = self.working.write().await;
        if let Some(mem) = working.iter_mut().find(|m| m.id == id) {
            mem.record_access();
            return;
        }
        drop(working);

        let mut short_term = self.short_term.write().await;
        if let Some(mem) = short_term.iter_mut().find(|m| m.id == id) {
            mem.record_access();
            return;
        }
        drop(short_term);

        let mut long_term = self.long_term_episodic.write().await;
        if let Some(mem) = long_term.iter_mut().find(|m| m.id == id) {
            mem.record_access();
            return;
        }
        drop(long_term);

        let mut semantic = self.long_term_semantic.write().await;
        if let Some(mem) = semantic.iter_mut().find(|m| m.id == id) {
            mem.record_access();
        }
    }

    /// Consolidate memories (promote important ones, expire old ones).
    pub async fn consolidate(&self) {
        // Working -> Short-term promotion
        let mut to_promote = Vec::new();
        {
            let mut working = self.working.write().await;
            working.retain(|mem| {
                if mem.is_expired() {
                    if mem.should_promote() {
                        let mut promoted = mem.clone();
                        promoted.promote();
                        to_promote.push(promoted);
                    }
                    false
                } else {
                    true
                }
            });
        }

        for mem in to_promote {
            self.store_in_layer(mem).await;
        }

        // Short-term -> Long-term promotion
        let mut to_promote = Vec::new();
        {
            let mut short_term = self.short_term.write().await;
            short_term.retain(|mem| {
                if mem.is_expired() || mem.should_promote() {
                    if mem.importance > self.config.promotion_threshold {
                        let mut promoted = mem.clone();
                        promoted.promote();
                        to_promote.push(promoted);
                    }
                    mem.should_promote() // Keep if being promoted (will be removed after)
                } else {
                    true
                }
            });
        }

        for mem in to_promote {
            self.store_in_layer(mem).await;
        }

        // Clean up expired long-term episodic
        {
            let mut episodic = self.long_term_episodic.write().await;
            episodic.retain(|mem| !mem.is_expired());
        }
    }

    /// Get working memory contents.
    pub async fn working_memory(&self) -> Vec<LayeredMemory> {
        self.working.read().await.iter().cloned().collect()
    }

    /// Clear working memory.
    pub async fn clear_working(&self) {
        self.working.write().await.clear();
    }

    /// Get memory statistics.
    pub async fn stats(&self) -> MemoryStats {
        MemoryStats {
            working_count: self.working.read().await.len(),
            short_term_count: self.short_term.read().await.len(),
            long_term_episodic_count: self.long_term_episodic.read().await.len(),
            long_term_semantic_count: self.long_term_semantic.read().await.len(),
            procedural_count: self.procedural.read().await.len(),
        }
    }

    /// Add a semantic memory directly (learned fact).
    pub async fn learn_fact(&self, content: &str, source: MemorySource) {
        let mut memory = LayeredMemory::new(content, source);
        memory.layer = MemoryLayer::LongTermSemantic;
        memory.importance = 0.8;
        memory.expires_at = None;
        self.long_term_semantic.write().await.push(memory);
    }

    /// Add a procedural memory (skill/procedure).
    pub async fn learn_procedure(&self, name: &str, content: &str, source: MemorySource) {
        let mut memory = LayeredMemory::new(content, source);
        memory.layer = MemoryLayer::Procedural;
        memory.importance = 0.9;
        memory.expires_at = None;
        memory.tags.push(name.to_string());
        self.procedural.write().await.push(memory);
    }
}

/// Options for recalling memories.
#[derive(Debug, Clone)]
pub struct RecallOptions {
    /// Minimum similarity score.
    pub min_similarity: f32,
    /// Maximum results.
    pub limit: Option<usize>,
    /// Include working memory.
    pub include_working: bool,
    /// Include short-term memory.
    pub include_short_term: bool,
    /// Include long-term memory.
    pub include_long_term: bool,
    /// Include procedural memory.
    pub include_procedural: bool,
    /// Filter by tags.
    pub tags: Option<Vec<String>>,
    /// Filter by user ID.
    pub user_id: Option<String>,
}

impl Default for RecallOptions {
    fn default() -> Self {
        Self {
            min_similarity: 0.5,
            limit: Some(10),
            include_working: true,
            include_short_term: true,
            include_long_term: true,
            include_procedural: true,
            tags: None,
            user_id: None,
        }
    }
}

/// Result from memory recall.
#[derive(Debug, Clone)]
pub struct RecallResult {
    /// The retrieved memory.
    pub memory: LayeredMemory,
    /// Similarity score to query.
    pub similarity: f32,
    /// Priority from layer.
    pub layer_priority: u8,
}

impl RecallResult {
    /// Calculate combined relevance score.
    pub fn combined_score(&self) -> f32 {
        let similarity_weight = 0.6;
        let priority_weight = 0.2;
        let importance_weight = 0.2;

        self.similarity * similarity_weight
            + (self.layer_priority as f32 / 10.0) * priority_weight
            + self.memory.effective_importance() * importance_weight
    }
}

/// Memory statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryStats {
    /// Working memory count.
    pub working_count: usize,
    /// Short-term memory count.
    pub short_term_count: usize,
    /// Long-term episodic count.
    pub long_term_episodic_count: usize,
    /// Long-term semantic count.
    pub long_term_semantic_count: usize,
    /// Procedural memory count.
    pub procedural_count: usize,
}

impl MemoryStats {
    /// Total memories across all layers.
    pub fn total(&self) -> usize {
        self.working_count
            + self.short_term_count
            + self.long_term_episodic_count
            + self.long_term_semantic_count
            + self.procedural_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_hierarchical_memory() {
        let config = HierarchicalMemoryConfig {
            working_memory_limit: 3,
            ..Default::default()
        };
        let memory = HierarchicalMemory::new(config);

        // Add memories with high importance so they get promoted when evicted
        for i in 0..5 {
            let mem =
                LayeredMemory::new(&format!("Memory {}", i), MemorySource::from_session("test"))
                    .with_importance(0.8); // All important enough to promote
            memory.remember(mem).await;
        }

        // Working memory should be limited to 3
        let stats = memory.stats().await;
        assert_eq!(stats.working_count, 3);

        // First 2 memories should have been evicted and promoted to short-term
        assert_eq!(stats.short_term_count, 2);
    }

    #[tokio::test]
    async fn test_memory_recall() {
        let memory = HierarchicalMemory::new(HierarchicalMemoryConfig::default());

        // Add memory with embedding
        let emb = vec![1.0, 0.0, 0.0, 0.0];
        let mem =
            LayeredMemory::new("Test memory", MemorySource::default()).with_embedding(emb.clone());
        memory.remember(mem).await;

        // Recall with same embedding
        let results = memory.recall(&emb, RecallOptions::default()).await;
        assert_eq!(results.len(), 1);
        assert!(results[0].similarity > 0.99);
    }

    #[test]
    fn test_decay_factor() {
        let mem = LayeredMemory::new("Test", MemorySource::default());
        // Fresh memory should have decay factor close to 1
        assert!(mem.decay_factor() > 0.99);
    }

    #[test]
    fn test_effective_importance() {
        let mut mem = LayeredMemory::new("Test", MemorySource::default()).with_importance(0.5);

        // Record multiple accesses
        for _ in 0..5 {
            mem.record_access();
        }

        // Importance should be boosted by access count
        assert!(mem.effective_importance() > 0.5);
    }

    #[tokio::test]
    async fn test_learn_fact() {
        let memory = HierarchicalMemory::new(HierarchicalMemoryConfig::default());

        memory
            .learn_fact("The sky is blue", MemorySource::default())
            .await;

        let stats = memory.stats().await;
        assert_eq!(stats.long_term_semantic_count, 1);
    }
}
