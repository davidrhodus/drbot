//! Microsoft Teams channel integration for drbot.
//!
//! This crate provides integration with Microsoft Teams for messaging.
//!
//! # Features
//!
//! - Azure AD OAuth2 authentication
//! - Microsoft Graph API integration
//! - Bot Framework support
//! - Activity notifications and proactive messaging
//! - Implements the Channel trait
//!
//! # Example
//!
//! ```rust,no_run
//! use drbot_msteams::{MsTeamsChannel, MsTeamsConfig};
//!
//! async fn example() {
//!     let config = MsTeamsConfig {
//!         tenant_id: "your-tenant-id".to_string(),
//!         client_id: "your-client-id".to_string(),
//!         client_secret: "your-client-secret".to_string(),
//!         bot_app_id: "your-bot-app-id".to_string(),
//!         ..Default::default()
//!     };
//!
//!     let channel = MsTeamsChannel::new(config).await.unwrap();
//! }
//! ```

mod api;
mod auth;
mod bot;
mod channel;

pub use api::{GraphApi, Team, TeamsChannel as ApiChannel, TeamsMember, TeamsMessage};
pub use auth::{AuthConfig, AzureAuth, TokenResponse};
pub use bot::{Activity, ActivityType, BotFramework, ConversationReference};
pub use channel::MsTeamsChannel;

use serde::{Deserialize, Serialize};

/// Result type for Microsoft Teams operations.
pub type Result<T> = std::result::Result<T, MsTeamsError>;

/// Microsoft Teams errors.
#[derive(Debug, thiserror::Error)]
pub enum MsTeamsError {
    #[error("Authentication failed: {0}")]
    AuthenticationFailed(String),
    #[error("Token expired")]
    TokenExpired,
    #[error("API error: {0}")]
    ApiError(String),
    #[error("Team not found: {0}")]
    TeamNotFound(String),
    #[error("Channel not found: {0}")]
    ChannelNotFound(String),
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),
    #[error("Rate limit exceeded")]
    RateLimitExceeded,
    #[error("HTTP error: {0}")]
    HttpError(String),
    #[error("Bot framework error: {0}")]
    BotFrameworkError(String),
}

/// Microsoft Teams channel configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MsTeamsConfig {
    /// Azure AD tenant ID.
    pub tenant_id: String,
    /// Azure AD application (client) ID.
    pub client_id: String,
    /// Azure AD client secret.
    pub client_secret: String,
    /// Bot application ID (for Bot Framework).
    pub bot_app_id: String,
    /// Messaging endpoint URL.
    #[serde(default)]
    pub messaging_endpoint: Option<String>,
    /// Allowed teams (empty = all).
    #[serde(default)]
    pub allowed_teams: Vec<String>,
    /// Allowed channels (empty = all).
    #[serde(default)]
    pub allowed_channels: Vec<String>,
    /// Enable proactive messaging.
    #[serde(default)]
    pub enable_proactive: bool,
    /// Enable activity notifications.
    #[serde(default = "default_notifications")]
    pub enable_notifications: bool,
}

fn default_notifications() -> bool {
    true
}

impl Default for MsTeamsConfig {
    fn default() -> Self {
        Self {
            tenant_id: String::new(),
            client_id: String::new(),
            client_secret: String::new(),
            bot_app_id: String::new(),
            messaging_endpoint: None,
            allowed_teams: Vec::new(),
            allowed_channels: Vec::new(),
            enable_proactive: false,
            enable_notifications: default_notifications(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_msteams_config_default() {
        let config = MsTeamsConfig::default();
        assert!(config.allowed_teams.is_empty());
        assert!(config.enable_notifications);
        assert!(!config.enable_proactive);
    }
}
