//! Observable pattern for drbot.
//!
//! This crate provides:
//! - Observable values with change notification
//! - Computed values
//! - Observer pattern

use std::sync::{Arc, RwLock, Weak};
use thiserror::Error;

/// Observable error types.
#[derive(Error, Debug, Clone)]
pub enum ObservableError {
    #[error("Observer not found")]
    ObserverNotFound,

    #[error("Observable disposed")]
    Disposed,
}

/// Result type for observable operations.
pub type Result<T> = std::result::Result<T, ObservableError>;

/// Observer ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ObserverId(usize);

/// Observable value that notifies observers on change.
pub struct Observable<T> {
    value: RwLock<T>,
    observers: RwLock<Vec<(ObserverId, Box<dyn Fn(&T) + Send + Sync>)>>,
    next_id: RwLock<usize>,
}

impl<T> Observable<T> {
    /// Create new observable.
    pub fn new(value: T) -> Self {
        Self {
            value: RwLock::new(value),
            observers: RwLock::new(Vec::new()),
            next_id: RwLock::new(0),
        }
    }

    /// Get current value.
    pub fn get(&self) -> T
    where
        T: Clone,
    {
        self.value.read().unwrap().clone()
    }

    /// Set value and notify observers.
    pub fn set(&self, value: T)
    where
        T: Clone,
    {
        {
            let mut v = self.value.write().unwrap();
            *v = value;
        }
        self.notify();
    }

    /// Update value with function.
    pub fn update<F>(&self, f: F)
    where
        F: FnOnce(&mut T),
        T: Clone,
    {
        {
            let mut v = self.value.write().unwrap();
            f(&mut v);
        }
        self.notify();
    }

    /// Subscribe to changes.
    pub fn subscribe<F>(&self, observer: F) -> ObserverId
    where
        F: Fn(&T) + Send + Sync + 'static,
    {
        let id = {
            let mut next_id = self.next_id.write().unwrap();
            let id = ObserverId(*next_id);
            *next_id += 1;
            id
        };

        let mut observers = self.observers.write().unwrap();
        observers.push((id, Box::new(observer)));
        id
    }

    /// Unsubscribe observer.
    pub fn unsubscribe(&self, id: ObserverId) -> bool {
        let mut observers = self.observers.write().unwrap();
        if let Some(pos) = observers.iter().position(|(oid, _)| *oid == id) {
            observers.remove(pos);
            true
        } else {
            false
        }
    }

    /// Notify all observers.
    fn notify(&self)
    where
        T: Clone,
    {
        let value = self.value.read().unwrap().clone();
        let observers = self.observers.read().unwrap();
        for (_, observer) in observers.iter() {
            observer(&value);
        }
    }

    /// Get observer count.
    pub fn observer_count(&self) -> usize {
        self.observers.read().unwrap().len()
    }
}

impl<T: Default> Default for Observable<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

/// Shared observable.
pub type SharedObservable<T> = Arc<Observable<T>>;

/// Create shared observable.
pub fn observable<T>(value: T) -> SharedObservable<T> {
    Arc::new(Observable::new(value))
}

/// Computed value derived from observables.
pub struct Computed<T> {
    value: RwLock<T>,
    compute: Box<dyn Fn() -> T + Send + Sync>,
}

impl<T> Computed<T> {
    /// Create computed value.
    pub fn new<F>(compute: F) -> Self
    where
        F: Fn() -> T + Send + Sync + 'static,
    {
        let value = compute();
        Self {
            value: RwLock::new(value),
            compute: Box::new(compute),
        }
    }

    /// Get current value.
    pub fn get(&self) -> T
    where
        T: Clone,
    {
        self.value.read().unwrap().clone()
    }

    /// Recompute value.
    pub fn recompute(&self) {
        let new_value = (self.compute)();
        *self.value.write().unwrap() = new_value;
    }
}

/// Subject - both observable and observer.
pub struct Subject<T> {
    value: RwLock<Option<T>>,
    observers: RwLock<Vec<Box<dyn Fn(&T) + Send + Sync>>>,
}

impl<T> Subject<T> {
    /// Create new subject.
    pub fn new() -> Self {
        Self {
            value: RwLock::new(None),
            observers: RwLock::new(Vec::new()),
        }
    }

    /// Create subject with initial value.
    pub fn with_value(value: T) -> Self {
        Self {
            value: RwLock::new(Some(value)),
            observers: RwLock::new(Vec::new()),
        }
    }

    /// Push next value.
    pub fn next(&self, value: T)
    where
        T: Clone,
    {
        {
            let mut v = self.value.write().unwrap();
            *v = Some(value);
        }
        let v = self.value.read().unwrap();
        if let Some(ref val) = *v {
            let observers = self.observers.read().unwrap();
            for observer in observers.iter() {
                observer(val);
            }
        }
    }

    /// Subscribe to values.
    pub fn subscribe<F>(&self, observer: F)
    where
        F: Fn(&T) + Send + Sync + 'static,
    {
        let mut observers = self.observers.write().unwrap();
        observers.push(Box::new(observer));
    }

    /// Get last value.
    pub fn value(&self) -> Option<T>
    where
        T: Clone,
    {
        self.value.read().unwrap().clone()
    }
}

impl<T> Default for Subject<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Behavior subject - emits current value to new subscribers.
pub struct BehaviorSubject<T: Clone> {
    value: RwLock<T>,
    observers: RwLock<Vec<Weak<dyn Fn(&T) + Send + Sync>>>,
}

impl<T: Clone> BehaviorSubject<T> {
    /// Create new behavior subject.
    pub fn new(initial: T) -> Self {
        Self {
            value: RwLock::new(initial),
            observers: RwLock::new(Vec::new()),
        }
    }

    /// Push next value.
    pub fn next(&self, value: T) {
        *self.value.write().unwrap() = value.clone();
        let observers = self.observers.read().unwrap();
        for weak in observers.iter() {
            if let Some(observer) = weak.upgrade() {
                observer(&value);
            }
        }
    }

    /// Get current value.
    pub fn value(&self) -> T {
        self.value.read().unwrap().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicI32, Ordering};

    #[test]
    fn test_observable() {
        let obs = Observable::new(0);
        let counter = Arc::new(AtomicI32::new(0));

        let c = counter.clone();
        obs.subscribe(move |v| {
            c.store(*v, Ordering::SeqCst);
        });

        obs.set(42);
        assert_eq!(counter.load(Ordering::SeqCst), 42);
    }

    #[test]
    fn test_unsubscribe() {
        let obs = Observable::new(0);
        let counter = Arc::new(AtomicI32::new(0));

        let c = counter.clone();
        let id = obs.subscribe(move |v| {
            c.store(*v, Ordering::SeqCst);
        });

        obs.set(10);
        assert_eq!(counter.load(Ordering::SeqCst), 10);

        obs.unsubscribe(id);
        obs.set(20);
        assert_eq!(counter.load(Ordering::SeqCst), 10); // Unchanged
    }

    #[test]
    fn test_subject() {
        let subject = Subject::new();
        let counter = Arc::new(AtomicI32::new(0));

        let c = counter.clone();
        subject.subscribe(move |v| {
            c.store(*v, Ordering::SeqCst);
        });

        subject.next(42);
        assert_eq!(counter.load(Ordering::SeqCst), 42);
        assert_eq!(subject.value(), Some(42));
    }

    #[test]
    fn test_computed() {
        let computed = Computed::new(|| 2 + 2);
        assert_eq!(computed.get(), 4);
    }
}
