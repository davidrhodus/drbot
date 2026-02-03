//! Message types for git-stored conversations.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Message role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    /// User message.
    User,
    /// Assistant message.
    Assistant,
    /// System message.
    System,
    /// Tool/function result.
    Tool,
}

impl std::fmt::Display for MessageRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MessageRole::User => write!(f, "user"),
            MessageRole::Assistant => write!(f, "assistant"),
            MessageRole::System => write!(f, "system"),
            MessageRole::Tool => write!(f, "tool"),
        }
    }
}

impl MessageRole {
    /// Parse from string.
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "user" => Some(MessageRole::User),
            "assistant" => Some(MessageRole::Assistant),
            "system" => Some(MessageRole::System),
            "tool" => Some(MessageRole::Tool),
            _ => None,
        }
    }
}

/// File attachment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attachment {
    /// Attachment ID.
    pub id: String,
    /// File name.
    pub filename: String,
    /// MIME type.
    pub mime_type: String,
    /// File size in bytes.
    pub size: u64,
    /// Relative path in the repository.
    pub path: String,
}

impl Attachment {
    /// Create a new attachment.
    pub fn new(
        filename: impl Into<String>,
        mime_type: impl Into<String>,
        path: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            filename: filename.into(),
            mime_type: mime_type.into(),
            size: 0,
            path: path.into(),
        }
    }

    /// Set file size.
    pub fn with_size(mut self, size: u64) -> Self {
        self.size = size;
        self
    }
}

/// A conversation message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Message ID.
    pub id: String,
    /// Message role.
    pub role: MessageRole,
    /// Message content.
    pub content: String,
    /// Timestamp.
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Attachments.
    pub attachments: Vec<Attachment>,
    /// Metadata.
    pub metadata: std::collections::HashMap<String, serde_json::Value>,
    /// Parent message ID (for threading).
    pub parent_id: Option<String>,
    /// Whether this message was edited.
    pub edited: bool,
    /// Original content if edited.
    pub original_content: Option<String>,
}

impl Message {
    /// Create a new message.
    pub fn new(role: MessageRole, content: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            role,
            content: content.into(),
            timestamp: chrono::Utc::now(),
            attachments: Vec::new(),
            metadata: std::collections::HashMap::new(),
            parent_id: None,
            edited: false,
            original_content: None,
        }
    }

    /// Create a user message.
    pub fn user(content: impl Into<String>) -> Self {
        Self::new(MessageRole::User, content)
    }

    /// Create an assistant message.
    pub fn assistant(content: impl Into<String>) -> Self {
        Self::new(MessageRole::Assistant, content)
    }

    /// Create a system message.
    pub fn system(content: impl Into<String>) -> Self {
        Self::new(MessageRole::System, content)
    }

    /// Add an attachment.
    pub fn with_attachment(mut self, attachment: Attachment) -> Self {
        self.attachments.push(attachment);
        self
    }

    /// Set parent message.
    pub fn with_parent(mut self, parent_id: impl Into<String>) -> Self {
        self.parent_id = Some(parent_id.into());
        self
    }

    /// Add metadata.
    pub fn with_metadata(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.metadata.insert(key.into(), value);
        self
    }

    /// Edit the message content.
    pub fn edit(&mut self, new_content: impl Into<String>) {
        if !self.edited {
            self.original_content = Some(self.content.clone());
            self.edited = true;
        }
        self.content = new_content.into();
    }

    /// Convert to markdown format.
    pub fn to_markdown(&self) -> String {
        let role_prefix = match self.role {
            MessageRole::User => "**User:**",
            MessageRole::Assistant => "**Assistant:**",
            MessageRole::System => "**System:**",
            MessageRole::Tool => "**Tool:**",
        };

        let mut md = format!("{}\n\n{}\n", role_prefix, self.content);

        if !self.attachments.is_empty() {
            md.push_str("\n*Attachments:*\n");
            for att in &self.attachments {
                md.push_str(&format!("- {} ({})\n", att.filename, att.mime_type));
            }
        }

        md
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_creation() {
        let msg = Message::user("Hello!");
        assert_eq!(msg.role, MessageRole::User);
        assert_eq!(msg.content, "Hello!");
    }

    #[test]
    fn test_message_edit() {
        let mut msg = Message::user("Original");
        msg.edit("Edited");

        assert!(msg.edited);
        assert_eq!(msg.content, "Edited");
        assert_eq!(msg.original_content, Some("Original".to_string()));
    }

    #[test]
    fn test_message_markdown() {
        let msg = Message::user("Hello!");
        let md = msg.to_markdown();
        assert!(md.contains("**User:**"));
        assert!(md.contains("Hello!"));
    }
}
