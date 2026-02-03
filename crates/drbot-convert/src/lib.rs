//! Type conversion utilities for drbot.
//!
//! This crate provides:
//! - Fallible conversion traits
//! - Conversion chains
//! - Type conversion registry

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;

/// Conversion error types.
#[derive(Error, Debug)]
pub enum ConvertError {
    #[error("Conversion failed: {0}")]
    Failed(String),

    #[error("No converter found for type")]
    NoConverter,

    #[error("Overflow")]
    Overflow,

    #[error("Underflow")]
    Underflow,

    #[error("Invalid input: {0}")]
    InvalidInput(String),
}

/// Result type for conversion operations.
pub type Result<T> = std::result::Result<T, ConvertError>;

/// Fallible conversion trait.
pub trait TryConvert<T>: Sized {
    /// Try to convert to target type.
    fn try_convert(value: T) -> Result<Self>;
}

/// Infallible conversion trait.
pub trait Convert<T>: Sized {
    /// Convert to target type.
    fn convert(value: T) -> Self;
}

// Implement TryConvert for numeric types
macro_rules! impl_try_convert_int {
    ($from:ty => $($to:ty),+) => {
        $(
            impl TryConvert<$from> for $to {
                fn try_convert(value: $from) -> Result<Self> {
                    <$to>::try_from(value).map_err(|_| {
                        if value < 0 as $from {
                            ConvertError::Underflow
                        } else {
                            ConvertError::Overflow
                        }
                    })
                }
            }
        )+
    };
}

impl_try_convert_int!(i64 => i8, i16, i32, u8, u16, u32, u64, usize);
impl_try_convert_int!(u64 => i8, i16, i32, i64, u8, u16, u32, usize);
impl_try_convert_int!(i32 => i8, i16, u8, u16, u32);
impl_try_convert_int!(u32 => i8, i16, i32, u8, u16);

/// Convert with default on failure.
pub fn convert_or_default<T, U>(value: T) -> U
where
    U: TryConvert<T> + Default,
{
    U::try_convert(value).unwrap_or_default()
}

/// Convert with fallback value on failure.
pub fn convert_or<T, U>(value: T, fallback: U) -> U
where
    U: TryConvert<T>,
{
    U::try_convert(value).unwrap_or(fallback)
}

/// Convert with fallback function on failure.
pub fn convert_or_else<T, U, F>(value: T, fallback: F) -> U
where
    U: TryConvert<T>,
    F: FnOnce() -> U,
{
    U::try_convert(value).unwrap_or_else(|_| fallback())
}

/// Conversion chain builder.
pub struct ConversionChain<T> {
    value: T,
}

impl<T> ConversionChain<T> {
    /// Start a conversion chain.
    pub fn new(value: T) -> Self {
        Self { value }
    }

    /// Convert to intermediate type.
    pub fn then<U>(self) -> Result<ConversionChain<U>>
    where
        U: TryConvert<T>,
    {
        Ok(ConversionChain {
            value: U::try_convert(self.value)?,
        })
    }

    /// Get final value.
    pub fn finish(self) -> T {
        self.value
    }
}

/// Start a conversion chain.
pub fn chain<T>(value: T) -> ConversionChain<T> {
    ConversionChain::new(value)
}

/// Type-erased converter.
type BoxedConverter = Box<dyn Fn(&dyn Any) -> Option<Box<dyn Any>> + Send + Sync>;

/// Dynamic type converter registry.
pub struct ConverterRegistry {
    converters: HashMap<(TypeId, TypeId), BoxedConverter>,
}

impl ConverterRegistry {
    /// Create new registry.
    pub fn new() -> Self {
        Self {
            converters: HashMap::new(),
        }
    }

    /// Register a converter.
    pub fn register<F, S, T>(&mut self, converter: F)
    where
        F: Fn(&S) -> Option<T> + Send + Sync + 'static,
        S: 'static,
        T: 'static,
    {
        let source_id = TypeId::of::<S>();
        let target_id = TypeId::of::<T>();

        let boxed: BoxedConverter = Box::new(move |any| {
            let source = any.downcast_ref::<S>()?;
            let result = converter(source)?;
            Some(Box::new(result) as Box<dyn Any>)
        });

        self.converters.insert((source_id, target_id), boxed);
    }

    /// Convert value using registered converter.
    pub fn convert<S: 'static, T: 'static>(&self, value: &S) -> Option<T> {
        let source_id = TypeId::of::<S>();
        let target_id = TypeId::of::<T>();

        let converter = self.converters.get(&(source_id, target_id))?;
        let result = converter(value)?;
        let boxed = result.downcast::<T>().ok()?;
        Some(*boxed)
    }
}

impl Default for ConverterRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Lossy conversion trait.
pub trait LossyConvert<T> {
    /// Convert with potential data loss.
    fn lossy_convert(value: T) -> Self;
}

impl LossyConvert<f64> for f32 {
    fn lossy_convert(value: f64) -> Self {
        value as f32
    }
}

impl LossyConvert<i64> for i32 {
    fn lossy_convert(value: i64) -> Self {
        value as i32
    }
}

impl LossyConvert<u64> for u32 {
    fn lossy_convert(value: u64) -> Self {
        value as u32
    }
}

/// Saturating conversion (clamps to min/max on overflow).
pub trait SaturatingConvert<T> {
    /// Convert with saturation.
    fn saturating_convert(value: T) -> Self;
}

impl SaturatingConvert<i64> for i32 {
    fn saturating_convert(value: i64) -> Self {
        if value > i32::MAX as i64 {
            i32::MAX
        } else if value < i32::MIN as i64 {
            i32::MIN
        } else {
            value as i32
        }
    }
}

impl SaturatingConvert<u64> for u32 {
    fn saturating_convert(value: u64) -> Self {
        if value > u32::MAX as u64 {
            u32::MAX
        } else {
            value as u32
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_try_convert() {
        let result: Result<i32> = TryConvert::try_convert(100i64);
        assert_eq!(result.unwrap(), 100);

        let overflow: Result<i8> = TryConvert::try_convert(1000i64);
        assert!(overflow.is_err());
    }

    #[test]
    fn test_convert_or_default() {
        let result: i32 = convert_or_default(100i64);
        assert_eq!(result, 100);

        let result: i8 = convert_or_default(1000i64);
        assert_eq!(result, 0); // Default for i8
    }

    #[test]
    fn test_registry() {
        let mut registry = ConverterRegistry::new();

        registry.register(|s: &String| Some(s.len()));

        let result: Option<usize> = registry.convert(&"hello".to_string());
        assert_eq!(result, Some(5));
    }

    #[test]
    fn test_saturating_convert() {
        let result = i32::saturating_convert(i64::MAX);
        assert_eq!(result, i32::MAX);

        let result = i32::saturating_convert(i64::MIN);
        assert_eq!(result, i32::MIN);
    }

    #[test]
    fn test_lossy_convert() {
        let result = f32::lossy_convert(std::f64::consts::PI);
        assert!((result - std::f32::consts::PI).abs() < 0.0001);
    }
}

// ============================================================================
// Kani Formal Verification Proofs
// ============================================================================

#[cfg(kani)]
mod kani_proofs {
    use super::*;

    // ========================================================================
    // TryConvert Trait Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_try_convert_i64_to_i32_valid_range() {
        let value: i64 = kani::any();
        kani::assume(value >= i32::MIN as i64 && value <= i32::MAX as i64);

        let result: Result<i32> = TryConvert::try_convert(value);

        kani::assert(result.is_ok(), "valid range must succeed");
        kani::assert(result.unwrap() as i64 == value, "value must be preserved");
    }

    #[kani::proof]
    fn proof_try_convert_i64_to_i32_overflow() {
        let value: i64 = kani::any();
        kani::assume(value > i32::MAX as i64);

        let result: Result<i32> = TryConvert::try_convert(value);

        kani::assert(result.is_err(), "overflow must fail");
    }

    #[kani::proof]
    fn proof_try_convert_i64_to_i32_underflow() {
        let value: i64 = kani::any();
        kani::assume(value < i32::MIN as i64);

        let result: Result<i32> = TryConvert::try_convert(value);

        kani::assert(result.is_err(), "underflow must fail");
    }

    #[kani::proof]
    fn proof_try_convert_i64_to_u8_valid_range() {
        let value: i64 = kani::any();
        kani::assume(value >= 0 && value <= 255);

        let result: Result<u8> = TryConvert::try_convert(value);

        kani::assert(result.is_ok(), "valid u8 range must succeed");
        kani::assert(result.unwrap() as i64 == value, "value must be preserved");
    }

    #[kani::proof]
    fn proof_try_convert_i64_to_u8_negative() {
        let value: i64 = kani::any();
        kani::assume(value < 0);

        let result: Result<u8> = TryConvert::try_convert(value);

        kani::assert(result.is_err(), "negative value must fail for u8");
    }

    #[kani::proof]
    fn proof_try_convert_u64_to_i32_valid_range() {
        let value: u64 = kani::any();
        kani::assume(value <= i32::MAX as u64);

        let result: Result<i32> = TryConvert::try_convert(value);

        kani::assert(result.is_ok(), "valid range must succeed");
        kani::assert(result.unwrap() as u64 == value, "value must be preserved");
    }

    #[kani::proof]
    fn proof_try_convert_u64_to_i32_overflow() {
        let value: u64 = kani::any();
        kani::assume(value > i32::MAX as u64);

        let result: Result<i32> = TryConvert::try_convert(value);

        kani::assert(result.is_err(), "overflow must fail");
    }

    // ========================================================================
    // convert_or_default() Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_convert_or_default_valid_value() {
        let value: i64 = kani::any();
        kani::assume(value >= i32::MIN as i64 && value <= i32::MAX as i64);

        let result: i32 = convert_or_default(value);

        kani::assert(result as i64 == value, "valid value must be preserved");
    }

    #[kani::proof]
    fn proof_convert_or_default_overflow_returns_default() {
        let value: i64 = kani::any();
        kani::assume(value > i32::MAX as i64);

        let result: i32 = convert_or_default(value);

        kani::assert(result == i32::default(), "overflow must return default");
    }

    #[kani::proof]
    fn proof_convert_or_default_underflow_returns_default() {
        let value: i64 = kani::any();
        kani::assume(value < i32::MIN as i64);

        let result: i32 = convert_or_default(value);

        kani::assert(result == i32::default(), "underflow must return default");
    }

    // ========================================================================
    // convert_or() Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_convert_or_valid_value() {
        let value: i64 = kani::any();
        let fallback: i32 = kani::any();
        kani::assume(value >= i32::MIN as i64 && value <= i32::MAX as i64);

        let result: i32 = convert_or(value, fallback);

        kani::assert(result as i64 == value, "valid value must be preserved");
    }

    #[kani::proof]
    fn proof_convert_or_invalid_returns_fallback() {
        let value: i64 = kani::any();
        let fallback: i32 = kani::any();
        kani::assume(value > i32::MAX as i64 || value < i32::MIN as i64);

        let result: i32 = convert_or(value, fallback);

        kani::assert(result == fallback, "invalid value must return fallback");
    }

    // ========================================================================
    // ConversionChain Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_chain_finish_returns_value() {
        let value: i32 = kani::any();

        let chain = ConversionChain::new(value);
        let result = chain.finish();

        kani::assert(result == value, "finish must return original value");
    }

    #[kani::proof]
    fn proof_chain_then_valid_conversion() {
        let value: i64 = kani::any();
        kani::assume(value >= i32::MIN as i64 && value <= i32::MAX as i64);

        let chain = chain(value);
        let result: Result<ConversionChain<i32>> = chain.then();

        kani::assert(result.is_ok(), "valid conversion must succeed");
        kani::assert(
            result.unwrap().finish() as i64 == value,
            "value must be preserved",
        );
    }

    #[kani::proof]
    fn proof_chain_then_invalid_conversion() {
        let value: i64 = kani::any();
        kani::assume(value > i32::MAX as i64);

        let chain = chain(value);
        let result: Result<ConversionChain<i32>> = chain.then();

        kani::assert(result.is_err(), "overflow must fail");
    }

    // ========================================================================
    // SaturatingConvert Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_saturating_convert_i64_to_i32_valid() {
        let value: i64 = kani::any();
        kani::assume(value >= i32::MIN as i64 && value <= i32::MAX as i64);

        let result = i32::saturating_convert(value);

        kani::assert(result as i64 == value, "valid value must be preserved");
    }

    #[kani::proof]
    fn proof_saturating_convert_i64_to_i32_overflow() {
        let value: i64 = kani::any();
        kani::assume(value > i32::MAX as i64);

        let result = i32::saturating_convert(value);

        kani::assert(result == i32::MAX, "overflow must saturate to MAX");
    }

    #[kani::proof]
    fn proof_saturating_convert_i64_to_i32_underflow() {
        let value: i64 = kani::any();
        kani::assume(value < i32::MIN as i64);

        let result = i32::saturating_convert(value);

        kani::assert(result == i32::MIN, "underflow must saturate to MIN");
    }

    #[kani::proof]
    fn proof_saturating_convert_u64_to_u32_valid() {
        let value: u64 = kani::any();
        kani::assume(value <= u32::MAX as u64);

        let result = u32::saturating_convert(value);

        kani::assert(result as u64 == value, "valid value must be preserved");
    }

    #[kani::proof]
    fn proof_saturating_convert_u64_to_u32_overflow() {
        let value: u64 = kani::any();
        kani::assume(value > u32::MAX as u64);

        let result = u32::saturating_convert(value);

        kani::assert(result == u32::MAX, "overflow must saturate to MAX");
    }

    #[kani::proof]
    fn proof_saturating_convert_result_in_range() {
        let value: i64 = kani::any();

        let result = i32::saturating_convert(value);

        kani::assert(result >= i32::MIN, "result must be >= MIN");
        kani::assert(result <= i32::MAX, "result must be <= MAX");
    }

    #[kani::proof]
    fn proof_saturating_convert_idempotent() {
        let value: i64 = kani::any();

        let once = i32::saturating_convert(value);
        let twice = i32::saturating_convert(once as i64);

        kani::assert(once == twice, "saturating convert must be idempotent");
    }

    // ========================================================================
    // LossyConvert Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_lossy_convert_i64_to_i32() {
        let value: i64 = kani::any();

        let result = i32::lossy_convert(value);

        // Lossy conversion truncates, so result should match the low 32 bits
        kani::assert(result == value as i32, "lossy must match cast");
    }

    #[kani::proof]
    fn proof_lossy_convert_u64_to_u32() {
        let value: u64 = kani::any();

        let result = u32::lossy_convert(value);

        kani::assert(result == value as u32, "lossy must match cast");
    }

    #[kani::proof]
    fn proof_lossy_convert_preserves_valid_values() {
        let value: i64 = kani::any();
        kani::assume(value >= i32::MIN as i64 && value <= i32::MAX as i64);

        let result = i32::lossy_convert(value);

        kani::assert(
            result as i64 == value,
            "valid values must be preserved exactly",
        );
    }

    // ========================================================================
    // ConverterRegistry Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_registry_new_is_empty() {
        let registry = ConverterRegistry::new();

        // Try to convert with no registered converters
        let result: Option<u32> = registry.convert(&42i32);

        kani::assert(result.is_none(), "empty registry must return None");
    }

    #[kani::proof]
    fn proof_registry_default_is_empty() {
        let registry = ConverterRegistry::default();

        let result: Option<u32> = registry.convert(&42i32);

        kani::assert(result.is_none(), "default registry must return None");
    }
}
