//! Argument parsing utilities for drbot.
//!
//! This crate provides:
//! - Argument parsing helpers
//! - Argument validation
//! - Argument builders

use std::collections::HashMap;
use std::str::FromStr;
use thiserror::Error;

/// Argument error types.
#[derive(Error, Debug)]
pub enum ArgError {
    #[error("Missing argument: {0}")]
    Missing(String),

    #[error("Invalid argument: {0}")]
    Invalid(String),

    #[error("Parse error for {key}: {message}")]
    ParseError { key: String, message: String },

    #[error("Validation error: {0}")]
    ValidationError(String),
}

/// Result type for argument operations.
pub type Result<T> = std::result::Result<T, ArgError>;

/// Parsed arguments.
#[derive(Debug, Clone, Default)]
pub struct Args {
    /// Positional arguments.
    positional: Vec<String>,
    /// Named arguments (--key=value or --key value).
    named: HashMap<String, String>,
    /// Flags (--flag).
    flags: HashMap<String, bool>,
}

impl Args {
    /// Create new empty args.
    pub fn new() -> Self {
        Self::default()
    }

    /// Parse from iterator.
    pub fn parse<I, S>(iter: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut args = Self::new();
        let mut iter = iter.into_iter().peekable();
        let mut expect_value_for: Option<String> = None;

        while let Some(arg) = iter.next() {
            let arg = arg.as_ref();

            if let Some(key) = expect_value_for.take() {
                args.named.insert(key, arg.to_string());
                continue;
            }

            if arg.starts_with("--") {
                let arg = &arg[2..];
                if let Some(eq_pos) = arg.find('=') {
                    let key = arg[..eq_pos].to_string();
                    let value = arg[eq_pos + 1..].to_string();
                    args.named.insert(key, value);
                } else if arg.starts_with("no-") {
                    args.flags.insert(arg[3..].to_string(), false);
                } else {
                    // Check if next arg is a value
                    if let Some(next) = iter.peek() {
                        let next = next.as_ref();
                        if !next.starts_with('-') {
                            expect_value_for = Some(arg.to_string());
                        } else {
                            args.flags.insert(arg.to_string(), true);
                        }
                    } else {
                        args.flags.insert(arg.to_string(), true);
                    }
                }
            } else if arg.starts_with('-') && arg.len() > 1 {
                // Short flags like -abc
                for c in arg[1..].chars() {
                    args.flags.insert(c.to_string(), true);
                }
            } else {
                args.positional.push(arg.to_string());
            }
        }

        args
    }

    /// Parse from environment args (skipping program name).
    pub fn from_env() -> Self {
        Self::parse(std::env::args().skip(1))
    }

    /// Get positional argument by index.
    pub fn positional(&self, index: usize) -> Option<&str> {
        self.positional.get(index).map(|s| s.as_str())
    }

    /// Get all positional arguments.
    pub fn positionals(&self) -> &[String] {
        &self.positional
    }

    /// Get named argument.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.named.get(key).map(|s| s.as_str())
    }

    /// Get named argument with default.
    pub fn get_or(&self, key: &str, default: &str) -> String {
        self.named
            .get(key)
            .cloned()
            .unwrap_or_else(|| default.to_string())
    }

    /// Get and parse named argument.
    pub fn get_parse<T: FromStr>(&self, key: &str) -> Result<T>
    where
        T::Err: std::fmt::Display,
    {
        let value = self
            .named
            .get(key)
            .ok_or_else(|| ArgError::Missing(key.to_string()))?;
        value.parse().map_err(|e: T::Err| ArgError::ParseError {
            key: key.to_string(),
            message: e.to_string(),
        })
    }

    /// Get and parse with default.
    pub fn get_parse_or<T: FromStr>(&self, key: &str, default: T) -> T
    where
        T::Err: std::fmt::Display,
    {
        self.get_parse(key).unwrap_or(default)
    }

    /// Check if flag is set.
    pub fn flag(&self, key: &str) -> bool {
        self.flags.get(key).copied().unwrap_or(false)
    }

    /// Check if any of the flags are set.
    pub fn any_flag(&self, keys: &[&str]) -> bool {
        keys.iter().any(|k| self.flag(k))
    }

    /// Check if argument exists (named or flag).
    pub fn has(&self, key: &str) -> bool {
        self.named.contains_key(key) || self.flags.contains_key(key)
    }

    /// Get all named arguments.
    pub fn named(&self) -> &HashMap<String, String> {
        &self.named
    }

    /// Get all flags.
    pub fn flags(&self) -> &HashMap<String, bool> {
        &self.flags
    }

    /// Set named argument.
    pub fn set(&mut self, key: &str, value: &str) {
        self.named.insert(key.to_string(), value.to_string());
    }

    /// Set flag.
    pub fn set_flag(&mut self, key: &str, value: bool) {
        self.flags.insert(key.to_string(), value);
    }

    /// Add positional argument.
    pub fn add_positional(&mut self, value: &str) {
        self.positional.push(value.to_string());
    }
}

/// Argument definition for validation.
#[derive(Debug, Clone)]
pub struct ArgDef {
    /// Argument name.
    pub name: String,
    /// Short name (single character).
    pub short: Option<char>,
    /// Description.
    pub description: String,
    /// Is required.
    pub required: bool,
    /// Default value.
    pub default: Option<String>,
    /// Allowed values.
    pub choices: Vec<String>,
}

impl ArgDef {
    /// Create new argument definition.
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            short: None,
            description: String::new(),
            required: false,
            default: None,
            choices: Vec::new(),
        }
    }

    /// Set short name.
    pub fn short(mut self, c: char) -> Self {
        self.short = Some(c);
        self
    }

    /// Set description.
    pub fn description(mut self, desc: &str) -> Self {
        self.description = desc.to_string();
        self
    }

    /// Set as required.
    pub fn required(mut self) -> Self {
        self.required = true;
        self
    }

    /// Set default value.
    pub fn default(mut self, value: &str) -> Self {
        self.default = Some(value.to_string());
        self
    }

    /// Set allowed choices.
    pub fn choices(mut self, choices: &[&str]) -> Self {
        self.choices = choices.iter().map(|s| s.to_string()).collect();
        self
    }
}

/// Argument parser with validation.
pub struct ArgParser {
    name: String,
    description: String,
    args: Vec<ArgDef>,
}

impl ArgParser {
    /// Create new argument parser.
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            description: String::new(),
            args: Vec::new(),
        }
    }

    /// Set description.
    pub fn description(mut self, desc: &str) -> Self {
        self.description = desc.to_string();
        self
    }

    /// Add argument definition.
    pub fn arg(mut self, def: ArgDef) -> Self {
        self.args.push(def);
        self
    }

    /// Parse and validate arguments.
    pub fn parse<I, S>(&self, iter: I) -> Result<Args>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut args = Args::parse(iter);

        // Apply defaults
        for def in &self.args {
            if !args.has(&def.name) {
                if let Some(default) = &def.default {
                    args.set(&def.name, default);
                }
            }
        }

        // Validate required
        for def in &self.args {
            if def.required && !args.has(&def.name) {
                return Err(ArgError::Missing(def.name.clone()));
            }
        }

        // Validate choices
        for def in &self.args {
            if !def.choices.is_empty() {
                if let Some(value) = args.get(&def.name) {
                    if !def.choices.iter().any(|c| c == value) {
                        return Err(ArgError::Invalid(format!(
                            "{}: must be one of {:?}",
                            def.name, def.choices
                        )));
                    }
                }
            }
        }

        Ok(args)
    }

    /// Generate help text.
    pub fn help(&self) -> String {
        let mut help = String::new();

        help.push_str(&self.name);
        if !self.description.is_empty() {
            help.push_str(&format!("\n{}\n", self.description));
        }
        help.push_str("\n\nArguments:\n");

        for def in &self.args {
            let mut line = format!("  --{}", def.name);
            if let Some(short) = def.short {
                line.push_str(&format!(", -{}", short));
            }
            if def.required {
                line.push_str(" (required)");
            }
            if !def.description.is_empty() {
                line.push_str(&format!("\n    {}", def.description));
            }
            if let Some(default) = &def.default {
                line.push_str(&format!(" [default: {}]", default));
            }
            if !def.choices.is_empty() {
                line.push_str(&format!(" [choices: {}]", def.choices.join(", ")));
            }
            line.push('\n');
            help.push_str(&line);
        }

        help
    }
}

/// Argument builder for programmatic argument construction.
pub struct ArgBuilder {
    args: Vec<String>,
}

impl ArgBuilder {
    /// Create new argument builder.
    pub fn new() -> Self {
        Self { args: Vec::new() }
    }

    /// Add positional argument.
    pub fn arg(mut self, value: &str) -> Self {
        self.args.push(value.to_string());
        self
    }

    /// Add named argument.
    pub fn named(mut self, key: &str, value: &str) -> Self {
        self.args.push(format!("--{}={}", key, value));
        self
    }

    /// Add flag.
    pub fn flag(mut self, key: &str) -> Self {
        self.args.push(format!("--{}", key));
        self
    }

    /// Add short flag.
    pub fn short_flag(mut self, c: char) -> Self {
        self.args.push(format!("-{}", c));
        self
    }

    /// Build into Args.
    pub fn build(self) -> Args {
        Args::parse(self.args)
    }

    /// Build into Vec<String>.
    pub fn into_vec(self) -> Vec<String> {
        self.args
    }
}

impl Default for ArgBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Split a command string into arguments, respecting quotes.
pub fn split_args(cmd: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut quote_char = '"';
    let mut escaped = false;

    for c in cmd.chars() {
        if escaped {
            current.push(c);
            escaped = false;
            continue;
        }

        match c {
            '\\' => escaped = true,
            '"' | '\'' if !in_quotes => {
                in_quotes = true;
                quote_char = c;
            }
            c if in_quotes && c == quote_char => {
                in_quotes = false;
            }
            ' ' | '\t' if !in_quotes => {
                if !current.is_empty() {
                    args.push(current);
                    current = String::new();
                }
            }
            _ => current.push(c),
        }
    }

    if !current.is_empty() {
        args.push(current);
    }

    args
}

/// Join arguments into a command string.
pub fn join_args(args: &[String]) -> String {
    args.iter()
        .map(|arg| {
            if arg.contains(' ') || arg.contains('\t') || arg.contains('"') {
                format!("\"{}\"", arg.replace('\\', "\\\\").replace('"', "\\\""))
            } else {
                arg.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_positional() {
        let args = Args::parse(["file1.txt", "file2.txt"]);
        assert_eq!(args.positional(0), Some("file1.txt"));
        assert_eq!(args.positional(1), Some("file2.txt"));
        assert_eq!(args.positionals().len(), 2);
    }

    #[test]
    fn test_parse_named() {
        let args = Args::parse(["--name=value", "--count", "10"]);
        assert_eq!(args.get("name"), Some("value"));
        assert_eq!(args.get("count"), Some("10"));
    }

    #[test]
    fn test_parse_flags() {
        let args = Args::parse(["--verbose", "--no-color", "-abc"]);
        assert!(args.flag("verbose"));
        assert!(!args.flag("color"));
        assert!(args.flag("a"));
        assert!(args.flag("b"));
        assert!(args.flag("c"));
    }

    #[test]
    fn test_parse_mixed() {
        let args = Args::parse(["input.txt", "--output=out.txt", "--verbose", "extra"]);
        assert_eq!(args.positional(0), Some("input.txt"));
        assert_eq!(args.positional(1), Some("extra"));
        assert_eq!(args.get("output"), Some("out.txt"));
        assert!(args.flag("verbose"));
    }

    #[test]
    fn test_get_parse() {
        let args = Args::parse(["--count=42"]);
        let count: i32 = args.get_parse("count").unwrap();
        assert_eq!(count, 42);
    }

    #[test]
    fn test_arg_parser() {
        let parser = ArgParser::new("test")
            .description("Test program")
            .arg(ArgDef::new("output").short('o').required())
            .arg(
                ArgDef::new("format")
                    .default("json")
                    .choices(&["json", "yaml"]),
            );

        let args = parser.parse(["--output=file.txt"]).unwrap();
        assert_eq!(args.get("output"), Some("file.txt"));
        assert_eq!(args.get("format"), Some("json"));
    }

    #[test]
    fn test_arg_builder() {
        let args = ArgBuilder::new()
            .arg("input.txt")
            .named("output", "out.txt")
            .flag("verbose")
            .build();

        assert_eq!(args.positional(0), Some("input.txt"));
        assert_eq!(args.get("output"), Some("out.txt"));
        assert!(args.flag("verbose"));
    }

    #[test]
    fn test_split_args() {
        let args = split_args("echo \"hello world\" --name='test value'");
        assert_eq!(args, vec!["echo", "hello world", "--name=test value"]);
    }

    #[test]
    fn test_join_args() {
        let args = vec![
            "echo".to_string(),
            "hello world".to_string(),
            "--flag".to_string(),
        ];
        let joined = join_args(&args);
        assert_eq!(joined, "echo \"hello world\" --flag");
    }
}
