//! Signal CLI API types.
//!
//! These types match the signal-cli JSON RPC interface.

use serde::{Deserialize, Serialize};

/// JSON RPC request.
#[derive(Debug, Clone, Serialize)]
pub struct JsonRpcRequest {
    /// JSON RPC version.
    pub jsonrpc: String,
    /// Method name.
    pub method: String,
    /// Parameters.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
    /// Request ID.
    pub id: u64,
}

impl JsonRpcRequest {
    /// Create a new JSON RPC request.
    pub fn new(method: impl Into<String>, id: u64) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            method: method.into(),
            params: None,
            id,
        }
    }

    /// Add parameters.
    pub fn with_params(mut self, params: impl Serialize) -> Self {
        self.params = serde_json::to_value(params).ok();
        self
    }
}

/// JSON RPC response.
#[derive(Debug, Clone, Deserialize)]
pub struct JsonRpcResponse {
    /// JSON RPC version.
    pub jsonrpc: String,
    /// Result (if successful).
    pub result: Option<serde_json::Value>,
    /// Error (if failed).
    pub error: Option<JsonRpcError>,
    /// Request ID.
    pub id: Option<u64>,
}

/// JSON RPC error.
#[derive(Debug, Clone, Deserialize)]
pub struct JsonRpcError {
    /// Error code.
    pub code: i32,
    /// Error message.
    pub message: String,
    /// Additional data.
    pub data: Option<serde_json::Value>,
}

/// Signal message envelope from receive.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalEnvelope {
    /// Source phone number or UUID.
    pub source: Option<String>,
    /// Source device ID.
    #[serde(rename = "sourceDevice")]
    pub source_device: Option<i32>,
    /// Timestamp.
    pub timestamp: Option<i64>,
    /// Data message.
    #[serde(rename = "dataMessage")]
    pub data_message: Option<DataMessage>,
    /// Sync message.
    #[serde(rename = "syncMessage")]
    pub sync_message: Option<SyncMessage>,
    /// Receipt message.
    #[serde(rename = "receiptMessage")]
    pub receipt_message: Option<ReceiptMessage>,
    /// Typing message.
    #[serde(rename = "typingMessage")]
    pub typing_message: Option<TypingMessage>,
}

/// Data message content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataMessage {
    /// Timestamp.
    pub timestamp: Option<i64>,
    /// Message body.
    pub message: Option<String>,
    /// Group info.
    #[serde(rename = "groupInfo")]
    pub group_info: Option<GroupInfo>,
    /// Attachments.
    pub attachments: Option<Vec<Attachment>>,
    /// Quote (reply).
    pub quote: Option<Quote>,
    /// Mentions.
    pub mentions: Option<Vec<Mention>>,
}

/// Sync message (for multi-device).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncMessage {
    /// Sent message.
    #[serde(rename = "sentMessage")]
    pub sent_message: Option<SentMessage>,
}

/// Sent message in sync.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SentMessage {
    /// Destination.
    pub destination: Option<String>,
    /// Timestamp.
    pub timestamp: Option<i64>,
    /// Message body.
    pub message: Option<String>,
    /// Group info.
    #[serde(rename = "groupInfo")]
    pub group_info: Option<GroupInfo>,
}

/// Receipt message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReceiptMessage {
    /// Receipt type.
    #[serde(rename = "type")]
    pub receipt_type: Option<String>,
    /// Timestamps.
    pub timestamps: Option<Vec<i64>>,
}

/// Typing message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypingMessage {
    /// Action (STARTED or STOPPED).
    pub action: Option<String>,
    /// Timestamp.
    pub timestamp: Option<i64>,
    /// Group ID.
    #[serde(rename = "groupId")]
    pub group_id: Option<String>,
}

/// Group info.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupInfo {
    /// Group ID.
    #[serde(rename = "groupId")]
    pub group_id: String,
    /// Group type.
    #[serde(rename = "type")]
    pub group_type: Option<String>,
}

/// Attachment info.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attachment {
    /// Content type.
    #[serde(rename = "contentType")]
    pub content_type: Option<String>,
    /// Filename.
    pub filename: Option<String>,
    /// Local ID.
    pub id: Option<String>,
    /// Size in bytes.
    pub size: Option<u64>,
}

/// Quote (reply).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Quote {
    /// Quote ID.
    pub id: Option<i64>,
    /// Author.
    pub author: Option<String>,
    /// Text.
    pub text: Option<String>,
}

/// Mention.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mention {
    /// Start position.
    pub start: Option<u32>,
    /// Length.
    pub length: Option<u32>,
    /// UUID.
    pub uuid: Option<String>,
}

/// Send message parameters.
#[derive(Debug, Clone, Serialize)]
pub struct SendMessageParams {
    /// Recipient (phone number or UUID).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recipient: Option<Vec<String>>,
    /// Group ID.
    #[serde(rename = "groupId", skip_serializing_if = "Option::is_none")]
    pub group_id: Option<String>,
    /// Message text.
    pub message: String,
    /// Quote timestamp (for replies).
    #[serde(rename = "quoteTimestamp", skip_serializing_if = "Option::is_none")]
    pub quote_timestamp: Option<i64>,
    /// Quote author (for replies).
    #[serde(rename = "quoteAuthor", skip_serializing_if = "Option::is_none")]
    pub quote_author: Option<String>,
}

/// Receive messages parameters.
#[derive(Debug, Clone, Serialize)]
pub struct ReceiveParams {
    /// Timeout in seconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<i32>,
}

/// Account info.
#[derive(Debug, Clone, Deserialize)]
pub struct AccountInfo {
    /// Phone number.
    pub number: Option<String>,
    /// UUID.
    pub uuid: Option<String>,
    /// Device ID.
    pub device: Option<i32>,
}
