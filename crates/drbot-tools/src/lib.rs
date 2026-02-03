//! Dynamic tool registration and execution framework.
//!
//! This crate provides:
//! - Tool definition and registration
//! - Dynamic tool execution
//! - Parameter validation
//! - Tool discovery

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Tool errors.
#[derive(Debug, Error)]
pub enum ToolError {
    #[error("Tool not found: {0}")]
    NotFound(String),

    #[error("Invalid parameters: {0}")]
    InvalidParams(String),

    #[error("Execution failed: {0}")]
    ExecutionFailed(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Timeout: {0}")]
    Timeout(String),
}

/// Result type for tool operations.
pub type Result<T> = std::result::Result<T, ToolError>;

/// Tool definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// Tool identifier.
    pub id: String,
    /// Tool name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Parameters schema.
    pub parameters: ParameterSchema,
    /// Required permissions.
    pub permissions: Vec<String>,
    /// Category.
    pub category: String,
    /// Is enabled.
    pub enabled: bool,
    /// Timeout in ms.
    pub timeout_ms: u64,
    /// Rate limit (calls per minute).
    pub rate_limit: Option<u32>,
}

/// Parameter schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParameterSchema {
    /// Schema type (always "object").
    pub schema_type: String,
    /// Properties.
    pub properties: HashMap<String, PropertySchema>,
    /// Required properties.
    pub required: Vec<String>,
}

/// Property schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropertySchema {
    /// Property type.
    pub prop_type: String,
    /// Description.
    pub description: String,
    /// Enum values (if applicable).
    pub enum_values: Option<Vec<String>>,
    /// Default value.
    pub default: Option<Value>,
}

/// Tool execution result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    /// Result identifier.
    pub id: String,
    /// Tool ID.
    pub tool_id: String,
    /// Success flag.
    pub success: bool,
    /// Output data.
    pub output: Value,
    /// Error message (if failed).
    pub error: Option<String>,
    /// Execution time in ms.
    pub execution_time_ms: u64,
    /// Timestamp.
    pub executed_at: DateTime<Utc>,
}

/// Tool executor trait.
#[async_trait]
pub trait ToolExecutor: Send + Sync {
    /// Execute the tool.
    async fn execute(&self, params: Value) -> Result<Value>;

    /// Get tool definition.
    fn definition(&self) -> ToolDefinition;
}

/// Tool execution context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionContext {
    /// User ID.
    pub user_id: Option<String>,
    /// Session ID.
    pub session_id: Option<String>,
    /// Permissions.
    pub permissions: Vec<String>,
    /// Metadata.
    pub metadata: HashMap<String, String>,
}

impl Default for ExecutionContext {
    fn default() -> Self {
        Self {
            user_id: None,
            session_id: None,
            permissions: vec!["*".to_string()],
            metadata: HashMap::new(),
        }
    }
}

/// The tool registry and executor.
pub struct ToolRegistry {
    /// Registered tools.
    tools: Arc<RwLock<HashMap<String, Arc<dyn ToolExecutor>>>>,
    /// Execution history.
    history: Arc<RwLock<Vec<ToolResult>>>,
    /// Rate limit tracking.
    rate_limits: Arc<RwLock<HashMap<String, Vec<DateTime<Utc>>>>>,
}

impl ToolRegistry {
    /// Create a new tool registry.
    pub fn new() -> Self {
        Self {
            tools: Arc::new(RwLock::new(HashMap::new())),
            history: Arc::new(RwLock::new(Vec::new())),
            rate_limits: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a tool.
    pub async fn register(&self, executor: Arc<dyn ToolExecutor>) {
        let def = executor.definition();
        let mut tools = self.tools.write().await;
        tools.insert(def.id.clone(), executor);
    }

    /// Unregister a tool.
    pub async fn unregister(&self, tool_id: &str) {
        let mut tools = self.tools.write().await;
        tools.remove(tool_id);
    }

    /// Execute a tool.
    pub async fn execute(
        &self,
        tool_id: &str,
        params: Value,
        context: ExecutionContext,
    ) -> Result<ToolResult> {
        let tools = self.tools.read().await;
        let executor = tools
            .get(tool_id)
            .ok_or_else(|| ToolError::NotFound(tool_id.to_string()))?
            .clone();
        drop(tools);

        let def = executor.definition();

        // Check if enabled
        if !def.enabled {
            return Err(ToolError::PermissionDenied(format!(
                "Tool '{}' is disabled",
                tool_id
            )));
        }

        // Check permissions
        if !self.check_permissions(&def.permissions, &context.permissions) {
            return Err(ToolError::PermissionDenied(format!(
                "Missing required permissions for tool '{}'",
                tool_id
            )));
        }

        // Check rate limit
        if let Some(limit) = def.rate_limit {
            if !self.check_rate_limit(tool_id, limit).await {
                return Err(ToolError::ExecutionFailed(format!(
                    "Rate limit exceeded for tool '{}'",
                    tool_id
                )));
            }
        }

        // Validate parameters
        self.validate_params(&def.parameters, &params)?;

        // Execute with timeout
        let start = std::time::Instant::now();
        let result = tokio::time::timeout(
            std::time::Duration::from_millis(def.timeout_ms),
            executor.execute(params),
        )
        .await;

        let execution_time_ms = start.elapsed().as_millis() as u64;

        let tool_result = match result {
            Ok(Ok(output)) => ToolResult {
                id: Uuid::new_v4().to_string(),
                tool_id: tool_id.to_string(),
                success: true,
                output,
                error: None,
                execution_time_ms,
                executed_at: Utc::now(),
            },
            Ok(Err(e)) => ToolResult {
                id: Uuid::new_v4().to_string(),
                tool_id: tool_id.to_string(),
                success: false,
                output: Value::Null,
                error: Some(e.to_string()),
                execution_time_ms,
                executed_at: Utc::now(),
            },
            Err(_) => {
                return Err(ToolError::Timeout(format!(
                    "Tool '{}' timed out after {}ms",
                    tool_id, def.timeout_ms
                )));
            }
        };

        // Record in history
        let mut history = self.history.write().await;
        history.push(tool_result.clone());
        if history.len() > 10000 {
            history.drain(0..1000);
        }

        if tool_result.success {
            Ok(tool_result)
        } else {
            Err(ToolError::ExecutionFailed(
                tool_result.error.unwrap_or_default(),
            ))
        }
    }

    /// Check permissions.
    fn check_permissions(&self, required: &[String], granted: &[String]) -> bool {
        if granted.contains(&"*".to_string()) {
            return true;
        }
        required.iter().all(|r| granted.contains(r))
    }

    /// Check rate limit.
    async fn check_rate_limit(&self, tool_id: &str, limit: u32) -> bool {
        let mut limits = self.rate_limits.write().await;
        let now = Utc::now();
        let one_minute_ago = now - chrono::Duration::minutes(1);

        let calls = limits.entry(tool_id.to_string()).or_insert_with(Vec::new);

        // Remove old entries
        calls.retain(|t| *t > one_minute_ago);

        if calls.len() >= limit as usize {
            false
        } else {
            calls.push(now);
            true
        }
    }

    /// Validate parameters.
    fn validate_params(&self, schema: &ParameterSchema, params: &Value) -> Result<()> {
        let obj = params
            .as_object()
            .ok_or_else(|| ToolError::InvalidParams("Parameters must be an object".to_string()))?;

        // Check required parameters
        for required in &schema.required {
            if !obj.contains_key(required) {
                return Err(ToolError::InvalidParams(format!(
                    "Missing required parameter: {}",
                    required
                )));
            }
        }

        // Validate types
        for (name, prop_schema) in &schema.properties {
            if let Some(value) = obj.get(name) {
                self.validate_type(name, value, prop_schema)?;
            }
        }

        Ok(())
    }

    /// Validate parameter type.
    fn validate_type(&self, name: &str, value: &Value, schema: &PropertySchema) -> Result<()> {
        let valid = match schema.prop_type.as_str() {
            "string" => value.is_string(),
            "number" => value.is_number(),
            "integer" => value.is_i64() || value.is_u64(),
            "boolean" => value.is_boolean(),
            "array" => value.is_array(),
            "object" => value.is_object(),
            _ => true,
        };

        if !valid {
            return Err(ToolError::InvalidParams(format!(
                "Parameter '{}' must be of type {}",
                name, schema.prop_type
            )));
        }

        // Check enum
        if let Some(enum_values) = &schema.enum_values {
            if let Some(s) = value.as_str() {
                if !enum_values.contains(&s.to_string()) {
                    return Err(ToolError::InvalidParams(format!(
                        "Parameter '{}' must be one of: {:?}",
                        name, enum_values
                    )));
                }
            }
        }

        Ok(())
    }

    /// List all tools.
    pub async fn list_tools(&self) -> Vec<ToolDefinition> {
        let tools = self.tools.read().await;
        tools.values().map(|t| t.definition()).collect()
    }

    /// Get tool definition.
    pub async fn get_tool(&self, tool_id: &str) -> Option<ToolDefinition> {
        let tools = self.tools.read().await;
        tools.get(tool_id).map(|t| t.definition())
    }

    /// Get execution history.
    pub async fn get_history(&self, limit: usize) -> Vec<ToolResult> {
        let history = self.history.read().await;
        history.iter().rev().take(limit).cloned().collect()
    }

    /// Get tools by category.
    pub async fn get_by_category(&self, category: &str) -> Vec<ToolDefinition> {
        let tools = self.tools.read().await;
        tools
            .values()
            .map(|t| t.definition())
            .filter(|d| d.category == category)
            .collect()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder for tool definitions.
pub struct ToolBuilder {
    def: ToolDefinition,
}

impl ToolBuilder {
    /// Create a new tool builder.
    pub fn new(name: &str, description: &str) -> Self {
        Self {
            def: ToolDefinition {
                id: Uuid::new_v4().to_string(),
                name: name.to_string(),
                description: description.to_string(),
                parameters: ParameterSchema {
                    schema_type: "object".to_string(),
                    properties: HashMap::new(),
                    required: Vec::new(),
                },
                permissions: Vec::new(),
                category: "general".to_string(),
                enabled: true,
                timeout_ms: 30000,
                rate_limit: None,
            },
        }
    }

    /// Add a parameter.
    pub fn param(mut self, name: &str, prop_type: &str, description: &str, required: bool) -> Self {
        self.def.parameters.properties.insert(
            name.to_string(),
            PropertySchema {
                prop_type: prop_type.to_string(),
                description: description.to_string(),
                enum_values: None,
                default: None,
            },
        );
        if required {
            self.def.parameters.required.push(name.to_string());
        }
        self
    }

    /// Set category.
    pub fn category(mut self, category: &str) -> Self {
        self.def.category = category.to_string();
        self
    }

    /// Add permission.
    pub fn permission(mut self, permission: &str) -> Self {
        self.def.permissions.push(permission.to_string());
        self
    }

    /// Set timeout.
    pub fn timeout(mut self, ms: u64) -> Self {
        self.def.timeout_ms = ms;
        self
    }

    /// Set rate limit.
    pub fn rate_limit(mut self, calls_per_minute: u32) -> Self {
        self.def.rate_limit = Some(calls_per_minute);
        self
    }

    /// Build the definition.
    pub fn build(self) -> ToolDefinition {
        self.def
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct EchoTool {
        def: ToolDefinition,
    }

    impl EchoTool {
        fn new() -> Self {
            Self {
                def: ToolBuilder::new("echo", "Echoes input")
                    .param("message", "string", "Message to echo", true)
                    .category("utility")
                    .build(),
            }
        }
    }

    #[async_trait]
    impl ToolExecutor for EchoTool {
        async fn execute(&self, params: Value) -> Result<Value> {
            let msg = params["message"].as_str().unwrap_or("no message");
            Ok(serde_json::json!({ "echo": msg }))
        }

        fn definition(&self) -> ToolDefinition {
            self.def.clone()
        }
    }

    #[tokio::test]
    async fn test_register_and_execute() {
        let registry = ToolRegistry::new();
        let tool = Arc::new(EchoTool::new());
        let tool_id = tool.definition().id.clone();

        registry.register(tool).await;

        let params = serde_json::json!({ "message": "Hello" });
        let result = registry
            .execute(&tool_id, params, ExecutionContext::default())
            .await
            .unwrap();

        assert!(result.success);
        assert_eq!(result.output["echo"], "Hello");
    }

    #[tokio::test]
    async fn test_missing_required_param() {
        let registry = ToolRegistry::new();
        let tool = Arc::new(EchoTool::new());
        let tool_id = tool.definition().id.clone();

        registry.register(tool).await;

        let params = serde_json::json!({});
        let result = registry
            .execute(&tool_id, params, ExecutionContext::default())
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_list_tools() {
        let registry = ToolRegistry::new();
        registry.register(Arc::new(EchoTool::new())).await;

        let tools = registry.list_tools().await;
        assert_eq!(tools.len(), 1);
    }

    #[tokio::test]
    async fn test_tool_not_found() {
        let registry = ToolRegistry::new();

        let result = registry
            .execute(
                "nonexistent",
                serde_json::json!({}),
                ExecutionContext::default(),
            )
            .await;
        assert!(matches!(result, Err(ToolError::NotFound(_))));
    }

    #[test]
    fn test_tool_builder() {
        let def = ToolBuilder::new("test", "Test tool")
            .param("input", "string", "Input value", true)
            .param("optional", "number", "Optional value", false)
            .category("testing")
            .timeout(5000)
            .rate_limit(10)
            .build();

        assert_eq!(def.name, "test");
        assert_eq!(def.parameters.required.len(), 1);
        assert_eq!(def.timeout_ms, 5000);
    }
}
