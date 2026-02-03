//! Knowledge storage backend.

use crate::{KnowledgeEntry, KnowledgeError, KnowledgeStats, Result};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Mutex;
use uuid::Uuid;

/// A document in the knowledge base.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    /// Unique document ID.
    pub id: Uuid,
    /// Document title.
    pub title: String,
    /// Document content.
    pub content: String,
    /// Document metadata.
    pub metadata: DocumentMetadata,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Last update timestamp.
    pub updated_at: DateTime<Utc>,
}

impl Document {
    /// Create a new document.
    pub fn new(title: impl Into<String>, content: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            title: title.into(),
            content: content.into(),
            metadata: DocumentMetadata::default(),
            created_at: now,
            updated_at: now,
        }
    }

    /// Set metadata.
    pub fn with_metadata(mut self, metadata: DocumentMetadata) -> Self {
        self.metadata = metadata;
        self
    }
}

/// Document metadata.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DocumentMetadata {
    /// Source type (file, url, conversation, etc.).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_type: Option<String>,
    /// Original source path or URL.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// MIME type.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    /// Author.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    /// Tags.
    #[serde(default)]
    pub tags: Vec<String>,
}

/// SQLite-backed knowledge store.
pub struct KnowledgeStore {
    conn: Mutex<Connection>,
}

impl KnowledgeStore {
    /// Create or open a knowledge store.
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(path).map_err(|e| KnowledgeError::Storage(e.to_string()))?;

        // Create tables
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS documents (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                content TEXT NOT NULL,
                metadata TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS entries (
                id TEXT PRIMARY KEY,
                document_id TEXT NOT NULL,
                content TEXT NOT NULL,
                embedding BLOB,
                metadata TEXT NOT NULL,
                created_at TEXT NOT NULL,
                FOREIGN KEY (document_id) REFERENCES documents(id) ON DELETE CASCADE
            );

            CREATE INDEX IF NOT EXISTS idx_entries_document ON entries(document_id);
            CREATE VIRTUAL TABLE IF NOT EXISTS entries_fts USING fts5(content, content=entries, content_rowid=rowid);
            ",
        )
        .map_err(|e| KnowledgeError::Storage(e.to_string()))?;

        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Create an in-memory store.
    pub fn in_memory() -> Result<Self> {
        Self::new(":memory:")
    }

    /// Add a document.
    pub async fn add_document(&self, document: &Document) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        let metadata_json = serde_json::to_string(&document.metadata)
            .map_err(|e| KnowledgeError::Storage(e.to_string()))?;

        conn.execute(
            "INSERT INTO documents (id, title, content, metadata, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                document.id.to_string(),
                document.title,
                document.content,
                metadata_json,
                document.created_at.to_rfc3339(),
                document.updated_at.to_rfc3339(),
            ],
        )
        .map_err(|e| KnowledgeError::Storage(e.to_string()))?;

        Ok(())
    }

    /// Add a knowledge entry.
    pub async fn add_entry(&self, entry: &KnowledgeEntry) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        let metadata_json = serde_json::to_string(&entry.metadata)
            .map_err(|e| KnowledgeError::Storage(e.to_string()))?;

        let embedding_bytes: Option<Vec<u8>> = entry
            .embedding
            .as_ref()
            .map(|e| e.iter().flat_map(|f| f.to_le_bytes()).collect());

        conn.execute(
            "INSERT INTO entries (id, document_id, content, embedding, metadata, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                entry.id.to_string(),
                entry.document_id.to_string(),
                entry.content,
                embedding_bytes,
                metadata_json,
                entry.created_at.to_rfc3339(),
            ],
        )
        .map_err(|e| KnowledgeError::Storage(e.to_string()))?;

        // Update FTS index
        conn.execute(
            "INSERT INTO entries_fts(rowid, content) VALUES (last_insert_rowid(), ?1)",
            params![entry.content],
        )
        .map_err(|e| KnowledgeError::Storage(e.to_string()))?;

        Ok(())
    }

    /// Get a document by ID.
    pub async fn get_document(&self, id: Uuid) -> Result<Option<Document>> {
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn
            .prepare("SELECT id, title, content, metadata, created_at, updated_at FROM documents WHERE id = ?1")
            .map_err(|e| KnowledgeError::Storage(e.to_string()))?;

        let result = stmt.query_row(params![id.to_string()], |row| {
            let id_str: String = row.get(0)?;
            let title: String = row.get(1)?;
            let content: String = row.get(2)?;
            let metadata_json: String = row.get(3)?;
            let created_at_str: String = row.get(4)?;
            let updated_at_str: String = row.get(5)?;

            Ok((
                id_str,
                title,
                content,
                metadata_json,
                created_at_str,
                updated_at_str,
            ))
        });

        match result {
            Ok((id_str, title, content, metadata_json, created_at_str, updated_at_str)) => {
                let metadata: DocumentMetadata = serde_json::from_str(&metadata_json)
                    .map_err(|e| KnowledgeError::Storage(e.to_string()))?;

                Ok(Some(Document {
                    id: Uuid::parse_str(&id_str)
                        .map_err(|e| KnowledgeError::Storage(e.to_string()))?,
                    title,
                    content,
                    metadata,
                    created_at: DateTime::parse_from_rfc3339(&created_at_str)
                        .map_err(|e| KnowledgeError::Storage(e.to_string()))?
                        .with_timezone(&Utc),
                    updated_at: DateTime::parse_from_rfc3339(&updated_at_str)
                        .map_err(|e| KnowledgeError::Storage(e.to_string()))?
                        .with_timezone(&Utc),
                }))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(KnowledgeError::Storage(e.to_string())),
        }
    }

    /// Delete a document and its entries.
    pub async fn delete_document(&self, id: Uuid) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        conn.execute(
            "DELETE FROM entries WHERE document_id = ?1",
            params![id.to_string()],
        )
        .map_err(|e| KnowledgeError::Storage(e.to_string()))?;

        conn.execute(
            "DELETE FROM documents WHERE id = ?1",
            params![id.to_string()],
        )
        .map_err(|e| KnowledgeError::Storage(e.to_string()))?;

        Ok(())
    }

    /// List all documents.
    pub async fn list_documents(&self) -> Result<Vec<Document>> {
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn
            .prepare("SELECT id, title, content, metadata, created_at, updated_at FROM documents ORDER BY created_at DESC")
            .map_err(|e| KnowledgeError::Storage(e.to_string()))?;

        let documents = stmt
            .query_map([], |row| {
                let id_str: String = row.get(0)?;
                let title: String = row.get(1)?;
                let content: String = row.get(2)?;
                let metadata_json: String = row.get(3)?;
                let created_at_str: String = row.get(4)?;
                let updated_at_str: String = row.get(5)?;

                Ok((
                    id_str,
                    title,
                    content,
                    metadata_json,
                    created_at_str,
                    updated_at_str,
                ))
            })
            .map_err(|e| KnowledgeError::Storage(e.to_string()))?
            .filter_map(|r| r.ok())
            .filter_map(
                |(id_str, title, content, metadata_json, created_at_str, updated_at_str)| {
                    let metadata: DocumentMetadata = serde_json::from_str(&metadata_json).ok()?;
                    Some(Document {
                        id: Uuid::parse_str(&id_str).ok()?,
                        title,
                        content,
                        metadata,
                        created_at: DateTime::parse_from_rfc3339(&created_at_str)
                            .ok()?
                            .with_timezone(&Utc),
                        updated_at: DateTime::parse_from_rfc3339(&updated_at_str)
                            .ok()?
                            .with_timezone(&Utc),
                    })
                },
            )
            .collect();

        Ok(documents)
    }

    /// Get all entries for a document.
    pub async fn get_entries(&self, document_id: Uuid) -> Result<Vec<KnowledgeEntry>> {
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn
            .prepare("SELECT id, document_id, content, embedding, metadata, created_at FROM entries WHERE document_id = ?1")
            .map_err(|e| KnowledgeError::Storage(e.to_string()))?;

        let entries = stmt
            .query_map(params![document_id.to_string()], |row| {
                let id_str: String = row.get(0)?;
                let doc_id_str: String = row.get(1)?;
                let content: String = row.get(2)?;
                let embedding_bytes: Option<Vec<u8>> = row.get(3)?;
                let metadata_json: String = row.get(4)?;
                let created_at_str: String = row.get(5)?;

                Ok((
                    id_str,
                    doc_id_str,
                    content,
                    embedding_bytes,
                    metadata_json,
                    created_at_str,
                ))
            })
            .map_err(|e| KnowledgeError::Storage(e.to_string()))?
            .filter_map(|r| r.ok())
            .filter_map(
                |(id_str, doc_id_str, content, embedding_bytes, metadata_json, created_at_str)| {
                    let embedding: Option<Vec<f32>> = embedding_bytes.map(|bytes| {
                        bytes
                            .chunks(4)
                            .filter_map(|chunk| {
                                if chunk.len() == 4 {
                                    Some(f32::from_le_bytes([
                                        chunk[0], chunk[1], chunk[2], chunk[3],
                                    ]))
                                } else {
                                    None
                                }
                            })
                            .collect()
                    });

                    let metadata: crate::EntryMetadata =
                        serde_json::from_str(&metadata_json).ok()?;

                    Some(KnowledgeEntry {
                        id: Uuid::parse_str(&id_str).ok()?,
                        document_id: Uuid::parse_str(&doc_id_str).ok()?,
                        content,
                        embedding,
                        metadata,
                        created_at: DateTime::parse_from_rfc3339(&created_at_str)
                            .ok()?
                            .with_timezone(&Utc),
                    })
                },
            )
            .collect();

        Ok(entries)
    }

    /// Full-text search entries.
    pub async fn fts_search(&self, query: &str, limit: usize) -> Result<Vec<KnowledgeEntry>> {
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn
            .prepare(
                "SELECT e.id, e.document_id, e.content, e.embedding, e.metadata, e.created_at
                 FROM entries e
                 JOIN entries_fts fts ON e.rowid = fts.rowid
                 WHERE entries_fts MATCH ?1
                 ORDER BY rank
                 LIMIT ?2",
            )
            .map_err(|e| KnowledgeError::Storage(e.to_string()))?;

        let entries = stmt
            .query_map(params![query, limit as i64], |row| {
                let id_str: String = row.get(0)?;
                let doc_id_str: String = row.get(1)?;
                let content: String = row.get(2)?;
                let embedding_bytes: Option<Vec<u8>> = row.get(3)?;
                let metadata_json: String = row.get(4)?;
                let created_at_str: String = row.get(5)?;

                Ok((
                    id_str,
                    doc_id_str,
                    content,
                    embedding_bytes,
                    metadata_json,
                    created_at_str,
                ))
            })
            .map_err(|e| KnowledgeError::Storage(e.to_string()))?
            .filter_map(|r| r.ok())
            .filter_map(
                |(id_str, doc_id_str, content, embedding_bytes, metadata_json, created_at_str)| {
                    let embedding: Option<Vec<f32>> = embedding_bytes.map(|bytes| {
                        bytes
                            .chunks(4)
                            .filter_map(|chunk| {
                                if chunk.len() == 4 {
                                    Some(f32::from_le_bytes([
                                        chunk[0], chunk[1], chunk[2], chunk[3],
                                    ]))
                                } else {
                                    None
                                }
                            })
                            .collect()
                    });

                    let metadata: crate::EntryMetadata =
                        serde_json::from_str(&metadata_json).ok()?;

                    Some(KnowledgeEntry {
                        id: Uuid::parse_str(&id_str).ok()?,
                        document_id: Uuid::parse_str(&doc_id_str).ok()?,
                        content,
                        embedding,
                        metadata,
                        created_at: DateTime::parse_from_rfc3339(&created_at_str)
                            .ok()?
                            .with_timezone(&Utc),
                    })
                },
            )
            .collect();

        Ok(entries)
    }

    /// Get all entries with embeddings (for vector search).
    pub async fn get_all_entries(&self) -> Result<Vec<KnowledgeEntry>> {
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn
            .prepare(
                "SELECT id, document_id, content, embedding, metadata, created_at FROM entries",
            )
            .map_err(|e| KnowledgeError::Storage(e.to_string()))?;

        let entries = stmt
            .query_map([], |row| {
                let id_str: String = row.get(0)?;
                let doc_id_str: String = row.get(1)?;
                let content: String = row.get(2)?;
                let embedding_bytes: Option<Vec<u8>> = row.get(3)?;
                let metadata_json: String = row.get(4)?;
                let created_at_str: String = row.get(5)?;

                Ok((
                    id_str,
                    doc_id_str,
                    content,
                    embedding_bytes,
                    metadata_json,
                    created_at_str,
                ))
            })
            .map_err(|e| KnowledgeError::Storage(e.to_string()))?
            .filter_map(|r| r.ok())
            .filter_map(
                |(id_str, doc_id_str, content, embedding_bytes, metadata_json, created_at_str)| {
                    let embedding: Option<Vec<f32>> = embedding_bytes.map(|bytes| {
                        bytes
                            .chunks(4)
                            .filter_map(|chunk| {
                                if chunk.len() == 4 {
                                    Some(f32::from_le_bytes([
                                        chunk[0], chunk[1], chunk[2], chunk[3],
                                    ]))
                                } else {
                                    None
                                }
                            })
                            .collect()
                    });

                    let metadata: crate::EntryMetadata =
                        serde_json::from_str(&metadata_json).ok()?;

                    Some(KnowledgeEntry {
                        id: Uuid::parse_str(&id_str).ok()?,
                        document_id: Uuid::parse_str(&doc_id_str).ok()?,
                        content,
                        embedding,
                        metadata,
                        created_at: DateTime::parse_from_rfc3339(&created_at_str)
                            .ok()?
                            .with_timezone(&Utc),
                    })
                },
            )
            .collect();

        Ok(entries)
    }

    /// Get statistics.
    pub async fn stats(&self) -> Result<KnowledgeStats> {
        let conn = self.conn.lock().unwrap();

        let doc_count: usize = conn
            .query_row("SELECT COUNT(*) FROM documents", [], |row| row.get(0))
            .map_err(|e| KnowledgeError::Storage(e.to_string()))?;

        let entry_count: usize = conn
            .query_row("SELECT COUNT(*) FROM entries", [], |row| row.get(0))
            .map_err(|e| KnowledgeError::Storage(e.to_string()))?;

        let total_size: usize = conn
            .query_row(
                "SELECT COALESCE(SUM(LENGTH(content)), 0) FROM entries",
                [],
                |row| row.get(0),
            )
            .map_err(|e| KnowledgeError::Storage(e.to_string()))?;

        Ok(KnowledgeStats {
            document_count: doc_count,
            entry_count,
            total_size,
            avg_entries_per_doc: if doc_count > 0 {
                entry_count as f32 / doc_count as f32
            } else {
                0.0
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_store_document() {
        let store = KnowledgeStore::in_memory().unwrap();

        let doc = Document::new("Test", "This is test content");
        store.add_document(&doc).await.unwrap();

        let retrieved = store.get_document(doc.id).await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().title, "Test");
    }

    #[tokio::test]
    async fn test_stats() {
        let store = KnowledgeStore::in_memory().unwrap();

        let stats = store.stats().await.unwrap();
        assert_eq!(stats.document_count, 0);
    }
}
