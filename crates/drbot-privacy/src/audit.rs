//! Audit logging for compliance.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Audit event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    /// Event ID.
    pub id: Uuid,
    /// Event level.
    pub level: AuditLevel,
    /// Event category.
    pub category: String,
    /// Event message.
    pub message: String,
    /// User ID.
    pub user_id: Option<String>,
    /// Session ID.
    pub session_id: Option<String>,
    /// IP address.
    pub ip_address: Option<String>,
    /// Additional data.
    pub data: Option<serde_json::Value>,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
}

impl AuditEvent {
    /// Create a new audit event.
    pub fn new(level: AuditLevel, category: &str, message: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            level,
            category: category.to_string(),
            message: message.into(),
            user_id: None,
            session_id: None,
            ip_address: None,
            data: None,
            timestamp: Utc::now(),
        }
    }

    /// Set user ID.
    pub fn with_user(mut self, user_id: &str) -> Self {
        self.user_id = Some(user_id.to_string());
        self
    }

    /// Set session ID.
    pub fn with_session(mut self, session_id: &str) -> Self {
        self.session_id = Some(session_id.to_string());
        self
    }

    /// Set additional data.
    pub fn with_data(mut self, data: serde_json::Value) -> Self {
        self.data = Some(data);
        self
    }
}

/// Audit level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuditLevel {
    Debug,
    Info,
    Warning,
    Error,
    Critical,
}

/// Audit configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditConfig {
    /// Minimum level to log.
    pub min_level: AuditLevel,
    /// Retention days.
    pub retention_days: u32,
    /// Include request data.
    pub include_request_data: bool,
    /// Include response data.
    pub include_response_data: bool,
    /// Categories to always log.
    pub always_log: Vec<String>,
    /// Categories to never log.
    pub never_log: Vec<String>,
}

impl Default for AuditConfig {
    fn default() -> Self {
        Self {
            min_level: AuditLevel::Info,
            retention_days: 90,
            include_request_data: false,
            include_response_data: false,
            always_log: vec![
                "auth".to_string(),
                "privacy".to_string(),
                "admin".to_string(),
            ],
            never_log: Vec::new(),
        }
    }
}

/// Audit logger.
pub struct AuditLogger {
    config: AuditConfig,
    events: Arc<RwLock<Vec<AuditEvent>>>,
}

impl AuditLogger {
    /// Create a new audit logger.
    pub fn new(config: AuditConfig) -> Self {
        Self {
            config,
            events: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Log an event.
    pub async fn log(&self, event: AuditEvent) {
        // Check if should log
        if !self.should_log(&event) {
            return;
        }

        let mut events = self.events.write().await;
        events.push(event);

        // Cleanup old events
        let cutoff = Utc::now() - chrono::Duration::days(self.config.retention_days as i64);
        events.retain(|e| e.timestamp > cutoff);
    }

    fn should_log(&self, event: &AuditEvent) -> bool {
        // Never log
        if self.config.never_log.contains(&event.category) {
            return false;
        }

        // Always log
        if self.config.always_log.contains(&event.category) {
            return true;
        }

        // Check level
        event.level >= self.config.min_level
    }

    /// Get events.
    pub async fn get_events(&self, filter: AuditFilter) -> Vec<AuditEvent> {
        let events = self.events.read().await;

        events
            .iter()
            .filter(|e| {
                let matches_level = filter.level.map_or(true, |l| e.level >= l);
                let matches_category = filter.category.as_ref().map_or(true, |c| &e.category == c);
                let matches_user = filter
                    .user_id
                    .as_ref()
                    .map_or(true, |u| e.user_id.as_ref() == Some(u));
                let matches_from = filter.from.map_or(true, |f| e.timestamp >= f);
                let matches_to = filter.to.map_or(true, |t| e.timestamp <= t);

                matches_level && matches_category && matches_user && matches_from && matches_to
            })
            .cloned()
            .collect()
    }

    /// Get recent events.
    pub async fn recent(&self, limit: usize) -> Vec<AuditEvent> {
        let events = self.events.read().await;
        events.iter().rev().take(limit).cloned().collect()
    }

    /// Export events for compliance.
    pub async fn export(&self, from: DateTime<Utc>, to: DateTime<Utc>) -> Vec<AuditEvent> {
        self.get_events(AuditFilter {
            from: Some(from),
            to: Some(to),
            ..Default::default()
        })
        .await
    }

    /// Get event count.
    pub async fn count(&self) -> usize {
        self.events.read().await.len()
    }
}

/// Audit filter.
#[derive(Debug, Clone, Default)]
pub struct AuditFilter {
    /// Minimum level.
    pub level: Option<AuditLevel>,
    /// Category.
    pub category: Option<String>,
    /// User ID.
    pub user_id: Option<String>,
    /// From date.
    pub from: Option<DateTime<Utc>>,
    /// To date.
    pub to: Option<DateTime<Utc>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_audit_logger() {
        let config = AuditConfig::default();
        let logger = AuditLogger::new(config);

        logger
            .log(AuditEvent::new(AuditLevel::Info, "test", "Test event"))
            .await;

        let count = logger.count().await;
        assert_eq!(count, 1);

        let events = logger.recent(10).await;
        assert_eq!(events.len(), 1);
    }

    #[tokio::test]
    async fn test_level_filtering() {
        let config = AuditConfig {
            min_level: AuditLevel::Warning,
            ..Default::default()
        };
        let logger = AuditLogger::new(config);

        logger
            .log(AuditEvent::new(AuditLevel::Debug, "test", "Debug"))
            .await;
        logger
            .log(AuditEvent::new(AuditLevel::Info, "test", "Info"))
            .await;
        logger
            .log(AuditEvent::new(AuditLevel::Warning, "test", "Warning"))
            .await;

        // Only warning should be logged
        let count = logger.count().await;
        assert_eq!(count, 1);
    }
}
