//! Notification system for drbot.
//!
//! This crate provides:
//! - Multi-channel notifications
//! - Notification templates
//! - Delivery tracking
//! - Preference management

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Notification error types.
#[derive(Error, Debug)]
pub enum NotificationError {
    #[error("Channel not found: {0}")]
    ChannelNotFound(String),

    #[error("Template not found: {0}")]
    TemplateNotFound(String),

    #[error("Delivery failed: {0}")]
    DeliveryFailed(String),

    #[error("Invalid recipient: {0}")]
    InvalidRecipient(String),
}

/// Result type for notification operations.
pub type Result<T> = std::result::Result<T, NotificationError>;

/// Notification priority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Priority {
    Low,
    Normal,
    High,
    Urgent,
}

/// Notification status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeliveryStatus {
    /// Pending delivery.
    Pending,
    /// Sent.
    Sent,
    /// Delivered.
    Delivered,
    /// Failed.
    Failed,
    /// Read by recipient.
    Read,
}

/// Notification channel type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChannelType {
    Email,
    Sms,
    Push,
    Webhook,
    InApp,
    Slack,
    Discord,
    Custom(String),
}

/// A notification recipient.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recipient {
    /// Recipient ID.
    pub id: String,
    /// Email address.
    pub email: Option<String>,
    /// Phone number.
    pub phone: Option<String>,
    /// Push token.
    pub push_token: Option<String>,
    /// Webhook URL.
    pub webhook_url: Option<String>,
    /// Preferred channels.
    pub preferred_channels: Vec<ChannelType>,
    /// Notification preferences.
    pub preferences: HashMap<String, bool>,
}

impl Recipient {
    /// Create a new recipient.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            email: None,
            phone: None,
            push_token: None,
            webhook_url: None,
            preferred_channels: Vec::new(),
            preferences: HashMap::new(),
        }
    }

    /// Set email.
    pub fn with_email(mut self, email: impl Into<String>) -> Self {
        self.email = Some(email.into());
        self
    }

    /// Set phone.
    pub fn with_phone(mut self, phone: impl Into<String>) -> Self {
        self.phone = Some(phone.into());
        self
    }

    /// Add preferred channel.
    pub fn prefers(mut self, channel: ChannelType) -> Self {
        self.preferred_channels.push(channel);
        self
    }
}

/// A notification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
    /// Notification ID.
    pub id: Uuid,
    /// Notification type.
    pub notification_type: String,
    /// Subject/title.
    pub subject: String,
    /// Body content.
    pub body: String,
    /// Template ID (if using template).
    pub template_id: Option<String>,
    /// Template variables.
    pub variables: HashMap<String, serde_json::Value>,
    /// Recipients.
    pub recipients: Vec<String>,
    /// Priority.
    pub priority: Priority,
    /// Channels to use.
    pub channels: Vec<ChannelType>,
    /// Created at.
    pub created_at: DateTime<Utc>,
    /// Scheduled for.
    pub scheduled_at: Option<DateTime<Utc>>,
    /// Expires at.
    pub expires_at: Option<DateTime<Utc>>,
    /// Metadata.
    pub metadata: HashMap<String, String>,
}

impl Notification {
    /// Create a new notification.
    pub fn new(
        notification_type: impl Into<String>,
        subject: impl Into<String>,
        body: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            notification_type: notification_type.into(),
            subject: subject.into(),
            body: body.into(),
            template_id: None,
            variables: HashMap::new(),
            recipients: Vec::new(),
            priority: Priority::Normal,
            channels: Vec::new(),
            created_at: Utc::now(),
            scheduled_at: None,
            expires_at: None,
            metadata: HashMap::new(),
        }
    }

    /// Add recipient.
    pub fn to(mut self, recipient_id: impl Into<String>) -> Self {
        self.recipients.push(recipient_id.into());
        self
    }

    /// Set priority.
    pub fn with_priority(mut self, priority: Priority) -> Self {
        self.priority = priority;
        self
    }

    /// Use specific channel.
    pub fn via(mut self, channel: ChannelType) -> Self {
        self.channels.push(channel);
        self
    }

    /// Use template.
    pub fn from_template(mut self, template_id: impl Into<String>) -> Self {
        self.template_id = Some(template_id.into());
        self
    }

    /// Add variable.
    pub fn with_variable(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.variables.insert(key.into(), value);
        self
    }

    /// Schedule for later.
    pub fn schedule_at(mut self, time: DateTime<Utc>) -> Self {
        self.scheduled_at = Some(time);
        self
    }
}

/// Notification template.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Template {
    /// Template ID.
    pub id: String,
    /// Template name.
    pub name: String,
    /// Subject template.
    pub subject: String,
    /// Body template.
    pub body: String,
    /// HTML body template.
    pub html_body: Option<String>,
    /// Supported channels.
    pub channels: Vec<ChannelType>,
}

impl Template {
    /// Create a new template.
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            subject: String::new(),
            body: String::new(),
            html_body: None,
            channels: Vec::new(),
        }
    }

    /// Set subject template.
    pub fn with_subject(mut self, subject: impl Into<String>) -> Self {
        self.subject = subject.into();
        self
    }

    /// Set body template.
    pub fn with_body(mut self, body: impl Into<String>) -> Self {
        self.body = body.into();
        self
    }

    /// Render template with variables.
    pub fn render(&self, variables: &HashMap<String, serde_json::Value>) -> (String, String) {
        let mut subject = self.subject.clone();
        let mut body = self.body.clone();

        for (key, value) in variables {
            let placeholder = format!("{{{{{}}}}}", key);
            let replacement = match value {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            subject = subject.replace(&placeholder, &replacement);
            body = body.replace(&placeholder, &replacement);
        }

        (subject, body)
    }
}

/// Delivery result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliveryResult {
    /// Notification ID.
    pub notification_id: Uuid,
    /// Recipient ID.
    pub recipient_id: String,
    /// Channel used.
    pub channel: ChannelType,
    /// Status.
    pub status: DeliveryStatus,
    /// Sent at.
    pub sent_at: Option<DateTime<Utc>>,
    /// Delivered at.
    pub delivered_at: Option<DateTime<Utc>>,
    /// Error message.
    pub error: Option<String>,
}

/// Notification channel trait.
#[async_trait]
pub trait NotificationChannel: Send + Sync {
    /// Get channel type.
    fn channel_type(&self) -> ChannelType;

    /// Send notification.
    async fn send(
        &self,
        notification: &Notification,
        recipient: &Recipient,
    ) -> Result<DeliveryResult>;

    /// Check if recipient can receive via this channel.
    fn can_deliver_to(&self, recipient: &Recipient) -> bool;
}

/// In-app notification channel.
pub struct InAppChannel {
    notifications: RwLock<HashMap<String, Vec<Notification>>>,
}

impl InAppChannel {
    /// Create new channel.
    pub fn new() -> Self {
        Self {
            notifications: RwLock::new(HashMap::new()),
        }
    }

    /// Get unread notifications for user.
    pub async fn get_unread(&self, user_id: &str) -> Vec<Notification> {
        let notifications = self.notifications.read().await;
        notifications.get(user_id).cloned().unwrap_or_default()
    }

    /// Mark as read.
    pub async fn mark_read(&self, user_id: &str, notification_id: Uuid) {
        let mut notifications = self.notifications.write().await;
        if let Some(list) = notifications.get_mut(user_id) {
            list.retain(|n| n.id != notification_id);
        }
    }
}

impl Default for InAppChannel {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl NotificationChannel for InAppChannel {
    fn channel_type(&self) -> ChannelType {
        ChannelType::InApp
    }

    async fn send(
        &self,
        notification: &Notification,
        recipient: &Recipient,
    ) -> Result<DeliveryResult> {
        let mut notifications = self.notifications.write().await;
        notifications
            .entry(recipient.id.clone())
            .or_default()
            .push(notification.clone());

        Ok(DeliveryResult {
            notification_id: notification.id,
            recipient_id: recipient.id.clone(),
            channel: ChannelType::InApp,
            status: DeliveryStatus::Delivered,
            sent_at: Some(Utc::now()),
            delivered_at: Some(Utc::now()),
            error: None,
        })
    }

    fn can_deliver_to(&self, _recipient: &Recipient) -> bool {
        true // In-app always available
    }
}

/// Notification service.
pub struct NotificationService {
    channels: RwLock<HashMap<String, Arc<dyn NotificationChannel>>>,
    templates: RwLock<HashMap<String, Template>>,
    recipients: RwLock<HashMap<String, Recipient>>,
    history: RwLock<Vec<DeliveryResult>>,
}

impl NotificationService {
    /// Create new service.
    pub fn new() -> Self {
        Self {
            channels: RwLock::new(HashMap::new()),
            templates: RwLock::new(HashMap::new()),
            recipients: RwLock::new(HashMap::new()),
            history: RwLock::new(Vec::new()),
        }
    }

    /// Register a channel.
    pub async fn register_channel(
        &self,
        name: impl Into<String>,
        channel: Arc<dyn NotificationChannel>,
    ) {
        let mut channels = self.channels.write().await;
        channels.insert(name.into(), channel);
    }

    /// Register a template.
    pub async fn register_template(&self, template: Template) {
        let mut templates = self.templates.write().await;
        templates.insert(template.id.clone(), template);
    }

    /// Register a recipient.
    pub async fn register_recipient(&self, recipient: Recipient) {
        let mut recipients = self.recipients.write().await;
        recipients.insert(recipient.id.clone(), recipient);
    }

    /// Send a notification.
    pub async fn send(&self, notification: Notification) -> Result<Vec<DeliveryResult>> {
        let channels = self.channels.read().await;
        let recipients = self.recipients.read().await;
        let templates = self.templates.read().await;

        // Apply template if specified
        let notification = if let Some(template_id) = &notification.template_id {
            let template = templates
                .get(template_id)
                .ok_or_else(|| NotificationError::TemplateNotFound(template_id.clone()))?;
            let (subject, body) = template.render(&notification.variables);
            Notification {
                subject,
                body,
                ..notification
            }
        } else {
            notification
        };

        let mut results = Vec::new();

        for recipient_id in &notification.recipients {
            let recipient = recipients
                .get(recipient_id)
                .ok_or_else(|| NotificationError::InvalidRecipient(recipient_id.clone()))?;

            // Determine channels to use
            let channels_to_use = if notification.channels.is_empty() {
                &recipient.preferred_channels
            } else {
                &notification.channels
            };

            for channel_type in channels_to_use {
                let channel_name = match channel_type {
                    ChannelType::Email => "email",
                    ChannelType::InApp => "inapp",
                    ChannelType::Push => "push",
                    ChannelType::Sms => "sms",
                    ChannelType::Webhook => "webhook",
                    ChannelType::Slack => "slack",
                    ChannelType::Discord => "discord",
                    ChannelType::Custom(name) => name.as_str(),
                };

                if let Some(channel) = channels.get(channel_name) {
                    if channel.can_deliver_to(recipient) {
                        let result = channel.send(&notification, recipient).await?;
                        results.push(result);
                    }
                }
            }
        }

        // Store history
        {
            let mut history = self.history.write().await;
            history.extend(results.clone());
        }

        Ok(results)
    }

    /// Get delivery history.
    pub async fn get_history(&self, notification_id: Option<Uuid>) -> Vec<DeliveryResult> {
        let history = self.history.read().await;
        if let Some(id) = notification_id {
            history
                .iter()
                .filter(|r| r.notification_id == id)
                .cloned()
                .collect()
        } else {
            history.clone()
        }
    }
}

impl Default for NotificationService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_notification_creation() {
        let notification = Notification::new("alert", "Test Alert", "This is a test")
            .to("user-123")
            .with_priority(Priority::High)
            .via(ChannelType::Email);

        assert_eq!(notification.subject, "Test Alert");
        assert_eq!(notification.recipients.len(), 1);
    }

    #[test]
    fn test_template_rendering() {
        let template = Template::new("welcome", "Welcome Email")
            .with_subject("Welcome, {{name}}!")
            .with_body("Hello {{name}}, welcome to {{app}}.");

        let mut vars = HashMap::new();
        vars.insert("name".to_string(), serde_json::json!("Alice"));
        vars.insert("app".to_string(), serde_json::json!("MyApp"));

        let (subject, body) = template.render(&vars);

        assert_eq!(subject, "Welcome, Alice!");
        assert_eq!(body, "Hello Alice, welcome to MyApp.");
    }

    #[test]
    fn test_recipient_creation() {
        let recipient = Recipient::new("user-123")
            .with_email("user@example.com")
            .prefers(ChannelType::Email);

        assert_eq!(recipient.id, "user-123");
        assert!(recipient.email.is_some());
    }

    #[tokio::test]
    async fn test_in_app_channel() {
        let channel = InAppChannel::new();

        let notification = Notification::new("test", "Test", "Body");
        let recipient = Recipient::new("user-123");

        let result = channel.send(&notification, &recipient).await.unwrap();
        assert_eq!(result.status, DeliveryStatus::Delivered);

        let unread = channel.get_unread("user-123").await;
        assert_eq!(unread.len(), 1);

        channel.mark_read("user-123", notification.id).await;
        let unread = channel.get_unread("user-123").await;
        assert!(unread.is_empty());
    }

    #[tokio::test]
    async fn test_notification_service() {
        let service = NotificationService::new();

        // Register channel
        service
            .register_channel("inapp", Arc::new(InAppChannel::new()))
            .await;

        // Register recipient
        let recipient = Recipient::new("user-123").prefers(ChannelType::InApp);
        service.register_recipient(recipient).await;

        // Send notification
        let notification = Notification::new("test", "Test", "Body")
            .to("user-123")
            .via(ChannelType::InApp);

        let results = service.send(notification).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].status, DeliveryStatus::Delivered);
    }

    #[tokio::test]
    async fn test_notification_with_template() {
        let service = NotificationService::new();

        // Register template
        let template = Template::new("welcome", "Welcome")
            .with_subject("Welcome, {{name}}!")
            .with_body("Hello {{name}}!");
        service.register_template(template).await;

        // Register channel and recipient
        service
            .register_channel("inapp", Arc::new(InAppChannel::new()))
            .await;
        service
            .register_recipient(Recipient::new("user-123").prefers(ChannelType::InApp))
            .await;

        // Send with template
        let notification = Notification::new("welcome", "", "")
            .from_template("welcome")
            .with_variable("name", serde_json::json!("Alice"))
            .to("user-123")
            .via(ChannelType::InApp);

        let results = service.send(notification).await.unwrap();
        assert_eq!(results.len(), 1);
    }

    #[tokio::test]
    async fn test_delivery_history() {
        let service = NotificationService::new();

        service
            .register_channel("inapp", Arc::new(InAppChannel::new()))
            .await;
        service
            .register_recipient(Recipient::new("user-123").prefers(ChannelType::InApp))
            .await;

        let notification = Notification::new("test", "Test", "Body")
            .to("user-123")
            .via(ChannelType::InApp);

        let id = notification.id;
        service.send(notification).await.unwrap();

        let history = service.get_history(Some(id)).await;
        assert_eq!(history.len(), 1);
    }
}
