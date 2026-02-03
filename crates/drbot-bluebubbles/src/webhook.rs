//! Socket.IO event handler for BlueBubbles.

use crate::{api::Message, Result};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};

/// Socket.IO event types.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum SocketEvent {
    /// New message received.
    NewMessage { data: Message },
    /// Message updated.
    MessageUpdated { data: Message },
    /// Message read.
    MessageRead { chat_guid: String },
    /// Typing indicator.
    TypingIndicator { display: bool, guid: String },
    /// Chat read status changed.
    ChatReadStatusChanged { chat_guid: String, read: bool },
    /// Connection status.
    Connected,
    /// Disconnected.
    Disconnected { reason: Option<String> },
    /// Error.
    Error { message: String },
}

/// Socket.IO handler for BlueBubbles real-time events.
pub struct SocketHandler {
    /// Server URL.
    server_url: String,
    /// Password.
    password: String,
    /// Event sender.
    event_tx: broadcast::Sender<SocketEvent>,
    /// Connected state.
    connected: Arc<RwLock<bool>>,
}

impl SocketHandler {
    /// Create a new socket handler.
    pub fn new(server_url: &str, password: &str) -> Self {
        let (event_tx, _) = broadcast::channel(256);

        Self {
            server_url: server_url.to_string(),
            password: password.to_string(),
            event_tx,
            connected: Arc::new(RwLock::new(false)),
        }
    }

    /// Connect to the server.
    pub async fn connect(&self) -> Result<()> {
        // In a real implementation, this would use rust_socketio to connect
        // For now, we'll just mark as connected

        let mut connected = self.connected.write().await;
        *connected = true;

        let _ = self.event_tx.send(SocketEvent::Connected);

        tracing::info!(url = %self.server_url, "Socket.IO connected");

        Ok(())
    }

    /// Disconnect from the server.
    pub async fn disconnect(&self) -> Result<()> {
        let mut connected = self.connected.write().await;
        *connected = false;

        let _ = self
            .event_tx
            .send(SocketEvent::Disconnected { reason: None });

        tracing::info!("Socket.IO disconnected");

        Ok(())
    }

    /// Check if connected.
    pub async fn is_connected(&self) -> bool {
        *self.connected.read().await
    }

    /// Subscribe to events.
    pub fn subscribe(&self) -> broadcast::Receiver<SocketEvent> {
        self.event_tx.subscribe()
    }

    /// Emit an event (for testing).
    pub fn emit_event(&self, event: SocketEvent) {
        let _ = self.event_tx.send(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_socket_handler() {
        let handler = SocketHandler::new("http://localhost:1234", "password");

        assert!(!handler.is_connected().await);

        handler.connect().await.unwrap();
        assert!(handler.is_connected().await);

        handler.disconnect().await.unwrap();
        assert!(!handler.is_connected().await);
    }
}
