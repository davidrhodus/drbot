//! Result utilities for drbot.
//!
//! This crate provides:
//! - Result extension methods
//! - Result combinators
//! - Error handling utilities

use thiserror::Error;

/// Result-related errors.
#[derive(Error, Debug, Clone)]
pub enum ResultError {
    #[error("Expected Ok but got Err")]
    ExpectedOk,

    #[error("Expected Err but got Ok")]
    ExpectedErr,

    #[error("All operations failed")]
    AllFailed,
}

/// Result extension trait.
pub trait ResultExt<T, E> {
    /// Tap into Ok value.
    fn tap_ok<F: FnOnce(&T)>(self, f: F) -> Self;

    /// Tap into Err value.
    fn tap_err<F: FnOnce(&E)>(self, f: F) -> Self;

    /// Map error to new type.
    fn map_err_to<E2>(self, err: E2) -> Result<T, E2>;

    /// Map error with context.
    fn context<C: std::fmt::Display>(self, context: C) -> Result<T, String>
    where
        E: std::fmt::Display;

    /// Map error with lazy context.
    fn with_context<C: std::fmt::Display, F: FnOnce() -> C>(self, f: F) -> Result<T, String>
    where
        E: std::fmt::Display;

    /// Convert to Option, discarding error.
    fn ok_or_log<F: FnOnce(&E)>(self, log: F) -> Option<T>;

    /// Flatten nested Result.
    fn flatten_result(self) -> Result<T, E>
    where
        T: Into<Result<T, E>>;

    /// Check if Ok and matches predicate.
    fn is_ok_and<F: FnOnce(&T) -> bool>(&self, f: F) -> bool;

    /// Check if Err and matches predicate.
    fn is_err_and<F: FnOnce(&E) -> bool>(&self, f: F) -> bool;
}

impl<T, E> ResultExt<T, E> for Result<T, E> {
    fn tap_ok<F: FnOnce(&T)>(self, f: F) -> Self {
        if let Ok(ref v) = self {
            f(v);
        }
        self
    }

    fn tap_err<F: FnOnce(&E)>(self, f: F) -> Self {
        if let Err(ref e) = self {
            f(e);
        }
        self
    }

    fn map_err_to<E2>(self, err: E2) -> Result<T, E2> {
        self.map_err(|_| err)
    }

    fn context<C: std::fmt::Display>(self, context: C) -> Result<T, String>
    where
        E: std::fmt::Display,
    {
        self.map_err(|e| format!("{}: {}", context, e))
    }

    fn with_context<C: std::fmt::Display, F: FnOnce() -> C>(self, f: F) -> Result<T, String>
    where
        E: std::fmt::Display,
    {
        self.map_err(|e| format!("{}: {}", f(), e))
    }

    fn ok_or_log<F: FnOnce(&E)>(self, log: F) -> Option<T> {
        match self {
            Ok(v) => Some(v),
            Err(e) => {
                log(&e);
                None
            }
        }
    }

    fn flatten_result(self) -> Result<T, E>
    where
        T: Into<Result<T, E>>,
    {
        self.and_then(|inner| inner.into())
    }

    fn is_ok_and<F: FnOnce(&T) -> bool>(&self, f: F) -> bool {
        match self {
            Ok(v) => f(v),
            Err(_) => false,
        }
    }

    fn is_err_and<F: FnOnce(&E) -> bool>(&self, f: F) -> bool {
        match self {
            Ok(_) => false,
            Err(e) => f(e),
        }
    }
}

/// Combine multiple Results.
pub struct ResultCombinator;

impl ResultCombinator {
    /// All must be Ok.
    pub fn all<T, E, I>(results: I) -> Result<Vec<T>, E>
    where
        I: IntoIterator<Item = Result<T, E>>,
    {
        results.into_iter().collect()
    }

    /// Partition into Oks and Errs.
    pub fn partition<T, E, I>(results: I) -> (Vec<T>, Vec<E>)
    where
        I: IntoIterator<Item = Result<T, E>>,
    {
        let mut oks = Vec::new();
        let mut errs = Vec::new();

        for result in results {
            match result {
                Ok(v) => oks.push(v),
                Err(e) => errs.push(e),
            }
        }

        (oks, errs)
    }

    /// First Ok or all errors.
    pub fn first_ok<T, E, I>(results: I) -> Result<T, Vec<E>>
    where
        I: IntoIterator<Item = Result<T, E>>,
    {
        let mut errors = Vec::new();

        for result in results {
            match result {
                Ok(v) => return Ok(v),
                Err(e) => errors.push(e),
            }
        }

        Err(errors)
    }

    /// Get all Ok values, ignoring errors.
    pub fn filter_ok<T, E, I>(results: I) -> Vec<T>
    where
        I: IntoIterator<Item = Result<T, E>>,
    {
        results.into_iter().filter_map(|r| r.ok()).collect()
    }

    /// Count Ok values.
    pub fn count_ok<T, E, I>(results: I) -> usize
    where
        I: IntoIterator<Item = Result<T, E>>,
    {
        results.into_iter().filter(|r| r.is_ok()).count()
    }

    /// Count Err values.
    pub fn count_err<T, E, I>(results: I) -> usize
    where
        I: IntoIterator<Item = Result<T, E>>,
    {
        results.into_iter().filter(|r| r.is_err()).count()
    }
}

/// Try operations with retry.
pub struct Retry {
    max_attempts: usize,
}

impl Retry {
    /// Create new retry with max attempts.
    pub fn new(max_attempts: usize) -> Self {
        Self { max_attempts }
    }

    /// Retry operation until success or max attempts.
    pub fn run<T, E, F>(&self, mut f: F) -> Result<T, E>
    where
        F: FnMut() -> Result<T, E>,
    {
        let mut last_error = None;

        for _ in 0..self.max_attempts {
            match f() {
                Ok(v) => return Ok(v),
                Err(e) => last_error = Some(e),
            }
        }

        Err(last_error.unwrap())
    }

    /// Retry with exponential backoff (sync).
    pub fn run_with_backoff<T, E, F>(&self, mut f: F, base_delay_ms: u64) -> Result<T, E>
    where
        F: FnMut() -> Result<T, E>,
    {
        let mut last_error = None;
        let mut delay = base_delay_ms;

        for _ in 0..self.max_attempts {
            match f() {
                Ok(v) => return Ok(v),
                Err(e) => {
                    last_error = Some(e);
                    std::thread::sleep(std::time::Duration::from_millis(delay));
                    delay *= 2;
                }
            }
        }

        Err(last_error.unwrap())
    }
}

/// Error accumulator for collecting multiple errors.
pub struct ErrorAccumulator<E> {
    errors: Vec<E>,
}

impl<E> ErrorAccumulator<E> {
    /// Create new accumulator.
    pub fn new() -> Self {
        Self { errors: Vec::new() }
    }

    /// Add error.
    pub fn add(&mut self, error: E) {
        self.errors.push(error);
    }

    /// Add error if Result is Err.
    pub fn capture<T>(&mut self, result: Result<T, E>) -> Option<T> {
        match result {
            Ok(v) => Some(v),
            Err(e) => {
                self.errors.push(e);
                None
            }
        }
    }

    /// Check if any errors.
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    /// Get errors.
    pub fn errors(&self) -> &[E] {
        &self.errors
    }

    /// Into errors.
    pub fn into_errors(self) -> Vec<E> {
        self.errors
    }

    /// Check result - Ok if no errors, Err with errors otherwise.
    pub fn result<T>(self, value: T) -> Result<T, Vec<E>> {
        if self.errors.is_empty() {
            Ok(value)
        } else {
            Err(self.errors)
        }
    }
}

impl<E> Default for ErrorAccumulator<E> {
    fn default() -> Self {
        Self::new()
    }
}

/// Convert various types to Result.
pub struct ToResult;

impl ToResult {
    /// Convert bool to Result.
    pub fn from_bool<E>(b: bool, err: E) -> Result<(), E> {
        if b {
            Ok(())
        } else {
            Err(err)
        }
    }

    /// Convert Option to Result.
    pub fn from_option<T, E>(opt: Option<T>, err: E) -> Result<T, E> {
        opt.ok_or(err)
    }

    /// Convert Option to Result with lazy error.
    pub fn from_option_with<T, E, F: FnOnce() -> E>(opt: Option<T>, f: F) -> Result<T, E> {
        opt.ok_or_else(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tap_ok() {
        let mut tapped = false;
        let _ = Ok::<_, &str>(5).tap_ok(|_| tapped = true);
        assert!(tapped);

        tapped = false;
        let _ = Err::<i32, _>("error").tap_ok(|_| tapped = true);
        assert!(!tapped);
    }

    #[test]
    fn test_context() {
        let result: Result<i32, &str> = Err("original");
        let with_context = result.context("context");
        assert_eq!(with_context.unwrap_err(), "context: original");
    }

    #[test]
    fn test_combinator_all() {
        let results: Vec<Result<i32, &str>> = vec![Ok(1), Ok(2), Ok(3)];
        assert_eq!(ResultCombinator::all(results), Ok(vec![1, 2, 3]));

        let with_err: Vec<Result<i32, &str>> = vec![Ok(1), Err("error"), Ok(3)];
        assert!(ResultCombinator::all(with_err).is_err());
    }

    #[test]
    fn test_partition() {
        let results: Vec<Result<i32, &str>> = vec![Ok(1), Err("a"), Ok(2), Err("b")];
        let (oks, errs) = ResultCombinator::partition(results);
        assert_eq!(oks, vec![1, 2]);
        assert_eq!(errs, vec!["a", "b"]);
    }

    #[test]
    fn test_first_ok() {
        let results: Vec<Result<i32, &str>> = vec![Err("a"), Ok(2), Ok(3)];
        assert_eq!(ResultCombinator::first_ok(results), Ok(2));

        let all_err: Vec<Result<i32, &str>> = vec![Err("a"), Err("b")];
        assert_eq!(ResultCombinator::first_ok(all_err), Err(vec!["a", "b"]));
    }

    #[test]
    fn test_retry() {
        let retry = Retry::new(3);
        let mut attempts = 0;

        let result = retry.run(|| {
            attempts += 1;
            if attempts < 3 {
                Err("not yet")
            } else {
                Ok("success")
            }
        });

        assert_eq!(result, Ok("success"));
        assert_eq!(attempts, 3);
    }

    #[test]
    fn test_error_accumulator() {
        let mut acc = ErrorAccumulator::new();

        acc.capture::<i32>(Ok(1));
        acc.capture::<i32>(Err("error1"));
        acc.capture::<i32>(Err("error2"));

        assert!(acc.has_errors());
        assert_eq!(acc.errors().len(), 2);
    }

    #[test]
    fn test_to_result() {
        assert!(ToResult::from_bool(true, "error").is_ok());
        assert!(ToResult::from_bool(false, "error").is_err());

        assert_eq!(ToResult::from_option(Some(5), "error"), Ok(5));
        assert_eq!(ToResult::from_option::<i32, _>(None, "error"), Err("error"));
    }
}
