//! Channel implementation for Google Chat.

use crate::{AuthConfig, GoogleChatApi, GoogleChatAuth, GoogleChatConfig, GoogleChatError, Result};
use async_trait::async_trait;
use drbot_channels::Channel;
use drbot_core::message::{IncomingMessage, OutgoingMessage};
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};

/// Google Chat channel implementation.
pub struct GoogleChatChannel {
    /// Configuration.
    config: GoogleChatConfig,
    /// API client.
    api: Arc<RwLock<GoogleChatApi>>,
    /// Auth handler.
    auth: Arc<RwLock<GoogleChatAuth>>,
    /// Message sender.
    message_tx: broadcast::Sender<IncomingMessage>,
    /// Connected state.
    connected: Arc<RwLock<bool>>,
}

impl GoogleChatChannel {
    /// Create a new Google Chat channel.
    pub fn new(config: GoogleChatConfig) -> Result<Self> {
        let auth_config = AuthConfig {
            key_file: config.credentials_path.clone(),
            key_json: config.credentials_json.clone(),
            ..Default::default()
        };

        let auth = GoogleChatAuth::new(auth_config)?;
        let api = GoogleChatApi::new();

        let (message_tx, _) = broadcast::channel(256);

        Ok(Self {
            config,
            api: Arc::new(RwLock::new(api)),
            auth: Arc::new(RwLock::new(auth)),
            message_tx,
            connected: Arc::new(RwLock::new(false)),
        })
    }

    /// Check if a space is allowed.
    fn is_space_allowed(&self, space_name: &str) -> bool {
        if self.config.allowed_spaces.is_empty() {
            true
        } else {
            self.config.allowed_spaces.iter().any(|s| s == space_name)
        }
    }

    /// Get the API client with a valid token.
    async fn get_api(&self) -> Result<GoogleChatApi> {
        let mut auth = self.auth.write().await;
        let token = auth.get_token().await?;

        let api = self.api.read().await;
        Ok(GoogleChatApi::new().with_token(&token))
    }
}

#[async_trait]
impl Channel for GoogleChatChannel {
    async fn connect(&mut self) -> drbot_core::Result<()> {
        // Authenticate
        {
            let mut auth = self.auth.write().await;
            auth.get_token()
                .await
                .map_err(|e| drbot_core::Error::Config(e.to_string()))?;
        }

        let mut connected = self.connected.write().await;
        *connected = true;

        tracing::info!("Google Chat channel connected");
        Ok(())
    }

    async fn send(&self, to: &str, message: OutgoingMessage) -> drbot_core::Result<()> {
        if !self.is_space_allowed(to) {
            return Err(drbot_core::Error::Config(format!(
                "Space not allowed: {}",
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
        let api = self
            .get_api()
            .await
            .map_err(|e| drbot_core::Error::Config(e.to_string()))?;

        api.send_message(to, &text)
            .await
            .map_err(|e| drbot_core::Error::Io(std::io::Error::other(e.to_string())))?;

        Ok(())
    }

    fn subscribe(&self) -> broadcast::Receiver<IncomingMessage> {
        self.message_tx.subscribe()
    }

    async fn disconnect(&mut self) -> drbot_core::Result<()> {
        let mut connected = self.connected.write().await;
        *connected = false;

        tracing::info!("Google Chat channel disconnected");
        Ok(())
    }

    fn channel_type(&self) -> &str {
        "googlechat"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_google_chat_channel_space_allowed() {
        let config = GoogleChatConfig {
            allowed_spaces: vec!["spaces/AAAA".to_string()],
            ..Default::default()
        };
        let channel = GoogleChatChannel::new(config).unwrap();

        assert!(channel.is_space_allowed("spaces/AAAA"));
        assert!(!channel.is_space_allowed("spaces/BBBB"));
    }

    #[test]
    fn test_google_chat_channel_all_spaces_allowed() {
        let config = GoogleChatConfig::default();
        let channel = GoogleChatChannel::new(config).unwrap();

        assert!(channel.is_space_allowed("spaces/AAAA"));
        assert!(channel.is_space_allowed("spaces/BBBB"));
    }
}
