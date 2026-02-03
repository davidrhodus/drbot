//! Generic resource pooling for drbot.
//!
//! This crate provides:
//! - Generic object pool
//! - Resource acquisition/release
//! - Pool statistics

use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;
use thiserror::Error;

/// Pool error types.
#[derive(Error, Debug, Clone)]
pub enum PoolError {
    #[error("Pool exhausted")]
    Exhausted,

    #[error("Acquisition timeout")]
    Timeout,

    #[error("Pool closed")]
    Closed,

    #[error("Resource creation failed: {0}")]
    CreationFailed(String),
}

/// Result type for pool operations.
pub type Result<T> = std::result::Result<T, PoolError>;

/// Pool configuration.
#[derive(Debug, Clone)]
pub struct PoolConfig {
    /// Maximum pool size.
    pub max_size: usize,
    /// Minimum idle resources.
    pub min_idle: usize,
    /// Maximum wait time for acquisition.
    pub max_wait: Option<Duration>,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            max_size: 10,
            min_idle: 1,
            max_wait: Some(Duration::from_secs(30)),
        }
    }
}

/// Resource factory trait.
pub trait ResourceFactory<T>: Send + Sync {
    /// Create new resource.
    fn create(&self) -> Result<T>;

    /// Validate resource is still usable.
    fn validate(&self, _resource: &T) -> bool {
        true
    }

    /// Reset resource before return to pool.
    fn reset(&self, _resource: &mut T) {}

    /// Destroy resource.
    fn destroy(&self, _resource: T) {}
}

/// Function-based resource factory.
pub struct FnFactory<T, F>
where
    F: Fn() -> Result<T> + Send + Sync,
{
    create_fn: F,
    _marker: std::marker::PhantomData<T>,
}

impl<T, F> FnFactory<T, F>
where
    F: Fn() -> Result<T> + Send + Sync,
{
    /// Create new function factory.
    pub fn new(create_fn: F) -> Self {
        Self {
            create_fn,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<T, F> ResourceFactory<T> for FnFactory<T, F>
where
    T: Send + Sync,
    F: Fn() -> Result<T> + Send + Sync,
{
    fn create(&self) -> Result<T> {
        (self.create_fn)()
    }
}

/// Internal pool state.
struct PoolInner<T> {
    resources: VecDeque<T>,
    in_use: usize,
    closed: bool,
}

/// Generic resource pool.
pub struct Pool<T> {
    inner: Arc<(Mutex<PoolInner<T>>, Condvar)>,
    factory: Arc<dyn ResourceFactory<T> + Send + Sync>,
    config: PoolConfig,
}

impl<T: Send + Sync + 'static> Pool<T> {
    /// Create new pool with factory.
    pub fn new<F: ResourceFactory<T> + 'static>(factory: F, config: PoolConfig) -> Self {
        Self {
            inner: Arc::new((
                Mutex::new(PoolInner {
                    resources: VecDeque::new(),
                    in_use: 0,
                    closed: false,
                }),
                Condvar::new(),
            )),
            factory: Arc::new(factory),
            config,
        }
    }

    /// Create pool with function factory.
    pub fn with_fn<F>(create_fn: F, config: PoolConfig) -> Self
    where
        F: Fn() -> Result<T> + Send + Sync + 'static,
    {
        Self::new(FnFactory::new(create_fn), config)
    }

    /// Acquire resource from pool.
    pub fn acquire(&self) -> Result<PooledResource<T>> {
        let mut inner = self.inner.0.lock().unwrap();

        loop {
            if inner.closed {
                return Err(PoolError::Closed);
            }

            // Try to get existing resource
            while let Some(resource) = inner.resources.pop_front() {
                if self.factory.validate(&resource) {
                    inner.in_use += 1;
                    return Ok(PooledResource {
                        resource: Some(resource),
                        pool: self.inner.clone(),
                        factory: self.factory.clone(),
                    });
                }
                // Resource invalid, destroy it
                self.factory.destroy(resource);
            }

            // No resources available, try to create new one
            let total = inner.resources.len() + inner.in_use;
            if total < self.config.max_size {
                drop(inner);
                match self.factory.create() {
                    Ok(resource) => {
                        let mut inner = self.inner.0.lock().unwrap();
                        inner.in_use += 1;
                        return Ok(PooledResource {
                            resource: Some(resource),
                            pool: self.inner.clone(),
                            factory: self.factory.clone(),
                        });
                    }
                    Err(e) => return Err(e),
                }
            }

            // Wait for resource to be returned
            if let Some(timeout) = self.config.max_wait {
                let (new_inner, result) = self.inner.1.wait_timeout(inner, timeout).unwrap();
                inner = new_inner;
                if result.timed_out() {
                    return Err(PoolError::Timeout);
                }
            } else {
                inner = self.inner.1.wait(inner).unwrap();
            }
        }
    }

    /// Try to acquire without waiting.
    pub fn try_acquire(&self) -> Result<PooledResource<T>> {
        let mut inner = self.inner.0.lock().unwrap();

        if inner.closed {
            return Err(PoolError::Closed);
        }

        while let Some(resource) = inner.resources.pop_front() {
            if self.factory.validate(&resource) {
                inner.in_use += 1;
                return Ok(PooledResource {
                    resource: Some(resource),
                    pool: self.inner.clone(),
                    factory: self.factory.clone(),
                });
            }
            self.factory.destroy(resource);
        }

        let total = inner.resources.len() + inner.in_use;
        if total < self.config.max_size {
            drop(inner);
            let resource = self.factory.create()?;
            let mut inner = self.inner.0.lock().unwrap();
            inner.in_use += 1;
            return Ok(PooledResource {
                resource: Some(resource),
                pool: self.inner.clone(),
                factory: self.factory.clone(),
            });
        }

        Err(PoolError::Exhausted)
    }

    /// Get pool statistics.
    pub fn stats(&self) -> PoolStats {
        let inner = self.inner.0.lock().unwrap();
        PoolStats {
            idle: inner.resources.len(),
            in_use: inner.in_use,
            max_size: self.config.max_size,
        }
    }

    /// Close the pool.
    pub fn close(&self) {
        let mut inner = self.inner.0.lock().unwrap();
        inner.closed = true;
        while let Some(resource) = inner.resources.pop_front() {
            self.factory.destroy(resource);
        }
        self.inner.1.notify_all();
    }
}

/// Pool statistics.
#[derive(Debug, Clone)]
pub struct PoolStats {
    pub idle: usize,
    pub in_use: usize,
    pub max_size: usize,
}

/// Pooled resource handle.
pub struct PooledResource<T> {
    resource: Option<T>,
    pool: Arc<(Mutex<PoolInner<T>>, Condvar)>,
    factory: Arc<dyn ResourceFactory<T> + Send + Sync>,
}

impl<T> PooledResource<T> {
    /// Get reference to resource.
    pub fn get(&self) -> &T {
        self.resource.as_ref().unwrap()
    }

    /// Get mutable reference to resource.
    pub fn get_mut(&mut self) -> &mut T {
        self.resource.as_mut().unwrap()
    }
}

impl<T> std::ops::Deref for PooledResource<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.get()
    }
}

impl<T> std::ops::DerefMut for PooledResource<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.get_mut()
    }
}

impl<T> Drop for PooledResource<T> {
    fn drop(&mut self) {
        if let Some(mut resource) = self.resource.take() {
            let mut inner = self.pool.0.lock().unwrap();
            inner.in_use -= 1;

            if !inner.closed {
                self.factory.reset(&mut resource);
                inner.resources.push_back(resource);
                self.pool.1.notify_one();
            } else {
                self.factory.destroy(resource);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicI32, Ordering};

    #[test]
    fn test_pool_basic() {
        let counter = Arc::new(AtomicI32::new(0));
        let c = counter.clone();

        let pool = Pool::with_fn(
            move || {
                c.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
            PoolConfig::default(),
        );

        {
            let _r1 = pool.acquire().unwrap();
            let _r2 = pool.acquire().unwrap();
        }

        // Resources returned to pool
        let stats = pool.stats();
        assert_eq!(stats.idle, 2);
        assert_eq!(stats.in_use, 0);
    }

    #[test]
    fn test_pool_reuse() {
        let counter = Arc::new(AtomicI32::new(0));
        let c = counter.clone();

        let pool = Pool::with_fn(
            move || {
                c.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
            PoolConfig::default(),
        );

        {
            let _r = pool.acquire().unwrap();
        }
        {
            let _r = pool.acquire().unwrap();
        }

        // Should only create once
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_pool_exhausted() {
        let config = PoolConfig {
            max_size: 1,
            max_wait: None,
            ..Default::default()
        };

        let pool = Pool::with_fn(|| Ok(()), config);

        let _r1 = pool.acquire().unwrap();
        let result = pool.try_acquire();

        assert!(matches!(result, Err(PoolError::Exhausted)));
    }
}
