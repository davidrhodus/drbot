//! Glob pattern matching for drbot.
//!
//! This crate provides:
//! - Glob pattern compilation
//! - Path matching
//! - Pattern utilities

use std::path::Path;
use thiserror::Error;

/// Glob error types.
#[derive(Error, Debug)]
pub enum GlobError {
    #[error("Invalid pattern: {0}")]
    InvalidPattern(String),

    #[error("Pattern error: {0}")]
    PatternError(String),
}

/// Result type for glob operations.
pub type Result<T> = std::result::Result<T, GlobError>;

/// Compiled glob pattern.
#[derive(Debug, Clone)]
pub struct Pattern {
    source: String,
    parts: Vec<PatternPart>,
    case_sensitive: bool,
}

#[derive(Debug, Clone)]
enum PatternPart {
    Literal(String),
    SingleChar,   // ?
    AnySequence,  // *
    RecursiveAny, // **
    CharClass(Vec<CharRange>),
    NegatedCharClass(Vec<CharRange>),
}

#[derive(Debug, Clone)]
struct CharRange {
    start: char,
    end: char,
}

impl CharRange {
    fn single(c: char) -> Self {
        Self { start: c, end: c }
    }

    fn range(start: char, end: char) -> Self {
        Self { start, end }
    }

    fn contains(&self, c: char) -> bool {
        c >= self.start && c <= self.end
    }
}

impl Pattern {
    /// Create new glob pattern.
    pub fn new(pattern: &str) -> Result<Self> {
        Self::with_options(pattern, true)
    }

    /// Create case-insensitive glob pattern.
    pub fn new_case_insensitive(pattern: &str) -> Result<Self> {
        Self::with_options(pattern, false)
    }

    /// Create with options.
    pub fn with_options(pattern: &str, case_sensitive: bool) -> Result<Self> {
        let parts = Self::compile(pattern)?;
        Ok(Self {
            source: pattern.to_string(),
            parts,
            case_sensitive,
        })
    }

    fn compile(pattern: &str) -> Result<Vec<PatternPart>> {
        let mut parts = Vec::new();
        let mut chars = pattern.chars().peekable();
        let mut literal = String::new();

        while let Some(c) = chars.next() {
            match c {
                '*' => {
                    if !literal.is_empty() {
                        parts.push(PatternPart::Literal(literal.clone()));
                        literal.clear();
                    }

                    if chars.peek() == Some(&'*') {
                        chars.next();
                        // Skip trailing slash for **
                        if chars.peek() == Some(&'/') {
                            chars.next();
                        }
                        parts.push(PatternPart::RecursiveAny);
                    } else {
                        parts.push(PatternPart::AnySequence);
                    }
                }
                '?' => {
                    if !literal.is_empty() {
                        parts.push(PatternPart::Literal(literal.clone()));
                        literal.clear();
                    }
                    parts.push(PatternPart::SingleChar);
                }
                '[' => {
                    if !literal.is_empty() {
                        parts.push(PatternPart::Literal(literal.clone()));
                        literal.clear();
                    }

                    let mut ranges = Vec::new();
                    let negated = chars.peek() == Some(&'!') || chars.peek() == Some(&'^');
                    if negated {
                        chars.next();
                    }

                    while let Some(c) = chars.next() {
                        if c == ']' {
                            break;
                        }

                        if chars.peek() == Some(&'-') {
                            chars.next();
                            if let Some(end) = chars.next() {
                                if end != ']' {
                                    ranges.push(CharRange::range(c, end));
                                    continue;
                                }
                            }
                        }

                        ranges.push(CharRange::single(c));
                    }

                    if negated {
                        parts.push(PatternPart::NegatedCharClass(ranges));
                    } else {
                        parts.push(PatternPart::CharClass(ranges));
                    }
                }
                '\\' => {
                    // Escape next character
                    if let Some(next) = chars.next() {
                        literal.push(next);
                    }
                }
                _ => {
                    literal.push(c);
                }
            }
        }

        if !literal.is_empty() {
            parts.push(PatternPart::Literal(literal));
        }

        Ok(parts)
    }

    /// Check if string matches pattern.
    pub fn matches(&self, text: &str) -> bool {
        self.matches_from(text, 0, 0)
    }

    fn matches_from(&self, text: &str, text_idx: usize, part_idx: usize) -> bool {
        let text_chars: Vec<char> = text.chars().collect();

        if part_idx >= self.parts.len() {
            return text_idx >= text_chars.len();
        }

        let part = &self.parts[part_idx];

        match part {
            PatternPart::Literal(lit) => {
                let lit_chars: Vec<char> = lit.chars().collect();
                for (i, &lc) in lit_chars.iter().enumerate() {
                    let tc_idx = text_idx + i;
                    if tc_idx >= text_chars.len() {
                        return false;
                    }
                    let tc = text_chars[tc_idx];
                    let matches = if self.case_sensitive {
                        tc == lc
                    } else {
                        tc.to_ascii_lowercase() == lc.to_ascii_lowercase()
                    };
                    if !matches {
                        return false;
                    }
                }
                self.matches_from(text, text_idx + lit_chars.len(), part_idx + 1)
            }
            PatternPart::SingleChar => {
                if text_idx >= text_chars.len() {
                    return false;
                }
                // ? doesn't match path separator
                if text_chars[text_idx] == '/' {
                    return false;
                }
                self.matches_from(text, text_idx + 1, part_idx + 1)
            }
            PatternPart::AnySequence => {
                // * matches any sequence except path separator
                for i in text_idx..=text_chars.len() {
                    if i > text_idx && text_chars[i - 1] == '/' {
                        break;
                    }
                    if self.matches_from(text, i, part_idx + 1) {
                        return true;
                    }
                }
                false
            }
            PatternPart::RecursiveAny => {
                // ** matches any sequence including path separators
                for i in text_idx..=text_chars.len() {
                    if self.matches_from(text, i, part_idx + 1) {
                        return true;
                    }
                }
                false
            }
            PatternPart::CharClass(ranges) => {
                if text_idx >= text_chars.len() {
                    return false;
                }
                let c = if self.case_sensitive {
                    text_chars[text_idx]
                } else {
                    text_chars[text_idx].to_ascii_lowercase()
                };
                let matches = ranges.iter().any(|r| {
                    let (start, end) = if self.case_sensitive {
                        (r.start, r.end)
                    } else {
                        (r.start.to_ascii_lowercase(), r.end.to_ascii_lowercase())
                    };
                    c >= start && c <= end
                });
                if matches {
                    self.matches_from(text, text_idx + 1, part_idx + 1)
                } else {
                    false
                }
            }
            PatternPart::NegatedCharClass(ranges) => {
                if text_idx >= text_chars.len() {
                    return false;
                }
                let c = if self.case_sensitive {
                    text_chars[text_idx]
                } else {
                    text_chars[text_idx].to_ascii_lowercase()
                };
                let matches = ranges.iter().any(|r| {
                    let (start, end) = if self.case_sensitive {
                        (r.start, r.end)
                    } else {
                        (r.start.to_ascii_lowercase(), r.end.to_ascii_lowercase())
                    };
                    c >= start && c <= end
                });
                if !matches {
                    self.matches_from(text, text_idx + 1, part_idx + 1)
                } else {
                    false
                }
            }
        }
    }

    /// Check if path matches pattern.
    pub fn matches_path(&self, path: &Path) -> bool {
        path.to_str().map(|s| self.matches(s)).unwrap_or(false)
    }

    /// Get the source pattern.
    pub fn as_str(&self) -> &str {
        &self.source
    }
}

impl std::fmt::Display for Pattern {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.source)
    }
}

/// Quick glob matching function.
pub fn matches(pattern: &str, text: &str) -> bool {
    Pattern::new(pattern)
        .map(|p| p.matches(text))
        .unwrap_or(false)
}

/// Quick case-insensitive glob matching.
pub fn matches_case_insensitive(pattern: &str, text: &str) -> bool {
    Pattern::new_case_insensitive(pattern)
        .map(|p| p.matches(text))
        .unwrap_or(false)
}

/// Glob pattern set.
pub struct PatternSet {
    patterns: Vec<Pattern>,
}

impl PatternSet {
    /// Create new pattern set.
    pub fn new() -> Self {
        Self {
            patterns: Vec::new(),
        }
    }

    /// Create from patterns.
    pub fn from_patterns(patterns: &[&str]) -> Result<Self> {
        let mut set = Self::new();
        for p in patterns {
            set.add(p)?;
        }
        Ok(set)
    }

    /// Add pattern.
    pub fn add(&mut self, pattern: &str) -> Result<()> {
        self.patterns.push(Pattern::new(pattern)?);
        Ok(())
    }

    /// Check if any pattern matches.
    pub fn matches(&self, text: &str) -> bool {
        self.patterns.iter().any(|p| p.matches(text))
    }

    /// Check if any pattern matches path.
    pub fn matches_path(&self, path: &Path) -> bool {
        self.patterns.iter().any(|p| p.matches_path(path))
    }

    /// Get matching patterns.
    pub fn matching_patterns(&self, text: &str) -> Vec<&Pattern> {
        self.patterns.iter().filter(|p| p.matches(text)).collect()
    }

    /// Get number of patterns.
    pub fn len(&self) -> usize {
        self.patterns.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.patterns.is_empty()
    }
}

impl Default for PatternSet {
    fn default() -> Self {
        Self::new()
    }
}

/// Gitignore-style matcher.
pub struct GitIgnore {
    patterns: Vec<(Pattern, bool)>, // (pattern, is_negated)
}

impl GitIgnore {
    /// Create new gitignore matcher.
    pub fn new() -> Self {
        Self {
            patterns: Vec::new(),
        }
    }

    /// Parse gitignore content.
    pub fn parse(content: &str) -> Result<Self> {
        let mut matcher = Self::new();

        for line in content.lines() {
            let line = line.trim();

            // Skip empty lines and comments
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            matcher.add_pattern(line)?;
        }

        Ok(matcher)
    }

    /// Add pattern.
    pub fn add_pattern(&mut self, pattern: &str) -> Result<()> {
        let (pattern, negated) = if let Some(stripped) = pattern.strip_prefix('!') {
            (stripped, true)
        } else {
            (pattern, false)
        };

        // Handle directory-only patterns
        let pattern = if pattern.ends_with('/') {
            format!("{}**", pattern)
        } else {
            pattern.to_string()
        };

        // Handle patterns that should match anywhere
        let pattern = if !pattern.contains('/') {
            format!("**/{}", pattern)
        } else if pattern.starts_with('/') {
            pattern[1..].to_string()
        } else {
            pattern
        };

        self.patterns.push((Pattern::new(&pattern)?, negated));
        Ok(())
    }

    /// Check if path should be ignored.
    pub fn is_ignored(&self, path: &str) -> bool {
        let mut ignored = false;

        for (pattern, negated) in &self.patterns {
            if pattern.matches(path) {
                ignored = !negated;
            }
        }

        ignored
    }

    /// Check if path is ignored.
    pub fn is_ignored_path(&self, path: &Path) -> bool {
        path.to_str().map(|s| self.is_ignored(s)).unwrap_or(false)
    }
}

impl Default for GitIgnore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_literal() {
        let p = Pattern::new("hello").unwrap();
        assert!(p.matches("hello"));
        assert!(!p.matches("world"));
        assert!(!p.matches("hello world"));
    }

    #[test]
    fn test_single_char() {
        let p = Pattern::new("h?llo").unwrap();
        assert!(p.matches("hello"));
        assert!(p.matches("hallo"));
        assert!(!p.matches("hllo"));
        assert!(!p.matches("heello"));
    }

    #[test]
    fn test_any_sequence() {
        let p = Pattern::new("*.txt").unwrap();
        assert!(p.matches("file.txt"));
        assert!(p.matches("hello.txt"));
        assert!(!p.matches("file.rs"));
        assert!(!p.matches("dir/file.txt")); // * doesn't match /

        let p = Pattern::new("hello*").unwrap();
        assert!(p.matches("hello"));
        assert!(p.matches("helloworld"));
        assert!(p.matches("hello.txt"));
    }

    #[test]
    fn test_recursive_any() {
        let p = Pattern::new("**/*.txt").unwrap();
        assert!(p.matches("file.txt"));
        assert!(p.matches("dir/file.txt"));
        assert!(p.matches("a/b/c/file.txt"));
        assert!(!p.matches("file.rs"));
    }

    #[test]
    fn test_char_class() {
        let p = Pattern::new("[abc]").unwrap();
        assert!(p.matches("a"));
        assert!(p.matches("b"));
        assert!(p.matches("c"));
        assert!(!p.matches("d"));

        let p = Pattern::new("[a-z]").unwrap();
        assert!(p.matches("a"));
        assert!(p.matches("z"));
        assert!(!p.matches("A"));
        assert!(!p.matches("1"));
    }

    #[test]
    fn test_negated_char_class() {
        let p = Pattern::new("[!abc]").unwrap();
        assert!(!p.matches("a"));
        assert!(!p.matches("b"));
        assert!(p.matches("d"));
        assert!(p.matches("x"));
    }

    #[test]
    fn test_case_insensitive() {
        let p = Pattern::new_case_insensitive("hello").unwrap();
        assert!(p.matches("hello"));
        assert!(p.matches("HELLO"));
        assert!(p.matches("HeLLo"));
    }

    #[test]
    fn test_pattern_set() {
        let set = PatternSet::from_patterns(&["*.txt", "*.rs", "*.md"]).unwrap();
        assert!(set.matches("file.txt"));
        assert!(set.matches("main.rs"));
        assert!(set.matches("README.md"));
        assert!(!set.matches("file.js"));
    }

    #[test]
    fn test_gitignore() {
        let gitignore = GitIgnore::parse(
            r#"
# Comment
*.log
target/
!important.log
"#,
        )
        .unwrap();

        assert!(gitignore.is_ignored("debug.log"));
        assert!(gitignore.is_ignored("target/debug/main"));
        assert!(!gitignore.is_ignored("important.log"));
        assert!(!gitignore.is_ignored("file.txt"));
    }

    #[test]
    fn test_matches_function() {
        assert!(matches("*.txt", "file.txt"));
        assert!(matches("hello?world", "hello_world"));
        assert!(!matches("*.txt", "file.rs"));
    }

    #[test]
    fn test_escape() {
        let p = Pattern::new(r"\*.txt").unwrap();
        assert!(p.matches("*.txt"));
        assert!(!p.matches("file.txt"));
    }
}
