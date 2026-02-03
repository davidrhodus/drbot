//! One-to-many map for drbot.
//!
//! This crate provides:
//! - MultiMap (one key to many values)
//! - Various collection strategies (Vec, HashSet)

use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use thiserror::Error;

/// MultiMap error types.
#[derive(Error, Debug)]
pub enum MultiMapError {
    #[error("Key not found")]
    KeyNotFound,

    #[error("Value not found")]
    ValueNotFound,
}

/// Result type for multimap operations.
pub type Result<T> = std::result::Result<T, MultiMapError>;

/// MultiMap with Vec storage (allows duplicates, preserves order).
#[derive(Debug, Clone)]
pub struct MultiMap<K, V> {
    inner: HashMap<K, Vec<V>>,
}

impl<K: Hash + Eq, V> MultiMap<K, V> {
    /// Create new multimap.
    pub fn new() -> Self {
        Self {
            inner: HashMap::new(),
        }
    }

    /// Insert a value for a key.
    pub fn insert(&mut self, key: K, value: V) {
        self.inner.entry(key).or_default().push(value);
    }

    /// Get all values for a key.
    pub fn get(&self, key: &K) -> Option<&Vec<V>> {
        self.inner.get(key)
    }

    /// Get mutable reference to values.
    pub fn get_mut(&mut self, key: &K) -> Option<&mut Vec<V>> {
        self.inner.get_mut(key)
    }

    /// Get first value for a key.
    pub fn get_first(&self, key: &K) -> Option<&V> {
        self.inner.get(key).and_then(|v| v.first())
    }

    /// Get last value for a key.
    pub fn get_last(&self, key: &K) -> Option<&V> {
        self.inner.get(key).and_then(|v| v.last())
    }

    /// Check if key exists.
    pub fn contains_key(&self, key: &K) -> bool {
        self.inner.contains_key(key)
    }

    /// Remove all values for a key.
    pub fn remove(&mut self, key: &K) -> Option<Vec<V>> {
        self.inner.remove(key)
    }

    /// Remove specific value from key.
    pub fn remove_value(&mut self, key: &K, value: &V) -> bool
    where
        V: PartialEq,
    {
        if let Some(values) = self.inner.get_mut(key) {
            if let Some(pos) = values.iter().position(|v| v == value) {
                values.remove(pos);
                if values.is_empty() {
                    self.inner.remove(key);
                }
                return true;
            }
        }
        false
    }

    /// Get number of keys.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Get total number of values.
    pub fn total_values(&self) -> usize {
        self.inner.values().map(|v| v.len()).sum()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Clear all entries.
    pub fn clear(&mut self) {
        self.inner.clear();
    }

    /// Iterate over keys.
    pub fn keys(&self) -> impl Iterator<Item = &K> {
        self.inner.keys()
    }

    /// Iterate over all values (flat).
    pub fn values(&self) -> impl Iterator<Item = &V> {
        self.inner.values().flatten()
    }

    /// Iterate over key-values pairs.
    pub fn iter(&self) -> impl Iterator<Item = (&K, &Vec<V>)> {
        self.inner.iter()
    }

    /// Iterate over all (key, value) pairs.
    pub fn flat_iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.inner
            .iter()
            .flat_map(|(k, vs)| vs.iter().map(move |v| (k, v)))
    }
}

impl<K: Hash + Eq, V> Default for MultiMap<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: Hash + Eq, V> FromIterator<(K, V)> for MultiMap<K, V> {
    fn from_iter<I: IntoIterator<Item = (K, V)>>(iter: I) -> Self {
        let mut map = MultiMap::new();
        for (k, v) in iter {
            map.insert(k, v);
        }
        map
    }
}

/// MultiMap with HashSet storage (no duplicates).
#[derive(Debug, Clone)]
pub struct MultiMapSet<K, V> {
    inner: HashMap<K, HashSet<V>>,
}

impl<K: Hash + Eq, V: Hash + Eq> MultiMapSet<K, V> {
    /// Create new multimap set.
    pub fn new() -> Self {
        Self {
            inner: HashMap::new(),
        }
    }

    /// Insert a value for a key.
    pub fn insert(&mut self, key: K, value: V) -> bool {
        self.inner.entry(key).or_default().insert(value)
    }

    /// Get all values for a key.
    pub fn get(&self, key: &K) -> Option<&HashSet<V>> {
        self.inner.get(key)
    }

    /// Check if key has specific value.
    pub fn contains(&self, key: &K, value: &V) -> bool {
        self.inner
            .get(key)
            .map(|s| s.contains(value))
            .unwrap_or(false)
    }

    /// Check if key exists.
    pub fn contains_key(&self, key: &K) -> bool {
        self.inner.contains_key(key)
    }

    /// Remove all values for a key.
    pub fn remove(&mut self, key: &K) -> Option<HashSet<V>> {
        self.inner.remove(key)
    }

    /// Remove specific value from key.
    pub fn remove_value(&mut self, key: &K, value: &V) -> bool {
        if let Some(values) = self.inner.get_mut(key) {
            let removed = values.remove(value);
            if values.is_empty() {
                self.inner.remove(key);
            }
            return removed;
        }
        false
    }

    /// Get number of keys.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Get total number of values.
    pub fn total_values(&self) -> usize {
        self.inner.values().map(|v| v.len()).sum()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Clear all entries.
    pub fn clear(&mut self) {
        self.inner.clear();
    }

    /// Iterate over keys.
    pub fn keys(&self) -> impl Iterator<Item = &K> {
        self.inner.keys()
    }

    /// Iterate over key-values pairs.
    pub fn iter(&self) -> impl Iterator<Item = (&K, &HashSet<V>)> {
        self.inner.iter()
    }
}

impl<K: Hash + Eq, V: Hash + Eq> Default for MultiMapSet<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_multimap() {
        let mut map = MultiMap::new();
        map.insert("key", 1);
        map.insert("key", 2);
        map.insert("key", 1); // Duplicate allowed

        assert_eq!(map.get(&"key"), Some(&vec![1, 2, 1]));
        assert_eq!(map.total_values(), 3);
    }

    #[test]
    fn test_multimap_remove() {
        let mut map = MultiMap::new();
        map.insert("key", 1);
        map.insert("key", 2);

        assert!(map.remove_value(&"key", &1));
        assert_eq!(map.get(&"key"), Some(&vec![2]));
    }

    #[test]
    fn test_multimap_set() {
        let mut map = MultiMapSet::new();
        assert!(map.insert("key", 1));
        assert!(map.insert("key", 2));
        assert!(!map.insert("key", 1)); // Duplicate rejected

        assert_eq!(map.total_values(), 2);
        assert!(map.contains(&"key", &1));
    }

    #[test]
    fn test_flat_iter() {
        let mut map = MultiMap::new();
        map.insert("a", 1);
        map.insert("a", 2);
        map.insert("b", 3);

        let pairs: Vec<_> = map.flat_iter().collect();
        assert_eq!(pairs.len(), 3);
    }
}

// ============================================================================
// Kani Formal Verification Proofs
// ============================================================================

#[cfg(kani)]
mod kani_proofs {
    use super::*;

    // ------------------------------------------------------------------------
    // MultiMap Proofs
    // ------------------------------------------------------------------------

    #[kani::proof]
    fn proof_multimap_new_is_empty() {
        let map: MultiMap<u8, u8> = MultiMap::new();
        kani::assert(map.is_empty(), "New multimap should be empty");
        kani::assert(map.len() == 0, "New multimap should have zero keys");
        kani::assert(
            map.total_values() == 0,
            "New multimap should have zero values",
        );
    }

    #[kani::proof]
    fn proof_multimap_insert_creates_key() {
        let mut map: MultiMap<u8, u8> = MultiMap::new();
        let key: u8 = kani::any();
        let value: u8 = kani::any();

        map.insert(key, value);

        kani::assert(map.contains_key(&key), "Key should exist after insert");
        kani::assert(!map.is_empty(), "Map should not be empty after insert");
        kani::assert(map.len() == 1, "Map should have one key after insert");
    }

    #[kani::proof]
    fn proof_multimap_insert_increases_total() {
        let mut map: MultiMap<u8, u8> = MultiMap::new();
        let key: u8 = kani::any();
        let v1: u8 = kani::any();
        let v2: u8 = kani::any();

        map.insert(key, v1);
        let count1 = map.total_values();

        map.insert(key, v2);
        let count2 = map.total_values();

        kani::assert(count1 == 1, "Should have one value after first insert");
        kani::assert(count2 == 2, "Should have two values after second insert");
    }

    #[kani::proof]
    fn proof_multimap_allows_duplicates() {
        let mut map: MultiMap<u8, u8> = MultiMap::new();
        let key: u8 = kani::any();
        let value: u8 = kani::any();

        map.insert(key, value);
        map.insert(key, value); // Same value again

        kani::assert(
            map.total_values() == 2,
            "MultiMap should allow duplicate values",
        );
        kani::assert(map.len() == 1, "Key count should remain 1");
    }

    #[kani::proof]
    fn proof_multimap_get_after_insert() {
        let mut map: MultiMap<u8, u8> = MultiMap::new();
        let key: u8 = kani::any();
        let value: u8 = kani::any();

        map.insert(key, value);
        let result = map.get(&key);

        kani::assert(result.is_some(), "Get should return Some after insert");
        kani::assert(result.unwrap().len() == 1, "Should have one value");
        kani::assert(result.unwrap()[0] == value, "Value should match inserted");
    }

    #[kani::proof]
    fn proof_multimap_get_first_last() {
        let mut map: MultiMap<u8, u8> = MultiMap::new();
        let key: u8 = kani::any();
        let v1: u8 = kani::any();
        let v2: u8 = kani::any();

        map.insert(key, v1);
        map.insert(key, v2);

        kani::assert(
            map.get_first(&key) == Some(&v1),
            "get_first should return first inserted value",
        );
        kani::assert(
            map.get_last(&key) == Some(&v2),
            "get_last should return last inserted value",
        );
    }

    #[kani::proof]
    fn proof_multimap_remove_returns_all() {
        let mut map: MultiMap<u8, u8> = MultiMap::new();
        let key: u8 = kani::any();
        let v1: u8 = kani::any();
        let v2: u8 = kani::any();

        map.insert(key, v1);
        map.insert(key, v2);

        let removed = map.remove(&key);

        kani::assert(removed.is_some(), "Remove should return Some");
        kani::assert(removed.unwrap().len() == 2, "Should return all values");
        kani::assert(!map.contains_key(&key), "Key should not exist after remove");
        kani::assert(
            map.is_empty(),
            "Map should be empty after removing only key",
        );
    }

    #[kani::proof]
    fn proof_multimap_remove_value_decreases_count() {
        let mut map: MultiMap<u8, u8> = MultiMap::new();
        let key: u8 = kani::any();
        let v1: u8 = kani::any();
        let v2: u8 = kani::any();
        kani::assume(v1 != v2); // Ensure distinct values

        map.insert(key, v1);
        map.insert(key, v2);

        let removed = map.remove_value(&key, &v1);

        kani::assert(removed, "Should successfully remove existing value");
        kani::assert(map.total_values() == 1, "Should have one value remaining");
        kani::assert(map.contains_key(&key), "Key should still exist");
    }

    #[kani::proof]
    fn proof_multimap_remove_last_value_removes_key() {
        let mut map: MultiMap<u8, u8> = MultiMap::new();
        let key: u8 = kani::any();
        let value: u8 = kani::any();

        map.insert(key, value);
        let removed = map.remove_value(&key, &value);

        kani::assert(removed, "Should successfully remove value");
        kani::assert(
            !map.contains_key(&key),
            "Key should be removed when last value removed",
        );
        kani::assert(map.is_empty(), "Map should be empty");
    }

    #[kani::proof]
    fn proof_multimap_remove_nonexistent_value() {
        let mut map: MultiMap<u8, u8> = MultiMap::new();
        let key: u8 = kani::any();
        let v1: u8 = kani::any();
        let v2: u8 = kani::any();
        kani::assume(v1 != v2);

        map.insert(key, v1);
        let removed = map.remove_value(&key, &v2);

        kani::assert(!removed, "Should return false for nonexistent value");
        kani::assert(map.total_values() == 1, "Count should be unchanged");
    }

    #[kani::proof]
    fn proof_multimap_clear() {
        let mut map: MultiMap<u8, u8> = MultiMap::new();
        let k1: u8 = kani::any();
        let k2: u8 = kani::any();
        let v1: u8 = kani::any();
        let v2: u8 = kani::any();
        kani::assume(k1 != k2);

        map.insert(k1, v1);
        map.insert(k2, v2);
        map.clear();

        kani::assert(map.is_empty(), "Map should be empty after clear");
        kani::assert(map.len() == 0, "Key count should be zero after clear");
        kani::assert(
            map.total_values() == 0,
            "Value count should be zero after clear",
        );
    }

    #[kani::proof]
    fn proof_multimap_is_empty_consistency() {
        let mut map: MultiMap<u8, u8> = MultiMap::new();
        let key: u8 = kani::any();
        let value: u8 = kani::any();

        kani::assert(
            map.is_empty() == (map.len() == 0),
            "is_empty should equal len==0",
        );

        map.insert(key, value);
        kani::assert(
            map.is_empty() == (map.len() == 0),
            "is_empty should equal len==0",
        );
    }

    #[kani::proof]
    fn proof_multimap_multiple_keys() {
        let mut map: MultiMap<u8, u8> = MultiMap::new();
        let k1: u8 = kani::any();
        let k2: u8 = kani::any();
        let v1: u8 = kani::any();
        let v2: u8 = kani::any();
        kani::assume(k1 != k2);

        map.insert(k1, v1);
        map.insert(k2, v2);

        kani::assert(map.len() == 2, "Should have two keys");
        kani::assert(map.total_values() == 2, "Should have two values");
        kani::assert(map.contains_key(&k1), "Should contain first key");
        kani::assert(map.contains_key(&k2), "Should contain second key");
    }

    // ------------------------------------------------------------------------
    // MultiMapSet Proofs
    // ------------------------------------------------------------------------

    #[kani::proof]
    fn proof_multimap_set_new_is_empty() {
        let map: MultiMapSet<u8, u8> = MultiMapSet::new();
        kani::assert(map.is_empty(), "New multimap set should be empty");
        kani::assert(map.len() == 0, "New multimap set should have zero keys");
        kani::assert(
            map.total_values() == 0,
            "New multimap set should have zero values",
        );
    }

    #[kani::proof]
    fn proof_multimap_set_insert_creates_key() {
        let mut map: MultiMapSet<u8, u8> = MultiMapSet::new();
        let key: u8 = kani::any();
        let value: u8 = kani::any();

        let inserted = map.insert(key, value);

        kani::assert(inserted, "First insert should return true");
        kani::assert(map.contains_key(&key), "Key should exist after insert");
        kani::assert(!map.is_empty(), "Map should not be empty after insert");
    }

    #[kani::proof]
    fn proof_multimap_set_rejects_duplicates() {
        let mut map: MultiMapSet<u8, u8> = MultiMapSet::new();
        let key: u8 = kani::any();
        let value: u8 = kani::any();

        let first = map.insert(key, value);
        let second = map.insert(key, value); // Same value again

        kani::assert(first, "First insert should return true");
        kani::assert(!second, "Duplicate insert should return false");
        kani::assert(map.total_values() == 1, "Should have only one value");
    }

    #[kani::proof]
    fn proof_multimap_set_allows_different_values() {
        let mut map: MultiMapSet<u8, u8> = MultiMapSet::new();
        let key: u8 = kani::any();
        let v1: u8 = kani::any();
        let v2: u8 = kani::any();
        kani::assume(v1 != v2);

        let first = map.insert(key, v1);
        let second = map.insert(key, v2);

        kani::assert(first, "First insert should return true");
        kani::assert(second, "Different value insert should return true");
        kani::assert(map.total_values() == 2, "Should have two distinct values");
    }

    #[kani::proof]
    fn proof_multimap_set_contains() {
        let mut map: MultiMapSet<u8, u8> = MultiMapSet::new();
        let key: u8 = kani::any();
        let v1: u8 = kani::any();
        let v2: u8 = kani::any();
        kani::assume(v1 != v2);

        map.insert(key, v1);

        kani::assert(map.contains(&key, &v1), "Should contain inserted value");
        kani::assert(
            !map.contains(&key, &v2),
            "Should not contain non-inserted value",
        );
    }

    #[kani::proof]
    fn proof_multimap_set_remove_value() {
        let mut map: MultiMapSet<u8, u8> = MultiMapSet::new();
        let key: u8 = kani::any();
        let v1: u8 = kani::any();
        let v2: u8 = kani::any();
        kani::assume(v1 != v2);

        map.insert(key, v1);
        map.insert(key, v2);

        let removed = map.remove_value(&key, &v1);

        kani::assert(removed, "Should successfully remove existing value");
        kani::assert(!map.contains(&key, &v1), "Should not contain removed value");
        kani::assert(map.contains(&key, &v2), "Should still contain other value");
        kani::assert(map.total_values() == 1, "Should have one value remaining");
    }

    #[kani::proof]
    fn proof_multimap_set_remove_last_value_removes_key() {
        let mut map: MultiMapSet<u8, u8> = MultiMapSet::new();
        let key: u8 = kani::any();
        let value: u8 = kani::any();

        map.insert(key, value);
        let removed = map.remove_value(&key, &value);

        kani::assert(removed, "Should successfully remove value");
        kani::assert(
            !map.contains_key(&key),
            "Key should be removed when last value removed",
        );
        kani::assert(map.is_empty(), "Map should be empty");
    }

    #[kani::proof]
    fn proof_multimap_set_remove_all() {
        let mut map: MultiMapSet<u8, u8> = MultiMapSet::new();
        let key: u8 = kani::any();
        let v1: u8 = kani::any();
        let v2: u8 = kani::any();
        kani::assume(v1 != v2);

        map.insert(key, v1);
        map.insert(key, v2);

        let removed = map.remove(&key);

        kani::assert(removed.is_some(), "Remove should return Some");
        kani::assert(removed.unwrap().len() == 2, "Should return all values");
        kani::assert(!map.contains_key(&key), "Key should not exist after remove");
    }

    #[kani::proof]
    fn proof_multimap_set_clear() {
        let mut map: MultiMapSet<u8, u8> = MultiMapSet::new();
        let k1: u8 = kani::any();
        let k2: u8 = kani::any();
        let v1: u8 = kani::any();
        let v2: u8 = kani::any();
        kani::assume(k1 != k2);

        map.insert(k1, v1);
        map.insert(k2, v2);
        map.clear();

        kani::assert(map.is_empty(), "Map should be empty after clear");
        kani::assert(map.len() == 0, "Key count should be zero after clear");
        kani::assert(
            map.total_values() == 0,
            "Value count should be zero after clear",
        );
    }

    #[kani::proof]
    fn proof_multimap_set_is_empty_consistency() {
        let mut map: MultiMapSet<u8, u8> = MultiMapSet::new();
        let key: u8 = kani::any();
        let value: u8 = kani::any();

        kani::assert(
            map.is_empty() == (map.len() == 0),
            "is_empty should equal len==0",
        );

        map.insert(key, value);
        kani::assert(
            map.is_empty() == (map.len() == 0),
            "is_empty should equal len==0",
        );
    }

    #[kani::proof]
    fn proof_multimap_set_contains_key_after_insert() {
        let mut map: MultiMapSet<u8, u8> = MultiMapSet::new();
        let key: u8 = kani::any();
        let value: u8 = kani::any();

        kani::assert(
            !map.contains_key(&key),
            "Should not contain key before insert",
        );
        map.insert(key, value);
        kani::assert(map.contains_key(&key), "Should contain key after insert");
    }

    #[kani::proof]
    fn proof_multimap_set_get_consistency() {
        let mut map: MultiMapSet<u8, u8> = MultiMapSet::new();
        let key: u8 = kani::any();
        let value: u8 = kani::any();

        kani::assert(
            map.get(&key).is_none(),
            "Get should return None for missing key",
        );

        map.insert(key, value);
        let result = map.get(&key);

        kani::assert(result.is_some(), "Get should return Some after insert");
        kani::assert(
            result.unwrap().contains(&value),
            "Set should contain inserted value",
        );
    }
}
