//! JSON utilities for drbot.
//!
//! This crate provides:
//! - JSON path queries
//! - JSON manipulation
//! - JSON diff and merge

use serde::{de::DeserializeOwned, Serialize};
use serde_json::{Map, Value};
use std::collections::HashMap;
use thiserror::Error;

/// JSON error types.
#[derive(Error, Debug)]
pub enum JsonError {
    #[error("Parse error: {0}")]
    Parse(#[from] serde_json::Error),

    #[error("Path not found: {0}")]
    PathNotFound(String),

    #[error("Type error: expected {expected}, got {actual}")]
    TypeError { expected: String, actual: String },

    #[error("Invalid path: {0}")]
    InvalidPath(String),
}

/// Result type for JSON operations.
pub type Result<T> = std::result::Result<T, JsonError>;

/// JSON path query.
pub struct JsonPath;

impl JsonPath {
    /// Get value at path.
    pub fn get<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
        let parts = Self::parse_path(path);
        let mut current = value;

        for part in parts {
            current = match part {
                PathPart::Key(key) => current.get(key)?,
                PathPart::Index(idx) => current.get(idx)?,
            };
        }

        Some(current)
    }

    /// Get value at path, returning error if not found.
    pub fn get_required<'a>(value: &'a Value, path: &str) -> Result<&'a Value> {
        Self::get(value, path).ok_or_else(|| JsonError::PathNotFound(path.to_string()))
    }

    /// Get string at path.
    pub fn get_str<'a>(value: &'a Value, path: &str) -> Option<&'a str> {
        Self::get(value, path)?.as_str()
    }

    /// Get i64 at path.
    pub fn get_i64(value: &Value, path: &str) -> Option<i64> {
        Self::get(value, path)?.as_i64()
    }

    /// Get f64 at path.
    pub fn get_f64(value: &Value, path: &str) -> Option<f64> {
        Self::get(value, path)?.as_f64()
    }

    /// Get bool at path.
    pub fn get_bool(value: &Value, path: &str) -> Option<bool> {
        Self::get(value, path)?.as_bool()
    }

    /// Get array at path.
    pub fn get_array<'a>(value: &'a Value, path: &str) -> Option<&'a Vec<Value>> {
        Self::get(value, path)?.as_array()
    }

    /// Get object at path.
    pub fn get_object<'a>(value: &'a Value, path: &str) -> Option<&'a Map<String, Value>> {
        Self::get(value, path)?.as_object()
    }

    /// Set value at path.
    pub fn set(value: &mut Value, path: &str, new_value: Value) -> Result<()> {
        let parts = Self::parse_path(path);
        if parts.is_empty() {
            return Err(JsonError::InvalidPath(path.to_string()));
        }

        let mut current = value;

        for (i, part) in parts.iter().enumerate() {
            if i == parts.len() - 1 {
                // Last part - set the value
                match part {
                    PathPart::Key(key) => {
                        if let Some(obj) = current.as_object_mut() {
                            obj.insert(key.clone(), new_value);
                            return Ok(());
                        }
                    }
                    PathPart::Index(idx) => {
                        if let Some(arr) = current.as_array_mut() {
                            if *idx < arr.len() {
                                arr[*idx] = new_value;
                                return Ok(());
                            }
                        }
                    }
                }
                return Err(JsonError::PathNotFound(path.to_string()));
            }

            // Navigate to next level
            current = match part {
                PathPart::Key(key) => current
                    .get_mut(key)
                    .ok_or_else(|| JsonError::PathNotFound(path.to_string()))?,
                PathPart::Index(idx) => current
                    .get_mut(*idx)
                    .ok_or_else(|| JsonError::PathNotFound(path.to_string()))?,
            };
        }

        Ok(())
    }

    /// Delete value at path.
    pub fn delete(value: &mut Value, path: &str) -> Result<Option<Value>> {
        let parts = Self::parse_path(path);
        if parts.is_empty() {
            return Err(JsonError::InvalidPath(path.to_string()));
        }

        let mut current = value;

        for (i, part) in parts.iter().enumerate() {
            if i == parts.len() - 1 {
                // Last part - delete the value
                return match part {
                    PathPart::Key(key) => {
                        if let Some(obj) = current.as_object_mut() {
                            Ok(obj.remove(key))
                        } else {
                            Ok(None)
                        }
                    }
                    PathPart::Index(idx) => {
                        if let Some(arr) = current.as_array_mut() {
                            if *idx < arr.len() {
                                Ok(Some(arr.remove(*idx)))
                            } else {
                                Ok(None)
                            }
                        } else {
                            Ok(None)
                        }
                    }
                };
            }

            current = match part {
                PathPart::Key(key) => current
                    .get_mut(key)
                    .ok_or_else(|| JsonError::PathNotFound(path.to_string()))?,
                PathPart::Index(idx) => current
                    .get_mut(*idx)
                    .ok_or_else(|| JsonError::PathNotFound(path.to_string()))?,
            };
        }

        Ok(None)
    }

    /// Check if path exists.
    pub fn exists(value: &Value, path: &str) -> bool {
        Self::get(value, path).is_some()
    }

    fn parse_path(path: &str) -> Vec<PathPart> {
        let mut parts = Vec::new();
        let path = path.trim_start_matches('$').trim_start_matches('.');

        if path.is_empty() {
            return parts;
        }

        for segment in path.split('.') {
            if segment.contains('[') {
                // Handle array access like "items[0]"
                let bracket_pos = segment.find('[').unwrap();
                if bracket_pos > 0 {
                    parts.push(PathPart::Key(segment[..bracket_pos].to_string()));
                }

                // Extract index
                if let Some(end) = segment.find(']') {
                    if let Ok(idx) = segment[bracket_pos + 1..end].parse() {
                        parts.push(PathPart::Index(idx));
                    }
                }
            } else {
                parts.push(PathPart::Key(segment.to_string()));
            }
        }

        parts
    }
}

#[derive(Debug, Clone)]
enum PathPart {
    Key(String),
    Index(usize),
}

/// JSON builder.
pub struct JsonBuilder {
    value: Value,
}

impl JsonBuilder {
    /// Create new object builder.
    pub fn object() -> Self {
        Self {
            value: Value::Object(Map::new()),
        }
    }

    /// Create new array builder.
    pub fn array() -> Self {
        Self {
            value: Value::Array(Vec::new()),
        }
    }

    /// Set field on object.
    pub fn set(mut self, key: &str, value: impl Into<Value>) -> Self {
        if let Some(obj) = self.value.as_object_mut() {
            obj.insert(key.to_string(), value.into());
        }
        self
    }

    /// Set field if value is Some.
    pub fn set_opt<V: Into<Value>>(self, key: &str, value: Option<V>) -> Self {
        if let Some(v) = value {
            self.set(key, v)
        } else {
            self
        }
    }

    /// Push value to array.
    pub fn push(mut self, value: impl Into<Value>) -> Self {
        if let Some(arr) = self.value.as_array_mut() {
            arr.push(value.into());
        }
        self
    }

    /// Build the JSON value.
    pub fn build(self) -> Value {
        self.value
    }
}

impl From<JsonBuilder> for Value {
    fn from(builder: JsonBuilder) -> Self {
        builder.value
    }
}

/// JSON utilities.
pub struct Json;

impl Json {
    /// Parse JSON string.
    pub fn parse(s: &str) -> Result<Value> {
        Ok(serde_json::from_str(s)?)
    }

    /// Parse JSON into type.
    pub fn parse_as<T: DeserializeOwned>(s: &str) -> Result<T> {
        Ok(serde_json::from_str(s)?)
    }

    /// Serialize to JSON string.
    pub fn stringify(value: &Value) -> String {
        value.to_string()
    }

    /// Serialize to pretty JSON string.
    pub fn stringify_pretty(value: &Value) -> String {
        serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
    }

    /// Serialize type to JSON string.
    pub fn to_string<T: Serialize>(value: &T) -> Result<String> {
        Ok(serde_json::to_string(value)?)
    }

    /// Serialize type to pretty JSON string.
    pub fn to_string_pretty<T: Serialize>(value: &T) -> Result<String> {
        Ok(serde_json::to_string_pretty(value)?)
    }

    /// Serialize type to JSON value.
    pub fn to_value<T: Serialize>(value: &T) -> Result<Value> {
        Ok(serde_json::to_value(value)?)
    }

    /// Deep merge two JSON objects.
    pub fn merge(base: &mut Value, other: &Value) {
        match (base, other) {
            (Value::Object(base_map), Value::Object(other_map)) => {
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

    /// Diff two JSON values.
    pub fn diff(a: &Value, b: &Value) -> Vec<JsonDiff> {
        let mut diffs = Vec::new();
        Self::diff_recursive(a, b, String::new(), &mut diffs);
        diffs
    }

    fn diff_recursive(a: &Value, b: &Value, path: String, diffs: &mut Vec<JsonDiff>) {
        if a == b {
            return;
        }

        match (a, b) {
            (Value::Object(obj_a), Value::Object(obj_b)) => {
                // Check for removed/changed keys
                for (key, val_a) in obj_a {
                    let new_path = if path.is_empty() {
                        key.clone()
                    } else {
                        format!("{}.{}", path, key)
                    };

                    if let Some(val_b) = obj_b.get(key) {
                        Self::diff_recursive(val_a, val_b, new_path, diffs);
                    } else {
                        diffs.push(JsonDiff::Removed {
                            path: new_path,
                            value: val_a.clone(),
                        });
                    }
                }

                // Check for added keys
                for (key, val_b) in obj_b {
                    if !obj_a.contains_key(key) {
                        let new_path = if path.is_empty() {
                            key.clone()
                        } else {
                            format!("{}.{}", path, key)
                        };
                        diffs.push(JsonDiff::Added {
                            path: new_path,
                            value: val_b.clone(),
                        });
                    }
                }
            }
            (Value::Array(arr_a), Value::Array(arr_b)) => {
                let max_len = arr_a.len().max(arr_b.len());
                for i in 0..max_len {
                    let new_path = format!("{}[{}]", path, i);
                    match (arr_a.get(i), arr_b.get(i)) {
                        (Some(val_a), Some(val_b)) => {
                            Self::diff_recursive(val_a, val_b, new_path, diffs);
                        }
                        (Some(val_a), None) => {
                            diffs.push(JsonDiff::Removed {
                                path: new_path,
                                value: val_a.clone(),
                            });
                        }
                        (None, Some(val_b)) => {
                            diffs.push(JsonDiff::Added {
                                path: new_path,
                                value: val_b.clone(),
                            });
                        }
                        (None, None) => unreachable!(),
                    }
                }
            }
            _ => {
                diffs.push(JsonDiff::Changed {
                    path,
                    old: a.clone(),
                    new: b.clone(),
                });
            }
        }
    }

    /// Flatten nested JSON to dot-notation paths.
    pub fn flatten(value: &Value) -> HashMap<String, Value> {
        let mut result = HashMap::new();
        Self::flatten_recursive(value, String::new(), &mut result);
        result
    }

    fn flatten_recursive(value: &Value, path: String, result: &mut HashMap<String, Value>) {
        match value {
            Value::Object(obj) => {
                for (key, val) in obj {
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

    /// Unflatten dot-notation paths to nested JSON.
    pub fn unflatten(flat: &HashMap<String, Value>) -> Value {
        let mut result = Value::Object(Map::new());

        for (path, value) in flat {
            Self::set_nested(&mut result, path, value.clone());
        }

        result
    }

    fn set_nested(root: &mut Value, path: &str, value: Value) {
        let parts = JsonPath::parse_path(path);
        let mut current = root;

        for (i, part) in parts.iter().enumerate() {
            if i == parts.len() - 1 {
                match part {
                    PathPart::Key(key) => {
                        if let Some(obj) = current.as_object_mut() {
                            obj.insert(key.clone(), value);
                        }
                    }
                    PathPart::Index(idx) => {
                        if let Some(arr) = current.as_array_mut() {
                            while arr.len() <= *idx {
                                arr.push(Value::Null);
                            }
                            arr[*idx] = value;
                        }
                    }
                }
                return;
            }

            let next_is_array = matches!(parts.get(i + 1), Some(PathPart::Index(_)));

            current = match part {
                PathPart::Key(key) => {
                    if let Some(obj) = current.as_object_mut() {
                        if !obj.contains_key(key) {
                            let new_value = if next_is_array {
                                Value::Array(Vec::new())
                            } else {
                                Value::Object(Map::new())
                            };
                            obj.insert(key.clone(), new_value);
                        }
                        obj.get_mut(key).unwrap()
                    } else {
                        return;
                    }
                }
                PathPart::Index(idx) => {
                    if let Some(arr) = current.as_array_mut() {
                        while arr.len() <= *idx {
                            let new_value = if next_is_array {
                                Value::Array(Vec::new())
                            } else {
                                Value::Object(Map::new())
                            };
                            arr.push(new_value);
                        }
                        &mut arr[*idx]
                    } else {
                        return;
                    }
                }
            };
        }
    }
}

/// JSON diff entry.
#[derive(Debug, Clone, PartialEq)]
pub enum JsonDiff {
    Added {
        path: String,
        value: Value,
    },
    Removed {
        path: String,
        value: Value,
    },
    Changed {
        path: String,
        old: Value,
        new: Value,
    },
}

impl JsonDiff {
    /// Get the path.
    pub fn path(&self) -> &str {
        match self {
            JsonDiff::Added { path, .. } => path,
            JsonDiff::Removed { path, .. } => path,
            JsonDiff::Changed { path, .. } => path,
        }
    }
}

// Implement Into<Value> for common types
impl From<&str> for JsonBuilder {
    fn from(_: &str) -> Self {
        JsonBuilder::object()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_path_get() {
        let json: Value = serde_json::json!({
            "name": "test",
            "nested": {
                "value": 42
            },
            "items": [1, 2, 3]
        });

        assert_eq!(JsonPath::get_str(&json, "name"), Some("test"));
        assert_eq!(JsonPath::get_i64(&json, "nested.value"), Some(42));
        assert_eq!(JsonPath::get_i64(&json, "items[0]"), Some(1));
        assert_eq!(JsonPath::get_i64(&json, "items[2]"), Some(3));
        assert!(JsonPath::get(&json, "nonexistent").is_none());
    }

    #[test]
    fn test_json_path_set() {
        let mut json: Value = serde_json::json!({
            "name": "test",
            "nested": {
                "value": 42
            }
        });

        JsonPath::set(&mut json, "name", Value::String("updated".to_string())).unwrap();
        assert_eq!(JsonPath::get_str(&json, "name"), Some("updated"));

        JsonPath::set(&mut json, "nested.value", Value::Number(100.into())).unwrap();
        assert_eq!(JsonPath::get_i64(&json, "nested.value"), Some(100));
    }

    #[test]
    fn test_json_path_delete() {
        let mut json: Value = serde_json::json!({
            "name": "test",
            "remove": "me"
        });

        let removed = JsonPath::delete(&mut json, "remove").unwrap();
        assert_eq!(removed, Some(Value::String("me".to_string())));
        assert!(JsonPath::get(&json, "remove").is_none());
    }

    #[test]
    fn test_json_builder() {
        let json = JsonBuilder::object()
            .set("name", "test")
            .set("count", 42)
            .set_opt("optional", Some("value"))
            .set_opt::<String>("missing", None)
            .build();

        assert_eq!(JsonPath::get_str(&json, "name"), Some("test"));
        assert_eq!(JsonPath::get_i64(&json, "count"), Some(42));
        assert_eq!(JsonPath::get_str(&json, "optional"), Some("value"));
        assert!(JsonPath::get(&json, "missing").is_none());
    }

    #[test]
    fn test_json_merge() {
        let mut base: Value = serde_json::json!({
            "a": 1,
            "b": {
                "c": 2
            }
        });

        let other: Value = serde_json::json!({
            "b": {
                "d": 3
            },
            "e": 4
        });

        Json::merge(&mut base, &other);

        assert_eq!(JsonPath::get_i64(&base, "a"), Some(1));
        assert_eq!(JsonPath::get_i64(&base, "b.c"), Some(2));
        assert_eq!(JsonPath::get_i64(&base, "b.d"), Some(3));
        assert_eq!(JsonPath::get_i64(&base, "e"), Some(4));
    }

    #[test]
    fn test_json_diff() {
        let a: Value = serde_json::json!({
            "name": "test",
            "count": 1,
            "removed": "value"
        });

        let b: Value = serde_json::json!({
            "name": "test",
            "count": 2,
            "added": "new"
        });

        let diffs = Json::diff(&a, &b);
        assert_eq!(diffs.len(), 3);

        assert!(diffs
            .iter()
            .any(|d| matches!(d, JsonDiff::Changed { path, .. } if path == "count")));
        assert!(diffs
            .iter()
            .any(|d| matches!(d, JsonDiff::Removed { path, .. } if path == "removed")));
        assert!(diffs
            .iter()
            .any(|d| matches!(d, JsonDiff::Added { path, .. } if path == "added")));
    }

    #[test]
    fn test_json_flatten() {
        let json: Value = serde_json::json!({
            "a": {
                "b": 1,
                "c": {
                    "d": 2
                }
            },
            "e": [1, 2]
        });

        let flat = Json::flatten(&json);

        assert_eq!(flat.get("a.b"), Some(&Value::Number(1.into())));
        assert_eq!(flat.get("a.c.d"), Some(&Value::Number(2.into())));
        assert_eq!(flat.get("e[0]"), Some(&Value::Number(1.into())));
        assert_eq!(flat.get("e[1]"), Some(&Value::Number(2.into())));
    }
}
