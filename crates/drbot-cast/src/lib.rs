//! Type casting utilities for drbot.
//!
//! This crate provides:
//! - Safe numeric casts
//! - Checked/unchecked casting
//! - Cast result types

use thiserror::Error;

/// Cast error types.
#[derive(Error, Debug, Clone, PartialEq)]
pub enum CastError {
    #[error("Value {0} overflows target type")]
    Overflow(String),

    #[error("Value {0} underflows target type")]
    Underflow(String),

    #[error("Cannot cast negative value to unsigned type")]
    NegativeToUnsigned,

    #[error("Precision loss casting {0}")]
    PrecisionLoss(String),
}

/// Result type for cast operations.
pub type Result<T> = std::result::Result<T, CastError>;

/// Safe cast trait.
pub trait SafeCast<T>: Sized {
    /// Attempt to cast to target type.
    fn safe_cast(self) -> Result<T>;
}

/// Checked cast trait (returns Option).
pub trait CheckedCast<T>: Sized {
    /// Cast with bounds checking.
    fn checked_cast(self) -> Option<T>;
}

/// Unchecked cast trait.
pub trait UncheckedCast<T>: Sized {
    /// Cast without bounds checking (may truncate).
    fn unchecked_cast(self) -> T;
}

// Implement for i64 -> smaller types
impl SafeCast<i32> for i64 {
    fn safe_cast(self) -> Result<i32> {
        if self > i32::MAX as i64 {
            Err(CastError::Overflow(self.to_string()))
        } else if self < i32::MIN as i64 {
            Err(CastError::Underflow(self.to_string()))
        } else {
            Ok(self as i32)
        }
    }
}

impl SafeCast<i16> for i64 {
    fn safe_cast(self) -> Result<i16> {
        if self > i16::MAX as i64 {
            Err(CastError::Overflow(self.to_string()))
        } else if self < i16::MIN as i64 {
            Err(CastError::Underflow(self.to_string()))
        } else {
            Ok(self as i16)
        }
    }
}

impl SafeCast<i8> for i64 {
    fn safe_cast(self) -> Result<i8> {
        if self > i8::MAX as i64 {
            Err(CastError::Overflow(self.to_string()))
        } else if self < i8::MIN as i64 {
            Err(CastError::Underflow(self.to_string()))
        } else {
            Ok(self as i8)
        }
    }
}

impl SafeCast<u64> for i64 {
    fn safe_cast(self) -> Result<u64> {
        if self < 0 {
            Err(CastError::NegativeToUnsigned)
        } else {
            Ok(self as u64)
        }
    }
}

impl SafeCast<u32> for i64 {
    fn safe_cast(self) -> Result<u32> {
        if self < 0 {
            Err(CastError::NegativeToUnsigned)
        } else if self > u32::MAX as i64 {
            Err(CastError::Overflow(self.to_string()))
        } else {
            Ok(self as u32)
        }
    }
}

// Implement for u64 -> smaller types
impl SafeCast<i64> for u64 {
    fn safe_cast(self) -> Result<i64> {
        if self > i64::MAX as u64 {
            Err(CastError::Overflow(self.to_string()))
        } else {
            Ok(self as i64)
        }
    }
}

impl SafeCast<u32> for u64 {
    fn safe_cast(self) -> Result<u32> {
        if self > u32::MAX as u64 {
            Err(CastError::Overflow(self.to_string()))
        } else {
            Ok(self as u32)
        }
    }
}

impl SafeCast<usize> for u64 {
    fn safe_cast(self) -> Result<usize> {
        if self > usize::MAX as u64 {
            Err(CastError::Overflow(self.to_string()))
        } else {
            Ok(self as usize)
        }
    }
}

// Implement for f64 -> f32
impl SafeCast<f32> for f64 {
    fn safe_cast(self) -> Result<f32> {
        if self.is_infinite() || self.is_nan() {
            Ok(self as f32)
        } else if self.abs() > f32::MAX as f64 {
            Err(CastError::Overflow(self.to_string()))
        } else {
            Ok(self as f32)
        }
    }
}

// Implement CheckedCast
impl<T, U> CheckedCast<U> for T
where
    T: SafeCast<U>,
{
    fn checked_cast(self) -> Option<U> {
        self.safe_cast().ok()
    }
}

// Implement UncheckedCast for primitives
macro_rules! impl_unchecked_cast {
    ($from:ty => $to:ty) => {
        impl UncheckedCast<$to> for $from {
            fn unchecked_cast(self) -> $to {
                self as $to
            }
        }
    };
}

impl_unchecked_cast!(i64 => i32);
impl_unchecked_cast!(i64 => i16);
impl_unchecked_cast!(i64 => i8);
impl_unchecked_cast!(i64 => u64);
impl_unchecked_cast!(i64 => u32);
impl_unchecked_cast!(u64 => i64);
impl_unchecked_cast!(u64 => u32);
impl_unchecked_cast!(u64 => usize);
impl_unchecked_cast!(f64 => f32);
impl_unchecked_cast!(i32 => i16);
impl_unchecked_cast!(i32 => i8);
impl_unchecked_cast!(u32 => u16);
impl_unchecked_cast!(u32 => u8);

/// Cast helper functions.
pub fn cast<T, U>(value: T) -> Result<U>
where
    T: SafeCast<U>,
{
    value.safe_cast()
}

/// Checked cast helper.
pub fn checked<T, U>(value: T) -> Option<U>
where
    T: CheckedCast<U>,
{
    value.checked_cast()
}

/// Unchecked cast helper.
pub fn unchecked<T, U>(value: T) -> U
where
    T: UncheckedCast<U>,
{
    value.unchecked_cast()
}

/// Cast with saturation (clamp to bounds).
pub fn saturating<T: Saturate<U>, U>(value: T) -> U {
    value.saturate()
}

/// Saturating cast trait.
pub trait Saturate<T> {
    /// Cast with saturation to target bounds.
    fn saturate(self) -> T;
}

impl Saturate<i32> for i64 {
    fn saturate(self) -> i32 {
        if self > i32::MAX as i64 {
            i32::MAX
        } else if self < i32::MIN as i64 {
            i32::MIN
        } else {
            self as i32
        }
    }
}

impl Saturate<u32> for u64 {
    fn saturate(self) -> u32 {
        if self > u32::MAX as u64 {
            u32::MAX
        } else {
            self as u32
        }
    }
}

impl Saturate<u32> for i64 {
    fn saturate(self) -> u32 {
        if self < 0 {
            0
        } else if self > u32::MAX as i64 {
            u32::MAX
        } else {
            self as u32
        }
    }
}

impl Saturate<u8> for i32 {
    fn saturate(self) -> u8 {
        if self < 0 {
            0
        } else if self > u8::MAX as i32 {
            u8::MAX
        } else {
            self as u8
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_safe_cast() {
        let result: Result<i32> = 100i64.safe_cast();
        assert_eq!(result.unwrap(), 100);

        let overflow: Result<i8> = 1000i64.safe_cast();
        assert!(matches!(overflow, Err(CastError::Overflow(_))));

        let negative: Result<u32> = (-5i64).safe_cast();
        assert!(matches!(negative, Err(CastError::NegativeToUnsigned)));
    }

    #[test]
    fn test_checked_cast() {
        let result: Option<i32> = 100i64.checked_cast();
        assert_eq!(result, Some(100));

        let overflow: Option<i8> = 1000i64.checked_cast();
        assert_eq!(overflow, None);
    }

    #[test]
    fn test_unchecked_cast() {
        let result: i32 = 100i64.unchecked_cast();
        assert_eq!(result, 100);

        // Truncation happens
        let truncated: i8 = 1000i64.unchecked_cast();
        assert_ne!(truncated as i64, 1000i64);
    }

    #[test]
    fn test_saturating() {
        let result: i32 = saturating(i64::MAX);
        assert_eq!(result, i32::MAX);

        let result: i32 = saturating(i64::MIN);
        assert_eq!(result, i32::MIN);

        let result: u32 = saturating(-10i64);
        assert_eq!(result, 0);
    }

    #[test]
    fn test_helpers() {
        let result: Result<i32> = cast(100i64);
        assert_eq!(result.unwrap(), 100);

        let result: Option<i32> = checked(100i64);
        assert_eq!(result, Some(100));

        let result: i32 = unchecked(100i64);
        assert_eq!(result, 100);
    }
}

// ============================================================================
// Kani Formal Verification Proofs
// ============================================================================

#[cfg(kani)]
mod kani_proofs {
    use super::*;

    // ========================================================================
    // SafeCast i64 -> i32 Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_safe_cast_i64_to_i32_in_range() {
        let value: i32 = kani::any();
        let as_i64 = value as i64;

        let result: Result<i32> = as_i64.safe_cast();
        kani::assert(result.is_ok(), "in-range cast succeeds");
        kani::assert(result.unwrap() == value, "cast preserves value");
    }

    #[kani::proof]
    fn proof_safe_cast_i64_to_i32_overflow() {
        let value: i64 = (i32::MAX as i64) + 1;

        let result: Result<i32> = value.safe_cast();
        kani::assert(result.is_err(), "overflow detected");
    }

    #[kani::proof]
    fn proof_safe_cast_i64_to_i32_underflow() {
        let value: i64 = (i32::MIN as i64) - 1;

        let result: Result<i32> = value.safe_cast();
        kani::assert(result.is_err(), "underflow detected");
    }

    // ========================================================================
    // SafeCast i64 -> i16 Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_safe_cast_i64_to_i16_in_range() {
        let value: i16 = kani::any();
        let as_i64 = value as i64;

        let result: Result<i16> = as_i64.safe_cast();
        kani::assert(result.is_ok(), "in-range cast succeeds");
        kani::assert(result.unwrap() == value, "cast preserves value");
    }

    #[kani::proof]
    fn proof_safe_cast_i64_to_i16_overflow() {
        let value: i64 = (i16::MAX as i64) + 1;

        let result: Result<i16> = value.safe_cast();
        kani::assert(result.is_err(), "overflow detected");
    }

    // ========================================================================
    // SafeCast i64 -> i8 Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_safe_cast_i64_to_i8_in_range() {
        let value: i8 = kani::any();
        let as_i64 = value as i64;

        let result: Result<i8> = as_i64.safe_cast();
        kani::assert(result.is_ok(), "in-range cast succeeds");
        kani::assert(result.unwrap() == value, "cast preserves value");
    }

    #[kani::proof]
    fn proof_safe_cast_i64_to_i8_overflow() {
        let value: i64 = (i8::MAX as i64) + 1;

        let result: Result<i8> = value.safe_cast();
        kani::assert(result.is_err(), "overflow detected");
    }

    #[kani::proof]
    fn proof_safe_cast_i64_to_i8_underflow() {
        let value: i64 = (i8::MIN as i64) - 1;

        let result: Result<i8> = value.safe_cast();
        kani::assert(result.is_err(), "underflow detected");
    }

    // ========================================================================
    // SafeCast i64 -> u64 Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_safe_cast_i64_to_u64_positive() {
        let value: i64 = kani::any();
        kani::assume(value >= 0);

        let result: Result<u64> = value.safe_cast();
        kani::assert(result.is_ok(), "non-negative cast succeeds");
        kani::assert(result.unwrap() == value as u64, "cast preserves value");
    }

    #[kani::proof]
    fn proof_safe_cast_i64_to_u64_negative() {
        let value: i64 = kani::any();
        kani::assume(value < 0);

        let result: Result<u64> = value.safe_cast();
        kani::assert(result.is_err(), "negative to unsigned fails");
    }

    // ========================================================================
    // SafeCast i64 -> u32 Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_safe_cast_i64_to_u32_in_range() {
        let value: u32 = kani::any();
        let as_i64 = value as i64;

        let result: Result<u32> = as_i64.safe_cast();
        kani::assert(result.is_ok(), "in-range cast succeeds");
        kani::assert(result.unwrap() == value, "cast preserves value");
    }

    #[kani::proof]
    fn proof_safe_cast_i64_to_u32_negative() {
        let result: Result<u32> = (-1i64).safe_cast();
        kani::assert(result.is_err(), "negative to u32 fails");
    }

    #[kani::proof]
    fn proof_safe_cast_i64_to_u32_overflow() {
        let value: i64 = (u32::MAX as i64) + 1;

        let result: Result<u32> = value.safe_cast();
        kani::assert(result.is_err(), "overflow detected");
    }

    // ========================================================================
    // SafeCast u64 -> i64 Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_safe_cast_u64_to_i64_in_range() {
        let value: u64 = kani::any();
        kani::assume(value <= i64::MAX as u64);

        let result: Result<i64> = value.safe_cast();
        kani::assert(result.is_ok(), "in-range cast succeeds");
        kani::assert(result.unwrap() == value as i64, "cast preserves value");
    }

    #[kani::proof]
    fn proof_safe_cast_u64_to_i64_overflow() {
        let value: u64 = (i64::MAX as u64) + 1;

        let result: Result<i64> = value.safe_cast();
        kani::assert(result.is_err(), "overflow detected");
    }

    // ========================================================================
    // SafeCast u64 -> u32 Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_safe_cast_u64_to_u32_in_range() {
        let value: u32 = kani::any();
        let as_u64 = value as u64;

        let result: Result<u32> = as_u64.safe_cast();
        kani::assert(result.is_ok(), "in-range cast succeeds");
        kani::assert(result.unwrap() == value, "cast preserves value");
    }

    #[kani::proof]
    fn proof_safe_cast_u64_to_u32_overflow() {
        let value: u64 = (u32::MAX as u64) + 1;

        let result: Result<u32> = value.safe_cast();
        kani::assert(result.is_err(), "overflow detected");
    }

    // ========================================================================
    // CheckedCast Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_checked_cast_success() {
        let value: i32 = kani::any();
        let as_i64 = value as i64;

        let result: Option<i32> = as_i64.checked_cast();
        kani::assert(result.is_some(), "valid cast returns Some");
        kani::assert(result.unwrap() == value, "checked cast preserves value");
    }

    #[kani::proof]
    fn proof_checked_cast_failure() {
        let value: i64 = (i32::MAX as i64) + 1;

        let result: Option<i32> = value.checked_cast();
        kani::assert(result.is_none(), "invalid cast returns None");
    }

    // ========================================================================
    // UncheckedCast Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_unchecked_cast_in_range() {
        let value: i32 = kani::any();
        let as_i64 = value as i64;

        let result: i32 = as_i64.unchecked_cast();
        kani::assert(result == value, "unchecked cast preserves in-range value");
    }

    #[kani::proof]
    fn proof_unchecked_cast_truncates() {
        let value: i64 = 0x1_0000_0001i64; // Larger than i32::MAX

        let result: i32 = value.unchecked_cast();
        // Just verify it doesn't panic - truncation is expected
        kani::assert(result == 1, "unchecked cast truncates");
    }

    // ========================================================================
    // Saturate Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_saturate_i64_to_i32_in_range() {
        let value: i32 = kani::any();
        let as_i64 = value as i64;

        let result: i32 = as_i64.saturate();
        kani::assert(result == value, "saturate preserves in-range value");
    }

    #[kani::proof]
    fn proof_saturate_i64_to_i32_overflow() {
        let value: i64 = (i32::MAX as i64) + 1;

        let result: i32 = value.saturate();
        kani::assert(result == i32::MAX, "saturate clamps to MAX");
    }

    #[kani::proof]
    fn proof_saturate_i64_to_i32_underflow() {
        let value: i64 = (i32::MIN as i64) - 1;

        let result: i32 = value.saturate();
        kani::assert(result == i32::MIN, "saturate clamps to MIN");
    }

    #[kani::proof]
    fn proof_saturate_u64_to_u32_in_range() {
        let value: u32 = kani::any();
        let as_u64 = value as u64;

        let result: u32 = as_u64.saturate();
        kani::assert(result == value, "saturate preserves in-range value");
    }

    #[kani::proof]
    fn proof_saturate_u64_to_u32_overflow() {
        let value: u64 = (u32::MAX as u64) + 1;

        let result: u32 = value.saturate();
        kani::assert(result == u32::MAX, "saturate clamps to MAX");
    }

    #[kani::proof]
    fn proof_saturate_i64_to_u32_negative() {
        let value: i64 = -100;

        let result: u32 = value.saturate();
        kani::assert(result == 0, "saturate clamps negative to 0");
    }

    #[kani::proof]
    fn proof_saturate_i64_to_u32_overflow() {
        let value: i64 = (u32::MAX as i64) + 1;

        let result: u32 = value.saturate();
        kani::assert(result == u32::MAX, "saturate clamps to MAX");
    }

    #[kani::proof]
    fn proof_saturate_i32_to_u8_in_range() {
        let value: i32 = kani::any();
        kani::assume(value >= 0 && value <= 255);

        let result: u8 = value.saturate();
        kani::assert(result == value as u8, "saturate preserves in-range value");
    }

    #[kani::proof]
    fn proof_saturate_i32_to_u8_negative() {
        let value: i32 = -50;

        let result: u8 = value.saturate();
        kani::assert(result == 0, "saturate clamps negative to 0");
    }

    #[kani::proof]
    fn proof_saturate_i32_to_u8_overflow() {
        let value: i32 = 300;

        let result: u8 = value.saturate();
        kani::assert(result == 255, "saturate clamps to u8::MAX");
    }

    // ========================================================================
    // Helper Function Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_cast_helper() {
        let value: i32 = kani::any();
        let as_i64 = value as i64;

        let result: Result<i32> = cast(as_i64);
        kani::assert(result.is_ok(), "cast helper works");
        kani::assert(result.unwrap() == value, "cast helper preserves value");
    }

    #[kani::proof]
    fn proof_checked_helper() {
        let value: i32 = kani::any();
        let as_i64 = value as i64;

        let result: Option<i32> = checked(as_i64);
        kani::assert(result.is_some(), "checked helper works");
        kani::assert(result.unwrap() == value, "checked helper preserves value");
    }

    #[kani::proof]
    fn proof_unchecked_helper() {
        let value: i32 = kani::any();
        let as_i64 = value as i64;

        let result: i32 = unchecked(as_i64);
        kani::assert(result == value, "unchecked helper preserves value");
    }

    #[kani::proof]
    fn proof_saturating_helper() {
        let value: i32 = kani::any();
        let as_i64 = value as i64;

        let result: i32 = saturating(as_i64);
        kani::assert(
            result == value,
            "saturating helper preserves in-range value",
        );
    }
}
