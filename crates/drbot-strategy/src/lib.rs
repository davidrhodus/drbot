//! Strategy pattern utilities for drbot.
//!
//! This crate provides:
//! - Strategy trait
//! - Strategy context
//! - Strategy selection

use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;

/// Strategy error types.
#[derive(Error, Debug)]
pub enum StrategyError {
    #[error("Strategy not found: {0}")]
    NotFound(String),

    #[error("Execution failed: {0}")]
    ExecutionFailed(String),

    #[error("No strategy set")]
    NoStrategy,
}

/// Result type for strategy operations.
pub type Result<T> = std::result::Result<T, StrategyError>;

/// Strategy trait.
pub trait Strategy<I, O>: Send + Sync {
    /// Execute strategy.
    fn execute(&self, input: I) -> O;
}

/// Function-based strategy.
pub struct FnStrategy<I, O, F: Fn(I) -> O + Send + Sync> {
    func: F,
    _marker: std::marker::PhantomData<(I, O)>,
}

impl<I, O, F: Fn(I) -> O + Send + Sync> FnStrategy<I, O, F> {
    /// Create new function strategy.
    pub fn new(func: F) -> Self {
        Self {
            func,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<I: Send + Sync, O: Send + Sync, F: Fn(I) -> O + Send + Sync> Strategy<I, O>
    for FnStrategy<I, O, F>
{
    fn execute(&self, input: I) -> O {
        (self.func)(input)
    }
}

/// Strategy context that uses a strategy.
pub struct Context<I, O> {
    strategy: Option<Arc<dyn Strategy<I, O>>>,
}

impl<I, O> Context<I, O> {
    /// Create new context.
    pub fn new() -> Self {
        Self { strategy: None }
    }

    /// Create with strategy.
    pub fn with_strategy(strategy: Arc<dyn Strategy<I, O>>) -> Self {
        Self {
            strategy: Some(strategy),
        }
    }

    /// Set strategy.
    pub fn set_strategy(&mut self, strategy: Arc<dyn Strategy<I, O>>) {
        self.strategy = Some(strategy);
    }

    /// Clear strategy.
    pub fn clear_strategy(&mut self) {
        self.strategy = None;
    }

    /// Has strategy.
    pub fn has_strategy(&self) -> bool {
        self.strategy.is_some()
    }

    /// Execute current strategy.
    pub fn execute(&self, input: I) -> Result<O> {
        match &self.strategy {
            Some(s) => Ok(s.execute(input)),
            None => Err(StrategyError::NoStrategy),
        }
    }
}

impl<I, O> Default for Context<I, O> {
    fn default() -> Self {
        Self::new()
    }
}

/// Strategy registry for named strategies.
pub struct StrategyRegistry<I, O> {
    strategies: HashMap<String, Arc<dyn Strategy<I, O>>>,
    default: Option<String>,
}

impl<I, O> StrategyRegistry<I, O> {
    /// Create new registry.
    pub fn new() -> Self {
        Self {
            strategies: HashMap::new(),
            default: None,
        }
    }

    /// Register strategy.
    pub fn register(&mut self, name: impl Into<String>, strategy: Arc<dyn Strategy<I, O>>) {
        self.strategies.insert(name.into(), strategy);
    }

    /// Set default strategy.
    pub fn set_default(&mut self, name: impl Into<String>) {
        self.default = Some(name.into());
    }

    /// Get strategy by name.
    pub fn get(&self, name: &str) -> Option<&Arc<dyn Strategy<I, O>>> {
        self.strategies.get(name)
    }

    /// Get default strategy.
    pub fn get_default(&self) -> Option<&Arc<dyn Strategy<I, O>>> {
        self.default.as_ref().and_then(|n| self.strategies.get(n))
    }

    /// Execute named strategy.
    pub fn execute(&self, name: &str, input: I) -> Result<O> {
        let strategy = self
            .strategies
            .get(name)
            .ok_or_else(|| StrategyError::NotFound(name.to_string()))?;
        Ok(strategy.execute(input))
    }

    /// Execute default strategy.
    pub fn execute_default(&self, input: I) -> Result<O> {
        let strategy = self.get_default().ok_or(StrategyError::NoStrategy)?;
        Ok(strategy.execute(input))
    }

    /// List strategy names.
    pub fn names(&self) -> Vec<&str> {
        self.strategies.keys().map(|s| s.as_str()).collect()
    }
}

impl<I, O> Default for StrategyRegistry<I, O> {
    fn default() -> Self {
        Self::new()
    }
}

/// Strategy selector based on input.
pub struct StrategySelector<I, O> {
    strategies: Vec<(
        Box<dyn Fn(&I) -> bool + Send + Sync>,
        Arc<dyn Strategy<I, O>>,
    )>,
    fallback: Option<Arc<dyn Strategy<I, O>>>,
}

impl<I, O> StrategySelector<I, O> {
    /// Create new selector.
    pub fn new() -> Self {
        Self {
            strategies: Vec::new(),
            fallback: None,
        }
    }

    /// Add strategy with predicate.
    pub fn when<P>(&mut self, predicate: P, strategy: Arc<dyn Strategy<I, O>>)
    where
        P: Fn(&I) -> bool + Send + Sync + 'static,
    {
        self.strategies.push((Box::new(predicate), strategy));
    }

    /// Set fallback strategy.
    pub fn fallback(&mut self, strategy: Arc<dyn Strategy<I, O>>) {
        self.fallback = Some(strategy);
    }

    /// Select and execute strategy.
    pub fn execute(&self, input: I) -> Result<O> {
        for (predicate, strategy) in &self.strategies {
            if predicate(&input) {
                return Ok(strategy.execute(input));
            }
        }

        match &self.fallback {
            Some(s) => Ok(s.execute(input)),
            None => Err(StrategyError::NoStrategy),
        }
    }
}

impl<I, O> Default for StrategySelector<I, O> {
    fn default() -> Self {
        Self::new()
    }
}

/// Composite strategy that chains strategies.
pub struct CompositeStrategy<T> {
    strategies: Vec<Arc<dyn Strategy<T, T>>>,
}

impl<T> CompositeStrategy<T> {
    /// Create new composite strategy.
    pub fn new() -> Self {
        Self {
            strategies: Vec::new(),
        }
    }

    /// Add strategy.
    pub fn add(&mut self, strategy: Arc<dyn Strategy<T, T>>) {
        self.strategies.push(strategy);
    }

    /// Add strategy (builder pattern).
    pub fn with(mut self, strategy: Arc<dyn Strategy<T, T>>) -> Self {
        self.add(strategy);
        self
    }
}

impl<T> Default for CompositeStrategy<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Send + Sync> Strategy<T, T> for CompositeStrategy<T> {
    fn execute(&self, mut input: T) -> T {
        for strategy in &self.strategies {
            input = strategy.execute(input);
        }
        input
    }
}

/// Helper to create strategy.
pub fn strategy<I: Send + Sync + 'static, O: Send + Sync + 'static, F>(
    func: F,
) -> Arc<dyn Strategy<I, O>>
where
    F: Fn(I) -> O + Send + Sync + 'static,
{
    Arc::new(FnStrategy::new(func))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fn_strategy() {
        let s = FnStrategy::new(|x: i32| x * 2);
        assert_eq!(s.execute(21), 42);
    }

    #[test]
    fn test_context() {
        let mut context = Context::new();
        context.set_strategy(strategy(|x: i32| x * 2));

        assert_eq!(context.execute(21).unwrap(), 42);
    }

    #[test]
    fn test_registry() {
        let mut registry = StrategyRegistry::new();
        registry.register("double", strategy(|x: i32| x * 2));
        registry.register("triple", strategy(|x: i32| x * 3));
        registry.set_default("double");

        assert_eq!(registry.execute("double", 10).unwrap(), 20);
        assert_eq!(registry.execute("triple", 10).unwrap(), 30);
        assert_eq!(registry.execute_default(21).unwrap(), 42);
    }

    #[test]
    fn test_selector() {
        let mut selector = StrategySelector::new();
        selector.when(|x: &i32| *x < 10, strategy(|x: i32| x * 2));
        selector.when(|x: &i32| *x < 100, strategy(|x: i32| x * 3));
        selector.fallback(strategy(|x: i32| x * 4));

        assert_eq!(selector.execute(5).unwrap(), 10); // < 10: *2
        assert_eq!(selector.execute(50).unwrap(), 150); // < 100: *3
        assert_eq!(selector.execute(200).unwrap(), 800); // fallback: *4
    }
}
