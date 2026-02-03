//! Pin utilities for drbot.
//!
//! This crate provides:
//! - Pin utilities
//! - Pinning patterns
//! - Self-referential helpers

use std::marker::PhantomPinned;
use std::pin::Pin;
use thiserror::Error;

/// Pin error types.
#[derive(Error, Debug, Clone)]
pub enum PinError {
    #[error("Already pinned")]
    AlreadyPinned,

    #[error("Not pinned")]
    NotPinned,
}

/// Result type for pin operations.
pub type Result<T> = std::result::Result<T, PinError>;

/// Pin a value on the stack.
#[macro_export]
macro_rules! pin_stack {
    ($name:ident, $value:expr) => {
        let mut $name = $value;
        #[allow(unused_mut)]
        let mut $name = unsafe { std::pin::Pin::new_unchecked(&mut $name) };
    };
}

/// Pinned wrapper.
pub struct Pinned<T> {
    value: T,
    _pin: PhantomPinned,
}

impl<T> Pinned<T> {
    /// Create new pinned value.
    pub fn new(value: T) -> Self {
        Self {
            value,
            _pin: PhantomPinned,
        }
    }

    /// Get reference (requires Pin).
    pub fn get_ref(self: Pin<&Self>) -> &T {
        &Pin::get_ref(self).value
    }

    /// Get mutable reference (requires Pin).
    pub fn get_mut(self: Pin<&mut Self>) -> &mut T {
        // SAFETY: We don't move the value.
        unsafe { &mut self.get_unchecked_mut().value }
    }
}

/// Pin a boxed value.
pub fn pin_box<T>(value: T) -> Pin<Box<T>> {
    Box::pin(value)
}

/// Unpin wrapper for types that need Unpin.
#[derive(Debug, Clone, Copy, Default)]
pub struct Unpinned<T>(pub T);

impl<T> Unpinned<T> {
    /// Create new.
    pub fn new(value: T) -> Self {
        Self(value)
    }

    /// Get inner.
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> std::ops::Deref for Unpinned<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> std::ops::DerefMut for Unpinned<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T> Unpin for Unpinned<T> {}

/// Check if type is Unpin.
pub const fn is_unpin<T: Unpin>() -> bool {
    true
}

/// Pin projection helper.
pub struct PinProject<'a, T: ?Sized> {
    inner: Pin<&'a mut T>,
}

impl<'a, T: ?Sized> PinProject<'a, T> {
    /// Create from pinned reference.
    pub fn new(inner: Pin<&'a mut T>) -> Self {
        Self { inner }
    }

    /// Get inner pin.
    pub fn as_pin(&mut self) -> Pin<&mut T> {
        self.inner.as_mut()
    }

    /// Get reference.
    pub fn as_ref(&self) -> Pin<&T> {
        self.inner.as_ref()
    }
}

/// Pinnable trait for types that can be pinned.
pub trait Pinnable: Sized {
    /// Pin on heap.
    fn pin_box(self) -> Pin<Box<Self>> {
        Box::pin(self)
    }
}

impl<T> Pinnable for T {}

/// Maybe pinned value.
pub enum MaybePin<'a, T> {
    /// Pinned reference.
    Pinned(Pin<&'a mut T>),
    /// Unpinned reference.
    Unpinned(&'a mut T),
}

impl<'a, T> MaybePin<'a, T> {
    /// Create pinned.
    pub fn pinned(pin: Pin<&'a mut T>) -> Self {
        Self::Pinned(pin)
    }

    /// Create unpinned.
    pub fn unpinned(r: &'a mut T) -> Self {
        Self::Unpinned(r)
    }

    /// Is pinned.
    pub fn is_pinned(&self) -> bool {
        matches!(self, Self::Pinned(_))
    }

    /// Get reference.
    pub fn as_ref(&self) -> &T {
        match self {
            Self::Pinned(p) => p.as_ref().get_ref(),
            Self::Unpinned(r) => r,
        }
    }
}

impl<'a, T: Unpin> MaybePin<'a, T> {
    /// Get mutable reference (only for Unpin types).
    pub fn as_mut(&mut self) -> &mut T {
        match self {
            Self::Pinned(p) => p.as_mut().get_mut(),
            Self::Unpinned(r) => r,
        }
    }
}

/// Pin-safe cell.
pub struct PinCell<T> {
    value: std::cell::UnsafeCell<T>,
    _pin: PhantomPinned,
}

impl<T> PinCell<T> {
    /// Create new.
    pub fn new(value: T) -> Self {
        Self {
            value: std::cell::UnsafeCell::new(value),
            _pin: PhantomPinned,
        }
    }

    /// Get reference.
    pub fn get(self: Pin<&Self>) -> &T {
        // SAFETY: We don't move the value.
        unsafe { &*self.value.get() }
    }

    /// Get mutable reference.
    pub fn get_mut(self: Pin<&mut Self>) -> &mut T {
        // SAFETY: We have exclusive access.
        unsafe { &mut *self.get_unchecked_mut().value.get() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pinned() {
        let pinned = Pinned::new(42);
        let boxed = Box::pin(pinned);
        assert_eq!(*boxed.as_ref().get_ref(), 42);
    }

    #[test]
    fn test_unpinned() {
        let u = Unpinned::new(42);
        assert_eq!(*u, 42);
    }

    #[test]
    fn test_pin_box() {
        let pinned = pin_box(42);
        assert_eq!(*pinned, 42);
    }

    #[test]
    fn test_pinnable() {
        let val = 42;
        let pinned = val.pin_box();
        assert_eq!(*pinned, 42);
    }
}
