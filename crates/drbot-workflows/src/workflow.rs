//! Workflow definition and management.

use crate::action::Action;
use crate::trigger::Trigger;
use crate::{Result, WorkflowError};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Workflow status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowStatus {
    /// Workflow is active and can be triggered.
    Active,
    /// Workflow is paused.
    Paused,
    /// Workflow is disabled.
    Disabled,
    /// Workflow is currently running.
    Running,
    /// Workflow completed.
    Completed,
    /// Workflow failed.
    Failed,
}

/// A workflow definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workflow {
    /// Workflow ID.
    pub id: Uuid,
    /// Workflow name.
    pub name: String,
    /// Description.
    pub description: Option<String>,
    /// Triggers that start the workflow.
    pub triggers: Vec<Trigger>,
    /// Actions to execute.
    pub actions: Vec<Action>,
    /// Current status.
    pub status: WorkflowStatus,
    /// Created timestamp.
    pub created_at: DateTime<Utc>,
    /// Updated timestamp.
    pub updated_at: DateTime<Utc>,
    /// Owner user ID.
    pub owner_id: Option<String>,
    /// Tags for organization.
    pub tags: Vec<String>,
    /// Maximum retries on failure.
    pub max_retries: u32,
    /// Timeout in seconds.
    pub timeout_secs: u64,
}

impl Workflow {
    /// Create a new workflow.
    pub fn new(name: &str) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            name: name.to_string(),
            description: None,
            triggers: Vec::new(),
            actions: Vec::new(),
            status: WorkflowStatus::Active,
            created_at: now,
            updated_at: now,
            owner_id: None,
            tags: Vec::new(),
            max_retries: 3,
            timeout_secs: 300,
        }
    }

    /// Check if workflow can be triggered.
    pub fn can_run(&self) -> bool {
        matches!(self.status, WorkflowStatus::Active)
    }

    /// Add a trigger.
    pub fn add_trigger(&mut self, trigger: Trigger) {
        self.triggers.push(trigger);
        self.updated_at = Utc::now();
    }

    /// Add an action.
    pub fn add_action(&mut self, action: Action) {
        self.actions.push(action);
        self.updated_at = Utc::now();
    }

    /// Pause the workflow.
    pub fn pause(&mut self) {
        self.status = WorkflowStatus::Paused;
        self.updated_at = Utc::now();
    }

    /// Resume the workflow.
    pub fn resume(&mut self) {
        if self.status == WorkflowStatus::Paused {
            self.status = WorkflowStatus::Active;
            self.updated_at = Utc::now();
        }
    }

    /// Disable the workflow.
    pub fn disable(&mut self) {
        self.status = WorkflowStatus::Disabled;
        self.updated_at = Utc::now();
    }
}

/// Builder for creating workflows.
pub struct WorkflowBuilder {
    workflow: Workflow,
}

impl WorkflowBuilder {
    /// Create a new builder.
    pub fn new(name: &str) -> Self {
        Self {
            workflow: Workflow::new(name),
        }
    }

    /// Set description.
    pub fn description(mut self, description: &str) -> Self {
        self.workflow.description = Some(description.to_string());
        self
    }

    /// Add a trigger.
    pub fn trigger(mut self, trigger: Trigger) -> Self {
        self.workflow.triggers.push(trigger);
        self
    }

    /// Add an action.
    pub fn action(mut self, action: Action) -> Self {
        self.workflow.actions.push(action);
        self
    }

    /// Set owner.
    pub fn owner(mut self, owner_id: &str) -> Self {
        self.workflow.owner_id = Some(owner_id.to_string());
        self
    }

    /// Add a tag.
    pub fn tag(mut self, tag: &str) -> Self {
        self.workflow.tags.push(tag.to_string());
        self
    }

    /// Set max retries.
    pub fn max_retries(mut self, retries: u32) -> Self {
        self.workflow.max_retries = retries;
        self
    }

    /// Set timeout.
    pub fn timeout_secs(mut self, secs: u64) -> Self {
        self.workflow.timeout_secs = secs;
        self
    }

    /// Build the workflow.
    pub fn build(self) -> Workflow {
        self.workflow
    }
}

/// Workflow run history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowRun {
    /// Run ID.
    pub id: Uuid,
    /// Workflow ID.
    pub workflow_id: Uuid,
    /// Start time.
    pub started_at: DateTime<Utc>,
    /// End time.
    pub ended_at: Option<DateTime<Utc>>,
    /// Status.
    pub status: WorkflowStatus,
    /// Trigger that started this run.
    pub trigger_id: Option<Uuid>,
    /// Action results.
    pub action_results: Vec<ActionRunResult>,
    /// Error message (if failed).
    pub error: Option<String>,
}

/// Result of running an action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionRunResult {
    /// Action index.
    pub action_index: usize,
    /// Whether it succeeded.
    pub success: bool,
    /// Output.
    pub output: Option<String>,
    /// Error.
    pub error: Option<String>,
    /// Duration in milliseconds.
    pub duration_ms: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workflow_creation() {
        let workflow = Workflow::new("Test Workflow");
        assert_eq!(workflow.name, "Test Workflow");
        assert!(workflow.can_run());
    }

    #[test]
    fn test_workflow_builder() {
        let workflow = WorkflowBuilder::new("Test")
            .description("A test workflow")
            .tag("test")
            .max_retries(5)
            .build();

        assert_eq!(workflow.name, "Test");
        assert_eq!(workflow.description, Some("A test workflow".to_string()));
        assert!(workflow.tags.contains(&"test".to_string()));
        assert_eq!(workflow.max_retries, 5);
    }

    #[test]
    fn test_workflow_pause_resume() {
        let mut workflow = Workflow::new("Test");
        assert!(workflow.can_run());

        workflow.pause();
        assert!(!workflow.can_run());

        workflow.resume();
        assert!(workflow.can_run());
    }
}
