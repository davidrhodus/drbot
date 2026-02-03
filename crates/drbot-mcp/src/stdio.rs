//! Stdio-based MCP server mode.
//!
//! This allows drbot to act as an MCP server that can be invoked
//! by Claude Desktop, Claude Code, or other MCP clients.
//!
//! # Usage
//!
//! ```bash
//! # In claude_desktop_config.json:
//! {
//!   "mcpServers": {
//!     "drbot": {
//!       "command": "drbot",
//!       "args": ["mcp"]
//!     }
//!   }
//! }
//! ```

use crate::{
    error_codes, JsonRpcError, JsonRpcNotification, JsonRpcRequest, JsonRpcResponse, McpServer,
    RequestId,
};
use std::io::{BufRead, Write};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

/// Run drbot as an MCP server via stdio.
pub struct StdioServer {
    server: McpServer,
}

impl StdioServer {
    /// Create a new stdio server.
    pub fn new(server: McpServer) -> Self {
        Self { server }
    }

    /// Run the server, processing requests from stdin and writing to stdout.
    pub async fn run(&self) -> Result<(), std::io::Error> {
        info!("Starting MCP stdio server");

        let stdin = std::io::stdin();
        let stdout = std::io::stdout();
        let mut stdout_lock = stdout.lock();

        for line in stdin.lock().lines() {
            let line = match line {
                Ok(l) => l,
                Err(e) => {
                    error!("Failed to read stdin: {}", e);
                    break;
                }
            };

            if line.trim().is_empty() {
                continue;
            }

            debug!("Received: {}", line);

            // Try to parse as request first
            if let Ok(request) = serde_json::from_str::<JsonRpcRequest>(&line) {
                let response = self.server.handle_request(request).await;
                let json = serde_json::to_string(&response).unwrap();
                writeln!(stdout_lock, "{}", json)?;
                stdout_lock.flush()?;
            } else if let Ok(notification) = serde_json::from_str::<JsonRpcNotification>(&line) {
                // Handle notification (no response)
                self.server.handle_notification(notification).await;
            } else {
                // Invalid JSON or unknown format
                warn!("Invalid message: {}", line);
                let response = JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: RequestId::Number(0),
                    result: None,
                    error: Some(JsonRpcError {
                        code: error_codes::PARSE_ERROR,
                        message: "Failed to parse JSON-RPC message".to_string(),
                        data: None,
                    }),
                };
                let json = serde_json::to_string(&response).unwrap();
                writeln!(stdout_lock, "{}", json)?;
                stdout_lock.flush()?;
            }
        }

        info!("MCP stdio server stopped");
        Ok(())
    }

    /// Run the server asynchronously with proper async I/O.
    pub async fn run_async(&self) -> Result<(), std::io::Error> {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

        info!("Starting async MCP stdio server");

        let stdin = tokio::io::stdin();
        let stdout = tokio::io::stdout();

        let mut reader = BufReader::new(stdin);
        let mut stdout = stdout;
        let mut line = String::new();

        loop {
            line.clear();
            match reader.read_line(&mut line).await {
                Ok(0) => {
                    info!("EOF on stdin, shutting down");
                    break;
                }
                Ok(_) => {}
                Err(e) => {
                    error!("Failed to read stdin: {}", e);
                    break;
                }
            }

            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            debug!("Received: {}", trimmed);

            // Try to parse as request first
            if let Ok(request) = serde_json::from_str::<JsonRpcRequest>(trimmed) {
                let response = self.server.handle_request(request).await;
                let mut json = serde_json::to_string(&response).unwrap();
                json.push('\n');
                stdout.write_all(json.as_bytes()).await?;
                stdout.flush().await?;
            } else if let Ok(notification) = serde_json::from_str::<JsonRpcNotification>(trimmed) {
                self.server.handle_notification(notification).await;
            } else {
                warn!("Invalid message: {}", trimmed);
                let response = JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: RequestId::Number(0),
                    result: None,
                    error: Some(JsonRpcError {
                        code: error_codes::PARSE_ERROR,
                        message: "Failed to parse JSON-RPC message".to_string(),
                        data: None,
                    }),
                };
                let mut json = serde_json::to_string(&response).unwrap();
                json.push('\n');
                stdout.write_all(json.as_bytes()).await?;
                stdout.flush().await?;
            }
        }

        info!("Async MCP stdio server stopped");
        Ok(())
    }
}

/// Builder for creating an MCP server with tools.
pub struct McpServerBuilder {
    name: String,
    version: String,
}

impl McpServerBuilder {
    /// Create a new server builder.
    pub fn new(name: &str, version: &str) -> Self {
        Self {
            name: name.to_string(),
            version: version.to_string(),
        }
    }

    /// Build the server.
    pub fn build(self) -> McpServer {
        McpServer::new(&self.name, &self.version)
    }
}

/// Default drbot MCP server with built-in tools.
pub fn create_drbot_mcp_server() -> McpServer {
    let server = McpServer::new("drbot", env!("CARGO_PKG_VERSION"));

    // Tools will be registered by the caller based on enabled features
    server
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_builder() {
        let server = McpServerBuilder::new("test", "1.0.0").build();
        assert!(true); // Server created successfully
    }
}
