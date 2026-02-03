//! Response handling utilities for drbot.
//!
//! This crate provides:
//! - Response wrapper types
//! - Response body handling
//! - Status checking utilities

use std::collections::HashMap;
use thiserror::Error;

/// Response error types.
#[derive(Error, Debug, Clone)]
pub enum ResponseError {
    #[error("Response error: {status} - {message}")]
    Status { status: u16, message: String },

    #[error("Body read error: {0}")]
    BodyRead(String),

    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Timeout")]
    Timeout,
}

/// Result type for response operations.
pub type Result<T> = std::result::Result<T, ResponseError>;

/// HTTP response.
#[derive(Debug, Clone)]
pub struct Response {
    pub status: u16,
    pub headers: HashMap<String, String>,
    body: ResponseBody,
}

impl Response {
    /// Create new response.
    pub fn new(status: u16) -> Self {
        Self {
            status,
            headers: HashMap::new(),
            body: ResponseBody::Empty,
        }
    }

    /// Set header.
    pub fn header<K: Into<String>, V: Into<String>>(mut self, key: K, value: V) -> Self {
        self.headers.insert(key.into(), value.into());
        self
    }

    /// Set text body.
    pub fn text<S: Into<String>>(mut self, text: S) -> Self {
        self.body = ResponseBody::Text(text.into());
        self
    }

    /// Set bytes body.
    pub fn bytes<B: Into<Vec<u8>>>(mut self, bytes: B) -> Self {
        self.body = ResponseBody::Bytes(bytes.into());
        self
    }

    /// Check if success (2xx).
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }

    /// Check if redirect (3xx).
    pub fn is_redirect(&self) -> bool {
        (300..400).contains(&self.status)
    }

    /// Check if client error (4xx).
    pub fn is_client_error(&self) -> bool {
        (400..500).contains(&self.status)
    }

    /// Check if server error (5xx).
    pub fn is_server_error(&self) -> bool {
        (500..600).contains(&self.status)
    }

    /// Check if error (4xx or 5xx).
    pub fn is_error(&self) -> bool {
        self.is_client_error() || self.is_server_error()
    }

    /// Get body as text.
    pub fn text_body(&self) -> Result<String> {
        match &self.body {
            ResponseBody::Empty => Ok(String::new()),
            ResponseBody::Text(s) => Ok(s.clone()),
            ResponseBody::Bytes(b) => {
                String::from_utf8(b.clone()).map_err(|e| ResponseError::ParseError(e.to_string()))
            }
        }
    }

    /// Get body as bytes.
    pub fn bytes_body(&self) -> Vec<u8> {
        match &self.body {
            ResponseBody::Empty => Vec::new(),
            ResponseBody::Text(s) => s.clone().into_bytes(),
            ResponseBody::Bytes(b) => b.clone(),
        }
    }

    /// Get header value.
    pub fn get_header(&self, name: &str) -> Option<&String> {
        // Case-insensitive lookup
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v)
    }

    /// Get content type.
    pub fn content_type(&self) -> Option<&String> {
        self.get_header("Content-Type")
    }

    /// Get content length.
    pub fn content_length(&self) -> Option<usize> {
        self.get_header("Content-Length")
            .and_then(|v| v.parse().ok())
    }

    /// Check success or return error.
    pub fn error_for_status(self) -> Result<Self> {
        if self.is_success() {
            Ok(self)
        } else {
            let body = self.text_body().unwrap_or_default();
            Err(ResponseError::Status {
                status: self.status,
                message: body,
            })
        }
    }
}

/// Response body.
#[derive(Debug, Clone)]
pub enum ResponseBody {
    Empty,
    Text(String),
    Bytes(Vec<u8>),
}

impl ResponseBody {
    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        match self {
            Self::Empty => true,
            Self::Text(s) => s.is_empty(),
            Self::Bytes(b) => b.is_empty(),
        }
    }

    /// Get length.
    pub fn len(&self) -> usize {
        match self {
            Self::Empty => 0,
            Self::Text(s) => s.len(),
            Self::Bytes(b) => b.len(),
        }
    }
}

/// Response builder for creating test responses.
#[derive(Debug)]
pub struct ResponseBuilder {
    status: u16,
    headers: HashMap<String, String>,
    body: ResponseBody,
}

impl ResponseBuilder {
    /// Create OK response.
    pub fn ok() -> Self {
        Self::new(200)
    }

    /// Create created response.
    pub fn created() -> Self {
        Self::new(201)
    }

    /// Create no content response.
    pub fn no_content() -> Self {
        Self::new(204)
    }

    /// Create bad request response.
    pub fn bad_request() -> Self {
        Self::new(400)
    }

    /// Create unauthorized response.
    pub fn unauthorized() -> Self {
        Self::new(401)
    }

    /// Create forbidden response.
    pub fn forbidden() -> Self {
        Self::new(403)
    }

    /// Create not found response.
    pub fn not_found() -> Self {
        Self::new(404)
    }

    /// Create internal server error response.
    pub fn internal_error() -> Self {
        Self::new(500)
    }

    /// Create new builder.
    pub fn new(status: u16) -> Self {
        Self {
            status,
            headers: HashMap::new(),
            body: ResponseBody::Empty,
        }
    }

    /// Set header.
    pub fn header<K: Into<String>, V: Into<String>>(mut self, key: K, value: V) -> Self {
        self.headers.insert(key.into(), value.into());
        self
    }

    /// Set content type.
    pub fn content_type<S: Into<String>>(self, ct: S) -> Self {
        self.header("Content-Type", ct)
    }

    /// Set text body.
    pub fn text<S: Into<String>>(mut self, text: S) -> Self {
        self.body = ResponseBody::Text(text.into());
        self.content_type("text/plain")
    }

    /// Set JSON body.
    pub fn json<S: Into<String>>(mut self, json: S) -> Self {
        self.body = ResponseBody::Text(json.into());
        self.content_type("application/json")
    }

    /// Set bytes body.
    pub fn bytes<B: Into<Vec<u8>>>(mut self, bytes: B) -> Self {
        self.body = ResponseBody::Bytes(bytes.into());
        self.content_type("application/octet-stream")
    }

    /// Build response.
    pub fn build(self) -> Response {
        Response {
            status: self.status,
            headers: self.headers,
            body: self.body,
        }
    }
}

/// Response status classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusClass {
    Informational,
    Success,
    Redirect,
    ClientError,
    ServerError,
    Unknown,
}

impl StatusClass {
    /// Classify status code.
    pub fn from_status(status: u16) -> Self {
        match status {
            100..=199 => Self::Informational,
            200..=299 => Self::Success,
            300..=399 => Self::Redirect,
            400..=499 => Self::ClientError,
            500..=599 => Self::ServerError,
            _ => Self::Unknown,
        }
    }

    /// Check if retryable.
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::ServerError)
    }
}

/// Check if status code is retryable.
pub fn is_retryable_status(status: u16) -> bool {
    matches!(status, 408 | 425 | 429 | 500 | 502 | 503 | 504)
}

/// Get reason phrase for status code.
pub fn reason_phrase(status: u16) -> &'static str {
    match status {
        100 => "Continue",
        101 => "Switching Protocols",
        200 => "OK",
        201 => "Created",
        202 => "Accepted",
        204 => "No Content",
        301 => "Moved Permanently",
        302 => "Found",
        304 => "Not Modified",
        307 => "Temporary Redirect",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        409 => "Conflict",
        422 => "Unprocessable Entity",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        504 => "Gateway Timeout",
        _ => "Unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_response_status() {
        let resp = Response::new(200);
        assert!(resp.is_success());
        assert!(!resp.is_error());

        let resp = Response::new(404);
        assert!(resp.is_client_error());
        assert!(resp.is_error());
    }

    #[test]
    fn test_response_builder() {
        let resp = ResponseBuilder::ok().json(r#"{"success":true}"#).build();

        assert_eq!(resp.status, 200);
        assert_eq!(resp.content_type(), Some(&"application/json".to_string()));
    }

    #[test]
    fn test_error_for_status() {
        let resp = Response::new(200).text("OK");
        assert!(resp.error_for_status().is_ok());

        let resp = Response::new(404).text("Not Found");
        assert!(resp.error_for_status().is_err());
    }

    #[test]
    fn test_status_class() {
        assert_eq!(StatusClass::from_status(200), StatusClass::Success);
        assert_eq!(StatusClass::from_status(404), StatusClass::ClientError);
        assert!(StatusClass::ServerError.is_retryable());
    }

    #[test]
    fn test_retryable_status() {
        assert!(is_retryable_status(503));
        assert!(is_retryable_status(429));
        assert!(!is_retryable_status(404));
    }
}
