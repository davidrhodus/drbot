//! Conversation branching and replay.
//!
//! This crate provides:
//! - Conversation history tracking
//! - Branch creation and navigation
//! - State restoration
//! - Timeline visualization

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Time travel errors.
#[derive(Debug, Error)]
pub enum TimeTravelError {
    #[error("Conversation not found: {0}")]
    ConversationNotFound(String),

    #[error("Branch not found: {0}")]
    BranchNotFound(String),

    #[error("Message not found: {0}")]
    MessageNotFound(String),

    #[error("Cannot branch from this point: {0}")]
    InvalidBranchPoint(String),

    #[error("Storage error: {0}")]
    StorageError(String),
}

/// Result type for time travel operations.
pub type Result<T> = std::result::Result<T, TimeTravelError>;

/// A conversation message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Message identifier.
    pub id: String,
    /// Role (user, assistant, system).
    pub role: MessageRole,
    /// Content.
    pub content: String,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
    /// Metadata.
    pub metadata: HashMap<String, String>,
    /// Model used (for assistant messages).
    pub model: Option<String>,
    /// Token count.
    pub tokens: Option<usize>,
}

/// Message roles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageRole {
    User,
    Assistant,
    System,
}

/// A conversation branch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Branch {
    /// Branch identifier.
    pub id: String,
    /// Branch name.
    pub name: String,
    /// Parent branch (if any).
    pub parent_branch: Option<String>,
    /// Branch point message ID.
    pub branch_point: Option<String>,
    /// Messages in this branch.
    pub messages: Vec<Message>,
    /// Created timestamp.
    pub created_at: DateTime<Utc>,
    /// Is this the active branch.
    pub is_active: bool,
    /// Branch description.
    pub description: Option<String>,
}

/// A complete conversation with branches.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    /// Conversation identifier.
    pub id: String,
    /// Conversation title.
    pub title: String,
    /// All branches.
    pub branches: HashMap<String, Branch>,
    /// Current active branch.
    pub active_branch: String,
    /// Created timestamp.
    pub created_at: DateTime<Utc>,
    /// Last updated.
    pub updated_at: DateTime<Utc>,
    /// Metadata.
    pub metadata: HashMap<String, String>,
}

/// Conversation snapshot for replay.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    /// Snapshot identifier.
    pub id: String,
    /// Name.
    pub name: String,
    /// Conversation ID.
    pub conversation_id: String,
    /// Branch ID.
    pub branch_id: String,
    /// Message index (inclusive).
    pub message_index: usize,
    /// Full state at this point.
    pub state: ConversationState,
    /// Created timestamp.
    pub created_at: DateTime<Utc>,
}

/// Conversation state for restoration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConversationState {
    /// Messages up to this point.
    pub messages: Vec<Message>,
    /// System prompt.
    pub system_prompt: Option<String>,
    /// Model.
    pub model: Option<String>,
    /// Temperature.
    pub temperature: Option<f64>,
    /// Custom state.
    pub custom: HashMap<String, String>,
}

/// Timeline entry for visualization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineEntry {
    /// Entry identifier.
    pub id: String,
    /// Branch ID.
    pub branch_id: String,
    /// Message ID.
    pub message_id: String,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
    /// Entry type.
    pub entry_type: TimelineEntryType,
    /// Preview text.
    pub preview: String,
}

/// Timeline entry types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TimelineEntryType {
    UserMessage,
    AssistantMessage,
    BranchCreated,
    BranchSwitched,
    SnapshotCreated,
}

/// Storage provider for persistence.
#[async_trait]
pub trait TimeTravelStorage: Send + Sync {
    /// Save conversation.
    async fn save_conversation(&self, conversation: &Conversation) -> Result<()>;

    /// Load conversation.
    async fn load_conversation(&self, id: &str) -> Result<Option<Conversation>>;

    /// Save snapshot.
    async fn save_snapshot(&self, snapshot: &Snapshot) -> Result<()>;

    /// Load snapshots for conversation.
    async fn load_snapshots(&self, conversation_id: &str) -> Result<Vec<Snapshot>>;
}

/// The time travel engine.
pub struct TimeTravelEngine {
    /// Storage provider.
    storage: Option<Arc<dyn TimeTravelStorage>>,
    /// Active conversations.
    conversations: Arc<RwLock<HashMap<String, Conversation>>>,
    /// Snapshots.
    snapshots: Arc<RwLock<HashMap<String, Vec<Snapshot>>>>,
}

impl TimeTravelEngine {
    /// Create a new time travel engine.
    pub fn new() -> Self {
        Self {
            storage: None,
            conversations: Arc::new(RwLock::new(HashMap::new())),
            snapshots: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Set storage provider.
    pub fn with_storage(mut self, storage: Arc<dyn TimeTravelStorage>) -> Self {
        self.storage = Some(storage);
        self
    }

    /// Create a new conversation.
    pub async fn create_conversation(&self, title: &str) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        let main_branch_id = Uuid::new_v4().to_string();
        let now = Utc::now();

        let main_branch = Branch {
            id: main_branch_id.clone(),
            name: "main".to_string(),
            parent_branch: None,
            branch_point: None,
            messages: Vec::new(),
            created_at: now,
            is_active: true,
            description: Some("Main conversation branch".to_string()),
        };

        let mut branches = HashMap::new();
        branches.insert(main_branch_id.clone(), main_branch);

        let conversation = Conversation {
            id: id.clone(),
            title: title.to_string(),
            branches,
            active_branch: main_branch_id,
            created_at: now,
            updated_at: now,
            metadata: HashMap::new(),
        };

        let mut conversations = self.conversations.write().await;
        conversations.insert(id.clone(), conversation.clone());

        if let Some(storage) = &self.storage {
            storage.save_conversation(&conversation).await?;
        }

        Ok(id)
    }

    /// Add a message to the active branch.
    pub async fn add_message(
        &self,
        conversation_id: &str,
        role: MessageRole,
        content: &str,
    ) -> Result<String> {
        let mut conversations = self.conversations.write().await;
        let conversation = conversations
            .get_mut(conversation_id)
            .ok_or_else(|| TimeTravelError::ConversationNotFound(conversation_id.to_string()))?;

        let branch = conversation
            .branches
            .get_mut(&conversation.active_branch)
            .ok_or_else(|| TimeTravelError::BranchNotFound(conversation.active_branch.clone()))?;

        let message = Message {
            id: Uuid::new_v4().to_string(),
            role,
            content: content.to_string(),
            timestamp: Utc::now(),
            metadata: HashMap::new(),
            model: None,
            tokens: None,
        };

        let id = message.id.clone();
        branch.messages.push(message);
        conversation.updated_at = Utc::now();

        if let Some(storage) = &self.storage {
            storage.save_conversation(conversation).await?;
        }

        Ok(id)
    }

    /// Create a branch from a specific message.
    pub async fn create_branch(
        &self,
        conversation_id: &str,
        from_message_id: &str,
        name: &str,
    ) -> Result<String> {
        let mut conversations = self.conversations.write().await;
        let conversation = conversations
            .get_mut(conversation_id)
            .ok_or_else(|| TimeTravelError::ConversationNotFound(conversation_id.to_string()))?;

        let source_branch = conversation
            .branches
            .get(&conversation.active_branch)
            .ok_or_else(|| TimeTravelError::BranchNotFound(conversation.active_branch.clone()))?;

        // Find message index
        let message_idx = source_branch
            .messages
            .iter()
            .position(|m| m.id == from_message_id)
            .ok_or_else(|| TimeTravelError::MessageNotFound(from_message_id.to_string()))?;

        // Copy messages up to and including the branch point
        let messages: Vec<Message> = source_branch.messages[..=message_idx].to_vec();

        let branch_id = Uuid::new_v4().to_string();
        let new_branch = Branch {
            id: branch_id.clone(),
            name: name.to_string(),
            parent_branch: Some(conversation.active_branch.clone()),
            branch_point: Some(from_message_id.to_string()),
            messages,
            created_at: Utc::now(),
            is_active: false,
            description: None,
        };

        conversation.branches.insert(branch_id.clone(), new_branch);
        conversation.updated_at = Utc::now();

        if let Some(storage) = &self.storage {
            storage.save_conversation(conversation).await?;
        }

        Ok(branch_id)
    }

    /// Switch to a different branch.
    pub async fn switch_branch(&self, conversation_id: &str, branch_id: &str) -> Result<()> {
        let mut conversations = self.conversations.write().await;
        let conversation = conversations
            .get_mut(conversation_id)
            .ok_or_else(|| TimeTravelError::ConversationNotFound(conversation_id.to_string()))?;

        if !conversation.branches.contains_key(branch_id) {
            return Err(TimeTravelError::BranchNotFound(branch_id.to_string()));
        }

        // Update active states
        for (id, branch) in conversation.branches.iter_mut() {
            branch.is_active = id == branch_id;
        }

        conversation.active_branch = branch_id.to_string();
        conversation.updated_at = Utc::now();

        if let Some(storage) = &self.storage {
            storage.save_conversation(conversation).await?;
        }

        Ok(())
    }

    /// Get messages from active branch.
    pub async fn get_messages(&self, conversation_id: &str) -> Result<Vec<Message>> {
        let conversations = self.conversations.read().await;
        let conversation = conversations
            .get(conversation_id)
            .ok_or_else(|| TimeTravelError::ConversationNotFound(conversation_id.to_string()))?;

        let branch = conversation
            .branches
            .get(&conversation.active_branch)
            .ok_or_else(|| TimeTravelError::BranchNotFound(conversation.active_branch.clone()))?;

        Ok(branch.messages.clone())
    }

    /// Get all branches.
    pub async fn get_branches(&self, conversation_id: &str) -> Result<Vec<Branch>> {
        let conversations = self.conversations.read().await;
        let conversation = conversations
            .get(conversation_id)
            .ok_or_else(|| TimeTravelError::ConversationNotFound(conversation_id.to_string()))?;

        Ok(conversation.branches.values().cloned().collect())
    }

    /// Create a snapshot at current position.
    pub async fn create_snapshot(&self, conversation_id: &str, name: &str) -> Result<String> {
        let conversations = self.conversations.read().await;
        let conversation = conversations
            .get(conversation_id)
            .ok_or_else(|| TimeTravelError::ConversationNotFound(conversation_id.to_string()))?;

        let branch = conversation
            .branches
            .get(&conversation.active_branch)
            .ok_or_else(|| TimeTravelError::BranchNotFound(conversation.active_branch.clone()))?;

        let snapshot = Snapshot {
            id: Uuid::new_v4().to_string(),
            name: name.to_string(),
            conversation_id: conversation_id.to_string(),
            branch_id: conversation.active_branch.clone(),
            message_index: branch.messages.len().saturating_sub(1),
            state: ConversationState {
                messages: branch.messages.clone(),
                system_prompt: None,
                model: None,
                temperature: None,
                custom: HashMap::new(),
            },
            created_at: Utc::now(),
        };

        let id = snapshot.id.clone();
        drop(conversations);

        let mut snapshots = self.snapshots.write().await;
        snapshots
            .entry(conversation_id.to_string())
            .or_default()
            .push(snapshot.clone());

        if let Some(storage) = &self.storage {
            storage.save_snapshot(&snapshot).await?;
        }

        Ok(id)
    }

    /// Restore from a snapshot.
    pub async fn restore_snapshot(&self, conversation_id: &str, snapshot_id: &str) -> Result<()> {
        let snapshots = self.snapshots.read().await;
        let conversation_snapshots = snapshots
            .get(conversation_id)
            .ok_or_else(|| TimeTravelError::ConversationNotFound(conversation_id.to_string()))?;

        let snapshot = conversation_snapshots
            .iter()
            .find(|s| s.id == snapshot_id)
            .ok_or_else(|| TimeTravelError::MessageNotFound(snapshot_id.to_string()))?
            .clone();

        drop(snapshots);

        let mut conversations = self.conversations.write().await;
        let conversation = conversations
            .get_mut(conversation_id)
            .ok_or_else(|| TimeTravelError::ConversationNotFound(conversation_id.to_string()))?;

        // Create a new branch from snapshot
        let branch_id = Uuid::new_v4().to_string();
        let restored_branch = Branch {
            id: branch_id.clone(),
            name: format!("restored-{}", &snapshot.name),
            parent_branch: Some(snapshot.branch_id.clone()),
            branch_point: None,
            messages: snapshot.state.messages.clone(),
            created_at: Utc::now(),
            is_active: true,
            description: Some(format!("Restored from snapshot: {}", snapshot.name)),
        };

        // Deactivate other branches
        for branch in conversation.branches.values_mut() {
            branch.is_active = false;
        }

        conversation
            .branches
            .insert(branch_id.clone(), restored_branch);
        conversation.active_branch = branch_id;
        conversation.updated_at = Utc::now();

        if let Some(storage) = &self.storage {
            storage.save_conversation(conversation).await?;
        }

        Ok(())
    }

    /// Rewind to a specific message (removes subsequent messages).
    pub async fn rewind_to(&self, conversation_id: &str, message_id: &str) -> Result<()> {
        let mut conversations = self.conversations.write().await;
        let conversation = conversations
            .get_mut(conversation_id)
            .ok_or_else(|| TimeTravelError::ConversationNotFound(conversation_id.to_string()))?;

        let branch = conversation
            .branches
            .get_mut(&conversation.active_branch)
            .ok_or_else(|| TimeTravelError::BranchNotFound(conversation.active_branch.clone()))?;

        let message_idx = branch
            .messages
            .iter()
            .position(|m| m.id == message_id)
            .ok_or_else(|| TimeTravelError::MessageNotFound(message_id.to_string()))?;

        // Truncate messages after this point
        branch.messages.truncate(message_idx + 1);
        conversation.updated_at = Utc::now();

        if let Some(storage) = &self.storage {
            storage.save_conversation(conversation).await?;
        }

        Ok(())
    }

    /// Get timeline for visualization.
    pub async fn get_timeline(&self, conversation_id: &str) -> Result<Vec<TimelineEntry>> {
        let conversations = self.conversations.read().await;
        let conversation = conversations
            .get(conversation_id)
            .ok_or_else(|| TimeTravelError::ConversationNotFound(conversation_id.to_string()))?;

        let mut entries = Vec::new();

        for (branch_id, branch) in &conversation.branches {
            for message in &branch.messages {
                let entry_type = match message.role {
                    MessageRole::User => TimelineEntryType::UserMessage,
                    MessageRole::Assistant => TimelineEntryType::AssistantMessage,
                    MessageRole::System => TimelineEntryType::UserMessage,
                };

                entries.push(TimelineEntry {
                    id: Uuid::new_v4().to_string(),
                    branch_id: branch_id.clone(),
                    message_id: message.id.clone(),
                    timestamp: message.timestamp,
                    entry_type,
                    preview: message.content.chars().take(50).collect(),
                });
            }
        }

        entries.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));

        Ok(entries)
    }

    /// Delete a branch (cannot delete main).
    pub async fn delete_branch(&self, conversation_id: &str, branch_id: &str) -> Result<()> {
        let mut conversations = self.conversations.write().await;
        let conversation = conversations
            .get_mut(conversation_id)
            .ok_or_else(|| TimeTravelError::ConversationNotFound(conversation_id.to_string()))?;

        let branch = conversation
            .branches
            .get(branch_id)
            .ok_or_else(|| TimeTravelError::BranchNotFound(branch_id.to_string()))?;

        if branch.name == "main" {
            return Err(TimeTravelError::InvalidBranchPoint(
                "Cannot delete main branch".to_string(),
            ));
        }

        conversation.branches.remove(branch_id);
        conversation.updated_at = Utc::now();

        if let Some(storage) = &self.storage {
            storage.save_conversation(conversation).await?;
        }

        Ok(())
    }
}

impl Default for TimeTravelEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_conversation() {
        let engine = TimeTravelEngine::new();
        let id = engine.create_conversation("Test Chat").await.unwrap();
        assert!(!id.is_empty());

        let branches = engine.get_branches(&id).await.unwrap();
        assert_eq!(branches.len(), 1);
        assert_eq!(branches[0].name, "main");
    }

    #[tokio::test]
    async fn test_add_messages() {
        let engine = TimeTravelEngine::new();
        let conv_id = engine.create_conversation("Test").await.unwrap();

        engine
            .add_message(&conv_id, MessageRole::User, "Hello")
            .await
            .unwrap();
        engine
            .add_message(&conv_id, MessageRole::Assistant, "Hi there!")
            .await
            .unwrap();

        let messages = engine.get_messages(&conv_id).await.unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].content, "Hello");
        assert_eq!(messages[1].content, "Hi there!");
    }

    #[tokio::test]
    async fn test_create_branch() {
        let engine = TimeTravelEngine::new();
        let conv_id = engine.create_conversation("Test").await.unwrap();

        let msg_id = engine
            .add_message(&conv_id, MessageRole::User, "First")
            .await
            .unwrap();
        engine
            .add_message(&conv_id, MessageRole::Assistant, "Response")
            .await
            .unwrap();

        let branch_id = engine
            .create_branch(&conv_id, &msg_id, "experiment")
            .await
            .unwrap();
        assert!(!branch_id.is_empty());

        let branches = engine.get_branches(&conv_id).await.unwrap();
        assert_eq!(branches.len(), 2);
    }

    #[tokio::test]
    async fn test_switch_branch() {
        let engine = TimeTravelEngine::new();
        let conv_id = engine.create_conversation("Test").await.unwrap();

        let msg_id = engine
            .add_message(&conv_id, MessageRole::User, "Hello")
            .await
            .unwrap();
        let branch_id = engine
            .create_branch(&conv_id, &msg_id, "alt")
            .await
            .unwrap();

        engine.switch_branch(&conv_id, &branch_id).await.unwrap();

        // Add message to new branch
        engine
            .add_message(&conv_id, MessageRole::User, "On new branch")
            .await
            .unwrap();

        let messages = engine.get_messages(&conv_id).await.unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[1].content, "On new branch");
    }

    #[tokio::test]
    async fn test_snapshot_and_restore() {
        let engine = TimeTravelEngine::new();
        let conv_id = engine.create_conversation("Test").await.unwrap();

        engine
            .add_message(&conv_id, MessageRole::User, "First")
            .await
            .unwrap();
        let snapshot_id = engine
            .create_snapshot(&conv_id, "checkpoint1")
            .await
            .unwrap();

        engine
            .add_message(&conv_id, MessageRole::User, "Second")
            .await
            .unwrap();
        engine
            .add_message(&conv_id, MessageRole::User, "Third")
            .await
            .unwrap();

        let messages = engine.get_messages(&conv_id).await.unwrap();
        assert_eq!(messages.len(), 3);

        engine
            .restore_snapshot(&conv_id, &snapshot_id)
            .await
            .unwrap();

        let messages = engine.get_messages(&conv_id).await.unwrap();
        assert_eq!(messages.len(), 1);
    }

    #[tokio::test]
    async fn test_rewind() {
        let engine = TimeTravelEngine::new();
        let conv_id = engine.create_conversation("Test").await.unwrap();

        let msg1 = engine
            .add_message(&conv_id, MessageRole::User, "First")
            .await
            .unwrap();
        engine
            .add_message(&conv_id, MessageRole::User, "Second")
            .await
            .unwrap();
        engine
            .add_message(&conv_id, MessageRole::User, "Third")
            .await
            .unwrap();

        engine.rewind_to(&conv_id, &msg1).await.unwrap();

        let messages = engine.get_messages(&conv_id).await.unwrap();
        assert_eq!(messages.len(), 1);
    }
}
