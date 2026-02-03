//! Smart retry and recovery for drbot.
//!
//! Resilient operations with automatic recovery.
//!
//! # Features
//!
//! - Automatic retries
//! - Exponential backoff
//! - Circuit breaker
//! - Fallback strategies
//! - Health monitoring

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Resilience result type.
pub type Result<T> = std::result::Result<T, ResilienceError>;

/// Resilience errors.
#[derive(Debug, thiserror::Error)]
pub enum ResilienceError {
    #[error("Max retries exceeded: {0}")]
    MaxRetriesExceeded(String),
    #[error("Circuit open: {0}")]
    CircuitOpen(String),
    #[error("Timeout: {0}")]
    Timeout(String),
    #[error("All fallbacks failed: {0}")]
    AllFallbacksFailed(String),
    #[error("Operation failed: {0}")]
    OperationFailed(String),
}

/// Retry configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    /// Maximum retries.
    pub max_retries: usize,
    /// Initial delay.
    pub initial_delay_ms: u64,
    /// Maximum delay.
    pub max_delay_ms: u64,
    /// Backoff multiplier.
    pub multiplier: f32,
    /// Jitter factor (0-1).
    pub jitter: f32,
    /// Retry on these error types.
    pub retry_on: Vec<String>,
    /// Don't retry on these error types.
    pub dont_retry_on: Vec<String>,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_delay_ms: 100,
            max_delay_ms: 30000,
            multiplier: 2.0,
            jitter: 0.1,
            retry_on: Vec::new(),
            dont_retry_on: Vec::new(),
        }
    }
}

/// Circuit breaker configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitConfig {
    /// Failure threshold before opening.
    pub failure_threshold: usize,
    /// Success threshold to close.
    pub success_threshold: usize,
    /// Time to wait before half-open.
    pub timeout_ms: u64,
    /// Window size for failure counting.
    pub window_size: usize,
}

impl Default for CircuitConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            success_threshold: 3,
            timeout_ms: 30000,
            window_size: 10,
        }
    }
}

/// Circuit state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CircuitState {
    Closed,
    Open,
    HalfOpen,
}

/// Circuit breaker.
pub struct CircuitBreaker {
    name: String,
    config: CircuitConfig,
    state: Arc<RwLock<CircuitState>>,
    failures: AtomicUsize,
    successes: AtomicUsize,
    last_failure: Arc<RwLock<Option<std::time::Instant>>>,
}

impl CircuitBreaker {
    /// Create a new circuit breaker.
    pub fn new(name: &str, config: CircuitConfig) -> Self {
        Self {
            name: name.to_string(),
            config,
            state: Arc::new(RwLock::new(CircuitState::Closed)),
            failures: AtomicUsize::new(0),
            successes: AtomicUsize::new(0),
            last_failure: Arc::new(RwLock::new(None)),
        }
    }

    /// Check if circuit allows requests.
    pub async fn can_execute(&self) -> bool {
        let state = *self.state.read().await;

        match state {
            CircuitState::Closed => true,
            CircuitState::Open => {
                // Check if timeout has passed
                if let Some(last) = *self.last_failure.read().await {
                    if last.elapsed().as_millis() as u64 >= self.config.timeout_ms {
                        // Transition to half-open
                        *self.state.write().await = CircuitState::HalfOpen;
                        true
                    } else {
                        false
                    }
                } else {
                    true
                }
            }
            CircuitState::HalfOpen => true,
        }
    }

    /// Record a success.
    pub async fn record_success(&self) {
        let state = *self.state.read().await;

        match state {
            CircuitState::HalfOpen => {
                let successes = self.successes.fetch_add(1, Ordering::SeqCst) + 1;
                if successes >= self.config.success_threshold {
                    *self.state.write().await = CircuitState::Closed;
                    self.failures.store(0, Ordering::SeqCst);
                    self.successes.store(0, Ordering::SeqCst);
                }
            }
            CircuitState::Closed => {
                self.failures.store(0, Ordering::SeqCst);
            }
            _ => {}
        }
    }

    /// Record a failure.
    pub async fn record_failure(&self) {
        let state = *self.state.read().await;
        *self.last_failure.write().await = Some(std::time::Instant::now());

        match state {
            CircuitState::HalfOpen => {
                *self.state.write().await = CircuitState::Open;
                self.successes.store(0, Ordering::SeqCst);
            }
            CircuitState::Closed => {
                let failures = self.failures.fetch_add(1, Ordering::SeqCst) + 1;
                if failures >= self.config.failure_threshold {
                    *self.state.write().await = CircuitState::Open;
                }
            }
            _ => {}
        }
    }

    /// Get current state.
    pub async fn state(&self) -> CircuitState {
        *self.state.read().await
    }

    /// Get name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get failure count.
    pub fn failure_count(&self) -> usize {
        self.failures.load(Ordering::SeqCst)
    }

    /// Reset the circuit.
    pub async fn reset(&self) {
        *self.state.write().await = CircuitState::Closed;
        self.failures.store(0, Ordering::SeqCst);
        self.successes.store(0, Ordering::SeqCst);
        *self.last_failure.write().await = None;
    }
}

/// Retry executor.
pub struct RetryExecutor {
    config: RetryConfig,
}

impl RetryExecutor {
    /// Create a new retry executor.
    pub fn new(config: RetryConfig) -> Self {
        Self { config }
    }

    /// Execute with retries.
    pub async fn execute<F, Fut, T, E>(&self, operation: F) -> std::result::Result<T, E>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = std::result::Result<T, E>>,
        E: std::fmt::Display,
    {
        let mut attempt = 0;
        let mut delay = self.config.initial_delay_ms;

        loop {
            match operation().await {
                Ok(result) => return Ok(result),
                Err(e) if attempt < self.config.max_retries => {
                    attempt += 1;

                    // Calculate delay with jitter
                    let jitter = if self.config.jitter > 0.0 {
                        let range = (delay as f32 * self.config.jitter) as u64;
                        if range > 0 {
                            rand_delay(range)
                        } else {
                            0
                        }
                    } else {
                        0
                    };

                    let sleep_time = delay + jitter;
                    tokio::time::sleep(Duration::from_millis(sleep_time)).await;

                    // Increase delay for next attempt
                    delay = ((delay as f32 * self.config.multiplier) as u64)
                        .min(self.config.max_delay_ms);

                    tracing::warn!(
                        attempt = attempt,
                        max_retries = self.config.max_retries,
                        "Retry after error: {}",
                        e
                    );
                }
                Err(e) => return Err(e),
            }
        }
    }
}

/// Simple random delay.
fn rand_delay(max: u64) -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .subsec_nanos() as u64;
    nanos % max
}

/// Fallback strategy.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FallbackStrategy {
    /// Return default value.
    Default { value: serde_json::Value },
    /// Use cached value.
    Cached,
    /// Use alternate provider.
    AlternateProvider { provider_id: String },
    /// Return error.
    Error { message: String },
    /// Custom fallback.
    Custom { handler: String },
}

/// Resilient operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResilientOperation {
    /// Operation ID.
    pub id: Uuid,
    /// Operation name.
    pub name: String,
    /// Retry config.
    pub retry_config: RetryConfig,
    /// Circuit breaker config.
    pub circuit_config: CircuitConfig,
    /// Fallback strategies.
    pub fallbacks: Vec<FallbackStrategy>,
    /// Timeout.
    pub timeout_ms: Option<u64>,
}

impl ResilientOperation {
    /// Create a new operation.
    pub fn new(name: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.to_string(),
            retry_config: RetryConfig::default(),
            circuit_config: CircuitConfig::default(),
            fallbacks: Vec::new(),
            timeout_ms: None,
        }
    }

    /// Set retry config.
    pub fn with_retry(mut self, config: RetryConfig) -> Self {
        self.retry_config = config;
        self
    }

    /// Set circuit config.
    pub fn with_circuit(mut self, config: CircuitConfig) -> Self {
        self.circuit_config = config;
        self
    }

    /// Add fallback.
    pub fn with_fallback(mut self, fallback: FallbackStrategy) -> Self {
        self.fallbacks.push(fallback);
        self
    }

    /// Set timeout.
    pub fn with_timeout(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = Some(timeout_ms);
        self
    }
}

/// Health status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatus {
    /// Service name.
    pub name: String,
    /// Is healthy.
    pub healthy: bool,
    /// Circuit state.
    pub circuit_state: CircuitState,
    /// Recent failures.
    pub recent_failures: usize,
    /// Success rate.
    pub success_rate: f32,
    /// Average latency.
    pub avg_latency_ms: f64,
    /// Last check.
    pub last_check: DateTime<Utc>,
}

/// Health metrics.
#[derive(Debug, Default)]
pub struct HealthMetrics {
    successes: AtomicU64,
    failures: AtomicU64,
    total_latency_ms: AtomicU64,
}

impl HealthMetrics {
    /// Record a success with latency.
    pub fn record_success(&self, latency_ms: u64) {
        self.successes.fetch_add(1, Ordering::SeqCst);
        self.total_latency_ms
            .fetch_add(latency_ms, Ordering::SeqCst);
    }

    /// Record a failure.
    pub fn record_failure(&self) {
        self.failures.fetch_add(1, Ordering::SeqCst);
    }

    /// Get success rate.
    pub fn success_rate(&self) -> f32 {
        let successes = self.successes.load(Ordering::SeqCst) as f32;
        let failures = self.failures.load(Ordering::SeqCst) as f32;
        let total = successes + failures;
        if total > 0.0 {
            successes / total
        } else {
            1.0
        }
    }

    /// Get average latency.
    pub fn avg_latency_ms(&self) -> f64 {
        let successes = self.successes.load(Ordering::SeqCst);
        let total_latency = self.total_latency_ms.load(Ordering::SeqCst);
        if successes > 0 {
            total_latency as f64 / successes as f64
        } else {
            0.0
        }
    }

    /// Reset metrics.
    pub fn reset(&self) {
        self.successes.store(0, Ordering::SeqCst);
        self.failures.store(0, Ordering::SeqCst);
        self.total_latency_ms.store(0, Ordering::SeqCst);
    }
}

/// Resilience manager.
pub struct ResilienceManager {
    circuits: Arc<RwLock<HashMap<String, Arc<CircuitBreaker>>>>,
    metrics: Arc<RwLock<HashMap<String, Arc<HealthMetrics>>>>,
}

impl ResilienceManager {
    /// Create a new resilience manager.
    pub fn new() -> Self {
        Self {
            circuits: Arc::new(RwLock::new(HashMap::new())),
            metrics: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a circuit breaker.
    pub async fn register_circuit(&self, name: &str, config: CircuitConfig) {
        let circuit = Arc::new(CircuitBreaker::new(name, config));
        self.circuits
            .write()
            .await
            .insert(name.to_string(), circuit);
        self.metrics
            .write()
            .await
            .insert(name.to_string(), Arc::new(HealthMetrics::default()));
    }

    /// Get a circuit breaker.
    pub async fn get_circuit(&self, name: &str) -> Option<Arc<CircuitBreaker>> {
        self.circuits.read().await.get(name).cloned()
    }

    /// Execute with circuit breaker.
    pub async fn execute_with_circuit<F, Fut, T>(&self, name: &str, operation: F) -> Result<T>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T>>,
    {
        let circuit = self.get_circuit(name).await.ok_or_else(|| {
            ResilienceError::OperationFailed(format!("Circuit not found: {}", name))
        })?;

        if !circuit.can_execute().await {
            return Err(ResilienceError::CircuitOpen(name.to_string()));
        }

        let start = std::time::Instant::now();
        let result = operation().await;
        let latency = start.elapsed().as_millis() as u64;

        let metrics = self.metrics.read().await.get(name).cloned();

        match &result {
            Ok(_) => {
                circuit.record_success().await;
                if let Some(m) = metrics {
                    m.record_success(latency);
                }
            }
            Err(_) => {
                circuit.record_failure().await;
                if let Some(m) = metrics {
                    m.record_failure();
                }
            }
        }

        result
    }

    /// Get health status for a service.
    pub async fn health_status(&self, name: &str) -> Option<HealthStatus> {
        let circuit = self.circuits.read().await.get(name).cloned()?;
        let metrics = self.metrics.read().await.get(name).cloned()?;

        let circuit_state = circuit.state().await;
        let success_rate = metrics.success_rate();

        Some(HealthStatus {
            name: name.to_string(),
            healthy: circuit_state == CircuitState::Closed && success_rate > 0.9,
            circuit_state,
            recent_failures: circuit.failure_count(),
            success_rate,
            avg_latency_ms: metrics.avg_latency_ms(),
            last_check: Utc::now(),
        })
    }

    /// Get all health statuses.
    pub async fn all_health(&self) -> Vec<HealthStatus> {
        let mut statuses = Vec::new();
        let circuits = self.circuits.read().await;

        for name in circuits.keys() {
            if let Some(status) = self.health_status(name).await {
                statuses.push(status);
            }
        }

        statuses
    }

    /// Reset a circuit.
    pub async fn reset_circuit(&self, name: &str) {
        if let Some(circuit) = self.circuits.read().await.get(name) {
            circuit.reset().await;
        }
        if let Some(metrics) = self.metrics.read().await.get(name) {
            metrics.reset();
        }
    }
}

impl Default for ResilienceManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_circuit_breaker() {
        let circuit = CircuitBreaker::new(
            "test",
            CircuitConfig {
                failure_threshold: 2,
                ..Default::default()
            },
        );

        assert!(circuit.can_execute().await);
        assert_eq!(circuit.state().await, CircuitState::Closed);

        // Record failures
        circuit.record_failure().await;
        circuit.record_failure().await;

        assert_eq!(circuit.state().await, CircuitState::Open);
    }

    #[tokio::test]
    async fn test_retry_executor() {
        let config = RetryConfig {
            max_retries: 2,
            initial_delay_ms: 10,
            ..Default::default()
        };

        let executor = RetryExecutor::new(config);
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();

        let result: std::result::Result<i32, String> = executor
            .execute(|| {
                let count = counter_clone.fetch_add(1, Ordering::SeqCst);
                async move {
                    if count < 2 {
                        Err("fail".to_string())
                    } else {
                        Ok(42)
                    }
                }
            })
            .await;

        assert_eq!(result, Ok(42));
        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_resilience_manager() {
        let manager = ResilienceManager::new();
        manager
            .register_circuit("test-service", CircuitConfig::default())
            .await;

        let result = manager
            .execute_with_circuit("test-service", || async { Ok::<_, ResilienceError>(42) })
            .await;

        assert_eq!(result.unwrap(), 42);

        let status = manager.health_status("test-service").await.unwrap();
        assert!(status.healthy);
        assert_eq!(status.success_rate, 1.0);
    }

    #[test]
    fn test_health_metrics() {
        let metrics = HealthMetrics::default();

        metrics.record_success(100);
        metrics.record_success(200);
        metrics.record_failure();

        assert!((metrics.success_rate() - 0.666).abs() < 0.01);
        assert_eq!(metrics.avg_latency_ms(), 150.0);
    }
}
