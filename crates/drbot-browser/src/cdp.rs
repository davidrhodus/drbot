//! Chrome DevTools Protocol client.

use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, RwLock};
use tokio_tungstenite::{connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream};
use tracing::{debug, warn};

/// CDP method call.
#[derive(Debug, Clone, Serialize)]
pub struct CdpRequest {
    /// Request ID.
    pub id: u64,
    /// Method name.
    pub method: String,
    /// Optional session ID (when attached to a target with `flatten: true`).
    #[serde(rename = "sessionId", skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// Parameters.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

/// CDP response.
#[derive(Debug, Clone, Deserialize)]
pub struct CdpResponse {
    /// Request ID.
    pub id: u64,
    /// Result (on success).
    pub result: Option<serde_json::Value>,
    /// Error (on failure).
    pub error: Option<CdpError>,
}

/// CDP error.
#[derive(Debug, Clone, Deserialize)]
pub struct CdpError {
    /// Error code.
    pub code: i64,
    /// Error message.
    pub message: String,
    /// Additional data.
    #[allow(dead_code)] // Keep for protocol completeness; not always inspected.
    pub data: Option<serde_json::Value>,
}

/// CDP event.
#[derive(Debug, Clone, Deserialize)]
pub struct CdpEvent {
    /// Event method.
    pub method: String,
    /// Optional session ID (when attached to a target with `flatten: true`).
    #[serde(rename = "sessionId", default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// Event parameters.
    pub params: Option<serde_json::Value>,
}

/// Incoming CDP message (response or event).
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum CdpMessage {
    /// Response to a request.
    Response(CdpResponse),
    /// Async event.
    Event(CdpEvent),
}

/// CDP connection state.
pub struct CdpConnection {
    /// Request ID counter.
    next_id: AtomicU64,
    /// Sender for outgoing messages.
    tx: mpsc::Sender<String>,
    /// Pending requests waiting for responses.
    pending: Arc<RwLock<HashMap<u64, oneshot::Sender<CdpResponse>>>>,
}

type WsStream = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

impl CdpConnection {
    /// Connect to a CDP endpoint.
    pub async fn connect(ws_url: &str) -> drbot_core::Result<(Self, mpsc::Receiver<CdpEvent>)> {
        let (ws_stream, _) = connect_async(ws_url)
            .await
            .map_err(|e| drbot_core::Error::WebSocket(format!("connect failed: {}", e)))?;

        let ws_stream: WsStream = ws_stream;
        let (mut write, mut read) = ws_stream.split();
        let (tx, mut rx) = mpsc::channel::<String>(64);
        let (event_tx, event_rx) = mpsc::channel::<CdpEvent>(256);
        let pending: Arc<RwLock<HashMap<u64, oneshot::Sender<CdpResponse>>>> =
            Arc::new(RwLock::new(HashMap::new()));
        let pending_clone = pending.clone();

        // Writer task
        tokio::spawn(async move {
            while let Some(msg) = rx.recv().await {
                if write.send(Message::Text(msg.into())).await.is_err() {
                    break;
                }
            }
        });

        // Reader task
        let event_tx_clone = event_tx.clone();
        tokio::spawn(async move {
            while let Some(Ok(msg)) = read.next().await {
                if let Message::Text(text) = msg {
                    let text_str: &str = text.as_ref();
                    match serde_json::from_str::<CdpMessage>(text_str) {
                        Ok(CdpMessage::Response(resp)) => {
                            let mut pending = pending_clone.write().await;
                            if let Some(tx) = pending.remove(&resp.id) {
                                let _ = tx.send(resp);
                            }
                        }
                        Ok(CdpMessage::Event(event)) => {
                            if event_tx_clone.send(event).await.is_err() {
                                break;
                            }
                        }
                        Err(e) => {
                            warn!("Failed to parse CDP message: {}", e);
                        }
                    }
                }
            }
        });

        Ok((
            Self {
                next_id: AtomicU64::new(1),
                tx,
                pending,
            },
            event_rx,
        ))
    }

    /// Send a CDP command and wait for response.
    pub async fn send(
        &self,
        method: impl Into<String>,
        params: Option<serde_json::Value>,
    ) -> drbot_core::Result<serde_json::Value> {
        self.send_with_session(method, params, None).await
    }

    /// Send a CDP command scoped to a specific target session.
    pub async fn send_with_session(
        &self,
        method: impl Into<String>,
        params: Option<serde_json::Value>,
        session_id: Option<&str>,
    ) -> drbot_core::Result<serde_json::Value> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let request = CdpRequest {
            id,
            method: method.into(),
            session_id: session_id.map(|s| s.to_string()),
            params,
        };

        let (resp_tx, resp_rx) = oneshot::channel();
        self.pending.write().await.insert(id, resp_tx);

        let json = serde_json::to_string(&request)
            .map_err(|e| drbot_core::Error::Internal(format!("JSON serialize failed: {}", e)))?;

        debug!("CDP request: {}", json);

        self.tx
            .send(json)
            .await
            .map_err(|_| drbot_core::Error::WebSocket("CDP connection closed".to_string()))?;

        let response = resp_rx
            .await
            .map_err(|_| drbot_core::Error::WebSocket("CDP response channel closed".to_string()))?;

        if let Some(error) = response.error {
            return Err(drbot_core::Error::Internal(format!(
                "CDP error {}: {}",
                error.code, error.message
            )));
        }

        Ok(response.result.unwrap_or(serde_json::Value::Null))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cdp_request_serialize() {
        let req = CdpRequest {
            id: 1,
            method: "Page.navigate".to_string(),
            session_id: Some("session123".to_string()),
            params: Some(serde_json::json!({"url": "https://example.com"})),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("Page.navigate"));
        assert!(json.contains("example.com"));
        assert!(json.contains("sessionId"));
    }

    #[test]
    fn test_cdp_response_deserialize() {
        let json = r#"{"id": 1, "result": {"frameId": "ABC123"}}"#;
        let resp: CdpResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.id, 1);
        assert!(resp.result.is_some());
    }

    #[test]
    fn test_cdp_event_deserialize() {
        let json = r#"{"method": "Page.loadEventFired", "params": {"timestamp": 12345.0}}"#;
        let event: CdpEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.method, "Page.loadEventFired");
    }

    #[test]
    fn test_cdp_event_with_session_deserialize() {
        let json = r#"{"sessionId":"session123","method":"Runtime.consoleAPICalled","params":{"type":"log"}}"#;
        let event: CdpEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.session_id.as_deref(), Some("session123"));
        assert_eq!(event.method, "Runtime.consoleAPICalled");
    }
}
