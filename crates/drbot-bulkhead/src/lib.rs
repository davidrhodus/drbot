//! Bulkhead pattern for isolation in drbot.
//!
//! This crate provides:
//! - Semaphore-based bulkheads
//! - Thread pool isolation
//! - Resource partitioning
//! - Fairness policies

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::{OwnedSemaphorePermit, RwLock, Semaphore};

/// Bulkhead error types.
#[derive(Error, Debug)]
pub enum BulkheadError {
    #[error("Bulkhead full: {0} concurrent executions")]
    Full(usize),

    #[error("Acquisition timeout after {0:?}")]
    Timeout(Duration),

    #[error("Bulkhead closed")]
    Closed,

    #[error("Queue full")]
    QueueFull,
}

/// Result type for bulkhead operations.
pub type Result<T> = std::result::Result<T, BulkheadError>;

/// Bulkhead configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BulkheadConfig {
    /// Maximum concurrent executions.
    pub max_concurrent: usize,
    /// Maximum queue size (0 for no queue).
    pub max_queue: usize,
    /// Queue timeout.
    pub queue_timeout: Duration,
    /// Name for metrics/logging.
    pub name: String,
}

impl Default for BulkheadConfig {
    fn default() -> Self {
        Self {
            max_concurrent: 10,
            max_queue: 100,
            queue_timeout: Duration::from_secs(30),
            name: "default".to_string(),
        }
    }
}

impl BulkheadConfig {
    /// Create a new config with name.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Default::default()
        }
    }

    /// Set max concurrent.
    pub fn max_concurrent(mut self, n: usize) -> Self {
        self.max_concurrent = n;
        self
    }

    /// Set max queue size.
    pub fn max_queue(mut self, n: usize) -> Self {
        self.max_queue = n;
        self
    }

    /// Set queue timeout.
    pub fn queue_timeout(mut self, timeout: Duration) -> Self {
        self.queue_timeout = timeout;
        self
    }
}

/// Bulkhead statistics.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct BulkheadStats {
    /// Current concurrent executions.
    pub current_concurrent: usize,
    /// Current queue size.
    pub current_queue: usize,
    /// Total successful acquisitions.
    pub successful_acquisitions: u64,
    /// Total rejected (bulkhead full).
    pub rejected: u64,
    /// Total timed out.
    pub timed_out: u64,
    /// Average wait time in milliseconds.
    pub avg_wait_ms: f64,
}

/// A semaphore-based bulkhead.
pub struct Bulkhead {
    config: BulkheadConfig,
    semaphore: Arc<Semaphore>,
    current_concurrent: AtomicUsize,
    current_queue: AtomicUsize,
    successful: AtomicU64,
    rejected: AtomicU64,
    timed_out: AtomicU64,
    total_wait_ms: AtomicU64,
    closed: std::sync::atomic::AtomicBool,
}

impl Bulkhead {
    /// Create a new bulkhead.
    pub fn new(config: BulkheadConfig) -> Self {
        let semaphore = Arc::new(Semaphore::new(config.max_concurrent));

        Self {
            config,
            semaphore,
            current_concurrent: AtomicUsize::new(0),
            current_queue: AtomicUsize::new(0),
            successful: AtomicU64::new(0),
            rejected: AtomicU64::new(0),
            timed_out: AtomicU64::new(0),
            total_wait_ms: AtomicU64::new(0),
            closed: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Try to acquire a permit without waiting.
    pub fn try_acquire(&self) -> Result<BulkheadPermit> {
        if self.closed.load(Ordering::Relaxed) {
            return Err(BulkheadError::Closed);
        }

        match self.semaphore.clone().try_acquire_owned() {
            Ok(permit) => {
                self.current_concurrent.fetch_add(1, Ordering::Relaxed);
                self.successful.fetch_add(1, Ordering::Relaxed);
                Ok(BulkheadPermit {
                    _permit: permit,
                    current_concurrent: &self.current_concurrent,
                })
            }
            Err(_) => {
                self.rejected.fetch_add(1, Ordering::Relaxed);
                Err(BulkheadError::Full(self.config.max_concurrent))
            }
        }
    }

    /// Acquire a permit, waiting if necessary.
    pub async fn acquire(&self) -> Result<BulkheadPermit> {
        if self.closed.load(Ordering::Relaxed) {
            return Err(BulkheadError::Closed);
        }

        // Check queue capacity
        let queue_size = self.current_queue.fetch_add(1, Ordering::Relaxed);
        if self.config.max_queue > 0 && queue_size >= self.config.max_queue {
            self.current_queue.fetch_sub(1, Ordering::Relaxed);
            self.rejected.fetch_add(1, Ordering::Relaxed);
            return Err(BulkheadError::QueueFull);
        }

        let start = std::time::Instant::now();

        let result = if self.config.queue_timeout.is_zero() {
            // No timeout, wait indefinitely
            self.semaphore
                .clone()
                .acquire_owned()
                .await
                .map_err(|_| BulkheadError::Closed)
        } else {
            // Wait with timeout
            tokio::time::timeout(
                self.config.queue_timeout,
                self.semaphore.clone().acquire_owned(),
            )
            .await
            .map_err(|_| {
                self.timed_out.fetch_add(1, Ordering::Relaxed);
                BulkheadError::Timeout(self.config.queue_timeout)
            })?
            .map_err(|_| BulkheadError::Closed)
        };

        self.current_queue.fetch_sub(1, Ordering::Relaxed);

        match result {
            Ok(permit) => {
                let wait_ms = start.elapsed().as_millis() as u64;
                self.total_wait_ms.fetch_add(wait_ms, Ordering::Relaxed);
                self.current_concurrent.fetch_add(1, Ordering::Relaxed);
                self.successful.fetch_add(1, Ordering::Relaxed);
                Ok(BulkheadPermit {
                    _permit: permit,
                    current_concurrent: &self.current_concurrent,
                })
            }
            Err(e) => Err(e),
        }
    }

    /// Get current statistics.
    pub fn stats(&self) -> BulkheadStats {
        let successful = self.successful.load(Ordering::Relaxed);
        let total_wait = self.total_wait_ms.load(Ordering::Relaxed);

        BulkheadStats {
            current_concurrent: self.current_concurrent.load(Ordering::Relaxed),
            current_queue: self.current_queue.load(Ordering::Relaxed),
            successful_acquisitions: successful,
            rejected: self.rejected.load(Ordering::Relaxed),
            timed_out: self.timed_out.load(Ordering::Relaxed),
            avg_wait_ms: if successful > 0 {
                total_wait as f64 / successful as f64
            } else {
                0.0
            },
        }
    }

    /// Get the config.
    pub fn config(&self) -> &BulkheadConfig {
        &self.config
    }

    /// Check if bulkhead is closed.
    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Relaxed)
    }

    /// Close the bulkhead.
    pub fn close(&self) {
        self.closed.store(true, Ordering::SeqCst);
        self.semaphore.close();
    }

    /// Get available permits.
    pub fn available_permits(&self) -> usize {
        self.semaphore.available_permits()
    }
}

/// A permit acquired from a bulkhead.
pub struct BulkheadPermit<'a> {
    _permit: OwnedSemaphorePermit,
    current_concurrent: &'a AtomicUsize,
}

impl<'a> Drop for BulkheadPermit<'a> {
    fn drop(&mut self) {
        self.current_concurrent.fetch_sub(1, Ordering::Relaxed);
    }
}

/// Multi-tenant bulkhead with per-tenant limits.
pub struct TenantBulkhead {
    global_config: BulkheadConfig,
    tenant_config: BulkheadConfig,
    global_semaphore: Arc<Semaphore>,
    tenant_semaphores: RwLock<HashMap<String, Arc<Semaphore>>>,
    stats: RwLock<HashMap<String, TenantStats>>,
}

#[derive(Debug, Default, Clone)]
struct TenantStats {
    successful: u64,
    rejected: u64,
}

impl TenantBulkhead {
    /// Create a new tenant bulkhead.
    pub fn new(global_config: BulkheadConfig, tenant_config: BulkheadConfig) -> Self {
        let global_semaphore = Arc::new(Semaphore::new(global_config.max_concurrent));

        Self {
            global_config,
            tenant_config,
            global_semaphore,
            tenant_semaphores: RwLock::new(HashMap::new()),
            stats: RwLock::new(HashMap::new()),
        }
    }

    /// Get or create tenant semaphore.
    async fn get_tenant_semaphore(&self, tenant: &str) -> Arc<Semaphore> {
        {
            let semaphores = self.tenant_semaphores.read().await;
            if let Some(sem) = semaphores.get(tenant) {
                return sem.clone();
            }
        }

        let mut semaphores = self.tenant_semaphores.write().await;
        semaphores
            .entry(tenant.to_string())
            .or_insert_with(|| Arc::new(Semaphore::new(self.tenant_config.max_concurrent)))
            .clone()
    }

    /// Try to acquire a permit for a tenant.
    pub async fn try_acquire(&self, tenant: &str) -> Result<TenantPermit> {
        // First try global
        let global_permit = self
            .global_semaphore
            .clone()
            .try_acquire_owned()
            .map_err(|_| BulkheadError::Full(self.global_config.max_concurrent))?;

        // Then try tenant
        let tenant_sem = self.get_tenant_semaphore(tenant).await;
        match tenant_sem.clone().try_acquire_owned() {
            Ok(tenant_permit) => {
                let mut stats = self.stats.write().await;
                stats.entry(tenant.to_string()).or_default().successful += 1;

                Ok(TenantPermit {
                    _global_permit: global_permit,
                    _tenant_permit: tenant_permit,
                })
            }
            Err(_) => {
                let mut stats = self.stats.write().await;
                stats.entry(tenant.to_string()).or_default().rejected += 1;
                Err(BulkheadError::Full(self.tenant_config.max_concurrent))
            }
        }
    }

    /// Acquire a permit for a tenant with waiting.
    pub async fn acquire(&self, tenant: &str) -> Result<TenantPermit> {
        let timeout = self.global_config.queue_timeout;

        // First acquire global
        let global_permit =
            tokio::time::timeout(timeout, self.global_semaphore.clone().acquire_owned())
                .await
                .map_err(|_| BulkheadError::Timeout(timeout))?
                .map_err(|_| BulkheadError::Closed)?;

        // Then acquire tenant
        let tenant_sem = self.get_tenant_semaphore(tenant).await;
        let tenant_permit = tokio::time::timeout(timeout, tenant_sem.acquire_owned())
            .await
            .map_err(|_| BulkheadError::Timeout(timeout))?
            .map_err(|_| BulkheadError::Closed)?;

        let mut stats = self.stats.write().await;
        stats.entry(tenant.to_string()).or_default().successful += 1;

        Ok(TenantPermit {
            _global_permit: global_permit,
            _tenant_permit: tenant_permit,
        })
    }

    /// Get global available permits.
    pub fn global_available(&self) -> usize {
        self.global_semaphore.available_permits()
    }

    /// Get tenant available permits.
    pub async fn tenant_available(&self, tenant: &str) -> usize {
        let semaphores = self.tenant_semaphores.read().await;
        semaphores
            .get(tenant)
            .map(|s| s.available_permits())
            .unwrap_or(self.tenant_config.max_concurrent)
    }
}

/// A permit for a tenant.
pub struct TenantPermit {
    _global_permit: OwnedSemaphorePermit,
    _tenant_permit: OwnedSemaphorePermit,
}

/// Weighted bulkhead for priority-based access.
pub struct WeightedBulkhead {
    config: BulkheadConfig,
    semaphore: Arc<Semaphore>,
    weights: RwLock<HashMap<String, usize>>,
}

impl WeightedBulkhead {
    /// Create a new weighted bulkhead.
    pub fn new(config: BulkheadConfig) -> Self {
        let semaphore = Arc::new(Semaphore::new(config.max_concurrent));

        Self {
            config,
            semaphore,
            weights: RwLock::new(HashMap::new()),
        }
    }

    /// Set weight for a key (default weight is 1).
    pub async fn set_weight(&self, key: &str, weight: usize) {
        let mut weights = self.weights.write().await;
        weights.insert(key.to_string(), weight);
    }

    /// Get weight for a key.
    pub async fn get_weight(&self, key: &str) -> usize {
        let weights = self.weights.read().await;
        *weights.get(key).unwrap_or(&1)
    }

    /// Acquire permits based on weight.
    pub async fn acquire(&self, key: &str) -> Result<WeightedPermit> {
        let weight = self.get_weight(key).await;

        if weight > self.config.max_concurrent {
            return Err(BulkheadError::Full(self.config.max_concurrent));
        }

        let permit = tokio::time::timeout(
            self.config.queue_timeout,
            self.semaphore.clone().acquire_many_owned(weight as u32),
        )
        .await
        .map_err(|_| BulkheadError::Timeout(self.config.queue_timeout))?
        .map_err(|_| BulkheadError::Closed)?;

        Ok(WeightedPermit { _permit: permit })
    }

    /// Available permits.
    pub fn available(&self) -> usize {
        self.semaphore.available_permits()
    }
}

/// A weighted permit.
pub struct WeightedPermit {
    _permit: tokio::sync::OwnedSemaphorePermit,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_bulkhead_try_acquire() {
        let config = BulkheadConfig::new("test").max_concurrent(2);
        let bulkhead = Bulkhead::new(config);

        let _permit1 = bulkhead.try_acquire().unwrap();
        let _permit2 = bulkhead.try_acquire().unwrap();

        // Third should fail
        let result = bulkhead.try_acquire();
        assert!(matches!(result, Err(BulkheadError::Full(2))));
    }

    #[tokio::test]
    async fn test_bulkhead_acquire() {
        let config = BulkheadConfig::new("test").max_concurrent(2);
        let bulkhead = Arc::new(Bulkhead::new(config));

        let permit1 = bulkhead.acquire().await.unwrap();
        let _permit2 = bulkhead.acquire().await.unwrap();

        // Drop first permit
        drop(permit1);

        // Now we can acquire again
        let _permit3 = bulkhead.acquire().await.unwrap();
    }

    #[tokio::test]
    async fn test_bulkhead_stats() {
        let config = BulkheadConfig::new("test").max_concurrent(1);
        let bulkhead = Bulkhead::new(config);

        let _permit = bulkhead.try_acquire().unwrap();
        let _ = bulkhead.try_acquire(); // Rejected

        let stats = bulkhead.stats();
        assert_eq!(stats.successful_acquisitions, 1);
        assert_eq!(stats.rejected, 1);
        assert_eq!(stats.current_concurrent, 1);
    }

    #[tokio::test]
    async fn test_bulkhead_close() {
        let config = BulkheadConfig::new("test").max_concurrent(2);
        let bulkhead = Bulkhead::new(config);

        bulkhead.close();
        assert!(bulkhead.is_closed());

        let result = bulkhead.try_acquire();
        assert!(matches!(result, Err(BulkheadError::Closed)));
    }

    #[tokio::test]
    async fn test_tenant_bulkhead() {
        let global = BulkheadConfig::new("global").max_concurrent(10);
        let tenant = BulkheadConfig::new("tenant").max_concurrent(2);
        let bulkhead = TenantBulkhead::new(global, tenant);

        let _permit1 = bulkhead.try_acquire("tenant-a").await.unwrap();
        let _permit2 = bulkhead.try_acquire("tenant-a").await.unwrap();

        // Third for same tenant should fail
        let result = bulkhead.try_acquire("tenant-a").await;
        assert!(matches!(result, Err(BulkheadError::Full(_))));

        // Different tenant should work
        let _permit3 = bulkhead.try_acquire("tenant-b").await.unwrap();
    }

    #[tokio::test]
    async fn test_weighted_bulkhead() {
        let config = BulkheadConfig::new("test").max_concurrent(10);
        let bulkhead = WeightedBulkhead::new(config);

        bulkhead.set_weight("heavy", 5).await;
        bulkhead.set_weight("light", 1).await;

        let _permit1 = bulkhead.acquire("heavy").await.unwrap();
        assert_eq!(bulkhead.available(), 5);

        let _permit2 = bulkhead.acquire("light").await.unwrap();
        assert_eq!(bulkhead.available(), 4);
    }

    #[test]
    fn test_config_builder() {
        let config = BulkheadConfig::new("test")
            .max_concurrent(5)
            .max_queue(50)
            .queue_timeout(Duration::from_secs(10));

        assert_eq!(config.name, "test");
        assert_eq!(config.max_concurrent, 5);
        assert_eq!(config.max_queue, 50);
        assert_eq!(config.queue_timeout, Duration::from_secs(10));
    }
}
