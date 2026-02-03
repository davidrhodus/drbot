//! Precondition checking for drbot.
//!
//! This crate provides:
//! - Precondition macros
//! - Require checks
//! - Argument validation

use thiserror::Error;

/// Precondition error types.
#[derive(Error, Debug, Clone)]
pub enum PreconditionError {
    #[error("Precondition failed: {0}")]
    Failed(String),

    #[error("Argument '{name}' invalid: {reason}")]
    InvalidArgument { name: String, reason: String },

    #[error("Required condition not met: {0}")]
    RequireFailed(String),
}

/// Result type for precondition operations.
pub type Result<T> = std::result::Result<T, PreconditionError>;

/// Check precondition.
#[inline]
pub fn require(condition: bool, message: &str) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(PreconditionError::RequireFailed(message.to_string()))
    }
}

/// Check precondition with lazy message.
#[inline]
pub fn require_with<F>(condition: bool, message_fn: F) -> Result<()>
where
    F: FnOnce() -> String,
{
    if condition {
        Ok(())
    } else {
        Err(PreconditionError::RequireFailed(message_fn()))
    }
}

/// Check that argument is not None.
pub fn require_some<T>(value: Option<T>, name: &str) -> Result<T> {
    value.ok_or_else(|| PreconditionError::InvalidArgument {
        name: name.to_string(),
        reason: "must not be None".to_string(),
    })
}

/// Check that argument is not empty.
pub fn require_not_empty(value: &str, name: &str) -> Result<()> {
    if value.is_empty() {
        Err(PreconditionError::InvalidArgument {
            name: name.to_string(),
            reason: "must not be empty".to_string(),
        })
    } else {
        Ok(())
    }
}

/// Check that slice is not empty.
pub fn require_non_empty_slice<T>(value: &[T], name: &str) -> Result<()> {
    if value.is_empty() {
        Err(PreconditionError::InvalidArgument {
            name: name.to_string(),
            reason: "must not be empty".to_string(),
        })
    } else {
        Ok(())
    }
}

/// Check that value is positive.
pub fn require_positive<T: PartialOrd + Default>(value: T, name: &str) -> Result<()> {
    if value > T::default() {
        Ok(())
    } else {
        Err(PreconditionError::InvalidArgument {
            name: name.to_string(),
            reason: "must be positive".to_string(),
        })
    }
}

/// Check that value is non-negative.
pub fn require_non_negative<T: PartialOrd + Default>(value: T, name: &str) -> Result<()> {
    if value >= T::default() {
        Ok(())
    } else {
        Err(PreconditionError::InvalidArgument {
            name: name.to_string(),
            reason: "must be non-negative".to_string(),
        })
    }
}

/// Check that value is in range.
pub fn require_in_range<T: PartialOrd + std::fmt::Display>(
    value: T,
    min: T,
    max: T,
    name: &str,
) -> Result<()> {
    if value >= min && value <= max {
        Ok(())
    } else {
        Err(PreconditionError::InvalidArgument {
            name: name.to_string(),
            reason: format!("must be between {} and {}", min, max),
        })
    }
}

/// Precondition builder.
pub struct Preconditions {
    errors: Vec<PreconditionError>,
}

impl Preconditions {
    /// Create new preconditions builder.
    pub fn new() -> Self {
        Self { errors: Vec::new() }
    }

    /// Add requirement.
    pub fn require(mut self, condition: bool, message: &str) -> Self {
        if !condition {
            self.errors
                .push(PreconditionError::RequireFailed(message.to_string()));
        }
        self
    }

    /// Check argument.
    pub fn check_arg<F>(mut self, name: &str, check: F) -> Self
    where
        F: FnOnce() -> Option<String>,
    {
        if let Some(reason) = check() {
            self.errors.push(PreconditionError::InvalidArgument {
                name: name.to_string(),
                reason,
            });
        }
        self
    }

    /// Verify all preconditions.
    pub fn verify(self) -> Result<()> {
        if self.errors.is_empty() {
            Ok(())
        } else if self.errors.len() == 1 {
            Err(self.errors.into_iter().next().unwrap())
        } else {
            // Combine errors into one message
            let messages: Vec<_> = self.errors.iter().map(|e| e.to_string()).collect();
            Err(PreconditionError::Failed(messages.join("; ")))
        }
    }

    /// Check if all preconditions pass.
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }
}

impl Default for Preconditions {
    fn default() -> Self {
        Self::new()
    }
}

/// Start precondition checks.
pub fn check() -> Preconditions {
    Preconditions::new()
}

/// Assert precondition (panics on failure).
#[track_caller]
pub fn assert_precondition(condition: bool, message: &str) {
    if !condition {
        panic!("Precondition failed: {}", message);
    }
}

/// Debug-only precondition check.
#[cfg(debug_assertions)]
#[track_caller]
pub fn debug_require(condition: bool, message: &str) {
    if !condition {
        panic!("Debug precondition failed: {}", message);
    }
}

#[cfg(not(debug_assertions))]
#[inline]
pub fn debug_require(_condition: bool, _message: &str) {}

/// Precondition wrapper for functions.
pub struct WithPreconditions<F> {
    preconditions: Vec<Box<dyn Fn() -> Result<()> + Send + Sync>>,
    func: F,
}

impl<F, T> WithPreconditions<F>
where
    F: Fn() -> T,
{
    /// Create new preconditioned function.
    pub fn new(func: F) -> Self {
        Self {
            preconditions: Vec::new(),
            func,
        }
    }

    /// Add precondition.
    pub fn require<P>(mut self, precondition: P) -> Self
    where
        P: Fn() -> Result<()> + Send + Sync + 'static,
    {
        self.preconditions.push(Box::new(precondition));
        self
    }

    /// Execute with precondition checks.
    pub fn execute(&self) -> Result<T> {
        for precondition in &self.preconditions {
            precondition()?;
        }
        Ok((self.func)())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_require_success() {
        assert!(require(true, "should pass").is_ok());
    }

    #[test]
    fn test_require_failure() {
        let result = require(false, "should fail");
        assert!(result.is_err());
    }

    #[test]
    fn test_require_some() {
        assert!(require_some(Some(42), "value").is_ok());
        assert!(require_some::<i32>(None, "value").is_err());
    }

    #[test]
    fn test_require_not_empty() {
        assert!(require_not_empty("hello", "name").is_ok());
        assert!(require_not_empty("", "name").is_err());
    }

    #[test]
    fn test_require_positive() {
        assert!(require_positive(5, "count").is_ok());
        assert!(require_positive(0, "count").is_err());
        assert!(require_positive(-1, "count").is_err());
    }

    #[test]
    fn test_require_in_range() {
        assert!(require_in_range(5, 1, 10, "value").is_ok());
        assert!(require_in_range(0, 1, 10, "value").is_err());
        assert!(require_in_range(11, 1, 10, "value").is_err());
    }

    #[test]
    fn test_preconditions_builder() {
        let result = check()
            .require(true, "first")
            .require(true, "second")
            .verify();
        assert!(result.is_ok());

        let result = check()
            .require(false, "first")
            .require(true, "second")
            .verify();
        assert!(result.is_err());
    }

    #[test]
    #[should_panic(expected = "Precondition failed")]
    fn test_assert_precondition() {
        assert_precondition(false, "test");
    }
}
