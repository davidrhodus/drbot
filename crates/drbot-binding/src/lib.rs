//! Data binding utilities for drbot.
//!
//! This crate provides:
//! - One-way binding
//! - Two-way binding
//! - Binding expressions
//! - Binding groups

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};
use thiserror::Error;

/// Binding error types.
#[derive(Error, Debug)]
pub enum BindingError {
    #[error("Binding broken")]
    Broken,

    #[error("Source unavailable")]
    SourceUnavailable,

    #[error("Target unavailable")]
    TargetUnavailable,

    #[error("Conversion failed")]
    ConversionFailed,
}

/// Result type for binding operations.
pub type Result<T> = std::result::Result<T, BindingError>;

/// Binding ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BindingId(u64);

impl BindingId {
    /// Generate new ID.
    pub fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        Self(COUNTER.fetch_add(1, Ordering::SeqCst))
    }
}

impl Default for BindingId {
    fn default() -> Self {
        Self::new()
    }
}

/// Bindable value trait.
pub trait Bindable: Send + Sync {
    /// Value type.
    type Value: Clone;

    /// Get value.
    fn get(&self) -> Self::Value;

    /// Set value.
    fn set(&self, value: Self::Value);

    /// Subscribe to changes.
    fn subscribe(&self, callback: Arc<dyn Fn() + Send + Sync>);
}

/// Simple bindable wrapper.
pub struct BindableValue<T: Clone + Send + Sync> {
    value: Mutex<T>,
    subscribers: Mutex<Vec<Weak<dyn Fn() + Send + Sync>>>,
}

impl<T: Clone + Send + Sync> BindableValue<T> {
    /// Create new bindable value.
    pub fn new(value: T) -> Arc<Self> {
        Arc::new(Self {
            value: Mutex::new(value),
            subscribers: Mutex::new(Vec::new()),
        })
    }

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

impl<T: Clone + Send + Sync + 'static> Bindable for BindableValue<T> {
    type Value = T;

    fn get(&self) -> T {
        self.value.lock().unwrap().clone()
    }

    fn set(&self, value: T) {
        *self.value.lock().unwrap() = value;
        self.notify();
    }

    fn subscribe(&self, callback: Arc<dyn Fn() + Send + Sync>) {
        self.subscribers
            .lock()
            .unwrap()
            .push(Arc::downgrade(&callback));
    }
}

/// One-way binding (source to target).
pub struct OneWayBinding<S: Bindable, T: Bindable> {
    id: BindingId,
    source: Arc<S>,
    target: Arc<T>,
    active: AtomicBool,
    _callback: Arc<dyn Fn() + Send + Sync>,
}

impl<S: Bindable + 'static, T: Bindable<Value = S::Value> + 'static> OneWayBinding<S, T> {
    /// Create new one-way binding.
    pub fn new(source: Arc<S>, target: Arc<T>) -> Arc<Self> {
        let target_clone = target.clone();
        let source_clone = source.clone();

        let callback: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
            let value = source_clone.get();
            target_clone.set(value);
        });

        // Initial sync
        target.set(source.get());

        // Subscribe to source changes
        source.subscribe(callback.clone());

        Arc::new(Self {
            id: BindingId::new(),
            source,
            target,
            active: AtomicBool::new(true),
            _callback: callback,
        })
    }

    /// Get ID.
    pub fn id(&self) -> BindingId {
        self.id
    }

    /// Check if active.
    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::SeqCst)
    }

    /// Deactivate binding.
    pub fn deactivate(&self) {
        self.active.store(false, Ordering::SeqCst);
    }

    /// Sync manually.
    pub fn sync(&self) {
        if self.is_active() {
            self.target.set(self.source.get());
        }
    }
}

/// Two-way binding.
pub struct TwoWayBinding<A: Bindable, B: Bindable> {
    id: BindingId,
    a: Arc<A>,
    b: Arc<B>,
    active: AtomicBool,
    updating: Arc<AtomicBool>,
    _callback_a: Arc<dyn Fn() + Send + Sync>,
    _callback_b: Arc<dyn Fn() + Send + Sync>,
}

impl<A: Bindable + 'static, B: Bindable<Value = A::Value> + 'static> TwoWayBinding<A, B> {
    /// Create new two-way binding.
    pub fn new(a: Arc<A>, b: Arc<B>) -> Arc<Self> {
        let updating = Arc::new(AtomicBool::new(false));

        let b_clone = b.clone();
        let a_clone_for_a = a.clone();
        let updating_a = updating.clone();

        let callback_a: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
            if !updating_a.swap(true, Ordering::SeqCst) {
                b_clone.set(a_clone_for_a.get());
                updating_a.store(false, Ordering::SeqCst);
            }
        });

        let a_clone = a.clone();
        let b_clone_for_b = b.clone();
        let updating_b = updating.clone();

        let callback_b: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
            if !updating_b.swap(true, Ordering::SeqCst) {
                a_clone.set(b_clone_for_b.get());
                updating_b.store(false, Ordering::SeqCst);
            }
        });

        // Initial sync
        b.set(a.get());

        // Subscribe both ways
        a.subscribe(callback_a.clone());
        b.subscribe(callback_b.clone());

        Arc::new(Self {
            id: BindingId::new(),
            a,
            b,
            active: AtomicBool::new(true),
            updating,
            _callback_a: callback_a,
            _callback_b: callback_b,
        })
    }

    /// Get ID.
    pub fn id(&self) -> BindingId {
        self.id
    }

    /// Check if active.
    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::SeqCst)
    }

    /// Deactivate binding.
    pub fn deactivate(&self) {
        self.active.store(false, Ordering::SeqCst);
    }
}

/// Binding with conversion.
pub struct ConvertingBinding<S, T, C>
where
    S: Bindable,
    T: Bindable,
    C: Fn(S::Value) -> T::Value + Send + Sync,
{
    id: BindingId,
    source: Arc<S>,
    target: Arc<T>,
    _converter: C,
    _callback: Arc<dyn Fn() + Send + Sync>,
}

impl<S, T, C> ConvertingBinding<S, T, C>
where
    S: Bindable + 'static,
    T: Bindable + 'static,
    C: Fn(S::Value) -> T::Value + Send + Sync + Clone + 'static,
{
    /// Create new converting binding.
    pub fn new(source: Arc<S>, target: Arc<T>, converter: C) -> Arc<Self> {
        let target_clone = target.clone();
        let source_clone = source.clone();
        let converter_clone = converter.clone();

        let callback: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
            let value = source_clone.get();
            let converted = converter_clone(value);
            target_clone.set(converted);
        });

        // Initial sync
        target.set(converter(source.get()));

        // Subscribe
        source.subscribe(callback.clone());

        Arc::new(Self {
            id: BindingId::new(),
            source,
            target,
            _converter: converter,
            _callback: callback,
        })
    }

    /// Get ID.
    pub fn id(&self) -> BindingId {
        self.id
    }
}

/// Binding group.
pub struct BindingGroup {
    bindings: Mutex<Vec<Box<dyn std::any::Any + Send + Sync>>>,
}

impl BindingGroup {
    /// Create new group.
    pub fn new() -> Self {
        Self {
            bindings: Mutex::new(Vec::new()),
        }
    }

    /// Add binding.
    pub fn add<T: std::any::Any + Send + Sync + 'static>(&self, binding: Arc<T>) {
        self.bindings.lock().unwrap().push(Box::new(binding));
    }

    /// Clear all bindings.
    pub fn clear(&self) {
        self.bindings.lock().unwrap().clear();
    }

    /// Get count.
    pub fn count(&self) -> usize {
        self.bindings.lock().unwrap().len()
    }
}

impl Default for BindingGroup {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bindable_value() {
        let value = BindableValue::new(42);
        assert_eq!(value.get(), 42);

        value.set(100);
        assert_eq!(value.get(), 100);
    }

    #[test]
    fn test_one_way_binding() {
        let source = BindableValue::new(0);
        let target = BindableValue::new(0);

        let _binding = OneWayBinding::new(source.clone(), target.clone());

        // Initial sync
        assert_eq!(target.get(), 0);

        source.set(42);
        // Note: In this test the callback is synchronous
        assert_eq!(target.get(), 42);
    }

    #[test]
    fn test_two_way_binding() {
        let a = BindableValue::new(0);
        let b = BindableValue::new(0);

        let _binding = TwoWayBinding::new(a.clone(), b.clone());

        a.set(42);
        assert_eq!(b.get(), 42);

        b.set(100);
        assert_eq!(a.get(), 100);
    }

    #[test]
    fn test_converting_binding() {
        let source = BindableValue::new(10);
        let target = BindableValue::new(String::new());

        let _binding =
            ConvertingBinding::new(source.clone(), target.clone(), |v: i32| v.to_string());

        assert_eq!(target.get(), "10");

        source.set(42);
        assert_eq!(target.get(), "42");
    }
}
