//! Clipboard content types and detection.

use serde::{Deserialize, Serialize};

/// Type of clipboard content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContentType {
    /// Plain text.
    PlainText,
    /// Rich text (HTML, RTF).
    RichText,
    /// URL.
    Url,
    /// Email address.
    Email,
    /// Phone number.
    PhoneNumber,
    /// Code snippet.
    Code,
    /// File path.
    FilePath,
    /// JSON data.
    Json,
    /// Image data.
    Image,
    /// File reference.
    File,
    /// Unknown type.
    Unknown,
}

impl ContentType {
    /// Detect content type from text.
    pub fn detect(text: &str) -> Self {
        let trimmed = text.trim();

        // Check for URL
        if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
            return ContentType::Url;
        }

        // Check for email
        if is_email(trimmed) {
            return ContentType::Email;
        }

        // Check for phone number
        if is_phone_number(trimmed) {
            return ContentType::PhoneNumber;
        }

        // Check for file path
        if is_file_path(trimmed) {
            return ContentType::FilePath;
        }

        // Check for JSON
        if (trimmed.starts_with('{') && trimmed.ends_with('}'))
            || (trimmed.starts_with('[') && trimmed.ends_with(']'))
        {
            if serde_json::from_str::<serde_json::Value>(trimmed).is_ok() {
                return ContentType::Json;
            }
        }

        // Check for code
        if looks_like_code(trimmed) {
            return ContentType::Code;
        }

        ContentType::PlainText
    }
}

/// Clipboard content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardContent {
    /// Detected content type.
    pub content_type: ContentType,
    /// Text content (if available).
    pub text: Option<String>,
    /// HTML content (if available).
    pub html: Option<String>,
    /// Image data (if available).
    pub image: Option<Vec<u8>>,
    /// File paths (if available).
    pub files: Vec<String>,
    /// When this was captured.
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl ClipboardContent {
    /// Create from plain text.
    pub fn from_text(text: impl Into<String>) -> Self {
        let text = text.into();
        let content_type = ContentType::detect(&text);

        Self {
            content_type,
            text: Some(text),
            html: None,
            image: None,
            files: Vec::new(),
            timestamp: chrono::Utc::now(),
        }
    }

    /// Create from HTML.
    pub fn from_html(html: impl Into<String>, text: Option<String>) -> Self {
        Self {
            content_type: ContentType::RichText,
            text,
            html: Some(html.into()),
            image: None,
            files: Vec::new(),
            timestamp: chrono::Utc::now(),
        }
    }

    /// Create from image.
    pub fn from_image(data: Vec<u8>) -> Self {
        Self {
            content_type: ContentType::Image,
            text: None,
            html: None,
            image: Some(data),
            files: Vec::new(),
            timestamp: chrono::Utc::now(),
        }
    }

    /// Create from file paths.
    pub fn from_files(paths: Vec<String>) -> Self {
        Self {
            content_type: ContentType::File,
            text: None,
            html: None,
            image: None,
            files: paths,
            timestamp: chrono::Utc::now(),
        }
    }

    /// Get the primary text content.
    pub fn as_text(&self) -> Option<&str> {
        self.text.as_deref()
    }

    /// Get content length.
    pub fn len(&self) -> usize {
        self.text.as_ref().map(|t| t.len()).unwrap_or(0)
            + self.html.as_ref().map(|h| h.len()).unwrap_or(0)
            + self.image.as_ref().map(|i| i.len()).unwrap_or(0)
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.text.is_none() && self.html.is_none() && self.image.is_none() && self.files.is_empty()
    }
}

/// Check if text looks like an email address.
fn is_email(text: &str) -> bool {
    let at_pos = text.find('@');
    let dot_pos = text.rfind('.');

    if let (Some(at), Some(dot)) = (at_pos, dot_pos) {
        // Basic email pattern: something@something.something
        at > 0 && dot > at + 1 && dot < text.len() - 1 && !text.contains(' ')
    } else {
        false
    }
}

/// Check if text looks like a phone number.
fn is_phone_number(text: &str) -> bool {
    let digits: String = text.chars().filter(|c| c.is_ascii_digit()).collect();
    let has_only_phone_chars = text
        .chars()
        .all(|c| c.is_ascii_digit() || c == '+' || c == '-' || c == '(' || c == ')' || c == ' ');

    has_only_phone_chars && digits.len() >= 7 && digits.len() <= 15
}

/// Check if text looks like a file path.
fn is_file_path(text: &str) -> bool {
    // Unix-style paths
    if text.starts_with('/') || text.starts_with("~/") {
        return true;
    }

    // Windows-style paths
    if text.len() >= 3 {
        let bytes = text.as_bytes();
        if bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && (bytes[2] == b'\\' || bytes[2] == b'/')
        {
            return true;
        }
    }

    false
}

/// Check if text looks like code.
fn looks_like_code(text: &str) -> bool {
    // Check for common code patterns
    let code_indicators = [
        "function ",
        "fn ",
        "def ",
        "class ",
        "const ",
        "let ",
        "var ",
        "import ",
        "from ",
        "require(",
        "pub ",
        "async ",
        "await ",
        "if (",
        "if(",
        "for (",
        "for(",
        "while (",
        "while(",
        "return ",
        "=>",
        "->",
        "::",
        "||",
        "&&",
    ];

    let lines: Vec<&str> = text.lines().collect();

    // Multiple lines with consistent indentation
    if lines.len() > 1 {
        let indented_lines = lines
            .iter()
            .filter(|l| l.starts_with("  ") || l.starts_with('\t'))
            .count();
        if indented_lines as f32 / lines.len() as f32 > 0.3 {
            return true;
        }
    }

    // Contains code-like syntax
    for indicator in &code_indicators {
        if text.contains(indicator) {
            return true;
        }
    }

    // Has semicolons at end of lines
    let semicolon_lines = lines.iter().filter(|l| l.trim().ends_with(';')).count();
    if lines.len() > 2 && semicolon_lines as f32 / lines.len() as f32 > 0.3 {
        return true;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_content_type_detection() {
        assert_eq!(ContentType::detect("https://example.com"), ContentType::Url);
        assert_eq!(ContentType::detect("test@example.com"), ContentType::Email);
        assert_eq!(
            ContentType::detect("+1 (555) 123-4567"),
            ContentType::PhoneNumber
        );
        assert_eq!(ContentType::detect("/usr/local/bin"), ContentType::FilePath);
        assert_eq!(
            ContentType::detect(r#"{"key": "value"}"#),
            ContentType::Json
        );
        assert_eq!(ContentType::detect("Hello, World!"), ContentType::PlainText);
    }

    #[test]
    fn test_code_detection() {
        assert_eq!(
            ContentType::detect("function test() { return 42; }"),
            ContentType::Code
        );
        assert_eq!(
            ContentType::detect("const x = 5;\nconst y = 10;"),
            ContentType::Code
        );
    }

    #[test]
    fn test_clipboard_content_from_text() {
        let content = ClipboardContent::from_text("Hello, World!");
        assert_eq!(content.content_type, ContentType::PlainText);
        assert_eq!(content.as_text(), Some("Hello, World!"));
    }
}
