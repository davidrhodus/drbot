//! Parser combinators and utilities for drbot.
//!
//! This crate provides:
//! - Parser combinator primitives
//! - Error recovery
//! - Position tracking
//! - Common parsers

use std::fmt;
use thiserror::Error;

/// Parser error types.
#[derive(Error, Debug, Clone)]
pub enum ParseError {
    #[error("Unexpected end of input")]
    UnexpectedEof,

    #[error("Expected '{expected}', found '{found}'")]
    Expected { expected: String, found: String },

    #[error("Unexpected character: '{0}'")]
    UnexpectedChar(char),

    #[error("Invalid syntax at position {0}: {1}")]
    InvalidSyntax(usize, String),

    #[error("Parse error: {0}")]
    Custom(String),
}

/// Result type for parser operations.
pub type Result<T> = std::result::Result<T, ParseError>;

/// Position in input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Position {
    /// Byte offset.
    pub offset: usize,
    /// Line number (1-based).
    pub line: usize,
    /// Column number (1-based).
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

    /// Advance by character.
    pub fn advance(&mut self, c: char) {
        self.offset += c.len_utf8();
        if c == '\n' {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }
    }
}

impl fmt::Display for Position {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.line, self.column)
    }
}

/// Span in input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    /// Start position.
    pub start: Position,
    /// End position.
    pub end: Position,
}

impl Span {
    /// Create new span.
    pub fn new(start: Position, end: Position) -> Self {
        Self { start, end }
    }

    /// Create empty span at position.
    pub fn empty(pos: Position) -> Self {
        Self {
            start: pos,
            end: pos,
        }
    }

    /// Merge two spans.
    pub fn merge(&self, other: &Span) -> Self {
        Self {
            start: if self.start.offset < other.start.offset {
                self.start
            } else {
                other.start
            },
            end: if self.end.offset > other.end.offset {
                self.end
            } else {
                other.end
            },
        }
    }
}

/// Parser input.
#[derive(Debug, Clone)]
pub struct Input<'a> {
    /// Input string.
    source: &'a str,
    /// Current position.
    position: Position,
}

impl<'a> Input<'a> {
    /// Create new input.
    pub fn new(source: &'a str) -> Self {
        Self {
            source,
            position: Position::start(),
        }
    }

    /// Get remaining input.
    pub fn remaining(&self) -> &'a str {
        &self.source[self.position.offset..]
    }

    /// Check if at end.
    pub fn is_empty(&self) -> bool {
        self.position.offset >= self.source.len()
    }

    /// Peek next character.
    pub fn peek(&self) -> Option<char> {
        self.remaining().chars().next()
    }

    /// Peek n characters ahead.
    pub fn peek_n(&self, n: usize) -> Option<char> {
        self.remaining().chars().nth(n)
    }

    /// Get current position.
    pub fn position(&self) -> Position {
        self.position
    }

    /// Advance by one character.
    pub fn advance(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.position.advance(c);
        Some(c)
    }

    /// Advance while predicate holds.
    pub fn advance_while<F>(&mut self, predicate: F) -> &'a str
    where
        F: Fn(char) -> bool,
    {
        let start = self.position.offset;
        while let Some(c) = self.peek() {
            if !predicate(c) {
                break;
            }
            self.advance();
        }
        &self.source[start..self.position.offset]
    }

    /// Skip whitespace.
    pub fn skip_whitespace(&mut self) {
        self.advance_while(|c| c.is_whitespace());
    }

    /// Try to match string.
    pub fn match_str(&mut self, s: &str) -> bool {
        if self.remaining().starts_with(s) {
            for c in s.chars() {
                self.position.advance(c);
            }
            true
        } else {
            false
        }
    }

    /// Fork input for lookahead.
    pub fn fork(&self) -> Self {
        self.clone()
    }
}

/// Parser output.
#[derive(Debug, Clone)]
pub struct Output<T> {
    /// Parsed value.
    pub value: T,
    /// Span of parsed content.
    pub span: Span,
}

impl<T> Output<T> {
    /// Create new output.
    pub fn new(value: T, span: Span) -> Self {
        Self { value, span }
    }

    /// Map the value.
    pub fn map<U, F>(self, f: F) -> Output<U>
    where
        F: FnOnce(T) -> U,
    {
        Output {
            value: f(self.value),
            span: self.span,
        }
    }
}

/// Parser trait.
pub trait Parser<'a>: Sized {
    /// Output type.
    type Output;

    /// Parse input.
    fn parse(&self, input: &mut Input<'a>) -> Result<Output<Self::Output>>;

    /// Map parser output.
    fn map<F, U>(self, f: F) -> Map<Self, F>
    where
        F: Fn(Self::Output) -> U,
    {
        Map { parser: self, f }
    }

    /// Sequence with another parser.
    fn then<P>(self, other: P) -> Then<Self, P>
    where
        P: Parser<'a>,
    {
        Then {
            first: self,
            second: other,
        }
    }

    /// Alternative parser.
    fn or<P>(self, other: P) -> Or<Self, P>
    where
        P: Parser<'a, Output = Self::Output>,
    {
        Or {
            first: self,
            second: other,
        }
    }

    /// Optional parser.
    fn optional(self) -> Optional<Self> {
        Optional { parser: self }
    }

    /// Repeat parser.
    fn many(self) -> Many<Self> {
        Many {
            parser: self,
            min: 0,
        }
    }

    /// Repeat at least once.
    fn many1(self) -> Many<Self> {
        Many {
            parser: self,
            min: 1,
        }
    }
}

/// Map combinator.
pub struct Map<P, F> {
    parser: P,
    f: F,
}

impl<'a, P, F, U> Parser<'a> for Map<P, F>
where
    P: Parser<'a>,
    F: Fn(P::Output) -> U,
{
    type Output = U;

    fn parse(&self, input: &mut Input<'a>) -> Result<Output<Self::Output>> {
        let output = self.parser.parse(input)?;
        Ok(output.map(&self.f))
    }
}

/// Then combinator.
pub struct Then<P1, P2> {
    first: P1,
    second: P2,
}

impl<'a, P1, P2> Parser<'a> for Then<P1, P2>
where
    P1: Parser<'a>,
    P2: Parser<'a>,
{
    type Output = (P1::Output, P2::Output);

    fn parse(&self, input: &mut Input<'a>) -> Result<Output<Self::Output>> {
        let first = self.first.parse(input)?;
        let second = self.second.parse(input)?;
        let span = first.span.merge(&second.span);
        Ok(Output::new((first.value, second.value), span))
    }
}

/// Or combinator.
pub struct Or<P1, P2> {
    first: P1,
    second: P2,
}

impl<'a, P1, P2> Parser<'a> for Or<P1, P2>
where
    P1: Parser<'a>,
    P2: Parser<'a, Output = P1::Output>,
{
    type Output = P1::Output;

    fn parse(&self, input: &mut Input<'a>) -> Result<Output<Self::Output>> {
        let mut fork = input.fork();
        match self.first.parse(&mut fork) {
            Ok(output) => {
                *input = fork;
                Ok(output)
            }
            Err(_) => self.second.parse(input),
        }
    }
}

/// Optional combinator.
pub struct Optional<P> {
    parser: P,
}

impl<'a, P> Parser<'a> for Optional<P>
where
    P: Parser<'a>,
{
    type Output = Option<P::Output>;

    fn parse(&self, input: &mut Input<'a>) -> Result<Output<Self::Output>> {
        let start = input.position();
        let mut fork = input.fork();
        match self.parser.parse(&mut fork) {
            Ok(output) => {
                *input = fork;
                Ok(Output::new(Some(output.value), output.span))
            }
            Err(_) => Ok(Output::new(None, Span::empty(start))),
        }
    }
}

/// Many combinator.
pub struct Many<P> {
    parser: P,
    min: usize,
}

impl<'a, P> Parser<'a> for Many<P>
where
    P: Parser<'a>,
{
    type Output = Vec<P::Output>;

    fn parse(&self, input: &mut Input<'a>) -> Result<Output<Self::Output>> {
        let start = input.position();
        let mut values = Vec::new();

        loop {
            let mut fork = input.fork();
            match self.parser.parse(&mut fork) {
                Ok(output) => {
                    *input = fork;
                    values.push(output.value);
                }
                Err(_) => break,
            }
        }

        if values.len() < self.min {
            return Err(ParseError::Custom(format!(
                "Expected at least {} items, got {}",
                self.min,
                values.len()
            )));
        }

        let end = input.position();
        Ok(Output::new(values, Span::new(start, end)))
    }
}

/// Parse a single character.
pub fn char_parser(expected: char) -> impl for<'a> Parser<'a, Output = char> {
    CharParser(expected)
}

struct CharParser(char);

impl<'a> Parser<'a> for CharParser {
    type Output = char;

    fn parse(&self, input: &mut Input<'a>) -> Result<Output<Self::Output>> {
        let start = input.position();
        match input.advance() {
            Some(c) if c == self.0 => {
                let end = input.position();
                Ok(Output::new(c, Span::new(start, end)))
            }
            Some(c) => Err(ParseError::Expected {
                expected: self.0.to_string(),
                found: c.to_string(),
            }),
            None => Err(ParseError::UnexpectedEof),
        }
    }
}

/// Parse identifier.
pub fn identifier() -> impl for<'a> Parser<'a, Output = String> {
    IdentifierParser
}

struct IdentifierParser;

impl<'a> Parser<'a> for IdentifierParser {
    type Output = String;

    fn parse(&self, input: &mut Input<'a>) -> Result<Output<Self::Output>> {
        let start = input.position();

        let first = input.peek().ok_or(ParseError::UnexpectedEof)?;
        if !first.is_alphabetic() && first != '_' {
            return Err(ParseError::UnexpectedChar(first));
        }
        input.advance();

        let rest = input.advance_while(|c| c.is_alphanumeric() || c == '_');
        let end = input.position();

        let ident = format!("{}{}", first, rest);
        Ok(Output::new(ident, Span::new(start, end)))
    }
}

/// Parse integer.
pub fn integer() -> impl for<'a> Parser<'a, Output = i64> {
    IntegerParser
}

struct IntegerParser;

impl<'a> Parser<'a> for IntegerParser {
    type Output = i64;

    fn parse(&self, input: &mut Input<'a>) -> Result<Output<Self::Output>> {
        let start = input.position();

        let negative = input.match_str("-");
        let digits = input.advance_while(|c| c.is_ascii_digit());

        if digits.is_empty() {
            return Err(ParseError::Custom("Expected integer".to_string()));
        }

        let end = input.position();
        let value: i64 = digits
            .parse()
            .map_err(|_| ParseError::Custom(format!("Invalid integer: {}", digits)))?;

        Ok(Output::new(
            if negative { -value } else { value },
            Span::new(start, end),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_position() {
        let mut pos = Position::start();
        pos.advance('a');
        assert_eq!(pos.offset, 1);
        assert_eq!(pos.column, 2);

        pos.advance('\n');
        assert_eq!(pos.line, 2);
        assert_eq!(pos.column, 1);
    }

    #[test]
    fn test_input() {
        let mut input = Input::new("hello world");
        assert_eq!(input.peek(), Some('h'));
        input.advance();
        assert_eq!(input.peek(), Some('e'));
        assert_eq!(input.advance_while(|c| c.is_alphabetic()), "ello");
    }

    #[test]
    fn test_char_parser() {
        let parser = char_parser('a');
        let mut input = Input::new("abc");
        let result = parser.parse(&mut input).unwrap();
        assert_eq!(result.value, 'a');
    }

    #[test]
    fn test_identifier() {
        let parser = identifier();
        let mut input = Input::new("foo_bar123");
        let result = parser.parse(&mut input).unwrap();
        assert_eq!(result.value, "foo_bar123");
    }

    #[test]
    fn test_integer() {
        let parser = integer();

        let mut input = Input::new("123");
        let result = parser.parse(&mut input).unwrap();
        assert_eq!(result.value, 123);

        let mut input = Input::new("-456");
        let result = parser.parse(&mut input).unwrap();
        assert_eq!(result.value, -456);
    }
}
