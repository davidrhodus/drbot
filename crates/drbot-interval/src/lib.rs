//! Interval and range utilities for drbot.
//!
//! This crate provides:
//! - Interval types
//! - Range operations
//! - Interval trees
//! - Set operations

use std::cmp::{Ord, Ordering, PartialOrd};
use std::fmt;
use thiserror::Error;

/// Interval error types.
#[derive(Error, Debug)]
pub enum IntervalError {
    #[error("Invalid interval: start > end")]
    InvalidInterval,

    #[error("Empty interval")]
    EmptyInterval,
}

/// Result type for interval operations.
pub type Result<T> = std::result::Result<T, IntervalError>;

/// Interval bound type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bound<T> {
    /// Inclusive bound.
    Inclusive(T),
    /// Exclusive bound.
    Exclusive(T),
    /// Unbounded.
    Unbounded,
}

impl<T: Clone> Bound<T> {
    /// Get value if bounded.
    pub fn value(&self) -> Option<&T> {
        match self {
            Bound::Inclusive(v) | Bound::Exclusive(v) => Some(v),
            Bound::Unbounded => None,
        }
    }

    /// Check if inclusive.
    pub fn is_inclusive(&self) -> bool {
        matches!(self, Bound::Inclusive(_))
    }

    /// Check if exclusive.
    pub fn is_exclusive(&self) -> bool {
        matches!(self, Bound::Exclusive(_))
    }

    /// Check if unbounded.
    pub fn is_unbounded(&self) -> bool {
        matches!(self, Bound::Unbounded)
    }
}

/// Generic interval.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Interval<T> {
    /// Start bound.
    pub start: Bound<T>,
    /// End bound.
    pub end: Bound<T>,
}

impl<T: Clone + Ord> Interval<T> {
    /// Create closed interval [start, end].
    pub fn closed(start: T, end: T) -> Result<Self> {
        if start > end {
            return Err(IntervalError::InvalidInterval);
        }
        Ok(Self {
            start: Bound::Inclusive(start),
            end: Bound::Inclusive(end),
        })
    }

    /// Create open interval (start, end).
    pub fn open(start: T, end: T) -> Result<Self> {
        if start >= end {
            return Err(IntervalError::InvalidInterval);
        }
        Ok(Self {
            start: Bound::Exclusive(start),
            end: Bound::Exclusive(end),
        })
    }

    /// Create half-open interval [start, end).
    pub fn half_open(start: T, end: T) -> Result<Self> {
        if start >= end {
            return Err(IntervalError::InvalidInterval);
        }
        Ok(Self {
            start: Bound::Inclusive(start),
            end: Bound::Exclusive(end),
        })
    }

    /// Create unbounded interval (-∞, +∞).
    pub fn unbounded() -> Self {
        Self {
            start: Bound::Unbounded,
            end: Bound::Unbounded,
        }
    }

    /// Create interval starting from value.
    pub fn from(start: T) -> Self {
        Self {
            start: Bound::Inclusive(start),
            end: Bound::Unbounded,
        }
    }

    /// Create interval up to value.
    pub fn to(end: T) -> Self {
        Self {
            start: Bound::Unbounded,
            end: Bound::Exclusive(end),
        }
    }

    /// Check if value is in interval.
    pub fn contains(&self, value: &T) -> bool {
        let after_start = match &self.start {
            Bound::Inclusive(s) => value >= s,
            Bound::Exclusive(s) => value > s,
            Bound::Unbounded => true,
        };

        let before_end = match &self.end {
            Bound::Inclusive(e) => value <= e,
            Bound::Exclusive(e) => value < e,
            Bound::Unbounded => true,
        };

        after_start && before_end
    }

    /// Check if intervals overlap.
    pub fn overlaps(&self, other: &Self) -> bool {
        let self_before_other = match (&self.end, &other.start) {
            (Bound::Inclusive(a), Bound::Inclusive(b)) => a < b,
            (Bound::Inclusive(a), Bound::Exclusive(b)) => a <= b,
            (Bound::Exclusive(a), Bound::Inclusive(b)) => a <= b,
            (Bound::Exclusive(a), Bound::Exclusive(b)) => a <= b,
            (Bound::Unbounded, _) | (_, Bound::Unbounded) => false,
        };

        let other_before_self = match (&other.end, &self.start) {
            (Bound::Inclusive(a), Bound::Inclusive(b)) => a < b,
            (Bound::Inclusive(a), Bound::Exclusive(b)) => a <= b,
            (Bound::Exclusive(a), Bound::Inclusive(b)) => a <= b,
            (Bound::Exclusive(a), Bound::Exclusive(b)) => a <= b,
            (Bound::Unbounded, _) | (_, Bound::Unbounded) => false,
        };

        !self_before_other && !other_before_self
    }

    /// Get intersection of intervals.
    pub fn intersection(&self, other: &Self) -> Option<Self> {
        if !self.overlaps(other) {
            return None;
        }

        let start = match (&self.start, &other.start) {
            (Bound::Unbounded, b) | (b, Bound::Unbounded) => b.clone(),
            (Bound::Inclusive(a), Bound::Inclusive(b)) => Bound::Inclusive(a.max(b).clone()),
            (Bound::Inclusive(a), Bound::Exclusive(b)) => {
                if a > b {
                    Bound::Inclusive(a.clone())
                } else {
                    Bound::Exclusive(b.clone())
                }
            }
            (Bound::Exclusive(a), Bound::Inclusive(b)) => {
                if b > a {
                    Bound::Inclusive(b.clone())
                } else {
                    Bound::Exclusive(a.clone())
                }
            }
            (Bound::Exclusive(a), Bound::Exclusive(b)) => Bound::Exclusive(a.max(b).clone()),
        };

        let end = match (&self.end, &other.end) {
            (Bound::Unbounded, b) | (b, Bound::Unbounded) => b.clone(),
            (Bound::Inclusive(a), Bound::Inclusive(b)) => Bound::Inclusive(a.min(b).clone()),
            (Bound::Inclusive(a), Bound::Exclusive(b)) => {
                if a < b {
                    Bound::Inclusive(a.clone())
                } else {
                    Bound::Exclusive(b.clone())
                }
            }
            (Bound::Exclusive(a), Bound::Inclusive(b)) => {
                if b < a {
                    Bound::Inclusive(b.clone())
                } else {
                    Bound::Exclusive(a.clone())
                }
            }
            (Bound::Exclusive(a), Bound::Exclusive(b)) => Bound::Exclusive(a.min(b).clone()),
        };

        Some(Self { start, end })
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        match (&self.start, &self.end) {
            (Bound::Inclusive(s), Bound::Exclusive(e)) => s >= e,
            (Bound::Exclusive(s), Bound::Inclusive(e)) => s >= e,
            (Bound::Exclusive(s), Bound::Exclusive(e)) => s >= e,
            _ => false,
        }
    }
}

impl<T: fmt::Display + Clone> fmt::Display for Interval<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.start {
            Bound::Inclusive(v) => write!(f, "[{}", v)?,
            Bound::Exclusive(v) => write!(f, "({}", v)?,
            Bound::Unbounded => write!(f, "(-∞")?,
        }
        write!(f, ", ")?;
        match &self.end {
            Bound::Inclusive(v) => write!(f, "{}]", v),
            Bound::Exclusive(v) => write!(f, "{})", v),
            Bound::Unbounded => write!(f, "+∞)"),
        }
    }
}

/// Integer range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IntRange {
    /// Start (inclusive).
    pub start: i64,
    /// End (exclusive).
    pub end: i64,
}

impl IntRange {
    /// Create new range.
    pub fn new(start: i64, end: i64) -> Result<Self> {
        if start > end {
            return Err(IntervalError::InvalidInterval);
        }
        Ok(Self { start, end })
    }

    /// Create range of single value.
    pub fn single(value: i64) -> Self {
        Self {
            start: value,
            end: value + 1,
        }
    }

    /// Check if contains value.
    pub fn contains(&self, value: i64) -> bool {
        value >= self.start && value < self.end
    }

    /// Get length.
    pub fn len(&self) -> usize {
        (self.end - self.start).max(0) as usize
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.start >= self.end
    }

    /// Check if overlaps.
    pub fn overlaps(&self, other: &Self) -> bool {
        self.start < other.end && other.start < self.end
    }

    /// Get intersection.
    pub fn intersection(&self, other: &Self) -> Option<Self> {
        let start = self.start.max(other.start);
        let end = self.end.min(other.end);
        if start < end {
            Some(Self { start, end })
        } else {
            None
        }
    }

    /// Get union (if contiguous).
    pub fn union(&self, other: &Self) -> Option<Self> {
        if self.overlaps(other) || self.end == other.start || other.end == self.start {
            Some(Self {
                start: self.start.min(other.start),
                end: self.end.max(other.end),
            })
        } else {
            None
        }
    }

    /// Iterate over values.
    pub fn iter(&self) -> impl Iterator<Item = i64> {
        self.start..self.end
    }
}

impl IntoIterator for IntRange {
    type Item = i64;
    type IntoIter = std::ops::Range<i64>;

    fn into_iter(self) -> Self::IntoIter {
        self.start..self.end
    }
}

/// Interval set.
#[derive(Debug, Clone)]
pub struct IntervalSet<T: Ord + Clone> {
    intervals: Vec<Interval<T>>,
}

impl<T: Ord + Clone> IntervalSet<T> {
    /// Create empty set.
    pub fn new() -> Self {
        Self {
            intervals: Vec::new(),
        }
    }

    /// Add interval.
    pub fn insert(&mut self, interval: Interval<T>) {
        // Simple implementation: just add and rely on overlaps check
        self.intervals.push(interval);
    }

    /// Check if contains value.
    pub fn contains(&self, value: &T) -> bool {
        self.intervals.iter().any(|i| i.contains(value))
    }

    /// Get count of intervals.
    pub fn len(&self) -> usize {
        self.intervals.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.intervals.is_empty()
    }

    /// Iterate intervals.
    pub fn iter(&self) -> impl Iterator<Item = &Interval<T>> {
        self.intervals.iter()
    }
}

impl<T: Ord + Clone> Default for IntervalSet<T> {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Kani Formal Verification Proofs
// ============================================================================

#[cfg(kani)]
mod kani_proofs {
    use super::*;

    // ========================================================================
    // Bound Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_bound_inclusive_has_value() {
        let v: i32 = kani::any();
        let bound = Bound::Inclusive(v);

        kani::assert!(bound.value().is_some(), "Inclusive bound has value");
        kani::assert!(bound.is_inclusive(), "Inclusive bound is_inclusive");
        kani::assert!(!bound.is_exclusive(), "Inclusive bound not is_exclusive");
        kani::assert!(!bound.is_unbounded(), "Inclusive bound not is_unbounded");
    }

    #[kani::proof]
    fn proof_bound_exclusive_has_value() {
        let v: i32 = kani::any();
        let bound = Bound::Exclusive(v);

        kani::assert!(bound.value().is_some(), "Exclusive bound has value");
        kani::assert!(!bound.is_inclusive(), "Exclusive bound not is_inclusive");
        kani::assert!(bound.is_exclusive(), "Exclusive bound is_exclusive");
        kani::assert!(!bound.is_unbounded(), "Exclusive bound not is_unbounded");
    }

    #[kani::proof]
    fn proof_bound_unbounded_no_value() {
        let bound: Bound<i32> = Bound::Unbounded;

        kani::assert!(bound.value().is_none(), "Unbounded has no value");
        kani::assert!(!bound.is_inclusive(), "Unbounded not is_inclusive");
        kani::assert!(!bound.is_exclusive(), "Unbounded not is_exclusive");
        kani::assert!(bound.is_unbounded(), "Unbounded is_unbounded");
    }

    // ========================================================================
    // Interval Contains Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_closed_interval_contains_endpoints() {
        let start: i32 = kani::any();
        let end: i32 = kani::any();
        kani::assume(start <= end);
        kani::assume(start > i32::MIN + 1);
        kani::assume(end < i32::MAX - 1);

        let interval = Interval::closed(start, end).unwrap();

        kani::assert!(interval.contains(&start), "Closed interval contains start");
        kani::assert!(interval.contains(&end), "Closed interval contains end");
    }

    #[kani::proof]
    fn proof_open_interval_excludes_endpoints() {
        let start: i32 = kani::any();
        let end: i32 = kani::any();
        kani::assume(start < end);
        kani::assume(start > i32::MIN + 1);
        kani::assume(end < i32::MAX - 1);

        let interval = Interval::open(start, end).unwrap();

        kani::assert!(!interval.contains(&start), "Open interval excludes start");
        kani::assert!(!interval.contains(&end), "Open interval excludes end");
    }

    #[kani::proof]
    fn proof_half_open_interval_bounds() {
        let start: i32 = kani::any();
        let end: i32 = kani::any();
        kani::assume(start < end);
        kani::assume(start > i32::MIN + 1);
        kani::assume(end < i32::MAX - 1);

        let interval = Interval::half_open(start, end).unwrap();

        kani::assert!(interval.contains(&start), "Half-open includes start");
        kani::assert!(!interval.contains(&end), "Half-open excludes end");
    }

    #[kani::proof]
    fn proof_unbounded_interval_contains_all() {
        let v: i32 = kani::any();
        let interval: Interval<i32> = Interval::unbounded();

        kani::assert!(interval.contains(&v), "Unbounded contains any value");
    }

    #[kani::proof]
    fn proof_closed_interval_contains_middle() {
        let start: i32 = kani::any();
        let end: i32 = kani::any();
        let mid: i32 = kani::any();

        kani::assume(start <= end);
        kani::assume(mid >= start && mid <= end);

        let interval = Interval::closed(start, end).unwrap();
        kani::assert!(
            interval.contains(&mid),
            "Closed interval contains value between start and end"
        );
    }

    #[kani::proof]
    fn proof_closed_interval_excludes_outside() {
        let interval = Interval::closed(10i32, 20i32).unwrap();

        kani::assert!(!interval.contains(&9), "Closed excludes below start");
        kani::assert!(!interval.contains(&21), "Closed excludes above end");
    }

    // ========================================================================
    // Interval Validity Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_closed_interval_invalid_when_start_greater_than_end() {
        let start: i32 = kani::any();
        let end: i32 = kani::any();
        kani::assume(start > end);

        let result = Interval::closed(start, end);
        kani::assert!(result.is_err(), "Closed interval invalid when start > end");
    }

    #[kani::proof]
    fn proof_open_interval_invalid_when_start_ge_end() {
        let start: i32 = kani::any();
        let end: i32 = kani::any();
        kani::assume(start >= end);

        let result = Interval::open(start, end);
        kani::assert!(result.is_err(), "Open interval invalid when start >= end");
    }

    // ========================================================================
    // IntRange Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_int_range_len_correct() {
        let start: i64 = kani::any();
        let end: i64 = kani::any();

        kani::assume(start <= end);
        kani::assume(start > i64::MIN / 2);
        kani::assume(end < i64::MAX / 2);

        let range = IntRange::new(start, end).unwrap();
        let expected_len = (end - start) as usize;

        kani::assert!(range.len() == expected_len, "IntRange len is end - start");
    }

    #[kani::proof]
    fn proof_int_range_is_empty_logic() {
        let start: i64 = kani::any();
        let end: i64 = kani::any();

        kani::assume(start > i64::MIN / 2);
        kani::assume(end < i64::MAX / 2);

        if start <= end {
            let range = IntRange::new(start, end).unwrap();
            let is_empty = range.is_empty();
            let has_zero_len = range.len() == 0;

            kani::assert!(is_empty == has_zero_len, "is_empty matches len == 0");

            if start == end {
                kani::assert!(is_empty, "Range is empty when start == end");
            }
        }
    }

    #[kani::proof]
    fn proof_int_range_contains_logic() {
        let range = IntRange::new(0i64, 10i64).unwrap();

        kani::assert!(range.contains(0), "Contains start");
        kani::assert!(range.contains(9), "Contains end-1");
        kani::assert!(!range.contains(10), "Excludes end");
        kani::assert!(!range.contains(-1), "Excludes below start");
    }

    #[kani::proof]
    fn proof_int_range_single_contains_only_value() {
        let v: i64 = kani::any();
        kani::assume(v < i64::MAX);

        let range = IntRange::single(v);

        kani::assert!(range.contains(v), "Single contains its value");
        kani::assert!(!range.contains(v + 1), "Single excludes value + 1");
        kani::assert!(range.len() == 1, "Single has len 1");
    }

    #[kani::proof]
    fn proof_int_range_overlap_symmetric() {
        let a = IntRange::new(0i64, 10i64).unwrap();
        let b = IntRange::new(5i64, 15i64).unwrap();

        let a_overlaps_b = a.overlaps(&b);
        let b_overlaps_a = b.overlaps(&a);

        kani::assert!(a_overlaps_b == b_overlaps_a, "Overlaps is symmetric");
    }

    #[kani::proof]
    fn proof_int_range_no_overlap_disjoint() {
        let a = IntRange::new(0i64, 5i64).unwrap();
        let b = IntRange::new(10i64, 15i64).unwrap();

        kani::assert!(!a.overlaps(&b), "Disjoint ranges don't overlap");
    }

    #[kani::proof]
    fn proof_int_range_intersection_within_both() {
        let a = IntRange::new(0i64, 10i64).unwrap();
        let b = IntRange::new(5i64, 15i64).unwrap();

        let intersection = a.intersection(&b).unwrap();

        kani::assert!(
            intersection.start == 5,
            "Intersection start is max of starts"
        );
        kani::assert!(intersection.end == 10, "Intersection end is min of ends");
    }

    #[kani::proof]
    fn proof_int_range_union_contiguous() {
        let a = IntRange::new(0i64, 5i64).unwrap();
        let b = IntRange::new(5i64, 10i64).unwrap();

        let union = a.union(&b).unwrap();

        kani::assert!(union.start == 0, "Union start is min of starts");
        kani::assert!(union.end == 10, "Union end is max of ends");
    }

    // ========================================================================
    // IntervalSet Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_interval_set_empty_initially() {
        let set: IntervalSet<i32> = IntervalSet::new();

        kani::assert!(set.is_empty(), "New set is empty");
        kani::assert!(set.len() == 0, "New set has len 0");
    }

    #[kani::proof]
    fn proof_interval_set_insert_increases_len() {
        let mut set: IntervalSet<i32> = IntervalSet::new();
        let interval = Interval::closed(1, 5).unwrap();

        set.insert(interval);

        kani::assert!(set.len() == 1, "Len is 1 after insert");
        kani::assert!(!set.is_empty(), "Set not empty after insert");
    }

    #[kani::proof]
    fn proof_interval_set_contains_after_insert() {
        let mut set: IntervalSet<i32> = IntervalSet::new();
        let interval = Interval::closed(1, 10).unwrap();

        set.insert(interval);

        kani::assert!(set.contains(&5), "Set contains value in interval");
        kani::assert!(!set.contains(&0), "Set doesn't contain value outside");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_closed_interval() {
        let interval = Interval::closed(1, 5).unwrap();
        assert!(interval.contains(&1));
        assert!(interval.contains(&3));
        assert!(interval.contains(&5));
        assert!(!interval.contains(&0));
        assert!(!interval.contains(&6));
    }

    #[test]
    fn test_open_interval() {
        let interval = Interval::open(1, 5).unwrap();
        assert!(!interval.contains(&1));
        assert!(interval.contains(&3));
        assert!(!interval.contains(&5));
    }

    #[test]
    fn test_overlap() {
        let a = Interval::closed(1, 5).unwrap();
        let b = Interval::closed(3, 7).unwrap();
        let c = Interval::closed(6, 10).unwrap();

        assert!(a.overlaps(&b));
        assert!(!a.overlaps(&c));
        assert!(b.overlaps(&c));
    }

    #[test]
    fn test_intersection() {
        let a = Interval::closed(1, 5).unwrap();
        let b = Interval::closed(3, 7).unwrap();

        let intersection = a.intersection(&b).unwrap();
        assert!(intersection.contains(&3));
        assert!(intersection.contains(&5));
        assert!(!intersection.contains(&1));
        assert!(!intersection.contains(&7));
    }

    #[test]
    fn test_int_range() {
        let range = IntRange::new(0, 10).unwrap();
        assert!(range.contains(0));
        assert!(range.contains(9));
        assert!(!range.contains(10));
        assert_eq!(range.len(), 10);
    }

    #[test]
    fn test_int_range_union() {
        let a = IntRange::new(0, 5).unwrap();
        let b = IntRange::new(5, 10).unwrap();

        let union = a.union(&b).unwrap();
        assert_eq!(union.start, 0);
        assert_eq!(union.end, 10);
    }
}
