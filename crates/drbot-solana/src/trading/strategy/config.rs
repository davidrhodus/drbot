//! Trading strategy configuration.

use serde::{Deserialize, Serialize};

/// Trading strategy configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TradingStrategyConfig {
    /// Take profit percentage (e.g., 50.0 = +50%).
    pub take_profit_pct: f64,
    /// Stop loss percentage (e.g., 25.0 = -25%).
    pub stop_loss_pct: f64,
    /// Enable trailing stop.
    pub trailing_stop_enabled: bool,
    /// Trailing stop trigger percentage (activates after this gain).
    pub trailing_stop_trigger_pct: f64,
    /// Trailing stop distance percentage (trails behind peak by this amount).
    pub trailing_stop_distance_pct: f64,
    /// Maximum position size in USD.
    pub max_position_size_usd: f64,
    /// Minimum liquidity required in USD.
    pub min_liquidity_usd: f64,
    /// Minimum 24h volume required in USD.
    pub min_volume_24h_usd: f64,
    /// Maximum token age in hours (for new token strategy).
    pub max_token_age_hours: Option<f64>,
    /// Minimum momentum score (0-100).
    pub min_momentum_score: f64,
    /// Slippage tolerance in basis points.
    pub slippage_bps: u16,
    /// Monitoring interval in seconds.
    pub monitor_interval_secs: u64,
    /// Maximum concurrent positions.
    pub max_positions: usize,
    /// Enable auto-compounding profits.
    pub auto_compound: bool,
    /// Cooldown period after closing position (seconds).
    pub cooldown_secs: u64,

    // === Momentum Death Exit ===
    /// Enable momentum death exit (5m <-8% AND 1h <-15%).
    pub momentum_death_enabled: bool,
    /// 5m price change threshold for momentum death (default: -8.0).
    pub momentum_death_5m_threshold: f64,
    /// 1h price change threshold for momentum death (default: -15.0).
    pub momentum_death_1h_threshold: f64,

    // === SOL Swing Trading ===
    /// Enable SOL swing trading (buy dips, sell pumps).
    pub sol_swing_enabled: bool,
    /// SOL daily drop threshold to trigger buy (default: -7.0%).
    pub sol_buy_dip_threshold: f64,
    /// SOL daily pump threshold to trigger sell (default: +6.0%).
    pub sol_sell_pump_threshold: f64,
    /// Maximum USD to spend on SOL dip buys.
    pub sol_swing_max_usd: f64,
    /// Percentage of SOL holdings to sell on pump.
    pub sol_sell_pct: f64,
    /// Minimum SOL balance to maintain (don't sell below this).
    pub sol_min_balance: f64,
}

impl Default for TradingStrategyConfig {
    fn default() -> Self {
        Self {
            take_profit_pct: 50.0, // +50% take profit
            stop_loss_pct: 25.0,   // -25% stop loss
            trailing_stop_enabled: true,
            trailing_stop_trigger_pct: 30.0, // Activate trailing stop at +30% (OpenClaw default)
            trailing_stop_distance_pct: 15.0, // Trail 15% behind peak (OpenClaw default)
            max_position_size_usd: 100.0,
            min_liquidity_usd: 10_000.0,
            min_volume_24h_usd: 5_000.0,
            max_token_age_hours: Some(24.0),
            min_momentum_score: 25.0, // OpenClaw default minimum score
            slippage_bps: 100,        // 1%
            monitor_interval_secs: 30,
            max_positions: 4, // OpenClaw default
            auto_compound: false,
            cooldown_secs: 300, // 5 minutes

            // Momentum death exit (OpenClaw: 5m <-8% AND 1h <-15%)
            momentum_death_enabled: true,
            momentum_death_5m_threshold: -8.0,
            momentum_death_1h_threshold: -15.0,

            // SOL swing trading (OpenClaw defaults)
            sol_swing_enabled: true,
            sol_buy_dip_threshold: -7.0,  // Buy on -7% daily drop
            sol_sell_pump_threshold: 6.0, // Sell on +6% daily pump
            sol_swing_max_usd: 15.0,      // Max $15 per swing trade
            sol_sell_pct: 30.0,           // Sell 30% of holdings on pump
            sol_min_balance: 0.2,         // Keep at least 0.2 SOL
        }
    }
}

impl TradingStrategyConfig {
    /// Create an aggressive configuration.
    pub fn aggressive() -> Self {
        Self {
            take_profit_pct: 100.0, // +100%
            stop_loss_pct: 30.0,    // -30%
            trailing_stop_trigger_pct: 40.0,
            trailing_stop_distance_pct: 20.0,
            max_position_size_usd: 200.0,
            min_liquidity_usd: 5_000.0,
            min_volume_24h_usd: 2_500.0,
            max_token_age_hours: Some(12.0),
            min_momentum_score: 30.0,
            momentum_death_enabled: true,
            momentum_death_5m_threshold: -10.0, // More tolerant
            momentum_death_1h_threshold: -20.0,
            sol_swing_enabled: true,
            sol_buy_dip_threshold: -10.0,  // Bigger dips for aggressive
            sol_sell_pump_threshold: 10.0, // Bigger pumps
            ..Default::default()
        }
    }

    /// Create a conservative configuration.
    pub fn conservative() -> Self {
        Self {
            take_profit_pct: 25.0, // +25%
            stop_loss_pct: 15.0,   // -15%
            trailing_stop_trigger_pct: 15.0,
            trailing_stop_distance_pct: 5.0,
            max_position_size_usd: 50.0,
            min_liquidity_usd: 50_000.0,
            min_volume_24h_usd: 25_000.0,
            max_token_age_hours: None, // No age restriction
            min_momentum_score: 40.0,
            momentum_death_enabled: true,
            momentum_death_5m_threshold: -5.0, // Tighter for conservative
            momentum_death_1h_threshold: -10.0,
            sol_swing_enabled: true,
            sol_buy_dip_threshold: -5.0,  // Smaller dips
            sol_sell_pump_threshold: 4.0, // Smaller pumps
            sol_swing_max_usd: 10.0,
            ..Default::default()
        }
    }

    /// Create a scalping configuration (quick in/out).
    pub fn scalping() -> Self {
        Self {
            take_profit_pct: 10.0, // +10%
            stop_loss_pct: 5.0,    // -5%
            trailing_stop_enabled: false,
            trailing_stop_trigger_pct: 0.0,
            trailing_stop_distance_pct: 0.0,
            max_position_size_usd: 50.0,
            min_liquidity_usd: 25_000.0,
            min_volume_24h_usd: 10_000.0,
            max_token_age_hours: None,
            min_momentum_score: 20.0,
            monitor_interval_secs: 10,
            max_positions: 5,
            cooldown_secs: 60,
            momentum_death_enabled: true,
            momentum_death_5m_threshold: -3.0, // Very tight for scalping
            momentum_death_1h_threshold: -5.0,
            sol_swing_enabled: false, // No swing trading for scalping
            ..Default::default()
        }
    }

    /// Validate the configuration.
    pub fn validate(&self) -> Result<(), String> {
        if self.take_profit_pct <= 0.0 {
            return Err("Take profit must be positive".to_string());
        }
        if self.stop_loss_pct <= 0.0 {
            return Err("Stop loss must be positive".to_string());
        }
        if self.trailing_stop_enabled {
            if self.trailing_stop_trigger_pct <= 0.0 {
                return Err("Trailing stop trigger must be positive".to_string());
            }
            if self.trailing_stop_distance_pct <= 0.0 {
                return Err("Trailing stop distance must be positive".to_string());
            }
            if self.trailing_stop_distance_pct >= self.trailing_stop_trigger_pct {
                return Err("Trailing stop distance must be less than trigger".to_string());
            }
        }
        if self.max_position_size_usd <= 0.0 {
            return Err("Max position size must be positive".to_string());
        }
        if self.max_positions == 0 {
            return Err("Max positions must be at least 1".to_string());
        }
        Ok(())
    }
}

/// Trading action to take.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TradingAction {
    /// Hold the position.
    Hold,
    /// Take profit - sell at target.
    TakeProfit,
    /// Stop loss - sell to limit losses.
    StopLoss,
    /// Trailing stop triggered.
    TrailingStop,
    /// Momentum death - rapid price decline on short timeframes.
    MomentumDeath,
    /// Manual exit requested.
    ManualExit,
}

impl std::fmt::Display for TradingAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Hold => write!(f, "HOLD"),
            Self::TakeProfit => write!(f, "TAKE_PROFIT"),
            Self::StopLoss => write!(f, "STOP_LOSS"),
            Self::TrailingStop => write!(f, "TRAILING_STOP"),
            Self::MomentumDeath => write!(f, "MOMENTUM_DEATH"),
            Self::ManualExit => write!(f, "MANUAL_EXIT"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = TradingStrategyConfig::default();
        assert_eq!(config.take_profit_pct, 50.0);
        assert_eq!(config.stop_loss_pct, 25.0);
        assert!(config.trailing_stop_enabled);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_config_validation() {
        let mut config = TradingStrategyConfig::default();

        config.take_profit_pct = -10.0;
        assert!(config.validate().is_err());

        config.take_profit_pct = 50.0;
        config.trailing_stop_distance_pct = 50.0; // Greater than trigger
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_presets() {
        assert!(TradingStrategyConfig::aggressive().validate().is_ok());
        assert!(TradingStrategyConfig::conservative().validate().is_ok());
        assert!(TradingStrategyConfig::scalping().validate().is_ok());
    }
}
