//! Request builder utilities for drbot.
//!
//! This crate provides:
//! - Request builder pattern
//! - Query string utilities
//! - Request configuration

use std::collections::HashMap;
use thiserror::Error;

/// Request error types.
#[derive(Error, Debug, Clone)]
pub enum RequestError {
    #[error("Missing required field: {0}")]
    MissingField(String),

    #[error("Invalid URL: {0}")]
    InvalidUrl(String),

    #[error("Invalid header: {0}")]
    InvalidHeader(String),

    #[error("Build error: {0}")]
    BuildError(String),
}

/// Result type for request operations.
pub type Result<T> = std::result::Result<T, RequestError>;

/// HTTP method (simplified).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
    Put,
    Delete,
    Patch,
    Head,
    Options,
}

impl Method {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Delete => "DELETE",
            Self::Patch => "PATCH",
            Self::Head => "HEAD",
            Self::Options => "OPTIONS",
        }
    }
}

/// Request body.
#[derive(Debug, Clone)]
pub enum Body {
    None,
    Text(String),
    Bytes(Vec<u8>),
    Json(String),
    Form(HashMap<String, String>),
}

impl Body {
    /// Check if body is empty.
    pub fn is_empty(&self) -> bool {
        match self {
            Self::None => true,
            Self::Text(s) | Self::Json(s) => s.is_empty(),
            Self::Bytes(b) => b.is_empty(),
            Self::Form(f) => f.is_empty(),
        }
    }

    /// Get content type for body.
    pub fn content_type(&self) -> Option<&'static str> {
        match self {
            Self::None => None,
            Self::Text(_) => Some("text/plain"),
            Self::Bytes(_) => Some("application/octet-stream"),
            Self::Json(_) => Some("application/json"),
            Self::Form(_) => Some("application/x-www-form-urlencoded"),
        }
    }

    /// Convert to bytes.
    pub fn into_bytes(self) -> Vec<u8> {
        match self {
            Self::None => Vec::new(),
            Self::Text(s) | Self::Json(s) => s.into_bytes(),
            Self::Bytes(b) => b,
            Self::Form(f) => encode_form(&f).into_bytes(),
        }
    }
}

/// Request builder.
#[derive(Debug, Clone)]
pub struct RequestBuilder {
    method: Method,
    url: String,
    headers: HashMap<String, String>,
    query: HashMap<String, String>,
    body: Body,
    timeout_ms: Option<u64>,
}

impl RequestBuilder {
    /// Create GET request.
    pub fn get<S: Into<String>>(url: S) -> Self {
        Self::new(Method::Get, url)
    }

    /// Create POST request.
    pub fn post<S: Into<String>>(url: S) -> Self {
        Self::new(Method::Post, url)
    }

    /// Create PUT request.
    pub fn put<S: Into<String>>(url: S) -> Self {
        Self::new(Method::Put, url)
    }

    /// Create DELETE request.
    pub fn delete<S: Into<String>>(url: S) -> Self {
        Self::new(Method::Delete, url)
    }

    /// Create PATCH request.
    pub fn patch<S: Into<String>>(url: S) -> Self {
        Self::new(Method::Patch, url)
    }

    /// Create new request builder.
    pub fn new<S: Into<String>>(method: Method, url: S) -> Self {
        Self {
            method,
            url: url.into(),
            headers: HashMap::new(),
            query: HashMap::new(),
            body: Body::None,
            timeout_ms: None,
        }
    }

    /// Add header.
    pub fn header<K: Into<String>, V: Into<String>>(mut self, key: K, value: V) -> Self {
        self.headers.insert(key.into(), value.into());
        self
    }

    /// Add multiple headers.
    pub fn headers<I, K, V>(mut self, headers: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        for (k, v) in headers {
            self.headers.insert(k.into(), v.into());
        }
        self
    }

    /// Set authorization header.
    pub fn bearer_auth<S: AsRef<str>>(self, token: S) -> Self {
        self.header("Authorization", format!("Bearer {}", token.as_ref()))
    }

    /// Set basic auth.
    pub fn basic_auth<U: AsRef<str>, P: AsRef<str>>(self, username: U, password: P) -> Self {
        let credentials = format!("{}:{}", username.as_ref(), password.as_ref());
        let encoded = base64_encode(credentials.as_bytes());
        self.header("Authorization", format!("Basic {}", encoded))
    }

    /// Add query parameter.
    pub fn query<K: Into<String>, V: Into<String>>(mut self, key: K, value: V) -> Self {
        self.query.insert(key.into(), value.into());
        self
    }

    /// Add multiple query parameters.
    pub fn query_params<I, K, V>(mut self, params: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        for (k, v) in params {
            self.query.insert(k.into(), v.into());
        }
        self
    }

    /// Set text body.
    pub fn text<S: Into<String>>(mut self, body: S) -> Self {
        self.body = Body::Text(body.into());
        self
    }

    /// Set bytes body.
    pub fn bytes<B: Into<Vec<u8>>>(mut self, body: B) -> Self {
        self.body = Body::Bytes(body.into());
        self
    }

    /// Set JSON body.
    pub fn json<S: Into<String>>(mut self, json: S) -> Self {
        self.body = Body::Json(json.into());
        self
    }

    /// Set form body.
    pub fn form(mut self, form: HashMap<String, String>) -> Self {
        self.body = Body::Form(form);
        self
    }

    /// Set timeout.
    pub fn timeout_ms(mut self, ms: u64) -> Self {
        self.timeout_ms = Some(ms);
        self
    }

    /// Build request.
    pub fn build(self) -> Result<Request> {
        if self.url.is_empty() {
            return Err(RequestError::MissingField("url".into()));
        }

        let mut headers = self.headers;

        // Add content type if not set and body has one
        if !headers.contains_key("Content-Type") {
            if let Some(ct) = self.body.content_type() {
                headers.insert("Content-Type".into(), ct.into());
            }
        }

        Ok(Request {
            method: self.method,
            url: self.url,
            headers,
            query: self.query,
            body: self.body,
            timeout_ms: self.timeout_ms,
        })
    }
}

/// Built request.
#[derive(Debug, Clone)]
pub struct Request {
    pub method: Method,
    pub url: String,
    pub headers: HashMap<String, String>,
    pub query: HashMap<String, String>,
    pub body: Body,
    pub timeout_ms: Option<u64>,
}

impl Request {
    /// Get full URL with query string.
    pub fn full_url(&self) -> String {
        if self.query.is_empty() {
            self.url.clone()
        } else {
            let qs = encode_query(&self.query);
            if self.url.contains('?') {
                format!("{}&{}", self.url, qs)
            } else {
                format!("{}?{}", self.url, qs)
            }
        }
    }

    /// Get body bytes.
    pub fn body_bytes(&self) -> Vec<u8> {
        self.body.clone().into_bytes()
    }
}

/// Query string builder.
#[derive(Debug, Clone, Default)]
pub struct QueryString {
    params: Vec<(String, String)>,
}

impl QueryString {
    /// Create new query string builder.
    pub fn new() -> Self {
        Self { params: Vec::new() }
    }

    /// Add parameter.
    pub fn param<K: Into<String>, V: Into<String>>(mut self, key: K, value: V) -> Self {
        self.params.push((key.into(), value.into()));
        self
    }

    /// Add optional parameter.
    pub fn param_opt<K: Into<String>, V: Into<String>>(mut self, key: K, value: Option<V>) -> Self {
        if let Some(v) = value {
            self.params.push((key.into(), v.into()));
        }
        self
    }

    /// Build query string.
    pub fn build(self) -> String {
        self.params
            .into_iter()
            .map(|(k, v)| format!("{}={}", url_encode(&k), url_encode(&v)))
            .collect::<Vec<_>>()
            .join("&")
    }
}

/// URL encode a string.
fn url_encode(s: &str) -> String {
    let mut result = String::new();
    for c in s.chars() {
        if c.is_ascii_alphanumeric() || "-_.~".contains(c) {
            result.push(c);
        } else {
            for b in c.to_string().as_bytes() {
                result.push_str(&format!("%{:02X}", b));
            }
        }
    }
    result
}

/// Encode form data.
fn encode_form(form: &HashMap<String, String>) -> String {
    form.iter()
        .map(|(k, v)| format!("{}={}", url_encode(k), url_encode(v)))
        .collect::<Vec<_>>()
        .join("&")
}

/// Encode query parameters.
fn encode_query(query: &HashMap<String, String>) -> String {
    encode_form(query)
}

/// Simple base64 encoding.
fn base64_encode(data: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut result = String::new();
    let mut i = 0;

    while i < data.len() {
        let b1 = data[i];
        let b2 = if i + 1 < data.len() { data[i + 1] } else { 0 };
        let b3 = if i + 2 < data.len() { data[i + 2] } else { 0 };

        result.push(CHARS[(b1 >> 2) as usize] as char);
        result.push(CHARS[(((b1 & 0x03) << 4) | (b2 >> 4)) as usize] as char);

        if i + 1 < data.len() {
            result.push(CHARS[(((b2 & 0x0F) << 2) | (b3 >> 6)) as usize] as char);
        } else {
            result.push('=');
        }

        if i + 2 < data.len() {
            result.push(CHARS[(b3 & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }

        i += 3;
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_builder() {
        let req = RequestBuilder::get("https://example.com/api")
            .header("Accept", "application/json")
            .query("page", "1")
            .build()
            .unwrap();

        assert_eq!(req.method, Method::Get);
        assert!(req.full_url().contains("page=1"));
    }

    #[test]
    fn test_post_with_json() {
        let req = RequestBuilder::post("https://example.com/api")
            .json(r#"{"name":"test"}"#)
            .build()
            .unwrap();

        assert_eq!(req.method, Method::Post);
        assert_eq!(
            req.headers.get("Content-Type"),
            Some(&"application/json".to_string())
        );
    }

    #[test]
    fn test_bearer_auth() {
        let req = RequestBuilder::get("https://example.com")
            .bearer_auth("my-token")
            .build()
            .unwrap();

        assert_eq!(
            req.headers.get("Authorization"),
            Some(&"Bearer my-token".to_string())
        );
    }

    #[test]
    fn test_query_string() {
        let qs = QueryString::new()
            .param("key", "value")
            .param("name", "hello world")
            .build();

        assert!(qs.contains("key=value"));
        assert!(qs.contains("hello%20world"));
    }
}
