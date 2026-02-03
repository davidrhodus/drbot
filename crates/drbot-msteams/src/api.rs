//! Microsoft Graph API types and client.

use crate::{AuthConfig, AzureAuth, MsTeamsError, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// A Microsoft Teams team.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Team {
    /// Team ID.
    pub id: String,
    /// Display name.
    pub display_name: String,
    /// Description.
    pub description: Option<String>,
    /// Internal ID.
    pub internal_id: Option<String>,
    /// Web URL.
    pub web_url: Option<String>,
}

/// A Microsoft Teams channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamsChannel {
    /// Channel ID.
    pub id: String,
    /// Display name.
    pub display_name: String,
    /// Description.
    pub description: Option<String>,
    /// Web URL.
    pub web_url: Option<String>,
    /// Email address (if enabled).
    pub email: Option<String>,
    /// Membership type.
    pub membership_type: Option<String>,
}

/// A Teams team member.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamsMember {
    /// Member ID.
    pub id: String,
    /// Display name.
    pub display_name: Option<String>,
    /// Email.
    pub email: Option<String>,
    /// User ID.
    pub user_id: Option<String>,
    /// Roles.
    #[serde(default)]
    pub roles: Vec<String>,
}

/// A Teams message.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamsMessage {
    /// Message ID.
    pub id: String,
    /// Message body.
    pub body: MessageBody,
    /// From user.
    pub from: Option<MessageFrom>,
    /// Created datetime.
    pub created_date_time: Option<DateTime<Utc>>,
    /// Last modified datetime.
    pub last_modified_date_time: Option<DateTime<Utc>>,
    /// Reply-to message ID.
    pub reply_to_id: Option<String>,
    /// Web URL.
    pub web_url: Option<String>,
    /// Attachments.
    #[serde(default)]
    pub attachments: Vec<MessageAttachment>,
    /// Mentions.
    #[serde(default)]
    pub mentions: Vec<MessageMention>,
}

/// Message body content.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageBody {
    /// Content type (text or html).
    pub content_type: String,
    /// Content.
    pub content: String,
}

impl MessageBody {
    /// Create a text message body.
    pub fn text(content: &str) -> Self {
        Self {
            content_type: "text".to_string(),
            content: content.to_string(),
        }
    }

    /// Create an HTML message body.
    pub fn html(content: &str) -> Self {
        Self {
            content_type: "html".to_string(),
            content: content.to_string(),
        }
    }
}

/// Message sender information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageFrom {
    /// User info.
    pub user: Option<MessageUser>,
}

/// User information in a message.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageUser {
    /// User ID.
    pub id: Option<String>,
    /// Display name.
    pub display_name: Option<String>,
    /// User identity type.
    pub user_identity_type: Option<String>,
}

/// Message attachment.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageAttachment {
    /// Attachment ID.
    pub id: String,
    /// Content type.
    pub content_type: String,
    /// Content URL.
    pub content_url: Option<String>,
    /// Name.
    pub name: Option<String>,
}

/// Message mention.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageMention {
    /// Mention ID.
    pub id: i32,
    /// Mention text.
    pub mention_text: String,
    /// Mentioned user.
    pub mentioned: Option<MentionedUser>,
}

/// Mentioned user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MentionedUser {
    /// User info.
    pub user: Option<MessageUser>,
}

/// Microsoft Graph API client for Teams.
pub struct GraphApi {
    auth: Arc<AzureAuth>,
    client: reqwest::Client,
    base_url: String,
}

impl GraphApi {
    /// Create a new Graph API client.
    pub fn new(tenant_id: &str, client_id: &str, client_secret: &str) -> Self {
        let auth_config = AuthConfig::for_graph(tenant_id, client_id, client_secret);
        let auth = Arc::new(AzureAuth::new(auth_config));

        Self {
            auth,
            client: reqwest::Client::new(),
            base_url: "https://graph.microsoft.com/v1.0".to_string(),
        }
    }

    /// Create with existing auth.
    pub fn with_auth(auth: Arc<AzureAuth>) -> Self {
        Self {
            auth,
            client: reqwest::Client::new(),
            base_url: "https://graph.microsoft.com/v1.0".to_string(),
        }
    }

    /// Make an authenticated GET request.
    async fn get<T: for<'de> Deserialize<'de>>(&self, path: &str) -> Result<T> {
        let token = self.auth.get_token().await?;
        let url = format!("{}{}", self.base_url, path);

        let response = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", token))
            .send()
            .await
            .map_err(|e| MsTeamsError::HttpError(e.to_string()))?;

        if response.status() == 429 {
            return Err(MsTeamsError::RateLimitExceeded);
        }

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(MsTeamsError::ApiError(error_text));
        }

        response
            .json()
            .await
            .map_err(|e| MsTeamsError::ApiError(e.to_string()))
    }

    /// Make an authenticated POST request.
    async fn post<T: for<'de> Deserialize<'de>, B: Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T> {
        let token = self.auth.get_token().await?;
        let url = format!("{}{}", self.base_url, path);

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", token))
            .header("Content-Type", "application/json")
            .json(body)
            .send()
            .await
            .map_err(|e| MsTeamsError::HttpError(e.to_string()))?;

        if response.status() == 429 {
            return Err(MsTeamsError::RateLimitExceeded);
        }

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(MsTeamsError::ApiError(error_text));
        }

        response
            .json()
            .await
            .map_err(|e| MsTeamsError::ApiError(e.to_string()))
    }

    /// List joined teams.
    pub async fn list_teams(&self) -> Result<Vec<Team>> {
        #[derive(Deserialize)]
        struct Response {
            value: Vec<Team>,
        }

        let response: Response = self.get("/me/joinedTeams").await?;
        Ok(response.value)
    }

    /// Get a team by ID.
    pub async fn get_team(&self, team_id: &str) -> Result<Team> {
        self.get(&format!("/teams/{}", team_id)).await
    }

    /// List channels in a team.
    pub async fn list_channels(&self, team_id: &str) -> Result<Vec<TeamsChannel>> {
        #[derive(Deserialize)]
        struct Response {
            value: Vec<TeamsChannel>,
        }

        let response: Response = self.get(&format!("/teams/{}/channels", team_id)).await?;
        Ok(response.value)
    }

    /// Get a channel by ID.
    pub async fn get_channel(&self, team_id: &str, channel_id: &str) -> Result<TeamsChannel> {
        self.get(&format!("/teams/{}/channels/{}", team_id, channel_id))
            .await
    }

    /// List messages in a channel.
    pub async fn list_messages(
        &self,
        team_id: &str,
        channel_id: &str,
    ) -> Result<Vec<TeamsMessage>> {
        #[derive(Deserialize)]
        struct Response {
            value: Vec<TeamsMessage>,
        }

        let response: Response = self
            .get(&format!(
                "/teams/{}/channels/{}/messages",
                team_id, channel_id
            ))
            .await?;
        Ok(response.value)
    }

    /// Send a message to a channel.
    pub async fn send_message(
        &self,
        team_id: &str,
        channel_id: &str,
        body: MessageBody,
    ) -> Result<TeamsMessage> {
        #[derive(Serialize)]
        struct SendRequest {
            body: MessageBody,
        }

        let request = SendRequest { body };

        self.post(
            &format!("/teams/{}/channels/{}/messages", team_id, channel_id),
            &request,
        )
        .await
    }

    /// Reply to a message.
    pub async fn reply_to_message(
        &self,
        team_id: &str,
        channel_id: &str,
        message_id: &str,
        body: MessageBody,
    ) -> Result<TeamsMessage> {
        #[derive(Serialize)]
        struct SendRequest {
            body: MessageBody,
        }

        let request = SendRequest { body };

        self.post(
            &format!(
                "/teams/{}/channels/{}/messages/{}/replies",
                team_id, channel_id, message_id
            ),
            &request,
        )
        .await
    }

    /// List team members.
    pub async fn list_members(&self, team_id: &str) -> Result<Vec<TeamsMember>> {
        #[derive(Deserialize)]
        struct Response {
            value: Vec<TeamsMember>,
        }

        let response: Response = self.get(&format!("/teams/{}/members", team_id)).await?;
        Ok(response.value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_body() {
        let text = MessageBody::text("Hello");
        assert_eq!(text.content_type, "text");
        assert_eq!(text.content, "Hello");

        let html = MessageBody::html("<b>Hello</b>");
        assert_eq!(html.content_type, "html");
    }
}
