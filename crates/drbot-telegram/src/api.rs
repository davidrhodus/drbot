//! Telegram Bot API types.

use serde::{Deserialize, Serialize};

/// Response wrapper from Telegram API.
#[derive(Debug, Clone, Deserialize)]
pub struct ApiResponse<T> {
    /// Whether the request was successful.
    pub ok: bool,
    /// Result data (if successful).
    pub result: Option<T>,
    /// Error description (if failed).
    pub description: Option<String>,
    /// Error code (if failed).
    pub error_code: Option<i32>,
}

/// Telegram Update object.
#[derive(Debug, Clone, Deserialize)]
pub struct Update {
    /// Update ID.
    pub update_id: i64,
    /// New incoming message.
    pub message: Option<TelegramMessage>,
    /// New edited message.
    pub edited_message: Option<TelegramMessage>,
    /// Callback query from inline keyboard.
    pub callback_query: Option<CallbackQuery>,
}

/// Telegram Message object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramMessage {
    /// Unique message ID.
    pub message_id: i64,
    /// Sender of the message.
    pub from: Option<User>,
    /// Chat the message belongs to.
    pub chat: Chat,
    /// Date the message was sent (Unix timestamp).
    pub date: i64,
    /// Text content of the message.
    pub text: Option<String>,
    /// Caption for media messages.
    pub caption: Option<String>,
    /// Photo sizes (if message contains a photo).
    pub photo: Option<Vec<PhotoSize>>,
    /// Document (if message contains a file).
    pub document: Option<Document>,
    /// Audio file (if message contains audio).
    pub audio: Option<Audio>,
    /// Voice message (if message contains a voice note).
    pub voice: Option<Voice>,
    /// Reply to message.
    pub reply_to_message: Option<Box<TelegramMessage>>,
}

/// Telegram User object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    /// Unique user ID.
    pub id: i64,
    /// Whether user is a bot.
    pub is_bot: bool,
    /// User's first name.
    pub first_name: String,
    /// User's last name.
    pub last_name: Option<String>,
    /// User's username.
    pub username: Option<String>,
    /// User's language code.
    pub language_code: Option<String>,
}

impl User {
    /// Get full display name.
    pub fn full_name(&self) -> String {
        match &self.last_name {
            Some(last) => format!("{} {}", self.first_name, last),
            None => self.first_name.clone(),
        }
    }
}

/// Telegram Chat object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chat {
    /// Unique chat ID.
    pub id: i64,
    /// Type of chat.
    #[serde(rename = "type")]
    pub chat_type: String,
    /// Title (for groups/channels).
    pub title: Option<String>,
    /// Username (for private chats/channels).
    pub username: Option<String>,
    /// First name (for private chats).
    pub first_name: Option<String>,
    /// Last name (for private chats).
    pub last_name: Option<String>,
}

/// Photo size object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhotoSize {
    /// File ID.
    pub file_id: String,
    /// Unique file identifier.
    pub file_unique_id: String,
    /// Photo width.
    pub width: i32,
    /// Photo height.
    pub height: i32,
    /// File size.
    pub file_size: Option<i64>,
}

/// Document object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    /// File ID.
    pub file_id: String,
    /// Unique file identifier.
    pub file_unique_id: String,
    /// Original filename.
    pub file_name: Option<String>,
    /// MIME type.
    pub mime_type: Option<String>,
    /// File size.
    pub file_size: Option<i64>,
}

/// Audio object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Audio {
    /// File ID.
    pub file_id: String,
    /// Unique file identifier.
    pub file_unique_id: String,
    /// Duration in seconds.
    pub duration: i32,
    /// Performer.
    pub performer: Option<String>,
    /// Title.
    pub title: Option<String>,
    /// MIME type.
    pub mime_type: Option<String>,
    /// File size.
    pub file_size: Option<i64>,
}

/// Voice object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Voice {
    /// File ID.
    pub file_id: String,
    /// Unique file identifier.
    pub file_unique_id: String,
    /// Duration in seconds.
    pub duration: i32,
    /// MIME type.
    pub mime_type: Option<String>,
    /// File size.
    pub file_size: Option<i64>,
}

/// Callback query from inline keyboard.
#[derive(Debug, Clone, Deserialize)]
pub struct CallbackQuery {
    /// Unique callback ID.
    pub id: String,
    /// Sender.
    pub from: User,
    /// Message with callback button.
    pub message: Option<TelegramMessage>,
    /// Data associated with callback button.
    pub data: Option<String>,
}

/// File object from getFile.
#[derive(Debug, Clone, Deserialize)]
pub struct File {
    /// File ID.
    pub file_id: String,
    /// Unique file identifier.
    pub file_unique_id: String,
    /// File size.
    pub file_size: Option<i64>,
    /// File path for download.
    pub file_path: Option<String>,
}

/// Request to send a message.
#[derive(Debug, Clone, Serialize)]
pub struct SendMessageRequest {
    /// Target chat ID.
    pub chat_id: i64,
    /// Message text.
    pub text: String,
    /// Parse mode (HTML, Markdown, MarkdownV2).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parse_mode: Option<String>,
    /// Reply to message ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_to_message_id: Option<i64>,
    /// Disable link preview.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disable_web_page_preview: Option<bool>,
}

/// Request to get updates.
#[derive(Debug, Clone, Serialize)]
pub struct GetUpdatesRequest {
    /// Offset (ID of first update to return).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<i64>,
    /// Maximum number of updates to retrieve.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<i32>,
    /// Timeout for long polling.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<i32>,
    /// List of update types to receive.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_updates: Option<Vec<String>>,
}
