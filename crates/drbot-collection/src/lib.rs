//! Collection utilities for drbot.
//!
//! This crate provides:
//! - Vec extensions
//! - HashMap extensions
//! - HashSet extensions
//! - Collection builders

use std::collections::{HashMap, HashSet};
use std::hash::Hash;

/// Vec extension trait.
pub trait VecExt<T> {
    /// Get first element.
    fn first_or(&self, default: T) -> T
    where
        T: Clone;

    /// Get last element.
    fn last_or(&self, default: T) -> T
    where
        T: Clone;

    /// Safe get by index.
    fn get_or(&self, index: usize, default: T) -> T
    where
        T: Clone;

    /// Remove duplicates (preserving order).
    fn dedup_by_key<K, F>(&mut self, key: F)
    where
        K: Eq + Hash,
        F: Fn(&T) -> K;

    /// Partition into two vecs.
    fn partition_by<F>(&self, predicate: F) -> (Vec<T>, Vec<T>)
    where
        T: Clone,
        F: Fn(&T) -> bool;

    /// Chunk into groups of n.
    fn chunks_exact_vec(&self, size: usize) -> Vec<Vec<T>>
    where
        T: Clone;

    /// Interleave with another vec.
    fn interleave(&self, other: &[T]) -> Vec<T>
    where
        T: Clone;

    /// Rotate left by n positions.
    fn rotate_left_vec(&self, n: usize) -> Vec<T>
    where
        T: Clone;

    /// Rotate right by n positions.
    fn rotate_right_vec(&self, n: usize) -> Vec<T>
    where
        T: Clone;
}

impl<T> VecExt<T> for Vec<T> {
    fn first_or(&self, default: T) -> T
    where
        T: Clone,
    {
        self.first().cloned().unwrap_or(default)
    }

    fn last_or(&self, default: T) -> T
    where
        T: Clone,
    {
        self.last().cloned().unwrap_or(default)
    }

    fn get_or(&self, index: usize, default: T) -> T
    where
        T: Clone,
    {
        self.get(index).cloned().unwrap_or(default)
    }

    fn dedup_by_key<K, F>(&mut self, key: F)
    where
        K: Eq + Hash,
        F: Fn(&T) -> K,
    {
        let mut seen = HashSet::new();
        self.retain(|item| seen.insert(key(item)));
    }

    fn partition_by<F>(&self, predicate: F) -> (Vec<T>, Vec<T>)
    where
        T: Clone,
        F: Fn(&T) -> bool,
    {
        let mut true_vec = Vec::new();
        let mut false_vec = Vec::new();

        for item in self {
            if predicate(item) {
                true_vec.push(item.clone());
            } else {
                false_vec.push(item.clone());
            }
        }

        (true_vec, false_vec)
    }

    fn chunks_exact_vec(&self, size: usize) -> Vec<Vec<T>>
    where
        T: Clone,
    {
        self.chunks(size).map(|c| c.to_vec()).collect()
    }

    fn interleave(&self, other: &[T]) -> Vec<T>
    where
        T: Clone,
    {
        let mut result = Vec::with_capacity(self.len() + other.len());
        let mut self_iter = self.iter();
        let mut other_iter = other.iter();

        loop {
            match (self_iter.next(), other_iter.next()) {
                (Some(a), Some(b)) => {
                    result.push(a.clone());
                    result.push(b.clone());
                }
                (Some(a), None) => {
                    result.push(a.clone());
                    result.extend(self_iter.cloned());
                    break;
                }
                (None, Some(b)) => {
                    result.push(b.clone());
                    result.extend(other_iter.cloned());
                    break;
                }
                (None, None) => break,
            }
        }

        result
    }

    fn rotate_left_vec(&self, n: usize) -> Vec<T>
    where
        T: Clone,
    {
        if self.is_empty() {
            return Vec::new();
        }
        let n = n % self.len();
        let mut result = self[n..].to_vec();
        result.extend_from_slice(&self[..n]);
        result
    }

    fn rotate_right_vec(&self, n: usize) -> Vec<T>
    where
        T: Clone,
    {
        if self.is_empty() {
            return Vec::new();
        }
        let n = n % self.len();
        self.rotate_left_vec(self.len() - n)
    }
}

/// HashMap extension trait.
pub trait HashMapExt<K, V> {
    /// Get or insert with default.
    fn get_or_insert(&mut self, key: K, default: V) -> &mut V
    where
        K: Eq + Hash;

    /// Get or insert with closure.
    fn get_or_insert_with<F>(&mut self, key: K, f: F) -> &mut V
    where
        K: Eq + Hash,
        F: FnOnce() -> V;

    /// Increment numeric value.
    fn increment(&mut self, key: K)
    where
        K: Eq + Hash,
        V: Default + std::ops::AddAssign<i32>;

    /// Merge with another map.
    fn merge(&mut self, other: HashMap<K, V>)
    where
        K: Eq + Hash;

    /// Invert map (swap keys and values).
    fn invert(&self) -> HashMap<V, K>
    where
        K: Clone,
        V: Eq + Hash + Clone;

    /// Filter by predicate.
    fn filter_by<F>(&self, predicate: F) -> HashMap<K, V>
    where
        K: Clone + Eq + Hash,
        V: Clone,
        F: Fn(&K, &V) -> bool;

    /// Map values.
    fn map_values<U, F>(&self, f: F) -> HashMap<K, U>
    where
        K: Clone + Eq + Hash,
        F: Fn(&V) -> U;
}

impl<K, V> HashMapExt<K, V> for HashMap<K, V> {
    fn get_or_insert(&mut self, key: K, default: V) -> &mut V
    where
        K: Eq + Hash,
    {
        self.entry(key).or_insert(default)
    }

    fn get_or_insert_with<F>(&mut self, key: K, f: F) -> &mut V
    where
        K: Eq + Hash,
        F: FnOnce() -> V,
    {
        self.entry(key).or_insert_with(f)
    }

    fn increment(&mut self, key: K)
    where
        K: Eq + Hash,
        V: Default + std::ops::AddAssign<i32>,
    {
        let entry = self.entry(key).or_insert_with(V::default);
        *entry += 1;
    }

    fn merge(&mut self, other: HashMap<K, V>)
    where
        K: Eq + Hash,
    {
        for (key, value) in other {
            self.insert(key, value);
        }
    }

    fn invert(&self) -> HashMap<V, K>
    where
        K: Clone,
        V: Eq + Hash + Clone,
    {
        self.iter().map(|(k, v)| (v.clone(), k.clone())).collect()
    }

    fn filter_by<F>(&self, predicate: F) -> HashMap<K, V>
    where
        K: Clone + Eq + Hash,
        V: Clone,
        F: Fn(&K, &V) -> bool,
    {
        self.iter()
            .filter(|(k, v)| predicate(k, v))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    fn map_values<U, F>(&self, f: F) -> HashMap<K, U>
    where
        K: Clone + Eq + Hash,
        F: Fn(&V) -> U,
    {
        self.iter().map(|(k, v)| (k.clone(), f(v))).collect()
    }
}

/// HashSet extension trait.
pub trait HashSetExt<T> {
    /// Add multiple items.
    fn add_all<I: IntoIterator<Item = T>>(&mut self, items: I)
    where
        T: Eq + Hash;

    /// Symmetric difference (XOR).
    fn symmetric_difference_owned(&self, other: &HashSet<T>) -> HashSet<T>
    where
        T: Eq + Hash + Clone;

    /// Check if subset.
    fn is_subset_of(&self, other: &HashSet<T>) -> bool
    where
        T: Eq + Hash;

    /// Check if superset.
    fn is_superset_of(&self, other: &HashSet<T>) -> bool
    where
        T: Eq + Hash;
}

impl<T> HashSetExt<T> for HashSet<T> {
    fn add_all<I: IntoIterator<Item = T>>(&mut self, items: I)
    where
        T: Eq + Hash,
    {
        for item in items {
            self.insert(item);
        }
    }

    fn symmetric_difference_owned(&self, other: &HashSet<T>) -> HashSet<T>
    where
        T: Eq + Hash + Clone,
    {
        self.symmetric_difference(other).cloned().collect()
    }

    fn is_subset_of(&self, other: &HashSet<T>) -> bool
    where
        T: Eq + Hash,
    {
        self.is_subset(other)
    }

    fn is_superset_of(&self, other: &HashSet<T>) -> bool
    where
        T: Eq + Hash,
    {
        self.is_superset(other)
    }
}

/// Frequency counter.
pub struct Counter<T: Eq + Hash> {
    counts: HashMap<T, usize>,
}

impl<T: Eq + Hash> Counter<T> {
    /// Create new counter.
    pub fn new() -> Self {
        Self {
            counts: HashMap::new(),
        }
    }

    /// Count item.
    pub fn count(&mut self, item: T) {
        *self.counts.entry(item).or_insert(0) += 1;
    }

    /// Count multiple items.
    pub fn count_all<I: IntoIterator<Item = T>>(&mut self, items: I) {
        for item in items {
            self.count(item);
        }
    }

    /// Get count for item.
    pub fn get(&self, item: &T) -> usize {
        *self.counts.get(item).unwrap_or(&0)
    }

    /// Get most common items.
    pub fn most_common(&self, n: usize) -> Vec<(&T, usize)>
    where
        T: Clone,
    {
        let mut items: Vec<_> = self.counts.iter().map(|(k, &v)| (k, v)).collect();
        items.sort_by(|a, b| b.1.cmp(&a.1));
        items.truncate(n);
        items
    }

    /// Get total count.
    pub fn total(&self) -> usize {
        self.counts.values().sum()
    }

    /// Get unique count.
    pub fn unique(&self) -> usize {
        self.counts.len()
    }
}

impl<T: Eq + Hash> Default for Counter<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Eq + Hash> FromIterator<T> for Counter<T> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        let mut counter = Self::new();
        counter.count_all(iter);
        counter
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vec_first_last() {
        let v = vec![1, 2, 3];
        assert_eq!(v.first_or(0), 1);
        assert_eq!(v.last_or(0), 3);

        let empty: Vec<i32> = vec![];
        assert_eq!(empty.first_or(0), 0);
    }

    #[test]
    fn test_vec_partition() {
        let v = vec![1, 2, 3, 4, 5];
        let (evens, odds) = v.partition_by(|&x| x % 2 == 0);
        assert_eq!(evens, vec![2, 4]);
        assert_eq!(odds, vec![1, 3, 5]);
    }

    #[test]
    fn test_vec_interleave() {
        let a = vec![1, 3, 5];
        let b = vec![2, 4, 6];
        assert_eq!(a.interleave(&b), vec![1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn test_vec_rotate() {
        let v = vec![1, 2, 3, 4, 5];
        assert_eq!(v.rotate_left_vec(2), vec![3, 4, 5, 1, 2]);
        assert_eq!(v.rotate_right_vec(2), vec![4, 5, 1, 2, 3]);
    }

    #[test]
    fn test_hashmap_increment() {
        let mut map: HashMap<&str, i32> = HashMap::new();
        map.increment("a");
        map.increment("a");
        map.increment("b");
        assert_eq!(map["a"], 2);
        assert_eq!(map["b"], 1);
    }

    #[test]
    fn test_hashmap_invert() {
        let mut map = HashMap::new();
        map.insert("a", 1);
        map.insert("b", 2);

        let inverted = map.invert();
        assert_eq!(inverted[&1], "a");
        assert_eq!(inverted[&2], "b");
    }

    #[test]
    fn test_counter() {
        let items = vec!["a", "b", "a", "c", "a", "b"];
        let counter: Counter<_> = items.into_iter().collect();

        assert_eq!(counter.get(&"a"), 3);
        assert_eq!(counter.get(&"b"), 2);
        assert_eq!(counter.total(), 6);
        assert_eq!(counter.unique(), 3);

        let common = counter.most_common(2);
        assert_eq!(common[0], (&"a", 3));
    }
}
