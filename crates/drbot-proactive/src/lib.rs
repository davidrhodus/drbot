//! Proactive assistant engine for drbot.
//!
//! Enables the assistant to proactively reach out based on
//! context, time, and learned patterns.

mod automation;
mod briefing;
mod context_triggers;
mod engine;
mod pattern;
mod scheduler;
mod trigger;

pub use automation::{
    AutomationConfig, AutomationSuggestion, AutomationType, DetectedPattern, PatternData,
    PatternDetector, PatternDetectorConfig, PatternType, SuggestionStatus,
};
pub use briefing::{
    Briefing, BriefingConfig, BriefingDataSource, BriefingGenerator, BriefingItem, BriefingSection,
    BriefingType, SectionType,
};
pub use context_triggers::{
    ContextState, ContextTrigger, ContextTriggerManager, ContextTriggerType, TriggerAction,
    TriggerCondition, TriggerEvent,
};
pub use engine::{EngineConfig, ProactiveEngine};
pub use pattern::{Pattern, PatternMatcher};
pub use scheduler::{ScheduledAction, Scheduler};
pub use trigger::{ProactiveTrigger, TriggerType};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Proactive result.
pub type Result<T> = std::result::Result<T, ProactiveError>;

/// Proactive errors.
#[derive(Debug, thiserror::Error)]
pub enum ProactiveError {
    #[error("Trigger failed: {0}")]
    TriggerFailed(String),
    #[error("Pattern matching error: {0}")]
    PatternError(String),
    #[error("Scheduling error: {0}")]
    SchedulingError(String),
}

/// Proactive configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProactiveConfig {
    /// Whether proactive mode is enabled.
    pub enabled: bool,
    /// Minimum interval between proactive messages (seconds).
    pub min_interval_secs: u64,
    /// Maximum proactive messages per day.
    pub max_daily_messages: u32,
    /// Quiet hours start (24h format).
    pub quiet_hours_start: Option<u8>,
    /// Quiet hours end (24h format).
    pub quiet_hours_end: Option<u8>,
    /// Enabled trigger types.
    pub enabled_triggers: Vec<String>,
}

impl Default for ProactiveConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            min_interval_secs: 3600, // 1 hour
            max_daily_messages: 5,
            quiet_hours_start: Some(22),
            quiet_hours_end: Some(8),
            enabled_triggers: vec![
                "reminder".to_string(),
                "follow_up".to_string(),
                "daily_summary".to_string(),
            ],
        }
    }
}

/// A proactive message to be sent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProactiveMessage {
    /// Message ID.
    pub id: Uuid,
    /// Target channel.
    pub channel_id: String,
    /// Target user (optional).
    pub user_id: Option<String>,
    /// Message content.
    pub content: String,
    /// When to send.
    pub scheduled_for: DateTime<Utc>,
    /// Trigger that created this message.
    pub trigger_type: String,
    /// Priority (1-10).
    pub priority: u8,
    /// Whether this message has been sent.
    pub sent: bool,
}

impl ProactiveMessage {
    /// Create a new proactive message.
    pub fn new(channel_id: &str, content: &str, trigger_type: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            channel_id: channel_id.to_string(),
            user_id: None,
            content: content.to_string(),
            scheduled_for: Utc::now(),
            trigger_type: trigger_type.to_string(),
            priority: 5,
            sent: false,
        }
    }

    /// Set the target user.
    pub fn for_user(mut self, user_id: &str) -> Self {
        self.user_id = Some(user_id.to_string());
        self
    }

    /// Set the scheduled time.
    pub fn at(mut self, time: DateTime<Utc>) -> Self {
        self.scheduled_for = time;
        self
    }

    /// Set the priority.
    pub fn with_priority(mut self, priority: u8) -> Self {
        self.priority = priority.min(10);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_proactive_config_default() {
        let config = ProactiveConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.max_daily_messages, 5);
    }

    #[test]
    fn test_proactive_message() {
        let msg = ProactiveMessage::new("channel1", "Hello!", "reminder")
            .for_user("user1")
            .with_priority(8);

        assert_eq!(msg.channel_id, "channel1");
        assert_eq!(msg.user_id, Some("user1".to_string()));
        assert_eq!(msg.priority, 8);
    }
}
