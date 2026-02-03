//! Null object pattern utilities for drbot.
//!
//! This crate provides:
//! - Null object trait
//! - Default null implementations
//! - Optional null wrappers

use thiserror::Error;

/// Null object error types.
#[derive(Error, Debug)]
pub enum NullObjectError {
    #[error("Null object operation")]
    NullOperation,

    #[error("Invalid null object")]
    Invalid,
}

/// Result type for null object operations.
pub type Result<T> = std::result::Result<T, NullObjectError>;

/// Trait for objects that have a null variant.
pub trait Nullable: Default {
    /// Check if this is the null object.
    fn is_null(&self) -> bool;

    /// Get the null instance.
    fn null() -> Self
    where
        Self: Sized,
    {
        Self::default()
    }
}

/// Null logger implementation.
#[derive(Debug, Clone, Default)]
pub struct NullLogger;

impl NullLogger {
    /// Create new null logger.
    pub fn new() -> Self {
        Self
    }

    /// Log message (does nothing).
    pub fn log(&self, _message: &str) {}

    /// Log error (does nothing).
    pub fn error(&self, _message: &str) {}

    /// Log warning (does nothing).
    pub fn warn(&self, _message: &str) {}

    /// Log info (does nothing).
    pub fn info(&self, _message: &str) {}

    /// Log debug (does nothing).
    pub fn debug(&self, _message: &str) {}
}

impl Nullable for NullLogger {
    fn is_null(&self) -> bool {
        true
    }
}

/// Null cache implementation.
#[derive(Debug, Clone, Default)]
pub struct NullCache<K, V> {
    _marker: std::marker::PhantomData<(K, V)>,
}

impl<K, V> NullCache<K, V> {
    /// Create new null cache.
    pub fn new() -> Self {
        Self {
            _marker: std::marker::PhantomData,
        }
    }

    /// Get value (always returns None).
    pub fn get(&self, _key: &K) -> Option<&V> {
        None
    }

    /// Set value (does nothing).
    pub fn set(&self, _key: K, _value: V) {}

    /// Remove value (does nothing, returns None).
    pub fn remove(&self, _key: &K) -> Option<V> {
        None
    }

    /// Clear cache (does nothing).
    pub fn clear(&self) {}
}

impl<K: Default, V: Default> Nullable for NullCache<K, V> {
    fn is_null(&self) -> bool {
        true
    }
}

/// Null handler that does nothing.
#[derive(Debug, Clone, Default)]
pub struct NullHandler<T> {
    _marker: std::marker::PhantomData<T>,
}

impl<T> NullHandler<T> {
    /// Create new null handler.
    pub fn new() -> Self {
        Self {
            _marker: std::marker::PhantomData,
        }
    }

    /// Handle value (does nothing).
    pub fn handle(&self, _value: T) {}
}

impl<T: Default> Nullable for NullHandler<T> {
    fn is_null(&self) -> bool {
        true
    }
}

/// Null provider that returns defaults.
#[derive(Debug, Clone, Default)]
pub struct NullProvider<T: Default> {
    _marker: std::marker::PhantomData<T>,
}

impl<T: Default> NullProvider<T> {
    /// Create new null provider.
    pub fn new() -> Self {
        Self {
            _marker: std::marker::PhantomData,
        }
    }

    /// Get value (returns default).
    pub fn get(&self) -> T {
        T::default()
    }
}

impl<T: Default> Nullable for NullProvider<T> {
    fn is_null(&self) -> bool {
        true
    }
}

/// Maybe type that can be null.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Maybe<T> {
    /// Has value.
    Just(T),
    /// Null/no value.
    Null,
}

impl<T> Maybe<T> {
    /// Create with value.
    pub fn just(value: T) -> Self {
        Self::Just(value)
    }

    /// Create null.
    pub fn null() -> Self {
        Self::Null
    }

    /// Check if null.
    pub fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }

    /// Check if has value.
    pub fn is_just(&self) -> bool {
        matches!(self, Self::Just(_))
    }

    /// Get value or default.
    pub fn unwrap_or(self, default: T) -> T {
        match self {
            Self::Just(v) => v,
            Self::Null => default,
        }
    }

    /// Get value or compute default.
    pub fn unwrap_or_else<F: FnOnce() -> T>(self, f: F) -> T {
        match self {
            Self::Just(v) => v,
            Self::Null => f(),
        }
    }

    /// Map value if present.
    pub fn map<U, F: FnOnce(T) -> U>(self, f: F) -> Maybe<U> {
        match self {
            Self::Just(v) => Maybe::Just(f(v)),
            Self::Null => Maybe::Null,
        }
    }

    /// Flat map.
    pub fn and_then<U, F: FnOnce(T) -> Maybe<U>>(self, f: F) -> Maybe<U> {
        match self {
            Self::Just(v) => f(v),
            Self::Null => Maybe::Null,
        }
    }

    /// Convert to Option.
    pub fn into_option(self) -> Option<T> {
        match self {
            Self::Just(v) => Some(v),
            Self::Null => None,
        }
    }
}

impl<T> Default for Maybe<T> {
    fn default() -> Self {
        Self::Null
    }
}

impl<T> From<Option<T>> for Maybe<T> {
    fn from(opt: Option<T>) -> Self {
        match opt {
            Some(v) => Self::Just(v),
            None => Self::Null,
        }
    }
}

impl<T> From<Maybe<T>> for Option<T> {
    fn from(maybe: Maybe<T>) -> Self {
        maybe.into_option()
    }
}

/// Null-safe wrapper.
pub struct NullSafe<T: Nullable> {
    inner: T,
}

impl<T: Nullable> NullSafe<T> {
    /// Create new null-safe wrapper.
    pub fn new(value: T) -> Self {
        Self { inner: value }
    }

    /// Create with null value.
    pub fn null() -> Self {
        Self {
            inner: T::default(),
        }
    }

    /// Check if null.
    pub fn is_null(&self) -> bool {
        self.inner.is_null()
    }

    /// Get inner value.
    pub fn inner(&self) -> &T {
        &self.inner
    }

    /// Get mutable inner.
    pub fn inner_mut(&mut self) -> &mut T {
        &mut self.inner
    }

    /// Take inner value.
    pub fn into_inner(self) -> T {
        self.inner
    }
}

impl<T: Nullable> Default for NullSafe<T> {
    fn default() -> Self {
        Self::null()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_null_logger() {
        let logger = NullLogger::new();
        assert!(logger.is_null());
        logger.log("test"); // Does nothing
    }

    #[test]
    fn test_null_cache() {
        let cache: NullCache<String, i32> = NullCache::new();
        assert!(cache.is_null());
        assert!(cache.get(&"key".to_string()).is_none());
    }

    #[test]
    fn test_maybe() {
        let just = Maybe::just(42);
        let null: Maybe<i32> = Maybe::null();

        assert!(just.is_just());
        assert!(null.is_null());

        assert_eq!(just.unwrap_or(0), 42);
        assert_eq!(null.unwrap_or(0), 0);
    }

    #[test]
    fn test_maybe_map() {
        let just = Maybe::just(21);
        let result = just.map(|x| x * 2);

        assert_eq!(result, Maybe::just(42));

        let null: Maybe<i32> = Maybe::null();
        let result = null.map(|x| x * 2);
        assert!(result.is_null());
    }
}
