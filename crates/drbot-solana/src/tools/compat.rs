//! Tool compatibility types.
//!
//! These types bridge the Solana tools to various tool systems.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// Tool definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// Tool name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Parameters.
    pub parameters: Vec<ToolParameter>,
    /// Category.
    pub category: Option<String>,
    /// Examples.
    pub examples: Option<Vec<String>>,
}

/// Tool parameter definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolParameter {
    /// Parameter name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Parameter type (string, number, boolean, etc.).
    pub param_type: String,
    /// Whether the parameter is required.
    pub required: bool,
    /// Enum values (if applicable).
    pub enum_values: Option<Vec<String>>,
    /// Default value.
    pub default: Option<Value>,
}

/// Tool input.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInput {
    /// Parameters.
    pub parameters: Value,
}

impl ToolInput {
    /// Create from a JSON value.
    pub fn from_value(value: Value) -> Self {
        Self { parameters: value }
    }

    /// Create from a map.
    pub fn from_map(map: HashMap<String, Value>) -> Self {
        Self {
            parameters: Value::Object(map.into_iter().collect()),
        }
    }

    /// Convert parameters to a HashMap.
    pub fn to_params(&self) -> HashMap<String, Value> {
        match &self.parameters {
            Value::Object(map) => map.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
            _ => HashMap::new(),
        }
    }
}

/// Tool output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolOutput {
    /// Result data.
    pub result: Value,
    /// Metadata.
    pub metadata: HashMap<String, Value>,
}

impl Default for ToolOutput {
    fn default() -> Self {
        Self {
            result: Value::Null,
            metadata: HashMap::new(),
        }
    }
}

/// Tool error.
#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    /// Invalid input.
    #[error("Invalid input: {0}")]
    InvalidInput(String),
    /// Execution failed.
    #[error("Execution failed: {0}")]
    ExecutionFailed(String),
    /// Permission denied.
    #[error("Permission denied: {0}")]
    PermissionDenied(String),
}

/// Tool result type.
pub type ToolResult = std::result::Result<ToolOutput, ToolError>;

/// Tool trait.
pub trait Tool: Send + Sync {
    /// Get the tool definition.
    fn definition(&self) -> ToolDefinition;
}

/// Tool executor trait.
#[async_trait]
pub trait ToolExecutor: Tool {
    /// Execute the tool with the given input.
    async fn execute(&self, input: ToolInput) -> ToolResult;
}

/// Convert a ToolDefinition to JSON schema format.
impl ToolDefinition {
    /// Convert to JSON schema for MCP compatibility.
    pub fn to_json_schema(&self) -> Value {
        let mut properties = serde_json::Map::new();
        let mut required = Vec::new();

        for param in &self.parameters {
            let mut prop = serde_json::Map::new();
            prop.insert("type".to_string(), Value::String(param.param_type.clone()));
            prop.insert(
                "description".to_string(),
                Value::String(param.description.clone()),
            );

            if let Some(ref enum_values) = param.enum_values {
                prop.insert(
                    "enum".to_string(),
                    Value::Array(
                        enum_values
                            .iter()
                            .map(|v| Value::String(v.clone()))
                            .collect(),
                    ),
                );
            }

            if let Some(ref default) = param.default {
                prop.insert("default".to_string(), default.clone());
            }

            properties.insert(param.name.clone(), Value::Object(prop));

            if param.required {
                required.push(Value::String(param.name.clone()));
            }
        }

        serde_json::json!({
            "type": "object",
            "properties": properties,
            "required": required
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_definition_schema() {
        let def = ToolDefinition {
            name: "test".to_string(),
            description: "Test tool".to_string(),
            parameters: vec![ToolParameter {
                name: "action".to_string(),
                description: "Action to perform".to_string(),
                param_type: "string".to_string(),
                required: true,
                enum_values: Some(vec!["a".to_string(), "b".to_string()]),
                default: None,
            }],
            category: None,
            examples: None,
        };

        let schema = def.to_json_schema();
        assert!(schema["properties"]["action"]["enum"].is_array());
    }
}
