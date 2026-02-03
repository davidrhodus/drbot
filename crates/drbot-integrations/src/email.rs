//! Email integration.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{IntegrationProvider, Result};

/// Email message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailMessage {
    /// Message ID.
    pub id: String,
    /// Thread ID.
    pub thread_id: Option<String>,
    /// From address.
    pub from: EmailAddress,
    /// To addresses.
    pub to: Vec<EmailAddress>,
    /// CC addresses.
    pub cc: Vec<EmailAddress>,
    /// BCC addresses.
    pub bcc: Vec<EmailAddress>,
    /// Subject.
    pub subject: String,
    /// Body (text).
    pub body_text: Option<String>,
    /// Body (HTML).
    pub body_html: Option<String>,
    /// Attachments.
    pub attachments: Vec<Attachment>,
    /// Is read.
    pub is_read: bool,
    /// Is starred.
    pub is_starred: bool,
    /// Labels.
    pub labels: Vec<String>,
    /// Sent at.
    pub sent_at: DateTime<Utc>,
    /// Received at.
    pub received_at: Option<DateTime<Utc>>,
}

/// Email address.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailAddress {
    /// Email address.
    pub email: String,
    /// Display name.
    pub name: Option<String>,
}

impl EmailAddress {
    /// Create from email string.
    pub fn new(email: &str) -> Self {
        Self {
            email: email.to_string(),
            name: None,
        }
    }

    /// Create with name.
    pub fn with_name(email: &str, name: &str) -> Self {
        Self {
            email: email.to_string(),
            name: Some(name.to_string()),
        }
    }
}

/// Email attachment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attachment {
    /// Attachment ID.
    pub id: String,
    /// File name.
    pub filename: String,
    /// MIME type.
    pub mime_type: String,
    /// Size in bytes.
    pub size: u64,
}

/// Email configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailConfig {
    /// Provider (gmail, outlook).
    pub provider: String,
    /// Primary email.
    pub primary_email: Option<String>,
    /// Sync folders.
    pub sync_folders: Vec<String>,
    /// Max sync count.
    pub max_sync: usize,
}

impl Default for EmailConfig {
    fn default() -> Self {
        Self {
            provider: "gmail".to_string(),
            primary_email: None,
            sync_folders: vec!["inbox".to_string()],
            max_sync: 100,
        }
    }
}

/// Email provider trait.
#[async_trait]
pub trait EmailProvider: IntegrationProvider {
    /// Get messages.
    async fn get_messages(
        &self,
        folder: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<EmailMessage>>;

    /// Get message by ID.
    async fn get_message(&self, id: &str) -> Result<EmailMessage>;

    /// Send message.
    async fn send(&self, message: SendEmail) -> Result<EmailMessage>;

    /// Reply to message.
    async fn reply(&self, message_id: &str, body: &str) -> Result<EmailMessage>;

    /// Forward message.
    async fn forward(&self, message_id: &str, to: Vec<EmailAddress>) -> Result<EmailMessage>;

    /// Mark as read.
    async fn mark_read(&self, message_id: &str) -> Result<()>;

    /// Mark as unread.
    async fn mark_unread(&self, message_id: &str) -> Result<()>;

    /// Star message.
    async fn star(&self, message_id: &str) -> Result<()>;

    /// Unstar message.
    async fn unstar(&self, message_id: &str) -> Result<()>;

    /// Move to folder.
    async fn move_to(&self, message_id: &str, folder: &str) -> Result<()>;

    /// Delete message.
    async fn delete(&self, message_id: &str) -> Result<()>;

    /// Search messages.
    async fn search(&self, query: &str, limit: usize) -> Result<Vec<EmailMessage>>;
}

/// Send email request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendEmail {
    /// To addresses.
    pub to: Vec<EmailAddress>,
    /// CC addresses.
    pub cc: Vec<EmailAddress>,
    /// BCC addresses.
    pub bcc: Vec<EmailAddress>,
    /// Subject.
    pub subject: String,
    /// Body (text).
    pub body_text: Option<String>,
    /// Body (HTML).
    pub body_html: Option<String>,
    /// Reply to message ID.
    pub reply_to: Option<String>,
}

impl SendEmail {
    /// Create a new email.
    pub fn new(to: &str, subject: &str, body: &str) -> Self {
        Self {
            to: vec![EmailAddress::new(to)],
            cc: Vec::new(),
            bcc: Vec::new(),
            subject: subject.to_string(),
            body_text: Some(body.to_string()),
            body_html: None,
            reply_to: None,
        }
    }

    /// Add CC.
    pub fn cc(mut self, email: &str) -> Self {
        self.cc.push(EmailAddress::new(email));
        self
    }

    /// Add BCC.
    pub fn bcc(mut self, email: &str) -> Self {
        self.bcc.push(EmailAddress::new(email));
        self
    }

    /// Set as HTML.
    pub fn html(mut self, html: &str) -> Self {
        self.body_html = Some(html.to_string());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_send_email() {
        let email = SendEmail::new("test@example.com", "Hello", "This is a test")
            .cc("cc@example.com")
            .html("<p>This is a test</p>");

        assert_eq!(email.to.len(), 1);
        assert_eq!(email.cc.len(), 1);
        assert!(email.body_html.is_some());
    }
}
