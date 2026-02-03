//! Input validation for drbot.
//!
//! This crate provides:
//! - Validation rules
//! - Composable validators
//! - Error collection

use std::collections::HashMap;
use thiserror::Error;

/// Validation error types.
#[derive(Error, Debug, Clone)]
pub enum ValidationError {
    #[error("Field '{field}': {message}")]
    FieldError { field: String, message: String },

    #[error("Validation failed: {0}")]
    Failed(String),

    #[error("Multiple validation errors")]
    Multiple(Vec<ValidationError>),
}

/// Result type for validation operations.
pub type Result<T> = std::result::Result<T, ValidationError>;

/// Validation result with multiple errors.
#[derive(Debug, Clone)]
pub struct ValidationResult {
    errors: Vec<ValidationError>,
}

impl ValidationResult {
    /// Create new empty result.
    pub fn new() -> Self {
        Self { errors: Vec::new() }
    }

    /// Add error.
    pub fn add_error(&mut self, error: ValidationError) {
        self.errors.push(error);
    }

    /// Add field error.
    pub fn add_field_error(&mut self, field: impl Into<String>, message: impl Into<String>) {
        self.errors.push(ValidationError::FieldError {
            field: field.into(),
            message: message.into(),
        });
    }

    /// Check if valid.
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }

    /// Get errors.
    pub fn errors(&self) -> &[ValidationError] {
        &self.errors
    }

    /// Convert to Result.
    pub fn into_result(self) -> Result<()> {
        if self.errors.is_empty() {
            Ok(())
        } else if self.errors.len() == 1 {
            Err(self.errors.into_iter().next().unwrap())
        } else {
            Err(ValidationError::Multiple(self.errors))
        }
    }

    /// Merge with another result.
    pub fn merge(&mut self, other: ValidationResult) {
        self.errors.extend(other.errors);
    }
}

impl Default for ValidationResult {
    fn default() -> Self {
        Self::new()
    }
}

/// Validator trait.
pub trait Validator<T> {
    /// Validate value.
    fn validate(&self, value: &T) -> ValidationResult;
}

/// Function-based validator.
pub struct FnValidator<T, F> {
    validate_fn: F,
    _marker: std::marker::PhantomData<T>,
}

impl<T, F> FnValidator<T, F>
where
    F: Fn(&T) -> ValidationResult,
{
    /// Create new function validator.
    pub fn new(f: F) -> Self {
        Self {
            validate_fn: f,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<T, F> Validator<T> for FnValidator<T, F>
where
    F: Fn(&T) -> ValidationResult,
{
    fn validate(&self, value: &T) -> ValidationResult {
        (self.validate_fn)(value)
    }
}

/// Create validator from function.
pub fn validator<T, F>(f: F) -> FnValidator<T, F>
where
    F: Fn(&T) -> ValidationResult,
{
    FnValidator::new(f)
}

/// Composed validator.
pub struct ComposedValidator<T> {
    validators: Vec<Box<dyn Validator<T> + Send + Sync>>,
}

impl<T> ComposedValidator<T> {
    /// Create new composed validator.
    pub fn new() -> Self {
        Self {
            validators: Vec::new(),
        }
    }

    /// Add validator.
    pub fn add<V: Validator<T> + Send + Sync + 'static>(mut self, validator: V) -> Self {
        self.validators.push(Box::new(validator));
        self
    }
}

impl<T> Default for ComposedValidator<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Validator<T> for ComposedValidator<T> {
    fn validate(&self, value: &T) -> ValidationResult {
        let mut result = ValidationResult::new();
        for validator in &self.validators {
            result.merge(validator.validate(value));
        }
        result
    }
}

// Common validators

/// Not empty validator.
pub fn not_empty(field: &str) -> impl Fn(&str) -> ValidationResult + '_ {
    move |value: &str| {
        let mut result = ValidationResult::new();
        if value.is_empty() {
            result.add_field_error(field, "must not be empty");
        }
        result
    }
}

/// Min length validator.
pub fn min_length(field: &str, min: usize) -> impl Fn(&str) -> ValidationResult + '_ {
    move |value: &str| {
        let mut result = ValidationResult::new();
        if value.len() < min {
            result.add_field_error(field, format!("must be at least {} characters", min));
        }
        result
    }
}

/// Max length validator.
pub fn max_length(field: &str, max: usize) -> impl Fn(&str) -> ValidationResult + '_ {
    move |value: &str| {
        let mut result = ValidationResult::new();
        if value.len() > max {
            result.add_field_error(field, format!("must be at most {} characters", max));
        }
        result
    }
}

/// Range validator.
pub fn in_range<T: PartialOrd + std::fmt::Display>(
    field: &str,
    min: T,
    max: T,
) -> impl Fn(&T) -> ValidationResult + '_
where
    T: 'static,
{
    move |value: &T| {
        let mut result = ValidationResult::new();
        if value < &min || value > &max {
            result.add_field_error(field, format!("must be between {} and {}", min, max));
        }
        result
    }
}

/// Matches pattern validator.
pub fn matches_pattern<'a>(
    field: &'a str,
    pattern: &'a str,
) -> impl Fn(&str) -> ValidationResult + 'a {
    move |value: &str| {
        let mut result = ValidationResult::new();
        // Simple pattern matching - just check contains for demo
        if !value.contains(pattern) {
            result.add_field_error(field, format!("must match pattern '{}'", pattern));
        }
        result
    }
}

/// Email validator (simple).
pub fn email(field: &str) -> impl Fn(&str) -> ValidationResult + '_ {
    move |value: &str| {
        let mut result = ValidationResult::new();
        if !value.contains('@') || !value.contains('.') {
            result.add_field_error(field, "must be a valid email address");
        }
        result
    }
}

/// Field validator builder.
pub struct FieldValidator<'a, T> {
    field: &'a str,
    value: &'a T,
    result: ValidationResult,
}

impl<'a, T> FieldValidator<'a, T> {
    /// Create new field validator.
    pub fn new(field: &'a str, value: &'a T) -> Self {
        Self {
            field,
            value,
            result: ValidationResult::new(),
        }
    }

    /// Add custom validation.
    pub fn validate<F>(mut self, f: F) -> Self
    where
        F: FnOnce(&T) -> Option<String>,
    {
        if let Some(message) = f(self.value) {
            self.result.add_field_error(self.field, message);
        }
        self
    }

    /// Finish validation.
    pub fn finish(self) -> ValidationResult {
        self.result
    }
}

impl<'a> FieldValidator<'a, String> {
    /// Not empty.
    pub fn not_empty(self) -> Self {
        self.validate(|v| {
            if v.is_empty() {
                Some("must not be empty".to_string())
            } else {
                None
            }
        })
    }

    /// Min length.
    pub fn min_length(self, min: usize) -> Self {
        self.validate(move |v| {
            if v.len() < min {
                Some(format!("must be at least {} characters", min))
            } else {
                None
            }
        })
    }

    /// Max length.
    pub fn max_length(self, max: usize) -> Self {
        self.validate(move |v| {
            if v.len() > max {
                Some(format!("must be at most {} characters", max))
            } else {
                None
            }
        })
    }
}

/// Create field validator.
pub fn validate_field<'a, T>(field: &'a str, value: &'a T) -> FieldValidator<'a, T> {
    FieldValidator::new(field, value)
}

/// Validate all and collect errors.
pub fn validate_all(validations: Vec<ValidationResult>) -> ValidationResult {
    let mut result = ValidationResult::new();
    for v in validations {
        result.merge(v);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validation_result() {
        let mut result = ValidationResult::new();
        assert!(result.is_valid());

        result.add_field_error("name", "is required");
        assert!(!result.is_valid());
        assert_eq!(result.errors().len(), 1);
    }

    #[test]
    fn test_not_empty() {
        let validator = not_empty("name");
        assert!(validator("hello").is_valid());
        assert!(!validator("").is_valid());
    }

    #[test]
    fn test_min_length() {
        let validator = min_length("password", 8);
        assert!(validator("longpassword").is_valid());
        assert!(!validator("short").is_valid());
    }

    #[test]
    fn test_email_validator() {
        let validator = email("email");
        assert!(validator("test@example.com").is_valid());
        assert!(!validator("invalid").is_valid());
    }

    #[test]
    fn test_field_validator() {
        let value = "hi".to_string();
        let result = validate_field("name", &value)
            .not_empty()
            .min_length(5)
            .finish();

        assert!(!result.is_valid());
    }

    #[test]
    fn test_validate_all() {
        let result = validate_all(vec![
            not_empty("name")(""),
            min_length("password", 8)("short"),
        ]);

        assert!(!result.is_valid());
        assert_eq!(result.errors().len(), 2);
    }
}
