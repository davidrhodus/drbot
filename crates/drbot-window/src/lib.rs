//! Sliding window utilities for drbot.
//!
//! This crate provides:
//! - Sliding window operations
//! - Window iterators
//! - Rolling window buffers

use std::collections::VecDeque;
use thiserror::Error;

/// Window error types.
#[derive(Error, Debug, Clone)]
pub enum WindowError {
    #[error("Invalid window size")]
    InvalidSize,

    #[error("Window not full")]
    NotFull,
}

/// Result type for window operations.
pub type Result<T> = std::result::Result<T, WindowError>;

/// Sliding windows over a slice.
pub fn windows<T>(slice: &[T], size: usize) -> impl Iterator<Item = &[T]> {
    slice.windows(size)
}

/// Collect windows to vec.
pub fn windows_to_vec<T: Clone>(slice: &[T], size: usize) -> Vec<Vec<T>> {
    slice.windows(size).map(|w| w.to_vec()).collect()
}

/// Sliding window buffer.
#[derive(Debug, Clone)]
pub struct SlidingWindow<T> {
    buffer: VecDeque<T>,
    size: usize,
}

impl<T> SlidingWindow<T> {
    /// Create new window.
    pub fn new(size: usize) -> Self {
        Self {
            buffer: VecDeque::with_capacity(size),
            size,
        }
    }

    /// Push item, returns oldest if window was full.
    pub fn push(&mut self, item: T) -> Option<T> {
        let old = if self.buffer.len() >= self.size {
            self.buffer.pop_front()
        } else {
            None
        };
        self.buffer.push_back(item);
        old
    }

    /// Is full.
    pub fn is_full(&self) -> bool {
        self.buffer.len() >= self.size
    }

    /// Is empty.
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    /// Length.
    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    /// Window size.
    pub fn window_size(&self) -> usize {
        self.size
    }

    /// Get item at index.
    pub fn get(&self, index: usize) -> Option<&T> {
        self.buffer.get(index)
    }

    /// Get oldest item.
    pub fn oldest(&self) -> Option<&T> {
        self.buffer.front()
    }

    /// Get newest item.
    pub fn newest(&self) -> Option<&T> {
        self.buffer.back()
    }

    /// Iterate items.
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.buffer.iter()
    }

    /// Clear window.
    pub fn clear(&mut self) {
        self.buffer.clear();
    }

    /// As slice (may not be contiguous).
    pub fn as_slices(&self) -> (&[T], &[T]) {
        self.buffer.as_slices()
    }

    /// Drain all items.
    pub fn drain(&mut self) -> impl Iterator<Item = T> + '_ {
        self.buffer.drain(..)
    }
}

impl<T: Clone> SlidingWindow<T> {
    /// To vec.
    pub fn to_vec(&self) -> Vec<T> {
        self.buffer.iter().cloned().collect()
    }
}

/// Rolling statistics window.
#[derive(Debug, Clone)]
pub struct RollingWindow {
    values: VecDeque<f64>,
    size: usize,
    sum: f64,
}

impl RollingWindow {
    /// Create new.
    pub fn new(size: usize) -> Self {
        Self {
            values: VecDeque::with_capacity(size),
            size,
            sum: 0.0,
        }
    }

    /// Push value.
    pub fn push(&mut self, value: f64) {
        if self.values.len() >= self.size {
            if let Some(old) = self.values.pop_front() {
                self.sum -= old;
            }
        }
        self.sum += value;
        self.values.push_back(value);
    }

    /// Current mean.
    pub fn mean(&self) -> Option<f64> {
        if self.values.is_empty() {
            None
        } else {
            Some(self.sum / self.values.len() as f64)
        }
    }

    /// Current sum.
    pub fn sum(&self) -> f64 {
        self.sum
    }

    /// Current min.
    pub fn min(&self) -> Option<f64> {
        self.values.iter().cloned().reduce(f64::min)
    }

    /// Current max.
    pub fn max(&self) -> Option<f64> {
        self.values.iter().cloned().reduce(f64::max)
    }

    /// Is full.
    pub fn is_full(&self) -> bool {
        self.values.len() >= self.size
    }

    /// Count.
    pub fn count(&self) -> usize {
        self.values.len()
    }

    /// Clear.
    pub fn clear(&mut self) {
        self.values.clear();
        self.sum = 0.0;
    }
}

/// Window iterator that yields windows.
pub struct WindowIter<T, I: Iterator<Item = T>> {
    iter: I,
    buffer: VecDeque<T>,
    size: usize,
    started: bool,
}

impl<T: Clone, I: Iterator<Item = T>> WindowIter<T, I> {
    /// Create new.
    pub fn new(iter: I, size: usize) -> Self {
        Self {
            iter,
            buffer: VecDeque::with_capacity(size),
            size,
            started: false,
        }
    }
}

impl<T: Clone, I: Iterator<Item = T>> Iterator for WindowIter<T, I> {
    type Item = Vec<T>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.size == 0 {
            return None;
        }

        // Fill buffer initially
        while self.buffer.len() < self.size {
            match self.iter.next() {
                Some(item) => self.buffer.push_back(item),
                None => return None,
            }
        }

        if !self.started {
            self.started = true;
            return Some(self.buffer.iter().cloned().collect());
        }

        // Slide window
        match self.iter.next() {
            Some(item) => {
                self.buffer.pop_front();
                self.buffer.push_back(item);
                Some(self.buffer.iter().cloned().collect())
            }
            None => None,
        }
    }
}

/// Create window iterator.
pub fn window_iter<T: Clone, I: Iterator<Item = T>>(iter: I, size: usize) -> WindowIter<T, I> {
    WindowIter::new(iter, size)
}

/// Pair-wise windows (adjacent pairs).
pub fn pairs<T>(slice: &[T]) -> impl Iterator<Item = (&T, &T)> {
    slice.windows(2).map(|w| (&w[0], &w[1]))
}

/// Triple windows.
pub fn triples<T>(slice: &[T]) -> impl Iterator<Item = (&T, &T, &T)> {
    slice.windows(3).map(|w| (&w[0], &w[1], &w[2]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_windows() {
        let arr = [1, 2, 3, 4, 5];
        let windows: Vec<_> = windows(&arr, 3).collect();
        assert_eq!(windows, vec![&[1, 2, 3][..], &[2, 3, 4], &[3, 4, 5]]);
    }

    #[test]
    fn test_sliding_window() {
        let mut window = SlidingWindow::new(3);
        window.push(1);
        window.push(2);
        window.push(3);
        assert!(window.is_full());

        let old = window.push(4);
        assert_eq!(old, Some(1));
        assert_eq!(window.oldest(), Some(&2));
        assert_eq!(window.newest(), Some(&4));
    }

    #[test]
    fn test_rolling_window() {
        let mut window = RollingWindow::new(3);
        window.push(1.0);
        window.push(2.0);
        window.push(3.0);
        assert_eq!(window.mean(), Some(2.0));

        window.push(4.0);
        assert_eq!(window.mean(), Some(3.0)); // (2+3+4)/3
    }

    #[test]
    fn test_window_iter() {
        let v = vec![1, 2, 3, 4, 5];
        let windows: Vec<_> = window_iter(v.into_iter(), 3).collect();
        assert_eq!(windows, vec![vec![1, 2, 3], vec![2, 3, 4], vec![3, 4, 5]]);
    }

    #[test]
    fn test_pairs() {
        let arr = [1, 2, 3, 4];
        let pairs: Vec<_> = pairs(&arr).collect();
        assert_eq!(pairs, vec![(&1, &2), (&2, &3), (&3, &4)]);
    }
}
