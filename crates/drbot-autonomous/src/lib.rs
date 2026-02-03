//! Autonomous multi-step task execution with self-healing workflows.
//!
//! This crate provides autonomous task execution capabilities that can:
//! - Break complex goals into executable steps
//! - Execute multi-step workflows with dependency tracking
//! - Detect and recover from failures automatically
//! - Learn from execution patterns to improve future runs

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Autonomous execution errors.
#[derive(Debug, Error)]
pub enum AutonomousError {
    #[error("Task planning failed: {0}")]
    PlanningFailed(String),

    #[error("Step execution failed: {0}")]
    ExecutionFailed(String),

    #[error("Recovery failed after {attempts} attempts: {reason}")]
    RecoveryFailed { attempts: u32, reason: String },

    #[error("Timeout after {0} seconds")]
    Timeout(u64),

    #[error("Task not found: {0}")]
    TaskNotFound(String),

    #[error("Circular dependency detected")]
    CircularDependency,

    #[error("Provider error: {0}")]
    ProviderError(String),
}

/// Result type for autonomous operations.
pub type Result<T> = std::result::Result<T, AutonomousError>;

/// Task status in the execution pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskStatus {
    /// Task is pending execution.
    Pending,
    /// Task is currently planning.
    Planning,
    /// Task is ready to execute.
    Ready,
    /// Task is currently executing.
    Executing,
    /// Task is recovering from failure.
    Recovering,
    /// Task completed successfully.
    Completed,
    /// Task failed permanently.
    Failed,
    /// Task was cancelled.
    Cancelled,
}

/// A single executable step in a task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Step {
    /// Unique step identifier.
    pub id: String,
    /// Human-readable step name.
    pub name: String,
    /// Description of what this step does.
    pub description: String,
    /// Step type for execution routing.
    pub step_type: StepType,
    /// Parameters for this step.
    pub parameters: HashMap<String, serde_json::Value>,
    /// IDs of steps this depends on.
    pub dependencies: Vec<String>,
    /// Expected outputs from this step.
    pub expected_outputs: Vec<String>,
    /// Maximum retry attempts.
    pub max_retries: u32,
    /// Current retry count.
    pub retry_count: u32,
    /// Step status.
    pub status: TaskStatus,
    /// Actual outputs after execution.
    pub outputs: HashMap<String, serde_json::Value>,
    /// Error message if failed.
    pub error: Option<String>,
}

/// Type of step for execution routing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StepType {
    /// AI inference step.
    Inference { prompt_template: String },
    /// Tool execution step.
    ToolCall { tool_name: String },
    /// Code execution step.
    CodeExecution { language: String, code: String },
    /// HTTP request step.
    HttpRequest { method: String, url: String },
    /// File operation step.
    FileOperation { operation: String, path: String },
    /// Conditional branch step.
    Conditional { condition: String },
    /// Parallel execution of sub-steps.
    Parallel { sub_steps: Vec<String> },
    /// Human approval checkpoint.
    HumanApproval { message: String },
    /// Custom step type.
    Custom { handler: String },
}

/// An autonomous task with planning and execution capabilities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutonomousTask {
    /// Unique task identifier.
    pub id: String,
    /// High-level goal description.
    pub goal: String,
    /// Task context and constraints.
    pub context: TaskContext,
    /// Planned steps for execution.
    pub steps: Vec<Step>,
    /// Current task status.
    pub status: TaskStatus,
    /// Execution history for learning.
    pub history: Vec<ExecutionEvent>,
    /// Task metadata.
    pub metadata: TaskMetadata,
}

/// Context for task planning and execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskContext {
    /// Available tools and capabilities.
    pub available_tools: Vec<String>,
    /// Environment variables.
    pub environment: HashMap<String, String>,
    /// Resource constraints.
    pub constraints: ResourceConstraints,
    /// User preferences.
    pub preferences: HashMap<String, serde_json::Value>,
}

/// Resource constraints for task execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceConstraints {
    /// Maximum execution time in seconds.
    pub max_duration_secs: u64,
    /// Maximum memory usage in bytes.
    pub max_memory_bytes: Option<u64>,
    /// Maximum API calls.
    pub max_api_calls: Option<u32>,
    /// Maximum cost in cents.
    pub max_cost_cents: Option<u32>,
}

impl Default for ResourceConstraints {
    fn default() -> Self {
        Self {
            max_duration_secs: 300,
            max_memory_bytes: None,
            max_api_calls: Some(100),
            max_cost_cents: Some(1000),
        }
    }
}

/// Metadata about a task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskMetadata {
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Last update timestamp.
    pub updated_at: DateTime<Utc>,
    /// Completion timestamp.
    pub completed_at: Option<DateTime<Utc>>,
    /// Total execution time in milliseconds.
    pub execution_time_ms: u64,
    /// Number of recovery attempts.
    pub recovery_attempts: u32,
    /// User who created the task.
    pub created_by: Option<String>,
    /// Tags for organization.
    pub tags: Vec<String>,
}

/// An event in the execution history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionEvent {
    /// Event timestamp.
    pub timestamp: DateTime<Utc>,
    /// Event type.
    pub event_type: ExecutionEventType,
    /// Associated step ID if applicable.
    pub step_id: Option<String>,
    /// Event details.
    pub details: serde_json::Value,
}

/// Types of execution events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExecutionEventType {
    TaskCreated,
    PlanningStarted,
    PlanningCompleted,
    StepStarted,
    StepCompleted,
    StepFailed,
    RecoveryStarted,
    RecoverySucceeded,
    RecoveryFailed,
    TaskCompleted,
    TaskFailed,
    TaskCancelled,
}

/// Recovery strategy for failed steps.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RecoveryStrategy {
    /// Retry the same step.
    Retry { max_attempts: u32, backoff_ms: u64 },
    /// Try an alternative approach.
    Alternative { alternative_step: Step },
    /// Skip and continue.
    Skip { default_output: serde_json::Value },
    /// Rollback and replan.
    Replan { from_step: String },
    /// Escalate to human.
    Escalate { message: String },
    /// Fail the entire task.
    Fail,
}

/// Provider trait for autonomous execution capabilities.
#[async_trait]
pub trait AutonomousProvider: Send + Sync {
    /// Plan steps to achieve a goal.
    async fn plan(&self, goal: &str, context: &TaskContext) -> Result<Vec<Step>>;

    /// Execute a single step.
    async fn execute_step(
        &self,
        step: &Step,
        context: &TaskContext,
    ) -> Result<HashMap<String, serde_json::Value>>;

    /// Determine recovery strategy for a failed step.
    async fn determine_recovery(
        &self,
        step: &Step,
        error: &str,
        context: &TaskContext,
    ) -> Result<RecoveryStrategy>;

    /// Replan from a specific step.
    async fn replan(&self, task: &AutonomousTask, from_step: &str) -> Result<Vec<Step>>;
}

/// The autonomous executor that manages task execution.
pub struct AutonomousExecutor {
    /// Task provider for planning and execution.
    provider: Arc<dyn AutonomousProvider>,
    /// Active tasks.
    tasks: Arc<RwLock<HashMap<String, AutonomousTask>>>,
    /// Execution patterns learned from history.
    patterns: Arc<RwLock<Vec<ExecutionPattern>>>,
}

/// A learned execution pattern for optimization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionPattern {
    /// Pattern identifier.
    pub id: String,
    /// Goal pattern (regex or template).
    pub goal_pattern: String,
    /// Successful step sequence.
    pub successful_steps: Vec<String>,
    /// Average execution time.
    pub avg_execution_time_ms: u64,
    /// Success rate (0.0 - 1.0).
    pub success_rate: f64,
    /// Number of times this pattern was used.
    pub usage_count: u32,
}

impl AutonomousExecutor {
    /// Create a new autonomous executor.
    pub fn new(provider: Arc<dyn AutonomousProvider>) -> Self {
        Self {
            provider,
            tasks: Arc::new(RwLock::new(HashMap::new())),
            patterns: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Create a new task from a goal.
    pub async fn create_task(&self, goal: &str, context: TaskContext) -> Result<AutonomousTask> {
        let now = Utc::now();
        let task = AutonomousTask {
            id: Uuid::new_v4().to_string(),
            goal: goal.to_string(),
            context,
            steps: Vec::new(),
            status: TaskStatus::Pending,
            history: vec![ExecutionEvent {
                timestamp: now,
                event_type: ExecutionEventType::TaskCreated,
                step_id: None,
                details: serde_json::json!({ "goal": goal }),
            }],
            metadata: TaskMetadata {
                created_at: now,
                updated_at: now,
                completed_at: None,
                execution_time_ms: 0,
                recovery_attempts: 0,
                created_by: None,
                tags: Vec::new(),
            },
        };

        let mut tasks = self.tasks.write().await;
        tasks.insert(task.id.clone(), task.clone());

        Ok(task)
    }

    /// Plan and execute a task.
    pub async fn execute(&self, task_id: &str) -> Result<AutonomousTask> {
        let mut task = {
            let tasks = self.tasks.read().await;
            tasks
                .get(task_id)
                .cloned()
                .ok_or_else(|| AutonomousError::TaskNotFound(task_id.to_string()))?
        };

        // Planning phase
        task.status = TaskStatus::Planning;
        task.history.push(ExecutionEvent {
            timestamp: Utc::now(),
            event_type: ExecutionEventType::PlanningStarted,
            step_id: None,
            details: serde_json::json!({}),
        });

        let steps = self.provider.plan(&task.goal, &task.context).await?;
        task.steps = steps;

        task.history.push(ExecutionEvent {
            timestamp: Utc::now(),
            event_type: ExecutionEventType::PlanningCompleted,
            step_id: None,
            details: serde_json::json!({ "step_count": task.steps.len() }),
        });

        // Execution phase
        task.status = TaskStatus::Executing;
        let start_time = std::time::Instant::now();

        let execution_result = self.execute_steps(&mut task).await;

        task.metadata.execution_time_ms = start_time.elapsed().as_millis() as u64;
        task.metadata.updated_at = Utc::now();

        match execution_result {
            Ok(_) => {
                task.status = TaskStatus::Completed;
                task.metadata.completed_at = Some(Utc::now());
                task.history.push(ExecutionEvent {
                    timestamp: Utc::now(),
                    event_type: ExecutionEventType::TaskCompleted,
                    step_id: None,
                    details: serde_json::json!({}),
                });

                // Learn from successful execution
                self.learn_pattern(&task).await;
            }
            Err(e) => {
                task.status = TaskStatus::Failed;
                task.history.push(ExecutionEvent {
                    timestamp: Utc::now(),
                    event_type: ExecutionEventType::TaskFailed,
                    step_id: None,
                    details: serde_json::json!({ "error": e.to_string() }),
                });

                // Update task in storage
                let mut tasks = self.tasks.write().await;
                tasks.insert(task.id.clone(), task.clone());

                return Err(e);
            }
        }

        // Update task in storage
        let mut tasks = self.tasks.write().await;
        tasks.insert(task.id.clone(), task.clone());

        Ok(task)
    }

    /// Execute all steps in order respecting dependencies.
    async fn execute_steps(&self, task: &mut AutonomousTask) -> Result<()> {
        let step_count = task.steps.len();
        let mut completed_steps: Vec<String> = Vec::new();

        while completed_steps.len() < step_count {
            // Find next executable step
            let next_step_idx = task.steps.iter().position(|s| {
                s.status == TaskStatus::Pending
                    && s.dependencies
                        .iter()
                        .all(|dep| completed_steps.contains(dep))
            });

            let step_idx = match next_step_idx {
                Some(idx) => idx,
                None => {
                    // Check for circular dependency
                    let pending_count = task
                        .steps
                        .iter()
                        .filter(|s| s.status == TaskStatus::Pending)
                        .count();
                    if pending_count > 0 {
                        return Err(AutonomousError::CircularDependency);
                    }
                    break;
                }
            };

            // Execute the step
            task.steps[step_idx].status = TaskStatus::Executing;
            task.history.push(ExecutionEvent {
                timestamp: Utc::now(),
                event_type: ExecutionEventType::StepStarted,
                step_id: Some(task.steps[step_idx].id.clone()),
                details: serde_json::json!({ "name": task.steps[step_idx].name }),
            });

            match self.execute_step_with_recovery(task, step_idx).await {
                Ok(outputs) => {
                    task.steps[step_idx].outputs = outputs;
                    task.steps[step_idx].status = TaskStatus::Completed;
                    completed_steps.push(task.steps[step_idx].id.clone());

                    task.history.push(ExecutionEvent {
                        timestamp: Utc::now(),
                        event_type: ExecutionEventType::StepCompleted,
                        step_id: Some(task.steps[step_idx].id.clone()),
                        details: serde_json::json!({}),
                    });
                }
                Err(e) => {
                    task.steps[step_idx].status = TaskStatus::Failed;
                    task.steps[step_idx].error = Some(e.to_string());

                    task.history.push(ExecutionEvent {
                        timestamp: Utc::now(),
                        event_type: ExecutionEventType::StepFailed,
                        step_id: Some(task.steps[step_idx].id.clone()),
                        details: serde_json::json!({ "error": e.to_string() }),
                    });

                    return Err(e);
                }
            }
        }

        Ok(())
    }

    /// Execute a step with automatic recovery on failure.
    async fn execute_step_with_recovery(
        &self,
        task: &mut AutonomousTask,
        step_idx: usize,
    ) -> Result<HashMap<String, serde_json::Value>> {
        loop {
            let step = task.steps[step_idx].clone();

            match self.provider.execute_step(&step, &task.context).await {
                Ok(outputs) => return Ok(outputs),
                Err(e) => {
                    // Attempt recovery
                    task.steps[step_idx].status = TaskStatus::Recovering;
                    task.metadata.recovery_attempts += 1;

                    task.history.push(ExecutionEvent {
                        timestamp: Utc::now(),
                        event_type: ExecutionEventType::RecoveryStarted,
                        step_id: Some(step.id.clone()),
                        details: serde_json::json!({ "error": e.to_string() }),
                    });

                    let recovery = self
                        .provider
                        .determine_recovery(&step, &e.to_string(), &task.context)
                        .await?;

                    match recovery {
                        RecoveryStrategy::Retry {
                            max_attempts,
                            backoff_ms,
                        } => {
                            if task.steps[step_idx].retry_count < max_attempts {
                                task.steps[step_idx].retry_count += 1;
                                tokio::time::sleep(tokio::time::Duration::from_millis(backoff_ms))
                                    .await;
                                // Loop continues for retry
                                continue;
                            }
                            return Err(AutonomousError::RecoveryFailed {
                                attempts: task.steps[step_idx].retry_count,
                                reason: "Max retries exceeded".to_string(),
                            });
                        }
                        RecoveryStrategy::Skip { default_output } => {
                            task.history.push(ExecutionEvent {
                                timestamp: Utc::now(),
                                event_type: ExecutionEventType::RecoverySucceeded,
                                step_id: Some(step.id.clone()),
                                details: serde_json::json!({ "strategy": "skip" }),
                            });

                            let mut outputs = HashMap::new();
                            outputs.insert("default".to_string(), default_output);
                            return Ok(outputs);
                        }
                        RecoveryStrategy::Fail => {
                            return Err(e);
                        }
                        _ => {
                            // Other strategies require more complex handling
                            return Err(e);
                        }
                    }
                }
            }
        }
    }

    /// Learn execution pattern from a successful task.
    async fn learn_pattern(&self, task: &AutonomousTask) {
        let pattern = ExecutionPattern {
            id: Uuid::new_v4().to_string(),
            goal_pattern: task.goal.clone(),
            successful_steps: task.steps.iter().map(|s| s.name.clone()).collect(),
            avg_execution_time_ms: task.metadata.execution_time_ms,
            success_rate: 1.0,
            usage_count: 1,
        };

        let mut patterns = self.patterns.write().await;
        patterns.push(pattern);
    }

    /// Get a task by ID.
    pub async fn get_task(&self, task_id: &str) -> Option<AutonomousTask> {
        let tasks = self.tasks.read().await;
        tasks.get(task_id).cloned()
    }

    /// Cancel a running task.
    pub async fn cancel_task(&self, task_id: &str) -> Result<()> {
        let mut tasks = self.tasks.write().await;
        if let Some(task) = tasks.get_mut(task_id) {
            task.status = TaskStatus::Cancelled;
            task.history.push(ExecutionEvent {
                timestamp: Utc::now(),
                event_type: ExecutionEventType::TaskCancelled,
                step_id: None,
                details: serde_json::json!({}),
            });
            Ok(())
        } else {
            Err(AutonomousError::TaskNotFound(task_id.to_string()))
        }
    }

    /// Get learned patterns.
    pub async fn get_patterns(&self) -> Vec<ExecutionPattern> {
        let patterns = self.patterns.read().await;
        patterns.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockProvider;

    #[async_trait]
    impl AutonomousProvider for MockProvider {
        async fn plan(&self, goal: &str, _context: &TaskContext) -> Result<Vec<Step>> {
            Ok(vec![
                Step {
                    id: "step1".to_string(),
                    name: "Analyze".to_string(),
                    description: format!("Analyze: {}", goal),
                    step_type: StepType::Inference {
                        prompt_template: "Analyze {{goal}}".to_string(),
                    },
                    parameters: HashMap::new(),
                    dependencies: vec![],
                    expected_outputs: vec!["analysis".to_string()],
                    max_retries: 3,
                    retry_count: 0,
                    status: TaskStatus::Pending,
                    outputs: HashMap::new(),
                    error: None,
                },
                Step {
                    id: "step2".to_string(),
                    name: "Execute".to_string(),
                    description: "Execute based on analysis".to_string(),
                    step_type: StepType::ToolCall {
                        tool_name: "executor".to_string(),
                    },
                    parameters: HashMap::new(),
                    dependencies: vec!["step1".to_string()],
                    expected_outputs: vec!["result".to_string()],
                    max_retries: 3,
                    retry_count: 0,
                    status: TaskStatus::Pending,
                    outputs: HashMap::new(),
                    error: None,
                },
            ])
        }

        async fn execute_step(
            &self,
            step: &Step,
            _context: &TaskContext,
        ) -> Result<HashMap<String, serde_json::Value>> {
            let mut outputs = HashMap::new();
            outputs.insert(
                "result".to_string(),
                serde_json::json!({
                    "step": step.name,
                    "status": "completed"
                }),
            );
            Ok(outputs)
        }

        async fn determine_recovery(
            &self,
            _step: &Step,
            _error: &str,
            _context: &TaskContext,
        ) -> Result<RecoveryStrategy> {
            Ok(RecoveryStrategy::Retry {
                max_attempts: 3,
                backoff_ms: 100,
            })
        }

        async fn replan(&self, task: &AutonomousTask, _from_step: &str) -> Result<Vec<Step>> {
            Ok(task.steps.clone())
        }
    }

    #[tokio::test]
    async fn test_create_task() {
        let provider = Arc::new(MockProvider);
        let executor = AutonomousExecutor::new(provider);

        let context = TaskContext {
            available_tools: vec!["search".to_string()],
            environment: HashMap::new(),
            constraints: ResourceConstraints::default(),
            preferences: HashMap::new(),
        };

        let task = executor.create_task("Test goal", context).await.unwrap();
        assert_eq!(task.goal, "Test goal");
        assert_eq!(task.status, TaskStatus::Pending);
    }

    #[tokio::test]
    async fn test_execute_task() {
        let provider = Arc::new(MockProvider);
        let executor = AutonomousExecutor::new(provider);

        let context = TaskContext {
            available_tools: vec!["search".to_string()],
            environment: HashMap::new(),
            constraints: ResourceConstraints::default(),
            preferences: HashMap::new(),
        };

        let task = executor
            .create_task("Build a report", context)
            .await
            .unwrap();
        let completed = executor.execute(&task.id).await.unwrap();

        assert_eq!(completed.status, TaskStatus::Completed);
        assert_eq!(completed.steps.len(), 2);
        assert!(completed
            .steps
            .iter()
            .all(|s| s.status == TaskStatus::Completed));
    }

    #[tokio::test]
    async fn test_step_dependencies() {
        let provider = Arc::new(MockProvider);
        let executor = AutonomousExecutor::new(provider);

        let context = TaskContext {
            available_tools: vec![],
            environment: HashMap::new(),
            constraints: ResourceConstraints::default(),
            preferences: HashMap::new(),
        };

        let task = executor
            .create_task("Sequential task", context)
            .await
            .unwrap();
        let completed = executor.execute(&task.id).await.unwrap();

        // Verify step1 completed before step2 (by checking history order)
        let step1_complete = completed.history.iter().position(|e| {
            matches!(e.event_type, ExecutionEventType::StepCompleted)
                && e.step_id == Some("step1".to_string())
        });
        let step2_complete = completed.history.iter().position(|e| {
            matches!(e.event_type, ExecutionEventType::StepCompleted)
                && e.step_id == Some("step2".to_string())
        });

        assert!(step1_complete.unwrap() < step2_complete.unwrap());
    }

    #[tokio::test]
    async fn test_cancel_task() {
        let provider = Arc::new(MockProvider);
        let executor = AutonomousExecutor::new(provider);

        let context = TaskContext {
            available_tools: vec![],
            environment: HashMap::new(),
            constraints: ResourceConstraints::default(),
            preferences: HashMap::new(),
        };

        let task = executor
            .create_task("Cancellable task", context)
            .await
            .unwrap();
        executor.cancel_task(&task.id).await.unwrap();

        let cancelled = executor.get_task(&task.id).await.unwrap();
        assert_eq!(cancelled.status, TaskStatus::Cancelled);
    }

    #[tokio::test]
    async fn test_pattern_learning() {
        let provider = Arc::new(MockProvider);
        let executor = AutonomousExecutor::new(provider);

        let context = TaskContext {
            available_tools: vec![],
            environment: HashMap::new(),
            constraints: ResourceConstraints::default(),
            preferences: HashMap::new(),
        };

        let task = executor.create_task("Pattern task", context).await.unwrap();
        executor.execute(&task.id).await.unwrap();

        let patterns = executor.get_patterns().await;
        assert!(!patterns.is_empty());
        assert_eq!(patterns[0].goal_pattern, "Pattern task");
    }

    #[test]
    fn test_step_types() {
        let inference = StepType::Inference {
            prompt_template: "test".to_string(),
        };
        let tool = StepType::ToolCall {
            tool_name: "test".to_string(),
        };
        let code = StepType::CodeExecution {
            language: "rust".to_string(),
            code: "fn main() {}".to_string(),
        };

        // Ensure serialization works
        let _ = serde_json::to_string(&inference).unwrap();
        let _ = serde_json::to_string(&tool).unwrap();
        let _ = serde_json::to_string(&code).unwrap();
    }
}
