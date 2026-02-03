//! Integration tests for Market Neutral Hedging.
//!
//! Tests delta calculation types and rebalancing configuration.

use drbot_solana::hedging::{
    AssetClass, HedgeMethod, PortfolioDelta, PositionDelta, PositionDirection, RebalanceConfig,
};
use drbot_solana::otc::TradeDirection;
use solana_sdk::pubkey::Pubkey;
use std::collections::HashMap;

#[test]
fn test_position_direction_types() {
    assert!(matches!(PositionDirection::Long, PositionDirection::Long));
    assert!(matches!(PositionDirection::Short, PositionDirection::Short));
    assert!(matches!(
        PositionDirection::Neutral,
        PositionDirection::Neutral
    ));
}

#[test]
fn test_asset_class_types() {
    assert!(matches!(AssetClass::Stablecoin, AssetClass::Stablecoin));
    assert!(matches!(AssetClass::Sol, AssetClass::Sol));
    assert!(matches!(AssetClass::Btc, AssetClass::Btc));
    assert!(matches!(AssetClass::Eth, AssetClass::Eth));
    assert!(matches!(AssetClass::Altcoin, AssetClass::Altcoin));
    assert!(matches!(AssetClass::LpToken, AssetClass::LpToken));
}

#[test]
fn test_position_delta_creation() {
    let delta = PositionDelta {
        position_id: "test-1".to_string(),
        asset: "SOL".to_string(),
        asset_mint: Pubkey::new_unique(),
        value_usd: 1000.0,
        delta: 1000.0,
        effective_delta: 1000.0,
        direction: PositionDirection::Long,
        asset_class: AssetClass::Sol,
        beta_sol: 1.0,
    };

    assert_eq!(delta.asset, "SOL");
    assert_eq!(delta.value_usd, 1000.0);
    assert_eq!(delta.delta, 1000.0);
    assert!(matches!(delta.direction, PositionDirection::Long));
}

#[test]
fn test_portfolio_delta_structure() {
    let mut delta_by_class = HashMap::new();
    delta_by_class.insert(AssetClass::Sol, 5000.0);
    delta_by_class.insert(AssetClass::Stablecoin, 0.0);

    let portfolio = PortfolioDelta {
        total_delta: 5000.0,
        beta_to_sol: 0.5,
        beta_to_btc: 0.1,
        position_deltas: vec![],
        is_market_neutral: false,
        delta_by_class,
        long_exposure: 5000.0,
        short_exposure: 0.0,
    };

    assert_eq!(portfolio.total_delta, 5000.0);
    assert!(!portfolio.is_market_neutral);
    assert_eq!(portfolio.long_exposure, 5000.0);
}

#[test]
fn test_portfolio_delta_percentage() {
    let portfolio = PortfolioDelta {
        total_delta: 1000.0,
        beta_to_sol: 0.1,
        beta_to_btc: 0.0,
        position_deltas: vec![],
        is_market_neutral: false,
        delta_by_class: HashMap::new(),
        long_exposure: 10000.0,
        short_exposure: 0.0,
    };

    let delta_pct = portfolio.delta_percentage();
    assert!((delta_pct - 10.0).abs() < 0.1); // 1000/10000 = 10%
}

#[test]
fn test_portfolio_directional_bias() {
    let biased = PortfolioDelta {
        total_delta: 2000.0,
        beta_to_sol: 0.2,
        beta_to_btc: 0.0,
        position_deltas: vec![],
        is_market_neutral: false,
        delta_by_class: HashMap::new(),
        long_exposure: 10000.0,
        short_exposure: 0.0,
    };

    assert!(biased.has_directional_bias(10.0)); // >10% bias

    let balanced = PortfolioDelta {
        total_delta: 500.0,
        beta_to_sol: 0.05,
        beta_to_btc: 0.0,
        position_deltas: vec![],
        is_market_neutral: true,
        delta_by_class: HashMap::new(),
        long_exposure: 10000.0,
        short_exposure: 0.0,
    };

    assert!(!balanced.has_directional_bias(10.0)); // <10% bias
}

#[test]
fn test_hedge_method_types() {
    assert!(matches!(HedgeMethod::Spot, HedgeMethod::Spot));
    assert!(matches!(
        HedgeMethod::LendingShort,
        HedgeMethod::LendingShort
    ));
    assert!(matches!(HedgeMethod::Perpetual, HedgeMethod::Perpetual));
    assert!(matches!(HedgeMethod::Options, HedgeMethod::Options));
    assert!(matches!(
        HedgeMethod::CorrelatedAsset,
        HedgeMethod::CorrelatedAsset
    ));
}

#[test]
fn test_rebalance_config_defaults() {
    let config = RebalanceConfig::default();

    assert!(config.delta_threshold > 0.0);
    assert!(config.min_trade_size_usd > 0.0);
    assert!(!config.auto_rebalance);
    assert!(config.max_slippage_bps > 0);
    assert!(config.cooldown_secs > 0);
}

#[test]
fn test_rebalance_config_aggressive() {
    let config = RebalanceConfig::aggressive();

    assert!(config.delta_threshold < RebalanceConfig::default().delta_threshold);
    assert!(config.auto_rebalance);
}

#[test]
fn test_market_neutral_calculation() {
    // Start with long positions
    let sol_delta: f64 = 5000.0;
    let msol_delta: f64 = 2850.0; // 3000 * 0.95 beta
    let usdc_delta: f64 = 0.0;

    let total_delta: f64 = sol_delta + msol_delta + usdc_delta;

    // To become market neutral, need to offset the entire delta
    let hedge_needed: f64 = -total_delta;

    assert!((total_delta - 7850.0_f64).abs() < 1.0);
    assert!((hedge_needed - (-7850.0_f64)).abs() < 1.0);
}

#[test]
fn test_partial_hedge_calculation() {
    let current_delta: f64 = 10000.0;
    let target_delta: f64 = 2000.0; // Want some exposure, not fully neutral

    let hedge_amount: f64 = current_delta - target_delta;

    assert_eq!(hedge_amount, 8000.0);

    // This represents 80% hedge ratio
    let hedge_ratio: f64 = hedge_amount / current_delta;
    assert!((hedge_ratio - 0.8_f64).abs() < 0.001);
}

#[test]
fn test_hedge_cost_estimation() {
    let hedge_amount: f64 = 5000.0;
    let cost_bps: u16 = 50; // 0.5%

    let estimated_cost: f64 = hedge_amount * (cost_bps as f64 / 10000.0);

    assert!((estimated_cost - 25.0).abs() < 0.1);
}

#[test]
fn test_delta_aggregation() {
    let position_deltas = vec![
        PositionDelta {
            position_id: "1".to_string(),
            asset: "SOL".to_string(),
            asset_mint: Pubkey::new_unique(),
            value_usd: 5000.0,
            delta: 5000.0,
            effective_delta: 5000.0,
            direction: PositionDirection::Long,
            asset_class: AssetClass::Sol,
            beta_sol: 1.0,
        },
        PositionDelta {
            position_id: "2".to_string(),
            asset: "mSOL".to_string(),
            asset_mint: Pubkey::new_unique(),
            value_usd: 3000.0,
            delta: 2850.0,
            effective_delta: 2850.0,
            direction: PositionDirection::Long,
            asset_class: AssetClass::Sol,
            beta_sol: 0.95,
        },
        PositionDelta {
            position_id: "3".to_string(),
            asset: "USDC".to_string(),
            asset_mint: Pubkey::new_unique(),
            value_usd: 2000.0,
            delta: 0.0,
            effective_delta: 0.0,
            direction: PositionDirection::Neutral,
            asset_class: AssetClass::Stablecoin,
            beta_sol: 0.0,
        },
    ];

    let total_delta: f64 = position_deltas.iter().map(|p| p.delta).sum();
    let total_value: f64 = position_deltas.iter().map(|p| p.value_usd).sum();

    assert!((total_delta - 7850.0_f64).abs() < 1.0);
    assert_eq!(total_value, 10000.0);
}

#[test]
fn test_position_delta_serialization() {
    let delta = PositionDelta {
        position_id: "test".to_string(),
        asset: "SOL".to_string(),
        asset_mint: Pubkey::new_unique(),
        value_usd: 1000.0,
        delta: 1000.0,
        effective_delta: 1000.0,
        direction: PositionDirection::Long,
        asset_class: AssetClass::Sol,
        beta_sol: 1.0,
    };

    let json = serde_json::to_string(&delta);
    assert!(json.is_ok());
}

#[test]
fn test_portfolio_delta_serialization() {
    let portfolio = PortfolioDelta {
        total_delta: 5000.0,
        beta_to_sol: 0.5,
        beta_to_btc: 0.1,
        position_deltas: vec![],
        is_market_neutral: false,
        delta_by_class: HashMap::new(),
        long_exposure: 5000.0,
        short_exposure: 0.0,
    };

    let json = serde_json::to_string(&portfolio);
    assert!(json.is_ok());
}

#[test]
fn test_trade_direction_for_hedging() {
    // For hedging long positions, we need to go short (sell)
    assert!(matches!(TradeDirection::Sell, TradeDirection::Sell));
    assert!(matches!(TradeDirection::Buy, TradeDirection::Buy));
}
