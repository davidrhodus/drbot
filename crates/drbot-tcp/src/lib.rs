//! TCP utilities for drbot.
//!
//! This crate provides:
//! - TCP connection management
//! - Connection pooling
//! - Keep-alive support
//! - Framing protocols

use async_trait::async_trait;
use bytes::{Buf, BufMut, BytesMut};
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Mutex, Semaphore};
use tokio::time::timeout;

/// TCP error types.
#[derive(Error, Debug)]
pub enum TcpError {
    #[error("IO error: {0}")]
    IoError(#[from] io::Error),

    #[error("Connection timeout")]
    Timeout,

    #[error("Connection refused")]
    ConnectionRefused,

    #[error("Pool exhausted")]
    PoolExhausted,

    #[error("Frame too large: {size} > {max}")]
    FrameTooLarge { size: usize, max: usize },

    #[error("Invalid frame")]
    InvalidFrame,
}

/// Result type for TCP operations.
pub type Result<T> = std::result::Result<T, TcpError>;

/// TCP connection wrapper.
pub struct TcpConnection {
    stream: TcpStream,
    read_buffer: BytesMut,
    write_buffer: BytesMut,
}

impl TcpConnection {
    /// Create from stream.
    pub fn new(stream: TcpStream) -> Self {
        Self {
            stream,
            read_buffer: BytesMut::with_capacity(8192),
            write_buffer: BytesMut::with_capacity(8192),
        }
    }

    /// Connect to address.
    pub async fn connect(addr: &str) -> Result<Self> {
        let stream = TcpStream::connect(addr).await?;
        Ok(Self::new(stream))
    }

    /// Connect with timeout.
    pub async fn connect_timeout(addr: &str, duration: Duration) -> Result<Self> {
        let stream = timeout(duration, TcpStream::connect(addr))
            .await
            .map_err(|_| TcpError::Timeout)??;
        Ok(Self::new(stream))
    }

    /// Get peer address.
    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        self.stream.peer_addr()
    }

    /// Get local address.
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.stream.local_addr()
    }

    /// Set TCP nodelay.
    pub fn set_nodelay(&self, nodelay: bool) -> io::Result<()> {
        self.stream.set_nodelay(nodelay)
    }

    /// Read exact bytes.
    pub async fn read_exact(&mut self, buf: &mut [u8]) -> Result<()> {
        self.stream.read_exact(buf).await?;
        Ok(())
    }

    /// Read up to n bytes.
    pub async fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        let n = self.stream.read(buf).await?;
        Ok(n)
    }

    /// Write all bytes.
    pub async fn write_all(&mut self, buf: &[u8]) -> Result<()> {
        self.stream.write_all(buf).await?;
        Ok(())
    }

    /// Flush write buffer.
    pub async fn flush(&mut self) -> Result<()> {
        self.stream.flush().await?;
        Ok(())
    }

    /// Shutdown connection.
    pub async fn shutdown(&mut self) -> Result<()> {
        self.stream.shutdown().await?;
        Ok(())
    }
}

/// Frame codec trait.
#[async_trait]
pub trait FrameCodec: Send + Sync {
    /// Decode frame from buffer.
    fn decode(&self, buf: &mut BytesMut) -> Result<Option<Vec<u8>>>;

    /// Encode frame to buffer.
    fn encode(&self, frame: &[u8], buf: &mut BytesMut) -> Result<()>;
}

/// Length-prefixed frame codec.
pub struct LengthPrefixCodec {
    max_frame_size: usize,
    length_bytes: usize,
}

impl LengthPrefixCodec {
    /// Create new codec with 4-byte length prefix.
    pub fn new(max_frame_size: usize) -> Self {
        Self {
            max_frame_size,
            length_bytes: 4,
        }
    }

    /// Use 2-byte length prefix.
    pub fn length_u16(mut self) -> Self {
        self.length_bytes = 2;
        self
    }
}

impl Default for LengthPrefixCodec {
    fn default() -> Self {
        Self::new(1024 * 1024) // 1MB default
    }
}

#[async_trait]
impl FrameCodec for LengthPrefixCodec {
    fn decode(&self, buf: &mut BytesMut) -> Result<Option<Vec<u8>>> {
        if buf.len() < self.length_bytes {
            return Ok(None);
        }

        let length = if self.length_bytes == 2 {
            let mut len_buf = [0u8; 2];
            len_buf.copy_from_slice(&buf[..2]);
            u16::from_be_bytes(len_buf) as usize
        } else {
            let mut len_buf = [0u8; 4];
            len_buf.copy_from_slice(&buf[..4]);
            u32::from_be_bytes(len_buf) as usize
        };

        if length > self.max_frame_size {
            return Err(TcpError::FrameTooLarge {
                size: length,
                max: self.max_frame_size,
            });
        }

        let total_len = self.length_bytes + length;
        if buf.len() < total_len {
            return Ok(None);
        }

        buf.advance(self.length_bytes);
        let frame = buf.split_to(length).to_vec();
        Ok(Some(frame))
    }

    fn encode(&self, frame: &[u8], buf: &mut BytesMut) -> Result<()> {
        if frame.len() > self.max_frame_size {
            return Err(TcpError::FrameTooLarge {
                size: frame.len(),
                max: self.max_frame_size,
            });
        }

        if self.length_bytes == 2 {
            buf.put_u16(frame.len() as u16);
        } else {
            buf.put_u32(frame.len() as u32);
        }
        buf.put_slice(frame);
        Ok(())
    }
}

/// Line-based frame codec.
pub struct LineCodec {
    max_line_size: usize,
}

impl LineCodec {
    /// Create new line codec.
    pub fn new(max_line_size: usize) -> Self {
        Self { max_line_size }
    }
}

impl Default for LineCodec {
    fn default() -> Self {
        Self::new(8192)
    }
}

#[async_trait]
impl FrameCodec for LineCodec {
    fn decode(&self, buf: &mut BytesMut) -> Result<Option<Vec<u8>>> {
        if let Some(pos) = buf.iter().position(|&b| b == b'\n') {
            if pos > self.max_line_size {
                return Err(TcpError::FrameTooLarge {
                    size: pos,
                    max: self.max_line_size,
                });
            }

            let mut line = buf.split_to(pos + 1);
            // Remove trailing \r\n or \n
            if line.ends_with(b"\r\n") {
                line.truncate(line.len() - 2);
            } else {
                line.truncate(line.len() - 1);
            }
            return Ok(Some(line.to_vec()));
        }

        if buf.len() > self.max_line_size {
            return Err(TcpError::FrameTooLarge {
                size: buf.len(),
                max: self.max_line_size,
            });
        }

        Ok(None)
    }

    fn encode(&self, frame: &[u8], buf: &mut BytesMut) -> Result<()> {
        buf.put_slice(frame);
        buf.put_u8(b'\n');
        Ok(())
    }
}

/// Framed connection.
pub struct FramedConnection<C: FrameCodec> {
    conn: TcpConnection,
    codec: C,
    read_buf: BytesMut,
}

impl<C: FrameCodec> FramedConnection<C> {
    /// Create framed connection.
    pub fn new(conn: TcpConnection, codec: C) -> Self {
        Self {
            conn,
            codec,
            read_buf: BytesMut::with_capacity(8192),
        }
    }

    /// Read next frame.
    pub async fn read_frame(&mut self) -> Result<Option<Vec<u8>>> {
        loop {
            if let Some(frame) = self.codec.decode(&mut self.read_buf)? {
                return Ok(Some(frame));
            }

            let mut temp_buf = [0u8; 4096];
            let n = self.conn.read(&mut temp_buf).await?;
            if n == 0 {
                return Ok(None);
            }
            self.read_buf.extend_from_slice(&temp_buf[..n]);
        }
    }

    /// Write frame.
    pub async fn write_frame(&mut self, frame: &[u8]) -> Result<()> {
        let mut buf = BytesMut::with_capacity(frame.len() + 8);
        self.codec.encode(frame, &mut buf)?;
        self.conn.write_all(&buf).await?;
        self.conn.flush().await
    }
}

/// TCP listener wrapper.
pub struct TcpServer {
    listener: TcpListener,
}

impl TcpServer {
    /// Bind to address.
    pub async fn bind(addr: &str) -> Result<Self> {
        let listener = TcpListener::bind(addr).await?;
        Ok(Self { listener })
    }

    /// Get local address.
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.listener.local_addr()
    }

    /// Accept connection.
    pub async fn accept(&self) -> Result<(TcpConnection, SocketAddr)> {
        let (stream, addr) = self.listener.accept().await?;
        Ok((TcpConnection::new(stream), addr))
    }
}

/// Simple connection pool.
pub struct ConnectionPool {
    addr: String,
    connections: Arc<Mutex<Vec<TcpConnection>>>,
    semaphore: Arc<Semaphore>,
    connect_timeout: Duration,
}

impl ConnectionPool {
    /// Create new pool.
    pub fn new(addr: impl Into<String>, max_connections: usize) -> Self {
        Self {
            addr: addr.into(),
            connections: Arc::new(Mutex::new(Vec::new())),
            semaphore: Arc::new(Semaphore::new(max_connections)),
            connect_timeout: Duration::from_secs(10),
        }
    }

    /// Set connect timeout.
    pub fn connect_timeout(mut self, duration: Duration) -> Self {
        self.connect_timeout = duration;
        self
    }

    /// Get connection from pool.
    pub async fn get(&self) -> Result<PooledConnection> {
        let permit = self
            .semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| TcpError::PoolExhausted)?;

        let conn = {
            let mut connections = self.connections.lock().await;
            connections.pop()
        };

        let conn = match conn {
            Some(c) => c,
            None => TcpConnection::connect_timeout(&self.addr, self.connect_timeout).await?,
        };

        Ok(PooledConnection {
            connections: Arc::clone(&self.connections),
            conn: Some(conn),
            _permit: permit,
        })
    }
}

/// Pooled connection wrapper.
pub struct PooledConnection {
    connections: Arc<Mutex<Vec<TcpConnection>>>,
    conn: Option<TcpConnection>,
    _permit: tokio::sync::OwnedSemaphorePermit,
}

impl PooledConnection {
    /// Get inner connection.
    pub fn inner(&mut self) -> &mut TcpConnection {
        self.conn.as_mut().unwrap()
    }
}

impl Drop for PooledConnection {
    fn drop(&mut self) {
        if let Some(conn) = self.conn.take() {
            let connections = Arc::clone(&self.connections);
            tokio::spawn(async move {
                let mut pool = connections.lock().await;
                pool.push(conn);
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_length_prefix_codec() {
        let codec = LengthPrefixCodec::new(1024);
        let mut buf = BytesMut::new();

        // Encode
        codec.encode(b"hello", &mut buf).unwrap();
        assert_eq!(buf.len(), 4 + 5);

        // Decode
        let frame = codec.decode(&mut buf).unwrap();
        assert_eq!(frame, Some(b"hello".to_vec()));
    }

    #[test]
    fn test_line_codec() {
        let codec = LineCodec::new(1024);
        let mut buf = BytesMut::from(&b"hello\nworld\n"[..]);

        let frame1 = codec.decode(&mut buf).unwrap();
        assert_eq!(frame1, Some(b"hello".to_vec()));

        let frame2 = codec.decode(&mut buf).unwrap();
        assert_eq!(frame2, Some(b"world".to_vec()));
    }

    #[test]
    fn test_line_codec_crlf() {
        let codec = LineCodec::new(1024);
        let mut buf = BytesMut::from(&b"hello\r\n"[..]);

        let frame = codec.decode(&mut buf).unwrap();
        assert_eq!(frame, Some(b"hello".to_vec()));
    }

    #[test]
    fn test_frame_too_large() {
        let codec = LengthPrefixCodec::new(10);

        let result = codec.encode(b"this is too long", &mut BytesMut::new());
        assert!(matches!(result, Err(TcpError::FrameTooLarge { .. })));
    }
}
