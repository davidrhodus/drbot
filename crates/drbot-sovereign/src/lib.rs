//! Data sovereignty with encrypted context and privacy controls.
//!
//! This crate provides data sovereignty capabilities:
//! - User controls their data completely
//! - Encrypted context storage
//! - Granular privacy settings
//! - Data retention policies

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Sovereignty errors.
#[derive(Debug, Error)]
pub enum SovereignError {
    #[error("Access denied: {0}")]
    AccessDenied(String),

    #[error("Encryption failed: {0}")]
    EncryptionFailed(String),

    #[error("Decryption failed: {0}")]
    DecryptionFailed(String),

    #[error("Policy violation: {0}")]
    PolicyViolation(String),

    #[error("Data not found: {0}")]
    DataNotFound(String),

    #[error("Provider error: {0}")]
    ProviderError(String),
}

/// Result type for sovereignty operations.
pub type Result<T> = std::result::Result<T, SovereignError>;

/// A user's data sovereignty profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SovereignProfile {
    /// Profile identifier.
    pub id: String,
    /// User identifier.
    pub user_id: String,
    /// Privacy settings.
    pub privacy: PrivacySettings,
    /// Data retention policy.
    pub retention: RetentionPolicy,
    /// Access permissions.
    pub permissions: Vec<Permission>,
    /// Encryption settings.
    pub encryption: EncryptionSettings,
    /// Created timestamp.
    pub created_at: DateTime<Utc>,
    /// Last updated.
    pub updated_at: DateTime<Utc>,
}

/// Privacy settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrivacySettings {
    /// Allow conversation history.
    pub allow_history: bool,
    /// Allow learning from interactions.
    pub allow_learning: bool,
    /// Allow sharing with third parties.
    pub allow_sharing: bool,
    /// Data categories to protect.
    pub protected_categories: Vec<DataCategory>,
    /// PII detection sensitivity.
    pub pii_sensitivity: Sensitivity,
    /// Anonymize data.
    pub anonymize: bool,
}

impl Default for PrivacySettings {
    fn default() -> Self {
        Self {
            allow_history: true,
            allow_learning: false,
            allow_sharing: false,
            protected_categories: vec![
                DataCategory::Personal,
                DataCategory::Financial,
                DataCategory::Health,
            ],
            pii_sensitivity: Sensitivity::High,
            anonymize: true,
        }
    }
}

/// Data categories.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum DataCategory {
    Personal,
    Financial,
    Health,
    Location,
    Communication,
    Professional,
    Behavioral,
    Custom(String),
}

/// Sensitivity levels.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum Sensitivity {
    Low,
    Medium,
    High,
    Maximum,
}

/// Data retention policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetentionPolicy {
    /// Default retention period in days.
    pub default_days: Option<u32>,
    /// Category-specific retention.
    pub category_retention: HashMap<DataCategory, u32>,
    /// Auto-delete enabled.
    pub auto_delete: bool,
    /// Archive before delete.
    pub archive_before_delete: bool,
}

impl Default for RetentionPolicy {
    fn default() -> Self {
        Self {
            default_days: Some(30),
            category_retention: HashMap::new(),
            auto_delete: true,
            archive_before_delete: true,
        }
    }
}

/// A permission grant.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Permission {
    /// Permission identifier.
    pub id: String,
    /// What is being permitted.
    pub scope: PermissionScope,
    /// Who is granted permission.
    pub grantee: String,
    /// Permission level.
    pub level: PermissionLevel,
    /// Expiry.
    pub expires_at: Option<DateTime<Utc>>,
    /// Created timestamp.
    pub created_at: DateTime<Utc>,
}

/// Permission scope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PermissionScope {
    /// All data.
    All,
    /// Specific category.
    Category(DataCategory),
    /// Specific data item.
    Item(String),
    /// Time range.
    TimeRange {
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    },
}

/// Permission level.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum PermissionLevel {
    None,
    Read,
    Write,
    Delete,
    Admin,
}

/// Encryption settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptionSettings {
    /// Encryption enabled.
    pub enabled: bool,
    /// Encryption algorithm.
    pub algorithm: EncryptionAlgorithm,
    /// Key derivation.
    pub key_derivation: KeyDerivation,
    /// Encrypt at rest.
    pub encrypt_at_rest: bool,
    /// Encrypt in transit.
    pub encrypt_in_transit: bool,
}

impl Default for EncryptionSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            algorithm: EncryptionAlgorithm::Aes256Gcm,
            key_derivation: KeyDerivation::Argon2id,
            encrypt_at_rest: true,
            encrypt_in_transit: true,
        }
    }
}

/// Encryption algorithms.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum EncryptionAlgorithm {
    Aes256Gcm,
    ChaCha20Poly1305,
    Aes256Cbc,
}

/// Key derivation functions.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum KeyDerivation {
    Argon2id,
    Scrypt,
    Pbkdf2,
}

/// Protected data item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtectedData {
    /// Data identifier.
    pub id: String,
    /// Owner user ID.
    pub owner_id: String,
    /// Data category.
    pub category: DataCategory,
    /// Encrypted content (base64).
    pub encrypted_content: String,
    /// Encryption metadata.
    pub encryption_metadata: EncryptionMetadata,
    /// Created timestamp.
    pub created_at: DateTime<Utc>,
    /// Expires at.
    pub expires_at: Option<DateTime<Utc>>,
    /// Access log.
    pub access_log: Vec<AccessLogEntry>,
}

/// Encryption metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptionMetadata {
    /// Algorithm used.
    pub algorithm: EncryptionAlgorithm,
    /// IV/nonce (base64).
    pub nonce: String,
    /// Key ID.
    pub key_id: String,
    /// Encrypted at.
    pub encrypted_at: DateTime<Utc>,
}

/// Access log entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessLogEntry {
    /// Who accessed.
    pub accessor: String,
    /// Access type.
    pub access_type: AccessType,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
    /// Success.
    pub success: bool,
    /// Reason if denied.
    pub denial_reason: Option<String>,
}

/// Types of access.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum AccessType {
    Read,
    Write,
    Delete,
    Export,
    Share,
}

/// Data export request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportRequest {
    /// Request ID.
    pub id: String,
    /// User requesting.
    pub user_id: String,
    /// Categories to export.
    pub categories: Vec<DataCategory>,
    /// Time range.
    pub time_range: Option<(DateTime<Utc>, DateTime<Utc>)>,
    /// Format.
    pub format: ExportFormat,
    /// Include encrypted.
    pub include_encrypted: bool,
}

/// Export formats.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ExportFormat {
    Json,
    Csv,
    Encrypted,
}

/// Provider for sovereignty operations.
#[async_trait]
pub trait SovereignProvider: Send + Sync {
    /// Encrypt data.
    async fn encrypt(
        &self,
        data: &[u8],
        settings: &EncryptionSettings,
    ) -> Result<(Vec<u8>, EncryptionMetadata)>;

    /// Decrypt data.
    async fn decrypt(&self, encrypted: &[u8], metadata: &EncryptionMetadata) -> Result<Vec<u8>>;

    /// Detect PII in content.
    async fn detect_pii(&self, content: &str) -> Result<Vec<PiiDetection>>;

    /// Anonymize content.
    async fn anonymize(&self, content: &str, detections: &[PiiDetection]) -> Result<String>;
}

/// PII detection result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiiDetection {
    /// Type of PII.
    pub pii_type: PiiType,
    /// Start offset.
    pub start: usize,
    /// End offset.
    pub end: usize,
    /// Original text.
    pub text: String,
    /// Confidence.
    pub confidence: f64,
}

/// Types of PII.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PiiType {
    Name,
    Email,
    Phone,
    Address,
    Ssn,
    CreditCard,
    DateOfBirth,
    IpAddress,
    Custom(String),
}

/// The sovereignty controller.
pub struct SovereignController {
    /// Provider for operations.
    provider: Arc<dyn SovereignProvider>,
    /// User profiles.
    profiles: Arc<RwLock<HashMap<String, SovereignProfile>>>,
    /// Protected data storage.
    data: Arc<RwLock<HashMap<String, ProtectedData>>>,
}

impl SovereignController {
    /// Create a new sovereignty controller.
    pub fn new(provider: Arc<dyn SovereignProvider>) -> Self {
        Self {
            provider,
            profiles: Arc::new(RwLock::new(HashMap::new())),
            data: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create or get a user profile.
    pub async fn get_or_create_profile(&self, user_id: &str) -> SovereignProfile {
        let mut profiles = self.profiles.write().await;

        profiles
            .entry(user_id.to_string())
            .or_insert_with(|| SovereignProfile {
                id: Uuid::new_v4().to_string(),
                user_id: user_id.to_string(),
                privacy: PrivacySettings::default(),
                retention: RetentionPolicy::default(),
                permissions: Vec::new(),
                encryption: EncryptionSettings::default(),
                created_at: Utc::now(),
                updated_at: Utc::now(),
            })
            .clone()
    }

    /// Update privacy settings.
    pub async fn update_privacy(&self, user_id: &str, privacy: PrivacySettings) -> Result<()> {
        let mut profiles = self.profiles.write().await;

        if let Some(profile) = profiles.get_mut(user_id) {
            profile.privacy = privacy;
            profile.updated_at = Utc::now();
            Ok(())
        } else {
            Err(SovereignError::DataNotFound(user_id.to_string()))
        }
    }

    /// Store protected data.
    pub async fn store(
        &self,
        user_id: &str,
        content: &str,
        category: DataCategory,
    ) -> Result<String> {
        let profile = self.get_or_create_profile(user_id).await;

        // Check if we should anonymize
        let content = if profile.privacy.anonymize {
            let detections = self.provider.detect_pii(content).await?;
            self.provider.anonymize(content, &detections).await?
        } else {
            content.to_string()
        };

        // Encrypt if enabled
        let (encrypted, metadata) = if profile.encryption.enabled {
            self.provider
                .encrypt(content.as_bytes(), &profile.encryption)
                .await?
        } else {
            (
                content.as_bytes().to_vec(),
                EncryptionMetadata {
                    algorithm: EncryptionAlgorithm::Aes256Gcm,
                    nonce: String::new(),
                    key_id: String::new(),
                    encrypted_at: Utc::now(),
                },
            )
        };

        // Calculate expiry
        let expires_at = profile
            .retention
            .category_retention
            .get(&category)
            .or(profile.retention.default_days.as_ref())
            .map(|days| Utc::now() + chrono::Duration::days(*days as i64));

        let data = ProtectedData {
            id: Uuid::new_v4().to_string(),
            owner_id: user_id.to_string(),
            category,
            encrypted_content: base64_encode(&encrypted),
            encryption_metadata: metadata,
            created_at: Utc::now(),
            expires_at,
            access_log: vec![AccessLogEntry {
                accessor: user_id.to_string(),
                access_type: AccessType::Write,
                timestamp: Utc::now(),
                success: true,
                denial_reason: None,
            }],
        };

        let id = data.id.clone();
        let mut storage = self.data.write().await;
        storage.insert(id.clone(), data);

        Ok(id)
    }

    /// Retrieve protected data.
    pub async fn retrieve(&self, data_id: &str, accessor_id: &str) -> Result<String> {
        let mut storage = self.data.write().await;
        let data = storage
            .get_mut(data_id)
            .ok_or_else(|| SovereignError::DataNotFound(data_id.to_string()))?;

        // Check access permission
        if data.owner_id != accessor_id {
            let profiles = self.profiles.read().await;
            let profile = profiles
                .get(&data.owner_id)
                .ok_or_else(|| SovereignError::AccessDenied("No profile".to_string()))?;

            let has_permission = profile.permissions.iter().any(|p| {
                p.grantee == accessor_id
                    && p.level >= PermissionLevel::Read
                    && p.expires_at.map_or(true, |exp| exp > Utc::now())
            });

            if !has_permission {
                data.access_log.push(AccessLogEntry {
                    accessor: accessor_id.to_string(),
                    access_type: AccessType::Read,
                    timestamp: Utc::now(),
                    success: false,
                    denial_reason: Some("No permission".to_string()),
                });
                return Err(SovereignError::AccessDenied("No permission".to_string()));
            }
        }

        // Decrypt
        let encrypted = base64_decode(&data.encrypted_content)?;
        let decrypted = self
            .provider
            .decrypt(&encrypted, &data.encryption_metadata)
            .await?;

        // Log access
        data.access_log.push(AccessLogEntry {
            accessor: accessor_id.to_string(),
            access_type: AccessType::Read,
            timestamp: Utc::now(),
            success: true,
            denial_reason: None,
        });

        String::from_utf8(decrypted).map_err(|e| SovereignError::DecryptionFailed(e.to_string()))
    }

    /// Delete data.
    pub async fn delete(&self, data_id: &str, user_id: &str) -> Result<()> {
        let mut storage = self.data.write().await;

        if let Some(data) = storage.get(data_id) {
            if data.owner_id != user_id {
                return Err(SovereignError::AccessDenied("Not owner".to_string()));
            }
        }

        storage.remove(data_id);
        Ok(())
    }

    /// Export user data.
    pub async fn export(&self, request: ExportRequest) -> Result<String> {
        let storage = self.data.read().await;

        let user_data: Vec<_> = storage
            .values()
            .filter(|d| d.owner_id == request.user_id)
            .filter(|d| request.categories.is_empty() || request.categories.contains(&d.category))
            .cloned()
            .collect();

        match request.format {
            ExportFormat::Json => serde_json::to_string(&user_data)
                .map_err(|e| SovereignError::ProviderError(e.to_string())),
            _ => serde_json::to_string(&user_data)
                .map_err(|e| SovereignError::ProviderError(e.to_string())),
        }
    }

    /// Grant permission.
    pub async fn grant_permission(&self, user_id: &str, permission: Permission) -> Result<()> {
        let mut profiles = self.profiles.write().await;

        if let Some(profile) = profiles.get_mut(user_id) {
            profile.permissions.push(permission);
            profile.updated_at = Utc::now();
            Ok(())
        } else {
            Err(SovereignError::DataNotFound(user_id.to_string()))
        }
    }

    /// Revoke permission.
    pub async fn revoke_permission(&self, user_id: &str, permission_id: &str) -> Result<()> {
        let mut profiles = self.profiles.write().await;

        if let Some(profile) = profiles.get_mut(user_id) {
            profile.permissions.retain(|p| p.id != permission_id);
            profile.updated_at = Utc::now();
            Ok(())
        } else {
            Err(SovereignError::DataNotFound(user_id.to_string()))
        }
    }

    /// Run retention cleanup.
    pub async fn cleanup_expired(&self) -> usize {
        let mut storage = self.data.write().await;
        let now = Utc::now();
        let before = storage.len();

        storage.retain(|_, data| data.expires_at.map_or(true, |exp| exp > now));

        before - storage.len()
    }
}

fn base64_encode(data: &[u8]) -> String {
    // Simple placeholder - would use proper base64 crate
    data.iter().map(|b| format!("{:02x}", b)).collect()
}

fn base64_decode(s: &str) -> Result<Vec<u8>> {
    // Simple placeholder
    let bytes: Result<Vec<u8>> = (0..s.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&s[i..i + 2], 16)
                .map_err(|e| SovereignError::DecryptionFailed(e.to_string()))
        })
        .collect();
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockProvider;

    #[async_trait]
    impl SovereignProvider for MockProvider {
        async fn encrypt(
            &self,
            data: &[u8],
            _settings: &EncryptionSettings,
        ) -> Result<(Vec<u8>, EncryptionMetadata)> {
            Ok((
                data.to_vec(),
                EncryptionMetadata {
                    algorithm: EncryptionAlgorithm::Aes256Gcm,
                    nonce: "nonce".to_string(),
                    key_id: "key1".to_string(),
                    encrypted_at: Utc::now(),
                },
            ))
        }

        async fn decrypt(
            &self,
            encrypted: &[u8],
            _metadata: &EncryptionMetadata,
        ) -> Result<Vec<u8>> {
            Ok(encrypted.to_vec())
        }

        async fn detect_pii(&self, _content: &str) -> Result<Vec<PiiDetection>> {
            Ok(vec![])
        }

        async fn anonymize(&self, content: &str, _detections: &[PiiDetection]) -> Result<String> {
            Ok(content.to_string())
        }
    }

    #[tokio::test]
    async fn test_create_profile() {
        let provider = Arc::new(MockProvider);
        let controller = SovereignController::new(provider);

        let profile = controller.get_or_create_profile("user1").await;
        assert_eq!(profile.user_id, "user1");
        assert!(profile.privacy.allow_history);
    }

    #[tokio::test]
    async fn test_store_and_retrieve() {
        let provider = Arc::new(MockProvider);
        let controller = SovereignController::new(provider);

        controller.get_or_create_profile("user1").await;

        let id = controller
            .store("user1", "secret data", DataCategory::Personal)
            .await
            .unwrap();
        let retrieved = controller.retrieve(&id, "user1").await.unwrap();

        assert_eq!(retrieved, "secret data");
    }

    #[tokio::test]
    async fn test_access_denied() {
        let provider = Arc::new(MockProvider);
        let controller = SovereignController::new(provider);

        controller.get_or_create_profile("user1").await;
        controller.get_or_create_profile("user2").await;

        let id = controller
            .store("user1", "secret", DataCategory::Personal)
            .await
            .unwrap();

        // user2 should not have access
        let result = controller.retrieve(&id, "user2").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_grant_permission() {
        let provider = Arc::new(MockProvider);
        let controller = SovereignController::new(provider);

        controller.get_or_create_profile("user1").await;

        let permission = Permission {
            id: "p1".to_string(),
            scope: PermissionScope::All,
            grantee: "user2".to_string(),
            level: PermissionLevel::Read,
            expires_at: None,
            created_at: Utc::now(),
        };

        controller
            .grant_permission("user1", permission)
            .await
            .unwrap();
    }

    #[test]
    fn test_permission_levels() {
        assert!(PermissionLevel::Admin > PermissionLevel::Write);
        assert!(PermissionLevel::Write > PermissionLevel::Read);
    }
}
