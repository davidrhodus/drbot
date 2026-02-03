//! Non-zero type utilities for drbot.
//!
//! This crate provides:
//! - Non-zero wrapper utilities
//! - Positive/negative constraints
//! - Non-empty validation

use std::num::{NonZeroI32, NonZeroI64, NonZeroU32, NonZeroU64, NonZeroUsize};
use thiserror::Error;

/// NonZero error types.
#[derive(Error, Debug, Clone)]
pub enum NonZeroError {
    #[error("Value is zero")]
    IsZero,

    #[error("Value is negative")]
    IsNegative,

    #[error("Value is not positive")]
    NotPositive,
}

/// Result type for non-zero operations.
pub type Result<T> = std::result::Result<T, NonZeroError>;

/// Extension trait for creating non-zero values.
pub trait TryIntoNonZero: Sized {
    /// The non-zero type.
    type NonZero;

    /// Try to convert to non-zero.
    fn try_into_nonzero(self) -> Result<Self::NonZero>;
}

impl TryIntoNonZero for u32 {
    type NonZero = NonZeroU32;

    fn try_into_nonzero(self) -> Result<Self::NonZero> {
        NonZeroU32::new(self).ok_or(NonZeroError::IsZero)
    }
}

impl TryIntoNonZero for u64 {
    type NonZero = NonZeroU64;

    fn try_into_nonzero(self) -> Result<Self::NonZero> {
        NonZeroU64::new(self).ok_or(NonZeroError::IsZero)
    }
}

impl TryIntoNonZero for usize {
    type NonZero = NonZeroUsize;

    fn try_into_nonzero(self) -> Result<Self::NonZero> {
        NonZeroUsize::new(self).ok_or(NonZeroError::IsZero)
    }
}

impl TryIntoNonZero for i32 {
    type NonZero = NonZeroI32;

    fn try_into_nonzero(self) -> Result<Self::NonZero> {
        NonZeroI32::new(self).ok_or(NonZeroError::IsZero)
    }
}

impl TryIntoNonZero for i64 {
    type NonZero = NonZeroI64;

    fn try_into_nonzero(self) -> Result<Self::NonZero> {
        NonZeroI64::new(self).ok_or(NonZeroError::IsZero)
    }
}

/// A positive integer (> 0).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Positive<T>(T);

impl<T> Positive<T> {
    /// Get inner value.
    pub fn into_inner(self) -> T {
        self.0
    }

    /// Get reference.
    pub fn get(&self) -> &T {
        &self.0
    }
}

impl Positive<i32> {
    /// Create positive i32.
    pub fn new(value: i32) -> Result<Self> {
        if value > 0 {
            Ok(Self(value))
        } else {
            Err(NonZeroError::NotPositive)
        }
    }
}

impl Positive<i64> {
    /// Create positive i64.
    pub fn new(value: i64) -> Result<Self> {
        if value > 0 {
            Ok(Self(value))
        } else {
            Err(NonZeroError::NotPositive)
        }
    }
}

impl Positive<f32> {
    /// Create positive f32.
    pub fn new(value: f32) -> Result<Self> {
        if value > 0.0 {
            Ok(Self(value))
        } else {
            Err(NonZeroError::NotPositive)
        }
    }
}

impl Positive<f64> {
    /// Create positive f64.
    pub fn new(value: f64) -> Result<Self> {
        if value > 0.0 {
            Ok(Self(value))
        } else {
            Err(NonZeroError::NotPositive)
        }
    }
}

/// A non-negative integer (>= 0).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NonNegative<T>(T);

impl<T> NonNegative<T> {
    /// Get inner value.
    pub fn into_inner(self) -> T {
        self.0
    }

    /// Get reference.
    pub fn get(&self) -> &T {
        &self.0
    }
}

impl NonNegative<i32> {
    /// Create non-negative i32.
    pub fn new(value: i32) -> Result<Self> {
        if value >= 0 {
            Ok(Self(value))
        } else {
            Err(NonZeroError::IsNegative)
        }
    }

    /// Create from zero.
    pub fn zero() -> Self {
        Self(0)
    }
}

impl NonNegative<i64> {
    /// Create non-negative i64.
    pub fn new(value: i64) -> Result<Self> {
        if value >= 0 {
            Ok(Self(value))
        } else {
            Err(NonZeroError::IsNegative)
        }
    }

    /// Create from zero.
    pub fn zero() -> Self {
        Self(0)
    }
}

/// Create non-zero u32 or panic.
pub fn nonzero_u32(value: u32) -> NonZeroU32 {
    NonZeroU32::new(value).expect("value must be non-zero")
}

/// Create non-zero u64 or panic.
pub fn nonzero_u64(value: u64) -> NonZeroU64 {
    NonZeroU64::new(value).expect("value must be non-zero")
}

/// Create non-zero usize or panic.
pub fn nonzero_usize(value: usize) -> NonZeroUsize {
    NonZeroUsize::new(value).expect("value must be non-zero")
}

/// Create non-zero u32 with default of 1.
pub fn nonzero_u32_or_one(value: u32) -> NonZeroU32 {
    NonZeroU32::new(value).unwrap_or(NonZeroU32::new(1).unwrap())
}

/// Create non-zero usize with default of 1.
pub fn nonzero_usize_or_one(value: usize) -> NonZeroUsize {
    NonZeroUsize::new(value).unwrap_or(NonZeroUsize::new(1).unwrap())
}

/// Divide rounding up using non-zero divisor.
pub fn div_ceil_nonzero(a: usize, b: NonZeroUsize) -> usize {
    (a + b.get() - 1) / b.get()
}

/// Check if value is zero.
pub fn is_zero<T: Default + PartialEq>(value: &T) -> bool {
    *value == T::default()
}

/// Check if value is non-zero.
pub fn is_nonzero<T: Default + PartialEq>(value: &T) -> bool {
    *value != T::default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_try_into_nonzero() {
        let nz: Result<NonZeroU32> = 42u32.try_into_nonzero();
        assert!(nz.is_ok());
        assert_eq!(nz.unwrap().get(), 42);

        let zero: Result<NonZeroU32> = 0u32.try_into_nonzero();
        assert!(zero.is_err());
    }

    #[test]
    fn test_positive() {
        assert!(Positive::new(42i32).is_ok());
        assert!(Positive::new(0i32).is_err());
        assert!(Positive::new(-1i32).is_err());

        assert!(Positive::new(3.14f64).is_ok());
        assert!(Positive::new(0.0f64).is_err());
    }

    #[test]
    fn test_non_negative() {
        assert!(NonNegative::new(42i32).is_ok());
        assert!(NonNegative::new(0i32).is_ok());
        assert!(NonNegative::new(-1i32).is_err());
    }

    #[test]
    fn test_div_ceil() {
        let divisor = NonZeroUsize::new(3).unwrap();
        assert_eq!(div_ceil_nonzero(10, divisor), 4);
        assert_eq!(div_ceil_nonzero(9, divisor), 3);
    }

    #[test]
    fn test_is_zero() {
        assert!(is_zero(&0));
        assert!(!is_zero(&1));
        assert!(is_nonzero(&1));
    }
}
