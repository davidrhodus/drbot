//! Lifetime utilities for drbot.
//!
//! This crate provides:
//! - Lifetime management
//! - Lifetime coercion helpers
//! - Variance markers

use std::marker::PhantomData;
use thiserror::Error;

/// Lifetime error types.
#[derive(Error, Debug, Clone)]
pub enum LifetimeError {
    #[error("Lifetime expired")]
    Expired,

    #[error("Invalid lifetime")]
    Invalid,
}

/// Result type for lifetime operations.
pub type Result<T> = std::result::Result<T, LifetimeError>;

/// Covariant lifetime marker.
pub struct Covariant<'a>(PhantomData<&'a ()>);

impl<'a> Covariant<'a> {
    /// Create new.
    pub const fn new() -> Self {
        Self(PhantomData)
    }
}

impl<'a> Default for Covariant<'a> {
    fn default() -> Self {
        Self::new()
    }
}

/// Contravariant lifetime marker.
pub struct Contravariant<'a>(PhantomData<fn(&'a ())>);

impl<'a> Contravariant<'a> {
    /// Create new.
    pub const fn new() -> Self {
        Self(PhantomData)
    }
}

impl<'a> Default for Contravariant<'a> {
    fn default() -> Self {
        Self::new()
    }
}

/// Invariant lifetime marker.
pub struct Invariant<'a>(PhantomData<fn(&'a ()) -> &'a ()>);

impl<'a> Invariant<'a> {
    /// Create new.
    pub const fn new() -> Self {
        Self(PhantomData)
    }
}

impl<'a> Default for Invariant<'a> {
    fn default() -> Self {
        Self::new()
    }
}

/// Branded lifetime for type safety.
pub struct Brand<'brand, T> {
    value: T,
    _brand: Invariant<'brand>,
}

impl<'brand, T> Brand<'brand, T> {
    /// Create branded value.
    pub fn new(value: T) -> Self {
        Self {
            value,
            _brand: Invariant::new(),
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

    /// Into inner.
    pub fn into_inner(self) -> T {
        self.value
    }
}

impl<'brand, T> std::ops::Deref for Brand<'brand, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<'brand, T> std::ops::DerefMut for Brand<'brand, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.value
    }
}

/// Scope guard with lifetime.
pub struct ScopeGuard<'a, T, F: FnOnce(&mut T)> {
    value: std::mem::ManuallyDrop<T>,
    cleanup: Option<F>,
    disarmed: bool,
    _lifetime: PhantomData<&'a ()>,
}

impl<'a, T, F: FnOnce(&mut T)> ScopeGuard<'a, T, F> {
    /// Create new scope guard.
    pub fn new(value: T, cleanup: F) -> Self {
        Self {
            value: std::mem::ManuallyDrop::new(value),
            cleanup: Some(cleanup),
            disarmed: false,
            _lifetime: PhantomData,
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

    /// Disarm the guard (don't run cleanup).
    pub fn disarm(mut self) -> T {
        self.cleanup = None;
        self.disarmed = true;
        unsafe { std::mem::ManuallyDrop::take(&mut self.value) }
    }
}

impl<'a, T, F: FnOnce(&mut T)> Drop for ScopeGuard<'a, T, F> {
    fn drop(&mut self) {
        if let Some(cleanup) = self.cleanup.take() {
            cleanup(&mut self.value);
        }
        if !self.disarmed {
            unsafe { std::mem::ManuallyDrop::drop(&mut self.value) };
        }
    }
}

impl<'a, T, F: FnOnce(&mut T)> std::ops::Deref for ScopeGuard<'a, T, F> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<'a, T, F: FnOnce(&mut T)> std::ops::DerefMut for ScopeGuard<'a, T, F> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.value
    }
}

/// Lifetime-bound value.
pub struct Bound<'a, T> {
    value: T,
    _lifetime: Covariant<'a>,
}

impl<'a, T> Bound<'a, T> {
    /// Create new.
    pub fn new(value: T) -> Self {
        Self {
            value,
            _lifetime: Covariant::new(),
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

    /// Into inner.
    pub fn into_inner(self) -> T {
        self.value
    }
}

impl<'a, T> std::ops::Deref for Bound<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<'a, T> std::ops::DerefMut for Bound<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.value
    }
}

/// Static lifetime marker.
pub struct Static<T: 'static> {
    value: T,
}

impl<T: 'static> Static<T> {
    /// Create new.
    pub fn new(value: T) -> Self {
        Self { value }
    }

    /// Get reference.
    pub fn get(&self) -> &T {
        &self.value
    }

    /// Get mutable reference.
    pub fn get_mut(&mut self) -> &mut T {
        &mut self.value
    }

    /// Into inner.
    pub fn into_inner(self) -> T {
        self.value
    }
}

impl<T: 'static> std::ops::Deref for Static<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<T: 'static> std::ops::DerefMut for Static<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.value
    }
}

/// With lifetime scope.
pub fn with_scope<T, F, R>(value: T, f: F) -> R
where
    F: FnOnce(&T) -> R,
{
    f(&value)
}

/// With mutable lifetime scope.
pub fn with_scope_mut<T, F, R>(mut value: T, f: F) -> R
where
    F: FnOnce(&mut T) -> R,
{
    f(&mut value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_brand() {
        let branded = Brand::new(42);
        assert_eq!(*branded, 42);
    }

    #[test]
    fn test_scope_guard() {
        let mut ran = false;
        {
            let _guard = ScopeGuard::new(&mut ran, |r| *r = true);
        }
        assert!(ran);
    }

    #[test]
    fn test_scope_guard_disarm() {
        let mut ran = false;
        let guard = ScopeGuard::new(&mut ran, |r| *r = true);
        guard.disarm();
        assert!(!ran);
    }

    #[test]
    fn test_bound() {
        let bound = Bound::new(42);
        assert_eq!(*bound, 42);
    }

    #[test]
    fn test_with_scope() {
        let result = with_scope(42, |v| *v * 2);
        assert_eq!(result, 84);
    }
}
