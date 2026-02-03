//! Checked arithmetic utilities for drbot.
//!
//! This crate provides:
//! - Checked wrapper types
//! - Checked operations that return Option
//! - Overflow detection

use thiserror::Error;

/// Checked error types.
#[derive(Error, Debug, Clone)]
pub enum CheckedError {
    #[error("Arithmetic overflow")]
    Overflow,

    #[error("Arithmetic underflow")]
    Underflow,

    #[error("Division by zero")]
    DivisionByZero,
}

/// Result type for checked operations.
pub type Result<T> = std::result::Result<T, CheckedError>;

/// A checked integer wrapper.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Checked<T>(pub T);

impl<T> Checked<T> {
    /// Create new checked value.
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

macro_rules! impl_checked_ops {
    ($($t:ty),*) => {
        $(
            impl Checked<$t> {
                /// Checked addition.
                pub fn checked_add(self, rhs: $t) -> Option<Self> {
                    self.0.checked_add(rhs).map(Self)
                }

                /// Checked subtraction.
                pub fn checked_sub(self, rhs: $t) -> Option<Self> {
                    self.0.checked_sub(rhs).map(Self)
                }

                /// Checked multiplication.
                pub fn checked_mul(self, rhs: $t) -> Option<Self> {
                    self.0.checked_mul(rhs).map(Self)
                }

                /// Checked division.
                pub fn checked_div(self, rhs: $t) -> Option<Self> {
                    self.0.checked_div(rhs).map(Self)
                }

                /// Checked remainder.
                pub fn checked_rem(self, rhs: $t) -> Option<Self> {
                    self.0.checked_rem(rhs).map(Self)
                }

                /// Checked power.
                pub fn checked_pow(self, exp: u32) -> Option<Self> {
                    self.0.checked_pow(exp).map(Self)
                }

                /// Checked negation.
                pub fn checked_neg(self) -> Option<Self> {
                    self.0.checked_neg().map(Self)
                }

                /// Checked left shift.
                pub fn checked_shl(self, rhs: u32) -> Option<Self> {
                    self.0.checked_shl(rhs).map(Self)
                }

                /// Checked right shift.
                pub fn checked_shr(self, rhs: u32) -> Option<Self> {
                    self.0.checked_shr(rhs).map(Self)
                }

                /// Try add, returning Result.
                pub fn try_add(self, rhs: $t) -> Result<Self> {
                    self.checked_add(rhs).ok_or(CheckedError::Overflow)
                }

                /// Try subtract, returning Result.
                pub fn try_sub(self, rhs: $t) -> Result<Self> {
                    self.checked_sub(rhs).ok_or(CheckedError::Underflow)
                }

                /// Try multiply, returning Result.
                pub fn try_mul(self, rhs: $t) -> Result<Self> {
                    self.checked_mul(rhs).ok_or(CheckedError::Overflow)
                }

                /// Try divide, returning Result.
                pub fn try_div(self, rhs: $t) -> Result<Self> {
                    if rhs == 0 {
                        Err(CheckedError::DivisionByZero)
                    } else {
                        self.checked_div(rhs).ok_or(CheckedError::Overflow)
                    }
                }
            }
        )*
    };
}

impl_checked_ops!(i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize);

/// Extension trait for checked operations.
pub trait CheckedExt: Sized {
    /// Create checked wrapper.
    fn checked(self) -> Checked<Self>;
}

macro_rules! impl_checked_ext {
    ($($t:ty),*) => {
        $(
            impl CheckedExt for $t {
                fn checked(self) -> Checked<Self> {
                    Checked(self)
                }
            }
        )*
    };
}

impl_checked_ext!(i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize);

/// Perform checked addition returning Result.
pub fn add<T>(a: T, b: T) -> Result<T>
where
    Checked<T>: Copy,
    T: Copy,
    Checked<T>: CheckedAdd<T>,
{
    Checked(a).try_add(b).map(|c| c.0)
}

/// Trait for checked addition.
pub trait CheckedAdd<T> {
    /// Try to add.
    fn try_add(self, rhs: T) -> Result<Self>
    where
        Self: Sized;
}

macro_rules! impl_checked_add {
    ($($t:ty),*) => {
        $(
            impl CheckedAdd<$t> for Checked<$t> {
                fn try_add(self, rhs: $t) -> Result<Self> {
                    self.checked_add(rhs).ok_or(CheckedError::Overflow)
                }
            }
        )*
    };
}

impl_checked_add!(i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize);

/// Overflow-detecting counter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OverflowCounter {
    value: usize,
    overflowed: bool,
}

impl OverflowCounter {
    /// Create new counter.
    pub fn new() -> Self {
        Self {
            value: 0,
            overflowed: false,
        }
    }

    /// Create with initial value.
    pub fn with_value(value: usize) -> Self {
        Self {
            value,
            overflowed: false,
        }
    }

    /// Increment, detecting overflow.
    pub fn increment(&mut self) -> Option<usize> {
        match self.value.checked_add(1) {
            Some(new_value) => {
                self.value = new_value;
                Some(new_value)
            }
            None => {
                self.overflowed = true;
                None
            }
        }
    }

    /// Decrement, detecting underflow.
    pub fn decrement(&mut self) -> Option<usize> {
        match self.value.checked_sub(1) {
            Some(new_value) => {
                self.value = new_value;
                Some(new_value)
            }
            None => None,
        }
    }

    /// Get current value.
    pub fn get(&self) -> usize {
        self.value
    }

    /// Check if overflow occurred.
    pub fn has_overflowed(&self) -> bool {
        self.overflowed
    }

    /// Reset counter.
    pub fn reset(&mut self) {
        self.value = 0;
        self.overflowed = false;
    }
}

impl Default for OverflowCounter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_checked_add() {
        let a = Checked(250u8);
        assert!(a.checked_add(5).is_some());
        assert!(a.checked_add(10).is_none());
    }

    #[test]
    fn test_checked_sub() {
        let a = Checked(10u8);
        assert!(a.checked_sub(5).is_some());
        assert!(a.checked_sub(20).is_none());
    }

    #[test]
    fn test_checked_div() {
        let a = Checked(10i32);
        assert_eq!(a.checked_div(2), Some(Checked(5)));
        assert_eq!(a.checked_div(0), None);
    }

    #[test]
    fn test_try_operations() {
        let a = Checked(250u8);
        assert!(a.try_add(5).is_ok());
        assert!(a.try_add(10).is_err());

        let b = Checked(10u8);
        assert!(b.try_div(0).is_err());
    }

    #[test]
    fn test_overflow_counter() {
        let mut counter = OverflowCounter::new();
        assert_eq!(counter.increment(), Some(1));
        assert_eq!(counter.increment(), Some(2));
        assert!(!counter.has_overflowed());

        let mut max_counter = OverflowCounter::with_value(usize::MAX);
        assert_eq!(max_counter.increment(), None);
        assert!(max_counter.has_overflowed());
    }
}

// ============================================================================
// Kani Formal Verification Proofs
// ============================================================================

#[cfg(kani)]
mod kani_proofs {
    use super::*;

    // ========================================================================
    // Checked Wrapper Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_checked_new() {
        let value: i8 = kani::any();
        let checked = Checked::new(value);

        kani::assert(*checked.get() == value, "get returns value");
    }

    #[kani::proof]
    fn proof_checked_into_inner() {
        let value: i8 = kani::any();
        let checked = Checked::new(value);

        kani::assert(checked.into_inner() == value, "into_inner returns value");
    }

    #[kani::proof]
    fn proof_checked_get_mut() {
        let value: i8 = kani::any();
        let mut checked = Checked::new(value);

        *checked.get_mut() = 42;
        kani::assert(*checked.get() == 42, "get_mut allows modification");
    }

    #[kani::proof]
    fn proof_checked_default() {
        let checked: Checked<i32> = Checked::default();

        kani::assert(*checked.get() == 0, "default is zero");
    }

    // ========================================================================
    // Checked Add Proofs (u8 for bounded verification)
    // ========================================================================

    #[kani::proof]
    fn proof_checked_add_success() {
        let a: u8 = kani::any();
        let b: u8 = kani::any();
        kani::assume((a as u16) + (b as u16) <= 255);

        let checked = Checked(a);
        let result = checked.checked_add(b);

        kani::assert(result.is_some(), "no overflow");
        kani::assert(result.unwrap().0 == a + b, "correct sum");
    }

    #[kani::proof]
    fn proof_checked_add_overflow() {
        let a: u8 = kani::any();
        let b: u8 = kani::any();
        kani::assume((a as u16) + (b as u16) > 255);

        let checked = Checked(a);
        let result = checked.checked_add(b);

        kani::assert(result.is_none(), "overflow detected");
    }

    // ========================================================================
    // Checked Sub Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_checked_sub_success() {
        let a: u8 = kani::any();
        let b: u8 = kani::any();
        kani::assume(a >= b);

        let checked = Checked(a);
        let result = checked.checked_sub(b);

        kani::assert(result.is_some(), "no underflow");
        kani::assert(result.unwrap().0 == a - b, "correct difference");
    }

    #[kani::proof]
    fn proof_checked_sub_underflow() {
        let a: u8 = kani::any();
        let b: u8 = kani::any();
        kani::assume(a < b);

        let checked = Checked(a);
        let result = checked.checked_sub(b);

        kani::assert(result.is_none(), "underflow detected");
    }

    // ========================================================================
    // Checked Mul Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_checked_mul_success() {
        let a: u8 = kani::any();
        let b: u8 = kani::any();
        kani::assume((a as u16) * (b as u16) <= 255);

        let checked = Checked(a);
        let result = checked.checked_mul(b);

        kani::assert(result.is_some(), "no overflow");
        kani::assert(result.unwrap().0 == a * b, "correct product");
    }

    #[kani::proof]
    fn proof_checked_mul_overflow() {
        let a: u8 = kani::any();
        let b: u8 = kani::any();
        kani::assume((a as u16) * (b as u16) > 255);

        let checked = Checked(a);
        let result = checked.checked_mul(b);

        kani::assert(result.is_none(), "overflow detected");
    }

    // ========================================================================
    // Checked Div Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_checked_div_success() {
        let a: u8 = kani::any();
        let b: u8 = kani::any();
        kani::assume(b != 0);

        let checked = Checked(a);
        let result = checked.checked_div(b);

        kani::assert(result.is_some(), "division succeeds");
        kani::assert(result.unwrap().0 == a / b, "correct quotient");
    }

    #[kani::proof]
    fn proof_checked_div_by_zero() {
        let a: u8 = kani::any();

        let checked = Checked(a);
        let result = checked.checked_div(0);

        kani::assert(result.is_none(), "div by zero returns None");
    }

    // ========================================================================
    // Checked Rem Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_checked_rem_success() {
        let a: u8 = kani::any();
        let b: u8 = kani::any();
        kani::assume(b != 0);

        let checked = Checked(a);
        let result = checked.checked_rem(b);

        kani::assert(result.is_some(), "rem succeeds");
        kani::assert(result.unwrap().0 == a % b, "correct remainder");
    }

    #[kani::proof]
    fn proof_checked_rem_by_zero() {
        let a: u8 = kani::any();

        let checked = Checked(a);
        let result = checked.checked_rem(0);

        kani::assert(result.is_none(), "rem by zero returns None");
    }

    // ========================================================================
    // Checked Pow Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_checked_pow_success() {
        let base: u8 = kani::any();
        kani::assume(base <= 3);

        let checked = Checked(base);
        let result = checked.checked_pow(2);

        if base <= 15 {
            kani::assert(result.is_some(), "small powers succeed");
        }
    }

    #[kani::proof]
    fn proof_checked_pow_zero() {
        let base: u8 = kani::any();
        kani::assume(base > 0);

        let checked = Checked(base);
        let result = checked.checked_pow(0);

        kani::assert(result == Some(Checked(1)), "x^0 = 1");
    }

    // ========================================================================
    // Try Operations Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_try_add_ok() {
        let a: u8 = kani::any();
        let b: u8 = kani::any();
        kani::assume((a as u16) + (b as u16) <= 255);

        let checked = Checked(a);
        let result = checked.try_add(b);

        kani::assert(result.is_ok(), "try_add succeeds");
    }

    #[kani::proof]
    fn proof_try_add_err() {
        let a: u8 = kani::any();
        let b: u8 = kani::any();
        kani::assume((a as u16) + (b as u16) > 255);

        let checked = Checked(a);
        let result = checked.try_add(b);

        kani::assert(result.is_err(), "try_add fails on overflow");
    }

    #[kani::proof]
    fn proof_try_sub_ok() {
        let a: u8 = kani::any();
        let b: u8 = kani::any();
        kani::assume(a >= b);

        let checked = Checked(a);
        let result = checked.try_sub(b);

        kani::assert(result.is_ok(), "try_sub succeeds");
    }

    #[kani::proof]
    fn proof_try_sub_err() {
        let a: u8 = kani::any();
        let b: u8 = kani::any();
        kani::assume(a < b);

        let checked = Checked(a);
        let result = checked.try_sub(b);

        kani::assert(result.is_err(), "try_sub fails on underflow");
    }

    #[kani::proof]
    fn proof_try_div_ok() {
        let a: u8 = kani::any();
        let b: u8 = kani::any();
        kani::assume(b != 0);

        let checked = Checked(a);
        let result = checked.try_div(b);

        kani::assert(result.is_ok(), "try_div succeeds");
    }

    #[kani::proof]
    fn proof_try_div_by_zero() {
        let a: u8 = kani::any();

        let checked = Checked(a);
        let result = checked.try_div(0);

        kani::assert(result.is_err(), "try_div fails on zero");
    }

    // ========================================================================
    // CheckedExt Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_checked_ext() {
        let value: u8 = kani::any();
        let checked = value.checked();

        kani::assert(*checked.get() == value, "checked() wraps value");
    }

    // ========================================================================
    // OverflowCounter Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_overflow_counter_new() {
        let counter = OverflowCounter::new();

        kani::assert(counter.get() == 0, "new counter is 0");
        kani::assert(!counter.has_overflowed(), "no overflow initially");
    }

    #[kani::proof]
    fn proof_overflow_counter_default() {
        let counter = OverflowCounter::default();

        kani::assert(counter.get() == 0, "default counter is 0");
    }

    #[kani::proof]
    fn proof_overflow_counter_with_value() {
        let value: u8 = kani::any();
        let counter = OverflowCounter::with_value(value as usize);

        kani::assert(counter.get() == value as usize, "with_value sets value");
        kani::assert(!counter.has_overflowed(), "no overflow");
    }

    #[kani::proof]
    fn proof_overflow_counter_increment() {
        let value: u8 = kani::any();
        kani::assume(value < 255);

        let mut counter = OverflowCounter::with_value(value as usize);
        let result = counter.increment();

        kani::assert(
            result == Some((value as usize) + 1),
            "increment returns new value",
        );
        kani::assert(counter.get() == (value as usize) + 1, "value incremented");
    }

    #[kani::proof]
    fn proof_overflow_counter_increment_max() {
        let mut counter = OverflowCounter::with_value(usize::MAX);
        let result = counter.increment();

        kani::assert(result.is_none(), "increment at MAX returns None");
        kani::assert(counter.has_overflowed(), "overflow flag set");
    }

    #[kani::proof]
    fn proof_overflow_counter_decrement() {
        let value: u8 = kani::any();
        kani::assume(value > 0);

        let mut counter = OverflowCounter::with_value(value as usize);
        let result = counter.decrement();

        kani::assert(
            result == Some((value as usize) - 1),
            "decrement returns new value",
        );
    }

    #[kani::proof]
    fn proof_overflow_counter_decrement_zero() {
        let mut counter = OverflowCounter::new();
        let result = counter.decrement();

        kani::assert(result.is_none(), "decrement at 0 returns None");
    }

    #[kani::proof]
    fn proof_overflow_counter_reset() {
        let mut counter = OverflowCounter::with_value(100);
        counter.increment();
        counter.reset();

        kani::assert(counter.get() == 0, "reset sets value to 0");
        kani::assert(!counter.has_overflowed(), "reset clears overflow flag");
    }
}
