//! Connection pooling for drbot.
//!
//! This crate provides:
//! - Generic connection pooling
//! - Idle connection management
//! - Health checking
//! - Connection lifecycle hooks

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::{Mutex, OwnedSemaphorePermit, Semaphore};
use uuid::Uuid;

/// Pool error types.
#[derive(Error, Debug)]
pub enum PoolError {
    #[error("Pool exhausted")]
    Exhausted,

    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    #[error("Connection timeout after {0:?}")]
    Timeout(Duration),

    #[error("Pool closed")]
    Closed,

    #[error("Health check failed: {0}")]
    HealthCheckFailed(String),
}

/// Result type for pool operations.
pub type Result<T> = std::result::Result<T, PoolError>;

/// Pool configuration.
#[derive(Debug, Clone)]
pub struct PoolConfig {
    /// Minimum number of connections.
    pub min_size: usize,
    /// Maximum number of connections.
    pub max_size: usize,
    /// Connection acquisition timeout.
    pub acquire_timeout: Duration,
    /// Maximum idle time before closing.
    pub max_idle_time: Duration,
    /// Health check interval.
    pub health_check_interval: Duration,
    /// Maximum lifetime of a connection.
    pub max_lifetime: Duration,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            min_size: 1,
            max_size: 10,
            acquire_timeout: Duration::from_secs(30),
            max_idle_time: Duration::from_secs(600),
            health_check_interval: Duration::from_secs(30),
            max_lifetime: Duration::from_secs(3600),
        }
    }
}

/// Pool statistics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PoolStats {
    /// Total connections created.
    pub connections_created: u64,
    /// Total connections closed.
    pub connections_closed: u64,
    /// Currently active connections.
    pub active_connections: usize,
    /// Currently idle connections.
    pub idle_connections: usize,
    /// Total acquisitions.
    pub total_acquisitions: u64,
    /// Failed acquisitions.
    pub failed_acquisitions: u64,
    /// Total wait time in milliseconds.
    pub total_wait_time_ms: u64,
}

/// Trait for connection factories.
#[async_trait]
pub trait ConnectionFactory: Send + Sync {
    /// The connection type.
    type Connection: Send + 'static;

    /// Create a new connection.
    async fn create(&self) -> Result<Self::Connection>;

    /// Check if a connection is healthy.
    async fn check(&self, conn: &Self::Connection) -> bool;

    /// Called before a connection is returned to the pool.
    async fn on_release(&self, _conn: &mut Self::Connection) {}

    /// Called when a connection is destroyed.
    async fn on_destroy(&self, _conn: Self::Connection) {}
}

/// Pooled connection wrapper.
struct PooledConnection<C> {
    /// The actual connection.
    connection: C,
    /// Connection ID.
    id: Uuid,
    /// When the connection was created.
    created_at: DateTime<Utc>,
    /// When the connection was last used.
    last_used_at: DateTime<Utc>,
    /// Number of times this connection has been used.
    use_count: u64,
}

impl<C> PooledConnection<C> {
    fn new(connection: C) -> Self {
        let now = Utc::now();
        Self {
            connection,
            id: Uuid::new_v4(),
            created_at: now,
            last_used_at: now,
            use_count: 0,
        }
    }

    fn touch(&mut self) {
        self.last_used_at = Utc::now();
        self.use_count += 1;
    }

    fn age(&self) -> chrono::Duration {
        Utc::now() - self.created_at
    }

    fn idle_time(&self) -> chrono::Duration {
        Utc::now() - self.last_used_at
    }
}

/// A guard that returns the connection to the pool when dropped.
pub struct PoolGuard<C: Send + 'static> {
    connection: Option<C>,
    pool: Arc<PoolInner<C>>,
    _permit: OwnedSemaphorePermit,
}

impl<C: Send + 'static> PoolGuard<C> {
    /// Get a reference to the connection.
    pub fn get(&self) -> &C {
        self.connection.as_ref().unwrap()
    }

    /// Get a mutable reference to the connection.
    pub fn get_mut(&mut self) -> &mut C {
        self.connection.as_mut().unwrap()
    }

    /// Detach the connection from the pool (don't return it).
    pub fn detach(mut self) -> C {
        self.connection.take().unwrap()
    }
}

impl<C: Send + 'static> Drop for PoolGuard<C> {
    fn drop(&mut self) {
        if let Some(connection) = self.connection.take() {
            // Return to pool asynchronously
            let pool = self.pool.clone();
            tokio::spawn(async move {
                pool.return_connection(connection).await;
            });
        }
    }
}

impl<C: Send + 'static> std::ops::Deref for PoolGuard<C> {
    type Target = C;

    fn deref(&self) -> &Self::Target {
        self.get()
    }
}

impl<C: Send + 'static> std::ops::DerefMut for PoolGuard<C> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.get_mut()
    }
}

/// Inner pool state.
struct PoolInner<C> {
    config: PoolConfig,
    factory: Arc<dyn ConnectionFactory<Connection = C> + Send + Sync>,
    idle_connections: Mutex<VecDeque<PooledConnection<C>>>,
    semaphore: Arc<Semaphore>,
    stats: PoolStatsInner,
    closed: std::sync::atomic::AtomicBool,
}

struct PoolStatsInner {
    connections_created: AtomicU64,
    connections_closed: AtomicU64,
    active_connections: AtomicUsize,
    idle_connections: AtomicUsize,
    total_acquisitions: AtomicU64,
    failed_acquisitions: AtomicU64,
    total_wait_time_ms: AtomicU64,
}

impl Default for PoolStatsInner {
    fn default() -> Self {
        Self {
            connections_created: AtomicU64::new(0),
            connections_closed: AtomicU64::new(0),
            active_connections: AtomicUsize::new(0),
            idle_connections: AtomicUsize::new(0),
            total_acquisitions: AtomicU64::new(0),
            failed_acquisitions: AtomicU64::new(0),
            total_wait_time_ms: AtomicU64::new(0),
        }
    }
}

impl PoolStatsInner {
    fn to_stats(&self) -> PoolStats {
        PoolStats {
            connections_created: self.connections_created.load(Ordering::Relaxed),
            connections_closed: self.connections_closed.load(Ordering::Relaxed),
            active_connections: self.active_connections.load(Ordering::Relaxed),
            idle_connections: self.idle_connections.load(Ordering::Relaxed),
            total_acquisitions: self.total_acquisitions.load(Ordering::Relaxed),
            failed_acquisitions: self.failed_acquisitions.load(Ordering::Relaxed),
            total_wait_time_ms: self.total_wait_time_ms.load(Ordering::Relaxed),
        }
    }
}

impl<C: Send + 'static> PoolInner<C> {
    async fn get_idle_connection(&self) -> Option<PooledConnection<C>> {
        let mut idle = self.idle_connections.lock().await;

        while let Some(mut pooled) = idle.pop_front() {
            self.stats.idle_connections.fetch_sub(1, Ordering::Relaxed);

            // Check if connection is still valid
            let max_lifetime = chrono::Duration::from_std(self.config.max_lifetime)
                .unwrap_or(chrono::Duration::hours(1));
            let max_idle = chrono::Duration::from_std(self.config.max_idle_time)
                .unwrap_or(chrono::Duration::minutes(10));

            if pooled.age() > max_lifetime || pooled.idle_time() > max_idle {
                self.stats
                    .connections_closed
                    .fetch_add(1, Ordering::Relaxed);
                self.factory.on_destroy(pooled.connection).await;
                continue;
            }

            // Health check
            if !self.factory.check(&pooled.connection).await {
                self.stats
                    .connections_closed
                    .fetch_add(1, Ordering::Relaxed);
                self.factory.on_destroy(pooled.connection).await;
                continue;
            }

            pooled.touch();
            return Some(pooled);
        }

        None
    }

    async fn return_connection(&self, connection: C) {
        if self.closed.load(Ordering::Relaxed) {
            self.factory.on_destroy(connection).await;
            self.stats
                .connections_closed
                .fetch_add(1, Ordering::Relaxed);
            return;
        }

        let mut pooled = PooledConnection::new(connection);
        self.factory.on_release(&mut pooled.connection).await;

        let mut idle = self.idle_connections.lock().await;
        idle.push_back(pooled);
        self.stats.idle_connections.fetch_add(1, Ordering::Relaxed);
        self.stats
            .active_connections
            .fetch_sub(1, Ordering::Relaxed);
    }
}

/// Connection pool.
pub struct Pool<C> {
    inner: Arc<PoolInner<C>>,
}

impl<C: Send + 'static> Pool<C> {
    /// Create a new pool.
    pub fn new<F>(config: PoolConfig, factory: F) -> Self
    where
        F: ConnectionFactory<Connection = C> + Send + Sync + 'static,
    {
        let semaphore = Arc::new(Semaphore::new(config.max_size));

        Self {
            inner: Arc::new(PoolInner {
                config,
                factory: Arc::new(factory),
                idle_connections: Mutex::new(VecDeque::new()),
                semaphore,
                stats: PoolStatsInner::default(),
                closed: std::sync::atomic::AtomicBool::new(false),
            }),
        }
    }

    /// Acquire a connection from the pool.
    pub async fn acquire(&self) -> Result<PoolGuard<C>> {
        if self.inner.closed.load(Ordering::Relaxed) {
            return Err(PoolError::Closed);
        }

        let start = std::time::Instant::now();

        // Wait for permit
        let permit = tokio::time::timeout(
            self.inner.config.acquire_timeout,
            self.inner.semaphore.clone().acquire_owned(),
        )
        .await
        .map_err(|_| PoolError::Timeout(self.inner.config.acquire_timeout))?
        .map_err(|_| PoolError::Closed)?;

        let wait_time = start.elapsed().as_millis() as u64;
        self.inner
            .stats
            .total_wait_time_ms
            .fetch_add(wait_time, Ordering::Relaxed);

        // Try to get an idle connection
        if let Some(pooled) = self.inner.get_idle_connection().await {
            self.inner
                .stats
                .total_acquisitions
                .fetch_add(1, Ordering::Relaxed);
            self.inner
                .stats
                .active_connections
                .fetch_add(1, Ordering::Relaxed);

            return Ok(PoolGuard {
                connection: Some(pooled.connection),
                pool: self.inner.clone(),
                _permit: permit,
            });
        }

        // Create a new connection
        match self.inner.factory.create().await {
            Ok(connection) => {
                self.inner
                    .stats
                    .connections_created
                    .fetch_add(1, Ordering::Relaxed);
                self.inner
                    .stats
                    .total_acquisitions
                    .fetch_add(1, Ordering::Relaxed);
                self.inner
                    .stats
                    .active_connections
                    .fetch_add(1, Ordering::Relaxed);

                Ok(PoolGuard {
                    connection: Some(connection),
                    pool: self.inner.clone(),
                    _permit: permit,
                })
            }
            Err(e) => {
                self.inner
                    .stats
                    .failed_acquisitions
                    .fetch_add(1, Ordering::Relaxed);
                Err(e)
            }
        }
    }

    /// Get pool statistics.
    pub fn stats(&self) -> PoolStats {
        self.inner.stats.to_stats()
    }

    /// Close the pool.
    pub async fn close(&self) {
        self.inner.closed.store(true, Ordering::SeqCst);

        // Destroy all idle connections
        let mut idle = self.inner.idle_connections.lock().await;
        while let Some(pooled) = idle.pop_front() {
            self.inner.factory.on_destroy(pooled.connection).await;
            self.inner
                .stats
                .connections_closed
                .fetch_add(1, Ordering::Relaxed);
        }
        self.inner
            .stats
            .idle_connections
            .store(0, Ordering::Relaxed);
    }

    /// Check if pool is closed.
    pub fn is_closed(&self) -> bool {
        self.inner.closed.load(Ordering::Relaxed)
    }

    /// Get current pool size.
    pub fn size(&self) -> usize {
        let stats = self.stats();
        stats.active_connections + stats.idle_connections
    }
}

impl<C> Clone for Pool<C> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

/// Simple pool for testing.
pub struct SimpleFactory<C, F>
where
    F: Fn() -> C + Send + Sync,
{
    create_fn: F,
    _marker: std::marker::PhantomData<C>,
}

impl<C, F> SimpleFactory<C, F>
where
    F: Fn() -> C + Send + Sync,
{
    /// Create a new simple factory.
    pub fn new(create_fn: F) -> Self {
        Self {
            create_fn,
            _marker: std::marker::PhantomData,
        }
    }
}

#[async_trait]
impl<C, F> ConnectionFactory for SimpleFactory<C, F>
where
    C: Send + Sync + 'static,
    F: Fn() -> C + Send + Sync,
{
    type Connection = C;

    async fn create(&self) -> Result<Self::Connection> {
        Ok((self.create_fn)())
    }

    async fn check(&self, _conn: &Self::Connection) -> bool {
        true
    }
}

/// Builder for pools.
pub struct PoolBuilder<C> {
    config: PoolConfig,
    factory: Option<Arc<dyn ConnectionFactory<Connection = C> + Send + Sync>>,
}

impl<C: Send + 'static> PoolBuilder<C> {
    /// Create a new pool builder.
    pub fn new() -> Self {
        Self {
            config: PoolConfig::default(),
            factory: None,
        }
    }

    /// Set minimum pool size.
    pub fn min_size(mut self, size: usize) -> Self {
        self.config.min_size = size;
        self
    }

    /// Set maximum pool size.
    pub fn max_size(mut self, size: usize) -> Self {
        self.config.max_size = size;
        self
    }

    /// Set acquire timeout.
    pub fn acquire_timeout(mut self, timeout: Duration) -> Self {
        self.config.acquire_timeout = timeout;
        self
    }

    /// Set max idle time.
    pub fn max_idle_time(mut self, duration: Duration) -> Self {
        self.config.max_idle_time = duration;
        self
    }

    /// Set max lifetime.
    pub fn max_lifetime(mut self, duration: Duration) -> Self {
        self.config.max_lifetime = duration;
        self
    }

    /// Set the factory.
    pub fn factory<F>(mut self, factory: F) -> Self
    where
        F: ConnectionFactory<Connection = C> + Send + Sync + 'static,
    {
        self.factory = Some(Arc::new(factory));
        self
    }

    /// Build the pool.
    pub fn build(self) -> Result<Pool<C>> {
        let factory = self
            .factory
            .ok_or_else(|| PoolError::ConnectionFailed("No factory provided".to_string()))?;

        let semaphore = Arc::new(Semaphore::new(self.config.max_size));

        Ok(Pool {
            inner: Arc::new(PoolInner {
                config: self.config,
                factory,
                idle_connections: Mutex::new(VecDeque::new()),
                semaphore,
                stats: PoolStatsInner::default(),
                closed: std::sync::atomic::AtomicBool::new(false),
            }),
        })
    }
}

impl<C: Send + 'static> Default for PoolBuilder<C> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockConnection {
        id: u64,
    }

    struct MockFactory {
        counter: AtomicU64,
    }

    impl MockFactory {
        fn new() -> Self {
            Self {
                counter: AtomicU64::new(0),
            }
        }
    }

    #[async_trait]
    impl ConnectionFactory for MockFactory {
        type Connection = MockConnection;

        async fn create(&self) -> Result<MockConnection> {
            let id = self.counter.fetch_add(1, Ordering::Relaxed);
            Ok(MockConnection { id })
        }

        async fn check(&self, _conn: &MockConnection) -> bool {
            true
        }
    }

    #[tokio::test]
    async fn test_pool_acquire_release() {
        let pool = Pool::new(PoolConfig::default(), MockFactory::new());

        let conn = pool.acquire().await.unwrap();
        assert_eq!(conn.id, 0);

        let stats = pool.stats();
        assert_eq!(stats.connections_created, 1);
        assert_eq!(stats.active_connections, 1);

        drop(conn);
        tokio::time::sleep(Duration::from_millis(10)).await;

        let stats = pool.stats();
        assert_eq!(stats.idle_connections, 1);
    }

    #[tokio::test]
    async fn test_pool_reuse() {
        let pool = Pool::new(PoolConfig::default(), MockFactory::new());

        let conn1 = pool.acquire().await.unwrap();
        let id = conn1.id;
        drop(conn1);

        tokio::time::sleep(Duration::from_millis(10)).await;

        let conn2 = pool.acquire().await.unwrap();
        assert_eq!(conn2.id, id); // Same connection reused
    }

    #[tokio::test]
    async fn test_pool_max_size() {
        let config = PoolConfig {
            max_size: 2,
            acquire_timeout: Duration::from_millis(100),
            ..Default::default()
        };

        let pool = Pool::new(config, MockFactory::new());

        let _conn1 = pool.acquire().await.unwrap();
        let _conn2 = pool.acquire().await.unwrap();

        // Third should timeout
        let result = pool.acquire().await;
        assert!(matches!(result, Err(PoolError::Timeout(_))));
    }

    #[tokio::test]
    async fn test_pool_close() {
        let pool = Pool::new(PoolConfig::default(), MockFactory::new());

        let conn = pool.acquire().await.unwrap();
        drop(conn);

        tokio::time::sleep(Duration::from_millis(10)).await;

        pool.close().await;
        assert!(pool.is_closed());

        let result = pool.acquire().await;
        assert!(matches!(result, Err(PoolError::Closed)));
    }

    #[tokio::test]
    async fn test_pool_stats() {
        let pool = Pool::new(PoolConfig::default(), MockFactory::new());

        let conn = pool.acquire().await.unwrap();
        let stats = pool.stats();

        assert_eq!(stats.connections_created, 1);
        assert_eq!(stats.total_acquisitions, 1);
        assert_eq!(stats.active_connections, 1);

        drop(conn);
        tokio::time::sleep(Duration::from_millis(10)).await;

        let stats = pool.stats();
        assert_eq!(stats.active_connections, 0);
        assert_eq!(stats.idle_connections, 1);
    }

    #[tokio::test]
    async fn test_simple_factory() {
        let counter = Arc::new(AtomicU64::new(0));
        let counter_clone = counter.clone();

        let factory = SimpleFactory::new(move || MockConnection {
            id: counter_clone.fetch_add(1, Ordering::Relaxed),
        });

        let pool = Pool::new(PoolConfig::default(), factory);

        let conn = pool.acquire().await.unwrap();
        assert_eq!(conn.id, 0);
    }

    #[tokio::test]
    async fn test_pool_builder() {
        let pool = PoolBuilder::new()
            .max_size(5)
            .min_size(1)
            .acquire_timeout(Duration::from_secs(10))
            .factory(MockFactory::new())
            .build()
            .unwrap();

        let conn = pool.acquire().await.unwrap();
        assert_eq!(conn.id, 0);
    }

    #[tokio::test]
    async fn test_pool_guard_deref() {
        let pool = Pool::new(PoolConfig::default(), MockFactory::new());
        let conn = pool.acquire().await.unwrap();

        // Test Deref
        assert_eq!(conn.id, 0);
    }
}
