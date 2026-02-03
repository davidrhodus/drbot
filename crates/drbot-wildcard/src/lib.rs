//! Wildcard pattern matching for drbot.
//!
//! This crate provides:
//! - Glob-like wildcard matching
//! - `*` matches any sequence
//! - `?` matches single character
//! - Character classes [abc]

use thiserror::Error;

/// Wildcard error types.
#[derive(Error, Debug)]
pub enum WildcardError {
    #[error("Invalid pattern: {0}")]
    InvalidPattern(String),

    #[error("Unclosed bracket")]
    UnclosedBracket,
}

/// Result type for wildcard operations.
pub type Result<T> = std::result::Result<T, WildcardError>;

/// Compiled wildcard pattern.
#[derive(Debug, Clone)]
pub struct Wildcard {
    parts: Vec<WildcardPart>,
    original: String,
}

#[derive(Debug, Clone)]
enum WildcardPart {
    Literal(String),
    AnyOne,                     // ?
    AnyMany,                    // *
    CharClass(Vec<char>, bool), // [abc] or [^abc]
}

impl Wildcard {
    /// Compile wildcard pattern.
    pub fn new(pattern: &str) -> Result<Self> {
        let mut parts = Vec::new();
        let mut chars = pattern.chars().peekable();
        let mut current_literal = String::new();

        while let Some(ch) = chars.next() {
            match ch {
                '*' => {
                    if !current_literal.is_empty() {
                        parts.push(WildcardPart::Literal(current_literal.clone()));
                        current_literal.clear();
                    }
                    // Collapse multiple stars
                    while chars.peek() == Some(&'*') {
                        chars.next();
                    }
                    parts.push(WildcardPart::AnyMany);
                }
                '?' => {
                    if !current_literal.is_empty() {
                        parts.push(WildcardPart::Literal(current_literal.clone()));
                        current_literal.clear();
                    }
                    parts.push(WildcardPart::AnyOne);
                }
                '[' => {
                    if !current_literal.is_empty() {
                        parts.push(WildcardPart::Literal(current_literal.clone()));
                        current_literal.clear();
                    }

                    let negated = chars.peek() == Some(&'^');
                    if negated {
                        chars.next();
                    }

                    let mut class_chars = Vec::new();
                    loop {
                        match chars.next() {
                            Some(']') => break,
                            Some(c) => class_chars.push(c),
                            None => return Err(WildcardError::UnclosedBracket),
                        }
                    }

                    parts.push(WildcardPart::CharClass(class_chars, negated));
                }
                '\\' => {
                    // Escape next character
                    if let Some(next) = chars.next() {
                        current_literal.push(next);
                    }
                }
                _ => {
                    current_literal.push(ch);
                }
            }
        }

        if !current_literal.is_empty() {
            parts.push(WildcardPart::Literal(current_literal));
        }

        Ok(Self {
            parts,
            original: pattern.to_string(),
        })
    }

    /// Get original pattern.
    pub fn pattern(&self) -> &str {
        &self.original
    }

    /// Check if string matches pattern.
    pub fn matches(&self, input: &str) -> bool {
        self.matches_recursive(&self.parts, input)
    }

    fn matches_recursive(&self, parts: &[WildcardPart], input: &str) -> bool {
        if parts.is_empty() {
            return input.is_empty();
        }

        let part = &parts[0];
        let rest = &parts[1..];

        match part {
            WildcardPart::Literal(lit) => {
                if input.starts_with(lit) {
                    self.matches_recursive(rest, &input[lit.len()..])
                } else {
                    false
                }
            }
            WildcardPart::AnyOne => {
                if input.is_empty() {
                    false
                } else {
                    let char_len = input.chars().next().unwrap().len_utf8();
                    self.matches_recursive(rest, &input[char_len..])
                }
            }
            WildcardPart::AnyMany => {
                // Try matching zero or more characters
                for i in 0..=input.len() {
                    // Only try at character boundaries
                    if input.is_char_boundary(i) {
                        if self.matches_recursive(rest, &input[i..]) {
                            return true;
                        }
                    }
                }
                false
            }
            WildcardPart::CharClass(chars, negated) => {
                if input.is_empty() {
                    return false;
                }

                let first = input.chars().next().unwrap();
                let in_class = chars.contains(&first);
                let matches = if *negated { !in_class } else { in_class };

                if matches {
                    self.matches_recursive(rest, &input[first.len_utf8()..])
                } else {
                    false
                }
            }
        }
    }

    /// Check if pattern has wildcards.
    pub fn has_wildcards(&self) -> bool {
        self.parts
            .iter()
            .any(|p| !matches!(p, WildcardPart::Literal(_)))
    }
}

/// Simple wildcard match function.
pub fn matches(pattern: &str, input: &str) -> bool {
    match Wildcard::new(pattern) {
        Ok(w) => w.matches(input),
        Err(_) => false,
    }
}

/// Case-insensitive wildcard match.
pub fn matches_ignore_case(pattern: &str, input: &str) -> bool {
    let pattern_lower = pattern.to_lowercase();
    let input_lower = input.to_lowercase();
    matches(&pattern_lower, &input_lower)
}

/// Filter items by wildcard pattern.
pub fn filter<'a, T, F>(pattern: &str, items: &'a [T], getter: F) -> Vec<&'a T>
where
    F: Fn(&T) -> &str,
{
    let wildcard = match Wildcard::new(pattern) {
        Ok(w) => w,
        Err(_) => return vec![],
    };

    items
        .iter()
        .filter(|item| wildcard.matches(getter(item)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_literal() {
        assert!(matches("hello", "hello"));
        assert!(!matches("hello", "world"));
    }

    #[test]
    fn test_star() {
        assert!(matches("*.txt", "file.txt"));
        assert!(matches("*.txt", ".txt"));
        assert!(matches("*", "anything"));
        assert!(matches("a*b", "ab"));
        assert!(matches("a*b", "aXXXb"));
    }

    #[test]
    fn test_question() {
        assert!(matches("?.txt", "a.txt"));
        assert!(!matches("?.txt", "ab.txt"));
        assert!(matches("???", "abc"));
    }

    #[test]
    fn test_char_class() {
        assert!(matches("[abc]", "a"));
        assert!(matches("[abc]", "b"));
        assert!(!matches("[abc]", "d"));
        assert!(matches("[^abc]", "d"));
        assert!(!matches("[^abc]", "a"));
    }

    #[test]
    fn test_complex() {
        assert!(matches("src/*.rs", "src/main.rs"));
        assert!(matches("test_?.txt", "test_1.txt"));
        assert!(matches("**", "anything"));
        assert!(matches("a*b*c", "aXbYc"));
    }

    #[test]
    fn test_escape() {
        assert!(matches("\\*", "*"));
        assert!(matches("\\?", "?"));
    }

    #[test]
    fn test_case_insensitive() {
        assert!(matches_ignore_case("*.TXT", "file.txt"));
        assert!(matches_ignore_case("Hello*", "hello world"));
    }
}
