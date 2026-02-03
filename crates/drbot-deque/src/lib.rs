//! Double-ended queue utilities for drbot.
//!
//! This crate provides:
//! - Bounded deque
//! - Priority deque
//! - Steal-capable deque
//! - Deque utilities

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use thiserror::Error;

/// Deque error types.
#[derive(Error, Debug)]
pub enum DequeError {
    #[error("Deque full")]
    Full,

    #[error("Deque empty")]
    Empty,
}

/// Result type for deque operations.
pub type Result<T> = std::result::Result<T, DequeError>;

/// Bounded deque.
pub struct BoundedDeque<T> {
    inner: VecDeque<T>,
    capacity: usize,
}

impl<T> BoundedDeque<T> {
    /// Create new bounded deque.
    pub fn new(capacity: usize) -> Self {
        Self {
            inner: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    /// Get capacity.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Get length.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Check if full.
    pub fn is_full(&self) -> bool {
        self.inner.len() >= self.capacity
    }

    /// Push to front.
    pub fn push_front(&mut self, item: T) -> Result<()> {
        if self.is_full() {
            return Err(DequeError::Full);
        }
        self.inner.push_front(item);
        Ok(())
    }

    /// Push to back.
    pub fn push_back(&mut self, item: T) -> Result<()> {
        if self.is_full() {
            return Err(DequeError::Full);
        }
        self.inner.push_back(item);
        Ok(())
    }

    /// Pop from front.
    pub fn pop_front(&mut self) -> Option<T> {
        self.inner.pop_front()
    }

    /// Pop from back.
    pub fn pop_back(&mut self) -> Option<T> {
        self.inner.pop_back()
    }

    /// Peek front.
    pub fn front(&self) -> Option<&T> {
        self.inner.front()
    }

    /// Peek back.
    pub fn back(&self) -> Option<&T> {
        self.inner.back()
    }

    /// Clear deque.
    pub fn clear(&mut self) {
        self.inner.clear();
    }

    /// Iterate.
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.inner.iter()
    }

    /// Drain all items.
    pub fn drain(&mut self) -> impl Iterator<Item = T> + '_ {
        self.inner.drain(..)
    }
}

impl<T> Default for BoundedDeque<T> {
    fn default() -> Self {
        Self::new(16)
    }
}

/// Priority item.
struct PriorityItem<T> {
    item: T,
    priority: i32,
}

/// Priority deque (higher priority first).
pub struct PriorityDeque<T> {
    items: Vec<PriorityItem<T>>,
    capacity: Option<usize>,
}

impl<T> PriorityDeque<T> {
    /// Create new priority deque.
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

    /// Push with priority.
    pub fn push(&mut self, item: T, priority: i32) -> Result<()> {
        if let Some(cap) = self.capacity {
            if self.items.len() >= cap {
                return Err(DequeError::Full);
            }
        }

        // Find insertion point
        let pos = self
            .items
            .iter()
            .position(|i| i.priority < priority)
            .unwrap_or(self.items.len());

        self.items.insert(pos, PriorityItem { item, priority });
        Ok(())
    }

    /// Pop highest priority.
    pub fn pop(&mut self) -> Option<T> {
        if self.items.is_empty() {
            None
        } else {
            Some(self.items.remove(0).item)
        }
    }

    /// Pop lowest priority.
    pub fn pop_back(&mut self) -> Option<T> {
        self.items.pop().map(|i| i.item)
    }

    /// Peek highest priority.
    pub fn peek(&self) -> Option<&T> {
        self.items.first().map(|i| &i.item)
    }

    /// Peek lowest priority.
    pub fn peek_back(&self) -> Option<&T> {
        self.items.last().map(|i| &i.item)
    }

    /// Clear deque.
    pub fn clear(&mut self) {
        self.items.clear();
    }
}

impl<T> Default for PriorityDeque<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Work-stealing deque.
pub struct WorkStealingDeque<T> {
    local: Mutex<VecDeque<T>>,
}

impl<T> WorkStealingDeque<T> {
    /// Create new work-stealing deque.
    pub fn new() -> Self {
        Self {
            local: Mutex::new(VecDeque::new()),
        }
    }

    /// Push to back (local end).
    pub fn push(&self, item: T) {
        self.local.lock().unwrap().push_back(item);
    }

    /// Pop from back (local end).
    pub fn pop(&self) -> Option<T> {
        self.local.lock().unwrap().pop_back()
    }

    /// Steal from front (remote end).
    pub fn steal(&self) -> Option<T> {
        self.local.lock().unwrap().pop_front()
    }

    /// Steal batch.
    pub fn steal_batch(&self, max: usize) -> Vec<T> {
        let mut local = self.local.lock().unwrap();
        let count = max.min(local.len() / 2).max(1).min(local.len());
        local.drain(..count).collect()
    }

    /// Get length.
    pub fn len(&self) -> usize {
        self.local.lock().unwrap().len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.local.lock().unwrap().is_empty()
    }
}

impl<T> Default for WorkStealingDeque<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Shared deque.
pub struct SharedDeque<T> {
    inner: Arc<Mutex<VecDeque<T>>>,
}

impl<T> SharedDeque<T> {
    /// Create new shared deque.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(VecDeque::new())),
        }
    }

    /// Push back.
    pub fn push_back(&self, item: T) {
        self.inner.lock().unwrap().push_back(item);
    }

    /// Push front.
    pub fn push_front(&self, item: T) {
        self.inner.lock().unwrap().push_front(item);
    }

    /// Pop front.
    pub fn pop_front(&self) -> Option<T> {
        self.inner.lock().unwrap().pop_front()
    }

    /// Pop back.
    pub fn pop_back(&self) -> Option<T> {
        self.inner.lock().unwrap().pop_back()
    }

    /// Get length.
    pub fn len(&self) -> usize {
        self.inner.lock().unwrap().len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.inner.lock().unwrap().is_empty()
    }

    /// Clear.
    pub fn clear(&self) {
        self.inner.lock().unwrap().clear();
    }
}

impl<T> Clone for SharedDeque<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<T> Default for SharedDeque<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Sliding window deque.
pub struct SlidingWindow<T> {
    inner: VecDeque<T>,
    size: usize,
}

impl<T> SlidingWindow<T> {
    /// Create new sliding window.
    pub fn new(size: usize) -> Self {
        Self {
            inner: VecDeque::with_capacity(size),
            size,
        }
    }

    /// Push item, removing oldest if full.
    pub fn push(&mut self, item: T) -> Option<T> {
        let removed = if self.inner.len() >= self.size {
            self.inner.pop_front()
        } else {
            None
        };
        self.inner.push_back(item);
        removed
    }

    /// Get window size.
    pub fn size(&self) -> usize {
        self.size
    }

    /// Get current length.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Check if full.
    pub fn is_full(&self) -> bool {
        self.inner.len() >= self.size
    }

    /// Get items.
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.inner.iter()
    }

    /// Get as slice.
    pub fn as_slices(&self) -> (&[T], &[T]) {
        self.inner.as_slices()
    }

    /// Clear window.
    pub fn clear(&mut self) {
        self.inner.clear();
    }
}

impl<T> Default for SlidingWindow<T> {
    fn default() -> Self {
        Self::new(10)
    }
}

// ============================================================================
// Kani Formal Verification Proofs
// ============================================================================

#[cfg(kani)]
mod kani_proofs {
    use super::*;

    // ========================================================================
    // BoundedDeque Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_bounded_deque_len_capacity_invariant() {
        let capacity: usize = kani::any();
        kani::assume(capacity > 0 && capacity <= 16);

        let deque: BoundedDeque<u8> = BoundedDeque::new(capacity);
        kani::assert!(deque.len() <= deque.capacity(), "len <= capacity initially");
    }

    #[kani::proof]
    fn proof_bounded_deque_is_full_logic() {
        let capacity: usize = kani::any();
        kani::assume(capacity > 0 && capacity <= 8);

        let mut deque: BoundedDeque<u8> = BoundedDeque::new(capacity);

        // Fill the deque
        for _ in 0..capacity {
            let _ = deque.push_back(1);
        }

        kani::assert!(deque.is_full(), "Deque is full when len == capacity");
        kani::assert!(deque.len() == capacity, "len equals capacity when full");
    }

    #[kani::proof]
    fn proof_bounded_deque_is_empty_logic() {
        let capacity: usize = kani::any();
        kani::assume(capacity > 0 && capacity <= 16);

        let deque: BoundedDeque<u8> = BoundedDeque::new(capacity);
        kani::assert!(deque.is_empty(), "New deque is empty");
        kani::assert!(deque.len() == 0, "Empty deque has len 0");
    }

    #[kani::proof]
    fn proof_bounded_deque_push_back_increases_len() {
        let mut deque: BoundedDeque<u8> = BoundedDeque::new(4);
        let initial_len = deque.len();

        let result = deque.push_back(42);

        if result.is_ok() {
            kani::assert!(deque.len() == initial_len + 1, "push_back increases len");
        }
    }

    #[kani::proof]
    fn proof_bounded_deque_push_front_increases_len() {
        let mut deque: BoundedDeque<u8> = BoundedDeque::new(4);
        let initial_len = deque.len();

        let result = deque.push_front(42);

        if result.is_ok() {
            kani::assert!(deque.len() == initial_len + 1, "push_front increases len");
        }
    }

    #[kani::proof]
    fn proof_bounded_deque_pop_front_decreases_len() {
        let mut deque: BoundedDeque<u8> = BoundedDeque::new(4);
        deque.push_back(1).unwrap();
        let initial_len = deque.len();

        let result = deque.pop_front();

        if result.is_some() {
            kani::assert!(deque.len() == initial_len - 1, "pop_front decreases len");
        }
    }

    #[kani::proof]
    fn proof_bounded_deque_pop_back_decreases_len() {
        let mut deque: BoundedDeque<u8> = BoundedDeque::new(4);
        deque.push_back(1).unwrap();
        let initial_len = deque.len();

        let result = deque.pop_back();

        if result.is_some() {
            kani::assert!(deque.len() == initial_len - 1, "pop_back decreases len");
        }
    }

    #[kani::proof]
    fn proof_bounded_deque_full_rejects_push() {
        let mut deque: BoundedDeque<u8> = BoundedDeque::new(2);
        deque.push_back(1).unwrap();
        deque.push_back(2).unwrap();

        kani::assert!(deque.is_full(), "Deque is full");

        let result = deque.push_back(3);
        kani::assert!(result.is_err(), "Push to full deque fails");
    }

    #[kani::proof]
    fn proof_bounded_deque_default_capacity() {
        let deque: BoundedDeque<u8> = BoundedDeque::default();
        kani::assert!(deque.capacity() == 16, "Default capacity is 16");
    }

    // ========================================================================
    // SlidingWindow Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_sliding_window_len_size_invariant() {
        let size: usize = kani::any();
        kani::assume(size > 0 && size <= 8);

        let window: SlidingWindow<u8> = SlidingWindow::new(size);
        kani::assert!(window.len() <= window.size(), "len <= size initially");
    }

    #[kani::proof]
    fn proof_sliding_window_is_full_logic() {
        let size: usize = kani::any();
        kani::assume(size > 0 && size <= 4);

        let mut window: SlidingWindow<u8> = SlidingWindow::new(size);

        // Fill the window
        for i in 0..size {
            window.push(i as u8);
        }

        kani::assert!(window.is_full(), "Window is full when len == size");
        kani::assert!(window.len() == size, "len equals size when full");
    }

    #[kani::proof]
    fn proof_sliding_window_push_when_full_removes_oldest() {
        let mut window: SlidingWindow<u8> = SlidingWindow::new(3);

        window.push(1);
        window.push(2);
        window.push(3);

        kani::assert!(window.is_full(), "Window is full");

        let removed = window.push(4);
        kani::assert!(removed == Some(1), "Oldest item (1) was removed");
        kani::assert!(window.len() == 3, "Length stays at size");
    }

    #[kani::proof]
    fn proof_sliding_window_push_when_not_full_returns_none() {
        let mut window: SlidingWindow<u8> = SlidingWindow::new(3);

        let removed = window.push(1);
        kani::assert!(removed.is_none(), "No item removed when not full");
    }

    #[kani::proof]
    fn proof_sliding_window_default_size() {
        let window: SlidingWindow<u8> = SlidingWindow::default();
        kani::assert!(window.size() == 10, "Default size is 10");
    }

    // ========================================================================
    // PriorityDeque Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_priority_deque_empty_initially() {
        let deque: PriorityDeque<u8> = PriorityDeque::new();
        kani::assert!(deque.is_empty(), "New deque is empty");
        kani::assert!(deque.len() == 0, "New deque has len 0");
    }

    #[kani::proof]
    fn proof_priority_deque_push_increases_len() {
        let mut deque: PriorityDeque<u8> = PriorityDeque::new();

        let result = deque.push(42, 5);
        kani::assert!(result.is_ok(), "Push succeeds");
        kani::assert!(deque.len() == 1, "len is 1 after push");
    }

    #[kani::proof]
    fn proof_priority_deque_pop_decreases_len() {
        let mut deque: PriorityDeque<u8> = PriorityDeque::new();
        deque.push(42, 5).unwrap();

        let result = deque.pop();
        kani::assert!(result.is_some(), "Pop returns item");
        kani::assert!(deque.len() == 0, "len is 0 after pop");
    }

    #[kani::proof]
    fn proof_priority_deque_ordering() {
        let mut deque: PriorityDeque<&str> = PriorityDeque::new();

        deque.push("low", 1).unwrap();
        deque.push("high", 10).unwrap();
        deque.push("medium", 5).unwrap();

        // Highest priority should come first
        let first = deque.pop();
        kani::assert!(first == Some("high"), "Highest priority first");
    }

    #[kani::proof]
    fn proof_priority_deque_with_capacity_full() {
        let mut deque: PriorityDeque<u8> = PriorityDeque::with_capacity(2);

        deque.push(1, 1).unwrap();
        deque.push(2, 2).unwrap();

        let result = deque.push(3, 3);
        kani::assert!(result.is_err(), "Push to full bounded deque fails");
    }

    // ========================================================================
    // WorkStealingDeque Proofs (basic invariants only, no concurrency)
    // ========================================================================

    #[kani::proof]
    fn proof_work_stealing_deque_empty_initially() {
        let deque: WorkStealingDeque<u8> = WorkStealingDeque::new();
        kani::assert!(deque.is_empty(), "New deque is empty");
    }

    #[kani::proof]
    fn proof_work_stealing_deque_lifo_local() {
        let deque = WorkStealingDeque::new();

        deque.push(1);
        deque.push(2);
        deque.push(3);

        // Local pop is LIFO (from back)
        let item = deque.pop();
        kani::assert!(item == Some(3), "Local pop is LIFO");
    }

    #[kani::proof]
    fn proof_work_stealing_deque_fifo_steal() {
        let deque = WorkStealingDeque::new();

        deque.push(1);
        deque.push(2);
        deque.push(3);

        // Steal is FIFO (from front)
        let item = deque.steal();
        kani::assert!(item == Some(1), "Steal is FIFO");
    }

    // ========================================================================
    // SharedDeque Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_shared_deque_empty_initially() {
        let deque: SharedDeque<u8> = SharedDeque::new();
        kani::assert!(deque.is_empty(), "New shared deque is empty");
        kani::assert!(deque.len() == 0, "New shared deque has len 0");
    }

    #[kani::proof]
    fn proof_shared_deque_push_pop_front() {
        let deque: SharedDeque<u8> = SharedDeque::new();

        deque.push_front(1);
        deque.push_front(2);

        let item = deque.pop_front();
        kani::assert!(
            item == Some(2),
            "pop_front returns most recently pushed front"
        );
    }

    #[kani::proof]
    fn proof_shared_deque_push_pop_back() {
        let deque: SharedDeque<u8> = SharedDeque::new();

        deque.push_back(1);
        deque.push_back(2);

        let item = deque.pop_back();
        kani::assert!(
            item == Some(2),
            "pop_back returns most recently pushed back"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bounded_deque() {
        let mut d = BoundedDeque::new(3);

        d.push_back(1).unwrap();
        d.push_back(2).unwrap();
        d.push_back(3).unwrap();
        assert!(d.push_back(4).is_err());

        assert_eq!(d.pop_front(), Some(1));
        d.push_back(4).unwrap();
    }

    #[test]
    fn test_priority_deque() {
        let mut d = PriorityDeque::new();

        d.push("low", 1).unwrap();
        d.push("high", 10).unwrap();
        d.push("medium", 5).unwrap();

        assert_eq!(d.pop(), Some("high"));
        assert_eq!(d.pop(), Some("medium"));
        assert_eq!(d.pop(), Some("low"));
    }

    #[test]
    fn test_work_stealing() {
        let d = WorkStealingDeque::new();

        d.push(1);
        d.push(2);
        d.push(3);

        // Pop from local end (LIFO)
        assert_eq!(d.pop(), Some(3));

        // Steal from remote end (FIFO)
        assert_eq!(d.steal(), Some(1));
    }

    #[test]
    fn test_sliding_window() {
        let mut w = SlidingWindow::new(3);

        assert_eq!(w.push(1), None);
        assert_eq!(w.push(2), None);
        assert_eq!(w.push(3), None);
        assert_eq!(w.push(4), Some(1));

        let items: Vec<_> = w.iter().cloned().collect();
        assert_eq!(items, vec![2, 3, 4]);
    }
}
