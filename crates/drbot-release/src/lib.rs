//! Resource release utilities for drbot.
//!
//! This crate provides:
//! - Resource release patterns
//! - Release guards
//! - Automatic release

use thiserror::Error;

/// Release error types.
#[derive(Error, Debug, Clone)]
pub enum ReleaseError {
    #[error("Release failed: {0}")]
    Failed(String),

    #[error("Already released")]
    AlreadyReleased,

    #[error("Not acquired")]
    NotAcquired,
}

/// Result type for release operations.
pub type Result<T> = std::result::Result<T, ReleaseError>;

/// Releasable trait.
pub trait Releasable {
    /// Release the resource.
    fn release(&mut self) -> Result<()>;

    /// Check if released.
    fn is_released(&self) -> bool;
}

/// Release guard.
pub struct ReleaseGuard<T: Releasable> {
    resource: Option<T>,
}

impl<T: Releasable> ReleaseGuard<T> {
    /// Create new guard.
    pub fn new(resource: T) -> Self {
        Self {
            resource: Some(resource),
        }
    }

    /// Get resource reference.
    pub fn get(&self) -> Option<&T> {
        self.resource.as_ref()
    }

    /// Get mutable reference.
    pub fn get_mut(&mut self) -> Option<&mut T> {
        self.resource.as_mut()
    }

    /// Release manually.
    pub fn release(&mut self) -> Result<()> {
        if let Some(mut r) = self.resource.take() {
            r.release()
        } else {
            Err(ReleaseError::AlreadyReleased)
        }
    }

    /// Take without releasing.
    pub fn take(mut self) -> Option<T> {
        self.resource.take()
    }
}

impl<T: Releasable> Drop for ReleaseGuard<T> {
    fn drop(&mut self) {
        if let Some(mut r) = self.resource.take() {
            let _ = r.release();
        }
    }
}

/// Simple releasable resource.
pub struct Resource<T, F: FnOnce(&mut T)> {
    value: Option<T>,
    releaser: Option<F>,
}

impl<T, F: FnOnce(&mut T)> Resource<T, F> {
    /// Create new resource.
    pub fn new(value: T, releaser: F) -> Self {
        Self {
            value: Some(value),
            releaser: Some(releaser),
        }
    }

    /// Get reference.
    pub fn get(&self) -> Option<&T> {
        self.value.as_ref()
    }

    /// Get mutable reference.
    pub fn get_mut(&mut self) -> Option<&mut T> {
        self.value.as_mut()
    }

    /// Release resource.
    pub fn release(&mut self) {
        if let (Some(mut v), Some(r)) = (self.value.take(), self.releaser.take()) {
            r(&mut v);
        }
    }

    /// Is released.
    pub fn is_released(&self) -> bool {
        self.value.is_none()
    }

    /// Take value without release.
    pub fn into_inner(mut self) -> Option<T> {
        self.releaser = None;
        self.value.take()
    }
}

impl<T, F: FnOnce(&mut T)> Drop for Resource<T, F> {
    fn drop(&mut self) {
        self.release();
    }
}

/// Counted resource release.
pub struct CountedRelease {
    acquired: std::sync::atomic::AtomicUsize,
    released: std::sync::atomic::AtomicUsize,
}

impl CountedRelease {
    /// Create new.
    pub const fn new() -> Self {
        Self {
            acquired: std::sync::atomic::AtomicUsize::new(0),
            released: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// Mark as acquired.
    pub fn acquire(&self) {
        self.acquired
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// Mark as released.
    pub fn release(&self) {
        self.released
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// Get acquired count.
    pub fn acquired(&self) -> usize {
        self.acquired.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Get released count.
    pub fn released(&self) -> usize {
        self.released.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Get outstanding count.
    pub fn outstanding(&self) -> usize {
        self.acquired().saturating_sub(self.released())
    }

    /// All released.
    pub fn all_released(&self) -> bool {
        self.outstanding() == 0
    }
}

impl Default for CountedRelease {
    fn default() -> Self {
        Self::new()
    }
}

/// Deferred release.
pub struct DeferredRelease<F: FnOnce()> {
    func: Option<F>,
    triggered: bool,
}

impl<F: FnOnce()> DeferredRelease<F> {
    /// Create new.
    pub fn new(f: F) -> Self {
        Self {
            func: Some(f),
            triggered: false,
        }
    }

    /// Trigger release.
    pub fn trigger(&mut self) {
        if !self.triggered {
            if let Some(f) = self.func.take() {
                f();
            }
            self.triggered = true;
        }
    }

    /// Cancel release.
    pub fn cancel(&mut self) {
        self.func = None;
        self.triggered = true;
    }

    /// Is triggered.
    pub fn is_triggered(&self) -> bool {
        self.triggered
    }
}

impl<F: FnOnce()> Drop for DeferredRelease<F> {
    fn drop(&mut self) {
        self.trigger();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resource() {
        let mut released = false;
        {
            let mut _r = Resource::new(42, |_| released = true);
        }
        assert!(released);
    }

    #[test]
    fn test_resource_take() {
        let mut released = false;
        {
            let r = Resource::new(42, |_| released = true);
            let _ = r.into_inner();
        }
        assert!(!released);
    }

    #[test]
    fn test_counted() {
        let counter = CountedRelease::new();
        counter.acquire();
        counter.acquire();
        assert_eq!(counter.outstanding(), 2);

        counter.release();
        assert_eq!(counter.outstanding(), 1);

        counter.release();
        assert!(counter.all_released());
    }

    #[test]
    fn test_deferred() {
        let mut triggered = false;
        {
            let _d = DeferredRelease::new(|| triggered = true);
        }
        assert!(triggered);
    }

    #[test]
    fn test_deferred_cancel() {
        let mut triggered = false;
        {
            let mut d = DeferredRelease::new(|| triggered = true);
            d.cancel();
        }
        assert!(!triggered);
    }
}
