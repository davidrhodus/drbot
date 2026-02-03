//! MCP client for connecting to MCP servers.

use crate::protocol::*;
use crate::transport::{Transport, TransportError};
use crate::{
    ClientCapabilities, Implementation, JsonRpcRequest, RequestId, ServerCapabilities, MCP_VERSION,
};
use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use tracing::{debug, info};

/// MCP client for communicating with MCP servers.
pub struct McpClient {
    transport: Arc<dyn Transport>,
    request_id: AtomicI64,
    server_capabilities: Option<ServerCapabilities>,
    server_info: Option<Implementation>,
}

impl McpClient {
    /// Create a new MCP client with the given transport.
    pub fn new(transport: Arc<dyn Transport>) -> Self {
        Self {
            transport,
            request_id: AtomicI64::new(1),
            server_capabilities: None,
            server_info: None,
        }
    }

    /// Generate next request ID.
    fn next_id(&self) -> RequestId {
        RequestId::Number(self.request_id.fetch_add(1, Ordering::SeqCst))
    }

    /// Send a request and get response.
    async fn call<T: serde::de::DeserializeOwned>(
        &self,
        method: &str,
        params: impl serde::Serialize,
    ) -> Result<T, McpError> {
        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: self.next_id(),
            method: method.to_string(),
            params: Some(serde_json::to_value(params)?),
        };

        debug!(method, "Sending MCP request");
        let response = self.transport.request(request).await?;

        if let Some(error) = response.error {
            return Err(McpError::ServerError {
                code: error.code,
                message: error.message,
            });
        }

        let result = response.result.ok_or(McpError::NoResult)?;
        Ok(serde_json::from_value(result)?)
    }

    /// Initialize the connection with the server.
    pub async fn initialize(&mut self) -> Result<InitializeResult, McpError> {
        let params = InitializeParams {
            protocol_version: MCP_VERSION.to_string(),
            capabilities: ClientCapabilities::default(),
            client_info: Implementation::default(),
        };

        let result: InitializeResult = self.call("initialize", params).await?;

        self.server_capabilities = Some(result.capabilities.clone());
        self.server_info = Some(result.server_info.clone());

        info!(
            server = %result.server_info.name,
            version = %result.server_info.version,
            "Connected to MCP server"
        );

        // Send initialized notification
        self.transport
            .notify(crate::JsonRpcNotification {
                jsonrpc: "2.0".to_string(),
                method: "notifications/initialized".to_string(),
                params: None,
            })
            .await?;

        Ok(result)
    }

    /// Get server capabilities (must initialize first).
    pub fn capabilities(&self) -> Option<&ServerCapabilities> {
        self.server_capabilities.as_ref()
    }

    /// List available tools.
    pub async fn list_tools(&self) -> Result<Vec<ToolDefinition>, McpError> {
        let result: ListToolsResult = self.call("tools/list", serde_json::json!({})).await?;
        Ok(result.tools)
    }

    /// Call a tool.
    pub async fn call_tool(
        &self,
        name: &str,
        arguments: HashMap<String, serde_json::Value>,
    ) -> Result<CallToolResult, McpError> {
        let params = CallToolParams {
            name: name.to_string(),
            arguments,
        };
        self.call("tools/call", params).await
    }

    /// List available resources.
    pub async fn list_resources(&self) -> Result<Vec<ResourceDefinition>, McpError> {
        let result: ListResourcesResult =
            self.call("resources/list", serde_json::json!({})).await?;
        Ok(result.resources)
    }

    /// Read a resource.
    pub async fn read_resource(&self, uri: &str) -> Result<ReadResourceResult, McpError> {
        let params = ReadResourceParams {
            uri: uri.to_string(),
        };
        self.call("resources/read", params).await
    }

    /// List available prompts.
    pub async fn list_prompts(&self) -> Result<Vec<PromptDefinition>, McpError> {
        let result: ListPromptsResult = self.call("prompts/list", serde_json::json!({})).await?;
        Ok(result.prompts)
    }

    /// Get a prompt.
    pub async fn get_prompt(
        &self,
        name: &str,
        arguments: HashMap<String, String>,
    ) -> Result<GetPromptResult, McpError> {
        let params = GetPromptParams {
            name: name.to_string(),
            arguments,
        };
        self.call("prompts/get", params).await
    }

    /// Close the connection.
    pub async fn close(&self) -> Result<(), McpError> {
        self.transport.close().await?;
        Ok(())
    }
}

/// MCP client errors.
#[derive(Debug, thiserror::Error)]
pub enum McpError {
    #[error("Transport error: {0}")]
    Transport(#[from] TransportError),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Server error ({code}): {message}")]
    ServerError { code: i32, message: String },
    #[error("No result in response")]
    NoResult,
    #[error("Not initialized")]
    NotInitialized,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcp_version() {
        assert!(!MCP_VERSION.is_empty());
    }
}
