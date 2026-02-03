//! Configuration parsing utilities for drbot.
//!
//! This crate provides:
//! - Generic config value types
//! - Config parsing helpers
//! - Type conversion utilities

use std::collections::HashMap;
use thiserror::Error;

/// Config parse error types.
#[derive(Error, Debug, Clone)]
pub enum ConfigParseError {
    #[error("Missing key: {0}")]
    MissingKey(String),

    #[error("Type error: expected {expected}, found {found}")]
    TypeError { expected: String, found: String },

    #[error("Invalid value for {key}: {message}")]
    InvalidValue { key: String, message: String },

    #[error("Parse error: {0}")]
    ParseError(String),
}

/// Result type for config parse operations.
pub type Result<T> = std::result::Result<T, ConfigParseError>;

/// Configuration value.
#[derive(Debug, Clone, PartialEq)]
pub enum ConfigValue {
    Null,
    Bool(bool),
    Integer(i64),
    Float(f64),
    String(String),
    Array(Vec<ConfigValue>),
    Object(HashMap<String, ConfigValue>),
}

impl ConfigValue {
    /// Check if null.
    pub fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }

    /// Get as bool.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(b) => Some(*b),
            Self::String(s) => match s.to_lowercase().as_str() {
                "true" | "yes" | "1" | "on" => Some(true),
                "false" | "no" | "0" | "off" => Some(false),
                _ => None,
            },
            Self::Integer(i) => Some(*i != 0),
            _ => None,
        }
    }

    /// Get as integer.
    pub fn as_integer(&self) -> Option<i64> {
        match self {
            Self::Integer(i) => Some(*i),
            Self::Float(f) => Some(*f as i64),
            Self::String(s) => s.parse().ok(),
            Self::Bool(b) => Some(if *b { 1 } else { 0 }),
            _ => None,
        }
    }

    /// Get as float.
    pub fn as_float(&self) -> Option<f64> {
        match self {
            Self::Float(f) => Some(*f),
            Self::Integer(i) => Some(*i as f64),
            Self::String(s) => s.parse().ok(),
            _ => None,
        }
    }

    /// Get as string.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(s) => Some(s),
            _ => None,
        }
    }

    /// Get as array.
    pub fn as_array(&self) -> Option<&Vec<ConfigValue>> {
        match self {
            Self::Array(arr) => Some(arr),
            _ => None,
        }
    }

    /// Get as object.
    pub fn as_object(&self) -> Option<&HashMap<String, ConfigValue>> {
        match self {
            Self::Object(obj) => Some(obj),
            _ => None,
        }
    }

    /// Get nested value by path.
    pub fn get(&self, path: &str) -> Option<&ConfigValue> {
        let parts: Vec<&str> = path.split('.').collect();
        let mut current = self;

        for part in parts {
            match current {
                Self::Object(obj) => {
                    current = obj.get(part)?;
                }
                Self::Array(arr) => {
                    let idx: usize = part.parse().ok()?;
                    current = arr.get(idx)?;
                }
                _ => return None,
            }
        }

        Some(current)
    }

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
}

impl Default for ConfigValue {
    fn default() -> Self {
        Self::Null
    }
}

impl From<bool> for ConfigValue {
    fn from(v: bool) -> Self {
        Self::Bool(v)
    }
}

impl From<i64> for ConfigValue {
    fn from(v: i64) -> Self {
        Self::Integer(v)
    }
}

impl From<i32> for ConfigValue {
    fn from(v: i32) -> Self {
        Self::Integer(v as i64)
    }
}

impl From<f64> for ConfigValue {
    fn from(v: f64) -> Self {
        Self::Float(v)
    }
}

impl From<String> for ConfigValue {
    fn from(v: String) -> Self {
        Self::String(v)
    }
}

impl From<&str> for ConfigValue {
    fn from(v: &str) -> Self {
        Self::String(v.to_string())
    }
}

impl<T: Into<ConfigValue>> From<Vec<T>> for ConfigValue {
    fn from(v: Vec<T>) -> Self {
        Self::Array(v.into_iter().map(Into::into).collect())
    }
}

/// Config parser for key=value format.
pub struct KeyValueParser {
    delimiter: char,
    comment_prefix: Option<char>,
}

impl KeyValueParser {
    /// Create new parser.
    pub fn new() -> Self {
        Self {
            delimiter: '=',
            comment_prefix: Some('#'),
        }
    }

    /// Set delimiter.
    pub fn delimiter(mut self, d: char) -> Self {
        self.delimiter = d;
        self
    }

    /// Set comment prefix.
    pub fn comment_prefix(mut self, prefix: Option<char>) -> Self {
        self.comment_prefix = prefix;
        self
    }

    /// Parse string.
    pub fn parse(&self, input: &str) -> Result<HashMap<String, String>> {
        let mut result = HashMap::new();

        for line in input.lines() {
            let line = line.trim();

            // Skip empty lines
            if line.is_empty() {
                continue;
            }

            // Skip comments
            if let Some(prefix) = self.comment_prefix {
                if line.starts_with(prefix) {
                    continue;
                }
            }

            // Parse key=value
            if let Some(pos) = line.find(self.delimiter) {
                let key = line[..pos].trim().to_string();
                let value = line[pos + 1..].trim().to_string();
                result.insert(key, value);
            }
        }

        Ok(result)
    }
}

impl Default for KeyValueParser {
    fn default() -> Self {
        Self::new()
    }
}

/// Parse duration string (e.g., "1h30m", "45s", "100ms").
pub fn parse_duration(s: &str) -> Result<std::time::Duration> {
    let s = s.trim().to_lowercase();

    if s.ends_with("ms") {
        let num: u64 = s[..s.len() - 2]
            .parse()
            .map_err(|_| ConfigParseError::ParseError("Invalid milliseconds".into()))?;
        return Ok(std::time::Duration::from_millis(num));
    }

    if s.ends_with('s') {
        let num: u64 = s[..s.len() - 1]
            .parse()
            .map_err(|_| ConfigParseError::ParseError("Invalid seconds".into()))?;
        return Ok(std::time::Duration::from_secs(num));
    }

    if s.ends_with('m') {
        let num: u64 = s[..s.len() - 1]
            .parse()
            .map_err(|_| ConfigParseError::ParseError("Invalid minutes".into()))?;
        return Ok(std::time::Duration::from_secs(num * 60));
    }

    if s.ends_with('h') {
        let num: u64 = s[..s.len() - 1]
            .parse()
            .map_err(|_| ConfigParseError::ParseError("Invalid hours".into()))?;
        return Ok(std::time::Duration::from_secs(num * 3600));
    }

    if s.ends_with('d') {
        let num: u64 = s[..s.len() - 1]
            .parse()
            .map_err(|_| ConfigParseError::ParseError("Invalid days".into()))?;
        return Ok(std::time::Duration::from_secs(num * 86400));
    }

    // Default to seconds
    let num: u64 = s
        .parse()
        .map_err(|_| ConfigParseError::ParseError("Invalid duration".into()))?;
    Ok(std::time::Duration::from_secs(num))
}

/// Parse size string (e.g., "1KB", "512MB", "1GB").
pub fn parse_size(s: &str) -> Result<u64> {
    let s = s.trim().to_uppercase();

    let (num_str, multiplier) = if s.ends_with("GB") {
        (&s[..s.len() - 2], 1024 * 1024 * 1024)
    } else if s.ends_with("MB") {
        (&s[..s.len() - 2], 1024 * 1024)
    } else if s.ends_with("KB") {
        (&s[..s.len() - 2], 1024)
    } else if s.ends_with('B') {
        (&s[..s.len() - 1], 1)
    } else {
        (s.as_str(), 1)
    };

    let num: u64 = num_str
        .trim()
        .parse()
        .map_err(|_| ConfigParseError::ParseError("Invalid size".into()))?;

    Ok(num * multiplier)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_value_types() {
        let v = ConfigValue::Integer(42);
        assert_eq!(v.as_integer(), Some(42));
        assert_eq!(v.as_float(), Some(42.0));

        let v = ConfigValue::String("true".into());
        assert_eq!(v.as_bool(), Some(true));
    }

    #[test]
    fn test_config_value_get() {
        let mut obj = HashMap::new();
        obj.insert("nested".to_string(), ConfigValue::Integer(42));
        let v = ConfigValue::Object(obj);

        assert_eq!(v.get("nested").and_then(|v| v.as_integer()), Some(42));
    }

    #[test]
    fn test_key_value_parser() {
        let input = "
            # Comment
            key1 = value1
            key2 = value2
        ";

        let parser = KeyValueParser::new();
        let result = parser.parse(input).unwrap();

        assert_eq!(result.get("key1"), Some(&"value1".to_string()));
        assert_eq!(result.get("key2"), Some(&"value2".to_string()));
    }

    #[test]
    fn test_parse_duration() {
        assert_eq!(parse_duration("100ms").unwrap().as_millis(), 100);
        assert_eq!(parse_duration("30s").unwrap().as_secs(), 30);
        assert_eq!(parse_duration("5m").unwrap().as_secs(), 300);
        assert_eq!(parse_duration("2h").unwrap().as_secs(), 7200);
    }

    #[test]
    fn test_parse_size() {
        assert_eq!(parse_size("100").unwrap(), 100);
        assert_eq!(parse_size("1KB").unwrap(), 1024);
        assert_eq!(parse_size("1MB").unwrap(), 1024 * 1024);
        assert_eq!(parse_size("1GB").unwrap(), 1024 * 1024 * 1024);
    }
}
