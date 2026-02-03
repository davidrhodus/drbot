//! Network request/response monitoring via Chrome DevTools Protocol.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};

/// HTTP method.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Head,
    Options,
    Connect,
    Trace,
}

impl std::fmt::Display for HttpMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Get => write!(f, "GET"),
            Self::Post => write!(f, "POST"),
            Self::Put => write!(f, "PUT"),
            Self::Patch => write!(f, "PATCH"),
            Self::Delete => write!(f, "DELETE"),
            Self::Head => write!(f, "HEAD"),
            Self::Options => write!(f, "OPTIONS"),
            Self::Connect => write!(f, "CONNECT"),
            Self::Trace => write!(f, "TRACE"),
        }
    }
}

impl Default for HttpMethod {
    fn default() -> Self {
        Self::Get
    }
}

/// Resource type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ResourceType {
    Document,
    Stylesheet,
    Image,
    Media,
    Font,
    Script,
    TextTrack,
    Xhr,
    Fetch,
    EventSource,
    WebSocket,
    Manifest,
    Other,
}

/// Network request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkRequest {
    /// Request ID.
    pub id: String,
    /// URL.
    pub url: String,
    /// HTTP method.
    pub method: HttpMethod,
    /// Request headers.
    pub headers: HashMap<String, String>,
    /// POST data (if applicable).
    pub post_data: Option<String>,
    /// Resource type.
    pub resource_type: ResourceType,
    /// Initiator (where the request came from).
    pub initiator: Option<String>,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
    /// Frame ID.
    pub frame_id: Option<String>,
}

impl NetworkRequest {
    /// Create a new network request.
    pub fn new(id: &str, url: &str, method: HttpMethod) -> Self {
        Self {
            id: id.to_string(),
            url: url.to_string(),
            method,
            headers: HashMap::new(),
            post_data: None,
            resource_type: ResourceType::Other,
            initiator: None,
            timestamp: Utc::now(),
            frame_id: None,
        }
    }

    /// Get the domain from the URL.
    pub fn domain(&self) -> Option<&str> {
        url::Url::parse(&self.url)
            .ok()
            .and_then(|u| u.host_str().map(|s| s.to_string()))
            .as_deref()
            .map(|_| {
                // Return a reference to the domain in the URL
                // This is a workaround since we can't return a reference to a temporary
                self.url
                    .split("//")
                    .nth(1)
                    .and_then(|s| s.split('/').next())
            })
            .flatten()
    }

    /// Check if this is an XHR/Fetch request.
    pub fn is_api_request(&self) -> bool {
        matches!(self.resource_type, ResourceType::Xhr | ResourceType::Fetch)
    }
}

/// Network response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkResponse {
    /// Request ID (links to request).
    pub request_id: String,
    /// Status code.
    pub status: u16,
    /// Status text.
    pub status_text: String,
    /// Response headers.
    pub headers: HashMap<String, String>,
    /// MIME type.
    pub mime_type: Option<String>,
    /// Content length.
    pub content_length: Option<u64>,
    /// Whether from cache.
    pub from_cache: bool,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
    /// Response body (if captured).
    pub body: Option<Vec<u8>>,
}

impl NetworkResponse {
    /// Create a new network response.
    pub fn new(request_id: &str, status: u16, status_text: &str) -> Self {
        Self {
            request_id: request_id.to_string(),
            status,
            status_text: status_text.to_string(),
            headers: HashMap::new(),
            mime_type: None,
            content_length: None,
            from_cache: false,
            timestamp: Utc::now(),
            body: None,
        }
    }

    /// Check if response is successful (2xx).
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }

    /// Check if response is a redirect (3xx).
    pub fn is_redirect(&self) -> bool {
        (300..400).contains(&self.status)
    }

    /// Check if response is a client error (4xx).
    pub fn is_client_error(&self) -> bool {
        (400..500).contains(&self.status)
    }

    /// Check if response is a server error (5xx).
    pub fn is_server_error(&self) -> bool {
        (500..600).contains(&self.status)
    }

    /// Check if response is an error (4xx or 5xx).
    pub fn is_error(&self) -> bool {
        self.is_client_error() || self.is_server_error()
    }

    /// Get body as string (if available and text).
    pub fn body_text(&self) -> Option<String> {
        self.body
            .as_ref()
            .and_then(|b| String::from_utf8(b.clone()).ok())
    }
}

/// A completed request-response pair.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkEntry {
    /// The request.
    pub request: NetworkRequest,
    /// The response (if completed).
    pub response: Option<NetworkResponse>,
    /// Total time in milliseconds.
    pub duration_ms: Option<u64>,
    /// Error message (if failed).
    pub error: Option<String>,
}

/// Network event for monitoring.
#[derive(Debug, Clone)]
pub enum NetworkEvent {
    /// Request was sent.
    RequestSent(NetworkRequest),
    /// Response received.
    ResponseReceived(NetworkResponse),
    /// Request completed.
    Completed(NetworkEntry),
    /// Request failed.
    Failed { request_id: String, error: String },
}

/// Network monitor configuration.
#[derive(Debug, Clone)]
pub struct NetworkConfig {
    /// Capture request bodies.
    pub capture_request_bodies: bool,
    /// Capture response bodies.
    pub capture_response_bodies: bool,
    /// Maximum response body size to capture (bytes).
    pub max_body_size: usize,
    /// URL patterns to capture (empty = all).
    pub url_patterns: Vec<String>,
    /// Resource types to capture (empty = all).
    pub resource_types: Vec<ResourceType>,
    /// Maximum entries to buffer.
    pub max_buffer_size: usize,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            capture_request_bodies: false,
            capture_response_bodies: false,
            max_body_size: 1024 * 1024, // 1MB
            url_patterns: Vec::new(),
            resource_types: Vec::new(),
            max_buffer_size: 500,
        }
    }
}

/// Network request/response monitor.
pub struct NetworkMonitor {
    /// Configuration.
    config: NetworkConfig,
    /// Pending requests (waiting for response).
    pending: Arc<RwLock<HashMap<String, NetworkRequest>>>,
    /// Completed entries.
    entries: Arc<RwLock<Vec<NetworkEntry>>>,
    /// Event broadcast channel.
    event_tx: broadcast::Sender<NetworkEvent>,
    /// Whether monitoring is active.
    active: Arc<RwLock<bool>>,
}

impl NetworkMonitor {
    /// Create a new network monitor.
    pub fn new(config: NetworkConfig) -> Self {
        let (event_tx, _) = broadcast::channel(256);
        Self {
            config,
            pending: Arc::new(RwLock::new(HashMap::new())),
            entries: Arc::new(RwLock::new(Vec::new())),
            event_tx,
            active: Arc::new(RwLock::new(false)),
        }
    }

    /// Start monitoring.
    pub async fn start(&self) {
        let mut active = self.active.write().await;
        *active = true;
    }

    /// Stop monitoring.
    pub async fn stop(&self) {
        let mut active = self.active.write().await;
        *active = false;
    }

    /// Check if monitoring is active.
    pub async fn is_active(&self) -> bool {
        *self.active.read().await
    }

    /// Record a request.
    pub async fn request_started(&self, request: NetworkRequest) {
        if !*self.active.read().await {
            return;
        }

        if !self.should_capture(&request) {
            return;
        }

        let mut pending = self.pending.write().await;
        pending.insert(request.id.clone(), request.clone());

        let _ = self.event_tx.send(NetworkEvent::RequestSent(request));
    }

    /// Record a response.
    pub async fn response_received(&self, response: NetworkResponse) {
        if !*self.active.read().await {
            return;
        }

        let pending = self.pending.read().await;
        if !pending.contains_key(&response.request_id) {
            return;
        }

        let _ = self.event_tx.send(NetworkEvent::ResponseReceived(response));
    }

    /// Mark request as completed.
    pub async fn request_completed(&self, request_id: &str) {
        if !*self.active.read().await {
            return;
        }

        let mut pending = self.pending.write().await;
        let Some(request) = pending.remove(request_id) else {
            return;
        };

        let entry = NetworkEntry {
            duration_ms: Some((Utc::now() - request.timestamp).num_milliseconds() as u64),
            request,
            response: None, // Would be filled from CDP events
            error: None,
        };

        drop(pending);

        let mut entries = self.entries.write().await;
        entries.push(entry.clone());

        // Trim if needed
        if entries.len() > self.config.max_buffer_size {
            let to_remove = entries.len() - self.config.max_buffer_size;
            entries.drain(0..to_remove);
        }

        let _ = self.event_tx.send(NetworkEvent::Completed(entry));
    }

    /// Mark request as failed.
    pub async fn request_failed(&self, request_id: &str, error: &str) {
        if !*self.active.read().await {
            return;
        }

        let mut pending = self.pending.write().await;
        let Some(request) = pending.remove(request_id) else {
            return;
        };

        let entry = NetworkEntry {
            duration_ms: Some((Utc::now() - request.timestamp).num_milliseconds() as u64),
            request,
            response: None,
            error: Some(error.to_string()),
        };

        drop(pending);

        let mut entries = self.entries.write().await;
        entries.push(entry);

        let _ = self.event_tx.send(NetworkEvent::Failed {
            request_id: request_id.to_string(),
            error: error.to_string(),
        });
    }

    fn should_capture(&self, request: &NetworkRequest) -> bool {
        // Check URL patterns
        if !self.config.url_patterns.is_empty() {
            let matches = self
                .config
                .url_patterns
                .iter()
                .any(|p| request.url.contains(p));
            if !matches {
                return false;
            }
        }

        // Check resource types
        if !self.config.resource_types.is_empty() {
            if !self.config.resource_types.contains(&request.resource_type) {
                return false;
            }
        }

        true
    }

    /// Get all entries.
    pub async fn get_entries(&self) -> Vec<NetworkEntry> {
        let entries = self.entries.read().await;
        entries.clone()
    }

    /// Get failed entries.
    pub async fn get_failed(&self) -> Vec<NetworkEntry> {
        let entries = self.entries.read().await;
        entries
            .iter()
            .filter(|e| e.error.is_some() || e.response.as_ref().map_or(false, |r| r.is_error()))
            .cloned()
            .collect()
    }

    /// Get API requests (XHR/Fetch).
    pub async fn get_api_requests(&self) -> Vec<NetworkEntry> {
        let entries = self.entries.read().await;
        entries
            .iter()
            .filter(|e| e.request.is_api_request())
            .cloned()
            .collect()
    }

    /// Clear entries.
    pub async fn clear(&self) {
        let mut entries = self.entries.write().await;
        entries.clear();
    }

    /// Subscribe to events.
    pub fn subscribe(&self) -> broadcast::Receiver<NetworkEvent> {
        self.event_tx.subscribe()
    }

    /// Get entry count.
    pub async fn count(&self) -> usize {
        let entries = self.entries.read().await;
        entries.len()
    }

    /// Get pending count.
    pub async fn pending_count(&self) -> usize {
        let pending = self.pending.read().await;
        pending.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_network_request() {
        let request = NetworkRequest::new("1", "https://api.example.com/users", HttpMethod::Get);
        assert!(!request.is_api_request()); // Not marked as XHR/Fetch yet
    }

    #[test]
    fn test_network_response() {
        let response = NetworkResponse::new("1", 200, "OK");
        assert!(response.is_success());
        assert!(!response.is_error());

        let error = NetworkResponse::new("2", 404, "Not Found");
        assert!(error.is_client_error());
        assert!(error.is_error());
    }

    #[tokio::test]
    async fn test_network_monitor() {
        let monitor = NetworkMonitor::new(NetworkConfig::default());
        monitor.start().await;

        let request = NetworkRequest::new("1", "https://example.com/api", HttpMethod::Post);
        monitor.request_started(request).await;

        assert_eq!(monitor.pending_count().await, 1);

        monitor.request_completed("1").await;

        assert_eq!(monitor.pending_count().await, 0);
        assert_eq!(monitor.count().await, 1);
    }
}
