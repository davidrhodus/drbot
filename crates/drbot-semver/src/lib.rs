//! Semantic versioning for drbot.
//!
//! This crate provides:
//! - Semantic version parsing
//! - Version comparison
//! - Version ranges and constraints

use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::fmt;
use std::str::FromStr;
use thiserror::Error;

/// Semver error types.
#[derive(Error, Debug)]
pub enum SemverError {
    #[error("Invalid version: {0}")]
    InvalidVersion(String),

    #[error("Invalid constraint: {0}")]
    InvalidConstraint(String),
}

/// Result type for semver operations.
pub type Result<T> = std::result::Result<T, SemverError>;

/// Semantic version.
#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct Version {
    /// Major version number.
    pub major: u64,
    /// Minor version number.
    pub minor: u64,
    /// Patch version number.
    pub patch: u64,
    /// Pre-release identifiers.
    pub prerelease: Vec<String>,
    /// Build metadata.
    pub build: Vec<String>,
}

impl Version {
    /// Create new version.
    pub fn new(major: u64, minor: u64, patch: u64) -> Self {
        Self {
            major,
            minor,
            patch,
            prerelease: Vec::new(),
            build: Vec::new(),
        }
    }

    /// Create version with prerelease.
    pub fn with_prerelease(mut self, prerelease: &[&str]) -> Self {
        self.prerelease = prerelease.iter().map(|s| s.to_string()).collect();
        self
    }

    /// Create version with build metadata.
    pub fn with_build(mut self, build: &[&str]) -> Self {
        self.build = build.iter().map(|s| s.to_string()).collect();
        self
    }

    /// Parse version from string.
    pub fn parse(s: &str) -> Result<Self> {
        let s = s.strip_prefix('v').unwrap_or(s);

        // Split build metadata
        let (version_pre, build) = if let Some(pos) = s.find('+') {
            let (v, b) = s.split_at(pos);
            (v, Some(&b[1..]))
        } else {
            (s, None)
        };

        // Split prerelease
        let (version, prerelease) = if let Some(pos) = version_pre.find('-') {
            let (v, p) = version_pre.split_at(pos);
            (v, Some(&p[1..]))
        } else {
            (version_pre, None)
        };

        // Parse major.minor.patch
        let parts: Vec<&str> = version.split('.').collect();
        if parts.len() < 1 || parts.len() > 3 {
            return Err(SemverError::InvalidVersion(s.to_string()));
        }

        let major = parts[0]
            .parse()
            .map_err(|_| SemverError::InvalidVersion(s.to_string()))?;
        let minor = parts
            .get(1)
            .map(|s| s.parse())
            .transpose()
            .map_err(|_| SemverError::InvalidVersion(s.to_string()))?
            .unwrap_or(0);
        let patch = parts
            .get(2)
            .map(|s| s.parse())
            .transpose()
            .map_err(|_| SemverError::InvalidVersion(s.to_string()))?
            .unwrap_or(0);

        let prerelease = prerelease
            .map(|p| p.split('.').map(|s| s.to_string()).collect())
            .unwrap_or_default();

        let build = build
            .map(|b| b.split('.').map(|s| s.to_string()).collect())
            .unwrap_or_default();

        Ok(Self {
            major,
            minor,
            patch,
            prerelease,
            build,
        })
    }

    /// Check if version is prerelease.
    pub fn is_prerelease(&self) -> bool {
        !self.prerelease.is_empty()
    }

    /// Check if version is stable (>= 1.0.0 and no prerelease).
    pub fn is_stable(&self) -> bool {
        self.major >= 1 && self.prerelease.is_empty()
    }

    /// Increment major version.
    pub fn increment_major(&self) -> Self {
        Self::new(self.major + 1, 0, 0)
    }

    /// Increment minor version.
    pub fn increment_minor(&self) -> Self {
        Self::new(self.major, self.minor + 1, 0)
    }

    /// Increment patch version.
    pub fn increment_patch(&self) -> Self {
        Self::new(self.major, self.minor, self.patch + 1)
    }

    /// Check compatibility (same major version, for stable versions).
    pub fn is_compatible(&self, other: &Self) -> bool {
        if self.major == 0 || other.major == 0 {
            // Pre-1.0: minor version must match
            self.major == other.major && self.minor == other.minor
        } else {
            // Post-1.0: major version must match
            self.major == other.major
        }
    }
}

impl FromStr for Version {
    type Err = SemverError;

    fn from_str(s: &str) -> Result<Self> {
        Self::parse(s)
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if !self.prerelease.is_empty() {
            write!(f, "-{}", self.prerelease.join("."))?;
        }
        if !self.build.is_empty() {
            write!(f, "+{}", self.build.join("."))?;
        }
        Ok(())
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        // Compare major.minor.patch
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

        // Compare prerelease (empty > non-empty)
        match (self.prerelease.is_empty(), other.prerelease.is_empty()) {
            (true, false) => return Ordering::Greater,
            (false, true) => return Ordering::Less,
            (true, true) => return Ordering::Equal,
            (false, false) => {}
        }

        // Compare prerelease identifiers
        for (a, b) in self.prerelease.iter().zip(other.prerelease.iter()) {
            let cmp = match (a.parse::<u64>(), b.parse::<u64>()) {
                (Ok(a), Ok(b)) => a.cmp(&b),
                (Ok(_), Err(_)) => Ordering::Less,
                (Err(_), Ok(_)) => Ordering::Greater,
                (Err(_), Err(_)) => a.cmp(b),
            };
            if cmp != Ordering::Equal {
                return cmp;
            }
        }

        self.prerelease.len().cmp(&other.prerelease.len())
    }
}

/// Version constraint operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operator {
    /// Exact match (=).
    Exact,
    /// Greater than (>).
    Greater,
    /// Greater or equal (>=).
    GreaterEq,
    /// Less than (<).
    Less,
    /// Less or equal (<=).
    LessEq,
    /// Compatible (~=).
    Compatible,
    /// Caret (^).
    Caret,
    /// Tilde (~).
    Tilde,
    /// Wildcard (*).
    Wildcard,
}

/// Version constraint.
#[derive(Debug, Clone)]
pub struct Constraint {
    operator: Operator,
    version: Version,
}

impl Constraint {
    /// Create new constraint.
    pub fn new(operator: Operator, version: Version) -> Self {
        Self { operator, version }
    }

    /// Parse constraint from string.
    pub fn parse(s: &str) -> Result<Self> {
        let s = s.trim();

        // Check for operators
        let (op, version_str) = if s.starts_with(">=") {
            (Operator::GreaterEq, &s[2..])
        } else if s.starts_with("<=") {
            (Operator::LessEq, &s[2..])
        } else if s.starts_with("~=") {
            (Operator::Compatible, &s[2..])
        } else if s.starts_with('^') {
            (Operator::Caret, &s[1..])
        } else if s.starts_with('~') {
            (Operator::Tilde, &s[1..])
        } else if s.starts_with('>') {
            (Operator::Greater, &s[1..])
        } else if s.starts_with('<') {
            (Operator::Less, &s[1..])
        } else if s.starts_with('=') {
            (Operator::Exact, &s[1..])
        } else if s.contains('*') {
            (Operator::Wildcard, s)
        } else {
            (Operator::Exact, s)
        };

        // Handle wildcard
        if op == Operator::Wildcard {
            let version_str = version_str.replace('*', "0");
            let version = Version::parse(&version_str)?;
            return Ok(Self {
                operator: op,
                version,
            });
        }

        let version = Version::parse(version_str.trim())?;
        Ok(Self {
            operator: op,
            version,
        })
    }

    /// Check if version matches constraint.
    pub fn matches(&self, version: &Version) -> bool {
        match self.operator {
            Operator::Exact => version == &self.version,
            Operator::Greater => version > &self.version,
            Operator::GreaterEq => version >= &self.version,
            Operator::Less => version < &self.version,
            Operator::LessEq => version <= &self.version,
            Operator::Compatible => {
                version >= &self.version && version.is_compatible(&self.version)
            }
            Operator::Caret => {
                if self.version.major == 0 {
                    if self.version.minor == 0 {
                        // ^0.0.x: patch must match
                        version.major == 0
                            && version.minor == 0
                            && version.patch == self.version.patch
                    } else {
                        // ^0.y.z: minor must match
                        version.major == 0
                            && version.minor == self.version.minor
                            && version >= &self.version
                    }
                } else {
                    // ^x.y.z: major must match
                    version.major == self.version.major && version >= &self.version
                }
            }
            Operator::Tilde => {
                // ~x.y.z: major and minor must match
                version.major == self.version.major
                    && version.minor == self.version.minor
                    && version >= &self.version
            }
            Operator::Wildcard => {
                // x.y.*: major and minor must match
                version.major == self.version.major && version.minor == self.version.minor
            }
        }
    }
}

impl FromStr for Constraint {
    type Err = SemverError;

    fn from_str(s: &str) -> Result<Self> {
        Self::parse(s)
    }
}

impl fmt::Display for Constraint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let op = match self.operator {
            Operator::Exact => "=",
            Operator::Greater => ">",
            Operator::GreaterEq => ">=",
            Operator::Less => "<",
            Operator::LessEq => "<=",
            Operator::Compatible => "~=",
            Operator::Caret => "^",
            Operator::Tilde => "~",
            Operator::Wildcard => "",
        };
        write!(f, "{}{}", op, self.version)
    }
}

/// Version range (multiple constraints).
#[derive(Debug, Clone)]
pub struct VersionRange {
    constraints: Vec<Constraint>,
}

impl VersionRange {
    /// Create new version range.
    pub fn new() -> Self {
        Self {
            constraints: Vec::new(),
        }
    }

    /// Create from single constraint.
    pub fn from_constraint(constraint: Constraint) -> Self {
        Self {
            constraints: vec![constraint],
        }
    }

    /// Parse version range from string.
    pub fn parse(s: &str) -> Result<Self> {
        let mut constraints = Vec::new();

        // Split by comma or space
        for part in s.split(|c| c == ',' || c == ' ') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            constraints.push(Constraint::parse(part)?);
        }

        Ok(Self { constraints })
    }

    /// Add constraint.
    pub fn add(mut self, constraint: Constraint) -> Self {
        self.constraints.push(constraint);
        self
    }

    /// Check if version matches all constraints.
    pub fn matches(&self, version: &Version) -> bool {
        self.constraints.iter().all(|c| c.matches(version))
    }

    /// Get all constraints.
    pub fn constraints(&self) -> &[Constraint] {
        &self.constraints
    }
}

impl Default for VersionRange {
    fn default() -> Self {
        Self::new()
    }
}

impl FromStr for VersionRange {
    type Err = SemverError;

    fn from_str(s: &str) -> Result<Self> {
        Self::parse(s)
    }
}

impl fmt::Display for VersionRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let parts: Vec<_> = self.constraints.iter().map(|c| c.to_string()).collect();
        write!(f, "{}", parts.join(", "))
    }
}

/// Find the latest version matching a constraint.
pub fn find_latest<'a>(versions: &'a [Version], constraint: &Constraint) -> Option<&'a Version> {
    versions.iter().filter(|v| constraint.matches(v)).max()
}

/// Find all versions matching a constraint.
pub fn find_matching<'a>(versions: &'a [Version], constraint: &Constraint) -> Vec<&'a Version> {
    versions.iter().filter(|v| constraint.matches(v)).collect()
}

/// Sort versions in ascending order.
pub fn sort_versions(versions: &mut [Version]) {
    versions.sort();
}

/// Sort versions in descending order.
pub fn sort_versions_desc(versions: &mut [Version]) {
    versions.sort_by(|a, b| b.cmp(a));
}

// ============================================================================
// Kani Formal Verification Proofs
// ============================================================================

#[cfg(kani)]
mod kani_proofs {
    use super::*;

    // ========================================================================
    // Version Comparison Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_version_equality_reflexive() {
        let major: u64 = kani::any();
        let minor: u64 = kani::any();
        let patch: u64 = kani::any();

        kani::assume(major < 100);
        kani::assume(minor < 100);
        kani::assume(patch < 100);

        let v = Version::new(major, minor, patch);
        kani::assert!(v == v, "Version equals itself");
    }

    #[kani::proof]
    fn proof_version_comparison_major_takes_precedence() {
        let v1 = Version::new(2, 0, 0);
        let v2 = Version::new(1, 99, 99);

        kani::assert!(v1 > v2, "Higher major is greater regardless of minor/patch");
    }

    #[kani::proof]
    fn proof_version_comparison_minor_second() {
        let v1 = Version::new(1, 2, 0);
        let v2 = Version::new(1, 1, 99);

        kani::assert!(v1 > v2, "Higher minor is greater when major equal");
    }

    #[kani::proof]
    fn proof_version_comparison_patch_third() {
        let v1 = Version::new(1, 1, 2);
        let v2 = Version::new(1, 1, 1);

        kani::assert!(v1 > v2, "Higher patch is greater when major.minor equal");
    }

    #[kani::proof]
    fn proof_version_release_greater_than_prerelease() {
        let release = Version::new(1, 0, 0);
        let prerelease = Version::new(1, 0, 0).with_prerelease(&["alpha"]);

        kani::assert!(
            release > prerelease,
            "Release > prerelease with same version"
        );
    }

    #[kani::proof]
    fn proof_version_comparison_symmetric() {
        let v1 = Version::new(1, 2, 3);
        let v2 = Version::new(1, 2, 4);

        let cmp1 = v1 < v2;
        let cmp2 = v2 > v1;

        kani::assert!(cmp1 == cmp2, "Comparison is symmetric");
    }

    // ========================================================================
    // Version Increment Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_increment_major_resets_minor_patch() {
        let major: u64 = kani::any();
        let minor: u64 = kani::any();
        let patch: u64 = kani::any();

        kani::assume(major < u64::MAX);
        kani::assume(minor < 100);
        kani::assume(patch < 100);

        let v = Version::new(major, minor, patch);
        let incremented = v.increment_major();

        kani::assert!(incremented.major == major + 1, "Major incremented");
        kani::assert!(incremented.minor == 0, "Minor reset to 0");
        kani::assert!(incremented.patch == 0, "Patch reset to 0");
    }

    #[kani::proof]
    fn proof_increment_minor_resets_patch() {
        let major: u64 = kani::any();
        let minor: u64 = kani::any();
        let patch: u64 = kani::any();

        kani::assume(major < 100);
        kani::assume(minor < u64::MAX);
        kani::assume(patch < 100);

        let v = Version::new(major, minor, patch);
        let incremented = v.increment_minor();

        kani::assert!(incremented.major == major, "Major unchanged");
        kani::assert!(incremented.minor == minor + 1, "Minor incremented");
        kani::assert!(incremented.patch == 0, "Patch reset to 0");
    }

    #[kani::proof]
    fn proof_increment_patch_only() {
        let major: u64 = kani::any();
        let minor: u64 = kani::any();
        let patch: u64 = kani::any();

        kani::assume(major < 100);
        kani::assume(minor < 100);
        kani::assume(patch < u64::MAX);

        let v = Version::new(major, minor, patch);
        let incremented = v.increment_patch();

        kani::assert!(incremented.major == major, "Major unchanged");
        kani::assert!(incremented.minor == minor, "Minor unchanged");
        kani::assert!(incremented.patch == patch + 1, "Patch incremented");
    }

    #[kani::proof]
    fn proof_increment_produces_greater_version() {
        let v = Version::new(1, 2, 3);

        kani::assert!(v.increment_major() > v, "increment_major > original");
        kani::assert!(v.increment_minor() > v, "increment_minor > original");
        kani::assert!(v.increment_patch() > v, "increment_patch > original");
    }

    // ========================================================================
    // Version Stability Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_is_stable_requires_major_ge_1() {
        let major: u64 = kani::any();
        kani::assume(major < 10);

        let v = Version::new(major, 0, 0);

        if major >= 1 {
            kani::assert!(v.is_stable(), "Version >= 1.0.0 is stable");
        } else {
            kani::assert!(!v.is_stable(), "Version < 1.0.0 is not stable");
        }
    }

    #[kani::proof]
    fn proof_is_stable_requires_no_prerelease() {
        let v_stable = Version::new(1, 0, 0);
        let v_prerelease = Version::new(1, 0, 0).with_prerelease(&["alpha"]);

        kani::assert!(v_stable.is_stable(), "1.0.0 is stable");
        kani::assert!(!v_prerelease.is_stable(), "1.0.0-alpha is not stable");
    }

    #[kani::proof]
    fn proof_is_prerelease_logic() {
        let v = Version::new(1, 0, 0);
        let v_pre = Version::new(1, 0, 0).with_prerelease(&["beta"]);

        kani::assert!(!v.is_prerelease(), "Release is not prerelease");
        kani::assert!(v_pre.is_prerelease(), "Prerelease is prerelease");
    }

    // ========================================================================
    // Version Compatibility Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_compatibility_same_major_post_1() {
        let v1 = Version::new(2, 3, 4);
        let v2 = Version::new(2, 5, 6);

        kani::assert!(v1.is_compatible(&v2), "Same major (>= 1) is compatible");
    }

    #[kani::proof]
    fn proof_compatibility_different_major_post_1() {
        let v1 = Version::new(2, 0, 0);
        let v2 = Version::new(3, 0, 0);

        kani::assert!(
            !v1.is_compatible(&v2),
            "Different major (>= 1) is incompatible"
        );
    }

    #[kani::proof]
    fn proof_compatibility_0_x_requires_same_minor() {
        let v1 = Version::new(0, 2, 3);
        let v2 = Version::new(0, 2, 5);
        let v3 = Version::new(0, 3, 0);

        kani::assert!(v1.is_compatible(&v2), "Same 0.minor is compatible");
        kani::assert!(!v1.is_compatible(&v3), "Different 0.minor is incompatible");
    }

    // ========================================================================
    // Operator Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_operator_variants() {
        let exact = Operator::Exact;
        let greater = Operator::Greater;
        let greater_eq = Operator::GreaterEq;
        let less = Operator::Less;
        let less_eq = Operator::LessEq;

        // All are distinct
        kani::assert!(exact != greater, "Exact != Greater");
        kani::assert!(greater != greater_eq, "Greater != GreaterEq");
        kani::assert!(less != less_eq, "Less != LessEq");
        kani::assert!(exact != less, "Exact != Less");
    }

    // ========================================================================
    // Constraint Matching Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_constraint_exact_match() {
        let constraint = Constraint::new(Operator::Exact, Version::new(1, 2, 3));

        let matching = Version::new(1, 2, 3);
        let not_matching = Version::new(1, 2, 4);

        kani::assert!(constraint.matches(&matching), "Exact matches same version");
        kani::assert!(
            !constraint.matches(&not_matching),
            "Exact doesn't match different"
        );
    }

    #[kani::proof]
    fn proof_constraint_greater() {
        let constraint = Constraint::new(Operator::Greater, Version::new(1, 0, 0));

        let v_greater = Version::new(1, 0, 1);
        let v_equal = Version::new(1, 0, 0);
        let v_less = Version::new(0, 9, 9);

        kani::assert!(
            constraint.matches(&v_greater),
            "Greater matches higher version"
        );
        kani::assert!(!constraint.matches(&v_equal), "Greater doesn't match equal");
        kani::assert!(!constraint.matches(&v_less), "Greater doesn't match lower");
    }

    #[kani::proof]
    fn proof_constraint_greater_eq() {
        let constraint = Constraint::new(Operator::GreaterEq, Version::new(1, 0, 0));

        let v_greater = Version::new(1, 0, 1);
        let v_equal = Version::new(1, 0, 0);
        let v_less = Version::new(0, 9, 9);

        kani::assert!(constraint.matches(&v_greater), "GreaterEq matches higher");
        kani::assert!(constraint.matches(&v_equal), "GreaterEq matches equal");
        kani::assert!(
            !constraint.matches(&v_less),
            "GreaterEq doesn't match lower"
        );
    }

    #[kani::proof]
    fn proof_constraint_less() {
        let constraint = Constraint::new(Operator::Less, Version::new(2, 0, 0));

        let v_less = Version::new(1, 9, 9);
        let v_equal = Version::new(2, 0, 0);
        let v_greater = Version::new(2, 0, 1);

        kani::assert!(constraint.matches(&v_less), "Less matches lower version");
        kani::assert!(!constraint.matches(&v_equal), "Less doesn't match equal");
        kani::assert!(!constraint.matches(&v_greater), "Less doesn't match higher");
    }

    #[kani::proof]
    fn proof_constraint_less_eq() {
        let constraint = Constraint::new(Operator::LessEq, Version::new(2, 0, 0));

        let v_less = Version::new(1, 9, 9);
        let v_equal = Version::new(2, 0, 0);
        let v_greater = Version::new(2, 0, 1);

        kani::assert!(constraint.matches(&v_less), "LessEq matches lower");
        kani::assert!(constraint.matches(&v_equal), "LessEq matches equal");
        kani::assert!(
            !constraint.matches(&v_greater),
            "LessEq doesn't match higher"
        );
    }

    #[kani::proof]
    fn proof_constraint_tilde() {
        let constraint = Constraint::new(Operator::Tilde, Version::new(1, 2, 3));

        let v_patch_higher = Version::new(1, 2, 9);
        let v_minor_higher = Version::new(1, 3, 0);

        kani::assert!(
            constraint.matches(&v_patch_higher),
            "Tilde allows patch increase"
        );
        kani::assert!(
            !constraint.matches(&v_minor_higher),
            "Tilde doesn't allow minor increase"
        );
    }

    #[kani::proof]
    fn proof_constraint_caret_stable() {
        let constraint = Constraint::new(Operator::Caret, Version::new(1, 2, 3));

        let v_minor_higher = Version::new(1, 5, 0);
        let v_major_higher = Version::new(2, 0, 0);

        kani::assert!(
            constraint.matches(&v_minor_higher),
            "Caret allows minor increase (stable)"
        );
        kani::assert!(
            !constraint.matches(&v_major_higher),
            "Caret doesn't allow major increase"
        );
    }

    // ========================================================================
    // VersionRange Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_version_range_empty_matches_all() {
        let range = VersionRange::new();
        let v = Version::new(1, 0, 0);

        kani::assert!(range.matches(&v), "Empty range matches all");
    }

    #[kani::proof]
    fn proof_version_range_single_constraint() {
        let constraint = Constraint::new(Operator::GreaterEq, Version::new(1, 0, 0));
        let range = VersionRange::from_constraint(constraint);

        let v_match = Version::new(1, 5, 0);
        let v_no_match = Version::new(0, 9, 0);

        kani::assert!(range.matches(&v_match), "Range with one constraint matches");
        kani::assert!(!range.matches(&v_no_match), "Range rejects non-matching");
    }

    #[kani::proof]
    fn proof_version_range_multiple_constraints() {
        let c1 = Constraint::new(Operator::GreaterEq, Version::new(1, 0, 0));
        let c2 = Constraint::new(Operator::Less, Version::new(2, 0, 0));
        let range = VersionRange::new().add(c1).add(c2);

        let v_in_range = Version::new(1, 5, 0);
        let v_too_low = Version::new(0, 9, 0);
        let v_too_high = Version::new(2, 0, 0);

        kani::assert!(range.matches(&v_in_range), "In range matches");
        kani::assert!(!range.matches(&v_too_low), "Too low doesn't match");
        kani::assert!(!range.matches(&v_too_high), "Too high doesn't match");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_version() {
        let v = Version::parse("1.2.3").unwrap();
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 2);
        assert_eq!(v.patch, 3);

        let v = Version::parse("v1.2.3").unwrap();
        assert_eq!(v.major, 1);

        let v = Version::parse("1.2.3-alpha.1").unwrap();
        assert_eq!(v.prerelease, vec!["alpha", "1"]);

        let v = Version::parse("1.2.3+build.123").unwrap();
        assert_eq!(v.build, vec!["build", "123"]);

        let v = Version::parse("1.2.3-beta.1+build").unwrap();
        assert_eq!(v.prerelease, vec!["beta", "1"]);
        assert_eq!(v.build, vec!["build"]);
    }

    #[test]
    fn test_version_comparison() {
        assert!(Version::new(1, 0, 0) > Version::new(0, 9, 9));
        assert!(Version::new(1, 1, 0) > Version::new(1, 0, 9));
        assert!(Version::new(1, 0, 1) > Version::new(1, 0, 0));

        // Prerelease is lower than release
        let release = Version::new(1, 0, 0);
        let prerelease = Version::new(1, 0, 0).with_prerelease(&["alpha"]);
        assert!(release > prerelease);

        // Numeric prerelease comparison
        let alpha1 = Version::new(1, 0, 0).with_prerelease(&["alpha", "1"]);
        let alpha2 = Version::new(1, 0, 0).with_prerelease(&["alpha", "2"]);
        assert!(alpha2 > alpha1);
    }

    #[test]
    fn test_constraint_exact() {
        let c = Constraint::parse("=1.0.0").unwrap();
        assert!(c.matches(&Version::new(1, 0, 0)));
        assert!(!c.matches(&Version::new(1, 0, 1)));
    }

    #[test]
    fn test_constraint_greater() {
        let c = Constraint::parse(">1.0.0").unwrap();
        assert!(!c.matches(&Version::new(1, 0, 0)));
        assert!(c.matches(&Version::new(1, 0, 1)));
        assert!(c.matches(&Version::new(2, 0, 0)));
    }

    #[test]
    fn test_constraint_caret() {
        let c = Constraint::parse("^1.2.3").unwrap();
        assert!(c.matches(&Version::new(1, 2, 3)));
        assert!(c.matches(&Version::new(1, 9, 0)));
        assert!(!c.matches(&Version::new(2, 0, 0)));
        assert!(!c.matches(&Version::new(1, 2, 2)));

        // ^0.2.3 should allow 0.2.x
        let c = Constraint::parse("^0.2.3").unwrap();
        assert!(c.matches(&Version::new(0, 2, 5)));
        assert!(!c.matches(&Version::new(0, 3, 0)));
    }

    #[test]
    fn test_constraint_tilde() {
        let c = Constraint::parse("~1.2.3").unwrap();
        assert!(c.matches(&Version::new(1, 2, 3)));
        assert!(c.matches(&Version::new(1, 2, 9)));
        assert!(!c.matches(&Version::new(1, 3, 0)));
    }

    #[test]
    fn test_version_range() {
        let range = VersionRange::parse(">=1.0.0, <2.0.0").unwrap();
        assert!(range.matches(&Version::new(1, 0, 0)));
        assert!(range.matches(&Version::new(1, 5, 0)));
        assert!(!range.matches(&Version::new(2, 0, 0)));
        assert!(!range.matches(&Version::new(0, 9, 0)));
    }

    #[test]
    fn test_version_display() {
        let v = Version::new(1, 2, 3)
            .with_prerelease(&["alpha"])
            .with_build(&["123"]);
        assert_eq!(v.to_string(), "1.2.3-alpha+123");
    }

    #[test]
    fn test_increment() {
        let v = Version::new(1, 2, 3);
        assert_eq!(v.increment_major(), Version::new(2, 0, 0));
        assert_eq!(v.increment_minor(), Version::new(1, 3, 0));
        assert_eq!(v.increment_patch(), Version::new(1, 2, 4));
    }

    #[test]
    fn test_is_stable() {
        assert!(Version::new(1, 0, 0).is_stable());
        assert!(!Version::new(0, 9, 0).is_stable());
        assert!(!Version::new(1, 0, 0).with_prerelease(&["beta"]).is_stable());
    }

    #[test]
    fn test_find_latest() {
        let versions = vec![
            Version::new(1, 0, 0),
            Version::new(1, 1, 0),
            Version::new(2, 0, 0),
            Version::new(1, 2, 0),
        ];

        let c = Constraint::parse("^1.0.0").unwrap();
        let latest = find_latest(&versions, &c).unwrap();
        assert_eq!(latest, &Version::new(1, 2, 0));
    }
}
