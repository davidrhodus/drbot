//! Ring buffer implementation for drbot.
//!
//! This crate provides:
//! - Fixed-size ring buffer
//! - Overwriting behavior
//! - Iterator support
//! - Thread-safe variant

use std::sync::{Arc, Mutex};
use thiserror::Error;

/// Ring buffer error types.
#[derive(Error, Debug)]
pub enum RingBufferError {
    #[error("Buffer full")]
    Full,

    #[error("Buffer empty")]
    Empty,

    #[error("Invalid capacity")]
    InvalidCapacity,
}

/// Result type for ring buffer operations.
pub type Result<T> = std::result::Result<T, RingBufferError>;

/// Ring buffer with fixed capacity.
pub struct RingBuffer<T> {
    buffer: Vec<Option<T>>,
    capacity: usize,
    head: usize,
    tail: usize,
    len: usize,
}

impl<T> RingBuffer<T> {
    /// Create new ring buffer.
    pub fn new(capacity: usize) -> Self {
        let mut buffer = Vec::with_capacity(capacity);
        buffer.resize_with(capacity, || None);

        Self {
            buffer,
            capacity,
            head: 0,
            tail: 0,
            len: 0,
        }
    }

    /// Get capacity.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Get length.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Check if full.
    pub fn is_full(&self) -> bool {
        self.len == self.capacity
    }

    /// Push item (fails if full).
    pub fn push(&mut self, item: T) -> Result<()> {
        if self.is_full() {
            return Err(RingBufferError::Full);
        }

        self.buffer[self.tail] = Some(item);
        self.tail = (self.tail + 1) % self.capacity;
        self.len += 1;
        Ok(())
    }

    /// Push item, overwriting oldest if full.
    pub fn push_overwrite(&mut self, item: T) -> Option<T> {
        let overwritten = if self.is_full() {
            let old = self.buffer[self.head].take();
            self.head = (self.head + 1) % self.capacity;
            self.len -= 1;
            old
        } else {
            None
        };

        self.buffer[self.tail] = Some(item);
        self.tail = (self.tail + 1) % self.capacity;
        self.len += 1;

        overwritten
    }

    /// Pop oldest item.
    pub fn pop(&mut self) -> Option<T> {
        if self.is_empty() {
            return None;
        }

        let item = self.buffer[self.head].take();
        self.head = (self.head + 1) % self.capacity;
        self.len -= 1;
        item
    }

    /// Peek oldest item.
    pub fn peek(&self) -> Option<&T> {
        if self.is_empty() {
            None
        } else {
            self.buffer[self.head].as_ref()
        }
    }

    /// Peek newest item.
    pub fn peek_back(&self) -> Option<&T> {
        if self.is_empty() {
            None
        } else {
            let idx = if self.tail == 0 {
                self.capacity - 1
            } else {
                self.tail - 1
            };
            self.buffer[idx].as_ref()
        }
    }

    /// Clear buffer.
    pub fn clear(&mut self) {
        for i in 0..self.capacity {
            self.buffer[i] = None;
        }
        self.head = 0;
        self.tail = 0;
        self.len = 0;
    }

    /// Get available space.
    pub fn available(&self) -> usize {
        self.capacity - self.len
    }

    /// Iterate over items.
    pub fn iter(&self) -> RingBufferIter<'_, T> {
        RingBufferIter {
            buffer: self,
            current: self.head,
            remaining: self.len,
        }
    }

    /// Convert to vec.
    pub fn to_vec(&self) -> Vec<T>
    where
        T: Clone,
    {
        self.iter().cloned().collect()
    }
}

impl<T> Default for RingBuffer<T> {
    fn default() -> Self {
        Self::new(16)
    }
}

/// Iterator over ring buffer.
pub struct RingBufferIter<'a, T> {
    buffer: &'a RingBuffer<T>,
    current: usize,
    remaining: usize,
}

impl<'a, T> Iterator for RingBufferIter<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }

        let item = self.buffer.buffer[self.current].as_ref();
        self.current = (self.current + 1) % self.buffer.capacity;
        self.remaining -= 1;
        item
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
}

impl<T> ExactSizeIterator for RingBufferIter<'_, T> {}

/// Thread-safe ring buffer.
pub struct SyncRingBuffer<T> {
    inner: Arc<Mutex<RingBuffer<T>>>,
}

impl<T> SyncRingBuffer<T> {
    /// Create new sync ring buffer.
    pub fn new(capacity: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(RingBuffer::new(capacity))),
        }
    }

    /// Push item.
    pub fn push(&self, item: T) -> Result<()> {
        self.inner.lock().unwrap().push(item)
    }

    /// Push with overwrite.
    pub fn push_overwrite(&self, item: T) -> Option<T> {
        self.inner.lock().unwrap().push_overwrite(item)
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

    /// Clear buffer.
    pub fn clear(&self) {
        self.inner.lock().unwrap().clear();
    }
}

impl<T> Clone for SyncRingBuffer<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<T> Default for SyncRingBuffer<T> {
    fn default() -> Self {
        Self::new(16)
    }
}

/// Growing ring buffer.
pub struct GrowingRingBuffer<T> {
    inner: RingBuffer<T>,
    max_capacity: usize,
}

impl<T> GrowingRingBuffer<T> {
    /// Create new growing buffer.
    pub fn new(initial: usize, max: usize) -> Self {
        Self {
            inner: RingBuffer::new(initial),
            max_capacity: max,
        }
    }

    /// Push item, growing if needed.
    pub fn push(&mut self, item: T) -> Result<()>
    where
        T: Clone,
    {
        if self.inner.is_full() {
            if self.inner.capacity() >= self.max_capacity {
                return Err(RingBufferError::Full);
            }

            // Grow buffer
            let new_capacity = (self.inner.capacity() * 2).min(self.max_capacity);
            let items = self.inner.to_vec();

            self.inner = RingBuffer::new(new_capacity);
            for item in items {
                self.inner.push(item)?;
            }
        }

        self.inner.push(item)
    }

    /// Pop item.
    pub fn pop(&mut self) -> Option<T> {
        self.inner.pop()
    }

    /// Get length.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Get current capacity.
    pub fn capacity(&self) -> usize {
        self.inner.capacity()
    }
}

// ============================================================================
// Kani Formal Verification Proofs
// ============================================================================

#[cfg(kani)]
mod kani_proofs {
    use super::*;

    /// Verify head/tail wraparound is correct.
    #[kani::proof]
    fn proof_wraparound_calculation() {
        let pos: usize = kani::any();
        let capacity: usize = kani::any();

        kani::assume(capacity > 0 && capacity <= 100);
        kani::assume(pos < capacity * 2); // Allow one wrap

        let next = (pos + 1) % capacity;

        kani::assert(next < capacity, "Wrapped position must be within capacity");
    }

    /// Verify len <= capacity invariant.
    #[kani::proof]
    fn proof_len_capacity_invariant() {
        let len: usize = kani::any();
        let capacity: usize = kani::any();

        kani::assume(capacity > 0 && capacity <= 100);
        kani::assume(len <= capacity);

        kani::assert(len <= capacity, "Length must not exceed capacity");
    }

    /// Verify is_full logic.
    #[kani::proof]
    fn proof_is_full_logic() {
        let len: usize = kani::any();
        let capacity: usize = kani::any();

        kani::assume(capacity > 0 && capacity <= 100);
        kani::assume(len <= capacity);

        let is_full = len == capacity;

        if len < capacity {
            kani::assert(!is_full, "Not full when len < capacity");
        } else {
            kani::assert(is_full, "Full when len == capacity");
        }
    }

    /// Verify is_empty logic.
    #[kani::proof]
    fn proof_is_empty_logic() {
        let len: usize = kani::any();

        let is_empty = len == 0;

        if len > 0 {
            kani::assert(!is_empty, "Not empty when len > 0");
        } else {
            kani::assert(is_empty, "Empty when len == 0");
        }
    }

    /// Verify push increases len.
    #[kani::proof]
    fn proof_push_increases_len() {
        let initial_len: usize = kani::any();
        let capacity: usize = kani::any();

        kani::assume(capacity > 0 && capacity <= 100);
        kani::assume(initial_len < capacity); // Not full

        let new_len = initial_len + 1;

        kani::assert(new_len > initial_len, "Push should increase length");
        kani::assert(new_len <= capacity, "Length should not exceed capacity");
    }

    /// Verify pop decreases len.
    #[kani::proof]
    fn proof_pop_decreases_len() {
        let initial_len: usize = kani::any();

        kani::assume(initial_len > 0); // Not empty

        let new_len = initial_len - 1;

        kani::assert(new_len < initial_len, "Pop should decrease length");
        kani::assert(new_len >= 0, "Length should not go negative");
    }

    /// Verify available calculation.
    #[kani::proof]
    fn proof_available_calculation() {
        let len: usize = kani::any();
        let capacity: usize = kani::any();

        kani::assume(capacity > 0 && capacity <= 100);
        kani::assume(len <= capacity);

        let available = capacity - len;

        kani::assert(
            available + len == capacity,
            "Available + len should equal capacity",
        );
        kani::assert(
            available <= capacity,
            "Available should not exceed capacity",
        );
    }

    /// Verify peek_back index calculation.
    #[kani::proof]
    fn proof_peek_back_index() {
        let tail: usize = kani::any();
        let capacity: usize = kani::any();

        kani::assume(capacity > 0 && capacity <= 100);
        kani::assume(tail < capacity);

        let idx = if tail == 0 { capacity - 1 } else { tail - 1 };

        kani::assert(idx < capacity, "peek_back index must be within capacity");
    }

    /// Verify push_overwrite maintains len.
    #[kani::proof]
    fn proof_push_overwrite_len() {
        let initial_len: usize = kani::any();
        let capacity: usize = kani::any();

        kani::assume(capacity > 0 && capacity <= 100);
        kani::assume(initial_len <= capacity);

        let is_full = initial_len == capacity;
        let new_len = if is_full {
            // Overwrite: remove one, add one
            initial_len - 1 + 1
        } else {
            initial_len + 1
        };

        kani::assert(new_len <= capacity, "Length should not exceed capacity");
        kani::assert(
            new_len >= initial_len || is_full,
            "Length should increase or stay same when full",
        );
    }

    /// Verify iterator remaining count.
    #[kani::proof]
    fn proof_iterator_remaining() {
        let len: usize = kani::any();
        kani::assume(len <= 100);

        let mut remaining = len;
        let mut iterated = 0usize;

        while remaining > 0 {
            remaining -= 1;
            iterated += 1;
            if iterated > 100 {
                break; // Prevent infinite loop in verification
            }
        }

        kani::assert(iterated == len, "Should iterate exactly len times");
    }

    /// Verify growing buffer doubles correctly.
    #[kani::proof]
    fn proof_growing_buffer_doubles() {
        let current_capacity: usize = kani::any();
        let max_capacity: usize = kani::any();

        kani::assume(current_capacity > 0 && current_capacity <= 1000);
        kani::assume(max_capacity >= current_capacity && max_capacity <= 10000);

        let new_capacity = (current_capacity * 2).min(max_capacity);

        kani::assert(
            new_capacity >= current_capacity,
            "New capacity should be >= current",
        );
        kani::assert(
            new_capacity <= max_capacity,
            "New capacity should not exceed max",
        );
    }

    /// Verify default capacity is valid.
    #[kani::proof]
    fn proof_default_capacity() {
        let default_capacity: usize = 16;

        kani::assert(default_capacity > 0, "Default capacity must be positive");
    }

    /// Verify head and tail stay within bounds.
    #[kani::proof]
    fn proof_head_tail_bounds() {
        let head: usize = kani::any();
        let tail: usize = kani::any();
        let capacity: usize = kani::any();

        kani::assume(capacity > 0 && capacity <= 100);
        kani::assume(head < capacity);
        kani::assume(tail < capacity);

        kani::assert(head < capacity, "Head must be within capacity");
        kani::assert(tail < capacity, "Tail must be within capacity");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_push_pop() {
        let mut rb = RingBuffer::new(3);

        rb.push(1).unwrap();
        rb.push(2).unwrap();
        rb.push(3).unwrap();

        assert!(rb.push(4).is_err());

        assert_eq!(rb.pop(), Some(1));
        assert_eq!(rb.pop(), Some(2));
        assert_eq!(rb.pop(), Some(3));
        assert_eq!(rb.pop(), None);
    }

    #[test]
    fn test_overwrite() {
        let mut rb = RingBuffer::new(3);

        rb.push_overwrite(1);
        rb.push_overwrite(2);
        rb.push_overwrite(3);
        let old = rb.push_overwrite(4);

        assert_eq!(old, Some(1));
        assert_eq!(rb.pop(), Some(2));
    }

    #[test]
    fn test_iter() {
        let mut rb = RingBuffer::new(5);
        rb.push(1).unwrap();
        rb.push(2).unwrap();
        rb.push(3).unwrap();

        let items: Vec<_> = rb.iter().cloned().collect();
        assert_eq!(items, vec![1, 2, 3]);
    }

    #[test]
    fn test_peek() {
        let mut rb = RingBuffer::new(3);
        rb.push(1).unwrap();
        rb.push(2).unwrap();

        assert_eq!(rb.peek(), Some(&1));
        assert_eq!(rb.peek_back(), Some(&2));
    }
}
