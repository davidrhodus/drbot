//! Momentum-based trading strategy with OpenClaw-style scoring.

use crate::discovery::TokenOpportunity;
use serde::{Deserialize, Serialize};

/// Momentum score calculation for opportunities.
///
/// Scoring algorithm based on OpenClaw strategy:
/// - FDV-based bonuses (micro/small/mid cap)
/// - 5m/1h price momentum
/// - Volume/liquidity ratio
/// - Buy/sell pressure
/// - Penalties for overextension
#[derive(Debug, Clone)]
pub struct MomentumScorer {
    /// Weight for FDV (market cap) factor.
    pub fdv_weight: f64,
    /// Weight for momentum factor.
    pub momentum_weight: f64,
    /// Weight for volume/liquidity ratio.
    pub volume_liq_weight: f64,
    /// Weight for buy pressure.
    pub buy_pressure_weight: f64,
    /// Weight for age factor (newer = higher score).
    pub age_weight: f64,
}

impl Default for MomentumScorer {
    fn default() -> Self {
        Self {
            fdv_weight: 0.20,
            momentum_weight: 0.30,
            volume_liq_weight: 0.25,
            buy_pressure_weight: 0.15,
            age_weight: 0.10,
        }
    }
}

impl MomentumScorer {
    /// Calculate momentum score for an opportunity (0-100).
    /// Uses OpenClaw-style scoring with FDV, momentum, volume, and buy pressure.
    pub fn score(&self, opportunity: &TokenOpportunity) -> MomentumScore {
        let mut raw_score = 0.0;
        let mut details = ScoreDetails::default();

        // 1. FDV-based bonus (30 pts max)
        let fdv_score = self.score_fdv(opportunity.fdv);
        details.fdv_bonus = fdv_score;
        raw_score += fdv_score;

        // 2. Momentum scoring (45 pts max)
        let momentum_score = self.score_momentum(
            opportunity.price_change_5m,
            opportunity.price_change_1h,
            opportunity.price_change_24h,
        );
        details.momentum_score = momentum_score;
        raw_score += momentum_score;

        // 3. Volume/Liquidity ratio (35 pts max)
        let vol_liq_score = self.score_volume_liquidity(opportunity.volume_liquidity_ratio());
        details.volume_liq_score = vol_liq_score;
        raw_score += vol_liq_score;

        // 4. Buy pressure (25 pts max)
        let buy_pressure_score = self.score_buy_pressure(opportunity.buy_sell_ratio());
        details.buy_pressure_score = buy_pressure_score;
        raw_score += buy_pressure_score;

        // 5. Apply penalties
        let penalties = self.calculate_penalties(
            opportunity.price_change_5m,
            opportunity.price_change_1h,
            opportunity.price_change_24h,
            opportunity.liquidity_usd,
            opportunity.buy_sell_ratio(),
        );
        details.penalties = penalties;
        raw_score += penalties; // Penalties are negative

        // Clamp to 0-100
        let total = raw_score.max(0.0).min(100.0);

        MomentumScore {
            total,
            fdv: details.fdv_bonus,
            momentum: details.momentum_score,
            volume_liq: details.volume_liq_score,
            buy_pressure: details.buy_pressure_score,
            penalties: details.penalties,
            details: Some(details),
        }
    }

    /// Score based on FDV (Fully Diluted Valuation).
    /// Lower FDV = more upside potential.
    fn score_fdv(&self, fdv: Option<f64>) -> f64 {
        match fdv {
            None => 0.0,
            Some(fdv) if fdv <= 0.0 => 0.0,
            Some(fdv) if fdv < 500_000.0 => 30.0, // Micro cap: highest upside
            Some(fdv) if fdv < 2_000_000.0 => 20.0, // Small cap: good upside
            Some(fdv) if fdv < 10_000_000.0 => 10.0, // Mid cap: moderate upside
            Some(_) => 0.0,                       // Large cap: no bonus
        }
    }

    /// Score based on price momentum (5m, 1h changes).
    fn score_momentum(&self, m5: f64, h1: f64, h24: f64) -> f64 {
        let mut score = 0.0;

        // Steady 5m rise (0-15%): +3 pts per %
        if m5 > 0.0 && m5 < 15.0 {
            score += m5 * 3.0;
        }

        // Hourly momentum (0-30%): +2 pts per %
        if h1 > 0.0 && h1 < 30.0 {
            score += h1 * 2.0;
        }

        // Acceleration bonus: if 1h > 5% AND 5m > 0
        if h1 > 5.0 && m5 > 0.0 {
            score += 15.0;
        }

        // Additional bonus for sustained momentum
        if h24 > 0.0 && h1 > 0.0 && m5 > 0.0 {
            score += 5.0;
        }

        score.min(45.0)
    }

    /// Score based on volume/liquidity ratio.
    fn score_volume_liquidity(&self, ratio: f64) -> f64 {
        let mut score = 0.0_f64;

        // High ratio (>2): +20 pts
        if ratio > 2.0 {
            score += 20.0;
        }

        // Very high ratio (>5): +15 pts additional
        if ratio > 5.0 {
            score += 15.0;
        }

        score.min(35.0_f64)
    }

    /// Score based on buy/sell ratio.
    fn score_buy_pressure(&self, buy_sell_ratio: f64) -> f64 {
        let mut score = 0.0_f64;

        // Buy/sell ratio > 1.3: +15 pts
        if buy_sell_ratio > 1.3 {
            score += 15.0;
        }

        // Buy/sell ratio >= 2.0: +10 pts additional
        if buy_sell_ratio >= 2.0 {
            score += 10.0;
        }

        score.min(25.0_f64)
    }

    /// Calculate penalties (returns negative values).
    fn calculate_penalties(
        &self,
        m5: f64,
        h1: f64,
        h24: f64,
        liquidity: f64,
        buy_sell_ratio: f64,
    ) -> f64 {
        let mut penalties = 0.0;

        // Already pumped (5m > 30%): -25 pts
        if m5 > 30.0 {
            penalties -= 25.0;
        }

        // Dumping (1h < -15%): -20 pts
        if h1 < -15.0 {
            penalties -= 20.0;
        }

        // Dead token (24h < -40%): -20 pts
        if h24 < -40.0 {
            penalties -= 20.0;
        }

        // Low liquidity (< $15k): -10 pts
        if liquidity < 15_000.0 {
            penalties -= 10.0;
        }

        // Sell pressure (buy/sell < 0.5): -15 pts
        if buy_sell_ratio < 0.5 {
            penalties -= 15.0;
        }

        penalties
    }
}

/// Detailed score breakdown.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScoreDetails {
    /// FDV bonus points.
    pub fdv_bonus: f64,
    /// Momentum score points.
    pub momentum_score: f64,
    /// Volume/liquidity ratio points.
    pub volume_liq_score: f64,
    /// Buy pressure points.
    pub buy_pressure_score: f64,
    /// Penalty points (negative).
    pub penalties: f64,
}

/// Momentum score breakdown.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MomentumScore {
    /// Total momentum score (0-100).
    pub total: f64,
    /// FDV component score.
    pub fdv: f64,
    /// Momentum component score.
    pub momentum: f64,
    /// Volume/liquidity component score.
    pub volume_liq: f64,
    /// Buy pressure component score.
    pub buy_pressure: f64,
    /// Penalty points (negative).
    pub penalties: f64,
    /// Detailed breakdown.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<ScoreDetails>,
}

impl Default for MomentumScore {
    fn default() -> Self {
        Self {
            total: 0.0,
            fdv: 0.0,
            momentum: 0.0,
            volume_liq: 0.0,
            buy_pressure: 0.0,
            penalties: 0.0,
            details: Some(ScoreDetails::default()),
        }
    }
}

impl MomentumScore {
    /// Check if score meets minimum threshold.
    pub fn meets_threshold(&self, min_score: f64) -> bool {
        self.total >= min_score
    }

    /// Get a qualitative rating.
    pub fn rating(&self) -> &'static str {
        if self.total >= 80.0 {
            "Excellent"
        } else if self.total >= 65.0 {
            "Good"
        } else if self.total >= 50.0 {
            "Fair"
        } else if self.total >= 35.0 {
            "Weak"
        } else {
            "Poor"
        }
    }
}

/// Scored opportunity ready for trading consideration.
#[derive(Debug, Clone)]
pub struct ScoredOpportunity {
    /// The underlying opportunity.
    pub opportunity: TokenOpportunity,
    /// Momentum score.
    pub score: MomentumScore,
    /// Risk assessment.
    pub risk_level: RiskLevel,
}

impl ScoredOpportunity {
    /// Create a scored opportunity.
    pub fn new(opportunity: TokenOpportunity, scorer: &MomentumScorer) -> Self {
        let score = scorer.score(&opportunity);
        let risk_level = Self::assess_risk(&opportunity, &score);
        Self {
            opportunity,
            score,
            risk_level,
        }
    }

    fn assess_risk(opportunity: &TokenOpportunity, score: &MomentumScore) -> RiskLevel {
        let mut risk_score = 0;

        // Low liquidity = higher risk
        if opportunity.liquidity_usd < 10_000.0 {
            risk_score += 3;
        } else if opportunity.liquidity_usd < 25_000.0 {
            risk_score += 2;
        } else if opportunity.liquidity_usd < 50_000.0 {
            risk_score += 1;
        }

        // Very new tokens = higher risk
        if let Some(age) = opportunity.age_hours {
            if age < 1.0 {
                risk_score += 3;
            } else if age < 6.0 {
                risk_score += 2;
            } else if age < 24.0 {
                risk_score += 1;
            }
        }

        // Extreme price changes = higher risk
        if opportunity.price_change_24h > 200.0 || opportunity.price_change_24h < -30.0 {
            risk_score += 2;
        } else if opportunity.price_change_24h > 100.0 || opportunity.price_change_24h < -15.0 {
            risk_score += 1;
        }

        // Low momentum score = higher risk
        if score.total < 40.0 {
            risk_score += 2;
        } else if score.total < 55.0 {
            risk_score += 1;
        }

        // Heavy sell pressure = higher risk
        if opportunity.buy_sell_ratio() < 0.7 {
            risk_score += 2;
        }

        // Already pumped = higher risk
        if opportunity.price_change_5m > 30.0 {
            risk_score += 2;
        }

        match risk_score {
            0..=2 => RiskLevel::Low,
            3..=5 => RiskLevel::Medium,
            6..=8 => RiskLevel::High,
            _ => RiskLevel::Extreme,
        }
    }
}

/// Risk level assessment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Extreme,
}

impl std::fmt::Display for RiskLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Low => write!(f, "Low"),
            Self::Medium => write!(f, "Medium"),
            Self::High => write!(f, "High"),
            Self::Extreme => write!(f, "Extreme"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discovery::OpportunitySource;

    fn create_test_opportunity(
        fdv: Option<f64>,
        m5: f64,
        h1: f64,
        h24: f64,
        volume: f64,
        liquidity: f64,
        buys: i32,
        sells: i32,
        age_hours: Option<f64>,
    ) -> TokenOpportunity {
        TokenOpportunity {
            address: "test".to_string(),
            symbol: "TEST".to_string(),
            name: "Test Token".to_string(),
            price_usd: 1.0,
            volume_24h: volume,
            liquidity_usd: liquidity,
            price_change_24h: h24,
            price_change_5m: m5,
            price_change_1h: h1,
            price_change_6h: h24 / 4.0,
            created_at: None,
            age_hours,
            source: OpportunitySource::DexScreener,
            market_cap: None,
            fdv,
            dex: "raydium".to_string(),
            pair_address: "pair".to_string(),
            url: None,
            buys_24h: buys,
            sells_24h: sells,
        }
    }

    #[test]
    fn test_fdv_scoring() {
        let scorer = MomentumScorer::default();

        // Micro cap should get 30 pts
        let micro = create_test_opportunity(
            Some(200_000.0),
            0.0,
            0.0,
            0.0,
            50_000.0,
            50_000.0,
            100,
            100,
            Some(24.0),
        );
        let score = scorer.score(&micro);
        assert_eq!(score.fdv, 30.0);

        // Small cap should get 20 pts
        let small = create_test_opportunity(
            Some(1_000_000.0),
            0.0,
            0.0,
            0.0,
            50_000.0,
            50_000.0,
            100,
            100,
            Some(24.0),
        );
        let score = scorer.score(&small);
        assert_eq!(score.fdv, 20.0);

        // Mid cap should get 10 pts
        let mid = create_test_opportunity(
            Some(5_000_000.0),
            0.0,
            0.0,
            0.0,
            50_000.0,
            50_000.0,
            100,
            100,
            Some(24.0),
        );
        let score = scorer.score(&mid);
        assert_eq!(score.fdv, 10.0);

        // Large cap should get 0 pts
        let large = create_test_opportunity(
            Some(50_000_000.0),
            0.0,
            0.0,
            0.0,
            50_000.0,
            50_000.0,
            100,
            100,
            Some(24.0),
        );
        let score = scorer.score(&large);
        assert_eq!(score.fdv, 0.0);
    }

    #[test]
    fn test_momentum_scoring() {
        let scorer = MomentumScorer::default();

        // High momentum opportunity: 5m=5%, 1h=10%
        let high = create_test_opportunity(
            Some(500_000.0),
            5.0,
            10.0,
            20.0,
            100_000.0,
            50_000.0,
            150,
            100,
            Some(6.0),
        );
        let score = scorer.score(&high);
        // 5m: 5*3=15, 1h: 10*2=20, acceleration: 15, sustained: 5 = 55, capped at 45
        assert!(score.momentum >= 35.0);

        // Low momentum: negative changes
        let low = create_test_opportunity(
            Some(500_000.0),
            -5.0,
            -10.0,
            -20.0,
            50_000.0,
            50_000.0,
            100,
            100,
            Some(24.0),
        );
        let score = scorer.score(&low);
        assert_eq!(score.momentum, 0.0);
    }

    #[test]
    fn test_buy_pressure_scoring() {
        let scorer = MomentumScorer::default();

        // Strong buy pressure: 200 buys, 100 sells = 2.0 ratio
        let strong = create_test_opportunity(
            None,
            5.0,
            10.0,
            15.0,
            100_000.0,
            50_000.0,
            200,
            100,
            Some(24.0),
        );
        let score = scorer.score(&strong);
        assert_eq!(score.buy_pressure, 25.0); // 15 + 10 = 25

        // Moderate buy pressure: 140 buys, 100 sells = 1.4 ratio
        let moderate = create_test_opportunity(
            None,
            5.0,
            10.0,
            15.0,
            100_000.0,
            50_000.0,
            140,
            100,
            Some(24.0),
        );
        let score = scorer.score(&moderate);
        assert_eq!(score.buy_pressure, 15.0); // 15 only

        // Neutral: 100 buys, 100 sells = 1.0 ratio
        let neutral = create_test_opportunity(
            None,
            5.0,
            10.0,
            15.0,
            100_000.0,
            50_000.0,
            100,
            100,
            Some(24.0),
        );
        let score = scorer.score(&neutral);
        assert_eq!(score.buy_pressure, 0.0);
    }

    #[test]
    fn test_penalties() {
        let scorer = MomentumScorer::default();

        // Already pumped (5m > 30%)
        let pumped = create_test_opportunity(
            None,
            35.0,
            10.0,
            50.0,
            100_000.0,
            50_000.0,
            100,
            100,
            Some(24.0),
        );
        let score = scorer.score(&pumped);
        assert!(score.penalties <= -25.0);

        // Dumping (1h < -15%)
        let dumping = create_test_opportunity(
            None,
            0.0,
            -20.0,
            -30.0,
            100_000.0,
            50_000.0,
            100,
            100,
            Some(24.0),
        );
        let score = scorer.score(&dumping);
        assert!(score.penalties <= -20.0);

        // Low liquidity
        let low_liq = create_test_opportunity(
            None,
            5.0,
            10.0,
            15.0,
            50_000.0,
            10_000.0,
            100,
            100,
            Some(24.0),
        );
        let score = scorer.score(&low_liq);
        assert!(score.penalties <= -10.0);
    }

    #[test]
    fn test_volume_liquidity_ratio() {
        let scorer = MomentumScorer::default();

        // Very high ratio (>5): 300k volume, 50k liquidity = 6.0
        let high = create_test_opportunity(
            None,
            5.0,
            10.0,
            15.0,
            300_000.0,
            50_000.0,
            100,
            100,
            Some(24.0),
        );
        let score = scorer.score(&high);
        assert_eq!(score.volume_liq, 35.0); // 20 + 15

        // High ratio (>2): 150k volume, 50k liquidity = 3.0
        let medium = create_test_opportunity(
            None,
            5.0,
            10.0,
            15.0,
            150_000.0,
            50_000.0,
            100,
            100,
            Some(24.0),
        );
        let score = scorer.score(&medium);
        assert_eq!(score.volume_liq, 20.0);
    }

    #[test]
    fn test_risk_assessment() {
        let scorer = MomentumScorer::default();

        // Low risk: good liquidity, reasonable age, stable price
        let low_risk = create_test_opportunity(
            Some(5_000_000.0),
            5.0,
            10.0,
            15.0,
            100_000.0,
            100_000.0,
            150,
            100,
            Some(48.0),
        );
        let scored = ScoredOpportunity::new(low_risk, &scorer);
        assert!(matches!(
            scored.risk_level,
            RiskLevel::Low | RiskLevel::Medium
        ));

        // High risk: low liquidity, very new, extreme price change, heavy selling
        let high_risk = create_test_opportunity(
            Some(200_000.0),
            35.0,
            -20.0,
            250.0,
            50_000.0,
            5_000.0,
            50,
            200,
            Some(0.5),
        );
        let scored_high = ScoredOpportunity::new(high_risk, &scorer);
        assert!(matches!(
            scored_high.risk_level,
            RiskLevel::High | RiskLevel::Extreme
        ));
    }

    #[test]
    fn test_full_score_calculation() {
        let scorer = MomentumScorer::default();

        // Optimal opportunity: micro cap, good momentum, high vol/liq, strong buy pressure
        let optimal = create_test_opportunity(
            Some(300_000.0), // Micro cap: +30
            8.0,             // 5m +8%
            15.0,            // 1h +15%
            25.0,            // 24h +25%
            500_000.0,       // High volume
            80_000.0,        // Good liquidity (ratio > 6)
            250,             // Strong buys
            100,             // vs sells (2.5 ratio)
            Some(12.0),
        );

        let score = scorer.score(&optimal);
        println!("Optimal score: {:?}", score);
        assert!(
            score.total >= 70.0,
            "Expected high score, got {}",
            score.total
        );
    }
}
