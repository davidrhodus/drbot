//! Diagnostic information collection for drbot.
//!
//! This crate provides:
//! - System information collection
//! - Runtime diagnostics
//! - Error reports
//! - Debug dumps

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::time::{Duration, Instant};
use thiserror::Error;

/// Diagnostic error types.
#[derive(Error, Debug)]
pub enum DiagnosticError {
    #[error("Collection failed: {0}")]
    CollectionFailed(String),

    #[error("IO error: {0}")]
    Io(String),

    #[error("Serialization error: {0}")]
    Serialization(String),
}

/// Result type for diagnostic operations.
pub type Result<T> = std::result::Result<T, DiagnosticError>;

/// System information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemInfo {
    /// Operating system.
    pub os: String,
    /// OS version.
    pub os_version: Option<String>,
    /// Architecture.
    pub arch: String,
    /// Number of CPUs.
    pub num_cpus: usize,
    /// Hostname.
    pub hostname: Option<String>,
    /// Current working directory.
    pub cwd: Option<String>,
}

impl SystemInfo {
    /// Collect system info.
    pub fn collect() -> Self {
        Self {
            os: env::consts::OS.to_string(),
            os_version: None, // Would need platform-specific code
            arch: env::consts::ARCH.to_string(),
            num_cpus: std::thread::available_parallelism()
                .map(|p| p.get())
                .unwrap_or(1),
            hostname: hostname::get().ok().and_then(|h| h.into_string().ok()),
            cwd: env::current_dir().ok().map(|p| p.display().to_string()),
        }
    }
}

/// Get hostname (simple implementation using environment variables).
mod hostname {
    use std::ffi::OsString;

    pub fn get() -> std::io::Result<OsString> {
        // Try common environment variables for hostname
        std::env::var_os("HOSTNAME")
            .or_else(|| std::env::var_os("COMPUTERNAME"))
            .or_else(|| std::env::var_os("HOST"))
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "hostname not found"))
    }
}

/// Runtime information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeInfo {
    /// Rust version.
    pub rust_version: String,
    /// Package version.
    pub package_version: Option<String>,
    /// Process ID.
    pub pid: u32,
    /// Start time.
    pub start_time: DateTime<Utc>,
    /// Uptime.
    pub uptime: Duration,
    /// Environment variables (filtered).
    pub env_vars: HashMap<String, String>,
}

impl RuntimeInfo {
    /// Collect runtime info.
    pub fn collect(start: Instant) -> Self {
        Self {
            rust_version: rustc_version(),
            package_version: option_env!("CARGO_PKG_VERSION").map(|s| s.to_string()),
            pid: std::process::id(),
            start_time: Utc::now()
                - chrono::Duration::from_std(start.elapsed()).unwrap_or_default(),
            uptime: start.elapsed(),
            env_vars: collect_safe_env_vars(),
        }
    }
}

/// Get rustc version.
fn rustc_version() -> String {
    option_env!("RUSTC_VERSION")
        .unwrap_or("unknown")
        .to_string()
}

/// Collect safe environment variables (exclude secrets).
fn collect_safe_env_vars() -> HashMap<String, String> {
    let exclude_patterns = [
        "KEY",
        "SECRET",
        "TOKEN",
        "PASSWORD",
        "CREDENTIAL",
        "AUTH",
        "PRIVATE",
        "API_KEY",
        "APIKEY",
    ];

    env::vars()
        .filter(|(k, _)| {
            let upper = k.to_uppercase();
            !exclude_patterns.iter().any(|p| upper.contains(p))
        })
        .take(50) // Limit number of vars
        .collect()
}

/// Memory information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryInfo {
    /// Current memory usage (estimate).
    pub current_bytes: Option<u64>,
    /// Peak memory usage (estimate).
    pub peak_bytes: Option<u64>,
}

impl MemoryInfo {
    /// Collect memory info.
    pub fn collect() -> Self {
        Self {
            current_bytes: None, // Would need platform-specific code
            peak_bytes: None,
        }
    }
}

/// Diagnostic report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticReport {
    /// Report ID.
    pub id: String,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
    /// System info.
    pub system: SystemInfo,
    /// Runtime info.
    pub runtime: RuntimeInfo,
    /// Memory info.
    pub memory: MemoryInfo,
    /// Custom sections.
    pub sections: HashMap<String, serde_json::Value>,
    /// Errors/warnings.
    pub issues: Vec<DiagnosticIssue>,
}

impl DiagnosticReport {
    /// Create new report.
    pub fn new(start_time: Instant) -> Self {
        Self {
            id: uuid_simple(),
            timestamp: Utc::now(),
            system: SystemInfo::collect(),
            runtime: RuntimeInfo::collect(start_time),
            memory: MemoryInfo::collect(),
            sections: HashMap::new(),
            issues: Vec::new(),
        }
    }

    /// Add custom section.
    pub fn add_section(&mut self, name: impl Into<String>, data: impl Serialize) {
        if let Ok(value) = serde_json::to_value(data) {
            self.sections.insert(name.into(), value);
        }
    }

    /// Add issue.
    pub fn add_issue(&mut self, issue: DiagnosticIssue) {
        self.issues.push(issue);
    }

    /// Convert to JSON.
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string_pretty(self)
            .map_err(|e| DiagnosticError::Serialization(e.to_string()))
    }

    /// Convert to compact JSON.
    pub fn to_json_compact(&self) -> Result<String> {
        serde_json::to_string(self).map_err(|e| DiagnosticError::Serialization(e.to_string()))
    }
}

/// Generate simple UUID.
fn uuid_simple() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();

    format!(
        "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
        now.as_secs() as u32,
        (now.subsec_nanos() >> 16) as u16,
        0x4000 | ((now.subsec_nanos() & 0x0fff) as u16),
        0x8000 | ((now.as_nanos() & 0x3fff) as u16),
        (now.as_nanos() & 0xffffffffffff) as u64
    )
}

/// Diagnostic issue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticIssue {
    /// Issue level.
    pub level: IssueLevel,
    /// Issue code.
    pub code: String,
    /// Message.
    pub message: String,
    /// Context.
    pub context: HashMap<String, String>,
}

impl DiagnosticIssue {
    /// Create error issue.
    pub fn error(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            level: IssueLevel::Error,
            code: code.into(),
            message: message.into(),
            context: HashMap::new(),
        }
    }

    /// Create warning issue.
    pub fn warning(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            level: IssueLevel::Warning,
            code: code.into(),
            message: message.into(),
            context: HashMap::new(),
        }
    }

    /// Create info issue.
    pub fn info(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            level: IssueLevel::Info,
            code: code.into(),
            message: message.into(),
            context: HashMap::new(),
        }
    }

    /// Add context.
    pub fn with_context(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.context.insert(key.into(), value.into());
        self
    }
}

/// Issue severity level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IssueLevel {
    Error,
    Warning,
    Info,
}

/// Health check result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheck {
    /// Check name.
    pub name: String,
    /// Passed.
    pub passed: bool,
    /// Duration.
    pub duration: Duration,
    /// Message.
    pub message: Option<String>,
    /// Details.
    pub details: HashMap<String, serde_json::Value>,
}

impl HealthCheck {
    /// Create passed check.
    pub fn pass(name: impl Into<String>, duration: Duration) -> Self {
        Self {
            name: name.into(),
            passed: true,
            duration,
            message: None,
            details: HashMap::new(),
        }
    }

    /// Create failed check.
    pub fn fail(name: impl Into<String>, duration: Duration, message: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            passed: false,
            duration,
            message: Some(message.into()),
            details: HashMap::new(),
        }
    }

    /// Add detail.
    pub fn with_detail(mut self, key: impl Into<String>, value: impl Serialize) -> Self {
        if let Ok(v) = serde_json::to_value(value) {
            self.details.insert(key.into(), v);
        }
        self
    }
}

/// Health check runner.
pub struct HealthChecker {
    checks: Vec<Box<dyn Fn() -> HealthCheck + Send + Sync>>,
}

impl HealthChecker {
    /// Create new checker.
    pub fn new() -> Self {
        Self { checks: Vec::new() }
    }

    /// Add check.
    pub fn add_check<F>(&mut self, check: F)
    where
        F: Fn() -> HealthCheck + Send + Sync + 'static,
    {
        self.checks.push(Box::new(check));
    }

    /// Run all checks.
    pub fn run(&self) -> Vec<HealthCheck> {
        self.checks.iter().map(|c| c()).collect()
    }

    /// Run and return overall status.
    pub fn check_all(&self) -> (bool, Vec<HealthCheck>) {
        let results = self.run();
        let all_passed = results.iter().all(|c| c.passed);
        (all_passed, results)
    }
}

impl Default for HealthChecker {
    fn default() -> Self {
        Self::new()
    }
}

/// Debug snapshot for troubleshooting.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebugSnapshot {
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
    /// Label.
    pub label: String,
    /// Data.
    pub data: HashMap<String, serde_json::Value>,
}

impl DebugSnapshot {
    /// Create new snapshot.
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            timestamp: Utc::now(),
            label: label.into(),
            data: HashMap::new(),
        }
    }

    /// Add data.
    pub fn add(&mut self, key: impl Into<String>, value: impl Serialize) {
        if let Ok(v) = serde_json::to_value(value) {
            self.data.insert(key.into(), v);
        }
    }

    /// Builder pattern for adding data.
    pub fn with(mut self, key: impl Into<String>, value: impl Serialize) -> Self {
        self.add(key, value);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_info() {
        let info = SystemInfo::collect();
        assert!(!info.os.is_empty());
        assert!(!info.arch.is_empty());
        assert!(info.num_cpus > 0);
    }

    #[test]
    fn test_runtime_info() {
        let start = Instant::now();
        std::thread::sleep(Duration::from_millis(10));
        let info = RuntimeInfo::collect(start);

        assert!(info.uptime >= Duration::from_millis(10));
        assert!(info.pid > 0);
    }

    #[test]
    fn test_diagnostic_report() {
        let start = Instant::now();
        let mut report = DiagnosticReport::new(start);

        report.add_section("custom", serde_json::json!({"key": "value"}));
        report.add_issue(DiagnosticIssue::info("TEST001", "Test issue"));

        assert!(!report.id.is_empty());
        assert!(report.sections.contains_key("custom"));
        assert_eq!(report.issues.len(), 1);
    }

    #[test]
    fn test_health_check() {
        let check =
            HealthCheck::pass("test", Duration::from_millis(10)).with_detail("version", "1.0.0");

        assert!(check.passed);
        assert!(check.details.contains_key("version"));
    }

    #[test]
    fn test_health_checker() {
        let mut checker = HealthChecker::new();
        checker.add_check(|| HealthCheck::pass("check1", Duration::from_millis(1)));
        checker.add_check(|| HealthCheck::pass("check2", Duration::from_millis(1)));

        let (passed, results) = checker.check_all();
        assert!(passed);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_debug_snapshot() {
        let snapshot = DebugSnapshot::new("test")
            .with("count", 42)
            .with("name", "test");

        assert_eq!(snapshot.label, "test");
        assert!(snapshot.data.contains_key("count"));
        assert!(snapshot.data.contains_key("name"));
    }
}
