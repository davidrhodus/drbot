//! Conversation types.

use crate::message::Message;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Conversation metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationMetadata {
    /// Conversation title.
    pub title: String,
    /// Creation timestamp.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Last updated timestamp.
    pub updated_at: chrono::DateTime<chrono::Utc>,
    /// Tags.
    pub tags: Vec<String>,
    /// Custom metadata.
    pub custom: HashMap<String, serde_json::Value>,
    /// Model used.
    pub model: Option<String>,
    /// Workspace/project association.
    pub workspace: Option<String>,
}

impl Default for ConversationMetadata {
    fn default() -> Self {
        let now = chrono::Utc::now();
        Self {
            title: "New Conversation".to_string(),
            created_at: now,
            updated_at: now,
            tags: Vec::new(),
            custom: HashMap::new(),
            model: None,
            workspace: None,
        }
    }
}

impl ConversationMetadata {
    /// Create with a title.
    pub fn with_title(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            ..Default::default()
        }
    }

    /// Add a tag.
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    /// Set the model.
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// Set the workspace.
    pub fn with_workspace(mut self, workspace: impl Into<String>) -> Self {
        self.workspace = Some(workspace.into());
        self
    }

    /// Update the timestamp.
    pub fn touch(&mut self) {
        self.updated_at = chrono::Utc::now();
    }
}

/// Conversation branch information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationBranch {
    /// Branch name.
    pub name: String,
    /// Parent branch (if any).
    pub parent: Option<String>,
    /// Message ID where branch started.
    pub fork_point: Option<String>,
    /// Creation timestamp.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Description.
    pub description: Option<String>,
}

impl ConversationBranch {
    /// Create the main branch.
    pub fn main() -> Self {
        Self {
            name: "main".to_string(),
            parent: None,
            fork_point: None,
            created_at: chrono::Utc::now(),
            description: None,
        }
    }

    /// Create a new branch.
    pub fn new(
        name: impl Into<String>,
        parent: impl Into<String>,
        fork_point: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            parent: Some(parent.into()),
            fork_point: Some(fork_point.into()),
            created_at: chrono::Utc::now(),
            description: None,
        }
    }

    /// Set description.
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }
}

/// A conversation with git history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    /// Unique conversation ID.
    pub id: String,
    /// Metadata.
    pub metadata: ConversationMetadata,
    /// Messages in the conversation.
    pub messages: Vec<Message>,
    /// Current branch.
    pub current_branch: String,
    /// Available branches.
    pub branches: Vec<ConversationBranch>,
    /// System prompt.
    pub system_prompt: Option<String>,
}

impl Conversation {
    /// Create a new conversation.
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            metadata: ConversationMetadata::with_title(title),
            messages: Vec::new(),
            current_branch: "main".to_string(),
            branches: vec![ConversationBranch::main()],
            system_prompt: None,
        }
    }

    /// Add a message.
    pub fn add_message(&mut self, message: Message) {
        self.messages.push(message);
        self.metadata.touch();
    }

    /// Get the last message.
    pub fn last_message(&self) -> Option<&Message> {
        self.messages.last()
    }

    /// Set system prompt.
    pub fn with_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }

    /// Create a branch from the current state.
    pub fn create_branch(&mut self, name: impl Into<String>) -> &ConversationBranch {
        let fork_point = self.messages.last().map(|m| m.id.clone());
        let branch = ConversationBranch {
            name: name.into(),
            parent: Some(self.current_branch.clone()),
            fork_point,
            created_at: chrono::Utc::now(),
            description: None,
        };
        self.branches.push(branch);
        self.branches.last().unwrap()
    }

    /// Switch to a branch.
    pub fn switch_branch(&mut self, name: &str) -> bool {
        if self.branches.iter().any(|b| b.name == name) {
            self.current_branch = name.to_string();
            true
        } else {
            false
        }
    }

    /// Convert to markdown format.
    pub fn to_markdown(&self) -> String {
        let mut md = format!("# {}\n\n", self.metadata.title);

        if let Some(system) = &self.system_prompt {
            md.push_str(&format!("*System: {}*\n\n---\n\n", system));
        }

        for message in &self.messages {
            md.push_str(&message.to_markdown());
            md.push_str("\n---\n\n");
        }

        md
    }

    /// Get message count.
    pub fn message_count(&self) -> usize {
        self.messages.len()
    }

    /// Check if conversation is empty.
    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_conversation_creation() {
        let conv = Conversation::new("Test Chat");
        assert_eq!(conv.metadata.title, "Test Chat");
        assert!(conv.is_empty());
    }

    #[test]
    fn test_add_message() {
        let mut conv = Conversation::new("Test");
        conv.add_message(Message::user("Hello"));
        conv.add_message(Message::assistant("Hi there!"));

        assert_eq!(conv.message_count(), 2);
    }

    #[test]
    fn test_branching() {
        let mut conv = Conversation::new("Test");
        conv.add_message(Message::user("Hello"));

        conv.create_branch("experiment");
        assert!(conv.switch_branch("experiment"));
        assert_eq!(conv.current_branch, "experiment");
    }

    #[test]
    fn test_markdown_export() {
        let mut conv = Conversation::new("Test Chat");
        conv.add_message(Message::user("Hello"));
        conv.add_message(Message::assistant("Hi!"));

        let md = conv.to_markdown();
        assert!(md.contains("# Test Chat"));
        assert!(md.contains("**User:**"));
        assert!(md.contains("Hello"));
    }
}
