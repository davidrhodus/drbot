//! Update manifest format.

use crate::ReleaseChannel;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Update manifest - describes available releases.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateManifest {
    /// Manifest version.
    pub manifest_version: u32,
    /// Release channel.
    pub channel: ReleaseChannel,
    /// Latest version.
    pub latest_version: String,
    /// Minimum supported version for upgrade.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_version: Option<String>,
    /// Release date.
    pub release_date: String,
    /// Release notes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release_notes: Option<String>,
    /// Release notes URL.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release_notes_url: Option<String>,
    /// Binaries for each platform.
    pub binaries: HashMap<String, BinaryInfo>,
    /// Previous versions available for rollback.
    #[serde(default)]
    pub previous_versions: Vec<PreviousVersion>,
}

/// Binary information for a specific platform.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BinaryInfo {
    /// Download URL.
    pub url: String,
    /// SHA256 checksum.
    pub sha256: String,
    /// File size in bytes.
    pub size: u64,
    /// Target architecture.
    pub arch: String,
    /// Target OS.
    pub os: String,
    /// Optional signature URL.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature_url: Option<String>,
}

impl BinaryInfo {
    /// Get the platform key for this binary.
    pub fn platform_key(&self) -> String {
        format!("{}-{}", self.os, self.arch)
    }
}

/// Previous version information for rollback.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreviousVersion {
    /// Version string.
    pub version: String,
    /// Release date.
    pub release_date: String,
    /// Binaries for each platform.
    pub binaries: HashMap<String, BinaryInfo>,
}

impl UpdateManifest {
    /// Get binary info for the current platform.
    pub fn binary_for_current_platform(&self) -> Option<&BinaryInfo> {
        let key = current_platform_key();
        self.binaries.get(&key)
    }

    /// Check if the current version needs an update.
    pub fn needs_update(&self, current_version: &str) -> bool {
        compare_versions(current_version, &self.latest_version) < 0
    }

    /// Check if the current version is supported for upgrade.
    pub fn is_version_supported(&self, current_version: &str) -> bool {
        match &self.min_version {
            Some(min) => compare_versions(current_version, min) >= 0,
            None => true,
        }
    }
}

/// Get the platform key for the current system.
pub fn current_platform_key() -> String {
    let os = if cfg!(target_os = "macos") {
        "darwin"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "unknown"
    };

    let arch = if cfg!(target_arch = "x86_64") {
        "x86_64"
    } else if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else if cfg!(target_arch = "x86") {
        "x86"
    } else {
        "unknown"
    };

    format!("{}-{}", os, arch)
}

/// Compare two semantic version strings.
/// Returns -1 if a < b, 0 if a == b, 1 if a > b.
pub fn compare_versions(a: &str, b: &str) -> i32 {
    let parse = |s: &str| -> Vec<u32> {
        s.trim_start_matches('v')
            .split(&['.', '-'][..])
            .filter_map(|p| p.parse().ok())
            .collect()
    };

    let a_parts = parse(a);
    let b_parts = parse(b);

    for (ap, bp) in a_parts.iter().zip(b_parts.iter()) {
        match ap.cmp(bp) {
            std::cmp::Ordering::Less => return -1,
            std::cmp::Ordering::Greater => return 1,
            std::cmp::Ordering::Equal => continue,
        }
    }

    match a_parts.len().cmp(&b_parts.len()) {
        std::cmp::Ordering::Less => -1,
        std::cmp::Ordering::Greater => 1,
        std::cmp::Ordering::Equal => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compare_versions() {
        assert_eq!(compare_versions("1.0.0", "1.0.0"), 0);
        assert_eq!(compare_versions("1.0.0", "1.0.1"), -1);
        assert_eq!(compare_versions("1.0.1", "1.0.0"), 1);
        assert_eq!(compare_versions("1.1.0", "1.0.9"), 1);
        assert_eq!(compare_versions("2.0.0", "1.9.9"), 1);
        assert_eq!(compare_versions("v1.0.0", "1.0.0"), 0);
    }

    #[test]
    fn test_platform_key() {
        let key = current_platform_key();
        assert!(key.contains("-"));
    }

    #[test]
    fn test_manifest_needs_update() {
        let manifest = UpdateManifest {
            manifest_version: 1,
            channel: ReleaseChannel::Stable,
            latest_version: "1.2.0".to_string(),
            min_version: Some("1.0.0".to_string()),
            release_date: "2024-01-01".to_string(),
            release_notes: None,
            release_notes_url: None,
            binaries: HashMap::new(),
            previous_versions: vec![],
        };

        assert!(manifest.needs_update("1.0.0"));
        assert!(manifest.needs_update("1.1.0"));
        assert!(!manifest.needs_update("1.2.0"));
        assert!(!manifest.needs_update("1.3.0"));
    }

    #[test]
    fn test_manifest_version_supported() {
        let manifest = UpdateManifest {
            manifest_version: 1,
            channel: ReleaseChannel::Stable,
            latest_version: "2.0.0".to_string(),
            min_version: Some("1.5.0".to_string()),
            release_date: "2024-01-01".to_string(),
            release_notes: None,
            release_notes_url: None,
            binaries: HashMap::new(),
            previous_versions: vec![],
        };

        assert!(!manifest.is_version_supported("1.0.0"));
        assert!(manifest.is_version_supported("1.5.0"));
        assert!(manifest.is_version_supported("1.6.0"));
    }
}
