//! Matrix channel for drbot.
//!
//! Implements a minimal Matrix Client-Server API integration:
//! - Long-poll `/sync` for incoming `m.room.message` text events.
//! - Send text via `PUT /rooms/{roomId}/send/m.room.message/{txnId}`.

use async_trait::async_trait;
use drbot_channels::Channel;
use drbot_core::message::{Content, IncomingMessage, MessageSender, OutgoingMessage};
use drbot_core::{Error, Result};
use reqwest::Client;
use serde_json::Value;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, watch, RwLock};
use tracing::{debug, info, warn};
use uuid::Uuid;

/// Matrix channel configuration.
#[derive(Debug, Clone)]
pub struct MatrixConfig {
    /// Homeserver base URL (e.g. `https://matrix.org`).
    pub homeserver_url: String,
    /// User ID (e.g. `@user:matrix.org`).
    pub user_id: String,
    /// Access token.
    pub access_token: String,
    /// Allowed rooms (empty = all rooms).
    pub allowed_rooms: Vec<String>,
}

impl MatrixConfig {
    pub fn new(
        homeserver_url: impl Into<String>,
        user_id: impl Into<String>,
        access_token: impl Into<String>,
    ) -> Self {
        Self {
            homeserver_url: homeserver_url.into(),
            user_id: user_id.into(),
            access_token: access_token.into(),
            allowed_rooms: Vec::new(),
        }
    }

    fn base_url(&self) -> String {
        self.homeserver_url.trim_end_matches('/').to_string()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MatrixState {
    Disconnected,
    Connecting,
    Connected,
    Error(String),
}

/// Matrix channel implementation.
pub struct MatrixChannel {
    config: MatrixConfig,
    client: Client,
    state: Arc<RwLock<MatrixState>>,
    incoming_tx: broadcast::Sender<IncomingMessage>,
    shutdown_tx: Option<watch::Sender<bool>>,
    sync_handle: Option<tokio::task::JoinHandle<()>>,
}

impl MatrixChannel {
    pub fn new(config: MatrixConfig) -> Self {
        let (incoming_tx, _) = broadcast::channel(256);
        Self {
            config,
            client: Client::builder()
                .timeout(Duration::from_secs(35))
                .build()
                .unwrap_or_else(|_| Client::new()),
            state: Arc::new(RwLock::new(MatrixState::Disconnected)),
            incoming_tx,
            shutdown_tx: None,
            sync_handle: None,
        }
    }

    /// Current connection state.
    pub async fn state(&self) -> MatrixState {
        self.state.read().await.clone()
    }

    fn allowed_rooms_set(&self) -> HashSet<String> {
        self.config.allowed_rooms.iter().cloned().collect()
    }

    async fn set_state(&self, state: MatrixState) {
        *self.state.write().await = state;
    }
}

#[async_trait]
impl Channel for MatrixChannel {
    async fn connect(&mut self) -> Result<()> {
        match *self.state.read().await {
            MatrixState::Connected | MatrixState::Connecting => return Ok(()),
            MatrixState::Disconnected | MatrixState::Error(_) => {}
        }

        self.set_state(MatrixState::Connecting).await;

        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        self.shutdown_tx = Some(shutdown_tx);

        let client = self.client.clone();
        let config = self.config.clone();
        let allowed_rooms = self.allowed_rooms_set();
        let incoming_tx = self.incoming_tx.clone();
        let state = self.state.clone();

        info!(homeserver = %config.homeserver_url, "Matrix connect: starting sync loop");
        let handle = tokio::spawn(async move {
            if let Err(e) = sync_loop(
                client,
                config,
                allowed_rooms,
                incoming_tx,
                shutdown_rx,
                state,
            )
            .await
            {
                warn!(error = %e, "Matrix sync loop exited with error");
            }
        });
        self.sync_handle = Some(handle);

        // Consider the channel connected once the loop is running; it will update state on success/failure.
        self.set_state(MatrixState::Connected).await;
        Ok(())
    }

    async fn send(&self, to: &str, message: OutgoingMessage) -> Result<()> {
        let text = message
            .content
            .iter()
            .filter_map(|c| match c {
                Content::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");

        if text.trim().is_empty() {
            return Err(Error::InvalidInput(
                "Matrix send requires text content".into(),
            ));
        }

        let base_url = self.config.base_url();
        let txn_id = Uuid::new_v4().to_string();
        let url = format!(
            "{}/_matrix/client/v3/rooms/{}/send/m.room.message/{}",
            base_url, to, txn_id
        );

        let body = serde_json::json!({
            "msgtype": "m.text",
            "body": text,
        });

        let resp = self
            .client
            .put(&url)
            .bearer_auth(&self.config.access_token)
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::Http(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(Error::Channel(format!(
                "Matrix send failed ({}): {}",
                status, body
            )));
        }

        Ok(())
    }

    fn subscribe(&self) -> broadcast::Receiver<IncomingMessage> {
        self.incoming_tx.subscribe()
    }

    async fn disconnect(&mut self) -> Result<()> {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(true);
        }
        if let Some(handle) = self.sync_handle.take() {
            handle.abort();
        }
        self.set_state(MatrixState::Disconnected).await;
        info!("Matrix disconnected");
        Ok(())
    }

    fn channel_type(&self) -> &str {
        "matrix"
    }
}

async fn sync_loop(
    client: Client,
    config: MatrixConfig,
    allowed_rooms: HashSet<String>,
    incoming_tx: broadcast::Sender<IncomingMessage>,
    mut shutdown_rx: watch::Receiver<bool>,
    state: Arc<RwLock<MatrixState>>,
) -> Result<()> {
    let base_url = config.base_url();
    let sync_url = format!("{}/_matrix/client/v3/sync", base_url);

    let mut since: Option<String> = None;
    let mut backoff = Duration::from_secs(1);
    let max_backoff = Duration::from_secs(30);

    loop {
        if *shutdown_rx.borrow() {
            break;
        }

        let mut req = client
            .get(&sync_url)
            .bearer_auth(&config.access_token)
            .query(&[("timeout", "30000")]);
        if let Some(since) = &since {
            req = req.query(&[("since", since)]);
        }

        let resp = match req.send().await {
            Ok(r) => r,
            Err(e) => {
                set_state(&state, MatrixState::Error(e.to_string())).await;
                warn!(error = %e, "Matrix sync request failed; backing off");
                tokio::select! {
                    _ = tokio::time::sleep(backoff) => {}
                    _ = shutdown_rx.changed() => {}
                }
                backoff = (backoff * 2).min(max_backoff);
                continue;
            }
        };

        if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
            let body = resp.text().await.unwrap_or_default();
            set_state(
                &state,
                MatrixState::Error(format!("unauthorized: {}", body)),
            )
            .await;
            return Err(Error::Auth("Matrix unauthorized".into()));
        }

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            set_state(
                &state,
                MatrixState::Error(format!("sync failed ({}): {}", status, body)),
            )
            .await;
            warn!(status = %status, "Matrix sync non-success; backing off");
            tokio::select! {
                _ = tokio::time::sleep(backoff) => {}
                _ = shutdown_rx.changed() => {}
            }
            backoff = (backoff * 2).min(max_backoff);
            continue;
        }

        let body: Value = match resp.json().await {
            Ok(v) => v,
            Err(e) => {
                set_state(&state, MatrixState::Error(e.to_string())).await;
                warn!(error = %e, "Matrix sync JSON parse failed; backing off");
                tokio::select! {
                    _ = tokio::time::sleep(backoff) => {}
                    _ = shutdown_rx.changed() => {}
                }
                backoff = (backoff * 2).min(max_backoff);
                continue;
            }
        };

        backoff = Duration::from_secs(1);
        set_state(&state, MatrixState::Connected).await;

        since = body
            .get("next_batch")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        // Extract join room timelines.
        if let Some(join) = body.pointer("/rooms/join").and_then(|v| v.as_object()) {
            for (room_id, room_data) in join {
                if !allowed_rooms.is_empty() && !allowed_rooms.contains(room_id) {
                    continue;
                }
                let events = room_data
                    .pointer("/timeline/events")
                    .and_then(|v| v.as_array());
                let Some(events) = events else { continue };

                for event in events {
                    let Some(event_type) = event.get("type").and_then(|v| v.as_str()) else {
                        continue;
                    };
                    if event_type != "m.room.message" {
                        continue;
                    }

                    let Some(sender_id) = event.get("sender").and_then(|v| v.as_str()) else {
                        continue;
                    };
                    if sender_id == config.user_id {
                        // Avoid echoing our own messages.
                        continue;
                    }

                    let Some(content) = event.get("content").and_then(|v| v.as_object()) else {
                        continue;
                    };
                    let msgtype = content.get("msgtype").and_then(|v| v.as_str());
                    if msgtype != Some("m.text") {
                        continue;
                    }
                    let Some(text) = content.get("body").and_then(|v| v.as_str()) else {
                        continue;
                    };

                    let incoming = IncomingMessage {
                        id: Uuid::new_v4(),
                        channel_type: "matrix".to_string(),
                        channel_id: room_id.to_string(),
                        sender: MessageSender {
                            id: sender_id.to_string(),
                            name: None,
                            username: None,
                        },
                        content: vec![Content::Text {
                            text: text.to_string(),
                        }],
                        received_at: chrono::Utc::now(),
                        raw: Some(event.clone()),
                        reply_to: None,
                    };

                    debug!(room_id = %room_id, sender = %sender_id, "Matrix message received");
                    let _ = incoming_tx.send(incoming);
                }
            }
        }
    }

    set_state(&state, MatrixState::Disconnected).await;
    Ok(())
}

async fn set_state(state: &Arc<RwLock<MatrixState>>, new_state: MatrixState) {
    *state.write().await = new_state;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_base_url_trims_slash() {
        let cfg = MatrixConfig::new("https://example.org/", "@u:example.org", "tok");
        assert_eq!(cfg.base_url(), "https://example.org");
    }
}
