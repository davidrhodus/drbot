//! Message types and utilities for drbot.
//!
//! This crate provides:
//! - Message type definitions
//! - Message builders
//! - Message serialization
//! - Common message patterns

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

/// Message error types.
#[derive(Error, Debug)]
pub enum MessageError {
    #[error("Invalid message: {0}")]
    Invalid(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Missing required field: {0}")]
    MissingField(String),
}

/// Result type for message operations.
pub type Result<T> = std::result::Result<T, MessageError>;

/// Message ID.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MessageId(pub String);

impl MessageId {
    /// Generate new message ID.
    pub fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        use std::time::{SystemTime, UNIX_EPOCH};

        static COUNTER: AtomicU64 = AtomicU64::new(0);

        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let seq = COUNTER.fetch_add(1, Ordering::SeqCst);

        Self(format!("{:x}-{:x}", ts, seq))
    }

    /// Create from string.
    pub fn from_str(s: impl Into<String>) -> Self {
        Self(s.into())
    }
}

impl Default for MessageId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for MessageId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Message priority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Priority {
    Low = 0,
    Normal = 1,
    High = 2,
    Urgent = 3,
}

impl Default for Priority {
    fn default() -> Self {
        Priority::Normal
    }
}

/// Message header.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageHeader {
    /// Message ID.
    pub id: MessageId,
    /// Message type.
    pub message_type: String,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
    /// Priority.
    pub priority: Priority,
    /// Correlation ID.
    pub correlation_id: Option<MessageId>,
    /// Reply-to address.
    pub reply_to: Option<String>,
    /// Time-to-live in seconds.
    pub ttl: Option<u64>,
    /// Custom headers.
    pub headers: HashMap<String, String>,
}

impl MessageHeader {
    /// Create new header.
    pub fn new(message_type: impl Into<String>) -> Self {
        Self {
            id: MessageId::new(),
            message_type: message_type.into(),
            timestamp: Utc::now(),
            priority: Priority::Normal,
            correlation_id: None,
            reply_to: None,
            ttl: None,
            headers: HashMap::new(),
        }
    }

    /// Set priority.
    pub fn with_priority(mut self, priority: Priority) -> Self {
        self.priority = priority;
        self
    }

    /// Set correlation ID.
    pub fn with_correlation(mut self, id: MessageId) -> Self {
        self.correlation_id = Some(id);
        self
    }

    /// Set reply-to address.
    pub fn with_reply_to(mut self, address: impl Into<String>) -> Self {
        self.reply_to = Some(address.into());
        self
    }

    /// Set TTL.
    pub fn with_ttl(mut self, ttl: u64) -> Self {
        self.ttl = Some(ttl);
        self
    }

    /// Add custom header.
    pub fn with_header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(key.into(), value.into());
        self
    }

    /// Check if expired.
    pub fn is_expired(&self) -> bool {
        if let Some(ttl) = self.ttl {
            let expiry = self.timestamp + chrono::Duration::seconds(ttl as i64);
            Utc::now() > expiry
        } else {
            false
        }
    }
}

/// Generic message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message<T> {
    /// Header.
    pub header: MessageHeader,
    /// Body.
    pub body: T,
}

impl<T> Message<T> {
    /// Create new message.
    pub fn new(message_type: impl Into<String>, body: T) -> Self {
        Self {
            header: MessageHeader::new(message_type),
            body,
        }
    }

    /// Get message ID.
    pub fn id(&self) -> &MessageId {
        &self.header.id
    }

    /// Get message type.
    pub fn message_type(&self) -> &str {
        &self.header.message_type
    }

    /// Get timestamp.
    pub fn timestamp(&self) -> DateTime<Utc> {
        self.header.timestamp
    }

    /// Set header.
    pub fn with_header(mut self, header: MessageHeader) -> Self {
        self.header = header;
        self
    }

    /// Map body to different type.
    pub fn map<U, F>(self, f: F) -> Message<U>
    where
        F: FnOnce(T) -> U,
    {
        Message {
            header: self.header,
            body: f(self.body),
        }
    }
}

/// Text message body.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextBody {
    /// Text content.
    pub content: String,
    /// Content type (e.g., "text/plain", "text/markdown").
    pub content_type: String,
}

impl TextBody {
    /// Create plain text body.
    pub fn plain(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            content_type: "text/plain".to_string(),
        }
    }

    /// Create markdown body.
    pub fn markdown(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            content_type: "text/markdown".to_string(),
        }
    }
}

/// JSON message body.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonBody {
    /// JSON data.
    pub data: serde_json::Value,
}

impl JsonBody {
    /// Create from value.
    pub fn new(data: impl Serialize) -> Result<Self> {
        let data =
            serde_json::to_value(data).map_err(|e| MessageError::Serialization(e.to_string()))?;
        Ok(Self { data })
    }

    /// Create from raw JSON value.
    pub fn from_value(data: serde_json::Value) -> Self {
        Self { data }
    }

    /// Get field.
    pub fn get(&self, key: &str) -> Option<&serde_json::Value> {
        self.data.get(key)
    }
}

/// Binary message body.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BinaryBody {
    /// Binary data (base64 encoded in JSON).
    #[serde(with = "base64_serde")]
    pub data: Vec<u8>,
    /// Content type.
    pub content_type: String,
}

mod base64_serde {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(data: &[u8], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&base64_encode(data))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        base64_decode(&s).map_err(serde::de::Error::custom)
    }

    fn base64_encode(data: &[u8]) -> String {
        const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut result = String::new();

        for chunk in data.chunks(3) {
            let b0 = chunk[0] as usize;
            let b1 = chunk.get(1).copied().unwrap_or(0) as usize;
            let b2 = chunk.get(2).copied().unwrap_or(0) as usize;

            result.push(CHARS[(b0 >> 2)] as char);
            result.push(CHARS[((b0 & 0x03) << 4) | (b1 >> 4)] as char);

            if chunk.len() > 1 {
                result.push(CHARS[((b1 & 0x0f) << 2) | (b2 >> 6)] as char);
            } else {
                result.push('=');
            }

            if chunk.len() > 2 {
                result.push(CHARS[b2 & 0x3f] as char);
            } else {
                result.push('=');
            }
        }

        result
    }

    fn base64_decode(s: &str) -> Result<Vec<u8>, String> {
        const DECODE: [i8; 128] = [
            -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1,
            -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, 62,
            -1, -1, -1, 63, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, -1, -1, -1, -1, -1, -1, -1, 0,
            1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24,
            25, -1, -1, -1, -1, -1, -1, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40,
            41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, -1, -1, -1, -1, -1,
        ];

        let bytes: Vec<u8> = s.bytes().filter(|&b| b != b'=').collect();
        let mut result = Vec::new();

        for chunk in bytes.chunks(4) {
            if chunk.len() < 2 {
                break;
            }

            let b0 = DECODE.get(chunk[0] as usize).copied().unwrap_or(-1);
            let b1 = DECODE.get(chunk[1] as usize).copied().unwrap_or(-1);
            let b2 = chunk
                .get(2)
                .and_then(|&c| DECODE.get(c as usize).copied())
                .unwrap_or(0);
            let b3 = chunk
                .get(3)
                .and_then(|&c| DECODE.get(c as usize).copied())
                .unwrap_or(0);

            if b0 < 0 || b1 < 0 {
                return Err("Invalid base64".to_string());
            }

            result.push(((b0 << 2) | (b1 >> 4)) as u8);
            if chunk.len() > 2 {
                result.push((((b1 & 0x0f) << 4) | (b2 >> 2)) as u8);
            }
            if chunk.len() > 3 {
                result.push((((b2 & 0x03) << 6) | b3) as u8);
            }
        }

        Ok(result)
    }
}

impl BinaryBody {
    /// Create new binary body.
    pub fn new(data: Vec<u8>, content_type: impl Into<String>) -> Self {
        Self {
            data,
            content_type: content_type.into(),
        }
    }
}

/// Message builder.
pub struct MessageBuilder<T> {
    header: MessageHeader,
    body: Option<T>,
}

impl<T> MessageBuilder<T> {
    /// Create new builder.
    pub fn new(message_type: impl Into<String>) -> Self {
        Self {
            header: MessageHeader::new(message_type),
            body: None,
        }
    }

    /// Set body.
    pub fn body(mut self, body: T) -> Self {
        self.body = Some(body);
        self
    }

    /// Set priority.
    pub fn priority(mut self, priority: Priority) -> Self {
        self.header = self.header.with_priority(priority);
        self
    }

    /// Set correlation ID.
    pub fn correlation(mut self, id: MessageId) -> Self {
        self.header = self.header.with_correlation(id);
        self
    }

    /// Set reply-to.
    pub fn reply_to(mut self, address: impl Into<String>) -> Self {
        self.header = self.header.with_reply_to(address);
        self
    }

    /// Set TTL.
    pub fn ttl(mut self, ttl: u64) -> Self {
        self.header = self.header.with_ttl(ttl);
        self
    }

    /// Add header.
    pub fn header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.header = self.header.with_header(key, value);
        self
    }

    /// Build message.
    pub fn build(self) -> Result<Message<T>> {
        let body = self
            .body
            .ok_or_else(|| MessageError::MissingField("body".to_string()))?;
        Ok(Message {
            header: self.header,
            body,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_id() {
        let id1 = MessageId::new();
        let id2 = MessageId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_message_creation() {
        let msg = Message::new("test.event", TextBody::plain("Hello"));
        assert_eq!(msg.message_type(), "test.event");
        assert_eq!(msg.body.content, "Hello");
    }

    #[test]
    fn test_message_builder() {
        let msg = MessageBuilder::new("test")
            .body(TextBody::plain("content"))
            .priority(Priority::High)
            .ttl(3600)
            .build()
            .unwrap();

        assert_eq!(msg.header.priority, Priority::High);
        assert_eq!(msg.header.ttl, Some(3600));
    }

    #[test]
    fn test_json_body() {
        let body = JsonBody::new(serde_json::json!({"key": "value"})).unwrap();
        assert_eq!(body.get("key").unwrap(), "value");
    }

    #[test]
    fn test_message_expiry() {
        let header = MessageHeader::new("test").with_ttl(0);
        // TTL of 0 means immediate expiry
        std::thread::sleep(std::time::Duration::from_millis(10));
        assert!(header.is_expired());
    }
}
