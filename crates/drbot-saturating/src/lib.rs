//! Saturating arithmetic utilities for drbot.
//!
//! This crate provides:
//! - Saturating wrapper types
//! - Saturating operations
//! - Clamped arithmetic

use std::ops::{Add, Div, Mul, Sub};
use thiserror::Error;

/// Saturating error types.
#[derive(Error, Debug, Clone)]
pub enum SaturatingError {
    #[error("Would overflow")]
    Overflow,

    #[error("Would underflow")]
    Underflow,
}

/// Result type for saturating operations.
pub type Result<T> = std::result::Result<T, SaturatingError>;

/// A saturating integer wrapper.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Saturating<T>(pub T);

impl<T> Saturating<T> {
    /// Create new saturating value.
    pub const fn new(value: T) -> Self {
        Self(value)
    }

    /// Get inner value.
    pub fn into_inner(self) -> T {
        self.0
    }

    /// Get reference.
    pub fn get(&self) -> &T {
        &self.0
    }

    /// Get mutable reference.
    pub fn get_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

macro_rules! impl_saturating_ops {
    ($($t:ty),*) => {
        $(
            impl Add for Saturating<$t> {
                type Output = Self;

                fn add(self, rhs: Self) -> Self::Output {
                    Self(self.0.saturating_add(rhs.0))
                }
            }

            impl Sub for Saturating<$t> {
                type Output = Self;

                fn sub(self, rhs: Self) -> Self::Output {
                    Self(self.0.saturating_sub(rhs.0))
                }
            }

            impl Mul for Saturating<$t> {
                type Output = Self;

                fn mul(self, rhs: Self) -> Self::Output {
                    Self(self.0.saturating_mul(rhs.0))
                }
            }

            impl Div for Saturating<$t> {
                type Output = Self;

                fn div(self, rhs: Self) -> Self::Output {
                    Self(self.0.saturating_div(rhs.0))
                }
            }

            impl Saturating<$t> {
                /// Saturating addition.
                pub fn saturating_add(self, rhs: $t) -> Self {
                    Self(self.0.saturating_add(rhs))
                }

                /// Saturating subtraction.
                pub fn saturating_sub(self, rhs: $t) -> Self {
                    Self(self.0.saturating_sub(rhs))
                }

                /// Saturating multiplication.
                pub fn saturating_mul(self, rhs: $t) -> Self {
                    Self(self.0.saturating_mul(rhs))
                }

                /// Saturating power.
                pub fn saturating_pow(self, exp: u32) -> Self {
                    Self(self.0.saturating_pow(exp))
                }

                /// Saturating negation.
                pub fn saturating_neg(self) -> Self {
                    Self(self.0.saturating_neg())
                }

                /// Saturating absolute value.
                pub fn saturating_abs(self) -> Self {
                    Self(self.0.saturating_abs())
                }
            }
        )*
    };
}

impl_saturating_ops!(i8, i16, i32, i64, i128, isize);

macro_rules! impl_saturating_ops_unsigned {
    ($($t:ty),*) => {
        $(
            impl Add for Saturating<$t> {
                type Output = Self;

                fn add(self, rhs: Self) -> Self::Output {
                    Self(self.0.saturating_add(rhs.0))
                }
            }

            impl Sub for Saturating<$t> {
                type Output = Self;

                fn sub(self, rhs: Self) -> Self::Output {
                    Self(self.0.saturating_sub(rhs.0))
                }
            }

            impl Mul for Saturating<$t> {
                type Output = Self;

                fn mul(self, rhs: Self) -> Self::Output {
                    Self(self.0.saturating_mul(rhs.0))
                }
            }

            impl Div for Saturating<$t> {
                type Output = Self;

                fn div(self, rhs: Self) -> Self::Output {
                    Self(self.0.saturating_div(rhs.0))
                }
            }

            impl Saturating<$t> {
                /// Saturating addition.
                pub fn saturating_add(self, rhs: $t) -> Self {
                    Self(self.0.saturating_add(rhs))
                }

                /// Saturating subtraction.
                pub fn saturating_sub(self, rhs: $t) -> Self {
                    Self(self.0.saturating_sub(rhs))
                }

                /// Saturating multiplication.
                pub fn saturating_mul(self, rhs: $t) -> Self {
                    Self(self.0.saturating_mul(rhs))
                }

                /// Saturating power.
                pub fn saturating_pow(self, exp: u32) -> Self {
                    Self(self.0.saturating_pow(exp))
                }
            }
        )*
    };
}

impl_saturating_ops_unsigned!(u8, u16, u32, u64, u128, usize);

/// Extension trait for saturating operations.
pub trait SaturatingExt: Sized {
    /// Create saturating wrapper.
    fn saturating(self) -> Saturating<Self>;
}

macro_rules! impl_saturating_ext {
    ($($t:ty),*) => {
        $(
            impl SaturatingExt for $t {
                fn saturating(self) -> Saturating<Self> {
                    Saturating(self)
                }
            }
        )*
    };
}

impl_saturating_ext!(i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize);

/// Saturating counter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SaturatingCounter {
    value: usize,
    max: usize,
}

impl SaturatingCounter {
    /// Create counter with maximum.
    pub fn new(max: usize) -> Self {
        Self { value: 0, max }
    }

    /// Create counter with initial value.
    pub fn with_value(value: usize, max: usize) -> Self {
        Self {
            value: value.min(max),
            max,
        }
    }

    /// Increment counter.
    pub fn increment(&mut self) -> usize {
        if self.value < self.max {
            self.value += 1;
        }
        self.value
    }

    /// Decrement counter.
    pub fn decrement(&mut self) -> usize {
        self.value = self.value.saturating_sub(1);
        self.value
    }

    /// Get current value.
    pub fn get(&self) -> usize {
        self.value
    }

    /// Check if at maximum.
    pub fn is_saturated(&self) -> bool {
        self.value >= self.max
    }

    /// Check if at zero.
    pub fn is_zero(&self) -> bool {
        self.value == 0
    }

    /// Reset to zero.
    pub fn reset(&mut self) {
        self.value = 0;
    }

    /// Set to maximum.
    pub fn saturate(&mut self) {
        self.value = self.max;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_saturating_add() {
        let a = Saturating(250u8);
        let b = Saturating(10u8);
        assert_eq!((a + b).0, 255);
    }

    #[test]
    fn test_saturating_sub() {
        let a = Saturating(10u8);
        let b = Saturating(20u8);
        assert_eq!((a - b).0, 0);
    }

    #[test]
    fn test_saturating_mul() {
        let a = Saturating(200u8);
        let b = Saturating(2u8);
        assert_eq!((a * b).0, 255);
    }

    #[test]
    fn test_saturating_counter() {
        let mut counter = SaturatingCounter::new(3);
        assert_eq!(counter.get(), 0);

        counter.increment();
        counter.increment();
        counter.increment();
        assert_eq!(counter.get(), 3);
        assert!(counter.is_saturated());

        counter.increment();
        assert_eq!(counter.get(), 3);

        counter.decrement();
        assert_eq!(counter.get(), 2);
    }

    #[test]
    fn test_saturating_ext() {
        let x = 100u8.saturating();
        let result = x.saturating_add(200);
        assert_eq!(result.0, 255);
    }
}
