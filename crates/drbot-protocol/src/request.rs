//! Request types for the drbot protocol.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A client request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    /// Unique request ID.
    pub id: Uuid,
    /// Method name (e.g., "chat.send", "session.create").
    pub method: String,
    /// Method parameters.
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub params: serde_json::Value,
}

impl Request {
    /// Create a new request.
    pub fn new(id: Uuid, method: impl Into<String>, params: impl Serialize) -> Self {
        Self {
            id,
            method: method.into(),
            params: serde_json::to_value(params).unwrap_or(serde_json::Value::Null),
        }
    }

    /// Create a request with a new UUID.
    pub fn create(method: impl Into<String>, params: impl Serialize) -> Self {
        Self::new(Uuid::new_v4(), method, params)
    }
}

// ============================================================================
// Chat Methods
// ============================================================================

/// Parameters for chat.send method.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatSendParams {
    /// Session ID (optional, will create new if not provided).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<Uuid>,
    /// Message content.
    pub message: String,
    /// Model to use (optional, uses default if not provided).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Whether to stream the response.
    #[serde(default)]
    pub stream: bool,
    /// Additional options.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub options: Option<ChatOptions>,
}

/// Chat options.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChatOptions {
    /// Maximum tokens to generate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<usize>,
    /// Temperature (0.0 - 1.0).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// Top-p sampling.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    /// Stop sequences.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_sequences: Option<Vec<String>>,
    /// System prompt override.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
}

/// Parameters for chat.cancel method.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCancelParams {
    /// Request ID to cancel.
    pub request_id: Uuid,
}

// ============================================================================
// Session Methods
// ============================================================================

/// Parameters for session.create method.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionCreateParams {
    /// Optional title.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Optional model to use.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Optional system prompt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
}

/// Parameters for session.get method.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionGetParams {
    /// Session ID.
    pub session_id: Uuid,
}

/// Parameters for session.list method.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionListParams {
    /// Maximum number of sessions to return.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
    /// Offset for pagination.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset: Option<usize>,
    /// Filter by state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
}

/// Parameters for session.delete method.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionDeleteParams {
    /// Session ID.
    pub session_id: Uuid,
}

/// Parameters for session.clear method.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionClearParams {
    /// Session ID.
    pub session_id: Uuid,
}

// ============================================================================
// Auth Methods
// ============================================================================

/// Parameters for auth.login method.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthLoginParams {
    /// Authentication token.
    pub token: String,
}

/// Parameters for auth.logout method.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AuthLogoutParams {}

// ============================================================================
// Provider Methods
// ============================================================================

/// Parameters for provider.list method.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProviderListParams {}

/// Parameters for provider.models method.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderModelsParams {
    /// Provider name (optional, lists all if not provided).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
}

// ============================================================================
// Channel Methods
// ============================================================================

/// Parameters for channel.list method.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChannelListParams {}

/// Parameters for channel.status method.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelStatusParams {
    /// Channel type.
    pub channel_type: String,
}

// ============================================================================
// System Methods
// ============================================================================

/// Parameters for system.ping method.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SystemPingParams {}

/// Parameters for system.info method.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SystemInfoParams {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chat_send_params() {
        let params = ChatSendParams {
            session_id: Some(Uuid::new_v4()),
            message: "Hello".to_string(),
            model: Some("claude-3-opus".to_string()),
            stream: true,
            options: None,
        };

        let json = serde_json::to_string(&params).unwrap();
        let parsed: ChatSendParams = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.message, "Hello");
        assert!(parsed.stream);
    }
}
