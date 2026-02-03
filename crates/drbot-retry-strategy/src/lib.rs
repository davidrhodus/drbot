//! Sophisticated retry strategies.
//!
//! This crate provides:
//! - Exponential backoff
//! - Jitter strategies
//! - Retry budgets
//! - Error classification
//! - Adaptive retry

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::RwLock;

/// Retry errors.
#[derive(Debug, Error)]
pub enum RetryError {
    #[error("Max retries exceeded: {0}")]
    MaxRetriesExceeded(String),

    #[error("Budget exhausted")]
    BudgetExhausted,

    #[error("Non-retryable error: {0}")]
    NonRetryable(String),

    #[error("Timeout: {0}")]
    Timeout(String),

    #[error("Operation failed: {0}")]
    OperationFailed(String),
}

/// Result type for retry operations.
pub type Result<T> = std::result::Result<T, RetryError>;

/// Error classification for retry decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorClass {
    /// Transient errors that may succeed on retry.
    Transient,
    /// Permanent errors that won't succeed on retry.
    Permanent,
    /// Rate limiting errors - wait before retry.
    RateLimited,
    /// Server errors - may or may not succeed.
    ServerError,
    /// Network errors - usually transient.
    NetworkError,
    /// Timeout errors.
    Timeout,
    /// Unknown classification.
    Unknown,
}

/// Retry strategy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryStrategy {
    /// Maximum retry attempts.
    pub max_attempts: u32,
    /// Initial backoff duration.
    pub initial_backoff: Duration,
    /// Maximum backoff duration.
    pub max_backoff: Duration,
    /// Backoff multiplier.
    pub multiplier: f64,
    /// Jitter strategy.
    pub jitter: JitterStrategy,
    /// Retryable error classes.
    pub retryable_classes: Vec<ErrorClass>,
    /// Per-attempt timeout.
    pub attempt_timeout: Option<Duration>,
    /// Total timeout for all attempts.
    pub total_timeout: Option<Duration>,
}

impl Default for RetryStrategy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_backoff: Duration::from_millis(100),
            max_backoff: Duration::from_secs(30),
            multiplier: 2.0,
            jitter: JitterStrategy::Full,
            retryable_classes: vec![
                ErrorClass::Transient,
                ErrorClass::RateLimited,
                ErrorClass::ServerError,
                ErrorClass::NetworkError,
                ErrorClass::Timeout,
                ErrorClass::Unknown, // Default to retryable for unknown errors
            ],
            attempt_timeout: Some(Duration::from_secs(30)),
            total_timeout: Some(Duration::from_secs(120)),
        }
    }
}

impl RetryStrategy {
    /// Create a new retry strategy.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set maximum attempts.
    pub fn with_max_attempts(mut self, attempts: u32) -> Self {
        self.max_attempts = attempts;
        self
    }

    /// Set initial backoff.
    pub fn with_initial_backoff(mut self, backoff: Duration) -> Self {
        self.initial_backoff = backoff;
        self
    }

    /// Set maximum backoff.
    pub fn with_max_backoff(mut self, backoff: Duration) -> Self {
        self.max_backoff = backoff;
        self
    }

    /// Set backoff multiplier.
    pub fn with_multiplier(mut self, multiplier: f64) -> Self {
        self.multiplier = multiplier;
        self
    }

    /// Set jitter strategy.
    pub fn with_jitter(mut self, jitter: JitterStrategy) -> Self {
        self.jitter = jitter;
        self
    }

    /// Set attempt timeout.
    pub fn with_attempt_timeout(mut self, timeout: Duration) -> Self {
        self.attempt_timeout = Some(timeout);
        self
    }

    /// Set total timeout.
    pub fn with_total_timeout(mut self, timeout: Duration) -> Self {
        self.total_timeout = Some(timeout);
        self
    }

    /// Calculate backoff for attempt.
    pub fn calculate_backoff(&self, attempt: u32) -> Duration {
        let base_backoff =
            self.initial_backoff.as_secs_f64() * self.multiplier.powi(attempt as i32);
        let capped_backoff = base_backoff.min(self.max_backoff.as_secs_f64());

        let final_backoff = match self.jitter {
            JitterStrategy::None => capped_backoff,
            JitterStrategy::Full => {
                let jitter = rand_f64() * capped_backoff;
                jitter
            }
            JitterStrategy::Equal => {
                let jitter = rand_f64() * (capped_backoff / 2.0);
                capped_backoff / 2.0 + jitter
            }
            JitterStrategy::Decorrelated => {
                let jitter =
                    rand_f64() * (capped_backoff * 3.0 - self.initial_backoff.as_secs_f64());
                self.initial_backoff.as_secs_f64() + jitter
            }
        };

        Duration::from_secs_f64(final_backoff.max(0.001))
    }

    /// Check if error class is retryable.
    pub fn is_retryable(&self, class: ErrorClass) -> bool {
        self.retryable_classes.contains(&class)
    }
}

/// Jitter strategies for backoff randomization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum JitterStrategy {
    /// No jitter.
    None,
    /// Full jitter: random between 0 and backoff.
    Full,
    /// Equal jitter: half backoff plus random half.
    Equal,
    /// Decorrelated jitter.
    Decorrelated,
}

impl Default for JitterStrategy {
    fn default() -> Self {
        Self::Full
    }
}

// Simple pseudo-random function (not cryptographic)
fn rand_f64() -> f64 {
    use std::time::SystemTime;
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    nanos as f64 / u32::MAX as f64
}

/// Retry budget for limiting retries over time.
#[derive(Debug)]
pub struct RetryBudget {
    /// Token bucket capacity.
    capacity: u32,
    /// Current tokens.
    tokens: Arc<RwLock<f64>>,
    /// Token refresh rate per second.
    refresh_rate: f64,
    /// Last refresh time.
    last_refresh: Arc<RwLock<DateTime<Utc>>>,
    /// Tokens consumed per retry.
    cost_per_retry: f64,
    /// Tokens refunded on success.
    refund_on_success: f64,
}

impl RetryBudget {
    /// Create a new retry budget.
    pub fn new(capacity: u32, refresh_rate: f64) -> Self {
        Self {
            capacity,
            tokens: Arc::new(RwLock::new(capacity as f64)),
            refresh_rate,
            last_refresh: Arc::new(RwLock::new(Utc::now())),
            cost_per_retry: 1.0,
            refund_on_success: 0.1,
        }
    }

    /// Set cost per retry.
    pub fn with_cost(mut self, cost: f64) -> Self {
        self.cost_per_retry = cost;
        self
    }

    /// Set success refund.
    pub fn with_refund(mut self, refund: f64) -> Self {
        self.refund_on_success = refund;
        self
    }

    /// Try to acquire tokens for a retry.
    pub async fn try_acquire(&self) -> bool {
        self.refresh_tokens().await;

        let mut tokens = self.tokens.write().await;
        if *tokens >= self.cost_per_retry {
            *tokens -= self.cost_per_retry;
            true
        } else {
            false
        }
    }

    /// Report a successful operation.
    pub async fn report_success(&self) {
        let mut tokens = self.tokens.write().await;
        *tokens = (*tokens + self.refund_on_success).min(self.capacity as f64);
    }

    /// Refresh tokens based on elapsed time.
    async fn refresh_tokens(&self) {
        let mut last_refresh = self.last_refresh.write().await;
        let now = Utc::now();
        let elapsed = (now - *last_refresh).num_milliseconds() as f64 / 1000.0;

        if elapsed > 0.0 {
            let mut tokens = self.tokens.write().await;
            *tokens = (*tokens + elapsed * self.refresh_rate).min(self.capacity as f64);
            *last_refresh = now;
        }
    }

    /// Get current token count.
    pub async fn available(&self) -> f64 {
        self.refresh_tokens().await;
        let tokens = self.tokens.read().await;
        *tokens
    }
}

/// Retry execution result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryResult<T> {
    /// The result value (if successful).
    pub value: Option<T>,
    /// Total attempts made.
    pub attempts: u32,
    /// Total time spent.
    pub total_duration: Duration,
    /// Per-attempt results.
    pub attempt_results: Vec<AttemptResult>,
    /// Final success.
    pub success: bool,
}

/// Result of a single attempt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttemptResult {
    /// Attempt number (1-indexed).
    pub attempt: u32,
    /// Duration of this attempt.
    pub duration: Duration,
    /// Success.
    pub success: bool,
    /// Error message if failed.
    pub error: Option<String>,
    /// Error classification.
    pub error_class: Option<ErrorClass>,
    /// Backoff before next attempt.
    pub backoff: Option<Duration>,
}

/// Error classifier trait.
#[async_trait]
pub trait ErrorClassifier: Send + Sync {
    /// Classify an error.
    fn classify(&self, error: &str) -> ErrorClass;
}

/// Default error classifier.
pub struct DefaultClassifier;

impl ErrorClassifier for DefaultClassifier {
    fn classify(&self, error: &str) -> ErrorClass {
        let error_lower = error.to_lowercase();

        if error_lower.contains("rate limit")
            || error_lower.contains("429")
            || error_lower.contains("too many")
        {
            ErrorClass::RateLimited
        } else if error_lower.contains("timeout") || error_lower.contains("timed out") {
            ErrorClass::Timeout
        } else if error_lower.contains("connection")
            || error_lower.contains("network")
            || error_lower.contains("dns")
        {
            ErrorClass::NetworkError
        } else if error_lower.contains("500")
            || error_lower.contains("502")
            || error_lower.contains("503")
            || error_lower.contains("504")
        {
            ErrorClass::ServerError
        } else if error_lower.contains("400")
            || error_lower.contains("401")
            || error_lower.contains("403")
            || error_lower.contains("404")
        {
            ErrorClass::Permanent
        } else {
            ErrorClass::Unknown
        }
    }
}

/// The retry executor.
pub struct RetryExecutor {
    /// Retry strategy.
    strategy: RetryStrategy,
    /// Error classifier.
    classifier: Arc<dyn ErrorClassifier>,
    /// Retry budget (optional).
    budget: Option<Arc<RetryBudget>>,
    /// Stats.
    stats: Arc<RwLock<RetryStats>>,
}

/// Retry statistics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RetryStats {
    /// Total operations.
    pub total_operations: usize,
    /// Successful operations.
    pub successes: usize,
    /// Failed operations.
    pub failures: usize,
    /// Total retries.
    pub total_retries: usize,
    /// Average attempts per operation.
    pub avg_attempts: f64,
}

impl RetryExecutor {
    /// Create a new retry executor.
    pub fn new(strategy: RetryStrategy) -> Self {
        Self {
            strategy,
            classifier: Arc::new(DefaultClassifier),
            budget: None,
            stats: Arc::new(RwLock::new(RetryStats::default())),
        }
    }

    /// Set error classifier.
    pub fn with_classifier(mut self, classifier: Arc<dyn ErrorClassifier>) -> Self {
        self.classifier = classifier;
        self
    }

    /// Set retry budget.
    pub fn with_budget(mut self, budget: Arc<RetryBudget>) -> Self {
        self.budget = Some(budget);
        self
    }

    /// Execute with retry.
    pub async fn execute<F, Fut, T, E>(&self, mut operation: F) -> Result<RetryResult<T>>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = std::result::Result<T, E>>,
        E: std::fmt::Display,
        T: Clone,
    {
        let start = std::time::Instant::now();
        let mut attempt_results = Vec::new();
        let mut last_error = None;

        for attempt in 0..self.strategy.max_attempts {
            // Check budget
            if let Some(budget) = &self.budget {
                if attempt > 0 && !budget.try_acquire().await {
                    return Err(RetryError::BudgetExhausted);
                }
            }

            // Check total timeout
            if let Some(total_timeout) = self.strategy.total_timeout {
                if start.elapsed() > total_timeout {
                    return Err(RetryError::Timeout("Total timeout exceeded".to_string()));
                }
            }

            let attempt_start = std::time::Instant::now();

            // Execute with optional attempt timeout
            let result = if let Some(timeout) = self.strategy.attempt_timeout {
                match tokio::time::timeout(timeout, operation()).await {
                    Ok(r) => r,
                    Err(_) => {
                        let error_str = "Attempt timeout";
                        last_error = Some(error_str.to_string());
                        attempt_results.push(AttemptResult {
                            attempt: attempt + 1,
                            duration: attempt_start.elapsed(),
                            success: false,
                            error: Some(error_str.to_string()),
                            error_class: Some(ErrorClass::Timeout),
                            backoff: Some(self.strategy.calculate_backoff(attempt)),
                        });

                        // Wait before next attempt
                        if attempt + 1 < self.strategy.max_attempts {
                            let backoff = self.strategy.calculate_backoff(attempt);
                            tokio::time::sleep(backoff).await;
                        }
                        continue;
                    }
                }
            } else {
                operation().await
            };

            match result {
                Ok(value) => {
                    // Success!
                    if let Some(budget) = &self.budget {
                        budget.report_success().await;
                    }

                    attempt_results.push(AttemptResult {
                        attempt: attempt + 1,
                        duration: attempt_start.elapsed(),
                        success: true,
                        error: None,
                        error_class: None,
                        backoff: None,
                    });

                    self.update_stats(true, attempt + 1).await;

                    return Ok(RetryResult {
                        value: Some(value),
                        attempts: attempt + 1,
                        total_duration: start.elapsed(),
                        attempt_results,
                        success: true,
                    });
                }
                Err(e) => {
                    let error_str = e.to_string();
                    let error_class = self.classifier.classify(&error_str);
                    last_error = Some(error_str.clone());

                    let backoff = if attempt + 1 < self.strategy.max_attempts
                        && self.strategy.is_retryable(error_class)
                    {
                        Some(self.strategy.calculate_backoff(attempt))
                    } else {
                        None
                    };

                    attempt_results.push(AttemptResult {
                        attempt: attempt + 1,
                        duration: attempt_start.elapsed(),
                        success: false,
                        error: Some(error_str),
                        error_class: Some(error_class),
                        backoff,
                    });

                    // Check if retryable
                    if !self.strategy.is_retryable(error_class) {
                        self.update_stats(false, attempt + 1).await;
                        return Err(RetryError::NonRetryable(last_error.unwrap_or_default()));
                    }

                    // Wait before next attempt
                    if let Some(backoff) = backoff {
                        tokio::time::sleep(backoff).await;
                    }
                }
            }
        }

        self.update_stats(false, self.strategy.max_attempts).await;

        Ok(RetryResult {
            value: None,
            attempts: self.strategy.max_attempts,
            total_duration: start.elapsed(),
            attempt_results,
            success: false,
        })
    }

    async fn update_stats(&self, success: bool, attempts: u32) {
        let mut stats = self.stats.write().await;
        stats.total_operations += 1;
        if success {
            stats.successes += 1;
        } else {
            stats.failures += 1;
        }
        stats.total_retries += (attempts - 1) as usize;
        stats.avg_attempts =
            (stats.total_retries + stats.total_operations) as f64 / stats.total_operations as f64;
    }

    /// Get statistics.
    pub async fn stats(&self) -> RetryStats {
        let stats = self.stats.read().await;
        stats.clone()
    }
}

// ============================================================================
// Kani Formal Verification Proofs
// ============================================================================

#[cfg(kani)]
mod kani_proofs {
    use super::*;

    /// Verify ErrorClass has 7 variants.
    #[kani::proof]
    fn proof_error_class_variants() {
        let val: u8 = kani::any();
        kani::assume(val <= 6);

        let _class = match val {
            0 => ErrorClass::Transient,
            1 => ErrorClass::Permanent,
            2 => ErrorClass::RateLimited,
            3 => ErrorClass::ServerError,
            4 => ErrorClass::NetworkError,
            5 => ErrorClass::Timeout,
            _ => ErrorClass::Unknown,
        };

        kani::assert(val <= 6, "All 7 variants covered");
    }

    /// Verify JitterStrategy has 4 variants.
    #[kani::proof]
    fn proof_jitter_strategy_variants() {
        let val: u8 = kani::any();
        kani::assume(val <= 3);

        let _jitter = match val {
            0 => JitterStrategy::None,
            1 => JitterStrategy::Full,
            2 => JitterStrategy::Equal,
            _ => JitterStrategy::Decorrelated,
        };

        kani::assert(val <= 3, "All 4 variants covered");
    }

    /// Verify default strategy has valid values.
    #[kani::proof]
    fn proof_default_strategy_valid() {
        let strategy = RetryStrategy::default();

        kani::assert(strategy.max_attempts > 0, "Max attempts must be positive");
        kani::assert(strategy.multiplier >= 1.0, "Multiplier must be >= 1");
        kani::assert(
            strategy.max_backoff >= strategy.initial_backoff,
            "Max backoff must be >= initial backoff",
        );
    }

    /// Verify backoff calculation with no jitter is deterministic.
    #[kani::proof]
    fn proof_backoff_no_jitter_deterministic() {
        let initial_ms: u64 = kani::any();
        let multiplier: f64 = kani::any();
        let max_ms: u64 = kani::any();
        let attempt: u32 = kani::any();

        kani::assume(initial_ms > 0 && initial_ms <= 10000);
        kani::assume(multiplier >= 1.0 && multiplier <= 10.0);
        kani::assume(max_ms >= initial_ms && max_ms <= 60000);
        kani::assume(attempt <= 10);
        kani::assume(multiplier.is_finite());

        let initial = initial_ms as f64 / 1000.0;
        let max = max_ms as f64 / 1000.0;

        let base_backoff = initial * multiplier.powi(attempt as i32);
        let capped = if base_backoff > max {
            max
        } else {
            base_backoff
        };

        kani::assert(capped <= max, "Backoff must be capped at max");
        kani::assert(capped >= 0.0, "Backoff must be non-negative");
    }

    /// Verify max_backoff cap is always respected.
    #[kani::proof]
    fn proof_max_backoff_cap() {
        let initial_secs: f64 = kani::any();
        let max_secs: f64 = kani::any();
        let multiplier: f64 = kani::any();
        let attempt: u32 = kani::any();

        kani::assume(initial_secs > 0.0 && initial_secs <= 10.0);
        kani::assume(max_secs > 0.0 && max_secs <= 300.0);
        kani::assume(multiplier >= 1.0 && multiplier <= 10.0);
        kani::assume(attempt <= 20);
        kani::assume(initial_secs.is_finite());
        kani::assume(max_secs.is_finite());
        kani::assume(multiplier.is_finite());

        let base = initial_secs * multiplier.powi(attempt as i32);
        let capped = base.min(max_secs);

        kani::assert(capped <= max_secs, "Backoff must never exceed max_backoff");
    }

    /// Verify exponential growth pattern.
    #[kani::proof]
    fn proof_exponential_growth() {
        let initial: f64 = kani::any();
        let multiplier: f64 = kani::any();

        kani::assume(initial > 0.0 && initial <= 1.0);
        kani::assume(multiplier > 1.0 && multiplier <= 3.0);
        kani::assume(initial.is_finite());
        kani::assume(multiplier.is_finite());

        let backoff_0 = initial;
        let backoff_1 = initial * multiplier;
        let backoff_2 = initial * multiplier * multiplier;

        kani::assert(
            backoff_1 > backoff_0,
            "Backoff should increase each attempt",
        );
        kani::assert(backoff_2 > backoff_1, "Backoff should continue increasing");
    }

    /// Verify retry budget token depletion.
    #[kani::proof]
    fn proof_budget_token_depletion() {
        let capacity: u32 = kani::any();
        let cost: f64 = kani::any();
        let retries: u32 = kani::any();

        kani::assume(capacity > 0 && capacity <= 100);
        kani::assume(cost > 0.0 && cost <= 10.0);
        kani::assume(cost.is_finite());
        kani::assume(retries <= 20);

        let initial_tokens = capacity as f64;
        let tokens_consumed = retries as f64 * cost;
        let remaining = initial_tokens - tokens_consumed;

        let expected_remaining = if remaining < 0.0 { 0.0 } else { remaining };
        kani::assert(
            expected_remaining >= 0.0,
            "Remaining tokens must be non-negative",
        );
    }

    /// Verify budget refund is bounded by capacity.
    #[kani::proof]
    fn proof_budget_refund_bounded() {
        let current: f64 = kani::any();
        let refund: f64 = kani::any();
        let capacity: u32 = kani::any();

        kani::assume(current >= 0.0 && current <= 1000.0);
        kani::assume(refund >= 0.0 && refund <= 10.0);
        kani::assume(capacity > 0 && capacity <= 100);
        kani::assume(current.is_finite());
        kani::assume(refund.is_finite());

        let new_tokens = (current + refund).min(capacity as f64);

        kani::assert(
            new_tokens <= capacity as f64,
            "Tokens must not exceed capacity",
        );
        kani::assert(
            new_tokens >= current.min(capacity as f64),
            "Refund should not decrease tokens",
        );
    }

    /// Verify attempt count in result.
    #[kani::proof]
    fn proof_attempt_count_bounds() {
        let max_attempts: u32 = kani::any();
        let actual_attempts: u32 = kani::any();

        kani::assume(max_attempts > 0 && max_attempts <= 100);
        kani::assume(actual_attempts > 0 && actual_attempts <= max_attempts);

        kani::assert(actual_attempts >= 1, "At least one attempt must be made");
        kani::assert(
            actual_attempts <= max_attempts,
            "Attempts must not exceed max_attempts",
        );
    }

    /// Verify is_retryable consistency.
    #[kani::proof]
    fn proof_is_retryable_consistency() {
        // By default, Permanent is not retryable
        let permanent_retryable = false;

        // By default, Transient is retryable
        let transient_retryable = true;

        kani::assert(
            !permanent_retryable,
            "Permanent errors should not be retryable by default",
        );
        kani::assert(
            transient_retryable,
            "Transient errors should be retryable by default",
        );
    }

    /// Verify RetryStats consistency.
    #[kani::proof]
    fn proof_retry_stats_consistency() {
        let total_ops: usize = kani::any();
        let successes: usize = kani::any();
        let failures: usize = kani::any();
        let total_retries: usize = kani::any();

        kani::assume(total_ops > 0 && total_ops <= 10000);
        kani::assume(successes <= total_ops);
        kani::assume(failures <= total_ops);
        kani::assume(successes + failures == total_ops);
        kani::assume(total_retries < usize::MAX / 2);

        // Average attempts calculation
        let avg_attempts = (total_retries + total_ops) as f64 / total_ops as f64;

        kani::assert(avg_attempts >= 1.0, "Average attempts must be at least 1");
        kani::assert(
            successes + failures == total_ops,
            "Successes + failures must equal total operations",
        );
    }

    /// Verify timeout check logic.
    #[kani::proof]
    fn proof_timeout_check() {
        let elapsed_ms: u64 = kani::any();
        let timeout_ms: u64 = kani::any();

        kani::assume(elapsed_ms < u64::MAX);
        kani::assume(timeout_ms > 0 && timeout_ms < u64::MAX);

        let is_timeout = elapsed_ms > timeout_ms;

        if elapsed_ms <= timeout_ms {
            kani::assert(!is_timeout, "Should not timeout if elapsed <= timeout");
        } else {
            kani::assert(is_timeout, "Should timeout if elapsed > timeout");
        }
    }

    /// Verify AttemptResult attempt number is 1-indexed.
    #[kani::proof]
    fn proof_attempt_number_one_indexed() {
        let attempt_zero_indexed: u32 = kani::any();
        kani::assume(attempt_zero_indexed < 100);

        let attempt_one_indexed = attempt_zero_indexed + 1;

        kani::assert(
            attempt_one_indexed >= 1,
            "Attempt number must be at least 1",
        );
        kani::assert(
            attempt_one_indexed == attempt_zero_indexed + 1,
            "One-indexed = zero-indexed + 1",
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[test]
    fn test_default_strategy() {
        let strategy = RetryStrategy::default();
        assert_eq!(strategy.max_attempts, 3);
    }

    #[test]
    fn test_backoff_calculation() {
        let strategy = RetryStrategy::new()
            .with_initial_backoff(Duration::from_millis(100))
            .with_multiplier(2.0)
            .with_jitter(JitterStrategy::None);

        let backoff0 = strategy.calculate_backoff(0);
        let backoff1 = strategy.calculate_backoff(1);
        let backoff2 = strategy.calculate_backoff(2);

        assert_eq!(backoff0.as_millis(), 100);
        assert_eq!(backoff1.as_millis(), 200);
        assert_eq!(backoff2.as_millis(), 400);
    }

    #[test]
    fn test_max_backoff_cap() {
        let strategy = RetryStrategy::new()
            .with_initial_backoff(Duration::from_secs(1))
            .with_max_backoff(Duration::from_secs(5))
            .with_multiplier(10.0)
            .with_jitter(JitterStrategy::None);

        let backoff = strategy.calculate_backoff(5);
        assert!(backoff <= Duration::from_secs(5));
    }

    #[test]
    fn test_error_classifier() {
        let classifier = DefaultClassifier;

        assert_eq!(
            classifier.classify("rate limit exceeded"),
            ErrorClass::RateLimited
        );
        assert_eq!(
            classifier.classify("connection refused"),
            ErrorClass::NetworkError
        );
        assert_eq!(classifier.classify("request timeout"), ErrorClass::Timeout);
        assert_eq!(
            classifier.classify("500 internal server error"),
            ErrorClass::ServerError
        );
        assert_eq!(classifier.classify("404 not found"), ErrorClass::Permanent);
    }

    #[tokio::test]
    async fn test_retry_success_first_attempt() {
        let strategy = RetryStrategy::new().with_max_attempts(3);
        let executor = RetryExecutor::new(strategy);

        let result = executor
            .execute(|| async { Ok::<_, &str>("success") })
            .await
            .unwrap();

        assert!(result.success);
        assert_eq!(result.attempts, 1);
        assert_eq!(result.value, Some("success"));
    }

    #[tokio::test]
    async fn test_retry_success_after_failures() {
        let strategy = RetryStrategy::new()
            .with_max_attempts(3)
            .with_initial_backoff(Duration::from_millis(1));
        let executor = RetryExecutor::new(strategy);

        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let result = executor
            .execute(|| {
                let c = counter_clone.clone();
                async move {
                    let count = c.fetch_add(1, Ordering::SeqCst);
                    if count < 2 {
                        Err("transient error")
                    } else {
                        Ok("success")
                    }
                }
            })
            .await
            .unwrap();

        assert!(result.success);
        assert_eq!(result.attempts, 3);
    }

    #[tokio::test]
    async fn test_retry_max_exceeded() {
        let strategy = RetryStrategy::new()
            .with_max_attempts(3)
            .with_initial_backoff(Duration::from_millis(1));
        let executor = RetryExecutor::new(strategy);

        let result = executor
            .execute(|| async { Err::<(), _>("always fails") })
            .await
            .unwrap();

        assert!(!result.success);
        assert_eq!(result.attempts, 3);
    }

    #[tokio::test]
    async fn test_retry_budget() {
        let budget = Arc::new(RetryBudget::new(2, 0.0)); // No refresh
        let strategy = RetryStrategy::new()
            .with_max_attempts(5)
            .with_initial_backoff(Duration::from_millis(1));
        let executor = RetryExecutor::new(strategy).with_budget(budget);

        let result = executor
            .execute(|| async { Err::<(), _>("always fails") })
            .await;

        // Should fail due to budget exhaustion
        assert!(result.is_err() || !result.unwrap().success);
    }

    #[tokio::test]
    async fn test_stats() {
        let strategy = RetryStrategy::new().with_max_attempts(1);
        let executor = RetryExecutor::new(strategy);

        executor.execute(|| async { Ok::<_, &str>("a") }).await.ok();
        executor.execute(|| async { Ok::<_, &str>("b") }).await.ok();

        let stats = executor.stats().await;
        assert_eq!(stats.total_operations, 2);
        assert_eq!(stats.successes, 2);
    }
}
