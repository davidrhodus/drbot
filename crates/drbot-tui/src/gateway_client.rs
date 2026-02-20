//! Gateway WebSocket client for the TUI.

use drbot_core::{Error, Result};
use drbot_protocol::{Event, Request, Response, WsMessage};
use futures::{SinkExt, StreamExt};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, Mutex};
use tokio_tungstenite::tungstenite::Message;

const DEFAULT_REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

#[derive(Clone)]
pub struct GatewayClient {
    write_tx: mpsc::Sender<Message>,
    pending: Arc<Mutex<HashMap<uuid::Uuid, oneshot::Sender<Response>>>>,
}

impl GatewayClient {
    pub async fn connect(
        url: &str,
    ) -> Result<(Self, mpsc::Receiver<Event>, mpsc::Receiver<Response>)> {
        let (ws_stream, _) = tokio_tungstenite::connect_async(url)
            .await
            .map_err(|e| Error::WebSocket(e.to_string()))?;
        let (mut write, mut read) = ws_stream.split();

        let (write_tx, mut write_rx) = mpsc::channel::<Message>(64);
        let pending: Arc<Mutex<HashMap<uuid::Uuid, oneshot::Sender<Response>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let pending_for_read = pending.clone();

        let (event_tx, event_rx) = mpsc::channel::<Event>(256);
        let (response_tx, response_rx) = mpsc::channel::<Response>(64);

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
                        let parsed: WsMessage = match serde_json::from_str(&text) {
                            Ok(m) => m,
                            Err(_) => continue,
                        };
                        match parsed {
                            WsMessage::Response(resp) => {
                                let tx = {
                                    let mut guard = pending_for_read.lock().await;
                                    guard.remove(&resp.id)
                                };
                                if let Some(tx) = tx {
                                    let _ = tx.send(resp);
                                } else {
                                    let _ = response_tx.send(resp).await;
                                }
                            }
                            WsMessage::Event(event) => {
                                let _ = event_tx.send(event).await;
                            }
                            WsMessage::Request(_) => {}
                        }
                    }
                    Message::Close(_) => break,
                    Message::Ping(_) | Message::Pong(_) | Message::Binary(_) => {}
                    Message::Frame(_) => {}
                }
            }

            let _ = event_tx
                .send(Event::new(
                    "system.disconnected",
                    serde_json::json!({ "reason": "gateway connection closed" }),
                ))
                .await;
        });

        Ok((Self { write_tx, pending }, event_rx, response_rx))
    }

    pub async fn send_request(&self, request: Request) -> Result<()> {
        let ws_msg = WsMessage::Request(request);
        let json = serde_json::to_string(&ws_msg)?;
        self.write_tx
            .send(Message::Text(json.into()))
            .await
            .map_err(|e| Error::WebSocket(e.to_string()))?;
        Ok(())
    }

    pub async fn request<P: Serialize>(&self, method: &str, params: P) -> Result<Response> {
        let request = Request::create(method, params);
        let id = request.id;
        let (tx, rx) = oneshot::channel::<Response>();

        {
            let mut guard = self.pending.lock().await;
            guard.insert(id, tx);
        }

        if let Err(e) = self.send_request(request).await {
            let mut guard = self.pending.lock().await;
            guard.remove(&id);
            return Err(e);
        }

        match tokio::time::timeout(DEFAULT_REQUEST_TIMEOUT, rx).await {
            Ok(Ok(resp)) => Ok(resp),
            Ok(Err(_)) => Err(Error::WebSocket("gateway response channel closed".into())),
            Err(_) => {
                let mut guard = self.pending.lock().await;
                guard.remove(&id);
                Err(Error::Timeout(format!(
                    "gateway request timed out: {}",
                    method
                )))
            }
        }
    }
}
