//! Error chaining and wrapping for drbot.
//!
//! This crate provides:
//! - Error chaining with context
//! - Error wrapping
//! - Error cause iteration

use std::error::Error as StdError;
use std::fmt;
use thiserror::Error;

/// Chained error with context.
#[derive(Debug)]
pub struct ChainedError {
    message: String,
    source: Option<Box<dyn StdError + Send + Sync>>,
}

impl ChainedError {
    /// Create new chained error.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            source: None,
        }
    }

    /// Create chained error with source.
    pub fn with_source<E: StdError + Send + Sync + 'static>(
        message: impl Into<String>,
        source: E,
    ) -> Self {
        Self {
            message: message.into(),
            source: Some(Box::new(source)),
        }
    }

    /// Add context message.
    pub fn context(self, message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            source: Some(Box::new(self)),
        }
    }

    /// Get error message.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Iterate over error chain.
    pub fn chain(&self) -> ErrorChainIter<'_> {
        ErrorChainIter {
            current: Some(self as &dyn StdError),
        }
    }
}

impl fmt::Display for ChainedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl StdError for ChainedError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        self.source
            .as_ref()
            .map(|e| e.as_ref() as &(dyn StdError + 'static))
    }
}

/// Iterator over error chain.
pub struct ErrorChainIter<'a> {
    current: Option<&'a (dyn StdError + 'a)>,
}

impl<'a> Iterator for ErrorChainIter<'a> {
    type Item = &'a (dyn StdError + 'a);

    fn next(&mut self) -> Option<Self::Item> {
        let current = self.current?;
        self.current = current.source();
        Some(current)
    }
}

/// Extension trait for adding context to errors.
pub trait ErrorContext<T, E> {
    /// Add context to error.
    fn context(self, message: impl Into<String>) -> Result<T, ChainedError>;

    /// Add lazy context to error.
    fn with_context<F>(self, f: F) -> Result<T, ChainedError>
    where
        F: FnOnce() -> String;
}

impl<T, E: StdError + Send + Sync + 'static> ErrorContext<T, E> for Result<T, E> {
    fn context(self, message: impl Into<String>) -> Result<T, ChainedError> {
        self.map_err(|e| ChainedError::with_source(message, e))
    }

    fn with_context<F>(self, f: F) -> Result<T, ChainedError>
    where
        F: FnOnce() -> String,
    {
        self.map_err(|e| ChainedError::with_source(f(), e))
    }
}

/// Wrapper error that preserves the original error type.
#[derive(Debug)]
pub struct WrappedError<E> {
    context: String,
    inner: E,
}

impl<E> WrappedError<E> {
    /// Create new wrapped error.
    pub fn new(context: impl Into<String>, inner: E) -> Self {
        Self {
            context: context.into(),
            inner,
        }
    }

    /// Get context message.
    pub fn context(&self) -> &str {
        &self.context
    }

    /// Get inner error.
    pub fn inner(&self) -> &E {
        &self.inner
    }

    /// Unwrap inner error.
    pub fn into_inner(self) -> E {
        self.inner
    }
}

impl<E: fmt::Display> fmt::Display for WrappedError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.context, self.inner)
    }
}

impl<E: StdError + 'static> StdError for WrappedError<E> {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        Some(&self.inner)
    }
}

/// Extension trait for wrapping errors.
pub trait WrapError<T, E> {
    /// Wrap error with context.
    fn wrap(self, context: impl Into<String>) -> Result<T, WrappedError<E>>;
}

impl<T, E> WrapError<T, E> for Result<T, E> {
    fn wrap(self, context: impl Into<String>) -> Result<T, WrappedError<E>> {
        self.map_err(|e| WrappedError::new(context, e))
    }
}

/// Error with backtrace information.
#[derive(Debug)]
pub struct TracedError {
    message: String,
    location: Option<&'static std::panic::Location<'static>>,
    source: Option<Box<dyn StdError + Send + Sync>>,
}

impl TracedError {
    /// Create new traced error.
    #[track_caller]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            location: Some(std::panic::Location::caller()),
            source: None,
        }
    }

    /// Create traced error with source.
    #[track_caller]
    pub fn with_source<E: StdError + Send + Sync + 'static>(
        message: impl Into<String>,
        source: E,
    ) -> Self {
        Self {
            message: message.into(),
            location: Some(std::panic::Location::caller()),
            source: Some(Box::new(source)),
        }
    }

    /// Get location where error was created.
    pub fn location(&self) -> Option<&std::panic::Location<'static>> {
        self.location
    }
}

impl fmt::Display for TracedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(loc) = self.location {
            write!(
                f,
                "{} at {}:{}:{}",
                self.message,
                loc.file(),
                loc.line(),
                loc.column()
            )
        } else {
            write!(f, "{}", self.message)
        }
    }
}

impl StdError for TracedError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        self.source
            .as_ref()
            .map(|e| e.as_ref() as &(dyn StdError + 'static))
    }
}

/// Format error chain as string.
pub fn format_error_chain<E: StdError>(error: &E) -> String {
    let mut chain = Vec::new();
    let mut current: Option<&dyn StdError> = Some(error);

    while let Some(err) = current {
        chain.push(err.to_string());
        current = err.source();
    }

    chain.join("\n  Caused by: ")
}

/// Count errors in chain.
pub fn error_chain_length<E: StdError>(error: &E) -> usize {
    let mut count = 0;
    let mut current: Option<&dyn StdError> = Some(error);

    while let Some(err) = current {
        count += 1;
        current = err.source();
    }

    count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Error)]
    #[error("root error")]
    struct RootError;

    #[test]
    fn test_chained_error() {
        let err = ChainedError::new("top level").context("middle level");

        assert_eq!(err.message(), "middle level");
        assert!(err.source().is_some());
    }

    #[test]
    fn test_context_extension() {
        let result: Result<(), RootError> = Err(RootError);
        let chained = result.context("failed to do something");

        assert!(chained.is_err());
        let err = chained.unwrap_err();
        assert_eq!(err.message(), "failed to do something");
    }

    #[test]
    fn test_error_chain() {
        let err = ChainedError::with_source("level 2", RootError).context("level 1");

        let chain: Vec<_> = err.chain().map(|e| e.to_string()).collect();
        assert_eq!(chain.len(), 3);
    }

    #[test]
    fn test_wrapped_error() {
        let result: Result<(), RootError> = Err(RootError);
        let wrapped = result.wrap("operation failed");

        let err = wrapped.unwrap_err();
        assert_eq!(err.context(), "operation failed");
    }

    #[test]
    fn test_traced_error() {
        let err = TracedError::new("something went wrong");
        assert!(err.location().is_some());
        assert!(err.to_string().contains("something went wrong"));
    }
}
