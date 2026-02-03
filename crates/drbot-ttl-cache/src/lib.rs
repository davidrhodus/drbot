//! TTL-based cache for drbot.
//!
//! This crate provides:
//! - Time-to-live cache entries
//! - Automatic expiration
//! - Configurable TTL

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::RwLock;
use std::time::{Duration, Instant};
use thiserror::Error;

/// TTL cache error types.
#[derive(Error, Debug, Clone)]
pub enum TtlCacheError {
    #[error("Entry expired")]
    Expired,

    #[error("Key not found")]
    NotFound,
}

/// Result type for TTL cache operations.
pub type Result<T> = std::result::Result<T, TtlCacheError>;

/// Cache entry with expiration.
#[derive(Clone)]
struct TtlEntry<V> {
    value: V,
    expires_at: Instant,
}

impl<V> TtlEntry<V> {
    fn new(value: V, ttl: Duration) -> Self {
        Self {
            value,
            expires_at: Instant::now() + ttl,
        }
    }

    fn is_expired(&self) -> bool {
        Instant::now() >= self.expires_at
    }

    fn remaining(&self) -> Duration {
        self.expires_at.saturating_duration_since(Instant::now())
    }
}

/// TTL cache.
pub struct TtlCache<K, V> {
    store: RwLock<HashMap<K, TtlEntry<V>>>,
    default_ttl: Duration,
}

impl<K: Eq + Hash + Clone, V: Clone> TtlCache<K, V> {
    /// Create new TTL cache with default TTL.
    pub fn new(default_ttl: Duration) -> Self {
        Self {
            store: RwLock::new(HashMap::new()),
            default_ttl,
        }
    }

    /// Get value if not expired.
    pub fn get(&self, key: &K) -> Option<V> {
        let store = self.store.read().unwrap();
        store.get(key).and_then(|entry| {
            if entry.is_expired() {
                None
            } else {
                Some(entry.value.clone())
            }
        })
    }

    /// Get value with remaining TTL.
    pub fn get_with_ttl(&self, key: &K) -> Option<(V, Duration)> {
        let store = self.store.read().unwrap();
        store.get(key).and_then(|entry| {
            if entry.is_expired() {
                None
            } else {
                Some((entry.value.clone(), entry.remaining()))
            }
        })
    }

    /// Set value with default TTL.
    pub fn set(&self, key: K, value: V) {
        self.set_with_ttl(key, value, self.default_ttl);
    }

    /// Set value with custom TTL.
    pub fn set_with_ttl(&self, key: K, value: V, ttl: Duration) {
        let mut store = self.store.write().unwrap();
        store.insert(key, TtlEntry::new(value, ttl));
    }

    /// Remove value.
    pub fn remove(&self, key: &K) -> Option<V> {
        let mut store = self.store.write().unwrap();
        store.remove(key).and_then(|entry| {
            if entry.is_expired() {
                None
            } else {
                Some(entry.value)
            }
        })
    }

    /// Check if key exists and is not expired.
    pub fn contains(&self, key: &K) -> bool {
        self.get(key).is_some()
    }

    /// Clear all entries.
    pub fn clear(&self) {
        let mut store = self.store.write().unwrap();
        store.clear();
    }

    /// Remove expired entries.
    pub fn cleanup(&self) -> usize {
        let mut store = self.store.write().unwrap();
        let before = store.len();
        store.retain(|_, entry| !entry.is_expired());
        before - store.len()
    }

    /// Get number of entries (including expired).
    pub fn len(&self) -> usize {
        let store = self.store.read().unwrap();
        store.len()
    }

    /// Get number of non-expired entries.
    pub fn active_len(&self) -> usize {
        let store = self.store.read().unwrap();
        store.values().filter(|e| !e.is_expired()).count()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get or insert with default TTL.
    pub fn get_or_insert<F>(&self, key: K, f: F) -> V
    where
        F: FnOnce() -> V,
    {
        self.get_or_insert_with_ttl(key, self.default_ttl, f)
    }

    /// Get or insert with custom TTL.
    pub fn get_or_insert_with_ttl<F>(&self, key: K, ttl: Duration, f: F) -> V
    where
        F: FnOnce() -> V,
    {
        // Try read first
        if let Some(value) = self.get(&key) {
            return value;
        }

        // Compute and insert
        let value = f();
        self.set_with_ttl(key, value.clone(), ttl);
        value
    }

    /// Refresh TTL for existing entry.
    pub fn refresh(&self, key: &K) -> bool {
        self.refresh_with_ttl(key, self.default_ttl)
    }

    /// Refresh with custom TTL.
    pub fn refresh_with_ttl(&self, key: &K, ttl: Duration) -> bool {
        let mut store = self.store.write().unwrap();
        if let Some(entry) = store.get_mut(key) {
            if !entry.is_expired() {
                entry.expires_at = Instant::now() + ttl;
                return true;
            }
        }
        false
    }
}

/// Cache entry info.
#[derive(Debug, Clone)]
pub struct EntryInfo {
    pub remaining_ttl: Duration,
    pub is_expired: bool,
}

impl<K: Eq + Hash + Clone, V: Clone> TtlCache<K, V> {
    /// Get entry info.
    pub fn entry_info(&self, key: &K) -> Option<EntryInfo> {
        let store = self.store.read().unwrap();
        store.get(key).map(|entry| EntryInfo {
            remaining_ttl: entry.remaining(),
            is_expired: entry.is_expired(),
        })
    }
}

/// Builder for TTL cache.
pub struct TtlCacheBuilder {
    default_ttl: Duration,
}

impl TtlCacheBuilder {
    /// Create new builder.
    pub fn new() -> Self {
        Self {
            default_ttl: Duration::from_secs(300), // 5 minutes
        }
    }

    /// Set default TTL.
    pub fn default_ttl(mut self, ttl: Duration) -> Self {
        self.default_ttl = ttl;
        self
    }

    /// Set TTL in seconds.
    pub fn ttl_secs(self, secs: u64) -> Self {
        self.default_ttl(Duration::from_secs(secs))
    }

    /// Set TTL in minutes.
    pub fn ttl_minutes(self, mins: u64) -> Self {
        self.default_ttl(Duration::from_secs(mins * 60))
    }

    /// Build cache.
    pub fn build<K: Eq + Hash + Clone, V: Clone>(self) -> TtlCache<K, V> {
        TtlCache::new(self.default_ttl)
    }
}

impl Default for TtlCacheBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ttl_cache_basic() {
        let cache = TtlCache::new(Duration::from_secs(60));

        cache.set("key", "value");
        assert_eq!(cache.get(&"key"), Some("value"));
        assert!(cache.contains(&"key"));
    }

    #[test]
    fn test_ttl_expiration() {
        let cache = TtlCache::new(Duration::from_millis(10));

        cache.set("key", "value");
        assert!(cache.get(&"key").is_some());

        std::thread::sleep(Duration::from_millis(20));
        assert!(cache.get(&"key").is_none());
    }

    #[test]
    fn test_custom_ttl() {
        let cache = TtlCache::new(Duration::from_secs(60));

        cache.set_with_ttl("short", "value", Duration::from_millis(10));
        cache.set_with_ttl("long", "value", Duration::from_secs(60));

        std::thread::sleep(Duration::from_millis(20));

        assert!(cache.get(&"short").is_none());
        assert!(cache.get(&"long").is_some());
    }

    #[test]
    fn test_cleanup() {
        let cache = TtlCache::new(Duration::from_millis(10));

        cache.set("key1", "value1");
        cache.set("key2", "value2");

        std::thread::sleep(Duration::from_millis(20));

        let removed = cache.cleanup();
        assert_eq!(removed, 2);
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn test_refresh() {
        let cache = TtlCache::new(Duration::from_millis(50));

        cache.set("key", "value");
        std::thread::sleep(Duration::from_millis(30));

        assert!(cache.refresh(&"key"));
        std::thread::sleep(Duration::from_millis(30));

        // Should still be valid due to refresh
        assert!(cache.get(&"key").is_some());
    }

    #[test]
    fn test_builder() {
        let cache: TtlCache<String, i32> = TtlCacheBuilder::new().ttl_minutes(5).build();

        cache.set("test".to_string(), 42);
        assert_eq!(cache.get(&"test".to_string()), Some(42));
    }
}
