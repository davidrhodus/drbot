//! Bot Framework integration for Microsoft Teams.

use crate::{AuthConfig, AzureAuth, MsTeamsError, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Activity type for Bot Framework.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ActivityType {
    /// User sent a message.
    Message,
    /// User is typing.
    Typing,
    /// Conversation update (member added/removed).
    ConversationUpdate,
    /// Message reaction.
    MessageReaction,
    /// Installation update.
    InstallationUpdate,
    /// Invoke (e.g., messaging extension).
    Invoke,
    /// Event.
    Event,
    /// End of conversation.
    EndOfConversation,
    /// Other activity type.
    #[serde(other)]
    Other,
}

impl Default for ActivityType {
    fn default() -> Self {
        Self::Message
    }
}

/// Channel data for Teams-specific information.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TeamsChannelData {
    /// Team info.
    pub team: Option<TeamInfo>,
    /// Channel info.
    pub channel: Option<ChannelInfo>,
    /// Tenant info.
    pub tenant: Option<TenantInfo>,
}

/// Team information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamInfo {
    /// Team ID.
    pub id: String,
    /// Team name.
    pub name: Option<String>,
}

/// Channel information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelInfo {
    /// Channel ID.
    pub id: String,
    /// Channel name.
    pub name: Option<String>,
}

/// Tenant information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantInfo {
    /// Tenant ID.
    pub id: String,
}

/// Bot Framework activity (incoming/outgoing message).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Activity {
    /// Activity type.
    #[serde(rename = "type")]
    pub activity_type: ActivityType,
    /// Activity ID.
    pub id: Option<String>,
    /// Timestamp.
    pub timestamp: Option<DateTime<Utc>>,
    /// Local timestamp.
    pub local_timestamp: Option<DateTime<Utc>>,
    /// Service URL.
    pub service_url: Option<String>,
    /// Channel ID.
    pub channel_id: Option<String>,
    /// From account.
    pub from: Option<ChannelAccount>,
    /// Conversation info.
    pub conversation: Option<ConversationAccount>,
    /// Recipient account.
    pub recipient: Option<ChannelAccount>,
    /// Text content.
    pub text: Option<String>,
    /// Text format.
    pub text_format: Option<String>,
    /// Attachments.
    #[serde(default)]
    pub attachments: Vec<Attachment>,
    /// Reply-to ID.
    pub reply_to_id: Option<String>,
    /// Value (for invoke activities).
    pub value: Option<serde_json::Value>,
    /// Teams-specific channel data.
    pub channel_data: Option<TeamsChannelData>,
}

impl Activity {
    /// Create a new message activity.
    pub fn message(text: &str) -> Self {
        Self {
            activity_type: ActivityType::Message,
            id: None,
            timestamp: Some(Utc::now()),
            local_timestamp: None,
            service_url: None,
            channel_id: None,
            from: None,
            conversation: None,
            recipient: None,
            text: Some(text.to_string()),
            text_format: Some("plain".to_string()),
            attachments: Vec::new(),
            reply_to_id: None,
            value: None,
            channel_data: None,
        }
    }

    /// Create a typing indicator activity.
    pub fn typing() -> Self {
        Self {
            activity_type: ActivityType::Typing,
            ..Default::default()
        }
    }

    /// Set reply-to ID.
    pub fn with_reply_to(mut self, reply_to_id: &str) -> Self {
        self.reply_to_id = Some(reply_to_id.to_string());
        self
    }

    /// Add an attachment.
    pub fn with_attachment(mut self, attachment: Attachment) -> Self {
        self.attachments.push(attachment);
        self
    }

    /// Get the sender's user ID.
    pub fn sender_id(&self) -> Option<&str> {
        self.from.as_ref().map(|f| f.id.as_str())
    }

    /// Get the sender's name.
    pub fn sender_name(&self) -> Option<&str> {
        self.from.as_ref().and_then(|f| f.name.as_deref())
    }

    /// Check if this is a message activity.
    pub fn is_message(&self) -> bool {
        self.activity_type == ActivityType::Message
    }
}

impl Default for Activity {
    fn default() -> Self {
        Self {
            activity_type: ActivityType::Message,
            id: None,
            timestamp: None,
            local_timestamp: None,
            service_url: None,
            channel_id: None,
            from: None,
            conversation: None,
            recipient: None,
            text: None,
            text_format: None,
            attachments: Vec::new(),
            reply_to_id: None,
            value: None,
            channel_data: None,
        }
    }
}

/// Channel account (user/bot).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelAccount {
    /// Account ID.
    pub id: String,
    /// Account name.
    pub name: Option<String>,
    /// AAD object ID.
    pub aad_object_id: Option<String>,
    /// Role.
    pub role: Option<String>,
}

impl ChannelAccount {
    /// Create a new channel account.
    pub fn new(id: &str) -> Self {
        Self {
            id: id.to_string(),
            name: None,
            aad_object_id: None,
            role: None,
        }
    }

    /// Set the name.
    pub fn with_name(mut self, name: &str) -> Self {
        self.name = Some(name.to_string());
        self
    }
}

/// Conversation account.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationAccount {
    /// Conversation ID.
    pub id: String,
    /// Is group conversation.
    pub is_group: Option<bool>,
    /// Conversation type.
    pub conversation_type: Option<String>,
    /// Tenant ID.
    pub tenant_id: Option<String>,
    /// Conversation name.
    pub name: Option<String>,
}

/// Activity attachment.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Attachment {
    /// Content type (MIME).
    pub content_type: String,
    /// Content URL.
    pub content_url: Option<String>,
    /// Content (inline).
    pub content: Option<serde_json::Value>,
    /// Name.
    pub name: Option<String>,
}

impl Attachment {
    /// Create an adaptive card attachment.
    pub fn adaptive_card(content: serde_json::Value) -> Self {
        Self {
            content_type: "application/vnd.microsoft.card.adaptive".to_string(),
            content_url: None,
            content: Some(content),
            name: None,
        }
    }

    /// Create an image attachment.
    pub fn image(url: &str, name: Option<&str>) -> Self {
        Self {
            content_type: "image/png".to_string(),
            content_url: Some(url.to_string()),
            content: None,
            name: name.map(String::from),
        }
    }
}

/// Conversation reference for proactive messaging.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationReference {
    /// Activity ID.
    pub activity_id: Option<String>,
    /// User account.
    pub user: Option<ChannelAccount>,
    /// Bot account.
    pub bot: Option<ChannelAccount>,
    /// Conversation.
    pub conversation: Option<ConversationAccount>,
    /// Channel ID.
    pub channel_id: Option<String>,
    /// Service URL.
    pub service_url: Option<String>,
}

impl ConversationReference {
    /// Create from an incoming activity.
    pub fn from_activity(activity: &Activity, bot_id: &str) -> Self {
        Self {
            activity_id: activity.id.clone(),
            user: activity.from.clone(),
            bot: Some(ChannelAccount::new(bot_id)),
            conversation: activity.conversation.clone(),
            channel_id: activity.channel_id.clone(),
            service_url: activity.service_url.clone(),
        }
    }
}

/// Bot Framework client.
pub struct BotFramework {
    auth: Arc<AzureAuth>,
    client: reqwest::Client,
    bot_app_id: String,
}

impl BotFramework {
    /// Create a new Bot Framework client.
    pub fn new(client_id: &str, client_secret: &str) -> Self {
        let auth_config = AuthConfig::for_bot_framework(client_id, client_secret);
        let auth = Arc::new(AzureAuth::new(auth_config));

        Self {
            auth,
            client: reqwest::Client::new(),
            bot_app_id: client_id.to_string(),
        }
    }

    /// Send an activity (reply or proactive).
    pub async fn send_activity(
        &self,
        service_url: &str,
        conversation_id: &str,
        activity: Activity,
    ) -> Result<String> {
        let token = self.auth.get_token().await?;
        let url = format!(
            "{}/v3/conversations/{}/activities",
            service_url.trim_end_matches('/'),
            conversation_id
        );

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", token))
            .header("Content-Type", "application/json")
            .json(&activity)
            .send()
            .await
            .map_err(|e| MsTeamsError::HttpError(e.to_string()))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(MsTeamsError::BotFrameworkError(error_text));
        }

        #[derive(Deserialize)]
        struct Response {
            id: String,
        }

        let result: Response = response
            .json()
            .await
            .map_err(|e| MsTeamsError::BotFrameworkError(e.to_string()))?;

        Ok(result.id)
    }

    /// Reply to an activity.
    pub async fn reply_to_activity(
        &self,
        service_url: &str,
        conversation_id: &str,
        activity_id: &str,
        activity: Activity,
    ) -> Result<String> {
        let token = self.auth.get_token().await?;
        let url = format!(
            "{}/v3/conversations/{}/activities/{}",
            service_url.trim_end_matches('/'),
            conversation_id,
            activity_id
        );

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", token))
            .header("Content-Type", "application/json")
            .json(&activity)
            .send()
            .await
            .map_err(|e| MsTeamsError::HttpError(e.to_string()))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(MsTeamsError::BotFrameworkError(error_text));
        }

        #[derive(Deserialize)]
        struct Response {
            id: String,
        }

        let result: Response = response
            .json()
            .await
            .map_err(|e| MsTeamsError::BotFrameworkError(e.to_string()))?;

        Ok(result.id)
    }

    /// Send a proactive message using a conversation reference.
    pub async fn send_proactive(
        &self,
        reference: &ConversationReference,
        activity: Activity,
    ) -> Result<String> {
        let service_url = reference
            .service_url
            .as_ref()
            .ok_or_else(|| MsTeamsError::InvalidConfig("Missing service URL".into()))?;

        let conversation_id = reference
            .conversation
            .as_ref()
            .map(|c| &c.id)
            .ok_or_else(|| MsTeamsError::InvalidConfig("Missing conversation ID".into()))?;

        self.send_activity(service_url, conversation_id, activity)
            .await
    }

    /// Get the bot app ID.
    pub fn bot_app_id(&self) -> &str {
        &self.bot_app_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_activity() {
        let activity = Activity::message("Hello, Teams!");
        assert!(activity.is_message());
        assert_eq!(activity.text, Some("Hello, Teams!".to_string()));
    }

    #[test]
    fn test_attachment() {
        let card = Attachment::adaptive_card(serde_json::json!({"type": "AdaptiveCard"}));
        assert!(card.content_type.contains("adaptive"));
        assert!(card.content.is_some());
    }
}
