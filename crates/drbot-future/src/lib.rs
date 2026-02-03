//! Future/Promise utilities for drbot (sync version).
//!
//! This crate provides:
//! - Simple future/promise for synchronous code
//! - Blocking resolution
//! - Chaining

use std::sync::{Arc, Condvar, Mutex};
use thiserror::Error;

/// Future error types.
#[derive(Error, Debug, Clone)]
pub enum FutureError {
    #[error("Future cancelled")]
    Cancelled,

    #[error("Future timed out")]
    Timeout,

    #[error("Future failed: {0}")]
    Failed(String),
}

/// Result type for future operations.
pub type Result<T> = std::result::Result<T, FutureError>;

/// State of a future.
#[derive(Clone)]
enum FutureState<T: Clone> {
    Pending,
    Ready(T),
    Failed(FutureError),
}

/// Simple synchronous future.
pub struct Future<T: Clone> {
    state: Arc<(Mutex<FutureState<T>>, Condvar)>,
}

impl<T: Clone> Future<T> {
    /// Create new pending future.
    pub fn new() -> Self {
        Self {
            state: Arc::new((Mutex::new(FutureState::Pending), Condvar::new())),
        }
    }

    /// Create ready future with value.
    pub fn ready(value: T) -> Self {
        Self {
            state: Arc::new((Mutex::new(FutureState::Ready(value)), Condvar::new())),
        }
    }

    /// Create failed future.
    pub fn failed(error: FutureError) -> Self {
        Self {
            state: Arc::new((Mutex::new(FutureState::Failed(error)), Condvar::new())),
        }
    }

    /// Check if complete.
    pub fn is_complete(&self) -> bool {
        let state = self.state.0.lock().unwrap();
        !matches!(*state, FutureState::Pending)
    }

    /// Check if ready.
    pub fn is_ready(&self) -> bool {
        let state = self.state.0.lock().unwrap();
        matches!(*state, FutureState::Ready(_))
    }

    /// Check if failed.
    pub fn is_failed(&self) -> bool {
        let state = self.state.0.lock().unwrap();
        matches!(*state, FutureState::Failed(_))
    }

    /// Wait for result (blocking).
    pub fn get(&self) -> Result<T> {
        let mut state = self.state.0.lock().unwrap();
        while matches!(*state, FutureState::Pending) {
            state = self.state.1.wait(state).unwrap();
        }
        match &*state {
            FutureState::Ready(v) => Ok(v.clone()),
            FutureState::Failed(e) => Err(e.clone()),
            FutureState::Pending => unreachable!(),
        }
    }

    /// Wait with timeout.
    pub fn get_timeout(&self, timeout: std::time::Duration) -> Result<T> {
        let mut state = self.state.0.lock().unwrap();
        let deadline = std::time::Instant::now() + timeout;

        while matches!(*state, FutureState::Pending) {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                return Err(FutureError::Timeout);
            }
            let result = self.state.1.wait_timeout(state, remaining).unwrap();
            state = result.0;
        }

        match &*state {
            FutureState::Ready(v) => Ok(v.clone()),
            FutureState::Failed(e) => Err(e.clone()),
            FutureState::Pending => unreachable!(),
        }
    }

    /// Try to get result (non-blocking).
    pub fn try_get(&self) -> Option<Result<T>> {
        let state = self.state.0.lock().unwrap();
        match &*state {
            FutureState::Ready(v) => Some(Ok(v.clone())),
            FutureState::Failed(e) => Some(Err(e.clone())),
            FutureState::Pending => None,
        }
    }
}

impl<T: Clone> Default for Future<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Clone> Clone for Future<T> {
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
        }
    }
}

/// Promise to complete a future.
pub struct Promise<T: Clone> {
    state: Arc<(Mutex<FutureState<T>>, Condvar)>,
}

impl<T: Clone> Promise<T> {
    /// Create new promise and its future.
    pub fn new() -> (Self, Future<T>) {
        let state = Arc::new((Mutex::new(FutureState::Pending), Condvar::new()));
        let future = Future {
            state: state.clone(),
        };
        (Self { state }, future)
    }

    /// Complete with value.
    pub fn complete(self, value: T) -> bool {
        let mut state = self.state.0.lock().unwrap();
        if matches!(*state, FutureState::Pending) {
            *state = FutureState::Ready(value);
            self.state.1.notify_all();
            true
        } else {
            false
        }
    }

    /// Complete with error.
    pub fn fail(self, error: FutureError) -> bool {
        let mut state = self.state.0.lock().unwrap();
        if matches!(*state, FutureState::Pending) {
            *state = FutureState::Failed(error);
            self.state.1.notify_all();
            true
        } else {
            false
        }
    }
}

impl<T: Clone> Default for Promise<T> {
    fn default() -> Self {
        Self::new().0
    }
}

/// Complete a future from a function in a thread.
pub fn spawn_future<T: Clone + Send + 'static, F: FnOnce() -> T + Send + 'static>(
    f: F,
) -> Future<T> {
    let (promise, future) = Promise::new();
    std::thread::spawn(move || {
        let result = f();
        promise.complete(result);
    });
    future
}

/// Complete a future from a fallible function.
pub fn spawn_future_result<T: Clone + Send + 'static, F>(f: F) -> Future<T>
where
    F: FnOnce() -> std::result::Result<T, String> + Send + 'static,
{
    let (promise, future) = Promise::new();
    std::thread::spawn(move || match f() {
        Ok(value) => {
            promise.complete(value);
        }
        Err(e) => {
            promise.fail(FutureError::Failed(e));
        }
    });
    future
}

/// Wait for all futures.
pub fn all<T: Clone>(futures: Vec<Future<T>>) -> Vec<Result<T>> {
    futures.into_iter().map(|f| f.get()).collect()
}

/// Wait for first completed future.
pub fn race<T: Clone + Send + 'static>(futures: Vec<Future<T>>) -> Result<T> {
    // Simple implementation: poll in loop
    loop {
        for f in &futures {
            if let Some(result) = f.try_get() {
                return result;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_future_ready() {
        let future = Future::ready(42);
        assert!(future.is_complete());
        assert!(future.is_ready());
        assert_eq!(future.get().unwrap(), 42);
    }

    #[test]
    fn test_promise() {
        let (promise, future) = Promise::new();

        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(10));
            promise.complete(42);
        });

        assert!(!future.is_complete());
        assert_eq!(future.get().unwrap(), 42);
    }

    #[test]
    fn test_spawn_future() {
        let future = spawn_future(|| {
            std::thread::sleep(std::time::Duration::from_millis(10));
            42
        });

        assert_eq!(future.get().unwrap(), 42);
    }

    #[test]
    fn test_all() {
        let futures = vec![Future::ready(1), Future::ready(2), Future::ready(3)];

        let results: Vec<_> = all(futures).into_iter().map(|r| r.unwrap()).collect();
        assert_eq!(results, vec![1, 2, 3]);
    }
}
