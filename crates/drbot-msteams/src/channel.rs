//! Microsoft Teams Channel implementation.

use crate::{
    Activity, ActivityType, BotFramework, ConversationReference, GraphApi, MsTeamsConfig,
    MsTeamsError, Result,
};
use async_trait::async_trait;
use drbot_channels::{Channel, ChannelEvent, IncomingMessage, OutgoingMessage};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tracing::{debug, error, info};

/// Microsoft Teams channel implementation.
pub struct MsTeamsChannel {
    /// Configuration.
    config: MsTeamsConfig,
    /// Graph API client.
    graph_api: GraphApi,
    /// Bot Framework client.
    bot_framework: BotFramework,
    /// Conversation references for proactive messaging.
    conversation_refs: Arc<RwLock<HashMap<String, ConversationReference>>>,
    /// Event sender.
    event_tx: broadcast::Sender<ChannelEvent>,
    /// Connected state.
    connected: Arc<RwLock<bool>>,
}

impl MsTeamsChannel {
    /// Create a new Microsoft Teams channel.
    pub async fn new(config: MsTeamsConfig) -> Result<Self> {
        if config.tenant_id.is_empty() {
            return Err(MsTeamsError::InvalidConfig("Missing tenant_id".into()));
        }
        if config.client_id.is_empty() {
            return Err(MsTeamsError::InvalidConfig("Missing client_id".into()));
        }
        if config.client_secret.is_empty() {
            return Err(MsTeamsError::InvalidConfig("Missing client_secret".into()));
        }

        let graph_api = GraphApi::new(&config.tenant_id, &config.client_id, &config.client_secret);

        let bot_framework = BotFramework::new(&config.client_id, &config.client_secret);

        let (event_tx, _) = broadcast::channel(256);

        Ok(Self {
            config,
            graph_api,
            bot_framework,
            conversation_refs: Arc::new(RwLock::new(HashMap::new())),
            event_tx,
            connected: Arc::new(RwLock::new(false)),
        })
    }

    /// Handle an incoming Bot Framework activity.
    pub async fn handle_activity(&self, activity: Activity) -> Result<Option<String>> {
        if activity.activity_type != ActivityType::Message {
            debug!(
                activity_type = ?activity.activity_type,
                "Ignoring non-message activity"
            );
            return Ok(None);
        }

        let Some(text) = &activity.text else {
            return Ok(None);
        };

        // Check allowed teams/channels
        if !self.is_allowed(&activity) {
            debug!("Activity from non-allowed team/channel");
            return Ok(None);
        }

        // Store conversation reference for proactive messaging
        if self.config.enable_proactive {
            if let Some(conv) = &activity.conversation {
                let reference =
                    ConversationReference::from_activity(&activity, &self.config.bot_app_id);
                let mut refs = self.conversation_refs.write().await;
                refs.insert(conv.id.clone(), reference);
            }
        }

        // Convert to incoming message
        let sender_id = activity
            .from
            .as_ref()
            .map(|f| f.id.clone())
            .unwrap_or_default();
        let sender_name = activity
            .from
            .as_ref()
            .and_then(|f| f.name.clone())
            .unwrap_or_default();
        let conversation_id = activity
            .conversation
            .as_ref()
            .map(|c| c.id.clone())
            .unwrap_or_default();

        let message = IncomingMessage {
            id: activity.id.clone().unwrap_or_default(),
            channel_type: "msteams".to_string(),
            sender_id,
            sender_name,
            conversation_id: conversation_id.clone(),
            text: text.clone(),
            timestamp: activity.timestamp.unwrap_or_else(chrono::Utc::now),
            reply_to: activity.reply_to_id.clone(),
            metadata: HashMap::new(),
        };

        // Broadcast the event
        let _ = self.event_tx.send(ChannelEvent::MessageReceived(message));

        info!(
            conversation_id = %conversation_id,
            "Received Teams message"
        );

        Ok(activity.id)
    }

    /// Send a proactive message to a conversation.
    pub async fn send_proactive(&self, conversation_id: &str, text: &str) -> Result<String> {
        let refs = self.conversation_refs.read().await;
        let reference = refs
            .get(conversation_id)
            .ok_or_else(|| MsTeamsError::ChannelNotFound(conversation_id.to_string()))?
            .clone();
        drop(refs);

        let activity = Activity::message(text);
        self.bot_framework
            .send_proactive(&reference, activity)
            .await
    }

    fn is_allowed(&self, activity: &Activity) -> bool {
        // If no restrictions, allow all
        if self.config.allowed_teams.is_empty() && self.config.allowed_channels.is_empty() {
            return true;
        }

        // Check team
        if !self.config.allowed_teams.is_empty() {
            if let Some(channel_data) = &activity.channel_data {
                if let Some(team) = &channel_data.team {
                    if !self.config.allowed_teams.contains(&team.id) {
                        return false;
                    }
                }
            }
        }

        // Check channel
        if !self.config.allowed_channels.is_empty() {
            if let Some(channel_data) = &activity.channel_data {
                if let Some(channel) = &channel_data.channel {
                    if !self.config.allowed_channels.contains(&channel.id) {
                        return false;
                    }
                }
            }
        }

        true
    }

    /// Get the Graph API client.
    pub fn graph_api(&self) -> &GraphApi {
        &self.graph_api
    }

    /// Get the Bot Framework client.
    pub fn bot_framework(&self) -> &BotFramework {
        &self.bot_framework
    }
}

#[async_trait]
impl Channel for MsTeamsChannel {
    async fn connect(&mut self) -> drbot_core::Result<()> {
        // Verify credentials by making a test API call
        match self.graph_api.list_teams().await {
            Ok(teams) => {
                info!(team_count = teams.len(), "Connected to Microsoft Teams");
                let mut connected = self.connected.write().await;
                *connected = true;
                let _ = self.event_tx.send(ChannelEvent::Connected);
                Ok(())
            }
            Err(e) => {
                error!(error = %e, "Failed to connect to Microsoft Teams");
                Err(drbot_core::Error::Channel(e.to_string()))
            }
        }
    }

    async fn disconnect(&mut self) -> drbot_core::Result<()> {
        let mut connected = self.connected.write().await;
        *connected = false;
        let _ = self.event_tx.send(ChannelEvent::Disconnected);
        info!("Disconnected from Microsoft Teams");
        Ok(())
    }

    async fn send(&self, to: &str, message: OutgoingMessage) -> drbot_core::Result<()> {
        // Try to find conversation reference first (for proactive)
        let refs = self.conversation_refs.read().await;
        if let Some(reference) = refs.get(to) {
            let reference = reference.clone();
            drop(refs);

            let activity = Activity::message(&message.text);
            self.bot_framework
                .send_proactive(&reference, activity)
                .await
                .map_err(|e| drbot_core::Error::Channel(e.to_string()))?;

            return Ok(());
        }
        drop(refs);

        // Otherwise, try Graph API (requires team_id:channel_id format)
        let parts: Vec<&str> = to.split(':').collect();
        if parts.len() == 2 {
            let body = crate::api::MessageBody::text(&message.text);
            self.graph_api
                .send_message(parts[0], parts[1], body)
                .await
                .map_err(|e| drbot_core::Error::Channel(e.to_string()))?;

            return Ok(());
        }

        Err(drbot_core::Error::Channel(format!(
            "Invalid destination format: {}",
            to
        )))
    }

    fn subscribe(&self) -> broadcast::Receiver<ChannelEvent> {
        self.event_tx.subscribe()
    }

    fn channel_type(&self) -> &str {
        "msteams"
    }

    async fn is_connected(&self) -> bool {
        *self.connected.read().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_channel_config() {
        let config = MsTeamsConfig {
            tenant_id: "test-tenant".to_string(),
            client_id: "test-client".to_string(),
            client_secret: "test-secret".to_string(),
            bot_app_id: "test-bot".to_string(),
            ..Default::default()
        };

        assert!(!config.enable_proactive);
        assert!(config.enable_notifications);
    }
}
