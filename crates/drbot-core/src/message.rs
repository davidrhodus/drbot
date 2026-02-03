//! Message types for drbot.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Role of a message sender.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// System message (instructions/context).
    System,
    /// User message.
    User,
    /// Assistant message.
    Assistant,
}

/// Content type in a message.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Content {
    /// Plain text content.
    Text { text: String },
    /// Image content.
    Image {
        /// Base64 encoded image data or URL.
        source: ImageSource,
        /// Optional alt text.
        alt_text: Option<String>,
    },
    /// File attachment.
    File {
        /// File name.
        name: String,
        /// MIME type.
        mime_type: String,
        /// Base64 encoded data or URL.
        data: String,
    },
    /// Audio content.
    Audio {
        /// Base64 encoded audio data or URL.
        source: String,
        /// Duration in seconds.
        duration_secs: Option<f32>,
    },
    /// Tool use request (from assistant).
    ToolUse {
        /// Tool use ID.
        id: String,
        /// Tool name.
        name: String,
        /// Tool input as JSON.
        input: serde_json::Value,
    },
    /// Tool result (from user, in response to tool use).
    ToolResult {
        /// Tool use ID this result corresponds to.
        tool_use_id: String,
        /// Result content.
        content: String,
        /// Whether the tool execution failed.
        is_error: bool,
    },
}

/// Image source.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ImageSource {
    /// Base64 encoded image.
    Base64 {
        /// MIME type (e.g., "image/png").
        media_type: String,
        /// Base64 encoded data.
        data: String,
    },
    /// URL to image.
    Url { url: String },
}

/// A message in a conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Unique message ID.
    pub id: Uuid,
    /// Message role.
    pub role: Role,
    /// Message content blocks.
    pub content: Vec<Content>,
    /// Timestamp when the message was created.
    pub created_at: DateTime<Utc>,
    /// Optional metadata.
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub metadata: serde_json::Map<String, serde_json::Value>,
}

impl Message {
    /// Create a new text message.
    pub fn text(role: Role, text: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            role,
            content: vec![Content::Text { text: text.into() }],
            created_at: Utc::now(),
            metadata: serde_json::Map::new(),
        }
    }

    /// Create a new system message.
    pub fn system(text: impl Into<String>) -> Self {
        Self::text(Role::System, text)
    }

    /// Create a new user message.
    pub fn user(text: impl Into<String>) -> Self {
        Self::text(Role::User, text)
    }

    /// Create a new assistant message.
    pub fn assistant(text: impl Into<String>) -> Self {
        Self::text(Role::Assistant, text)
    }

    /// Get the text content of the message (concatenated if multiple text blocks).
    pub fn text_content(&self) -> String {
        self.content
            .iter()
            .filter_map(|c| match c {
                Content::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("")
    }
}

/// An incoming message from a channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncomingMessage {
    /// Unique message ID.
    pub id: Uuid,
    /// Channel type (e.g., "telegram", "discord").
    pub channel_type: String,
    /// Channel-specific conversation/chat ID.
    pub channel_id: String,
    /// Sender information.
    pub sender: MessageSender,
    /// Message content.
    pub content: Vec<Content>,
    /// Timestamp when the message was received.
    pub received_at: DateTime<Utc>,
    /// Raw message data from the platform.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw: Option<serde_json::Value>,
    /// Optional reply-to message ID.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<String>,
}

/// Sender of an incoming message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageSender {
    /// Platform-specific user ID.
    pub id: String,
    /// Display name.
    pub name: Option<String>,
    /// Username/handle.
    pub username: Option<String>,
}

/// An outgoing message to a channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutgoingMessage {
    /// Message content blocks.
    pub content: Vec<Content>,
    /// Optional reply-to message ID.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<String>,
    /// Optional metadata for the channel.
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub metadata: serde_json::Map<String, serde_json::Value>,
}

impl OutgoingMessage {
    /// Create a new text message.
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            content: vec![Content::Text { text: text.into() }],
            reply_to: None,
            metadata: serde_json::Map::new(),
        }
    }

    /// Set the reply-to message ID.
    pub fn reply_to(mut self, message_id: impl Into<String>) -> Self {
        self.reply_to = Some(message_id.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_text() {
        let msg = Message::user("Hello, world!");
        assert_eq!(msg.role, Role::User);
        assert_eq!(msg.text_content(), "Hello, world!");
    }

    #[test]
    fn test_message_serialization() {
        let msg = Message::assistant("Hi there!");
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.text_content(), "Hi there!");
    }
}
