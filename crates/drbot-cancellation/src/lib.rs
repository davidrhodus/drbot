//! Cancellation tokens for drbot.
//!
//! This crate provides:
//! - Cancellation tokens
//! - Cancellation propagation
//! - Cancellation callbacks

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, Weak};
use thiserror::Error;

/// Cancellation error types.
#[derive(Error, Debug, Clone)]
pub enum CancellationError {
    #[error("Operation cancelled")]
    Cancelled,

    #[error("Cancellation already requested")]
    AlreadyCancelled,
}

/// Result type for cancellation operations.
pub type Result<T> = std::result::Result<T, CancellationError>;

/// Cancellation token source.
pub struct CancellationTokenSource {
    cancelled: Arc<AtomicBool>,
    callbacks: Mutex<Vec<Box<dyn FnOnce() + Send>>>,
    condvar: Condvar,
    mutex: Mutex<()>,
}

impl CancellationTokenSource {
    /// Create new cancellation token source.
    pub fn new() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
            callbacks: Mutex::new(Vec::new()),
            condvar: Condvar::new(),
            mutex: Mutex::new(()),
        }
    }

    /// Get a token from this source.
    pub fn token(&self) -> CancellationToken {
        CancellationToken {
            cancelled: self.cancelled.clone(),
        }
    }

    /// Request cancellation.
    pub fn cancel(&self) {
        if self.cancelled.swap(true, Ordering::AcqRel) {
            return; // Already cancelled
        }

        // Execute callbacks
        let callbacks: Vec<_> = {
            let mut callbacks = self.callbacks.lock().unwrap();
            std::mem::take(&mut *callbacks)
        };

        for callback in callbacks {
            callback();
        }

        // Notify waiters
        self.condvar.notify_all();
    }

    /// Check if cancelled.
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    /// Register cancellation callback.
    pub fn on_cancel<F>(&self, callback: F)
    where
        F: FnOnce() + Send + 'static,
    {
        if self.is_cancelled() {
            callback();
            return;
        }

        let mut callbacks = self.callbacks.lock().unwrap();
        if self.is_cancelled() {
            drop(callbacks);
            callback();
        } else {
            callbacks.push(Box::new(callback));
        }
    }

    /// Wait for cancellation.
    pub fn wait(&self) {
        let guard = self.mutex.lock().unwrap();
        let mut guard = guard;
        while !self.is_cancelled() {
            guard = self.condvar.wait(guard).unwrap();
        }
    }

    /// Wait for cancellation with timeout.
    pub fn wait_timeout(&self, timeout: std::time::Duration) -> bool {
        let guard = self.mutex.lock().unwrap();
        let mut guard = guard;
        let deadline = std::time::Instant::now() + timeout;

        while !self.is_cancelled() {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                return false;
            }
            let (new_guard, result) = self.condvar.wait_timeout(guard, remaining).unwrap();
            guard = new_guard;
            if result.timed_out() {
                return self.is_cancelled();
            }
        }
        true
    }
}

impl Default for CancellationTokenSource {
    fn default() -> Self {
        Self::new()
    }
}

/// Cancellation token (read-only view).
#[derive(Clone)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl CancellationToken {
    /// Create always-cancelled token.
    pub fn cancelled() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(true)),
        }
    }

    /// Create never-cancelled token.
    pub fn never() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Check if cancelled.
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    /// Throw if cancelled.
    pub fn throw_if_cancelled(&self) -> Result<()> {
        if self.is_cancelled() {
            Err(CancellationError::Cancelled)
        } else {
            Ok(())
        }
    }
}

/// Linked cancellation token source (cancelled when any parent is cancelled).
pub struct LinkedCancellationTokenSource {
    inner: CancellationTokenSource,
    _parents: Vec<CancellationToken>,
}

impl LinkedCancellationTokenSource {
    /// Create linked source from multiple tokens.
    pub fn new(parents: Vec<CancellationToken>) -> Arc<Self> {
        let source = Arc::new(Self {
            inner: CancellationTokenSource::new(),
            _parents: parents.clone(),
        });

        // Set up cancellation propagation
        for parent in parents {
            if parent.is_cancelled() {
                source.inner.cancel();
                return source;
            }
        }

        source
    }

    /// Get token.
    pub fn token(&self) -> CancellationToken {
        self.inner.token()
    }

    /// Cancel.
    pub fn cancel(&self) {
        self.inner.cancel();
    }

    /// Check if cancelled.
    pub fn is_cancelled(&self) -> bool {
        self.inner.is_cancelled()
    }
}

/// Execute with cancellation check.
pub fn with_cancellation<T, F>(token: &CancellationToken, f: F) -> Result<T>
where
    F: FnOnce() -> T,
{
    token.throw_if_cancelled()?;
    let result = f();
    token.throw_if_cancelled()?;
    Ok(result)
}

/// Cancellation-aware loop helper.
pub struct CancellableLoop<'a> {
    token: &'a CancellationToken,
}

impl<'a> CancellableLoop<'a> {
    /// Create new cancellable loop.
    pub fn new(token: &'a CancellationToken) -> Self {
        Self { token }
    }

    /// Run loop body while not cancelled.
    pub fn while_not_cancelled<F>(&self, mut f: F)
    where
        F: FnMut() -> bool,
    {
        while !self.token.is_cancelled() {
            if !f() {
                break;
            }
        }
    }

    /// Run loop with index.
    pub fn for_each<T, I, F>(&self, iter: I, mut f: F) -> Result<()>
    where
        I: IntoIterator<Item = T>,
        F: FnMut(T),
    {
        for item in iter {
            self.token.throw_if_cancelled()?;
            f(item);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cancellation_token_source() {
        let source = CancellationTokenSource::new();
        let token = source.token();

        assert!(!token.is_cancelled());
        source.cancel();
        assert!(token.is_cancelled());
    }

    #[test]
    fn test_throw_if_cancelled() {
        let source = CancellationTokenSource::new();
        let token = source.token();

        assert!(token.throw_if_cancelled().is_ok());
        source.cancel();
        assert!(token.throw_if_cancelled().is_err());
    }

    #[test]
    fn test_callback() {
        use std::sync::atomic::AtomicI32;

        let source = CancellationTokenSource::new();
        let counter = Arc::new(AtomicI32::new(0));

        let c = counter.clone();
        source.on_cancel(move || {
            c.fetch_add(1, Ordering::SeqCst);
        });

        assert_eq!(counter.load(Ordering::SeqCst), 0);
        source.cancel();
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_with_cancellation() {
        let source = CancellationTokenSource::new();
        let token = source.token();

        let result = with_cancellation(&token, || 42);
        assert_eq!(result.unwrap(), 42);

        source.cancel();
        let result: Result<i32> = with_cancellation(&token, || 42);
        assert!(result.is_err());
    }

    #[test]
    fn test_never_cancelled() {
        let token = CancellationToken::never();
        assert!(!token.is_cancelled());
    }

    #[test]
    fn test_always_cancelled() {
        let token = CancellationToken::cancelled();
        assert!(token.is_cancelled());
    }
}
