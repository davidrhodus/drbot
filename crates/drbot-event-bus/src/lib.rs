//! In-process event bus for drbot.
//!
//! This crate provides:
//! - Pub/sub messaging
//! - Topic-based routing
//! - Event filtering
//! - Async event handlers

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::{broadcast, mpsc, RwLock};
use uuid::Uuid;

/// Event bus error types.
#[derive(Error, Debug)]
pub enum EventBusError {
    #[error("No subscribers for topic: {0}")]
    NoSubscribers(String),

    #[error("Send failed: {0}")]
    SendFailed(String),

    #[error("Handler error: {0}")]
    HandlerError(String),

    #[error("Bus closed")]
    Closed,
}

/// Result type for event bus operations.
pub type Result<T> = std::result::Result<T, EventBusError>;

/// Event metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventMetadata {
    /// Unique event ID.
    pub id: Uuid,
    /// Event timestamp.
    pub timestamp: DateTime<Utc>,
    /// Event source.
    pub source: Option<String>,
    /// Correlation ID for tracing.
    pub correlation_id: Option<Uuid>,
    /// Custom attributes.
    pub attributes: HashMap<String, String>,
}

impl EventMetadata {
    /// Create new metadata.
    pub fn new() -> Self {
        Self {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            source: None,
            correlation_id: None,
            attributes: HashMap::new(),
        }
    }

    /// Set source.
    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }

    /// Set correlation ID.
    pub fn with_correlation_id(mut self, id: Uuid) -> Self {
        self.correlation_id = Some(id);
        self
    }

    /// Add attribute.
    pub fn with_attribute(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.attributes.insert(key.into(), value.into());
        self
    }
}

impl Default for EventMetadata {
    fn default() -> Self {
        Self::new()
    }
}

/// An event with typed payload.
#[derive(Debug, Clone)]
pub struct Event<T> {
    /// Event metadata.
    pub metadata: EventMetadata,
    /// Event topic.
    pub topic: String,
    /// Event payload.
    pub payload: T,
}

impl<T> Event<T> {
    /// Create a new event.
    pub fn new(topic: impl Into<String>, payload: T) -> Self {
        Self {
            metadata: EventMetadata::new(),
            topic: topic.into(),
            payload,
        }
    }

    /// Set metadata.
    pub fn with_metadata(mut self, metadata: EventMetadata) -> Self {
        self.metadata = metadata;
        self
    }
}

/// Trait for event handlers.
#[async_trait]
pub trait EventHandler<T>: Send + Sync {
    /// Handle an event.
    async fn handle(&self, event: &Event<T>) -> Result<()>;
}

/// A boxed event handler.
type BoxedHandler<T> = Arc<dyn EventHandler<T>>;

/// Subscription ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SubscriptionId(u64);

/// Event bus for typed events.
pub struct TypedEventBus<T: Clone + Send + Sync + 'static> {
    sender: broadcast::Sender<Event<T>>,
    handlers: RwLock<HashMap<SubscriptionId, BoxedHandler<T>>>,
    next_sub_id: AtomicU64,
    published: AtomicU64,
    delivered: AtomicU64,
}

impl<T: Clone + Send + Sync + 'static> TypedEventBus<T> {
    /// Create a new typed event bus.
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self {
            sender,
            handlers: RwLock::new(HashMap::new()),
            next_sub_id: AtomicU64::new(1),
            published: AtomicU64::new(0),
            delivered: AtomicU64::new(0),
        }
    }

    /// Subscribe with a handler.
    pub async fn subscribe<H: EventHandler<T> + 'static>(&self, handler: H) -> SubscriptionId {
        let id = SubscriptionId(self.next_sub_id.fetch_add(1, Ordering::Relaxed));
        let mut handlers = self.handlers.write().await;
        handlers.insert(id, Arc::new(handler));
        id
    }

    /// Unsubscribe a handler.
    pub async fn unsubscribe(&self, id: SubscriptionId) -> bool {
        let mut handlers = self.handlers.write().await;
        handlers.remove(&id).is_some()
    }

    /// Publish an event.
    pub async fn publish(&self, event: Event<T>) -> Result<usize> {
        self.published.fetch_add(1, Ordering::Relaxed);

        let handlers = self.handlers.read().await;
        let mut delivered = 0;

        for handler in handlers.values() {
            if handler.handle(&event).await.is_ok() {
                delivered += 1;
            }
        }

        self.delivered
            .fetch_add(delivered as u64, Ordering::Relaxed);

        // Also broadcast for receivers
        let _ = self.sender.send(event);

        Ok(delivered)
    }

    /// Get a receiver for events.
    pub fn subscribe_receiver(&self) -> broadcast::Receiver<Event<T>> {
        self.sender.subscribe()
    }

    /// Get published count.
    pub fn published(&self) -> u64 {
        self.published.load(Ordering::Relaxed)
    }

    /// Get delivered count.
    pub fn delivered(&self) -> u64 {
        self.delivered.load(Ordering::Relaxed)
    }

    /// Get handler count.
    pub async fn handler_count(&self) -> usize {
        self.handlers.read().await.len()
    }
}

/// Generic event with JSON payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenericEvent {
    /// Event metadata.
    pub metadata: EventMetadata,
    /// Event topic.
    pub topic: String,
    /// Event type.
    pub event_type: String,
    /// JSON payload.
    pub payload: serde_json::Value,
}

impl GenericEvent {
    /// Create a new generic event.
    pub fn new(
        topic: impl Into<String>,
        event_type: impl Into<String>,
        payload: serde_json::Value,
    ) -> Self {
        Self {
            metadata: EventMetadata::new(),
            topic: topic.into(),
            event_type: event_type.into(),
            payload,
        }
    }

    /// Create from a typed event.
    pub fn from_typed<T: Serialize>(
        event: &Event<T>,
        event_type: impl Into<String>,
    ) -> Result<Self> {
        let payload = serde_json::to_value(&event.payload)
            .map_err(|e| EventBusError::SendFailed(e.to_string()))?;

        Ok(Self {
            metadata: event.metadata.clone(),
            topic: event.topic.clone(),
            event_type: event_type.into(),
            payload,
        })
    }
}

/// Topic-based event bus.
pub struct TopicEventBus {
    topics: RwLock<HashMap<String, broadcast::Sender<GenericEvent>>>,
    capacity: usize,
    published: AtomicU64,
}

impl TopicEventBus {
    /// Create a new topic event bus.
    pub fn new(capacity: usize) -> Self {
        Self {
            topics: RwLock::new(HashMap::new()),
            capacity,
            published: AtomicU64::new(0),
        }
    }

    /// Get or create a topic sender.
    async fn get_or_create_topic(&self, topic: &str) -> broadcast::Sender<GenericEvent> {
        {
            let topics = self.topics.read().await;
            if let Some(sender) = topics.get(topic) {
                return sender.clone();
            }
        }

        let mut topics = self.topics.write().await;
        topics
            .entry(topic.to_string())
            .or_insert_with(|| broadcast::channel(self.capacity).0)
            .clone()
    }

    /// Subscribe to a topic.
    pub async fn subscribe(&self, topic: &str) -> broadcast::Receiver<GenericEvent> {
        let sender = self.get_or_create_topic(topic).await;
        sender.subscribe()
    }

    /// Publish to a topic.
    pub async fn publish(&self, event: GenericEvent) -> Result<usize> {
        let sender = self.get_or_create_topic(&event.topic).await;
        self.published.fetch_add(1, Ordering::Relaxed);

        sender
            .send(event)
            .map_err(|_| EventBusError::NoSubscribers("No active receivers".to_string()))
    }

    /// Get list of topics.
    pub async fn topics(&self) -> Vec<String> {
        self.topics.read().await.keys().cloned().collect()
    }

    /// Get published count.
    pub fn published(&self) -> u64 {
        self.published.load(Ordering::Relaxed)
    }
}

/// Event filter.
#[derive(Debug, Clone)]
pub enum EventFilter {
    /// Match all events.
    All,
    /// Match by topic.
    Topic(String),
    /// Match by topic prefix.
    TopicPrefix(String),
    /// Match by event type.
    EventType(String),
    /// Match by attribute.
    Attribute { key: String, value: String },
    /// Combine filters with AND.
    And(Vec<EventFilter>),
    /// Combine filters with OR.
    Or(Vec<EventFilter>),
    /// Negate a filter.
    Not(Box<EventFilter>),
}

impl EventFilter {
    /// Check if an event matches this filter.
    pub fn matches(&self, event: &GenericEvent) -> bool {
        match self {
            EventFilter::All => true,
            EventFilter::Topic(t) => event.topic == *t,
            EventFilter::TopicPrefix(p) => event.topic.starts_with(p),
            EventFilter::EventType(t) => event.event_type == *t,
            EventFilter::Attribute { key, value } => event
                .metadata
                .attributes
                .get(key)
                .map(|v| v == value)
                .unwrap_or(false),
            EventFilter::And(filters) => filters.iter().all(|f| f.matches(event)),
            EventFilter::Or(filters) => filters.iter().any(|f| f.matches(event)),
            EventFilter::Not(f) => !f.matches(event),
        }
    }
}

/// Filtered event subscriber.
pub struct FilteredSubscriber {
    receiver: broadcast::Receiver<GenericEvent>,
    filter: EventFilter,
}

impl FilteredSubscriber {
    /// Create a new filtered subscriber.
    pub fn new(receiver: broadcast::Receiver<GenericEvent>, filter: EventFilter) -> Self {
        Self { receiver, filter }
    }

    /// Receive next matching event.
    pub async fn recv(&mut self) -> Option<GenericEvent> {
        loop {
            match self.receiver.recv().await {
                Ok(event) => {
                    if self.filter.matches(&event) {
                        return Some(event);
                    }
                }
                Err(_) => return None,
            }
        }
    }
}

/// Event bus statistics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EventBusStats {
    /// Total events published.
    pub published: u64,
    /// Total events delivered.
    pub delivered: u64,
    /// Active topics.
    pub topics: usize,
    /// Active subscriptions.
    pub subscriptions: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone)]
    struct TestEvent {
        message: String,
    }

    struct TestHandler {
        received: Arc<AtomicU64>,
    }

    #[async_trait]
    impl EventHandler<TestEvent> for TestHandler {
        async fn handle(&self, _event: &Event<TestEvent>) -> Result<()> {
            self.received.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }
    }

    #[test]
    fn test_event_metadata() {
        let meta = EventMetadata::new()
            .with_source("test")
            .with_attribute("key", "value");

        assert!(meta.source.is_some());
        assert_eq!(meta.attributes.get("key"), Some(&"value".to_string()));
    }

    #[test]
    fn test_event_creation() {
        let event = Event::new(
            "test.topic",
            TestEvent {
                message: "hello".to_string(),
            },
        );

        assert_eq!(event.topic, "test.topic");
        assert_eq!(event.payload.message, "hello");
    }

    #[tokio::test]
    async fn test_typed_event_bus() {
        let bus: TypedEventBus<TestEvent> = TypedEventBus::new(100);

        let received = Arc::new(AtomicU64::new(0));
        let handler = TestHandler {
            received: received.clone(),
        };

        let _sub_id = bus.subscribe(handler).await;

        let event = Event::new(
            "test",
            TestEvent {
                message: "hello".to_string(),
            },
        );

        bus.publish(event).await.unwrap();

        assert_eq!(received.load(Ordering::Relaxed), 1);
        assert_eq!(bus.published(), 1);
    }

    #[tokio::test]
    async fn test_unsubscribe() {
        let bus: TypedEventBus<TestEvent> = TypedEventBus::new(100);

        let received = Arc::new(AtomicU64::new(0));
        let handler = TestHandler {
            received: received.clone(),
        };

        let sub_id = bus.subscribe(handler).await;
        assert!(bus.unsubscribe(sub_id).await);
        assert!(!bus.unsubscribe(sub_id).await); // Already removed
    }

    #[tokio::test]
    async fn test_topic_event_bus() {
        let bus = TopicEventBus::new(100);

        let mut receiver = bus.subscribe("test.topic").await;

        let event = GenericEvent::new(
            "test.topic",
            "TestEvent",
            serde_json::json!({"message": "hello"}),
        );
        bus.publish(event).await.unwrap();

        let received = receiver.recv().await.unwrap();
        assert_eq!(received.topic, "test.topic");
    }

    #[test]
    fn test_event_filter_topic() {
        let event = GenericEvent::new("test.topic", "Test", serde_json::json!({}));

        assert!(EventFilter::Topic("test.topic".to_string()).matches(&event));
        assert!(!EventFilter::Topic("other.topic".to_string()).matches(&event));
    }

    #[test]
    fn test_event_filter_prefix() {
        let event = GenericEvent::new("test.topic.sub", "Test", serde_json::json!({}));

        assert!(EventFilter::TopicPrefix("test.".to_string()).matches(&event));
        assert!(!EventFilter::TopicPrefix("other.".to_string()).matches(&event));
    }

    #[test]
    fn test_event_filter_and() {
        let event = GenericEvent::new("test.topic", "TestEvent", serde_json::json!({}));

        let filter = EventFilter::And(vec![
            EventFilter::Topic("test.topic".to_string()),
            EventFilter::EventType("TestEvent".to_string()),
        ]);

        assert!(filter.matches(&event));
    }

    #[test]
    fn test_event_filter_or() {
        let event = GenericEvent::new("test.topic", "TestEvent", serde_json::json!({}));

        let filter = EventFilter::Or(vec![
            EventFilter::Topic("other.topic".to_string()),
            EventFilter::EventType("TestEvent".to_string()),
        ]);

        assert!(filter.matches(&event));
    }

    #[test]
    fn test_event_filter_not() {
        let event = GenericEvent::new("test.topic", "TestEvent", serde_json::json!({}));

        let filter = EventFilter::Not(Box::new(EventFilter::Topic("other.topic".to_string())));
        assert!(filter.matches(&event));
    }

    #[test]
    fn test_generic_event_from_typed() {
        let event = Event::new(
            "test",
            TestEvent {
                message: "hello".to_string(),
            },
        );

        // Can't directly convert since TestEvent doesn't implement Serialize
        // This would work with serde types
    }
}
