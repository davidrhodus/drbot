//! Object pool pattern for drbot.
//!
//! This crate provides:
//! - Object pool for reusable objects
//! - Automatic object recycling
//! - Pool statistics

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, Weak};
use thiserror::Error;

/// Pool error types.
#[derive(Error, Debug)]
pub enum PoolError {
    #[error("Pool exhausted")]
    Exhausted,

    #[error("Creation failed: {0}")]
    CreationFailed(String),

    #[error("Pool closed")]
    Closed,
}

/// Result type for pool operations.
pub type Result<T> = std::result::Result<T, PoolError>;

/// Resettable trait for pooled objects.
pub trait Resettable {
    /// Reset object to initial state.
    fn reset(&mut self);
}

/// Object pool configuration.
#[derive(Debug, Clone)]
pub struct PoolConfig {
    /// Initial pool size.
    pub initial_size: usize,
    /// Maximum pool size.
    pub max_size: usize,
    /// Create objects on demand.
    pub lazy: bool,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            initial_size: 0,
            max_size: 100,
            lazy: true,
        }
    }
}

/// Pool statistics.
#[derive(Debug, Clone, Default)]
pub struct PoolStats {
    /// Objects currently in pool.
    pub available: usize,
    /// Objects currently in use.
    pub in_use: usize,
    /// Total objects created.
    pub created: usize,
    /// Total times objects were reused.
    pub reused: usize,
}

/// Generic object pool.
pub struct ObjectPool<T> {
    objects: Mutex<VecDeque<T>>,
    creator: Box<dyn Fn() -> Result<T> + Send + Sync>,
    config: PoolConfig,
    stats: Mutex<PoolStats>,
    closed: std::sync::atomic::AtomicBool,
}

impl<T: Resettable> ObjectPool<T> {
    /// Create new pool with creator function.
    pub fn new<F>(creator: F, config: PoolConfig) -> Arc<Self>
    where
        F: Fn() -> Result<T> + Send + Sync + 'static,
    {
        let pool = Arc::new(Self {
            objects: Mutex::new(VecDeque::new()),
            creator: Box::new(creator),
            config: config.clone(),
            stats: Mutex::new(PoolStats::default()),
            closed: std::sync::atomic::AtomicBool::new(false),
        });

        // Pre-populate if not lazy
        if !config.lazy && config.initial_size > 0 {
            let mut objects = pool.objects.lock().unwrap();
            let mut stats = pool.stats.lock().unwrap();

            for _ in 0..config.initial_size {
                if let Ok(obj) = (pool.creator)() {
                    objects.push_back(obj);
                    stats.created += 1;
                    stats.available += 1;
                }
            }
        }

        pool
    }

    /// Acquire object from pool.
    pub fn acquire(self: &Arc<Self>) -> Result<PooledObject<T>> {
        if self.closed.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(PoolError::Closed);
        }

        let object = {
            let mut objects = self.objects.lock().unwrap();
            let mut stats = self.stats.lock().unwrap();

            if let Some(obj) = objects.pop_front() {
                stats.available -= 1;
                stats.in_use += 1;
                stats.reused += 1;
                Some(obj)
            } else if stats.created < self.config.max_size {
                // Create new object
                match (self.creator)() {
                    Ok(obj) => {
                        stats.created += 1;
                        stats.in_use += 1;
                        Some(obj)
                    }
                    Err(e) => return Err(e),
                }
            } else {
                None
            }
        };

        match object {
            Some(obj) => Ok(PooledObject {
                object: Some(obj),
                pool: Arc::downgrade(self),
            }),
            None => Err(PoolError::Exhausted),
        }
    }

    /// Return object to pool.
    fn release(&self, mut object: T) {
        if self.closed.load(std::sync::atomic::Ordering::SeqCst) {
            return;
        }

        object.reset();

        let mut objects = self.objects.lock().unwrap();
        let mut stats = self.stats.lock().unwrap();

        stats.in_use -= 1;
        stats.available += 1;
        objects.push_back(object);
    }

    /// Get pool statistics.
    pub fn stats(&self) -> PoolStats {
        self.stats.lock().unwrap().clone()
    }

    /// Get available count.
    pub fn available(&self) -> usize {
        self.objects.lock().unwrap().len()
    }

    /// Close pool (no more acquisitions).
    pub fn close(&self) {
        self.closed.store(true, std::sync::atomic::Ordering::SeqCst);
    }

    /// Check if closed.
    pub fn is_closed(&self) -> bool {
        self.closed.load(std::sync::atomic::Ordering::SeqCst)
    }
}

/// Handle to a pooled object.
pub struct PooledObject<T: Resettable> {
    object: Option<T>,
    pool: Weak<ObjectPool<T>>,
}

impl<T: Resettable> PooledObject<T> {
    /// Get reference to object.
    pub fn get(&self) -> &T {
        self.object.as_ref().unwrap()
    }

    /// Get mutable reference to object.
    pub fn get_mut(&mut self) -> &mut T {
        self.object.as_mut().unwrap()
    }
}

impl<T: Resettable> std::ops::Deref for PooledObject<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.get()
    }
}

impl<T: Resettable> std::ops::DerefMut for PooledObject<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.get_mut()
    }
}

impl<T: Resettable> Drop for PooledObject<T> {
    fn drop(&mut self) {
        if let Some(object) = self.object.take() {
            if let Some(pool) = self.pool.upgrade() {
                pool.release(object);
            }
        }
    }
}

/// Simple resettable buffer.
#[derive(Debug)]
pub struct Buffer {
    data: Vec<u8>,
    capacity: usize,
}

impl Buffer {
    /// Create new buffer.
    pub fn new(capacity: usize) -> Self {
        Self {
            data: Vec::with_capacity(capacity),
            capacity,
        }
    }

    /// Get data.
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Get mutable data.
    pub fn data_mut(&mut self) -> &mut Vec<u8> {
        &mut self.data
    }

    /// Get capacity.
    pub fn capacity(&self) -> usize {
        self.capacity
    }
}

impl Resettable for Buffer {
    fn reset(&mut self) {
        self.data.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Default)]
    struct TestObject {
        value: i32,
    }

    impl Resettable for TestObject {
        fn reset(&mut self) {
            self.value = 0;
        }
    }

    #[test]
    fn test_basic_pool() {
        let pool = ObjectPool::new(
            || Ok(TestObject::default()),
            PoolConfig {
                max_size: 10,
                ..Default::default()
            },
        );

        let mut obj = pool.acquire().unwrap();
        obj.value = 42;
        drop(obj);

        // Object should be reset and reused
        let obj2 = pool.acquire().unwrap();
        assert_eq!(obj2.value, 0);

        let stats = pool.stats();
        assert_eq!(stats.created, 1);
        assert_eq!(stats.reused, 1);
    }

    #[test]
    fn test_pool_exhaustion() {
        let pool = ObjectPool::new(
            || Ok(TestObject::default()),
            PoolConfig {
                max_size: 1,
                ..Default::default()
            },
        );

        let _obj1 = pool.acquire().unwrap();
        let result = pool.acquire();

        assert!(matches!(result, Err(PoolError::Exhausted)));
    }

    #[test]
    fn test_pool_return() {
        let pool = ObjectPool::new(
            || Ok(TestObject::default()),
            PoolConfig {
                max_size: 1,
                ..Default::default()
            },
        );

        {
            let _obj = pool.acquire().unwrap();
        }

        // Object returned, should be able to acquire again
        let _obj2 = pool.acquire().unwrap();
    }
}
