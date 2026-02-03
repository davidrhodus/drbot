//! Error log parsing and analysis.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Source of an error.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorSource {
    /// Compiler error.
    Compiler,
    /// Runtime error.
    Runtime,
    /// Test failure.
    Test,
    /// Linter warning.
    Lint,
    /// Build system error.
    Build,
    /// Unknown source.
    Unknown,
}

/// A single error entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorEntry {
    /// Error source.
    pub source: ErrorSource,
    /// Error message.
    pub message: String,
    /// File path (if applicable).
    pub file: Option<PathBuf>,
    /// Line number (if applicable).
    pub line: Option<usize>,
    /// Column (if applicable).
    pub column: Option<usize>,
    /// Error code (e.g., E0308 for Rust).
    pub code: Option<String>,
    /// Severity level.
    pub severity: ErrorSeverity,
    /// Suggested fix (if available).
    pub suggestion: Option<String>,
}

/// Error severity level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorSeverity {
    Error,
    Warning,
    Info,
    Hint,
}

impl ErrorEntry {
    /// Create a new error entry.
    pub fn new(source: ErrorSource, message: impl Into<String>) -> Self {
        Self {
            source,
            message: message.into(),
            file: None,
            line: None,
            column: None,
            code: None,
            severity: ErrorSeverity::Error,
            suggestion: None,
        }
    }

    /// Set file location.
    pub fn with_location(mut self, file: PathBuf, line: usize, column: Option<usize>) -> Self {
        self.file = Some(file);
        self.line = Some(line);
        self.column = column;
        self
    }

    /// Set error code.
    pub fn with_code(mut self, code: impl Into<String>) -> Self {
        self.code = Some(code.into());
        self
    }

    /// Set severity.
    pub fn with_severity(mut self, severity: ErrorSeverity) -> Self {
        self.severity = severity;
        self
    }

    /// Set suggestion.
    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.suggestion = Some(suggestion.into());
        self
    }

    /// Format as a concise string.
    pub fn format_brief(&self) -> String {
        let loc = if let Some(file) = &self.file {
            let line = self.line.unwrap_or(0);
            format!("{}:{}", file.display(), line)
        } else {
            "unknown".to_string()
        };

        format!("[{:?}] {}: {}", self.severity, loc, self.message)
    }
}

/// Collection of error entries.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ErrorLog {
    /// Error entries.
    pub entries: Vec<ErrorEntry>,
    /// Raw output that was parsed.
    pub raw_output: Option<String>,
}

impl ErrorLog {
    /// Create a new error log.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an entry.
    pub fn add(&mut self, entry: ErrorEntry) {
        self.entries.push(entry);
    }

    /// Parse from Rust compiler output.
    pub fn parse_rust_output(output: &str) -> Self {
        let mut log = ErrorLog::new();
        log.raw_output = Some(output.to_string());

        // Simple parser for cargo/rustc output
        for line in output.lines() {
            // Match pattern: error[E0308]: ...
            if let Some(start) = line.find("error[") {
                if let Some(end) = line.find("]:") {
                    let code = &line[start + 6..end];
                    let message = &line[end + 3..];

                    log.add(
                        ErrorEntry::new(ErrorSource::Compiler, message.trim())
                            .with_code(code)
                            .with_severity(ErrorSeverity::Error),
                    );
                }
            }
            // Match pattern: warning: ...
            else if line.contains("warning:") {
                if let Some(idx) = line.find("warning:") {
                    let message = &line[idx + 9..];
                    log.add(
                        ErrorEntry::new(ErrorSource::Compiler, message.trim())
                            .with_severity(ErrorSeverity::Warning),
                    );
                }
            }
        }

        log
    }

    /// Parse from TypeScript/ESLint output.
    pub fn parse_typescript_output(output: &str) -> Self {
        let mut log = ErrorLog::new();
        log.raw_output = Some(output.to_string());

        // Parse TypeScript error format: file(line,col): error TS1234: message
        for line in output.lines() {
            if line.contains("error TS") || line.contains("error:") {
                log.add(
                    ErrorEntry::new(ErrorSource::Compiler, line.trim())
                        .with_severity(ErrorSeverity::Error),
                );
            } else if line.contains("warning") {
                log.add(
                    ErrorEntry::new(ErrorSource::Lint, line.trim())
                        .with_severity(ErrorSeverity::Warning),
                );
            }
        }

        log
    }

    /// Parse from Python output.
    pub fn parse_python_output(output: &str) -> Self {
        let mut log = ErrorLog::new();
        log.raw_output = Some(output.to_string());

        // Parse Python traceback and errors
        let mut current_error: Option<String> = None;

        for line in output.lines() {
            if line.contains("Error:") || line.contains("Exception:") {
                current_error = Some(line.to_string());
            }
            if line.starts_with("  File \"") {
                // Part of traceback
            }
        }

        if let Some(error) = current_error {
            log.add(
                ErrorEntry::new(ErrorSource::Runtime, error.trim())
                    .with_severity(ErrorSeverity::Error),
            );
        }

        log
    }

    /// Get error count.
    pub fn error_count(&self) -> usize {
        self.entries
            .iter()
            .filter(|e| e.severity == ErrorSeverity::Error)
            .count()
    }

    /// Get warning count.
    pub fn warning_count(&self) -> usize {
        self.entries
            .iter()
            .filter(|e| e.severity == ErrorSeverity::Warning)
            .count()
    }

    /// Format as summary.
    pub fn summary(&self) -> String {
        let errors = self.error_count();
        let warnings = self.warning_count();
        format!("{} error(s), {} warning(s)", errors, warnings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_entry() {
        let entry = ErrorEntry::new(ErrorSource::Compiler, "Type mismatch")
            .with_code("E0308")
            .with_severity(ErrorSeverity::Error);

        assert_eq!(entry.code, Some("E0308".to_string()));
        assert!(entry.format_brief().contains("Type mismatch"));
    }

    #[test]
    fn test_parse_rust_output() {
        let output = r#"
error[E0308]: mismatched types
   --> src/main.rs:10:5
    |
warning: unused variable: `x`
   --> src/main.rs:5:9
"#;

        let log = ErrorLog::parse_rust_output(output);
        assert!(log.error_count() >= 1);
        assert!(log.warning_count() >= 1);
    }

    #[test]
    fn test_error_log_summary() {
        let mut log = ErrorLog::new();
        log.add(ErrorEntry::new(ErrorSource::Compiler, "error1"));
        log.add(
            ErrorEntry::new(ErrorSource::Compiler, "warning1")
                .with_severity(ErrorSeverity::Warning),
        );

        assert_eq!(log.summary(), "1 error(s), 1 warning(s)");
    }
}
