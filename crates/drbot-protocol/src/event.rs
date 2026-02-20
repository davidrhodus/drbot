//! Event types for the drbot protocol.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A server event (push notification).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    /// Event type.
    pub event_type: String,
    /// Event data.
    pub data: serde_json::Value,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
}

impl Event {
    /// Create a new event.
    pub fn new(event_type: impl Into<String>, data: impl Serialize) -> Self {
        Self {
            event_type: event_type.into(),
            data: serde_json::to_value(data).unwrap_or(serde_json::Value::Null),
            timestamp: Utc::now(),
        }
    }
}

// ============================================================================
// Chat Events
// ============================================================================

/// Event types for chat operations.
pub mod chat {
    use super::*;

    /// Stream started event.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct StreamStartEvent {
        /// Request ID.
        pub request_id: Uuid,
        /// Session ID.
        pub session_id: Uuid,
        /// Message ID.
        pub message_id: Uuid,
        /// Model being used.
        pub model: String,
        /// Provider name (if known).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub provider: Option<String>,
    }

    /// Stream delta (chunk) event.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct StreamDeltaEvent {
        /// Request ID.
        pub request_id: Uuid,
        /// Content delta.
        pub delta: String,
    }

    /// Stream completed event.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct StreamCompleteEvent {
        /// Request ID.
        pub request_id: Uuid,
        /// Final content.
        pub content: String,
        /// Stop reason.
        pub stop_reason: Option<String>,
        /// Token usage.
        pub usage: Option<super::super::response::TokenUsage>,
    }

    /// Stream error event.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct StreamErrorEvent {
        /// Request ID.
        pub request_id: Uuid,
        /// Error message.
        pub error: String,
    }
}

// ============================================================================
// Session Events
// ============================================================================

/// Event types for session operations.
pub mod session {
    use super::*;

    /// Session created event.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct CreatedEvent {
        /// Session ID.
        pub session_id: Uuid,
        /// Session title.
        pub title: Option<String>,
    }

    /// Session updated event.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct UpdatedEvent {
        /// Session ID.
        pub session_id: Uuid,
        /// What was updated.
        pub updated_fields: Vec<String>,
    }

    /// Session deleted event.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct DeletedEvent {
        /// Session ID.
        pub session_id: Uuid,
    }
}

// ============================================================================
// Channel Events
// ============================================================================

/// Event types for channel operations.
pub mod channel {
    use super::*;

    /// Channel connected event.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ConnectedEvent {
        /// Channel type.
        pub channel_type: String,
    }

    /// Channel disconnected event.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct DisconnectedEvent {
        /// Channel type.
        pub channel_type: String,
        /// Reason for disconnection.
        pub reason: Option<String>,
    }

    /// Incoming message event (from a channel).
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct MessageEvent {
        /// Channel type.
        pub channel_type: String,
        /// Channel-specific chat ID.
        pub channel_id: String,
        /// Sender ID.
        pub sender_id: String,
        /// Sender name.
        pub sender_name: Option<String>,
        /// Message content.
        pub content: String,
        /// Raw message data.
        pub raw: Option<serde_json::Value>,
    }
}

// ============================================================================
// System Events
// ============================================================================

/// Event types for system notifications.
pub mod system {
    use super::*;

    /// Connection established event.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ConnectedEvent {
        /// Client ID assigned.
        pub client_id: Uuid,
        /// Server version.
        pub server_version: String,
        /// Protocol version.
        pub protocol_version: String,
    }

    /// Heartbeat event.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct HeartbeatEvent {
        /// Server timestamp.
        pub timestamp: i64,
    }

    /// Server shutting down event.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ShutdownEvent {
        /// Reason for shutdown.
        pub reason: Option<String>,
        /// Seconds until shutdown.
        pub in_seconds: Option<u32>,
    }
}

// ============================================================================
// Provider Events
// ============================================================================

/// Event types for provider state changes.
pub mod provider {
    use super::*;

    /// Provider changed event.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ChangedEvent {
        /// New active provider name.
        pub provider: String,
        /// Previous active provider name (if any).
        pub previous_provider: Option<String>,
        /// Optional reason for the change (e.g. failure message).
        pub reason: Option<String>,
    }
}

/// Event type constants.
pub mod event_types {
    // Chat events
    pub const CHAT_STREAM_START: &str = "chat.stream.start";
    pub const CHAT_STREAM_DELTA: &str = "chat.stream.delta";
    pub const CHAT_STREAM_COMPLETE: &str = "chat.stream.complete";
    pub const CHAT_STREAM_ERROR: &str = "chat.stream.error";

    // Session events
    pub const SESSION_CREATED: &str = "session.created";
    pub const SESSION_UPDATED: &str = "session.updated";
    pub const SESSION_DELETED: &str = "session.deleted";

    // Channel events
    pub const CHANNEL_CONNECTED: &str = "channel.connected";
    pub const CHANNEL_DISCONNECTED: &str = "channel.disconnected";
    pub const CHANNEL_MESSAGE: &str = "channel.message";

    // System events
    pub const SYSTEM_CONNECTED: &str = "system.connected";
    pub const SYSTEM_HEARTBEAT: &str = "system.heartbeat";
    pub const SYSTEM_SHUTDOWN: &str = "system.shutdown";

    // Provider events
    pub const PROVIDER_CHANGED: &str = "provider.changed";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_creation() {
        let event = Event::new(
            event_types::CHAT_STREAM_DELTA,
            chat::StreamDeltaEvent {
                request_id: Uuid::new_v4(),
                delta: "Hello".to_string(),
            },
        );

        assert_eq!(event.event_type, "chat.stream.delta");
    }

    #[test]
    fn test_event_serialization() {
        let event = Event::new(
            event_types::SYSTEM_HEARTBEAT,
            system::HeartbeatEvent {
                timestamp: 1234567890,
            },
        );

        let json = serde_json::to_string(&event).unwrap();
        let parsed: Event = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.event_type, "system.heartbeat");
    }
}
