//! Tokenization utilities for drbot.
//!
//! This crate provides:
//! - Generic tokenizer trait
//! - Common token types
//! - Whitespace handling

use thiserror::Error;

/// Tokenizer error types.
#[derive(Error, Debug, Clone)]
pub enum TokenError {
    #[error("Unexpected character: {0}")]
    UnexpectedChar(char),

    #[error("Unterminated string")]
    UnterminatedString,

    #[error("Invalid number: {0}")]
    InvalidNumber(String),

    #[error("Unexpected end of input")]
    UnexpectedEof,
}

/// Result type for tokenizer operations.
pub type Result<T> = std::result::Result<T, TokenError>;

/// Token with position information.
#[derive(Debug, Clone, PartialEq)]
pub struct Token<T> {
    pub kind: T,
    pub text: String,
    pub span: Span,
}

impl<T> Token<T> {
    /// Create new token.
    pub fn new(kind: T, text: impl Into<String>, span: Span) -> Self {
        Self {
            kind,
            text: text.into(),
            span,
        }
    }
}

/// Span in source text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    /// Create new span.
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    /// Get span length.
    pub fn len(&self) -> usize {
        self.end.saturating_sub(self.start)
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.start >= self.end
    }

    /// Combine two spans.
    pub fn join(&self, other: &Span) -> Span {
        Span {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }
}

/// Common token kinds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommonToken {
    Identifier(String),
    Integer(i64),
    Float(String),
    String(String),
    Operator(String),
    Punctuation(char),
    Whitespace,
    Newline,
    Comment(String),
    Eof,
}

/// Generic tokenizer trait.
pub trait Tokenizer {
    type Token;

    /// Get next token.
    fn next_token(&mut self) -> Result<Token<Self::Token>>;

    /// Peek at next token without consuming.
    fn peek_token(&mut self) -> Result<&Token<Self::Token>>;

    /// Check if at end.
    fn is_eof(&self) -> bool;
}

/// Simple string tokenizer.
pub struct StringTokenizer<'a> {
    input: &'a str,
    position: usize,
    peeked: Option<Token<CommonToken>>,
}

impl<'a> StringTokenizer<'a> {
    /// Create new tokenizer.
    pub fn new(input: &'a str) -> Self {
        Self {
            input,
            position: 0,
            peeked: None,
        }
    }

    /// Get current position.
    pub fn position(&self) -> usize {
        self.position
    }

    /// Get remaining input.
    pub fn remaining(&self) -> &str {
        &self.input[self.position..]
    }

    fn current_char(&self) -> Option<char> {
        self.input[self.position..].chars().next()
    }

    fn advance(&mut self) -> Option<char> {
        let c = self.current_char()?;
        self.position += c.len_utf8();
        Some(c)
    }

    fn skip_whitespace(&mut self) {
        while let Some(c) = self.current_char() {
            if c.is_whitespace() && c != '\n' {
                self.advance();
            } else {
                break;
            }
        }
    }

    fn read_identifier(&mut self) -> Token<CommonToken> {
        let start = self.position;
        while let Some(c) = self.current_char() {
            if c.is_alphanumeric() || c == '_' {
                self.advance();
            } else {
                break;
            }
        }
        let text = &self.input[start..self.position];
        Token::new(
            CommonToken::Identifier(text.to_string()),
            text,
            Span::new(start, self.position),
        )
    }

    fn read_number(&mut self) -> Result<Token<CommonToken>> {
        let start = self.position;
        let mut has_dot = false;

        while let Some(c) = self.current_char() {
            if c.is_ascii_digit() {
                self.advance();
            } else if c == '.' && !has_dot {
                has_dot = true;
                self.advance();
            } else {
                break;
            }
        }

        let text = &self.input[start..self.position];
        let kind = if has_dot {
            CommonToken::Float(text.to_string())
        } else {
            let value: i64 = text
                .parse()
                .map_err(|_| TokenError::InvalidNumber(text.to_string()))?;
            CommonToken::Integer(value)
        };

        Ok(Token::new(kind, text, Span::new(start, self.position)))
    }

    fn read_string(&mut self, quote: char) -> Result<Token<CommonToken>> {
        let start = self.position;
        self.advance(); // consume opening quote

        let mut value = String::new();
        loop {
            match self.current_char() {
                None => return Err(TokenError::UnterminatedString),
                Some(c) if c == quote => {
                    self.advance();
                    break;
                }
                Some('\\') => {
                    self.advance();
                    if let Some(escaped) = self.advance() {
                        match escaped {
                            'n' => value.push('\n'),
                            't' => value.push('\t'),
                            'r' => value.push('\r'),
                            '\\' => value.push('\\'),
                            c if c == quote => value.push(c),
                            _ => value.push(escaped),
                        }
                    }
                }
                Some(c) => {
                    value.push(c);
                    self.advance();
                }
            }
        }

        let text = &self.input[start..self.position];
        Ok(Token::new(
            CommonToken::String(value),
            text,
            Span::new(start, self.position),
        ))
    }
}

impl<'a> Tokenizer for StringTokenizer<'a> {
    type Token = CommonToken;

    fn next_token(&mut self) -> Result<Token<CommonToken>> {
        if let Some(token) = self.peeked.take() {
            return Ok(token);
        }

        self.skip_whitespace();

        let start = self.position;

        match self.current_char() {
            None => Ok(Token::new(CommonToken::Eof, "", Span::new(start, start))),
            Some('\n') => {
                self.advance();
                Ok(Token::new(
                    CommonToken::Newline,
                    "\n",
                    Span::new(start, self.position),
                ))
            }
            Some(c) if c.is_alphabetic() || c == '_' => Ok(self.read_identifier()),
            Some(c) if c.is_ascii_digit() => self.read_number(),
            Some('"') => self.read_string('"'),
            Some('\'') => self.read_string('\''),
            Some(c) => {
                self.advance();
                Ok(Token::new(
                    CommonToken::Punctuation(c),
                    c.to_string(),
                    Span::new(start, self.position),
                ))
            }
        }
    }

    fn peek_token(&mut self) -> Result<&Token<CommonToken>> {
        if self.peeked.is_none() {
            self.peeked = Some(self.next_token()?);
        }
        Ok(self.peeked.as_ref().unwrap())
    }

    fn is_eof(&self) -> bool {
        self.position >= self.input.len()
    }
}

/// Collect all tokens.
pub fn tokenize<T: Tokenizer>(tokenizer: &mut T) -> Result<Vec<Token<T::Token>>>
where
    T::Token: PartialEq,
{
    let mut tokens = Vec::new();
    loop {
        let token = tokenizer.next_token()?;
        let is_eof = tokenizer.is_eof();
        tokens.push(token);
        if is_eof {
            break;
        }
    }
    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenize_identifier() {
        let mut tokenizer = StringTokenizer::new("hello world");
        let token = tokenizer.next_token().unwrap();

        assert!(matches!(token.kind, CommonToken::Identifier(s) if s == "hello"));
    }

    #[test]
    fn test_tokenize_number() {
        let mut tokenizer = StringTokenizer::new("123 45.67");
        let t1 = tokenizer.next_token().unwrap();
        let t2 = tokenizer.next_token().unwrap();

        assert!(matches!(t1.kind, CommonToken::Integer(123)));
        assert!(matches!(t2.kind, CommonToken::Float(s) if s == "45.67"));
    }

    #[test]
    fn test_tokenize_string() {
        let mut tokenizer = StringTokenizer::new(r#""hello\nworld""#);
        let token = tokenizer.next_token().unwrap();

        assert!(matches!(token.kind, CommonToken::String(s) if s == "hello\nworld"));
    }

    #[test]
    fn test_span() {
        let s1 = Span::new(0, 5);
        let s2 = Span::new(3, 10);

        assert_eq!(s1.len(), 5);
        assert_eq!(s1.join(&s2), Span::new(0, 10));
    }
}
