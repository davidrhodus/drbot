//! Export formats.

use crate::{Conversation, ExportConfig, ExportError, ExportOptions, Result};
use serde::{Deserialize, Serialize};

/// Available export formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExportFormat {
    /// JSON format.
    Json,
    /// Markdown format.
    Markdown,
    /// HTML format.
    Html,
    /// Plain text format.
    Text,
}

impl ExportFormat {
    /// Get content type.
    pub fn content_type(&self) -> &str {
        match self {
            ExportFormat::Json => "application/json",
            ExportFormat::Markdown => "text/markdown",
            ExportFormat::Html => "text/html",
            ExportFormat::Text => "text/plain",
        }
    }

    /// Get file extension.
    pub fn extension(&self) -> &str {
        match self {
            ExportFormat::Json => "json",
            ExportFormat::Markdown => "md",
            ExportFormat::Html => "html",
            ExportFormat::Text => "txt",
        }
    }
}

/// JSON exporter.
pub struct JsonExporter;

impl JsonExporter {
    /// Export to JSON.
    pub fn export(conversation: &Conversation, options: &ExportOptions) -> Result<String> {
        let output = if options.include_metadata {
            serde_json::to_string_pretty(conversation)
        } else {
            // Export only messages
            serde_json::to_string_pretty(&conversation.messages)
        };

        output.map_err(|e| ExportError::ExportFailed(e.to_string()))
    }
}

/// Markdown exporter.
pub struct MarkdownExporter;

impl MarkdownExporter {
    /// Export to Markdown.
    pub fn export(conversation: &Conversation, options: &ExportOptions) -> Result<String> {
        let mut output = String::new();

        // Title
        let title = options.custom_title.as_ref().unwrap_or(&conversation.title);
        output.push_str(&format!("# {}\n\n", title));

        // Metadata
        if options.include_metadata {
            output.push_str("## Metadata\n\n");
            output.push_str(&format!("- **Channel:** {}\n", conversation.channel_id));
            output.push_str(&format!(
                "- **Started:** {}\n",
                conversation.started_at.format("%Y-%m-%d %H:%M:%S UTC")
            ));
            if let Some(ended) = conversation.ended_at {
                output.push_str(&format!(
                    "- **Ended:** {}\n",
                    ended.format("%Y-%m-%d %H:%M:%S UTC")
                ));
            }
            output.push_str(&format!(
                "- **Participants:** {}\n",
                conversation.participants.join(", ")
            ));
            output.push_str("\n---\n\n");
        }

        // Messages
        output.push_str("## Conversation\n\n");

        for message in &conversation.messages {
            let role_emoji = match message.role.as_str() {
                "user" => "👤",
                "assistant" => "🤖",
                "system" => "⚙️",
                _ => "💬",
            };

            if options.include_timestamps {
                output.push_str(&format!(
                    "### {} {} *{}*\n\n",
                    role_emoji,
                    message.sender,
                    message.timestamp.format("%Y-%m-%d %H:%M")
                ));
            } else {
                output.push_str(&format!("### {} {}\n\n", role_emoji, message.sender));
            }

            output.push_str(&message.content);
            output.push_str("\n\n");

            // Attachments
            if options.include_attachments && !message.attachments.is_empty() {
                output.push_str("**Attachments:**\n");
                for attachment in &message.attachments {
                    output.push_str(&format!(
                        "- {} ({}, {} bytes)\n",
                        attachment.filename, attachment.mime_type, attachment.size
                    ));
                }
                output.push_str("\n");
            }
        }

        Ok(output)
    }
}

/// HTML exporter.
pub struct HtmlExporter;

impl HtmlExporter {
    /// Export to HTML.
    pub fn export(
        conversation: &Conversation,
        options: &ExportOptions,
        config: &ExportConfig,
    ) -> Result<String> {
        let title = options.custom_title.as_ref().unwrap_or(&conversation.title);

        let css = config
            .custom_css
            .as_ref()
            .map(|s| s.as_str())
            .unwrap_or(DEFAULT_CSS);

        let mut output = format!(
            r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>{}</title>
    <style>
{}
    </style>
</head>
<body>
    <div class="container">
        <h1>{}</h1>
"#,
            escape_html(title),
            css,
            escape_html(title)
        );

        // Metadata
        if options.include_metadata {
            output.push_str(
                r#"        <div class="metadata">
"#,
            );
            output.push_str(&format!(
                "            <p><strong>Channel:</strong> {}</p>\n",
                escape_html(&conversation.channel_id)
            ));
            output.push_str(&format!(
                "            <p><strong>Started:</strong> {}</p>\n",
                conversation.started_at.format("%Y-%m-%d %H:%M:%S UTC")
            ));
            if let Some(ended) = conversation.ended_at {
                output.push_str(&format!(
                    "            <p><strong>Ended:</strong> {}</p>\n",
                    ended.format("%Y-%m-%d %H:%M:%S UTC")
                ));
            }
            output.push_str(&format!(
                "            <p><strong>Participants:</strong> {}</p>\n",
                escape_html(&conversation.participants.join(", "))
            ));
            output.push_str("        </div>\n");
        }

        // Messages
        output.push_str(
            r#"        <div class="messages">
"#,
        );

        for message in &conversation.messages {
            let role_class = match message.role.as_str() {
                "user" => "user",
                "assistant" => "assistant",
                "system" => "system",
                _ => "other",
            };

            output.push_str(&format!(
                r#"            <div class="message {}">
                <div class="message-header">
                    <span class="sender">{}</span>
"#,
                role_class,
                escape_html(&message.sender)
            ));

            if options.include_timestamps {
                output.push_str(&format!(
                    r#"                    <span class="timestamp">{}</span>
"#,
                    message.timestamp.format("%Y-%m-%d %H:%M")
                ));
            }

            output.push_str(
                r#"                </div>
"#,
            );

            output.push_str(&format!(
                r#"                <div class="content">{}</div>
"#,
                escape_html(&message.content).replace('\n', "<br>")
            ));

            output.push_str("            </div>\n");
        }

        output.push_str(
            r#"        </div>
    </div>
</body>
</html>"#,
        );

        Ok(output)
    }
}

/// Escape HTML entities.
fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

/// Default CSS for HTML export.
const DEFAULT_CSS: &str = r#"
        body {
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
            line-height: 1.6;
            color: #333;
            max-width: 800px;
            margin: 0 auto;
            padding: 20px;
            background: #f5f5f5;
        }
        .container {
            background: white;
            border-radius: 8px;
            padding: 30px;
            box-shadow: 0 2px 10px rgba(0,0,0,0.1);
        }
        h1 {
            border-bottom: 2px solid #eee;
            padding-bottom: 10px;
        }
        .metadata {
            background: #f9f9f9;
            padding: 15px;
            border-radius: 5px;
            margin-bottom: 20px;
        }
        .metadata p {
            margin: 5px 0;
        }
        .messages {
            display: flex;
            flex-direction: column;
            gap: 15px;
        }
        .message {
            padding: 15px;
            border-radius: 10px;
            max-width: 85%;
        }
        .message.user {
            background: #e3f2fd;
            margin-left: auto;
        }
        .message.assistant {
            background: #f5f5f5;
        }
        .message.system {
            background: #fff3e0;
            font-style: italic;
        }
        .message-header {
            display: flex;
            justify-content: space-between;
            margin-bottom: 8px;
            font-size: 0.9em;
        }
        .sender {
            font-weight: bold;
        }
        .timestamp {
            color: #666;
        }
        .content {
            white-space: pre-wrap;
        }
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ExportMessage;
    use chrono::Utc;
    use uuid::Uuid;

    fn test_conversation() -> Conversation {
        let mut conv = Conversation::new(Uuid::new_v4(), "Test", "channel1");
        conv.messages.push(ExportMessage {
            id: Uuid::new_v4(),
            sender: "User".to_string(),
            role: "user".to_string(),
            content: "Hello!".to_string(),
            timestamp: Utc::now(),
            attachments: vec![],
        });
        conv
    }

    #[test]
    fn test_json_export() {
        let conv = test_conversation();
        let options = ExportOptions::default();
        let result = JsonExporter::export(&conv, &options).unwrap();
        assert!(result.contains("Hello!"));
    }

    #[test]
    fn test_markdown_export() {
        let conv = test_conversation();
        let options = ExportOptions::default();
        let result = MarkdownExporter::export(&conv, &options).unwrap();
        assert!(result.contains("# Test"));
        assert!(result.contains("Hello!"));
    }

    #[test]
    fn test_html_export() {
        let conv = test_conversation();
        let options = ExportOptions::default();
        let config = ExportConfig::default();
        let result = HtmlExporter::export(&conv, &options, &config).unwrap();
        assert!(result.contains("<!DOCTYPE html>"));
        assert!(result.contains("Hello!"));
    }
}
