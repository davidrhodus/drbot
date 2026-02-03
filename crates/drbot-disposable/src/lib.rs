//! Disposable pattern utilities for drbot.
//!
//! This crate provides:
//! - Disposable trait
//! - Auto-dispose wrappers
//! - Dispose groups
//! - Cleanup callbacks

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use thiserror::Error;

/// Disposable error types.
#[derive(Error, Debug)]
pub enum DisposableError {
    #[error("Already disposed")]
    AlreadyDisposed,

    #[error("Dispose failed: {0}")]
    DisposeFailed(String),
}

/// Result type for disposable operations.
pub type Result<T> = std::result::Result<T, DisposableError>;

/// Disposable trait.
pub trait Disposable {
    /// Dispose the resource.
    fn dispose(&mut self);

    /// Check if disposed.
    fn is_disposed(&self) -> bool;
}

/// Simple disposable wrapper.
pub struct DisposableValue<T> {
    value: Option<T>,
    on_dispose: Option<Box<dyn FnOnce(T) + Send>>,
}

impl<T> DisposableValue<T> {
    /// Create new disposable.
    pub fn new(value: T) -> Self {
        Self {
            value: Some(value),
            on_dispose: None,
        }
    }

    /// Create with dispose callback.
    pub fn with_callback<F>(value: T, on_dispose: F) -> Self
    where
        F: FnOnce(T) + Send + 'static,
    {
        Self {
            value: Some(value),
            on_dispose: Some(Box::new(on_dispose)),
        }
    }

    /// Get reference to value.
    pub fn get(&self) -> Option<&T> {
        self.value.as_ref()
    }

    /// Get mutable reference to value.
    pub fn get_mut(&mut self) -> Option<&mut T> {
        self.value.as_mut()
    }

    /// Take value without disposing.
    pub fn take(&mut self) -> Option<T> {
        self.on_dispose = None;
        self.value.take()
    }
}

impl<T> Disposable for DisposableValue<T> {
    fn dispose(&mut self) {
        if let Some(value) = self.value.take() {
            if let Some(callback) = self.on_dispose.take() {
                callback(value);
            }
        }
    }

    fn is_disposed(&self) -> bool {
        self.value.is_none()
    }
}

impl<T> Drop for DisposableValue<T> {
    fn drop(&mut self) {
        self.dispose();
    }
}

/// Disposable handle with cleanup function.
pub struct DisposableHandle {
    disposed: AtomicBool,
    cleanup: Mutex<Option<Box<dyn FnOnce() + Send>>>,
}

impl DisposableHandle {
    /// Create new handle.
    pub fn new<F>(cleanup: F) -> Self
    where
        F: FnOnce() + Send + 'static,
    {
        Self {
            disposed: AtomicBool::new(false),
            cleanup: Mutex::new(Some(Box::new(cleanup))),
        }
    }

    /// Create empty handle.
    pub fn empty() -> Self {
        Self {
            disposed: AtomicBool::new(false),
            cleanup: Mutex::new(None),
        }
    }
}

impl Disposable for DisposableHandle {
    fn dispose(&mut self) {
        if !self.disposed.swap(true, Ordering::SeqCst) {
            if let Some(cleanup) = self.cleanup.lock().unwrap().take() {
                cleanup();
            }
        }
    }

    fn is_disposed(&self) -> bool {
        self.disposed.load(Ordering::SeqCst)
    }
}

impl Drop for DisposableHandle {
    fn drop(&mut self) {
        self.dispose();
    }
}

/// Group of disposables.
pub struct DisposableGroup {
    disposables: Mutex<Vec<Box<dyn Disposable + Send>>>,
    disposed: AtomicBool,
}

impl DisposableGroup {
    /// Create new group.
    pub fn new() -> Self {
        Self {
            disposables: Mutex::new(Vec::new()),
            disposed: AtomicBool::new(false),
        }
    }

    /// Add disposable to group.
    pub fn add<D: Disposable + Send + 'static>(&self, disposable: D) {
        if !self.is_disposed() {
            self.disposables.lock().unwrap().push(Box::new(disposable));
        }
    }

    /// Add cleanup function.
    pub fn add_cleanup<F>(&self, cleanup: F)
    where
        F: FnOnce() + Send + 'static,
    {
        self.add(DisposableHandle::new(cleanup));
    }

    /// Get count.
    pub fn count(&self) -> usize {
        self.disposables.lock().unwrap().len()
    }

    /// Clear all without disposing.
    pub fn clear(&self) {
        self.disposables.lock().unwrap().clear();
    }
}

impl Default for DisposableGroup {
    fn default() -> Self {
        Self::new()
    }
}

impl Disposable for DisposableGroup {
    fn dispose(&mut self) {
        if !self.disposed.swap(true, Ordering::SeqCst) {
            let mut disposables = self.disposables.lock().unwrap();
            // Dispose in reverse order
            while let Some(mut d) = disposables.pop() {
                d.dispose();
            }
        }
    }

    fn is_disposed(&self) -> bool {
        self.disposed.load(Ordering::SeqCst)
    }
}

impl Drop for DisposableGroup {
    fn drop(&mut self) {
        self.dispose();
    }
}

/// Shared disposable.
pub struct SharedDisposable {
    inner: Arc<Mutex<Option<Box<dyn FnOnce() + Send>>>>,
    disposed: Arc<AtomicBool>,
}

impl SharedDisposable {
    /// Create new shared disposable.
    pub fn new<F>(cleanup: F) -> Self
    where
        F: FnOnce() + Send + 'static,
    {
        Self {
            inner: Arc::new(Mutex::new(Some(Box::new(cleanup)))),
            disposed: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Dispose (any clone can dispose).
    pub fn dispose(&self) {
        if !self.disposed.swap(true, Ordering::SeqCst) {
            if let Some(cleanup) = self.inner.lock().unwrap().take() {
                cleanup();
            }
        }
    }

    /// Check if disposed.
    pub fn is_disposed(&self) -> bool {
        self.disposed.load(Ordering::SeqCst)
    }
}

impl Clone for SharedDisposable {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            disposed: self.disposed.clone(),
        }
    }
}

/// Scope guard that runs cleanup on drop.
pub struct ScopeGuard<F: FnOnce()> {
    cleanup: Option<F>,
}

impl<F: FnOnce()> ScopeGuard<F> {
    /// Create new scope guard.
    pub fn new(cleanup: F) -> Self {
        Self {
            cleanup: Some(cleanup),
        }
    }

    /// Dismiss guard without running cleanup.
    pub fn dismiss(mut self) {
        self.cleanup = None;
    }
}

impl<F: FnOnce()> Drop for ScopeGuard<F> {
    fn drop(&mut self) {
        if let Some(cleanup) = self.cleanup.take() {
            cleanup();
        }
    }
}

/// Create scope guard.
pub fn defer<F: FnOnce()>(cleanup: F) -> ScopeGuard<F> {
    ScopeGuard::new(cleanup)
}

/// Disposable adapter for closures.
pub struct DisposeAdapter<F: FnMut()> {
    cleanup: Option<F>,
    disposed: bool,
}

impl<F: FnMut()> DisposeAdapter<F> {
    /// Create new adapter.
    pub fn new(cleanup: F) -> Self {
        Self {
            cleanup: Some(cleanup),
            disposed: false,
        }
    }
}

impl<F: FnMut()> Disposable for DisposeAdapter<F> {
    fn dispose(&mut self) {
        if !self.disposed {
            self.disposed = true;
            if let Some(mut cleanup) = self.cleanup.take() {
                cleanup();
            }
        }
    }

    fn is_disposed(&self) -> bool {
        self.disposed
    }
}

impl<F: FnMut()> Drop for DisposeAdapter<F> {
    fn drop(&mut self) {
        self.dispose();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    #[test]
    fn test_disposable_value() {
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();

        {
            let _disposable = DisposableValue::with_callback(42, move |_| {
                counter_clone.fetch_add(1, Ordering::SeqCst);
            });
        }

        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_disposable_handle() {
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();

        {
            let _handle = DisposableHandle::new(move || {
                counter_clone.fetch_add(1, Ordering::SeqCst);
            });
        }

        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_disposable_group() {
        let counter = Arc::new(AtomicUsize::new(0));

        {
            let mut group = DisposableGroup::new();
            for _ in 0..3 {
                let c = counter.clone();
                group.add_cleanup(move || {
                    c.fetch_add(1, Ordering::SeqCst);
                });
            }
        }

        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn test_shared_disposable() {
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();

        let disposable = SharedDisposable::new(move || {
            counter_clone.fetch_add(1, Ordering::SeqCst);
        });

        let clone1 = disposable.clone();
        let clone2 = disposable.clone();

        clone1.dispose();
        clone2.dispose(); // Should not run again
        disposable.dispose(); // Should not run again

        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_scope_guard() {
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();

        {
            let _guard = defer(move || {
                counter_clone.fetch_add(1, Ordering::SeqCst);
            });
        }

        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_scope_guard_dismiss() {
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();

        {
            let guard = defer(move || {
                counter_clone.fetch_add(1, Ordering::SeqCst);
            });
            guard.dismiss();
        }

        assert_eq!(counter.load(Ordering::SeqCst), 0);
    }
}
