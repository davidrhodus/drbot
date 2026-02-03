//! Adapter pattern utilities for drbot.
//!
//! This crate provides:
//! - Adapter trait for interface conversion
//! - Object adapters
//! - Class adapters (via composition)

use std::sync::Arc;
use thiserror::Error;

/// Adapter error types.
#[derive(Error, Debug)]
pub enum AdapterError {
    #[error("Adaptation failed: {0}")]
    Failed(String),

    #[error("Incompatible types")]
    Incompatible,
}

/// Result type for adapter operations.
pub type Result<T> = std::result::Result<T, AdapterError>;

/// Adapter trait for converting between interfaces.
pub trait Adapter<From, To>: Send + Sync {
    /// Adapt from source to target type.
    fn adapt(&self, source: From) -> Result<To>;
}

/// Identity adapter (no conversion).
pub struct IdentityAdapter<T>(std::marker::PhantomData<T>);

impl<T> IdentityAdapter<T> {
    /// Create new identity adapter.
    pub fn new() -> Self {
        Self(std::marker::PhantomData)
    }
}

impl<T> Default for IdentityAdapter<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Send + Sync> Adapter<T, T> for IdentityAdapter<T> {
    fn adapt(&self, source: T) -> Result<T> {
        Ok(source)
    }
}

/// Function-based adapter.
pub struct FnAdapter<From, To, F: Fn(From) -> Result<To> + Send + Sync> {
    func: F,
    _marker: std::marker::PhantomData<(From, To)>,
}

impl<From, To, F: Fn(From) -> Result<To> + Send + Sync> FnAdapter<From, To, F> {
    /// Create new function adapter.
    pub fn new(func: F) -> Self {
        Self {
            func,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<From: Send + Sync, To: Send + Sync, F: Fn(From) -> Result<To> + Send + Sync> Adapter<From, To>
    for FnAdapter<From, To, F>
{
    fn adapt(&self, source: From) -> Result<To> {
        (self.func)(source)
    }
}

/// Infallible function adapter.
pub struct InfallibleAdapter<From, To, F: Fn(From) -> To + Send + Sync> {
    func: F,
    _marker: std::marker::PhantomData<(From, To)>,
}

impl<From, To, F: Fn(From) -> To + Send + Sync> InfallibleAdapter<From, To, F> {
    /// Create new infallible adapter.
    pub fn new(func: F) -> Self {
        Self {
            func,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<From: Send + Sync, To: Send + Sync, F: Fn(From) -> To + Send + Sync> Adapter<From, To>
    for InfallibleAdapter<From, To, F>
{
    fn adapt(&self, source: From) -> Result<To> {
        Ok((self.func)(source))
    }
}

/// Chained adapter that composes two adapters.
pub struct ChainedAdapter<A, B, C> {
    first: Arc<dyn Adapter<A, B>>,
    second: Arc<dyn Adapter<B, C>>,
}

impl<A, B, C> ChainedAdapter<A, B, C> {
    /// Create new chained adapter.
    pub fn new(first: Arc<dyn Adapter<A, B>>, second: Arc<dyn Adapter<B, C>>) -> Self {
        Self { first, second }
    }
}

impl<A: 'static, B: 'static, C: 'static> Adapter<A, C> for ChainedAdapter<A, B, C> {
    fn adapt(&self, source: A) -> Result<C> {
        let intermediate = self.first.adapt(source)?;
        self.second.adapt(intermediate)
    }
}

/// Bidirectional adapter.
pub struct BidirectionalAdapter<A, B> {
    forward: Arc<dyn Adapter<A, B>>,
    backward: Arc<dyn Adapter<B, A>>,
}

impl<A, B> BidirectionalAdapter<A, B> {
    /// Create new bidirectional adapter.
    pub fn new(forward: Arc<dyn Adapter<A, B>>, backward: Arc<dyn Adapter<B, A>>) -> Self {
        Self { forward, backward }
    }

    /// Adapt forward.
    pub fn adapt_forward(&self, source: A) -> Result<B> {
        self.forward.adapt(source)
    }

    /// Adapt backward.
    pub fn adapt_backward(&self, source: B) -> Result<A> {
        self.backward.adapt(source)
    }
}

/// Adapter registry for multiple adapters.
pub struct AdapterRegistry<From, To> {
    adapters: std::collections::HashMap<String, Arc<dyn Adapter<From, To>>>,
}

impl<From, To> AdapterRegistry<From, To> {
    /// Create new registry.
    pub fn new() -> Self {
        Self {
            adapters: std::collections::HashMap::new(),
        }
    }

    /// Register adapter.
    pub fn register(&mut self, name: impl Into<String>, adapter: Arc<dyn Adapter<From, To>>) {
        self.adapters.insert(name.into(), adapter);
    }

    /// Get adapter by name.
    pub fn get(&self, name: &str) -> Option<&Arc<dyn Adapter<From, To>>> {
        self.adapters.get(name)
    }

    /// Adapt using named adapter.
    pub fn adapt(&self, name: &str, source: From) -> Result<To> {
        let adapter = self
            .adapters
            .get(name)
            .ok_or_else(|| AdapterError::Failed(format!("Adapter '{}' not found", name)))?;
        adapter.adapt(source)
    }

    /// List adapter names.
    pub fn names(&self) -> Vec<&str> {
        self.adapters.keys().map(|s| s.as_str()).collect()
    }
}

impl<From, To> Default for AdapterRegistry<From, To> {
    fn default() -> Self {
        Self::new()
    }
}

/// Object adapter wrapping an adaptee.
pub struct ObjectAdapter<Adaptee, Target> {
    adaptee: Adaptee,
    adapt_fn: Box<dyn Fn(&Adaptee) -> Target + Send + Sync>,
}

impl<Adaptee, Target> ObjectAdapter<Adaptee, Target> {
    /// Create new object adapter.
    pub fn new<F>(adaptee: Adaptee, adapt_fn: F) -> Self
    where
        F: Fn(&Adaptee) -> Target + Send + Sync + 'static,
    {
        Self {
            adaptee,
            adapt_fn: Box::new(adapt_fn),
        }
    }

    /// Get adapted value.
    pub fn get(&self) -> Target {
        (self.adapt_fn)(&self.adaptee)
    }

    /// Get reference to adaptee.
    pub fn adaptee(&self) -> &Adaptee {
        &self.adaptee
    }
}

/// Helper to create function adapter.
pub fn adapter<From: Send + Sync + 'static, To: Send + Sync + 'static, F>(
    func: F,
) -> Arc<dyn Adapter<From, To>>
where
    F: Fn(From) -> Result<To> + Send + Sync + 'static,
{
    Arc::new(FnAdapter::new(func))
}

/// Helper to create infallible adapter.
pub fn infallible_adapter<From: Send + Sync + 'static, To: Send + Sync + 'static, F>(
    func: F,
) -> Arc<dyn Adapter<From, To>>
where
    F: Fn(From) -> To + Send + Sync + 'static,
{
    Arc::new(InfallibleAdapter::new(func))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identity_adapter() {
        let adapter = IdentityAdapter::<i32>::new();
        assert_eq!(adapter.adapt(42).unwrap(), 42);
    }

    #[test]
    fn test_fn_adapter() {
        let adapter = FnAdapter::new(|x: i32| Ok(x.to_string()));
        assert_eq!(adapter.adapt(42).unwrap(), "42");
    }

    #[test]
    fn test_infallible_adapter() {
        let adapter = InfallibleAdapter::new(|x: i32| x * 2);
        assert_eq!(adapter.adapt(21).unwrap(), 42);
    }

    #[test]
    fn test_chained_adapter() {
        let first: Arc<dyn Adapter<i32, String>> =
            Arc::new(InfallibleAdapter::new(|x: i32| x.to_string()));
        let second: Arc<dyn Adapter<String, usize>> =
            Arc::new(InfallibleAdapter::new(|s: String| s.len()));

        let chained = ChainedAdapter::new(first, second);
        assert_eq!(chained.adapt(123).unwrap(), 3); // "123".len() = 3
    }

    #[test]
    fn test_registry() {
        let mut registry = AdapterRegistry::new();

        registry.register("double", infallible_adapter(|x: i32| x * 2));
        registry.register("negate", infallible_adapter(|x: i32| -x));

        assert_eq!(registry.adapt("double", 21).unwrap(), 42);
        assert_eq!(registry.adapt("negate", 42).unwrap(), -42);
    }

    #[test]
    fn test_object_adapter() {
        struct LegacyApi {
            data: Vec<i32>,
        }

        let adapter = ObjectAdapter::new(
            LegacyApi {
                data: vec![1, 2, 3],
            },
            |api| api.data.iter().sum::<i32>(),
        );

        assert_eq!(adapter.get(), 6);
    }
}
