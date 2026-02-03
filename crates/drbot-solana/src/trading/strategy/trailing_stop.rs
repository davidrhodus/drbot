//! Trailing stop implementation.

use super::{Position, TradingAction, TradingStrategyConfig};

/// Trailing stop manager.
#[derive(Debug, Clone)]
pub struct TrailingStopManager {
    /// Trigger percentage (when to activate trailing stop).
    trigger_pct: f64,
    /// Distance percentage (how far behind peak to set stop).
    distance_pct: f64,
}

impl TrailingStopManager {
    /// Create a new trailing stop manager.
    pub fn new(trigger_pct: f64, distance_pct: f64) -> Self {
        Self {
            trigger_pct,
            distance_pct,
        }
    }

    /// Create from config.
    pub fn from_config(config: &TradingStrategyConfig) -> Self {
        Self {
            trigger_pct: config.trailing_stop_trigger_pct,
            distance_pct: config.trailing_stop_distance_pct,
        }
    }

    /// Check and update trailing stop for a position.
    /// Returns the action to take.
    pub fn check(&self, position: &mut Position) -> TradingAction {
        let gain_pct = position.pnl_pct;

        // Check if we should activate trailing stop
        if !position.trailing_stop_active && gain_pct >= self.trigger_pct {
            let stop_price = self.calculate_stop_price(position.peak_price);
            position.activate_trailing_stop(stop_price);
        }

        // If trailing stop is active, update and check
        if position.trailing_stop_active {
            // Update stop price based on new peak
            let new_stop_price = self.calculate_stop_price(position.peak_price);
            position.update_trailing_stop(new_stop_price);

            // Check if stop is triggered
            if let Some(stop_price) = position.trailing_stop_price {
                if position.current_price <= stop_price {
                    return TradingAction::TrailingStop;
                }
            }
        }

        TradingAction::Hold
    }

    /// Calculate stop price based on peak price.
    fn calculate_stop_price(&self, peak_price: f64) -> f64 {
        peak_price * (1.0 - self.distance_pct / 100.0)
    }

    /// Get current stop price for a position.
    pub fn current_stop_price(&self, position: &Position) -> Option<f64> {
        if position.trailing_stop_active {
            position.trailing_stop_price
        } else if position.pnl_pct >= self.trigger_pct {
            // Would activate at this price
            Some(self.calculate_stop_price(position.peak_price))
        } else {
            None
        }
    }

    /// Get the gain percentage needed to activate trailing stop.
    pub fn trigger_threshold(&self) -> f64 {
        self.trigger_pct
    }

    /// Get the distance percentage from peak.
    pub fn distance(&self) -> f64 {
        self.distance_pct
    }
}

/// Trailing stop status for reporting.
#[derive(Debug, Clone)]
pub struct TrailingStopStatus {
    /// Whether trailing stop is active.
    pub active: bool,
    /// Current stop price (if active).
    pub stop_price: Option<f64>,
    /// Distance from current price to stop.
    pub distance_to_stop: Option<f64>,
    /// Distance as percentage.
    pub distance_to_stop_pct: Option<f64>,
    /// Gain needed to activate (if not active).
    pub gain_to_activate: Option<f64>,
}

impl TrailingStopStatus {
    /// Get status for a position.
    pub fn for_position(position: &Position, manager: &TrailingStopManager) -> Self {
        if position.trailing_stop_active {
            let stop_price = position.trailing_stop_price.unwrap_or(0.0);
            let distance = position.current_price - stop_price;
            let distance_pct = (distance / position.current_price) * 100.0;

            Self {
                active: true,
                stop_price: position.trailing_stop_price,
                distance_to_stop: Some(distance),
                distance_to_stop_pct: Some(distance_pct),
                gain_to_activate: None,
            }
        } else {
            let gain_needed = manager.trigger_threshold() - position.pnl_pct;
            Self {
                active: false,
                stop_price: None,
                distance_to_stop: None,
                distance_to_stop_pct: None,
                gain_to_activate: Some(gain_needed.max(0.0)),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_position(entry_price: f64, current_price: f64) -> Position {
        let mut position = Position::new(
            "token".to_string(),
            "TEST".to_string(),
            entry_price,
            100.0,
            "sig".to_string(),
            70.0,
            "test".to_string(),
        );
        position.update_price(current_price);
        position
    }

    #[test]
    fn test_trailing_stop_activation() {
        let manager = TrailingStopManager::new(20.0, 10.0); // Activate at +20%, trail 10%

        // Position at +15% - should not activate
        let mut position = create_test_position(1.0, 1.15);
        let action = manager.check(&mut position);
        assert_eq!(action, TradingAction::Hold);
        assert!(!position.trailing_stop_active);

        // Position at +25% - should activate
        let mut position2 = create_test_position(1.0, 1.25);
        let action2 = manager.check(&mut position2);
        assert_eq!(action2, TradingAction::Hold);
        assert!(position2.trailing_stop_active);
        // Stop should be at 1.25 * 0.9 = 1.125
        assert!((position2.trailing_stop_price.unwrap() - 1.125).abs() < 0.001);
    }

    #[test]
    fn test_trailing_stop_triggered() {
        let manager = TrailingStopManager::new(20.0, 10.0);

        // Create position, activate trailing stop
        let mut position = create_test_position(1.0, 1.30);
        position.peak_price = 1.50; // Peak was at 1.50
        manager.check(&mut position);
        assert!(position.trailing_stop_active);
        // Stop at 1.50 * 0.9 = 1.35

        // Price drops below stop
        position.update_price(1.30);
        let action = manager.check(&mut position);
        assert_eq!(action, TradingAction::TrailingStop);
    }

    #[test]
    fn test_stop_only_moves_up() {
        let manager = TrailingStopManager::new(20.0, 10.0);

        let mut position = create_test_position(1.0, 1.50);
        manager.check(&mut position);
        let initial_stop = position.trailing_stop_price.unwrap();

        // Price goes higher
        position.update_price(1.60);
        manager.check(&mut position);
        let higher_stop = position.trailing_stop_price.unwrap();
        assert!(higher_stop > initial_stop);

        // Price drops (but not below stop)
        position.update_price(1.50);
        manager.check(&mut position);
        // Stop should not have moved down
        assert_eq!(position.trailing_stop_price.unwrap(), higher_stop);
    }
}
