//! Timeout handling utilities for drbot.
//!
//! This crate provides:
//! - Configurable timeouts
//! - Timeout policies
//! - Deadline propagation
//! - Timeout budgets

use chrono::{DateTime, Utc};
use futures::Future;
use serde::{Deserialize, Serialize};
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::RwLock;

/// Timeout error types.
#[derive(Error, Debug, Clone)]
pub enum TimeoutError {
    #[error("Operation timed out after {0:?}")]
    Elapsed(Duration),

    #[error("Deadline exceeded at {0}")]
    DeadlineExceeded(DateTime<Utc>),

    #[error("Budget exhausted: {0:?} remaining of {1:?}")]
    BudgetExhausted(Duration, Duration),

    #[error("Cancelled")]
    Cancelled,
}

/// Result type for timeout operations.
pub type Result<T> = std::result::Result<T, TimeoutError>;

/// Timeout configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeoutConfig {
    /// Default timeout duration.
    pub default: Duration,
    /// Connect timeout.
    pub connect: Duration,
    /// Read timeout.
    pub read: Duration,
    /// Write timeout.
    pub write: Duration,
    /// Idle timeout.
    pub idle: Duration,
}

impl Default for TimeoutConfig {
    fn default() -> Self {
        Self {
            default: Duration::from_secs(30),
            connect: Duration::from_secs(10),
            read: Duration::from_secs(30),
            write: Duration::from_secs(30),
            idle: Duration::from_secs(60),
        }
    }
}

/// A deadline for an operation.
#[derive(Debug, Clone, Copy)]
pub struct Deadline {
    /// When the deadline expires.
    expires_at: std::time::Instant,
    /// Original duration.
    duration: Duration,
}

impl Deadline {
    /// Create a new deadline from duration.
    pub fn from_duration(duration: Duration) -> Self {
        Self {
            expires_at: std::time::Instant::now() + duration,
            duration,
        }
    }

    /// Create a deadline that expires at a specific instant.
    pub fn at(instant: std::time::Instant) -> Self {
        let now = std::time::Instant::now();
        let duration = instant.saturating_duration_since(now);
        Self {
            expires_at: instant,
            duration,
        }
    }

    /// Check if the deadline has passed.
    pub fn is_expired(&self) -> bool {
        std::time::Instant::now() >= self.expires_at
    }

    /// Get remaining time.
    pub fn remaining(&self) -> Duration {
        self.expires_at
            .saturating_duration_since(std::time::Instant::now())
    }

    /// Get elapsed time.
    pub fn elapsed(&self) -> Duration {
        let start = self.expires_at - self.duration;
        std::time::Instant::now().saturating_duration_since(start)
    }

    /// Get the original duration.
    pub fn duration(&self) -> Duration {
        self.duration
    }

    /// Get the expiration instant.
    pub fn expires_at(&self) -> std::time::Instant {
        self.expires_at
    }

    /// Create a child deadline with a shorter duration.
    pub fn child(&self, max_duration: Duration) -> Self {
        let remaining = self.remaining();
        let duration = remaining.min(max_duration);
        Self::from_duration(duration)
    }
}

/// Run a future with a timeout.
pub async fn with_timeout<F, T>(duration: Duration, future: F) -> Result<T>
where
    F: Future<Output = T>,
{
    tokio::time::timeout(duration, future)
        .await
        .map_err(|_| TimeoutError::Elapsed(duration))
}

/// Run a future with a deadline.
pub async fn with_deadline<F, T>(deadline: Deadline, future: F) -> Result<T>
where
    F: Future<Output = T>,
{
    let remaining = deadline.remaining();
    if remaining.is_zero() {
        return Err(TimeoutError::DeadlineExceeded(Utc::now()));
    }

    tokio::time::timeout(remaining, future)
        .await
        .map_err(|_| TimeoutError::DeadlineExceeded(Utc::now()))
}

/// Timeout budget for distributing time across operations.
#[derive(Debug)]
pub struct TimeoutBudget {
    total: Duration,
    remaining: AtomicU64,
    started_at: std::time::Instant,
}

impl TimeoutBudget {
    /// Create a new timeout budget.
    pub fn new(total: Duration) -> Self {
        Self {
            total,
            remaining: AtomicU64::new(total.as_micros() as u64),
            started_at: std::time::Instant::now(),
        }
    }

    /// Get remaining budget.
    pub fn remaining(&self) -> Duration {
        Duration::from_micros(self.remaining.load(Ordering::Relaxed))
    }

    /// Get elapsed time.
    pub fn elapsed(&self) -> Duration {
        self.started_at.elapsed()
    }

    /// Get total budget.
    pub fn total(&self) -> Duration {
        self.total
    }

    /// Check if budget is exhausted.
    pub fn is_exhausted(&self) -> bool {
        self.remaining.load(Ordering::Relaxed) == 0
    }

    /// Consume time from the budget.
    pub fn consume(&self, duration: Duration) -> Result<()> {
        let micros = duration.as_micros() as u64;
        let mut current = self.remaining.load(Ordering::Relaxed);

        loop {
            if current < micros {
                return Err(TimeoutError::BudgetExhausted(
                    Duration::from_micros(current),
                    self.total,
                ));
            }

            match self.remaining.compare_exchange_weak(
                current,
                current - micros,
                Ordering::SeqCst,
                Ordering::Relaxed,
            ) {
                Ok(_) => return Ok(()),
                Err(c) => current = c,
            }
        }
    }

    /// Reserve time from the budget.
    pub fn reserve(&self, duration: Duration) -> Result<BudgetReservation> {
        self.consume(duration)?;
        Ok(BudgetReservation {
            budget: self,
            reserved: duration,
            consumed: Duration::ZERO,
        })
    }

    /// Run a future within the budget.
    pub async fn run<F, T>(&self, future: F) -> Result<T>
    where
        F: Future<Output = T>,
    {
        let remaining = self.remaining();
        if remaining.is_zero() {
            return Err(TimeoutError::BudgetExhausted(Duration::ZERO, self.total));
        }

        let start = std::time::Instant::now();
        let result = with_timeout(remaining, future).await?;
        let elapsed = start.elapsed();

        // Consume the actual time used
        let _ = self.consume(elapsed);

        Ok(result)
    }
}

/// A reservation of time from a budget.
pub struct BudgetReservation<'a> {
    budget: &'a TimeoutBudget,
    reserved: Duration,
    consumed: Duration,
}

impl<'a> BudgetReservation<'a> {
    /// Get reserved time.
    pub fn reserved(&self) -> Duration {
        self.reserved
    }

    /// Get remaining reserved time.
    pub fn remaining(&self) -> Duration {
        self.reserved.saturating_sub(self.consumed)
    }

    /// Record consumed time.
    pub fn consume(&mut self, duration: Duration) {
        self.consumed += duration;
    }

    /// Complete the reservation, returning unused time to the budget.
    pub fn complete(self) {
        let unused = self.reserved.saturating_sub(self.consumed);
        if !unused.is_zero() {
            // Return unused time
            self.budget
                .remaining
                .fetch_add(unused.as_micros() as u64, Ordering::Relaxed);
        }
    }
}

/// Timeout policy.
#[derive(Debug, Clone)]
pub enum TimeoutPolicy {
    /// Fixed timeout for all attempts.
    Fixed(Duration),
    /// Per-attempt timeout.
    PerAttempt(Duration),
    /// Total timeout across all attempts.
    Total(Duration),
    /// Exponential backoff with max.
    Exponential {
        initial: Duration,
        factor: f64,
        max: Duration,
    },
}

impl TimeoutPolicy {
    /// Get timeout for attempt number (0-indexed).
    pub fn timeout_for_attempt(&self, attempt: u32) -> Duration {
        match self {
            TimeoutPolicy::Fixed(d) => *d,
            TimeoutPolicy::PerAttempt(d) => *d,
            TimeoutPolicy::Total(d) => *d,
            TimeoutPolicy::Exponential {
                initial,
                factor,
                max,
            } => {
                let timeout = initial.as_secs_f64() * factor.powi(attempt as i32);
                Duration::from_secs_f64(timeout.min(max.as_secs_f64()))
            }
        }
    }
}

/// Timeout context for propagating deadlines.
#[derive(Clone)]
pub struct TimeoutContext {
    deadline: Option<Deadline>,
    budget: Option<Arc<TimeoutBudget>>,
    config: Arc<TimeoutConfig>,
}

impl TimeoutContext {
    /// Create a new timeout context.
    pub fn new(config: TimeoutConfig) -> Self {
        Self {
            deadline: None,
            budget: None,
            config: Arc::new(config),
        }
    }

    /// Create with a deadline.
    pub fn with_deadline(mut self, deadline: Deadline) -> Self {
        self.deadline = Some(deadline);
        self
    }

    /// Create with a budget.
    pub fn with_budget(mut self, budget: Arc<TimeoutBudget>) -> Self {
        self.budget = Some(budget);
        self
    }

    /// Get the effective timeout.
    pub fn effective_timeout(&self) -> Duration {
        if let Some(deadline) = &self.deadline {
            deadline.remaining()
        } else if let Some(budget) = &self.budget {
            budget.remaining()
        } else {
            self.config.default
        }
    }

    /// Check if the context has expired.
    pub fn is_expired(&self) -> bool {
        if let Some(deadline) = &self.deadline {
            if deadline.is_expired() {
                return true;
            }
        }
        if let Some(budget) = &self.budget {
            if budget.is_exhausted() {
                return true;
            }
        }
        false
    }

    /// Run a future with this context's timeout.
    pub async fn run<F, T>(&self, future: F) -> Result<T>
    where
        F: Future<Output = T>,
    {
        let timeout = self.effective_timeout();
        if timeout.is_zero() {
            return Err(TimeoutError::DeadlineExceeded(Utc::now()));
        }

        with_timeout(timeout, future).await
    }

    /// Create a child context with a shorter timeout.
    pub fn child(&self, max_duration: Duration) -> Self {
        let new_deadline = self
            .deadline
            .map(|d| d.child(max_duration))
            .unwrap_or_else(|| Deadline::from_duration(max_duration));

        Self {
            deadline: Some(new_deadline),
            budget: self.budget.clone(),
            config: self.config.clone(),
        }
    }

    /// Get the config.
    pub fn config(&self) -> &TimeoutConfig {
        &self.config
    }
}

impl Default for TimeoutContext {
    fn default() -> Self {
        Self::new(TimeoutConfig::default())
    }
}

/// Statistics for timeout tracking.
#[derive(Debug, Default)]
pub struct TimeoutStats {
    total_operations: AtomicU64,
    timed_out: AtomicU64,
    total_duration_ms: AtomicU64,
}

impl TimeoutStats {
    /// Create new stats.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a completed operation.
    pub fn record_success(&self, duration: Duration) {
        self.total_operations.fetch_add(1, Ordering::Relaxed);
        self.total_duration_ms
            .fetch_add(duration.as_millis() as u64, Ordering::Relaxed);
    }

    /// Record a timed out operation.
    pub fn record_timeout(&self) {
        self.total_operations.fetch_add(1, Ordering::Relaxed);
        self.timed_out.fetch_add(1, Ordering::Relaxed);
    }

    /// Get total operations.
    pub fn total_operations(&self) -> u64 {
        self.total_operations.load(Ordering::Relaxed)
    }

    /// Get timeout count.
    pub fn timeouts(&self) -> u64 {
        self.timed_out.load(Ordering::Relaxed)
    }

    /// Get timeout rate.
    pub fn timeout_rate(&self) -> f64 {
        let total = self.total_operations();
        if total == 0 {
            0.0
        } else {
            self.timeouts() as f64 / total as f64
        }
    }

    /// Get average duration in milliseconds.
    pub fn avg_duration_ms(&self) -> f64 {
        let total = self.total_operations() - self.timeouts();
        if total == 0 {
            0.0
        } else {
            self.total_duration_ms.load(Ordering::Relaxed) as f64 / total as f64
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deadline_creation() {
        let deadline = Deadline::from_duration(Duration::from_secs(10));
        assert!(!deadline.is_expired());
        assert!(deadline.remaining() <= Duration::from_secs(10));
    }

    #[test]
    fn test_deadline_expired() {
        let deadline = Deadline::from_duration(Duration::from_nanos(1));
        std::thread::sleep(Duration::from_millis(1));
        assert!(deadline.is_expired());
    }

    #[test]
    fn test_deadline_child() {
        let parent = Deadline::from_duration(Duration::from_secs(10));
        let child = parent.child(Duration::from_secs(5));
        assert!(child.remaining() <= Duration::from_secs(5));

        let child2 = parent.child(Duration::from_secs(20));
        assert!(child2.remaining() <= Duration::from_secs(10));
    }

    #[tokio::test]
    async fn test_with_timeout_success() {
        let result = with_timeout(Duration::from_secs(1), async { 42 }).await;
        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn test_with_timeout_elapsed() {
        let result = with_timeout(Duration::from_millis(10), async {
            tokio::time::sleep(Duration::from_secs(1)).await;
            42
        })
        .await;

        assert!(matches!(result, Err(TimeoutError::Elapsed(_))));
    }

    #[tokio::test]
    async fn test_with_deadline() {
        let deadline = Deadline::from_duration(Duration::from_secs(1));
        let result = with_deadline(deadline, async { 42 }).await;
        assert_eq!(result.unwrap(), 42);
    }

    #[test]
    fn test_timeout_budget() {
        let budget = TimeoutBudget::new(Duration::from_secs(10));
        assert_eq!(budget.total(), Duration::from_secs(10));
        assert!(!budget.is_exhausted());

        budget.consume(Duration::from_secs(5)).unwrap();
        assert!(budget.remaining() <= Duration::from_secs(5));

        budget.consume(Duration::from_secs(5)).unwrap();
        assert!(budget.is_exhausted());
    }

    #[test]
    fn test_budget_exhausted() {
        let budget = TimeoutBudget::new(Duration::from_secs(1));
        let result = budget.consume(Duration::from_secs(2));
        assert!(matches!(result, Err(TimeoutError::BudgetExhausted(_, _))));
    }

    #[test]
    fn test_budget_reservation() {
        let budget = TimeoutBudget::new(Duration::from_secs(10));
        let mut reservation = budget.reserve(Duration::from_secs(5)).unwrap();

        assert_eq!(reservation.reserved(), Duration::from_secs(5));
        reservation.consume(Duration::from_secs(2));
        assert_eq!(reservation.remaining(), Duration::from_secs(3));

        reservation.complete();
        // Unused 3 seconds returned
        assert!(budget.remaining() >= Duration::from_secs(7));
    }

    #[test]
    fn test_timeout_policy_fixed() {
        let policy = TimeoutPolicy::Fixed(Duration::from_secs(5));
        assert_eq!(policy.timeout_for_attempt(0), Duration::from_secs(5));
        assert_eq!(policy.timeout_for_attempt(5), Duration::from_secs(5));
    }

    #[test]
    fn test_timeout_policy_exponential() {
        let policy = TimeoutPolicy::Exponential {
            initial: Duration::from_secs(1),
            factor: 2.0,
            max: Duration::from_secs(10),
        };

        assert_eq!(policy.timeout_for_attempt(0), Duration::from_secs(1));
        assert_eq!(policy.timeout_for_attempt(1), Duration::from_secs(2));
        assert_eq!(policy.timeout_for_attempt(2), Duration::from_secs(4));
        assert_eq!(policy.timeout_for_attempt(10), Duration::from_secs(10)); // Capped at max
    }

    #[tokio::test]
    async fn test_timeout_context() {
        let ctx = TimeoutContext::default();
        assert!(!ctx.is_expired());

        let result = ctx.run(async { 42 }).await;
        assert_eq!(result.unwrap(), 42);
    }

    #[test]
    fn test_timeout_stats() {
        let stats = TimeoutStats::new();

        stats.record_success(Duration::from_millis(100));
        stats.record_success(Duration::from_millis(200));
        stats.record_timeout();

        assert_eq!(stats.total_operations(), 3);
        assert_eq!(stats.timeouts(), 1);
        assert!((stats.timeout_rate() - 0.333).abs() < 0.01);
        assert!((stats.avg_duration_ms() - 150.0).abs() < 0.01);
    }
}
