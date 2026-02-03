//! Configuration validation utilities for drbot.
//!
//! This crate provides:
//! - Validation rules
//! - Schema validation
//! - Error collection

use std::collections::HashMap;
use thiserror::Error;

/// Validation error types.
#[derive(Error, Debug, Clone)]
pub enum ValidationError {
    #[error("Required field missing: {0}")]
    Required(String),

    #[error("Invalid type for {field}: expected {expected}, found {found}")]
    InvalidType {
        field: String,
        expected: String,
        found: String,
    },

    #[error("Invalid value for {field}: {message}")]
    InvalidValue { field: String, message: String },

    #[error("Value out of range for {field}: {value} not in [{min}, {max}]")]
    OutOfRange {
        field: String,
        value: String,
        min: String,
        max: String,
    },

    #[error("Multiple validation errors")]
    Multiple(Vec<ValidationError>),
}

/// Result type for validation operations.
pub type Result<T> = std::result::Result<T, ValidationError>;

/// Validation result that collects errors.
#[derive(Debug, Clone, Default)]
pub struct ValidationResult {
    errors: Vec<ValidationError>,
}

impl ValidationResult {
    /// Create new result.
    pub fn new() -> Self {
        Self { errors: Vec::new() }
    }

    /// Add error.
    pub fn add_error(&mut self, error: ValidationError) {
        self.errors.push(error);
    }

    /// Check if valid.
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }

    /// Get errors.
    pub fn errors(&self) -> &[ValidationError] {
        &self.errors
    }

    /// Convert to result.
    pub fn into_result(self) -> Result<()> {
        if self.errors.is_empty() {
            Ok(())
        } else if self.errors.len() == 1 {
            Err(self.errors.into_iter().next().unwrap())
        } else {
            Err(ValidationError::Multiple(self.errors))
        }
    }

    /// Merge another result.
    pub fn merge(&mut self, other: ValidationResult) {
        self.errors.extend(other.errors);
    }
}

/// Validator trait.
pub trait Validator<T> {
    /// Validate value.
    fn validate(&self, value: &T) -> ValidationResult;
}

/// Required validator.
pub struct RequiredValidator {
    field_name: String,
}

impl RequiredValidator {
    pub fn new(field_name: impl Into<String>) -> Self {
        Self {
            field_name: field_name.into(),
        }
    }
}

impl Validator<Option<String>> for RequiredValidator {
    fn validate(&self, value: &Option<String>) -> ValidationResult {
        let mut result = ValidationResult::new();
        if value.is_none() || value.as_ref().map(|s| s.is_empty()).unwrap_or(true) {
            result.add_error(ValidationError::Required(self.field_name.clone()));
        }
        result
    }
}

/// Range validator for numbers.
pub struct RangeValidator<T> {
    field_name: String,
    min: Option<T>,
    max: Option<T>,
}

impl<T> RangeValidator<T> {
    pub fn new(field_name: impl Into<String>) -> Self {
        Self {
            field_name: field_name.into(),
            min: None,
            max: None,
        }
    }

    pub fn min(mut self, min: T) -> Self {
        self.min = Some(min);
        self
    }

    pub fn max(mut self, max: T) -> Self {
        self.max = Some(max);
        self
    }
}

impl<T: PartialOrd + std::fmt::Display + Clone> Validator<T> for RangeValidator<T> {
    fn validate(&self, value: &T) -> ValidationResult {
        let mut result = ValidationResult::new();

        let in_range = match (&self.min, &self.max) {
            (Some(min), Some(max)) => value >= min && value <= max,
            (Some(min), None) => value >= min,
            (None, Some(max)) => value <= max,
            (None, None) => true,
        };

        if !in_range {
            result.add_error(ValidationError::OutOfRange {
                field: self.field_name.clone(),
                value: value.to_string(),
                min: self
                    .min
                    .as_ref()
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "-∞".into()),
                max: self
                    .max
                    .as_ref()
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "∞".into()),
            });
        }

        result
    }
}

/// String length validator.
pub struct LengthValidator {
    field_name: String,
    min: Option<usize>,
    max: Option<usize>,
}

impl LengthValidator {
    pub fn new(field_name: impl Into<String>) -> Self {
        Self {
            field_name: field_name.into(),
            min: None,
            max: None,
        }
    }

    pub fn min(mut self, min: usize) -> Self {
        self.min = Some(min);
        self
    }

    pub fn max(mut self, max: usize) -> Self {
        self.max = Some(max);
        self
    }
}

impl Validator<String> for LengthValidator {
    fn validate(&self, value: &String) -> ValidationResult {
        let mut result = ValidationResult::new();
        let len = value.len();

        let valid = match (self.min, self.max) {
            (Some(min), Some(max)) => len >= min && len <= max,
            (Some(min), None) => len >= min,
            (None, Some(max)) => len <= max,
            (None, None) => true,
        };

        if !valid {
            result.add_error(ValidationError::InvalidValue {
                field: self.field_name.clone(),
                message: format!(
                    "length {} not in range [{}, {}]",
                    len,
                    self.min
                        .map(|v| v.to_string())
                        .unwrap_or_else(|| "0".into()),
                    self.max
                        .map(|v| v.to_string())
                        .unwrap_or_else(|| "∞".into())
                ),
            });
        }

        result
    }
}

/// Pattern validator.
pub struct PatternValidator {
    field_name: String,
    pattern: String,
}

impl PatternValidator {
    pub fn new(field_name: impl Into<String>, pattern: impl Into<String>) -> Self {
        Self {
            field_name: field_name.into(),
            pattern: pattern.into(),
        }
    }
}

impl Validator<String> for PatternValidator {
    fn validate(&self, value: &String) -> ValidationResult {
        let mut result = ValidationResult::new();

        // Simple pattern matching (for demo - in production use regex)
        let matches = if self.pattern == "email" {
            value.contains('@') && value.contains('.')
        } else if self.pattern == "url" {
            value.starts_with("http://") || value.starts_with("https://")
        } else {
            true
        };

        if !matches {
            result.add_error(ValidationError::InvalidValue {
                field: self.field_name.clone(),
                message: format!("does not match pattern '{}'", self.pattern),
            });
        }

        result
    }
}

/// Enum validator.
pub struct EnumValidator {
    field_name: String,
    allowed: Vec<String>,
}

impl EnumValidator {
    pub fn new<S: Into<String>>(field_name: impl Into<String>, allowed: Vec<S>) -> Self {
        Self {
            field_name: field_name.into(),
            allowed: allowed.into_iter().map(Into::into).collect(),
        }
    }
}

impl Validator<String> for EnumValidator {
    fn validate(&self, value: &String) -> ValidationResult {
        let mut result = ValidationResult::new();

        if !self.allowed.contains(value) {
            result.add_error(ValidationError::InvalidValue {
                field: self.field_name.clone(),
                message: format!("must be one of: {:?}", self.allowed),
            });
        }

        result
    }
}

/// Config schema for validation.
#[derive(Default)]
pub struct ConfigSchema {
    validators: HashMap<String, Vec<Box<dyn Fn(&str) -> ValidationResult + Send + Sync>>>,
}

impl ConfigSchema {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add string field validation.
    pub fn string_field<F>(mut self, field: &str, validator: F) -> Self
    where
        F: Fn(&str) -> ValidationResult + Send + Sync + 'static,
    {
        self.validators
            .entry(field.to_string())
            .or_default()
            .push(Box::new(validator));
        self
    }

    /// Validate a map of string values.
    pub fn validate(&self, values: &HashMap<String, String>) -> ValidationResult {
        let mut result = ValidationResult::new();

        for (field, validators) in &self.validators {
            let value = values.get(field).map(|s| s.as_str()).unwrap_or("");
            for validator in validators {
                result.merge(validator(value));
            }
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_required_validator() {
        let v = RequiredValidator::new("name");

        assert!(v.validate(&Some("value".to_string())).is_valid());
        assert!(!v.validate(&None).is_valid());
        assert!(!v.validate(&Some("".to_string())).is_valid());
    }

    #[test]
    fn test_range_validator() {
        let v = RangeValidator::new("port").min(1).max(65535);

        assert!(v.validate(&8080).is_valid());
        assert!(!v.validate(&0).is_valid());
        assert!(!v.validate(&70000).is_valid());
    }

    #[test]
    fn test_length_validator() {
        let v = LengthValidator::new("password").min(8).max(100);

        assert!(v.validate(&"longpassword".to_string()).is_valid());
        assert!(!v.validate(&"short".to_string()).is_valid());
    }

    #[test]
    fn test_enum_validator() {
        let v = EnumValidator::new("env", vec!["dev", "staging", "prod"]);

        assert!(v.validate(&"dev".to_string()).is_valid());
        assert!(!v.validate(&"invalid".to_string()).is_valid());
    }

    #[test]
    fn test_validation_result() {
        let mut result = ValidationResult::new();
        assert!(result.is_valid());

        result.add_error(ValidationError::Required("field".into()));
        assert!(!result.is_valid());
        assert_eq!(result.errors().len(), 1);
    }
}
