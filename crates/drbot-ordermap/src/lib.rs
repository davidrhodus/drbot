//! Ordered map (insertion order) for drbot.
//!
//! This crate provides:
//! - OrderMap that preserves insertion order
//! - Index-based access
//! - Ordered iteration

use std::collections::HashMap;
use std::hash::Hash;
use thiserror::Error;

/// OrderMap error types.
#[derive(Error, Debug)]
pub enum OrderMapError {
    #[error("Key not found")]
    KeyNotFound,

    #[error("Index out of bounds")]
    IndexOutOfBounds,
}

/// Result type for ordermap operations.
pub type Result<T> = std::result::Result<T, OrderMapError>;

/// Map that preserves insertion order.
#[derive(Debug, Clone)]
pub struct OrderMap<K, V> {
    keys: Vec<K>,
    values: HashMap<K, V>,
}

impl<K: Hash + Eq + Clone, V> OrderMap<K, V> {
    /// Create new ordered map.
    pub fn new() -> Self {
        Self {
            keys: Vec::new(),
            values: HashMap::new(),
        }
    }

    /// Create with capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            keys: Vec::with_capacity(capacity),
            values: HashMap::with_capacity(capacity),
        }
    }

    /// Insert key-value pair.
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        let old = self.values.insert(key.clone(), value);
        if old.is_none() {
            self.keys.push(key);
        }
        old
    }

    /// Get value by key.
    pub fn get(&self, key: &K) -> Option<&V> {
        self.values.get(key)
    }

    /// Get mutable value by key.
    pub fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        self.values.get_mut(key)
    }

    /// Get value by index.
    pub fn get_index(&self, index: usize) -> Option<(&K, &V)> {
        self.keys
            .get(index)
            .and_then(|k| self.values.get(k).map(|v| (k, v)))
    }

    /// Get index of key.
    pub fn index_of(&self, key: &K) -> Option<usize> {
        self.keys.iter().position(|k| k == key)
    }

    /// Get first key-value pair.
    pub fn first(&self) -> Option<(&K, &V)> {
        self.get_index(0)
    }

    /// Get last key-value pair.
    pub fn last(&self) -> Option<(&K, &V)> {
        if self.keys.is_empty() {
            None
        } else {
            self.get_index(self.keys.len() - 1)
        }
    }

    /// Check if key exists.
    pub fn contains_key(&self, key: &K) -> bool {
        self.values.contains_key(key)
    }

    /// Remove by key.
    pub fn remove(&mut self, key: &K) -> Option<V> {
        if let Some(value) = self.values.remove(key) {
            self.keys.retain(|k| k != key);
            Some(value)
        } else {
            None
        }
    }

    /// Remove by index.
    pub fn remove_index(&mut self, index: usize) -> Option<(K, V)> {
        if index < self.keys.len() {
            let key = self.keys.remove(index);
            let value = self.values.remove(&key)?;
            Some((key, value))
        } else {
            None
        }
    }

    /// Pop last entry.
    pub fn pop(&mut self) -> Option<(K, V)> {
        let key = self.keys.pop()?;
        let value = self.values.remove(&key)?;
        Some((key, value))
    }

    /// Shift first entry.
    pub fn shift(&mut self) -> Option<(K, V)> {
        if self.keys.is_empty() {
            return None;
        }
        let key = self.keys.remove(0);
        let value = self.values.remove(&key)?;
        Some((key, value))
    }

    /// Get length.
    pub fn len(&self) -> usize {
        self.keys.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    /// Clear all entries.
    pub fn clear(&mut self) {
        self.keys.clear();
        self.values.clear();
    }

    /// Iterate over keys in order.
    pub fn keys(&self) -> impl Iterator<Item = &K> {
        self.keys.iter()
    }

    /// Iterate over values in order.
    pub fn values(&self) -> impl Iterator<Item = &V> {
        self.keys.iter().filter_map(|k| self.values.get(k))
    }

    /// Iterate over key-value pairs in order.
    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.keys
            .iter()
            .filter_map(|k| self.values.get(k).map(|v| (k, v)))
    }

    /// Sort by key.
    pub fn sort_keys(&mut self)
    where
        K: Ord,
    {
        self.keys.sort();
    }

    /// Sort by value.
    pub fn sort_by_value<F>(&mut self, cmp: F)
    where
        F: Fn(&V, &V) -> std::cmp::Ordering,
    {
        self.keys
            .sort_by(|a, b| match (self.values.get(a), self.values.get(b)) {
                (Some(va), Some(vb)) => cmp(va, vb),
                _ => std::cmp::Ordering::Equal,
            });
    }

    /// Reverse order.
    pub fn reverse(&mut self) {
        self.keys.reverse();
    }
}

impl<K: Hash + Eq + Clone, V> Default for OrderMap<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: Hash + Eq + Clone, V> FromIterator<(K, V)> for OrderMap<K, V> {
    fn from_iter<I: IntoIterator<Item = (K, V)>>(iter: I) -> Self {
        let mut map = OrderMap::new();
        for (k, v) in iter {
            map.insert(k, v);
        }
        map
    }
}

impl<K: Hash + Eq + Clone, V> IntoIterator for OrderMap<K, V> {
    type Item = (K, V);
    type IntoIter = std::vec::IntoIter<(K, V)>;

    fn into_iter(self) -> Self::IntoIter {
        let pairs: Vec<(K, V)> = self
            .keys
            .into_iter()
            .filter_map(|k| {
                // This is a bit awkward but necessary for owned iteration
                None::<(K, V)> // Placeholder
            })
            .collect();
        pairs.into_iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insertion_order() {
        let mut map = OrderMap::new();
        map.insert("c", 3);
        map.insert("a", 1);
        map.insert("b", 2);

        let keys: Vec<_> = map.keys().cloned().collect();
        assert_eq!(keys, vec!["c", "a", "b"]);
    }

    #[test]
    fn test_get_index() {
        let mut map = OrderMap::new();
        map.insert("a", 1);
        map.insert("b", 2);

        assert_eq!(map.get_index(0), Some((&"a", &1)));
        assert_eq!(map.get_index(1), Some((&"b", &2)));
        assert_eq!(map.get_index(2), None);
    }

    #[test]
    fn test_pop_and_shift() {
        let mut map = OrderMap::new();
        map.insert("a", 1);
        map.insert("b", 2);
        map.insert("c", 3);

        assert_eq!(map.pop(), Some(("c", 3)));
        assert_eq!(map.shift(), Some(("a", 1)));
        assert_eq!(map.len(), 1);
    }

    #[test]
    fn test_sort_keys() {
        let mut map = OrderMap::new();
        map.insert("c", 3);
        map.insert("a", 1);
        map.insert("b", 2);

        map.sort_keys();
        let keys: Vec<_> = map.keys().cloned().collect();
        assert_eq!(keys, vec!["a", "b", "c"]);
    }
}
