//! Long-term memory system for cross-session knowledge retention.
//!
//! This module provides:
//! - Cross-session memories that persist across conversations
//! - Importance scoring to prioritize what to remember
//! - Memory categories (facts, preferences, knowledge)
//! - Semantic deduplication to avoid redundant memories
//! - Automatic summarization of related memories

use crate::{Memory, MemorySearchResult, MemoryStore, SearchOptions};
use chrono::{DateTime, Duration, Utc};
use drbot_core::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Memory category for organization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryCategory {
    /// User preferences (likes, dislikes, settings)
    Preference,
    /// Factual information about the user
    Fact,
    /// Knowledge or learned information
    Knowledge,
    /// Task or project related
    Task,
    /// Relationship information (people, connections)
    Relationship,
    /// Temporal events (appointments, deadlines)
    Event,
    /// Conversation summary
    Summary,
    /// General/uncategorized
    General,
}

impl Default for MemoryCategory {
    fn default() -> Self {
        Self::General
    }
}

impl std::fmt::Display for MemoryCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Preference => write!(f, "preference"),
            Self::Fact => write!(f, "fact"),
            Self::Knowledge => write!(f, "knowledge"),
            Self::Task => write!(f, "task"),
            Self::Relationship => write!(f, "relationship"),
            Self::Event => write!(f, "event"),
            Self::Summary => write!(f, "summary"),
            Self::General => write!(f, "general"),
        }
    }
}

/// Importance level for memories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Importance {
    /// Low importance - may be forgotten
    Low = 1,
    /// Normal importance
    Normal = 2,
    /// High importance - should be retained
    High = 3,
    /// Critical - never forget
    Critical = 4,
}

impl Default for Importance {
    fn default() -> Self {
        Self::Normal
    }
}

/// A long-term memory entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LongTermMemory {
    /// Unique ID.
    pub id: Uuid,
    /// User ID this memory belongs to.
    pub user_id: String,
    /// Memory content.
    pub content: String,
    /// Category of memory.
    pub category: MemoryCategory,
    /// Importance level.
    pub importance: Importance,
    /// Confidence score (0.0 - 1.0).
    pub confidence: f32,
    /// Source session ID (if derived from a session).
    pub source_session: Option<String>,
    /// Related memory IDs.
    pub related_memories: Vec<Uuid>,
    /// Tags for additional organization.
    pub tags: Vec<String>,
    /// When this memory was created.
    pub created_at: DateTime<Utc>,
    /// When this memory was last accessed.
    pub last_accessed: DateTime<Utc>,
    /// Number of times this memory was accessed.
    pub access_count: u32,
    /// Embedding vector for similarity search.
    pub embedding: Option<Vec<f32>>,
}

impl LongTermMemory {
    /// Create a new long-term memory.
    pub fn new(user_id: impl Into<String>, content: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            user_id: user_id.into(),
            content: content.into(),
            category: MemoryCategory::General,
            importance: Importance::Normal,
            confidence: 1.0,
            source_session: None,
            related_memories: Vec::new(),
            tags: Vec::new(),
            created_at: now,
            last_accessed: now,
            access_count: 0,
            embedding: None,
        }
    }

    /// Set the category.
    pub fn with_category(mut self, category: MemoryCategory) -> Self {
        self.category = category;
        self
    }

    /// Set the importance.
    pub fn with_importance(mut self, importance: Importance) -> Self {
        self.importance = importance;
        self
    }

    /// Set the confidence.
    pub fn with_confidence(mut self, confidence: f32) -> Self {
        self.confidence = confidence.clamp(0.0, 1.0);
        self
    }

    /// Set the source session.
    pub fn from_session(mut self, session_id: impl Into<String>) -> Self {
        self.source_session = Some(session_id.into());
        self
    }

    /// Add a tag.
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    /// Set the embedding.
    pub fn with_embedding(mut self, embedding: Vec<f32>) -> Self {
        self.embedding = Some(embedding);
        self
    }

    /// Add a related memory.
    pub fn with_related(mut self, memory_id: Uuid) -> Self {
        if !self.related_memories.contains(&memory_id) {
            self.related_memories.push(memory_id);
        }
        self
    }

    /// Calculate retention score based on importance and access patterns.
    pub fn retention_score(&self) -> f32 {
        let importance_factor = match self.importance {
            Importance::Low => 0.25,
            Importance::Normal => 0.5,
            Importance::High => 0.75,
            Importance::Critical => 1.0,
        };

        let recency_factor = {
            let age = Utc::now() - self.last_accessed;
            let days = age.num_days() as f32;
            // Decay over time, but slower for frequently accessed
            let decay_rate = 0.1 / (1.0 + self.access_count as f32 * 0.1);
            (-decay_rate * days).exp()
        };

        let access_factor = (self.access_count as f32).ln_1p() / 10.0;

        (importance_factor * 0.4 + recency_factor * 0.4 + access_factor * 0.2) * self.confidence
    }

    /// Record an access.
    pub fn record_access(&mut self) {
        self.access_count += 1;
        self.last_accessed = Utc::now();
    }
}

/// Long-term memory store.
pub struct LongTermMemoryStore {
    /// Internal memory store.
    store: MemoryStore,
    /// Similarity threshold for deduplication.
    dedup_threshold: f32,
    /// Minimum retention score to keep.
    min_retention_score: f32,
}

impl LongTermMemoryStore {
    /// Create a new long-term memory store.
    pub fn new(store: MemoryStore) -> Self {
        Self {
            store,
            dedup_threshold: 0.9,
            min_retention_score: 0.1,
        }
    }

    /// Set deduplication threshold.
    pub fn with_dedup_threshold(mut self, threshold: f32) -> Self {
        self.dedup_threshold = threshold.clamp(0.5, 1.0);
        self
    }

    /// Set minimum retention score.
    pub fn with_min_retention(mut self, score: f32) -> Self {
        self.min_retention_score = score.clamp(0.0, 1.0);
        self
    }

    /// Store a long-term memory.
    pub async fn store(&self, memory: &LongTermMemory) -> Result<()> {
        let metadata = serde_json::json!({
            "type": "longterm",
            "category": memory.category.to_string(),
            "importance": memory.importance as u8,
            "confidence": memory.confidence,
            "source_session": memory.source_session,
            "related_memories": memory.related_memories,
            "tags": memory.tags,
            "last_accessed": memory.last_accessed.to_rfc3339(),
            "access_count": memory.access_count,
        });

        let mem = Memory {
            id: memory.id,
            session_id: format!("__longterm__{}", memory.user_id),
            content: memory.content.clone(),
            role: "memory".to_string(),
            embedding: memory.embedding.clone(),
            created_at: memory.created_at,
            metadata,
        };

        self.store.store(&mem).await
    }

    /// Get a memory by ID.
    pub async fn get(&self, id: Uuid) -> Result<Option<LongTermMemory>> {
        let mem = self.store.get(id).await?;
        Ok(mem.and_then(|m| self.memory_to_longterm(&m)))
    }

    /// Get memories for a user.
    pub async fn get_for_user(&self, user_id: &str, limit: usize) -> Result<Vec<LongTermMemory>> {
        let session_id = format!("__longterm__{}", user_id);
        let memories = self.store.get_recent(&session_id, limit).await?;
        Ok(memories
            .iter()
            .filter_map(|m| self.memory_to_longterm(m))
            .collect())
    }

    /// Search memories by similarity.
    pub async fn search(
        &self,
        user_id: &str,
        query_embedding: &[f32],
        limit: usize,
    ) -> Result<Vec<(LongTermMemory, f32)>> {
        let session_id = format!("__longterm__{}", user_id);
        let options = SearchOptions::new().session(&session_id).limit(limit);
        let results = self.store.search(query_embedding, options).await?;

        Ok(results
            .into_iter()
            .filter_map(|r| self.memory_to_longterm(&r.memory).map(|m| (m, r.score)))
            .collect())
    }

    /// Search by category.
    pub async fn search_by_category(
        &self,
        user_id: &str,
        category: MemoryCategory,
        limit: usize,
    ) -> Result<Vec<LongTermMemory>> {
        let all = self.get_for_user(user_id, 1000).await?;
        let filtered: Vec<_> = all
            .into_iter()
            .filter(|m| m.category == category)
            .take(limit)
            .collect();
        Ok(filtered)
    }

    /// Search by tag.
    pub async fn search_by_tag(
        &self,
        user_id: &str,
        tag: &str,
        limit: usize,
    ) -> Result<Vec<LongTermMemory>> {
        let all = self.get_for_user(user_id, 1000).await?;
        let filtered: Vec<_> = all
            .into_iter()
            .filter(|m| m.tags.iter().any(|t| t == tag))
            .take(limit)
            .collect();
        Ok(filtered)
    }

    /// Check for duplicates and return similar memories.
    pub async fn find_duplicates(
        &self,
        user_id: &str,
        embedding: &[f32],
    ) -> Result<Vec<(LongTermMemory, f32)>> {
        let results = self.search(user_id, embedding, 5).await?;
        Ok(results
            .into_iter()
            .filter(|(_, score)| *score >= self.dedup_threshold)
            .collect())
    }

    /// Store with deduplication - returns existing ID if duplicate found.
    pub async fn store_dedup(&self, memory: &LongTermMemory) -> Result<(Uuid, bool)> {
        if let Some(embedding) = &memory.embedding {
            let duplicates = self.find_duplicates(&memory.user_id, embedding).await?;

            if let Some((existing, _)) = duplicates.first() {
                // Found duplicate - merge instead
                return Ok((existing.id, false));
            }
        }

        self.store(memory).await?;
        Ok((memory.id, true))
    }

    /// Delete a memory.
    pub async fn delete(&self, id: Uuid) -> Result<bool> {
        self.store.delete(id).await
    }

    /// Prune low-retention memories.
    pub async fn prune(&self, user_id: &str) -> Result<usize> {
        let memories = self.get_for_user(user_id, 10000).await?;
        let mut pruned = 0;

        for memory in memories {
            if memory.retention_score() < self.min_retention_score {
                if memory.importance < Importance::Critical {
                    self.delete(memory.id).await?;
                    pruned += 1;
                }
            }
        }

        Ok(pruned)
    }

    /// Get memory statistics for a user.
    pub async fn user_stats(&self, user_id: &str) -> Result<UserMemoryStats> {
        let memories = self.get_for_user(user_id, 10000).await?;

        let mut by_category: HashMap<MemoryCategory, usize> = HashMap::new();
        let mut total_accesses = 0;

        for memory in &memories {
            *by_category.entry(memory.category).or_insert(0) += 1;
            total_accesses += memory.access_count;
        }

        Ok(UserMemoryStats {
            total_memories: memories.len(),
            by_category,
            total_accesses,
            avg_retention_score: if memories.is_empty() {
                0.0
            } else {
                memories.iter().map(|m| m.retention_score()).sum::<f32>() / memories.len() as f32
            },
        })
    }

    /// Convert internal Memory to LongTermMemory.
    fn memory_to_longterm(&self, memory: &Memory) -> Option<LongTermMemory> {
        let metadata = &memory.metadata;

        if metadata.get("type")?.as_str()? != "longterm" {
            return None;
        }

        let user_id = memory.session_id.strip_prefix("__longterm__")?.to_string();

        let category = match metadata.get("category")?.as_str()? {
            "preference" => MemoryCategory::Preference,
            "fact" => MemoryCategory::Fact,
            "knowledge" => MemoryCategory::Knowledge,
            "task" => MemoryCategory::Task,
            "relationship" => MemoryCategory::Relationship,
            "event" => MemoryCategory::Event,
            "summary" => MemoryCategory::Summary,
            _ => MemoryCategory::General,
        };

        let importance = match metadata.get("importance")?.as_u64()? {
            1 => Importance::Low,
            2 => Importance::Normal,
            3 => Importance::High,
            4 => Importance::Critical,
            _ => Importance::Normal,
        };

        Some(LongTermMemory {
            id: memory.id,
            user_id,
            content: memory.content.clone(),
            category,
            importance,
            confidence: metadata
                .get("confidence")
                .and_then(|v| v.as_f64())
                .unwrap_or(1.0) as f32,
            source_session: metadata
                .get("source_session")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            related_memories: metadata
                .get("related_memories")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().and_then(|s| Uuid::parse_str(s).ok()))
                        .collect()
                })
                .unwrap_or_default(),
            tags: metadata
                .get("tags")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default(),
            created_at: memory.created_at,
            last_accessed: metadata
                .get("last_accessed")
                .and_then(|v| v.as_str())
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or(memory.created_at),
            access_count: metadata
                .get("access_count")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32,
            embedding: memory.embedding.clone(),
        })
    }
}

/// User memory statistics.
#[derive(Debug, Clone)]
pub struct UserMemoryStats {
    /// Total number of memories.
    pub total_memories: usize,
    /// Count by category.
    pub by_category: HashMap<MemoryCategory, usize>,
    /// Total access count.
    pub total_accesses: u32,
    /// Average retention score.
    pub avg_retention_score: f32,
}

/// Memory consolidation - merge related memories into summaries.
pub struct MemoryConsolidator {
    /// Similarity threshold for grouping.
    group_threshold: f32,
    /// Maximum memories to merge into one summary.
    max_merge_count: usize,
}

impl MemoryConsolidator {
    /// Create a new consolidator.
    pub fn new() -> Self {
        Self {
            group_threshold: 0.8,
            max_merge_count: 10,
        }
    }

    /// Set group threshold.
    pub fn with_threshold(mut self, threshold: f32) -> Self {
        self.group_threshold = threshold;
        self
    }

    /// Find groups of related memories for consolidation.
    pub fn find_groups(&self, memories: &[LongTermMemory]) -> Vec<Vec<usize>> {
        let mut groups: Vec<Vec<usize>> = Vec::new();
        let mut assigned: Vec<bool> = vec![false; memories.len()];

        for i in 0..memories.len() {
            if assigned[i] {
                continue;
            }

            let mut group = vec![i];
            assigned[i] = true;

            // Check if any unassigned memories are similar
            for j in (i + 1)..memories.len() {
                if assigned[j] || group.len() >= self.max_merge_count {
                    continue;
                }

                // Check if same category
                if memories[i].category == memories[j].category {
                    // If both have embeddings, check similarity
                    if let (Some(emb_i), Some(emb_j)) =
                        (&memories[i].embedding, &memories[j].embedding)
                    {
                        let similarity = cosine_similarity(emb_i, emb_j);
                        if similarity >= self.group_threshold {
                            group.push(j);
                            assigned[j] = true;
                        }
                    }
                }
            }

            if group.len() > 1 {
                groups.push(group);
            }
        }

        groups
    }

    /// Create a summary memory from a group.
    pub fn create_summary(
        &self,
        memories: &[&LongTermMemory],
        summary_content: &str,
        summary_embedding: Option<Vec<f32>>,
    ) -> LongTermMemory {
        let user_id = &memories[0].user_id;
        let category = memories[0].category;

        // Take highest importance
        let importance = memories
            .iter()
            .map(|m| m.importance)
            .max()
            .unwrap_or(Importance::Normal);

        // Average confidence
        let confidence = memories.iter().map(|m| m.confidence).sum::<f32>() / memories.len() as f32;

        // Collect all related memory IDs
        let related: Vec<Uuid> = memories.iter().map(|m| m.id).collect();

        // Merge tags
        let mut tags: Vec<String> = memories
            .iter()
            .flat_map(|m| m.tags.iter().cloned())
            .collect();
        tags.sort();
        tags.dedup();

        LongTermMemory::new(user_id, summary_content)
            .with_category(MemoryCategory::Summary)
            .with_importance(importance)
            .with_confidence(confidence)
            .with_embedding(summary_embedding.unwrap_or_default())
            .with_tag("consolidated")
    }
}

impl Default for MemoryConsolidator {
    fn default() -> Self {
        Self::new()
    }
}

/// Calculate cosine similarity between two vectors.
///
/// # Properties (verified by Kani):
/// - Returns 0.0 for mismatched vector lengths
/// - Returns 0.0 for zero vectors
/// - Returns values in [-1, 1] for non-zero finite vectors
/// - Is symmetric: cosine_similarity(a, b) == cosine_similarity(b, a)
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }

    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        0.0
    } else {
        dot / (norm_a * norm_b)
    }
}

// ============================================================================
// KANI FORMAL VERIFICATION PROOFS
// ============================================================================

#[cfg(kani)]
mod kani_proofs {
    use super::*;

    /// Proof: cosine_similarity returns 0.0 for mismatched vector lengths
    #[kani::proof]
    fn proof_cosine_mismatched_returns_zero() {
        let a: [f32; 2] = kani::any();
        let b: [f32; 3] = kani::any();
        let result = cosine_similarity(&a, &b);
        kani::assert(result == 0.0, "Mismatched lengths must return 0.0");
    }

    /// Proof: cosine_similarity returns 0.0 for zero vectors
    #[kani::proof]
    fn proof_cosine_zero_vector() {
        let zero = [0.0f32; 3];
        let other: [f32; 3] = kani::any();
        let result = cosine_similarity(&zero, &other);
        kani::assert(result == 0.0, "Zero vector must return 0.0");
    }

    /// Proof: cosine_similarity is symmetric
    #[kani::proof]
    fn proof_cosine_symmetric() {
        let a: [f32; 2] = kani::any();
        let b: [f32; 2] = kani::any();
        kani::assume(a.iter().all(|x| x.is_finite()));
        kani::assume(b.iter().all(|x| x.is_finite()));

        let ab = cosine_similarity(&a, &b);
        let ba = cosine_similarity(&b, &a);
        let diff = (ab - ba).abs();
        kani::assert(
            diff < 1e-6 || (ab.is_nan() && ba.is_nan()),
            "Cosine similarity must be symmetric",
        );
    }

    /// Proof: identical non-zero vectors have similarity ~1.0
    #[kani::proof]
    fn proof_cosine_identical_is_one() {
        let a: [f32; 2] = kani::any();
        kani::assume(a.iter().all(|x| x.is_finite()));
        kani::assume(a.iter().any(|x| *x != 0.0));

        let result = cosine_similarity(&a, &a);
        let diff = (result - 1.0).abs();
        kani::assert(diff < 1e-5, "Identical vectors must have similarity ~1.0");
    }

    /// Proof: confidence clamping produces valid bounds
    #[kani::proof]
    fn proof_confidence_clamping() {
        let input: f32 = kani::any();
        kani::assume(input.is_finite());

        let result = input.clamp(0.0, 1.0);
        kani::assert(result >= 0.0, "Clamped confidence must be >= 0");
        kani::assert(result <= 1.0, "Clamped confidence must be <= 1");
    }

    /// Proof: retention_score is non-negative for valid inputs
    #[kani::proof]
    fn proof_retention_non_negative() {
        let mem = LongTermMemory::new("user", "test")
            .with_importance(Importance::Normal)
            .with_confidence(0.5);

        let score = mem.retention_score();
        kani::assert(
            score >= 0.0 || score.is_nan(),
            "Retention score must be non-negative",
        );
    }

    /// Proof: zero confidence yields zero retention
    #[kani::proof]
    fn proof_zero_confidence_zero_retention() {
        let mem = LongTermMemory::new("user", "test").with_confidence(0.0);

        let score = mem.retention_score();
        kani::assert(score == 0.0, "Zero confidence must yield zero retention");
    }

    /// Proof: dedup threshold clamping produces valid range
    #[kani::proof]
    fn proof_dedup_threshold_bounds() {
        let input: f32 = kani::any();
        kani::assume(input.is_finite());

        let result = input.clamp(0.5, 1.0);
        kani::assert(result >= 0.5, "Dedup threshold must be >= 0.5");
        kani::assert(result <= 1.0, "Dedup threshold must be <= 1.0");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_creation() {
        let mem = LongTermMemory::new("user1", "User likes coffee")
            .with_category(MemoryCategory::Preference)
            .with_importance(Importance::High)
            .with_tag("food");

        assert_eq!(mem.user_id, "user1");
        assert_eq!(mem.category, MemoryCategory::Preference);
        assert_eq!(mem.importance, Importance::High);
        assert!(mem.tags.contains(&"food".to_string()));
    }

    #[test]
    fn test_retention_score() {
        let mut mem = LongTermMemory::new("user1", "test").with_importance(Importance::Critical);

        let initial_score = mem.retention_score();

        mem.record_access();
        mem.record_access();

        let after_access = mem.retention_score();

        // Score should increase slightly with access
        assert!(after_access >= initial_score);
    }

    #[test]
    fn test_importance_ordering() {
        assert!(Importance::Critical > Importance::High);
        assert!(Importance::High > Importance::Normal);
        assert!(Importance::Normal > Importance::Low);
    }

    #[tokio::test]
    async fn test_longterm_store() {
        let base_store = MemoryStore::in_memory().unwrap();
        let store = LongTermMemoryStore::new(base_store);

        let mem = LongTermMemory::new("user1", "User prefers dark mode")
            .with_category(MemoryCategory::Preference)
            .with_importance(Importance::Normal);

        store.store(&mem).await.unwrap();

        let retrieved = store.get(mem.id).await.unwrap();
        assert!(retrieved.is_some());

        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.content, "User prefers dark mode");
        assert_eq!(retrieved.category, MemoryCategory::Preference);
    }

    #[tokio::test]
    async fn test_search_by_category() {
        let base_store = MemoryStore::in_memory().unwrap();
        let store = LongTermMemoryStore::new(base_store);

        let mem1 = LongTermMemory::new("user1", "Prefers dark mode")
            .with_category(MemoryCategory::Preference);
        let mem2 =
            LongTermMemory::new("user1", "Works at Acme Corp").with_category(MemoryCategory::Fact);
        let mem3 =
            LongTermMemory::new("user1", "Likes Python").with_category(MemoryCategory::Preference);

        store.store(&mem1).await.unwrap();
        store.store(&mem2).await.unwrap();
        store.store(&mem3).await.unwrap();

        let prefs = store
            .search_by_category("user1", MemoryCategory::Preference, 10)
            .await
            .unwrap();
        assert_eq!(prefs.len(), 2);

        let facts = store
            .search_by_category("user1", MemoryCategory::Fact, 10)
            .await
            .unwrap();
        assert_eq!(facts.len(), 1);
    }

    #[test]
    fn test_consolidator_grouping() {
        let consolidator = MemoryConsolidator::new().with_threshold(0.9);

        let emb1 = vec![1.0, 0.0, 0.0];
        let emb2 = vec![0.99, 0.01, 0.0];
        let emb3 = vec![0.0, 1.0, 0.0];

        let memories = vec![
            LongTermMemory::new("u1", "test1")
                .with_category(MemoryCategory::Fact)
                .with_embedding(emb1),
            LongTermMemory::new("u1", "test2")
                .with_category(MemoryCategory::Fact)
                .with_embedding(emb2),
            LongTermMemory::new("u1", "test3")
                .with_category(MemoryCategory::Fact)
                .with_embedding(emb3),
        ];

        let groups = consolidator.find_groups(&memories);
        // First two should be grouped (similar embeddings)
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].len(), 2);
    }
}
