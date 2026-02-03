//! Configuration merging utilities for drbot.
//!
//! This crate provides:
//! - Deep merge strategies
//! - Override handling
//! - Layered configuration

use std::collections::HashMap;
use thiserror::Error;

/// Merge error types.
#[derive(Error, Debug, Clone)]
pub enum MergeError {
    #[error("Type mismatch at {path}: cannot merge {from} into {to}")]
    TypeMismatch {
        path: String,
        from: String,
        to: String,
    },

    #[error("Merge conflict at {path}")]
    Conflict { path: String },
}

/// Result type for merge operations.
pub type Result<T> = std::result::Result<T, MergeError>;

/// Merge strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeStrategy {
    /// Replace entire value.
    Replace,
    /// Deep merge objects, replace primitives.
    Deep,
    /// Append arrays instead of replacing.
    Append,
    /// Keep original value (don't override).
    Keep,
}

impl Default for MergeStrategy {
    fn default() -> Self {
        Self::Deep
    }
}

/// Configuration value for merging.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Integer(i64),
    Float(f64),
    String(String),
    Array(Vec<Value>),
    Object(HashMap<String, Value>),
}

impl Value {
    /// Get type name.
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Null => "null",
            Self::Bool(_) => "bool",
            Self::Integer(_) => "integer",
            Self::Float(_) => "float",
            Self::String(_) => "string",
            Self::Array(_) => "array",
            Self::Object(_) => "object",
        }
    }

    /// Check if value is object.
    pub fn is_object(&self) -> bool {
        matches!(self, Self::Object(_))
    }

    /// Check if value is array.
    pub fn is_array(&self) -> bool {
        matches!(self, Self::Array(_))
    }

    /// Merge another value into this one.
    pub fn merge(&mut self, other: Value, strategy: MergeStrategy) -> Result<()> {
        self.merge_at("", other, strategy)
    }

    fn merge_at(&mut self, path: &str, other: Value, strategy: MergeStrategy) -> Result<()> {
        match strategy {
            MergeStrategy::Replace => {
                *self = other;
                Ok(())
            }
            MergeStrategy::Keep => {
                // Don't change anything
                Ok(())
            }
            MergeStrategy::Deep => self.deep_merge_at(path, other),
            MergeStrategy::Append => self.append_merge_at(path, other),
        }
    }

    fn deep_merge_at(&mut self, path: &str, other: Value) -> Result<()> {
        match (self, other) {
            // Merge objects recursively
            (Value::Object(base), Value::Object(overlay)) => {
                for (key, value) in overlay {
                    let child_path = if path.is_empty() {
                        key.clone()
                    } else {
                        format!("{}.{}", path, key)
                    };

                    if let Some(existing) = base.get_mut(&key) {
                        existing.deep_merge_at(&child_path, value)?;
                    } else {
                        base.insert(key, value);
                    }
                }
                Ok(())
            }
            // Replace other types
            (base, other) => {
                *base = other;
                Ok(())
            }
        }
    }

    fn append_merge_at(&mut self, path: &str, other: Value) -> Result<()> {
        match (self, other) {
            // Append arrays
            (Value::Array(base), Value::Array(overlay)) => {
                base.extend(overlay);
                Ok(())
            }
            // Merge objects recursively with append
            (Value::Object(base), Value::Object(overlay)) => {
                for (key, value) in overlay {
                    let child_path = if path.is_empty() {
                        key.clone()
                    } else {
                        format!("{}.{}", path, key)
                    };

                    if let Some(existing) = base.get_mut(&key) {
                        existing.append_merge_at(&child_path, value)?;
                    } else {
                        base.insert(key, value);
                    }
                }
                Ok(())
            }
            // Replace other types
            (base, other) => {
                *base = other;
                Ok(())
            }
        }
    }
}

/// Layered configuration.
pub struct LayeredConfig {
    layers: Vec<(String, Value)>,
    strategy: MergeStrategy,
}

impl LayeredConfig {
    /// Create new layered config.
    pub fn new() -> Self {
        Self {
            layers: Vec::new(),
            strategy: MergeStrategy::Deep,
        }
    }

    /// Set merge strategy.
    pub fn strategy(mut self, strategy: MergeStrategy) -> Self {
        self.strategy = strategy;
        self
    }

    /// Add layer.
    pub fn add_layer<S: Into<String>>(&mut self, name: S, config: Value) {
        self.layers.push((name.into(), config));
    }

    /// Add layer with builder pattern.
    pub fn layer<S: Into<String>>(mut self, name: S, config: Value) -> Self {
        self.add_layer(name, config);
        self
    }

    /// Merge all layers.
    pub fn merge(&self) -> Result<Value> {
        let mut result = Value::Object(HashMap::new());

        for (_, layer) in &self.layers {
            result.merge(layer.clone(), self.strategy)?;
        }

        Ok(result)
    }

    /// Get layer names.
    pub fn layer_names(&self) -> Vec<&str> {
        self.layers.iter().map(|(name, _)| name.as_str()).collect()
    }
}

impl Default for LayeredConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// Merge two HashMaps.
pub fn merge_maps<K, V>(base: &mut HashMap<K, V>, overlay: HashMap<K, V>)
where
    K: std::hash::Hash + Eq,
{
    for (key, value) in overlay {
        base.insert(key, value);
    }
}

/// Deep merge two HashMaps of Values.
pub fn deep_merge_maps(
    base: &mut HashMap<String, Value>,
    overlay: HashMap<String, Value>,
) -> Result<()> {
    for (key, value) in overlay {
        if let Some(existing) = base.get_mut(&key) {
            existing.merge(value, MergeStrategy::Deep)?;
        } else {
            base.insert(key, value);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_object(pairs: Vec<(&str, Value)>) -> Value {
        let mut map = HashMap::new();
        for (k, v) in pairs {
            map.insert(k.to_string(), v);
        }
        Value::Object(map)
    }

    #[test]
    fn test_replace_merge() {
        let mut base = Value::Integer(1);
        base.merge(Value::Integer(2), MergeStrategy::Replace)
            .unwrap();
        assert_eq!(base, Value::Integer(2));
    }

    #[test]
    fn test_keep_merge() {
        let mut base = Value::Integer(1);
        base.merge(Value::Integer(2), MergeStrategy::Keep).unwrap();
        assert_eq!(base, Value::Integer(1));
    }

    #[test]
    fn test_deep_merge_objects() {
        let mut base = make_object(vec![("a", Value::Integer(1)), ("b", Value::Integer(2))]);

        let overlay = make_object(vec![("b", Value::Integer(20)), ("c", Value::Integer(3))]);

        base.merge(overlay, MergeStrategy::Deep).unwrap();

        if let Value::Object(map) = base {
            assert_eq!(map.get("a"), Some(&Value::Integer(1)));
            assert_eq!(map.get("b"), Some(&Value::Integer(20)));
            assert_eq!(map.get("c"), Some(&Value::Integer(3)));
        } else {
            panic!("Expected object");
        }
    }

    #[test]
    fn test_append_merge_arrays() {
        let mut base = Value::Array(vec![Value::Integer(1), Value::Integer(2)]);
        let overlay = Value::Array(vec![Value::Integer(3), Value::Integer(4)]);

        base.merge(overlay, MergeStrategy::Append).unwrap();

        if let Value::Array(arr) = base {
            assert_eq!(arr.len(), 4);
        } else {
            panic!("Expected array");
        }
    }

    #[test]
    fn test_layered_config() {
        let config = LayeredConfig::new()
            .layer(
                "defaults",
                make_object(vec![
                    ("port", Value::Integer(8080)),
                    ("debug", Value::Bool(false)),
                ]),
            )
            .layer("overrides", make_object(vec![("debug", Value::Bool(true))]));

        let merged = config.merge().unwrap();

        if let Value::Object(map) = merged {
            assert_eq!(map.get("port"), Some(&Value::Integer(8080)));
            assert_eq!(map.get("debug"), Some(&Value::Bool(true)));
        } else {
            panic!("Expected object");
        }
    }
}
