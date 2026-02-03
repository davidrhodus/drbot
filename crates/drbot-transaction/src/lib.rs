//! Transaction-like semantics for drbot.
//!
//! This crate provides:
//! - Transaction abstraction
//! - Commit/rollback
//! - Savepoints

use thiserror::Error;

/// Transaction error types.
#[derive(Error, Debug, Clone)]
pub enum TransactionError {
    #[error("Transaction already committed")]
    AlreadyCommitted,

    #[error("Transaction already rolled back")]
    AlreadyRolledBack,

    #[error("Transaction failed: {0}")]
    Failed(String),

    #[error("Savepoint not found: {0}")]
    SavepointNotFound(String),
}

/// Result type for transaction operations.
pub type Result<T> = std::result::Result<T, TransactionError>;

/// Transaction state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionState {
    Active,
    Committed,
    RolledBack,
}

/// Transaction trait.
pub trait Transactional {
    /// Commit the transaction.
    fn commit(&mut self) -> Result<()>;

    /// Rollback the transaction.
    fn rollback(&mut self) -> Result<()>;

    /// Get transaction state.
    fn state(&self) -> TransactionState;
}

/// Simple transaction with actions.
pub struct Transaction<T: Clone> {
    initial_state: T,
    current_state: T,
    state: TransactionState,
    savepoints: Vec<(String, T)>,
}

impl<T: Clone> Transaction<T> {
    /// Create new transaction.
    pub fn new(initial_state: T) -> Self {
        let current_state = initial_state.clone();
        Self {
            initial_state,
            current_state,
            state: TransactionState::Active,
            savepoints: Vec::new(),
        }
    }

    /// Get current state.
    pub fn get(&self) -> &T {
        &self.current_state
    }

    /// Get mutable current state.
    pub fn get_mut(&mut self) -> Option<&mut T> {
        if self.state == TransactionState::Active {
            Some(&mut self.current_state)
        } else {
            None
        }
    }

    /// Update state with function.
    pub fn update<F>(&mut self, f: F) -> Result<()>
    where
        F: FnOnce(&mut T),
    {
        if self.state != TransactionState::Active {
            return Err(if self.state == TransactionState::Committed {
                TransactionError::AlreadyCommitted
            } else {
                TransactionError::AlreadyRolledBack
            });
        }

        f(&mut self.current_state);
        Ok(())
    }

    /// Create savepoint.
    pub fn savepoint(&mut self, name: impl Into<String>) -> Result<()> {
        if self.state != TransactionState::Active {
            return Err(TransactionError::AlreadyCommitted);
        }

        self.savepoints
            .push((name.into(), self.current_state.clone()));
        Ok(())
    }

    /// Rollback to savepoint.
    pub fn rollback_to(&mut self, name: &str) -> Result<()> {
        if self.state != TransactionState::Active {
            return Err(TransactionError::AlreadyRolledBack);
        }

        let pos = self
            .savepoints
            .iter()
            .rposition(|(n, _)| n == name)
            .ok_or_else(|| TransactionError::SavepointNotFound(name.to_string()))?;

        self.current_state = self.savepoints[pos].1.clone();
        self.savepoints.truncate(pos);
        Ok(())
    }

    /// Into committed value.
    pub fn into_committed(mut self) -> Result<T> {
        self.commit()?;
        Ok(self.current_state)
    }
}

impl<T: Clone> Transactional for Transaction<T> {
    fn commit(&mut self) -> Result<()> {
        match self.state {
            TransactionState::Active => {
                self.state = TransactionState::Committed;
                self.savepoints.clear();
                Ok(())
            }
            TransactionState::Committed => Err(TransactionError::AlreadyCommitted),
            TransactionState::RolledBack => Err(TransactionError::AlreadyRolledBack),
        }
    }

    fn rollback(&mut self) -> Result<()> {
        match self.state {
            TransactionState::Active => {
                self.current_state = self.initial_state.clone();
                self.state = TransactionState::RolledBack;
                self.savepoints.clear();
                Ok(())
            }
            TransactionState::Committed => Err(TransactionError::AlreadyCommitted),
            TransactionState::RolledBack => Err(TransactionError::AlreadyRolledBack),
        }
    }

    fn state(&self) -> TransactionState {
        self.state
    }
}

/// Execute in transaction context.
pub fn transact<T: Clone, R, F>(initial: T, f: F) -> Result<(R, T)>
where
    F: FnOnce(&mut Transaction<T>) -> Result<R>,
{
    let mut tx = Transaction::new(initial);
    let result = f(&mut tx)?;
    tx.commit()?;
    Ok((result, tx.current_state))
}

/// Try to execute, rollback on error.
pub fn try_transact<T: Clone, R, E, F>(initial: T, f: F) -> std::result::Result<(R, T), E>
where
    F: FnOnce(&mut Transaction<T>) -> std::result::Result<R, E>,
{
    let mut tx = Transaction::new(initial);
    match f(&mut tx) {
        Ok(result) => {
            let _ = tx.commit();
            Ok((result, tx.current_state))
        }
        Err(e) => {
            let _ = tx.rollback();
            Err(e)
        }
    }
}

/// Transaction with actions for undo.
pub struct ActionTransaction {
    actions: Vec<Box<dyn FnOnce() + Send>>,
    undo_actions: Vec<Box<dyn FnOnce() + Send>>,
    state: TransactionState,
}

impl ActionTransaction {
    /// Create new action transaction.
    pub fn new() -> Self {
        Self {
            actions: Vec::new(),
            undo_actions: Vec::new(),
            state: TransactionState::Active,
        }
    }

    /// Add action with undo.
    pub fn add_action<A, U>(&mut self, action: A, undo: U) -> Result<()>
    where
        A: FnOnce() + Send + 'static,
        U: FnOnce() + Send + 'static,
    {
        if self.state != TransactionState::Active {
            return Err(TransactionError::AlreadyCommitted);
        }

        self.actions.push(Box::new(action));
        self.undo_actions.push(Box::new(undo));
        Ok(())
    }

    /// Execute all actions.
    pub fn execute(&mut self) -> Result<()> {
        if self.state != TransactionState::Active {
            return Err(TransactionError::AlreadyCommitted);
        }

        for action in self.actions.drain(..) {
            action();
        }
        Ok(())
    }
}

impl Default for ActionTransaction {
    fn default() -> Self {
        Self::new()
    }
}

impl Transactional for ActionTransaction {
    fn commit(&mut self) -> Result<()> {
        match self.state {
            TransactionState::Active => {
                self.undo_actions.clear();
                self.state = TransactionState::Committed;
                Ok(())
            }
            TransactionState::Committed => Err(TransactionError::AlreadyCommitted),
            TransactionState::RolledBack => Err(TransactionError::AlreadyRolledBack),
        }
    }

    fn rollback(&mut self) -> Result<()> {
        match self.state {
            TransactionState::Active => {
                // Execute undo actions in reverse order
                for undo in self.undo_actions.drain(..).rev() {
                    undo();
                }
                self.state = TransactionState::RolledBack;
                Ok(())
            }
            TransactionState::Committed => Err(TransactionError::AlreadyCommitted),
            TransactionState::RolledBack => Err(TransactionError::AlreadyRolledBack),
        }
    }

    fn state(&self) -> TransactionState {
        self.state
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transaction_commit() {
        let mut tx = Transaction::new(0);
        tx.update(|v| *v += 1).unwrap();
        tx.update(|v| *v += 1).unwrap();

        assert_eq!(*tx.get(), 2);
        tx.commit().unwrap();
        assert_eq!(tx.state(), TransactionState::Committed);
    }

    #[test]
    fn test_transaction_rollback() {
        let mut tx = Transaction::new(0);
        tx.update(|v| *v += 1).unwrap();
        tx.update(|v| *v += 1).unwrap();

        tx.rollback().unwrap();
        assert_eq!(*tx.get(), 0);
        assert_eq!(tx.state(), TransactionState::RolledBack);
    }

    #[test]
    fn test_savepoint() {
        let mut tx = Transaction::new(0);
        tx.update(|v| *v = 1).unwrap();
        tx.savepoint("sp1").unwrap();
        tx.update(|v| *v = 2).unwrap();
        tx.savepoint("sp2").unwrap();
        tx.update(|v| *v = 3).unwrap();

        tx.rollback_to("sp1").unwrap();
        assert_eq!(*tx.get(), 1);
    }

    #[test]
    fn test_transact() {
        let (result, final_state) = transact(0, |tx| {
            tx.update(|v| *v += 10)?;
            Ok(*tx.get())
        })
        .unwrap();

        assert_eq!(result, 10);
        assert_eq!(final_state, 10);
    }

    #[test]
    fn test_action_transaction() {
        let counter = std::sync::Arc::new(std::sync::atomic::AtomicI32::new(0));

        let mut tx = ActionTransaction::new();

        let c = counter.clone();
        tx.add_action(
            move || {
                c.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            },
            || {},
        )
        .unwrap();

        tx.execute().unwrap();
        tx.commit().unwrap();

        assert_eq!(counter.load(std::sync::atomic::Ordering::SeqCst), 1);
    }
}
