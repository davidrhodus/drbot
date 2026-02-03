//! Position tracking for trading.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A trading position.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    /// Unique position ID.
    pub id: String,
    /// Token mint address.
    pub token_mint: String,
    /// Token symbol.
    pub token_symbol: String,
    /// Entry price in USD.
    pub entry_price: f64,
    /// Current price in USD.
    pub current_price: f64,
    /// Highest price seen since entry.
    pub peak_price: f64,
    /// Amount of tokens held.
    pub amount: f64,
    /// Position size in USD at entry.
    pub entry_value_usd: f64,
    /// Current position value in USD.
    pub current_value_usd: f64,
    /// Entry transaction signature.
    pub entry_signature: String,
    /// Exit transaction signature (if closed).
    pub exit_signature: Option<String>,
    /// Position opened at.
    pub opened_at: DateTime<Utc>,
    /// Position closed at.
    pub closed_at: Option<DateTime<Utc>>,
    /// Position status.
    pub status: PositionStatus,
    /// Profit/loss percentage.
    pub pnl_pct: f64,
    /// Profit/loss in USD.
    pub pnl_usd: f64,
    /// Whether trailing stop is active.
    pub trailing_stop_active: bool,
    /// Trailing stop price (if active).
    pub trailing_stop_price: Option<f64>,
    /// Close reason (if closed).
    pub close_reason: Option<String>,
    /// Momentum score at entry.
    pub entry_momentum_score: f64,
    /// Source of the opportunity.
    pub source: String,
    /// Current 5m price change percentage.
    pub price_change_5m: f64,
    /// Current 1h price change percentage.
    pub price_change_1h: f64,
}

impl Position {
    /// Create a new open position.
    pub fn new(
        token_mint: String,
        token_symbol: String,
        entry_price: f64,
        amount: f64,
        entry_signature: String,
        momentum_score: f64,
        source: String,
    ) -> Self {
        let entry_value_usd = entry_price * amount;
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            token_mint,
            token_symbol,
            entry_price,
            current_price: entry_price,
            peak_price: entry_price,
            amount,
            entry_value_usd,
            current_value_usd: entry_value_usd,
            entry_signature,
            exit_signature: None,
            opened_at: Utc::now(),
            closed_at: None,
            status: PositionStatus::Open,
            pnl_pct: 0.0,
            pnl_usd: 0.0,
            trailing_stop_active: false,
            trailing_stop_price: None,
            close_reason: None,
            entry_momentum_score: momentum_score,
            source,
            price_change_5m: 0.0,
            price_change_1h: 0.0,
        }
    }

    /// Update the position with a new price.
    pub fn update_price(&mut self, new_price: f64) {
        self.current_price = new_price;
        self.current_value_usd = new_price * self.amount;

        // Update peak price
        if new_price > self.peak_price {
            self.peak_price = new_price;
        }

        // Calculate P&L
        self.pnl_usd = self.current_value_usd - self.entry_value_usd;
        self.pnl_pct = ((self.current_price / self.entry_price) - 1.0) * 100.0;
    }

    /// Update price with momentum data for momentum death detection.
    pub fn update_price_with_momentum(
        &mut self,
        new_price: f64,
        price_change_5m: f64,
        price_change_1h: f64,
    ) {
        self.update_price(new_price);
        self.price_change_5m = price_change_5m;
        self.price_change_1h = price_change_1h;
    }

    /// Check if momentum death condition is met.
    pub fn is_momentum_dead(&self, m5_threshold: f64, h1_threshold: f64) -> bool {
        self.price_change_5m < m5_threshold && self.price_change_1h < h1_threshold
    }

    /// Activate trailing stop at the given price.
    pub fn activate_trailing_stop(&mut self, stop_price: f64) {
        self.trailing_stop_active = true;
        self.trailing_stop_price = Some(stop_price);
    }

    /// Update trailing stop price.
    pub fn update_trailing_stop(&mut self, new_stop_price: f64) {
        if let Some(current_stop) = self.trailing_stop_price {
            // Only move stop up, never down
            if new_stop_price > current_stop {
                self.trailing_stop_price = Some(new_stop_price);
            }
        }
    }

    /// Close the position.
    pub fn close(&mut self, exit_signature: String, reason: &str) {
        self.status = PositionStatus::Closed;
        self.exit_signature = Some(exit_signature);
        self.closed_at = Some(Utc::now());
        self.close_reason = Some(reason.to_string());
    }

    /// Check if position is open.
    pub fn is_open(&self) -> bool {
        self.status == PositionStatus::Open
    }

    /// Get position duration.
    pub fn duration(&self) -> chrono::Duration {
        let end = self.closed_at.unwrap_or_else(Utc::now);
        end - self.opened_at
    }

    /// Get duration in a human-readable format.
    pub fn duration_str(&self) -> String {
        let duration = self.duration();
        let hours = duration.num_hours();
        let minutes = duration.num_minutes() % 60;
        let seconds = duration.num_seconds() % 60;

        if hours > 0 {
            format!("{}h {}m", hours, minutes)
        } else if minutes > 0 {
            format!("{}m {}s", minutes, seconds)
        } else {
            format!("{}s", seconds)
        }
    }

    /// Get price change from peak (for trailing stop calculation).
    pub fn change_from_peak_pct(&self) -> f64 {
        ((self.current_price / self.peak_price) - 1.0) * 100.0
    }
}

/// Position status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PositionStatus {
    /// Position is open.
    Open,
    /// Position is closed.
    Closed,
    /// Position is pending (order submitted but not confirmed).
    Pending,
    /// Position failed to open.
    Failed,
}

/// Position summary for reporting.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionSummary {
    /// Total positions opened.
    pub total_positions: usize,
    /// Currently open positions.
    pub open_positions: usize,
    /// Closed positions.
    pub closed_positions: usize,
    /// Winning trades (closed with profit).
    pub winning_trades: usize,
    /// Losing trades (closed with loss).
    pub losing_trades: usize,
    /// Win rate percentage.
    pub win_rate_pct: f64,
    /// Total realized P&L in USD.
    pub total_realized_pnl_usd: f64,
    /// Total unrealized P&L in USD.
    pub total_unrealized_pnl_usd: f64,
    /// Average profit on winning trades.
    pub avg_win_pct: f64,
    /// Average loss on losing trades.
    pub avg_loss_pct: f64,
    /// Best trade P&L percentage.
    pub best_trade_pct: f64,
    /// Worst trade P&L percentage.
    pub worst_trade_pct: f64,
    /// Average position duration.
    pub avg_duration_secs: i64,
}

impl PositionSummary {
    /// Calculate summary from positions.
    pub fn from_positions(positions: &[Position]) -> Self {
        let total_positions = positions.len();
        let open_positions = positions.iter().filter(|p| p.is_open()).count();
        let closed: Vec<_> = positions.iter().filter(|p| !p.is_open()).collect();
        let closed_positions = closed.len();

        let winning_trades = closed.iter().filter(|p| p.pnl_usd > 0.0).count();
        let losing_trades = closed.iter().filter(|p| p.pnl_usd < 0.0).count();

        let win_rate_pct = if closed_positions > 0 {
            (winning_trades as f64 / closed_positions as f64) * 100.0
        } else {
            0.0
        };

        let total_realized_pnl_usd: f64 = closed.iter().map(|p| p.pnl_usd).sum();
        let total_unrealized_pnl_usd: f64 = positions
            .iter()
            .filter(|p| p.is_open())
            .map(|p| p.pnl_usd)
            .sum();

        let wins: Vec<f64> = closed
            .iter()
            .filter(|p| p.pnl_pct > 0.0)
            .map(|p| p.pnl_pct)
            .collect();
        let losses: Vec<f64> = closed
            .iter()
            .filter(|p| p.pnl_pct < 0.0)
            .map(|p| p.pnl_pct)
            .collect();

        let avg_win_pct = if wins.is_empty() {
            0.0
        } else {
            wins.iter().sum::<f64>() / wins.len() as f64
        };

        let avg_loss_pct = if losses.is_empty() {
            0.0
        } else {
            losses.iter().sum::<f64>() / losses.len() as f64
        };

        let best_trade_pct = closed
            .iter()
            .map(|p| p.pnl_pct)
            .max_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap_or(0.0);

        let worst_trade_pct = closed
            .iter()
            .map(|p| p.pnl_pct)
            .min_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap_or(0.0);

        let avg_duration_secs = if closed_positions > 0 {
            closed
                .iter()
                .map(|p| p.duration().num_seconds())
                .sum::<i64>()
                / closed_positions as i64
        } else {
            0
        };

        Self {
            total_positions,
            open_positions,
            closed_positions,
            winning_trades,
            losing_trades,
            win_rate_pct,
            total_realized_pnl_usd,
            total_unrealized_pnl_usd,
            avg_win_pct,
            avg_loss_pct,
            best_trade_pct,
            worst_trade_pct,
            avg_duration_secs,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_position_pnl() {
        let mut position = Position::new(
            "token123".to_string(),
            "TEST".to_string(),
            1.0,   // Entry at $1.00
            100.0, // 100 tokens
            "sig123".to_string(),
            75.0,
            "dexscreener".to_string(),
        );

        assert_eq!(position.entry_value_usd, 100.0);
        assert_eq!(position.pnl_pct, 0.0);

        // Price goes up 50%
        position.update_price(1.5);
        assert_eq!(position.current_value_usd, 150.0);
        assert_eq!(position.pnl_pct, 50.0);
        assert_eq!(position.pnl_usd, 50.0);

        // Price drops to 20% below entry
        position.update_price(0.8);
        assert!((position.pnl_pct - (-20.0)).abs() < 0.0001);
        assert_eq!(position.peak_price, 1.5); // Peak should remain
    }

    #[test]
    fn test_trailing_stop() {
        let mut position = Position::new(
            "token123".to_string(),
            "TEST".to_string(),
            1.0,
            100.0,
            "sig123".to_string(),
            75.0,
            "dexscreener".to_string(),
        );

        position.activate_trailing_stop(0.9);
        assert!(position.trailing_stop_active);
        assert_eq!(position.trailing_stop_price, Some(0.9));

        // Try to move stop up
        position.update_trailing_stop(1.1);
        assert_eq!(position.trailing_stop_price, Some(1.1));

        // Try to move stop down (should not work)
        position.update_trailing_stop(1.0);
        assert_eq!(position.trailing_stop_price, Some(1.1));
    }

    #[test]
    fn test_position_summary() {
        let mut positions = vec![
            Position::new(
                "t1".into(),
                "T1".into(),
                1.0,
                100.0,
                "s1".into(),
                70.0,
                "ds".into(),
            ),
            Position::new(
                "t2".into(),
                "T2".into(),
                1.0,
                100.0,
                "s2".into(),
                80.0,
                "ds".into(),
            ),
        ];

        // Close first with profit
        positions[0].update_price(1.5);
        positions[0].close("exit1".into(), "take_profit");

        // Close second with loss
        positions[1].update_price(0.8);
        positions[1].close("exit2".into(), "stop_loss");

        let summary = PositionSummary::from_positions(&positions);
        assert_eq!(summary.total_positions, 2);
        assert_eq!(summary.closed_positions, 2);
        assert_eq!(summary.winning_trades, 1);
        assert_eq!(summary.losing_trades, 1);
        assert_eq!(summary.win_rate_pct, 50.0);
    }
}
