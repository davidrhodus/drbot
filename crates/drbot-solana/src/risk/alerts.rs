//! Risk alert system.
//!
//! Generates and manages risk alerts based on portfolio analysis.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Risk alert types.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RiskAlert {
    /// High correlation between assets.
    HighCorrelation {
        asset_a: String,
        asset_b: String,
        correlation: f64,
        combined_percentage: f64,
    },
    /// Single asset concentration.
    ConcentrationRisk {
        asset: String,
        percentage: f64,
        limit: f64,
    },
    /// Protocol exposure limit exceeded.
    ProtocolExposure {
        protocol: String,
        percentage: f64,
        limit: f64,
    },
    /// Dependency chain risk.
    DependencyChain {
        chain: Vec<String>,
        impact_score: u8,
    },
    /// Market volatility alert.
    HighVolatility {
        asset: String,
        volatility: f64,
        threshold: f64,
    },
    /// Liquidity risk.
    LowLiquidity {
        asset: String,
        available_liquidity_usd: f64,
        position_size_usd: f64,
    },
    /// Price impact warning.
    PriceImpact {
        asset: String,
        estimated_impact_pct: f64,
        trade_size_usd: f64,
    },
    /// Smart contract upgrade detected.
    ContractUpgrade {
        protocol: String,
        program_id: String,
        risk_level: AlertSeverity,
    },
}

impl RiskAlert {
    /// Get the severity of this alert.
    pub fn severity(&self) -> AlertSeverity {
        match self {
            RiskAlert::HighCorrelation {
                correlation,
                combined_percentage,
                ..
            } => {
                if *correlation > 0.9 && *combined_percentage > 50.0 {
                    AlertSeverity::Critical
                } else if *correlation > 0.8 {
                    AlertSeverity::High
                } else {
                    AlertSeverity::Medium
                }
            }
            RiskAlert::ConcentrationRisk {
                percentage, limit, ..
            } => {
                let excess = percentage - limit;
                if excess > 30.0 {
                    AlertSeverity::Critical
                } else if excess > 15.0 {
                    AlertSeverity::High
                } else {
                    AlertSeverity::Medium
                }
            }
            RiskAlert::ProtocolExposure {
                percentage, limit, ..
            } => {
                let excess = percentage - limit;
                if excess > 30.0 {
                    AlertSeverity::Critical
                } else if excess > 15.0 {
                    AlertSeverity::High
                } else {
                    AlertSeverity::Medium
                }
            }
            RiskAlert::DependencyChain { impact_score, .. } => {
                if *impact_score >= 8 {
                    AlertSeverity::Critical
                } else if *impact_score >= 5 {
                    AlertSeverity::High
                } else {
                    AlertSeverity::Medium
                }
            }
            RiskAlert::HighVolatility {
                volatility,
                threshold,
                ..
            } => {
                let excess = volatility / threshold;
                if excess > 3.0 {
                    AlertSeverity::Critical
                } else if excess > 2.0 {
                    AlertSeverity::High
                } else {
                    AlertSeverity::Medium
                }
            }
            RiskAlert::LowLiquidity {
                available_liquidity_usd,
                position_size_usd,
                ..
            } => {
                let ratio = available_liquidity_usd / position_size_usd.max(1.0);
                if ratio < 2.0 {
                    AlertSeverity::Critical
                } else if ratio < 5.0 {
                    AlertSeverity::High
                } else {
                    AlertSeverity::Medium
                }
            }
            RiskAlert::PriceImpact {
                estimated_impact_pct,
                ..
            } => {
                if *estimated_impact_pct > 5.0 {
                    AlertSeverity::Critical
                } else if *estimated_impact_pct > 2.0 {
                    AlertSeverity::High
                } else {
                    AlertSeverity::Medium
                }
            }
            RiskAlert::ContractUpgrade { risk_level, .. } => *risk_level,
        }
    }

    /// Get a human-readable title for this alert.
    pub fn title(&self) -> String {
        match self {
            RiskAlert::HighCorrelation {
                asset_a, asset_b, ..
            } => {
                format!("High Correlation: {} & {}", asset_a, asset_b)
            }
            RiskAlert::ConcentrationRisk { asset, .. } => {
                format!("Concentration Risk: {}", asset)
            }
            RiskAlert::ProtocolExposure { protocol, .. } => {
                format!("Protocol Exposure: {}", protocol)
            }
            RiskAlert::DependencyChain { chain, .. } => {
                format!("Dependency Chain: {} protocols", chain.len())
            }
            RiskAlert::HighVolatility { asset, .. } => {
                format!("High Volatility: {}", asset)
            }
            RiskAlert::LowLiquidity { asset, .. } => {
                format!("Low Liquidity: {}", asset)
            }
            RiskAlert::PriceImpact { asset, .. } => {
                format!("Price Impact: {}", asset)
            }
            RiskAlert::ContractUpgrade { protocol, .. } => {
                format!("Contract Upgrade: {}", protocol)
            }
        }
    }

    /// Get a detailed description of this alert.
    pub fn description(&self) -> String {
        match self {
            RiskAlert::HighCorrelation {
                asset_a,
                asset_b,
                correlation,
                combined_percentage,
            } => {
                format!(
                    "{} and {} have {:.1}% correlation with {:.1}% combined portfolio weight. \
                    Consider diversifying into uncorrelated assets.",
                    asset_a,
                    asset_b,
                    correlation * 100.0,
                    combined_percentage
                )
            }
            RiskAlert::ConcentrationRisk {
                asset,
                percentage,
                limit,
            } => {
                format!(
                    "{} represents {:.1}% of portfolio (limit: {:.1}%). \
                    Consider reducing position or rebalancing.",
                    asset, percentage, limit
                )
            }
            RiskAlert::ProtocolExposure {
                protocol,
                percentage,
                limit,
            } => {
                format!(
                    "{} exposure is {:.1}% (limit: {:.1}%). \
                    Consider diversifying across protocols.",
                    protocol, percentage, limit
                )
            }
            RiskAlert::DependencyChain {
                chain,
                impact_score,
            } => {
                format!(
                    "Dependency chain through {} protocols: {}. \
                    Impact score: {}/10. A failure in this chain could affect multiple positions.",
                    chain.len(),
                    chain.join(" → "),
                    impact_score
                )
            }
            RiskAlert::HighVolatility {
                asset,
                volatility,
                threshold,
            } => {
                format!(
                    "{} volatility ({:.1}%) exceeds threshold ({:.1}%). \
                    Consider reducing position size or hedging.",
                    asset,
                    volatility * 100.0,
                    threshold * 100.0
                )
            }
            RiskAlert::LowLiquidity {
                asset,
                available_liquidity_usd,
                position_size_usd,
            } => {
                format!(
                    "{} has ${:.0} available liquidity for ${:.0} position. \
                    Exiting may cause significant slippage.",
                    asset, available_liquidity_usd, position_size_usd
                )
            }
            RiskAlert::PriceImpact {
                asset,
                estimated_impact_pct,
                trade_size_usd,
            } => {
                format!(
                    "Trading ${:.0} of {} would cause ~{:.2}% price impact. \
                    Consider splitting into smaller trades.",
                    trade_size_usd, asset, estimated_impact_pct
                )
            }
            RiskAlert::ContractUpgrade {
                protocol,
                program_id,
                ..
            } => {
                format!(
                    "{} (program: {}) has been upgraded. \
                    Review changes before interacting.",
                    protocol, program_id
                )
            }
        }
    }

    /// Get recommended actions for this alert.
    pub fn recommendations(&self) -> Vec<String> {
        match self {
            RiskAlert::HighCorrelation { .. } => vec![
                "Reduce position in one of the correlated assets".to_string(),
                "Add uncorrelated assets to portfolio".to_string(),
                "Consider stablecoin allocation for diversification".to_string(),
            ],
            RiskAlert::ConcentrationRisk { .. } => vec![
                "Reduce position size to below limit".to_string(),
                "Distribute across multiple assets".to_string(),
                "Set stop-loss to manage downside".to_string(),
            ],
            RiskAlert::ProtocolExposure { .. } => vec![
                "Move some funds to alternative protocols".to_string(),
                "Research protocol security and audit status".to_string(),
                "Monitor protocol governance proposals".to_string(),
            ],
            RiskAlert::DependencyChain { .. } => vec![
                "Understand the dependency relationships".to_string(),
                "Monitor all protocols in the chain".to_string(),
                "Have contingency plan for cascading failures".to_string(),
            ],
            RiskAlert::HighVolatility { .. } => vec![
                "Reduce position size".to_string(),
                "Set tighter stop-losses".to_string(),
                "Consider hedging with derivatives".to_string(),
            ],
            RiskAlert::LowLiquidity { .. } => vec![
                "Plan exit strategy in advance".to_string(),
                "Use limit orders instead of market orders".to_string(),
                "Consider time-weighted exits".to_string(),
            ],
            RiskAlert::PriceImpact { .. } => vec![
                "Split trade into smaller chunks".to_string(),
                "Use TWAP (Time-Weighted Average Price) strategy".to_string(),
                "Consider using DEX aggregators".to_string(),
            ],
            RiskAlert::ContractUpgrade { .. } => vec![
                "Review upgrade changelog".to_string(),
                "Verify audit status of new version".to_string(),
                "Consider temporary withdrawal until verified".to_string(),
            ],
        }
    }
}

/// Alert severity levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlertSeverity {
    /// Low severity, informational.
    Low,
    /// Medium severity, should review.
    Medium,
    /// High severity, action recommended.
    High,
    /// Critical severity, immediate action needed.
    Critical,
}

impl AlertSeverity {
    /// Get color code for display.
    pub fn color(&self) -> &'static str {
        match self {
            AlertSeverity::Low => "gray",
            AlertSeverity::Medium => "yellow",
            AlertSeverity::High => "orange",
            AlertSeverity::Critical => "red",
        }
    }

    /// Get emoji for display.
    pub fn emoji(&self) -> &'static str {
        match self {
            AlertSeverity::Low => "ℹ️",
            AlertSeverity::Medium => "⚠️",
            AlertSeverity::High => "🔶",
            AlertSeverity::Critical => "🚨",
        }
    }
}

/// A stored alert with metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredAlert {
    /// Unique identifier.
    pub id: Uuid,
    /// The alert content.
    pub alert: RiskAlert,
    /// When the alert was created.
    pub created_at: DateTime<Utc>,
    /// Whether the alert has been acknowledged.
    pub acknowledged: bool,
    /// When the alert was acknowledged.
    pub acknowledged_at: Option<DateTime<Utc>>,
    /// Notes from user.
    pub notes: Option<String>,
}

impl StoredAlert {
    /// Create a new stored alert.
    pub fn new(alert: RiskAlert) -> Self {
        Self {
            id: Uuid::new_v4(),
            alert,
            created_at: Utc::now(),
            acknowledged: false,
            acknowledged_at: None,
            notes: None,
        }
    }

    /// Acknowledge the alert.
    pub fn acknowledge(&mut self, notes: Option<String>) {
        self.acknowledged = true;
        self.acknowledged_at = Some(Utc::now());
        self.notes = notes;
    }
}

/// Alert manager for tracking and managing alerts.
pub struct AlertManager {
    alerts: Vec<StoredAlert>,
    max_alerts: usize,
}

impl AlertManager {
    /// Create a new alert manager.
    pub fn new(max_alerts: usize) -> Self {
        Self {
            alerts: Vec::new(),
            max_alerts,
        }
    }

    /// Add an alert.
    pub fn add(&mut self, alert: RiskAlert) -> Uuid {
        let stored = StoredAlert::new(alert);
        let id = stored.id;
        self.alerts.push(stored);

        // Trim old alerts if exceeding max
        while self.alerts.len() > self.max_alerts {
            // Remove oldest acknowledged alert first
            if let Some(idx) = self.alerts.iter().position(|a| a.acknowledged) {
                self.alerts.remove(idx);
            } else {
                // Remove oldest alert
                self.alerts.remove(0);
            }
        }

        id
    }

    /// Add multiple alerts.
    pub fn add_all(&mut self, alerts: Vec<RiskAlert>) {
        for alert in alerts {
            self.add(alert);
        }
    }

    /// Get all active (unacknowledged) alerts.
    pub fn active(&self) -> Vec<&StoredAlert> {
        self.alerts.iter().filter(|a| !a.acknowledged).collect()
    }

    /// Get alerts by severity.
    pub fn by_severity(&self, severity: AlertSeverity) -> Vec<&StoredAlert> {
        self.alerts
            .iter()
            .filter(|a| !a.acknowledged && a.alert.severity() == severity)
            .collect()
    }

    /// Acknowledge an alert.
    pub fn acknowledge(&mut self, id: Uuid, notes: Option<String>) -> bool {
        if let Some(alert) = self.alerts.iter_mut().find(|a| a.id == id) {
            alert.acknowledge(notes);
            true
        } else {
            false
        }
    }

    /// Get critical alerts count.
    pub fn critical_count(&self) -> usize {
        self.by_severity(AlertSeverity::Critical).len()
    }

    /// Get high severity alerts count.
    pub fn high_count(&self) -> usize {
        self.by_severity(AlertSeverity::High).len()
    }

    /// Clear acknowledged alerts.
    pub fn clear_acknowledged(&mut self) {
        self.alerts.retain(|a| !a.acknowledged);
    }

    /// Get all alerts.
    pub fn all(&self) -> &[StoredAlert] {
        &self.alerts
    }
}

impl Default for AlertManager {
    fn default() -> Self {
        Self::new(100)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_alert_severity() {
        let alert = RiskAlert::HighCorrelation {
            asset_a: "SOL".to_string(),
            asset_b: "mSOL".to_string(),
            correlation: 0.95,
            combined_percentage: 60.0,
        };

        assert_eq!(alert.severity(), AlertSeverity::Critical);
    }

    #[test]
    fn test_alert_manager() {
        let mut manager = AlertManager::new(10);

        let alert = RiskAlert::ConcentrationRisk {
            asset: "SOL".to_string(),
            percentage: 50.0,
            limit: 25.0,
        };

        let id = manager.add(alert);
        assert_eq!(manager.active().len(), 1);

        manager.acknowledge(id, Some("Reviewed".to_string()));
        assert_eq!(manager.active().len(), 0);
    }

    #[test]
    fn test_alert_description() {
        let alert = RiskAlert::ProtocolExposure {
            protocol: "Solend".to_string(),
            percentage: 50.0,
            limit: 40.0,
        };

        let desc = alert.description();
        assert!(desc.contains("Solend"));
        assert!(desc.contains("50.0%"));
    }
}
