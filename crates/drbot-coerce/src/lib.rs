//! Type coercion utilities for drbot.
//!
//! This crate provides:
//! - Automatic type coercion
//! - Coercion rules
//! - Type promotion

use std::any::{Any, TypeId};
use std::collections::HashMap;
use thiserror::Error;

/// Coercion error types.
#[derive(Error, Debug)]
pub enum CoerceError {
    #[error("Cannot coerce {0} to {1}")]
    CannotCoerce(String, String),

    #[error("No coercion rule found")]
    NoRule,

    #[error("Coercion would lose precision")]
    PrecisionLoss,
}

/// Result type for coercion operations.
pub type Result<T> = std::result::Result<T, CoerceError>;

/// Coercible trait for types that can be coerced.
pub trait Coercible {
    /// Try to coerce to bool.
    fn to_bool(&self) -> Option<bool> {
        None
    }

    /// Try to coerce to integer.
    fn to_int(&self) -> Option<i64> {
        None
    }

    /// Try to coerce to float.
    fn to_float(&self) -> Option<f64> {
        None
    }

    /// Try to coerce to string.
    fn to_string_value(&self) -> Option<String> {
        None
    }
}

impl Coercible for bool {
    fn to_bool(&self) -> Option<bool> {
        Some(*self)
    }

    fn to_int(&self) -> Option<i64> {
        Some(if *self { 1 } else { 0 })
    }

    fn to_float(&self) -> Option<f64> {
        Some(if *self { 1.0 } else { 0.0 })
    }

    fn to_string_value(&self) -> Option<String> {
        Some(self.to_string())
    }
}

impl Coercible for i64 {
    fn to_bool(&self) -> Option<bool> {
        Some(*self != 0)
    }

    fn to_int(&self) -> Option<i64> {
        Some(*self)
    }

    fn to_float(&self) -> Option<f64> {
        Some(*self as f64)
    }

    fn to_string_value(&self) -> Option<String> {
        Some(self.to_string())
    }
}

impl Coercible for i32 {
    fn to_bool(&self) -> Option<bool> {
        Some(*self != 0)
    }

    fn to_int(&self) -> Option<i64> {
        Some(*self as i64)
    }

    fn to_float(&self) -> Option<f64> {
        Some(*self as f64)
    }

    fn to_string_value(&self) -> Option<String> {
        Some(self.to_string())
    }
}

impl Coercible for f64 {
    fn to_bool(&self) -> Option<bool> {
        Some(*self != 0.0)
    }

    fn to_int(&self) -> Option<i64> {
        Some(*self as i64)
    }

    fn to_float(&self) -> Option<f64> {
        Some(*self)
    }

    fn to_string_value(&self) -> Option<String> {
        Some(self.to_string())
    }
}

impl Coercible for String {
    fn to_bool(&self) -> Option<bool> {
        match self.to_lowercase().as_str() {
            "true" | "1" | "yes" | "on" => Some(true),
            "false" | "0" | "no" | "off" | "" => Some(false),
            _ => None,
        }
    }

    fn to_int(&self) -> Option<i64> {
        self.parse().ok()
    }

    fn to_float(&self) -> Option<f64> {
        self.parse().ok()
    }

    fn to_string_value(&self) -> Option<String> {
        Some(self.clone())
    }
}

impl Coercible for &str {
    fn to_bool(&self) -> Option<bool> {
        match self.to_lowercase().as_str() {
            "true" | "1" | "yes" | "on" => Some(true),
            "false" | "0" | "no" | "off" | "" => Some(false),
            _ => None,
        }
    }

    fn to_int(&self) -> Option<i64> {
        self.parse().ok()
    }

    fn to_float(&self) -> Option<f64> {
        self.parse().ok()
    }

    fn to_string_value(&self) -> Option<String> {
        Some((*self).to_string())
    }
}

/// Coerce value to bool.
pub fn to_bool<T: Coercible>(value: &T) -> Result<bool> {
    value
        .to_bool()
        .ok_or_else(|| CoerceError::CannotCoerce("value".into(), "bool".into()))
}

/// Coerce value to int.
pub fn to_int<T: Coercible>(value: &T) -> Result<i64> {
    value
        .to_int()
        .ok_or_else(|| CoerceError::CannotCoerce("value".into(), "int".into()))
}

/// Coerce value to float.
pub fn to_float<T: Coercible>(value: &T) -> Result<f64> {
    value
        .to_float()
        .ok_or_else(|| CoerceError::CannotCoerce("value".into(), "float".into()))
}

/// Coerce value to string.
pub fn to_string<T: Coercible>(value: &T) -> Result<String> {
    value
        .to_string_value()
        .ok_or_else(|| CoerceError::CannotCoerce("value".into(), "string".into()))
}

/// Type coercion priority (higher = preferred).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TypePriority {
    Bool = 1,
    Int = 2,
    Float = 3,
    String = 4,
}

/// Determine common type for two values.
pub fn common_type(a: TypePriority, b: TypePriority) -> TypePriority {
    a.max(b)
}

/// Dynamic value that can be coerced.
#[derive(Debug, Clone)]
pub enum DynamicValue {
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    Null,
}

impl DynamicValue {
    /// Get type priority.
    pub fn type_priority(&self) -> Option<TypePriority> {
        match self {
            DynamicValue::Bool(_) => Some(TypePriority::Bool),
            DynamicValue::Int(_) => Some(TypePriority::Int),
            DynamicValue::Float(_) => Some(TypePriority::Float),
            DynamicValue::String(_) => Some(TypePriority::String),
            DynamicValue::Null => None,
        }
    }

    /// Coerce to bool.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            DynamicValue::Bool(v) => Some(*v),
            DynamicValue::Int(v) => Some(*v != 0),
            DynamicValue::Float(v) => Some(*v != 0.0),
            DynamicValue::String(v) => v.to_bool(),
            DynamicValue::Null => Some(false),
        }
    }

    /// Coerce to int.
    pub fn as_int(&self) -> Option<i64> {
        match self {
            DynamicValue::Bool(v) => Some(if *v { 1 } else { 0 }),
            DynamicValue::Int(v) => Some(*v),
            DynamicValue::Float(v) => Some(*v as i64),
            DynamicValue::String(v) => v.parse().ok(),
            DynamicValue::Null => None,
        }
    }

    /// Coerce to float.
    pub fn as_float(&self) -> Option<f64> {
        match self {
            DynamicValue::Bool(v) => Some(if *v { 1.0 } else { 0.0 }),
            DynamicValue::Int(v) => Some(*v as f64),
            DynamicValue::Float(v) => Some(*v),
            DynamicValue::String(v) => v.parse().ok(),
            DynamicValue::Null => None,
        }
    }

    /// Coerce to string.
    pub fn as_string(&self) -> String {
        match self {
            DynamicValue::Bool(v) => v.to_string(),
            DynamicValue::Int(v) => v.to_string(),
            DynamicValue::Float(v) => v.to_string(),
            DynamicValue::String(v) => v.clone(),
            DynamicValue::Null => "null".to_string(),
        }
    }

    /// Check if null.
    pub fn is_null(&self) -> bool {
        matches!(self, DynamicValue::Null)
    }

    /// Check if truthy.
    pub fn is_truthy(&self) -> bool {
        self.as_bool().unwrap_or(false)
    }
}

impl From<bool> for DynamicValue {
    fn from(v: bool) -> Self {
        DynamicValue::Bool(v)
    }
}

impl From<i64> for DynamicValue {
    fn from(v: i64) -> Self {
        DynamicValue::Int(v)
    }
}

impl From<i32> for DynamicValue {
    fn from(v: i32) -> Self {
        DynamicValue::Int(v as i64)
    }
}

impl From<f64> for DynamicValue {
    fn from(v: f64) -> Self {
        DynamicValue::Float(v)
    }
}

impl From<String> for DynamicValue {
    fn from(v: String) -> Self {
        DynamicValue::String(v)
    }
}

impl From<&str> for DynamicValue {
    fn from(v: &str) -> Self {
        DynamicValue::String(v.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bool_coercion() {
        assert_eq!(to_bool(&true).unwrap(), true);
        assert_eq!(to_bool(&1i64).unwrap(), true);
        assert_eq!(to_bool(&0i64).unwrap(), false);
        assert_eq!(to_bool(&"yes").unwrap(), true);
        assert_eq!(to_bool(&"false").unwrap(), false);
    }

    #[test]
    fn test_int_coercion() {
        assert_eq!(to_int(&true).unwrap(), 1);
        assert_eq!(to_int(&42i64).unwrap(), 42);
        assert_eq!(to_int(&3.14f64).unwrap(), 3);
        assert_eq!(to_int(&"123").unwrap(), 123);
    }

    #[test]
    fn test_float_coercion() {
        assert_eq!(to_float(&true).unwrap(), 1.0);
        assert_eq!(to_float(&42i64).unwrap(), 42.0);
        assert_eq!(to_float(&3.14f64).unwrap(), 3.14);
    }

    #[test]
    fn test_dynamic_value() {
        let v = DynamicValue::from(42i32);
        assert_eq!(v.as_bool(), Some(true));
        assert_eq!(v.as_int(), Some(42));
        assert_eq!(v.as_float(), Some(42.0));
        assert_eq!(v.as_string(), "42");
    }

    #[test]
    fn test_truthy() {
        assert!(DynamicValue::from(1i32).is_truthy());
        assert!(!DynamicValue::from(0i32).is_truthy());
        assert!(!DynamicValue::Null.is_truthy());
    }
}
