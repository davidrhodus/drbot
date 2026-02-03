//! Bridge protocol for communicating with the Baileys Node.js process.

use serde::{Deserialize, Serialize};

/// Message from Rust to Bridge.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum BridgeRequest {
    /// Initialize connection.
    #[serde(rename = "init")]
    Init {
        /// Session data directory.
        session_dir: String,
    },

    /// Send a text message.
    #[serde(rename = "send_message")]
    SendMessage {
        /// Message ID for tracking.
        id: String,
        /// Recipient JID (phone@s.whatsapp.net).
        to: String,
        /// Message text.
        text: String,
    },

    /// Send a media message.
    #[serde(rename = "send_media")]
    SendMedia {
        /// Message ID for tracking.
        id: String,
        /// Recipient JID.
        to: String,
        /// Media type (image, video, audio, document).
        media_type: String,
        /// Base64 encoded media data or URL.
        data: String,
        /// Optional caption.
        caption: Option<String>,
        /// Filename for documents.
        filename: Option<String>,
    },

    /// Get QR code for authentication.
    #[serde(rename = "get_qr")]
    GetQr,

    /// Check connection status.
    #[serde(rename = "status")]
    Status,

    /// Disconnect and cleanup.
    #[serde(rename = "disconnect")]
    Disconnect,
}

/// Message from Bridge to Rust.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum BridgeEvent {
    /// Connection status changed.
    #[serde(rename = "connection")]
    Connection {
        /// Connection state.
        status: ConnectionStatus,
    },

    /// QR code for authentication.
    #[serde(rename = "qr")]
    Qr {
        /// QR code string.
        qr: String,
    },

    /// Message received.
    #[serde(rename = "message")]
    Message {
        /// Message data.
        #[serde(flatten)]
        message: WhatsAppMessage,
    },

    /// Message sent confirmation.
    #[serde(rename = "sent")]
    Sent {
        /// Request ID that was sent.
        id: String,
        /// WhatsApp message ID.
        message_id: String,
    },

    /// Error occurred.
    #[serde(rename = "error")]
    Error {
        /// Error message.
        error: String,
        /// Related request ID if applicable.
        id: Option<String>,
    },

    /// Ready to receive/send messages.
    #[serde(rename = "ready")]
    Ready,
}

/// Connection status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConnectionStatus {
    /// Connecting.
    Connecting,
    /// Open and authenticated.
    Open,
    /// Closed.
    Close,
    /// Logged out.
    LoggedOut,
}

/// WhatsApp message from the bridge.
#[derive(Debug, Clone, Deserialize)]
pub struct WhatsAppMessage {
    /// Message ID.
    pub id: String,
    /// Chat JID.
    pub chat: String,
    /// Sender JID.
    pub sender: String,
    /// Sender name (push name).
    pub sender_name: Option<String>,
    /// Message timestamp (Unix seconds).
    pub timestamp: i64,
    /// Text content.
    pub text: Option<String>,
    /// Whether the message is from the user.
    pub from_me: bool,
    /// Media type if present.
    pub media_type: Option<String>,
    /// Media URL if present.
    pub media_url: Option<String>,
    /// Quoted message ID.
    pub quoted_id: Option<String>,
}

impl WhatsAppMessage {
    /// Check if this is a group message.
    pub fn is_group(&self) -> bool {
        self.chat.contains("@g.us")
    }

    /// Get the phone number from a JID.
    pub fn phone_from_jid(jid: &str) -> Option<String> {
        jid.split('@').next().map(|s| s.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bridge_request_serialize() {
        let req = BridgeRequest::SendMessage {
            id: "123".to_string(),
            to: "1234567890@s.whatsapp.net".to_string(),
            text: "Hello".to_string(),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("send_message"));
        assert!(json.contains("Hello"));
    }

    #[test]
    fn test_bridge_event_deserialize() {
        let json = r#"{"type": "connection", "status": "open"}"#;
        let event: BridgeEvent = serde_json::from_str(json).unwrap();
        match event {
            BridgeEvent::Connection { status } => {
                assert_eq!(status, ConnectionStatus::Open);
            }
            _ => panic!("Wrong event type"),
        }
    }

    #[test]
    fn test_whatsapp_message_deserialize() {
        let json = r#"{
            "id": "MSG123",
            "chat": "1234567890@s.whatsapp.net",
            "sender": "1234567890@s.whatsapp.net",
            "sender_name": "John",
            "timestamp": 1700000000,
            "text": "Hello",
            "from_me": false
        }"#;
        let msg: WhatsAppMessage = serde_json::from_str(json).unwrap();
        assert_eq!(msg.id, "MSG123");
        assert_eq!(msg.text, Some("Hello".to_string()));
        assert!(!msg.is_group());
    }

    #[test]
    fn test_is_group() {
        let msg = WhatsAppMessage {
            id: "1".to_string(),
            chat: "12345678-1234567@g.us".to_string(),
            sender: "1234567890@s.whatsapp.net".to_string(),
            sender_name: None,
            timestamp: 0,
            text: None,
            from_me: false,
            media_type: None,
            media_url: None,
            quoted_id: None,
        };
        assert!(msg.is_group());
    }

    #[test]
    fn test_phone_from_jid() {
        assert_eq!(
            WhatsAppMessage::phone_from_jid("1234567890@s.whatsapp.net"),
            Some("1234567890".to_string())
        );
    }
}
