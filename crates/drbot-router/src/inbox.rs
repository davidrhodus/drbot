//! Unified inbox for cross-platform message aggregation.
//!
//! Aggregates messages from all channels into a single prioritized view.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Unified message from any channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedMessage {
    /// Unique message ID.
    pub id: Uuid,
    /// Source channel type.
    pub channel_type: ChannelType,
    /// Channel identifier.
    pub channel_id: String,
    /// Sender information.
    pub sender: Sender,
    /// Message content.
    pub content: MessageContent,
    /// Priority score (higher = more important).
    pub priority: i32,
    /// AI-generated summary (if long).
    pub summary: Option<String>,
    /// Suggested response.
    pub suggested_reply: Option<String>,
    /// Message status.
    pub status: MessageStatus,
    /// Labels/tags.
    pub labels: Vec<String>,
    /// Original timestamp.
    pub timestamp: DateTime<Utc>,
    /// When received in inbox.
    pub received_at: DateTime<Utc>,
}

/// Channel types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelType {
    WhatsApp,
    Telegram,
    Slack,
    Discord,
    Signal,
    IMessage,
    Matrix,
    Email,
    WebChat,
    Custom,
}

impl std::fmt::Display for ChannelType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChannelType::WhatsApp => write!(f, "WhatsApp"),
            ChannelType::Telegram => write!(f, "Telegram"),
            ChannelType::Slack => write!(f, "Slack"),
            ChannelType::Discord => write!(f, "Discord"),
            ChannelType::Signal => write!(f, "Signal"),
            ChannelType::IMessage => write!(f, "iMessage"),
            ChannelType::Matrix => write!(f, "Matrix"),
            ChannelType::Email => write!(f, "Email"),
            ChannelType::WebChat => write!(f, "WebChat"),
            ChannelType::Custom => write!(f, "Custom"),
        }
    }
}

/// Sender information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sender {
    /// Sender ID in the source platform.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Avatar URL.
    pub avatar_url: Option<String>,
    /// Whether this is a known contact.
    pub is_contact: bool,
    /// VIP flag.
    pub is_vip: bool,
}

/// Message content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageContent {
    /// Text content.
    pub text: String,
    /// Attachments.
    pub attachments: Vec<Attachment>,
    /// Whether this is a reply to another message.
    pub is_reply: bool,
    /// Original message ID if reply.
    pub reply_to: Option<Uuid>,
    /// Mentions.
    pub mentions: Vec<String>,
}

/// Attachment information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attachment {
    /// Attachment type.
    pub attachment_type: AttachmentType,
    /// File name.
    pub name: String,
    /// File size in bytes.
    pub size: u64,
    /// MIME type.
    pub mime_type: String,
    /// URL to the attachment.
    pub url: Option<String>,
}

/// Attachment types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttachmentType {
    Image,
    Video,
    Audio,
    Document,
    File,
}

/// Message status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageStatus {
    /// New, unread message.
    Unread,
    /// Read but not actioned.
    Read,
    /// Replied to.
    Replied,
    /// Archived.
    Archived,
    /// Snoozed until later.
    Snoozed,
    /// Waiting for response.
    WaitingResponse,
}

/// Inbox filter options.
#[derive(Debug, Clone, Default)]
pub struct InboxFilter {
    /// Filter by channel types.
    pub channels: Option<Vec<ChannelType>>,
    /// Filter by status.
    pub status: Option<Vec<MessageStatus>>,
    /// Filter by sender.
    pub sender_id: Option<String>,
    /// Filter by labels.
    pub labels: Option<Vec<String>>,
    /// Filter by priority (minimum).
    pub min_priority: Option<i32>,
    /// Only VIP senders.
    pub vip_only: bool,
    /// Search text.
    pub search: Option<String>,
    /// Date range start.
    pub from_date: Option<DateTime<Utc>>,
    /// Date range end.
    pub to_date: Option<DateTime<Utc>>,
}

impl InboxFilter {
    /// Create a new filter.
    pub fn new() -> Self {
        Self::default()
    }

    /// Filter by channel.
    pub fn channel(mut self, channel: ChannelType) -> Self {
        self.channels.get_or_insert_with(Vec::new).push(channel);
        self
    }

    /// Filter by status.
    pub fn status(mut self, status: MessageStatus) -> Self {
        self.status.get_or_insert_with(Vec::new).push(status);
        self
    }

    /// Filter unread only.
    pub fn unread_only(self) -> Self {
        self.status(MessageStatus::Unread)
    }

    /// Filter VIP only.
    pub fn vip_only(mut self) -> Self {
        self.vip_only = true;
        self
    }

    /// Filter by search text.
    pub fn search(mut self, query: &str) -> Self {
        self.search = Some(query.to_string());
        self
    }
}

/// Priority calculation configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriorityConfig {
    /// Base priority for VIP contacts.
    pub vip_bonus: i32,
    /// Base priority for known contacts.
    pub contact_bonus: i32,
    /// Priority boost for mentions.
    pub mention_bonus: i32,
    /// Priority boost for direct messages.
    pub dm_bonus: i32,
    /// Priority decay per hour.
    pub time_decay_per_hour: f32,
    /// Channel-specific priority multipliers.
    pub channel_multipliers: HashMap<ChannelType, f32>,
    /// Keywords that boost priority.
    pub urgent_keywords: Vec<String>,
    /// Keyword priority boost.
    pub urgent_keyword_bonus: i32,
}

impl Default for PriorityConfig {
    fn default() -> Self {
        let mut channel_multipliers = HashMap::new();
        channel_multipliers.insert(ChannelType::WhatsApp, 1.2);
        channel_multipliers.insert(ChannelType::Signal, 1.3);
        channel_multipliers.insert(ChannelType::IMessage, 1.2);
        channel_multipliers.insert(ChannelType::Slack, 1.1);
        channel_multipliers.insert(ChannelType::Discord, 0.9);
        channel_multipliers.insert(ChannelType::Email, 0.8);

        Self {
            vip_bonus: 50,
            contact_bonus: 20,
            mention_bonus: 30,
            dm_bonus: 25,
            time_decay_per_hour: 0.5,
            channel_multipliers,
            urgent_keywords: vec![
                "urgent".to_string(),
                "asap".to_string(),
                "emergency".to_string(),
                "important".to_string(),
                "help".to_string(),
            ],
            urgent_keyword_bonus: 40,
        }
    }
}

/// Unified inbox manager.
pub struct UnifiedInbox {
    messages: Arc<RwLock<Vec<UnifiedMessage>>>,
    priority_config: PriorityConfig,
    vip_contacts: Arc<RwLock<Vec<String>>>,
    known_contacts: Arc<RwLock<Vec<String>>>,
}

impl UnifiedInbox {
    /// Create a new unified inbox.
    pub fn new(priority_config: PriorityConfig) -> Self {
        Self {
            messages: Arc::new(RwLock::new(Vec::new())),
            priority_config,
            vip_contacts: Arc::new(RwLock::new(Vec::new())),
            known_contacts: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Add a VIP contact.
    pub async fn add_vip(&self, contact_id: &str) {
        self.vip_contacts.write().await.push(contact_id.to_string());
    }

    /// Add a known contact.
    pub async fn add_contact(&self, contact_id: &str) {
        self.known_contacts
            .write()
            .await
            .push(contact_id.to_string());
    }

    /// Ingest a message from a channel.
    pub async fn ingest(&self, message: UnifiedMessage) {
        let mut messages = self.messages.write().await;

        // Calculate priority
        let priority = self.calculate_priority(&message).await;

        let mut msg = message;
        msg.priority = priority;

        messages.push(msg);

        // Sort by priority (descending) and time
        messages.sort_by(|a, b| {
            b.priority
                .cmp(&a.priority)
                .then_with(|| b.timestamp.cmp(&a.timestamp))
        });
    }

    async fn calculate_priority(&self, message: &UnifiedMessage) -> i32 {
        let mut priority: f32 = 0.0;

        // VIP bonus
        let vip_contacts = self.vip_contacts.read().await;
        if vip_contacts.contains(&message.sender.id) || message.sender.is_vip {
            priority += self.priority_config.vip_bonus as f32;
        }

        // Known contact bonus
        let known_contacts = self.known_contacts.read().await;
        if known_contacts.contains(&message.sender.id) || message.sender.is_contact {
            priority += self.priority_config.contact_bonus as f32;
        }

        // Mention bonus
        if !message.content.mentions.is_empty() {
            priority += self.priority_config.mention_bonus as f32;
        }

        // Channel multiplier
        if let Some(multiplier) = self
            .priority_config
            .channel_multipliers
            .get(&message.channel_type)
        {
            priority *= multiplier;
        }

        // Urgent keywords
        let text_lower = message.content.text.to_lowercase();
        for keyword in &self.priority_config.urgent_keywords {
            if text_lower.contains(&keyword.to_lowercase()) {
                priority += self.priority_config.urgent_keyword_bonus as f32;
                break;
            }
        }

        // Time decay
        let hours_old = Utc::now()
            .signed_duration_since(message.timestamp)
            .num_hours() as f32;
        priority -= hours_old * self.priority_config.time_decay_per_hour;

        priority.max(0.0) as i32
    }

    /// Get messages with filter.
    pub async fn get_messages(&self, filter: InboxFilter, limit: usize) -> Vec<UnifiedMessage> {
        let messages = self.messages.read().await;

        messages
            .iter()
            .filter(|m| self.matches_filter(m, &filter))
            .take(limit)
            .cloned()
            .collect()
    }

    fn matches_filter(&self, message: &UnifiedMessage, filter: &InboxFilter) -> bool {
        // Channel filter
        if let Some(channels) = &filter.channels {
            if !channels.contains(&message.channel_type) {
                return false;
            }
        }

        // Status filter
        if let Some(statuses) = &filter.status {
            if !statuses.contains(&message.status) {
                return false;
            }
        }

        // Sender filter
        if let Some(sender_id) = &filter.sender_id {
            if message.sender.id != *sender_id {
                return false;
            }
        }

        // Label filter
        if let Some(labels) = &filter.labels {
            if !labels.iter().any(|l| message.labels.contains(l)) {
                return false;
            }
        }

        // Priority filter
        if let Some(min_priority) = filter.min_priority {
            if message.priority < min_priority {
                return false;
            }
        }

        // VIP filter
        if filter.vip_only && !message.sender.is_vip {
            return false;
        }

        // Search filter
        if let Some(search) = &filter.search {
            let search_lower = search.to_lowercase();
            let text_lower = message.content.text.to_lowercase();
            if !text_lower.contains(&search_lower)
                && !message.sender.name.to_lowercase().contains(&search_lower)
            {
                return false;
            }
        }

        // Date range filter
        if let Some(from_date) = filter.from_date {
            if message.timestamp < from_date {
                return false;
            }
        }
        if let Some(to_date) = filter.to_date {
            if message.timestamp > to_date {
                return false;
            }
        }

        true
    }

    /// Update message status.
    pub async fn update_status(&self, message_id: Uuid, status: MessageStatus) -> bool {
        let mut messages = self.messages.write().await;
        if let Some(msg) = messages.iter_mut().find(|m| m.id == message_id) {
            msg.status = status;
            true
        } else {
            false
        }
    }

    /// Add a label to a message.
    pub async fn add_label(&self, message_id: Uuid, label: &str) -> bool {
        let mut messages = self.messages.write().await;
        if let Some(msg) = messages.iter_mut().find(|m| m.id == message_id) {
            if !msg.labels.contains(&label.to_string()) {
                msg.labels.push(label.to_string());
            }
            true
        } else {
            false
        }
    }

    /// Get inbox statistics.
    pub async fn stats(&self) -> InboxStats {
        let messages = self.messages.read().await;

        let unread_count = messages
            .iter()
            .filter(|m| m.status == MessageStatus::Unread)
            .count();
        let urgent_count = messages.iter().filter(|m| m.priority >= 80).count();
        let vip_count = messages.iter().filter(|m| m.sender.is_vip).count();

        let mut by_channel: HashMap<ChannelType, usize> = HashMap::new();
        for msg in messages.iter() {
            *by_channel.entry(msg.channel_type).or_insert(0) += 1;
        }

        InboxStats {
            total_messages: messages.len(),
            unread_count,
            urgent_count,
            vip_count,
            messages_by_channel: by_channel,
        }
    }

    /// Get urgent messages summary.
    pub async fn urgent_summary(&self) -> String {
        let messages = self.messages.read().await;

        let urgent: Vec<_> = messages
            .iter()
            .filter(|m| m.status == MessageStatus::Unread && m.priority >= 50)
            .take(5)
            .collect();

        if urgent.is_empty() {
            return "No urgent messages.".to_string();
        }

        let mut summary = format!("{} urgent messages:\n", urgent.len());
        for msg in urgent {
            summary.push_str(&format!(
                "• {} ({}): {}\n",
                msg.sender.name,
                msg.channel_type,
                if msg.content.text.len() > 50 {
                    format!("{}...", &msg.content.text[..50])
                } else {
                    msg.content.text.clone()
                }
            ));
        }

        summary
    }
}

/// Inbox statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InboxStats {
    /// Total message count.
    pub total_messages: usize,
    /// Unread message count.
    pub unread_count: usize,
    /// Urgent message count.
    pub urgent_count: usize,
    /// VIP message count.
    pub vip_count: usize,
    /// Messages by channel.
    pub messages_by_channel: HashMap<ChannelType, usize>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_unified_inbox() {
        let inbox = UnifiedInbox::new(PriorityConfig::default());

        let message = UnifiedMessage {
            id: Uuid::new_v4(),
            channel_type: ChannelType::WhatsApp,
            channel_id: "chat1".to_string(),
            sender: Sender {
                id: "user1".to_string(),
                name: "John".to_string(),
                avatar_url: None,
                is_contact: true,
                is_vip: false,
            },
            content: MessageContent {
                text: "Hello!".to_string(),
                attachments: Vec::new(),
                is_reply: false,
                reply_to: None,
                mentions: Vec::new(),
            },
            priority: 0,
            summary: None,
            suggested_reply: None,
            status: MessageStatus::Unread,
            labels: Vec::new(),
            timestamp: Utc::now(),
            received_at: Utc::now(),
        };

        inbox.ingest(message).await;

        let messages = inbox.get_messages(InboxFilter::new(), 10).await;
        assert_eq!(messages.len(), 1);
    }

    #[tokio::test]
    async fn test_priority_calculation() {
        let inbox = UnifiedInbox::new(PriorityConfig::default());
        inbox.add_vip("vip_user").await;

        let vip_message = UnifiedMessage {
            id: Uuid::new_v4(),
            channel_type: ChannelType::Signal,
            channel_id: "chat1".to_string(),
            sender: Sender {
                id: "vip_user".to_string(),
                name: "VIP".to_string(),
                avatar_url: None,
                is_contact: true,
                is_vip: true,
            },
            content: MessageContent {
                text: "Urgent help needed!".to_string(),
                attachments: Vec::new(),
                is_reply: false,
                reply_to: None,
                mentions: Vec::new(),
            },
            priority: 0,
            summary: None,
            suggested_reply: None,
            status: MessageStatus::Unread,
            labels: Vec::new(),
            timestamp: Utc::now(),
            received_at: Utc::now(),
        };

        inbox.ingest(vip_message).await;

        let messages = inbox.get_messages(InboxFilter::new(), 10).await;
        assert!(messages[0].priority > 100); // VIP + contact + urgent keyword bonuses
    }
}
