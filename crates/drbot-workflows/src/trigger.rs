//! Workflow triggers.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Trigger types.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriggerType {
    /// Time-based trigger (cron).
    Schedule,
    /// Webhook trigger.
    Webhook,
    /// Message trigger.
    Message,
    /// Event trigger.
    Event,
    /// Manual trigger.
    Manual,
}

/// A workflow trigger.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trigger {
    /// Trigger ID.
    pub id: Uuid,
    /// Trigger type.
    pub trigger_type: TriggerType,
    /// Trigger name.
    pub name: String,
    /// Trigger condition.
    pub condition: TriggerCondition,
    /// Whether trigger is enabled.
    pub enabled: bool,
    /// Last triggered time.
    pub last_triggered: Option<DateTime<Utc>>,
}

impl Trigger {
    /// Create a new trigger.
    pub fn new(trigger_type: TriggerType, name: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            trigger_type,
            name: name.to_string(),
            condition: TriggerCondition::Always,
            enabled: true,
            last_triggered: None,
        }
    }

    /// Create a schedule trigger.
    pub fn schedule(name: &str, cron: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            trigger_type: TriggerType::Schedule,
            name: name.to_string(),
            condition: TriggerCondition::Cron(cron.to_string()),
            enabled: true,
            last_triggered: None,
        }
    }

    /// Create a webhook trigger.
    pub fn webhook(name: &str) -> Self {
        Self::new(TriggerType::Webhook, name)
    }

    /// Create a message trigger.
    pub fn on_message(name: &str, pattern: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            trigger_type: TriggerType::Message,
            name: name.to_string(),
            condition: TriggerCondition::MessageMatch(pattern.to_string()),
            enabled: true,
            last_triggered: None,
        }
    }

    /// Create an event trigger.
    pub fn on_event(name: &str, event_type: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            trigger_type: TriggerType::Event,
            name: name.to_string(),
            condition: TriggerCondition::EventType(event_type.to_string()),
            enabled: true,
            last_triggered: None,
        }
    }

    /// Set condition.
    pub fn with_condition(mut self, condition: TriggerCondition) -> Self {
        self.condition = condition;
        self
    }

    /// Disable trigger.
    pub fn disable(&mut self) {
        self.enabled = false;
    }

    /// Enable trigger.
    pub fn enable(&mut self) {
        self.enabled = true;
    }

    /// Check if trigger matches.
    pub fn matches(&self, context: &TriggerContext) -> bool {
        if !self.enabled {
            return false;
        }

        self.condition.evaluate(context)
    }

    /// Mark as triggered.
    pub fn mark_triggered(&mut self) {
        self.last_triggered = Some(Utc::now());
    }
}

/// Trigger condition.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriggerCondition {
    /// Always trigger.
    Always,
    /// Never trigger (disabled).
    Never,
    /// Cron expression.
    Cron(String),
    /// Message content matches pattern.
    MessageMatch(String),
    /// Event type matches.
    EventType(String),
    /// Custom expression.
    Expression(String),
    /// All conditions must match.
    All(Vec<TriggerCondition>),
    /// Any condition must match.
    Any(Vec<TriggerCondition>),
}

impl TriggerCondition {
    /// Evaluate condition against context.
    pub fn evaluate(&self, context: &TriggerContext) -> bool {
        match self {
            TriggerCondition::Always => true,
            TriggerCondition::Never => false,
            TriggerCondition::Cron(_cron) => {
                // In production, use cron parser
                // For now, just check if we have a scheduled event
                context.event_type.as_deref() == Some("schedule")
            }
            TriggerCondition::MessageMatch(pattern) => {
                if let Some(message) = &context.message {
                    message.to_lowercase().contains(&pattern.to_lowercase())
                } else {
                    false
                }
            }
            TriggerCondition::EventType(event_type) => {
                context.event_type.as_deref() == Some(event_type)
            }
            TriggerCondition::Expression(_expr) => {
                // Would use expression evaluator
                true
            }
            TriggerCondition::All(conditions) => conditions.iter().all(|c| c.evaluate(context)),
            TriggerCondition::Any(conditions) => conditions.iter().any(|c| c.evaluate(context)),
        }
    }
}

/// Context for trigger evaluation.
#[derive(Debug, Clone, Default)]
pub struct TriggerContext {
    /// Event type.
    pub event_type: Option<String>,
    /// Message content.
    pub message: Option<String>,
    /// User ID.
    pub user_id: Option<String>,
    /// Channel ID.
    pub channel_id: Option<String>,
    /// Additional data.
    pub data: std::collections::HashMap<String, serde_json::Value>,
}

impl TriggerContext {
    /// Create a new context.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create from message.
    pub fn from_message(message: &str, user_id: &str, channel_id: &str) -> Self {
        Self {
            event_type: Some("message".to_string()),
            message: Some(message.to_string()),
            user_id: Some(user_id.to_string()),
            channel_id: Some(channel_id.to_string()),
            data: std::collections::HashMap::new(),
        }
    }

    /// Create from event.
    pub fn from_event(event_type: &str) -> Self {
        Self {
            event_type: Some(event_type.to_string()),
            ..Default::default()
        }
    }

    /// Set a data value.
    pub fn with_data(mut self, key: &str, value: serde_json::Value) -> Self {
        self.data.insert(key.to_string(), value);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trigger_creation() {
        let trigger = Trigger::schedule("Daily", "0 9 * * *");
        assert_eq!(trigger.trigger_type, TriggerType::Schedule);
        assert!(trigger.enabled);
    }

    #[test]
    fn test_message_trigger() {
        let trigger = Trigger::on_message("Keyword", "hello");
        let context = TriggerContext::from_message("Hello world!", "user1", "channel1");

        assert!(trigger.matches(&context));
    }

    #[test]
    fn test_condition_all() {
        let condition = TriggerCondition::All(vec![
            TriggerCondition::Always,
            TriggerCondition::EventType("message".to_string()),
        ]);

        let context = TriggerContext::from_event("message");
        assert!(condition.evaluate(&context));
    }
}
