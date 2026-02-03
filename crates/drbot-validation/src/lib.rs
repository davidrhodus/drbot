//! Input validation for drbot.
//!
//! This crate provides:
//! - Validation rules
//! - Composable validators
//! - Error collection
//! - Field-level validation

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::marker::PhantomData;
use thiserror::Error;

/// Validation error types.
#[derive(Error, Debug, Clone)]
pub enum ValidationError {
    #[error("Required field")]
    Required,

    #[error("Invalid format: {0}")]
    InvalidFormat(String),

    #[error("Too short: minimum {min}, got {actual}")]
    TooShort { min: usize, actual: usize },

    #[error("Too long: maximum {max}, got {actual}")]
    TooLong { max: usize, actual: usize },

    #[error("Below minimum: {min}")]
    BelowMin { min: String },

    #[error("Above maximum: {max}")]
    AboveMax { max: String },

    #[error("Invalid choice: got {value}, expected one of {choices:?}")]
    InvalidChoice { value: String, choices: Vec<String> },

    #[error("Pattern mismatch")]
    PatternMismatch,

    #[error("Custom: {0}")]
    Custom(String),
}

/// Field validation error.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldError {
    /// Field name.
    pub field: String,
    /// Error message.
    pub message: String,
    /// Error code.
    pub code: String,
}

impl FieldError {
    /// Create new field error.
    pub fn new(
        field: impl Into<String>,
        message: impl Into<String>,
        code: impl Into<String>,
    ) -> Self {
        Self {
            field: field.into(),
            message: message.into(),
            code: code.into(),
        }
    }
}

/// Validation result.
#[derive(Debug, Clone, Default)]
pub struct ValidationResult {
    /// Field errors.
    pub errors: Vec<FieldError>,
}

impl ValidationResult {
    /// Create empty result.
    pub fn new() -> Self {
        Self { errors: Vec::new() }
    }

    /// Add error.
    pub fn add_error(&mut self, error: FieldError) {
        self.errors.push(error);
    }

    /// Add error with field and message.
    pub fn error(&mut self, field: impl Into<String>, message: impl Into<String>) {
        self.errors
            .push(FieldError::new(field, message, "validation_error"));
    }

    /// Check if valid.
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }

    /// Check if has errors.
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    /// Merge another result.
    pub fn merge(&mut self, other: ValidationResult) {
        self.errors.extend(other.errors);
    }

    /// Get errors for field.
    pub fn field_errors(&self, field: &str) -> Vec<&FieldError> {
        self.errors.iter().filter(|e| e.field == field).collect()
    }

    /// Convert to result.
    pub fn to_result<T>(self, value: T) -> Result<T, ValidationResult> {
        if self.is_valid() {
            Ok(value)
        } else {
            Err(self)
        }
    }
}

/// Validation rule trait.
pub trait Rule<T>: Send + Sync {
    /// Validate value.
    fn validate(&self, value: &T) -> Option<ValidationError>;
}

/// Required rule.
pub struct Required;

impl Rule<Option<String>> for Required {
    fn validate(&self, value: &Option<String>) -> Option<ValidationError> {
        match value {
            None => Some(ValidationError::Required),
            Some(s) if s.is_empty() => Some(ValidationError::Required),
            _ => None,
        }
    }
}

impl Rule<String> for Required {
    fn validate(&self, value: &String) -> Option<ValidationError> {
        if value.is_empty() {
            Some(ValidationError::Required)
        } else {
            None
        }
    }
}

/// Min length rule.
pub struct MinLength(pub usize);

impl Rule<String> for MinLength {
    fn validate(&self, value: &String) -> Option<ValidationError> {
        if value.len() < self.0 {
            Some(ValidationError::TooShort {
                min: self.0,
                actual: value.len(),
            })
        } else {
            None
        }
    }
}

/// Max length rule.
pub struct MaxLength(pub usize);

impl Rule<String> for MaxLength {
    fn validate(&self, value: &String) -> Option<ValidationError> {
        if value.len() > self.0 {
            Some(ValidationError::TooLong {
                max: self.0,
                actual: value.len(),
            })
        } else {
            None
        }
    }
}

/// Length range rule.
pub struct LengthRange {
    pub min: usize,
    pub max: usize,
}

impl LengthRange {
    pub fn new(min: usize, max: usize) -> Self {
        Self { min, max }
    }
}

impl Rule<String> for LengthRange {
    fn validate(&self, value: &String) -> Option<ValidationError> {
        let len = value.len();
        if len < self.min {
            Some(ValidationError::TooShort {
                min: self.min,
                actual: len,
            })
        } else if len > self.max {
            Some(ValidationError::TooLong {
                max: self.max,
                actual: len,
            })
        } else {
            None
        }
    }
}

/// Pattern rule.
pub struct Pattern {
    regex: Regex,
    description: String,
}

impl Pattern {
    /// Create new pattern rule.
    pub fn new(pattern: &str, description: impl Into<String>) -> Result<Self, regex::Error> {
        Ok(Self {
            regex: Regex::new(pattern)?,
            description: description.into(),
        })
    }

    /// Email pattern.
    pub fn email() -> Self {
        Self::new(
            r"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$",
            "Invalid email format",
        )
        .unwrap()
    }

    /// URL pattern.
    pub fn url() -> Self {
        Self::new(r"^https?://[^\s/$.?#].[^\s]*$", "Invalid URL format").unwrap()
    }

    /// Alphanumeric pattern.
    pub fn alphanumeric() -> Self {
        Self::new(r"^[a-zA-Z0-9]+$", "Must be alphanumeric").unwrap()
    }

    /// UUID pattern.
    pub fn uuid() -> Self {
        Self::new(
            r"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$",
            "Invalid UUID format",
        )
        .unwrap()
    }
}

impl Rule<String> for Pattern {
    fn validate(&self, value: &String) -> Option<ValidationError> {
        if self.regex.is_match(value) {
            None
        } else {
            Some(ValidationError::InvalidFormat(self.description.clone()))
        }
    }
}

/// Numeric range rule.
pub struct Range<T> {
    pub min: Option<T>,
    pub max: Option<T>,
}

impl<T: PartialOrd + std::fmt::Display + Clone> Range<T> {
    pub fn new(min: Option<T>, max: Option<T>) -> Self {
        Self { min, max }
    }

    pub fn min(min: T) -> Self {
        Self {
            min: Some(min),
            max: None,
        }
    }

    pub fn max(max: T) -> Self {
        Self {
            min: None,
            max: Some(max),
        }
    }

    pub fn between(min: T, max: T) -> Self {
        Self {
            min: Some(min),
            max: Some(max),
        }
    }
}

impl<T: PartialOrd + std::fmt::Display + Clone + Send + Sync> Rule<T> for Range<T> {
    fn validate(&self, value: &T) -> Option<ValidationError> {
        if let Some(ref min) = self.min {
            if value < min {
                return Some(ValidationError::BelowMin {
                    min: min.to_string(),
                });
            }
        }
        if let Some(ref max) = self.max {
            if value > max {
                return Some(ValidationError::AboveMax {
                    max: max.to_string(),
                });
            }
        }
        None
    }
}

/// One of rule.
pub struct OneOf {
    choices: Vec<String>,
}

impl OneOf {
    pub fn new<S: Into<String>>(choices: Vec<S>) -> Self {
        Self {
            choices: choices.into_iter().map(|s| s.into()).collect(),
        }
    }
}

impl Rule<String> for OneOf {
    fn validate(&self, value: &String) -> Option<ValidationError> {
        if self.choices.contains(value) {
            None
        } else {
            Some(ValidationError::InvalidChoice {
                value: value.clone(),
                choices: self.choices.clone(),
            })
        }
    }
}

/// Custom rule.
pub struct Custom<F, T> {
    validator: F,
    message: String,
    _phantom: PhantomData<T>,
}

impl<F, T> Custom<F, T>
where
    F: Fn(&T) -> bool,
{
    pub fn new(validator: F, message: impl Into<String>) -> Self {
        Self {
            validator,
            message: message.into(),
            _phantom: PhantomData,
        }
    }
}

impl<F, T> Rule<T> for Custom<F, T>
where
    F: Fn(&T) -> bool + Send + Sync,
    T: Send + Sync,
{
    fn validate(&self, value: &T) -> Option<ValidationError> {
        if (self.validator)(value) {
            None
        } else {
            Some(ValidationError::Custom(self.message.clone()))
        }
    }
}

/// Field validator.
pub struct FieldValidator<T> {
    field: String,
    rules: Vec<Box<dyn Rule<T>>>,
}

impl<T> FieldValidator<T> {
    /// Create new field validator.
    pub fn new(field: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            rules: Vec::new(),
        }
    }

    /// Add rule.
    pub fn rule<R: Rule<T> + 'static>(mut self, rule: R) -> Self {
        self.rules.push(Box::new(rule));
        self
    }

    /// Validate value.
    pub fn validate(&self, value: &T) -> ValidationResult {
        let mut result = ValidationResult::new();

        for rule in &self.rules {
            if let Some(error) = rule.validate(value) {
                result.add_error(FieldError::new(
                    &self.field,
                    error.to_string(),
                    "validation_error",
                ));
            }
        }

        result
    }
}

/// Validator builder.
pub struct Validator {
    results: ValidationResult,
}

impl Validator {
    /// Create new validator.
    pub fn new() -> Self {
        Self {
            results: ValidationResult::new(),
        }
    }

    /// Validate field.
    pub fn field<T>(mut self, field: &str, value: &T, rules: &[&dyn Rule<T>]) -> Self {
        for rule in rules {
            if let Some(error) = rule.validate(value) {
                self.results.add_error(FieldError::new(
                    field,
                    error.to_string(),
                    "validation_error",
                ));
            }
        }
        self
    }

    /// Add custom validation.
    pub fn check<F>(mut self, field: &str, condition: F, message: &str) -> Self
    where
        F: FnOnce() -> bool,
    {
        if !condition() {
            self.results.error(field, message);
        }
        self
    }

    /// Finish validation.
    pub fn finish(self) -> ValidationResult {
        self.results
    }
}

impl Default for Validator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_required() {
        let rule = Required;

        assert!(rule.validate(&"hello".to_string()).is_none());
        assert!(rule.validate(&"".to_string()).is_some());
    }

    #[test]
    fn test_min_length() {
        let rule = MinLength(5);

        assert!(rule.validate(&"hello".to_string()).is_none());
        assert!(rule.validate(&"hi".to_string()).is_some());
    }

    #[test]
    fn test_max_length() {
        let rule = MaxLength(5);

        assert!(rule.validate(&"hello".to_string()).is_none());
        assert!(rule.validate(&"hello world".to_string()).is_some());
    }

    #[test]
    fn test_pattern_email() {
        let rule = Pattern::email();

        assert!(rule.validate(&"test@example.com".to_string()).is_none());
        assert!(rule.validate(&"invalid".to_string()).is_some());
    }

    #[test]
    fn test_range() {
        let rule = Range::between(1, 10);

        assert!(rule.validate(&5).is_none());
        assert!(rule.validate(&0).is_some());
        assert!(rule.validate(&11).is_some());
    }

    #[test]
    fn test_one_of() {
        let rule = OneOf::new(vec!["a", "b", "c"]);

        assert!(rule.validate(&"a".to_string()).is_none());
        assert!(rule.validate(&"d".to_string()).is_some());
    }

    #[test]
    fn test_custom() {
        let rule = Custom::new(|s: &String| s.starts_with("test"), "Must start with test");

        assert!(rule.validate(&"test123".to_string()).is_none());
        assert!(rule.validate(&"hello".to_string()).is_some());
    }

    #[test]
    fn test_field_validator() {
        let validator = FieldValidator::new("email")
            .rule(Required)
            .rule(Pattern::email());

        let result = validator.validate(&"test@example.com".to_string());
        assert!(result.is_valid());

        let result = validator.validate(&"invalid".to_string());
        assert!(result.has_errors());
    }

    #[test]
    fn test_validator_builder() {
        let required = Required;
        let min_len = MinLength(3);

        let result = Validator::new()
            .field("name", &"ab".to_string(), &[&required, &min_len])
            .check("age", || 25 >= 18, "Must be 18+")
            .finish();

        assert!(result.has_errors());
        assert_eq!(result.field_errors("name").len(), 1);
    }

    #[test]
    fn test_validation_result() {
        let mut result = ValidationResult::new();
        result.error("field1", "Error 1");
        result.error("field2", "Error 2");

        assert!(!result.is_valid());
        assert_eq!(result.errors.len(), 2);
    }
}
