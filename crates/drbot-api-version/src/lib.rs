//! API versioning for drbot.
//!
//! This crate provides API versioning support including version parsing,
//! negotiation, deprecation handling, and migration utilities.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;

/// API version error types.
#[derive(Error, Debug)]
pub enum VersionError {
    #[error("Invalid version format: {0}")]
    InvalidFormat(String),

    #[error("Version not found: {0}")]
    NotFound(String),

    #[error("Version deprecated: {0}")]
    Deprecated(String),

    #[error("Version not supported: {0}")]
    NotSupported(String),

    #[error("Migration error: {0}")]
    MigrationError(String),
}

/// Result type for version operations.
pub type Result<T> = std::result::Result<T, VersionError>;

/// Semantic version representation.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Version {
    /// Major version number.
    pub major: u32,
    /// Minor version number.
    pub minor: u32,
    /// Patch version number.
    pub patch: u32,
    /// Pre-release identifier (e.g., "alpha", "beta.1").
    pub prerelease: Option<String>,
}

impl Version {
    /// Create a new version.
    pub fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            major,
            minor,
            patch,
            prerelease: None,
        }
    }

    /// Create a version with pre-release.
    pub fn with_prerelease(mut self, prerelease: impl Into<String>) -> Self {
        self.prerelease = Some(prerelease.into());
        self
    }

    /// Parse a version string.
    pub fn parse(s: &str) -> Result<Self> {
        let re = Regex::new(r"^v?(\d+)\.(\d+)\.(\d+)(?:-(.+))?$")
            .map_err(|e| VersionError::InvalidFormat(e.to_string()))?;

        let caps = re
            .captures(s)
            .ok_or_else(|| VersionError::InvalidFormat(s.to_string()))?;

        let major = caps[1]
            .parse()
            .map_err(|_| VersionError::InvalidFormat(s.to_string()))?;
        let minor = caps[2]
            .parse()
            .map_err(|_| VersionError::InvalidFormat(s.to_string()))?;
        let patch = caps[3]
            .parse()
            .map_err(|_| VersionError::InvalidFormat(s.to_string()))?;
        let prerelease = caps.get(4).map(|m| m.as_str().to_string());

        Ok(Self {
            major,
            minor,
            patch,
            prerelease,
        })
    }

    /// Check if this is a stable version (no pre-release).
    pub fn is_stable(&self) -> bool {
        self.prerelease.is_none()
    }

    /// Check if this version is compatible with another (same major version).
    pub fn is_compatible(&self, other: &Version) -> bool {
        self.major == other.major
    }

    /// Get the next major version.
    pub fn next_major(&self) -> Self {
        Self::new(self.major + 1, 0, 0)
    }

    /// Get the next minor version.
    pub fn next_minor(&self) -> Self {
        Self::new(self.major, self.minor + 1, 0)
    }

    /// Get the next patch version.
    pub fn next_patch(&self) -> Self {
        Self::new(self.major, self.minor, self.patch + 1)
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.prerelease {
            Some(pre) => write!(f, "{}.{}.{}-{}", self.major, self.minor, self.patch, pre),
            None => write!(f, "{}.{}.{}", self.major, self.minor, self.patch),
        }
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.major.cmp(&other.major) {
            Ordering::Equal => {}
            ord => return ord,
        }
        match self.minor.cmp(&other.minor) {
            Ordering::Equal => {}
            ord => return ord,
        }
        match self.patch.cmp(&other.patch) {
            Ordering::Equal => {}
            ord => return ord,
        }
        // Pre-release versions are less than stable
        match (&self.prerelease, &other.prerelease) {
            (None, None) => Ordering::Equal,
            (None, Some(_)) => Ordering::Greater,
            (Some(_), None) => Ordering::Less,
            (Some(a), Some(b)) => a.cmp(b),
        }
    }
}

/// API version status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VersionStatus {
    /// Version is current and fully supported.
    Current,
    /// Version is supported but not the latest.
    Supported,
    /// Version is deprecated but still functional.
    Deprecated,
    /// Version is no longer supported.
    Sunset,
}

/// API version information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionInfo {
    /// The version.
    pub version: Version,
    /// Version status.
    pub status: VersionStatus,
    /// Release date.
    pub released: DateTime<Utc>,
    /// Deprecation date (if deprecated).
    pub deprecated: Option<DateTime<Utc>>,
    /// Sunset date (if sunset).
    pub sunset: Option<DateTime<Utc>>,
    /// Changelog URL.
    pub changelog_url: Option<String>,
    /// Migration guide URL.
    pub migration_url: Option<String>,
}

impl VersionInfo {
    /// Create new version info.
    pub fn new(version: Version, released: DateTime<Utc>) -> Self {
        Self {
            version,
            status: VersionStatus::Current,
            released,
            deprecated: None,
            sunset: None,
            changelog_url: None,
            migration_url: None,
        }
    }

    /// Set status.
    pub fn with_status(mut self, status: VersionStatus) -> Self {
        self.status = status;
        self
    }

    /// Set deprecation date.
    pub fn with_deprecation(mut self, date: DateTime<Utc>) -> Self {
        self.deprecated = Some(date);
        self.status = VersionStatus::Deprecated;
        self
    }

    /// Set sunset date.
    pub fn with_sunset(mut self, date: DateTime<Utc>) -> Self {
        self.sunset = Some(date);
        self.status = VersionStatus::Sunset;
        self
    }

    /// Check if version is active (not sunset).
    pub fn is_active(&self) -> bool {
        self.status != VersionStatus::Sunset
    }
}

/// Version negotiation strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NegotiationStrategy {
    /// Use exact version match.
    Exact,
    /// Accept any compatible version (same major).
    Compatible,
    /// Use the latest stable version.
    Latest,
    /// Accept any version, prefer latest.
    Any,
}

/// API version registry.
pub struct VersionRegistry {
    versions: RwLock<HashMap<String, VersionInfo>>,
    default_version: RwLock<Option<Version>>,
    strategy: NegotiationStrategy,
}

impl VersionRegistry {
    /// Create a new version registry.
    pub fn new() -> Self {
        Self {
            versions: RwLock::new(HashMap::new()),
            default_version: RwLock::new(None),
            strategy: NegotiationStrategy::Compatible,
        }
    }

    /// Create registry with negotiation strategy.
    pub fn with_strategy(mut self, strategy: NegotiationStrategy) -> Self {
        self.strategy = strategy;
        self
    }

    /// Register a version.
    pub async fn register(&self, info: VersionInfo) {
        let key = info.version.to_string();
        let mut versions = self.versions.write().await;
        versions.insert(key, info);
    }

    /// Set the default version.
    pub async fn set_default(&self, version: Version) {
        let mut default = self.default_version.write().await;
        *default = Some(version);
    }

    /// Get version info.
    pub async fn get(&self, version: &Version) -> Option<VersionInfo> {
        let key = version.to_string();
        let versions = self.versions.read().await;
        versions.get(&key).cloned()
    }

    /// Get all versions.
    pub async fn list(&self) -> Vec<VersionInfo> {
        let versions = self.versions.read().await;
        let mut list: Vec<_> = versions.values().cloned().collect();
        list.sort_by(|a, b| b.version.cmp(&a.version));
        list
    }

    /// Get active versions (not sunset).
    pub async fn list_active(&self) -> Vec<VersionInfo> {
        self.list()
            .await
            .into_iter()
            .filter(|v| v.is_active())
            .collect()
    }

    /// Negotiate version based on requested version.
    pub async fn negotiate(&self, requested: Option<&Version>) -> Result<VersionInfo> {
        let versions = self.versions.read().await;

        match requested {
            Some(req) => {
                let key = req.to_string();
                match self.strategy {
                    NegotiationStrategy::Exact => versions
                        .get(&key)
                        .cloned()
                        .ok_or_else(|| VersionError::NotFound(key)),
                    NegotiationStrategy::Compatible => {
                        // Find best compatible version
                        let compatible: Vec<_> = versions
                            .values()
                            .filter(|v| v.version.is_compatible(req) && v.is_active())
                            .collect();

                        if compatible.is_empty() {
                            return Err(VersionError::NotFound(format!(
                                "No compatible version for {}",
                                req
                            )));
                        }

                        // Return highest compatible version
                        compatible
                            .into_iter()
                            .max_by(|a, b| a.version.cmp(&b.version))
                            .cloned()
                            .ok_or_else(|| VersionError::NotFound(key))
                    }
                    NegotiationStrategy::Latest | NegotiationStrategy::Any => {
                        // Return latest stable version
                        versions
                            .values()
                            .filter(|v| v.is_active() && v.version.is_stable())
                            .max_by(|a, b| a.version.cmp(&b.version))
                            .cloned()
                            .ok_or_else(|| VersionError::NotFound("No active versions".to_string()))
                    }
                }
            }
            None => {
                // Use default or latest
                let default = self.default_version.read().await;
                if let Some(ref def) = *default {
                    let key = def.to_string();
                    return versions
                        .get(&key)
                        .cloned()
                        .ok_or_else(|| VersionError::NotFound(key));
                }

                // Return latest stable
                versions
                    .values()
                    .filter(|v| v.is_active() && v.version.is_stable())
                    .max_by(|a, b| a.version.cmp(&b.version))
                    .cloned()
                    .ok_or_else(|| VersionError::NotFound("No active versions".to_string()))
            }
        }
    }
}

impl Default for VersionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Version extractor from requests.
pub struct VersionExtractor {
    header_name: String,
    query_param: String,
    path_prefix: String,
}

impl VersionExtractor {
    /// Create a new version extractor with defaults.
    pub fn new() -> Self {
        Self {
            header_name: "X-API-Version".to_string(),
            query_param: "api_version".to_string(),
            path_prefix: "/v".to_string(),
        }
    }

    /// Set the header name.
    pub fn header_name(mut self, name: impl Into<String>) -> Self {
        self.header_name = name.into();
        self
    }

    /// Set the query parameter name.
    pub fn query_param(mut self, param: impl Into<String>) -> Self {
        self.query_param = param.into();
        self
    }

    /// Set the path prefix.
    pub fn path_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.path_prefix = prefix.into();
        self
    }

    /// Extract version from header value.
    pub fn from_header(&self, value: &str) -> Result<Version> {
        Version::parse(value)
    }

    /// Extract version from path.
    pub fn from_path(&self, path: &str) -> Option<Result<Version>> {
        // Look for /v1, /v2.0, /v1.2.3 patterns
        let re = Regex::new(r"/v(\d+(?:\.\d+(?:\.\d+)?)?)").ok()?;

        re.captures(path).map(|caps| {
            let version_str = &caps[1];
            // Normalize to full semver
            let parts: Vec<&str> = version_str.split('.').collect();
            let normalized = match parts.len() {
                1 => format!("{}.0.0", parts[0]),
                2 => format!("{}.{}.0", parts[0], parts[1]),
                _ => version_str.to_string(),
            };
            Version::parse(&normalized)
        })
    }

    /// Extract version from query string.
    pub fn from_query(&self, query: &str) -> Option<Result<Version>> {
        for pair in query.split('&') {
            let parts: Vec<&str> = pair.split('=').collect();
            if parts.len() == 2 && parts[0] == self.query_param {
                return Some(Version::parse(parts[1]));
            }
        }
        None
    }

    /// Get the header name.
    pub fn get_header_name(&self) -> &str {
        &self.header_name
    }
}

impl Default for VersionExtractor {
    fn default() -> Self {
        Self::new()
    }
}

/// Deprecation warning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeprecationWarning {
    /// The deprecated version.
    pub version: Version,
    /// Warning message.
    pub message: String,
    /// Sunset date.
    pub sunset_date: Option<DateTime<Utc>>,
    /// Recommended version to migrate to.
    pub recommended: Option<Version>,
    /// Migration guide URL.
    pub migration_url: Option<String>,
}

impl DeprecationWarning {
    /// Create a new deprecation warning.
    pub fn new(version: Version, message: impl Into<String>) -> Self {
        Self {
            version,
            message: message.into(),
            sunset_date: None,
            recommended: None,
            migration_url: None,
        }
    }

    /// Set sunset date.
    pub fn with_sunset(mut self, date: DateTime<Utc>) -> Self {
        self.sunset_date = Some(date);
        self
    }

    /// Set recommended version.
    pub fn with_recommended(mut self, version: Version) -> Self {
        self.recommended = Some(version);
        self
    }

    /// Set migration URL.
    pub fn with_migration_url(mut self, url: impl Into<String>) -> Self {
        self.migration_url = Some(url.into());
        self
    }

    /// Format as HTTP Deprecation header.
    pub fn to_header(&self) -> String {
        match self.sunset_date {
            Some(date) => format!("sunset=\"{}\"", date.to_rfc2822()),
            None => "true".to_string(),
        }
    }

    /// Format as HTTP Link header for migration.
    pub fn to_link_header(&self) -> Option<String> {
        self.migration_url
            .as_ref()
            .map(|url| format!("<{}>; rel=\"deprecation\"", url))
    }
}

/// Trait for version migrations.
#[async_trait]
pub trait VersionMigration: Send + Sync {
    /// Source version.
    fn from_version(&self) -> &Version;

    /// Target version.
    fn to_version(&self) -> &Version;

    /// Migrate data from source to target version.
    async fn migrate(&self, data: serde_json::Value) -> Result<serde_json::Value>;

    /// Reverse migration (if supported).
    async fn reverse(&self, data: serde_json::Value) -> Result<serde_json::Value>;
}

/// Migration chain for complex version upgrades.
pub struct MigrationChain {
    migrations: Vec<Arc<dyn VersionMigration>>,
}

impl MigrationChain {
    /// Create a new migration chain.
    pub fn new() -> Self {
        Self {
            migrations: Vec::new(),
        }
    }

    /// Add a migration to the chain.
    pub fn add(mut self, migration: Arc<dyn VersionMigration>) -> Self {
        self.migrations.push(migration);
        self
    }

    /// Find migration path between versions.
    pub fn find_path(
        &self,
        from: &Version,
        to: &Version,
    ) -> Option<Vec<&Arc<dyn VersionMigration>>> {
        if from == to {
            return Some(Vec::new());
        }

        // Build graph and find path (simple BFS for now)
        let mut path = Vec::new();
        let mut current = from.clone();

        while &current != to {
            let next = self
                .migrations
                .iter()
                .find(|m| m.from_version() == &current);

            match next {
                Some(m) => {
                    current = m.to_version().clone();
                    path.push(m);
                }
                None => return None,
            }
        }

        Some(path)
    }

    /// Execute migration chain.
    pub async fn migrate(
        &self,
        data: serde_json::Value,
        from: &Version,
        to: &Version,
    ) -> Result<serde_json::Value> {
        let path = self.find_path(from, to).ok_or_else(|| {
            VersionError::MigrationError(format!("No migration path from {} to {}", from, to))
        })?;

        let mut result = data;
        for migration in path {
            result = migration.migrate(result).await?;
        }

        Ok(result)
    }
}

impl Default for MigrationChain {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_parse() {
        let v = Version::parse("1.2.3").unwrap();
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 2);
        assert_eq!(v.patch, 3);
        assert!(v.prerelease.is_none());

        let v = Version::parse("v2.0.0-beta.1").unwrap();
        assert_eq!(v.major, 2);
        assert_eq!(v.prerelease, Some("beta.1".to_string()));
    }

    #[test]
    fn test_version_ordering() {
        let v1 = Version::new(1, 0, 0);
        let v2 = Version::new(1, 1, 0);
        let v3 = Version::new(2, 0, 0);
        let v1_alpha = Version::new(1, 0, 0).with_prerelease("alpha");

        assert!(v1 < v2);
        assert!(v2 < v3);
        assert!(v1_alpha < v1); // Pre-release < stable
    }

    #[test]
    fn test_version_compatibility() {
        let v1 = Version::new(1, 0, 0);
        let v1_1 = Version::new(1, 1, 0);
        let v2 = Version::new(2, 0, 0);

        assert!(v1.is_compatible(&v1_1));
        assert!(!v1.is_compatible(&v2));
    }

    #[test]
    fn test_version_display() {
        let v = Version::new(1, 2, 3);
        assert_eq!(v.to_string(), "1.2.3");

        let v = Version::new(2, 0, 0).with_prerelease("rc.1");
        assert_eq!(v.to_string(), "2.0.0-rc.1");
    }

    #[tokio::test]
    async fn test_version_registry() {
        let registry = VersionRegistry::new();

        let v1 = VersionInfo::new(Version::new(1, 0, 0), Utc::now())
            .with_status(VersionStatus::Supported);
        let v2 =
            VersionInfo::new(Version::new(2, 0, 0), Utc::now()).with_status(VersionStatus::Current);

        registry.register(v1).await;
        registry.register(v2).await;

        let versions = registry.list().await;
        assert_eq!(versions.len(), 2);
        assert_eq!(versions[0].version, Version::new(2, 0, 0)); // Sorted descending
    }

    #[tokio::test]
    async fn test_version_negotiation() {
        let registry = VersionRegistry::new().with_strategy(NegotiationStrategy::Compatible);

        let v1 = VersionInfo::new(Version::new(1, 0, 0), Utc::now());
        let v1_1 = VersionInfo::new(Version::new(1, 1, 0), Utc::now());
        let v2 = VersionInfo::new(Version::new(2, 0, 0), Utc::now());

        registry.register(v1).await;
        registry.register(v1_1).await;
        registry.register(v2).await;

        // Request v1.0.0, should get v1.1.0 (highest compatible)
        let result = registry
            .negotiate(Some(&Version::new(1, 0, 0)))
            .await
            .unwrap();
        assert_eq!(result.version, Version::new(1, 1, 0));

        // Request nothing, should get latest (v2.0.0)
        let result = registry.negotiate(None).await.unwrap();
        assert_eq!(result.version, Version::new(2, 0, 0));
    }

    #[test]
    fn test_version_extractor_from_path() {
        let extractor = VersionExtractor::new();

        let result = extractor.from_path("/v1/users").unwrap().unwrap();
        assert_eq!(result, Version::new(1, 0, 0));

        let result = extractor.from_path("/v2.1/items").unwrap().unwrap();
        assert_eq!(result, Version::new(2, 1, 0));

        let result = extractor.from_path("/api/users");
        assert!(result.is_none());
    }

    #[test]
    fn test_version_extractor_from_query() {
        let extractor = VersionExtractor::new();

        let result = extractor
            .from_query("api_version=1.0.0&foo=bar")
            .unwrap()
            .unwrap();
        assert_eq!(result, Version::new(1, 0, 0));

        let result = extractor.from_query("foo=bar");
        assert!(result.is_none());
    }

    #[test]
    fn test_deprecation_warning() {
        let warning = DeprecationWarning::new(Version::new(1, 0, 0), "Version 1.0 is deprecated")
            .with_recommended(Version::new(2, 0, 0));

        assert_eq!(warning.version, Version::new(1, 0, 0));
        assert_eq!(warning.recommended, Some(Version::new(2, 0, 0)));
        assert_eq!(warning.to_header(), "true");
    }

    #[test]
    fn test_version_info_status() {
        let info = VersionInfo::new(Version::new(1, 0, 0), Utc::now());
        assert!(info.is_active());
        assert_eq!(info.status, VersionStatus::Current);

        let sunset_info =
            VersionInfo::new(Version::new(0, 9, 0), Utc::now()).with_sunset(Utc::now());
        assert!(!sunset_info.is_active());
        assert_eq!(sunset_info.status, VersionStatus::Sunset);
    }
}
