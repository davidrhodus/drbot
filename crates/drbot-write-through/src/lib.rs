//! Write-through cache pattern for drbot.
//!
//! This crate provides:
//! - Synchronous write-through
//! - Async write-behind (write-back)
//! - Configurable write strategies

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::RwLock;
use thiserror::Error;

/// Write-through cache error types.
#[derive(Error, Debug, Clone)]
pub enum WriteThroughError {
    #[error("Store write failed: {0}")]
    WriteFailed(String),

    #[error("Store read failed: {0}")]
    ReadFailed(String),

    #[error("Key not found")]
    NotFound,
}

/// Result type for write-through operations.
pub type Result<T> = std::result::Result<T, WriteThroughError>;

/// Backing store trait.
pub trait BackingStore<K, V> {
    /// Read from store.
    fn read(&self, key: &K) -> Result<Option<V>>;

    /// Write to store.
    fn write(&self, key: &K, value: &V) -> Result<()>;

    /// Delete from store.
    fn delete(&self, key: &K) -> Result<()>;
}

/// In-memory backing store for testing.
pub struct MemoryStore<K, V> {
    data: RwLock<HashMap<K, V>>,
}

impl<K: Eq + Hash + Clone, V: Clone> MemoryStore<K, V> {
    /// Create new memory store.
    pub fn new() -> Self {
        Self {
            data: RwLock::new(HashMap::new()),
        }
    }
}

impl<K: Eq + Hash + Clone, V: Clone> Default for MemoryStore<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: Eq + Hash + Clone, V: Clone> BackingStore<K, V> for MemoryStore<K, V> {
    fn read(&self, key: &K) -> Result<Option<V>> {
        let data = self.data.read().unwrap();
        Ok(data.get(key).cloned())
    }

    fn write(&self, key: &K, value: &V) -> Result<()> {
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

/// Write-through cache.
pub struct WriteThroughCache<K, V, S> {
    cache: RwLock<HashMap<K, V>>,
    store: S,
}

impl<K: Eq + Hash + Clone, V: Clone, S: BackingStore<K, V>> WriteThroughCache<K, V, S> {
    /// Create new write-through cache.
    pub fn new(store: S) -> Self {
        Self {
            cache: RwLock::new(HashMap::new()),
            store,
        }
    }

    /// Get value (cache then store).
    pub fn get(&self, key: &K) -> Result<Option<V>> {
        // Check cache first
        {
            let cache = self.cache.read().unwrap();
            if let Some(value) = cache.get(key) {
                return Ok(Some(value.clone()));
            }
        }

        // Load from store
        if let Some(value) = self.store.read(key)? {
            let mut cache = self.cache.write().unwrap();
            cache.insert(key.clone(), value.clone());
            return Ok(Some(value));
        }

        Ok(None)
    }

    /// Set value (write to both cache and store).
    pub fn set(&self, key: K, value: V) -> Result<()> {
        // Write to store first
        self.store.write(&key, &value)?;

        // Then update cache
        let mut cache = self.cache.write().unwrap();
        cache.insert(key, value);

        Ok(())
    }

    /// Delete value.
    pub fn delete(&self, key: &K) -> Result<()> {
        // Delete from store first
        self.store.delete(key)?;

        // Then remove from cache
        let mut cache = self.cache.write().unwrap();
        cache.remove(key);

        Ok(())
    }

    /// Invalidate cache entry (keep store).
    pub fn invalidate(&self, key: &K) {
        let mut cache = self.cache.write().unwrap();
        cache.remove(key);
    }

    /// Clear cache (keep store).
    pub fn clear_cache(&self) {
        let mut cache = self.cache.write().unwrap();
        cache.clear();
    }

    /// Get cache size.
    pub fn cache_size(&self) -> usize {
        let cache = self.cache.read().unwrap();
        cache.len()
    }

    /// Refresh cache from store.
    pub fn refresh(&self, key: &K) -> Result<Option<V>> {
        self.invalidate(key);
        self.get(key)
    }
}

/// Write strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteStrategy {
    /// Write to store immediately on every write.
    WriteThrough,
    /// Buffer writes and flush periodically.
    WriteBack,
    /// Write to cache only, store on eviction.
    WriteAround,
}

/// Buffered write-back cache.
pub struct WriteBackCache<K, V, S> {
    cache: RwLock<HashMap<K, V>>,
    dirty: RwLock<std::collections::HashSet<K>>,
    store: S,
}

impl<K: Eq + Hash + Clone, V: Clone, S: BackingStore<K, V>> WriteBackCache<K, V, S> {
    /// Create new write-back cache.
    pub fn new(store: S) -> Self {
        Self {
            cache: RwLock::new(HashMap::new()),
            dirty: RwLock::new(std::collections::HashSet::new()),
            store,
        }
    }

    /// Get value.
    pub fn get(&self, key: &K) -> Result<Option<V>> {
        // Check cache
        {
            let cache = self.cache.read().unwrap();
            if let Some(value) = cache.get(key) {
                return Ok(Some(value.clone()));
            }
        }

        // Load from store
        if let Some(value) = self.store.read(key)? {
            let mut cache = self.cache.write().unwrap();
            cache.insert(key.clone(), value.clone());
            return Ok(Some(value));
        }

        Ok(None)
    }

    /// Set value (write to cache, mark dirty).
    pub fn set(&self, key: K, value: V) {
        let mut cache = self.cache.write().unwrap();
        let mut dirty = self.dirty.write().unwrap();

        cache.insert(key.clone(), value);
        dirty.insert(key);
    }

    /// Flush dirty entries to store.
    pub fn flush(&self) -> Result<usize> {
        let dirty_keys: Vec<K> = {
            let dirty = self.dirty.read().unwrap();
            dirty.iter().cloned().collect()
        };

        let cache = self.cache.read().unwrap();
        let mut flushed = 0;

        for key in &dirty_keys {
            if let Some(value) = cache.get(key) {
                self.store.write(key, value)?;
                flushed += 1;
            }
        }

        let mut dirty = self.dirty.write().unwrap();
        dirty.clear();

        Ok(flushed)
    }

    /// Get number of dirty entries.
    pub fn dirty_count(&self) -> usize {
        let dirty = self.dirty.read().unwrap();
        dirty.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_write_through() {
        let store = MemoryStore::new();
        let cache = WriteThroughCache::new(store);

        cache.set("key", "value").unwrap();
        assert_eq!(cache.get(&"key").unwrap(), Some("value"));
    }

    #[test]
    fn test_write_through_persistence() {
        let store = MemoryStore::new();
        let cache = WriteThroughCache::new(store);

        cache.set("key", "value").unwrap();
        cache.clear_cache();

        // Should still load from store
        assert_eq!(cache.get(&"key").unwrap(), Some("value"));
    }

    #[test]
    fn test_write_through_delete() {
        let store = MemoryStore::new();
        let cache = WriteThroughCache::new(store);

        cache.set("key", "value").unwrap();
        cache.delete(&"key").unwrap();

        assert_eq!(cache.get(&"key").unwrap(), None);
    }

    #[test]
    fn test_write_back() {
        let store = MemoryStore::new();
        let cache = WriteBackCache::new(store);

        cache.set("key", "value");
        assert_eq!(cache.dirty_count(), 1);

        cache.flush().unwrap();
        assert_eq!(cache.dirty_count(), 0);
    }

    #[test]
    fn test_write_back_read() {
        let store = MemoryStore::new();
        store.write(&"existing", &"from_store").unwrap();

        let cache = WriteBackCache::new(store);
        assert_eq!(cache.get(&"existing").unwrap(), Some("from_store"));
    }
}
