//! Parser combinator utilities for drbot.
//!
//! This crate provides:
//! - Basic parser combinators
//! - Composition operators
//! - Error handling

use std::marker::PhantomData;
use thiserror::Error;

/// Parser error types.
#[derive(Error, Debug, Clone)]
pub enum ParserError {
    #[error("Expected: {0}")]
    Expected(String),

    #[error("Unexpected input")]
    Unexpected,

    #[error("Parse error: {0}")]
    Error(String),
}

/// Result type for parser operations.
pub type Result<T> = std::result::Result<T, ParserError>;

/// Parser output.
#[derive(Debug, Clone)]
pub struct ParseOutput<'a, T> {
    pub value: T,
    pub remaining: &'a str,
}

impl<'a, T> ParseOutput<'a, T> {
    pub fn new(value: T, remaining: &'a str) -> Self {
        Self { value, remaining }
    }

    pub fn map<U, F: FnOnce(T) -> U>(self, f: F) -> ParseOutput<'a, U> {
        ParseOutput {
            value: f(self.value),
            remaining: self.remaining,
        }
    }
}

/// Parser trait.
pub trait Parser<'a, T> {
    fn parse(&self, input: &'a str) -> Result<ParseOutput<'a, T>>;
}

/// Function-based parser.
pub struct FnParser<F, T> {
    parser_fn: F,
    _phantom: PhantomData<T>,
}

impl<F, T> FnParser<F, T> {
    pub fn new(f: F) -> Self {
        Self {
            parser_fn: f,
            _phantom: PhantomData,
        }
    }
}

impl<'a, F, T> Parser<'a, T> for FnParser<F, T>
where
    F: Fn(&'a str) -> Result<ParseOutput<'a, T>>,
{
    fn parse(&self, input: &'a str) -> Result<ParseOutput<'a, T>> {
        (self.parser_fn)(input)
    }
}

/// Create parser from function.
pub fn parser<'a, F, T>(f: F) -> FnParser<F, T>
where
    F: Fn(&'a str) -> Result<ParseOutput<'a, T>>,
{
    FnParser::new(f)
}

/// Match exact string.
pub fn tag<'a>(tag: &'a str) -> impl Parser<'a, &'a str> {
    FnParser::new(move |input: &'a str| {
        if input.starts_with(tag) {
            Ok(ParseOutput::new(tag, &input[tag.len()..]))
        } else {
            Err(ParserError::Expected(tag.to_string()))
        }
    })
}

/// Match single character.
pub fn char_parser<'a>(c: char) -> impl Parser<'a, char> {
    FnParser::new(move |input: &'a str| match input.chars().next() {
        Some(ch) if ch == c => Ok(ParseOutput::new(c, &input[c.len_utf8()..])),
        _ => Err(ParserError::Expected(c.to_string())),
    })
}

/// Match any character satisfying predicate.
pub fn satisfy<'a, F>(predicate: F) -> impl Parser<'a, char>
where
    F: Fn(char) -> bool + 'a,
{
    FnParser::new(move |input: &'a str| match input.chars().next() {
        Some(c) if predicate(c) => Ok(ParseOutput::new(c, &input[c.len_utf8()..])),
        _ => Err(ParserError::Unexpected),
    })
}

/// Match one or more characters satisfying predicate.
pub fn take_while1<'a, F>(predicate: F) -> impl Parser<'a, &'a str>
where
    F: Fn(char) -> bool + 'a,
{
    FnParser::new(move |input: &'a str| {
        let end = input
            .char_indices()
            .find(|(_, c)| !predicate(*c))
            .map(|(i, _)| i)
            .unwrap_or(input.len());

        if end == 0 {
            Err(ParserError::Unexpected)
        } else {
            Ok(ParseOutput::new(&input[..end], &input[end..]))
        }
    })
}

/// Match zero or more characters satisfying predicate.
pub fn take_while<'a, F>(predicate: F) -> impl Parser<'a, &'a str>
where
    F: Fn(char) -> bool + 'a,
{
    FnParser::new(move |input: &'a str| {
        let end = input
            .char_indices()
            .find(|(_, c)| !predicate(*c))
            .map(|(i, _)| i)
            .unwrap_or(input.len());

        Ok(ParseOutput::new(&input[..end], &input[end..]))
    })
}

/// Match whitespace.
pub fn whitespace<'a>() -> impl Parser<'a, &'a str> {
    take_while(|c: char| c.is_whitespace())
}

/// Match digits.
pub fn digits<'a>() -> impl Parser<'a, &'a str> {
    take_while1(|c: char| c.is_ascii_digit())
}

/// Match alphabetic characters.
pub fn alpha<'a>() -> impl Parser<'a, &'a str> {
    take_while1(|c: char| c.is_alphabetic())
}

/// Match alphanumeric characters.
pub fn alphanumeric<'a>() -> impl Parser<'a, &'a str> {
    take_while1(|c: char| c.is_alphanumeric())
}

/// Map parser result.
pub struct Map<P, F, T, U> {
    parser: P,
    map_fn: F,
    _phantom: PhantomData<(T, U)>,
}

impl<'a, P, F, T, U> Parser<'a, U> for Map<P, F, T, U>
where
    P: Parser<'a, T>,
    F: Fn(T) -> U,
{
    fn parse(&self, input: &'a str) -> Result<ParseOutput<'a, U>> {
        self.parser.parse(input).map(|out| out.map(&self.map_fn))
    }
}

/// Map over parser.
pub fn map<'a, P, F, T, U>(parser: P, f: F) -> Map<P, F, T, U>
where
    P: Parser<'a, T>,
    F: Fn(T) -> U,
{
    Map {
        parser,
        map_fn: f,
        _phantom: PhantomData,
    }
}

/// Optional parser.
pub struct Optional<P> {
    parser: P,
}

impl<'a, P, T> Parser<'a, Option<T>> for Optional<P>
where
    P: Parser<'a, T>,
{
    fn parse(&self, input: &'a str) -> Result<ParseOutput<'a, Option<T>>> {
        match self.parser.parse(input) {
            Ok(out) => Ok(ParseOutput::new(Some(out.value), out.remaining)),
            Err(_) => Ok(ParseOutput::new(None, input)),
        }
    }
}

/// Make parser optional.
pub fn optional<P>(parser: P) -> Optional<P> {
    Optional { parser }
}

/// Sequence two parsers.
pub struct Seq<P1, P2> {
    first: P1,
    second: P2,
}

impl<'a, P1, P2, T1, T2> Parser<'a, (T1, T2)> for Seq<P1, P2>
where
    P1: Parser<'a, T1>,
    P2: Parser<'a, T2>,
{
    fn parse(&self, input: &'a str) -> Result<ParseOutput<'a, (T1, T2)>> {
        let out1 = self.first.parse(input)?;
        let out2 = self.second.parse(out1.remaining)?;
        Ok(ParseOutput::new((out1.value, out2.value), out2.remaining))
    }
}

/// Sequence parsers.
pub fn seq<P1, P2>(first: P1, second: P2) -> Seq<P1, P2> {
    Seq { first, second }
}

/// Alternative parser.
pub struct Alt<P1, P2> {
    first: P1,
    second: P2,
}

impl<'a, P1, P2, T> Parser<'a, T> for Alt<P1, P2>
where
    P1: Parser<'a, T>,
    P2: Parser<'a, T>,
{
    fn parse(&self, input: &'a str) -> Result<ParseOutput<'a, T>> {
        match self.first.parse(input) {
            Ok(out) => Ok(out),
            Err(_) => self.second.parse(input),
        }
    }
}

/// Alternative.
pub fn alt<P1, P2>(first: P1, second: P2) -> Alt<P1, P2> {
    Alt { first, second }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tag() {
        let parser = tag("hello");
        let result = parser.parse("hello world").unwrap();

        assert_eq!(result.value, "hello");
        assert_eq!(result.remaining, " world");
    }

    #[test]
    fn test_char_parser() {
        let parser = char_parser('a');
        let result = parser.parse("abc").unwrap();

        assert_eq!(result.value, 'a');
        assert_eq!(result.remaining, "bc");
    }

    #[test]
    fn test_digits() {
        let parser = digits();
        let result = parser.parse("123abc").unwrap();

        assert_eq!(result.value, "123");
        assert_eq!(result.remaining, "abc");
    }

    #[test]
    fn test_map() {
        let parser = map(digits(), |s| s.parse::<i32>().unwrap());
        let result = parser.parse("42rest").unwrap();

        assert_eq!(result.value, 42);
    }

    #[test]
    fn test_optional() {
        let parser = optional(tag("maybe"));

        let r1 = parser.parse("maybe here").unwrap();
        assert_eq!(r1.value, Some("maybe"));

        let r2 = parser.parse("not here").unwrap();
        assert_eq!(r2.value, None);
        assert_eq!(r2.remaining, "not here");
    }

    #[test]
    fn test_seq() {
        let parser = seq(alpha(), seq(whitespace(), digits()));
        let result = parser.parse("hello 123").unwrap();

        assert_eq!(result.value.0, "hello");
        assert_eq!((result.value.1).1, "123");
    }

    #[test]
    fn test_alt() {
        let parser = alt(tag("yes"), tag("no"));

        let r1 = parser.parse("yes").unwrap();
        assert_eq!(r1.value, "yes");

        let r2 = parser.parse("no").unwrap();
        assert_eq!(r2.value, "no");
    }
}
