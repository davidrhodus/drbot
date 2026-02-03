//! Data transformation for drbot.
//!
//! This crate provides:
//! - Value transformers
//! - Pipeline composition
//! - Type conversion
//! - Object mapping

use serde_json::Value;
use std::marker::PhantomData;
use thiserror::Error;

/// Transform error types.
#[derive(Error, Debug)]
pub enum TransformError {
    #[error("Type mismatch: expected {expected}, got {actual}")]
    TypeMismatch { expected: String, actual: String },

    #[error("Missing field: {0}")]
    MissingField(String),

    #[error("Invalid value: {0}")]
    InvalidValue(String),

    #[error("Transform failed: {0}")]
    Failed(String),
}

/// Result type for transform operations.
pub type Result<T> = std::result::Result<T, TransformError>;

/// Transformer trait.
pub trait Transformer<In, Out>: Send + Sync {
    /// Transform value.
    fn transform(&self, input: In) -> Result<Out>;
}

/// Identity transformer.
pub struct Identity;

impl<T> Transformer<T, T> for Identity {
    fn transform(&self, input: T) -> Result<T> {
        Ok(input)
    }
}

/// Map transformer.
pub struct Map<F> {
    f: F,
}

impl<F> Map<F> {
    pub fn new(f: F) -> Self {
        Self { f }
    }
}

impl<In, Out, F> Transformer<In, Out> for Map<F>
where
    F: Fn(In) -> Out + Send + Sync,
{
    fn transform(&self, input: In) -> Result<Out> {
        Ok((self.f)(input))
    }
}

/// Try map transformer.
pub struct TryMap<F> {
    f: F,
}

impl<F> TryMap<F> {
    pub fn new(f: F) -> Self {
        Self { f }
    }
}

impl<In, Out, F> Transformer<In, Out> for TryMap<F>
where
    F: Fn(In) -> Result<Out> + Send + Sync,
{
    fn transform(&self, input: In) -> Result<Out> {
        (self.f)(input)
    }
}

/// Compose two transformers.
pub struct Compose<A, B, Mid> {
    first: A,
    second: B,
    _phantom: PhantomData<Mid>,
}

impl<A, B, Mid> Compose<A, B, Mid> {
    pub fn new(first: A, second: B) -> Self {
        Self {
            first,
            second,
            _phantom: PhantomData,
        }
    }
}

impl<A, B, In, Mid, Out> Transformer<In, Out> for Compose<A, B, Mid>
where
    A: Transformer<In, Mid>,
    B: Transformer<Mid, Out>,
    Mid: Send + Sync,
{
    fn transform(&self, input: In) -> Result<Out> {
        let mid = self.first.transform(input)?;
        self.second.transform(mid)
    }
}

/// Pipeline of transformers.
pub struct Pipeline<T> {
    transformers: Vec<Box<dyn Transformer<T, T>>>,
}

impl<T: 'static> Pipeline<T> {
    /// Create empty pipeline.
    pub fn new() -> Self {
        Self {
            transformers: Vec::new(),
        }
    }

    /// Add transformer.
    pub fn add<Tr: Transformer<T, T> + 'static>(mut self, transformer: Tr) -> Self {
        self.transformers.push(Box::new(transformer));
        self
    }

    /// Run pipeline.
    pub fn run(&self, mut value: T) -> Result<T> {
        for transformer in &self.transformers {
            value = transformer.transform(value)?;
        }
        Ok(value)
    }
}

impl<T: 'static> Default for Pipeline<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// String transformers.
pub mod string {
    use super::*;

    /// Trim whitespace.
    pub struct Trim;

    impl Transformer<String, String> for Trim {
        fn transform(&self, input: String) -> Result<String> {
            Ok(input.trim().to_string())
        }
    }

    /// To lowercase.
    pub struct ToLower;

    impl Transformer<String, String> for ToLower {
        fn transform(&self, input: String) -> Result<String> {
            Ok(input.to_lowercase())
        }
    }

    /// To uppercase.
    pub struct ToUpper;

    impl Transformer<String, String> for ToUpper {
        fn transform(&self, input: String) -> Result<String> {
            Ok(input.to_uppercase())
        }
    }

    /// Capitalize.
    pub struct Capitalize;

    impl Transformer<String, String> for Capitalize {
        fn transform(&self, input: String) -> Result<String> {
            let mut chars = input.chars();
            match chars.next() {
                None => Ok(String::new()),
                Some(c) => Ok(c.to_uppercase().chain(chars).collect()),
            }
        }
    }

    /// Replace substring.
    pub struct Replace {
        from: String,
        to: String,
    }

    impl Replace {
        pub fn new(from: impl Into<String>, to: impl Into<String>) -> Self {
            Self {
                from: from.into(),
                to: to.into(),
            }
        }
    }

    impl Transformer<String, String> for Replace {
        fn transform(&self, input: String) -> Result<String> {
            Ok(input.replace(&self.from, &self.to))
        }
    }

    /// Truncate string.
    pub struct Truncate {
        max_len: usize,
        suffix: Option<String>,
    }

    impl Truncate {
        pub fn new(max_len: usize) -> Self {
            Self {
                max_len,
                suffix: None,
            }
        }

        pub fn with_suffix(mut self, suffix: impl Into<String>) -> Self {
            self.suffix = Some(suffix.into());
            self
        }
    }

    impl Transformer<String, String> for Truncate {
        fn transform(&self, input: String) -> Result<String> {
            if input.len() <= self.max_len {
                Ok(input)
            } else {
                let mut result: String = input.chars().take(self.max_len).collect();
                if let Some(ref suffix) = self.suffix {
                    result.push_str(suffix);
                }
                Ok(result)
            }
        }
    }
}

/// JSON transformers.
pub mod json {
    use super::*;

    /// Extract field.
    pub struct Extract {
        path: Vec<String>,
    }

    impl Extract {
        pub fn new(path: &str) -> Self {
            Self {
                path: path.split('.').map(|s| s.to_string()).collect(),
            }
        }
    }

    impl Transformer<Value, Value> for Extract {
        fn transform(&self, input: Value) -> Result<Value> {
            let mut current = &input;
            for key in &self.path {
                current = current
                    .get(key)
                    .ok_or_else(|| TransformError::MissingField(key.clone()))?;
            }
            Ok(current.clone())
        }
    }

    /// Set field.
    pub struct Set {
        path: String,
        value: Value,
    }

    impl Set {
        pub fn new(path: impl Into<String>, value: Value) -> Self {
            Self {
                path: path.into(),
                value,
            }
        }
    }

    impl Transformer<Value, Value> for Set {
        fn transform(&self, mut input: Value) -> Result<Value> {
            if let Value::Object(ref mut obj) = input {
                obj.insert(self.path.clone(), self.value.clone());
            }
            Ok(input)
        }
    }

    /// Remove field.
    pub struct Remove {
        path: String,
    }

    impl Remove {
        pub fn new(path: impl Into<String>) -> Self {
            Self { path: path.into() }
        }
    }

    impl Transformer<Value, Value> for Remove {
        fn transform(&self, mut input: Value) -> Result<Value> {
            if let Value::Object(ref mut obj) = input {
                obj.remove(&self.path);
            }
            Ok(input)
        }
    }

    /// Rename field.
    pub struct Rename {
        from: String,
        to: String,
    }

    impl Rename {
        pub fn new(from: impl Into<String>, to: impl Into<String>) -> Self {
            Self {
                from: from.into(),
                to: to.into(),
            }
        }
    }

    impl Transformer<Value, Value> for Rename {
        fn transform(&self, mut input: Value) -> Result<Value> {
            if let Value::Object(ref mut obj) = input {
                if let Some(value) = obj.remove(&self.from) {
                    obj.insert(self.to.clone(), value);
                }
            }
            Ok(input)
        }
    }

    /// Pick fields.
    pub struct Pick {
        fields: Vec<String>,
    }

    impl Pick {
        pub fn new<S: Into<String>>(fields: Vec<S>) -> Self {
            Self {
                fields: fields.into_iter().map(|s| s.into()).collect(),
            }
        }
    }

    impl Transformer<Value, Value> for Pick {
        fn transform(&self, input: Value) -> Result<Value> {
            if let Value::Object(obj) = input {
                let mut result = serde_json::Map::new();
                for field in &self.fields {
                    if let Some(value) = obj.get(field) {
                        result.insert(field.clone(), value.clone());
                    }
                }
                Ok(Value::Object(result))
            } else {
                Ok(input)
            }
        }
    }

    /// Omit fields.
    pub struct Omit {
        fields: Vec<String>,
    }

    impl Omit {
        pub fn new<S: Into<String>>(fields: Vec<S>) -> Self {
            Self {
                fields: fields.into_iter().map(|s| s.into()).collect(),
            }
        }
    }

    impl Transformer<Value, Value> for Omit {
        fn transform(&self, input: Value) -> Result<Value> {
            if let Value::Object(mut obj) = input {
                for field in &self.fields {
                    obj.remove(field);
                }
                Ok(Value::Object(obj))
            } else {
                Ok(input)
            }
        }
    }

    /// Flatten nested object.
    pub struct Flatten {
        separator: String,
    }

    impl Flatten {
        pub fn new() -> Self {
            Self {
                separator: ".".to_string(),
            }
        }

        pub fn with_separator(mut self, sep: impl Into<String>) -> Self {
            self.separator = sep.into();
            self
        }

        fn flatten_value(
            &self,
            prefix: &str,
            value: &Value,
            result: &mut serde_json::Map<String, Value>,
        ) {
            match value {
                Value::Object(obj) => {
                    for (key, val) in obj {
                        let new_prefix = if prefix.is_empty() {
                            key.clone()
                        } else {
                            format!("{}{}{}", prefix, self.separator, key)
                        };
                        self.flatten_value(&new_prefix, val, result);
                    }
                }
                _ => {
                    result.insert(prefix.to_string(), value.clone());
                }
            }
        }
    }

    impl Default for Flatten {
        fn default() -> Self {
            Self::new()
        }
    }

    impl Transformer<Value, Value> for Flatten {
        fn transform(&self, input: Value) -> Result<Value> {
            let mut result = serde_json::Map::new();
            self.flatten_value("", &input, &mut result);
            Ok(Value::Object(result))
        }
    }
}

/// Object mapper.
pub struct ObjectMapper {
    mappings: Vec<FieldMapping>,
}

struct FieldMapping {
    source: String,
    target: String,
    transform: Option<Box<dyn Transformer<Value, Value>>>,
}

impl ObjectMapper {
    /// Create new mapper.
    pub fn new() -> Self {
        Self {
            mappings: Vec::new(),
        }
    }

    /// Map field directly.
    pub fn map(mut self, source: impl Into<String>, target: impl Into<String>) -> Self {
        self.mappings.push(FieldMapping {
            source: source.into(),
            target: target.into(),
            transform: None,
        });
        self
    }

    /// Map and transform field.
    pub fn map_with<T: Transformer<Value, Value> + 'static>(
        mut self,
        source: impl Into<String>,
        target: impl Into<String>,
        transform: T,
    ) -> Self {
        self.mappings.push(FieldMapping {
            source: source.into(),
            target: target.into(),
            transform: Some(Box::new(transform)),
        });
        self
    }

    /// Apply mapping.
    pub fn apply(&self, input: &Value) -> Result<Value> {
        let input_obj = input
            .as_object()
            .ok_or_else(|| TransformError::TypeMismatch {
                expected: "object".to_string(),
                actual: "other".to_string(),
            })?;

        let mut result = serde_json::Map::new();

        for mapping in &self.mappings {
            if let Some(value) = input_obj.get(&mapping.source) {
                let transformed = if let Some(ref transform) = mapping.transform {
                    transform.transform(value.clone())?
                } else {
                    value.clone()
                };
                result.insert(mapping.target.clone(), transformed);
            }
        }

        Ok(Value::Object(result))
    }
}

impl Default for ObjectMapper {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identity() {
        let transformer = Identity;
        assert_eq!(transformer.transform(42).unwrap(), 42);
    }

    #[test]
    fn test_map() {
        let transformer = Map::new(|x: i32| x * 2);
        assert_eq!(transformer.transform(21).unwrap(), 42);
    }

    #[test]
    fn test_compose() {
        let double = Map::new(|x: i32| x * 2);
        let add_one = Map::new(|x: i32| x + 1);
        let composed: Compose<_, _, i32> = Compose::new(double, add_one);

        assert_eq!(composed.transform(20).unwrap(), 41);
    }

    #[test]
    fn test_string_trim() {
        let transformer = string::Trim;
        assert_eq!(
            transformer.transform("  hello  ".to_string()).unwrap(),
            "hello"
        );
    }

    #[test]
    fn test_string_lower() {
        let transformer = string::ToLower;
        assert_eq!(transformer.transform("HELLO".to_string()).unwrap(), "hello");
    }

    #[test]
    fn test_string_truncate() {
        let transformer = string::Truncate::new(5).with_suffix("...");
        assert_eq!(
            transformer.transform("hello world".to_string()).unwrap(),
            "hello..."
        );
    }

    #[test]
    fn test_json_extract() {
        let transformer = json::Extract::new("user.name");
        let input = serde_json::json!({"user": {"name": "Alice"}});
        let result = transformer.transform(input).unwrap();
        assert_eq!(result, serde_json::json!("Alice"));
    }

    #[test]
    fn test_json_pick() {
        let transformer = json::Pick::new(vec!["a", "b"]);
        let input = serde_json::json!({"a": 1, "b": 2, "c": 3});
        let result = transformer.transform(input).unwrap();
        assert_eq!(result, serde_json::json!({"a": 1, "b": 2}));
    }

    #[test]
    fn test_json_omit() {
        let transformer = json::Omit::new(vec!["c"]);
        let input = serde_json::json!({"a": 1, "b": 2, "c": 3});
        let result = transformer.transform(input).unwrap();
        assert_eq!(result, serde_json::json!({"a": 1, "b": 2}));
    }

    #[test]
    fn test_json_flatten() {
        let transformer = json::Flatten::new();
        let input = serde_json::json!({"user": {"name": "Alice", "age": 30}});
        let result = transformer.transform(input).unwrap();
        assert_eq!(result.get("user.name"), Some(&serde_json::json!("Alice")));
    }

    #[test]
    fn test_object_mapper() {
        let mapper = ObjectMapper::new()
            .map("firstName", "first_name")
            .map("lastName", "last_name");

        let input = serde_json::json!({"firstName": "John", "lastName": "Doe"});
        let result = mapper.apply(&input).unwrap();

        assert_eq!(result.get("first_name"), Some(&serde_json::json!("John")));
    }

    #[test]
    fn test_pipeline() {
        let pipeline = Pipeline::new().add(string::Trim).add(string::ToLower);

        let result = pipeline.run("  HELLO  ".to_string()).unwrap();
        assert_eq!(result, "hello");
    }
}
