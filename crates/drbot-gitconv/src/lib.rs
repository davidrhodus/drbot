//! Git-native conversation storage for drbot.
//!
//! Stores conversations in a git repository with full version history,
//! branching, and diff capabilities.
//!
//! # Features
//!
//! - Conversations stored as markdown/JSON files
//! - Full git history for all changes
//! - Branching for conversation forks
//! - Diff between conversation versions
//! - Search across conversation history
//!
//! # Example
//!
//! ```rust,no_run
//! use drbot_gitconv::{GitConversationStore, Conversation};
//!
//! async fn example() {
//!     let store = GitConversationStore::open("~/.drbot/conversations").await.unwrap();
//!
//!     // Create a new conversation
//!     let conv = store.create("New Chat").await.unwrap();
//!
//!     // Add messages
//!     store.add_message(&conv.id, "user", "Hello!").await.unwrap();
//!     store.add_message(&conv.id, "assistant", "Hi there!").await.unwrap();
//!
//!     // Commit changes
//!     store.commit(&conv.id, "Added greeting").await.unwrap();
//! }
//! ```

mod conversation;
mod diff;
mod message;
mod store;

pub use conversation::{Conversation, ConversationBranch, ConversationMetadata};
pub use diff::{ConversationDiff, DiffEntry, DiffType};
pub use message::{Attachment, Message, MessageRole};
pub use store::{GitConversationStore, StoreConfig};

/// Result type.
pub type Result<T> = std::result::Result<T, GitConvError>;

/// Git conversation errors.
#[derive(Debug, thiserror::Error)]
pub enum GitConvError {
    #[error("Repository error: {0}")]
    RepoError(String),
    #[error("Conversation not found: {0}")]
    NotFound(String),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    SerializeError(String),
    #[error("Git error: {0}")]
    GitError(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_store_creation() {
        let temp_dir = std::env::temp_dir().join("drbot-gitconv-test");
        let _ = std::fs::remove_dir_all(&temp_dir);

        let store = GitConversationStore::open(&temp_dir).await.unwrap();
        assert!(store.list().await.unwrap().is_empty());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
