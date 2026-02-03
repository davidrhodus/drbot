//! Clamping utilities for drbot.
//!
//! This crate provides:
//! - Value clamping
//! - Range clamping
//! - Soft clamping

use thiserror::Error;

/// Clamp error types.
#[derive(Error, Debug, Clone)]
pub enum ClampError {
    #[error("Invalid range: min > max")]
    InvalidRange,
}

/// Result type for clamp operations.
pub type Result<T> = std::result::Result<T, ClampError>;

/// Clamp value to range.
pub fn clamp<T: Ord>(value: T, min: T, max: T) -> T {
    if value < min {
        min
    } else if value > max {
        max
    } else {
        value
    }
}

/// Clamp value to minimum.
pub fn clamp_min<T: Ord>(value: T, min: T) -> T {
    if value < min {
        min
    } else {
        value
    }
}

/// Clamp value to maximum.
pub fn clamp_max<T: Ord>(value: T, max: T) -> T {
    if value > max {
        max
    } else {
        value
    }
}

/// Extension trait for clamping.
pub trait ClampExt: Ord + Sized {
    /// Clamp to range.
    fn clamp_to(self, min: Self, max: Self) -> Self {
        clamp(self, min, max)
    }

    /// Clamp to minimum.
    fn clamp_min(self, min: Self) -> Self {
        clamp_min(self, min)
    }

    /// Clamp to maximum.
    fn clamp_max(self, max: Self) -> Self {
        clamp_max(self, max)
    }
}

impl<T: Ord> ClampExt for T {}

/// Clamp with info about whether clamping occurred.
pub fn clamp_with_info<T: Ord + Clone>(value: T, min: T, max: T) -> (T, ClampInfo) {
    if value < min {
        (min, ClampInfo::ClampedToMin)
    } else if value > max {
        (max, ClampInfo::ClampedToMax)
    } else {
        (value, ClampInfo::NotClamped)
    }
}

/// Information about clamping result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClampInfo {
    /// Value was not clamped.
    NotClamped,
    /// Value was clamped to minimum.
    ClampedToMin,
    /// Value was clamped to maximum.
    ClampedToMax,
}

impl ClampInfo {
    /// Check if clamping occurred.
    pub fn was_clamped(&self) -> bool {
        !matches!(self, Self::NotClamped)
    }
}

/// Soft clamp using smooth function.
pub fn soft_clamp(value: f64, min: f64, max: f64, softness: f64) -> f64 {
    let mid = (min + max) / 2.0;
    let half_range = (max - min) / 2.0;
    let normalized = (value - mid) / half_range;
    let soft = normalized.tanh() * (1.0 - softness) + normalized * softness;
    mid + soft * half_range
}

/// Soft clamp for f32.
pub fn soft_clamp_f32(value: f32, min: f32, max: f32, softness: f32) -> f32 {
    soft_clamp(value as f64, min as f64, max as f64, softness as f64) as f32
}

/// Wrap value to range (modular).
pub fn wrap<T>(value: T, min: T, max: T) -> T
where
    T: Copy
        + std::ops::Sub<Output = T>
        + std::ops::Rem<Output = T>
        + std::ops::Add<Output = T>
        + PartialOrd,
{
    let range = max - min;
    let mut result = (value - min) % range;
    if result < min - min {
        // If negative (for signed types)
        result = result + range;
    }
    result + min
}

/// Wrap integer value to range.
pub fn wrap_int(value: i64, min: i64, max: i64) -> i64 {
    let range = max - min;
    let mut result = (value - min) % range;
    if result < 0 {
        result += range;
    }
    result + min
}

/// Wrap float value to range.
pub fn wrap_float(value: f64, min: f64, max: f64) -> f64 {
    let range = max - min;
    let mut result = (value - min) % range;
    if result < 0.0 {
        result += range;
    }
    result + min
}

/// Mirror value at range boundaries.
pub fn mirror(value: f64, min: f64, max: f64) -> f64 {
    let range = max - min;
    let normalized = (value - min) / range;
    let period = normalized.floor() as i64;

    let fractional = normalized - period as f64;
    let mirrored = if period % 2 == 0 {
        fractional
    } else {
        1.0 - fractional
    };

    min + mirrored * range
}

/// A clamped value type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Clamped<T> {
    value: T,
    min: T,
    max: T,
}

impl<T: Ord + Clone> Clamped<T> {
    /// Create new clamped value.
    pub fn new(value: T, min: T, max: T) -> Self {
        let clamped = clamp(value, min.clone(), max.clone());
        Self {
            value: clamped,
            min,
            max,
        }
    }

    /// Get value.
    pub fn get(&self) -> &T {
        &self.value
    }

    /// Set value (will be clamped).
    pub fn set(&mut self, value: T) {
        self.value = clamp(value, self.min.clone(), self.max.clone());
    }

    /// Get minimum.
    pub fn min(&self) -> &T {
        &self.min
    }

    /// Get maximum.
    pub fn max(&self) -> &T {
        &self.max
    }

    /// Check if at minimum.
    pub fn is_at_min(&self) -> bool {
        self.value == self.min
    }

    /// Check if at maximum.
    pub fn is_at_max(&self) -> bool {
        self.value == self.max
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clamp() {
        assert_eq!(clamp(5, 0, 10), 5);
        assert_eq!(clamp(-5, 0, 10), 0);
        assert_eq!(clamp(15, 0, 10), 10);
    }

    #[test]
    fn test_clamp_ext() {
        assert_eq!(5.clamp_to(0, 10), 5);
        assert_eq!((-5).clamp_min(0), 0);
        assert_eq!(15.clamp_max(10), 10);
    }

    #[test]
    fn test_clamp_with_info() {
        let (val, info) = clamp_with_info(5, 0, 10);
        assert_eq!(val, 5);
        assert_eq!(info, ClampInfo::NotClamped);

        let (val, info) = clamp_with_info(-5, 0, 10);
        assert_eq!(val, 0);
        assert_eq!(info, ClampInfo::ClampedToMin);
    }

    #[test]
    fn test_wrap_int() {
        assert_eq!(wrap_int(15, 0, 10), 5);
        assert_eq!(wrap_int(-3, 0, 10), 7);
        assert_eq!(wrap_int(5, 0, 10), 5);
    }

    #[test]
    fn test_clamped_type() {
        let mut c = Clamped::new(15, 0, 10);
        assert_eq!(*c.get(), 10);

        c.set(5);
        assert_eq!(*c.get(), 5);

        c.set(-5);
        assert_eq!(*c.get(), 0);
        assert!(c.is_at_min());
    }
}

// ============================================================================
// Kani Formal Verification Proofs
// ============================================================================

#[cfg(kani)]
mod kani_proofs {
    use super::*;

    // ========================================================================
    // clamp() Function Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_clamp_result_in_range() {
        let value: i8 = kani::any();
        let min: i8 = kani::any();
        let max: i8 = kani::any();
        kani::assume(min <= max);

        let result = clamp(value, min, max);

        kani::assert(result >= min, "clamped value must be >= min");
        kani::assert(result <= max, "clamped value must be <= max");
    }

    #[kani::proof]
    fn proof_clamp_preserves_value_in_range() {
        let value: i8 = kani::any();
        let min: i8 = kani::any();
        let max: i8 = kani::any();
        kani::assume(min <= max);
        kani::assume(value >= min && value <= max);

        let result = clamp(value, min, max);

        kani::assert(result == value, "value in range must be preserved");
    }

    #[kani::proof]
    fn proof_clamp_below_min_returns_min() {
        let value: i8 = kani::any();
        let min: i8 = kani::any();
        let max: i8 = kani::any();
        kani::assume(min <= max);
        kani::assume(value < min);

        let result = clamp(value, min, max);

        kani::assert(result == min, "value below min must clamp to min");
    }

    #[kani::proof]
    fn proof_clamp_above_max_returns_max() {
        let value: i8 = kani::any();
        let min: i8 = kani::any();
        let max: i8 = kani::any();
        kani::assume(min <= max);
        kani::assume(value > max);

        let result = clamp(value, min, max);

        kani::assert(result == max, "value above max must clamp to max");
    }

    #[kani::proof]
    fn proof_clamp_idempotent() {
        let value: i8 = kani::any();
        let min: i8 = kani::any();
        let max: i8 = kani::any();
        kani::assume(min <= max);

        let once = clamp(value, min, max);
        let twice = clamp(once, min, max);

        kani::assert(once == twice, "clamp must be idempotent");
    }

    // ========================================================================
    // clamp_min() / clamp_max() Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_clamp_min_result_ge_min() {
        let value: i8 = kani::any();
        let min: i8 = kani::any();

        let result = clamp_min(value, min);

        kani::assert(result >= min, "clamp_min result must be >= min");
    }

    #[kani::proof]
    fn proof_clamp_min_preserves_when_above() {
        let value: i8 = kani::any();
        let min: i8 = kani::any();
        kani::assume(value >= min);

        let result = clamp_min(value, min);

        kani::assert(result == value, "value >= min must be preserved");
    }

    #[kani::proof]
    fn proof_clamp_max_result_le_max() {
        let value: i8 = kani::any();
        let max: i8 = kani::any();

        let result = clamp_max(value, max);

        kani::assert(result <= max, "clamp_max result must be <= max");
    }

    #[kani::proof]
    fn proof_clamp_max_preserves_when_below() {
        let value: i8 = kani::any();
        let max: i8 = kani::any();
        kani::assume(value <= max);

        let result = clamp_max(value, max);

        kani::assert(result == value, "value <= max must be preserved");
    }

    // ========================================================================
    // ClampExt Trait Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_clamp_ext_clamp_to() {
        let value: i8 = kani::any();
        let min: i8 = kani::any();
        let max: i8 = kani::any();
        kani::assume(min <= max);

        let ext_result = value.clamp_to(min, max);
        let fn_result = clamp(value, min, max);

        kani::assert(
            ext_result == fn_result,
            "clamp_to must match clamp function",
        );
    }

    #[kani::proof]
    fn proof_clamp_ext_clamp_min_matches() {
        let value: i8 = kani::any();
        let min: i8 = kani::any();

        let ext_result = value.clamp_min(min);
        let fn_result = clamp_min(value, min);

        kani::assert(
            ext_result == fn_result,
            "ClampExt::clamp_min must match clamp_min",
        );
    }

    #[kani::proof]
    fn proof_clamp_ext_clamp_max_matches() {
        let value: i8 = kani::any();
        let max: i8 = kani::any();

        let ext_result = value.clamp_max(max);
        let fn_result = clamp_max(value, max);

        kani::assert(
            ext_result == fn_result,
            "ClampExt::clamp_max must match clamp_max",
        );
    }

    // ========================================================================
    // clamp_with_info() Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_clamp_with_info_value_matches_clamp() {
        let value: i8 = kani::any();
        let min: i8 = kani::any();
        let max: i8 = kani::any();
        kani::assume(min <= max);

        let (result, _info) = clamp_with_info(value, min, max);
        let expected = clamp(value, min, max);

        kani::assert(result == expected, "clamp_with_info value must match clamp");
    }

    #[kani::proof]
    fn proof_clamp_with_info_not_clamped() {
        let value: i8 = kani::any();
        let min: i8 = kani::any();
        let max: i8 = kani::any();
        kani::assume(min <= max);
        kani::assume(value >= min && value <= max);

        let (_result, info) = clamp_with_info(value, min, max);

        kani::assert(
            info == ClampInfo::NotClamped,
            "in-range value must not be clamped",
        );
        kani::assert(!info.was_clamped(), "was_clamped must be false");
    }

    #[kani::proof]
    fn proof_clamp_with_info_clamped_to_min() {
        let value: i8 = kani::any();
        let min: i8 = kani::any();
        let max: i8 = kani::any();
        kani::assume(min <= max);
        kani::assume(value < min);

        let (_result, info) = clamp_with_info(value, min, max);

        kani::assert(
            info == ClampInfo::ClampedToMin,
            "below-min must report ClampedToMin",
        );
        kani::assert(info.was_clamped(), "was_clamped must be true");
    }

    #[kani::proof]
    fn proof_clamp_with_info_clamped_to_max() {
        let value: i8 = kani::any();
        let min: i8 = kani::any();
        let max: i8 = kani::any();
        kani::assume(min <= max);
        kani::assume(value > max);

        let (_result, info) = clamp_with_info(value, min, max);

        kani::assert(
            info == ClampInfo::ClampedToMax,
            "above-max must report ClampedToMax",
        );
        kani::assert(info.was_clamped(), "was_clamped must be true");
    }

    // ========================================================================
    // wrap_int() Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_wrap_int_result_in_range() {
        let value: i8 = kani::any();
        let min: i8 = kani::any();
        let max: i8 = kani::any();
        kani::assume(min < max); // Range must be non-empty

        let result = wrap_int(value as i64, min as i64, max as i64);

        kani::assert(result >= min as i64, "wrapped value must be >= min");
        kani::assert(result < max as i64, "wrapped value must be < max");
    }

    #[kani::proof]
    fn proof_wrap_int_preserves_value_in_range() {
        let value: i8 = kani::any();
        let min: i8 = kani::any();
        let max: i8 = kani::any();
        kani::assume(min < max);
        kani::assume(value >= min && value < max);

        let result = wrap_int(value as i64, min as i64, max as i64);

        kani::assert(result == value as i64, "value in range must be preserved");
    }

    // ========================================================================
    // Clamped<T> Type Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_clamped_new_value_in_range() {
        let value: i8 = kani::any();
        let min: i8 = kani::any();
        let max: i8 = kani::any();
        kani::assume(min <= max);

        let c = Clamped::new(value, min, max);

        kani::assert(*c.get() >= min, "clamped value must be >= min");
        kani::assert(*c.get() <= max, "clamped value must be <= max");
    }

    #[kani::proof]
    fn proof_clamped_new_matches_clamp() {
        let value: i8 = kani::any();
        let min: i8 = kani::any();
        let max: i8 = kani::any();
        kani::assume(min <= max);

        let c = Clamped::new(value, min, max);
        let expected = clamp(value, min, max);

        kani::assert(
            *c.get() == expected,
            "Clamped::new must match clamp function",
        );
    }

    #[kani::proof]
    fn proof_clamped_min_max_accessors() {
        let value: i8 = kani::any();
        let min: i8 = kani::any();
        let max: i8 = kani::any();
        kani::assume(min <= max);

        let c = Clamped::new(value, min, max);

        kani::assert(*c.min() == min, "min() must return min");
        kani::assert(*c.max() == max, "max() must return max");
    }

    #[kani::proof]
    fn proof_clamped_set_clamps_value() {
        let init: i8 = kani::any();
        let new_value: i8 = kani::any();
        let min: i8 = kani::any();
        let max: i8 = kani::any();
        kani::assume(min <= max);

        let mut c = Clamped::new(init, min, max);
        c.set(new_value);

        kani::assert(*c.get() >= min, "set value must be >= min");
        kani::assert(*c.get() <= max, "set value must be <= max");
        kani::assert(
            *c.get() == clamp(new_value, min, max),
            "set must clamp value",
        );
    }

    #[kani::proof]
    fn proof_clamped_is_at_min() {
        let value: i8 = kani::any();
        let min: i8 = kani::any();
        let max: i8 = kani::any();
        kani::assume(min <= max);
        kani::assume(value <= min); // Ensure value will be clamped to min

        let c = Clamped::new(value, min, max);

        kani::assert(c.is_at_min(), "value <= min must result in is_at_min()");
    }

    #[kani::proof]
    fn proof_clamped_is_at_max() {
        let value: i8 = kani::any();
        let min: i8 = kani::any();
        let max: i8 = kani::any();
        kani::assume(min <= max);
        kani::assume(value >= max); // Ensure value will be clamped to max

        let c = Clamped::new(value, min, max);

        kani::assert(c.is_at_max(), "value >= max must result in is_at_max()");
    }

    #[kani::proof]
    fn proof_clamped_invariant_preserved() {
        let init: i8 = kani::any();
        let v1: i8 = kani::any();
        let v2: i8 = kani::any();
        let min: i8 = kani::any();
        let max: i8 = kani::any();
        kani::assume(min <= max);

        let mut c = Clamped::new(init, min, max);

        // Check invariant after creation
        kani::assert(*c.get() >= min && *c.get() <= max, "invariant after new");

        // Check invariant after set
        c.set(v1);
        kani::assert(
            *c.get() >= min && *c.get() <= max,
            "invariant after first set",
        );

        c.set(v2);
        kani::assert(
            *c.get() >= min && *c.get() <= max,
            "invariant after second set",
        );
    }
}
