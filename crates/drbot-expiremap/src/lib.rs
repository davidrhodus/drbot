//! Map with expiring entries for drbot.
//!
//! This crate provides:
//! - ExpireMap with TTL-based expiration
//! - Lazy and active expiration modes
//! - Expiration callbacks

use std::collections::HashMap;
use std::hash::Hash;
use std::time::{Duration, Instant};
use thiserror::Error;

/// ExpireMap error types.
#[derive(Error, Debug)]
pub enum ExpireMapError {
    #[error("Key not found")]
    KeyNotFound,

    #[error("Key expired")]
    Expired,
}

/// Result type for expiremap operations.
pub type Result<T> = std::result::Result<T, ExpireMapError>;

/// Entry with expiration.
#[derive(Debug, Clone)]
struct Entry<V> {
    value: V,
    expires_at: Instant,
}

impl<V> Entry<V> {
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

/// Map with time-based expiration.
#[derive(Debug)]
pub struct ExpireMap<K, V> {
    entries: HashMap<K, Entry<V>>,
    default_ttl: Duration,
}

impl<K: Hash + Eq + Clone, V> ExpireMap<K, V> {
    /// Create new expire map with default TTL.
    pub fn new(default_ttl: Duration) -> Self {
        Self {
            entries: HashMap::new(),
            default_ttl,
        }
    }

    /// Insert with default TTL.
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        self.insert_with_ttl(key, value, self.default_ttl)
    }

    /// Insert with custom TTL.
    pub fn insert_with_ttl(&mut self, key: K, value: V, ttl: Duration) -> Option<V> {
        let old = self.entries.insert(key, Entry::new(value, ttl));
        old.map(|e| e.value)
    }

    /// Get value if not expired.
    pub fn get(&self, key: &K) -> Option<&V> {
        self.entries.get(key).and_then(|entry| {
            if entry.is_expired() {
                None
            } else {
                Some(&entry.value)
            }
        })
    }

    /// Get mutable value if not expired.
    pub fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        self.entries.get_mut(key).and_then(|entry| {
            if entry.is_expired() {
                None
            } else {
                Some(&mut entry.value)
            }
        })
    }

    /// Get remaining time for key.
    pub fn get_ttl(&self, key: &K) -> Option<Duration> {
        self.entries.get(key).map(|e| e.remaining())
    }

    /// Check if key exists and is not expired.
    pub fn contains_key(&self, key: &K) -> bool {
        self.get(key).is_some()
    }

    /// Refresh TTL for existing key.
    pub fn refresh(&mut self, key: &K) -> bool {
        self.refresh_with_ttl(key, self.default_ttl)
    }

    /// Refresh with custom TTL.
    pub fn refresh_with_ttl(&mut self, key: &K, ttl: Duration) -> bool {
        if let Some(entry) = self.entries.get_mut(key) {
            entry.expires_at = Instant::now() + ttl;
            true
        } else {
            false
        }
    }

    /// Remove entry.
    pub fn remove(&mut self, key: &K) -> Option<V> {
        self.entries.remove(key).map(|e| e.value)
    }

    /// Remove expired entries.
    pub fn cleanup(&mut self) -> usize {
        let before = self.entries.len();
        self.entries.retain(|_, entry| !entry.is_expired());
        before - self.entries.len()
    }

    /// Get number of entries (including expired).
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Get number of non-expired entries.
    pub fn len_valid(&self) -> usize {
        self.entries.values().filter(|e| !e.is_expired()).count()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Clear all entries.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Iterate over non-expired entries.
    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.entries
            .iter()
            .filter(|(_, e)| !e.is_expired())
            .map(|(k, e)| (k, &e.value))
    }

    /// Iterate over keys of non-expired entries.
    pub fn keys(&self) -> impl Iterator<Item = &K> {
        self.entries
            .iter()
            .filter(|(_, e)| !e.is_expired())
            .map(|(k, _)| k)
    }

    /// Get all expired keys.
    pub fn expired_keys(&self) -> Vec<K> {
        self.entries
            .iter()
            .filter(|(_, e)| e.is_expired())
            .map(|(k, _)| k.clone())
            .collect()
    }
}

impl<K: Hash + Eq + Clone, V> Default for ExpireMap<K, V> {
    fn default() -> Self {
        Self::new(Duration::from_secs(60))
    }
}

/// Builder for ExpireMap.
pub struct ExpireMapBuilder<K, V> {
    default_ttl: Duration,
    initial_entries: Vec<(K, V, Option<Duration>)>,
}

impl<K: Hash + Eq + Clone, V> ExpireMapBuilder<K, V> {
    /// Create new builder.
    pub fn new() -> Self {
        Self {
            default_ttl: Duration::from_secs(60),
            initial_entries: Vec::new(),
        }
    }

    /// Set default TTL.
    pub fn default_ttl(mut self, ttl: Duration) -> Self {
        self.default_ttl = ttl;
        self
    }

    /// Add initial entry.
    pub fn entry(mut self, key: K, value: V) -> Self {
        self.initial_entries.push((key, value, None));
        self
    }

    /// Add initial entry with custom TTL.
    pub fn entry_with_ttl(mut self, key: K, value: V, ttl: Duration) -> Self {
        self.initial_entries.push((key, value, Some(ttl)));
        self
    }

    /// Build the map.
    pub fn build(self) -> ExpireMap<K, V> {
        let mut map = ExpireMap::new(self.default_ttl);
        for (key, value, ttl) in self.initial_entries {
            let ttl = ttl.unwrap_or(self.default_ttl);
            map.insert_with_ttl(key, value, ttl);
        }
        map
    }
}

impl<K: Hash + Eq + Clone, V> Default for ExpireMapBuilder<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_basic_operations() {
        let mut map = ExpireMap::new(Duration::from_secs(60));
        map.insert("key", "value");

        assert_eq!(map.get(&"key"), Some(&"value"));
        assert!(map.contains_key(&"key"));
    }

    #[test]
    fn test_expiration() {
        let mut map = ExpireMap::new(Duration::from_millis(50));
        map.insert("key", "value");

        assert!(map.contains_key(&"key"));

        thread::sleep(Duration::from_millis(60));

        assert!(!map.contains_key(&"key"));
    }

    #[test]
    fn test_refresh() {
        let mut map = ExpireMap::new(Duration::from_millis(50));
        map.insert("key", "value");

        thread::sleep(Duration::from_millis(30));
        map.refresh(&"key");

        thread::sleep(Duration::from_millis(30));
        assert!(map.contains_key(&"key")); // Still valid after refresh
    }

    #[test]
    fn test_cleanup() {
        let mut map = ExpireMap::new(Duration::from_millis(10));
        map.insert("a", 1);
        map.insert("b", 2);

        thread::sleep(Duration::from_millis(20));
        map.insert("c", 3); // This one is fresh

        let removed = map.cleanup();
        assert_eq!(removed, 2);
        assert_eq!(map.len(), 1);
    }

    #[test]
    fn test_builder() {
        let map: ExpireMap<&str, i32> = ExpireMapBuilder::new()
            .default_ttl(Duration::from_secs(30))
            .entry("a", 1)
            .entry("b", 2)
            .build();

        assert_eq!(map.len(), 2);
    }
}
