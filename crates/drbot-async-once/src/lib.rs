//! Async once cell for drbot.
//!
//! This crate provides:
//! - Async lazy initialization
//! - Once cell with async init
//! - Lazy static alternatives
//! - Cached async computations

use std::cell::UnsafeCell;
use std::future::Future;
use std::sync::atomic::{AtomicU8, Ordering};
use thiserror::Error;
use tokio::sync::OnceCell;

/// Once cell error types.
#[derive(Error, Debug)]
pub enum OnceError {
    #[error("Already initialized")]
    AlreadyInitialized,

    #[error("Initialization failed")]
    InitFailed,

    #[error("Not yet initialized")]
    NotInitialized,
}

/// Result type for once operations.
pub type Result<T> = std::result::Result<T, OnceError>;

/// Async once cell.
pub struct AsyncOnce<T> {
    inner: OnceCell<T>,
}

impl<T> AsyncOnce<T> {
    /// Create new uninitialized once cell.
    pub const fn new() -> Self {
        Self {
            inner: OnceCell::const_new(),
        }
    }

    /// Create initialized once cell.
    pub fn with_value(value: T) -> Self {
        let cell = Self::new();
        let _ = cell.inner.set(value);
        cell
    }

    /// Get or initialize with async function.
    pub async fn get_or_init<F, Fut>(&self, f: F) -> &T
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = T>,
    {
        self.inner.get_or_init(f).await
    }

    /// Try to get or initialize.
    pub async fn get_or_try_init<F, Fut, E>(&self, f: F) -> std::result::Result<&T, E>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = std::result::Result<T, E>>,
    {
        self.inner.get_or_try_init(f).await
    }

    /// Get reference if initialized.
    pub fn get(&self) -> Option<&T> {
        self.inner.get()
    }

    /// Set value (fails if already set).
    pub fn set(&self, value: T) -> Result<()> {
        self.inner
            .set(value)
            .map_err(|_| OnceError::AlreadyInitialized)
    }

    /// Check if initialized.
    pub fn is_initialized(&self) -> bool {
        self.inner.initialized()
    }
}

impl<T> Default for AsyncOnce<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Async lazy value.
pub struct AsyncLazy<T, F = fn() -> std::pin::Pin<Box<dyn Future<Output = T> + Send>>> {
    cell: OnceCell<T>,
    init: F,
}

impl<T, F, Fut> AsyncLazy<T, F>
where
    F: Fn() -> Fut,
    Fut: Future<Output = T>,
{
    /// Create new lazy value.
    pub const fn new(init: F) -> Self {
        Self {
            cell: OnceCell::const_new(),
            init,
        }
    }

    /// Force initialization and get reference.
    pub async fn force(&self) -> &T {
        self.cell.get_or_init(&self.init).await
    }

    /// Get if already initialized.
    pub fn get(&self) -> Option<&T> {
        self.cell.get()
    }
}

/// State for manual once cell.
const UNINIT: u8 = 0;
const INITIALIZING: u8 = 1;
const INIT: u8 = 2;

/// Manual async once cell with more control.
pub struct ManualOnce<T> {
    state: AtomicU8,
    value: UnsafeCell<Option<T>>,
    notify: tokio::sync::Notify,
}

// Safety: We protect access with state transitions
unsafe impl<T: Send> Send for ManualOnce<T> {}
unsafe impl<T: Send + Sync> Sync for ManualOnce<T> {}

impl<T> ManualOnce<T> {
    /// Create new uninitialized cell.
    pub const fn new() -> Self {
        Self {
            state: AtomicU8::new(UNINIT),
            value: UnsafeCell::new(None),
            notify: tokio::sync::Notify::const_new(),
        }
    }

    /// Initialize the cell.
    pub async fn init<F, Fut>(&self, f: F) -> Result<()>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = T>,
    {
        // Try to transition from UNINIT to INITIALIZING
        match self
            .state
            .compare_exchange(UNINIT, INITIALIZING, Ordering::AcqRel, Ordering::Acquire)
        {
            Ok(_) => {
                // We're the initializer
                let value = f().await;

                // Safety: We're the only one with INITIALIZING state
                unsafe {
                    *self.value.get() = Some(value);
                }

                self.state.store(INIT, Ordering::Release);
                self.notify.notify_waiters();
                Ok(())
            }
            Err(INITIALIZING) => {
                // Someone else is initializing, wait
                while self.state.load(Ordering::Acquire) == INITIALIZING {
                    self.notify.notified().await;
                }
                Err(OnceError::AlreadyInitialized)
            }
            Err(_) => {
                // Already initialized
                Err(OnceError::AlreadyInitialized)
            }
        }
    }

    /// Get reference (waits if initializing).
    pub async fn get(&self) -> Option<&T> {
        loop {
            match self.state.load(Ordering::Acquire) {
                UNINIT => return None,
                INITIALIZING => {
                    self.notify.notified().await;
                }
                INIT => {
                    // Safety: State is INIT, value is set
                    unsafe {
                        return (*self.value.get()).as_ref();
                    }
                }
                _ => unreachable!(),
            }
        }
    }

    /// Get without waiting.
    pub fn try_get(&self) -> Option<&T> {
        if self.state.load(Ordering::Acquire) == INIT {
            // Safety: State is INIT
            unsafe { (*self.value.get()).as_ref() }
        } else {
            None
        }
    }

    /// Check if initialized.
    pub fn is_initialized(&self) -> bool {
        self.state.load(Ordering::Acquire) == INIT
    }
}

impl<T> Default for ManualOnce<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Cached async computation.
pub struct CachedAsync<T, F> {
    cell: OnceCell<T>,
    compute: F,
}

impl<T: Clone, F, Fut> CachedAsync<T, F>
where
    F: Fn() -> Fut,
    Fut: Future<Output = T>,
{
    /// Create new cached computation.
    pub const fn new(compute: F) -> Self {
        Self {
            cell: OnceCell::const_new(),
            compute,
        }
    }

    /// Get cached value or compute.
    pub async fn get(&self) -> T {
        self.cell.get_or_init(&self.compute).await.clone()
    }

    /// Invalidate cache.
    pub fn invalidate(&mut self) {
        self.cell = OnceCell::const_new();
    }
}

/// Async memoization helper.
pub struct Memoized<K, V> {
    cache: tokio::sync::RwLock<std::collections::HashMap<K, V>>,
}

impl<K: std::hash::Hash + Eq + Clone, V: Clone> Memoized<K, V> {
    /// Create new memoized cache.
    pub fn new() -> Self {
        Self {
            cache: tokio::sync::RwLock::new(std::collections::HashMap::new()),
        }
    }

    /// Get or compute value.
    pub async fn get_or_insert<F, Fut>(&self, key: K, f: F) -> V
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = V>,
    {
        // Check cache first
        {
            let cache = self.cache.read().await;
            if let Some(value) = cache.get(&key) {
                return value.clone();
            }
        }

        // Compute and cache
        let value = f().await;
        {
            let mut cache = self.cache.write().await;
            cache.entry(key).or_insert(value.clone());
        }
        value
    }

    /// Clear cache.
    pub async fn clear(&self) {
        self.cache.write().await.clear();
    }

    /// Remove specific key.
    pub async fn remove(&self, key: &K) -> Option<V> {
        self.cache.write().await.remove(key)
    }
}

impl<K: std::hash::Hash + Eq + Clone, V: Clone> Default for Memoized<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_async_once() {
        let once = AsyncOnce::new();

        let value = once.get_or_init(|| async { 42 }).await;
        assert_eq!(*value, 42);

        let value2 = once.get_or_init(|| async { 100 }).await;
        assert_eq!(*value2, 42); // Still 42
    }

    #[tokio::test]
    async fn test_single_init() {
        let once = Arc::new(AsyncOnce::new());
        let counter = Arc::new(AtomicUsize::new(0));

        let mut handles = vec![];
        for _ in 0..10 {
            let o = Arc::clone(&once);
            let c = Arc::clone(&counter);
            handles.push(tokio::spawn(async move {
                o.get_or_init(|| async {
                    c.fetch_add(1, Ordering::SeqCst);
                    42
                })
                .await;
            }));
        }

        for h in handles {
            h.await.unwrap();
        }

        assert_eq!(counter.load(Ordering::SeqCst), 1); // Init called once
    }

    #[tokio::test]
    async fn test_async_lazy() {
        static LAZY: AsyncLazy<i32, fn() -> std::pin::Pin<Box<dyn Future<Output = i32> + Send>>> =
            AsyncLazy::new(|| Box::pin(async { 42 }));

        assert!(LAZY.get().is_none());

        let value = LAZY.force().await;
        assert_eq!(*value, 42);

        assert!(LAZY.get().is_some());
    }

    #[tokio::test]
    async fn test_manual_once() {
        let once = ManualOnce::new();

        assert!(!once.is_initialized());

        once.init(|| async { 42 }).await.unwrap();
        assert!(once.is_initialized());

        assert_eq!(once.get().await, Some(&42));
    }

    #[tokio::test]
    async fn test_memoized() {
        let memo: Memoized<&str, i32> = Memoized::new();
        let counter = Arc::new(AtomicUsize::new(0));

        let c1 = Arc::clone(&counter);
        let v1 = memo
            .get_or_insert("key", || {
                let c = Arc::clone(&c1);
                async move {
                    c.fetch_add(1, Ordering::SeqCst);
                    42
                }
            })
            .await;
        assert_eq!(v1, 42);

        let c2 = Arc::clone(&counter);
        let v2 = memo
            .get_or_insert("key", || {
                let c = Arc::clone(&c2);
                async move {
                    c.fetch_add(1, Ordering::SeqCst);
                    100
                }
            })
            .await;
        assert_eq!(v2, 42); // Cached value

        assert_eq!(counter.load(Ordering::SeqCst), 1); // Computed once
    }
}
