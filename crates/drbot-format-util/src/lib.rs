//! Formatting utilities for drbot.
//!
//! This crate provides:
//! - Number formatting
//! - Size formatting
//! - Duration formatting

use thiserror::Error;

/// Format error types.
#[derive(Error, Debug, Clone)]
pub enum FormatError {
    #[error("Invalid format: {0}")]
    Invalid(String),
}

/// Result type for format operations.
pub type Result<T> = std::result::Result<T, FormatError>;

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

/// Format number with thousands separator.
pub fn format_number(n: i64) -> String {
    let s = n.abs().to_string();
    let mut result = String::new();
    let chars: Vec<char> = s.chars().rev().collect();

    for (i, c) in chars.iter().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(*c);
    }

    if n < 0 {
        result.push('-');
    }

    result.chars().rev().collect()
}

/// Format float with precision.
pub fn format_float(f: f64, precision: usize) -> String {
    format!("{:.prec$}", f, prec = precision)
}

/// Format percentage.
pub fn format_percent(value: f64, precision: usize) -> String {
    format!("{:.prec$}%", value * 100.0, prec = precision)
}

/// Format duration in seconds.
pub fn format_duration_secs(secs: u64) -> String {
    let hours = secs / 3600;
    let mins = (secs % 3600) / 60;
    let secs = secs % 60;

    if hours > 0 {
        format!("{}h {}m {}s", hours, mins, secs)
    } else if mins > 0 {
        format!("{}m {}s", mins, secs)
    } else {
        format!("{}s", secs)
    }
}

/// Format duration in milliseconds.
pub fn format_duration_ms(ms: u64) -> String {
    if ms >= 1000 {
        format_duration_secs(ms / 1000)
    } else {
        format!("{}ms", ms)
    }
}

/// Format ordinal number.
pub fn format_ordinal(n: i64) -> String {
    let suffix = match n.abs() % 100 {
        11 | 12 | 13 => "th",
        _ => match n.abs() % 10 {
            1 => "st",
            2 => "nd",
            3 => "rd",
            _ => "th",
        },
    };
    format!("{}{}", n, suffix)
}

/// Format as hex.
pub fn format_hex(value: u64) -> String {
    format!("0x{:x}", value)
}

/// Format as hex with padding.
pub fn format_hex_padded(value: u64, width: usize) -> String {
    format!("0x{:0>width$x}", value, width = width)
}

/// Format as binary.
pub fn format_binary(value: u64) -> String {
    format!("0b{:b}", value)
}

/// Format as octal.
pub fn format_octal(value: u64) -> String {
    format!("0o{:o}", value)
}

/// Pluralize word.
pub fn pluralize(count: i64, singular: &str, plural: &str) -> String {
    if count.abs() == 1 {
        format!("{} {}", count, singular)
    } else {
        format!("{} {}", count, plural)
    }
}

/// Simple pluralize with 's'.
pub fn pluralize_s(count: i64, word: &str) -> String {
    if count.abs() == 1 {
        format!("{} {}", count, word)
    } else {
        format!("{} {}s", count, word)
    }
}

/// Format list with 'and'.
pub fn format_list(items: &[&str]) -> String {
    match items.len() {
        0 => String::new(),
        1 => items[0].to_string(),
        2 => format!("{} and {}", items[0], items[1]),
        _ => {
            let (last, rest) = items.split_last().unwrap();
            format!("{}, and {}", rest.join(", "), last)
        }
    }
}

/// Format list with 'or'.
pub fn format_list_or(items: &[&str]) -> String {
    match items.len() {
        0 => String::new(),
        1 => items[0].to_string(),
        2 => format!("{} or {}", items[0], items[1]),
        _ => {
            let (last, rest) = items.split_last().unwrap();
            format!("{}, or {}", rest.join(", "), last)
        }
    }
}

/// Truncate string with ellipsis.
pub fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}

/// Pad left.
pub fn pad_left(s: &str, width: usize, pad: char) -> String {
    if s.len() >= width {
        s.to_string()
    } else {
        format!("{}{}", pad.to_string().repeat(width - s.len()), s)
    }
}

/// Pad right.
pub fn pad_right(s: &str, width: usize, pad: char) -> String {
    if s.len() >= width {
        s.to_string()
    } else {
        format!("{}{}", s, pad.to_string().repeat(width - s.len()))
    }
}

/// Center string.
pub fn center(s: &str, width: usize, pad: char) -> String {
    if s.len() >= width {
        s.to_string()
    } else {
        let padding = width - s.len();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(500), "500 B");
        assert_eq!(format_bytes(1024), "1.00 KB");
        assert_eq!(format_bytes(1536), "1.50 KB");
        assert_eq!(format_bytes(1048576), "1.00 MB");
    }

    #[test]
    fn test_format_number() {
        assert_eq!(format_number(1000), "1,000");
        assert_eq!(format_number(1000000), "1,000,000");
        assert_eq!(format_number(-1234), "-1,234");
    }

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration_secs(30), "30s");
        assert_eq!(format_duration_secs(90), "1m 30s");
        assert_eq!(format_duration_secs(3661), "1h 1m 1s");
    }

    #[test]
    fn test_format_ordinal() {
        assert_eq!(format_ordinal(1), "1st");
        assert_eq!(format_ordinal(2), "2nd");
        assert_eq!(format_ordinal(3), "3rd");
        assert_eq!(format_ordinal(4), "4th");
        assert_eq!(format_ordinal(11), "11th");
        assert_eq!(format_ordinal(21), "21st");
    }

    #[test]
    fn test_pluralize() {
        assert_eq!(pluralize_s(1, "item"), "1 item");
        assert_eq!(pluralize_s(2, "item"), "2 items");
    }

    #[test]
    fn test_format_list() {
        assert_eq!(format_list(&["a", "b", "c"]), "a, b, and c");
        assert_eq!(format_list(&["a", "b"]), "a and b");
    }
}
