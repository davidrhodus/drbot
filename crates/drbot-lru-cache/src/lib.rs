//! LRU cache for drbot.
//!
//! This crate provides:
//! - Least Recently Used eviction
//! - Bounded capacity
//! - Access ordering

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::RwLock;
use thiserror::Error;

/// LRU cache error types.
#[derive(Error, Debug, Clone)]
pub enum LruCacheError {
    #[error("Key not found")]
    NotFound,

    #[error("Cache is empty")]
    Empty,
}

/// Result type for LRU cache operations.
pub type Result<T> = std::result::Result<T, LruCacheError>;

/// LRU cache entry.
struct LruEntry<K, V> {
    key: K,
    value: V,
    prev: Option<usize>,
    next: Option<usize>,
}

/// LRU cache implementation.
pub struct LruCache<K, V> {
    capacity: usize,
    entries: RwLock<LruCacheInner<K, V>>,
}

struct LruCacheInner<K, V> {
    map: HashMap<K, usize>,
    entries: Vec<Option<LruEntry<K, V>>>,
    head: Option<usize>,
    tail: Option<usize>,
    free: Vec<usize>,
}

impl<K: Eq + Hash + Clone, V: Clone> LruCache<K, V> {
    /// Create new LRU cache with capacity.
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "Capacity must be positive");
        Self {
            capacity,
            entries: RwLock::new(LruCacheInner {
                map: HashMap::with_capacity(capacity),
                entries: Vec::with_capacity(capacity),
                head: None,
                tail: None,
                free: Vec::new(),
            }),
        }
    }

    /// Get value and mark as recently used.
    pub fn get(&self, key: &K) -> Option<V> {
        let mut inner = self.entries.write().unwrap();

        let idx = *inner.map.get(key)?;
        let value = inner.entries[idx].as_ref()?.value.clone();

        // Move to head
        Self::move_to_head(&mut inner, idx);

        Some(value)
    }

    /// Peek value without updating recency.
    pub fn peek(&self, key: &K) -> Option<V> {
        let inner = self.entries.read().unwrap();
        let idx = *inner.map.get(key)?;
        inner.entries[idx].as_ref().map(|e| e.value.clone())
    }

    /// Set value.
    pub fn set(&self, key: K, value: V) -> Option<V> {
        let mut inner = self.entries.write().unwrap();

        // Check if key exists
        if let Some(&idx) = inner.map.get(&key) {
            let old_value = inner.entries[idx].as_ref().map(|e| e.value.clone());
            if let Some(entry) = inner.entries[idx].as_mut() {
                entry.value = value;
            }
            Self::move_to_head(&mut inner, idx);
            return old_value;
        }

        // Evict if at capacity
        if inner.map.len() >= self.capacity {
            Self::evict_lru(&mut inner);
        }

        // Get index for new entry
        let idx = if let Some(free_idx) = inner.free.pop() {
            free_idx
        } else {
            let idx = inner.entries.len();
            inner.entries.push(None);
            idx
        };

        // Create entry
        let entry = LruEntry {
            key: key.clone(),
            value,
            prev: None,
            next: inner.head,
        };

        // Update head's prev
        if let Some(head_idx) = inner.head {
            if let Some(head) = inner.entries[head_idx].as_mut() {
                head.prev = Some(idx);
            }
        }

        inner.entries[idx] = Some(entry);
        inner.map.insert(key, idx);

        // Update head
        inner.head = Some(idx);
        if inner.tail.is_none() {
            inner.tail = Some(idx);
        }

        None
    }

    /// Remove value.
    pub fn remove(&self, key: &K) -> Option<V> {
        let mut inner = self.entries.write().unwrap();

        let idx = inner.map.remove(key)?;
        Self::unlink(&mut inner, idx);
        let entry = inner.entries[idx].take()?;
        inner.free.push(idx);

        Some(entry.value)
    }

    /// Check if key exists.
    pub fn contains(&self, key: &K) -> bool {
        let inner = self.entries.read().unwrap();
        inner.map.contains_key(key)
    }

    /// Get number of entries.
    pub fn len(&self) -> usize {
        let inner = self.entries.read().unwrap();
        inner.map.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get capacity.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Clear all entries.
    pub fn clear(&self) {
        let mut inner = self.entries.write().unwrap();
        inner.map.clear();
        inner.entries.clear();
        inner.head = None;
        inner.tail = None;
        inner.free.clear();
    }

    /// Get most recently used key.
    pub fn mru(&self) -> Option<K> {
        let inner = self.entries.read().unwrap();
        inner
            .head
            .and_then(|idx| inner.entries[idx].as_ref().map(|e| e.key.clone()))
    }

    /// Get least recently used key.
    pub fn lru(&self) -> Option<K> {
        let inner = self.entries.read().unwrap();
        inner
            .tail
            .and_then(|idx| inner.entries[idx].as_ref().map(|e| e.key.clone()))
    }

    fn move_to_head(inner: &mut LruCacheInner<K, V>, idx: usize) {
        if inner.head == Some(idx) {
            return;
        }

        Self::unlink(inner, idx);

        // Link at head
        if let Some(entry) = inner.entries[idx].as_mut() {
            entry.prev = None;
            entry.next = inner.head;
        }

        if let Some(head_idx) = inner.head {
            if let Some(head) = inner.entries[head_idx].as_mut() {
                head.prev = Some(idx);
            }
        }

        inner.head = Some(idx);
        if inner.tail.is_none() {
            inner.tail = Some(idx);
        }
    }

    fn unlink(inner: &mut LruCacheInner<K, V>, idx: usize) {
        let (prev, next) = {
            let entry = inner.entries[idx].as_ref().unwrap();
            (entry.prev, entry.next)
        };

        if let Some(prev_idx) = prev {
            if let Some(prev_entry) = inner.entries[prev_idx].as_mut() {
                prev_entry.next = next;
            }
        } else {
            inner.head = next;
        }

        if let Some(next_idx) = next {
            if let Some(next_entry) = inner.entries[next_idx].as_mut() {
                next_entry.prev = prev;
            }
        } else {
            inner.tail = prev;
        }
    }

    fn evict_lru(inner: &mut LruCacheInner<K, V>) {
        let Some(tail_idx) = inner.tail else {
            return;
        };

        if inner
            .entries
            .get(tail_idx)
            .and_then(|e| e.as_ref())
            .is_none()
        {
            return;
        }

        Self::unlink(inner, tail_idx);

        if let Some(entry) = inner.entries[tail_idx].take() {
            inner.map.remove(&entry.key);
            inner.free.push(tail_idx);
        }
    }
}

/// Get or insert helper.
impl<K: Eq + Hash + Clone, V: Clone> LruCache<K, V> {
    /// Get or insert value.
    pub fn get_or_insert<F>(&self, key: K, f: F) -> V
    where
        F: FnOnce() -> V,
    {
        if let Some(value) = self.get(&key) {
            return value;
        }

        let value = f();
        self.set(key, value.clone());
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lru_basic() {
        let cache = LruCache::new(3);

        cache.set("a", 1);
        cache.set("b", 2);
        cache.set("c", 3);

        assert_eq!(cache.get(&"a"), Some(1));
        assert_eq!(cache.get(&"b"), Some(2));
        assert_eq!(cache.get(&"c"), Some(3));
    }

    #[test]
    fn test_lru_eviction() {
        let cache = LruCache::new(2);

        cache.set("a", 1);
        cache.set("b", 2);
        cache.set("c", 3); // Should evict "a"

        assert!(cache.get(&"a").is_none());
        assert_eq!(cache.get(&"b"), Some(2));
        assert_eq!(cache.get(&"c"), Some(3));
    }

    #[test]
    fn test_lru_access_order() {
        let cache = LruCache::new(2);

        cache.set("a", 1);
        cache.set("b", 2);
        cache.get(&"a"); // Access "a", making "b" LRU
        cache.set("c", 3); // Should evict "b"

        assert_eq!(cache.get(&"a"), Some(1));
        assert!(cache.get(&"b").is_none());
        assert_eq!(cache.get(&"c"), Some(3));
    }

    #[test]
    fn test_mru_lru() {
        let cache = LruCache::new(3);

        cache.set("a", 1);
        cache.set("b", 2);
        cache.set("c", 3);

        assert_eq!(cache.mru(), Some("c"));
        assert_eq!(cache.lru(), Some("a"));

        cache.get(&"a");
        assert_eq!(cache.mru(), Some("a"));
    }

    #[test]
    fn test_remove() {
        let cache = LruCache::new(3);

        cache.set("a", 1);
        cache.set("b", 2);

        assert_eq!(cache.remove(&"a"), Some(1));
        assert!(cache.get(&"a").is_none());
        assert_eq!(cache.len(), 1);
    }
}
