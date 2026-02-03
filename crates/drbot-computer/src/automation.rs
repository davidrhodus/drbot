//! Task automation with checkpoints for human-in-the-loop control.
//!
//! Provides autonomous task execution with safety checkpoints.

use crate::actions::{Action, ActionResult};
use crate::controller::{ComputerController, ExecutionMode};
use crate::{ComputerError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{error, info, warn};
use uuid::Uuid;

/// A checkpoint for human-in-the-loop approval.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    /// Unique checkpoint ID.
    pub id: String,
    /// Description of what's happening.
    pub description: String,
    /// Actions that will be executed after approval.
    pub pending_actions: Vec<Action>,
    /// Screenshot of current state.
    pub screenshot: Option<Vec<u8>>,
    /// Whether this is a critical checkpoint.
    pub critical: bool,
    /// Timestamp when checkpoint was created.
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl Checkpoint {
    /// Create a new checkpoint.
    pub fn new(description: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            description: description.into(),
            pending_actions: Vec::new(),
            screenshot: None,
            critical: false,
            created_at: chrono::Utc::now(),
        }
    }

    /// Add pending actions.
    pub fn with_actions(mut self, actions: Vec<Action>) -> Self {
        self.pending_actions = actions;
        self
    }

    /// Mark as critical.
    pub fn critical(mut self) -> Self {
        self.critical = true;
        self
    }

    /// Add screenshot.
    pub fn with_screenshot(mut self, screenshot: Vec<u8>) -> Self {
        self.screenshot = Some(screenshot);
        self
    }
}

/// User action on a checkpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CheckpointAction {
    /// Approve and continue.
    Approve,
    /// Modify the pending actions.
    Modify { actions: Vec<Action> },
    /// Skip this checkpoint.
    Skip,
    /// Abort the entire task.
    Abort,
    /// Pause the task.
    Pause,
}

/// A task to be executed autonomously.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    /// Task ID.
    pub id: String,
    /// Task name.
    pub name: String,
    /// Task description.
    pub description: Option<String>,
    /// Steps in the task.
    pub steps: Vec<TaskStep>,
    /// Task state.
    pub state: TaskState,
    /// Current step index.
    pub current_step: usize,
    /// Results from completed steps.
    pub results: Vec<StepResult>,
    /// Task metadata.
    pub metadata: HashMap<String, serde_json::Value>,
}

/// A single step in a task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskStep {
    /// Step name.
    pub name: String,
    /// Step description.
    pub description: Option<String>,
    /// Actions to execute.
    pub actions: Vec<Action>,
    /// Whether to checkpoint before this step.
    pub checkpoint: bool,
    /// Condition for executing this step.
    pub condition: Option<String>,
    /// Retry count on failure.
    pub max_retries: u32,
}

impl TaskStep {
    /// Create a new step.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: None,
            actions: Vec::new(),
            checkpoint: false,
            condition: None,
            max_retries: 0,
        }
    }

    /// Add actions.
    pub fn with_actions(mut self, actions: Vec<Action>) -> Self {
        self.actions = actions;
        self
    }

    /// Enable checkpoint before this step.
    pub fn with_checkpoint(mut self) -> Self {
        self.checkpoint = true;
        self
    }

    /// Set description.
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }
}

/// Task state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskState {
    /// Not yet started.
    Pending,
    /// Currently running.
    Running,
    /// Paused at a checkpoint.
    Paused,
    /// Waiting for checkpoint approval.
    WaitingForApproval,
    /// Successfully completed.
    Completed,
    /// Failed with error.
    Failed,
    /// Aborted by user.
    Aborted,
}

/// Result of a step execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepResult {
    /// Step name.
    pub step_name: String,
    /// Whether step succeeded.
    pub success: bool,
    /// Action results.
    pub action_results: Vec<ActionResult>,
    /// Error message if failed.
    pub error: Option<String>,
    /// Duration in milliseconds.
    pub duration_ms: u64,
}

impl Task {
    /// Create a new task.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name: name.into(),
            description: None,
            steps: Vec::new(),
            state: TaskState::Pending,
            current_step: 0,
            results: Vec::new(),
            metadata: HashMap::new(),
        }
    }

    /// Add a step.
    pub fn add_step(mut self, step: TaskStep) -> Self {
        self.steps.push(step);
        self
    }

    /// Set description.
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Add metadata.
    pub fn with_metadata(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.metadata.insert(key.into(), value);
        self
    }

    /// Check if task is complete.
    pub fn is_complete(&self) -> bool {
        matches!(
            self.state,
            TaskState::Completed | TaskState::Failed | TaskState::Aborted
        )
    }

    /// Get progress as percentage.
    pub fn progress(&self) -> f32 {
        if self.steps.is_empty() {
            return 100.0;
        }
        (self.current_step as f32 / self.steps.len() as f32) * 100.0
    }
}

/// Task runner for executing tasks with checkpoints.
pub struct TaskRunner {
    controller: Arc<RwLock<ComputerController>>,
    /// Channel for checkpoint requests.
    checkpoint_tx: mpsc::Sender<Checkpoint>,
    /// Channel for checkpoint responses.
    checkpoint_rx: Arc<RwLock<mpsc::Receiver<CheckpointAction>>>,
    /// Channel for sending checkpoint responses.
    response_tx: mpsc::Sender<CheckpointAction>,
    /// Currently running task.
    current_task: Arc<RwLock<Option<Task>>>,
}

impl TaskRunner {
    /// Create a new task runner.
    pub async fn new() -> Result<Self> {
        let controller = ComputerController::new().await?;
        let (checkpoint_tx, _checkpoint_rx) = mpsc::channel(10);
        let (response_tx, response_rx) = mpsc::channel(10);

        Ok(Self {
            controller: Arc::new(RwLock::new(controller)),
            checkpoint_tx,
            checkpoint_rx: Arc::new(RwLock::new(response_rx)),
            response_tx,
            current_task: Arc::new(RwLock::new(None)),
        })
    }

    /// Create with existing controller.
    pub fn with_controller(controller: ComputerController) -> Self {
        let (checkpoint_tx, _checkpoint_rx) = mpsc::channel(10);
        let (response_tx, response_rx) = mpsc::channel(10);

        Self {
            controller: Arc::new(RwLock::new(controller)),
            checkpoint_tx,
            checkpoint_rx: Arc::new(RwLock::new(response_rx)),
            response_tx,
            current_task: Arc::new(RwLock::new(None)),
        }
    }

    /// Run a task.
    pub async fn run(&self, mut task: Task) -> Result<Task> {
        info!("Starting task: {}", task.name);
        task.state = TaskState::Running;

        // Store current task
        {
            let mut current = self.current_task.write().await;
            *current = Some(task.clone());
        }

        while task.current_step < task.steps.len() && !task.is_complete() {
            let step = &task.steps[task.current_step].clone();

            // Check for checkpoint
            if step.checkpoint {
                info!("Checkpoint at step: {}", step.name);
                let checkpoint = Checkpoint::new(format!("Step: {}", step.name))
                    .with_actions(step.actions.clone());

                task.state = TaskState::WaitingForApproval;

                // Send checkpoint
                if self.checkpoint_tx.send(checkpoint.clone()).await.is_err() {
                    warn!("No checkpoint receiver");
                }

                // Wait for response
                let action = {
                    let mut rx = self.checkpoint_rx.write().await;
                    match tokio::time::timeout(std::time::Duration::from_secs(300), rx.recv()).await
                    {
                        Ok(Some(action)) => action,
                        Ok(None) => CheckpointAction::Abort,
                        Err(_) => {
                            error!("Checkpoint timeout");
                            CheckpointAction::Abort
                        }
                    }
                };

                match action {
                    CheckpointAction::Approve => {
                        task.state = TaskState::Running;
                    }
                    CheckpointAction::Abort => {
                        task.state = TaskState::Aborted;
                        break;
                    }
                    CheckpointAction::Pause => {
                        task.state = TaskState::Paused;
                        break;
                    }
                    CheckpointAction::Skip => {
                        task.current_step += 1;
                        continue;
                    }
                    CheckpointAction::Modify { actions } => {
                        // Execute modified actions
                        let mut controller = self.controller.write().await;
                        for action in actions {
                            controller
                                .execute(action)
                                .with_mode(ExecutionMode::Immediate)
                                .await?;
                        }
                        task.current_step += 1;
                        continue;
                    }
                }
            }

            // Execute step
            let start = std::time::Instant::now();
            let mut action_results = Vec::new();
            let mut step_success = true;
            let mut step_error = None;

            {
                let mut controller = self.controller.write().await;
                for action in &step.actions {
                    match controller
                        .execute(action.clone())
                        .with_mode(ExecutionMode::Immediate)
                        .await
                    {
                        Ok(result) => {
                            if !result.success {
                                step_success = false;
                                step_error = result.error.clone();
                            }
                            action_results.push(result);
                        }
                        Err(e) => {
                            step_success = false;
                            step_error = Some(e.to_string());
                            break;
                        }
                    }
                }
            }

            let step_result = StepResult {
                step_name: step.name.clone(),
                success: step_success,
                action_results,
                error: step_error,
                duration_ms: start.elapsed().as_millis() as u64,
            };

            task.results.push(step_result);

            if !step_success {
                // Handle retries
                let retries_remaining = step.max_retries.saturating_sub(
                    task.results
                        .iter()
                        .filter(|r| r.step_name == step.name && !r.success)
                        .count() as u32,
                );

                if retries_remaining > 0 {
                    warn!(
                        "Step '{}' failed, retrying ({} retries remaining)",
                        step.name, retries_remaining
                    );
                    // Don't increment current_step, will retry on next iteration
                    // Add a small delay before retry
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    continue;
                } else {
                    error!("Step '{}' failed after all retries", step.name);
                    task.state = TaskState::Failed;
                    break;
                }
            }

            task.current_step += 1;
        }

        if task.current_step >= task.steps.len() && task.state == TaskState::Running {
            task.state = TaskState::Completed;
        }

        info!("Task {} finished with state: {:?}", task.name, task.state);

        // Clear current task
        {
            let mut current = self.current_task.write().await;
            *current = None;
        }

        Ok(task)
    }

    /// Send a checkpoint response.
    pub async fn respond_to_checkpoint(&self, action: CheckpointAction) -> Result<()> {
        self.response_tx
            .send(action)
            .await
            .map_err(|_| ComputerError::ActionFailed("Failed to send checkpoint response".into()))
    }

    /// Get current task.
    pub async fn current_task(&self) -> Option<Task> {
        self.current_task.read().await.clone()
    }

    /// Pause current task.
    pub async fn pause(&self) -> Result<()> {
        self.respond_to_checkpoint(CheckpointAction::Pause).await
    }

    /// Resume paused task.
    pub async fn resume(&self) -> Result<()> {
        self.respond_to_checkpoint(CheckpointAction::Approve).await
    }

    /// Abort current task.
    pub async fn abort(&self) -> Result<()> {
        self.respond_to_checkpoint(CheckpointAction::Abort).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_checkpoint_new() {
        let checkpoint = Checkpoint::new("Test checkpoint");
        assert!(!checkpoint.critical);
        assert!(checkpoint.pending_actions.is_empty());
    }

    #[test]
    fn test_task_new() {
        let task = Task::new("Test task")
            .with_description("A test task")
            .add_step(TaskStep::new("Step 1").with_checkpoint());

        assert_eq!(task.name, "Test task");
        assert_eq!(task.steps.len(), 1);
        assert!(task.steps[0].checkpoint);
    }

    #[test]
    fn test_task_progress() {
        let mut task = Task::new("Test")
            .add_step(TaskStep::new("Step 1"))
            .add_step(TaskStep::new("Step 2"))
            .add_step(TaskStep::new("Step 3"))
            .add_step(TaskStep::new("Step 4"));

        assert_eq!(task.progress(), 0.0);

        task.current_step = 2;
        assert_eq!(task.progress(), 50.0);

        task.current_step = 4;
        assert_eq!(task.progress(), 100.0);
    }

    #[tokio::test]
    async fn test_task_runner_new() {
        let runner = TaskRunner::new().await.unwrap();
        assert!(runner.current_task().await.is_none());
    }
}
