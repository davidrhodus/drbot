//! Branch manager for managing conversation branches.

use crate::branch::{Branch, BranchMessage, BranchMetadata, BranchPoint, BranchStatus};
use crate::diff::{BranchDiff, DiffType, MessageDiff};
use crate::storage::{BranchStorage, MemoryBranchStorage};
use crate::{BranchError, Result};
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};
use uuid::Uuid;

/// Configuration for the branch manager.
#[derive(Debug, Clone)]
pub struct BranchManagerConfig {
    /// Default branch name.
    pub default_branch: String,
    /// Maximum branches per conversation.
    pub max_branches: usize,
    /// Auto-create default branch if not exists.
    pub auto_create_default: bool,
    /// Keep deleted branches for history.
    pub keep_deleted: bool,
}

impl Default for BranchManagerConfig {
    fn default() -> Self {
        Self {
            default_branch: "main".to_string(),
            max_branches: 100,
            auto_create_default: true,
            keep_deleted: true,
        }
    }
}

/// Manager for conversation branches.
pub struct BranchManager {
    config: BranchManagerConfig,
    storage: Arc<dyn BranchStorage>,
}

impl BranchManager {
    /// Create a new branch manager.
    pub fn new() -> Self {
        Self::with_config(BranchManagerConfig::default())
    }

    /// Create with custom config.
    pub fn with_config(config: BranchManagerConfig) -> Self {
        let storage = Arc::new(MemoryBranchStorage::new());
        Self::with_storage(config, storage)
    }

    /// Create with custom storage.
    pub fn with_storage(config: BranchManagerConfig, storage: Arc<dyn BranchStorage>) -> Self {
        Self { config, storage }
    }

    /// Ensure default branch exists.
    async fn ensure_default_branch(&self) -> Result<()> {
        if self.config.auto_create_default {
            if self
                .storage
                .get_branch(&self.config.default_branch)
                .await?
                .is_none()
            {
                let branch = Branch::new(&self.config.default_branch);
                self.storage.save_branch(&branch).await?;
            }
        }
        Ok(())
    }

    /// Get a branch by name.
    pub async fn get_branch(&self, name: &str) -> Result<Option<Branch>> {
        self.storage.get_branch(name).await
    }

    /// Get the default branch.
    pub async fn get_default_branch(&self) -> Result<Branch> {
        self.ensure_default_branch().await?;
        self.storage
            .get_branch(&self.config.default_branch)
            .await?
            .ok_or_else(|| BranchError::NotFound(self.config.default_branch.clone()))
    }

    /// Create a new branch from a parent branch.
    pub async fn create_branch(
        &self,
        parent_name: &str,
        new_name: &str,
        message_index: usize,
    ) -> Result<String> {
        // Check if branch already exists
        if self.storage.get_branch(new_name).await?.is_some() {
            return Err(BranchError::AlreadyExists(new_name.to_string()));
        }

        // Get parent branch
        self.ensure_default_branch().await?;
        let parent = self
            .storage
            .get_branch(parent_name)
            .await?
            .ok_or_else(|| BranchError::NotFound(parent_name.to_string()))?;

        // Validate branch point
        if message_index >= parent.messages.len() && !parent.messages.is_empty() {
            return Err(BranchError::InvalidBranchPoint(format!(
                "Message index {} is out of bounds (max: {})",
                message_index,
                parent.messages.len() - 1
            )));
        }

        // Check max branches
        let branch_count = self.storage.list_branches().await?.len();
        if branch_count >= self.config.max_branches {
            return Err(BranchError::InvalidOperation(format!(
                "Maximum number of branches ({}) reached",
                self.config.max_branches
            )));
        }

        // Create the new branch
        let branch = Branch::from_parent(new_name, &parent, message_index);
        self.storage.save_branch(&branch).await?;

        info!(
            branch = new_name,
            parent = parent_name,
            "Created new branch"
        );
        Ok(new_name.to_string())
    }

    /// Create a new empty branch.
    pub async fn create_empty_branch(&self, name: &str) -> Result<String> {
        if self.storage.get_branch(name).await?.is_some() {
            return Err(BranchError::AlreadyExists(name.to_string()));
        }

        let branch = Branch::new(name);
        self.storage.save_branch(&branch).await?;

        info!(branch = name, "Created empty branch");
        Ok(name.to_string())
    }

    /// Add a message to a branch.
    pub async fn add_message(&self, branch_name: &str, role: &str, content: &str) -> Result<Uuid> {
        self.ensure_default_branch().await?;

        let mut branch = self
            .storage
            .get_branch(branch_name)
            .await?
            .ok_or_else(|| BranchError::NotFound(branch_name.to_string()))?;

        if !branch.is_active() {
            return Err(BranchError::InvalidOperation(format!(
                "Cannot add messages to {} branch",
                match branch.status {
                    BranchStatus::Archived => "archived",
                    BranchStatus::Merged => "merged",
                    BranchStatus::Deleted => "deleted",
                    _ => "inactive",
                }
            )));
        }

        let message = BranchMessage::new(role, content);
        let message_id = message.id;
        branch.messages.push(message);
        branch.updated_at = Utc::now();

        self.storage.save_branch(&branch).await?;

        debug!(branch = branch_name, message_id = %message_id, "Added message to branch");
        Ok(message_id)
    }

    /// List all branches.
    pub async fn list_branches(&self) -> Result<Vec<Branch>> {
        self.ensure_default_branch().await?;
        self.storage.list_branches().await
    }

    /// List active branches only.
    pub async fn list_active_branches(&self) -> Result<Vec<Branch>> {
        let branches = self.storage.list_branches().await?;
        Ok(branches.into_iter().filter(|b| b.is_active()).collect())
    }

    /// Delete a branch.
    pub async fn delete_branch(&self, name: &str) -> Result<()> {
        if name == self.config.default_branch {
            return Err(BranchError::InvalidOperation(
                "Cannot delete default branch".to_string(),
            ));
        }

        if self.config.keep_deleted {
            // Mark as deleted instead of removing
            let mut branch = self
                .storage
                .get_branch(name)
                .await?
                .ok_or_else(|| BranchError::NotFound(name.to_string()))?;
            branch.status = BranchStatus::Deleted;
            branch.updated_at = Utc::now();
            self.storage.save_branch(&branch).await?;
        } else {
            self.storage.delete_branch(name).await?;
        }

        info!(branch = name, "Deleted branch");
        Ok(())
    }

    /// Archive a branch.
    pub async fn archive_branch(&self, name: &str) -> Result<()> {
        let mut branch = self
            .storage
            .get_branch(name)
            .await?
            .ok_or_else(|| BranchError::NotFound(name.to_string()))?;

        branch.archive();
        self.storage.save_branch(&branch).await?;

        info!(branch = name, "Archived branch");
        Ok(())
    }

    /// Rename a branch.
    pub async fn rename_branch(&self, old_name: &str, new_name: &str) -> Result<()> {
        if old_name == self.config.default_branch {
            return Err(BranchError::InvalidOperation(
                "Cannot rename default branch".to_string(),
            ));
        }

        if self.storage.get_branch(new_name).await?.is_some() {
            return Err(BranchError::AlreadyExists(new_name.to_string()));
        }

        let mut branch = self
            .storage
            .get_branch(old_name)
            .await?
            .ok_or_else(|| BranchError::NotFound(old_name.to_string()))?;

        // Delete old entry
        self.storage.delete_branch(old_name).await?;

        // Save with new name
        branch.name = new_name.to_string();
        branch.updated_at = Utc::now();
        self.storage.save_branch(&branch).await?;

        // Update children's parent references
        let all_branches = self.storage.list_branches().await?;
        for mut b in all_branches {
            if b.parent.as_ref() == Some(&old_name.to_string()) {
                b.parent = Some(new_name.to_string());
                if let Some(ref mut bp) = b.branch_point {
                    bp.parent_branch = new_name.to_string();
                }
                self.storage.save_branch(&b).await?;
            }
        }

        info!(old = old_name, new = new_name, "Renamed branch");
        Ok(())
    }

    /// Compare two branches.
    pub async fn compare(&self, branch_a: &str, branch_b: &str) -> Result<BranchDiff> {
        let a = self
            .storage
            .get_branch(branch_a)
            .await?
            .ok_or_else(|| BranchError::NotFound(branch_a.to_string()))?;
        let b = self
            .storage
            .get_branch(branch_b)
            .await?
            .ok_or_else(|| BranchError::NotFound(branch_b.to_string()))?;

        Ok(BranchDiff::compare(&a, &b))
    }

    /// Merge a branch into another.
    pub async fn merge(
        &self,
        source: &str,
        target: &str,
        strategy: MergeStrategy,
    ) -> Result<MergeResult> {
        let source_branch = self
            .storage
            .get_branch(source)
            .await?
            .ok_or_else(|| BranchError::NotFound(source.to_string()))?;
        let mut target_branch = self
            .storage
            .get_branch(target)
            .await?
            .ok_or_else(|| BranchError::NotFound(target.to_string()))?;

        if !target_branch.is_active() {
            return Err(BranchError::InvalidOperation(
                "Cannot merge into inactive branch".to_string(),
            ));
        }

        // Find common ancestor
        let common_index = self.find_common_ancestor(&source_branch, &target_branch);

        let messages_merged = match strategy {
            MergeStrategy::Append => {
                // Append all messages from source that come after common ancestor
                let new_messages: Vec<_> = source_branch
                    .messages
                    .iter()
                    .skip(common_index + 1)
                    .cloned()
                    .collect();
                let count = new_messages.len();
                target_branch.messages.extend(new_messages);
                count
            }
            MergeStrategy::Interleave => {
                // Interleave messages by timestamp
                let source_new: Vec<_> = source_branch
                    .messages
                    .iter()
                    .skip(common_index + 1)
                    .cloned()
                    .collect();
                let target_new: Vec<_> = target_branch
                    .messages
                    .iter()
                    .skip(common_index + 1)
                    .cloned()
                    .collect();

                // Keep common prefix
                target_branch.messages.truncate(common_index + 1);

                // Merge and sort by timestamp
                let mut all_new: Vec<_> = source_new.into_iter().chain(target_new).collect();
                all_new.sort_by_key(|m| m.timestamp);

                let count = all_new.len();
                target_branch.messages.extend(all_new);
                count
            }
            MergeStrategy::Replace => {
                // Replace target with source messages after common ancestor
                target_branch.messages.truncate(common_index + 1);
                let new_messages: Vec<_> = source_branch
                    .messages
                    .iter()
                    .skip(common_index + 1)
                    .cloned()
                    .collect();
                let count = new_messages.len();
                target_branch.messages.extend(new_messages);
                count
            }
        };

        target_branch.updated_at = Utc::now();
        self.storage.save_branch(&target_branch).await?;

        // Mark source as merged
        let mut source_branch = source_branch;
        source_branch.mark_merged(target);
        self.storage.save_branch(&source_branch).await?;

        info!(
            source = source,
            target = target,
            messages = messages_merged,
            "Merged branches"
        );

        Ok(MergeResult {
            source: source.to_string(),
            target: target.to_string(),
            messages_merged,
            common_ancestor_index: common_index,
        })
    }

    /// Find common ancestor index between two branches.
    fn find_common_ancestor(&self, a: &Branch, b: &Branch) -> usize {
        let min_len = a.messages.len().min(b.messages.len());
        for i in 0..min_len {
            if a.messages[i].id != b.messages[i].id {
                return if i > 0 { i - 1 } else { 0 };
            }
        }
        if min_len > 0 {
            min_len - 1
        } else {
            0
        }
    }

    /// Get branch history (parent chain).
    pub async fn get_branch_history(&self, name: &str) -> Result<Vec<String>> {
        let mut history = Vec::new();
        let mut current_name = name.to_string();

        while let Some(branch) = self.storage.get_branch(&current_name).await? {
            history.push(branch.name.clone());
            match branch.parent {
                Some(parent) => current_name = parent,
                None => break,
            }
        }

        Ok(history)
    }

    /// Get all child branches.
    pub async fn get_children(&self, name: &str) -> Result<Vec<Branch>> {
        let all = self.storage.list_branches().await?;
        Ok(all
            .into_iter()
            .filter(|b| b.parent.as_ref() == Some(&name.to_string()))
            .collect())
    }

    /// Update branch metadata.
    pub async fn update_metadata(&self, name: &str, metadata: BranchMetadata) -> Result<()> {
        let mut branch = self
            .storage
            .get_branch(name)
            .await?
            .ok_or_else(|| BranchError::NotFound(name.to_string()))?;

        branch.metadata = metadata;
        branch.updated_at = Utc::now();
        self.storage.save_branch(&branch).await?;

        Ok(())
    }

    /// Switch to a branch (returns the branch).
    pub async fn checkout(&self, name: &str) -> Result<Branch> {
        self.storage
            .get_branch(name)
            .await?
            .ok_or_else(|| BranchError::NotFound(name.to_string()))
    }
}

impl Default for BranchManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Merge strategy.
#[derive(Debug, Clone, Copy)]
pub enum MergeStrategy {
    /// Append source messages to target.
    Append,
    /// Interleave messages by timestamp.
    Interleave,
    /// Replace target messages with source messages.
    Replace,
}

/// Result of a merge operation.
#[derive(Debug, Clone)]
pub struct MergeResult {
    /// Source branch name.
    pub source: String,
    /// Target branch name.
    pub target: String,
    /// Number of messages merged.
    pub messages_merged: usize,
    /// Index of common ancestor.
    pub common_ancestor_index: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_branch_manager_creation() {
        let manager = BranchManager::new();
        let branches = manager.list_branches().await.unwrap();
        assert!(!branches.is_empty()); // Should have default branch
    }

    #[tokio::test]
    async fn test_create_and_list_branches() {
        let manager = BranchManager::new();

        // Add messages to default
        manager.add_message("main", "user", "Hello").await.unwrap();
        manager
            .add_message("main", "assistant", "Hi!")
            .await
            .unwrap();

        // Create branches
        manager.create_branch("main", "feature-1", 0).await.unwrap();
        manager.create_branch("main", "feature-2", 1).await.unwrap();

        let branches = manager.list_branches().await.unwrap();
        assert_eq!(branches.len(), 3);
    }

    #[tokio::test]
    async fn test_delete_branch() {
        let manager = BranchManager::new();
        manager.create_empty_branch("test").await.unwrap();

        manager.delete_branch("test").await.unwrap();

        // With keep_deleted=true, branch should still exist but be deleted status
        let branch = manager.get_branch("test").await.unwrap().unwrap();
        assert_eq!(branch.status, BranchStatus::Deleted);
    }

    #[tokio::test]
    async fn test_cannot_delete_default() {
        let manager = BranchManager::new();
        let result = manager.delete_branch("main").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_branch_history() {
        let manager = BranchManager::new();

        manager.add_message("main", "user", "Hello").await.unwrap();
        manager.create_branch("main", "child", 0).await.unwrap();

        manager
            .add_message("child", "user", "In child")
            .await
            .unwrap();
        manager
            .create_branch("child", "grandchild", 1)
            .await
            .unwrap();

        let history = manager.get_branch_history("grandchild").await.unwrap();
        assert_eq!(history, vec!["grandchild", "child", "main"]);
    }

    #[tokio::test]
    async fn test_merge_branches() {
        let manager = BranchManager::new();

        // Setup main with messages
        manager.add_message("main", "user", "Hello").await.unwrap();
        manager
            .add_message("main", "assistant", "Hi!")
            .await
            .unwrap();

        // Create branch and add different messages
        manager.create_branch("main", "feature", 1).await.unwrap();
        manager
            .add_message("feature", "user", "Feature work")
            .await
            .unwrap();

        // Merge feature into main
        let result = manager
            .merge("feature", "main", MergeStrategy::Append)
            .await
            .unwrap();
        assert_eq!(result.messages_merged, 1);

        // Check main has the merged message
        let main = manager.get_branch("main").await.unwrap().unwrap();
        assert_eq!(main.message_count(), 3);
    }
}
