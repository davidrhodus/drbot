//! Channel implementation for BlueBubbles.

use crate::{
    BlueBubblesApi, BlueBubblesConfig, BlueBubblesError, Result, SocketEvent, SocketHandler,
};
use async_trait::async_trait;
use drbot_channels::Channel;
use drbot_core::message::{IncomingMessage, OutgoingMessage};
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};

/// BlueBubbles channel implementation.
pub struct BlueBubblesChannel {
    /// Configuration.
    config: BlueBubblesConfig,
    /// API client.
    api: Arc<BlueBubblesApi>,
    /// Socket handler.
    socket: Arc<SocketHandler>,
    /// Message sender.
    message_tx: broadcast::Sender<IncomingMessage>,
    /// Connected state.
    connected: Arc<RwLock<bool>>,
}

impl BlueBubblesChannel {
    /// Create a new BlueBubbles channel.
    pub fn new(config: BlueBubblesConfig) -> Self {
        let api = Arc::new(BlueBubblesApi::new(&config.server_url, &config.password));
        let socket = Arc::new(SocketHandler::new(&config.server_url, &config.password));
        let (message_tx, _) = broadcast::channel(256);

        Self {
            config,
            api,
            socket,
            message_tx,
            connected: Arc::new(RwLock::new(false)),
        }
    }

    /// Check if a handle is allowed.
    fn is_handle_allowed(&self, address: &str) -> bool {
        if self.config.allowed_handles.is_empty() {
            true
        } else {
            self.config.allowed_handles.iter().any(|h| h == address)
        }
    }

    /// Start processing socket events.
    async fn process_events(&self) {
        let mut rx = self.socket.subscribe();
        let message_tx = self.message_tx.clone();
        let allowed_handles = self.config.allowed_handles.clone();

        tokio::spawn(async move {
            while let Ok(event) = rx.recv().await {
                match event {
                    SocketEvent::NewMessage { data } => {
                        // Check if from allowed handle
                        if let Some(ref handle) = data.handle {
                            if !allowed_handles.is_empty()
                                && !allowed_handles.contains(&handle.address)
                            {
                                continue;
                            }
                        }

                        // Convert to IncomingMessage
                        if let Some(text) = data.text {
                            if !data.is_from_me {
                                let sender = data
                                    .handle
                                    .map(|h| h.address)
                                    .unwrap_or_else(|| "unknown".to_string());

                                let msg = IncomingMessage {
                                    id: uuid::Uuid::new_v4(),
                                    channel_type: "bluebubbles".to_string(),
                                    channel_id: data.chat_guid.clone().unwrap_or_default(),
                                    sender: drbot_core::message::MessageSender {
                                        id: sender.clone(),
                                        name: Some(sender.clone()),
                                        username: None,
                                    },
                                    content: vec![drbot_core::message::Content::Text { text }],
                                    received_at: chrono::Utc::now(),
                                    raw: None,
                                    reply_to: None,
                                };

                                let _ = message_tx.send(msg);
                            }
                        }
                    }
                    SocketEvent::Error { message } => {
                        tracing::warn!(error = %message, "BlueBubbles socket error");
                    }
                    _ => {}
                }
            }
        });
    }
}

#[async_trait]
impl Channel for BlueBubblesChannel {
    async fn connect(&mut self) -> drbot_core::Result<()> {
        // Test API connection
        self.api
            .server_info()
            .await
            .map_err(|e| drbot_core::Error::Config(e.to_string()))?;

        // Connect socket if enabled
        if self.config.enable_socket {
            self.socket
                .connect()
                .await
                .map_err(|e| drbot_core::Error::Io(std::io::Error::other(e.to_string())))?;

            self.process_events().await;
        }

        let mut connected = self.connected.write().await;
        *connected = true;

        tracing::info!("BlueBubbles channel connected");
        Ok(())
    }

    async fn send(&self, to: &str, message: OutgoingMessage) -> drbot_core::Result<()> {
        if !self.is_handle_allowed(to) {
            return Err(drbot_core::Error::Config(format!(
                "Handle not allowed: {}",
                to
            )));
        }

        // Extract text from content blocks
        let text: String = message
            .content
            .iter()
            .filter_map(|c| {
                if let drbot_core::message::Content::Text { text } = c {
                    Some(text.as_str())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("\n");

        // Try to find existing chat first, otherwise send to new chat
        let chats = self
            .api
            .list_chats(Some(100), None)
            .await
            .map_err(|e| drbot_core::Error::Io(std::io::Error::other(e.to_string())))?;

        let existing_chat = chats
            .iter()
            .find(|c| c.participants.iter().any(|p| p.address == to));

        if let Some(chat) = existing_chat {
            self.api
                .send_message(&chat.guid, &text)
                .await
                .map_err(|e| drbot_core::Error::Io(std::io::Error::other(e.to_string())))?;
        } else {
            self.api
                .send_new_message(to, &text)
                .await
                .map_err(|e| drbot_core::Error::Io(std::io::Error::other(e.to_string())))?;
        }

        Ok(())
    }

    fn subscribe(&self) -> broadcast::Receiver<IncomingMessage> {
        self.message_tx.subscribe()
    }

    async fn disconnect(&mut self) -> drbot_core::Result<()> {
        if self.config.enable_socket {
            self.socket
                .disconnect()
                .await
                .map_err(|e| drbot_core::Error::Io(std::io::Error::other(e.to_string())))?;
        }

        let mut connected = self.connected.write().await;
        *connected = false;

        tracing::info!("BlueBubbles channel disconnected");
        Ok(())
    }

    fn channel_type(&self) -> &str {
        "bluebubbles"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bluebubbles_channel_handle_allowed() {
        let config = BlueBubblesConfig {
            allowed_handles: vec!["+1234567890".to_string()],
            ..Default::default()
        };
        let channel = BlueBubblesChannel::new(config);

        assert!(channel.is_handle_allowed("+1234567890"));
        assert!(!channel.is_handle_allowed("+0987654321"));
    }
}
