//! Integration tests for Correlated Risk Detection.
//!
//! Tests portfolio risk analysis, correlation detection, and alert generation.

use drbot_solana::defi::{Position, PositionType};
use drbot_solana::risk::{
    ConcentrationMetrics, CorrelatedPair, CorrelationMatrix, PortfolioAnalyzer, PortfolioRisk,
    PriceHistory, RiskAlert, RiskConfig,
};
use solana_sdk::pubkey::Pubkey;
use std::collections::HashMap;

/// Create a mock position for testing.
fn mock_position(protocol: &str, asset: &str, value: f64) -> Position {
    Position {
        protocol: protocol.to_string(),
        id: format!("{}-{}", protocol.to_lowercase(), asset.to_lowercase()),
        position_type: PositionType::Supply,
        asset_symbol: asset.to_string(),
        asset_mint: Pubkey::new_unique(),
        amount: (value * 1e9 / 100.0) as u64,
        usd_value: value,
        current_apy: 0.05,
        unclaimed_rewards: vec![],
    }
}

#[test]
fn test_concentration_metrics_equal_weights() {
    let weights = vec![1000.0, 1000.0, 1000.0, 1000.0];
    let metrics = ConcentrationMetrics::from_weights(&weights);

    // HHI for 4 equal weights = 4 * (0.25)^2 = 0.25
    assert!((metrics.herfindahl_index - 0.25).abs() < 0.01);
    assert_eq!(metrics.position_count, 4);
    assert!((metrics.largest_position_pct - 25.0).abs() < 0.1);
    assert!((metrics.top_3_pct - 75.0).abs() < 0.1);
    assert!((metrics.effective_positions - 4.0).abs() < 0.1);
}

#[test]
fn test_concentration_metrics_highly_concentrated() {
    let weights = vec![9000.0, 500.0, 500.0];
    let metrics = ConcentrationMetrics::from_weights(&weights);

    // Highly concentrated
    assert!(metrics.herfindahl_index > 0.8);
    assert!(metrics.is_concentrated());
    assert!(metrics.largest_position_pct > 85.0);
}

#[test]
fn test_concentration_metrics_single_position() {
    let weights = vec![10000.0];
    let metrics = ConcentrationMetrics::from_weights(&weights);

    assert!((metrics.herfindahl_index - 1.0).abs() < 0.01);
    assert_eq!(metrics.largest_position_pct, 100.0);
    assert!(metrics.is_concentrated());
}

#[test]
fn test_concentration_metrics_empty() {
    let weights: Vec<f64> = vec![];
    let metrics = ConcentrationMetrics::from_weights(&weights);

    assert_eq!(metrics.herfindahl_index, 0.0);
    assert_eq!(metrics.position_count, 0);
}

#[test]
fn test_correlation_matrix_construction() {
    let assets = vec!["SOL".to_string(), "mSOL".to_string(), "USDC".to_string()];
    let matrix = CorrelationMatrix::default_for_assets(&assets);

    assert_eq!(matrix.assets.len(), 3);
    // Check diagonal is 1
    assert_eq!(matrix.matrix[0][0], 1.0);
    assert_eq!(matrix.matrix[1][1], 1.0);
    assert_eq!(matrix.matrix[2][2], 1.0);
}

#[test]
fn test_correlation_matrix_sol_derivatives() {
    let assets = vec!["SOL".to_string(), "mSOL".to_string(), "JitoSOL".to_string()];
    let matrix = CorrelationMatrix::default_for_assets(&assets);

    // SOL derivatives should be highly correlated
    let high_pairs: Vec<_> = matrix
        .high_correlation_pairs
        .iter()
        .filter(|p| p.correlation >= 0.8)
        .collect();

    assert!(!high_pairs.is_empty());
}

#[test]
fn test_portfolio_risk_analyzer_basic() {
    let config = RiskConfig::default();
    let analyzer = PortfolioAnalyzer::new(config);

    let positions = vec![
        mock_position("Solend", "SOL", 5000.0),
        mock_position("Marginfi", "USDC", 3000.0),
        mock_position("Kamino", "ETH", 2000.0),
    ];

    let risk = analyzer.analyze(&positions, None);

    assert_eq!(risk.total_value_usd, 10000.0);
    assert!(risk.var_95 > 0.0);
    assert!(risk.var_99 > risk.var_95);
    assert_eq!(risk.concentration.position_count, 3);
}

#[test]
fn test_portfolio_risk_protocol_exposure() {
    let config = RiskConfig::default();
    let analyzer = PortfolioAnalyzer::new(config);

    let positions = vec![
        mock_position("Solend", "SOL", 5000.0),
        mock_position("Solend", "USDC", 3000.0),
        mock_position("Marinade", "mSOL", 2000.0),
    ];

    let risk = analyzer.analyze(&positions, None);

    assert_eq!(risk.protocol_exposure.get("Solend"), Some(&8000.0));
    assert_eq!(risk.protocol_exposure.get("Marinade"), Some(&2000.0));
}

#[test]
fn test_portfolio_risk_asset_exposure() {
    let config = RiskConfig::default();
    let analyzer = PortfolioAnalyzer::new(config);

    let positions = vec![
        mock_position("Solend", "SOL", 5000.0),
        mock_position("Marginfi", "SOL", 2000.0),
        mock_position("Kamino", "USDC", 3000.0),
    ];

    let risk = analyzer.analyze(&positions, None);

    assert_eq!(risk.asset_exposure.get("SOL"), Some(&7000.0));
    assert_eq!(risk.asset_exposure.get("USDC"), Some(&3000.0));
}

#[test]
fn test_risk_alert_concentration() {
    let config = RiskConfig {
        concentration_limit_single: 25.0,
        ..Default::default()
    };
    let analyzer = PortfolioAnalyzer::new(config);

    // One position is 60% of portfolio - should trigger alert
    let positions = vec![
        mock_position("Solend", "SOL", 6000.0),
        mock_position("Marginfi", "USDC", 2000.0),
        mock_position("Kamino", "ETH", 2000.0),
    ];

    let risk = analyzer.analyze(&positions, None);

    let concentration_alerts: Vec<_> = risk
        .alerts
        .iter()
        .filter(|a| matches!(a, RiskAlert::ConcentrationRisk { .. }))
        .collect();

    assert!(!concentration_alerts.is_empty());
}

#[test]
fn test_risk_alert_protocol_exposure() {
    let config = RiskConfig {
        concentration_limit_protocol: 40.0,
        ..Default::default()
    };
    let analyzer = PortfolioAnalyzer::new(config);

    // Solend has 80% of portfolio - should trigger alert
    let positions = vec![
        mock_position("Solend", "SOL", 5000.0),
        mock_position("Solend", "USDC", 3000.0),
        mock_position("Marinade", "mSOL", 2000.0),
    ];

    let risk = analyzer.analyze(&positions, None);

    let protocol_alerts: Vec<_> = risk
        .alerts
        .iter()
        .filter(|a| matches!(a, RiskAlert::ProtocolExposure { .. }))
        .collect();

    assert!(!protocol_alerts.is_empty());
}

#[test]
fn test_portfolio_risk_score() {
    let config = RiskConfig::default();
    let analyzer = PortfolioAnalyzer::new(config);

    // Low risk portfolio - well diversified
    let low_risk_positions = vec![
        mock_position("Solend", "SOL", 2500.0),
        mock_position("Marginfi", "USDC", 2500.0),
        mock_position("Kamino", "ETH", 2500.0),
        mock_position("Marinade", "BTC", 2500.0),
    ];
    let low_risk = analyzer.analyze(&low_risk_positions, None);

    // High risk portfolio - concentrated
    let high_risk_positions = vec![
        mock_position("Solend", "SOL", 8000.0),
        mock_position("Solend", "mSOL", 2000.0),
    ];
    let high_risk = analyzer.analyze(&high_risk_positions, None);

    assert!(low_risk.risk_score() <= high_risk.risk_score());
}

#[test]
fn test_is_high_risk() {
    let config = RiskConfig::default();
    let analyzer = PortfolioAnalyzer::new(config);

    // Concentrated portfolio should be high risk
    let concentrated = vec![mock_position("A", "SOL", 10000.0)];
    let concentrated_risk = analyzer.analyze(&concentrated, None);

    assert!(concentrated_risk.is_high_risk());
}

#[test]
fn test_price_history_volatility() {
    let mut history = PriceHistory::new();

    // Add price data for SOL
    history.prices.insert(
        "SOL".to_string(),
        vec![100.0, 102.0, 98.0, 105.0, 103.0, 107.0, 104.0],
    );

    let volatility = history.volatility("SOL");
    assert!(volatility.is_some());
    assert!(volatility.unwrap() > 0.0);
}

#[test]
fn test_price_history_daily_returns() {
    let mut history = PriceHistory::new();

    history
        .prices
        .insert("SOL".to_string(), vec![100.0, 110.0, 105.0]);

    let returns = history.daily_returns("SOL").unwrap();

    // First return: (110 - 100) / 100 = 0.10
    assert!((returns[0] - 0.10).abs() < 0.001);
    // Second return: (105 - 110) / 110 ≈ -0.0454
    assert!((returns[1] - (-0.0454)).abs() < 0.001);
}

#[test]
fn test_risk_config_defaults() {
    let config = RiskConfig::default();

    assert_eq!(config.correlation_threshold, 0.7);
    assert_eq!(config.concentration_limit_single, 25.0);
    assert_eq!(config.concentration_limit_protocol, 40.0);
    assert_eq!(config.var_confidence, 0.95);
    assert_eq!(config.volatility_lookback_days, 30);
}

#[test]
fn test_correlated_pair_structure() {
    let pair = CorrelatedPair {
        asset_a: "SOL".to_string(),
        asset_b: "mSOL".to_string(),
        correlation: 0.95,
    };

    assert_eq!(pair.asset_a, "SOL");
    assert_eq!(pair.asset_b, "mSOL");
    assert!(pair.correlation > 0.9);
}
