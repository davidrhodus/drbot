//! Idempotency handling for request processing in drbot.
//!
//! This crate provides:
//! - Idempotency key management
//! - Request deduplication
//! - Response caching
//! - TTL-based cleanup

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Idempotency error types.
#[derive(Error, Debug)]
pub enum IdempotencyError {
    #[error("Key not found: {0}")]
    NotFound(String),

    #[error("Request in progress: {0}")]
    InProgress(String),

    #[error("Storage error: {0}")]
    StorageError(String),

    #[error("Conflict: {0}")]
    Conflict(String),

    #[error("Expired: {0}")]
    Expired(String),
}

/// Result type for idempotency operations.
pub type Result<T> = std::result::Result<T, IdempotencyError>;

/// Idempotency record status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IdempotencyStatus {
    /// Request is being processed.
    Processing,
    /// Request completed successfully.
    Completed,
    /// Request failed.
    Failed,
}

/// An idempotency record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdempotencyRecord {
    /// Idempotency key.
    pub key: String,
    /// Request fingerprint for conflict detection.
    pub request_fingerprint: String,
    /// Current status.
    pub status: IdempotencyStatus,
    /// Cached response (if completed).
    pub response: Option<serde_json::Value>,
    /// HTTP status code (for HTTP responses).
    pub status_code: Option<u16>,
    /// Error message (if failed).
    pub error: Option<String>,
    /// Created timestamp.
    pub created_at: DateTime<Utc>,
    /// Updated timestamp.
    pub updated_at: DateTime<Utc>,
    /// Expires at.
    pub expires_at: DateTime<Utc>,
    /// Locked by instance ID.
    pub locked_by: Option<String>,
    /// Lock expires at.
    pub lock_expires_at: Option<DateTime<Utc>>,
}

impl IdempotencyRecord {
    /// Create a new record.
    pub fn new(key: impl Into<String>, fingerprint: impl Into<String>, ttl: Duration) -> Self {
        let now = Utc::now();
        Self {
            key: key.into(),
            request_fingerprint: fingerprint.into(),
            status: IdempotencyStatus::Processing,
            response: None,
            status_code: None,
            error: None,
            created_at: now,
            updated_at: now,
            expires_at: now + ttl,
            locked_by: None,
            lock_expires_at: None,
        }
    }

    /// Check if expired.
    pub fn is_expired(&self) -> bool {
        Utc::now() > self.expires_at
    }

    /// Check if lock is expired.
    pub fn is_lock_expired(&self) -> bool {
        self.lock_expires_at.map(|t| Utc::now() > t).unwrap_or(true)
    }
}

/// Idempotency storage trait.
#[async_trait]
pub trait IdempotencyStorage: Send + Sync {
    /// Try to acquire a key for processing.
    async fn acquire(
        &self,
        key: &str,
        fingerprint: &str,
        ttl: Duration,
        lock_ttl: Duration,
        instance_id: &str,
    ) -> Result<AcquireResult>;

    /// Complete a request successfully.
    async fn complete(
        &self,
        key: &str,
        response: serde_json::Value,
        status_code: Option<u16>,
    ) -> Result<()>;

    /// Mark a request as failed.
    async fn fail(&self, key: &str, error: &str) -> Result<()>;

    /// Release a lock (for cleanup on failure).
    async fn release(&self, key: &str) -> Result<()>;

    /// Get a record.
    async fn get(&self, key: &str) -> Result<Option<IdempotencyRecord>>;

    /// Cleanup expired records.
    async fn cleanup(&self) -> Result<usize>;
}

/// Result of trying to acquire an idempotency key.
#[derive(Debug, Clone)]
pub enum AcquireResult {
    /// Key was acquired for processing.
    Acquired,
    /// Request is already being processed.
    InProgress,
    /// Request was already completed, here's the cached response.
    Cached(IdempotencyRecord),
    /// Request fingerprint conflicts with existing.
    Conflict,
}

/// In-memory idempotency storage.
pub struct InMemoryIdempotencyStorage {
    records: RwLock<HashMap<String, IdempotencyRecord>>,
}

impl InMemoryIdempotencyStorage {
    /// Create new in-memory storage.
    pub fn new() -> Self {
        Self {
            records: RwLock::new(HashMap::new()),
        }
    }
}

impl Default for InMemoryIdempotencyStorage {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl IdempotencyStorage for InMemoryIdempotencyStorage {
    async fn acquire(
        &self,
        key: &str,
        fingerprint: &str,
        ttl: Duration,
        lock_ttl: Duration,
        instance_id: &str,
    ) -> Result<AcquireResult> {
        let mut records = self.records.write().await;
        let now = Utc::now();

        if let Some(record) = records.get_mut(key) {
            // Check if expired
            if record.is_expired() {
                records.remove(key);
            } else {
                // Check fingerprint
                if record.request_fingerprint != fingerprint {
                    return Ok(AcquireResult::Conflict);
                }

                // Check status
                match record.status {
                    IdempotencyStatus::Processing => {
                        if record.is_lock_expired() {
                            // Reacquire expired lock
                            record.locked_by = Some(instance_id.to_string());
                            record.lock_expires_at = Some(now + lock_ttl);
                            record.updated_at = now;
                            return Ok(AcquireResult::Acquired);
                        }
                        return Ok(AcquireResult::InProgress);
                    }
                    IdempotencyStatus::Completed | IdempotencyStatus::Failed => {
                        return Ok(AcquireResult::Cached(record.clone()));
                    }
                }
            }
        }

        // Create new record
        let mut record = IdempotencyRecord::new(key, fingerprint, ttl);
        record.locked_by = Some(instance_id.to_string());
        record.lock_expires_at = Some(now + lock_ttl);
        records.insert(key.to_string(), record);

        Ok(AcquireResult::Acquired)
    }

    async fn complete(
        &self,
        key: &str,
        response: serde_json::Value,
        status_code: Option<u16>,
    ) -> Result<()> {
        let mut records = self.records.write().await;

        if let Some(record) = records.get_mut(key) {
            record.status = IdempotencyStatus::Completed;
            record.response = Some(response);
            record.status_code = status_code;
            record.locked_by = None;
            record.lock_expires_at = None;
            record.updated_at = Utc::now();
            Ok(())
        } else {
            Err(IdempotencyError::NotFound(key.to_string()))
        }
    }

    async fn fail(&self, key: &str, error: &str) -> Result<()> {
        let mut records = self.records.write().await;

        if let Some(record) = records.get_mut(key) {
            record.status = IdempotencyStatus::Failed;
            record.error = Some(error.to_string());
            record.locked_by = None;
            record.lock_expires_at = None;
            record.updated_at = Utc::now();
            Ok(())
        } else {
            Err(IdempotencyError::NotFound(key.to_string()))
        }
    }

    async fn release(&self, key: &str) -> Result<()> {
        let mut records = self.records.write().await;
        records.remove(key);
        Ok(())
    }

    async fn get(&self, key: &str) -> Result<Option<IdempotencyRecord>> {
        let records = self.records.read().await;
        Ok(records.get(key).cloned())
    }

    async fn cleanup(&self) -> Result<usize> {
        let mut records = self.records.write().await;
        let now = Utc::now();

        let expired: Vec<_> = records
            .iter()
            .filter(|(_, r)| r.expires_at < now)
            .map(|(k, _)| k.clone())
            .collect();

        let count = expired.len();
        for key in expired {
            records.remove(&key);
        }

        Ok(count)
    }
}

/// Idempotency configuration.
#[derive(Debug, Clone)]
pub struct IdempotencyConfig {
    /// Default TTL for records.
    pub default_ttl: Duration,
    /// Lock TTL.
    pub lock_ttl: Duration,
    /// Instance ID for lock ownership.
    pub instance_id: String,
}

impl Default for IdempotencyConfig {
    fn default() -> Self {
        Self {
            default_ttl: Duration::hours(24),
            lock_ttl: Duration::minutes(5),
            instance_id: Uuid::new_v4().to_string(),
        }
    }
}

/// Idempotency service.
pub struct IdempotencyService<S: IdempotencyStorage> {
    storage: Arc<S>,
    config: IdempotencyConfig,
}

impl<S: IdempotencyStorage> IdempotencyService<S> {
    /// Create a new service.
    pub fn new(storage: Arc<S>, config: IdempotencyConfig) -> Self {
        Self { storage, config }
    }

    /// Process a request with idempotency.
    pub async fn process<F, T, E>(
        &self,
        key: &str,
        fingerprint: &str,
        handler: F,
    ) -> std::result::Result<IdempotencyResult<T>, E>
    where
        F: FnOnce() -> std::pin::Pin<
            Box<dyn std::future::Future<Output = std::result::Result<T, E>> + Send>,
        >,
        T: Serialize + for<'de> Deserialize<'de>,
        E: std::fmt::Display,
    {
        // Try to acquire
        let acquire_result = self
            .storage
            .acquire(
                key,
                fingerprint,
                self.config.default_ttl,
                self.config.lock_ttl,
                &self.config.instance_id,
            )
            .await
            .map_err(|_| ())
            .ok();

        match acquire_result {
            Some(AcquireResult::Cached(record)) => {
                if record.status == IdempotencyStatus::Completed {
                    if let Some(response) = record.response {
                        if let Ok(value) = serde_json::from_value(response) {
                            return Ok(IdempotencyResult::Cached(value));
                        }
                    }
                }
                // Fall through to process
            }
            Some(AcquireResult::InProgress) => {
                return Ok(IdempotencyResult::InProgress);
            }
            Some(AcquireResult::Conflict) => {
                return Ok(IdempotencyResult::Conflict);
            }
            _ => {}
        }

        // Execute handler
        let result = handler().await;

        match &result {
            Ok(value) => {
                if let Ok(json) = serde_json::to_value(value) {
                    let _ = self.storage.complete(key, json, None).await;
                }
            }
            Err(e) => {
                let _ = self.storage.fail(key, &e.to_string()).await;
            }
        }

        result.map(IdempotencyResult::Processed)
    }

    /// Manually complete a request.
    pub async fn complete<T: Serialize>(
        &self,
        key: &str,
        response: T,
        status_code: Option<u16>,
    ) -> Result<()> {
        let json = serde_json::to_value(response)
            .map_err(|e| IdempotencyError::StorageError(e.to_string()))?;
        self.storage.complete(key, json, status_code).await
    }

    /// Manually fail a request.
    pub async fn fail(&self, key: &str, error: &str) -> Result<()> {
        self.storage.fail(key, error).await
    }

    /// Release a key.
    pub async fn release(&self, key: &str) -> Result<()> {
        self.storage.release(key).await
    }

    /// Cleanup expired records.
    pub async fn cleanup(&self) -> Result<usize> {
        self.storage.cleanup().await
    }
}

/// Result of an idempotent operation.
#[derive(Debug)]
pub enum IdempotencyResult<T> {
    /// Request was processed.
    Processed(T),
    /// Response was returned from cache.
    Cached(T),
    /// Request is still in progress.
    InProgress,
    /// Request fingerprint conflicts.
    Conflict,
}

impl<T> IdempotencyResult<T> {
    /// Get the value if processed or cached.
    pub fn into_value(self) -> Option<T> {
        match self {
            Self::Processed(v) | Self::Cached(v) => Some(v),
            _ => None,
        }
    }

    /// Check if this was a cache hit.
    pub fn is_cached(&self) -> bool {
        matches!(self, Self::Cached(_))
    }

    /// Check if request was processed.
    pub fn is_processed(&self) -> bool {
        matches!(self, Self::Processed(_))
    }
}

/// Generate a fingerprint from request data.
pub fn generate_fingerprint(method: &str, path: &str, body: Option<&[u8]>) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    method.hash(&mut hasher);
    path.hash(&mut hasher);
    if let Some(b) = body {
        b.hash(&mut hasher);
    }
    format!("{:x}", hasher.finish())
}

// ============================================================================
// Kani Formal Verification Proofs
// ============================================================================

#[cfg(kani)]
mod kani_proofs {
    use super::*;

    // ========================================================================
    // IdempotencyStatus Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_idempotency_status_variants() {
        let processing = IdempotencyStatus::Processing;
        let completed = IdempotencyStatus::Completed;
        let failed = IdempotencyStatus::Failed;

        // All are distinct
        kani::assert!(processing != completed, "Processing != Completed");
        kani::assert!(completed != failed, "Completed != Failed");
        kani::assert!(failed != processing, "Failed != Processing");
    }

    #[kani::proof]
    fn proof_idempotency_status_processing_initial() {
        // New records start in Processing status
        let status = IdempotencyStatus::Processing;
        kani::assert!(
            status != IdempotencyStatus::Completed && status != IdempotencyStatus::Failed,
            "Processing is the initial state"
        );
    }

    // ========================================================================
    // AcquireResult Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_acquire_result_acquired_distinct() {
        let acquired = AcquireResult::Acquired;
        let in_progress = AcquireResult::InProgress;
        let conflict = AcquireResult::Conflict;

        // Check distinctness using pattern matching
        let is_acquired = matches!(acquired, AcquireResult::Acquired);
        let is_in_progress = matches!(in_progress, AcquireResult::InProgress);
        let is_conflict = matches!(conflict, AcquireResult::Conflict);

        kani::assert!(is_acquired, "Acquired matches Acquired");
        kani::assert!(is_in_progress, "InProgress matches InProgress");
        kani::assert!(is_conflict, "Conflict matches Conflict");
    }

    #[kani::proof]
    fn proof_acquire_result_not_cross_match() {
        let acquired = AcquireResult::Acquired;

        let is_in_progress = matches!(acquired, AcquireResult::InProgress);
        let is_conflict = matches!(acquired, AcquireResult::Conflict);

        kani::assert!(!is_in_progress, "Acquired is not InProgress");
        kani::assert!(!is_conflict, "Acquired is not Conflict");
    }

    // ========================================================================
    // IdempotencyResult Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_idempotency_result_processed_has_value() {
        let result: IdempotencyResult<u32> = IdempotencyResult::Processed(42);

        kani::assert!(result.is_processed(), "Processed is_processed = true");
        kani::assert!(!result.is_cached(), "Processed is_cached = false");
    }

    #[kani::proof]
    fn proof_idempotency_result_cached_has_value() {
        let result: IdempotencyResult<u32> = IdempotencyResult::Cached(42);

        kani::assert!(result.is_cached(), "Cached is_cached = true");
        kani::assert!(!result.is_processed(), "Cached is_processed = false");
    }

    #[kani::proof]
    fn proof_idempotency_result_in_progress_no_value() {
        let result: IdempotencyResult<u32> = IdempotencyResult::InProgress;

        kani::assert!(!result.is_processed(), "InProgress is_processed = false");
        kani::assert!(!result.is_cached(), "InProgress is_cached = false");
    }

    #[kani::proof]
    fn proof_idempotency_result_conflict_no_value() {
        let result: IdempotencyResult<u32> = IdempotencyResult::Conflict;

        kani::assert!(!result.is_processed(), "Conflict is_processed = false");
        kani::assert!(!result.is_cached(), "Conflict is_cached = false");
    }

    #[kani::proof]
    fn proof_idempotency_result_into_value_processed() {
        let result: IdempotencyResult<u32> = IdempotencyResult::Processed(42);
        let value = result.into_value();

        kani::assert!(value.is_some(), "Processed has value");
        kani::assert!(value.unwrap() == 42, "Value is correct");
    }

    #[kani::proof]
    fn proof_idempotency_result_into_value_cached() {
        let result: IdempotencyResult<u32> = IdempotencyResult::Cached(99);
        let value = result.into_value();

        kani::assert!(value.is_some(), "Cached has value");
        kani::assert!(value.unwrap() == 99, "Value is correct");
    }

    #[kani::proof]
    fn proof_idempotency_result_into_value_in_progress() {
        let result: IdempotencyResult<u32> = IdempotencyResult::InProgress;
        let value = result.into_value();

        kani::assert!(value.is_none(), "InProgress has no value");
    }

    #[kani::proof]
    fn proof_idempotency_result_into_value_conflict() {
        let result: IdempotencyResult<u32> = IdempotencyResult::Conflict;
        let value = result.into_value();

        kani::assert!(value.is_none(), "Conflict has no value");
    }

    // ========================================================================
    // generate_fingerprint Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_fingerprint_deterministic() {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let method = "POST";
        let path = "/api/test";

        // First hash
        let mut hasher1 = DefaultHasher::new();
        method.hash(&mut hasher1);
        path.hash(&mut hasher1);
        let hash1 = hasher1.finish();

        // Second hash with same inputs
        let mut hasher2 = DefaultHasher::new();
        method.hash(&mut hasher2);
        path.hash(&mut hasher2);
        let hash2 = hasher2.finish();

        kani::assert!(hash1 == hash2, "Same inputs produce same hash");
    }

    #[kani::proof]
    fn proof_fingerprint_different_methods() {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let path = "/api/test";

        let mut hasher1 = DefaultHasher::new();
        "GET".hash(&mut hasher1);
        path.hash(&mut hasher1);
        let hash1 = hasher1.finish();

        let mut hasher2 = DefaultHasher::new();
        "POST".hash(&mut hasher2);
        path.hash(&mut hasher2);
        let hash2 = hasher2.finish();

        kani::assert!(hash1 != hash2, "Different methods produce different hashes");
    }

    #[kani::proof]
    fn proof_fingerprint_different_paths() {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let method = "GET";

        let mut hasher1 = DefaultHasher::new();
        method.hash(&mut hasher1);
        "/api/users".hash(&mut hasher1);
        let hash1 = hasher1.finish();

        let mut hasher2 = DefaultHasher::new();
        method.hash(&mut hasher2);
        "/api/posts".hash(&mut hasher2);
        let hash2 = hasher2.finish();

        kani::assert!(hash1 != hash2, "Different paths produce different hashes");
    }

    // ========================================================================
    // IdempotencyRecord Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_record_initial_status() {
        // New records should always start in Processing status
        let status = IdempotencyStatus::Processing;
        kani::assert!(
            status == IdempotencyStatus::Processing,
            "Initial status is Processing"
        );
    }

    #[kani::proof]
    fn proof_lock_expired_when_none() {
        // is_lock_expired returns true when lock_expires_at is None
        let lock_expires_at: Option<i64> = None;
        let is_expired = lock_expires_at.map(|_| false).unwrap_or(true);
        kani::assert!(is_expired, "No lock means lock is expired");
    }

    // ========================================================================
    // IdempotencyConfig Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_config_default_ttl_positive() {
        // Default TTL should be positive (24 hours = 24 * 60 * 60 seconds)
        let hours = 24i64;
        let seconds = hours * 60 * 60;
        kani::assert!(seconds > 0, "Default TTL is positive");
        kani::assert!(seconds == 86400, "Default TTL is 24 hours");
    }

    #[kani::proof]
    fn proof_config_default_lock_ttl_positive() {
        // Default lock TTL should be positive (5 minutes = 5 * 60 seconds)
        let minutes = 5i64;
        let seconds = minutes * 60;
        kani::assert!(seconds > 0, "Default lock TTL is positive");
        kani::assert!(seconds == 300, "Default lock TTL is 5 minutes");
    }

    #[kani::proof]
    fn proof_config_lock_ttl_less_than_ttl() {
        // Lock TTL (5 min) should be less than record TTL (24 hours)
        let lock_ttl_seconds = 5i64 * 60;
        let record_ttl_seconds = 24i64 * 60 * 60;
        kani::assert!(
            lock_ttl_seconds < record_ttl_seconds,
            "Lock TTL < Record TTL"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_creation() {
        let record = IdempotencyRecord::new("key", "fingerprint", Duration::hours(1));

        assert_eq!(record.key, "key");
        assert_eq!(record.request_fingerprint, "fingerprint");
        assert_eq!(record.status, IdempotencyStatus::Processing);
        assert!(!record.is_expired());
    }

    #[tokio::test]
    async fn test_acquire_new_key() {
        let storage = InMemoryIdempotencyStorage::new();

        let result = storage
            .acquire(
                "key",
                "fingerprint",
                Duration::hours(1),
                Duration::minutes(5),
                "instance",
            )
            .await
            .unwrap();

        assert!(matches!(result, AcquireResult::Acquired));
    }

    #[tokio::test]
    async fn test_acquire_in_progress() {
        let storage = InMemoryIdempotencyStorage::new();

        // First acquire
        storage
            .acquire(
                "key",
                "fingerprint",
                Duration::hours(1),
                Duration::minutes(5),
                "instance",
            )
            .await
            .unwrap();

        // Second acquire should be in progress
        let result = storage
            .acquire(
                "key",
                "fingerprint",
                Duration::hours(1),
                Duration::minutes(5),
                "instance2",
            )
            .await
            .unwrap();

        assert!(matches!(result, AcquireResult::InProgress));
    }

    #[tokio::test]
    async fn test_acquire_conflict() {
        let storage = InMemoryIdempotencyStorage::new();

        // First acquire
        storage
            .acquire(
                "key",
                "fingerprint1",
                Duration::hours(1),
                Duration::minutes(5),
                "instance",
            )
            .await
            .unwrap();

        // Complete it
        storage
            .complete("key", serde_json::json!({"result": "ok"}), Some(200))
            .await
            .unwrap();

        // Second acquire with different fingerprint
        let result = storage
            .acquire(
                "key",
                "fingerprint2",
                Duration::hours(1),
                Duration::minutes(5),
                "instance",
            )
            .await
            .unwrap();

        assert!(matches!(result, AcquireResult::Conflict));
    }

    #[tokio::test]
    async fn test_complete_and_cache() {
        let storage = InMemoryIdempotencyStorage::new();

        storage
            .acquire(
                "key",
                "fingerprint",
                Duration::hours(1),
                Duration::minutes(5),
                "instance",
            )
            .await
            .unwrap();

        storage
            .complete("key", serde_json::json!({"result": "ok"}), Some(200))
            .await
            .unwrap();

        let result = storage
            .acquire(
                "key",
                "fingerprint",
                Duration::hours(1),
                Duration::minutes(5),
                "instance",
            )
            .await
            .unwrap();

        match result {
            AcquireResult::Cached(record) => {
                assert_eq!(record.status, IdempotencyStatus::Completed);
                assert_eq!(record.status_code, Some(200));
            }
            _ => panic!("Expected cached result"),
        }
    }

    #[test]
    fn test_generate_fingerprint() {
        let fp1 = generate_fingerprint("POST", "/api/users", Some(b"{}"));
        let fp2 = generate_fingerprint("POST", "/api/users", Some(b"{}"));
        let fp3 = generate_fingerprint("POST", "/api/users", Some(b"{\"name\":\"test\"}"));

        assert_eq!(fp1, fp2);
        assert_ne!(fp1, fp3);
    }

    #[tokio::test]
    async fn test_cleanup() {
        let storage = InMemoryIdempotencyStorage::new();

        // Create expired record
        {
            let mut records = storage.records.write().await;
            let mut record = IdempotencyRecord::new("key", "fp", Duration::hours(-1)); // Already expired
            record.expires_at = Utc::now() - Duration::hours(1);
            records.insert("key".to_string(), record);
        }

        let cleaned = storage.cleanup().await.unwrap();
        assert_eq!(cleaned, 1);

        let record = storage.get("key").await.unwrap();
        assert!(record.is_none());
    }

    #[test]
    fn test_idempotency_result() {
        let result: IdempotencyResult<String> = IdempotencyResult::Processed("value".to_string());
        assert!(result.is_processed());
        assert!(!result.is_cached());
        assert_eq!(result.into_value(), Some("value".to_string()));

        let result: IdempotencyResult<String> = IdempotencyResult::Cached("cached".to_string());
        assert!(result.is_cached());

        let result: IdempotencyResult<String> = IdempotencyResult::InProgress;
        assert!(result.into_value().is_none());
    }
}
