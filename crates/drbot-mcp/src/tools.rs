//! MCP tool definitions and registry.

use crate::protocol::{CallToolResult, ToolContent, ToolDefinition};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;

/// Trait for implementing MCP tools.
#[async_trait]
pub trait Tool: Send + Sync {
    /// Tool name.
    fn name(&self) -> &str;

    /// Tool description.
    fn description(&self) -> Option<&str>;

    /// JSON schema for input parameters.
    fn input_schema(&self) -> serde_json::Value;

    /// Execute the tool with given arguments.
    async fn call(
        &self,
        arguments: HashMap<String, serde_json::Value>,
    ) -> Result<CallToolResult, String>;
}

/// Registry for MCP tools.
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
}

impl ToolRegistry {
    /// Create a new tool registry.
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// Register a tool.
    pub fn register(&mut self, tool: Arc<dyn Tool>) {
        self.tools.insert(tool.name().to_string(), tool);
    }

    /// Get a tool by name.
    pub fn get(&self, name: &str) -> Option<&Arc<dyn Tool>> {
        self.tools.get(name)
    }

    /// List all tools.
    pub fn list(&self) -> impl Iterator<Item = ToolDefinition> + '_ {
        self.tools.values().map(|t| ToolDefinition {
            name: t.name().to_string(),
            description: t.description().map(|s| s.to_string()),
            input_schema: t.input_schema(),
        })
    }

    /// Call a tool by name.
    pub async fn call(
        &self,
        name: &str,
        arguments: HashMap<String, serde_json::Value>,
    ) -> Result<CallToolResult, String> {
        let tool = self
            .tools
            .get(name)
            .ok_or_else(|| format!("Tool not found: {}", name))?;
        tool.call(arguments).await
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// A simple function-based tool.
pub struct FnTool<F>
where
    F: Fn(HashMap<String, serde_json::Value>) -> Result<String, String> + Send + Sync,
{
    name: String,
    description: Option<String>,
    input_schema: serde_json::Value,
    func: F,
}

impl<F> FnTool<F>
where
    F: Fn(HashMap<String, serde_json::Value>) -> Result<String, String> + Send + Sync,
{
    /// Create a new function-based tool.
    pub fn new(
        name: impl Into<String>,
        description: Option<String>,
        input_schema: serde_json::Value,
        func: F,
    ) -> Self {
        Self {
            name: name.into(),
            description,
            input_schema,
            func,
        }
    }
}

#[async_trait]
impl<F> Tool for FnTool<F>
where
    F: Fn(HashMap<String, serde_json::Value>) -> Result<String, String> + Send + Sync,
{
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    fn input_schema(&self) -> serde_json::Value {
        self.input_schema.clone()
    }

    async fn call(
        &self,
        arguments: HashMap<String, serde_json::Value>,
    ) -> Result<CallToolResult, String> {
        let result = (self.func)(arguments)?;
        Ok(CallToolResult {
            content: vec![ToolContent::Text { text: result }],
            is_error: false,
        })
    }
}

/// Built-in: Echo tool for testing.
pub struct EchoTool;

#[async_trait]
impl Tool for EchoTool {
    fn name(&self) -> &str {
        "echo"
    }

    fn description(&self) -> Option<&str> {
        Some("Echo back the input message")
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "message": {
                    "type": "string",
                    "description": "Message to echo"
                }
            },
            "required": ["message"]
        })
    }

    async fn call(
        &self,
        arguments: HashMap<String, serde_json::Value>,
    ) -> Result<CallToolResult, String> {
        let message = arguments
            .get("message")
            .and_then(|v| v.as_str())
            .ok_or("Missing message argument")?;

        Ok(CallToolResult {
            content: vec![ToolContent::Text {
                text: message.to_string(),
            }],
            is_error: false,
        })
    }
}

/// Built-in: Current time tool.
pub struct CurrentTimeTool;

#[async_trait]
impl Tool for CurrentTimeTool {
    fn name(&self) -> &str {
        "current_time"
    }

    fn description(&self) -> Option<&str> {
        Some("Get the current date and time")
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "timezone": {
                    "type": "string",
                    "description": "Timezone (optional, defaults to UTC)"
                }
            }
        })
    }

    async fn call(
        &self,
        _arguments: HashMap<String, serde_json::Value>,
    ) -> Result<CallToolResult, String> {
        let now = chrono::Utc::now();
        Ok(CallToolResult {
            content: vec![ToolContent::Text {
                text: now.to_rfc3339(),
            }],
            is_error: false,
        })
    }
}

/// Built-in: Read file tool.
pub struct ReadFileTool;

#[async_trait]
impl Tool for ReadFileTool {
    fn name(&self) -> &str {
        "read_file"
    }

    fn description(&self) -> Option<&str> {
        Some("Read the contents of a file")
    }

    fn input_schema(&self) -> serde_json::Value {
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

    async fn call(
        &self,
        arguments: HashMap<String, serde_json::Value>,
    ) -> Result<CallToolResult, String> {
        let path = arguments
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or("Missing path argument")?;

        let content = tokio::fs::read_to_string(path)
            .await
            .map_err(|e| format!("Failed to read file: {}", e))?;

        Ok(CallToolResult {
            content: vec![ToolContent::Text { text: content }],
            is_error: false,
        })
    }
}

/// Built-in: Write file tool.
pub struct WriteFileTool;

#[async_trait]
impl Tool for WriteFileTool {
    fn name(&self) -> &str {
        "write_file"
    }

    fn description(&self) -> Option<&str> {
        Some("Write content to a file")
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the file to write"
                },
                "content": {
                    "type": "string",
                    "description": "Content to write"
                }
            },
            "required": ["path", "content"]
        })
    }

    async fn call(
        &self,
        arguments: HashMap<String, serde_json::Value>,
    ) -> Result<CallToolResult, String> {
        let path = arguments
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or("Missing path argument")?;

        let content = arguments
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or("Missing content argument")?;

        tokio::fs::write(path, content)
            .await
            .map_err(|e| format!("Failed to write file: {}", e))?;

        Ok(CallToolResult {
            content: vec![ToolContent::Text {
                text: format!("Successfully wrote {} bytes to {}", content.len(), path),
            }],
            is_error: false,
        })
    }
}

/// Built-in: List directory tool.
pub struct ListDirectoryTool;

#[async_trait]
impl Tool for ListDirectoryTool {
    fn name(&self) -> &str {
        "list_directory"
    }

    fn description(&self) -> Option<&str> {
        Some("List contents of a directory")
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the directory"
                }
            },
            "required": ["path"]
        })
    }

    async fn call(
        &self,
        arguments: HashMap<String, serde_json::Value>,
    ) -> Result<CallToolResult, String> {
        let path = arguments
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or("Missing path argument")?;

        let mut entries = tokio::fs::read_dir(path)
            .await
            .map_err(|e| format!("Failed to read directory: {}", e))?;

        let mut items = Vec::new();
        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| format!("Failed to read entry: {}", e))?
        {
            let metadata = entry.metadata().await.ok();
            let file_type = if metadata.as_ref().map(|m| m.is_dir()).unwrap_or(false) {
                "dir"
            } else {
                "file"
            };
            items.push(format!(
                "{} [{}]",
                entry.file_name().to_string_lossy(),
                file_type
            ));
        }

        Ok(CallToolResult {
            content: vec![ToolContent::Text {
                text: items.join("\n"),
            }],
            is_error: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_echo_tool() {
        let tool = EchoTool;
        let mut args = HashMap::new();
        args.insert("message".to_string(), serde_json::json!("hello"));

        let result = tool.call(args).await.unwrap();
        assert!(!result.is_error);
        assert_eq!(result.content.len(), 1);
    }

    #[tokio::test]
    async fn test_current_time_tool() {
        let tool = CurrentTimeTool;
        let result = tool.call(HashMap::new()).await.unwrap();
        assert!(!result.is_error);
    }

    #[test]
    fn test_tool_registry() {
        let mut registry = ToolRegistry::new();
        registry.register(Arc::new(EchoTool));
        registry.register(Arc::new(CurrentTimeTool));

        assert!(registry.get("echo").is_some());
        assert!(registry.get("current_time").is_some());
        assert!(registry.get("unknown").is_none());

        let tools: Vec<_> = registry.list().collect();
        assert_eq!(tools.len(), 2);
    }
}
