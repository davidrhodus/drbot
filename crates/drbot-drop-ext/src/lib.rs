//! Drop trait extensions for drbot.
//!
//! This crate provides:
//! - Drop utilities
//! - Drop guards
//! - Defer patterns

use std::mem::ManuallyDrop;
use thiserror::Error;

/// Drop extension error types.
#[derive(Error, Debug, Clone)]
pub enum DropExtError {
    #[error("Drop failed: {0}")]
    Failed(String),
}

/// Result type for drop operations.
pub type Result<T> = std::result::Result<T, DropExtError>;

/// Drop guard that runs a closure on drop.
pub struct DropGuard<F: FnOnce()> {
    func: Option<F>,
}

impl<F: FnOnce()> DropGuard<F> {
    /// Create new drop guard.
    pub fn new(f: F) -> Self {
        Self { func: Some(f) }
    }

    /// Disarm the guard.
    pub fn disarm(mut self) {
        self.func = None;
    }

    /// Trigger early.
    pub fn trigger(mut self) {
        if let Some(f) = self.func.take() {
            f();
        }
    }
}

impl<F: FnOnce()> Drop for DropGuard<F> {
    fn drop(&mut self) {
        if let Some(f) = self.func.take() {
            f();
        }
    }
}

/// Create drop guard.
pub fn defer<F: FnOnce()>(f: F) -> DropGuard<F> {
    DropGuard::new(f)
}

/// Scoped value that runs cleanup on drop.
pub struct Scoped<T, F: FnOnce(&mut T)> {
    value: T,
    cleanup: Option<F>,
}

impl<T, F: FnOnce(&mut T)> Scoped<T, F> {
    /// Create new scoped value.
    pub fn new(value: T, cleanup: F) -> Self {
        Self {
            value,
            cleanup: Some(cleanup),
        }
    }

    /// Get reference.
    pub fn get(&self) -> &T {
        &self.value
    }

    /// Get mutable reference.
    pub fn get_mut(&mut self) -> &mut T {
        &mut self.value
    }

    /// Into inner without cleanup.
    pub fn into_inner_no_cleanup(mut self) -> T {
        self.cleanup = None;
        // SAFETY: We prevent the destructor from running cleanup.
        let mut md = ManuallyDrop::new(self);
        unsafe { std::ptr::read(&mut md.value) }
    }
}

impl<T, F: FnOnce(&mut T)> Drop for Scoped<T, F> {
    fn drop(&mut self) {
        if let Some(f) = self.cleanup.take() {
            f(&mut self.value);
        }
    }
}

impl<T, F: FnOnce(&mut T)> std::ops::Deref for Scoped<T, F> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<T, F: FnOnce(&mut T)> std::ops::DerefMut for Scoped<T, F> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.value
    }
}

/// Drop counter.
#[derive(Debug, Clone, Default)]
pub struct DropCounter {
    count: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

impl DropCounter {
    /// Create new counter.
    pub fn new() -> Self {
        Self {
            count: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        }
    }

    /// Get drop count.
    pub fn count(&self) -> usize {
        self.count.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Create tracked value.
    pub fn track<T>(&self, value: T) -> Tracked<T> {
        Tracked {
            value,
            counter: self.count.clone(),
        }
    }
}

/// Tracked value that increments counter on drop.
pub struct Tracked<T> {
    value: T,
    counter: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

impl<T> Tracked<T> {
    /// Get reference.
    pub fn get(&self) -> &T {
        &self.value
    }

    /// Get mutable reference.
    pub fn get_mut(&mut self) -> &mut T {
        &mut self.value
    }

    /// Into inner without incrementing counter.
    pub fn into_inner(self) -> T {
        let mut md = ManuallyDrop::new(self);
        unsafe { std::ptr::read(&mut md.value) }
    }
}

impl<T> Drop for Tracked<T> {
    fn drop(&mut self) {
        self.counter
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
}

impl<T> std::ops::Deref for Tracked<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

/// Explicit drop.
pub fn explicit_drop<T>(_value: T) {
    // Value is dropped here.
}

/// Drop in place.
pub fn drop_in_place<T>(value: &mut T) {
    // SAFETY: We take a mutable reference, so we own it.
    unsafe { std::ptr::drop_in_place(value) };
}

/// Forget value (prevent drop).
pub fn forget<T>(value: T) {
    std::mem::forget(value);
}

/// Check if type needs drop.
pub fn needs_drop<T>() -> bool {
    std::mem::needs_drop::<T>()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_drop_guard() {
        let mut dropped = false;
        {
            let _guard = defer(|| dropped = true);
        }
        assert!(dropped);
    }

    #[test]
    fn test_drop_guard_disarm() {
        let mut dropped = false;
        {
            let guard = defer(|| dropped = true);
            guard.disarm();
        }
        assert!(!dropped);
    }

    #[test]
    fn test_scoped() {
        let mut cleaned = false;
        {
            let _s = Scoped::new(42, |_| cleaned = true);
        }
        assert!(cleaned);
    }

    #[test]
    fn test_drop_counter() {
        let counter = DropCounter::new();
        {
            let _t1 = counter.track(1);
            let _t2 = counter.track(2);
        }
        assert_eq!(counter.count(), 2);
    }

    #[test]
    fn test_needs_drop() {
        assert!(!needs_drop::<i32>());
        assert!(needs_drop::<String>());
    }
}
