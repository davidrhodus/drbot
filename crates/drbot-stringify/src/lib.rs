//! Stringification utilities for drbot.
//!
//! This crate provides:
//! - Value to string conversion
//! - Formatting helpers
//! - Human-readable output

use thiserror::Error;

/// Stringify error types.
#[derive(Error, Debug)]
pub enum StringifyError {
    #[error("Cannot stringify value")]
    CannotStringify,

    #[error("Format error: {0}")]
    FormatError(String),
}

/// Result type for stringify operations.
pub type Result<T> = std::result::Result<T, StringifyError>;

/// Stringify trait for custom types.
pub trait Stringify {
    /// Convert to string representation.
    fn stringify(&self) -> String;

    /// Convert to debug representation.
    fn debug_string(&self) -> String {
        self.stringify()
    }
}

impl Stringify for bool {
    fn stringify(&self) -> String {
        if *self { "true" } else { "false" }.to_string()
    }
}

impl Stringify for i64 {
    fn stringify(&self) -> String {
        self.to_string()
    }
}

impl Stringify for i32 {
    fn stringify(&self) -> String {
        self.to_string()
    }
}

impl Stringify for f64 {
    fn stringify(&self) -> String {
        if self.fract() == 0.0 {
            format!("{:.1}", self)
        } else {
            self.to_string()
        }
    }
}

impl Stringify for String {
    fn stringify(&self) -> String {
        self.clone()
    }
}

impl Stringify for &str {
    fn stringify(&self) -> String {
        (*self).to_string()
    }
}

/// Format bytes as human-readable size.
pub fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    const TB: u64 = GB * 1024;

    if bytes >= TB {
        format!("{:.2} TB", bytes as f64 / TB as f64)
    } else if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// Format duration in milliseconds as human-readable string.
pub fn format_duration_ms(ms: u64) -> String {
    const SECOND: u64 = 1000;
    const MINUTE: u64 = SECOND * 60;
    const HOUR: u64 = MINUTE * 60;
    const DAY: u64 = HOUR * 24;

    if ms >= DAY {
        let days = ms / DAY;
        let hours = (ms % DAY) / HOUR;
        if hours > 0 {
            format!("{}d {}h", days, hours)
        } else {
            format!("{}d", days)
        }
    } else if ms >= HOUR {
        let hours = ms / HOUR;
        let minutes = (ms % HOUR) / MINUTE;
        if minutes > 0 {
            format!("{}h {}m", hours, minutes)
        } else {
            format!("{}h", hours)
        }
    } else if ms >= MINUTE {
        let minutes = ms / MINUTE;
        let seconds = (ms % MINUTE) / SECOND;
        if seconds > 0 {
            format!("{}m {}s", minutes, seconds)
        } else {
            format!("{}m", minutes)
        }
    } else if ms >= SECOND {
        format!("{:.1}s", ms as f64 / SECOND as f64)
    } else {
        format!("{}ms", ms)
    }
}

/// Format number with thousands separator.
pub fn format_number(n: i64, separator: char) -> String {
    let s = n.abs().to_string();
    let negative = n < 0;

    let formatted: String = s
        .chars()
        .rev()
        .enumerate()
        .map(|(i, c)| {
            if i > 0 && i % 3 == 0 {
                format!("{}{}", separator, c)
            } else {
                c.to_string()
            }
        })
        .collect::<String>()
        .chars()
        .rev()
        .collect();

    if negative {
        format!("-{}", formatted)
    } else {
        formatted
    }
}

/// Format percentage.
pub fn format_percent(value: f64, decimals: usize) -> String {
    format!("{:.prec$}%", value * 100.0, prec = decimals)
}

/// Format list as comma-separated string.
pub fn format_list<T: ToString>(items: &[T], separator: &str) -> String {
    items
        .iter()
        .map(|item| item.to_string())
        .collect::<Vec<_>>()
        .join(separator)
}

/// Format list with "and" for last item.
pub fn format_list_natural<T: ToString>(items: &[T]) -> String {
    match items.len() {
        0 => String::new(),
        1 => items[0].to_string(),
        2 => format!("{} and {}", items[0].to_string(), items[1].to_string()),
        _ => {
            let last = items.last().unwrap().to_string();
            let rest: Vec<_> = items[..items.len() - 1]
                .iter()
                .map(|i| i.to_string())
                .collect();
            format!("{}, and {}", rest.join(", "), last)
        }
    }
}

/// Truncate string with ellipsis.
pub fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else if max_len <= 3 {
        s[..max_len].to_string()
    } else {
        format!("{}...", &s[..max_len - 3])
    }
}

/// Pad string to width.
pub fn pad_left(s: &str, width: usize, ch: char) -> String {
    if s.len() >= width {
        s.to_string()
    } else {
        let padding: String = std::iter::repeat(ch).take(width - s.len()).collect();
        format!("{}{}", padding, s)
    }
}

/// Pad string to width (right).
pub fn pad_right(s: &str, width: usize, ch: char) -> String {
    if s.len() >= width {
        s.to_string()
    } else {
        let padding: String = std::iter::repeat(ch).take(width - s.len()).collect();
        format!("{}{}", s, padding)
    }
}

/// Center string in width.
pub fn center(s: &str, width: usize, ch: char) -> String {
    if s.len() >= width {
        s.to_string()
    } else {
        let total_padding = width - s.len();
        let left_padding = total_padding / 2;
        let right_padding = total_padding - left_padding;
        let left: String = std::iter::repeat(ch).take(left_padding).collect();
        let right: String = std::iter::repeat(ch).take(right_padding).collect();
        format!("{}{}{}", left, s, right)
    }
}

/// Quote string.
pub fn quote(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Format key-value pair.
pub fn format_kv(key: &str, value: &str, separator: &str) -> String {
    format!("{}{}{}", key, separator, value)
}

/// Format table row.
pub fn format_row(cells: &[&str], widths: &[usize], separator: &str) -> String {
    cells
        .iter()
        .zip(widths.iter())
        .map(|(cell, &width)| pad_right(cell, width, ' '))
        .collect::<Vec<_>>()
        .join(separator)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(500), "500 B");
        assert_eq!(format_bytes(1024), "1.00 KB");
        assert_eq!(format_bytes(1536), "1.50 KB");
        assert_eq!(format_bytes(1048576), "1.00 MB");
        assert_eq!(format_bytes(1073741824), "1.00 GB");
    }

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration_ms(500), "500ms");
        assert_eq!(format_duration_ms(1500), "1.5s");
        assert_eq!(format_duration_ms(90000), "1m 30s");
        assert_eq!(format_duration_ms(3661000), "1h 1m");
    }

    #[test]
    fn test_format_number() {
        assert_eq!(format_number(1234567, ','), "1,234,567");
        assert_eq!(format_number(-1234567, ','), "-1,234,567");
        assert_eq!(format_number(123, ','), "123");
    }

    #[test]
    fn test_format_percent() {
        assert_eq!(format_percent(0.5, 0), "50%");
        assert_eq!(format_percent(0.123, 1), "12.3%");
    }

    #[test]
    fn test_format_list() {
        assert_eq!(format_list(&["a", "b", "c"], ", "), "a, b, c");
    }

    #[test]
    fn test_format_list_natural() {
        assert_eq!(format_list_natural(&["a"]), "a");
        assert_eq!(format_list_natural(&["a", "b"]), "a and b");
        assert_eq!(format_list_natural(&["a", "b", "c"]), "a, b, and c");
    }

    #[test]
    fn test_truncate() {
        assert_eq!(truncate("hello world", 5), "he...");
        assert_eq!(truncate("hi", 5), "hi");
    }

    #[test]
    fn test_padding() {
        assert_eq!(pad_left("42", 5, '0'), "00042");
        assert_eq!(pad_right("hi", 5, ' '), "hi   ");
        assert_eq!(center("hi", 6, '-'), "--hi--");
    }

    #[test]
    fn test_quote() {
        assert_eq!(quote("hello"), "\"hello\"");
        assert_eq!(quote("say \"hi\""), "\"say \\\"hi\\\"\"");
    }
}
