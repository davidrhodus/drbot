//! Finalization utilities for drbot.
//!
//! This crate provides:
//! - Finalization patterns
//! - Finalizer registration
//! - Finalization ordering

use thiserror::Error;

/// Finalize error types.
#[derive(Error, Debug, Clone)]
pub enum FinalizeError {
    #[error("Finalization failed: {0}")]
    Failed(String),

    #[error("Already finalized")]
    AlreadyFinalized,
}

/// Result type for finalize operations.
pub type Result<T> = std::result::Result<T, FinalizeError>;

/// Finalizable trait.
pub trait Finalizable {
    /// Finalize the resource.
    fn finalize(&mut self) -> Result<()>;
}

/// Finalizer callback.
pub type FinalizerFn = Box<dyn FnOnce() + Send>;

/// Finalizer registry.
#[derive(Default)]
pub struct FinalizerRegistry {
    finalizers: Vec<FinalizerFn>,
}

impl FinalizerRegistry {
    /// Create new registry.
    pub fn new() -> Self {
        Self {
            finalizers: Vec::new(),
        }
    }

    /// Register finalizer.
    pub fn register<F: FnOnce() + Send + 'static>(&mut self, f: F) {
        self.finalizers.push(Box::new(f));
    }

    /// Run all finalizers.
    pub fn finalize_all(&mut self) {
        while let Some(f) = self.finalizers.pop() {
            f();
        }
    }

    /// Count pending.
    pub fn pending(&self) -> usize {
        self.finalizers.len()
    }
}

impl Drop for FinalizerRegistry {
    fn drop(&mut self) {
        self.finalize_all();
    }
}

/// Finalization state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinalizationState {
    /// Not started.
    NotStarted,
    /// In progress.
    InProgress,
    /// Completed.
    Completed,
    /// Failed.
    Failed,
}

/// Finalization tracker.
#[derive(Debug)]
pub struct FinalizationTracker {
    state: FinalizationState,
    error: Option<String>,
}

impl FinalizationTracker {
    /// Create new tracker.
    pub fn new() -> Self {
        Self {
            state: FinalizationState::NotStarted,
            error: None,
        }
    }

    /// Get state.
    pub fn state(&self) -> FinalizationState {
        self.state
    }

    /// Start finalization.
    pub fn start(&mut self) -> Result<()> {
        if self.state != FinalizationState::NotStarted {
            return Err(FinalizeError::AlreadyFinalized);
        }
        self.state = FinalizationState::InProgress;
        Ok(())
    }

    /// Complete finalization.
    pub fn complete(&mut self) {
        self.state = FinalizationState::Completed;
    }

    /// Fail finalization.
    pub fn fail(&mut self, error: &str) {
        self.state = FinalizationState::Failed;
        self.error = Some(error.to_string());
    }

    /// Get error.
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Is finalized.
    pub fn is_finalized(&self) -> bool {
        matches!(
            self.state,
            FinalizationState::Completed | FinalizationState::Failed
        )
    }
}

impl Default for FinalizationTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Finalizable wrapper.
pub struct Finalizable_<T, F: FnOnce(&mut T) -> Result<()>> {
    value: T,
    finalizer: Option<F>,
    finalized: bool,
}

impl<T, F: FnOnce(&mut T) -> Result<()>> Finalizable_<T, F> {
    /// Create new.
    pub fn new(value: T, finalizer: F) -> Self {
        Self {
            value,
            finalizer: Some(finalizer),
            finalized: false,
        }
    }

    /// Get reference.
    pub fn get(&self) -> &T {
        &self.value
    }

    /// Get mutable reference.
    pub fn get_mut(&mut self) -> &mut T {
        &mut self.value
    }

    /// Finalize.
    pub fn finalize(&mut self) -> Result<()> {
        if self.finalized {
            return Err(FinalizeError::AlreadyFinalized);
        }
        if let Some(f) = self.finalizer.take() {
            let result = f(&mut self.value);
            self.finalized = true;
            result
        } else {
            Err(FinalizeError::AlreadyFinalized)
        }
    }

    /// Is finalized.
    pub fn is_finalized(&self) -> bool {
        self.finalized
    }
}

impl<T, F: FnOnce(&mut T) -> Result<()>> Drop for Finalizable_<T, F> {
    fn drop(&mut self) {
        if !self.finalized {
            if let Some(f) = self.finalizer.take() {
                let _ = f(&mut self.value);
            }
        }
    }
}

/// Two-phase finalizer.
pub struct TwoPhaseFinalizer<F1: FnOnce(), F2: FnOnce()> {
    prepare: Option<F1>,
    commit: Option<F2>,
    prepared: bool,
}

impl<F1: FnOnce(), F2: FnOnce()> TwoPhaseFinalizer<F1, F2> {
    /// Create new.
    pub fn new(prepare: F1, commit: F2) -> Self {
        Self {
            prepare: Some(prepare),
            commit: Some(commit),
            prepared: false,
        }
    }

    /// Prepare phase.
    pub fn prepare(&mut self) {
        if !self.prepared {
            if let Some(f) = self.prepare.take() {
                f();
            }
            self.prepared = true;
        }
    }

    /// Commit phase.
    pub fn commit(&mut self) {
        if self.prepared {
            if let Some(f) = self.commit.take() {
                f();
            }
        }
    }

    /// Is prepared.
    pub fn is_prepared(&self) -> bool {
        self.prepared
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_finalizer_registry() {
        let mut order = Vec::new();
        {
            let mut registry = FinalizerRegistry::new();
            let ptr = &mut order as *mut Vec<i32>;
            registry.register(move || unsafe { (*ptr).push(1) });
            registry.register(move || unsafe { (*ptr).push(2) });
        }
        // LIFO order
        assert_eq!(order, vec![2, 1]);
    }

    #[test]
    fn test_finalization_tracker() {
        let mut tracker = FinalizationTracker::new();
        assert_eq!(tracker.state(), FinalizationState::NotStarted);

        tracker.start().unwrap();
        assert_eq!(tracker.state(), FinalizationState::InProgress);

        tracker.complete();
        assert!(tracker.is_finalized());
    }

    #[test]
    fn test_finalizable() {
        let mut finalized = false;
        {
            let mut f = Finalizable_::new(42, |_| {
                finalized = true;
                Ok(())
            });
            let _ = f.finalize();
        }
        assert!(finalized);
    }
}
