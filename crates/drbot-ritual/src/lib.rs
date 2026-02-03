//! Workflow ritualization and pattern learning.
//!
//! This crate provides workflow automation capabilities:
//! - Learn repeated workflow patterns
//! - Suggest automation for rituals
//! - Execute ritualized workflows
//! - Track workflow effectiveness

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Ritual errors.
#[derive(Debug, Error)]
pub enum RitualError {
    #[error("Ritual not found: {0}")]
    RitualNotFound(String),

    #[error("Execution failed: {0}")]
    ExecutionFailed(String),

    #[error("Pattern not detected: {0}")]
    PatternNotDetected(String),

    #[error("Provider error: {0}")]
    ProviderError(String),
}

/// Result type for ritual operations.
pub type Result<T> = std::result::Result<T, RitualError>;

/// A ritualized workflow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ritual {
    /// Ritual identifier.
    pub id: String,
    /// Ritual name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Trigger conditions.
    pub triggers: Vec<Trigger>,
    /// Workflow steps.
    pub steps: Vec<RitualStep>,
    /// Created from observations.
    pub source: RitualSource,
    /// Effectiveness metrics.
    pub metrics: RitualMetrics,
    /// Whether enabled.
    pub enabled: bool,
    /// Created timestamp.
    pub created_at: DateTime<Utc>,
}

/// What triggers a ritual.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trigger {
    /// Trigger type.
    pub trigger_type: TriggerType,
    /// Trigger condition.
    pub condition: String,
    /// Priority.
    pub priority: u8,
}

/// Types of triggers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TriggerType {
    /// Time-based (cron-like).
    Schedule { cron: String },
    /// Event-based.
    Event { event_type: String },
    /// Keyword in conversation.
    Keyword { keywords: Vec<String> },
    /// Context match.
    Context { pattern: String },
    /// Manual invocation.
    Manual,
}

/// A step in a ritual.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RitualStep {
    /// Step identifier.
    pub id: String,
    /// Step name.
    pub name: String,
    /// Step action.
    pub action: StepAction,
    /// Dependencies on other steps.
    pub depends_on: Vec<String>,
    /// Timeout in seconds.
    pub timeout_secs: Option<u32>,
    /// Retry configuration.
    pub retry: RetryConfig,
}

/// Actions a step can perform.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StepAction {
    /// Send a message.
    Message { template: String },
    /// Make an API call.
    ApiCall {
        endpoint: String,
        method: String,
        body: Option<String>,
    },
    /// Run a command.
    Command { command: String },
    /// Conditional branch.
    Conditional {
        condition: String,
        then_step: String,
        else_step: Option<String>,
    },
    /// Wait for input.
    WaitForInput { prompt: String, timeout_secs: u32 },
    /// Transform data.
    Transform {
        input: String,
        transformation: String,
    },
    /// Store data.
    Store { key: String, value: String },
    /// Custom action.
    Custom {
        handler: String,
        params: HashMap<String, serde_json::Value>,
    },
}

/// Retry configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    /// Maximum retries.
    pub max_retries: u32,
    /// Backoff in milliseconds.
    pub backoff_ms: u64,
    /// Exponential backoff.
    pub exponential: bool,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            backoff_ms: 1000,
            exponential: true,
        }
    }
}

/// How the ritual was created.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RitualSource {
    /// Created manually.
    Manual,
    /// Learned from observations.
    Learned {
        observation_count: u32,
        confidence: f64,
    },
    /// Suggested by system.
    Suggested { reason: String },
    /// Imported.
    Imported { source: String },
}

/// Metrics for a ritual.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RitualMetrics {
    /// Times executed.
    pub executions: u32,
    /// Successful executions.
    pub successes: u32,
    /// Average duration in ms.
    pub avg_duration_ms: u64,
    /// Last executed.
    pub last_executed: Option<DateTime<Utc>>,
    /// User satisfaction score.
    pub satisfaction: Option<f64>,
}

/// An observed workflow pattern.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Observation {
    /// Observation ID.
    pub id: String,
    /// Sequence of actions observed.
    pub actions: Vec<ObservedAction>,
    /// Context when observed.
    pub context: ObservationContext,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
}

/// An observed action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservedAction {
    /// Action type.
    pub action_type: String,
    /// Action parameters.
    pub params: HashMap<String, serde_json::Value>,
    /// Duration in ms.
    pub duration_ms: u64,
    /// Result.
    pub result: ActionResult,
}

/// Result of an observed action.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ActionResult {
    Success,
    Failure,
    Partial,
    Skipped,
}

/// Context of an observation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservationContext {
    /// User ID.
    pub user_id: String,
    /// Session ID.
    pub session_id: Option<String>,
    /// Time of day.
    pub time_of_day: TimeOfDay,
    /// Day of week.
    pub day_of_week: DayOfWeek,
    /// Additional context.
    pub metadata: HashMap<String, String>,
}

/// Time of day.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum TimeOfDay {
    Morning,
    Afternoon,
    Evening,
    Night,
}

/// Day of week.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum DayOfWeek {
    Monday,
    Tuesday,
    Wednesday,
    Thursday,
    Friday,
    Saturday,
    Sunday,
}

/// Execution result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    /// Ritual ID.
    pub ritual_id: String,
    /// Execution ID.
    pub execution_id: String,
    /// Status.
    pub status: ExecutionStatus,
    /// Step results.
    pub step_results: Vec<StepResult>,
    /// Duration in ms.
    pub duration_ms: u64,
    /// Timestamp.
    pub completed_at: DateTime<Utc>,
}

/// Execution status.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ExecutionStatus {
    Success,
    PartialSuccess,
    Failure,
    Cancelled,
}

/// Result of a step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepResult {
    /// Step ID.
    pub step_id: String,
    /// Success.
    pub success: bool,
    /// Output.
    pub output: Option<serde_json::Value>,
    /// Error if any.
    pub error: Option<String>,
    /// Duration in ms.
    pub duration_ms: u64,
}

/// Provider for ritual operations.
#[async_trait]
pub trait RitualProvider: Send + Sync {
    /// Detect patterns from observations.
    async fn detect_patterns(&self, observations: &[Observation]) -> Result<Vec<DetectedPattern>>;

    /// Suggest a ritual from a pattern.
    async fn suggest_ritual(&self, pattern: &DetectedPattern) -> Result<Ritual>;

    /// Execute a step action.
    async fn execute_action(
        &self,
        action: &StepAction,
        context: &ExecutionContext,
    ) -> Result<serde_json::Value>;
}

/// A detected pattern.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedPattern {
    /// Pattern ID.
    pub id: String,
    /// Pattern description.
    pub description: String,
    /// Action sequence.
    pub sequence: Vec<String>,
    /// Frequency.
    pub frequency: u32,
    /// Confidence.
    pub confidence: f64,
    /// Typical context.
    pub typical_context: HashMap<String, String>,
}

/// Context for execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionContext {
    /// Variables.
    pub variables: HashMap<String, serde_json::Value>,
    /// Step outputs.
    pub step_outputs: HashMap<String, serde_json::Value>,
    /// User ID.
    pub user_id: String,
}

/// The ritual engine.
pub struct RitualEngine {
    /// Provider for operations.
    provider: Arc<dyn RitualProvider>,
    /// Registered rituals.
    rituals: Arc<RwLock<HashMap<String, Ritual>>>,
    /// Observations for learning.
    observations: Arc<RwLock<Vec<Observation>>>,
    /// Execution history.
    executions: Arc<RwLock<Vec<ExecutionResult>>>,
}

impl RitualEngine {
    /// Create a new ritual engine.
    pub fn new(provider: Arc<dyn RitualProvider>) -> Self {
        Self {
            provider,
            rituals: Arc::new(RwLock::new(HashMap::new())),
            observations: Arc::new(RwLock::new(Vec::new())),
            executions: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Register a ritual.
    pub async fn register_ritual(&self, ritual: Ritual) -> Result<String> {
        let id = ritual.id.clone();
        let mut rituals = self.rituals.write().await;
        rituals.insert(id.clone(), ritual);
        Ok(id)
    }

    /// Record an observation.
    pub async fn record_observation(&self, observation: Observation) {
        let mut observations = self.observations.write().await;
        observations.push(observation);
    }

    /// Learn rituals from observations.
    pub async fn learn(&self) -> Result<Vec<Ritual>> {
        let observations: Vec<_> = {
            let obs = self.observations.read().await;
            obs.clone()
        };

        if observations.len() < 3 {
            return Err(RitualError::PatternNotDetected(
                "Not enough observations".to_string(),
            ));
        }

        let patterns = self.provider.detect_patterns(&observations).await?;

        let mut learned = Vec::new();
        for pattern in patterns {
            if pattern.confidence >= 0.7 {
                let ritual = self.provider.suggest_ritual(&pattern).await?;
                self.register_ritual(ritual.clone()).await?;
                learned.push(ritual);
            }
        }

        Ok(learned)
    }

    /// Execute a ritual.
    pub async fn execute(&self, ritual_id: &str, user_id: &str) -> Result<ExecutionResult> {
        let ritual = {
            let rituals = self.rituals.read().await;
            rituals
                .get(ritual_id)
                .cloned()
                .ok_or_else(|| RitualError::RitualNotFound(ritual_id.to_string()))?
        };

        let start = std::time::Instant::now();
        let execution_id = Uuid::new_v4().to_string();

        let mut context = ExecutionContext {
            variables: HashMap::new(),
            step_outputs: HashMap::new(),
            user_id: user_id.to_string(),
        };

        let mut step_results = Vec::new();
        let mut all_success = true;

        for step in &ritual.steps {
            let step_start = std::time::Instant::now();

            let result = self.provider.execute_action(&step.action, &context).await;

            let step_result = match result {
                Ok(output) => {
                    context.step_outputs.insert(step.id.clone(), output.clone());
                    StepResult {
                        step_id: step.id.clone(),
                        success: true,
                        output: Some(output),
                        error: None,
                        duration_ms: step_start.elapsed().as_millis() as u64,
                    }
                }
                Err(e) => {
                    all_success = false;
                    StepResult {
                        step_id: step.id.clone(),
                        success: false,
                        output: None,
                        error: Some(e.to_string()),
                        duration_ms: step_start.elapsed().as_millis() as u64,
                    }
                }
            };

            step_results.push(step_result);
        }

        let result = ExecutionResult {
            ritual_id: ritual_id.to_string(),
            execution_id,
            status: if all_success {
                ExecutionStatus::Success
            } else {
                ExecutionStatus::PartialSuccess
            },
            step_results,
            duration_ms: start.elapsed().as_millis() as u64,
            completed_at: Utc::now(),
        };

        // Update metrics
        let mut rituals = self.rituals.write().await;
        if let Some(r) = rituals.get_mut(ritual_id) {
            r.metrics.executions += 1;
            if all_success {
                r.metrics.successes += 1;
            }
            r.metrics.last_executed = Some(Utc::now());
        }

        // Store execution
        let mut executions = self.executions.write().await;
        executions.push(result.clone());

        Ok(result)
    }

    /// Get all rituals.
    pub async fn list_rituals(&self) -> Vec<Ritual> {
        let rituals = self.rituals.read().await;
        rituals.values().cloned().collect()
    }

    /// Get ritual by ID.
    pub async fn get_ritual(&self, id: &str) -> Option<Ritual> {
        let rituals = self.rituals.read().await;
        rituals.get(id).cloned()
    }

    /// Check if any ritual should trigger.
    pub async fn check_triggers(&self, context: &HashMap<String, String>) -> Vec<Ritual> {
        let rituals = self.rituals.read().await;

        rituals
            .values()
            .filter(|r| r.enabled)
            .filter(|r| {
                r.triggers.iter().any(|t| match &t.trigger_type {
                    TriggerType::Keyword { keywords } => {
                        if let Some(text) = context.get("text") {
                            keywords
                                .iter()
                                .any(|k| text.to_lowercase().contains(&k.to_lowercase()))
                        } else {
                            false
                        }
                    }
                    TriggerType::Manual => false,
                    _ => false,
                })
            })
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockProvider;

    #[async_trait]
    impl RitualProvider for MockProvider {
        async fn detect_patterns(
            &self,
            observations: &[Observation],
        ) -> Result<Vec<DetectedPattern>> {
            Ok(vec![DetectedPattern {
                id: Uuid::new_v4().to_string(),
                description: "Daily standup".to_string(),
                sequence: observations
                    .first()
                    .map(|o| o.actions.iter().map(|a| a.action_type.clone()).collect())
                    .unwrap_or_default(),
                frequency: observations.len() as u32,
                confidence: 0.85,
                typical_context: HashMap::new(),
            }])
        }

        async fn suggest_ritual(&self, pattern: &DetectedPattern) -> Result<Ritual> {
            Ok(Ritual {
                id: Uuid::new_v4().to_string(),
                name: pattern.description.clone(),
                description: format!("Learned ritual: {}", pattern.description),
                triggers: vec![Trigger {
                    trigger_type: TriggerType::Manual,
                    condition: String::new(),
                    priority: 5,
                }],
                steps: pattern
                    .sequence
                    .iter()
                    .enumerate()
                    .map(|(i, action)| RitualStep {
                        id: format!("step_{}", i),
                        name: action.clone(),
                        action: StepAction::Message {
                            template: action.clone(),
                        },
                        depends_on: vec![],
                        timeout_secs: None,
                        retry: RetryConfig::default(),
                    })
                    .collect(),
                source: RitualSource::Learned {
                    observation_count: pattern.frequency,
                    confidence: pattern.confidence,
                },
                metrics: RitualMetrics::default(),
                enabled: true,
                created_at: Utc::now(),
            })
        }

        async fn execute_action(
            &self,
            action: &StepAction,
            _context: &ExecutionContext,
        ) -> Result<serde_json::Value> {
            match action {
                StepAction::Message { template } => Ok(serde_json::json!({ "sent": template })),
                _ => Ok(serde_json::json!({ "executed": true })),
            }
        }
    }

    #[tokio::test]
    async fn test_register_ritual() {
        let provider = Arc::new(MockProvider);
        let engine = RitualEngine::new(provider);

        let ritual = Ritual {
            id: "r1".to_string(),
            name: "Test Ritual".to_string(),
            description: "A test ritual".to_string(),
            triggers: vec![],
            steps: vec![],
            source: RitualSource::Manual,
            metrics: RitualMetrics::default(),
            enabled: true,
            created_at: Utc::now(),
        };

        let id = engine.register_ritual(ritual).await.unwrap();
        assert_eq!(id, "r1");
    }

    #[tokio::test]
    async fn test_learn_from_observations() {
        let provider = Arc::new(MockProvider);
        let engine = RitualEngine::new(provider);

        // Add observations
        for i in 0..5 {
            engine
                .record_observation(Observation {
                    id: format!("obs_{}", i),
                    actions: vec![
                        ObservedAction {
                            action_type: "check_email".to_string(),
                            params: HashMap::new(),
                            duration_ms: 100,
                            result: ActionResult::Success,
                        },
                        ObservedAction {
                            action_type: "update_status".to_string(),
                            params: HashMap::new(),
                            duration_ms: 50,
                            result: ActionResult::Success,
                        },
                    ],
                    context: ObservationContext {
                        user_id: "user1".to_string(),
                        session_id: None,
                        time_of_day: TimeOfDay::Morning,
                        day_of_week: DayOfWeek::Monday,
                        metadata: HashMap::new(),
                    },
                    timestamp: Utc::now(),
                })
                .await;
        }

        let learned = engine.learn().await.unwrap();
        assert!(!learned.is_empty());
    }

    #[tokio::test]
    async fn test_execute_ritual() {
        let provider = Arc::new(MockProvider);
        let engine = RitualEngine::new(provider);

        let ritual = Ritual {
            id: "r1".to_string(),
            name: "Test".to_string(),
            description: "Test".to_string(),
            triggers: vec![],
            steps: vec![RitualStep {
                id: "s1".to_string(),
                name: "Send greeting".to_string(),
                action: StepAction::Message {
                    template: "Hello!".to_string(),
                },
                depends_on: vec![],
                timeout_secs: None,
                retry: RetryConfig::default(),
            }],
            source: RitualSource::Manual,
            metrics: RitualMetrics::default(),
            enabled: true,
            created_at: Utc::now(),
        };

        engine.register_ritual(ritual).await.unwrap();
        let result = engine.execute("r1", "user1").await.unwrap();

        assert!(matches!(result.status, ExecutionStatus::Success));
        assert_eq!(result.step_results.len(), 1);
    }

    #[test]
    fn test_trigger_types() {
        let schedule = TriggerType::Schedule {
            cron: "0 9 * * *".to_string(),
        };
        let keyword = TriggerType::Keyword {
            keywords: vec!["standup".to_string()],
        };

        let _ = serde_json::to_string(&schedule).unwrap();
        let _ = serde_json::to_string(&keyword).unwrap();
    }
}
