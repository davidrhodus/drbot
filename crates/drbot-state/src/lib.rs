//! State management utilities for drbot.
//!
//! This crate provides:
//! - State containers
//! - State transitions
//! - State history
//! - Observable state

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use thiserror::Error;

/// State error types.
#[derive(Error, Debug)]
pub enum StateError {
    #[error("Invalid state transition from {from} to {to}")]
    InvalidTransition { from: String, to: String },

    #[error("State locked")]
    Locked,

    #[error("State not found")]
    NotFound,
}

/// Result type for state operations.
pub type Result<T> = std::result::Result<T, StateError>;

/// State container.
pub struct State<T> {
    value: RwLock<T>,
    version: AtomicU64,
}

impl<T> State<T> {
    /// Create new state.
    pub fn new(value: T) -> Self {
        Self {
            value: RwLock::new(value),
            version: AtomicU64::new(0),
        }
    }

    /// Get current value.
    pub fn get(&self) -> T
    where
        T: Clone,
    {
        self.value.read().unwrap().clone()
    }

    /// Get reference to value.
    pub fn with<R, F: FnOnce(&T) -> R>(&self, f: F) -> R {
        f(&self.value.read().unwrap())
    }

    /// Set value.
    pub fn set(&self, value: T) {
        *self.value.write().unwrap() = value;
        self.version.fetch_add(1, Ordering::SeqCst);
    }

    /// Update value with function.
    pub fn update<F: FnOnce(&mut T)>(&self, f: F) {
        f(&mut self.value.write().unwrap());
        self.version.fetch_add(1, Ordering::SeqCst);
    }

    /// Get version.
    pub fn version(&self) -> u64 {
        self.version.load(Ordering::SeqCst)
    }

    /// Swap value.
    pub fn swap(&self, value: T) -> T {
        let old = std::mem::replace(&mut *self.value.write().unwrap(), value);
        self.version.fetch_add(1, Ordering::SeqCst);
        old
    }
}

impl<T: Default> Default for State<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

/// State with history.
pub struct HistoryState<T: Clone> {
    current: T,
    history: Vec<T>,
    max_history: usize,
    position: usize,
}

impl<T: Clone> HistoryState<T> {
    /// Create new history state.
    pub fn new(initial: T, max_history: usize) -> Self {
        Self {
            current: initial.clone(),
            history: vec![initial],
            max_history,
            position: 0,
        }
    }

    /// Get current value.
    pub fn get(&self) -> &T {
        &self.current
    }

    /// Set new value (clears forward history).
    pub fn set(&mut self, value: T) {
        // Truncate forward history
        self.history.truncate(self.position + 1);

        // Add new state
        self.history.push(value.clone());
        self.position += 1;

        // Trim old history
        while self.history.len() > self.max_history {
            self.history.remove(0);
            self.position = self.position.saturating_sub(1);
        }

        self.current = value;
    }

    /// Undo to previous state.
    pub fn undo(&mut self) -> bool {
        if self.position > 0 {
            self.position -= 1;
            self.current = self.history[self.position].clone();
            true
        } else {
            false
        }
    }

    /// Redo to next state.
    pub fn redo(&mut self) -> bool {
        if self.position < self.history.len() - 1 {
            self.position += 1;
            self.current = self.history[self.position].clone();
            true
        } else {
            false
        }
    }

    /// Check if can undo.
    pub fn can_undo(&self) -> bool {
        self.position > 0
    }

    /// Check if can redo.
    pub fn can_redo(&self) -> bool {
        self.position < self.history.len() - 1
    }

    /// Get history length.
    pub fn history_len(&self) -> usize {
        self.history.len()
    }

    /// Clear history keeping current.
    pub fn clear_history(&mut self) {
        let current = self.current.clone();
        self.history.clear();
        self.history.push(current);
        self.position = 0;
    }
}

/// Computed state that derives from other state.
pub struct ComputedState<T, S> {
    source: Arc<State<S>>,
    compute: Box<dyn Fn(&S) -> T + Send + Sync>,
    cached: Mutex<Option<(u64, T)>>,
}

impl<T: Clone, S> ComputedState<T, S> {
    /// Create new computed state.
    pub fn new<F>(source: Arc<State<S>>, compute: F) -> Self
    where
        F: Fn(&S) -> T + Send + Sync + 'static,
    {
        Self {
            source,
            compute: Box::new(compute),
            cached: Mutex::new(None),
        }
    }

    /// Get computed value.
    pub fn get(&self) -> T {
        let version = self.source.version();
        let mut cached = self.cached.lock().unwrap();

        if let Some((v, ref value)) = *cached {
            if v == version {
                return value.clone();
            }
        }

        let value = self.source.with(&self.compute);
        *cached = Some((version, value.clone()));
        value
    }

    /// Force recompute.
    pub fn invalidate(&self) {
        *self.cached.lock().unwrap() = None;
    }
}

/// State store with multiple named states.
pub struct StateStore<T> {
    states: RwLock<std::collections::HashMap<String, T>>,
}

impl<T: Clone> StateStore<T> {
    /// Create new store.
    pub fn new() -> Self {
        Self {
            states: RwLock::new(std::collections::HashMap::new()),
        }
    }

    /// Get state.
    pub fn get(&self, key: &str) -> Option<T> {
        self.states.read().unwrap().get(key).cloned()
    }

    /// Set state.
    pub fn set(&self, key: impl Into<String>, value: T) {
        self.states.write().unwrap().insert(key.into(), value);
    }

    /// Remove state.
    pub fn remove(&self, key: &str) -> Option<T> {
        self.states.write().unwrap().remove(key)
    }

    /// Check if key exists.
    pub fn contains(&self, key: &str) -> bool {
        self.states.read().unwrap().contains_key(key)
    }

    /// Get all keys.
    pub fn keys(&self) -> Vec<String> {
        self.states.read().unwrap().keys().cloned().collect()
    }

    /// Clear all.
    pub fn clear(&self) {
        self.states.write().unwrap().clear();
    }
}

impl<T: Clone> Default for StateStore<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Atomic state for simple types.
pub struct AtomicState<T> {
    inner: Mutex<T>,
}

impl<T> AtomicState<T> {
    /// Create new atomic state.
    pub fn new(value: T) -> Self {
        Self {
            inner: Mutex::new(value),
        }
    }

    /// Get value.
    pub fn get(&self) -> T
    where
        T: Clone,
    {
        self.inner.lock().unwrap().clone()
    }

    /// Set value.
    pub fn set(&self, value: T) {
        *self.inner.lock().unwrap() = value;
    }

    /// Swap value.
    pub fn swap(&self, value: T) -> T {
        std::mem::replace(&mut *self.inner.lock().unwrap(), value)
    }

    /// Update with function.
    pub fn update<F: FnOnce(&mut T)>(&self, f: F) {
        f(&mut self.inner.lock().unwrap());
    }

    /// Compare and swap.
    pub fn compare_swap<F: FnOnce(&T) -> bool>(&self, predicate: F, new_value: T) -> bool {
        let mut guard = self.inner.lock().unwrap();
        if predicate(&*guard) {
            *guard = new_value;
            true
        } else {
            false
        }
    }
}

impl<T: Default> Default for AtomicState<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state() {
        let state = State::new(0);
        assert_eq!(state.get(), 0);

        state.set(5);
        assert_eq!(state.get(), 5);
        assert_eq!(state.version(), 1);

        state.update(|v| *v += 1);
        assert_eq!(state.get(), 6);
    }

    #[test]
    fn test_history_state() {
        let mut state = HistoryState::new(0, 10);

        state.set(1);
        state.set(2);
        state.set(3);

        assert_eq!(*state.get(), 3);

        assert!(state.undo());
        assert_eq!(*state.get(), 2);

        assert!(state.redo());
        assert_eq!(*state.get(), 3);
    }

    #[test]
    fn test_computed_state() {
        let source = Arc::new(State::new(5));
        let computed = ComputedState::new(source.clone(), |v| v * 2);

        assert_eq!(computed.get(), 10);

        source.set(10);
        assert_eq!(computed.get(), 20);
    }

    #[test]
    fn test_state_store() {
        let store = StateStore::new();

        store.set("count", 42);
        assert_eq!(store.get("count"), Some(42));

        store.remove("count");
        assert_eq!(store.get("count"), None);
    }
}
