//! Circuit breaker pattern for drbot.
//!
//! This crate provides:
//! - Circuit breaker state machine
//! - Failure tracking
//! - Half-open testing

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::RwLock;
use std::time::{Duration, Instant};
use thiserror::Error;

/// Circuit breaker error types.
#[derive(Error, Debug, Clone)]
pub enum CircuitBreakerError {
    #[error("Circuit is open")]
    Open,

    #[error("Operation failed: {0}")]
    OperationFailed(String),
}

/// Result type for circuit breaker operations.
pub type Result<T> = std::result::Result<T, CircuitBreakerError>;

/// Circuit breaker state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    /// Normal operation.
    Closed,
    /// Failing, rejecting requests.
    Open,
    /// Testing if service recovered.
    HalfOpen,
}

/// Circuit breaker configuration.
#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    /// Failure threshold to open circuit.
    pub failure_threshold: u32,
    /// Success threshold to close circuit (from half-open).
    pub success_threshold: u32,
    /// Time to wait before transitioning to half-open.
    pub reset_timeout: Duration,
    /// Window for counting failures.
    pub failure_window: Option<Duration>,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            success_threshold: 3,
            reset_timeout: Duration::from_secs(30),
            failure_window: Some(Duration::from_secs(60)),
        }
    }
}

/// Circuit breaker.
pub struct CircuitBreaker {
    state: RwLock<CircuitState>,
    failure_count: AtomicU32,
    success_count: AtomicU32,
    last_failure_time: RwLock<Option<Instant>>,
    opened_at: RwLock<Option<Instant>>,
    config: CircuitBreakerConfig,
}

impl CircuitBreaker {
    /// Create new circuit breaker.
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            state: RwLock::new(CircuitState::Closed),
            failure_count: AtomicU32::new(0),
            success_count: AtomicU32::new(0),
            last_failure_time: RwLock::new(None),
            opened_at: RwLock::new(None),
            config,
        }
    }

    /// Create with default config.
    pub fn default_config() -> Self {
        Self::new(CircuitBreakerConfig::default())
    }

    /// Get current state.
    pub fn state(&self) -> CircuitState {
        *self.state.read().unwrap()
    }

    /// Check if circuit allows request.
    pub fn allow_request(&self) -> bool {
        let state = self.state.read().unwrap();

        match *state {
            CircuitState::Closed => true,
            CircuitState::Open => {
                // Check if timeout has passed
                if let Some(opened_at) = *self.opened_at.read().unwrap() {
                    if opened_at.elapsed() >= self.config.reset_timeout {
                        drop(state);
                        self.transition_to_half_open();
                        return true;
                    }
                }
                false
            }
            CircuitState::HalfOpen => true,
        }
    }

    /// Record successful operation.
    pub fn record_success(&self) {
        let state = *self.state.read().unwrap();

        match state {
            CircuitState::Closed => {
                self.failure_count.store(0, Ordering::Release);
            }
            CircuitState::HalfOpen => {
                let successes = self.success_count.fetch_add(1, Ordering::AcqRel) + 1;
                if successes >= self.config.success_threshold {
                    self.transition_to_closed();
                }
            }
            CircuitState::Open => {}
        }
    }

    /// Record failed operation.
    pub fn record_failure(&self) {
        let state = *self.state.read().unwrap();

        match state {
            CircuitState::Closed => {
                let now = Instant::now();

                // Check if we should reset the failure count (outside window)
                if let Some(window) = self.config.failure_window {
                    if let Some(last_failure) = *self.last_failure_time.read().unwrap() {
                        if last_failure.elapsed() > window {
                            self.failure_count.store(0, Ordering::Release);
                        }
                    }
                }

                *self.last_failure_time.write().unwrap() = Some(now);
                let failures = self.failure_count.fetch_add(1, Ordering::AcqRel) + 1;

                if failures >= self.config.failure_threshold {
                    self.transition_to_open();
                }
            }
            CircuitState::HalfOpen => {
                self.transition_to_open();
            }
            CircuitState::Open => {}
        }
    }

    /// Transition to open state.
    fn transition_to_open(&self) {
        *self.state.write().unwrap() = CircuitState::Open;
        *self.opened_at.write().unwrap() = Some(Instant::now());
        self.failure_count.store(0, Ordering::Release);
        self.success_count.store(0, Ordering::Release);
    }

    /// Transition to half-open state.
    fn transition_to_half_open(&self) {
        *self.state.write().unwrap() = CircuitState::HalfOpen;
        self.success_count.store(0, Ordering::Release);
    }

    /// Transition to closed state.
    fn transition_to_closed(&self) {
        *self.state.write().unwrap() = CircuitState::Closed;
        *self.opened_at.write().unwrap() = None;
        self.failure_count.store(0, Ordering::Release);
        self.success_count.store(0, Ordering::Release);
    }

    /// Execute operation with circuit breaker.
    pub fn call<T, E, F>(&self, f: F) -> std::result::Result<T, E>
    where
        E: From<CircuitBreakerError>,
        F: FnOnce() -> std::result::Result<T, E>,
    {
        if !self.allow_request() {
            return Err(CircuitBreakerError::Open.into());
        }

        match f() {
            Ok(result) => {
                self.record_success();
                Ok(result)
            }
            Err(e) => {
                self.record_failure();
                Err(e)
            }
        }
    }

    /// Get statistics.
    pub fn stats(&self) -> CircuitBreakerStats {
        CircuitBreakerStats {
            state: self.state(),
            failure_count: self.failure_count.load(Ordering::Acquire),
            success_count: self.success_count.load(Ordering::Acquire),
        }
    }

    /// Manually reset the circuit.
    pub fn reset(&self) {
        self.transition_to_closed();
    }

    /// Manually open the circuit.
    pub fn trip(&self) {
        self.transition_to_open();
    }
}

/// Circuit breaker statistics.
#[derive(Debug, Clone)]
pub struct CircuitBreakerStats {
    pub state: CircuitState,
    pub failure_count: u32,
    pub success_count: u32,
}

/// Builder for circuit breaker.
pub struct CircuitBreakerBuilder {
    config: CircuitBreakerConfig,
}

impl CircuitBreakerBuilder {
    /// Create new builder.
    pub fn new() -> Self {
        Self {
            config: CircuitBreakerConfig::default(),
        }
    }

    /// Set failure threshold.
    pub fn failure_threshold(mut self, threshold: u32) -> Self {
        self.config.failure_threshold = threshold;
        self
    }

    /// Set success threshold.
    pub fn success_threshold(mut self, threshold: u32) -> Self {
        self.config.success_threshold = threshold;
        self
    }

    /// Set reset timeout.
    pub fn reset_timeout(mut self, timeout: Duration) -> Self {
        self.config.reset_timeout = timeout;
        self
    }

    /// Set failure window.
    pub fn failure_window(mut self, window: Duration) -> Self {
        self.config.failure_window = Some(window);
        self
    }

    /// Build circuit breaker.
    pub fn build(self) -> CircuitBreaker {
        CircuitBreaker::new(self.config)
    }
}

impl Default for CircuitBreakerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_circuit_closed() {
        let cb = CircuitBreaker::default_config();
        assert_eq!(cb.state(), CircuitState::Closed);
        assert!(cb.allow_request());
    }

    #[test]
    fn test_circuit_opens_on_failures() {
        let cb = CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 3,
            ..Default::default()
        });

        for _ in 0..3 {
            cb.record_failure();
        }

        assert_eq!(cb.state(), CircuitState::Open);
        assert!(!cb.allow_request());
    }

    #[test]
    fn test_success_resets_failures() {
        let cb = CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 3,
            ..Default::default()
        });

        cb.record_failure();
        cb.record_failure();
        cb.record_success();

        assert_eq!(cb.state(), CircuitState::Closed);
    }

    #[test]
    fn test_builder() {
        let cb = CircuitBreakerBuilder::new()
            .failure_threshold(10)
            .success_threshold(5)
            .reset_timeout(Duration::from_secs(60))
            .build();

        assert_eq!(cb.config.failure_threshold, 10);
        assert_eq!(cb.config.success_threshold, 5);
    }
}
