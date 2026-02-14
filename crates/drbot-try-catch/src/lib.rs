//! Try/catch style error handling for drbot.
//!
//! This crate provides:
//! - Try/catch blocks
//! - Exception-style handling
//! - Multi-catch support

use std::any::Any;
use std::panic::{self, AssertUnwindSafe};
use thiserror::Error;

/// Try/catch error types.
#[derive(Error, Debug)]
pub enum TryCatchError {
    #[error("Panic occurred: {0}")]
    Panic(String),

    #[error("Error caught: {0}")]
    Caught(String),

    #[error("Unhandled error")]
    Unhandled,
}

/// Result type for try/catch operations.
pub type Result<T> = std::result::Result<T, TryCatchError>;

/// Execute code with panic catching.
pub fn try_catch<T, F>(f: F) -> Result<T>
where
    F: FnOnce() -> T + panic::UnwindSafe,
{
    match panic::catch_unwind(f) {
        Ok(value) => Ok(value),
        Err(panic_info) => {
            let message = panic_to_string(&panic_info);
            Err(TryCatchError::Panic(message))
        }
    }
}

/// Execute code with panic catching and handler.
pub fn try_catch_with<T, F, H>(f: F, handler: H) -> T
where
    F: FnOnce() -> T + panic::UnwindSafe,
    H: FnOnce(TryCatchError) -> T,
{
    match try_catch(f) {
        Ok(value) => value,
        Err(e) => handler(e),
    }
}

/// Convert panic info to string.
fn panic_to_string(panic_info: &Box<dyn Any + Send>) -> String {
    if let Some(s) = panic_info.downcast_ref::<&str>() {
        s.to_string()
    } else if let Some(s) = panic_info.downcast_ref::<String>() {
        s.clone()
    } else {
        "Unknown panic".to_string()
    }
}

/// Builder for try/catch blocks.
pub struct TryBlock<T, E> {
    result: std::result::Result<T, E>,
}

impl<T, E> TryBlock<T, E> {
    /// Start try block with result.
    pub fn new(result: std::result::Result<T, E>) -> Self {
        Self { result }
    }

    /// Catch specific error type.
    pub fn catch<H>(self, handler: H) -> TryBlock<T, E>
    where
        H: FnOnce(E) -> T,
    {
        match self.result {
            Ok(v) => TryBlock { result: Ok(v) },
            Err(e) => TryBlock {
                result: Ok(handler(e)),
            },
        }
    }

    /// Catch if condition matches.
    pub fn catch_if<P, H>(self, predicate: P, handler: H) -> Self
    where
        P: FnOnce(&E) -> bool,
        H: FnOnce(E) -> T,
    {
        match self.result {
            Ok(v) => TryBlock { result: Ok(v) },
            Err(e) if predicate(&e) => TryBlock {
                result: Ok(handler(e)),
            },
            Err(e) => TryBlock { result: Err(e) },
        }
    }

    /// Get final result.
    pub fn result(self) -> std::result::Result<T, E> {
        self.result
    }

    /// Unwrap with default.
    pub fn unwrap_or(self, default: T) -> T {
        self.result.unwrap_or(default)
    }

    /// Unwrap with handler.
    pub fn unwrap_or_else<F>(self, f: F) -> T
    where
        F: FnOnce(E) -> T,
    {
        self.result.unwrap_or_else(f)
    }
}

/// Start try block.
pub fn try_block<T, E>(result: std::result::Result<T, E>) -> TryBlock<T, E> {
    TryBlock::new(result)
}

/// Guard that runs code on scope exit if there was a panic.
pub struct PanicGuard<F: FnOnce()> {
    handler: Option<F>,
}

impl<F: FnOnce()> PanicGuard<F> {
    /// Create new panic guard.
    pub fn new(handler: F) -> Self {
        Self {
            handler: Some(handler),
        }
    }

    /// Disarm the guard.
    pub fn disarm(&mut self) {
        self.handler = None;
    }
}

impl<F: FnOnce()> Drop for PanicGuard<F> {
    fn drop(&mut self) {
        if std::thread::panicking() {
            if let Some(handler) = self.handler.take() {
                handler();
            }
        }
    }
}

/// Execute with automatic cleanup on panic.
pub fn with_panic_cleanup<T, F, C>(f: F, cleanup: C) -> Result<T>
where
    F: FnOnce() -> T + panic::UnwindSafe,
    C: FnOnce() + panic::UnwindSafe,
{
    let cleanup = AssertUnwindSafe(cleanup);
    let result = panic::catch_unwind(f);

    match result {
        Ok(value) => Ok(value),
        Err(panic_info) => {
            // Run cleanup
            let _ = panic::catch_unwind(cleanup);
            let message = panic_to_string(&panic_info);
            Err(TryCatchError::Panic(message))
        }
    }
}

/// Multi-error catcher.
pub struct MultiCatch<T> {
    string_handler: Option<Box<dyn FnOnce(String) -> T>>,
    default_handler: Option<Box<dyn FnOnce() -> T>>,
}

impl<T> MultiCatch<T> {
    /// Create new multi-catch.
    pub fn new() -> Self {
        Self {
            string_handler: None,
            default_handler: None,
        }
    }

    /// Catch string panics.
    pub fn catch_string<F>(mut self, handler: F) -> Self
    where
        F: FnOnce(String) -> T + 'static,
    {
        self.string_handler = Some(Box::new(handler));
        self
    }

    /// Catch any panic with default handler.
    pub fn catch_any<F>(mut self, handler: F) -> Self
    where
        F: FnOnce() -> T + 'static,
    {
        self.default_handler = Some(Box::new(handler));
        self
    }

    /// Execute with multi-catch.
    pub fn try_<F>(self, f: F) -> std::result::Result<T, Box<dyn Any + Send>>
    where
        F: FnOnce() -> T + panic::UnwindSafe,
    {
        match panic::catch_unwind(f) {
            Ok(value) => Ok(value),
            Err(panic_info) => {
                // Try string handler first
                if let Some(handler) = self.string_handler {
                    let msg = panic_to_string(&panic_info);
                    return Ok(handler(msg));
                }
                // Try default handler
                if let Some(handler) = self.default_handler {
                    return Ok(handler());
                }
                Err(panic_info)
            }
        }
    }
}

impl<T> Default for MultiCatch<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Rethrow panic.
pub fn rethrow(panic_info: Box<dyn Any + Send>) -> ! {
    panic::resume_unwind(panic_info)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_try_catch_success() {
        let result = try_catch(|| 42);
        assert_eq!(result.unwrap(), 42);
    }

    #[test]
    fn test_try_catch_panic() {
        let result: Result<()> = try_catch(|| panic!("test panic"));
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), TryCatchError::Panic(_)));
    }

    #[test]
    fn test_try_catch_with_handler() {
        let result = try_catch_with(|| panic!("test"), |_| 0);
        assert_eq!(result, 0);
    }

    #[test]
    fn test_try_block() {
        let result: std::result::Result<i32, &str> = Err("error");
        let value = try_block(result).catch(|_| 42).unwrap_or(0);
        assert_eq!(value, 42);
    }

    #[test]
    fn test_panic_guard() {
        use std::cell::Cell;

        let called = Cell::new(false);

        let _ = panic::catch_unwind(AssertUnwindSafe(|| {
            let _guard = PanicGuard::new(|| called.set(true));
            panic!("test");
        }));

        assert!(called.get());
    }
}
