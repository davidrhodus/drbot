//! Branch data structures.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// A conversation branch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Branch {
    /// Unique branch identifier.
    pub id: Uuid,
    /// Branch name (human-readable).
    pub name: String,
    /// Parent branch name (None for root).
    pub parent: Option<String>,
    /// Point where this branch was created.
    pub branch_point: Option<BranchPoint>,
    /// Messages in this branch.
    pub messages: Vec<BranchMessage>,
    /// Branch status.
    pub status: BranchStatus,
    /// Branch metadata.
    pub metadata: BranchMetadata,
    /// Created timestamp.
    pub created_at: DateTime<Utc>,
    /// Updated timestamp.
    pub updated_at: DateTime<Utc>,
}

impl Branch {
    /// Create a new root branch.
    pub fn new(name: &str) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            name: name.to_string(),
            parent: None,
            branch_point: None,
            messages: Vec::new(),
            status: BranchStatus::Active,
            metadata: BranchMetadata::default(),
            created_at: now,
            updated_at: now,
        }
    }

    /// Create a branch from a parent.
    pub fn from_parent(name: &str, parent: &Branch, message_index: usize) -> Self {
        let now = Utc::now();

        // Copy messages up to the branch point
        let messages: Vec<BranchMessage> = parent
            .messages
            .iter()
            .take(message_index + 1)
            .cloned()
            .collect();

        Self {
            id: Uuid::new_v4(),
            name: name.to_string(),
            parent: Some(parent.name.clone()),
            branch_point: Some(BranchPoint {
                parent_branch: parent.name.clone(),
                message_index,
                message_id: parent.messages.get(message_index).map(|m| m.id),
            }),
            messages,
            status: BranchStatus::Active,
            metadata: BranchMetadata {
                description: Some(format!(
                    "Branched from '{}' at message {}",
                    parent.name, message_index
                )),
                ..Default::default()
            },
            created_at: now,
            updated_at: now,
        }
    }

    /// Add a message to the branch.
    pub fn add_message(&mut self, role: &str, content: &str) {
        let message = BranchMessage::new(role, content);
        self.messages.push(message);
        self.updated_at = Utc::now();
    }

    /// Get the message at an index.
    pub fn get_message(&self, index: usize) -> Option<&BranchMessage> {
        self.messages.get(index)
    }

    /// Get the last message.
    pub fn last_message(&self) -> Option<&BranchMessage> {
        self.messages.last()
    }

    /// Count messages.
    pub fn message_count(&self) -> usize {
        self.messages.len()
    }

    /// Check if branch is active.
    pub fn is_active(&self) -> bool {
        self.status == BranchStatus::Active
    }

    /// Archive the branch.
    pub fn archive(&mut self) {
        self.status = BranchStatus::Archived;
        self.updated_at = Utc::now();
    }

    /// Mark as merged.
    pub fn mark_merged(&mut self, into_branch: &str) {
        self.status = BranchStatus::Merged;
        self.metadata.merged_into = Some(into_branch.to_string());
        self.updated_at = Utc::now();
    }
}

/// A message in a branch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchMessage {
    /// Message ID.
    pub id: Uuid,
    /// Role (user, assistant, system).
    pub role: String,
    /// Message content.
    pub content: String,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
    /// Additional metadata.
    pub metadata: HashMap<String, serde_json::Value>,
}

impl BranchMessage {
    /// Create a new message.
    pub fn new(role: &str, content: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            role: role.to_string(),
            content: content.to_string(),
            timestamp: Utc::now(),
            metadata: HashMap::new(),
        }
    }

    /// Add metadata.
    pub fn with_metadata(mut self, key: &str, value: serde_json::Value) -> Self {
        self.metadata.insert(key.to_string(), value);
        self
    }
}

/// Point where a branch was created.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchPoint {
    /// Parent branch name.
    pub parent_branch: String,
    /// Index of the message where branch was created.
    pub message_index: usize,
    /// ID of the message (for verification).
    pub message_id: Option<Uuid>,
}

/// Branch status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BranchStatus {
    /// Branch is active and can receive messages.
    Active,
    /// Branch is archived (read-only).
    Archived,
    /// Branch was merged into another.
    Merged,
    /// Branch was deleted.
    Deleted,
}

/// Branch metadata.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BranchMetadata {
    /// Description of the branch.
    pub description: Option<String>,
    /// Tags for categorization.
    pub tags: Vec<String>,
    /// Branch that this was merged into.
    pub merged_into: Option<String>,
    /// Custom properties.
    pub properties: HashMap<String, serde_json::Value>,
    /// Star/favorite flag.
    pub starred: bool,
    /// Color for UI.
    pub color: Option<String>,
}

impl BranchMetadata {
    /// Add a tag.
    pub fn add_tag(&mut self, tag: &str) {
        if !self.tags.contains(&tag.to_string()) {
            self.tags.push(tag.to_string());
        }
    }

    /// Remove a tag.
    pub fn remove_tag(&mut self, tag: &str) {
        self.tags.retain(|t| t != tag);
    }

    /// Set a property.
    pub fn set_property(&mut self, key: &str, value: serde_json::Value) {
        self.properties.insert(key.to_string(), value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_branch_creation() {
        let branch = Branch::new("main");
        assert_eq!(branch.name, "main");
        assert!(branch.parent.is_none());
        assert!(branch.is_active());
    }

    #[test]
    fn test_branch_messages() {
        let mut branch = Branch::new("test");
        branch.add_message("user", "Hello");
        branch.add_message("assistant", "Hi there!");

        assert_eq!(branch.message_count(), 2);
        assert_eq!(branch.get_message(0).unwrap().role, "user");
        assert_eq!(branch.last_message().unwrap().content, "Hi there!");
    }

    #[test]
    fn test_branch_from_parent() {
        let mut parent = Branch::new("main");
        parent.add_message("user", "Message 1");
        parent.add_message("assistant", "Response 1");
        parent.add_message("user", "Message 2");

        let child = Branch::from_parent("feature", &parent, 1);

        assert_eq!(child.parent, Some("main".to_string()));
        assert_eq!(child.message_count(), 2); // Messages up to index 1
        assert!(child.branch_point.is_some());
    }

    #[test]
    fn test_branch_status() {
        let mut branch = Branch::new("test");
        assert!(branch.is_active());

        branch.archive();
        assert_eq!(branch.status, BranchStatus::Archived);

        let mut branch2 = Branch::new("test2");
        branch2.mark_merged("main");
        assert_eq!(branch2.status, BranchStatus::Merged);
        assert_eq!(branch2.metadata.merged_into, Some("main".to_string()));
    }

    #[test]
    fn test_branch_metadata() {
        let mut metadata = BranchMetadata::default();
        metadata.add_tag("experiment");
        metadata.add_tag("important");

        assert!(metadata.tags.contains(&"experiment".to_string()));
        assert_eq!(metadata.tags.len(), 2);

        metadata.remove_tag("experiment");
        assert_eq!(metadata.tags.len(), 1);
    }
}
