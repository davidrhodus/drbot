//! Resource management utilities for drbot.
//!
//! This crate provides:
//! - Resource handles
//! - Resource pools
//! - Reference counting
//! - RAII patterns

use std::ops::{Deref, DerefMut};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, Weak};
use thiserror::Error;

/// Resource error types.
#[derive(Error, Debug)]
pub enum ResourceError {
    #[error("Resource not available")]
    NotAvailable,

    #[error("Resource exhausted")]
    Exhausted,

    #[error("Resource already acquired")]
    AlreadyAcquired,

    #[error("Resource released")]
    Released,

    #[error("Pool closed")]
    PoolClosed,
}

/// Result type for resource operations.
pub type Result<T> = std::result::Result<T, ResourceError>;

/// Resource ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ResourceId(u64);

impl ResourceId {
    /// Generate new resource ID.
    pub fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        Self(COUNTER.fetch_add(1, Ordering::SeqCst))
    }
}

impl Default for ResourceId {
    fn default() -> Self {
        Self::new()
    }
}

/// Managed resource handle.
pub struct Resource<T> {
    id: ResourceId,
    data: T,
    ref_count: Arc<AtomicUsize>,
}

impl<T> Resource<T> {
    /// Create new resource.
    pub fn new(data: T) -> Self {
        Self {
            id: ResourceId::new(),
            data,
            ref_count: Arc::new(AtomicUsize::new(1)),
        }
    }

    /// Get resource ID.
    pub fn id(&self) -> ResourceId {
        self.id
    }

    /// Get reference count.
    pub fn ref_count(&self) -> usize {
        self.ref_count.load(Ordering::SeqCst)
    }

    /// Create a handle to this resource.
    pub fn handle(&self) -> ResourceHandle<T>
    where
        T: Clone,
    {
        self.ref_count.fetch_add(1, Ordering::SeqCst);
        ResourceHandle {
            id: self.id,
            data: self.data.clone(),
            ref_count: self.ref_count.clone(),
        }
    }

    /// Consume and get inner data.
    pub fn into_inner(self) -> T {
        self.data
    }
}

impl<T> Deref for Resource<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

impl<T> DerefMut for Resource<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.data
    }
}

/// Resource handle (reference-counted).
pub struct ResourceHandle<T> {
    id: ResourceId,
    data: T,
    ref_count: Arc<AtomicUsize>,
}

impl<T> ResourceHandle<T> {
    /// Get resource ID.
    pub fn id(&self) -> ResourceId {
        self.id
    }

    /// Get reference count.
    pub fn ref_count(&self) -> usize {
        self.ref_count.load(Ordering::SeqCst)
    }
}

impl<T> Deref for ResourceHandle<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

impl<T> Drop for ResourceHandle<T> {
    fn drop(&mut self) {
        self.ref_count.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Shared resource with Arc semantics.
pub struct SharedResource<T> {
    inner: Arc<Mutex<T>>,
    id: ResourceId,
}

impl<T> SharedResource<T> {
    /// Create new shared resource.
    pub fn new(data: T) -> Self {
        Self {
            inner: Arc::new(Mutex::new(data)),
            id: ResourceId::new(),
        }
    }

    /// Get resource ID.
    pub fn id(&self) -> ResourceId {
        self.id
    }

    /// Access resource.
    pub fn access(&self) -> std::sync::MutexGuard<'_, T> {
        self.inner.lock().unwrap()
    }

    /// Try to access resource.
    pub fn try_access(&self) -> Option<std::sync::MutexGuard<'_, T>> {
        self.inner.try_lock().ok()
    }

    /// Get reference count.
    pub fn ref_count(&self) -> usize {
        Arc::strong_count(&self.inner)
    }

    /// Create weak reference.
    pub fn downgrade(&self) -> WeakResource<T> {
        WeakResource {
            inner: Arc::downgrade(&self.inner),
            id: self.id,
        }
    }
}

impl<T> Clone for SharedResource<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            id: self.id,
        }
    }
}

/// Weak resource reference.
pub struct WeakResource<T> {
    inner: Weak<Mutex<T>>,
    id: ResourceId,
}

impl<T> WeakResource<T> {
    /// Try to upgrade to shared resource.
    pub fn upgrade(&self) -> Option<SharedResource<T>> {
        self.inner
            .upgrade()
            .map(|inner| SharedResource { inner, id: self.id })
    }

    /// Check if resource is still alive.
    pub fn is_alive(&self) -> bool {
        self.inner.strong_count() > 0
    }

    /// Get resource ID.
    pub fn id(&self) -> ResourceId {
        self.id
    }
}

impl<T> Clone for WeakResource<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            id: self.id,
        }
    }
}

/// Resource pool.
pub struct ResourcePool<T> {
    resources: Mutex<Vec<T>>,
    max_size: usize,
    created: AtomicUsize,
}

impl<T> ResourcePool<T> {
    /// Create new pool.
    pub fn new(max_size: usize) -> Self {
        Self {
            resources: Mutex::new(Vec::with_capacity(max_size)),
            max_size,
            created: AtomicUsize::new(0),
        }
    }

    /// Get pool size.
    pub fn size(&self) -> usize {
        self.resources.lock().unwrap().len()
    }

    /// Get max size.
    pub fn max_size(&self) -> usize {
        self.max_size
    }

    /// Check if pool is empty.
    pub fn is_empty(&self) -> bool {
        self.resources.lock().unwrap().is_empty()
    }

    /// Get total created resources.
    pub fn total_created(&self) -> usize {
        self.created.load(Ordering::SeqCst)
    }

    /// Acquire resource from pool.
    pub fn acquire(&self) -> Option<T> {
        self.resources.lock().unwrap().pop()
    }

    /// Release resource back to pool.
    pub fn release(&self, resource: T) -> bool {
        let mut resources = self.resources.lock().unwrap();
        if resources.len() < self.max_size {
            resources.push(resource);
            true
        } else {
            false
        }
    }

    /// Add new resource to pool.
    pub fn add(&self, resource: T) -> bool {
        let mut resources = self.resources.lock().unwrap();
        if resources.len() < self.max_size {
            resources.push(resource);
            self.created.fetch_add(1, Ordering::SeqCst);
            true
        } else {
            false
        }
    }

    /// Clear all resources.
    pub fn clear(&self) {
        self.resources.lock().unwrap().clear();
    }
}

/// Pooled resource guard.
pub struct PooledResource<'a, T> {
    resource: Option<T>,
    pool: &'a ResourcePool<T>,
}

impl<'a, T> PooledResource<'a, T> {
    /// Create new pooled resource.
    pub fn new(resource: T, pool: &'a ResourcePool<T>) -> Self {
        Self {
            resource: Some(resource),
            pool,
        }
    }

    /// Take resource without returning to pool.
    pub fn take(mut self) -> T {
        self.resource.take().unwrap()
    }
}

impl<T> Deref for PooledResource<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.resource.as_ref().unwrap()
    }
}

impl<T> DerefMut for PooledResource<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.resource.as_mut().unwrap()
    }
}

impl<T> Drop for PooledResource<'_, T> {
    fn drop(&mut self) {
        if let Some(resource) = self.resource.take() {
            self.pool.release(resource);
        }
    }
}

/// Resource quota tracker.
#[derive(Debug)]
pub struct Quota {
    limit: usize,
    used: AtomicUsize,
}

impl Quota {
    /// Create new quota.
    pub fn new(limit: usize) -> Self {
        Self {
            limit,
            used: AtomicUsize::new(0),
        }
    }

    /// Get limit.
    pub fn limit(&self) -> usize {
        self.limit
    }

    /// Get used amount.
    pub fn used(&self) -> usize {
        self.used.load(Ordering::SeqCst)
    }

    /// Get remaining.
    pub fn remaining(&self) -> usize {
        self.limit.saturating_sub(self.used())
    }

    /// Check if exceeded.
    pub fn is_exceeded(&self) -> bool {
        self.used() >= self.limit
    }

    /// Try to acquire.
    pub fn acquire(&self, amount: usize) -> Result<()> {
        loop {
            let current = self.used.load(Ordering::SeqCst);
            if current + amount > self.limit {
                return Err(ResourceError::Exhausted);
            }
            if self
                .used
                .compare_exchange(
                    current,
                    current + amount,
                    Ordering::SeqCst,
                    Ordering::SeqCst,
                )
                .is_ok()
            {
                return Ok(());
            }
        }
    }

    /// Release.
    pub fn release(&self, amount: usize) {
        self.used.fetch_sub(amount, Ordering::SeqCst);
    }

    /// Reset quota.
    pub fn reset(&self) {
        self.used.store(0, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resource() {
        let resource = Resource::new(42);
        assert_eq!(*resource, 42);
        assert_eq!(resource.ref_count(), 1);
    }

    #[test]
    fn test_resource_handle() {
        let resource = Resource::new(42);
        let handle1 = resource.handle();
        let handle2 = resource.handle();

        assert_eq!(*handle1, 42);
        assert_eq!(*handle2, 42);
        assert_eq!(resource.ref_count(), 3);

        drop(handle1);
        assert_eq!(resource.ref_count(), 2);
    }

    #[test]
    fn test_shared_resource() {
        let shared = SharedResource::new(42);
        let clone = shared.clone();

        assert_eq!(*shared.access(), 42);
        assert_eq!(shared.ref_count(), 2);

        *clone.access() = 100;
        assert_eq!(*shared.access(), 100);
    }

    #[test]
    fn test_weak_resource() {
        let shared = SharedResource::new(42);
        let weak = shared.downgrade();

        assert!(weak.is_alive());
        assert!(weak.upgrade().is_some());

        drop(shared);
        assert!(!weak.is_alive());
        assert!(weak.upgrade().is_none());
    }

    #[test]
    fn test_resource_pool() {
        let pool = ResourcePool::new(3);
        pool.add(1);
        pool.add(2);

        assert_eq!(pool.size(), 2);

        let r1 = pool.acquire().unwrap();
        assert_eq!(r1, 2);
        assert_eq!(pool.size(), 1);

        pool.release(r1);
        assert_eq!(pool.size(), 2);
    }

    #[test]
    fn test_quota() {
        let quota = Quota::new(100);

        assert!(quota.acquire(50).is_ok());
        assert_eq!(quota.used(), 50);
        assert_eq!(quota.remaining(), 50);

        assert!(quota.acquire(60).is_err());
        assert!(quota.acquire(50).is_ok());

        quota.release(100);
        assert_eq!(quota.used(), 0);
    }
}
