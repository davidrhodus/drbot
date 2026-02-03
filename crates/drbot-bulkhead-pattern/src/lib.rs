//! Bulkhead pattern for drbot.
//!
//! This crate provides:
//! - Concurrent execution limits
//! - Resource isolation
//! - Queue-based bulkhead

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;
use thiserror::Error;

/// Bulkhead error types.
#[derive(Error, Debug, Clone)]
pub enum BulkheadError {
    #[error("Bulkhead full")]
    Full,

    #[error("Bulkhead timeout")]
    Timeout,

    #[error("Bulkhead rejected")]
    Rejected,
}

/// Result type for bulkhead operations.
pub type Result<T> = std::result::Result<T, BulkheadError>;

/// Semaphore-based bulkhead.
pub struct Bulkhead {
    max_concurrent: u32,
    current: AtomicU32,
    cond: Condvar,
    mutex: Mutex<()>,
}

impl Bulkhead {
    /// Create new bulkhead with max concurrent operations.
    pub fn new(max_concurrent: u32) -> Self {
        Self {
            max_concurrent,
            current: AtomicU32::new(0),
            cond: Condvar::new(),
            mutex: Mutex::new(()),
        }
    }

    /// Try to enter the bulkhead.
    pub fn try_enter(&self) -> Result<BulkheadPermit<'_>> {
        let current = self.current.load(Ordering::Acquire);
        if current < self.max_concurrent {
            if self
                .current
                .compare_exchange(current, current + 1, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                return Ok(BulkheadPermit { bulkhead: self });
            }
        }
        Err(BulkheadError::Full)
    }

    /// Enter the bulkhead, waiting if necessary.
    pub fn enter(&self) -> BulkheadPermit<'_> {
        loop {
            match self.try_enter() {
                Ok(permit) => return permit,
                Err(_) => {
                    let guard = self.mutex.lock().unwrap();
                    drop(self.cond.wait(guard).unwrap());
                }
            }
        }
    }

    /// Enter with timeout.
    pub fn enter_timeout(&self, timeout: Duration) -> Result<BulkheadPermit<'_>> {
        let deadline = std::time::Instant::now() + timeout;

        loop {
            match self.try_enter() {
                Ok(permit) => return Ok(permit),
                Err(_) => {
                    let remaining = deadline.saturating_duration_since(std::time::Instant::now());
                    if remaining.is_zero() {
                        return Err(BulkheadError::Timeout);
                    }

                    let guard = self.mutex.lock().unwrap();
                    drop(self.cond.wait_timeout(guard, remaining).unwrap());
                }
            }
        }
    }

    /// Get current usage.
    pub fn current(&self) -> u32 {
        self.current.load(Ordering::Acquire)
    }

    /// Get available slots.
    pub fn available(&self) -> u32 {
        self.max_concurrent.saturating_sub(self.current())
    }

    /// Execute function within bulkhead.
    pub fn execute<T, F>(&self, f: F) -> Result<T>
    where
        F: FnOnce() -> T,
    {
        let _permit = self.try_enter()?;
        Ok(f())
    }

    /// Execute with waiting.
    pub fn execute_wait<T, F>(&self, f: F) -> T
    where
        F: FnOnce() -> T,
    {
        let _permit = self.enter();
        f()
    }
}

/// Bulkhead permit.
pub struct BulkheadPermit<'a> {
    bulkhead: &'a Bulkhead,
}

impl<'a> Drop for BulkheadPermit<'a> {
    fn drop(&mut self) {
        self.bulkhead.current.fetch_sub(1, Ordering::AcqRel);
        self.bulkhead.cond.notify_one();
    }
}

/// Bulkhead with queue.
pub struct QueuedBulkhead {
    inner: Bulkhead,
    queue_size: u32,
    queued: AtomicU32,
}

impl QueuedBulkhead {
    /// Create new queued bulkhead.
    pub fn new(max_concurrent: u32, queue_size: u32) -> Self {
        Self {
            inner: Bulkhead::new(max_concurrent),
            queue_size,
            queued: AtomicU32::new(0),
        }
    }

    /// Try to enter, queueing if bulkhead is full.
    pub fn enter(&self) -> Result<BulkheadPermit<'_>> {
        // Try immediate entry
        if let Ok(permit) = self.inner.try_enter() {
            return Ok(permit);
        }

        // Try to join queue
        let queued = self.queued.fetch_add(1, Ordering::AcqRel);
        if queued >= self.queue_size {
            self.queued.fetch_sub(1, Ordering::AcqRel);
            return Err(BulkheadError::Rejected);
        }

        // Wait for entry
        let permit = self.inner.enter();
        self.queued.fetch_sub(1, Ordering::AcqRel);
        Ok(permit)
    }

    /// Get queue size.
    pub fn queue_length(&self) -> u32 {
        self.queued.load(Ordering::Acquire)
    }

    /// Get active count.
    pub fn active(&self) -> u32 {
        self.inner.current()
    }
}

/// Thread-pool based bulkhead.
pub struct ThreadPoolBulkhead {
    bulkhead: Arc<Bulkhead>,
}

impl ThreadPoolBulkhead {
    /// Create new thread-pool bulkhead.
    pub fn new(max_concurrent: u32) -> Self {
        Self {
            bulkhead: Arc::new(Bulkhead::new(max_concurrent)),
        }
    }

    /// Submit task.
    pub fn submit<F, T>(&self, f: F) -> std::thread::JoinHandle<Result<T>>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        let bulkhead = self.bulkhead.clone();
        std::thread::spawn(move || {
            let _permit = bulkhead.try_enter()?;
            Ok(f())
        })
    }
}

/// Bulkhead statistics.
#[derive(Debug, Clone)]
pub struct BulkheadStats {
    pub max_concurrent: u32,
    pub current: u32,
    pub available: u32,
}

impl Bulkhead {
    /// Get statistics.
    pub fn stats(&self) -> BulkheadStats {
        BulkheadStats {
            max_concurrent: self.max_concurrent,
            current: self.current(),
            available: self.available(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bulkhead_basic() {
        let bulkhead = Bulkhead::new(2);

        let _p1 = bulkhead.try_enter().unwrap();
        let _p2 = bulkhead.try_enter().unwrap();

        assert!(bulkhead.try_enter().is_err());
        assert_eq!(bulkhead.current(), 2);
    }

    #[test]
    fn test_permit_release() {
        let bulkhead = Bulkhead::new(1);

        {
            let _p = bulkhead.try_enter().unwrap();
            assert!(bulkhead.try_enter().is_err());
        }

        // Permit dropped, should be able to enter now
        assert!(bulkhead.try_enter().is_ok());
    }

    #[test]
    fn test_execute() {
        let bulkhead = Bulkhead::new(1);

        let result = bulkhead.execute(|| 42);
        assert_eq!(result.unwrap(), 42);
    }

    #[test]
    fn test_queued_bulkhead() {
        let bulkhead = QueuedBulkhead::new(1, 2);

        let p1 = bulkhead.enter().unwrap();
        assert_eq!(bulkhead.active(), 1);

        drop(p1);
        assert_eq!(bulkhead.active(), 0);
    }
}
