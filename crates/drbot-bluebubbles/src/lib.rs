//! BlueBubbles (iMessage) channel integration for drbot.
//!
//! This crate provides integration with BlueBubbles server for iMessage access.
//!
//! # Features
//!
//! - HTTP API + Socket.IO events
//! - Message sending and receiving
//! - Attachment support
//! - Implements the Channel trait

mod api;
mod channel;
mod webhook;

pub use api::{BlueBubblesApi, Chat, Handle, Message as BBMessage};
pub use channel::BlueBubblesChannel;
pub use webhook::{SocketEvent, SocketHandler};

use serde::{Deserialize, Serialize};

/// Result type for BlueBubbles operations.
pub type Result<T> = std::result::Result<T, BlueBubblesError>;

/// BlueBubbles errors.
#[derive(Debug, thiserror::Error)]
pub enum BlueBubblesError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),
    #[error("Authentication failed: {0}")]
    AuthenticationFailed(String),
    #[error("API error: {0}")]
    ApiError(String),
    #[error("Chat not found: {0}")]
    ChatNotFound(String),
    #[error("HTTP error: {0}")]
    HttpError(String),
}

/// BlueBubbles channel configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlueBubblesConfig {
    /// BlueBubbles server URL.
    pub server_url: String,
    /// Server password.
    pub password: String,
    /// Enable Socket.IO for real-time events.
    #[serde(default = "default_socket")]
    pub enable_socket: bool,
    /// Allowed phone numbers/emails (empty = all).
    #[serde(default)]
    pub allowed_handles: Vec<String>,
}

fn default_socket() -> bool {
    true
}

impl Default for BlueBubblesConfig {
    fn default() -> Self {
        Self {
            server_url: "http://localhost:1234".to_string(),
            password: String::new(),
            enable_socket: default_socket(),
            allowed_handles: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bluebubbles_config_default() {
        let config = BlueBubblesConfig::default();
        assert!(config.enable_socket);
        assert!(config.allowed_handles.is_empty());
    }
}
