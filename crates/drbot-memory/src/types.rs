//! Memory types.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A stored memory entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Memory {
    /// Unique ID.
    pub id: Uuid,
    /// Session ID this memory belongs to.
    pub session_id: String,
    /// Memory content.
    pub content: String,
    /// Role (user, assistant, system).
    pub role: String,
    /// Embedding vector (if computed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding: Option<Vec<f32>>,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Metadata.
    #[serde(default)]
    pub metadata: serde_json::Value,
}

impl Memory {
    /// Create a new memory entry.
    pub fn new(
        session_id: impl Into<String>,
        role: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            session_id: session_id.into(),
            content: content.into(),
            role: role.into(),
            embedding: None,
            created_at: Utc::now(),
            metadata: serde_json::Value::Null,
        }
    }

    /// Set embedding vector.
    pub fn with_embedding(mut self, embedding: Vec<f32>) -> Self {
        self.embedding = Some(embedding);
        self
    }

    /// Set metadata.
    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = metadata;
        self
    }
}

/// Search result from vector similarity search.
#[derive(Debug, Clone)]
pub struct MemorySearchResult {
    /// The memory entry.
    pub memory: Memory,
    /// Similarity score (0-1, higher is more similar).
    pub score: f32,
}

/// Options for memory search.
#[derive(Debug, Clone, Default)]
pub struct SearchOptions {
    /// Maximum number of results.
    pub limit: Option<usize>,
    /// Minimum similarity score (0-1).
    pub min_score: Option<f32>,
    /// Filter by session ID.
    pub session_id: Option<String>,
    /// Filter by role.
    pub role: Option<String>,
}

impl SearchOptions {
    /// Create new search options.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set result limit.
    pub fn limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Set minimum score.
    pub fn min_score(mut self, score: f32) -> Self {
        self.min_score = Some(score);
        self
    }

    /// Filter by session.
    pub fn session(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }

    /// Filter by role.
    pub fn role(mut self, role: impl Into<String>) -> Self {
        self.role = Some(role.into());
        self
    }
}

/// Statistics about memory storage.
#[derive(Debug, Clone, Default)]
pub struct MemoryStats {
    /// Total number of memories.
    pub total_memories: usize,
    /// Number of memories with embeddings.
    pub with_embeddings: usize,
    /// Number of unique sessions.
    pub unique_sessions: usize,
}
