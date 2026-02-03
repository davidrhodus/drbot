//! Release channel definitions.

use serde::{Deserialize, Serialize};

/// Release channel for updates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReleaseChannel {
    /// Stable releases.
    #[default]
    Stable,
    /// Beta releases.
    Beta,
    /// Development releases.
    Dev,
    /// Nightly builds.
    Nightly,
}

impl ReleaseChannel {
    /// Get the manifest URL for this channel.
    pub fn manifest_url(&self) -> &'static str {
        match self {
            Self::Stable => "https://releases.drbot.io/stable/manifest.json",
            Self::Beta => "https://releases.drbot.io/beta/manifest.json",
            Self::Dev => "https://releases.drbot.io/dev/manifest.json",
            Self::Nightly => "https://releases.drbot.io/nightly/manifest.json",
        }
    }

    /// Get the channel name.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Beta => "beta",
            Self::Dev => "dev",
            Self::Nightly => "nightly",
        }
    }

    /// Parse from string.
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "stable" => Some(Self::Stable),
            "beta" => Some(Self::Beta),
            "dev" | "development" => Some(Self::Dev),
            "nightly" => Some(Self::Nightly),
            _ => None,
        }
    }

    /// Get update frequency description.
    pub fn frequency(&self) -> &'static str {
        match self {
            Self::Stable => "Stable releases, typically monthly",
            Self::Beta => "Beta releases, typically weekly",
            Self::Dev => "Development releases, multiple times per week",
            Self::Nightly => "Nightly builds, daily",
        }
    }

    /// Whether this channel is considered stable.
    pub fn is_stable(&self) -> bool {
        matches!(self, Self::Stable)
    }

    /// Whether this channel may contain breaking changes.
    pub fn may_have_breaking_changes(&self) -> bool {
        matches!(self, Self::Dev | Self::Nightly)
    }
}

impl std::fmt::Display for ReleaseChannel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

impl std::str::FromStr for ReleaseChannel {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Self::from_str(s).ok_or_else(|| format!("Unknown release channel: {}", s))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_channel_manifest_url() {
        assert!(ReleaseChannel::Stable.manifest_url().contains("stable"));
        assert!(ReleaseChannel::Beta.manifest_url().contains("beta"));
        assert!(ReleaseChannel::Nightly.manifest_url().contains("nightly"));
    }

    #[test]
    fn test_channel_from_str() {
        assert_eq!(
            ReleaseChannel::from_str("stable"),
            Some(ReleaseChannel::Stable)
        );
        assert_eq!(ReleaseChannel::from_str("BETA"), Some(ReleaseChannel::Beta));
        assert_eq!(ReleaseChannel::from_str("dev"), Some(ReleaseChannel::Dev));
        assert_eq!(ReleaseChannel::from_str("unknown"), None);
    }

    #[test]
    fn test_channel_stability() {
        assert!(ReleaseChannel::Stable.is_stable());
        assert!(!ReleaseChannel::Nightly.is_stable());
        assert!(ReleaseChannel::Nightly.may_have_breaking_changes());
    }
}
