//! Design by contract for drbot.
//!
//! This crate provides:
//! - Precondition/postcondition checking
//! - Contract definition
//! - Contract enforcement

use thiserror::Error;

/// Contract error types.
#[derive(Error, Debug, Clone)]
pub enum ContractError {
    #[error("Precondition failed: {0}")]
    PreconditionFailed(String),

    #[error("Postcondition failed: {0}")]
    PostconditionFailed(String),

    #[error("Invariant violated: {0}")]
    InvariantViolated(String),
}

/// Result type for contract operations.
pub type Result<T> = std::result::Result<T, ContractError>;

/// Contract definition.
pub struct Contract<T> {
    preconditions: Vec<(Box<dyn Fn(&T) -> bool + Send + Sync>, String)>,
    postconditions: Vec<(Box<dyn Fn(&T, &T) -> bool + Send + Sync>, String)>,
}

impl<T> Contract<T> {
    /// Create new contract.
    pub fn new() -> Self {
        Self {
            preconditions: Vec::new(),
            postconditions: Vec::new(),
        }
    }

    /// Add precondition.
    pub fn requires<F>(mut self, condition: F, message: impl Into<String>) -> Self
    where
        F: Fn(&T) -> bool + Send + Sync + 'static,
    {
        self.preconditions
            .push((Box::new(condition), message.into()));
        self
    }

    /// Add postcondition (checks old and new state).
    pub fn ensures<F>(mut self, condition: F, message: impl Into<String>) -> Self
    where
        F: Fn(&T, &T) -> bool + Send + Sync + 'static,
    {
        self.postconditions
            .push((Box::new(condition), message.into()));
        self
    }

    /// Check preconditions.
    pub fn check_preconditions(&self, value: &T) -> Result<()> {
        for (check, message) in &self.preconditions {
            if !check(value) {
                return Err(ContractError::PreconditionFailed(message.clone()));
            }
        }
        Ok(())
    }

    /// Check postconditions.
    pub fn check_postconditions(&self, old: &T, new: &T) -> Result<()> {
        for (check, message) in &self.postconditions {
            if !check(old, new) {
                return Err(ContractError::PostconditionFailed(message.clone()));
            }
        }
        Ok(())
    }
}

impl<T> Default for Contract<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Execute function with contract enforcement.
pub fn with_contract<T, F, R>(contract: &Contract<T>, state: &mut T, f: F) -> Result<R>
where
    T: Clone,
    F: FnOnce(&mut T) -> R,
{
    // Check preconditions
    contract.check_preconditions(state)?;

    // Save old state
    let old_state = state.clone();

    // Execute function
    let result = f(state);

    // Check postconditions
    contract.check_postconditions(&old_state, state)?;

    Ok(result)
}

/// Contracted function wrapper.
pub struct Contracted<T, F, R>
where
    F: Fn(&mut T) -> R,
{
    contract: Contract<T>,
    func: F,
}

impl<T, F, R> Contracted<T, F, R>
where
    T: Clone,
    F: Fn(&mut T) -> R,
{
    /// Create new contracted function.
    pub fn new(func: F) -> Self {
        Self {
            contract: Contract::new(),
            func,
        }
    }

    /// Add precondition.
    pub fn requires<P>(mut self, condition: P, message: impl Into<String>) -> Self
    where
        P: Fn(&T) -> bool + Send + Sync + 'static,
    {
        self.contract = self.contract.requires(condition, message);
        self
    }

    /// Add postcondition.
    pub fn ensures<P>(mut self, condition: P, message: impl Into<String>) -> Self
    where
        P: Fn(&T, &T) -> bool + Send + Sync + 'static,
    {
        self.contract = self.contract.ensures(condition, message);
        self
    }

    /// Execute with contract checking.
    pub fn call(&self, state: &mut T) -> Result<R> {
        self.contract.check_preconditions(state)?;
        let old_state = state.clone();
        let result = (self.func)(state);
        self.contract.check_postconditions(&old_state, state)?;
        Ok(result)
    }
}

/// Create contracted function.
pub fn contracted<T, F, R>(func: F) -> Contracted<T, F, R>
where
    T: Clone,
    F: Fn(&mut T) -> R,
{
    Contracted::new(func)
}

/// Simple precondition check.
pub fn requires(condition: bool, message: &str) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(ContractError::PreconditionFailed(message.to_string()))
    }
}

/// Simple postcondition check.
pub fn ensures(condition: bool, message: &str) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(ContractError::PostconditionFailed(message.to_string()))
    }
}

/// Simple invariant check.
pub fn invariant(condition: bool, message: &str) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(ContractError::InvariantViolated(message.to_string()))
    }
}

/// Old value wrapper for postconditions.
pub struct Old<T: Clone>(T);

impl<T: Clone> Old<T> {
    /// Capture old value.
    pub fn capture(value: &T) -> Self {
        Self(value.clone())
    }

    /// Get captured value.
    pub fn get(&self) -> &T {
        &self.0
    }

    /// Into inner value.
    pub fn into_inner(self) -> T {
        self.0
    }
}

/// Execute with old value captured.
pub fn with_old<T, F, R>(value: &T, f: F) -> (R, Old<T>)
where
    T: Clone,
    F: FnOnce(&Old<T>) -> R,
{
    let old = Old::capture(value);
    let result = f(&old);
    (result, old)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_contract() {
        let contract = Contract::<i32>::new()
            .requires(|x| *x >= 0, "value must be non-negative")
            .ensures(|old, new| new >= old, "value must not decrease");

        let mut value = 5;
        let result = with_contract(&contract, &mut value, |v| {
            *v += 1;
        });
        assert!(result.is_ok());
        assert_eq!(value, 6);
    }

    #[test]
    fn test_precondition_failure() {
        let contract = Contract::<i32>::new().requires(|x| *x >= 0, "value must be non-negative");

        let mut value = -1;
        let result = with_contract(&contract, &mut value, |v| {
            *v += 1;
        });
        assert!(matches!(result, Err(ContractError::PreconditionFailed(_))));
    }

    #[test]
    fn test_postcondition_failure() {
        let contract = Contract::<i32>::new().ensures(|old, new| new > old, "value must increase");

        let mut value = 5;
        let result = with_contract(&contract, &mut value, |_v| {
            // Don't change value
        });
        assert!(matches!(result, Err(ContractError::PostconditionFailed(_))));
    }

    #[test]
    fn test_contracted_function() {
        let increment = contracted(|x: &mut i32| {
            *x += 1;
        })
        .requires(|x| *x >= 0, "must be non-negative")
        .ensures(|old, new| *new == *old + 1, "must increment by 1");

        let mut value = 5;
        assert!(increment.call(&mut value).is_ok());
        assert_eq!(value, 6);
    }

    #[test]
    fn test_simple_checks() {
        assert!(requires(true, "test").is_ok());
        assert!(requires(false, "test").is_err());

        assert!(ensures(true, "test").is_ok());
        assert!(ensures(false, "test").is_err());

        assert!(invariant(true, "test").is_ok());
        assert!(invariant(false, "test").is_err());
    }
}
