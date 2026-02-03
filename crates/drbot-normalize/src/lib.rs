//! Data normalization for drbot.
//!
//! This crate provides:
//! - Text normalization
//! - Unicode normalization
//! - Number formatting
//! - Date/time normalization

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Normalization error types.
#[derive(Error, Debug)]
pub enum NormalizeError {
    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Normalization failed: {0}")]
    Failed(String),
}

/// Result type for normalization operations.
pub type Result<T> = std::result::Result<T, NormalizeError>;

/// Normalizer trait.
pub trait Normalizer<T>: Send + Sync {
    /// Normalize value.
    fn normalize(&self, input: T) -> Result<T>;
}

/// String case normalization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Case {
    Lower,
    Upper,
    Title,
    Sentence,
    Camel,
    Snake,
    Kebab,
    Pascal,
}

/// Case normalizer.
pub struct CaseNormalizer(pub Case);

impl CaseNormalizer {
    pub fn lower() -> Self {
        Self(Case::Lower)
    }
    pub fn upper() -> Self {
        Self(Case::Upper)
    }
    pub fn title() -> Self {
        Self(Case::Title)
    }
    pub fn snake() -> Self {
        Self(Case::Snake)
    }
    pub fn kebab() -> Self {
        Self(Case::Kebab)
    }
    pub fn camel() -> Self {
        Self(Case::Camel)
    }
    pub fn pascal() -> Self {
        Self(Case::Pascal)
    }

    fn to_words(s: &str) -> Vec<String> {
        let mut words = Vec::new();
        let mut current = String::new();

        for c in s.chars() {
            if c.is_alphanumeric() {
                if c.is_uppercase()
                    && !current.is_empty()
                    && current
                        .chars()
                        .last()
                        .map(|l| l.is_lowercase())
                        .unwrap_or(false)
                {
                    words.push(current.to_lowercase());
                    current = String::new();
                }
                current.push(c);
            } else if !current.is_empty() {
                words.push(current.to_lowercase());
                current = String::new();
            }
        }

        if !current.is_empty() {
            words.push(current.to_lowercase());
        }

        words
    }

    fn capitalize(s: &str) -> String {
        let mut chars = s.chars();
        match chars.next() {
            None => String::new(),
            Some(c) => c.to_uppercase().chain(chars).collect(),
        }
    }
}

impl Normalizer<String> for CaseNormalizer {
    fn normalize(&self, input: String) -> Result<String> {
        let result = match self.0 {
            Case::Lower => input.to_lowercase(),
            Case::Upper => input.to_uppercase(),
            Case::Title => input
                .split_whitespace()
                .map(Self::capitalize)
                .collect::<Vec<_>>()
                .join(" "),
            Case::Sentence => {
                let lower = input.to_lowercase();
                Self::capitalize(&lower)
            }
            Case::Snake => Self::to_words(&input).join("_"),
            Case::Kebab => Self::to_words(&input).join("-"),
            Case::Camel => {
                let words = Self::to_words(&input);
                let mut result = String::new();
                for (i, word) in words.iter().enumerate() {
                    if i == 0 {
                        result.push_str(word);
                    } else {
                        result.push_str(&Self::capitalize(word));
                    }
                }
                result
            }
            Case::Pascal => Self::to_words(&input)
                .iter()
                .map(|w| Self::capitalize(w))
                .collect(),
        };
        Ok(result)
    }
}

/// Whitespace normalizer.
pub struct WhitespaceNormalizer {
    collapse: bool,
    trim: bool,
}

impl WhitespaceNormalizer {
    pub fn new() -> Self {
        Self {
            collapse: true,
            trim: true,
        }
    }

    pub fn collapse_only() -> Self {
        Self {
            collapse: true,
            trim: false,
        }
    }

    pub fn trim_only() -> Self {
        Self {
            collapse: false,
            trim: true,
        }
    }
}

impl Default for WhitespaceNormalizer {
    fn default() -> Self {
        Self::new()
    }
}

impl Normalizer<String> for WhitespaceNormalizer {
    fn normalize(&self, input: String) -> Result<String> {
        let mut result = input;

        if self.trim {
            result = result.trim().to_string();
        }

        if self.collapse {
            let mut collapsed = String::with_capacity(result.len());
            let mut prev_space = false;

            for c in result.chars() {
                if c.is_whitespace() {
                    if !prev_space {
                        collapsed.push(' ');
                        prev_space = true;
                    }
                } else {
                    collapsed.push(c);
                    prev_space = false;
                }
            }
            result = collapsed;
        }

        Ok(result)
    }
}

/// Number normalizer.
pub struct NumberNormalizer {
    decimal_places: Option<usize>,
    thousands_separator: Option<char>,
}

impl NumberNormalizer {
    pub fn new() -> Self {
        Self {
            decimal_places: None,
            thousands_separator: None,
        }
    }

    pub fn decimal_places(mut self, places: usize) -> Self {
        self.decimal_places = Some(places);
        self
    }

    pub fn thousands_separator(mut self, sep: char) -> Self {
        self.thousands_separator = Some(sep);
        self
    }

    /// Format number as string.
    pub fn format(&self, value: f64) -> String {
        let formatted = match self.decimal_places {
            Some(places) => format!("{:.1$}", value, places),
            None => format!("{}", value),
        };

        if let Some(sep) = self.thousands_separator {
            Self::add_thousands_separator(&formatted, sep)
        } else {
            formatted
        }
    }

    fn add_thousands_separator(s: &str, sep: char) -> String {
        let parts: Vec<&str> = s.split('.').collect();
        let integer = parts[0];
        let decimal = parts.get(1);

        let mut result = String::new();
        let chars: Vec<char> = integer.chars().collect();
        let start = if chars[0] == '-' { 1 } else { 0 };

        for (i, c) in chars.iter().enumerate() {
            if i > start && (chars.len() - i) % 3 == 0 {
                result.push(sep);
            }
            result.push(*c);
        }

        if let Some(dec) = decimal {
            result.push('.');
            result.push_str(dec);
        }

        result
    }
}

impl Default for NumberNormalizer {
    fn default() -> Self {
        Self::new()
    }
}

impl Normalizer<f64> for NumberNormalizer {
    fn normalize(&self, input: f64) -> Result<f64> {
        match self.decimal_places {
            Some(places) => {
                let factor = 10f64.powi(places as i32);
                Ok((input * factor).round() / factor)
            }
            None => Ok(input),
        }
    }
}

/// Phone number normalizer.
pub struct PhoneNormalizer {
    default_country: String,
}

impl PhoneNormalizer {
    pub fn new(default_country: impl Into<String>) -> Self {
        Self {
            default_country: default_country.into(),
        }
    }

    /// Extract digits only.
    pub fn digits_only(phone: &str) -> String {
        phone.chars().filter(|c| c.is_ascii_digit()).collect()
    }
}

impl Normalizer<String> for PhoneNormalizer {
    fn normalize(&self, input: String) -> Result<String> {
        let digits = Self::digits_only(&input);

        if digits.len() < 7 {
            return Err(NormalizeError::InvalidInput(
                "Phone number too short".to_string(),
            ));
        }

        // Basic normalization - just return digits with + prefix
        if digits.len() == 10 {
            // US format assumption
            Ok(format!("+1{}", digits))
        } else if digits.len() == 11 && digits.starts_with('1') {
            Ok(format!("+{}", digits))
        } else {
            Ok(format!("+{}", digits))
        }
    }
}

/// Email normalizer.
pub struct EmailNormalizer {
    lowercase: bool,
    remove_plus_addressing: bool,
}

impl EmailNormalizer {
    pub fn new() -> Self {
        Self {
            lowercase: true,
            remove_plus_addressing: false,
        }
    }

    pub fn remove_plus_addressing(mut self) -> Self {
        self.remove_plus_addressing = true;
        self
    }
}

impl Default for EmailNormalizer {
    fn default() -> Self {
        Self::new()
    }
}

impl Normalizer<String> for EmailNormalizer {
    fn normalize(&self, input: String) -> Result<String> {
        let input = input.trim();

        let parts: Vec<&str> = input.split('@').collect();
        if parts.len() != 2 {
            return Err(NormalizeError::InvalidInput(
                "Invalid email format".to_string(),
            ));
        }

        let mut local = parts[0].to_string();
        let domain = parts[1];

        if self.remove_plus_addressing {
            if let Some(idx) = local.find('+') {
                local = local[..idx].to_string();
            }
        }

        let result = if self.lowercase {
            format!("{}@{}", local.to_lowercase(), domain.to_lowercase())
        } else {
            format!("{}@{}", local, domain)
        };

        Ok(result)
    }
}

/// URL normalizer.
pub struct UrlNormalizer {
    lowercase_host: bool,
    remove_trailing_slash: bool,
    remove_default_port: bool,
}

impl UrlNormalizer {
    pub fn new() -> Self {
        Self {
            lowercase_host: true,
            remove_trailing_slash: true,
            remove_default_port: true,
        }
    }
}

impl Default for UrlNormalizer {
    fn default() -> Self {
        Self::new()
    }
}

impl Normalizer<String> for UrlNormalizer {
    fn normalize(&self, input: String) -> Result<String> {
        let mut url = input.trim().to_string();

        // Lowercase scheme and host
        if self.lowercase_host {
            if let Some(idx) = url.find("://") {
                let (scheme, rest) = url.split_at(idx + 3);
                if let Some(path_idx) = rest.find('/') {
                    let (host, path) = rest.split_at(path_idx);
                    url = format!("{}{}{}", scheme.to_lowercase(), host.to_lowercase(), path);
                } else {
                    url = format!("{}{}", scheme.to_lowercase(), rest.to_lowercase());
                }
            }
        }

        // Remove default ports
        if self.remove_default_port {
            url = url.replace(":80/", "/");
            url = url.replace(":443/", "/");
            if url.ends_with(":80") {
                url = url[..url.len() - 3].to_string();
            }
            if url.ends_with(":443") {
                url = url[..url.len() - 4].to_string();
            }
        }

        // Remove trailing slash
        if self.remove_trailing_slash && url.ends_with('/') && !url.ends_with("://") {
            url.pop();
        }

        Ok(url)
    }
}

/// Normalizer chain.
pub struct NormalizerChain<T> {
    normalizers: Vec<Box<dyn Normalizer<T>>>,
}

impl<T: 'static> NormalizerChain<T> {
    pub fn new() -> Self {
        Self {
            normalizers: Vec::new(),
        }
    }

    pub fn add<N: Normalizer<T> + 'static>(mut self, normalizer: N) -> Self {
        self.normalizers.push(Box::new(normalizer));
        self
    }
}

impl<T: 'static> Default for NormalizerChain<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Normalizer<T> for NormalizerChain<T> {
    fn normalize(&self, mut input: T) -> Result<T> {
        for normalizer in &self.normalizers {
            input = normalizer.normalize(input)?;
        }
        Ok(input)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_case_lower() {
        let normalizer = CaseNormalizer::lower();
        assert_eq!(
            normalizer.normalize("HELLO World".to_string()).unwrap(),
            "hello world"
        );
    }

    #[test]
    fn test_case_upper() {
        let normalizer = CaseNormalizer::upper();
        assert_eq!(
            normalizer.normalize("hello world".to_string()).unwrap(),
            "HELLO WORLD"
        );
    }

    #[test]
    fn test_case_title() {
        let normalizer = CaseNormalizer::title();
        assert_eq!(
            normalizer.normalize("hello world".to_string()).unwrap(),
            "Hello World"
        );
    }

    #[test]
    fn test_case_snake() {
        let normalizer = CaseNormalizer::snake();
        assert_eq!(
            normalizer.normalize("helloWorld".to_string()).unwrap(),
            "hello_world"
        );
        assert_eq!(
            normalizer.normalize("Hello World".to_string()).unwrap(),
            "hello_world"
        );
    }

    #[test]
    fn test_case_kebab() {
        let normalizer = CaseNormalizer::kebab();
        assert_eq!(
            normalizer.normalize("HelloWorld".to_string()).unwrap(),
            "hello-world"
        );
    }

    #[test]
    fn test_case_camel() {
        let normalizer = CaseNormalizer::camel();
        assert_eq!(
            normalizer.normalize("hello_world".to_string()).unwrap(),
            "helloWorld"
        );
    }

    #[test]
    fn test_whitespace() {
        let normalizer = WhitespaceNormalizer::new();
        assert_eq!(
            normalizer
                .normalize("  hello    world  ".to_string())
                .unwrap(),
            "hello world"
        );
    }

    #[test]
    fn test_number_format() {
        let normalizer = NumberNormalizer::new().decimal_places(2);
        assert_eq!(normalizer.format(1234.5678), "1234.57");
    }

    #[test]
    fn test_number_thousands() {
        let normalizer = NumberNormalizer::new().thousands_separator(',');
        assert_eq!(normalizer.format(1234567.89), "1,234,567.89");
    }

    #[test]
    fn test_phone() {
        let normalizer = PhoneNormalizer::new("US");
        assert_eq!(
            normalizer.normalize("(555) 123-4567".to_string()).unwrap(),
            "+15551234567"
        );
    }

    #[test]
    fn test_email() {
        let normalizer = EmailNormalizer::new();
        assert_eq!(
            normalizer
                .normalize("User@Example.COM".to_string())
                .unwrap(),
            "user@example.com"
        );
    }

    #[test]
    fn test_email_plus_addressing() {
        let normalizer = EmailNormalizer::new().remove_plus_addressing();
        assert_eq!(
            normalizer
                .normalize("user+tag@example.com".to_string())
                .unwrap(),
            "user@example.com"
        );
    }

    #[test]
    fn test_url() {
        let normalizer = UrlNormalizer::new();
        assert_eq!(
            normalizer
                .normalize("HTTP://Example.COM:80/path/".to_string())
                .unwrap(),
            "http://example.com/path"
        );
    }

    #[test]
    fn test_chain() {
        let chain = NormalizerChain::new()
            .add(WhitespaceNormalizer::new())
            .add(CaseNormalizer::lower());

        assert_eq!(
            chain.normalize("  HELLO   WORLD  ".to_string()).unwrap(),
            "hello world"
        );
    }
}
