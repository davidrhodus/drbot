//! Assumption checking for drbot.
//!
//! This crate provides:
//! - Assumption declarations
//! - Assumption tracking
//! - Assumption validation

use std::collections::HashMap;
use std::sync::RwLock;
use thiserror::Error;

/// Assumption error types.
#[derive(Error, Debug, Clone)]
pub enum AssumptionError {
    #[error("Assumption '{name}' violated: {reason}")]
    Violated { name: String, reason: String },

    #[error("Unknown assumption: {0}")]
    Unknown(String),

    #[error("Assumption not yet validated: {0}")]
    NotValidated(String),
}

/// Result type for assumption operations.
pub type Result<T> = std::result::Result<T, AssumptionError>;

/// Assumption state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssumptionState {
    /// Assumption declared but not validated.
    Pending,
    /// Assumption validated as true.
    Valid,
    /// Assumption validated as false.
    Invalid,
}

/// Assumption tracker.
pub struct Assumptions {
    assumptions: RwLock<HashMap<String, AssumptionState>>,
}

impl Assumptions {
    /// Create new assumption tracker.
    pub fn new() -> Self {
        Self {
            assumptions: RwLock::new(HashMap::new()),
        }
    }

    /// Declare assumption.
    pub fn assume(&self, name: impl Into<String>) {
        let mut assumptions = self.assumptions.write().unwrap();
        assumptions.insert(name.into(), AssumptionState::Pending);
    }

    /// Validate assumption as true.
    pub fn validate(&self, name: &str) -> Result<()> {
        let mut assumptions = self.assumptions.write().unwrap();
        if assumptions.contains_key(name) {
            assumptions.insert(name.to_string(), AssumptionState::Valid);
            Ok(())
        } else {
            Err(AssumptionError::Unknown(name.to_string()))
        }
    }

    /// Invalidate assumption.
    pub fn invalidate(&self, name: &str, reason: &str) -> Result<()> {
        let mut assumptions = self.assumptions.write().unwrap();
        if assumptions.contains_key(name) {
            assumptions.insert(name.to_string(), AssumptionState::Invalid);
            Err(AssumptionError::Violated {
                name: name.to_string(),
                reason: reason.to_string(),
            })
        } else {
            Err(AssumptionError::Unknown(name.to_string()))
        }
    }

    /// Check assumption state.
    pub fn check(&self, name: &str) -> Option<AssumptionState> {
        let assumptions = self.assumptions.read().unwrap();
        assumptions.get(name).copied()
    }

    /// Assert all assumptions are valid.
    pub fn assert_all_valid(&self) -> Result<()> {
        let assumptions = self.assumptions.read().unwrap();
        for (name, state) in assumptions.iter() {
            match state {
                AssumptionState::Pending => {
                    return Err(AssumptionError::NotValidated(name.clone()));
                }
                AssumptionState::Invalid => {
                    return Err(AssumptionError::Violated {
                        name: name.clone(),
                        reason: "previously invalidated".to_string(),
                    });
                }
                AssumptionState::Valid => {}
            }
        }
        Ok(())
    }

    /// Get all pending assumptions.
    pub fn pending(&self) -> Vec<String> {
        let assumptions = self.assumptions.read().unwrap();
        assumptions
            .iter()
            .filter_map(|(name, state)| {
                if *state == AssumptionState::Pending {
                    Some(name.clone())
                } else {
                    None
                }
            })
            .collect()
    }

    /// Clear all assumptions.
    pub fn clear(&self) {
        let mut assumptions = self.assumptions.write().unwrap();
        assumptions.clear();
    }
}

impl Default for Assumptions {
    fn default() -> Self {
        Self::new()
    }
}

/// Assume condition is true.
#[inline]
pub fn assume(condition: bool, name: &str) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(AssumptionError::Violated {
            name: name.to_string(),
            reason: "condition is false".to_string(),
        })
    }
}

/// Assume with reason on failure.
#[inline]
pub fn assume_with_reason(condition: bool, name: &str, reason: &str) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(AssumptionError::Violated {
            name: name.to_string(),
            reason: reason.to_string(),
        })
    }
}

/// Debug-only assumption.
#[cfg(debug_assertions)]
#[track_caller]
pub fn debug_assume(condition: bool, name: &str) {
    if !condition {
        panic!("Debug assumption '{}' violated", name);
    }
}

#[cfg(not(debug_assertions))]
#[inline]
pub fn debug_assume(_condition: bool, _name: &str) {}

/// Unsafe assumption hint (for optimization).
///
/// # Safety
/// The caller must ensure the condition is true.
#[inline]
pub unsafe fn assume_unchecked(condition: bool) {
    if !condition {
        std::hint::unreachable_unchecked();
    }
}

/// Assumption guard that validates on drop.
pub struct AssumptionGuard<'a> {
    assumptions: &'a Assumptions,
    name: String,
    validated: bool,
}

impl<'a> AssumptionGuard<'a> {
    /// Create new assumption guard.
    pub fn new(assumptions: &'a Assumptions, name: impl Into<String>) -> Self {
        let name = name.into();
        assumptions.assume(&name);
        Self {
            assumptions,
            name,
            validated: false,
        }
    }

    /// Validate the assumption.
    pub fn validate(mut self) {
        let _ = self.assumptions.validate(&self.name);
        self.validated = true;
    }

    /// Invalidate the assumption.
    pub fn invalidate(mut self, reason: &str) {
        let _ = self.assumptions.invalidate(&self.name, reason);
        self.validated = true;
    }
}

impl<'a> Drop for AssumptionGuard<'a> {
    fn drop(&mut self) {
        if !self.validated {
            // Assumption not explicitly validated, mark as invalid
            let _ = self
                .assumptions
                .invalidate(&self.name, "guard dropped without validation");
        }
    }
}

/// Create assumption guard.
pub fn guarded_assume<'a>(
    assumptions: &'a Assumptions,
    name: impl Into<String>,
) -> AssumptionGuard<'a> {
    AssumptionGuard::new(assumptions, name)
}

/// Assumption builder for complex conditions.
pub struct AssumeBuilder {
    conditions: Vec<(String, bool)>,
}

impl AssumeBuilder {
    /// Create new assumption builder.
    pub fn new() -> Self {
        Self {
            conditions: Vec::new(),
        }
    }

    /// Add assumption.
    pub fn that(mut self, name: impl Into<String>, condition: bool) -> Self {
        self.conditions.push((name.into(), condition));
        self
    }

    /// Verify all assumptions.
    pub fn verify(self) -> Result<()> {
        for (name, condition) in self.conditions {
            if !condition {
                return Err(AssumptionError::Violated {
                    name,
                    reason: "condition is false".to_string(),
                });
            }
        }
        Ok(())
    }

    /// Get all violated assumptions.
    pub fn violations(self) -> Vec<String> {
        self.conditions
            .into_iter()
            .filter_map(
                |(name, condition)| {
                    if !condition {
                        Some(name)
                    } else {
                        None
                    }
                },
            )
            .collect()
    }
}

impl Default for AssumeBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Start building assumptions.
pub fn assuming() -> AssumeBuilder {
    AssumeBuilder::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_assume() {
        assert!(assume(true, "test").is_ok());
        assert!(assume(false, "test").is_err());
    }

    #[test]
    fn test_assumptions_tracker() {
        let tracker = Assumptions::new();

        tracker.assume("user_authenticated");
        assert_eq!(
            tracker.check("user_authenticated"),
            Some(AssumptionState::Pending)
        );

        tracker.validate("user_authenticated").unwrap();
        assert_eq!(
            tracker.check("user_authenticated"),
            Some(AssumptionState::Valid)
        );
    }

    #[test]
    fn test_assumption_guard() {
        let tracker = Assumptions::new();

        {
            let guard = guarded_assume(&tracker, "test_assumption");
            guard.validate();
        }

        assert_eq!(
            tracker.check("test_assumption"),
            Some(AssumptionState::Valid)
        );
    }

    #[test]
    fn test_assume_builder() {
        let result = assuming()
            .that("positive", 5 > 0)
            .that("bounded", 5 < 10)
            .verify();
        assert!(result.is_ok());

        let result = assuming().that("impossible", false).verify();
        assert!(result.is_err());
    }

    #[test]
    fn test_violations() {
        let violations = assuming()
            .that("valid", true)
            .that("invalid1", false)
            .that("invalid2", false)
            .violations();

        assert_eq!(violations, vec!["invalid1", "invalid2"]);
    }
}

// ============================================================================
// Kani Formal Verification Proofs
// ============================================================================

#[cfg(kani)]
mod kani_proofs {
    use super::*;

    // ========================================================================
    // AssumptionState Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_assumption_state_pending_distinct() {
        let pending = AssumptionState::Pending;
        let valid = AssumptionState::Valid;
        let invalid = AssumptionState::Invalid;

        kani::assert(pending != valid, "Pending must differ from Valid");
        kani::assert(pending != invalid, "Pending must differ from Invalid");
        kani::assert(valid != invalid, "Valid must differ from Invalid");
    }

    #[kani::proof]
    fn proof_assumption_state_equality_reflexive() {
        let pending = AssumptionState::Pending;
        let valid = AssumptionState::Valid;
        let invalid = AssumptionState::Invalid;

        kani::assert(pending == pending, "Pending must equal itself");
        kani::assert(valid == valid, "Valid must equal itself");
        kani::assert(invalid == invalid, "Invalid must equal itself");
    }

    // ========================================================================
    // assume() Function Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_assume_true_ok() {
        let result = assume(true, "test");
        kani::assert(result.is_ok(), "true condition must succeed");
    }

    #[kani::proof]
    fn proof_assume_false_err() {
        let result = assume(false, "test");
        kani::assert(result.is_err(), "false condition must fail");
    }

    // ========================================================================
    // assume_with_reason() Function Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_assume_with_reason_true_ok() {
        let result = assume_with_reason(true, "test", "reason");
        kani::assert(result.is_ok(), "true condition must succeed");
    }

    #[kani::proof]
    fn proof_assume_with_reason_false_err() {
        let result = assume_with_reason(false, "test", "reason");
        kani::assert(result.is_err(), "false condition must fail");
    }

    // ========================================================================
    // Assumptions Tracker Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_assumptions_new_empty() {
        let tracker = Assumptions::new();
        kani::assert(
            tracker.check("nonexistent").is_none(),
            "new tracker must be empty",
        );
    }

    #[kani::proof]
    fn proof_assumptions_default_empty() {
        let tracker = Assumptions::default();
        kani::assert(
            tracker.check("nonexistent").is_none(),
            "default tracker must be empty",
        );
    }

    #[kani::proof]
    fn proof_assumptions_assume_sets_pending() {
        let tracker = Assumptions::new();
        tracker.assume("test");

        let state = tracker.check("test");
        kani::assert(
            state == Some(AssumptionState::Pending),
            "assume must set Pending",
        );
    }

    #[kani::proof]
    fn proof_assumptions_validate_sets_valid() {
        let tracker = Assumptions::new();
        tracker.assume("test");
        let result = tracker.validate("test");

        kani::assert(result.is_ok(), "validate must succeed for existing");
        kani::assert(
            tracker.check("test") == Some(AssumptionState::Valid),
            "must be Valid",
        );
    }

    #[kani::proof]
    fn proof_assumptions_validate_unknown_err() {
        let tracker = Assumptions::new();
        let result = tracker.validate("nonexistent");

        kani::assert(result.is_err(), "validate unknown must fail");
    }

    #[kani::proof]
    fn proof_assumptions_invalidate_sets_invalid() {
        let tracker = Assumptions::new();
        tracker.assume("test");
        let _ = tracker.invalidate("test", "reason");

        kani::assert(
            tracker.check("test") == Some(AssumptionState::Invalid),
            "must be Invalid",
        );
    }

    #[kani::proof]
    fn proof_assumptions_invalidate_unknown_err() {
        let tracker = Assumptions::new();
        let result = tracker.invalidate("nonexistent", "reason");

        kani::assert(result.is_err(), "invalidate unknown must fail");
    }

    #[kani::proof]
    fn proof_assumptions_clear_removes_all() {
        let tracker = Assumptions::new();
        tracker.assume("test1");
        tracker.assume("test2");
        tracker.clear();

        kani::assert(tracker.check("test1").is_none(), "test1 must be removed");
        kani::assert(tracker.check("test2").is_none(), "test2 must be removed");
    }

    #[kani::proof]
    fn proof_assumptions_assert_all_valid_empty_ok() {
        let tracker = Assumptions::new();
        let result = tracker.assert_all_valid();
        kani::assert(result.is_ok(), "empty tracker must pass assert_all_valid");
    }

    #[kani::proof]
    fn proof_assumptions_assert_all_valid_pending_err() {
        let tracker = Assumptions::new();
        tracker.assume("test");
        let result = tracker.assert_all_valid();
        kani::assert(
            result.is_err(),
            "pending assumption must fail assert_all_valid",
        );
    }

    #[kani::proof]
    fn proof_assumptions_assert_all_valid_all_valid_ok() {
        let tracker = Assumptions::new();
        tracker.assume("test");
        let _ = tracker.validate("test");
        let result = tracker.assert_all_valid();
        kani::assert(result.is_ok(), "all valid must pass assert_all_valid");
    }

    #[kani::proof]
    fn proof_assumptions_assert_all_valid_invalid_err() {
        let tracker = Assumptions::new();
        tracker.assume("test");
        let _ = tracker.invalidate("test", "reason");
        let result = tracker.assert_all_valid();
        kani::assert(
            result.is_err(),
            "invalid assumption must fail assert_all_valid",
        );
    }

    // ========================================================================
    // AssumeBuilder Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_assume_builder_new_empty() {
        let builder = AssumeBuilder::new();
        let result = builder.verify();
        kani::assert(result.is_ok(), "empty builder must verify ok");
    }

    #[kani::proof]
    fn proof_assume_builder_default_empty() {
        let builder = AssumeBuilder::default();
        let result = builder.verify();
        kani::assert(result.is_ok(), "default builder must verify ok");
    }

    #[kani::proof]
    fn proof_assume_builder_that_true_ok() {
        let result = assuming().that("test", true).verify();
        kani::assert(result.is_ok(), "true condition must verify ok");
    }

    #[kani::proof]
    fn proof_assume_builder_that_false_err() {
        let result = assuming().that("test", false).verify();
        kani::assert(result.is_err(), "false condition must fail verify");
    }

    #[kani::proof]
    fn proof_assume_builder_all_true_ok() {
        let result = assuming()
            .that("a", true)
            .that("b", true)
            .that("c", true)
            .verify();
        kani::assert(result.is_ok(), "all true must verify ok");
    }

    #[kani::proof]
    fn proof_assume_builder_any_false_err() {
        let result = assuming()
            .that("a", true)
            .that("b", false)
            .that("c", true)
            .verify();
        kani::assert(result.is_err(), "any false must fail verify");
    }

    #[kani::proof]
    fn proof_assume_builder_violations_empty_when_all_true() {
        let violations = assuming().that("a", true).that("b", true).violations();
        kani::assert(violations.is_empty(), "no violations when all true");
    }

    #[kani::proof]
    fn proof_assume_builder_violations_count() {
        let violations = assuming()
            .that("a", true)
            .that("b", false)
            .that("c", false)
            .violations();
        kani::assert(violations.len() == 2, "must have 2 violations");
    }

    // ========================================================================
    // assuming() Function Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_assuming_returns_builder() {
        let builder = assuming();
        let result = builder.verify();
        kani::assert(result.is_ok(), "assuming must return valid builder");
    }
}
