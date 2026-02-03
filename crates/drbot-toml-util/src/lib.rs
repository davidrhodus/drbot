//! TOML utilities for drbot.
//!
//! This crate provides:
//! - TOML parsing and manipulation
//! - TOML path queries
//! - TOML to/from JSON conversion

use serde::{de::DeserializeOwned, Serialize};
use std::collections::HashMap;
use thiserror::Error;
use toml::Value;

/// TOML error types.
#[derive(Error, Debug)]
pub enum TomlError {
    #[error("Parse error: {0}")]
    Parse(#[from] toml::de::Error),

    #[error("Serialize error: {0}")]
    Serialize(#[from] toml::ser::Error),

    #[error("Path not found: {0}")]
    PathNotFound(String),

    #[error("Type error: expected {expected}, got {actual}")]
    TypeError { expected: String, actual: String },
}

/// Result type for TOML operations.
pub type Result<T> = std::result::Result<T, TomlError>;

/// TOML path query.
pub struct TomlPath;

impl TomlPath {
    /// Get value at path.
    pub fn get<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
        let parts: Vec<&str> = path.split('.').collect();
        let mut current = value;

        for part in parts {
            // Handle array access like "items[0]"
            if let Some(bracket_pos) = part.find('[') {
                let key = &part[..bracket_pos];
                if !key.is_empty() {
                    current = current.get(key)?;
                }

                if let Some(end) = part.find(']') {
                    if let Ok(idx) = part[bracket_pos + 1..end].parse::<usize>() {
                        current = current.get(idx)?;
                    }
                }
            } else {
                current = current.get(part)?;
            }
        }

        Some(current)
    }

    /// Get value at path, returning error if not found.
    pub fn get_required<'a>(value: &'a Value, path: &str) -> Result<&'a Value> {
        Self::get(value, path).ok_or_else(|| TomlError::PathNotFound(path.to_string()))
    }

    /// Get string at path.
    pub fn get_str<'a>(value: &'a Value, path: &str) -> Option<&'a str> {
        Self::get(value, path)?.as_str()
    }

    /// Get i64 at path.
    pub fn get_i64(value: &Value, path: &str) -> Option<i64> {
        Self::get(value, path)?.as_integer()
    }

    /// Get f64 at path.
    pub fn get_f64(value: &Value, path: &str) -> Option<f64> {
        Self::get(value, path)?.as_float()
    }

    /// Get bool at path.
    pub fn get_bool(value: &Value, path: &str) -> Option<bool> {
        Self::get(value, path)?.as_bool()
    }

    /// Get array at path.
    pub fn get_array<'a>(value: &'a Value, path: &str) -> Option<&'a Vec<Value>> {
        Self::get(value, path)?.as_array()
    }

    /// Get table at path.
    pub fn get_table<'a>(
        value: &'a Value,
        path: &str,
    ) -> Option<&'a toml::map::Map<String, Value>> {
        Self::get(value, path)?.as_table()
    }

    /// Check if path exists.
    pub fn exists(value: &Value, path: &str) -> bool {
        Self::get(value, path).is_some()
    }
}

/// TOML utilities.
pub struct Toml;

impl Toml {
    /// Parse TOML string.
    pub fn parse(s: &str) -> Result<Value> {
        Ok(s.parse()?)
    }

    /// Parse TOML into type.
    pub fn parse_as<T: DeserializeOwned>(s: &str) -> Result<T> {
        Ok(toml::from_str(s)?)
    }

    /// Serialize to TOML string.
    pub fn stringify(value: &Value) -> String {
        value.to_string()
    }

    /// Serialize type to TOML string.
    pub fn to_string<T: Serialize>(value: &T) -> Result<String> {
        Ok(toml::to_string(value)?)
    }

    /// Serialize type to pretty TOML string.
    pub fn to_string_pretty<T: Serialize>(value: &T) -> Result<String> {
        Ok(toml::to_string_pretty(value)?)
    }

    /// Merge two TOML tables.
    pub fn merge(base: &mut Value, other: &Value) {
        match (base, other) {
            (Value::Table(base_map), Value::Table(other_map)) => {
                for (key, value) in other_map {
                    if let Some(base_value) = base_map.get_mut(key) {
                        Self::merge(base_value, value);
                    } else {
                        base_map.insert(key.clone(), value.clone());
                    }
                }
            }
            (base, other) => {
                *base = other.clone();
            }
        }
    }

    /// Convert TOML to JSON value.
    pub fn to_json(value: &Value) -> serde_json::Value {
        match value {
            Value::String(s) => serde_json::Value::String(s.clone()),
            Value::Integer(i) => serde_json::Value::Number((*i).into()),
            Value::Float(f) => serde_json::json!(*f),
            Value::Boolean(b) => serde_json::Value::Bool(*b),
            Value::Datetime(dt) => serde_json::Value::String(dt.to_string()),
            Value::Array(arr) => serde_json::Value::Array(arr.iter().map(Self::to_json).collect()),
            Value::Table(map) => {
                let obj: serde_json::Map<String, serde_json::Value> = map
                    .iter()
                    .map(|(k, v)| (k.clone(), Self::to_json(v)))
                    .collect();
                serde_json::Value::Object(obj)
            }
        }
    }

    /// Convert JSON to TOML value.
    pub fn from_json(value: &serde_json::Value) -> Option<Value> {
        match value {
            serde_json::Value::Null => None,
            serde_json::Value::Bool(b) => Some(Value::Boolean(*b)),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Some(Value::Integer(i))
                } else if let Some(f) = n.as_f64() {
                    Some(Value::Float(f))
                } else {
                    None
                }
            }
            serde_json::Value::String(s) => Some(Value::String(s.clone())),
            serde_json::Value::Array(arr) => {
                let toml_arr: Vec<Value> = arr.iter().filter_map(Self::from_json).collect();
                Some(Value::Array(toml_arr))
            }
            serde_json::Value::Object(obj) => {
                let mut map = toml::map::Map::new();
                for (k, v) in obj {
                    if let Some(toml_v) = Self::from_json(v) {
                        map.insert(k.clone(), toml_v);
                    }
                }
                Some(Value::Table(map))
            }
        }
    }

    /// Flatten nested TOML to dot-notation paths.
    pub fn flatten(value: &Value) -> HashMap<String, Value> {
        let mut result = HashMap::new();
        Self::flatten_recursive(value, String::new(), &mut result);
        result
    }

    fn flatten_recursive(value: &Value, path: String, result: &mut HashMap<String, Value>) {
        match value {
            Value::Table(map) => {
                for (key, val) in map {
                    let new_path = if path.is_empty() {
                        key.clone()
                    } else {
                        format!("{}.{}", path, key)
                    };
                    Self::flatten_recursive(val, new_path, result);
                }
            }
            Value::Array(arr) => {
                for (i, val) in arr.iter().enumerate() {
                    let new_path = format!("{}[{}]", path, i);
                    Self::flatten_recursive(val, new_path, result);
                }
            }
            _ => {
                result.insert(path, value.clone());
            }
        }
    }

    /// Get all keys in a TOML table.
    pub fn keys(value: &Value) -> Vec<String> {
        match value {
            Value::Table(map) => map.keys().cloned().collect(),
            _ => Vec::new(),
        }
    }

    /// Get all keys recursively with dot-notation.
    pub fn all_keys(value: &Value) -> Vec<String> {
        Self::flatten(value).keys().cloned().collect()
    }
}

/// TOML table builder.
pub struct TomlBuilder {
    value: toml::map::Map<String, Value>,
}

impl TomlBuilder {
    /// Create new TOML builder.
    pub fn new() -> Self {
        Self {
            value: toml::map::Map::new(),
        }
    }

    /// Set string value.
    pub fn set_str(mut self, key: &str, value: &str) -> Self {
        self.value
            .insert(key.to_string(), Value::String(value.to_string()));
        self
    }

    /// Set integer value.
    pub fn set_i64(mut self, key: &str, value: i64) -> Self {
        self.value.insert(key.to_string(), Value::Integer(value));
        self
    }

    /// Set float value.
    pub fn set_f64(mut self, key: &str, value: f64) -> Self {
        self.value.insert(key.to_string(), Value::Float(value));
        self
    }

    /// Set boolean value.
    pub fn set_bool(mut self, key: &str, value: bool) -> Self {
        self.value.insert(key.to_string(), Value::Boolean(value));
        self
    }

    /// Set array value.
    pub fn set_array(mut self, key: &str, value: Vec<Value>) -> Self {
        self.value.insert(key.to_string(), Value::Array(value));
        self
    }

    /// Set table value.
    pub fn set_table(mut self, key: &str, value: TomlBuilder) -> Self {
        self.value
            .insert(key.to_string(), Value::Table(value.value));
        self
    }

    /// Build the TOML value.
    pub fn build(self) -> Value {
        Value::Table(self.value)
    }

    /// Build and convert to string.
    pub fn to_string(self) -> String {
        self.build().to_string()
    }
}

impl Default for TomlBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_toml_parse() {
        let toml_str = r#"
            name = "test"
            [nested]
            value = 42
        "#;

        let value = Toml::parse(toml_str).unwrap();
        assert_eq!(TomlPath::get_str(&value, "name"), Some("test"));
        assert_eq!(TomlPath::get_i64(&value, "nested.value"), Some(42));
    }

    #[test]
    fn test_toml_path_array() {
        let toml_str = r#"
            [[items]]
            name = "a"
            [[items]]
            name = "b"
        "#;

        let value = Toml::parse(toml_str).unwrap();
        assert_eq!(TomlPath::get_str(&value, "items[0].name"), Some("a"));
        assert_eq!(TomlPath::get_str(&value, "items[1].name"), Some("b"));
    }

    #[test]
    fn test_toml_merge() {
        let mut base = Toml::parse(
            r#"
            a = 1
            [b]
            c = 2
        "#,
        )
        .unwrap();

        let other = Toml::parse(
            r#"
            [b]
            d = 3
            e = 4
        "#,
        )
        .unwrap();

        Toml::merge(&mut base, &other);

        assert_eq!(TomlPath::get_i64(&base, "a"), Some(1));
        assert_eq!(TomlPath::get_i64(&base, "b.c"), Some(2));
        assert_eq!(TomlPath::get_i64(&base, "b.d"), Some(3));
        assert_eq!(TomlPath::get_i64(&base, "b.e"), Some(4));
    }

    #[test]
    fn test_toml_to_json() {
        let toml = Toml::parse(
            r#"
            name = "test"
            count = 42
            enabled = true
        "#,
        )
        .unwrap();

        let json = Toml::to_json(&toml);

        assert_eq!(json["name"], "test");
        assert_eq!(json["count"], 42);
        assert_eq!(json["enabled"], true);
    }

    #[test]
    fn test_toml_builder() {
        let toml = TomlBuilder::new()
            .set_str("name", "test")
            .set_i64("count", 42)
            .set_bool("enabled", true)
            .set_table("nested", TomlBuilder::new().set_str("inner", "value"))
            .build();

        assert_eq!(TomlPath::get_str(&toml, "name"), Some("test"));
        assert_eq!(TomlPath::get_i64(&toml, "count"), Some(42));
        assert_eq!(TomlPath::get_bool(&toml, "enabled"), Some(true));
        assert_eq!(TomlPath::get_str(&toml, "nested.inner"), Some("value"));
    }

    #[test]
    fn test_toml_flatten() {
        let toml = Toml::parse(
            r#"
            a = 1
            [b]
            c = 2
            d = 3
        "#,
        )
        .unwrap();

        let flat = Toml::flatten(&toml);

        assert_eq!(flat.get("a"), Some(&Value::Integer(1)));
        assert_eq!(flat.get("b.c"), Some(&Value::Integer(2)));
        assert_eq!(flat.get("b.d"), Some(&Value::Integer(3)));
    }

    #[test]
    fn test_toml_keys() {
        let toml = Toml::parse(
            r#"
            a = 1
            b = 2
            [c]
            d = 3
        "#,
        )
        .unwrap();

        let keys = Toml::keys(&toml);
        assert!(keys.contains(&"a".to_string()));
        assert!(keys.contains(&"b".to_string()));
        assert!(keys.contains(&"c".to_string()));
    }
}
