//! Invariant checking for drbot.
//!
//! This crate provides:
//! - Invariant traits
//! - Runtime invariant checks
//! - Invariant guards

use thiserror::Error;

/// Invariant error types.
#[derive(Error, Debug, Clone)]
pub enum InvariantError {
    #[error("Invariant violated: {0}")]
    Violated(String),

    #[error("Invariant '{name}' violated: {message}")]
    Named { name: String, message: String },
}

/// Result type for invariant operations.
pub type Result<T> = std::result::Result<T, InvariantError>;

/// Trait for types with invariants.
pub trait Invariant {
    /// Check if invariant holds.
    fn check_invariant(&self) -> Result<()>;

    /// Check invariant, returning self if valid.
    fn assert_invariant(self) -> Result<Self>
    where
        Self: Sized,
    {
        self.check_invariant()?;
        Ok(self)
    }
}

/// Check invariant condition.
#[inline]
pub fn check(condition: bool, message: &str) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(InvariantError::Violated(message.to_string()))
    }
}

/// Check named invariant.
#[inline]
pub fn check_named(condition: bool, name: &str, message: &str) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(InvariantError::Named {
            name: name.to_string(),
            message: message.to_string(),
        })
    }
}

/// Assert invariant (panics on failure).
#[track_caller]
pub fn assert_invariant(condition: bool, message: &str) {
    if !condition {
        panic!("Invariant violated: {}", message);
    }
}

/// Debug-only invariant check.
#[cfg(debug_assertions)]
#[track_caller]
pub fn debug_invariant(condition: bool, message: &str) {
    if !condition {
        panic!("Debug invariant violated: {}", message);
    }
}

#[cfg(not(debug_assertions))]
#[inline]
pub fn debug_invariant(_condition: bool, _message: &str) {}

/// Invariant checker that collects violations.
pub struct InvariantChecker {
    violations: Vec<InvariantError>,
}

impl InvariantChecker {
    /// Create new invariant checker.
    pub fn new() -> Self {
        Self {
            violations: Vec::new(),
        }
    }

    /// Check invariant.
    pub fn check(mut self, condition: bool, message: &str) -> Self {
        if !condition {
            self.violations
                .push(InvariantError::Violated(message.to_string()));
        }
        self
    }

    /// Check named invariant.
    pub fn check_named(mut self, condition: bool, name: &str, message: &str) -> Self {
        if !condition {
            self.violations.push(InvariantError::Named {
                name: name.to_string(),
                message: message.to_string(),
            });
        }
        self
    }

    /// Verify all invariants hold.
    pub fn verify(self) -> Result<()> {
        if self.violations.is_empty() {
            Ok(())
        } else if self.violations.len() == 1 {
            Err(self.violations.into_iter().next().unwrap())
        } else {
            let messages: Vec<_> = self.violations.iter().map(|e| e.to_string()).collect();
            Err(InvariantError::Violated(messages.join("; ")))
        }
    }

    /// Check if all invariants hold.
    pub fn is_valid(&self) -> bool {
        self.violations.is_empty()
    }

    /// Get violations.
    pub fn violations(&self) -> &[InvariantError] {
        &self.violations
    }
}

impl Default for InvariantChecker {
    fn default() -> Self {
        Self::new()
    }
}

/// Start invariant checking.
pub fn invariants() -> InvariantChecker {
    InvariantChecker::new()
}

/// Guard that checks invariant on drop.
pub struct InvariantGuard<T, F>
where
    F: Fn(&T) -> bool,
{
    value: Option<T>,
    check: F,
    message: &'static str,
}

impl<T, F> InvariantGuard<T, F>
where
    F: Fn(&T) -> bool,
{
    /// Create new invariant guard.
    pub fn new(value: T, check: F, message: &'static str) -> Self {
        Self {
            value: Some(value),
            check,
            message,
        }
    }

    /// Get reference to value.
    pub fn get(&self) -> &T {
        self.value.as_ref().unwrap()
    }

    /// Get mutable reference to value.
    pub fn get_mut(&mut self) -> &mut T {
        self.value.as_mut().unwrap()
    }

    /// Unwrap value, checking invariant first.
    pub fn into_inner(mut self) -> T {
        let value = self.value.take().unwrap();
        assert!((self.check)(&value), "Invariant violated: {}", self.message);
        value
    }
}

impl<T, F> Drop for InvariantGuard<T, F>
where
    F: Fn(&T) -> bool,
{
    fn drop(&mut self) {
        if let Some(ref value) = self.value {
            if !(self.check)(value) {
                // In debug mode, panic. In release, just log.
                #[cfg(debug_assertions)]
                panic!("Invariant violated on drop: {}", self.message);
            }
        }
    }
}

/// Create invariant guard.
pub fn guard<T, F>(value: T, check: F, message: &'static str) -> InvariantGuard<T, F>
where
    F: Fn(&T) -> bool,
{
    InvariantGuard::new(value, check, message)
}

/// Wrapper that maintains invariant.
pub struct Validated<T> {
    value: T,
    validator: Box<dyn Fn(&T) -> bool + Send + Sync>,
}

impl<T> Validated<T> {
    /// Create validated wrapper.
    pub fn new<F>(value: T, validator: F) -> Result<Self>
    where
        F: Fn(&T) -> bool + Send + Sync + 'static,
    {
        if validator(&value) {
            Ok(Self {
                value,
                validator: Box::new(validator),
            })
        } else {
            Err(InvariantError::Violated(
                "Initial value violates invariant".to_string(),
            ))
        }
    }

    /// Get reference to value.
    pub fn get(&self) -> &T {
        &self.value
    }

    /// Update value if new value satisfies invariant.
    pub fn set(&mut self, value: T) -> Result<()> {
        if (self.validator)(&value) {
            self.value = value;
            Ok(())
        } else {
            Err(InvariantError::Violated(
                "New value violates invariant".to_string(),
            ))
        }
    }

    /// Update with function.
    pub fn update<F>(&mut self, f: F) -> Result<()>
    where
        F: FnOnce(&T) -> T,
    {
        let new_value = f(&self.value);
        self.set(new_value)
    }

    /// Into inner value.
    pub fn into_inner(self) -> T {
        self.value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check() {
        assert!(check(true, "should pass").is_ok());
        assert!(check(false, "should fail").is_err());
    }

    #[test]
    fn test_invariant_trait() {
        struct Positive(i32);

        impl Invariant for Positive {
            fn check_invariant(&self) -> Result<()> {
                check(self.0 > 0, "value must be positive")
            }
        }

        let p = Positive(5);
        assert!(p.check_invariant().is_ok());

        let n = Positive(-1);
        assert!(n.check_invariant().is_err());
    }

    #[test]
    fn test_invariant_checker() {
        let result = invariants()
            .check(true, "first")
            .check(true, "second")
            .verify();
        assert!(result.is_ok());

        let result = invariants().check(false, "first").verify();
        assert!(result.is_err());
    }

    #[test]
    fn test_validated() {
        let mut v = Validated::new(5, |x| *x > 0).unwrap();
        assert_eq!(*v.get(), 5);

        assert!(v.set(10).is_ok());
        assert!(v.set(-1).is_err());
    }

    #[test]
    #[should_panic(expected = "Invariant violated")]
    fn test_assert_invariant() {
        assert_invariant(false, "test");
    }
}
