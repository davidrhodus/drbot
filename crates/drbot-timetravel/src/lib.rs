//! Conversation time-travel for drbot.
//!
//! Git-like navigation through conversation history.
//!
//! # Features
//!
//! - Branch conversations at any point
//! - Checkout previous states
//! - Diff between conversation versions
//! - Cherry-pick messages
//! - Merge conversation branches

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Time-travel result type.
pub type Result<T> = std::result::Result<T, TimeTravelError>;

/// Time-travel errors.
#[derive(Debug, thiserror::Error)]
pub enum TimeTravelError {
    #[error("Commit not found: {0}")]
    CommitNotFound(String),
    #[error("Branch not found: {0}")]
    BranchNotFound(String),
    #[error("Branch already exists: {0}")]
    BranchExists(String),
    #[error("Cannot merge: {0}")]
    MergeConflict(String),
    #[error("Invalid operation: {0}")]
    InvalidOperation(String),
}

/// A conversation commit (snapshot).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Commit {
    /// Commit hash (short UUID).
    pub hash: String,
    /// Full commit ID.
    pub id: Uuid,
    /// Parent commit hash.
    pub parent: Option<String>,
    /// Commit message.
    pub message: String,
    /// Messages in this commit.
    pub messages: Vec<ConversationMessage>,
    /// Author (user or system).
    pub author: String,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
    /// Metadata.
    pub metadata: HashMap<String, String>,
}

impl Commit {
    /// Create a new commit.
    pub fn new(parent: Option<String>, message: &str, messages: Vec<ConversationMessage>) -> Self {
        let id = Uuid::new_v4();
        let hash = id.to_string()[..8].to_string();

        Self {
            hash,
            id,
            parent,
            message: message.to_string(),
            messages,
            author: "user".to_string(),
            timestamp: Utc::now(),
            metadata: HashMap::new(),
        }
    }

    /// Set author.
    pub fn with_author(mut self, author: &str) -> Self {
        self.author = author.to_string();
        self
    }

    /// Add metadata.
    pub fn with_metadata(mut self, key: &str, value: &str) -> Self {
        self.metadata.insert(key.to_string(), value.to_string());
        self
    }
}

/// A message in the conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationMessage {
    /// Message ID.
    pub id: Uuid,
    /// Role (user, assistant, system).
    pub role: String,
    /// Content.
    pub content: String,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
}

impl ConversationMessage {
    /// Create a new message.
    pub fn new(role: &str, content: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            role: role.to_string(),
            content: content.to_string(),
            timestamp: Utc::now(),
        }
    }
}

/// A branch in the conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Branch {
    /// Branch name.
    pub name: String,
    /// Current commit hash.
    pub head: String,
    /// Created at.
    pub created_at: DateTime<Utc>,
    /// Description.
    pub description: Option<String>,
}

/// Diff between two commits.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diff {
    /// Base commit hash.
    pub base: String,
    /// Target commit hash.
    pub target: String,
    /// Added messages.
    pub added: Vec<ConversationMessage>,
    /// Removed messages.
    pub removed: Vec<ConversationMessage>,
    /// Modified messages (old, new).
    pub modified: Vec<(ConversationMessage, ConversationMessage)>,
}

/// Conversation history with version control.
pub struct ConversationHistory {
    /// All commits.
    commits: Arc<RwLock<HashMap<String, Commit>>>,
    /// All branches.
    branches: Arc<RwLock<HashMap<String, Branch>>>,
    /// Current branch.
    current_branch: Arc<RwLock<String>>,
    /// Current HEAD.
    head: Arc<RwLock<Option<String>>>,
}

impl ConversationHistory {
    /// Create a new conversation history.
    pub fn new() -> Self {
        let mut branches = HashMap::new();
        branches.insert(
            "main".to_string(),
            Branch {
                name: "main".to_string(),
                head: String::new(),
                created_at: Utc::now(),
                description: Some("Main conversation branch".to_string()),
            },
        );

        Self {
            commits: Arc::new(RwLock::new(HashMap::new())),
            branches: Arc::new(RwLock::new(branches)),
            current_branch: Arc::new(RwLock::new("main".to_string())),
            head: Arc::new(RwLock::new(None)),
        }
    }

    /// Commit current messages.
    pub async fn commit(
        &self,
        message: &str,
        messages: Vec<ConversationMessage>,
    ) -> Result<String> {
        let parent = self.head.read().await.clone();
        let commit = Commit::new(parent, message, messages);
        let hash = commit.hash.clone();

        // Store commit
        self.commits.write().await.insert(hash.clone(), commit);

        // Update HEAD
        *self.head.write().await = Some(hash.clone());

        // Update branch head
        let branch_name = self.current_branch.read().await.clone();
        let mut branches = self.branches.write().await;
        if let Some(branch) = branches.get_mut(&branch_name) {
            branch.head = hash.clone();
        }

        Ok(hash)
    }

    /// Get a commit by hash.
    pub async fn get_commit(&self, hash: &str) -> Option<Commit> {
        self.commits.read().await.get(hash).cloned()
    }

    /// Checkout a specific commit.
    pub async fn checkout(&self, hash: &str) -> Result<Commit> {
        let commits = self.commits.read().await;

        if let Some(commit) = commits.get(hash) {
            *self.head.write().await = Some(hash.to_string());
            Ok(commit.clone())
        } else {
            Err(TimeTravelError::CommitNotFound(hash.to_string()))
        }
    }

    /// Checkout a branch.
    pub async fn checkout_branch(&self, name: &str) -> Result<()> {
        let branches = self.branches.read().await;

        if let Some(branch) = branches.get(name) {
            *self.current_branch.write().await = name.to_string();
            *self.head.write().await = if branch.head.is_empty() {
                None
            } else {
                Some(branch.head.clone())
            };
            Ok(())
        } else {
            Err(TimeTravelError::BranchNotFound(name.to_string()))
        }
    }

    /// Create a new branch.
    pub async fn create_branch(&self, name: &str) -> Result<()> {
        let mut branches = self.branches.write().await;

        if branches.contains_key(name) {
            return Err(TimeTravelError::BranchExists(name.to_string()));
        }

        let head = self.head.read().await.clone().unwrap_or_default();

        branches.insert(
            name.to_string(),
            Branch {
                name: name.to_string(),
                head,
                created_at: Utc::now(),
                description: None,
            },
        );

        Ok(())
    }

    /// Delete a branch.
    pub async fn delete_branch(&self, name: &str) -> Result<()> {
        if name == "main" {
            return Err(TimeTravelError::InvalidOperation(
                "Cannot delete main branch".to_string(),
            ));
        }

        let current = self.current_branch.read().await.clone();
        if current == name {
            return Err(TimeTravelError::InvalidOperation(
                "Cannot delete current branch".to_string(),
            ));
        }

        let mut branches = self.branches.write().await;
        branches.remove(name);

        Ok(())
    }

    /// List all branches.
    pub async fn list_branches(&self) -> Vec<Branch> {
        self.branches.read().await.values().cloned().collect()
    }

    /// Get commit history (log).
    pub async fn log(&self, limit: usize) -> Vec<Commit> {
        let commits = self.commits.read().await;
        let mut current = self.head.read().await.clone();
        let mut history = Vec::new();

        while let Some(hash) = current {
            if history.len() >= limit {
                break;
            }

            if let Some(commit) = commits.get(&hash) {
                current = commit.parent.clone();
                history.push(commit.clone());
            } else {
                break;
            }
        }

        history
    }

    /// Diff between two commits.
    pub async fn diff(&self, base_hash: &str, target_hash: &str) -> Result<Diff> {
        let commits = self.commits.read().await;

        let base = commits
            .get(base_hash)
            .ok_or_else(|| TimeTravelError::CommitNotFound(base_hash.to_string()))?;
        let target = commits
            .get(target_hash)
            .ok_or_else(|| TimeTravelError::CommitNotFound(target_hash.to_string()))?;

        let base_ids: std::collections::HashSet<_> = base.messages.iter().map(|m| m.id).collect();
        let target_ids: std::collections::HashSet<_> =
            target.messages.iter().map(|m| m.id).collect();

        let added: Vec<_> = target
            .messages
            .iter()
            .filter(|m| !base_ids.contains(&m.id))
            .cloned()
            .collect();

        let removed: Vec<_> = base
            .messages
            .iter()
            .filter(|m| !target_ids.contains(&m.id))
            .cloned()
            .collect();

        Ok(Diff {
            base: base_hash.to_string(),
            target: target_hash.to_string(),
            added,
            removed,
            modified: Vec::new(),
        })
    }

    /// Cherry-pick a message from another commit.
    pub async fn cherry_pick(
        &self,
        source_hash: &str,
        message_id: Uuid,
    ) -> Result<ConversationMessage> {
        let commits = self.commits.read().await;

        let source = commits
            .get(source_hash)
            .ok_or_else(|| TimeTravelError::CommitNotFound(source_hash.to_string()))?;

        source
            .messages
            .iter()
            .find(|m| m.id == message_id)
            .cloned()
            .ok_or_else(|| TimeTravelError::InvalidOperation("Message not found".to_string()))
    }

    /// Reset to a previous commit (soft reset - keep changes).
    pub async fn reset(&self, hash: &str) -> Result<()> {
        let commits = self.commits.read().await;

        if !commits.contains_key(hash) {
            return Err(TimeTravelError::CommitNotFound(hash.to_string()));
        }

        *self.head.write().await = Some(hash.to_string());

        let branch_name = self.current_branch.read().await.clone();
        let mut branches = self.branches.write().await;
        if let Some(branch) = branches.get_mut(&branch_name) {
            branch.head = hash.to_string();
        }

        Ok(())
    }

    /// Merge another branch into current.
    pub async fn merge(&self, source_branch: &str, message: &str) -> Result<String> {
        let branches = self.branches.read().await;

        let source = branches
            .get(source_branch)
            .ok_or_else(|| TimeTravelError::BranchNotFound(source_branch.to_string()))?
            .clone();

        let commits = self.commits.read().await;
        let source_commit = commits
            .get(&source.head)
            .ok_or_else(|| TimeTravelError::CommitNotFound(source.head.clone()))?
            .clone();

        drop(commits);
        drop(branches);

        // Create merge commit with combined messages
        let merge_commit = Commit::new(
            self.head.read().await.clone(),
            message,
            source_commit.messages,
        )
        .with_metadata("merge_from", source_branch);

        let hash = merge_commit.hash.clone();

        self.commits
            .write()
            .await
            .insert(hash.clone(), merge_commit);
        *self.head.write().await = Some(hash.clone());

        let branch_name = self.current_branch.read().await.clone();
        let mut branches = self.branches.write().await;
        if let Some(branch) = branches.get_mut(&branch_name) {
            branch.head = hash.clone();
        }

        Ok(hash)
    }

    /// Get current branch name.
    pub async fn current_branch(&self) -> String {
        self.current_branch.read().await.clone()
    }

    /// Get current HEAD hash.
    pub async fn head(&self) -> Option<String> {
        self.head.read().await.clone()
    }

    /// Get all messages from current HEAD.
    pub async fn current_messages(&self) -> Vec<ConversationMessage> {
        if let Some(hash) = self.head.read().await.clone() {
            if let Some(commit) = self.commits.read().await.get(&hash) {
                return commit.messages.clone();
            }
        }
        Vec::new()
    }
}

impl Default for ConversationHistory {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_commit_and_checkout() {
        let history = ConversationHistory::new();

        let messages = vec![
            ConversationMessage::new("user", "Hello"),
            ConversationMessage::new("assistant", "Hi there!"),
        ];

        let hash = history.commit("Initial commit", messages).await.unwrap();
        assert!(!hash.is_empty());

        let commit = history.get_commit(&hash).await.unwrap();
        assert_eq!(commit.messages.len(), 2);
    }

    #[tokio::test]
    async fn test_branching() {
        let history = ConversationHistory::new();

        // Create initial commit
        let messages = vec![ConversationMessage::new("user", "Start")];
        history.commit("Initial", messages).await.unwrap();

        // Create branch
        history.create_branch("feature").await.unwrap();
        history.checkout_branch("feature").await.unwrap();

        assert_eq!(history.current_branch().await, "feature");

        // Commit on branch
        let messages = vec![ConversationMessage::new("user", "Feature work")];
        history.commit("Feature commit", messages).await.unwrap();

        // Switch back to main
        history.checkout_branch("main").await.unwrap();
        assert_eq!(history.current_branch().await, "main");
    }

    #[tokio::test]
    async fn test_log() {
        let history = ConversationHistory::new();

        for i in 0..5 {
            let messages = vec![ConversationMessage::new("user", &format!("Message {}", i))];
            history
                .commit(&format!("Commit {}", i), messages)
                .await
                .unwrap();
        }

        let log = history.log(3).await;
        assert_eq!(log.len(), 3);
        assert!(log[0].message.contains("4")); // Most recent first
    }

    #[tokio::test]
    async fn test_diff() {
        let history = ConversationHistory::new();

        // Create shared message with same ID
        let shared_msg = ConversationMessage::new("user", "Hello");
        let new_msg = ConversationMessage::new("assistant", "Hi!");

        let messages1 = vec![shared_msg.clone()];
        let hash1 = history.commit("First", messages1).await.unwrap();

        let messages2 = vec![shared_msg.clone(), new_msg];
        let hash2 = history.commit("Second", messages2).await.unwrap();

        let diff = history.diff(&hash1, &hash2).await.unwrap();
        assert_eq!(diff.added.len(), 1);
    }
}
