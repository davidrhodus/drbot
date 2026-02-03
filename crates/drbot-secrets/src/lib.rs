//! Secrets management for drbot.
//!
//! This crate provides:
//! - Secure secret storage
//! - Secret rotation
//! - Access control
//! - Audit logging

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Secrets error types.
#[derive(Error, Debug)]
pub enum SecretsError {
    #[error("Secret not found: {0}")]
    NotFound(String),

    #[error("Access denied")]
    AccessDenied,

    #[error("Secret expired")]
    Expired,

    #[error("Invalid secret: {0}")]
    Invalid(String),

    #[error("Storage error: {0}")]
    StorageError(String),
}

/// Result type for secrets operations.
pub type Result<T> = std::result::Result<T, SecretsError>;

/// Secret metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretMetadata {
    /// Secret ID.
    pub id: String,
    /// Secret name.
    pub name: String,
    /// Description.
    pub description: Option<String>,
    /// Created at.
    pub created_at: DateTime<Utc>,
    /// Updated at.
    pub updated_at: DateTime<Utc>,
    /// Expires at.
    pub expires_at: Option<DateTime<Utc>>,
    /// Version.
    pub version: u32,
    /// Tags.
    pub tags: Vec<String>,
    /// Owner.
    pub owner: Option<String>,
    /// Rotation policy.
    pub rotation_days: Option<u32>,
    /// Last rotated.
    pub last_rotated_at: Option<DateTime<Utc>>,
}

impl SecretMetadata {
    /// Check if expired.
    pub fn is_expired(&self) -> bool {
        self.expires_at.map(|t| Utc::now() > t).unwrap_or(false)
    }

    /// Check if rotation is needed.
    pub fn needs_rotation(&self) -> bool {
        if let (Some(rotation_days), Some(last_rotated)) =
            (self.rotation_days, self.last_rotated_at)
        {
            let next_rotation = last_rotated + Duration::days(rotation_days as i64);
            Utc::now() > next_rotation
        } else if let Some(rotation_days) = self.rotation_days {
            let next_rotation = self.created_at + Duration::days(rotation_days as i64);
            Utc::now() > next_rotation
        } else {
            false
        }
    }
}

/// A secret value.
#[derive(Debug, Clone)]
pub struct Secret {
    /// Metadata.
    pub metadata: SecretMetadata,
    /// The secret value (encrypted in real implementation).
    value: String,
}

impl Secret {
    /// Create a new secret.
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        let now = Utc::now();
        let name = name.into();
        Self {
            metadata: SecretMetadata {
                id: Uuid::new_v4().to_string(),
                name: name.clone(),
                description: None,
                created_at: now,
                updated_at: now,
                expires_at: None,
                version: 1,
                tags: Vec::new(),
                owner: None,
                rotation_days: None,
                last_rotated_at: None,
            },
            value: value.into(),
        }
    }

    /// Set description.
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.metadata.description = Some(description.into());
        self
    }

    /// Set expiration.
    pub fn expires_in(mut self, duration: Duration) -> Self {
        self.metadata.expires_at = Some(Utc::now() + duration);
        self
    }

    /// Set rotation policy.
    pub fn with_rotation(mut self, days: u32) -> Self {
        self.metadata.rotation_days = Some(days);
        self
    }

    /// Add tag.
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.metadata.tags.push(tag.into());
        self
    }

    /// Set owner.
    pub fn with_owner(mut self, owner: impl Into<String>) -> Self {
        self.metadata.owner = Some(owner.into());
        self
    }

    /// Get the value.
    pub fn value(&self) -> &str {
        &self.value
    }

    /// Update the value.
    pub fn update_value(&mut self, value: impl Into<String>) {
        self.value = value.into();
        self.metadata.version += 1;
        self.metadata.updated_at = Utc::now();
    }

    /// Rotate the secret.
    pub fn rotate(&mut self, new_value: impl Into<String>) {
        self.value = new_value.into();
        self.metadata.version += 1;
        self.metadata.updated_at = Utc::now();
        self.metadata.last_rotated_at = Some(Utc::now());
    }
}

/// Access record for auditing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessRecord {
    /// Record ID.
    pub id: Uuid,
    /// Secret name.
    pub secret_name: String,
    /// Accessor identity.
    pub accessor: String,
    /// Action type.
    pub action: AccessAction,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
    /// Success.
    pub success: bool,
    /// Reason for denial (if applicable).
    pub denial_reason: Option<String>,
}

/// Access action type.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum AccessAction {
    Read,
    Write,
    Delete,
    Rotate,
    List,
}

/// Secrets storage trait.
#[async_trait]
pub trait SecretsStorage: Send + Sync {
    /// Store a secret.
    async fn store(&self, secret: Secret) -> Result<()>;

    /// Get a secret.
    async fn get(&self, name: &str) -> Result<Option<Secret>>;

    /// Delete a secret.
    async fn delete(&self, name: &str) -> Result<()>;

    /// List secret names.
    async fn list(&self) -> Result<Vec<SecretMetadata>>;

    /// List secrets by tag.
    async fn list_by_tag(&self, tag: &str) -> Result<Vec<SecretMetadata>>;
}

/// In-memory secrets storage.
pub struct InMemorySecretsStorage {
    secrets: RwLock<HashMap<String, Secret>>,
}

impl InMemorySecretsStorage {
    /// Create new storage.
    pub fn new() -> Self {
        Self {
            secrets: RwLock::new(HashMap::new()),
        }
    }
}

impl Default for InMemorySecretsStorage {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SecretsStorage for InMemorySecretsStorage {
    async fn store(&self, secret: Secret) -> Result<()> {
        let mut secrets = self.secrets.write().await;
        secrets.insert(secret.metadata.name.clone(), secret);
        Ok(())
    }

    async fn get(&self, name: &str) -> Result<Option<Secret>> {
        let secrets = self.secrets.read().await;
        Ok(secrets.get(name).cloned())
    }

    async fn delete(&self, name: &str) -> Result<()> {
        let mut secrets = self.secrets.write().await;
        secrets.remove(name);
        Ok(())
    }

    async fn list(&self) -> Result<Vec<SecretMetadata>> {
        let secrets = self.secrets.read().await;
        Ok(secrets.values().map(|s| s.metadata.clone()).collect())
    }

    async fn list_by_tag(&self, tag: &str) -> Result<Vec<SecretMetadata>> {
        let secrets = self.secrets.read().await;
        Ok(secrets
            .values()
            .filter(|s| s.metadata.tags.contains(&tag.to_string()))
            .map(|s| s.metadata.clone())
            .collect())
    }
}

/// Secrets manager.
pub struct SecretsManager<S: SecretsStorage> {
    storage: Arc<S>,
    access_log: RwLock<Vec<AccessRecord>>,
}

impl<S: SecretsStorage> SecretsManager<S> {
    /// Create new manager.
    pub fn new(storage: Arc<S>) -> Self {
        Self {
            storage,
            access_log: RwLock::new(Vec::new()),
        }
    }

    async fn log_access(
        &self,
        secret_name: &str,
        accessor: &str,
        action: AccessAction,
        success: bool,
        reason: Option<&str>,
    ) {
        let record = AccessRecord {
            id: Uuid::new_v4(),
            secret_name: secret_name.to_string(),
            accessor: accessor.to_string(),
            action,
            timestamp: Utc::now(),
            success,
            denial_reason: reason.map(|s| s.to_string()),
        };
        self.access_log.write().await.push(record);
    }

    /// Get a secret.
    pub async fn get(&self, name: &str, accessor: &str) -> Result<Secret> {
        let secret = self
            .storage
            .get(name)
            .await?
            .ok_or_else(|| SecretsError::NotFound(name.to_string()))?;

        if secret.metadata.is_expired() {
            self.log_access(name, accessor, AccessAction::Read, false, Some("Expired"))
                .await;
            return Err(SecretsError::Expired);
        }

        self.log_access(name, accessor, AccessAction::Read, true, None)
            .await;
        Ok(secret)
    }

    /// Get just the value.
    pub async fn get_value(&self, name: &str, accessor: &str) -> Result<String> {
        Ok(self.get(name, accessor).await?.value.clone())
    }

    /// Store a secret.
    pub async fn store(&self, secret: Secret, accessor: &str) -> Result<()> {
        let name = secret.metadata.name.clone();
        self.storage.store(secret).await?;
        self.log_access(&name, accessor, AccessAction::Write, true, None)
            .await;
        Ok(())
    }

    /// Delete a secret.
    pub async fn delete(&self, name: &str, accessor: &str) -> Result<()> {
        self.storage.delete(name).await?;
        self.log_access(name, accessor, AccessAction::Delete, true, None)
            .await;
        Ok(())
    }

    /// Rotate a secret.
    pub async fn rotate(&self, name: &str, new_value: &str, accessor: &str) -> Result<()> {
        let mut secret = self
            .storage
            .get(name)
            .await?
            .ok_or_else(|| SecretsError::NotFound(name.to_string()))?;

        secret.rotate(new_value);
        self.storage.store(secret).await?;
        self.log_access(name, accessor, AccessAction::Rotate, true, None)
            .await;
        Ok(())
    }

    /// List secrets.
    pub async fn list(&self, accessor: &str) -> Result<Vec<SecretMetadata>> {
        self.log_access("*", accessor, AccessAction::List, true, None)
            .await;
        self.storage.list().await
    }

    /// Get secrets needing rotation.
    pub async fn get_rotation_needed(&self) -> Result<Vec<SecretMetadata>> {
        let all = self.storage.list().await?;
        Ok(all.into_iter().filter(|m| m.needs_rotation()).collect())
    }

    /// Get expired secrets.
    pub async fn get_expired(&self) -> Result<Vec<SecretMetadata>> {
        let all = self.storage.list().await?;
        Ok(all.into_iter().filter(|m| m.is_expired()).collect())
    }

    /// Get access log.
    pub async fn get_access_log(&self) -> Vec<AccessRecord> {
        self.access_log.read().await.clone()
    }

    /// Get access log for a secret.
    pub async fn get_access_log_for(&self, name: &str) -> Vec<AccessRecord> {
        self.access_log
            .read()
            .await
            .iter()
            .filter(|r| r.secret_name == name)
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_secret_creation() {
        let secret = Secret::new("api_key", "secret-value")
            .with_description("API key for service")
            .with_tag("production");

        assert_eq!(secret.metadata.name, "api_key");
        assert_eq!(secret.value(), "secret-value");
        assert!(secret.metadata.tags.contains(&"production".to_string()));
    }

    #[test]
    fn test_secret_rotation() {
        let mut secret = Secret::new("api_key", "old-value").with_rotation(30);

        secret.rotate("new-value");

        assert_eq!(secret.value(), "new-value");
        assert_eq!(secret.metadata.version, 2);
        assert!(secret.metadata.last_rotated_at.is_some());
    }

    #[test]
    fn test_expiration() {
        let expired = Secret::new("test", "value").expires_in(Duration::days(-1)); // Already expired

        assert!(expired.metadata.is_expired());

        let valid = Secret::new("test", "value").expires_in(Duration::days(1));

        assert!(!valid.metadata.is_expired());
    }

    #[tokio::test]
    async fn test_storage() {
        let storage = InMemorySecretsStorage::new();

        let secret = Secret::new("test", "value");
        storage.store(secret).await.unwrap();

        let retrieved = storage.get("test").await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().value(), "value");
    }

    #[tokio::test]
    async fn test_manager_get() {
        let storage = Arc::new(InMemorySecretsStorage::new());
        let manager = SecretsManager::new(storage.clone());

        let secret = Secret::new("api_key", "secret123");
        manager.store(secret, "admin").await.unwrap();

        let retrieved = manager.get_value("api_key", "app").await.unwrap();
        assert_eq!(retrieved, "secret123");
    }

    #[tokio::test]
    async fn test_manager_not_found() {
        let storage = Arc::new(InMemorySecretsStorage::new());
        let manager = SecretsManager::new(storage);

        let result = manager.get("nonexistent", "app").await;
        assert!(matches!(result, Err(SecretsError::NotFound(_))));
    }

    #[tokio::test]
    async fn test_manager_expired() {
        let storage = Arc::new(InMemorySecretsStorage::new());
        let manager = SecretsManager::new(storage);

        let secret = Secret::new("expired", "value").expires_in(Duration::days(-1));
        manager.store(secret, "admin").await.unwrap();

        let result = manager.get("expired", "app").await;
        assert!(matches!(result, Err(SecretsError::Expired)));
    }

    #[tokio::test]
    async fn test_access_log() {
        let storage = Arc::new(InMemorySecretsStorage::new());
        let manager = SecretsManager::new(storage);

        let secret = Secret::new("test", "value");
        manager.store(secret, "admin").await.unwrap();
        manager.get("test", "user1").await.unwrap();
        manager.get("test", "user2").await.unwrap();

        let log = manager.get_access_log().await;
        assert_eq!(log.len(), 3); // store + 2 gets
    }

    #[tokio::test]
    async fn test_rotation_needed() {
        let storage = Arc::new(InMemorySecretsStorage::new());
        let manager = SecretsManager::new(storage);

        let old_secret = Secret::new("old", "value").with_rotation(0); // Rotation needed immediately

        manager.store(old_secret, "admin").await.unwrap();

        let needing_rotation = manager.get_rotation_needed().await.unwrap();
        assert_eq!(needing_rotation.len(), 1);
    }

    #[tokio::test]
    async fn test_list_by_tag() {
        let storage = InMemorySecretsStorage::new();

        storage
            .store(Secret::new("s1", "v").with_tag("prod"))
            .await
            .unwrap();
        storage
            .store(Secret::new("s2", "v").with_tag("prod"))
            .await
            .unwrap();
        storage
            .store(Secret::new("s3", "v").with_tag("dev"))
            .await
            .unwrap();

        let prod = storage.list_by_tag("prod").await.unwrap();
        assert_eq!(prod.len(), 2);
    }
}
