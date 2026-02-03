//! Workflow automation for drbot.
//!
//! Provides workflow definition, execution, and management.
//!
//! # Features
//!
//! - Declarative workflow definition
//! - Conditional branching and loops
//! - Parallel execution
//! - State management with variables
//! - Agent integration
//!
//! # Example
//!
//! ```rust,no_run
//! use drbot_workflows::{WorkflowBuilder, Trigger};
//!
//! let workflow = WorkflowBuilder::new("Daily Report")
//!     .description("Generate and send daily report")
//!     .trigger(Trigger::schedule("daily-trigger", "0 9 * * *"))
//!     .build();
//! ```

mod action;
mod executor;
mod nodes;
mod trigger;
mod workflow;

pub use action::{Action, ActionResult, ActionType};
pub use executor::{ExecutionContext, WorkflowExecutor};
pub use nodes::{
    ActionNode, AgentNode, CompareOp, Condition, ConditionNode, JoinMode, LoopNode, LoopType,
    ParallelNode, SubWorkflowNode, Transform, TransformNode, WaitFor, WaitNode, WorkflowContext,
    WorkflowNode,
};
pub use trigger::{Trigger, TriggerCondition, TriggerType};
pub use workflow::{ActionRunResult, Workflow, WorkflowBuilder, WorkflowRun, WorkflowStatus};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Workflow result.
pub type Result<T> = std::result::Result<T, WorkflowError>;

/// Workflow errors.
#[derive(Debug, thiserror::Error)]
pub enum WorkflowError {
    #[error("Workflow not found: {0}")]
    NotFound(Uuid),
    #[error("Invalid workflow definition: {0}")]
    InvalidDefinition(String),
    #[error("Execution failed: {0}")]
    ExecutionFailed(String),
    #[error("Trigger error: {0}")]
    TriggerError(String),
    #[error("Action error: {0}")]
    ActionError(String),
}

/// Workflow configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowConfig {
    /// Maximum concurrent workflows.
    pub max_concurrent: usize,
    /// Default timeout in seconds.
    pub default_timeout_secs: u64,
    /// Whether to persist workflow state.
    pub persist_state: bool,
}

impl Default for WorkflowConfig {
    fn default() -> Self {
        Self {
            max_concurrent: 10,
            default_timeout_secs: 300,
            persist_state: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workflow_config_default() {
        let config = WorkflowConfig::default();
        assert_eq!(config.max_concurrent, 10);
    }
}
