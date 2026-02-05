//! Planning system for multi-step task execution.

use crate::{AgentError, Result};
use drbot_providers::Provider;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{debug, info};

/// A step in the execution plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Step {
    /// Step number.
    pub number: usize,
    /// Description of what this step does.
    pub description: String,
    /// Tool to use (if any).
    pub tool: Option<String>,
    /// Expected output.
    pub expected_output: Option<String>,
    /// Dependencies (step numbers that must complete first).
    pub dependencies: Vec<usize>,
    /// Whether this step is complete.
    pub completed: bool,
    /// Result of this step (if completed).
    pub result: Option<String>,
}

/// An execution plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    /// Plan ID.
    pub id: String,
    /// Original task description.
    pub task: String,
    /// Steps to execute.
    pub steps: Vec<Step>,
    /// Overall goal.
    pub goal: String,
    /// Current step index.
    pub current_step: usize,
    /// Whether the plan is complete.
    pub completed: bool,
}

impl Plan {
    /// Create a new plan.
    pub fn new(task: &str, goal: &str) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            task: task.to_string(),
            steps: Vec::new(),
            goal: goal.to_string(),
            current_step: 0,
            completed: false,
        }
    }

    /// Add a step to the plan.
    pub fn add_step(&mut self, description: &str, tool: Option<&str>) -> &mut Step {
        let number = self.steps.len() + 1;
        self.steps.push(Step {
            number,
            description: description.to_string(),
            tool: tool.map(|s| s.to_string()),
            expected_output: None,
            dependencies: Vec::new(),
            completed: false,
            result: None,
        });
        self.steps.last_mut().unwrap()
    }

    /// Get the next executable step.
    pub fn next_step(&self) -> Option<&Step> {
        self.steps.iter().find(|s| {
            !s.completed
                && s.dependencies.iter().all(|dep| {
                    self.steps
                        .iter()
                        .find(|d| d.number == *dep)
                        .map(|d| d.completed)
                        .unwrap_or(false)
                })
        })
    }

    /// Mark a step as complete.
    pub fn complete_step(&mut self, step_number: usize, result: &str) -> Result<()> {
        if let Some(step) = self.steps.iter_mut().find(|s| s.number == step_number) {
            step.completed = true;
            step.result = Some(result.to_string());

            // Check if all steps are complete
            self.completed = self.steps.iter().all(|s| s.completed);
            Ok(())
        } else {
            Err(AgentError::ExecutionFailed(format!(
                "Step {} not found",
                step_number
            )))
        }
    }

    /// Get progress percentage.
    pub fn progress(&self) -> f32 {
        if self.steps.is_empty() {
            return 0.0;
        }
        let completed = self.steps.iter().filter(|s| s.completed).count();
        completed as f32 / self.steps.len() as f32 * 100.0
    }
}

/// Planner that creates execution plans from tasks.
pub struct Planner {
    provider: Arc<dyn Provider>,
}

impl Planner {
    /// Create a new planner.
    pub fn new(provider: Arc<dyn Provider>) -> Self {
        Self { provider }
    }

    /// Create a plan for a task.
    pub async fn plan(&self, task: &str, available_tools: &[String]) -> Result<Plan> {
        info!("Creating plan for task: {}", task);

        // Build prompt for planning
        let tools_list = available_tools.join(", ");
        let prompt = format!(
            r#"You are a planning assistant. Break down the following task into clear, actionable steps.

Task: {}

Available tools: {}

Respond with a JSON object in this exact format:
{{
    "goal": "Brief description of the overall goal",
    "steps": [
        {{
            "description": "What to do in this step",
            "tool": "tool_name or null if no tool needed",
            "dependencies": [1, 2]  // step numbers this depends on, empty array if none
        }}
    ]
}}

Keep the plan concise and actionable. Only use the available tools listed above."#,
            task, tools_list
        );

        // Call LLM to generate plan
        let messages = vec![drbot_core::message::Message::user(&prompt)];
        let options = drbot_providers::ChatOptions {
            model: None,
            max_tokens: Some(2048),
            temperature: Some(0.3),
            top_p: None,
            stop_sequences: None,
            system_prompt: Some(
                "You are a planning assistant that creates structured execution plans.".to_string(),
            ),
            tools: None,
        };

        let response = self
            .provider
            .chat(&messages, options)
            .await
            .map_err(|e| AgentError::PlanningFailed(e.to_string()))?;

        // Parse the response
        let plan = self.parse_plan_response(task, &response.content)?;

        debug!("Created plan with {} steps", plan.steps.len());
        Ok(plan)
    }

    /// Parse the LLM's plan response.
    fn parse_plan_response(&self, task: &str, response: &str) -> Result<Plan> {
        // Extract JSON from response
        let json_start = response
            .find('{')
            .ok_or_else(|| AgentError::PlanningFailed("No JSON found in response".to_string()))?;
        let json_end = response
            .rfind('}')
            .ok_or_else(|| AgentError::PlanningFailed("Invalid JSON in response".to_string()))?
            + 1;

        let json_str = &response[json_start..json_end];
        let value: serde_json::Value = serde_json::from_str(json_str)
            .map_err(|e| AgentError::PlanningFailed(format!("Failed to parse plan JSON: {}", e)))?;

        let goal = value["goal"]
            .as_str()
            .unwrap_or("Complete the task")
            .to_string();
        let mut plan = Plan::new(task, &goal);

        if let Some(steps) = value["steps"].as_array() {
            for step_value in steps {
                let description = step_value["description"].as_str().unwrap_or("Unknown step");
                let tool = step_value["tool"].as_str().filter(|s| *s != "null");

                let step = plan.add_step(description, tool);

                if let Some(deps) = step_value["dependencies"].as_array() {
                    step.dependencies = deps
                        .iter()
                        .filter_map(|d| d.as_u64().map(|n| n as usize))
                        .collect();
                }
            }
        }

        Ok(plan)
    }

    /// Replan after a step failure.
    pub async fn replan(
        &self,
        plan: &Plan,
        failed_step: usize,
        error: &str,
        available_tools: &[String],
    ) -> Result<Plan> {
        info!(
            "Replanning after failure in step {}: {}",
            failed_step, error
        );

        let completed_summary: Vec<String> = plan
            .steps
            .iter()
            .filter(|s| s.completed)
            .map(|s| {
                format!(
                    "Step {}: {} (Result: {})",
                    s.number,
                    s.description,
                    s.result.as_deref().unwrap_or("done")
                )
            })
            .collect();

        let prompt = format!(
            r#"A task execution plan has failed. Please create a revised plan.

Original task: {}
Original goal: {}

Completed steps:
{}

Failed step {}: {}
Error: {}

Available tools: {}

Create a new plan that continues from the completed steps and works around the failure.
Respond with JSON in the same format as before."#,
            plan.task,
            plan.goal,
            completed_summary.join("\n"),
            failed_step,
            plan.steps
                .get(failed_step - 1)
                .map(|s| s.description.as_str())
                .unwrap_or("unknown"),
            error,
            available_tools.join(", ")
        );

        let messages = vec![drbot_core::message::Message::user(&prompt)];
        let options = drbot_providers::ChatOptions {
            model: None,
            max_tokens: Some(2048),
            temperature: Some(0.3),
            top_p: None,
            stop_sequences: None,
            system_prompt: Some(
                "You are a planning assistant that adapts plans when steps fail.".to_string(),
            ),
            tools: None,
        };

        let response = self
            .provider
            .chat(&messages, options)
            .await
            .map_err(|e| AgentError::PlanningFailed(e.to_string()))?;

        self.parse_plan_response(&plan.task, &response.content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plan_creation() {
        let mut plan = Plan::new("Test task", "Test goal");
        plan.add_step("First step", Some("tool1"));
        plan.add_step("Second step", None);

        assert_eq!(plan.steps.len(), 2);
        assert_eq!(plan.progress(), 0.0);
    }

    #[test]
    fn test_plan_completion() {
        let mut plan = Plan::new("Test task", "Test goal");
        plan.add_step("First step", None);
        plan.add_step("Second step", None);

        plan.complete_step(1, "Done").unwrap();
        assert_eq!(plan.progress(), 50.0);
        assert!(!plan.completed);

        plan.complete_step(2, "Done").unwrap();
        assert_eq!(plan.progress(), 100.0);
        assert!(plan.completed);
    }

    #[test]
    fn test_next_step_with_dependencies() {
        let mut plan = Plan::new("Test task", "Test goal");
        plan.add_step("First step", None);
        let step2 = plan.add_step("Second step", None);
        step2.dependencies = vec![1];

        // Next step should be step 1
        let next = plan.next_step().unwrap();
        assert_eq!(next.number, 1);

        // After completing step 1, next should be step 2
        plan.complete_step(1, "Done").unwrap();
        let next = plan.next_step().unwrap();
        assert_eq!(next.number, 2);
    }
}
