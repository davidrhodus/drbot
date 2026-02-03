//! API key management for drbot.
//!
//! This crate provides:
//! - API key generation
//! - Key validation and verification
//! - Key metadata and permissions
//! - Key rotation support

use chrono::{DateTime, Duration, Utc};
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use thiserror::Error;

/// API key error types.
#[derive(Error, Debug)]
pub enum ApiKeyError {
    #[error("Key not found")]
    NotFound,

    #[error("Key expired")]
    Expired,

    #[error("Key revoked")]
    Revoked,

    #[error("Key inactive")]
    Inactive,

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Rate limit exceeded")]
    RateLimitExceeded,

    #[error("Invalid key format")]
    InvalidFormat,

    #[error("Generation error: {0}")]
    GenerationError(String),
}

/// Result type for API key operations.
pub type Result<T> = std::result::Result<T, ApiKeyError>;

/// API key status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeyStatus {
    /// Key is active.
    Active,
    /// Key is inactive (disabled).
    Inactive,
    /// Key is revoked (permanently disabled).
    Revoked,
    /// Key is expired.
    Expired,
}

/// API key metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKey {
    /// Key ID (public identifier).
    pub id: String,
    /// Key hash (for verification).
    pub key_hash: String,
    /// Key prefix (for identification).
    pub prefix: String,
    /// Display name.
    pub name: String,
    /// Description.
    pub description: Option<String>,
    /// Owner/user ID.
    pub owner_id: String,
    /// Permissions/scopes.
    pub permissions: HashSet<String>,
    /// Creation time.
    pub created_at: DateTime<Utc>,
    /// Expiration time.
    pub expires_at: Option<DateTime<Utc>>,
    /// Last used time.
    pub last_used_at: Option<DateTime<Utc>>,
    /// Status.
    pub status: KeyStatus,
    /// Request count.
    pub request_count: u64,
    /// Rate limit (requests per hour).
    pub rate_limit: Option<u32>,
    /// Additional metadata.
    pub metadata: HashMap<String, String>,
}

impl ApiKey {
    /// Check if key is valid.
    pub fn is_valid(&self) -> bool {
        self.status == KeyStatus::Active && !self.is_expired()
    }

    /// Check if key is expired.
    pub fn is_expired(&self) -> bool {
        self.expires_at.map(|exp| Utc::now() > exp).unwrap_or(false)
    }

    /// Check if key has permission.
    pub fn has_permission(&self, permission: &str) -> bool {
        self.permissions.contains(permission) || self.permissions.contains("*")
    }

    /// Check all permissions.
    pub fn has_all_permissions(&self, permissions: &[&str]) -> bool {
        permissions.iter().all(|p| self.has_permission(p))
    }

    /// Check any permission.
    pub fn has_any_permission(&self, permissions: &[&str]) -> bool {
        permissions.iter().any(|p| self.has_permission(p))
    }
}

/// Generated key result.
#[derive(Debug, Clone)]
pub struct GeneratedKey {
    /// Full key (only available at creation).
    pub key: String,
    /// Key metadata.
    pub api_key: ApiKey,
}

/// API key generator.
pub struct Generator {
    /// Key prefix.
    prefix: String,
    /// Key length in bytes.
    length: usize,
}

impl Generator {
    /// Create new generator.
    pub fn new(prefix: &str) -> Self {
        Self {
            prefix: prefix.to_string(),
            length: 32,
        }
    }

    /// Set key length.
    pub fn length(mut self, length: usize) -> Self {
        self.length = length;
        self
    }

    /// Generate new API key.
    pub fn generate(&self, name: &str, owner_id: &str) -> Result<GeneratedKey> {
        let rng = SystemRandom::new();

        // Generate key bytes
        let mut key_bytes = vec![0u8; self.length];
        rng.fill(&mut key_bytes)
            .map_err(|_| ApiKeyError::GenerationError("Failed to generate random bytes".into()))?;

        // Generate key ID
        let mut id_bytes = [0u8; 8];
        rng.fill(&mut id_bytes)
            .map_err(|_| ApiKeyError::GenerationError("Failed to generate ID".into()))?;

        let key_hex = hex_encode(&key_bytes);
        let full_key = format!("{}_{}", self.prefix, key_hex);
        let key_hash = hash_key(&full_key);

        let api_key = ApiKey {
            id: hex_encode(&id_bytes),
            key_hash,
            prefix: self.prefix.clone(),
            name: name.to_string(),
            description: None,
            owner_id: owner_id.to_string(),
            permissions: HashSet::new(),
            created_at: Utc::now(),
            expires_at: None,
            last_used_at: None,
            status: KeyStatus::Active,
            request_count: 0,
            rate_limit: None,
            metadata: HashMap::new(),
        };

        Ok(GeneratedKey {
            key: full_key,
            api_key,
        })
    }
}

/// API key store.
pub struct KeyStore {
    keys: HashMap<String, ApiKey>,
    key_hashes: HashMap<String, String>, // hash -> id
}

impl KeyStore {
    /// Create new store.
    pub fn new() -> Self {
        Self {
            keys: HashMap::new(),
            key_hashes: HashMap::new(),
        }
    }

    /// Store an API key.
    pub fn store(&mut self, key: ApiKey) {
        self.key_hashes.insert(key.key_hash.clone(), key.id.clone());
        self.keys.insert(key.id.clone(), key);
    }

    /// Get key by ID.
    pub fn get(&self, id: &str) -> Option<&ApiKey> {
        self.keys.get(id)
    }

    /// Get mutable key by ID.
    pub fn get_mut(&mut self, id: &str) -> Option<&mut ApiKey> {
        self.keys.get_mut(id)
    }

    /// Validate a key and return its metadata.
    pub fn validate(&mut self, key: &str) -> Result<&ApiKey> {
        let key_hash = hash_key(key);

        let id = self
            .key_hashes
            .get(&key_hash)
            .ok_or(ApiKeyError::NotFound)?;
        let api_key = self.keys.get_mut(id).ok_or(ApiKeyError::NotFound)?;

        // Update last used
        api_key.last_used_at = Some(Utc::now());
        api_key.request_count += 1;

        // Check status
        match api_key.status {
            KeyStatus::Inactive => return Err(ApiKeyError::Inactive),
            KeyStatus::Revoked => return Err(ApiKeyError::Revoked),
            KeyStatus::Expired => return Err(ApiKeyError::Expired),
            KeyStatus::Active => {}
        }

        // Check expiration
        if api_key.is_expired() {
            api_key.status = KeyStatus::Expired;
            return Err(ApiKeyError::Expired);
        }

        Ok(api_key)
    }

    /// Validate key with required permissions.
    pub fn validate_with_permissions(&mut self, key: &str, required: &[&str]) -> Result<&ApiKey> {
        let api_key = self.validate(key)?;

        if !api_key.has_all_permissions(required) {
            return Err(ApiKeyError::PermissionDenied(required.join(", ")));
        }

        Ok(api_key)
    }

    /// Revoke a key.
    pub fn revoke(&mut self, id: &str) -> Result<()> {
        let key = self.keys.get_mut(id).ok_or(ApiKeyError::NotFound)?;
        key.status = KeyStatus::Revoked;
        Ok(())
    }

    /// Deactivate a key.
    pub fn deactivate(&mut self, id: &str) -> Result<()> {
        let key = self.keys.get_mut(id).ok_or(ApiKeyError::NotFound)?;
        key.status = KeyStatus::Inactive;
        Ok(())
    }

    /// Activate a key.
    pub fn activate(&mut self, id: &str) -> Result<()> {
        let key = self.keys.get_mut(id).ok_or(ApiKeyError::NotFound)?;
        if key.status == KeyStatus::Revoked {
            return Err(ApiKeyError::Revoked);
        }
        key.status = KeyStatus::Active;
        Ok(())
    }

    /// Get all keys for an owner.
    pub fn get_by_owner(&self, owner_id: &str) -> Vec<&ApiKey> {
        self.keys
            .values()
            .filter(|k| k.owner_id == owner_id)
            .collect()
    }

    /// Delete expired keys.
    pub fn cleanup_expired(&mut self) -> usize {
        let expired: Vec<String> = self
            .keys
            .iter()
            .filter(|(_, k)| k.is_expired())
            .map(|(id, _)| id.clone())
            .collect();

        let count = expired.len();
        for id in expired {
            if let Some(key) = self.keys.remove(&id) {
                self.key_hashes.remove(&key.key_hash);
            }
        }
        count
    }
}

impl Default for KeyStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Key builder for creating keys with options.
pub struct KeyBuilder {
    name: String,
    owner_id: String,
    description: Option<String>,
    permissions: HashSet<String>,
    expires_in: Option<Duration>,
    rate_limit: Option<u32>,
    metadata: HashMap<String, String>,
}

impl KeyBuilder {
    /// Create new builder.
    pub fn new(name: &str, owner_id: &str) -> Self {
        Self {
            name: name.to_string(),
            owner_id: owner_id.to_string(),
            description: None,
            permissions: HashSet::new(),
            expires_in: None,
            rate_limit: None,
            metadata: HashMap::new(),
        }
    }

    /// Set description.
    pub fn description(mut self, description: &str) -> Self {
        self.description = Some(description.to_string());
        self
    }

    /// Add permission.
    pub fn permission(mut self, permission: &str) -> Self {
        self.permissions.insert(permission.to_string());
        self
    }

    /// Add multiple permissions.
    pub fn permissions(mut self, permissions: &[&str]) -> Self {
        for p in permissions {
            self.permissions.insert((*p).to_string());
        }
        self
    }

    /// Set expiration.
    pub fn expires_in(mut self, duration: Duration) -> Self {
        self.expires_in = Some(duration);
        self
    }

    /// Set rate limit.
    pub fn rate_limit(mut self, limit: u32) -> Self {
        self.rate_limit = Some(limit);
        self
    }

    /// Add metadata.
    pub fn metadata(mut self, key: &str, value: &str) -> Self {
        self.metadata.insert(key.to_string(), value.to_string());
        self
    }

    /// Build the key.
    pub fn build(self, generator: &Generator) -> Result<GeneratedKey> {
        let mut result = generator.generate(&self.name, &self.owner_id)?;

        result.api_key.description = self.description;
        result.api_key.permissions = self.permissions;
        result.api_key.rate_limit = self.rate_limit;
        result.api_key.metadata = self.metadata;

        if let Some(duration) = self.expires_in {
            result.api_key.expires_at = Some(Utc::now() + duration);
        }

        Ok(result)
    }
}

fn hex_encode(data: &[u8]) -> String {
    data.iter().map(|b| format!("{:02x}", b)).collect()
}

fn hash_key(key: &str) -> String {
    use ring::digest;
    let hash = digest::digest(&digest::SHA256, key.as_bytes());
    hex_encode(hash.as_ref())
}

/// Extract prefix from API key.
pub fn extract_prefix(key: &str) -> Option<&str> {
    key.split('_').next()
}

/// Validate key format.
pub fn is_valid_format(key: &str) -> bool {
    let parts: Vec<&str> = key.split('_').collect();
    parts.len() == 2 && !parts[0].is_empty() && parts[1].len() >= 32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_key() {
        let gen = Generator::new("drbot");
        let result = gen.generate("test key", "user123").unwrap();

        assert!(result.key.starts_with("drbot_"));
        assert_eq!(result.api_key.name, "test key");
        assert_eq!(result.api_key.owner_id, "user123");
    }

    #[test]
    fn test_validate_key() {
        let gen = Generator::new("drbot");
        let result = gen.generate("test", "user").unwrap();

        let mut store = KeyStore::new();
        store.store(result.api_key);

        assert!(store.validate(&result.key).is_ok());
        assert!(store.validate("invalid_key").is_err());
    }

    #[test]
    fn test_permissions() {
        let gen = Generator::new("drbot");
        let result = KeyBuilder::new("test", "user")
            .permissions(&["read", "write"])
            .build(&gen)
            .unwrap();

        assert!(result.api_key.has_permission("read"));
        assert!(result.api_key.has_permission("write"));
        assert!(!result.api_key.has_permission("admin"));
    }

    #[test]
    fn test_revoke_key() {
        let gen = Generator::new("drbot");
        let result = gen.generate("test", "user").unwrap();

        let mut store = KeyStore::new();
        store.store(result.api_key.clone());

        store.revoke(&result.api_key.id).unwrap();

        assert!(matches!(
            store.validate(&result.key),
            Err(ApiKeyError::Revoked)
        ));
    }

    #[test]
    fn test_key_expiration() {
        let gen = Generator::new("drbot");
        let result = KeyBuilder::new("test", "user")
            .expires_in(Duration::seconds(-10)) // Already expired
            .build(&gen)
            .unwrap();

        assert!(result.api_key.is_expired());
    }

    #[test]
    fn test_key_format_validation() {
        assert!(is_valid_format(
            "drbot_a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6"
        ));
        assert!(!is_valid_format("invalid"));
        assert!(!is_valid_format("_abc"));
    }
}
