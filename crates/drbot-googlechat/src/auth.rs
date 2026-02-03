//! Google Chat authentication.

use crate::{GoogleChatError, Result};
use chrono::{Duration, Utc};
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Authentication configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    /// Service account key file path.
    pub key_file: Option<String>,
    /// Service account key JSON (inline).
    pub key_json: Option<String>,
    /// OAuth2 scopes.
    #[serde(default = "default_scopes")]
    pub scopes: Vec<String>,
}

fn default_scopes() -> Vec<String> {
    vec!["https://www.googleapis.com/auth/chat.bot".to_string()]
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            key_file: None,
            key_json: None,
            scopes: default_scopes(),
        }
    }
}

/// Service account credentials.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceAccount {
    /// Service account type.
    #[serde(rename = "type")]
    pub account_type: String,
    /// Project ID.
    pub project_id: String,
    /// Private key ID.
    pub private_key_id: String,
    /// Private key (PEM format).
    pub private_key: String,
    /// Client email.
    pub client_email: String,
    /// Client ID.
    pub client_id: String,
    /// Auth URI.
    pub auth_uri: String,
    /// Token URI.
    pub token_uri: String,
}

impl ServiceAccount {
    /// Load from a file.
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| GoogleChatError::InvalidConfig(e.to_string()))?;
        Self::from_json(&content)
    }

    /// Load from JSON string.
    pub fn from_json(json: &str) -> Result<Self> {
        serde_json::from_str(json).map_err(|e| GoogleChatError::InvalidConfig(e.to_string()))
    }
}

/// Google Chat authentication handler.
pub struct GoogleChatAuth {
    /// Authentication config.
    config: AuthConfig,
    /// Service account credentials.
    service_account: Option<ServiceAccount>,
    /// Current access token.
    access_token: Option<String>,
    /// Token expiry time.
    token_expiry: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: i64,
    #[allow(dead_code)]
    token_type: Option<String>,
}

#[derive(Debug, Serialize)]
struct JwtClaims<'a> {
    iss: &'a str,
    scope: String,
    aud: &'a str,
    exp: usize,
    iat: usize,
}

impl GoogleChatAuth {
    /// Create a new auth handler.
    pub fn new(config: AuthConfig) -> Result<Self> {
        let service_account = if let Some(ref key_file) = config.key_file {
            Some(ServiceAccount::from_file(key_file)?)
        } else if let Some(ref key_json) = config.key_json {
            Some(ServiceAccount::from_json(key_json)?)
        } else {
            None
        };

        Ok(Self {
            config,
            service_account,
            access_token: None,
            token_expiry: None,
        })
    }

    /// Get or refresh the access token.
    pub async fn get_token(&mut self) -> Result<String> {
        // Check if current token is still valid
        if let (Some(token), Some(expiry)) = (&self.access_token, &self.token_expiry) {
            if chrono::Utc::now() < *expiry {
                return Ok(token.clone());
            }
        }

        // Need to refresh token
        self.refresh_token().await
    }

    /// Refresh the access token.
    async fn refresh_token(&mut self) -> Result<String> {
        let service_account = self
            .service_account
            .as_ref()
            .ok_or_else(|| GoogleChatError::AuthenticationFailed("No credentials".into()))?;

        let now = Utc::now();
        let iat = now.timestamp().max(0) as usize;
        let exp = (now + Duration::hours(1)).timestamp().max(0) as usize;

        let scopes = if self.config.scopes.is_empty() {
            default_scopes()
        } else {
            self.config.scopes.clone()
        };

        let claims = JwtClaims {
            iss: &service_account.client_email,
            scope: scopes.join(" "),
            aud: &service_account.token_uri,
            exp,
            iat,
        };

        let key =
            EncodingKey::from_rsa_pem(service_account.private_key.as_bytes()).map_err(|e| {
                GoogleChatError::AuthenticationFailed(format!("Invalid service account key: {e}"))
            })?;

        let mut header = Header::new(Algorithm::RS256);
        header.typ = Some("JWT".to_string());

        let assertion = jsonwebtoken::encode(&header, &claims, &key).map_err(|e| {
            GoogleChatError::AuthenticationFailed(format!("JWT signing failed: {e}"))
        })?;

        let client = reqwest::Client::new();
        let resp = client
            .post(&service_account.token_uri)
            .form(&[
                ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
                ("assertion", assertion.as_str()),
            ])
            .send()
            .await
            .map_err(|e| GoogleChatError::HttpError(e.to_string()))?;

        let status = resp.status();
        let body = resp
            .text()
            .await
            .map_err(|e| GoogleChatError::HttpError(e.to_string()))?;

        if !status.is_success() {
            return Err(GoogleChatError::AuthenticationFailed(format!(
                "Token refresh failed ({}): {}",
                status.as_u16(),
                body
            )));
        }

        let token: TokenResponse = serde_json::from_str(&body).map_err(|e| {
            GoogleChatError::AuthenticationFailed(format!("Bad token response: {e}"))
        })?;

        // Apply a small skew so callers don't hit expiry mid-request.
        let expires_at = now + Duration::seconds(token.expires_in.max(0)) - Duration::seconds(30);

        self.access_token = Some(token.access_token.clone());
        self.token_expiry = Some(expires_at);

        Ok(token.access_token)
    }

    /// Check if authenticated.
    pub fn is_authenticated(&self) -> bool {
        if let (Some(_), Some(expiry)) = (&self.access_token, &self.token_expiry) {
            chrono::Utc::now() < *expiry
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_config_default() {
        let config = AuthConfig::default();
        assert!(config.key_file.is_none());
        assert!(!config.scopes.is_empty());
    }
}
