//! Long polling support for drbot.
//!
//! This crate provides:
//! - Long polling server
//! - Client connection management
//! - Message queueing
//! - Timeout handling

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::{mpsc, oneshot, RwLock};
use tokio::time::timeout;
use uuid::Uuid;

/// Long polling error types.
#[derive(Error, Debug)]
pub enum LongPollError {
    #[error("Timeout")]
    Timeout,

    #[error("Client not found: {0}")]
    ClientNotFound(String),

    #[error("Channel closed")]
    ChannelClosed,

    #[error("Queue full")]
    QueueFull,

    #[error("Invalid request: {0}")]
    InvalidRequest(String),
}

/// Result type for long polling operations.
pub type Result<T> = std::result::Result<T, LongPollError>;

/// Message envelope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Message ID.
    pub id: Uuid,
    /// Message type.
    pub message_type: String,
    /// Payload.
    pub payload: serde_json::Value,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
    /// Sequence number.
    pub sequence: u64,
}

impl Message {
    /// Create a new message.
    pub fn new(message_type: impl Into<String>, payload: serde_json::Value) -> Self {
        Self {
            id: Uuid::new_v4(),
            message_type: message_type.into(),
            payload,
            timestamp: Utc::now(),
            sequence: 0,
        }
    }

    /// Create from typed data.
    pub fn typed<T: Serialize>(message_type: impl Into<String>, data: &T) -> Result<Self> {
        let payload =
            serde_json::to_value(data).map_err(|e| LongPollError::InvalidRequest(e.to_string()))?;
        Ok(Self::new(message_type, payload))
    }
}

/// Poll request.
#[derive(Debug, Clone, Deserialize)]
pub struct PollRequest {
    /// Client ID.
    pub client_id: String,
    /// Last received sequence.
    pub last_sequence: Option<u64>,
    /// Timeout in milliseconds.
    pub timeout_ms: Option<u64>,
    /// Max messages to return.
    pub max_messages: Option<usize>,
}

impl PollRequest {
    /// Create a new poll request.
    pub fn new(client_id: impl Into<String>) -> Self {
        Self {
            client_id: client_id.into(),
            last_sequence: None,
            timeout_ms: Some(30000),
            max_messages: Some(100),
        }
    }

    /// Set last sequence.
    pub fn since(mut self, sequence: u64) -> Self {
        self.last_sequence = Some(sequence);
        self
    }

    /// Set timeout.
    pub fn timeout(mut self, ms: u64) -> Self {
        self.timeout_ms = Some(ms);
        self
    }
}

/// Poll response.
#[derive(Debug, Clone, Serialize)]
pub struct PollResponse {
    /// Messages.
    pub messages: Vec<Message>,
    /// Next sequence to poll from.
    pub next_sequence: u64,
    /// Whether there are more messages.
    pub has_more: bool,
    /// Server timestamp.
    pub timestamp: DateTime<Utc>,
}

impl PollResponse {
    /// Create an empty response.
    pub fn empty(next_sequence: u64) -> Self {
        Self {
            messages: Vec::new(),
            next_sequence,
            has_more: false,
            timestamp: Utc::now(),
        }
    }
}

/// Client state.
#[derive(Debug)]
struct ClientState {
    id: String,
    last_poll: DateTime<Utc>,
    last_sequence: u64,
    pending_sender: Option<oneshot::Sender<Vec<Message>>>,
    metadata: HashMap<String, String>,
}

impl ClientState {
    fn new(id: String) -> Self {
        Self {
            id,
            last_poll: Utc::now(),
            last_sequence: 0,
            pending_sender: None,
            metadata: HashMap::new(),
        }
    }
}

/// Message queue per client.
struct MessageQueue {
    messages: Vec<Message>,
    sequence_counter: u64,
    max_size: usize,
}

impl MessageQueue {
    fn new(max_size: usize) -> Self {
        Self {
            messages: Vec::new(),
            sequence_counter: 0,
            max_size,
        }
    }

    fn push(&mut self, mut message: Message) -> u64 {
        self.sequence_counter += 1;
        message.sequence = self.sequence_counter;

        self.messages.push(message);

        // Trim old messages
        if self.messages.len() > self.max_size {
            self.messages.remove(0);
        }

        self.sequence_counter
    }

    fn get_since(&self, sequence: u64, limit: usize) -> Vec<Message> {
        self.messages
            .iter()
            .filter(|m| m.sequence > sequence)
            .take(limit)
            .cloned()
            .collect()
    }

    fn has_messages_since(&self, sequence: u64) -> bool {
        self.messages.iter().any(|m| m.sequence > sequence)
    }

    fn latest_sequence(&self) -> u64 {
        self.sequence_counter
    }
}

/// Long polling server.
pub struct LongPollServer {
    clients: RwLock<HashMap<String, ClientState>>,
    queues: RwLock<HashMap<String, MessageQueue>>,
    default_timeout: std::time::Duration,
    max_queue_size: usize,
}

impl LongPollServer {
    /// Create a new server.
    pub fn new() -> Self {
        Self {
            clients: RwLock::new(HashMap::new()),
            queues: RwLock::new(HashMap::new()),
            default_timeout: std::time::Duration::from_secs(30),
            max_queue_size: 1000,
        }
    }

    /// Set default timeout.
    pub fn with_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.default_timeout = timeout;
        self
    }

    /// Set max queue size.
    pub fn with_max_queue_size(mut self, size: usize) -> Self {
        self.max_queue_size = size;
        self
    }

    /// Register a client.
    pub async fn register_client(&self, client_id: impl Into<String>) -> String {
        let id = client_id.into();
        let mut clients = self.clients.write().await;
        clients
            .entry(id.clone())
            .or_insert_with(|| ClientState::new(id.clone()));

        let mut queues = self.queues.write().await;
        queues
            .entry(id.clone())
            .or_insert_with(|| MessageQueue::new(self.max_queue_size));

        id
    }

    /// Unregister a client.
    pub async fn unregister_client(&self, client_id: &str) {
        let mut clients = self.clients.write().await;
        clients.remove(client_id);

        let mut queues = self.queues.write().await;
        queues.remove(client_id);
    }

    /// Poll for messages.
    pub async fn poll(&self, request: PollRequest) -> Result<PollResponse> {
        let timeout_ms = request
            .timeout_ms
            .unwrap_or(self.default_timeout.as_millis() as u64);
        let max_messages = request.max_messages.unwrap_or(100);
        let last_sequence = request.last_sequence.unwrap_or(0);

        // Check for existing messages first
        {
            let queues = self.queues.read().await;
            if let Some(queue) = queues.get(&request.client_id) {
                if queue.has_messages_since(last_sequence) {
                    let messages = queue.get_since(last_sequence, max_messages);
                    let next_sequence =
                        messages.last().map(|m| m.sequence).unwrap_or(last_sequence);
                    let has_more = queue.latest_sequence() > next_sequence;

                    return Ok(PollResponse {
                        messages,
                        next_sequence,
                        has_more,
                        timestamp: Utc::now(),
                    });
                }
            } else {
                return Err(LongPollError::ClientNotFound(request.client_id));
            }
        }

        // Wait for new messages
        let (tx, rx) = oneshot::channel();

        {
            let mut clients = self.clients.write().await;
            if let Some(client) = clients.get_mut(&request.client_id) {
                client.last_poll = Utc::now();
                client.pending_sender = Some(tx);
            } else {
                return Err(LongPollError::ClientNotFound(request.client_id));
            }
        }

        // Wait with timeout
        let wait_result = timeout(std::time::Duration::from_millis(timeout_ms), rx).await;

        // Clear pending sender
        {
            let mut clients = self.clients.write().await;
            if let Some(client) = clients.get_mut(&request.client_id) {
                client.pending_sender = None;
            }
        }

        match wait_result {
            Ok(Ok(messages)) => {
                let next_sequence = messages.last().map(|m| m.sequence).unwrap_or(last_sequence);
                Ok(PollResponse {
                    messages,
                    next_sequence,
                    has_more: false,
                    timestamp: Utc::now(),
                })
            }
            Ok(Err(_)) => Err(LongPollError::ChannelClosed),
            Err(_) => {
                // Timeout - return empty response
                Ok(PollResponse::empty(last_sequence))
            }
        }
    }

    /// Send a message to a client.
    pub async fn send(&self, client_id: &str, message: Message) -> Result<u64> {
        // Add to queue
        let sequence = {
            let mut queues = self.queues.write().await;
            let queue = queues
                .get_mut(client_id)
                .ok_or_else(|| LongPollError::ClientNotFound(client_id.to_string()))?;
            queue.push(message.clone())
        };

        // Wake up pending poll
        {
            let mut clients = self.clients.write().await;
            if let Some(client) = clients.get_mut(client_id) {
                if let Some(sender) = client.pending_sender.take() {
                    let mut msg = message;
                    msg.sequence = sequence;
                    let _ = sender.send(vec![msg]);
                }
            }
        }

        Ok(sequence)
    }

    /// Broadcast to all clients.
    pub async fn broadcast(&self, message: Message) -> Vec<(String, u64)> {
        let client_ids: Vec<String> = {
            let clients = self.clients.read().await;
            clients.keys().cloned().collect()
        };

        let mut results = Vec::new();
        for client_id in client_ids {
            if let Ok(seq) = self.send(&client_id, message.clone()).await {
                results.push((client_id, seq));
            }
        }
        results
    }

    /// Get connected client IDs.
    pub async fn client_ids(&self) -> Vec<String> {
        let clients = self.clients.read().await;
        clients.keys().cloned().collect()
    }

    /// Get client count.
    pub async fn client_count(&self) -> usize {
        let clients = self.clients.read().await;
        clients.len()
    }

    /// Clean up stale clients.
    pub async fn cleanup_stale(&self, max_age: Duration) -> Vec<String> {
        let now = Utc::now();
        let mut removed = Vec::new();

        let stale_ids: Vec<String> = {
            let clients = self.clients.read().await;
            clients
                .iter()
                .filter(|(_, c)| now - c.last_poll > max_age)
                .map(|(id, _)| id.clone())
                .collect()
        };

        for id in stale_ids {
            self.unregister_client(&id).await;
            removed.push(id);
        }

        removed
    }
}

impl Default for LongPollServer {
    fn default() -> Self {
        Self::new()
    }
}

/// Channel-based long polling.
pub struct LongPollChannel {
    name: String,
    server: Arc<LongPollServer>,
}

impl LongPollChannel {
    /// Create a new channel.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            server: Arc::new(LongPollServer::new()),
        }
    }

    /// Get channel name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get server.
    pub fn server(&self) -> Arc<LongPollServer> {
        self.server.clone()
    }
}

/// Long polling hub for multiple channels.
pub struct LongPollHub {
    channels: RwLock<HashMap<String, Arc<LongPollChannel>>>,
}

impl LongPollHub {
    /// Create a new hub.
    pub fn new() -> Self {
        Self {
            channels: RwLock::new(HashMap::new()),
        }
    }

    /// Get or create a channel.
    pub async fn channel(&self, name: &str) -> Arc<LongPollChannel> {
        let channels = self.channels.read().await;
        if let Some(channel) = channels.get(name) {
            return channel.clone();
        }
        drop(channels);

        let mut channels = self.channels.write().await;
        if let Some(channel) = channels.get(name) {
            return channel.clone();
        }

        let channel = Arc::new(LongPollChannel::new(name));
        channels.insert(name.to_string(), channel.clone());
        channel
    }

    /// Remove a channel.
    pub async fn remove_channel(&self, name: &str) -> Option<Arc<LongPollChannel>> {
        let mut channels = self.channels.write().await;
        channels.remove(name)
    }

    /// List channel names.
    pub async fn list_channels(&self) -> Vec<String> {
        let channels = self.channels.read().await;
        channels.keys().cloned().collect()
    }
}

impl Default for LongPollHub {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_creation() {
        let message = Message::new("update", serde_json::json!({"key": "value"}));
        assert_eq!(message.message_type, "update");
    }

    #[test]
    fn test_poll_request_builder() {
        let request = PollRequest::new("client-1").since(100).timeout(5000);

        assert_eq!(request.client_id, "client-1");
        assert_eq!(request.last_sequence, Some(100));
        assert_eq!(request.timeout_ms, Some(5000));
    }

    #[tokio::test]
    async fn test_register_client() {
        let server = LongPollServer::new();

        let id = server.register_client("client-1").await;
        assert_eq!(id, "client-1");

        let clients = server.client_ids().await;
        assert!(clients.contains(&"client-1".to_string()));
    }

    #[tokio::test]
    async fn test_send_message() {
        let server = LongPollServer::new();
        server.register_client("client-1").await;

        let message = Message::new("test", serde_json::json!({"data": "hello"}));
        let seq = server.send("client-1", message).await.unwrap();

        assert_eq!(seq, 1);
    }

    #[tokio::test]
    async fn test_poll_existing_messages() {
        let server = LongPollServer::new();
        server.register_client("client-1").await;

        let message = Message::new("test", serde_json::json!({"data": "hello"}));
        server.send("client-1", message).await.unwrap();

        let request = PollRequest::new("client-1").timeout(100);
        let response = server.poll(request).await.unwrap();

        assert_eq!(response.messages.len(), 1);
        assert_eq!(response.next_sequence, 1);
    }

    #[tokio::test]
    async fn test_poll_timeout() {
        let server = LongPollServer::new();
        server.register_client("client-1").await;

        let request = PollRequest::new("client-1").timeout(100);
        let response = server.poll(request).await.unwrap();

        assert!(response.messages.is_empty());
    }

    #[tokio::test]
    async fn test_broadcast() {
        let server = LongPollServer::new();
        server.register_client("client-1").await;
        server.register_client("client-2").await;

        let message = Message::new("broadcast", serde_json::json!({"data": "all"}));
        let results = server.broadcast(message).await;

        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn test_hub() {
        let hub = LongPollHub::new();

        let channel = hub.channel("events").await;
        assert_eq!(channel.name(), "events");

        let channels = hub.list_channels().await;
        assert!(channels.contains(&"events".to_string()));
    }

    #[tokio::test]
    async fn test_client_not_found() {
        let server = LongPollServer::new();

        let request = PollRequest::new("nonexistent");
        let result = server.poll(request).await;

        assert!(matches!(result, Err(LongPollError::ClientNotFound(_))));
    }

    #[tokio::test]
    async fn test_cleanup_stale() {
        let server = LongPollServer::new();
        server.register_client("client-1").await;

        // Immediate cleanup with zero age should remove
        let removed = server.cleanup_stale(Duration::zero()).await;
        assert!(removed.contains(&"client-1".to_string()));
    }
}
