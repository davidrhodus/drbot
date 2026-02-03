//! Async semaphore for drbot.
//!
//! This crate provides:
//! - Counting semaphore
//! - RAII-style permits
//! - Timed acquisition
//! - Fair scheduling

use std::sync::Arc;
use thiserror::Error;
use tokio::sync::{OwnedSemaphorePermit, Semaphore, TryAcquireError};

/// Semaphore error types.
#[derive(Error, Debug)]
pub enum SemaphoreError {
    #[error("Semaphore closed")]
    Closed,

    #[error("No permits available")]
    NoPermits,

    #[error("Acquisition timeout")]
    Timeout,
}

/// Result type for semaphore operations.
pub type Result<T> = std::result::Result<T, SemaphoreError>;

/// Async counting semaphore.
pub struct AsyncSemaphore {
    inner: Arc<Semaphore>,
    max_permits: usize,
}

impl AsyncSemaphore {
    /// Create new semaphore with given permits.
    pub fn new(permits: usize) -> Self {
        Self {
            inner: Arc::new(Semaphore::new(permits)),
            max_permits: permits,
        }
    }

    /// Acquire a permit.
    pub async fn acquire(&self) -> Result<Permit<'_>> {
        let permit = self
            .inner
            .acquire()
            .await
            .map_err(|_| SemaphoreError::Closed)?;
        Ok(Permit { _inner: permit })
    }

    /// Acquire multiple permits.
    pub async fn acquire_many(&self, n: u32) -> Result<PermitMany<'_>> {
        let permit = self
            .inner
            .acquire_many(n)
            .await
            .map_err(|_| SemaphoreError::Closed)?;
        Ok(PermitMany { _inner: permit })
    }

    /// Try to acquire without blocking.
    pub fn try_acquire(&self) -> Result<Permit<'_>> {
        let permit = self.inner.try_acquire().map_err(|e| match e {
            TryAcquireError::NoPermits => SemaphoreError::NoPermits,
            TryAcquireError::Closed => SemaphoreError::Closed,
        })?;
        Ok(Permit { _inner: permit })
    }

    /// Acquire with timeout.
    pub async fn acquire_timeout(&self, timeout: std::time::Duration) -> Result<Permit<'_>> {
        tokio::time::timeout(timeout, self.acquire())
            .await
            .map_err(|_| SemaphoreError::Timeout)?
    }

    /// Get available permits.
    pub fn available_permits(&self) -> usize {
        self.inner.available_permits()
    }

    /// Get max permits.
    pub fn max_permits(&self) -> usize {
        self.max_permits
    }

    /// Add permits.
    pub fn add_permits(&self, n: usize) {
        self.inner.add_permits(n);
    }

    /// Close the semaphore.
    pub fn close(&self) {
        self.inner.close();
    }

    /// Check if closed.
    pub fn is_closed(&self) -> bool {
        self.inner.is_closed()
    }
}

impl Clone for AsyncSemaphore {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
            max_permits: self.max_permits,
        }
    }
}

/// RAII permit guard.
pub struct Permit<'a> {
    _inner: tokio::sync::SemaphorePermit<'a>,
}

/// RAII permit guard for multiple permits.
pub struct PermitMany<'a> {
    _inner: tokio::sync::SemaphorePermit<'a>,
}

/// Owned semaphore for sharing across tasks.
pub struct OwnedSemaphore {
    inner: Arc<Semaphore>,
}

impl OwnedSemaphore {
    /// Create new owned semaphore.
    pub fn new(permits: usize) -> Self {
        Self {
            inner: Arc::new(Semaphore::new(permits)),
        }
    }

    /// Acquire an owned permit.
    pub async fn acquire_owned(self: Arc<Self>) -> Result<OwnedPermit> {
        let permit = Arc::clone(&self.inner)
            .acquire_owned()
            .await
            .map_err(|_| SemaphoreError::Closed)?;
        Ok(OwnedPermit { _inner: permit })
    }

    /// Try to acquire owned permit.
    pub fn try_acquire_owned(self: Arc<Self>) -> Result<OwnedPermit> {
        let permit = Arc::clone(&self.inner)
            .try_acquire_owned()
            .map_err(|e| match e {
                TryAcquireError::NoPermits => SemaphoreError::NoPermits,
                TryAcquireError::Closed => SemaphoreError::Closed,
            })?;
        Ok(OwnedPermit { _inner: permit })
    }

    /// Get available permits.
    pub fn available_permits(&self) -> usize {
        self.inner.available_permits()
    }
}

/// Owned permit that can be sent across task boundaries.
pub struct OwnedPermit {
    _inner: OwnedSemaphorePermit,
}

/// Resource pool using semaphore.
pub struct ResourcePool<T: Send + 'static> {
    resources: Arc<tokio::sync::Mutex<Vec<T>>>,
    semaphore: Arc<Semaphore>,
}

impl<T: Send + 'static> ResourcePool<T> {
    /// Create new resource pool.
    pub fn new(resources: Vec<T>) -> Self {
        let count = resources.len();
        Self {
            resources: Arc::new(tokio::sync::Mutex::new(resources)),
            semaphore: Arc::new(Semaphore::new(count)),
        }
    }

    /// Acquire a resource.
    pub async fn acquire(&self) -> Option<PooledResource<T>> {
        let permit = self.semaphore.acquire().await.ok()?;
        let resource = self.resources.lock().await.pop()?;
        permit.forget(); // We'll handle cleanup in drop

        Some(PooledResource {
            resource: Some(resource),
            pool: Arc::clone(&self.resources),
            semaphore: Arc::clone(&self.semaphore),
        })
    }

    /// Get available count.
    pub fn available(&self) -> usize {
        self.semaphore.available_permits()
    }
}

impl<T: Send + 'static> Clone for ResourcePool<T> {
    fn clone(&self) -> Self {
        Self {
            resources: Arc::clone(&self.resources),
            semaphore: Arc::clone(&self.semaphore),
        }
    }
}

/// Pooled resource guard.
pub struct PooledResource<T: Send + 'static> {
    resource: Option<T>,
    pool: Arc<tokio::sync::Mutex<Vec<T>>>,
    semaphore: Arc<Semaphore>,
}

impl<T: Send + 'static> std::ops::Deref for PooledResource<T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.resource.as_ref().unwrap()
    }
}

impl<T: Send + 'static> std::ops::DerefMut for PooledResource<T> {
    fn deref_mut(&mut self) -> &mut T {
        self.resource.as_mut().unwrap()
    }
}

impl<T: Send + 'static> Drop for PooledResource<T> {
    fn drop(&mut self) {
        if let Some(resource) = self.resource.take() {
            let pool = Arc::clone(&self.pool);
            let semaphore = Arc::clone(&self.semaphore);

            // Return resource to pool
            tokio::spawn(async move {
                pool.lock().await.push(resource);
                semaphore.add_permits(1);
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_basic_acquire() {
        let sem = AsyncSemaphore::new(2);

        let p1 = sem.acquire().await.unwrap();
        let p2 = sem.acquire().await.unwrap();

        assert_eq!(sem.available_permits(), 0);

        drop(p1);
        assert_eq!(sem.available_permits(), 1);

        drop(p2);
        assert_eq!(sem.available_permits(), 2);
    }

    #[tokio::test]
    async fn test_try_acquire() {
        let sem = AsyncSemaphore::new(1);

        let p1 = sem.try_acquire().unwrap();
        assert!(sem.try_acquire().is_err());

        drop(p1);
        assert!(sem.try_acquire().is_ok());
    }

    #[tokio::test]
    async fn test_acquire_many() {
        let sem = AsyncSemaphore::new(5);

        let p = sem.acquire_many(3).await.unwrap();
        assert_eq!(sem.available_permits(), 2);

        drop(p);
        assert_eq!(sem.available_permits(), 5);
    }

    #[tokio::test]
    async fn test_resource_pool() {
        let pool = ResourcePool::new(vec!["a", "b", "c"]);

        let r1 = pool.acquire().await.unwrap();
        let r2 = pool.acquire().await.unwrap();

        assert_eq!(pool.available(), 1);

        drop(r1);
        drop(r2);

        // Give time for async cleanup
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        assert_eq!(pool.available(), 3);
    }
}
