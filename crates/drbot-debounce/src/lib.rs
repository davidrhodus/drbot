//! Debouncing and throttling utilities for drbot.
//!
//! This crate provides:
//! - Debouncing
//! - Throttling
//! - Rate limiting helpers
//! - Coalescing

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use thiserror::Error;

/// Debounce error types.
#[derive(Error, Debug)]
pub enum DebounceError {
    #[error("Already pending")]
    AlreadyPending,

    #[error("Cancelled")]
    Cancelled,
}

/// Result type for debounce operations.
pub type Result<T> = std::result::Result<T, DebounceError>;

/// Debouncer that coalesces rapid calls.
pub struct Debouncer {
    delay: Duration,
    last_call: Mutex<Option<Instant>>,
    pending: std::sync::atomic::AtomicBool,
}

impl Debouncer {
    /// Create new debouncer.
    pub fn new(delay: Duration) -> Self {
        Self {
            delay,
            last_call: Mutex::new(None),
            pending: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Check if should execute now.
    pub fn should_execute(&self) -> bool {
        let now = Instant::now();
        let mut last = self.last_call.lock().unwrap();

        match *last {
            Some(t) if now.duration_since(t) < self.delay => false,
            _ => {
                *last = Some(now);
                true
            }
        }
    }

    /// Execute if debounce period has passed.
    pub fn execute<F, R>(&self, f: F) -> Option<R>
    where
        F: FnOnce() -> R,
    {
        if self.should_execute() {
            Some(f())
        } else {
            None
        }
    }

    /// Reset the debouncer.
    pub fn reset(&self) {
        *self.last_call.lock().unwrap() = None;
    }

    /// Get remaining time until next execution allowed.
    pub fn remaining(&self) -> Duration {
        let last = self.last_call.lock().unwrap();
        match *last {
            Some(t) => {
                let elapsed = t.elapsed();
                if elapsed >= self.delay {
                    Duration::ZERO
                } else {
                    self.delay - elapsed
                }
            }
            None => Duration::ZERO,
        }
    }
}

/// Throttler that limits execution rate.
pub struct Throttler {
    interval: Duration,
    last_execution: Mutex<Option<Instant>>,
}

impl Throttler {
    /// Create new throttler.
    pub fn new(interval: Duration) -> Self {
        Self {
            interval,
            last_execution: Mutex::new(None),
        }
    }

    /// Try to execute (returns false if throttled).
    pub fn try_execute<F, R>(&self, f: F) -> Option<R>
    where
        F: FnOnce() -> R,
    {
        let now = Instant::now();
        let mut last = self.last_execution.lock().unwrap();

        match *last {
            Some(t) if now.duration_since(t) < self.interval => None,
            _ => {
                *last = Some(now);
                Some(f())
            }
        }
    }

    /// Check if throttled.
    pub fn is_throttled(&self) -> bool {
        let last = self.last_execution.lock().unwrap();
        match *last {
            Some(t) => t.elapsed() < self.interval,
            None => false,
        }
    }

    /// Get wait time.
    pub fn wait_time(&self) -> Duration {
        let last = self.last_execution.lock().unwrap();
        match *last {
            Some(t) => {
                let elapsed = t.elapsed();
                if elapsed >= self.interval {
                    Duration::ZERO
                } else {
                    self.interval - elapsed
                }
            }
            None => Duration::ZERO,
        }
    }

    /// Reset throttler.
    pub fn reset(&self) {
        *self.last_execution.lock().unwrap() = None;
    }
}

/// Leaky bucket rate limiter.
pub struct LeakyBucket {
    capacity: u64,
    current: AtomicU64,
    leak_rate: f64,
    last_update: Mutex<Instant>,
}

impl LeakyBucket {
    /// Create new bucket.
    pub fn new(capacity: u64, leak_rate: f64) -> Self {
        Self {
            capacity,
            current: AtomicU64::new(0),
            leak_rate,
            last_update: Mutex::new(Instant::now()),
        }
    }

    /// Try to add tokens.
    pub fn try_acquire(&self, tokens: u64) -> bool {
        self.leak();

        let current = self.current.load(Ordering::SeqCst);
        if current + tokens <= self.capacity {
            self.current.store(current + tokens, Ordering::SeqCst);
            true
        } else {
            false
        }
    }

    /// Leak tokens based on elapsed time.
    fn leak(&self) {
        let mut last = self.last_update.lock().unwrap();
        let now = Instant::now();
        let elapsed = now.duration_since(*last).as_secs_f64();

        let leaked = (elapsed * self.leak_rate) as u64;
        if leaked > 0 {
            let current = self.current.load(Ordering::SeqCst);
            self.current
                .store(current.saturating_sub(leaked), Ordering::SeqCst);
            *last = now;
        }
    }

    /// Get current level.
    pub fn level(&self) -> u64 {
        self.leak();
        self.current.load(Ordering::SeqCst)
    }

    /// Get available capacity.
    pub fn available(&self) -> u64 {
        self.capacity - self.level()
    }
}

/// Token bucket rate limiter.
pub struct TokenBucket {
    capacity: u64,
    tokens: AtomicU64,
    refill_rate: f64,
    last_refill: Mutex<Instant>,
}

impl TokenBucket {
    /// Create new bucket.
    pub fn new(capacity: u64, refill_rate: f64) -> Self {
        Self {
            capacity,
            tokens: AtomicU64::new(capacity),
            refill_rate,
            last_refill: Mutex::new(Instant::now()),
        }
    }

    /// Try to acquire tokens.
    pub fn try_acquire(&self, count: u64) -> bool {
        self.refill();

        loop {
            let current = self.tokens.load(Ordering::SeqCst);
            if current < count {
                return false;
            }
            if self
                .tokens
                .compare_exchange(current, current - count, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
            {
                return true;
            }
        }
    }

    /// Refill tokens based on elapsed time.
    fn refill(&self) {
        let mut last = self.last_refill.lock().unwrap();
        let now = Instant::now();
        let elapsed = now.duration_since(*last).as_secs_f64();

        let new_tokens = (elapsed * self.refill_rate) as u64;
        if new_tokens > 0 {
            let current = self.tokens.load(Ordering::SeqCst);
            let new_total = (current + new_tokens).min(self.capacity);
            self.tokens.store(new_total, Ordering::SeqCst);
            *last = now;
        }
    }

    /// Get available tokens.
    pub fn available(&self) -> u64 {
        self.refill();
        self.tokens.load(Ordering::SeqCst)
    }

    /// Wait time until tokens available.
    pub fn wait_time(&self, count: u64) -> Duration {
        self.refill();
        let current = self.tokens.load(Ordering::SeqCst);
        if current >= count {
            Duration::ZERO
        } else {
            let needed = count - current;
            Duration::from_secs_f64(needed as f64 / self.refill_rate)
        }
    }
}

/// Coalescer that batches rapid updates.
pub struct Coalescer<T> {
    delay: Duration,
    pending: Arc<Mutex<Option<T>>>,
    last_update: Mutex<Instant>,
}

impl<T: Clone> Coalescer<T> {
    /// Create new coalescer.
    pub fn new(delay: Duration) -> Self {
        Self {
            delay,
            pending: Arc::new(Mutex::new(None)),
            last_update: Mutex::new(Instant::now()),
        }
    }

    /// Update with new value.
    pub fn update(&self, value: T) {
        *self.pending.lock().unwrap() = Some(value);
        *self.last_update.lock().unwrap() = Instant::now();
    }

    /// Get value if delay has passed.
    pub fn get(&self) -> Option<T> {
        let last = *self.last_update.lock().unwrap();
        if last.elapsed() >= self.delay {
            self.pending.lock().unwrap().take()
        } else {
            None
        }
    }

    /// Force get the pending value.
    pub fn force_get(&self) -> Option<T> {
        self.pending.lock().unwrap().take()
    }

    /// Check if has pending.
    pub fn has_pending(&self) -> bool {
        self.pending.lock().unwrap().is_some()
    }

    /// Get remaining time.
    pub fn remaining(&self) -> Duration {
        let elapsed = self.last_update.lock().unwrap().elapsed();
        if elapsed >= self.delay {
            Duration::ZERO
        } else {
            self.delay - elapsed
        }
    }
}

/// Async debouncer.
pub struct AsyncDebouncer {
    delay: Duration,
    notify: Arc<tokio::sync::Notify>,
}

impl AsyncDebouncer {
    /// Create new async debouncer.
    pub fn new(delay: Duration) -> Self {
        Self {
            delay,
            notify: Arc::new(tokio::sync::Notify::new()),
        }
    }

    /// Wait for debounce period.
    pub async fn debounce(&self) {
        tokio::time::sleep(self.delay).await;
    }

    /// Signal that action occurred.
    pub fn signal(&self) {
        self.notify.notify_waiters();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_debouncer() {
        let debouncer = Debouncer::new(Duration::from_millis(100));

        assert!(debouncer.should_execute());
        assert!(!debouncer.should_execute());

        std::thread::sleep(Duration::from_millis(150));
        assert!(debouncer.should_execute());
    }

    #[test]
    fn test_throttler() {
        let throttler = Throttler::new(Duration::from_millis(100));

        let r1 = throttler.try_execute(|| 1);
        assert_eq!(r1, Some(1));

        let r2 = throttler.try_execute(|| 2);
        assert_eq!(r2, None);

        std::thread::sleep(Duration::from_millis(150));
        let r3 = throttler.try_execute(|| 3);
        assert_eq!(r3, Some(3));
    }

    #[test]
    fn test_token_bucket() {
        let bucket = TokenBucket::new(10, 1.0);

        assert!(bucket.try_acquire(5));
        assert_eq!(bucket.available(), 5);

        assert!(bucket.try_acquire(5));
        assert!(!bucket.try_acquire(1));
    }

    #[test]
    fn test_coalescer() {
        let coalescer = Coalescer::new(Duration::from_millis(50));

        coalescer.update(1);
        coalescer.update(2);
        coalescer.update(3);

        // Not enough time passed
        assert!(coalescer.get().is_none());

        std::thread::sleep(Duration::from_millis(60));
        assert_eq!(coalescer.get(), Some(3));
    }
}
