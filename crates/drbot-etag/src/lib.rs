//! ETag and conditional request handling for drbot.
//!
//! This crate provides:
//! - ETag generation
//! - If-Match/If-None-Match handling
//! - Last-Modified support
//! - Cache validation

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use thiserror::Error;

/// ETag error types.
#[derive(Error, Debug)]
pub enum EtagError {
    #[error("Invalid ETag format: {0}")]
    InvalidFormat(String),

    #[error("Precondition failed")]
    PreconditionFailed,

    #[error("Not modified")]
    NotModified,
}

/// Result type for ETag operations.
pub type Result<T> = std::result::Result<T, EtagError>;

/// ETag value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ETag {
    /// Tag value.
    value: String,
    /// Is weak.
    weak: bool,
}

impl ETag {
    /// Create a strong ETag.
    pub fn strong(value: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            weak: false,
        }
    }

    /// Create a weak ETag.
    pub fn weak(value: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            weak: true,
        }
    }

    /// Generate from hashable content.
    pub fn from_hash<T: Hash>(content: &T) -> Self {
        let mut hasher = DefaultHasher::new();
        content.hash(&mut hasher);
        Self::strong(format!("{:x}", hasher.finish()))
    }

    /// Generate from bytes.
    pub fn from_bytes(content: &[u8]) -> Self {
        let mut hasher = DefaultHasher::new();
        content.hash(&mut hasher);
        Self::strong(format!("{:x}", hasher.finish()))
    }

    /// Generate from multiple values.
    pub fn from_parts<T: Hash>(parts: &[T]) -> Self {
        let mut hasher = DefaultHasher::new();
        for part in parts {
            part.hash(&mut hasher);
        }
        Self::strong(format!("{:x}", hasher.finish()))
    }

    /// Parse from header value.
    pub fn parse(header: &str) -> Result<Self> {
        let header = header.trim();

        if let Some(value) = header.strip_prefix("W/") {
            let value = value.trim_matches('"');
            Ok(Self::weak(value))
        } else {
            let value = header.trim_matches('"');
            if value.is_empty() {
                return Err(EtagError::InvalidFormat("Empty ETag".to_string()));
            }
            Ok(Self::strong(value))
        }
    }

    /// Get the value.
    pub fn value(&self) -> &str {
        &self.value
    }

    /// Check if weak.
    pub fn is_weak(&self) -> bool {
        self.weak
    }

    /// Convert to header value.
    pub fn to_header(&self) -> String {
        if self.weak {
            format!("W/\"{}\"", self.value)
        } else {
            format!("\"{}\"", self.value)
        }
    }

    /// Strong comparison.
    pub fn strong_eq(&self, other: &ETag) -> bool {
        !self.weak && !other.weak && self.value == other.value
    }

    /// Weak comparison.
    pub fn weak_eq(&self, other: &ETag) -> bool {
        self.value == other.value
    }
}

impl std::fmt::Display for ETag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_header())
    }
}

/// Parse If-Match header.
pub fn parse_if_match(header: &str) -> Result<IfMatch> {
    let header = header.trim();

    if header == "*" {
        return Ok(IfMatch::Any);
    }

    let tags: Vec<ETag> = header
        .split(',')
        .filter_map(|s| ETag::parse(s.trim()).ok())
        .collect();

    if tags.is_empty() {
        Err(EtagError::InvalidFormat("No valid ETags".to_string()))
    } else {
        Ok(IfMatch::Tags(tags))
    }
}

/// If-Match header value.
#[derive(Debug, Clone)]
pub enum IfMatch {
    /// Match any.
    Any,
    /// Match specific tags.
    Tags(Vec<ETag>),
}

impl IfMatch {
    /// Check if matches.
    pub fn matches(&self, etag: &ETag) -> bool {
        match self {
            IfMatch::Any => true,
            IfMatch::Tags(tags) => tags.iter().any(|t| t.strong_eq(etag)),
        }
    }
}

/// Parse If-None-Match header.
pub fn parse_if_none_match(header: &str) -> Result<IfNoneMatch> {
    let header = header.trim();

    if header == "*" {
        return Ok(IfNoneMatch::Any);
    }

    let tags: Vec<ETag> = header
        .split(',')
        .filter_map(|s| ETag::parse(s.trim()).ok())
        .collect();

    if tags.is_empty() {
        Err(EtagError::InvalidFormat("No valid ETags".to_string()))
    } else {
        Ok(IfNoneMatch::Tags(tags))
    }
}

/// If-None-Match header value.
#[derive(Debug, Clone)]
pub enum IfNoneMatch {
    /// Match any (for PUT).
    Any,
    /// Match specific tags.
    Tags(Vec<ETag>),
}

impl IfNoneMatch {
    /// Check if matches (returns true if resource should NOT be returned).
    pub fn matches(&self, etag: &ETag) -> bool {
        match self {
            IfNoneMatch::Any => true,
            IfNoneMatch::Tags(tags) => tags.iter().any(|t| t.weak_eq(etag)),
        }
    }
}

/// Last-Modified handling.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LastModified(DateTime<Utc>);

impl LastModified {
    /// Create from DateTime.
    pub fn new(time: DateTime<Utc>) -> Self {
        Self(time)
    }

    /// Current time.
    pub fn now() -> Self {
        Self(Utc::now())
    }

    /// Get the datetime.
    pub fn datetime(&self) -> DateTime<Utc> {
        self.0
    }

    /// Format for HTTP header.
    pub fn to_header(&self) -> String {
        self.0.format("%a, %d %b %Y %H:%M:%S GMT").to_string()
    }

    /// Parse from HTTP header.
    pub fn parse(header: &str) -> Result<Self> {
        // Try common formats
        if let Ok(dt) = DateTime::parse_from_rfc2822(header) {
            return Ok(Self(dt.with_timezone(&Utc)));
        }

        // Try RFC 3339
        if let Ok(dt) = DateTime::parse_from_rfc3339(header) {
            return Ok(Self(dt.with_timezone(&Utc)));
        }

        Err(EtagError::InvalidFormat(format!(
            "Invalid date: {}",
            header
        )))
    }
}

/// Conditional request validator.
#[derive(Debug, Clone)]
pub struct Validator {
    etag: Option<ETag>,
    last_modified: Option<LastModified>,
}

impl Validator {
    /// Create new validator.
    pub fn new() -> Self {
        Self {
            etag: None,
            last_modified: None,
        }
    }

    /// Set ETag.
    pub fn with_etag(mut self, etag: ETag) -> Self {
        self.etag = Some(etag);
        self
    }

    /// Set Last-Modified.
    pub fn with_last_modified(mut self, last_modified: LastModified) -> Self {
        self.last_modified = Some(last_modified);
        self
    }

    /// Validate If-Match.
    pub fn validate_if_match(&self, header: &str) -> Result<()> {
        let if_match = parse_if_match(header)?;

        if let Some(ref etag) = self.etag {
            if !if_match.matches(etag) {
                return Err(EtagError::PreconditionFailed);
            }
        }

        Ok(())
    }

    /// Validate If-None-Match.
    pub fn validate_if_none_match(&self, header: &str) -> Result<()> {
        let if_none_match = parse_if_none_match(header)?;

        if let Some(ref etag) = self.etag {
            if if_none_match.matches(etag) {
                return Err(EtagError::NotModified);
            }
        }

        Ok(())
    }

    /// Validate If-Modified-Since.
    pub fn validate_if_modified_since(&self, header: &str) -> Result<()> {
        let since = LastModified::parse(header)?;

        if let Some(ref last_modified) = self.last_modified {
            if last_modified.0 <= since.0 {
                return Err(EtagError::NotModified);
            }
        }

        Ok(())
    }

    /// Validate If-Unmodified-Since.
    pub fn validate_if_unmodified_since(&self, header: &str) -> Result<()> {
        let since = LastModified::parse(header)?;

        if let Some(ref last_modified) = self.last_modified {
            if last_modified.0 > since.0 {
                return Err(EtagError::PreconditionFailed);
            }
        }

        Ok(())
    }
}

impl Default for Validator {
    fn default() -> Self {
        Self::new()
    }
}

/// Cache headers builder.
#[derive(Debug, Clone)]
pub struct CacheHeaders {
    /// ETag.
    pub etag: Option<ETag>,
    /// Last-Modified.
    pub last_modified: Option<LastModified>,
    /// Cache-Control.
    pub cache_control: Option<String>,
    /// Vary.
    pub vary: Vec<String>,
}

impl CacheHeaders {
    /// Create new builder.
    pub fn new() -> Self {
        Self {
            etag: None,
            last_modified: None,
            cache_control: None,
            vary: Vec::new(),
        }
    }

    /// Set ETag.
    pub fn with_etag(mut self, etag: ETag) -> Self {
        self.etag = Some(etag);
        self
    }

    /// Set Last-Modified.
    pub fn with_last_modified(mut self, last_modified: LastModified) -> Self {
        self.last_modified = Some(last_modified);
        self
    }

    /// Set Cache-Control.
    pub fn with_cache_control(mut self, value: impl Into<String>) -> Self {
        self.cache_control = Some(value.into());
        self
    }

    /// Add Vary header.
    pub fn vary(mut self, header: impl Into<String>) -> Self {
        self.vary.push(header.into());
        self
    }

    /// No cache.
    pub fn no_cache(mut self) -> Self {
        self.cache_control = Some("no-cache, no-store, must-revalidate".to_string());
        self
    }

    /// Max age.
    pub fn max_age(mut self, seconds: u64) -> Self {
        self.cache_control = Some(format!("max-age={}", seconds));
        self
    }

    /// Public with max age.
    pub fn public(mut self, max_age: u64) -> Self {
        self.cache_control = Some(format!("public, max-age={}", max_age));
        self
    }

    /// Private with max age.
    pub fn private(mut self, max_age: u64) -> Self {
        self.cache_control = Some(format!("private, max-age={}", max_age));
        self
    }
}

impl Default for CacheHeaders {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_etag_strong() {
        let etag = ETag::strong("abc123");
        assert!(!etag.is_weak());
        assert_eq!(etag.to_header(), "\"abc123\"");
    }

    #[test]
    fn test_etag_weak() {
        let etag = ETag::weak("abc123");
        assert!(etag.is_weak());
        assert_eq!(etag.to_header(), "W/\"abc123\"");
    }

    #[test]
    fn test_etag_parse() {
        let strong = ETag::parse("\"abc123\"").unwrap();
        assert!(!strong.is_weak());
        assert_eq!(strong.value(), "abc123");

        let weak = ETag::parse("W/\"abc123\"").unwrap();
        assert!(weak.is_weak());
    }

    #[test]
    fn test_etag_from_hash() {
        let etag1 = ETag::from_hash(&"content");
        let etag2 = ETag::from_hash(&"content");
        assert_eq!(etag1, etag2);

        let etag3 = ETag::from_hash(&"different");
        assert_ne!(etag1, etag3);
    }

    #[test]
    fn test_etag_comparison() {
        let strong1 = ETag::strong("abc");
        let strong2 = ETag::strong("abc");
        let weak1 = ETag::weak("abc");

        assert!(strong1.strong_eq(&strong2));
        assert!(!strong1.strong_eq(&weak1));
        assert!(strong1.weak_eq(&weak1));
    }

    #[test]
    fn test_if_match() {
        let if_match = parse_if_match("\"abc\", \"def\"").unwrap();

        let abc = ETag::strong("abc");
        let xyz = ETag::strong("xyz");

        assert!(if_match.matches(&abc));
        assert!(!if_match.matches(&xyz));
    }

    #[test]
    fn test_if_match_any() {
        let if_match = parse_if_match("*").unwrap();
        let etag = ETag::strong("anything");
        assert!(if_match.matches(&etag));
    }

    #[test]
    fn test_validator() {
        let etag = ETag::strong("v1");
        let validator = Validator::new().with_etag(etag);

        assert!(validator.validate_if_match("\"v1\"").is_ok());
        assert!(validator.validate_if_match("\"v2\"").is_err());
        assert!(validator.validate_if_none_match("\"v2\"").is_ok());
        assert!(validator.validate_if_none_match("\"v1\"").is_err());
    }

    #[test]
    fn test_cache_headers() {
        let headers = CacheHeaders::new()
            .with_etag(ETag::strong("abc"))
            .max_age(3600)
            .vary("Accept");

        assert!(headers.etag.is_some());
        assert_eq!(headers.cache_control, Some("max-age=3600".to_string()));
        assert!(headers.vary.contains(&"Accept".to_string()));
    }

    #[test]
    fn test_last_modified() {
        let lm = LastModified::now();
        let header = lm.to_header();
        assert!(header.contains("GMT"));
    }
}
