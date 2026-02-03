//! Bidirectional map for drbot.
//!
//! This crate provides:
//! - BiMap with O(1) lookup in both directions
//! - Insertion/removal maintaining consistency
//! - Iteration over pairs, keys, and values

use std::collections::HashMap;
use std::hash::Hash;
use thiserror::Error;

/// BiMap error types.
#[derive(Error, Debug)]
pub enum BiMapError {
    #[error("Key not found")]
    KeyNotFound,

    #[error("Value not found")]
    ValueNotFound,

    #[error("Key already exists")]
    KeyExists,

    #[error("Value already exists")]
    ValueExists,
}

/// Result type for bimap operations.
pub type Result<T> = std::result::Result<T, BiMapError>;

/// Bidirectional map.
#[derive(Debug, Clone)]
pub struct BiMap<K, V> {
    forward: HashMap<K, V>,
    backward: HashMap<V, K>,
}

impl<K, V> BiMap<K, V>
where
    K: Hash + Eq + Clone,
    V: Hash + Eq + Clone,
{
    /// Create new empty bimap.
    pub fn new() -> Self {
        Self {
            forward: HashMap::new(),
            backward: HashMap::new(),
        }
    }

    /// Create with capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            forward: HashMap::with_capacity(capacity),
            backward: HashMap::with_capacity(capacity),
        }
    }

    /// Insert a key-value pair.
    /// Returns old value if key existed, or old key if value existed.
    pub fn insert(&mut self, key: K, value: V) -> Option<(Option<V>, Option<K>)> {
        let old_value = self.forward.remove(&key);
        let old_key = self.backward.remove(&value);

        if let Some(ref v) = old_value {
            self.backward.remove(v);
        }
        if let Some(ref k) = old_key {
            self.forward.remove(k);
        }

        self.forward.insert(key.clone(), value.clone());
        self.backward.insert(value, key);

        if old_value.is_some() || old_key.is_some() {
            Some((old_value, old_key))
        } else {
            None
        }
    }

    /// Insert only if neither key nor value exists.
    pub fn insert_no_overwrite(&mut self, key: K, value: V) -> Result<()> {
        if self.forward.contains_key(&key) {
            return Err(BiMapError::KeyExists);
        }
        if self.backward.contains_key(&value) {
            return Err(BiMapError::ValueExists);
        }

        self.forward.insert(key.clone(), value.clone());
        self.backward.insert(value, key);
        Ok(())
    }

    /// Get value by key.
    pub fn get_by_key(&self, key: &K) -> Option<&V> {
        self.forward.get(key)
    }

    /// Get key by value.
    pub fn get_by_value(&self, value: &V) -> Option<&K> {
        self.backward.get(value)
    }

    /// Check if key exists.
    pub fn contains_key(&self, key: &K) -> bool {
        self.forward.contains_key(key)
    }

    /// Check if value exists.
    pub fn contains_value(&self, value: &V) -> bool {
        self.backward.contains_key(value)
    }

    /// Remove by key.
    pub fn remove_by_key(&mut self, key: &K) -> Option<V> {
        if let Some(value) = self.forward.remove(key) {
            self.backward.remove(&value);
            Some(value)
        } else {
            None
        }
    }

    /// Remove by value.
    pub fn remove_by_value(&mut self, value: &V) -> Option<K> {
        if let Some(key) = self.backward.remove(value) {
            self.forward.remove(&key);
            Some(key)
        } else {
            None
        }
    }

    /// Get number of pairs.
    pub fn len(&self) -> usize {
        self.forward.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.forward.is_empty()
    }

    /// Clear all entries.
    pub fn clear(&mut self) {
        self.forward.clear();
        self.backward.clear();
    }

    /// Iterate over key-value pairs.
    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.forward.iter()
    }

    /// Iterate over keys.
    pub fn keys(&self) -> impl Iterator<Item = &K> {
        self.forward.keys()
    }

    /// Iterate over values.
    pub fn values(&self) -> impl Iterator<Item = &V> {
        self.forward.values()
    }
}

impl<K, V> Default for BiMap<K, V>
where
    K: Hash + Eq + Clone,
    V: Hash + Eq + Clone,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<K, V> FromIterator<(K, V)> for BiMap<K, V>
where
    K: Hash + Eq + Clone,
    V: Hash + Eq + Clone,
{
    fn from_iter<I: IntoIterator<Item = (K, V)>>(iter: I) -> Self {
        let mut bimap = BiMap::new();
        for (k, v) in iter {
            bimap.insert(k, v);
        }
        bimap
    }
}

impl<K, V> IntoIterator for BiMap<K, V> {
    type Item = (K, V);
    type IntoIter = std::collections::hash_map::IntoIter<K, V>;

    fn into_iter(self) -> Self::IntoIter {
        self.forward.into_iter()
    }
}

// ============================================================================
// Kani Formal Verification Proofs
// ============================================================================

#[cfg(kani)]
mod kani_proofs {
    use super::*;

    // ========================================================================
    // BiMap Basic Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_bimap_empty_initially() {
        let bimap: BiMap<i32, i32> = BiMap::new();

        kani::assert!(bimap.is_empty(), "New BiMap is empty");
        kani::assert!(bimap.len() == 0, "New BiMap has len 0");
    }

    #[kani::proof]
    fn proof_bimap_insert_increases_len() {
        let mut bimap: BiMap<i32, i32> = BiMap::new();

        let result = bimap.insert(1, 100);

        kani::assert!(result.is_none(), "First insert returns None");
        kani::assert!(bimap.len() == 1, "Len is 1 after insert");
        kani::assert!(!bimap.is_empty(), "BiMap not empty after insert");
    }

    #[kani::proof]
    fn proof_bimap_bidirectional_lookup() {
        let mut bimap: BiMap<i32, i32> = BiMap::new();

        bimap.insert(1, 100);

        kani::assert!(bimap.get_by_key(&1) == Some(&100), "Forward lookup works");
        kani::assert!(
            bimap.get_by_value(&100) == Some(&1),
            "Backward lookup works"
        );
    }

    #[kani::proof]
    fn proof_bimap_consistency_invariant() {
        let mut bimap: BiMap<i32, i32> = BiMap::new();

        bimap.insert(1, 100);
        bimap.insert(2, 200);

        // forward.len() == backward.len() always
        let forward_len = bimap.len();

        // If forward has key, backward must have value pointing back
        if let Some(&v) = bimap.get_by_key(&1) {
            kani::assert!(
                bimap.get_by_value(&v) == Some(&1),
                "Consistency: key -> value -> key"
            );
        }

        if let Some(&k) = bimap.get_by_value(&100) {
            kani::assert!(
                bimap.get_by_key(&k) == Some(&100),
                "Consistency: value -> key -> value"
            );
        }

        kani::assert!(forward_len == 2, "Len matches insert count");
    }

    #[kani::proof]
    fn proof_bimap_contains_key_and_value() {
        let mut bimap: BiMap<i32, i32> = BiMap::new();

        bimap.insert(1, 100);

        kani::assert!(bimap.contains_key(&1), "Contains key after insert");
        kani::assert!(bimap.contains_value(&100), "Contains value after insert");
        kani::assert!(!bimap.contains_key(&2), "Doesn't contain other key");
        kani::assert!(!bimap.contains_value(&200), "Doesn't contain other value");
    }

    // ========================================================================
    // BiMap Insert Overwrite Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_bimap_insert_overwrite_key() {
        let mut bimap: BiMap<i32, i32> = BiMap::new();

        bimap.insert(1, 100);
        bimap.insert(1, 200); // Overwrite key 1

        kani::assert!(bimap.get_by_key(&1) == Some(&200), "Key maps to new value");
        kani::assert!(bimap.get_by_value(&100).is_none(), "Old value removed");
        kani::assert!(
            bimap.get_by_value(&200) == Some(&1),
            "New value maps to key"
        );
        kani::assert!(bimap.len() == 1, "Len still 1 after overwrite");
    }

    #[kani::proof]
    fn proof_bimap_insert_overwrite_value() {
        let mut bimap: BiMap<i32, i32> = BiMap::new();

        bimap.insert(1, 100);
        bimap.insert(2, 100); // Overwrite value 100

        kani::assert!(
            bimap.get_by_value(&100) == Some(&2),
            "Value maps to new key"
        );
        kani::assert!(bimap.get_by_key(&1).is_none(), "Old key removed");
        kani::assert!(bimap.get_by_key(&2) == Some(&100), "New key maps to value");
        kani::assert!(bimap.len() == 1, "Len still 1 after overwrite");
    }

    // ========================================================================
    // BiMap Insert No Overwrite Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_bimap_insert_no_overwrite_success() {
        let mut bimap: BiMap<i32, i32> = BiMap::new();

        let result = bimap.insert_no_overwrite(1, 100);

        kani::assert!(result.is_ok(), "Insert succeeds when key/value don't exist");
        kani::assert!(bimap.len() == 1, "Len is 1 after insert");
    }

    #[kani::proof]
    fn proof_bimap_insert_no_overwrite_key_exists() {
        let mut bimap: BiMap<i32, i32> = BiMap::new();

        bimap.insert(1, 100);
        let result = bimap.insert_no_overwrite(1, 200);

        kani::assert!(result.is_err(), "Insert fails when key exists");
        kani::assert!(
            bimap.get_by_key(&1) == Some(&100),
            "Original value unchanged"
        );
    }

    #[kani::proof]
    fn proof_bimap_insert_no_overwrite_value_exists() {
        let mut bimap: BiMap<i32, i32> = BiMap::new();

        bimap.insert(1, 100);
        let result = bimap.insert_no_overwrite(2, 100);

        kani::assert!(result.is_err(), "Insert fails when value exists");
        kani::assert!(
            bimap.get_by_value(&100) == Some(&1),
            "Original key unchanged"
        );
    }

    // ========================================================================
    // BiMap Remove Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_bimap_remove_by_key() {
        let mut bimap: BiMap<i32, i32> = BiMap::new();

        bimap.insert(1, 100);
        let removed = bimap.remove_by_key(&1);

        kani::assert!(removed == Some(100), "Remove returns value");
        kani::assert!(bimap.is_empty(), "BiMap empty after remove");
        kani::assert!(!bimap.contains_key(&1), "Key removed");
        kani::assert!(!bimap.contains_value(&100), "Value also removed");
    }

    #[kani::proof]
    fn proof_bimap_remove_by_value() {
        let mut bimap: BiMap<i32, i32> = BiMap::new();

        bimap.insert(1, 100);
        let removed = bimap.remove_by_value(&100);

        kani::assert!(removed == Some(1), "Remove returns key");
        kani::assert!(bimap.is_empty(), "BiMap empty after remove");
        kani::assert!(!bimap.contains_key(&1), "Key also removed");
        kani::assert!(!bimap.contains_value(&100), "Value removed");
    }

    #[kani::proof]
    fn proof_bimap_remove_nonexistent() {
        let mut bimap: BiMap<i32, i32> = BiMap::new();

        let by_key = bimap.remove_by_key(&1);
        let by_value = bimap.remove_by_value(&100);

        kani::assert!(by_key.is_none(), "Remove nonexistent key returns None");
        kani::assert!(by_value.is_none(), "Remove nonexistent value returns None");
    }

    #[kani::proof]
    fn proof_bimap_remove_preserves_others() {
        let mut bimap: BiMap<i32, i32> = BiMap::new();

        bimap.insert(1, 100);
        bimap.insert(2, 200);

        bimap.remove_by_key(&1);

        kani::assert!(bimap.get_by_key(&2) == Some(&200), "Other key preserved");
        kani::assert!(
            bimap.get_by_value(&200) == Some(&2),
            "Other value preserved"
        );
        kani::assert!(bimap.len() == 1, "Len is 1 after partial remove");
    }

    // ========================================================================
    // BiMap Clear Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_bimap_clear() {
        let mut bimap: BiMap<i32, i32> = BiMap::new();

        bimap.insert(1, 100);
        bimap.insert(2, 200);
        bimap.clear();

        kani::assert!(bimap.is_empty(), "BiMap empty after clear");
        kani::assert!(bimap.len() == 0, "Len is 0 after clear");
    }

    // ========================================================================
    // BiMap Multiple Operations Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_bimap_multiple_inserts() {
        let mut bimap: BiMap<i32, i32> = BiMap::new();

        bimap.insert(1, 100);
        bimap.insert(2, 200);
        bimap.insert(3, 300);

        kani::assert!(bimap.len() == 3, "Len is 3 after 3 inserts");
        kani::assert!(bimap.get_by_key(&1) == Some(&100), "First pair intact");
        kani::assert!(bimap.get_by_key(&2) == Some(&200), "Second pair intact");
        kani::assert!(bimap.get_by_key(&3) == Some(&300), "Third pair intact");
    }

    #[kani::proof]
    fn proof_bimap_len_consistency() {
        let mut bimap: BiMap<i32, i32> = BiMap::new();

        bimap.insert(1, 100);
        let len1 = bimap.len();

        bimap.insert(2, 200);
        let len2 = bimap.len();

        bimap.remove_by_key(&1);
        let len3 = bimap.len();

        kani::assert!(len1 == 1, "Len is 1 after first insert");
        kani::assert!(len2 == 2, "Len is 2 after second insert");
        kani::assert!(len3 == 1, "Len is 1 after remove");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert_and_get() {
        let mut bimap = BiMap::new();
        bimap.insert("hello", 1);
        bimap.insert("world", 2);

        assert_eq!(bimap.get_by_key(&"hello"), Some(&1));
        assert_eq!(bimap.get_by_key(&"world"), Some(&2));
        assert_eq!(bimap.get_by_value(&1), Some(&"hello"));
        assert_eq!(bimap.get_by_value(&2), Some(&"world"));
    }

    #[test]
    fn test_overwrite() {
        let mut bimap = BiMap::new();
        bimap.insert("a", 1);
        bimap.insert("a", 2); // Overwrites key "a"

        assert_eq!(bimap.get_by_key(&"a"), Some(&2));
        assert_eq!(bimap.get_by_value(&1), None);
        assert_eq!(bimap.get_by_value(&2), Some(&"a"));
    }

    #[test]
    fn test_remove() {
        let mut bimap = BiMap::new();
        bimap.insert("key", "value");

        assert_eq!(bimap.remove_by_key(&"key"), Some("value"));
        assert!(bimap.is_empty());
    }

    #[test]
    fn test_no_overwrite() {
        let mut bimap = BiMap::new();
        bimap.insert("a", 1);

        assert!(bimap.insert_no_overwrite("a", 2).is_err());
        assert!(bimap.insert_no_overwrite("b", 1).is_err());
        assert!(bimap.insert_no_overwrite("b", 2).is_ok());
    }
}
