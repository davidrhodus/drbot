//! Response types for the drbot protocol.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A server response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    /// Request ID this response corresponds to.
    pub id: Uuid,
    /// Result data (present on success).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    /// Error (present on failure).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<ResponseError>,
}

impl Response {
    /// Create a success response.
    pub fn success(id: Uuid, result: impl Serialize) -> Self {
        Self {
            id,
            result: Some(serde_json::to_value(result).unwrap_or(serde_json::Value::Null)),
            error: None,
        }
    }

    /// Create an error response.
    pub fn error(id: Uuid, code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            id,
            result: None,
            error: Some(ResponseError {
                code,
                message: message.into(),
                data: None,
            }),
        }
    }

    /// Create an error response with additional data.
    pub fn error_with_data(
        id: Uuid,
        code: ErrorCode,
        message: impl Into<String>,
        data: impl Serialize,
    ) -> Self {
        Self {
            id,
            result: None,
            error: Some(ResponseError {
                code,
                message: message.into(),
                data: Some(serde_json::to_value(data).unwrap_or(serde_json::Value::Null)),
            }),
        }
    }

    /// Check if the response is successful.
    pub fn is_success(&self) -> bool {
        self.error.is_none()
    }

    /// Check if the response is an error.
    pub fn is_error(&self) -> bool {
        self.error.is_some()
    }
}

/// Error information in a response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseError {
    /// Error code.
    pub code: ErrorCode,
    /// Human-readable error message.
    pub message: String,
    /// Additional error data.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

/// Error codes (based on JSON-RPC with extensions).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(into = "i32", from = "i32")]
pub enum ErrorCode {
    /// Parse error - invalid JSON.
    ParseError,
    /// Invalid request - not a valid request object.
    InvalidRequest,
    /// Method not found.
    MethodNotFound,
    /// Invalid parameters.
    InvalidParams,
    /// Internal error.
    InternalError,
    /// Authentication required.
    AuthRequired,
    /// Permission denied.
    PermissionDenied,
    /// Resource not found.
    NotFound,
    /// Rate limited.
    RateLimited,
    /// Request cancelled.
    Cancelled,
    /// Provider error.
    ProviderError,
    /// Channel error.
    ChannelError,
    /// Session error.
    SessionError,
    /// Unknown error code.
    Unknown(i32),
}

impl From<ErrorCode> for i32 {
    fn from(code: ErrorCode) -> i32 {
        match code {
            ErrorCode::ParseError => -32700,
            ErrorCode::InvalidRequest => -32600,
            ErrorCode::MethodNotFound => -32601,
            ErrorCode::InvalidParams => -32602,
            ErrorCode::InternalError => -32603,
            ErrorCode::AuthRequired => -32001,
            ErrorCode::PermissionDenied => -32002,
            ErrorCode::NotFound => -32003,
            ErrorCode::RateLimited => -32004,
            ErrorCode::Cancelled => -32005,
            ErrorCode::ProviderError => -32010,
            ErrorCode::ChannelError => -32011,
            ErrorCode::SessionError => -32012,
            ErrorCode::Unknown(code) => code,
        }
    }
}

impl From<i32> for ErrorCode {
    fn from(code: i32) -> ErrorCode {
        match code {
            -32700 => ErrorCode::ParseError,
            -32600 => ErrorCode::InvalidRequest,
            -32601 => ErrorCode::MethodNotFound,
            -32602 => ErrorCode::InvalidParams,
            -32603 => ErrorCode::InternalError,
            -32001 => ErrorCode::AuthRequired,
            -32002 => ErrorCode::PermissionDenied,
            -32003 => ErrorCode::NotFound,
            -32004 => ErrorCode::RateLimited,
            -32005 => ErrorCode::Cancelled,
            -32010 => ErrorCode::ProviderError,
            -32011 => ErrorCode::ChannelError,
            -32012 => ErrorCode::SessionError,
            code => ErrorCode::Unknown(code),
        }
    }
}

// ============================================================================
// Chat Results
// ============================================================================

/// Result of chat.send method.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatSendResult {
    /// Session ID.
    pub session_id: Uuid,
    /// Message ID.
    pub message_id: Uuid,
    /// Response content (if not streaming).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// Model used.
    pub model: String,
    /// Token usage.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<TokenUsage>,
}

/// Token usage information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    /// Input tokens.
    pub input_tokens: usize,
    /// Output tokens.
    pub output_tokens: usize,
}

// ============================================================================
// Session Results
// ============================================================================

/// Result of session.create method.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionCreateResult {
    /// Created session ID.
    pub session_id: Uuid,
}

/// Result of session.get method.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionGetResult {
    /// Session data.
    pub session: SessionInfo,
}

/// Session information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    /// Session ID.
    pub id: Uuid,
    /// Session title.
    pub title: Option<String>,
    /// Model used.
    pub model: Option<String>,
    /// Message count.
    pub message_count: usize,
    /// Created timestamp.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Last updated timestamp.
    pub updated_at: chrono::DateTime<chrono::Utc>,
    /// Session state.
    pub state: String,
}

/// Result of session.list method.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionListResult {
    /// List of sessions.
    pub sessions: Vec<SessionInfo>,
    /// Total count.
    pub total: usize,
}

// ============================================================================
// Auth Results
// ============================================================================

/// Result of auth.login method.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthLoginResult {
    /// Whether login was successful.
    pub success: bool,
    /// User ID.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_id: Option<Uuid>,
}

// ============================================================================
// Provider Results
// ============================================================================

/// Result of provider.list method.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderListResult {
    /// Available providers.
    pub providers: Vec<ProviderInfo>,
}

/// Provider information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderInfo {
    /// Provider name.
    pub name: String,
    /// Provider status.
    pub status: String,
    /// Available models.
    pub models: Vec<String>,
}

/// Result of provider.models method.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderModelsResult {
    /// Available models.
    pub models: Vec<ModelInfo>,
}

/// Model information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    /// Model ID.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Provider name.
    pub provider: String,
    /// Context window size.
    pub context_window: usize,
    /// Maximum output tokens.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<usize>,
}

// ============================================================================
// Channel Results
// ============================================================================

/// Result of channel.list method.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelListResult {
    /// Available channels.
    pub channels: Vec<ChannelInfo>,
}

/// Channel information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelInfo {
    /// Channel type.
    pub channel_type: String,
    /// Channel status.
    pub status: ChannelStatus,
    /// Connection time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connected_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Channel status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChannelStatus {
    /// Channel is connected.
    Connected,
    /// Channel is disconnected.
    Disconnected,
    /// Channel is connecting.
    Connecting,
    /// Channel encountered an error.
    Error,
}

// ============================================================================
// System Results
// ============================================================================

/// Result of system.ping method.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemPingResult {
    /// Pong response.
    pub pong: bool,
    /// Server timestamp.
    pub timestamp: i64,
}

/// Result of system.info method.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemInfoResult {
    /// Server version.
    pub version: String,
    /// Protocol version.
    pub protocol_version: String,
    /// Uptime in seconds.
    pub uptime_secs: u64,
    /// Connected clients.
    pub connected_clients: usize,
    /// Active sessions.
    pub active_sessions: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_code_conversion() {
        let code = ErrorCode::AuthRequired;
        let num: i32 = code.into();
        assert_eq!(num, -32001);

        let back: ErrorCode = num.into();
        assert_eq!(back, ErrorCode::AuthRequired);
    }

    #[test]
    fn test_response_success() {
        let id = Uuid::new_v4();
        let resp = Response::success(id, serde_json::json!({"status": "ok"}));
        assert!(resp.is_success());
        assert!(!resp.is_error());
    }

    #[test]
    fn test_response_error() {
        let id = Uuid::new_v4();
        let resp = Response::error(id, ErrorCode::NotFound, "Session not found");
        assert!(!resp.is_success());
        assert!(resp.is_error());
        assert_eq!(resp.error.unwrap().code, ErrorCode::NotFound);
    }
}
