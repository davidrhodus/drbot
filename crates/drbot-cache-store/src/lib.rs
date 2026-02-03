//! Generic cache store for drbot.
//!
//! This crate provides:
//! - Cache trait abstraction
//! - In-memory cache implementation
//! - Cache statistics

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::RwLock;
use thiserror::Error;

/// Cache error types.
#[derive(Error, Debug, Clone)]
pub enum CacheError {
    #[error("Key not found")]
    NotFound,

    #[error("Cache full")]
    Full,

    #[error("Cache error: {0}")]
    Error(String),
}

/// Result type for cache operations.
pub type Result<T> = std::result::Result<T, CacheError>;

/// Cache trait.
pub trait Cache<K, V> {
    /// Get value by key.
    fn get(&self, key: &K) -> Option<V>;

    /// Set value.
    fn set(&self, key: K, value: V);

    /// Remove value.
    fn remove(&self, key: &K) -> Option<V>;

    /// Check if key exists.
    fn contains(&self, key: &K) -> bool;

    /// Clear all entries.
    fn clear(&self);

    /// Get number of entries.
    fn len(&self) -> usize;

    /// Check if empty.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Simple in-memory cache.
pub struct MemoryCache<K, V> {
    store: RwLock<HashMap<K, V>>,
    max_size: Option<usize>,
}

impl<K: Eq + Hash + Clone, V: Clone> MemoryCache<K, V> {
    /// Create new unbounded cache.
    pub fn new() -> Self {
        Self {
            store: RwLock::new(HashMap::new()),
            max_size: None,
        }
    }

    /// Create bounded cache.
    pub fn with_capacity(max_size: usize) -> Self {
        Self {
            store: RwLock::new(HashMap::with_capacity(max_size)),
            max_size: Some(max_size),
        }
    }

    /// Get or insert value.
    pub fn get_or_insert<F>(&self, key: K, f: F) -> V
    where
        F: FnOnce() -> V,
    {
        // Try read first
        {
            let store = self.store.read().unwrap();
            if let Some(value) = store.get(&key) {
                return value.clone();
            }
        }

        // Insert if not found
        let mut store = self.store.write().unwrap();
        store.entry(key).or_insert_with(f).clone()
    }

    /// Get or insert with result.
    pub fn get_or_try_insert<F, E>(&self, key: K, f: F) -> std::result::Result<V, E>
    where
        F: FnOnce() -> std::result::Result<V, E>,
    {
        // Try read first
        {
            let store = self.store.read().unwrap();
            if let Some(value) = store.get(&key) {
                return Ok(value.clone());
            }
        }

        // Compute and insert
        let value = f()?;
        let mut store = self.store.write().unwrap();
        store.insert(key, value.clone());
        Ok(value)
    }

    /// Get all keys.
    pub fn keys(&self) -> Vec<K> {
        let store = self.store.read().unwrap();
        store.keys().cloned().collect()
    }

    /// Get all values.
    pub fn values(&self) -> Vec<V> {
        let store = self.store.read().unwrap();
        store.values().cloned().collect()
    }
}

impl<K: Eq + Hash + Clone, V: Clone> Default for MemoryCache<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: Eq + Hash + Clone, V: Clone> Cache<K, V> for MemoryCache<K, V> {
    fn get(&self, key: &K) -> Option<V> {
        let store = self.store.read().unwrap();
        store.get(key).cloned()
    }

    fn set(&self, key: K, value: V) {
        let mut store = self.store.write().unwrap();

        // Check max size
        if let Some(max) = self.max_size {
            if store.len() >= max && !store.contains_key(&key) {
                // Remove arbitrary entry (simple eviction)
                if let Some(k) = store.keys().next().cloned() {
                    store.remove(&k);
                }
            }
        }

        store.insert(key, value);
    }

    fn remove(&self, key: &K) -> Option<V> {
        let mut store = self.store.write().unwrap();
        store.remove(key)
    }

    fn contains(&self, key: &K) -> bool {
        let store = self.store.read().unwrap();
        store.contains_key(key)
    }

    fn clear(&self) {
        let mut store = self.store.write().unwrap();
        store.clear();
    }

    fn len(&self) -> usize {
        let store = self.store.read().unwrap();
        store.len()
    }
}

/// Cache with statistics.
pub struct StatsCache<K, V, C: Cache<K, V>> {
    inner: C,
    hits: std::sync::atomic::AtomicU64,
    misses: std::sync::atomic::AtomicU64,
    _marker: std::marker::PhantomData<(K, V)>,
}

impl<K, V, C: Cache<K, V>> StatsCache<K, V, C> {
    /// Create new stats cache.
    pub fn new(cache: C) -> Self {
        Self {
            inner: cache,
            hits: std::sync::atomic::AtomicU64::new(0),
            misses: std::sync::atomic::AtomicU64::new(0),
            _marker: std::marker::PhantomData,
        }
    }

    /// Get hit count.
    pub fn hits(&self) -> u64 {
        self.hits.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Get miss count.
    pub fn misses(&self) -> u64 {
        self.misses.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Get hit rate.
    pub fn hit_rate(&self) -> f64 {
        let hits = self.hits() as f64;
        let total = hits + self.misses() as f64;
        if total == 0.0 {
            0.0
        } else {
            hits / total
        }
    }

    /// Reset statistics.
    pub fn reset_stats(&self) {
        self.hits.store(0, std::sync::atomic::Ordering::Relaxed);
        self.misses.store(0, std::sync::atomic::Ordering::Relaxed);
    }

    /// Get statistics.
    pub fn stats(&self) -> CacheStats {
        CacheStats {
            hits: self.hits(),
            misses: self.misses(),
            hit_rate: self.hit_rate(),
            size: self.inner.len(),
        }
    }
}

impl<K, V, C: Cache<K, V>> Cache<K, V> for StatsCache<K, V, C> {
    fn get(&self, key: &K) -> Option<V> {
        match self.inner.get(key) {
            Some(v) => {
                self.hits.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                Some(v)
            }
            None => {
                self.misses
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                None
            }
        }
    }

    fn set(&self, key: K, value: V) {
        self.inner.set(key, value);
    }

    fn remove(&self, key: &K) -> Option<V> {
        self.inner.remove(key)
    }

    fn contains(&self, key: &K) -> bool {
        self.inner.contains(key)
    }

    fn clear(&self) {
        self.inner.clear();
    }

    fn len(&self) -> usize {
        self.inner.len()
    }
}

/// Cache statistics.
#[derive(Debug, Clone)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub hit_rate: f64,
    pub size: usize,
}

/// Null cache (no-op).
pub struct NullCache<K, V>(std::marker::PhantomData<(K, V)>);

impl<K, V> NullCache<K, V> {
    /// Create new null cache.
    pub fn new() -> Self {
        Self(std::marker::PhantomData)
    }
}

impl<K, V> Default for NullCache<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K, V> Cache<K, V> for NullCache<K, V> {
    fn get(&self, _key: &K) -> Option<V> {
        None
    }

    fn set(&self, _key: K, _value: V) {}

    fn remove(&self, _key: &K) -> Option<V> {
        None
    }

    fn contains(&self, _key: &K) -> bool {
        false
    }

    fn clear(&self) {}

    fn len(&self) -> usize {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_cache() {
        let cache = MemoryCache::new();

        cache.set("key1", "value1");
        assert_eq!(cache.get(&"key1"), Some("value1"));
        assert!(cache.contains(&"key1"));
        assert_eq!(cache.len(), 1);

        cache.remove(&"key1");
        assert!(cache.get(&"key1").is_none());
    }

    #[test]
    fn test_get_or_insert() {
        let cache = MemoryCache::new();

        let value = cache.get_or_insert("key", || "computed");
        assert_eq!(value, "computed");

        cache.set("key", "updated");
        let value = cache.get_or_insert("key", || "computed");
        assert_eq!(value, "updated");
    }

    #[test]
    fn test_bounded_cache() {
        let cache = MemoryCache::<i32, i32>::with_capacity(2);

        cache.set(1, 100);
        cache.set(2, 200);
        cache.set(3, 300);

        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn test_stats_cache() {
        let cache = StatsCache::new(MemoryCache::new());

        cache.set("key", "value");
        cache.get(&"key"); // Hit
        cache.get(&"missing"); // Miss

        assert_eq!(cache.hits(), 1);
        assert_eq!(cache.misses(), 1);
        assert_eq!(cache.hit_rate(), 0.5);
    }
}
