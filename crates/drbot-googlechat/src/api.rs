//! Google Chat API client.

use crate::{GoogleChatError, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Google Chat space.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Space {
    /// Space resource name.
    pub name: String,
    /// Display name.
    pub display_name: Option<String>,
    /// Space type (ROOM, DM, etc.).
    pub space_type: String,
    /// Single thread mode.
    pub single_user_bot_dm: bool,
    /// Threaded mode.
    pub threaded: bool,
}

/// Google Chat message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Message resource name.
    pub name: String,
    /// Sender.
    pub sender: Member,
    /// Create time.
    pub create_time: DateTime<Utc>,
    /// Text content.
    pub text: Option<String>,
    /// Thread name.
    pub thread: Option<Thread>,
    /// Space name.
    pub space: String,
    /// Argument text (without @mentions).
    pub argument_text: Option<String>,
}

/// Message thread.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Thread {
    /// Thread resource name.
    pub name: String,
}

/// Space member.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Member {
    /// Member resource name.
    pub name: String,
    /// Display name.
    pub display_name: Option<String>,
    /// Member type (HUMAN, BOT).
    pub member_type: String,
    /// Domain ID.
    pub domain_id: Option<String>,
}

/// Google Chat API client.
pub struct GoogleChatApi {
    /// Base URL for the API.
    base_url: String,
    /// Access token.
    access_token: Option<String>,
    /// HTTP client.
    client: reqwest::Client,
}

impl GoogleChatApi {
    /// Create a new API client.
    pub fn new() -> Self {
        Self {
            base_url: "https://chat.googleapis.com/v1".to_string(),
            access_token: None,
            client: reqwest::Client::new(),
        }
    }

    /// Set the access token.
    pub fn with_token(mut self, token: &str) -> Self {
        self.access_token = Some(token.to_string());
        self
    }

    /// List spaces the bot has access to.
    pub async fn list_spaces(&self) -> Result<Vec<Space>> {
        let token = self
            .access_token
            .as_ref()
            .ok_or_else(|| GoogleChatError::AuthenticationFailed("No access token".into()))?;

        let response = self
            .client
            .get(format!("{}/spaces", self.base_url))
            .header("Authorization", format!("Bearer {}", token))
            .send()
            .await
            .map_err(|e| GoogleChatError::HttpError(e.to_string()))?;

        if !response.status().is_success() {
            let error = response.text().await.unwrap_or_default();
            return Err(GoogleChatError::ApiError(error));
        }

        #[derive(Deserialize)]
        struct ListSpacesResponse {
            spaces: Option<Vec<Space>>,
        }

        let data: ListSpacesResponse = response
            .json()
            .await
            .map_err(|e| GoogleChatError::ApiError(e.to_string()))?;

        Ok(data.spaces.unwrap_or_default())
    }

    /// Get a space by name.
    pub async fn get_space(&self, name: &str) -> Result<Space> {
        let token = self
            .access_token
            .as_ref()
            .ok_or_else(|| GoogleChatError::AuthenticationFailed("No access token".into()))?;

        let response = self
            .client
            .get(format!("{}/{}", self.base_url, name))
            .header("Authorization", format!("Bearer {}", token))
            .send()
            .await
            .map_err(|e| GoogleChatError::HttpError(e.to_string()))?;

        if !response.status().is_success() {
            if response.status() == 404 {
                return Err(GoogleChatError::SpaceNotFound(name.to_string()));
            }
            let error = response.text().await.unwrap_or_default();
            return Err(GoogleChatError::ApiError(error));
        }

        response
            .json()
            .await
            .map_err(|e| GoogleChatError::ApiError(e.to_string()))
    }

    /// Send a message to a space.
    pub async fn send_message(&self, space: &str, text: &str) -> Result<Message> {
        let token = self
            .access_token
            .as_ref()
            .ok_or_else(|| GoogleChatError::AuthenticationFailed("No access token".into()))?;

        #[derive(Serialize)]
        struct CreateMessageRequest {
            text: String,
        }

        let request = CreateMessageRequest {
            text: text.to_string(),
        };

        let response = self
            .client
            .post(format!("{}/{}/messages", self.base_url, space))
            .header("Authorization", format!("Bearer {}", token))
            .json(&request)
            .send()
            .await
            .map_err(|e| GoogleChatError::HttpError(e.to_string()))?;

        if !response.status().is_success() {
            let error = response.text().await.unwrap_or_default();
            return Err(GoogleChatError::ApiError(error));
        }

        response
            .json()
            .await
            .map_err(|e| GoogleChatError::ApiError(e.to_string()))
    }

    /// Send a message in a thread.
    pub async fn send_message_in_thread(
        &self,
        space: &str,
        thread: &str,
        text: &str,
    ) -> Result<Message> {
        let token = self
            .access_token
            .as_ref()
            .ok_or_else(|| GoogleChatError::AuthenticationFailed("No access token".into()))?;

        #[derive(Serialize)]
        struct CreateMessageRequest {
            text: String,
            thread: ThreadRef,
        }

        #[derive(Serialize)]
        struct ThreadRef {
            name: String,
        }

        let request = CreateMessageRequest {
            text: text.to_string(),
            thread: ThreadRef {
                name: thread.to_string(),
            },
        };

        let response = self
            .client
            .post(format!("{}/{}/messages", self.base_url, space))
            .header("Authorization", format!("Bearer {}", token))
            .query(&[("messageReplyOption", "REPLY_MESSAGE_FALLBACK_TO_NEW_THREAD")])
            .json(&request)
            .send()
            .await
            .map_err(|e| GoogleChatError::HttpError(e.to_string()))?;

        if !response.status().is_success() {
            let error = response.text().await.unwrap_or_default();
            return Err(GoogleChatError::ApiError(error));
        }

        response
            .json()
            .await
            .map_err(|e| GoogleChatError::ApiError(e.to_string()))
    }
}

impl Default for GoogleChatApi {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_google_chat_api_new() {
        let api = GoogleChatApi::new();
        assert!(api.access_token.is_none());
    }
}
