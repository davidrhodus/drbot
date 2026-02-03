//! String joining utilities for drbot.
//!
//! This crate provides:
//! - String joining operations
//! - Concatenation helpers
//! - Formatting joins

use std::fmt::Display;
use thiserror::Error;

/// Join error types.
#[derive(Error, Debug, Clone)]
pub enum JoinError {
    #[error("Empty items")]
    Empty,
}

/// Result type for join operations.
pub type Result<T> = std::result::Result<T, JoinError>;

/// Join strings with separator.
pub fn join(sep: &str, items: &[&str]) -> String {
    items.join(sep)
}

/// Join owned strings.
pub fn join_owned(sep: &str, items: Vec<String>) -> String {
    items.join(sep)
}

/// Join with separator.
pub fn join_iter<I, T>(sep: &str, iter: I) -> String
where
    I: IntoIterator<Item = T>,
    T: Display,
{
    iter.into_iter()
        .map(|x| x.to_string())
        .collect::<Vec<_>>()
        .join(sep)
}

/// Join with comma.
pub fn join_comma<I, T>(iter: I) -> String
where
    I: IntoIterator<Item = T>,
    T: Display,
{
    join_iter(", ", iter)
}

/// Join with space.
pub fn join_space<I, T>(iter: I) -> String
where
    I: IntoIterator<Item = T>,
    T: Display,
{
    join_iter(" ", iter)
}

/// Join with newline.
pub fn join_lines<I, T>(iter: I) -> String
where
    I: IntoIterator<Item = T>,
    T: Display,
{
    join_iter("\n", iter)
}

/// Join with custom formatting.
pub fn join_format<I, T, F>(sep: &str, iter: I, formatter: F) -> String
where
    I: IntoIterator<Item = T>,
    F: Fn(T) -> String,
{
    iter.into_iter()
        .map(formatter)
        .collect::<Vec<_>>()
        .join(sep)
}

/// Concat strings (no separator).
pub fn concat(items: &[&str]) -> String {
    items.concat()
}

/// Concat with function.
pub fn concat_map<I, T, F>(iter: I, mapper: F) -> String
where
    I: IntoIterator<Item = T>,
    F: Fn(T) -> String,
{
    iter.into_iter().map(mapper).collect()
}

/// Join with last separator different (e.g., "a, b, and c").
pub fn join_with_last(sep: &str, last_sep: &str, items: &[&str]) -> String {
    match items.len() {
        0 => String::new(),
        1 => items[0].to_string(),
        2 => format!("{}{}{}", items[0], last_sep, items[1]),
        _ => {
            let (last, rest) = items.split_last().unwrap();
            format!("{}{}{}", rest.join(sep), last_sep, last)
        }
    }
}

/// Join with "and" (Oxford comma style).
pub fn join_and(items: &[&str]) -> String {
    join_with_last(", ", ", and ", items)
}

/// Join with "or".
pub fn join_or(items: &[&str]) -> String {
    join_with_last(", ", ", or ", items)
}

/// Join wrapping each item.
pub fn join_wrapped<I, T>(sep: &str, prefix: &str, suffix: &str, iter: I) -> String
where
    I: IntoIterator<Item = T>,
    T: Display,
{
    iter.into_iter()
        .map(|x| format!("{}{}{}", prefix, x, suffix))
        .collect::<Vec<_>>()
        .join(sep)
}

/// Join as quoted strings.
pub fn join_quoted<I, T>(sep: &str, iter: I) -> String
where
    I: IntoIterator<Item = T>,
    T: Display,
{
    join_wrapped(sep, "\"", "\"", iter)
}

/// Join as list items (numbered).
pub fn join_numbered<I, T>(iter: I) -> String
where
    I: IntoIterator<Item = T>,
    T: Display,
{
    iter.into_iter()
        .enumerate()
        .map(|(i, x)| format!("{}. {}", i + 1, x))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Join as bullet list.
pub fn join_bullets<I, T>(bullet: &str, iter: I) -> String
where
    I: IntoIterator<Item = T>,
    T: Display,
{
    iter.into_iter()
        .map(|x| format!("{} {}", bullet, x))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Join non-empty items only.
pub fn join_non_empty(sep: &str, items: &[&str]) -> String {
    items
        .iter()
        .filter(|s| !s.is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join(sep)
}

/// Join with transformation.
pub fn join_transform<I, T, F>(sep: &str, iter: I, transform: F) -> String
where
    I: IntoIterator<Item = T>,
    F: Fn(T) -> String,
{
    iter.into_iter()
        .map(transform)
        .collect::<Vec<_>>()
        .join(sep)
}

/// Joiner builder.
pub struct Joiner {
    separator: String,
    prefix: String,
    suffix: String,
    last_separator: Option<String>,
    skip_empty: bool,
}

impl Joiner {
    /// Create new joiner.
    pub fn new(separator: &str) -> Self {
        Self {
            separator: separator.to_string(),
            prefix: String::new(),
            suffix: String::new(),
            last_separator: None,
            skip_empty: false,
        }
    }

    /// Set prefix.
    pub fn prefix(mut self, prefix: &str) -> Self {
        self.prefix = prefix.to_string();
        self
    }

    /// Set suffix.
    pub fn suffix(mut self, suffix: &str) -> Self {
        self.suffix = suffix.to_string();
        self
    }

    /// Set last separator.
    pub fn last_separator(mut self, sep: &str) -> Self {
        self.last_separator = Some(sep.to_string());
        self
    }

    /// Skip empty items.
    pub fn skip_empty(mut self) -> Self {
        self.skip_empty = true;
        self
    }

    /// Join items.
    pub fn join<I, T>(&self, iter: I) -> String
    where
        I: IntoIterator<Item = T>,
        T: Display,
    {
        let items: Vec<String> = if self.skip_empty {
            iter.into_iter()
                .map(|x| x.to_string())
                .filter(|s| !s.is_empty())
                .collect()
        } else {
            iter.into_iter().map(|x| x.to_string()).collect()
        };

        let joined = if let Some(ref last_sep) = self.last_separator {
            match items.len() {
                0 => String::new(),
                1 => items[0].clone(),
                _ => {
                    let (last, rest) = items.split_last().unwrap();
                    format!("{}{}{}", rest.join(&self.separator), last_sep, last)
                }
            }
        } else {
            items.join(&self.separator)
        };

        format!("{}{}{}", self.prefix, joined, self.suffix)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_join() {
        assert_eq!(join(", ", &["a", "b", "c"]), "a, b, c");
    }

    #[test]
    fn test_join_iter() {
        assert_eq!(join_iter(", ", vec![1, 2, 3]), "1, 2, 3");
    }

    #[test]
    fn test_join_and() {
        assert_eq!(join_and(&["a", "b", "c"]), "a, b, and c");
        assert_eq!(join_and(&["a", "b"]), "a, and b");
        assert_eq!(join_and(&["a"]), "a");
    }

    #[test]
    fn test_join_quoted() {
        assert_eq!(join_quoted(", ", vec!["a", "b"]), "\"a\", \"b\"");
    }

    #[test]
    fn test_join_numbered() {
        assert_eq!(join_numbered(vec!["a", "b"]), "1. a\n2. b");
    }

    #[test]
    fn test_joiner() {
        let result = Joiner::new(", ")
            .prefix("[")
            .suffix("]")
            .join(vec!["a", "b", "c"]);
        assert_eq!(result, "[a, b, c]");
    }

    #[test]
    fn test_joiner_with_last() {
        let result = Joiner::new(", ")
            .last_separator(" and ")
            .join(vec!["a", "b", "c"]);
        assert_eq!(result, "a, b and c");
    }
}
