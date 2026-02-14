//! Cell type extensions for drbot.
//!
//! This crate provides:
//! - Cell extensions
//! - RefCell extensions
//! - Lazy cell patterns

use std::cell::{Cell, RefCell};
use thiserror::Error;

/// Cell extension error types.
#[derive(Error, Debug, Clone)]
pub enum CellError {
    #[error("Already borrowed")]
    AlreadyBorrowed,

    #[error("Already mutably borrowed")]
    AlreadyMutablyBorrowed,
}

/// Result type for cell operations.
pub type Result<T> = std::result::Result<T, CellError>;

/// Cell extension trait.
pub trait CellExt<T: Copy> {
    /// Update cell with function.
    fn update<F: FnOnce(T) -> T>(&self, f: F);

    /// Swap value.
    fn swap(&self, value: T) -> T;

    /// Get and set.
    fn get_set(&self, value: T) -> T;

    /// Modify if predicate.
    fn modify_if<F: FnOnce(T) -> T>(&self, predicate: bool, f: F);
}

impl<T: Copy> CellExt<T> for Cell<T> {
    fn update<F: FnOnce(T) -> T>(&self, f: F) {
        self.set(f(self.get()));
    }

    fn swap(&self, value: T) -> T {
        self.replace(value)
    }

    fn get_set(&self, value: T) -> T {
        self.replace(value)
    }

    fn modify_if<F: FnOnce(T) -> T>(&self, predicate: bool, f: F) {
        if predicate {
            self.update(f);
        }
    }
}

/// RefCell extension trait.
pub trait RefCellExt<T> {
    /// Try borrow.
    fn try_get(&self) -> Result<std::cell::Ref<'_, T>>;

    /// Try borrow mut.
    fn try_get_mut(&self) -> Result<std::cell::RefMut<'_, T>>;

    /// Update with function.
    fn update<F: FnOnce(&mut T)>(&self, f: F) -> Result<()>;

    /// Replace value.
    fn swap(&self, value: T) -> Result<T>;
}

impl<T> RefCellExt<T> for RefCell<T> {
    fn try_get(&self) -> Result<std::cell::Ref<'_, T>> {
        self.try_borrow()
            .map_err(|_| CellError::AlreadyMutablyBorrowed)
    }

    fn try_get_mut(&self) -> Result<std::cell::RefMut<'_, T>> {
        self.try_borrow_mut()
            .map_err(|_| CellError::AlreadyBorrowed)
    }

    fn update<F: FnOnce(&mut T)>(&self, f: F) -> Result<()> {
        let mut borrowed = self.try_get_mut()?;
        f(&mut borrowed);
        Ok(())
    }

    fn swap(&self, value: T) -> Result<T> {
        let mut borrowed = self.try_get_mut()?;
        Ok(std::mem::replace(&mut *borrowed, value))
    }
}

/// Lazy cell.
pub struct LazyCell<T, F = fn() -> T> {
    cell: std::cell::OnceCell<T>,
    init: Cell<Option<F>>,
}

impl<T, F: FnOnce() -> T> LazyCell<T, F> {
    /// Create new.
    pub const fn new(init: F) -> Self {
        Self {
            cell: std::cell::OnceCell::new(),
            init: Cell::new(Some(init)),
        }
    }

    /// Get value.
    pub fn get(&self) -> &T {
        self.cell.get_or_init(|| {
            let init = self.init.take().expect("LazyCell already initialized");
            init()
        })
    }

    /// Is initialized.
    pub fn is_initialized(&self) -> bool {
        self.cell.get().is_some()
    }
}

impl<T, F: FnOnce() -> T> std::ops::Deref for LazyCell<T, F> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.get()
    }
}

/// Cached cell.
pub struct CachedCell<T, F> {
    value: RefCell<Option<T>>,
    compute: F,
}

impl<T: Clone, F: Fn() -> T> CachedCell<T, F> {
    /// Create new.
    pub fn new(compute: F) -> Self {
        Self {
            value: RefCell::new(None),
            compute,
        }
    }

    /// Get value (computing if needed).
    pub fn get(&self) -> T {
        let mut value = self.value.borrow_mut();
        if value.is_none() {
            *value = Some((self.compute)());
        }
        value.clone().unwrap()
    }

    /// Invalidate cache.
    pub fn invalidate(&self) {
        *self.value.borrow_mut() = None;
    }

    /// Is cached.
    pub fn is_cached(&self) -> bool {
        self.value.borrow().is_some()
    }
}

/// Cell with history.
pub struct HistoryCell<T: Clone> {
    current: Cell<T>,
    history: RefCell<Vec<T>>,
    max_history: usize,
}

impl<T: Clone + Copy> HistoryCell<T> {
    /// Create new.
    pub fn new(value: T, max_history: usize) -> Self {
        Self {
            current: Cell::new(value),
            history: RefCell::new(Vec::new()),
            max_history,
        }
    }

    /// Get current.
    pub fn get(&self) -> T {
        self.current.get()
    }

    /// Set value.
    pub fn set(&self, value: T) {
        let old = self.current.replace(value);
        let mut history = self.history.borrow_mut();
        if history.len() >= self.max_history {
            history.remove(0);
        }
        history.push(old);
    }

    /// Undo.
    pub fn undo(&self) -> bool {
        let mut history = self.history.borrow_mut();
        if let Some(prev) = history.pop() {
            self.current.set(prev);
            true
        } else {
            false
        }
    }

    /// History length.
    pub fn history_len(&self) -> usize {
        self.history.borrow().len()
    }
}

/// Validated cell.
pub struct ValidatedCell<T: Copy, V: Fn(T) -> bool> {
    cell: Cell<T>,
    validator: V,
}

impl<T: Copy, V: Fn(T) -> bool> ValidatedCell<T, V> {
    /// Create new.
    pub fn new(value: T, validator: V) -> Option<Self> {
        if validator(value) {
            Some(Self {
                cell: Cell::new(value),
                validator,
            })
        } else {
            None
        }
    }

    /// Get value.
    pub fn get(&self) -> T {
        self.cell.get()
    }

    /// Try set value.
    pub fn try_set(&self, value: T) -> bool {
        if (self.validator)(value) {
            self.cell.set(value);
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cell_ext() {
        let cell = Cell::new(5);
        cell.update(|x| x * 2);
        assert_eq!(cell.get(), 10);

        let old = CellExt::swap(&cell, 20);
        assert_eq!(old, 10);
        assert_eq!(cell.get(), 20);
    }

    #[test]
    fn test_refcell_ext() {
        let cell = RefCell::new(5);
        cell.update(|x| *x *= 2).unwrap();
        assert_eq!(*cell.borrow(), 10);
    }

    #[test]
    fn test_lazy_cell() {
        let lazy = LazyCell::new(|| "computed".to_string());
        assert!(!lazy.is_initialized());
        assert_eq!(*lazy, "computed");
        assert!(lazy.is_initialized());
    }

    #[test]
    fn test_history_cell() {
        let cell = HistoryCell::new(1, 5);
        cell.set(2);
        cell.set(3);
        assert_eq!(cell.get(), 3);

        cell.undo();
        assert_eq!(cell.get(), 2);
    }

    #[test]
    fn test_validated_cell() {
        let cell = ValidatedCell::new(5, |x| x > 0).unwrap();
        assert!(cell.try_set(10));
        assert!(!cell.try_set(-1));
        assert_eq!(cell.get(), 10);
    }
}
