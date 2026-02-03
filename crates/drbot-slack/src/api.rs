//! Slack API types.

use serde::{Deserialize, Serialize};

/// Slack event wrapper.
#[derive(Debug, Clone, Deserialize)]
pub struct SlackEnvelope {
    /// Envelope ID for acknowledgment.
    pub envelope_id: String,
    /// Type of envelope.
    #[serde(rename = "type")]
    pub envelope_type: String,
    /// Payload data.
    pub payload: Option<EventPayload>,
    /// Retry information.
    pub retry_attempt: Option<u32>,
    /// Retry reason.
    pub retry_reason: Option<String>,
}

/// Event payload.
#[derive(Debug, Clone, Deserialize)]
pub struct EventPayload {
    /// Event type.
    #[serde(rename = "type")]
    pub event_type: String,
    /// Event data.
    pub event: Option<SlackEvent>,
    /// Event ID.
    pub event_id: Option<String>,
    /// Event time.
    pub event_time: Option<u64>,
}

/// Slack event types.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum SlackEvent {
    /// Message event.
    #[serde(rename = "message")]
    Message(MessageEvent),
    /// App mention event.
    #[serde(rename = "app_mention")]
    AppMention(MessageEvent),
    /// Other event types.
    #[serde(other)]
    Unknown,
}

/// Message event data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageEvent {
    /// Channel ID.
    pub channel: String,
    /// User ID who sent the message.
    pub user: Option<String>,
    /// Message text.
    pub text: String,
    /// Message timestamp (unique ID).
    pub ts: String,
    /// Thread timestamp (if in thread).
    pub thread_ts: Option<String>,
    /// Bot ID (if from a bot).
    pub bot_id: Option<String>,
    /// Subtype (e.g., "bot_message").
    pub subtype: Option<String>,
}

/// Socket Mode hello message.
#[derive(Debug, Clone, Deserialize)]
pub struct HelloMessage {
    /// Type is "hello".
    #[serde(rename = "type")]
    pub msg_type: String,
    /// Number of connections.
    pub num_connections: Option<u32>,
    /// Connection info.
    pub connection_info: Option<ConnectionInfo>,
}

/// Connection info from hello.
#[derive(Debug, Clone, Deserialize)]
pub struct ConnectionInfo {
    /// App ID.
    pub app_id: String,
}

/// Acknowledgment message to send.
#[derive(Debug, Clone, Serialize)]
pub struct Acknowledgment {
    /// Envelope ID to acknowledge.
    pub envelope_id: String,
}

/// Web API response wrapper.
#[derive(Debug, Clone, Deserialize)]
pub struct SlackApiResponse<T> {
    /// Whether the request succeeded.
    pub ok: bool,
    /// Error message if not ok.
    pub error: Option<String>,
    /// Response data.
    #[serde(flatten)]
    pub data: Option<T>,
}

/// apps.connections.open response.
#[derive(Debug, Clone, Deserialize)]
pub struct ConnectionsOpenResponse {
    /// WebSocket URL to connect to.
    pub url: Option<String>,
}

/// chat.postMessage request.
#[derive(Debug, Clone, Serialize)]
pub struct PostMessageRequest {
    /// Channel ID.
    pub channel: String,
    /// Message text.
    pub text: String,
    /// Thread timestamp (for replies).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_ts: Option<String>,
}

/// chat.postMessage response.
#[derive(Debug, Clone, Deserialize)]
pub struct PostMessageResponse {
    /// Posted message timestamp.
    pub ts: Option<String>,
    /// Channel ID.
    pub channel: Option<String>,
}

/// auth.test response.
#[derive(Debug, Clone, Deserialize)]
pub struct AuthTestResponse {
    /// Bot user ID.
    pub user_id: Option<String>,
    /// Bot ID.
    pub bot_id: Option<String>,
    /// Team ID.
    pub team_id: Option<String>,
    /// Team name.
    pub team: Option<String>,
    /// User name.
    pub user: Option<String>,
}

/// users.info response.
#[derive(Debug, Clone, Deserialize)]
pub struct UsersInfoResponse {
    /// User object.
    pub user: Option<SlackUser>,
}

/// Slack user object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlackUser {
    /// User ID.
    pub id: String,
    /// Team ID.
    pub team_id: Option<String>,
    /// Username.
    pub name: String,
    /// Real name.
    pub real_name: Option<String>,
    /// Profile info.
    pub profile: Option<UserProfile>,
    /// Whether user is a bot.
    #[serde(default)]
    pub is_bot: bool,
}

/// User profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserProfile {
    /// Display name.
    pub display_name: Option<String>,
    /// Real name.
    pub real_name: Option<String>,
    /// Email.
    pub email: Option<String>,
}

impl SlackUser {
    /// Get display name.
    pub fn display_name(&self) -> &str {
        self.profile
            .as_ref()
            .and_then(|p| p.display_name.as_deref())
            .filter(|s| !s.is_empty())
            .or(self.real_name.as_deref())
            .unwrap_or(&self.name)
    }
}
