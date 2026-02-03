//! Outbox pattern for reliable messaging in drbot.
//!
//! This crate provides:
//! - Transactional outbox storage
//! - Message publishing with at-least-once delivery
//! - Retry handling for failed publishes
//! - Cleanup of published messages

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Outbox error types.
#[derive(Error, Debug)]
pub enum OutboxError {
    #[error("Message not found: {0}")]
    NotFound(Uuid),

    #[error("Storage error: {0}")]
    StorageError(String),

    #[error("Publish error: {0}")]
    PublishError(String),

    #[error("Serialization error: {0}")]
    SerializationError(String),
}

/// Result type for outbox operations.
pub type Result<T> = std::result::Result<T, OutboxError>;

/// Outbox message status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageStatus {
    /// Message is pending publication.
    Pending,
    /// Message is being processed.
    Processing,
    /// Message was published successfully.
    Published,
    /// Message failed to publish.
    Failed,
}

/// An outbox message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutboxMessage {
    /// Unique message ID.
    pub id: Uuid,
    /// Aggregate ID (for ordering).
    pub aggregate_id: Option<String>,
    /// Message topic/destination.
    pub topic: String,
    /// Message type.
    pub message_type: String,
    /// Message payload.
    pub payload: serde_json::Value,
    /// Message headers.
    pub headers: HashMap<String, String>,
    /// Current status.
    pub status: MessageStatus,
    /// Retry count.
    pub retry_count: u32,
    /// Maximum retries allowed.
    pub max_retries: u32,
    /// Created timestamp.
    pub created_at: DateTime<Utc>,
    /// Updated timestamp.
    pub updated_at: DateTime<Utc>,
    /// Scheduled for processing at.
    pub scheduled_at: DateTime<Utc>,
    /// Published timestamp.
    pub published_at: Option<DateTime<Utc>>,
    /// Last error message.
    pub last_error: Option<String>,
}

impl OutboxMessage {
    /// Create a new outbox message.
    pub fn new(
        topic: impl Into<String>,
        message_type: impl Into<String>,
        payload: serde_json::Value,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            aggregate_id: None,
            topic: topic.into(),
            message_type: message_type.into(),
            payload,
            headers: HashMap::new(),
            status: MessageStatus::Pending,
            retry_count: 0,
            max_retries: 3,
            created_at: now,
            updated_at: now,
            scheduled_at: now,
            published_at: None,
            last_error: None,
        }
    }

    /// Set aggregate ID.
    pub fn with_aggregate_id(mut self, id: impl Into<String>) -> Self {
        self.aggregate_id = Some(id.into());
        self
    }

    /// Add a header.
    pub fn with_header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(key.into(), value.into());
        self
    }

    /// Set max retries.
    pub fn with_max_retries(mut self, max: u32) -> Self {
        self.max_retries = max;
        self
    }

    /// Schedule for later.
    pub fn scheduled_for(mut self, at: DateTime<Utc>) -> Self {
        self.scheduled_at = at;
        self
    }

    /// Check if retryable.
    pub fn is_retryable(&self) -> bool {
        self.retry_count < self.max_retries
    }
}

/// Message publisher trait.
#[async_trait]
pub trait MessagePublisher: Send + Sync {
    /// Publish a message.
    async fn publish(&self, message: &OutboxMessage) -> Result<()>;
}

/// Outbox storage trait.
#[async_trait]
pub trait OutboxStorage: Send + Sync {
    /// Store a message.
    async fn store(&self, message: OutboxMessage) -> Result<()>;

    /// Get pending messages ready for processing.
    async fn get_pending(&self, limit: usize) -> Result<Vec<OutboxMessage>>;

    /// Mark a message as processing.
    async fn mark_processing(&self, id: Uuid) -> Result<()>;

    /// Mark a message as published.
    async fn mark_published(&self, id: Uuid) -> Result<()>;

    /// Mark a message as failed.
    async fn mark_failed(&self, id: Uuid, error: &str) -> Result<()>;

    /// Schedule retry for a message.
    async fn schedule_retry(&self, id: Uuid, at: DateTime<Utc>) -> Result<()>;

    /// Delete published messages older than a duration.
    async fn cleanup(&self, older_than: Duration) -> Result<usize>;

    /// Get message by ID.
    async fn get(&self, id: Uuid) -> Result<Option<OutboxMessage>>;
}

/// In-memory outbox storage.
pub struct InMemoryOutboxStorage {
    messages: RwLock<HashMap<Uuid, OutboxMessage>>,
}

impl InMemoryOutboxStorage {
    /// Create a new in-memory storage.
    pub fn new() -> Self {
        Self {
            messages: RwLock::new(HashMap::new()),
        }
    }
}

impl Default for InMemoryOutboxStorage {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl OutboxStorage for InMemoryOutboxStorage {
    async fn store(&self, message: OutboxMessage) -> Result<()> {
        let mut messages = self.messages.write().await;
        messages.insert(message.id, message);
        Ok(())
    }

    async fn get_pending(&self, limit: usize) -> Result<Vec<OutboxMessage>> {
        let messages = self.messages.read().await;
        let now = Utc::now();

        let pending: Vec<_> = messages
            .values()
            .filter(|m| m.status == MessageStatus::Pending && m.scheduled_at <= now)
            .take(limit)
            .cloned()
            .collect();

        Ok(pending)
    }

    async fn mark_processing(&self, id: Uuid) -> Result<()> {
        let mut messages = self.messages.write().await;
        if let Some(msg) = messages.get_mut(&id) {
            msg.status = MessageStatus::Processing;
            msg.updated_at = Utc::now();
            Ok(())
        } else {
            Err(OutboxError::NotFound(id))
        }
    }

    async fn mark_published(&self, id: Uuid) -> Result<()> {
        let mut messages = self.messages.write().await;
        if let Some(msg) = messages.get_mut(&id) {
            msg.status = MessageStatus::Published;
            msg.published_at = Some(Utc::now());
            msg.updated_at = Utc::now();
            Ok(())
        } else {
            Err(OutboxError::NotFound(id))
        }
    }

    async fn mark_failed(&self, id: Uuid, error: &str) -> Result<()> {
        let mut messages = self.messages.write().await;
        if let Some(msg) = messages.get_mut(&id) {
            msg.status = MessageStatus::Failed;
            msg.last_error = Some(error.to_string());
            msg.retry_count += 1;
            msg.updated_at = Utc::now();
            Ok(())
        } else {
            Err(OutboxError::NotFound(id))
        }
    }

    async fn schedule_retry(&self, id: Uuid, at: DateTime<Utc>) -> Result<()> {
        let mut messages = self.messages.write().await;
        if let Some(msg) = messages.get_mut(&id) {
            msg.status = MessageStatus::Pending;
            msg.scheduled_at = at;
            msg.updated_at = Utc::now();
            Ok(())
        } else {
            Err(OutboxError::NotFound(id))
        }
    }

    async fn cleanup(&self, older_than: Duration) -> Result<usize> {
        let mut messages = self.messages.write().await;
        let cutoff = Utc::now() - older_than;

        let to_remove: Vec<_> = messages
            .iter()
            .filter(|(_, m)| {
                m.status == MessageStatus::Published
                    && m.published_at.map(|p| p < cutoff).unwrap_or(false)
            })
            .map(|(id, _)| *id)
            .collect();

        let count = to_remove.len();
        for id in to_remove {
            messages.remove(&id);
        }

        Ok(count)
    }

    async fn get(&self, id: Uuid) -> Result<Option<OutboxMessage>> {
        let messages = self.messages.read().await;
        Ok(messages.get(&id).cloned())
    }
}

/// Outbox processor configuration.
#[derive(Debug, Clone)]
pub struct OutboxProcessorConfig {
    /// Batch size for processing.
    pub batch_size: usize,
    /// Processing interval.
    pub interval: std::time::Duration,
    /// Retry delay (exponential backoff base).
    pub retry_delay: std::time::Duration,
}

impl Default for OutboxProcessorConfig {
    fn default() -> Self {
        Self {
            batch_size: 100,
            interval: std::time::Duration::from_secs(1),
            retry_delay: std::time::Duration::from_secs(5),
        }
    }
}

/// Outbox processor.
pub struct OutboxProcessor<S: OutboxStorage, P: MessagePublisher> {
    storage: Arc<S>,
    publisher: Arc<P>,
    config: OutboxProcessorConfig,
}

impl<S: OutboxStorage, P: MessagePublisher> OutboxProcessor<S, P> {
    /// Create a new processor.
    pub fn new(storage: Arc<S>, publisher: Arc<P>, config: OutboxProcessorConfig) -> Self {
        Self {
            storage,
            publisher,
            config,
        }
    }

    /// Process a batch of messages.
    pub async fn process_batch(&self) -> Result<usize> {
        let messages = self.storage.get_pending(self.config.batch_size).await?;
        let mut processed = 0;

        for message in messages {
            self.storage.mark_processing(message.id).await?;

            match self.publisher.publish(&message).await {
                Ok(()) => {
                    self.storage.mark_published(message.id).await?;
                    processed += 1;
                }
                Err(e) => {
                    self.storage.mark_failed(message.id, &e.to_string()).await?;

                    if message.is_retryable() {
                        let retry_delay = Duration::seconds(
                            (self.config.retry_delay.as_secs() as i64)
                                * (2_i64.pow(message.retry_count)),
                        );
                        let retry_at = Utc::now() + retry_delay;
                        self.storage.schedule_retry(message.id, retry_at).await?;
                    }
                }
            }
        }

        Ok(processed)
    }

    /// Run cleanup of old published messages.
    pub async fn cleanup(&self, older_than: Duration) -> Result<usize> {
        self.storage.cleanup(older_than).await
    }
}

/// Outbox service for storing and processing messages.
pub struct OutboxService<S: OutboxStorage> {
    storage: Arc<S>,
}

impl<S: OutboxStorage> OutboxService<S> {
    /// Create a new outbox service.
    pub fn new(storage: Arc<S>) -> Self {
        Self { storage }
    }

    /// Store a message in the outbox.
    pub async fn store(&self, message: OutboxMessage) -> Result<Uuid> {
        let id = message.id;
        self.storage.store(message).await?;
        Ok(id)
    }

    /// Get a message by ID.
    pub async fn get(&self, id: Uuid) -> Result<Option<OutboxMessage>> {
        self.storage.get(id).await
    }

    /// Get pending messages.
    pub async fn get_pending(&self, limit: usize) -> Result<Vec<OutboxMessage>> {
        self.storage.get_pending(limit).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct NoopPublisher;

    #[async_trait]
    impl MessagePublisher for NoopPublisher {
        async fn publish(&self, _message: &OutboxMessage) -> Result<()> {
            Ok(())
        }
    }

    struct FailingPublisher;

    #[async_trait]
    impl MessagePublisher for FailingPublisher {
        async fn publish(&self, _message: &OutboxMessage) -> Result<()> {
            Err(OutboxError::PublishError("Connection failed".to_string()))
        }
    }

    #[test]
    fn test_outbox_message_creation() {
        let message = OutboxMessage::new("topic", "type", serde_json::json!({"key": "value"}));

        assert_eq!(message.topic, "topic");
        assert_eq!(message.message_type, "type");
        assert_eq!(message.status, MessageStatus::Pending);
        assert_eq!(message.retry_count, 0);
    }

    #[test]
    fn test_outbox_message_builder() {
        let message = OutboxMessage::new("topic", "type", serde_json::json!({}))
            .with_aggregate_id("agg-123")
            .with_header("correlation-id", "corr-456")
            .with_max_retries(5);

        assert_eq!(message.aggregate_id, Some("agg-123".to_string()));
        assert_eq!(
            message.headers.get("correlation-id"),
            Some(&"corr-456".to_string())
        );
        assert_eq!(message.max_retries, 5);
    }

    #[tokio::test]
    async fn test_in_memory_storage() {
        let storage = InMemoryOutboxStorage::new();

        let message = OutboxMessage::new("topic", "type", serde_json::json!({}));
        let id = message.id;

        storage.store(message).await.unwrap();

        let retrieved = storage.get(id).await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().id, id);
    }

    #[tokio::test]
    async fn test_get_pending() {
        let storage = InMemoryOutboxStorage::new();

        let message1 = OutboxMessage::new("topic", "type", serde_json::json!({}));
        let message2 = OutboxMessage::new("topic", "type", serde_json::json!({}));

        storage.store(message1).await.unwrap();
        storage.store(message2).await.unwrap();

        let pending = storage.get_pending(10).await.unwrap();
        assert_eq!(pending.len(), 2);
    }

    #[tokio::test]
    async fn test_mark_published() {
        let storage = InMemoryOutboxStorage::new();

        let message = OutboxMessage::new("topic", "type", serde_json::json!({}));
        let id = message.id;

        storage.store(message).await.unwrap();
        storage.mark_published(id).await.unwrap();

        let retrieved = storage.get(id).await.unwrap().unwrap();
        assert_eq!(retrieved.status, MessageStatus::Published);
        assert!(retrieved.published_at.is_some());
    }

    #[tokio::test]
    async fn test_processor_success() {
        let storage = Arc::new(InMemoryOutboxStorage::new());
        let publisher = Arc::new(NoopPublisher);

        let message = OutboxMessage::new("topic", "type", serde_json::json!({}));
        let id = message.id;

        storage.store(message).await.unwrap();

        let processor =
            OutboxProcessor::new(storage.clone(), publisher, OutboxProcessorConfig::default());

        let processed = processor.process_batch().await.unwrap();
        assert_eq!(processed, 1);

        let retrieved = storage.get(id).await.unwrap().unwrap();
        assert_eq!(retrieved.status, MessageStatus::Published);
    }

    #[tokio::test]
    async fn test_processor_failure_retry() {
        let storage = Arc::new(InMemoryOutboxStorage::new());
        let publisher = Arc::new(FailingPublisher);

        let message = OutboxMessage::new("topic", "type", serde_json::json!({}));
        let id = message.id;

        storage.store(message).await.unwrap();

        let processor =
            OutboxProcessor::new(storage.clone(), publisher, OutboxProcessorConfig::default());

        let processed = processor.process_batch().await.unwrap();
        assert_eq!(processed, 0);

        let retrieved = storage.get(id).await.unwrap().unwrap();
        assert_eq!(retrieved.status, MessageStatus::Pending); // Scheduled for retry
        assert_eq!(retrieved.retry_count, 1);
    }

    #[test]
    fn test_retryable() {
        let mut message = OutboxMessage::new("topic", "type", serde_json::json!({}));
        message.max_retries = 3;

        assert!(message.is_retryable());

        message.retry_count = 3;
        assert!(!message.is_retryable());
    }
}
