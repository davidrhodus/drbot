//! Wrapping arithmetic utilities for drbot.
//!
//! This crate provides:
//! - Wrapping wrapper types
//! - Wrapping operations
//! - Modular arithmetic

use std::ops::{Add, Div, Mul, Rem, Sub};
use thiserror::Error;

/// Wrapping error types.
#[derive(Error, Debug, Clone)]
pub enum WrappingError {
    #[error("Division by zero")]
    DivisionByZero,
}

/// Result type for wrapping operations.
pub type Result<T> = std::result::Result<T, WrappingError>;

/// A wrapping integer wrapper.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Wrapping<T>(pub T);

impl<T> Wrapping<T> {
    /// Create new wrapping value.
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

macro_rules! impl_wrapping_ops {
    ($($t:ty),*) => {
        $(
            impl Add for Wrapping<$t> {
                type Output = Self;

                fn add(self, rhs: Self) -> Self::Output {
                    Self(self.0.wrapping_add(rhs.0))
                }
            }

            impl Sub for Wrapping<$t> {
                type Output = Self;

                fn sub(self, rhs: Self) -> Self::Output {
                    Self(self.0.wrapping_sub(rhs.0))
                }
            }

            impl Mul for Wrapping<$t> {
                type Output = Self;

                fn mul(self, rhs: Self) -> Self::Output {
                    Self(self.0.wrapping_mul(rhs.0))
                }
            }

            impl Div for Wrapping<$t> {
                type Output = Self;

                fn div(self, rhs: Self) -> Self::Output {
                    Self(self.0.wrapping_div(rhs.0))
                }
            }

            impl Rem for Wrapping<$t> {
                type Output = Self;

                fn rem(self, rhs: Self) -> Self::Output {
                    Self(self.0.wrapping_rem(rhs.0))
                }
            }

            impl Wrapping<$t> {
                /// Wrapping addition.
                pub fn wrapping_add(self, rhs: $t) -> Self {
                    Self(self.0.wrapping_add(rhs))
                }

                /// Wrapping subtraction.
                pub fn wrapping_sub(self, rhs: $t) -> Self {
                    Self(self.0.wrapping_sub(rhs))
                }

                /// Wrapping multiplication.
                pub fn wrapping_mul(self, rhs: $t) -> Self {
                    Self(self.0.wrapping_mul(rhs))
                }

                /// Wrapping power.
                pub fn wrapping_pow(self, exp: u32) -> Self {
                    Self(self.0.wrapping_pow(exp))
                }

                /// Wrapping negation.
                pub fn wrapping_neg(self) -> Self {
                    Self(self.0.wrapping_neg())
                }

                /// Wrapping absolute value.
                pub fn wrapping_abs(self) -> Self {
                    Self(self.0.wrapping_abs())
                }

                /// Wrapping left shift.
                pub fn wrapping_shl(self, rhs: u32) -> Self {
                    Self(self.0.wrapping_shl(rhs))
                }

                /// Wrapping right shift.
                pub fn wrapping_shr(self, rhs: u32) -> Self {
                    Self(self.0.wrapping_shr(rhs))
                }
            }
        )*
    };
}

impl_wrapping_ops!(i8, i16, i32, i64, i128, isize);

macro_rules! impl_wrapping_ops_unsigned {
    ($($t:ty),*) => {
        $(
            impl Add for Wrapping<$t> {
                type Output = Self;

                fn add(self, rhs: Self) -> Self::Output {
                    Self(self.0.wrapping_add(rhs.0))
                }
            }

            impl Sub for Wrapping<$t> {
                type Output = Self;

                fn sub(self, rhs: Self) -> Self::Output {
                    Self(self.0.wrapping_sub(rhs.0))
                }
            }

            impl Mul for Wrapping<$t> {
                type Output = Self;

                fn mul(self, rhs: Self) -> Self::Output {
                    Self(self.0.wrapping_mul(rhs.0))
                }
            }

            impl Div for Wrapping<$t> {
                type Output = Self;

                fn div(self, rhs: Self) -> Self::Output {
                    Self(self.0.wrapping_div(rhs.0))
                }
            }

            impl Rem for Wrapping<$t> {
                type Output = Self;

                fn rem(self, rhs: Self) -> Self::Output {
                    Self(self.0.wrapping_rem(rhs.0))
                }
            }

            impl Wrapping<$t> {
                /// Wrapping addition.
                pub fn wrapping_add(self, rhs: $t) -> Self {
                    Self(self.0.wrapping_add(rhs))
                }

                /// Wrapping subtraction.
                pub fn wrapping_sub(self, rhs: $t) -> Self {
                    Self(self.0.wrapping_sub(rhs))
                }

                /// Wrapping multiplication.
                pub fn wrapping_mul(self, rhs: $t) -> Self {
                    Self(self.0.wrapping_mul(rhs))
                }

                /// Wrapping power.
                pub fn wrapping_pow(self, exp: u32) -> Self {
                    Self(self.0.wrapping_pow(exp))
                }

                /// Wrapping negation.
                pub fn wrapping_neg(self) -> Self {
                    Self(self.0.wrapping_neg())
                }

                /// Wrapping left shift.
                pub fn wrapping_shl(self, rhs: u32) -> Self {
                    Self(self.0.wrapping_shl(rhs))
                }

                /// Wrapping right shift.
                pub fn wrapping_shr(self, rhs: u32) -> Self {
                    Self(self.0.wrapping_shr(rhs))
                }
            }
        )*
    };
}

impl_wrapping_ops_unsigned!(u8, u16, u32, u64, u128, usize);

/// Extension trait for wrapping operations.
pub trait WrappingExt: Sized {
    /// Create wrapping wrapper.
    fn wrapping(self) -> Wrapping<Self>;
}

macro_rules! impl_wrapping_ext {
    ($($t:ty),*) => {
        $(
            impl WrappingExt for $t {
                fn wrapping(self) -> Wrapping<Self> {
                    Wrapping(self)
                }
            }
        )*
    };
}

impl_wrapping_ext!(i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize);

/// A circular index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CircularIndex {
    index: usize,
    len: usize,
}

impl CircularIndex {
    /// Create new circular index.
    pub fn new(len: usize) -> Self {
        Self { index: 0, len }
    }

    /// Create with initial index.
    pub fn with_index(index: usize, len: usize) -> Self {
        Self {
            index: index % len,
            len,
        }
    }

    /// Get current index.
    pub fn get(&self) -> usize {
        self.index
    }

    /// Advance by one.
    pub fn advance(&mut self) -> usize {
        let current = self.index;
        self.index = (self.index + 1) % self.len;
        current
    }

    /// Go back by one.
    pub fn back(&mut self) -> usize {
        let current = self.index;
        self.index = self.index.checked_sub(1).unwrap_or(self.len - 1);
        current
    }

    /// Advance by n.
    pub fn advance_by(&mut self, n: usize) -> usize {
        let current = self.index;
        self.index = (self.index + n) % self.len;
        current
    }

    /// Reset to zero.
    pub fn reset(&mut self) {
        self.index = 0;
    }

    /// Set index.
    pub fn set(&mut self, index: usize) {
        self.index = index % self.len;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wrapping_add() {
        let a = Wrapping(250u8);
        let b = Wrapping(10u8);
        assert_eq!((a + b).0, 4); // 260 wraps to 4
    }

    #[test]
    fn test_wrapping_sub() {
        let a = Wrapping(10u8);
        let b = Wrapping(20u8);
        assert_eq!((a - b).0, 246); // Wraps around
    }

    #[test]
    fn test_wrapping_mul() {
        let a = Wrapping(200u8);
        let b = Wrapping(2u8);
        assert_eq!((a * b).0, 144); // 400 wraps to 144
    }

    #[test]
    fn test_wrapping_ext() {
        let x = 255u8.wrapping();
        let result = x.wrapping_add(5);
        assert_eq!(result.0, 4);
    }

    #[test]
    fn test_circular_index() {
        let mut idx = CircularIndex::new(3);
        assert_eq!(idx.get(), 0);

        assert_eq!(idx.advance(), 0);
        assert_eq!(idx.get(), 1);

        assert_eq!(idx.advance(), 1);
        assert_eq!(idx.get(), 2);

        assert_eq!(idx.advance(), 2);
        assert_eq!(idx.get(), 0); // Wrapped

        assert_eq!(idx.back(), 0);
        assert_eq!(idx.get(), 2); // Wrapped back
    }
}
