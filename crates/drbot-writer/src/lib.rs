//! Writer utilities for drbot.
//!
//! This crate provides:
//! - Buffered writing utilities
//! - Counting writer
//! - Limited writer

use std::io::{self, Write};
use thiserror::Error;

/// Writer error types.
#[derive(Error, Debug)]
pub enum WriterError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("Write limit exceeded: {limit} bytes")]
    LimitExceeded { limit: u64 },

    #[error("Writer closed")]
    Closed,
}

/// Result type for writer operations.
pub type Result<T> = std::result::Result<T, WriterError>;

/// Counting writer that tracks bytes written.
pub struct CountingWriter<W: Write> {
    inner: W,
    bytes_written: u64,
}

impl<W: Write> CountingWriter<W> {
    /// Create new counting writer.
    pub fn new(inner: W) -> Self {
        Self {
            inner,
            bytes_written: 0,
        }
    }

    /// Get total bytes written.
    pub fn bytes_written(&self) -> u64 {
        self.bytes_written
    }

    /// Reset counter.
    pub fn reset_count(&mut self) {
        self.bytes_written = 0;
    }

    /// Get inner writer.
    pub fn into_inner(self) -> W {
        self.inner
    }

    /// Get reference to inner writer.
    pub fn get_ref(&self) -> &W {
        &self.inner
    }

    /// Get mutable reference to inner writer.
    pub fn get_mut(&mut self) -> &mut W {
        &mut self.inner
    }
}

impl<W: Write> Write for CountingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.bytes_written += n as u64;
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

/// Limited writer that enforces a maximum write size.
pub struct LimitedWriter<W: Write> {
    inner: W,
    limit: u64,
    written: u64,
}

impl<W: Write> LimitedWriter<W> {
    /// Create new limited writer.
    pub fn new(inner: W, limit: u64) -> Self {
        Self {
            inner,
            limit,
            written: 0,
        }
    }

    /// Get remaining capacity.
    pub fn remaining(&self) -> u64 {
        self.limit.saturating_sub(self.written)
    }

    /// Check if limit reached.
    pub fn is_full(&self) -> bool {
        self.written >= self.limit
    }

    /// Get bytes written.
    pub fn bytes_written(&self) -> u64 {
        self.written
    }

    /// Get inner writer.
    pub fn into_inner(self) -> W {
        self.inner
    }
}

impl<W: Write> Write for LimitedWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.written >= self.limit {
            return Err(io::Error::new(
                io::ErrorKind::WriteZero,
                "Write limit exceeded",
            ));
        }

        let remaining = (self.limit - self.written) as usize;
        let to_write = buf.len().min(remaining);
        let n = self.inner.write(&buf[..to_write])?;
        self.written += n as u64;
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

/// Null writer that discards all data.
pub struct NullWriter;

impl Write for NullWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Tee writer that writes to two destinations.
pub struct TeeWriter<W1: Write, W2: Write> {
    writer1: W1,
    writer2: W2,
}

impl<W1: Write, W2: Write> TeeWriter<W1, W2> {
    /// Create new tee writer.
    pub fn new(writer1: W1, writer2: W2) -> Self {
        Self { writer1, writer2 }
    }

    /// Get inner writers.
    pub fn into_inner(self) -> (W1, W2) {
        (self.writer1, self.writer2)
    }
}

impl<W1: Write, W2: Write> Write for TeeWriter<W1, W2> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n1 = self.writer1.write(buf)?;
        self.writer2.write_all(&buf[..n1])?;
        Ok(n1)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.writer1.flush()?;
        self.writer2.flush()
    }
}

/// Byte writer for building byte buffers.
pub struct ByteWriter {
    buffer: Vec<u8>,
}

impl ByteWriter {
    /// Create new byte writer.
    pub fn new() -> Self {
        Self { buffer: Vec::new() }
    }

    /// Create with capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            buffer: Vec::with_capacity(capacity),
        }
    }

    /// Write u8.
    pub fn write_u8(&mut self, value: u8) {
        self.buffer.push(value);
    }

    /// Write u16 big endian.
    pub fn write_u16_be(&mut self, value: u16) {
        self.buffer.extend_from_slice(&value.to_be_bytes());
    }

    /// Write u16 little endian.
    pub fn write_u16_le(&mut self, value: u16) {
        self.buffer.extend_from_slice(&value.to_le_bytes());
    }

    /// Write u32 big endian.
    pub fn write_u32_be(&mut self, value: u32) {
        self.buffer.extend_from_slice(&value.to_be_bytes());
    }

    /// Write u32 little endian.
    pub fn write_u32_le(&mut self, value: u32) {
        self.buffer.extend_from_slice(&value.to_le_bytes());
    }

    /// Write u64 big endian.
    pub fn write_u64_be(&mut self, value: u64) {
        self.buffer.extend_from_slice(&value.to_be_bytes());
    }

    /// Write u64 little endian.
    pub fn write_u64_le(&mut self, value: u64) {
        self.buffer.extend_from_slice(&value.to_le_bytes());
    }

    /// Write bytes.
    pub fn write_bytes(&mut self, data: &[u8]) {
        self.buffer.extend_from_slice(data);
    }

    /// Write zeros.
    pub fn write_zeros(&mut self, count: usize) {
        self.buffer.resize(self.buffer.len() + count, 0);
    }

    /// Get current length.
    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    /// Get buffer reference.
    pub fn as_bytes(&self) -> &[u8] {
        &self.buffer
    }

    /// Consume and return buffer.
    pub fn into_bytes(self) -> Vec<u8> {
        self.buffer
    }

    /// Clear buffer.
    pub fn clear(&mut self) {
        self.buffer.clear();
    }
}

impl Default for ByteWriter {
    fn default() -> Self {
        Self::new()
    }
}

impl Write for ByteWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.buffer.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Line writer for writing text lines.
pub struct LineWriter<W: Write> {
    inner: W,
    line_ending: &'static str,
}

impl<W: Write> LineWriter<W> {
    /// Create new line writer with LF endings.
    pub fn new(inner: W) -> Self {
        Self {
            inner,
            line_ending: "\n",
        }
    }

    /// Create with CRLF endings.
    pub fn with_crlf(inner: W) -> Self {
        Self {
            inner,
            line_ending: "\r\n",
        }
    }

    /// Write a line.
    pub fn write_line(&mut self, line: &str) -> io::Result<()> {
        self.inner.write_all(line.as_bytes())?;
        self.inner.write_all(self.line_ending.as_bytes())
    }

    /// Write multiple lines.
    pub fn write_lines<I, S>(&mut self, lines: I) -> io::Result<()>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        for line in lines {
            self.write_line(line.as_ref())?;
        }
        Ok(())
    }

    /// Get inner writer.
    pub fn into_inner(self) -> W {
        self.inner
    }
}

impl<W: Write> Write for LineWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.inner.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_counting_writer() {
        let mut writer = CountingWriter::new(Vec::new());
        writer.write_all(b"hello").unwrap();
        writer.write_all(b" world").unwrap();

        assert_eq!(writer.bytes_written(), 11);
        assert_eq!(writer.into_inner(), b"hello world");
    }

    #[test]
    fn test_limited_writer() {
        let mut writer = LimitedWriter::new(Vec::new(), 5);
        writer.write_all(b"hel").unwrap();
        writer.write_all(b"lo").unwrap();

        assert!(writer.is_full());
        assert!(writer.write(b"!").is_err());
    }

    #[test]
    fn test_tee_writer() {
        let mut writer = TeeWriter::new(Vec::new(), Vec::new());
        writer.write_all(b"test").unwrap();

        let (w1, w2) = writer.into_inner();
        assert_eq!(w1, b"test");
        assert_eq!(w2, b"test");
    }

    #[test]
    fn test_byte_writer() {
        let mut writer = ByteWriter::new();
        writer.write_u8(1);
        writer.write_u16_be(0x0203);
        writer.write_u32_be(0x04050607);

        assert_eq!(writer.as_bytes(), &[1, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07]);
    }

    #[test]
    fn test_line_writer() {
        let mut writer = LineWriter::new(Vec::new());
        writer.write_line("line1").unwrap();
        writer.write_line("line2").unwrap();

        assert_eq!(writer.into_inner(), b"line1\nline2\n");
    }
}
