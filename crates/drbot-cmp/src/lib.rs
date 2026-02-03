//! Comparison utilities for drbot.
//!
//! This crate provides:
//! - Comparison helpers
//! - Chained comparisons
//! - Custom comparators

use std::cmp::Ordering;
use thiserror::Error;

/// Comparison error types.
#[derive(Error, Debug, Clone)]
pub enum CmpError {
    #[error("Comparison failed")]
    ComparisonFailed,
}

/// Result type for comparison operations.
pub type Result<T> = std::result::Result<T, CmpError>;

/// Compare two values and return ordering.
pub fn compare<T: Ord>(a: &T, b: &T) -> Ordering {
    a.cmp(b)
}

/// Compare with custom comparator.
pub fn compare_by<T, F: Fn(&T, &T) -> Ordering>(a: &T, b: &T, f: F) -> Ordering {
    f(a, b)
}

/// Compare by key extraction.
pub fn compare_by_key<T, K: Ord, F: Fn(&T) -> K>(a: &T, b: &T, f: F) -> Ordering {
    f(a).cmp(&f(b))
}

/// Reverse comparison result.
pub fn reverse_ordering(ord: Ordering) -> Ordering {
    ord.reverse()
}

/// Chain two orderings (use second if first is Equal).
pub fn then_ordering(first: Ordering, second: Ordering) -> Ordering {
    first.then(second)
}

/// A comparator function wrapper.
pub struct Comparator<T, F>
where
    F: Fn(&T, &T) -> Ordering,
{
    compare_fn: F,
    _marker: std::marker::PhantomData<T>,
}

impl<T, F> Comparator<T, F>
where
    F: Fn(&T, &T) -> Ordering,
{
    /// Create new comparator.
    pub fn new(compare_fn: F) -> Self {
        Self {
            compare_fn,
            _marker: std::marker::PhantomData,
        }
    }

    /// Compare two values.
    pub fn compare(&self, a: &T, b: &T) -> Ordering {
        (self.compare_fn)(a, b)
    }

    /// Check if a < b.
    pub fn lt(&self, a: &T, b: &T) -> bool {
        self.compare(a, b) == Ordering::Less
    }

    /// Check if a <= b.
    pub fn le(&self, a: &T, b: &T) -> bool {
        self.compare(a, b) != Ordering::Greater
    }

    /// Check if a > b.
    pub fn gt(&self, a: &T, b: &T) -> bool {
        self.compare(a, b) == Ordering::Greater
    }

    /// Check if a >= b.
    pub fn ge(&self, a: &T, b: &T) -> bool {
        self.compare(a, b) != Ordering::Less
    }

    /// Check if a == b.
    pub fn eq(&self, a: &T, b: &T) -> bool {
        self.compare(a, b) == Ordering::Equal
    }
}

/// Create comparator from Ord trait.
pub fn natural_comparator<T: Ord>() -> Comparator<T, impl Fn(&T, &T) -> Ordering> {
    Comparator::new(|a: &T, b: &T| a.cmp(b))
}

/// Create reverse comparator.
pub fn reverse_comparator<T: Ord>() -> Comparator<T, impl Fn(&T, &T) -> Ordering> {
    Comparator::new(|a: &T, b: &T| b.cmp(a))
}

/// Create comparator by key.
pub fn key_comparator<T, K: Ord, F: Fn(&T) -> K + Clone>(
    key_fn: F,
) -> Comparator<T, impl Fn(&T, &T) -> Ordering> {
    Comparator::new(move |a: &T, b: &T| key_fn(a).cmp(&key_fn(b)))
}

/// Chained comparator builder.
pub struct ChainedComparator<T> {
    comparators: Vec<Box<dyn Fn(&T, &T) -> Ordering + Send + Sync>>,
}

impl<T: 'static> ChainedComparator<T> {
    /// Create new chained comparator.
    pub fn new() -> Self {
        Self {
            comparators: Vec::new(),
        }
    }

    /// Add comparator.
    pub fn then<F: Fn(&T, &T) -> Ordering + Send + Sync + 'static>(mut self, f: F) -> Self {
        self.comparators.push(Box::new(f));
        self
    }

    /// Add comparator by key.
    pub fn then_by_key<K: Ord + 'static, F: Fn(&T) -> K + Send + Sync + 'static>(
        self,
        key_fn: F,
    ) -> Self {
        self.then(move |a, b| key_fn(a).cmp(&key_fn(b)))
    }

    /// Compare two values.
    pub fn compare(&self, a: &T, b: &T) -> Ordering {
        for cmp in &self.comparators {
            match cmp(a, b) {
                Ordering::Equal => continue,
                other => return other,
            }
        }
        Ordering::Equal
    }
}

impl<T: 'static> Default for ChainedComparator<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Three-way comparison result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ThreeWay {
    /// First is less.
    Less,
    /// Both are equal.
    Equal,
    /// First is greater.
    Greater,
}

impl From<Ordering> for ThreeWay {
    fn from(ord: Ordering) -> Self {
        match ord {
            Ordering::Less => Self::Less,
            Ordering::Equal => Self::Equal,
            Ordering::Greater => Self::Greater,
        }
    }
}

impl From<ThreeWay> for Ordering {
    fn from(tw: ThreeWay) -> Self {
        match tw {
            ThreeWay::Less => Self::Less,
            ThreeWay::Equal => Self::Equal,
            ThreeWay::Greater => Self::Greater,
        }
    }
}

/// Check if value is between bounds (inclusive).
pub fn between<T: Ord>(value: &T, min: &T, max: &T) -> bool {
    value >= min && value <= max
}

/// Check if value is strictly between bounds.
pub fn strictly_between<T: Ord>(value: &T, min: &T, max: &T) -> bool {
    value > min && value < max
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compare() {
        assert_eq!(compare(&1, &2), Ordering::Less);
        assert_eq!(compare(&2, &2), Ordering::Equal);
        assert_eq!(compare(&3, &2), Ordering::Greater);
    }

    #[test]
    fn test_comparator() {
        let cmp = natural_comparator::<i32>();
        assert!(cmp.lt(&1, &2));
        assert!(cmp.eq(&2, &2));
        assert!(cmp.gt(&3, &2));
    }

    #[test]
    fn test_reverse_comparator() {
        let cmp = reverse_comparator::<i32>();
        assert!(cmp.gt(&1, &2));
        assert!(cmp.lt(&3, &2));
    }

    #[test]
    fn test_key_comparator() {
        let cmp = key_comparator(|s: &String| s.len());
        assert_eq!(
            cmp.compare(&"a".to_string(), &"bb".to_string()),
            Ordering::Less
        );
    }

    #[test]
    fn test_chained_comparator() {
        #[derive(Debug)]
        struct Person {
            name: String,
            age: u32,
        }

        let cmp = ChainedComparator::new()
            .then_by_key(|p: &Person| p.age)
            .then_by_key(|p: &Person| p.name.clone());

        let alice = Person {
            name: "Alice".to_string(),
            age: 30,
        };
        let bob = Person {
            name: "Bob".to_string(),
            age: 30,
        };

        assert_eq!(cmp.compare(&alice, &bob), Ordering::Less);
    }

    #[test]
    fn test_between() {
        assert!(between(&5, &1, &10));
        assert!(between(&1, &1, &10));
        assert!(!between(&0, &1, &10));
    }
}

// ============================================================================
// Kani Formal Verification Proofs
// ============================================================================

#[cfg(kani)]
mod kani_proofs {
    use super::*;

    // ========================================================================
    // compare() Function Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_compare_reflexive() {
        let x: i8 = kani::any();
        kani::assert(compare(&x, &x) == Ordering::Equal, "compare reflexive");
    }

    #[kani::proof]
    fn proof_compare_antisymmetric() {
        let a: i8 = kani::any();
        let b: i8 = kani::any();

        let ab = compare(&a, &b);
        let ba = compare(&b, &a);

        kani::assert(ab == ba.reverse(), "compare antisymmetric");
    }

    #[kani::proof]
    fn proof_compare_less() {
        let a: i8 = kani::any();
        let b: i8 = kani::any();
        kani::assume(a < b);

        kani::assert(compare(&a, &b) == Ordering::Less, "a < b implies Less");
    }

    #[kani::proof]
    fn proof_compare_greater() {
        let a: i8 = kani::any();
        let b: i8 = kani::any();
        kani::assume(a > b);

        kani::assert(
            compare(&a, &b) == Ordering::Greater,
            "a > b implies Greater",
        );
    }

    #[kani::proof]
    fn proof_compare_equal() {
        let a: i8 = kani::any();
        let b: i8 = kani::any();
        kani::assume(a == b);

        kani::assert(compare(&a, &b) == Ordering::Equal, "a == b implies Equal");
    }

    // ========================================================================
    // compare_by_key() Function Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_compare_by_key_reflexive() {
        let x: i8 = kani::any();
        let result = compare_by_key(&x, &x, |v: &i8| *v);
        kani::assert(result == Ordering::Equal, "compare_by_key reflexive");
    }

    // ========================================================================
    // reverse_ordering() Function Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_reverse_ordering_less() {
        kani::assert(
            reverse_ordering(Ordering::Less) == Ordering::Greater,
            "reverse Less to Greater",
        );
    }

    #[kani::proof]
    fn proof_reverse_ordering_equal() {
        kani::assert(
            reverse_ordering(Ordering::Equal) == Ordering::Equal,
            "reverse Equal to Equal",
        );
    }

    #[kani::proof]
    fn proof_reverse_ordering_greater() {
        kani::assert(
            reverse_ordering(Ordering::Greater) == Ordering::Less,
            "reverse Greater to Less",
        );
    }

    #[kani::proof]
    fn proof_reverse_ordering_involution() {
        let ord: Ordering = kani::any();
        let reversed_twice = reverse_ordering(reverse_ordering(ord));
        kani::assert(reversed_twice == ord, "double reverse is identity");
    }

    // ========================================================================
    // then_ordering() Function Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_then_ordering_less_first() {
        let second: Ordering = kani::any();
        kani::assert(
            then_ordering(Ordering::Less, second) == Ordering::Less,
            "Less first stays Less",
        );
    }

    #[kani::proof]
    fn proof_then_ordering_greater_first() {
        let second: Ordering = kani::any();
        kani::assert(
            then_ordering(Ordering::Greater, second) == Ordering::Greater,
            "Greater first stays Greater",
        );
    }

    #[kani::proof]
    fn proof_then_ordering_equal_uses_second() {
        let second: Ordering = kani::any();
        kani::assert(
            then_ordering(Ordering::Equal, second) == second,
            "Equal first uses second",
        );
    }

    // ========================================================================
    // Comparator Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_comparator_lt_consistent() {
        let a: i8 = kani::any();
        let b: i8 = kani::any();

        let cmp = Comparator::new(|x: &i8, y: &i8| x.cmp(y));
        let lt_result = cmp.lt(&a, &b);
        let cmp_result = cmp.compare(&a, &b) == Ordering::Less;

        kani::assert(lt_result == cmp_result, "lt consistent with compare");
    }

    #[kani::proof]
    fn proof_comparator_le_consistent() {
        let a: i8 = kani::any();
        let b: i8 = kani::any();

        let cmp = Comparator::new(|x: &i8, y: &i8| x.cmp(y));
        let le_result = cmp.le(&a, &b);
        let cmp_result = cmp.compare(&a, &b) != Ordering::Greater;

        kani::assert(le_result == cmp_result, "le consistent with compare");
    }

    #[kani::proof]
    fn proof_comparator_gt_consistent() {
        let a: i8 = kani::any();
        let b: i8 = kani::any();

        let cmp = Comparator::new(|x: &i8, y: &i8| x.cmp(y));
        let gt_result = cmp.gt(&a, &b);
        let cmp_result = cmp.compare(&a, &b) == Ordering::Greater;

        kani::assert(gt_result == cmp_result, "gt consistent with compare");
    }

    #[kani::proof]
    fn proof_comparator_ge_consistent() {
        let a: i8 = kani::any();
        let b: i8 = kani::any();

        let cmp = Comparator::new(|x: &i8, y: &i8| x.cmp(y));
        let ge_result = cmp.ge(&a, &b);
        let cmp_result = cmp.compare(&a, &b) != Ordering::Less;

        kani::assert(ge_result == cmp_result, "ge consistent with compare");
    }

    #[kani::proof]
    fn proof_comparator_eq_consistent() {
        let a: i8 = kani::any();
        let b: i8 = kani::any();

        let cmp = Comparator::new(|x: &i8, y: &i8| x.cmp(y));
        let eq_result = cmp.eq(&a, &b);
        let cmp_result = cmp.compare(&a, &b) == Ordering::Equal;

        kani::assert(eq_result == cmp_result, "eq consistent with compare");
    }

    #[kani::proof]
    fn proof_comparator_lt_gt_exclusive() {
        let a: i8 = kani::any();
        let b: i8 = kani::any();

        let cmp = Comparator::new(|x: &i8, y: &i8| x.cmp(y));

        // Cannot be both lt and gt
        kani::assert(!(cmp.lt(&a, &b) && cmp.gt(&a, &b)), "lt and gt exclusive");
    }

    #[kani::proof]
    fn proof_comparator_le_ge_overlap_eq() {
        let a: i8 = kani::any();
        let b: i8 = kani::any();

        let cmp = Comparator::new(|x: &i8, y: &i8| x.cmp(y));

        // If both le and ge, must be equal
        if cmp.le(&a, &b) && cmp.ge(&a, &b) {
            kani::assert(cmp.eq(&a, &b), "le and ge implies eq");
        }
    }

    // ========================================================================
    // ThreeWay Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_threeway_from_ordering_less() {
        let tw: ThreeWay = Ordering::Less.into();
        kani::assert(tw == ThreeWay::Less, "Ordering::Less to ThreeWay::Less");
    }

    #[kani::proof]
    fn proof_threeway_from_ordering_equal() {
        let tw: ThreeWay = Ordering::Equal.into();
        kani::assert(tw == ThreeWay::Equal, "Ordering::Equal to ThreeWay::Equal");
    }

    #[kani::proof]
    fn proof_threeway_from_ordering_greater() {
        let tw: ThreeWay = Ordering::Greater.into();
        kani::assert(
            tw == ThreeWay::Greater,
            "Ordering::Greater to ThreeWay::Greater",
        );
    }

    #[kani::proof]
    fn proof_threeway_to_ordering_less() {
        let ord: Ordering = ThreeWay::Less.into();
        kani::assert(ord == Ordering::Less, "ThreeWay::Less to Ordering::Less");
    }

    #[kani::proof]
    fn proof_threeway_to_ordering_equal() {
        let ord: Ordering = ThreeWay::Equal.into();
        kani::assert(ord == Ordering::Equal, "ThreeWay::Equal to Ordering::Equal");
    }

    #[kani::proof]
    fn proof_threeway_to_ordering_greater() {
        let ord: Ordering = ThreeWay::Greater.into();
        kani::assert(
            ord == Ordering::Greater,
            "ThreeWay::Greater to Ordering::Greater",
        );
    }

    #[kani::proof]
    fn proof_threeway_roundtrip() {
        let tw = ThreeWay::Less;
        let ord: Ordering = tw.into();
        let tw2: ThreeWay = ord.into();
        kani::assert(tw == tw2, "ThreeWay roundtrip Less");

        let tw = ThreeWay::Equal;
        let ord: Ordering = tw.into();
        let tw2: ThreeWay = ord.into();
        kani::assert(tw == tw2, "ThreeWay roundtrip Equal");

        let tw = ThreeWay::Greater;
        let ord: Ordering = tw.into();
        let tw2: ThreeWay = ord.into();
        kani::assert(tw == tw2, "ThreeWay roundtrip Greater");
    }

    #[kani::proof]
    fn proof_threeway_distinct() {
        kani::assert(ThreeWay::Less != ThreeWay::Equal, "Less != Equal");
        kani::assert(ThreeWay::Less != ThreeWay::Greater, "Less != Greater");
        kani::assert(ThreeWay::Equal != ThreeWay::Greater, "Equal != Greater");
    }

    // ========================================================================
    // between() Function Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_between_inclusive_min() {
        let min: i8 = kani::any();
        let max: i8 = kani::any();
        kani::assume(min <= max);

        kani::assert(between(&min, &min, &max), "min is between min and max");
    }

    #[kani::proof]
    fn proof_between_inclusive_max() {
        let min: i8 = kani::any();
        let max: i8 = kani::any();
        kani::assume(min <= max);

        kani::assert(between(&max, &min, &max), "max is between min and max");
    }

    #[kani::proof]
    fn proof_between_middle() {
        let min: i8 = kani::any();
        let mid: i8 = kani::any();
        let max: i8 = kani::any();
        kani::assume(min <= mid && mid <= max);

        kani::assert(between(&mid, &min, &max), "mid is between min and max");
    }

    #[kani::proof]
    fn proof_between_below_min() {
        let value: i8 = kani::any();
        let min: i8 = kani::any();
        let max: i8 = kani::any();
        kani::assume(value < min && min <= max);

        kani::assert(!between(&value, &min, &max), "below min not between");
    }

    #[kani::proof]
    fn proof_between_above_max() {
        let value: i8 = kani::any();
        let min: i8 = kani::any();
        let max: i8 = kani::any();
        kani::assume(value > max && min <= max);

        kani::assert(!between(&value, &min, &max), "above max not between");
    }

    // ========================================================================
    // strictly_between() Function Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_strictly_between_excludes_min() {
        let min: i8 = kani::any();
        let max: i8 = kani::any();
        kani::assume(min < max);

        kani::assert(
            !strictly_between(&min, &min, &max),
            "min not strictly between",
        );
    }

    #[kani::proof]
    fn proof_strictly_between_excludes_max() {
        let min: i8 = kani::any();
        let max: i8 = kani::any();
        kani::assume(min < max);

        kani::assert(
            !strictly_between(&max, &min, &max),
            "max not strictly between",
        );
    }

    #[kani::proof]
    fn proof_strictly_between_middle() {
        let min: i8 = kani::any();
        let mid: i8 = kani::any();
        let max: i8 = kani::any();
        kani::assume(min < mid && mid < max);

        kani::assert(strictly_between(&mid, &min, &max), "mid strictly between");
    }

    #[kani::proof]
    fn proof_strictly_between_implies_between() {
        let value: i8 = kani::any();
        let min: i8 = kani::any();
        let max: i8 = kani::any();
        kani::assume(min <= max);

        if strictly_between(&value, &min, &max) {
            kani::assert(
                between(&value, &min, &max),
                "strictly between implies between",
            );
        }
    }

    // ========================================================================
    // ChainedComparator Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_chained_comparator_empty_equal() {
        let cmp: ChainedComparator<i8> = ChainedComparator::new();
        let a: i8 = kani::any();
        let b: i8 = kani::any();

        kani::assert(
            cmp.compare(&a, &b) == Ordering::Equal,
            "empty chain returns Equal",
        );
    }

    #[kani::proof]
    fn proof_chained_comparator_default_equal() {
        let cmp: ChainedComparator<i8> = ChainedComparator::default();
        let a: i8 = kani::any();
        let b: i8 = kani::any();

        kani::assert(
            cmp.compare(&a, &b) == Ordering::Equal,
            "default chain returns Equal",
        );
    }
}
