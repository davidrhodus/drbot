//! Text truncation for drbot.
//!
//! This crate provides:
//! - Smart text truncation
//! - Word-aware truncation
//! - Various ellipsis styles

use thiserror::Error;

/// Truncate error types.
#[derive(Error, Debug)]
pub enum TruncateError {
    #[error("Invalid length: {0}")]
    InvalidLength(usize),
}

/// Result type for truncate operations.
pub type Result<T> = std::result::Result<T, TruncateError>;

/// Truncation position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Position {
    /// Truncate at end (default).
    End,
    /// Truncate at start.
    Start,
    /// Truncate in middle.
    Middle,
}

/// Truncation options.
#[derive(Debug, Clone)]
pub struct TruncateOptions {
    /// Maximum length.
    pub max_length: usize,
    /// Ellipsis string.
    pub ellipsis: String,
    /// Truncation position.
    pub position: Position,
    /// Break on word boundaries.
    pub word_boundary: bool,
    /// Preserve whole words.
    pub preserve_words: bool,
}

impl TruncateOptions {
    /// Create new options with max length.
    pub fn new(max_length: usize) -> Self {
        Self {
            max_length,
            ellipsis: "...".to_string(),
            position: Position::End,
            word_boundary: false,
            preserve_words: false,
        }
    }

    /// Set ellipsis.
    pub fn ellipsis(mut self, ellipsis: &str) -> Self {
        self.ellipsis = ellipsis.to_string();
        self
    }

    /// Set position.
    pub fn position(mut self, position: Position) -> Self {
        self.position = position;
        self
    }

    /// Enable word boundary breaking.
    pub fn word_boundary(mut self) -> Self {
        self.word_boundary = true;
        self
    }

    /// Enable word preservation.
    pub fn preserve_words(mut self) -> Self {
        self.preserve_words = true;
        self
    }
}

impl Default for TruncateOptions {
    fn default() -> Self {
        Self::new(100)
    }
}

/// Text truncator.
pub struct Truncate;

impl Truncate {
    /// Truncate string with options.
    pub fn with_options(s: &str, options: &TruncateOptions) -> String {
        if s.len() <= options.max_length {
            return s.to_string();
        }

        let ellipsis_len = options.ellipsis.len();
        if options.max_length <= ellipsis_len {
            return options.ellipsis[..options.max_length].to_string();
        }

        let available = options.max_length - ellipsis_len;

        match options.position {
            Position::End => {
                if options.preserve_words || options.word_boundary {
                    let truncated = Self::truncate_at_word_end(s, available);
                    format!("{}{}", truncated, options.ellipsis)
                } else {
                    format!("{}{}", &s[..available], options.ellipsis)
                }
            }
            Position::Start => {
                if options.preserve_words || options.word_boundary {
                    let truncated = Self::truncate_at_word_start(s, available);
                    format!("{}{}", options.ellipsis, truncated)
                } else {
                    let start = s.len() - available;
                    format!("{}{}", options.ellipsis, &s[start..])
                }
            }
            Position::Middle => {
                let half = available / 2;
                let start_len = half + (available % 2);
                let end_len = half;

                if options.preserve_words || options.word_boundary {
                    let start = Self::truncate_at_word_end(s, start_len);
                    let end = Self::truncate_at_word_start(s, end_len);
                    format!("{}{}{}", start, options.ellipsis, end)
                } else {
                    let start = &s[..start_len];
                    let end = &s[s.len() - end_len..];
                    format!("{}{}{}", start, options.ellipsis, end)
                }
            }
        }
    }

    fn truncate_at_word_end(s: &str, max_len: usize) -> &str {
        if s.len() <= max_len {
            return s;
        }

        // Find last space before max_len
        let slice = &s[..max_len];
        if let Some(pos) = slice.rfind(|c: char| c.is_whitespace()) {
            &s[..pos]
        } else {
            slice
        }
    }

    fn truncate_at_word_start(s: &str, max_len: usize) -> &str {
        if s.len() <= max_len {
            return s;
        }

        let start = s.len() - max_len;
        let slice = &s[start..];

        // Find first space after start
        if let Some(pos) = slice.find(|c: char| c.is_whitespace()) {
            &slice[pos + 1..]
        } else {
            slice
        }
    }

    /// Simple truncate at end.
    pub fn end(s: &str, max_length: usize) -> String {
        Self::with_options(s, &TruncateOptions::new(max_length))
    }

    /// Truncate at end with custom ellipsis.
    pub fn end_with(s: &str, max_length: usize, ellipsis: &str) -> String {
        Self::with_options(s, &TruncateOptions::new(max_length).ellipsis(ellipsis))
    }

    /// Truncate at start.
    pub fn start(s: &str, max_length: usize) -> String {
        Self::with_options(
            s,
            &TruncateOptions::new(max_length).position(Position::Start),
        )
    }

    /// Truncate in middle.
    pub fn middle(s: &str, max_length: usize) -> String {
        Self::with_options(
            s,
            &TruncateOptions::new(max_length).position(Position::Middle),
        )
    }

    /// Truncate preserving words.
    pub fn words(s: &str, max_length: usize) -> String {
        Self::with_options(s, &TruncateOptions::new(max_length).preserve_words())
    }

    /// Truncate to N words.
    pub fn n_words(s: &str, n: usize) -> String {
        let words: Vec<&str> = s.split_whitespace().collect();
        if words.len() <= n {
            return s.to_string();
        }
        format!("{}...", words[..n].join(" "))
    }

    /// Truncate to N sentences.
    pub fn n_sentences(s: &str, n: usize) -> String {
        let sentence_endings = ['.', '!', '?'];
        let mut count = 0;
        let mut end_pos = 0;

        for (i, c) in s.char_indices() {
            if sentence_endings.contains(&c) {
                count += 1;
                end_pos = i + 1;
                if count >= n {
                    break;
                }
            }
        }

        if count >= n {
            s[..end_pos].trim().to_string()
        } else {
            s.to_string()
        }
    }

    /// Truncate lines.
    pub fn lines(s: &str, n: usize) -> String {
        let lines: Vec<&str> = s.lines().collect();
        if lines.len() <= n {
            return s.to_string();
        }
        format!("{}...", lines[..n].join("\n"))
    }

    /// Truncate path (preserve filename).
    pub fn path(s: &str, max_length: usize) -> String {
        if s.len() <= max_length {
            return s.to_string();
        }

        // Find last path separator
        let sep_pos = s.rfind('/').or_else(|| s.rfind('\\'));

        if let Some(pos) = sep_pos {
            let filename = &s[pos + 1..];
            if filename.len() + 4 >= max_length {
                // Filename too long, just truncate
                return Self::end(filename, max_length);
            }

            let available = max_length - filename.len() - 4; // ".../" + filename
            let dir = &s[..pos];

            if dir.len() <= available {
                return s.to_string();
            }

            format!(".../{}", filename)
        } else {
            Self::end(s, max_length)
        }
    }

    /// Smart truncate (picks best position based on content).
    pub fn smart(s: &str, max_length: usize) -> String {
        if s.len() <= max_length {
            return s.to_string();
        }

        // For file paths, use path truncation
        if s.contains('/') || s.contains('\\') {
            return Self::path(s, max_length);
        }

        // For text with sentences, try to end at sentence boundary
        let sentence_endings = ['.', '!', '?'];
        let truncated = Self::end(s, max_length);

        // Check if we can end at a sentence
        let without_ellipsis = &truncated[..truncated.len().saturating_sub(3)];
        if let Some(pos) = without_ellipsis.rfind(|c: char| sentence_endings.contains(&c)) {
            if pos > max_length / 2 {
                return format!("{}", &s[..=pos]);
            }
        }

        // Default to word-aware truncation
        Self::words(s, max_length)
    }
}

/// Quick truncate function.
pub fn truncate(s: &str, max_length: usize) -> String {
    Truncate::end(s, max_length)
}

/// Truncate with ellipsis.
pub fn truncate_ellipsis(s: &str, max_length: usize, ellipsis: &str) -> String {
    Truncate::end_with(s, max_length, ellipsis)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_truncation_needed() {
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate("hello", 5), "hello");
    }

    #[test]
    fn test_basic_truncation() {
        assert_eq!(truncate("hello world", 8), "hello...");
    }

    #[test]
    fn test_custom_ellipsis() {
        assert_eq!(truncate_ellipsis("hello world", 9, "…"), "hello wo…");
    }

    #[test]
    fn test_truncate_start() {
        assert_eq!(Truncate::start("hello world", 8), "...world");
    }

    #[test]
    fn test_truncate_middle() {
        let result = Truncate::middle("hello beautiful world", 15);
        assert!(result.contains("..."));
        assert!(result.len() <= 15);
    }

    #[test]
    fn test_truncate_words() {
        let result = Truncate::words("hello beautiful world", 15);
        assert!(result.ends_with("..."));
        // Should break at word boundary
        assert!(!result.contains("beauti..."));
    }

    #[test]
    fn test_truncate_n_words() {
        assert_eq!(Truncate::n_words("one two three four", 2), "one two...");
        assert_eq!(Truncate::n_words("one two", 5), "one two");
    }

    #[test]
    fn test_truncate_n_sentences() {
        let text = "First sentence. Second sentence! Third?";
        assert_eq!(Truncate::n_sentences(text, 1), "First sentence.");
        assert_eq!(
            Truncate::n_sentences(text, 2),
            "First sentence. Second sentence!"
        );
    }

    #[test]
    fn test_truncate_lines() {
        let text = "line 1\nline 2\nline 3";
        assert_eq!(Truncate::lines(text, 2), "line 1\nline 2...");
    }

    #[test]
    fn test_truncate_path() {
        let path = "/very/long/path/to/file.txt";
        let result = Truncate::path(path, 20);
        assert!(result.contains("file.txt"));
        assert!(result.len() <= 20);
    }

    #[test]
    fn test_very_short_max() {
        assert_eq!(truncate("hello", 3), "...");
        assert_eq!(truncate("hello", 2), "..");
    }

    #[test]
    fn test_options_builder() {
        let options = TruncateOptions::new(20)
            .ellipsis("…")
            .position(Position::Middle)
            .preserve_words();

        assert_eq!(options.max_length, 20);
        assert_eq!(options.ellipsis, "…");
        assert_eq!(options.position, Position::Middle);
        assert!(options.preserve_words);
    }

    #[test]
    fn test_smart_truncate() {
        // Text with sentence
        let text = "First sentence. Second sentence starts here and goes on.";
        let result = Truncate::smart(text, 30);
        // Should try to end at sentence boundary
        assert!(result.len() <= 30);

        // Path
        let path = "/some/long/path/to/file.txt";
        let result = Truncate::smart(path, 20);
        assert!(result.contains("file.txt") || result.contains("..."));
    }
}
