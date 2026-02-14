//! Iterator pattern utilities for drbot.
//!
//! This crate provides:
//! - Custom iterator types
//! - Aggregate trait
//! - Iterator combinators

use thiserror::Error;

/// Iterator error types.
#[derive(Error, Debug)]
pub enum IteratorError {
    #[error("No more elements")]
    Exhausted,

    #[error("Invalid state")]
    InvalidState,
}

/// Result type for iterator operations.
pub type Result<T> = std::result::Result<T, IteratorError>;

/// Aggregate trait for collections.
pub trait Aggregate {
    /// Item type.
    type Item;
    /// Iterator type.
    type Iter: Iterator<Item = Self::Item>;

    /// Create iterator.
    fn create_iterator(&self) -> Self::Iter;
}

/// Range iterator.
pub struct RangeIterator {
    current: i64,
    end: i64,
    step: i64,
}

impl RangeIterator {
    /// Create new range iterator.
    pub fn new(start: i64, end: i64) -> Self {
        Self {
            current: start,
            end,
            step: 1,
        }
    }

    /// Create with step.
    pub fn with_step(start: i64, end: i64, step: i64) -> Self {
        Self {
            current: start,
            end,
            step,
        }
    }
}

impl Iterator for RangeIterator {
    type Item = i64;

    fn next(&mut self) -> Option<Self::Item> {
        if (self.step > 0 && self.current < self.end) || (self.step < 0 && self.current > self.end)
        {
            let value = self.current;
            self.current += self.step;
            Some(value)
        } else {
            None
        }
    }
}

/// Cycle iterator that repeats elements.
pub struct CycleIterator<I: Iterator + Clone>
where
    I::Item: Clone,
{
    original: I,
    current: I,
    count: Option<usize>,
    remaining: usize,
}

impl<I: Iterator + Clone> CycleIterator<I>
where
    I::Item: Clone,
{
    /// Create infinite cycle.
    pub fn infinite(iter: I) -> Self {
        Self {
            original: iter.clone(),
            current: iter,
            count: None,
            remaining: 0,
        }
    }

    /// Create finite cycle.
    pub fn times(iter: I, count: usize) -> Self {
        Self {
            original: iter.clone(),
            current: iter,
            count: Some(count),
            remaining: count,
        }
    }
}

impl<I: Iterator + Clone> Iterator for CycleIterator<I>
where
    I::Item: Clone,
{
    type Item = I::Item;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.count.is_some() && self.remaining == 0 {
                return None;
            }

            if let Some(item) = self.current.next() {
                return Some(item);
            }

            match self.count {
                Some(_) => {
                    self.remaining = self.remaining.saturating_sub(1);
                    if self.remaining == 0 {
                        return None;
                    }
                    self.current = self.original.clone();
                }
                None => {
                    self.current = self.original.clone();
                }
            }
        }
    }
}

/// Batch iterator that yields chunks.
pub struct BatchIterator<I: Iterator> {
    inner: I,
    batch_size: usize,
}

impl<I: Iterator> BatchIterator<I> {
    /// Create new batch iterator.
    pub fn new(inner: I, batch_size: usize) -> Self {
        Self { inner, batch_size }
    }
}

impl<I: Iterator> Iterator for BatchIterator<I> {
    type Item = Vec<I::Item>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut batch = Vec::with_capacity(self.batch_size);
        for _ in 0..self.batch_size {
            match self.inner.next() {
                Some(item) => batch.push(item),
                None => break,
            }
        }
        if batch.is_empty() {
            None
        } else {
            Some(batch)
        }
    }
}

/// Interleave iterator.
pub struct InterleaveIterator<I, J>
where
    I: Iterator,
    J: Iterator<Item = I::Item>,
{
    first: I,
    second: J,
    use_first: bool,
}

impl<I, J> InterleaveIterator<I, J>
where
    I: Iterator,
    J: Iterator<Item = I::Item>,
{
    /// Create new interleave iterator.
    pub fn new(first: I, second: J) -> Self {
        Self {
            first,
            second,
            use_first: true,
        }
    }
}

impl<I, J> Iterator for InterleaveIterator<I, J>
where
    I: Iterator,
    J: Iterator<Item = I::Item>,
{
    type Item = I::Item;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.use_first {
                self.use_first = false;
                if let Some(item) = self.first.next() {
                    return Some(item);
                }
                if let Some(item) = self.second.next() {
                    return Some(item);
                }
                return None;
            } else {
                self.use_first = true;
                if let Some(item) = self.second.next() {
                    return Some(item);
                }
                if let Some(item) = self.first.next() {
                    return Some(item);
                }
                return None;
            }
        }
    }
}

/// Peeking iterator wrapper.
pub struct PeekIterator<I: Iterator> {
    inner: I,
    peeked: Option<Option<I::Item>>,
}

impl<I: Iterator> PeekIterator<I> {
    /// Create new peeking iterator.
    pub fn new(inner: I) -> Self {
        Self {
            inner,
            peeked: None,
        }
    }

    /// Peek at next element.
    pub fn peek(&mut self) -> Option<&I::Item> {
        if self.peeked.is_none() {
            self.peeked = Some(self.inner.next());
        }
        self.peeked.as_ref().unwrap().as_ref()
    }
}

impl<I: Iterator> Iterator for PeekIterator<I> {
    type Item = I::Item;

    fn next(&mut self) -> Option<Self::Item> {
        match self.peeked.take() {
            Some(v) => v,
            None => self.inner.next(),
        }
    }
}

/// Sliding window iterator.
pub struct WindowIterator<T: Clone> {
    data: Vec<T>,
    window_size: usize,
    position: usize,
}

impl<T: Clone> WindowIterator<T> {
    /// Create new window iterator.
    pub fn new(data: Vec<T>, window_size: usize) -> Self {
        Self {
            data,
            window_size,
            position: 0,
        }
    }
}

impl<T: Clone> Iterator for WindowIterator<T> {
    type Item = Vec<T>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.position + self.window_size <= self.data.len() {
            let window = self.data[self.position..self.position + self.window_size].to_vec();
            self.position += 1;
            Some(window)
        } else {
            None
        }
    }
}

/// Extension trait for iterator helpers.
pub trait IteratorExt: Iterator + Sized {
    /// Batch into chunks.
    fn batch(self, size: usize) -> BatchIterator<Self> {
        BatchIterator::new(self, size)
    }

    /// Create peeking iterator.
    fn peekable_ext(self) -> PeekIterator<Self> {
        PeekIterator::new(self)
    }
}

impl<I: Iterator> IteratorExt for I {}

/// Create range iterator.
pub fn range(start: i64, end: i64) -> RangeIterator {
    RangeIterator::new(start, end)
}

/// Create stepped range.
pub fn range_step(start: i64, end: i64, step: i64) -> RangeIterator {
    RangeIterator::with_step(start, end, step)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_range_iterator() {
        let result: Vec<_> = RangeIterator::new(0, 5).collect();
        assert_eq!(result, vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn test_range_step() {
        let result: Vec<_> = RangeIterator::with_step(0, 10, 2).collect();
        assert_eq!(result, vec![0, 2, 4, 6, 8]);
    }

    #[test]
    fn test_batch_iterator() {
        let result: Vec<_> = vec![1, 2, 3, 4, 5].into_iter().batch(2).collect();
        assert_eq!(result, vec![vec![1, 2], vec![3, 4], vec![5]]);
    }

    #[test]
    fn test_cycle_iterator() {
        let result: Vec<_> = CycleIterator::times(vec![1, 2].into_iter(), 3).collect();
        assert_eq!(result, vec![1, 2, 1, 2, 1, 2]);
    }

    #[test]
    fn test_interleave() {
        let result: Vec<_> =
            InterleaveIterator::new(vec![1, 3, 5].into_iter(), vec![2, 4, 6].into_iter()).collect();
        assert_eq!(result, vec![1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn test_window_iterator() {
        let result: Vec<_> = WindowIterator::new(vec![1, 2, 3, 4, 5], 3).collect();
        assert_eq!(result, vec![vec![1, 2, 3], vec![2, 3, 4], vec![3, 4, 5]]);
    }
}
