//! Upgrade detection and risk assessment.
//!
//! Analyzes program upgrades and provides risk assessments.

use super::program_watcher::ProgramEvent;
use crate::risk::AlertSeverity;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;
use std::collections::HashMap;

/// Upgrade event with risk assessment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpgradeEvent {
    /// Program that was upgraded.
    pub program: Pubkey,
    /// Program name.
    pub name: String,
    /// Type of upgrade event.
    pub event_type: UpgradeEventType,
    /// Risk assessment.
    pub risk_assessment: UpgradeRisk,
    /// When the upgrade occurred.
    pub timestamp: DateTime<Utc>,
    /// Original event data.
    pub raw_event: ProgramEvent,
}

/// Types of upgrade events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpgradeEventType {
    /// Program code was upgraded.
    ProgramUpgraded,
    /// Upgrade authority was changed.
    UpgradeAuthorityChanged,
    /// Upgrade buffer was created (potential upgrade incoming).
    BufferCreated,
    /// Program was made immutable.
    ImmutableSet,
}

/// Risk assessment for an upgrade.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpgradeRisk {
    /// Overall risk level.
    pub level: RiskLevel,
    /// Risk factors identified.
    pub factors: Vec<RiskFactor>,
    /// Recommended actions.
    pub recommendations: Vec<String>,
    /// Risk score (0-100).
    pub score: u8,
}

impl UpgradeRisk {
    /// Create a low risk assessment.
    pub fn low(factors: Vec<RiskFactor>, recommendations: Vec<String>) -> Self {
        Self {
            level: RiskLevel::Low,
            factors,
            recommendations,
            score: 20,
        }
    }

    /// Create a medium risk assessment.
    pub fn medium(factors: Vec<RiskFactor>, recommendations: Vec<String>) -> Self {
        Self {
            level: RiskLevel::Medium,
            factors,
            recommendations,
            score: 50,
        }
    }

    /// Create a high risk assessment.
    pub fn high(factors: Vec<RiskFactor>, recommendations: Vec<String>) -> Self {
        Self {
            level: RiskLevel::High,
            factors,
            recommendations,
            score: 75,
        }
    }

    /// Create a critical risk assessment.
    pub fn critical(factors: Vec<RiskFactor>, recommendations: Vec<String>) -> Self {
        Self {
            level: RiskLevel::Critical,
            factors,
            recommendations,
            score: 95,
        }
    }
}

/// Risk level classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    /// Low risk - routine upgrade.
    Low,
    /// Medium risk - requires attention.
    Medium,
    /// High risk - action recommended.
    High,
    /// Critical risk - immediate action needed.
    Critical,
}

impl From<RiskLevel> for AlertSeverity {
    fn from(level: RiskLevel) -> Self {
        match level {
            RiskLevel::Low => AlertSeverity::Low,
            RiskLevel::Medium => AlertSeverity::Medium,
            RiskLevel::High => AlertSeverity::High,
            RiskLevel::Critical => AlertSeverity::Critical,
        }
    }
}

/// Individual risk factors.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskFactor {
    /// Factor name.
    pub name: String,
    /// Factor description.
    pub description: String,
    /// Factor severity.
    pub severity: FactorSeverity,
    /// Weight in overall score.
    pub weight: f64,
}

impl RiskFactor {
    /// Create a new risk factor.
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        severity: FactorSeverity,
    ) -> Self {
        let weight = match severity {
            FactorSeverity::Low => 0.1,
            FactorSeverity::Medium => 0.25,
            FactorSeverity::High => 0.5,
            FactorSeverity::Critical => 1.0,
        };

        Self {
            name: name.into(),
            description: description.into(),
            severity,
            weight,
        }
    }
}

/// Severity of individual factors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FactorSeverity {
    Low,
    Medium,
    High,
    Critical,
}

/// Upgrade detector that analyzes program changes.
pub struct UpgradeDetector {
    /// Known safe upgrade authorities.
    trusted_authorities: HashMap<Pubkey, String>,
    /// Programs with recent upgrades.
    recent_upgrades: HashMap<Pubkey, Vec<UpgradeEvent>>,
    /// Configuration.
    config: UpgradeDetectorConfig,
}

/// Configuration for upgrade detection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpgradeDetectorConfig {
    /// How long to consider an upgrade "recent" (hours).
    pub recent_window_hours: u64,
    /// Maximum upgrades in window before alert.
    pub max_upgrades_in_window: usize,
    /// Whether to trust known protocol authorities.
    pub trust_known_authorities: bool,
}

impl Default for UpgradeDetectorConfig {
    fn default() -> Self {
        Self {
            recent_window_hours: 24,
            max_upgrades_in_window: 2,
            trust_known_authorities: true,
        }
    }
}

impl UpgradeDetector {
    /// Create a new upgrade detector.
    pub fn new(config: UpgradeDetectorConfig) -> Self {
        let mut trusted = HashMap::new();

        // Add known protocol multisigs/authorities
        // These would be populated from a registry in production

        Self {
            trusted_authorities: trusted,
            recent_upgrades: HashMap::new(),
            config,
        }
    }

    /// Add a trusted authority.
    pub fn add_trusted_authority(&mut self, authority: Pubkey, name: String) {
        self.trusted_authorities.insert(authority, name);
    }

    /// Analyze a program event and return upgrade assessment.
    pub fn analyze_event(&mut self, event: &ProgramEvent) -> Option<UpgradeEvent> {
        match event {
            ProgramEvent::ProgramUpgraded {
                program,
                name,
                old_slot,
                new_slot,
                old_hash,
                new_hash,
                timestamp,
            } => {
                let risk = self.assess_program_upgrade(program, name, *old_slot, *new_slot);

                let upgrade_event = UpgradeEvent {
                    program: *program,
                    name: name.clone(),
                    event_type: UpgradeEventType::ProgramUpgraded,
                    risk_assessment: risk,
                    timestamp: *timestamp,
                    raw_event: event.clone(),
                };

                self.record_upgrade(&upgrade_event);

                Some(upgrade_event)
            }

            ProgramEvent::UpgradeAuthorityChanged {
                program,
                name,
                old_authority,
                new_authority,
                timestamp,
            } => {
                let risk = self.assess_authority_change(
                    program,
                    name,
                    old_authority.as_ref(),
                    new_authority.as_ref(),
                );

                let upgrade_event = UpgradeEvent {
                    program: *program,
                    name: name.clone(),
                    event_type: UpgradeEventType::UpgradeAuthorityChanged,
                    risk_assessment: risk,
                    timestamp: *timestamp,
                    raw_event: event.clone(),
                };

                Some(upgrade_event)
            }

            ProgramEvent::ImmutableSet {
                program,
                name,
                timestamp,
            } => {
                // Making a program immutable is generally low risk
                let risk = UpgradeRisk::low(
                    vec![RiskFactor::new(
                        "Immutable",
                        "Program can no longer be upgraded",
                        FactorSeverity::Low,
                    )],
                    vec![
                        "This is generally positive for security".to_string(),
                        "Verify you don't need future upgrades".to_string(),
                    ],
                );

                let upgrade_event = UpgradeEvent {
                    program: *program,
                    name: name.clone(),
                    event_type: UpgradeEventType::ImmutableSet,
                    risk_assessment: risk,
                    timestamp: *timestamp,
                    raw_event: event.clone(),
                };

                Some(upgrade_event)
            }

            ProgramEvent::BufferCreated {
                program,
                buffer,
                authority,
                timestamp,
            } => {
                let risk = self.assess_buffer_creation(program, buffer, authority);

                let upgrade_event = UpgradeEvent {
                    program: *program,
                    name: String::new(),
                    event_type: UpgradeEventType::BufferCreated,
                    risk_assessment: risk,
                    timestamp: *timestamp,
                    raw_event: event.clone(),
                };

                Some(upgrade_event)
            }
        }
    }

    /// Assess risk of a program upgrade.
    fn assess_program_upgrade(
        &self,
        program: &Pubkey,
        name: &str,
        old_slot: u64,
        new_slot: u64,
    ) -> UpgradeRisk {
        let mut factors = Vec::new();
        let mut recommendations = Vec::new();

        // Check upgrade frequency
        let recent_count = self.get_recent_upgrade_count(program);
        if recent_count >= self.config.max_upgrades_in_window {
            factors.push(RiskFactor::new(
                "Frequent Upgrades",
                format!(
                    "Program upgraded {} times in the last {} hours",
                    recent_count, self.config.recent_window_hours
                ),
                FactorSeverity::High,
            ));
            recommendations.push("Monitor closely for suspicious behavior".to_string());
        }

        // Check slot delta (how big the jump)
        let slot_delta = new_slot - old_slot;
        if slot_delta < 1000 {
            factors.push(RiskFactor::new(
                "Quick Succession",
                "Upgrade occurred very recently after last deployment",
                FactorSeverity::Medium,
            ));
        }

        // Determine overall risk level
        let level = if factors
            .iter()
            .any(|f| f.severity == FactorSeverity::Critical)
        {
            RiskLevel::Critical
        } else if factors.iter().any(|f| f.severity == FactorSeverity::High) {
            RiskLevel::High
        } else if factors.iter().any(|f| f.severity == FactorSeverity::Medium) {
            RiskLevel::Medium
        } else {
            RiskLevel::Low
        };

        // Default recommendations
        recommendations.push("Review changelog and audit reports".to_string());
        recommendations.push("Consider reducing exposure temporarily".to_string());

        let score = self.calculate_score(&factors);
        UpgradeRisk {
            level,
            factors,
            recommendations,
            score,
        }
    }

    /// Assess risk of authority change.
    fn assess_authority_change(
        &self,
        program: &Pubkey,
        name: &str,
        old_authority: Option<&Pubkey>,
        new_authority: Option<&Pubkey>,
    ) -> UpgradeRisk {
        let mut factors = Vec::new();
        let mut recommendations = Vec::new();

        // Check if authority changed to unknown entity
        if let Some(new_auth) = new_authority {
            if !self.trusted_authorities.contains_key(new_auth) {
                factors.push(RiskFactor::new(
                    "Unknown Authority",
                    "Upgrade authority changed to an unknown address",
                    FactorSeverity::High,
                ));
                recommendations.push("Verify the new authority is legitimate".to_string());
            }
        }

        // Check if authority was removed (made immutable)
        if old_authority.is_some() && new_authority.is_none() {
            factors.push(RiskFactor::new(
                "Authority Removed",
                "Program is now immutable",
                FactorSeverity::Low,
            ));
            recommendations.push("Verify this was intentional".to_string());
        }

        // Sudden authority change is suspicious
        if old_authority.is_some() && new_authority.is_some() {
            factors.push(RiskFactor::new(
                "Authority Transfer",
                "Upgrade authority transferred to a new address",
                FactorSeverity::High,
            ));
            recommendations.push("Confirm this is a legitimate governance action".to_string());
        }

        let level = if factors
            .iter()
            .any(|f| f.severity == FactorSeverity::Critical)
        {
            RiskLevel::Critical
        } else if factors.iter().any(|f| f.severity == FactorSeverity::High) {
            RiskLevel::High
        } else {
            RiskLevel::Medium
        };

        let score = self.calculate_score(&factors);
        UpgradeRisk {
            level,
            factors,
            recommendations,
            score,
        }
    }

    /// Assess risk of buffer creation.
    fn assess_buffer_creation(
        &self,
        program: &Pubkey,
        buffer: &Pubkey,
        authority: &Pubkey,
    ) -> UpgradeRisk {
        let mut factors = Vec::new();
        let mut recommendations = Vec::new();

        factors.push(RiskFactor::new(
            "Pending Upgrade",
            "An upgrade buffer has been created, indicating an upgrade is being prepared",
            FactorSeverity::Medium,
        ));

        if !self.trusted_authorities.contains_key(authority) {
            factors.push(RiskFactor::new(
                "Unknown Buffer Authority",
                "The buffer was created by an unknown authority",
                FactorSeverity::High,
            ));
        }

        recommendations.push("Monitor for the actual upgrade".to_string());
        recommendations.push("Review any announced changes".to_string());

        let level = if factors.iter().any(|f| f.severity == FactorSeverity::High) {
            RiskLevel::High
        } else {
            RiskLevel::Medium
        };

        let score = self.calculate_score(&factors);
        UpgradeRisk {
            level,
            factors,
            recommendations,
            score,
        }
    }

    /// Record an upgrade for frequency tracking.
    fn record_upgrade(&mut self, event: &UpgradeEvent) {
        self.recent_upgrades
            .entry(event.program)
            .or_default()
            .push(event.clone());

        // Clean up old entries
        let cutoff = Utc::now() - chrono::Duration::hours(self.config.recent_window_hours as i64);

        for upgrades in self.recent_upgrades.values_mut() {
            upgrades.retain(|u| u.timestamp > cutoff);
        }
    }

    /// Get count of recent upgrades for a program.
    fn get_recent_upgrade_count(&self, program: &Pubkey) -> usize {
        let cutoff = Utc::now() - chrono::Duration::hours(self.config.recent_window_hours as i64);

        self.recent_upgrades
            .get(program)
            .map(|upgrades| upgrades.iter().filter(|u| u.timestamp > cutoff).count())
            .unwrap_or(0)
    }

    /// Calculate risk score from factors.
    fn calculate_score(&self, factors: &[RiskFactor]) -> u8 {
        if factors.is_empty() {
            return 20; // Base low risk
        }

        let total_weight: f64 = factors.iter().map(|f| f.weight).sum();
        let max_weight = factors.len() as f64; // Max if all were critical

        let normalized = (total_weight / max_weight) * 100.0;
        (normalized as u8).min(100)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_risk_factor() {
        let factor = RiskFactor::new("Test Factor", "Test description", FactorSeverity::High);

        assert_eq!(factor.weight, 0.5);
    }

    #[test]
    fn test_upgrade_risk_levels() {
        let low = UpgradeRisk::low(vec![], vec![]);
        let critical = UpgradeRisk::critical(vec![], vec![]);

        assert_eq!(low.level, RiskLevel::Low);
        assert_eq!(critical.level, RiskLevel::Critical);
        assert!(critical.score > low.score);
    }

    #[test]
    fn test_upgrade_detector() {
        let mut detector = UpgradeDetector::new(UpgradeDetectorConfig::default());

        let event = ProgramEvent::ImmutableSet {
            program: Pubkey::new_unique(),
            name: "Test".to_string(),
            timestamp: Utc::now(),
        };

        let upgrade_event = detector.analyze_event(&event);
        assert!(upgrade_event.is_some());

        let upgrade = upgrade_event.unwrap();
        assert_eq!(upgrade.event_type, UpgradeEventType::ImmutableSet);
        assert_eq!(upgrade.risk_assessment.level, RiskLevel::Low);
    }
}
