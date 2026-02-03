//! Telegram Bot API channel for drbot.
//!
//! This crate provides a Telegram channel implementation using long polling.

mod api;
mod client;

pub use api::*;
pub use client::TelegramClient;

use async_trait::async_trait;
use drbot_channels::Channel;
use drbot_core::message::{Content, IncomingMessage, MessageSender, OutgoingMessage};
use drbot_core::Result;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Telegram channel configuration.
#[derive(Debug, Clone)]
pub struct TelegramConfig {
    /// Bot token from @BotFather.
    pub token: String,
    /// Polling timeout in seconds.
    pub polling_timeout: i32,
    /// Allowed update types.
    pub allowed_updates: Vec<String>,
}

impl TelegramConfig {
    /// Create a new configuration with a bot token.
    pub fn new(token: impl Into<String>) -> Self {
        Self {
            token: token.into(),
            polling_timeout: 30,
            allowed_updates: vec!["message".to_string(), "edited_message".to_string()],
        }
    }

    /// Set polling timeout.
    pub fn with_polling_timeout(mut self, timeout: i32) -> Self {
        self.polling_timeout = timeout;
        self
    }
}

/// Telegram channel implementation.
pub struct TelegramChannel {
    /// Configuration.
    config: TelegramConfig,
    /// API client.
    client: TelegramClient,
    /// Message sender for incoming messages.
    sender: broadcast::Sender<IncomingMessage>,
    /// Whether the channel is running.
    running: Arc<AtomicBool>,
    /// Polling task handle.
    poll_handle: Option<tokio::task::JoinHandle<()>>,
    /// Bot info.
    bot_info: Option<User>,
}

impl TelegramChannel {
    /// Create a new Telegram channel.
    pub fn new(config: TelegramConfig) -> Self {
        let (sender, _) = broadcast::channel(256);
        Self {
            client: TelegramClient::new(&config.token),
            config,
            sender,
            running: Arc::new(AtomicBool::new(false)),
            poll_handle: None,
            bot_info: None,
        }
    }

    /// Create from a bot token.
    pub fn from_token(token: impl Into<String>) -> Self {
        Self::new(TelegramConfig::new(token))
    }

    /// Get the bot information.
    pub fn bot_info(&self) -> Option<&User> {
        self.bot_info.as_ref()
    }

    /// Convert a Telegram message to an IncomingMessage.
    fn convert_message(msg: &TelegramMessage) -> Option<IncomingMessage> {
        let sender = msg.from.as_ref()?;

        let mut content = Vec::new();

        // Add text content
        if let Some(text) = &msg.text {
            content.push(Content::Text { text: text.clone() });
        }

        // Add caption as text if present
        if let Some(caption) = &msg.caption {
            if msg.text.is_none() {
                content.push(Content::Text {
                    text: caption.clone(),
                });
            }
        }

        // Skip if no content
        if content.is_empty() {
            return None;
        }

        Some(IncomingMessage {
            id: Uuid::new_v4(),
            channel_type: "telegram".to_string(),
            channel_id: msg.chat.id.to_string(),
            sender: MessageSender {
                id: sender.id.to_string(),
                name: Some(sender.full_name()),
                username: sender.username.clone(),
            },
            content,
            received_at: chrono::Utc::now(),
            raw: serde_json::to_value(msg).ok(),
            reply_to: msg
                .reply_to_message
                .as_ref()
                .map(|m| m.message_id.to_string()),
        })
    }

    /// Start the polling loop.
    async fn start_polling(&mut self) -> Result<()> {
        let client = self.client.clone();
        let sender = self.sender.clone();
        let running = self.running.clone();
        let timeout = self.config.polling_timeout;
        let allowed_updates = self.config.allowed_updates.clone();

        // Delete any existing webhook
        if let Err(e) = client.delete_webhook().await {
            warn!(error = %e, "Failed to delete webhook, continuing anyway");
        }

        let handle = tokio::spawn(async move {
            let mut offset: Option<i64> = None;

            while running.load(Ordering::SeqCst) {
                let request = GetUpdatesRequest {
                    offset,
                    limit: Some(100),
                    timeout: Some(timeout),
                    allowed_updates: Some(allowed_updates.clone()),
                };

                match client.get_updates(&request).await {
                    Ok(updates) => {
                        for update in updates {
                            // Update offset for next request
                            offset = Some(update.update_id + 1);

                            // Process message
                            if let Some(msg) = update.message.or(update.edited_message) {
                                debug!(
                                    message_id = msg.message_id,
                                    chat_id = msg.chat.id,
                                    "Received message"
                                );

                                if let Some(incoming) = Self::convert_message(&msg) {
                                    if sender.send(incoming).is_err() {
                                        // No receivers, but that's okay
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        error!(error = %e, "Failed to get updates");
                        // Wait before retrying
                        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                    }
                }
            }

            info!("Polling loop stopped");
        });

        self.poll_handle = Some(handle);
        Ok(())
    }
}

#[async_trait]
impl Channel for TelegramChannel {
    async fn connect(&mut self) -> Result<()> {
        info!("Connecting to Telegram");

        // Get bot info
        let bot = self.client.get_me().await?;
        info!(
            bot_id = bot.id,
            username = ?bot.username,
            "Connected as @{}",
            bot.username.as_deref().unwrap_or("unknown")
        );
        self.bot_info = Some(bot);

        // Start polling
        self.running.store(true, Ordering::SeqCst);
        self.start_polling().await?;

        Ok(())
    }

    async fn send(&self, to: &str, message: OutgoingMessage) -> Result<()> {
        let chat_id: i64 = to
            .parse()
            .map_err(|_| drbot_core::Error::Internal(format!("Invalid chat ID: {}", to)))?;

        // Get text content
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
            return Err(drbot_core::Error::Internal(
                "No text content in message".to_string(),
            ));
        }

        let request = SendMessageRequest {
            chat_id,
            text,
            parse_mode: None,
            reply_to_message_id: message.reply_to.and_then(|r| r.parse().ok()),
            disable_web_page_preview: None,
        };

        self.client.send_message(&request).await?;
        Ok(())
    }

    fn subscribe(&self) -> broadcast::Receiver<IncomingMessage> {
        self.sender.subscribe()
    }

    async fn disconnect(&mut self) -> Result<()> {
        info!("Disconnecting from Telegram");

        self.running.store(false, Ordering::SeqCst);

        if let Some(handle) = self.poll_handle.take() {
            // Give the polling loop time to notice the stop signal
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            handle.abort();
        }

        Ok(())
    }

    fn channel_type(&self) -> &str {
        "telegram"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_builder() {
        let config = TelegramConfig::new("test_token").with_polling_timeout(60);

        assert_eq!(config.token, "test_token");
        assert_eq!(config.polling_timeout, 60);
    }

    #[test]
    fn test_channel_creation() {
        let channel = TelegramChannel::from_token("test_token");
        assert_eq!(channel.channel_type(), "telegram");
    }
}
