//! Plan executor that runs steps and handles failures.

use crate::planner::{Plan, Step};
use crate::tools::AgentTool;
use crate::{AgentError, AgentEvent, Result, ToolCall, ToolResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

/// Result of executing a plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    /// The final plan state.
    pub plan: Plan,
    /// Whether execution succeeded.
    pub success: bool,
    /// Final output or error message.
    pub output: String,
    /// Total steps executed.
    pub steps_executed: usize,
    /// Total steps that failed.
    pub steps_failed: usize,
}

/// Executor configuration.
#[derive(Debug, Clone)]
pub struct ExecutorConfig {
    /// Maximum retries per step.
    pub max_retries: usize,
    /// Whether to continue on step failure.
    pub continue_on_failure: bool,
    /// Timeout per step in seconds.
    pub step_timeout_secs: u64,
}

impl Default for ExecutorConfig {
    fn default() -> Self {
        Self {
            max_retries: 2,
            continue_on_failure: false,
            step_timeout_secs: 120,
        }
    }
}

/// Plan executor.
pub struct Executor {
    config: ExecutorConfig,
    tools: HashMap<String, Arc<dyn AgentTool>>,
}

impl Executor {
    /// Create a new executor.
    pub fn new(config: ExecutorConfig) -> Self {
        Self {
            config,
            tools: HashMap::new(),
        }
    }

    /// Register a tool.
    pub fn register_tool(&mut self, tool: Arc<dyn AgentTool>) {
        self.tools.insert(tool.name().to_string(), tool);
    }

    /// Execute a plan.
    pub async fn execute(&self, plan: &mut Plan) -> Result<ExecutionResult> {
        let (tx, _rx) = mpsc::channel(100);
        self.execute_with_events(plan, tx).await
    }

    /// Execute a plan with event streaming.
    pub async fn execute_with_events(
        &self,
        plan: &mut Plan,
        events: mpsc::Sender<AgentEvent>,
    ) -> Result<ExecutionResult> {
        info!(plan_id = %plan.id, "Starting plan execution");

        let mut steps_executed = 0;
        let mut steps_failed = 0;
        let mut last_result = String::new();

        while let Some(step) = plan.next_step().cloned() {
            debug!(step = step.number, "Executing step: {}", step.description);

            let _ = events
                .send(AgentEvent::Thought {
                    content: format!("Executing step {}: {}", step.number, step.description),
                })
                .await;

            let result = self.execute_step(&step, &events).await;
            steps_executed += 1;

            match result {
                Ok(output) => {
                    plan.complete_step(step.number, &output)?;
                    last_result = output;

                    let _ = events
                        .send(AgentEvent::Thought {
                            content: format!("Step {} completed successfully", step.number),
                        })
                        .await;
                }
                Err(e) => {
                    steps_failed += 1;
                    error!(step = step.number, error = %e, "Step failed");

                    let _ = events
                        .send(AgentEvent::Error {
                            message: format!("Step {} failed: {}", step.number, e),
                        })
                        .await;

                    if !self.config.continue_on_failure {
                        return Ok(ExecutionResult {
                            plan: plan.clone(),
                            success: false,
                            output: format!("Execution failed at step {}: {}", step.number, e),
                            steps_executed,
                            steps_failed,
                        });
                    }

                    // Mark as complete anyway if continuing
                    plan.complete_step(step.number, &format!("Failed: {}", e))?;
                }
            }
        }

        let success = steps_failed == 0;
        info!(plan_id = %plan.id, success, "Plan execution complete");

        Ok(ExecutionResult {
            plan: plan.clone(),
            success,
            output: last_result,
            steps_executed,
            steps_failed,
        })
    }

    /// Execute a single step.
    async fn execute_step(&self, step: &Step, events: &mpsc::Sender<AgentEvent>) -> Result<String> {
        let mut attempts = 0;

        while attempts <= self.config.max_retries {
            attempts += 1;

            match self.try_execute_step(step, events).await {
                Ok(result) => return Ok(result),
                Err(e) if attempts <= self.config.max_retries => {
                    warn!(
                        step = step.number,
                        attempt = attempts,
                        error = %e,
                        "Step failed, retrying"
                    );
                    tokio::time::sleep(tokio::time::Duration::from_millis(500 * attempts as u64))
                        .await;
                }
                Err(e) => return Err(e),
            }
        }

        Err(AgentError::ExecutionFailed(
            "Max retries exceeded".to_string(),
        ))
    }

    /// Try to execute a step once.
    async fn try_execute_step(
        &self,
        step: &Step,
        events: &mpsc::Sender<AgentEvent>,
    ) -> Result<String> {
        // If step has a tool, execute it
        if let Some(tool_name) = &step.tool {
            let tool = self
                .tools
                .get(tool_name)
                .ok_or_else(|| AgentError::ToolNotFound(tool_name.clone()))?;

            let _ = events
                .send(AgentEvent::ToolCall {
                    tool: tool_name.clone(),
                    args: serde_json::json!({ "description": step.description }),
                })
                .await;

            // Build arguments from step description
            // In a real implementation, we'd have the planner provide structured args
            let args = serde_json::json!({
                "input": step.description
            });

            let result = tokio::time::timeout(
                tokio::time::Duration::from_secs(self.config.step_timeout_secs),
                tool.execute(args),
            )
            .await
            .map_err(|_| AgentError::Timeout)?
            .map_err(|e| AgentError::ToolError(e.to_string()))?;

            let _ = events
                .send(AgentEvent::ToolResult {
                    tool: tool_name.clone(),
                    result: result.clone(),
                    is_error: false,
                })
                .await;

            Ok(result)
        } else {
            // No tool - this is a thinking/planning step
            Ok(format!("Completed: {}", step.description))
        }
    }
}

impl Default for Executor {
    fn default() -> Self {
        Self::new(ExecutorConfig::default())
    }
}

/// Execution context for passing state between steps.
#[derive(Debug, Clone, Default)]
pub struct ExecutionContext {
    /// Variables set during execution.
    pub variables: HashMap<String, serde_json::Value>,
    /// Previous step results.
    pub step_results: HashMap<usize, String>,
}

impl ExecutionContext {
    /// Create a new context.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set a variable.
    pub fn set(&mut self, key: &str, value: serde_json::Value) {
        self.variables.insert(key.to_string(), value);
    }

    /// Get a variable.
    pub fn get(&self, key: &str) -> Option<&serde_json::Value> {
        self.variables.get(key)
    }

    /// Record step result.
    pub fn record_result(&mut self, step: usize, result: String) {
        self.step_results.insert(step, result);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_executor_config_default() {
        let config = ExecutorConfig::default();
        assert_eq!(config.max_retries, 2);
        assert!(!config.continue_on_failure);
    }

    #[test]
    fn test_execution_context() {
        let mut ctx = ExecutionContext::new();
        ctx.set("key", serde_json::json!("value"));
        assert_eq!(ctx.get("key"), Some(&serde_json::json!("value")));
    }
}
