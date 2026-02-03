//! Request/response logging for drbot.
//!
//! This crate provides structured logging for HTTP requests and responses,
//! including timing, headers, body capture, and filtering.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Request log error types.
#[derive(Error, Debug)]
pub enum RequestLogError {
    #[error("Log entry not found: {0}")]
    NotFound(String),

    #[error("Storage error: {0}")]
    StorageError(String),

    #[error("Filter error: {0}")]
    FilterError(String),

    #[error("Serialization error: {0}")]
    SerializationError(String),
}

/// Result type for request log operations.
pub type Result<T> = std::result::Result<T, RequestLogError>;

/// HTTP method.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HttpMethod {
    GET,
    POST,
    PUT,
    DELETE,
    PATCH,
    HEAD,
    OPTIONS,
    TRACE,
    CONNECT,
}

impl std::fmt::Display for HttpMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HttpMethod::GET => write!(f, "GET"),
            HttpMethod::POST => write!(f, "POST"),
            HttpMethod::PUT => write!(f, "PUT"),
            HttpMethod::DELETE => write!(f, "DELETE"),
            HttpMethod::PATCH => write!(f, "PATCH"),
            HttpMethod::HEAD => write!(f, "HEAD"),
            HttpMethod::OPTIONS => write!(f, "OPTIONS"),
            HttpMethod::TRACE => write!(f, "TRACE"),
            HttpMethod::CONNECT => write!(f, "CONNECT"),
        }
    }
}

/// HTTP headers represented as a map.
pub type Headers = HashMap<String, String>;

/// Request information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestInfo {
    /// Unique request ID.
    pub id: Uuid,
    /// HTTP method.
    pub method: HttpMethod,
    /// Request URL.
    pub url: String,
    /// Request path.
    pub path: String,
    /// Query parameters.
    pub query: HashMap<String, String>,
    /// Request headers.
    pub headers: Headers,
    /// Request body (if captured).
    pub body: Option<String>,
    /// Body size in bytes.
    pub body_size: usize,
    /// Remote address.
    pub remote_addr: Option<String>,
    /// Timestamp when request was received.
    pub timestamp: DateTime<Utc>,
}

impl RequestInfo {
    /// Create a new request info.
    pub fn new(method: HttpMethod, url: impl Into<String>) -> Self {
        let url = url.into();
        let path = url.split('?').next().unwrap_or(&url).to_string();
        Self {
            id: Uuid::new_v4(),
            method,
            url,
            path,
            query: HashMap::new(),
            headers: HashMap::new(),
            body: None,
            body_size: 0,
            remote_addr: None,
            timestamp: Utc::now(),
        }
    }

    /// Set the request body.
    pub fn with_body(mut self, body: impl Into<String>) -> Self {
        let body = body.into();
        self.body_size = body.len();
        self.body = Some(body);
        self
    }

    /// Add a header.
    pub fn with_header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(key.into(), value.into());
        self
    }

    /// Set the remote address.
    pub fn with_remote_addr(mut self, addr: impl Into<String>) -> Self {
        self.remote_addr = Some(addr.into());
        self
    }
}

/// Response information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseInfo {
    /// HTTP status code.
    pub status: u16,
    /// Response headers.
    pub headers: Headers,
    /// Response body (if captured).
    pub body: Option<String>,
    /// Body size in bytes.
    pub body_size: usize,
    /// Timestamp when response was sent.
    pub timestamp: DateTime<Utc>,
}

impl ResponseInfo {
    /// Create a new response info.
    pub fn new(status: u16) -> Self {
        Self {
            status,
            headers: HashMap::new(),
            body: None,
            body_size: 0,
            timestamp: Utc::now(),
        }
    }

    /// Set the response body.
    pub fn with_body(mut self, body: impl Into<String>) -> Self {
        let body = body.into();
        self.body_size = body.len();
        self.body = Some(body);
        self
    }

    /// Add a header.
    pub fn with_header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(key.into(), value.into());
        self
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
}

/// A complete request/response log entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    /// Unique entry ID.
    pub id: Uuid,
    /// Request information.
    pub request: RequestInfo,
    /// Response information (None if request failed).
    pub response: Option<ResponseInfo>,
    /// Duration in milliseconds.
    pub duration_ms: u64,
    /// Error message if request failed.
    pub error: Option<String>,
    /// Custom tags.
    pub tags: HashMap<String, String>,
    /// Trace ID for distributed tracing.
    pub trace_id: Option<String>,
    /// Span ID for distributed tracing.
    pub span_id: Option<String>,
}

impl LogEntry {
    /// Create a new log entry.
    pub fn new(request: RequestInfo) -> Self {
        Self {
            id: Uuid::new_v4(),
            request,
            response: None,
            duration_ms: 0,
            error: None,
            tags: HashMap::new(),
            trace_id: None,
            span_id: None,
        }
    }

    /// Set the response.
    pub fn with_response(mut self, response: ResponseInfo, duration_ms: u64) -> Self {
        self.response = Some(response);
        self.duration_ms = duration_ms;
        self
    }

    /// Set an error.
    pub fn with_error(mut self, error: impl Into<String>) -> Self {
        self.error = Some(error.into());
        self
    }

    /// Add a tag.
    pub fn with_tag(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.tags.insert(key.into(), value.into());
        self
    }

    /// Set trace context.
    pub fn with_trace(mut self, trace_id: impl Into<String>, span_id: impl Into<String>) -> Self {
        self.trace_id = Some(trace_id.into());
        self.span_id = Some(span_id.into());
        self
    }

    /// Check if request was successful.
    pub fn is_success(&self) -> bool {
        self.response.as_ref().map_or(false, |r| r.is_success())
    }
}

/// Filter for querying log entries.
#[derive(Debug, Clone, Default)]
pub struct LogFilter {
    /// Filter by HTTP method.
    pub method: Option<HttpMethod>,
    /// Filter by path prefix.
    pub path_prefix: Option<String>,
    /// Filter by status code range.
    pub status_min: Option<u16>,
    pub status_max: Option<u16>,
    /// Filter by minimum duration.
    pub min_duration_ms: Option<u64>,
    /// Filter by time range.
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    /// Filter by tag.
    pub tags: HashMap<String, String>,
    /// Limit results.
    pub limit: Option<usize>,
    /// Offset for pagination.
    pub offset: Option<usize>,
}

impl LogFilter {
    /// Create a new filter.
    pub fn new() -> Self {
        Self::default()
    }

    /// Filter by method.
    pub fn method(mut self, method: HttpMethod) -> Self {
        self.method = Some(method);
        self
    }

    /// Filter by path prefix.
    pub fn path_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.path_prefix = Some(prefix.into());
        self
    }

    /// Filter by status range.
    pub fn status_range(mut self, min: u16, max: u16) -> Self {
        self.status_min = Some(min);
        self.status_max = Some(max);
        self
    }

    /// Filter for errors only.
    pub fn errors_only(self) -> Self {
        self.status_range(400, 599)
    }

    /// Filter by minimum duration.
    pub fn min_duration(mut self, ms: u64) -> Self {
        self.min_duration_ms = Some(ms);
        self
    }

    /// Filter by time range.
    pub fn time_range(mut self, start: DateTime<Utc>, end: DateTime<Utc>) -> Self {
        self.start_time = Some(start);
        self.end_time = Some(end);
        self
    }

    /// Add tag filter.
    pub fn tag(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.tags.insert(key.into(), value.into());
        self
    }

    /// Set limit.
    pub fn limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Set offset.
    pub fn offset(mut self, offset: usize) -> Self {
        self.offset = Some(offset);
        self
    }

    /// Check if an entry matches this filter.
    pub fn matches(&self, entry: &LogEntry) -> bool {
        // Method filter
        if let Some(method) = &self.method {
            if entry.request.method != *method {
                return false;
            }
        }

        // Path prefix filter
        if let Some(prefix) = &self.path_prefix {
            if !entry.request.path.starts_with(prefix) {
                return false;
            }
        }

        // Status filter
        if let Some(response) = &entry.response {
            if let Some(min) = self.status_min {
                if response.status < min {
                    return false;
                }
            }
            if let Some(max) = self.status_max {
                if response.status > max {
                    return false;
                }
            }
        } else if self.status_min.is_some() || self.status_max.is_some() {
            // If filtering by status but no response, exclude
            return false;
        }

        // Duration filter
        if let Some(min_duration) = self.min_duration_ms {
            if entry.duration_ms < min_duration {
                return false;
            }
        }

        // Time range filter
        if let Some(start) = self.start_time {
            if entry.request.timestamp < start {
                return false;
            }
        }
        if let Some(end) = self.end_time {
            if entry.request.timestamp > end {
                return false;
            }
        }

        // Tag filter
        for (key, value) in &self.tags {
            match entry.tags.get(key) {
                Some(v) if v == value => {}
                _ => return false,
            }
        }

        true
    }
}

/// Trait for log storage backends.
#[async_trait]
pub trait LogStorage: Send + Sync {
    /// Store a log entry.
    async fn store(&self, entry: LogEntry) -> Result<()>;

    /// Get a log entry by ID.
    async fn get(&self, id: Uuid) -> Result<Option<LogEntry>>;

    /// Query log entries.
    async fn query(&self, filter: &LogFilter) -> Result<Vec<LogEntry>>;

    /// Delete entries older than the given duration.
    async fn cleanup(&self, older_than: chrono::Duration) -> Result<usize>;

    /// Get storage statistics.
    async fn stats(&self) -> Result<StorageStats>;
}

/// Storage statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageStats {
    /// Total number of entries.
    pub total_entries: usize,
    /// Total storage size in bytes.
    pub size_bytes: usize,
    /// Oldest entry timestamp.
    pub oldest_entry: Option<DateTime<Utc>>,
    /// Newest entry timestamp.
    pub newest_entry: Option<DateTime<Utc>>,
}

/// In-memory log storage.
pub struct InMemoryStorage {
    entries: RwLock<Vec<LogEntry>>,
    max_entries: usize,
}

impl InMemoryStorage {
    /// Create a new in-memory storage.
    pub fn new(max_entries: usize) -> Self {
        Self {
            entries: RwLock::new(Vec::new()),
            max_entries,
        }
    }
}

#[async_trait]
impl LogStorage for InMemoryStorage {
    async fn store(&self, entry: LogEntry) -> Result<()> {
        let mut entries = self.entries.write().await;

        // Remove oldest entries if at capacity
        while entries.len() >= self.max_entries {
            entries.remove(0);
        }

        entries.push(entry);
        Ok(())
    }

    async fn get(&self, id: Uuid) -> Result<Option<LogEntry>> {
        let entries = self.entries.read().await;
        Ok(entries.iter().find(|e| e.id == id).cloned())
    }

    async fn query(&self, filter: &LogFilter) -> Result<Vec<LogEntry>> {
        let entries = self.entries.read().await;

        let mut results: Vec<_> = entries
            .iter()
            .filter(|e| filter.matches(e))
            .cloned()
            .collect();

        // Apply offset
        if let Some(offset) = filter.offset {
            if offset < results.len() {
                results = results[offset..].to_vec();
            } else {
                results.clear();
            }
        }

        // Apply limit
        if let Some(limit) = filter.limit {
            results.truncate(limit);
        }

        Ok(results)
    }

    async fn cleanup(&self, older_than: chrono::Duration) -> Result<usize> {
        let cutoff = Utc::now() - older_than;
        let mut entries = self.entries.write().await;
        let before_count = entries.len();
        entries.retain(|e| e.request.timestamp >= cutoff);
        Ok(before_count - entries.len())
    }

    async fn stats(&self) -> Result<StorageStats> {
        let entries = self.entries.read().await;

        let oldest = entries.first().map(|e| e.request.timestamp);
        let newest = entries.last().map(|e| e.request.timestamp);

        // Estimate size (rough approximation)
        let size_bytes = entries.len() * 1024; // ~1KB per entry estimate

        Ok(StorageStats {
            total_entries: entries.len(),
            size_bytes,
            oldest_entry: oldest,
            newest_entry: newest,
        })
    }
}

/// Sensitive header names that should be redacted.
const SENSITIVE_HEADERS: &[&str] = &[
    "authorization",
    "cookie",
    "set-cookie",
    "x-api-key",
    "x-auth-token",
    "api-key",
    "access-token",
    "refresh-token",
];

/// Header redactor for sensitive data.
pub struct HeaderRedactor {
    sensitive_patterns: Vec<String>,
}

impl HeaderRedactor {
    /// Create a new header redactor with default patterns.
    pub fn new() -> Self {
        Self {
            sensitive_patterns: SENSITIVE_HEADERS.iter().map(|s| s.to_string()).collect(),
        }
    }

    /// Add a sensitive header pattern.
    pub fn add_pattern(mut self, pattern: impl Into<String>) -> Self {
        self.sensitive_patterns.push(pattern.into().to_lowercase());
        self
    }

    /// Redact sensitive headers.
    pub fn redact(&self, headers: &mut Headers) {
        for (key, value) in headers.iter_mut() {
            let lower_key = key.to_lowercase();
            if self
                .sensitive_patterns
                .iter()
                .any(|p| lower_key.contains(p))
            {
                *value = "[REDACTED]".to_string();
            }
        }
    }
}

impl Default for HeaderRedactor {
    fn default() -> Self {
        Self::new()
    }
}

/// Request logger with configurable options.
pub struct RequestLogger {
    storage: Arc<dyn LogStorage>,
    redactor: HeaderRedactor,
    log_body: bool,
    max_body_size: usize,
}

impl RequestLogger {
    /// Create a new request logger.
    pub fn new(storage: Arc<dyn LogStorage>) -> Self {
        Self {
            storage,
            redactor: HeaderRedactor::new(),
            log_body: true,
            max_body_size: 64 * 1024, // 64KB default
        }
    }

    /// Set whether to log request/response bodies.
    pub fn log_body(mut self, enabled: bool) -> Self {
        self.log_body = enabled;
        self
    }

    /// Set maximum body size to log.
    pub fn max_body_size(mut self, size: usize) -> Self {
        self.max_body_size = size;
        self
    }

    /// Set custom header redactor.
    pub fn redactor(mut self, redactor: HeaderRedactor) -> Self {
        self.redactor = redactor;
        self
    }

    /// Log a request/response pair.
    pub async fn log(&self, mut entry: LogEntry) -> Result<()> {
        // Redact sensitive headers
        self.redactor.redact(&mut entry.request.headers);
        if let Some(ref mut response) = entry.response {
            self.redactor.redact(&mut response.headers);
        }

        // Truncate body if needed
        if !self.log_body {
            entry.request.body = None;
            if let Some(ref mut response) = entry.response {
                response.body = None;
            }
        } else {
            if let Some(ref body) = entry.request.body {
                if body.len() > self.max_body_size {
                    entry.request.body = Some(format!(
                        "{}... [truncated, {} bytes total]",
                        &body[..self.max_body_size],
                        body.len()
                    ));
                }
            }
            if let Some(ref mut response) = entry.response {
                if let Some(ref body) = response.body {
                    if body.len() > self.max_body_size {
                        response.body = Some(format!(
                            "{}... [truncated, {} bytes total]",
                            &body[..self.max_body_size],
                            body.len()
                        ));
                    }
                }
            }
        }

        self.storage.store(entry).await
    }

    /// Query logs.
    pub async fn query(&self, filter: &LogFilter) -> Result<Vec<LogEntry>> {
        self.storage.query(filter).await
    }

    /// Get a specific log entry.
    pub async fn get(&self, id: Uuid) -> Result<Option<LogEntry>> {
        self.storage.get(id).await
    }

    /// Get storage statistics.
    pub async fn stats(&self) -> Result<StorageStats> {
        self.storage.stats().await
    }
}

/// Log output formatter.
pub struct LogFormatter;

impl LogFormatter {
    /// Format entry as single-line log.
    pub fn format_line(entry: &LogEntry) -> String {
        let status = entry
            .response
            .as_ref()
            .map(|r| r.status.to_string())
            .unwrap_or_else(|| "ERR".to_string());

        format!(
            "{} {} {} {} {}ms",
            entry.request.timestamp.format("%Y-%m-%d %H:%M:%S"),
            entry.request.method,
            entry.request.path,
            status,
            entry.duration_ms
        )
    }

    /// Format entry as JSON.
    pub fn format_json(entry: &LogEntry) -> std::result::Result<String, serde_json::Error> {
        serde_json::to_string(entry)
    }

    /// Format entry as pretty JSON.
    pub fn format_json_pretty(entry: &LogEntry) -> std::result::Result<String, serde_json::Error> {
        serde_json::to_string_pretty(entry)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_info() {
        let request = RequestInfo::new(HttpMethod::GET, "https://example.com/api/users?page=1")
            .with_header("Content-Type", "application/json")
            .with_remote_addr("192.168.1.1");

        assert_eq!(request.method, HttpMethod::GET);
        assert_eq!(request.path, "https://example.com/api/users");
        assert_eq!(
            request.headers.get("Content-Type"),
            Some(&"application/json".to_string())
        );
    }

    #[test]
    fn test_response_info() {
        let response = ResponseInfo::new(200)
            .with_body(r#"{"status": "ok"}"#)
            .with_header("Content-Type", "application/json");

        assert!(response.is_success());
        assert!(!response.is_client_error());
        assert!(!response.is_server_error());
        assert_eq!(response.body_size, 16);
    }

    #[test]
    fn test_log_entry() {
        let request = RequestInfo::new(HttpMethod::POST, "/api/login");
        let response = ResponseInfo::new(200);
        let entry = LogEntry::new(request)
            .with_response(response, 150)
            .with_tag("user", "test_user");

        assert!(entry.is_success());
        assert_eq!(entry.duration_ms, 150);
        assert_eq!(entry.tags.get("user"), Some(&"test_user".to_string()));
    }

    #[test]
    fn test_filter_matches() {
        let request = RequestInfo::new(HttpMethod::GET, "/api/users");
        let response = ResponseInfo::new(200);
        let entry = LogEntry::new(request).with_response(response, 100);

        // Method filter
        assert!(LogFilter::new().method(HttpMethod::GET).matches(&entry));
        assert!(!LogFilter::new().method(HttpMethod::POST).matches(&entry));

        // Path filter
        assert!(LogFilter::new().path_prefix("/api").matches(&entry));
        assert!(!LogFilter::new().path_prefix("/admin").matches(&entry));

        // Status filter
        assert!(LogFilter::new().status_range(200, 299).matches(&entry));
        assert!(!LogFilter::new().errors_only().matches(&entry));
    }

    #[tokio::test]
    async fn test_in_memory_storage() {
        let storage = InMemoryStorage::new(100);

        let request = RequestInfo::new(HttpMethod::GET, "/test");
        let entry = LogEntry::new(request);
        let entry_id = entry.id;

        storage.store(entry).await.unwrap();

        let retrieved = storage.get(entry_id).await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().id, entry_id);
    }

    #[tokio::test]
    async fn test_storage_query() {
        let storage = InMemoryStorage::new(100);

        // Store multiple entries
        for i in 0..5 {
            let method = if i % 2 == 0 {
                HttpMethod::GET
            } else {
                HttpMethod::POST
            };
            let request = RequestInfo::new(method, "/test");
            let entry = LogEntry::new(request);
            storage.store(entry).await.unwrap();
        }

        // Query GET requests
        let results = storage
            .query(&LogFilter::new().method(HttpMethod::GET))
            .await
            .unwrap();
        assert_eq!(results.len(), 3);

        // Query POST requests
        let results = storage
            .query(&LogFilter::new().method(HttpMethod::POST))
            .await
            .unwrap();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_header_redactor() {
        let mut headers = HashMap::new();
        headers.insert("Authorization".to_string(), "Bearer secret123".to_string());
        headers.insert("Content-Type".to_string(), "application/json".to_string());
        headers.insert("X-API-Key".to_string(), "my-secret-key".to_string());

        let redactor = HeaderRedactor::new();
        redactor.redact(&mut headers);

        assert_eq!(
            headers.get("Authorization"),
            Some(&"[REDACTED]".to_string())
        );
        assert_eq!(
            headers.get("Content-Type"),
            Some(&"application/json".to_string())
        );
        assert_eq!(headers.get("X-API-Key"), Some(&"[REDACTED]".to_string()));
    }

    #[tokio::test]
    async fn test_request_logger() {
        let storage = Arc::new(InMemoryStorage::new(100));
        let logger = RequestLogger::new(storage.clone()).log_body(true);

        let request = RequestInfo::new(HttpMethod::GET, "/api/test")
            .with_header("Authorization", "Bearer secret");
        let response = ResponseInfo::new(200);
        let entry = LogEntry::new(request).with_response(response, 50);

        logger.log(entry).await.unwrap();

        let results = logger.query(&LogFilter::new()).await.unwrap();
        assert_eq!(results.len(), 1);

        // Verify header was redacted
        let logged_entry = &results[0];
        assert_eq!(
            logged_entry.request.headers.get("Authorization"),
            Some(&"[REDACTED]".to_string())
        );
    }

    #[test]
    fn test_log_formatter() {
        let request = RequestInfo::new(HttpMethod::GET, "/api/test");
        let response = ResponseInfo::new(200);
        let entry = LogEntry::new(request).with_response(response, 50);

        let line = LogFormatter::format_line(&entry);
        assert!(line.contains("GET"));
        assert!(line.contains("/api/test"));
        assert!(line.contains("200"));
        assert!(line.contains("50ms"));
    }
}
