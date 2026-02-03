//! Proactive triggers.

use crate::{ProactiveConfig, ProactiveMessage, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Trigger type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriggerType {
    /// Time-based trigger.
    Schedule,
    /// Inactivity trigger.
    Inactivity,
    /// Event-based trigger.
    Event,
    /// Pattern-based trigger.
    Pattern,
    /// Context-based trigger.
    Context,
}

/// A proactive trigger.
#[derive(Clone)]
pub struct ProactiveTrigger {
    /// Trigger ID.
    pub id: Uuid,
    /// Trigger type.
    pub trigger_type: TriggerType,
    /// Trigger name.
    pub name: String,
    /// Target channel.
    pub channel_id: String,
    /// Target user (optional).
    pub user_id: Option<String>,
    /// Message template.
    pub message_template: String,
    /// Whether trigger is enabled.
    pub enabled: bool,
    /// Last fired time.
    last_fired: Arc<RwLock<Option<DateTime<Utc>>>>,
    /// Custom condition function.
    condition: Option<Arc<dyn Fn() -> bool + Send + Sync>>,
    /// Schedule expression (for schedule triggers).
    pub schedule: Option<String>,
    /// Inactivity threshold in seconds.
    pub inactivity_threshold_secs: Option<u64>,
}

impl std::fmt::Debug for ProactiveTrigger {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProactiveTrigger")
            .field("id", &self.id)
            .field("trigger_type", &self.trigger_type)
            .field("name", &self.name)
            .field("channel_id", &self.channel_id)
            .field("user_id", &self.user_id)
            .field("enabled", &self.enabled)
            .field("schedule", &self.schedule)
            .field("inactivity_threshold_secs", &self.inactivity_threshold_secs)
            .finish()
    }
}

impl ProactiveTrigger {
    /// Create a new trigger.
    pub fn new(trigger_type: TriggerType, name: &str, channel_id: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            trigger_type,
            name: name.to_string(),
            channel_id: channel_id.to_string(),
            user_id: None,
            message_template: String::new(),
            enabled: true,
            last_fired: Arc::new(RwLock::new(None)),
            condition: None,
            schedule: None,
            inactivity_threshold_secs: None,
        }
    }

    /// Create a daily reminder trigger.
    pub fn daily_reminder(
        name: &str,
        channel_id: &str,
        hour: u8,
        minute: u8,
        message: &str,
    ) -> Self {
        let mut trigger = Self::new(TriggerType::Schedule, name, channel_id);
        trigger.schedule = Some(format!("{} {} * * *", minute, hour));
        trigger.message_template = message.to_string();
        trigger
    }

    /// Create an inactivity check trigger.
    pub fn inactivity_check(name: &str, channel_id: &str, hours: u64, message: &str) -> Self {
        let mut trigger = Self::new(TriggerType::Inactivity, name, channel_id);
        trigger.inactivity_threshold_secs = Some(hours * 3600);
        trigger.message_template = message.to_string();
        trigger
    }

    /// Set target user.
    pub fn for_user(mut self, user_id: &str) -> Self {
        self.user_id = Some(user_id.to_string());
        self
    }

    /// Set message template.
    pub fn with_message(mut self, template: &str) -> Self {
        self.message_template = template.to_string();
        self
    }

    /// Set custom condition.
    pub fn with_condition<F>(mut self, condition: F) -> Self
    where
        F: Fn() -> bool + Send + Sync + 'static,
    {
        self.condition = Some(Arc::new(condition));
        self
    }

    /// Check if trigger should fire.
    pub async fn should_fire(&self, config: &ProactiveConfig) -> bool {
        if !self.enabled {
            return false;
        }

        // Check if trigger type is enabled
        let type_name = match self.trigger_type {
            TriggerType::Schedule => "schedule",
            TriggerType::Inactivity => "inactivity",
            TriggerType::Event => "event",
            TriggerType::Pattern => "pattern",
            TriggerType::Context => "context",
        };

        if !config
            .enabled_triggers
            .iter()
            .any(|t| t == type_name || t == "all")
        {
            return false;
        }

        // Check custom condition
        if let Some(condition) = &self.condition {
            if !condition() {
                return false;
            }
        }

        // Type-specific checks
        match self.trigger_type {
            TriggerType::Schedule => self.check_schedule().await,
            TriggerType::Inactivity => self.check_inactivity().await,
            _ => true,
        }
    }

    /// Check schedule trigger.
    async fn check_schedule(&self) -> bool {
        let _schedule = match &self.schedule {
            Some(s) => s,
            None => return false,
        };

        // Simple check: has enough time passed since last fire?
        // In production, use proper cron parsing
        let last_fired = self.last_fired.read().await;
        if let Some(last) = *last_fired {
            let elapsed = Utc::now().signed_duration_since(last);
            // Don't fire more than once per hour for schedules
            if elapsed.num_hours() < 1 {
                return false;
            }
        }

        true
    }

    /// Check inactivity trigger.
    async fn check_inactivity(&self) -> bool {
        // This would check against actual user activity tracking
        // For now, always return false (requires external state)
        false
    }

    /// Create message from trigger.
    pub async fn create_message(&self) -> Option<ProactiveMessage> {
        if self.message_template.is_empty() {
            return None;
        }

        // Mark as fired
        *self.last_fired.write().await = Some(Utc::now());

        let mut message = ProactiveMessage::new(
            &self.channel_id,
            &self.message_template,
            &format!("{:?}", self.trigger_type).to_lowercase(),
        );

        if let Some(user_id) = &self.user_id {
            message = message.for_user(user_id);
        }

        Some(message)
    }

    /// Enable trigger.
    pub fn enable(&mut self) {
        self.enabled = true;
    }

    /// Disable trigger.
    pub fn disable(&mut self) {
        self.enabled = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trigger_creation() {
        let trigger = ProactiveTrigger::new(TriggerType::Schedule, "Test", "channel1");
        assert!(trigger.enabled);
        assert_eq!(trigger.trigger_type, TriggerType::Schedule);
    }

    #[test]
    fn test_daily_reminder() {
        let trigger =
            ProactiveTrigger::daily_reminder("Morning", "channel1", 9, 0, "Good morning!");

        assert_eq!(trigger.schedule, Some("0 9 * * *".to_string()));
        assert_eq!(trigger.message_template, "Good morning!");
    }

    #[tokio::test]
    async fn test_create_message() {
        let trigger = ProactiveTrigger::new(TriggerType::Event, "Test", "channel1")
            .with_message("Hello!")
            .for_user("user1");

        let message = trigger.create_message().await.unwrap();
        assert_eq!(message.channel_id, "channel1");
        assert_eq!(message.user_id, Some("user1".to_string()));
        assert_eq!(message.content, "Hello!");
    }
}
