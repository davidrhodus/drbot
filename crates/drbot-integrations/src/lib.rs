//! External service integrations for drbot.
//!
//! Provides connectors for external services like calendar, email, and project management.
//!
//! # Features
//!
//! - Calendar integration (Google, Outlook)
//! - Email integration
//! - Notion API
//! - Linear issues
//! - GitHub integration
//! - Webhook support

mod calendar;
mod email;
mod github;
mod linear;
mod notion;
mod webhooks;

pub use calendar::{CalendarConfig, CalendarEvent, CalendarProvider};
pub use email::{EmailConfig, EmailMessage, EmailProvider};
pub use github::{GitHubClient, GitHubIssue, GitHubPR};
pub use linear::{LinearClient, LinearIssue, LinearProject};
pub use notion::{NotionClient, NotionDatabase, NotionPage};
pub use webhooks::{WebhookConfig, WebhookEvent, WebhookManager};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Integration result type.
pub type Result<T> = std::result::Result<T, IntegrationError>;

/// Integration errors.
#[derive(Debug, thiserror::Error)]
pub enum IntegrationError {
    #[error("Authentication failed: {0}")]
    AuthFailed(String),
    #[error("API error: {0}")]
    ApiError(String),
    #[error("Rate limited")]
    RateLimited,
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Configuration error: {0}")]
    ConfigError(String),
    #[error("Network error: {0}")]
    NetworkError(String),
}

/// Integration provider trait.
#[async_trait]
pub trait IntegrationProvider: Send + Sync {
    /// Get provider name.
    fn name(&self) -> &str;

    /// Check if connected.
    async fn is_connected(&self) -> bool;

    /// Connect/authenticate.
    async fn connect(&mut self) -> Result<()>;

    /// Disconnect.
    async fn disconnect(&mut self) -> Result<()>;

    /// Refresh authentication.
    async fn refresh(&mut self) -> Result<()>;
}

/// OAuth credentials.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthCredentials {
    /// Access token.
    pub access_token: String,
    /// Refresh token.
    pub refresh_token: Option<String>,
    /// Token type.
    pub token_type: String,
    /// Expires at.
    pub expires_at: Option<DateTime<Utc>>,
    /// Scopes.
    pub scopes: Vec<String>,
}

/// API key credentials.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyCredentials {
    /// API key.
    pub api_key: String,
    /// Additional headers.
    pub headers: HashMap<String, String>,
}

/// Credentials type.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Credentials {
    OAuth(OAuthCredentials),
    ApiKey(ApiKeyCredentials),
    None,
}

/// Integration configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrationConfig {
    /// Integration ID.
    pub id: Uuid,
    /// Provider name.
    pub provider: String,
    /// Credentials.
    pub credentials: Credentials,
    /// Settings.
    pub settings: HashMap<String, serde_json::Value>,
    /// Is enabled.
    pub enabled: bool,
    /// Created at.
    pub created_at: DateTime<Utc>,
}

impl IntegrationConfig {
    /// Create a new integration config.
    pub fn new(provider: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            provider: provider.to_string(),
            credentials: Credentials::None,
            settings: HashMap::new(),
            enabled: true,
            created_at: Utc::now(),
        }
    }

    /// Set OAuth credentials.
    pub fn with_oauth(mut self, credentials: OAuthCredentials) -> Self {
        self.credentials = Credentials::OAuth(credentials);
        self
    }

    /// Set API key credentials.
    pub fn with_api_key(mut self, api_key: &str) -> Self {
        self.credentials = Credentials::ApiKey(ApiKeyCredentials {
            api_key: api_key.to_string(),
            headers: HashMap::new(),
        });
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_integration_config() {
        let config = IntegrationConfig::new("github").with_api_key("token123");

        assert_eq!(config.provider, "github");
        assert!(matches!(config.credentials, Credentials::ApiKey(_)));
    }
}
