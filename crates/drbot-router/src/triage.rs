//! AI-powered message triage and auto-drafting.
//!
//! Uses AI to categorize messages and generate response drafts.

use crate::inbox::{MessageStatus, UnifiedMessage};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Triage result for a message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriageResult {
    /// Message ID.
    pub message_id: Uuid,
    /// Category.
    pub category: MessageCategory,
    /// Urgency level.
    pub urgency: UrgencyLevel,
    /// Sentiment.
    pub sentiment: Sentiment,
    /// Intent.
    pub intent: MessageIntent,
    /// Suggested action.
    pub suggested_action: SuggestedAction,
    /// Auto-generated reply draft.
    pub draft_reply: Option<DraftReply>,
    /// Confidence score.
    pub confidence: f32,
    /// Triage timestamp.
    pub triaged_at: DateTime<Utc>,
}

/// Message categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageCategory {
    /// Work-related.
    Work,
    /// Personal.
    Personal,
    /// Promotional/marketing.
    Promotional,
    /// Automated notifications.
    Notification,
    /// Support request.
    Support,
    /// Social/casual.
    Social,
    /// Financial/transactional.
    Financial,
    /// Spam.
    Spam,
    /// Other/unknown.
    Other,
}

/// Urgency levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UrgencyLevel {
    Low = 1,
    Normal = 2,
    High = 3,
    Critical = 4,
}

/// Message sentiment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Sentiment {
    Positive,
    Neutral,
    Negative,
    Mixed,
}

/// Detected message intent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageIntent {
    /// Asking a question.
    Question,
    /// Making a request.
    Request,
    /// Sharing information.
    Informational,
    /// Scheduling/calendar related.
    Scheduling,
    /// Greeting/social.
    Social,
    /// Complaint.
    Complaint,
    /// Thank you/appreciation.
    Appreciation,
    /// Confirmation needed.
    Confirmation,
    /// Follow-up on previous.
    FollowUp,
    /// Action required.
    ActionRequired,
    /// Other.
    Other,
}

/// Suggested action for a message.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum SuggestedAction {
    /// Reply immediately.
    ReplyNow { reason: String },
    /// Reply later.
    ReplyLater { suggested_time: DateTime<Utc> },
    /// No reply needed.
    NoReplyNeeded { reason: String },
    /// Archive/dismiss.
    Archive,
    /// Forward to someone.
    Forward { to: String, reason: String },
    /// Add to calendar.
    AddToCalendar {
        title: String,
        datetime: DateTime<Utc>,
    },
    /// Create task.
    CreateTask {
        title: String,
        due: Option<DateTime<Utc>>,
    },
    /// Review manually.
    ManualReview { reason: String },
}

/// Auto-generated reply draft.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DraftReply {
    /// Draft ID.
    pub id: Uuid,
    /// Reply text.
    pub text: String,
    /// Tone used.
    pub tone: ReplyTone,
    /// Confidence in the draft.
    pub confidence: f32,
    /// Alternative drafts.
    pub alternatives: Vec<String>,
    /// Whether to send automatically.
    pub auto_send: bool,
    /// Send delay if auto-sending.
    pub send_delay_secs: Option<u64>,
}

/// Reply tone options.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReplyTone {
    Professional,
    Friendly,
    Casual,
    Formal,
    Apologetic,
    Assertive,
    Empathetic,
}

/// Triage configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriageConfig {
    /// Enable auto-triage.
    pub enabled: bool,
    /// Enable draft generation.
    pub generate_drafts: bool,
    /// Auto-archive spam.
    pub auto_archive_spam: bool,
    /// Auto-archive promotional.
    pub auto_archive_promotional: bool,
    /// Minimum confidence for auto-actions.
    pub min_confidence: f32,
    /// Default reply tone.
    pub default_tone: ReplyTone,
    /// VIP contacts always urgent.
    pub vip_always_urgent: bool,
    /// Enable learning from corrections.
    pub enable_learning: bool,
}

impl Default for TriageConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            generate_drafts: true,
            auto_archive_spam: true,
            auto_archive_promotional: false,
            min_confidence: 0.8,
            default_tone: ReplyTone::Professional,
            vip_always_urgent: true,
            enable_learning: true,
        }
    }
}

/// Triage provider trait.
#[async_trait]
pub trait TriageProvider: Send + Sync {
    /// Triage a message.
    async fn triage(&self, message: &UnifiedMessage) -> TriageResult;

    /// Generate a draft reply.
    async fn generate_draft(&self, message: &UnifiedMessage, tone: ReplyTone)
        -> Option<DraftReply>;

    /// Learn from user correction.
    async fn learn_correction(
        &self,
        message_id: Uuid,
        original: &TriageResult,
        corrected_category: Option<MessageCategory>,
        corrected_urgency: Option<UrgencyLevel>,
        corrected_reply: Option<String>,
    );
}

/// Rule-based triage provider (fallback).
pub struct RuleBasedTriageProvider {
    config: TriageConfig,
}

impl RuleBasedTriageProvider {
    /// Create a new rule-based provider.
    pub fn new(config: TriageConfig) -> Self {
        Self { config }
    }

    fn detect_category(&self, text: &str) -> MessageCategory {
        let lower = text.to_lowercase();

        if lower.contains("unsubscribe") || lower.contains("marketing") || lower.contains("sale") {
            return MessageCategory::Promotional;
        }
        if lower.contains("invoice") || lower.contains("payment") || lower.contains("order") {
            return MessageCategory::Financial;
        }
        if lower.contains("meeting") || lower.contains("schedule") || lower.contains("calendar") {
            return MessageCategory::Work;
        }
        if lower.contains("help") || lower.contains("support") || lower.contains("issue") {
            return MessageCategory::Support;
        }

        MessageCategory::Other
    }

    fn detect_urgency(&self, text: &str, sender_is_vip: bool) -> UrgencyLevel {
        if sender_is_vip && self.config.vip_always_urgent {
            return UrgencyLevel::High;
        }

        let lower = text.to_lowercase();

        if lower.contains("urgent") || lower.contains("asap") || lower.contains("emergency") {
            return UrgencyLevel::Critical;
        }
        if lower.contains("important") || lower.contains("priority") {
            return UrgencyLevel::High;
        }

        UrgencyLevel::Normal
    }

    fn detect_sentiment(&self, text: &str) -> Sentiment {
        let lower = text.to_lowercase();

        let positive_words = ["thank", "great", "awesome", "love", "happy", "appreciate"];
        let negative_words = [
            "angry",
            "upset",
            "disappointed",
            "frustrated",
            "problem",
            "issue",
        ];

        let positive_count = positive_words.iter().filter(|w| lower.contains(*w)).count();
        let negative_count = negative_words.iter().filter(|w| lower.contains(*w)).count();

        if positive_count > 0 && negative_count > 0 {
            Sentiment::Mixed
        } else if positive_count > negative_count {
            Sentiment::Positive
        } else if negative_count > positive_count {
            Sentiment::Negative
        } else {
            Sentiment::Neutral
        }
    }

    fn detect_intent(&self, text: &str) -> MessageIntent {
        let lower = text.to_lowercase();

        if lower.contains('?') {
            return MessageIntent::Question;
        }
        if lower.contains("please") || lower.contains("can you") || lower.contains("could you") {
            return MessageIntent::Request;
        }
        if lower.contains("schedule") || lower.contains("meeting") || lower.contains("call") {
            return MessageIntent::Scheduling;
        }
        if lower.contains("thank") {
            return MessageIntent::Appreciation;
        }
        if lower.contains("hi") || lower.contains("hello") || lower.contains("hey") {
            return MessageIntent::Social;
        }

        MessageIntent::Informational
    }
}

#[async_trait]
impl TriageProvider for RuleBasedTriageProvider {
    async fn triage(&self, message: &UnifiedMessage) -> TriageResult {
        let text = &message.content.text;

        let category = self.detect_category(text);
        let urgency = self.detect_urgency(text, message.sender.is_vip);
        let sentiment = self.detect_sentiment(text);
        let intent = self.detect_intent(text);

        let suggested_action = match category {
            MessageCategory::Spam => SuggestedAction::Archive,
            MessageCategory::Promotional if self.config.auto_archive_promotional => {
                SuggestedAction::Archive
            }
            _ => match urgency {
                UrgencyLevel::Critical => SuggestedAction::ReplyNow {
                    reason: "Urgent message".to_string(),
                },
                UrgencyLevel::High => SuggestedAction::ReplyNow {
                    reason: "High priority".to_string(),
                },
                _ => SuggestedAction::ReplyLater {
                    suggested_time: Utc::now() + chrono::Duration::hours(4),
                },
            },
        };

        let draft_reply = if self.config.generate_drafts
            && !matches!(suggested_action, SuggestedAction::Archive)
        {
            self.generate_draft(message, self.config.default_tone).await
        } else {
            None
        };

        TriageResult {
            message_id: message.id,
            category,
            urgency,
            sentiment,
            intent,
            suggested_action,
            draft_reply,
            confidence: 0.7, // Rule-based is less confident
            triaged_at: Utc::now(),
        }
    }

    async fn generate_draft(
        &self,
        message: &UnifiedMessage,
        tone: ReplyTone,
    ) -> Option<DraftReply> {
        let greeting = match tone {
            ReplyTone::Professional | ReplyTone::Formal => {
                format!("Hi {},", message.sender.name)
            }
            ReplyTone::Friendly | ReplyTone::Casual => {
                format!("Hey {}!", message.sender.name)
            }
            _ => format!("Hi {},", message.sender.name),
        };

        let body = match self.detect_intent(&message.content.text) {
            MessageIntent::Question => {
                "Thanks for reaching out. I'll get back to you shortly with an answer."
            }
            MessageIntent::Request => {
                "Thanks for your request. I'm working on it and will update you soon."
            }
            MessageIntent::Appreciation => "You're welcome! Happy to help.",
            MessageIntent::Social => "Great to hear from you!",
            _ => "Thanks for your message. I'll review and get back to you.",
        };

        Some(DraftReply {
            id: Uuid::new_v4(),
            text: format!("{}\n\n{}", greeting, body),
            tone,
            confidence: 0.6,
            alternatives: Vec::new(),
            auto_send: false,
            send_delay_secs: None,
        })
    }

    async fn learn_correction(
        &self,
        _message_id: Uuid,
        _original: &TriageResult,
        _corrected_category: Option<MessageCategory>,
        _corrected_urgency: Option<UrgencyLevel>,
        _corrected_reply: Option<String>,
    ) {
        // Rule-based provider doesn't learn
    }
}

/// Triage manager.
pub struct TriageManager {
    provider: Box<dyn TriageProvider>,
    config: TriageConfig,
}

impl TriageManager {
    /// Create a new triage manager.
    pub fn new(provider: Box<dyn TriageProvider>, config: TriageConfig) -> Self {
        Self { provider, config }
    }

    /// Triage a message.
    pub async fn triage(&self, message: &UnifiedMessage) -> TriageResult {
        self.provider.triage(message).await
    }

    /// Process auto-actions for a triage result.
    pub async fn process_auto_actions(
        &self,
        message: &mut UnifiedMessage,
        result: &TriageResult,
    ) -> Vec<AutoAction> {
        let mut actions = Vec::new();

        if result.confidence < self.config.min_confidence {
            return actions;
        }

        match &result.suggested_action {
            SuggestedAction::Archive
                if self.config.auto_archive_spam && result.category == MessageCategory::Spam =>
            {
                message.status = MessageStatus::Archived;
                actions.push(AutoAction::Archived {
                    message_id: message.id,
                });
            }
            _ => {}
        }

        if let Some(draft) = &result.draft_reply {
            if draft.auto_send && draft.confidence >= self.config.min_confidence {
                actions.push(AutoAction::DraftCreated {
                    message_id: message.id,
                    draft_id: draft.id,
                });
            }
        }

        actions
    }
}

/// Auto-action taken by the system.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum AutoAction {
    Archived { message_id: Uuid },
    DraftCreated { message_id: Uuid, draft_id: Uuid },
    LabelAdded { message_id: Uuid, label: String },
    PriorityChanged { message_id: Uuid, new_priority: i32 },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inbox::{ChannelType, MessageContent, Sender};

    #[tokio::test]
    async fn test_rule_based_triage() {
        let provider = RuleBasedTriageProvider::new(TriageConfig::default());

        let message = UnifiedMessage {
            id: Uuid::new_v4(),
            channel_type: ChannelType::Slack,
            channel_id: "general".to_string(),
            sender: Sender {
                id: "user1".to_string(),
                name: "Alice".to_string(),
                avatar_url: None,
                is_contact: true,
                is_vip: false,
            },
            content: MessageContent {
                text: "Can you help me with this urgent issue?".to_string(),
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

        let result = provider.triage(&message).await;

        assert_eq!(result.urgency, UrgencyLevel::Critical);
        // Message ends with '?' so it's detected as a question first
        assert_eq!(result.intent, MessageIntent::Question);
        assert!(result.draft_reply.is_some());
    }

    #[test]
    fn test_detect_sentiment() {
        let provider = RuleBasedTriageProvider::new(TriageConfig::default());

        assert_eq!(
            provider.detect_sentiment("Thank you so much!"),
            Sentiment::Positive
        );
        assert_eq!(
            provider.detect_sentiment("I'm frustrated with this issue"),
            Sentiment::Negative
        );
        assert_eq!(
            provider.detect_sentiment("The meeting is at 3pm"),
            Sentiment::Neutral
        );
    }
}
