//! Sequence utilities for drbot.
//!
//! This crate provides:
//! - Sequence generators
//! - Sequence operations (window, chunk, etc.)
//! - Sequence comparison

use thiserror::Error;

/// Sequence error types.
#[derive(Error, Debug)]
pub enum SequenceError {
    #[error("Empty sequence")]
    Empty,

    #[error("Invalid window size")]
    InvalidWindowSize,

    #[error("Index out of bounds")]
    IndexOutOfBounds,
}

/// Result type for sequence operations.
pub type Result<T> = std::result::Result<T, SequenceError>;

/// Sliding window iterator.
pub struct Windows<'a, T> {
    slice: &'a [T],
    size: usize,
    pos: usize,
}

impl<'a, T> Windows<'a, T> {
    /// Create new window iterator.
    pub fn new(slice: &'a [T], size: usize) -> Self {
        Self {
            slice,
            size,
            pos: 0,
        }
    }
}

impl<'a, T> Iterator for Windows<'a, T> {
    type Item = &'a [T];

    fn next(&mut self) -> Option<Self::Item> {
        if self.pos + self.size > self.slice.len() {
            return None;
        }
        let window = &self.slice[self.pos..self.pos + self.size];
        self.pos += 1;
        Some(window)
    }
}

/// Chunk iterator.
pub struct Chunks<'a, T> {
    slice: &'a [T],
    size: usize,
    pos: usize,
}

impl<'a, T> Chunks<'a, T> {
    /// Create new chunk iterator.
    pub fn new(slice: &'a [T], size: usize) -> Self {
        Self {
            slice,
            size,
            pos: 0,
        }
    }
}

impl<'a, T> Iterator for Chunks<'a, T> {
    type Item = &'a [T];

    fn next(&mut self) -> Option<Self::Item> {
        if self.pos >= self.slice.len() {
            return None;
        }
        let end = (self.pos + self.size).min(self.slice.len());
        let chunk = &self.slice[self.pos..end];
        self.pos = end;
        Some(chunk)
    }
}

/// Create sliding windows over a slice.
pub fn windows<T>(slice: &[T], size: usize) -> Windows<'_, T> {
    Windows::new(slice, size)
}

/// Create chunks over a slice.
pub fn chunks<T>(slice: &[T], size: usize) -> Chunks<'_, T> {
    Chunks::new(slice, size)
}

/// Interleave two iterators.
pub fn interleave<T, I, J>(a: I, b: J) -> impl Iterator<Item = T>
where
    I: IntoIterator<Item = T>,
    J: IntoIterator<Item = T>,
{
    a.into_iter()
        .zip(b)
        .flat_map(|(x, y)| std::iter::once(x).chain(std::iter::once(y)))
}

/// Generate range sequence.
pub fn range(start: i64, end: i64) -> impl Iterator<Item = i64> {
    start..end
}

/// Generate range with step.
pub fn range_step(start: i64, end: i64, step: i64) -> impl Iterator<Item = i64> {
    let direction = if step > 0 { 1 } else { -1 };
    std::iter::successors(Some(start), move |&n| {
        let next = n + step;
        if direction > 0 && next < end {
            Some(next)
        } else if direction < 0 && next > end {
            Some(next)
        } else {
            None
        }
    })
}

/// Generate repeat sequence.
pub fn repeat<T: Clone>(value: T, count: usize) -> impl Iterator<Item = T> {
    std::iter::repeat(value).take(count)
}

/// Generate cycle sequence.
pub fn cycle<T: Clone>(values: Vec<T>) -> impl Iterator<Item = T> {
    values.into_iter().cycle()
}

/// Take first n elements.
pub fn take<T, I: IntoIterator<Item = T>>(iter: I, n: usize) -> impl Iterator<Item = T> {
    iter.into_iter().take(n)
}

/// Skip first n elements.
pub fn skip<T, I: IntoIterator<Item = T>>(iter: I, n: usize) -> impl Iterator<Item = T> {
    iter.into_iter().skip(n)
}

/// Take while predicate is true.
pub fn take_while<T, I, F>(iter: I, predicate: F) -> impl Iterator<Item = T>
where
    I: IntoIterator<Item = T>,
    F: FnMut(&T) -> bool,
{
    iter.into_iter().take_while(predicate)
}

/// Skip while predicate is true.
pub fn skip_while<T, I, F>(iter: I, predicate: F) -> impl Iterator<Item = T>
where
    I: IntoIterator<Item = T>,
    F: FnMut(&T) -> bool,
{
    iter.into_iter().skip_while(predicate)
}

/// Check if sequence is sorted.
pub fn is_sorted<T: Ord>(slice: &[T]) -> bool {
    slice.windows(2).all(|w| w[0] <= w[1])
}

/// Check if sequence is strictly sorted.
pub fn is_strictly_sorted<T: Ord>(slice: &[T]) -> bool {
    slice.windows(2).all(|w| w[0] < w[1])
}

/// Find longest common prefix.
pub fn common_prefix<'a, T: Eq>(a: &'a [T], b: &'a [T]) -> &'a [T] {
    let len = a.iter().zip(b.iter()).take_while(|(x, y)| x == y).count();
    &a[..len]
}

/// Find longest common suffix.
pub fn common_suffix<'a, T: Eq>(a: &'a [T], b: &'a [T]) -> &'a [T] {
    let len = a
        .iter()
        .rev()
        .zip(b.iter().rev())
        .take_while(|(x, y)| x == y)
        .count();
    &a[a.len() - len..]
}

/// Deduplicate consecutive elements.
pub fn dedup<T: Eq + Clone>(slice: &[T]) -> Vec<T> {
    let mut result = Vec::new();
    for item in slice {
        if result.last() != Some(item) {
            result.push(item.clone());
        }
    }
    result
}

/// Unique elements (preserves order).
pub fn unique<T: Eq + Clone>(slice: &[T]) -> Vec<T> {
    let mut result = Vec::new();
    for item in slice {
        if !result.contains(item) {
            result.push(item.clone());
        }
    }
    result
}

/// Flatten nested iterators.
pub fn flatten<T, I, J>(iter: I) -> impl Iterator<Item = T>
where
    I: IntoIterator<Item = J>,
    J: IntoIterator<Item = T>,
{
    iter.into_iter().flatten()
}

/// Enumerate with offset.
pub fn enumerate_from<T, I>(iter: I, start: usize) -> impl Iterator<Item = (usize, T)>
where
    I: IntoIterator<Item = T>,
{
    iter.into_iter()
        .enumerate()
        .map(move |(i, v)| (i + start, v))
}

/// Find runs of consecutive values.
pub fn runs<T: Eq + Clone>(slice: &[T]) -> Vec<(T, usize)> {
    let mut result = Vec::new();
    let mut iter = slice.iter();

    if let Some(first) = iter.next() {
        let mut current = first.clone();
        let mut count = 1;

        for item in iter {
            if item == &current {
                count += 1;
            } else {
                result.push((current, count));
                current = item.clone();
                count = 1;
            }
        }
        result.push((current, count));
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_windows() {
        let data = vec![1, 2, 3, 4, 5];
        let wins: Vec<_> = windows(&data, 3).collect();
        assert_eq!(wins.len(), 3);
        assert_eq!(wins[0], &[1, 2, 3]);
        assert_eq!(wins[1], &[2, 3, 4]);
    }

    #[test]
    fn test_chunks() {
        let data = vec![1, 2, 3, 4, 5];
        let cks: Vec<_> = chunks(&data, 2).collect();
        assert_eq!(cks.len(), 3);
        assert_eq!(cks[0], &[1, 2]);
        assert_eq!(cks[2], &[5]);
    }

    #[test]
    fn test_is_sorted() {
        assert!(is_sorted(&[1, 2, 3, 4]));
        assert!(is_sorted(&[1, 1, 2, 3]));
        assert!(!is_sorted(&[1, 3, 2, 4]));
    }

    #[test]
    fn test_common_prefix() {
        let a = vec![1, 2, 3, 4];
        let b = vec![1, 2, 5, 6];
        assert_eq!(common_prefix(&a, &b), &[1, 2]);
    }

    #[test]
    fn test_dedup() {
        let data = vec![1, 1, 2, 2, 2, 3, 1, 1];
        assert_eq!(dedup(&data), vec![1, 2, 3, 1]);
    }

    #[test]
    fn test_unique() {
        let data = vec![1, 2, 1, 3, 2, 4];
        assert_eq!(unique(&data), vec![1, 2, 3, 4]);
    }

    #[test]
    fn test_runs() {
        let data = vec!['a', 'a', 'b', 'b', 'b', 'a'];
        let runs = runs(&data);
        assert_eq!(runs, vec![('a', 2), ('b', 3), ('a', 1)]);
    }
}
