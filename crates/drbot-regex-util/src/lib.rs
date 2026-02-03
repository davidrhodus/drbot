//! Regex utilities for drbot.
//!
//! This crate provides:
//! - Common regex patterns
//! - Regex builder
//! - Match utilities

use regex::{Captures, Regex, RegexBuilder};
use std::collections::HashMap;
use thiserror::Error;

/// Regex error types.
#[derive(Error, Debug)]
pub enum RegexError {
    #[error("Invalid regex: {0}")]
    Invalid(#[from] regex::Error),

    #[error("No match found")]
    NoMatch,
}

/// Result type for regex operations.
pub type Result<T> = std::result::Result<T, RegexError>;

/// Common regex patterns.
pub struct Patterns;

impl Patterns {
    /// Email pattern.
    pub fn email() -> &'static str {
        r"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$"
    }

    /// URL pattern.
    pub fn url() -> &'static str {
        r"^https?://[^\s/$.?#].[^\s]*$"
    }

    /// IPv4 address pattern.
    pub fn ipv4() -> &'static str {
        r"^(?:(?:25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)\.){3}(?:25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)$"
    }

    /// IPv6 address pattern (simplified).
    pub fn ipv6() -> &'static str {
        r"^(?:[0-9a-fA-F]{1,4}:){7}[0-9a-fA-F]{1,4}$"
    }

    /// UUID pattern.
    pub fn uuid() -> &'static str {
        r"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$"
    }

    /// Phone number pattern (US format).
    pub fn phone_us() -> &'static str {
        r"^(?:\+1)?[-.\s]?\(?[0-9]{3}\)?[-.\s]?[0-9]{3}[-.\s]?[0-9]{4}$"
    }

    /// Date pattern (YYYY-MM-DD).
    pub fn date_iso() -> &'static str {
        r"^\d{4}-(?:0[1-9]|1[0-2])-(?:0[1-9]|[12]\d|3[01])$"
    }

    /// Time pattern (HH:MM:SS).
    pub fn time_24h() -> &'static str {
        r"^(?:[01]\d|2[0-3]):[0-5]\d:[0-5]\d$"
    }

    /// Datetime pattern (ISO 8601).
    pub fn datetime_iso() -> &'static str {
        r"^\d{4}-(?:0[1-9]|1[0-2])-(?:0[1-9]|[12]\d|3[01])T(?:[01]\d|2[0-3]):[0-5]\d:[0-5]\d(?:\.\d+)?(?:Z|[+-](?:[01]\d|2[0-3]):[0-5]\d)?$"
    }

    /// Hex color pattern.
    pub fn hex_color() -> &'static str {
        r"^#?(?:[0-9a-fA-F]{3}){1,2}$"
    }

    /// Credit card pattern (simplified).
    pub fn credit_card() -> &'static str {
        r"^\d{4}[-\s]?\d{4}[-\s]?\d{4}[-\s]?\d{4}$"
    }

    /// Slug pattern.
    pub fn slug() -> &'static str {
        r"^[a-z0-9]+(?:-[a-z0-9]+)*$"
    }

    /// Username pattern (alphanumeric with underscore).
    pub fn username() -> &'static str {
        r"^[a-zA-Z][a-zA-Z0-9_]{2,29}$"
    }

    /// Strong password pattern.
    pub fn password_strong() -> &'static str {
        r"^(?=.*[a-z])(?=.*[A-Z])(?=.*\d)(?=.*[@$!%*?&])[A-Za-z\d@$!%*?&]{8,}$"
    }

    /// Semantic version pattern.
    pub fn semver() -> &'static str {
        r"^v?(\d+)\.(\d+)\.(\d+)(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?(?:\+([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?$"
    }

    /// Whitespace pattern.
    pub fn whitespace() -> &'static str {
        r"\s+"
    }

    /// Word boundary pattern.
    pub fn word() -> &'static str {
        r"\b\w+\b"
    }

    /// Integer pattern.
    pub fn integer() -> &'static str {
        r"^-?\d+$"
    }

    /// Decimal pattern.
    pub fn decimal() -> &'static str {
        r"^-?\d+(?:\.\d+)?$"
    }
}

/// Regex validator.
pub struct Validator {
    patterns: HashMap<String, Regex>,
}

impl Validator {
    /// Create new validator.
    pub fn new() -> Self {
        Self {
            patterns: HashMap::new(),
        }
    }

    /// Add pattern.
    pub fn add_pattern(mut self, name: &str, pattern: &str) -> Result<Self> {
        let regex = Regex::new(pattern)?;
        self.patterns.insert(name.to_string(), regex);
        Ok(self)
    }

    /// Validate value against pattern.
    pub fn validate(&self, name: &str, value: &str) -> bool {
        self.patterns
            .get(name)
            .map(|r| r.is_match(value))
            .unwrap_or(false)
    }

    /// Get all pattern names.
    pub fn pattern_names(&self) -> Vec<&str> {
        self.patterns.keys().map(|s| s.as_str()).collect()
    }
}

impl Default for Validator {
    fn default() -> Self {
        Self::new()
    }
}

/// Quick validation functions.
pub struct Validate;

impl Validate {
    /// Check if valid email.
    pub fn email(s: &str) -> bool {
        Regex::new(Patterns::email())
            .map(|r| r.is_match(s))
            .unwrap_or(false)
    }

    /// Check if valid URL.
    pub fn url(s: &str) -> bool {
        Regex::new(Patterns::url())
            .map(|r| r.is_match(s))
            .unwrap_or(false)
    }

    /// Check if valid IPv4.
    pub fn ipv4(s: &str) -> bool {
        Regex::new(Patterns::ipv4())
            .map(|r| r.is_match(s))
            .unwrap_or(false)
    }

    /// Check if valid UUID.
    pub fn uuid(s: &str) -> bool {
        Regex::new(Patterns::uuid())
            .map(|r| r.is_match(s))
            .unwrap_or(false)
    }

    /// Check if valid ISO date.
    pub fn date(s: &str) -> bool {
        Regex::new(Patterns::date_iso())
            .map(|r| r.is_match(s))
            .unwrap_or(false)
    }

    /// Check if valid hex color.
    pub fn hex_color(s: &str) -> bool {
        Regex::new(Patterns::hex_color())
            .map(|r| r.is_match(s))
            .unwrap_or(false)
    }

    /// Check if valid slug.
    pub fn slug(s: &str) -> bool {
        Regex::new(Patterns::slug())
            .map(|r| r.is_match(s))
            .unwrap_or(false)
    }

    /// Check if valid semver.
    pub fn semver(s: &str) -> bool {
        Regex::new(Patterns::semver())
            .map(|r| r.is_match(s))
            .unwrap_or(false)
    }

    /// Check if valid integer.
    pub fn integer(s: &str) -> bool {
        Regex::new(Patterns::integer())
            .map(|r| r.is_match(s))
            .unwrap_or(false)
    }

    /// Check if valid decimal.
    pub fn decimal(s: &str) -> bool {
        Regex::new(Patterns::decimal())
            .map(|r| r.is_match(s))
            .unwrap_or(false)
    }
}

/// Regex match result.
#[derive(Debug, Clone)]
pub struct Match {
    /// Matched text.
    pub text: String,
    /// Start position.
    pub start: usize,
    /// End position.
    pub end: usize,
    /// Captured groups.
    pub groups: Vec<Option<String>>,
    /// Named captures.
    pub named: HashMap<String, String>,
}

impl Match {
    /// Get captured group by index.
    pub fn group(&self, index: usize) -> Option<&str> {
        self.groups.get(index).and_then(|s| s.as_deref())
    }

    /// Get named capture.
    pub fn named(&self, name: &str) -> Option<&str> {
        self.named.get(name).map(|s| s.as_str())
    }
}

/// Regex utilities.
pub struct Re;

impl Re {
    /// Compile regex.
    pub fn compile(pattern: &str) -> Result<Regex> {
        Ok(Regex::new(pattern)?)
    }

    /// Compile case-insensitive regex.
    pub fn compile_case_insensitive(pattern: &str) -> Result<Regex> {
        Ok(RegexBuilder::new(pattern).case_insensitive(true).build()?)
    }

    /// Test if pattern matches.
    pub fn test(pattern: &str, text: &str) -> bool {
        Regex::new(pattern)
            .map(|r| r.is_match(text))
            .unwrap_or(false)
    }

    /// Find first match.
    pub fn find(pattern: &str, text: &str) -> Result<Option<Match>> {
        let regex = Regex::new(pattern)?;

        Ok(regex.captures(text).map(|caps| {
            let m = caps.get(0).unwrap();
            Match {
                text: m.as_str().to_string(),
                start: m.start(),
                end: m.end(),
                groups: caps
                    .iter()
                    .map(|c| c.map(|m| m.as_str().to_string()))
                    .collect(),
                named: HashMap::new(),
            }
        }))
    }

    /// Find all matches.
    pub fn find_all(pattern: &str, text: &str) -> Result<Vec<Match>> {
        let regex = Regex::new(pattern)?;

        Ok(regex
            .captures_iter(text)
            .map(|caps| {
                let m = caps.get(0).unwrap();
                Match {
                    text: m.as_str().to_string(),
                    start: m.start(),
                    end: m.end(),
                    groups: caps
                        .iter()
                        .map(|c| c.map(|m| m.as_str().to_string()))
                        .collect(),
                    named: HashMap::new(),
                }
            })
            .collect())
    }

    /// Replace first match.
    pub fn replace(pattern: &str, text: &str, replacement: &str) -> Result<String> {
        let regex = Regex::new(pattern)?;
        Ok(regex.replace(text, replacement).to_string())
    }

    /// Replace all matches.
    pub fn replace_all(pattern: &str, text: &str, replacement: &str) -> Result<String> {
        let regex = Regex::new(pattern)?;
        Ok(regex.replace_all(text, replacement).to_string())
    }

    /// Replace with function.
    pub fn replace_with<F>(pattern: &str, text: &str, replacer: F) -> Result<String>
    where
        F: Fn(&Captures) -> String,
    {
        let regex = Regex::new(pattern)?;
        Ok(regex.replace_all(text, replacer).to_string())
    }

    /// Split by pattern.
    pub fn split(pattern: &str, text: &str) -> Result<Vec<String>> {
        let regex = Regex::new(pattern)?;
        Ok(regex.split(text).map(|s| s.to_string()).collect())
    }

    /// Extract all matches as strings.
    pub fn extract_all(pattern: &str, text: &str) -> Result<Vec<String>> {
        let regex = Regex::new(pattern)?;
        Ok(regex
            .find_iter(text)
            .map(|m| m.as_str().to_string())
            .collect())
    }

    /// Count matches.
    pub fn count(pattern: &str, text: &str) -> Result<usize> {
        let regex = Regex::new(pattern)?;
        Ok(regex.find_iter(text).count())
    }

    /// Escape regex special characters.
    pub fn escape(s: &str) -> String {
        regex::escape(s)
    }
}

/// Regex builder with fluent API.
pub struct ReBuilder {
    pattern: String,
    case_insensitive: bool,
    multi_line: bool,
    dot_matches_new_line: bool,
}

impl ReBuilder {
    /// Create new builder.
    pub fn new() -> Self {
        Self {
            pattern: String::new(),
            case_insensitive: false,
            multi_line: false,
            dot_matches_new_line: false,
        }
    }

    /// Set pattern.
    pub fn pattern(mut self, pattern: &str) -> Self {
        self.pattern = pattern.to_string();
        self
    }

    /// Add to pattern.
    pub fn add(mut self, pattern: &str) -> Self {
        self.pattern.push_str(pattern);
        self
    }

    /// Set case insensitive.
    pub fn case_insensitive(mut self, value: bool) -> Self {
        self.case_insensitive = value;
        self
    }

    /// Set multi-line mode.
    pub fn multi_line(mut self, value: bool) -> Self {
        self.multi_line = value;
        self
    }

    /// Set dot matches newline.
    pub fn dot_matches_new_line(mut self, value: bool) -> Self {
        self.dot_matches_new_line = value;
        self
    }

    /// Build the regex.
    pub fn build(self) -> Result<Regex> {
        Ok(RegexBuilder::new(&self.pattern)
            .case_insensitive(self.case_insensitive)
            .multi_line(self.multi_line)
            .dot_matches_new_line(self.dot_matches_new_line)
            .build()?)
    }
}

impl Default for ReBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_email() {
        assert!(Validate::email("test@example.com"));
        assert!(Validate::email("user.name+tag@domain.co.uk"));
        assert!(!Validate::email("invalid"));
        assert!(!Validate::email("@domain.com"));
    }

    #[test]
    fn test_validate_url() {
        assert!(Validate::url("https://example.com"));
        assert!(Validate::url("http://example.com/path?query=1"));
        assert!(!Validate::url("not a url"));
    }

    #[test]
    fn test_validate_uuid() {
        assert!(Validate::uuid("123e4567-e89b-12d3-a456-426614174000"));
        assert!(!Validate::uuid("not-a-uuid"));
    }

    #[test]
    fn test_validate_semver() {
        assert!(Validate::semver("1.0.0"));
        assert!(Validate::semver("v1.2.3"));
        assert!(Validate::semver("1.0.0-alpha.1"));
        assert!(Validate::semver("1.0.0+build.123"));
        assert!(!Validate::semver("1.0"));
    }

    #[test]
    fn test_re_find() {
        let m = Re::find(r"(\d+)", "age: 42").unwrap().unwrap();
        assert_eq!(m.text, "42");
        assert_eq!(m.group(1), Some("42"));
    }

    #[test]
    fn test_re_find_all() {
        let matches = Re::find_all(r"\d+", "1, 2, 3").unwrap();
        assert_eq!(matches.len(), 3);
        assert_eq!(matches[0].text, "1");
        assert_eq!(matches[1].text, "2");
        assert_eq!(matches[2].text, "3");
    }

    #[test]
    fn test_re_replace() {
        let result = Re::replace(r"\d+", "age: 42", "XX").unwrap();
        assert_eq!(result, "age: XX");

        let result = Re::replace_all(r"\d+", "1, 2, 3", "X").unwrap();
        assert_eq!(result, "X, X, X");
    }

    #[test]
    fn test_re_split() {
        let parts = Re::split(r"\s*,\s*", "a, b, c").unwrap();
        assert_eq!(parts, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_re_count() {
        assert_eq!(Re::count(r"\d", "a1b2c3").unwrap(), 3);
    }

    #[test]
    fn test_re_escape() {
        let escaped = Re::escape("hello (world)");
        assert_eq!(escaped, r"hello \(world\)");
    }

    #[test]
    fn test_re_builder() {
        let re = ReBuilder::new()
            .pattern(r"hello")
            .case_insensitive(true)
            .build()
            .unwrap();

        assert!(re.is_match("HELLO"));
        assert!(re.is_match("Hello"));
    }

    #[test]
    fn test_validator() {
        let validator = Validator::new()
            .add_pattern("email", Patterns::email())
            .unwrap()
            .add_pattern("url", Patterns::url())
            .unwrap();

        assert!(validator.validate("email", "test@example.com"));
        assert!(!validator.validate("email", "invalid"));
        assert!(validator.validate("url", "https://example.com"));
    }
}
