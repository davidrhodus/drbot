//! Cell-like wrapper utilities for drbot.
//!
//! This crate provides:
//! - Enhanced Cell variants
//! - Lazy initialization cells
//! - Once cells

use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicBool, Ordering};
use thiserror::Error;

/// Cell error types.
#[derive(Error, Debug, Clone)]
pub enum CellError {
    #[error("Cell already initialized")]
    AlreadyInitialized,

    #[error("Cell not initialized")]
    NotInitialized,

    #[error("Cell is borrowed")]
    Borrowed,
}

/// Result type for cell operations.
pub type Result<T> = std::result::Result<T, CellError>;

/// A cell that can only be set once.
pub struct OnceCell<T> {
    value: UnsafeCell<Option<T>>,
    initialized: AtomicBool,
}

impl<T> OnceCell<T> {
    /// Create new empty once cell.
    pub const fn new() -> Self {
        Self {
            value: UnsafeCell::new(None),
            initialized: AtomicBool::new(false),
        }
    }

    /// Create with value.
    pub fn with_value(value: T) -> Self {
        Self {
            value: UnsafeCell::new(Some(value)),
            initialized: AtomicBool::new(true),
        }
    }

    /// Check if initialized.
    pub fn is_initialized(&self) -> bool {
        self.initialized.load(Ordering::Acquire)
    }

    /// Get reference if initialized.
    pub fn get(&self) -> Option<&T> {
        if self.initialized.load(Ordering::Acquire) {
            // SAFETY: We checked initialized flag with Acquire ordering
            unsafe { (*self.value.get()).as_ref() }
        } else {
            None
        }
    }

    /// Set value if not initialized.
    pub fn set(&self, value: T) -> Result<()> {
        if self
            .initialized
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            // SAFETY: We won the race to initialize
            unsafe {
                *self.value.get() = Some(value);
            }
            Ok(())
        } else {
            Err(CellError::AlreadyInitialized)
        }
    }

    /// Get or initialize with function.
    pub fn get_or_init<F: FnOnce() -> T>(&self, f: F) -> &T {
        if !self.initialized.load(Ordering::Acquire) {
            let _ = self.set(f());
        }
        self.get().unwrap()
    }

    /// Try get or initialize.
    pub fn get_or_try_init<F, E>(&self, f: F) -> std::result::Result<&T, E>
    where
        F: FnOnce() -> std::result::Result<T, E>,
    {
        if !self.initialized.load(Ordering::Acquire) {
            let value = f()?;
            let _ = self.set(value);
        }
        Ok(self.get().unwrap())
    }

    /// Take value if initialized.
    pub fn take(&mut self) -> Option<T> {
        if self.initialized.load(Ordering::Acquire) {
            self.initialized.store(false, Ordering::Release);
            // SAFETY: We have &mut self
            unsafe { (*self.value.get()).take() }
        } else {
            None
        }
    }

    /// Into inner value.
    pub fn into_inner(self) -> Option<T> {
        self.value.into_inner()
    }
}

impl<T> Default for OnceCell<T> {
    fn default() -> Self {
        Self::new()
    }
}

// SAFETY: OnceCell uses atomic operations for synchronization
unsafe impl<T: Send> Send for OnceCell<T> {}
unsafe impl<T: Send + Sync> Sync for OnceCell<T> {}

/// A lazy cell that initializes on first access.
pub struct LazyCell<T, F = fn() -> T> {
    cell: OnceCell<T>,
    init: UnsafeCell<Option<F>>,
}

impl<T, F: FnOnce() -> T> LazyCell<T, F> {
    /// Create new lazy cell.
    pub const fn new(f: F) -> Self {
        Self {
            cell: OnceCell::new(),
            init: UnsafeCell::new(Some(f)),
        }
    }

    /// Force initialization and get reference.
    pub fn force(&self) -> &T {
        self.cell.get_or_init(|| {
            // SAFETY: We only take init once
            let f = unsafe { (*self.init.get()).take() };
            f.expect("LazyCell init already taken")()
        })
    }

    /// Check if initialized.
    pub fn is_initialized(&self) -> bool {
        self.cell.is_initialized()
    }
}

impl<T, F: FnOnce() -> T> std::ops::Deref for LazyCell<T, F> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.force()
    }
}

// SAFETY: LazyCell uses OnceCell which is thread-safe
unsafe impl<T: Send, F: Send> Send for LazyCell<T, F> {}
unsafe impl<T: Send + Sync, F: Send> Sync for LazyCell<T, F> {}

/// A cell with optional value that tracks presence.
pub struct OptionCell<T> {
    value: UnsafeCell<Option<T>>,
}

impl<T> OptionCell<T> {
    /// Create empty option cell.
    pub const fn new() -> Self {
        Self {
            value: UnsafeCell::new(None),
        }
    }

    /// Create with value.
    pub fn some(value: T) -> Self {
        Self {
            value: UnsafeCell::new(Some(value)),
        }
    }

    /// Check if has value.
    pub fn is_some(&self) -> bool {
        // SAFETY: Reading Option's discriminant is safe
        unsafe { (*self.value.get()).is_some() }
    }

    /// Check if empty.
    pub fn is_none(&self) -> bool {
        !self.is_some()
    }

    /// Set value.
    pub fn set(&self, value: T) {
        // SAFETY: Single-threaded access assumed
        unsafe {
            *self.value.get() = Some(value);
        }
    }

    /// Clear value.
    pub fn clear(&self) {
        // SAFETY: Single-threaded access assumed
        unsafe {
            *self.value.get() = None;
        }
    }

    /// Take value.
    pub fn take(&self) -> Option<T> {
        // SAFETY: Single-threaded access assumed
        unsafe { (*self.value.get()).take() }
    }

    /// Replace value.
    pub fn replace(&self, value: T) -> Option<T> {
        // SAFETY: Single-threaded access assumed
        unsafe { (*self.value.get()).replace(value) }
    }
}

impl<T: Copy> OptionCell<T> {
    /// Get copy of value.
    pub fn get(&self) -> Option<T> {
        // SAFETY: T is Copy so reading is safe
        unsafe { *self.value.get() }
    }
}

impl<T> Default for OptionCell<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// A cell that counts accesses.
pub struct CountingCell<T> {
    value: UnsafeCell<T>,
    read_count: std::cell::Cell<usize>,
    write_count: std::cell::Cell<usize>,
}

impl<T> CountingCell<T> {
    /// Create new counting cell.
    pub fn new(value: T) -> Self {
        Self {
            value: UnsafeCell::new(value),
            read_count: std::cell::Cell::new(0),
            write_count: std::cell::Cell::new(0),
        }
    }

    /// Get read count.
    pub fn read_count(&self) -> usize {
        self.read_count.get()
    }

    /// Get write count.
    pub fn write_count(&self) -> usize {
        self.write_count.get()
    }

    /// Reset counts.
    pub fn reset_counts(&self) {
        self.read_count.set(0);
        self.write_count.set(0);
    }

    /// Set value.
    pub fn set(&self, value: T) {
        self.write_count.set(self.write_count.get() + 1);
        // SAFETY: Single-threaded access assumed
        unsafe {
            *self.value.get() = value;
        }
    }
}

impl<T: Copy> CountingCell<T> {
    /// Get value.
    pub fn get(&self) -> T {
        self.read_count.set(self.read_count.get() + 1);
        // SAFETY: T is Copy
        unsafe { *self.value.get() }
    }
}

impl<T: Default> Default for CountingCell<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_once_cell() {
        let cell = OnceCell::new();
        assert!(!cell.is_initialized());

        cell.set(42).unwrap();
        assert!(cell.is_initialized());
        assert_eq!(cell.get(), Some(&42));

        assert!(cell.set(100).is_err());
        assert_eq!(cell.get(), Some(&42));
    }

    #[test]
    fn test_once_cell_get_or_init() {
        let cell = OnceCell::new();

        let value = cell.get_or_init(|| 42);
        assert_eq!(*value, 42);

        let value = cell.get_or_init(|| 100);
        assert_eq!(*value, 42);
    }

    #[test]
    fn test_lazy_cell() {
        let lazy = LazyCell::new(|| 42 * 2);
        assert!(!lazy.is_initialized());

        assert_eq!(*lazy, 84);
        assert!(lazy.is_initialized());
    }

    #[test]
    fn test_option_cell() {
        let cell = OptionCell::<i32>::new();
        assert!(cell.is_none());

        cell.set(42);
        assert!(cell.is_some());
        assert_eq!(cell.get(), Some(42));

        let taken = cell.take();
        assert_eq!(taken, Some(42));
        assert!(cell.is_none());
    }

    #[test]
    fn test_counting_cell() {
        let cell = CountingCell::new(0);
        assert_eq!(cell.read_count(), 0);
        assert_eq!(cell.write_count(), 0);

        cell.set(42);
        assert_eq!(cell.write_count(), 1);

        let _ = cell.get();
        let _ = cell.get();
        assert_eq!(cell.read_count(), 2);
    }
}
