//! Program diff analysis.
//!
//! Analyzes differences between program versions to identify
//! potentially risky changes.

use serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;
use std::collections::HashMap;

/// Analysis of changes between program versions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgramDiff {
    /// Program address.
    pub program: Pubkey,
    /// Program name.
    pub name: String,
    /// Old version hash.
    pub old_hash: String,
    /// New version hash.
    pub new_hash: String,
    /// Size change in bytes.
    pub size_delta: i64,
    /// Detected changes.
    pub changes: Vec<DetectedChange>,
    /// Overall risk assessment.
    pub risk_indicators: Vec<RiskIndicator>,
}

impl ProgramDiff {
    /// Check if the diff indicates potentially risky changes.
    pub fn has_risk_indicators(&self) -> bool {
        !self.risk_indicators.is_empty()
    }

    /// Get the highest risk indicator.
    pub fn highest_risk(&self) -> Option<&RiskIndicator> {
        self.risk_indicators.iter().max_by_key(|r| r.severity as u8)
    }
}

/// A detected change in the program.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedChange {
    /// Type of change.
    pub change_type: ChangeType,
    /// Description of the change.
    pub description: String,
    /// Confidence in detection (0-100).
    pub confidence: u8,
}

/// Types of changes that can be detected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeType {
    /// New instruction added.
    InstructionAdded,
    /// Instruction removed.
    InstructionRemoved,
    /// Instruction modified.
    InstructionModified,
    /// Account validation changed.
    AccountValidationChanged,
    /// Access control modified.
    AccessControlChanged,
    /// Significant size change.
    SizeChanged,
    /// New dependency added.
    DependencyAdded,
    /// Cryptographic function changed.
    CryptoChanged,
    /// Fund transfer logic modified.
    TransferLogicChanged,
    /// Unknown change.
    Unknown,
}

/// Risk indicator from diff analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskIndicator {
    /// Indicator name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Severity level.
    pub severity: IndicatorSeverity,
    /// Related changes.
    pub related_changes: Vec<ChangeType>,
}

/// Severity of risk indicators.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IndicatorSeverity {
    /// Informational only.
    Info,
    /// Low severity.
    Low,
    /// Medium severity.
    Medium,
    /// High severity.
    High,
    /// Critical severity.
    Critical,
}

/// Analyzer for program diffs.
pub struct DiffAnalyzer {
    /// Patterns that indicate risk.
    risk_patterns: Vec<RiskPattern>,
    /// Known safe patterns.
    safe_patterns: Vec<SafePattern>,
}

impl DiffAnalyzer {
    /// Create a new diff analyzer with default patterns.
    pub fn new() -> Self {
        let risk_patterns = vec![
            RiskPattern {
                name: "Large Size Increase".to_string(),
                description: "Significant increase in program size".to_string(),
                check: Box::new(|diff| diff.size_delta > 50_000),
                severity: IndicatorSeverity::Medium,
            },
            RiskPattern {
                name: "Size Decrease".to_string(),
                description: "Program size decreased (possible removal of safety checks)"
                    .to_string(),
                check: Box::new(|diff| diff.size_delta < -10_000),
                severity: IndicatorSeverity::High,
            },
            RiskPattern {
                name: "Access Control Change".to_string(),
                description: "Changes to access control detected".to_string(),
                check: Box::new(|diff| {
                    diff.changes
                        .iter()
                        .any(|c| c.change_type == ChangeType::AccessControlChanged)
                }),
                severity: IndicatorSeverity::High,
            },
            RiskPattern {
                name: "Transfer Logic Change".to_string(),
                description: "Fund transfer logic was modified".to_string(),
                check: Box::new(|diff| {
                    diff.changes
                        .iter()
                        .any(|c| c.change_type == ChangeType::TransferLogicChanged)
                }),
                severity: IndicatorSeverity::Critical,
            },
            RiskPattern {
                name: "Crypto Change".to_string(),
                description: "Cryptographic operations were modified".to_string(),
                check: Box::new(|diff| {
                    diff.changes
                        .iter()
                        .any(|c| c.change_type == ChangeType::CryptoChanged)
                }),
                severity: IndicatorSeverity::High,
            },
        ];

        Self {
            risk_patterns,
            safe_patterns: Vec::new(),
        }
    }

    /// Analyze a program diff.
    pub fn analyze(&self, diff: &mut ProgramDiff) {
        // Apply risk patterns
        for pattern in &self.risk_patterns {
            if (pattern.check)(diff) {
                diff.risk_indicators.push(RiskIndicator {
                    name: pattern.name.clone(),
                    description: pattern.description.clone(),
                    severity: pattern.severity,
                    related_changes: diff.changes.iter().map(|c| c.change_type).collect(),
                });
            }
        }
    }

    /// Compare two program data blobs and create a diff.
    pub fn compare_data(
        &self,
        program: Pubkey,
        name: String,
        old_data: &[u8],
        new_data: &[u8],
        old_hash: String,
        new_hash: String,
    ) -> ProgramDiff {
        let size_delta = new_data.len() as i64 - old_data.len() as i64;

        let mut changes = Vec::new();

        // Detect size change
        if size_delta.abs() > 1000 {
            changes.push(DetectedChange {
                change_type: ChangeType::SizeChanged,
                description: format!("Size changed by {} bytes", size_delta),
                confidence: 100,
            });
        }

        // In a real implementation, would perform more sophisticated analysis:
        // - Disassemble and compare instructions
        // - Identify changed functions
        // - Detect access control patterns
        // - Analyze CPI calls

        // For now, add a generic change detection
        if old_hash != new_hash {
            changes.push(DetectedChange {
                change_type: ChangeType::Unknown,
                description: "Program bytecode changed".to_string(),
                confidence: 100,
            });
        }

        let mut diff = ProgramDiff {
            program,
            name,
            old_hash,
            new_hash,
            size_delta,
            changes,
            risk_indicators: Vec::new(),
        };

        // Run analysis
        self.analyze(&mut diff);

        diff
    }

    /// Create a diff from just hashes and size (when we don't have bytecode).
    pub fn create_minimal_diff(
        &self,
        program: Pubkey,
        name: String,
        old_hash: String,
        new_hash: String,
        size_delta: i64,
    ) -> ProgramDiff {
        let mut changes = Vec::new();

        if old_hash != new_hash {
            changes.push(DetectedChange {
                change_type: ChangeType::Unknown,
                description: "Program was upgraded".to_string(),
                confidence: 100,
            });
        }

        if size_delta.abs() > 1000 {
            changes.push(DetectedChange {
                change_type: ChangeType::SizeChanged,
                description: format!("Size changed by {} bytes", size_delta),
                confidence: 100,
            });
        }

        let mut diff = ProgramDiff {
            program,
            name,
            old_hash,
            new_hash,
            size_delta,
            changes,
            risk_indicators: Vec::new(),
        };

        self.analyze(&mut diff);

        diff
    }
}

impl Default for DiffAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

/// A pattern that indicates risk.
struct RiskPattern {
    name: String,
    description: String,
    check: Box<dyn Fn(&ProgramDiff) -> bool + Send + Sync>,
    severity: IndicatorSeverity,
}

/// A pattern that indicates the change is safe.
struct SafePattern {
    name: String,
    description: String,
    check: Box<dyn Fn(&ProgramDiff) -> bool + Send + Sync>,
}

/// Summary of diff analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffSummary {
    /// Program name.
    pub name: String,
    /// Overall risk level.
    pub risk_level: IndicatorSeverity,
    /// Number of changes detected.
    pub change_count: usize,
    /// Number of risk indicators.
    pub risk_indicator_count: usize,
    /// Brief summary.
    pub summary: String,
}

impl From<&ProgramDiff> for DiffSummary {
    fn from(diff: &ProgramDiff) -> Self {
        let risk_level = diff
            .highest_risk()
            .map(|r| r.severity)
            .unwrap_or(IndicatorSeverity::Info);

        let summary = if diff.risk_indicators.is_empty() {
            "No significant risks detected".to_string()
        } else {
            format!("{} risk indicator(s) detected", diff.risk_indicators.len())
        };

        Self {
            name: diff.name.clone(),
            risk_level,
            change_count: diff.changes.len(),
            risk_indicator_count: diff.risk_indicators.len(),
            summary,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diff_analyzer() {
        let analyzer = DiffAnalyzer::new();

        let diff = analyzer.create_minimal_diff(
            Pubkey::new_unique(),
            "Test".to_string(),
            "abc123".to_string(),
            "def456".to_string(),
            100_000, // Large size increase
        );

        assert!(diff.has_risk_indicators());
        assert!(diff
            .risk_indicators
            .iter()
            .any(|r| r.name == "Large Size Increase"));
    }

    #[test]
    fn test_size_decrease_risk() {
        let analyzer = DiffAnalyzer::new();

        let diff = analyzer.create_minimal_diff(
            Pubkey::new_unique(),
            "Test".to_string(),
            "abc123".to_string(),
            "def456".to_string(),
            -50_000, // Size decrease
        );

        assert!(diff
            .risk_indicators
            .iter()
            .any(|r| r.name == "Size Decrease"));
    }

    #[test]
    fn test_diff_summary() {
        let diff = ProgramDiff {
            program: Pubkey::new_unique(),
            name: "Test".to_string(),
            old_hash: "abc".to_string(),
            new_hash: "def".to_string(),
            size_delta: 1000,
            changes: vec![DetectedChange {
                change_type: ChangeType::Unknown,
                description: "Test".to_string(),
                confidence: 100,
            }],
            risk_indicators: vec![RiskIndicator {
                name: "Test Risk".to_string(),
                description: "Test".to_string(),
                severity: IndicatorSeverity::High,
                related_changes: vec![],
            }],
        };

        let summary: DiffSummary = (&diff).into();
        assert_eq!(summary.risk_level, IndicatorSeverity::High);
        assert_eq!(summary.change_count, 1);
    }
}
