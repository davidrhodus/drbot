//! String utilities for drbot.
//!
//! This crate provides:
//! - String manipulation helpers
//! - Case conversion
//! - Pattern matching
//! - Text parsing

use regex::Regex;

/// String extension trait.
pub trait StringExt {
    /// Check if string is blank (empty or whitespace only).
    fn is_blank(&self) -> bool;

    /// Check if string is not blank.
    fn is_not_blank(&self) -> bool {
        !self.is_blank()
    }

    /// Truncate to max length.
    fn truncate_to(&self, max_len: usize) -> String;

    /// Truncate with suffix.
    fn truncate_with(&self, max_len: usize, suffix: &str) -> String;

    /// Get left n characters.
    fn left(&self, n: usize) -> &str;

    /// Get right n characters.
    fn right(&self, n: usize) -> &str;

    /// Remove prefix if present.
    fn remove_prefix(&self, prefix: &str) -> &str;

    /// Remove suffix if present.
    fn remove_suffix(&self, suffix: &str) -> &str;

    /// Pad left to width.
    fn pad_left(&self, width: usize, pad: char) -> String;

    /// Pad right to width.
    fn pad_right(&self, width: usize, pad: char) -> String;

    /// Center in width.
    fn center(&self, width: usize, pad: char) -> String;

    /// Count occurrences of substring.
    fn count_occurrences(&self, sub: &str) -> usize;

    /// Check if contains any of the given patterns.
    fn contains_any(&self, patterns: &[&str]) -> bool;

    /// Check if contains all of the given patterns.
    fn contains_all(&self, patterns: &[&str]) -> bool;

    /// Split and trim each part.
    fn split_trim(&self, sep: &str) -> Vec<String>;
}

impl StringExt for str {
    fn is_blank(&self) -> bool {
        self.trim().is_empty()
    }

    fn truncate_to(&self, max_len: usize) -> String {
        if self.chars().count() <= max_len {
            self.to_string()
        } else {
            self.chars().take(max_len).collect()
        }
    }

    fn truncate_with(&self, max_len: usize, suffix: &str) -> String {
        let char_count = self.chars().count();
        if char_count <= max_len {
            self.to_string()
        } else {
            let take = max_len.saturating_sub(suffix.chars().count());
            format!("{}{}", self.chars().take(take).collect::<String>(), suffix)
        }
    }

    fn left(&self, n: usize) -> &str {
        let end = self
            .char_indices()
            .nth(n)
            .map(|(i, _)| i)
            .unwrap_or(self.len());
        &self[..end]
    }

    fn right(&self, n: usize) -> &str {
        let char_count = self.chars().count();
        if n >= char_count {
            self
        } else {
            let start = self
                .char_indices()
                .nth(char_count - n)
                .map(|(i, _)| i)
                .unwrap_or(0);
            &self[start..]
        }
    }

    fn remove_prefix(&self, prefix: &str) -> &str {
        self.strip_prefix(prefix).unwrap_or(self)
    }

    fn remove_suffix(&self, suffix: &str) -> &str {
        self.strip_suffix(suffix).unwrap_or(self)
    }

    fn pad_left(&self, width: usize, pad: char) -> String {
        let char_count = self.chars().count();
        if char_count >= width {
            self.to_string()
        } else {
            let padding: String = std::iter::repeat(pad).take(width - char_count).collect();
            format!("{}{}", padding, self)
        }
    }

    fn pad_right(&self, width: usize, pad: char) -> String {
        let char_count = self.chars().count();
        if char_count >= width {
            self.to_string()
        } else {
            let padding: String = std::iter::repeat(pad).take(width - char_count).collect();
            format!("{}{}", self, padding)
        }
    }

    fn center(&self, width: usize, pad: char) -> String {
        let char_count = self.chars().count();
        if char_count >= width {
            self.to_string()
        } else {
            let total_pad = width - char_count;
            let left_pad = total_pad / 2;
            let right_pad = total_pad - left_pad;
            let left: String = std::iter::repeat(pad).take(left_pad).collect();
            let right: String = std::iter::repeat(pad).take(right_pad).collect();
            format!("{}{}{}", left, self, right)
        }
    }

    fn count_occurrences(&self, sub: &str) -> usize {
        if sub.is_empty() {
            return 0;
        }
        self.matches(sub).count()
    }

    fn contains_any(&self, patterns: &[&str]) -> bool {
        patterns.iter().any(|p| self.contains(p))
    }

    fn contains_all(&self, patterns: &[&str]) -> bool {
        patterns.iter().all(|p| self.contains(p))
    }

    fn split_trim(&self, sep: &str) -> Vec<String> {
        self.split(sep)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    }
}

impl StringExt for String {
    fn is_blank(&self) -> bool {
        self.as_str().is_blank()
    }

    fn truncate_to(&self, max_len: usize) -> String {
        self.as_str().truncate_to(max_len)
    }

    fn truncate_with(&self, max_len: usize, suffix: &str) -> String {
        self.as_str().truncate_with(max_len, suffix)
    }

    fn left(&self, n: usize) -> &str {
        self.as_str().left(n)
    }

    fn right(&self, n: usize) -> &str {
        self.as_str().right(n)
    }

    fn remove_prefix(&self, prefix: &str) -> &str {
        self.as_str().remove_prefix(prefix)
    }

    fn remove_suffix(&self, suffix: &str) -> &str {
        self.as_str().remove_suffix(suffix)
    }

    fn pad_left(&self, width: usize, pad: char) -> String {
        self.as_str().pad_left(width, pad)
    }

    fn pad_right(&self, width: usize, pad: char) -> String {
        self.as_str().pad_right(width, pad)
    }

    fn center(&self, width: usize, pad: char) -> String {
        self.as_str().center(width, pad)
    }

    fn count_occurrences(&self, sub: &str) -> usize {
        self.as_str().count_occurrences(sub)
    }

    fn contains_any(&self, patterns: &[&str]) -> bool {
        self.as_str().contains_any(patterns)
    }

    fn contains_all(&self, patterns: &[&str]) -> bool {
        self.as_str().contains_all(patterns)
    }

    fn split_trim(&self, sep: &str) -> Vec<String> {
        self.as_str().split_trim(sep)
    }
}

/// Case utilities.
pub struct Case;

impl Case {
    /// Convert to snake_case.
    pub fn to_snake(s: &str) -> String {
        let mut result = String::new();
        for (i, c) in s.chars().enumerate() {
            if c.is_uppercase() {
                if i > 0 {
                    result.push('_');
                }
                result.push(c.to_ascii_lowercase());
            } else if c == '-' || c == ' ' {
                result.push('_');
            } else {
                result.push(c);
            }
        }
        result
    }

    /// Convert to kebab-case.
    pub fn to_kebab(s: &str) -> String {
        Self::to_snake(s).replace('_', "-")
    }

    /// Convert to camelCase.
    pub fn to_camel(s: &str) -> String {
        let mut result = String::new();
        let mut capitalize_next = false;

        for c in s.chars() {
            if c == '_' || c == '-' || c == ' ' {
                capitalize_next = true;
            } else if capitalize_next {
                result.push(c.to_ascii_uppercase());
                capitalize_next = false;
            } else {
                result.push(c.to_ascii_lowercase());
            }
        }
        result
    }

    /// Convert to PascalCase.
    pub fn to_pascal(s: &str) -> String {
        let camel = Self::to_camel(s);
        let mut chars = camel.chars();
        match chars.next() {
            None => String::new(),
            Some(c) => c.to_uppercase().chain(chars).collect(),
        }
    }

    /// Convert to Title Case.
    pub fn to_title(s: &str) -> String {
        s.split_whitespace()
            .map(|word| {
                let mut chars = word.chars();
                match chars.next() {
                    None => String::new(),
                    Some(c) => c
                        .to_uppercase()
                        .chain(chars.map(|c| c.to_ascii_lowercase()))
                        .collect(),
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// Pattern matcher using regex.
pub struct Patterns;

impl Patterns {
    /// Email pattern.
    pub fn email() -> Regex {
        Regex::new(r"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$").unwrap()
    }

    /// URL pattern.
    pub fn url() -> Regex {
        Regex::new(r"^https?://[^\s/$.?#].[^\s]*$").unwrap()
    }

    /// Phone pattern (US).
    pub fn phone_us() -> Regex {
        Regex::new(r"^(\+1)?[-.\s]?\(?\d{3}\)?[-.\s]?\d{3}[-.\s]?\d{4}$").unwrap()
    }

    /// UUID pattern.
    pub fn uuid() -> Regex {
        Regex::new(r"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$")
            .unwrap()
    }

    /// IP address pattern.
    pub fn ip_address() -> Regex {
        Regex::new(r"^(\d{1,3}\.){3}\d{1,3}$").unwrap()
    }

    /// Alphanumeric pattern.
    pub fn alphanumeric() -> Regex {
        Regex::new(r"^[a-zA-Z0-9]+$").unwrap()
    }

    /// Check if matches pattern.
    pub fn matches(pattern: &Regex, s: &str) -> bool {
        pattern.is_match(s)
    }
}

/// Text utilities.
pub struct Text;

impl Text {
    /// Extract words from text.
    pub fn words(s: &str) -> Vec<&str> {
        s.split_whitespace().collect()
    }

    /// Count words.
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

    /// Wrap text to width.
    pub fn wrap(s: &str, width: usize) -> Vec<String> {
        let mut lines = Vec::new();
        let mut current_line = String::new();

        for word in s.split_whitespace() {
            if current_line.is_empty() {
                current_line = word.to_string();
            } else if current_line.len() + 1 + word.len() <= width {
                current_line.push(' ');
                current_line.push_str(word);
            } else {
                lines.push(current_line);
                current_line = word.to_string();
            }
        }

        if !current_line.is_empty() {
            lines.push(current_line);
        }

        lines
    }

    /// Indent text.
    pub fn indent(s: &str, indent: &str) -> String {
        s.lines()
            .map(|line| format!("{}{}", indent, line))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Dedent text (remove common leading whitespace).
    pub fn dedent(s: &str) -> String {
        let lines: Vec<&str> = s.lines().collect();
        let non_empty: Vec<&&str> = lines.iter().filter(|l| !l.trim().is_empty()).collect();

        if non_empty.is_empty() {
            return s.to_string();
        }

        let min_indent = non_empty
            .iter()
            .map(|l| l.len() - l.trim_start().len())
            .min()
            .unwrap_or(0);

        lines
            .iter()
            .map(|l| {
                if l.len() >= min_indent {
                    &l[min_indent..]
                } else {
                    l.trim()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
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
    fn test_truncate() {
        assert_eq!("hello world".truncate_to(5), "hello");
        assert_eq!("hello world".truncate_with(8, "..."), "hello...");
    }

    #[test]
    fn test_left_right() {
        assert_eq!("hello".left(3), "hel");
        assert_eq!("hello".right(3), "llo");
    }

    #[test]
    fn test_padding() {
        assert_eq!("42".pad_left(5, '0'), "00042");
        assert_eq!("hi".pad_right(5, ' '), "hi   ");
        assert_eq!("hi".center(6, '-'), "--hi--");
    }

    #[test]
    fn test_count_occurrences() {
        assert_eq!("hello".count_occurrences("l"), 2);
        assert_eq!("aaa".count_occurrences("aa"), 1); // Non-overlapping
    }

    #[test]
    fn test_case_conversion() {
        assert_eq!(Case::to_snake("helloWorld"), "hello_world");
        assert_eq!(Case::to_kebab("helloWorld"), "hello-world");
        assert_eq!(Case::to_camel("hello_world"), "helloWorld");
        assert_eq!(Case::to_pascal("hello_world"), "HelloWorld");
        assert_eq!(Case::to_title("hello world"), "Hello World");
    }

    #[test]
    fn test_patterns() {
        assert!(Patterns::matches(&Patterns::email(), "test@example.com"));
        assert!(!Patterns::matches(&Patterns::email(), "invalid"));
        assert!(Patterns::matches(
            &Patterns::uuid(),
            "550e8400-e29b-41d4-a716-446655440000"
        ));
    }

    #[test]
    fn test_text_utils() {
        assert_eq!(Text::word_count("hello world"), 2);
        assert_eq!(Text::line_count("a\nb\nc"), 3);

        let wrapped = Text::wrap("hello world foo bar", 10);
        assert!(wrapped.len() >= 2);

        let indented = Text::indent("hello\nworld", "  ");
        assert!(indented.starts_with("  hello"));
    }

    #[test]
    fn test_split_trim() {
        let result = "a , b , c ".split_trim(",");
        assert_eq!(result, vec!["a", "b", "c"]);
    }
}
