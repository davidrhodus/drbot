//! Serialization utilities for drbot.
//!
//! This crate provides:
//! - JSON serialization helpers
//! - TOML serialization helpers
//! - Format conversion
//! - Pretty printing

use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;
use thiserror::Error;

/// Serialization error types.
#[derive(Error, Debug)]
pub enum SerializationError {
    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("TOML serialize error: {0}")]
    TomlSerError(#[from] toml::ser::Error),

    #[error("TOML deserialize error: {0}")]
    TomlDeError(#[from] toml::de::Error),

    #[error("Format error: {0}")]
    FormatError(String),
}

/// Result type for serialization operations.
pub type Result<T> = std::result::Result<T, SerializationError>;

/// Serialization format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Json,
    JsonPretty,
    Toml,
}

/// Universal serializer.
pub struct Serializer;

impl Serializer {
    /// Serialize to string.
    pub fn serialize<T: Serialize>(value: &T, format: Format) -> Result<String> {
        match format {
            Format::Json => Ok(serde_json::to_string(value)?),
            Format::JsonPretty => Ok(serde_json::to_string_pretty(value)?),
            Format::Toml => Ok(toml::to_string(value)?),
        }
    }

    /// Deserialize from string.
    pub fn deserialize<T: DeserializeOwned>(data: &str, format: Format) -> Result<T> {
        match format {
            Format::Json | Format::JsonPretty => Ok(serde_json::from_str(data)?),
            Format::Toml => Ok(toml::from_str(data)?),
        }
    }

    /// Serialize to bytes.
    pub fn serialize_bytes<T: Serialize>(value: &T, format: Format) -> Result<Vec<u8>> {
        Ok(Self::serialize(value, format)?.into_bytes())
    }

    /// Deserialize from bytes.
    pub fn deserialize_bytes<T: DeserializeOwned>(data: &[u8], format: Format) -> Result<T> {
        let str_data = std::str::from_utf8(data)
            .map_err(|e| SerializationError::FormatError(format!("Invalid UTF-8: {}", e)))?;
        Self::deserialize(str_data, format)
    }
}

/// JSON utilities.
pub struct Json;

impl Json {
    /// Serialize to JSON string.
    pub fn stringify<T: Serialize>(value: &T) -> Result<String> {
        Ok(serde_json::to_string(value)?)
    }

    /// Serialize to pretty JSON string.
    pub fn stringify_pretty<T: Serialize>(value: &T) -> Result<String> {
        Ok(serde_json::to_string_pretty(value)?)
    }

    /// Parse JSON string.
    pub fn parse<T: DeserializeOwned>(data: &str) -> Result<T> {
        Ok(serde_json::from_str(data)?)
    }

    /// Parse to dynamic Value.
    pub fn parse_value(data: &str) -> Result<Value> {
        Ok(serde_json::from_str(data)?)
    }

    /// Convert value to JSON Value.
    pub fn to_value<T: Serialize>(value: T) -> Result<Value> {
        Ok(serde_json::to_value(value)?)
    }

    /// Convert JSON Value to type.
    pub fn from_value<T: DeserializeOwned>(value: Value) -> Result<T> {
        Ok(serde_json::from_value(value)?)
    }

    /// Merge two JSON objects.
    pub fn merge(base: &Value, overlay: &Value) -> Value {
        match (base, overlay) {
            (Value::Object(base_obj), Value::Object(overlay_obj)) => {
                let mut result = base_obj.clone();
                for (key, value) in overlay_obj {
                    if let Some(base_value) = result.get(key) {
                        result.insert(key.clone(), Self::merge(base_value, value));
                    } else {
                        result.insert(key.clone(), value.clone());
                    }
                }
                Value::Object(result)
            }
            (_, overlay) => overlay.clone(),
        }
    }

    /// Get nested value by path.
    pub fn get_path<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
        let mut current = value;
        for key in path.split('.') {
            current = current.get(key)?;
        }
        Some(current)
    }

    /// Set nested value by path.
    pub fn set_path(value: &mut Value, path: &str, new_value: Value) {
        let keys: Vec<&str> = path.split('.').collect();
        let mut current = value;

        for key in &keys[..keys.len() - 1] {
            if !current.is_object() {
                *current = Value::Object(serde_json::Map::new());
            }
            if current.get(*key).is_none() {
                current
                    .as_object_mut()
                    .unwrap()
                    .insert((*key).to_string(), Value::Object(serde_json::Map::new()));
            }
            current = current.get_mut(*key).unwrap();
        }

        if let Some(obj) = current.as_object_mut() {
            obj.insert(keys.last().unwrap().to_string(), new_value);
        }
    }
}

/// TOML utilities.
pub struct Toml;

impl Toml {
    /// Serialize to TOML string.
    pub fn stringify<T: Serialize>(value: &T) -> Result<String> {
        Ok(toml::to_string(value)?)
    }

    /// Serialize to pretty TOML string.
    pub fn stringify_pretty<T: Serialize>(value: &T) -> Result<String> {
        Ok(toml::to_string_pretty(value)?)
    }

    /// Parse TOML string.
    pub fn parse<T: DeserializeOwned>(data: &str) -> Result<T> {
        Ok(toml::from_str(data)?)
    }

    /// Parse to dynamic Value.
    pub fn parse_value(data: &str) -> Result<toml::Value> {
        Ok(toml::from_str(data)?)
    }
}

/// Format converter.
pub struct Converter;

impl Converter {
    /// Convert JSON to TOML.
    pub fn json_to_toml(json: &str) -> Result<String> {
        let value: Value = Json::parse(json)?;
        let toml_value = Self::json_value_to_toml(&value)?;
        Ok(toml::to_string(&toml_value)?)
    }

    /// Convert TOML to JSON.
    pub fn toml_to_json(toml_str: &str) -> Result<String> {
        let value: toml::Value = Toml::parse(toml_str)?;
        let json_value = Self::toml_value_to_json(&value);
        Ok(serde_json::to_string_pretty(&json_value)?)
    }

    fn json_value_to_toml(value: &Value) -> Result<toml::Value> {
        match value {
            Value::Null => Ok(toml::Value::String("null".to_string())),
            Value::Bool(b) => Ok(toml::Value::Boolean(*b)),
            Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Ok(toml::Value::Integer(i))
                } else if let Some(f) = n.as_f64() {
                    Ok(toml::Value::Float(f))
                } else {
                    Err(SerializationError::FormatError(
                        "Invalid number".to_string(),
                    ))
                }
            }
            Value::String(s) => Ok(toml::Value::String(s.clone())),
            Value::Array(arr) => {
                let toml_arr: Result<Vec<_>> = arr.iter().map(Self::json_value_to_toml).collect();
                Ok(toml::Value::Array(toml_arr?))
            }
            Value::Object(obj) => {
                let mut table = toml::map::Map::new();
                for (k, v) in obj {
                    table.insert(k.clone(), Self::json_value_to_toml(v)?);
                }
                Ok(toml::Value::Table(table))
            }
        }
    }

    fn toml_value_to_json(value: &toml::Value) -> Value {
        match value {
            toml::Value::Boolean(b) => Value::Bool(*b),
            toml::Value::Integer(i) => Value::Number((*i).into()),
            toml::Value::Float(f) => {
                Value::Number(serde_json::Number::from_f64(*f).unwrap_or_else(|| 0.into()))
            }
            toml::Value::String(s) => Value::String(s.clone()),
            toml::Value::Datetime(dt) => Value::String(dt.to_string()),
            toml::Value::Array(arr) => {
                Value::Array(arr.iter().map(Self::toml_value_to_json).collect())
            }
            toml::Value::Table(table) => {
                let obj: serde_json::Map<String, Value> = table
                    .iter()
                    .map(|(k, v)| (k.clone(), Self::toml_value_to_json(v)))
                    .collect();
                Value::Object(obj)
            }
        }
    }
}

/// Pretty printer.
pub struct PrettyPrinter;

impl PrettyPrinter {
    /// Pretty print JSON with custom indentation.
    pub fn json_indent<T: Serialize>(value: &T, indent: usize) -> Result<String> {
        let json = serde_json::to_value(value)?;
        Ok(Self::format_json_value(&json, 0, indent))
    }

    fn format_json_value(value: &Value, depth: usize, indent: usize) -> String {
        let prefix = " ".repeat(depth * indent);
        let inner_prefix = " ".repeat((depth + 1) * indent);

        match value {
            Value::Null => "null".to_string(),
            Value::Bool(b) => b.to_string(),
            Value::Number(n) => n.to_string(),
            Value::String(s) => format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"")),
            Value::Array(arr) if arr.is_empty() => "[]".to_string(),
            Value::Array(arr) => {
                let items: Vec<String> = arr
                    .iter()
                    .map(|v| {
                        format!(
                            "{}{}",
                            inner_prefix,
                            Self::format_json_value(v, depth + 1, indent)
                        )
                    })
                    .collect();
                format!("[\n{}\n{}]", items.join(",\n"), prefix)
            }
            Value::Object(obj) if obj.is_empty() => "{}".to_string(),
            Value::Object(obj) => {
                let items: Vec<String> = obj
                    .iter()
                    .map(|(k, v)| {
                        format!(
                            "{}\"{}\": {}",
                            inner_prefix,
                            k,
                            Self::format_json_value(v, depth + 1, indent)
                        )
                    })
                    .collect();
                format!("{{\n{}\n{}}}", items.join(",\n"), prefix)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct TestStruct {
        name: String,
        age: u32,
    }

    #[test]
    fn test_json_stringify() {
        let value = TestStruct {
            name: "Alice".to_string(),
            age: 30,
        };
        let json = Json::stringify(&value).unwrap();
        assert!(json.contains("Alice"));
        assert!(json.contains("30"));
    }

    #[test]
    fn test_json_parse() {
        let json = r#"{"name": "Bob", "age": 25}"#;
        let value: TestStruct = Json::parse(json).unwrap();
        assert_eq!(value.name, "Bob");
        assert_eq!(value.age, 25);
    }

    #[test]
    fn test_json_merge() {
        let base = serde_json::json!({"a": 1, "b": {"c": 2}});
        let overlay = serde_json::json!({"b": {"d": 3}, "e": 4});
        let merged = Json::merge(&base, &overlay);

        assert_eq!(merged["a"], 1);
        assert_eq!(merged["b"]["c"], 2);
        assert_eq!(merged["b"]["d"], 3);
        assert_eq!(merged["e"], 4);
    }

    #[test]
    fn test_json_path() {
        let value = serde_json::json!({"user": {"name": "Alice"}});
        let name = Json::get_path(&value, "user.name");
        assert_eq!(name, Some(&serde_json::json!("Alice")));
    }

    #[test]
    fn test_json_set_path() {
        let mut value = serde_json::json!({});
        Json::set_path(&mut value, "user.name", serde_json::json!("Alice"));
        assert_eq!(value["user"]["name"], "Alice");
    }

    #[test]
    fn test_toml_roundtrip() {
        let value = TestStruct {
            name: "Charlie".to_string(),
            age: 35,
        };
        let toml_str = Toml::stringify(&value).unwrap();
        let parsed: TestStruct = Toml::parse(&toml_str).unwrap();
        assert_eq!(value, parsed);
    }

    #[test]
    fn test_format_conversion() {
        let json = r#"{"name": "Dave", "active": true}"#;
        let toml_str = Converter::json_to_toml(json).unwrap();
        assert!(toml_str.contains("name"));

        let json_back = Converter::toml_to_json(&toml_str).unwrap();
        assert!(json_back.contains("Dave"));
    }

    #[test]
    fn test_universal_serializer() {
        let value = TestStruct {
            name: "Eve".to_string(),
            age: 28,
        };

        let json = Serializer::serialize(&value, Format::Json).unwrap();
        let parsed: TestStruct = Serializer::deserialize(&json, Format::Json).unwrap();
        assert_eq!(value, parsed);
    }
}
