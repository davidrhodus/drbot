//! Pretty printing utilities for drbot.
//!
//! This crate provides:
//! - Document algebra for pretty printing
//! - Layout algorithms
//! - Table formatting
//! - Tree printing

use std::fmt::Write;
use thiserror::Error;

/// Pretty printer error types.
#[derive(Error, Debug)]
pub enum PrettyError {
    #[error("Format error: {0}")]
    FormatError(String),

    #[error("Width exceeded")]
    WidthExceeded,
}

/// Result type for pretty operations.
pub type Result<T> = std::result::Result<T, PrettyError>;

/// Document element for pretty printing.
#[derive(Debug, Clone)]
pub enum Doc {
    /// Empty document.
    Nil,
    /// Single line text.
    Text(String),
    /// Hard line break.
    Line,
    /// Soft line break (space if fits, newline otherwise).
    SoftLine,
    /// Concatenation.
    Concat(Vec<Doc>),
    /// Nested with indent.
    Nest(i32, Box<Doc>),
    /// Group (try to fit on one line).
    Group(Box<Doc>),
    /// Fill (like concat but with soft breaks).
    Fill(Vec<Doc>),
}

impl Doc {
    /// Empty document.
    pub fn nil() -> Self {
        Doc::Nil
    }

    /// Text document.
    pub fn text(s: impl Into<String>) -> Self {
        Doc::Text(s.into())
    }

    /// Hard line break.
    pub fn line() -> Self {
        Doc::Line
    }

    /// Soft line break.
    pub fn softline() -> Self {
        Doc::SoftLine
    }

    /// Space.
    pub fn space() -> Self {
        Doc::Text(" ".to_string())
    }

    /// Concatenate documents.
    pub fn concat(docs: Vec<Doc>) -> Self {
        Doc::Concat(docs)
    }

    /// Nest document.
    pub fn nest(indent: i32, doc: Doc) -> Self {
        Doc::Nest(indent, Box::new(doc))
    }

    /// Group document.
    pub fn group(doc: Doc) -> Self {
        Doc::Group(Box::new(doc))
    }

    /// Fill with documents.
    pub fn fill(docs: Vec<Doc>) -> Self {
        Doc::Fill(docs)
    }

    /// Append another document.
    pub fn append(self, other: Doc) -> Self {
        match (self, other) {
            (Doc::Nil, d) | (d, Doc::Nil) => d,
            (Doc::Concat(mut a), Doc::Concat(b)) => {
                a.extend(b);
                Doc::Concat(a)
            }
            (Doc::Concat(mut a), d) => {
                a.push(d);
                Doc::Concat(a)
            }
            (d, Doc::Concat(mut b)) => {
                b.insert(0, d);
                Doc::Concat(b)
            }
            (a, b) => Doc::Concat(vec![a, b]),
        }
    }

    /// Join documents with separator.
    pub fn join(docs: Vec<Doc>, sep: Doc) -> Self {
        let mut result = Vec::new();
        for (i, doc) in docs.into_iter().enumerate() {
            if i > 0 {
                result.push(sep.clone());
            }
            result.push(doc);
        }
        Doc::Concat(result)
    }

    /// Surround with prefix and suffix.
    pub fn surround(self, prefix: Doc, suffix: Doc) -> Self {
        Doc::Concat(vec![prefix, self, suffix])
    }

    /// Parenthesize.
    pub fn parens(self) -> Self {
        self.surround(Doc::text("("), Doc::text(")"))
    }

    /// Bracket.
    pub fn brackets(self) -> Self {
        self.surround(Doc::text("["), Doc::text("]"))
    }

    /// Brace.
    pub fn braces(self) -> Self {
        self.surround(Doc::text("{"), Doc::text("}"))
    }
}

/// Layout mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Flat,
    Break,
}

/// Layout state.
struct LayoutState<'a> {
    items: Vec<(i32, Mode, &'a Doc)>,
}

/// Pretty printer.
pub struct Printer {
    width: usize,
    indent_str: String,
}

impl Printer {
    /// Create new printer.
    pub fn new(width: usize) -> Self {
        Self {
            width,
            indent_str: "  ".to_string(),
        }
    }

    /// Set indent string.
    pub fn with_indent(mut self, indent: impl Into<String>) -> Self {
        self.indent_str = indent.into();
        self
    }

    /// Render document to string.
    pub fn render(&self, doc: &Doc) -> String {
        let mut output = String::new();
        let mut state = LayoutState {
            items: vec![(0, Mode::Break, doc)],
        };
        let mut column = 0;

        while let Some((indent, mode, doc)) = state.items.pop() {
            match doc {
                Doc::Nil => {}
                Doc::Text(s) => {
                    output.push_str(s);
                    column += s.len();
                }
                Doc::Line => {
                    output.push('\n');
                    for _ in 0..indent {
                        output.push_str(&self.indent_str);
                    }
                    column = indent as usize * self.indent_str.len();
                }
                Doc::SoftLine => {
                    if mode == Mode::Flat {
                        output.push(' ');
                        column += 1;
                    } else {
                        output.push('\n');
                        for _ in 0..indent {
                            output.push_str(&self.indent_str);
                        }
                        column = indent as usize * self.indent_str.len();
                    }
                }
                Doc::Concat(docs) => {
                    for d in docs.iter().rev() {
                        state.items.push((indent, mode, d));
                    }
                }
                Doc::Nest(i, inner) => {
                    state.items.push((indent + i, mode, inner));
                }
                Doc::Group(inner) => {
                    if self.fits(
                        self.width.saturating_sub(column),
                        &[(indent, Mode::Flat, inner)],
                    ) {
                        state.items.push((indent, Mode::Flat, inner));
                    } else {
                        state.items.push((indent, Mode::Break, inner));
                    }
                }
                Doc::Fill(docs) => {
                    for d in docs.iter().rev() {
                        state.items.push((indent, mode, d));
                    }
                }
            }
        }

        output
    }

    /// Check if document fits in width.
    fn fits(&self, width: usize, items: &[(i32, Mode, &Doc)]) -> bool {
        let mut remaining = width as i32;
        let mut items: Vec<_> = items.iter().cloned().collect();

        while let Some((indent, mode, doc)) = items.pop() {
            if remaining < 0 {
                return false;
            }

            match doc {
                Doc::Nil => {}
                Doc::Text(s) => {
                    remaining -= s.len() as i32;
                }
                Doc::Line => {
                    return true; // Line break fits
                }
                Doc::SoftLine => {
                    if mode == Mode::Flat {
                        remaining -= 1;
                    } else {
                        return true;
                    }
                }
                Doc::Concat(docs) => {
                    for d in docs.iter().rev() {
                        items.push((indent, mode, d));
                    }
                }
                Doc::Nest(i, inner) => {
                    items.push((indent + i, mode, inner));
                }
                Doc::Group(inner) => {
                    items.push((indent, Mode::Flat, inner));
                }
                Doc::Fill(docs) => {
                    for d in docs.iter().rev() {
                        items.push((indent, mode, d));
                    }
                }
            }
        }

        remaining >= 0
    }
}

impl Default for Printer {
    fn default() -> Self {
        Self::new(80)
    }
}

/// Table builder for formatted output.
#[derive(Debug, Clone)]
pub struct Table {
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
    alignments: Vec<Alignment>,
}

/// Column alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Alignment {
    Left,
    Right,
    Center,
}

impl Default for Alignment {
    fn default() -> Self {
        Alignment::Left
    }
}

impl Table {
    /// Create new table.
    pub fn new() -> Self {
        Self {
            headers: Vec::new(),
            rows: Vec::new(),
            alignments: Vec::new(),
        }
    }

    /// Add header.
    pub fn header(mut self, name: impl Into<String>) -> Self {
        self.headers.push(name.into());
        self.alignments.push(Alignment::Left);
        self
    }

    /// Add headers.
    pub fn headers<I, S>(mut self, names: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        for name in names {
            self.headers.push(name.into());
            self.alignments.push(Alignment::Left);
        }
        self
    }

    /// Set alignment for column.
    pub fn align(mut self, col: usize, alignment: Alignment) -> Self {
        if col < self.alignments.len() {
            self.alignments[col] = alignment;
        }
        self
    }

    /// Add row.
    pub fn row<I, S>(mut self, values: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.rows
            .push(values.into_iter().map(|v| v.into()).collect());
        self
    }

    /// Render table.
    pub fn render(&self) -> String {
        let num_cols = self
            .headers
            .len()
            .max(self.rows.iter().map(|r| r.len()).max().unwrap_or(0));

        if num_cols == 0 {
            return String::new();
        }

        // Calculate column widths
        let mut widths = vec![0; num_cols];
        for (i, h) in self.headers.iter().enumerate() {
            widths[i] = widths[i].max(h.len());
        }
        for row in &self.rows {
            for (i, cell) in row.iter().enumerate() {
                if i < widths.len() {
                    widths[i] = widths[i].max(cell.len());
                }
            }
        }

        let mut output = String::new();

        // Header
        if !self.headers.is_empty() {
            output.push_str(&self.format_row(&self.headers, &widths));
            output.push('\n');

            // Separator
            let sep: Vec<_> = widths.iter().map(|w| "-".repeat(*w)).collect();
            output.push_str(&sep.join("-+-"));
            output.push('\n');
        }

        // Rows
        for row in &self.rows {
            output.push_str(&self.format_row(row, &widths));
            output.push('\n');
        }

        output
    }

    fn format_row(&self, cells: &[String], widths: &[usize]) -> String {
        let formatted: Vec<_> = cells
            .iter()
            .enumerate()
            .map(|(i, cell)| {
                let width = widths.get(i).copied().unwrap_or(0);
                let align = self.alignments.get(i).copied().unwrap_or(Alignment::Left);
                self.align_text(cell, width, align)
            })
            .collect();

        formatted.join(" | ")
    }

    fn align_text(&self, text: &str, width: usize, align: Alignment) -> String {
        let padding = width.saturating_sub(text.len());
        match align {
            Alignment::Left => format!("{}{}", text, " ".repeat(padding)),
            Alignment::Right => format!("{}{}", " ".repeat(padding), text),
            Alignment::Center => {
                let left = padding / 2;
                let right = padding - left;
                format!("{}{}{}", " ".repeat(left), text, " ".repeat(right))
            }
        }
    }
}

impl Default for Table {
    fn default() -> Self {
        Self::new()
    }
}

/// Tree printer for hierarchical structures.
pub struct TreePrinter {
    indent: String,
    branch: String,
    last_branch: String,
    vertical: String,
}

impl TreePrinter {
    /// Create new tree printer.
    pub fn new() -> Self {
        Self {
            indent: "    ".to_string(),
            branch: "├── ".to_string(),
            last_branch: "└── ".to_string(),
            vertical: "│   ".to_string(),
        }
    }

    /// Render tree.
    pub fn render<T, F>(&self, root: &T, label: &str, get_children: &F) -> String
    where
        F: Fn(&T) -> Vec<(T, String)>,
        T: Clone,
    {
        let mut output = String::new();
        self.render_node(&mut output, root, label, "", true, get_children);
        output
    }

    fn render_node<T, F>(
        &self,
        output: &mut String,
        node: &T,
        label: &str,
        prefix: &str,
        is_last: bool,
        get_children: &F,
    ) where
        F: Fn(&T) -> Vec<(T, String)>,
        T: Clone,
    {
        let branch = if is_last {
            &self.last_branch
        } else {
            &self.branch
        };
        writeln!(
            output,
            "{}{}{}",
            prefix,
            if prefix.is_empty() { "" } else { branch },
            label
        )
        .ok();

        let children = get_children(node);
        let child_prefix = if prefix.is_empty() {
            String::new()
        } else if is_last {
            format!("{}{}", prefix, &self.indent)
        } else {
            format!("{}{}", prefix, &self.vertical)
        };

        for (i, (child, child_label)) in children.iter().enumerate() {
            let child_is_last = i == children.len() - 1;
            self.render_node(
                output,
                child,
                child_label,
                &child_prefix,
                child_is_last,
                get_children,
            );
        }
    }
}

impl Default for TreePrinter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_doc_text() {
        let doc = Doc::text("hello");
        let printer = Printer::new(80);
        assert_eq!(printer.render(&doc), "hello");
    }

    #[test]
    fn test_doc_concat() {
        let doc = Doc::concat(vec![Doc::text("hello"), Doc::space(), Doc::text("world")]);
        let printer = Printer::new(80);
        assert_eq!(printer.render(&doc), "hello world");
    }

    #[test]
    fn test_doc_nest() {
        let doc = Doc::concat(vec![
            Doc::text("{"),
            Doc::nest(1, Doc::concat(vec![Doc::line(), Doc::text("content")])),
            Doc::line(),
            Doc::text("}"),
        ]);
        let printer = Printer::new(80);
        let result = printer.render(&doc);
        assert!(result.contains("content"));
    }

    #[test]
    fn test_table() {
        let table = Table::new()
            .headers(["Name", "Age"])
            .row(["Alice", "30"])
            .row(["Bob", "25"]);

        let output = table.render();
        assert!(output.contains("Alice"));
        assert!(output.contains("Bob"));
    }

    #[test]
    fn test_table_alignment() {
        let table = Table::new()
            .headers(["Name", "Value"])
            .align(1, Alignment::Right)
            .row(["a", "1"])
            .row(["b", "100"]);

        let output = table.render();
        assert!(output.contains("Name"));
    }
}
