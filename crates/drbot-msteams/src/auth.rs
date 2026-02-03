//! Azure AD OAuth2 authentication for Microsoft Teams.

use crate::{MsTeamsError, Result};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Authentication configuration.
#[derive(Debug, Clone)]
pub struct AuthConfig {
    /// Tenant ID.
    pub tenant_id: String,
    /// Client ID.
    pub client_id: String,
    /// Client secret.
    pub client_secret: String,
    /// Scopes to request.
    pub scopes: Vec<String>,
}

impl AuthConfig {
    /// Create a new auth config for Microsoft Graph.
    pub fn for_graph(tenant_id: &str, client_id: &str, client_secret: &str) -> Self {
        Self {
            tenant_id: tenant_id.to_string(),
            client_id: client_id.to_string(),
            client_secret: client_secret.to_string(),
            scopes: vec!["https://graph.microsoft.com/.default".to_string()],
        }
    }

    /// Create a new auth config for Bot Framework.
    pub fn for_bot_framework(client_id: &str, client_secret: &str) -> Self {
        Self {
            tenant_id: "botframework.com".to_string(),
            client_id: client_id.to_string(),
            client_secret: client_secret.to_string(),
            scopes: vec!["https://api.botframework.com/.default".to_string()],
        }
    }
}

/// OAuth2 token response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenResponse {
    /// Access token.
    pub access_token: String,
    /// Token type (usually "Bearer").
    pub token_type: String,
    /// Expires in seconds.
    pub expires_in: u64,
    /// Resource/scope.
    #[serde(default)]
    pub resource: Option<String>,
}

/// Cached token with expiry.
#[derive(Debug, Clone)]
struct CachedToken {
    token: String,
    expires_at: DateTime<Utc>,
}

impl CachedToken {
    fn is_expired(&self) -> bool {
        // Consider expired 5 minutes before actual expiry
        Utc::now() + Duration::minutes(5) > self.expires_at
    }
}

/// Azure AD authentication handler.
pub struct AzureAuth {
    config: AuthConfig,
    client: reqwest::Client,
    cached_token: Arc<RwLock<Option<CachedToken>>>,
}

impl AzureAuth {
    /// Create a new Azure auth handler.
    pub fn new(config: AuthConfig) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
            cached_token: Arc::new(RwLock::new(None)),
        }
    }

    /// Get the token endpoint URL.
    fn token_endpoint(&self) -> String {
        format!(
            "https://login.microsoftonline.com/{}/oauth2/v2.0/token",
            self.config.tenant_id
        )
    }

    /// Acquire a new access token.
    async fn acquire_token(&self) -> Result<TokenResponse> {
        let scopes = self.config.scopes.join(" ");

        let params = [
            ("client_id", self.config.client_id.as_str()),
            ("client_secret", self.config.client_secret.as_str()),
            ("scope", &scopes),
            ("grant_type", "client_credentials"),
        ];

        let response = self
            .client
            .post(&self.token_endpoint())
            .form(&params)
            .send()
            .await
            .map_err(|e| MsTeamsError::HttpError(e.to_string()))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(MsTeamsError::AuthenticationFailed(error_text));
        }

        let token_response: TokenResponse = response
            .json()
            .await
            .map_err(|e| MsTeamsError::AuthenticationFailed(e.to_string()))?;

        Ok(token_response)
    }

    /// Get a valid access token (from cache or acquire new).
    pub async fn get_token(&self) -> Result<String> {
        // Check cache
        {
            let cache = self.cached_token.read().await;
            if let Some(cached) = cache.as_ref() {
                if !cached.is_expired() {
                    return Ok(cached.token.clone());
                }
            }
        }

        // Acquire new token
        let token_response = self.acquire_token().await?;

        // Cache it
        let expires_at = Utc::now() + Duration::seconds(token_response.expires_in as i64);
        let cached = CachedToken {
            token: token_response.access_token.clone(),
            expires_at,
        };

        {
            let mut cache = self.cached_token.write().await;
            *cache = Some(cached);
        }

        Ok(token_response.access_token)
    }

    /// Force refresh the token.
    pub async fn refresh(&self) -> Result<String> {
        {
            let mut cache = self.cached_token.write().await;
            *cache = None;
        }
        self.get_token().await
    }

    /// Clear the cached token.
    pub async fn clear_cache(&self) {
        let mut cache = self.cached_token.write().await;
        *cache = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_config() {
        let config = AuthConfig::for_graph("tenant", "client", "secret");
        assert!(config.scopes[0].contains("graph.microsoft.com"));

        let bot_config = AuthConfig::for_bot_framework("client", "secret");
        assert!(bot_config.scopes[0].contains("botframework.com"));
    }
}
