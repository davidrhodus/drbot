//! Event definitions for analytics.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Event types.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventType {
    /// Message sent or received.
    Message,
    /// API call made.
    ApiCall,
    /// Session started.
    SessionStart,
    /// Session ended.
    SessionEnd,
    /// Error occurred.
    Error,
    /// Feature used.
    FeatureUsed,
    /// User action.
    UserAction,
    /// Custom event.
    Custom(String),
}

impl std::fmt::Display for EventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EventType::Message => write!(f, "message"),
            EventType::ApiCall => write!(f, "api_call"),
            EventType::SessionStart => write!(f, "session_start"),
            EventType::SessionEnd => write!(f, "session_end"),
            EventType::Error => write!(f, "error"),
            EventType::FeatureUsed => write!(f, "feature_used"),
            EventType::UserAction => write!(f, "user_action"),
            EventType::Custom(s) => write!(f, "{}", s),
        }
    }
}

/// An analytics event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    /// Event ID.
    pub id: String,
    /// Event type.
    pub event_type: EventType,
    /// Timestamp.
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Event properties.
    pub properties: HashMap<String, serde_json::Value>,
    /// Session ID (if applicable).
    pub session_id: Option<String>,
    /// Model used (if applicable).
    pub model: Option<String>,
}

impl Event {
    /// Create a new event.
    pub fn new(event_type: EventType) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            event_type,
            timestamp: chrono::Utc::now(),
            properties: HashMap::new(),
            session_id: None,
            model: None,
        }
    }

    /// Create a message event.
    pub fn message(role: &str, token_count: u64) -> Self {
        Self::new(EventType::Message)
            .with_property("role", role)
            .with_property("tokens", token_count)
    }

    /// Create an API call event.
    pub fn api_call(model: &str, tokens_in: u64, tokens_out: u64, latency_ms: u64) -> Self {
        Self::new(EventType::ApiCall)
            .with_model(model)
            .with_property("tokens_in", tokens_in)
            .with_property("tokens_out", tokens_out)
            .with_property("latency_ms", latency_ms)
    }

    /// Create a session start event.
    pub fn session_start(session_id: &str) -> Self {
        Self::new(EventType::SessionStart).with_session(session_id)
    }

    /// Create a session end event.
    pub fn session_end(session_id: &str, duration_secs: u64) -> Self {
        Self::new(EventType::SessionEnd)
            .with_session(session_id)
            .with_property("duration_secs", duration_secs)
    }

    /// Create an error event.
    pub fn error(error_type: &str, message: &str) -> Self {
        Self::new(EventType::Error)
            .with_property("error_type", error_type)
            .with_property("message", message)
    }

    /// Create a feature used event.
    pub fn feature_used(feature: &str) -> Self {
        Self::new(EventType::FeatureUsed).with_property("feature", feature)
    }

    /// Create a custom event.
    pub fn custom(name: impl Into<String>) -> Self {
        Self::new(EventType::Custom(name.into()))
    }

    /// Add a property.
    pub fn with_property(
        mut self,
        key: impl Into<String>,
        value: impl Into<serde_json::Value>,
    ) -> Self {
        self.properties.insert(key.into(), value.into());
        self
    }

    /// Set session ID.
    pub fn with_session(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }

    /// Set model.
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// Get a property as string.
    pub fn get_property_str(&self, key: &str) -> Option<&str> {
        self.properties.get(key).and_then(|v| v.as_str())
    }

    /// Get a property as number.
    pub fn get_property_num(&self, key: &str) -> Option<f64> {
        self.properties.get(key).and_then(|v| v.as_f64())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_event() {
        let event = Event::message("user", 100);
        assert_eq!(event.event_type, EventType::Message);
        assert_eq!(event.get_property_str("role"), Some("user"));
    }

    #[test]
    fn test_api_call_event() {
        let event = Event::api_call("gpt-4", 50, 100, 500);
        assert_eq!(event.model, Some("gpt-4".to_string()));
        assert_eq!(event.get_property_num("latency_ms"), Some(500.0));
    }

    #[test]
    fn test_custom_event() {
        let event = Event::custom("button_click").with_property("button_id", "submit");
        assert!(matches!(event.event_type, EventType::Custom(s) if s == "button_click"));
    }
}
