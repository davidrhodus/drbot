//! Main exporter interface.

use crate::formats::{ExportFormat, HtmlExporter, JsonExporter, MarkdownExporter};
use crate::{Conversation, ExportConfig, ExportError, Result};
use std::path::Path;
use tracing::info;

/// Export options.
#[derive(Debug, Clone)]
pub struct ExportOptions {
    /// Export format.
    pub format: ExportFormat,
    /// Include metadata.
    pub include_metadata: bool,
    /// Include timestamps.
    pub include_timestamps: bool,
    /// Include attachments.
    pub include_attachments: bool,
    /// Custom title.
    pub custom_title: Option<String>,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            format: ExportFormat::Markdown,
            include_metadata: true,
            include_timestamps: true,
            include_attachments: false,
            custom_title: None,
        }
    }
}

/// Export result.
#[derive(Debug, Clone)]
pub struct ExportResult {
    /// Exported content.
    pub content: String,
    /// Content type (MIME type).
    pub content_type: String,
    /// Suggested filename.
    pub filename: String,
    /// Size in bytes.
    pub size: usize,
}

/// Main exporter.
pub struct Exporter {
    config: ExportConfig,
}

impl Exporter {
    /// Create a new exporter.
    pub fn new(config: ExportConfig) -> Self {
        Self { config }
    }

    /// Export a conversation.
    pub fn export(
        &self,
        conversation: &Conversation,
        options: &ExportOptions,
    ) -> Result<ExportResult> {
        let content = match options.format {
            ExportFormat::Json => JsonExporter::export(conversation, options)?,
            ExportFormat::Markdown => MarkdownExporter::export(conversation, options)?,
            ExportFormat::Html => HtmlExporter::export(conversation, options, &self.config)?,
            ExportFormat::Text => self.export_text(conversation, options)?,
        };

        let content_type = options.format.content_type().to_string();
        let extension = options.format.extension();
        let filename = format!("{}.{}", sanitize_filename(&conversation.title), extension);

        Ok(ExportResult {
            size: content.len(),
            content,
            content_type,
            filename,
        })
    }

    /// Export to file.
    pub fn export_to_file(
        &self,
        conversation: &Conversation,
        options: &ExportOptions,
        path: &Path,
    ) -> Result<ExportResult> {
        let result = self.export(conversation, options)?;

        std::fs::write(path, &result.content).map_err(|e| ExportError::IoError(e))?;

        info!("Exported conversation to {:?}", path);

        Ok(result)
    }

    /// Export as plain text.
    fn export_text(&self, conversation: &Conversation, options: &ExportOptions) -> Result<String> {
        let mut output = String::new();

        // Title
        let title = options.custom_title.as_ref().unwrap_or(&conversation.title);
        output.push_str(title);
        output.push_str("\n");
        output.push_str(&"=".repeat(title.len()));
        output.push_str("\n\n");

        // Metadata
        if options.include_metadata {
            output.push_str(&format!("Channel: {}\n", conversation.channel_id));
            output.push_str(&format!(
                "Started: {}\n",
                conversation.started_at.format("%Y-%m-%d %H:%M")
            ));
            if let Some(ended) = conversation.ended_at {
                output.push_str(&format!("Ended: {}\n", ended.format("%Y-%m-%d %H:%M")));
            }
            output.push_str(&format!(
                "Participants: {}\n",
                conversation.participants.join(", ")
            ));
            output.push_str("\n---\n\n");
        }

        // Messages
        for message in &conversation.messages {
            if options.include_timestamps {
                output.push_str(&format!("[{}] ", message.timestamp.format("%H:%M")));
            }
            output.push_str(&format!("{}: {}\n\n", message.sender, message.content));
        }

        Ok(output)
    }

    /// Export multiple conversations.
    pub fn export_batch(
        &self,
        conversations: &[Conversation],
        options: &ExportOptions,
    ) -> Result<Vec<ExportResult>> {
        conversations
            .iter()
            .map(|c| self.export(c, options))
            .collect()
    }
}

impl Default for Exporter {
    fn default() -> Self {
        Self::new(ExportConfig::default())
    }
}

/// Sanitize a filename.
fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect::<String>()
        .chars()
        .take(50)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn test_conversation() -> Conversation {
        Conversation::new(Uuid::new_v4(), "Test Conversation", "channel1")
    }

    #[test]
    fn test_export_text() {
        let exporter = Exporter::default();
        let conversation = test_conversation();
        let options = ExportOptions {
            format: ExportFormat::Text,
            ..Default::default()
        };

        let result = exporter.export(&conversation, &options).unwrap();
        assert!(result.content.contains("Test Conversation"));
        assert_eq!(result.content_type, "text/plain");
    }

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(sanitize_filename("Hello World!"), "Hello_World_");
        assert_eq!(sanitize_filename("test-file_name"), "test-file_name");
    }
}
