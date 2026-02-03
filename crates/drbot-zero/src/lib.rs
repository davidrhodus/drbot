//! Zero/null value utilities for drbot.
//!
//! This crate provides:
//! - Zero trait
//! - Zero checking
//! - Zero initialization

use thiserror::Error;

/// Zero error types.
#[derive(Error, Debug, Clone)]
pub enum ZeroError {
    #[error("Value is zero")]
    IsZero,

    #[error("Division by zero")]
    DivisionByZero,
}

/// Result type for zero operations.
pub type Result<T> = std::result::Result<T, ZeroError>;

/// Zero trait.
pub trait Zero {
    /// Get zero value.
    fn zero() -> Self;

    /// Check if zero.
    fn is_zero(&self) -> bool;
}

impl Zero for i8 {
    fn zero() -> Self {
        0
    }
    fn is_zero(&self) -> bool {
        *self == 0
    }
}

impl Zero for i16 {
    fn zero() -> Self {
        0
    }
    fn is_zero(&self) -> bool {
        *self == 0
    }
}

impl Zero for i32 {
    fn zero() -> Self {
        0
    }
    fn is_zero(&self) -> bool {
        *self == 0
    }
}

impl Zero for i64 {
    fn zero() -> Self {
        0
    }
    fn is_zero(&self) -> bool {
        *self == 0
    }
}

impl Zero for i128 {
    fn zero() -> Self {
        0
    }
    fn is_zero(&self) -> bool {
        *self == 0
    }
}

impl Zero for isize {
    fn zero() -> Self {
        0
    }
    fn is_zero(&self) -> bool {
        *self == 0
    }
}

impl Zero for u8 {
    fn zero() -> Self {
        0
    }
    fn is_zero(&self) -> bool {
        *self == 0
    }
}

impl Zero for u16 {
    fn zero() -> Self {
        0
    }
    fn is_zero(&self) -> bool {
        *self == 0
    }
}

impl Zero for u32 {
    fn zero() -> Self {
        0
    }
    fn is_zero(&self) -> bool {
        *self == 0
    }
}

impl Zero for u64 {
    fn zero() -> Self {
        0
    }
    fn is_zero(&self) -> bool {
        *self == 0
    }
}

impl Zero for u128 {
    fn zero() -> Self {
        0
    }
    fn is_zero(&self) -> bool {
        *self == 0
    }
}

impl Zero for usize {
    fn zero() -> Self {
        0
    }
    fn is_zero(&self) -> bool {
        *self == 0
    }
}

impl Zero for f32 {
    fn zero() -> Self {
        0.0
    }
    fn is_zero(&self) -> bool {
        *self == 0.0
    }
}

impl Zero for f64 {
    fn zero() -> Self {
        0.0
    }
    fn is_zero(&self) -> bool {
        *self == 0.0
    }
}

/// One trait.
pub trait One {
    /// Get one value.
    fn one() -> Self;

    /// Check if one.
    fn is_one(&self) -> bool;
}

impl One for i8 {
    fn one() -> Self {
        1
    }
    fn is_one(&self) -> bool {
        *self == 1
    }
}

impl One for i16 {
    fn one() -> Self {
        1
    }
    fn is_one(&self) -> bool {
        *self == 1
    }
}

impl One for i32 {
    fn one() -> Self {
        1
    }
    fn is_one(&self) -> bool {
        *self == 1
    }
}

impl One for i64 {
    fn one() -> Self {
        1
    }
    fn is_one(&self) -> bool {
        *self == 1
    }
}

impl One for i128 {
    fn one() -> Self {
        1
    }
    fn is_one(&self) -> bool {
        *self == 1
    }
}

impl One for isize {
    fn one() -> Self {
        1
    }
    fn is_one(&self) -> bool {
        *self == 1
    }
}

impl One for u8 {
    fn one() -> Self {
        1
    }
    fn is_one(&self) -> bool {
        *self == 1
    }
}

impl One for u16 {
    fn one() -> Self {
        1
    }
    fn is_one(&self) -> bool {
        *self == 1
    }
}

impl One for u32 {
    fn one() -> Self {
        1
    }
    fn is_one(&self) -> bool {
        *self == 1
    }
}

impl One for u64 {
    fn one() -> Self {
        1
    }
    fn is_one(&self) -> bool {
        *self == 1
    }
}

impl One for u128 {
    fn one() -> Self {
        1
    }
    fn is_one(&self) -> bool {
        *self == 1
    }
}

impl One for usize {
    fn one() -> Self {
        1
    }
    fn is_one(&self) -> bool {
        *self == 1
    }
}

impl One for f32 {
    fn one() -> Self {
        1.0
    }
    fn is_one(&self) -> bool {
        *self == 1.0
    }
}

impl One for f64 {
    fn one() -> Self {
        1.0
    }
    fn is_one(&self) -> bool {
        *self == 1.0
    }
}

/// Non-zero wrapper.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NonZero<T: Zero>(T);

impl<T: Zero + Clone> NonZero<T> {
    /// Create new non-zero value.
    pub fn new(value: T) -> Result<Self> {
        if value.is_zero() {
            Err(ZeroError::IsZero)
        } else {
            Ok(Self(value))
        }
    }

    /// Get value.
    pub fn get(&self) -> &T {
        &self.0
    }

    /// Into inner.
    pub fn into_inner(self) -> T {
        self.0
    }
}

/// Zero extension trait.
pub trait ZeroExt: Zero {
    /// Set to zero.
    fn set_zero(&mut self)
    where
        Self: Sized,
    {
        *self = Self::zero();
    }

    /// Replace with zero.
    fn take_zero(&mut self) -> Self
    where
        Self: Sized + Clone,
    {
        let old = self.clone();
        *self = Self::zero();
        old
    }

    /// Check if not zero.
    fn is_nonzero(&self) -> bool {
        !self.is_zero()
    }
}

impl<T: Zero> ZeroExt for T {}

/// Safe division.
pub fn safe_div<T>(a: T, b: T) -> Result<T>
where
    T: Zero + std::ops::Div<Output = T>,
{
    if b.is_zero() {
        Err(ZeroError::DivisionByZero)
    } else {
        Ok(a / b)
    }
}

/// Div or zero.
pub fn div_or_zero<T>(a: T, b: T) -> T
where
    T: Zero + std::ops::Div<Output = T>,
{
    if b.is_zero() {
        T::zero()
    } else {
        a / b
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zero() {
        assert!(0i32.is_zero());
        assert!(!42i32.is_zero());
        assert_eq!(i32::zero(), 0);
    }

    #[test]
    fn test_one() {
        assert!(1i32.is_one());
        assert!(!42i32.is_one());
        assert_eq!(i32::one(), 1);
    }

    #[test]
    fn test_nonzero() {
        assert!(NonZero::new(0i32).is_err());
        let nz = NonZero::new(42i32).unwrap();
        assert_eq!(*nz.get(), 42);
    }

    #[test]
    fn test_safe_div() {
        assert_eq!(safe_div(10i32, 2).unwrap(), 5);
        assert!(safe_div(10i32, 0).is_err());
    }

    #[test]
    fn test_div_or_zero() {
        assert_eq!(div_or_zero(10i32, 2), 5);
        assert_eq!(div_or_zero(10i32, 0), 0);
    }
}
