//! WebSocket protocol definitions for drbot gateway.
//!
//! This crate defines the JSON-RPC style request/response protocol
//! used for communication between clients and the drbot gateway.

pub mod event;
pub mod openclaw;
pub mod request;
pub mod response;

pub use event::*;
pub use request::*;
pub use response::*;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Protocol version.
pub const PROTOCOL_VERSION: &str = "1.0";

/// A WebSocket message (can be request, response, or event).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WsMessage {
    /// Client request.
    Request(Request),
    /// Server response.
    Response(Response),
    /// Server event (push notification).
    Event(Event),
}

impl WsMessage {
    /// Create a request message.
    pub fn request(id: Uuid, method: impl Into<String>, params: impl Serialize) -> Self {
        WsMessage::Request(Request::new(id, method, params))
    }

    /// Create a success response.
    pub fn success(id: Uuid, result: impl Serialize) -> Self {
        WsMessage::Response(Response::success(id, result))
    }

    /// Create an error response.
    pub fn error(id: Uuid, code: ErrorCode, message: impl Into<String>) -> Self {
        WsMessage::Response(Response::error(id, code, message))
    }

    /// Create an event message.
    pub fn event(event_type: impl Into<String>, data: impl Serialize) -> Self {
        WsMessage::Event(Event::new(event_type, data))
    }

    /// Parse a WebSocket message from JSON.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Serialize the message to JSON.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }
}

/// Common metadata included in messages.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MessageMeta {
    /// Protocol version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Timestamp in milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_serialization() {
        let msg = WsMessage::request(
            Uuid::new_v4(),
            "chat.send",
            serde_json::json!({ "message": "Hello" }),
        );
        let json = msg.to_json().unwrap();
        let parsed = WsMessage::from_json(&json).unwrap();

        match parsed {
            WsMessage::Request(req) => {
                assert_eq!(req.method, "chat.send");
            }
            _ => panic!("Expected Request"),
        }
    }

    #[test]
    fn test_response_serialization() {
        let id = Uuid::new_v4();
        let msg = WsMessage::success(id, serde_json::json!({ "status": "ok" }));
        let json = msg.to_json().unwrap();
        let parsed = WsMessage::from_json(&json).unwrap();

        match parsed {
            WsMessage::Response(resp) => {
                assert_eq!(resp.id, id);
                assert!(resp.error.is_none());
            }
            _ => panic!("Expected Response"),
        }
    }
}
