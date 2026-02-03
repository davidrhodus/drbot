//! HTTP client utilities for drbot.
//!
//! This crate provides:
//! - HTTP client wrapper with retry and timeout
//! - Request/response helpers
//! - JSON API client
//! - Download utilities

use async_trait::async_trait;
use reqwest::{Client, Method, RequestBuilder, Response, StatusCode};
use serde::{de::DeserializeOwned, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use thiserror::Error;

/// HTTP error types.
#[derive(Error, Debug)]
pub enum HttpError {
    #[error("Request failed: {0}")]
    RequestFailed(#[from] reqwest::Error),

    #[error("Timeout")]
    Timeout,

    #[error("Status {status}: {message}")]
    StatusError { status: u16, message: String },

    #[error("Deserialization failed: {0}")]
    DeserializationFailed(String),

    #[error("Invalid URL: {0}")]
    InvalidUrl(String),
}

/// Result type for HTTP operations.
pub type Result<T> = std::result::Result<T, HttpError>;

/// HTTP client configuration.
#[derive(Debug, Clone)]
pub struct HttpClientConfig {
    /// Request timeout.
    pub timeout: Duration,
    /// Connection timeout.
    pub connect_timeout: Duration,
    /// Max retries.
    pub max_retries: u32,
    /// Retry delay.
    pub retry_delay: Duration,
    /// User agent.
    pub user_agent: String,
}

impl Default for HttpClientConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(30),
            connect_timeout: Duration::from_secs(10),
            max_retries: 3,
            retry_delay: Duration::from_millis(500),
            user_agent: "drbot/1.0".to_string(),
        }
    }
}

/// HTTP client wrapper.
pub struct HttpClient {
    client: Client,
    config: HttpClientConfig,
    base_url: Option<String>,
    default_headers: HashMap<String, String>,
}

impl HttpClient {
    /// Create new HTTP client.
    pub fn new(config: HttpClientConfig) -> Result<Self> {
        let client = Client::builder()
            .timeout(config.timeout)
            .connect_timeout(config.connect_timeout)
            .user_agent(&config.user_agent)
            .build()?;

        Ok(Self {
            client,
            config,
            base_url: None,
            default_headers: HashMap::new(),
        })
    }

    /// Create with default config.
    pub fn default_client() -> Result<Self> {
        Self::new(HttpClientConfig::default())
    }

    /// Set base URL.
    pub fn base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = Some(url.into());
        self
    }

    /// Add default header.
    pub fn header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.default_headers.insert(name.into(), value.into());
        self
    }

    /// Set bearer token.
    pub fn bearer_token(self, token: impl Into<String>) -> Self {
        self.header("Authorization", format!("Bearer {}", token.into()))
    }

    fn build_url(&self, path: &str) -> String {
        if let Some(ref base) = self.base_url {
            if path.starts_with("http://") || path.starts_with("https://") {
                path.to_string()
            } else {
                format!("{}{}", base.trim_end_matches('/'), path)
            }
        } else {
            path.to_string()
        }
    }

    fn apply_headers(&self, mut builder: RequestBuilder) -> RequestBuilder {
        for (key, value) in &self.default_headers {
            builder = builder.header(key, value);
        }
        builder
    }

    /// Execute request with retries.
    pub async fn execute(&self, mut builder: RequestBuilder) -> Result<Response> {
        builder = self.apply_headers(builder);

        let mut last_error = None;
        for attempt in 0..=self.config.max_retries {
            if attempt > 0 {
                tokio::time::sleep(self.config.retry_delay).await;
            }

            match builder.try_clone().unwrap().send().await {
                Ok(response) => {
                    if response.status().is_server_error() && attempt < self.config.max_retries {
                        last_error = Some(HttpError::StatusError {
                            status: response.status().as_u16(),
                            message: "Server error, retrying".to_string(),
                        });
                        continue;
                    }
                    return Ok(response);
                }
                Err(e) => {
                    if e.is_timeout() {
                        last_error = Some(HttpError::Timeout);
                    } else {
                        last_error = Some(HttpError::RequestFailed(e));
                    }
                }
            }
        }

        Err(last_error.unwrap_or(HttpError::Timeout))
    }

    /// GET request.
    pub async fn get(&self, path: &str) -> Result<Response> {
        let url = self.build_url(path);
        let builder = self.client.get(&url);
        self.execute(builder).await
    }

    /// POST request with JSON body.
    pub async fn post<T: Serialize>(&self, path: &str, body: &T) -> Result<Response> {
        let url = self.build_url(path);
        let builder = self.client.post(&url).json(body);
        self.execute(builder).await
    }

    /// PUT request with JSON body.
    pub async fn put<T: Serialize>(&self, path: &str, body: &T) -> Result<Response> {
        let url = self.build_url(path);
        let builder = self.client.put(&url).json(body);
        self.execute(builder).await
    }

    /// PATCH request with JSON body.
    pub async fn patch<T: Serialize>(&self, path: &str, body: &T) -> Result<Response> {
        let url = self.build_url(path);
        let builder = self.client.patch(&url).json(body);
        self.execute(builder).await
    }

    /// DELETE request.
    pub async fn delete(&self, path: &str) -> Result<Response> {
        let url = self.build_url(path);
        let builder = self.client.delete(&url);
        self.execute(builder).await
    }

    /// GET JSON.
    pub async fn get_json<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let response = self.get(path).await?;
        Self::check_status(&response)?;
        response
            .json()
            .await
            .map_err(|e| HttpError::DeserializationFailed(e.to_string()))
    }

    /// POST and get JSON response.
    pub async fn post_json<B: Serialize, R: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<R> {
        let response = self.post(path, body).await?;
        Self::check_status(&response)?;
        response
            .json()
            .await
            .map_err(|e| HttpError::DeserializationFailed(e.to_string()))
    }

    fn check_status(response: &Response) -> Result<()> {
        if response.status().is_success() {
            Ok(())
        } else {
            Err(HttpError::StatusError {
                status: response.status().as_u16(),
                message: response.status().to_string(),
            })
        }
    }
}

/// JSON API client trait.
#[async_trait]
pub trait ApiClient: Send + Sync {
    /// Get resource.
    async fn get<T: DeserializeOwned + Send>(&self, path: &str) -> Result<T>;

    /// Create resource.
    async fn create<B: Serialize + Send + Sync, R: DeserializeOwned + Send>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<R>;

    /// Update resource.
    async fn update<B: Serialize + Send + Sync, R: DeserializeOwned + Send>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<R>;

    /// Delete resource.
    async fn delete(&self, path: &str) -> Result<()>;
}

#[async_trait]
impl ApiClient for HttpClient {
    async fn get<T: DeserializeOwned + Send>(&self, path: &str) -> Result<T> {
        self.get_json(path).await
    }

    async fn create<B: Serialize + Send + Sync, R: DeserializeOwned + Send>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<R> {
        self.post_json(path, body).await
    }

    async fn update<B: Serialize + Send + Sync, R: DeserializeOwned + Send>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<R> {
        let response = self.put(path, body).await?;
        Self::check_status(&response)?;
        response
            .json()
            .await
            .map_err(|e| HttpError::DeserializationFailed(e.to_string()))
    }

    async fn delete(&self, path: &str) -> Result<()> {
        let response = HttpClient::delete(self, path).await?;
        Self::check_status(&response)?;
        Ok(())
    }
}

/// Response helper extension.
pub trait ResponseExt {
    /// Check if successful.
    fn is_success(&self) -> bool;

    /// Get status code.
    fn status_code(&self) -> u16;
}

impl ResponseExt for Response {
    fn is_success(&self) -> bool {
        self.status().is_success()
    }

    fn status_code(&self) -> u16 {
        self.status().as_u16()
    }
}

/// Download progress callback.
pub type ProgressCallback = Box<dyn Fn(u64, Option<u64>) + Send + Sync>;

/// Downloader for large files.
pub struct Downloader {
    client: HttpClient,
}

impl Downloader {
    /// Create new downloader.
    pub fn new(client: HttpClient) -> Self {
        Self { client }
    }

    /// Download to bytes.
    pub async fn download_bytes(&self, url: &str) -> Result<Vec<u8>> {
        let response = self.client.get(url).await?;
        HttpClient::check_status(&response)?;
        response
            .bytes()
            .await
            .map(|b| b.to_vec())
            .map_err(HttpError::RequestFailed)
    }

    /// Download to string.
    pub async fn download_text(&self, url: &str) -> Result<String> {
        let response = self.client.get(url).await?;
        HttpClient::check_status(&response)?;
        response.text().await.map_err(HttpError::RequestFailed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = HttpClientConfig::default();
        assert_eq!(config.timeout, Duration::from_secs(30));
        assert_eq!(config.max_retries, 3);
    }

    #[test]
    fn test_build_url() {
        let client = HttpClient::default_client()
            .unwrap()
            .base_url("https://api.example.com");

        assert_eq!(client.build_url("/users"), "https://api.example.com/users");
        assert_eq!(
            client.build_url("https://other.com/path"),
            "https://other.com/path"
        );
    }

    #[test]
    fn test_headers() {
        let client = HttpClient::default_client()
            .unwrap()
            .header("X-Custom", "value")
            .bearer_token("token123");

        assert_eq!(
            client.default_headers.get("X-Custom"),
            Some(&"value".to_string())
        );
        assert_eq!(
            client.default_headers.get("Authorization"),
            Some(&"Bearer token123".to_string())
        );
    }
}
