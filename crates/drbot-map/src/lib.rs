//! Generic map utilities for drbot.
//!
//! This crate provides:
//! - Map extension traits
//! - Map merging utilities
//! - Key/value transformations
//! - Map comparison

use std::collections::HashMap;
use std::hash::Hash;
use thiserror::Error;

/// Map error types.
#[derive(Error, Debug)]
pub enum MapError {
    #[error("Key not found")]
    KeyNotFound,

    #[error("Key already exists")]
    KeyExists,

    #[error("Merge conflict")]
    MergeConflict,
}

/// Result type for map operations.
pub type Result<T> = std::result::Result<T, MapError>;

/// Extension trait for HashMap.
pub trait MapExt<K, V> {
    /// Get or insert with default.
    fn get_or_default(&mut self, key: K) -> &mut V
    where
        V: Default;

    /// Get or insert with function.
    fn get_or_insert_with<F>(&mut self, key: K, f: F) -> &mut V
    where
        F: FnOnce() -> V;

    /// Update existing value.
    fn update<F>(&mut self, key: &K, f: F) -> Option<&V>
    where
        F: FnOnce(&mut V);

    /// Get multiple keys.
    fn get_many<'a, I>(&'a self, keys: I) -> Vec<Option<&'a V>>
    where
        I: IntoIterator<Item = &'a K>,
        K: 'a;

    /// Remove multiple keys.
    fn remove_many<'a, I>(&mut self, keys: I) -> Vec<Option<V>>
    where
        I: IntoIterator<Item = &'a K>,
        K: 'a;
}

impl<K: Hash + Eq + Clone, V> MapExt<K, V> for HashMap<K, V> {
    fn get_or_default(&mut self, key: K) -> &mut V
    where
        V: Default,
    {
        self.entry(key).or_default()
    }

    fn get_or_insert_with<F>(&mut self, key: K, f: F) -> &mut V
    where
        F: FnOnce() -> V,
    {
        self.entry(key).or_insert_with(f)
    }

    fn update<F>(&mut self, key: &K, f: F) -> Option<&V>
    where
        F: FnOnce(&mut V),
    {
        if let Some(v) = self.get_mut(key) {
            f(v);
            Some(v)
        } else {
            None
        }
    }

    fn get_many<'a, I>(&'a self, keys: I) -> Vec<Option<&'a V>>
    where
        I: IntoIterator<Item = &'a K>,
        K: 'a,
    {
        keys.into_iter().map(|k| self.get(k)).collect()
    }

    fn remove_many<'a, I>(&mut self, keys: I) -> Vec<Option<V>>
    where
        I: IntoIterator<Item = &'a K>,
        K: 'a,
    {
        keys.into_iter().map(|k| self.remove(k)).collect()
    }
}

/// Merge strategy for combining maps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeStrategy {
    /// Keep values from first map.
    KeepFirst,
    /// Keep values from second map.
    KeepSecond,
    /// Error on conflict.
    ErrorOnConflict,
}

/// Merge two maps.
pub fn merge<K, V>(
    mut first: HashMap<K, V>,
    second: HashMap<K, V>,
    strategy: MergeStrategy,
) -> Result<HashMap<K, V>>
where
    K: Hash + Eq,
{
    for (k, v) in second {
        match first.entry(k) {
            std::collections::hash_map::Entry::Occupied(mut entry) => match strategy {
                MergeStrategy::KeepFirst => {}
                MergeStrategy::KeepSecond => {
                    entry.insert(v);
                }
                MergeStrategy::ErrorOnConflict => {
                    return Err(MapError::MergeConflict);
                }
            },
            std::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert(v);
            }
        }
    }
    Ok(first)
}

/// Map keys transformation.
pub fn map_keys<K1, K2, V, F>(map: HashMap<K1, V>, f: F) -> HashMap<K2, V>
where
    K2: Hash + Eq,
    F: Fn(K1) -> K2,
{
    map.into_iter().map(|(k, v)| (f(k), v)).collect()
}

/// Map values transformation.
pub fn map_values<K, V1, V2, F>(map: HashMap<K, V1>, f: F) -> HashMap<K, V2>
where
    K: Hash + Eq,
    F: Fn(V1) -> V2,
{
    map.into_iter().map(|(k, v)| (k, f(v))).collect()
}

/// Filter map by key.
pub fn filter_keys<K, V, F>(map: HashMap<K, V>, f: F) -> HashMap<K, V>
where
    K: Hash + Eq,
    F: Fn(&K) -> bool,
{
    map.into_iter().filter(|(k, _)| f(k)).collect()
}

/// Filter map by value.
pub fn filter_values<K, V, F>(map: HashMap<K, V>, f: F) -> HashMap<K, V>
where
    K: Hash + Eq,
    F: Fn(&V) -> bool,
{
    map.into_iter().filter(|(_, v)| f(v)).collect()
}

/// Invert a map (swap keys and values).
pub fn invert<K, V>(map: HashMap<K, V>) -> HashMap<V, K>
where
    K: Hash + Eq,
    V: Hash + Eq,
{
    map.into_iter().map(|(k, v)| (v, k)).collect()
}

/// Get difference between maps (keys in first but not second).
pub fn difference<K, V>(first: &HashMap<K, V>, second: &HashMap<K, V>) -> HashMap<K, V>
where
    K: Hash + Eq + Clone,
    V: Clone,
{
    first
        .iter()
        .filter(|(k, _)| !second.contains_key(*k))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

/// Get intersection of maps (keys in both).
pub fn intersection<K, V>(first: &HashMap<K, V>, second: &HashMap<K, V>) -> HashMap<K, V>
where
    K: Hash + Eq + Clone,
    V: Clone,
{
    first
        .iter()
        .filter(|(k, _)| second.contains_key(*k))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

/// Symmetric difference (keys in one but not both).
pub fn symmetric_difference<K, V>(first: &HashMap<K, V>, second: &HashMap<K, V>) -> HashMap<K, V>
where
    K: Hash + Eq + Clone,
    V: Clone,
{
    let mut result = difference(first, second);
    result.extend(difference(second, first));
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_map_ext() {
        let mut map: HashMap<String, i32> = HashMap::new();

        *map.get_or_default("count".to_string()) = 5;
        assert_eq!(map.get("count"), Some(&5));

        map.update(&"count".to_string(), |v| *v += 1);
        assert_eq!(map.get("count"), Some(&6));
    }

    #[test]
    fn test_merge() {
        let mut first = HashMap::new();
        first.insert("a", 1);
        first.insert("b", 2);

        let mut second = HashMap::new();
        second.insert("b", 3);
        second.insert("c", 4);

        let merged = merge(first, second, MergeStrategy::KeepSecond).unwrap();
        assert_eq!(merged.get(&"a"), Some(&1));
        assert_eq!(merged.get(&"b"), Some(&3));
        assert_eq!(merged.get(&"c"), Some(&4));
    }

    #[test]
    fn test_invert() {
        let mut map = HashMap::new();
        map.insert("a", 1);
        map.insert("b", 2);

        let inverted = invert(map);
        assert_eq!(inverted.get(&1), Some(&"a"));
        assert_eq!(inverted.get(&2), Some(&"b"));
    }

    #[test]
    fn test_difference() {
        let mut first = HashMap::new();
        first.insert("a", 1);
        first.insert("b", 2);

        let mut second = HashMap::new();
        second.insert("b", 2);
        second.insert("c", 3);

        let diff = difference(&first, &second);
        assert!(diff.contains_key(&"a"));
        assert!(!diff.contains_key(&"b"));
    }
}
