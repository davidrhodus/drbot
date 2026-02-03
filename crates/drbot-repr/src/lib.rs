//! Representation utilities for drbot.
//!
//! This crate provides:
//! - Value representation
//! - Type representation
//! - Structural representation

use std::any::type_name;
use std::fmt::Debug;
use thiserror::Error;

/// Repr error types.
#[derive(Error, Debug, Clone)]
pub enum ReprError {
    #[error("Cannot represent: {0}")]
    CannotRepresent(String),
}

/// Result type for repr operations.
pub type Result<T> = std::result::Result<T, ReprError>;

/// Value representation.
#[derive(Debug, Clone)]
pub enum Repr {
    /// Null/None.
    Null,
    /// Boolean.
    Bool(bool),
    /// Integer.
    Int(i64),
    /// Unsigned integer.
    UInt(u64),
    /// Float.
    Float(f64),
    /// String.
    String(String),
    /// Bytes.
    Bytes(Vec<u8>),
    /// Array.
    Array(Vec<Repr>),
    /// Map.
    Map(Vec<(String, Repr)>),
    /// Struct.
    Struct {
        name: String,
        fields: Vec<(String, Repr)>,
    },
    /// Enum variant.
    Enum {
        name: String,
        variant: String,
        value: Option<Box<Repr>>,
    },
    /// Tuple.
    Tuple(Vec<Repr>),
    /// Custom.
    Custom(String),
}

impl Repr {
    /// Create null.
    pub fn null() -> Self {
        Self::Null
    }

    /// Create bool.
    pub fn bool(v: bool) -> Self {
        Self::Bool(v)
    }

    /// Create int.
    pub fn int(v: i64) -> Self {
        Self::Int(v)
    }

    /// Create uint.
    pub fn uint(v: u64) -> Self {
        Self::UInt(v)
    }

    /// Create float.
    pub fn float(v: f64) -> Self {
        Self::Float(v)
    }

    /// Create string.
    pub fn string(v: impl Into<String>) -> Self {
        Self::String(v.into())
    }

    /// Create bytes.
    pub fn bytes(v: Vec<u8>) -> Self {
        Self::Bytes(v)
    }

    /// Create array.
    pub fn array(items: Vec<Repr>) -> Self {
        Self::Array(items)
    }

    /// Create struct.
    pub fn r#struct(name: &str, fields: Vec<(&str, Repr)>) -> Self {
        Self::Struct {
            name: name.to_string(),
            fields: fields
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect(),
        }
    }

    /// Create enum.
    pub fn r#enum(name: &str, variant: &str, value: Option<Repr>) -> Self {
        Self::Enum {
            name: name.to_string(),
            variant: variant.to_string(),
            value: value.map(Box::new),
        }
    }

    /// Create tuple.
    pub fn tuple(items: Vec<Repr>) -> Self {
        Self::Tuple(items)
    }

    /// Check if null.
    pub fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }

    /// Get type name.
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Null => "null",
            Self::Bool(_) => "bool",
            Self::Int(_) => "int",
            Self::UInt(_) => "uint",
            Self::Float(_) => "float",
            Self::String(_) => "string",
            Self::Bytes(_) => "bytes",
            Self::Array(_) => "array",
            Self::Map(_) => "map",
            Self::Struct { .. } => "struct",
            Self::Enum { .. } => "enum",
            Self::Tuple(_) => "tuple",
            Self::Custom(_) => "custom",
        }
    }

    /// Format to string.
    pub fn format(&self) -> String {
        match self {
            Self::Null => "null".to_string(),
            Self::Bool(v) => v.to_string(),
            Self::Int(v) => v.to_string(),
            Self::UInt(v) => v.to_string(),
            Self::Float(v) => v.to_string(),
            Self::String(v) => format!("\"{}\"", v),
            Self::Bytes(v) => format!("[{} bytes]", v.len()),
            Self::Array(items) => {
                let formatted: Vec<_> = items.iter().map(|i| i.format()).collect();
                format!("[{}]", formatted.join(", "))
            }
            Self::Map(entries) => {
                let formatted: Vec<_> = entries
                    .iter()
                    .map(|(k, v)| format!("{}: {}", k, v.format()))
                    .collect();
                format!("{{{}}}", formatted.join(", "))
            }
            Self::Struct { name, fields } => {
                let formatted: Vec<_> = fields
                    .iter()
                    .map(|(k, v)| format!("{}: {}", k, v.format()))
                    .collect();
                format!("{} {{ {} }}", name, formatted.join(", "))
            }
            Self::Enum {
                name,
                variant,
                value,
            } => match value {
                Some(v) => format!("{}::{}({})", name, variant, v.format()),
                None => format!("{}::{}", name, variant),
            },
            Self::Tuple(items) => {
                let formatted: Vec<_> = items.iter().map(|i| i.format()).collect();
                format!("({})", formatted.join(", "))
            }
            Self::Custom(s) => s.clone(),
        }
    }
}

/// Get type representation.
pub fn type_repr<T>() -> String {
    type_name::<T>().to_string()
}

/// Get short type name.
pub fn type_short<T>() -> String {
    let full = type_name::<T>();
    full.rsplit("::").next().unwrap_or(full).to_string()
}

/// Representable trait.
pub trait Representable {
    /// Get representation.
    fn repr(&self) -> Repr;
}

impl Representable for () {
    fn repr(&self) -> Repr {
        Repr::Null
    }
}

impl Representable for bool {
    fn repr(&self) -> Repr {
        Repr::Bool(*self)
    }
}

impl Representable for i64 {
    fn repr(&self) -> Repr {
        Repr::Int(*self)
    }
}

impl Representable for u64 {
    fn repr(&self) -> Repr {
        Repr::UInt(*self)
    }
}

impl Representable for f64 {
    fn repr(&self) -> Repr {
        Repr::Float(*self)
    }
}

impl Representable for String {
    fn repr(&self) -> Repr {
        Repr::String(self.clone())
    }
}

impl Representable for &str {
    fn repr(&self) -> Repr {
        Repr::String((*self).to_string())
    }
}

impl<T: Representable> Representable for Vec<T> {
    fn repr(&self) -> Repr {
        Repr::Array(self.iter().map(|v| v.repr()).collect())
    }
}

impl<T: Representable> Representable for Option<T> {
    fn repr(&self) -> Repr {
        match self {
            Some(v) => Repr::r#enum("Option", "Some", Some(v.repr())),
            None => Repr::r#enum("Option", "None", None),
        }
    }
}

/// Debug representation.
pub fn debug_repr<T: Debug>(value: &T) -> Repr {
    Repr::Custom(format!("{:?}", value))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_repr() {
        assert_eq!(Repr::null().type_name(), "null");
        assert_eq!(Repr::bool(true).format(), "true");
        assert_eq!(Repr::int(42).format(), "42");
        assert_eq!(Repr::string("hello").format(), "\"hello\"");
    }

    #[test]
    fn test_struct_repr() {
        let r = Repr::r#struct("Point", vec![("x", Repr::int(10)), ("y", Repr::int(20))]);
        let formatted = r.format();
        assert!(formatted.contains("Point"));
        assert!(formatted.contains("x: 10"));
    }

    #[test]
    fn test_type_repr() {
        let r = type_repr::<String>();
        assert!(r.contains("String"));
    }

    #[test]
    fn test_representable() {
        assert_eq!(true.repr().format(), "true");
        assert_eq!(42i64.repr().format(), "42");
        assert_eq!("hello".repr().format(), "\"hello\"");
    }
}
