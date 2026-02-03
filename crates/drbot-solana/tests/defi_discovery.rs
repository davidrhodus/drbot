//! Integration tests for DeFi Opportunity Discovery.
//!
//! Tests the yield discovery, filtering, and approval flow.

use drbot_solana::defi::{
    ApprovalConfig, DeFiAction, DeFiApprovalManager, Position, PositionType, ProtocolType, SortBy,
    YieldAggregator, YieldFilter, YieldOpportunity,
};
use solana_sdk::pubkey::Pubkey;

/// Create a mock yield opportunity for testing.
fn mock_opportunity(
    protocol: &str,
    asset: &str,
    apy: f64,
    risk_score: u8,
    tvl: f64,
) -> YieldOpportunity {
    YieldOpportunity::new(
        protocol,
        format!("{}-{}", protocol.to_lowercase(), asset.to_lowercase()),
        asset,
        Pubkey::new_unique(),
        apy,
        tvl,
        risk_score,
    )
}

/// Create a mock position for testing.
fn mock_position(protocol: &str, asset: &str, value: f64, apy: f64) -> Position {
    Position {
        protocol: protocol.to_string(),
        id: format!("{}-{}", protocol.to_lowercase(), asset.to_lowercase()),
        position_type: PositionType::Supply,
        asset_symbol: asset.to_string(),
        asset_mint: Pubkey::new_unique(),
        amount: (value * 1e9 / 100.0) as u64, // Assume $100 price
        usd_value: value,
        current_apy: apy,
        unclaimed_rewards: vec![],
    }
}

#[test]
fn test_yield_filter_by_min_apy() {
    let opportunities = vec![
        mock_opportunity("Solend", "SOL", 0.05, 3, 1_000_000.0),
        mock_opportunity("Marginfi", "SOL", 0.08, 4, 500_000.0),
        mock_opportunity("Kamino", "USDC", 0.03, 2, 2_000_000.0),
    ];

    let filter = YieldFilter::default().with_min_apy(0.06);

    let filtered: Vec<_> = opportunities
        .iter()
        .filter(|o| filter.min_apy.map_or(true, |min| o.apy >= min))
        .collect();

    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].protocol, "Marginfi");
}

#[test]
fn test_yield_filter_by_max_risk() {
    let opportunities = vec![
        mock_opportunity("Solend", "SOL", 0.05, 3, 1_000_000.0),
        mock_opportunity("Marginfi", "SOL", 0.15, 7, 500_000.0),
        mock_opportunity("Kamino", "USDC", 0.03, 2, 2_000_000.0),
    ];

    let filter = YieldFilter::default().with_max_risk(4);

    let filtered: Vec<_> = opportunities
        .iter()
        .filter(|o| {
            filter
                .max_risk_score
                .map_or(true, |max| o.risk_score <= max)
        })
        .collect();

    assert_eq!(filtered.len(), 2);
    assert!(filtered.iter().all(|o| o.risk_score <= 4));
}

#[test]
fn test_yield_filter_by_protocol() {
    let opportunities = vec![
        mock_opportunity("Solend", "SOL", 0.05, 3, 1_000_000.0),
        mock_opportunity("Marginfi", "SOL", 0.08, 4, 500_000.0),
        mock_opportunity("Solend", "USDC", 0.04, 2, 2_000_000.0),
    ];

    let filter = YieldFilter::default().with_protocols(vec!["Solend".to_string()]);

    let filtered: Vec<_> = opportunities
        .iter()
        .filter(|o| {
            filter
                .protocols
                .as_ref()
                .map_or(true, |p| p.contains(&o.protocol))
        })
        .collect();

    assert_eq!(filtered.len(), 2);
    assert!(filtered.iter().all(|o| o.protocol == "Solend"));
}

#[test]
fn test_yield_filter_by_asset() {
    let opportunities = vec![
        mock_opportunity("Solend", "SOL", 0.05, 3, 1_000_000.0),
        mock_opportunity("Marginfi", "SOL", 0.08, 4, 500_000.0),
        mock_opportunity("Kamino", "USDC", 0.03, 2, 2_000_000.0),
    ];

    let filter = YieldFilter::default().with_assets(vec!["SOL".to_string()]);

    let filtered: Vec<_> = opportunities
        .iter()
        .filter(|o| {
            filter
                .assets
                .as_ref()
                .map_or(true, |a| a.iter().any(|asset| o.asset.contains(asset)))
        })
        .collect();

    assert_eq!(filtered.len(), 2);
    assert!(filtered.iter().all(|o| o.asset == "SOL"));
}

#[test]
fn test_yield_sorting_by_apy_desc() {
    let mut opportunities = vec![
        mock_opportunity("Solend", "SOL", 0.05, 3, 1_000_000.0),
        mock_opportunity("Marginfi", "SOL", 0.15, 4, 500_000.0),
        mock_opportunity("Kamino", "USDC", 0.08, 2, 2_000_000.0),
    ];

    opportunities.sort_by(|a, b| {
        b.apy
            .partial_cmp(&a.apy)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    assert_eq!(opportunities[0].apy, 0.15);
    assert_eq!(opportunities[1].apy, 0.08);
    assert_eq!(opportunities[2].apy, 0.05);
}

#[test]
fn test_yield_sorting_by_risk_adjusted_return() {
    let mut opportunities = vec![
        mock_opportunity("High Risk", "SOL", 0.20, 8, 1_000_000.0), // 0.20/8 = 0.025
        mock_opportunity("Medium Risk", "SOL", 0.10, 3, 500_000.0), // 0.10/3 = 0.033
        mock_opportunity("Low Risk", "USDC", 0.05, 1, 2_000_000.0), // 0.05/1 = 0.05
    ];

    opportunities.sort_by(|a, b| {
        let ratio_a = a.apy / (a.risk_score as f64);
        let ratio_b = b.apy / (b.risk_score as f64);
        ratio_b
            .partial_cmp(&ratio_a)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Best risk-adjusted return first
    assert_eq!(opportunities[0].protocol, "Low Risk");
    assert_eq!(opportunities[1].protocol, "Medium Risk");
    assert_eq!(opportunities[2].protocol, "High Risk");
}

#[test]
fn test_yield_filter_with_limit() {
    let opportunities = vec![
        mock_opportunity("A", "SOL", 0.05, 3, 1_000_000.0),
        mock_opportunity("B", "SOL", 0.08, 4, 500_000.0),
        mock_opportunity("C", "USDC", 0.03, 2, 2_000_000.0),
        mock_opportunity("D", "ETH", 0.12, 5, 800_000.0),
    ];

    let filter = YieldFilter::default().with_limit(2);
    let limit = filter.limit.unwrap();

    let limited: Vec<_> = opportunities.into_iter().take(limit).collect();
    assert_eq!(limited.len(), 2);
}

#[test]
fn test_combined_filter_criteria() {
    let opportunities = vec![
        mock_opportunity("Solend", "SOL", 0.07, 3, 1_000_000.0),
        mock_opportunity("Marginfi", "SOL", 0.15, 7, 500_000.0),
        mock_opportunity("Kamino", "USDC", 0.04, 2, 50_000.0), // Low TVL
        mock_opportunity("Marinade", "mSOL", 0.06, 2, 2_000_000.0),
    ];

    let filter = YieldFilter::default()
        .with_min_apy(0.05)
        .with_max_risk(5)
        .with_min_tvl(100_000.0);

    let filtered: Vec<_> = opportunities
        .iter()
        .filter(|o| {
            let passes_apy = filter.min_apy.map_or(true, |min| o.apy >= min);
            let passes_risk = filter
                .max_risk_score
                .map_or(true, |max| o.risk_score <= max);
            let passes_tvl = filter.min_tvl_usd.map_or(true, |min| o.tvl_usd >= min);
            passes_apy && passes_risk && passes_tvl
        })
        .collect();

    assert_eq!(filtered.len(), 2);
    // Should include Solend (0.07 APY, risk 3, 1M TVL) and Marinade (0.06 APY, risk 2, 2M TVL)
    assert!(filtered.iter().any(|o| o.protocol == "Solend"));
    assert!(filtered.iter().any(|o| o.protocol == "Marinade"));
}

#[test]
fn test_approval_config_defaults() {
    let config = ApprovalConfig::default();
    assert_eq!(config.threshold_usd, 100.0);
    assert_eq!(config.expiration_secs, 300);
    assert!(config.always_require.contains(&DeFiAction::Withdraw));
}

#[test]
fn test_approval_required_above_threshold() {
    let config = ApprovalConfig::default();

    // $150 deposit should require approval (above $100 threshold)
    let amount = 150.0;
    assert!(amount > config.threshold_usd);

    // $50 deposit should not require approval
    let small_amount = 50.0;
    assert!(small_amount < config.threshold_usd);
}

#[test]
fn test_defi_action_types() {
    // Verify all action types exist
    let _deposit = DeFiAction::Deposit;
    let _withdraw = DeFiAction::Withdraw;
    let _stake = DeFiAction::Stake;
    let _unstake = DeFiAction::Unstake;
    let _borrow = DeFiAction::Borrow;
    let _repay = DeFiAction::Repay;
    let _claim = DeFiAction::ClaimRewards;

    // Test equality
    assert_eq!(DeFiAction::Deposit, DeFiAction::Deposit);
    assert_ne!(DeFiAction::Deposit, DeFiAction::Withdraw);
}

#[test]
fn test_position_value_aggregation() {
    let positions = vec![
        mock_position("Solend", "SOL", 5000.0, 0.05),
        mock_position("Marginfi", "USDC", 3000.0, 0.04),
        mock_position("Marinade", "mSOL", 2000.0, 0.06),
    ];

    let total_value: f64 = positions.iter().map(|p| p.usd_value).sum();
    assert_eq!(total_value, 10000.0);

    // Weighted average APY
    let weighted_apy: f64 = positions
        .iter()
        .map(|p| p.current_apy * p.usd_value)
        .sum::<f64>()
        / total_value;

    let expected_apy = (0.05 * 5000.0 + 0.04 * 3000.0 + 0.06 * 2000.0) / 10000.0;
    assert!((weighted_apy - expected_apy).abs() < 0.0001);
}

#[test]
fn test_positions_grouped_by_protocol() {
    let positions = vec![
        mock_position("Solend", "SOL", 5000.0, 0.05),
        mock_position("Solend", "USDC", 3000.0, 0.04),
        mock_position("Marinade", "mSOL", 2000.0, 0.06),
    ];

    use std::collections::HashMap;
    let mut by_protocol: HashMap<String, Vec<&Position>> = HashMap::new();
    for pos in &positions {
        by_protocol
            .entry(pos.protocol.clone())
            .or_default()
            .push(pos);
    }

    assert_eq!(by_protocol.len(), 2);
    assert_eq!(by_protocol.get("Solend").unwrap().len(), 2);
    assert_eq!(by_protocol.get("Marinade").unwrap().len(), 1);
}

#[test]
fn test_protocol_type_classification() {
    assert!(matches!(ProtocolType::Lending, ProtocolType::Lending));
    assert!(matches!(
        ProtocolType::LiquidStaking,
        ProtocolType::LiquidStaking
    ));
    assert!(matches!(ProtocolType::Vault, ProtocolType::Vault));
}

#[test]
fn test_position_types() {
    assert!(matches!(PositionType::Supply, PositionType::Supply));
    assert!(matches!(PositionType::Borrow, PositionType::Borrow));
    assert!(matches!(PositionType::Stake, PositionType::Stake));
    assert!(matches!(PositionType::Vault, PositionType::Vault));
    assert!(matches!(PositionType::Liquidity, PositionType::Liquidity));
}

#[test]
fn test_yield_opportunity_with_metadata() {
    let opp = YieldOpportunity::new(
        "Kamino",
        "usdc-vault",
        "USDC",
        Pubkey::new_unique(),
        0.08,
        5_000_000.0,
        3,
    )
    .with_metadata(serde_json::json!({
        "vault_type": "leverage",
        "max_leverage": 3.0
    }));

    assert_eq!(opp.protocol, "Kamino");
    assert_eq!(opp.metadata["vault_type"], "leverage");
}

#[test]
fn test_approval_config_strict_mode() {
    let config = ApprovalConfig::strict();
    assert_eq!(config.threshold_usd, 0.0);
    assert!(config.require_all);
}

#[test]
fn test_approval_config_builders() {
    let config = ApprovalConfig::default()
        .with_threshold(500.0)
        .with_expiration(600);

    assert_eq!(config.threshold_usd, 500.0);
    assert_eq!(config.expiration_secs, 600);
}

#[tokio::test]
async fn test_approval_manager_requires_approval() {
    let manager = DeFiApprovalManager::new(ApprovalConfig::default());

    // Under threshold - no approval needed
    assert!(!manager.requires_approval(DeFiAction::Deposit, 50.0));

    // Over threshold - approval needed
    assert!(manager.requires_approval(DeFiAction::Deposit, 150.0));

    // Withdraw always requires approval
    assert!(manager.requires_approval(DeFiAction::Withdraw, 10.0));
}

#[tokio::test]
async fn test_approval_flow() {
    let manager = DeFiApprovalManager::new(ApprovalConfig::default());
    let opp = mock_opportunity("Marinade", "SOL", 0.07, 2, 100_000_000.0);

    // Request approval
    let pending = manager
        .request_approval(opp, DeFiAction::Stake, 1_000_000_000, 150.0)
        .await
        .unwrap();

    assert_eq!(pending.approval_code.len(), 6);

    // Approve with correct code
    let approved = manager
        .approve(pending.id, &pending.approval_code)
        .await
        .unwrap();

    assert!(matches!(
        approved.status,
        drbot_solana::defi::PendingStatus::Approved
    ));
}
