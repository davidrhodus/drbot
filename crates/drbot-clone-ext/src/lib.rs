//! Clone trait extensions for drbot.
//!
//! This crate provides:
//! - Clone utilities
//! - Deep cloning
//! - Conditional cloning

use thiserror::Error;

/// Clone extension error types.
#[derive(Error, Debug, Clone)]
pub enum CloneExtError {
    #[error("Clone failed: {0}")]
    Failed(String),
}

/// Result type for clone operations.
pub type Result<T> = std::result::Result<T, CloneExtError>;

/// Clone extension trait.
pub trait CloneExt: Clone {
    /// Clone if predicate is true.
    fn clone_if(&self, predicate: bool) -> Option<Self> {
        if predicate {
            Some(self.clone())
        } else {
            None
        }
    }

    /// Clone n times.
    fn clone_n(&self, n: usize) -> Vec<Self> {
        (0..n).map(|_| self.clone()).collect()
    }

    /// Clone into boxed.
    fn clone_boxed(&self) -> Box<Self> {
        Box::new(self.clone())
    }

    /// Clone with transformation.
    fn clone_with<F: FnOnce(&mut Self)>(&self, f: F) -> Self {
        let mut cloned = self.clone();
        f(&mut cloned);
        cloned
    }
}

impl<T: Clone> CloneExt for T {}

/// Try clone trait.
pub trait TryClone {
    /// Clone error type.
    type Error;

    /// Try to clone.
    fn try_clone(&self) -> std::result::Result<Self, Self::Error>
    where
        Self: Sized;
}

/// Implement TryClone for Clone types.
impl<T: Clone> TryClone for T {
    type Error = std::convert::Infallible;

    fn try_clone(&self) -> std::result::Result<Self, Self::Error> {
        Ok(self.clone())
    }
}

/// Deep clone trait.
pub trait DeepClone {
    /// Perform deep clone.
    fn deep_clone(&self) -> Self;
}

/// Default implementation for Clone types.
impl<T: Clone> DeepClone for T {
    fn deep_clone(&self) -> Self {
        self.clone()
    }
}

/// Clone counter.
#[derive(Debug, Default)]
pub struct CloneCounter {
    count: std::sync::atomic::AtomicUsize,
}

impl CloneCounter {
    /// Create new counter.
    pub fn new() -> Self {
        Self {
            count: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// Increment and get count.
    pub fn increment(&self) -> usize {
        self.count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    }

    /// Get current count.
    pub fn count(&self) -> usize {
        self.count.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Reset count.
    pub fn reset(&self) {
        self.count.store(0, std::sync::atomic::Ordering::Relaxed);
    }
}

/// Counted clone wrapper.
#[derive(Debug)]
pub struct Counted<T> {
    value: T,
    counter: std::sync::Arc<CloneCounter>,
}

impl<T: Clone> Counted<T> {
    /// Create new counted value.
    pub fn new(value: T) -> Self {
        Self {
            value,
            counter: std::sync::Arc::new(CloneCounter::new()),
        }
    }

    /// Get value reference.
    pub fn get(&self) -> &T {
        &self.value
    }

    /// Get mutable reference.
    pub fn get_mut(&mut self) -> &mut T {
        &mut self.value
    }

    /// Get clone count.
    pub fn clone_count(&self) -> usize {
        self.counter.count()
    }

    /// Into inner value.
    pub fn into_inner(self) -> T {
        self.value
    }
}

impl<T: Clone> Clone for Counted<T> {
    fn clone(&self) -> Self {
        self.counter.increment();
        Self {
            value: self.value.clone(),
            counter: self.counter.clone(),
        }
    }
}

/// Lazy clone.
#[derive(Debug)]
pub enum LazyClone<T: Clone> {
    /// Original reference.
    Borrowed(*const T),
    /// Owned clone.
    Owned(T),
}

impl<T: Clone> LazyClone<T> {
    /// Create from reference.
    pub fn new(value: &T) -> Self {
        Self::Borrowed(value as *const T)
    }

    /// Force clone if borrowed.
    pub fn make_owned(&mut self) {
        if let Self::Borrowed(ptr) = *self {
            // SAFETY: The pointer is valid as long as the original reference is valid.
            let cloned = unsafe { (*ptr).clone() };
            *self = Self::Owned(cloned);
        }
    }

    /// Check if owned.
    pub fn is_owned(&self) -> bool {
        matches!(self, Self::Owned(_))
    }

    /// Into owned.
    pub fn into_owned(mut self) -> T {
        self.make_owned();
        match self {
            Self::Owned(v) => v,
            Self::Borrowed(_) => unreachable!(),
        }
    }
}

/// Clone from source.
pub fn clone_from<T: Clone>(source: &T) -> T {
    source.clone()
}

/// Clone into destination.
pub fn clone_into<T: Clone>(source: &T, dest: &mut T) {
    dest.clone_from(source);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clone_ext() {
        let v = 42;
        assert_eq!(v.clone_if(true), Some(42));
        assert_eq!(v.clone_if(false), None);
        assert_eq!(v.clone_n(3), vec![42, 42, 42]);
    }

    #[test]
    fn test_clone_with() {
        let v = vec![1, 2, 3];
        let v2 = v.clone_with(|v| v.push(4));
        assert_eq!(v2, vec![1, 2, 3, 4]);
        assert_eq!(v, vec![1, 2, 3]);
    }

    #[test]
    fn test_counted() {
        let c = Counted::new(42);
        assert_eq!(c.clone_count(), 0);

        let _c2 = c.clone();
        assert_eq!(c.clone_count(), 1);

        let _c3 = c.clone();
        assert_eq!(c.clone_count(), 2);
    }

    #[test]
    fn test_lazy_clone() {
        let original = String::from("hello");
        let mut lazy = LazyClone::new(&original);

        assert!(!lazy.is_owned());
        lazy.make_owned();
        assert!(lazy.is_owned());
    }
}
