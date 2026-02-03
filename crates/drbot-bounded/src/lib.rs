//! Bounded numeric types for drbot.
//!
//! This crate provides:
//! - Bounded integer types
//! - Range-constrained values
//! - Clamped values

use thiserror::Error;

/// Bounded error types.
#[derive(Error, Debug, Clone)]
pub enum BoundedError {
    #[error("Value {0} is below minimum {1}")]
    BelowMinimum(String, String),

    #[error("Value {0} is above maximum {1}")]
    AboveMaximum(String, String),

    #[error("Invalid bounds: min {0} > max {1}")]
    InvalidBounds(String, String),
}

/// Result type for bounded operations.
pub type Result<T> = std::result::Result<T, BoundedError>;

/// A value bounded between min and max.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Bounded<T> {
    value: T,
    min: T,
    max: T,
}

impl<T: Copy + Ord + std::fmt::Display> Bounded<T> {
    /// Create new bounded value.
    pub fn new(value: T, min: T, max: T) -> Result<Self> {
        if min > max {
            return Err(BoundedError::InvalidBounds(
                min.to_string(),
                max.to_string(),
            ));
        }
        if value < min {
            return Err(BoundedError::BelowMinimum(
                value.to_string(),
                min.to_string(),
            ));
        }
        if value > max {
            return Err(BoundedError::AboveMaximum(
                value.to_string(),
                max.to_string(),
            ));
        }
        Ok(Self { value, min, max })
    }

    /// Create bounded value, clamping to bounds.
    pub fn clamped(value: T, min: T, max: T) -> Self {
        let clamped = if value < min {
            min
        } else if value > max {
            max
        } else {
            value
        };
        Self {
            value: clamped,
            min,
            max,
        }
    }

    /// Get value.
    pub fn get(&self) -> T {
        self.value
    }

    /// Get minimum.
    pub fn min(&self) -> T {
        self.min
    }

    /// Get maximum.
    pub fn max(&self) -> T {
        self.max
    }

    /// Set value with bounds checking.
    pub fn set(&mut self, value: T) -> Result<()> {
        if value < self.min {
            return Err(BoundedError::BelowMinimum(
                value.to_string(),
                self.min.to_string(),
            ));
        }
        if value > self.max {
            return Err(BoundedError::AboveMaximum(
                value.to_string(),
                self.max.to_string(),
            ));
        }
        self.value = value;
        Ok(())
    }

    /// Set value, clamping to bounds.
    pub fn set_clamped(&mut self, value: T) {
        self.value = if value < self.min {
            self.min
        } else if value > self.max {
            self.max
        } else {
            value
        };
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

/// A percentage (0-100).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Percentage(u8);

impl Percentage {
    /// Create new percentage.
    pub fn new(value: u8) -> Result<Self> {
        if value > 100 {
            return Err(BoundedError::AboveMaximum(
                value.to_string(),
                "100".to_string(),
            ));
        }
        Ok(Self(value))
    }

    /// Create percentage, clamping to 0-100.
    pub fn clamped(value: u8) -> Self {
        Self(value.min(100))
    }

    /// Create from ratio (0.0 - 1.0).
    pub fn from_ratio(ratio: f64) -> Self {
        Self((ratio.clamp(0.0, 1.0) * 100.0) as u8)
    }

    /// Get value.
    pub fn get(&self) -> u8 {
        self.0
    }

    /// Get as ratio (0.0 - 1.0).
    pub fn as_ratio(&self) -> f64 {
        self.0 as f64 / 100.0
    }

    /// Zero percent.
    pub const fn zero() -> Self {
        Self(0)
    }

    /// Full percent (100%).
    pub const fn full() -> Self {
        Self(100)
    }

    /// Half (50%).
    pub const fn half() -> Self {
        Self(50)
    }
}

/// A unit interval value (0.0 - 1.0).
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
pub struct UnitInterval(f64);

impl UnitInterval {
    /// Create new unit interval value.
    pub fn new(value: f64) -> Result<Self> {
        if value < 0.0 {
            return Err(BoundedError::BelowMinimum(
                value.to_string(),
                "0.0".to_string(),
            ));
        }
        if value > 1.0 {
            return Err(BoundedError::AboveMaximum(
                value.to_string(),
                "1.0".to_string(),
            ));
        }
        Ok(Self(value))
    }

    /// Create clamped to 0-1.
    pub fn clamped(value: f64) -> Self {
        Self(value.clamp(0.0, 1.0))
    }

    /// Get value.
    pub fn get(&self) -> f64 {
        self.0
    }

    /// Zero.
    pub const fn zero() -> Self {
        Self(0.0)
    }

    /// One.
    pub const fn one() -> Self {
        Self(1.0)
    }

    /// Half.
    pub fn half() -> Self {
        Self(0.5)
    }

    /// Complement (1 - value).
    pub fn complement(&self) -> Self {
        Self(1.0 - self.0)
    }

    /// Interpolate between two values.
    pub fn lerp(&self, a: f64, b: f64) -> f64 {
        a + (b - a) * self.0
    }
}

/// An angle in degrees (0-360).
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
pub struct Degrees(f64);

impl Degrees {
    /// Create new angle, normalizing to 0-360.
    pub fn new(value: f64) -> Self {
        let normalized = value.rem_euclid(360.0);
        Self(normalized)
    }

    /// Get value.
    pub fn get(&self) -> f64 {
        self.0
    }

    /// Convert to radians.
    pub fn to_radians(&self) -> f64 {
        self.0.to_radians()
    }

    /// Create from radians.
    pub fn from_radians(rad: f64) -> Self {
        Self::new(rad.to_degrees())
    }

    /// Add angles.
    pub fn add(&self, other: Self) -> Self {
        Self::new(self.0 + other.0)
    }

    /// Subtract angles.
    pub fn sub(&self, other: Self) -> Self {
        Self::new(self.0 - other.0)
    }
}

/// A byte value (0-255).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct ByteValue(u8);

impl ByteValue {
    /// Create new byte value.
    pub fn new(value: u8) -> Self {
        Self(value)
    }

    /// Get value.
    pub fn get(&self) -> u8 {
        self.0
    }

    /// Create from normalized float (0.0 - 1.0).
    pub fn from_float(value: f64) -> Self {
        Self((value.clamp(0.0, 1.0) * 255.0) as u8)
    }

    /// Convert to normalized float.
    pub fn to_float(&self) -> f64 {
        self.0 as f64 / 255.0
    }

    /// Minimum value.
    pub const fn min() -> Self {
        Self(0)
    }

    /// Maximum value.
    pub const fn max() -> Self {
        Self(255)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bounded() {
        let b = Bounded::new(50, 0, 100);
        assert!(b.is_ok());
        assert_eq!(b.unwrap().get(), 50);

        let below = Bounded::new(-1, 0, 100);
        assert!(below.is_err());

        let above = Bounded::new(101, 0, 100);
        assert!(above.is_err());
    }

    #[test]
    fn test_bounded_clamped() {
        let b = Bounded::clamped(150, 0, 100);
        assert_eq!(b.get(), 100);

        let b2 = Bounded::clamped(-10, 0, 100);
        assert_eq!(b2.get(), 0);
    }

    #[test]
    fn test_percentage() {
        let p = Percentage::new(50);
        assert!(p.is_ok());
        assert_eq!(p.unwrap().as_ratio(), 0.5);

        let invalid = Percentage::new(150);
        assert!(invalid.is_err());

        let clamped = Percentage::clamped(150);
        assert_eq!(clamped.get(), 100);
    }

    #[test]
    fn test_unit_interval() {
        let u = UnitInterval::new(0.5);
        assert!(u.is_ok());

        let lerp_result = u.unwrap().lerp(0.0, 100.0);
        assert_eq!(lerp_result, 50.0);
    }

    #[test]
    fn test_degrees() {
        let d = Degrees::new(450.0);
        assert_eq!(d.get(), 90.0);

        let d2 = Degrees::new(-90.0);
        assert_eq!(d2.get(), 270.0);
    }

    #[test]
    fn test_byte_value() {
        let b = ByteValue::from_float(0.5);
        assert_eq!(b.get(), 127);

        assert!((b.to_float() - 0.498).abs() < 0.01);
    }
}

// ============================================================================
// Kani Formal Verification Proofs
// ============================================================================

#[cfg(kani)]
mod kani_proofs {
    use super::*;

    // ------------------------------------------------------------------------
    // Bounded<T> Proofs
    // ------------------------------------------------------------------------

    #[kani::proof]
    fn proof_bounded_valid_bounds() {
        let min: i8 = kani::any();
        let max: i8 = kani::any();
        let value: i8 = kani::any();
        kani::assume(min <= max);
        kani::assume(value >= min && value <= max);

        let result = Bounded::new(value, min, max);
        kani::assert(result.is_ok(), "Valid bounds and value should succeed");

        let b = result.unwrap();
        kani::assert(b.get() == value, "Get returns the value");
        kani::assert(b.min() == min, "Min returns the minimum");
        kani::assert(b.max() == max, "Max returns the maximum");
    }

    #[kani::proof]
    fn proof_bounded_below_min_fails() {
        let min: i8 = kani::any();
        let max: i8 = kani::any();
        let value: i8 = kani::any();
        kani::assume(min <= max);
        kani::assume(value < min);

        let result = Bounded::new(value, min, max);
        kani::assert(result.is_err(), "Value below min should fail");
    }

    #[kani::proof]
    fn proof_bounded_above_max_fails() {
        let min: i8 = kani::any();
        let max: i8 = kani::any();
        let value: i8 = kani::any();
        kani::assume(min <= max);
        kani::assume(value > max);

        let result = Bounded::new(value, min, max);
        kani::assert(result.is_err(), "Value above max should fail");
    }

    #[kani::proof]
    fn proof_bounded_invalid_bounds_fails() {
        let min: i8 = kani::any();
        let max: i8 = kani::any();
        let value: i8 = kani::any();
        kani::assume(min > max);

        let result = Bounded::new(value, min, max);
        kani::assert(result.is_err(), "Invalid bounds (min > max) should fail");
    }

    #[kani::proof]
    fn proof_bounded_clamped_within_bounds() {
        let min: i8 = kani::any();
        let max: i8 = kani::any();
        let value: i8 = kani::any();
        kani::assume(min <= max);

        let b = Bounded::clamped(value, min, max);
        let v = b.get();

        kani::assert(v >= min, "Clamped value >= min");
        kani::assert(v <= max, "Clamped value <= max");
    }

    #[kani::proof]
    fn proof_bounded_clamped_preserves_valid() {
        let min: i8 = kani::any();
        let max: i8 = kani::any();
        let value: i8 = kani::any();
        kani::assume(min <= max);
        kani::assume(value >= min && value <= max);

        let b = Bounded::clamped(value, min, max);
        kani::assert(b.get() == value, "Clamped preserves valid values");
    }

    #[kani::proof]
    fn proof_bounded_is_at_min() {
        let min: i8 = kani::any();
        let max: i8 = kani::any();
        kani::assume(min <= max);

        let b = Bounded::new(min, min, max).unwrap();
        kani::assert(
            b.is_at_min(),
            "Value at min should return true for is_at_min",
        );
    }

    #[kani::proof]
    fn proof_bounded_is_at_max() {
        let min: i8 = kani::any();
        let max: i8 = kani::any();
        kani::assume(min <= max);

        let b = Bounded::new(max, min, max).unwrap();
        kani::assert(
            b.is_at_max(),
            "Value at max should return true for is_at_max",
        );
    }

    #[kani::proof]
    fn proof_bounded_set_valid() {
        let min: i8 = kani::any();
        let max: i8 = kani::any();
        let v1: i8 = kani::any();
        let v2: i8 = kani::any();
        kani::assume(min <= max);
        kani::assume(v1 >= min && v1 <= max);
        kani::assume(v2 >= min && v2 <= max);

        let mut b = Bounded::new(v1, min, max).unwrap();
        let result = b.set(v2);

        kani::assert(result.is_ok(), "Set with valid value should succeed");
        kani::assert(b.get() == v2, "Get returns new value after set");
    }

    #[kani::proof]
    fn proof_bounded_set_clamped() {
        let min: i8 = kani::any();
        let max: i8 = kani::any();
        let v1: i8 = kani::any();
        let v2: i8 = kani::any();
        kani::assume(min <= max);
        kani::assume(v1 >= min && v1 <= max);

        let mut b = Bounded::new(v1, min, max).unwrap();
        b.set_clamped(v2);

        kani::assert(b.get() >= min, "Set clamped value >= min");
        kani::assert(b.get() <= max, "Set clamped value <= max");
    }

    // ------------------------------------------------------------------------
    // Percentage Proofs
    // ------------------------------------------------------------------------

    #[kani::proof]
    fn proof_percentage_valid() {
        let value: u8 = kani::any();
        kani::assume(value <= 100);

        let result = Percentage::new(value);
        kani::assert(result.is_ok(), "Valid percentage (0-100) should succeed");
        kani::assert(result.unwrap().get() == value, "Get returns the value");
    }

    #[kani::proof]
    fn proof_percentage_above_100_fails() {
        let value: u8 = kani::any();
        kani::assume(value > 100);

        let result = Percentage::new(value);
        kani::assert(result.is_err(), "Percentage > 100 should fail");
    }

    #[kani::proof]
    fn proof_percentage_clamped_bounds() {
        let value: u8 = kani::any();

        let p = Percentage::clamped(value);
        kani::assert(p.get() <= 100, "Clamped percentage <= 100");
    }

    #[kani::proof]
    fn proof_percentage_as_ratio_bounds() {
        let value: u8 = kani::any();
        kani::assume(value <= 100);

        let p = Percentage::new(value).unwrap();
        let ratio = p.as_ratio();

        kani::assert(ratio >= 0.0, "Ratio >= 0.0");
        kani::assert(ratio <= 1.0, "Ratio <= 1.0");
    }

    #[kani::proof]
    fn proof_percentage_constants() {
        kani::assert(Percentage::zero().get() == 0, "zero() returns 0");
        kani::assert(Percentage::half().get() == 50, "half() returns 50");
        kani::assert(Percentage::full().get() == 100, "full() returns 100");
    }

    // ------------------------------------------------------------------------
    // UnitInterval Proofs
    // ------------------------------------------------------------------------

    #[kani::proof]
    fn proof_unit_interval_below_zero_fails() {
        // Test a negative value
        let result = UnitInterval::new(-0.5);
        kani::assert(result.is_err(), "Negative value should fail");
    }

    #[kani::proof]
    fn proof_unit_interval_above_one_fails() {
        // Test value above 1.0
        let result = UnitInterval::new(1.5);
        kani::assert(result.is_err(), "Value > 1.0 should fail");
    }

    #[kani::proof]
    fn proof_unit_interval_constants() {
        let zero = UnitInterval::zero();
        let one = UnitInterval::one();
        let half = UnitInterval::half();

        kani::assert(zero.get() == 0.0, "zero() returns 0.0");
        kani::assert(one.get() == 1.0, "one() returns 1.0");
        kani::assert(half.get() == 0.5, "half() returns 0.5");
    }

    #[kani::proof]
    fn proof_unit_interval_complement() {
        let half = UnitInterval::half();
        let comp = half.complement();

        kani::assert(comp.get() == 0.5, "Complement of 0.5 is 0.5");
    }

    #[kani::proof]
    fn proof_unit_interval_lerp_endpoints() {
        let zero = UnitInterval::zero();
        let one = UnitInterval::one();

        kani::assert(zero.lerp(10.0, 20.0) == 10.0, "lerp(0) returns a");
        kani::assert(one.lerp(10.0, 20.0) == 20.0, "lerp(1) returns b");
    }

    // ------------------------------------------------------------------------
    // Degrees Proofs
    // ------------------------------------------------------------------------

    #[kani::proof]
    fn proof_degrees_normalize_positive() {
        let d = Degrees::new(450.0);
        kani::assert(d.get() == 90.0, "450 degrees normalizes to 90");
    }

    #[kani::proof]
    fn proof_degrees_bounds() {
        // Test that normalized values are in [0, 360)
        let d1 = Degrees::new(0.0);
        let d2 = Degrees::new(180.0);
        let d3 = Degrees::new(359.0);

        kani::assert(d1.get() >= 0.0 && d1.get() < 360.0, "0 degrees in bounds");
        kani::assert(d2.get() >= 0.0 && d2.get() < 360.0, "180 degrees in bounds");
        kani::assert(d3.get() >= 0.0 && d3.get() < 360.0, "359 degrees in bounds");
    }

    #[kani::proof]
    fn proof_degrees_add() {
        let d1 = Degrees::new(90.0);
        let d2 = Degrees::new(180.0);
        let sum = d1.add(d2);

        kani::assert(sum.get() == 270.0, "90 + 180 = 270 degrees");
    }

    #[kani::proof]
    fn proof_degrees_add_wrap() {
        let d1 = Degrees::new(270.0);
        let d2 = Degrees::new(180.0);
        let sum = d1.add(d2);

        kani::assert(sum.get() == 90.0, "270 + 180 = 90 degrees (wrapped)");
    }

    // ------------------------------------------------------------------------
    // ByteValue Proofs
    // ------------------------------------------------------------------------

    #[kani::proof]
    fn proof_byte_value_new() {
        let value: u8 = kani::any();
        let b = ByteValue::new(value);

        kani::assert(b.get() == value, "new() preserves value");
    }

    #[kani::proof]
    fn proof_byte_value_min_max() {
        kani::assert(ByteValue::min().get() == 0, "min() is 0");
        kani::assert(ByteValue::max().get() == 255, "max() is 255");
    }

    #[kani::proof]
    fn proof_byte_value_from_float_bounds() {
        // Test boundary values
        let b0 = ByteValue::from_float(0.0);
        let b1 = ByteValue::from_float(1.0);

        kani::assert(b0.get() == 0, "from_float(0.0) is 0");
        kani::assert(b1.get() == 255, "from_float(1.0) is 255");
    }

    #[kani::proof]
    fn proof_byte_value_from_float_clamps() {
        let below = ByteValue::from_float(-0.5);
        let above = ByteValue::from_float(1.5);

        kani::assert(below.get() == 0, "Negative float clamps to 0");
        kani::assert(above.get() == 255, "Float > 1.0 clamps to 255");
    }

    #[kani::proof]
    fn proof_byte_value_to_float_bounds() {
        let b: u8 = kani::any();
        let bv = ByteValue::new(b);
        let f = bv.to_float();

        kani::assert(f >= 0.0, "to_float >= 0.0");
        kani::assert(f <= 1.0, "to_float <= 1.0");
    }
}
