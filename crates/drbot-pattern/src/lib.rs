//! Pattern utilities for drbot.
//!
//! This crate provides:
//! - Pattern matching helpers
//! - Pattern extraction
//! - Pattern builder

use thiserror::Error;

/// Pattern error types.
#[derive(Error, Debug)]
pub enum PatternError {
    #[error("Invalid pattern")]
    Invalid,

    #[error("Pattern not found")]
    NotFound,

    #[error("Compilation failed: {0}")]
    CompilationFailed(String),
}

/// Result type for pattern operations.
pub type Result<T> = std::result::Result<T, PatternError>;

/// Simple string pattern matcher.
#[derive(Debug, Clone)]
pub struct StringPattern {
    parts: Vec<PatternPart>,
    original: String,
}

#[derive(Debug, Clone)]
enum PatternPart {
    Literal(String),
    Placeholder(String),
}

impl StringPattern {
    /// Create pattern from template string.
    /// Placeholders use `{name}` syntax.
    pub fn new(template: &str) -> Result<Self> {
        let mut parts = Vec::new();
        let mut current = String::new();
        let mut in_placeholder = false;
        let mut placeholder_name = String::new();

        for ch in template.chars() {
            match ch {
                '{' if !in_placeholder => {
                    if !current.is_empty() {
                        parts.push(PatternPart::Literal(current.clone()));
                        current.clear();
                    }
                    in_placeholder = true;
                }
                '}' if in_placeholder => {
                    if placeholder_name.is_empty() {
                        return Err(PatternError::Invalid);
                    }
                    parts.push(PatternPart::Placeholder(placeholder_name.clone()));
                    placeholder_name.clear();
                    in_placeholder = false;
                }
                _ if in_placeholder => {
                    placeholder_name.push(ch);
                }
                _ => {
                    current.push(ch);
                }
            }
        }

        if in_placeholder {
            return Err(PatternError::Invalid);
        }

        if !current.is_empty() {
            parts.push(PatternPart::Literal(current));
        }

        Ok(Self {
            parts,
            original: template.to_string(),
        })
    }

    /// Get original pattern string.
    pub fn pattern(&self) -> &str {
        &self.original
    }

    /// Get placeholder names.
    pub fn placeholders(&self) -> Vec<&str> {
        self.parts
            .iter()
            .filter_map(|p| match p {
                PatternPart::Placeholder(name) => Some(name.as_str()),
                _ => None,
            })
            .collect()
    }

    /// Format pattern with values.
    pub fn format(&self, values: &std::collections::HashMap<String, String>) -> String {
        self.parts
            .iter()
            .map(|part| match part {
                PatternPart::Literal(s) => s.clone(),
                PatternPart::Placeholder(name) => values.get(name).cloned().unwrap_or_default(),
            })
            .collect()
    }

    /// Extract values from string using pattern.
    pub fn extract(&self, input: &str) -> Option<std::collections::HashMap<String, String>> {
        let mut values = std::collections::HashMap::new();
        let mut remaining = input;

        for (i, part) in self.parts.iter().enumerate() {
            match part {
                PatternPart::Literal(lit) => {
                    if !remaining.starts_with(lit) {
                        return None;
                    }
                    remaining = &remaining[lit.len()..];
                }
                PatternPart::Placeholder(name) => {
                    // Find where placeholder ends (at next literal or end)
                    let end_pos = if i + 1 < self.parts.len() {
                        if let PatternPart::Literal(next_lit) = &self.parts[i + 1] {
                            remaining.find(next_lit.as_str()).unwrap_or(remaining.len())
                        } else {
                            remaining.len()
                        }
                    } else {
                        remaining.len()
                    };

                    let value = &remaining[..end_pos];
                    values.insert(name.clone(), value.to_string());
                    remaining = &remaining[end_pos..];
                }
            }
        }

        if remaining.is_empty() {
            Some(values)
        } else {
            None
        }
    }

    /// Check if string matches pattern.
    pub fn matches(&self, input: &str) -> bool {
        self.extract(input).is_some()
    }
}

/// Pattern builder for complex patterns.
#[derive(Debug, Default)]
pub struct PatternBuilder {
    parts: Vec<String>,
}

impl PatternBuilder {
    /// Create new builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add literal text.
    pub fn literal(mut self, text: &str) -> Self {
        self.parts.push(text.to_string());
        self
    }

    /// Add placeholder.
    pub fn placeholder(mut self, name: &str) -> Self {
        self.parts.push(format!("{{{}}}", name));
        self
    }

    /// Build pattern.
    pub fn build(self) -> Result<StringPattern> {
        let template = self.parts.join("");
        StringPattern::new(&template)
    }
}

/// Simple pattern for prefix/suffix matching.
#[derive(Debug, Clone)]
pub struct AffixPattern {
    prefix: Option<String>,
    suffix: Option<String>,
}

impl AffixPattern {
    /// Create prefix pattern.
    pub fn prefix(prefix: &str) -> Self {
        Self {
            prefix: Some(prefix.to_string()),
            suffix: None,
        }
    }

    /// Create suffix pattern.
    pub fn suffix(suffix: &str) -> Self {
        Self {
            prefix: None,
            suffix: Some(suffix.to_string()),
        }
    }

    /// Create prefix and suffix pattern.
    pub fn both(prefix: &str, suffix: &str) -> Self {
        Self {
            prefix: Some(prefix.to_string()),
            suffix: Some(suffix.to_string()),
        }
    }

    /// Check if string matches.
    pub fn matches(&self, input: &str) -> bool {
        let prefix_ok = self
            .prefix
            .as_ref()
            .map(|p| input.starts_with(p))
            .unwrap_or(true);
        let suffix_ok = self
            .suffix
            .as_ref()
            .map(|s| input.ends_with(s))
            .unwrap_or(true);
        prefix_ok && suffix_ok
    }

    /// Extract middle part.
    pub fn extract_middle<'a>(&self, input: &'a str) -> Option<&'a str> {
        if !self.matches(input) {
            return None;
        }

        let start = self.prefix.as_ref().map(|p| p.len()).unwrap_or(0);
        let end = input.len() - self.suffix.as_ref().map(|s| s.len()).unwrap_or(0);

        if start <= end {
            Some(&input[start..end])
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_string_pattern() {
        let pattern = StringPattern::new("Hello, {name}!").unwrap();

        let mut values = HashMap::new();
        values.insert("name".to_string(), "World".to_string());

        assert_eq!(pattern.format(&values), "Hello, World!");
    }

    #[test]
    fn test_pattern_extract() {
        let pattern = StringPattern::new("user/{id}/profile").unwrap();
        let extracted = pattern.extract("user/123/profile").unwrap();

        assert_eq!(extracted.get("id"), Some(&"123".to_string()));
    }

    #[test]
    fn test_pattern_matches() {
        let pattern = StringPattern::new("GET /api/{endpoint}").unwrap();

        assert!(pattern.matches("GET /api/users"));
        assert!(!pattern.matches("POST /api/users"));
    }

    #[test]
    fn test_pattern_builder() {
        let pattern = PatternBuilder::new()
            .literal("prefix_")
            .placeholder("value")
            .literal("_suffix")
            .build()
            .unwrap();

        assert!(pattern.matches("prefix_hello_suffix"));
    }

    #[test]
    fn test_affix_pattern() {
        let pattern = AffixPattern::both("pre_", "_suf");

        assert!(pattern.matches("pre_middle_suf"));
        assert!(!pattern.matches("pre_middle"));

        assert_eq!(pattern.extract_middle("pre_middle_suf"), Some("middle"));
    }

    #[test]
    fn test_placeholders() {
        let pattern = StringPattern::new("{a}/{b}/{c}").unwrap();
        let placeholders = pattern.placeholders();
        assert_eq!(placeholders, vec!["a", "b", "c"]);
    }
}
