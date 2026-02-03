//! Signal protocol channel for drbot.
//!
//! This crate provides Signal integration via signal-cli's JSON RPC interface.
//! It requires signal-cli to be running in daemon mode.
//!
//! # Setup
//!
//! 1. Install signal-cli: https://github.com/AsamK/signal-cli
//! 2. Register or link your account
//! 3. Start the daemon: `signal-cli -a +1234567890 daemon --socket /tmp/signal-cli.sock`

mod api;

pub use api::*;

use async_trait::async_trait;
use drbot_channels::Channel;
use drbot_core::message::{Content, IncomingMessage, MessageSender, OutgoingMessage};
use drbot_core::Result;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};
use tracing::{debug, error, info};
use uuid::Uuid;

/// Signal channel configuration.
#[derive(Debug, Clone)]
pub struct SignalConfig {
    /// Path to the signal-cli socket.
    pub socket_path: PathBuf,
    /// Account phone number (e.g., "+1234567890").
    pub account: String,
}

impl SignalConfig {
    /// Create a new configuration.
    pub fn new(account: impl Into<String>) -> Self {
        Self {
            socket_path: PathBuf::from("/tmp/signal-cli.sock"),
            account: account.into(),
        }
    }

    /// Set custom socket path.
    pub fn with_socket_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.socket_path = path.into();
        self
    }
}

/// Signal channel implementation.
pub struct SignalChannel {
    /// Configuration.
    config: SignalConfig,
    /// Unix socket connection.
    socket: Arc<Mutex<Option<UnixStream>>>,
    /// Request ID counter.
    request_id: AtomicU64,
    /// Broadcast sender for incoming messages.
    incoming_tx: broadcast::Sender<IncomingMessage>,
    /// Whether connected.
    connected: Arc<AtomicBool>,
    /// Polling task handle.
    poll_handle: Option<tokio::task::JoinHandle<()>>,
}

impl SignalChannel {
    /// Create a new Signal channel.
    pub fn new(config: SignalConfig) -> Self {
        let (incoming_tx, _) = broadcast::channel(256);
        Self {
            config,
            socket: Arc::new(Mutex::new(None)),
            request_id: AtomicU64::new(1),
            incoming_tx,
            connected: Arc::new(AtomicBool::new(false)),
            poll_handle: None,
        }
    }

    /// Send a JSON RPC request and get response.
    async fn send_request(&self, request: JsonRpcRequest) -> Result<JsonRpcResponse> {
        let socket = self.socket.lock().await;
        let mut stream = socket
            .as_ref()
            .ok_or_else(|| drbot_core::Error::Channel("Not connected to signal-cli".to_string()))?;

        // Write request
        let request_json = serde_json::to_string(&request)
            .map_err(|e| drbot_core::Error::Internal(e.to_string()))?;
        writeln!(stream, "{}", request_json)
            .map_err(|e| drbot_core::Error::Channel(format!("Failed to send request: {}", e)))?;
        stream
            .flush()
            .map_err(|e| drbot_core::Error::Channel(format!("Failed to flush: {}", e)))?;

        // Read response
        let mut reader = BufReader::new(stream);
        let mut response_line = String::new();
        reader
            .read_line(&mut response_line)
            .map_err(|e| drbot_core::Error::Channel(format!("Failed to read response: {}", e)))?;

        let response: JsonRpcResponse = serde_json::from_str(&response_line)
            .map_err(|e| drbot_core::Error::Channel(format!("Failed to parse response: {}", e)))?;

        if let Some(err) = response.error {
            return Err(drbot_core::Error::Channel(format!(
                "Signal-cli error {}: {}",
                err.code, err.message
            )));
        }

        Ok(response)
    }

    /// Get the next request ID.
    fn next_request_id(&self) -> u64 {
        self.request_id.fetch_add(1, Ordering::SeqCst)
    }

    /// Convert a Signal envelope to an IncomingMessage.
    fn convert_envelope(envelope: &SignalEnvelope) -> Option<IncomingMessage> {
        // Get the data message
        let data_msg = envelope.data_message.as_ref()?;
        let text = data_msg.message.as_ref()?;
        let source = envelope.source.as_ref()?;

        // Determine channel ID (group or direct)
        let channel_id = if let Some(group) = &data_msg.group_info {
            group.group_id.clone()
        } else {
            source.clone()
        };

        Some(IncomingMessage {
            id: Uuid::new_v4(),
            channel_type: "signal".to_string(),
            channel_id,
            sender: MessageSender {
                id: source.clone(),
                name: None, // Signal doesn't provide names via JSON RPC
                username: Some(source.clone()),
            },
            content: vec![Content::Text { text: text.clone() }],
            received_at: chrono::Utc::now(),
            raw: serde_json::to_value(envelope).ok(),
            reply_to: data_msg
                .quote
                .as_ref()
                .and_then(|q| q.id.map(|id| id.to_string())),
        })
    }
}

#[async_trait]
impl Channel for SignalChannel {
    async fn connect(&mut self) -> Result<()> {
        info!(socket = ?self.config.socket_path, "Connecting to signal-cli");

        // Connect to Unix socket
        let stream = UnixStream::connect(&self.config.socket_path).map_err(|e| {
            drbot_core::Error::Channel(format!(
                "Failed to connect to signal-cli at {:?}: {}",
                self.config.socket_path, e
            ))
        })?;

        *self.socket.lock().await = Some(stream);
        self.connected.store(true, Ordering::SeqCst);

        info!(account = %self.config.account, "Connected to signal-cli");

        // Start polling for messages
        let config = self.config.clone();
        let incoming_tx = self.incoming_tx.clone();
        let connected = self.connected.clone();

        let poll_handle = tokio::spawn(async move {
            while connected.load(Ordering::SeqCst) {
                // Use spawn_blocking for the synchronous socket operations
                let socket_path = config.socket_path.clone();
                let result = tokio::task::spawn_blocking(move || {
                    // Connect for this poll iteration
                    let stream = UnixStream::connect(&socket_path)?;

                    // Set read timeout
                    stream.set_read_timeout(Some(std::time::Duration::from_secs(30)))?;

                    // Send receive request
                    let request = JsonRpcRequest::new("receive", 1)
                        .with_params(ReceiveParams { timeout: Some(10) });

                    let request_json = serde_json::to_string(&request)
                        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

                    let mut stream_ref = &stream;
                    writeln!(stream_ref, "{}", request_json)?;

                    // Read response
                    let mut reader = BufReader::new(&stream);
                    let mut response_line = String::new();
                    reader.read_line(&mut response_line)?;

                    Ok::<_, std::io::Error>(response_line)
                })
                .await;

                match result {
                    Ok(Ok(response_line)) => {
                        if let Ok(response) =
                            serde_json::from_str::<JsonRpcResponse>(&response_line)
                        {
                            if let Some(result) = response.result {
                                if let Ok(envelopes) =
                                    serde_json::from_value::<Vec<SignalEnvelope>>(result)
                                {
                                    for envelope in envelopes {
                                        debug!("Received Signal envelope");
                                        if let Some(incoming) = Self::convert_envelope(&envelope) {
                                            let _ = incoming_tx.send(incoming);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Ok(Err(e)) => {
                        if e.kind() != std::io::ErrorKind::WouldBlock
                            && e.kind() != std::io::ErrorKind::TimedOut
                        {
                            debug!(error = %e, "Poll error (may be normal)");
                        }
                        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    }
                    Err(e) => {
                        error!(error = %e, "Spawn blocking failed");
                        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                    }
                }
            }

            info!("Signal polling stopped");
        });

        self.poll_handle = Some(poll_handle);
        Ok(())
    }

    async fn send(&self, to: &str, message: OutgoingMessage) -> Result<()> {
        let text = message
            .content
            .iter()
            .filter_map(|c| match c {
                Content::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");

        if text.is_empty() {
            return Err(drbot_core::Error::InvalidInput(
                "No text content in message".to_string(),
            ));
        }

        // Determine if this is a group or direct message
        let (recipient, group_id) = if to.starts_with('+') || to.contains('-') && to.len() > 30 {
            // UUID or phone number = direct message
            (Some(vec![to.to_string()]), None)
        } else {
            // Group ID
            (None, Some(to.to_string()))
        };

        let params = SendMessageParams {
            recipient,
            group_id,
            message: text,
            quote_timestamp: None,
            quote_author: None,
        };

        let request = JsonRpcRequest::new("send", self.next_request_id()).with_params(params);

        self.send_request(request).await?;
        Ok(())
    }

    fn subscribe(&self) -> broadcast::Receiver<IncomingMessage> {
        self.incoming_tx.subscribe()
    }

    async fn disconnect(&mut self) -> Result<()> {
        info!("Disconnecting from Signal");

        self.connected.store(false, Ordering::SeqCst);

        if let Some(handle) = self.poll_handle.take() {
            handle.abort();
        }

        *self.socket.lock().await = None;

        Ok(())
    }

    fn channel_type(&self) -> &str {
        "signal"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_creation() {
        let config = SignalConfig::new("+1234567890");
        assert_eq!(config.account, "+1234567890");
        assert_eq!(config.socket_path, PathBuf::from("/tmp/signal-cli.sock"));
    }

    #[test]
    fn test_config_with_socket() {
        let config = SignalConfig::new("+1234567890").with_socket_path("/custom/path.sock");
        assert_eq!(config.socket_path, PathBuf::from("/custom/path.sock"));
    }

    #[test]
    fn test_channel_creation() {
        let channel = SignalChannel::new(SignalConfig::new("+1234567890"));
        assert_eq!(channel.channel_type(), "signal");
    }

    #[test]
    fn test_json_rpc_request() {
        let request =
            JsonRpcRequest::new("receive", 1).with_params(ReceiveParams { timeout: Some(10) });
        assert_eq!(request.method, "receive");
        assert_eq!(request.id, 1);
        assert!(request.params.is_some());
    }
}
