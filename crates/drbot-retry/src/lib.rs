//! Retry logic with backoff for drbot.
//!
//! This crate provides:
//! - Configurable retry policies
//! - Exponential backoff
//! - Jitter support

use std::thread;
use std::time::Duration;
use thiserror::Error;

/// Retry error types.
#[derive(Error, Debug)]
pub enum RetryError<E> {
    #[error("Max retries ({0}) exceeded")]
    MaxRetriesExceeded(u32),

    #[error("Operation failed: {0}")]
    OperationFailed(#[source] E),

    #[error("Retry aborted")]
    Aborted,
}

/// Result type for retry operations.
pub type Result<T, E> = std::result::Result<T, RetryError<E>>;

/// Backoff strategy.
#[derive(Debug, Clone)]
pub enum Backoff {
    /// Constant delay between retries.
    Constant(Duration),
    /// Linear backoff: delay * attempt.
    Linear { initial: Duration, max: Duration },
    /// Exponential backoff: initial * 2^attempt.
    Exponential { initial: Duration, max: Duration },
    /// No delay.
    None,
}

impl Backoff {
    /// Calculate delay for attempt.
    pub fn delay(&self, attempt: u32) -> Duration {
        match self {
            Backoff::Constant(d) => *d,
            Backoff::Linear { initial, max } => {
                let delay = initial.saturating_mul(attempt);
                delay.min(*max)
            }
            Backoff::Exponential { initial, max } => {
                let multiplier = 2u64.saturating_pow(attempt);
                let delay = Duration::from_millis(initial.as_millis() as u64 * multiplier);
                delay.min(*max)
            }
            Backoff::None => Duration::ZERO,
        }
    }
}

/// Retry policy configuration.
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    /// Maximum number of retries.
    pub max_retries: u32,
    /// Backoff strategy.
    pub backoff: Backoff,
    /// Add random jitter (0.0-1.0 multiplier).
    pub jitter: Option<f64>,
}

impl RetryPolicy {
    /// Create new policy with max retries and backoff.
    pub fn new(max_retries: u32, backoff: Backoff) -> Self {
        Self {
            max_retries,
            backoff,
            jitter: None,
        }
    }

    /// Set jitter factor.
    pub fn with_jitter(mut self, jitter: f64) -> Self {
        self.jitter = Some(jitter.clamp(0.0, 1.0));
        self
    }

    /// Calculate delay with optional jitter.
    fn calculate_delay(&self, attempt: u32) -> Duration {
        let base_delay = self.backoff.delay(attempt);
        if let Some(jitter) = self.jitter {
            // Simple pseudo-random jitter using attempt as seed
            let random_factor = ((attempt as f64 * 7.0) % 1.0) * jitter;
            let jitter_millis = (base_delay.as_millis() as f64 * random_factor) as u64;
            base_delay + Duration::from_millis(jitter_millis)
        } else {
            base_delay
        }
    }
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 3,
            backoff: Backoff::Exponential {
                initial: Duration::from_millis(100),
                max: Duration::from_secs(10),
            },
            jitter: None,
        }
    }
}

/// Execute operation with retry.
pub fn retry<T, E, F>(policy: &RetryPolicy, mut operation: F) -> Result<T, E>
where
    F: FnMut() -> std::result::Result<T, E>,
{
    let mut last_error = None;

    for attempt in 0..=policy.max_retries {
        match operation() {
            Ok(value) => return Ok(value),
            Err(e) => {
                last_error = Some(e);
                if attempt < policy.max_retries {
                    let delay = policy.calculate_delay(attempt);
                    if !delay.is_zero() {
                        thread::sleep(delay);
                    }
                }
            }
        }
    }

    Err(RetryError::MaxRetriesExceeded(policy.max_retries))
}

/// Execute operation with retry and condition.
pub fn retry_if<T, E, F, C>(policy: &RetryPolicy, mut operation: F, should_retry: C) -> Result<T, E>
where
    F: FnMut() -> std::result::Result<T, E>,
    C: Fn(&E) -> bool,
{
    for attempt in 0..=policy.max_retries {
        match operation() {
            Ok(value) => return Ok(value),
            Err(e) => {
                if !should_retry(&e) {
                    return Err(RetryError::OperationFailed(e));
                }
                if attempt < policy.max_retries {
                    let delay = policy.calculate_delay(attempt);
                    if !delay.is_zero() {
                        thread::sleep(delay);
                    }
                }
            }
        }
    }

    Err(RetryError::MaxRetriesExceeded(policy.max_retries))
}

/// Retry builder for fluent API.
pub struct Retry<F, E> {
    operation: F,
    policy: RetryPolicy,
    _marker: std::marker::PhantomData<E>,
}

impl<T, E, F> Retry<F, E>
where
    F: FnMut() -> std::result::Result<T, E>,
{
    /// Create new retry builder.
    pub fn new(operation: F) -> Self {
        Self {
            operation,
            policy: RetryPolicy::default(),
            _marker: std::marker::PhantomData,
        }
    }

    /// Set max retries.
    pub fn max_retries(mut self, n: u32) -> Self {
        self.policy.max_retries = n;
        self
    }

    /// Set constant backoff.
    pub fn constant_backoff(mut self, delay: Duration) -> Self {
        self.policy.backoff = Backoff::Constant(delay);
        self
    }

    /// Set exponential backoff.
    pub fn exponential_backoff(mut self, initial: Duration, max: Duration) -> Self {
        self.policy.backoff = Backoff::Exponential { initial, max };
        self
    }

    /// Set linear backoff.
    pub fn linear_backoff(mut self, initial: Duration, max: Duration) -> Self {
        self.policy.backoff = Backoff::Linear { initial, max };
        self
    }

    /// Set no backoff.
    pub fn no_backoff(mut self) -> Self {
        self.policy.backoff = Backoff::None;
        self
    }

    /// Add jitter.
    pub fn with_jitter(mut self, jitter: f64) -> Self {
        self.policy = self.policy.with_jitter(jitter);
        self
    }

    /// Execute with retry.
    pub fn execute(self) -> Result<T, E> {
        let mut operation = self.operation;
        retry(&self.policy, || operation())
    }
}

/// Create retry builder.
pub fn with_retry<T, E, F>(operation: F) -> Retry<F, E>
where
    F: FnMut() -> std::result::Result<T, E>,
{
    Retry::new(operation)
}

/// Simple retry with count.
pub fn retry_n<T, E, F>(n: u32, mut operation: F) -> Result<T, E>
where
    F: FnMut() -> std::result::Result<T, E>,
{
    retry(&RetryPolicy::new(n, Backoff::None), || operation())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn test_retry_success() {
        let counter = Cell::new(0);
        let result = retry(&RetryPolicy::default(), || {
            counter.set(counter.get() + 1);
            if counter.get() >= 2 {
                Ok(42)
            } else {
                Err("not yet")
            }
        });

        assert_eq!(result.unwrap(), 42);
        assert_eq!(counter.get(), 2);
    }

    #[test]
    fn test_retry_exhausted() {
        let result: Result<(), &str> =
            retry(&RetryPolicy::new(2, Backoff::None), || Err("always fails"));

        assert!(matches!(result, Err(RetryError::MaxRetriesExceeded(2))));
    }

    #[test]
    fn test_backoff_exponential() {
        let backoff = Backoff::Exponential {
            initial: Duration::from_millis(100),
            max: Duration::from_secs(10),
        };

        assert_eq!(backoff.delay(0), Duration::from_millis(100));
        assert_eq!(backoff.delay(1), Duration::from_millis(200));
        assert_eq!(backoff.delay(2), Duration::from_millis(400));
    }

    #[test]
    fn test_fluent_api() {
        let counter = Cell::new(0);
        let result = with_retry(|| {
            counter.set(counter.get() + 1);
            if counter.get() >= 2 {
                Ok(42)
            } else {
                Err::<i32, _>("not yet")
            }
        })
        .max_retries(5)
        .no_backoff()
        .execute();

        assert_eq!(result.unwrap(), 42);
    }
}
