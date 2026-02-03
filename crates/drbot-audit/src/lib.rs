//! Audit trail for drbot.
//!
//! Complete logging of AI actions and data access.
//!
//! # Features
//!
//! - Action logging
//! - Data access tracking
//! - Compliance reporting
//! - Query and export
//! - Retention policies

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Audit result type.
pub type Result<T> = std::result::Result<T, AuditError>;

/// Audit errors.
#[derive(Debug, thiserror::Error)]
pub enum AuditError {
    #[error("Entry not found: {0}")]
    NotFound(String),
    #[error("Storage error: {0}")]
    StorageError(String),
    #[error("Query error: {0}")]
    QueryError(String),
}

/// Audit event type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    // AI Actions
    Query,
    Response,
    ToolCall,
    ToolResult,

    // Data Access
    DataRead,
    DataWrite,
    DataDelete,

    // System
    SessionStart,
    SessionEnd,
    ConfigChange,

    // Security
    AuthSuccess,
    AuthFailure,
    PermissionDenied,

    // User Actions
    UserInput,
    Feedback,
    Export,

    Custom,
}

/// Audit log entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    /// Entry ID.
    pub id: Uuid,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
    /// Event type.
    pub event_type: EventType,
    /// Actor (user, system, etc).
    pub actor: String,
    /// Action description.
    pub action: String,
    /// Resource affected.
    pub resource: Option<String>,
    /// Resource type.
    pub resource_type: Option<String>,
    /// Session ID.
    pub session_id: Option<Uuid>,
    /// Request ID.
    pub request_id: Option<Uuid>,
    /// Details.
    pub details: HashMap<String, serde_json::Value>,
    /// Success.
    pub success: bool,
    /// Error message.
    pub error: Option<String>,
    /// Source IP/location.
    pub source: Option<String>,
    /// Duration (ms).
    pub duration_ms: Option<u64>,
}

impl AuditEntry {
    /// Create a new entry.
    pub fn new(event_type: EventType, actor: &str, action: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            event_type,
            actor: actor.to_string(),
            action: action.to_string(),
            resource: None,
            resource_type: None,
            session_id: None,
            request_id: None,
            details: HashMap::new(),
            success: true,
            error: None,
            source: None,
            duration_ms: None,
        }
    }

    /// Set resource.
    pub fn with_resource(mut self, resource: &str, resource_type: &str) -> Self {
        self.resource = Some(resource.to_string());
        self.resource_type = Some(resource_type.to_string());
        self
    }

    /// Set session.
    pub fn with_session(mut self, session_id: Uuid) -> Self {
        self.session_id = Some(session_id);
        self
    }

    /// Add detail.
    pub fn with_detail(mut self, key: &str, value: serde_json::Value) -> Self {
        self.details.insert(key.to_string(), value);
        self
    }

    /// Mark as failed.
    pub fn failed(mut self, error: &str) -> Self {
        self.success = false;
        self.error = Some(error.to_string());
        self
    }
}

/// Audit query.
#[derive(Debug, Clone, Default)]
pub struct AuditQuery {
    /// Event types filter.
    pub event_types: Option<Vec<EventType>>,
    /// Actor filter.
    pub actor: Option<String>,
    /// Session filter.
    pub session_id: Option<Uuid>,
    /// Start time.
    pub start_time: Option<DateTime<Utc>>,
    /// End time.
    pub end_time: Option<DateTime<Utc>>,
    /// Success filter.
    pub success: Option<bool>,
    /// Resource filter.
    pub resource: Option<String>,
    /// Limit.
    pub limit: Option<usize>,
    /// Offset.
    pub offset: Option<usize>,
}

impl AuditQuery {
    /// New query.
    pub fn new() -> Self {
        Self::default()
    }

    /// Filter by event types.
    pub fn event_types(mut self, types: Vec<EventType>) -> Self {
        self.event_types = Some(types);
        self
    }

    /// Filter by actor.
    pub fn actor(mut self, actor: &str) -> Self {
        self.actor = Some(actor.to_string());
        self
    }

    /// Filter by time range.
    pub fn time_range(mut self, start: DateTime<Utc>, end: DateTime<Utc>) -> Self {
        self.start_time = Some(start);
        self.end_time = Some(end);
        self
    }

    /// Filter last N hours.
    pub fn last_hours(mut self, hours: i64) -> Self {
        self.start_time = Some(Utc::now() - Duration::hours(hours));
        self.end_time = Some(Utc::now());
        self
    }

    /// Paginate.
    pub fn paginate(mut self, limit: usize, offset: usize) -> Self {
        self.limit = Some(limit);
        self.offset = Some(offset);
        self
    }
}

/// Audit summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditSummary {
    /// Period start.
    pub start_time: DateTime<Utc>,
    /// Period end.
    pub end_time: DateTime<Utc>,
    /// Total events.
    pub total_events: usize,
    /// Events by type.
    pub by_type: HashMap<EventType, usize>,
    /// Events by actor.
    pub by_actor: HashMap<String, usize>,
    /// Success rate.
    pub success_rate: f32,
    /// Unique sessions.
    pub unique_sessions: usize,
    /// Top resources.
    pub top_resources: Vec<(String, usize)>,
}

/// Retention policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetentionPolicy {
    /// Default retention days.
    pub default_days: u32,
    /// Per-type retention.
    pub by_type: HashMap<EventType, u32>,
    /// Minimum retention days.
    pub min_days: u32,
}

impl Default for RetentionPolicy {
    fn default() -> Self {
        Self {
            default_days: 90,
            by_type: HashMap::new(),
            min_days: 7,
        }
    }
}

/// Audit configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditConfig {
    /// Enable auditing.
    pub enabled: bool,
    /// Retention policy.
    pub retention: RetentionPolicy,
    /// Log queries.
    pub log_queries: bool,
    /// Log responses.
    pub log_responses: bool,
    /// Redact sensitive data.
    pub redact_sensitive: bool,
}

impl Default for AuditConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            retention: RetentionPolicy::default(),
            log_queries: true,
            log_responses: true,
            redact_sensitive: true,
        }
    }
}

/// Audit trail manager.
pub struct AuditTrail {
    config: AuditConfig,
    entries: Arc<RwLock<Vec<AuditEntry>>>,
}

impl AuditTrail {
    /// Create a new audit trail.
    pub fn new(config: AuditConfig) -> Self {
        Self {
            config,
            entries: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Log an entry.
    pub async fn log(&self, entry: AuditEntry) {
        if !self.config.enabled {
            return;
        }

        let mut entry = entry;

        // Redact sensitive data
        if self.config.redact_sensitive {
            entry = self.redact(entry);
        }

        self.entries.write().await.push(entry);
    }

    /// Log query.
    pub async fn log_query(&self, actor: &str, query: &str, session_id: Option<Uuid>) {
        if !self.config.log_queries {
            return;
        }

        let mut entry = AuditEntry::new(EventType::Query, actor, "User query")
            .with_detail("query", serde_json::json!(query));

        if let Some(sid) = session_id {
            entry = entry.with_session(sid);
        }

        self.log(entry).await;
    }

    /// Log response.
    pub async fn log_response(
        &self,
        actor: &str,
        response_len: usize,
        session_id: Option<Uuid>,
        duration_ms: u64,
    ) {
        if !self.config.log_responses {
            return;
        }

        let mut entry = AuditEntry::new(EventType::Response, actor, "AI response")
            .with_detail("response_length", serde_json::json!(response_len));
        entry.duration_ms = Some(duration_ms);

        if let Some(sid) = session_id {
            entry = entry.with_session(sid);
        }

        self.log(entry).await;
    }

    /// Log tool call.
    pub async fn log_tool_call(&self, tool_name: &str, success: bool, duration_ms: u64) {
        let mut entry = AuditEntry::new(
            EventType::ToolCall,
            "system",
            &format!("Tool: {}", tool_name),
        );
        entry.duration_ms = Some(duration_ms);
        entry.success = success;

        self.log(entry).await;
    }

    /// Log data access.
    pub async fn log_data_access(
        &self,
        actor: &str,
        action: EventType,
        resource: &str,
        resource_type: &str,
    ) {
        let entry = AuditEntry::new(action, actor, &format!("{:?} {}", action, resource))
            .with_resource(resource, resource_type);

        self.log(entry).await;
    }

    fn redact(&self, mut entry: AuditEntry) -> AuditEntry {
        // Redact common sensitive patterns
        let sensitive_keys = ["password", "token", "secret", "key", "credential"];

        for key in &sensitive_keys {
            if entry.details.contains_key(*key) {
                entry
                    .details
                    .insert(key.to_string(), serde_json::json!("[REDACTED]"));
            }
        }

        entry
    }

    /// Query entries.
    pub async fn query(&self, query: &AuditQuery) -> Vec<AuditEntry> {
        let entries = self.entries.read().await;

        let mut filtered: Vec<_> = entries
            .iter()
            .filter(|e| {
                // Event type filter
                if let Some(ref types) = query.event_types {
                    if !types.contains(&e.event_type) {
                        return false;
                    }
                }

                // Actor filter
                if let Some(ref actor) = query.actor {
                    if &e.actor != actor {
                        return false;
                    }
                }

                // Session filter
                if let Some(ref session) = query.session_id {
                    if e.session_id.as_ref() != Some(session) {
                        return false;
                    }
                }

                // Time range filter
                if let Some(ref start) = query.start_time {
                    if &e.timestamp < start {
                        return false;
                    }
                }
                if let Some(ref end) = query.end_time {
                    if &e.timestamp > end {
                        return false;
                    }
                }

                // Success filter
                if let Some(success) = query.success {
                    if e.success != success {
                        return false;
                    }
                }

                // Resource filter
                if let Some(ref resource) = query.resource {
                    if e.resource.as_ref() != Some(resource) {
                        return false;
                    }
                }

                true
            })
            .cloned()
            .collect();

        // Sort by timestamp descending
        filtered.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

        // Pagination
        if let Some(offset) = query.offset {
            filtered = filtered.into_iter().skip(offset).collect();
        }
        if let Some(limit) = query.limit {
            filtered.truncate(limit);
        }

        filtered
    }

    /// Get summary.
    pub async fn summary(&self, start: DateTime<Utc>, end: DateTime<Utc>) -> AuditSummary {
        let entries = self.entries.read().await;

        let filtered: Vec<_> = entries
            .iter()
            .filter(|e| e.timestamp >= start && e.timestamp <= end)
            .collect();

        let mut by_type: HashMap<EventType, usize> = HashMap::new();
        let mut by_actor: HashMap<String, usize> = HashMap::new();
        let mut resources: HashMap<String, usize> = HashMap::new();
        let mut sessions = std::collections::HashSet::new();
        let mut success_count = 0;

        for entry in &filtered {
            *by_type.entry(entry.event_type).or_insert(0) += 1;
            *by_actor.entry(entry.actor.clone()).or_insert(0) += 1;

            if let Some(ref resource) = entry.resource {
                *resources.entry(resource.clone()).or_insert(0) += 1;
            }

            if let Some(session) = entry.session_id {
                sessions.insert(session);
            }

            if entry.success {
                success_count += 1;
            }
        }

        let mut top_resources: Vec<_> = resources.into_iter().collect();
        top_resources.sort_by(|a, b| b.1.cmp(&a.1));
        top_resources.truncate(10);

        let success_rate = if filtered.is_empty() {
            1.0
        } else {
            success_count as f32 / filtered.len() as f32
        };

        AuditSummary {
            start_time: start,
            end_time: end,
            total_events: filtered.len(),
            by_type,
            by_actor,
            success_rate,
            unique_sessions: sessions.len(),
            top_resources,
        }
    }

    /// Apply retention policy.
    pub async fn apply_retention(&self) {
        let cutoff = Utc::now() - Duration::days(self.config.retention.default_days as i64);

        let mut entries = self.entries.write().await;
        entries.retain(|e| {
            let retention_days = self
                .config
                .retention
                .by_type
                .get(&e.event_type)
                .copied()
                .unwrap_or(self.config.retention.default_days);

            let entry_cutoff = Utc::now() - Duration::days(retention_days as i64);
            e.timestamp > entry_cutoff
        });
    }

    /// Export entries.
    pub async fn export(&self, query: &AuditQuery) -> String {
        let entries = self.query(query).await;
        serde_json::to_string_pretty(&entries).unwrap_or_default()
    }

    /// Get statistics.
    pub async fn stats(&self) -> AuditStats {
        let entries = self.entries.read().await;

        let oldest = entries.iter().map(|e| e.timestamp).min();
        let newest = entries.iter().map(|e| e.timestamp).max();

        AuditStats {
            total_entries: entries.len(),
            oldest_entry: oldest,
            newest_entry: newest,
        }
    }
}

/// Audit statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditStats {
    pub total_entries: usize,
    pub oldest_entry: Option<DateTime<Utc>>,
    pub newest_entry: Option<DateTime<Utc>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_log_entry() {
        let audit = AuditTrail::new(AuditConfig::default());

        let entry = AuditEntry::new(EventType::Query, "user1", "Test query");
        audit.log(entry).await;

        let stats = audit.stats().await;
        assert_eq!(stats.total_entries, 1);
    }

    #[tokio::test]
    async fn test_query_filter() {
        let audit = AuditTrail::new(AuditConfig::default());

        audit
            .log(AuditEntry::new(EventType::Query, "user1", "Query 1"))
            .await;
        audit
            .log(AuditEntry::new(EventType::Response, "system", "Response 1"))
            .await;
        audit
            .log(AuditEntry::new(EventType::Query, "user2", "Query 2"))
            .await;

        let query = AuditQuery::new().event_types(vec![EventType::Query]);
        let results = audit.query(&query).await;
        assert_eq!(results.len(), 2);

        let query = AuditQuery::new().actor("user1");
        let results = audit.query(&query).await;
        assert_eq!(results.len(), 1);
    }

    #[tokio::test]
    async fn test_summary() {
        let audit = AuditTrail::new(AuditConfig::default());

        audit
            .log(AuditEntry::new(EventType::Query, "user1", "Q1"))
            .await;
        audit
            .log(AuditEntry::new(EventType::Query, "user1", "Q2"))
            .await;
        audit
            .log(AuditEntry::new(EventType::Response, "system", "R1"))
            .await;

        let summary = audit
            .summary(
                Utc::now() - Duration::hours(1),
                Utc::now() + Duration::hours(1),
            )
            .await;

        assert_eq!(summary.total_events, 3);
        assert_eq!(summary.by_type.get(&EventType::Query), Some(&2));
    }

    #[tokio::test]
    async fn test_redaction() {
        let audit = AuditTrail::new(AuditConfig::default());

        let entry = AuditEntry::new(EventType::DataWrite, "system", "Store secret")
            .with_detail("password", serde_json::json!("secret123"));

        audit.log(entry).await;

        let results = audit.query(&AuditQuery::new()).await;
        assert_eq!(
            results[0].details.get("password"),
            Some(&serde_json::json!("[REDACTED]"))
        );
    }
}
