//! Default configuration handling for drbot.
//!
//! This crate provides:
//! - Default value handling
//! - Fallback chains
//! - Optional value utilities

use std::collections::HashMap;
use thiserror::Error;

/// Default config error types.
#[derive(Error, Debug, Clone)]
pub enum DefaultError {
    #[error("No default available for: {0}")]
    NoDefault(String),

    #[error("Invalid default: {0}")]
    Invalid(String),
}

/// Result type for default operations.
pub type Result<T> = std::result::Result<T, DefaultError>;

/// Default value provider trait.
pub trait DefaultProvider<T> {
    /// Get default value.
    fn default_value(&self) -> T;
}

/// Static default provider.
pub struct StaticDefault<T: Clone> {
    value: T,
}

impl<T: Clone> StaticDefault<T> {
    pub fn new(value: T) -> Self {
        Self { value }
    }
}

impl<T: Clone> DefaultProvider<T> for StaticDefault<T> {
    fn default_value(&self) -> T {
        self.value.clone()
    }
}

/// Lazy default provider.
pub struct LazyDefault<T, F: Fn() -> T> {
    factory: F,
}

impl<T, F: Fn() -> T> LazyDefault<T, F> {
    pub fn new(factory: F) -> Self {
        Self { factory }
    }
}

impl<T, F: Fn() -> T> DefaultProvider<T> for LazyDefault<T, F> {
    fn default_value(&self) -> T {
        (self.factory)()
    }
}

/// Optional value with default.
#[derive(Debug, Clone)]
pub struct WithDefault<T> {
    value: Option<T>,
    default: T,
}

impl<T: Clone> WithDefault<T> {
    /// Create new with default.
    pub fn new(default: T) -> Self {
        Self {
            value: None,
            default,
        }
    }

    /// Create with value.
    pub fn with_value(value: T, default: T) -> Self {
        Self {
            value: Some(value),
            default,
        }
    }

    /// Set value.
    pub fn set(&mut self, value: T) {
        self.value = Some(value);
    }

    /// Clear value (use default).
    pub fn clear(&mut self) {
        self.value = None;
    }

    /// Get value or default.
    pub fn get(&self) -> T {
        self.value.clone().unwrap_or_else(|| self.default.clone())
    }

    /// Check if using default.
    pub fn is_default(&self) -> bool {
        self.value.is_none()
    }

    /// Get the explicit value if set.
    pub fn explicit(&self) -> Option<&T> {
        self.value.as_ref()
    }

    /// Get the default value.
    pub fn default(&self) -> &T {
        &self.default
    }
}

/// Fallback chain for values.
pub struct FallbackChain<T> {
    sources: Vec<Box<dyn Fn() -> Option<T>>>,
    default: Option<T>,
}

impl<T: Clone + 'static> FallbackChain<T> {
    /// Create new fallback chain.
    pub fn new() -> Self {
        Self {
            sources: Vec::new(),
            default: None,
        }
    }

    /// Add source.
    pub fn source<F: Fn() -> Option<T> + 'static>(mut self, f: F) -> Self {
        self.sources.push(Box::new(f));
        self
    }

    /// Set default.
    pub fn default(mut self, value: T) -> Self {
        self.default = Some(value);
        self
    }

    /// Get value from first available source.
    pub fn get(&self) -> Option<T> {
        for source in &self.sources {
            if let Some(value) = source() {
                return Some(value);
            }
        }
        self.default.clone()
    }

    /// Get value or error.
    pub fn get_or_error(&self, name: &str) -> Result<T> {
        self.get()
            .ok_or_else(|| DefaultError::NoDefault(name.to_string()))
    }
}

impl<T: Clone + 'static> Default for FallbackChain<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Default registry for named defaults.
pub struct DefaultRegistry<T> {
    defaults: HashMap<String, T>,
}

impl<T: Clone> DefaultRegistry<T> {
    /// Create new registry.
    pub fn new() -> Self {
        Self {
            defaults: HashMap::new(),
        }
    }

    /// Register default.
    pub fn register<K: Into<String>>(&mut self, key: K, value: T) {
        self.defaults.insert(key.into(), value);
    }

    /// Get default.
    pub fn get(&self, key: &str) -> Option<T> {
        self.defaults.get(key).cloned()
    }

    /// Get default or error.
    pub fn get_or_error(&self, key: &str) -> Result<T> {
        self.get(key)
            .ok_or_else(|| DefaultError::NoDefault(key.to_string()))
    }

    /// Check if default exists.
    pub fn has(&self, key: &str) -> bool {
        self.defaults.contains_key(key)
    }

    /// Get all keys.
    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.defaults.keys()
    }
}

impl<T: Clone> Default for DefaultRegistry<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Get value or default.
pub fn get_or_default<T>(value: Option<T>, default: T) -> T {
    value.unwrap_or(default)
}

/// Get value or compute default.
pub fn get_or_else<T, F: FnOnce() -> T>(value: Option<T>, f: F) -> T {
    value.unwrap_or_else(f)
}

/// Coalesce - get first Some value.
pub fn coalesce<T>(values: Vec<Option<T>>) -> Option<T> {
    values.into_iter().flatten().next()
}

/// Config with defaults.
#[derive(Debug, Clone)]
pub struct ConfigWithDefaults {
    values: HashMap<String, String>,
    defaults: HashMap<String, String>,
}

impl ConfigWithDefaults {
    /// Create new config.
    pub fn new() -> Self {
        Self {
            values: HashMap::new(),
            defaults: HashMap::new(),
        }
    }

    /// Set default.
    pub fn set_default<K: Into<String>, V: Into<String>>(&mut self, key: K, value: V) {
        self.defaults.insert(key.into(), value.into());
    }

    /// Set value.
    pub fn set<K: Into<String>, V: Into<String>>(&mut self, key: K, value: V) {
        self.values.insert(key.into(), value.into());
    }

    /// Get value.
    pub fn get(&self, key: &str) -> Option<&String> {
        self.values.get(key).or_else(|| self.defaults.get(key))
    }

    /// Get value or custom default.
    pub fn get_or<'a>(&'a self, key: &str, default: &'a str) -> &'a str {
        self.get(key).map(|s| s.as_str()).unwrap_or(default)
    }

    /// Check if using default.
    pub fn is_default(&self, key: &str) -> bool {
        !self.values.contains_key(key) && self.defaults.contains_key(key)
    }

    /// Get all effective values.
    pub fn effective(&self) -> HashMap<String, String> {
        let mut result = self.defaults.clone();
        for (k, v) in &self.values {
            result.insert(k.clone(), v.clone());
        }
        result
    }
}

impl Default for ConfigWithDefaults {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_with_default() {
        let mut wd = WithDefault::new(42);
        assert_eq!(wd.get(), 42);
        assert!(wd.is_default());

        wd.set(100);
        assert_eq!(wd.get(), 100);
        assert!(!wd.is_default());

        wd.clear();
        assert_eq!(wd.get(), 42);
    }

    #[test]
    fn test_fallback_chain() {
        let chain = FallbackChain::new()
            .source(|| None::<i32>)
            .source(|| Some(42))
            .source(|| Some(100))
            .default(0);

        assert_eq!(chain.get(), Some(42)); // First Some value
    }

    #[test]
    fn test_default_registry() {
        let mut registry = DefaultRegistry::new();
        registry.register("port", 8080);
        registry.register("timeout", 30);

        assert_eq!(registry.get("port"), Some(8080));
        assert_eq!(registry.get("unknown"), None);
    }

    #[test]
    fn test_coalesce() {
        assert_eq!(coalesce(vec![None, Some(1), Some(2)]), Some(1));
        assert_eq!(coalesce(vec![None, None]), None::<i32>);
    }

    #[test]
    fn test_config_with_defaults() {
        let mut config = ConfigWithDefaults::new();
        config.set_default("port", "8080");
        config.set_default("host", "localhost");
        config.set("port", "9000");

        assert_eq!(config.get("port"), Some(&"9000".to_string()));
        assert_eq!(config.get("host"), Some(&"localhost".to_string()));
        assert!(!config.is_default("port"));
        assert!(config.is_default("host"));
    }
}
