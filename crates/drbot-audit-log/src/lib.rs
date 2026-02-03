//! Audit logging for drbot.
//!
//! This crate provides:
//! - Structured audit events
//! - Queryable audit trail
//! - Compliance-ready logging
//! - Event correlation

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Audit log error types.
#[derive(Error, Debug)]
pub enum AuditError {
    #[error("Event not found: {0}")]
    EventNotFound(Uuid),

    #[error("Storage error: {0}")]
    StorageError(String),

    #[error("Query error: {0}")]
    QueryError(String),
}

/// Result type for audit operations.
pub type Result<T> = std::result::Result<T, AuditError>;

/// Audit event severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Severity {
    /// Informational.
    Info,
    /// Low severity.
    Low,
    /// Medium severity.
    Medium,
    /// High severity.
    High,
    /// Critical.
    Critical,
}

/// Audit event outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Outcome {
    /// Action succeeded.
    Success,
    /// Action failed.
    Failure,
    /// Action was denied.
    Denied,
    /// Outcome unknown.
    Unknown,
}

/// Audit event category.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Category {
    /// Authentication events.
    Authentication,
    /// Authorization events.
    Authorization,
    /// Data access.
    DataAccess,
    /// Data modification.
    DataModification,
    /// Configuration change.
    ConfigChange,
    /// System event.
    System,
    /// Security event.
    Security,
    /// Custom category.
    Custom(String),
}

/// Actor (who performed the action).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Actor {
    /// Actor ID.
    pub id: String,
    /// Actor type.
    pub actor_type: String,
    /// Display name.
    pub name: Option<String>,
    /// IP address.
    pub ip_address: Option<String>,
    /// User agent.
    pub user_agent: Option<String>,
}

impl Actor {
    /// Create a new actor.
    pub fn new(id: impl Into<String>, actor_type: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            actor_type: actor_type.into(),
            name: None,
            ip_address: None,
            user_agent: None,
        }
    }

    /// Set name.
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Set IP address.
    pub fn with_ip(mut self, ip: impl Into<String>) -> Self {
        self.ip_address = Some(ip.into());
        self
    }
}

/// Target (what was acted upon).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Target {
    /// Target ID.
    pub id: String,
    /// Target type.
    pub target_type: String,
    /// Target name.
    pub name: Option<String>,
}

impl Target {
    /// Create a new target.
    pub fn new(id: impl Into<String>, target_type: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            target_type: target_type.into(),
            name: None,
        }
    }

    /// Set name.
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }
}

/// An audit event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    /// Event ID.
    pub id: Uuid,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
    /// Event category.
    pub category: Category,
    /// Action performed.
    pub action: String,
    /// Event description.
    pub description: String,
    /// Actor.
    pub actor: Option<Actor>,
    /// Target.
    pub target: Option<Target>,
    /// Outcome.
    pub outcome: Outcome,
    /// Severity.
    pub severity: Severity,
    /// Correlation ID.
    pub correlation_id: Option<Uuid>,
    /// Session ID.
    pub session_id: Option<String>,
    /// Additional data.
    pub data: HashMap<String, serde_json::Value>,
    /// Tags.
    pub tags: Vec<String>,
}

impl AuditEvent {
    /// Create a new event.
    pub fn new(action: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            category: Category::System,
            action: action.into(),
            description: description.into(),
            actor: None,
            target: None,
            outcome: Outcome::Unknown,
            severity: Severity::Info,
            correlation_id: None,
            session_id: None,
            data: HashMap::new(),
            tags: Vec::new(),
        }
    }

    /// Set category.
    pub fn with_category(mut self, category: Category) -> Self {
        self.category = category;
        self
    }

    /// Set actor.
    pub fn with_actor(mut self, actor: Actor) -> Self {
        self.actor = Some(actor);
        self
    }

    /// Set target.
    pub fn with_target(mut self, target: Target) -> Self {
        self.target = Some(target);
        self
    }

    /// Set outcome.
    pub fn with_outcome(mut self, outcome: Outcome) -> Self {
        self.outcome = outcome;
        self
    }

    /// Set severity.
    pub fn with_severity(mut self, severity: Severity) -> Self {
        self.severity = severity;
        self
    }

    /// Set correlation ID.
    pub fn with_correlation_id(mut self, id: Uuid) -> Self {
        self.correlation_id = Some(id);
        self
    }

    /// Add data.
    pub fn with_data(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.data.insert(key.into(), value);
        self
    }

    /// Add tag.
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }
}

/// Audit query.
#[derive(Debug, Clone, Default)]
pub struct AuditQuery {
    /// Filter by actor ID.
    pub actor_id: Option<String>,
    /// Filter by target ID.
    pub target_id: Option<String>,
    /// Filter by action.
    pub action: Option<String>,
    /// Filter by category.
    pub category: Option<String>,
    /// Filter by outcome.
    pub outcome: Option<Outcome>,
    /// Filter by minimum severity.
    pub min_severity: Option<Severity>,
    /// Filter by correlation ID.
    pub correlation_id: Option<Uuid>,
    /// Start time.
    pub start_time: Option<DateTime<Utc>>,
    /// End time.
    pub end_time: Option<DateTime<Utc>>,
    /// Limit.
    pub limit: Option<usize>,
    /// Offset.
    pub offset: Option<usize>,
}

impl AuditQuery {
    /// Create a new query.
    pub fn new() -> Self {
        Self::default()
    }

    /// Filter by actor.
    pub fn actor(mut self, id: impl Into<String>) -> Self {
        self.actor_id = Some(id.into());
        self
    }

    /// Filter by target.
    pub fn target(mut self, id: impl Into<String>) -> Self {
        self.target_id = Some(id.into());
        self
    }

    /// Filter by action.
    pub fn action(mut self, action: impl Into<String>) -> Self {
        self.action = Some(action.into());
        self
    }

    /// Filter by time range.
    pub fn time_range(mut self, start: DateTime<Utc>, end: DateTime<Utc>) -> Self {
        self.start_time = Some(start);
        self.end_time = Some(end);
        self
    }

    /// Set limit.
    pub fn limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Check if event matches query.
    fn matches(&self, event: &AuditEvent) -> bool {
        if let Some(ref actor_id) = self.actor_id {
            if event.actor.as_ref().map(|a| &a.id) != Some(actor_id) {
                return false;
            }
        }

        if let Some(ref target_id) = self.target_id {
            if event.target.as_ref().map(|t| &t.id) != Some(target_id) {
                return false;
            }
        }

        if let Some(ref action) = self.action {
            if &event.action != action {
                return false;
            }
        }

        if let Some(ref outcome) = self.outcome {
            if &event.outcome != outcome {
                return false;
            }
        }

        if let Some(ref corr_id) = self.correlation_id {
            if event.correlation_id.as_ref() != Some(corr_id) {
                return false;
            }
        }

        if let Some(start) = self.start_time {
            if event.timestamp < start {
                return false;
            }
        }

        if let Some(end) = self.end_time {
            if event.timestamp > end {
                return false;
            }
        }

        true
    }
}

/// Audit storage trait.
#[async_trait]
pub trait AuditStorage: Send + Sync {
    /// Store an event.
    async fn store(&self, event: AuditEvent) -> Result<()>;

    /// Get an event by ID.
    async fn get(&self, id: Uuid) -> Result<Option<AuditEvent>>;

    /// Query events.
    async fn query(&self, query: AuditQuery) -> Result<Vec<AuditEvent>>;

    /// Count events matching query.
    async fn count(&self, query: AuditQuery) -> Result<u64>;
}

/// In-memory audit storage.
pub struct InMemoryAuditStorage {
    events: RwLock<Vec<AuditEvent>>,
}

impl InMemoryAuditStorage {
    /// Create new storage.
    pub fn new() -> Self {
        Self {
            events: RwLock::new(Vec::new()),
        }
    }
}

impl Default for InMemoryAuditStorage {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AuditStorage for InMemoryAuditStorage {
    async fn store(&self, event: AuditEvent) -> Result<()> {
        let mut events = self.events.write().await;
        events.push(event);
        Ok(())
    }

    async fn get(&self, id: Uuid) -> Result<Option<AuditEvent>> {
        let events = self.events.read().await;
        Ok(events.iter().find(|e| e.id == id).cloned())
    }

    async fn query(&self, query: AuditQuery) -> Result<Vec<AuditEvent>> {
        let events = self.events.read().await;
        let mut results: Vec<_> = events
            .iter()
            .filter(|e| query.matches(e))
            .cloned()
            .collect();

        // Sort by timestamp descending
        results.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

        // Apply offset and limit
        if let Some(offset) = query.offset {
            results = results.into_iter().skip(offset).collect();
        }
        if let Some(limit) = query.limit {
            results.truncate(limit);
        }

        Ok(results)
    }

    async fn count(&self, query: AuditQuery) -> Result<u64> {
        let events = self.events.read().await;
        Ok(events.iter().filter(|e| query.matches(e)).count() as u64)
    }
}

/// Audit logger.
pub struct AuditLogger<S: AuditStorage> {
    storage: Arc<S>,
}

impl<S: AuditStorage> AuditLogger<S> {
    /// Create new logger.
    pub fn new(storage: Arc<S>) -> Self {
        Self { storage }
    }

    /// Log an event.
    pub async fn log(&self, event: AuditEvent) -> Result<Uuid> {
        let id = event.id;
        self.storage.store(event).await?;
        Ok(id)
    }

    /// Log authentication event.
    pub async fn log_auth(
        &self,
        actor: Actor,
        success: bool,
        reason: Option<&str>,
    ) -> Result<Uuid> {
        let mut event = AuditEvent::new(
            if success { "login" } else { "login_failed" },
            if success {
                "User authenticated successfully"
            } else {
                "Authentication failed"
            },
        )
        .with_category(Category::Authentication)
        .with_actor(actor)
        .with_outcome(if success {
            Outcome::Success
        } else {
            Outcome::Failure
        })
        .with_severity(if success {
            Severity::Info
        } else {
            Severity::Medium
        });

        if let Some(r) = reason {
            event = event.with_data("reason", serde_json::json!(r));
        }

        self.log(event).await
    }

    /// Log data access.
    pub async fn log_access(&self, actor: Actor, target: Target, action: &str) -> Result<Uuid> {
        let event = AuditEvent::new(action, format!("Data accessed: {}", action))
            .with_category(Category::DataAccess)
            .with_actor(actor)
            .with_target(target)
            .with_outcome(Outcome::Success);

        self.log(event).await
    }

    /// Query events.
    pub async fn query(&self, query: AuditQuery) -> Result<Vec<AuditEvent>> {
        self.storage.query(query).await
    }

    /// Get event by ID.
    pub async fn get(&self, id: Uuid) -> Result<Option<AuditEvent>> {
        self.storage.get(id).await
    }

    /// Get events by correlation ID.
    pub async fn get_correlated(&self, correlation_id: Uuid) -> Result<Vec<AuditEvent>> {
        let mut query = AuditQuery::new();
        query.correlation_id = Some(correlation_id);
        self.storage.query(query).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_creation() {
        let event = AuditEvent::new("user.create", "Created new user")
            .with_category(Category::DataModification)
            .with_actor(Actor::new("admin", "user").with_name("Admin"))
            .with_target(Target::new("user-123", "user"))
            .with_outcome(Outcome::Success);

        assert_eq!(event.action, "user.create");
        assert!(event.actor.is_some());
    }

    #[tokio::test]
    async fn test_store_and_get() {
        let storage = InMemoryAuditStorage::new();

        let event = AuditEvent::new("test", "Test event");
        let id = event.id;
        storage.store(event).await.unwrap();

        let retrieved = storage.get(id).await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().id, id);
    }

    #[tokio::test]
    async fn test_query_by_actor() {
        let storage = InMemoryAuditStorage::new();

        storage
            .store(AuditEvent::new("action1", "desc").with_actor(Actor::new("user1", "user")))
            .await
            .unwrap();

        storage
            .store(AuditEvent::new("action2", "desc").with_actor(Actor::new("user2", "user")))
            .await
            .unwrap();

        let query = AuditQuery::new().actor("user1");
        let results = storage.query(query).await.unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].actor.as_ref().unwrap().id, "user1");
    }

    #[tokio::test]
    async fn test_query_by_action() {
        let storage = InMemoryAuditStorage::new();

        storage
            .store(AuditEvent::new("login", "desc"))
            .await
            .unwrap();
        storage
            .store(AuditEvent::new("logout", "desc"))
            .await
            .unwrap();

        let query = AuditQuery::new().action("login");
        let results = storage.query(query).await.unwrap();

        assert_eq!(results.len(), 1);
    }

    #[tokio::test]
    async fn test_count() {
        let storage = InMemoryAuditStorage::new();

        for _ in 0..5 {
            storage
                .store(AuditEvent::new("test", "desc"))
                .await
                .unwrap();
        }

        let count = storage.count(AuditQuery::new()).await.unwrap();
        assert_eq!(count, 5);
    }

    #[tokio::test]
    async fn test_audit_logger() {
        let storage = Arc::new(InMemoryAuditStorage::new());
        let logger = AuditLogger::new(storage);

        let actor = Actor::new("user1", "user");
        logger.log_auth(actor, true, None).await.unwrap();

        let results = logger.query(AuditQuery::new()).await.unwrap();
        assert_eq!(results.len(), 1);
    }

    #[tokio::test]
    async fn test_limit_and_offset() {
        let storage = InMemoryAuditStorage::new();

        for i in 0..10 {
            storage
                .store(AuditEvent::new(format!("action{}", i), "desc"))
                .await
                .unwrap();
        }

        let query = AuditQuery::new().limit(5);
        let results = storage.query(query).await.unwrap();
        assert_eq!(results.len(), 5);
    }
}
