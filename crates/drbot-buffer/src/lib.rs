//! Buffer utilities for drbot.
//!
//! This crate provides:
//! - Byte buffer
//! - Double buffer
//! - Line buffer
//! - Chunk buffer

use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use thiserror::Error;

/// Buffer error types.
#[derive(Error, Debug)]
pub enum BufferError {
    #[error("Buffer full")]
    Full,

    #[error("Buffer empty")]
    Empty,

    #[error("Insufficient space: need {need}, have {have}")]
    InsufficientSpace { need: usize, have: usize },

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Result type for buffer operations.
pub type Result<T> = std::result::Result<T, BufferError>;

/// Byte buffer.
pub struct ByteBuffer {
    data: Vec<u8>,
    capacity: usize,
    read_pos: usize,
    write_pos: usize,
}

impl ByteBuffer {
    /// Create new byte buffer.
    pub fn new(capacity: usize) -> Self {
        Self {
            data: vec![0; capacity],
            capacity,
            read_pos: 0,
            write_pos: 0,
        }
    }

    /// Get capacity.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Get readable bytes.
    pub fn readable(&self) -> usize {
        self.write_pos - self.read_pos
    }

    /// Get writable space.
    pub fn writable(&self) -> usize {
        self.capacity - self.write_pos
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.readable() == 0
    }

    /// Write bytes.
    pub fn write_bytes(&mut self, data: &[u8]) -> Result<usize> {
        let to_write = data.len().min(self.writable());
        if to_write == 0 && !data.is_empty() {
            return Err(BufferError::Full);
        }

        self.data[self.write_pos..self.write_pos + to_write].copy_from_slice(&data[..to_write]);
        self.write_pos += to_write;
        Ok(to_write)
    }

    /// Read bytes.
    pub fn read_bytes(&mut self, buf: &mut [u8]) -> usize {
        let to_read = buf.len().min(self.readable());
        buf[..to_read].copy_from_slice(&self.data[self.read_pos..self.read_pos + to_read]);
        self.read_pos += to_read;
        to_read
    }

    /// Peek bytes without consuming.
    pub fn peek(&self, buf: &mut [u8]) -> usize {
        let to_read = buf.len().min(self.readable());
        buf[..to_read].copy_from_slice(&self.data[self.read_pos..self.read_pos + to_read]);
        to_read
    }

    /// Get readable slice.
    pub fn as_slice(&self) -> &[u8] {
        &self.data[self.read_pos..self.write_pos]
    }

    /// Consume N bytes.
    pub fn consume(&mut self, n: usize) -> usize {
        let consumed = n.min(self.readable());
        self.read_pos += consumed;
        consumed
    }

    /// Compact buffer.
    pub fn compact(&mut self) {
        if self.read_pos > 0 {
            self.data.copy_within(self.read_pos..self.write_pos, 0);
            self.write_pos -= self.read_pos;
            self.read_pos = 0;
        }
    }

    /// Clear buffer.
    pub fn clear(&mut self) {
        self.read_pos = 0;
        self.write_pos = 0;
    }
}

impl Write for ByteBuffer {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        Ok(self.write_bytes(buf).unwrap_or(0))
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Read for ByteBuffer {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        Ok(self.read_bytes(buf))
    }
}

/// Double buffer for swapping.
pub struct DoubleBuffer<T> {
    front: T,
    back: T,
}

impl<T: Default> DoubleBuffer<T> {
    /// Create new double buffer.
    pub fn new() -> Self {
        Self {
            front: T::default(),
            back: T::default(),
        }
    }

    /// Get front buffer.
    pub fn front(&self) -> &T {
        &self.front
    }

    /// Get back buffer.
    pub fn back(&self) -> &T {
        &self.back
    }

    /// Get mutable back buffer.
    pub fn back_mut(&mut self) -> &mut T {
        &mut self.back
    }

    /// Swap buffers.
    pub fn swap(&mut self) {
        std::mem::swap(&mut self.front, &mut self.back);
    }
}

impl<T: Default> Default for DoubleBuffer<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Line buffer.
pub struct LineBuffer {
    buffer: String,
    max_line_length: Option<usize>,
}

impl LineBuffer {
    /// Create new line buffer.
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
            max_line_length: None,
        }
    }

    /// Create with max line length.
    pub fn with_max_length(max: usize) -> Self {
        Self {
            buffer: String::new(),
            max_line_length: Some(max),
        }
    }

    /// Append text.
    pub fn append(&mut self, text: &str) {
        self.buffer.push_str(text);
    }

    /// Get next complete line.
    pub fn next_line(&mut self) -> Option<String> {
        if let Some(pos) = self.buffer.find('\n') {
            let line = self.buffer[..pos].to_string();
            self.buffer.drain(..=pos);
            Some(line)
        } else if let Some(max) = self.max_line_length {
            if self.buffer.len() >= max {
                let line = self.buffer.drain(..max).collect();
                return Some(line);
            }
            None
        } else {
            None
        }
    }

    /// Get all complete lines.
    pub fn lines(&mut self) -> Vec<String> {
        let mut lines = Vec::new();
        while let Some(line) = self.next_line() {
            lines.push(line);
        }
        lines
    }

    /// Flush remaining content.
    pub fn flush(&mut self) -> Option<String> {
        if self.buffer.is_empty() {
            None
        } else {
            Some(std::mem::take(&mut self.buffer))
        }
    }

    /// Check if has pending data.
    pub fn has_pending(&self) -> bool {
        !self.buffer.is_empty()
    }

    /// Clear buffer.
    pub fn clear(&mut self) {
        self.buffer.clear();
    }
}

impl Default for LineBuffer {
    fn default() -> Self {
        Self::new()
    }
}

/// Chunk buffer.
pub struct ChunkBuffer<T> {
    chunks: Vec<Vec<T>>,
    chunk_size: usize,
    current: Vec<T>,
}

impl<T> ChunkBuffer<T> {
    /// Create new chunk buffer.
    pub fn new(chunk_size: usize) -> Self {
        Self {
            chunks: Vec::new(),
            chunk_size,
            current: Vec::with_capacity(chunk_size),
        }
    }

    /// Push item.
    pub fn push(&mut self, item: T) {
        self.current.push(item);
        if self.current.len() >= self.chunk_size {
            let chunk = std::mem::replace(&mut self.current, Vec::with_capacity(self.chunk_size));
            self.chunks.push(chunk);
        }
    }

    /// Get complete chunks.
    pub fn take_chunks(&mut self) -> Vec<Vec<T>> {
        std::mem::take(&mut self.chunks)
    }

    /// Flush remaining as final chunk.
    pub fn flush(&mut self) -> Option<Vec<T>> {
        if self.current.is_empty() {
            None
        } else {
            Some(std::mem::replace(
                &mut self.current,
                Vec::with_capacity(self.chunk_size),
            ))
        }
    }

    /// Get total item count.
    pub fn len(&self) -> usize {
        self.chunks.iter().map(|c| c.len()).sum::<usize>() + self.current.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.chunks.is_empty() && self.current.is_empty()
    }

    /// Get chunk count.
    pub fn chunk_count(&self) -> usize {
        self.chunks.len()
    }
}

/// Shared buffer.
pub struct SharedBuffer<T> {
    inner: Arc<Mutex<Vec<T>>>,
    capacity: Option<usize>,
}

impl<T> SharedBuffer<T> {
    /// Create new shared buffer.
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
        let mut buf = self.inner.lock().unwrap();
        if let Some(cap) = self.capacity {
            if buf.len() >= cap {
                return Err(BufferError::Full);
            }
        }
        buf.push(item);
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

    /// Clear buffer.
    pub fn clear(&self) {
        self.inner.lock().unwrap().clear();
    }

    /// Drain all items.
    pub fn drain(&self) -> Vec<T> {
        self.inner.lock().unwrap().drain(..).collect()
    }
}

impl<T> Clone for SharedBuffer<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            capacity: self.capacity,
        }
    }
}

impl<T> Default for SharedBuffer<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_byte_buffer() {
        let mut buf = ByteBuffer::new(16);

        buf.write_bytes(b"hello").unwrap();
        assert_eq!(buf.readable(), 5);

        let mut out = [0u8; 3];
        buf.read_bytes(&mut out);
        assert_eq!(&out, b"hel");
        assert_eq!(buf.readable(), 2);
    }

    #[test]
    fn test_double_buffer() {
        let mut db: DoubleBuffer<Vec<i32>> = DoubleBuffer::new();

        db.back_mut().push(1);
        db.back_mut().push(2);
        db.swap();

        assert_eq!(db.front(), &vec![1, 2]);
        assert!(db.back().is_empty());
    }

    #[test]
    fn test_line_buffer() {
        let mut lb = LineBuffer::new();

        lb.append("hello\nworld\n");
        assert_eq!(lb.next_line(), Some("hello".to_string()));
        assert_eq!(lb.next_line(), Some("world".to_string()));
        assert_eq!(lb.next_line(), None);
    }

    #[test]
    fn test_chunk_buffer() {
        let mut cb = ChunkBuffer::new(3);

        cb.push(1);
        cb.push(2);
        cb.push(3);
        cb.push(4);

        let chunks = cb.take_chunks();
        assert_eq!(chunks, vec![vec![1, 2, 3]]);

        let remaining = cb.flush();
        assert_eq!(remaining, Some(vec![4]));
    }
}

// ============================================================================
// Kani Formal Verification Proofs
// ============================================================================

#[cfg(kani)]
mod kani_proofs {
    use super::*;

    // ========================================================================
    // ByteBuffer Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_byte_buffer_new() {
        let cap: u8 = kani::any();
        kani::assume(cap > 0 && cap <= 64);

        let buf = ByteBuffer::new(cap as usize);

        kani::assert(buf.capacity() == cap as usize, "capacity matches");
        kani::assert(buf.readable() == 0, "no readable bytes");
        kani::assert(buf.writable() == cap as usize, "all space writable");
        kani::assert(buf.is_empty(), "buffer is empty");
    }

    #[kani::proof]
    fn proof_byte_buffer_write_read() {
        let mut buf = ByteBuffer::new(16);

        let result = buf.write_bytes(b"test");
        kani::assert(result.is_ok(), "write succeeds");
        kani::assert(result.unwrap() == 4, "wrote 4 bytes");
        kani::assert(buf.readable() == 4, "4 bytes readable");

        let mut out = [0u8; 4];
        let read = buf.read_bytes(&mut out);
        kani::assert(read == 4, "read 4 bytes");
        kani::assert(buf.readable() == 0, "no bytes readable");
    }

    #[kani::proof]
    fn proof_byte_buffer_peek() {
        let mut buf = ByteBuffer::new(16);
        buf.write_bytes(b"hello").unwrap();

        let mut out = [0u8; 3];
        let peeked = buf.peek(&mut out);

        kani::assert(peeked == 3, "peeked 3 bytes");
        kani::assert(buf.readable() == 5, "readable unchanged after peek");
    }

    #[kani::proof]
    fn proof_byte_buffer_consume() {
        let mut buf = ByteBuffer::new(16);
        buf.write_bytes(b"hello").unwrap();

        let consumed = buf.consume(3);
        kani::assert(consumed == 3, "consumed 3 bytes");
        kani::assert(buf.readable() == 2, "2 bytes left");
    }

    #[kani::proof]
    fn proof_byte_buffer_clear() {
        let mut buf = ByteBuffer::new(16);
        buf.write_bytes(b"hello").unwrap();
        buf.clear();

        kani::assert(buf.is_empty(), "buffer empty after clear");
        kani::assert(buf.readable() == 0, "no readable bytes");
        kani::assert(buf.writable() == 16, "all space writable");
    }

    #[kani::proof]
    fn proof_byte_buffer_compact() {
        let mut buf = ByteBuffer::new(16);
        buf.write_bytes(b"hello").unwrap();
        buf.consume(3); // Read "hel"
        buf.compact();

        kani::assert(buf.readable() == 2, "2 bytes still readable");
        kani::assert(buf.writable() == 14, "more space writable after compact");
    }

    #[kani::proof]
    fn proof_byte_buffer_full() {
        let mut buf = ByteBuffer::new(4);
        buf.write_bytes(b"full").unwrap();

        let result = buf.write_bytes(b"x");
        kani::assert(result.is_err(), "write to full buffer fails");
    }

    // ========================================================================
    // DoubleBuffer Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_double_buffer_new() {
        let db: DoubleBuffer<Vec<i32>> = DoubleBuffer::new();

        kani::assert(db.front().is_empty(), "front is empty");
        kani::assert(db.back().is_empty(), "back is empty");
    }

    #[kani::proof]
    fn proof_double_buffer_default() {
        let db: DoubleBuffer<i32> = DoubleBuffer::default();

        kani::assert(*db.front() == 0, "front is default");
        kani::assert(*db.back() == 0, "back is default");
    }

    #[kani::proof]
    fn proof_double_buffer_back_mut() {
        let mut db: DoubleBuffer<i32> = DoubleBuffer::new();

        *db.back_mut() = 42;

        kani::assert(*db.back() == 42, "back modified");
        kani::assert(*db.front() == 0, "front unchanged");
    }

    #[kani::proof]
    fn proof_double_buffer_swap() {
        let mut db: DoubleBuffer<i32> = DoubleBuffer::new();

        *db.back_mut() = 42;
        db.swap();

        kani::assert(*db.front() == 42, "front has old back value");
        kani::assert(*db.back() == 0, "back has old front value");
    }

    #[kani::proof]
    fn proof_double_buffer_swap_twice() {
        let mut db: DoubleBuffer<i32> = DoubleBuffer::new();

        *db.back_mut() = 42;
        db.swap();
        db.swap();

        kani::assert(*db.back() == 42, "back restored after two swaps");
    }

    // ========================================================================
    // LineBuffer Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_line_buffer_new() {
        let lb = LineBuffer::new();

        kani::assert(!lb.has_pending(), "no pending data");
    }

    #[kani::proof]
    fn proof_line_buffer_default() {
        let lb = LineBuffer::default();

        kani::assert(!lb.has_pending(), "no pending data");
    }

    #[kani::proof]
    fn proof_line_buffer_append() {
        let mut lb = LineBuffer::new();
        lb.append("hello");

        kani::assert(lb.has_pending(), "has pending data");
    }

    #[kani::proof]
    fn proof_line_buffer_next_line_no_newline() {
        let mut lb = LineBuffer::new();
        lb.append("hello");

        let line = lb.next_line();
        kani::assert(line.is_none(), "no complete line without newline");
    }

    #[kani::proof]
    fn proof_line_buffer_next_line_with_newline() {
        let mut lb = LineBuffer::new();
        lb.append("hello\n");

        let line = lb.next_line();
        kani::assert(line.is_some(), "has complete line");
        kani::assert(line.unwrap() == "hello", "line content correct");
    }

    #[kani::proof]
    fn proof_line_buffer_flush() {
        let mut lb = LineBuffer::new();
        lb.append("hello");

        let flushed = lb.flush();
        kani::assert(flushed.is_some(), "flush returns content");
        kani::assert(flushed.unwrap() == "hello", "flushed content correct");
        kani::assert(!lb.has_pending(), "no pending after flush");
    }

    #[kani::proof]
    fn proof_line_buffer_flush_empty() {
        let mut lb = LineBuffer::new();

        let flushed = lb.flush();
        kani::assert(flushed.is_none(), "flush on empty returns None");
    }

    #[kani::proof]
    fn proof_line_buffer_clear() {
        let mut lb = LineBuffer::new();
        lb.append("hello");
        lb.clear();

        kani::assert(!lb.has_pending(), "no pending after clear");
    }

    // ========================================================================
    // ChunkBuffer Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_chunk_buffer_new() {
        let cb: ChunkBuffer<i32> = ChunkBuffer::new(3);

        kani::assert(cb.is_empty(), "new chunk buffer is empty");
        kani::assert(cb.len() == 0, "len is 0");
        kani::assert(cb.chunk_count() == 0, "no complete chunks");
    }

    #[kani::proof]
    fn proof_chunk_buffer_push_no_chunk() {
        let mut cb: ChunkBuffer<i32> = ChunkBuffer::new(3);
        cb.push(1);
        cb.push(2);

        kani::assert(cb.len() == 2, "len is 2");
        kani::assert(cb.chunk_count() == 0, "no complete chunks yet");
    }

    #[kani::proof]
    fn proof_chunk_buffer_push_creates_chunk() {
        let mut cb: ChunkBuffer<i32> = ChunkBuffer::new(3);
        cb.push(1);
        cb.push(2);
        cb.push(3);

        kani::assert(cb.chunk_count() == 1, "one complete chunk");
        kani::assert(cb.len() == 3, "len is 3");
    }

    #[kani::proof]
    fn proof_chunk_buffer_take_chunks() {
        let mut cb: ChunkBuffer<i32> = ChunkBuffer::new(2);
        cb.push(1);
        cb.push(2);
        cb.push(3);

        let chunks = cb.take_chunks();
        kani::assert(chunks.len() == 1, "took one chunk");
        kani::assert(cb.chunk_count() == 0, "no chunks after take");
    }

    #[kani::proof]
    fn proof_chunk_buffer_flush() {
        let mut cb: ChunkBuffer<i32> = ChunkBuffer::new(3);
        cb.push(1);

        let remaining = cb.flush();
        kani::assert(remaining.is_some(), "flush returns partial");
        kani::assert(remaining.unwrap().len() == 1, "partial has 1 item");
    }

    #[kani::proof]
    fn proof_chunk_buffer_flush_empty() {
        let mut cb: ChunkBuffer<i32> = ChunkBuffer::new(3);

        let remaining = cb.flush();
        kani::assert(remaining.is_none(), "flush empty returns None");
    }

    // ========================================================================
    // SharedBuffer Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_shared_buffer_new() {
        let buf: SharedBuffer<i32> = SharedBuffer::new();

        kani::assert(buf.is_empty(), "new shared buffer is empty");
        kani::assert(buf.len() == 0, "len is 0");
    }

    #[kani::proof]
    fn proof_shared_buffer_default() {
        let buf: SharedBuffer<i32> = SharedBuffer::default();

        kani::assert(buf.is_empty(), "default shared buffer is empty");
    }

    #[kani::proof]
    fn proof_shared_buffer_push_pop() {
        let buf: SharedBuffer<i32> = SharedBuffer::new();

        buf.push(42).unwrap();
        kani::assert(buf.len() == 1, "len is 1 after push");

        let popped = buf.pop();
        kani::assert(popped == Some(42), "pop returns pushed value");
        kani::assert(buf.is_empty(), "empty after pop");
    }

    #[kani::proof]
    fn proof_shared_buffer_pop_empty() {
        let buf: SharedBuffer<i32> = SharedBuffer::new();

        let popped = buf.pop();
        kani::assert(popped.is_none(), "pop empty returns None");
    }

    #[kani::proof]
    fn proof_shared_buffer_with_capacity_limit() {
        let buf: SharedBuffer<i32> = SharedBuffer::with_capacity(2);

        buf.push(1).unwrap();
        buf.push(2).unwrap();
        let result = buf.push(3);

        kani::assert(result.is_err(), "push beyond capacity fails");
    }

    #[kani::proof]
    fn proof_shared_buffer_clear() {
        let buf: SharedBuffer<i32> = SharedBuffer::new();
        buf.push(1).unwrap();
        buf.push(2).unwrap();
        buf.clear();

        kani::assert(buf.is_empty(), "empty after clear");
    }

    #[kani::proof]
    fn proof_shared_buffer_drain() {
        let buf: SharedBuffer<i32> = SharedBuffer::new();
        buf.push(1).unwrap();
        buf.push(2).unwrap();

        let drained = buf.drain();
        kani::assert(drained.len() == 2, "drained 2 items");
        kani::assert(buf.is_empty(), "empty after drain");
    }

    #[kani::proof]
    fn proof_shared_buffer_clone() {
        let buf1: SharedBuffer<i32> = SharedBuffer::new();
        buf1.push(42).unwrap();

        let buf2 = buf1.clone();
        kani::assert(buf2.len() == 1, "clone shares state");
    }
}
