//! MCP server implementation for hosting tools and resources.

use crate::protocol::*;
use crate::resources::ResourceRegistry;
use crate::tools::ToolRegistry;
use crate::{
    error_codes, Implementation, JsonRpcError, JsonRpcNotification, JsonRpcRequest,
    JsonRpcResponse, ServerCapabilities, MCP_VERSION,
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, warn};

/// MCP server for hosting tools and resources.
pub struct McpServer {
    info: Implementation,
    capabilities: ServerCapabilities,
    tools: Arc<RwLock<ToolRegistry>>,
    resources: Arc<RwLock<ResourceRegistry>>,
    prompts: Arc<RwLock<HashMap<String, PromptDefinition>>>,
}

impl McpServer {
    /// Create a new MCP server.
    pub fn new(name: &str, version: &str) -> Self {
        Self {
            info: Implementation {
                name: name.to_string(),
                version: version.to_string(),
            },
            capabilities: ServerCapabilities {
                tools: Some(crate::ToolsCapability { list_changed: true }),
                resources: Some(crate::ResourcesCapability {
                    subscribe: false,
                    list_changed: true,
                }),
                prompts: Some(crate::PromptsCapability { list_changed: true }),
                logging: Some(crate::LoggingCapability {}),
            },
            tools: Arc::new(RwLock::new(ToolRegistry::new())),
            resources: Arc::new(RwLock::new(ResourceRegistry::new())),
            prompts: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get the tool registry for registration.
    pub fn tools(&self) -> Arc<RwLock<ToolRegistry>> {
        self.tools.clone()
    }

    /// Get the resource registry for registration.
    pub fn resources(&self) -> Arc<RwLock<ResourceRegistry>> {
        self.resources.clone()
    }

    /// Register a prompt.
    pub async fn register_prompt(&self, prompt: PromptDefinition) {
        let mut prompts = self.prompts.write().await;
        prompts.insert(prompt.name.clone(), prompt);
    }

    /// Handle an incoming JSON-RPC request.
    pub async fn handle_request(&self, request: JsonRpcRequest) -> JsonRpcResponse {
        debug!(method = %request.method, "Handling MCP request");

        let result = match request.method.as_str() {
            "initialize" => self.handle_initialize(request.params).await,
            "tools/list" => self.handle_list_tools().await,
            "tools/call" => self.handle_call_tool(request.params).await,
            "resources/list" => self.handle_list_resources().await,
            "resources/read" => self.handle_read_resource(request.params).await,
            "prompts/list" => self.handle_list_prompts().await,
            "prompts/get" => self.handle_get_prompt(request.params).await,
            "ping" => Ok(serde_json::json!({})),
            _ => {
                warn!(method = %request.method, "Unknown MCP method");
                Err(JsonRpcError {
                    code: error_codes::METHOD_NOT_FOUND,
                    message: format!("Unknown method: {}", request.method),
                    data: None,
                })
            }
        };

        match result {
            Ok(value) => JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: request.id,
                result: Some(value),
                error: None,
            },
            Err(error) => JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: request.id,
                result: None,
                error: Some(error),
            },
        }
    }

    /// Handle notifications (no response needed).
    pub async fn handle_notification(&self, notification: JsonRpcNotification) {
        debug!(method = %notification.method, "Handling MCP notification");
        // Most notifications don't need handling
    }

    async fn handle_initialize(
        &self,
        _params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, JsonRpcError> {
        let result = InitializeResult {
            protocol_version: MCP_VERSION.to_string(),
            capabilities: self.capabilities.clone(),
            server_info: self.info.clone(),
            instructions: None,
        };
        Ok(serde_json::to_value(result).unwrap())
    }

    async fn handle_list_tools(&self) -> Result<serde_json::Value, JsonRpcError> {
        let tools = self.tools.read().await;
        let definitions: Vec<ToolDefinition> = tools.list().collect();
        let result = ListToolsResult {
            tools: definitions,
            next_cursor: None,
        };
        Ok(serde_json::to_value(result).unwrap())
    }

    async fn handle_call_tool(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, JsonRpcError> {
        let params: CallToolParams = params
            .ok_or_else(|| JsonRpcError {
                code: error_codes::INVALID_PARAMS,
                message: "Missing params".to_string(),
                data: None,
            })
            .and_then(|p| {
                serde_json::from_value(p).map_err(|e| JsonRpcError {
                    code: error_codes::INVALID_PARAMS,
                    message: e.to_string(),
                    data: None,
                })
            })?;

        let tools = self.tools.read().await;
        let result = tools
            .call(&params.name, params.arguments)
            .await
            .map_err(|e| JsonRpcError {
                code: error_codes::INTERNAL_ERROR,
                message: e,
                data: None,
            })?;

        Ok(serde_json::to_value(result).unwrap())
    }

    async fn handle_list_resources(&self) -> Result<serde_json::Value, JsonRpcError> {
        let resources = self.resources.read().await;
        let definitions: Vec<ResourceDefinition> = resources.list().collect();
        let result = ListResourcesResult {
            resources: definitions,
            next_cursor: None,
        };
        Ok(serde_json::to_value(result).unwrap())
    }

    async fn handle_read_resource(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, JsonRpcError> {
        let params: ReadResourceParams = params
            .ok_or_else(|| JsonRpcError {
                code: error_codes::INVALID_PARAMS,
                message: "Missing params".to_string(),
                data: None,
            })
            .and_then(|p| {
                serde_json::from_value(p).map_err(|e| JsonRpcError {
                    code: error_codes::INVALID_PARAMS,
                    message: e.to_string(),
                    data: None,
                })
            })?;

        let resources = self.resources.read().await;
        let result = resources
            .read(&params.uri)
            .await
            .map_err(|e| JsonRpcError {
                code: error_codes::INTERNAL_ERROR,
                message: e,
                data: None,
            })?;

        Ok(serde_json::to_value(result).unwrap())
    }

    async fn handle_list_prompts(&self) -> Result<serde_json::Value, JsonRpcError> {
        let prompts = self.prompts.read().await;
        let definitions: Vec<PromptDefinition> = prompts.values().cloned().collect();
        let result = ListPromptsResult {
            prompts: definitions,
            next_cursor: None,
        };
        Ok(serde_json::to_value(result).unwrap())
    }

    async fn handle_get_prompt(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, JsonRpcError> {
        let params: GetPromptParams = params
            .ok_or_else(|| JsonRpcError {
                code: error_codes::INVALID_PARAMS,
                message: "Missing params".to_string(),
                data: None,
            })
            .and_then(|p| {
                serde_json::from_value(p).map_err(|e| JsonRpcError {
                    code: error_codes::INVALID_PARAMS,
                    message: e.to_string(),
                    data: None,
                })
            })?;

        let prompts = self.prompts.read().await;
        let _prompt = prompts.get(&params.name).ok_or_else(|| JsonRpcError {
            code: error_codes::INVALID_PARAMS,
            message: format!("Prompt not found: {}", params.name),
            data: None,
        })?;

        // For now, return a simple result
        let result = GetPromptResult {
            description: None,
            messages: vec![],
        };
        Ok(serde_json::to_value(result).unwrap())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_creation() {
        let server = McpServer::new("test-server", "1.0.0");
        assert_eq!(server.info.name, "test-server");
    }
}
