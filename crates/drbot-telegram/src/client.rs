//! Telegram Bot API client.

use crate::api::{
    ApiResponse, File, GetUpdatesRequest, SendMessageRequest, TelegramMessage, Update,
};
use drbot_core::{Error, Result};
use reqwest::Client;
use tracing::{debug, error, trace};

/// Convert reqwest error to our error type.
fn http_err(e: reqwest::Error) -> Error {
    Error::Http(e.to_string())
}

/// Telegram Bot API client.
#[derive(Clone)]
pub struct TelegramClient {
    /// HTTP client.
    client: Client,
    /// Bot token.
    token: String,
    /// Base API URL.
    base_url: String,
}

impl TelegramClient {
    /// Create a new Telegram client.
    pub fn new(token: impl Into<String>) -> Self {
        Self {
            client: Client::new(),
            token: token.into(),
            base_url: "https://api.telegram.org".to_string(),
        }
    }

    /// Build URL for an API method.
    fn url(&self, method: &str) -> String {
        format!("{}/bot{}/{}", self.base_url, self.token, method)
    }

    /// Get bot information.
    pub async fn get_me(&self) -> Result<crate::api::User> {
        let url = self.url("getMe");
        debug!("Calling getMe");

        let response: ApiResponse<crate::api::User> = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(http_err)?
            .json()
            .await
            .map_err(http_err)?;

        if response.ok {
            response
                .result
                .ok_or_else(|| Error::Internal("Missing result in getMe response".to_string()))
        } else {
            Err(Error::Internal(
                response
                    .description
                    .unwrap_or_else(|| "Unknown error".to_string()),
            ))
        }
    }

    /// Get updates using long polling.
    pub async fn get_updates(&self, request: &GetUpdatesRequest) -> Result<Vec<Update>> {
        let url = self.url("getUpdates");
        trace!("Calling getUpdates with offset {:?}", request.offset);

        let response: ApiResponse<Vec<Update>> = self
            .client
            .post(&url)
            .json(request)
            .timeout(std::time::Duration::from_secs(
                request.timeout.unwrap_or(30) as u64 + 10,
            ))
            .send()
            .await
            .map_err(http_err)?
            .json()
            .await
            .map_err(http_err)?;

        if response.ok {
            Ok(response.result.unwrap_or_default())
        } else {
            let err = response
                .description
                .unwrap_or_else(|| "Unknown error".to_string());
            error!(error = %err, "getUpdates failed");
            Err(Error::Internal(err))
        }
    }

    /// Send a text message.
    pub async fn send_message(&self, request: &SendMessageRequest) -> Result<TelegramMessage> {
        let url = self.url("sendMessage");
        debug!(chat_id = request.chat_id, "Sending message");

        let response: ApiResponse<TelegramMessage> = self
            .client
            .post(&url)
            .json(request)
            .send()
            .await
            .map_err(http_err)?
            .json()
            .await
            .map_err(http_err)?;

        if response.ok {
            response.result.ok_or_else(|| {
                Error::Internal("Missing result in sendMessage response".to_string())
            })
        } else {
            let err = response
                .description
                .unwrap_or_else(|| "Unknown error".to_string());
            error!(error = %err, "sendMessage failed");
            Err(Error::Internal(err))
        }
    }

    /// Get file information for downloading.
    pub async fn get_file(&self, file_id: &str) -> Result<File> {
        let url = self.url("getFile");
        debug!(file_id = %file_id, "Getting file info");

        let response: ApiResponse<File> = self
            .client
            .post(&url)
            .json(&serde_json::json!({ "file_id": file_id }))
            .send()
            .await
            .map_err(http_err)?
            .json()
            .await
            .map_err(http_err)?;

        if response.ok {
            response
                .result
                .ok_or_else(|| Error::Internal("Missing result in getFile response".to_string()))
        } else {
            let err = response
                .description
                .unwrap_or_else(|| "Unknown error".to_string());
            error!(error = %err, "getFile failed");
            Err(Error::Internal(err))
        }
    }

    /// Download a file.
    pub async fn download_file(&self, file_path: &str) -> Result<Vec<u8>> {
        let url = format!("{}/file/bot{}/{}", self.base_url, self.token, file_path);
        debug!(file_path = %file_path, "Downloading file");

        let bytes = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(http_err)?
            .bytes()
            .await
            .map_err(http_err)?;
        Ok(bytes.to_vec())
    }

    /// Delete a webhook (to use long polling).
    pub async fn delete_webhook(&self) -> Result<bool> {
        let url = self.url("deleteWebhook");
        debug!("Deleting webhook");

        let response: ApiResponse<bool> = self
            .client
            .post(&url)
            .json(&serde_json::json!({ "drop_pending_updates": false }))
            .send()
            .await
            .map_err(http_err)?
            .json()
            .await
            .map_err(http_err)?;

        if response.ok {
            Ok(response.result.unwrap_or(false))
        } else {
            let err = response
                .description
                .unwrap_or_else(|| "Unknown error".to_string());
            error!(error = %err, "deleteWebhook failed");
            Err(Error::Internal(err))
        }
    }
}
