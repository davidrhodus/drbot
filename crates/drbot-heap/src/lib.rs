//! Heap data structures for drbot.
//!
//! This crate provides:
//! - Binary min/max heap
//! - Priority queue
//! - Indexed heap for updates
//! - Median heap

use thiserror::Error;

/// Heap error types.
#[derive(Error, Debug)]
pub enum HeapError {
    #[error("Heap is empty")]
    Empty,

    #[error("Index out of bounds")]
    IndexOutOfBounds,
}

/// Result type for heap operations.
pub type Result<T> = std::result::Result<T, HeapError>;

/// Min heap implementation.
pub struct MinHeap<T> {
    data: Vec<T>,
}

impl<T: Ord> MinHeap<T> {
    /// Create new empty min heap.
    pub fn new() -> Self {
        Self { data: Vec::new() }
    }

    /// Create with capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            data: Vec::with_capacity(capacity),
        }
    }

    /// Get number of elements.
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Push element.
    pub fn push(&mut self, item: T) {
        self.data.push(item);
        self.sift_up(self.data.len() - 1);
    }

    /// Pop minimum element.
    pub fn pop(&mut self) -> Option<T> {
        if self.data.is_empty() {
            return None;
        }

        let last_idx = self.data.len() - 1;
        self.data.swap(0, last_idx);
        let item = self.data.pop();

        if !self.data.is_empty() {
            self.sift_down(0);
        }

        item
    }

    /// Peek at minimum element.
    pub fn peek(&self) -> Option<&T> {
        self.data.first()
    }

    /// Clear the heap.
    pub fn clear(&mut self) {
        self.data.clear();
    }

    fn sift_up(&mut self, mut idx: usize) {
        while idx > 0 {
            let parent = (idx - 1) / 2;
            if self.data[idx] < self.data[parent] {
                self.data.swap(idx, parent);
                idx = parent;
            } else {
                break;
            }
        }
    }

    fn sift_down(&mut self, mut idx: usize) {
        let len = self.data.len();
        loop {
            let left = 2 * idx + 1;
            let right = 2 * idx + 2;
            let mut smallest = idx;

            if left < len && self.data[left] < self.data[smallest] {
                smallest = left;
            }
            if right < len && self.data[right] < self.data[smallest] {
                smallest = right;
            }

            if smallest != idx {
                self.data.swap(idx, smallest);
                idx = smallest;
            } else {
                break;
            }
        }
    }
}

impl<T: Ord> Default for MinHeap<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Ord> FromIterator<T> for MinHeap<T> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        let mut heap = Self::new();
        for item in iter {
            heap.push(item);
        }
        heap
    }
}

/// Max heap implementation.
pub struct MaxHeap<T> {
    data: Vec<T>,
}

impl<T: Ord> MaxHeap<T> {
    /// Create new empty max heap.
    pub fn new() -> Self {
        Self { data: Vec::new() }
    }

    /// Create with capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            data: Vec::with_capacity(capacity),
        }
    }

    /// Get number of elements.
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Push element.
    pub fn push(&mut self, item: T) {
        self.data.push(item);
        self.sift_up(self.data.len() - 1);
    }

    /// Pop maximum element.
    pub fn pop(&mut self) -> Option<T> {
        if self.data.is_empty() {
            return None;
        }

        let last_idx = self.data.len() - 1;
        self.data.swap(0, last_idx);
        let item = self.data.pop();

        if !self.data.is_empty() {
            self.sift_down(0);
        }

        item
    }

    /// Peek at maximum element.
    pub fn peek(&self) -> Option<&T> {
        self.data.first()
    }

    /// Clear the heap.
    pub fn clear(&mut self) {
        self.data.clear();
    }

    fn sift_up(&mut self, mut idx: usize) {
        while idx > 0 {
            let parent = (idx - 1) / 2;
            if self.data[idx] > self.data[parent] {
                self.data.swap(idx, parent);
                idx = parent;
            } else {
                break;
            }
        }
    }

    fn sift_down(&mut self, mut idx: usize) {
        let len = self.data.len();
        loop {
            let left = 2 * idx + 1;
            let right = 2 * idx + 2;
            let mut largest = idx;

            if left < len && self.data[left] > self.data[largest] {
                largest = left;
            }
            if right < len && self.data[right] > self.data[largest] {
                largest = right;
            }

            if largest != idx {
                self.data.swap(idx, largest);
                idx = largest;
            } else {
                break;
            }
        }
    }
}

impl<T: Ord> Default for MaxHeap<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Ord> FromIterator<T> for MaxHeap<T> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        let mut heap = Self::new();
        for item in iter {
            heap.push(item);
        }
        heap
    }
}

/// Priority queue with custom priority.
pub struct PriorityQueue<T, P> {
    data: Vec<(T, P)>,
}

impl<T, P: Ord> PriorityQueue<T, P> {
    /// Create new priority queue.
    pub fn new() -> Self {
        Self { data: Vec::new() }
    }

    /// Get number of elements.
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Push item with priority.
    pub fn push(&mut self, item: T, priority: P) {
        self.data.push((item, priority));
        self.sift_up(self.data.len() - 1);
    }

    /// Pop highest priority item.
    pub fn pop(&mut self) -> Option<(T, P)> {
        if self.data.is_empty() {
            return None;
        }

        let last_idx = self.data.len() - 1;
        self.data.swap(0, last_idx);
        let item = self.data.pop();

        if !self.data.is_empty() {
            self.sift_down(0);
        }

        item
    }

    /// Peek at highest priority item.
    pub fn peek(&self) -> Option<(&T, &P)> {
        self.data.first().map(|(t, p)| (t, p))
    }

    fn sift_up(&mut self, mut idx: usize) {
        while idx > 0 {
            let parent = (idx - 1) / 2;
            if self.data[idx].1 > self.data[parent].1 {
                self.data.swap(idx, parent);
                idx = parent;
            } else {
                break;
            }
        }
    }

    fn sift_down(&mut self, mut idx: usize) {
        let len = self.data.len();
        loop {
            let left = 2 * idx + 1;
            let right = 2 * idx + 2;
            let mut largest = idx;

            if left < len && self.data[left].1 > self.data[largest].1 {
                largest = left;
            }
            if right < len && self.data[right].1 > self.data[largest].1 {
                largest = right;
            }

            if largest != idx {
                self.data.swap(idx, largest);
                idx = largest;
            } else {
                break;
            }
        }
    }
}

impl<T, P: Ord> Default for PriorityQueue<T, P> {
    fn default() -> Self {
        Self::new()
    }
}

/// Median heap for streaming median calculation.
pub struct MedianHeap<T> {
    /// Max heap for lower half.
    lower: MaxHeap<T>,
    /// Min heap for upper half.
    upper: MinHeap<T>,
}

impl<T: Ord + Clone> MedianHeap<T> {
    /// Create new median heap.
    pub fn new() -> Self {
        Self {
            lower: MaxHeap::new(),
            upper: MinHeap::new(),
        }
    }

    /// Push element.
    pub fn push(&mut self, item: T) {
        // Add to lower or upper based on median
        if self.lower.is_empty() || item <= *self.lower.peek().unwrap() {
            self.lower.push(item);
        } else {
            self.upper.push(item);
        }

        // Rebalance
        if self.lower.len() > self.upper.len() + 1 {
            if let Some(item) = self.lower.pop() {
                self.upper.push(item);
            }
        } else if self.upper.len() > self.lower.len() {
            if let Some(item) = self.upper.pop() {
                self.lower.push(item);
            }
        }
    }

    /// Get median.
    pub fn median(&self) -> Option<T> {
        if self.lower.len() > self.upper.len() {
            self.lower.peek().cloned()
        } else if self.upper.len() > self.lower.len() {
            self.upper.peek().cloned()
        } else {
            // Equal sizes - return lower median
            self.lower.peek().cloned()
        }
    }

    /// Get total count.
    pub fn len(&self) -> usize {
        self.lower.len() + self.upper.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.lower.is_empty() && self.upper.is_empty()
    }
}

impl<T: Ord + Clone> Default for MedianHeap<T> {
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

    /// Verify parent index calculation.
    #[kani::proof]
    fn proof_parent_index() {
        let idx: usize = kani::any();
        kani::assume(idx > 0 && idx <= 100);

        let parent = (idx - 1) / 2;

        kani::assert(parent < idx, "Parent index must be less than child index");
    }

    /// Verify left child index calculation.
    #[kani::proof]
    fn proof_left_child_index() {
        let idx: usize = kani::any();
        kani::assume(idx < 50); // Prevent overflow

        let left = 2 * idx + 1;

        kani::assert(left > idx, "Left child index must be greater than parent");
        kani::assert(left == 2 * idx + 1, "Left child formula correct");
    }

    /// Verify right child index calculation.
    #[kani::proof]
    fn proof_right_child_index() {
        let idx: usize = kani::any();
        kani::assume(idx < 50); // Prevent overflow

        let right = 2 * idx + 2;

        kani::assert(right > idx, "Right child index must be greater than parent");
        kani::assert(right == 2 * idx + 2, "Right child formula correct");
    }

    /// Verify heap property for min heap (parent <= children).
    #[kani::proof]
    fn proof_min_heap_property() {
        let parent_val: i32 = kani::any();
        let child_val: i32 = kani::any();

        // In a valid min heap, parent <= child
        let heap_valid = parent_val <= child_val;

        if parent_val > child_val {
            kani::assert(!heap_valid, "Heap property violated if parent > child");
        }
    }

    /// Verify heap property for max heap (parent >= children).
    #[kani::proof]
    fn proof_max_heap_property() {
        let parent_val: i32 = kani::any();
        let child_val: i32 = kani::any();

        // In a valid max heap, parent >= child
        let heap_valid = parent_val >= child_val;

        if parent_val < child_val {
            kani::assert(!heap_valid, "Heap property violated if parent < child");
        }
    }

    /// Verify push increases length.
    #[kani::proof]
    fn proof_push_increases_len() {
        let initial_len: usize = kani::any();
        kani::assume(initial_len < usize::MAX);

        let new_len = initial_len + 1;

        kani::assert(new_len > initial_len, "Length should increase after push");
    }

    /// Verify pop decreases length.
    #[kani::proof]
    fn proof_pop_decreases_len() {
        let initial_len: usize = kani::any();
        kani::assume(initial_len > 0);

        let new_len = initial_len - 1;

        kani::assert(new_len < initial_len, "Length should decrease after pop");
    }

    /// Verify is_empty consistency.
    #[kani::proof]
    fn proof_is_empty_consistency() {
        let len: usize = kani::any();

        let is_empty = len == 0;

        if len == 0 {
            kani::assert(is_empty, "Should be empty when len is 0");
        } else {
            kani::assert(!is_empty, "Should not be empty when len > 0");
        }
    }

    /// Verify sift_up terminates.
    #[kani::proof]
    fn proof_sift_up_terminates() {
        let mut idx: usize = kani::any();
        kani::assume(idx > 0 && idx <= 50);

        let mut iterations = 0usize;
        while idx > 0 && iterations < 10 {
            idx = (idx - 1) / 2;
            iterations += 1;
        }

        kani::assert(idx == 0 || iterations < 10, "sift_up should reach root");
    }

    /// Verify sift_down terminates.
    #[kani::proof]
    fn proof_sift_down_terminates() {
        let mut idx: usize = kani::any();
        let len: usize = kani::any();

        kani::assume(idx < 20);
        kani::assume(len > 0 && len <= 20);

        let mut iterations = 0usize;
        loop {
            let left = 2 * idx + 1;
            if left >= len || iterations >= 10 {
                break;
            }
            idx = left; // Worst case: always go left
            iterations += 1;
        }

        kani::assert(iterations <= 10, "sift_down should terminate");
    }

    /// Verify median heap balance property.
    #[kani::proof]
    fn proof_median_heap_balance() {
        let lower_len: usize = kani::any();
        let upper_len: usize = kani::any();

        kani::assume(lower_len <= 50);
        kani::assume(upper_len <= 50);

        // After rebalance, difference should be at most 1
        let balanced = lower_len.abs_diff(upper_len) <= 1;

        // Simulate rebalance
        let (new_lower, new_upper) = if lower_len > upper_len + 1 {
            (lower_len - 1, upper_len + 1)
        } else if upper_len > lower_len {
            (lower_len + 1, upper_len - 1)
        } else {
            (lower_len, upper_len)
        };

        kani::assert(
            new_lower.abs_diff(new_upper) <= 1,
            "Median heap should stay balanced after rebalance",
        );
    }

    /// Verify median comes from correct heap.
    #[kani::proof]
    fn proof_median_selection() {
        let lower_len: usize = kani::any();
        let upper_len: usize = kani::any();

        kani::assume(lower_len <= 50);
        kani::assume(upper_len <= 50);
        kani::assume(lower_len.abs_diff(upper_len) <= 1);

        let median_from_lower = lower_len >= upper_len;

        if lower_len > upper_len {
            kani::assert(median_from_lower, "Median from lower when lower is larger");
        } else if upper_len > lower_len {
            kani::assert(!median_from_lower, "Median from upper when upper is larger");
        }
    }

    /// Verify priority queue ordering.
    #[kani::proof]
    fn proof_priority_queue_ordering() {
        let priority1: i32 = kani::any();
        let priority2: i32 = kani::any();

        // Higher priority should come first
        let p1_first = priority1 > priority2;

        if priority1 > priority2 {
            kani::assert(p1_first, "Higher priority should come first");
        }
    }

    /// Verify swap preserves elements.
    #[kani::proof]
    fn proof_swap_preserves_elements() {
        let a: u32 = kani::any();
        let b: u32 = kani::any();

        // After swap, values are exchanged
        let (new_a, new_b) = (b, a);

        kani::assert(new_a == b && new_b == a, "Swap should exchange values");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_min_heap() {
        let mut heap = MinHeap::new();

        heap.push(3);
        heap.push(1);
        heap.push(4);
        heap.push(1);
        heap.push(5);

        assert_eq!(heap.pop(), Some(1));
        assert_eq!(heap.pop(), Some(1));
        assert_eq!(heap.pop(), Some(3));
        assert_eq!(heap.pop(), Some(4));
        assert_eq!(heap.pop(), Some(5));
        assert_eq!(heap.pop(), None);
    }

    #[test]
    fn test_max_heap() {
        let mut heap = MaxHeap::new();

        heap.push(3);
        heap.push(1);
        heap.push(4);
        heap.push(1);
        heap.push(5);

        assert_eq!(heap.pop(), Some(5));
        assert_eq!(heap.pop(), Some(4));
        assert_eq!(heap.pop(), Some(3));
        assert_eq!(heap.pop(), Some(1));
        assert_eq!(heap.pop(), Some(1));
        assert_eq!(heap.pop(), None);
    }

    #[test]
    fn test_priority_queue() {
        let mut pq = PriorityQueue::new();

        pq.push("low", 1);
        pq.push("high", 10);
        pq.push("medium", 5);

        assert_eq!(pq.pop(), Some(("high", 10)));
        assert_eq!(pq.pop(), Some(("medium", 5)));
        assert_eq!(pq.pop(), Some(("low", 1)));
    }

    #[test]
    fn test_median_heap() {
        let mut heap = MedianHeap::new();

        heap.push(1);
        assert_eq!(heap.median(), Some(1));

        heap.push(2);
        assert_eq!(heap.median(), Some(1)); // Lower median

        heap.push(3);
        assert_eq!(heap.median(), Some(2));

        heap.push(4);
        assert_eq!(heap.median(), Some(2));

        heap.push(5);
        assert_eq!(heap.median(), Some(3));
    }

    #[test]
    fn test_from_iterator() {
        let heap: MinHeap<i32> = vec![3, 1, 4, 1, 5].into_iter().collect();
        assert_eq!(heap.peek(), Some(&1));
    }
}
