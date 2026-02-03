//! Display trait extensions for drbot.
//!
//! This crate provides:
//! - Display formatting utilities
//! - Custom display formatting
//! - Display adapters

use std::fmt::{self, Display};
use thiserror::Error;

/// Display extension error types.
#[derive(Error, Debug, Clone)]
pub enum DisplayExtError {
    #[error("Format error: {0}")]
    Format(String),
}

/// Result type for display operations.
pub type Result<T> = std::result::Result<T, DisplayExtError>;

/// Display to string.
pub fn display_string<T: Display + ?Sized>(value: &T) -> String {
    format!("{}", value)
}

/// Display with width.
pub fn display_width<T: Display + ?Sized>(value: &T, width: usize) -> String {
    format!("{:width$}", value, width = width)
}

/// Display right-aligned.
pub fn display_right<T: Display + ?Sized>(value: &T, width: usize) -> String {
    format!("{:>width$}", value, width = width)
}

/// Display centered.
pub fn display_center<T: Display + ?Sized>(value: &T, width: usize) -> String {
    format!("{:^width$}", value, width = width)
}

/// Display with fill char.
pub fn display_fill<T: Display>(value: &T, width: usize, fill: char) -> String {
    let s = format!("{}", value);
    if s.len() >= width {
        s
    } else {
        let padding = width - s.len();
        format!("{}{}", fill.to_string().repeat(padding), s)
    }
}

/// Display extension trait.
pub trait DisplayExt: Display {
    /// To display string.
    fn to_display(&self) -> String {
        display_string(self)
    }

    /// To fixed width.
    fn to_width(&self, width: usize) -> String {
        display_width(self, width)
    }

    /// To right-aligned.
    fn to_right(&self, width: usize) -> String {
        display_right(self, width)
    }

    /// To centered.
    fn to_center(&self, width: usize) -> String {
        display_center(self, width)
    }
}

impl<T: Display> DisplayExt for T {}

/// Display wrapper with custom format.
pub struct DisplayWith<T, F>
where
    F: Fn(&T, &mut fmt::Formatter<'_>) -> fmt::Result,
{
    value: T,
    formatter: F,
}

impl<T, F> DisplayWith<T, F>
where
    F: Fn(&T, &mut fmt::Formatter<'_>) -> fmt::Result,
{
    /// Create new.
    pub fn new(value: T, formatter: F) -> Self {
        Self { value, formatter }
    }
}

impl<T, F> Display for DisplayWith<T, F>
where
    F: Fn(&T, &mut fmt::Formatter<'_>) -> fmt::Result,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        (self.formatter)(&self.value, f)
    }
}

/// Join items for display.
pub fn display_join<T: Display>(items: &[T], sep: &str) -> String {
    items
        .iter()
        .map(|i| format!("{}", i))
        .collect::<Vec<_>>()
        .join(sep)
}

/// Display with prefix.
pub fn display_prefix<T: Display>(value: &T, prefix: &str) -> String {
    format!("{}{}", prefix, value)
}

/// Display with suffix.
pub fn display_suffix<T: Display>(value: &T, suffix: &str) -> String {
    format!("{}{}", value, suffix)
}

/// Display wrapped.
pub fn display_wrapped<T: Display>(value: &T, left: &str, right: &str) -> String {
    format!("{}{}{}", left, value, right)
}

/// Quoted display.
pub fn display_quoted<T: Display>(value: &T) -> String {
    display_wrapped(value, "\"", "\"")
}

/// Parenthesized display.
pub fn display_parens<T: Display>(value: &T) -> String {
    display_wrapped(value, "(", ")")
}

/// Bracketed display.
pub fn display_brackets<T: Display>(value: &T) -> String {
    display_wrapped(value, "[", "]")
}

/// Braced display.
pub fn display_braces<T: Display>(value: &T) -> String {
    display_wrapped(value, "{", "}")
}

/// Conditional display.
pub struct ConditionalDisplay<T: Display> {
    value: T,
    condition: bool,
    fallback: String,
}

impl<T: Display> ConditionalDisplay<T> {
    /// Create new.
    pub fn new(value: T, condition: bool, fallback: &str) -> Self {
        Self {
            value,
            condition,
            fallback: fallback.to_string(),
        }
    }
}

impl<T: Display> Display for ConditionalDisplay<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.condition {
            write!(f, "{}", self.value)
        } else {
            write!(f, "{}", self.fallback)
        }
    }
}

/// Optional display.
pub struct OptionalDisplay<T: Display> {
    value: Option<T>,
    none_str: String,
}

impl<T: Display> OptionalDisplay<T> {
    /// Create new.
    pub fn new(value: Option<T>, none_str: &str) -> Self {
        Self {
            value,
            none_str: none_str.to_string(),
        }
    }
}

impl<T: Display> Display for OptionalDisplay<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.value {
            Some(v) => write!(f, "{}", v),
            None => write!(f, "{}", self.none_str),
        }
    }
}

/// Truncate display.
pub fn display_truncate<T: Display>(value: &T, max_len: usize) -> String {
    let s = format!("{}", value);
    if s.len() <= max_len {
        s
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}

/// Pad display.
pub fn display_pad<T: Display>(value: &T, width: usize, pad: char, align: Align) -> String {
    let s = format!("{}", value);
    if s.len() >= width {
        return s;
    }

    let padding = width - s.len();
    match align {
        Align::Left => format!("{}{}", s, pad.to_string().repeat(padding)),
        Align::Right => format!("{}{}", pad.to_string().repeat(padding), s),
        Align::Center => {
            let left = padding / 2;
            let right = padding - left;
            format!(
                "{}{}{}",
                pad.to_string().repeat(left),
                s,
                pad.to_string().repeat(right)
            )
        }
    }
}

/// Alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    Right,
    Center,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_display_string() {
        assert_eq!(display_string(&42), "42");
        assert_eq!(display_string(&"hello"), "hello");
    }

    #[test]
    fn test_display_width() {
        assert_eq!(display_width(&42, 5), "42   ");
        assert_eq!(display_right(&42, 5), "   42");
        assert_eq!(display_center(&42, 5), " 42  ");
    }

    #[test]
    fn test_display_join() {
        assert_eq!(display_join(&[1, 2, 3], ", "), "1, 2, 3");
    }

    #[test]
    fn test_display_wrapped() {
        assert_eq!(display_quoted(&"hello"), "\"hello\"");
        assert_eq!(display_parens(&42), "(42)");
        assert_eq!(display_brackets(&42), "[42]");
    }

    #[test]
    fn test_optional_display() {
        let some = OptionalDisplay::new(Some(42), "none");
        let none: OptionalDisplay<i32> = OptionalDisplay::new(None, "none");
        assert_eq!(format!("{}", some), "42");
        assert_eq!(format!("{}", none), "none");
    }

    #[test]
    fn test_display_pad() {
        assert_eq!(display_pad(&42, 5, '0', Align::Right), "00042");
        assert_eq!(display_pad(&42, 5, '-', Align::Left), "42---");
    }
}
