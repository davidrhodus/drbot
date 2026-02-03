//! Discord API types.

use serde::{Deserialize, Serialize};

/// Discord Gateway opcodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Opcode {
    Dispatch = 0,
    Heartbeat = 1,
    Identify = 2,
    PresenceUpdate = 3,
    VoiceStateUpdate = 4,
    Resume = 6,
    Reconnect = 7,
    RequestGuildMembers = 8,
    InvalidSession = 9,
    Hello = 10,
    HeartbeatAck = 11,
}

impl From<u8> for Opcode {
    fn from(value: u8) -> Self {
        match value {
            0 => Opcode::Dispatch,
            1 => Opcode::Heartbeat,
            2 => Opcode::Identify,
            3 => Opcode::PresenceUpdate,
            4 => Opcode::VoiceStateUpdate,
            6 => Opcode::Resume,
            7 => Opcode::Reconnect,
            8 => Opcode::RequestGuildMembers,
            9 => Opcode::InvalidSession,
            10 => Opcode::Hello,
            11 => Opcode::HeartbeatAck,
            _ => Opcode::Dispatch,
        }
    }
}

/// Gateway payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayPayload {
    /// Opcode.
    pub op: u8,
    /// Event data.
    pub d: Option<serde_json::Value>,
    /// Sequence number (for resume).
    pub s: Option<u64>,
    /// Event name (for dispatch events).
    pub t: Option<String>,
}

/// Hello event data.
#[derive(Debug, Clone, Deserialize)]
pub struct HelloData {
    /// Heartbeat interval in milliseconds.
    pub heartbeat_interval: u64,
}

/// Identify payload.
#[derive(Debug, Clone, Serialize)]
pub struct IdentifyData {
    /// Bot token.
    pub token: String,
    /// Connection properties.
    pub properties: ConnectionProperties,
    /// Gateway intents.
    pub intents: u32,
}

/// Connection properties for identify.
#[derive(Debug, Clone, Serialize)]
pub struct ConnectionProperties {
    /// Operating system.
    pub os: String,
    /// Library name.
    pub browser: String,
    /// Library name.
    pub device: String,
}

/// Discord user object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    /// User ID.
    pub id: String,
    /// Username.
    pub username: String,
    /// Discriminator (legacy, may be "0").
    pub discriminator: String,
    /// Display name.
    pub global_name: Option<String>,
    /// Whether user is a bot.
    #[serde(default)]
    pub bot: bool,
}

impl User {
    /// Get display name.
    pub fn display_name(&self) -> &str {
        self.global_name.as_deref().unwrap_or(&self.username)
    }
}

/// Discord message object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Message ID.
    pub id: String,
    /// Channel ID.
    pub channel_id: String,
    /// Author.
    pub author: User,
    /// Content.
    pub content: String,
    /// Timestamp.
    pub timestamp: String,
    /// Guild ID (if in guild).
    pub guild_id: Option<String>,
    /// Referenced message (for replies).
    pub referenced_message: Option<Box<Message>>,
}

/// Ready event data.
#[derive(Debug, Clone, Deserialize)]
pub struct ReadyData {
    /// Gateway version.
    pub v: u8,
    /// Bot user.
    pub user: User,
    /// Session ID.
    pub session_id: String,
    /// Resume gateway URL.
    pub resume_gateway_url: String,
}

/// Create message request.
#[derive(Debug, Clone, Serialize)]
pub struct CreateMessageRequest {
    /// Message content.
    pub content: String,
    /// Message reference (for replies).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_reference: Option<MessageReference>,
}

/// Message reference for replies.
#[derive(Debug, Clone, Serialize)]
pub struct MessageReference {
    /// Message ID to reply to.
    pub message_id: String,
}

/// Gateway intents.
pub mod intents {
    pub const GUILDS: u32 = 1 << 0;
    pub const GUILD_MESSAGES: u32 = 1 << 9;
    pub const GUILD_MESSAGE_CONTENT: u32 = 1 << 15;
    pub const DIRECT_MESSAGES: u32 = 1 << 12;
    pub const MESSAGE_CONTENT: u32 = 1 << 15;
}
