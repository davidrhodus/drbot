//! Async mutex for drbot.
//!
//! This crate provides:
//! - Async mutex with fair scheduling
//! - Try-lock functionality
//! - Timed locks
//! - RAII-style guards

use std::cell::UnsafeCell;
use std::ops::{Deref, DerefMut};
use std::sync::atomic::{AtomicBool, Ordering};
use thiserror::Error;
use tokio::sync::Notify;

/// Mutex error types.
#[derive(Error, Debug)]
pub enum MutexError {
    #[error("Lock timeout")]
    Timeout,

    #[error("Lock poisoned")]
    Poisoned,

    #[error("Would block")]
    WouldBlock,
}

/// Result type for mutex operations.
pub type Result<T> = std::result::Result<T, MutexError>;

/// Async mutex.
pub struct AsyncMutex<T> {
    locked: AtomicBool,
    notify: Notify,
    data: UnsafeCell<T>,
}

// Safety: We only access data through the lock guard
unsafe impl<T: Send> Send for AsyncMutex<T> {}
unsafe impl<T: Send> Sync for AsyncMutex<T> {}

impl<T> AsyncMutex<T> {
    /// Create new mutex.
    pub fn new(data: T) -> Self {
        Self {
            locked: AtomicBool::new(false),
            notify: Notify::new(),
            data: UnsafeCell::new(data),
        }
    }

    /// Acquire lock asynchronously.
    pub async fn lock(&self) -> MutexGuard<'_, T> {
        loop {
            if self.try_acquire() {
                return MutexGuard { mutex: self };
            }
            self.notify.notified().await;
        }
    }

    /// Try to acquire lock without blocking.
    pub fn try_lock(&self) -> Option<MutexGuard<'_, T>> {
        if self.try_acquire() {
            Some(MutexGuard { mutex: self })
        } else {
            None
        }
    }

    /// Try to acquire lock with timeout.
    pub async fn lock_timeout(&self, timeout: std::time::Duration) -> Result<MutexGuard<'_, T>> {
        let deadline = tokio::time::Instant::now() + timeout;

        loop {
            if self.try_acquire() {
                return Ok(MutexGuard { mutex: self });
            }

            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                return Err(MutexError::Timeout);
            }

            tokio::select! {
                _ = self.notify.notified() => {},
                _ = tokio::time::sleep(remaining) => {
                    return Err(MutexError::Timeout);
                }
            }
        }
    }

    /// Check if locked.
    pub fn is_locked(&self) -> bool {
        self.locked.load(Ordering::Acquire)
    }

    /// Get inner data (requires mutable access to self).
    pub fn get_mut(&mut self) -> &mut T {
        self.data.get_mut()
    }

    /// Consume mutex and return inner data.
    pub fn into_inner(self) -> T {
        self.data.into_inner()
    }

    fn try_acquire(&self) -> bool {
        self.locked
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
    }

    fn release(&self) {
        self.locked.store(false, Ordering::Release);
        self.notify.notify_one();
    }
}

/// RAII lock guard.
pub struct MutexGuard<'a, T> {
    mutex: &'a AsyncMutex<T>,
}

impl<T> Deref for MutexGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        // Safety: We hold the lock
        unsafe { &*self.mutex.data.get() }
    }
}

impl<T> DerefMut for MutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        // Safety: We hold the lock
        unsafe { &mut *self.mutex.data.get() }
    }
}

impl<T> Drop for MutexGuard<'_, T> {
    fn drop(&mut self) {
        self.mutex.release();
    }
}

/// Fair async mutex with queue ordering.
pub struct FairMutex<T> {
    inner: tokio::sync::Mutex<T>,
}

impl<T> FairMutex<T> {
    /// Create new fair mutex.
    pub fn new(data: T) -> Self {
        Self {
            inner: tokio::sync::Mutex::new(data),
        }
    }

    /// Acquire lock.
    pub async fn lock(&self) -> tokio::sync::MutexGuard<'_, T> {
        self.inner.lock().await
    }

    /// Try to acquire lock.
    pub fn try_lock(&self) -> Option<tokio::sync::MutexGuard<'_, T>> {
        self.inner.try_lock().ok()
    }
}

/// Reentrant async mutex (allows same task to acquire multiple times).
pub struct ReentrantMutex<T> {
    inner: tokio::sync::Mutex<(T, Option<tokio::task::Id>, usize)>,
}

impl<T> ReentrantMutex<T> {
    /// Create new reentrant mutex.
    pub fn new(data: T) -> Self {
        Self {
            inner: tokio::sync::Mutex::new((data, None, 0)),
        }
    }

    /// Acquire lock.
    pub async fn lock(&self) -> ReentrantGuard<'_, T> {
        let current_task = tokio::task::try_id();

        loop {
            let mut guard = self.inner.lock().await;

            match (guard.1, current_task) {
                // Not locked
                (None, Some(id)) => {
                    guard.1 = Some(id);
                    guard.2 = 1;
                    return ReentrantGuard {
                        mutex: self,
                        _guard: Some(guard),
                    };
                }
                // Locked by same task
                (Some(owner), Some(id)) if owner == id => {
                    guard.2 += 1;
                    return ReentrantGuard {
                        mutex: self,
                        _guard: Some(guard),
                    };
                }
                // Locked by different task, wait
                _ => {
                    drop(guard);
                    tokio::task::yield_now().await;
                }
            }
        }
    }
}

/// Reentrant mutex guard.
pub struct ReentrantGuard<'a, T> {
    #[allow(dead_code)]
    mutex: &'a ReentrantMutex<T>,
    _guard: Option<tokio::sync::MutexGuard<'a, (T, Option<tokio::task::Id>, usize)>>,
}

impl<T> Deref for ReentrantGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self._guard.as_ref().unwrap().0
    }
}

impl<T> DerefMut for ReentrantGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self._guard.as_mut().unwrap().0
    }
}

impl<T> Drop for ReentrantGuard<'_, T> {
    fn drop(&mut self) {
        if let Some(guard) = &mut self._guard {
            guard.2 -= 1;
            if guard.2 == 0 {
                guard.1 = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_basic_lock() {
        let mutex = AsyncMutex::new(42);

        {
            let mut guard = mutex.lock().await;
            *guard = 100;
        }

        let guard = mutex.lock().await;
        assert_eq!(*guard, 100);
    }

    #[tokio::test]
    async fn test_try_lock() {
        let mutex = AsyncMutex::new(42);

        let guard = mutex.try_lock();
        assert!(guard.is_some());

        let guard2 = mutex.try_lock();
        assert!(guard2.is_none());

        drop(guard);

        let guard3 = mutex.try_lock();
        assert!(guard3.is_some());
    }

    #[tokio::test]
    async fn test_concurrent_access() {
        let mutex = Arc::new(AsyncMutex::new(0));
        let mut handles = vec![];

        for _ in 0..10 {
            let mutex = Arc::clone(&mutex);
            handles.push(tokio::spawn(async move {
                for _ in 0..100 {
                    let mut guard = mutex.lock().await;
                    *guard += 1;
                }
            }));
        }

        for handle in handles {
            handle.await.unwrap();
        }

        let guard = mutex.lock().await;
        assert_eq!(*guard, 1000);
    }

    #[tokio::test]
    async fn test_timeout() {
        let mutex = AsyncMutex::new(42);

        let _guard = mutex.lock().await;

        let result = tokio::time::timeout(
            std::time::Duration::from_millis(50),
            mutex.lock_timeout(std::time::Duration::from_millis(10)),
        )
        .await;

        assert!(result.is_ok());
        assert!(result.unwrap().is_err());
    }
}
