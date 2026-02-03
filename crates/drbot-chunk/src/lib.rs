//! Chunking utilities for drbot.
//!
//! This crate provides:
//! - Chunk operations
//! - Fixed-size chunking
//! - Dynamic chunking

use thiserror::Error;

/// Chunk error types.
#[derive(Error, Debug, Clone)]
pub enum ChunkError {
    #[error("Invalid chunk size")]
    InvalidSize,

    #[error("Not enough elements")]
    NotEnough,
}

/// Result type for chunk operations.
pub type Result<T> = std::result::Result<T, ChunkError>;

/// Chunk slice into fixed-size chunks.
pub fn chunks<T>(slice: &[T], chunk_size: usize) -> impl Iterator<Item = &[T]> {
    slice.chunks(chunk_size)
}

/// Chunk slice into exact-size chunks.
pub fn chunks_exact<T>(slice: &[T], chunk_size: usize) -> impl Iterator<Item = &[T]> {
    slice.chunks_exact(chunk_size)
}

/// Chunk mutable slice.
pub fn chunks_mut<T>(slice: &mut [T], chunk_size: usize) -> impl Iterator<Item = &mut [T]> {
    slice.chunks_mut(chunk_size)
}

/// Chunk exact mutable.
pub fn chunks_exact_mut<T>(slice: &mut [T], chunk_size: usize) -> impl Iterator<Item = &mut [T]> {
    slice.chunks_exact_mut(chunk_size)
}

/// Collect chunks into vec.
pub fn chunk_to_vec<T: Clone>(slice: &[T], chunk_size: usize) -> Vec<Vec<T>> {
    slice.chunks(chunk_size).map(|c| c.to_vec()).collect()
}

/// Chunk iter into fixed-size chunks.
pub fn chunk_iter<T, I: Iterator<Item = T>>(iter: I, chunk_size: usize) -> ChunkedIter<T, I> {
    ChunkedIter {
        iter,
        chunk_size,
        done: false,
    }
}

/// Chunked iterator.
pub struct ChunkedIter<T, I: Iterator<Item = T>> {
    iter: I,
    chunk_size: usize,
    done: bool,
}

impl<T, I: Iterator<Item = T>> Iterator for ChunkedIter<T, I> {
    type Item = Vec<T>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done || self.chunk_size == 0 {
            return None;
        }

        let mut chunk = Vec::with_capacity(self.chunk_size);
        for _ in 0..self.chunk_size {
            match self.iter.next() {
                Some(item) => chunk.push(item),
                None => {
                    self.done = true;
                    break;
                }
            }
        }

        if chunk.is_empty() {
            None
        } else {
            Some(chunk)
        }
    }
}

/// Split into n equal chunks.
pub fn split_into_n<T: Clone>(slice: &[T], n: usize) -> Vec<Vec<T>> {
    if n == 0 {
        return vec![];
    }
    let chunk_size = (slice.len() + n - 1) / n;
    chunk_to_vec(slice, chunk_size)
}

/// Chunk by predicate (split when predicate returns true).
pub fn chunk_by<T, F>(slice: &[T], mut predicate: F) -> Vec<&[T]>
where
    F: FnMut(&T) -> bool,
{
    let mut chunks = Vec::new();
    let mut start = 0;

    for (i, item) in slice.iter().enumerate() {
        if predicate(item) && i > start {
            chunks.push(&slice[start..i]);
            start = i;
        }
    }

    if start < slice.len() {
        chunks.push(&slice[start..]);
    }

    chunks
}

/// Chunk vec by predicate.
pub fn chunk_vec_by<T: Clone, F>(vec: &[T], predicate: F) -> Vec<Vec<T>>
where
    F: FnMut(&T) -> bool,
{
    chunk_by(vec, predicate)
        .into_iter()
        .map(|c| c.to_vec())
        .collect()
}

/// Fixed chunk wrapper.
#[derive(Debug, Clone)]
pub struct Chunk<T> {
    data: Vec<T>,
    size: usize,
}

impl<T> Chunk<T> {
    /// Create new chunk.
    pub fn new(size: usize) -> Self {
        Self {
            data: Vec::with_capacity(size),
            size,
        }
    }

    /// Push item.
    pub fn push(&mut self, item: T) -> Option<T> {
        if self.data.len() >= self.size {
            Some(item)
        } else {
            self.data.push(item);
            None
        }
    }

    /// Is full.
    pub fn is_full(&self) -> bool {
        self.data.len() >= self.size
    }

    /// Is empty.
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Length.
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Capacity.
    pub fn capacity(&self) -> usize {
        self.size
    }

    /// Remaining.
    pub fn remaining(&self) -> usize {
        self.size.saturating_sub(self.data.len())
    }

    /// Take chunk data.
    pub fn take(&mut self) -> Vec<T> {
        std::mem::take(&mut self.data)
    }

    /// Get slice.
    pub fn as_slice(&self) -> &[T] {
        &self.data
    }

    /// Get mutable slice.
    pub fn as_mut_slice(&mut self) -> &mut [T] {
        &mut self.data
    }

    /// Clear chunk.
    pub fn clear(&mut self) {
        self.data.clear();
    }
}

impl<T> AsRef<[T]> for Chunk<T> {
    fn as_ref(&self) -> &[T] {
        &self.data
    }
}

impl<T> AsMut<[T]> for Chunk<T> {
    fn as_mut(&mut self) -> &mut [T] {
        &mut self.data
    }
}

/// Buffered chunker for streaming data.
pub struct Chunker<T> {
    buffer: Vec<T>,
    chunk_size: usize,
}

impl<T> Chunker<T> {
    /// Create new chunker.
    pub fn new(chunk_size: usize) -> Self {
        Self {
            buffer: Vec::with_capacity(chunk_size),
            chunk_size,
        }
    }

    /// Push item, returns chunk if full.
    pub fn push(&mut self, item: T) -> Option<Vec<T>> {
        self.buffer.push(item);
        if self.buffer.len() >= self.chunk_size {
            Some(std::mem::replace(
                &mut self.buffer,
                Vec::with_capacity(self.chunk_size),
            ))
        } else {
            None
        }
    }

    /// Flush remaining items.
    pub fn flush(&mut self) -> Option<Vec<T>> {
        if self.buffer.is_empty() {
            None
        } else {
            Some(std::mem::replace(
                &mut self.buffer,
                Vec::with_capacity(self.chunk_size),
            ))
        }
    }

    /// Buffered count.
    pub fn buffered(&self) -> usize {
        self.buffer.len()
    }

    /// Is empty.
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunks() {
        let arr = [1, 2, 3, 4, 5];
        let chunks: Vec<_> = chunks(&arr, 2).collect();
        assert_eq!(chunks, vec![&[1, 2][..], &[3, 4], &[5]]);
    }

    #[test]
    fn test_chunk_iter() {
        let items = vec![1, 2, 3, 4, 5];
        let chunks: Vec<_> = chunk_iter(items.into_iter(), 2).collect();
        assert_eq!(chunks, vec![vec![1, 2], vec![3, 4], vec![5]]);
    }

    #[test]
    fn test_split_into_n() {
        let arr = [1, 2, 3, 4, 5];
        let chunks = split_into_n(&arr, 2);
        assert_eq!(chunks.len(), 2);
    }

    #[test]
    fn test_chunk() {
        let mut chunk = Chunk::new(3);
        chunk.push(1);
        chunk.push(2);
        assert!(!chunk.is_full());
        chunk.push(3);
        assert!(chunk.is_full());
    }

    #[test]
    fn test_chunker() {
        let mut chunker = Chunker::new(2);
        assert!(chunker.push(1).is_none());
        let chunk = chunker.push(2).unwrap();
        assert_eq!(chunk, vec![1, 2]);
        chunker.push(3);
        let rest = chunker.flush().unwrap();
        assert_eq!(rest, vec![3]);
    }
}
