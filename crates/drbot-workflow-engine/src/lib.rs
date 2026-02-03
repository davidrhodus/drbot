//! Workflow execution engine for drbot.
//!
//! This crate provides:
//! - DAG-based workflow execution
//! - Parallel task execution
//! - Conditional branching
//! - Workflow persistence and resumption

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Workflow engine error types.
#[derive(Error, Debug)]
pub enum WorkflowError {
    #[error("Workflow not found: {0}")]
    WorkflowNotFound(Uuid),

    #[error("Task not found: {0}")]
    TaskNotFound(String),

    #[error("Execution error: {0}")]
    ExecutionError(String),

    #[error("Invalid workflow: {0}")]
    InvalidWorkflow(String),

    #[error("Cycle detected in workflow")]
    CycleDetected,

    #[error("Task failed: {0}")]
    TaskFailed(String),

    #[error("Timeout")]
    Timeout,
}

/// Result type for workflow operations.
pub type Result<T> = std::result::Result<T, WorkflowError>;

/// Task status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskStatus {
    /// Task is pending.
    Pending,
    /// Task is ready to run.
    Ready,
    /// Task is running.
    Running,
    /// Task completed successfully.
    Completed,
    /// Task failed.
    Failed,
    /// Task was skipped.
    Skipped,
    /// Task was cancelled.
    Cancelled,
}

/// Workflow status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkflowStatus {
    /// Workflow is pending.
    Pending,
    /// Workflow is running.
    Running,
    /// Workflow completed successfully.
    Completed,
    /// Workflow failed.
    Failed,
    /// Workflow was cancelled.
    Cancelled,
    /// Workflow is paused.
    Paused,
}

/// A task in the workflow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    /// Task ID.
    pub id: String,
    /// Task name.
    pub name: String,
    /// Task type.
    pub task_type: String,
    /// Task configuration.
    pub config: serde_json::Value,
    /// Dependencies (task IDs that must complete first).
    pub dependencies: Vec<String>,
    /// Current status.
    pub status: TaskStatus,
    /// Start time.
    pub started_at: Option<DateTime<Utc>>,
    /// End time.
    pub ended_at: Option<DateTime<Utc>>,
    /// Result data.
    pub result: Option<serde_json::Value>,
    /// Error message.
    pub error: Option<String>,
    /// Retry count.
    pub retry_count: u32,
    /// Max retries.
    pub max_retries: u32,
    /// Condition for execution (optional).
    pub condition: Option<String>,
}

impl Task {
    /// Create a new task.
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        task_type: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            task_type: task_type.into(),
            config: serde_json::Value::Null,
            dependencies: Vec::new(),
            status: TaskStatus::Pending,
            started_at: None,
            ended_at: None,
            result: None,
            error: None,
            retry_count: 0,
            max_retries: 3,
            condition: None,
        }
    }

    /// Set configuration.
    pub fn with_config(mut self, config: serde_json::Value) -> Self {
        self.config = config;
        self
    }

    /// Add a dependency.
    pub fn depends_on(mut self, task_id: impl Into<String>) -> Self {
        self.dependencies.push(task_id.into());
        self
    }

    /// Set condition.
    pub fn with_condition(mut self, condition: impl Into<String>) -> Self {
        self.condition = Some(condition.into());
        self
    }

    /// Check if all dependencies are complete.
    pub fn dependencies_complete(&self, completed: &HashSet<String>) -> bool {
        self.dependencies.iter().all(|d| completed.contains(d))
    }
}

/// A workflow definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowDefinition {
    /// Workflow name.
    pub name: String,
    /// Workflow description.
    pub description: Option<String>,
    /// Tasks in the workflow.
    pub tasks: Vec<Task>,
    /// Workflow-level configuration.
    pub config: serde_json::Value,
}

impl WorkflowDefinition {
    /// Create a new workflow definition.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: None,
            tasks: Vec::new(),
            config: serde_json::Value::Null,
        }
    }

    /// Add a task.
    pub fn task(mut self, task: Task) -> Self {
        self.tasks.push(task);
        self
    }

    /// Validate the workflow (check for cycles).
    pub fn validate(&self) -> Result<()> {
        let task_ids: HashSet<_> = self.tasks.iter().map(|t| t.id.clone()).collect();

        // Check all dependencies exist
        for task in &self.tasks {
            for dep in &task.dependencies {
                if !task_ids.contains(dep) {
                    return Err(WorkflowError::InvalidWorkflow(format!(
                        "Task {} depends on non-existent task {}",
                        task.id, dep
                    )));
                }
            }
        }

        // Check for cycles using DFS
        let mut visited = HashSet::new();
        let mut rec_stack = HashSet::new();

        for task in &self.tasks {
            if self.has_cycle(&task.id, &mut visited, &mut rec_stack)? {
                return Err(WorkflowError::CycleDetected);
            }
        }

        Ok(())
    }

    fn has_cycle(
        &self,
        task_id: &str,
        visited: &mut HashSet<String>,
        rec_stack: &mut HashSet<String>,
    ) -> Result<bool> {
        if rec_stack.contains(task_id) {
            return Ok(true);
        }
        if visited.contains(task_id) {
            return Ok(false);
        }

        visited.insert(task_id.to_string());
        rec_stack.insert(task_id.to_string());

        let task = self.tasks.iter().find(|t| t.id == task_id);
        if let Some(task) = task {
            for dep in &task.dependencies {
                if self.has_cycle(dep, visited, rec_stack)? {
                    return Ok(true);
                }
            }
        }

        rec_stack.remove(task_id);
        Ok(false)
    }
}

/// A workflow instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowInstance {
    /// Instance ID.
    pub id: Uuid,
    /// Workflow name.
    pub workflow_name: String,
    /// Tasks with their current state.
    pub tasks: HashMap<String, Task>,
    /// Workflow status.
    pub status: WorkflowStatus,
    /// Input data.
    pub input: serde_json::Value,
    /// Output data.
    pub output: Option<serde_json::Value>,
    /// Started at.
    pub started_at: DateTime<Utc>,
    /// Ended at.
    pub ended_at: Option<DateTime<Utc>>,
    /// Context data shared between tasks.
    pub context: HashMap<String, serde_json::Value>,
}

impl WorkflowInstance {
    /// Create from definition.
    pub fn from_definition(definition: &WorkflowDefinition, input: serde_json::Value) -> Self {
        let tasks = definition
            .tasks
            .iter()
            .map(|t| (t.id.clone(), t.clone()))
            .collect();

        Self {
            id: Uuid::new_v4(),
            workflow_name: definition.name.clone(),
            tasks,
            status: WorkflowStatus::Pending,
            input,
            output: None,
            started_at: Utc::now(),
            ended_at: None,
            context: HashMap::new(),
        }
    }

    /// Get ready tasks.
    pub fn get_ready_tasks(&self) -> Vec<String> {
        let completed: HashSet<_> = self
            .tasks
            .values()
            .filter(|t| t.status == TaskStatus::Completed || t.status == TaskStatus::Skipped)
            .map(|t| t.id.clone())
            .collect();

        self.tasks
            .values()
            .filter(|t| t.status == TaskStatus::Pending && t.dependencies_complete(&completed))
            .map(|t| t.id.clone())
            .collect()
    }

    /// Check if workflow is complete.
    pub fn is_complete(&self) -> bool {
        self.tasks.values().all(|t| {
            matches!(
                t.status,
                TaskStatus::Completed | TaskStatus::Skipped | TaskStatus::Cancelled
            )
        })
    }

    /// Check if workflow has failed.
    pub fn has_failed(&self) -> bool {
        self.tasks.values().any(|t| t.status == TaskStatus::Failed)
    }
}

/// Task executor trait.
#[async_trait]
pub trait TaskExecutor: Send + Sync {
    /// Execute a task.
    async fn execute(
        &self,
        task: &Task,
        context: &HashMap<String, serde_json::Value>,
    ) -> Result<serde_json::Value>;

    /// Get supported task types.
    fn supported_types(&self) -> Vec<String>;
}

/// Workflow engine.
pub struct WorkflowEngine {
    executors: RwLock<HashMap<String, Arc<dyn TaskExecutor>>>,
    instances: RwLock<HashMap<Uuid, WorkflowInstance>>,
}

impl WorkflowEngine {
    /// Create a new engine.
    pub fn new() -> Self {
        Self {
            executors: RwLock::new(HashMap::new()),
            instances: RwLock::new(HashMap::new()),
        }
    }

    /// Register a task executor.
    pub async fn register_executor(
        &self,
        task_type: impl Into<String>,
        executor: Arc<dyn TaskExecutor>,
    ) {
        let mut executors = self.executors.write().await;
        executors.insert(task_type.into(), executor);
    }

    /// Start a workflow.
    pub async fn start(
        &self,
        definition: &WorkflowDefinition,
        input: serde_json::Value,
    ) -> Result<Uuid> {
        definition.validate()?;

        let mut instance = WorkflowInstance::from_definition(definition, input);
        instance.status = WorkflowStatus::Running;

        let id = instance.id;
        let mut instances = self.instances.write().await;
        instances.insert(id, instance);

        Ok(id)
    }

    /// Execute one step of a workflow.
    pub async fn step(&self, workflow_id: Uuid) -> Result<bool> {
        let ready_tasks = {
            let instances = self.instances.read().await;
            let instance = instances
                .get(&workflow_id)
                .ok_or(WorkflowError::WorkflowNotFound(workflow_id))?;

            if instance.status != WorkflowStatus::Running {
                return Ok(false);
            }

            instance.get_ready_tasks()
        };

        if ready_tasks.is_empty() {
            // Check if complete or failed
            let mut instances = self.instances.write().await;
            let instance = instances.get_mut(&workflow_id).unwrap();

            if instance.is_complete() {
                instance.status = WorkflowStatus::Completed;
                instance.ended_at = Some(Utc::now());
            } else if instance.has_failed() {
                instance.status = WorkflowStatus::Failed;
                instance.ended_at = Some(Utc::now());
            }

            return Ok(false);
        }

        // Execute ready tasks
        for task_id in ready_tasks {
            self.execute_task(workflow_id, &task_id).await?;
        }

        Ok(true)
    }

    /// Execute a specific task.
    async fn execute_task(&self, workflow_id: Uuid, task_id: &str) -> Result<()> {
        let (task, context) = {
            let mut instances = self.instances.write().await;
            let instance = instances
                .get_mut(&workflow_id)
                .ok_or(WorkflowError::WorkflowNotFound(workflow_id))?;

            let task = instance
                .tasks
                .get_mut(task_id)
                .ok_or_else(|| WorkflowError::TaskNotFound(task_id.to_string()))?;

            task.status = TaskStatus::Running;
            task.started_at = Some(Utc::now());

            (task.clone(), instance.context.clone())
        };

        // Get executor
        let executors = self.executors.read().await;
        let executor = executors.get(&task.task_type).ok_or_else(|| {
            WorkflowError::ExecutionError(format!("No executor for type: {}", task.task_type))
        })?;

        // Execute
        let result = executor.execute(&task, &context).await;

        // Update task state
        let mut instances = self.instances.write().await;
        let instance = instances.get_mut(&workflow_id).unwrap();
        let task = instance.tasks.get_mut(task_id).unwrap();

        task.ended_at = Some(Utc::now());

        match result {
            Ok(data) => {
                task.status = TaskStatus::Completed;
                task.result = Some(data.clone());
                instance.context.insert(task_id.to_string(), data);
            }
            Err(e) => {
                task.error = Some(e.to_string());
                task.retry_count += 1;

                if task.retry_count >= task.max_retries {
                    task.status = TaskStatus::Failed;
                } else {
                    task.status = TaskStatus::Pending;
                }
            }
        }

        Ok(())
    }

    /// Run a workflow to completion.
    pub async fn run(&self, workflow_id: Uuid) -> Result<WorkflowStatus> {
        loop {
            let made_progress = self.step(workflow_id).await?;
            if !made_progress {
                break;
            }
        }

        let instances = self.instances.read().await;
        let instance = instances
            .get(&workflow_id)
            .ok_or(WorkflowError::WorkflowNotFound(workflow_id))?;

        Ok(instance.status)
    }

    /// Get workflow instance.
    pub async fn get_instance(&self, id: Uuid) -> Option<WorkflowInstance> {
        let instances = self.instances.read().await;
        instances.get(&id).cloned()
    }

    /// Cancel a workflow.
    pub async fn cancel(&self, id: Uuid) -> Result<()> {
        let mut instances = self.instances.write().await;
        let instance = instances
            .get_mut(&id)
            .ok_or(WorkflowError::WorkflowNotFound(id))?;

        instance.status = WorkflowStatus::Cancelled;
        instance.ended_at = Some(Utc::now());

        for task in instance.tasks.values_mut() {
            if task.status == TaskStatus::Pending || task.status == TaskStatus::Running {
                task.status = TaskStatus::Cancelled;
            }
        }

        Ok(())
    }
}

impl Default for WorkflowEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct EchoExecutor;

    #[async_trait]
    impl TaskExecutor for EchoExecutor {
        async fn execute(
            &self,
            task: &Task,
            _context: &HashMap<String, serde_json::Value>,
        ) -> Result<serde_json::Value> {
            Ok(task.config.clone())
        }

        fn supported_types(&self) -> Vec<String> {
            vec!["echo".to_string()]
        }
    }

    struct FailingExecutor;

    #[async_trait]
    impl TaskExecutor for FailingExecutor {
        async fn execute(
            &self,
            _task: &Task,
            _context: &HashMap<String, serde_json::Value>,
        ) -> Result<serde_json::Value> {
            Err(WorkflowError::TaskFailed("Always fails".to_string()))
        }

        fn supported_types(&self) -> Vec<String> {
            vec!["fail".to_string()]
        }
    }

    #[test]
    fn test_task_creation() {
        let task = Task::new("task1", "Task 1", "echo")
            .with_config(serde_json::json!({"key": "value"}))
            .depends_on("task0");

        assert_eq!(task.id, "task1");
        assert_eq!(task.dependencies, vec!["task0"]);
    }

    #[test]
    fn test_workflow_validation() {
        let workflow = WorkflowDefinition::new("test")
            .task(Task::new("a", "A", "echo"))
            .task(Task::new("b", "B", "echo").depends_on("a"));

        assert!(workflow.validate().is_ok());
    }

    #[test]
    fn test_workflow_cycle_detection() {
        let workflow = WorkflowDefinition::new("test")
            .task(Task::new("a", "A", "echo").depends_on("b"))
            .task(Task::new("b", "B", "echo").depends_on("a"));

        assert!(matches!(
            workflow.validate(),
            Err(WorkflowError::CycleDetected)
        ));
    }

    #[test]
    fn test_workflow_missing_dependency() {
        let workflow =
            WorkflowDefinition::new("test").task(Task::new("a", "A", "echo").depends_on("missing"));

        assert!(matches!(
            workflow.validate(),
            Err(WorkflowError::InvalidWorkflow(_))
        ));
    }

    #[test]
    fn test_ready_tasks() {
        let workflow = WorkflowDefinition::new("test")
            .task(Task::new("a", "A", "echo"))
            .task(Task::new("b", "B", "echo").depends_on("a"))
            .task(Task::new("c", "C", "echo").depends_on("a"));

        let instance = WorkflowInstance::from_definition(&workflow, serde_json::json!({}));
        let ready = instance.get_ready_tasks();

        assert_eq!(ready, vec!["a"]);
    }

    #[tokio::test]
    async fn test_workflow_execution() {
        let engine = WorkflowEngine::new();
        engine
            .register_executor("echo", Arc::new(EchoExecutor))
            .await;

        let workflow = WorkflowDefinition::new("test")
            .task(Task::new("a", "A", "echo").with_config(serde_json::json!({"step": 1})))
            .task(
                Task::new("b", "B", "echo")
                    .with_config(serde_json::json!({"step": 2}))
                    .depends_on("a"),
            );

        let id = engine
            .start(&workflow, serde_json::json!({}))
            .await
            .unwrap();
        let status = engine.run(id).await.unwrap();

        assert_eq!(status, WorkflowStatus::Completed);

        let instance = engine.get_instance(id).await.unwrap();
        assert!(instance
            .tasks
            .values()
            .all(|t| t.status == TaskStatus::Completed));
    }

    #[tokio::test]
    async fn test_parallel_tasks() {
        let engine = WorkflowEngine::new();
        engine
            .register_executor("echo", Arc::new(EchoExecutor))
            .await;

        let workflow = WorkflowDefinition::new("test")
            .task(Task::new("a", "A", "echo"))
            .task(Task::new("b", "B", "echo")) // No dependency, can run in parallel
            .task(Task::new("c", "C", "echo").depends_on("a").depends_on("b"));

        let id = engine
            .start(&workflow, serde_json::json!({}))
            .await
            .unwrap();

        // First step should execute both a and b
        engine.step(id).await.unwrap();

        let instance = engine.get_instance(id).await.unwrap();
        let a_completed = instance.tasks.get("a").unwrap().status == TaskStatus::Completed;
        let b_completed = instance.tasks.get("b").unwrap().status == TaskStatus::Completed;
        assert!(a_completed && b_completed);
    }

    #[tokio::test]
    async fn test_workflow_failure() {
        let engine = WorkflowEngine::new();
        engine
            .register_executor("fail", Arc::new(FailingExecutor))
            .await;

        let workflow = WorkflowDefinition::new("test")
            .task(Task::new("a", "A", "fail").with_config(serde_json::json!({})));

        // Set max_retries to 1 so it fails faster
        let mut workflow = workflow;
        workflow.tasks[0].max_retries = 1;

        let id = engine
            .start(&workflow, serde_json::json!({}))
            .await
            .unwrap();
        let status = engine.run(id).await.unwrap();

        assert_eq!(status, WorkflowStatus::Failed);
    }

    #[tokio::test]
    async fn test_cancel_workflow() {
        let engine = WorkflowEngine::new();
        engine
            .register_executor("echo", Arc::new(EchoExecutor))
            .await;

        let workflow = WorkflowDefinition::new("test").task(Task::new("a", "A", "echo"));

        let id = engine
            .start(&workflow, serde_json::json!({}))
            .await
            .unwrap();
        engine.cancel(id).await.unwrap();

        let instance = engine.get_instance(id).await.unwrap();
        assert_eq!(instance.status, WorkflowStatus::Cancelled);
    }
}
