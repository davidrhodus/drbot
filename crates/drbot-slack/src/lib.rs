//! Slack channel for drbot.
//!
//! This crate provides Slack integration via Socket Mode for real-time events
//! and the Web API for sending messages.

mod api;

pub use api::*;

use async_trait::async_trait;
use drbot_channels::Channel;
use drbot_core::message::{Content, IncomingMessage, MessageSender, OutgoingMessage};
use drbot_core::Result;
use futures::{SinkExt, StreamExt};
use reqwest::Client;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tokio_tungstenite::{connect_async, tungstenite::Message as WsMessage};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Slack API base URL.
pub const API_BASE_URL: &str = "https://slack.com/api";

/// Slack channel configuration.
#[derive(Debug, Clone)]
pub struct SlackConfig {
    /// Bot token (xoxb-...).
    pub bot_token: String,
    /// App-level token for Socket Mode (xapp-...).
    pub app_token: String,
}

impl SlackConfig {
    /// Create a new configuration.
    pub fn new(bot_token: impl Into<String>, app_token: impl Into<String>) -> Self {
        Self {
            bot_token: bot_token.into(),
            app_token: app_token.into(),
        }
    }
}

/// Slack channel implementation.
pub struct SlackChannel {
    /// Configuration.
    config: SlackConfig,
    /// HTTP client.
    http_client: Client,
    /// Broadcast sender for incoming messages.
    incoming_tx: broadcast::Sender<IncomingMessage>,
    /// Whether connected.
    connected: Arc<AtomicBool>,
    /// Bot user ID.
    bot_user_id: Arc<RwLock<Option<String>>>,
    /// Socket Mode task handle.
    socket_handle: Option<tokio::task::JoinHandle<()>>,
}

impl SlackChannel {
    /// Create a new Slack channel.
    pub fn new(config: SlackConfig) -> Self {
        let (incoming_tx, _) = broadcast::channel(256);
        Self {
            config,
            http_client: Client::new(),
            incoming_tx,
            connected: Arc::new(AtomicBool::new(false)),
            bot_user_id: Arc::new(RwLock::new(None)),
            socket_handle: None,
        }
    }

    /// Get the Socket Mode WebSocket URL.
    async fn get_socket_url(&self) -> Result<String> {
        let url = format!("{}/apps.connections.open", API_BASE_URL);

        let response = self
            .http_client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.config.app_token))
            .header("Content-Type", "application/x-www-form-urlencoded")
            .send()
            .await
            .map_err(|e| drbot_core::Error::Http(e.to_string()))?;

        let api_response: SlackApiResponse<ConnectionsOpenResponse> = response
            .json()
            .await
            .map_err(|e| drbot_core::Error::Http(e.to_string()))?;

        if !api_response.ok {
            return Err(drbot_core::Error::Channel(format!(
                "Slack API error: {}",
                api_response.error.unwrap_or_default()
            )));
        }

        api_response
            .data
            .and_then(|d| d.url)
            .ok_or_else(|| drbot_core::Error::Channel("No WebSocket URL returned".to_string()))
    }

    /// Get bot user info via auth.test.
    async fn get_bot_info(&self) -> Result<AuthTestResponse> {
        let url = format!("{}/auth.test", API_BASE_URL);

        let response = self
            .http_client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.config.bot_token))
            .send()
            .await
            .map_err(|e| drbot_core::Error::Http(e.to_string()))?;

        let api_response: SlackApiResponse<AuthTestResponse> = response
            .json()
            .await
            .map_err(|e| drbot_core::Error::Http(e.to_string()))?;

        if !api_response.ok {
            return Err(drbot_core::Error::Channel(format!(
                "Slack auth.test error: {}",
                api_response.error.unwrap_or_default()
            )));
        }

        api_response
            .data
            .ok_or_else(|| drbot_core::Error::Channel("No auth data returned".to_string()))
    }

    /// Send a message via Web API.
    async fn post_message(&self, channel: &str, text: &str, thread_ts: Option<&str>) -> Result<()> {
        let url = format!("{}/chat.postMessage", API_BASE_URL);

        let request = PostMessageRequest {
            channel: channel.to_string(),
            text: text.to_string(),
            thread_ts: thread_ts.map(|s| s.to_string()),
        };

        let response = self
            .http_client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.config.bot_token))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| drbot_core::Error::Http(e.to_string()))?;

        let api_response: SlackApiResponse<PostMessageResponse> = response
            .json()
            .await
            .map_err(|e| drbot_core::Error::Http(e.to_string()))?;

        if !api_response.ok {
            error!(error = ?api_response.error, "Failed to send Slack message");
            return Err(drbot_core::Error::Channel(format!(
                "Slack chat.postMessage error: {}",
                api_response.error.unwrap_or_default()
            )));
        }

        Ok(())
    }
}

#[async_trait]
impl Channel for SlackChannel {
    async fn connect(&mut self) -> Result<()> {
        info!("Connecting to Slack via Socket Mode");

        // Get bot info
        let auth_info = self.get_bot_info().await?;
        info!(
            user = ?auth_info.user,
            team = ?auth_info.team,
            "Authenticated with Slack"
        );
        *self.bot_user_id.write().await = auth_info.user_id.clone();

        // Get Socket Mode URL
        let socket_url = self.get_socket_url().await?;
        debug!("Got Socket Mode URL");

        // Connect to WebSocket
        let (ws_stream, _) = connect_async(&socket_url)
            .await
            .map_err(|e| drbot_core::Error::WebSocket(e.to_string()))?;

        let (mut write, mut read) = ws_stream.split();

        // Wait for hello message
        let hello_msg = read
            .next()
            .await
            .ok_or_else(|| {
                drbot_core::Error::WebSocket("Connection closed before hello".to_string())
            })?
            .map_err(|e| drbot_core::Error::WebSocket(e.to_string()))?;

        match hello_msg {
            WsMessage::Text(text) => {
                if let Ok(hello) = serde_json::from_str::<HelloMessage>(&text) {
                    if hello.msg_type == "hello" {
                        info!("Received hello from Slack Socket Mode");
                    }
                }
            }
            _ => {
                return Err(drbot_core::Error::WebSocket(
                    "Expected text hello message".to_string(),
                ));
            }
        }

        self.connected.store(true, Ordering::SeqCst);

        // Start event processing task
        let incoming_tx = self.incoming_tx.clone();
        let connected = self.connected.clone();
        let bot_user_id = self.bot_user_id.clone();

        let socket_handle = tokio::spawn(async move {
            while connected.load(Ordering::SeqCst) {
                tokio::select! {
                    msg = read.next() => {
                        match msg {
                            Some(Ok(WsMessage::Text(text))) => {
                                // Try to parse as envelope
                                if let Ok(envelope) = serde_json::from_str::<SlackEnvelope>(&text) {
                                    // Always acknowledge
                                    let ack = Acknowledgment {
                                        envelope_id: envelope.envelope_id.clone(),
                                    };
                                    if let Ok(ack_json) = serde_json::to_string(&ack) {
                                        if write.send(WsMessage::Text(ack_json.into())).await.is_err() {
                                            break;
                                        }
                                    }

                                    // Process event
                                    if envelope.envelope_type == "events_api" {
                                        if let Some(payload) = envelope.payload {
                                            if let Some(event) = payload.event {
                                                match event {
                                                    SlackEvent::Message(msg) | SlackEvent::AppMention(msg) => {
                                                        // Skip bot messages
                                                        if msg.bot_id.is_some() {
                                                            continue;
                                                        }
                                                        if let Some(subtype) = &msg.subtype {
                                                            if subtype == "bot_message" {
                                                                continue;
                                                            }
                                                        }

                                                        // Skip our own messages
                                                        let bot_id = bot_user_id.read().await;
                                                        if let (Some(user), Some(bot)) = (&msg.user, &*bot_id) {
                                                            if user == bot {
                                                                continue;
                                                            }
                                                        }
                                                        drop(bot_id);

                                                        let incoming = IncomingMessage {
                                                            id: Uuid::new_v4(),
                                                            channel_type: "slack".to_string(),
                                                            channel_id: msg.channel.clone(),
                                                            sender: MessageSender {
                                                                id: msg.user.clone().unwrap_or_default(),
                                                                name: None, // Would need users.info call
                                                                username: msg.user.clone(),
                                                            },
                                                            content: vec![Content::Text { text: msg.text.clone() }],
                                                            received_at: chrono::Utc::now(),
                                                            raw: serde_json::to_value(&msg).ok(),
                                                            reply_to: msg.thread_ts.clone(),
                                                        };

                                                        let _ = incoming_tx.send(incoming);
                                                    }
                                                    SlackEvent::Unknown => {}
                                                }
                                            }
                                        }
                                    }
                                } else if let Ok(hello) = serde_json::from_str::<HelloMessage>(&text) {
                                    if hello.msg_type == "disconnect" {
                                        warn!("Received disconnect from Slack");
                                        break;
                                    }
                                }
                            }
                            Some(Ok(WsMessage::Ping(data))) => {
                                if write.send(WsMessage::Pong(data)).await.is_err() {
                                    break;
                                }
                            }
                            Some(Ok(WsMessage::Close(_))) | None => {
                                break;
                            }
                            Some(Err(e)) => {
                                error!(error = %e, "Slack WebSocket error");
                                break;
                            }
                            _ => {}
                        }
                    }
                }
            }

            connected.store(false, Ordering::SeqCst);
            info!("Slack Socket Mode disconnected");
        });

        self.socket_handle = Some(socket_handle);
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

        self.post_message(to, &text, message.reply_to.as_deref())
            .await
    }

    fn subscribe(&self) -> broadcast::Receiver<IncomingMessage> {
        self.incoming_tx.subscribe()
    }

    async fn disconnect(&mut self) -> Result<()> {
        info!("Disconnecting from Slack");

        self.connected.store(false, Ordering::SeqCst);

        if let Some(handle) = self.socket_handle.take() {
            handle.abort();
        }

        Ok(())
    }

    fn channel_type(&self) -> &str {
        "slack"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_creation() {
        let config = SlackConfig::new("xoxb-token", "xapp-token");
        assert_eq!(config.bot_token, "xoxb-token");
        assert_eq!(config.app_token, "xapp-token");
    }

    #[test]
    fn test_channel_creation() {
        let channel = SlackChannel::new(SlackConfig::new("xoxb-test", "xapp-test"));
        assert_eq!(channel.channel_type(), "slack");
    }

    #[test]
    fn test_user_display_name() {
        let user = SlackUser {
            id: "U123".to_string(),
            team_id: Some("T123".to_string()),
            name: "testuser".to_string(),
            real_name: Some("Test User".to_string()),
            profile: Some(UserProfile {
                display_name: Some("Display Name".to_string()),
                real_name: Some("Test User".to_string()),
                email: None,
            }),
            is_bot: false,
        };
        assert_eq!(user.display_name(), "Display Name");

        let user2 = SlackUser {
            id: "U456".to_string(),
            team_id: None,
            name: "anotheruser".to_string(),
            real_name: None,
            profile: None,
            is_bot: false,
        };
        assert_eq!(user2.display_name(), "anotheruser");
    }
}
