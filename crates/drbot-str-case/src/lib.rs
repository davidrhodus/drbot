//! String case conversion for drbot.
//!
//! This crate provides:
//! - Case conversion utilities
//! - Case detection
//! - Common case formats

use thiserror::Error;

/// Case error types.
#[derive(Error, Debug, Clone)]
pub enum CaseError {
    #[error("Empty string")]
    Empty,

    #[error("Invalid format")]
    InvalidFormat,
}

/// Result type for case operations.
pub type Result<T> = std::result::Result<T, CaseError>;

/// Case formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Case {
    /// lowercase
    Lower,
    /// UPPERCASE
    Upper,
    /// camelCase
    Camel,
    /// PascalCase
    Pascal,
    /// snake_case
    Snake,
    /// SCREAMING_SNAKE_CASE
    ScreamingSnake,
    /// kebab-case
    Kebab,
    /// SCREAMING-KEBAB-CASE
    ScreamingKebab,
    /// Title Case
    Title,
    /// Sentence case
    Sentence,
}

/// Convert to lowercase.
pub fn to_lower(s: &str) -> String {
    s.to_lowercase()
}

/// Convert to uppercase.
pub fn to_upper(s: &str) -> String {
    s.to_uppercase()
}

/// Convert to title case.
pub fn to_title(s: &str) -> String {
    s.split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first
                    .to_uppercase()
                    .chain(chars.flat_map(|c| c.to_lowercase()))
                    .collect(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Convert to sentence case.
pub fn to_sentence(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first
            .to_uppercase()
            .chain(chars.flat_map(|c| c.to_lowercase()))
            .collect(),
        None => String::new(),
    }
}

/// Split string into words (handles various cases).
fn split_words(s: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();

    let chars: Vec<char> = s.chars().collect();
    for i in 0..chars.len() {
        let c = chars[i];

        if c == '_' || c == '-' || c == ' ' {
            if !current.is_empty() {
                words.push(current.clone());
                current.clear();
            }
        } else if c.is_uppercase() {
            // Check if this starts a new word
            let prev_lower = i > 0 && chars[i - 1].is_lowercase();
            let next_lower = i + 1 < chars.len() && chars[i + 1].is_lowercase();

            if prev_lower || (current.len() > 1 && next_lower) {
                if !current.is_empty() {
                    words.push(current.clone());
                    current.clear();
                }
            }
            current.push(c);
        } else {
            current.push(c);
        }
    }

    if !current.is_empty() {
        words.push(current);
    }

    words
}

/// Convert to camelCase.
pub fn to_camel(s: &str) -> String {
    let words = split_words(s);
    let mut result = String::new();

    for (i, word) in words.iter().enumerate() {
        if i == 0 {
            result.push_str(&word.to_lowercase());
        } else {
            let mut chars = word.chars();
            if let Some(first) = chars.next() {
                result.extend(first.to_uppercase());
                result.push_str(&chars.collect::<String>().to_lowercase());
            }
        }
    }

    result
}

/// Convert to PascalCase.
pub fn to_pascal(s: &str) -> String {
    let words = split_words(s);
    let mut result = String::new();

    for word in words {
        let mut chars = word.chars();
        if let Some(first) = chars.next() {
            result.extend(first.to_uppercase());
            result.push_str(&chars.collect::<String>().to_lowercase());
        }
    }

    result
}

/// Convert to snake_case.
pub fn to_snake(s: &str) -> String {
    let words = split_words(s);
    words
        .iter()
        .map(|w| w.to_lowercase())
        .collect::<Vec<_>>()
        .join("_")
}

/// Convert to SCREAMING_SNAKE_CASE.
pub fn to_screaming_snake(s: &str) -> String {
    let words = split_words(s);
    words
        .iter()
        .map(|w| w.to_uppercase())
        .collect::<Vec<_>>()
        .join("_")
}

/// Convert to kebab-case.
pub fn to_kebab(s: &str) -> String {
    let words = split_words(s);
    words
        .iter()
        .map(|w| w.to_lowercase())
        .collect::<Vec<_>>()
        .join("-")
}

/// Convert to SCREAMING-KEBAB-CASE.
pub fn to_screaming_kebab(s: &str) -> String {
    let words = split_words(s);
    words
        .iter()
        .map(|w| w.to_uppercase())
        .collect::<Vec<_>>()
        .join("-")
}

/// Convert to specified case.
pub fn to_case(s: &str, case: Case) -> String {
    match case {
        Case::Lower => to_lower(s),
        Case::Upper => to_upper(s),
        Case::Camel => to_camel(s),
        Case::Pascal => to_pascal(s),
        Case::Snake => to_snake(s),
        Case::ScreamingSnake => to_screaming_snake(s),
        Case::Kebab => to_kebab(s),
        Case::ScreamingKebab => to_screaming_kebab(s),
        Case::Title => to_title(s),
        Case::Sentence => to_sentence(s),
    }
}

/// Detect case of string.
pub fn detect_case(s: &str) -> Option<Case> {
    if s.is_empty() {
        return None;
    }

    let has_upper = s.chars().any(|c| c.is_uppercase());
    let has_lower = s.chars().any(|c| c.is_lowercase());
    let has_underscore = s.contains('_');
    let has_hyphen = s.contains('-');
    let has_space = s.contains(' ');

    if has_space {
        if s.split_whitespace().all(|w| {
            let mut chars = w.chars();
            chars.next().map(|c| c.is_uppercase()).unwrap_or(false)
                && chars.all(|c| c.is_lowercase())
        }) {
            return Some(Case::Title);
        }
    }

    if has_underscore {
        if !has_lower {
            return Some(Case::ScreamingSnake);
        }
        return Some(Case::Snake);
    }

    if has_hyphen {
        if !has_lower {
            return Some(Case::ScreamingKebab);
        }
        return Some(Case::Kebab);
    }

    if !has_upper {
        return Some(Case::Lower);
    }

    if !has_lower {
        return Some(Case::Upper);
    }

    // Check camel vs pascal
    if let Some(first) = s.chars().next() {
        if first.is_uppercase() {
            return Some(Case::Pascal);
        }
        return Some(Case::Camel);
    }

    None
}

/// Capitalize first letter.
pub fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// Uncapitalize first letter.
pub fn uncapitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_lowercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// Swap case.
pub fn swap_case(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_uppercase() {
                c.to_lowercase().next().unwrap_or(c)
            } else if c.is_lowercase() {
                c.to_uppercase().next().unwrap_or(c)
            } else {
                c
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_camel() {
        assert_eq!(to_camel("hello_world"), "helloWorld");
        assert_eq!(to_camel("HelloWorld"), "helloWorld");
        assert_eq!(to_camel("hello-world"), "helloWorld");
    }

    #[test]
    fn test_to_pascal() {
        assert_eq!(to_pascal("hello_world"), "HelloWorld");
        assert_eq!(to_pascal("helloWorld"), "HelloWorld");
    }

    #[test]
    fn test_to_snake() {
        assert_eq!(to_snake("helloWorld"), "hello_world");
        assert_eq!(to_snake("HelloWorld"), "hello_world");
    }

    #[test]
    fn test_to_kebab() {
        assert_eq!(to_kebab("helloWorld"), "hello-world");
        assert_eq!(to_kebab("hello_world"), "hello-world");
    }

    #[test]
    fn test_to_title() {
        assert_eq!(to_title("hello world"), "Hello World");
    }

    #[test]
    fn test_detect_case() {
        assert_eq!(detect_case("helloWorld"), Some(Case::Camel));
        assert_eq!(detect_case("HelloWorld"), Some(Case::Pascal));
        assert_eq!(detect_case("hello_world"), Some(Case::Snake));
        assert_eq!(detect_case("hello-world"), Some(Case::Kebab));
    }

    #[test]
    fn test_swap_case() {
        assert_eq!(swap_case("Hello World"), "hELLO wORLD");
    }
}
