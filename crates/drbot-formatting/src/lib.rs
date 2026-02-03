//! Text and data formatting for drbot.
//!
//! This crate provides:
//! - Number formatting
//! - Date/time formatting
//! - Text formatting
//! - Table formatting

use chrono::{DateTime, Duration, Utc};
use thiserror::Error;

/// Formatting error types.
#[derive(Error, Debug)]
pub enum FormatError {
    #[error("Invalid format: {0}")]
    InvalidFormat(String),

    #[error("Parse error: {0}")]
    ParseError(String),
}

/// Result type for formatting operations.
pub type Result<T> = std::result::Result<T, FormatError>;

/// Number formatter.
pub struct NumberFormatter {
    decimal_places: Option<usize>,
    thousands_separator: Option<char>,
    decimal_separator: char,
}

impl NumberFormatter {
    /// Create new formatter.
    pub fn new() -> Self {
        Self {
            decimal_places: None,
            thousands_separator: None,
            decimal_separator: '.',
        }
    }

    /// Set decimal places.
    pub fn decimal_places(mut self, places: usize) -> Self {
        self.decimal_places = Some(places);
        self
    }

    /// Set thousands separator.
    pub fn thousands_separator(mut self, sep: char) -> Self {
        self.thousands_separator = Some(sep);
        self
    }

    /// Set decimal separator.
    pub fn decimal_separator(mut self, sep: char) -> Self {
        self.decimal_separator = sep;
        self
    }

    /// Format integer.
    pub fn format_int(&self, value: i64) -> String {
        let s = value.abs().to_string();
        let formatted = if let Some(sep) = self.thousands_separator {
            Self::add_thousands(s, sep)
        } else {
            s
        };

        if value < 0 {
            format!("-{}", formatted)
        } else {
            formatted
        }
    }

    /// Format float.
    pub fn format_float(&self, value: f64) -> String {
        let formatted = match self.decimal_places {
            Some(places) => format!("{:.1$}", value.abs(), places),
            None => format!("{}", value.abs()),
        };

        let parts: Vec<&str> = formatted.split('.').collect();
        let integer = if let Some(sep) = self.thousands_separator {
            Self::add_thousands(parts[0].to_string(), sep)
        } else {
            parts[0].to_string()
        };

        let result = if parts.len() > 1 {
            format!("{}{}{}", integer, self.decimal_separator, parts[1])
        } else {
            integer
        };

        if value < 0.0 {
            format!("-{}", result)
        } else {
            result
        }
    }

    fn add_thousands(s: String, sep: char) -> String {
        let bytes: Vec<char> = s.chars().collect();
        let mut result = String::new();

        for (i, c) in bytes.iter().enumerate() {
            if i > 0 && (bytes.len() - i) % 3 == 0 {
                result.push(sep);
            }
            result.push(*c);
        }

        result
    }
}

impl Default for NumberFormatter {
    fn default() -> Self {
        Self::new()
    }
}

/// Byte size formatter.
pub struct ByteFormatter;

impl ByteFormatter {
    const UNITS: &'static [&'static str] = &["B", "KB", "MB", "GB", "TB", "PB"];

    /// Format bytes to human readable string.
    pub fn format(bytes: u64) -> String {
        Self::format_with_precision(bytes, 2)
    }

    /// Format with custom precision.
    pub fn format_with_precision(bytes: u64, precision: usize) -> String {
        if bytes == 0 {
            return "0 B".to_string();
        }

        let mut size = bytes as f64;
        let mut unit_idx = 0;

        while size >= 1024.0 && unit_idx < Self::UNITS.len() - 1 {
            size /= 1024.0;
            unit_idx += 1;
        }

        if unit_idx == 0 {
            format!("{} {}", bytes, Self::UNITS[0])
        } else {
            format!("{:.1$} {2}", size, precision, Self::UNITS[unit_idx])
        }
    }

    /// Parse human readable size to bytes.
    pub fn parse(s: &str) -> Result<u64> {
        let s = s.trim().to_uppercase();
        let (num_str, unit) = s
            .find(|c: char| !c.is_ascii_digit() && c != '.')
            .map(|i| s.split_at(i))
            .unwrap_or((&s, "B"));

        let num: f64 = num_str
            .trim()
            .parse()
            .map_err(|_| FormatError::ParseError("Invalid number".to_string()))?;

        let multiplier = match unit.trim() {
            "" | "B" => 1,
            "KB" | "K" => 1024,
            "MB" | "M" => 1024 * 1024,
            "GB" | "G" => 1024 * 1024 * 1024,
            "TB" | "T" => 1024u64 * 1024 * 1024 * 1024,
            "PB" | "P" => 1024u64 * 1024 * 1024 * 1024 * 1024,
            _ => {
                return Err(FormatError::InvalidFormat(format!(
                    "Unknown unit: {}",
                    unit
                )))
            }
        };

        Ok((num * multiplier as f64) as u64)
    }
}

/// Duration formatter.
pub struct DurationFormatter;

impl DurationFormatter {
    /// Format duration to human readable string.
    pub fn format(duration: Duration) -> String {
        let total_secs = duration.num_seconds().abs();

        if total_secs < 60 {
            format!("{}s", total_secs)
        } else if total_secs < 3600 {
            let mins = total_secs / 60;
            let secs = total_secs % 60;
            if secs > 0 {
                format!("{}m {}s", mins, secs)
            } else {
                format!("{}m", mins)
            }
        } else if total_secs < 86400 {
            let hours = total_secs / 3600;
            let mins = (total_secs % 3600) / 60;
            if mins > 0 {
                format!("{}h {}m", hours, mins)
            } else {
                format!("{}h", hours)
            }
        } else {
            let days = total_secs / 86400;
            let hours = (total_secs % 86400) / 3600;
            if hours > 0 {
                format!("{}d {}h", days, hours)
            } else {
                format!("{}d", days)
            }
        }
    }

    /// Format duration verbose.
    pub fn format_verbose(duration: Duration) -> String {
        let total_secs = duration.num_seconds().abs();

        let days = total_secs / 86400;
        let hours = (total_secs % 86400) / 3600;
        let mins = (total_secs % 3600) / 60;
        let secs = total_secs % 60;

        let mut parts = Vec::new();
        if days > 0 {
            parts.push(format!("{} day{}", days, if days == 1 { "" } else { "s" }));
        }
        if hours > 0 {
            parts.push(format!(
                "{} hour{}",
                hours,
                if hours == 1 { "" } else { "s" }
            ));
        }
        if mins > 0 {
            parts.push(format!(
                "{} minute{}",
                mins,
                if mins == 1 { "" } else { "s" }
            ));
        }
        if secs > 0 || parts.is_empty() {
            parts.push(format!(
                "{} second{}",
                secs,
                if secs == 1 { "" } else { "s" }
            ));
        }

        parts.join(", ")
    }
}

/// Relative time formatter.
pub struct RelativeTime;

impl RelativeTime {
    /// Format as relative time (e.g., "2 hours ago").
    pub fn format(time: DateTime<Utc>) -> String {
        let now = Utc::now();
        let duration = now.signed_duration_since(time);

        let total_secs = duration.num_seconds();

        if total_secs < 0 {
            Self::format_future(Duration::seconds(-total_secs))
        } else {
            Self::format_past(Duration::seconds(total_secs))
        }
    }

    fn format_past(duration: Duration) -> String {
        let secs = duration.num_seconds();

        if secs < 60 {
            "just now".to_string()
        } else if secs < 3600 {
            let mins = secs / 60;
            format!("{} minute{} ago", mins, if mins == 1 { "" } else { "s" })
        } else if secs < 86400 {
            let hours = secs / 3600;
            format!("{} hour{} ago", hours, if hours == 1 { "" } else { "s" })
        } else if secs < 604800 {
            let days = secs / 86400;
            format!("{} day{} ago", days, if days == 1 { "" } else { "s" })
        } else if secs < 2592000 {
            let weeks = secs / 604800;
            format!("{} week{} ago", weeks, if weeks == 1 { "" } else { "s" })
        } else if secs < 31536000 {
            let months = secs / 2592000;
            format!("{} month{} ago", months, if months == 1 { "" } else { "s" })
        } else {
            let years = secs / 31536000;
            format!("{} year{} ago", years, if years == 1 { "" } else { "s" })
        }
    }

    fn format_future(duration: Duration) -> String {
        let secs = duration.num_seconds();

        if secs < 60 {
            "in a moment".to_string()
        } else if secs < 3600 {
            let mins = secs / 60;
            format!("in {} minute{}", mins, if mins == 1 { "" } else { "s" })
        } else if secs < 86400 {
            let hours = secs / 3600;
            format!("in {} hour{}", hours, if hours == 1 { "" } else { "s" })
        } else {
            let days = secs / 86400;
            format!("in {} day{}", days, if days == 1 { "" } else { "s" })
        }
    }
}

/// Text truncator.
pub struct Truncate;

impl Truncate {
    /// Truncate string to max length.
    pub fn chars(s: &str, max_len: usize) -> String {
        if s.chars().count() <= max_len {
            s.to_string()
        } else {
            format!("{}...", s.chars().take(max_len - 3).collect::<String>())
        }
    }

    /// Truncate string to max length with custom suffix.
    pub fn with_suffix(s: &str, max_len: usize, suffix: &str) -> String {
        if s.chars().count() <= max_len {
            s.to_string()
        } else {
            let take = max_len.saturating_sub(suffix.chars().count());
            format!("{}{}", s.chars().take(take).collect::<String>(), suffix)
        }
    }

    /// Truncate from middle.
    pub fn middle(s: &str, max_len: usize) -> String {
        let char_count = s.chars().count();
        if char_count <= max_len {
            s.to_string()
        } else {
            let side_len = (max_len - 3) / 2;
            let chars: Vec<char> = s.chars().collect();
            format!(
                "{}...{}",
                chars[..side_len].iter().collect::<String>(),
                chars[char_count - side_len..].iter().collect::<String>()
            )
        }
    }
}

/// Pluralizer.
pub struct Pluralize;

impl Pluralize {
    /// Simple pluralization.
    pub fn simple(count: i64, singular: &str, plural: &str) -> String {
        if count == 1 || count == -1 {
            format!("{} {}", count, singular)
        } else {
            format!("{} {}", count, plural)
        }
    }

    /// Auto pluralize by adding 's'.
    pub fn auto(count: i64, word: &str) -> String {
        if count == 1 || count == -1 {
            format!("{} {}", count, word)
        } else {
            format!("{} {}s", count, word)
        }
    }
}

/// Simple table formatter.
pub struct TableFormatter {
    columns: Vec<Column>,
    rows: Vec<Vec<String>>,
}

struct Column {
    header: String,
    width: usize,
    align: Alignment,
}

/// Column alignment.
#[derive(Clone, Copy)]
pub enum Alignment {
    Left,
    Right,
    Center,
}

impl TableFormatter {
    /// Create new table formatter.
    pub fn new() -> Self {
        Self {
            columns: Vec::new(),
            rows: Vec::new(),
        }
    }

    /// Add column.
    pub fn column(mut self, header: &str, align: Alignment) -> Self {
        self.columns.push(Column {
            header: header.to_string(),
            width: header.len(),
            align,
        });
        self
    }

    /// Add row.
    pub fn row<S: AsRef<str>>(mut self, cells: &[S]) -> Self {
        let row: Vec<String> = cells.iter().map(|s| s.as_ref().to_string()).collect();
        for (i, cell) in row.iter().enumerate() {
            if i < self.columns.len() {
                self.columns[i].width = self.columns[i].width.max(cell.len());
            }
        }
        self.rows.push(row);
        self
    }

    /// Format table to string.
    pub fn format(&self) -> String {
        let mut lines = Vec::new();

        // Header
        let header: Vec<String> = self
            .columns
            .iter()
            .map(|c| Self::align_cell(&c.header, c.width, c.align))
            .collect();
        lines.push(format!("| {} |", header.join(" | ")));

        // Separator
        let sep: Vec<String> = self.columns.iter().map(|c| "-".repeat(c.width)).collect();
        lines.push(format!("|-{}-|", sep.join("-|-")));

        // Rows
        for row in &self.rows {
            let cells: Vec<String> = self
                .columns
                .iter()
                .enumerate()
                .map(|(i, c)| {
                    let cell = row.get(i).map(|s| s.as_str()).unwrap_or("");
                    Self::align_cell(cell, c.width, c.align)
                })
                .collect();
            lines.push(format!("| {} |", cells.join(" | ")));
        }

        lines.join("\n")
    }

    fn align_cell(s: &str, width: usize, align: Alignment) -> String {
        match align {
            Alignment::Left => format!("{:<width$}", s, width = width),
            Alignment::Right => format!("{:>width$}", s, width = width),
            Alignment::Center => format!("{:^width$}", s, width = width),
        }
    }
}

impl Default for TableFormatter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_number_formatter() {
        let fmt = NumberFormatter::new()
            .thousands_separator(',')
            .decimal_places(2);

        assert_eq!(fmt.format_int(1234567), "1,234,567");
        assert_eq!(fmt.format_float(1234.5678), "1,234.57");
    }

    #[test]
    fn test_byte_formatter() {
        assert_eq!(ByteFormatter::format(0), "0 B");
        assert_eq!(ByteFormatter::format(1024), "1.00 KB");
        assert_eq!(ByteFormatter::format(1024 * 1024), "1.00 MB");
        assert_eq!(ByteFormatter::format(1536 * 1024), "1.50 MB");
    }

    #[test]
    fn test_byte_parse() {
        assert_eq!(ByteFormatter::parse("1KB").unwrap(), 1024);
        assert_eq!(ByteFormatter::parse("1.5 MB").unwrap(), 1572864);
    }

    #[test]
    fn test_duration_formatter() {
        assert_eq!(DurationFormatter::format(Duration::seconds(45)), "45s");
        assert_eq!(DurationFormatter::format(Duration::seconds(90)), "1m 30s");
        assert_eq!(DurationFormatter::format(Duration::seconds(3700)), "1h 1m");
        assert_eq!(DurationFormatter::format(Duration::seconds(90000)), "1d 1h");
    }

    #[test]
    fn test_truncate() {
        assert_eq!(Truncate::chars("hello world", 8), "hello...");
        assert_eq!(Truncate::middle("hello world", 8), "he...ld");
    }

    #[test]
    fn test_pluralize() {
        assert_eq!(Pluralize::auto(1, "item"), "1 item");
        assert_eq!(Pluralize::auto(5, "item"), "5 items");
        assert_eq!(Pluralize::simple(1, "person", "people"), "1 person");
        assert_eq!(Pluralize::simple(2, "person", "people"), "2 people");
    }

    #[test]
    fn test_table_formatter() {
        let table = TableFormatter::new()
            .column("Name", Alignment::Left)
            .column("Age", Alignment::Right)
            .row(&["Alice", "30"])
            .row(&["Bob", "25"]);

        let output = table.format();
        assert!(output.contains("Name"));
        assert!(output.contains("Alice"));
    }
}
