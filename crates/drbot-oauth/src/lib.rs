//! OAuth2 provider for third-party app authorization.
//!
//! This crate provides:
//! - OAuth2 authorization server
//! - Token management
//! - Client registration
//! - Scope management

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use uuid::Uuid;

/// OAuth errors.
#[derive(Debug, Error)]
pub enum OAuthError {
    #[error("Invalid client: {0}")]
    InvalidClient(String),

    #[error("Invalid grant: {0}")]
    InvalidGrant(String),

    #[error("Invalid scope: {0}")]
    InvalidScope(String),

    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    #[error("Token expired")]
    TokenExpired,

    #[error("Token revoked")]
    TokenRevoked,
}

/// Result type for OAuth operations.
pub type Result<T> = std::result::Result<T, OAuthError>;

/// OAuth client.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthClient {
    /// Client ID.
    pub client_id: String,
    /// Client secret (hashed).
    pub client_secret_hash: String,
    /// Client name.
    pub name: String,
    /// Redirect URIs.
    pub redirect_uris: Vec<String>,
    /// Allowed scopes.
    pub allowed_scopes: Vec<String>,
    /// Grant types.
    pub grant_types: Vec<GrantType>,
    /// Is confidential client.
    pub confidential: bool,
    /// Created at.
    pub created_at: DateTime<Utc>,
    /// Is active.
    pub active: bool,
}

/// Grant types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GrantType {
    AuthorizationCode,
    ClientCredentials,
    RefreshToken,
    Password,
}

/// Authorization code.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthorizationCode {
    /// Code value.
    pub code: String,
    /// Client ID.
    pub client_id: String,
    /// User ID.
    pub user_id: String,
    /// Redirect URI.
    pub redirect_uri: String,
    /// Scopes.
    pub scopes: Vec<String>,
    /// Code challenge (PKCE).
    pub code_challenge: Option<String>,
    /// Challenge method.
    pub code_challenge_method: Option<String>,
    /// Expires at.
    pub expires_at: DateTime<Utc>,
    /// Used.
    pub used: bool,
}

/// Access token.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessToken {
    /// Token value.
    pub token: String,
    /// Token type.
    pub token_type: String,
    /// Client ID.
    pub client_id: String,
    /// User ID (if user-based).
    pub user_id: Option<String>,
    /// Scopes.
    pub scopes: Vec<String>,
    /// Expires at.
    pub expires_at: DateTime<Utc>,
    /// Created at.
    pub created_at: DateTime<Utc>,
    /// Is revoked.
    pub revoked: bool,
}

/// Refresh token.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefreshToken {
    /// Token value.
    pub token: String,
    /// Associated access token.
    pub access_token: String,
    /// Client ID.
    pub client_id: String,
    /// User ID.
    pub user_id: Option<String>,
    /// Scopes.
    pub scopes: Vec<String>,
    /// Expires at.
    pub expires_at: DateTime<Utc>,
    /// Is revoked.
    pub revoked: bool,
}

/// Token response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenResponse {
    /// Access token.
    pub access_token: String,
    /// Token type.
    pub token_type: String,
    /// Expires in seconds.
    pub expires_in: u64,
    /// Refresh token (if applicable).
    pub refresh_token: Option<String>,
    /// Scopes.
    pub scope: String,
}

/// Scope definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scope {
    /// Scope name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Is default.
    pub is_default: bool,
}

/// Token configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenConfig {
    /// Access token lifetime in seconds.
    pub access_token_lifetime: u64,
    /// Refresh token lifetime in seconds.
    pub refresh_token_lifetime: u64,
    /// Authorization code lifetime in seconds.
    pub auth_code_lifetime: u64,
}

impl Default for TokenConfig {
    fn default() -> Self {
        Self {
            access_token_lifetime: 3600,     // 1 hour
            refresh_token_lifetime: 2592000, // 30 days
            auth_code_lifetime: 600,         // 10 minutes
        }
    }
}

/// The OAuth server.
pub struct OAuthServer {
    /// Registered clients.
    clients: Arc<RwLock<HashMap<String, OAuthClient>>>,
    /// Authorization codes.
    auth_codes: Arc<RwLock<HashMap<String, AuthorizationCode>>>,
    /// Access tokens.
    access_tokens: Arc<RwLock<HashMap<String, AccessToken>>>,
    /// Refresh tokens.
    refresh_tokens: Arc<RwLock<HashMap<String, RefreshToken>>>,
    /// Scopes.
    scopes: Arc<RwLock<HashMap<String, Scope>>>,
    /// Configuration.
    config: TokenConfig,
}

impl OAuthServer {
    /// Create a new OAuth server.
    pub fn new(config: TokenConfig) -> Self {
        Self {
            clients: Arc::new(RwLock::new(HashMap::new())),
            auth_codes: Arc::new(RwLock::new(HashMap::new())),
            access_tokens: Arc::new(RwLock::new(HashMap::new())),
            refresh_tokens: Arc::new(RwLock::new(HashMap::new())),
            scopes: Arc::new(RwLock::new(HashMap::new())),
            config,
        }
    }

    /// Register a client.
    pub async fn register_client(
        &self,
        name: &str,
        redirect_uris: Vec<String>,
        scopes: Vec<String>,
        grant_types: Vec<GrantType>,
        confidential: bool,
    ) -> (String, String) {
        let client_id = Uuid::new_v4().to_string();
        let client_secret = Uuid::new_v4().to_string();

        let client = OAuthClient {
            client_id: client_id.clone(),
            client_secret_hash: hash_secret(&client_secret),
            name: name.to_string(),
            redirect_uris,
            allowed_scopes: scopes,
            grant_types,
            confidential,
            created_at: Utc::now(),
            active: true,
        };

        let mut clients = self.clients.write().await;
        clients.insert(client_id.clone(), client);

        (client_id, client_secret)
    }

    /// Register a scope.
    pub async fn register_scope(&self, name: &str, description: &str, is_default: bool) {
        let scope = Scope {
            name: name.to_string(),
            description: description.to_string(),
            is_default,
        };

        let mut scopes = self.scopes.write().await;
        scopes.insert(name.to_string(), scope);
    }

    /// Create authorization code.
    pub async fn create_auth_code(
        &self,
        client_id: &str,
        user_id: &str,
        redirect_uri: &str,
        scopes: Vec<String>,
        code_challenge: Option<String>,
        code_challenge_method: Option<String>,
    ) -> Result<String> {
        // Validate client
        let clients = self.clients.read().await;
        let client = clients
            .get(client_id)
            .ok_or_else(|| OAuthError::InvalidClient(client_id.to_string()))?;

        if !client.active {
            return Err(OAuthError::InvalidClient("Client is inactive".to_string()));
        }

        // Validate redirect URI
        if !client.redirect_uris.contains(&redirect_uri.to_string()) {
            return Err(OAuthError::InvalidClient(
                "Invalid redirect URI".to_string(),
            ));
        }

        // Validate scopes
        self.validate_scopes(&scopes, &client.allowed_scopes)?;
        drop(clients);

        let code = generate_token();
        let auth_code = AuthorizationCode {
            code: code.clone(),
            client_id: client_id.to_string(),
            user_id: user_id.to_string(),
            redirect_uri: redirect_uri.to_string(),
            scopes,
            code_challenge,
            code_challenge_method,
            expires_at: Utc::now() + Duration::seconds(self.config.auth_code_lifetime as i64),
            used: false,
        };

        let mut codes = self.auth_codes.write().await;
        codes.insert(code.clone(), auth_code);

        Ok(code)
    }

    /// Exchange authorization code for tokens.
    pub async fn exchange_code(
        &self,
        client_id: &str,
        client_secret: &str,
        code: &str,
        redirect_uri: &str,
        code_verifier: Option<&str>,
    ) -> Result<TokenResponse> {
        // Validate client
        self.validate_client(client_id, client_secret).await?;

        // Get and validate code
        let mut codes = self.auth_codes.write().await;
        let auth_code = codes
            .get_mut(code)
            .ok_or_else(|| OAuthError::InvalidGrant("Invalid authorization code".to_string()))?;

        if auth_code.used {
            return Err(OAuthError::InvalidGrant("Code already used".to_string()));
        }

        if auth_code.expires_at < Utc::now() {
            return Err(OAuthError::InvalidGrant("Code expired".to_string()));
        }

        if auth_code.client_id != client_id {
            return Err(OAuthError::InvalidGrant("Client mismatch".to_string()));
        }

        if auth_code.redirect_uri != redirect_uri {
            return Err(OAuthError::InvalidGrant(
                "Redirect URI mismatch".to_string(),
            ));
        }

        // Verify PKCE if present
        if let Some(challenge) = &auth_code.code_challenge {
            let verifier = code_verifier
                .ok_or_else(|| OAuthError::InvalidGrant("Code verifier required".to_string()))?;
            let method = auth_code
                .code_challenge_method
                .as_deref()
                .unwrap_or("plain");

            let computed = match method {
                "S256" => base64_url_encode(&sha256(verifier)),
                _ => verifier.to_string(),
            };

            if &computed != challenge {
                return Err(OAuthError::InvalidGrant(
                    "Invalid code verifier".to_string(),
                ));
            }
        }

        // Mark code as used
        auth_code.used = true;
        let user_id = auth_code.user_id.clone();
        let scopes = auth_code.scopes.clone();
        drop(codes);

        // Generate tokens
        self.generate_tokens(client_id, Some(&user_id), scopes)
            .await
    }

    /// Client credentials grant.
    pub async fn client_credentials(
        &self,
        client_id: &str,
        client_secret: &str,
        scopes: Vec<String>,
    ) -> Result<TokenResponse> {
        let clients = self.clients.read().await;
        let client = self.validate_client(client_id, client_secret).await?;

        if !client.grant_types.contains(&GrantType::ClientCredentials) {
            return Err(OAuthError::InvalidGrant(
                "Grant type not allowed".to_string(),
            ));
        }

        self.validate_scopes(&scopes, &client.allowed_scopes)?;
        drop(clients);

        self.generate_tokens(client_id, None, scopes).await
    }

    /// Refresh token grant.
    pub async fn refresh_token(
        &self,
        client_id: &str,
        client_secret: &str,
        refresh_token: &str,
    ) -> Result<TokenResponse> {
        self.validate_client(client_id, client_secret).await?;

        let mut refresh_tokens = self.refresh_tokens.write().await;
        let rt = refresh_tokens
            .get_mut(refresh_token)
            .ok_or_else(|| OAuthError::InvalidGrant("Invalid refresh token".to_string()))?;

        if rt.revoked {
            return Err(OAuthError::TokenRevoked);
        }

        if rt.expires_at < Utc::now() {
            return Err(OAuthError::TokenExpired);
        }

        if rt.client_id != client_id {
            return Err(OAuthError::InvalidGrant("Client mismatch".to_string()));
        }

        // Revoke old refresh token
        rt.revoked = true;
        let user_id = rt.user_id.clone();
        let scopes = rt.scopes.clone();
        let old_access_token = rt.access_token.clone();
        drop(refresh_tokens);

        // Revoke old access token
        let mut access_tokens = self.access_tokens.write().await;
        if let Some(at) = access_tokens.get_mut(&old_access_token) {
            at.revoked = true;
        }
        drop(access_tokens);

        self.generate_tokens(client_id, user_id.as_deref(), scopes)
            .await
    }

    /// Validate access token.
    pub async fn validate_token(&self, token: &str) -> Result<AccessToken> {
        let tokens = self.access_tokens.read().await;
        let at = tokens
            .get(token)
            .ok_or_else(|| OAuthError::Unauthorized("Invalid token".to_string()))?;

        if at.revoked {
            return Err(OAuthError::TokenRevoked);
        }

        if at.expires_at < Utc::now() {
            return Err(OAuthError::TokenExpired);
        }

        Ok(at.clone())
    }

    /// Revoke token.
    pub async fn revoke_token(&self, token: &str) -> Result<()> {
        // Try access token
        let mut access_tokens = self.access_tokens.write().await;
        if let Some(at) = access_tokens.get_mut(token) {
            at.revoked = true;
            return Ok(());
        }
        drop(access_tokens);

        // Try refresh token
        let mut refresh_tokens = self.refresh_tokens.write().await;
        if let Some(rt) = refresh_tokens.get_mut(token) {
            rt.revoked = true;
            return Ok(());
        }

        Err(OAuthError::InvalidGrant("Token not found".to_string()))
    }

    /// Validate client credentials.
    async fn validate_client(&self, client_id: &str, client_secret: &str) -> Result<OAuthClient> {
        let clients = self.clients.read().await;
        let client = clients
            .get(client_id)
            .ok_or_else(|| OAuthError::InvalidClient(client_id.to_string()))?;

        if !client.active {
            return Err(OAuthError::InvalidClient("Client is inactive".to_string()));
        }

        if client.confidential && hash_secret(client_secret) != client.client_secret_hash {
            return Err(OAuthError::InvalidClient(
                "Invalid client secret".to_string(),
            ));
        }

        Ok(client.clone())
    }

    /// Validate scopes.
    fn validate_scopes(&self, requested: &[String], allowed: &[String]) -> Result<()> {
        for scope in requested {
            if !allowed.contains(scope) {
                return Err(OAuthError::InvalidScope(format!(
                    "Scope '{}' not allowed",
                    scope
                )));
            }
        }
        Ok(())
    }

    /// Generate tokens.
    async fn generate_tokens(
        &self,
        client_id: &str,
        user_id: Option<&str>,
        scopes: Vec<String>,
    ) -> Result<TokenResponse> {
        let access_token_value = generate_token();
        let refresh_token_value = generate_token();

        let access_token = AccessToken {
            token: access_token_value.clone(),
            token_type: "Bearer".to_string(),
            client_id: client_id.to_string(),
            user_id: user_id.map(String::from),
            scopes: scopes.clone(),
            expires_at: Utc::now() + Duration::seconds(self.config.access_token_lifetime as i64),
            created_at: Utc::now(),
            revoked: false,
        };

        let refresh_token = RefreshToken {
            token: refresh_token_value.clone(),
            access_token: access_token_value.clone(),
            client_id: client_id.to_string(),
            user_id: user_id.map(String::from),
            scopes: scopes.clone(),
            expires_at: Utc::now() + Duration::seconds(self.config.refresh_token_lifetime as i64),
            revoked: false,
        };

        let mut access_tokens = self.access_tokens.write().await;
        access_tokens.insert(access_token_value.clone(), access_token);
        drop(access_tokens);

        let mut refresh_tokens = self.refresh_tokens.write().await;
        refresh_tokens.insert(refresh_token_value.clone(), refresh_token);

        Ok(TokenResponse {
            access_token: access_token_value,
            token_type: "Bearer".to_string(),
            expires_in: self.config.access_token_lifetime,
            refresh_token: Some(refresh_token_value),
            scope: scopes.join(" "),
        })
    }

    /// Get client.
    pub async fn get_client(&self, client_id: &str) -> Option<OAuthClient> {
        let clients = self.clients.read().await;
        clients.get(client_id).cloned()
    }
}

impl Default for OAuthServer {
    fn default() -> Self {
        Self::new(TokenConfig::default())
    }
}

/// Generate a random token.
fn generate_token() -> String {
    Uuid::new_v4().to_string().replace("-", "") + &Uuid::new_v4().to_string().replace("-", "")
}

/// Hash a secret.
fn hash_secret(secret: &str) -> String {
    // In production, use proper hashing like bcrypt
    format!("hash:{}", secret)
}

/// SHA256 hash.
fn sha256(input: &str) -> Vec<u8> {
    // Simplified - in production use proper crypto
    input.as_bytes().to_vec()
}

/// Base64 URL encode.
fn base64_url_encode(data: &[u8]) -> String {
    // Simplified - in production use proper encoding
    data.iter().map(|b| format!("{:02x}", b)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_register_client() {
        let server = OAuthServer::default();

        let (client_id, _) = server
            .register_client(
                "Test App",
                vec!["https://example.com/callback".to_string()],
                vec!["read".to_string(), "write".to_string()],
                vec![GrantType::AuthorizationCode],
                true,
            )
            .await;

        let client = server.get_client(&client_id).await.unwrap();
        assert_eq!(client.name, "Test App");
    }

    #[tokio::test]
    async fn test_authorization_code_flow() {
        let server = OAuthServer::default();

        let (client_id, client_secret) = server
            .register_client(
                "Test App",
                vec!["https://example.com/callback".to_string()],
                vec!["read".to_string()],
                vec![GrantType::AuthorizationCode],
                true,
            )
            .await;

        // Create auth code
        let code = server
            .create_auth_code(
                &client_id,
                "user123",
                "https://example.com/callback",
                vec!["read".to_string()],
                None,
                None,
            )
            .await
            .unwrap();

        // Exchange for tokens
        let tokens = server
            .exchange_code(
                &client_id,
                &client_secret,
                &code,
                "https://example.com/callback",
                None,
            )
            .await
            .unwrap();

        assert!(!tokens.access_token.is_empty());
        assert!(tokens.refresh_token.is_some());
    }

    #[tokio::test]
    async fn test_client_credentials() {
        let server = OAuthServer::default();

        let (client_id, client_secret) = server
            .register_client(
                "Service",
                vec![],
                vec!["read".to_string()],
                vec![GrantType::ClientCredentials],
                true,
            )
            .await;

        let tokens = server
            .client_credentials(&client_id, &client_secret, vec!["read".to_string()])
            .await
            .unwrap();
        assert!(!tokens.access_token.is_empty());
    }

    #[tokio::test]
    async fn test_validate_token() {
        let server = OAuthServer::default();

        let (client_id, client_secret) = server
            .register_client(
                "Test",
                vec![],
                vec!["read".to_string()],
                vec![GrantType::ClientCredentials],
                true,
            )
            .await;

        let tokens = server
            .client_credentials(&client_id, &client_secret, vec!["read".to_string()])
            .await
            .unwrap();

        let validated = server.validate_token(&tokens.access_token).await.unwrap();
        assert_eq!(validated.client_id, client_id);
    }

    #[tokio::test]
    async fn test_revoke_token() {
        let server = OAuthServer::default();

        let (client_id, client_secret) = server
            .register_client(
                "Test",
                vec![],
                vec!["read".to_string()],
                vec![GrantType::ClientCredentials],
                true,
            )
            .await;

        let tokens = server
            .client_credentials(&client_id, &client_secret, vec!["read".to_string()])
            .await
            .unwrap();

        server.revoke_token(&tokens.access_token).await.unwrap();

        let result = server.validate_token(&tokens.access_token).await;
        assert!(matches!(result, Err(OAuthError::TokenRevoked)));
    }
}
