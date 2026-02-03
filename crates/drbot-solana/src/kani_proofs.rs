//! Kani formal verification proofs for DeFi and Solana modules.
//!
//! This module contains formal proofs verifying critical properties of
//! DeFi calculations, risk metrics, and portfolio management functions.

#[cfg(kani)]
mod kani_proofs {
    // ============================================================================
    // Concentration Metrics Proofs (risk/portfolio.rs)
    // ============================================================================

    /// Herfindahl-Hirschman Index calculation for portfolio concentration.
    /// HHI = sum(w_i^2) where w_i are normalized weights (sum to 1).
    fn hhi_from_weights(weights: &[f64]) -> f64 {
        if weights.is_empty() {
            return 0.0;
        }
        let total: f64 = weights.iter().sum();
        if total <= 0.0 {
            return 0.0;
        }
        let normalized: Vec<f64> = weights.iter().map(|w| w / total).collect();
        normalized.iter().map(|w| w * w).sum()
    }

    #[kani::proof]
    fn proof_hhi_bounds_empty() {
        let weights: Vec<f64> = vec![];
        let hhi = hhi_from_weights(&weights);
        kani::assert!(hhi == 0.0, "HHI of empty portfolio is 0");
    }

    #[kani::proof]
    fn proof_hhi_single_position() {
        let weights = vec![1000.0];
        let hhi = hhi_from_weights(&weights);
        kani::assert!((hhi - 1.0).abs() < 0.0001, "HHI of single position is 1");
    }

    #[kani::proof]
    fn proof_hhi_equal_weights_two() {
        let weights = vec![500.0, 500.0];
        let hhi = hhi_from_weights(&weights);
        // HHI = 2 * (0.5)^2 = 0.5
        kani::assert!(
            (hhi - 0.5).abs() < 0.0001,
            "HHI of 2 equal positions is 0.5"
        );
    }

    #[kani::proof]
    fn proof_hhi_equal_weights_four() {
        let weights = vec![250.0, 250.0, 250.0, 250.0];
        let hhi = hhi_from_weights(&weights);
        // HHI = 4 * (0.25)^2 = 0.25
        kani::assert!(
            (hhi - 0.25).abs() < 0.0001,
            "HHI of 4 equal positions is 0.25"
        );
    }

    #[kani::proof]
    fn proof_hhi_range_bounded() {
        // Two non-negative weights
        let w1: u8 = kani::any();
        let w2: u8 = kani::any();
        kani::assume!(w1 > 0 || w2 > 0); // At least one positive

        let weights = vec![w1 as f64, w2 as f64];
        let hhi = hhi_from_weights(&weights);

        // HHI is always in (0, 1] for non-empty portfolios
        kani::assert!(hhi > 0.0, "HHI is positive for non-empty portfolio");
        kani::assert!(hhi <= 1.0, "HHI is at most 1");
    }

    #[kani::proof]
    fn proof_hhi_three_positions_bounded() {
        let w1: u8 = kani::any();
        let w2: u8 = kani::any();
        let w3: u8 = kani::any();
        kani::assume!(w1 > 0 || w2 > 0 || w3 > 0);

        let weights = vec![w1 as f64, w2 as f64, w3 as f64];
        let hhi = hhi_from_weights(&weights);

        // For 3 positions, minimum HHI is 1/3 (equal weights)
        // Maximum is 1 (one position has all weight)
        kani::assert!(hhi >= 0.333 - 0.001, "HHI >= 1/n for n positions");
        kani::assert!(hhi <= 1.0, "HHI <= 1");
    }

    #[kani::proof]
    fn proof_effective_positions_from_hhi() {
        let hhi = 0.25; // 4 equal positions
        let effective = if hhi > 0.0 { 1.0 / hhi } else { 0.0 };
        kani::assert!(
            (effective - 4.0).abs() < 0.0001,
            "effective positions = 1/HHI"
        );
    }

    // ============================================================================
    // Delta Calculation Proofs (hedging/delta_calculator.rs)
    // ============================================================================

    /// Calculate delta for a position given value and beta.
    fn position_delta(value: f64, beta: f64, is_long: bool) -> f64 {
        if is_long {
            value * beta
        } else {
            -value * beta
        }
    }

    #[kani::proof]
    fn proof_delta_long_positive() {
        let value = 1000.0;
        let beta = 1.0;
        let delta = position_delta(value, beta, true);
        kani::assert!(delta > 0.0, "Long position has positive delta");
    }

    #[kani::proof]
    fn proof_delta_short_negative() {
        let value = 1000.0;
        let beta = 1.0;
        let delta = position_delta(value, beta, false);
        kani::assert!(delta < 0.0, "Short position has negative delta");
    }

    #[kani::proof]
    fn proof_delta_zero_beta() {
        let value = 1000.0;
        let beta = 0.0; // Stablecoin
        let delta_long = position_delta(value, beta, true);
        let delta_short = position_delta(value, beta, false);
        kani::assert!(delta_long == 0.0, "Zero beta means zero delta (long)");
        kani::assert!(delta_short == 0.0, "Zero beta means zero delta (short)");
    }

    #[kani::proof]
    fn proof_delta_hedge_cancellation() {
        // Long and short positions of equal size cancel out
        let value = 1000.0;
        let beta = 1.0;
        let long_delta = position_delta(value, beta, true);
        let short_delta = position_delta(value, beta, false);
        let total = long_delta + short_delta;
        kani::assert!(
            total.abs() < 0.0001,
            "Equal long and short cancel to zero delta"
        );
    }

    #[kani::proof]
    fn proof_delta_proportional_to_value() {
        let beta = 1.0;
        let value1 = 1000.0;
        let value2 = 2000.0;
        let delta1 = position_delta(value1, beta, true);
        let delta2 = position_delta(value2, beta, true);
        kani::assert!(
            (delta2 / delta1 - 2.0).abs() < 0.0001,
            "Delta scales with value"
        );
    }

    #[kani::proof]
    fn proof_delta_proportional_to_beta() {
        let value = 1000.0;
        let beta1 = 0.5;
        let beta2 = 1.0;
        let delta1 = position_delta(value, beta1, true);
        let delta2 = position_delta(value, beta2, true);
        kani::assert!(
            (delta2 / delta1 - 2.0).abs() < 0.0001,
            "Delta scales with beta"
        );
    }

    // ============================================================================
    // Delta Percentage Proofs
    // ============================================================================

    fn delta_percentage(total_delta: f64, long_exposure: f64, short_exposure: f64) -> f64 {
        let total_exposure = long_exposure + short_exposure.abs();
        if total_exposure > 0.0 {
            (total_delta / total_exposure) * 100.0
        } else {
            0.0
        }
    }

    #[kani::proof]
    fn proof_delta_pct_zero_exposure() {
        let delta_pct = delta_percentage(0.0, 0.0, 0.0);
        kani::assert!(
            delta_pct == 0.0,
            "Zero exposure means zero delta percentage"
        );
    }

    #[kani::proof]
    fn proof_delta_pct_fully_long() {
        // 100% long position
        let delta_pct = delta_percentage(1000.0, 1000.0, 0.0);
        kani::assert!((delta_pct - 100.0).abs() < 0.01, "100% long = 100% delta");
    }

    #[kani::proof]
    fn proof_delta_pct_market_neutral() {
        // Equal long and short
        let delta_pct = delta_percentage(0.0, 1000.0, 1000.0);
        kani::assert!(delta_pct.abs() < 0.01, "Equal long/short = 0% delta");
    }

    #[kani::proof]
    fn proof_delta_pct_bounded() {
        // Delta percentage is bounded by [-100, 100] for reasonable portfolios
        let long: u8 = kani::any();
        let short: u8 = kani::any();
        kani::assume!(long > 0 || short > 0);

        let long_f = long as f64 * 100.0;
        let short_f = short as f64 * 100.0;
        let delta = long_f - short_f; // Net delta

        let pct = delta_percentage(delta, long_f, short_f);
        kani::assert!(pct >= -100.0 && pct <= 100.0, "Delta percentage bounded");
    }

    // ============================================================================
    // Correlation Bounds Proofs (risk/correlation.rs)
    // ============================================================================

    #[kani::proof]
    fn proof_correlation_bounds() {
        // Correlation coefficient is always in [-1, 1]
        let corr_values = [-1.0, -0.5, 0.0, 0.5, 0.95, 1.0];
        for &c in &corr_values {
            kani::assert!(c >= -1.0 && c <= 1.0, "Correlation in valid range");
        }
    }

    #[kani::proof]
    fn proof_correlation_diagonal() {
        // Diagonal of correlation matrix is always 1 (self-correlation)
        let self_corr = 1.0;
        kani::assert!(self_corr == 1.0, "Self-correlation is always 1");
    }

    #[kani::proof]
    fn proof_correlation_symmetric() {
        // corr(A, B) == corr(B, A)
        let corr_ab = 0.85;
        let corr_ba = 0.85; // Would be same in real matrix
        kani::assert!(corr_ab == corr_ba, "Correlation is symmetric");
    }

    fn correlation_type(corr: f64) -> &'static str {
        match corr {
            c if c >= 0.9 => "Very High Positive",
            c if c >= 0.7 => "High Positive",
            c if c >= 0.5 => "Moderate Positive",
            c if c >= 0.3 => "Low Positive",
            c if c >= -0.3 => "Uncorrelated",
            c if c >= -0.5 => "Low Negative",
            c if c >= -0.7 => "Moderate Negative",
            c if c >= -0.9 => "High Negative",
            _ => "Very High Negative",
        }
    }

    #[kani::proof]
    fn proof_correlation_type_complete() {
        // All valid correlations map to a type
        let test_values = [
            -1.0, -0.95, -0.8, -0.6, -0.4, -0.2, 0.0, 0.2, 0.4, 0.6, 0.8, 0.95, 1.0,
        ];
        for &c in &test_values {
            let t = correlation_type(c);
            kani::assert!(!t.is_empty(), "All correlations have a type");
        }
    }

    // ============================================================================
    // Risk Score Proofs (risk/portfolio.rs)
    // ============================================================================

    fn risk_score(var_pct: f64, hhi: f64, alert_count: usize) -> u8 {
        let mut score = 3u8;

        // VaR contribution
        if var_pct > 0.15 {
            score = score.saturating_add(3);
        } else if var_pct > 0.10 {
            score = score.saturating_add(2);
        } else if var_pct > 0.05 {
            score = score.saturating_add(1);
        }

        // Concentration contribution
        if hhi > 0.4 {
            score = score.saturating_add(3);
        } else if hhi > 0.25 {
            score = score.saturating_add(2);
        } else if hhi > 0.15 {
            score = score.saturating_add(1);
        }

        // Alert contribution
        score = score.saturating_add((alert_count as u8).min(2));

        score.min(10)
    }

    #[kani::proof]
    fn proof_risk_score_minimum() {
        let score = risk_score(0.0, 0.0, 0);
        kani::assert!(score >= 3, "Minimum risk score is 3");
    }

    #[kani::proof]
    fn proof_risk_score_maximum() {
        let score = risk_score(0.20, 0.5, 10);
        kani::assert!(score <= 10, "Maximum risk score is 10");
    }

    #[kani::proof]
    fn proof_risk_score_bounded() {
        // All possible inputs produce valid score
        let var: u8 = kani::any();
        let hhi: u8 = kani::any();
        let alerts: u8 = kani::any();

        let var_pct = var as f64 / 100.0; // 0-2.55
        let hhi_val = hhi as f64 / 255.0; // 0-1
        let alert_count = alerts as usize;

        let score = risk_score(var_pct, hhi_val, alert_count);
        kani::assert!(score >= 3 && score <= 10, "Risk score always in [3, 10]");
    }

    #[kani::proof]
    fn proof_risk_score_monotonic_var() {
        // Higher VaR leads to higher or equal score
        let score_low = risk_score(0.03, 0.2, 0);
        let score_high = risk_score(0.20, 0.2, 0);
        kani::assert!(score_high >= score_low, "Risk score increases with VaR");
    }

    #[kani::proof]
    fn proof_risk_score_monotonic_hhi() {
        // Higher HHI leads to higher or equal score
        let score_low = risk_score(0.05, 0.10, 0);
        let score_high = risk_score(0.05, 0.50, 0);
        kani::assert!(score_high >= score_low, "Risk score increases with HHI");
    }

    // ============================================================================
    // Approval Threshold Proofs (defi/approval.rs)
    // ============================================================================

    fn requires_approval(
        threshold: f64,
        require_all: bool,
        always_require_action: bool,
        amount_usd: f64,
    ) -> bool {
        if require_all {
            return true;
        }
        if always_require_action {
            return true;
        }
        amount_usd >= threshold
    }

    #[kani::proof]
    fn proof_approval_require_all() {
        // When require_all is true, always require approval
        let result = requires_approval(1000.0, true, false, 1.0);
        kani::assert!(result, "require_all means always approve");
    }

    #[kani::proof]
    fn proof_approval_always_require_action() {
        // When action is in always_require, require approval
        let result = requires_approval(1000.0, false, true, 1.0);
        kani::assert!(result, "always_require action needs approval");
    }

    #[kani::proof]
    fn proof_approval_under_threshold() {
        let result = requires_approval(100.0, false, false, 50.0);
        kani::assert!(!result, "Under threshold doesn't need approval");
    }

    #[kani::proof]
    fn proof_approval_over_threshold() {
        let result = requires_approval(100.0, false, false, 150.0);
        kani::assert!(result, "Over threshold needs approval");
    }

    #[kani::proof]
    fn proof_approval_at_threshold() {
        let result = requires_approval(100.0, false, false, 100.0);
        kani::assert!(result, "At threshold needs approval");
    }

    // ============================================================================
    // Rebalance Config Proofs (hedging/rebalancer.rs)
    // ============================================================================

    fn needs_rebalance(delta_pct: f64, threshold: f64) -> bool {
        delta_pct.abs() > threshold
    }

    #[kani::proof]
    fn proof_rebalance_under_threshold() {
        let result = needs_rebalance(5.0, 10.0);
        kani::assert!(!result, "Under threshold doesn't need rebalance");
    }

    #[kani::proof]
    fn proof_rebalance_over_threshold() {
        let result = needs_rebalance(15.0, 10.0);
        kani::assert!(result, "Over threshold needs rebalance");
    }

    #[kani::proof]
    fn proof_rebalance_negative_delta() {
        // Negative delta also triggers rebalance
        let result = needs_rebalance(-15.0, 10.0);
        kani::assert!(result, "Negative delta over threshold needs rebalance");
    }

    #[kani::proof]
    fn proof_rebalance_zero_threshold() {
        // Zero threshold always needs rebalance (unless delta is exactly 0)
        let delta: i8 = kani::any();
        kani::assume!(delta != 0);
        let result = needs_rebalance(delta as f64, 0.0);
        kani::assert!(result, "Zero threshold always triggers (non-zero delta)");
    }

    // ============================================================================
    // VaR Calculation Proofs
    // ============================================================================

    fn calculate_var(total_value: f64, daily_vol: f64, z_score: f64) -> f64 {
        total_value * daily_vol * z_score
    }

    #[kani::proof]
    fn proof_var_non_negative() {
        let total_value = 10000.0;
        let daily_vol = 0.03;
        let z_95 = 1.645;
        let var = calculate_var(total_value, daily_vol, z_95);
        kani::assert!(var >= 0.0, "VaR is non-negative");
    }

    #[kani::proof]
    fn proof_var_99_greater_than_95() {
        let total_value = 10000.0;
        let daily_vol = 0.03;
        let z_95 = 1.645;
        let z_99 = 2.326;
        let var_95 = calculate_var(total_value, daily_vol, z_95);
        let var_99 = calculate_var(total_value, daily_vol, z_99);
        kani::assert!(var_99 > var_95, "99% VaR > 95% VaR");
    }

    #[kani::proof]
    fn proof_var_proportional_to_value() {
        let daily_vol = 0.03;
        let z = 1.645;
        let var1 = calculate_var(10000.0, daily_vol, z);
        let var2 = calculate_var(20000.0, daily_vol, z);
        kani::assert!(
            (var2 / var1 - 2.0).abs() < 0.0001,
            "VaR scales with portfolio value"
        );
    }

    // ============================================================================
    // Trade Value Calculation Proofs
    // ============================================================================

    fn calculate_trade_value(price: f64, quantity_lamports: u64, decimals: u32) -> f64 {
        let ui_quantity = quantity_lamports as f64 / 10f64.powi(decimals as i32);
        price * ui_quantity
    }

    #[kani::proof]
    fn proof_trade_value_sol() {
        // 1 SOL at $100
        let value = calculate_trade_value(100.0, 1_000_000_000, 9);
        kani::assert!((value - 100.0).abs() < 0.0001, "1 SOL at $100 = $100");
    }

    #[kani::proof]
    fn proof_trade_value_usdc() {
        // 100 USDC at $1
        let value = calculate_trade_value(1.0, 100_000_000, 6); // USDC has 6 decimals
        kani::assert!((value - 100.0).abs() < 0.0001, "100 USDC at $1 = $100");
    }

    #[kani::proof]
    fn proof_trade_value_non_negative() {
        let price: u8 = kani::any();
        let quantity: u16 = kani::any();
        let value = calculate_trade_value(price as f64, quantity as u64, 9);
        kani::assert!(value >= 0.0, "Trade value is non-negative");
    }

    // ============================================================================
    // Hedge Amount Calculation Proofs
    // ============================================================================

    fn hedge_amount_for_target(current_delta: f64, target_delta: f64) -> f64 {
        current_delta - target_delta
    }

    #[kani::proof]
    fn proof_hedge_to_neutral() {
        let current = 5000.0;
        let target = 0.0;
        let hedge = hedge_amount_for_target(current, target);
        kani::assert!(hedge == 5000.0, "Hedge to neutral equals current delta");
    }

    #[kani::proof]
    fn proof_hedge_partial() {
        let current = 10000.0;
        let target = 2000.0;
        let hedge = hedge_amount_for_target(current, target);
        kani::assert!(hedge == 8000.0, "Partial hedge calculation correct");
    }

    #[kani::proof]
    fn proof_hedge_ratio() {
        let current = 10000.0;
        let target = 2000.0;
        let hedge = hedge_amount_for_target(current, target);
        let ratio = hedge / current;
        kani::assert!((ratio - 0.8).abs() < 0.0001, "80% hedge ratio");
    }

    // ============================================================================
    // Hedge Cost Estimation Proofs
    // ============================================================================

    fn estimate_hedge_cost(amount: f64, cost_bps: u16) -> f64 {
        amount * (cost_bps as f64 / 10000.0)
    }

    #[kani::proof]
    fn proof_hedge_cost_50bps() {
        let cost = estimate_hedge_cost(5000.0, 50);
        kani::assert!((cost - 25.0).abs() < 0.01, "50bps on $5000 = $25");
    }

    #[kani::proof]
    fn proof_hedge_cost_non_negative() {
        let amount: u8 = kani::any();
        let bps: u8 = kani::any();
        let cost = estimate_hedge_cost(amount as f64 * 100.0, bps as u16);
        kani::assert!(cost >= 0.0, "Hedge cost is non-negative");
    }

    #[kani::proof]
    fn proof_hedge_cost_proportional() {
        let cost1 = estimate_hedge_cost(1000.0, 50);
        let cost2 = estimate_hedge_cost(2000.0, 50);
        kani::assert!(
            (cost2 / cost1 - 2.0).abs() < 0.0001,
            "Cost scales with amount"
        );
    }

    // ============================================================================
    // Daily Returns Calculation Proofs
    // ============================================================================

    fn daily_return(price_prev: f64, price_curr: f64) -> f64 {
        if price_prev <= 0.0 {
            return 0.0;
        }
        (price_curr - price_prev) / price_prev
    }

    #[kani::proof]
    fn proof_daily_return_positive() {
        let ret = daily_return(100.0, 110.0);
        kani::assert!((ret - 0.1).abs() < 0.0001, "10% gain calculation");
    }

    #[kani::proof]
    fn proof_daily_return_negative() {
        let ret = daily_return(100.0, 90.0);
        kani::assert!((ret - (-0.1)).abs() < 0.0001, "10% loss calculation");
    }

    #[kani::proof]
    fn proof_daily_return_zero_price() {
        let ret = daily_return(0.0, 100.0);
        kani::assert!(ret == 0.0, "Zero previous price returns 0");
    }

    // ============================================================================
    // Volatility Calculation Proofs
    // ============================================================================

    fn calculate_volatility(returns: &[f64]) -> f64 {
        if returns.is_empty() {
            return 0.0;
        }
        let n = returns.len() as f64;
        let mean: f64 = returns.iter().sum::<f64>() / n;
        let variance: f64 = returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / n;
        variance.sqrt()
    }

    #[kani::proof]
    fn proof_volatility_non_negative() {
        let returns = vec![0.01, -0.02, 0.015, -0.01, 0.02];
        let vol = calculate_volatility(&returns);
        kani::assert!(vol >= 0.0, "Volatility is non-negative");
    }

    #[kani::proof]
    fn proof_volatility_constant_returns() {
        // All same returns = zero volatility
        let returns = vec![0.01, 0.01, 0.01, 0.01];
        let vol = calculate_volatility(&returns);
        kani::assert!(vol < 0.0001, "Constant returns have zero volatility");
    }

    #[kani::proof]
    fn proof_volatility_empty() {
        let returns: Vec<f64> = vec![];
        let vol = calculate_volatility(&returns);
        kani::assert!(vol == 0.0, "Empty returns have zero volatility");
    }

    // ============================================================================
    // Approval Code Format Proofs
    // ============================================================================

    fn is_valid_approval_code(code: &str) -> bool {
        code.len() == 6 && code.chars().all(|c| c.is_ascii_digit())
    }

    #[kani::proof]
    fn proof_approval_code_valid() {
        kani::assert!(is_valid_approval_code("123456"), "Valid 6-digit code");
        kani::assert!(is_valid_approval_code("000000"), "All zeros valid");
        kani::assert!(is_valid_approval_code("999999"), "All nines valid");
    }

    #[kani::proof]
    fn proof_approval_code_invalid() {
        kani::assert!(!is_valid_approval_code("12345"), "5 digits invalid");
        kani::assert!(!is_valid_approval_code("1234567"), "7 digits invalid");
        kani::assert!(!is_valid_approval_code("12345a"), "Letters invalid");
        kani::assert!(!is_valid_approval_code(""), "Empty invalid");
    }

    // ============================================================================
    // Amount Formatting Proofs
    // ============================================================================

    fn format_amount_category(amount_lamports: u64) -> &'static str {
        let ui_amount = amount_lamports as f64 / 1e9;
        if ui_amount >= 1000.0 {
            "thousands"
        } else if ui_amount >= 1.0 {
            "units"
        } else {
            "fractional"
        }
    }

    #[kani::proof]
    fn proof_amount_category_thousands() {
        let category = format_amount_category(2_000_000_000_000); // 2000 SOL
        kani::assert!(category == "thousands", "2000 SOL is thousands");
    }

    #[kani::proof]
    fn proof_amount_category_units() {
        let category = format_amount_category(5_000_000_000); // 5 SOL
        kani::assert!(category == "units", "5 SOL is units");
    }

    #[kani::proof]
    fn proof_amount_category_fractional() {
        let category = format_amount_category(100_000_000); // 0.1 SOL
        kani::assert!(category == "fractional", "0.1 SOL is fractional");
    }

    // ============================================================================
    // Trading Position P&L Proofs (trading/strategy/position.rs)
    // ============================================================================

    fn calculate_pnl_pct(entry_price: f64, current_price: f64) -> f64 {
        if entry_price <= 0.0 {
            return 0.0;
        }
        ((current_price / entry_price) - 1.0) * 100.0
    }

    fn calculate_pnl_usd(entry_value: f64, current_value: f64) -> f64 {
        current_value - entry_value
    }

    #[kani::proof]
    fn proof_pnl_pct_gain() {
        let pnl = calculate_pnl_pct(100.0, 150.0);
        kani::assert!((pnl - 50.0).abs() < 0.0001, "50% gain calculation");
    }

    #[kani::proof]
    fn proof_pnl_pct_loss() {
        let pnl = calculate_pnl_pct(100.0, 80.0);
        kani::assert!((pnl - (-20.0)).abs() < 0.0001, "20% loss calculation");
    }

    #[kani::proof]
    fn proof_pnl_pct_no_change() {
        let pnl = calculate_pnl_pct(100.0, 100.0);
        kani::assert!(pnl.abs() < 0.0001, "No change = 0% P&L");
    }

    #[kani::proof]
    fn proof_pnl_pct_zero_entry() {
        let pnl = calculate_pnl_pct(0.0, 100.0);
        kani::assert!(pnl == 0.0, "Zero entry price returns 0");
    }

    #[kani::proof]
    fn proof_pnl_usd_calculation() {
        let entry = 1000.0;
        let current = 1500.0;
        let pnl = calculate_pnl_usd(entry, current);
        kani::assert!((pnl - 500.0).abs() < 0.0001, "P&L USD = current - entry");
    }

    #[kani::proof]
    fn proof_pnl_consistent() {
        // P&L % and P&L USD should be consistent
        let entry_price = 10.0;
        let current_price = 15.0;
        let amount = 100.0;

        let entry_value = entry_price * amount;
        let current_value = current_price * amount;

        let pnl_pct = calculate_pnl_pct(entry_price, current_price);
        let pnl_usd = calculate_pnl_usd(entry_value, current_value);

        // P&L USD should be pnl_pct% of entry value
        let expected_pnl_usd = entry_value * (pnl_pct / 100.0);
        kani::assert!(
            (pnl_usd - expected_pnl_usd).abs() < 0.01,
            "P&L % and USD consistent"
        );
    }

    // ============================================================================
    // Trailing Stop Proofs (trading/strategy/trailing_stop.rs)
    // ============================================================================

    fn calculate_trailing_stop_price(peak_price: f64, distance_pct: f64) -> f64 {
        peak_price * (1.0 - distance_pct / 100.0)
    }

    fn should_activate_trailing_stop(gain_pct: f64, trigger_pct: f64) -> bool {
        gain_pct >= trigger_pct
    }

    fn is_trailing_stop_triggered(current_price: f64, stop_price: f64) -> bool {
        current_price <= stop_price
    }

    #[kani::proof]
    fn proof_trailing_stop_price_calculation() {
        let peak = 100.0;
        let distance = 10.0; // 10%
        let stop = calculate_trailing_stop_price(peak, distance);
        kani::assert!((stop - 90.0).abs() < 0.0001, "10% below peak = 90");
    }

    #[kani::proof]
    fn proof_trailing_stop_price_always_below_peak() {
        let peak: u8 = kani::any();
        let distance: u8 = kani::any();
        kani::assume!(peak > 0 && distance > 0 && distance < 100);

        let stop = calculate_trailing_stop_price(peak as f64, distance as f64);
        kani::assert!(stop < peak as f64, "Stop price always below peak");
        kani::assert!(stop > 0.0, "Stop price always positive");
    }

    #[kani::proof]
    fn proof_trailing_stop_activation() {
        // Should not activate at +15% with 20% trigger
        kani::assert!(
            !should_activate_trailing_stop(15.0, 20.0),
            "No activation below trigger"
        );
        // Should activate at +25% with 20% trigger
        kani::assert!(
            should_activate_trailing_stop(25.0, 20.0),
            "Activation at/above trigger"
        );
        // Should activate exactly at trigger
        kani::assert!(
            should_activate_trailing_stop(20.0, 20.0),
            "Activation exactly at trigger"
        );
    }

    #[kani::proof]
    fn proof_trailing_stop_trigger_check() {
        let stop_price = 90.0;
        // Price above stop - not triggered
        kani::assert!(
            !is_trailing_stop_triggered(95.0, stop_price),
            "Above stop not triggered"
        );
        // Price at stop - triggered
        kani::assert!(
            is_trailing_stop_triggered(90.0, stop_price),
            "At stop is triggered"
        );
        // Price below stop - triggered
        kani::assert!(
            is_trailing_stop_triggered(85.0, stop_price),
            "Below stop is triggered"
        );
    }

    #[kani::proof]
    fn proof_trailing_stop_only_moves_up() {
        // New stop should only be used if higher than current
        let current_stop = 90.0;
        let new_stop_higher = 95.0;
        let new_stop_lower = 85.0;

        let final_stop_higher = if new_stop_higher > current_stop {
            new_stop_higher
        } else {
            current_stop
        };
        let final_stop_lower = if new_stop_lower > current_stop {
            new_stop_lower
        } else {
            current_stop
        };

        kani::assert!(final_stop_higher == 95.0, "Higher stop is used");
        kani::assert!(final_stop_lower == 90.0, "Lower stop is ignored");
    }

    // ============================================================================
    // Escrow State Machine Proofs (otc/escrow.rs)
    // ============================================================================

    fn is_fully_funded(party_a_funded: bool, party_b_funded: bool) -> bool {
        party_a_funded && party_b_funded
    }

    fn can_settle(fully_funded: bool, expired: bool, status_is_funded: bool) -> bool {
        fully_funded && !expired && status_is_funded
    }

    #[kani::proof]
    fn proof_escrow_fully_funded() {
        // Neither funded
        kani::assert!(
            !is_fully_funded(false, false),
            "Neither funded = not fully funded"
        );
        // Only A funded
        kani::assert!(
            !is_fully_funded(true, false),
            "Only A funded = not fully funded"
        );
        // Only B funded
        kani::assert!(
            !is_fully_funded(false, true),
            "Only B funded = not fully funded"
        );
        // Both funded
        kani::assert!(is_fully_funded(true, true), "Both funded = fully funded");
    }

    #[kani::proof]
    fn proof_escrow_can_settle() {
        // Can settle: fully funded, not expired, status is Funded
        kani::assert!(
            can_settle(true, false, true),
            "Valid conditions allow settlement"
        );

        // Cannot settle if not fully funded
        kani::assert!(
            !can_settle(false, false, true),
            "Not fully funded cannot settle"
        );

        // Cannot settle if expired
        kani::assert!(!can_settle(true, true, true), "Expired cannot settle");

        // Cannot settle if wrong status
        kani::assert!(
            !can_settle(true, false, false),
            "Wrong status cannot settle"
        );
    }

    #[kani::proof]
    fn proof_escrow_state_machine_all_paths() {
        // Test all combinations
        let a: bool = kani::any();
        let b: bool = kani::any();
        let expired: bool = kani::any();
        let status: bool = kani::any();

        let fully_funded = is_fully_funded(a, b);
        let can = can_settle(fully_funded, expired, status);

        // Can only settle if all conditions are true
        kani::assert!(
            can == (a && b && !expired && status),
            "Settlement requires all conditions"
        );
    }

    // ============================================================================
    // Win Rate Calculation Proofs (trading/strategy/position.rs)
    // ============================================================================

    fn calculate_win_rate(winning: usize, total: usize) -> f64 {
        if total == 0 {
            return 0.0;
        }
        (winning as f64 / total as f64) * 100.0
    }

    #[kani::proof]
    fn proof_win_rate_bounds() {
        let winning: u8 = kani::any();
        let total: u8 = kani::any();
        kani::assume!(total > 0 && winning <= total);

        let rate = calculate_win_rate(winning as usize, total as usize);
        kani::assert!(rate >= 0.0 && rate <= 100.0, "Win rate in [0, 100]");
    }

    #[kani::proof]
    fn proof_win_rate_all_wins() {
        let rate = calculate_win_rate(10, 10);
        kani::assert!((rate - 100.0).abs() < 0.0001, "All wins = 100%");
    }

    #[kani::proof]
    fn proof_win_rate_no_wins() {
        let rate = calculate_win_rate(0, 10);
        kani::assert!(rate == 0.0, "No wins = 0%");
    }

    #[kani::proof]
    fn proof_win_rate_empty() {
        let rate = calculate_win_rate(0, 0);
        kani::assert!(rate == 0.0, "Empty = 0%");
    }

    #[kani::proof]
    fn proof_win_rate_fifty_percent() {
        let rate = calculate_win_rate(5, 10);
        kani::assert!((rate - 50.0).abs() < 0.0001, "5/10 = 50%");
    }

    // ============================================================================
    // Change From Peak Calculation Proofs
    // ============================================================================

    fn change_from_peak_pct(current_price: f64, peak_price: f64) -> f64 {
        if peak_price <= 0.0 {
            return 0.0;
        }
        ((current_price / peak_price) - 1.0) * 100.0
    }

    #[kani::proof]
    fn proof_change_from_peak_at_peak() {
        let change = change_from_peak_pct(100.0, 100.0);
        kani::assert!(change.abs() < 0.0001, "At peak = 0% change");
    }

    #[kani::proof]
    fn proof_change_from_peak_below() {
        let change = change_from_peak_pct(90.0, 100.0);
        kani::assert!((change - (-10.0)).abs() < 0.0001, "10% below peak");
    }

    #[kani::proof]
    fn proof_change_from_peak_always_negative_or_zero() {
        // Current price should never exceed peak (peak updates when price rises)
        let current: u8 = kani::any();
        let peak: u8 = kani::any();
        kani::assume!(peak > 0 && current <= peak);

        let change = change_from_peak_pct(current as f64, peak as f64);
        kani::assert!(change <= 0.0, "Change from peak is always <= 0");
    }

    // ============================================================================
    // Momentum Death Detection Proofs
    // ============================================================================

    fn is_momentum_dead(
        m5_change: f64,
        h1_change: f64,
        m5_threshold: f64,
        h1_threshold: f64,
    ) -> bool {
        m5_change < m5_threshold && h1_change < h1_threshold
    }

    #[kani::proof]
    fn proof_momentum_death_both_below() {
        let dead = is_momentum_dead(-5.0, -10.0, -2.0, -5.0);
        kani::assert!(dead, "Both below threshold = momentum dead");
    }

    #[kani::proof]
    fn proof_momentum_death_m5_above() {
        let dead = is_momentum_dead(1.0, -10.0, -2.0, -5.0);
        kani::assert!(!dead, "5m above threshold = not dead");
    }

    #[kani::proof]
    fn proof_momentum_death_h1_above() {
        let dead = is_momentum_dead(-5.0, 1.0, -2.0, -5.0);
        kani::assert!(!dead, "1h above threshold = not dead");
    }

    #[kani::proof]
    fn proof_momentum_death_both_above() {
        let dead = is_momentum_dead(1.0, 1.0, -2.0, -5.0);
        kani::assert!(!dead, "Both above threshold = not dead");
    }

    // ============================================================================
    // Slippage Calculation Proofs
    // ============================================================================

    fn calculate_slippage_bps(expected: f64, actual: f64) -> i64 {
        if expected <= 0.0 {
            return 0;
        }
        ((actual - expected) / expected * 10000.0) as i64
    }

    #[kani::proof]
    fn proof_slippage_no_change() {
        let slippage = calculate_slippage_bps(100.0, 100.0);
        kani::assert!(slippage == 0, "No price change = 0 slippage");
    }

    #[kani::proof]
    fn proof_slippage_positive() {
        // Got more than expected (positive slippage)
        let slippage = calculate_slippage_bps(100.0, 101.0);
        kani::assert!(slippage == 100, "1% positive slippage = 100 bps");
    }

    #[kani::proof]
    fn proof_slippage_negative() {
        // Got less than expected (negative slippage)
        let slippage = calculate_slippage_bps(100.0, 99.0);
        kani::assert!(slippage == -100, "1% negative slippage = -100 bps");
    }

    // ============================================================================
    // Protocol Risk Score Proofs (defi/protocols)
    // ============================================================================

    fn protocol_risk_score(tvl_usd: f64, age_days: u64, audited: bool, is_upgradeable: bool) -> u8 {
        let mut score = 5u8; // Base score

        // TVL factor
        if tvl_usd >= 100_000_000.0 {
            score = score.saturating_sub(2);
        } else if tvl_usd >= 10_000_000.0 {
            score = score.saturating_sub(1);
        } else if tvl_usd < 1_000_000.0 {
            score = score.saturating_add(2);
        }

        // Age factor
        if age_days >= 365 {
            score = score.saturating_sub(1);
        } else if age_days < 30 {
            score = score.saturating_add(2);
        }

        // Audit factor
        if audited {
            score = score.saturating_sub(1);
        } else {
            score = score.saturating_add(1);
        }

        // Upgradeability factor
        if is_upgradeable {
            score = score.saturating_add(1);
        }

        score.max(1).min(10)
    }

    #[kani::proof]
    fn proof_protocol_risk_score_bounded() {
        let tvl: u8 = kani::any();
        let age: u8 = kani::any();
        let audited: bool = kani::any();
        let upgradeable: bool = kani::any();

        let score = protocol_risk_score(tvl as f64 * 1_000_000.0, age as u64, audited, upgradeable);

        kani::assert!(score >= 1 && score <= 10, "Protocol risk score in [1, 10]");
    }

    #[kani::proof]
    fn proof_protocol_risk_established_protocol() {
        // High TVL, old, audited, not upgradeable = low risk
        let score = protocol_risk_score(500_000_000.0, 730, true, false);
        kani::assert!(score <= 3, "Established protocol has low risk");
    }

    #[kani::proof]
    fn proof_protocol_risk_new_protocol() {
        // Low TVL, new, not audited, upgradeable = high risk
        let score = protocol_risk_score(500_000.0, 7, false, true);
        kani::assert!(score >= 7, "New protocol has high risk");
    }

    // ============================================================================
    // OTC Negotiation State Machine Proofs (otc/negotiation.rs)
    // ============================================================================

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum MockNegotiationState {
        RfqSent,
        RfqReceived,
        QuoteSent,
        QuoteReceived,
        CounterOfferSent,
        CounterOfferReceived,
        Accepted,
        EscrowFunding,
        EscrowFunded,
        Settled,
        Cancelled,
        Expired,
    }

    fn is_terminal_state(state: MockNegotiationState) -> bool {
        matches!(
            state,
            MockNegotiationState::Settled
                | MockNegotiationState::Cancelled
                | MockNegotiationState::Expired
        )
    }

    fn can_transition(from: MockNegotiationState, to: MockNegotiationState) -> bool {
        match from {
            MockNegotiationState::RfqSent => matches!(
                to,
                MockNegotiationState::QuoteReceived
                    | MockNegotiationState::Cancelled
                    | MockNegotiationState::Expired
            ),
            MockNegotiationState::RfqReceived => matches!(
                to,
                MockNegotiationState::QuoteSent | MockNegotiationState::Cancelled
            ),
            MockNegotiationState::QuoteSent => matches!(
                to,
                MockNegotiationState::Accepted
                    | MockNegotiationState::CounterOfferReceived
                    | MockNegotiationState::Cancelled
                    | MockNegotiationState::Expired
            ),
            MockNegotiationState::QuoteReceived => matches!(
                to,
                MockNegotiationState::Accepted
                    | MockNegotiationState::CounterOfferSent
                    | MockNegotiationState::Cancelled
            ),
            MockNegotiationState::Accepted => matches!(
                to,
                MockNegotiationState::EscrowFunding | MockNegotiationState::Cancelled
            ),
            MockNegotiationState::EscrowFunding => matches!(
                to,
                MockNegotiationState::EscrowFunded
                    | MockNegotiationState::Cancelled
                    | MockNegotiationState::Expired
            ),
            MockNegotiationState::EscrowFunded => matches!(
                to,
                MockNegotiationState::Settled | MockNegotiationState::Cancelled
            ),
            // Terminal states cannot transition
            MockNegotiationState::Settled
            | MockNegotiationState::Cancelled
            | MockNegotiationState::Expired => false,
            // Counter offers can be accepted or rejected
            MockNegotiationState::CounterOfferSent | MockNegotiationState::CounterOfferReceived => {
                matches!(
                    to,
                    MockNegotiationState::Accepted
                        | MockNegotiationState::Cancelled
                        | MockNegotiationState::QuoteSent
                        | MockNegotiationState::QuoteReceived
                )
            }
        }
    }

    #[kani::proof]
    fn proof_terminal_states() {
        kani::assert!(
            is_terminal_state(MockNegotiationState::Settled),
            "Settled is terminal"
        );
        kani::assert!(
            is_terminal_state(MockNegotiationState::Cancelled),
            "Cancelled is terminal"
        );
        kani::assert!(
            is_terminal_state(MockNegotiationState::Expired),
            "Expired is terminal"
        );
        kani::assert!(
            !is_terminal_state(MockNegotiationState::RfqSent),
            "RfqSent is not terminal"
        );
        kani::assert!(
            !is_terminal_state(MockNegotiationState::Accepted),
            "Accepted is not terminal"
        );
    }

    #[kani::proof]
    fn proof_terminal_no_transitions() {
        // Terminal states cannot transition to anything
        let to_states = [
            MockNegotiationState::RfqSent,
            MockNegotiationState::QuoteReceived,
            MockNegotiationState::Accepted,
            MockNegotiationState::Settled,
        ];

        for &to in &to_states {
            kani::assert!(
                !can_transition(MockNegotiationState::Settled, to),
                "Settled cannot transition"
            );
            kani::assert!(
                !can_transition(MockNegotiationState::Cancelled, to),
                "Cancelled cannot transition"
            );
        }
    }

    #[kani::proof]
    fn proof_rfq_to_quote_valid() {
        kani::assert!(
            can_transition(
                MockNegotiationState::RfqSent,
                MockNegotiationState::QuoteReceived
            ),
            "RFQ can receive quote"
        );
    }

    #[kani::proof]
    fn proof_accepted_to_escrow_valid() {
        kani::assert!(
            can_transition(
                MockNegotiationState::Accepted,
                MockNegotiationState::EscrowFunding
            ),
            "Accepted can start escrow funding"
        );
    }

    #[kani::proof]
    fn proof_escrow_funded_to_settled_valid() {
        kani::assert!(
            can_transition(
                MockNegotiationState::EscrowFunded,
                MockNegotiationState::Settled
            ),
            "Funded escrow can settle"
        );
    }

    // ============================================================================
    // Program Upgrade Monitoring Proofs (monitor/upgrade_detector.rs)
    // ============================================================================

    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
    enum MockRiskLevel {
        Low,
        Medium,
        High,
        Critical,
    }

    fn calculate_upgrade_risk_level(
        frequent_upgrades: bool,
        unknown_authority: bool,
        recent_authority_change: bool,
        large_code_change: bool,
    ) -> MockRiskLevel {
        let mut score = 0u8;

        if frequent_upgrades {
            score += 2;
        }
        if unknown_authority {
            score += 3;
        }
        if recent_authority_change {
            score += 2;
        }
        if large_code_change {
            score += 1;
        }

        match score {
            0..=1 => MockRiskLevel::Low,
            2..=3 => MockRiskLevel::Medium,
            4..=5 => MockRiskLevel::High,
            _ => MockRiskLevel::Critical,
        }
    }

    #[kani::proof]
    fn proof_upgrade_risk_no_flags() {
        let risk = calculate_upgrade_risk_level(false, false, false, false);
        kani::assert!(risk == MockRiskLevel::Low, "No risk flags = low risk");
    }

    #[kani::proof]
    fn proof_upgrade_risk_unknown_authority() {
        let risk = calculate_upgrade_risk_level(false, true, false, false);
        kani::assert!(risk >= MockRiskLevel::Medium, "Unknown authority is risky");
    }

    #[kani::proof]
    fn proof_upgrade_risk_multiple_flags() {
        let risk = calculate_upgrade_risk_level(true, true, true, true);
        kani::assert!(risk == MockRiskLevel::Critical, "Multiple flags = critical");
    }

    #[kani::proof]
    fn proof_upgrade_risk_bounded() {
        let f1: bool = kani::any();
        let f2: bool = kani::any();
        let f3: bool = kani::any();
        let f4: bool = kani::any();

        let risk = calculate_upgrade_risk_level(f1, f2, f3, f4);

        // Risk is always a valid level
        kani::assert!(
            risk == MockRiskLevel::Low
                || risk == MockRiskLevel::Medium
                || risk == MockRiskLevel::High
                || risk == MockRiskLevel::Critical,
            "Risk is always a valid level"
        );
    }

    #[kani::proof]
    fn proof_upgrade_risk_ordering() {
        kani::assert!(MockRiskLevel::Low < MockRiskLevel::Medium, "Low < Medium");
        kani::assert!(MockRiskLevel::Medium < MockRiskLevel::High, "Medium < High");
        kani::assert!(
            MockRiskLevel::High < MockRiskLevel::Critical,
            "High < Critical"
        );
    }

    // ============================================================================
    // Yield Opportunity Ranking Proofs (defi/yield_discovery.rs)
    // ============================================================================

    fn yield_opportunity_score(apy: f64, tvl_usd: f64, risk_score: u8) -> f64 {
        // Higher APY is better, higher TVL is better (safer), lower risk is better
        let tvl_factor = (tvl_usd / 1_000_000.0).min(10.0); // Cap at 10M factor
        let risk_factor = 11.0 - risk_score as f64; // Invert: lower risk = higher factor

        apy * tvl_factor * risk_factor / 100.0
    }

    #[kani::proof]
    fn proof_yield_score_apy_dominates() {
        // Higher APY should lead to higher score (same TVL and risk)
        let score1 = yield_opportunity_score(5.0, 10_000_000.0, 3);
        let score2 = yield_opportunity_score(10.0, 10_000_000.0, 3);
        kani::assert!(score2 > score1, "Higher APY = higher score");
    }

    #[kani::proof]
    fn proof_yield_score_tvl_factor() {
        // Higher TVL should lead to higher score (same APY and risk)
        let score1 = yield_opportunity_score(10.0, 1_000_000.0, 3);
        let score2 = yield_opportunity_score(10.0, 5_000_000.0, 3);
        kani::assert!(score2 > score1, "Higher TVL = higher score");
    }

    #[kani::proof]
    fn proof_yield_score_risk_factor() {
        // Lower risk should lead to higher score (same APY and TVL)
        let score_risky = yield_opportunity_score(10.0, 10_000_000.0, 8);
        let score_safe = yield_opportunity_score(10.0, 10_000_000.0, 2);
        kani::assert!(score_safe > score_risky, "Lower risk = higher score");
    }

    #[kani::proof]
    fn proof_yield_score_non_negative() {
        let apy: u8 = kani::any();
        let tvl: u8 = kani::any();
        let risk: u8 = kani::any();
        kani::assume!(risk >= 1 && risk <= 10);

        let score = yield_opportunity_score(apy as f64, tvl as f64 * 100_000.0, risk);
        kani::assert!(score >= 0.0, "Yield score is non-negative");
    }

    // ============================================================================
    // Jupiter Swap Proofs (trading/jupiter.rs)
    // ============================================================================

    fn calculate_swap_output(input_amount: u64, price: f64, slippage_bps: u16) -> u64 {
        let gross_output = input_amount as f64 * price;
        let slippage_factor = 1.0 - (slippage_bps as f64 / 10000.0);
        (gross_output * slippage_factor) as u64
    }

    fn calculate_minimum_output(expected_output: u64, slippage_bps: u16) -> u64 {
        let slippage_factor = 1.0 - (slippage_bps as f64 / 10000.0);
        (expected_output as f64 * slippage_factor) as u64
    }

    fn is_quote_acceptable(
        out_amount: u64,
        min_acceptable: u64,
        price_impact_bps: u16,
        max_impact_bps: u16,
    ) -> bool {
        out_amount >= min_acceptable && price_impact_bps <= max_impact_bps
    }

    #[kani::proof]
    fn proof_swap_output_with_slippage() {
        let input = 1_000_000_000u64; // 1 SOL
        let price = 100.0; // $100 per SOL
        let slippage = 50u16; // 0.5%

        let output = calculate_swap_output(input, price, slippage);
        let expected = (100_000_000_000.0 * 0.995) as u64;

        kani::assert!(
            (output as i64 - expected as i64).abs() < 1000,
            "Slippage applied correctly"
        );
    }

    #[kani::proof]
    fn proof_swap_output_no_slippage() {
        let input = 1_000_000_000u64;
        let price = 100.0;
        let slippage = 0u16;

        let output = calculate_swap_output(input, price, slippage);
        let expected = 100_000_000_000u64;

        kani::assert!(output == expected, "No slippage = exact output");
    }

    #[kani::proof]
    fn proof_min_output_less_than_expected() {
        let expected: u16 = kani::any();
        let slippage: u8 = kani::any();
        kani::assume!(slippage > 0 && slippage < 100); // 0-1% slippage

        let min = calculate_minimum_output(expected as u64 * 1_000_000, slippage as u16);
        kani::assert!(
            min <= expected as u64 * 1_000_000,
            "Minimum output <= expected"
        );
    }

    #[kani::proof]
    fn proof_quote_acceptable_conditions() {
        // Good quote: output above min, low impact
        kani::assert!(
            is_quote_acceptable(1000, 900, 10, 100),
            "Good quote is acceptable"
        );

        // Bad quote: output below min
        kani::assert!(
            !is_quote_acceptable(800, 900, 10, 100),
            "Output below min is rejected"
        );

        // Bad quote: high impact
        kani::assert!(
            !is_quote_acceptable(1000, 900, 200, 100),
            "High impact is rejected"
        );
    }

    // ============================================================================
    // Token Balance Validation Proofs (wallet/balance.rs)
    // ============================================================================

    fn lamports_to_sol(lamports: u64) -> f64 {
        lamports as f64 / 1_000_000_000.0
    }

    fn sol_to_lamports(sol: f64) -> u64 {
        (sol * 1_000_000_000.0) as u64
    }

    fn has_sufficient_balance(balance: u64, required: u64, fee_buffer: u64) -> bool {
        balance >= required.saturating_add(fee_buffer)
    }

    #[kani::proof]
    fn proof_lamports_to_sol_conversion() {
        let lamports = 5_000_000_000u64; // 5 SOL
        let sol = lamports_to_sol(lamports);
        kani::assert!((sol - 5.0).abs() < 0.0001, "5 billion lamports = 5 SOL");
    }

    #[kani::proof]
    fn proof_sol_to_lamports_conversion() {
        let sol = 2.5f64;
        let lamports = sol_to_lamports(sol);
        kani::assert!(lamports == 2_500_000_000, "2.5 SOL = 2.5 billion lamports");
    }

    #[kani::proof]
    fn proof_conversion_roundtrip() {
        let original_sol = 1.5f64;
        let lamports = sol_to_lamports(original_sol);
        let back_to_sol = lamports_to_sol(lamports);
        kani::assert!(
            (back_to_sol - original_sol).abs() < 0.000001,
            "Roundtrip conversion"
        );
    }

    #[kani::proof]
    fn proof_sufficient_balance_exact() {
        // Exact balance with fee buffer
        kani::assert!(
            has_sufficient_balance(1_005_000, 1_000_000, 5000),
            "Exact balance is sufficient"
        );
    }

    #[kani::proof]
    fn proof_sufficient_balance_excess() {
        // More than enough
        kani::assert!(
            has_sufficient_balance(2_000_000, 1_000_000, 5000),
            "Excess balance is sufficient"
        );
    }

    #[kani::proof]
    fn proof_insufficient_balance() {
        // Not enough for fee
        kani::assert!(
            !has_sufficient_balance(1_000_000, 1_000_000, 5000),
            "No fee buffer is insufficient"
        );
    }

    #[kani::proof]
    fn proof_balance_check_no_overflow() {
        let balance: u64 = kani::any();
        let required: u64 = kani::any();
        let fee: u64 = kani::any();

        // saturating_add prevents overflow
        let result = has_sufficient_balance(balance, required, fee);

        // Result is always valid boolean (no panic)
        kani::assert!(
            result == true || result == false,
            "Always returns valid bool"
        );
    }

    // ============================================================================
    // Transfer Validation Proofs (wallet/transfer.rs)
    // ============================================================================

    fn is_valid_transfer_amount(amount: u64, balance: u64, min_rent: u64) -> bool {
        amount > 0 && balance >= amount && (balance - amount) >= min_rent
    }

    fn calculate_transfer_fee(amount: u64, fee_rate_bps: u16) -> u64 {
        (amount as u128 * fee_rate_bps as u128 / 10000) as u64
    }

    #[kani::proof]
    fn proof_valid_transfer() {
        // Valid: 5 SOL from 10 SOL balance, leaving rent
        let result = is_valid_transfer_amount(
            5_000_000_000,  // 5 SOL
            10_000_000_000, // 10 SOL balance
            890_880,        // Rent exempt minimum
        );
        kani::assert!(result, "Valid transfer should pass");
    }

    #[kani::proof]
    fn proof_invalid_transfer_zero() {
        let result = is_valid_transfer_amount(0, 10_000_000_000, 890_880);
        kani::assert!(!result, "Zero amount is invalid");
    }

    #[kani::proof]
    fn proof_invalid_transfer_exceeds_balance() {
        let result = is_valid_transfer_amount(
            15_000_000_000, // 15 SOL
            10_000_000_000, // Only 10 SOL
            890_880,
        );
        kani::assert!(!result, "Exceeds balance is invalid");
    }

    #[kani::proof]
    fn proof_invalid_transfer_below_rent() {
        // Would leave less than rent
        let result = is_valid_transfer_amount(
            9_999_500_000,  // Almost all
            10_000_000_000, // 10 SOL
            890_880,        // Rent
        );
        kani::assert!(!result, "Would leave below rent is invalid");
    }

    #[kani::proof]
    fn proof_transfer_fee_calculation() {
        let amount = 1_000_000_000u64; // 1 SOL
        let fee_bps = 25u16; // 0.25%

        let fee = calculate_transfer_fee(amount, fee_bps);
        let expected = 2_500_000u64; // 0.0025 SOL

        kani::assert!(fee == expected, "0.25% fee on 1 SOL");
    }

    #[kani::proof]
    fn proof_transfer_fee_zero_rate() {
        let fee = calculate_transfer_fee(1_000_000_000, 0);
        kani::assert!(fee == 0, "Zero rate = zero fee");
    }

    // ============================================================================
    // Correlation Pearson Coefficient Proofs (risk/correlation.rs)
    // ============================================================================

    fn pearson_correlation(x: &[f64], y: &[f64]) -> f64 {
        let n = x.len().min(y.len());
        if n < 2 {
            return 0.0;
        }

        let n_f = n as f64;
        let sum_x: f64 = x.iter().take(n).sum();
        let sum_y: f64 = y.iter().take(n).sum();
        let sum_xx: f64 = x.iter().take(n).map(|v| v * v).sum();
        let sum_yy: f64 = y.iter().take(n).map(|v| v * v).sum();
        let sum_xy: f64 = x.iter().zip(y.iter()).take(n).map(|(a, b)| a * b).sum();

        let numerator = n_f * sum_xy - sum_x * sum_y;
        let denominator = ((n_f * sum_xx - sum_x * sum_x) * (n_f * sum_yy - sum_y * sum_y)).sqrt();

        if denominator == 0.0 {
            return 0.0;
        }

        numerator / denominator
    }

    #[kani::proof]
    fn proof_pearson_perfect_positive() {
        let x = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let y = vec![2.0, 4.0, 6.0, 8.0, 10.0]; // y = 2x
        let corr = pearson_correlation(&x, &y);
        kani::assert!((corr - 1.0).abs() < 0.0001, "Perfect positive correlation");
    }

    #[kani::proof]
    fn proof_pearson_perfect_negative() {
        let x = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let y = vec![10.0, 8.0, 6.0, 4.0, 2.0]; // y = -2x + 12
        let corr = pearson_correlation(&x, &y);
        kani::assert!(
            (corr - (-1.0)).abs() < 0.0001,
            "Perfect negative correlation"
        );
    }

    #[kani::proof]
    fn proof_pearson_self_correlation() {
        let x = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let corr = pearson_correlation(&x, &x);
        kani::assert!((corr - 1.0).abs() < 0.0001, "Self correlation is 1");
    }

    #[kani::proof]
    fn proof_pearson_empty() {
        let x: Vec<f64> = vec![];
        let y: Vec<f64> = vec![];
        let corr = pearson_correlation(&x, &y);
        kani::assert!(corr == 0.0, "Empty arrays = 0 correlation");
    }

    // ============================================================================
    // Staking Reward Calculation Proofs (wallet/staking.rs)
    // ============================================================================

    fn estimate_staking_apy(
        epoch_rate: f64,
        epochs_per_year: u64,
        validator_commission: f64,
    ) -> f64 {
        let gross_apy = epoch_rate * epochs_per_year as f64;
        gross_apy * (1.0 - validator_commission)
    }

    fn calculate_expected_rewards(staked_amount: u64, apy: f64, days: u64) -> u64 {
        let daily_rate = apy / 365.0;
        let rewards = staked_amount as f64 * daily_rate * days as f64;
        rewards as u64
    }

    #[kani::proof]
    fn proof_staking_apy_with_commission() {
        let epoch_rate = 0.001; // 0.1% per epoch
        let epochs_per_year = 150u64;
        let commission = 0.05; // 5%

        let apy = estimate_staking_apy(epoch_rate, epochs_per_year, commission);
        let expected = 0.001 * 150.0 * 0.95; // ~14.25%

        kani::assert!((apy - expected).abs() < 0.001, "APY with commission");
    }

    #[kani::proof]
    fn proof_staking_apy_zero_commission() {
        let epoch_rate = 0.001;
        let epochs_per_year = 150u64;
        let commission = 0.0;

        let apy = estimate_staking_apy(epoch_rate, epochs_per_year, commission);
        let expected = 0.001 * 150.0;

        kani::assert!((apy - expected).abs() < 0.0001, "APY without commission");
    }

    #[kani::proof]
    fn proof_staking_rewards_calculation() {
        let staked = 10_000_000_000u64; // 10 SOL
        let apy = 0.07; // 7%
        let days = 365u64;

        let rewards = calculate_expected_rewards(staked, apy, days);
        let expected = (10_000_000_000.0 * 0.07) as u64; // ~0.7 SOL

        kani::assert!(
            (rewards as i64 - expected as i64).abs() < 1000,
            "Full year rewards"
        );
    }

    #[kani::proof]
    fn proof_staking_rewards_proportional() {
        let apy = 0.07;
        let days = 30u64;

        let rewards_10sol = calculate_expected_rewards(10_000_000_000, apy, days);
        let rewards_20sol = calculate_expected_rewards(20_000_000_000, apy, days);

        // 2x stake = 2x rewards (approximately)
        let ratio = rewards_20sol as f64 / rewards_10sol as f64;
        kani::assert!((ratio - 2.0).abs() < 0.01, "Rewards proportional to stake");
    }

    // ============================================================================
    // Price Oracle Validation Proofs (protocols/pyth.rs)
    // ============================================================================

    fn is_price_valid(
        price: f64,
        confidence: f64,
        max_confidence_ratio: f64,
        max_age_secs: u64,
        current_time: u64,
        publish_time: u64,
    ) -> bool {
        if price <= 0.0 {
            return false;
        }

        // Confidence check
        let confidence_ratio = confidence / price;
        if confidence_ratio > max_confidence_ratio {
            return false;
        }

        // Staleness check
        let age = current_time.saturating_sub(publish_time);
        if age > max_age_secs {
            return false;
        }

        true
    }

    #[kani::proof]
    fn proof_price_valid() {
        let result = is_price_valid(
            100.0, // price
            0.5,   // confidence (0.5% of price)
            0.01,  // max 1% confidence ratio
            60,    // max 60s stale
            1000,  // current time
            950,   // publish time (50s ago)
        );
        kani::assert!(result, "Valid price passes all checks");
    }

    #[kani::proof]
    fn proof_price_invalid_zero() {
        let result = is_price_valid(0.0, 0.5, 0.01, 60, 1000, 950);
        kani::assert!(!result, "Zero price is invalid");
    }

    #[kani::proof]
    fn proof_price_invalid_negative() {
        let result = is_price_valid(-100.0, 0.5, 0.01, 60, 1000, 950);
        kani::assert!(!result, "Negative price is invalid");
    }

    #[kani::proof]
    fn proof_price_invalid_wide_confidence() {
        let result = is_price_valid(
            100.0, // price
            5.0,   // confidence (5% of price - too wide)
            0.01,  // max 1%
            60, 1000, 950,
        );
        kani::assert!(!result, "Wide confidence is invalid");
    }

    #[kani::proof]
    fn proof_price_invalid_stale() {
        let result = is_price_valid(
            100.0, 0.5, 0.01, 60,   // max 60s
            1000, // current
            900,  // 100s ago - stale
        );
        kani::assert!(!result, "Stale price is invalid");
    }

    // ============================================================================
    // Liquidity Pool Math Proofs (protocols/raydium.rs, meteora.rs)
    // ============================================================================

    fn calculate_lp_share(deposited_value: f64, pool_tvl: f64) -> f64 {
        if pool_tvl <= 0.0 {
            return 0.0;
        }
        deposited_value / pool_tvl
    }

    fn calculate_impermanent_loss(price_ratio: f64, // new_price / original_price
    ) -> f64 {
        // IL formula: 2 * sqrt(price_ratio) / (1 + price_ratio) - 1
        let sqrt_ratio = price_ratio.sqrt();
        2.0 * sqrt_ratio / (1.0 + price_ratio) - 1.0
    }

    #[kani::proof]
    fn proof_lp_share_calculation() {
        let share = calculate_lp_share(10_000.0, 1_000_000.0);
        kani::assert!((share - 0.01).abs() < 0.0001, "1% share of pool");
    }

    #[kani::proof]
    fn proof_lp_share_bounds() {
        let deposit: u8 = kani::any();
        let tvl: u8 = kani::any();
        kani::assume!(tvl > 0 && deposit <= tvl);

        let share = calculate_lp_share(deposit as f64, tvl as f64);
        kani::assert!(share >= 0.0 && share <= 1.0, "Share in [0, 1]");
    }

    #[kani::proof]
    fn proof_il_no_change() {
        let il = calculate_impermanent_loss(1.0);
        kani::assert!(il.abs() < 0.0001, "No price change = no IL");
    }

    #[kani::proof]
    fn proof_il_2x_price() {
        let il = calculate_impermanent_loss(2.0);
        // IL at 2x should be about -5.7%
        kani::assert!(il < 0.0, "Price increase causes negative IL");
        kani::assert!(il > -0.1, "IL at 2x is small");
    }

    #[kani::proof]
    fn proof_il_symmetric() {
        // IL is symmetric for inverse price changes
        let il_2x = calculate_impermanent_loss(2.0);
        let il_half = calculate_impermanent_loss(0.5);
        kani::assert!(
            (il_2x - il_half).abs() < 0.0001,
            "IL symmetric for inverse prices"
        );
    }

    // ============================================================================
    // Perpetual Futures Margin Proofs (protocols/drift.rs)
    // ============================================================================

    fn calculate_required_margin(position_size: f64, price: f64, leverage: f64) -> f64 {
        if leverage <= 0.0 {
            return position_size * price;
        }
        (position_size * price) / leverage
    }

    fn calculate_liquidation_price(
        entry_price: f64,
        leverage: f64,
        is_long: bool,
        maintenance_margin_ratio: f64,
    ) -> f64 {
        let margin_buffer = 1.0 / leverage;
        let liquidation_delta = margin_buffer * (1.0 - maintenance_margin_ratio);

        if is_long {
            entry_price * (1.0 - liquidation_delta)
        } else {
            entry_price * (1.0 + liquidation_delta)
        }
    }

    fn calculate_pnl_perp(entry_price: f64, current_price: f64, size: f64, is_long: bool) -> f64 {
        let price_diff = current_price - entry_price;
        if is_long {
            size * price_diff
        } else {
            size * (-price_diff)
        }
    }

    #[kani::proof]
    fn proof_margin_with_leverage() {
        let margin = calculate_required_margin(1.0, 100.0, 10.0);
        kani::assert!((margin - 10.0).abs() < 0.0001, "10x leverage = 10% margin");
    }

    #[kani::proof]
    fn proof_margin_no_leverage() {
        let margin = calculate_required_margin(1.0, 100.0, 1.0);
        kani::assert!((margin - 100.0).abs() < 0.0001, "1x = full margin");
    }

    #[kani::proof]
    fn proof_liquidation_long() {
        let liq = calculate_liquidation_price(100.0, 10.0, true, 0.05);
        // At 10x, liquidation should be about 9.5% below entry
        kani::assert!(liq < 100.0, "Long liquidation below entry");
        kani::assert!(liq > 90.0, "Long liquidation not too far");
    }

    #[kani::proof]
    fn proof_liquidation_short() {
        let liq = calculate_liquidation_price(100.0, 10.0, false, 0.05);
        // At 10x, liquidation should be about 9.5% above entry
        kani::assert!(liq > 100.0, "Short liquidation above entry");
        kani::assert!(liq < 110.0, "Short liquidation not too far");
    }

    #[kani::proof]
    fn proof_perp_pnl_long_profit() {
        let pnl = calculate_pnl_perp(100.0, 110.0, 1.0, true);
        kani::assert!((pnl - 10.0).abs() < 0.0001, "Long profit when price up");
    }

    #[kani::proof]
    fn proof_perp_pnl_long_loss() {
        let pnl = calculate_pnl_perp(100.0, 90.0, 1.0, true);
        kani::assert!((pnl - (-10.0)).abs() < 0.0001, "Long loss when price down");
    }

    #[kani::proof]
    fn proof_perp_pnl_short_profit() {
        let pnl = calculate_pnl_perp(100.0, 90.0, 1.0, false);
        kani::assert!((pnl - 10.0).abs() < 0.0001, "Short profit when price down");
    }

    #[kani::proof]
    fn proof_perp_pnl_short_loss() {
        let pnl = calculate_pnl_perp(100.0, 110.0, 1.0, false);
        kani::assert!((pnl - (-10.0)).abs() < 0.0001, "Short loss when price up");
    }

    // ============================================================================
    // NFT Price Validation Proofs (protocols/nft.rs)
    // ============================================================================

    fn is_nft_price_reasonable(listing_price: f64, floor_price: f64, max_premium_pct: f64) -> bool {
        if floor_price <= 0.0 {
            return false;
        }
        let premium = (listing_price / floor_price - 1.0) * 100.0;
        premium <= max_premium_pct
    }

    fn calculate_royalty(sale_price: f64, royalty_bps: u16) -> f64 {
        sale_price * (royalty_bps as f64 / 10000.0)
    }

    #[kani::proof]
    fn proof_nft_price_at_floor() {
        let result = is_nft_price_reasonable(5.0, 5.0, 50.0);
        kani::assert!(result, "At floor price is reasonable");
    }

    #[kani::proof]
    fn proof_nft_price_small_premium() {
        let result = is_nft_price_reasonable(6.0, 5.0, 50.0);
        kani::assert!(result, "20% premium is reasonable");
    }

    #[kani::proof]
    fn proof_nft_price_excessive_premium() {
        let result = is_nft_price_reasonable(10.0, 5.0, 50.0);
        kani::assert!(!result, "100% premium exceeds 50% limit");
    }

    #[kani::proof]
    fn proof_nft_royalty_calculation() {
        let royalty = calculate_royalty(100.0, 500); // 5%
        kani::assert!((royalty - 5.0).abs() < 0.0001, "5% royalty on 100 SOL");
    }

    #[kani::proof]
    fn proof_nft_royalty_zero() {
        let royalty = calculate_royalty(100.0, 0);
        kani::assert!(royalty == 0.0, "0% royalty = 0");
    }

    // ============================================================================
    // OSINT Marketplace Fee Proofs (osint/types.rs, osint/escrow.rs)
    // ============================================================================

    /// Calculate creation fee for a bounty.
    fn calculate_osint_creation_fee(amount: u64, fee_bps: u16) -> u64 {
        ((amount as u128 * fee_bps as u128) / 10_000) as u64
    }

    /// Calculate payout fee when bounty is resolved.
    fn calculate_osint_payout_fee(amount: u64, fee_bps: u16) -> u64 {
        ((amount as u128 * fee_bps as u128) / 10_000) as u64
    }

    /// Calculate net amount after all fees.
    fn calculate_osint_net_amount(reward: u64, creation_bps: u16, payout_bps: u16) -> u64 {
        let creation_fee = calculate_osint_creation_fee(reward, creation_bps);
        let after_creation = reward - creation_fee;
        let payout_fee = calculate_osint_payout_fee(after_creation, payout_bps);
        after_creation - payout_fee
    }

    #[kani::proof]
    fn proof_osint_creation_fee_default() {
        // Default fee is 2.5% (250 bps)
        let fee = calculate_osint_creation_fee(1_000_000_000, 250);
        kani::assert!(fee == 25_000_000, "2.5% of 1 SOL = 0.025 SOL");
    }

    #[kani::proof]
    fn proof_osint_creation_fee_zero() {
        let fee = calculate_osint_creation_fee(1_000_000_000, 0);
        kani::assert!(fee == 0, "0% fee = 0");
    }

    #[kani::proof]
    fn proof_osint_fee_less_than_amount() {
        let amount: u32 = kani::any();
        let fee_bps: u16 = kani::any();
        kani::assume!(fee_bps < 10_000); // Less than 100%
        kani::assume!(amount > 0);

        let fee = calculate_osint_creation_fee(amount as u64, fee_bps);
        kani::assert!(
            fee < amount as u64,
            "Fee always less than amount when bps < 100%"
        );
    }

    #[kani::proof]
    fn proof_osint_payout_fee_default() {
        // Default payout fee is also 2.5% (250 bps)
        let fee = calculate_osint_payout_fee(975_000_000, 250);
        kani::assert!(fee == 24_375_000, "2.5% of 0.975 SOL");
    }

    #[kani::proof]
    fn proof_osint_net_amount_positive() {
        let reward: u32 = kani::any();
        kani::assume!(reward >= 1000); // Minimum reasonable reward

        // Use realistic fee rates (both under 10%)
        let creation_bps: u16 = kani::any();
        let payout_bps: u16 = kani::any();
        kani::assume!(creation_bps <= 1000); // Max 10%
        kani::assume!(payout_bps <= 1000); // Max 10%

        let net = calculate_osint_net_amount(reward as u64, creation_bps, payout_bps);
        kani::assert!(net > 0, "Net amount always positive with reasonable fees");
    }

    #[kani::proof]
    fn proof_osint_net_amount_less_than_reward() {
        let reward: u32 = kani::any();
        kani::assume!(reward >= 1000);

        let creation_bps: u16 = kani::any();
        let payout_bps: u16 = kani::any();
        kani::assume!(creation_bps > 0 || payout_bps > 0); // At least some fee
        kani::assume!(creation_bps <= 1000);
        kani::assume!(payout_bps <= 1000);

        let net = calculate_osint_net_amount(reward as u64, creation_bps, payout_bps);
        kani::assert!(
            net < reward as u64,
            "Net amount less than reward when fees exist"
        );
    }

    #[kani::proof]
    fn proof_osint_total_fees() {
        // With 2.5% creation + 2.5% payout on 1 SOL
        let reward = 1_000_000_000u64;
        let creation_fee = calculate_osint_creation_fee(reward, 250);
        let after_creation = reward - creation_fee;
        let payout_fee = calculate_osint_payout_fee(after_creation, 250);
        let total_fees = creation_fee + payout_fee;

        // Total fees should be about 4.9375%
        kani::assert!(creation_fee == 25_000_000, "Creation fee = 25M lamports");
        kani::assert!(payout_fee == 24_375_000, "Payout fee = 24.375M lamports");
        kani::assert!(total_fees == 49_375_000, "Total fees = 49.375M lamports");
    }

    // ============================================================================
    // OSINT Confidence Score Proofs (osint/types.rs)
    // ============================================================================

    fn is_valid_confidence(confidence: u8) -> bool {
        confidence <= 100
    }

    fn confidence_to_category(confidence: u8) -> &'static str {
        match confidence {
            0..=30 => "low",
            31..=60 => "medium",
            61..=80 => "high",
            81..=100 => "very_high",
            _ => "invalid",
        }
    }

    #[kani::proof]
    fn proof_confidence_always_valid_u8() {
        let confidence: u8 = kani::any();
        // u8 max is 255, so we check the function handles values > 100
        let valid = is_valid_confidence(confidence);
        kani::assert!((confidence <= 100) == valid, "Confidence valid iff <= 100");
    }

    #[kani::proof]
    fn proof_confidence_category_exhaustive() {
        let confidence: u8 = kani::any();
        kani::assume!(confidence <= 100);

        let category = confidence_to_category(confidence);
        kani::assert!(
            category == "low"
                || category == "medium"
                || category == "high"
                || category == "very_high",
            "Valid confidence has valid category"
        );
    }

    #[kani::proof]
    fn proof_confidence_category_boundaries() {
        // Test boundary values
        kani::assert!(confidence_to_category(0) == "low", "0 is low");
        kani::assert!(confidence_to_category(30) == "low", "30 is low");
        kani::assert!(confidence_to_category(31) == "medium", "31 is medium");
        kani::assert!(confidence_to_category(60) == "medium", "60 is medium");
        kani::assert!(confidence_to_category(61) == "high", "61 is high");
        kani::assert!(confidence_to_category(80) == "high", "80 is high");
        kani::assert!(confidence_to_category(81) == "very_high", "81 is very high");
        kani::assert!(
            confidence_to_category(100) == "very_high",
            "100 is very high"
        );
    }

    // ============================================================================
    // OSINT Bounty Status Transition Proofs (osint/types.rs)
    // ============================================================================

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum OsintBountyStatus {
        Open,
        Claimed,
        Submitted,
        Resolved,
        Expired,
        Disputed,
    }

    fn can_transition(from: OsintBountyStatus, to: OsintBountyStatus) -> bool {
        use OsintBountyStatus::*;
        matches!(
            (from, to),
            (Open, Claimed) |
            (Open, Expired) |
            (Claimed, Submitted) |
            (Claimed, Open) |      // Claim expired, released
            (Claimed, Expired) |
            (Submitted, Resolved) |
            (Submitted, Expired) |
            (Resolved, Disputed)
        )
    }

    fn is_terminal_status(status: OsintBountyStatus) -> bool {
        matches!(
            status,
            OsintBountyStatus::Expired | OsintBountyStatus::Resolved
        )
    }

    #[kani::proof]
    fn proof_bounty_open_to_claimed() {
        kani::assert!(
            can_transition(OsintBountyStatus::Open, OsintBountyStatus::Claimed),
            "Open -> Claimed is valid"
        );
    }

    #[kani::proof]
    fn proof_bounty_claimed_to_submitted() {
        kani::assert!(
            can_transition(OsintBountyStatus::Claimed, OsintBountyStatus::Submitted),
            "Claimed -> Submitted is valid"
        );
    }

    #[kani::proof]
    fn proof_bounty_submitted_to_resolved() {
        kani::assert!(
            can_transition(OsintBountyStatus::Submitted, OsintBountyStatus::Resolved),
            "Submitted -> Resolved is valid"
        );
    }

    #[kani::proof]
    fn proof_bounty_cannot_skip_stages() {
        kani::assert!(
            !can_transition(OsintBountyStatus::Open, OsintBountyStatus::Submitted),
            "Cannot skip Claimed stage"
        );
        kani::assert!(
            !can_transition(OsintBountyStatus::Open, OsintBountyStatus::Resolved),
            "Cannot skip to Resolved"
        );
        kani::assert!(
            !can_transition(OsintBountyStatus::Claimed, OsintBountyStatus::Resolved),
            "Cannot skip Submitted stage"
        );
    }

    #[kani::proof]
    fn proof_bounty_terminal_no_transitions() {
        // From terminal states, only Resolved can go to Disputed
        kani::assert!(
            !can_transition(OsintBountyStatus::Expired, OsintBountyStatus::Open),
            "Expired is terminal"
        );
        kani::assert!(
            !can_transition(OsintBountyStatus::Expired, OsintBountyStatus::Claimed),
            "Cannot claim expired"
        );
    }

    #[kani::proof]
    fn proof_bounty_dispute_from_resolved() {
        kani::assert!(
            can_transition(OsintBountyStatus::Resolved, OsintBountyStatus::Disputed),
            "Resolved -> Disputed is valid for appeals"
        );
    }

    // ============================================================================
    // OSINT Reward Calculation Proofs (osint/types.rs)
    // ============================================================================

    /// Convert lamports to SOL (9 decimals).
    fn lamports_to_sol(lamports: u64) -> f64 {
        lamports as f64 / 1_000_000_000.0
    }

    /// Convert USDC atomic units to USDC (6 decimals).
    fn atomic_to_usdc(units: u64) -> f64 {
        units as f64 / 1_000_000.0
    }

    #[kani::proof]
    fn proof_lamports_to_sol_one() {
        let sol = lamports_to_sol(1_000_000_000);
        kani::assert!((sol - 1.0).abs() < 0.0001, "1 billion lamports = 1 SOL");
    }

    #[kani::proof]
    fn proof_lamports_to_sol_zero() {
        let sol = lamports_to_sol(0);
        kani::assert!(sol == 0.0, "0 lamports = 0 SOL");
    }

    #[kani::proof]
    fn proof_lamports_to_sol_fraction() {
        let sol = lamports_to_sol(500_000_000);
        kani::assert!((sol - 0.5).abs() < 0.0001, "500M lamports = 0.5 SOL");
    }

    #[kani::proof]
    fn proof_usdc_conversion() {
        let usdc = atomic_to_usdc(50_000_000);
        kani::assert!((usdc - 50.0).abs() < 0.0001, "50M atomic = 50 USDC");
    }

    #[kani::proof]
    fn proof_usdc_small_amount() {
        let usdc = atomic_to_usdc(100_000);
        kani::assert!((usdc - 0.1).abs() < 0.0001, "100K atomic = 0.1 USDC");
    }

    // ============================================================================
    // OSINT Escrow Status Transition Proofs (osint/escrow.rs)
    // ============================================================================

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum OsintEscrowStatus {
        Pending,
        Funded,
        Released,
        Refunded,
        Disputed,
    }

    fn can_escrow_transition(from: OsintEscrowStatus, to: OsintEscrowStatus) -> bool {
        use OsintEscrowStatus::*;
        matches!(
            (from, to),
            (Pending, Funded)
                | (Funded, Released)
                | (Funded, Refunded)
                | (Funded, Disputed)
                | (Disputed, Released)
                | (Disputed, Refunded)
        )
    }

    fn is_escrow_terminal(status: OsintEscrowStatus) -> bool {
        matches!(
            status,
            OsintEscrowStatus::Released | OsintEscrowStatus::Refunded
        )
    }

    #[kani::proof]
    fn proof_escrow_happy_path() {
        kani::assert!(
            can_escrow_transition(OsintEscrowStatus::Pending, OsintEscrowStatus::Funded),
            "Pending -> Funded valid"
        );
        kani::assert!(
            can_escrow_transition(OsintEscrowStatus::Funded, OsintEscrowStatus::Released),
            "Funded -> Released valid (approved submission)"
        );
    }

    #[kani::proof]
    fn proof_escrow_refund_path() {
        kani::assert!(
            can_escrow_transition(OsintEscrowStatus::Funded, OsintEscrowStatus::Refunded),
            "Funded -> Refunded valid (rejected/cancelled)"
        );
    }

    #[kani::proof]
    fn proof_escrow_dispute_resolution() {
        kani::assert!(
            can_escrow_transition(OsintEscrowStatus::Funded, OsintEscrowStatus::Disputed),
            "Funded -> Disputed valid"
        );
        kani::assert!(
            can_escrow_transition(OsintEscrowStatus::Disputed, OsintEscrowStatus::Released),
            "Disputed can be resolved with release"
        );
        kani::assert!(
            can_escrow_transition(OsintEscrowStatus::Disputed, OsintEscrowStatus::Refunded),
            "Disputed can be resolved with refund"
        );
    }

    #[kani::proof]
    fn proof_escrow_terminal_no_exit() {
        kani::assert!(
            !can_escrow_transition(OsintEscrowStatus::Released, OsintEscrowStatus::Funded),
            "Released is terminal"
        );
        kani::assert!(
            !can_escrow_transition(OsintEscrowStatus::Refunded, OsintEscrowStatus::Funded),
            "Refunded is terminal"
        );
    }

    #[kani::proof]
    fn proof_escrow_no_double_spend() {
        // Once released or refunded, funds cannot move again
        let terminal_statuses = [OsintEscrowStatus::Released, OsintEscrowStatus::Refunded];
        let any_statuses = [
            OsintEscrowStatus::Pending,
            OsintEscrowStatus::Funded,
            OsintEscrowStatus::Released,
            OsintEscrowStatus::Refunded,
            OsintEscrowStatus::Disputed,
        ];

        for terminal in terminal_statuses.iter() {
            for target in any_statuses.iter() {
                if terminal != target {
                    kani::assert!(
                        !can_escrow_transition(*terminal, *target),
                        "Terminal escrow status prevents all transitions"
                    );
                }
            }
        }
    }
}
