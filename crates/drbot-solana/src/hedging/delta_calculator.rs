//! Portfolio delta and beta calculation.
//!
//! Calculates directional exposure of a portfolio to various assets
//! and market factors.

use crate::defi::Position;
use serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;
use std::collections::HashMap;

/// Portfolio delta analysis result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortfolioDelta {
    /// Total portfolio delta (net directional exposure in USD).
    pub total_delta: f64,
    /// Beta to SOL (correlation-adjusted exposure).
    pub beta_to_sol: f64,
    /// Beta to BTC.
    pub beta_to_btc: f64,
    /// Per-position deltas.
    pub position_deltas: Vec<PositionDelta>,
    /// Whether portfolio is considered market neutral.
    pub is_market_neutral: bool,
    /// Delta by asset class.
    pub delta_by_class: HashMap<AssetClass, f64>,
    /// Net long/short breakdown.
    pub long_exposure: f64,
    /// Net short exposure.
    pub short_exposure: f64,
}

impl PortfolioDelta {
    /// Get the delta as a percentage of total portfolio value.
    pub fn delta_percentage(&self) -> f64 {
        let total_exposure = self.long_exposure + self.short_exposure.abs();
        if total_exposure > 0.0 {
            (self.total_delta / total_exposure) * 100.0
        } else {
            0.0
        }
    }

    /// Check if portfolio has significant directional bias.
    pub fn has_directional_bias(&self, threshold_pct: f64) -> bool {
        self.delta_percentage().abs() > threshold_pct
    }
}

/// Delta for a single position.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionDelta {
    /// Position identifier.
    pub position_id: String,
    /// Asset symbol.
    pub asset: String,
    /// Asset mint.
    pub asset_mint: Pubkey,
    /// Position value in USD.
    pub value_usd: f64,
    /// Delta (directional exposure).
    pub delta: f64,
    /// Effective delta after hedges.
    pub effective_delta: f64,
    /// Position direction.
    pub direction: PositionDirection,
    /// Asset class.
    pub asset_class: AssetClass,
    /// Beta to SOL.
    pub beta_sol: f64,
}

/// Position direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PositionDirection {
    /// Long position (profits when price goes up).
    Long,
    /// Short position (profits when price goes down).
    Short,
    /// Neutral position (no directional exposure, e.g., stablecoins).
    Neutral,
}

/// Asset classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetClass {
    /// Stablecoins (USDC, USDT, etc.).
    Stablecoin,
    /// SOL and SOL derivatives.
    Sol,
    /// BTC and BTC derivatives.
    Btc,
    /// ETH and ETH derivatives.
    Eth,
    /// Other altcoins.
    Altcoin,
    /// Liquidity pool tokens.
    LpToken,
}

/// Delta calculator for portfolios.
pub struct DeltaCalculator {
    /// Beta estimates for assets relative to SOL.
    sol_betas: HashMap<String, f64>,
    /// Beta estimates for assets relative to BTC.
    btc_betas: HashMap<String, f64>,
    /// Asset class mappings.
    asset_classes: HashMap<String, AssetClass>,
    /// Market neutral threshold (percentage).
    neutral_threshold: f64,
}

impl DeltaCalculator {
    /// Create a new delta calculator with default parameters.
    pub fn new() -> Self {
        let mut sol_betas = HashMap::new();
        let mut btc_betas = HashMap::new();
        let mut asset_classes = HashMap::new();

        // SOL and derivatives have beta ~1 to SOL
        sol_betas.insert("SOL".to_string(), 1.0);
        sol_betas.insert("mSOL".to_string(), 1.0);
        sol_betas.insert("JitoSOL".to_string(), 1.0);
        sol_betas.insert("stSOL".to_string(), 1.0);
        sol_betas.insert("bSOL".to_string(), 1.0);

        // Stablecoins have 0 beta
        sol_betas.insert("USDC".to_string(), 0.0);
        sol_betas.insert("USDT".to_string(), 0.0);
        sol_betas.insert("USDH".to_string(), 0.0);

        // Other assets have estimated betas
        sol_betas.insert("BTC".to_string(), 0.7);
        sol_betas.insert("ETH".to_string(), 0.75);
        sol_betas.insert("RAY".to_string(), 1.2);
        sol_betas.insert("ORCA".to_string(), 1.3);
        sol_betas.insert("JTO".to_string(), 1.1);
        sol_betas.insert("BONK".to_string(), 1.5);
        sol_betas.insert("WIF".to_string(), 1.6);

        // BTC betas
        btc_betas.insert("BTC".to_string(), 1.0);
        btc_betas.insert("WBTC".to_string(), 1.0);
        btc_betas.insert("SOL".to_string(), 0.7);
        btc_betas.insert("ETH".to_string(), 0.85);

        // Asset classes
        for stable in &["USDC", "USDT", "USDH", "DAI", "FRAX"] {
            asset_classes.insert(stable.to_string(), AssetClass::Stablecoin);
        }
        for sol in &["SOL", "mSOL", "JitoSOL", "stSOL", "bSOL"] {
            asset_classes.insert(sol.to_string(), AssetClass::Sol);
        }
        asset_classes.insert("BTC".to_string(), AssetClass::Btc);
        asset_classes.insert("WBTC".to_string(), AssetClass::Btc);
        asset_classes.insert("ETH".to_string(), AssetClass::Eth);
        asset_classes.insert("WETH".to_string(), AssetClass::Eth);

        Self {
            sol_betas,
            btc_betas,
            asset_classes,
            neutral_threshold: 10.0, // 10% delta considered neutral
        }
    }

    /// Set the market neutral threshold.
    pub fn with_neutral_threshold(mut self, threshold: f64) -> Self {
        self.neutral_threshold = threshold;
        self
    }

    /// Add a custom beta estimate.
    pub fn with_sol_beta(mut self, asset: impl Into<String>, beta: f64) -> Self {
        self.sol_betas.insert(asset.into(), beta);
        self
    }

    /// Calculate portfolio delta from positions.
    pub fn calculate(&self, positions: &[Position]) -> PortfolioDelta {
        let mut position_deltas = Vec::new();
        let mut total_delta = 0.0;
        let mut weighted_beta_sol = 0.0;
        let mut weighted_beta_btc = 0.0;
        let mut total_value = 0.0;
        let mut long_exposure = 0.0;
        let mut short_exposure = 0.0;
        let mut delta_by_class: HashMap<AssetClass, f64> = HashMap::new();

        for pos in positions {
            let asset = &pos.asset_symbol;
            let value = pos.usd_value;

            // Get betas
            let beta_sol = self.get_sol_beta(asset);
            let beta_btc = self.btc_betas.get(asset).copied().unwrap_or(0.5);

            // Get asset class
            let asset_class = self.get_asset_class(asset);

            // Determine direction
            let direction = self.determine_direction(asset, &pos);

            // Calculate delta
            let delta = match direction {
                PositionDirection::Long => value * beta_sol,
                PositionDirection::Short => -value * beta_sol,
                PositionDirection::Neutral => 0.0,
            };

            // Track exposures
            match direction {
                PositionDirection::Long => long_exposure += value,
                PositionDirection::Short => short_exposure += value,
                PositionDirection::Neutral => {}
            }

            // Accumulate
            total_delta += delta;
            weighted_beta_sol += beta_sol * value;
            weighted_beta_btc += beta_btc * value;
            total_value += value;
            *delta_by_class.entry(asset_class).or_default() += delta;

            position_deltas.push(PositionDelta {
                position_id: pos.id.clone(),
                asset: asset.clone(),
                asset_mint: pos.asset_mint,
                value_usd: value,
                delta,
                effective_delta: delta, // Same as delta for now
                direction,
                asset_class,
                beta_sol,
            });
        }

        // Calculate portfolio betas
        let beta_to_sol = if total_value > 0.0 {
            weighted_beta_sol / total_value
        } else {
            0.0
        };

        let beta_to_btc = if total_value > 0.0 {
            weighted_beta_btc / total_value
        } else {
            0.0
        };

        // Check if market neutral
        let delta_pct = if total_value > 0.0 {
            (total_delta.abs() / total_value) * 100.0
        } else {
            0.0
        };
        let is_market_neutral = delta_pct < self.neutral_threshold;

        PortfolioDelta {
            total_delta,
            beta_to_sol,
            beta_to_btc,
            position_deltas,
            is_market_neutral,
            delta_by_class,
            long_exposure,
            short_exposure,
        }
    }

    /// Get SOL beta for an asset.
    fn get_sol_beta(&self, asset: &str) -> f64 {
        // Try exact match first
        if let Some(&beta) = self.sol_betas.get(asset) {
            return beta;
        }

        // Check if it's a known type
        if asset.contains("USD") {
            return 0.0; // Stablecoin
        }
        if asset.contains("SOL") || asset.contains("mSOL") {
            return 1.0;
        }
        if asset.contains("BTC") {
            return 0.7;
        }
        if asset.contains("ETH") {
            return 0.75;
        }

        // Default beta for unknown assets
        0.8
    }

    /// Get asset class.
    fn get_asset_class(&self, asset: &str) -> AssetClass {
        if let Some(&class) = self.asset_classes.get(asset) {
            return class;
        }

        // Infer from name
        if asset.contains("USD") {
            AssetClass::Stablecoin
        } else if asset.contains("SOL") {
            AssetClass::Sol
        } else if asset.contains("BTC") {
            AssetClass::Btc
        } else if asset.contains("ETH") {
            AssetClass::Eth
        } else if asset.contains("-") || asset.contains("/") {
            AssetClass::LpToken
        } else {
            AssetClass::Altcoin
        }
    }

    /// Determine position direction.
    fn determine_direction(&self, asset: &str, pos: &Position) -> PositionDirection {
        // Stablecoins are neutral
        if self.get_asset_class(asset) == AssetClass::Stablecoin {
            return PositionDirection::Neutral;
        }

        // Check position type for borrow (short) vs supply (long)
        match pos.position_type {
            crate::defi::PositionType::Borrow => PositionDirection::Short,
            crate::defi::PositionType::Supply
            | crate::defi::PositionType::Stake
            | crate::defi::PositionType::Vault
            | crate::defi::PositionType::Liquidity => PositionDirection::Long,
        }
    }
}

impl Default for DeltaCalculator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::defi::PositionType;

    fn test_position(symbol: &str, value: f64, pos_type: PositionType) -> Position {
        Position {
            protocol: "Test".to_string(),
            id: format!("test-{}", symbol),
            position_type: pos_type,
            asset_mint: Pubkey::new_unique(),
            asset_symbol: symbol.to_string(),
            amount: (value * 1e9) as u64,
            usd_value: value,
            current_apy: 0.05,
            unclaimed_rewards: vec![],
        }
    }

    #[test]
    fn test_delta_calculation() {
        let calculator = DeltaCalculator::new();

        let positions = vec![
            test_position("SOL", 1000.0, PositionType::Stake),
            test_position("USDC", 500.0, PositionType::Supply),
        ];

        let delta = calculator.calculate(&positions);

        // SOL position should have full delta, USDC should be neutral
        assert!((delta.total_delta - 1000.0).abs() < 0.01);
        assert_eq!(delta.position_deltas.len(), 2);
    }

    #[test]
    fn test_neutral_detection() {
        let calculator = DeltaCalculator::new().with_neutral_threshold(10.0);

        // Equal long and short
        let positions = vec![
            test_position("SOL", 1000.0, PositionType::Stake),
            test_position("SOL", 1000.0, PositionType::Borrow),
        ];

        let delta = calculator.calculate(&positions);

        // Delta should be near zero
        assert!(delta.total_delta.abs() < 1.0);
        assert!(delta.is_market_neutral);
    }

    #[test]
    fn test_beta_calculation() {
        let calculator = DeltaCalculator::new();

        let positions = vec![
            test_position("SOL", 500.0, PositionType::Stake),
            test_position("mSOL", 500.0, PositionType::Stake),
        ];

        let delta = calculator.calculate(&positions);

        // Both have beta 1 to SOL
        assert!((delta.beta_to_sol - 1.0).abs() < 0.01);
    }
}
