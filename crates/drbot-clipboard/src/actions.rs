//! Smart clipboard actions.

use crate::content::{ClipboardContent, ContentType};
use serde::{Deserialize, Serialize};

/// Clipboard action types.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClipboardAction {
    /// Copy to clipboard.
    Copy(String),
    /// Paste from clipboard.
    Paste,
    /// Transform content.
    Transform(TransformAction),
    /// Open content (URL, file, etc.).
    Open,
    /// Share content.
    Share,
    /// Save content to file.
    SaveToFile(String),
}

/// Content transformation actions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TransformAction {
    /// Convert to uppercase.
    Uppercase,
    /// Convert to lowercase.
    Lowercase,
    /// Trim whitespace.
    Trim,
    /// Format as JSON.
    FormatJson,
    /// Minify JSON.
    MinifyJson,
    /// URL encode.
    UrlEncode,
    /// URL decode.
    UrlDecode,
    /// Base64 encode.
    Base64Encode,
    /// Base64 decode.
    Base64Decode,
    /// Extract URLs.
    ExtractUrls,
    /// Extract emails.
    ExtractEmails,
    /// Strip HTML.
    StripHtml,
    /// Custom regex replace.
    RegexReplace {
        pattern: String,
        replacement: String,
    },
}

impl TransformAction {
    /// Apply transformation to text.
    pub fn apply(&self, text: &str) -> String {
        match self {
            TransformAction::Uppercase => text.to_uppercase(),
            TransformAction::Lowercase => text.to_lowercase(),
            TransformAction::Trim => text.trim().to_string(),
            TransformAction::FormatJson => {
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(text) {
                    serde_json::to_string_pretty(&value).unwrap_or_else(|_| text.to_string())
                } else {
                    text.to_string()
                }
            }
            TransformAction::MinifyJson => {
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(text) {
                    serde_json::to_string(&value).unwrap_or_else(|_| text.to_string())
                } else {
                    text.to_string()
                }
            }
            TransformAction::UrlEncode => {
                // Simple URL encoding
                text.chars()
                    .map(|c| {
                        if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' || c == '~'
                        {
                            c.to_string()
                        } else {
                            format!("%{:02X}", c as u32)
                        }
                    })
                    .collect()
            }
            TransformAction::UrlDecode => {
                // Simple URL decoding
                let mut result = String::new();
                let mut chars = text.chars().peekable();
                while let Some(c) = chars.next() {
                    if c == '%' {
                        let hex: String = chars.by_ref().take(2).collect();
                        if let Ok(n) = u8::from_str_radix(&hex, 16) {
                            result.push(n as char);
                        } else {
                            result.push('%');
                            result.push_str(&hex);
                        }
                    } else if c == '+' {
                        result.push(' ');
                    } else {
                        result.push(c);
                    }
                }
                result
            }
            TransformAction::Base64Encode => {
                // Simple base64 encoding
                base64_encode(text.as_bytes())
            }
            TransformAction::Base64Decode => {
                base64_decode(text).unwrap_or_else(|_| text.to_string())
            }
            TransformAction::ExtractUrls => extract_urls(text).join("\n"),
            TransformAction::ExtractEmails => extract_emails(text).join("\n"),
            TransformAction::StripHtml => strip_html(text),
            TransformAction::RegexReplace {
                pattern,
                replacement,
            } => {
                // Note: In production, use the regex crate
                text.replace(pattern, replacement)
            }
        }
    }
}

/// Smart action suggestion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmartAction {
    /// Action label.
    pub label: String,
    /// Action description.
    pub description: String,
    /// The action to perform.
    pub action: ClipboardAction,
    /// Priority (higher = more relevant).
    pub priority: i32,
}

impl SmartAction {
    /// Create a new smart action.
    pub fn new(label: impl Into<String>, action: ClipboardAction) -> Self {
        Self {
            label: label.into(),
            description: String::new(),
            action,
            priority: 0,
        }
    }

    /// Set description.
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    /// Set priority.
    pub fn with_priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }
}

/// Get smart actions for clipboard content.
pub fn suggest_actions(content: &ClipboardContent) -> Vec<SmartAction> {
    let mut actions = Vec::new();

    match content.content_type {
        ContentType::Url => {
            actions.push(
                SmartAction::new("Open URL", ClipboardAction::Open)
                    .with_description("Open this URL in your browser")
                    .with_priority(100),
            );
            actions.push(
                SmartAction::new("Share", ClipboardAction::Share)
                    .with_description("Share this URL")
                    .with_priority(50),
            );
        }
        ContentType::Email => {
            actions.push(
                SmartAction::new("Compose Email", ClipboardAction::Open)
                    .with_description("Open email composer")
                    .with_priority(100),
            );
        }
        ContentType::Json => {
            actions.push(
                SmartAction::new(
                    "Format JSON",
                    ClipboardAction::Transform(TransformAction::FormatJson),
                )
                .with_description("Pretty print the JSON")
                .with_priority(100),
            );
            actions.push(
                SmartAction::new(
                    "Minify JSON",
                    ClipboardAction::Transform(TransformAction::MinifyJson),
                )
                .with_description("Compact the JSON")
                .with_priority(50),
            );
        }
        ContentType::Code => {
            actions.push(
                SmartAction::new("Format", ClipboardAction::Transform(TransformAction::Trim))
                    .with_description("Clean up formatting")
                    .with_priority(50),
            );
        }
        ContentType::FilePath => {
            actions.push(
                SmartAction::new("Open File", ClipboardAction::Open)
                    .with_description("Open this file")
                    .with_priority(100),
            );
        }
        _ => {}
    }

    // Common actions
    if content.text.is_some() {
        actions.push(
            SmartAction::new(
                "UPPERCASE",
                ClipboardAction::Transform(TransformAction::Uppercase),
            )
            .with_priority(10),
        );
        actions.push(
            SmartAction::new(
                "lowercase",
                ClipboardAction::Transform(TransformAction::Lowercase),
            )
            .with_priority(10),
        );
    }

    // Sort by priority
    actions.sort_by(|a, b| b.priority.cmp(&a.priority));

    actions
}

/// Simple base64 encoding.
fn base64_encode(data: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut result = String::new();
    let mut i = 0;

    while i < data.len() {
        let b0 = data[i] as usize;
        let b1 = data.get(i + 1).copied().unwrap_or(0) as usize;
        let b2 = data.get(i + 2).copied().unwrap_or(0) as usize;

        result.push(ALPHABET[b0 >> 2] as char);
        result.push(ALPHABET[((b0 & 0x03) << 4) | (b1 >> 4)] as char);

        if i + 1 < data.len() {
            result.push(ALPHABET[((b1 & 0x0f) << 2) | (b2 >> 6)] as char);
        } else {
            result.push('=');
        }

        if i + 2 < data.len() {
            result.push(ALPHABET[b2 & 0x3f] as char);
        } else {
            result.push('=');
        }

        i += 3;
    }

    result
}

/// Simple base64 decoding.
fn base64_decode(input: &str) -> Result<String, ()> {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut result = Vec::new();
    let input: Vec<u8> = input
        .bytes()
        .filter(|&c| c != b'=' && c != b'\n' && c != b'\r')
        .collect();

    for chunk in input.chunks(4) {
        if chunk.len() < 2 {
            break;
        }

        let b0 = ALPHABET.iter().position(|&c| c == chunk[0]).ok_or(())?;
        let b1 = ALPHABET.iter().position(|&c| c == chunk[1]).ok_or(())?;

        result.push(((b0 << 2) | (b1 >> 4)) as u8);

        if chunk.len() > 2 {
            let b2 = ALPHABET.iter().position(|&c| c == chunk[2]).ok_or(())?;
            result.push((((b1 & 0x0f) << 4) | (b2 >> 2)) as u8);

            if chunk.len() > 3 {
                let b3 = ALPHABET.iter().position(|&c| c == chunk[3]).ok_or(())?;
                result.push((((b2 & 0x03) << 6) | b3) as u8);
            }
        }
    }

    String::from_utf8(result).map_err(|_| ())
}

/// Extract URLs from text.
fn extract_urls(text: &str) -> Vec<String> {
    let mut urls = Vec::new();

    for word in text.split_whitespace() {
        if word.starts_with("http://") || word.starts_with("https://") {
            // Clean up trailing punctuation
            let url = word.trim_end_matches(|c| c == '.' || c == ',' || c == ')' || c == ']');
            urls.push(url.to_string());
        }
    }

    urls
}

/// Extract email addresses from text.
fn extract_emails(text: &str) -> Vec<String> {
    let mut emails = Vec::new();

    for word in text.split_whitespace() {
        let word = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '@' && c != '.');
        if word.contains('@') && word.contains('.') {
            let at_pos = word.find('@').unwrap();
            let dot_pos = word.rfind('.').unwrap();
            if at_pos > 0 && dot_pos > at_pos + 1 && dot_pos < word.len() - 1 {
                emails.push(word.to_string());
            }
        }
    }

    emails
}

/// Strip HTML tags from text.
fn strip_html(text: &str) -> String {
    let mut result = String::new();
    let mut in_tag = false;

    for c in text.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => result.push(c),
            _ => {}
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transform_uppercase() {
        let action = TransformAction::Uppercase;
        assert_eq!(action.apply("hello"), "HELLO");
    }

    #[test]
    fn test_transform_format_json() {
        let action = TransformAction::FormatJson;
        let result = action.apply(r#"{"a":1}"#);
        assert!(result.contains('\n')); // Pretty printed
    }

    #[test]
    fn test_extract_urls() {
        let urls = extract_urls("Check out https://example.com and http://test.com.");
        assert_eq!(urls.len(), 2);
    }

    #[test]
    fn test_extract_emails() {
        let emails = extract_emails("Contact us at test@example.com or info@test.org");
        assert_eq!(emails.len(), 2);
    }

    #[test]
    fn test_strip_html() {
        let result = strip_html("<p>Hello <b>World</b></p>");
        assert_eq!(result, "Hello World");
    }

    #[test]
    fn test_base64() {
        let encoded = base64_encode(b"Hello");
        assert_eq!(encoded, "SGVsbG8=");

        let decoded = base64_decode(&encoded).unwrap();
        assert_eq!(decoded, "Hello");
    }
}
