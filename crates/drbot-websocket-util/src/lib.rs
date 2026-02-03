//! WebSocket utilities for drbot.
//!
//! This crate provides:
//! - WebSocket message types
//! - Frame parsing utilities
//! - Connection state management

use thiserror::Error;

/// WebSocket error types.
#[derive(Error, Debug, Clone)]
pub enum WebSocketError {
    #[error("Connection closed")]
    ConnectionClosed,

    #[error("Invalid frame: {0}")]
    InvalidFrame(String),

    #[error("Protocol error: {0}")]
    ProtocolError(String),

    #[error("Message too large: {size} bytes (max: {max})")]
    MessageTooLarge { size: usize, max: usize },
}

/// Result type for WebSocket operations.
pub type Result<T> = std::result::Result<T, WebSocketError>;

/// WebSocket opcode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Opcode {
    Continuation,
    Text,
    Binary,
    Close,
    Ping,
    Pong,
}

impl Opcode {
    /// Create from byte value.
    pub fn from_byte(byte: u8) -> Option<Self> {
        match byte & 0x0F {
            0x0 => Some(Self::Continuation),
            0x1 => Some(Self::Text),
            0x2 => Some(Self::Binary),
            0x8 => Some(Self::Close),
            0x9 => Some(Self::Ping),
            0xA => Some(Self::Pong),
            _ => None,
        }
    }

    /// Convert to byte value.
    pub fn to_byte(self) -> u8 {
        match self {
            Self::Continuation => 0x0,
            Self::Text => 0x1,
            Self::Binary => 0x2,
            Self::Close => 0x8,
            Self::Ping => 0x9,
            Self::Pong => 0xA,
        }
    }

    /// Check if this is a control frame.
    pub fn is_control(self) -> bool {
        matches!(self, Self::Close | Self::Ping | Self::Pong)
    }
}

/// WebSocket message.
#[derive(Debug, Clone)]
pub enum Message {
    Text(String),
    Binary(Vec<u8>),
    Ping(Vec<u8>),
    Pong(Vec<u8>),
    Close(Option<CloseFrame>),
}

impl Message {
    /// Create a text message.
    pub fn text<S: Into<String>>(s: S) -> Self {
        Self::Text(s.into())
    }

    /// Create a binary message.
    pub fn binary<D: Into<Vec<u8>>>(data: D) -> Self {
        Self::Binary(data.into())
    }

    /// Create a ping message.
    pub fn ping<D: Into<Vec<u8>>>(data: D) -> Self {
        Self::Ping(data.into())
    }

    /// Create a pong message.
    pub fn pong<D: Into<Vec<u8>>>(data: D) -> Self {
        Self::Pong(data.into())
    }

    /// Create a close message.
    pub fn close(code: Option<u16>, reason: Option<String>) -> Self {
        Self::Close(code.map(|c| CloseFrame {
            code: c,
            reason: reason.unwrap_or_default(),
        }))
    }

    /// Check if message is text.
    pub fn is_text(&self) -> bool {
        matches!(self, Self::Text(_))
    }

    /// Check if message is binary.
    pub fn is_binary(&self) -> bool {
        matches!(self, Self::Binary(_))
    }

    /// Check if message is close.
    pub fn is_close(&self) -> bool {
        matches!(self, Self::Close(_))
    }

    /// Get message length.
    pub fn len(&self) -> usize {
        match self {
            Self::Text(s) => s.len(),
            Self::Binary(d) | Self::Ping(d) | Self::Pong(d) => d.len(),
            Self::Close(Some(f)) => 2 + f.reason.len(),
            Self::Close(None) => 0,
        }
    }

    /// Check if message is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Convert to bytes.
    pub fn into_data(self) -> Vec<u8> {
        match self {
            Self::Text(s) => s.into_bytes(),
            Self::Binary(d) | Self::Ping(d) | Self::Pong(d) => d,
            Self::Close(Some(f)) => {
                let mut data = f.code.to_be_bytes().to_vec();
                data.extend(f.reason.into_bytes());
                data
            }
            Self::Close(None) => Vec::new(),
        }
    }
}

/// Close frame data.
#[derive(Debug, Clone)]
pub struct CloseFrame {
    pub code: u16,
    pub reason: String,
}

impl CloseFrame {
    /// Normal closure.
    pub const NORMAL: u16 = 1000;
    /// Going away.
    pub const GOING_AWAY: u16 = 1001;
    /// Protocol error.
    pub const PROTOCOL_ERROR: u16 = 1002;
    /// Unsupported data.
    pub const UNSUPPORTED: u16 = 1003;
    /// Invalid payload.
    pub const INVALID_PAYLOAD: u16 = 1007;
    /// Policy violation.
    pub const POLICY_VIOLATION: u16 = 1008;
    /// Message too big.
    pub const TOO_BIG: u16 = 1009;
    /// Internal error.
    pub const INTERNAL_ERROR: u16 = 1011;
}

/// WebSocket connection state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    Connecting,
    Open,
    Closing,
    Closed,
}

impl ConnectionState {
    /// Check if connected.
    pub fn is_connected(&self) -> bool {
        *self == Self::Open
    }

    /// Check if can send messages.
    pub fn can_send(&self) -> bool {
        matches!(self, Self::Open | Self::Closing)
    }
}

/// Frame header.
#[derive(Debug, Clone)]
pub struct FrameHeader {
    pub fin: bool,
    pub opcode: Opcode,
    pub masked: bool,
    pub payload_len: u64,
    pub mask_key: Option<[u8; 4]>,
}

impl FrameHeader {
    /// Create a new frame header.
    pub fn new(fin: bool, opcode: Opcode, payload_len: u64) -> Self {
        Self {
            fin,
            opcode,
            masked: false,
            payload_len,
            mask_key: None,
        }
    }

    /// Set mask key.
    pub fn with_mask(mut self, key: [u8; 4]) -> Self {
        self.masked = true;
        self.mask_key = Some(key);
        self
    }

    /// Calculate header size.
    pub fn header_size(&self) -> usize {
        let mut size = 2; // Basic header
        if self.payload_len > 125 {
            if self.payload_len <= 65535 {
                size += 2;
            } else {
                size += 8;
            }
        }
        if self.masked {
            size += 4;
        }
        size
    }
}

/// Apply mask to data.
pub fn mask_data(data: &mut [u8], key: [u8; 4]) {
    for (i, byte) in data.iter_mut().enumerate() {
        *byte ^= key[i % 4];
    }
}

/// Generate a random mask key.
pub fn generate_mask_key() -> [u8; 4] {
    // Simple PRNG - in production use proper random
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64;

    [
        (seed & 0xFF) as u8,
        ((seed >> 8) & 0xFF) as u8,
        ((seed >> 16) & 0xFF) as u8,
        ((seed >> 24) & 0xFF) as u8,
    ]
}

/// WebSocket URL parser.
#[derive(Debug, Clone)]
pub struct WsUrl {
    pub secure: bool,
    pub host: String,
    pub port: u16,
    pub path: String,
}

impl WsUrl {
    /// Parse a WebSocket URL.
    pub fn parse(url: &str) -> Result<Self> {
        let (secure, rest) = if url.starts_with("wss://") {
            (true, &url[6..])
        } else if url.starts_with("ws://") {
            (false, &url[5..])
        } else {
            return Err(WebSocketError::ProtocolError(
                "Invalid WebSocket URL scheme".into(),
            ));
        };

        let (host_port, path) = match rest.find('/') {
            Some(i) => (&rest[..i], &rest[i..]),
            None => (rest, "/"),
        };

        let (host, port) = match host_port.rfind(':') {
            Some(i) => {
                let port: u16 = host_port[i + 1..]
                    .parse()
                    .map_err(|_| WebSocketError::ProtocolError("Invalid port".into()))?;
                (&host_port[..i], port)
            }
            None => (host_port, if secure { 443 } else { 80 }),
        };

        Ok(Self {
            secure,
            host: host.to_string(),
            port,
            path: path.to_string(),
        })
    }

    /// Get the origin.
    pub fn origin(&self) -> String {
        let scheme = if self.secure { "https" } else { "http" };
        format!("{}://{}", scheme, self.host)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_opcode() {
        assert_eq!(Opcode::from_byte(0x01), Some(Opcode::Text));
        assert_eq!(Opcode::Text.to_byte(), 0x01);
        assert!(Opcode::Ping.is_control());
        assert!(!Opcode::Text.is_control());
    }

    #[test]
    fn test_message() {
        let msg = Message::text("hello");
        assert!(msg.is_text());
        assert_eq!(msg.len(), 5);

        let close = Message::close(Some(1000), Some("bye".into()));
        assert!(close.is_close());
    }

    #[test]
    fn test_mask_data() {
        let mut data = vec![0x01, 0x02, 0x03, 0x04, 0x05];
        let key = [0xFF, 0xFF, 0xFF, 0xFF];
        mask_data(&mut data, key);
        assert_eq!(data, vec![0xFE, 0xFD, 0xFC, 0xFB, 0xFA]);
    }

    #[test]
    fn test_ws_url_parse() {
        let url = WsUrl::parse("wss://example.com:8080/chat").unwrap();
        assert!(url.secure);
        assert_eq!(url.host, "example.com");
        assert_eq!(url.port, 8080);
        assert_eq!(url.path, "/chat");
    }

    #[test]
    fn test_connection_state() {
        assert!(ConnectionState::Open.is_connected());
        assert!(ConnectionState::Open.can_send());
        assert!(ConnectionState::Closing.can_send());
        assert!(!ConnectionState::Closed.can_send());
    }
}
