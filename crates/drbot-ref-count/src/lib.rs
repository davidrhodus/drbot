//! Reference counting utilities for drbot.
//!
//! This crate provides:
//! - Reference counting helpers
//! - Rc/Arc extensions
//! - Shared ownership utilities

use std::rc::Rc;
use std::sync::Arc;
use thiserror::Error;

/// Reference count error types.
#[derive(Error, Debug, Clone)]
pub enum RefCountError {
    #[error("Not unique")]
    NotUnique,

    #[error("Dropped")]
    Dropped,
}

/// Result type for ref count operations.
pub type Result<T> = std::result::Result<T, RefCountError>;

/// Rc extension trait.
pub trait RcExt<T> {
    /// Clone and get reference.
    fn share(&self) -> (Self, &T)
    where
        Self: Sized;

    /// Strong count.
    fn strong(&self) -> usize;

    /// Weak count.
    fn weak(&self) -> usize;

    /// Is unique.
    fn is_unique(&self) -> bool;

    /// Try unwrap.
    fn try_unwrap_or_clone(self) -> T
    where
        T: Clone;
}

impl<T> RcExt<T> for Rc<T> {
    fn share(&self) -> (Self, &T) {
        (Rc::clone(self), self.as_ref())
    }

    fn strong(&self) -> usize {
        Rc::strong_count(self)
    }

    fn weak(&self) -> usize {
        Rc::weak_count(self)
    }

    fn is_unique(&self) -> bool {
        Rc::strong_count(self) == 1 && Rc::weak_count(self) == 0
    }

    fn try_unwrap_or_clone(self) -> T
    where
        T: Clone,
    {
        Rc::try_unwrap(self).unwrap_or_else(|rc| (*rc).clone())
    }
}

/// Arc extension trait.
pub trait ArcExt<T> {
    /// Clone and get reference.
    fn share(&self) -> (Self, &T)
    where
        Self: Sized;

    /// Strong count.
    fn strong(&self) -> usize;

    /// Weak count.
    fn weak(&self) -> usize;

    /// Is unique.
    fn is_unique(&self) -> bool;

    /// Try unwrap.
    fn try_unwrap_or_clone(self) -> T
    where
        T: Clone;
}

impl<T> ArcExt<T> for Arc<T> {
    fn share(&self) -> (Self, &T) {
        (Arc::clone(self), self.as_ref())
    }

    fn strong(&self) -> usize {
        Arc::strong_count(self)
    }

    fn weak(&self) -> usize {
        Arc::weak_count(self)
    }

    fn is_unique(&self) -> bool {
        Arc::strong_count(self) == 1 && Arc::weak_count(self) == 0
    }

    fn try_unwrap_or_clone(self) -> T
    where
        T: Clone,
    {
        Arc::try_unwrap(self).unwrap_or_else(|arc| (*arc).clone())
    }
}

/// Create Rc from value.
pub fn rc<T>(value: T) -> Rc<T> {
    Rc::new(value)
}

/// Create Arc from value.
pub fn arc<T>(value: T) -> Arc<T> {
    Arc::new(value)
}

/// Clone Rc.
pub fn clone_rc<T>(rc: &Rc<T>) -> Rc<T> {
    Rc::clone(rc)
}

/// Clone Arc.
pub fn clone_arc<T>(arc: &Arc<T>) -> Arc<T> {
    Arc::clone(arc)
}

/// Weak reference wrapper.
pub struct WeakRef<T> {
    weak: std::sync::Weak<T>,
}

impl<T> WeakRef<T> {
    /// Create from Arc.
    pub fn new(arc: &Arc<T>) -> Self {
        Self {
            weak: Arc::downgrade(arc),
        }
    }

    /// Try upgrade to Arc.
    pub fn upgrade(&self) -> Option<Arc<T>> {
        self.weak.upgrade()
    }

    /// Is alive (can be upgraded).
    pub fn is_alive(&self) -> bool {
        self.weak.strong_count() > 0
    }

    /// Strong count.
    pub fn strong_count(&self) -> usize {
        self.weak.strong_count()
    }

    /// Weak count.
    pub fn weak_count(&self) -> usize {
        self.weak.weak_count()
    }
}

impl<T> Clone for WeakRef<T> {
    fn clone(&self) -> Self {
        Self {
            weak: self.weak.clone(),
        }
    }
}

/// Local weak reference (non-thread-safe).
pub struct LocalWeakRef<T> {
    weak: std::rc::Weak<T>,
}

impl<T> LocalWeakRef<T> {
    /// Create from Rc.
    pub fn new(rc: &Rc<T>) -> Self {
        Self {
            weak: Rc::downgrade(rc),
        }
    }

    /// Try upgrade to Rc.
    pub fn upgrade(&self) -> Option<Rc<T>> {
        self.weak.upgrade()
    }

    /// Is alive.
    pub fn is_alive(&self) -> bool {
        self.weak.strong_count() > 0
    }

    /// Strong count.
    pub fn strong_count(&self) -> usize {
        self.weak.strong_count()
    }

    /// Weak count.
    pub fn weak_count(&self) -> usize {
        self.weak.weak_count()
    }
}

impl<T> Clone for LocalWeakRef<T> {
    fn clone(&self) -> Self {
        Self {
            weak: self.weak.clone(),
        }
    }
}

/// Reference counter.
#[derive(Debug)]
pub struct RefCounter {
    count: std::sync::atomic::AtomicUsize,
}

impl RefCounter {
    /// Create new.
    pub const fn new() -> Self {
        Self {
            count: std::sync::atomic::AtomicUsize::new(1),
        }
    }

    /// Increment.
    pub fn increment(&self) -> usize {
        self.count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            + 1
    }

    /// Decrement.
    pub fn decrement(&self) -> usize {
        self.count
            .fetch_sub(1, std::sync::atomic::Ordering::Relaxed)
            - 1
    }

    /// Get count.
    pub fn count(&self) -> usize {
        self.count.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Is unique.
    pub fn is_unique(&self) -> bool {
        self.count() == 1
    }

    /// Is zero.
    pub fn is_zero(&self) -> bool {
        self.count() == 0
    }
}

impl Default for RefCounter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rc_ext() {
        let r = rc(42);
        assert!(r.is_unique());
        assert_eq!(r.strong(), 1);

        let r2 = clone_rc(&r);
        assert!(!r.is_unique());
        assert_eq!(r.strong(), 2);
        drop(r2);
    }

    #[test]
    fn test_arc_ext() {
        let a = arc(42);
        assert!(a.is_unique());

        let a2 = clone_arc(&a);
        assert!(!a.is_unique());
        assert_eq!(a.strong(), 2);
        drop(a2);
    }

    #[test]
    fn test_weak_ref() {
        let a = arc(42);
        let weak = WeakRef::new(&a);
        assert!(weak.is_alive());

        let upgraded = weak.upgrade().unwrap();
        assert_eq!(*upgraded, 42);

        drop(a);
        drop(upgraded);
        assert!(!weak.is_alive());
    }

    #[test]
    fn test_ref_counter() {
        let counter = RefCounter::new();
        assert_eq!(counter.count(), 1);
        assert!(counter.is_unique());

        counter.increment();
        assert_eq!(counter.count(), 2);

        counter.decrement();
        assert_eq!(counter.count(), 1);
    }
}
