//! Memoization utilities for drbot.
//!
//! This crate provides:
//! - Memoized functions
//! - Cache policies
//! - Memoization helpers

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::RwLock;
use thiserror::Error;

/// Memoization error types.
#[derive(Error, Debug)]
pub enum MemoError {
    #[error("Cache miss")]
    CacheMiss,

    #[error("Computation failed: {0}")]
    ComputationFailed(String),
}

/// Result type for memo operations.
pub type Result<T> = std::result::Result<T, MemoError>;

/// Memoized function.
pub struct Memo<I, O, F>
where
    I: Eq + Hash + Clone,
    O: Clone,
    F: Fn(I) -> O,
{
    func: F,
    cache: RwLock<HashMap<I, O>>,
}

impl<I, O, F> Memo<I, O, F>
where
    I: Eq + Hash + Clone,
    O: Clone,
    F: Fn(I) -> O,
{
    /// Create new memoized function.
    pub fn new(func: F) -> Self {
        Self {
            func,
            cache: RwLock::new(HashMap::new()),
        }
    }

    /// Call the memoized function.
    pub fn call(&self, input: I) -> O {
        // Check cache
        {
            let cache = self.cache.read().unwrap();
            if let Some(output) = cache.get(&input) {
                return output.clone();
            }
        }

        // Compute and cache
        let output = (self.func)(input.clone());
        {
            let mut cache = self.cache.write().unwrap();
            cache.insert(input, output.clone());
        }
        output
    }

    /// Clear cache.
    pub fn clear(&self) {
        self.cache.write().unwrap().clear();
    }

    /// Get cache size.
    pub fn cache_size(&self) -> usize {
        self.cache.read().unwrap().len()
    }

    /// Invalidate specific entry.
    pub fn invalidate(&self, input: &I) {
        self.cache.write().unwrap().remove(input);
    }
}

/// Memoized function with LRU eviction.
pub struct LruMemo<I, O, F>
where
    I: Eq + Hash + Clone,
    O: Clone,
    F: Fn(I) -> O,
{
    func: F,
    cache: RwLock<HashMap<I, (O, u64)>>,
    order: RwLock<Vec<I>>,
    max_size: usize,
    counter: std::sync::atomic::AtomicU64,
}

impl<I, O, F> LruMemo<I, O, F>
where
    I: Eq + Hash + Clone,
    O: Clone,
    F: Fn(I) -> O,
{
    /// Create new LRU memoized function.
    pub fn new(func: F, max_size: usize) -> Self {
        Self {
            func,
            cache: RwLock::new(HashMap::new()),
            order: RwLock::new(Vec::new()),
            max_size,
            counter: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// Call the memoized function.
    pub fn call(&self, input: I) -> O {
        // Check cache
        {
            let cache = self.cache.read().unwrap();
            if let Some((output, _)) = cache.get(&input) {
                let output = output.clone();
                let time = self
                    .counter
                    .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                drop(cache);
                if let Ok(mut cache) = self.cache.write() {
                    if let Some(entry) = cache.get_mut(&input) {
                        entry.1 = time;
                    }
                }
                return output;
            }
        }

        // Compute
        let output = (self.func)(input.clone());
        let time = self
            .counter
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        // Evict if needed
        {
            let mut cache = self.cache.write().unwrap();
            if cache.len() >= self.max_size {
                // Find LRU entry
                let lru_key = cache
                    .iter()
                    .min_by_key(|(_, (_, t))| t)
                    .map(|(k, _)| k.clone());
                if let Some(key) = lru_key {
                    cache.remove(&key);
                }
            }
            cache.insert(input, (output.clone(), time));
        }

        output
    }

    /// Clear cache.
    pub fn clear(&self) {
        self.cache.write().unwrap().clear();
    }

    /// Get cache size.
    pub fn cache_size(&self) -> usize {
        self.cache.read().unwrap().len()
    }
}

/// Memoized function with TTL.
pub struct TtlMemo<I, O, F>
where
    I: Eq + Hash + Clone,
    O: Clone,
    F: Fn(I) -> O,
{
    func: F,
    cache: RwLock<HashMap<I, (O, std::time::Instant)>>,
    ttl: std::time::Duration,
}

impl<I, O, F> TtlMemo<I, O, F>
where
    I: Eq + Hash + Clone,
    O: Clone,
    F: Fn(I) -> O,
{
    /// Create new TTL memoized function.
    pub fn new(func: F, ttl: std::time::Duration) -> Self {
        Self {
            func,
            cache: RwLock::new(HashMap::new()),
            ttl,
        }
    }

    /// Call the memoized function.
    pub fn call(&self, input: I) -> O {
        let now = std::time::Instant::now();

        // Check cache
        {
            let cache = self.cache.read().unwrap();
            if let Some((output, created)) = cache.get(&input) {
                if now.duration_since(*created) < self.ttl {
                    return output.clone();
                }
            }
        }

        // Compute and cache
        let output = (self.func)(input.clone());
        {
            let mut cache = self.cache.write().unwrap();
            cache.insert(input, (output.clone(), now));
        }
        output
    }

    /// Clear expired entries.
    pub fn cleanup(&self) {
        let now = std::time::Instant::now();
        let mut cache = self.cache.write().unwrap();
        cache.retain(|_, (_, created)| now.duration_since(*created) < self.ttl);
    }

    /// Clear cache.
    pub fn clear(&self) {
        self.cache.write().unwrap().clear();
    }
}

/// Multi-argument memoization helper.
pub struct Memo2<I1, I2, O, F>
where
    I1: Eq + Hash + Clone,
    I2: Eq + Hash + Clone,
    O: Clone,
    F: Fn(I1, I2) -> O,
{
    func: F,
    cache: RwLock<HashMap<(I1, I2), O>>,
}

impl<I1, I2, O, F> Memo2<I1, I2, O, F>
where
    I1: Eq + Hash + Clone,
    I2: Eq + Hash + Clone,
    O: Clone,
    F: Fn(I1, I2) -> O,
{
    /// Create new 2-argument memoized function.
    pub fn new(func: F) -> Self {
        Self {
            func,
            cache: RwLock::new(HashMap::new()),
        }
    }

    /// Call the memoized function.
    pub fn call(&self, i1: I1, i2: I2) -> O {
        let key = (i1.clone(), i2.clone());

        // Check cache
        {
            let cache = self.cache.read().unwrap();
            if let Some(output) = cache.get(&key) {
                return output.clone();
            }
        }

        // Compute and cache
        let output = (self.func)(i1, i2);
        {
            let mut cache = self.cache.write().unwrap();
            cache.insert(key, output.clone());
        }
        output
    }

    /// Clear cache.
    pub fn clear(&self) {
        self.cache.write().unwrap().clear();
    }
}

/// Helper to create memoized function.
pub fn memoize<I, O, F>(func: F) -> Memo<I, O, F>
where
    I: Eq + Hash + Clone,
    O: Clone,
    F: Fn(I) -> O,
{
    Memo::new(func)
}

/// Helper to create LRU memoized function.
pub fn memoize_lru<I, O, F>(func: F, max_size: usize) -> LruMemo<I, O, F>
where
    I: Eq + Hash + Clone,
    O: Clone,
    F: Fn(I) -> O,
{
    LruMemo::new(func, max_size)
}

/// Helper to create TTL memoized function.
pub fn memoize_ttl<I, O, F>(func: F, ttl: std::time::Duration) -> TtlMemo<I, O, F>
where
    I: Eq + Hash + Clone,
    O: Clone,
    F: Fn(I) -> O,
{
    TtlMemo::new(func, ttl)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicI32, Ordering};

    #[test]
    fn test_memo() {
        let call_count = std::sync::Arc::new(AtomicI32::new(0));
        let cc = call_count.clone();

        let memo = memoize(move |x: i32| {
            cc.fetch_add(1, Ordering::SeqCst);
            x * 2
        });

        assert_eq!(memo.call(21), 42);
        assert_eq!(call_count.load(Ordering::SeqCst), 1);

        assert_eq!(memo.call(21), 42);
        assert_eq!(call_count.load(Ordering::SeqCst), 1); // Cached

        assert_eq!(memo.call(10), 20);
        assert_eq!(call_count.load(Ordering::SeqCst), 2); // New input
    }

    #[test]
    fn test_lru_memo() {
        let memo = memoize_lru(|x: i32| x * 2, 2);

        memo.call(1);
        memo.call(2);
        assert_eq!(memo.cache_size(), 2);

        memo.call(3); // Evicts 1
        assert_eq!(memo.cache_size(), 2);
    }

    #[test]
    fn test_memo2() {
        let memo = Memo2::new(|a: i32, b: i32| a + b);

        assert_eq!(memo.call(10, 32), 42);
        assert_eq!(memo.call(10, 32), 42); // Cached
    }
}
