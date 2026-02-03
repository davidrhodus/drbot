//! Chain of Responsibility pattern utilities for drbot.
//!
//! This crate provides:
//! - Handler chain
//! - Request processing pipeline
//! - Middleware-style handlers

use std::sync::Arc;
use thiserror::Error;

/// Chain error types.
#[derive(Error, Debug)]
pub enum ChainError {
    #[error("No handler found for request")]
    NoHandler,

    #[error("Handler error: {0}")]
    HandlerError(String),

    #[error("Chain broken")]
    ChainBroken,
}

/// Result type for chain operations.
pub type Result<T> = std::result::Result<T, ChainError>;

/// Handler trait for chain of responsibility.
pub trait Handler<R>: Send + Sync {
    /// Handle request, returning true if handled.
    fn handle(&self, request: &R) -> bool;

    /// Try to handle, returning result if handled.
    fn try_handle(&self, request: &R) -> Option<bool> {
        if self.can_handle(request) {
            Some(self.handle(request))
        } else {
            None
        }
    }

    /// Check if this handler can handle the request.
    fn can_handle(&self, _request: &R) -> bool {
        true
    }
}

/// Handler chain.
pub struct Chain<R> {
    handlers: Vec<Arc<dyn Handler<R>>>,
}

impl<R> Chain<R> {
    /// Create new chain.
    pub fn new() -> Self {
        Self {
            handlers: Vec::new(),
        }
    }

    /// Add handler to chain.
    pub fn add(&mut self, handler: Arc<dyn Handler<R>>) {
        self.handlers.push(handler);
    }

    /// Add handler (builder pattern).
    pub fn with(mut self, handler: Arc<dyn Handler<R>>) -> Self {
        self.add(handler);
        self
    }

    /// Process request through chain.
    pub fn process(&self, request: &R) -> bool {
        for handler in &self.handlers {
            if handler.handle(request) {
                return true;
            }
        }
        false
    }

    /// Process with first capable handler.
    pub fn process_first_capable(&self, request: &R) -> Option<bool> {
        for handler in &self.handlers {
            if handler.can_handle(request) {
                return Some(handler.handle(request));
            }
        }
        None
    }

    /// Get handler count.
    pub fn len(&self) -> usize {
        self.handlers.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.handlers.is_empty()
    }
}

impl<R> Default for Chain<R> {
    fn default() -> Self {
        Self::new()
    }
}

/// Function-based handler.
pub struct FnHandler<R, F: Fn(&R) -> bool + Send + Sync> {
    func: F,
    _marker: std::marker::PhantomData<R>,
}

impl<R, F: Fn(&R) -> bool + Send + Sync> FnHandler<R, F> {
    /// Create new function handler.
    pub fn new(func: F) -> Self {
        Self {
            func,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<R: Send + Sync, F: Fn(&R) -> bool + Send + Sync> Handler<R> for FnHandler<R, F> {
    fn handle(&self, request: &R) -> bool {
        (self.func)(request)
    }
}

/// Conditional handler with predicate.
pub struct ConditionalHandler<R, P: Fn(&R) -> bool + Send + Sync, H: Handler<R>> {
    predicate: P,
    handler: H,
    _marker: std::marker::PhantomData<R>,
}

impl<R, P: Fn(&R) -> bool + Send + Sync, H: Handler<R>> ConditionalHandler<R, P, H> {
    /// Create new conditional handler.
    pub fn new(predicate: P, handler: H) -> Self {
        Self {
            predicate,
            handler,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<R: Send + Sync, P: Fn(&R) -> bool + Send + Sync, H: Handler<R>> Handler<R>
    for ConditionalHandler<R, P, H>
{
    fn handle(&self, request: &R) -> bool {
        self.handler.handle(request)
    }

    fn can_handle(&self, request: &R) -> bool {
        (self.predicate)(request)
    }
}

/// Pipeline for transforming requests.
pub struct Pipeline<T> {
    stages: Vec<Arc<dyn Fn(T) -> T + Send + Sync>>,
}

impl<T> Pipeline<T> {
    /// Create new pipeline.
    pub fn new() -> Self {
        Self { stages: Vec::new() }
    }

    /// Add stage to pipeline.
    pub fn add<F>(&mut self, stage: F)
    where
        F: Fn(T) -> T + Send + Sync + 'static,
    {
        self.stages.push(Arc::new(stage));
    }

    /// Add stage (builder pattern).
    pub fn with<F>(mut self, stage: F) -> Self
    where
        F: Fn(T) -> T + Send + Sync + 'static,
    {
        self.add(stage);
        self
    }

    /// Process through pipeline.
    pub fn process(&self, mut value: T) -> T {
        for stage in &self.stages {
            value = stage(value);
        }
        value
    }

    /// Get stage count.
    pub fn len(&self) -> usize {
        self.stages.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.stages.is_empty()
    }
}

impl<T> Default for Pipeline<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Middleware chain for request/response processing.
pub struct MiddlewareChain<Req, Res> {
    middlewares: Vec<Arc<dyn Fn(Req, &dyn Fn(Req) -> Res) -> Res + Send + Sync>>,
    handler: Arc<dyn Fn(Req) -> Res + Send + Sync>,
}

impl<Req: Clone, Res> MiddlewareChain<Req, Res> {
    /// Create new middleware chain with final handler.
    pub fn new<F>(handler: F) -> Self
    where
        F: Fn(Req) -> Res + Send + Sync + 'static,
    {
        Self {
            middlewares: Vec::new(),
            handler: Arc::new(handler),
        }
    }

    /// Add middleware.
    pub fn use_middleware<M>(&mut self, middleware: M)
    where
        M: Fn(Req, &dyn Fn(Req) -> Res) -> Res + Send + Sync + 'static,
    {
        self.middlewares.push(Arc::new(middleware));
    }

    /// Process request through middleware chain.
    pub fn process(&self, request: Req) -> Res {
        self.process_with_index(request, 0)
    }

    fn process_with_index(&self, request: Req, index: usize) -> Res {
        if index >= self.middlewares.len() {
            (self.handler)(request)
        } else {
            let middleware = &self.middlewares[index];
            let next_index = index + 1;
            middleware(request, &|req| self.process_with_index(req, next_index))
        }
    }
}

/// Helper to create handler.
pub fn handler<R: Send + Sync + 'static, F>(func: F) -> Arc<dyn Handler<R>>
where
    F: Fn(&R) -> bool + Send + Sync + 'static,
{
    Arc::new(FnHandler::new(func))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chain() {
        let chain = Chain::new()
            .with(handler(|n: &i32| {
                if *n < 10 {
                    println!("Small");
                    true
                } else {
                    false
                }
            }))
            .with(handler(|n: &i32| {
                if *n < 100 {
                    println!("Medium");
                    true
                } else {
                    false
                }
            }))
            .with(handler(|_: &i32| {
                println!("Large");
                true
            }));

        assert!(chain.process(&5));
        assert!(chain.process(&50));
        assert!(chain.process(&500));
    }

    #[test]
    fn test_pipeline() {
        let pipeline = Pipeline::new()
            .with(|x: i32| x + 1)
            .with(|x: i32| x * 2)
            .with(|x: i32| x - 1);

        assert_eq!(pipeline.process(10), 21); // ((10 + 1) * 2) - 1
    }

    #[test]
    fn test_conditional_handler() {
        let handler = ConditionalHandler::new(|n: &i32| *n > 0, FnHandler::new(|_: &i32| true));

        assert!(handler.can_handle(&5));
        assert!(!handler.can_handle(&-5));
    }
}
