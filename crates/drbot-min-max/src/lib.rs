//! Min/Max utilities for drbot.
//!
//! This crate provides:
//! - Min/Max operations
//! - Extrema finding
//! - Range tracking

use std::cmp::Ordering;
use thiserror::Error;

/// MinMax error types.
#[derive(Error, Debug, Clone)]
pub enum MinMaxError {
    #[error("Empty collection")]
    Empty,

    #[error("No comparable values")]
    NoComparable,
}

/// Result type for min/max operations.
pub type Result<T> = std::result::Result<T, MinMaxError>;

/// Get minimum of two values.
pub fn min<T: Ord>(a: T, b: T) -> T {
    if a <= b {
        a
    } else {
        b
    }
}

/// Get maximum of two values.
pub fn max<T: Ord>(a: T, b: T) -> T {
    if a >= b {
        a
    } else {
        b
    }
}

/// Get minimum of three values.
pub fn min3<T: Ord>(a: T, b: T, c: T) -> T {
    min(min(a, b), c)
}

/// Get maximum of three values.
pub fn max3<T: Ord>(a: T, b: T, c: T) -> T {
    max(max(a, b), c)
}

/// Get both min and max of two values.
pub fn minmax<T: Ord>(a: T, b: T) -> (T, T) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

/// Get min by key.
pub fn min_by_key<T, K: Ord, F: Fn(&T) -> K>(a: T, b: T, f: F) -> T {
    if f(&a) <= f(&b) {
        a
    } else {
        b
    }
}

/// Get max by key.
pub fn max_by_key<T, K: Ord, F: Fn(&T) -> K>(a: T, b: T, f: F) -> T {
    if f(&a) >= f(&b) {
        a
    } else {
        b
    }
}

/// Get min by custom comparator.
pub fn min_by<T, F: Fn(&T, &T) -> Ordering>(a: T, b: T, f: F) -> T {
    if f(&a, &b) != Ordering::Greater {
        a
    } else {
        b
    }
}

/// Get max by custom comparator.
pub fn max_by<T, F: Fn(&T, &T) -> Ordering>(a: T, b: T, f: F) -> T {
    if f(&a, &b) != Ordering::Less {
        a
    } else {
        b
    }
}

/// Find minimum in slice.
pub fn slice_min<T: Ord>(items: &[T]) -> Option<&T> {
    items.iter().min()
}

/// Find maximum in slice.
pub fn slice_max<T: Ord>(items: &[T]) -> Option<&T> {
    items.iter().max()
}

/// Find both min and max in slice.
pub fn slice_minmax<T: Ord>(items: &[T]) -> Option<(&T, &T)> {
    if items.is_empty() {
        return None;
    }

    let mut min_val = &items[0];
    let mut max_val = &items[0];

    for item in items.iter().skip(1) {
        if item < min_val {
            min_val = item;
        }
        if item > max_val {
            max_val = item;
        }
    }

    Some((min_val, max_val))
}

/// A running min/max tracker.
#[derive(Debug, Clone)]
pub struct MinMaxTracker<T> {
    min: Option<T>,
    max: Option<T>,
    count: usize,
}

impl<T: Ord + Clone> MinMaxTracker<T> {
    /// Create new tracker.
    pub fn new() -> Self {
        Self {
            min: None,
            max: None,
            count: 0,
        }
    }

    /// Update with new value.
    pub fn update(&mut self, value: T) {
        self.count += 1;

        match &self.min {
            Some(m) if &value < m => self.min = Some(value.clone()),
            None => self.min = Some(value.clone()),
            _ => {}
        }

        match &self.max {
            Some(m) if &value > m => self.max = Some(value),
            None => self.max = Some(value),
            _ => {}
        }
    }

    /// Get current minimum.
    pub fn min(&self) -> Option<&T> {
        self.min.as_ref()
    }

    /// Get current maximum.
    pub fn max(&self) -> Option<&T> {
        self.max.as_ref()
    }

    /// Get both min and max.
    pub fn minmax(&self) -> Option<(&T, &T)> {
        match (&self.min, &self.max) {
            (Some(min), Some(max)) => Some((min, max)),
            _ => None,
        }
    }

    /// Get count of values seen.
    pub fn count(&self) -> usize {
        self.count
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Reset tracker.
    pub fn reset(&mut self) {
        self.min = None;
        self.max = None;
        self.count = 0;
    }
}

impl<T: Ord + Clone> Default for MinMaxTracker<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Find index of minimum in slice.
pub fn argmin<T: Ord>(items: &[T]) -> Option<usize> {
    items
        .iter()
        .enumerate()
        .min_by_key(|(_, v)| *v)
        .map(|(i, _)| i)
}

/// Find index of maximum in slice.
pub fn argmax<T: Ord>(items: &[T]) -> Option<usize> {
    items
        .iter()
        .enumerate()
        .max_by_key(|(_, v)| *v)
        .map(|(i, _)| i)
}

/// Find indices of both min and max.
pub fn argminmax<T: Ord>(items: &[T]) -> Option<(usize, usize)> {
    if items.is_empty() {
        return None;
    }

    let mut min_idx = 0;
    let mut max_idx = 0;

    for (i, item) in items.iter().enumerate().skip(1) {
        if item < &items[min_idx] {
            min_idx = i;
        }
        if item > &items[max_idx] {
            max_idx = i;
        }
    }

    Some((min_idx, max_idx))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_min_max() {
        assert_eq!(min(3, 5), 3);
        assert_eq!(max(3, 5), 5);
        assert_eq!(min3(3, 1, 5), 1);
        assert_eq!(max3(3, 1, 5), 5);
    }

    #[test]
    fn test_minmax() {
        assert_eq!(minmax(5, 3), (3, 5));
        assert_eq!(minmax(3, 5), (3, 5));
    }

    #[test]
    fn test_slice_minmax() {
        let items = [3, 1, 4, 1, 5, 9, 2, 6];
        assert_eq!(slice_min(&items), Some(&1));
        assert_eq!(slice_max(&items), Some(&9));
        assert_eq!(slice_minmax(&items), Some((&1, &9)));
    }

    #[test]
    fn test_tracker() {
        let mut tracker = MinMaxTracker::new();
        tracker.update(5);
        tracker.update(2);
        tracker.update(8);

        assert_eq!(tracker.min(), Some(&2));
        assert_eq!(tracker.max(), Some(&8));
        assert_eq!(tracker.count(), 3);
    }

    #[test]
    fn test_argmin_argmax() {
        let items = [3, 1, 4, 1, 5];
        assert_eq!(argmin(&items), Some(1));
        assert_eq!(argmax(&items), Some(4));
        assert_eq!(argminmax(&items), Some((1, 4)));
    }
}
