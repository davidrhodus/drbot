//! Input sanitization for drbot.
//!
//! This crate provides:
//! - HTML sanitization
//! - SQL injection prevention
//! - XSS protection
//! - Input cleaning

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use thiserror::Error;

/// Sanitization error types.
#[derive(Error, Debug)]
pub enum SanitizeError {
    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Blocked content detected")]
    BlockedContent,

    #[error("Regex error: {0}")]
    RegexError(String),
}

/// Result type for sanitization operations.
pub type Result<T> = std::result::Result<T, SanitizeError>;

/// Sanitizer trait.
pub trait Sanitizer: Send + Sync {
    /// Sanitize input.
    fn sanitize(&self, input: &str) -> String;
}

/// Trim whitespace sanitizer.
pub struct TrimSanitizer;

impl Sanitizer for TrimSanitizer {
    fn sanitize(&self, input: &str) -> String {
        input.trim().to_string()
    }
}

/// HTML entity encoder.
pub struct HtmlEncoder;

impl HtmlEncoder {
    /// Encode HTML entities.
    pub fn encode(input: &str) -> String {
        input
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
            .replace('\'', "&#x27;")
    }

    /// Decode HTML entities.
    pub fn decode(input: &str) -> String {
        input
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&#x27;", "'")
            .replace("&amp;", "&")
    }
}

impl Sanitizer for HtmlEncoder {
    fn sanitize(&self, input: &str) -> String {
        Self::encode(input)
    }
}

/// HTML tag stripper.
pub struct HtmlStripper {
    regex: Regex,
}

impl HtmlStripper {
    /// Create new stripper.
    pub fn new() -> Self {
        Self {
            regex: Regex::new(r"<[^>]*>").unwrap(),
        }
    }
}

impl Default for HtmlStripper {
    fn default() -> Self {
        Self::new()
    }
}

impl Sanitizer for HtmlStripper {
    fn sanitize(&self, input: &str) -> String {
        self.regex.replace_all(input, "").to_string()
    }
}

/// HTML sanitizer with allowed tags.
pub struct HtmlSanitizer {
    allowed_tags: HashSet<String>,
    allowed_attributes: HashSet<String>,
}

impl HtmlSanitizer {
    /// Create new sanitizer.
    pub fn new() -> Self {
        Self {
            allowed_tags: HashSet::new(),
            allowed_attributes: HashSet::new(),
        }
    }

    /// Allow a tag.
    pub fn allow_tag(mut self, tag: impl Into<String>) -> Self {
        self.allowed_tags.insert(tag.into().to_lowercase());
        self
    }

    /// Allow tags.
    pub fn allow_tags<S: Into<String>>(mut self, tags: Vec<S>) -> Self {
        for tag in tags {
            self.allowed_tags.insert(tag.into().to_lowercase());
        }
        self
    }

    /// Allow an attribute.
    pub fn allow_attribute(mut self, attr: impl Into<String>) -> Self {
        self.allowed_attributes.insert(attr.into().to_lowercase());
        self
    }

    /// Basic safe config.
    pub fn basic() -> Self {
        Self::new()
            .allow_tags(vec!["b", "i", "u", "em", "strong", "a", "p", "br"])
            .allow_attribute("href")
    }

    /// Extended safe config.
    pub fn extended() -> Self {
        Self::new()
            .allow_tags(vec![
                "b",
                "i",
                "u",
                "em",
                "strong",
                "a",
                "p",
                "br",
                "ul",
                "ol",
                "li",
                "h1",
                "h2",
                "h3",
                "h4",
                "h5",
                "h6",
                "blockquote",
                "code",
                "pre",
            ])
            .allow_attribute("href")
            .allow_attribute("class")
    }

    /// Check if tag is allowed.
    pub fn is_tag_allowed(&self, tag: &str) -> bool {
        self.allowed_tags.contains(&tag.to_lowercase())
    }
}

impl Default for HtmlSanitizer {
    fn default() -> Self {
        Self::new()
    }
}

impl Sanitizer for HtmlSanitizer {
    fn sanitize(&self, input: &str) -> String {
        // Simple implementation - encode all HTML if no tags allowed
        if self.allowed_tags.is_empty() {
            return HtmlEncoder::encode(input);
        }

        // For now, just strip all tags (a proper implementation would parse and filter)
        let stripper = HtmlStripper::new();
        stripper.sanitize(input)
    }
}

/// SQL sanitizer.
pub struct SqlSanitizer;

impl SqlSanitizer {
    /// Escape SQL string.
    pub fn escape(input: &str) -> String {
        input
            .replace('\'', "''")
            .replace('\\', "\\\\")
            .replace('\0', "\\0")
            .replace('\n', "\\n")
            .replace('\r', "\\r")
            .replace('\x1a', "\\Z")
    }

    /// Escape SQL identifier.
    pub fn escape_identifier(input: &str) -> String {
        format!("\"{}\"", input.replace('"', "\"\""))
    }
}

impl Sanitizer for SqlSanitizer {
    fn sanitize(&self, input: &str) -> String {
        Self::escape(input)
    }
}

/// URL sanitizer.
pub struct UrlSanitizer {
    allowed_schemes: HashSet<String>,
}

impl UrlSanitizer {
    /// Create new sanitizer.
    pub fn new() -> Self {
        let mut allowed = HashSet::new();
        allowed.insert("http".to_string());
        allowed.insert("https".to_string());
        allowed.insert("mailto".to_string());

        Self {
            allowed_schemes: allowed,
        }
    }

    /// Allow scheme.
    pub fn allow_scheme(mut self, scheme: impl Into<String>) -> Self {
        self.allowed_schemes.insert(scheme.into().to_lowercase());
        self
    }

    /// Validate URL.
    pub fn validate(&self, url: &str) -> bool {
        let url_lower = url.to_lowercase();

        // Check for javascript: and data: schemes
        if url_lower.starts_with("javascript:") || url_lower.starts_with("data:") {
            return false;
        }

        // Check allowed schemes
        for scheme in &self.allowed_schemes {
            if url_lower.starts_with(&format!("{}:", scheme)) {
                return true;
            }
        }

        // Allow relative URLs
        url.starts_with('/') || url.starts_with('#') || !url.contains(':')
    }
}

impl Default for UrlSanitizer {
    fn default() -> Self {
        Self::new()
    }
}

impl Sanitizer for UrlSanitizer {
    fn sanitize(&self, input: &str) -> String {
        if self.validate(input) {
            input.to_string()
        } else {
            String::new()
        }
    }
}

/// Filename sanitizer.
pub struct FilenameSanitizer {
    max_length: usize,
}

impl FilenameSanitizer {
    /// Create new sanitizer.
    pub fn new() -> Self {
        Self { max_length: 255 }
    }

    /// Set max length.
    pub fn max_length(mut self, len: usize) -> Self {
        self.max_length = len;
        self
    }
}

impl Default for FilenameSanitizer {
    fn default() -> Self {
        Self::new()
    }
}

impl Sanitizer for FilenameSanitizer {
    fn sanitize(&self, input: &str) -> String {
        let mut result: String = input
            .chars()
            .filter(|c| {
                !matches!(
                    c,
                    '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\0'
                )
            })
            .collect();

        // Remove leading/trailing dots and spaces
        result = result.trim_matches(|c| c == '.' || c == ' ').to_string();

        // Truncate if needed
        if result.len() > self.max_length {
            result.truncate(self.max_length);
        }

        // Ensure not empty
        if result.is_empty() {
            result = "unnamed".to_string();
        }

        result
    }
}

/// Whitespace normalizer.
pub struct WhitespaceNormalizer;

impl Sanitizer for WhitespaceNormalizer {
    fn sanitize(&self, input: &str) -> String {
        let re = Regex::new(r"\s+").unwrap();
        re.replace_all(input.trim(), " ").to_string()
    }
}

/// Control character remover.
pub struct ControlCharRemover;

impl Sanitizer for ControlCharRemover {
    fn sanitize(&self, input: &str) -> String {
        input
            .chars()
            .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
            .collect()
    }
}

/// Sanitizer chain.
pub struct SanitizerChain {
    sanitizers: Vec<Box<dyn Sanitizer>>,
}

impl SanitizerChain {
    /// Create new chain.
    pub fn new() -> Self {
        Self {
            sanitizers: Vec::new(),
        }
    }

    /// Add sanitizer.
    pub fn add<S: Sanitizer + 'static>(mut self, sanitizer: S) -> Self {
        self.sanitizers.push(Box::new(sanitizer));
        self
    }

    /// Run chain.
    pub fn run(&self, input: &str) -> String {
        let mut result = input.to_string();
        for sanitizer in &self.sanitizers {
            result = sanitizer.sanitize(&result);
        }
        result
    }
}

impl Default for SanitizerChain {
    fn default() -> Self {
        Self::new()
    }
}

impl Sanitizer for SanitizerChain {
    fn sanitize(&self, input: &str) -> String {
        self.run(input)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trim() {
        let sanitizer = TrimSanitizer;
        assert_eq!(sanitizer.sanitize("  hello  "), "hello");
    }

    #[test]
    fn test_html_encode() {
        let result = HtmlEncoder::encode("<script>alert('xss')</script>");
        assert!(!result.contains('<'));
        assert!(!result.contains('>'));
    }

    #[test]
    fn test_html_decode() {
        let encoded = "&lt;b&gt;hello&lt;/b&gt;";
        let decoded = HtmlEncoder::decode(encoded);
        assert_eq!(decoded, "<b>hello</b>");
    }

    #[test]
    fn test_html_stripper() {
        let sanitizer = HtmlStripper::new();
        let result = sanitizer.sanitize("<p>Hello <b>World</b></p>");
        assert_eq!(result, "Hello World");
    }

    #[test]
    fn test_sql_escape() {
        let result = SqlSanitizer::escape("O'Reilly");
        assert_eq!(result, "O''Reilly");
    }

    #[test]
    fn test_url_sanitizer() {
        let sanitizer = UrlSanitizer::new();

        assert!(sanitizer.validate("https://example.com"));
        assert!(sanitizer.validate("/path/to/page"));
        assert!(!sanitizer.validate("javascript:alert(1)"));
        assert!(!sanitizer.validate("data:text/html,<script>"));
    }

    #[test]
    fn test_filename_sanitizer() {
        let sanitizer = FilenameSanitizer::new();

        assert_eq!(sanitizer.sanitize("file.txt"), "file.txt");
        assert_eq!(sanitizer.sanitize("../../../etc/passwd"), "etcpasswd");
        assert_eq!(sanitizer.sanitize("file<name>.txt"), "filename.txt");
    }

    #[test]
    fn test_whitespace_normalizer() {
        let sanitizer = WhitespaceNormalizer;
        assert_eq!(
            sanitizer.sanitize("hello    world\n\ntest"),
            "hello world test"
        );
    }

    #[test]
    fn test_control_char_remover() {
        let sanitizer = ControlCharRemover;
        let input = "hello\x00world\x1f";
        let result = sanitizer.sanitize(input);
        assert_eq!(result, "helloworld");
    }

    #[test]
    fn test_chain() {
        let chain = SanitizerChain::new()
            .add(TrimSanitizer)
            .add(WhitespaceNormalizer)
            .add(HtmlEncoder);

        let result = chain.run("  <b>hello   world</b>  ");
        assert!(!result.contains('<'));
    }

    #[test]
    fn test_html_sanitizer_basic() {
        let sanitizer = HtmlSanitizer::basic();
        assert!(sanitizer.is_tag_allowed("b"));
        assert!(sanitizer.is_tag_allowed("a"));
        assert!(!sanitizer.is_tag_allowed("script"));
    }
}
