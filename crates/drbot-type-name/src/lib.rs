//! Type name utilities for drbot.
//!
//! This crate provides:
//! - Type name extraction
//! - Name formatting
//! - Generic parameter handling

use thiserror::Error;

/// Type name error types.
#[derive(Error, Debug, Clone)]
pub enum TypeNameError {
    #[error("Invalid type name: {0}")]
    Invalid(String),
}

/// Result type for type name operations.
pub type Result<T> = std::result::Result<T, TypeNameError>;

/// Get full type name.
pub fn full_name<T: 'static>() -> &'static str {
    std::any::type_name::<T>()
}

/// Get short type name (last component).
pub fn short_name<T: 'static>() -> &'static str {
    let full = std::any::type_name::<T>();
    extract_short_name(full)
}

/// Extract short name from full name.
pub fn extract_short_name(full: &str) -> &str {
    // Handle generic types
    if let Some(bracket_pos) = full.find('<') {
        let base = &full[..bracket_pos];
        base.rsplit("::").next().unwrap_or(base)
    } else {
        full.rsplit("::").next().unwrap_or(full)
    }
}

/// Get module path.
pub fn module_path<T: 'static>() -> Option<&'static str> {
    let full = std::any::type_name::<T>();
    extract_module_path(full)
}

/// Extract module path from full name.
pub fn extract_module_path(full: &str) -> Option<&str> {
    // Handle generic types
    let base = if let Some(bracket_pos) = full.find('<') {
        &full[..bracket_pos]
    } else {
        full
    };

    if let Some(last_sep) = base.rfind("::") {
        Some(&base[..last_sep])
    } else {
        None
    }
}

/// Parsed type name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeName {
    pub full: String,
    pub short: String,
    pub module: Option<String>,
    pub generics: Vec<TypeName>,
}

impl TypeName {
    /// Parse from full name.
    pub fn parse(full: &str) -> Self {
        let (base, generics) = parse_generics(full);

        let short = extract_short_name(base).to_string();
        let module = extract_module_path(base).map(|s| s.to_string());

        Self {
            full: full.to_string(),
            short,
            module,
            generics,
        }
    }

    /// Create for type.
    pub fn of<T: 'static>() -> Self {
        Self::parse(std::any::type_name::<T>())
    }

    /// Check if generic.
    pub fn is_generic(&self) -> bool {
        !self.generics.is_empty()
    }

    /// Format with custom options.
    pub fn format(&self, include_module: bool, include_generics: bool) -> String {
        let mut result = String::new();

        if include_module {
            if let Some(module) = &self.module {
                result.push_str(module);
                result.push_str("::");
            }
        }

        result.push_str(&self.short);

        if include_generics && !self.generics.is_empty() {
            result.push('<');
            for (i, generic) in self.generics.iter().enumerate() {
                if i > 0 {
                    result.push_str(", ");
                }
                result.push_str(&generic.format(include_module, true));
            }
            result.push('>');
        }

        result
    }
}

/// Parse generic parameters.
fn parse_generics(full: &str) -> (&str, Vec<TypeName>) {
    if let Some(start) = full.find('<') {
        if let Some(end) = full.rfind('>') {
            let base = &full[..start];
            let generics_str = &full[start + 1..end];
            let generics = split_generics(generics_str)
                .into_iter()
                .map(|s| TypeName::parse(s.trim()))
                .collect();
            return (base, generics);
        }
    }
    (full, Vec::new())
}

/// Split generic parameters handling nested brackets.
fn split_generics(s: &str) -> Vec<&str> {
    let mut result = Vec::new();
    let mut start = 0;
    let mut depth = 0;

    for (i, c) in s.char_indices() {
        match c {
            '<' => depth += 1,
            '>' => depth -= 1,
            ',' if depth == 0 => {
                result.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }

    if start < s.len() {
        result.push(&s[start..]);
    }

    result
}

/// Format type name for display.
pub fn display_name<T: 'static>() -> String {
    TypeName::of::<T>().format(false, true)
}

/// Format type name for debugging.
pub fn debug_name<T: 'static>() -> String {
    TypeName::of::<T>().format(true, true)
}

/// Check if type name matches pattern.
pub fn matches_pattern(type_name: &str, pattern: &str) -> bool {
    if pattern.ends_with('*') {
        type_name.starts_with(&pattern[..pattern.len() - 1])
    } else if pattern.starts_with('*') {
        type_name.ends_with(&pattern[1..])
    } else {
        type_name == pattern || type_name.ends_with(&format!("::{}", pattern))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_full_name() {
        let name = full_name::<Vec<String>>();
        assert!(name.contains("Vec"));
        assert!(name.contains("String"));
    }

    #[test]
    fn test_short_name() {
        let name = short_name::<Vec<String>>();
        assert_eq!(name, "Vec");
    }

    #[test]
    fn test_extract_short_name() {
        assert_eq!(extract_short_name("std::vec::Vec<i32>"), "Vec");
        assert_eq!(extract_short_name("MyType"), "MyType");
        assert_eq!(extract_short_name("foo::bar::Baz"), "Baz");
    }

    #[test]
    fn test_extract_module_path() {
        assert_eq!(extract_module_path("std::vec::Vec<i32>"), Some("std::vec"));
        assert_eq!(extract_module_path("MyType"), None);
    }

    #[test]
    fn test_type_name_parse() {
        let name = TypeName::parse("std::vec::Vec<std::string::String>");

        assert_eq!(name.short, "Vec");
        assert_eq!(name.module, Some("std::vec".to_string()));
        assert!(name.is_generic());
        assert_eq!(name.generics.len(), 1);
        assert_eq!(name.generics[0].short, "String");
    }

    #[test]
    fn test_type_name_format() {
        let name = TypeName::parse("std::vec::Vec<i32>");

        assert_eq!(name.format(false, false), "Vec");
        assert_eq!(name.format(false, true), "Vec<i32>");
        assert_eq!(name.format(true, true), "std::vec::Vec<i32>");
    }

    #[test]
    fn test_matches_pattern() {
        assert!(matches_pattern("std::vec::Vec", "Vec"));
        assert!(matches_pattern("std::vec::Vec", "std::vec::*"));
        assert!(matches_pattern("MyType", "*Type"));
        assert!(!matches_pattern("MyType", "Other"));
    }

    #[test]
    fn test_split_generics() {
        let parts = split_generics("A, B<C, D>, E");
        assert_eq!(parts, vec!["A", " B<C, D>", " E"]);
    }
}
