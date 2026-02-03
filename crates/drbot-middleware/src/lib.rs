//! Middleware chain for request processing in drbot.
//!
//! This crate provides:
//! - Composable middleware
//! - Request/response transformation
//! - Error handling middleware
//! - Logging and tracing middleware

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Middleware error types.
#[derive(Error, Debug)]
pub enum MiddlewareError {
    #[error("Request rejected: {0}")]
    Rejected(String),

    #[error("Processing error: {0}")]
    ProcessingError(String),

    #[error("Timeout")]
    Timeout,

    #[error("Chain broken")]
    ChainBroken,
}

/// Result type for middleware operations.
pub type Result<T> = std::result::Result<T, MiddlewareError>;

/// Request context.
#[derive(Debug, Clone)]
pub struct Context {
    /// Request ID.
    pub id: Uuid,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
    /// Attributes.
    pub attributes: HashMap<String, String>,
    /// Extensions for arbitrary data.
    extensions: HashMap<std::any::TypeId, Arc<dyn std::any::Any + Send + Sync>>,
}

impl Context {
    /// Create a new context.
    pub fn new() -> Self {
        Self {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            attributes: HashMap::new(),
            extensions: HashMap::new(),
        }
    }

    /// Set an attribute.
    pub fn set_attribute(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.attributes.insert(key.into(), value.into());
    }

    /// Get an attribute.
    pub fn get_attribute(&self, key: &str) -> Option<&str> {
        self.attributes.get(key).map(|s| s.as_str())
    }

    /// Set an extension.
    pub fn set_extension<T: Send + Sync + 'static>(&mut self, value: T) {
        self.extensions
            .insert(std::any::TypeId::of::<T>(), Arc::new(value));
    }

    /// Get an extension.
    pub fn get_extension<T: Send + Sync + 'static>(&self) -> Option<Arc<T>> {
        self.extensions
            .get(&std::any::TypeId::of::<T>())
            .and_then(|v| v.clone().downcast::<T>().ok())
    }

    /// Elapsed time since context creation.
    pub fn elapsed(&self) -> Duration {
        let now = Utc::now();
        (now - self.timestamp).to_std().unwrap_or(Duration::ZERO)
    }
}

impl Default for Context {
    fn default() -> Self {
        Self::new()
    }
}

/// Middleware trait.
#[async_trait]
pub trait Middleware<Req, Res>: Send + Sync {
    /// Process a request.
    async fn process(
        &self,
        ctx: &mut Context,
        request: Req,
        next: Next<'_, Req, Res>,
    ) -> Result<Res>;
}

/// Next handler in the chain.
pub struct Next<'a, Req, Res> {
    chain: &'a [Arc<dyn Middleware<Req, Res>>],
    handler: &'a dyn Handler<Req, Res>,
}

impl<'a, Req: Send + 'static, Res: Send + 'static> Next<'a, Req, Res> {
    /// Run the next middleware or handler.
    pub async fn run(self, ctx: &mut Context, request: Req) -> Result<Res> {
        if let Some((first, rest)) = self.chain.split_first() {
            let next = Next {
                chain: rest,
                handler: self.handler,
            };
            first.process(ctx, request, next).await
        } else {
            self.handler.handle(ctx, request).await
        }
    }
}

/// Final request handler.
#[async_trait]
pub trait Handler<Req, Res>: Send + Sync {
    /// Handle a request.
    async fn handle(&self, ctx: &mut Context, request: Req) -> Result<Res>;
}

/// Middleware chain.
pub struct MiddlewareChain<Req, Res> {
    middlewares: Vec<Arc<dyn Middleware<Req, Res>>>,
    handler: Arc<dyn Handler<Req, Res>>,
}

impl<Req: Send + 'static, Res: Send + 'static> MiddlewareChain<Req, Res> {
    /// Create a new chain with a handler.
    pub fn new<H: Handler<Req, Res> + 'static>(handler: H) -> Self {
        Self {
            middlewares: Vec::new(),
            handler: Arc::new(handler),
        }
    }

    /// Add a middleware to the chain.
    pub fn add<M: Middleware<Req, Res> + 'static>(mut self, middleware: M) -> Self {
        self.middlewares.push(Arc::new(middleware));
        self
    }

    /// Process a request through the chain.
    pub async fn process(&self, request: Req) -> Result<Res> {
        let mut ctx = Context::new();
        self.process_with_context(&mut ctx, request).await
    }

    /// Process with an existing context.
    pub async fn process_with_context(&self, ctx: &mut Context, request: Req) -> Result<Res> {
        let next = Next {
            chain: &self.middlewares,
            handler: self.handler.as_ref(),
        };
        next.run(ctx, request).await
    }
}

/// Logging middleware.
pub struct LoggingMiddleware {
    name: String,
}

impl LoggingMiddleware {
    /// Create a new logging middleware.
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }
}

#[async_trait]
impl<Req: Send + 'static, Res: Send + 'static> Middleware<Req, Res> for LoggingMiddleware {
    async fn process(
        &self,
        ctx: &mut Context,
        request: Req,
        next: Next<'_, Req, Res>,
    ) -> Result<Res> {
        let start = std::time::Instant::now();
        ctx.set_attribute("middleware.logging", &self.name);

        let result = next.run(ctx, request).await;

        let elapsed = start.elapsed();
        ctx.set_attribute("duration_ms", elapsed.as_millis().to_string());

        result
    }
}

/// Timeout middleware.
pub struct TimeoutMiddleware {
    timeout: Duration,
}

impl TimeoutMiddleware {
    /// Create a new timeout middleware.
    pub fn new(timeout: Duration) -> Self {
        Self { timeout }
    }
}

#[async_trait]
impl<Req: Send + 'static, Res: Send + 'static> Middleware<Req, Res> for TimeoutMiddleware {
    async fn process(
        &self,
        ctx: &mut Context,
        request: Req,
        next: Next<'_, Req, Res>,
    ) -> Result<Res> {
        tokio::time::timeout(self.timeout, next.run(ctx, request))
            .await
            .map_err(|_| MiddlewareError::Timeout)?
    }
}

/// Metrics middleware.
pub struct MetricsMiddleware {
    requests: AtomicU64,
    errors: AtomicU64,
    total_duration_ms: AtomicU64,
}

impl MetricsMiddleware {
    /// Create a new metrics middleware.
    pub fn new() -> Self {
        Self {
            requests: AtomicU64::new(0),
            errors: AtomicU64::new(0),
            total_duration_ms: AtomicU64::new(0),
        }
    }

    /// Get request count.
    pub fn requests(&self) -> u64 {
        self.requests.load(Ordering::Relaxed)
    }

    /// Get error count.
    pub fn errors(&self) -> u64 {
        self.errors.load(Ordering::Relaxed)
    }

    /// Get average duration in ms.
    pub fn avg_duration_ms(&self) -> f64 {
        let requests = self.requests();
        if requests == 0 {
            0.0
        } else {
            self.total_duration_ms.load(Ordering::Relaxed) as f64 / requests as f64
        }
    }
}

impl Default for MetricsMiddleware {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl<Req: Send + 'static, Res: Send + 'static> Middleware<Req, Res> for MetricsMiddleware {
    async fn process(
        &self,
        ctx: &mut Context,
        request: Req,
        next: Next<'_, Req, Res>,
    ) -> Result<Res> {
        self.requests.fetch_add(1, Ordering::Relaxed);
        let start = std::time::Instant::now();

        let result = next.run(ctx, request).await;

        let elapsed = start.elapsed().as_millis() as u64;
        self.total_duration_ms.fetch_add(elapsed, Ordering::Relaxed);

        if result.is_err() {
            self.errors.fetch_add(1, Ordering::Relaxed);
        }

        result
    }
}

/// Retry middleware.
pub struct RetryMiddleware {
    max_attempts: u32,
    delay: Duration,
}

impl RetryMiddleware {
    /// Create a new retry middleware.
    pub fn new(max_attempts: u32, delay: Duration) -> Self {
        Self {
            max_attempts,
            delay,
        }
    }
}

#[async_trait]
impl<Req: Send + 'static, Res: Send + 'static> Middleware<Req, Res> for RetryMiddleware {
    async fn process(
        &self,
        ctx: &mut Context,
        request: Req,
        next: Next<'_, Req, Res>,
    ) -> Result<Res> {
        ctx.set_attribute("retry.max_attempts", self.max_attempts.to_string());
        // Note: Retry logic would need to be implemented at a higher level
        // since next can only be called once per middleware invocation
        next.run(ctx, request).await
    }
}

/// Rate limiting middleware.
pub struct RateLimitMiddleware {
    requests_per_second: u32,
    current_count: AtomicU64,
    last_reset: RwLock<std::time::Instant>,
}

impl RateLimitMiddleware {
    /// Create a new rate limit middleware.
    pub fn new(requests_per_second: u32) -> Self {
        Self {
            requests_per_second,
            current_count: AtomicU64::new(0),
            last_reset: RwLock::new(std::time::Instant::now()),
        }
    }

    async fn check_limit(&self) -> bool {
        let mut last_reset = self.last_reset.write().await;
        let now = std::time::Instant::now();

        if now.duration_since(*last_reset) >= Duration::from_secs(1) {
            *last_reset = now;
            self.current_count.store(0, Ordering::Relaxed);
        }

        let count = self.current_count.fetch_add(1, Ordering::Relaxed);
        count < self.requests_per_second as u64
    }
}

#[async_trait]
impl<Req: Send + 'static, Res: Send + 'static> Middleware<Req, Res> for RateLimitMiddleware {
    async fn process(
        &self,
        ctx: &mut Context,
        request: Req,
        next: Next<'_, Req, Res>,
    ) -> Result<Res> {
        if !self.check_limit().await {
            return Err(MiddlewareError::Rejected("Rate limit exceeded".to_string()));
        }

        next.run(ctx, request).await
    }
}

/// Correlation ID middleware.
pub struct CorrelationMiddleware;

#[async_trait]
impl<Req: Send + 'static, Res: Send + 'static> Middleware<Req, Res> for CorrelationMiddleware {
    async fn process(
        &self,
        ctx: &mut Context,
        request: Req,
        next: Next<'_, Req, Res>,
    ) -> Result<Res> {
        if ctx.get_attribute("correlation_id").is_none() {
            ctx.set_attribute("correlation_id", Uuid::new_v4().to_string());
        }
        next.run(ctx, request).await
    }
}

/// Simple handler that applies a sync function.
pub struct MapHandler<F, Req, Res>
where
    F: Fn(Req) -> Result<Res> + Send + Sync,
{
    f: F,
    _req: std::marker::PhantomData<Req>,
    _res: std::marker::PhantomData<Res>,
}

impl<F, Req, Res> MapHandler<F, Req, Res>
where
    F: Fn(Req) -> Result<Res> + Send + Sync,
{
    /// Create a new map handler.
    pub fn new(f: F) -> Self {
        Self {
            f,
            _req: std::marker::PhantomData,
            _res: std::marker::PhantomData,
        }
    }
}

#[async_trait]
impl<F, Req, Res> Handler<Req, Res> for MapHandler<F, Req, Res>
where
    F: Fn(Req) -> Result<Res> + Send + Sync,
    Req: Send + Sync + 'static,
    Res: Send + Sync + 'static,
{
    async fn handle(&self, _ctx: &mut Context, request: Req) -> Result<Res> {
        (self.f)(request)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct EchoHandler;

    #[async_trait]
    impl Handler<String, String> for EchoHandler {
        async fn handle(&self, _ctx: &mut Context, request: String) -> Result<String> {
            Ok(request)
        }
    }

    #[tokio::test]
    async fn test_simple_chain() {
        let chain = MiddlewareChain::new(EchoHandler);
        let result = chain.process("hello".to_string()).await;
        assert_eq!(result.unwrap(), "hello");
    }

    #[tokio::test]
    async fn test_logging_middleware() {
        let chain = MiddlewareChain::new(EchoHandler).add(LoggingMiddleware::new("test"));

        let result = chain.process("hello".to_string()).await;
        assert_eq!(result.unwrap(), "hello");
    }

    #[tokio::test]
    async fn test_timeout_middleware() {
        let chain =
            MiddlewareChain::new(EchoHandler).add(TimeoutMiddleware::new(Duration::from_secs(1)));

        let result = chain.process("hello".to_string()).await;
        assert_eq!(result.unwrap(), "hello");
    }

    #[tokio::test]
    async fn test_metrics_middleware() {
        let metrics = Arc::new(MetricsMiddleware::new());
        let metrics_clone = metrics.clone();

        struct MetricsWrapper(Arc<MetricsMiddleware>);

        #[async_trait]
        impl Middleware<String, String> for MetricsWrapper {
            async fn process(
                &self,
                ctx: &mut Context,
                request: String,
                next: Next<'_, String, String>,
            ) -> Result<String> {
                self.0.process(ctx, request, next).await
            }
        }

        let chain = MiddlewareChain::new(EchoHandler).add(MetricsWrapper(metrics_clone));

        chain.process("hello".to_string()).await.unwrap();
        chain.process("world".to_string()).await.unwrap();

        assert_eq!(metrics.requests(), 2);
        assert_eq!(metrics.errors(), 0);
    }

    #[test]
    fn test_context() {
        let mut ctx = Context::new();
        ctx.set_attribute("key", "value");
        assert_eq!(ctx.get_attribute("key"), Some("value"));
    }

    #[test]
    fn test_context_extension() {
        let mut ctx = Context::new();
        ctx.set_extension(42u32);

        let value = ctx.get_extension::<u32>();
        assert_eq!(*value.unwrap(), 42);
    }

    #[tokio::test]
    async fn test_map_handler() {
        let chain = MiddlewareChain::new(MapHandler::new(|req: String| Ok(req)));
        let result = chain.process("hello".to_string()).await;
        assert_eq!(result.unwrap(), "hello");
    }

    #[tokio::test]
    async fn test_correlation_middleware() {
        let chain = MiddlewareChain::new(EchoHandler).add(CorrelationMiddleware);

        let mut ctx = Context::new();
        chain
            .process_with_context(&mut ctx, "hello".to_string())
            .await
            .unwrap();

        assert!(ctx.get_attribute("correlation_id").is_some());
    }
}
