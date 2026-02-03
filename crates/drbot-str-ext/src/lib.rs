//! String slice extensions for drbot.
//!
//! This crate provides:
//! - str extension methods
//! - String manipulation helpers
//! - Character utilities

use thiserror::Error;

/// String error types.
#[derive(Error, Debug, Clone)]
pub enum StrError {
    #[error("Empty string")]
    Empty,

    #[error("Index out of bounds")]
    IndexOutOfBounds,

    #[error("Invalid UTF-8")]
    InvalidUtf8,
}

/// Result type for string operations.
pub type Result<T> = std::result::Result<T, StrError>;

/// str extension trait.
pub trait StrExt {
    /// Is blank (empty or whitespace).
    fn is_blank(&self) -> bool;

    /// First char.
    fn first_char(&self) -> Option<char>;

    /// Last char.
    fn last_char(&self) -> Option<char>;

    /// Char at index.
    fn char_at(&self, index: usize) -> Option<char>;

    /// Count chars.
    fn char_count(&self) -> usize;

    /// Count lines.
    fn line_count(&self) -> usize;

    /// Count words.
    fn word_count(&self) -> usize;

    /// Reverse.
    fn reverse_chars(&self) -> String;

    /// Remove prefix.
    fn strip_prefix_if(&self, prefix: &str) -> &str;

    /// Remove suffix.
    fn strip_suffix_if(&self, suffix: &str) -> &str;

    /// Truncate to max chars.
    fn truncate_chars(&self, max: usize) -> &str;

    /// Ellipsis if too long.
    fn ellipsis(&self, max: usize) -> String;

    /// Take first n chars.
    fn take_chars(&self, n: usize) -> &str;

    /// Skip first n chars.
    fn skip_chars(&self, n: usize) -> &str;
}

impl StrExt for str {
    fn is_blank(&self) -> bool {
        self.trim().is_empty()
    }

    fn first_char(&self) -> Option<char> {
        self.chars().next()
    }

    fn last_char(&self) -> Option<char> {
        self.chars().last()
    }

    fn char_at(&self, index: usize) -> Option<char> {
        self.chars().nth(index)
    }

    fn char_count(&self) -> usize {
        self.chars().count()
    }

    fn line_count(&self) -> usize {
        if self.is_empty() {
            0
        } else {
            self.lines().count()
        }
    }

    fn word_count(&self) -> usize {
        self.split_whitespace().count()
    }

    fn reverse_chars(&self) -> String {
        self.chars().rev().collect()
    }

    fn strip_prefix_if(&self, prefix: &str) -> &str {
        self.strip_prefix(prefix).unwrap_or(self)
    }

    fn strip_suffix_if(&self, suffix: &str) -> &str {
        self.strip_suffix(suffix).unwrap_or(self)
    }

    fn truncate_chars(&self, max: usize) -> &str {
        if self.char_count() <= max {
            self
        } else {
            let end = self
                .char_indices()
                .nth(max)
                .map(|(i, _)| i)
                .unwrap_or(self.len());
            &self[..end]
        }
    }

    fn ellipsis(&self, max: usize) -> String {
        if self.char_count() <= max {
            self.to_string()
        } else if max <= 3 {
            ".".repeat(max)
        } else {
            format!("{}...", self.truncate_chars(max - 3))
        }
    }

    fn take_chars(&self, n: usize) -> &str {
        self.truncate_chars(n)
    }

    fn skip_chars(&self, n: usize) -> &str {
        if n >= self.char_count() {
            ""
        } else {
            let start = self
                .char_indices()
                .nth(n)
                .map(|(i, _)| i)
                .unwrap_or(self.len());
            &self[start..]
        }
    }
}

/// Check if string contains only digits.
pub fn is_numeric(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_digit())
}

/// Check if string contains only alphabetic chars.
pub fn is_alphabetic(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_alphabetic())
}

/// Check if string contains only alphanumeric chars.
pub fn is_alphanumeric(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_alphanumeric())
}

/// Check if string is ASCII.
pub fn is_ascii(s: &str) -> bool {
    s.is_ascii()
}

/// Check if string is lowercase.
pub fn is_lowercase(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| !c.is_uppercase())
}

/// Check if string is uppercase.
pub fn is_uppercase(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| !c.is_lowercase())
}

/// Find common prefix.
pub fn common_prefix<'a>(a: &'a str, b: &str) -> &'a str {
    let len = a
        .chars()
        .zip(b.chars())
        .take_while(|(ca, cb)| ca == cb)
        .count();
    a.truncate_chars(len)
}

/// Find common suffix.
pub fn common_suffix<'a>(a: &'a str, b: &str) -> &'a str {
    let len = a
        .chars()
        .rev()
        .zip(b.chars().rev())
        .take_while(|(ca, cb)| ca == cb)
        .count();
    if len == 0 {
        ""
    } else {
        let start = a
            .char_indices()
            .rev()
            .nth(len - 1)
            .map(|(i, _)| i)
            .unwrap_or(0);
        &a[start..]
    }
}

/// Repeat string n times.
pub fn repeat(s: &str, n: usize) -> String {
    s.repeat(n)
}

/// Pad left.
pub fn pad_left(s: &str, width: usize, ch: char) -> String {
    let char_count = s.char_count();
    if char_count >= width {
        s.to_string()
    } else {
        let padding: String = std::iter::repeat(ch).take(width - char_count).collect();
        format!("{}{}", padding, s)
    }
}

/// Pad right.
pub fn pad_right(s: &str, width: usize, ch: char) -> String {
    let char_count = s.char_count();
    if char_count >= width {
        s.to_string()
    } else {
        let padding: String = std::iter::repeat(ch).take(width - char_count).collect();
        format!("{}{}", s, padding)
    }
}

/// Center string.
pub fn center(s: &str, width: usize, ch: char) -> String {
    let char_count = s.char_count();
    if char_count >= width {
        s.to_string()
    } else {
        let total_pad = width - char_count;
        let left_pad = total_pad / 2;
        let right_pad = total_pad - left_pad;
        let left: String = std::iter::repeat(ch).take(left_pad).collect();
        let right: String = std::iter::repeat(ch).take(right_pad).collect();
        format!("{}{}{}", left, s, right)
    }
}

/// Remove multiple spaces.
pub fn collapse_whitespace(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut prev_ws = false;
    for c in s.chars() {
        if c.is_whitespace() {
            if !prev_ws {
                result.push(' ');
            }
            prev_ws = true;
        } else {
            result.push(c);
            prev_ws = false;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_blank() {
        assert!("".is_blank());
        assert!("   ".is_blank());
        assert!(!"hello".is_blank());
    }

    #[test]
    fn test_char_operations() {
        let s = "hello";
        assert_eq!(s.first_char(), Some('h'));
        assert_eq!(s.last_char(), Some('o'));
        assert_eq!(s.char_at(2), Some('l'));
        assert_eq!(s.char_count(), 5);
    }

    #[test]
    fn test_reverse() {
        assert_eq!("hello".reverse_chars(), "olleh");
    }

    #[test]
    fn test_truncate_and_ellipsis() {
        let s = "hello world";
        assert_eq!(s.truncate_chars(5), "hello");
        assert_eq!(s.ellipsis(8), "hello...");
    }

    #[test]
    fn test_common_prefix() {
        assert_eq!(common_prefix("hello", "help"), "hel");
        assert_eq!(common_prefix("abc", "xyz"), "");
    }

    #[test]
    fn test_padding() {
        assert_eq!(pad_left("42", 5, '0'), "00042");
        assert_eq!(pad_right("hi", 5, '-'), "hi---");
        assert_eq!(center("hi", 6, '*'), "**hi**");
    }
}
