//! OpenClaw (v3) gateway WebSocket client for the TUI.

use drbot_core::{Error, Result};
use drbot_protocol::openclaw::{
    ConnectAuth, ConnectClient, ConnectParams, ErrorShape, EventFrame, GatewayFrame, HelloOk,
    RequestFrame, ResponseFrame, OPENCLAW_PROTOCOL_VERSION,
};
use futures::{SinkExt, StreamExt};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, Mutex};
use tokio_tungstenite::tungstenite::Message;

const DEFAULT_REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

#[derive(Clone)]
pub struct OpenclawClient {
    write_tx: mpsc::Sender<Message>,
    pending: Arc<Mutex<HashMap<String, oneshot::Sender<ResponseFrame>>>>,
}

impl OpenclawClient {
    pub async fn connect(
        url: &str,
        auth_token: Option<&str>,
    ) -> Result<(Self, HelloOk, mpsc::Receiver<EventFrame>)> {
        let (ws_stream, _) = tokio_tungstenite::connect_async(url)
            .await
            .map_err(|e| Error::WebSocket(e.to_string()))?;
        let (mut write, mut read) = ws_stream.split();

        let (write_tx, mut write_rx) = mpsc::channel::<Message>(64);
        let pending: Arc<Mutex<HashMap<String, oneshot::Sender<ResponseFrame>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let pending_for_read = pending.clone();

        let (event_tx, event_rx) = mpsc::channel::<EventFrame>(256);

        tokio::spawn(async move {
            while let Some(msg) = write_rx.recv().await {
                if write.send(msg).await.is_err() {
                    break;
                }
            }
        });

        tokio::spawn(async move {
            loop {
                let Some(next) = read.next().await else {
                    break;
                };
                let Ok(msg) = next else {
                    break;
                };

                match msg {
                    Message::Text(text) => {
                        let parsed: GatewayFrame = match serde_json::from_str(&text) {
                            Ok(m) => m,
                            Err(_) => continue,
                        };
                        match parsed {
                            GatewayFrame::Res(resp) => {
                                let tx = {
                                    let mut guard = pending_for_read.lock().await;
                                    guard.remove(&resp.id)
                                };
                                if let Some(tx) = tx {
                                    let _ = tx.send(resp);
                                }
                            }
                            GatewayFrame::Event(event) => {
                                let _ = event_tx.send(event).await;
                            }
                            GatewayFrame::Req(_) => {}
                        }
                    }
                    Message::Close(_) => break,
                    Message::Ping(_) | Message::Pong(_) | Message::Binary(_) => {}
                    Message::Frame(_) => {}
                }
            }

            let _ = event_tx
                .send(EventFrame {
                    event: "system.disconnected".to_string(),
                    payload: Some(serde_json::json!({ "reason": "openclaw connection closed" })),
                    seq: None,
                    state_version: None,
                })
                .await;
        });

        let client = Self { write_tx, pending };

        // Handshake: first request MUST be `connect`.
        let connect_params = ConnectParams {
            min_protocol: OPENCLAW_PROTOCOL_VERSION,
            max_protocol: OPENCLAW_PROTOCOL_VERSION,
            client: ConnectClient {
                id: format!("drbot-tui-{}", uuid::Uuid::new_v4()),
                display_name: Some("drbot TUI".to_string()),
                version: env!("CARGO_PKG_VERSION").to_string(),
                platform: std::env::consts::OS.to_string(),
                device_family: None,
                model_identifier: None,
                mode: "tui".to_string(),
                instance_id: None,
            },
            caps: None,
            commands: None,
            permissions: None,
            path_env: None,
            role: Some("operator".to_string()),
            scopes: Some(vec!["global".to_string()]),
            auth: Some(ConnectAuth {
                token: auth_token
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty()),
                password: None,
            }),
            locale: None,
            user_agent: Some(format!("drbot-tui/{}", env!("CARGO_PKG_VERSION"))),
            device: None,
        };

        let resp = client.request("connect", Some(connect_params)).await?;
        if !resp.ok {
            let err = resp.error.unwrap_or_else(|| {
                ErrorShape::new("INVALID_REQUEST", "connect failed (no error details)")
            });
            return Err(Error::Auth(format!(
                "OpenClaw connect failed: {}",
                err.message
            )));
        }

        let payload = resp.payload.unwrap_or(serde_json::Value::Null);
        let hello: HelloOk =
            serde_json::from_value(payload).map_err(|e| Error::Serialization(e))?;

        Ok((client, hello, event_rx))
    }

    pub async fn request<P: Serialize>(
        &self,
        method: &str,
        params: Option<P>,
    ) -> Result<ResponseFrame> {
        let id = uuid::Uuid::new_v4().to_string();
        let (tx, rx) = oneshot::channel::<ResponseFrame>();

        {
            let mut guard = self.pending.lock().await;
            guard.insert(id.clone(), tx);
        }

        let frame = GatewayFrame::Req(RequestFrame {
            id: id.clone(),
            method: method.to_string(),
            params: match params {
                Some(p) => Some(serde_json::to_value(p)?),
                None => None,
            },
        });

        let json = serde_json::to_string(&frame)?;
        if self
            .write_tx
            .send(Message::Text(json.into()))
            .await
            .is_err()
        {
            let mut guard = self.pending.lock().await;
            guard.remove(&id);
            return Err(Error::WebSocket("openclaw send failed".into()));
        }

        match tokio::time::timeout(DEFAULT_REQUEST_TIMEOUT, rx).await {
            Ok(Ok(resp)) => Ok(resp),
            Ok(Err(_)) => Err(Error::WebSocket("openclaw response channel closed".into())),
            Err(_) => {
                let mut guard = self.pending.lock().await;
                guard.remove(&id);
                Err(Error::Timeout(format!(
                    "openclaw request timed out: {}",
                    method
                )))
            }
        }
    }
}
