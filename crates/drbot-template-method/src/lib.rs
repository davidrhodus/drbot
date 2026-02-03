//! Template method pattern utilities for drbot.
//!
//! This crate provides:
//! - Template method trait
//! - Customizable algorithm skeletons
//! - Hook points

use thiserror::Error;

/// Template method error types.
#[derive(Error, Debug)]
pub enum TemplateError {
    #[error("Step failed: {0}")]
    StepFailed(String),

    #[error("Precondition failed")]
    PreconditionFailed,

    #[error("Postcondition failed")]
    PostconditionFailed,
}

/// Result type for template operations.
pub type Result<T> = std::result::Result<T, TemplateError>;

/// Template method trait defining algorithm skeleton.
pub trait TemplateMethod: Send + Sync {
    /// Input type.
    type Input;
    /// Output type.
    type Output;

    /// Execute the template method (final algorithm).
    fn execute(&self, input: Self::Input) -> Result<Self::Output> {
        self.before_hook(&input)?;
        self.validate(&input)?;
        let result = self.do_execute(input)?;
        self.after_hook(&result)?;
        Ok(result)
    }

    /// Validate input (can be overridden).
    fn validate(&self, _input: &Self::Input) -> Result<()> {
        Ok(())
    }

    /// Before hook (can be overridden).
    fn before_hook(&self, _input: &Self::Input) -> Result<()> {
        Ok(())
    }

    /// Main execution (must be implemented).
    fn do_execute(&self, input: Self::Input) -> Result<Self::Output>;

    /// After hook (can be overridden).
    fn after_hook(&self, _output: &Self::Output) -> Result<()> {
        Ok(())
    }
}

/// Simple template with just the main operation.
pub struct SimpleTemplate<I, O, F: Fn(I) -> Result<O> + Send + Sync> {
    func: F,
    _marker: std::marker::PhantomData<(I, O)>,
}

impl<I, O, F: Fn(I) -> Result<O> + Send + Sync> SimpleTemplate<I, O, F> {
    /// Create new simple template.
    pub fn new(func: F) -> Self {
        Self {
            func,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<I: Send + Sync, O: Send + Sync, F: Fn(I) -> Result<O> + Send + Sync> TemplateMethod
    for SimpleTemplate<I, O, F>
{
    type Input = I;
    type Output = O;

    fn do_execute(&self, input: Self::Input) -> Result<Self::Output> {
        (self.func)(input)
    }
}

/// Template with validation.
pub struct ValidatedTemplate<I, O, V, E>
where
    V: Fn(&I) -> bool + Send + Sync,
    E: Fn(I) -> Result<O> + Send + Sync,
{
    validator: V,
    executor: E,
    _marker: std::marker::PhantomData<(I, O)>,
}

impl<I, O, V, E> ValidatedTemplate<I, O, V, E>
where
    V: Fn(&I) -> bool + Send + Sync,
    E: Fn(I) -> Result<O> + Send + Sync,
{
    /// Create new validated template.
    pub fn new(validator: V, executor: E) -> Self {
        Self {
            validator,
            executor,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<I: Send + Sync, O: Send + Sync, V, E> TemplateMethod for ValidatedTemplate<I, O, V, E>
where
    V: Fn(&I) -> bool + Send + Sync,
    E: Fn(I) -> Result<O> + Send + Sync,
{
    type Input = I;
    type Output = O;

    fn validate(&self, input: &Self::Input) -> Result<()> {
        if (self.validator)(input) {
            Ok(())
        } else {
            Err(TemplateError::PreconditionFailed)
        }
    }

    fn do_execute(&self, input: Self::Input) -> Result<Self::Output> {
        (self.executor)(input)
    }
}

/// Template with hooks.
pub struct HookedTemplate<I, O, B, E, A>
where
    B: Fn(&I) -> Result<()> + Send + Sync,
    E: Fn(I) -> Result<O> + Send + Sync,
    A: Fn(&O) -> Result<()> + Send + Sync,
{
    before: B,
    executor: E,
    after: A,
    _marker: std::marker::PhantomData<(I, O)>,
}

impl<I, O, B, E, A> HookedTemplate<I, O, B, E, A>
where
    B: Fn(&I) -> Result<()> + Send + Sync,
    E: Fn(I) -> Result<O> + Send + Sync,
    A: Fn(&O) -> Result<()> + Send + Sync,
{
    /// Create new hooked template.
    pub fn new(before: B, executor: E, after: A) -> Self {
        Self {
            before,
            executor,
            after,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<I: Send + Sync, O: Send + Sync, B, E, A> TemplateMethod for HookedTemplate<I, O, B, E, A>
where
    B: Fn(&I) -> Result<()> + Send + Sync,
    E: Fn(I) -> Result<O> + Send + Sync,
    A: Fn(&O) -> Result<()> + Send + Sync,
{
    type Input = I;
    type Output = O;

    fn before_hook(&self, input: &Self::Input) -> Result<()> {
        (self.before)(input)
    }

    fn do_execute(&self, input: Self::Input) -> Result<Self::Output> {
        (self.executor)(input)
    }

    fn after_hook(&self, output: &Self::Output) -> Result<()> {
        (self.after)(output)
    }
}

/// Multi-step template.
pub struct MultiStepTemplate<T> {
    steps: Vec<Box<dyn Fn(T) -> Result<T> + Send + Sync>>,
}

impl<T> MultiStepTemplate<T> {
    /// Create new multi-step template.
    pub fn new() -> Self {
        Self { steps: Vec::new() }
    }

    /// Add step.
    pub fn add_step<F>(&mut self, step: F)
    where
        F: Fn(T) -> Result<T> + Send + Sync + 'static,
    {
        self.steps.push(Box::new(step));
    }

    /// Add step (builder pattern).
    pub fn with_step<F>(mut self, step: F) -> Self
    where
        F: Fn(T) -> Result<T> + Send + Sync + 'static,
    {
        self.add_step(step);
        self
    }

    /// Execute all steps.
    pub fn execute(&self, mut value: T) -> Result<T> {
        for (i, step) in self.steps.iter().enumerate() {
            value =
                step(value).map_err(|e| TemplateError::StepFailed(format!("Step {}: {}", i, e)))?;
        }
        Ok(value)
    }

    /// Get step count.
    pub fn step_count(&self) -> usize {
        self.steps.len()
    }
}

impl<T> Default for MultiStepTemplate<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Retry template for operations that may fail.
pub struct RetryTemplate<I: Clone, O, F: Fn(I) -> Result<O> + Send + Sync> {
    executor: F,
    max_attempts: usize,
    _marker: std::marker::PhantomData<(I, O)>,
}

impl<I: Clone, O, F: Fn(I) -> Result<O> + Send + Sync> RetryTemplate<I, O, F> {
    /// Create new retry template.
    pub fn new(executor: F, max_attempts: usize) -> Self {
        Self {
            executor,
            max_attempts,
            _marker: std::marker::PhantomData,
        }
    }

    /// Execute with retries.
    pub fn execute(&self, input: I) -> Result<O> {
        let mut last_error = None;
        for _ in 0..self.max_attempts {
            match (self.executor)(input.clone()) {
                Ok(output) => return Ok(output),
                Err(e) => last_error = Some(e),
            }
        }
        Err(last_error.unwrap_or_else(|| TemplateError::StepFailed("No attempts made".to_string())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_template() {
        let template = SimpleTemplate::new(|x: i32| Ok(x * 2));
        assert_eq!(template.execute(21).unwrap(), 42);
    }

    #[test]
    fn test_validated_template() {
        let template = ValidatedTemplate::new(|x: &i32| *x > 0, |x: i32| Ok(x * 2));

        assert_eq!(template.execute(21).unwrap(), 42);
        assert!(template.execute(-1).is_err());
    }

    #[test]
    fn test_multi_step_template() {
        let template = MultiStepTemplate::new()
            .with_step(|x: i32| Ok(x + 1))
            .with_step(|x: i32| Ok(x * 2))
            .with_step(|x: i32| Ok(x - 1));

        assert_eq!(template.execute(10).unwrap(), 21); // ((10 + 1) * 2) - 1
    }

    #[test]
    fn test_retry_template() {
        use std::sync::atomic::{AtomicI32, Ordering};

        let attempts = AtomicI32::new(0);
        let template = RetryTemplate::new(
            |_: ()| {
                if attempts.fetch_add(1, Ordering::SeqCst) < 2 {
                    Err(TemplateError::StepFailed("Not yet".to_string()))
                } else {
                    Ok(42)
                }
            },
            5,
        );

        assert_eq!(template.execute(()).unwrap(), 42);
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }
}
