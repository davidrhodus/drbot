//! Reference counting utilities for drbot.
//!
//! This crate provides:
//! - Rc-like containers with extra features
//! - Weak reference utilities
//! - Reference counting patterns

use std::cell::Cell;
use std::ops::Deref;
use std::ptr::NonNull;
use thiserror::Error;

/// Rc error types.
#[derive(Error, Debug, Clone)]
pub enum RcError {
    #[error("No strong references remain")]
    NoStrongRefs,

    #[error("Reference count overflow")]
    Overflow,
}

/// Result type for Rc operations.
pub type Result<T> = std::result::Result<T, RcError>;

/// Reference counted inner data.
struct RcInner<T> {
    value: T,
    strong_count: Cell<usize>,
    weak_count: Cell<usize>,
}

/// A reference counted pointer with tracking.
pub struct TrackedRc<T> {
    ptr: NonNull<RcInner<T>>,
}

impl<T> TrackedRc<T> {
    /// Create new tracked Rc.
    pub fn new(value: T) -> Self {
        let inner = Box::new(RcInner {
            value,
            strong_count: Cell::new(1),
            weak_count: Cell::new(1), // One weak for all strongs
        });
        Self {
            ptr: NonNull::new(Box::into_raw(inner)).unwrap(),
        }
    }

    fn inner(&self) -> &RcInner<T> {
        // SAFETY: ptr is always valid while TrackedRc exists
        unsafe { self.ptr.as_ref() }
    }

    /// Get strong reference count.
    pub fn strong_count(&self) -> usize {
        self.inner().strong_count.get()
    }

    /// Get weak reference count.
    pub fn weak_count(&self) -> usize {
        self.inner().weak_count.get() - 1 // Subtract the one for strongs
    }

    /// Create a weak reference.
    pub fn downgrade(&self) -> WeakTrackedRc<T> {
        let inner = self.inner();
        inner.weak_count.set(inner.weak_count.get() + 1);
        WeakTrackedRc { ptr: self.ptr }
    }

    /// Try to get mutable reference if unique.
    pub fn get_mut(&mut self) -> Option<&mut T> {
        if self.strong_count() == 1 && self.weak_count() == 0 {
            // SAFETY: We are the only reference
            Some(unsafe { &mut (*self.ptr.as_ptr()).value })
        } else {
            None
        }
    }

    /// Make mutable (clone if shared).
    pub fn make_mut(&mut self) -> &mut T
    where
        T: Clone,
    {
        if self.strong_count() != 1 || self.weak_count() != 0 {
            // Clone the value
            *self = TrackedRc::new((**self).clone());
        }
        self.get_mut().unwrap()
    }

    /// Try to unwrap if unique.
    pub fn try_unwrap(self) -> std::result::Result<T, Self> {
        if self.strong_count() == 1 {
            // SAFETY: We are the only strong reference
            unsafe {
                let inner = self.inner();
                inner.strong_count.set(0);

                // Read the value
                let value = std::ptr::read(&(*self.ptr.as_ptr()).value);

                // Decrement weak count
                let weak = inner.weak_count.get() - 1;
                inner.weak_count.set(weak);

                if weak == 0 {
                    // Deallocate
                    drop(Box::from_raw(self.ptr.as_ptr()));
                }

                std::mem::forget(self);
                Ok(value)
            }
        } else {
            Err(self)
        }
    }

    /// Check if this is the only reference.
    pub fn is_unique(&self) -> bool {
        self.strong_count() == 1 && self.weak_count() == 0
    }

    /// Check if pointers are equal.
    pub fn ptr_eq(this: &Self, other: &Self) -> bool {
        this.ptr == other.ptr
    }
}

impl<T> Clone for TrackedRc<T> {
    fn clone(&self) -> Self {
        let inner = self.inner();
        inner.strong_count.set(inner.strong_count.get() + 1);
        Self { ptr: self.ptr }
    }
}

impl<T> Deref for TrackedRc<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner().value
    }
}

impl<T> Drop for TrackedRc<T> {
    fn drop(&mut self) {
        let inner = self.inner();
        let strong = inner.strong_count.get() - 1;
        inner.strong_count.set(strong);

        if strong == 0 {
            // Drop the value
            unsafe {
                std::ptr::drop_in_place(&mut (*self.ptr.as_ptr()).value);
            }

            // Decrement weak count
            let weak = inner.weak_count.get() - 1;
            inner.weak_count.set(weak);

            if weak == 0 {
                // Deallocate
                unsafe {
                    drop(Box::from_raw(self.ptr.as_ptr()));
                }
            }
        }
    }
}

/// Weak reference to TrackedRc.
pub struct WeakTrackedRc<T> {
    ptr: NonNull<RcInner<T>>,
}

impl<T> WeakTrackedRc<T> {
    fn inner(&self) -> &RcInner<T> {
        // SAFETY: ptr is valid while weak count > 0
        unsafe { self.ptr.as_ref() }
    }

    /// Try to upgrade to strong reference.
    pub fn upgrade(&self) -> Option<TrackedRc<T>> {
        let inner = self.inner();
        let strong = inner.strong_count.get();
        if strong == 0 {
            None
        } else {
            inner.strong_count.set(strong + 1);
            Some(TrackedRc { ptr: self.ptr })
        }
    }

    /// Get strong count.
    pub fn strong_count(&self) -> usize {
        self.inner().strong_count.get()
    }

    /// Get weak count.
    pub fn weak_count(&self) -> usize {
        self.inner().weak_count.get() - if self.strong_count() > 0 { 1 } else { 0 }
    }
}

impl<T> Clone for WeakTrackedRc<T> {
    fn clone(&self) -> Self {
        let inner = self.inner();
        inner.weak_count.set(inner.weak_count.get() + 1);
        Self { ptr: self.ptr }
    }
}

impl<T> Drop for WeakTrackedRc<T> {
    fn drop(&mut self) {
        let inner = self.inner();
        let weak = inner.weak_count.get() - 1;
        inner.weak_count.set(weak);

        if weak == 0 {
            // Deallocate
            unsafe {
                drop(Box::from_raw(self.ptr.as_ptr()));
            }
        }
    }
}

/// A reference counter that can be shared.
#[derive(Debug, Clone)]
pub struct RefCounter {
    count: std::rc::Rc<Cell<usize>>,
}

impl RefCounter {
    /// Create new counter starting at 0.
    pub fn new() -> Self {
        Self {
            count: std::rc::Rc::new(Cell::new(0)),
        }
    }

    /// Increment counter.
    pub fn increment(&self) -> usize {
        let new = self.count.get() + 1;
        self.count.set(new);
        new
    }

    /// Decrement counter.
    pub fn decrement(&self) -> usize {
        let current = self.count.get();
        if current > 0 {
            self.count.set(current - 1);
            current - 1
        } else {
            0
        }
    }

    /// Get current count.
    pub fn get(&self) -> usize {
        self.count.get()
    }

    /// Reset to zero.
    pub fn reset(&self) {
        self.count.set(0);
    }
}

impl Default for RefCounter {
    fn default() -> Self {
        Self::new()
    }
}

/// A guard that decrements counter on drop.
pub struct CountGuard {
    counter: RefCounter,
}

impl CountGuard {
    /// Create guard that increments counter.
    pub fn new(counter: RefCounter) -> Self {
        counter.increment();
        Self { counter }
    }
}

impl Drop for CountGuard {
    fn drop(&mut self) {
        self.counter.decrement();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tracked_rc() {
        let rc = TrackedRc::new(42);
        assert_eq!(*rc, 42);
        assert_eq!(rc.strong_count(), 1);

        let rc2 = rc.clone();
        assert_eq!(rc.strong_count(), 2);

        drop(rc2);
        assert_eq!(rc.strong_count(), 1);
    }

    #[test]
    fn test_weak_upgrade() {
        let rc = TrackedRc::new(42);
        let weak = rc.downgrade();
        assert_eq!(weak.strong_count(), 1);

        let upgraded = weak.upgrade();
        assert!(upgraded.is_some());
        assert_eq!(*upgraded.unwrap(), 42);

        drop(rc);
        let upgraded = weak.upgrade();
        assert!(upgraded.is_none());
    }

    #[test]
    fn test_make_mut() {
        let mut rc = TrackedRc::new(42);
        *rc.make_mut() = 100;
        assert_eq!(*rc, 100);

        let rc2 = rc.clone();
        *rc.make_mut() = 200; // Should clone
        assert_eq!(*rc, 200);
        assert_eq!(*rc2, 100);
    }

    #[test]
    fn test_ref_counter() {
        let counter = RefCounter::new();
        assert_eq!(counter.get(), 0);

        counter.increment();
        counter.increment();
        assert_eq!(counter.get(), 2);

        counter.decrement();
        assert_eq!(counter.get(), 1);
    }

    #[test]
    fn test_count_guard() {
        let counter = RefCounter::new();

        {
            let _guard1 = CountGuard::new(counter.clone());
            assert_eq!(counter.get(), 1);

            let _guard2 = CountGuard::new(counter.clone());
            assert_eq!(counter.get(), 2);
        }

        assert_eq!(counter.get(), 0);
    }
}
