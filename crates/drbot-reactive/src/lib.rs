//! Reactive primitives for drbot.
//!
//! This crate provides:
//! - Reactive values
//! - Computed values
//! - Effects
//! - Reactive scope

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};
use thiserror::Error;

/// Reactive error types.
#[derive(Error, Debug)]
pub enum ReactiveError {
    #[error("Circular dependency detected")]
    CircularDependency,

    #[error("Scope disposed")]
    ScopeDisposed,
}

/// Result type for reactive operations.
pub type Result<T> = std::result::Result<T, ReactiveError>;

/// Reactive ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ReactiveId(u64);

impl ReactiveId {
    /// Generate new ID.
    pub fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        Self(COUNTER.fetch_add(1, Ordering::SeqCst))
    }
}

impl Default for ReactiveId {
    fn default() -> Self {
        Self::new()
    }
}

/// Subscriber callback.
type Subscriber = Arc<dyn Fn() + Send + Sync>;

/// Reactive value.
pub struct Reactive<T> {
    id: ReactiveId,
    value: Mutex<T>,
    version: AtomicU64,
    subscribers: Mutex<Vec<Weak<dyn Fn() + Send + Sync>>>,
}

impl<T> Reactive<T> {
    /// Create new reactive value.
    pub fn new(value: T) -> Arc<Self> {
        Arc::new(Self {
            id: ReactiveId::new(),
            value: Mutex::new(value),
            version: AtomicU64::new(0),
            subscribers: Mutex::new(Vec::new()),
        })
    }

    /// Get ID.
    pub fn id(&self) -> ReactiveId {
        self.id
    }

    /// Get version.
    pub fn version(&self) -> u64 {
        self.version.load(Ordering::SeqCst)
    }

    /// Get value.
    pub fn get(&self) -> T
    where
        T: Clone,
    {
        self.value.lock().unwrap().clone()
    }

    /// Get with function.
    pub fn with<R, F: FnOnce(&T) -> R>(&self, f: F) -> R {
        f(&self.value.lock().unwrap())
    }

    /// Set value.
    pub fn set(&self, value: T) {
        *self.value.lock().unwrap() = value;
        self.version.fetch_add(1, Ordering::SeqCst);
        self.notify();
    }

    /// Update with function.
    pub fn update<F: FnOnce(&mut T)>(&self, f: F) {
        f(&mut self.value.lock().unwrap());
        self.version.fetch_add(1, Ordering::SeqCst);
        self.notify();
    }

    /// Subscribe to changes.
    pub fn subscribe(&self, callback: Subscriber) {
        self.subscribers
            .lock()
            .unwrap()
            .push(Arc::downgrade(&callback));
    }

    /// Notify subscribers.
    fn notify(&self) {
        let mut subscribers = self.subscribers.lock().unwrap();
        subscribers.retain(|weak| {
            if let Some(callback) = weak.upgrade() {
                callback();
                true
            } else {
                false
            }
        });
    }
}

/// Computed value.
pub struct Computed<T> {
    id: ReactiveId,
    compute: Box<dyn Fn() -> T + Send + Sync>,
    cached: Mutex<Option<(u64, T)>>,
    deps_version: Mutex<u64>,
}

impl<T: Clone> Computed<T> {
    /// Create new computed value.
    pub fn new<F>(compute: F) -> Arc<Self>
    where
        F: Fn() -> T + Send + Sync + 'static,
    {
        Arc::new(Self {
            id: ReactiveId::new(),
            compute: Box::new(compute),
            cached: Mutex::new(None),
            deps_version: Mutex::new(0),
        })
    }

    /// Get ID.
    pub fn id(&self) -> ReactiveId {
        self.id
    }

    /// Get computed value.
    pub fn get(&self) -> T {
        let mut cached = self.cached.lock().unwrap();
        let current_version = *self.deps_version.lock().unwrap();

        if let Some((version, ref value)) = *cached {
            if version == current_version {
                return value.clone();
            }
        }

        let value = (self.compute)();
        *cached = Some((current_version, value.clone()));
        value
    }

    /// Invalidate cache.
    pub fn invalidate(&self) {
        let mut version = self.deps_version.lock().unwrap();
        *version += 1;
    }
}

/// Effect that runs when dependencies change.
pub struct Effect {
    id: ReactiveId,
    effect: Box<dyn Fn() + Send + Sync>,
    cleanup: Mutex<Option<Box<dyn FnOnce() + Send>>>,
}

impl Effect {
    /// Create new effect.
    pub fn new<F>(effect: F) -> Arc<Self>
    where
        F: Fn() + Send + Sync + 'static,
    {
        let eff = Arc::new(Self {
            id: ReactiveId::new(),
            effect: Box::new(effect),
            cleanup: Mutex::new(None),
        });

        // Run effect immediately
        eff.run();

        eff
    }

    /// Get ID.
    pub fn id(&self) -> ReactiveId {
        self.id
    }

    /// Run effect.
    pub fn run(&self) {
        // Run cleanup first
        if let Some(cleanup) = self.cleanup.lock().unwrap().take() {
            cleanup();
        }

        // Run effect
        (self.effect)();
    }

    /// Set cleanup function.
    pub fn on_cleanup<F>(&self, cleanup: F)
    where
        F: FnOnce() + Send + 'static,
    {
        *self.cleanup.lock().unwrap() = Some(Box::new(cleanup));
    }

    /// Dispose effect.
    pub fn dispose(&self) {
        if let Some(cleanup) = self.cleanup.lock().unwrap().take() {
            cleanup();
        }
    }
}

impl Drop for Effect {
    fn drop(&mut self) {
        if let Some(cleanup) = self.cleanup.lock().unwrap().take() {
            cleanup();
        }
    }
}

/// Reactive scope for managing reactive primitives.
pub struct Scope {
    id: ReactiveId,
    effects: Mutex<Vec<Arc<Effect>>>,
    disposed: std::sync::atomic::AtomicBool,
}

impl Scope {
    /// Create new scope.
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            id: ReactiveId::new(),
            effects: Mutex::new(Vec::new()),
            disposed: std::sync::atomic::AtomicBool::new(false),
        })
    }

    /// Get ID.
    pub fn id(&self) -> ReactiveId {
        self.id
    }

    /// Create effect in scope.
    pub fn effect<F>(&self, effect: F) -> Arc<Effect>
    where
        F: Fn() + Send + Sync + 'static,
    {
        let eff = Effect::new(effect);
        self.effects.lock().unwrap().push(eff.clone());
        eff
    }

    /// Dispose scope.
    pub fn dispose(&self) {
        if self.disposed.swap(true, Ordering::SeqCst) {
            return;
        }

        let effects = std::mem::take(&mut *self.effects.lock().unwrap());
        for effect in effects {
            effect.dispose();
        }
    }

    /// Check if disposed.
    pub fn is_disposed(&self) -> bool {
        self.disposed.load(Ordering::SeqCst)
    }
}

impl Default for Scope {
    fn default() -> Self {
        Self {
            id: ReactiveId::new(),
            effects: Mutex::new(Vec::new()),
            disposed: std::sync::atomic::AtomicBool::new(false),
        }
    }
}

impl Drop for Scope {
    fn drop(&mut self) {
        self.dispose();
    }
}

/// Memo (cached computed).
pub struct Memo<T> {
    computed: Arc<Computed<T>>,
}

impl<T: Clone + PartialEq> Memo<T> {
    /// Create new memo.
    pub fn new<F>(compute: F) -> Self
    where
        F: Fn() -> T + Send + Sync + 'static,
    {
        Self {
            computed: Computed::new(compute),
        }
    }

    /// Get memoized value.
    pub fn get(&self) -> T {
        self.computed.get()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reactive() {
        let value = Reactive::new(0);

        assert_eq!(value.get(), 0);

        value.set(5);
        assert_eq!(value.get(), 5);
        assert_eq!(value.version(), 1);
    }

    #[test]
    fn test_reactive_subscribe() {
        let value = Reactive::new(0);
        let called = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let called_clone = called.clone();

        let callback: Subscriber = Arc::new(move || {
            called_clone.store(true, Ordering::SeqCst);
        });

        value.subscribe(callback.clone());
        value.set(5);

        // Keep callback alive
        drop(callback);

        assert!(called.load(Ordering::SeqCst));
    }

    #[test]
    fn test_computed() {
        let computed = Computed::new(|| 2 + 2);
        assert_eq!(computed.get(), 4);
    }

    #[test]
    fn test_scope() {
        let scope = Scope::new();
        let counter = Arc::new(std::sync::atomic::AtomicI32::new(0));
        let counter_clone = counter.clone();

        let _effect = scope.effect(move || {
            counter_clone.fetch_add(1, Ordering::SeqCst);
        });

        // Effect runs immediately
        assert_eq!(counter.load(Ordering::SeqCst), 1);

        scope.dispose();
        assert!(scope.is_disposed());
    }
}
