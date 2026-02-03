//! Discord gateway channel for drbot.
//!
//! This crate provides Discord integration via the Discord Gateway WebSocket API.

mod api;

pub use api::*;

use async_trait::async_trait;
use drbot_channels::Channel;
use drbot_core::message::{Content, IncomingMessage, MessageSender, OutgoingMessage};
use drbot_core::Result;
use futures::{SinkExt, StreamExt};
use reqwest::Client;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tokio_tungstenite::{connect_async, tungstenite::Message as WsMessage};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Discord API base URL.
pub const API_BASE_URL: &str = "https://discord.com/api/v10";

/// Discord Gateway URL.
pub const GATEWAY_URL: &str = "wss://gateway.discord.gg/?v=10&encoding=json";

/// Discord channel configuration.
#[derive(Debug, Clone)]
pub struct DiscordConfig {
    /// Bot token.
    pub token: String,
    /// Gateway intents.
    pub intents: u32,
}

impl DiscordConfig {
    /// Create a new configuration with a bot token.
    pub fn new(token: impl Into<String>) -> Self {
        Self {
            token: token.into(),
            // Default intents for message handling
            intents: intents::GUILDS
                | intents::GUILD_MESSAGES
                | intents::GUILD_MESSAGE_CONTENT
                | intents::DIRECT_MESSAGES,
        }
    }

    /// Set gateway intents.
    pub fn with_intents(mut self, intents: u32) -> Self {
        self.intents = intents;
        self
    }
}

/// Discord channel implementation.
pub struct DiscordChannel {
    /// Configuration.
    config: DiscordConfig,
    /// HTTP client for REST API.
    http_client: Client,
    /// Broadcast sender for incoming messages.
    incoming_tx: broadcast::Sender<IncomingMessage>,
    /// Whether the gateway is connected.
    connected: Arc<AtomicBool>,
    /// Current sequence number.
    sequence: Arc<AtomicU64>,
    /// Session ID for resume.
    session_id: Arc<RwLock<Option<String>>>,
    /// Bot user info.
    bot_user: Arc<RwLock<Option<User>>>,
    /// Gateway task handle.
    gateway_handle: Option<tokio::task::JoinHandle<()>>,
    /// Heartbeat task handle.
    heartbeat_handle: Option<tokio::task::JoinHandle<()>>,
}

impl DiscordChannel {
    /// Create a new Discord channel.
    pub fn new(config: DiscordConfig) -> Self {
        let (incoming_tx, _) = broadcast::channel(256);
        Self {
            config,
            http_client: Client::new(),
            incoming_tx,
            connected: Arc::new(AtomicBool::new(false)),
            sequence: Arc::new(AtomicU64::new(0)),
            session_id: Arc::new(RwLock::new(None)),
            bot_user: Arc::new(RwLock::new(None)),
            gateway_handle: None,
            heartbeat_handle: None,
        }
    }

    /// Create from a bot token.
    pub fn from_token(token: impl Into<String>) -> Self {
        Self::new(DiscordConfig::new(token))
    }

    /// Get the bot user info.
    pub async fn bot_user(&self) -> Option<User> {
        self.bot_user.read().await.clone()
    }

    /// Send a message via REST API.
    async fn send_rest_message(
        &self,
        channel_id: &str,
        content: &str,
        reply_to: Option<&str>,
    ) -> Result<()> {
        let url = format!("{}/channels/{}/messages", API_BASE_URL, channel_id);

        let request = CreateMessageRequest {
            content: content.to_string(),
            message_reference: reply_to.map(|id| MessageReference {
                message_id: id.to_string(),
            }),
        };

        let response = self
            .http_client
            .post(&url)
            .header("Authorization", format!("Bot {}", self.config.token))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| drbot_core::Error::Http(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            error!(status = %status, body = %body, "Failed to send Discord message");
            return Err(drbot_core::Error::Channel(format!(
                "Discord API error: {} - {}",
                status, body
            )));
        }

        Ok(())
    }
}

#[async_trait]
impl Channel for DiscordChannel {
    async fn connect(&mut self) -> Result<()> {
        info!("Connecting to Discord Gateway");

        let (ws_stream, _) = connect_async(GATEWAY_URL)
            .await
            .map_err(|e| drbot_core::Error::WebSocket(e.to_string()))?;

        let (mut write, mut read) = ws_stream.split();

        // Wait for Hello
        let hello_msg = read
            .next()
            .await
            .ok_or_else(|| {
                drbot_core::Error::WebSocket("Connection closed before Hello".to_string())
            })?
            .map_err(|e| drbot_core::Error::WebSocket(e.to_string()))?;

        let hello_payload: GatewayPayload = match hello_msg {
            WsMessage::Text(text) => serde_json::from_str(&text)?,
            _ => {
                return Err(drbot_core::Error::WebSocket(
                    "Expected text message".to_string(),
                ))
            }
        };

        let hello_data: HelloData = serde_json::from_value(
            hello_payload
                .d
                .ok_or_else(|| drbot_core::Error::WebSocket("Missing Hello data".to_string()))?,
        )?;

        let heartbeat_interval = hello_data.heartbeat_interval;
        debug!(interval = heartbeat_interval, "Received Hello");

        // Send Identify
        let identify = GatewayPayload {
            op: Opcode::Identify as u8,
            d: Some(serde_json::to_value(IdentifyData {
                token: self.config.token.clone(),
                properties: ConnectionProperties {
                    os: std::env::consts::OS.to_string(),
                    browser: "drbot".to_string(),
                    device: "drbot".to_string(),
                },
                intents: self.config.intents,
            })?),
            s: None,
            t: None,
        };

        write
            .send(WsMessage::Text(serde_json::to_string(&identify)?.into()))
            .await
            .map_err(|e| drbot_core::Error::WebSocket(e.to_string()))?;

        // Wait for Ready
        let ready_msg = read
            .next()
            .await
            .ok_or_else(|| {
                drbot_core::Error::WebSocket("Connection closed before Ready".to_string())
            })?
            .map_err(|e| drbot_core::Error::WebSocket(e.to_string()))?;

        let ready_payload: GatewayPayload = match ready_msg {
            WsMessage::Text(text) => serde_json::from_str(&text)?,
            _ => {
                return Err(drbot_core::Error::WebSocket(
                    "Expected text message".to_string(),
                ))
            }
        };

        if ready_payload.t.as_deref() == Some("READY") {
            let ready_data: ReadyData =
                serde_json::from_value(ready_payload.d.ok_or_else(|| {
                    drbot_core::Error::WebSocket("Missing Ready data".to_string())
                })?)?;

            info!(
                user = %ready_data.user.username,
                session_id = %ready_data.session_id,
                "Connected to Discord"
            );

            *self.session_id.write().await = Some(ready_data.session_id);
            *self.bot_user.write().await = Some(ready_data.user);
        }

        self.connected.store(true, Ordering::SeqCst);

        // Start heartbeat task
        let connected = self.connected.clone();
        let sequence = self.sequence.clone();
        let (heartbeat_tx, mut heartbeat_rx) = tokio::sync::mpsc::channel::<WsMessage>(8);

        let heartbeat_handle = tokio::spawn(async move {
            let mut interval =
                tokio::time::interval(std::time::Duration::from_millis(heartbeat_interval));
            while connected.load(Ordering::SeqCst) {
                interval.tick().await;
                let seq = sequence.load(Ordering::SeqCst);
                let heartbeat = GatewayPayload {
                    op: Opcode::Heartbeat as u8,
                    d: if seq > 0 {
                        Some(serde_json::json!(seq))
                    } else {
                        None
                    },
                    s: None,
                    t: None,
                };
                if let Ok(json) = serde_json::to_string(&heartbeat) {
                    if heartbeat_tx
                        .send(WsMessage::Text(json.into()))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
            }
        });
        self.heartbeat_handle = Some(heartbeat_handle);

        // Start message processing task
        let incoming_tx = self.incoming_tx.clone();
        let connected = self.connected.clone();
        let sequence = self.sequence.clone();
        let bot_user = self.bot_user.clone();

        let gateway_handle = tokio::spawn(async move {
            // Merge heartbeat and read streams
            loop {
                tokio::select! {
                    Some(msg) = heartbeat_rx.recv() => {
                        if write.send(msg).await.is_err() {
                            break;
                        }
                    }
                    msg = read.next() => {
                        match msg {
                            Some(Ok(WsMessage::Text(text))) => {
                                if let Ok(payload) = serde_json::from_str::<GatewayPayload>(&text) {
                                    // Update sequence
                                    if let Some(s) = payload.s {
                                        sequence.store(s, Ordering::SeqCst);
                                    }

                                    match Opcode::from(payload.op) {
                                        Opcode::Dispatch => {
                                            if payload.t.as_deref() == Some("MESSAGE_CREATE") {
                                                if let Some(data) = payload.d {
                                                    if let Ok(msg) = serde_json::from_value::<api::Message>(data) {
                                                        // Skip bot's own messages
                                                        let bot = bot_user.read().await;
                                                        if let Some(bot) = &*bot {
                                                            if msg.author.id == bot.id {
                                                                continue;
                                                            }
                                                        }
                                                        drop(bot);

                                                        let incoming = IncomingMessage {
                                                            id: Uuid::new_v4(),
                                                            channel_type: "discord".to_string(),
                                                            channel_id: msg.channel_id.clone(),
                                                            sender: MessageSender {
                                                                id: msg.author.id.clone(),
                                                                name: Some(msg.author.display_name().to_string()),
                                                                username: Some(msg.author.username.clone()),
                                                            },
                                                            content: vec![Content::Text { text: msg.content.clone() }],
                                                            received_at: chrono::Utc::now(),
                                                            raw: serde_json::to_value(&msg).ok(),
                                                            reply_to: msg.referenced_message.as_ref().map(|m| m.id.clone()),
                                                        };

                                                        let _ = incoming_tx.send(incoming);
                                                    }
                                                }
                                            }
                                        }
                                        Opcode::HeartbeatAck => {
                                            debug!("Heartbeat acknowledged");
                                        }
                                        Opcode::Reconnect => {
                                            warn!("Received reconnect request");
                                            break;
                                        }
                                        Opcode::InvalidSession => {
                                            warn!("Invalid session");
                                            break;
                                        }
                                        _ => {}
                                    }
                                }
                            }
                            Some(Ok(WsMessage::Close(_))) | None => {
                                break;
                            }
                            Some(Err(e)) => {
                                error!(error = %e, "WebSocket error");
                                break;
                            }
                            _ => {}
                        }
                    }
                }

                if !connected.load(Ordering::SeqCst) {
                    break;
                }
            }

            connected.store(false, Ordering::SeqCst);
            info!("Discord gateway disconnected");
        });
        self.gateway_handle = Some(gateway_handle);

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

        if text.is_empty() {
            return Err(drbot_core::Error::InvalidInput(
                "No text content in message".to_string(),
            ));
        }

        self.send_rest_message(to, &text, message.reply_to.as_deref())
            .await
    }

    fn subscribe(&self) -> broadcast::Receiver<IncomingMessage> {
        self.incoming_tx.subscribe()
    }

    async fn disconnect(&mut self) -> Result<()> {
        info!("Disconnecting from Discord");

        self.connected.store(false, Ordering::SeqCst);

        if let Some(handle) = self.heartbeat_handle.take() {
            handle.abort();
        }

        if let Some(handle) = self.gateway_handle.take() {
            handle.abort();
        }

        Ok(())
    }

    fn channel_type(&self) -> &str {
        "discord"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_creation() {
        let config = DiscordConfig::new("test_token");
        assert_eq!(config.token, "test_token");
        assert!(config.intents > 0);
    }

    #[test]
    fn test_channel_creation() {
        let channel = DiscordChannel::from_token("test_token");
        assert_eq!(channel.channel_type(), "discord");
    }

    #[test]
    fn test_user_display_name() {
        let user = User {
            id: "123".to_string(),
            username: "testuser".to_string(),
            discriminator: "0".to_string(),
            global_name: Some("Test User".to_string()),
            bot: false,
        };
        assert_eq!(user.display_name(), "Test User");

        let user2 = User {
            id: "456".to_string(),
            username: "anotheruser".to_string(),
            discriminator: "0".to_string(),
            global_name: None,
            bot: false,
        };
        assert_eq!(user2.display_name(), "anotheruser");
    }
}
