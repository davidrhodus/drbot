//! YAML utilities for drbot.
//!
//! This crate provides:
//! - Basic YAML-like structure handling
//! - YAML parsing (simplified)
//! - YAML serialization (simplified)
//!
//! Note: For full YAML support, consider adding serde_yaml as a dependency.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

/// YAML error types.
#[derive(Error, Debug)]
pub enum YamlError {
    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Path not found: {0}")]
    PathNotFound(String),

    #[error("Type error: expected {expected}, got {actual}")]
    TypeError { expected: String, actual: String },
}

/// Result type for YAML operations.
pub type Result<T> = std::result::Result<T, YamlError>;

/// YAML value (simplified representation).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum YamlValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    Array(Vec<YamlValue>),
    Object(HashMap<String, YamlValue>),
}

impl YamlValue {
    /// Create null value.
    pub fn null() -> Self {
        YamlValue::Null
    }

    /// Check if null.
    pub fn is_null(&self) -> bool {
        matches!(self, YamlValue::Null)
    }

    /// Get as bool.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            YamlValue::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// Get as i64.
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            YamlValue::Int(i) => Some(*i),
            _ => None,
        }
    }

    /// Get as f64.
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            YamlValue::Float(f) => Some(*f),
            YamlValue::Int(i) => Some(*i as f64),
            _ => None,
        }
    }

    /// Get as string.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            YamlValue::String(s) => Some(s),
            _ => None,
        }
    }

    /// Get as array.
    pub fn as_array(&self) -> Option<&Vec<YamlValue>> {
        match self {
            YamlValue::Array(arr) => Some(arr),
            _ => None,
        }
    }

    /// Get as object.
    pub fn as_object(&self) -> Option<&HashMap<String, YamlValue>> {
        match self {
            YamlValue::Object(obj) => Some(obj),
            _ => None,
        }
    }

    /// Get value by key.
    pub fn get(&self, key: &str) -> Option<&YamlValue> {
        match self {
            YamlValue::Object(obj) => obj.get(key),
            _ => None,
        }
    }

    /// Get value by index.
    pub fn get_index(&self, index: usize) -> Option<&YamlValue> {
        match self {
            YamlValue::Array(arr) => arr.get(index),
            _ => None,
        }
    }
}

impl Default for YamlValue {
    fn default() -> Self {
        YamlValue::Null
    }
}

impl From<bool> for YamlValue {
    fn from(b: bool) -> Self {
        YamlValue::Bool(b)
    }
}

impl From<i64> for YamlValue {
    fn from(i: i64) -> Self {
        YamlValue::Int(i)
    }
}

impl From<i32> for YamlValue {
    fn from(i: i32) -> Self {
        YamlValue::Int(i as i64)
    }
}

impl From<f64> for YamlValue {
    fn from(f: f64) -> Self {
        YamlValue::Float(f)
    }
}

impl From<&str> for YamlValue {
    fn from(s: &str) -> Self {
        YamlValue::String(s.to_string())
    }
}

impl From<String> for YamlValue {
    fn from(s: String) -> Self {
        YamlValue::String(s)
    }
}

impl<T: Into<YamlValue>> From<Vec<T>> for YamlValue {
    fn from(v: Vec<T>) -> Self {
        YamlValue::Array(v.into_iter().map(|x| x.into()).collect())
    }
}

/// Simple YAML parser for basic structures.
pub struct YamlParser;

impl YamlParser {
    /// Parse simple YAML string.
    pub fn parse(s: &str) -> Result<YamlValue> {
        let s = s.trim();

        if s.is_empty() || s == "null" || s == "~" {
            return Ok(YamlValue::Null);
        }

        // Check for boolean
        if s == "true" || s == "yes" || s == "on" {
            return Ok(YamlValue::Bool(true));
        }
        if s == "false" || s == "no" || s == "off" {
            return Ok(YamlValue::Bool(false));
        }

        // Check for number
        if let Ok(i) = s.parse::<i64>() {
            return Ok(YamlValue::Int(i));
        }
        if let Ok(f) = s.parse::<f64>() {
            return Ok(YamlValue::Float(f));
        }

        // Check for quoted string
        if (s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')) {
            return Ok(YamlValue::String(s[1..s.len() - 1].to_string()));
        }

        // Check for array
        if s.starts_with('[') && s.ends_with(']') {
            return Self::parse_inline_array(&s[1..s.len() - 1]);
        }

        // Check for object
        if s.starts_with('{') && s.ends_with('}') {
            return Self::parse_inline_object(&s[1..s.len() - 1]);
        }

        // Check for block structure
        if s.contains('\n') {
            return Self::parse_block(s);
        }

        // Default to string
        Ok(YamlValue::String(s.to_string()))
    }

    fn parse_inline_array(s: &str) -> Result<YamlValue> {
        let mut items = Vec::new();
        let s = s.trim();

        if s.is_empty() {
            return Ok(YamlValue::Array(items));
        }

        // Simple split by comma (doesn't handle nested structures well)
        for item in s.split(',') {
            items.push(Self::parse(item.trim())?);
        }

        Ok(YamlValue::Array(items))
    }

    fn parse_inline_object(s: &str) -> Result<YamlValue> {
        let mut obj = HashMap::new();
        let s = s.trim();

        if s.is_empty() {
            return Ok(YamlValue::Object(obj));
        }

        // Simple split by comma (doesn't handle nested structures well)
        for pair in s.split(',') {
            let pair = pair.trim();
            if let Some(colon_pos) = pair.find(':') {
                let key = pair[..colon_pos].trim().to_string();
                let value = Self::parse(pair[colon_pos + 1..].trim())?;
                obj.insert(key, value);
            }
        }

        Ok(YamlValue::Object(obj))
    }

    fn parse_block(s: &str) -> Result<YamlValue> {
        let lines: Vec<&str> = s.lines().collect();

        if lines.is_empty() {
            return Ok(YamlValue::Null);
        }

        // Check if it's an array (lines starting with -)
        let first_content_line = lines.iter().find(|l| !l.trim().is_empty());
        if let Some(line) = first_content_line {
            if line.trim().starts_with('-') {
                return Self::parse_block_array(&lines);
            }
        }

        // Otherwise, parse as object
        Self::parse_block_object(&lines)
    }

    fn parse_block_array(lines: &[&str]) -> Result<YamlValue> {
        let mut items = Vec::new();

        for line in lines {
            let trimmed = line.trim();
            if trimmed.starts_with('-') {
                let value = trimmed[1..].trim();
                items.push(Self::parse(value)?);
            }
        }

        Ok(YamlValue::Array(items))
    }

    fn parse_block_object(lines: &[&str]) -> Result<YamlValue> {
        let mut obj = HashMap::new();

        for line in lines {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            if let Some(colon_pos) = trimmed.find(':') {
                let key = trimmed[..colon_pos].trim().to_string();
                let value = trimmed[colon_pos + 1..].trim();

                if value.is_empty() {
                    obj.insert(key, YamlValue::Null);
                } else {
                    obj.insert(key, Self::parse(value)?);
                }
            }
        }

        Ok(YamlValue::Object(obj))
    }
}

/// YAML serializer (simplified).
pub struct YamlSerializer;

impl YamlSerializer {
    /// Serialize to YAML string.
    pub fn serialize(value: &YamlValue) -> String {
        Self::serialize_value(value, 0)
    }

    fn serialize_value(value: &YamlValue, indent: usize) -> String {
        match value {
            YamlValue::Null => "null".to_string(),
            YamlValue::Bool(b) => b.to_string(),
            YamlValue::Int(i) => i.to_string(),
            YamlValue::Float(f) => f.to_string(),
            YamlValue::String(s) => {
                if s.contains('\n') || s.contains(':') || s.contains('#') {
                    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
                } else {
                    s.clone()
                }
            }
            YamlValue::Array(arr) => {
                if arr.is_empty() {
                    "[]".to_string()
                } else {
                    let indent_str = "  ".repeat(indent);
                    let items: Vec<String> = arr
                        .iter()
                        .map(|v| {
                            format!("{}- {}", indent_str, Self::serialize_value(v, indent + 1))
                        })
                        .collect();
                    items.join("\n")
                }
            }
            YamlValue::Object(obj) => {
                if obj.is_empty() {
                    "{}".to_string()
                } else {
                    let indent_str = "  ".repeat(indent);
                    let items: Vec<String> = obj
                        .iter()
                        .map(|(k, v)| {
                            let value_str = Self::serialize_value(v, indent + 1);
                            if matches!(v, YamlValue::Object(_) | YamlValue::Array(_)) {
                                format!("{}{}:\n{}", indent_str, k, value_str)
                            } else {
                                format!("{}{}: {}", indent_str, k, value_str)
                            }
                        })
                        .collect();
                    items.join("\n")
                }
            }
        }
    }
}

/// YAML builder.
pub struct YamlBuilder {
    value: HashMap<String, YamlValue>,
}

impl YamlBuilder {
    /// Create new YAML builder.
    pub fn new() -> Self {
        Self {
            value: HashMap::new(),
        }
    }

    /// Set value.
    pub fn set<V: Into<YamlValue>>(mut self, key: &str, value: V) -> Self {
        self.value.insert(key.to_string(), value.into());
        self
    }

    /// Build the YAML value.
    pub fn build(self) -> YamlValue {
        YamlValue::Object(self.value)
    }

    /// Build and serialize to string.
    pub fn to_string(self) -> String {
        YamlSerializer::serialize(&self.build())
    }
}

impl Default for YamlBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// YAML path query.
pub struct YamlPath;

impl YamlPath {
    /// Get value at path.
    pub fn get<'a>(value: &'a YamlValue, path: &str) -> Option<&'a YamlValue> {
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
                        current = current.get_index(idx)?;
                    }
                }
            } else {
                current = current.get(part)?;
            }
        }

        Some(current)
    }

    /// Get string at path.
    pub fn get_str<'a>(value: &'a YamlValue, path: &str) -> Option<&'a str> {
        Self::get(value, path)?.as_str()
    }

    /// Get i64 at path.
    pub fn get_i64(value: &YamlValue, path: &str) -> Option<i64> {
        Self::get(value, path)?.as_i64()
    }

    /// Get bool at path.
    pub fn get_bool(value: &YamlValue, path: &str) -> Option<bool> {
        Self::get(value, path)?.as_bool()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_primitives() {
        assert_eq!(YamlParser::parse("null").unwrap(), YamlValue::Null);
        assert_eq!(YamlParser::parse("true").unwrap(), YamlValue::Bool(true));
        assert_eq!(YamlParser::parse("false").unwrap(), YamlValue::Bool(false));
        assert_eq!(YamlParser::parse("42").unwrap(), YamlValue::Int(42));
        assert_eq!(YamlParser::parse("3.14").unwrap(), YamlValue::Float(3.14));
        assert_eq!(
            YamlParser::parse("hello").unwrap(),
            YamlValue::String("hello".to_string())
        );
    }

    #[test]
    fn test_parse_inline_array() {
        let result = YamlParser::parse("[1, 2, 3]").unwrap();
        if let YamlValue::Array(arr) = result {
            assert_eq!(arr.len(), 3);
            assert_eq!(arr[0].as_i64(), Some(1));
        } else {
            panic!("Expected array");
        }
    }

    #[test]
    fn test_parse_inline_object() {
        let result = YamlParser::parse("{name: test, count: 42}").unwrap();
        if let YamlValue::Object(obj) = result {
            assert_eq!(obj.get("name").and_then(|v| v.as_str()), Some("test"));
            assert_eq!(obj.get("count").and_then(|v| v.as_i64()), Some(42));
        } else {
            panic!("Expected object");
        }
    }

    #[test]
    fn test_parse_block_object() {
        let yaml = r#"
name: test
count: 42
enabled: true
"#;
        let result = YamlParser::parse(yaml).unwrap();

        assert_eq!(YamlPath::get_str(&result, "name"), Some("test"));
        assert_eq!(YamlPath::get_i64(&result, "count"), Some(42));
        assert_eq!(YamlPath::get_bool(&result, "enabled"), Some(true));
    }

    #[test]
    fn test_parse_block_array() {
        let yaml = r#"
- one
- two
- three
"#;
        let result = YamlParser::parse(yaml).unwrap();

        if let YamlValue::Array(arr) = result {
            assert_eq!(arr.len(), 3);
            assert_eq!(arr[0].as_str(), Some("one"));
        } else {
            panic!("Expected array");
        }
    }

    #[test]
    fn test_yaml_builder() {
        let yaml = YamlBuilder::new()
            .set("name", "test")
            .set("count", 42i64)
            .set("enabled", true)
            .build();

        assert_eq!(YamlPath::get_str(&yaml, "name"), Some("test"));
        assert_eq!(YamlPath::get_i64(&yaml, "count"), Some(42));
        assert_eq!(YamlPath::get_bool(&yaml, "enabled"), Some(true));
    }

    #[test]
    fn test_serialize() {
        let yaml = YamlBuilder::new()
            .set("name", "test")
            .set("count", 42i64)
            .build();

        let s = YamlSerializer::serialize(&yaml);
        assert!(s.contains("name: test"));
        assert!(s.contains("count: 42"));
    }
}
