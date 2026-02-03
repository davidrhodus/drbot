//! Outbound webhook management for external integrations.
//!
//! This crate provides:
//! - Webhook registration
//! - Event routing
//! - Retry handling
//! - Delivery tracking

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;

/// Webhook errors.
#[derive(Debug, Error)]
pub enum WebhookError {
    #[error("Webhook not found: {0}")]
    NotFound(String),

    #[error("Delivery failed: {0}")]
    DeliveryFailed(String),

    #[error("Invalid URL: {0}")]
    InvalidUrl(String),

    #[error("Max retries exceeded: {0}")]
    MaxRetriesExceeded(String),
}

/// Result type for webhook operations.
pub type Result<T> = std::result::Result<T, WebhookError>;

/// Webhook definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Webhook {
    /// Webhook identifier.
    pub id: String,
    /// Webhook name.
    pub name: String,
    /// Target URL.
    pub url: String,
    /// Events to subscribe to.
    pub events: Vec<String>,
    /// Is active.
    pub active: bool,
    /// Secret for signing.
    pub secret: Option<String>,
    /// Headers to include.
    pub headers: HashMap<String, String>,
    /// Retry configuration.
    pub retry_config: RetryConfig,
    /// Created at.
    pub created_at: DateTime<Utc>,
    /// Last triggered.
    pub last_triggered: Option<DateTime<Utc>>,
}

/// Retry configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    /// Max retry attempts.
    pub max_attempts: u32,
    /// Initial delay in ms.
    pub initial_delay_ms: u64,
    /// Max delay in ms.
    pub max_delay_ms: u64,
    /// Backoff multiplier.
    pub backoff_multiplier: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_delay_ms: 1000,
            max_delay_ms: 60000,
            backoff_multiplier: 2.0,
        }
    }
}

/// Webhook event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookEvent {
    /// Event identifier.
    pub id: String,
    /// Event type.
    pub event_type: String,
    /// Event data.
    pub data: Value,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
    /// Source.
    pub source: String,
}

/// Delivery attempt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliveryAttempt {
    /// Attempt identifier.
    pub id: String,
    /// Webhook ID.
    pub webhook_id: String,
    /// Event ID.
    pub event_id: String,
    /// Attempt number.
    pub attempt: u32,
    /// Status code.
    pub status_code: Option<u16>,
    /// Success.
    pub success: bool,
    /// Error message.
    pub error: Option<String>,
    /// Response body.
    pub response_body: Option<String>,
    /// Duration in ms.
    pub duration_ms: u64,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
}

/// Delivery status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliveryStatus {
    /// Event ID.
    pub event_id: String,
    /// Webhook ID.
    pub webhook_id: String,
    /// Status.
    pub status: DeliveryState,
    /// Attempts.
    pub attempts: Vec<DeliveryAttempt>,
    /// Next retry at.
    pub next_retry_at: Option<DateTime<Utc>>,
}

/// Delivery states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeliveryState {
    Pending,
    Delivered,
    Failed,
    Retrying,
}

/// HTTP client for webhook delivery.
#[async_trait]
pub trait WebhookClient: Send + Sync {
    /// Send webhook request.
    async fn send(
        &self,
        url: &str,
        payload: &Value,
        headers: &HashMap<String, String>,
    ) -> Result<(u16, String)>;
}

/// The webhook manager.
pub struct WebhookManager {
    /// HTTP client.
    client: Arc<dyn WebhookClient>,
    /// Registered webhooks.
    webhooks: Arc<RwLock<HashMap<String, Webhook>>>,
    /// Delivery history.
    deliveries: Arc<RwLock<Vec<DeliveryAttempt>>>,
    /// Event channel.
    event_tx: broadcast::Sender<WebhookEvent>,
}

impl WebhookManager {
    /// Create a new webhook manager.
    pub fn new(client: Arc<dyn WebhookClient>) -> Self {
        let (event_tx, _) = broadcast::channel(1000);
        Self {
            client,
            webhooks: Arc::new(RwLock::new(HashMap::new())),
            deliveries: Arc::new(RwLock::new(Vec::new())),
            event_tx,
        }
    }

    /// Subscribe to events.
    pub fn subscribe(&self) -> broadcast::Receiver<WebhookEvent> {
        self.event_tx.subscribe()
    }

    /// Register a webhook.
    pub async fn register(&self, name: &str, url: &str, events: Vec<String>) -> Result<String> {
        // Validate URL
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return Err(WebhookError::InvalidUrl(url.to_string()));
        }

        let webhook = Webhook {
            id: Uuid::new_v4().to_string(),
            name: name.to_string(),
            url: url.to_string(),
            events,
            active: true,
            secret: Some(Uuid::new_v4().to_string()),
            headers: HashMap::new(),
            retry_config: RetryConfig::default(),
            created_at: Utc::now(),
            last_triggered: None,
        };

        let id = webhook.id.clone();
        let mut webhooks = self.webhooks.write().await;
        webhooks.insert(id.clone(), webhook);

        Ok(id)
    }

    /// Unregister a webhook.
    pub async fn unregister(&self, webhook_id: &str) -> Result<()> {
        let mut webhooks = self.webhooks.write().await;
        webhooks
            .remove(webhook_id)
            .ok_or_else(|| WebhookError::NotFound(webhook_id.to_string()))?;
        Ok(())
    }

    /// Update webhook.
    pub async fn update(
        &self,
        webhook_id: &str,
        url: Option<&str>,
        events: Option<Vec<String>>,
        active: Option<bool>,
    ) -> Result<()> {
        let mut webhooks = self.webhooks.write().await;
        let webhook = webhooks
            .get_mut(webhook_id)
            .ok_or_else(|| WebhookError::NotFound(webhook_id.to_string()))?;

        if let Some(u) = url {
            webhook.url = u.to_string();
        }
        if let Some(e) = events {
            webhook.events = e;
        }
        if let Some(a) = active {
            webhook.active = a;
        }

        Ok(())
    }

    /// Trigger an event.
    pub async fn trigger(
        &self,
        event_type: &str,
        data: Value,
        source: &str,
    ) -> Result<Vec<DeliveryStatus>> {
        let event = WebhookEvent {
            id: Uuid::new_v4().to_string(),
            event_type: event_type.to_string(),
            data,
            timestamp: Utc::now(),
            source: source.to_string(),
        };

        // Broadcast event
        let _ = self.event_tx.send(event.clone());

        // Find matching webhooks
        let webhooks = self.webhooks.read().await;
        let matching: Vec<_> = webhooks
            .values()
            .filter(|w| {
                w.active
                    && (w.events.contains(&event_type.to_string())
                        || w.events.contains(&"*".to_string()))
            })
            .cloned()
            .collect();
        drop(webhooks);

        // Deliver to each webhook
        let mut statuses = Vec::new();
        for webhook in matching {
            let status = self.deliver(&webhook, &event).await;
            statuses.push(status);
        }

        Ok(statuses)
    }

    /// Deliver event to webhook.
    async fn deliver(&self, webhook: &Webhook, event: &WebhookEvent) -> DeliveryStatus {
        let payload = serde_json::json!({
            "id": event.id,
            "type": event.event_type,
            "data": event.data,
            "timestamp": event.timestamp,
            "source": event.source,
        });

        let mut headers = webhook.headers.clone();
        headers.insert("Content-Type".to_string(), "application/json".to_string());
        if let Some(secret) = &webhook.secret {
            // Add signature header
            headers.insert(
                "X-Webhook-Signature".to_string(),
                format!("sha256={}", secret),
            );
        }

        let mut attempts = Vec::new();
        let mut delay_ms = webhook.retry_config.initial_delay_ms;

        for attempt_num in 1..=webhook.retry_config.max_attempts {
            let start = std::time::Instant::now();
            let result = self.client.send(&webhook.url, &payload, &headers).await;
            let duration_ms = start.elapsed().as_millis() as u64;

            let attempt = match result {
                Ok((status_code, response_body)) => {
                    let success = status_code >= 200 && status_code < 300;
                    DeliveryAttempt {
                        id: Uuid::new_v4().to_string(),
                        webhook_id: webhook.id.clone(),
                        event_id: event.id.clone(),
                        attempt: attempt_num,
                        status_code: Some(status_code),
                        success,
                        error: if success {
                            None
                        } else {
                            Some(format!("HTTP {}", status_code))
                        },
                        response_body: Some(response_body),
                        duration_ms,
                        timestamp: Utc::now(),
                    }
                }
                Err(e) => DeliveryAttempt {
                    id: Uuid::new_v4().to_string(),
                    webhook_id: webhook.id.clone(),
                    event_id: event.id.clone(),
                    attempt: attempt_num,
                    status_code: None,
                    success: false,
                    error: Some(e.to_string()),
                    response_body: None,
                    duration_ms,
                    timestamp: Utc::now(),
                },
            };

            // Record attempt
            let mut deliveries = self.deliveries.write().await;
            deliveries.push(attempt.clone());
            if deliveries.len() > 10000 {
                deliveries.drain(0..1000);
            }
            drop(deliveries);

            attempts.push(attempt.clone());

            if attempt.success {
                // Update last triggered
                let mut webhooks = self.webhooks.write().await;
                if let Some(w) = webhooks.get_mut(&webhook.id) {
                    w.last_triggered = Some(Utc::now());
                }

                return DeliveryStatus {
                    event_id: event.id.clone(),
                    webhook_id: webhook.id.clone(),
                    status: DeliveryState::Delivered,
                    attempts,
                    next_retry_at: None,
                };
            }

            // Wait before retry
            if attempt_num < webhook.retry_config.max_attempts {
                tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
                delay_ms = (delay_ms as f64 * webhook.retry_config.backoff_multiplier) as u64;
                delay_ms = delay_ms.min(webhook.retry_config.max_delay_ms);
            }
        }

        DeliveryStatus {
            event_id: event.id.clone(),
            webhook_id: webhook.id.clone(),
            status: DeliveryState::Failed,
            attempts,
            next_retry_at: None,
        }
    }

    /// Get webhook by ID.
    pub async fn get_webhook(&self, webhook_id: &str) -> Option<Webhook> {
        let webhooks = self.webhooks.read().await;
        webhooks.get(webhook_id).cloned()
    }

    /// List all webhooks.
    pub async fn list_webhooks(&self) -> Vec<Webhook> {
        let webhooks = self.webhooks.read().await;
        webhooks.values().cloned().collect()
    }

    /// Get delivery history.
    pub async fn get_deliveries(
        &self,
        webhook_id: Option<&str>,
        limit: usize,
    ) -> Vec<DeliveryAttempt> {
        let deliveries = self.deliveries.read().await;
        deliveries
            .iter()
            .rev()
            .filter(|d| webhook_id.map_or(true, |id| d.webhook_id == id))
            .take(limit)
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockClient {
        should_fail: bool,
    }

    #[async_trait]
    impl WebhookClient for MockClient {
        async fn send(
            &self,
            _url: &str,
            _payload: &Value,
            _headers: &HashMap<String, String>,
        ) -> Result<(u16, String)> {
            if self.should_fail {
                Err(WebhookError::DeliveryFailed(
                    "Connection refused".to_string(),
                ))
            } else {
                Ok((200, "OK".to_string()))
            }
        }
    }

    #[tokio::test]
    async fn test_register_webhook() {
        let client = Arc::new(MockClient { should_fail: false });
        let manager = WebhookManager::new(client);

        let id = manager
            .register(
                "Test",
                "https://example.com/webhook",
                vec!["user.created".to_string()],
            )
            .await
            .unwrap();

        let webhook = manager.get_webhook(&id).await.unwrap();
        assert_eq!(webhook.name, "Test");
    }

    #[tokio::test]
    async fn test_trigger_event() {
        let client = Arc::new(MockClient { should_fail: false });
        let manager = WebhookManager::new(client);

        manager
            .register(
                "Test",
                "https://example.com/webhook",
                vec!["test.event".to_string()],
            )
            .await
            .unwrap();

        let statuses = manager
            .trigger("test.event", serde_json::json!({"key": "value"}), "test")
            .await
            .unwrap();

        assert_eq!(statuses.len(), 1);
        assert_eq!(statuses[0].status, DeliveryState::Delivered);
    }

    #[tokio::test]
    async fn test_delivery_failure() {
        let client = Arc::new(MockClient { should_fail: true });
        let manager = WebhookManager::new(client);

        manager
            .register(
                "Test",
                "https://example.com/webhook",
                vec!["test.event".to_string()],
            )
            .await
            .unwrap();

        let statuses = manager
            .trigger("test.event", serde_json::json!({}), "test")
            .await
            .unwrap();

        assert_eq!(statuses[0].status, DeliveryState::Failed);
        assert_eq!(statuses[0].attempts.len(), 3); // Max retries
    }

    #[tokio::test]
    async fn test_event_filtering() {
        let client = Arc::new(MockClient { should_fail: false });
        let manager = WebhookManager::new(client);

        manager
            .register(
                "Test",
                "https://example.com/webhook",
                vec!["user.created".to_string()],
            )
            .await
            .unwrap();

        // This event type doesn't match
        let statuses = manager
            .trigger("user.deleted", serde_json::json!({}), "test")
            .await
            .unwrap();

        assert_eq!(statuses.len(), 0);
    }

    #[tokio::test]
    async fn test_wildcard_subscription() {
        let client = Arc::new(MockClient { should_fail: false });
        let manager = WebhookManager::new(client);

        manager
            .register("Test", "https://example.com/webhook", vec!["*".to_string()])
            .await
            .unwrap();

        let statuses = manager
            .trigger("any.event", serde_json::json!({}), "test")
            .await
            .unwrap();

        assert_eq!(statuses.len(), 1);
    }
}
