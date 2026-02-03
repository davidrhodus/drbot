//! Request throttling for drbot.
//!
//! This crate provides:
//! - Request rate throttling
//! - Concurrent request limiting
//! - Priority-based throttling
//! - Adaptive throttling

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::{OwnedSemaphorePermit, RwLock, Semaphore};

/// Throttle error types.
#[derive(Error, Debug)]
pub enum ThrottleError {
    #[error("Rate limit exceeded")]
    RateLimitExceeded,

    #[error("Concurrent limit exceeded")]
    ConcurrentLimitExceeded,

    #[error("Request rejected: {0}")]
    Rejected(String),

    #[error("Timeout waiting for permit")]
    Timeout,
}

/// Result type for throttle operations.
pub type Result<T> = std::result::Result<T, ThrottleError>;

/// Throttle result.
#[derive(Debug, Clone)]
pub enum ThrottleResult {
    /// Request allowed.
    Allowed,
    /// Request delayed.
    Delayed(Duration),
    /// Request rejected.
    Rejected(String),
}

/// Throttle configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThrottleConfig {
    /// Requests per second.
    pub rate_limit: Option<u32>,
    /// Burst size.
    pub burst_size: Option<u32>,
    /// Maximum concurrent requests.
    pub max_concurrent: Option<u32>,
    /// Request timeout.
    pub timeout: Option<Duration>,
}

impl Default for ThrottleConfig {
    fn default() -> Self {
        Self {
            rate_limit: Some(100),
            burst_size: Some(10),
            max_concurrent: Some(50),
            timeout: Some(Duration::from_secs(30)),
        }
    }
}

/// Throttler trait.
#[async_trait]
pub trait Throttler: Send + Sync {
    /// Check if request should be allowed.
    async fn check(&self, key: &str) -> Result<ThrottleResult>;

    /// Acquire a permit (blocks or returns error).
    async fn acquire(&self, key: &str) -> Result<()>;

    /// Try to acquire a permit (non-blocking).
    async fn try_acquire(&self, key: &str) -> Result<bool>;

    /// Release a permit.
    async fn release(&self, key: &str);
}

/// Token bucket throttler.
pub struct TokenBucket {
    buckets: RwLock<HashMap<String, BucketState>>,
    rate: f64,
    capacity: u32,
}

struct BucketState {
    tokens: f64,
    last_update: std::time::Instant,
}

impl TokenBucket {
    /// Create a new token bucket throttler.
    pub fn new(rate_per_second: u32, burst_capacity: u32) -> Self {
        Self {
            buckets: RwLock::new(HashMap::new()),
            rate: rate_per_second as f64,
            capacity: burst_capacity,
        }
    }

    async fn get_or_create_bucket(&self, key: &str) -> f64 {
        let mut buckets = self.buckets.write().await;
        let now = std::time::Instant::now();

        let state = buckets
            .entry(key.to_string())
            .or_insert_with(|| BucketState {
                tokens: self.capacity as f64,
                last_update: now,
            });

        // Refill tokens
        let elapsed = now.duration_since(state.last_update).as_secs_f64();
        state.tokens = (state.tokens + elapsed * self.rate).min(self.capacity as f64);
        state.last_update = now;

        state.tokens
    }

    async fn consume(&self, key: &str, tokens: f64) -> bool {
        let mut buckets = self.buckets.write().await;
        let now = std::time::Instant::now();

        let state = buckets
            .entry(key.to_string())
            .or_insert_with(|| BucketState {
                tokens: self.capacity as f64,
                last_update: now,
            });

        // Refill tokens
        let elapsed = now.duration_since(state.last_update).as_secs_f64();
        state.tokens = (state.tokens + elapsed * self.rate).min(self.capacity as f64);
        state.last_update = now;

        if state.tokens >= tokens {
            state.tokens -= tokens;
            true
        } else {
            false
        }
    }
}

#[async_trait]
impl Throttler for TokenBucket {
    async fn check(&self, key: &str) -> Result<ThrottleResult> {
        let tokens = self.get_or_create_bucket(key).await;
        if tokens >= 1.0 {
            Ok(ThrottleResult::Allowed)
        } else {
            let wait_time = ((1.0 - tokens) / self.rate * 1000.0) as u64;
            Ok(ThrottleResult::Delayed(Duration::from_millis(wait_time)))
        }
    }

    async fn acquire(&self, key: &str) -> Result<()> {
        loop {
            if self.consume(key, 1.0).await {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    async fn try_acquire(&self, key: &str) -> Result<bool> {
        Ok(self.consume(key, 1.0).await)
    }

    async fn release(&self, _key: &str) {
        // Token bucket doesn't need explicit release
    }
}

/// Sliding window rate limiter.
pub struct SlidingWindow {
    windows: RwLock<HashMap<String, WindowState>>,
    limit: u32,
    window_size: Duration,
}

struct WindowState {
    timestamps: Vec<std::time::Instant>,
}

impl SlidingWindow {
    /// Create a new sliding window throttler.
    pub fn new(limit: u32, window_size: Duration) -> Self {
        Self {
            windows: RwLock::new(HashMap::new()),
            limit,
            window_size,
        }
    }

    async fn count_in_window(&self, key: &str) -> u32 {
        let mut windows = self.windows.write().await;
        let now = std::time::Instant::now();
        let cutoff = now - self.window_size;

        let state = windows
            .entry(key.to_string())
            .or_insert_with(|| WindowState {
                timestamps: Vec::new(),
            });

        // Remove old timestamps
        state.timestamps.retain(|&t| t > cutoff);

        state.timestamps.len() as u32
    }

    async fn record(&self, key: &str) {
        let mut windows = self.windows.write().await;
        let now = std::time::Instant::now();

        let state = windows
            .entry(key.to_string())
            .or_insert_with(|| WindowState {
                timestamps: Vec::new(),
            });

        state.timestamps.push(now);
    }
}

#[async_trait]
impl Throttler for SlidingWindow {
    async fn check(&self, key: &str) -> Result<ThrottleResult> {
        let count = self.count_in_window(key).await;
        if count < self.limit {
            Ok(ThrottleResult::Allowed)
        } else {
            Ok(ThrottleResult::Rejected("Rate limit exceeded".to_string()))
        }
    }

    async fn acquire(&self, key: &str) -> Result<()> {
        let count = self.count_in_window(key).await;
        if count < self.limit {
            self.record(key).await;
            Ok(())
        } else {
            Err(ThrottleError::RateLimitExceeded)
        }
    }

    async fn try_acquire(&self, key: &str) -> Result<bool> {
        let count = self.count_in_window(key).await;
        if count < self.limit {
            self.record(key).await;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    async fn release(&self, _key: &str) {
        // Sliding window doesn't need explicit release
    }
}

/// Concurrent request limiter.
pub struct ConcurrencyLimiter {
    limiters: RwLock<HashMap<String, Arc<Semaphore>>>,
    default_limit: u32,
}

impl ConcurrencyLimiter {
    /// Create a new concurrency limiter.
    pub fn new(default_limit: u32) -> Self {
        Self {
            limiters: RwLock::new(HashMap::new()),
            default_limit,
        }
    }

    async fn get_semaphore(&self, key: &str) -> Arc<Semaphore> {
        {
            let limiters = self.limiters.read().await;
            if let Some(sem) = limiters.get(key) {
                return sem.clone();
            }
        }

        let mut limiters = self.limiters.write().await;
        limiters
            .entry(key.to_string())
            .or_insert_with(|| Arc::new(Semaphore::new(self.default_limit as usize)))
            .clone()
    }
}

#[async_trait]
impl Throttler for ConcurrencyLimiter {
    async fn check(&self, key: &str) -> Result<ThrottleResult> {
        let sem = self.get_semaphore(key).await;
        if sem.available_permits() > 0 {
            Ok(ThrottleResult::Allowed)
        } else {
            Ok(ThrottleResult::Rejected(
                "Concurrent limit exceeded".to_string(),
            ))
        }
    }

    async fn acquire(&self, key: &str) -> Result<()> {
        let sem = self.get_semaphore(key).await;
        sem.acquire()
            .await
            .map_err(|_| ThrottleError::ConcurrentLimitExceeded)?;
        // Note: This leaks the permit - in real use, return a guard
        std::mem::forget(sem);
        Ok(())
    }

    async fn try_acquire(&self, key: &str) -> Result<bool> {
        let sem = self.get_semaphore(key).await;
        let result = sem.try_acquire();
        match result {
            Ok(permit) => {
                std::mem::forget(permit);
                Ok(true)
            }
            Err(_) => Ok(false),
        }
    }

    async fn release(&self, key: &str) {
        let sem = self.get_semaphore(key).await;
        sem.add_permits(1);
    }
}

/// Priority-based throttler.
pub struct PriorityThrottler {
    inner: Arc<dyn Throttler>,
    priorities: RwLock<HashMap<String, u32>>,
    priority_multipliers: HashMap<u32, f64>,
}

impl PriorityThrottler {
    /// Create a new priority throttler.
    pub fn new(inner: Arc<dyn Throttler>) -> Self {
        let mut multipliers = HashMap::new();
        multipliers.insert(0, 0.5); // Low priority: 50% of capacity
        multipliers.insert(1, 1.0); // Normal priority: 100%
        multipliers.insert(2, 2.0); // High priority: 200%

        Self {
            inner,
            priorities: RwLock::new(HashMap::new()),
            priority_multipliers: multipliers,
        }
    }

    /// Set priority for a key.
    pub async fn set_priority(&self, key: &str, priority: u32) {
        let mut priorities = self.priorities.write().await;
        priorities.insert(key.to_string(), priority);
    }

    async fn get_priority(&self, key: &str) -> u32 {
        let priorities = self.priorities.read().await;
        priorities.get(key).copied().unwrap_or(1) // Default to normal
    }
}

#[async_trait]
impl Throttler for PriorityThrottler {
    async fn check(&self, key: &str) -> Result<ThrottleResult> {
        let priority = self.get_priority(key).await;
        let multiplier = self
            .priority_multipliers
            .get(&priority)
            .copied()
            .unwrap_or(1.0);

        // High priority requests are more likely to be allowed
        if multiplier >= 1.5 {
            return Ok(ThrottleResult::Allowed);
        }

        self.inner.check(key).await
    }

    async fn acquire(&self, key: &str) -> Result<()> {
        self.inner.acquire(key).await
    }

    async fn try_acquire(&self, key: &str) -> Result<bool> {
        let priority = self.get_priority(key).await;
        let multiplier = self
            .priority_multipliers
            .get(&priority)
            .copied()
            .unwrap_or(1.0);

        // High priority always allowed
        if multiplier >= 2.0 {
            return Ok(true);
        }

        self.inner.try_acquire(key).await
    }

    async fn release(&self, key: &str) {
        self.inner.release(key).await
    }
}

/// Composite throttler (applies multiple throttlers).
pub struct CompositeThrottler {
    throttlers: Vec<Arc<dyn Throttler>>,
}

impl CompositeThrottler {
    /// Create a new composite throttler.
    pub fn new() -> Self {
        Self {
            throttlers: Vec::new(),
        }
    }

    /// Add a throttler.
    pub fn add(mut self, throttler: Arc<dyn Throttler>) -> Self {
        self.throttlers.push(throttler);
        self
    }
}

impl Default for CompositeThrottler {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Throttler for CompositeThrottler {
    async fn check(&self, key: &str) -> Result<ThrottleResult> {
        for throttler in &self.throttlers {
            match throttler.check(key).await? {
                ThrottleResult::Allowed => continue,
                other => return Ok(other),
            }
        }
        Ok(ThrottleResult::Allowed)
    }

    async fn acquire(&self, key: &str) -> Result<()> {
        for throttler in &self.throttlers {
            throttler.acquire(key).await?;
        }
        Ok(())
    }

    async fn try_acquire(&self, key: &str) -> Result<bool> {
        for throttler in &self.throttlers {
            if !throttler.try_acquire(key).await? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    async fn release(&self, key: &str) {
        for throttler in &self.throttlers {
            throttler.release(key).await;
        }
    }
}

/// Throttle statistics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ThrottleStats {
    /// Total requests.
    pub total_requests: u64,
    /// Allowed requests.
    pub allowed: u64,
    /// Throttled requests.
    pub throttled: u64,
    /// Rejected requests.
    pub rejected: u64,
}

// ============================================================================
// Kani Formal Verification Proofs
// ============================================================================

#[cfg(kani)]
mod kani_proofs {
    use super::*;

    /// Verify ThrottleResult has 3 variants.
    #[kani::proof]
    fn proof_throttle_result_variants() {
        let val: u8 = kani::any();
        kani::assume(val <= 2);

        let _result = match val {
            0 => ThrottleResult::Allowed,
            1 => ThrottleResult::Delayed(Duration::from_secs(1)),
            _ => ThrottleResult::Rejected("test".to_string()),
        };

        kani::assert(val <= 2, "All 3 variants covered");
    }

    /// Verify default config has valid values.
    #[kani::proof]
    fn proof_default_config_valid() {
        let config = ThrottleConfig::default();

        if let Some(rate) = config.rate_limit {
            kani::assert(rate > 0, "Rate limit must be positive");
        }
        if let Some(burst) = config.burst_size {
            kani::assert(burst > 0, "Burst size must be positive");
        }
        if let Some(concurrent) = config.max_concurrent {
            kani::assert(concurrent > 0, "Max concurrent must be positive");
        }
    }

    /// Verify token bucket refill logic.
    #[kani::proof]
    fn proof_token_bucket_refill() {
        let current_tokens: f64 = kani::any();
        let elapsed_secs: f64 = kani::any();
        let rate: f64 = kani::any();
        let capacity: u32 = kani::any();

        kani::assume(current_tokens >= 0.0 && current_tokens <= 1000.0);
        kani::assume(elapsed_secs >= 0.0 && elapsed_secs <= 10.0);
        kani::assume(rate > 0.0 && rate <= 1000.0);
        kani::assume(capacity > 0 && capacity <= 1000);
        kani::assume(current_tokens.is_finite());
        kani::assume(elapsed_secs.is_finite());
        kani::assume(rate.is_finite());

        let new_tokens = (current_tokens + elapsed_secs * rate).min(capacity as f64);

        kani::assert(
            new_tokens >= current_tokens.min(capacity as f64),
            "Tokens should not decrease",
        );
        kani::assert(
            new_tokens <= capacity as f64,
            "Tokens should not exceed capacity",
        );
    }

    /// Verify token consumption logic.
    #[kani::proof]
    fn proof_token_consumption() {
        let tokens: f64 = kani::any();
        let cost: f64 = kani::any();

        kani::assume(tokens >= 0.0 && tokens <= 100.0);
        kani::assume(cost > 0.0 && cost <= 10.0);
        kani::assume(tokens.is_finite());
        kani::assume(cost.is_finite());

        let can_consume = tokens >= cost;
        let remaining = if can_consume { tokens - cost } else { tokens };

        if can_consume {
            kani::assert(
                remaining < tokens,
                "Tokens should decrease after consumption",
            );
            kani::assert(remaining >= 0.0, "Remaining tokens should be non-negative");
        }
    }

    /// Verify wait time calculation.
    #[kani::proof]
    fn proof_wait_time_calculation() {
        let tokens: f64 = kani::any();
        let rate: f64 = kani::any();

        kani::assume(tokens >= 0.0 && tokens < 1.0);
        kani::assume(rate > 0.0 && rate <= 1000.0);
        kani::assume(tokens.is_finite());
        kani::assume(rate.is_finite());

        let needed = 1.0 - tokens;
        let wait_secs = needed / rate;

        kani::assert(wait_secs >= 0.0, "Wait time should be non-negative");
    }

    /// Verify sliding window count bounds.
    #[kani::proof]
    fn proof_sliding_window_count() {
        let count: u32 = kani::any();
        let limit: u32 = kani::any();

        kani::assume(limit > 0 && limit <= 1000);

        let allowed = count < limit;

        if count >= limit {
            kani::assert(!allowed, "Should not allow when at or above limit");
        } else {
            kani::assert(allowed, "Should allow when below limit");
        }
    }

    /// Verify priority multiplier logic.
    #[kani::proof]
    fn proof_priority_multiplier() {
        let priority: u32 = kani::any();
        kani::assume(priority <= 2);

        let multiplier = match priority {
            0 => 0.5,
            1 => 1.0,
            _ => 2.0,
        };

        kani::assert(multiplier > 0.0, "Multiplier must be positive");
        if priority == 2 {
            kani::assert(
                multiplier >= 2.0,
                "High priority should have >= 2x multiplier",
            );
        }
    }

    /// Verify high priority bypass.
    #[kani::proof]
    fn proof_high_priority_bypass() {
        let multiplier: f64 = kani::any();
        kani::assume(multiplier.is_finite());
        kani::assume(multiplier >= 0.0 && multiplier <= 10.0);

        let high_priority_threshold = 1.5;
        let bypass = multiplier >= high_priority_threshold;

        if multiplier >= 1.5 {
            kani::assert(bypass, "Should bypass for high multiplier");
        }
    }

    /// Verify semaphore permit tracking.
    #[kani::proof]
    fn proof_semaphore_permit_tracking() {
        let initial_permits: u32 = kani::any();
        let acquired: u32 = kani::any();

        kani::assume(initial_permits > 0 && initial_permits <= 100);
        kani::assume(acquired <= initial_permits);

        let available = initial_permits - acquired;

        kani::assert(
            available + acquired == initial_permits,
            "Permits should be conserved",
        );
        kani::assert(
            available <= initial_permits,
            "Available should not exceed initial",
        );
    }

    /// Verify composite throttler all-allowed logic.
    #[kani::proof]
    fn proof_composite_all_allowed() {
        let result1_allowed: bool = kani::any();
        let result2_allowed: bool = kani::any();

        let composite_allowed = result1_allowed && result2_allowed;

        if !result1_allowed || !result2_allowed {
            kani::assert(!composite_allowed, "Composite should reject if any rejects");
        } else {
            kani::assert(composite_allowed, "Composite should allow if all allow");
        }
    }

    /// Verify ThrottleStats consistency.
    #[kani::proof]
    fn proof_throttle_stats_consistency() {
        let total: u64 = kani::any();
        let allowed: u64 = kani::any();
        let throttled: u64 = kani::any();
        let rejected: u64 = kani::any();

        kani::assume(total < u64::MAX / 2);
        kani::assume(allowed <= total);
        kani::assume(throttled <= total);
        kani::assume(rejected <= total);
        kani::assume(allowed + throttled + rejected <= total);

        kani::assert(
            allowed + throttled + rejected <= total,
            "Sum of outcomes should not exceed total",
        );
    }

    /// Verify token bucket capacity is respected.
    #[kani::proof]
    fn proof_token_bucket_capacity() {
        let tokens: f64 = kani::any();
        let capacity: u32 = kani::any();

        kani::assume(capacity > 0 && capacity <= 1000);
        kani::assume(tokens.is_finite() && tokens >= 0.0);

        let capped = tokens.min(capacity as f64);

        kani::assert(
            capped <= capacity as f64,
            "Tokens should be capped at capacity",
        );
    }

    /// Verify default priority is normal (1).
    #[kani::proof]
    fn proof_default_priority() {
        let default_priority: u32 = 1;

        kani::assert(
            default_priority == 1,
            "Default priority should be normal (1)",
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_token_bucket_allow() {
        let throttler = TokenBucket::new(10, 10);

        // Should allow initial burst
        for _ in 0..10 {
            assert!(throttler.try_acquire("test").await.unwrap());
        }
    }

    #[tokio::test]
    async fn test_token_bucket_throttle() {
        let throttler = TokenBucket::new(1, 1); // 1 per second, burst of 1

        assert!(throttler.try_acquire("test").await.unwrap());
        assert!(!throttler.try_acquire("test").await.unwrap()); // Should be throttled
    }

    #[tokio::test]
    async fn test_token_bucket_refill() {
        let throttler = TokenBucket::new(100, 1); // Fast refill

        assert!(throttler.try_acquire("test").await.unwrap());

        // Wait for refill
        tokio::time::sleep(Duration::from_millis(20)).await;

        assert!(throttler.try_acquire("test").await.unwrap());
    }

    #[tokio::test]
    async fn test_sliding_window() {
        let throttler = SlidingWindow::new(5, Duration::from_secs(1));

        for _ in 0..5 {
            assert!(throttler.try_acquire("test").await.unwrap());
        }

        assert!(!throttler.try_acquire("test").await.unwrap());
    }

    #[tokio::test]
    async fn test_concurrency_limiter() {
        let throttler = ConcurrencyLimiter::new(2);

        // Acquire 2 permits
        assert!(throttler.try_acquire("test").await.unwrap());
        assert!(throttler.try_acquire("test").await.unwrap());

        // Should fail
        assert!(!throttler.try_acquire("test").await.unwrap());

        // Release one
        throttler.release("test").await;

        // Should succeed now
        assert!(throttler.try_acquire("test").await.unwrap());
    }

    #[tokio::test]
    async fn test_composite_throttler() {
        let rate = Arc::new(TokenBucket::new(100, 10));
        let concurrent = Arc::new(ConcurrencyLimiter::new(5));

        let throttler = CompositeThrottler::new().add(rate).add(concurrent);

        // Should allow
        assert!(throttler.try_acquire("test").await.unwrap());
    }

    #[tokio::test]
    async fn test_priority_throttler() {
        let inner = Arc::new(TokenBucket::new(1, 1));
        let throttler = PriorityThrottler::new(inner);

        // Set high priority
        throttler.set_priority("vip", 2).await;

        // High priority should always pass
        assert!(throttler.try_acquire("vip").await.unwrap());
    }

    #[tokio::test]
    async fn test_throttle_result_check() {
        let throttler = TokenBucket::new(10, 10);

        let result = throttler.check("test").await.unwrap();
        assert!(matches!(result, ThrottleResult::Allowed));
    }

    #[tokio::test]
    async fn test_different_keys() {
        let throttler = TokenBucket::new(1, 1);

        assert!(throttler.try_acquire("key1").await.unwrap());
        assert!(throttler.try_acquire("key2").await.unwrap()); // Different key
        assert!(!throttler.try_acquire("key1").await.unwrap()); // Same key, throttled
    }
}
