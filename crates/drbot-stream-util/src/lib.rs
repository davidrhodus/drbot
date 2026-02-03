//! Stream processing utilities for drbot.
//!
//! This crate provides:
//! - Stream chunking
//! - Batching utilities
//! - Stream combinators

use thiserror::Error;

/// Stream error types.
#[derive(Error, Debug, Clone)]
pub enum StreamError {
    #[error("Stream ended")]
    EndOfStream,

    #[error("Stream error: {0}")]
    Error(String),

    #[error("Timeout")]
    Timeout,
}

/// Result type for stream operations.
pub type Result<T> = std::result::Result<T, StreamError>;

/// Chunk data into fixed-size pieces.
pub struct Chunker {
    chunk_size: usize,
}

impl Chunker {
    /// Create new chunker.
    pub fn new(chunk_size: usize) -> Self {
        assert!(chunk_size > 0, "Chunk size must be positive");
        Self { chunk_size }
    }

    /// Chunk a slice.
    pub fn chunk<'a, T>(&self, data: &'a [T]) -> impl Iterator<Item = &'a [T]> {
        data.chunks(self.chunk_size)
    }

    /// Chunk bytes.
    pub fn chunk_bytes<'a>(&self, data: &'a [u8]) -> impl Iterator<Item = &'a [u8]> {
        data.chunks(self.chunk_size)
    }

    /// Chunk a vector into owned chunks.
    pub fn chunk_owned<T: Clone>(&self, data: Vec<T>) -> Vec<Vec<T>> {
        data.chunks(self.chunk_size).map(|c| c.to_vec()).collect()
    }
}

/// Batch items by count or timeout.
#[derive(Debug)]
pub struct Batcher<T> {
    items: Vec<T>,
    max_size: usize,
}

impl<T> Batcher<T> {
    /// Create new batcher.
    pub fn new(max_size: usize) -> Self {
        Self {
            items: Vec::with_capacity(max_size),
            max_size,
        }
    }

    /// Add item to batch.
    pub fn add(&mut self, item: T) -> Option<Vec<T>> {
        self.items.push(item);
        if self.items.len() >= self.max_size {
            Some(self.flush())
        } else {
            None
        }
    }

    /// Flush current batch.
    pub fn flush(&mut self) -> Vec<T> {
        std::mem::replace(&mut self.items, Vec::with_capacity(self.max_size))
    }

    /// Check if batch is empty.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Get current batch size.
    pub fn len(&self) -> usize {
        self.items.len()
    }
}

/// Window over a stream.
#[derive(Debug)]
pub struct SlidingWindow<T> {
    items: Vec<T>,
    window_size: usize,
}

impl<T: Clone> SlidingWindow<T> {
    /// Create new sliding window.
    pub fn new(window_size: usize) -> Self {
        Self {
            items: Vec::with_capacity(window_size),
            window_size,
        }
    }

    /// Add item to window.
    pub fn push(&mut self, item: T) {
        if self.items.len() >= self.window_size {
            self.items.remove(0);
        }
        self.items.push(item);
    }

    /// Get current window.
    pub fn window(&self) -> &[T] {
        &self.items
    }

    /// Check if window is full.
    pub fn is_full(&self) -> bool {
        self.items.len() >= self.window_size
    }

    /// Clear window.
    pub fn clear(&mut self) {
        self.items.clear();
    }
}

/// Buffer for collecting stream items.
#[derive(Debug)]
pub struct StreamBuffer<T> {
    buffer: Vec<T>,
    capacity: usize,
}

impl<T> StreamBuffer<T> {
    /// Create new buffer.
    pub fn new(capacity: usize) -> Self {
        Self {
            buffer: Vec::with_capacity(capacity),
            capacity,
        }
    }

    /// Create unlimited buffer.
    pub fn unlimited() -> Self {
        Self {
            buffer: Vec::new(),
            capacity: usize::MAX,
        }
    }

    /// Push item to buffer.
    pub fn push(&mut self, item: T) -> bool {
        if self.buffer.len() < self.capacity {
            self.buffer.push(item);
            true
        } else {
            false
        }
    }

    /// Pop item from buffer.
    pub fn pop(&mut self) -> Option<T> {
        if !self.buffer.is_empty() {
            Some(self.buffer.remove(0))
        } else {
            None
        }
    }

    /// Drain all items.
    pub fn drain(&mut self) -> Vec<T> {
        std::mem::take(&mut self.buffer)
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    /// Check if full.
    pub fn is_full(&self) -> bool {
        self.buffer.len() >= self.capacity
    }

    /// Get length.
    pub fn len(&self) -> usize {
        self.buffer.len()
    }
}

/// Rate limiter for streams.
#[derive(Debug)]
pub struct StreamRateLimiter {
    items_per_second: f64,
    last_item_time: Option<std::time::Instant>,
}

impl StreamRateLimiter {
    /// Create new rate limiter.
    pub fn new(items_per_second: f64) -> Self {
        Self {
            items_per_second,
            last_item_time: None,
        }
    }

    /// Calculate delay before next item.
    pub fn next_delay(&mut self) -> std::time::Duration {
        let now = std::time::Instant::now();
        let min_interval = std::time::Duration::from_secs_f64(1.0 / self.items_per_second);

        let delay = match self.last_item_time {
            Some(last) => {
                let elapsed = now.duration_since(last);
                if elapsed < min_interval {
                    min_interval - elapsed
                } else {
                    std::time::Duration::ZERO
                }
            }
            None => std::time::Duration::ZERO,
        };

        self.last_item_time = Some(now + delay);
        delay
    }

    /// Mark item as processed.
    pub fn mark(&mut self) {
        self.last_item_time = Some(std::time::Instant::now());
    }
}

/// Progress tracker for streams.
#[derive(Debug, Clone)]
pub struct StreamProgress {
    total: Option<usize>,
    processed: usize,
}

impl StreamProgress {
    /// Create new progress tracker.
    pub fn new(total: Option<usize>) -> Self {
        Self {
            total,
            processed: 0,
        }
    }

    /// Increment processed count.
    pub fn increment(&mut self) {
        self.processed += 1;
    }

    /// Increment by amount.
    pub fn increment_by(&mut self, amount: usize) {
        self.processed += amount;
    }

    /// Get processed count.
    pub fn processed(&self) -> usize {
        self.processed
    }

    /// Get total (if known).
    pub fn total(&self) -> Option<usize> {
        self.total
    }

    /// Get progress as percentage.
    pub fn percentage(&self) -> Option<f64> {
        self.total
            .map(|t| (self.processed as f64 / t as f64) * 100.0)
    }

    /// Check if complete.
    pub fn is_complete(&self) -> bool {
        self.total.map(|t| self.processed >= t).unwrap_or(false)
    }
}

/// Map function over iterator.
pub fn map_iter<I, F, T, U>(iter: I, f: F) -> impl Iterator<Item = U>
where
    I: Iterator<Item = T>,
    F: Fn(T) -> U,
{
    iter.map(f)
}

/// Filter iterator.
pub fn filter_iter<I, F, T>(iter: I, predicate: F) -> impl Iterator<Item = T>
where
    I: Iterator<Item = T>,
    F: Fn(&T) -> bool,
{
    iter.filter(predicate)
}

/// Take first n items.
pub fn take_iter<I, T>(iter: I, n: usize) -> impl Iterator<Item = T>
where
    I: Iterator<Item = T>,
{
    iter.take(n)
}

/// Skip first n items.
pub fn skip_iter<I, T>(iter: I, n: usize) -> impl Iterator<Item = T>
where
    I: Iterator<Item = T>,
{
    iter.skip(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunker() {
        let chunker = Chunker::new(3);
        let data = vec![1, 2, 3, 4, 5, 6, 7];
        let chunks: Vec<_> = chunker.chunk(&data).collect();

        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0], &[1, 2, 3]);
        assert_eq!(chunks[1], &[4, 5, 6]);
        assert_eq!(chunks[2], &[7]);
    }

    #[test]
    fn test_batcher() {
        let mut batcher = Batcher::new(3);

        assert!(batcher.add(1).is_none());
        assert!(batcher.add(2).is_none());
        let batch = batcher.add(3);

        assert!(batch.is_some());
        assert_eq!(batch.unwrap(), vec![1, 2, 3]);
    }

    #[test]
    fn test_sliding_window() {
        let mut window = SlidingWindow::new(3);

        window.push(1);
        window.push(2);
        window.push(3);
        assert!(window.is_full());
        assert_eq!(window.window(), &[1, 2, 3]);

        window.push(4);
        assert_eq!(window.window(), &[2, 3, 4]);
    }

    #[test]
    fn test_stream_buffer() {
        let mut buffer = StreamBuffer::new(2);

        assert!(buffer.push(1));
        assert!(buffer.push(2));
        assert!(!buffer.push(3)); // Full

        assert_eq!(buffer.pop(), Some(1));
        assert!(buffer.push(3)); // Now has space
    }

    #[test]
    fn test_progress() {
        let mut progress = StreamProgress::new(Some(100));
        progress.increment_by(50);

        assert_eq!(progress.percentage(), Some(50.0));
        assert!(!progress.is_complete());

        progress.increment_by(50);
        assert!(progress.is_complete());
    }
}
