//! Debug trait extensions for drbot.
//!
//! This crate provides:
//! - Debug formatting utilities
//! - Pretty printing
//! - Debug output customization

use std::fmt::{Debug, Write};
use thiserror::Error;

/// Debug extension error types.
#[derive(Error, Debug, Clone)]
pub enum DebugExtError {
    #[error("Format error: {0}")]
    Format(String),
}

/// Result type for debug operations.
pub type Result<T> = std::result::Result<T, DebugExtError>;

/// Debug to string.
pub fn debug_string<T: Debug + ?Sized>(value: &T) -> String {
    format!("{:?}", value)
}

/// Debug to pretty string.
pub fn debug_pretty<T: Debug + ?Sized>(value: &T) -> String {
    format!("{:#?}", value)
}

/// Debug to compact string (no whitespace).
pub fn debug_compact<T: Debug + ?Sized>(value: &T) -> String {
    format!("{:?}", value)
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect()
}

/// Debug with max length.
pub fn debug_truncated<T: Debug + ?Sized>(value: &T, max_len: usize) -> String {
    let s = format!("{:?}", value);
    if s.len() <= max_len {
        s
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}

/// Debug extension trait.
pub trait DebugExt: Debug {
    /// To debug string.
    fn to_debug(&self) -> String {
        debug_string(self)
    }

    /// To pretty debug string.
    fn to_debug_pretty(&self) -> String {
        debug_pretty(self)
    }

    /// To compact debug string.
    fn to_debug_compact(&self) -> String {
        debug_compact(self)
    }

    /// To truncated debug string.
    fn to_debug_truncated(&self, max_len: usize) -> String {
        debug_truncated(self, max_len)
    }
}

impl<T: Debug> DebugExt for T {}

/// Debug builder for custom formatting.
pub struct DebugBuilder {
    output: String,
    indent: usize,
    indent_str: String,
}

impl DebugBuilder {
    /// Create new builder.
    pub fn new() -> Self {
        Self {
            output: String::new(),
            indent: 0,
            indent_str: "  ".to_string(),
        }
    }

    /// Set indent string.
    pub fn with_indent(mut self, indent: &str) -> Self {
        self.indent_str = indent.to_string();
        self
    }

    /// Add text.
    pub fn text(mut self, s: &str) -> Self {
        self.output.push_str(s);
        self
    }

    /// Add line.
    pub fn line(mut self, s: &str) -> Self {
        self.output.push_str(&self.current_indent());
        self.output.push_str(s);
        self.output.push('\n');
        self
    }

    /// Add debug value.
    pub fn value<T: Debug>(mut self, v: &T) -> Self {
        write!(self.output, "{:?}", v).ok();
        self
    }

    /// Add field.
    pub fn field<T: Debug>(mut self, name: &str, value: &T) -> Self {
        self.output.push_str(&self.current_indent());
        write!(self.output, "{}: {:?}\n", name, value).ok();
        self
    }

    /// Increase indent.
    pub fn indent(mut self) -> Self {
        self.indent += 1;
        self
    }

    /// Decrease indent.
    pub fn dedent(mut self) -> Self {
        self.indent = self.indent.saturating_sub(1);
        self
    }

    /// Add newline.
    pub fn newline(mut self) -> Self {
        self.output.push('\n');
        self
    }

    /// Finish.
    pub fn finish(self) -> String {
        self.output
    }

    fn current_indent(&self) -> String {
        self.indent_str.repeat(self.indent)
    }
}

impl Default for DebugBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Struct debug helper.
pub struct StructDebug<'a> {
    name: &'a str,
    fields: Vec<(&'a str, String)>,
}

impl<'a> StructDebug<'a> {
    /// Create new.
    pub fn new(name: &'a str) -> Self {
        Self {
            name,
            fields: Vec::new(),
        }
    }

    /// Add field.
    pub fn field<T: Debug>(mut self, name: &'a str, value: &T) -> Self {
        self.fields.push((name, format!("{:?}", value)));
        self
    }

    /// Finish.
    pub fn finish(self) -> String {
        if self.fields.is_empty() {
            format!("{} {{}}", self.name)
        } else {
            let fields: Vec<_> = self
                .fields
                .iter()
                .map(|(n, v)| format!("{}: {}", n, v))
                .collect();
            format!("{} {{ {} }}", self.name, fields.join(", "))
        }
    }

    /// Finish pretty.
    pub fn finish_pretty(self) -> String {
        if self.fields.is_empty() {
            format!("{} {{}}", self.name)
        } else {
            let fields: Vec<_> = self
                .fields
                .iter()
                .map(|(n, v)| format!("    {}: {}", n, v))
                .collect();
            format!("{} {{\n{}\n}}", self.name, fields.join(",\n"))
        }
    }
}

/// List debug helper.
pub struct ListDebug {
    items: Vec<String>,
}

impl ListDebug {
    /// Create new.
    pub fn new() -> Self {
        Self { items: Vec::new() }
    }

    /// Add item.
    pub fn item<T: Debug>(mut self, value: &T) -> Self {
        self.items.push(format!("{:?}", value));
        self
    }

    /// Finish.
    pub fn finish(self) -> String {
        format!("[{}]", self.items.join(", "))
    }

    /// Finish pretty.
    pub fn finish_pretty(self) -> String {
        if self.items.is_empty() {
            "[]".to_string()
        } else {
            let items: Vec<_> = self.items.iter().map(|v| format!("    {}", v)).collect();
            format!("[\n{}\n]", items.join(",\n"))
        }
    }
}

impl Default for ListDebug {
    fn default() -> Self {
        Self::new()
    }
}

/// Hex debug formatter.
pub fn debug_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Binary debug formatter.
pub fn debug_binary(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{:08b}", b))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_debug_string() {
        assert_eq!(debug_string(&42), "42");
        assert_eq!(debug_string(&"hello"), "\"hello\"");
    }

    #[test]
    fn test_debug_truncated() {
        let long = "a".repeat(100);
        let truncated = debug_truncated(&long, 20);
        assert!(truncated.len() <= 20);
        assert!(truncated.ends_with("..."));
    }

    #[test]
    fn test_struct_debug() {
        let s = StructDebug::new("Point")
            .field("x", &10)
            .field("y", &20)
            .finish();
        assert!(s.contains("Point"));
        assert!(s.contains("x: 10"));
    }

    #[test]
    fn test_list_debug() {
        let l = ListDebug::new().item(&1).item(&2).item(&3).finish();
        assert_eq!(l, "[1, 2, 3]");
    }

    #[test]
    fn test_debug_hex() {
        assert_eq!(debug_hex(&[0xde, 0xad, 0xbe, 0xef]), "deadbeef");
    }
}
