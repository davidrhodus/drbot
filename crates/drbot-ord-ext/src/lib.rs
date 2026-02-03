//! Ordering extension utilities for drbot.
//!
//! This crate provides:
//! - Ordering extensions
//! - Partial ordering utilities
//! - Ordering chains

use std::cmp::Ordering;
use thiserror::Error;

/// Ordering error types.
#[derive(Error, Debug, Clone)]
pub enum OrdError {
    #[error("Values are not comparable")]
    NotComparable,
}

/// Result type for ordering operations.
pub type Result<T> = std::result::Result<T, OrdError>;

/// Extension trait for Ordering.
pub trait OrderingExt {
    /// Check if less.
    fn is_less(&self) -> bool;

    /// Check if equal.
    fn is_equal(&self) -> bool;

    /// Check if greater.
    fn is_greater(&self) -> bool;

    /// Check if less or equal.
    fn is_le(&self) -> bool;

    /// Check if greater or equal.
    fn is_ge(&self) -> bool;

    /// Convert to i8 (-1, 0, 1).
    fn to_i8(&self) -> i8;

    /// Create from i8.
    fn from_i8(value: i8) -> Ordering;
}

impl OrderingExt for Ordering {
    fn is_less(&self) -> bool {
        matches!(self, Ordering::Less)
    }

    fn is_equal(&self) -> bool {
        matches!(self, Ordering::Equal)
    }

    fn is_greater(&self) -> bool {
        matches!(self, Ordering::Greater)
    }

    fn is_le(&self) -> bool {
        !matches!(self, Ordering::Greater)
    }

    fn is_ge(&self) -> bool {
        !matches!(self, Ordering::Less)
    }

    fn to_i8(&self) -> i8 {
        match self {
            Ordering::Less => -1,
            Ordering::Equal => 0,
            Ordering::Greater => 1,
        }
    }

    fn from_i8(value: i8) -> Ordering {
        match value.signum() {
            -1 => Ordering::Less,
            0 => Ordering::Equal,
            1 => Ordering::Greater,
            _ => unreachable!(),
        }
    }
}

/// Extension trait for Ord types.
pub trait OrdExt: Ord + Sized {
    /// Get the maximum of self and other.
    fn max_of(self, other: Self) -> Self {
        if self >= other {
            self
        } else {
            other
        }
    }

    /// Get the minimum of self and other.
    fn min_of(self, other: Self) -> Self {
        if self <= other {
            self
        } else {
            other
        }
    }

    /// Clamp value to range.
    fn clamp_to(self, min: Self, max: Self) -> Self {
        if self < min {
            min
        } else if self > max {
            max
        } else {
            self
        }
    }

    /// Check if in range (inclusive).
    fn in_range(&self, min: &Self, max: &Self) -> bool {
        self >= min && self <= max
    }

    /// Compare and return ordering.
    fn compare_to(&self, other: &Self) -> Ordering {
        self.cmp(other)
    }
}

impl<T: Ord> OrdExt for T {}

/// Extension trait for PartialOrd types.
pub trait PartialOrdExt: PartialOrd + Sized {
    /// Try to get maximum.
    fn try_max(self, other: Self) -> Option<Self> {
        self.partial_cmp(&other)
            .map(|ord| if ord != Ordering::Less { self } else { other })
    }

    /// Try to get minimum.
    fn try_min(self, other: Self) -> Option<Self> {
        self.partial_cmp(&other).map(|ord| {
            if ord != Ordering::Greater {
                self
            } else {
                other
            }
        })
    }

    /// Check if comparable.
    fn is_comparable(&self, other: &Self) -> bool {
        self.partial_cmp(other).is_some()
    }

    /// Check if strictly less.
    fn is_lt(&self, other: &Self) -> bool {
        self.partial_cmp(other) == Some(Ordering::Less)
    }

    /// Check if strictly greater.
    fn is_gt(&self, other: &Self) -> bool {
        self.partial_cmp(other) == Some(Ordering::Greater)
    }
}

impl<T: PartialOrd> PartialOrdExt for T {}

/// Build an ordering from multiple keys.
pub struct OrderingBuilder {
    result: Ordering,
}

impl OrderingBuilder {
    /// Start with Equal.
    pub fn new() -> Self {
        Self {
            result: Ordering::Equal,
        }
    }

    /// Compare values and chain.
    pub fn compare<T: Ord>(mut self, a: &T, b: &T) -> Self {
        if self.result == Ordering::Equal {
            self.result = a.cmp(b);
        }
        self
    }

    /// Compare by key and chain.
    pub fn compare_by_key<T, K: Ord, F: Fn(&T) -> K>(mut self, a: &T, b: &T, f: F) -> Self {
        if self.result == Ordering::Equal {
            self.result = f(a).cmp(&f(b));
        }
        self
    }

    /// Chain with existing ordering.
    pub fn then(mut self, ord: Ordering) -> Self {
        if self.result == Ordering::Equal {
            self.result = ord;
        }
        self
    }

    /// Finish and get result.
    pub fn finish(self) -> Ordering {
        self.result
    }
}

impl Default for OrderingBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Reverse an ordering.
pub fn reverse(ord: Ordering) -> Ordering {
    ord.reverse()
}

/// Create ordering from comparison result.
pub fn from_cmp<T: Ord>(a: &T, b: &T) -> Ordering {
    a.cmp(b)
}

/// Create ordering from bool (true = Greater, false = Less).
pub fn from_bool(value: bool) -> Ordering {
    if value {
        Ordering::Greater
    } else {
        Ordering::Less
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ordering_ext() {
        assert!(Ordering::Less.is_less());
        assert!(Ordering::Equal.is_equal());
        assert!(Ordering::Greater.is_greater());

        assert_eq!(Ordering::Less.to_i8(), -1);
        assert_eq!(Ordering::Equal.to_i8(), 0);
        assert_eq!(Ordering::Greater.to_i8(), 1);
    }

    #[test]
    fn test_ord_ext() {
        assert_eq!(5.max_of(3), 5);
        assert_eq!(5.min_of(3), 3);
        assert_eq!(5.clamp_to(0, 3), 3);
        assert!(5.in_range(&0, &10));
    }

    #[test]
    fn test_partial_ord_ext() {
        assert_eq!(5.0f64.try_max(3.0), Some(5.0));
        assert!(f64::NAN.try_max(3.0).is_none());
        assert!(5.0f64.is_comparable(&3.0));
        assert!(!f64::NAN.is_comparable(&3.0));
    }

    #[test]
    fn test_ordering_builder() {
        let ord = OrderingBuilder::new()
            .compare(&1, &1)
            .compare(&"a", &"b")
            .finish();
        assert_eq!(ord, Ordering::Less);
    }
}

// ============================================================================
// Kani Formal Verification Proofs
// ============================================================================

#[cfg(kani)]
mod kani_proofs {
    use super::*;

    // ========================================================================
    // OrderingExt Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_ordering_ext_is_less() {
        kani::assert(Ordering::Less.is_less(), "Less.is_less()");
        kani::assert(!Ordering::Equal.is_less(), "Equal.is_less() false");
        kani::assert(!Ordering::Greater.is_less(), "Greater.is_less() false");
    }

    #[kani::proof]
    fn proof_ordering_ext_is_equal() {
        kani::assert(!Ordering::Less.is_equal(), "Less.is_equal() false");
        kani::assert(Ordering::Equal.is_equal(), "Equal.is_equal()");
        kani::assert(!Ordering::Greater.is_equal(), "Greater.is_equal() false");
    }

    #[kani::proof]
    fn proof_ordering_ext_is_greater() {
        kani::assert(!Ordering::Less.is_greater(), "Less.is_greater() false");
        kani::assert(!Ordering::Equal.is_greater(), "Equal.is_greater() false");
        kani::assert(Ordering::Greater.is_greater(), "Greater.is_greater()");
    }

    #[kani::proof]
    fn proof_ordering_ext_is_le() {
        kani::assert(Ordering::Less.is_le(), "Less.is_le()");
        kani::assert(Ordering::Equal.is_le(), "Equal.is_le()");
        kani::assert(!Ordering::Greater.is_le(), "Greater.is_le() false");
    }

    #[kani::proof]
    fn proof_ordering_ext_is_ge() {
        kani::assert(!Ordering::Less.is_ge(), "Less.is_ge() false");
        kani::assert(Ordering::Equal.is_ge(), "Equal.is_ge()");
        kani::assert(Ordering::Greater.is_ge(), "Greater.is_ge()");
    }

    #[kani::proof]
    fn proof_ordering_ext_to_i8() {
        kani::assert(Ordering::Less.to_i8() == -1, "Less.to_i8() == -1");
        kani::assert(Ordering::Equal.to_i8() == 0, "Equal.to_i8() == 0");
        kani::assert(Ordering::Greater.to_i8() == 1, "Greater.to_i8() == 1");
    }

    #[kani::proof]
    fn proof_ordering_ext_from_i8_negative() {
        let i: i8 = kani::any();
        kani::assume(i < 0);

        kani::assert(Ordering::from_i8(i) == Ordering::Less, "negative -> Less");
    }

    #[kani::proof]
    fn proof_ordering_ext_from_i8_zero() {
        kani::assert(Ordering::from_i8(0) == Ordering::Equal, "0 -> Equal");
    }

    #[kani::proof]
    fn proof_ordering_ext_from_i8_positive() {
        let i: i8 = kani::any();
        kani::assume(i > 0);

        kani::assert(
            Ordering::from_i8(i) == Ordering::Greater,
            "positive -> Greater",
        );
    }

    #[kani::proof]
    fn proof_ordering_ext_roundtrip() {
        let less = Ordering::Less;
        let equal = Ordering::Equal;
        let greater = Ordering::Greater;

        kani::assert(Ordering::from_i8(less.to_i8()) == less, "Less roundtrip");
        kani::assert(Ordering::from_i8(equal.to_i8()) == equal, "Equal roundtrip");
        kani::assert(
            Ordering::from_i8(greater.to_i8()) == greater,
            "Greater roundtrip",
        );
    }

    // ========================================================================
    // OrdExt Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_ord_ext_max_of() {
        let a: i8 = kani::any();
        let b: i8 = kani::any();

        let result = a.max_of(b);
        kani::assert(result >= a && result >= b, "max_of >= both");
        kani::assert(result == a || result == b, "max_of is one of them");
    }

    #[kani::proof]
    fn proof_ord_ext_min_of() {
        let a: i8 = kani::any();
        let b: i8 = kani::any();

        let result = a.min_of(b);
        kani::assert(result <= a && result <= b, "min_of <= both");
        kani::assert(result == a || result == b, "min_of is one of them");
    }

    #[kani::proof]
    fn proof_ord_ext_clamp_to_bounds() {
        let value: i8 = kani::any();
        let min: i8 = kani::any();
        let max: i8 = kani::any();
        kani::assume(min <= max);

        let result = value.clamp_to(min, max);
        kani::assert(result >= min && result <= max, "clamp_to within bounds");
    }

    #[kani::proof]
    fn proof_ord_ext_clamp_to_preserves() {
        let value: i8 = kani::any();
        let min: i8 = kani::any();
        let max: i8 = kani::any();
        kani::assume(min <= max && value >= min && value <= max);

        let result = value.clamp_to(min, max);
        kani::assert(result == value, "clamp_to preserves in-range");
    }

    #[kani::proof]
    fn proof_ord_ext_in_range_true() {
        let value: i8 = kani::any();
        let min: i8 = kani::any();
        let max: i8 = kani::any();
        kani::assume(min <= max && value >= min && value <= max);

        kani::assert(value.in_range(&min, &max), "in_range true for valid");
    }

    #[kani::proof]
    fn proof_ord_ext_in_range_false_below() {
        let value: i8 = kani::any();
        let min: i8 = kani::any();
        let max: i8 = kani::any();
        kani::assume(min <= max && value < min);

        kani::assert(!value.in_range(&min, &max), "in_range false below min");
    }

    #[kani::proof]
    fn proof_ord_ext_in_range_false_above() {
        let value: i8 = kani::any();
        let min: i8 = kani::any();
        let max: i8 = kani::any();
        kani::assume(min <= max && value > max);

        kani::assert(!value.in_range(&min, &max), "in_range false above max");
    }

    #[kani::proof]
    fn proof_ord_ext_compare_to() {
        let a: i8 = kani::any();
        let b: i8 = kani::any();

        kani::assert(a.compare_to(&b) == a.cmp(&b), "compare_to == cmp");
    }

    // ========================================================================
    // PartialOrdExt Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_partial_ord_ext_try_max_comparable() {
        let a: i8 = kani::any();
        let b: i8 = kani::any();

        let result = a.try_max(b);
        kani::assert(result.is_some(), "try_max succeeds for i8");
        kani::assert(
            result.unwrap() >= a && result.unwrap() >= b,
            "try_max >= both",
        );
    }

    #[kani::proof]
    fn proof_partial_ord_ext_try_min_comparable() {
        let a: i8 = kani::any();
        let b: i8 = kani::any();

        let result = a.try_min(b);
        kani::assert(result.is_some(), "try_min succeeds for i8");
        kani::assert(
            result.unwrap() <= a && result.unwrap() <= b,
            "try_min <= both",
        );
    }

    #[kani::proof]
    fn proof_partial_ord_ext_is_comparable_i8() {
        let a: i8 = kani::any();
        let b: i8 = kani::any();

        kani::assert(a.is_comparable(&b), "i8 always comparable");
    }

    #[kani::proof]
    fn proof_partial_ord_ext_is_lt() {
        let a: i8 = kani::any();
        let b: i8 = kani::any();
        kani::assume(a < b);

        kani::assert(a.is_lt(&b), "is_lt when a < b");
        kani::assert(!b.is_lt(&a), "not is_lt when b > a");
    }

    #[kani::proof]
    fn proof_partial_ord_ext_is_gt() {
        let a: i8 = kani::any();
        let b: i8 = kani::any();
        kani::assume(a > b);

        kani::assert(a.is_gt(&b), "is_gt when a > b");
        kani::assert(!b.is_gt(&a), "not is_gt when b < a");
    }

    // ========================================================================
    // OrderingBuilder Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_ordering_builder_new_equal() {
        let builder = OrderingBuilder::new();
        kani::assert(builder.finish() == Ordering::Equal, "new builder is Equal");
    }

    #[kani::proof]
    fn proof_ordering_builder_default_equal() {
        let builder = OrderingBuilder::default();
        kani::assert(
            builder.finish() == Ordering::Equal,
            "default builder is Equal",
        );
    }

    #[kani::proof]
    fn proof_ordering_builder_compare_less() {
        let a: i8 = kani::any();
        let b: i8 = kani::any();
        kani::assume(a < b);

        let result = OrderingBuilder::new().compare(&a, &b).finish();
        kani::assert(result == Ordering::Less, "compare a < b is Less");
    }

    #[kani::proof]
    fn proof_ordering_builder_compare_greater() {
        let a: i8 = kani::any();
        let b: i8 = kani::any();
        kani::assume(a > b);

        let result = OrderingBuilder::new().compare(&a, &b).finish();
        kani::assert(result == Ordering::Greater, "compare a > b is Greater");
    }

    #[kani::proof]
    fn proof_ordering_builder_compare_equal() {
        let a: i8 = kani::any();

        let result = OrderingBuilder::new().compare(&a, &a).finish();
        kani::assert(result == Ordering::Equal, "compare a == a is Equal");
    }

    #[kani::proof]
    fn proof_ordering_builder_then_used_when_equal() {
        let ord: Ordering = kani::any();

        let result = OrderingBuilder::new().then(ord).finish();
        kani::assert(result == ord, "then uses ord when Equal");
    }

    #[kani::proof]
    fn proof_ordering_builder_then_ignored_when_not_equal() {
        let result = OrderingBuilder::new()
            .then(Ordering::Less)
            .then(Ordering::Greater) // Should be ignored
            .finish();

        kani::assert(result == Ordering::Less, "then ignores after non-Equal");
    }

    #[kani::proof]
    fn proof_ordering_builder_chain() {
        let a1: i8 = kani::any();
        let a2: i8 = kani::any();
        kani::assume(a1 < a2);

        // First compare is equal, second is not
        let result = OrderingBuilder::new()
            .compare(&1i8, &1i8)
            .compare(&a1, &a2)
            .finish();

        kani::assert(result == Ordering::Less, "chain uses first non-Equal");
    }

    // ========================================================================
    // reverse() Function Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_reverse_less() {
        kani::assert(reverse(Ordering::Less) == Ordering::Greater, "reverse Less");
    }

    #[kani::proof]
    fn proof_reverse_equal() {
        kani::assert(reverse(Ordering::Equal) == Ordering::Equal, "reverse Equal");
    }

    #[kani::proof]
    fn proof_reverse_greater() {
        kani::assert(
            reverse(Ordering::Greater) == Ordering::Less,
            "reverse Greater",
        );
    }

    #[kani::proof]
    fn proof_reverse_involution() {
        let ord: Ordering = kani::any();
        kani::assert(reverse(reverse(ord)) == ord, "reverse is involution");
    }

    // ========================================================================
    // from_cmp() Function Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_from_cmp_consistent() {
        let a: i8 = kani::any();
        let b: i8 = kani::any();

        kani::assert(from_cmp(&a, &b) == a.cmp(&b), "from_cmp == cmp");
    }

    // ========================================================================
    // from_bool() Function Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_from_bool_true() {
        kani::assert(
            from_bool(true) == Ordering::Greater,
            "from_bool(true) == Greater",
        );
    }

    #[kani::proof]
    fn proof_from_bool_false() {
        kani::assert(
            from_bool(false) == Ordering::Less,
            "from_bool(false) == Less",
        );
    }
}
