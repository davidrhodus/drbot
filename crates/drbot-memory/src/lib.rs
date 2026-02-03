//! Context and memory storage for drbot.
//!
//! This crate provides persistent storage for conversation history and
//! semantic search using vector embeddings.
//!
//! # Features
//!
//! - Store conversation memories with metadata
//! - Vector similarity search (using sqlite-vec if available)
//! - Session-based memory organization
//! - Efficient storage using SQLite
//! - Long-term cross-session memory
//! - Memory importance scoring and retention
//! - Semantic deduplication

mod layers;
mod longterm;
mod store;
mod types;

pub use layers::{
    HierarchicalMemory, HierarchicalMemoryConfig, LayeredMemory, MemoryLayer, MemorySource,
    MemoryStats, RecallOptions, RecallResult, SourceType,
};
pub use longterm::{
    Importance, LongTermMemory, LongTermMemoryStore, MemoryCategory, MemoryConsolidator,
    UserMemoryStats,
};
pub use store::MemoryStore;
pub use types::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_memory_creation() {
        let memory = Memory::new("session-1", "user", "Hello, how are you?");
        assert_eq!(memory.session_id, "session-1");
        assert_eq!(memory.role, "user");
        assert_eq!(memory.content, "Hello, how are you?");
        assert!(memory.embedding.is_none());
    }

    #[tokio::test]
    async fn test_memory_with_embedding() {
        let embedding = vec![0.1, 0.2, 0.3, 0.4];
        let memory = Memory::new("session-1", "assistant", "I'm doing well!")
            .with_embedding(embedding.clone());

        assert!(memory.embedding.is_some());
        assert_eq!(memory.embedding.unwrap(), embedding);
    }

    #[tokio::test]
    async fn test_store_and_retrieve() {
        let store = MemoryStore::in_memory().unwrap();

        let memory = Memory::new("test-session", "user", "Test message");
        let id = memory.id;

        store.store(&memory).await.unwrap();

        let retrieved = store.get(id).await.unwrap();
        assert!(retrieved.is_some());

        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.content, "Test message");
        assert_eq!(retrieved.session_id, "test-session");
    }

    #[tokio::test]
    async fn test_get_recent() {
        let store = MemoryStore::in_memory().unwrap();

        // Store multiple memories
        for i in 0..5 {
            let memory = Memory::new("session-1", "user", format!("Message {}", i));
            store.store(&memory).await.unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }

        let recent = store.get_recent("session-1", 3).await.unwrap();
        assert_eq!(recent.len(), 3);

        // Should be in chronological order (oldest first)
        assert!(recent[0].content.contains("2"));
        assert!(recent[1].content.contains("3"));
        assert!(recent[2].content.contains("4"));
    }

    #[tokio::test]
    async fn test_delete() {
        let store = MemoryStore::in_memory().unwrap();

        let memory = Memory::new("session-1", "user", "To be deleted");
        let id = memory.id;

        store.store(&memory).await.unwrap();
        assert!(store.get(id).await.unwrap().is_some());

        let deleted = store.delete(id).await.unwrap();
        assert!(deleted);

        assert!(store.get(id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_delete_session() {
        let store = MemoryStore::in_memory().unwrap();

        // Store memories in different sessions
        for i in 0..3 {
            store
                .store(&Memory::new("session-a", "user", format!("Msg A{}", i)))
                .await
                .unwrap();
            store
                .store(&Memory::new("session-b", "user", format!("Msg B{}", i)))
                .await
                .unwrap();
        }

        let deleted = store.delete_session("session-a").await.unwrap();
        assert_eq!(deleted, 3);

        let remaining = store.get_recent("session-a", 10).await.unwrap();
        assert!(remaining.is_empty());

        let remaining = store.get_recent("session-b", 10).await.unwrap();
        assert_eq!(remaining.len(), 3);
    }

    #[tokio::test]
    async fn test_stats() {
        let store = MemoryStore::in_memory().unwrap();

        store
            .store(&Memory::new("s1", "user", "Msg 1"))
            .await
            .unwrap();
        store
            .store(&Memory::new("s1", "assistant", "Msg 2"))
            .await
            .unwrap();
        store
            .store(&Memory::new("s2", "user", "Msg 3"))
            .await
            .unwrap();

        let stats = store.stats().await.unwrap();
        assert_eq!(stats.total_memories, 3);
        assert_eq!(stats.unique_sessions, 2);
    }

    #[tokio::test]
    async fn test_search_brute_force() {
        let store = MemoryStore::in_memory().unwrap();

        // Store memories with embeddings
        let emb1 = vec![1.0, 0.0, 0.0, 0.0];
        let emb2 = vec![0.9, 0.1, 0.0, 0.0];
        let emb3 = vec![0.0, 1.0, 0.0, 0.0];

        store
            .store(&Memory::new("s1", "user", "Similar to query").with_embedding(emb1))
            .await
            .unwrap();
        store
            .store(&Memory::new("s1", "user", "Also similar").with_embedding(emb2))
            .await
            .unwrap();
        store
            .store(&Memory::new("s1", "user", "Not similar").with_embedding(emb3))
            .await
            .unwrap();

        // Query with embedding similar to emb1
        let query = vec![1.0, 0.0, 0.0, 0.0];
        let results = store
            .search(&query, SearchOptions::new().limit(2))
            .await
            .unwrap();

        assert_eq!(results.len(), 2);
        assert!(results[0].score > results[1].score);
        assert_eq!(results[0].memory.content, "Similar to query");
    }

    #[test]
    fn test_search_options() {
        let opts = SearchOptions::new()
            .limit(5)
            .min_score(0.5)
            .session("session-1")
            .role("user");

        assert_eq!(opts.limit, Some(5));
        assert_eq!(opts.min_score, Some(0.5));
        assert_eq!(opts.session_id, Some("session-1".to_string()));
        assert_eq!(opts.role, Some("user".to_string()));
    }
}
