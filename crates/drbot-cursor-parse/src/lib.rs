//! Cursor-based parsing utilities for drbot.
//!
//! This crate provides:
//! - Cursor abstraction over token streams
//! - Parsing utilities
//! - Error recovery

use thiserror::Error;

/// Parse error types.
#[derive(Error, Debug, Clone)]
pub enum ParseError {
    #[error("Unexpected token: expected {expected}, found {found}")]
    UnexpectedToken { expected: String, found: String },

    #[error("Unexpected end of input")]
    UnexpectedEof,

    #[error("Parse error: {0}")]
    Error(String),
}

/// Result type for parse operations.
pub type Result<T> = std::result::Result<T, ParseError>;

/// Token cursor for parsing.
pub struct Cursor<'a, T> {
    tokens: &'a [T],
    position: usize,
}

impl<'a, T> Cursor<'a, T> {
    /// Create new cursor.
    pub fn new(tokens: &'a [T]) -> Self {
        Self {
            tokens,
            position: 0,
        }
    }

    /// Get current position.
    pub fn position(&self) -> usize {
        self.position
    }

    /// Check if at end.
    pub fn is_eof(&self) -> bool {
        self.position >= self.tokens.len()
    }

    /// Get remaining tokens.
    pub fn remaining(&self) -> usize {
        self.tokens.len().saturating_sub(self.position)
    }

    /// Peek current token.
    pub fn peek(&self) -> Option<&T> {
        self.tokens.get(self.position)
    }

    /// Peek nth token ahead.
    pub fn peek_n(&self, n: usize) -> Option<&T> {
        self.tokens.get(self.position + n)
    }

    /// Advance and return current token.
    pub fn advance(&mut self) -> Option<&T> {
        let token = self.tokens.get(self.position)?;
        self.position += 1;
        Some(token)
    }

    /// Advance by n tokens.
    pub fn advance_n(&mut self, n: usize) {
        self.position = (self.position + n).min(self.tokens.len());
    }

    /// Skip tokens while predicate is true.
    pub fn skip_while<F: Fn(&T) -> bool>(&mut self, predicate: F) {
        while let Some(token) = self.peek() {
            if predicate(token) {
                self.advance();
            } else {
                break;
            }
        }
    }

    /// Take tokens while predicate is true.
    pub fn take_while<F: Fn(&T) -> bool>(&mut self, predicate: F) -> &[T] {
        let start = self.position;
        self.skip_while(predicate);
        &self.tokens[start..self.position]
    }

    /// Fork cursor for lookahead.
    pub fn fork(&self) -> Self {
        Self {
            tokens: self.tokens,
            position: self.position,
        }
    }

    /// Reset to position of another cursor.
    pub fn reset_to(&mut self, other: &Self) {
        self.position = other.position;
    }
}

impl<'a, T: PartialEq> Cursor<'a, T> {
    /// Expect specific token.
    pub fn expect(&mut self, expected: &T) -> Result<&T>
    where
        T: std::fmt::Debug,
    {
        match self.peek() {
            Some(t) if t == expected => Ok(self.advance().unwrap()),
            Some(t) => Err(ParseError::UnexpectedToken {
                expected: format!("{:?}", expected),
                found: format!("{:?}", t),
            }),
            None => Err(ParseError::UnexpectedEof),
        }
    }

    /// Check if current token matches.
    pub fn check(&self, expected: &T) -> bool {
        self.peek() == Some(expected)
    }

    /// Match and consume if matches.
    pub fn matches(&mut self, expected: &T) -> bool {
        if self.check(expected) {
            self.advance();
            true
        } else {
            false
        }
    }
}

/// Checkpoint for backtracking.
pub struct Checkpoint {
    position: usize,
}

impl<'a, T> Cursor<'a, T> {
    /// Create checkpoint.
    pub fn checkpoint(&self) -> Checkpoint {
        Checkpoint {
            position: self.position,
        }
    }

    /// Restore to checkpoint.
    pub fn restore(&mut self, checkpoint: &Checkpoint) {
        self.position = checkpoint.position;
    }
}

/// Parse result with backtracking.
pub fn try_parse<T, F, R>(cursor: &mut Cursor<'_, T>, f: F) -> Option<R>
where
    F: FnOnce(&mut Cursor<'_, T>) -> Result<R>,
{
    let checkpoint = cursor.checkpoint();
    match f(cursor) {
        Ok(result) => Some(result),
        Err(_) => {
            cursor.restore(&checkpoint);
            None
        }
    }
}

/// Parse with alternatives.
pub fn alternatives<T, R>(
    cursor: &mut Cursor<'_, T>,
    parsers: &[fn(&mut Cursor<'_, T>) -> Result<R>],
) -> Result<R> {
    for parser in parsers {
        if let Some(result) = try_parse(cursor, parser) {
            return Ok(result);
        }
    }
    Err(ParseError::Error("No alternative matched".into()))
}

/// Parse separated list.
pub fn separated<T, R, F, S>(
    cursor: &mut Cursor<'_, T>,
    item_parser: F,
    separator: &T,
) -> Result<Vec<R>>
where
    T: PartialEq + std::fmt::Debug,
    F: Fn(&mut Cursor<'_, T>) -> Result<R>,
{
    let mut items = Vec::new();

    // Parse first item
    items.push(item_parser(cursor)?);

    // Parse remaining items
    while cursor.matches(separator) {
        items.push(item_parser(cursor)?);
    }

    Ok(items)
}

/// Parse optional.
pub fn optional<T, R, F>(cursor: &mut Cursor<'_, T>, parser: F) -> Option<R>
where
    F: FnOnce(&mut Cursor<'_, T>) -> Result<R>,
{
    try_parse(cursor, parser)
}

/// Parse many (zero or more).
pub fn many<T, R, F>(cursor: &mut Cursor<'_, T>, parser: F) -> Vec<R>
where
    F: Fn(&mut Cursor<'_, T>) -> Result<R>,
{
    let mut items = Vec::new();
    while let Some(item) = try_parse(cursor, &parser) {
        items.push(item);
    }
    items
}

/// Parse many1 (one or more).
pub fn many1<T, R, F>(cursor: &mut Cursor<'_, T>, parser: F) -> Result<Vec<R>>
where
    F: Fn(&mut Cursor<'_, T>) -> Result<R>,
{
    let first = parser(cursor)?;
    let mut items = vec![first];
    while let Some(item) = try_parse(cursor, &parser) {
        items.push(item);
    }
    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cursor_basic() {
        let tokens = vec![1, 2, 3, 4, 5];
        let mut cursor = Cursor::new(&tokens);

        assert_eq!(cursor.peek(), Some(&1));
        assert_eq!(cursor.advance(), Some(&1));
        assert_eq!(cursor.peek(), Some(&2));
        assert_eq!(cursor.remaining(), 4);
    }

    #[test]
    fn test_cursor_expect() {
        let tokens = vec![1, 2, 3];
        let mut cursor = Cursor::new(&tokens);

        assert!(cursor.expect(&1).is_ok());
        assert!(cursor.expect(&3).is_err());
    }

    #[test]
    fn test_cursor_matches() {
        let tokens = vec![1, 2, 3];
        let mut cursor = Cursor::new(&tokens);

        assert!(cursor.matches(&1));
        assert!(!cursor.matches(&3));
        assert_eq!(cursor.position(), 1);
    }

    #[test]
    fn test_checkpoint() {
        let tokens = vec![1, 2, 3, 4, 5];
        let mut cursor = Cursor::new(&tokens);

        cursor.advance_n(3);
        let checkpoint = cursor.checkpoint();
        cursor.advance_n(2);

        assert_eq!(cursor.position(), 5);
        cursor.restore(&checkpoint);
        assert_eq!(cursor.position(), 3);
    }

    #[test]
    fn test_try_parse() {
        let tokens = vec![1, 2, 3];
        let mut cursor = Cursor::new(&tokens);

        let result: Option<i32> = try_parse(&mut cursor, |c| {
            c.expect(&1)?;
            c.expect(&99)?; // Will fail
            Ok(42)
        });

        assert!(result.is_none());
        assert_eq!(cursor.position(), 0); // Backtracked
    }

    #[test]
    fn test_many() {
        let tokens = vec![1, 1, 1, 2, 3];
        let mut cursor = Cursor::new(&tokens);

        let items = many(&mut cursor, |c| {
            c.expect(&1)?;
            Ok(1)
        });

        assert_eq!(items.len(), 3);
        assert_eq!(cursor.position(), 3);
    }
}
