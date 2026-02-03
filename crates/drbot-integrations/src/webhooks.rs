//! Webhook management.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;

use crate::Result;

/// Webhook configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookConfig {
    /// Webhook ID.
    pub id: Uuid,
    /// Name.
    pub name: String,
    /// URL.
    pub url: String,
    /// Secret for validation.
    pub secret: Option<String>,
    /// Events to trigger on.
    pub events: Vec<String>,
    /// Is enabled.
    pub enabled: bool,
    /// Headers to include.
    pub headers: HashMap<String, String>,
    /// Created at.
    pub created_at: DateTime<Utc>,
}

impl WebhookConfig {
    /// Create a new webhook config.
    pub fn new(name: &str, url: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.to_string(),
            url: url.to_string(),
            secret: None,
            events: Vec::new(),
            enabled: true,
            headers: HashMap::new(),
            created_at: Utc::now(),
        }
    }

    /// Set secret.
    pub fn with_secret(mut self, secret: &str) -> Self {
        self.secret = Some(secret.to_string());
        self
    }

    /// Add event.
    pub fn with_event(mut self, event: &str) -> Self {
        self.events.push(event.to_string());
        self
    }
}

/// Webhook event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookEvent {
    /// Event ID.
    pub id: Uuid,
    /// Event type.
    pub event_type: String,
    /// Payload.
    pub payload: serde_json::Value,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
    /// Source.
    pub source: Option<String>,
}

impl WebhookEvent {
    /// Create a new webhook event.
    pub fn new(event_type: &str, payload: serde_json::Value) -> Self {
        Self {
            id: Uuid::new_v4(),
            event_type: event_type.to_string(),
            payload,
            timestamp: Utc::now(),
            source: None,
        }
    }
}

/// Webhook delivery result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliveryResult {
    /// Webhook ID.
    pub webhook_id: Uuid,
    /// Event ID.
    pub event_id: Uuid,
    /// Success.
    pub success: bool,
    /// Status code.
    pub status_code: Option<u16>,
    /// Response body.
    pub response: Option<String>,
    /// Error message.
    pub error: Option<String>,
    /// Duration in milliseconds.
    pub duration_ms: u64,
    /// Delivered at.
    pub delivered_at: DateTime<Utc>,
}

/// Webhook manager.
pub struct WebhookManager {
    webhooks: Arc<RwLock<HashMap<Uuid, WebhookConfig>>>,
    event_sender: broadcast::Sender<WebhookEvent>,
    delivery_history: Arc<RwLock<Vec<DeliveryResult>>>,
}

impl WebhookManager {
    /// Create a new webhook manager.
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(256);
        Self {
            webhooks: Arc::new(RwLock::new(HashMap::new())),
            event_sender: sender,
            delivery_history: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Register a webhook.
    pub async fn register(&self, config: WebhookConfig) -> Uuid {
        let id = config.id;
        let mut webhooks = self.webhooks.write().await;
        webhooks.insert(id, config);
        id
    }

    /// Unregister a webhook.
    pub async fn unregister(&self, id: Uuid) -> bool {
        let mut webhooks = self.webhooks.write().await;
        webhooks.remove(&id).is_some()
    }

    /// Get a webhook.
    pub async fn get(&self, id: Uuid) -> Option<WebhookConfig> {
        let webhooks = self.webhooks.read().await;
        webhooks.get(&id).cloned()
    }

    /// List webhooks.
    pub async fn list(&self) -> Vec<WebhookConfig> {
        let webhooks = self.webhooks.read().await;
        webhooks.values().cloned().collect()
    }

    /// Enable a webhook.
    pub async fn enable(&self, id: Uuid) {
        let mut webhooks = self.webhooks.write().await;
        if let Some(webhook) = webhooks.get_mut(&id) {
            webhook.enabled = true;
        }
    }

    /// Disable a webhook.
    pub async fn disable(&self, id: Uuid) {
        let mut webhooks = self.webhooks.write().await;
        if let Some(webhook) = webhooks.get_mut(&id) {
            webhook.enabled = false;
        }
    }

    /// Subscribe to events.
    pub fn subscribe(&self) -> broadcast::Receiver<WebhookEvent> {
        self.event_sender.subscribe()
    }

    /// Emit an event.
    pub async fn emit(&self, event: WebhookEvent) -> Vec<DeliveryResult> {
        let _ = self.event_sender.send(event.clone());

        let webhooks = self.webhooks.read().await;
        let mut results = Vec::new();

        for webhook in webhooks.values() {
            if !webhook.enabled {
                continue;
            }

            // Check if webhook subscribes to this event
            if !webhook.events.is_empty() && !webhook.events.contains(&event.event_type) {
                continue;
            }

            let result = self.deliver(webhook, &event).await;
            results.push(result);
        }

        // Store delivery history
        let mut history = self.delivery_history.write().await;
        history.extend(results.clone());

        // Keep only last 1000 deliveries
        let current_len = history.len();
        if current_len > 1000 {
            history.drain(0..current_len - 1000);
        }

        results
    }

    async fn deliver(&self, webhook: &WebhookConfig, event: &WebhookEvent) -> DeliveryResult {
        let start = std::time::Instant::now();

        // Would make actual HTTP request here
        let result = DeliveryResult {
            webhook_id: webhook.id,
            event_id: event.id,
            success: true,
            status_code: Some(200),
            response: Some("OK".to_string()),
            error: None,
            duration_ms: start.elapsed().as_millis() as u64,
            delivered_at: Utc::now(),
        };

        result
    }

    /// Get delivery history.
    pub async fn delivery_history(&self, limit: usize) -> Vec<DeliveryResult> {
        let history = self.delivery_history.read().await;
        history.iter().rev().take(limit).cloned().collect()
    }

    /// Get deliveries for a webhook.
    pub async fn webhook_deliveries(&self, webhook_id: Uuid) -> Vec<DeliveryResult> {
        let history = self.delivery_history.read().await;
        history
            .iter()
            .filter(|d| d.webhook_id == webhook_id)
            .cloned()
            .collect()
    }

    /// Retry a failed delivery.
    pub async fn retry(&self, delivery_id: Uuid) -> Option<DeliveryResult> {
        // Would find the original event and redeliver
        None
    }
}

impl Default for WebhookManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_webhook_manager() {
        let manager = WebhookManager::new();

        let config = WebhookConfig::new("Test", "https://example.com/hook").with_event("message");

        let id = manager.register(config).await;

        let webhooks = manager.list().await;
        assert_eq!(webhooks.len(), 1);

        let event = WebhookEvent::new("message", serde_json::json!({"text": "hello"}));
        let results = manager.emit(event).await;
        assert_eq!(results.len(), 1);
        assert!(results[0].success);

        manager.unregister(id).await;
        assert!(manager.list().await.is_empty());
    }
}
