//! Empty value utilities for drbot.
//!
//! This crate provides:
//! - Empty trait
//! - Empty checking
//! - Empty handling

use thiserror::Error;

/// Empty error types.
#[derive(Error, Debug, Clone)]
pub enum EmptyError {
    #[error("Value is empty")]
    IsEmpty,

    #[error("Expected non-empty")]
    ExpectedNonEmpty,
}

/// Result type for empty operations.
pub type Result<T> = std::result::Result<T, EmptyError>;

/// Empty trait.
pub trait Empty {
    /// Create empty value.
    fn empty() -> Self;

    /// Check if empty.
    fn is_empty(&self) -> bool;

    /// Check if not empty.
    fn is_not_empty(&self) -> bool {
        !self.is_empty()
    }
}

impl Empty for String {
    fn empty() -> Self {
        String::new()
    }
    fn is_empty(&self) -> bool {
        self.is_empty()
    }
}

impl<T> Empty for Vec<T> {
    fn empty() -> Self {
        Vec::new()
    }
    fn is_empty(&self) -> bool {
        self.is_empty()
    }
}

impl<K: std::hash::Hash + Eq, V> Empty for std::collections::HashMap<K, V> {
    fn empty() -> Self {
        std::collections::HashMap::new()
    }
    fn is_empty(&self) -> bool {
        self.is_empty()
    }
}

impl<T: std::hash::Hash + Eq> Empty for std::collections::HashSet<T> {
    fn empty() -> Self {
        std::collections::HashSet::new()
    }
    fn is_empty(&self) -> bool {
        self.is_empty()
    }
}

impl<K: Ord, V> Empty for std::collections::BTreeMap<K, V> {
    fn empty() -> Self {
        std::collections::BTreeMap::new()
    }
    fn is_empty(&self) -> bool {
        self.is_empty()
    }
}

impl<T: Ord> Empty for std::collections::BTreeSet<T> {
    fn empty() -> Self {
        std::collections::BTreeSet::new()
    }
    fn is_empty(&self) -> bool {
        self.is_empty()
    }
}

impl<T> Empty for std::collections::VecDeque<T> {
    fn empty() -> Self {
        std::collections::VecDeque::new()
    }
    fn is_empty(&self) -> bool {
        self.is_empty()
    }
}

impl<T> Empty for std::collections::LinkedList<T> {
    fn empty() -> Self {
        std::collections::LinkedList::new()
    }
    fn is_empty(&self) -> bool {
        self.is_empty()
    }
}

/// Non-empty wrapper.
#[derive(Debug, Clone)]
pub struct NonEmpty<T: Empty>(T);

impl<T: Empty + Clone> NonEmpty<T> {
    /// Create non-empty value.
    pub fn new(value: T) -> Result<Self> {
        if value.is_empty() {
            Err(EmptyError::IsEmpty)
        } else {
            Ok(Self(value))
        }
    }

    /// Get value.
    pub fn get(&self) -> &T {
        &self.0
    }

    /// Into inner.
    pub fn into_inner(self) -> T {
        self.0
    }
}

/// Empty extension trait.
pub trait EmptyExt: Empty + Sized {
    /// Clear to empty.
    fn clear_to_empty(&mut self)
    where
        Self: Clone,
    {
        *self = Self::empty();
    }

    /// Take value, leaving empty.
    fn take_empty(&mut self) -> Self
    where
        Self: Clone,
    {
        let old = self.clone();
        *self = Self::empty();
        old
    }

    /// Get if not empty.
    fn if_not_empty(&self) -> Option<&Self> {
        if self.is_empty() {
            None
        } else {
            Some(self)
        }
    }

    /// Or default if empty.
    fn or_default_if_empty(self, default: Self) -> Self {
        if self.is_empty() {
            default
        } else {
            self
        }
    }
}

impl<T: Empty + Sized> EmptyExt for T {}

/// Empty or trait for Option.
pub trait EmptyOr {
    type Inner;

    /// Return None if inner is empty.
    fn none_if_empty(self) -> Option<Self::Inner>;
}

impl<T: Empty> EmptyOr for Option<T> {
    type Inner = T;

    fn none_if_empty(self) -> Option<T> {
        self.filter(|v| !v.is_empty())
    }
}

/// Coalesce empty.
pub fn coalesce<T: Empty>(values: impl IntoIterator<Item = T>) -> Option<T> {
    values.into_iter().find(|v| !v.is_empty())
}

/// First non-empty.
pub fn first_non_empty<T: Empty>(a: T, b: T) -> T {
    if !a.is_empty() {
        a
    } else {
        b
    }
}

/// Empty string check.
pub fn is_blank(s: &str) -> bool {
    s.trim().is_empty()
}

/// Non-blank string.
pub fn non_blank(s: &str) -> Option<&str> {
    if is_blank(s) {
        None
    } else {
        Some(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_string() {
        assert!(String::empty().is_empty());
        assert!("".to_string().is_empty());
        assert!(!"hello".to_string().is_empty());
    }

    #[test]
    fn test_empty_vec() {
        let v: Vec<i32> = Vec::empty();
        assert!(v.is_empty());
        assert!(vec![1, 2, 3].is_not_empty());
    }

    #[test]
    fn test_non_empty() {
        assert!(NonEmpty::new(String::new()).is_err());
        let ne = NonEmpty::new("hello".to_string()).unwrap();
        assert_eq!(ne.get(), "hello");
    }

    #[test]
    fn test_coalesce() {
        let result = coalesce(vec!["".to_string(), "".to_string(), "hello".to_string()]);
        assert_eq!(result, Some("hello".to_string()));
    }

    #[test]
    fn test_is_blank() {
        assert!(is_blank(""));
        assert!(is_blank("   "));
        assert!(!is_blank("hello"));
    }
}
