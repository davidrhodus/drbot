//! Heuristic scoring for validators.

use super::types::ValidatorIntel;

/// Weights used for the default heuristic validator score.
#[derive(Debug, Clone, Copy)]
pub struct ScoreWeights {
    /// Starting score.
    pub base: f64,
    /// Penalty applied when delinquent.
    pub delinquent_penalty: f64,
    /// Per-point (percentage) commission penalty.
    pub commission_penalty_per_point: f64,
    /// Bonus multiplier for `ln(activated_stake_sol + 1)`.
    pub stake_ln_bonus: f64,
    /// Penalty multiplier for `skip_rate` (0..1).
    pub skip_rate_penalty: f64,
}

impl Default for ScoreWeights {
    fn default() -> Self {
        Self {
            base: 100.0,
            delinquent_penalty: 60.0,
            commission_penalty_per_point: 1.5,
            stake_ln_bonus: 2.0,
            skip_rate_penalty: 40.0,
        }
    }
}

/// Compute a heuristic validator score (0-100).
pub fn score_validator(intel: &ValidatorIntel) -> f64 {
    score_validator_with(intel, ScoreWeights::default())
}

/// Compute a heuristic validator score (0-100) with custom weights.
pub fn score_validator_with(intel: &ValidatorIntel, weights: ScoreWeights) -> f64 {
    let Some(vote) = intel.vote.as_ref() else {
        return 0.0;
    };

    let mut score = weights.base;

    if vote.delinquent {
        score -= weights.delinquent_penalty;
    }

    score -= (vote.commission as f64) * weights.commission_penalty_per_point;
    score += (vote.activated_stake_sol + 1.0).ln() * weights.stake_ln_bonus;

    if let Some(perf) = intel.performance.as_ref() {
        score -= perf.skip_rate * weights.skip_rate_penalty;
    }

    score.clamp(0.0, 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validator_intel::types::{ValidatorIntel, ValidatorVoteInfo};
    use solana_sdk::{native_token::LAMPORTS_PER_SOL, pubkey::Pubkey};

    #[test]
    fn test_score_validator_basics() {
        let identity = Pubkey::new_unique();
        let mut intel = ValidatorIntel::new(identity);
        let vote = ValidatorVoteInfo {
            vote_pubkey: Pubkey::new_unique(),
            node_pubkey: identity,
            activated_stake_lamports: 100 * LAMPORTS_PER_SOL,
            activated_stake_sol: 100.0,
            commission: 10,
            epoch_vote_account: true,
            epoch_credits: vec![],
            last_vote: 0,
            root_slot: 0,
            delinquent: false,
        };
        intel.vote = Some(vote);

        let score = score_validator(&intel);
        assert!(score > 0.0);
        assert!(score <= 100.0);
    }

    #[test]
    fn test_score_delinquent_is_worse() {
        let identity = Pubkey::new_unique();

        let base_vote = ValidatorVoteInfo {
            vote_pubkey: Pubkey::new_unique(),
            node_pubkey: identity,
            activated_stake_lamports: 10 * LAMPORTS_PER_SOL,
            activated_stake_sol: 10.0,
            commission: 5,
            epoch_vote_account: true,
            epoch_credits: vec![],
            last_vote: 0,
            root_slot: 0,
            delinquent: false,
        };

        let mut ok = ValidatorIntel::new(identity);
        ok.vote = Some(base_vote.clone());

        let mut bad = ValidatorIntel::new(identity);
        let mut bad_vote = base_vote;
        bad_vote.delinquent = true;
        bad.vote = Some(bad_vote);

        assert!(score_validator(&bad) < score_validator(&ok));
    }
}
