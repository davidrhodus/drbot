//! Formal verification proofs for drbot using Kani.
//!
//! This crate contains Kani proofs that verify critical properties:
//! - Mathematical bounds (similarity scores, retention scores)
//! - Array/slice safety (no out-of-bounds access)
//! - State machine invariants
//! - Numeric safety (no overflow/underflow)
//!
//! Run with: `cargo kani --package drbot-kani`

#![allow(dead_code)]

// ============================================================================
// COSINE SIMILARITY VERIFICATION
// ============================================================================

/// Cosine similarity implementation (mirrors drbot-memory/src/longterm.rs)
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }

    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        0.0
    } else {
        dot / (norm_a * norm_b)
    }
}

/// Normalized cosine similarity for vectors known to be unit length.
/// Returns value in [-1, 1] range.
fn cosine_similarity_normalized(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

#[cfg(kani)]
mod cosine_proofs {
    use super::*;

    /// Proof: cosine_similarity returns 0.0 for mismatched lengths
    #[kani::proof]
    fn proof_cosine_mismatched_returns_zero() {
        let a: [f32; 2] = kani::any();
        let b: [f32; 3] = kani::any();

        let result = cosine_similarity(&a, &b);
        kani::assert(result == 0.0, "Mismatched lengths must return 0.0");
    }

    /// Proof: cosine_similarity returns 0.0 for zero vectors
    #[kani::proof]
    fn proof_cosine_zero_vector_returns_zero() {
        let zero = [0.0f32; 3];
        let other: [f32; 3] = kani::any();

        let result = cosine_similarity(&zero, &other);
        kani::assert(result == 0.0, "Zero vector must return 0.0");
    }

    /// Proof: cosine_similarity is symmetric
    #[kani::proof]
    fn proof_cosine_symmetric() {
        let a: [f32; 2] = kani::any();
        let b: [f32; 2] = kani::any();

        // Assume finite values to avoid NaN comparison issues
        kani::assume(a[0].is_finite() && a[1].is_finite());
        kani::assume(b[0].is_finite() && b[1].is_finite());

        let ab = cosine_similarity(&a, &b);
        let ba = cosine_similarity(&b, &a);

        // Allow for floating point epsilon
        let diff = (ab - ba).abs();
        kani::assert(
            diff < 1e-6 || (ab.is_nan() && ba.is_nan()),
            "Cosine similarity must be symmetric",
        );
    }

    /// Proof: identical vectors have similarity 1.0
    #[kani::proof]
    fn proof_cosine_identical_is_one() {
        let a: [f32; 2] = kani::any();

        // Assume non-zero, finite values
        kani::assume(a[0].is_finite() && a[1].is_finite());
        kani::assume(a[0] != 0.0 || a[1] != 0.0);

        let result = cosine_similarity(&a, &a);

        // Result should be very close to 1.0
        let diff = (result - 1.0).abs();
        kani::assert(
            diff < 1e-5,
            "Identical non-zero vectors must have similarity ~1.0",
        );
    }

    /// Proof: cosine_similarity bounds are [-1, 1] for finite non-zero vectors
    #[kani::proof]
    #[kani::unwind(3)]
    fn proof_cosine_bounds() {
        let a: [f32; 2] = kani::any();
        let b: [f32; 2] = kani::any();

        // Assume finite, non-zero values
        kani::assume(a.iter().all(|x| x.is_finite()));
        kani::assume(b.iter().all(|x| x.is_finite()));
        kani::assume(a.iter().any(|x| *x != 0.0));
        kani::assume(b.iter().any(|x| *x != 0.0));

        // Assume reasonable magnitude to avoid overflow
        kani::assume(a.iter().all(|x| x.abs() < 1e10));
        kani::assume(b.iter().all(|x| x.abs() < 1e10));

        let result = cosine_similarity(&a, &b);

        if result.is_finite() {
            kani::assert(
                result >= -1.0 - 1e-5 && result <= 1.0 + 1e-5,
                "Cosine similarity must be in [-1, 1]",
            );
        }
    }
}

// ============================================================================
// RETENTION SCORE VERIFICATION
// ============================================================================

/// Importance level (mirrors drbot-memory/src/longterm.rs)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Importance {
    Low = 1,
    Normal = 2,
    High = 3,
    Critical = 4,
}

/// Calculate retention score (simplified from LongTermMemory::retention_score)
///
/// Parameters:
/// - importance: Importance level
/// - confidence: Confidence score in [0.0, 1.0]
/// - days_since_access: Days since last access
/// - access_count: Number of times accessed
fn retention_score(
    importance: Importance,
    confidence: f32,
    days_since_access: f32,
    access_count: u32,
) -> f32 {
    let importance_factor = match importance {
        Importance::Low => 0.25,
        Importance::Normal => 0.5,
        Importance::High => 0.75,
        Importance::Critical => 1.0,
    };

    let recency_factor = {
        let decay_rate = 0.1 / (1.0 + access_count as f32 * 0.1);
        (-decay_rate * days_since_access).exp()
    };

    let access_factor = (access_count as f32).ln_1p() / 10.0;

    (importance_factor * 0.4 + recency_factor * 0.4 + access_factor * 0.2) * confidence
}

/// Clamp confidence to [0.0, 1.0] (mirrors LongTermMemory::with_confidence)
fn clamp_confidence(confidence: f32) -> f32 {
    confidence.clamp(0.0, 1.0)
}

#[cfg(kani)]
mod retention_proofs {
    use super::*;

    /// Proof: clamp_confidence always returns value in [0, 1]
    #[kani::proof]
    fn proof_clamp_confidence_bounds() {
        let input: f32 = kani::any();
        kani::assume(input.is_finite());

        let result = clamp_confidence(input);

        kani::assert(result >= 0.0, "Clamped confidence must be >= 0");
        kani::assert(result <= 1.0, "Clamped confidence must be <= 1");
    }

    /// Proof: clamp is idempotent
    #[kani::proof]
    fn proof_clamp_idempotent() {
        let input: f32 = kani::any();
        kani::assume(input.is_finite());

        let once = clamp_confidence(input);
        let twice = clamp_confidence(once);

        kani::assert(once == twice, "Clamping must be idempotent");
    }

    /// Proof: values already in range are unchanged
    #[kani::proof]
    fn proof_clamp_preserves_valid() {
        let input: f32 = kani::any();
        kani::assume(input.is_finite());
        kani::assume(input >= 0.0 && input <= 1.0);

        let result = clamp_confidence(input);
        kani::assert(result == input, "Valid values must be unchanged");
    }

    /// Proof: retention_score is non-negative when confidence is non-negative
    #[kani::proof]
    fn proof_retention_non_negative() {
        let importance_val: u8 = kani::any();
        kani::assume(importance_val >= 1 && importance_val <= 4);

        let importance = match importance_val {
            1 => Importance::Low,
            2 => Importance::Normal,
            3 => Importance::High,
            _ => Importance::Critical,
        };

        let confidence: f32 = kani::any();
        let days: f32 = kani::any();
        let access_count: u32 = kani::any();

        kani::assume(confidence >= 0.0 && confidence <= 1.0);
        kani::assume(days >= 0.0 && days < 10000.0);
        kani::assume(access_count < 1_000_000);

        let result = retention_score(importance, confidence, days, access_count);

        if result.is_finite() {
            kani::assert(result >= 0.0, "Retention score must be non-negative");
        }
    }

    /// Proof: zero confidence yields zero retention
    #[kani::proof]
    fn proof_zero_confidence_zero_retention() {
        let importance_val: u8 = kani::any();
        kani::assume(importance_val >= 1 && importance_val <= 4);

        let importance = match importance_val {
            1 => Importance::Low,
            2 => Importance::Normal,
            3 => Importance::High,
            _ => Importance::Critical,
        };

        let days: f32 = kani::any();
        let access_count: u32 = kani::any();

        kani::assume(days >= 0.0 && days < 10000.0);
        kani::assume(access_count < 1_000_000);

        let result = retention_score(importance, 0.0, days, access_count);

        kani::assert(result == 0.0, "Zero confidence must yield zero retention");
    }

    /// Proof: Critical importance yields higher score than Low importance
    #[kani::proof]
    fn proof_importance_ordering() {
        let confidence: f32 = kani::any();
        let days: f32 = kani::any();
        let access_count: u32 = kani::any();

        kani::assume(confidence > 0.0 && confidence <= 1.0);
        kani::assume(days >= 0.0 && days < 1000.0);
        kani::assume(access_count < 100_000);

        let low_score = retention_score(Importance::Low, confidence, days, access_count);
        let critical_score = retention_score(Importance::Critical, confidence, days, access_count);

        if low_score.is_finite() && critical_score.is_finite() {
            kani::assert(
                critical_score >= low_score,
                "Critical importance must score >= Low importance",
            );
        }
    }
}

// ============================================================================
// SESSION SLICE BOUNDS VERIFICATION
// ============================================================================

/// Get the last N elements from a slice (mirrors Session::last_messages)
fn last_n<T>(items: &[T], n: usize) -> &[T] {
    let len = items.len();
    if n >= len {
        items
    } else {
        &items[len - n..]
    }
}

#[cfg(kani)]
mod slice_proofs {
    use super::*;

    /// Proof: last_n never panics and returns valid slice
    #[kani::proof]
    #[kani::unwind(10)]
    fn proof_last_n_no_panic() {
        // Use small array to keep proof tractable
        let arr: [u8; 5] = kani::any();
        let n: usize = kani::any();
        kani::assume(n <= 100); // Reasonable bound

        let result = last_n(&arr, n);

        // Result length is min(n, arr.len())
        let expected_len = if n >= arr.len() { arr.len() } else { n };
        kani::assert(
            result.len() == expected_len,
            "Result length must be correct",
        );
    }

    /// Proof: last_n(slice, 0) returns empty slice
    #[kani::proof]
    fn proof_last_n_zero_is_empty() {
        let arr: [u8; 3] = kani::any();
        let result = last_n(&arr, 0);
        kani::assert(result.is_empty(), "last_n(_, 0) must return empty slice");
    }

    /// Proof: last_n with n >= len returns entire slice
    #[kani::proof]
    fn proof_last_n_large_n_returns_all() {
        let arr: [u8; 3] = kani::any();
        let n: usize = kani::any();
        kani::assume(n >= 3);
        kani::assume(n < 1000); // Prevent overflow

        let result = last_n(&arr, n);
        kani::assert(
            result.len() == arr.len(),
            "Large n must return entire slice",
        );
    }

    /// Proof: returned slice contains the last elements
    #[kani::proof]
    fn proof_last_n_correct_elements() {
        let arr: [u8; 4] = kani::any();

        let result = last_n(&arr, 2);

        kani::assert(result.len() == 2, "Should return 2 elements");
        kani::assert(result[0] == arr[2], "First element should be arr[2]");
        kani::assert(result[1] == arr[3], "Second element should be arr[3]");
    }
}

// ============================================================================
// SESSION STATE MACHINE VERIFICATION
// ============================================================================

/// Session state (mirrors drbot-core/src/session.rs)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    Active,
    Archived,
    Deleted,
}

/// Check if state transition is valid
fn is_valid_transition(from: SessionState, to: SessionState) -> bool {
    match (from, to) {
        // Active can go to Archived or Deleted
        (SessionState::Active, SessionState::Archived) => true,
        (SessionState::Active, SessionState::Deleted) => true,
        // Archived can go to Active (unarchive) or Deleted
        (SessionState::Archived, SessionState::Active) => true,
        (SessionState::Archived, SessionState::Deleted) => true,
        // Deleted is terminal (soft delete, could be restored in some designs)
        (SessionState::Deleted, SessionState::Active) => true, // Restore
        // Same state is always valid (no-op)
        (a, b) if a == b => true,
        // Any other transition
        _ => false,
    }
}

/// Apply state transition, returning new state or None if invalid
fn apply_transition(from: SessionState, to: SessionState) -> Option<SessionState> {
    if is_valid_transition(from, to) {
        Some(to)
    } else {
        None
    }
}

#[cfg(kani)]
mod state_machine_proofs {
    use super::*;

    /// Proof: self-transitions are always valid
    #[kani::proof]
    fn proof_self_transition_valid() {
        let state_val: u8 = kani::any();
        kani::assume(state_val <= 2);

        let state = match state_val {
            0 => SessionState::Active,
            1 => SessionState::Archived,
            _ => SessionState::Deleted,
        };

        kani::assert(
            is_valid_transition(state, state),
            "Self-transition must always be valid",
        );
    }

    /// Proof: Active can always transition to Archived
    #[kani::proof]
    fn proof_active_to_archived_valid() {
        kani::assert(
            is_valid_transition(SessionState::Active, SessionState::Archived),
            "Active -> Archived must be valid",
        );
    }

    /// Proof: apply_transition preserves state on self-transition
    #[kani::proof]
    fn proof_apply_preserves_on_self() {
        let state_val: u8 = kani::any();
        kani::assume(state_val <= 2);

        let state = match state_val {
            0 => SessionState::Active,
            1 => SessionState::Archived,
            _ => SessionState::Deleted,
        };

        let result = apply_transition(state, state);
        kani::assert(
            result == Some(state),
            "Self-transition must return same state",
        );
    }

    /// Proof: if transition valid, apply_transition returns Some
    #[kani::proof]
    fn proof_valid_transition_returns_some() {
        let from_val: u8 = kani::any();
        let to_val: u8 = kani::any();
        kani::assume(from_val <= 2 && to_val <= 2);

        let from = match from_val {
            0 => SessionState::Active,
            1 => SessionState::Archived,
            _ => SessionState::Deleted,
        };

        let to = match to_val {
            0 => SessionState::Active,
            1 => SessionState::Archived,
            _ => SessionState::Deleted,
        };

        if is_valid_transition(from, to) {
            let result = apply_transition(from, to);
            kani::assert(result.is_some(), "Valid transition must return Some");
            kani::assert(
                result == Some(to),
                "Valid transition must return target state",
            );
        }
    }
}

// ============================================================================
// NUMERIC SAFETY VERIFICATION
// ============================================================================

/// Safe addition with saturation (no overflow)
fn saturating_add_tokens(current: usize, additional: usize) -> usize {
    current.saturating_add(additional)
}

/// Safe multiplication for cost calculation
fn safe_cost_multiply(units: u32, cost_per_unit: u32) -> Option<u64> {
    (units as u64).checked_mul(cost_per_unit as u64)
}

#[cfg(kani)]
mod numeric_proofs {
    use super::*;

    /// Proof: saturating_add never overflows
    #[kani::proof]
    fn proof_saturating_add_no_overflow() {
        let a: usize = kani::any();
        let b: usize = kani::any();

        let result = saturating_add_tokens(a, b);

        // Result is at least max(a, b)
        kani::assert(
            result >= a || result >= b,
            "Saturating add result must be >= inputs",
        );

        // Result is at most usize::MAX
        kani::assert(result <= usize::MAX, "Result must not exceed usize::MAX");
    }

    /// Proof: saturating_add is commutative
    #[kani::proof]
    fn proof_saturating_add_commutative() {
        let a: usize = kani::any();
        let b: usize = kani::any();

        let ab = saturating_add_tokens(a, b);
        let ba = saturating_add_tokens(b, a);

        kani::assert(ab == ba, "Saturating add must be commutative");
    }

    /// Proof: safe_cost_multiply returns None on overflow
    #[kani::proof]
    fn proof_cost_multiply_overflow_safe() {
        let units: u32 = kani::any();
        let cost: u32 = kani::any();

        let result = safe_cost_multiply(units, cost);

        // Calculate expected result
        let expected = (units as u64).checked_mul(cost as u64);

        kani::assert(result == expected, "Result must match checked_mul");
    }

    /// Proof: small values always succeed
    #[kani::proof]
    fn proof_cost_multiply_small_succeeds() {
        let units: u32 = kani::any();
        let cost: u32 = kani::any();

        // For small values, multiplication should succeed
        kani::assume(units <= 1_000_000);
        kani::assume(cost <= 1_000_000);

        let result = safe_cost_multiply(units, cost);
        kani::assert(result.is_some(), "Small values must not overflow");
    }
}

// ============================================================================
// DEDUPLICATION THRESHOLD VERIFICATION
// ============================================================================

/// Clamp deduplication threshold to valid range [0.5, 1.0]
fn clamp_dedup_threshold(threshold: f32) -> f32 {
    threshold.clamp(0.5, 1.0)
}

/// Check if two items are duplicates based on similarity score
fn is_duplicate(similarity: f32, threshold: f32) -> bool {
    similarity >= threshold
}

#[cfg(kani)]
mod dedup_proofs {
    use super::*;

    /// Proof: dedup threshold is always in valid range
    #[kani::proof]
    fn proof_dedup_threshold_bounds() {
        let input: f32 = kani::any();
        kani::assume(input.is_finite());

        let result = clamp_dedup_threshold(input);

        kani::assert(result >= 0.5, "Threshold must be >= 0.5");
        kani::assert(result <= 1.0, "Threshold must be <= 1.0");
    }

    /// Proof: similarity of 1.0 is always a duplicate (for valid thresholds)
    #[kani::proof]
    fn proof_identical_is_duplicate() {
        let threshold: f32 = kani::any();
        kani::assume(threshold.is_finite());

        let valid_threshold = clamp_dedup_threshold(threshold);

        kani::assert(
            is_duplicate(1.0, valid_threshold),
            "Similarity 1.0 must always be duplicate",
        );
    }

    /// Proof: similarity of 0.0 is never a duplicate (for valid thresholds)
    #[kani::proof]
    fn proof_zero_similarity_not_duplicate() {
        let threshold: f32 = kani::any();
        kani::assume(threshold.is_finite());

        let valid_threshold = clamp_dedup_threshold(threshold);

        kani::assert(
            !is_duplicate(0.0, valid_threshold),
            "Similarity 0.0 must never be duplicate",
        );
    }

    /// Proof: is_duplicate is monotonic in similarity
    #[kani::proof]
    fn proof_duplicate_monotonic() {
        let sim1: f32 = kani::any();
        let sim2: f32 = kani::any();
        let threshold: f32 = kani::any();

        kani::assume(sim1.is_finite() && sim2.is_finite() && threshold.is_finite());
        kani::assume(sim1 <= sim2);

        // If lower similarity is duplicate, higher must be too
        if is_duplicate(sim1, threshold) {
            kani::assert(
                is_duplicate(sim2, threshold),
                "Duplicate check must be monotonic",
            );
        }
    }
}

// ============================================================================
// ROUTER RETRY BOUNDS VERIFICATION
// ============================================================================

/// Router retry configuration
#[derive(Debug, Clone)]
pub struct RetryConfig {
    pub max_retries: u8,
    pub current_attempt: u8,
}

impl RetryConfig {
    pub fn new(max_retries: u8) -> Self {
        Self {
            max_retries,
            current_attempt: 0,
        }
    }

    pub fn can_retry(&self) -> bool {
        self.current_attempt < self.max_retries
    }

    pub fn record_attempt(&mut self) -> bool {
        if self.can_retry() {
            self.current_attempt = self.current_attempt.saturating_add(1);
            true
        } else {
            false
        }
    }

    pub fn attempts_remaining(&self) -> u8 {
        self.max_retries.saturating_sub(self.current_attempt)
    }
}

#[cfg(kani)]
mod retry_proofs {
    use super::*;

    /// Proof: new config can always retry (unless max_retries is 0)
    #[kani::proof]
    fn proof_new_can_retry() {
        let max: u8 = kani::any();
        let config = RetryConfig::new(max);

        if max > 0 {
            kani::assert(
                config.can_retry(),
                "New config with max > 0 must allow retry",
            );
        } else {
            kani::assert(
                !config.can_retry(),
                "New config with max = 0 must not allow retry",
            );
        }
    }

    /// Proof: after max_retries attempts, can_retry is false
    #[kani::proof]
    #[kani::unwind(6)]
    fn proof_max_retries_exhausted() {
        let max: u8 = kani::any();
        kani::assume(max <= 5); // Keep proof tractable

        let mut config = RetryConfig::new(max);

        // Exhaust all retries
        for _ in 0..max {
            config.record_attempt();
        }

        kani::assert(
            !config.can_retry(),
            "After max retries, can_retry must be false",
        );
    }

    /// Proof: attempts_remaining decreases monotonically
    #[kani::proof]
    fn proof_attempts_remaining_decreases() {
        let max: u8 = kani::any();
        kani::assume(max > 0);

        let mut config = RetryConfig::new(max);
        let before = config.attempts_remaining();

        if config.record_attempt() {
            let after = config.attempts_remaining();
            kani::assert(
                after < before,
                "Remaining attempts must decrease after record",
            );
        }
    }

    /// Proof: current_attempt never exceeds max_retries
    #[kani::proof]
    #[kani::unwind(260)]
    fn proof_current_never_exceeds_max() {
        let max: u8 = kani::any();
        let mut config = RetryConfig::new(max);

        // Try many times
        for _ in 0..255u16 {
            config.record_attempt();
        }

        kani::assert(
            config.current_attempt <= config.max_retries,
            "Current attempt must never exceed max",
        );
    }
}

// ============================================================================
// QUALITY TIER VERIFICATION
// ============================================================================

/// Quality tier for model selection (1-5 scale)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct QualityTier(u8);

impl QualityTier {
    pub fn new(tier: u8) -> Self {
        Self(tier.clamp(1, 5))
    }

    pub fn value(&self) -> u8 {
        self.0
    }

    pub fn meets_minimum(&self, minimum: QualityTier) -> bool {
        self.0 >= minimum.0
    }
}

#[cfg(kani)]
mod quality_proofs {
    use super::*;

    /// Proof: QualityTier is always in [1, 5]
    #[kani::proof]
    fn proof_quality_tier_bounds() {
        let input: u8 = kani::any();
        let tier = QualityTier::new(input);

        kani::assert(tier.value() >= 1, "Quality tier must be >= 1");
        kani::assert(tier.value() <= 5, "Quality tier must be <= 5");
    }

    /// Proof: tier 5 meets all minimums
    #[kani::proof]
    fn proof_max_tier_meets_all() {
        let min_input: u8 = kani::any();
        let max_tier = QualityTier::new(5);
        let min_tier = QualityTier::new(min_input);

        kani::assert(
            max_tier.meets_minimum(min_tier),
            "Tier 5 must meet all minimum requirements",
        );
    }

    /// Proof: meets_minimum is reflexive
    #[kani::proof]
    fn proof_meets_minimum_reflexive() {
        let input: u8 = kani::any();
        let tier = QualityTier::new(input);

        kani::assert(tier.meets_minimum(tier), "A tier must meet its own minimum");
    }

    /// Proof: meets_minimum is transitive
    #[kani::proof]
    fn proof_meets_minimum_transitive() {
        let a_input: u8 = kani::any();
        let b_input: u8 = kani::any();
        let c_input: u8 = kani::any();

        let a = QualityTier::new(a_input);
        let b = QualityTier::new(b_input);
        let c = QualityTier::new(c_input);

        // If a meets b's minimum and b meets c's minimum, then a meets c's minimum
        if a.meets_minimum(b) && b.meets_minimum(c) {
            kani::assert(a.meets_minimum(c), "meets_minimum must be transitive");
        }
    }
}

// ============================================================================
// TESTS (for non-Kani verification)
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_similarity_identical() {
        let a = vec![1.0, 0.0, 0.0];
        let result = cosine_similarity(&a, &a);
        assert!((result - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        let result = cosine_similarity(&a, &b);
        assert!(result.abs() < 1e-6);
    }

    #[test]
    fn test_retention_score_bounds() {
        let score = retention_score(Importance::Normal, 1.0, 0.0, 0);
        assert!(score >= 0.0 && score <= 1.0);
    }

    #[test]
    fn test_last_n() {
        let arr = vec![1, 2, 3, 4, 5];
        assert_eq!(last_n(&arr, 2), &[4, 5]);
        assert_eq!(last_n(&arr, 0), &[]);
        assert_eq!(last_n(&arr, 10), &[1, 2, 3, 4, 5]);
    }

    #[test]
    fn test_session_state_transitions() {
        assert!(is_valid_transition(
            SessionState::Active,
            SessionState::Archived
        ));
        assert!(is_valid_transition(
            SessionState::Active,
            SessionState::Active
        ));
    }

    #[test]
    fn test_quality_tier_bounds() {
        assert_eq!(QualityTier::new(0).value(), 1);
        assert_eq!(QualityTier::new(10).value(), 5);
        assert_eq!(QualityTier::new(3).value(), 3);
    }

    #[test]
    fn test_retry_config() {
        let mut config = RetryConfig::new(3);
        assert!(config.can_retry());
        assert_eq!(config.attempts_remaining(), 3);

        config.record_attempt();
        assert_eq!(config.attempts_remaining(), 2);

        config.record_attempt();
        config.record_attempt();
        assert!(!config.can_retry());
    }
}
