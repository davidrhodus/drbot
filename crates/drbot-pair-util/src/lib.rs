//! Pair utilities for drbot.
//!
//! This crate provides:
//! - Pair type and operations
//! - Key-value pairs
//! - Named pairs

use std::cmp::Ordering;
use thiserror::Error;

/// Pair error types.
#[derive(Error, Debug, Clone)]
pub enum PairError {
    #[error("Invalid pair operation")]
    InvalidOperation,
}

/// Result type for pair operations.
pub type Result<T> = std::result::Result<T, PairError>;

/// A generic pair type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Pair<A, B> {
    /// First element.
    pub first: A,
    /// Second element.
    pub second: B,
}

impl<A, B> Pair<A, B> {
    /// Create new pair.
    pub fn new(first: A, second: B) -> Self {
        Self { first, second }
    }

    /// Map first element.
    pub fn map_first<C, F: FnOnce(A) -> C>(self, f: F) -> Pair<C, B> {
        Pair {
            first: f(self.first),
            second: self.second,
        }
    }

    /// Map second element.
    pub fn map_second<C, F: FnOnce(B) -> C>(self, f: F) -> Pair<A, C> {
        Pair {
            first: self.first,
            second: f(self.second),
        }
    }

    /// Map both elements.
    pub fn map<C, D, F: FnOnce(A) -> C, G: FnOnce(B) -> D>(self, f: F, g: G) -> Pair<C, D> {
        Pair {
            first: f(self.first),
            second: g(self.second),
        }
    }

    /// Convert to tuple.
    pub fn into_tuple(self) -> (A, B) {
        (self.first, self.second)
    }

    /// Get reference to first.
    pub fn first_ref(&self) -> &A {
        &self.first
    }

    /// Get reference to second.
    pub fn second_ref(&self) -> &B {
        &self.second
    }

    /// Get mutable reference to first.
    pub fn first_mut(&mut self) -> &mut A {
        &mut self.first
    }

    /// Get mutable reference to second.
    pub fn second_mut(&mut self) -> &mut B {
        &mut self.second
    }
}

impl<A, B> Pair<A, B>
where
    A: Clone,
    B: Clone,
{
    /// Swap elements.
    pub fn swap(&self) -> Pair<B, A> {
        Pair {
            first: self.second.clone(),
            second: self.first.clone(),
        }
    }
}

impl<A, B> From<(A, B)> for Pair<A, B> {
    fn from((first, second): (A, B)) -> Self {
        Self { first, second }
    }
}

impl<A, B> From<Pair<A, B>> for (A, B) {
    fn from(pair: Pair<A, B>) -> Self {
        (pair.first, pair.second)
    }
}

/// A key-value pair.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct KeyValue<K, V> {
    /// The key.
    pub key: K,
    /// The value.
    pub value: V,
}

impl<K, V> KeyValue<K, V> {
    /// Create new key-value pair.
    pub fn new(key: K, value: V) -> Self {
        Self { key, value }
    }

    /// Map value.
    pub fn map_value<U, F: FnOnce(V) -> U>(self, f: F) -> KeyValue<K, U> {
        KeyValue {
            key: self.key,
            value: f(self.value),
        }
    }

    /// Map key.
    pub fn map_key<L, F: FnOnce(K) -> L>(self, f: F) -> KeyValue<L, V> {
        KeyValue {
            key: f(self.key),
            value: self.value,
        }
    }

    /// Convert to tuple.
    pub fn into_tuple(self) -> (K, V) {
        (self.key, self.value)
    }

    /// Get key reference.
    pub fn key(&self) -> &K {
        &self.key
    }

    /// Get value reference.
    pub fn value(&self) -> &V {
        &self.value
    }

    /// Get mutable value reference.
    pub fn value_mut(&mut self) -> &mut V {
        &mut self.value
    }
}

impl<K: Ord, V: Eq> Ord for KeyValue<K, V> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.key.cmp(&other.key)
    }
}

impl<K: PartialOrd, V: PartialEq> PartialOrd for KeyValue<K, V> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.key.partial_cmp(&other.key)
    }
}

impl<K, V> From<(K, V)> for KeyValue<K, V> {
    fn from((key, value): (K, V)) -> Self {
        Self { key, value }
    }
}

/// A named value pair.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Named<T> {
    /// The name.
    pub name: String,
    /// The value.
    pub value: T,
}

impl<T> Named<T> {
    /// Create new named value.
    pub fn new(name: impl Into<String>, value: T) -> Self {
        Self {
            name: name.into(),
            value,
        }
    }

    /// Map value.
    pub fn map<U, F: FnOnce(T) -> U>(self, f: F) -> Named<U> {
        Named {
            name: self.name,
            value: f(self.value),
        }
    }

    /// Rename.
    pub fn rename(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    /// Get name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get value reference.
    pub fn value(&self) -> &T {
        &self.value
    }

    /// Get mutable value reference.
    pub fn value_mut(&mut self) -> &mut T {
        &mut self.value
    }

    /// Into value.
    pub fn into_value(self) -> T {
        self.value
    }
}

/// A range pair (min, max).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MinMax<T> {
    /// Minimum value.
    pub min: T,
    /// Maximum value.
    pub max: T,
}

impl<T: Ord> MinMax<T> {
    /// Create new min-max pair (sorts values).
    pub fn new(a: T, b: T) -> Self {
        if a <= b {
            Self { min: a, max: b }
        } else {
            Self { min: b, max: a }
        }
    }

    /// Create from sorted values (unchecked).
    pub fn from_sorted(min: T, max: T) -> Self {
        Self { min, max }
    }

    /// Check if value is in range (inclusive).
    pub fn contains(&self, value: &T) -> bool {
        value >= &self.min && value <= &self.max
    }

    /// Get range size.
    pub fn span(&self) -> T
    where
        T: std::ops::Sub<Output = T> + Copy,
    {
        self.max - self.min
    }
}

impl<T: Ord + Clone> MinMax<T> {
    /// Extend range to include value.
    pub fn extend(&mut self, value: T) {
        if value < self.min {
            self.min = value;
        } else if value > self.max {
            self.max = value;
        }
    }

    /// Clamp value to range.
    pub fn clamp(&self, value: T) -> T {
        if value < self.min {
            self.min.clone()
        } else if value > self.max {
            self.max.clone()
        } else {
            value
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pair() {
        let p = Pair::new(1, "hello");
        assert_eq!(p.first, 1);
        assert_eq!(p.second, "hello");

        let mapped = p.map_first(|x| x * 2);
        assert_eq!(mapped.first, 2);
    }

    #[test]
    fn test_key_value() {
        let kv = KeyValue::new("name", 42);
        assert_eq!(kv.key(), &"name");
        assert_eq!(kv.value(), &42);

        let mapped = kv.map_value(|v| v * 2);
        assert_eq!(mapped.value(), &84);
    }

    #[test]
    fn test_named() {
        let named = Named::new("count", 42);
        assert_eq!(named.name(), "count");
        assert_eq!(named.value(), &42);

        let renamed = named.rename("total");
        assert_eq!(renamed.name(), "total");
    }

    #[test]
    fn test_min_max() {
        let mm = MinMax::new(10, 5);
        assert_eq!(mm.min, 5);
        assert_eq!(mm.max, 10);
        assert!(mm.contains(&7));
        assert!(!mm.contains(&3));

        assert_eq!(mm.clamp(3), 5);
        assert_eq!(mm.clamp(15), 10);
        assert_eq!(mm.clamp(7), 7);
    }
}
