//! Message envelope and routing for drbot.
//!
//! This crate provides:
//! - Envelope wrapping for messages
//! - Routing information
//! - Delivery tracking
//! - Acknowledgment handling

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

/// Envelope error types.
#[derive(Error, Debug)]
pub enum EnvelopeError {
    #[error("Invalid envelope: {0}")]
    Invalid(String),

    #[error("Routing error: {0}")]
    RoutingError(String),

    #[error("Delivery failed: {0}")]
    DeliveryFailed(String),
}

/// Result type for envelope operations.
pub type Result<T> = std::result::Result<T, EnvelopeError>;

/// Envelope ID.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EnvelopeId(pub String);

impl EnvelopeId {
    /// Generate new envelope ID.
    pub fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        use std::time::{SystemTime, UNIX_EPOCH};

        static COUNTER: AtomicU64 = AtomicU64::new(0);

        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;

        let seq = COUNTER.fetch_add(1, Ordering::SeqCst);

        Self(format!("env-{:x}-{:x}", ts, seq))
    }
}

impl Default for EnvelopeId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for EnvelopeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Delivery status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeliveryStatus {
    /// Pending delivery.
    Pending,
    /// In transit.
    InTransit,
    /// Delivered.
    Delivered,
    /// Failed.
    Failed,
    /// Rejected.
    Rejected,
    /// Expired.
    Expired,
}

/// Address for routing.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Address {
    /// Address type (e.g., "queue", "topic", "direct").
    pub address_type: String,
    /// Address value.
    pub value: String,
}

impl Address {
    /// Create new address.
    pub fn new(address_type: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            address_type: address_type.into(),
            value: value.into(),
        }
    }

    /// Create queue address.
    pub fn queue(name: impl Into<String>) -> Self {
        Self::new("queue", name)
    }

    /// Create topic address.
    pub fn topic(name: impl Into<String>) -> Self {
        Self::new("topic", name)
    }

    /// Create direct address.
    pub fn direct(name: impl Into<String>) -> Self {
        Self::new("direct", name)
    }
}

impl std::fmt::Display for Address {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}://{}", self.address_type, self.value)
    }
}

/// Routing information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Routing {
    /// Source address.
    pub from: Option<Address>,
    /// Destination address.
    pub to: Address,
    /// Reply-to address.
    pub reply_to: Option<Address>,
    /// Routing key.
    pub routing_key: Option<String>,
    /// Hop count.
    pub hop_count: u32,
    /// Max hops.
    pub max_hops: Option<u32>,
}

impl Routing {
    /// Create new routing.
    pub fn new(to: Address) -> Self {
        Self {
            from: None,
            to,
            reply_to: None,
            routing_key: None,
            hop_count: 0,
            max_hops: None,
        }
    }

    /// Set source.
    pub fn with_from(mut self, from: Address) -> Self {
        self.from = Some(from);
        self
    }

    /// Set reply-to.
    pub fn with_reply_to(mut self, reply_to: Address) -> Self {
        self.reply_to = Some(reply_to);
        self
    }

    /// Set routing key.
    pub fn with_routing_key(mut self, key: impl Into<String>) -> Self {
        self.routing_key = Some(key.into());
        self
    }

    /// Set max hops.
    pub fn with_max_hops(mut self, max: u32) -> Self {
        self.max_hops = Some(max);
        self
    }

    /// Increment hop count.
    pub fn increment_hop(&mut self) -> Result<()> {
        self.hop_count += 1;
        if let Some(max) = self.max_hops {
            if self.hop_count > max {
                return Err(EnvelopeError::RoutingError("Max hops exceeded".to_string()));
            }
        }
        Ok(())
    }
}

/// Delivery attempt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliveryAttempt {
    /// Attempt number.
    pub attempt: u32,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
    /// Status.
    pub status: DeliveryStatus,
    /// Error message if failed.
    pub error: Option<String>,
}

/// Delivery info.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliveryInfo {
    /// Current status.
    pub status: DeliveryStatus,
    /// Attempts.
    pub attempts: Vec<DeliveryAttempt>,
    /// Created at.
    pub created_at: DateTime<Utc>,
    /// Last updated.
    pub updated_at: DateTime<Utc>,
    /// Delivered at.
    pub delivered_at: Option<DateTime<Utc>>,
}

impl DeliveryInfo {
    /// Create new delivery info.
    pub fn new() -> Self {
        let now = Utc::now();
        Self {
            status: DeliveryStatus::Pending,
            attempts: Vec::new(),
            created_at: now,
            updated_at: now,
            delivered_at: None,
        }
    }

    /// Record attempt.
    pub fn record_attempt(&mut self, status: DeliveryStatus, error: Option<String>) {
        let attempt = DeliveryAttempt {
            attempt: self.attempts.len() as u32 + 1,
            timestamp: Utc::now(),
            status,
            error,
        };
        self.attempts.push(attempt);
        self.status = status;
        self.updated_at = Utc::now();

        if status == DeliveryStatus::Delivered {
            self.delivered_at = Some(Utc::now());
        }
    }

    /// Get attempt count.
    pub fn attempt_count(&self) -> usize {
        self.attempts.len()
    }
}

impl Default for DeliveryInfo {
    fn default() -> Self {
        Self::new()
    }
}

/// Message envelope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Envelope<T> {
    /// Envelope ID.
    pub id: EnvelopeId,
    /// Routing information.
    pub routing: Routing,
    /// Delivery information.
    pub delivery: DeliveryInfo,
    /// Payload.
    pub payload: T,
    /// Metadata.
    pub metadata: HashMap<String, serde_json::Value>,
    /// Acknowledgment required.
    pub ack_required: bool,
}

impl<T> Envelope<T> {
    /// Create new envelope.
    pub fn new(payload: T, to: Address) -> Self {
        Self {
            id: EnvelopeId::new(),
            routing: Routing::new(to),
            delivery: DeliveryInfo::new(),
            payload,
            metadata: HashMap::new(),
            ack_required: false,
        }
    }

    /// Set routing.
    pub fn with_routing(mut self, routing: Routing) -> Self {
        self.routing = routing;
        self
    }

    /// Set ack required.
    pub fn with_ack(mut self, required: bool) -> Self {
        self.ack_required = required;
        self
    }

    /// Add metadata.
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Serialize) -> Self {
        if let Ok(v) = serde_json::to_value(value) {
            self.metadata.insert(key.into(), v);
        }
        self
    }

    /// Get metadata.
    pub fn get_metadata(&self, key: &str) -> Option<&serde_json::Value> {
        self.metadata.get(key)
    }

    /// Mark as delivered.
    pub fn mark_delivered(&mut self) {
        self.delivery
            .record_attempt(DeliveryStatus::Delivered, None);
    }

    /// Mark as failed.
    pub fn mark_failed(&mut self, error: impl Into<String>) {
        self.delivery
            .record_attempt(DeliveryStatus::Failed, Some(error.into()));
    }

    /// Check if delivered.
    pub fn is_delivered(&self) -> bool {
        self.delivery.status == DeliveryStatus::Delivered
    }

    /// Map payload.
    pub fn map<U, F>(self, f: F) -> Envelope<U>
    where
        F: FnOnce(T) -> U,
    {
        Envelope {
            id: self.id,
            routing: self.routing,
            delivery: self.delivery,
            payload: f(self.payload),
            metadata: self.metadata,
            ack_required: self.ack_required,
        }
    }
}

/// Acknowledgment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ack {
    /// Envelope ID.
    pub envelope_id: EnvelopeId,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
    /// Success.
    pub success: bool,
    /// Error if failed.
    pub error: Option<String>,
}

impl Ack {
    /// Create success ack.
    pub fn success(envelope_id: EnvelopeId) -> Self {
        Self {
            envelope_id,
            timestamp: Utc::now(),
            success: true,
            error: None,
        }
    }

    /// Create failure ack.
    pub fn failure(envelope_id: EnvelopeId, error: impl Into<String>) -> Self {
        Self {
            envelope_id,
            timestamp: Utc::now(),
            success: false,
            error: Some(error.into()),
        }
    }
}

/// Envelope builder.
pub struct EnvelopeBuilder<T> {
    payload: Option<T>,
    routing: Option<Routing>,
    metadata: HashMap<String, serde_json::Value>,
    ack_required: bool,
}

impl<T> EnvelopeBuilder<T> {
    /// Create new builder.
    pub fn new() -> Self {
        Self {
            payload: None,
            routing: None,
            metadata: HashMap::new(),
            ack_required: false,
        }
    }

    /// Set payload.
    pub fn payload(mut self, payload: T) -> Self {
        self.payload = Some(payload);
        self
    }

    /// Set destination.
    pub fn to(mut self, address: Address) -> Self {
        self.routing = Some(Routing::new(address));
        self
    }

    /// Set routing.
    pub fn routing(mut self, routing: Routing) -> Self {
        self.routing = Some(routing);
        self
    }

    /// Add metadata.
    pub fn metadata(mut self, key: impl Into<String>, value: impl Serialize) -> Self {
        if let Ok(v) = serde_json::to_value(value) {
            self.metadata.insert(key.into(), v);
        }
        self
    }

    /// Set ack required.
    pub fn ack(mut self, required: bool) -> Self {
        self.ack_required = required;
        self
    }

    /// Build envelope.
    pub fn build(self) -> Result<Envelope<T>> {
        let payload = self
            .payload
            .ok_or_else(|| EnvelopeError::Invalid("Missing payload".to_string()))?;
        let routing = self
            .routing
            .ok_or_else(|| EnvelopeError::Invalid("Missing routing".to_string()))?;

        Ok(Envelope {
            id: EnvelopeId::new(),
            routing,
            delivery: DeliveryInfo::new(),
            payload,
            metadata: self.metadata,
            ack_required: self.ack_required,
        })
    }
}

impl<T> Default for EnvelopeBuilder<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_envelope_id() {
        let id1 = EnvelopeId::new();
        let id2 = EnvelopeId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_address() {
        let addr = Address::queue("my-queue");
        assert_eq!(addr.address_type, "queue");
        assert_eq!(addr.value, "my-queue");
    }

    #[test]
    fn test_envelope_creation() {
        let env = Envelope::new("test payload", Address::queue("test"));
        assert_eq!(env.payload, "test payload");
        assert_eq!(env.delivery.status, DeliveryStatus::Pending);
    }

    #[test]
    fn test_envelope_delivery() {
        let mut env = Envelope::new("test", Address::queue("test"));

        env.mark_delivered();
        assert!(env.is_delivered());
        assert_eq!(env.delivery.attempt_count(), 1);
    }

    #[test]
    fn test_envelope_builder() {
        let env = EnvelopeBuilder::new()
            .payload("test")
            .to(Address::queue("test"))
            .ack(true)
            .metadata("key", "value")
            .build()
            .unwrap();

        assert!(env.ack_required);
        assert!(env.get_metadata("key").is_some());
    }

    #[test]
    fn test_routing_hops() {
        let mut routing = Routing::new(Address::queue("test")).with_max_hops(3);

        assert!(routing.increment_hop().is_ok());
        assert!(routing.increment_hop().is_ok());
        assert!(routing.increment_hop().is_ok());
        assert!(routing.increment_hop().is_err());
    }

    #[test]
    fn test_ack() {
        let env_id = EnvelopeId::new();
        let ack = Ack::success(env_id.clone());
        assert!(ack.success);
        assert_eq!(ack.envelope_id, env_id);
    }
}
