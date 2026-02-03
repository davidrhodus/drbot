//! Specification pattern utilities for drbot.
//!
//! This crate provides:
//! - Specification trait
//! - Composite specifications (and, or, not)
//! - Specification builders

use std::sync::Arc;
use thiserror::Error;

/// Specification error types.
#[derive(Error, Debug)]
pub enum SpecificationError {
    #[error("Specification not satisfied")]
    NotSatisfied,

    #[error("Invalid specification")]
    Invalid,
}

/// Result type for specification operations.
pub type Result<T> = std::result::Result<T, SpecificationError>;

/// Specification trait.
pub trait Specification<T>: Send + Sync {
    /// Check if candidate satisfies specification.
    fn is_satisfied_by(&self, candidate: &T) -> bool;
}

/// And specification.
pub struct AndSpec<T> {
    left: Arc<dyn Specification<T>>,
    right: Arc<dyn Specification<T>>,
}

impl<T> AndSpec<T> {
    /// Create new and specification.
    pub fn new(left: Arc<dyn Specification<T>>, right: Arc<dyn Specification<T>>) -> Self {
        Self { left, right }
    }
}

impl<T> Specification<T> for AndSpec<T> {
    fn is_satisfied_by(&self, candidate: &T) -> bool {
        self.left.is_satisfied_by(candidate) && self.right.is_satisfied_by(candidate)
    }
}

/// Or specification.
pub struct OrSpec<T> {
    left: Arc<dyn Specification<T>>,
    right: Arc<dyn Specification<T>>,
}

impl<T> OrSpec<T> {
    /// Create new or specification.
    pub fn new(left: Arc<dyn Specification<T>>, right: Arc<dyn Specification<T>>) -> Self {
        Self { left, right }
    }
}

impl<T> Specification<T> for OrSpec<T> {
    fn is_satisfied_by(&self, candidate: &T) -> bool {
        self.left.is_satisfied_by(candidate) || self.right.is_satisfied_by(candidate)
    }
}

/// Not specification.
pub struct NotSpec<T> {
    inner: Arc<dyn Specification<T>>,
}

impl<T> NotSpec<T> {
    /// Create new not specification.
    pub fn new(inner: Arc<dyn Specification<T>>) -> Self {
        Self { inner }
    }
}

impl<T> Specification<T> for NotSpec<T> {
    fn is_satisfied_by(&self, candidate: &T) -> bool {
        !self.inner.is_satisfied_by(candidate)
    }
}

/// Function-based specification.
pub struct FnSpec<T, F: Fn(&T) -> bool + Send + Sync> {
    func: F,
    _marker: std::marker::PhantomData<T>,
}

impl<T, F: Fn(&T) -> bool + Send + Sync> FnSpec<T, F> {
    /// Create new function specification.
    pub fn new(func: F) -> Self {
        Self {
            func,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<T: Send + Sync, F: Fn(&T) -> bool + Send + Sync> Specification<T> for FnSpec<T, F> {
    fn is_satisfied_by(&self, candidate: &T) -> bool {
        (self.func)(candidate)
    }
}

/// Always true specification.
pub struct TrueSpec<T>(std::marker::PhantomData<T>);

impl<T> TrueSpec<T> {
    /// Create new true specification.
    pub fn new() -> Self {
        Self(std::marker::PhantomData)
    }
}

impl<T> Default for TrueSpec<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Send + Sync> Specification<T> for TrueSpec<T> {
    fn is_satisfied_by(&self, _candidate: &T) -> bool {
        true
    }
}

/// Always false specification.
pub struct FalseSpec<T>(std::marker::PhantomData<T>);

impl<T> FalseSpec<T> {
    /// Create new false specification.
    pub fn new() -> Self {
        Self(std::marker::PhantomData)
    }
}

impl<T> Default for FalseSpec<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Send + Sync> Specification<T> for FalseSpec<T> {
    fn is_satisfied_by(&self, _candidate: &T) -> bool {
        false
    }
}

/// Specification builder for fluent API.
pub struct SpecBuilder<T> {
    spec: Arc<dyn Specification<T>>,
}

impl<T: Send + Sync + 'static> SpecBuilder<T> {
    /// Create from specification.
    pub fn new(spec: Arc<dyn Specification<T>>) -> Self {
        Self { spec }
    }

    /// Create from function.
    pub fn from_fn<F: Fn(&T) -> bool + Send + Sync + 'static>(func: F) -> Self {
        Self {
            spec: Arc::new(FnSpec::new(func)),
        }
    }

    /// And with another specification.
    pub fn and(self, other: Arc<dyn Specification<T>>) -> Self {
        Self {
            spec: Arc::new(AndSpec::new(self.spec, other)),
        }
    }

    /// And with function.
    pub fn and_fn<F: Fn(&T) -> bool + Send + Sync + 'static>(self, func: F) -> Self {
        self.and(Arc::new(FnSpec::new(func)))
    }

    /// Or with another specification.
    pub fn or(self, other: Arc<dyn Specification<T>>) -> Self {
        Self {
            spec: Arc::new(OrSpec::new(self.spec, other)),
        }
    }

    /// Or with function.
    pub fn or_fn<F: Fn(&T) -> bool + Send + Sync + 'static>(self, func: F) -> Self {
        self.or(Arc::new(FnSpec::new(func)))
    }

    /// Negate specification.
    pub fn not(self) -> Self {
        Self {
            spec: Arc::new(NotSpec::new(self.spec)),
        }
    }

    /// Build specification.
    pub fn build(self) -> Arc<dyn Specification<T>> {
        self.spec
    }

    /// Check if satisfied.
    pub fn is_satisfied_by(&self, candidate: &T) -> bool {
        self.spec.is_satisfied_by(candidate)
    }
}

/// Helper to create specification from function.
pub fn spec<T: Send + Sync + 'static, F: Fn(&T) -> bool + Send + Sync + 'static>(
    func: F,
) -> Arc<dyn Specification<T>> {
    Arc::new(FnSpec::new(func))
}

/// Extension trait for specifications.
pub trait SpecificationExt<T>: Specification<T> + Sized
where
    Self: 'static,
{
    /// And with another specification.
    fn and(self, other: Arc<dyn Specification<T>>) -> AndSpec<T>
    where
        Self: Sized,
    {
        AndSpec::new(Arc::new(self), other)
    }

    /// Or with another specification.
    fn or(self, other: Arc<dyn Specification<T>>) -> OrSpec<T>
    where
        Self: Sized,
    {
        OrSpec::new(Arc::new(self), other)
    }

    /// Negate specification.
    fn negate(self) -> NotSpec<T>
    where
        Self: Sized,
    {
        NotSpec::new(Arc::new(self))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fn_spec() {
        let spec = FnSpec::new(|x: &i32| *x > 10);
        assert!(spec.is_satisfied_by(&15));
        assert!(!spec.is_satisfied_by(&5));
    }

    #[test]
    fn test_and_spec() {
        let positive = spec(|x: &i32| *x > 0);
        let even = spec(|x: &i32| *x % 2 == 0);
        let positive_even = AndSpec::new(positive, even);

        assert!(positive_even.is_satisfied_by(&4));
        assert!(!positive_even.is_satisfied_by(&3));
        assert!(!positive_even.is_satisfied_by(&-2));
    }

    #[test]
    fn test_or_spec() {
        let positive = spec(|x: &i32| *x > 0);
        let even = spec(|x: &i32| *x % 2 == 0);
        let positive_or_even = OrSpec::new(positive, even);

        assert!(positive_or_even.is_satisfied_by(&3));
        assert!(positive_or_even.is_satisfied_by(&-2));
        assert!(!positive_or_even.is_satisfied_by(&-3));
    }

    #[test]
    fn test_not_spec() {
        let positive = spec(|x: &i32| *x > 0);
        let not_positive = NotSpec::new(positive);

        assert!(not_positive.is_satisfied_by(&-5));
        assert!(!not_positive.is_satisfied_by(&5));
    }

    #[test]
    fn test_spec_builder() {
        let spec = SpecBuilder::from_fn(|x: &i32| *x > 0)
            .and_fn(|x: &i32| *x < 100)
            .and_fn(|x: &i32| *x % 2 == 0)
            .build();

        assert!(spec.is_satisfied_by(&42));
        assert!(!spec.is_satisfied_by(&101));
        assert!(!spec.is_satisfied_by(&43));
    }
}
