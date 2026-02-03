//! HTTP header utilities for drbot.
//!
//! This crate provides:
//! - Header name constants
//! - Header value parsing
//! - Header map utilities

use std::collections::HashMap;
use thiserror::Error;

/// Header error types.
#[derive(Error, Debug, Clone)]
pub enum HeaderError {
    #[error("Invalid header name: {0}")]
    InvalidName(String),

    #[error("Invalid header value: {0}")]
    InvalidValue(String),

    #[error("Missing required header: {0}")]
    MissingHeader(String),
}

/// Result type for header operations.
pub type Result<T> = std::result::Result<T, HeaderError>;

/// Common header names.
pub mod names {
    pub const ACCEPT: &str = "Accept";
    pub const ACCEPT_CHARSET: &str = "Accept-Charset";
    pub const ACCEPT_ENCODING: &str = "Accept-Encoding";
    pub const ACCEPT_LANGUAGE: &str = "Accept-Language";
    pub const AUTHORIZATION: &str = "Authorization";
    pub const CACHE_CONTROL: &str = "Cache-Control";
    pub const CONNECTION: &str = "Connection";
    pub const CONTENT_ENCODING: &str = "Content-Encoding";
    pub const CONTENT_LENGTH: &str = "Content-Length";
    pub const CONTENT_TYPE: &str = "Content-Type";
    pub const COOKIE: &str = "Cookie";
    pub const DATE: &str = "Date";
    pub const ETAG: &str = "ETag";
    pub const HOST: &str = "Host";
    pub const IF_MATCH: &str = "If-Match";
    pub const IF_MODIFIED_SINCE: &str = "If-Modified-Since";
    pub const IF_NONE_MATCH: &str = "If-None-Match";
    pub const LAST_MODIFIED: &str = "Last-Modified";
    pub const LOCATION: &str = "Location";
    pub const ORIGIN: &str = "Origin";
    pub const REFERER: &str = "Referer";
    pub const SET_COOKIE: &str = "Set-Cookie";
    pub const TRANSFER_ENCODING: &str = "Transfer-Encoding";
    pub const USER_AGENT: &str = "User-Agent";
    pub const WWW_AUTHENTICATE: &str = "WWW-Authenticate";
    pub const X_FORWARDED_FOR: &str = "X-Forwarded-For";
    pub const X_REQUEST_ID: &str = "X-Request-ID";
}

/// Header map with case-insensitive lookup.
#[derive(Debug, Clone, Default)]
pub struct HeaderMap {
    headers: HashMap<String, Vec<String>>,
}

impl HeaderMap {
    /// Create new header map.
    pub fn new() -> Self {
        Self {
            headers: HashMap::new(),
        }
    }

    /// Insert header value.
    pub fn insert<K: Into<String>, V: Into<String>>(&mut self, key: K, value: V) {
        let key = normalize_name(key.into());
        self.headers.insert(key, vec![value.into()]);
    }

    /// Append header value.
    pub fn append<K: Into<String>, V: Into<String>>(&mut self, key: K, value: V) {
        let key = normalize_name(key.into());
        self.headers
            .entry(key)
            .or_insert_with(Vec::new)
            .push(value.into());
    }

    /// Get first header value.
    pub fn get(&self, key: &str) -> Option<&String> {
        let key = normalize_name(key.to_string());
        self.headers.get(&key).and_then(|v| v.first())
    }

    /// Get all header values.
    pub fn get_all(&self, key: &str) -> Option<&Vec<String>> {
        let key = normalize_name(key.to_string());
        self.headers.get(&key)
    }

    /// Check if header exists.
    pub fn contains(&self, key: &str) -> bool {
        let key = normalize_name(key.to_string());
        self.headers.contains_key(&key)
    }

    /// Remove header.
    pub fn remove(&mut self, key: &str) -> Option<Vec<String>> {
        let key = normalize_name(key.to_string());
        self.headers.remove(&key)
    }

    /// Get number of headers.
    pub fn len(&self) -> usize {
        self.headers.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.headers.is_empty()
    }

    /// Iterate over headers.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &Vec<String>)> {
        self.headers.iter()
    }

    /// Get typed content length.
    pub fn content_length(&self) -> Option<usize> {
        self.get(names::CONTENT_LENGTH).and_then(|v| v.parse().ok())
    }

    /// Get content type.
    pub fn content_type(&self) -> Option<ContentType> {
        self.get(names::CONTENT_TYPE).map(|v| ContentType::parse(v))
    }
}

/// Normalize header name to title case.
fn normalize_name(name: String) -> String {
    name.to_lowercase()
}

/// Content type header value.
#[derive(Debug, Clone)]
pub struct ContentType {
    pub media_type: String,
    pub params: HashMap<String, String>,
}

impl ContentType {
    /// Parse content type value.
    pub fn parse(value: &str) -> Self {
        let parts: Vec<&str> = value.split(';').map(|s| s.trim()).collect();
        let media_type = parts.first().map(|s| s.to_string()).unwrap_or_default();

        let mut params = HashMap::new();
        for part in parts.iter().skip(1) {
            if let Some(eq_pos) = part.find('=') {
                let key = part[..eq_pos].trim().to_lowercase();
                let value = part[eq_pos + 1..].trim().trim_matches('"').to_string();
                params.insert(key, value);
            }
        }

        Self { media_type, params }
    }

    /// Get charset parameter.
    pub fn charset(&self) -> Option<&String> {
        self.params.get("charset")
    }

    /// Get boundary parameter (for multipart).
    pub fn boundary(&self) -> Option<&String> {
        self.params.get("boundary")
    }

    /// Check if JSON.
    pub fn is_json(&self) -> bool {
        self.media_type == "application/json" || self.media_type.ends_with("+json")
    }

    /// Check if form data.
    pub fn is_form(&self) -> bool {
        self.media_type == "application/x-www-form-urlencoded"
    }

    /// Check if multipart.
    pub fn is_multipart(&self) -> bool {
        self.media_type.starts_with("multipart/")
    }
}

/// Authorization header value.
#[derive(Debug, Clone)]
pub enum Authorization {
    Bearer(String),
    Basic { username: String, password: String },
    Other { scheme: String, credentials: String },
}

impl Authorization {
    /// Parse authorization header.
    pub fn parse(value: &str) -> Result<Self> {
        let parts: Vec<&str> = value.splitn(2, ' ').collect();
        if parts.len() != 2 {
            return Err(HeaderError::InvalidValue(value.into()));
        }

        let scheme = parts[0];
        let credentials = parts[1];

        match scheme.to_lowercase().as_str() {
            "bearer" => Ok(Self::Bearer(credentials.to_string())),
            "basic" => {
                let decoded = base64_decode(credentials)
                    .map_err(|_| HeaderError::InvalidValue("Invalid base64".into()))?;
                let decoded_str = String::from_utf8(decoded)
                    .map_err(|_| HeaderError::InvalidValue("Invalid UTF-8".into()))?;

                let cred_parts: Vec<&str> = decoded_str.splitn(2, ':').collect();
                if cred_parts.len() != 2 {
                    return Err(HeaderError::InvalidValue(
                        "Invalid basic auth format".into(),
                    ));
                }

                Ok(Self::Basic {
                    username: cred_parts[0].to_string(),
                    password: cred_parts[1].to_string(),
                })
            }
            _ => Ok(Self::Other {
                scheme: scheme.to_string(),
                credentials: credentials.to_string(),
            }),
        }
    }

    /// Create bearer authorization.
    pub fn bearer<S: Into<String>>(token: S) -> Self {
        Self::Bearer(token.into())
    }

    /// Create basic authorization.
    pub fn basic<U: Into<String>, P: Into<String>>(username: U, password: P) -> Self {
        Self::Basic {
            username: username.into(),
            password: password.into(),
        }
    }

    /// Convert to header value.
    pub fn to_header_value(&self) -> String {
        match self {
            Self::Bearer(token) => format!("Bearer {}", token),
            Self::Basic { username, password } => {
                let credentials = format!("{}:{}", username, password);
                let encoded = base64_encode(credentials.as_bytes());
                format!("Basic {}", encoded)
            }
            Self::Other {
                scheme,
                credentials,
            } => {
                format!("{} {}", scheme, credentials)
            }
        }
    }
}

/// Cache-Control directive.
#[derive(Debug, Clone)]
pub struct CacheControl {
    pub no_cache: bool,
    pub no_store: bool,
    pub max_age: Option<u32>,
    pub s_maxage: Option<u32>,
    pub private: bool,
    pub public: bool,
    pub must_revalidate: bool,
}

impl CacheControl {
    /// Parse cache-control header.
    pub fn parse(value: &str) -> Self {
        let mut result = Self::default();

        for part in value.split(',').map(|s| s.trim()) {
            let lower = part.to_lowercase();
            if lower == "no-cache" {
                result.no_cache = true;
            } else if lower == "no-store" {
                result.no_store = true;
            } else if lower == "private" {
                result.private = true;
            } else if lower == "public" {
                result.public = true;
            } else if lower == "must-revalidate" {
                result.must_revalidate = true;
            } else if let Some(val) = lower.strip_prefix("max-age=") {
                result.max_age = val.parse().ok();
            } else if let Some(val) = lower.strip_prefix("s-maxage=") {
                result.s_maxage = val.parse().ok();
            }
        }

        result
    }

    /// Check if cacheable.
    pub fn is_cacheable(&self) -> bool {
        !self.no_store && !self.no_cache
    }
}

impl Default for CacheControl {
    fn default() -> Self {
        Self {
            no_cache: false,
            no_store: false,
            max_age: None,
            s_maxage: None,
            private: false,
            public: false,
            must_revalidate: false,
        }
    }
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

/// Simple base64 decoding.
fn base64_decode(data: &str) -> std::result::Result<Vec<u8>, ()> {
    const DECODE: [i8; 128] = [
        -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1,
        -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, 62, -1, -1,
        -1, 63, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, -1, -1, -1, -1, -1, -1, -1, 0, 1, 2, 3, 4,
        5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, -1, -1, -1,
        -1, -1, -1, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45,
        46, 47, 48, 49, 50, 51, -1, -1, -1, -1, -1,
    ];

    let mut result = Vec::new();
    let bytes: Vec<u8> = data.bytes().filter(|&b| b != b'=').collect();

    let mut i = 0;
    while i < bytes.len() {
        let b1 = bytes.get(i).copied().unwrap_or(0);
        let b2 = bytes.get(i + 1).copied().unwrap_or(0);
        let b3 = bytes.get(i + 2).copied().unwrap_or(0);
        let b4 = bytes.get(i + 3).copied().unwrap_or(0);

        if b1 >= 128 || b2 >= 128 || b3 >= 128 || b4 >= 128 {
            return Err(());
        }

        let v1 = DECODE[b1 as usize];
        let v2 = DECODE[b2 as usize];
        let v3 = DECODE[b3 as usize];
        let v4 = DECODE[b4 as usize];

        if v1 < 0 || v2 < 0 {
            return Err(());
        }

        result.push(((v1 as u8) << 2) | ((v2 as u8) >> 4));

        if i + 2 < bytes.len() && v3 >= 0 {
            result.push(((v2 as u8) << 4) | ((v3 as u8) >> 2));
        }

        if i + 3 < bytes.len() && v4 >= 0 {
            result.push(((v3 as u8) << 6) | (v4 as u8));
        }

        i += 4;
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_header_map() {
        let mut headers = HeaderMap::new();
        headers.insert("Content-Type", "application/json");
        headers.append("Accept", "text/html");
        headers.append("Accept", "application/json");

        assert_eq!(
            headers.get("content-type"),
            Some(&"application/json".to_string())
        );
        assert_eq!(headers.get_all("accept").map(|v| v.len()), Some(2));
    }

    #[test]
    fn test_content_type() {
        let ct = ContentType::parse("application/json; charset=utf-8");
        assert!(ct.is_json());
        assert_eq!(ct.charset(), Some(&"utf-8".to_string()));

        let ct = ContentType::parse("multipart/form-data; boundary=----WebKitFormBoundary");
        assert!(ct.is_multipart());
        assert!(ct.boundary().is_some());
    }

    #[test]
    fn test_authorization() {
        let auth = Authorization::bearer("my-token");
        assert_eq!(auth.to_header_value(), "Bearer my-token");

        let auth = Authorization::basic("user", "pass");
        let header = auth.to_header_value();
        assert!(header.starts_with("Basic "));

        let parsed = Authorization::parse(&header).unwrap();
        if let Authorization::Basic { username, password } = parsed {
            assert_eq!(username, "user");
            assert_eq!(password, "pass");
        } else {
            panic!("Expected Basic auth");
        }
    }

    #[test]
    fn test_cache_control() {
        let cc = CacheControl::parse("max-age=3600, public");
        assert_eq!(cc.max_age, Some(3600));
        assert!(cc.public);
        assert!(cc.is_cacheable());

        let cc = CacheControl::parse("no-store, no-cache");
        assert!(!cc.is_cacheable());
    }
}
