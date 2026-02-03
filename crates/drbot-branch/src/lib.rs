//! Conversation branching and forking for drbot.
//!
//! Enables exploring alternative conversation paths, branching from any point,
//! and comparing different conversation outcomes.
//!
//! # Features
//!
//! - Branch conversations from any message
//! - Compare branches side by side
//! - Merge branches
//! - Track branch history and metadata
//!
//! # Example
//!
//! ```rust,no_run
//! use drbot_branch::{BranchManager, BranchPoint};
//!
//! async fn example() {
//!     let manager = BranchManager::new();
//!
//!     // Create a branch from a specific message
//!     let branch_id = manager.create_branch(
//!         "main",
//!         "experiment",
//!         5, // Branch from message index 5
//!     ).await.unwrap();
//! }
//! ```

mod branch;
mod diff;
mod manager;
mod storage;

pub use branch::{Branch, BranchMetadata, BranchPoint, BranchStatus};
pub use diff::{BranchDiff, DiffType, MessageDiff};
pub use manager::{BranchManager, BranchManagerConfig};
pub use storage::{BranchStorage, MemoryBranchStorage};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Result type for branch operations.
pub type Result<T> = std::result::Result<T, BranchError>;

/// Branch errors.
#[derive(Debug, thiserror::Error)]
pub enum BranchError {
    #[error("Branch not found: {0}")]
    NotFound(String),
    #[error("Invalid branch point: {0}")]
    InvalidBranchPoint(String),
    #[error("Cannot merge: {0}")]
    MergeConflict(String),
    #[error("Branch already exists: {0}")]
    AlreadyExists(String),
    #[error("Storage error: {0}")]
    StorageError(String),
    #[error("Invalid operation: {0}")]
    InvalidOperation(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_branch_manager_basic() {
        let manager = BranchManager::new();

        // Add some messages to main
        manager.add_message("main", "user", "Hello").await.unwrap();
        manager
            .add_message("main", "assistant", "Hi there!")
            .await
            .unwrap();

        // Create a branch
        let branch_id = manager
            .create_branch("main", "test-branch", 1)
            .await
            .unwrap();
        assert!(!branch_id.is_empty());

        // Add message to branch
        manager
            .add_message("test-branch", "user", "Different question")
            .await
            .unwrap();

        // List branches
        let branches = manager.list_branches().await.unwrap();
        assert!(branches.len() >= 2);
    }
}
