//! Portfolio-level risk metrics.
//!
//! Calculates portfolio risk metrics including Value at Risk (VaR),
//! maximum drawdown, and concentration metrics.

use super::{CorrelationMatrix, RiskAlert};
use crate::defi::Position;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Portfolio risk analysis result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortfolioRisk {
    /// Total portfolio value in USD.
    pub total_value_usd: f64,
    /// 95% Value at Risk (daily).
    pub var_95: f64,
    /// 99% Value at Risk (daily).
    pub var_99: f64,
    /// Maximum historical drawdown.
    pub max_drawdown: f64,
    /// Concentration metrics.
    pub concentration: ConcentrationMetrics,
    /// Asset correlation matrix.
    pub correlations: CorrelationMatrix,
    /// Exposure by protocol.
    pub protocol_exposure: HashMap<String, f64>,
    /// Exposure by asset.
    pub asset_exposure: HashMap<String, f64>,
    /// Generated risk alerts.
    pub alerts: Vec<RiskAlert>,
}

impl PortfolioRisk {
    /// Check if the portfolio is considered high risk.
    pub fn is_high_risk(&self) -> bool {
        !self.alerts.is_empty()
            || self.concentration.herfindahl_index > 0.25
            || self.var_95 > self.total_value_usd * 0.1
    }

    /// Get the overall risk score (1-10).
    pub fn risk_score(&self) -> u8 {
        let mut score = 3u8;

        // VaR contribution
        let var_pct = self.var_95 / self.total_value_usd.max(1.0);
        if var_pct > 0.15 {
            score += 3;
        } else if var_pct > 0.10 {
            score += 2;
        } else if var_pct > 0.05 {
            score += 1;
        }

        // Concentration contribution
        if self.concentration.herfindahl_index > 0.4 {
            score += 3;
        } else if self.concentration.herfindahl_index > 0.25 {
            score += 2;
        } else if self.concentration.herfindahl_index > 0.15 {
            score += 1;
        }

        // Alert contribution
        score += (self.alerts.len() as u8).min(2);

        score.min(10)
    }
}

/// Concentration metrics for portfolio analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConcentrationMetrics {
    /// Herfindahl-Hirschman Index (sum of squared weights).
    /// 0 = perfectly diversified, 1 = single position.
    pub herfindahl_index: f64,
    /// Largest single position as percentage.
    pub largest_position_pct: f64,
    /// Top 3 positions as percentage.
    pub top_3_pct: f64,
    /// Number of positions.
    pub position_count: usize,
    /// Effective number of positions (1/HHI).
    pub effective_positions: f64,
}

impl ConcentrationMetrics {
    /// Calculate concentration metrics from position weights.
    pub fn from_weights(weights: &[f64]) -> Self {
        if weights.is_empty() {
            return Self {
                herfindahl_index: 0.0,
                largest_position_pct: 0.0,
                top_3_pct: 0.0,
                position_count: 0,
                effective_positions: 0.0,
            };
        }

        let total: f64 = weights.iter().sum();
        if total <= 0.0 {
            return Self {
                herfindahl_index: 0.0,
                largest_position_pct: 0.0,
                top_3_pct: 0.0,
                position_count: weights.len(),
                effective_positions: 0.0,
            };
        }

        // Normalize weights
        let normalized: Vec<f64> = weights.iter().map(|w| w / total).collect();

        // HHI
        let hhi: f64 = normalized.iter().map(|w| w * w).sum();

        // Sort for top positions
        let mut sorted = normalized.clone();
        sorted.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));

        let largest = sorted.first().copied().unwrap_or(0.0);
        let top_3: f64 = sorted.iter().take(3).sum();

        Self {
            herfindahl_index: hhi,
            largest_position_pct: largest * 100.0,
            top_3_pct: top_3 * 100.0,
            position_count: weights.len(),
            effective_positions: if hhi > 0.0 { 1.0 / hhi } else { 0.0 },
        }
    }

    /// Check if concentrated.
    pub fn is_concentrated(&self) -> bool {
        self.herfindahl_index > 0.25 || self.largest_position_pct > 40.0
    }
}

/// Portfolio risk analyzer.
pub struct PortfolioAnalyzer {
    config: RiskConfig,
}

/// Risk analysis configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskConfig {
    /// Correlation threshold for alerts.
    pub correlation_threshold: f64,
    /// Single position concentration limit (percentage).
    pub concentration_limit_single: f64,
    /// Protocol concentration limit (percentage).
    pub concentration_limit_protocol: f64,
    /// VaR confidence level (e.g., 0.95).
    pub var_confidence: f64,
    /// Lookback period for volatility calculation (days).
    pub volatility_lookback_days: u32,
}

impl Default for RiskConfig {
    fn default() -> Self {
        Self {
            correlation_threshold: 0.7,
            concentration_limit_single: 25.0,
            concentration_limit_protocol: 40.0,
            var_confidence: 0.95,
            volatility_lookback_days: 30,
        }
    }
}

impl PortfolioAnalyzer {
    /// Create a new portfolio analyzer.
    pub fn new(config: RiskConfig) -> Self {
        Self { config }
    }

    /// Analyze portfolio risk from positions.
    pub fn analyze(
        &self,
        positions: &[Position],
        price_history: Option<&PriceHistory>,
    ) -> PortfolioRisk {
        let total_value: f64 = positions.iter().map(|p| p.usd_value).sum();
        let weights: Vec<f64> = positions.iter().map(|p| p.usd_value).collect();

        // Concentration
        let concentration = ConcentrationMetrics::from_weights(&weights);

        // Protocol exposure
        let mut protocol_exposure: HashMap<String, f64> = HashMap::new();
        for pos in positions {
            *protocol_exposure.entry(pos.protocol.clone()).or_default() += pos.usd_value;
        }

        // Asset exposure
        let mut asset_exposure: HashMap<String, f64> = HashMap::new();
        for pos in positions {
            *asset_exposure.entry(pos.asset_symbol.clone()).or_default() += pos.usd_value;
        }

        // Calculate correlations
        let asset_names: Vec<String> = positions.iter().map(|p| p.asset_symbol.clone()).collect();
        let correlations = if let Some(history) = price_history {
            CorrelationMatrix::from_price_history(&asset_names, history)
        } else {
            CorrelationMatrix::default_for_assets(&asset_names)
        };

        // Calculate VaR (simplified parametric VaR)
        let (var_95, var_99) = self.calculate_var(total_value, &weights, &correlations);

        // Generate alerts
        let alerts = self.generate_alerts(
            &concentration,
            &protocol_exposure,
            &asset_exposure,
            &correlations,
            total_value,
        );

        PortfolioRisk {
            total_value_usd: total_value,
            var_95,
            var_99,
            max_drawdown: 0.0, // Would need historical data
            concentration,
            correlations,
            protocol_exposure,
            asset_exposure,
            alerts,
        }
    }

    /// Calculate Value at Risk using parametric method.
    fn calculate_var(
        &self,
        total_value: f64,
        weights: &[f64],
        correlations: &CorrelationMatrix,
    ) -> (f64, f64) {
        // Simplified VaR calculation
        // In production, would use historical volatility and full covariance matrix

        // Assume average daily volatility of 3% for crypto
        let daily_vol = 0.03;

        // Z-scores for confidence levels
        let z_95 = 1.645;
        let z_99 = 2.326;

        // Portfolio volatility (simplified - assumes some diversification benefit)
        let diversification_factor = 1.0 / weights.len().max(1) as f64;
        let portfolio_vol = daily_vol * (1.0 - diversification_factor * 0.3).sqrt();

        let var_95 = total_value * portfolio_vol * z_95;
        let var_99 = total_value * portfolio_vol * z_99;

        (var_95, var_99)
    }

    /// Generate risk alerts based on analysis.
    fn generate_alerts(
        &self,
        concentration: &ConcentrationMetrics,
        protocol_exposure: &HashMap<String, f64>,
        asset_exposure: &HashMap<String, f64>,
        correlations: &CorrelationMatrix,
        total_value: f64,
    ) -> Vec<RiskAlert> {
        let mut alerts = Vec::new();

        // Single position concentration
        if concentration.largest_position_pct > self.config.concentration_limit_single {
            alerts.push(RiskAlert::ConcentrationRisk {
                asset: "Largest Position".to_string(),
                percentage: concentration.largest_position_pct,
                limit: self.config.concentration_limit_single,
            });
        }

        // Protocol concentration
        for (protocol, &value) in protocol_exposure {
            let pct = (value / total_value) * 100.0;
            if pct > self.config.concentration_limit_protocol {
                alerts.push(RiskAlert::ProtocolExposure {
                    protocol: protocol.clone(),
                    percentage: pct,
                    limit: self.config.concentration_limit_protocol,
                });
            }
        }

        // High correlation pairs
        for pair in &correlations.high_correlation_pairs {
            if pair.correlation >= self.config.correlation_threshold {
                // Calculate combined exposure
                let exp_a = asset_exposure.get(&pair.asset_a).copied().unwrap_or(0.0);
                let exp_b = asset_exposure.get(&pair.asset_b).copied().unwrap_or(0.0);
                let combined_pct = ((exp_a + exp_b) / total_value) * 100.0;

                alerts.push(RiskAlert::HighCorrelation {
                    asset_a: pair.asset_a.clone(),
                    asset_b: pair.asset_b.clone(),
                    correlation: pair.correlation,
                    combined_percentage: combined_pct,
                });
            }
        }

        alerts
    }

    /// Get configuration.
    pub fn config(&self) -> &RiskConfig {
        &self.config
    }
}

/// Historical price data for volatility calculation.
#[derive(Debug, Clone)]
pub struct PriceHistory {
    /// Asset prices by date.
    pub prices: HashMap<String, Vec<f64>>,
    /// Dates corresponding to prices.
    pub dates: Vec<chrono::NaiveDate>,
}

impl PriceHistory {
    /// Create empty price history.
    pub fn new() -> Self {
        Self {
            prices: HashMap::new(),
            dates: Vec::new(),
        }
    }

    /// Calculate daily returns for an asset.
    pub fn daily_returns(&self, asset: &str) -> Option<Vec<f64>> {
        let prices = self.prices.get(asset)?;
        if prices.len() < 2 {
            return None;
        }

        let returns: Vec<f64> = prices.windows(2).map(|w| (w[1] - w[0]) / w[0]).collect();

        Some(returns)
    }

    /// Calculate volatility (standard deviation of returns).
    pub fn volatility(&self, asset: &str) -> Option<f64> {
        let returns = self.daily_returns(asset)?;
        if returns.is_empty() {
            return None;
        }

        let mean: f64 = returns.iter().sum::<f64>() / returns.len() as f64;
        let variance: f64 =
            returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / returns.len() as f64;

        Some(variance.sqrt())
    }
}

impl Default for PriceHistory {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_concentration_metrics() {
        // Equal weights
        let equal = ConcentrationMetrics::from_weights(&[100.0, 100.0, 100.0, 100.0]);
        assert!((equal.herfindahl_index - 0.25).abs() < 0.01);
        assert_eq!(equal.position_count, 4);

        // Concentrated
        let concentrated = ConcentrationMetrics::from_weights(&[900.0, 50.0, 50.0]);
        assert!(concentrated.herfindahl_index > 0.8);
        assert!(concentrated.is_concentrated());
    }

    #[test]
    fn test_portfolio_risk_score() {
        let risk = PortfolioRisk {
            total_value_usd: 10000.0,
            var_95: 500.0,
            var_99: 700.0,
            max_drawdown: 0.0,
            concentration: ConcentrationMetrics::from_weights(&[5000.0, 3000.0, 2000.0]),
            correlations: CorrelationMatrix::default_for_assets(&[
                "A".to_string(),
                "B".to_string(),
            ]),
            protocol_exposure: HashMap::new(),
            asset_exposure: HashMap::new(),
            alerts: vec![],
        };

        let score = risk.risk_score();
        assert!(score >= 3 && score <= 10);
    }

    #[test]
    fn test_risk_config_defaults() {
        let config = RiskConfig::default();
        assert_eq!(config.correlation_threshold, 0.7);
        assert_eq!(config.concentration_limit_single, 25.0);
    }
}
