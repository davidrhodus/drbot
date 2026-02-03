//! Rate limiter for drbot.
//!
//! This crate provides:
//! - Token bucket algorithm
//! - Sliding window
//! - Fixed window

use std::sync::Mutex;
use std::time::{Duration, Instant};
use thiserror::Error;

/// Rate limiter error types.
#[derive(Error, Debug, Clone)]
pub enum RateLimitError {
    #[error("Rate limit exceeded")]
    Exceeded,

    #[error("Rate limit exceeded, retry after {0:?}")]
    ExceededRetryAfter(Duration),
}

/// Result type for rate limiter operations.
pub type Result<T> = std::result::Result<T, RateLimitError>;

/// Token bucket rate limiter.
pub struct TokenBucket {
    capacity: u32,
    tokens: Mutex<f64>,
    refill_rate: f64, // tokens per second
    last_refill: Mutex<Instant>,
}

impl TokenBucket {
    /// Create new token bucket.
    pub fn new(capacity: u32, refill_rate: f64) -> Self {
        Self {
            capacity,
            tokens: Mutex::new(capacity as f64),
            refill_rate,
            last_refill: Mutex::new(Instant::now()),
        }
    }

    /// Create with requests per second.
    pub fn per_second(requests: u32) -> Self {
        Self::new(requests, requests as f64)
    }

    /// Create with requests per minute.
    pub fn per_minute(requests: u32) -> Self {
        Self::new(requests, requests as f64 / 60.0)
    }

    /// Try to acquire a token.
    pub fn try_acquire(&self) -> Result<()> {
        self.try_acquire_n(1)
    }

    /// Try to acquire n tokens.
    pub fn try_acquire_n(&self, n: u32) -> Result<()> {
        let mut tokens = self.tokens.lock().unwrap();
        let mut last_refill = self.last_refill.lock().unwrap();

        // Refill tokens
        let now = Instant::now();
        let elapsed = now.duration_since(*last_refill);
        let refill = elapsed.as_secs_f64() * self.refill_rate;
        *tokens = (*tokens + refill).min(self.capacity as f64);
        *last_refill = now;

        // Check if we have enough tokens
        let needed = n as f64;
        if *tokens >= needed {
            *tokens -= needed;
            Ok(())
        } else {
            // Calculate wait time
            let deficit = needed - *tokens;
            let wait_secs = deficit / self.refill_rate;
            Err(RateLimitError::ExceededRetryAfter(Duration::from_secs_f64(
                wait_secs,
            )))
        }
    }

    /// Get available tokens.
    pub fn available(&self) -> u32 {
        let tokens = self.tokens.lock().unwrap();
        *tokens as u32
    }
}

/// Sliding window rate limiter.
pub struct SlidingWindow {
    window_size: Duration,
    max_requests: u32,
    requests: Mutex<Vec<Instant>>,
}

impl SlidingWindow {
    /// Create new sliding window limiter.
    pub fn new(window_size: Duration, max_requests: u32) -> Self {
        Self {
            window_size,
            max_requests,
            requests: Mutex::new(Vec::new()),
        }
    }

    /// Create with requests per second.
    pub fn per_second(max: u32) -> Self {
        Self::new(Duration::from_secs(1), max)
    }

    /// Create with requests per minute.
    pub fn per_minute(max: u32) -> Self {
        Self::new(Duration::from_secs(60), max)
    }

    /// Try to acquire.
    pub fn try_acquire(&self) -> Result<()> {
        let mut requests = self.requests.lock().unwrap();
        let now = Instant::now();

        // Remove old requests outside the window
        let cutoff = now - self.window_size;
        requests.retain(|&t| t > cutoff);

        // Check if under limit
        if requests.len() < self.max_requests as usize {
            requests.push(now);
            Ok(())
        } else {
            // Calculate when oldest request will expire
            if let Some(&oldest) = requests.first() {
                let expires_in = self.window_size - (now - oldest);
                Err(RateLimitError::ExceededRetryAfter(expires_in))
            } else {
                Err(RateLimitError::Exceeded)
            }
        }
    }

    /// Get current request count in window.
    pub fn current_count(&self) -> u32 {
        let mut requests = self.requests.lock().unwrap();
        let cutoff = Instant::now() - self.window_size;
        requests.retain(|&t| t > cutoff);
        requests.len() as u32
    }
}

/// Fixed window rate limiter.
pub struct FixedWindow {
    window_size: Duration,
    max_requests: u32,
    count: Mutex<u32>,
    window_start: Mutex<Instant>,
}

impl FixedWindow {
    /// Create new fixed window limiter.
    pub fn new(window_size: Duration, max_requests: u32) -> Self {
        Self {
            window_size,
            max_requests,
            count: Mutex::new(0),
            window_start: Mutex::new(Instant::now()),
        }
    }

    /// Create with requests per second.
    pub fn per_second(max: u32) -> Self {
        Self::new(Duration::from_secs(1), max)
    }

    /// Create with requests per minute.
    pub fn per_minute(max: u32) -> Self {
        Self::new(Duration::from_secs(60), max)
    }

    /// Try to acquire.
    pub fn try_acquire(&self) -> Result<()> {
        let mut count = self.count.lock().unwrap();
        let mut window_start = self.window_start.lock().unwrap();
        let now = Instant::now();

        // Check if window has expired
        if now.duration_since(*window_start) >= self.window_size {
            *window_start = now;
            *count = 0;
        }

        // Check if under limit
        if *count < self.max_requests {
            *count += 1;
            Ok(())
        } else {
            let remaining = self.window_size - now.duration_since(*window_start);
            Err(RateLimitError::ExceededRetryAfter(remaining))
        }
    }

    /// Get current count in window.
    pub fn current_count(&self) -> u32 {
        *self.count.lock().unwrap()
    }
}

/// Composite rate limiter (all must pass).
pub struct CompositeRateLimiter {
    limiters: Vec<Box<dyn RateLimiter + Send + Sync>>,
}

/// Rate limiter trait.
pub trait RateLimiter {
    /// Try to acquire.
    fn try_acquire(&self) -> Result<()>;
}

impl RateLimiter for TokenBucket {
    fn try_acquire(&self) -> Result<()> {
        TokenBucket::try_acquire(self)
    }
}

impl RateLimiter for SlidingWindow {
    fn try_acquire(&self) -> Result<()> {
        SlidingWindow::try_acquire(self)
    }
}

impl RateLimiter for FixedWindow {
    fn try_acquire(&self) -> Result<()> {
        FixedWindow::try_acquire(self)
    }
}

impl CompositeRateLimiter {
    /// Create new composite limiter.
    pub fn new() -> Self {
        Self {
            limiters: Vec::new(),
        }
    }

    /// Add limiter.
    pub fn add<L: RateLimiter + Send + Sync + 'static>(mut self, limiter: L) -> Self {
        self.limiters.push(Box::new(limiter));
        self
    }
}

impl Default for CompositeRateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

impl RateLimiter for CompositeRateLimiter {
    fn try_acquire(&self) -> Result<()> {
        for limiter in &self.limiters {
            limiter.try_acquire()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_bucket() {
        let bucket = TokenBucket::new(3, 1.0);

        assert!(bucket.try_acquire().is_ok());
        assert!(bucket.try_acquire().is_ok());
        assert!(bucket.try_acquire().is_ok());
        assert!(bucket.try_acquire().is_err()); // Exceeded
    }

    #[test]
    fn test_sliding_window() {
        let window = SlidingWindow::new(Duration::from_secs(1), 2);

        assert!(window.try_acquire().is_ok());
        assert!(window.try_acquire().is_ok());
        assert!(window.try_acquire().is_err());
    }

    #[test]
    fn test_fixed_window() {
        let window = FixedWindow::new(Duration::from_secs(1), 2);

        assert!(window.try_acquire().is_ok());
        assert!(window.try_acquire().is_ok());
        assert!(window.try_acquire().is_err());
    }

    #[test]
    fn test_composite() {
        let limiter = CompositeRateLimiter::new()
            .add(TokenBucket::new(5, 1.0))
            .add(FixedWindow::new(Duration::from_secs(1), 3));

        assert!(limiter.try_acquire().is_ok());
        assert!(limiter.try_acquire().is_ok());
        assert!(limiter.try_acquire().is_ok());
        assert!(limiter.try_acquire().is_err()); // Fixed window blocks
    }
}
