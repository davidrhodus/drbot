//! Token generation and validation for drbot.
//!
//! This crate provides:
//! - Secure random token generation
//! - Token with expiration
//! - Token storage and validation
//! - Various token formats

use chrono::{DateTime, Duration, Utc};
use ring::rand::{SecureRandom, SystemRandom};
use std::collections::HashMap;
use thiserror::Error;

/// Token error types.
#[derive(Error, Debug)]
pub enum TokenError {
    #[error("Token not found")]
    NotFound,

    #[error("Token expired")]
    Expired,

    #[error("Token revoked")]
    Revoked,

    #[error("Invalid token format")]
    InvalidFormat,

    #[error("Generation error: {0}")]
    GenerationError(String),
}

/// Result type for token operations.
pub type Result<T> = std::result::Result<T, TokenError>;

/// Token type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenType {
    /// Access token.
    Access,
    /// Refresh token.
    Refresh,
    /// Verification token (email, etc).
    Verification,
    /// Password reset token.
    PasswordReset,
    /// API key.
    ApiKey,
    /// Session token.
    Session,
}

/// Token metadata.
#[derive(Debug, Clone)]
pub struct TokenMeta {
    /// Token value.
    pub token: String,
    /// Token type.
    pub token_type: TokenType,
    /// Associated user/entity ID.
    pub subject: Option<String>,
    /// Creation time.
    pub created_at: DateTime<Utc>,
    /// Expiration time.
    pub expires_at: Option<DateTime<Utc>>,
    /// Additional metadata.
    pub metadata: HashMap<String, String>,
    /// Is revoked.
    pub revoked: bool,
}

impl TokenMeta {
    /// Create new token metadata.
    pub fn new(token: String, token_type: TokenType) -> Self {
        Self {
            token,
            token_type,
            subject: None,
            created_at: Utc::now(),
            expires_at: None,
            metadata: HashMap::new(),
            revoked: false,
        }
    }

    /// Set subject.
    pub fn subject(mut self, subject: &str) -> Self {
        self.subject = Some(subject.to_string());
        self
    }

    /// Set expiration.
    pub fn expires_at(mut self, expires: DateTime<Utc>) -> Self {
        self.expires_at = Some(expires);
        self
    }

    /// Set expiration from duration.
    pub fn expires_in(mut self, duration: Duration) -> Self {
        self.expires_at = Some(Utc::now() + duration);
        self
    }

    /// Add metadata.
    pub fn metadata(mut self, key: &str, value: &str) -> Self {
        self.metadata.insert(key.to_string(), value.to_string());
        self
    }

    /// Check if expired.
    pub fn is_expired(&self) -> bool {
        self.expires_at.map(|exp| Utc::now() > exp).unwrap_or(false)
    }

    /// Check if valid (not expired and not revoked).
    pub fn is_valid(&self) -> bool {
        !self.revoked && !self.is_expired()
    }

    /// Revoke the token.
    pub fn revoke(&mut self) {
        self.revoked = true;
    }

    /// Time until expiration.
    pub fn time_remaining(&self) -> Option<Duration> {
        self.expires_at.map(|exp| exp - Utc::now())
    }
}

/// Token generator.
pub struct Generator {
    /// Token length in bytes.
    length: usize,
    /// Prefix for tokens.
    prefix: Option<String>,
}

impl Generator {
    /// Create new generator.
    pub fn new() -> Self {
        Self {
            length: 32,
            prefix: None,
        }
    }

    /// Set token length.
    pub fn length(mut self, length: usize) -> Self {
        self.length = length;
        self
    }

    /// Set token prefix.
    pub fn prefix(mut self, prefix: &str) -> Self {
        self.prefix = Some(prefix.to_string());
        self
    }

    /// Generate a random token.
    pub fn generate(&self) -> Result<String> {
        let rng = SystemRandom::new();
        let mut bytes = vec![0u8; self.length];
        rng.fill(&mut bytes)
            .map_err(|_| TokenError::GenerationError("Failed to generate random bytes".into()))?;

        let token = hex_encode(&bytes);

        if let Some(ref prefix) = self.prefix {
            Ok(format!("{}_{}", prefix, token))
        } else {
            Ok(token)
        }
    }

    /// Generate token with metadata.
    pub fn generate_with_meta(&self, token_type: TokenType) -> Result<TokenMeta> {
        let token = self.generate()?;
        Ok(TokenMeta::new(token, token_type))
    }
}

impl Default for Generator {
    fn default() -> Self {
        Self::new()
    }
}

/// In-memory token store.
pub struct TokenStore {
    tokens: HashMap<String, TokenMeta>,
}

impl TokenStore {
    /// Create new store.
    pub fn new() -> Self {
        Self {
            tokens: HashMap::new(),
        }
    }

    /// Store a token.
    pub fn store(&mut self, meta: TokenMeta) {
        self.tokens.insert(meta.token.clone(), meta);
    }

    /// Get token metadata.
    pub fn get(&self, token: &str) -> Result<&TokenMeta> {
        self.tokens.get(token).ok_or(TokenError::NotFound)
    }

    /// Validate and get token.
    pub fn validate(&self, token: &str) -> Result<&TokenMeta> {
        let meta = self.get(token)?;

        if meta.revoked {
            return Err(TokenError::Revoked);
        }

        if meta.is_expired() {
            return Err(TokenError::Expired);
        }

        Ok(meta)
    }

    /// Revoke a token.
    pub fn revoke(&mut self, token: &str) -> Result<()> {
        let meta = self.tokens.get_mut(token).ok_or(TokenError::NotFound)?;
        meta.revoke();
        Ok(())
    }

    /// Remove expired tokens.
    pub fn cleanup(&mut self) -> usize {
        let before = self.tokens.len();
        self.tokens.retain(|_, meta| !meta.is_expired());
        before - self.tokens.len()
    }

    /// Get all tokens for a subject.
    pub fn get_by_subject(&self, subject: &str) -> Vec<&TokenMeta> {
        self.tokens
            .values()
            .filter(|meta| meta.subject.as_deref() == Some(subject))
            .collect()
    }

    /// Revoke all tokens for a subject.
    pub fn revoke_by_subject(&mut self, subject: &str) {
        for meta in self.tokens.values_mut() {
            if meta.subject.as_deref() == Some(subject) {
                meta.revoke();
            }
        }
    }
}

impl Default for TokenStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Generate a simple random token.
pub fn generate(length: usize) -> Result<String> {
    Generator::new().length(length).generate()
}

/// Generate a hex token.
pub fn generate_hex(bytes: usize) -> Result<String> {
    let rng = SystemRandom::new();
    let mut data = vec![0u8; bytes];
    rng.fill(&mut data)
        .map_err(|_| TokenError::GenerationError("Failed to generate random bytes".into()))?;
    Ok(hex_encode(&data))
}

/// Generate a URL-safe token.
pub fn generate_url_safe(bytes: usize) -> Result<String> {
    let rng = SystemRandom::new();
    let mut data = vec![0u8; bytes];
    rng.fill(&mut data)
        .map_err(|_| TokenError::GenerationError("Failed to generate random bytes".into()))?;

    Ok(base64_url_encode(&data))
}

fn base64_url_encode(data: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

    let mut result = String::new();
    let mut i = 0;

    while i < data.len() {
        let b0 = data[i] as usize;
        result.push(ALPHABET[b0 >> 2] as char);

        if i + 1 < data.len() {
            let b1 = data[i + 1] as usize;
            result.push(ALPHABET[((b0 & 0x03) << 4) | (b1 >> 4)] as char);

            if i + 2 < data.len() {
                let b2 = data[i + 2] as usize;
                result.push(ALPHABET[((b1 & 0x0f) << 2) | (b2 >> 6)] as char);
                result.push(ALPHABET[b2 & 0x3f] as char);
            } else {
                result.push(ALPHABET[(b1 & 0x0f) << 2] as char);
            }
        } else {
            result.push(ALPHABET[(b0 & 0x03) << 4] as char);
        }

        i += 3;
    }

    result
}

fn hex_encode(data: &[u8]) -> String {
    data.iter().map(|b| format!("{:02x}", b)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_token() {
        let gen = Generator::new().length(16);
        let token = gen.generate().unwrap();

        // 16 bytes = 32 hex chars
        assert_eq!(token.len(), 32);
    }

    #[test]
    fn test_generate_with_prefix() {
        let gen = Generator::new().prefix("drbot");
        let token = gen.generate().unwrap();

        assert!(token.starts_with("drbot_"));
    }

    #[test]
    fn test_token_store() {
        let mut store = TokenStore::new();
        let gen = Generator::new();

        let meta = gen
            .generate_with_meta(TokenType::Access)
            .unwrap()
            .subject("user123")
            .expires_in(Duration::hours(1));

        let token = meta.token.clone();
        store.store(meta);

        assert!(store.validate(&token).is_ok());
    }

    #[test]
    fn test_token_expiration() {
        let meta =
            TokenMeta::new("test".into(), TokenType::Access).expires_in(Duration::seconds(-10)); // Already expired

        assert!(meta.is_expired());
        assert!(!meta.is_valid());
    }

    #[test]
    fn test_token_revocation() {
        let mut store = TokenStore::new();
        let meta = TokenMeta::new("test_token".into(), TokenType::Access);

        store.store(meta);
        assert!(store.validate("test_token").is_ok());

        store.revoke("test_token").unwrap();
        assert!(matches!(
            store.validate("test_token"),
            Err(TokenError::Revoked)
        ));
    }

    #[test]
    fn test_url_safe_token() {
        let token = generate_url_safe(32).unwrap();
        // URL-safe characters only
        assert!(token
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_'));
    }
}
