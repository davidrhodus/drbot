//! Text scanning utilities for drbot.
//!
//! This crate provides:
//! - Character-level scanning
//! - Position tracking
//! - Lookahead support

use thiserror::Error;

/// Scanner error types.
#[derive(Error, Debug, Clone)]
pub enum ScanError {
    #[error("Unexpected character at position {0}: {1}")]
    UnexpectedChar(usize, char),

    #[error("Unexpected end of input")]
    UnexpectedEof,

    #[error("Expected '{expected}', found '{found}'")]
    Expected { expected: String, found: String },
}

/// Result type for scanner operations.
pub type Result<T> = std::result::Result<T, ScanError>;

/// Position in source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Position {
    pub offset: usize,
    pub line: usize,
    pub column: usize,
}

impl Position {
    /// Create new position.
    pub fn new(offset: usize, line: usize, column: usize) -> Self {
        Self {
            offset,
            line,
            column,
        }
    }

    /// Start position.
    pub fn start() -> Self {
        Self {
            offset: 0,
            line: 1,
            column: 1,
        }
    }
}

/// Text scanner.
pub struct Scanner<'a> {
    input: &'a str,
    position: Position,
    mark: Option<Position>,
}

impl<'a> Scanner<'a> {
    /// Create new scanner.
    pub fn new(input: &'a str) -> Self {
        Self {
            input,
            position: Position::start(),
            mark: None,
        }
    }

    /// Get current position.
    pub fn position(&self) -> Position {
        self.position
    }

    /// Get current offset.
    pub fn offset(&self) -> usize {
        self.position.offset
    }

    /// Check if at end.
    pub fn is_eof(&self) -> bool {
        self.position.offset >= self.input.len()
    }

    /// Get remaining input.
    pub fn remaining(&self) -> &str {
        &self.input[self.position.offset..]
    }

    /// Peek current character.
    pub fn peek(&self) -> Option<char> {
        self.input[self.position.offset..].chars().next()
    }

    /// Peek nth character ahead.
    pub fn peek_n(&self, n: usize) -> Option<char> {
        self.input[self.position.offset..].chars().nth(n)
    }

    /// Peek ahead string.
    pub fn peek_str(&self, len: usize) -> &str {
        let end = (self.position.offset + len).min(self.input.len());
        &self.input[self.position.offset..end]
    }

    /// Advance and return current character.
    pub fn advance(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.position.offset += c.len_utf8();
        if c == '\n' {
            self.position.line += 1;
            self.position.column = 1;
        } else {
            self.position.column += 1;
        }
        Some(c)
    }

    /// Advance n characters.
    pub fn advance_n(&mut self, n: usize) {
        for _ in 0..n {
            if self.advance().is_none() {
                break;
            }
        }
    }

    /// Set mark at current position.
    pub fn mark(&mut self) {
        self.mark = Some(self.position);
    }

    /// Reset to mark.
    pub fn reset(&mut self) {
        if let Some(mark) = self.mark {
            self.position = mark;
        }
    }

    /// Get text from mark to current position.
    pub fn marked_text(&self) -> &str {
        match self.mark {
            Some(mark) => &self.input[mark.offset..self.position.offset],
            None => "",
        }
    }

    /// Expect specific character.
    pub fn expect(&mut self, expected: char) -> Result<char> {
        match self.peek() {
            Some(c) if c == expected => {
                self.advance();
                Ok(c)
            }
            Some(c) => Err(ScanError::Expected {
                expected: expected.to_string(),
                found: c.to_string(),
            }),
            None => Err(ScanError::UnexpectedEof),
        }
    }

    /// Expect one of characters.
    pub fn expect_one_of(&mut self, chars: &[char]) -> Result<char> {
        match self.peek() {
            Some(c) if chars.contains(&c) => {
                self.advance();
                Ok(c)
            }
            Some(c) => Err(ScanError::Expected {
                expected: format!("one of {:?}", chars),
                found: c.to_string(),
            }),
            None => Err(ScanError::UnexpectedEof),
        }
    }

    /// Skip while predicate is true.
    pub fn skip_while<F: Fn(char) -> bool>(&mut self, predicate: F) {
        while let Some(c) = self.peek() {
            if predicate(c) {
                self.advance();
            } else {
                break;
            }
        }
    }

    /// Skip whitespace.
    pub fn skip_whitespace(&mut self) {
        self.skip_while(|c| c.is_whitespace());
    }

    /// Skip whitespace except newlines.
    pub fn skip_horizontal_whitespace(&mut self) {
        self.skip_while(|c| c.is_whitespace() && c != '\n');
    }

    /// Take while predicate is true.
    pub fn take_while<F: Fn(char) -> bool>(&mut self, predicate: F) -> &str {
        let start = self.position.offset;
        self.skip_while(predicate);
        &self.input[start..self.position.offset]
    }

    /// Take until predicate is true.
    pub fn take_until<F: Fn(char) -> bool>(&mut self, predicate: F) -> &str {
        self.take_while(|c| !predicate(c))
    }

    /// Try to match string.
    pub fn match_str(&mut self, s: &str) -> bool {
        if self.remaining().starts_with(s) {
            self.advance_n(s.chars().count());
            true
        } else {
            false
        }
    }

    /// Check if remaining starts with.
    pub fn starts_with(&self, s: &str) -> bool {
        self.remaining().starts_with(s)
    }

    /// Read identifier.
    pub fn read_identifier(&mut self) -> Option<&str> {
        if !self
            .peek()
            .map(|c| c.is_alphabetic() || c == '_')
            .unwrap_or(false)
        {
            return None;
        }
        Some(self.take_while(|c| c.is_alphanumeric() || c == '_'))
    }

    /// Read integer.
    pub fn read_integer(&mut self) -> Option<i64> {
        let start = self.position.offset;
        if self.peek() == Some('-') {
            self.advance();
        }
        if !self.peek().map(|c| c.is_ascii_digit()).unwrap_or(false) {
            self.position.offset = start;
            return None;
        }
        self.skip_while(|c| c.is_ascii_digit());
        self.input[start..self.position.offset].parse().ok()
    }

    /// Read quoted string.
    pub fn read_quoted_string(&mut self, quote: char) -> Result<String> {
        self.expect(quote)?;
        let mut result = String::new();
        loop {
            match self.peek() {
                None => return Err(ScanError::UnexpectedEof),
                Some(c) if c == quote => {
                    self.advance();
                    break;
                }
                Some('\\') => {
                    self.advance();
                    if let Some(escaped) = self.advance() {
                        match escaped {
                            'n' => result.push('\n'),
                            't' => result.push('\t'),
                            'r' => result.push('\r'),
                            '\\' => result.push('\\'),
                            c => result.push(c),
                        }
                    }
                }
                Some(c) => {
                    result.push(c);
                    self.advance();
                }
            }
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_scanning() {
        let mut scanner = Scanner::new("hello world");

        assert_eq!(scanner.peek(), Some('h'));
        assert_eq!(scanner.advance(), Some('h'));
        assert_eq!(scanner.advance(), Some('e'));
        assert_eq!(scanner.position().column, 3);
    }

    #[test]
    fn test_take_while() {
        let mut scanner = Scanner::new("hello123 world");

        let word = scanner.take_while(|c| c.is_alphanumeric());
        assert_eq!(word, "hello123");
    }

    #[test]
    fn test_match_str() {
        let mut scanner = Scanner::new("function main()");

        assert!(scanner.match_str("function"));
        assert_eq!(scanner.peek(), Some(' '));
    }

    #[test]
    fn test_read_identifier() {
        let mut scanner = Scanner::new("my_var = 123");

        let id = scanner.read_identifier();
        assert_eq!(id, Some("my_var"));
    }

    #[test]
    fn test_read_integer() {
        let mut scanner = Scanner::new("-42");

        let num = scanner.read_integer();
        assert_eq!(num, Some(-42));
    }

    #[test]
    fn test_mark_reset() {
        let mut scanner = Scanner::new("hello world");

        scanner.advance_n(5);
        scanner.mark();
        scanner.advance_n(6);
        scanner.reset();

        assert_eq!(scanner.position().offset, 5);
    }

    #[test]
    fn test_line_tracking() {
        let mut scanner = Scanner::new("line1\nline2\nline3");

        scanner.skip_while(|c| c != '\n');
        scanner.advance(); // newline

        assert_eq!(scanner.position().line, 2);
        assert_eq!(scanner.position().column, 1);
    }
}
