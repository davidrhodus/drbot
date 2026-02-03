//! Memory store implementation.

use crate::types::{Memory, MemorySearchResult, MemoryStats, SearchOptions};
use drbot_core::Result;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, info};
use uuid::Uuid;

/// Helper to convert rusqlite errors to drbot errors.
fn db_err(e: rusqlite::Error) -> drbot_core::Error {
    drbot_core::Error::Internal(format!("database error: {}", e))
}

/// Memory store backed by SQLite.
pub struct MemoryStore {
    /// Database connection.
    conn: Arc<Mutex<Connection>>,
    /// Embedding dimension (for vector search).
    embedding_dim: usize,
}

impl MemoryStore {
    /// Create a new memory store with file-based database.
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(path).map_err(db_err)?;
        Self::init_with_connection(conn)
    }

    /// Create an in-memory store.
    pub fn in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory().map_err(db_err)?;
        Self::init_with_connection(conn)
    }

    /// Initialize with existing connection.
    fn init_with_connection(conn: Connection) -> Result<Self> {
        // Initialize schema synchronously
        conn.execute(
            "CREATE TABLE IF NOT EXISTS memories (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                content TEXT NOT NULL,
                role TEXT NOT NULL,
                created_at TEXT NOT NULL,
                metadata TEXT DEFAULT '{}',
                embedding BLOB
            )",
            [],
        )
        .map_err(db_err)?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_memories_session ON memories(session_id)",
            [],
        )
        .map_err(db_err)?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_memories_created ON memories(created_at)",
            [],
        )
        .map_err(db_err)?;

        info!("Memory store schema initialized");

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            embedding_dim: 1536, // Default OpenAI embedding dimension
        })
    }

    /// Set the embedding dimension.
    pub fn with_embedding_dim(mut self, dim: usize) -> Self {
        self.embedding_dim = dim;
        self
    }

    /// Store a memory.
    pub async fn store(&self, memory: &Memory) -> Result<()> {
        let conn = self.conn.lock().await;

        let embedding_blob = memory
            .embedding
            .as_ref()
            .map(|e| Self::embedding_to_blob(e));

        conn.execute(
            "INSERT OR REPLACE INTO memories (id, session_id, content, role, created_at, metadata, embedding)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                memory.id.to_string(),
                memory.session_id,
                memory.content,
                memory.role,
                memory.created_at.to_rfc3339(),
                memory.metadata.to_string(),
                embedding_blob,
            ],
        )
        .map_err(db_err)?;

        debug!(id = %memory.id, session = %memory.session_id, "Stored memory");
        Ok(())
    }

    /// Get a memory by ID.
    pub async fn get(&self, id: Uuid) -> Result<Option<Memory>> {
        let conn = self.conn.lock().await;

        let result = conn
            .query_row(
                "SELECT id, session_id, content, role, created_at, metadata, embedding
                 FROM memories WHERE id = ?1",
                params![id.to_string()],
                |row| Self::row_to_memory(row),
            )
            .optional()
            .map_err(db_err)?;

        Ok(result)
    }

    /// Get recent memories for a session.
    pub async fn get_recent(&self, session_id: &str, limit: usize) -> Result<Vec<Memory>> {
        let conn = self.conn.lock().await;

        let mut stmt = conn
            .prepare(
                "SELECT id, session_id, content, role, created_at, metadata, embedding
                 FROM memories
                 WHERE session_id = ?1
                 ORDER BY created_at DESC
                 LIMIT ?2",
            )
            .map_err(db_err)?;

        let memories = stmt
            .query_map(params![session_id, limit], |row| Self::row_to_memory(row))
            .map_err(db_err)?
            .filter_map(|r| r.ok())
            .collect::<Vec<_>>();

        // Reverse to get chronological order
        let mut result = memories;
        result.reverse();

        Ok(result)
    }

    /// Search memories by vector similarity.
    pub async fn search(
        &self,
        query_embedding: &[f32],
        options: SearchOptions,
    ) -> Result<Vec<MemorySearchResult>> {
        let conn = self.conn.lock().await;

        let limit = options.limit.unwrap_or(10);
        let min_score = options.min_score.unwrap_or(0.0);

        // Use brute-force cosine similarity search
        self.search_brute_force(&conn, query_embedding, &options, limit, min_score)
    }

    /// Brute-force search using cosine similarity.
    fn search_brute_force(
        &self,
        conn: &Connection,
        query_embedding: &[f32],
        options: &SearchOptions,
        limit: usize,
        min_score: f32,
    ) -> Result<Vec<MemorySearchResult>> {
        let mut sql = String::from(
            "SELECT id, session_id, content, role, created_at, metadata, embedding
             FROM memories WHERE embedding IS NOT NULL",
        );

        if let Some(session) = &options.session_id {
            sql.push_str(&format!(" AND session_id = '{}'", session));
        }
        if let Some(role) = &options.role {
            sql.push_str(&format!(" AND role = '{}'", role));
        }

        let mut stmt = conn.prepare(&sql).map_err(db_err)?;
        let mut results: Vec<MemorySearchResult> = stmt
            .query_map([], |row| Self::row_to_memory(row))
            .map_err(db_err)?
            .filter_map(|r| r.ok())
            .filter_map(|memory| {
                if let Some(emb) = &memory.embedding {
                    let score = Self::cosine_similarity(query_embedding, emb);
                    Some(MemorySearchResult { memory, score })
                } else {
                    None
                }
            })
            .filter(|r| r.score >= min_score)
            .collect();

        // Sort by score descending
        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(limit);

        Ok(results)
    }

    /// Delete a memory by ID.
    pub async fn delete(&self, id: Uuid) -> Result<bool> {
        let conn = self.conn.lock().await;

        let deleted = conn
            .execute(
                "DELETE FROM memories WHERE id = ?1",
                params![id.to_string()],
            )
            .map_err(db_err)?;

        Ok(deleted > 0)
    }

    /// Delete all memories for a session.
    pub async fn delete_session(&self, session_id: &str) -> Result<usize> {
        let conn = self.conn.lock().await;

        let deleted = conn
            .execute(
                "DELETE FROM memories WHERE session_id = ?1",
                params![session_id],
            )
            .map_err(db_err)?;

        Ok(deleted)
    }

    /// Get storage statistics.
    pub async fn stats(&self) -> Result<MemoryStats> {
        let conn = self.conn.lock().await;

        let total_memories: usize = conn
            .query_row("SELECT COUNT(*) FROM memories", [], |row| row.get(0))
            .map_err(db_err)?;

        let with_embeddings: usize = conn
            .query_row(
                "SELECT COUNT(*) FROM memories WHERE embedding IS NOT NULL",
                [],
                |row| row.get(0),
            )
            .map_err(db_err)?;

        let unique_sessions: usize = conn
            .query_row(
                "SELECT COUNT(DISTINCT session_id) FROM memories",
                [],
                |row| row.get(0),
            )
            .map_err(db_err)?;

        Ok(MemoryStats {
            total_memories,
            with_embeddings,
            unique_sessions,
        })
    }

    /// Convert embedding to blob for storage.
    fn embedding_to_blob(embedding: &[f32]) -> Vec<u8> {
        embedding.iter().flat_map(|f| f.to_le_bytes()).collect()
    }

    /// Convert blob to embedding.
    fn blob_to_embedding(blob: &[u8]) -> Vec<f32> {
        blob.chunks_exact(4)
            .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
            .collect()
    }

    /// Calculate cosine similarity between two vectors.
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

    /// Convert database row to Memory.
    fn row_to_memory(row: &rusqlite::Row) -> rusqlite::Result<Memory> {
        let id_str: String = row.get(0)?;
        let session_id: String = row.get(1)?;
        let content: String = row.get(2)?;
        let role: String = row.get(3)?;
        let created_at_str: String = row.get(4)?;
        let metadata_str: String = row.get(5)?;
        let embedding_blob: Option<Vec<u8>> = row.get(6)?;

        let id = Uuid::parse_str(&id_str).unwrap_or_else(|_| Uuid::new_v4());
        let created_at = chrono::DateTime::parse_from_rfc3339(&created_at_str)
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or_else(|_| chrono::Utc::now());
        let metadata: serde_json::Value =
            serde_json::from_str(&metadata_str).unwrap_or(serde_json::Value::Null);
        let embedding = embedding_blob.map(|b| Self::blob_to_embedding(&b));

        Ok(Memory {
            id,
            session_id,
            content,
            role,
            embedding,
            created_at,
            metadata,
        })
    }
}
