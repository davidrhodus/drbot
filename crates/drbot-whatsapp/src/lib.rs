//! WhatsApp channel for drbot via Baileys protocol.
//!
//! This crate provides WhatsApp integration using a Node.js bridge process
//! that implements the Baileys (WhatsApp Web) protocol.
//!
//! # Requirements
//!
//! - Node.js with the Baileys bridge script installed
//! - Network access to WhatsApp servers
//!
//! # Architecture
//!
//! ```text
//! drbot-whatsapp <--WebSocket--> Node.js Bridge <--WhatsApp Web--> WhatsApp Servers
//! ```
//!
//! # Example
//!
//! ```rust,no_run
//! use drbot_whatsapp::WhatsAppChannel;
//! use drbot_channels::Channel;
//!
//! async fn example() -> drbot_core::Result<()> {
//!     let mut channel = WhatsAppChannel::new("ws://localhost:3001");
//!     channel.connect().await?;
//!
//!     // Subscribe to incoming messages
//!     let mut rx = channel.subscribe();
//!
//!     while let Ok(msg) = rx.recv().await {
//!         println!("Received: {:?}", msg);
//!     }
//!
//!     Ok(())
//! }
//! ```

mod bridge;

pub use bridge::{BridgeEvent, BridgeRequest, ConnectionStatus, WhatsAppMessage};

use async_trait::async_trait;
use chrono::{TimeZone, Utc};
use drbot_channels::Channel;
use drbot_core::message::{Content, IncomingMessage, MessageSender, OutgoingMessage};
use futures::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, oneshot, RwLock};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// WhatsApp channel implementation.
pub struct WhatsAppChannel {
    /// Bridge WebSocket URL.
    bridge_url: String,
    /// Session directory.
    session_dir: Option<String>,
    /// Message sender for incoming messages.
    tx: broadcast::Sender<IncomingMessage>,
    /// Whether connected.
    connected: Arc<AtomicBool>,
    /// Connection status.
    status: Arc<RwLock<ConnectionStatus>>,
    /// Sender for outgoing requests.
    request_tx: Option<mpsc::Sender<String>>,
    /// Pending send confirmations.
    pending_sends: Arc<RwLock<HashMap<String, oneshot::Sender<String>>>>,
    /// QR code callback.
    qr_callback: Option<Box<dyn Fn(&str) + Send + Sync>>,
}

impl WhatsAppChannel {
    /// Create a new WhatsApp channel.
    pub fn new(bridge_url: impl Into<String>) -> Self {
        let (tx, _) = broadcast::channel(256);

        Self {
            bridge_url: bridge_url.into(),
            session_dir: None,
            tx,
            connected: Arc::new(AtomicBool::new(false)),
            status: Arc::new(RwLock::new(ConnectionStatus::Close)),
            request_tx: None,
            pending_sends: Arc::new(RwLock::new(HashMap::new())),
            qr_callback: None,
        }
    }

    /// Set the session directory.
    pub fn with_session_dir(mut self, dir: impl Into<String>) -> Self {
        self.session_dir = Some(dir.into());
        self
    }

    /// Set a QR code callback.
    pub fn with_qr_callback<F>(mut self, callback: F) -> Self
    where
        F: Fn(&str) + Send + Sync + 'static,
    {
        self.qr_callback = Some(Box::new(callback));
        self
    }

    /// Get current connection status.
    pub async fn status(&self) -> ConnectionStatus {
        *self.status.read().await
    }

    /// Send a request to the bridge.
    async fn send_request(&self, request: BridgeRequest) -> drbot_core::Result<()> {
        let Some(tx) = &self.request_tx else {
            return Err(drbot_core::Error::Channel("Not connected".to_string()));
        };

        let json = serde_json::to_string(&request)
            .map_err(|e| drbot_core::Error::Internal(format!("JSON encode failed: {}", e)))?;

        tx.send(json)
            .await
            .map_err(|_| drbot_core::Error::Channel("Send channel closed".to_string()))?;

        Ok(())
    }

    /// Request QR code.
    pub async fn request_qr(&self) -> drbot_core::Result<()> {
        self.send_request(BridgeRequest::GetQr).await
    }
}

#[async_trait]
impl Channel for WhatsAppChannel {
    fn channel_type(&self) -> &str {
        "whatsapp"
    }

    async fn connect(&mut self) -> drbot_core::Result<()> {
        info!("Connecting to WhatsApp bridge at {}", self.bridge_url);

        let (ws_stream, _) = connect_async(&self.bridge_url)
            .await
            .map_err(|e| drbot_core::Error::Channel(format!("WebSocket connect failed: {}", e)))?;

        let (mut write, mut read) = ws_stream.split();
        let (request_tx, mut request_rx) = mpsc::channel::<String>(64);

        self.request_tx = Some(request_tx);
        self.connected.store(true, Ordering::SeqCst);

        // Send init message
        let session_dir = self
            .session_dir
            .clone()
            .unwrap_or_else(|| ".whatsapp".to_string());
        let init_msg = serde_json::to_string(&BridgeRequest::Init { session_dir })
            .map_err(|e| drbot_core::Error::Internal(e.to_string()))?;

        write
            .send(Message::Text(init_msg.into()))
            .await
            .map_err(|e| drbot_core::Error::Channel(format!("Send failed: {}", e)))?;

        // Clone handles for the spawned tasks
        let connected = self.connected.clone();
        let status = self.status.clone();
        let tx = self.tx.clone();
        let pending_sends = self.pending_sends.clone();
        let qr_callback = self.qr_callback.take();

        // Writer task
        tokio::spawn(async move {
            while let Some(msg) = request_rx.recv().await {
                if write.send(Message::Text(msg.into())).await.is_err() {
                    break;
                }
            }
        });

        // Reader task
        tokio::spawn(async move {
            while let Some(result) = read.next().await {
                let Ok(msg) = result else {
                    warn!("WebSocket read error");
                    continue;
                };

                let Message::Text(text) = msg else {
                    continue;
                };

                let text_str: &str = text.as_ref();
                let Ok(event) = serde_json::from_str::<BridgeEvent>(text_str) else {
                    warn!("Failed to parse bridge event: {}", text_str);
                    continue;
                };

                match event {
                    BridgeEvent::Connection { status: new_status } => {
                        info!("WhatsApp connection status: {:?}", new_status);
                        *status.write().await = new_status;

                        if new_status == ConnectionStatus::Close {
                            connected.store(false, Ordering::SeqCst);
                            break;
                        }
                    }
                    BridgeEvent::Qr { qr } => {
                        debug!("Received QR code");
                        if let Some(ref callback) = qr_callback {
                            callback(&qr);
                        }
                    }
                    BridgeEvent::Ready => {
                        info!("WhatsApp ready");
                    }
                    BridgeEvent::Message { message } => {
                        if message.from_me {
                            continue;
                        }

                        let Some(text) = message.text.clone() else {
                            continue;
                        };

                        let incoming = IncomingMessage {
                            id: Uuid::new_v4(),
                            channel_type: "whatsapp".to_string(),
                            channel_id: message.chat.clone(),
                            sender: MessageSender {
                                id: message.sender.clone(),
                                name: message.sender_name.clone(),
                                username: WhatsAppMessage::phone_from_jid(&message.sender),
                            },
                            content: vec![Content::Text { text }],
                            received_at: Utc
                                .timestamp_opt(message.timestamp, 0)
                                .single()
                                .unwrap_or_else(Utc::now),
                            raw: Some(serde_json::json!({
                                "id": message.id,
                                "is_group": message.is_group(),
                            })),
                            reply_to: message.quoted_id,
                        };

                        if tx.send(incoming).is_err() {
                            // No receivers
                        }
                    }
                    BridgeEvent::Sent { id, message_id } => {
                        debug!("Message sent: {} -> {}", id, message_id);
                        let mut pending = pending_sends.write().await;
                        if let Some(sender) = pending.remove(&id) {
                            let _ = sender.send(message_id);
                        }
                    }
                    BridgeEvent::Error { error, id } => {
                        error!("Bridge error: {} (id: {:?})", error, id);
                    }
                }
            }

            connected.store(false, Ordering::SeqCst);
            info!("WhatsApp connection closed");
        });

        info!("WhatsApp channel connected");
        Ok(())
    }

    async fn disconnect(&mut self) -> drbot_core::Result<()> {
        info!("Disconnecting WhatsApp channel");

        if let Err(e) = self.send_request(BridgeRequest::Disconnect).await {
            warn!("Error sending disconnect: {}", e);
        }

        self.connected.store(false, Ordering::SeqCst);
        self.request_tx = None;

        Ok(())
    }

    async fn send(&self, to: &str, message: OutgoingMessage) -> drbot_core::Result<()> {
        debug!("Sending WhatsApp message to {}", to);

        // Extract text content
        let text = message
            .content
            .iter()
            .filter_map(|c| {
                if let Content::Text { text } = c {
                    Some(text.as_str())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("\n");

        if text.is_empty() {
            return Err(drbot_core::Error::InvalidInput(
                "Message has no text content".to_string(),
            ));
        }

        // Ensure JID format
        let to_jid = if to.contains('@') {
            to.to_string()
        } else {
            format!("{}@s.whatsapp.net", to)
        };

        let id = Uuid::new_v4().to_string();
        let (response_tx, response_rx) = oneshot::channel();

        // Register pending send
        self.pending_sends
            .write()
            .await
            .insert(id.clone(), response_tx);

        // Send the message
        self.send_request(BridgeRequest::SendMessage {
            id: id.clone(),
            to: to_jid,
            text,
        })
        .await?;

        // Wait for confirmation with timeout
        match tokio::time::timeout(std::time::Duration::from_secs(30), response_rx).await {
            Ok(Ok(_message_id)) => Ok(()),
            Ok(Err(_)) => Err(drbot_core::Error::Channel("Send cancelled".to_string())),
            Err(_) => {
                // Remove from pending
                self.pending_sends.write().await.remove(&id);
                Err(drbot_core::Error::Timeout("Send timeout".to_string()))
            }
        }
    }

    fn subscribe(&self) -> broadcast::Receiver<IncomingMessage> {
        self.tx.subscribe()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_channel_type() {
        let channel = WhatsAppChannel::new("ws://localhost:3001");
        assert_eq!(channel.channel_type(), "whatsapp");
    }

    #[test]
    fn test_with_session_dir() {
        let channel = WhatsAppChannel::new("ws://localhost:3001").with_session_dir("/tmp/whatsapp");
        assert_eq!(channel.session_dir, Some("/tmp/whatsapp".to_string()));
    }

    #[test]
    fn test_jid_format() {
        // Test that we format JIDs correctly
        let phone = "1234567890";
        let jid = format!("{}@s.whatsapp.net", phone);
        assert_eq!(jid, "1234567890@s.whatsapp.net");
    }

    #[tokio::test]
    async fn test_initial_status() {
        let channel = WhatsAppChannel::new("ws://localhost:3001");
        assert_eq!(channel.status().await, ConnectionStatus::Close);
    }
}
