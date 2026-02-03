//! Thread-safety utilities for drbot.
//!
//! This crate provides:
//! - Thread-safe wrappers
//! - Shared state utilities
//! - Thread-local utilities

use std::sync::{Arc, Mutex, RwLock};
use thiserror::Error;

/// Thread-safe error types.
#[derive(Error, Debug, Clone)]
pub enum ThreadSafeError {
    #[error("Lock poisoned")]
    Poisoned,

    #[error("Would block")]
    WouldBlock,
}

/// Result type for thread-safe operations.
pub type Result<T> = std::result::Result<T, ThreadSafeError>;

/// Shared mutable state.
#[derive(Debug)]
pub struct Shared<T> {
    inner: Arc<Mutex<T>>,
}

impl<T> Shared<T> {
    /// Create new shared value.
    pub fn new(value: T) -> Self {
        Self {
            inner: Arc::new(Mutex::new(value)),
        }
    }

    /// Lock and get reference.
    pub fn lock(&self) -> Result<std::sync::MutexGuard<'_, T>> {
        self.inner.lock().map_err(|_| ThreadSafeError::Poisoned)
    }

    /// Try lock.
    pub fn try_lock(&self) -> Result<std::sync::MutexGuard<'_, T>> {
        self.inner.try_lock().map_err(|e| match e {
            std::sync::TryLockError::Poisoned(_) => ThreadSafeError::Poisoned,
            std::sync::TryLockError::WouldBlock => ThreadSafeError::WouldBlock,
        })
    }

    /// Get strong count.
    pub fn strong_count(&self) -> usize {
        Arc::strong_count(&self.inner)
    }
}

impl<T> Clone for Shared<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<T: Default> Default for Shared<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

/// Read-write shared state.
#[derive(Debug)]
pub struct RwShared<T> {
    inner: Arc<RwLock<T>>,
}

impl<T> RwShared<T> {
    /// Create new.
    pub fn new(value: T) -> Self {
        Self {
            inner: Arc::new(RwLock::new(value)),
        }
    }

    /// Read lock.
    pub fn read(&self) -> Result<std::sync::RwLockReadGuard<'_, T>> {
        self.inner.read().map_err(|_| ThreadSafeError::Poisoned)
    }

    /// Write lock.
    pub fn write(&self) -> Result<std::sync::RwLockWriteGuard<'_, T>> {
        self.inner.write().map_err(|_| ThreadSafeError::Poisoned)
    }

    /// Try read.
    pub fn try_read(&self) -> Result<std::sync::RwLockReadGuard<'_, T>> {
        self.inner.try_read().map_err(|e| match e {
            std::sync::TryLockError::Poisoned(_) => ThreadSafeError::Poisoned,
            std::sync::TryLockError::WouldBlock => ThreadSafeError::WouldBlock,
        })
    }

    /// Try write.
    pub fn try_write(&self) -> Result<std::sync::RwLockWriteGuard<'_, T>> {
        self.inner.try_write().map_err(|e| match e {
            std::sync::TryLockError::Poisoned(_) => ThreadSafeError::Poisoned,
            std::sync::TryLockError::WouldBlock => ThreadSafeError::WouldBlock,
        })
    }
}

impl<T> Clone for RwShared<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<T: Default> Default for RwShared<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

/// Thread-safe counter.
#[derive(Debug, Default)]
pub struct Counter {
    value: std::sync::atomic::AtomicUsize,
}

impl Counter {
    /// Create new.
    pub const fn new(value: usize) -> Self {
        Self {
            value: std::sync::atomic::AtomicUsize::new(value),
        }
    }

    /// Get value.
    pub fn get(&self) -> usize {
        self.value.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Set value.
    pub fn set(&self, value: usize) {
        self.value
            .store(value, std::sync::atomic::Ordering::Relaxed);
    }

    /// Increment.
    pub fn increment(&self) -> usize {
        self.value
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    }

    /// Decrement.
    pub fn decrement(&self) -> usize {
        self.value
            .fetch_sub(1, std::sync::atomic::Ordering::Relaxed)
    }

    /// Add.
    pub fn add(&self, n: usize) -> usize {
        self.value
            .fetch_add(n, std::sync::atomic::Ordering::Relaxed)
    }

    /// Compare and swap.
    pub fn compare_swap(&self, current: usize, new: usize) -> bool {
        self.value
            .compare_exchange(
                current,
                new,
                std::sync::atomic::Ordering::SeqCst,
                std::sync::atomic::Ordering::Relaxed,
            )
            .is_ok()
    }
}

/// Thread-safe flag.
#[derive(Debug, Default)]
pub struct Flag {
    value: std::sync::atomic::AtomicBool,
}

impl Flag {
    /// Create new.
    pub const fn new(value: bool) -> Self {
        Self {
            value: std::sync::atomic::AtomicBool::new(value),
        }
    }

    /// Get value.
    pub fn get(&self) -> bool {
        self.value.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Set value.
    pub fn set(&self, value: bool) {
        self.value
            .store(value, std::sync::atomic::Ordering::Relaxed);
    }

    /// Set to true.
    pub fn raise(&self) {
        self.set(true);
    }

    /// Set to false.
    pub fn lower(&self) {
        self.set(false);
    }

    /// Swap value.
    pub fn swap(&self, value: bool) -> bool {
        self.value.swap(value, std::sync::atomic::Ordering::Relaxed)
    }

    /// Set to true if false.
    pub fn try_raise(&self) -> bool {
        !self.swap(true)
    }
}

/// Once flag.
pub struct OnceFlag {
    done: Flag,
}

impl OnceFlag {
    /// Create new.
    pub const fn new() -> Self {
        Self {
            done: Flag::new(false),
        }
    }

    /// Run once.
    pub fn call_once<F: FnOnce()>(&self, f: F) {
        if self.done.try_raise() {
            f();
        }
    }

    /// Is done.
    pub fn is_done(&self) -> bool {
        self.done.get()
    }
}

impl Default for OnceFlag {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shared() {
        let shared = Shared::new(42);
        {
            let mut guard = shared.lock().unwrap();
            *guard = 84;
        }
        assert_eq!(*shared.lock().unwrap(), 84);
    }

    #[test]
    fn test_rw_shared() {
        let shared = RwShared::new(42);
        assert_eq!(*shared.read().unwrap(), 42);

        *shared.write().unwrap() = 84;
        assert_eq!(*shared.read().unwrap(), 84);
    }

    #[test]
    fn test_counter() {
        let counter = Counter::new(0);
        assert_eq!(counter.increment(), 0);
        assert_eq!(counter.get(), 1);
        assert_eq!(counter.add(5), 1);
        assert_eq!(counter.get(), 6);
    }

    #[test]
    fn test_flag() {
        let flag = Flag::new(false);
        assert!(!flag.get());
        flag.raise();
        assert!(flag.get());
    }

    #[test]
    fn test_once_flag() {
        let once = OnceFlag::new();
        let mut count = 0;

        once.call_once(|| count += 1);
        once.call_once(|| count += 1);
        once.call_once(|| count += 1);

        assert_eq!(count, 1);
    }
}
