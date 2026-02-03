//! Lexer utilities for drbot.
//!
//! This crate provides:
//! - Tokenization primitives
//! - Lexer state machine
//! - Token streams
//! - Common token types

use std::fmt;
use thiserror::Error;

/// Lexer error types.
#[derive(Error, Debug, Clone)]
pub enum LexerError {
    #[error("Unexpected character '{0}' at position {1}")]
    UnexpectedChar(char, usize),

    #[error("Unterminated string at position {0}")]
    UnterminatedString(usize),

    #[error("Invalid escape sequence at position {0}")]
    InvalidEscape(usize),

    #[error("Unexpected end of input")]
    UnexpectedEof,
}

/// Result type for lexer operations.
pub type Result<T> = std::result::Result<T, LexerError>;

/// Token location.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Location {
    /// Start offset.
    pub start: usize,
    /// End offset.
    pub end: usize,
    /// Line number.
    pub line: usize,
    /// Column number.
    pub column: usize,
}

impl Location {
    /// Create new location.
    pub fn new(start: usize, end: usize, line: usize, column: usize) -> Self {
        Self {
            start,
            end,
            line,
            column,
        }
    }

    /// Get length.
    pub fn len(&self) -> usize {
        self.end - self.start
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }
}

impl fmt::Display for Location {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.line, self.column)
    }
}

/// Generic token.
#[derive(Debug, Clone, PartialEq)]
pub struct Token<T> {
    /// Token kind.
    pub kind: T,
    /// Token text.
    pub text: String,
    /// Token location.
    pub location: Location,
}

impl<T> Token<T> {
    /// Create new token.
    pub fn new(kind: T, text: impl Into<String>, location: Location) -> Self {
        Self {
            kind,
            text: text.into(),
            location,
        }
    }

    /// Map token kind.
    pub fn map<U, F>(self, f: F) -> Token<U>
    where
        F: FnOnce(T) -> U,
    {
        Token {
            kind: f(self.kind),
            text: self.text,
            location: self.location,
        }
    }
}

/// Simple token kinds for common use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimpleToken {
    /// Identifier.
    Ident,
    /// Integer literal.
    Integer,
    /// Float literal.
    Float,
    /// String literal.
    String,
    /// Operator.
    Operator,
    /// Punctuation.
    Punct,
    /// Keyword.
    Keyword,
    /// Comment.
    Comment,
    /// Whitespace.
    Whitespace,
    /// Newline.
    Newline,
    /// End of file.
    Eof,
    /// Unknown.
    Unknown,
}

/// Lexer state.
#[derive(Debug, Clone)]
pub struct Lexer<'a> {
    /// Input source.
    source: &'a str,
    /// Current position.
    position: usize,
    /// Current line.
    line: usize,
    /// Current column.
    column: usize,
    /// Line start position.
    line_start: usize,
}

impl<'a> Lexer<'a> {
    /// Create new lexer.
    pub fn new(source: &'a str) -> Self {
        Self {
            source,
            position: 0,
            line: 1,
            column: 1,
            line_start: 0,
        }
    }

    /// Get remaining input.
    pub fn remaining(&self) -> &'a str {
        &self.source[self.position..]
    }

    /// Check if at end.
    pub fn is_eof(&self) -> bool {
        self.position >= self.source.len()
    }

    /// Peek current character.
    pub fn peek(&self) -> Option<char> {
        self.remaining().chars().next()
    }

    /// Peek nth character.
    pub fn peek_n(&self, n: usize) -> Option<char> {
        self.remaining().chars().nth(n)
    }

    /// Check if remaining input starts with string.
    pub fn starts_with(&self, s: &str) -> bool {
        self.remaining().starts_with(s)
    }

    /// Advance by one character.
    pub fn advance(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.position += c.len_utf8();
        if c == '\n' {
            self.line += 1;
            self.column = 1;
            self.line_start = self.position;
        } else {
            self.column += 1;
        }
        Some(c)
    }

    /// Advance while predicate holds.
    pub fn advance_while<F>(&mut self, predicate: F) -> &'a str
    where
        F: Fn(char) -> bool,
    {
        let start = self.position;
        while let Some(c) = self.peek() {
            if !predicate(c) {
                break;
            }
            self.advance();
        }
        &self.source[start..self.position]
    }

    /// Skip whitespace (not newlines).
    pub fn skip_whitespace(&mut self) {
        self.advance_while(|c| c.is_whitespace() && c != '\n');
    }

    /// Skip line (including newline).
    pub fn skip_line(&mut self) {
        self.advance_while(|c| c != '\n');
        self.advance(); // Skip newline if present
    }

    /// Get current location.
    pub fn location(&self) -> Location {
        Location::new(self.position, self.position, self.line, self.column)
    }

    /// Create location from start to current.
    pub fn location_from(&self, start: usize, start_line: usize, start_column: usize) -> Location {
        Location::new(start, self.position, start_line, start_column)
    }

    /// Consume and return token.
    pub fn token<T>(
        &mut self,
        kind: T,
        start: usize,
        start_line: usize,
        start_column: usize,
    ) -> Token<T> {
        let text = &self.source[start..self.position];
        Token::new(
            kind,
            text,
            self.location_from(start, start_line, start_column),
        )
    }

    /// Lex identifier.
    pub fn lex_identifier(&mut self) -> Token<SimpleToken> {
        let start = self.position;
        let line = self.line;
        let column = self.column;

        self.advance_while(|c| c.is_alphanumeric() || c == '_');
        self.token(SimpleToken::Ident, start, line, column)
    }

    /// Lex number.
    pub fn lex_number(&mut self) -> Token<SimpleToken> {
        let start = self.position;
        let line = self.line;
        let column = self.column;

        self.advance_while(|c| c.is_ascii_digit());

        let kind =
            if self.peek() == Some('.') && self.peek_n(1).map_or(false, |c| c.is_ascii_digit()) {
                self.advance(); // Skip '.'
                self.advance_while(|c| c.is_ascii_digit());
                SimpleToken::Float
            } else {
                SimpleToken::Integer
            };

        self.token(kind, start, line, column)
    }

    /// Lex string.
    pub fn lex_string(&mut self, quote: char) -> Result<Token<SimpleToken>> {
        let start = self.position;
        let line = self.line;
        let column = self.column;

        self.advance(); // Skip opening quote

        loop {
            match self.peek() {
                None => return Err(LexerError::UnterminatedString(start)),
                Some(c) if c == quote => {
                    self.advance();
                    break;
                }
                Some('\\') => {
                    self.advance();
                    if self.advance().is_none() {
                        return Err(LexerError::InvalidEscape(self.position));
                    }
                }
                Some(_) => {
                    self.advance();
                }
            }
        }

        Ok(self.token(SimpleToken::String, start, line, column))
    }

    /// Lex comment.
    pub fn lex_line_comment(&mut self) -> Token<SimpleToken> {
        let start = self.position;
        let line = self.line;
        let column = self.column;

        self.advance_while(|c| c != '\n');
        self.token(SimpleToken::Comment, start, line, column)
    }

    /// Lex block comment.
    pub fn lex_block_comment(&mut self, end: &str) -> Result<Token<SimpleToken>> {
        let start = self.position;
        let line = self.line;
        let column = self.column;

        while !self.is_eof() {
            if self.starts_with(end) {
                for _ in end.chars() {
                    self.advance();
                }
                return Ok(self.token(SimpleToken::Comment, start, line, column));
            }
            self.advance();
        }

        Err(LexerError::UnexpectedEof)
    }
}

/// Token stream.
#[derive(Debug)]
pub struct TokenStream<T> {
    tokens: Vec<Token<T>>,
    position: usize,
}

impl<T: Clone> TokenStream<T> {
    /// Create from tokens.
    pub fn new(tokens: Vec<Token<T>>) -> Self {
        Self {
            tokens,
            position: 0,
        }
    }

    /// Check if at end.
    pub fn is_eof(&self) -> bool {
        self.position >= self.tokens.len()
    }

    /// Peek current token.
    pub fn peek(&self) -> Option<&Token<T>> {
        self.tokens.get(self.position)
    }

    /// Peek nth token.
    pub fn peek_n(&self, n: usize) -> Option<&Token<T>> {
        self.tokens.get(self.position + n)
    }

    /// Advance and return token.
    pub fn advance(&mut self) -> Option<Token<T>> {
        if self.is_eof() {
            None
        } else {
            let token = self.tokens[self.position].clone();
            self.position += 1;
            Some(token)
        }
    }

    /// Get current position.
    pub fn position(&self) -> usize {
        self.position
    }

    /// Reset to position.
    pub fn reset(&mut self, position: usize) {
        self.position = position;
    }

    /// Get remaining tokens.
    pub fn remaining(&self) -> &[Token<T>] {
        &self.tokens[self.position..]
    }
}

impl<T: Clone> Iterator for TokenStream<T> {
    type Item = Token<T>;

    fn next(&mut self) -> Option<Self::Item> {
        self.advance()
    }
}

/// Simple lexer for common languages.
pub fn lex_simple(source: &str) -> Result<Vec<Token<SimpleToken>>> {
    let mut lexer = Lexer::new(source);
    let mut tokens = Vec::new();

    while !lexer.is_eof() {
        let c = lexer.peek().unwrap();

        let token = match c {
            // Whitespace
            ' ' | '\t' | '\r' => {
                let start = lexer.position;
                let line = lexer.line;
                let column = lexer.column;
                lexer.advance_while(|c| c == ' ' || c == '\t' || c == '\r');
                lexer.token(SimpleToken::Whitespace, start, line, column)
            }

            // Newline
            '\n' => {
                let start = lexer.position;
                let line = lexer.line;
                let column = lexer.column;
                lexer.advance();
                lexer.token(SimpleToken::Newline, start, line, column)
            }

            // Identifier
            'a'..='z' | 'A'..='Z' | '_' => lexer.lex_identifier(),

            // Number
            '0'..='9' => lexer.lex_number(),

            // String
            '"' | '\'' => lexer.lex_string(c)?,

            // Comment (// or /*)
            '/' if lexer.peek_n(1) == Some('/') => {
                lexer.advance();
                lexer.advance();
                lexer.lex_line_comment()
            }
            '/' if lexer.peek_n(1) == Some('*') => {
                lexer.advance();
                lexer.advance();
                lexer.lex_block_comment("*/")?
            }

            // Operators
            '+' | '-' | '*' | '/' | '%' | '=' | '<' | '>' | '!' | '&' | '|' | '^' => {
                let start = lexer.position;
                let line = lexer.line;
                let column = lexer.column;
                lexer.advance_while(|c| "+-*/%=<>!&|^".contains(c));
                lexer.token(SimpleToken::Operator, start, line, column)
            }

            // Punctuation
            '(' | ')' | '[' | ']' | '{' | '}' | ',' | ';' | ':' | '.' => {
                let start = lexer.position;
                let line = lexer.line;
                let column = lexer.column;
                lexer.advance();
                lexer.token(SimpleToken::Punct, start, line, column)
            }

            // Unknown
            _ => {
                let start = lexer.position;
                let line = lexer.line;
                let column = lexer.column;
                lexer.advance();
                lexer.token(SimpleToken::Unknown, start, line, column)
            }
        };

        tokens.push(token);
    }

    // Add EOF token
    tokens.push(Token::new(SimpleToken::Eof, "", lexer.location()));

    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lexer_basic() {
        let mut lexer = Lexer::new("hello 123");
        let ident = lexer.lex_identifier();
        assert_eq!(ident.text, "hello");
        assert_eq!(ident.kind, SimpleToken::Ident);

        lexer.skip_whitespace();

        let num = lexer.lex_number();
        assert_eq!(num.text, "123");
        assert_eq!(num.kind, SimpleToken::Integer);
    }

    #[test]
    fn test_lexer_string() {
        let mut lexer = Lexer::new(r#""hello world""#);
        let s = lexer.lex_string('"').unwrap();
        assert_eq!(s.text, r#""hello world""#);
    }

    #[test]
    fn test_lex_simple() {
        let tokens = lex_simple("let x = 42;").unwrap();
        let kinds: Vec<_> = tokens.iter().map(|t| t.kind).collect();

        assert!(kinds.contains(&SimpleToken::Ident));
        assert!(kinds.contains(&SimpleToken::Integer));
        assert!(kinds.contains(&SimpleToken::Operator));
        assert!(kinds.contains(&SimpleToken::Eof));
    }

    #[test]
    fn test_token_stream() {
        let tokens = vec![
            Token::new(SimpleToken::Ident, "x", Location::new(0, 1, 1, 1)),
            Token::new(SimpleToken::Operator, "=", Location::new(2, 3, 1, 3)),
            Token::new(SimpleToken::Integer, "1", Location::new(4, 5, 1, 5)),
        ];

        let mut stream = TokenStream::new(tokens);
        assert_eq!(stream.peek().unwrap().text, "x");
        assert_eq!(stream.advance().unwrap().text, "x");
        assert_eq!(stream.peek().unwrap().text, "=");
    }
}
