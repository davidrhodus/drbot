//! Environment-based configuration for drbot.
//!
//! This crate provides:
//! - Environment variable reading
//! - Prefix-based config
//! - Type-safe env parsing

use std::collections::HashMap;
use std::env;
use thiserror::Error;

/// Env config error types.
#[derive(Error, Debug, Clone)]
pub enum EnvConfigError {
    #[error("Missing environment variable: {0}")]
    Missing(String),

    #[error("Invalid value for {var}: {message}")]
    Invalid { var: String, message: String },

    #[error("Parse error for {var}: {message}")]
    ParseError { var: String, message: String },
}

/// Result type for env config operations.
pub type Result<T> = std::result::Result<T, EnvConfigError>;

/// Environment configuration reader.
#[derive(Debug, Clone)]
pub struct EnvConfig {
    prefix: Option<String>,
    separator: String,
}

impl EnvConfig {
    /// Create new env config reader.
    pub fn new() -> Self {
        Self {
            prefix: None,
            separator: "_".to_string(),
        }
    }

    /// Set prefix.
    pub fn prefix<S: Into<String>>(mut self, prefix: S) -> Self {
        self.prefix = Some(prefix.into());
        self
    }

    /// Set separator.
    pub fn separator<S: Into<String>>(mut self, sep: S) -> Self {
        self.separator = sep.into();
        self
    }

    /// Build full variable name.
    fn var_name(&self, key: &str) -> String {
        match &self.prefix {
            Some(prefix) => format!("{}{}{}", prefix, self.separator, key),
            None => key.to_string(),
        }
    }

    /// Get raw string value.
    pub fn get(&self, key: &str) -> Option<String> {
        let name = self.var_name(key);
        env::var(&name).ok()
    }

    /// Get required string value.
    pub fn get_required(&self, key: &str) -> Result<String> {
        let name = self.var_name(key);
        env::var(&name).map_err(|_| EnvConfigError::Missing(name))
    }

    /// Get with default.
    pub fn get_or<S: Into<String>>(&self, key: &str, default: S) -> String {
        self.get(key).unwrap_or_else(|| default.into())
    }

    /// Get as bool.
    pub fn get_bool(&self, key: &str) -> Option<bool> {
        self.get(key).and_then(|v| parse_bool(&v))
    }

    /// Get required bool.
    pub fn get_bool_required(&self, key: &str) -> Result<bool> {
        let name = self.var_name(key);
        let value = self.get_required(key)?;
        parse_bool(&value).ok_or_else(|| EnvConfigError::Invalid {
            var: name,
            message: "expected boolean".into(),
        })
    }

    /// Get bool with default.
    pub fn get_bool_or(&self, key: &str, default: bool) -> bool {
        self.get_bool(key).unwrap_or(default)
    }

    /// Get as integer.
    pub fn get_int<T: std::str::FromStr>(&self, key: &str) -> Option<T> {
        self.get(key).and_then(|v| v.parse().ok())
    }

    /// Get required integer.
    pub fn get_int_required<T: std::str::FromStr>(&self, key: &str) -> Result<T> {
        let name = self.var_name(key);
        let value = self.get_required(key)?;
        value.parse().map_err(|_| EnvConfigError::ParseError {
            var: name,
            message: "expected integer".into(),
        })
    }

    /// Get integer with default.
    pub fn get_int_or<T: std::str::FromStr>(&self, key: &str, default: T) -> T {
        self.get_int(key).unwrap_or(default)
    }

    /// Get as float.
    pub fn get_float(&self, key: &str) -> Option<f64> {
        self.get(key).and_then(|v| v.parse().ok())
    }

    /// Get float with default.
    pub fn get_float_or(&self, key: &str, default: f64) -> f64 {
        self.get_float(key).unwrap_or(default)
    }

    /// Get as list (comma-separated).
    pub fn get_list(&self, key: &str) -> Vec<String> {
        self.get(key)
            .map(|v| v.split(',').map(|s| s.trim().to_string()).collect())
            .unwrap_or_default()
    }

    /// Get all variables with prefix.
    pub fn get_all(&self) -> HashMap<String, String> {
        let mut result = HashMap::new();
        let prefix_with_sep = match &self.prefix {
            Some(p) => format!("{}{}", p, self.separator),
            None => String::new(),
        };

        for (key, value) in env::vars() {
            if prefix_with_sep.is_empty() || key.starts_with(&prefix_with_sep) {
                let clean_key = if prefix_with_sep.is_empty() {
                    key
                } else {
                    key[prefix_with_sep.len()..].to_string()
                };
                result.insert(clean_key, value);
            }
        }

        result
    }

    /// Check if variable is set.
    pub fn is_set(&self, key: &str) -> bool {
        self.get(key).is_some()
    }
}

impl Default for EnvConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// Parse boolean from string.
fn parse_bool(s: &str) -> Option<bool> {
    match s.to_lowercase().as_str() {
        "true" | "yes" | "1" | "on" | "enabled" => Some(true),
        "false" | "no" | "0" | "off" | "disabled" => Some(false),
        _ => None,
    }
}

/// Environment variable builder.
pub struct EnvBuilder {
    vars: HashMap<String, String>,
}

impl EnvBuilder {
    /// Create new builder.
    pub fn new() -> Self {
        Self {
            vars: HashMap::new(),
        }
    }

    /// Set variable.
    pub fn set<K: Into<String>, V: Into<String>>(mut self, key: K, value: V) -> Self {
        self.vars.insert(key.into(), value.into());
        self
    }

    /// Set variable if not already set in environment.
    pub fn set_default<K: Into<String>, V: Into<String>>(mut self, key: K, value: V) -> Self {
        let key = key.into();
        if env::var(&key).is_err() {
            self.vars.insert(key, value.into());
        }
        self
    }

    /// Apply to environment.
    pub fn apply(self) {
        for (key, value) in self.vars {
            env::set_var(key, value);
        }
    }

    /// Get built variables.
    pub fn build(self) -> HashMap<String, String> {
        self.vars
    }
}

impl Default for EnvBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Get environment or default.
pub fn get_env_or<S: Into<String>>(key: &str, default: S) -> String {
    env::var(key).unwrap_or_else(|_| default.into())
}

/// Get required environment variable.
pub fn get_env_required(key: &str) -> Result<String> {
    env::var(key).map_err(|_| EnvConfigError::Missing(key.to_string()))
}

/// Check if running in production.
pub fn is_production() -> bool {
    matches!(
        env::var("ENV")
            .or_else(|_| env::var("ENVIRONMENT"))
            .as_deref(),
        Ok("production") | Ok("prod")
    )
}

/// Check if running in development.
pub fn is_development() -> bool {
    matches!(
        env::var("ENV")
            .or_else(|_| env::var("ENVIRONMENT"))
            .as_deref(),
        Ok("development") | Ok("dev") | Err(_)
    )
}

/// Check if running in test.
pub fn is_test() -> bool {
    matches!(
        env::var("ENV")
            .or_else(|_| env::var("ENVIRONMENT"))
            .as_deref(),
        Ok("test") | Ok("testing")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_env_config_basic() {
        env::set_var("TEST_VAR_123", "hello");
        let config = EnvConfig::new();

        assert_eq!(config.get("TEST_VAR_123"), Some("hello".to_string()));
        assert_eq!(config.get("NONEXISTENT_VAR"), None);
        assert_eq!(config.get_or("NONEXISTENT_VAR", "default"), "default");

        env::remove_var("TEST_VAR_123");
    }

    #[test]
    fn test_env_config_prefix() {
        env::set_var("MYAPP_PORT", "8080");
        env::set_var("MYAPP_DEBUG", "true");

        let config = EnvConfig::new().prefix("MYAPP");

        assert_eq!(config.get_int::<u16>("PORT"), Some(8080));
        assert_eq!(config.get_bool("DEBUG"), Some(true));

        env::remove_var("MYAPP_PORT");
        env::remove_var("MYAPP_DEBUG");
    }

    #[test]
    fn test_env_config_list() {
        env::set_var("TEST_LIST", "a, b, c");
        let config = EnvConfig::new();

        let list = config.get_list("TEST_LIST");
        assert_eq!(list, vec!["a", "b", "c"]);

        env::remove_var("TEST_LIST");
    }

    #[test]
    fn test_parse_bool() {
        assert_eq!(parse_bool("true"), Some(true));
        assert_eq!(parse_bool("yes"), Some(true));
        assert_eq!(parse_bool("1"), Some(true));
        assert_eq!(parse_bool("false"), Some(false));
        assert_eq!(parse_bool("no"), Some(false));
        assert_eq!(parse_bool("invalid"), None);
    }

    #[test]
    fn test_env_builder() {
        let vars = EnvBuilder::new()
            .set("KEY1", "value1")
            .set("KEY2", "value2")
            .build();

        assert_eq!(vars.get("KEY1"), Some(&"value1".to_string()));
        assert_eq!(vars.get("KEY2"), Some(&"value2".to_string()));
    }
}
