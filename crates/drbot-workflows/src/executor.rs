//! Workflow executor.

use crate::action::{Action, ActionResult, ActionType};
use crate::workflow::{ActionRunResult, Workflow, WorkflowRun, WorkflowStatus};
use crate::{Result, WorkflowError};
use chrono::Utc;
use drbot_core::message::Message;
use drbot_openai::OpenAIProvider;
use drbot_providers::{ChatOptions, Provider};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::process::Command;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};
use uuid::Uuid;

/// Execution context for workflow runs.
#[derive(Debug, Clone, Default)]
pub struct ExecutionContext {
    /// Variables available during execution.
    pub variables: HashMap<String, serde_json::Value>,
    /// Trigger data.
    pub trigger_data: HashMap<String, serde_json::Value>,
    /// Previous action results.
    pub action_results: Vec<ActionResult>,
}

impl ExecutionContext {
    /// Create a new context.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set a variable.
    pub fn set_var(&mut self, name: &str, value: serde_json::Value) {
        self.variables.insert(name.to_string(), value);
    }

    /// Get a variable.
    pub fn get_var(&self, name: &str) -> Option<&serde_json::Value> {
        self.variables.get(name)
    }

    /// Get last action result.
    pub fn last_result(&self) -> Option<&ActionResult> {
        self.action_results.last()
    }

    /// Add action result.
    pub fn add_result(&mut self, result: ActionResult) {
        self.action_results.push(result);
    }
}

/// Workflow executor.
pub struct WorkflowExecutor {
    /// Running workflows.
    running: Arc<RwLock<HashMap<Uuid, WorkflowRun>>>,
    /// Maximum concurrent workflows.
    max_concurrent: usize,
}

impl WorkflowExecutor {
    /// Create a new executor.
    pub fn new(max_concurrent: usize) -> Self {
        Self {
            running: Arc::new(RwLock::new(HashMap::new())),
            max_concurrent,
        }
    }

    /// Execute a workflow.
    pub async fn execute(
        &self,
        workflow: &Workflow,
        mut context: ExecutionContext,
    ) -> Result<WorkflowRun> {
        // Check if we can start
        {
            let running = self.running.read().await;
            if running.len() >= self.max_concurrent {
                return Err(WorkflowError::ExecutionFailed(
                    "Max concurrent workflows reached".to_string(),
                ));
            }
        }

        info!(workflow_id = %workflow.id, "Starting workflow execution");

        let run_id = Uuid::new_v4();
        let mut run = WorkflowRun {
            id: run_id,
            workflow_id: workflow.id,
            started_at: Utc::now(),
            ended_at: None,
            status: WorkflowStatus::Running,
            trigger_id: None,
            action_results: Vec::new(),
            error: None,
        };

        // Register as running
        {
            let mut running = self.running.write().await;
            running.insert(run_id, run.clone());
        }

        // Execute actions
        let result = self
            .execute_actions(&workflow.actions, &mut context, &mut run)
            .await;

        // Update run status
        run.ended_at = Some(Utc::now());
        run.status = match result {
            Ok(_) => WorkflowStatus::Completed,
            Err(ref e) => {
                run.error = Some(e.to_string());
                WorkflowStatus::Failed
            }
        };

        // Remove from running
        {
            let mut running = self.running.write().await;
            running.remove(&run_id);
        }

        info!(
            workflow_id = %workflow.id,
            run_id = %run_id,
            status = ?run.status,
            "Workflow execution complete"
        );

        Ok(run)
    }

    /// Execute a list of actions.
    async fn execute_actions(
        &self,
        actions: &[Action],
        context: &mut ExecutionContext,
        run: &mut WorkflowRun,
    ) -> Result<()> {
        for (index, action) in actions.iter().enumerate() {
            debug!(action_id = %action.id, "Executing action: {}", action.name);

            let start = std::time::Instant::now();
            let result = self.execute_action(action, context).await;
            let duration_ms = start.elapsed().as_millis() as u64;

            let action_result = match result {
                Ok(output) => ActionRunResult {
                    action_index: index,
                    success: true,
                    output: Some(output.to_string()),
                    error: None,
                    duration_ms,
                },
                Err(e) => {
                    let error_msg = e.to_string();
                    if !action.continue_on_error {
                        run.action_results.push(ActionRunResult {
                            action_index: index,
                            success: false,
                            output: None,
                            error: Some(error_msg.clone()),
                            duration_ms,
                        });
                        return Err(WorkflowError::ExecutionFailed(error_msg));
                    }

                    warn!(action_id = %action.id, "Action failed but continuing: {}", error_msg);
                    ActionRunResult {
                        action_index: index,
                        success: false,
                        output: None,
                        error: Some(error_msg),
                        duration_ms,
                    }
                }
            };

            run.action_results.push(action_result);
        }

        Ok(())
    }

    /// Execute a single action.
    async fn execute_action(
        &self,
        action: &Action,
        context: &mut ExecutionContext,
    ) -> Result<serde_json::Value> {
        let mut last_error = None;
        let mut attempts = 0;

        while attempts <= action.retries {
            attempts += 1;

            let start = std::time::Instant::now();
            match self.try_execute_action(action, context).await {
                Ok(result) => {
                    context.add_result(ActionResult::success(
                        action.id,
                        result.clone(),
                        start.elapsed().as_millis() as u64,
                    ));
                    return Ok(result);
                }
                Err(e) => {
                    last_error = Some(e);
                    if attempts <= action.retries {
                        debug!(
                            action_id = %action.id,
                            attempt = attempts,
                            "Action failed, retrying..."
                        );
                        tokio::time::sleep(tokio::time::Duration::from_millis(
                            500 * attempts as u64,
                        ))
                        .await;
                    }
                }
            }
        }

        Err(last_error.unwrap())
    }

    /// Try to execute an action once.
    async fn try_execute_action(
        &self,
        action: &Action,
        context: &mut ExecutionContext,
    ) -> Result<serde_json::Value> {
        match action.action_type {
            ActionType::SendMessage => {
                let channel = action
                    .config
                    .channel
                    .as_ref()
                    .ok_or_else(|| WorkflowError::ActionError("Missing channel".to_string()))?;
                let message = action
                    .config
                    .message
                    .as_ref()
                    .ok_or_else(|| WorkflowError::ActionError("Missing message".to_string()))?;

                // Interpolate variables in message
                let message = self.interpolate(message, context);

                // In real implementation, send via channel
                info!(channel = %channel, "Sending message: {}", message);

                Ok(serde_json::json!({
                    "sent": true,
                    "channel": channel,
                    "message": message
                }))
            }

            ActionType::HttpRequest => {
                let method = action
                    .config
                    .method
                    .as_ref()
                    .ok_or_else(|| WorkflowError::ActionError("Missing method".to_string()))?;
                let url = action
                    .config
                    .url
                    .as_ref()
                    .ok_or_else(|| WorkflowError::ActionError("Missing URL".to_string()))?;

                let client = reqwest::Client::new();
                let mut request = match method.to_uppercase().as_str() {
                    "GET" => client.get(url),
                    "POST" => client.post(url),
                    "PUT" => client.put(url),
                    "DELETE" => client.delete(url),
                    _ => {
                        return Err(WorkflowError::ActionError(format!(
                            "Unknown method: {}",
                            method
                        )))
                    }
                };

                if let Some(body) = &action.config.body {
                    request = request.body(body.clone());
                }

                let response = request
                    .send()
                    .await
                    .map_err(|e| WorkflowError::ActionError(e.to_string()))?;

                let status = response.status().as_u16();
                let body = response
                    .text()
                    .await
                    .map_err(|e| WorkflowError::ActionError(e.to_string()))?;

                Ok(serde_json::json!({
                    "status": status,
                    "body": body
                }))
            }

            ActionType::Wait => {
                let duration = action.config.duration_secs.unwrap_or(1);
                tokio::time::sleep(tokio::time::Duration::from_secs(duration)).await;
                Ok(serde_json::json!({ "waited_secs": duration }))
            }

            ActionType::SetVariable => {
                let name = action.config.variable.as_ref().ok_or_else(|| {
                    WorkflowError::ActionError("Missing variable name".to_string())
                })?;
                let value = action
                    .config
                    .value
                    .clone()
                    .ok_or_else(|| WorkflowError::ActionError("Missing value".to_string()))?;

                context.set_var(name, value.clone());
                Ok(serde_json::json!({ "variable": name, "value": value }))
            }

            ActionType::Log => {
                let message = action
                    .config
                    .message
                    .as_ref()
                    .ok_or_else(|| WorkflowError::ActionError("Missing message".to_string()))?;
                let message = self.interpolate(message, context);

                info!("[Workflow Log] {}", message);
                Ok(serde_json::json!({ "logged": message }))
            }

            ActionType::ExecuteCode => {
                let language = action.config.language.as_deref().unwrap_or("bash");
                let code = action
                    .config
                    .code
                    .as_ref()
                    .ok_or_else(|| WorkflowError::ActionError("Missing code".to_string()))?;

                let code = self.interpolate(code, context);
                self.execute_code(language, &code).await
            }

            ActionType::Condition => {
                let condition =
                    action.config.condition.as_ref().ok_or_else(|| {
                        WorkflowError::ActionError("Missing condition".to_string())
                    })?;

                let expr = self.interpolate(condition, context);
                let value = self.evaluate_condition(&expr, context)?;

                if let Some(var) = action.config.variable.as_deref() {
                    context.set_var(var, serde_json::json!(value));
                }

                Ok(serde_json::json!({
                    "condition": expr,
                    "result": value,
                    "stored_as": action.config.variable,
                }))
            }

            ActionType::Loop => {
                let collection_name = action.config.variable.as_ref().ok_or_else(|| {
                    WorkflowError::ActionError("Missing collection variable".to_string())
                })?;

                let Some(collection) = context.variables.get(collection_name) else {
                    return Err(WorkflowError::ActionError(format!(
                        "Unknown variable: {}",
                        collection_name
                    )));
                };

                let items = collection.as_array().ok_or_else(|| {
                    WorkflowError::ActionError(format!(
                        "Variable {} is not an array",
                        collection_name
                    ))
                })?;
                let items: Vec<serde_json::Value> = items.clone();
                let iterations = items.len();

                let item_var = action
                    .config
                    .value
                    .as_ref()
                    .and_then(|v| v.as_str())
                    .unwrap_or("item")
                    .to_string();

                let original_item = context.variables.get(&item_var).cloned();

                let mut results = Vec::with_capacity(iterations);
                for item in &items {
                    context.set_var(&item_var, item.clone());

                    if let Some(code) = action.config.code.as_ref() {
                        let language = action.config.language.as_deref().unwrap_or("bash");
                        let code = self.interpolate(code, context);
                        results.push(self.execute_code(language, &code).await?);
                    } else {
                        results.push(item.clone());
                    }
                }

                match original_item {
                    Some(v) => {
                        context.set_var(&item_var, v);
                    }
                    None => {
                        context.variables.remove(&item_var);
                    }
                }

                Ok(serde_json::json!({
                    "iterations": iterations,
                    "item_var": item_var,
                    "results": results,
                }))
            }

            ActionType::AiCall => {
                let prompt = action
                    .config
                    .prompt
                    .as_ref()
                    .ok_or_else(|| WorkflowError::ActionError("Missing prompt".to_string()))?;
                let prompt = self.interpolate(prompt, context);

                let api_key = std::env::var("OPENAI_API_KEY").map_err(|_| {
                    WorkflowError::ActionError("Missing OPENAI_API_KEY".to_string())
                })?;

                let provider = OpenAIProvider::new(api_key);
                let options = ChatOptions {
                    model: action.config.model.clone(),
                    ..Default::default()
                };

                let resp = provider
                    .chat(&[Message::user(prompt)], options)
                    .await
                    .map_err(|e| WorkflowError::ActionError(e.to_string()))?;

                Ok(serde_json::json!({
                    "content": resp.content,
                    "model": resp.model,
                    "stop_reason": resp.stop_reason,
                    "usage": resp.usage.map(|u| serde_json::json!({
                        "input_tokens": u.input_tokens,
                        "output_tokens": u.output_tokens,
                    })),
                }))
            }
        }
    }

    async fn execute_code(&self, language: &str, code: &str) -> Result<serde_json::Value> {
        let mut cmd = match language.to_ascii_lowercase().as_str() {
            "bash" => {
                let mut c = Command::new("bash");
                c.arg("-lc").arg(code);
                c
            }
            "sh" => {
                let mut c = Command::new("sh");
                c.arg("-lc").arg(code);
                c
            }
            "zsh" => {
                let mut c = Command::new("zsh");
                c.arg("-lc").arg(code);
                c
            }
            "python" | "python3" => {
                let mut c = Command::new("python3");
                c.arg("-c").arg(code);
                c
            }
            "node" | "javascript" | "js" => {
                let mut c = Command::new("node");
                c.arg("-e").arg(code);
                c
            }
            other => {
                return Err(WorkflowError::ActionError(format!(
                    "Unsupported language: {}",
                    other
                )))
            }
        };

        let output = cmd
            .output()
            .await
            .map_err(|e| WorkflowError::ActionError(format!("Failed to execute code: {e}")))?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let exit_code = output.status.code();

        if !output.status.success() {
            return Err(WorkflowError::ActionError(format!(
                "Code execution failed (exit {:?}): {}",
                exit_code, stderr
            )));
        }

        Ok(serde_json::json!({
            "language": language,
            "exit_code": exit_code,
            "stdout": stdout,
            "stderr": stderr,
        }))
    }

    fn evaluate_condition(&self, expr: &str, context: &ExecutionContext) -> Result<bool> {
        let expr = expr.trim();
        if expr.is_empty() {
            return Err(WorkflowError::ActionError("Empty condition".to_string()));
        }

        if let Ok(value) = expr.parse::<bool>() {
            return Ok(value);
        }

        // Support: {{var}} (after interpolation) or bare var name.
        if let Some(value) = context.variables.get(expr) {
            return Ok(is_truthy(value));
        }

        // Support simple binary comparisons.
        for op in ["==", "!=", ">=", "<=", ">", "<"] {
            if let Some((left, right)) = split_once(expr, op) {
                return Ok(compare_operands(left.trim(), right.trim(), op, context));
            }
        }

        Ok(!expr.is_empty())
    }

    /// Interpolate variables in a string.
    fn interpolate(&self, template: &str, context: &ExecutionContext) -> String {
        let mut result = template.to_string();

        for (name, value) in &context.variables {
            let placeholder = format!("{{{{{}}}}}", name);
            let replacement = match value {
                serde_json::Value::String(s) => s.clone(),
                _ => value.to_string(),
            };
            result = result.replace(&placeholder, &replacement);
        }

        result
    }

    /// Get number of running workflows.
    pub async fn running_count(&self) -> usize {
        self.running.read().await.len()
    }
}

fn split_once<'a>(s: &'a str, pat: &str) -> Option<(&'a str, &'a str)> {
    let idx = s.find(pat)?;
    Some((&s[..idx], &s[idx + pat.len()..]))
}

fn is_truthy(v: &serde_json::Value) -> bool {
    match v {
        serde_json::Value::Null => false,
        serde_json::Value::Bool(b) => *b,
        serde_json::Value::Number(n) => n.as_f64().map(|x| x != 0.0).unwrap_or(false),
        serde_json::Value::String(s) => !s.is_empty() && s != "false" && s != "0",
        serde_json::Value::Array(a) => !a.is_empty(),
        serde_json::Value::Object(o) => !o.is_empty(),
    }
}

fn parse_operand(s: &str, context: &ExecutionContext) -> serde_json::Value {
    let s = s.trim();
    if (s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')) {
        return serde_json::Value::String(s[1..s.len() - 1].to_string());
    }
    if let Ok(b) = s.parse::<bool>() {
        return serde_json::Value::Bool(b);
    }
    if let Ok(n) = s.parse::<f64>() {
        return serde_json::json!(n);
    }
    if let Some(v) = context.variables.get(s) {
        return v.clone();
    }
    serde_json::Value::String(s.to_string())
}

fn compare_operands(left: &str, right: &str, op: &str, context: &ExecutionContext) -> bool {
    let l = parse_operand(left, context);
    let r = parse_operand(right, context);

    match op {
        "==" => l == r,
        "!=" => l != r,
        ">" | ">=" | "<" | "<=" => match (l.as_f64(), r.as_f64()) {
            (Some(a), Some(b)) => match op {
                ">" => a > b,
                ">=" => a >= b,
                "<" => a < b,
                "<=" => a <= b,
                _ => false,
            },
            _ => false,
        },
        _ => false,
    }
}

impl Default for WorkflowExecutor {
    fn default() -> Self {
        Self::new(10)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::Action;

    #[test]
    fn test_execution_context() {
        let mut ctx = ExecutionContext::new();
        ctx.set_var("name", serde_json::json!("Alice"));

        assert_eq!(ctx.get_var("name"), Some(&serde_json::json!("Alice")));
    }

    #[tokio::test]
    async fn test_executor_creation() {
        let executor = WorkflowExecutor::new(5);
        assert_eq!(executor.running_count().await, 0);
    }

    #[test]
    fn test_interpolation() {
        let executor = WorkflowExecutor::new(5);
        let mut ctx = ExecutionContext::new();
        ctx.set_var("name", serde_json::json!("World"));

        let result = executor.interpolate("Hello, {{name}}!", &ctx);
        assert_eq!(result, "Hello, World!");
    }
}
