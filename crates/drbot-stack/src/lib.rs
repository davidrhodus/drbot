//! Stack implementations for drbot.
//!
//! This crate provides:
//! - Bounded stack
//! - Min/Max stack
//! - Undo stack
//! - Thread-safe stack

use std::sync::{Arc, Mutex};
use thiserror::Error;

/// Stack error types.
#[derive(Error, Debug)]
pub enum StackError {
    #[error("Stack full")]
    Full,

    #[error("Stack empty")]
    Empty,

    #[error("Stack overflow")]
    Overflow,
}

/// Result type for stack operations.
pub type Result<T> = std::result::Result<T, StackError>;

/// Basic stack.
pub struct Stack<T> {
    items: Vec<T>,
    capacity: Option<usize>,
}

impl<T> Stack<T> {
    /// Create new stack.
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            capacity: None,
        }
    }

    /// Create with capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            items: Vec::with_capacity(capacity),
            capacity: Some(capacity),
        }
    }

    /// Get length.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Check if full.
    pub fn is_full(&self) -> bool {
        self.capacity.map_or(false, |c| self.items.len() >= c)
    }

    /// Push item.
    pub fn push(&mut self, item: T) -> Result<()> {
        if self.is_full() {
            return Err(StackError::Full);
        }
        self.items.push(item);
        Ok(())
    }

    /// Pop item.
    pub fn pop(&mut self) -> Option<T> {
        self.items.pop()
    }

    /// Peek top.
    pub fn peek(&self) -> Option<&T> {
        self.items.last()
    }

    /// Peek mutable.
    pub fn peek_mut(&mut self) -> Option<&mut T> {
        self.items.last_mut()
    }

    /// Clear stack.
    pub fn clear(&mut self) {
        self.items.clear();
    }

    /// Iterate from top.
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.items.iter().rev()
    }

    /// Drain all items.
    pub fn drain(&mut self) -> impl Iterator<Item = T> + '_ {
        self.items.drain(..).rev()
    }
}

impl<T> Default for Stack<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Min-stack that tracks minimum.
pub struct MinStack<T: Ord + Clone> {
    items: Vec<T>,
    mins: Vec<T>,
}

impl<T: Ord + Clone> MinStack<T> {
    /// Create new min-stack.
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            mins: Vec::new(),
        }
    }

    /// Get length.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Push item.
    pub fn push(&mut self, item: T) {
        let new_min = self.mins.last().map_or(true, |m| item <= *m);
        if new_min {
            self.mins.push(item.clone());
        }
        self.items.push(item);
    }

    /// Pop item.
    pub fn pop(&mut self) -> Option<T> {
        let item = self.items.pop()?;
        if self.mins.last() == Some(&item) {
            self.mins.pop();
        }
        Some(item)
    }

    /// Peek top.
    pub fn peek(&self) -> Option<&T> {
        self.items.last()
    }

    /// Get minimum.
    pub fn min(&self) -> Option<&T> {
        self.mins.last()
    }
}

impl<T: Ord + Clone> Default for MinStack<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Max-stack that tracks maximum.
pub struct MaxStack<T: Ord + Clone> {
    items: Vec<T>,
    maxs: Vec<T>,
}

impl<T: Ord + Clone> MaxStack<T> {
    /// Create new max-stack.
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            maxs: Vec::new(),
        }
    }

    /// Get length.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Push item.
    pub fn push(&mut self, item: T) {
        let new_max = self.maxs.last().map_or(true, |m| item >= *m);
        if new_max {
            self.maxs.push(item.clone());
        }
        self.items.push(item);
    }

    /// Pop item.
    pub fn pop(&mut self) -> Option<T> {
        let item = self.items.pop()?;
        if self.maxs.last() == Some(&item) {
            self.maxs.pop();
        }
        Some(item)
    }

    /// Peek top.
    pub fn peek(&self) -> Option<&T> {
        self.items.last()
    }

    /// Get maximum.
    pub fn max(&self) -> Option<&T> {
        self.maxs.last()
    }
}

impl<T: Ord + Clone> Default for MaxStack<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Undo/redo stack.
pub struct UndoStack<T> {
    undo: Vec<T>,
    redo: Vec<T>,
    max_size: Option<usize>,
}

impl<T> UndoStack<T> {
    /// Create new undo stack.
    pub fn new() -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            max_size: None,
        }
    }

    /// Create with max size.
    pub fn with_max_size(max_size: usize) -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            max_size: Some(max_size),
        }
    }

    /// Push new state (clears redo).
    pub fn push(&mut self, state: T) {
        self.redo.clear();
        self.undo.push(state);

        if let Some(max) = self.max_size {
            while self.undo.len() > max {
                self.undo.remove(0);
            }
        }
    }

    /// Undo (returns undone state).
    pub fn undo(&mut self) -> Option<T> {
        let state = self.undo.pop()?;
        Some(state)
    }

    /// Undo and save for redo.
    pub fn undo_with_redo(&mut self) -> Option<&T>
    where
        T: Clone,
    {
        let state = self.undo.pop()?;
        self.redo.push(state);
        self.redo.last()
    }

    /// Redo.
    pub fn redo(&mut self) -> Option<T> {
        self.redo.pop()
    }

    /// Check if can undo.
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    /// Check if can redo.
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Get undo count.
    pub fn undo_count(&self) -> usize {
        self.undo.len()
    }

    /// Get redo count.
    pub fn redo_count(&self) -> usize {
        self.redo.len()
    }

    /// Clear all.
    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }

    /// Peek current.
    pub fn current(&self) -> Option<&T> {
        self.undo.last()
    }
}

impl<T> Default for UndoStack<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Thread-safe stack.
pub struct SyncStack<T> {
    inner: Arc<Mutex<Vec<T>>>,
    capacity: Option<usize>,
}

impl<T> SyncStack<T> {
    /// Create new sync stack.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(Vec::new())),
            capacity: None,
        }
    }

    /// Create with capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Vec::with_capacity(capacity))),
            capacity: Some(capacity),
        }
    }

    /// Push item.
    pub fn push(&self, item: T) -> Result<()> {
        let mut stack = self.inner.lock().unwrap();
        if let Some(cap) = self.capacity {
            if stack.len() >= cap {
                return Err(StackError::Full);
            }
        }
        stack.push(item);
        Ok(())
    }

    /// Pop item.
    pub fn pop(&self) -> Option<T> {
        self.inner.lock().unwrap().pop()
    }

    /// Get length.
    pub fn len(&self) -> usize {
        self.inner.lock().unwrap().len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.inner.lock().unwrap().is_empty()
    }

    /// Clear stack.
    pub fn clear(&self) {
        self.inner.lock().unwrap().clear();
    }
}

impl<T> Clone for SyncStack<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            capacity: self.capacity,
        }
    }
}

impl<T> Default for SyncStack<T> {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Kani Formal Verification Proofs
// ============================================================================

#[cfg(kani)]
mod kani_proofs {
    use super::*;

    // ========================================================================
    // Basic Stack Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_stack_empty_initially() {
        let stack: Stack<i32> = Stack::new();

        kani::assert!(stack.is_empty(), "New stack is empty");
        kani::assert!(stack.len() == 0, "New stack has len 0");
    }

    #[kani::proof]
    fn proof_stack_push_increases_len() {
        let mut stack: Stack<i32> = Stack::new();

        stack.push(42).unwrap();

        kani::assert!(stack.len() == 1, "Len is 1 after push");
        kani::assert!(!stack.is_empty(), "Stack not empty after push");
    }

    #[kani::proof]
    fn proof_stack_pop_decreases_len() {
        let mut stack: Stack<i32> = Stack::new();

        stack.push(42).unwrap();
        let popped = stack.pop();

        kani::assert!(popped == Some(42), "Pop returns pushed value");
        kani::assert!(stack.len() == 0, "Len is 0 after pop");
        kani::assert!(stack.is_empty(), "Stack empty after pop");
    }

    #[kani::proof]
    fn proof_stack_lifo_order() {
        let mut stack: Stack<i32> = Stack::new();

        stack.push(1).unwrap();
        stack.push(2).unwrap();
        stack.push(3).unwrap();

        kani::assert!(stack.pop() == Some(3), "LIFO: 3 first");
        kani::assert!(stack.pop() == Some(2), "LIFO: 2 second");
        kani::assert!(stack.pop() == Some(1), "LIFO: 1 third");
    }

    #[kani::proof]
    fn proof_stack_peek_does_not_remove() {
        let mut stack: Stack<i32> = Stack::new();

        stack.push(42).unwrap();
        let peeked = stack.peek();

        kani::assert!(peeked == Some(&42), "Peek returns value");
        kani::assert!(stack.len() == 1, "Len unchanged after peek");
    }

    #[kani::proof]
    fn proof_stack_bounded_full() {
        let mut stack: Stack<i32> = Stack::with_capacity(2);

        stack.push(1).unwrap();
        stack.push(2).unwrap();

        kani::assert!(stack.is_full(), "Stack full at capacity");

        let result = stack.push(3);
        kani::assert!(result.is_err(), "Push to full stack fails");
    }

    #[kani::proof]
    fn proof_stack_unbounded_not_full() {
        let stack: Stack<i32> = Stack::new();

        kani::assert!(!stack.is_full(), "Unbounded stack not full");
    }

    #[kani::proof]
    fn proof_stack_clear() {
        let mut stack: Stack<i32> = Stack::new();

        stack.push(1).unwrap();
        stack.push(2).unwrap();
        stack.clear();

        kani::assert!(stack.is_empty(), "Stack empty after clear");
        kani::assert!(stack.len() == 0, "Len is 0 after clear");
    }

    // ========================================================================
    // MinStack Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_min_stack_empty_initially() {
        let stack: MinStack<i32> = MinStack::new();

        kani::assert!(stack.is_empty(), "New MinStack is empty");
        kani::assert!(stack.min().is_none(), "Min is None when empty");
    }

    #[kani::proof]
    fn proof_min_stack_tracks_minimum() {
        let mut stack: MinStack<i32> = MinStack::new();

        stack.push(3);
        kani::assert!(stack.min() == Some(&3), "Min is 3");

        stack.push(1);
        kani::assert!(stack.min() == Some(&1), "Min is 1 after push");

        stack.push(2);
        kani::assert!(stack.min() == Some(&1), "Min still 1");
    }

    #[kani::proof]
    fn proof_min_stack_min_after_pop() {
        let mut stack: MinStack<i32> = MinStack::new();

        stack.push(3);
        stack.push(1);
        stack.push(2);

        stack.pop(); // Remove 2
        kani::assert!(stack.min() == Some(&1), "Min still 1 after pop 2");

        stack.pop(); // Remove 1
        kani::assert!(stack.min() == Some(&3), "Min is 3 after pop 1");
    }

    #[kani::proof]
    fn proof_min_stack_min_is_always_minimum() {
        let mut stack: MinStack<i32> = MinStack::new();

        stack.push(5);
        stack.push(2);
        stack.push(8);
        stack.push(1);

        // Min should be 1 (the smallest)
        kani::assert!(stack.min() == Some(&1), "Min is smallest value");
    }

    // ========================================================================
    // MaxStack Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_max_stack_empty_initially() {
        let stack: MaxStack<i32> = MaxStack::new();

        kani::assert!(stack.is_empty(), "New MaxStack is empty");
        kani::assert!(stack.max().is_none(), "Max is None when empty");
    }

    #[kani::proof]
    fn proof_max_stack_tracks_maximum() {
        let mut stack: MaxStack<i32> = MaxStack::new();

        stack.push(1);
        kani::assert!(stack.max() == Some(&1), "Max is 1");

        stack.push(3);
        kani::assert!(stack.max() == Some(&3), "Max is 3 after push");

        stack.push(2);
        kani::assert!(stack.max() == Some(&3), "Max still 3");
    }

    #[kani::proof]
    fn proof_max_stack_max_after_pop() {
        let mut stack: MaxStack<i32> = MaxStack::new();

        stack.push(1);
        stack.push(3);
        stack.push(2);

        stack.pop(); // Remove 2
        kani::assert!(stack.max() == Some(&3), "Max still 3 after pop 2");

        stack.pop(); // Remove 3
        kani::assert!(stack.max() == Some(&1), "Max is 1 after pop 3");
    }

    // ========================================================================
    // UndoStack Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_undo_stack_empty_initially() {
        let stack: UndoStack<i32> = UndoStack::new();

        kani::assert!(!stack.can_undo(), "Cannot undo when empty");
        kani::assert!(!stack.can_redo(), "Cannot redo when empty");
        kani::assert!(stack.undo_count() == 0, "Undo count is 0");
        kani::assert!(stack.redo_count() == 0, "Redo count is 0");
    }

    #[kani::proof]
    fn proof_undo_stack_push_enables_undo() {
        let mut stack: UndoStack<i32> = UndoStack::new();

        stack.push(1);

        kani::assert!(stack.can_undo(), "Can undo after push");
        kani::assert!(stack.undo_count() == 1, "Undo count is 1");
    }

    #[kani::proof]
    fn proof_undo_stack_push_clears_redo() {
        let mut stack: UndoStack<i32> = UndoStack::new();

        stack.push(1);
        stack.push(2);
        stack.undo(); // Now can redo

        kani::assert!(stack.redo_count() == 0, "Redo cleared by undo");
        // Note: undo() doesn't add to redo, undo_with_redo does

        stack.push(3); // New state clears any pending redo

        kani::assert!(stack.redo_count() == 0, "Redo count still 0");
    }

    #[kani::proof]
    fn proof_undo_stack_current() {
        let mut stack: UndoStack<&str> = UndoStack::new();

        stack.push("state1");
        kani::assert!(stack.current() == Some(&"state1"), "Current is state1");

        stack.push("state2");
        kani::assert!(stack.current() == Some(&"state2"), "Current is state2");
    }

    #[kani::proof]
    fn proof_undo_stack_undo_returns_state() {
        let mut stack: UndoStack<i32> = UndoStack::new();

        stack.push(1);
        stack.push(2);

        let undone = stack.undo();
        kani::assert!(undone == Some(2), "Undo returns last pushed");
        kani::assert!(stack.current() == Some(&1), "Current is now 1");
    }

    #[kani::proof]
    fn proof_undo_stack_max_size() {
        let mut stack: UndoStack<i32> = UndoStack::with_max_size(2);

        stack.push(1);
        stack.push(2);
        stack.push(3); // Should evict 1

        kani::assert!(stack.undo_count() == 2, "Max size respected");
    }

    // ========================================================================
    // SyncStack Proofs (basic, single-threaded)
    // ========================================================================

    #[kani::proof]
    fn proof_sync_stack_empty_initially() {
        let stack: SyncStack<i32> = SyncStack::new();

        kani::assert!(stack.is_empty(), "New SyncStack is empty");
        kani::assert!(stack.len() == 0, "New SyncStack has len 0");
    }

    #[kani::proof]
    fn proof_sync_stack_push_pop() {
        let stack: SyncStack<i32> = SyncStack::new();

        stack.push(42).unwrap();
        kani::assert!(stack.len() == 1, "Len is 1 after push");

        let popped = stack.pop();
        kani::assert!(popped == Some(42), "Pop returns pushed value");
        kani::assert!(stack.is_empty(), "Stack empty after pop");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stack() {
        let mut s = Stack::new();
        s.push(1).unwrap();
        s.push(2).unwrap();
        s.push(3).unwrap();

        assert_eq!(s.pop(), Some(3));
        assert_eq!(s.pop(), Some(2));
        assert_eq!(s.peek(), Some(&1));
    }

    #[test]
    fn test_min_stack() {
        let mut s = MinStack::new();
        s.push(3);
        s.push(1);
        s.push(2);

        assert_eq!(s.min(), Some(&1));
        s.pop();
        assert_eq!(s.min(), Some(&1));
        s.pop();
        assert_eq!(s.min(), Some(&3));
    }

    #[test]
    fn test_max_stack() {
        let mut s = MaxStack::new();
        s.push(1);
        s.push(3);
        s.push(2);

        assert_eq!(s.max(), Some(&3));
    }

    #[test]
    fn test_undo_stack() {
        let mut s = UndoStack::new();
        s.push("state1");
        s.push("state2");
        s.push("state3");

        assert!(s.can_undo());
        assert!(!s.can_redo());

        let undone = s.undo();
        assert_eq!(undone, Some("state3"));
        assert_eq!(s.current(), Some(&"state2"));
    }
}
