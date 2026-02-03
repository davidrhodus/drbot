//! Observer pattern implementation for drbot.
//!
//! This crate provides:
//! - Observable subjects
//! - Observer trait
//! - Subscription management
//! - Event filtering

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use thiserror::Error;

/// Observer error types.
#[derive(Error, Debug)]
pub enum ObserverError {
    #[error("Observer not found")]
    NotFound,

    #[error("Already subscribed")]
    AlreadySubscribed,

    #[error("Subject disposed")]
    Disposed,
}

/// Result type for observer operations.
pub type Result<T> = std::result::Result<T, ObserverError>;

/// Subscription ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SubscriptionId(u64);

impl SubscriptionId {
    /// Generate new ID.
    pub fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        Self(COUNTER.fetch_add(1, Ordering::SeqCst))
    }
}

impl Default for SubscriptionId {
    fn default() -> Self {
        Self::new()
    }
}

/// Observer trait.
pub trait Observer<T>: Send + Sync {
    /// Called when value changes.
    fn on_next(&self, value: &T);

    /// Called on error.
    fn on_error(&self, _error: &str) {}

    /// Called when complete.
    fn on_complete(&self) {}
}

/// Function-based observer.
struct FnObserver<T: Send + Sync, F: Fn(&T) + Send + Sync> {
    callback: F,
    _marker: std::marker::PhantomData<fn() -> T>,
}

impl<T: Send + Sync, F: Fn(&T) + Send + Sync> Observer<T> for FnObserver<T, F> {
    fn on_next(&self, value: &T) {
        (self.callback)(value);
    }
}

/// Subscription handle.
pub struct Subscription {
    id: SubscriptionId,
    unsubscribe: Box<dyn FnOnce() + Send>,
}

impl Subscription {
    /// Get ID.
    pub fn id(&self) -> SubscriptionId {
        self.id
    }

    /// Unsubscribe.
    pub fn unsubscribe(self) {
        (self.unsubscribe)();
    }
}

/// Observable subject.
pub struct Subject<T> {
    observers: Mutex<Vec<(SubscriptionId, Arc<dyn Observer<T>>)>>,
    completed: std::sync::atomic::AtomicBool,
}

impl<T: Clone + Send + Sync + 'static> Subject<T> {
    /// Create new subject.
    pub fn new() -> Self {
        Self {
            observers: Mutex::new(Vec::new()),
            completed: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Subscribe with observer.
    pub fn subscribe(&self, observer: Arc<dyn Observer<T>>) -> Subscription {
        let id = SubscriptionId::new();
        self.observers.lock().unwrap().push((id, observer));

        let observers = Arc::new(Mutex::new(Some(self.observers.lock().unwrap())));
        let observers_weak = Arc::downgrade(&observers);
        drop(observers);

        Subscription {
            id,
            unsubscribe: Box::new(move || {
                // Note: This is a simplified implementation
                // In production, would need proper weak reference handling
                let _ = observers_weak;
            }),
        }
    }

    /// Subscribe with callback.
    pub fn subscribe_fn<F>(&self, callback: F) -> Subscription
    where
        F: Fn(&T) + Send + Sync + 'static,
    {
        let observer = Arc::new(FnObserver {
            callback,
            _marker: std::marker::PhantomData,
        });
        self.subscribe(observer)
    }

    /// Emit value to all observers.
    pub fn next(&self, value: T) {
        if self.completed.load(Ordering::SeqCst) {
            return;
        }

        let observers = self.observers.lock().unwrap();
        for (_, observer) in observers.iter() {
            observer.on_next(&value);
        }
    }

    /// Signal error.
    pub fn error(&self, error: &str) {
        let observers = self.observers.lock().unwrap();
        for (_, observer) in observers.iter() {
            observer.on_error(error);
        }
    }

    /// Signal completion.
    pub fn complete(&self) {
        self.completed.store(true, Ordering::SeqCst);
        let observers = self.observers.lock().unwrap();
        for (_, observer) in observers.iter() {
            observer.on_complete();
        }
    }

    /// Check if completed.
    pub fn is_completed(&self) -> bool {
        self.completed.load(Ordering::SeqCst)
    }

    /// Get observer count.
    pub fn observer_count(&self) -> usize {
        self.observers.lock().unwrap().len()
    }

    /// Unsubscribe by ID.
    pub fn unsubscribe(&self, id: SubscriptionId) -> bool {
        let mut observers = self.observers.lock().unwrap();
        let len_before = observers.len();
        observers.retain(|(i, _)| *i != id);
        observers.len() < len_before
    }
}

impl<T: Clone + Send + Sync + 'static> Default for Subject<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Behavior subject (remembers last value).
pub struct BehaviorSubject<T: Clone> {
    subject: Subject<T>,
    current: Mutex<T>,
}

impl<T: Clone + Send + Sync + 'static> BehaviorSubject<T> {
    /// Create new behavior subject.
    pub fn new(initial: T) -> Self {
        Self {
            subject: Subject::new(),
            current: Mutex::new(initial),
        }
    }

    /// Get current value.
    pub fn value(&self) -> T {
        self.current.lock().unwrap().clone()
    }

    /// Subscribe (immediately receives current value).
    pub fn subscribe_fn<F>(&self, callback: F) -> Subscription
    where
        F: Fn(&T) + Send + Sync + 'static,
    {
        // Send current value immediately
        let current = self.current.lock().unwrap().clone();
        callback(&current);

        self.subject.subscribe_fn(callback)
    }

    /// Emit new value.
    pub fn next(&self, value: T) {
        *self.current.lock().unwrap() = value.clone();
        self.subject.next(value);
    }

    /// Complete.
    pub fn complete(&self) {
        self.subject.complete();
    }
}

/// Replay subject (remembers N last values).
pub struct ReplaySubject<T: Clone> {
    subject: Subject<T>,
    buffer: Mutex<Vec<T>>,
    buffer_size: usize,
}

impl<T: Clone + Send + Sync + 'static> ReplaySubject<T> {
    /// Create new replay subject.
    pub fn new(buffer_size: usize) -> Self {
        Self {
            subject: Subject::new(),
            buffer: Mutex::new(Vec::with_capacity(buffer_size)),
            buffer_size,
        }
    }

    /// Subscribe (receives buffered values).
    pub fn subscribe_fn<F>(&self, callback: F) -> Subscription
    where
        F: Fn(&T) + Send + Sync + 'static,
    {
        // Send buffered values
        let buffer = self.buffer.lock().unwrap();
        for value in buffer.iter() {
            callback(value);
        }
        drop(buffer);

        self.subject.subscribe_fn(callback)
    }

    /// Emit new value.
    pub fn next(&self, value: T) {
        let mut buffer = self.buffer.lock().unwrap();
        buffer.push(value.clone());
        while buffer.len() > self.buffer_size {
            buffer.remove(0);
        }
        drop(buffer);

        self.subject.next(value);
    }

    /// Complete.
    pub fn complete(&self) {
        self.subject.complete();
    }
}

/// Observable property.
pub struct Property<T: Clone> {
    value: Mutex<T>,
    observers: Mutex<Vec<Arc<dyn Fn(&T) + Send + Sync>>>,
}

impl<T: Clone> Property<T> {
    /// Create new property.
    pub fn new(value: T) -> Self {
        Self {
            value: Mutex::new(value),
            observers: Mutex::new(Vec::new()),
        }
    }

    /// Get value.
    pub fn get(&self) -> T {
        self.value.lock().unwrap().clone()
    }

    /// Set value (notifies observers).
    pub fn set(&self, value: T) {
        *self.value.lock().unwrap() = value.clone();

        let observers = self.observers.lock().unwrap();
        for observer in observers.iter() {
            observer(&value);
        }
    }

    /// Observe changes.
    pub fn observe<F>(&self, callback: F)
    where
        F: Fn(&T) + Send + Sync + 'static,
    {
        self.observers.lock().unwrap().push(Arc::new(callback));
    }

    /// Update with function.
    pub fn update<F: FnOnce(&mut T)>(&self, f: F) {
        let mut value = self.value.lock().unwrap();
        f(&mut value);
        let new_value = value.clone();
        drop(value);

        let observers = self.observers.lock().unwrap();
        for observer in observers.iter() {
            observer(&new_value);
        }
    }
}

impl<T: Clone + Default> Default for Property<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicI32;

    #[test]
    fn test_subject() {
        let subject = Subject::new();
        let counter = Arc::new(AtomicI32::new(0));
        let counter_clone = counter.clone();

        subject.subscribe_fn(move |v: &i32| {
            counter_clone.fetch_add(*v, Ordering::SeqCst);
        });

        subject.next(5);
        subject.next(3);

        assert_eq!(counter.load(Ordering::SeqCst), 8);
    }

    #[test]
    fn test_behavior_subject() {
        let subject = BehaviorSubject::new(10);
        let received = Arc::new(AtomicI32::new(0));
        let received_clone = received.clone();

        // Should immediately receive current value
        subject.subscribe_fn(move |v: &i32| {
            received_clone.store(*v, Ordering::SeqCst);
        });

        assert_eq!(received.load(Ordering::SeqCst), 10);

        subject.next(20);
        assert_eq!(received.load(Ordering::SeqCst), 20);
    }

    #[test]
    fn test_property() {
        let prop = Property::new(0);
        let observed = Arc::new(AtomicI32::new(0));
        let observed_clone = observed.clone();

        prop.observe(move |v: &i32| {
            observed_clone.store(*v, Ordering::SeqCst);
        });

        prop.set(42);
        assert_eq!(observed.load(Ordering::SeqCst), 42);
    }
}
