//! Google Chat channel integration for drbot.
//!
//! This crate provides integration with Google Chat for messaging.
//!
//! # Features
//!
//! - OAuth2 / Service account authentication
//! - Incoming webhook handler
//! - Space and room management
//! - Implements the Channel trait

mod api;
mod auth;
mod channel;
mod webhook;

pub use api::{GoogleChatApi, Member, Message as ChatMessage, Space};
pub use auth::{AuthConfig, GoogleChatAuth, ServiceAccount};
pub use channel::GoogleChatChannel;
pub use webhook::{WebhookEvent, WebhookHandler};

use serde::{Deserialize, Serialize};

/// Result type for Google Chat operations.
pub type Result<T> = std::result::Result<T, GoogleChatError>;

/// Google Chat errors.
#[derive(Debug, thiserror::Error)]
pub enum GoogleChatError {
    #[error("Authentication failed: {0}")]
    AuthenticationFailed(String),
    #[error("API error: {0}")]
    ApiError(String),
    #[error("Space not found: {0}")]
    SpaceNotFound(String),
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),
    #[error("Rate limit exceeded")]
    RateLimitExceeded,
    #[error("HTTP error: {0}")]
    HttpError(String),
}

/// Google Chat channel configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoogleChatConfig {
    /// Path to service account credentials JSON.
    pub credentials_path: Option<String>,
    /// Service account credentials JSON (inline).
    pub credentials_json: Option<String>,
    /// Allowed spaces (empty = all).
    #[serde(default)]
    pub allowed_spaces: Vec<String>,
    /// Webhook URL for incoming messages.
    pub webhook_url: Option<String>,
    /// Enable bot mentions only.
    #[serde(default)]
    pub mentions_only: bool,
}

impl Default for GoogleChatConfig {
    fn default() -> Self {
        Self {
            credentials_path: None,
            credentials_json: None,
            allowed_spaces: Vec::new(),
            webhook_url: None,
            mentions_only: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_google_chat_config_default() {
        let config = GoogleChatConfig::default();
        assert!(config.allowed_spaces.is_empty());
        assert!(!config.mentions_only);
    }
}
