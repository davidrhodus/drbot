//! Text manipulation utilities for drbot.
//!
//! This crate provides:
//! - Text normalization
//! - Whitespace handling
//! - Line operations
//! - Word operations

use thiserror::Error;

/// Text error types.
#[derive(Error, Debug)]
pub enum TextError {
    #[error("Invalid input: {0}")]
    InvalidInput(String),
}

/// Result type for text operations.
pub type Result<T> = std::result::Result<T, TextError>;

/// Text utilities.
pub struct Text;

impl Text {
    /// Normalize whitespace (collapse multiple spaces, trim).
    pub fn normalize_whitespace(s: &str) -> String {
        let mut result = String::new();
        let mut prev_space = true; // Start true to trim leading

        for c in s.chars() {
            if c.is_whitespace() {
                if !prev_space {
                    result.push(' ');
                    prev_space = true;
                }
            } else {
                result.push(c);
                prev_space = false;
            }
        }

        // Trim trailing space
        if result.ends_with(' ') {
            result.pop();
        }

        result
    }

    /// Remove all whitespace.
    pub fn remove_whitespace(s: &str) -> String {
        s.chars().filter(|c| !c.is_whitespace()).collect()
    }

    /// Collapse newlines to single newline.
    pub fn collapse_newlines(s: &str) -> String {
        let mut result = String::new();
        let mut prev_newline = false;

        for c in s.chars() {
            if c == '\n' {
                if !prev_newline {
                    result.push('\n');
                    prev_newline = true;
                }
            } else {
                result.push(c);
                prev_newline = false;
            }
        }

        result
    }

    /// Normalize line endings to LF.
    pub fn normalize_line_endings(s: &str) -> String {
        s.replace("\r\n", "\n").replace('\r', "\n")
    }

    /// Count words in text.
    pub fn word_count(s: &str) -> usize {
        s.split_whitespace().count()
    }

    /// Count characters (excluding whitespace).
    pub fn char_count(s: &str) -> usize {
        s.chars().filter(|c| !c.is_whitespace()).count()
    }

    /// Count lines.
    pub fn line_count(s: &str) -> usize {
        if s.is_empty() {
            0
        } else {
            s.lines().count()
        }
    }

    /// Get words from text.
    pub fn words(s: &str) -> Vec<&str> {
        s.split_whitespace().collect()
    }

    /// Get lines from text.
    pub fn lines(s: &str) -> Vec<&str> {
        s.lines().collect()
    }

    /// Get first n words.
    pub fn first_n_words(s: &str, n: usize) -> String {
        s.split_whitespace().take(n).collect::<Vec<_>>().join(" ")
    }

    /// Get last n words.
    pub fn last_n_words(s: &str, n: usize) -> String {
        let words: Vec<&str> = s.split_whitespace().collect();
        let start = words.len().saturating_sub(n);
        words[start..].join(" ")
    }

    /// Reverse text.
    pub fn reverse(s: &str) -> String {
        s.chars().rev().collect()
    }

    /// Reverse words (not characters).
    pub fn reverse_words(s: &str) -> String {
        s.split_whitespace().rev().collect::<Vec<_>>().join(" ")
    }

    /// Remove duplicate words.
    pub fn dedupe_words(s: &str) -> String {
        let mut seen = std::collections::HashSet::new();
        s.split_whitespace()
            .filter(|word| seen.insert(*word))
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Check if text is blank (empty or only whitespace).
    pub fn is_blank(s: &str) -> bool {
        s.trim().is_empty()
    }

    /// Check if text is numeric.
    pub fn is_numeric(s: &str) -> bool {
        !s.is_empty() && s.chars().all(|c| c.is_ascii_digit())
    }

    /// Check if text is alphabetic.
    pub fn is_alphabetic(s: &str) -> bool {
        !s.is_empty() && s.chars().all(|c| c.is_alphabetic())
    }

    /// Check if text is alphanumeric.
    pub fn is_alphanumeric(s: &str) -> bool {
        !s.is_empty() && s.chars().all(|c| c.is_alphanumeric())
    }

    /// Remove prefix if present.
    pub fn strip_prefix<'a>(s: &'a str, prefix: &str) -> &'a str {
        s.strip_prefix(prefix).unwrap_or(s)
    }

    /// Remove suffix if present.
    pub fn strip_suffix<'a>(s: &'a str, suffix: &str) -> &'a str {
        s.strip_suffix(suffix).unwrap_or(s)
    }

    /// Ensure string starts with prefix.
    pub fn ensure_prefix(s: &str, prefix: &str) -> String {
        if s.starts_with(prefix) {
            s.to_string()
        } else {
            format!("{}{}", prefix, s)
        }
    }

    /// Ensure string ends with suffix.
    pub fn ensure_suffix(s: &str, suffix: &str) -> String {
        if s.ends_with(suffix) {
            s.to_string()
        } else {
            format!("{}{}", s, suffix)
        }
    }

    /// Squeeze repeated characters.
    pub fn squeeze(s: &str, c: char) -> String {
        let mut result = String::new();
        let mut prev = None;

        for ch in s.chars() {
            if Some(ch) == prev && ch == c {
                continue;
            }
            result.push(ch);
            prev = Some(ch);
        }

        result
    }

    /// Replace tabs with spaces.
    pub fn expand_tabs(s: &str, tab_width: usize) -> String {
        s.replace('\t', &" ".repeat(tab_width))
    }
}

/// Line operations.
pub struct Lines;

impl Lines {
    /// Indent all lines.
    pub fn indent(s: &str, indent: &str) -> String {
        s.lines()
            .map(|line| format!("{}{}", indent, line))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Indent all lines with spaces.
    pub fn indent_spaces(s: &str, count: usize) -> String {
        Self::indent(s, &" ".repeat(count))
    }

    /// Dedent (remove common leading whitespace).
    pub fn dedent(s: &str) -> String {
        let lines: Vec<&str> = s.lines().collect();

        // Find minimum indent (ignoring blank lines)
        let min_indent = lines
            .iter()
            .filter(|line| !line.trim().is_empty())
            .map(|line| line.len() - line.trim_start().len())
            .min()
            .unwrap_or(0);

        lines
            .iter()
            .map(|line| {
                if line.len() >= min_indent {
                    &line[min_indent..]
                } else {
                    *line
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Wrap lines at width.
    pub fn wrap(s: &str, width: usize) -> String {
        let mut result = Vec::new();
        let mut current_line = String::new();

        for word in s.split_whitespace() {
            if current_line.is_empty() {
                current_line = word.to_string();
            } else if current_line.len() + 1 + word.len() <= width {
                current_line.push(' ');
                current_line.push_str(word);
            } else {
                result.push(current_line);
                current_line = word.to_string();
            }
        }

        if !current_line.is_empty() {
            result.push(current_line);
        }

        result.join("\n")
    }

    /// Unwrap lines (join wrapped lines).
    pub fn unwrap(s: &str) -> String {
        s.lines()
            .map(|line| line.trim())
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Number lines.
    pub fn number(s: &str) -> String {
        s.lines()
            .enumerate()
            .map(|(i, line)| format!("{:4} {}", i + 1, line))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Number lines with custom format.
    pub fn number_with_format(s: &str, format: &str) -> String {
        s.lines()
            .enumerate()
            .map(|(i, line)| {
                format
                    .replace("{n}", &(i + 1).to_string())
                    .replace("{line}", line)
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Prefix all lines.
    pub fn prefix(s: &str, prefix: &str) -> String {
        s.lines()
            .map(|line| format!("{}{}", prefix, line))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Suffix all lines.
    pub fn suffix(s: &str, suffix: &str) -> String {
        s.lines()
            .map(|line| format!("{}{}", line, suffix))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Filter lines matching predicate.
    pub fn filter<F>(s: &str, pred: F) -> String
    where
        F: Fn(&str) -> bool,
    {
        s.lines()
            .filter(|line| pred(line))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Filter out blank lines.
    pub fn filter_blank(s: &str) -> String {
        Self::filter(s, |line| !line.trim().is_empty())
    }

    /// Take first n lines.
    pub fn head(s: &str, n: usize) -> String {
        s.lines().take(n).collect::<Vec<_>>().join("\n")
    }

    /// Take last n lines.
    pub fn tail(s: &str, n: usize) -> String {
        let lines: Vec<&str> = s.lines().collect();
        let start = lines.len().saturating_sub(n);
        lines[start..].join("\n")
    }

    /// Sort lines.
    pub fn sort(s: &str) -> String {
        let mut lines: Vec<&str> = s.lines().collect();
        lines.sort();
        lines.join("\n")
    }

    /// Sort lines in reverse.
    pub fn sort_reverse(s: &str) -> String {
        let mut lines: Vec<&str> = s.lines().collect();
        lines.sort_by(|a, b| b.cmp(a));
        lines.join("\n")
    }

    /// Unique lines (preserve order).
    pub fn unique(s: &str) -> String {
        let mut seen = std::collections::HashSet::new();
        s.lines()
            .filter(|line| seen.insert(*line))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Reverse line order.
    pub fn reverse(s: &str) -> String {
        s.lines().rev().collect::<Vec<_>>().join("\n")
    }
}

/// Word operations.
pub struct Words;

impl Words {
    /// Extract words matching pattern.
    pub fn extract_matching<'a>(s: &'a str, pattern: &str) -> Vec<&'a str> {
        s.split_whitespace()
            .filter(|word| word.contains(pattern))
            .collect()
    }

    /// Extract words of specific length.
    pub fn of_length(s: &str, len: usize) -> Vec<&str> {
        s.split_whitespace()
            .filter(|word| word.len() == len)
            .collect()
    }

    /// Get longest word.
    pub fn longest(s: &str) -> Option<&str> {
        s.split_whitespace().max_by_key(|word| word.len())
    }

    /// Get shortest word.
    pub fn shortest(s: &str) -> Option<&str> {
        s.split_whitespace().min_by_key(|word| word.len())
    }

    /// Count word frequency.
    pub fn frequency(s: &str) -> std::collections::HashMap<String, usize> {
        let mut freq = std::collections::HashMap::new();
        for word in s.split_whitespace() {
            *freq.entry(word.to_lowercase()).or_insert(0) += 1;
        }
        freq
    }

    /// Get most common words.
    pub fn most_common(s: &str, n: usize) -> Vec<(String, usize)> {
        let freq = Self::frequency(s);
        let mut items: Vec<_> = freq.into_iter().collect();
        items.sort_by(|a, b| b.1.cmp(&a.1));
        items.truncate(n);
        items
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_whitespace() {
        assert_eq!(
            Text::normalize_whitespace("  hello   world  "),
            "hello world"
        );
        assert_eq!(Text::normalize_whitespace("a\n\nb"), "a b");
    }

    #[test]
    fn test_word_count() {
        assert_eq!(Text::word_count("hello world"), 2);
        assert_eq!(Text::word_count("  one  two  three  "), 3);
    }

    #[test]
    fn test_line_count() {
        assert_eq!(Text::line_count("hello\nworld"), 2);
        assert_eq!(Text::line_count("single"), 1);
        assert_eq!(Text::line_count(""), 0);
    }

    #[test]
    fn test_reverse() {
        assert_eq!(Text::reverse("hello"), "olleh");
        assert_eq!(Text::reverse_words("hello world"), "world hello");
    }

    #[test]
    fn test_ensure_prefix_suffix() {
        assert_eq!(Text::ensure_prefix("world", "hello "), "hello world");
        assert_eq!(Text::ensure_prefix("hello world", "hello "), "hello world");
        assert_eq!(Text::ensure_suffix("hello", " world"), "hello world");
    }

    #[test]
    fn test_indent() {
        assert_eq!(Lines::indent("a\nb", "  "), "  a\n  b");
        assert_eq!(Lines::indent_spaces("a\nb", 4), "    a\n    b");
    }

    #[test]
    fn test_dedent() {
        let input = "    hello\n    world";
        assert_eq!(Lines::dedent(input), "hello\nworld");
    }

    #[test]
    fn test_wrap() {
        let text = "hello world foo bar";
        let wrapped = Lines::wrap(text, 10);
        assert!(wrapped.contains('\n'));
    }

    #[test]
    fn test_head_tail() {
        let text = "a\nb\nc\nd\ne";
        assert_eq!(Lines::head(text, 2), "a\nb");
        assert_eq!(Lines::tail(text, 2), "d\ne");
    }

    #[test]
    fn test_sort() {
        assert_eq!(Lines::sort("c\na\nb"), "a\nb\nc");
    }

    #[test]
    fn test_unique() {
        assert_eq!(Lines::unique("a\nb\na\nc"), "a\nb\nc");
    }

    #[test]
    fn test_word_frequency() {
        let freq = Words::frequency("hello world hello");
        assert_eq!(freq.get("hello"), Some(&2));
        assert_eq!(freq.get("world"), Some(&1));
    }
}
