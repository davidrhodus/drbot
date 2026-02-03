//! Client connection management.

use drbot_protocol::{Event, WsMessage};
use futures::stream::SplitSink;
use futures::SinkExt;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;
use uuid::Uuid;

/// A connected WebSocket client.
#[derive(Clone)]
pub struct Client {
    inner: Arc<ClientInner>,
}

struct ClientInner {
    /// Unique client ID.
    id: Uuid,
    /// Remote address.
    addr: SocketAddr,
    /// Whether the client is authenticated.
    authenticated: Mutex<bool>,
    /// WebSocket sender.
    sender: Mutex<SplitSink<WebSocketStream<TcpStream>, Message>>,
    /// Connection time.
    connected_at: Instant,
}

impl Client {
    /// Create a new client.
    pub fn new(addr: SocketAddr, sender: SplitSink<WebSocketStream<TcpStream>, Message>) -> Self {
        Self {
            inner: Arc::new(ClientInner {
                id: Uuid::new_v4(),
                addr,
                authenticated: Mutex::new(false),
                sender: Mutex::new(sender),
                connected_at: Instant::now(),
            }),
        }
    }

    /// Get the client ID.
    pub fn id(&self) -> Uuid {
        self.inner.id
    }

    /// Get the remote address.
    pub fn addr(&self) -> SocketAddr {
        self.inner.addr
    }

    /// Check if the client is authenticated.
    pub async fn is_authenticated(&self) -> bool {
        *self.inner.authenticated.lock().await
    }

    /// Set the authenticated state.
    pub async fn set_authenticated(&self, authenticated: bool) {
        *self.inner.authenticated.lock().await = authenticated;
    }

    /// Get connection duration in seconds.
    pub fn connection_duration_secs(&self) -> u64 {
        self.inner.connected_at.elapsed().as_secs()
    }

    /// Send a WebSocket message to the client.
    pub async fn send(&self, message: WsMessage) -> Result<(), String> {
        let json = message
            .to_json()
            .map_err(|e| format!("Failed to serialize message: {}", e))?;

        let mut sender = self.inner.sender.lock().await;
        sender
            .send(Message::Text(json.into()))
            .await
            .map_err(|e| format!("Failed to send message: {}", e))
    }

    /// Send an event to the client.
    pub async fn send_event(&self, event: Event) -> Result<(), String> {
        self.send(WsMessage::Event(event)).await
    }

    /// Close the connection.
    pub async fn close(&self) -> Result<(), String> {
        let mut sender = self.inner.sender.lock().await;
        sender
            .close()
            .await
            .map_err(|e| format!("Failed to close connection: {}", e))
    }
}
