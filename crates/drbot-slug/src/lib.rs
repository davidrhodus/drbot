//! Slug generation for drbot.
//!
//! This crate provides:
//! - URL-safe slug generation
//! - Customizable slug formats
//! - Slug validation

use regex::Regex;
use thiserror::Error;

/// Slug error types.
#[derive(Error, Debug)]
pub enum SlugError {
    #[error("Empty input")]
    EmptyInput,

    #[error("Invalid slug: {0}")]
    InvalidSlug(String),
}

/// Result type for slug operations.
pub type Result<T> = std::result::Result<T, SlugError>;

/// Slug generator configuration.
#[derive(Debug, Clone)]
pub struct SlugConfig {
    /// Separator character.
    pub separator: char,
    /// Maximum length (0 for unlimited).
    pub max_length: usize,
    /// Convert to lowercase.
    pub lowercase: bool,
    /// Remove numbers.
    pub remove_numbers: bool,
    /// Custom replacements.
    pub replacements: Vec<(String, String)>,
}

impl Default for SlugConfig {
    fn default() -> Self {
        Self {
            separator: '-',
            max_length: 0,
            lowercase: true,
            remove_numbers: false,
            replacements: Vec::new(),
        }
    }
}

impl SlugConfig {
    /// Create new config.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set separator.
    pub fn separator(mut self, sep: char) -> Self {
        self.separator = sep;
        self
    }

    /// Set max length.
    pub fn max_length(mut self, len: usize) -> Self {
        self.max_length = len;
        self
    }

    /// Set lowercase.
    pub fn lowercase(mut self, lowercase: bool) -> Self {
        self.lowercase = lowercase;
        self
    }

    /// Set remove numbers.
    pub fn remove_numbers(mut self, remove: bool) -> Self {
        self.remove_numbers = remove;
        self
    }

    /// Add replacement.
    pub fn replace(mut self, from: impl Into<String>, to: impl Into<String>) -> Self {
        self.replacements.push((from.into(), to.into()));
        self
    }
}

/// Slug generator.
pub struct Slugify {
    config: SlugConfig,
}

impl Slugify {
    /// Create with default config.
    pub fn new() -> Self {
        Self {
            config: SlugConfig::default(),
        }
    }

    /// Create with custom config.
    pub fn with_config(config: SlugConfig) -> Self {
        Self { config }
    }

    /// Generate slug from string.
    pub fn slugify(&self, input: &str) -> Result<String> {
        if input.trim().is_empty() {
            return Err(SlugError::EmptyInput);
        }

        let mut result = input.to_string();
        let sep = self.config.separator;

        // Apply custom replacements
        for (from, to) in &self.config.replacements {
            let replacement = if to.is_empty() || from.chars().all(|c| c.is_alphanumeric()) {
                to.clone()
            } else {
                format!("{}{}{}", sep, to, sep)
            };

            result = result.replace(from, &replacement);
        }

        // Convert to lowercase if configured
        if self.config.lowercase {
            result = result.to_lowercase();
        }

        // Normalize unicode to ASCII equivalents
        result = self.normalize_unicode(&result);

        // Remove numbers if configured
        if self.config.remove_numbers {
            result = result.chars().filter(|c| !c.is_ascii_digit()).collect();
        }

        // Replace non-alphanumeric with separator
        result = result
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { sep })
            .collect();

        // Collapse multiple separators
        let sep_str = sep.to_string();
        let double_sep = format!("{}{}", sep, sep);
        while result.contains(&double_sep) {
            result = result.replace(&double_sep, &sep_str);
        }

        // Trim separators from start and end
        result = result.trim_matches(sep).to_string();

        // Apply max length
        if self.config.max_length > 0 && result.len() > self.config.max_length {
            result = result[..self.config.max_length]
                .trim_end_matches(sep)
                .to_string();
        }

        if result.is_empty() {
            return Err(SlugError::EmptyInput);
        }

        Ok(result)
    }

    fn normalize_unicode(&self, s: &str) -> String {
        let replacements = [
            ('á', 'a'),
            ('à', 'a'),
            ('ä', 'a'),
            ('â', 'a'),
            ('ã', 'a'),
            ('é', 'e'),
            ('è', 'e'),
            ('ë', 'e'),
            ('ê', 'e'),
            ('í', 'i'),
            ('ì', 'i'),
            ('ï', 'i'),
            ('î', 'i'),
            ('ó', 'o'),
            ('ò', 'o'),
            ('ö', 'o'),
            ('ô', 'o'),
            ('õ', 'o'),
            ('ú', 'u'),
            ('ù', 'u'),
            ('ü', 'u'),
            ('û', 'u'),
            ('ñ', 'n'),
            ('ç', 'c'),
            ('ß', 's'),
            ('Á', 'A'),
            ('À', 'A'),
            ('Ä', 'A'),
            ('Â', 'A'),
            ('Ã', 'A'),
            ('É', 'E'),
            ('È', 'E'),
            ('Ë', 'E'),
            ('Ê', 'E'),
            ('Í', 'I'),
            ('Ì', 'I'),
            ('Ï', 'I'),
            ('Î', 'I'),
            ('Ó', 'O'),
            ('Ò', 'O'),
            ('Ö', 'O'),
            ('Ô', 'O'),
            ('Õ', 'O'),
            ('Ú', 'U'),
            ('Ù', 'U'),
            ('Ü', 'U'),
            ('Û', 'U'),
            ('Ñ', 'N'),
            ('Ç', 'C'),
        ];

        let mut result = s.to_string();
        for (from, to) in replacements {
            result = result.replace(from, &to.to_string());
        }
        result
    }
}

impl Default for Slugify {
    fn default() -> Self {
        Self::new()
    }
}

/// Quick slug function with default settings.
pub fn slugify(input: &str) -> Result<String> {
    Slugify::new().slugify(input)
}

/// Quick slug function with separator.
pub fn slugify_with(input: &str, separator: char) -> Result<String> {
    Slugify::with_config(SlugConfig::new().separator(separator)).slugify(input)
}

/// Slug validator.
pub struct SlugValidator {
    pattern: Regex,
    min_length: usize,
    max_length: usize,
}

impl SlugValidator {
    /// Create new validator.
    pub fn new() -> Self {
        Self {
            pattern: Regex::new(r"^[a-z0-9]+(?:-[a-z0-9]+)*$").unwrap(),
            min_length: 1,
            max_length: 255,
        }
    }

    /// Set min length.
    pub fn min_length(mut self, len: usize) -> Self {
        self.min_length = len;
        self
    }

    /// Set max length.
    pub fn max_length(mut self, len: usize) -> Self {
        self.max_length = len;
        self
    }

    /// Set custom pattern.
    pub fn pattern(mut self, pattern: &str) -> Result<Self> {
        self.pattern = Regex::new(pattern).map_err(|e| SlugError::InvalidSlug(e.to_string()))?;
        Ok(self)
    }

    /// Validate slug.
    pub fn validate(&self, slug: &str) -> Result<()> {
        if slug.len() < self.min_length {
            return Err(SlugError::InvalidSlug(format!(
                "Slug too short: {} < {}",
                slug.len(),
                self.min_length
            )));
        }

        if slug.len() > self.max_length {
            return Err(SlugError::InvalidSlug(format!(
                "Slug too long: {} > {}",
                slug.len(),
                self.max_length
            )));
        }

        if !self.pattern.is_match(slug) {
            return Err(SlugError::InvalidSlug(
                "Slug does not match pattern".to_string(),
            ));
        }

        Ok(())
    }

    /// Check if valid.
    pub fn is_valid(&self, slug: &str) -> bool {
        self.validate(slug).is_ok()
    }
}

impl Default for SlugValidator {
    fn default() -> Self {
        Self::new()
    }
}

/// Unique slug generator.
pub struct UniqueSlug<F> {
    slugify: Slugify,
    exists_fn: F,
    max_attempts: usize,
}

impl<F> UniqueSlug<F>
where
    F: Fn(&str) -> bool,
{
    /// Create new unique slug generator.
    pub fn new(exists_fn: F) -> Self {
        Self {
            slugify: Slugify::new(),
            exists_fn,
            max_attempts: 100,
        }
    }

    /// Set max attempts.
    pub fn max_attempts(mut self, attempts: usize) -> Self {
        self.max_attempts = attempts;
        self
    }

    /// Generate unique slug.
    pub fn generate(&self, input: &str) -> Result<String> {
        let base = self.slugify.slugify(input)?;

        if !(self.exists_fn)(&base) {
            return Ok(base);
        }

        for i in 1..=self.max_attempts {
            let candidate = format!("{}-{}", base, i);
            if !(self.exists_fn)(&candidate) {
                return Ok(candidate);
            }
        }

        // Use random suffix
        let random: u32 = std::collections::hash_map::RandomState::new()
            .build_hasher()
            .finish() as u32;
        Ok(format!("{}-{:x}", base, random))
    }
}

use std::hash::{BuildHasher, Hasher};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_slug() {
        assert_eq!(slugify("Hello World").unwrap(), "hello-world");
        assert_eq!(slugify("  Spaces  ").unwrap(), "spaces");
        assert_eq!(slugify("Multiple   Spaces").unwrap(), "multiple-spaces");
    }

    #[test]
    fn test_special_characters() {
        assert_eq!(slugify("Hello & World").unwrap(), "hello-world");
        assert_eq!(slugify("Price: $100").unwrap(), "price-100");
        assert_eq!(slugify("Test!@#$%").unwrap(), "test");
    }

    #[test]
    fn test_unicode() {
        assert_eq!(slugify("Café").unwrap(), "cafe");
        assert_eq!(slugify("naïve").unwrap(), "naive");
        assert_eq!(slugify("Zürich").unwrap(), "zurich");
    }

    #[test]
    fn test_custom_separator() {
        assert_eq!(slugify_with("Hello World", '_').unwrap(), "hello_world");
    }

    #[test]
    fn test_max_length() {
        let config = SlugConfig::new().max_length(10);
        let slugify = Slugify::with_config(config);
        let slug = slugify.slugify("This is a very long title").unwrap();
        assert!(slug.len() <= 10);
    }

    #[test]
    fn test_empty_input() {
        assert!(slugify("").is_err());
        assert!(slugify("   ").is_err());
    }

    #[test]
    fn test_validator() {
        let validator = SlugValidator::new();

        assert!(validator.is_valid("hello-world"));
        assert!(validator.is_valid("test123"));
        assert!(!validator.is_valid("Hello-World")); // Uppercase
        assert!(!validator.is_valid("hello--world")); // Double hyphen
        assert!(!validator.is_valid("-hello")); // Leading hyphen
    }

    #[test]
    fn test_validator_length() {
        let validator = SlugValidator::new().min_length(5).max_length(10);

        assert!(!validator.is_valid("hi"));
        assert!(validator.is_valid("hello"));
        assert!(!validator.is_valid("hello-world-test"));
    }

    #[test]
    fn test_unique_slug() {
        let existing = vec!["hello", "hello-1", "hello-2"];
        let generator = UniqueSlug::new(|slug| existing.contains(&slug));

        let slug = generator.generate("Hello").unwrap();
        assert_eq!(slug, "hello-3");
    }

    #[test]
    fn test_custom_replacements() {
        let config = SlugConfig::new().replace("&", "and").replace("@", "at");
        let slugify = Slugify::with_config(config);

        assert_eq!(slugify.slugify("Tom & Jerry").unwrap(), "tom-and-jerry");
        assert_eq!(slugify.slugify("email@test").unwrap(), "email-at-test");
    }
}
