//! Pub/sub messaging for drbot.
//!
//! This crate provides:
//! - Topic-based publish/subscribe
//! - Pattern-based subscriptions
//! - Message persistence options
//! - Delivery guarantees

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::{broadcast, mpsc, RwLock};
use uuid::Uuid;

/// Pub/sub error types.
#[derive(Error, Debug)]
pub enum PubSubError {
    #[error("Topic not found: {0}")]
    TopicNotFound(String),

    #[error("No subscribers")]
    NoSubscribers,

    #[error("Publish failed: {0}")]
    PublishFailed(String),

    #[error("Subscribe failed: {0}")]
    SubscribeFailed(String),

    #[error("Channel closed")]
    ChannelClosed,
}

/// Result type for pub/sub operations.
pub type Result<T> = std::result::Result<T, PubSubError>;

/// Message delivery guarantee.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeliveryGuarantee {
    /// Best effort, messages may be lost.
    AtMostOnce,
    /// Messages delivered at least once, may be duplicated.
    AtLeastOnce,
    /// Messages delivered exactly once.
    ExactlyOnce,
}

/// A pub/sub message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Unique message ID.
    pub id: Uuid,
    /// Topic.
    pub topic: String,
    /// Message payload.
    pub payload: serde_json::Value,
    /// Headers.
    pub headers: HashMap<String, String>,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
    /// Delivery guarantee.
    pub guarantee: DeliveryGuarantee,
    /// Sequence number (for ordering).
    pub sequence: u64,
}

impl Message {
    /// Create a new message.
    pub fn new(topic: impl Into<String>, payload: serde_json::Value) -> Self {
        Self {
            id: Uuid::new_v4(),
            topic: topic.into(),
            payload,
            headers: HashMap::new(),
            timestamp: Utc::now(),
            guarantee: DeliveryGuarantee::AtMostOnce,
            sequence: 0,
        }
    }

    /// Add a header.
    pub fn with_header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(key.into(), value.into());
        self
    }

    /// Set delivery guarantee.
    pub fn with_guarantee(mut self, guarantee: DeliveryGuarantee) -> Self {
        self.guarantee = guarantee;
        self
    }
}

/// Subscription pattern.
#[derive(Debug, Clone)]
pub enum SubscriptionPattern {
    /// Exact topic match.
    Exact(String),
    /// Prefix match (e.g., "events.*").
    Prefix(String),
    /// Wildcard match (e.g., "events.*.created").
    Wildcard(String),
    /// Match all topics.
    All,
}

impl SubscriptionPattern {
    /// Check if a topic matches this pattern.
    pub fn matches(&self, topic: &str) -> bool {
        match self {
            SubscriptionPattern::Exact(t) => topic == t,
            SubscriptionPattern::Prefix(p) => topic.starts_with(p),
            SubscriptionPattern::Wildcard(pattern) => {
                let parts: Vec<&str> = pattern.split('.').collect();
                let topic_parts: Vec<&str> = topic.split('.').collect();

                if parts.len() != topic_parts.len() {
                    return false;
                }

                parts
                    .iter()
                    .zip(topic_parts.iter())
                    .all(|(p, t)| *p == "*" || p == t)
            }
            SubscriptionPattern::All => true,
        }
    }
}

/// Subscription ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SubscriptionId(u64);

/// A subscription.
pub struct Subscription {
    /// Subscription ID.
    pub id: SubscriptionId,
    /// Pattern.
    pub pattern: SubscriptionPattern,
    /// Message receiver.
    receiver: mpsc::Receiver<Message>,
}

impl Subscription {
    /// Receive the next message.
    pub async fn recv(&mut self) -> Option<Message> {
        self.receiver.recv().await
    }
}

/// Topic configuration.
#[derive(Debug, Clone)]
pub struct TopicConfig {
    /// Maximum message retention.
    pub retention: Option<std::time::Duration>,
    /// Maximum messages to retain.
    pub max_messages: Option<usize>,
    /// Default delivery guarantee.
    pub default_guarantee: DeliveryGuarantee,
}

impl Default for TopicConfig {
    fn default() -> Self {
        Self {
            retention: None,
            max_messages: None,
            default_guarantee: DeliveryGuarantee::AtMostOnce,
        }
    }
}

/// Topic state.
struct TopicState {
    config: TopicConfig,
    sender: broadcast::Sender<Message>,
    sequence: AtomicU64,
    message_count: AtomicU64,
}

/// Subscriber state.
struct SubscriberState {
    pattern: SubscriptionPattern,
    sender: mpsc::Sender<Message>,
}

/// Pub/sub broker.
pub struct PubSubBroker {
    topics: RwLock<HashMap<String, Arc<TopicState>>>,
    subscribers: RwLock<HashMap<SubscriptionId, SubscriberState>>,
    next_sub_id: AtomicU64,
    channel_capacity: usize,
    published: AtomicU64,
    delivered: AtomicU64,
}

impl PubSubBroker {
    /// Create a new broker.
    pub fn new(channel_capacity: usize) -> Self {
        Self {
            topics: RwLock::new(HashMap::new()),
            subscribers: RwLock::new(HashMap::new()),
            next_sub_id: AtomicU64::new(1),
            channel_capacity,
            published: AtomicU64::new(0),
            delivered: AtomicU64::new(0),
        }
    }

    /// Create a topic.
    pub async fn create_topic(&self, name: impl Into<String>, config: TopicConfig) {
        let (sender, _) = broadcast::channel(self.channel_capacity);
        let state = Arc::new(TopicState {
            config,
            sender,
            sequence: AtomicU64::new(0),
            message_count: AtomicU64::new(0),
        });

        let mut topics = self.topics.write().await;
        topics.insert(name.into(), state);
    }

    /// Publish a message.
    pub async fn publish(&self, mut message: Message) -> Result<u64> {
        let topics = self.topics.read().await;

        // Get or create topic
        let topic_state = if let Some(state) = topics.get(&message.topic) {
            state.clone()
        } else {
            drop(topics);
            self.create_topic(&message.topic, TopicConfig::default())
                .await;
            let topics = self.topics.read().await;
            topics.get(&message.topic).unwrap().clone()
        };

        // Set sequence number
        message.sequence = topic_state.sequence.fetch_add(1, Ordering::SeqCst);
        topic_state.message_count.fetch_add(1, Ordering::Relaxed);

        self.published.fetch_add(1, Ordering::Relaxed);

        // Broadcast to topic subscribers
        let _ = topic_state.sender.send(message.clone());

        // Deliver to pattern subscribers
        let subscribers = self.subscribers.read().await;
        let mut delivered = 0u64;

        for (_, sub) in subscribers.iter() {
            if sub.pattern.matches(&message.topic) {
                if sub.sender.send(message.clone()).await.is_ok() {
                    delivered += 1;
                }
            }
        }

        self.delivered.fetch_add(delivered, Ordering::Relaxed);
        Ok(delivered)
    }

    /// Subscribe with a pattern.
    pub async fn subscribe(&self, pattern: SubscriptionPattern) -> Subscription {
        let id = SubscriptionId(self.next_sub_id.fetch_add(1, Ordering::SeqCst));
        let (sender, receiver) = mpsc::channel(self.channel_capacity);

        let state = SubscriberState {
            pattern: pattern.clone(),
            sender,
        };

        let mut subscribers = self.subscribers.write().await;
        subscribers.insert(id, state);

        Subscription {
            id,
            pattern,
            receiver,
        }
    }

    /// Subscribe to a specific topic.
    pub async fn subscribe_topic(&self, topic: impl Into<String>) -> Subscription {
        self.subscribe(SubscriptionPattern::Exact(topic.into()))
            .await
    }

    /// Unsubscribe.
    pub async fn unsubscribe(&self, id: SubscriptionId) -> bool {
        let mut subscribers = self.subscribers.write().await;
        subscribers.remove(&id).is_some()
    }

    /// Get list of topics.
    pub async fn topics(&self) -> Vec<String> {
        self.topics.read().await.keys().cloned().collect()
    }

    /// Get published count.
    pub fn published(&self) -> u64 {
        self.published.load(Ordering::Relaxed)
    }

    /// Get delivered count.
    pub fn delivered(&self) -> u64 {
        self.delivered.load(Ordering::Relaxed)
    }

    /// Get subscriber count.
    pub async fn subscriber_count(&self) -> usize {
        self.subscribers.read().await.len()
    }
}

impl Default for PubSubBroker {
    fn default() -> Self {
        Self::new(1000)
    }
}

/// Message acknowledgment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Acknowledgment {
    /// Message ID.
    pub message_id: Uuid,
    /// Subscriber ID.
    pub subscriber_id: SubscriptionId,
    /// Acknowledged at.
    pub timestamp: DateTime<Utc>,
    /// Success or failure.
    pub success: bool,
    /// Error message if failed.
    pub error: Option<String>,
}

impl Acknowledgment {
    /// Create a success acknowledgment.
    pub fn success(message_id: Uuid, subscriber_id: SubscriptionId) -> Self {
        Self {
            message_id,
            subscriber_id,
            timestamp: Utc::now(),
            success: true,
            error: None,
        }
    }

    /// Create a failure acknowledgment.
    pub fn failure(
        message_id: Uuid,
        subscriber_id: SubscriptionId,
        error: impl Into<String>,
    ) -> Self {
        Self {
            message_id,
            subscriber_id,
            timestamp: Utc::now(),
            success: false,
            error: Some(error.into()),
        }
    }
}

/// Pub/sub statistics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PubSubStats {
    /// Total messages published.
    pub published: u64,
    /// Total messages delivered.
    pub delivered: u64,
    /// Number of topics.
    pub topics: usize,
    /// Number of subscribers.
    pub subscribers: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_creation() {
        let msg = Message::new("test.topic", serde_json::json!({"key": "value"}))
            .with_header("content-type", "application/json")
            .with_guarantee(DeliveryGuarantee::AtLeastOnce);

        assert_eq!(msg.topic, "test.topic");
        assert_eq!(
            msg.headers.get("content-type"),
            Some(&"application/json".to_string())
        );
        assert_eq!(msg.guarantee, DeliveryGuarantee::AtLeastOnce);
    }

    #[test]
    fn test_subscription_pattern_exact() {
        let pattern = SubscriptionPattern::Exact("events.user.created".to_string());

        assert!(pattern.matches("events.user.created"));
        assert!(!pattern.matches("events.user.updated"));
    }

    #[test]
    fn test_subscription_pattern_prefix() {
        let pattern = SubscriptionPattern::Prefix("events.user.".to_string());

        assert!(pattern.matches("events.user.created"));
        assert!(pattern.matches("events.user.updated"));
        assert!(!pattern.matches("events.order.created"));
    }

    #[test]
    fn test_subscription_pattern_wildcard() {
        let pattern = SubscriptionPattern::Wildcard("events.*.created".to_string());

        assert!(pattern.matches("events.user.created"));
        assert!(pattern.matches("events.order.created"));
        assert!(!pattern.matches("events.user.updated"));
    }

    #[test]
    fn test_subscription_pattern_all() {
        let pattern = SubscriptionPattern::All;

        assert!(pattern.matches("any.topic"));
        assert!(pattern.matches("another.topic.here"));
    }

    #[tokio::test]
    async fn test_broker_publish_subscribe() {
        let broker = PubSubBroker::new(100);

        let mut sub = broker.subscribe_topic("test.topic").await;

        let msg = Message::new("test.topic", serde_json::json!({"test": true}));
        broker.publish(msg).await.unwrap();

        let received = tokio::time::timeout(std::time::Duration::from_millis(100), sub.recv())
            .await
            .unwrap();

        assert!(received.is_some());
        let received = received.unwrap();
        assert_eq!(received.topic, "test.topic");
    }

    #[tokio::test]
    async fn test_pattern_subscription() {
        let broker = PubSubBroker::new(100);

        let mut sub = broker
            .subscribe(SubscriptionPattern::Prefix("events.".to_string()))
            .await;

        // Should receive
        let msg1 = Message::new("events.created", serde_json::json!({}));
        broker.publish(msg1).await.unwrap();

        // Should not receive
        let msg2 = Message::new("orders.created", serde_json::json!({}));
        broker.publish(msg2).await.unwrap();

        let received = tokio::time::timeout(std::time::Duration::from_millis(100), sub.recv())
            .await
            .unwrap();

        assert!(received.is_some());
        assert_eq!(received.unwrap().topic, "events.created");
    }

    #[tokio::test]
    async fn test_unsubscribe() {
        let broker = PubSubBroker::new(100);

        let sub = broker.subscribe_topic("test").await;
        let id = sub.id;

        assert_eq!(broker.subscriber_count().await, 1);
        assert!(broker.unsubscribe(id).await);
        assert_eq!(broker.subscriber_count().await, 0);
    }

    #[tokio::test]
    async fn test_create_topic() {
        let broker = PubSubBroker::new(100);

        broker
            .create_topic("my.topic", TopicConfig::default())
            .await;

        let topics = broker.topics().await;
        assert!(topics.contains(&"my.topic".to_string()));
    }

    #[test]
    fn test_acknowledgment() {
        let msg_id = Uuid::new_v4();
        let sub_id = SubscriptionId(1);

        let ack = Acknowledgment::success(msg_id, sub_id);
        assert!(ack.success);
        assert!(ack.error.is_none());

        let nack = Acknowledgment::failure(msg_id, sub_id, "Processing failed");
        assert!(!nack.success);
        assert!(nack.error.is_some());
    }
}
