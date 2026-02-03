//! Equality extension utilities for drbot.
//!
//! This crate provides:
//! - Equality extensions
//! - Approximate equality
//! - Custom equality

use thiserror::Error;

/// Equality error types.
#[derive(Error, Debug, Clone)]
pub enum EqError {
    #[error("Values are not equal")]
    NotEqual,
}

/// Result type for equality operations.
pub type Result<T> = std::result::Result<T, EqError>;

/// Extension trait for PartialEq types.
pub trait EqExt: PartialEq + Sized {
    /// Check if equal to any of the given values.
    fn is_any_of(&self, values: &[Self]) -> bool {
        values.iter().any(|v| self == v)
    }

    /// Check if not equal.
    fn is_ne(&self, other: &Self) -> bool {
        self != other
    }

    /// Return Some(self) if equal to other.
    fn if_eq(self, other: &Self) -> Option<Self> {
        if &self == other {
            Some(self)
        } else {
            None
        }
    }

    /// Return Some(self) if not equal to other.
    fn if_ne(self, other: &Self) -> Option<Self> {
        if &self != other {
            Some(self)
        } else {
            None
        }
    }
}

impl<T: PartialEq> EqExt for T {}

/// Approximate equality for floating point.
pub trait ApproxEq {
    /// Check if approximately equal with epsilon.
    fn approx_eq(&self, other: &Self, epsilon: Self) -> bool;

    /// Check if approximately equal with relative epsilon.
    fn approx_eq_relative(&self, other: &Self, epsilon: Self) -> bool;
}

impl ApproxEq for f32 {
    fn approx_eq(&self, other: &Self, epsilon: Self) -> bool {
        (self - other).abs() <= epsilon
    }

    fn approx_eq_relative(&self, other: &Self, epsilon: Self) -> bool {
        let diff = (self - other).abs();
        let largest = self.abs().max(other.abs());
        diff <= largest * epsilon
    }
}

impl ApproxEq for f64 {
    fn approx_eq(&self, other: &Self, epsilon: Self) -> bool {
        (self - other).abs() <= epsilon
    }

    fn approx_eq_relative(&self, other: &Self, epsilon: Self) -> bool {
        let diff = (self - other).abs();
        let largest = self.abs().max(other.abs());
        diff <= largest * epsilon
    }
}

/// Check if two floats are approximately equal.
pub fn approx_eq_f32(a: f32, b: f32, epsilon: f32) -> bool {
    a.approx_eq(&b, epsilon)
}

/// Check if two floats are approximately equal.
pub fn approx_eq_f64(a: f64, b: f64, epsilon: f64) -> bool {
    a.approx_eq(&b, epsilon)
}

/// Default epsilon for f32.
pub const F32_EPSILON: f32 = 1e-6;

/// Default epsilon for f64.
pub const F64_EPSILON: f64 = 1e-10;

/// Compare with custom equality function.
pub fn eq_by<T, F: Fn(&T, &T) -> bool>(a: &T, b: &T, f: F) -> bool {
    f(a, b)
}

/// Compare by key extraction.
pub fn eq_by_key<T, K: Eq, F: Fn(&T) -> K>(a: &T, b: &T, f: F) -> bool {
    f(a) == f(b)
}

/// A custom equality wrapper.
pub struct EqualBy<T, F>
where
    F: Fn(&T, &T) -> bool,
{
    value: T,
    eq_fn: F,
}

impl<T, F> EqualBy<T, F>
where
    F: Fn(&T, &T) -> bool,
{
    /// Create new wrapper.
    pub fn new(value: T, eq_fn: F) -> Self {
        Self { value, eq_fn }
    }

    /// Get inner value.
    pub fn into_inner(self) -> T {
        self.value
    }

    /// Check equality with another wrapped value.
    pub fn eq(&self, other: &Self) -> bool {
        (self.eq_fn)(&self.value, &other.value)
    }
}

/// Case-insensitive string equality.
pub fn eq_ignore_case(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

/// Check if all elements in slice are equal.
pub fn all_equal<T: Eq>(items: &[T]) -> bool {
    if items.is_empty() {
        return true;
    }
    let first = &items[0];
    items.iter().all(|item| item == first)
}

/// Check if all elements in slice are equal by key.
pub fn all_equal_by_key<T, K: Eq, F: Fn(&T) -> K>(items: &[T], f: F) -> bool {
    if items.is_empty() {
        return true;
    }
    let first_key = f(&items[0]);
    items.iter().all(|item| f(item) == first_key)
}

/// Count equal pairs in slice.
pub fn count_equal_pairs<T: Eq>(items: &[T]) -> usize {
    let mut count = 0;
    for i in 0..items.len() {
        for j in (i + 1)..items.len() {
            if items[i] == items[j] {
                count += 1;
            }
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_eq_ext() {
        assert!(5.is_any_of(&[1, 3, 5, 7]));
        assert!(!5.is_any_of(&[1, 2, 3]));
        assert!(5.is_ne(&3));
    }

    #[test]
    fn test_approx_eq() {
        assert!(1.0f64.approx_eq(&1.0000000001, F64_EPSILON));
        assert!(!1.0f64.approx_eq(&1.1, F64_EPSILON));
    }

    #[test]
    fn test_eq_ignore_case() {
        assert!(eq_ignore_case("Hello", "hello"));
        assert!(!eq_ignore_case("Hello", "World"));
    }

    #[test]
    fn test_all_equal() {
        assert!(all_equal(&[1, 1, 1]));
        assert!(!all_equal(&[1, 2, 1]));
        assert!(all_equal::<i32>(&[]));
    }

    #[test]
    fn test_eq_by_key() {
        let a = "hello";
        let b = "world";
        assert!(eq_by_key(&a, &b, |s: &&str| s.len()));
    }

    #[test]
    fn test_if_eq() {
        let x = 5;
        assert_eq!(x.if_eq(&5), Some(5));
        assert_eq!(x.if_eq(&3), None);
    }
}
