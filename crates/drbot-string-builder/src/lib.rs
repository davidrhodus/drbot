//! String builder for drbot.
//!
//! This crate provides:
//! - Fluent string building
//! - Efficient string concatenation
//! - Builder patterns

use std::fmt::{self, Display, Write};
use thiserror::Error;

/// Builder error types.
#[derive(Error, Debug, Clone)]
pub enum BuilderError {
    #[error("Format error")]
    FormatError,

    #[error("Capacity exceeded")]
    CapacityExceeded,
}

/// Result type for builder operations.
pub type Result<T> = std::result::Result<T, BuilderError>;

/// String builder with fluent API.
#[derive(Debug, Clone)]
pub struct StringBuilder {
    buffer: String,
}

impl StringBuilder {
    /// Create new builder.
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
        }
    }

    /// Create with capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            buffer: String::with_capacity(capacity),
        }
    }

    /// Append string.
    pub fn append(mut self, s: &str) -> Self {
        self.buffer.push_str(s);
        self
    }

    /// Append owned string.
    pub fn append_string(mut self, s: String) -> Self {
        self.buffer.push_str(&s);
        self
    }

    /// Append display value.
    pub fn append_display<T: Display>(mut self, value: T) -> Self {
        write!(self.buffer, "{}", value).ok();
        self
    }

    /// Append char.
    pub fn append_char(mut self, c: char) -> Self {
        self.buffer.push(c);
        self
    }

    /// Append line.
    pub fn append_line(mut self, s: &str) -> Self {
        self.buffer.push_str(s);
        self.buffer.push('\n');
        self
    }

    /// Append newline.
    pub fn newline(mut self) -> Self {
        self.buffer.push('\n');
        self
    }

    /// Append space.
    pub fn space(mut self) -> Self {
        self.buffer.push(' ');
        self
    }

    /// Append tab.
    pub fn tab(mut self) -> Self {
        self.buffer.push('\t');
        self
    }

    /// Append n spaces.
    pub fn spaces(mut self, n: usize) -> Self {
        self.buffer.extend(std::iter::repeat(' ').take(n));
        self
    }

    /// Append repeated string.
    pub fn repeat(mut self, s: &str, n: usize) -> Self {
        for _ in 0..n {
            self.buffer.push_str(s);
        }
        self
    }

    /// Append with format.
    pub fn append_fmt(mut self, args: fmt::Arguments<'_>) -> Self {
        write!(self.buffer, "{}", args).ok();
        self
    }

    /// Append conditionally.
    pub fn append_if(self, condition: bool, s: &str) -> Self {
        if condition {
            self.append(s)
        } else {
            self
        }
    }

    /// Append with separator.
    pub fn append_with_sep(mut self, sep: &str, items: &[&str]) -> Self {
        for (i, item) in items.iter().enumerate() {
            if i > 0 {
                self.buffer.push_str(sep);
            }
            self.buffer.push_str(item);
        }
        self
    }

    /// Append iterator with separator.
    pub fn append_iter_sep<I, T>(mut self, sep: &str, iter: I) -> Self
    where
        I: IntoIterator<Item = T>,
        T: Display,
    {
        let mut first = true;
        for item in iter {
            if !first {
                self.buffer.push_str(sep);
            }
            write!(self.buffer, "{}", item).ok();
            first = false;
        }
        self
    }

    /// Length.
    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    /// Is empty.
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    /// Capacity.
    pub fn capacity(&self) -> usize {
        self.buffer.capacity()
    }

    /// Clear.
    pub fn clear(mut self) -> Self {
        self.buffer.clear();
        self
    }

    /// Build string.
    pub fn build(self) -> String {
        self.buffer
    }

    /// Build and trim.
    pub fn build_trimmed(self) -> String {
        self.buffer.trim().to_string()
    }

    /// Get current content.
    pub fn as_str(&self) -> &str {
        &self.buffer
    }
}

impl Default for StringBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl From<StringBuilder> for String {
    fn from(sb: StringBuilder) -> Self {
        sb.build()
    }
}

impl From<&str> for StringBuilder {
    fn from(s: &str) -> Self {
        Self {
            buffer: s.to_string(),
        }
    }
}

impl From<String> for StringBuilder {
    fn from(s: String) -> Self {
        Self { buffer: s }
    }
}

impl fmt::Display for StringBuilder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.buffer)
    }
}

/// Create new builder.
pub fn builder() -> StringBuilder {
    StringBuilder::new()
}

/// Create builder with initial string.
pub fn from_str(s: &str) -> StringBuilder {
    StringBuilder::from(s)
}

/// Join items with separator.
pub fn join<I, T>(sep: &str, iter: I) -> String
where
    I: IntoIterator<Item = T>,
    T: Display,
{
    StringBuilder::new().append_iter_sep(sep, iter).build()
}

/// Join strings with separator.
pub fn join_strs(sep: &str, items: &[&str]) -> String {
    items.join(sep)
}

/// Concat strings.
pub fn concat(items: &[&str]) -> String {
    items.concat()
}

/// Indented builder.
pub struct IndentedBuilder {
    buffer: String,
    indent: String,
    indent_level: usize,
    at_line_start: bool,
}

impl IndentedBuilder {
    /// Create new.
    pub fn new(indent: &str) -> Self {
        Self {
            buffer: String::new(),
            indent: indent.to_string(),
            indent_level: 0,
            at_line_start: true,
        }
    }

    /// Increase indent.
    pub fn indent(mut self) -> Self {
        self.indent_level += 1;
        self
    }

    /// Decrease indent.
    pub fn dedent(mut self) -> Self {
        self.indent_level = self.indent_level.saturating_sub(1);
        self
    }

    fn write_indent(&mut self) {
        if self.at_line_start && self.indent_level > 0 {
            for _ in 0..self.indent_level {
                self.buffer.push_str(&self.indent);
            }
        }
        self.at_line_start = false;
    }

    /// Append.
    pub fn append(mut self, s: &str) -> Self {
        for (i, line) in s.split('\n').enumerate() {
            if i > 0 {
                self.buffer.push('\n');
                self.at_line_start = true;
            }
            if !line.is_empty() {
                self.write_indent();
                self.buffer.push_str(line);
            }
        }
        self
    }

    /// Append line.
    pub fn line(mut self, s: &str) -> Self {
        self.write_indent();
        self.buffer.push_str(s);
        self.buffer.push('\n');
        self.at_line_start = true;
        self
    }

    /// Empty line.
    pub fn empty_line(mut self) -> Self {
        self.buffer.push('\n');
        self.at_line_start = true;
        self
    }

    /// Build.
    pub fn build(self) -> String {
        self.buffer
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_string_builder() {
        let s = StringBuilder::new()
            .append("Hello")
            .space()
            .append("World")
            .build();
        assert_eq!(s, "Hello World");
    }

    #[test]
    fn test_append_line() {
        let s = StringBuilder::new()
            .append_line("Line 1")
            .append_line("Line 2")
            .build();
        assert_eq!(s, "Line 1\nLine 2\n");
    }

    #[test]
    fn test_append_if() {
        let s = StringBuilder::new()
            .append("start")
            .append_if(true, "-yes")
            .append_if(false, "-no")
            .build();
        assert_eq!(s, "start-yes");
    }

    #[test]
    fn test_join() {
        let items = vec![1, 2, 3];
        assert_eq!(join(", ", items), "1, 2, 3");
    }

    #[test]
    fn test_indented_builder() {
        let s = IndentedBuilder::new("  ")
            .line("start")
            .indent()
            .line("indented")
            .indent()
            .line("more indented")
            .dedent()
            .line("back")
            .build();
        assert_eq!(s, "start\n  indented\n    more indented\n  back\n");
    }
}
