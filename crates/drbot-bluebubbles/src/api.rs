//! BlueBubbles API client.

use crate::{BlueBubblesError, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A chat (conversation).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chat {
    /// Chat GUID.
    pub guid: String,
    /// Chat identifier.
    pub chat_identifier: String,
    /// Display name.
    pub display_name: Option<String>,
    /// Participants.
    pub participants: Vec<Handle>,
    /// Last message date.
    pub last_message_date: Option<DateTime<Utc>>,
    /// Is group chat.
    pub is_group: bool,
}

/// A message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Message GUID.
    pub guid: String,
    /// Text content.
    pub text: Option<String>,
    /// Subject.
    pub subject: Option<String>,
    /// Date sent.
    pub date_created: DateTime<Utc>,
    /// Date read.
    pub date_read: Option<DateTime<Utc>>,
    /// Date delivered.
    pub date_delivered: Option<DateTime<Utc>>,
    /// Is from me.
    pub is_from_me: bool,
    /// Chat GUID.
    pub chat_guid: Option<String>,
    /// Handle (sender/recipient).
    pub handle: Option<Handle>,
    /// Attachments.
    #[serde(default)]
    pub attachments: Vec<Attachment>,
}

/// A handle (contact).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Handle {
    /// Handle address (phone/email).
    pub address: String,
    /// Service (iMessage, SMS).
    pub service: Option<String>,
    /// Country.
    pub country: Option<String>,
}

/// An attachment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attachment {
    /// Attachment GUID.
    pub guid: String,
    /// File name.
    pub filename: Option<String>,
    /// MIME type.
    pub mime_type: Option<String>,
    /// File size.
    pub total_bytes: Option<i64>,
}

/// BlueBubbles API client.
pub struct BlueBubblesApi {
    /// Server URL.
    base_url: String,
    /// Password.
    password: String,
    /// HTTP client.
    client: reqwest::Client,
}

impl BlueBubblesApi {
    /// Create a new API client.
    pub fn new(base_url: &str, password: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            password: password.to_string(),
            client: reqwest::Client::new(),
        }
    }

    /// Get server info.
    pub async fn server_info(&self) -> Result<serde_json::Value> {
        let response = self
            .client
            .get(format!("{}/api/v1/server/info", self.base_url))
            .query(&[("password", &self.password)])
            .send()
            .await
            .map_err(|e| BlueBubblesError::HttpError(e.to_string()))?;

        if !response.status().is_success() {
            let error = response.text().await.unwrap_or_default();
            return Err(BlueBubblesError::ApiError(error));
        }

        response
            .json()
            .await
            .map_err(|e| BlueBubblesError::ApiError(e.to_string()))
    }

    /// List chats.
    pub async fn list_chats(&self, limit: Option<u32>, offset: Option<u32>) -> Result<Vec<Chat>> {
        let mut query: Vec<(&str, String)> = vec![("password", self.password.clone())];
        if let Some(l) = limit {
            query.push(("limit", l.to_string()));
        }
        if let Some(o) = offset {
            query.push(("offset", o.to_string()));
        }

        let response = self
            .client
            .get(format!("{}/api/v1/chat/query", self.base_url))
            .query(&query)
            .send()
            .await
            .map_err(|e| BlueBubblesError::HttpError(e.to_string()))?;

        if !response.status().is_success() {
            let error = response.text().await.unwrap_or_default();
            return Err(BlueBubblesError::ApiError(error));
        }

        #[derive(Deserialize)]
        struct ChatsResponse {
            data: Vec<Chat>,
        }

        let data: ChatsResponse = response
            .json()
            .await
            .map_err(|e| BlueBubblesError::ApiError(e.to_string()))?;

        Ok(data.data)
    }

    /// Get a chat by GUID.
    pub async fn get_chat(&self, guid: &str) -> Result<Chat> {
        let response = self
            .client
            .get(format!("{}/api/v1/chat/{}", self.base_url, guid))
            .query(&[("password", &self.password)])
            .send()
            .await
            .map_err(|e| BlueBubblesError::HttpError(e.to_string()))?;

        if !response.status().is_success() {
            if response.status() == 404 {
                return Err(BlueBubblesError::ChatNotFound(guid.to_string()));
            }
            let error = response.text().await.unwrap_or_default();
            return Err(BlueBubblesError::ApiError(error));
        }

        #[derive(Deserialize)]
        struct ChatResponse {
            data: Chat,
        }

        let data: ChatResponse = response
            .json()
            .await
            .map_err(|e| BlueBubblesError::ApiError(e.to_string()))?;

        Ok(data.data)
    }

    /// Get messages from a chat.
    pub async fn get_messages(
        &self,
        chat_guid: &str,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<Vec<Message>> {
        let mut query: Vec<(&str, String)> = vec![("password", self.password.clone())];
        if let Some(l) = limit {
            query.push(("limit", l.to_string()));
        }
        if let Some(o) = offset {
            query.push(("offset", o.to_string()));
        }

        let response = self
            .client
            .get(format!(
                "{}/api/v1/chat/{}/message",
                self.base_url, chat_guid
            ))
            .query(&query)
            .send()
            .await
            .map_err(|e| BlueBubblesError::HttpError(e.to_string()))?;

        if !response.status().is_success() {
            let error = response.text().await.unwrap_or_default();
            return Err(BlueBubblesError::ApiError(error));
        }

        #[derive(Deserialize)]
        struct MessagesResponse {
            data: Vec<Message>,
        }

        let data: MessagesResponse = response
            .json()
            .await
            .map_err(|e| BlueBubblesError::ApiError(e.to_string()))?;

        Ok(data.data)
    }

    /// Send a message.
    pub async fn send_message(&self, chat_guid: &str, text: &str) -> Result<Message> {
        #[derive(Serialize)]
        struct SendMessageRequest<'a> {
            chat_guid: &'a str,
            message: &'a str,
            method: &'a str,
        }

        let request = SendMessageRequest {
            chat_guid,
            message: text,
            method: "private-api",
        };

        let response = self
            .client
            .post(format!("{}/api/v1/message/text", self.base_url))
            .query(&[("password", &self.password)])
            .json(&request)
            .send()
            .await
            .map_err(|e| BlueBubblesError::HttpError(e.to_string()))?;

        if !response.status().is_success() {
            let error = response.text().await.unwrap_or_default();
            return Err(BlueBubblesError::ApiError(error));
        }

        #[derive(Deserialize)]
        struct MessageResponse {
            data: Message,
        }

        let data: MessageResponse = response
            .json()
            .await
            .map_err(|e| BlueBubblesError::ApiError(e.to_string()))?;

        Ok(data.data)
    }

    /// Send a message to a new chat.
    pub async fn send_new_message(&self, address: &str, text: &str) -> Result<Message> {
        #[derive(Serialize)]
        struct SendMessageRequest<'a> {
            address: &'a str,
            message: &'a str,
            method: &'a str,
        }

        let request = SendMessageRequest {
            address,
            message: text,
            method: "private-api",
        };

        let response = self
            .client
            .post(format!("{}/api/v1/message/text", self.base_url))
            .query(&[("password", &self.password)])
            .json(&request)
            .send()
            .await
            .map_err(|e| BlueBubblesError::HttpError(e.to_string()))?;

        if !response.status().is_success() {
            let error = response.text().await.unwrap_or_default();
            return Err(BlueBubblesError::ApiError(error));
        }

        #[derive(Deserialize)]
        struct MessageResponse {
            data: Message,
        }

        let data: MessageResponse = response
            .json()
            .await
            .map_err(|e| BlueBubblesError::ApiError(e.to_string()))?;

        Ok(data.data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bluebubbles_api_new() {
        let api = BlueBubblesApi::new("http://localhost:1234", "password");
        assert_eq!(api.base_url, "http://localhost:1234");
    }
}
