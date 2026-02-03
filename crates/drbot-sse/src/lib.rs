//! Server-Sent Events for drbot.
//!
//! This crate provides:
//! - SSE event formatting
//! - Event streaming
//! - Reconnection support
//! - Event filtering

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;

/// SSE error types.
#[derive(Error, Debug)]
pub enum SseError {
    #[error("Connection closed")]
    ConnectionClosed,

    #[error("Channel full")]
    ChannelFull,

    #[error("Invalid event: {0}")]
    InvalidEvent(String),

    #[error("Stream error: {0}")]
    StreamError(String),
}

/// Result type for SSE operations.
pub type Result<T> = std::result::Result<T, SseError>;

/// SSE event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    /// Event ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Event type.
    #[serde(rename = "event", skip_serializing_if = "Option::is_none")]
    pub event_type: Option<String>,
    /// Event data.
    pub data: String,
    /// Retry timeout in milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry: Option<u64>,
}

impl Event {
    /// Create a new event with data.
    pub fn data(data: impl Into<String>) -> Self {
        Self {
            id: None,
            event_type: None,
            data: data.into(),
            retry: None,
        }
    }

    /// Create a new event with type and data.
    pub fn message(event_type: impl Into<String>, data: impl Into<String>) -> Self {
        Self {
            id: None,
            event_type: Some(event_type.into()),
            data: data.into(),
            retry: None,
        }
    }

    /// Set event ID.
    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// Set retry timeout.
    pub fn with_retry(mut self, ms: u64) -> Self {
        self.retry = Some(ms);
        self
    }

    /// Create JSON event.
    pub fn json<T: Serialize>(event_type: impl Into<String>, data: &T) -> Result<Self> {
        let data =
            serde_json::to_string(data).map_err(|e| SseError::InvalidEvent(e.to_string()))?;
        Ok(Self::message(event_type, data))
    }

    /// Format as SSE string.
    pub fn to_sse_string(&self) -> String {
        let mut result = String::new();

        if let Some(ref id) = self.id {
            result.push_str(&format!("id: {}\n", id));
        }

        if let Some(ref event_type) = self.event_type {
            result.push_str(&format!("event: {}\n", event_type));
        }

        if let Some(retry) = self.retry {
            result.push_str(&format!("retry: {}\n", retry));
        }

        // Data can have multiple lines
        for line in self.data.lines() {
            result.push_str(&format!("data: {}\n", line));
        }

        // Empty line to end event
        result.push('\n');
        result
    }
}

/// SSE comment (for keep-alive).
#[derive(Debug, Clone)]
pub struct Comment(pub String);

impl Comment {
    /// Create a new comment.
    pub fn new(text: impl Into<String>) -> Self {
        Self(text.into())
    }

    /// Create keep-alive comment.
    pub fn keep_alive() -> Self {
        Self::new("keep-alive")
    }

    /// Format as SSE string.
    pub fn to_sse_string(&self) -> String {
        format!(": {}\n\n", self.0)
    }
}

/// SSE stream item.
#[derive(Debug, Clone)]
pub enum StreamItem {
    /// Event.
    Event(Event),
    /// Comment.
    Comment(Comment),
}

impl StreamItem {
    /// Format as SSE string.
    pub fn to_sse_string(&self) -> String {
        match self {
            StreamItem::Event(e) => e.to_sse_string(),
            StreamItem::Comment(c) => c.to_sse_string(),
        }
    }
}

/// Event source for SSE.
pub struct EventSource {
    receiver: broadcast::Receiver<StreamItem>,
    last_event_id: Option<String>,
}

impl EventSource {
    /// Create from broadcast receiver.
    pub fn new(receiver: broadcast::Receiver<StreamItem>) -> Self {
        Self {
            receiver,
            last_event_id: None,
        }
    }

    /// Set last event ID for reconnection.
    pub fn with_last_event_id(mut self, id: impl Into<String>) -> Self {
        self.last_event_id = Some(id.into());
        self
    }

    /// Get last event ID.
    pub fn last_event_id(&self) -> Option<&str> {
        self.last_event_id.as_deref()
    }
}

/// SSE broadcaster.
pub struct Broadcaster {
    sender: broadcast::Sender<StreamItem>,
    event_counter: AtomicU64,
    subscribers: AtomicU64,
}

impl Broadcaster {
    /// Create a new broadcaster.
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self {
            sender,
            event_counter: AtomicU64::new(0),
            subscribers: AtomicU64::new(0),
        }
    }

    /// Subscribe to events.
    pub fn subscribe(&self) -> EventSource {
        self.subscribers.fetch_add(1, Ordering::SeqCst);
        EventSource::new(self.sender.subscribe())
    }

    /// Get subscriber count.
    pub fn subscriber_count(&self) -> u64 {
        self.subscribers.load(Ordering::SeqCst)
    }

    /// Send an event.
    pub fn send(&self, mut event: Event) -> Result<u64> {
        let id = self.event_counter.fetch_add(1, Ordering::SeqCst);
        if event.id.is_none() {
            event.id = Some(id.to_string());
        }

        self.sender
            .send(StreamItem::Event(event))
            .map_err(|_| SseError::ChannelFull)?;

        Ok(id)
    }

    /// Send a comment (keep-alive).
    pub fn send_comment(&self, comment: Comment) -> Result<()> {
        self.sender
            .send(StreamItem::Comment(comment))
            .map_err(|_| SseError::ChannelFull)?;
        Ok(())
    }

    /// Send keep-alive.
    pub fn keep_alive(&self) -> Result<()> {
        self.send_comment(Comment::keep_alive())
    }
}

impl Default for Broadcaster {
    fn default() -> Self {
        Self::new(1000)
    }
}

/// Channel-based SSE.
pub struct Channel {
    id: Uuid,
    name: String,
    broadcaster: Broadcaster,
    created_at: DateTime<Utc>,
}

impl Channel {
    /// Create a new channel.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            broadcaster: Broadcaster::default(),
            created_at: Utc::now(),
        }
    }

    /// Get channel ID.
    pub fn id(&self) -> Uuid {
        self.id
    }

    /// Get channel name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Subscribe.
    pub fn subscribe(&self) -> EventSource {
        self.broadcaster.subscribe()
    }

    /// Publish event.
    pub fn publish(&self, event: Event) -> Result<u64> {
        self.broadcaster.send(event)
    }

    /// Publish message.
    pub fn publish_message(
        &self,
        event_type: impl Into<String>,
        data: impl Into<String>,
    ) -> Result<u64> {
        self.broadcaster.send(Event::message(event_type, data))
    }

    /// Subscriber count.
    pub fn subscriber_count(&self) -> u64 {
        self.broadcaster.subscriber_count()
    }
}

/// SSE hub for managing channels.
pub struct Hub {
    channels: RwLock<HashMap<String, Arc<Channel>>>,
}

impl Hub {
    /// Create a new hub.
    pub fn new() -> Self {
        Self {
            channels: RwLock::new(HashMap::new()),
        }
    }

    /// Create or get a channel.
    pub async fn channel(&self, name: &str) -> Arc<Channel> {
        let channels = self.channels.read().await;
        if let Some(channel) = channels.get(name) {
            return channel.clone();
        }
        drop(channels);

        let mut channels = self.channels.write().await;
        // Double-check
        if let Some(channel) = channels.get(name) {
            return channel.clone();
        }

        let channel = Arc::new(Channel::new(name));
        channels.insert(name.to_string(), channel.clone());
        channel
    }

    /// Remove a channel.
    pub async fn remove_channel(&self, name: &str) -> Option<Arc<Channel>> {
        let mut channels = self.channels.write().await;
        channels.remove(name)
    }

    /// List channels.
    pub async fn list_channels(&self) -> Vec<String> {
        let channels = self.channels.read().await;
        channels.keys().cloned().collect()
    }

    /// Broadcast to all channels.
    pub async fn broadcast(&self, event: Event) {
        let channels = self.channels.read().await;
        for channel in channels.values() {
            let _ = channel.publish(event.clone());
        }
    }
}

impl Default for Hub {
    fn default() -> Self {
        Self::new()
    }
}

/// Client connection info.
#[derive(Debug, Clone)]
pub struct ClientInfo {
    /// Client ID.
    pub id: Uuid,
    /// Last event ID received.
    pub last_event_id: Option<String>,
    /// Connected at.
    pub connected_at: DateTime<Utc>,
    /// User agent.
    pub user_agent: Option<String>,
    /// Client IP.
    pub client_ip: Option<String>,
}

impl ClientInfo {
    /// Create new client info.
    pub fn new() -> Self {
        Self {
            id: Uuid::new_v4(),
            last_event_id: None,
            connected_at: Utc::now(),
            user_agent: None,
            client_ip: None,
        }
    }
}

impl Default for ClientInfo {
    fn default() -> Self {
        Self::new()
    }
}

/// Event filter.
pub trait EventFilter: Send + Sync {
    /// Filter an event.
    fn filter(&self, event: &Event) -> bool;
}

/// Type filter.
pub struct TypeFilter {
    event_types: Vec<String>,
}

impl TypeFilter {
    /// Create a new type filter.
    pub fn new(types: Vec<String>) -> Self {
        Self { event_types: types }
    }
}

impl EventFilter for TypeFilter {
    fn filter(&self, event: &Event) -> bool {
        event
            .event_type
            .as_ref()
            .map(|t| self.event_types.contains(t))
            .unwrap_or(true)
    }
}

/// Filtered event source.
pub struct FilteredEventSource<F: EventFilter> {
    source: EventSource,
    filter: F,
}

impl<F: EventFilter> FilteredEventSource<F> {
    /// Create filtered source.
    pub fn new(source: EventSource, filter: F) -> Self {
        Self { source, filter }
    }
}

/// SSE response builder.
pub struct ResponseBuilder {
    retry: Option<u64>,
    headers: HashMap<String, String>,
}

impl ResponseBuilder {
    /// Create new builder.
    pub fn new() -> Self {
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "text/event-stream".to_string());
        headers.insert("Cache-Control".to_string(), "no-cache".to_string());
        headers.insert("Connection".to_string(), "keep-alive".to_string());

        Self {
            retry: None,
            headers,
        }
    }

    /// Set retry interval.
    pub fn retry(mut self, ms: u64) -> Self {
        self.retry = Some(ms);
        self
    }

    /// Add header.
    pub fn header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(key.into(), value.into());
        self
    }

    /// Get headers.
    pub fn headers(&self) -> &HashMap<String, String> {
        &self.headers
    }
}

impl Default for ResponseBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_formatting() {
        let event = Event::message("update", "Hello, World!")
            .with_id("1")
            .with_retry(3000);

        let sse = event.to_sse_string();
        assert!(sse.contains("id: 1"));
        assert!(sse.contains("event: update"));
        assert!(sse.contains("data: Hello, World!"));
        assert!(sse.contains("retry: 3000"));
    }

    #[test]
    fn test_multiline_data() {
        let event = Event::data("Line 1\nLine 2\nLine 3");
        let sse = event.to_sse_string();

        assert!(sse.contains("data: Line 1\n"));
        assert!(sse.contains("data: Line 2\n"));
        assert!(sse.contains("data: Line 3\n"));
    }

    #[test]
    fn test_comment_formatting() {
        let comment = Comment::keep_alive();
        let sse = comment.to_sse_string();
        assert_eq!(sse, ": keep-alive\n\n");
    }

    #[test]
    fn test_json_event() {
        #[derive(Serialize)]
        struct Data {
            message: String,
        }

        let event = Event::json(
            "data",
            &Data {
                message: "test".to_string(),
            },
        )
        .unwrap();
        assert!(event.data.contains("message"));
    }

    #[test]
    fn test_broadcaster() {
        let broadcaster = Broadcaster::new(100);
        let _source = broadcaster.subscribe();

        let id = broadcaster.send(Event::data("test")).unwrap();
        assert_eq!(id, 0);

        let id = broadcaster.send(Event::data("test2")).unwrap();
        assert_eq!(id, 1);
    }

    #[tokio::test]
    async fn test_hub() {
        let hub = Hub::new();

        let channel = hub.channel("events").await;
        assert_eq!(channel.name(), "events");

        let channels = hub.list_channels().await;
        assert!(channels.contains(&"events".to_string()));
    }

    #[test]
    fn test_type_filter() {
        let filter = TypeFilter::new(vec!["message".to_string(), "update".to_string()]);

        let event1 = Event::message("message", "test");
        let event2 = Event::message("other", "test");
        let event3 = Event::data("test");

        assert!(filter.filter(&event1));
        assert!(!filter.filter(&event2));
        assert!(filter.filter(&event3)); // No type = passes
    }

    #[test]
    fn test_response_builder() {
        let builder = ResponseBuilder::new()
            .retry(5000)
            .header("X-Custom", "value");

        let headers = builder.headers();
        assert_eq!(
            headers.get("Content-Type"),
            Some(&"text/event-stream".to_string())
        );
        assert_eq!(headers.get("X-Custom"), Some(&"value".to_string()));
    }
}
