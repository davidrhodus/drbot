//! Factory pattern utilities for drbot.
//!
//! This crate provides:
//! - Factory trait
//! - Factory registry
//! - Abstract factory pattern

use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;

/// Factory error types.
#[derive(Error, Debug)]
pub enum FactoryError {
    #[error("Factory not found for type: {0}")]
    NotFound(String),

    #[error("Creation failed: {0}")]
    CreationFailed(String),

    #[error("Invalid configuration")]
    InvalidConfig,
}

/// Result type for factory operations.
pub type Result<T> = std::result::Result<T, FactoryError>;

/// Factory trait for creating objects.
pub trait Factory<T>: Send + Sync {
    /// Create a new instance.
    fn create(&self) -> Result<T>;
}

/// Factory with configuration.
pub trait ConfigurableFactory<T, C>: Send + Sync {
    /// Create with configuration.
    fn create_with_config(&self, config: &C) -> Result<T>;
}

/// Simple function factory.
pub struct FnFactory<T, F: Fn() -> Result<T> + Send + Sync> {
    creator: F,
    _marker: std::marker::PhantomData<T>,
}

impl<T, F: Fn() -> Result<T> + Send + Sync> FnFactory<T, F> {
    /// Create new function factory.
    pub fn new(creator: F) -> Self {
        Self {
            creator,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<T: Send + Sync, F: Fn() -> Result<T> + Send + Sync> Factory<T> for FnFactory<T, F> {
    fn create(&self) -> Result<T> {
        (self.creator)()
    }
}

/// Default factory using Default trait.
pub struct DefaultFactory<T: Default>(std::marker::PhantomData<T>);

impl<T: Default> DefaultFactory<T> {
    /// Create new default factory.
    pub fn new() -> Self {
        Self(std::marker::PhantomData)
    }
}

impl<T: Default> Default for DefaultFactory<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Default + Send + Sync> Factory<T> for DefaultFactory<T> {
    fn create(&self) -> Result<T> {
        Ok(T::default())
    }
}

/// Clone factory using Clone trait.
pub struct CloneFactory<T: Clone + Send + Sync> {
    prototype: T,
}

impl<T: Clone + Send + Sync> CloneFactory<T> {
    /// Create new clone factory.
    pub fn new(prototype: T) -> Self {
        Self { prototype }
    }
}

impl<T: Clone + Send + Sync> Factory<T> for CloneFactory<T> {
    fn create(&self) -> Result<T> {
        Ok(self.prototype.clone())
    }
}

/// Factory registry for dynamic factory lookup.
pub struct FactoryRegistry<T> {
    factories: HashMap<String, Arc<dyn Factory<T>>>,
}

impl<T> FactoryRegistry<T> {
    /// Create new registry.
    pub fn new() -> Self {
        Self {
            factories: HashMap::new(),
        }
    }

    /// Register a factory.
    pub fn register(&mut self, name: impl Into<String>, factory: Arc<dyn Factory<T>>) {
        self.factories.insert(name.into(), factory);
    }

    /// Get factory by name.
    pub fn get(&self, name: &str) -> Option<&Arc<dyn Factory<T>>> {
        self.factories.get(name)
    }

    /// Create instance using named factory.
    pub fn create(&self, name: &str) -> Result<T> {
        let factory = self
            .factories
            .get(name)
            .ok_or_else(|| FactoryError::NotFound(name.to_string()))?;
        factory.create()
    }

    /// List registered factory names.
    pub fn names(&self) -> Vec<&str> {
        self.factories.keys().map(|s| s.as_str()).collect()
    }
}

impl<T> Default for FactoryRegistry<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Abstract factory for creating families of objects.
pub trait AbstractFactory: Send + Sync {
    /// Product A type.
    type ProductA;
    /// Product B type.
    type ProductB;

    /// Create product A.
    fn create_product_a(&self) -> Self::ProductA;

    /// Create product B.
    fn create_product_b(&self) -> Self::ProductB;
}

/// Singleton factory (creates once, returns same instance).
pub struct SingletonFactory<T: Clone + Send + Sync> {
    instance: std::sync::RwLock<Option<T>>,
    creator: Box<dyn Fn() -> Result<T> + Send + Sync>,
}

impl<T: Clone + Send + Sync> SingletonFactory<T> {
    /// Create new singleton factory.
    pub fn new<F>(creator: F) -> Self
    where
        F: Fn() -> Result<T> + Send + Sync + 'static,
    {
        Self {
            instance: std::sync::RwLock::new(None),
            creator: Box::new(creator),
        }
    }

    /// Get or create the singleton instance.
    pub fn get_or_create(&self) -> Result<T> {
        // Check if already created
        {
            let read = self.instance.read().unwrap();
            if let Some(ref instance) = *read {
                return Ok(instance.clone());
            }
        }

        // Create new instance
        let mut write = self.instance.write().unwrap();
        if write.is_none() {
            let instance = (self.creator)()?;
            *write = Some(instance);
        }
        Ok(write.as_ref().unwrap().clone())
    }
}

impl<T: Clone + Send + Sync> Factory<T> for SingletonFactory<T> {
    fn create(&self) -> Result<T> {
        self.get_or_create()
    }
}

/// Lazy factory (delays creation until first use).
pub struct LazyFactory<T> {
    creator: Box<dyn Fn() -> Result<T> + Send + Sync>,
}

impl<T> LazyFactory<T> {
    /// Create new lazy factory.
    pub fn new<F>(creator: F) -> Self
    where
        F: Fn() -> Result<T> + Send + Sync + 'static,
    {
        Self {
            creator: Box::new(creator),
        }
    }
}

impl<T> Factory<T> for LazyFactory<T> {
    fn create(&self) -> Result<T> {
        (self.creator)()
    }
}

/// Helper to create factories.
pub fn factory<T, F>(creator: F) -> Arc<dyn Factory<T>>
where
    T: Send + Sync + 'static,
    F: Fn() -> Result<T> + Send + Sync + 'static,
{
    Arc::new(FnFactory::new(creator))
}

/// Helper to create default factories.
pub fn default_factory<T>() -> Arc<dyn Factory<T>>
where
    T: Default + Send + Sync + 'static,
{
    Arc::new(DefaultFactory::<T>::new())
}

/// Helper to create clone factories.
pub fn clone_factory<T>(prototype: T) -> Arc<dyn Factory<T>>
where
    T: Clone + Send + Sync + 'static,
{
    Arc::new(CloneFactory::new(prototype))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fn_factory() {
        let factory = FnFactory::new(|| Ok(42));
        assert_eq!(factory.create().unwrap(), 42);
    }

    #[test]
    fn test_default_factory() {
        let factory = DefaultFactory::<String>::new();
        assert_eq!(factory.create().unwrap(), String::new());
    }

    #[test]
    fn test_clone_factory() {
        let factory = CloneFactory::new(vec![1, 2, 3]);
        let result = factory.create().unwrap();
        assert_eq!(result, vec![1, 2, 3]);
    }

    #[test]
    fn test_registry() {
        let mut registry = FactoryRegistry::new();
        registry.register("answer", factory(|| Ok(42)));
        registry.register("zero", factory(|| Ok(0)));

        assert_eq!(registry.create("answer").unwrap(), 42);
        assert_eq!(registry.create("zero").unwrap(), 0);
        assert!(registry.create("missing").is_err());
    }

    #[test]
    fn test_singleton_factory() {
        use std::sync::atomic::{AtomicI32, Ordering};

        static COUNTER: AtomicI32 = AtomicI32::new(0);

        let factory = SingletonFactory::new(|| {
            let value = COUNTER.fetch_add(1, Ordering::SeqCst);
            Ok(value)
        });

        let a = factory.create().unwrap();
        let b = factory.create().unwrap();
        let c = factory.create().unwrap();

        assert_eq!(a, b);
        assert_eq!(b, c);
    }
}
