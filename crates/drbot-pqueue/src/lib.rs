//! Priority queue implementations for drbot.
//!
//! This crate provides:
//! - Binary heap priority queue
//! - Keyed priority queue
//! - Indexed priority queue
//! - Thread-safe variants

use std::cmp::Ordering;
use std::collections::HashMap;
use std::hash::Hash;
use std::sync::{Arc, Mutex};
use thiserror::Error;

/// Priority queue error types.
#[derive(Error, Debug)]
pub enum PriorityQueueError {
    #[error("Queue full")]
    Full,

    #[error("Queue empty")]
    Empty,

    #[error("Key not found")]
    KeyNotFound,

    #[error("Key already exists")]
    KeyExists,
}

/// Result type for priority queue operations.
pub type Result<T> = std::result::Result<T, PriorityQueueError>;

/// Priority queue entry.
#[derive(Debug, Clone)]
struct Entry<T, P> {
    item: T,
    priority: P,
}

/// Binary heap priority queue (max-heap).
pub struct PriorityQueue<T, P: Ord> {
    heap: Vec<Entry<T, P>>,
    capacity: Option<usize>,
}

impl<T, P: Ord> PriorityQueue<T, P> {
    /// Create new priority queue.
    pub fn new() -> Self {
        Self {
            heap: Vec::new(),
            capacity: None,
        }
    }

    /// Create with capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            heap: Vec::with_capacity(capacity),
            capacity: Some(capacity),
        }
    }

    /// Get length.
    pub fn len(&self) -> usize {
        self.heap.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.heap.is_empty()
    }

    /// Check if full.
    pub fn is_full(&self) -> bool {
        self.capacity.map_or(false, |c| self.heap.len() >= c)
    }

    /// Push item with priority.
    pub fn push(&mut self, item: T, priority: P) -> Result<()> {
        if self.is_full() {
            return Err(PriorityQueueError::Full);
        }

        self.heap.push(Entry { item, priority });
        self.sift_up(self.heap.len() - 1);
        Ok(())
    }

    /// Pop highest priority item.
    pub fn pop(&mut self) -> Option<(T, P)> {
        if self.heap.is_empty() {
            return None;
        }

        let last = self.heap.len() - 1;
        self.heap.swap(0, last);
        let entry = self.heap.pop()?;

        if !self.heap.is_empty() {
            self.sift_down(0);
        }

        Some((entry.item, entry.priority))
    }

    /// Peek highest priority item.
    pub fn peek(&self) -> Option<(&T, &P)> {
        self.heap.first().map(|e| (&e.item, &e.priority))
    }

    /// Clear queue.
    pub fn clear(&mut self) {
        self.heap.clear();
    }

    fn sift_up(&mut self, mut index: usize) {
        while index > 0 {
            let parent = (index - 1) / 2;
            if self.heap[index].priority <= self.heap[parent].priority {
                break;
            }
            self.heap.swap(index, parent);
            index = parent;
        }
    }

    fn sift_down(&mut self, mut index: usize) {
        loop {
            let left = 2 * index + 1;
            let right = 2 * index + 2;
            let mut largest = index;

            if left < self.heap.len() && self.heap[left].priority > self.heap[largest].priority {
                largest = left;
            }
            if right < self.heap.len() && self.heap[right].priority > self.heap[largest].priority {
                largest = right;
            }

            if largest == index {
                break;
            }

            self.heap.swap(index, largest);
            index = largest;
        }
    }
}

impl<T, P: Ord> Default for PriorityQueue<T, P> {
    fn default() -> Self {
        Self::new()
    }
}

/// Min-priority queue.
pub struct MinPriorityQueue<T, P: Ord> {
    inner: PriorityQueue<T, std::cmp::Reverse<P>>,
}

impl<T, P: Ord> MinPriorityQueue<T, P> {
    /// Create new min-priority queue.
    pub fn new() -> Self {
        Self {
            inner: PriorityQueue::new(),
        }
    }

    /// Push item.
    pub fn push(&mut self, item: T, priority: P) -> Result<()> {
        self.inner.push(item, std::cmp::Reverse(priority))
    }

    /// Pop minimum priority item.
    pub fn pop(&mut self) -> Option<(T, P)> {
        self.inner.pop().map(|(item, rev)| (item, rev.0))
    }

    /// Peek minimum priority item.
    pub fn peek(&self) -> Option<(&T, &P)> {
        self.inner.peek().map(|(item, rev)| (item, &rev.0))
    }

    /// Get length.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Clear queue.
    pub fn clear(&mut self) {
        self.inner.clear();
    }
}

impl<T, P: Ord> Default for MinPriorityQueue<T, P> {
    fn default() -> Self {
        Self::new()
    }
}

/// Keyed priority queue (allows updates).
pub struct KeyedPriorityQueue<K: Eq + Hash + Clone, V, P: Ord + Clone> {
    heap: Vec<Entry<(K, V), P>>,
    index_map: HashMap<K, usize>,
}

impl<K: Eq + Hash + Clone, V, P: Ord + Clone> KeyedPriorityQueue<K, V, P> {
    /// Create new keyed priority queue.
    pub fn new() -> Self {
        Self {
            heap: Vec::new(),
            index_map: HashMap::new(),
        }
    }

    /// Get length.
    pub fn len(&self) -> usize {
        self.heap.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.heap.is_empty()
    }

    /// Check if contains key.
    pub fn contains_key(&self, key: &K) -> bool {
        self.index_map.contains_key(key)
    }

    /// Push or update item.
    pub fn push(&mut self, key: K, value: V, priority: P) {
        if let Some(&index) = self.index_map.get(&key) {
            // Update existing
            let old_priority = self.heap[index].priority.clone();
            self.heap[index].item.1 = value;
            self.heap[index].priority = priority.clone();

            match priority.cmp(&old_priority) {
                Ordering::Greater => self.sift_up(index),
                Ordering::Less => self.sift_down(index),
                Ordering::Equal => {}
            }
        } else {
            // Insert new
            let index = self.heap.len();
            self.heap.push(Entry {
                item: (key.clone(), value),
                priority,
            });
            self.index_map.insert(key, index);
            self.sift_up(index);
        }
    }

    /// Pop highest priority item.
    pub fn pop(&mut self) -> Option<(K, V, P)> {
        if self.heap.is_empty() {
            return None;
        }

        let last = self.heap.len() - 1;
        self.swap(0, last);

        let entry = self.heap.pop()?;
        self.index_map.remove(&entry.item.0);

        if !self.heap.is_empty() {
            self.sift_down(0);
        }

        Some((entry.item.0, entry.item.1, entry.priority))
    }

    /// Remove by key.
    pub fn remove(&mut self, key: &K) -> Option<(V, P)> {
        let index = *self.index_map.get(key)?;
        let last = self.heap.len() - 1;

        if index != last {
            self.swap(index, last);
        }

        let entry = self.heap.pop()?;
        self.index_map.remove(key);

        if index < self.heap.len() {
            self.sift_down(index);
            self.sift_up(index);
        }

        Some((entry.item.1, entry.priority))
    }

    /// Clear queue.
    pub fn clear(&mut self) {
        self.heap.clear();
        self.index_map.clear();
    }

    fn swap(&mut self, i: usize, j: usize) {
        self.heap.swap(i, j);
        let ki = self.heap[i].item.0.clone();
        let kj = self.heap[j].item.0.clone();
        self.index_map.insert(ki, i);
        self.index_map.insert(kj, j);
    }

    fn sift_up(&mut self, mut index: usize) {
        while index > 0 {
            let parent = (index - 1) / 2;
            if self.heap[index].priority <= self.heap[parent].priority {
                break;
            }
            self.swap(index, parent);
            index = parent;
        }
    }

    fn sift_down(&mut self, mut index: usize) {
        loop {
            let left = 2 * index + 1;
            let right = 2 * index + 2;
            let mut largest = index;

            if left < self.heap.len() && self.heap[left].priority > self.heap[largest].priority {
                largest = left;
            }
            if right < self.heap.len() && self.heap[right].priority > self.heap[largest].priority {
                largest = right;
            }

            if largest == index {
                break;
            }

            self.swap(index, largest);
            index = largest;
        }
    }
}

impl<K: Eq + Hash + Clone, V, P: Ord + Clone> Default for KeyedPriorityQueue<K, V, P> {
    fn default() -> Self {
        Self::new()
    }
}

/// Thread-safe priority queue.
pub struct SyncPriorityQueue<T: Send, P: Ord + Send> {
    inner: Arc<Mutex<PriorityQueue<T, P>>>,
}

impl<T: Send, P: Ord + Send> SyncPriorityQueue<T, P> {
    /// Create new sync priority queue.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(PriorityQueue::new())),
        }
    }

    /// Push item.
    pub fn push(&self, item: T, priority: P) -> Result<()> {
        self.inner.lock().unwrap().push(item, priority)
    }

    /// Pop item.
    pub fn pop(&self) -> Option<(T, P)> {
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

    /// Clear queue.
    pub fn clear(&self) {
        self.inner.lock().unwrap().clear();
    }
}

impl<T: Send, P: Ord + Send> Clone for SyncPriorityQueue<T, P> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<T: Send, P: Ord + Send> Default for SyncPriorityQueue<T, P> {
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
    // Index Calculation Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_parent_index_calculation() {
        let index: usize = kani::any();
        kani::assume(index > 0 && index < 100);

        let parent = (index - 1) / 2;

        // Parent is always less than the child index
        kani::assert!(parent < index, "Parent index < child index");

        // Left child of parent is 2*parent + 1, right is 2*parent + 2
        let left_child = 2 * parent + 1;
        let right_child = 2 * parent + 2;

        // Index must be either left or right child
        kani::assert!(
            index == left_child || index == right_child,
            "Index is left or right child of parent"
        );
    }

    #[kani::proof]
    fn proof_left_child_index_calculation() {
        let parent: usize = kani::any();
        kani::assume(parent < 50);

        let left = 2 * parent + 1;

        kani::assert!(left > parent, "Left child > parent");
        kani::assert!(left % 2 == 1, "Left child is odd");
    }

    #[kani::proof]
    fn proof_right_child_index_calculation() {
        let parent: usize = kani::any();
        kani::assume(parent < 50);

        let right = 2 * parent + 2;

        kani::assert!(right > parent, "Right child > parent");
        kani::assert!(right % 2 == 0, "Right child is even");
    }

    #[kani::proof]
    fn proof_children_adjacent() {
        let parent: usize = kani::any();
        kani::assume(parent < 50);

        let left = 2 * parent + 1;
        let right = 2 * parent + 2;

        kani::assert!(right == left + 1, "Right child is left + 1");
    }

    // ========================================================================
    // PriorityQueue Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_priority_queue_empty_initially() {
        let pq: PriorityQueue<u8, i32> = PriorityQueue::new();
        kani::assert!(pq.is_empty(), "New queue is empty");
        kani::assert!(pq.len() == 0, "New queue has len 0");
    }

    #[kani::proof]
    fn proof_priority_queue_push_increases_len() {
        let mut pq: PriorityQueue<u8, i32> = PriorityQueue::new();

        let result = pq.push(42, 5);
        kani::assert!(result.is_ok(), "Push succeeds");
        kani::assert!(pq.len() == 1, "len is 1 after push");
        kani::assert!(!pq.is_empty(), "Queue not empty after push");
    }

    #[kani::proof]
    fn proof_priority_queue_pop_decreases_len() {
        let mut pq: PriorityQueue<u8, i32> = PriorityQueue::new();
        pq.push(42, 5).unwrap();

        let result = pq.pop();
        kani::assert!(result.is_some(), "Pop returns item");
        kani::assert!(pq.len() == 0, "len is 0 after pop");
        kani::assert!(pq.is_empty(), "Queue empty after pop");
    }

    #[kani::proof]
    fn proof_priority_queue_pop_empty() {
        let mut pq: PriorityQueue<u8, i32> = PriorityQueue::new();

        let result = pq.pop();
        kani::assert!(result.is_none(), "Pop on empty returns None");
    }

    #[kani::proof]
    fn proof_priority_queue_peek_does_not_remove() {
        let mut pq: PriorityQueue<u8, i32> = PriorityQueue::new();
        pq.push(42, 5).unwrap();

        let _peek = pq.peek();
        kani::assert!(pq.len() == 1, "Peek does not change len");
    }

    #[kani::proof]
    fn proof_priority_queue_max_heap_property() {
        let mut pq: PriorityQueue<&str, i32> = PriorityQueue::new();

        pq.push("low", 1).unwrap();
        pq.push("high", 10).unwrap();
        pq.push("medium", 5).unwrap();

        // Max-heap: highest priority first
        let (item, priority) = pq.pop().unwrap();
        kani::assert!(item == "high", "Highest priority item first");
        kani::assert!(priority == 10, "Highest priority is 10");
    }

    #[kani::proof]
    fn proof_priority_queue_with_capacity_full() {
        let mut pq: PriorityQueue<u8, i32> = PriorityQueue::with_capacity(2);

        pq.push(1, 1).unwrap();
        pq.push(2, 2).unwrap();

        kani::assert!(pq.is_full(), "Queue is full at capacity");

        let result = pq.push(3, 3);
        kani::assert!(result.is_err(), "Push to full queue fails");
    }

    #[kani::proof]
    fn proof_priority_queue_unbounded_not_full() {
        let pq: PriorityQueue<u8, i32> = PriorityQueue::new();
        kani::assert!(!pq.is_full(), "Unbounded queue is never full initially");
    }

    #[kani::proof]
    fn proof_priority_queue_clear() {
        let mut pq: PriorityQueue<u8, i32> = PriorityQueue::new();
        pq.push(1, 1).unwrap();
        pq.push(2, 2).unwrap();

        pq.clear();
        kani::assert!(pq.is_empty(), "Queue empty after clear");
        kani::assert!(pq.len() == 0, "len is 0 after clear");
    }

    // ========================================================================
    // MinPriorityQueue Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_min_priority_queue_empty_initially() {
        let pq: MinPriorityQueue<u8, i32> = MinPriorityQueue::new();
        kani::assert!(pq.is_empty(), "New min queue is empty");
    }

    #[kani::proof]
    fn proof_min_priority_queue_min_first() {
        let mut pq: MinPriorityQueue<&str, i32> = MinPriorityQueue::new();

        pq.push("low", 1).unwrap();
        pq.push("high", 10).unwrap();
        pq.push("medium", 5).unwrap();

        // Min-heap: lowest priority first
        let (item, priority) = pq.pop().unwrap();
        kani::assert!(item == "low", "Lowest priority item first");
        kani::assert!(priority == 1, "Lowest priority is 1");
    }

    #[kani::proof]
    fn proof_min_priority_queue_push_increases_len() {
        let mut pq: MinPriorityQueue<u8, i32> = MinPriorityQueue::new();

        pq.push(42, 5).unwrap();
        kani::assert!(pq.len() == 1, "len is 1 after push");
    }

    // ========================================================================
    // KeyedPriorityQueue Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_keyed_pq_empty_initially() {
        let pq: KeyedPriorityQueue<&str, u8, i32> = KeyedPriorityQueue::new();
        kani::assert!(pq.is_empty(), "New keyed queue is empty");
    }

    #[kani::proof]
    fn proof_keyed_pq_contains_key_after_push() {
        let mut pq: KeyedPriorityQueue<&str, u8, i32> = KeyedPriorityQueue::new();

        pq.push("key", 42, 5);
        kani::assert!(pq.contains_key(&"key"), "Contains key after push");
    }

    #[kani::proof]
    fn proof_keyed_pq_update_replaces_value() {
        let mut pq: KeyedPriorityQueue<&str, u8, i32> = KeyedPriorityQueue::new();

        pq.push("key", 1, 5);
        pq.push("key", 2, 10); // Update

        kani::assert!(pq.len() == 1, "Update does not increase len");

        let (key, value, priority) = pq.pop().unwrap();
        kani::assert!(key == "key", "Key preserved");
        kani::assert!(value == 2, "Value updated");
        kani::assert!(priority == 10, "Priority updated");
    }

    #[kani::proof]
    fn proof_keyed_pq_remove_by_key() {
        let mut pq: KeyedPriorityQueue<&str, u8, i32> = KeyedPriorityQueue::new();

        pq.push("a", 1, 5);
        pq.push("b", 2, 10);

        let removed = pq.remove(&"a");
        kani::assert!(removed.is_some(), "Remove returns item");
        kani::assert!(pq.len() == 1, "len decreases after remove");
        kani::assert!(!pq.contains_key(&"a"), "Key removed");
        kani::assert!(pq.contains_key(&"b"), "Other key preserved");
    }

    #[kani::proof]
    fn proof_keyed_pq_remove_nonexistent() {
        let mut pq: KeyedPriorityQueue<&str, u8, i32> = KeyedPriorityQueue::new();

        let removed = pq.remove(&"nonexistent");
        kani::assert!(removed.is_none(), "Remove nonexistent returns None");
    }

    // ========================================================================
    // SyncPriorityQueue Proofs (basic, single-threaded)
    // ========================================================================

    #[kani::proof]
    fn proof_sync_pq_empty_initially() {
        let pq: SyncPriorityQueue<u8, i32> = SyncPriorityQueue::new();
        kani::assert!(pq.is_empty(), "New sync queue is empty");
    }

    #[kani::proof]
    fn proof_sync_pq_push_pop() {
        let pq: SyncPriorityQueue<u8, i32> = SyncPriorityQueue::new();

        pq.push(42, 5).unwrap();
        kani::assert!(pq.len() == 1, "len is 1 after push");

        let result = pq.pop();
        kani::assert!(result == Some((42, 5)), "Pop returns pushed item");
        kani::assert!(pq.is_empty(), "Queue empty after pop");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_priority_queue() {
        let mut pq = PriorityQueue::new();

        pq.push("low", 1).unwrap();
        pq.push("high", 10).unwrap();
        pq.push("medium", 5).unwrap();

        assert_eq!(pq.pop(), Some(("high", 10)));
        assert_eq!(pq.pop(), Some(("medium", 5)));
        assert_eq!(pq.pop(), Some(("low", 1)));
    }

    #[test]
    fn test_min_priority_queue() {
        let mut pq = MinPriorityQueue::new();

        pq.push("low", 1).unwrap();
        pq.push("high", 10).unwrap();
        pq.push("medium", 5).unwrap();

        assert_eq!(pq.pop(), Some(("low", 1)));
        assert_eq!(pq.pop(), Some(("medium", 5)));
        assert_eq!(pq.pop(), Some(("high", 10)));
    }

    #[test]
    fn test_keyed_priority_queue() {
        let mut pq: KeyedPriorityQueue<&str, i32, i32> = KeyedPriorityQueue::new();

        pq.push("a", 1, 5);
        pq.push("b", 2, 10);
        pq.push("a", 3, 15); // Update "a" with higher priority

        assert_eq!(pq.pop(), Some(("a", 3, 15)));
        assert_eq!(pq.pop(), Some(("b", 2, 10)));
    }
}
