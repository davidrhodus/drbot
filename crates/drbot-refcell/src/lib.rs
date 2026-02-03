//! RefCell-like wrapper utilities for drbot.
//!
//! This crate provides:
//! - Enhanced RefCell variants
//! - Debug RefCell with borrow tracking
//! - RefCell with callbacks

use std::cell::{Cell, UnsafeCell};
use std::ops::{Deref, DerefMut};
use thiserror::Error;

/// RefCell error types.
#[derive(Error, Debug, Clone)]
pub enum RefCellError {
    #[error("Already borrowed mutably")]
    AlreadyBorrowedMut,

    #[error("Already borrowed")]
    AlreadyBorrowed,

    #[error("Borrow tracking overflow")]
    BorrowOverflow,
}

/// Result type for RefCell operations.
pub type Result<T> = std::result::Result<T, RefCellError>;

/// Borrow state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BorrowState {
    Unused,
    Reading(usize),
    Writing,
}

/// A RefCell with explicit borrow tracking.
pub struct TrackedRefCell<T> {
    value: UnsafeCell<T>,
    state: Cell<BorrowState>,
}

impl<T> TrackedRefCell<T> {
    /// Create new tracked ref cell.
    pub fn new(value: T) -> Self {
        Self {
            value: UnsafeCell::new(value),
            state: Cell::new(BorrowState::Unused),
        }
    }

    /// Try borrow immutably.
    pub fn try_borrow(&self) -> Result<TrackedRef<'_, T>> {
        match self.state.get() {
            BorrowState::Writing => Err(RefCellError::AlreadyBorrowedMut),
            BorrowState::Unused => {
                self.state.set(BorrowState::Reading(1));
                Ok(TrackedRef { cell: self })
            }
            BorrowState::Reading(n) => {
                if n == usize::MAX {
                    return Err(RefCellError::BorrowOverflow);
                }
                self.state.set(BorrowState::Reading(n + 1));
                Ok(TrackedRef { cell: self })
            }
        }
    }

    /// Try borrow mutably.
    pub fn try_borrow_mut(&self) -> Result<TrackedRefMut<'_, T>> {
        match self.state.get() {
            BorrowState::Unused => {
                self.state.set(BorrowState::Writing);
                Ok(TrackedRefMut { cell: self })
            }
            _ => Err(RefCellError::AlreadyBorrowed),
        }
    }

    /// Borrow immutably (panics if borrowed mutably).
    pub fn borrow(&self) -> TrackedRef<'_, T> {
        self.try_borrow().expect("Already borrowed mutably")
    }

    /// Borrow mutably (panics if borrowed).
    pub fn borrow_mut(&self) -> TrackedRefMut<'_, T> {
        self.try_borrow_mut().expect("Already borrowed")
    }

    /// Get current borrow state.
    pub fn borrow_state(&self) -> &'static str {
        match self.state.get() {
            BorrowState::Unused => "unused",
            BorrowState::Reading(n) => {
                if n == 1 {
                    "reading (1)"
                } else {
                    "reading (multiple)"
                }
            }
            BorrowState::Writing => "writing",
        }
    }

    /// Check if currently borrowed.
    pub fn is_borrowed(&self) -> bool {
        !matches!(self.state.get(), BorrowState::Unused)
    }

    /// Into inner value.
    pub fn into_inner(self) -> T {
        self.value.into_inner()
    }

    /// Get mutable reference (requires &mut self).
    pub fn get_mut(&mut self) -> &mut T {
        self.value.get_mut()
    }
}

impl<T: Default> Default for TrackedRefCell<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

/// Immutable borrow guard.
pub struct TrackedRef<'a, T> {
    cell: &'a TrackedRefCell<T>,
}

impl<T> Deref for TrackedRef<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        // SAFETY: We have a valid borrow
        unsafe { &*self.cell.value.get() }
    }
}

impl<T> Drop for TrackedRef<'_, T> {
    fn drop(&mut self) {
        match self.cell.state.get() {
            BorrowState::Reading(1) => {
                self.cell.state.set(BorrowState::Unused);
            }
            BorrowState::Reading(n) => {
                self.cell.state.set(BorrowState::Reading(n - 1));
            }
            _ => unreachable!(),
        }
    }
}

/// Mutable borrow guard.
pub struct TrackedRefMut<'a, T> {
    cell: &'a TrackedRefCell<T>,
}

impl<T> Deref for TrackedRefMut<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        // SAFETY: We have exclusive access
        unsafe { &*self.cell.value.get() }
    }
}

impl<T> DerefMut for TrackedRefMut<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        // SAFETY: We have exclusive access
        unsafe { &mut *self.cell.value.get() }
    }
}

impl<T> Drop for TrackedRefMut<'_, T> {
    fn drop(&mut self) {
        self.cell.state.set(BorrowState::Unused);
    }
}

/// RefCell with callbacks on borrow/release.
pub struct CallbackRefCell<T, F, G>
where
    F: Fn(),
    G: Fn(),
{
    inner: TrackedRefCell<T>,
    on_borrow: F,
    on_release: G,
}

impl<T, F: Fn(), G: Fn()> CallbackRefCell<T, F, G> {
    /// Create with callbacks.
    pub fn new(value: T, on_borrow: F, on_release: G) -> Self {
        Self {
            inner: TrackedRefCell::new(value),
            on_borrow,
            on_release,
        }
    }

    /// Borrow with callback.
    pub fn borrow(&self) -> CallbackRef<'_, T, G> {
        (self.on_borrow)();
        let inner_ref = self.inner.borrow();
        CallbackRef {
            inner: inner_ref,
            on_release: &self.on_release,
        }
    }

    /// Borrow mutably with callback.
    pub fn borrow_mut(&self) -> CallbackRefMut<'_, T, G> {
        (self.on_borrow)();
        let inner_ref = self.inner.borrow_mut();
        CallbackRefMut {
            inner: inner_ref,
            on_release: &self.on_release,
        }
    }
}

/// Callback ref guard.
pub struct CallbackRef<'a, T, G: Fn()> {
    inner: TrackedRef<'a, T>,
    on_release: &'a G,
}

impl<T, G: Fn()> Deref for CallbackRef<'_, T, G> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &*self.inner
    }
}

impl<T, G: Fn()> Drop for CallbackRef<'_, T, G> {
    fn drop(&mut self) {
        (self.on_release)();
    }
}

/// Callback ref mut guard.
pub struct CallbackRefMut<'a, T, G: Fn()> {
    inner: TrackedRefMut<'a, T>,
    on_release: &'a G,
}

impl<T, G: Fn()> Deref for CallbackRefMut<'_, T, G> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &*self.inner
    }
}

impl<T, G: Fn()> DerefMut for CallbackRefMut<'_, T, G> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut *self.inner
    }
}

impl<T, G: Fn()> Drop for CallbackRefMut<'_, T, G> {
    fn drop(&mut self) {
        (self.on_release)();
    }
}

/// RefCell that tracks modification history.
pub struct VersionedRefCell<T> {
    inner: TrackedRefCell<T>,
    version: Cell<u64>,
}

impl<T> VersionedRefCell<T> {
    /// Create new versioned ref cell.
    pub fn new(value: T) -> Self {
        Self {
            inner: TrackedRefCell::new(value),
            version: Cell::new(0),
        }
    }

    /// Get current version.
    pub fn version(&self) -> u64 {
        self.version.get()
    }

    /// Borrow immutably.
    pub fn borrow(&self) -> TrackedRef<'_, T> {
        self.inner.borrow()
    }

    /// Borrow mutably (increments version).
    pub fn borrow_mut(&self) -> VersionedRefMut<'_, T> {
        let inner = self.inner.borrow_mut();
        VersionedRefMut {
            inner,
            version: &self.version,
        }
    }
}

/// Versioned mutable borrow guard.
pub struct VersionedRefMut<'a, T> {
    inner: TrackedRefMut<'a, T>,
    version: &'a Cell<u64>,
}

impl<T> Deref for VersionedRefMut<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &*self.inner
    }
}

impl<T> DerefMut for VersionedRefMut<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut *self.inner
    }
}

impl<T> Drop for VersionedRefMut<'_, T> {
    fn drop(&mut self) {
        self.version.set(self.version.get() + 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tracked_ref_cell() {
        let cell = TrackedRefCell::new(42);

        {
            let r = cell.borrow();
            assert_eq!(*r, 42);
            assert!(cell.is_borrowed());
        }
        assert!(!cell.is_borrowed());
    }

    #[test]
    fn test_multiple_immutable_borrows() {
        let cell = TrackedRefCell::new(42);

        let r1 = cell.borrow();
        let r2 = cell.borrow();
        assert_eq!(*r1, 42);
        assert_eq!(*r2, 42);
    }

    #[test]
    fn test_mutable_borrow() {
        let cell = TrackedRefCell::new(42);

        {
            let mut r = cell.borrow_mut();
            *r = 100;
        }

        assert_eq!(*cell.borrow(), 100);
    }

    #[test]
    fn test_versioned_ref_cell() {
        let cell = VersionedRefCell::new(42);
        assert_eq!(cell.version(), 0);

        {
            let mut r = cell.borrow_mut();
            *r = 100;
        }
        assert_eq!(cell.version(), 1);

        {
            let _ = cell.borrow();
        }
        assert_eq!(cell.version(), 1);
    }
}
