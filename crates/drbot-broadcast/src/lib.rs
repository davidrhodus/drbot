//! Broadcast channel utilities for drbot.
//!
//! This crate provides:
//! - Broadcast channels
//! - Topic-based pub/sub
//! - Subscriber management
//! - Message filtering

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use thiserror::Error;
use tokio::sync::broadcast;

/// Broadcast error types.
#[derive(Error, Debug)]
pub enum BroadcastError {
    #[error("Channel closed")]
    Closed,

    #[error("Send failed: {0}")]
    SendFailed(String),

    #[error("Receive failed")]
    ReceiveFailed,

    #[error("No subscribers")]
    NoSubscribers,

    #[error("Topic not found: {0}")]
    TopicNotFound(String),
}

/// Result type for broadcast operations.
pub type Result<T> = std::result::Result<T, BroadcastError>;

/// Subscriber ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SubscriberId(u64);

impl SubscriberId {
    /// Generate new subscriber ID.
    pub fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        Self(COUNTER.fetch_add(1, Ordering::SeqCst))
    }
}

impl Default for SubscriberId {
    fn default() -> Self {
        Self::new()
    }
}

/// Broadcast sender.
#[derive(Clone)]
pub struct Broadcaster<T: Clone> {
    sender: broadcast::Sender<T>,
    subscriber_count: Arc<AtomicU64>,
}

impl<T: Clone> Broadcaster<T> {
    /// Create new broadcaster with capacity.
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self {
            sender,
            subscriber_count: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Send to all subscribers.
    ///
    /// Tokio broadcast channels return an error when there are no receivers.
    /// For our use-cases, that's equivalent to delivering to 0 subscribers.
    pub fn send(&self, value: T) -> Result<usize> {
        match self.sender.send(value) {
            Ok(n) => Ok(n),
            Err(_e) => Ok(0),
        }
    }

    /// Create a new subscriber.
    pub fn subscribe(&self) -> Subscriber<T> {
        self.subscriber_count.fetch_add(1, Ordering::SeqCst);
        Subscriber {
            receiver: self.sender.subscribe(),
            id: SubscriberId::new(),
            counter: self.subscriber_count.clone(),
        }
    }

    /// Get subscriber count.
    pub fn subscriber_count(&self) -> u64 {
        self.subscriber_count.load(Ordering::SeqCst)
    }

    /// Check if there are subscribers.
    pub fn has_subscribers(&self) -> bool {
        self.subscriber_count() > 0
    }
}

/// Broadcast subscriber.
pub struct Subscriber<T: Clone> {
    receiver: broadcast::Receiver<T>,
    id: SubscriberId,
    counter: Arc<AtomicU64>,
}

impl<T: Clone> Subscriber<T> {
    /// Receive next value.
    pub async fn recv(&mut self) -> Result<T> {
        self.receiver
            .recv()
            .await
            .map_err(|_| BroadcastError::ReceiveFailed)
    }

    /// Get subscriber ID.
    pub fn id(&self) -> SubscriberId {
        self.id
    }
}

impl<T: Clone> Drop for Subscriber<T> {
    fn drop(&mut self) {
        self.counter.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Topic-based pub/sub system.
pub struct TopicBroadcaster<T: Clone + Send + 'static> {
    topics: RwLock<HashMap<String, broadcast::Sender<T>>>,
    capacity: usize,
}

impl<T: Clone + Send + 'static> TopicBroadcaster<T> {
    /// Create new topic broadcaster.
    pub fn new(capacity: usize) -> Self {
        Self {
            topics: RwLock::new(HashMap::new()),
            capacity,
        }
    }

    /// Publish to topic.
    pub fn publish(&self, topic: &str, value: T) -> Result<usize> {
        let topics = self.topics.read().unwrap();
        if let Some(sender) = topics.get(topic) {
            sender
                .send(value)
                .map_err(|e| BroadcastError::SendFailed(e.to_string()))
        } else {
            Err(BroadcastError::TopicNotFound(topic.to_string()))
        }
    }

    /// Subscribe to topic.
    pub fn subscribe(&self, topic: &str) -> TopicSubscriber<T> {
        let mut topics = self.topics.write().unwrap();
        let sender = topics
            .entry(topic.to_string())
            .or_insert_with(|| broadcast::channel(self.capacity).0);

        TopicSubscriber {
            receiver: sender.subscribe(),
            topic: topic.to_string(),
            id: SubscriberId::new(),
        }
    }

    /// Create topic if not exists.
    pub fn create_topic(&self, topic: &str) {
        let mut topics = self.topics.write().unwrap();
        topics
            .entry(topic.to_string())
            .or_insert_with(|| broadcast::channel(self.capacity).0);
    }

    /// Check if topic exists.
    pub fn topic_exists(&self, topic: &str) -> bool {
        self.topics.read().unwrap().contains_key(topic)
    }

    /// Get all topics.
    pub fn topics(&self) -> Vec<String> {
        self.topics.read().unwrap().keys().cloned().collect()
    }

    /// Remove topic (if no subscribers).
    pub fn remove_topic(&self, topic: &str) -> bool {
        let mut topics = self.topics.write().unwrap();
        if let Some(sender) = topics.get(topic) {
            if sender.receiver_count() == 0 {
                topics.remove(topic);
                return true;
            }
        }
        false
    }
}

/// Topic subscriber.
pub struct TopicSubscriber<T: Clone> {
    receiver: broadcast::Receiver<T>,
    topic: String,
    id: SubscriberId,
}

impl<T: Clone> TopicSubscriber<T> {
    /// Receive next value.
    pub async fn recv(&mut self) -> Result<T> {
        self.receiver
            .recv()
            .await
            .map_err(|_| BroadcastError::ReceiveFailed)
    }

    /// Get topic name.
    pub fn topic(&self) -> &str {
        &self.topic
    }

    /// Get subscriber ID.
    pub fn id(&self) -> SubscriberId {
        self.id
    }
}

/// Multi-topic subscriber.
pub struct MultiTopicSubscriber<T: Clone + Send + 'static> {
    subscribers: Mutex<HashMap<String, broadcast::Receiver<T>>>,
    broadcaster: Arc<TopicBroadcaster<T>>,
}

impl<T: Clone + Send + 'static> MultiTopicSubscriber<T> {
    /// Create new multi-topic subscriber.
    pub fn new(broadcaster: Arc<TopicBroadcaster<T>>) -> Self {
        Self {
            subscribers: Mutex::new(HashMap::new()),
            broadcaster,
        }
    }

    /// Subscribe to topic.
    pub fn subscribe(&self, topic: &str) {
        let sub = self.broadcaster.subscribe(topic);
        self.subscribers
            .lock()
            .unwrap()
            .insert(topic.to_string(), sub.receiver);
    }

    /// Unsubscribe from topic.
    pub fn unsubscribe(&self, topic: &str) {
        self.subscribers.lock().unwrap().remove(topic);
    }

    /// Get subscribed topics.
    pub fn topics(&self) -> Vec<String> {
        self.subscribers.lock().unwrap().keys().cloned().collect()
    }
}

/// Filtered broadcast channel.
pub struct FilteredBroadcaster<T: Clone> {
    broadcaster: Broadcaster<T>,
}

impl<T: Clone> FilteredBroadcaster<T> {
    /// Create new filtered broadcaster.
    pub fn new(capacity: usize) -> Self {
        Self {
            broadcaster: Broadcaster::new(capacity),
        }
    }

    /// Send to all subscribers.
    pub fn send(&self, value: T) -> Result<usize> {
        self.broadcaster.send(value)
    }

    /// Subscribe with filter.
    pub fn subscribe_filtered<F>(&self, filter: F) -> FilteredSubscriber<T, F>
    where
        F: Fn(&T) -> bool,
    {
        FilteredSubscriber {
            subscriber: self.broadcaster.subscribe(),
            filter,
        }
    }
}

/// Filtered subscriber.
pub struct FilteredSubscriber<T: Clone, F: Fn(&T) -> bool> {
    subscriber: Subscriber<T>,
    filter: F,
}

impl<T: Clone, F: Fn(&T) -> bool> FilteredSubscriber<T, F> {
    /// Receive next matching value.
    pub async fn recv(&mut self) -> Result<T> {
        loop {
            let value = self.subscriber.recv().await?;
            if (self.filter)(&value) {
                return Ok(value);
            }
        }
    }
}

/// Broadcast with replay buffer.
pub struct ReplayBroadcaster<T: Clone> {
    broadcaster: Broadcaster<T>,
    buffer: Mutex<Vec<T>>,
    buffer_size: usize,
}

impl<T: Clone> ReplayBroadcaster<T> {
    /// Create new replay broadcaster.
    pub fn new(capacity: usize, buffer_size: usize) -> Self {
        Self {
            broadcaster: Broadcaster::new(capacity),
            buffer: Mutex::new(Vec::with_capacity(buffer_size)),
            buffer_size,
        }
    }

    /// Send and buffer.
    pub fn send(&self, value: T) -> Result<usize> {
        let mut buffer = self.buffer.lock().unwrap();
        if buffer.len() >= self.buffer_size {
            buffer.remove(0);
        }
        buffer.push(value.clone());
        drop(buffer);

        self.broadcaster.send(value)
    }

    /// Subscribe and replay buffer.
    pub fn subscribe(&self) -> (Subscriber<T>, Vec<T>) {
        let buffer = self.buffer.lock().unwrap().clone();
        (self.broadcaster.subscribe(), buffer)
    }

    /// Get buffer contents.
    pub fn buffer(&self) -> Vec<T> {
        self.buffer.lock().unwrap().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_broadcaster() {
        let broadcaster = Broadcaster::new(10);
        let mut sub1 = broadcaster.subscribe();
        let mut sub2 = broadcaster.subscribe();

        broadcaster.send(42).unwrap();

        assert_eq!(sub1.recv().await.unwrap(), 42);
        assert_eq!(sub2.recv().await.unwrap(), 42);
    }

    #[tokio::test]
    async fn test_subscriber_count() {
        let broadcaster = Broadcaster::<i32>::new(10);
        assert_eq!(broadcaster.subscriber_count(), 0);

        let _sub1 = broadcaster.subscribe();
        assert_eq!(broadcaster.subscriber_count(), 1);

        {
            let _sub2 = broadcaster.subscribe();
            assert_eq!(broadcaster.subscriber_count(), 2);
        }

        assert_eq!(broadcaster.subscriber_count(), 1);
    }

    #[tokio::test]
    async fn test_topic_broadcaster() {
        let broadcaster = TopicBroadcaster::new(10);

        let mut sub = broadcaster.subscribe("test-topic");
        broadcaster.publish("test-topic", "hello").unwrap();

        assert_eq!(sub.recv().await.unwrap(), "hello");
    }

    #[tokio::test]
    async fn test_filtered_subscriber() {
        let broadcaster = FilteredBroadcaster::new(10);
        let mut sub = broadcaster.subscribe_filtered(|x: &i32| *x > 10);

        broadcaster.send(5).unwrap();
        broadcaster.send(15).unwrap();

        assert_eq!(sub.recv().await.unwrap(), 15);
    }

    #[tokio::test]
    async fn test_replay_broadcaster() {
        let broadcaster = ReplayBroadcaster::new(10, 3);

        broadcaster.send(1).unwrap();
        broadcaster.send(2).unwrap();
        broadcaster.send(3).unwrap();

        let (_, replay) = broadcaster.subscribe();
        assert_eq!(replay, vec![1, 2, 3]);
    }
}
