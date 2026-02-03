//! Cursor-based pagination for drbot.
//!
//! This crate provides:
//! - Cursor encoding/decoding
//! - Relay-style connections
//! - Bidirectional cursors
//! - Seek-based pagination

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::marker::PhantomData;
use thiserror::Error;
use uuid::Uuid;

/// Cursor error types.
#[derive(Error, Debug)]
pub enum CursorError {
    #[error("Invalid cursor: {0}")]
    InvalidCursor(String),

    #[error("Expired cursor")]
    ExpiredCursor,

    #[error("Encoding error: {0}")]
    EncodingError(String),

    #[error("Decoding error: {0}")]
    DecodingError(String),
}

/// Result type for cursor operations.
pub type Result<T> = std::result::Result<T, CursorError>;

/// Cursor direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Direction {
    /// Forward pagination.
    Forward,
    /// Backward pagination.
    Backward,
}

/// Opaque cursor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cursor(String);

impl Cursor {
    /// Create cursor from string.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Get cursor value.
    pub fn value(&self) -> &str {
        &self.0
    }

    /// Encode a value as cursor.
    pub fn encode<T: Serialize>(value: &T) -> Result<Self> {
        let json =
            serde_json::to_string(value).map_err(|e| CursorError::EncodingError(e.to_string()))?;

        // Base64 encode for URL safety
        use std::io::Write;
        let mut encoder = base64_encoder();
        encoder
            .write_all(json.as_bytes())
            .map_err(|e| CursorError::EncodingError(e.to_string()))?;
        let encoded = encoder.finish();

        Ok(Self(encoded))
    }

    /// Decode cursor to value.
    pub fn decode<T: for<'de> Deserialize<'de>>(&self) -> Result<T> {
        let decoded =
            base64_decode(&self.0).map_err(|e| CursorError::DecodingError(e.to_string()))?;

        let json =
            String::from_utf8(decoded).map_err(|e| CursorError::DecodingError(e.to_string()))?;

        serde_json::from_str(&json).map_err(|e| CursorError::DecodingError(e.to_string()))
    }
}

// Simple base64 encoder/decoder
fn base64_encoder() -> Base64Encoder {
    Base64Encoder { buffer: Vec::new() }
}

struct Base64Encoder {
    buffer: Vec<u8>,
}

impl std::io::Write for Base64Encoder {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.buffer.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Base64Encoder {
    fn finish(self) -> String {
        const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
        let mut result = String::new();

        for chunk in self.buffer.chunks(3) {
            let b0 = chunk[0] as usize;
            let b1 = chunk.get(1).copied().unwrap_or(0) as usize;
            let b2 = chunk.get(2).copied().unwrap_or(0) as usize;

            result.push(CHARS[b0 >> 2] as char);
            result.push(CHARS[((b0 & 0x03) << 4) | (b1 >> 4)] as char);

            if chunk.len() > 1 {
                result.push(CHARS[((b1 & 0x0f) << 2) | (b2 >> 6)] as char);
            }
            if chunk.len() > 2 {
                result.push(CHARS[b2 & 0x3f] as char);
            }
        }

        result
    }
}

fn base64_decode(input: &str) -> std::result::Result<Vec<u8>, String> {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

    let mut result = Vec::new();
    let bytes: Vec<u8> = input.bytes().collect();

    for chunk in bytes.chunks(4) {
        let mut vals = [0u8; 4];
        for (i, &byte) in chunk.iter().enumerate() {
            vals[i] = CHARS
                .iter()
                .position(|&c| c == byte)
                .ok_or_else(|| format!("Invalid character: {}", byte as char))?
                as u8;
        }

        result.push((vals[0] << 2) | (vals[1] >> 4));
        if chunk.len() > 2 {
            result.push((vals[1] << 4) | (vals[2] >> 2));
        }
        if chunk.len() > 3 {
            result.push((vals[2] << 6) | vals[3]);
        }
    }

    Ok(result)
}

impl std::fmt::Display for Cursor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Cursor-based pagination request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CursorRequest {
    /// Cursor to start from.
    pub after: Option<Cursor>,
    /// Cursor to end at.
    pub before: Option<Cursor>,
    /// Number of items (forward).
    pub first: Option<u64>,
    /// Number of items (backward).
    pub last: Option<u64>,
}

impl Default for CursorRequest {
    fn default() -> Self {
        Self {
            after: None,
            before: None,
            first: Some(20),
            last: None,
        }
    }
}

impl CursorRequest {
    /// Create forward request.
    pub fn forward(first: u64) -> Self {
        Self {
            first: Some(first),
            ..Default::default()
        }
    }

    /// Create backward request.
    pub fn backward(last: u64) -> Self {
        Self {
            first: None,
            last: Some(last),
            ..Default::default()
        }
    }

    /// Set after cursor.
    pub fn after(mut self, cursor: Cursor) -> Self {
        self.after = Some(cursor);
        self
    }

    /// Set before cursor.
    pub fn before(mut self, cursor: Cursor) -> Self {
        self.before = Some(cursor);
        self
    }

    /// Get direction.
    pub fn direction(&self) -> Direction {
        if self.last.is_some() {
            Direction::Backward
        } else {
            Direction::Forward
        }
    }

    /// Get limit.
    pub fn limit(&self) -> u64 {
        self.first.or(self.last).unwrap_or(20)
    }
}

/// Page info for connections.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageInfo {
    /// Has previous page.
    pub has_previous_page: bool,
    /// Has next page.
    pub has_next_page: bool,
    /// First cursor.
    pub start_cursor: Option<Cursor>,
    /// Last cursor.
    pub end_cursor: Option<Cursor>,
}

impl PageInfo {
    /// Create empty page info.
    pub fn empty() -> Self {
        Self {
            has_previous_page: false,
            has_next_page: false,
            start_cursor: None,
            end_cursor: None,
        }
    }
}

/// Edge in a connection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge<T> {
    /// The node.
    pub node: T,
    /// Cursor for this node.
    pub cursor: Cursor,
}

impl<T> Edge<T> {
    /// Create new edge.
    pub fn new(node: T, cursor: Cursor) -> Self {
        Self { node, cursor }
    }

    /// Map the node.
    pub fn map<U, F>(self, f: F) -> Edge<U>
    where
        F: FnOnce(T) -> U,
    {
        Edge {
            node: f(self.node),
            cursor: self.cursor,
        }
    }
}

/// Relay-style connection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Connection<T> {
    /// Edges.
    pub edges: Vec<Edge<T>>,
    /// Page info.
    pub page_info: PageInfo,
    /// Total count (optional).
    pub total_count: Option<u64>,
}

impl<T> Connection<T> {
    /// Create empty connection.
    pub fn empty() -> Self {
        Self {
            edges: Vec::new(),
            page_info: PageInfo::empty(),
            total_count: Some(0),
        }
    }

    /// Create from edges.
    pub fn from_edges(edges: Vec<Edge<T>>, has_previous: bool, has_next: bool) -> Self {
        let page_info = PageInfo {
            has_previous_page: has_previous,
            has_next_page: has_next,
            start_cursor: edges.first().map(|e| e.cursor.clone()),
            end_cursor: edges.last().map(|e| e.cursor.clone()),
        };

        Self {
            edges,
            page_info,
            total_count: None,
        }
    }

    /// Set total count.
    pub fn with_total_count(mut self, count: u64) -> Self {
        self.total_count = Some(count);
        self
    }

    /// Map edges.
    pub fn map<U, F>(self, mut f: F) -> Connection<U>
    where
        F: FnMut(T) -> U,
    {
        Connection {
            edges: self.edges.into_iter().map(|e| e.map(&mut f)).collect(),
            page_info: self.page_info,
            total_count: self.total_count,
        }
    }

    /// Get nodes.
    pub fn nodes(&self) -> Vec<&T> {
        self.edges.iter().map(|e| &e.node).collect()
    }

    /// Is empty.
    pub fn is_empty(&self) -> bool {
        self.edges.is_empty()
    }

    /// Edge count.
    pub fn len(&self) -> usize {
        self.edges.len()
    }
}

/// Cursor factory for consistent cursor generation.
pub struct CursorFactory<T> {
    _marker: PhantomData<T>,
}

impl<T: Serialize + for<'de> Deserialize<'de>> CursorFactory<T> {
    /// Create new factory.
    pub fn new() -> Self {
        Self {
            _marker: PhantomData,
        }
    }

    /// Create cursor from value.
    pub fn create(&self, value: &T) -> Result<Cursor> {
        Cursor::encode(value)
    }

    /// Parse cursor to value.
    pub fn parse(&self, cursor: &Cursor) -> Result<T> {
        cursor.decode()
    }
}

impl<T: Serialize + for<'de> Deserialize<'de>> Default for CursorFactory<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Common cursor value types.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdCursor {
    /// ID value.
    pub id: String,
}

impl IdCursor {
    /// Create new ID cursor.
    pub fn new(id: impl Into<String>) -> Self {
        Self { id: id.into() }
    }
}

/// Timestamp cursor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimestampCursor {
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
    /// Tie-breaker ID.
    pub id: Option<String>,
}

impl TimestampCursor {
    /// Create new timestamp cursor.
    pub fn new(timestamp: DateTime<Utc>) -> Self {
        Self {
            timestamp,
            id: None,
        }
    }

    /// With ID for tie-breaking.
    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }
}

/// Composite cursor for multiple sort keys.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompositeCursor {
    /// Values.
    pub values: Vec<serde_json::Value>,
}

impl CompositeCursor {
    /// Create new composite cursor.
    pub fn new() -> Self {
        Self { values: Vec::new() }
    }

    /// Add value.
    pub fn add<T: Serialize>(mut self, value: T) -> Self {
        if let Ok(v) = serde_json::to_value(value) {
            self.values.push(v);
        }
        self
    }
}

impl Default for CompositeCursor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cursor_encoding() {
        let cursor = IdCursor::new("item-123");
        let encoded = Cursor::encode(&cursor).unwrap();
        let decoded: IdCursor = encoded.decode().unwrap();
        assert_eq!(decoded.id, "item-123");
    }

    #[test]
    fn test_cursor_request() {
        let request = CursorRequest::forward(10);
        assert_eq!(request.limit(), 10);
        assert_eq!(request.direction(), Direction::Forward);
    }

    #[test]
    fn test_connection() {
        let edges = vec![
            Edge::new("a", Cursor::new("cursor1")),
            Edge::new("b", Cursor::new("cursor2")),
        ];

        let conn = Connection::from_edges(edges, false, true);

        assert_eq!(conn.len(), 2);
        assert!(!conn.page_info.has_previous_page);
        assert!(conn.page_info.has_next_page);
    }

    #[test]
    fn test_page_info() {
        let edges = vec![
            Edge::new(1, Cursor::new("a")),
            Edge::new(2, Cursor::new("b")),
        ];

        let conn = Connection::from_edges(edges, true, true);

        assert_eq!(
            conn.page_info.start_cursor.as_ref().map(|c| c.value()),
            Some("a")
        );
        assert_eq!(
            conn.page_info.end_cursor.as_ref().map(|c| c.value()),
            Some("b")
        );
    }

    #[test]
    fn test_timestamp_cursor() {
        let cursor = TimestampCursor::new(Utc::now()).with_id("tie-breaker");
        let encoded = Cursor::encode(&cursor).unwrap();
        let decoded: TimestampCursor = encoded.decode().unwrap();
        assert_eq!(decoded.id, Some("tie-breaker".to_string()));
    }

    #[test]
    fn test_composite_cursor() {
        let cursor = CompositeCursor::new().add("name").add(123);

        let encoded = Cursor::encode(&cursor).unwrap();
        let decoded: CompositeCursor = encoded.decode().unwrap();
        assert_eq!(decoded.values.len(), 2);
    }

    #[test]
    fn test_cursor_factory() {
        let factory = CursorFactory::<IdCursor>::new();
        let cursor = factory.create(&IdCursor::new("test")).unwrap();
        let parsed = factory.parse(&cursor).unwrap();
        assert_eq!(parsed.id, "test");
    }

    #[test]
    fn test_empty_connection() {
        let conn: Connection<String> = Connection::empty();
        assert!(conn.is_empty());
        assert!(!conn.page_info.has_next_page);
        assert!(!conn.page_info.has_previous_page);
    }

    #[test]
    fn test_map_connection() {
        let edges = vec![Edge::new(1, Cursor::new("a"))];
        let conn = Connection::from_edges(edges, false, false);
        let mapped = conn.map(|n| n * 2);
        assert_eq!(mapped.edges[0].node, 2);
    }
}
