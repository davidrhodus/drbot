//! Storage backends for the cache.

use crate::embedding::Embedding;
use crate::{CacheEntry, CacheError, Result};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::debug;

/// Trait for cache storage backends.
#[async_trait]
pub trait CacheStorage: Send + Sync {
    /// Store an entry.
    async fn store(&self, key: &str, entry: CacheEntry) -> Result<()>;

    /// Retrieve an entry by exact key.
    async fn get(&self, key: &str) -> Result<Option<CacheEntry>>;

    /// Get all entries with their embeddings for similarity search.
    async fn get_all_with_embeddings(&self) -> Result<Vec<(String, Embedding, CacheEntry)>>;

    /// Delete an entry.
    async fn delete(&self, key: &str) -> Result<()>;

    /// Clear all entries.
    async fn clear(&self) -> Result<()>;

    /// Get number of entries.
    async fn len(&self) -> Result<usize>;

    /// Check if empty.
    async fn is_empty(&self) -> Result<bool> {
        Ok(self.len().await? == 0)
    }

    /// Evict entries based on TTL.
    async fn evict_expired(&self) -> Result<usize>;
}

/// In-memory storage backend.
#[derive(Debug, Default)]
pub struct MemoryStorage {
    entries: Arc<RwLock<HashMap<String, (Embedding, CacheEntry)>>>,
}

impl MemoryStorage {
    /// Create a new memory storage.
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl CacheStorage for MemoryStorage {
    async fn store(&self, key: &str, entry: CacheEntry) -> Result<()> {
        let mut entries = self.entries.write().await;
        entries.insert(key.to_string(), (entry.embedding.clone(), entry));
        Ok(())
    }

    async fn get(&self, key: &str) -> Result<Option<CacheEntry>> {
        let entries = self.entries.read().await;
        Ok(entries.get(key).map(|(_, e)| e.clone()))
    }

    async fn get_all_with_embeddings(&self) -> Result<Vec<(String, Embedding, CacheEntry)>> {
        let entries = self.entries.read().await;
        Ok(entries
            .iter()
            .map(|(k, (emb, e))| (k.clone(), emb.clone(), e.clone()))
            .collect())
    }

    async fn delete(&self, key: &str) -> Result<()> {
        let mut entries = self.entries.write().await;
        entries.remove(key);
        Ok(())
    }

    async fn clear(&self) -> Result<()> {
        let mut entries = self.entries.write().await;
        entries.clear();
        Ok(())
    }

    async fn len(&self) -> Result<usize> {
        let entries = self.entries.read().await;
        Ok(entries.len())
    }

    async fn evict_expired(&self) -> Result<usize> {
        let now = chrono::Utc::now();
        let mut entries = self.entries.write().await;
        let before = entries.len();

        entries.retain(|_, (_, e)| {
            if let Some(expires_at) = e.expires_at {
                expires_at > now
            } else {
                true
            }
        });

        let evicted = before - entries.len();
        if evicted > 0 {
            debug!("Evicted {} expired entries", evicted);
        }
        Ok(evicted)
    }
}

/// SQLite storage backend.
pub struct SqliteStorage {
    // In a real implementation, this would use rusqlite
    memory_fallback: MemoryStorage,
}

impl SqliteStorage {
    /// Create a new SQLite storage.
    pub fn new(_path: &str) -> Result<Self> {
        // For now, fall back to memory storage
        Ok(Self {
            memory_fallback: MemoryStorage::new(),
        })
    }

    /// Create an in-memory SQLite database.
    pub fn in_memory() -> Result<Self> {
        Self::new(":memory:")
    }
}

#[async_trait]
impl CacheStorage for SqliteStorage {
    async fn store(&self, key: &str, entry: CacheEntry) -> Result<()> {
        self.memory_fallback.store(key, entry).await
    }

    async fn get(&self, key: &str) -> Result<Option<CacheEntry>> {
        self.memory_fallback.get(key).await
    }

    async fn get_all_with_embeddings(&self) -> Result<Vec<(String, Embedding, CacheEntry)>> {
        self.memory_fallback.get_all_with_embeddings().await
    }

    async fn delete(&self, key: &str) -> Result<()> {
        self.memory_fallback.delete(key).await
    }

    async fn clear(&self) -> Result<()> {
        self.memory_fallback.clear().await
    }

    async fn len(&self) -> Result<usize> {
        self.memory_fallback.len().await
    }

    async fn evict_expired(&self) -> Result<usize> {
        self.memory_fallback.evict_expired().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_memory_storage_basic() {
        let storage = MemoryStorage::new();

        let entry = CacheEntry {
            query: "test".to_string(),
            content: "response".to_string(),
            embedding: vec![1.0, 0.0, 0.0],
            created_at: chrono::Utc::now(),
            expires_at: None,
            hits: 0,
            metadata: Default::default(),
        };

        storage.store("test", entry.clone()).await.unwrap();
        assert_eq!(storage.len().await.unwrap(), 1);

        let retrieved = storage.get("test").await.unwrap().unwrap();
        assert_eq!(retrieved.content, "response");

        storage.delete("test").await.unwrap();
        assert_eq!(storage.len().await.unwrap(), 0);
    }

    #[tokio::test]
    async fn test_evict_expired() {
        let storage = MemoryStorage::new();

        let expired_entry = CacheEntry {
            query: "expired".to_string(),
            content: "old".to_string(),
            embedding: vec![1.0],
            created_at: chrono::Utc::now() - chrono::Duration::hours(2),
            expires_at: Some(chrono::Utc::now() - chrono::Duration::hours(1)),
            hits: 0,
            metadata: Default::default(),
        };

        let valid_entry = CacheEntry {
            query: "valid".to_string(),
            content: "new".to_string(),
            embedding: vec![0.0, 1.0],
            created_at: chrono::Utc::now(),
            expires_at: Some(chrono::Utc::now() + chrono::Duration::hours(1)),
            hits: 0,
            metadata: Default::default(),
        };

        storage.store("expired", expired_entry).await.unwrap();
        storage.store("valid", valid_entry).await.unwrap();
        assert_eq!(storage.len().await.unwrap(), 2);

        let evicted = storage.evict_expired().await.unwrap();
        assert_eq!(evicted, 1);
        assert_eq!(storage.len().await.unwrap(), 1);
    }
}
