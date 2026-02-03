//! Resource reclamation utilities for drbot.
//!
//! This crate provides:
//! - Resource reclamation
//! - Object pooling
//! - Memory reclamation

use std::collections::VecDeque;
use thiserror::Error;

/// Reclaim error types.
#[derive(Error, Debug, Clone)]
pub enum ReclaimError {
    #[error("Reclamation failed: {0}")]
    Failed(String),

    #[error("Pool exhausted")]
    PoolExhausted,

    #[error("Invalid resource")]
    Invalid,
}

/// Result type for reclaim operations.
pub type Result<T> = std::result::Result<T, ReclaimError>;

/// Reclaimable trait.
pub trait Reclaimable {
    /// Reset for reuse.
    fn reset(&mut self);

    /// Check if reusable.
    fn is_reusable(&self) -> bool {
        true
    }
}

/// Reclaim pool.
pub struct ReclaimPool<T: Reclaimable> {
    pool: VecDeque<T>,
    max_size: usize,
}

impl<T: Reclaimable> ReclaimPool<T> {
    /// Create new pool.
    pub fn new(max_size: usize) -> Self {
        Self {
            pool: VecDeque::new(),
            max_size,
        }
    }

    /// Get from pool.
    pub fn get(&mut self) -> Option<T> {
        while let Some(mut item) = self.pool.pop_front() {
            if item.is_reusable() {
                item.reset();
                return Some(item);
            }
        }
        None
    }

    /// Return to pool.
    pub fn put(&mut self, item: T) {
        if self.pool.len() < self.max_size && item.is_reusable() {
            self.pool.push_back(item);
        }
    }

    /// Pool size.
    pub fn size(&self) -> usize {
        self.pool.len()
    }

    /// Clear pool.
    pub fn clear(&mut self) {
        self.pool.clear();
    }

    /// Is empty.
    pub fn is_empty(&self) -> bool {
        self.pool.is_empty()
    }
}

/// Pooled value.
pub struct Pooled<'a, T: Reclaimable> {
    value: Option<T>,
    pool: &'a std::cell::RefCell<ReclaimPool<T>>,
}

impl<'a, T: Reclaimable> Pooled<'a, T> {
    /// Create new pooled value.
    pub fn new(value: T, pool: &'a std::cell::RefCell<ReclaimPool<T>>) -> Self {
        Self {
            value: Some(value),
            pool,
        }
    }

    /// Take value (won't return to pool).
    pub fn take(mut self) -> T {
        self.value.take().unwrap()
    }
}

impl<T: Reclaimable> std::ops::Deref for Pooled<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.value.as_ref().unwrap()
    }
}

impl<T: Reclaimable> std::ops::DerefMut for Pooled<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.value.as_mut().unwrap()
    }
}

impl<T: Reclaimable> Drop for Pooled<'_, T> {
    fn drop(&mut self) {
        if let Some(value) = self.value.take() {
            self.pool.borrow_mut().put(value);
        }
    }
}

/// Reclaim statistics.
#[derive(Debug, Clone, Default)]
pub struct ReclaimStats {
    /// Total allocations.
    pub allocations: usize,
    /// Total reclamations.
    pub reclamations: usize,
    /// Reuse count.
    pub reuse_count: usize,
    /// Failed reclamations.
    pub failures: usize,
}

impl ReclaimStats {
    /// Create new stats.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record allocation.
    pub fn record_allocation(&mut self) {
        self.allocations += 1;
    }

    /// Record reclamation.
    pub fn record_reclamation(&mut self) {
        self.reclamations += 1;
    }

    /// Record reuse.
    pub fn record_reuse(&mut self) {
        self.reuse_count += 1;
    }

    /// Record failure.
    pub fn record_failure(&mut self) {
        self.failures += 1;
    }

    /// Reuse ratio.
    pub fn reuse_ratio(&self) -> f64 {
        if self.allocations == 0 {
            0.0
        } else {
            self.reuse_count as f64 / self.allocations as f64
        }
    }
}

/// Resettable buffer.
#[derive(Debug, Clone)]
pub struct ResettableBuffer {
    data: Vec<u8>,
    max_capacity: usize,
}

impl ResettableBuffer {
    /// Create new buffer.
    pub fn new(max_capacity: usize) -> Self {
        Self {
            data: Vec::new(),
            max_capacity,
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

    /// Extend buffer.
    pub fn extend(&mut self, bytes: &[u8]) {
        self.data.extend_from_slice(bytes);
    }

    /// Length.
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Is empty.
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

impl Reclaimable for ResettableBuffer {
    fn reset(&mut self) {
        self.data.clear();
    }

    fn is_reusable(&self) -> bool {
        self.data.capacity() <= self.max_capacity
    }
}

/// Reclaim on drop wrapper.
pub struct ReclaimOnDrop<T, F: FnOnce(T)> {
    value: Option<T>,
    reclaimer: Option<F>,
}

impl<T, F: FnOnce(T)> ReclaimOnDrop<T, F> {
    /// Create new.
    pub fn new(value: T, reclaimer: F) -> Self {
        Self {
            value: Some(value),
            reclaimer: Some(reclaimer),
        }
    }

    /// Get reference.
    pub fn get(&self) -> Option<&T> {
        self.value.as_ref()
    }

    /// Get mutable reference.
    pub fn get_mut(&mut self) -> Option<&mut T> {
        self.value.as_mut()
    }

    /// Take value, skipping reclamation.
    pub fn take(mut self) -> T {
        self.reclaimer = None;
        self.value.take().unwrap()
    }
}

impl<T, F: FnOnce(T)> Drop for ReclaimOnDrop<T, F> {
    fn drop(&mut self) {
        if let (Some(v), Some(r)) = (self.value.take(), self.reclaimer.take()) {
            r(v);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct TestResource {
        value: i32,
    }

    impl Reclaimable for TestResource {
        fn reset(&mut self) {
            self.value = 0;
        }
    }

    #[test]
    fn test_reclaim_pool() {
        let mut pool: ReclaimPool<TestResource> = ReclaimPool::new(10);

        pool.put(TestResource { value: 42 });
        assert_eq!(pool.size(), 1);

        let item = pool.get().unwrap();
        assert_eq!(item.value, 0); // Reset
        assert!(pool.is_empty());
    }

    #[test]
    fn test_reclaim_stats() {
        let mut stats = ReclaimStats::new();
        stats.record_allocation();
        stats.record_allocation();
        stats.record_reuse();

        assert_eq!(stats.allocations, 2);
        assert_eq!(stats.reuse_count, 1);
        assert!((stats.reuse_ratio() - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_resettable_buffer() {
        let mut buf = ResettableBuffer::new(1024);
        buf.extend(b"hello");
        assert_eq!(buf.len(), 5);

        buf.reset();
        assert!(buf.is_empty());
    }
}
