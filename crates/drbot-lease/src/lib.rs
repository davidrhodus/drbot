//! Lease-based resource management for drbot.
//!
//! This crate provides:
//! - Time-based leases
//! - Lease renewal
//! - Automatic expiration
//! - Lease coordination

use chrono::{DateTime, Duration, Utc};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use thiserror::Error;

/// Lease error types.
#[derive(Error, Debug)]
pub enum LeaseError {
    #[error("Lease expired")]
    Expired,

    #[error("Lease not found: {0}")]
    NotFound(String),

    #[error("Lease already held")]
    AlreadyHeld,

    #[error("Invalid lease duration")]
    InvalidDuration,

    #[error("Renewal failed")]
    RenewalFailed,
}

/// Result type for lease operations.
pub type Result<T> = std::result::Result<T, LeaseError>;

/// Lease ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LeaseId(u64);

impl LeaseId {
    /// Generate new lease ID.
    pub fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        Self(COUNTER.fetch_add(1, Ordering::SeqCst))
    }
}

impl Default for LeaseId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for LeaseId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "lease-{}", self.0)
    }
}

/// Lease state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeaseState {
    /// Lease is active.
    Active,
    /// Lease is expired.
    Expired,
    /// Lease is released.
    Released,
}

/// Lease information.
#[derive(Debug, Clone)]
pub struct LeaseInfo {
    /// Lease ID.
    pub id: LeaseId,
    /// Resource key.
    pub resource_key: String,
    /// Holder ID.
    pub holder: String,
    /// Created time.
    pub created_at: DateTime<Utc>,
    /// Expires at.
    pub expires_at: DateTime<Utc>,
    /// Last renewed.
    pub renewed_at: Option<DateTime<Utc>>,
    /// State.
    pub state: LeaseState,
}

impl LeaseInfo {
    /// Check if expired.
    pub fn is_expired(&self) -> bool {
        self.state == LeaseState::Expired || Utc::now() > self.expires_at
    }

    /// Get remaining time.
    pub fn remaining(&self) -> Duration {
        if self.is_expired() {
            Duration::zero()
        } else {
            self.expires_at - Utc::now()
        }
    }

    /// Get remaining seconds.
    pub fn remaining_secs(&self) -> i64 {
        self.remaining().num_seconds()
    }
}

/// Lease handle.
pub struct Lease {
    info: Arc<Mutex<LeaseInfo>>,
}

impl Lease {
    /// Create new lease.
    pub fn new(
        resource_key: impl Into<String>,
        holder: impl Into<String>,
        duration: Duration,
    ) -> Result<Self> {
        if duration <= Duration::zero() {
            return Err(LeaseError::InvalidDuration);
        }

        let now = Utc::now();
        let info = LeaseInfo {
            id: LeaseId::new(),
            resource_key: resource_key.into(),
            holder: holder.into(),
            created_at: now,
            expires_at: now + duration,
            renewed_at: None,
            state: LeaseState::Active,
        };

        Ok(Self {
            info: Arc::new(Mutex::new(info)),
        })
    }

    /// Get lease ID.
    pub fn id(&self) -> LeaseId {
        self.info.lock().unwrap().id
    }

    /// Get lease info.
    pub fn info(&self) -> LeaseInfo {
        self.info.lock().unwrap().clone()
    }

    /// Check if valid.
    pub fn is_valid(&self) -> bool {
        let info = self.info.lock().unwrap();
        info.state == LeaseState::Active && !info.is_expired()
    }

    /// Renew lease.
    pub fn renew(&self, duration: Duration) -> Result<()> {
        if duration <= Duration::zero() {
            return Err(LeaseError::InvalidDuration);
        }

        let mut info = self.info.lock().unwrap();
        if info.is_expired() {
            return Err(LeaseError::Expired);
        }

        let now = Utc::now();
        info.expires_at = now + duration;
        info.renewed_at = Some(now);
        Ok(())
    }

    /// Release lease.
    pub fn release(&self) {
        let mut info = self.info.lock().unwrap();
        info.state = LeaseState::Released;
    }

    /// Get remaining time.
    pub fn remaining(&self) -> Duration {
        self.info.lock().unwrap().remaining()
    }
}

impl Clone for Lease {
    fn clone(&self) -> Self {
        Self {
            info: self.info.clone(),
        }
    }
}

/// Lease manager.
pub struct LeaseManager {
    leases: Mutex<HashMap<String, Lease>>,
    default_duration: Duration,
}

impl LeaseManager {
    /// Create new lease manager.
    pub fn new(default_duration: Duration) -> Self {
        Self {
            leases: Mutex::new(HashMap::new()),
            default_duration,
        }
    }

    /// Acquire lease for resource.
    pub fn acquire(
        &self,
        resource_key: impl Into<String>,
        holder: impl Into<String>,
    ) -> Result<Lease> {
        self.acquire_with_duration(resource_key, holder, self.default_duration)
    }

    /// Acquire lease with specific duration.
    pub fn acquire_with_duration(
        &self,
        resource_key: impl Into<String>,
        holder: impl Into<String>,
        duration: Duration,
    ) -> Result<Lease> {
        let key = resource_key.into();
        let mut leases = self.leases.lock().unwrap();

        // Check if already leased
        if let Some(existing) = leases.get(&key) {
            if existing.is_valid() {
                return Err(LeaseError::AlreadyHeld);
            }
        }

        let lease = Lease::new(&key, holder, duration)?;
        leases.insert(key, lease.clone());
        Ok(lease)
    }

    /// Release lease.
    pub fn release(&self, resource_key: &str) -> bool {
        let mut leases = self.leases.lock().unwrap();
        if let Some(lease) = leases.remove(resource_key) {
            lease.release();
            true
        } else {
            false
        }
    }

    /// Get lease for resource.
    pub fn get(&self, resource_key: &str) -> Option<Lease> {
        let leases = self.leases.lock().unwrap();
        leases.get(resource_key).cloned()
    }

    /// Check if resource is leased.
    pub fn is_leased(&self, resource_key: &str) -> bool {
        let leases = self.leases.lock().unwrap();
        leases.get(resource_key).map_or(false, |l| l.is_valid())
    }

    /// Clean up expired leases.
    pub fn cleanup(&self) -> usize {
        let mut leases = self.leases.lock().unwrap();
        let before = leases.len();
        leases.retain(|_, lease| lease.is_valid());
        before - leases.len()
    }

    /// Get all active leases.
    pub fn active_leases(&self) -> Vec<LeaseInfo> {
        let leases = self.leases.lock().unwrap();
        leases
            .values()
            .filter(|l| l.is_valid())
            .map(|l| l.info())
            .collect()
    }

    /// Get lease count.
    pub fn count(&self) -> usize {
        self.leases.lock().unwrap().len()
    }
}

/// Renewable lease with automatic renewal.
pub struct RenewableLease {
    lease: Lease,
    renewal_duration: Duration,
    renewal_threshold: Duration,
}

impl RenewableLease {
    /// Create new renewable lease.
    pub fn new(lease: Lease, renewal_duration: Duration, renewal_threshold: Duration) -> Self {
        Self {
            lease,
            renewal_duration,
            renewal_threshold,
        }
    }

    /// Get underlying lease.
    pub fn lease(&self) -> &Lease {
        &self.lease
    }

    /// Check if renewal needed.
    pub fn needs_renewal(&self) -> bool {
        self.lease.remaining() <= self.renewal_threshold
    }

    /// Renew if needed.
    pub fn renew_if_needed(&self) -> Result<bool> {
        if self.needs_renewal() {
            self.lease.renew(self.renewal_duration)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Force renewal.
    pub fn renew(&self) -> Result<()> {
        self.lease.renew(self.renewal_duration)
    }
}

/// Lease guard that releases on drop.
pub struct LeaseGuard<'a> {
    manager: &'a LeaseManager,
    resource_key: String,
}

impl<'a> LeaseGuard<'a> {
    /// Create new guard.
    pub fn new(manager: &'a LeaseManager, resource_key: String) -> Self {
        Self {
            manager,
            resource_key,
        }
    }

    /// Get resource key.
    pub fn resource_key(&self) -> &str {
        &self.resource_key
    }
}

impl Drop for LeaseGuard<'_> {
    fn drop(&mut self) {
        self.manager.release(&self.resource_key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lease_creation() {
        let lease = Lease::new("resource1", "holder1", Duration::seconds(60)).unwrap();
        assert!(lease.is_valid());
        assert!(lease.remaining().num_seconds() > 0);
    }

    #[test]
    fn test_lease_expiration() {
        let lease = Lease::new("resource1", "holder1", Duration::milliseconds(1)).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
        assert!(!lease.is_valid());
    }

    #[test]
    fn test_lease_renewal() {
        let lease = Lease::new("resource1", "holder1", Duration::seconds(1)).unwrap();
        lease.renew(Duration::seconds(60)).unwrap();
        assert!(lease.remaining().num_seconds() > 50);
    }

    #[test]
    fn test_lease_release() {
        let lease = Lease::new("resource1", "holder1", Duration::seconds(60)).unwrap();
        lease.release();
        assert!(!lease.is_valid());
    }

    #[test]
    fn test_lease_manager() {
        let manager = LeaseManager::new(Duration::seconds(60));

        let lease = manager.acquire("resource1", "holder1").unwrap();
        assert!(lease.is_valid());
        assert!(manager.is_leased("resource1"));

        // Can't acquire same resource
        assert!(manager.acquire("resource1", "holder2").is_err());

        manager.release("resource1");
        assert!(!manager.is_leased("resource1"));
    }

    #[test]
    fn test_renewable_lease() {
        let lease = Lease::new("resource1", "holder1", Duration::seconds(10)).unwrap();
        let renewable = RenewableLease::new(lease, Duration::seconds(60), Duration::seconds(5));

        // After 10 seconds, should need renewal (remaining <= threshold)
        // For now, just test the interface
        assert!(!renewable.needs_renewal()); // Still have 10s, threshold is 5s
    }
}
