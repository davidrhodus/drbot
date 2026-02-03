//! Initialization utilities for drbot.
//!
//! This crate provides:
//! - Initialization helpers
//! - Once initialization
//! - Lazy initialization

use std::sync::Once;
use thiserror::Error;

/// Init error types.
#[derive(Error, Debug, Clone)]
pub enum InitError {
    #[error("Already initialized")]
    AlreadyInitialized,

    #[error("Not initialized")]
    NotInitialized,

    #[error("Init failed: {0}")]
    Failed(String),
}

/// Result type for init operations.
pub type Result<T> = std::result::Result<T, InitError>;

/// Once initializer.
pub struct OnceInit<T> {
    once: Once,
    value: std::cell::UnsafeCell<Option<T>>,
}

impl<T> OnceInit<T> {
    /// Create new.
    pub const fn new() -> Self {
        Self {
            once: Once::new(),
            value: std::cell::UnsafeCell::new(None),
        }
    }

    /// Initialize.
    pub fn init<F: FnOnce() -> T>(&self, f: F) {
        self.once.call_once(|| {
            // SAFETY: This is only called once.
            unsafe {
                *self.value.get() = Some(f());
            }
        });
    }

    /// Try initialize.
    pub fn try_init<F: FnOnce() -> T>(&self, f: F) -> Result<()> {
        if self.is_initialized() {
            return Err(InitError::AlreadyInitialized);
        }
        self.init(f);
        Ok(())
    }

    /// Get value.
    pub fn get(&self) -> Option<&T> {
        if self.is_initialized() {
            // SAFETY: Value is initialized and immutable.
            unsafe { (*self.value.get()).as_ref() }
        } else {
            None
        }
    }

    /// Get or init.
    pub fn get_or_init<F: FnOnce() -> T>(&self, f: F) -> &T {
        self.init(f);
        self.get().unwrap()
    }

    /// Is initialized.
    pub fn is_initialized(&self) -> bool {
        self.once.is_completed()
    }
}

// SAFETY: OnceInit uses Once for synchronization.
unsafe impl<T: Send + Sync> Sync for OnceInit<T> {}
unsafe impl<T: Send> Send for OnceInit<T> {}

impl<T> Default for OnceInit<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Lazy value.
pub struct Lazy<T, F = fn() -> T> {
    cell: std::cell::OnceCell<T>,
    init: std::cell::Cell<Option<F>>,
}

impl<T, F: FnOnce() -> T> Lazy<T, F> {
    /// Create new lazy value.
    pub const fn new(f: F) -> Self {
        Self {
            cell: std::cell::OnceCell::new(),
            init: std::cell::Cell::new(Some(f)),
        }
    }

    /// Get value.
    pub fn get(&self) -> &T {
        self.cell.get_or_init(|| {
            let f = self.init.take().expect("Lazy already initialized");
            f()
        })
    }

    /// Is initialized.
    pub fn is_initialized(&self) -> bool {
        self.cell.get().is_some()
    }
}

impl<T, F: FnOnce() -> T> std::ops::Deref for Lazy<T, F> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.get()
    }
}

/// Init guard.
pub struct InitGuard<T> {
    value: Option<T>,
    initialized: bool,
}

impl<T> InitGuard<T> {
    /// Create new.
    pub fn new() -> Self {
        Self {
            value: None,
            initialized: false,
        }
    }

    /// Initialize.
    pub fn init(&mut self, value: T) -> Result<()> {
        if self.initialized {
            return Err(InitError::AlreadyInitialized);
        }
        self.value = Some(value);
        self.initialized = true;
        Ok(())
    }

    /// Get value.
    pub fn get(&self) -> Result<&T> {
        self.value.as_ref().ok_or(InitError::NotInitialized)
    }

    /// Get mutable.
    pub fn get_mut(&mut self) -> Result<&mut T> {
        self.value.as_mut().ok_or(InitError::NotInitialized)
    }

    /// Is initialized.
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    /// Take value.
    pub fn take(&mut self) -> Result<T> {
        if !self.initialized {
            return Err(InitError::NotInitialized);
        }
        self.initialized = false;
        self.value.take().ok_or(InitError::NotInitialized)
    }
}

impl<T> Default for InitGuard<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Initialization state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InitState {
    /// Not initialized.
    Uninitialized,
    /// Initializing.
    Initializing,
    /// Initialized.
    Initialized,
    /// Failed.
    Failed,
}

/// Stateful initializer.
pub struct StatefulInit<T> {
    state: InitState,
    value: Option<T>,
    error: Option<String>,
}

impl<T> StatefulInit<T> {
    /// Create new.
    pub fn new() -> Self {
        Self {
            state: InitState::Uninitialized,
            value: None,
            error: None,
        }
    }

    /// Get state.
    pub fn state(&self) -> InitState {
        self.state
    }

    /// Start initialization.
    pub fn start(&mut self) -> Result<()> {
        if self.state != InitState::Uninitialized {
            return Err(InitError::AlreadyInitialized);
        }
        self.state = InitState::Initializing;
        Ok(())
    }

    /// Complete initialization.
    pub fn complete(&mut self, value: T) {
        self.value = Some(value);
        self.state = InitState::Initialized;
    }

    /// Fail initialization.
    pub fn fail(&mut self, error: &str) {
        self.error = Some(error.to_string());
        self.state = InitState::Failed;
    }

    /// Get value.
    pub fn get(&self) -> Result<&T> {
        match self.state {
            InitState::Initialized => self.value.as_ref().ok_or(InitError::NotInitialized),
            InitState::Failed => Err(InitError::Failed(self.error.clone().unwrap_or_default())),
            _ => Err(InitError::NotInitialized),
        }
    }
}

impl<T> Default for StatefulInit<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_once_init() {
        let init: OnceInit<i32> = OnceInit::new();
        assert!(!init.is_initialized());

        init.init(|| 42);
        assert!(init.is_initialized());
        assert_eq!(init.get(), Some(&42));
    }

    #[test]
    fn test_lazy() {
        let lazy = Lazy::new(|| "hello".to_string());
        assert!(!lazy.is_initialized());
        assert_eq!(lazy.get(), "hello");
        assert!(lazy.is_initialized());
    }

    #[test]
    fn test_init_guard() {
        let mut guard: InitGuard<i32> = InitGuard::new();
        assert!(!guard.is_initialized());

        guard.init(42).unwrap();
        assert!(guard.is_initialized());
        assert_eq!(guard.get().unwrap(), &42);

        assert!(guard.init(84).is_err());
    }

    #[test]
    fn test_stateful_init() {
        let mut init: StatefulInit<i32> = StatefulInit::new();
        assert_eq!(init.state(), InitState::Uninitialized);

        init.start().unwrap();
        assert_eq!(init.state(), InitState::Initializing);

        init.complete(42);
        assert_eq!(init.state(), InitState::Initialized);
        assert_eq!(init.get().unwrap(), &42);
    }
}
