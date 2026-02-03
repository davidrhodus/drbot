//! Integration tests for Smart Contract Upgrade Monitoring.
//!
//! Tests program watching, upgrade detection, and risk assessment.

use chrono::Utc;
use drbot_solana::monitor::{
    DiffAnalyzer, FactorSeverity, RiskFactor, RiskLevel, UpgradeDetector, UpgradeDetectorConfig,
    UpgradeEventType, UpgradeRisk, WatchedProgram,
};
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;

#[test]
fn test_watched_program_creation() {
    let program = WatchedProgram {
        address: Pubkey::from_str("So1endDq2YkqhipRh3WViPa8hdiSpxWy6z3Z6tMCpAo").unwrap(),
        name: "Solend".to_string(),
        upgrade_authority: Some(Pubkey::new_unique()),
        last_deployed_slot: 100_000_000,
        data_hash: "abc123".to_string(),
        executable_data_address: None,
        is_upgradeable: true,
        added_at: Utc::now(),
        last_checked_at: Utc::now(),
    };

    assert_eq!(program.name, "Solend");
    assert!(!program.is_immutable());
    assert!(program.upgrade_authority.is_some());
}

#[test]
fn test_watched_program_immutable() {
    let program = WatchedProgram {
        address: Pubkey::new_unique(),
        name: "ImmutableProgram".to_string(),
        upgrade_authority: None,
        last_deployed_slot: 100_000_000,
        data_hash: "xyz789".to_string(),
        executable_data_address: None,
        is_upgradeable: true,
        added_at: Utc::now(),
        last_checked_at: Utc::now(),
    };

    assert!(program.is_immutable());
    assert!(program.upgrade_authority.is_none());
}

#[test]
fn test_watched_program_non_upgradeable() {
    let program = WatchedProgram {
        address: Pubkey::new_unique(),
        name: "NonUpgradeable".to_string(),
        upgrade_authority: Some(Pubkey::new_unique()),
        last_deployed_slot: 100_000_000,
        data_hash: "xyz789".to_string(),
        executable_data_address: None,
        is_upgradeable: false, // Not upgradeable
        added_at: Utc::now(),
        last_checked_at: Utc::now(),
    };

    // Even with authority, non-upgradeable is immutable
    assert!(program.is_immutable());
}

#[test]
fn test_upgrade_event_types() {
    assert!(matches!(
        UpgradeEventType::ProgramUpgraded,
        UpgradeEventType::ProgramUpgraded
    ));
    assert!(matches!(
        UpgradeEventType::UpgradeAuthorityChanged,
        UpgradeEventType::UpgradeAuthorityChanged
    ));
    assert!(matches!(
        UpgradeEventType::BufferCreated,
        UpgradeEventType::BufferCreated
    ));
    assert!(matches!(
        UpgradeEventType::ImmutableSet,
        UpgradeEventType::ImmutableSet
    ));
}

#[test]
fn test_upgrade_risk_levels() {
    let low = UpgradeRisk::low(vec![], vec!["No concerns".to_string()]);
    let medium = UpgradeRisk::medium(vec![], vec!["Monitor situation".to_string()]);
    let high = UpgradeRisk::high(vec![], vec!["Review immediately".to_string()]);
    let critical = UpgradeRisk::critical(vec![], vec!["Take action now".to_string()]);

    assert!(matches!(low.level, RiskLevel::Low));
    assert!(matches!(medium.level, RiskLevel::Medium));
    assert!(matches!(high.level, RiskLevel::High));
    assert!(matches!(critical.level, RiskLevel::Critical));

    // Scores should increase with risk level
    assert!(low.score < medium.score);
    assert!(medium.score < high.score);
    assert!(high.score < critical.score);
}

#[test]
fn test_risk_factor_creation() {
    let factor = RiskFactor::new(
        "Unknown Deployer",
        "The upgrade was performed by an unknown address",
        FactorSeverity::High,
    );

    assert_eq!(factor.name, "Unknown Deployer");
    assert!(matches!(factor.severity, FactorSeverity::High));
    assert!(factor.weight > 0.0);
}

#[test]
fn test_risk_factor_severity_weights() {
    let low = RiskFactor::new("Low", "desc", FactorSeverity::Low);
    let medium = RiskFactor::new("Medium", "desc", FactorSeverity::Medium);
    let high = RiskFactor::new("High", "desc", FactorSeverity::High);
    let critical = RiskFactor::new("Critical", "desc", FactorSeverity::Critical);

    assert!(low.weight < medium.weight);
    assert!(medium.weight < high.weight);
    assert!(high.weight < critical.weight);
}

#[test]
fn test_upgrade_detector_config_default() {
    let config = UpgradeDetectorConfig::default();

    assert!(config.max_upgrades_in_window > 0);
    assert!(config.recent_window_hours > 0);
}

#[test]
fn test_upgrade_detector_creation() {
    let config = UpgradeDetectorConfig::default();
    let mut detector = UpgradeDetector::new(config);

    // Add a trusted authority
    let authority = Pubkey::new_unique();
    detector.add_trusted_authority(authority, "Solend Team".to_string());

    // Detector should be created successfully
    // (No way to verify trusted authorities without internal access)
}

#[test]
fn test_diff_analyzer_creation() {
    let analyzer = DiffAnalyzer::new();

    // Create a minimal diff with size increase
    let diff = analyzer.create_minimal_diff(
        Pubkey::new_unique(),
        "TestProgram".to_string(),
        "old_hash".to_string(),
        "new_hash".to_string(),
        100_000, // Large size increase
    );

    assert!(diff.has_risk_indicators());
}

#[test]
fn test_diff_analyzer_same_hash() {
    let analyzer = DiffAnalyzer::new();

    let diff = analyzer.create_minimal_diff(
        Pubkey::new_unique(),
        "TestProgram".to_string(),
        "same_hash".to_string(),
        "same_hash".to_string(),
        0, // No size change
    );

    // Same hash and no size change = no changes detected
    assert!(diff.changes.is_empty());
}

#[test]
fn test_diff_analyzer_size_decrease() {
    let analyzer = DiffAnalyzer::new();

    let diff = analyzer.create_minimal_diff(
        Pubkey::new_unique(),
        "TestProgram".to_string(),
        "old_hash".to_string(),
        "new_hash".to_string(),
        -50_000, // Size decrease
    );

    // Size decrease should trigger a risk indicator
    assert!(diff
        .risk_indicators
        .iter()
        .any(|r| r.name.contains("Size Decrease")));
}

#[test]
fn test_upgrade_risk_with_factors() {
    let risk = UpgradeRisk::high(
        vec![
            RiskFactor::new("Unknown Authority", "desc", FactorSeverity::High),
            RiskFactor::new("Large Change", "desc", FactorSeverity::Medium),
        ],
        vec![
            "Verify the upgrade with the team".to_string(),
            "Consider reducing exposure".to_string(),
        ],
    );

    assert_eq!(risk.recommendations.len(), 2);
    assert_eq!(risk.factors.len(), 2);
}

#[test]
fn test_known_program_addresses() {
    // Verify known program addresses parse correctly
    let programs = vec![
        ("Solend", "So1endDq2YkqhipRh3WViPa8hdiSpxWy6z3Z6tMCpAo"),
        ("Marginfi", "MFv2hWf31Z9kbCa1snEPYctwafyhdvnV7FZnsebVacA"),
        ("Marinade", "MarBmsSgKXdrN1egZf5sqe1TMai9K1rChYNDJgjq7aD"),
        ("Jupiter", "JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4"),
    ];

    for (name, address) in programs {
        let pubkey = Pubkey::from_str(address);
        assert!(pubkey.is_ok(), "Invalid pubkey for {}", name);
    }
}

#[test]
fn test_risk_level_ordering() {
    // RiskLevel should have a logical ordering
    let levels = vec![
        RiskLevel::Low,
        RiskLevel::Medium,
        RiskLevel::High,
        RiskLevel::Critical,
    ];

    // Verify they are all distinct
    for i in 0..levels.len() {
        for j in 0..levels.len() {
            if i == j {
                assert_eq!(levels[i], levels[j]);
            } else {
                assert_ne!(levels[i], levels[j]);
            }
        }
    }
}

#[test]
fn test_factor_severity_ordering() {
    let severities = vec![
        FactorSeverity::Low,
        FactorSeverity::Medium,
        FactorSeverity::High,
        FactorSeverity::Critical,
    ];

    // Verify they are all distinct
    for i in 0..severities.len() {
        for j in 0..severities.len() {
            if i == j {
                assert_eq!(severities[i], severities[j]);
            } else {
                assert_ne!(severities[i], severities[j]);
            }
        }
    }
}

#[test]
fn test_upgrade_risk_serialization() {
    let risk = UpgradeRisk::low(vec![], vec![]);
    let json = serde_json::to_string(&risk);
    assert!(json.is_ok());
}

#[test]
fn test_risk_factor_serialization() {
    let factor = RiskFactor::new("Test", "Description", FactorSeverity::Medium);
    let json = serde_json::to_string(&factor);
    assert!(json.is_ok());
}
