//! Conversation export for drbot.
//!
//! Export conversations to various formats (JSON, Markdown, HTML, PDF).

mod exporter;
mod formats;
mod template;

pub use exporter::{ExportOptions, ExportResult, Exporter};
pub use formats::{ExportFormat, HtmlExporter, JsonExporter, MarkdownExporter};
pub use template::{Template, TemplateEngine};

use chrono::{DateTime, Utc};
use drbot_core::message::Message;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Export result.
pub type Result<T> = std::result::Result<T, ExportError>;

/// Export errors.
#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    #[error("Export failed: {0}")]
    ExportFailed(String),
    #[error("Unsupported format: {0}")]
    UnsupportedFormat(String),
    #[error("Template error: {0}")]
    TemplateError(String),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

/// Export configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportConfig {
    /// Default export format.
    pub default_format: String,
    /// Output directory.
    pub output_dir: Option<String>,
    /// Include metadata.
    pub include_metadata: bool,
    /// Include timestamps.
    pub include_timestamps: bool,
    /// Custom CSS for HTML export.
    pub custom_css: Option<String>,
}

impl Default for ExportConfig {
    fn default() -> Self {
        Self {
            default_format: "markdown".to_string(),
            output_dir: None,
            include_metadata: true,
            include_timestamps: true,
            custom_css: None,
        }
    }
}

/// A conversation to export.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    /// Conversation ID.
    pub id: Uuid,
    /// Conversation title.
    pub title: String,
    /// Channel ID.
    pub channel_id: String,
    /// Participants.
    pub participants: Vec<String>,
    /// Messages.
    pub messages: Vec<ExportMessage>,
    /// Start time.
    pub started_at: DateTime<Utc>,
    /// End time.
    pub ended_at: Option<DateTime<Utc>>,
    /// Metadata.
    pub metadata: std::collections::HashMap<String, serde_json::Value>,
}

impl Conversation {
    /// Create a new conversation for export.
    pub fn new(id: Uuid, title: &str, channel_id: &str) -> Self {
        Self {
            id,
            title: title.to_string(),
            channel_id: channel_id.to_string(),
            participants: Vec::new(),
            messages: Vec::new(),
            started_at: Utc::now(),
            ended_at: None,
            metadata: std::collections::HashMap::new(),
        }
    }

    /// Add a message.
    pub fn add_message(&mut self, message: ExportMessage) {
        self.messages.push(message);
    }

    /// Add a participant.
    pub fn add_participant(&mut self, participant: &str) {
        if !self.participants.contains(&participant.to_string()) {
            self.participants.push(participant.to_string());
        }
    }
}

/// A message for export.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportMessage {
    /// Message ID.
    pub id: Uuid,
    /// Sender.
    pub sender: String,
    /// Role (user/assistant/system).
    pub role: String,
    /// Content.
    pub content: String,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
    /// Attachments.
    pub attachments: Vec<Attachment>,
}

/// An attachment in a message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attachment {
    /// Attachment ID.
    pub id: Uuid,
    /// Filename.
    pub filename: String,
    /// MIME type.
    pub mime_type: String,
    /// Size in bytes.
    pub size: u64,
    /// URL or path.
    pub url: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_export_config_default() {
        let config = ExportConfig::default();
        assert_eq!(config.default_format, "markdown");
        assert!(config.include_timestamps);
    }

    #[test]
    fn test_conversation() {
        let mut conv = Conversation::new(Uuid::new_v4(), "Test", "channel1");
        conv.add_participant("Alice");
        conv.add_participant("Bot");
        conv.add_participant("Alice"); // Duplicate

        assert_eq!(conv.participants.len(), 2);
    }
}
