//! Context-aware triggers for proactive actions.
//!
//! Monitors various context signals to trigger proactive AI interactions.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::sync::broadcast;
use uuid::Uuid;

/// Context trigger types.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextTriggerType {
    /// Calendar event approaching.
    CalendarEvent,
    /// Email received.
    EmailReceived,
    /// Screen activity (specific app/content).
    ScreenActivity,
    /// Location change.
    LocationChange,
    /// Time-based (recurring).
    TimeBased,
    /// Idle detection.
    IdleDetected,
    /// Focus mode started/ended.
    FocusMode,
    /// Pattern detected in user behavior.
    PatternDetected,
    /// External webhook.
    Webhook,
    /// Custom trigger.
    Custom(String),
}

/// A context trigger configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextTrigger {
    /// Unique trigger ID.
    pub id: Uuid,
    /// Trigger name.
    pub name: String,
    /// Trigger type.
    pub trigger_type: ContextTriggerType,
    /// Trigger conditions.
    pub conditions: Vec<TriggerCondition>,
    /// Action to take.
    pub action: TriggerAction,
    /// Cooldown period in seconds.
    pub cooldown_secs: u64,
    /// Whether enabled.
    pub enabled: bool,
    /// Last triggered time.
    pub last_triggered: Option<DateTime<Utc>>,
    /// Trigger count.
    pub trigger_count: u64,
}

impl ContextTrigger {
    /// Create a new context trigger.
    pub fn new(name: &str, trigger_type: ContextTriggerType) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.to_string(),
            trigger_type,
            conditions: Vec::new(),
            action: TriggerAction::SendMessage {
                template: String::new(),
            },
            cooldown_secs: 300,
            enabled: true,
            last_triggered: None,
            trigger_count: 0,
        }
    }

    /// Add a condition.
    pub fn with_condition(mut self, condition: TriggerCondition) -> Self {
        self.conditions.push(condition);
        self
    }

    /// Set the action.
    pub fn with_action(mut self, action: TriggerAction) -> Self {
        self.action = action;
        self
    }

    /// Set cooldown.
    pub fn with_cooldown(mut self, secs: u64) -> Self {
        self.cooldown_secs = secs;
        self
    }

    /// Check if the trigger is ready to fire (not in cooldown).
    pub fn is_ready(&self) -> bool {
        if !self.enabled {
            return false;
        }

        match self.last_triggered {
            None => true,
            Some(last) => {
                let elapsed = Utc::now().signed_duration_since(last);
                elapsed.num_seconds() as u64 >= self.cooldown_secs
            }
        }
    }

    /// Check if all conditions are met.
    pub fn check_conditions(&self, context: &ContextState) -> bool {
        self.conditions.iter().all(|c| c.evaluate(context))
    }
}

/// Condition for a trigger.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TriggerCondition {
    /// Time of day condition.
    TimeOfDay { start_hour: u8, end_hour: u8 },
    /// Day of week condition.
    DayOfWeek { days: Vec<String> },
    /// App in focus.
    AppInFocus { app_names: Vec<String> },
    /// Screen contains text.
    ScreenContains { keywords: Vec<String> },
    /// Calendar event within timeframe.
    CalendarEventSoon { minutes_before: u32 },
    /// User idle for duration.
    UserIdle { min_idle_secs: u64 },
    /// Custom condition with expression.
    Custom { expression: String },
    /// All sub-conditions must be true.
    All { conditions: Vec<TriggerCondition> },
    /// Any sub-condition must be true.
    Any { conditions: Vec<TriggerCondition> },
}

impl TriggerCondition {
    /// Evaluate the condition against current context.
    pub fn evaluate(&self, context: &ContextState) -> bool {
        match self {
            TriggerCondition::TimeOfDay {
                start_hour,
                end_hour,
            } => {
                let hour = context.current_hour;
                if start_hour <= end_hour {
                    hour >= *start_hour && hour < *end_hour
                } else {
                    // Wraps around midnight
                    hour >= *start_hour || hour < *end_hour
                }
            }
            TriggerCondition::DayOfWeek { days } => days
                .iter()
                .any(|d| d.eq_ignore_ascii_case(&context.day_of_week)),
            TriggerCondition::AppInFocus { app_names } => {
                if let Some(app) = &context.focused_app {
                    app_names
                        .iter()
                        .any(|n| app.to_lowercase().contains(&n.to_lowercase()))
                } else {
                    false
                }
            }
            TriggerCondition::ScreenContains { keywords } => {
                if let Some(text) = &context.screen_text {
                    let lower = text.to_lowercase();
                    keywords.iter().any(|k| lower.contains(&k.to_lowercase()))
                } else {
                    false
                }
            }
            TriggerCondition::CalendarEventSoon { minutes_before } => context
                .next_event_minutes
                .map_or(false, |m| m <= *minutes_before),
            TriggerCondition::UserIdle { min_idle_secs } => context.idle_seconds >= *min_idle_secs,
            TriggerCondition::Custom { .. } => {
                // Custom conditions would need an expression evaluator
                true
            }
            TriggerCondition::All { conditions } => conditions.iter().all(|c| c.evaluate(context)),
            TriggerCondition::Any { conditions } => conditions.iter().any(|c| c.evaluate(context)),
        }
    }
}

/// Action to take when trigger fires.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum TriggerAction {
    /// Send a message.
    SendMessage { template: String },
    /// Generate a briefing.
    GenerateBriefing { briefing_type: String },
    /// Execute a workflow.
    ExecuteWorkflow { workflow_id: Uuid },
    /// Call a webhook.
    CallWebhook { url: String, method: String },
    /// Run an agent.
    RunAgent { agent_id: String, prompt: String },
    /// Multiple actions.
    Multiple { actions: Vec<TriggerAction> },
}

/// Current context state for evaluating triggers.
#[derive(Debug, Clone, Default)]
pub struct ContextState {
    /// Current hour (0-23).
    pub current_hour: u8,
    /// Current day of week.
    pub day_of_week: String,
    /// Currently focused app.
    pub focused_app: Option<String>,
    /// Screen text content.
    pub screen_text: Option<String>,
    /// Minutes until next calendar event.
    pub next_event_minutes: Option<u32>,
    /// Seconds user has been idle.
    pub idle_seconds: u64,
    /// Current location.
    pub location: Option<String>,
    /// Custom context values.
    pub custom: HashMap<String, String>,
}

impl ContextState {
    /// Create a new context state.
    pub fn new() -> Self {
        let now = Utc::now();
        Self {
            current_hour: now.format("%H").to_string().parse().unwrap_or(0),
            day_of_week: now.format("%A").to_string(),
            ..Default::default()
        }
    }

    /// Set focused app.
    pub fn with_app(mut self, app: &str) -> Self {
        self.focused_app = Some(app.to_string());
        self
    }

    /// Set screen text.
    pub fn with_screen_text(mut self, text: &str) -> Self {
        self.screen_text = Some(text.to_string());
        self
    }

    /// Set next event time.
    pub fn with_next_event(mut self, minutes: u32) -> Self {
        self.next_event_minutes = Some(minutes);
        self
    }

    /// Set idle time.
    pub fn with_idle(mut self, seconds: u64) -> Self {
        self.idle_seconds = seconds;
        self
    }
}

/// Event emitted when a trigger fires.
#[derive(Debug, Clone)]
pub struct TriggerEvent {
    /// Trigger ID.
    pub trigger_id: Uuid,
    /// Trigger name.
    pub trigger_name: String,
    /// Action to execute.
    pub action: TriggerAction,
    /// Context that caused the trigger.
    pub context: ContextState,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
}

/// Context trigger manager.
pub struct ContextTriggerManager {
    triggers: Vec<ContextTrigger>,
    event_sender: broadcast::Sender<TriggerEvent>,
}

impl ContextTriggerManager {
    /// Create a new manager.
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(100);
        Self {
            triggers: Vec::new(),
            event_sender: sender,
        }
    }

    /// Add a trigger.
    pub fn add_trigger(&mut self, trigger: ContextTrigger) {
        self.triggers.push(trigger);
    }

    /// Remove a trigger.
    pub fn remove_trigger(&mut self, id: Uuid) -> bool {
        if let Some(pos) = self.triggers.iter().position(|t| t.id == id) {
            self.triggers.remove(pos);
            true
        } else {
            false
        }
    }

    /// Get all triggers.
    pub fn triggers(&self) -> &[ContextTrigger] {
        &self.triggers
    }

    /// Subscribe to trigger events.
    pub fn subscribe(&self) -> broadcast::Receiver<TriggerEvent> {
        self.event_sender.subscribe()
    }

    /// Evaluate all triggers against current context.
    pub fn evaluate(&mut self, context: &ContextState) -> Vec<TriggerEvent> {
        let mut events = Vec::new();
        let now = Utc::now();

        for trigger in &mut self.triggers {
            if trigger.is_ready() && trigger.check_conditions(context) {
                let event = TriggerEvent {
                    trigger_id: trigger.id,
                    trigger_name: trigger.name.clone(),
                    action: trigger.action.clone(),
                    context: context.clone(),
                    timestamp: now,
                };

                trigger.last_triggered = Some(now);
                trigger.trigger_count += 1;

                let _ = self.event_sender.send(event.clone());
                events.push(event);
            }
        }

        events
    }
}

impl Default for ContextTriggerManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trigger_creation() {
        let trigger = ContextTrigger::new("Meeting reminder", ContextTriggerType::CalendarEvent)
            .with_condition(TriggerCondition::CalendarEventSoon { minutes_before: 15 })
            .with_action(TriggerAction::SendMessage {
                template: "You have a meeting in {minutes} minutes".to_string(),
            })
            .with_cooldown(600);

        assert_eq!(trigger.name, "Meeting reminder");
        assert!(trigger.is_ready());
    }

    #[test]
    fn test_condition_evaluation() {
        let context = ContextState {
            current_hour: 10,
            day_of_week: "Monday".to_string(),
            focused_app: Some("VS Code".to_string()),
            ..Default::default()
        };

        let time_condition = TriggerCondition::TimeOfDay {
            start_hour: 9,
            end_hour: 17,
        };
        assert!(time_condition.evaluate(&context));

        let app_condition = TriggerCondition::AppInFocus {
            app_names: vec!["VS Code".to_string(), "IntelliJ".to_string()],
        };
        assert!(app_condition.evaluate(&context));
    }

    #[test]
    fn test_manager() {
        let mut manager = ContextTriggerManager::new();

        let trigger = ContextTrigger::new("Test", ContextTriggerType::TimeBased).with_condition(
            TriggerCondition::TimeOfDay {
                start_hour: 0,
                end_hour: 24,
            },
        );

        manager.add_trigger(trigger);

        let context = ContextState::new();
        let events = manager.evaluate(&context);

        assert_eq!(events.len(), 1);
    }
}
