//! Tool system for agent capabilities.

use crate::{AgentError, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Arc;
use tokio::process::Command;
use tracing::{debug, warn};

/// A tool that an agent can use.
#[async_trait]
pub trait AgentTool: Send + Sync {
    /// Tool name.
    fn name(&self) -> &str;

    /// Tool description.
    fn description(&self) -> &str;

    /// JSON schema for parameters.
    fn parameters(&self) -> Value;

    /// Execute the tool.
    async fn execute(&self, args: Value) -> Result<String>;
}

/// Collection of built-in tools.
pub struct BuiltinTools;

impl BuiltinTools {
    /// Get all built-in tools.
    pub fn all() -> Vec<Arc<dyn AgentTool>> {
        vec![
            Arc::new(BashTool::new()),
            Arc::new(ReadFileTool),
            Arc::new(WriteFileTool),
            Arc::new(ListDirectoryTool),
            Arc::new(SearchTool),
            Arc::new(HttpTool),
            Arc::new(CalculatorTool),
        ]
    }
}

/// Tool for executing bash commands.
pub struct BashTool {
    /// Allowed command prefixes (for sandboxing).
    allowed_prefixes: Vec<String>,
    /// Timeout in seconds.
    timeout_secs: u64,
}

impl BashTool {
    pub fn new() -> Self {
        fn truthy(value: &str) -> bool {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        }

        let allow_all = std::env::var("DRBOT_OPENCLAW_AGENT_BASH_ALLOW_ALL")
            .ok()
            .as_deref()
            .map(truthy)
            .unwrap_or(false)
            || std::env::var("DRBOT_AGENT_BASH_ALLOW_ALL")
                .ok()
                .as_deref()
                .map(truthy)
                .unwrap_or(false);

        let allowlist_raw = std::env::var("DRBOT_OPENCLAW_AGENT_BASH_ALLOWLIST")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .or_else(|| {
                std::env::var("DRBOT_AGENT_BASH_ALLOWLIST")
                    .ok()
                    .filter(|v| !v.trim().is_empty())
            });

        let allowlist = allowlist_raw
            .as_deref()
            .map(|raw| {
                raw.split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let allow_all = allow_all
            || allowlist.iter().any(|s| s == "*" || s.eq_ignore_ascii_case("all"));

        let allowed_prefixes = if allow_all {
            Vec::new()
        } else if !allowlist.is_empty() {
            allowlist
        } else {
            vec![
                "ls".to_string(),
                "cat".to_string(),
                "echo".to_string(),
                "pwd".to_string(),
                "date".to_string(),
                "whoami".to_string(),
                "find".to_string(),
                "grep".to_string(),
                "head".to_string(),
                "tail".to_string(),
                "wc".to_string(),
                "sort".to_string(),
                "uniq".to_string(),
            ]
        };
        Self {
            allowed_prefixes,
            timeout_secs: 30,
        }
    }

    pub fn with_allowed_commands(mut self, commands: Vec<String>) -> Self {
        self.allowed_prefixes = commands;
        self
    }

    fn is_allowed(&self, command: &str) -> bool {
        if self.allowed_prefixes.is_empty() {
            return true; // No restrictions
        }
        let cmd = command.trim().split_whitespace().next().unwrap_or("");
        self.allowed_prefixes
            .iter()
            .any(|p| cmd == p || cmd.ends_with(&format!("/{}", p)))
    }
}

impl Default for BashTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AgentTool for BashTool {
    fn name(&self) -> &str {
        "bash"
    }

    fn description(&self) -> &str {
        "Execute a bash command and return the output"
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The bash command to execute"
                }
            },
            "required": ["command"]
        })
    }

    async fn execute(&self, args: Value) -> Result<String> {
        let command = args["command"]
            .as_str()
            .ok_or_else(|| AgentError::ToolError("Missing 'command' argument".to_string()))?;

        if !self.is_allowed(command) {
            return Err(AgentError::ToolError(format!(
                "Command not allowed: {}",
                command
            )));
        }

        debug!("Executing bash command: {}", command);

        let output = tokio::time::timeout(
            tokio::time::Duration::from_secs(self.timeout_secs),
            Command::new("bash")
                .arg("-c")
                .arg(command)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output(),
        )
        .await
        .map_err(|_| AgentError::Timeout)?
        .map_err(|e| AgentError::ToolError(e.to_string()))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if output.status.success() {
            Ok(stdout.to_string())
        } else {
            Ok(format!(
                "Error (exit code {}): {}\n{}",
                output.status.code().unwrap_or(-1),
                stdout,
                stderr
            ))
        }
    }
}

/// Tool for reading files.
pub struct ReadFileTool;

#[async_trait]
impl AgentTool for ReadFileTool {
    fn name(&self) -> &str {
        "read_file"
    }

    fn description(&self) -> &str {
        "Read the contents of a file"
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the file to read"
                }
            },
            "required": ["path"]
        })
    }

    async fn execute(&self, args: Value) -> Result<String> {
        let path = args["path"]
            .as_str()
            .ok_or_else(|| AgentError::ToolError("Missing 'path' argument".to_string()))?;

        tokio::fs::read_to_string(path)
            .await
            .map_err(|e| AgentError::ToolError(format!("Failed to read file: {}", e)))
    }
}

/// Tool for writing files.
pub struct WriteFileTool;

#[async_trait]
impl AgentTool for WriteFileTool {
    fn name(&self) -> &str {
        "write_file"
    }

    fn description(&self) -> &str {
        "Write content to a file"
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the file to write"
                },
                "content": {
                    "type": "string",
                    "description": "Content to write to the file"
                }
            },
            "required": ["path", "content"]
        })
    }

    async fn execute(&self, args: Value) -> Result<String> {
        let path = args["path"]
            .as_str()
            .ok_or_else(|| AgentError::ToolError("Missing 'path' argument".to_string()))?;
        let content = args["content"]
            .as_str()
            .ok_or_else(|| AgentError::ToolError("Missing 'content' argument".to_string()))?;

        tokio::fs::write(path, content)
            .await
            .map_err(|e| AgentError::ToolError(format!("Failed to write file: {}", e)))?;

        Ok(format!(
            "Successfully wrote {} bytes to {}",
            content.len(),
            path
        ))
    }
}

/// Tool for listing directory contents.
pub struct ListDirectoryTool;

#[async_trait]
impl AgentTool for ListDirectoryTool {
    fn name(&self) -> &str {
        "list_directory"
    }

    fn description(&self) -> &str {
        "List files and directories in a path"
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Directory path to list"
                }
            },
            "required": ["path"]
        })
    }

    async fn execute(&self, args: Value) -> Result<String> {
        let path = args["path"]
            .as_str()
            .ok_or_else(|| AgentError::ToolError("Missing 'path' argument".to_string()))?;

        let mut entries = tokio::fs::read_dir(path)
            .await
            .map_err(|e| AgentError::ToolError(format!("Failed to read directory: {}", e)))?;

        let mut items = Vec::new();
        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| AgentError::ToolError(e.to_string()))?
        {
            let file_type = entry.file_type().await.ok();
            let type_indicator = if file_type.map(|t| t.is_dir()).unwrap_or(false) {
                "/"
            } else {
                ""
            };
            items.push(format!(
                "{}{}",
                entry.file_name().to_string_lossy(),
                type_indicator
            ));
        }

        items.sort();
        Ok(items.join("\n"))
    }
}

/// Tool for searching text in files.
pub struct SearchTool;

#[async_trait]
impl AgentTool for SearchTool {
    fn name(&self) -> &str {
        "search"
    }

    fn description(&self) -> &str {
        "Search for text patterns in files"
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "Text pattern to search for"
                },
                "path": {
                    "type": "string",
                    "description": "Directory or file to search in"
                }
            },
            "required": ["pattern", "path"]
        })
    }

    async fn execute(&self, args: Value) -> Result<String> {
        let pattern = args["pattern"]
            .as_str()
            .ok_or_else(|| AgentError::ToolError("Missing 'pattern' argument".to_string()))?;
        let path = args["path"]
            .as_str()
            .ok_or_else(|| AgentError::ToolError("Missing 'path' argument".to_string()))?;

        let output = Command::new("grep")
            .args(["-r", "-n", "--include=*", pattern, path])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| AgentError::ToolError(format!("Search failed: {}", e)))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        if stdout.is_empty() {
            Ok("No matches found".to_string())
        } else {
            Ok(stdout.to_string())
        }
    }
}

/// Tool for making HTTP requests.
pub struct HttpTool;

#[async_trait]
impl AgentTool for HttpTool {
    fn name(&self) -> &str {
        "http"
    }

    fn description(&self) -> &str {
        "Make HTTP requests to URLs"
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "method": {
                    "type": "string",
                    "enum": ["GET", "POST"],
                    "description": "HTTP method"
                },
                "url": {
                    "type": "string",
                    "description": "URL to request"
                },
                "body": {
                    "type": "string",
                    "description": "Request body (for POST)"
                }
            },
            "required": ["method", "url"]
        })
    }

    async fn execute(&self, args: Value) -> Result<String> {
        let method = args["method"]
            .as_str()
            .ok_or_else(|| AgentError::ToolError("Missing 'method' argument".to_string()))?;
        let url = args["url"]
            .as_str()
            .ok_or_else(|| AgentError::ToolError("Missing 'url' argument".to_string()))?;

        let client = reqwest::Client::new();

        let response: reqwest::Response = match method.to_uppercase().as_str() {
            "GET" => client
                .get(url)
                .send()
                .await
                .map_err(|e| AgentError::ToolError(format!("HTTP request failed: {}", e)))?,
            "POST" => {
                let body = args["body"].as_str().unwrap_or("");
                client
                    .post(url)
                    .body(body.to_string())
                    .send()
                    .await
                    .map_err(|e| AgentError::ToolError(format!("HTTP request failed: {}", e)))?
            }
            _ => {
                return Err(AgentError::ToolError(format!(
                    "Unsupported method: {}",
                    method
                )))
            }
        };

        let status = response.status();
        let text: String = response
            .text()
            .await
            .map_err(|e| AgentError::ToolError(format!("Failed to read response: {}", e)))?;

        Ok(format!("Status: {}\n\n{}", status, text))
    }
}

/// Tool for basic calculations.
pub struct CalculatorTool;

#[async_trait]
impl AgentTool for CalculatorTool {
    fn name(&self) -> &str {
        "calculator"
    }

    fn description(&self) -> &str {
        "Perform basic mathematical calculations"
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "expression": {
                    "type": "string",
                    "description": "Mathematical expression to evaluate (e.g., '2 + 2', '10 * 5')"
                }
            },
            "required": ["expression"]
        })
    }

    async fn execute(&self, args: Value) -> Result<String> {
        let expr = args["expression"]
            .as_str()
            .ok_or_else(|| AgentError::ToolError("Missing 'expression' argument".to_string()))?;

        // Simple expression evaluator
        let result = evaluate_simple_expr(expr)
            .map_err(|e| AgentError::ToolError(format!("Invalid expression: {}", e)))?;

        Ok(result.to_string())
    }
}

/// Simple expression evaluator for basic math.
fn evaluate_simple_expr(expr: &str) -> std::result::Result<f64, String> {
    let expr = expr.trim();

    // Try to parse as a single number first
    if let Ok(n) = expr.parse::<f64>() {
        return Ok(n);
    }

    // Find operator
    for op in ['+', '-', '*', '/', '%'] {
        if let Some(pos) = expr.rfind(op) {
            if pos > 0 {
                let left = evaluate_simple_expr(&expr[..pos])?;
                let right = evaluate_simple_expr(&expr[pos + 1..])?;

                return match op {
                    '+' => Ok(left + right),
                    '-' => Ok(left - right),
                    '*' => Ok(left * right),
                    '/' => {
                        if right == 0.0 {
                            Err("Division by zero".to_string())
                        } else {
                            Ok(left / right)
                        }
                    }
                    '%' => Ok(left % right),
                    _ => unreachable!(),
                };
            }
        }
    }

    Err(format!("Cannot evaluate: {}", expr))
}

/// Tool registry for managing available tools.
#[derive(Default)]
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn AgentTool>>,
}

impl ToolRegistry {
    /// Create a new registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a registry with builtin tools.
    pub fn with_builtins() -> Self {
        let mut registry = Self::new();
        for tool in BuiltinTools::all() {
            registry.register(tool);
        }
        registry
    }

    /// Register a tool.
    pub fn register(&mut self, tool: Arc<dyn AgentTool>) {
        self.tools.insert(tool.name().to_string(), tool);
    }

    /// Get a tool by name.
    pub fn get(&self, name: &str) -> Option<Arc<dyn AgentTool>> {
        self.tools.get(name).cloned()
    }

    /// List all tool names.
    pub fn list(&self) -> Vec<String> {
        self.tools.keys().cloned().collect()
    }

    /// Get all tools.
    pub fn all(&self) -> Vec<Arc<dyn AgentTool>> {
        self.tools.values().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculator() {
        assert_eq!(evaluate_simple_expr("2 + 2").unwrap(), 4.0);
        assert_eq!(evaluate_simple_expr("10 * 5").unwrap(), 50.0);
        assert_eq!(evaluate_simple_expr("100 / 4").unwrap(), 25.0);
    }

    #[test]
    fn test_bash_tool_allowed() {
        let tool = BashTool::new();
        assert!(tool.is_allowed("ls -la"));
        assert!(tool.is_allowed("cat file.txt"));
        assert!(!tool.is_allowed("rm -rf /"));
    }

    #[test]
    fn test_tool_registry() {
        let mut registry = ToolRegistry::new();
        registry.register(Arc::new(CalculatorTool));

        assert!(registry.get("calculator").is_some());
        assert!(registry.get("nonexistent").is_none());
    }
}
