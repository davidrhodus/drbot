//! Parsing utilities for drbot.
//!
//! This crate provides:
//! - Common parsing functions
//! - Parse result utilities
//! - Parsing helpers

use thiserror::Error;

/// Parse error types.
#[derive(Error, Debug)]
pub enum ParseError {
    #[error("Failed to parse: {0}")]
    Failed(String),

    #[error("Invalid format: {0}")]
    InvalidFormat(String),

    #[error("Empty input")]
    EmptyInput,

    #[error("Unexpected character: {0}")]
    UnexpectedChar(char),

    #[error("Overflow")]
    Overflow,
}

/// Result type for parse operations.
pub type Result<T> = std::result::Result<T, ParseError>;

/// Parse string to bool.
pub fn parse_bool(s: &str) -> Result<bool> {
    match s.trim().to_lowercase().as_str() {
        "true" | "1" | "yes" | "on" | "y" => Ok(true),
        "false" | "0" | "no" | "off" | "n" => Ok(false),
        _ => Err(ParseError::InvalidFormat(format!(
            "'{}' is not a valid boolean",
            s
        ))),
    }
}

/// Parse string to integer with optional radix.
pub fn parse_int(s: &str) -> Result<i64> {
    let s = s.trim();

    if s.is_empty() {
        return Err(ParseError::EmptyInput);
    }

    // Detect radix from prefix
    if s.starts_with("0x") || s.starts_with("0X") {
        i64::from_str_radix(&s[2..], 16)
            .map_err(|_| ParseError::InvalidFormat("invalid hex number".into()))
    } else if s.starts_with("0o") || s.starts_with("0O") {
        i64::from_str_radix(&s[2..], 8)
            .map_err(|_| ParseError::InvalidFormat("invalid octal number".into()))
    } else if s.starts_with("0b") || s.starts_with("0B") {
        i64::from_str_radix(&s[2..], 2)
            .map_err(|_| ParseError::InvalidFormat("invalid binary number".into()))
    } else {
        s.parse()
            .map_err(|_| ParseError::InvalidFormat(format!("'{}' is not a valid integer", s)))
    }
}

/// Parse string to float.
pub fn parse_float(s: &str) -> Result<f64> {
    let s = s.trim();

    if s.is_empty() {
        return Err(ParseError::EmptyInput);
    }

    s.parse()
        .map_err(|_| ParseError::InvalidFormat(format!("'{}' is not a valid float", s)))
}

/// Parse duration string (e.g., "1h30m", "500ms").
pub fn parse_duration_ms(s: &str) -> Result<u64> {
    let s = s.trim();
    let mut total_ms = 0u64;
    let mut current_num = String::new();

    for ch in s.chars() {
        if ch.is_ascii_digit() {
            current_num.push(ch);
        } else {
            if current_num.is_empty() {
                return Err(ParseError::InvalidFormat("missing number".into()));
            }

            let num: u64 = current_num
                .parse()
                .map_err(|_| ParseError::InvalidFormat("invalid number".into()))?;
            current_num.clear();

            let multiplier = match ch {
                'h' | 'H' => 3600000,
                'm' if s.chars().nth(s.find('m').unwrap_or(0) + 1) == Some('s') => continue,
                'm' | 'M' => 60000,
                's' if current_num.is_empty() => 1000,
                's' | 'S' => 1000,
                _ => return Err(ParseError::UnexpectedChar(ch)),
            };

            total_ms += num * multiplier;
        }
    }

    // Handle trailing number (assume milliseconds)
    if !current_num.is_empty() {
        let num: u64 = current_num
            .parse()
            .map_err(|_| ParseError::InvalidFormat("invalid number".into()))?;
        total_ms += num;
    }

    Ok(total_ms)
}

/// Parse size string (e.g., "1KB", "2.5MB").
pub fn parse_size_bytes(s: &str) -> Result<u64> {
    let s = s.trim().to_uppercase();

    let (num_str, unit) = if s.ends_with("GB") {
        (&s[..s.len() - 2], 1024 * 1024 * 1024)
    } else if s.ends_with("MB") {
        (&s[..s.len() - 2], 1024 * 1024)
    } else if s.ends_with("KB") {
        (&s[..s.len() - 2], 1024)
    } else if s.ends_with('B') {
        (&s[..s.len() - 1], 1)
    } else {
        (s.as_str(), 1)
    };

    let num: f64 = num_str
        .trim()
        .parse()
        .map_err(|_| ParseError::InvalidFormat(format!("invalid number: {}", num_str)))?;

    Ok((num * unit as f64) as u64)
}

/// Parse key=value pair.
pub fn parse_kv(s: &str, delimiter: char) -> Result<(String, String)> {
    let parts: Vec<&str> = s.splitn(2, delimiter).collect();

    if parts.len() != 2 {
        return Err(ParseError::InvalidFormat(format!(
            "expected key{}value format",
            delimiter
        )));
    }

    Ok((parts[0].trim().to_string(), parts[1].trim().to_string()))
}

/// Parse comma-separated values.
pub fn parse_csv(s: &str) -> Vec<String> {
    s.split(',')
        .map(|part| part.trim().to_string())
        .filter(|part| !part.is_empty())
        .collect()
}

/// Parse with default on failure.
pub fn parse_or<T, F>(s: &str, default: T, parser: F) -> T
where
    F: FnOnce(&str) -> Result<T>,
{
    parser(s).unwrap_or(default)
}

/// Parse with default on failure using Default trait.
pub fn parse_or_default<T, F>(s: &str, parser: F) -> T
where
    T: Default,
    F: FnOnce(&str) -> Result<T>,
{
    parser(s).unwrap_or_default()
}

/// Try multiple parsers in order.
pub fn try_parse<T>(s: &str, parsers: &[fn(&str) -> Result<T>]) -> Result<T> {
    for parser in parsers {
        if let Ok(result) = parser(s) {
            return Ok(result);
        }
    }
    Err(ParseError::Failed("no parser succeeded".into()))
}

/// Parse or return original string.
pub fn parse_or_string(s: &str) -> ParsedValue {
    if let Ok(b) = parse_bool(s) {
        return ParsedValue::Bool(b);
    }
    if let Ok(i) = parse_int(s) {
        return ParsedValue::Int(i);
    }
    if let Ok(f) = parse_float(s) {
        return ParsedValue::Float(f);
    }
    ParsedValue::String(s.to_string())
}

/// Parsed value enum.
#[derive(Debug, Clone, PartialEq)]
pub enum ParsedValue {
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_bool() {
        assert_eq!(parse_bool("true").unwrap(), true);
        assert_eq!(parse_bool("FALSE").unwrap(), false);
        assert_eq!(parse_bool("yes").unwrap(), true);
        assert_eq!(parse_bool("0").unwrap(), false);
        assert!(parse_bool("invalid").is_err());
    }

    #[test]
    fn test_parse_int() {
        assert_eq!(parse_int("42").unwrap(), 42);
        assert_eq!(parse_int("-123").unwrap(), -123);
        assert_eq!(parse_int("0x1F").unwrap(), 31);
        assert_eq!(parse_int("0o17").unwrap(), 15);
        assert_eq!(parse_int("0b1010").unwrap(), 10);
    }

    #[test]
    fn test_parse_float() {
        assert_eq!(parse_float("3.14").unwrap(), 3.14);
        assert_eq!(parse_float("-2.5").unwrap(), -2.5);
        assert_eq!(parse_float("1e10").unwrap(), 1e10);
    }

    #[test]
    fn test_parse_size() {
        assert_eq!(parse_size_bytes("1KB").unwrap(), 1024);
        assert_eq!(parse_size_bytes("2MB").unwrap(), 2 * 1024 * 1024);
        assert_eq!(parse_size_bytes("100").unwrap(), 100);
    }

    #[test]
    fn test_parse_kv() {
        let (k, v) = parse_kv("key=value", '=').unwrap();
        assert_eq!(k, "key");
        assert_eq!(v, "value");

        let (k, v) = parse_kv("name: John", ':').unwrap();
        assert_eq!(k, "name");
        assert_eq!(v, "John");
    }

    #[test]
    fn test_parse_csv() {
        let values = parse_csv("a, b, c");
        assert_eq!(values, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_parse_or_string() {
        assert_eq!(parse_or_string("true"), ParsedValue::Bool(true));
        assert_eq!(parse_or_string("42"), ParsedValue::Int(42));
        assert_eq!(parse_or_string("3.14"), ParsedValue::Float(3.14));
        assert_eq!(
            parse_or_string("hello"),
            ParsedValue::String("hello".to_string())
        );
    }
}
