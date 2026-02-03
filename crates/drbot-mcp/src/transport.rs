//! MCP transport implementations.
//!
//! Supports stdio (for local processes) and HTTP/SSE (for remote servers).

use crate::{JsonRpcNotification, JsonRpcRequest, JsonRpcResponse};
use async_trait::async_trait;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::mpsc;
use tracing::{debug, error, warn};

/// Transport trait for MCP communication.
#[async_trait]
pub trait Transport: Send + Sync {
    /// Send a request and wait for response.
    async fn request(&self, request: JsonRpcRequest) -> Result<JsonRpcResponse, TransportError>;

    /// Send a notification (no response expected).
    async fn notify(&self, notification: JsonRpcNotification) -> Result<(), TransportError>;

    /// Close the transport.
    async fn close(&self) -> Result<(), TransportError>;
}

/// Transport errors.
#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Channel closed")]
    ChannelClosed,
    #[error("Process exited")]
    ProcessExited,
    #[error("Timeout")]
    Timeout,
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),
}

/// Stdio transport for local MCP servers.
pub struct StdioTransport {
    stdin_tx: mpsc::Sender<String>,
    response_rx: tokio::sync::Mutex<mpsc::Receiver<JsonRpcResponse>>,
    _child: Child,
}

impl StdioTransport {
    /// Spawn a new MCP server process.
    pub async fn spawn(command: &str, args: &[&str]) -> Result<Self, TransportError> {
        let mut child = Command::new(command)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?;

        let stdin = child.stdin.take().expect("stdin not captured");
        let stdout = child.stdout.take().expect("stdout not captured");

        let (stdin_tx, mut stdin_rx) = mpsc::channel::<String>(32);
        let (response_tx, response_rx) = mpsc::channel::<JsonRpcResponse>(32);

        // Stdin writer task
        tokio::spawn(async move {
            let mut stdin = stdin;
            while let Some(msg) = stdin_rx.recv().await {
                if let Err(e) = stdin.write_all(msg.as_bytes()).await {
                    error!("Failed to write to stdin: {}", e);
                    break;
                }
                if let Err(e) = stdin.write_all(b"\n").await {
                    error!("Failed to write newline: {}", e);
                    break;
                }
                if let Err(e) = stdin.flush().await {
                    error!("Failed to flush stdin: {}", e);
                    break;
                }
            }
        });

        // Stdout reader task
        tokio::spawn(async move {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                debug!("MCP stdout: {}", line);
                match serde_json::from_str::<JsonRpcResponse>(&line) {
                    Ok(response) => {
                        if response_tx.send(response).await.is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        warn!("Failed to parse MCP response: {} - line: {}", e, line);
                    }
                }
            }
        });

        Ok(Self {
            stdin_tx,
            response_rx: tokio::sync::Mutex::new(response_rx),
            _child: child,
        })
    }
}

#[async_trait]
impl Transport for StdioTransport {
    async fn request(&self, request: JsonRpcRequest) -> Result<JsonRpcResponse, TransportError> {
        let json = serde_json::to_string(&request)?;
        self.stdin_tx
            .send(json)
            .await
            .map_err(|_| TransportError::ChannelClosed)?;

        let mut rx = self.response_rx.lock().await;
        rx.recv().await.ok_or(TransportError::ChannelClosed)
    }

    async fn notify(&self, notification: JsonRpcNotification) -> Result<(), TransportError> {
        let json = serde_json::to_string(&notification)?;
        self.stdin_tx
            .send(json)
            .await
            .map_err(|_| TransportError::ChannelClosed)?;
        Ok(())
    }

    async fn close(&self) -> Result<(), TransportError> {
        drop(self.stdin_tx.clone());
        Ok(())
    }
}

/// HTTP/SSE transport for remote MCP servers.
pub struct HttpTransport {
    base_url: String,
    client: reqwest::Client,
}

impl HttpTransport {
    /// Create a new HTTP transport.
    pub fn new(base_url: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl Transport for HttpTransport {
    async fn request(&self, request: JsonRpcRequest) -> Result<JsonRpcResponse, TransportError> {
        let response = self
            .client
            .post(&self.base_url)
            .json(&request)
            .send()
            .await
            .map_err(|e| TransportError::ConnectionFailed(e.to_string()))?;

        let response: JsonRpcResponse = response
            .json()
            .await
            .map_err(|e| TransportError::ConnectionFailed(e.to_string()))?;

        Ok(response)
    }

    async fn notify(&self, notification: JsonRpcNotification) -> Result<(), TransportError> {
        self.client
            .post(&self.base_url)
            .json(&notification)
            .send()
            .await
            .map_err(|e| TransportError::ConnectionFailed(e.to_string()))?;
        Ok(())
    }

    async fn close(&self) -> Result<(), TransportError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_http_transport_creation() {
        let transport = HttpTransport::new("http://localhost:8080/mcp");
        assert_eq!(transport.base_url, "http://localhost:8080/mcp");
    }
}
