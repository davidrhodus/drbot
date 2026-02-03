//! Reader utilities for drbot.
//!
//! This crate provides:
//! - Buffered reading utilities
//! - Line reading
//! - Peek functionality

use std::io::{self, BufRead, Read};
use thiserror::Error;

/// Reader error types.
#[derive(Error, Debug)]
pub enum ReaderError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("End of input")]
    EndOfInput,

    #[error("Invalid data: {0}")]
    InvalidData(String),
}

/// Result type for reader operations.
pub type Result<T> = std::result::Result<T, ReaderError>;

/// Peekable byte reader.
pub struct PeekReader<R: Read> {
    inner: R,
    peeked: Option<u8>,
}

impl<R: Read> PeekReader<R> {
    /// Create new peek reader.
    pub fn new(inner: R) -> Self {
        Self {
            inner,
            peeked: None,
        }
    }

    /// Peek next byte without consuming.
    pub fn peek(&mut self) -> Result<Option<u8>> {
        if self.peeked.is_some() {
            return Ok(self.peeked);
        }

        let mut buf = [0u8; 1];
        match self.inner.read(&mut buf)? {
            0 => Ok(None),
            _ => {
                self.peeked = Some(buf[0]);
                Ok(self.peeked)
            }
        }
    }

    /// Read next byte.
    pub fn read_byte(&mut self) -> Result<Option<u8>> {
        if let Some(b) = self.peeked.take() {
            return Ok(Some(b));
        }

        let mut buf = [0u8; 1];
        match self.inner.read(&mut buf)? {
            0 => Ok(None),
            _ => Ok(Some(buf[0])),
        }
    }

    /// Get inner reader.
    pub fn into_inner(self) -> R {
        self.inner
    }
}

/// Line reader with line number tracking.
pub struct LineReader<R: BufRead> {
    inner: R,
    line_number: usize,
    buffer: String,
}

impl<R: BufRead> LineReader<R> {
    /// Create new line reader.
    pub fn new(inner: R) -> Self {
        Self {
            inner,
            line_number: 0,
            buffer: String::new(),
        }
    }

    /// Read next line.
    pub fn read_line(&mut self) -> Result<Option<&str>> {
        self.buffer.clear();
        match self.inner.read_line(&mut self.buffer)? {
            0 => Ok(None),
            _ => {
                self.line_number += 1;
                // Trim trailing newline
                if self.buffer.ends_with('\n') {
                    self.buffer.pop();
                    if self.buffer.ends_with('\r') {
                        self.buffer.pop();
                    }
                }
                Ok(Some(&self.buffer))
            }
        }
    }

    /// Get current line number.
    pub fn line_number(&self) -> usize {
        self.line_number
    }

    /// Get inner reader.
    pub fn into_inner(self) -> R {
        self.inner
    }
}

/// Counting reader that tracks bytes read.
pub struct CountingReader<R: Read> {
    inner: R,
    bytes_read: u64,
}

impl<R: Read> CountingReader<R> {
    /// Create new counting reader.
    pub fn new(inner: R) -> Self {
        Self {
            inner,
            bytes_read: 0,
        }
    }

    /// Get total bytes read.
    pub fn bytes_read(&self) -> u64 {
        self.bytes_read
    }

    /// Reset counter.
    pub fn reset_count(&mut self) {
        self.bytes_read = 0;
    }

    /// Get inner reader.
    pub fn into_inner(self) -> R {
        self.inner
    }
}

impl<R: Read> Read for CountingReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.bytes_read += n as u64;
        Ok(n)
    }
}

/// Limited reader that enforces a maximum read size.
pub struct LimitedReader<R: Read> {
    inner: R,
    limit: u64,
    read: u64,
}

impl<R: Read> LimitedReader<R> {
    /// Create new limited reader.
    pub fn new(inner: R, limit: u64) -> Self {
        Self {
            inner,
            limit,
            read: 0,
        }
    }

    /// Get remaining bytes.
    pub fn remaining(&self) -> u64 {
        self.limit.saturating_sub(self.read)
    }

    /// Check if limit reached.
    pub fn is_exhausted(&self) -> bool {
        self.read >= self.limit
    }

    /// Get inner reader.
    pub fn into_inner(self) -> R {
        self.inner
    }
}

impl<R: Read> Read for LimitedReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.read >= self.limit {
            return Ok(0);
        }

        let remaining = (self.limit - self.read) as usize;
        let to_read = buf.len().min(remaining);
        let n = self.inner.read(&mut buf[..to_read])?;
        self.read += n as u64;
        Ok(n)
    }
}

/// Read all bytes from reader.
pub fn read_all<R: Read>(reader: &mut R) -> Result<Vec<u8>> {
    let mut buffer = Vec::new();
    reader.read_to_end(&mut buffer)?;
    Ok(buffer)
}

/// Read all bytes with limit.
pub fn read_all_limited<R: Read>(reader: &mut R, limit: usize) -> Result<Vec<u8>> {
    let mut buffer = Vec::with_capacity(limit.min(1024));
    let mut total = 0;

    loop {
        if total >= limit {
            break;
        }

        let to_read = (limit - total).min(8192);
        let old_len = buffer.len();
        buffer.resize(old_len + to_read, 0);

        match reader.read(&mut buffer[old_len..]) {
            Ok(0) => {
                buffer.truncate(old_len);
                break;
            }
            Ok(n) => {
                buffer.truncate(old_len + n);
                total += n;
            }
            Err(e) => return Err(e.into()),
        }
    }

    Ok(buffer)
}

/// Read exact number of bytes.
pub fn read_exact<R: Read>(reader: &mut R, size: usize) -> Result<Vec<u8>> {
    let mut buffer = vec![0u8; size];
    reader.read_exact(&mut buffer)?;
    Ok(buffer)
}

/// Read until delimiter.
pub fn read_until<R: BufRead>(reader: &mut R, delimiter: u8) -> Result<Vec<u8>> {
    let mut buffer = Vec::new();
    reader.read_until(delimiter, &mut buffer)?;
    Ok(buffer)
}

/// Read lines from reader.
pub fn read_lines<R: BufRead>(reader: R) -> impl Iterator<Item = Result<String>> {
    reader.lines().map(|r| r.map_err(ReaderError::from))
}

/// Byte slice reader.
pub struct ByteReader<'a> {
    data: &'a [u8],
    position: usize,
}

impl<'a> ByteReader<'a> {
    /// Create new byte reader.
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, position: 0 }
    }

    /// Get remaining bytes.
    pub fn remaining(&self) -> &[u8] {
        &self.data[self.position..]
    }

    /// Check if at end.
    pub fn is_empty(&self) -> bool {
        self.position >= self.data.len()
    }

    /// Get current position.
    pub fn position(&self) -> usize {
        self.position
    }

    /// Read u8.
    pub fn read_u8(&mut self) -> Result<u8> {
        if self.position >= self.data.len() {
            return Err(ReaderError::EndOfInput);
        }
        let b = self.data[self.position];
        self.position += 1;
        Ok(b)
    }

    /// Read u16 big endian.
    pub fn read_u16_be(&mut self) -> Result<u16> {
        if self.position + 2 > self.data.len() {
            return Err(ReaderError::EndOfInput);
        }
        let value = u16::from_be_bytes([self.data[self.position], self.data[self.position + 1]]);
        self.position += 2;
        Ok(value)
    }

    /// Read u32 big endian.
    pub fn read_u32_be(&mut self) -> Result<u32> {
        if self.position + 4 > self.data.len() {
            return Err(ReaderError::EndOfInput);
        }
        let value = u32::from_be_bytes([
            self.data[self.position],
            self.data[self.position + 1],
            self.data[self.position + 2],
            self.data[self.position + 3],
        ]);
        self.position += 4;
        Ok(value)
    }

    /// Read bytes.
    pub fn read_bytes(&mut self, n: usize) -> Result<&[u8]> {
        if self.position + n > self.data.len() {
            return Err(ReaderError::EndOfInput);
        }
        let bytes = &self.data[self.position..self.position + n];
        self.position += n;
        Ok(bytes)
    }

    /// Skip bytes.
    pub fn skip(&mut self, n: usize) -> Result<()> {
        if self.position + n > self.data.len() {
            return Err(ReaderError::EndOfInput);
        }
        self.position += n;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_peek_reader() {
        let data = Cursor::new(vec![1, 2, 3]);
        let mut reader = PeekReader::new(data);

        assert_eq!(reader.peek().unwrap(), Some(1));
        assert_eq!(reader.peek().unwrap(), Some(1)); // Same value
        assert_eq!(reader.read_byte().unwrap(), Some(1));
        assert_eq!(reader.read_byte().unwrap(), Some(2));
    }

    #[test]
    fn test_line_reader() {
        let data = Cursor::new("line1\nline2\nline3".as_bytes());
        let mut reader = LineReader::new(data);

        assert_eq!(reader.read_line().unwrap(), Some("line1"));
        assert_eq!(reader.line_number(), 1);
        assert_eq!(reader.read_line().unwrap(), Some("line2"));
        assert_eq!(reader.line_number(), 2);
    }

    #[test]
    fn test_counting_reader() {
        let data = Cursor::new(vec![1, 2, 3, 4, 5]);
        let mut reader = CountingReader::new(data);

        let mut buf = [0u8; 3];
        reader.read(&mut buf).unwrap();
        assert_eq!(reader.bytes_read(), 3);
    }

    #[test]
    fn test_limited_reader() {
        let data = Cursor::new(vec![1, 2, 3, 4, 5]);
        let mut reader = LimitedReader::new(data, 3);

        let mut buf = Vec::new();
        reader.read_to_end(&mut buf).unwrap();
        assert_eq!(buf, vec![1, 2, 3]);
    }

    #[test]
    fn test_byte_reader() {
        let data = vec![0x00, 0x01, 0x00, 0x02, 0x00, 0x00, 0x00, 0x03];
        let mut reader = ByteReader::new(&data);

        assert_eq!(reader.read_u8().unwrap(), 0);
        assert_eq!(reader.read_u8().unwrap(), 1);
        assert_eq!(reader.read_u16_be().unwrap(), 2);
        assert_eq!(reader.read_u32_be().unwrap(), 3);
    }
}
