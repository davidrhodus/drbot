//! Content negotiation for drbot.
//!
//! This crate provides:
//! - Accept header parsing
//! - Content type matching
//! - Language negotiation
//! - Charset negotiation

use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use thiserror::Error;

/// Content negotiation error types.
#[derive(Error, Debug)]
pub enum NegotiationError {
    #[error("Invalid media type: {0}")]
    InvalidMediaType(String),

    #[error("No acceptable media type")]
    NotAcceptable,

    #[error("Invalid header: {0}")]
    InvalidHeader(String),
}

/// Result type for negotiation operations.
pub type Result<T> = std::result::Result<T, NegotiationError>;

/// Media type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaType {
    /// Type (e.g., "application").
    pub r#type: String,
    /// Subtype (e.g., "json").
    pub subtype: String,
    /// Parameters.
    pub params: Vec<(String, String)>,
}

impl MediaType {
    /// Create new media type.
    pub fn new(r#type: impl Into<String>, subtype: impl Into<String>) -> Self {
        Self {
            r#type: r#type.into(),
            subtype: subtype.into(),
            params: Vec::new(),
        }
    }

    /// Add parameter.
    pub fn with_param(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.params.push((key.into(), value.into()));
        self
    }

    /// Parse from string.
    pub fn parse(s: &str) -> Result<Self> {
        let s = s.trim();

        // Split type/subtype from params
        let mut parts = s.split(';');
        let type_part = parts
            .next()
            .ok_or_else(|| NegotiationError::InvalidMediaType(s.to_string()))?
            .trim();

        // Parse type/subtype
        let mut type_parts = type_part.split('/');
        let r#type = type_parts
            .next()
            .ok_or_else(|| NegotiationError::InvalidMediaType(s.to_string()))?
            .trim()
            .to_lowercase();
        let subtype = type_parts
            .next()
            .ok_or_else(|| NegotiationError::InvalidMediaType(s.to_string()))?
            .trim()
            .to_lowercase();

        // Parse params
        let mut params = Vec::new();
        for part in parts {
            if let Some((key, value)) = part.split_once('=') {
                params.push((
                    key.trim().to_lowercase(),
                    value.trim().trim_matches('"').to_string(),
                ));
            }
        }

        Ok(Self {
            r#type,
            subtype,
            params,
        })
    }

    /// Common media types.
    pub fn json() -> Self {
        Self::new("application", "json")
    }

    pub fn xml() -> Self {
        Self::new("application", "xml")
    }

    pub fn html() -> Self {
        Self::new("text", "html")
    }

    pub fn text() -> Self {
        Self::new("text", "plain")
    }

    pub fn form() -> Self {
        Self::new("application", "x-www-form-urlencoded")
    }

    pub fn multipart() -> Self {
        Self::new("multipart", "form-data")
    }

    pub fn octet_stream() -> Self {
        Self::new("application", "octet-stream")
    }

    /// Check if matches another media type.
    pub fn matches(&self, other: &MediaType) -> bool {
        let type_matches = self.r#type == "*" || other.r#type == "*" || self.r#type == other.r#type;
        let subtype_matches =
            self.subtype == "*" || other.subtype == "*" || self.subtype == other.subtype;
        type_matches && subtype_matches
    }

    /// Get specificity score.
    pub fn specificity(&self) -> u8 {
        let mut score = 0;
        if self.r#type != "*" {
            score += 2;
        }
        if self.subtype != "*" {
            score += 1;
        }
        score
    }

    /// Get parameter value.
    pub fn param(&self, key: &str) -> Option<&str> {
        self.params
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    /// Format as string.
    pub fn to_string(&self) -> String {
        let mut s = format!("{}/{}", self.r#type, self.subtype);
        for (key, value) in &self.params {
            if value.contains(' ') || value.contains(';') {
                s.push_str(&format!("; {}=\"{}\"", key, value));
            } else {
                s.push_str(&format!("; {}={}", key, value));
            }
        }
        s
    }
}

impl std::fmt::Display for MediaType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_string())
    }
}

/// Accept header entry with quality.
#[derive(Debug, Clone)]
pub struct AcceptEntry {
    /// Media type.
    pub media_type: MediaType,
    /// Quality (0.0 to 1.0).
    pub quality: f32,
}

impl AcceptEntry {
    /// Create new entry.
    pub fn new(media_type: MediaType, quality: f32) -> Self {
        Self {
            media_type,
            quality: quality.clamp(0.0, 1.0),
        }
    }

    /// Parse from string.
    pub fn parse(s: &str) -> Result<Self> {
        let s = s.trim();

        // Look for quality parameter
        let mut quality = 1.0f32;
        let mut media_str = s;

        // Find q= parameter
        if let Some(idx) = s.to_lowercase().find(";q=") {
            let (media_part, q_part) = s.split_at(idx);
            media_str = media_part;

            let q_value = &q_part[3..]; // Skip ";q="
            let q_end = q_value.find(';').unwrap_or(q_value.len());
            if let Ok(q) = q_value[..q_end].trim().parse::<f32>() {
                quality = q.clamp(0.0, 1.0);
            }
        }

        let media_type = MediaType::parse(media_str)?;
        Ok(Self {
            media_type,
            quality,
        })
    }
}

impl PartialEq for AcceptEntry {
    fn eq(&self, other: &Self) -> bool {
        (self.quality - other.quality).abs() < f32::EPSILON && self.media_type == other.media_type
    }
}

impl PartialOrd for AcceptEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        // Higher quality first
        match other.quality.partial_cmp(&self.quality) {
            Some(Ordering::Equal) => {
                // More specific first
                Some(
                    other
                        .media_type
                        .specificity()
                        .cmp(&self.media_type.specificity()),
                )
            }
            ord => ord,
        }
    }
}

/// Parse Accept header.
pub fn parse_accept(header: &str) -> Result<Vec<AcceptEntry>> {
    let mut entries = Vec::new();

    for part in header.split(',') {
        if let Ok(entry) = AcceptEntry::parse(part) {
            entries.push(entry);
        }
    }

    // Sort by quality and specificity
    entries.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));

    Ok(entries)
}

/// Content negotiator.
pub struct Negotiator {
    supported: Vec<MediaType>,
}

impl Negotiator {
    /// Create new negotiator.
    pub fn new(supported: Vec<MediaType>) -> Self {
        Self { supported }
    }

    /// Negotiate best match.
    pub fn negotiate(&self, accept_header: &str) -> Result<MediaType> {
        let accepted = parse_accept(accept_header)?;

        if accepted.is_empty() {
            return self
                .supported
                .first()
                .cloned()
                .ok_or(NegotiationError::NotAcceptable);
        }

        for entry in &accepted {
            for supported in &self.supported {
                if entry.media_type.matches(supported) {
                    return Ok(supported.clone());
                }
            }
        }

        Err(NegotiationError::NotAcceptable)
    }

    /// Check if accepts any of our supported types.
    pub fn accepts_any(&self, accept_header: &str) -> bool {
        self.negotiate(accept_header).is_ok()
    }
}

/// Language tag.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LanguageTag {
    /// Primary language.
    pub language: String,
    /// Region/country.
    pub region: Option<String>,
}

impl LanguageTag {
    /// Create new tag.
    pub fn new(language: impl Into<String>) -> Self {
        Self {
            language: language.into().to_lowercase(),
            region: None,
        }
    }

    /// With region.
    pub fn with_region(mut self, region: impl Into<String>) -> Self {
        self.region = Some(region.into().to_uppercase());
        self
    }

    /// Parse from string.
    pub fn parse(s: &str) -> Result<Self> {
        let s = s.trim();
        let mut parts = s.split('-');

        let language = parts
            .next()
            .ok_or_else(|| NegotiationError::InvalidHeader(s.to_string()))?
            .to_lowercase();

        let region = parts.next().map(|r| r.to_uppercase());

        Ok(Self { language, region })
    }

    /// Check if matches.
    pub fn matches(&self, other: &LanguageTag) -> bool {
        if self.language == "*" || other.language == "*" {
            return true;
        }

        if self.language != other.language {
            return false;
        }

        // If both have regions, they must match
        match (&self.region, &other.region) {
            (Some(a), Some(b)) => a == b,
            _ => true, // Wildcard region match
        }
    }

    /// Format as string.
    pub fn to_string(&self) -> String {
        match &self.region {
            Some(r) => format!("{}-{}", self.language, r),
            None => self.language.clone(),
        }
    }
}

impl std::fmt::Display for LanguageTag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_string())
    }
}

/// Language negotiator.
pub struct LanguageNegotiator {
    supported: Vec<LanguageTag>,
    default: LanguageTag,
}

impl LanguageNegotiator {
    /// Create new negotiator.
    pub fn new(supported: Vec<LanguageTag>, default: LanguageTag) -> Self {
        Self { supported, default }
    }

    /// Negotiate best match.
    pub fn negotiate(&self, accept_language: &str) -> LanguageTag {
        let mut entries: Vec<(LanguageTag, f32)> = Vec::new();

        for part in accept_language.split(',') {
            let part = part.trim();
            let (tag_str, quality) = if let Some(idx) = part.find(";q=") {
                let (t, q) = part.split_at(idx);
                let quality = q[3..].parse::<f32>().unwrap_or(1.0);
                (t.trim(), quality)
            } else {
                (part, 1.0)
            };

            if let Ok(tag) = LanguageTag::parse(tag_str) {
                entries.push((tag, quality));
            }
        }

        entries.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(Ordering::Equal));

        for (requested, _) in &entries {
            for supported in &self.supported {
                if requested.matches(supported) {
                    return supported.clone();
                }
            }
        }

        self.default.clone()
    }
}

/// Charset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Charset(String);

impl Charset {
    /// Create new charset.
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into().to_lowercase())
    }

    pub fn utf8() -> Self {
        Self::new("utf-8")
    }

    pub fn iso_8859_1() -> Self {
        Self::new("iso-8859-1")
    }

    pub fn name(&self) -> &str {
        &self.0
    }

    /// Check if matches.
    pub fn matches(&self, other: &Charset) -> bool {
        self.0 == "*" || other.0 == "*" || self.0 == other.0
    }
}

impl std::fmt::Display for Charset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_media_type_parse() {
        let mt = MediaType::parse("application/json").unwrap();
        assert_eq!(mt.r#type, "application");
        assert_eq!(mt.subtype, "json");
    }

    #[test]
    fn test_media_type_with_params() {
        let mt = MediaType::parse("text/html; charset=utf-8").unwrap();
        assert_eq!(mt.r#type, "text");
        assert_eq!(mt.param("charset"), Some("utf-8"));
    }

    #[test]
    fn test_media_type_matches() {
        let json = MediaType::json();
        let any = MediaType::new("*", "*");
        let app_any = MediaType::new("application", "*");

        assert!(any.matches(&json));
        assert!(app_any.matches(&json));
        assert!(!MediaType::xml().matches(&json));
    }

    #[test]
    fn test_accept_entry_parse() {
        let entry = AcceptEntry::parse("text/html;q=0.9").unwrap();
        assert_eq!(entry.quality, 0.9);
        assert_eq!(entry.media_type.r#type, "text");
    }

    #[test]
    fn test_parse_accept() {
        let entries = parse_accept("text/html, application/json;q=0.9, */*;q=0.1").unwrap();

        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].quality, 1.0); // text/html
        assert_eq!(entries[1].quality, 0.9); // application/json
    }

    #[test]
    fn test_negotiator() {
        let negotiator = Negotiator::new(vec![MediaType::json(), MediaType::xml()]);

        let result = negotiator
            .negotiate("application/json, application/xml;q=0.9")
            .unwrap();
        assert_eq!(result, MediaType::json());
    }

    #[test]
    fn test_negotiator_not_acceptable() {
        let negotiator = Negotiator::new(vec![MediaType::json()]);
        let result = negotiator.negotiate("text/html");
        assert!(matches!(result, Err(NegotiationError::NotAcceptable)));
    }

    #[test]
    fn test_language_tag() {
        let tag = LanguageTag::new("en").with_region("US");
        assert_eq!(tag.to_string(), "en-US");
    }

    #[test]
    fn test_language_tag_parse() {
        let tag = LanguageTag::parse("en-GB").unwrap();
        assert_eq!(tag.language, "en");
        assert_eq!(tag.region, Some("GB".to_string()));
    }

    #[test]
    fn test_language_negotiator() {
        let negotiator = LanguageNegotiator::new(
            vec![
                LanguageTag::new("en"),
                LanguageTag::new("de"),
                LanguageTag::new("fr"),
            ],
            LanguageTag::new("en"),
        );

        let result = negotiator.negotiate("de-DE, en;q=0.8");
        assert_eq!(result.language, "de");
    }

    #[test]
    fn test_charset() {
        let utf8 = Charset::utf8();
        let any = Charset::new("*");

        assert!(utf8.matches(&any));
        assert!(any.matches(&utf8));
    }

    #[test]
    fn test_media_type_specificity() {
        let any = MediaType::new("*", "*");
        let app_any = MediaType::new("application", "*");
        let json = MediaType::json();

        assert!(any.specificity() < app_any.specificity());
        assert!(app_any.specificity() < json.specificity());
    }
}
