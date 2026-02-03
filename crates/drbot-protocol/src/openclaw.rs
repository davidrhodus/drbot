//! OpenClaw Gateway protocol (v3) compatibility types.
//!
//! drbot maintains its own legacy gateway protocol, but can also expose an
//! OpenClaw-compatible WebSocket endpoint for interoperability with OpenClaw
//! clients (Control UI, CLI, nodes).

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// OpenClaw Gateway protocol version (integer).
pub const OPENCLAW_PROTOCOL_VERSION: u32 = 3;

// ---------------------------------------------------------------------------
// Frames
// ---------------------------------------------------------------------------

/// Top-level gateway frame (request/response/event).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum GatewayFrame {
    /// Client request.
    #[serde(rename = "req")]
    Req(RequestFrame),
    /// Server response.
    #[serde(rename = "res")]
    Res(ResponseFrame),
    /// Server event.
    #[serde(rename = "event")]
    Event(EventFrame),
}

/// Request frame: `{ type: "req", id, method, params? }`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestFrame {
    pub id: String,
    pub method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

/// Response frame: `{ type: "res", id, ok, payload?, error? }`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseFrame {
    pub id: String,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorShape>,
}

/// Event frame: `{ type: "event", event, payload?, seq?, stateVersion? }`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventFrame {
    pub event: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seq: Option<u64>,
    #[serde(
        default,
        rename = "stateVersion",
        skip_serializing_if = "Option::is_none"
    )]
    pub state_version: Option<StateVersion>,
}

// ---------------------------------------------------------------------------
// Error shape + codes
// ---------------------------------------------------------------------------

/// OpenClaw error codes.
///
/// Keep as string constants (instead of a strict enum) to stay forward-compatible.
pub mod error_codes {
    pub const NOT_LINKED: &str = "NOT_LINKED";
    pub const NOT_PAIRED: &str = "NOT_PAIRED";
    pub const AGENT_TIMEOUT: &str = "AGENT_TIMEOUT";
    pub const INVALID_REQUEST: &str = "INVALID_REQUEST";
    pub const UNAVAILABLE: &str = "UNAVAILABLE";
}

/// OpenClaw error shape.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorShape {
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retryable: Option<bool>,
    #[serde(
        default,
        rename = "retryAfterMs",
        skip_serializing_if = "Option::is_none"
    )]
    pub retry_after_ms: Option<u64>,
}

impl ErrorShape {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            details: None,
            retryable: None,
            retry_after_ms: None,
        }
    }

    pub fn with_details(mut self, details: Value) -> Self {
        self.details = Some(details);
        self
    }
}

// ---------------------------------------------------------------------------
// Connect + hello-ok
// ---------------------------------------------------------------------------

/// State version tuple carried on some events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateVersion {
    pub presence: u64,
    pub health: u64,
}

/// Presence entry (schema-light).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresenceEntry {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ip: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
    #[serde(
        default,
        rename = "deviceFamily",
        skip_serializing_if = "Option::is_none"
    )]
    pub device_family: Option<String>,
    #[serde(
        default,
        rename = "modelIdentifier",
        skip_serializing_if = "Option::is_none"
    )]
    pub model_identifier: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    #[serde(
        default,
        rename = "lastInputSeconds",
        skip_serializing_if = "Option::is_none"
    )]
    pub last_input_seconds: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    pub ts: u64,
    #[serde(default, rename = "deviceId", skip_serializing_if = "Option::is_none")]
    pub device_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub roles: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scopes: Option<Vec<String>>,
    #[serde(
        default,
        rename = "instanceId",
        skip_serializing_if = "Option::is_none"
    )]
    pub instance_id: Option<String>,
}

/// Gateway snapshot sent in hello-ok.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub presence: Vec<PresenceEntry>,
    pub health: Value,
    #[serde(rename = "stateVersion")]
    pub state_version: StateVersion,
    #[serde(rename = "uptimeMs")]
    pub uptime_ms: u64,
    #[serde(
        default,
        rename = "configPath",
        skip_serializing_if = "Option::is_none"
    )]
    pub config_path: Option<String>,
    #[serde(default, rename = "stateDir", skip_serializing_if = "Option::is_none")]
    pub state_dir: Option<String>,
    #[serde(
        default,
        rename = "sessionDefaults",
        skip_serializing_if = "Option::is_none"
    )]
    pub session_defaults: Option<Value>,
}

/// `connect` params (schema-light; drbot validates only what it needs).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectParams {
    #[serde(rename = "minProtocol")]
    pub min_protocol: u32,
    #[serde(rename = "maxProtocol")]
    pub max_protocol: u32,
    pub client: ConnectClient,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caps: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commands: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permissions: Option<std::collections::HashMap<String, bool>>,
    #[serde(default, rename = "pathEnv", skip_serializing_if = "Option::is_none")]
    pub path_env: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scopes: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<ConnectAuth>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locale: Option<String>,
    #[serde(default, rename = "userAgent", skip_serializing_if = "Option::is_none")]
    pub user_agent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectClient {
    pub id: String,
    #[serde(
        default,
        rename = "displayName",
        skip_serializing_if = "Option::is_none"
    )]
    pub display_name: Option<String>,
    pub version: String,
    pub platform: String,
    #[serde(
        default,
        rename = "deviceFamily",
        skip_serializing_if = "Option::is_none"
    )]
    pub device_family: Option<String>,
    #[serde(
        default,
        rename = "modelIdentifier",
        skip_serializing_if = "Option::is_none"
    )]
    pub model_identifier: Option<String>,
    pub mode: String,
    #[serde(
        default,
        rename = "instanceId",
        skip_serializing_if = "Option::is_none"
    )]
    pub instance_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectAuth {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
}

/// `hello-ok` payload (nested inside `connect` response payload).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HelloOk {
    #[serde(rename = "type")]
    pub kind: String, // always "hello-ok"
    pub protocol: u32,
    pub server: HelloServer,
    pub features: HelloFeatures,
    pub snapshot: Snapshot,
    #[serde(
        default,
        rename = "canvasHostUrl",
        skip_serializing_if = "Option::is_none"
    )]
    pub canvas_host_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<Value>,
    pub policy: HelloPolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HelloServer {
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(rename = "connId")]
    pub conn_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HelloFeatures {
    pub methods: Vec<String>,
    pub events: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HelloPolicy {
    #[serde(rename = "maxPayload")]
    pub max_payload: u64,
    #[serde(rename = "maxBufferedBytes")]
    pub max_buffered_bytes: u64,
    #[serde(rename = "tickIntervalMs")]
    pub tick_interval_ms: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_request_frame() {
        let frame = GatewayFrame::Req(RequestFrame {
            id: "1".to_string(),
            method: "health".to_string(),
            params: Some(serde_json::json!({})),
        });
        let json = serde_json::to_string(&frame).unwrap();
        let parsed: GatewayFrame = serde_json::from_str(&json).unwrap();
        match parsed {
            GatewayFrame::Req(req) => {
                assert_eq!(req.method, "health");
            }
            _ => panic!("expected req frame"),
        }
    }
}
