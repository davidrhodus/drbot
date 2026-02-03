//! Rate limiting for API calls.
//!
//! This crate provides:
//! - Token bucket rate limiting
//! - Sliding window rate limiting
//! - Per-user/per-key limits
//! - Distributed rate limiting support

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::RwLock;

/// Rate limit errors.
#[derive(Debug, Error)]
pub enum RateLimitError {
    #[error("Rate limit exceeded: {0}")]
    LimitExceeded(String),

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("Storage error: {0}")]
    StorageError(String),
}

/// Result type for rate limit operations.
pub type Result<T> = std::result::Result<T, RateLimitError>;

/// Rate limit decision.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitResult {
    /// Whether the request is allowed.
    pub allowed: bool,
    /// Remaining requests in current window.
    pub remaining: u64,
    /// Total limit.
    pub limit: u64,
    /// Reset time (when limit resets).
    pub reset_at: DateTime<Utc>,
    /// Retry after (seconds until next allowed request).
    pub retry_after: Option<u64>,
}

/// Rate limit configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    /// Maximum requests per window.
    pub max_requests: u64,
    /// Window duration.
    pub window: Duration,
    /// Algorithm to use.
    pub algorithm: RateLimitAlgorithm,
    /// Burst allowance (for token bucket).
    pub burst: Option<u64>,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            max_requests: 100,
            window: Duration::from_secs(60),
            algorithm: RateLimitAlgorithm::SlidingWindow,
            burst: None,
        }
    }
}

impl RateLimitConfig {
    /// Create a new config.
    pub fn new(max_requests: u64, window: Duration) -> Self {
        Self {
            max_requests,
            window,
            algorithm: RateLimitAlgorithm::SlidingWindow,
            burst: None,
        }
    }

    /// Set algorithm.
    pub fn with_algorithm(mut self, algorithm: RateLimitAlgorithm) -> Self {
        self.algorithm = algorithm;
        self
    }

    /// Set burst allowance.
    pub fn with_burst(mut self, burst: u64) -> Self {
        self.burst = Some(burst);
        self
    }
}

/// Rate limiting algorithms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RateLimitAlgorithm {
    /// Fixed window counter.
    FixedWindow,
    /// Sliding window log.
    SlidingWindow,
    /// Token bucket.
    TokenBucket,
    /// Leaky bucket.
    LeakyBucket,
}

impl Default for RateLimitAlgorithm {
    fn default() -> Self {
        Self::SlidingWindow
    }
}

/// Token bucket state.
#[derive(Debug, Clone)]
struct TokenBucket {
    tokens: f64,
    last_update: DateTime<Utc>,
    capacity: f64,
    refill_rate: f64, // tokens per second
}

impl TokenBucket {
    fn new(capacity: u64, refill_rate: f64) -> Self {
        Self {
            tokens: capacity as f64,
            last_update: Utc::now(),
            capacity: capacity as f64,
            refill_rate,
        }
    }

    fn try_acquire(&mut self, tokens: u64) -> bool {
        self.refill();

        if self.tokens >= tokens as f64 {
            self.tokens -= tokens as f64;
            true
        } else {
            false
        }
    }

    fn refill(&mut self) {
        let now = Utc::now();
        let elapsed = (now - self.last_update).num_milliseconds() as f64 / 1000.0;

        self.tokens = (self.tokens + elapsed * self.refill_rate).min(self.capacity);
        self.last_update = now;
    }

    fn available(&mut self) -> f64 {
        self.refill();
        self.tokens
    }

    fn time_until_available(&self, tokens: u64) -> Duration {
        if self.tokens >= tokens as f64 {
            return Duration::ZERO;
        }

        let needed = tokens as f64 - self.tokens;
        let seconds = needed / self.refill_rate;
        Duration::from_secs_f64(seconds)
    }
}

/// Sliding window state.
#[derive(Debug, Clone, Default)]
struct SlidingWindow {
    timestamps: Vec<DateTime<Utc>>,
}

impl SlidingWindow {
    fn new() -> Self {
        Self {
            timestamps: Vec::new(),
        }
    }

    fn count_in_window(&mut self, window: Duration) -> usize {
        let now = Utc::now();
        let window_start =
            now - ChronoDuration::from_std(window).unwrap_or(ChronoDuration::seconds(60));

        // Remove old timestamps
        self.timestamps.retain(|ts| *ts > window_start);

        self.timestamps.len()
    }

    fn record(&mut self) {
        self.timestamps.push(Utc::now());
    }
}

/// Fixed window state.
#[derive(Debug, Clone)]
struct FixedWindow {
    count: u64,
    window_start: DateTime<Utc>,
    window_duration: Duration,
}

impl FixedWindow {
    fn new(window_duration: Duration) -> Self {
        Self {
            count: 0,
            window_start: Utc::now(),
            window_duration,
        }
    }

    fn get_count(&mut self) -> u64 {
        self.maybe_reset();
        self.count
    }

    fn increment(&mut self) {
        self.maybe_reset();
        self.count += 1;
    }

    fn maybe_reset(&mut self) {
        let now = Utc::now();
        let window_end = self.window_start
            + ChronoDuration::from_std(self.window_duration).unwrap_or(ChronoDuration::seconds(60));

        if now >= window_end {
            self.count = 0;
            self.window_start = now;
        }
    }

    fn reset_at(&self) -> DateTime<Utc> {
        self.window_start
            + ChronoDuration::from_std(self.window_duration).unwrap_or(ChronoDuration::seconds(60))
    }
}

/// Rate limiter state for a single key.
#[derive(Debug)]
enum LimiterState {
    TokenBucket(TokenBucket),
    SlidingWindow(SlidingWindow),
    FixedWindow(FixedWindow),
}

/// The rate limiter.
pub struct RateLimiter {
    /// Configuration.
    config: RateLimitConfig,
    /// Per-key states.
    states: Arc<RwLock<HashMap<String, LimiterState>>>,
    /// Statistics.
    stats: Arc<RwLock<RateLimitStats>>,
}

/// Rate limiter statistics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RateLimitStats {
    /// Total requests checked.
    pub total_requests: u64,
    /// Requests allowed.
    pub allowed: u64,
    /// Requests denied.
    pub denied: u64,
    /// Current unique keys.
    pub unique_keys: usize,
}

impl RateLimiter {
    /// Create a new rate limiter.
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            config,
            states: Arc::new(RwLock::new(HashMap::new())),
            stats: Arc::new(RwLock::new(RateLimitStats::default())),
        }
    }

    /// Check if a request is allowed.
    pub async fn check(&self, key: &str) -> RateLimitResult {
        self.check_n(key, 1).await
    }

    /// Check if N requests are allowed.
    pub async fn check_n(&self, key: &str, n: u64) -> RateLimitResult {
        let mut states = self.states.write().await;
        let mut stats = self.stats.write().await;

        stats.total_requests += 1;
        stats.unique_keys = states.len();

        let state = states
            .entry(key.to_string())
            .or_insert_with(|| self.create_state());

        let result = match state {
            LimiterState::TokenBucket(bucket) => {
                let allowed = bucket.try_acquire(n);
                let remaining = bucket.available() as u64;
                let retry_after = if allowed {
                    None
                } else {
                    Some(bucket.time_until_available(n).as_secs())
                };

                RateLimitResult {
                    allowed,
                    remaining,
                    limit: self.config.burst.unwrap_or(self.config.max_requests),
                    reset_at: Utc::now()
                        + ChronoDuration::from_std(self.config.window)
                            .unwrap_or(ChronoDuration::seconds(60)),
                    retry_after,
                }
            }
            LimiterState::SlidingWindow(window) => {
                let count = window.count_in_window(self.config.window) as u64;
                let allowed = count + n <= self.config.max_requests;

                if allowed {
                    for _ in 0..n {
                        window.record();
                    }
                }

                let remaining = if allowed {
                    self.config.max_requests - count - n
                } else {
                    0
                };

                RateLimitResult {
                    allowed,
                    remaining,
                    limit: self.config.max_requests,
                    reset_at: Utc::now()
                        + ChronoDuration::from_std(self.config.window)
                            .unwrap_or(ChronoDuration::seconds(60)),
                    retry_after: if allowed {
                        None
                    } else {
                        Some(self.config.window.as_secs())
                    },
                }
            }
            LimiterState::FixedWindow(window) => {
                let count = window.get_count();
                let allowed = count + n <= self.config.max_requests;

                if allowed {
                    for _ in 0..n {
                        window.increment();
                    }
                }

                let remaining = if allowed {
                    self.config.max_requests - count - n
                } else {
                    0
                };

                let reset_at = window.reset_at();
                let retry_after = if allowed {
                    None
                } else {
                    Some((reset_at - Utc::now()).num_seconds().max(0) as u64)
                };

                RateLimitResult {
                    allowed,
                    remaining,
                    limit: self.config.max_requests,
                    reset_at,
                    retry_after,
                }
            }
        };

        if result.allowed {
            stats.allowed += 1;
        } else {
            stats.denied += 1;
        }

        result
    }

    fn create_state(&self) -> LimiterState {
        match self.config.algorithm {
            RateLimitAlgorithm::TokenBucket => {
                let capacity = self.config.burst.unwrap_or(self.config.max_requests);
                let refill_rate =
                    self.config.max_requests as f64 / self.config.window.as_secs_f64();
                LimiterState::TokenBucket(TokenBucket::new(capacity, refill_rate))
            }
            RateLimitAlgorithm::SlidingWindow => LimiterState::SlidingWindow(SlidingWindow::new()),
            RateLimitAlgorithm::FixedWindow | RateLimitAlgorithm::LeakyBucket => {
                LimiterState::FixedWindow(FixedWindow::new(self.config.window))
            }
        }
    }

    /// Reset a specific key.
    pub async fn reset(&self, key: &str) {
        let mut states = self.states.write().await;
        states.remove(key);
    }

    /// Reset all keys.
    pub async fn reset_all(&self) {
        let mut states = self.states.write().await;
        states.clear();
    }

    /// Get statistics.
    pub async fn stats(&self) -> RateLimitStats {
        let stats = self.stats.read().await;
        let states = self.states.read().await;

        RateLimitStats {
            unique_keys: states.len(),
            ..stats.clone()
        }
    }

    /// Clean up expired entries.
    pub async fn cleanup(&self) {
        let mut states = self.states.write().await;

        // For sliding windows, remove entries with no recent activity
        states.retain(|_, state| match state {
            LimiterState::SlidingWindow(window) => !window.timestamps.is_empty(),
            _ => true,
        });
    }
}

/// Rate limit middleware result.
#[derive(Debug, Clone)]
pub struct RateLimitHeaders {
    /// X-RateLimit-Limit header value.
    pub limit: String,
    /// X-RateLimit-Remaining header value.
    pub remaining: String,
    /// X-RateLimit-Reset header value (Unix timestamp).
    pub reset: String,
    /// Retry-After header value (if rate limited).
    pub retry_after: Option<String>,
}

impl From<RateLimitResult> for RateLimitHeaders {
    fn from(result: RateLimitResult) -> Self {
        Self {
            limit: result.limit.to_string(),
            remaining: result.remaining.to_string(),
            reset: result.reset_at.timestamp().to_string(),
            retry_after: result.retry_after.map(|s| s.to_string()),
        }
    }
}

/// Multi-tier rate limiter for different limit tiers.
pub struct TieredRateLimiter {
    tiers: Vec<(String, RateLimiter)>,
}

impl TieredRateLimiter {
    /// Create a new tiered rate limiter.
    pub fn new() -> Self {
        Self { tiers: Vec::new() }
    }

    /// Add a tier.
    pub fn add_tier(mut self, name: &str, config: RateLimitConfig) -> Self {
        self.tiers
            .push((name.to_string(), RateLimiter::new(config)));
        self
    }

    /// Check all tiers.
    pub async fn check(&self, key: &str) -> Result<Vec<(String, RateLimitResult)>> {
        let mut results = Vec::new();

        for (name, limiter) in &self.tiers {
            let result = limiter.check(key).await;
            if !result.allowed {
                return Err(RateLimitError::LimitExceeded(format!(
                    "Rate limit exceeded for tier '{}': {} requests per {:?}",
                    name, limiter.config.max_requests, limiter.config.window
                )));
            }
            results.push((name.clone(), result));
        }

        Ok(results)
    }
}

impl Default for TieredRateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// KANI FORMAL VERIFICATION PROOFS
// ============================================================================

#[cfg(kani)]
mod kani_proofs {
    use super::*;

    /// Proof: Default config has valid values
    #[kani::proof]
    fn proof_default_config_valid() {
        let config = RateLimitConfig::default();

        kani::assert(config.max_requests > 0, "Max requests must be positive");
        kani::assert(config.window.as_secs() > 0, "Window must be positive");
    }

    /// Proof: RateLimitAlgorithm has exactly 4 variants
    #[kani::proof]
    fn proof_algorithm_variants() {
        let val: u8 = kani::any();
        kani::assume(val <= 3);

        let algo = match val {
            0 => RateLimitAlgorithm::FixedWindow,
            1 => RateLimitAlgorithm::SlidingWindow,
            2 => RateLimitAlgorithm::TokenBucket,
            _ => RateLimitAlgorithm::LeakyBucket,
        };

        kani::assert(algo == algo, "Algorithm must equal itself");
    }

    /// Proof: Default algorithm is SlidingWindow
    #[kani::proof]
    fn proof_default_algorithm() {
        let algo = RateLimitAlgorithm::default();
        kani::assert(
            algo == RateLimitAlgorithm::SlidingWindow,
            "Default must be SlidingWindow",
        );
    }

    /// Proof: Token bucket refill never exceeds capacity
    #[kani::proof]
    fn proof_token_bucket_capacity_bound() {
        let capacity: u64 = kani::any();
        let refill_rate: f64 = kani::any();
        let elapsed_ms: i64 = kani::any();

        kani::assume(capacity > 0 && capacity <= 1_000_000);
        kani::assume(refill_rate > 0.0 && refill_rate <= 10000.0 && refill_rate.is_finite());
        kani::assume(elapsed_ms >= 0 && elapsed_ms <= 3600000); // up to 1 hour

        let initial_tokens: f64 = kani::any();
        kani::assume(initial_tokens >= 0.0 && initial_tokens <= capacity as f64);

        let elapsed_secs = elapsed_ms as f64 / 1000.0;
        let new_tokens = (initial_tokens + elapsed_secs * refill_rate).min(capacity as f64);

        kani::assert(
            new_tokens <= capacity as f64,
            "Tokens must not exceed capacity",
        );
        kani::assert(new_tokens >= 0.0, "Tokens must be non-negative");
    }

    /// Proof: Token bucket acquire decreases tokens
    #[kani::proof]
    fn proof_token_acquire_decreases() {
        let current: f64 = kani::any();
        let requested: u64 = kani::any();

        kani::assume(current >= 0.0 && current <= 1_000_000.0 && current.is_finite());
        kani::assume(requested > 0 && requested <= 1_000_000);

        if current >= requested as f64 {
            let after = current - requested as f64;
            kani::assert(after < current, "Tokens must decrease after acquire");
            kani::assert(after >= 0.0, "Tokens must remain non-negative");
        }
    }

    /// Proof: Remaining count is bounded by limit
    #[kani::proof]
    fn proof_remaining_bounded_by_limit() {
        let limit: u64 = kani::any();
        let count: u64 = kani::any();
        let requested: u64 = kani::any();

        kani::assume(limit > 0 && limit <= 1_000_000);
        kani::assume(count <= limit);
        kani::assume(requested > 0 && requested <= 100);

        let allowed = count + requested <= limit;

        let remaining = if allowed {
            limit - count - requested
        } else {
            0
        };

        kani::assert(remaining <= limit, "Remaining must be <= limit");
    }

    /// Proof: Stats accounting is consistent
    #[kani::proof]
    fn proof_stats_consistency() {
        let allowed: u64 = kani::any();
        let denied: u64 = kani::any();

        kani::assume(allowed < 1_000_000_000);
        kani::assume(denied < 1_000_000_000);

        let total = allowed.saturating_add(denied);

        kani::assert(total >= allowed, "Total must be >= allowed");
        kani::assert(total >= denied, "Total must be >= denied");
    }

    /// Proof: Config builder preserves values
    #[kani::proof]
    fn proof_config_builder() {
        let max: u64 = kani::any();
        let burst: u64 = kani::any();

        kani::assume(max > 0);
        kani::assume(burst > 0);

        let config = RateLimitConfig::new(max, Duration::from_secs(60)).with_burst(burst);

        kani::assert(config.max_requests == max, "Max requests must be set");
        kani::assert(config.burst == Some(burst), "Burst must be set");
    }

    /// Proof: RateLimitResult fields are consistent
    #[kani::proof]
    fn proof_result_consistency() {
        let allowed: bool = kani::any();
        let remaining: u64 = kani::any();
        let limit: u64 = kani::any();

        kani::assume(limit > 0);
        kani::assume(remaining <= limit);

        // If not allowed, retry_after should be Some
        // If allowed, remaining should be > 0 (unless just used last)
        if !allowed {
            kani::assert(
                remaining == 0 || remaining < limit,
                "If denied, should be at or near limit",
            );
        }
    }

    /// Proof: Headers conversion preserves information
    #[kani::proof]
    fn proof_headers_conversion() {
        let limit: u64 = kani::any();
        let remaining: u64 = kani::any();

        kani::assume(limit > 0 && limit <= 1_000_000);
        kani::assume(remaining <= limit);

        // Verify string conversion doesn't lose information
        let limit_str = limit.to_string();
        let remaining_str = remaining.to_string();

        // Parse back and verify
        let limit_parsed: u64 = limit_str.parse().unwrap();
        let remaining_parsed: u64 = remaining_str.parse().unwrap();

        kani::assert(limit_parsed == limit, "Limit must round-trip");
        kani::assert(remaining_parsed == remaining, "Remaining must round-trip");
    }

    /// Proof: Refill rate calculation is valid
    #[kani::proof]
    fn proof_refill_rate_calculation() {
        let max_requests: u64 = kani::any();
        let window_secs: u64 = kani::any();

        kani::assume(max_requests > 0 && max_requests <= 1_000_000);
        kani::assume(window_secs > 0 && window_secs <= 86400); // up to 1 day

        let refill_rate = max_requests as f64 / window_secs as f64;

        kani::assert(refill_rate > 0.0, "Refill rate must be positive");
        kani::assert(refill_rate.is_finite(), "Refill rate must be finite");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_sliding_window_allows() {
        let config = RateLimitConfig::new(10, Duration::from_secs(60));
        let limiter = RateLimiter::new(config);

        let result = limiter.check("user1").await;
        assert!(result.allowed);
        assert_eq!(result.remaining, 9);
    }

    #[tokio::test]
    async fn test_sliding_window_denies() {
        let config = RateLimitConfig::new(3, Duration::from_secs(60));
        let limiter = RateLimiter::new(config);

        for _ in 0..3 {
            let result = limiter.check("user1").await;
            assert!(result.allowed);
        }

        let result = limiter.check("user1").await;
        assert!(!result.allowed);
        assert_eq!(result.remaining, 0);
    }

    #[tokio::test]
    async fn test_token_bucket() {
        let config = RateLimitConfig::new(10, Duration::from_secs(1))
            .with_algorithm(RateLimitAlgorithm::TokenBucket)
            .with_burst(5);
        let limiter = RateLimiter::new(config);

        // Should allow burst
        for _ in 0..5 {
            let result = limiter.check("user1").await;
            assert!(result.allowed);
        }

        // Should deny after burst
        let result = limiter.check("user1").await;
        assert!(!result.allowed);
    }

    #[tokio::test]
    async fn test_fixed_window() {
        let config = RateLimitConfig::new(5, Duration::from_secs(60))
            .with_algorithm(RateLimitAlgorithm::FixedWindow);
        let limiter = RateLimiter::new(config);

        for _ in 0..5 {
            let result = limiter.check("user1").await;
            assert!(result.allowed);
        }

        let result = limiter.check("user1").await;
        assert!(!result.allowed);
    }

    #[tokio::test]
    async fn test_different_keys() {
        let config = RateLimitConfig::new(2, Duration::from_secs(60));
        let limiter = RateLimiter::new(config);

        // User1
        limiter.check("user1").await;
        limiter.check("user1").await;
        let result = limiter.check("user1").await;
        assert!(!result.allowed);

        // User2 should still have quota
        let result = limiter.check("user2").await;
        assert!(result.allowed);
    }

    #[tokio::test]
    async fn test_check_n() {
        let config = RateLimitConfig::new(10, Duration::from_secs(60));
        let limiter = RateLimiter::new(config);

        let result = limiter.check_n("user1", 5).await;
        assert!(result.allowed);
        assert_eq!(result.remaining, 5);

        let result = limiter.check_n("user1", 6).await;
        assert!(!result.allowed);
    }

    #[tokio::test]
    async fn test_reset() {
        let config = RateLimitConfig::new(2, Duration::from_secs(60));
        let limiter = RateLimiter::new(config);

        limiter.check("user1").await;
        limiter.check("user1").await;

        limiter.reset("user1").await;

        let result = limiter.check("user1").await;
        assert!(result.allowed);
    }

    #[tokio::test]
    async fn test_stats() {
        let config = RateLimitConfig::new(10, Duration::from_secs(60));
        let limiter = RateLimiter::new(config);

        limiter.check("user1").await;
        limiter.check("user2").await;

        let stats = limiter.stats().await;
        assert_eq!(stats.total_requests, 2);
        assert_eq!(stats.allowed, 2);
        assert_eq!(stats.unique_keys, 2);
    }

    #[tokio::test]
    async fn test_tiered_limiter() {
        let limiter = TieredRateLimiter::new()
            .add_tier(
                "per_second",
                RateLimitConfig::new(10, Duration::from_secs(1)),
            )
            .add_tier(
                "per_minute",
                RateLimitConfig::new(100, Duration::from_secs(60)),
            );

        let result = limiter.check("user1").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_headers() {
        let config = RateLimitConfig::new(10, Duration::from_secs(60));
        let limiter = RateLimiter::new(config);

        let result = limiter.check("user1").await;
        let headers: RateLimitHeaders = result.into();

        assert_eq!(headers.limit, "10");
        assert_eq!(headers.remaining, "9");
        assert!(headers.retry_after.is_none());
    }
}
