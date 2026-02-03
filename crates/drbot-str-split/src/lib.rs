//! String splitting utilities for drbot.
//!
//! This crate provides:
//! - Advanced string splitting
//! - Tokenization
//! - Delimiter handling

use thiserror::Error;

/// Split error types.
#[derive(Error, Debug, Clone)]
pub enum SplitError {
    #[error("Empty string")]
    Empty,

    #[error("Invalid delimiter")]
    InvalidDelimiter,
}

/// Result type for split operations.
pub type Result<T> = std::result::Result<T, SplitError>;

/// Split by string.
pub fn split<'a>(s: &'a str, delimiter: &str) -> Vec<&'a str> {
    s.split(delimiter).collect()
}

/// Split by char.
pub fn split_char(s: &str, delimiter: char) -> Vec<&str> {
    s.split(delimiter).collect()
}

/// Split by whitespace.
pub fn split_whitespace(s: &str) -> Vec<&str> {
    s.split_whitespace().collect()
}

/// Split into lines.
pub fn split_lines(s: &str) -> Vec<&str> {
    s.lines().collect()
}

/// Split at first occurrence.
pub fn split_once<'a>(s: &'a str, delimiter: &str) -> Option<(&'a str, &'a str)> {
    s.split_once(delimiter)
}

/// Split at last occurrence.
pub fn rsplit_once<'a>(s: &'a str, delimiter: &str) -> Option<(&'a str, &'a str)> {
    s.rsplit_once(delimiter)
}

/// Split into n parts.
pub fn splitn<'a>(s: &'a str, n: usize, delimiter: &str) -> Vec<&'a str> {
    s.splitn(n, delimiter).collect()
}

/// Split from right into n parts.
pub fn rsplitn<'a>(s: &'a str, n: usize, delimiter: &str) -> Vec<&'a str> {
    s.rsplitn(n, delimiter).collect()
}

/// Split inclusive (keep delimiter at end of each part).
pub fn split_inclusive<'a>(s: &'a str, delimiter: char) -> Vec<&'a str> {
    s.split_inclusive(delimiter).collect()
}

/// Split keeping empty parts.
pub fn split_keep_empty<'a>(s: &'a str, delimiter: &str) -> Vec<&'a str> {
    s.split(delimiter).collect()
}

/// Split removing empty parts.
pub fn split_non_empty<'a>(s: &'a str, delimiter: &str) -> Vec<&'a str> {
    s.split(delimiter).filter(|p| !p.is_empty()).collect()
}

/// Split by any of multiple delimiters.
pub fn split_any<'a>(s: &'a str, delimiters: &[char]) -> Vec<&'a str> {
    s.split(|c| delimiters.contains(&c)).collect()
}

/// Split by predicate.
pub fn split_by<'a, F>(s: &'a str, predicate: F) -> Vec<&'a str>
where
    F: FnMut(char) -> bool,
{
    s.split(predicate).collect()
}

/// Split into fixed-size parts.
pub fn split_chunks(s: &str, chunk_size: usize) -> Vec<&str> {
    if chunk_size == 0 {
        return vec![];
    }

    let mut chunks = Vec::new();
    let mut start = 0;

    while start < s.len() {
        let end = (start + chunk_size).min(s.len());
        // Ensure we don't split in the middle of a UTF-8 character
        let end = s[start..]
            .char_indices()
            .take_while(|(i, _)| *i < chunk_size)
            .last()
            .map(|(i, c)| start + i + c.len_utf8())
            .unwrap_or(s.len());
        chunks.push(&s[start..end]);
        start = end;
    }

    chunks
}

/// Split into words (alphanumeric sequences).
pub fn split_words(s: &str) -> Vec<&str> {
    s.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect()
}

/// Split by regex-like pattern (simplified: just char classes).
pub fn split_pattern<'a>(s: &'a str, pattern: &str) -> Vec<&'a str> {
    match pattern {
        "\\s" | "\\s+" => s.split_whitespace().collect(),
        "\\d" => s.split(|c: char| c.is_ascii_digit()).collect(),
        "\\w" => s
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .collect(),
        _ => s.split(pattern).collect(),
    }
}

/// Tokenize string by multiple delimiter types.
#[derive(Debug, Clone)]
pub struct Tokenizer<'a> {
    source: &'a str,
    position: usize,
    delimiters: Vec<char>,
    skip_empty: bool,
}

impl<'a> Tokenizer<'a> {
    /// Create new tokenizer.
    pub fn new(source: &'a str) -> Self {
        Self {
            source,
            position: 0,
            delimiters: vec![' ', '\t', '\n'],
            skip_empty: true,
        }
    }

    /// Set delimiters.
    pub fn with_delimiters(mut self, delimiters: Vec<char>) -> Self {
        self.delimiters = delimiters;
        self
    }

    /// Set skip empty.
    pub fn skip_empty(mut self, skip: bool) -> Self {
        self.skip_empty = skip;
        self
    }

    /// Get all tokens.
    pub fn tokenize(self) -> Vec<&'a str> {
        if self.skip_empty {
            self.source
                .split(|c| self.delimiters.contains(&c))
                .filter(|t| !t.is_empty())
                .collect()
        } else {
            self.source
                .split(|c| self.delimiters.contains(&c))
                .collect()
        }
    }
}

/// Split into key-value pair.
pub fn split_kv<'a>(s: &'a str, sep: char) -> Option<(&'a str, &'a str)> {
    s.split_once(sep).map(|(k, v)| (k.trim(), v.trim()))
}

/// Split into multiple key-value pairs.
pub fn split_kvs<'a>(s: &'a str, pair_sep: &str, kv_sep: char) -> Vec<(&'a str, &'a str)> {
    s.split(pair_sep)
        .filter_map(|pair| split_kv(pair, kv_sep))
        .collect()
}

/// Extract quoted strings.
pub fn extract_quoted(s: &str, quote: char) -> Vec<&str> {
    let mut results = Vec::new();
    let mut in_quote = false;
    let mut start = 0;

    for (i, c) in s.char_indices() {
        if c == quote {
            if in_quote {
                results.push(&s[start..i]);
                in_quote = false;
            } else {
                start = i + c.len_utf8();
                in_quote = true;
            }
        }
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split() {
        assert_eq!(split("a,b,c", ","), vec!["a", "b", "c"]);
    }

    #[test]
    fn test_split_once() {
        assert_eq!(split_once("key=value", "="), Some(("key", "value")));
        assert_eq!(split_once("a=b=c", "="), Some(("a", "b=c")));
    }

    #[test]
    fn test_split_non_empty() {
        assert_eq!(split_non_empty("a,,b,,c", ","), vec!["a", "b", "c"]);
    }

    #[test]
    fn test_split_any() {
        assert_eq!(split_any("a,b;c", &[',', ';']), vec!["a", "b", "c"]);
    }

    #[test]
    fn test_split_words() {
        assert_eq!(
            split_words("hello, world! how are you?"),
            vec!["hello", "world", "how", "are", "you"]
        );
    }

    #[test]
    fn test_split_kv() {
        assert_eq!(split_kv("key = value", '='), Some(("key", "value")));
    }

    #[test]
    fn test_tokenizer() {
        let tokens = Tokenizer::new("hello world\tfoo").tokenize();
        assert_eq!(tokens, vec!["hello", "world", "foo"]);
    }

    #[test]
    fn test_extract_quoted() {
        assert_eq!(
            extract_quoted("say 'hello' and 'world'", '\''),
            vec!["hello", "world"]
        );
    }
}
