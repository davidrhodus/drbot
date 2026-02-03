//! Storage backend for branches.

use crate::branch::Branch;
use crate::{BranchError, Result};
use async_trait::async_trait;
use std::collections::HashMap;
use tokio::sync::RwLock;

/// Storage trait for branches.
#[async_trait]
pub trait BranchStorage: Send + Sync {
    /// Save a branch.
    async fn save_branch(&self, branch: &Branch) -> Result<()>;

    /// Get a branch by name.
    async fn get_branch(&self, name: &str) -> Result<Option<Branch>>;

    /// List all branches.
    async fn list_branches(&self) -> Result<Vec<Branch>>;

    /// Delete a branch.
    async fn delete_branch(&self, name: &str) -> Result<()>;

    /// Check if a branch exists.
    async fn branch_exists(&self, name: &str) -> Result<bool> {
        Ok(self.get_branch(name).await?.is_some())
    }
}

/// In-memory branch storage.
#[derive(Default)]
pub struct MemoryBranchStorage {
    branches: RwLock<HashMap<String, Branch>>,
}

impl MemoryBranchStorage {
    /// Create new memory storage.
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl BranchStorage for MemoryBranchStorage {
    async fn save_branch(&self, branch: &Branch) -> Result<()> {
        let mut branches = self.branches.write().await;
        branches.insert(branch.name.clone(), branch.clone());
        Ok(())
    }

    async fn get_branch(&self, name: &str) -> Result<Option<Branch>> {
        let branches = self.branches.read().await;
        Ok(branches.get(name).cloned())
    }

    async fn list_branches(&self) -> Result<Vec<Branch>> {
        let branches = self.branches.read().await;
        Ok(branches.values().cloned().collect())
    }

    async fn delete_branch(&self, name: &str) -> Result<()> {
        let mut branches = self.branches.write().await;
        branches.remove(name);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_memory_storage() {
        let storage = MemoryBranchStorage::new();

        // Save a branch
        let branch = Branch::new("test");
        storage.save_branch(&branch).await.unwrap();

        // Get it back
        let loaded = storage.get_branch("test").await.unwrap();
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().name, "test");

        // List
        let all = storage.list_branches().await.unwrap();
        assert_eq!(all.len(), 1);

        // Delete
        storage.delete_branch("test").await.unwrap();
        let deleted = storage.get_branch("test").await.unwrap();
        assert!(deleted.is_none());
    }

    #[tokio::test]
    async fn test_branch_exists() {
        let storage = MemoryBranchStorage::new();

        let branch = Branch::new("exists");
        storage.save_branch(&branch).await.unwrap();

        assert!(storage.branch_exists("exists").await.unwrap());
        assert!(!storage.branch_exists("not-exists").await.unwrap());
    }
}
