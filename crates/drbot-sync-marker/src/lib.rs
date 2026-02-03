//! Sync trait utilities for drbot.
//!
//! This crate provides:
//! - Sync wrappers
//! - Sync assertions
//! - Thread-safe markers

use std::marker::PhantomData;
use thiserror::Error;

/// Sync error types.
#[derive(Error, Debug, Clone)]
pub enum SyncError {
    #[error("Not syncable")]
    NotSyncable,

    #[error("Sync failed: {0}")]
    Failed(String),
}

/// Result type for sync operations.
pub type Result<T> = std::result::Result<T, SyncError>;

/// Assert that a type is Sync.
pub fn assert_sync<T: Sync>() {}

/// Assert that a value is Sync.
pub fn assert_sync_val<T: Sync>(_: &T) {}

/// Wrapper that asserts Sync.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AssertSync<T>(pub T);

// SAFETY: User asserts this is safe.
unsafe impl<T> Sync for AssertSync<T> {}

impl<T> AssertSync<T> {
    /// Create new.
    pub fn new(value: T) -> Self {
        Self(value)
    }

    /// Get reference.
    pub fn get(&self) -> &T {
        &self.0
    }

    /// Get mutable reference.
    pub fn get_mut(&mut self) -> &mut T {
        &mut self.0
    }

    /// Into inner.
    pub fn into_inner(self) -> T {
        self.0
    }
}

/// Sync bound marker.
#[derive(Debug, Clone, Copy, Default)]
pub struct SyncBound<T: Sync> {
    _marker: PhantomData<T>,
}

impl<T: Sync> SyncBound<T> {
    /// Create new.
    pub fn new() -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}

/// Not Sync marker.
#[derive(Debug, Clone, Copy, Default)]
pub struct NotSync {
    _marker: PhantomData<std::cell::Cell<()>>,
}

impl NotSync {
    /// Create new.
    pub fn new() -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}

/// Assert Send + Sync.
pub fn assert_send_sync<T: Send + Sync>() {}

/// Wrapper that asserts Send + Sync.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AssertSendSync<T>(pub T);

// SAFETY: User asserts this is safe.
unsafe impl<T> Send for AssertSendSync<T> {}
unsafe impl<T> Sync for AssertSendSync<T> {}

impl<T> AssertSendSync<T> {
    /// Create new.
    pub fn new(value: T) -> Self {
        Self(value)
    }

    /// Get reference.
    pub fn get(&self) -> &T {
        &self.0
    }

    /// Get mutable reference.
    pub fn get_mut(&mut self) -> &mut T {
        &mut self.0
    }

    /// Into inner.
    pub fn into_inner(self) -> T {
        self.0
    }
}

/// Syncable wrapper.
pub struct Syncable<T: Sync> {
    value: T,
}

impl<T: Sync> Syncable<T> {
    /// Create new.
    pub fn new(value: T) -> Self {
        Self { value }
    }

    /// Get reference.
    pub fn get(&self) -> &T {
        &self.value
    }

    /// Into inner.
    pub fn into_inner(self) -> T {
        self.value
    }
}

/// Check if type is Sync at compile time.
pub const fn is_sync<T: Sync>() -> bool {
    true
}

/// Check if type is Send + Sync at compile time.
pub const fn is_send_sync<T: Send + Sync>() -> bool {
    true
}

/// Thread-safe marker.
#[derive(Debug, Clone, Copy, Default)]
pub struct ThreadSafe<T: Send + Sync> {
    _marker: PhantomData<T>,
}

impl<T: Send + Sync> ThreadSafe<T> {
    /// Create new.
    pub fn new() -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_assert_sync() {
        assert_sync::<i32>();
        assert_sync::<String>();
        assert_sync_val(&42);
    }

    #[test]
    fn test_syncable() {
        let s = Syncable::new(42);
        assert_eq!(*s.get(), 42);
    }

    #[test]
    fn test_send_sync() {
        assert_send_sync::<i32>();
        assert_send_sync::<String>();
    }
}
