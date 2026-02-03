//! Decorator pattern utilities for drbot.
//!
//! This crate provides:
//! - Decorator trait for wrapping behavior
//! - Layered decorators
//! - Function decorators

use std::sync::Arc;
use thiserror::Error;

/// Decorator error types.
#[derive(Error, Debug)]
pub enum DecoratorError {
    #[error("Decoration failed: {0}")]
    Failed(String),

    #[error("Invalid decorator")]
    Invalid,
}

/// Result type for decorator operations.
pub type Result<T> = std::result::Result<T, DecoratorError>;

/// Decorator trait for wrapping components.
pub trait Decorator<T>: Send + Sync {
    /// Decorate the input value.
    fn decorate(&self, value: T) -> T;
}

/// Identity decorator (no-op).
pub struct IdentityDecorator;

impl<T> Decorator<T> for IdentityDecorator {
    fn decorate(&self, value: T) -> T {
        value
    }
}

/// Function-based decorator.
pub struct FnDecorator<T, F: Fn(T) -> T + Send + Sync> {
    func: F,
    _marker: std::marker::PhantomData<T>,
}

impl<T, F: Fn(T) -> T + Send + Sync> FnDecorator<T, F> {
    /// Create new function decorator.
    pub fn new(func: F) -> Self {
        Self {
            func,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<T: Send + Sync, F: Fn(T) -> T + Send + Sync> Decorator<T> for FnDecorator<T, F> {
    fn decorate(&self, value: T) -> T {
        (self.func)(value)
    }
}

/// Chained decorator that applies multiple decorators.
pub struct ChainedDecorator<T> {
    decorators: Vec<Arc<dyn Decorator<T>>>,
}

impl<T> ChainedDecorator<T> {
    /// Create new chained decorator.
    pub fn new() -> Self {
        Self {
            decorators: Vec::new(),
        }
    }

    /// Add decorator to chain.
    pub fn add(&mut self, decorator: Arc<dyn Decorator<T>>) {
        self.decorators.push(decorator);
    }

    /// Add decorator to chain (builder pattern).
    pub fn with(mut self, decorator: Arc<dyn Decorator<T>>) -> Self {
        self.add(decorator);
        self
    }
}

impl<T> Default for ChainedDecorator<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Decorator<T> for ChainedDecorator<T> {
    fn decorate(&self, value: T) -> T {
        self.decorators.iter().fold(value, |v, d| d.decorate(v))
    }
}

/// Conditional decorator that applies only if condition is met.
pub struct ConditionalDecorator<T, P: Fn(&T) -> bool + Send + Sync> {
    inner: Arc<dyn Decorator<T>>,
    predicate: P,
}

impl<T, P: Fn(&T) -> bool + Send + Sync> ConditionalDecorator<T, P> {
    /// Create new conditional decorator.
    pub fn new(inner: Arc<dyn Decorator<T>>, predicate: P) -> Self {
        Self { inner, predicate }
    }
}

impl<T, P: Fn(&T) -> bool + Send + Sync> Decorator<T> for ConditionalDecorator<T, P> {
    fn decorate(&self, value: T) -> T {
        if (self.predicate)(&value) {
            self.inner.decorate(value)
        } else {
            value
        }
    }
}

/// Logging decorator that wraps decoration with logging.
pub struct LoggingDecorator<T, L: Fn(&str) + Send + Sync> {
    inner: Arc<dyn Decorator<T>>,
    name: String,
    logger: L,
}

impl<T, L: Fn(&str) + Send + Sync> LoggingDecorator<T, L> {
    /// Create new logging decorator.
    pub fn new(inner: Arc<dyn Decorator<T>>, name: impl Into<String>, logger: L) -> Self {
        Self {
            inner,
            name: name.into(),
            logger,
        }
    }
}

impl<T, L: Fn(&str) + Send + Sync> Decorator<T> for LoggingDecorator<T, L> {
    fn decorate(&self, value: T) -> T {
        (self.logger)(&format!("Before {}", self.name));
        let result = self.inner.decorate(value);
        (self.logger)(&format!("After {}", self.name));
        result
    }
}

/// Caching decorator that caches results.
pub struct CachingDecorator<T: Clone + Send + Sync> {
    inner: Arc<dyn Decorator<T>>,
    cache: std::sync::RwLock<Option<T>>,
}

impl<T: Clone + Send + Sync> CachingDecorator<T> {
    /// Create new caching decorator.
    pub fn new(inner: Arc<dyn Decorator<T>>) -> Self {
        Self {
            inner,
            cache: std::sync::RwLock::new(None),
        }
    }

    /// Clear cache.
    pub fn clear(&self) {
        let mut cache = self.cache.write().unwrap();
        *cache = None;
    }
}

impl<T: Clone + Send + Sync> Decorator<T> for CachingDecorator<T> {
    fn decorate(&self, value: T) -> T {
        // Check cache
        {
            let cache = self.cache.read().unwrap();
            if let Some(ref cached) = *cache {
                return cached.clone();
            }
        }

        // Compute and cache
        let result = self.inner.decorate(value);
        {
            let mut cache = self.cache.write().unwrap();
            *cache = Some(result.clone());
        }
        result
    }
}

/// Helper to create decorators.
pub fn decorator<T, F>(func: F) -> Arc<dyn Decorator<T>>
where
    T: Send + Sync + 'static,
    F: Fn(T) -> T + Send + Sync + 'static,
{
    Arc::new(FnDecorator::new(func))
}

/// Helper to chain decorators.
pub fn chain<T: 'static>(decorators: Vec<Arc<dyn Decorator<T>>>) -> Arc<dyn Decorator<T>> {
    let mut chained = ChainedDecorator::new();
    for d in decorators {
        chained.add(d);
    }
    Arc::new(chained)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identity_decorator() {
        let decorator = IdentityDecorator;
        assert_eq!(decorator.decorate(42), 42);
    }

    #[test]
    fn test_fn_decorator() {
        let decorator = FnDecorator::new(|x: i32| x * 2);
        assert_eq!(decorator.decorate(21), 42);
    }

    #[test]
    fn test_chained_decorator() {
        let d1 = decorator(|x: i32| x + 1);
        let d2 = decorator(|x: i32| x * 2);

        let chained = ChainedDecorator::new().with(d1).with(d2);

        assert_eq!(chained.decorate(10), 22); // (10 + 1) * 2
    }

    #[test]
    fn test_conditional_decorator() {
        let inner = decorator(|x: i32| x * 2);
        let cond = ConditionalDecorator::new(inner, |x: &i32| *x > 10);

        assert_eq!(cond.decorate(5), 5); // Not decorated
        assert_eq!(cond.decorate(15), 30); // Decorated
    }
}
