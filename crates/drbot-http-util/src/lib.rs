//! HTTP utilities for drbot.
//!
//! This crate provides:
//! - HTTP method types
//! - Status code utilities
//! - Content type handling

use thiserror::Error;

/// HTTP error types.
#[derive(Error, Debug, Clone)]
pub enum HttpError {
    #[error("Invalid method: {0}")]
    InvalidMethod(String),

    #[error("Invalid status code: {0}")]
    InvalidStatusCode(u16),

    #[error("Invalid header: {0}")]
    InvalidHeader(String),

    #[error("Request failed: {0}")]
    RequestFailed(String),
}

/// Result type for HTTP operations.
pub type Result<T> = std::result::Result<T, HttpError>;

/// HTTP method.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Method {
    Get,
    Post,
    Put,
    Delete,
    Patch,
    Head,
    Options,
    Connect,
    Trace,
}

impl Method {
    /// Parse method from string.
    pub fn from_str(s: &str) -> Result<Self> {
        match s.to_uppercase().as_str() {
            "GET" => Ok(Self::Get),
            "POST" => Ok(Self::Post),
            "PUT" => Ok(Self::Put),
            "DELETE" => Ok(Self::Delete),
            "PATCH" => Ok(Self::Patch),
            "HEAD" => Ok(Self::Head),
            "OPTIONS" => Ok(Self::Options),
            "CONNECT" => Ok(Self::Connect),
            "TRACE" => Ok(Self::Trace),
            _ => Err(HttpError::InvalidMethod(s.to_string())),
        }
    }

    /// Convert to string.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Delete => "DELETE",
            Self::Patch => "PATCH",
            Self::Head => "HEAD",
            Self::Options => "OPTIONS",
            Self::Connect => "CONNECT",
            Self::Trace => "TRACE",
        }
    }

    /// Check if method is safe (no side effects).
    pub fn is_safe(&self) -> bool {
        matches!(self, Self::Get | Self::Head | Self::Options | Self::Trace)
    }

    /// Check if method is idempotent.
    pub fn is_idempotent(&self) -> bool {
        !matches!(self, Self::Post | Self::Patch)
    }

    /// Check if method typically has a body.
    pub fn has_body(&self) -> bool {
        matches!(self, Self::Post | Self::Put | Self::Patch)
    }
}

impl std::fmt::Display for Method {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// HTTP status code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StatusCode(u16);

impl StatusCode {
    // 1xx Informational
    pub const CONTINUE: Self = Self(100);
    pub const SWITCHING_PROTOCOLS: Self = Self(101);

    // 2xx Success
    pub const OK: Self = Self(200);
    pub const CREATED: Self = Self(201);
    pub const ACCEPTED: Self = Self(202);
    pub const NO_CONTENT: Self = Self(204);

    // 3xx Redirection
    pub const MOVED_PERMANENTLY: Self = Self(301);
    pub const FOUND: Self = Self(302);
    pub const SEE_OTHER: Self = Self(303);
    pub const NOT_MODIFIED: Self = Self(304);
    pub const TEMPORARY_REDIRECT: Self = Self(307);
    pub const PERMANENT_REDIRECT: Self = Self(308);

    // 4xx Client Error
    pub const BAD_REQUEST: Self = Self(400);
    pub const UNAUTHORIZED: Self = Self(401);
    pub const FORBIDDEN: Self = Self(403);
    pub const NOT_FOUND: Self = Self(404);
    pub const METHOD_NOT_ALLOWED: Self = Self(405);
    pub const CONFLICT: Self = Self(409);
    pub const GONE: Self = Self(410);
    pub const UNPROCESSABLE_ENTITY: Self = Self(422);
    pub const TOO_MANY_REQUESTS: Self = Self(429);

    // 5xx Server Error
    pub const INTERNAL_SERVER_ERROR: Self = Self(500);
    pub const NOT_IMPLEMENTED: Self = Self(501);
    pub const BAD_GATEWAY: Self = Self(502);
    pub const SERVICE_UNAVAILABLE: Self = Self(503);
    pub const GATEWAY_TIMEOUT: Self = Self(504);

    /// Create from number.
    pub fn from_u16(code: u16) -> Result<Self> {
        if (100..600).contains(&code) {
            Ok(Self(code))
        } else {
            Err(HttpError::InvalidStatusCode(code))
        }
    }

    /// Get numeric value.
    pub fn as_u16(&self) -> u16 {
        self.0
    }

    /// Check if informational (1xx).
    pub fn is_informational(&self) -> bool {
        (100..200).contains(&self.0)
    }

    /// Check if success (2xx).
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.0)
    }

    /// Check if redirection (3xx).
    pub fn is_redirection(&self) -> bool {
        (300..400).contains(&self.0)
    }

    /// Check if client error (4xx).
    pub fn is_client_error(&self) -> bool {
        (400..500).contains(&self.0)
    }

    /// Check if server error (5xx).
    pub fn is_server_error(&self) -> bool {
        (500..600).contains(&self.0)
    }

    /// Check if error (4xx or 5xx).
    pub fn is_error(&self) -> bool {
        self.is_client_error() || self.is_server_error()
    }

    /// Get reason phrase.
    pub fn reason_phrase(&self) -> &'static str {
        match self.0 {
            100 => "Continue",
            101 => "Switching Protocols",
            200 => "OK",
            201 => "Created",
            202 => "Accepted",
            204 => "No Content",
            301 => "Moved Permanently",
            302 => "Found",
            303 => "See Other",
            304 => "Not Modified",
            307 => "Temporary Redirect",
            308 => "Permanent Redirect",
            400 => "Bad Request",
            401 => "Unauthorized",
            403 => "Forbidden",
            404 => "Not Found",
            405 => "Method Not Allowed",
            409 => "Conflict",
            410 => "Gone",
            422 => "Unprocessable Entity",
            429 => "Too Many Requests",
            500 => "Internal Server Error",
            501 => "Not Implemented",
            502 => "Bad Gateway",
            503 => "Service Unavailable",
            504 => "Gateway Timeout",
            _ => "Unknown",
        }
    }
}

impl std::fmt::Display for StatusCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}", self.0, self.reason_phrase())
    }
}

/// Common content types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentType {
    pub media_type: String,
    pub charset: Option<String>,
}

impl ContentType {
    // Common types
    pub const TEXT_PLAIN: &'static str = "text/plain";
    pub const TEXT_HTML: &'static str = "text/html";
    pub const TEXT_CSS: &'static str = "text/css";
    pub const TEXT_JAVASCRIPT: &'static str = "text/javascript";
    pub const APPLICATION_JSON: &'static str = "application/json";
    pub const APPLICATION_XML: &'static str = "application/xml";
    pub const APPLICATION_FORM: &'static str = "application/x-www-form-urlencoded";
    pub const MULTIPART_FORM: &'static str = "multipart/form-data";
    pub const APPLICATION_OCTET_STREAM: &'static str = "application/octet-stream";
    pub const IMAGE_PNG: &'static str = "image/png";
    pub const IMAGE_JPEG: &'static str = "image/jpeg";
    pub const IMAGE_GIF: &'static str = "image/gif";
    pub const IMAGE_WEBP: &'static str = "image/webp";

    /// Create new content type.
    pub fn new<S: Into<String>>(media_type: S) -> Self {
        Self {
            media_type: media_type.into(),
            charset: None,
        }
    }

    /// Set charset.
    pub fn with_charset<S: Into<String>>(mut self, charset: S) -> Self {
        self.charset = Some(charset.into());
        self
    }

    /// Parse content type header.
    pub fn parse(value: &str) -> Self {
        let parts: Vec<&str> = value.split(';').map(|s| s.trim()).collect();
        let media_type = parts.first().map(|s| s.to_string()).unwrap_or_default();

        let charset = parts.iter().skip(1).find_map(|part| {
            let kv: Vec<&str> = part.splitn(2, '=').collect();
            if kv.len() == 2 && kv[0].trim().eq_ignore_ascii_case("charset") {
                Some(kv[1].trim().trim_matches('"').to_string())
            } else {
                None
            }
        });

        Self {
            media_type,
            charset,
        }
    }

    /// Check if JSON.
    pub fn is_json(&self) -> bool {
        self.media_type == Self::APPLICATION_JSON || self.media_type.ends_with("+json")
    }

    /// Check if XML.
    pub fn is_xml(&self) -> bool {
        self.media_type == Self::APPLICATION_XML
            || self.media_type == "text/xml"
            || self.media_type.ends_with("+xml")
    }

    /// Check if text.
    pub fn is_text(&self) -> bool {
        self.media_type.starts_with("text/")
    }

    /// Check if image.
    pub fn is_image(&self) -> bool {
        self.media_type.starts_with("image/")
    }

    /// Convert to header value.
    pub fn to_header_value(&self) -> String {
        match &self.charset {
            Some(cs) => format!("{}; charset={}", self.media_type, cs),
            None => self.media_type.clone(),
        }
    }
}

impl std::fmt::Display for ContentType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_header_value())
    }
}

/// HTTP version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpVersion {
    Http10,
    Http11,
    Http2,
    Http3,
}

impl HttpVersion {
    /// Parse version string.
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "HTTP/1.0" => Some(Self::Http10),
            "HTTP/1.1" => Some(Self::Http11),
            "HTTP/2" | "HTTP/2.0" => Some(Self::Http2),
            "HTTP/3" | "HTTP/3.0" => Some(Self::Http3),
            _ => None,
        }
    }

    /// Convert to string.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Http10 => "HTTP/1.0",
            Self::Http11 => "HTTP/1.1",
            Self::Http2 => "HTTP/2",
            Self::Http3 => "HTTP/3",
        }
    }
}

impl std::fmt::Display for HttpVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Check if URL needs encoding.
pub fn needs_url_encoding(s: &str) -> bool {
    s.chars()
        .any(|c| !c.is_ascii_alphanumeric() && !"-_.~".contains(c))
}

/// Simple URL encoding.
pub fn url_encode(s: &str) -> String {
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

/// Simple URL decoding.
pub fn url_decode(s: &str) -> Result<String> {
    let mut result = Vec::new();
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '%' {
            let hex: String = chars.by_ref().take(2).collect();
            if hex.len() == 2 {
                if let Ok(b) = u8::from_str_radix(&hex, 16) {
                    result.push(b);
                } else {
                    return Err(HttpError::InvalidHeader("Invalid URL encoding".into()));
                }
            }
        } else if c == '+' {
            result.push(b' ');
        } else {
            result.push(c as u8);
        }
    }

    String::from_utf8(result).map_err(|_| HttpError::InvalidHeader("Invalid UTF-8 in URL".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_method() {
        assert_eq!(Method::from_str("GET").unwrap(), Method::Get);
        assert_eq!(Method::Post.as_str(), "POST");
        assert!(Method::Get.is_safe());
        assert!(!Method::Post.is_safe());
        assert!(Method::Post.has_body());
    }

    #[test]
    fn test_status_code() {
        assert!(StatusCode::OK.is_success());
        assert!(StatusCode::NOT_FOUND.is_client_error());
        assert!(StatusCode::INTERNAL_SERVER_ERROR.is_server_error());
        assert_eq!(StatusCode::OK.reason_phrase(), "OK");
    }

    #[test]
    fn test_content_type() {
        let ct = ContentType::parse("application/json; charset=utf-8");
        assert!(ct.is_json());
        assert_eq!(ct.charset, Some("utf-8".into()));

        let ct2 = ContentType::new("text/html").with_charset("utf-8");
        assert!(ct2.is_text());
    }

    #[test]
    fn test_url_encoding() {
        assert_eq!(url_encode("hello world"), "hello%20world");
        assert_eq!(url_decode("hello%20world").unwrap(), "hello world");
    }
}
