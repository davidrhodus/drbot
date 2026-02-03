//! Natural language automation for drbot.
//!
//! Define automations in natural language.
//!
//! # Features
//!
//! - Natural language triggers
//! - Action definitions
//! - Workflow automation
//! - Scheduled automations
//! - Event-driven automations

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Automation result type.
pub type Result<T> = std::result::Result<T, AutomateError>;

/// Automation errors.
#[derive(Debug, thiserror::Error)]
pub enum AutomateError {
    #[error("Automation not found: {0}")]
    NotFound(String),
    #[error("Parse failed: {0}")]
    ParseFailed(String),
    #[error("Execution failed: {0}")]
    ExecutionFailed(String),
    #[error("Invalid trigger: {0}")]
    InvalidTrigger(String),
    #[error("Invalid action: {0}")]
    InvalidAction(String),
}

/// Automation definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Automation {
    /// Automation ID.
    pub id: Uuid,
    /// Automation name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Natural language definition.
    pub natural_definition: String,
    /// Parsed trigger.
    pub trigger: Trigger,
    /// Actions to execute.
    pub actions: Vec<Action>,
    /// Conditions.
    pub conditions: Vec<Condition>,
    /// Whether enabled.
    pub enabled: bool,
    /// Priority (higher = executed first).
    pub priority: i32,
    /// Created at.
    pub created_at: DateTime<Utc>,
    /// Last triggered.
    pub last_triggered: Option<DateTime<Utc>>,
    /// Trigger count.
    pub trigger_count: u64,
}

impl Automation {
    /// Create a new automation.
    pub fn new(name: &str, trigger: Trigger) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.to_string(),
            description: String::new(),
            natural_definition: String::new(),
            trigger,
            actions: Vec::new(),
            conditions: Vec::new(),
            enabled: true,
            priority: 0,
            created_at: Utc::now(),
            last_triggered: None,
            trigger_count: 0,
        }
    }

    /// Set description.
    pub fn with_description(mut self, desc: &str) -> Self {
        self.description = desc.to_string();
        self
    }

    /// Add action.
    pub fn with_action(mut self, action: Action) -> Self {
        self.actions.push(action);
        self
    }

    /// Add condition.
    pub fn with_condition(mut self, condition: Condition) -> Self {
        self.conditions.push(condition);
        self
    }

    /// Set priority.
    pub fn with_priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }

    /// Record a trigger.
    pub fn record_trigger(&mut self) {
        self.trigger_count += 1;
        self.last_triggered = Some(Utc::now());
    }
}

/// Trigger types.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Trigger {
    /// Message contains keyword.
    Keyword {
        keywords: Vec<String>,
        case_sensitive: bool,
    },
    /// Message matches pattern.
    Pattern { pattern: String },
    /// Scheduled time.
    Schedule { cron: String },
    /// Event occurred.
    Event { event_type: String },
    /// User joined.
    UserJoined { channel: Option<String> },
    /// User mentioned.
    Mentioned { user_id: Option<String> },
    /// Time-based.
    Time { hour: u8, minute: u8, days: Vec<u8> },
    /// Manual trigger.
    Manual,
    /// Webhook received.
    Webhook { path: String },
    /// File uploaded.
    FileUploaded { mime_types: Vec<String> },
    /// AI detected intent.
    Intent { intent: String, confidence: f32 },
}

impl Trigger {
    /// Check if trigger matches event.
    pub fn matches(&self, event: &TriggerEvent) -> bool {
        match (self, event) {
            (
                Trigger::Keyword {
                    keywords,
                    case_sensitive,
                },
                TriggerEvent::Message { content, .. },
            ) => {
                let check_content = if *case_sensitive {
                    content.clone()
                } else {
                    content.to_lowercase()
                };
                keywords.iter().any(|k| {
                    let check_keyword = if *case_sensitive {
                        k.clone()
                    } else {
                        k.to_lowercase()
                    };
                    check_content.contains(&check_keyword)
                })
            }
            (Trigger::Pattern { pattern }, TriggerEvent::Message { content, .. }) => {
                regex::Regex::new(pattern)
                    .map(|re| re.is_match(content))
                    .unwrap_or(false)
            }
            (Trigger::Event { event_type }, TriggerEvent::Custom { name, .. }) => {
                event_type == name
            }
            (Trigger::UserJoined { channel }, TriggerEvent::UserJoined { channel_id, .. }) => {
                channel.as_ref().map(|c| c == channel_id).unwrap_or(true)
            }
            (
                Trigger::FileUploaded { mime_types },
                TriggerEvent::FileUploaded { mime_type, .. },
            ) => mime_types.is_empty() || mime_types.contains(mime_type),
            (Trigger::Manual, TriggerEvent::Manual { .. }) => true,
            (Trigger::Webhook { path }, TriggerEvent::Webhook { request_path, .. }) => {
                path == request_path
            }
            _ => false,
        }
    }
}

/// Trigger event.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TriggerEvent {
    /// Message received.
    Message {
        content: String,
        user_id: String,
        channel_id: String,
    },
    /// User joined.
    UserJoined { user_id: String, channel_id: String },
    /// File uploaded.
    FileUploaded {
        file_id: String,
        mime_type: String,
        user_id: String,
    },
    /// Webhook called.
    Webhook {
        request_path: String,
        payload: serde_json::Value,
    },
    /// Manual trigger.
    Manual { triggered_by: String },
    /// Custom event.
    Custom {
        name: String,
        data: serde_json::Value,
    },
    /// Scheduled.
    Scheduled { schedule_id: String },
}

/// Action types.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Action {
    /// Send a message.
    SendMessage { channel: String, message: String },
    /// Reply to trigger.
    Reply { message: String },
    /// Call AI.
    AiGenerate {
        prompt: String,
        model: Option<String>,
    },
    /// Execute HTTP request.
    HttpRequest {
        method: String,
        url: String,
        body: Option<serde_json::Value>,
    },
    /// Set variable.
    SetVariable {
        name: String,
        value: serde_json::Value,
    },
    /// Trigger another automation.
    TriggerAutomation { automation_id: String },
    /// Wait/delay.
    Delay { seconds: u64 },
    /// Conditional branch.
    If {
        condition: Condition,
        then_actions: Vec<Action>,
        else_actions: Vec<Action>,
    },
    /// Loop.
    Loop { count: usize, actions: Vec<Action> },
    /// Log message.
    Log { level: String, message: String },
    /// Transform data.
    Transform {
        input: String,
        operation: String,
        output: String,
    },
    /// Notify user.
    Notify { user_id: String, message: String },
}

impl Action {
    /// Create a send message action.
    pub fn send_message(channel: &str, message: &str) -> Self {
        Action::SendMessage {
            channel: channel.to_string(),
            message: message.to_string(),
        }
    }

    /// Create a reply action.
    pub fn reply(message: &str) -> Self {
        Action::Reply {
            message: message.to_string(),
        }
    }

    /// Create an AI generate action.
    pub fn ai_generate(prompt: &str) -> Self {
        Action::AiGenerate {
            prompt: prompt.to_string(),
            model: None,
        }
    }
}

/// Condition types.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Condition {
    /// Always true.
    Always,
    /// Never true.
    Never,
    /// Variable equals value.
    Equals {
        variable: String,
        value: serde_json::Value,
    },
    /// Variable contains value.
    Contains { variable: String, value: String },
    /// Numeric comparison.
    Compare {
        variable: String,
        operator: CompareOp,
        value: f64,
    },
    /// Time-based condition.
    TimeIs { operator: CompareOp, hour: u8 },
    /// Day of week.
    DayIs { days: Vec<u8> },
    /// User is.
    UserIs { user_id: String },
    /// Channel is.
    ChannelIs { channel_id: String },
    /// Logical AND.
    And { conditions: Vec<Condition> },
    /// Logical OR.
    Or { conditions: Vec<Condition> },
    /// Logical NOT.
    Not { condition: Box<Condition> },
}

/// Comparison operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompareOp {
    Equal,
    NotEqual,
    Greater,
    GreaterOrEqual,
    Less,
    LessOrEqual,
}

/// Execution context.
#[derive(Debug, Clone, Default)]
pub struct ExecutionContext {
    /// Variables.
    pub variables: HashMap<String, serde_json::Value>,
    /// Trigger event.
    pub trigger_event: Option<TriggerEvent>,
    /// Results from previous actions.
    pub action_results: Vec<ActionResult>,
}

impl ExecutionContext {
    /// Create a new context.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set a variable.
    pub fn set_variable(&mut self, name: &str, value: serde_json::Value) {
        self.variables.insert(name.to_string(), value);
    }

    /// Get a variable.
    pub fn get_variable(&self, name: &str) -> Option<&serde_json::Value> {
        self.variables.get(name)
    }
}

/// Action result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionResult {
    /// Action index.
    pub action_index: usize,
    /// Success.
    pub success: bool,
    /// Output.
    pub output: Option<serde_json::Value>,
    /// Error message.
    pub error: Option<String>,
    /// Duration in ms.
    pub duration_ms: u64,
}

/// Execution result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    /// Automation ID.
    pub automation_id: Uuid,
    /// Success.
    pub success: bool,
    /// Action results.
    pub action_results: Vec<ActionResult>,
    /// Final context.
    pub final_variables: HashMap<String, serde_json::Value>,
    /// Total duration in ms.
    pub duration_ms: u64,
}

/// Trait for action executors.
#[async_trait]
pub trait ActionExecutor: Send + Sync {
    /// Execute an action.
    async fn execute(
        &self,
        action: &Action,
        context: &mut ExecutionContext,
    ) -> Result<ActionResult>;
}

/// Automation engine.
pub struct AutomationEngine {
    automations: Arc<RwLock<HashMap<Uuid, Automation>>>,
}

impl AutomationEngine {
    /// Create a new automation engine.
    pub fn new() -> Self {
        Self {
            automations: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register an automation.
    pub async fn register(&self, automation: Automation) {
        self.automations
            .write()
            .await
            .insert(automation.id, automation);
    }

    /// Unregister an automation.
    pub async fn unregister(&self, id: Uuid) -> Option<Automation> {
        self.automations.write().await.remove(&id)
    }

    /// Get an automation.
    pub async fn get(&self, id: Uuid) -> Option<Automation> {
        self.automations.read().await.get(&id).cloned()
    }

    /// List all automations.
    pub async fn list(&self) -> Vec<Automation> {
        self.automations.read().await.values().cloned().collect()
    }

    /// Enable/disable an automation.
    pub async fn set_enabled(&self, id: Uuid, enabled: bool) -> Option<()> {
        self.automations.write().await.get_mut(&id).map(|a| {
            a.enabled = enabled;
        })
    }

    /// Process an event and trigger matching automations.
    pub async fn process_event<E: ActionExecutor>(
        &self,
        event: TriggerEvent,
        executor: &E,
    ) -> Vec<ExecutionResult> {
        let mut results = Vec::new();

        let automations: Vec<_> = {
            let guard = self.automations.read().await;
            guard.values().filter(|a| a.enabled).cloned().collect()
        };

        // Sort by priority
        let mut sorted = automations;
        sorted.sort_by(|a, b| b.priority.cmp(&a.priority));

        for automation in sorted {
            if automation.trigger.matches(&event) {
                let result = self
                    .execute_automation(&automation, event.clone(), executor)
                    .await;
                results.push(result);

                // Record trigger
                if let Some(a) = self.automations.write().await.get_mut(&automation.id) {
                    a.record_trigger();
                }
            }
        }

        results
    }

    async fn execute_automation<E: ActionExecutor>(
        &self,
        automation: &Automation,
        event: TriggerEvent,
        executor: &E,
    ) -> ExecutionResult {
        let start = std::time::Instant::now();
        let mut context = ExecutionContext::new();
        context.trigger_event = Some(event);

        let mut success = true;
        let mut action_results = Vec::new();

        for (index, action) in automation.actions.iter().enumerate() {
            let result = executor.execute(action, &mut context).await;

            let action_result = match result {
                Ok(r) => r,
                Err(e) => {
                    success = false;
                    ActionResult {
                        action_index: index,
                        success: false,
                        output: None,
                        error: Some(e.to_string()),
                        duration_ms: 0,
                    }
                }
            };

            context.action_results.push(action_result.clone());
            action_results.push(action_result);
        }

        ExecutionResult {
            automation_id: automation.id,
            success,
            action_results,
            final_variables: context.variables,
            duration_ms: start.elapsed().as_millis() as u64,
        }
    }

    /// Parse natural language automation definition.
    pub fn parse_natural_language(&self, definition: &str) -> Result<Automation> {
        // Simple parsing for common patterns
        let lower = definition.to_lowercase();

        let trigger = if lower.contains("when") && lower.contains("message contains") {
            // Extract keywords
            let keywords = self.extract_quoted_strings(definition);
            Trigger::Keyword {
                keywords,
                case_sensitive: false,
            }
        } else if lower.contains("every day at") || lower.contains("daily at") {
            // Parse time
            let time = self.extract_time(definition);
            Trigger::Time {
                hour: time.0,
                minute: time.1,
                days: vec![1, 2, 3, 4, 5, 6, 7],
            }
        } else if lower.contains("when user joins") {
            Trigger::UserJoined { channel: None }
        } else if lower.contains("when file is uploaded") {
            Trigger::FileUploaded {
                mime_types: Vec::new(),
            }
        } else {
            return Err(AutomateError::ParseFailed(
                "Could not parse trigger".to_string(),
            ));
        };

        let mut automation = Automation::new("Parsed Automation", trigger);
        automation.natural_definition = definition.to_string();

        // Parse actions
        if lower.contains("reply with") || lower.contains("respond with") {
            let message = self
                .extract_quoted_strings(definition)
                .pop()
                .unwrap_or_default();
            automation.actions.push(Action::reply(&message));
        } else if lower.contains("send message") {
            let parts: Vec<_> = self.extract_quoted_strings(definition);
            if !parts.is_empty() {
                automation.actions.push(Action::reply(&parts[0]));
            }
        }

        Ok(automation)
    }

    fn extract_quoted_strings(&self, text: &str) -> Vec<String> {
        let re = regex::Regex::new(r#"["']([^"']+)["']"#).unwrap();
        re.captures_iter(text)
            .filter_map(|c| c.get(1).map(|m| m.as_str().to_string()))
            .collect()
    }

    fn extract_time(&self, text: &str) -> (u8, u8) {
        let re = regex::Regex::new(r"(\d{1,2}):(\d{2})").unwrap();
        if let Some(caps) = re.captures(text) {
            let hour = caps
                .get(1)
                .and_then(|m| m.as_str().parse().ok())
                .unwrap_or(9);
            let minute = caps
                .get(2)
                .and_then(|m| m.as_str().parse().ok())
                .unwrap_or(0);
            (hour, minute)
        } else {
            (9, 0)
        }
    }
}

impl Default for AutomationEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Simple action executor for testing.
pub struct SimpleExecutor;

#[async_trait]
impl ActionExecutor for SimpleExecutor {
    async fn execute(
        &self,
        action: &Action,
        context: &mut ExecutionContext,
    ) -> Result<ActionResult> {
        let start = std::time::Instant::now();

        let output = match action {
            Action::Reply { message } => Some(serde_json::json!({ "message": message })),
            Action::SendMessage { channel, message } => {
                Some(serde_json::json!({ "channel": channel, "message": message }))
            }
            Action::SetVariable { name, value } => {
                context.set_variable(name, value.clone());
                None
            }
            Action::Delay { seconds } => {
                tokio::time::sleep(tokio::time::Duration::from_secs(*seconds)).await;
                None
            }
            Action::Log { level, message } => {
                tracing::info!(level = %level, "Automation log: {}", message);
                None
            }
            _ => None,
        };

        Ok(ActionResult {
            action_index: context.action_results.len(),
            success: true,
            output,
            error: None,
            duration_ms: start.elapsed().as_millis() as u64,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_automation_engine() {
        let engine = AutomationEngine::new();

        let automation = Automation::new(
            "Test",
            Trigger::Keyword {
                keywords: vec!["hello".to_string()],
                case_sensitive: false,
            },
        )
        .with_action(Action::reply("Hi there!"));

        engine.register(automation).await;

        let automations = engine.list().await;
        assert_eq!(automations.len(), 1);
    }

    #[tokio::test]
    async fn test_trigger_matching() {
        let trigger = Trigger::Keyword {
            keywords: vec!["test".to_string()],
            case_sensitive: false,
        };

        let event = TriggerEvent::Message {
            content: "This is a test message".to_string(),
            user_id: "user-1".to_string(),
            channel_id: "ch-1".to_string(),
        };

        assert!(trigger.matches(&event));
    }

    #[tokio::test]
    async fn test_natural_language_parsing() {
        let engine = AutomationEngine::new();

        let result = engine
            .parse_natural_language(r#"When message contains "hello", reply with "Hi there!""#);

        assert!(result.is_ok());
        let automation = result.unwrap();
        assert!(!automation.actions.is_empty());
    }

    #[tokio::test]
    async fn test_process_event() {
        let engine = AutomationEngine::new();
        let executor = SimpleExecutor;

        let automation = Automation::new(
            "Greeting",
            Trigger::Keyword {
                keywords: vec!["hello".to_string()],
                case_sensitive: false,
            },
        )
        .with_action(Action::reply("Welcome!"));

        engine.register(automation).await;

        let event = TriggerEvent::Message {
            content: "Hello everyone!".to_string(),
            user_id: "user-1".to_string(),
            channel_id: "general".to_string(),
        };

        let results = engine.process_event(event, &executor).await;
        assert_eq!(results.len(), 1);
        assert!(results[0].success);
    }
}
