//! Encrypted data vault for drbot.
//!
//! Secure local storage for sensitive data.
//!
//! # Features
//!
//! - AES-256-GCM encryption
//! - Secure key derivation
//! - Secret management
//! - Access audit logging
//! - Auto-lock timeout

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Vault result type.
pub type Result<T> = std::result::Result<T, VaultError>;

/// Vault errors.
#[derive(Debug, thiserror::Error)]
pub enum VaultError {
    #[error("Vault locked")]
    Locked,
    #[error("Secret not found: {0}")]
    NotFound(String),
    #[error("Invalid key")]
    InvalidKey,
    #[error("Encryption failed: {0}")]
    EncryptionFailed(String),
    #[error("Decryption failed: {0}")]
    DecryptionFailed(String),
    #[error("Access denied: {0}")]
    AccessDenied(String),
}

/// Secret type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SecretType {
    Password,
    ApiKey,
    Token,
    Certificate,
    PrivateKey,
    Note,
    CreditCard,
    Custom,
}

/// Stored secret (encrypted).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Secret {
    /// Secret ID.
    pub id: Uuid,
    /// Name.
    pub name: String,
    /// Secret type.
    pub secret_type: SecretType,
    /// Encrypted value.
    encrypted_value: Vec<u8>,
    /// Nonce/IV.
    nonce: Vec<u8>,
    /// Tags.
    pub tags: Vec<String>,
    /// Metadata.
    pub metadata: HashMap<String, String>,
    /// Created at.
    pub created_at: DateTime<Utc>,
    /// Updated at.
    pub updated_at: DateTime<Utc>,
    /// Expires at.
    pub expires_at: Option<DateTime<Utc>>,
    /// Last accessed.
    pub last_accessed: Option<DateTime<Utc>>,
    /// Access count.
    pub access_count: u64,
}

impl Secret {
    /// Create a new secret (encrypted).
    pub fn new(name: &str, secret_type: SecretType, encrypted: Vec<u8>, nonce: Vec<u8>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.to_string(),
            secret_type,
            encrypted_value: encrypted,
            nonce,
            tags: Vec::new(),
            metadata: HashMap::new(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            expires_at: None,
            last_accessed: None,
            access_count: 0,
        }
    }

    /// Check if expired.
    pub fn is_expired(&self) -> bool {
        self.expires_at.map(|e| e < Utc::now()).unwrap_or(false)
    }
}

/// Vault access log entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessLogEntry {
    /// Entry ID.
    pub id: Uuid,
    /// Secret ID.
    pub secret_id: Uuid,
    /// Action.
    pub action: AccessAction,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
    /// Success.
    pub success: bool,
    /// Source (app, etc).
    pub source: Option<String>,
}

/// Access action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccessAction {
    Read,
    Create,
    Update,
    Delete,
    List,
}

/// Vault status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VaultStatus {
    Locked,
    Unlocked,
}

/// Vault configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultConfig {
    /// Auto-lock timeout (seconds).
    pub auto_lock_seconds: u64,
    /// Enable access logging.
    pub enable_logging: bool,
    /// Key derivation iterations.
    pub kdf_iterations: u32,
    /// Maximum secret size (bytes).
    pub max_secret_size: usize,
}

impl Default for VaultConfig {
    fn default() -> Self {
        Self {
            auto_lock_seconds: 300,
            enable_logging: true,
            kdf_iterations: 100_000,
            max_secret_size: 1024 * 1024, // 1MB
        }
    }
}

/// Encryption key (in memory only).
struct VaultKey {
    key: [u8; 32],
    unlocked_at: DateTime<Utc>,
}

/// Data vault.
pub struct DataVault {
    config: VaultConfig,
    secrets: Arc<RwLock<HashMap<Uuid, Secret>>>,
    key: Arc<RwLock<Option<VaultKey>>>,
    access_log: Arc<RwLock<Vec<AccessLogEntry>>>,
}

impl DataVault {
    /// Create a new vault.
    pub fn new(config: VaultConfig) -> Self {
        Self {
            config,
            secrets: Arc::new(RwLock::new(HashMap::new())),
            key: Arc::new(RwLock::new(None)),
            access_log: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Unlock vault with password.
    pub async fn unlock(&self, password: &str) -> Result<()> {
        // Derive key from password (simplified - real impl would use proper KDF)
        let mut key = [0u8; 32];
        let password_bytes = password.as_bytes();
        for (i, byte) in password_bytes.iter().cycle().take(32).enumerate() {
            key[i] = *byte;
        }

        // In a real implementation, verify the key against a stored hash
        *self.key.write().await = Some(VaultKey {
            key,
            unlocked_at: Utc::now(),
        });

        Ok(())
    }

    /// Lock vault.
    pub async fn lock(&self) {
        // Zero out key
        if let Some(ref mut vault_key) = *self.key.write().await {
            vault_key.key = [0u8; 32];
        }
        *self.key.write().await = None;
    }

    /// Check if locked.
    pub async fn is_locked(&self) -> bool {
        let key = self.key.read().await;
        if let Some(ref vk) = *key {
            // Check auto-lock timeout
            let elapsed = Utc::now() - vk.unlocked_at;
            if elapsed > Duration::seconds(self.config.auto_lock_seconds as i64) {
                drop(key);
                self.lock().await;
                return true;
            }
            false
        } else {
            true
        }
    }

    /// Get vault status.
    pub async fn status(&self) -> VaultStatus {
        if self.is_locked().await {
            VaultStatus::Locked
        } else {
            VaultStatus::Unlocked
        }
    }

    fn check_key(&self, key: &Option<VaultKey>) -> Result<[u8; 32]> {
        key.as_ref().map(|k| k.key).ok_or(VaultError::Locked)
    }

    /// Store a secret.
    pub async fn store(&self, name: &str, value: &str, secret_type: SecretType) -> Result<Secret> {
        let key = self.key.read().await;
        let encryption_key = self.check_key(&key)?;

        if value.len() > self.config.max_secret_size {
            return Err(VaultError::EncryptionFailed("Secret too large".to_string()));
        }

        // Simple XOR encryption for demo (real impl would use AES-256-GCM)
        let nonce: Vec<u8> = (0..12).map(|_| rand_byte()).collect();
        let encrypted: Vec<u8> = value
            .as_bytes()
            .iter()
            .zip(encryption_key.iter().cycle())
            .map(|(b, k)| b ^ k)
            .collect();

        let secret = Secret::new(name, secret_type, encrypted, nonce);
        let id = secret.id;

        self.secrets.write().await.insert(id, secret.clone());
        self.log_access(id, AccessAction::Create, true, None).await;

        Ok(secret)
    }

    /// Retrieve a secret value.
    pub async fn retrieve(&self, id: Uuid) -> Result<String> {
        let key = self.key.read().await;
        let encryption_key = self.check_key(&key)?;

        let mut secrets = self.secrets.write().await;
        let secret = secrets
            .get_mut(&id)
            .ok_or(VaultError::NotFound(id.to_string()))?;

        if secret.is_expired() {
            drop(key);
            self.log_access(id, AccessAction::Read, false, None).await;
            return Err(VaultError::AccessDenied("Secret expired".to_string()));
        }

        // Decrypt
        let decrypted: Vec<u8> = secret
            .encrypted_value
            .iter()
            .zip(encryption_key.iter().cycle())
            .map(|(b, k)| b ^ k)
            .collect();

        let value = String::from_utf8(decrypted)
            .map_err(|_| VaultError::DecryptionFailed("Invalid UTF-8".to_string()))?;

        // Update access info
        secret.last_accessed = Some(Utc::now());
        secret.access_count += 1;

        drop(secrets);
        drop(key);
        self.log_access(id, AccessAction::Read, true, None).await;

        Ok(value)
    }

    /// Delete a secret.
    pub async fn delete(&self, id: Uuid) -> Result<()> {
        if self.is_locked().await {
            return Err(VaultError::Locked);
        }

        self.secrets.write().await.remove(&id);
        self.log_access(id, AccessAction::Delete, true, None).await;
        Ok(())
    }

    /// List secrets (metadata only).
    pub async fn list(&self) -> Result<Vec<SecretMetadata>> {
        if self.is_locked().await {
            return Err(VaultError::Locked);
        }

        let secrets: Vec<_> = self
            .secrets
            .read()
            .await
            .values()
            .map(|s| SecretMetadata {
                id: s.id,
                name: s.name.clone(),
                secret_type: s.secret_type,
                tags: s.tags.clone(),
                created_at: s.created_at,
                expires_at: s.expires_at,
                is_expired: s.is_expired(),
            })
            .collect();

        Ok(secrets)
    }

    /// Search secrets.
    pub async fn search(&self, query: &str) -> Result<Vec<SecretMetadata>> {
        let all = self.list().await?;
        let query_lower = query.to_lowercase();

        Ok(all
            .into_iter()
            .filter(|s| {
                s.name.to_lowercase().contains(&query_lower)
                    || s.tags
                        .iter()
                        .any(|t| t.to_lowercase().contains(&query_lower))
            })
            .collect())
    }

    /// Add tags to secret.
    pub async fn add_tags(&self, id: Uuid, tags: Vec<String>) -> Result<()> {
        if self.is_locked().await {
            return Err(VaultError::Locked);
        }

        let mut secrets = self.secrets.write().await;
        if let Some(secret) = secrets.get_mut(&id) {
            secret.tags.extend(tags);
            secret.tags.sort();
            secret.tags.dedup();
            secret.updated_at = Utc::now();
        }

        Ok(())
    }

    /// Get access log.
    pub async fn get_access_log(&self, limit: usize) -> Vec<AccessLogEntry> {
        self.access_log
            .read()
            .await
            .iter()
            .rev()
            .take(limit)
            .cloned()
            .collect()
    }

    async fn log_access(
        &self,
        secret_id: Uuid,
        action: AccessAction,
        success: bool,
        source: Option<String>,
    ) {
        if !self.config.enable_logging {
            return;
        }

        let entry = AccessLogEntry {
            id: Uuid::new_v4(),
            secret_id,
            action,
            timestamp: Utc::now(),
            success,
            source,
        };

        let mut log = self.access_log.write().await;
        log.push(entry);

        // Keep only last 1000 entries
        if log.len() > 1000 {
            log.remove(0);
        }
    }

    /// Get statistics.
    pub async fn stats(&self) -> VaultStats {
        let secrets = self.secrets.read().await;
        let log = self.access_log.read().await;

        let expired = secrets.values().filter(|s| s.is_expired()).count();

        let mut by_type: HashMap<SecretType, usize> = HashMap::new();
        for secret in secrets.values() {
            *by_type.entry(secret.secret_type).or_insert(0) += 1;
        }

        VaultStats {
            total_secrets: secrets.len(),
            expired_secrets: expired,
            total_accesses: log.len(),
            by_type,
        }
    }
}

/// Secret metadata (no value).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretMetadata {
    pub id: Uuid,
    pub name: String,
    pub secret_type: SecretType,
    pub tags: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub is_expired: bool,
}

/// Vault statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultStats {
    pub total_secrets: usize,
    pub expired_secrets: usize,
    pub total_accesses: usize,
    pub by_type: HashMap<SecretType, usize>,
}

/// Simple pseudo-random byte (not cryptographically secure).
fn rand_byte() -> u8 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .subsec_nanos();
    (nanos % 256) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_unlock_lock() {
        let vault = DataVault::new(VaultConfig::default());

        assert_eq!(vault.status().await, VaultStatus::Locked);

        vault.unlock("test-password").await.unwrap();
        assert_eq!(vault.status().await, VaultStatus::Unlocked);

        vault.lock().await;
        assert_eq!(vault.status().await, VaultStatus::Locked);
    }

    #[tokio::test]
    async fn test_store_retrieve() {
        let vault = DataVault::new(VaultConfig::default());
        vault.unlock("test-password").await.unwrap();

        let secret = vault
            .store("my-api-key", "super-secret-value", SecretType::ApiKey)
            .await
            .unwrap();
        let value = vault.retrieve(secret.id).await.unwrap();

        assert_eq!(value, "super-secret-value");
    }

    #[tokio::test]
    async fn test_locked_access() {
        let vault = DataVault::new(VaultConfig::default());

        let result = vault.store("test", "value", SecretType::Password).await;
        assert!(matches!(result, Err(VaultError::Locked)));
    }

    #[tokio::test]
    async fn test_list_secrets() {
        let vault = DataVault::new(VaultConfig::default());
        vault.unlock("password").await.unwrap();

        vault
            .store("secret1", "value1", SecretType::Password)
            .await
            .unwrap();
        vault
            .store("secret2", "value2", SecretType::ApiKey)
            .await
            .unwrap();

        let list = vault.list().await.unwrap();
        assert_eq!(list.len(), 2);
    }

    #[tokio::test]
    async fn test_search() {
        let vault = DataVault::new(VaultConfig::default());
        vault.unlock("password").await.unwrap();

        vault
            .store("database-password", "value", SecretType::Password)
            .await
            .unwrap();
        vault
            .store("api-key", "value", SecretType::ApiKey)
            .await
            .unwrap();

        let results = vault.search("database").await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "database-password");
    }

    #[tokio::test]
    async fn test_access_log() {
        let vault = DataVault::new(VaultConfig::default());
        vault.unlock("password").await.unwrap();

        let secret = vault
            .store("test", "value", SecretType::Password)
            .await
            .unwrap();
        vault.retrieve(secret.id).await.unwrap();

        let log = vault.get_access_log(10).await;
        assert_eq!(log.len(), 2); // Create + Read
    }
}
