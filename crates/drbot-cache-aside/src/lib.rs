//! Cache-aside pattern for drbot.
//!
//! This crate provides:
//! - Lazy loading cache
//! - Manual cache management
//! - Flexible caching strategies

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::RwLock;
use thiserror::Error;

/// Cache-aside error types.
#[derive(Error, Debug, Clone)]
pub enum CacheAsideError {
    #[error("Data source error: {0}")]
    SourceError(String),

    #[error("Cache error: {0}")]
    CacheError(String),

    #[error("Key not found")]
    NotFound,
}

/// Result type for cache-aside operations.
pub type Result<T> = std::result::Result<T, CacheAsideError>;

/// Data source trait.
pub trait DataSource<K, V> {
    /// Load data from source.
    fn load(&self, key: &K) -> Result<Option<V>>;

    /// Save data to source.
    fn save(&self, key: &K, value: &V) -> Result<()>;

    /// Delete from source.
    fn delete(&self, key: &K) -> Result<()>;
}

/// Cache-aside implementation.
pub struct CacheAside<K, V, S> {
    cache: RwLock<HashMap<K, V>>,
    source: S,
}

impl<K: Eq + Hash + Clone, V: Clone, S: DataSource<K, V>> CacheAside<K, V, S> {
    /// Create new cache-aside.
    pub fn new(source: S) -> Self {
        Self {
            cache: RwLock::new(HashMap::new()),
            source,
        }
    }

    /// Get value using cache-aside pattern.
    ///
    /// 1. Check cache
    /// 2. If miss, load from source
    /// 3. Store in cache
    /// 4. Return value
    pub fn get(&self, key: &K) -> Result<Option<V>> {
        // 1. Check cache
        {
            let cache = self.cache.read().unwrap();
            if let Some(value) = cache.get(key) {
                return Ok(Some(value.clone()));
            }
        }

        // 2. Load from source
        let value = self.source.load(key)?;

        // 3. Store in cache if found
        if let Some(ref v) = value {
            let mut cache = self.cache.write().unwrap();
            cache.insert(key.clone(), v.clone());
        }

        // 4. Return
        Ok(value)
    }

    /// Update value (write-through to source, invalidate cache).
    pub fn update(&self, key: K, value: V) -> Result<()> {
        // Write to source
        self.source.save(&key, &value)?;

        // Update cache
        let mut cache = self.cache.write().unwrap();
        cache.insert(key, value);

        Ok(())
    }

    /// Delete value.
    pub fn delete(&self, key: &K) -> Result<()> {
        // Delete from source
        self.source.delete(key)?;

        // Remove from cache
        let mut cache = self.cache.write().unwrap();
        cache.remove(key);

        Ok(())
    }

    /// Invalidate cache entry.
    pub fn invalidate(&self, key: &K) {
        let mut cache = self.cache.write().unwrap();
        cache.remove(key);
    }

    /// Invalidate all cache entries.
    pub fn invalidate_all(&self) {
        let mut cache = self.cache.write().unwrap();
        cache.clear();
    }

    /// Prefetch values into cache.
    pub fn prefetch(&self, keys: &[K]) -> Result<()> {
        for key in keys {
            let _ = self.get(key)?;
        }
        Ok(())
    }

    /// Check if cached.
    pub fn is_cached(&self, key: &K) -> bool {
        let cache = self.cache.read().unwrap();
        cache.contains_key(key)
    }

    /// Get cache size.
    pub fn cache_size(&self) -> usize {
        let cache = self.cache.read().unwrap();
        cache.len()
    }

    /// Get or compute with custom loader.
    pub fn get_or_load<F>(&self, key: &K, loader: F) -> Result<V>
    where
        F: FnOnce(&K) -> Result<V>,
    {
        // Check cache
        {
            let cache = self.cache.read().unwrap();
            if let Some(value) = cache.get(key) {
                return Ok(value.clone());
            }
        }

        // Load with custom loader
        let value = loader(key)?;

        // Cache it
        let mut cache = self.cache.write().unwrap();
        cache.insert(key.clone(), value.clone());

        Ok(value)
    }
}

/// In-memory data source for testing.
pub struct MemorySource<K, V> {
    data: RwLock<HashMap<K, V>>,
}

impl<K: Eq + Hash + Clone, V: Clone> MemorySource<K, V> {
    /// Create new memory source.
    pub fn new() -> Self {
        Self {
            data: RwLock::new(HashMap::new()),
        }
    }

    /// Preload data.
    pub fn preload(&self, key: K, value: V) {
        let mut data = self.data.write().unwrap();
        data.insert(key, value);
    }
}

impl<K: Eq + Hash + Clone, V: Clone> Default for MemorySource<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: Eq + Hash + Clone, V: Clone> DataSource<K, V> for MemorySource<K, V> {
    fn load(&self, key: &K) -> Result<Option<V>> {
        let data = self.data.read().unwrap();
        Ok(data.get(key).cloned())
    }

    fn save(&self, key: &K, value: &V) -> Result<()> {
        let mut data = self.data.write().unwrap();
        data.insert(key.clone(), value.clone());
        Ok(())
    }

    fn delete(&self, key: &K) -> Result<()> {
        let mut data = self.data.write().unwrap();
        data.remove(key);
        Ok(())
    }
}

/// Function-based data source.
pub struct FnSource<K, V, L, S, D>
where
    L: Fn(&K) -> Result<Option<V>>,
    S: Fn(&K, &V) -> Result<()>,
    D: Fn(&K) -> Result<()>,
{
    load_fn: L,
    save_fn: S,
    delete_fn: D,
    _marker: std::marker::PhantomData<(K, V)>,
}

impl<K, V, L, S, D> FnSource<K, V, L, S, D>
where
    L: Fn(&K) -> Result<Option<V>>,
    S: Fn(&K, &V) -> Result<()>,
    D: Fn(&K) -> Result<()>,
{
    /// Create new function source.
    pub fn new(load_fn: L, save_fn: S, delete_fn: D) -> Self {
        Self {
            load_fn,
            save_fn,
            delete_fn,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<K, V, L, S, D> DataSource<K, V> for FnSource<K, V, L, S, D>
where
    L: Fn(&K) -> Result<Option<V>>,
    S: Fn(&K, &V) -> Result<()>,
    D: Fn(&K) -> Result<()>,
{
    fn load(&self, key: &K) -> Result<Option<V>> {
        (self.load_fn)(key)
    }

    fn save(&self, key: &K, value: &V) -> Result<()> {
        (self.save_fn)(key, value)
    }

    fn delete(&self, key: &K) -> Result<()> {
        (self.delete_fn)(key)
    }
}

/// Cache-aside statistics.
#[derive(Debug, Clone, Default)]
pub struct CacheAsideStats {
    pub hits: u64,
    pub misses: u64,
    pub cache_size: usize,
}

impl CacheAsideStats {
    /// Calculate hit rate.
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[test]
    fn test_cache_aside_basic() {
        let source = MemorySource::new();
        source.preload("key", "value");

        let cache = CacheAside::new(source);

        assert_eq!(cache.get(&"key").unwrap(), Some("value"));
        assert!(cache.is_cached(&"key"));
    }

    #[test]
    fn test_cache_aside_caching() {
        let load_count = std::sync::Arc::new(AtomicU32::new(0));
        let lc = load_count.clone();

        let source = FnSource::new(
            move |_: &&str| {
                lc.fetch_add(1, Ordering::SeqCst);
                Ok(Some("value"))
            },
            |_, _| Ok(()),
            |_| Ok(()),
        );

        let cache = CacheAside::new(source);

        cache.get(&"key").unwrap();
        cache.get(&"key").unwrap();
        cache.get(&"key").unwrap();

        // Should only load once
        assert_eq!(load_count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_invalidation() {
        let source = MemorySource::new();
        source.preload("key", "value");

        let cache = CacheAside::new(source);

        cache.get(&"key").unwrap();
        assert!(cache.is_cached(&"key"));

        cache.invalidate(&"key");
        assert!(!cache.is_cached(&"key"));
    }

    #[test]
    fn test_update() {
        let source = MemorySource::new();
        let cache = CacheAside::new(source);

        cache.update("key", "value").unwrap();
        assert_eq!(cache.get(&"key").unwrap(), Some("value"));
    }

    #[test]
    fn test_prefetch() {
        let source = MemorySource::new();
        source.preload("a", 1);
        source.preload("b", 2);
        source.preload("c", 3);

        let cache = CacheAside::new(source);

        cache.prefetch(&["a", "b", "c"]).unwrap();

        assert!(cache.is_cached(&"a"));
        assert!(cache.is_cached(&"b"));
        assert!(cache.is_cached(&"c"));
    }
}
