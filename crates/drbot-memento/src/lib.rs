//! Memento pattern utilities for drbot.
//!
//! This crate provides:
//! - Memento for capturing state
//! - Caretaker for managing mementos
//! - State snapshots

use std::collections::VecDeque;
use thiserror::Error;

/// Memento error types.
#[derive(Error, Debug)]
pub enum MementoError {
    #[error("No memento available")]
    NoMemento,

    #[error("Restore failed: {0}")]
    RestoreFailed(String),

    #[error("Invalid memento")]
    Invalid,
}

/// Result type for memento operations.
pub type Result<T> = std::result::Result<T, MementoError>;

/// Memento that stores state.
#[derive(Debug, Clone)]
pub struct Memento<S: Clone> {
    state: S,
    timestamp: std::time::Instant,
    label: Option<String>,
}

impl<S: Clone> Memento<S> {
    /// Create new memento.
    pub fn new(state: S) -> Self {
        Self {
            state,
            timestamp: std::time::Instant::now(),
            label: None,
        }
    }

    /// Create labeled memento.
    pub fn with_label(state: S, label: impl Into<String>) -> Self {
        Self {
            state,
            timestamp: std::time::Instant::now(),
            label: Some(label.into()),
        }
    }

    /// Get state.
    pub fn state(&self) -> &S {
        &self.state
    }

    /// Get state (consuming).
    pub fn into_state(self) -> S {
        self.state
    }

    /// Get timestamp.
    pub fn timestamp(&self) -> std::time::Instant {
        self.timestamp
    }

    /// Get age.
    pub fn age(&self) -> std::time::Duration {
        self.timestamp.elapsed()
    }

    /// Get label.
    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }
}

/// Originator trait for objects that can save/restore state.
pub trait Originator: Clone {
    /// Create memento of current state.
    fn save(&self) -> Memento<Self> {
        Memento::new(self.clone())
    }

    /// Restore from memento.
    fn restore(&mut self, memento: &Memento<Self>) {
        *self = memento.state().clone();
    }
}

// Blanket implementation
impl<T: Clone> Originator for T {}

/// Caretaker that manages mementos.
pub struct Caretaker<S: Clone> {
    mementos: VecDeque<Memento<S>>,
    max_size: usize,
}

impl<S: Clone> Caretaker<S> {
    /// Create new caretaker.
    pub fn new(max_size: usize) -> Self {
        Self {
            mementos: VecDeque::new(),
            max_size,
        }
    }

    /// Save memento.
    pub fn save(&mut self, memento: Memento<S>) {
        self.mementos.push_back(memento);
        while self.mementos.len() > self.max_size {
            self.mementos.pop_front();
        }
    }

    /// Get latest memento.
    pub fn latest(&self) -> Option<&Memento<S>> {
        self.mementos.back()
    }

    /// Pop and return latest memento.
    pub fn pop(&mut self) -> Option<Memento<S>> {
        self.mementos.pop_back()
    }

    /// Get memento by index (0 = oldest).
    pub fn get(&self, index: usize) -> Option<&Memento<S>> {
        self.mementos.get(index)
    }

    /// Get memento by label.
    pub fn find_by_label(&self, label: &str) -> Option<&Memento<S>> {
        self.mementos
            .iter()
            .rev()
            .find(|m| m.label() == Some(label))
    }

    /// Get count.
    pub fn len(&self) -> usize {
        self.mementos.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.mementos.is_empty()
    }

    /// Clear all mementos.
    pub fn clear(&mut self) {
        self.mementos.clear();
    }
}

impl<S: Clone> Default for Caretaker<S> {
    fn default() -> Self {
        Self::new(100)
    }
}

/// State manager with undo/redo support.
pub struct StateManager<S: Clone> {
    current: S,
    history: VecDeque<Memento<S>>,
    future: VecDeque<Memento<S>>,
    max_history: usize,
}

impl<S: Clone> StateManager<S> {
    /// Create new state manager.
    pub fn new(initial: S, max_history: usize) -> Self {
        Self {
            current: initial,
            history: VecDeque::new(),
            future: VecDeque::new(),
            max_history,
        }
    }

    /// Get current state.
    pub fn current(&self) -> &S {
        &self.current
    }

    /// Get mutable current state.
    pub fn current_mut(&mut self) -> &mut S {
        &mut self.current
    }

    /// Save checkpoint before modification.
    pub fn checkpoint(&mut self) {
        self.checkpoint_labeled(None::<&str>);
    }

    /// Save labeled checkpoint.
    pub fn checkpoint_labeled(&mut self, label: Option<impl Into<String>>) {
        let memento = match label {
            Some(l) => Memento::with_label(self.current.clone(), l),
            None => Memento::new(self.current.clone()),
        };
        self.history.push_back(memento);
        while self.history.len() > self.max_history {
            self.history.pop_front();
        }
        self.future.clear();
    }

    /// Set new state (saves current to history).
    pub fn set(&mut self, state: S) {
        self.checkpoint();
        self.current = state;
    }

    /// Undo to previous state.
    pub fn undo(&mut self) -> Result<()> {
        let prev = self.history.pop_back().ok_or(MementoError::NoMemento)?;
        let current_memento = Memento::new(self.current.clone());
        self.future.push_back(current_memento);
        self.current = prev.into_state();
        Ok(())
    }

    /// Redo to next state.
    pub fn redo(&mut self) -> Result<()> {
        let next = self.future.pop_back().ok_or(MementoError::NoMemento)?;
        let current_memento = Memento::new(self.current.clone());
        self.history.push_back(current_memento);
        self.current = next.into_state();
        Ok(())
    }

    /// Check if can undo.
    pub fn can_undo(&self) -> bool {
        !self.history.is_empty()
    }

    /// Check if can redo.
    pub fn can_redo(&self) -> bool {
        !self.future.is_empty()
    }

    /// Get history length.
    pub fn history_len(&self) -> usize {
        self.history.len()
    }

    /// Get redo length.
    pub fn redo_len(&self) -> usize {
        self.future.len()
    }

    /// Reset to initial state (clears history).
    pub fn reset(&mut self, state: S) {
        self.current = state;
        self.history.clear();
        self.future.clear();
    }
}

/// Snapshot store for periodic snapshots.
pub struct SnapshotStore<S: Clone> {
    snapshots: VecDeque<Memento<S>>,
    max_snapshots: usize,
}

impl<S: Clone> SnapshotStore<S> {
    /// Create new snapshot store.
    pub fn new(max_snapshots: usize) -> Self {
        Self {
            snapshots: VecDeque::new(),
            max_snapshots,
        }
    }

    /// Take snapshot.
    pub fn snapshot(&mut self, state: &S) {
        self.snapshot_labeled(state, None::<&str>);
    }

    /// Take labeled snapshot.
    pub fn snapshot_labeled(&mut self, state: &S, label: Option<impl Into<String>>) {
        let memento = match label {
            Some(l) => Memento::with_label(state.clone(), l),
            None => Memento::new(state.clone()),
        };
        self.snapshots.push_back(memento);
        while self.snapshots.len() > self.max_snapshots {
            self.snapshots.pop_front();
        }
    }

    /// Get latest snapshot.
    pub fn latest(&self) -> Option<&Memento<S>> {
        self.snapshots.back()
    }

    /// Get snapshot by index.
    pub fn get(&self, index: usize) -> Option<&Memento<S>> {
        self.snapshots.get(index)
    }

    /// Get all snapshots.
    pub fn all(&self) -> impl Iterator<Item = &Memento<S>> {
        self.snapshots.iter()
    }

    /// Get snapshot count.
    pub fn len(&self) -> usize {
        self.snapshots.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.snapshots.is_empty()
    }

    /// Clear all snapshots.
    pub fn clear(&mut self) {
        self.snapshots.clear();
    }
}

impl<S: Clone> Default for SnapshotStore<S> {
    fn default() -> Self {
        Self::new(10)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memento() {
        let memento = Memento::with_label(42, "answer");
        assert_eq!(*memento.state(), 42);
        assert_eq!(memento.label(), Some("answer"));
    }

    #[test]
    fn test_caretaker() {
        let mut caretaker = Caretaker::new(3);

        caretaker.save(Memento::new(1));
        caretaker.save(Memento::new(2));
        caretaker.save(Memento::new(3));
        caretaker.save(Memento::new(4)); // 1 should be removed

        assert_eq!(caretaker.len(), 3);
        assert_eq!(*caretaker.get(0).unwrap().state(), 2);
        assert_eq!(*caretaker.latest().unwrap().state(), 4);
    }

    #[test]
    fn test_state_manager() {
        let mut manager = StateManager::new("initial".to_string(), 10);

        manager.set("first".to_string());
        manager.set("second".to_string());
        manager.set("third".to_string());

        assert_eq!(manager.current(), "third");

        manager.undo().unwrap();
        assert_eq!(manager.current(), "second");

        manager.redo().unwrap();
        assert_eq!(manager.current(), "third");
    }

    #[test]
    fn test_snapshot_store() {
        let mut store = SnapshotStore::new(5);

        store.snapshot(&vec![1, 2, 3]);
        store.snapshot_labeled(&vec![4, 5, 6], Some("checkpoint"));

        assert_eq!(store.len(), 2);
        assert_eq!(store.latest().unwrap().state(), &vec![4, 5, 6]);
    }
}
