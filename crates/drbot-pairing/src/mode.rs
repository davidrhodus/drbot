//! Pairing mode definitions.

use serde::{Deserialize, Serialize};

/// Pairing mode determines how senders are verified.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PairingMode {
    /// Anyone can interact (no verification required).
    #[default]
    Open,
    /// Requires an approval code to pair.
    ApprovalCode,
    /// Only senders on the allowlist can interact.
    Allowlist,
    /// Combines approval code for initial pairing with allowlist for ongoing access.
    Hybrid,
    /// Disabled - no new pairings allowed.
    Disabled,
}

impl PairingMode {
    /// Check if this mode requires explicit approval.
    pub fn requires_approval(&self) -> bool {
        matches!(self, Self::ApprovalCode | Self::Hybrid)
    }

    /// Check if this mode uses an allowlist.
    pub fn uses_allowlist(&self) -> bool {
        matches!(self, Self::Allowlist | Self::Hybrid)
    }

    /// Check if new pairings are allowed.
    pub fn allows_new_pairings(&self) -> bool {
        !matches!(self, Self::Disabled)
    }

    /// Get a human-readable description.
    pub fn description(&self) -> &'static str {
        match self {
            Self::Open => "Anyone can interact without verification",
            Self::ApprovalCode => "Requires an approval code for initial pairing",
            Self::Allowlist => "Only pre-approved senders can interact",
            Self::Hybrid => "Requires approval code, then added to allowlist",
            Self::Disabled => "No new pairings allowed",
        }
    }
}

impl std::fmt::Display for PairingMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Open => write!(f, "open"),
            Self::ApprovalCode => write!(f, "approval_code"),
            Self::Allowlist => write!(f, "allowlist"),
            Self::Hybrid => write!(f, "hybrid"),
            Self::Disabled => write!(f, "disabled"),
        }
    }
}

impl std::str::FromStr for PairingMode {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "open" => Ok(Self::Open),
            "approval_code" | "approvalcode" | "code" => Ok(Self::ApprovalCode),
            "allowlist" | "whitelist" => Ok(Self::Allowlist),
            "hybrid" => Ok(Self::Hybrid),
            "disabled" | "off" => Ok(Self::Disabled),
            _ => Err(format!("Unknown pairing mode: {}", s)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pairing_mode_properties() {
        assert!(!PairingMode::Open.requires_approval());
        assert!(PairingMode::ApprovalCode.requires_approval());
        assert!(!PairingMode::Allowlist.requires_approval());
        assert!(PairingMode::Hybrid.requires_approval());

        assert!(!PairingMode::Open.uses_allowlist());
        assert!(!PairingMode::ApprovalCode.uses_allowlist());
        assert!(PairingMode::Allowlist.uses_allowlist());
        assert!(PairingMode::Hybrid.uses_allowlist());

        assert!(PairingMode::Open.allows_new_pairings());
        assert!(!PairingMode::Disabled.allows_new_pairings());
    }

    #[test]
    fn test_pairing_mode_from_str() {
        assert_eq!("open".parse::<PairingMode>().unwrap(), PairingMode::Open);
        assert_eq!(
            "approval_code".parse::<PairingMode>().unwrap(),
            PairingMode::ApprovalCode
        );
        assert_eq!(
            "allowlist".parse::<PairingMode>().unwrap(),
            PairingMode::Allowlist
        );
        assert_eq!(
            "hybrid".parse::<PairingMode>().unwrap(),
            PairingMode::Hybrid
        );
    }
}
