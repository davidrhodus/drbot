//! Rollback support for drbot.
//!
//! This crate provides:
//! - Rollback stack
//! - Compensating actions
//! - Partial rollback

use thiserror::Error;

/// Rollback error types.
#[derive(Error, Debug, Clone)]
pub enum RollbackError {
    #[error("Rollback failed: {0}")]
    Failed(String),

    #[error("No actions to rollback")]
    Empty,

    #[error("Checkpoint not found: {0}")]
    CheckpointNotFound(String),
}

/// Result type for rollback operations.
pub type Result<T> = std::result::Result<T, RollbackError>;

/// Compensation action.
pub type CompensateAction = Box<dyn FnOnce() + Send>;

/// Rollback stack for managing compensating actions.
pub struct RollbackStack {
    actions: Vec<(Option<String>, CompensateAction)>,
}

impl RollbackStack {
    /// Create new rollback stack.
    pub fn new() -> Self {
        Self {
            actions: Vec::new(),
        }
    }

    /// Push compensating action.
    pub fn push<F>(&mut self, action: F)
    where
        F: FnOnce() + Send + 'static,
    {
        self.actions.push((None, Box::new(action)));
    }

    /// Push action with checkpoint name.
    pub fn checkpoint<F>(&mut self, name: impl Into<String>, action: F)
    where
        F: FnOnce() + Send + 'static,
    {
        self.actions.push((Some(name.into()), Box::new(action)));
    }

    /// Rollback all actions.
    pub fn rollback_all(&mut self) {
        while let Some((_, action)) = self.actions.pop() {
            action();
        }
    }

    /// Rollback to checkpoint.
    pub fn rollback_to(&mut self, checkpoint: &str) -> Result<()> {
        let pos = self
            .actions
            .iter()
            .rposition(|(name, _)| name.as_deref() == Some(checkpoint))
            .ok_or_else(|| RollbackError::CheckpointNotFound(checkpoint.to_string()))?;

        while self.actions.len() > pos + 1 {
            if let Some((_, action)) = self.actions.pop() {
                action();
            }
        }

        Ok(())
    }

    /// Rollback last n actions.
    pub fn rollback_last(&mut self, n: usize) {
        for _ in 0..n {
            if let Some((_, action)) = self.actions.pop() {
                action();
            }
        }
    }

    /// Clear without executing.
    pub fn clear(&mut self) {
        self.actions.clear();
    }

    /// Get number of pending actions.
    pub fn len(&self) -> usize {
        self.actions.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.actions.is_empty()
    }
}

impl Default for RollbackStack {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for RollbackStack {
    fn drop(&mut self) {
        // Rollback all on drop (for error cases)
        self.rollback_all();
    }
}

/// Execute with automatic rollback on failure.
pub fn with_rollback<T, E, F>(mut f: F) -> std::result::Result<T, E>
where
    F: FnMut(&mut RollbackStack) -> std::result::Result<T, E>,
{
    let mut stack = RollbackStack::new();
    match f(&mut stack) {
        Ok(result) => {
            stack.clear(); // Success, don't rollback
            Ok(result)
        }
        Err(e) => {
            stack.rollback_all();
            Err(e)
        }
    }
}

/// Rollback guard that auto-rollbacks unless disarmed.
pub struct RollbackGuard {
    stack: Option<RollbackStack>,
}

impl RollbackGuard {
    /// Create new rollback guard.
    pub fn new() -> Self {
        Self {
            stack: Some(RollbackStack::new()),
        }
    }

    /// Push compensating action.
    pub fn push<F>(&mut self, action: F)
    where
        F: FnOnce() + Send + 'static,
    {
        if let Some(ref mut stack) = self.stack {
            stack.push(action);
        }
    }

    /// Create checkpoint.
    pub fn checkpoint<F>(&mut self, name: impl Into<String>, action: F)
    where
        F: FnOnce() + Send + 'static,
    {
        if let Some(ref mut stack) = self.stack {
            stack.checkpoint(name, action);
        }
    }

    /// Commit (disarm the guard).
    pub fn commit(mut self) {
        if let Some(mut stack) = self.stack.take() {
            stack.clear();
        }
    }

    /// Manual rollback.
    pub fn rollback(&mut self) {
        if let Some(mut stack) = self.stack.take() {
            stack.rollback_all();
        }
    }
}

impl Default for RollbackGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for RollbackGuard {
    fn drop(&mut self) {
        // Auto-rollback if not committed
        if let Some(mut stack) = self.stack.take() {
            stack.rollback_all();
        }
    }
}

/// Compensating action builder.
pub struct CompensateBuilder<T> {
    value: T,
    compensate: Option<CompensateAction>,
}

impl<T> CompensateBuilder<T> {
    /// Create new builder.
    pub fn new(value: T) -> Self {
        Self {
            value,
            compensate: None,
        }
    }

    /// Set compensating action.
    pub fn on_rollback<F>(mut self, action: F) -> Self
    where
        F: FnOnce() + Send + 'static,
    {
        self.compensate = Some(Box::new(action));
        self
    }

    /// Register with rollback stack.
    pub fn register(self, stack: &mut RollbackStack) -> T {
        if let Some(action) = self.compensate {
            stack.push(action);
        }
        self.value
    }
}

/// Create compensating action builder.
pub fn compensate<T>(value: T) -> CompensateBuilder<T> {
    CompensateBuilder::new(value)
}

/// Saga pattern implementation.
pub struct Saga {
    steps: Vec<(Box<dyn FnOnce() -> bool + Send>, CompensateAction)>,
}

impl Saga {
    /// Create new saga.
    pub fn new() -> Self {
        Self { steps: Vec::new() }
    }

    /// Add step with compensation.
    pub fn step<A, C>(mut self, action: A, compensate: C) -> Self
    where
        A: FnOnce() -> bool + Send + 'static,
        C: FnOnce() + Send + 'static,
    {
        self.steps.push((Box::new(action), Box::new(compensate)));
        self
    }

    /// Execute saga.
    pub fn execute(self) -> Result<()> {
        let mut completed = Vec::new();

        for (action, compensate) in self.steps {
            if action() {
                completed.push(compensate);
            } else {
                // Rollback completed steps in reverse order
                for comp in completed.into_iter().rev() {
                    comp();
                }
                return Err(RollbackError::Failed("Saga step failed".to_string()));
            }
        }

        // All steps succeeded, discard compensations
        Ok(())
    }
}

impl Default for Saga {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicI32, Ordering};
    use std::sync::Arc;

    #[test]
    fn test_rollback_stack() {
        let counter = Arc::new(AtomicI32::new(0));

        let mut stack = RollbackStack::new();

        let c = counter.clone();
        stack.push(move || {
            c.fetch_sub(1, Ordering::SeqCst);
        });
        counter.fetch_add(1, Ordering::SeqCst);

        let c = counter.clone();
        stack.push(move || {
            c.fetch_sub(1, Ordering::SeqCst);
        });
        counter.fetch_add(1, Ordering::SeqCst);

        assert_eq!(counter.load(Ordering::SeqCst), 2);

        stack.rollback_all();
        assert_eq!(counter.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn test_checkpoint() {
        let counter = Arc::new(AtomicI32::new(0));

        let mut stack = RollbackStack::new();

        let c = counter.clone();
        stack.checkpoint("cp1", move || {
            c.store(0, Ordering::SeqCst);
        });
        counter.store(1, Ordering::SeqCst);

        let c = counter.clone();
        stack.push(move || {
            c.fetch_sub(1, Ordering::SeqCst);
        });
        counter.fetch_add(1, Ordering::SeqCst);

        stack.rollback_to("cp1").unwrap();
        assert_eq!(counter.load(Ordering::SeqCst), 1); // Only last action rolled back
    }

    #[test]
    fn test_with_rollback() {
        let counter = Arc::new(AtomicI32::new(0));

        let c = counter.clone();
        let result: std::result::Result<(), &str> = with_rollback(|stack| {
            c.fetch_add(1, Ordering::SeqCst);
            let c2 = c.clone();
            stack.push(move || {
                c2.fetch_sub(1, Ordering::SeqCst);
            });
            Err("failure")
        });

        assert!(result.is_err());
        assert_eq!(counter.load(Ordering::SeqCst), 0); // Rolled back
    }

    #[test]
    fn test_rollback_guard() {
        let counter = Arc::new(AtomicI32::new(0));

        // Test auto-rollback
        {
            let mut guard = RollbackGuard::new();
            let c = counter.clone();
            guard.push(move || {
                c.fetch_sub(1, Ordering::SeqCst);
            });
            counter.fetch_add(1, Ordering::SeqCst);
            // Guard drops here, triggering rollback
        }
        assert_eq!(counter.load(Ordering::SeqCst), 0);

        // Test commit
        {
            let mut guard = RollbackGuard::new();
            let c = counter.clone();
            guard.push(move || {
                c.fetch_sub(1, Ordering::SeqCst);
            });
            counter.fetch_add(1, Ordering::SeqCst);
            guard.commit();
        }
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_saga() {
        let counter = Arc::new(AtomicI32::new(0));

        let c1 = counter.clone();
        let c1_comp = counter.clone();
        let c2 = counter.clone();
        let c2_comp = counter.clone();

        let result = Saga::new()
            .step(
                move || {
                    c1.fetch_add(1, Ordering::SeqCst);
                    true
                },
                move || {
                    c1_comp.fetch_sub(1, Ordering::SeqCst);
                },
            )
            .step(
                move || {
                    c2.fetch_add(1, Ordering::SeqCst);
                    true
                },
                move || {
                    c2_comp.fetch_sub(1, Ordering::SeqCst);
                },
            )
            .execute();

        assert!(result.is_ok());
        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }
}
