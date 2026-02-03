//! Reliable message queuing with persistence.
//!
//! This crate provides:
//! - Persistent message queues
//! - At-least-once delivery
//! - Dead letter queues
//! - Priority queuing

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BinaryHeap, HashMap, VecDeque};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Queue errors.
#[derive(Debug, Error)]
pub enum QueueError {
    #[error("Queue not found: {0}")]
    QueueNotFound(String),

    #[error("Message not found: {0}")]
    MessageNotFound(String),

    #[error("Queue full: {0}")]
    QueueFull(String),

    #[error("Persistence error: {0}")]
    PersistenceError(String),
}

/// Result type for queue operations.
pub type Result<T> = std::result::Result<T, QueueError>;

/// A queued message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueMessage {
    /// Message identifier.
    pub id: String,
    /// Queue name.
    pub queue: String,
    /// Message payload.
    pub payload: String,
    /// Priority (higher = more urgent).
    pub priority: i32,
    /// Created timestamp.
    pub created_at: DateTime<Utc>,
    /// Scheduled delivery time.
    pub scheduled_at: Option<DateTime<Utc>>,
    /// Number of delivery attempts.
    pub attempts: u32,
    /// Maximum attempts before dead letter.
    pub max_attempts: u32,
    /// Last attempt time.
    pub last_attempt: Option<DateTime<Utc>>,
    /// Visibility timeout (when processing).
    pub visible_at: Option<DateTime<Utc>>,
    /// Message metadata.
    pub metadata: HashMap<String, String>,
}

impl PartialEq for QueueMessage {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for QueueMessage {}

impl PartialOrd for QueueMessage {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for QueueMessage {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Higher priority first, then older messages first
        match self.priority.cmp(&other.priority) {
            std::cmp::Ordering::Equal => other.created_at.cmp(&self.created_at),
            other => other,
        }
    }
}

/// Queue configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueConfig {
    /// Queue name.
    pub name: String,
    /// Maximum queue size.
    pub max_size: usize,
    /// Default visibility timeout (seconds).
    pub visibility_timeout: u64,
    /// Default max attempts.
    pub max_attempts: u32,
    /// Dead letter queue name.
    pub dead_letter_queue: Option<String>,
    /// Enable priority ordering.
    pub priority_enabled: bool,
}

impl Default for QueueConfig {
    fn default() -> Self {
        Self {
            name: "default".to_string(),
            max_size: 100000,
            visibility_timeout: 30,
            max_attempts: 3,
            dead_letter_queue: None,
            priority_enabled: false,
        }
    }
}

/// Queue statistics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct QueueStats {
    /// Total messages in queue.
    pub message_count: usize,
    /// Messages currently being processed.
    pub in_flight: usize,
    /// Total messages enqueued.
    pub total_enqueued: usize,
    /// Total messages processed.
    pub total_processed: usize,
    /// Total messages failed.
    pub total_failed: usize,
    /// Messages in dead letter queue.
    pub dead_letter_count: usize,
}

/// Queue persistence provider.
#[async_trait]
pub trait QueuePersistence: Send + Sync {
    /// Save message to storage.
    async fn save(&self, message: &QueueMessage) -> Result<()>;

    /// Load message from storage.
    async fn load(&self, id: &str) -> Result<Option<QueueMessage>>;

    /// Delete message from storage.
    async fn delete(&self, id: &str) -> Result<()>;

    /// Load all messages for a queue.
    async fn load_queue(&self, queue: &str) -> Result<Vec<QueueMessage>>;
}

/// Internal queue state.
struct QueueState {
    config: QueueConfig,
    messages: VecDeque<QueueMessage>,
    priority_messages: BinaryHeap<QueueMessage>,
    in_flight: HashMap<String, QueueMessage>,
    stats: QueueStats,
}

/// The message queue system.
pub struct MessageQueue {
    /// Persistence provider.
    persistence: Option<Arc<dyn QueuePersistence>>,
    /// Queue states.
    queues: Arc<RwLock<HashMap<String, QueueState>>>,
}

impl MessageQueue {
    /// Create a new message queue.
    pub fn new() -> Self {
        Self {
            persistence: None,
            queues: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Set persistence provider.
    pub fn with_persistence(mut self, persistence: Arc<dyn QueuePersistence>) -> Self {
        self.persistence = Some(persistence);
        self
    }

    /// Create a queue.
    pub async fn create_queue(&self, config: QueueConfig) -> Result<()> {
        let mut queues = self.queues.write().await;
        let name = config.name.clone();

        queues.insert(
            name,
            QueueState {
                config,
                messages: VecDeque::new(),
                priority_messages: BinaryHeap::new(),
                in_flight: HashMap::new(),
                stats: QueueStats::default(),
            },
        );

        Ok(())
    }

    /// Delete a queue.
    pub async fn delete_queue(&self, name: &str) -> Result<()> {
        let mut queues = self.queues.write().await;
        queues
            .remove(name)
            .ok_or_else(|| QueueError::QueueNotFound(name.to_string()))?;
        Ok(())
    }

    /// Enqueue a message.
    pub async fn enqueue(
        &self,
        queue: &str,
        payload: String,
        priority: Option<i32>,
        scheduled_at: Option<DateTime<Utc>>,
    ) -> Result<String> {
        let mut queues = self.queues.write().await;
        let state = queues
            .get_mut(queue)
            .ok_or_else(|| QueueError::QueueNotFound(queue.to_string()))?;

        let total_messages = state.messages.len() + state.priority_messages.len();
        if total_messages >= state.config.max_size {
            return Err(QueueError::QueueFull(queue.to_string()));
        }

        let message = QueueMessage {
            id: Uuid::new_v4().to_string(),
            queue: queue.to_string(),
            payload,
            priority: priority.unwrap_or(0),
            created_at: Utc::now(),
            scheduled_at,
            attempts: 0,
            max_attempts: state.config.max_attempts,
            last_attempt: None,
            visible_at: None,
            metadata: HashMap::new(),
        };

        let id = message.id.clone();

        // Persist if enabled
        if let Some(persistence) = &self.persistence {
            persistence.save(&message).await?;
        }

        // Add to appropriate queue
        if state.config.priority_enabled {
            state.priority_messages.push(message);
        } else {
            state.messages.push_back(message);
        }

        state.stats.total_enqueued += 1;
        state.stats.message_count += 1;

        Ok(id)
    }

    /// Dequeue a message for processing.
    pub async fn dequeue(&self, queue: &str) -> Result<Option<QueueMessage>> {
        let mut queues = self.queues.write().await;
        let state = queues
            .get_mut(queue)
            .ok_or_else(|| QueueError::QueueNotFound(queue.to_string()))?;

        let now = Utc::now();

        // Get next available message
        let message = if state.config.priority_enabled {
            // Find first visible message from priority queue
            let mut temp = BinaryHeap::new();
            let mut found = None;

            while let Some(mut msg) = state.priority_messages.pop() {
                // Check if scheduled and visible
                let is_scheduled = msg.scheduled_at.map(|s| s <= now).unwrap_or(true);
                let is_visible = msg.visible_at.map(|v| v <= now).unwrap_or(true);

                if found.is_none() && is_scheduled && is_visible {
                    msg.attempts += 1;
                    msg.last_attempt = Some(now);
                    msg.visible_at = Some(
                        now + chrono::Duration::seconds(state.config.visibility_timeout as i64),
                    );
                    found = Some(msg);
                } else {
                    temp.push(msg);
                }
            }

            state.priority_messages = temp;
            found
        } else {
            // Find first visible message from FIFO queue
            let mut found_idx = None;
            for (i, msg) in state.messages.iter().enumerate() {
                let is_scheduled = msg.scheduled_at.map(|s| s <= now).unwrap_or(true);
                let is_visible = msg.visible_at.map(|v| v <= now).unwrap_or(true);

                if is_scheduled && is_visible {
                    found_idx = Some(i);
                    break;
                }
            }

            found_idx.and_then(|i| {
                state.messages.remove(i).map(|mut msg| {
                    msg.attempts += 1;
                    msg.last_attempt = Some(now);
                    msg.visible_at = Some(
                        now + chrono::Duration::seconds(state.config.visibility_timeout as i64),
                    );
                    msg
                })
            })
        };

        // Track in-flight
        if let Some(ref msg) = message {
            state.in_flight.insert(msg.id.clone(), msg.clone());
            state.stats.in_flight += 1;
            state.stats.message_count = state.stats.message_count.saturating_sub(1);
        }

        Ok(message)
    }

    /// Acknowledge successful processing.
    pub async fn ack(&self, queue: &str, message_id: &str) -> Result<()> {
        let mut queues = self.queues.write().await;
        let state = queues
            .get_mut(queue)
            .ok_or_else(|| QueueError::QueueNotFound(queue.to_string()))?;

        state
            .in_flight
            .remove(message_id)
            .ok_or_else(|| QueueError::MessageNotFound(message_id.to_string()))?;

        state.stats.in_flight = state.stats.in_flight.saturating_sub(1);
        state.stats.total_processed += 1;

        // Remove from persistence
        if let Some(persistence) = &self.persistence {
            persistence.delete(message_id).await?;
        }

        Ok(())
    }

    /// Negative acknowledgement - return to queue.
    pub async fn nack(&self, queue: &str, message_id: &str) -> Result<()> {
        let mut queues = self.queues.write().await;
        let state = queues
            .get_mut(queue)
            .ok_or_else(|| QueueError::QueueNotFound(queue.to_string()))?;

        let mut message = state
            .in_flight
            .remove(message_id)
            .ok_or_else(|| QueueError::MessageNotFound(message_id.to_string()))?;

        state.stats.in_flight = state.stats.in_flight.saturating_sub(1);

        // Check if max attempts exceeded
        if message.attempts >= message.max_attempts {
            // Move to dead letter queue
            if let Some(dlq_name) = &state.config.dead_letter_queue.clone() {
                message.visible_at = None;
                drop(queues);

                // Re-acquire to add to DLQ
                let mut queues = self.queues.write().await;
                if let Some(dlq_state) = queues.get_mut(dlq_name) {
                    dlq_state.messages.push_back(message);
                    dlq_state.stats.message_count += 1;
                    dlq_state.stats.dead_letter_count += 1;
                }

                if let Some(state) = queues.get_mut(queue) {
                    state.stats.total_failed += 1;
                }
            } else {
                state.stats.total_failed += 1;
            }
        } else {
            // Return to queue
            message.visible_at = None;
            if state.config.priority_enabled {
                state.priority_messages.push(message);
            } else {
                state.messages.push_back(message);
            }
            state.stats.message_count += 1;
        }

        Ok(())
    }

    /// Get queue statistics.
    pub async fn stats(&self, queue: &str) -> Result<QueueStats> {
        let queues = self.queues.read().await;
        let state = queues
            .get(queue)
            .ok_or_else(|| QueueError::QueueNotFound(queue.to_string()))?;

        Ok(state.stats.clone())
    }

    /// Peek at next message without dequeuing.
    pub async fn peek(&self, queue: &str) -> Result<Option<QueueMessage>> {
        let queues = self.queues.read().await;
        let state = queues
            .get(queue)
            .ok_or_else(|| QueueError::QueueNotFound(queue.to_string()))?;

        let message = if state.config.priority_enabled {
            state.priority_messages.peek().cloned()
        } else {
            state.messages.front().cloned()
        };

        Ok(message)
    }

    /// Get queue length.
    pub async fn len(&self, queue: &str) -> Result<usize> {
        let queues = self.queues.read().await;
        let state = queues
            .get(queue)
            .ok_or_else(|| QueueError::QueueNotFound(queue.to_string()))?;

        let len = if state.config.priority_enabled {
            state.priority_messages.len()
        } else {
            state.messages.len()
        };

        Ok(len)
    }

    /// Purge all messages from a queue.
    pub async fn purge(&self, queue: &str) -> Result<usize> {
        let mut queues = self.queues.write().await;
        let state = queues
            .get_mut(queue)
            .ok_or_else(|| QueueError::QueueNotFound(queue.to_string()))?;

        let count = state.messages.len() + state.priority_messages.len();
        state.messages.clear();
        state.priority_messages.clear();
        state.stats.message_count = 0;

        Ok(count)
    }
}

impl Default for MessageQueue {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_queue() {
        let mq = MessageQueue::new();
        mq.create_queue(QueueConfig {
            name: "test".to_string(),
            ..Default::default()
        })
        .await
        .unwrap();

        let len = mq.len("test").await.unwrap();
        assert_eq!(len, 0);
    }

    #[tokio::test]
    async fn test_enqueue_dequeue() {
        let mq = MessageQueue::new();
        mq.create_queue(QueueConfig {
            name: "test".to_string(),
            ..Default::default()
        })
        .await
        .unwrap();

        let id = mq
            .enqueue("test", "hello".to_string(), None, None)
            .await
            .unwrap();
        assert!(!id.is_empty());

        let msg = mq.dequeue("test").await.unwrap().unwrap();
        assert_eq!(msg.payload, "hello");
    }

    #[tokio::test]
    async fn test_ack() {
        let mq = MessageQueue::new();
        mq.create_queue(QueueConfig {
            name: "test".to_string(),
            ..Default::default()
        })
        .await
        .unwrap();

        mq.enqueue("test", "hello".to_string(), None, None)
            .await
            .unwrap();
        let msg = mq.dequeue("test").await.unwrap().unwrap();

        mq.ack("test", &msg.id).await.unwrap();

        let stats = mq.stats("test").await.unwrap();
        assert_eq!(stats.total_processed, 1);
        assert_eq!(stats.in_flight, 0);
    }

    #[tokio::test]
    async fn test_priority_queue() {
        let mq = MessageQueue::new();
        mq.create_queue(QueueConfig {
            name: "test".to_string(),
            priority_enabled: true,
            ..Default::default()
        })
        .await
        .unwrap();

        mq.enqueue("test", "low".to_string(), Some(1), None)
            .await
            .unwrap();
        mq.enqueue("test", "high".to_string(), Some(10), None)
            .await
            .unwrap();
        mq.enqueue("test", "medium".to_string(), Some(5), None)
            .await
            .unwrap();

        let msg1 = mq.dequeue("test").await.unwrap().unwrap();
        assert_eq!(msg1.payload, "high");

        let msg2 = mq.dequeue("test").await.unwrap().unwrap();
        assert_eq!(msg2.payload, "medium");

        let msg3 = mq.dequeue("test").await.unwrap().unwrap();
        assert_eq!(msg3.payload, "low");
    }

    #[tokio::test]
    async fn test_nack_retry() {
        let mq = MessageQueue::new();
        mq.create_queue(QueueConfig {
            name: "test".to_string(),
            max_attempts: 3,
            visibility_timeout: 0,
            ..Default::default()
        })
        .await
        .unwrap();

        mq.enqueue("test", "retry".to_string(), None, None)
            .await
            .unwrap();

        // First attempt
        let msg = mq.dequeue("test").await.unwrap().unwrap();
        assert_eq!(msg.attempts, 1);
        mq.nack("test", &msg.id).await.unwrap();

        // Second attempt
        let msg = mq.dequeue("test").await.unwrap().unwrap();
        assert_eq!(msg.attempts, 2);
    }

    #[tokio::test]
    async fn test_purge() {
        let mq = MessageQueue::new();
        mq.create_queue(QueueConfig {
            name: "test".to_string(),
            ..Default::default()
        })
        .await
        .unwrap();

        for i in 0..5 {
            mq.enqueue("test", format!("msg{}", i), None, None)
                .await
                .unwrap();
        }

        let purged = mq.purge("test").await.unwrap();
        assert_eq!(purged, 5);

        let len = mq.len("test").await.unwrap();
        assert_eq!(len, 0);
    }
}
