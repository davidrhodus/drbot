//! Environment variable utilities for drbot.
//!
//! This crate provides:
//! - Type-safe environment variable access
//! - Default values and validation
//! - Environment snapshots

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::str::FromStr;
use thiserror::Error;

/// Environment error types.
#[derive(Error, Debug)]
pub enum EnvError {
    #[error("Variable not found: {0}")]
    NotFound(String),

    #[error("Parse error for {key}: {message}")]
    ParseError { key: String, message: String },

    #[error("Validation error for {key}: {message}")]
    ValidationError { key: String, message: String },
}

/// Result type for environment operations.
pub type Result<T> = std::result::Result<T, EnvError>;

/// Environment variable accessor.
pub struct Env;

impl Env {
    /// Get environment variable as string.
    pub fn get(key: &str) -> Result<String> {
        env::var(key).map_err(|_| EnvError::NotFound(key.to_string()))
    }

    /// Get environment variable with default.
    pub fn get_or(key: &str, default: &str) -> String {
        env::var(key).unwrap_or_else(|_| default.to_string())
    }

    /// Get environment variable, returning None if not found.
    pub fn get_opt(key: &str) -> Option<String> {
        env::var(key).ok()
    }

    /// Get and parse environment variable.
    pub fn get_parse<T: FromStr>(key: &str) -> Result<T>
    where
        T::Err: std::fmt::Display,
    {
        let value = Self::get(key)?;
        value.parse().map_err(|e: T::Err| EnvError::ParseError {
            key: key.to_string(),
            message: e.to_string(),
        })
    }

    /// Get and parse with default.
    pub fn get_parse_or<T: FromStr>(key: &str, default: T) -> T
    where
        T::Err: std::fmt::Display,
    {
        Self::get_parse(key).unwrap_or(default)
    }

    /// Get boolean environment variable.
    pub fn get_bool(key: &str) -> Result<bool> {
        let value = Self::get(key)?;
        match value.to_lowercase().as_str() {
            "true" | "1" | "yes" | "on" => Ok(true),
            "false" | "0" | "no" | "off" => Ok(false),
            _ => Err(EnvError::ParseError {
                key: key.to_string(),
                message: format!("Invalid boolean value: {}", value),
            }),
        }
    }

    /// Get boolean with default.
    pub fn get_bool_or(key: &str, default: bool) -> bool {
        Self::get_bool(key).unwrap_or(default)
    }

    /// Get integer environment variable.
    pub fn get_i64(key: &str) -> Result<i64> {
        Self::get_parse(key)
    }

    /// Get integer with default.
    pub fn get_i64_or(key: &str, default: i64) -> i64 {
        Self::get_parse_or(key, default)
    }

    /// Get unsigned integer environment variable.
    pub fn get_u64(key: &str) -> Result<u64> {
        Self::get_parse(key)
    }

    /// Get unsigned integer with default.
    pub fn get_u64_or(key: &str, default: u64) -> u64 {
        Self::get_parse_or(key, default)
    }

    /// Get float environment variable.
    pub fn get_f64(key: &str) -> Result<f64> {
        Self::get_parse(key)
    }

    /// Get float with default.
    pub fn get_f64_or(key: &str, default: f64) -> f64 {
        Self::get_parse_or(key, default)
    }

    /// Get list environment variable (comma-separated).
    pub fn get_list(key: &str) -> Result<Vec<String>> {
        let value = Self::get(key)?;
        Ok(value.split(',').map(|s| s.trim().to_string()).collect())
    }

    /// Get list with default.
    pub fn get_list_or(key: &str, default: Vec<String>) -> Vec<String> {
        Self::get_list(key).unwrap_or(default)
    }

    /// Set environment variable.
    pub fn set(key: &str, value: &str) {
        env::set_var(key, value);
    }

    /// Remove environment variable.
    pub fn remove(key: &str) {
        env::remove_var(key);
    }

    /// Check if variable exists.
    pub fn exists(key: &str) -> bool {
        env::var(key).is_ok()
    }

    /// Get all environment variables.
    pub fn all() -> HashMap<String, String> {
        env::vars().collect()
    }

    /// Get variables with prefix.
    pub fn with_prefix(prefix: &str) -> HashMap<String, String> {
        env::vars().filter(|(k, _)| k.starts_with(prefix)).collect()
    }

    /// Get current directory.
    pub fn current_dir() -> Result<std::path::PathBuf> {
        env::current_dir().map_err(|_| EnvError::NotFound("CWD".to_string()))
    }

    /// Get home directory.
    pub fn home_dir() -> Option<std::path::PathBuf> {
        env::var("HOME")
            .or_else(|_| env::var("USERPROFILE"))
            .ok()
            .map(std::path::PathBuf::from)
    }

    /// Get temp directory.
    pub fn temp_dir() -> std::path::PathBuf {
        env::temp_dir()
    }
}

/// Environment variable builder with validation.
pub struct EnvVar<T> {
    key: String,
    default: Option<T>,
    validator: Option<Box<dyn Fn(&T) -> std::result::Result<(), String>>>,
}

impl<T> EnvVar<T>
where
    T: FromStr,
    T::Err: std::fmt::Display,
{
    /// Create new environment variable accessor.
    pub fn new(key: &str) -> Self {
        Self {
            key: key.to_string(),
            default: None,
            validator: None,
        }
    }

    /// Set default value.
    pub fn default(mut self, value: T) -> Self {
        self.default = Some(value);
        self
    }

    /// Add validator.
    pub fn validate<F>(mut self, f: F) -> Self
    where
        F: Fn(&T) -> std::result::Result<(), String> + 'static,
    {
        self.validator = Some(Box::new(f));
        self
    }

    /// Get the value.
    pub fn get(self) -> Result<T> {
        let value = match Env::get(&self.key) {
            Ok(v) => v.parse().map_err(|e: T::Err| EnvError::ParseError {
                key: self.key.clone(),
                message: e.to_string(),
            })?,
            Err(_) => match self.default {
                Some(d) => d,
                None => return Err(EnvError::NotFound(self.key)),
            },
        };

        if let Some(validator) = self.validator {
            validator(&value).map_err(|msg| EnvError::ValidationError {
                key: self.key,
                message: msg,
            })?;
        }

        Ok(value)
    }
}

/// Environment snapshot for testing or isolation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvSnapshot {
    vars: HashMap<String, String>,
}

impl EnvSnapshot {
    /// Capture current environment.
    pub fn capture() -> Self {
        Self {
            vars: env::vars().collect(),
        }
    }

    /// Capture with prefix filter.
    pub fn capture_prefix(prefix: &str) -> Self {
        Self {
            vars: env::vars().filter(|(k, _)| k.starts_with(prefix)).collect(),
        }
    }

    /// Restore environment from snapshot.
    pub fn restore(&self) {
        for (key, value) in &self.vars {
            env::set_var(key, value);
        }
    }

    /// Get variable from snapshot.
    pub fn get(&self, key: &str) -> Option<&String> {
        self.vars.get(key)
    }

    /// Merge with another snapshot.
    pub fn merge(&mut self, other: &EnvSnapshot) {
        self.vars.extend(other.vars.clone());
    }

    /// Get all variables.
    pub fn vars(&self) -> &HashMap<String, String> {
        &self.vars
    }

    /// To JSON string.
    pub fn to_json(&self) -> serde_json::Result<String> {
        serde_json::to_string_pretty(&self.vars)
    }

    /// From JSON string.
    pub fn from_json(json: &str) -> serde_json::Result<Self> {
        let vars: HashMap<String, String> = serde_json::from_str(json)?;
        Ok(Self { vars })
    }
}

/// Environment scope guard.
pub struct EnvScope {
    original: HashMap<String, Option<String>>,
}

impl EnvScope {
    /// Create new scope with temporary variables.
    pub fn new(vars: &[(&str, &str)]) -> Self {
        let mut original = HashMap::new();

        for (key, value) in vars {
            original.insert(key.to_string(), env::var(key).ok());
            env::set_var(key, value);
        }

        Self { original }
    }

    /// Set a variable in this scope.
    pub fn set(&mut self, key: &str, value: &str) {
        if !self.original.contains_key(key) {
            self.original.insert(key.to_string(), env::var(key).ok());
        }
        env::set_var(key, value);
    }
}

impl Drop for EnvScope {
    fn drop(&mut self) {
        for (key, value) in &self.original {
            match value {
                Some(v) => env::set_var(key, v),
                None => env::remove_var(key),
            }
        }
    }
}

/// Required environment variables checker.
pub struct RequiredEnv {
    vars: Vec<String>,
    missing: Vec<String>,
}

impl RequiredEnv {
    /// Create new required env checker.
    pub fn new() -> Self {
        Self {
            vars: Vec::new(),
            missing: Vec::new(),
        }
    }

    /// Add required variable.
    pub fn require(mut self, key: &str) -> Self {
        self.vars.push(key.to_string());
        if !Env::exists(key) {
            self.missing.push(key.to_string());
        }
        self
    }

    /// Add multiple required variables.
    pub fn require_all(mut self, keys: &[&str]) -> Self {
        for key in keys {
            self.vars.push(key.to_string());
            if !Env::exists(key) {
                self.missing.push(key.to_string());
            }
        }
        self
    }

    /// Check if all required variables exist.
    pub fn check(&self) -> Result<()> {
        if self.missing.is_empty() {
            Ok(())
        } else {
            Err(EnvError::NotFound(format!(
                "Missing required environment variables: {}",
                self.missing.join(", ")
            )))
        }
    }

    /// Get missing variables.
    pub fn missing(&self) -> &[String] {
        &self.missing
    }

    /// Check if any are missing.
    pub fn has_missing(&self) -> bool {
        !self.missing.is_empty()
    }
}

impl Default for RequiredEnv {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_env_basic() {
        Env::set("TEST_VAR", "hello");
        assert_eq!(Env::get("TEST_VAR").unwrap(), "hello");
        assert!(Env::exists("TEST_VAR"));
        Env::remove("TEST_VAR");
        assert!(!Env::exists("TEST_VAR"));
    }

    #[test]
    fn test_env_default() {
        assert_eq!(Env::get_or("NONEXISTENT_VAR", "default"), "default");
    }

    #[test]
    fn test_env_parse() {
        Env::set("TEST_INT", "42");
        assert_eq!(Env::get_i64("TEST_INT").unwrap(), 42);
        Env::remove("TEST_INT");
    }

    #[test]
    fn test_env_bool() {
        Env::set("TEST_BOOL", "true");
        assert!(Env::get_bool("TEST_BOOL").unwrap());

        Env::set("TEST_BOOL", "0");
        assert!(!Env::get_bool("TEST_BOOL").unwrap());

        Env::remove("TEST_BOOL");
    }

    #[test]
    fn test_env_list() {
        Env::set("TEST_LIST", "a, b, c");
        let list = Env::get_list("TEST_LIST").unwrap();
        assert_eq!(list, vec!["a", "b", "c"]);
        Env::remove("TEST_LIST");
    }

    #[test]
    fn test_env_scope() {
        Env::set("SCOPE_TEST", "original");

        {
            let _scope = EnvScope::new(&[("SCOPE_TEST", "scoped")]);
            assert_eq!(Env::get("SCOPE_TEST").unwrap(), "scoped");
        }

        assert_eq!(Env::get("SCOPE_TEST").unwrap(), "original");
        Env::remove("SCOPE_TEST");
    }

    #[test]
    fn test_env_snapshot() {
        Env::set("SNAP_TEST", "value");
        let snapshot = EnvSnapshot::capture_prefix("SNAP_");
        assert_eq!(snapshot.get("SNAP_TEST"), Some(&"value".to_string()));
        Env::remove("SNAP_TEST");
    }

    #[test]
    fn test_env_var_builder() {
        Env::set("BUILDER_TEST", "100");
        let value: i32 = EnvVar::new("BUILDER_TEST")
            .validate(|v| {
                if *v > 0 {
                    Ok(())
                } else {
                    Err("must be positive".to_string())
                }
            })
            .get()
            .unwrap();
        assert_eq!(value, 100);
        Env::remove("BUILDER_TEST");
    }

    #[test]
    fn test_required_env() {
        Env::set("REQ_A", "a");
        let checker = RequiredEnv::new()
            .require("REQ_A")
            .require("REQ_NONEXISTENT");

        assert!(checker.has_missing());
        assert_eq!(checker.missing(), &["REQ_NONEXISTENT"]);
        Env::remove("REQ_A");
    }
}
