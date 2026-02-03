//! Duplication utilities for drbot.
//!
//! This crate provides:
//! - Value duplication
//! - Collection duplication
//! - Duplication strategies

use thiserror::Error;

/// Duplicate error types.
#[derive(Error, Debug, Clone)]
pub enum DuplicateError {
    #[error("Duplication failed: {0}")]
    Failed(String),
}

/// Result type for duplicate operations.
pub type Result<T> = std::result::Result<T, DuplicateError>;

/// Duplicate a value n times.
pub fn duplicate<T: Clone>(value: &T, n: usize) -> Vec<T> {
    (0..n).map(|_| value.clone()).collect()
}

/// Duplicate with factory.
pub fn duplicate_with<T, F: Fn(usize) -> T>(n: usize, f: F) -> Vec<T> {
    (0..n).map(f).collect()
}

/// Duplicate into array.
pub fn duplicate_array<T: Clone + Default, const N: usize>(value: &T) -> [T; N] {
    let mut arr: [T; N] = std::array::from_fn(|_| T::default());
    for item in arr.iter_mut() {
        *item = value.clone();
    }
    arr
}

/// Duplicatable trait.
pub trait Duplicatable: Clone {
    /// Duplicate n times.
    fn duplicate(&self, n: usize) -> Vec<Self> {
        duplicate(self, n)
    }

    /// Duplicate with transformation.
    fn duplicate_map<U, F: Fn(&Self, usize) -> U>(&self, n: usize, f: F) -> Vec<U> {
        (0..n).map(|i| f(self, i)).collect()
    }
}

impl<T: Clone> Duplicatable for T {}

/// Duplicate detector.
#[derive(Debug, Clone)]
pub struct DuplicateDetector<T: Eq + std::hash::Hash> {
    seen: std::collections::HashSet<T>,
}

impl<T: Eq + std::hash::Hash + Clone> DuplicateDetector<T> {
    /// Create new detector.
    pub fn new() -> Self {
        Self {
            seen: std::collections::HashSet::new(),
        }
    }

    /// Check if duplicate.
    pub fn is_duplicate(&mut self, value: &T) -> bool {
        !self.seen.insert(value.clone())
    }

    /// Check without marking.
    pub fn has_seen(&self, value: &T) -> bool {
        self.seen.contains(value)
    }

    /// Get count of unique items.
    pub fn unique_count(&self) -> usize {
        self.seen.len()
    }

    /// Clear.
    pub fn clear(&mut self) {
        self.seen.clear();
    }
}

impl<T: Eq + std::hash::Hash + Clone> Default for DuplicateDetector<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Find duplicates in slice.
pub fn find_duplicates<T: Eq + std::hash::Hash + Clone>(items: &[T]) -> Vec<T> {
    let mut seen = std::collections::HashSet::new();
    let mut duplicates = std::collections::HashSet::new();

    for item in items {
        if !seen.insert(item.clone()) {
            duplicates.insert(item.clone());
        }
    }

    duplicates.into_iter().collect()
}

/// Count duplicates.
pub fn count_duplicates<T: Eq + std::hash::Hash>(items: &[T]) -> usize {
    let unique: std::collections::HashSet<_> = items.iter().collect();
    items.len() - unique.len()
}

/// Remove duplicates.
pub fn deduplicate<T: Eq + std::hash::Hash + Clone>(items: &[T]) -> Vec<T> {
    let mut seen = std::collections::HashSet::new();
    items
        .iter()
        .filter(|item| seen.insert((*item).clone()))
        .cloned()
        .collect()
}

/// Remove consecutive duplicates.
pub fn deduplicate_consecutive<T: Eq + Clone>(items: &[T]) -> Vec<T> {
    if items.is_empty() {
        return Vec::new();
    }

    let mut result = vec![items[0].clone()];
    for item in items.iter().skip(1) {
        if result.last() != Some(item) {
            result.push(item.clone());
        }
    }
    result
}

/// Duplicate counter.
pub fn count_occurrences<T: Eq + std::hash::Hash>(
    items: &[T],
) -> std::collections::HashMap<&T, usize> {
    let mut counts = std::collections::HashMap::new();
    for item in items {
        *counts.entry(item).or_insert(0) += 1;
    }
    counts
}

/// Has duplicates check.
pub fn has_duplicates<T: Eq + std::hash::Hash>(items: &[T]) -> bool {
    let unique: std::collections::HashSet<_> = items.iter().collect();
    unique.len() != items.len()
}

/// All unique check.
pub fn all_unique<T: Eq + std::hash::Hash>(items: &[T]) -> bool {
    !has_duplicates(items)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_duplicate() {
        assert_eq!(duplicate(&42, 3), vec![42, 42, 42]);
    }

    #[test]
    fn test_duplicate_with() {
        let result = duplicate_with(3, |i| i * 2);
        assert_eq!(result, vec![0, 2, 4]);
    }

    #[test]
    fn test_find_duplicates() {
        let items = vec![1, 2, 2, 3, 3, 3, 4];
        let dups = find_duplicates(&items);
        assert!(dups.contains(&2));
        assert!(dups.contains(&3));
        assert!(!dups.contains(&1));
    }

    #[test]
    fn test_deduplicate() {
        let items = vec![1, 2, 2, 3, 1, 4];
        let unique = deduplicate(&items);
        assert_eq!(unique.len(), 4);
    }

    #[test]
    fn test_deduplicate_consecutive() {
        let items = vec![1, 1, 2, 2, 2, 3, 1, 1];
        let result = deduplicate_consecutive(&items);
        assert_eq!(result, vec![1, 2, 3, 1]);
    }

    #[test]
    fn test_detector() {
        let mut detector = DuplicateDetector::new();
        assert!(!detector.is_duplicate(&1));
        assert!(!detector.is_duplicate(&2));
        assert!(detector.is_duplicate(&1));
    }
}
