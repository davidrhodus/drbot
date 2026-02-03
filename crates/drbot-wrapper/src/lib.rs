//! Wrapper type utilities for drbot.
//!
//! This crate provides:
//! - Generic wrapper types
//! - Newtype wrappers
//! - Tagged wrappers

use std::ops::{Deref, DerefMut};
use thiserror::Error;

/// Wrapper error types.
#[derive(Error, Debug)]
pub enum WrapperError {
    #[error("Unwrap failed")]
    UnwrapFailed,

    #[error("Invalid wrapper state")]
    InvalidState,
}

/// Result type for wrapper operations.
pub type Result<T> = std::result::Result<T, WrapperError>;

/// Generic wrapper that holds a value.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Wrapper<T>(pub T);

impl<T> Wrapper<T> {
    /// Create new wrapper.
    pub fn new(value: T) -> Self {
        Self(value)
    }

    /// Get inner value.
    pub fn into_inner(self) -> T {
        self.0
    }

    /// Get reference to inner value.
    pub fn inner(&self) -> &T {
        &self.0
    }

    /// Get mutable reference to inner value.
    pub fn inner_mut(&mut self) -> &mut T {
        &mut self.0
    }

    /// Map inner value.
    pub fn map<U, F: FnOnce(T) -> U>(self, f: F) -> Wrapper<U> {
        Wrapper(f(self.0))
    }
}

impl<T> Deref for Wrapper<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> DerefMut for Wrapper<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T> From<T> for Wrapper<T> {
    fn from(value: T) -> Self {
        Self::new(value)
    }
}

impl<T: Default> Default for Wrapper<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

/// Tagged wrapper with a type tag.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Tagged<T, Tag> {
    value: T,
    _tag: std::marker::PhantomData<Tag>,
}

impl<T, Tag> Tagged<T, Tag> {
    /// Create new tagged value.
    pub fn new(value: T) -> Self {
        Self {
            value,
            _tag: std::marker::PhantomData,
        }
    }

    /// Get inner value.
    pub fn into_inner(self) -> T {
        self.value
    }

    /// Get reference to inner value.
    pub fn inner(&self) -> &T {
        &self.value
    }

    /// Get mutable reference to inner value.
    pub fn inner_mut(&mut self) -> &mut T {
        &mut self.value
    }

    /// Map inner value.
    pub fn map<U, F: FnOnce(T) -> U>(self, f: F) -> Tagged<U, Tag> {
        Tagged::new(f(self.value))
    }

    /// Retag with different tag type.
    pub fn retag<NewTag>(self) -> Tagged<T, NewTag> {
        Tagged::new(self.value)
    }
}

impl<T, Tag> Deref for Tagged<T, Tag> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<T, Tag> DerefMut for Tagged<T, Tag> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.value
    }
}

impl<T: Default, Tag> Default for Tagged<T, Tag> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

/// Labeled wrapper with a string label.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Labeled<T> {
    value: T,
    label: String,
}

impl<T> Labeled<T> {
    /// Create new labeled value.
    pub fn new(value: T, label: impl Into<String>) -> Self {
        Self {
            value,
            label: label.into(),
        }
    }

    /// Get label.
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Set label.
    pub fn set_label(&mut self, label: impl Into<String>) {
        self.label = label.into();
    }

    /// Get inner value.
    pub fn into_inner(self) -> T {
        self.value
    }

    /// Get reference to inner value.
    pub fn inner(&self) -> &T {
        &self.value
    }

    /// Get mutable reference to inner value.
    pub fn inner_mut(&mut self) -> &mut T {
        &mut self.value
    }
}

impl<T> Deref for Labeled<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<T> DerefMut for Labeled<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.value
    }
}

/// Versioned wrapper with version number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Versioned<T> {
    value: T,
    version: u64,
}

impl<T> Versioned<T> {
    /// Create new versioned value.
    pub fn new(value: T) -> Self {
        Self { value, version: 1 }
    }

    /// Create with specific version.
    pub fn with_version(value: T, version: u64) -> Self {
        Self { value, version }
    }

    /// Get version.
    pub fn version(&self) -> u64 {
        self.version
    }

    /// Update value and increment version.
    pub fn update(&mut self, value: T) {
        self.value = value;
        self.version += 1;
    }

    /// Get inner value.
    pub fn into_inner(self) -> T {
        self.value
    }

    /// Get reference to inner value.
    pub fn inner(&self) -> &T {
        &self.value
    }

    /// Get mutable reference to inner value (does not increment version).
    pub fn inner_mut(&mut self) -> &mut T {
        &mut self.value
    }
}

impl<T> Deref for Versioned<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

/// Timestamped wrapper.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Timestamped<T> {
    value: T,
    created_at: std::time::Instant,
    updated_at: std::time::Instant,
}

impl<T> Timestamped<T> {
    /// Create new timestamped value.
    pub fn new(value: T) -> Self {
        let now = std::time::Instant::now();
        Self {
            value,
            created_at: now,
            updated_at: now,
        }
    }

    /// Get created timestamp.
    pub fn created_at(&self) -> std::time::Instant {
        self.created_at
    }

    /// Get updated timestamp.
    pub fn updated_at(&self) -> std::time::Instant {
        self.updated_at
    }

    /// Get age since creation.
    pub fn age(&self) -> std::time::Duration {
        self.created_at.elapsed()
    }

    /// Get time since last update.
    pub fn since_update(&self) -> std::time::Duration {
        self.updated_at.elapsed()
    }

    /// Update value.
    pub fn update(&mut self, value: T) {
        self.value = value;
        self.updated_at = std::time::Instant::now();
    }

    /// Get inner value.
    pub fn into_inner(self) -> T {
        self.value
    }

    /// Get reference to inner value.
    pub fn inner(&self) -> &T {
        &self.value
    }

    /// Get mutable reference (updates timestamp).
    pub fn inner_mut(&mut self) -> &mut T {
        self.updated_at = std::time::Instant::now();
        &mut self.value
    }
}

impl<T> Deref for Timestamped<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

/// Optional wrapper with explicit None representation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Maybe<T> {
    Just(T),
    Nothing,
}

impl<T> Maybe<T> {
    /// Create Just variant.
    pub fn just(value: T) -> Self {
        Self::Just(value)
    }

    /// Create Nothing variant.
    pub fn nothing() -> Self {
        Self::Nothing
    }

    /// Check if Just.
    pub fn is_just(&self) -> bool {
        matches!(self, Self::Just(_))
    }

    /// Check if Nothing.
    pub fn is_nothing(&self) -> bool {
        matches!(self, Self::Nothing)
    }

    /// Get value or default.
    pub fn unwrap_or(self, default: T) -> T {
        match self {
            Self::Just(v) => v,
            Self::Nothing => default,
        }
    }

    /// Map inner value.
    pub fn map<U, F: FnOnce(T) -> U>(self, f: F) -> Maybe<U> {
        match self {
            Self::Just(v) => Maybe::Just(f(v)),
            Self::Nothing => Maybe::Nothing,
        }
    }

    /// Convert to Option.
    pub fn to_option(self) -> Option<T> {
        match self {
            Self::Just(v) => Some(v),
            Self::Nothing => None,
        }
    }
}

impl<T> From<Option<T>> for Maybe<T> {
    fn from(opt: Option<T>) -> Self {
        match opt {
            Some(v) => Self::Just(v),
            None => Self::Nothing,
        }
    }
}

impl<T> Default for Maybe<T> {
    fn default() -> Self {
        Self::Nothing
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wrapper() {
        let w = Wrapper::new(42);
        assert_eq!(*w, 42);
        assert_eq!(w.into_inner(), 42);
    }

    #[test]
    fn test_tagged() {
        struct UserId;
        struct OrderId;

        let user_id: Tagged<u64, UserId> = Tagged::new(123);
        let order_id: Tagged<u64, OrderId> = Tagged::new(123);

        assert_eq!(*user_id, 123);
        assert_eq!(*order_id, 123);
        // Different types, can't compare directly
    }

    #[test]
    fn test_labeled() {
        let labeled = Labeled::new(42, "answer");
        assert_eq!(labeled.label(), "answer");
        assert_eq!(*labeled, 42);
    }

    #[test]
    fn test_versioned() {
        let mut versioned = Versioned::new("v1".to_string());
        assert_eq!(versioned.version(), 1);

        versioned.update("v2".to_string());
        assert_eq!(versioned.version(), 2);
        assert_eq!(*versioned, "v2");
    }

    #[test]
    fn test_maybe() {
        let just = Maybe::just(42);
        let nothing: Maybe<i32> = Maybe::nothing();

        assert!(just.is_just());
        assert!(nothing.is_nothing());
        assert_eq!(just.unwrap_or(0), 42);
        assert_eq!(nothing.unwrap_or(0), 0);
    }
}
