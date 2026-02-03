//! Resource disposal for drbot.
//!
//! This crate provides:
//! - Disposable resources
//! - Disposal tracking
//! - Safe cleanup

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use thiserror::Error;

/// Disposer error types.
#[derive(Error, Debug, Clone)]
pub enum DisposerError {
    #[error("Resource already disposed")]
    AlreadyDisposed,

    #[error("Disposal failed: {0}")]
    Failed(String),
}

/// Result type for disposer operations.
pub type Result<T> = std::result::Result<T, DisposerError>;

/// Disposable trait.
pub trait Disposable {
    /// Dispose of the resource.
    fn dispose(&mut self) -> Result<()>;

    /// Check if already disposed.
    fn is_disposed(&self) -> bool;
}

/// Simple disposable wrapper.
pub struct DisposableResource<T, F>
where
    F: FnOnce(T),
{
    resource: Option<T>,
    dispose_fn: Option<F>,
    disposed: bool,
}

impl<T, F> DisposableResource<T, F>
where
    F: FnOnce(T),
{
    /// Create new disposable resource.
    pub fn new(resource: T, dispose_fn: F) -> Self {
        Self {
            resource: Some(resource),
            dispose_fn: Some(dispose_fn),
            disposed: false,
        }
    }

    /// Get reference to resource.
    pub fn get(&self) -> Option<&T> {
        if self.disposed {
            None
        } else {
            self.resource.as_ref()
        }
    }

    /// Get mutable reference to resource.
    pub fn get_mut(&mut self) -> Option<&mut T> {
        if self.disposed {
            None
        } else {
            self.resource.as_mut()
        }
    }

    /// Dispose and return resource.
    pub fn take(mut self) -> Option<T> {
        self.dispose_fn = None;
        self.resource.take()
    }
}

impl<T, F> Disposable for DisposableResource<T, F>
where
    F: FnOnce(T),
{
    fn dispose(&mut self) -> Result<()> {
        if self.disposed {
            return Err(DisposerError::AlreadyDisposed);
        }

        if let (Some(resource), Some(dispose_fn)) = (self.resource.take(), self.dispose_fn.take()) {
            dispose_fn(resource);
        }
        self.disposed = true;
        Ok(())
    }

    fn is_disposed(&self) -> bool {
        self.disposed
    }
}

impl<T, F> Drop for DisposableResource<T, F>
where
    F: FnOnce(T),
{
    fn drop(&mut self) {
        if !self.disposed {
            if let (Some(resource), Some(dispose_fn)) =
                (self.resource.take(), self.dispose_fn.take())
            {
                dispose_fn(resource);
            }
        }
    }
}

/// Create disposable resource.
pub fn disposable<T, F>(resource: T, dispose_fn: F) -> DisposableResource<T, F>
where
    F: FnOnce(T),
{
    DisposableResource::new(resource, dispose_fn)
}

/// Disposal tracker for multiple resources.
pub struct DisposalTracker {
    disposed: AtomicBool,
    resources: std::sync::Mutex<Vec<Box<dyn FnOnce() + Send>>>,
}

impl DisposalTracker {
    /// Create new disposal tracker.
    pub fn new() -> Self {
        Self {
            disposed: AtomicBool::new(false),
            resources: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// Register resource for disposal.
    pub fn register<F>(&self, dispose_fn: F)
    where
        F: FnOnce() + Send + 'static,
    {
        if !self.disposed.load(Ordering::Acquire) {
            let mut resources = self.resources.lock().unwrap();
            resources.push(Box::new(dispose_fn));
        }
    }

    /// Dispose all registered resources.
    pub fn dispose_all(&self) {
        if self.disposed.swap(true, Ordering::AcqRel) {
            return; // Already disposed
        }

        let resources: Vec<_> = {
            let mut resources = self.resources.lock().unwrap();
            std::mem::take(&mut *resources)
        };

        // Dispose in reverse order
        for dispose_fn in resources.into_iter().rev() {
            dispose_fn();
        }
    }

    /// Check if disposed.
    pub fn is_disposed(&self) -> bool {
        self.disposed.load(Ordering::Acquire)
    }
}

impl Default for DisposalTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for DisposalTracker {
    fn drop(&mut self) {
        self.dispose_all();
    }
}

/// Shared disposal tracker.
pub type SharedTracker = Arc<DisposalTracker>;

/// Create shared tracker.
pub fn shared_tracker() -> SharedTracker {
    Arc::new(DisposalTracker::new())
}

/// Auto-dispose guard.
pub struct AutoDispose<T: Disposable> {
    resource: T,
}

impl<T: Disposable> AutoDispose<T> {
    /// Create new auto-dispose guard.
    pub fn new(resource: T) -> Self {
        Self { resource }
    }

    /// Get reference to resource.
    pub fn get(&self) -> &T {
        &self.resource
    }

    /// Get mutable reference to resource.
    pub fn get_mut(&mut self) -> &mut T {
        &mut self.resource
    }

    /// Into inner, taking ownership.
    pub fn into_inner(self) -> T {
        // Prevent dispose on drop
        let resource = std::mem::ManuallyDrop::new(self);
        unsafe { std::ptr::read(&resource.resource) }
    }
}

impl<T: Disposable> Drop for AutoDispose<T> {
    fn drop(&mut self) {
        if !self.resource.is_disposed() {
            let _ = self.resource.dispose();
        }
    }
}

/// Using pattern for disposable resources.
pub fn using<T, R, F>(mut resource: T, f: F) -> Result<R>
where
    T: Disposable,
    F: FnOnce(&mut T) -> R,
{
    let result = f(&mut resource);
    resource.dispose()?;
    Ok(result)
}

/// Disposable handle that tracks disposal state.
pub struct Handle {
    disposed: Arc<AtomicBool>,
}

impl Handle {
    /// Create new handle.
    pub fn new() -> Self {
        Self {
            disposed: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Dispose the handle.
    pub fn dispose(&self) {
        self.disposed.store(true, Ordering::Release);
    }

    /// Check if disposed.
    pub fn is_disposed(&self) -> bool {
        self.disposed.load(Ordering::Acquire)
    }

    /// Clone the disposal state (for tracking).
    pub fn tracker(&self) -> DisposalState {
        DisposalState {
            disposed: self.disposed.clone(),
        }
    }
}

impl Default for Handle {
    fn default() -> Self {
        Self::new()
    }
}

/// Disposal state tracker.
#[derive(Clone)]
pub struct DisposalState {
    disposed: Arc<AtomicBool>,
}

impl DisposalState {
    /// Check if disposed.
    pub fn is_disposed(&self) -> bool {
        self.disposed.load(Ordering::Acquire)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn test_disposable_resource() {
        let disposed = Cell::new(false);
        {
            let _r = disposable(42, |_| disposed.set(true));
        }
        assert!(disposed.get());
    }

    #[test]
    fn test_manual_dispose() {
        let disposed = Cell::new(false);
        let mut r = disposable(42, |_| disposed.set(true));

        r.dispose().unwrap();
        assert!(r.is_disposed());
        assert!(disposed.get());

        // Second dispose should fail
        assert!(r.dispose().is_err());
    }

    #[test]
    fn test_disposal_tracker() {
        let order = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));

        let tracker = DisposalTracker::new();

        let o1 = order.clone();
        tracker.register(move || o1.lock().unwrap().push(1));

        let o2 = order.clone();
        tracker.register(move || o2.lock().unwrap().push(2));

        tracker.dispose_all();

        // Should dispose in reverse order
        assert_eq!(*order.lock().unwrap(), vec![2, 1]);
    }

    #[test]
    fn test_using() {
        let disposed = std::sync::Arc::new(AtomicBool::new(false));
        let d = disposed.clone();

        struct TestResource(Arc<AtomicBool>);
        impl Disposable for TestResource {
            fn dispose(&mut self) -> Result<()> {
                self.0.store(true, Ordering::Release);
                Ok(())
            }
            fn is_disposed(&self) -> bool {
                self.0.load(Ordering::Acquire)
            }
        }

        let result = using(TestResource(d), |_r| 42);
        assert_eq!(result.unwrap(), 42);
        assert!(disposed.load(Ordering::Acquire));
    }

    #[test]
    fn test_handle() {
        let handle = Handle::new();
        let tracker = handle.tracker();

        assert!(!handle.is_disposed());
        assert!(!tracker.is_disposed());

        handle.dispose();

        assert!(handle.is_disposed());
        assert!(tracker.is_disposed());
    }
}
