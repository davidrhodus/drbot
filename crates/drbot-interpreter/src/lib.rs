//! Interpreter pattern utilities for drbot.
//!
//! This crate provides:
//! - Expression trait
//! - Context for interpretation
//! - Common expression types

use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;

/// Interpreter error types.
#[derive(Error, Debug)]
pub enum InterpreterError {
    #[error("Undefined variable: {0}")]
    UndefinedVariable(String),

    #[error("Type error: {0}")]
    TypeError(String),

    #[error("Evaluation failed: {0}")]
    EvaluationFailed(String),
}

/// Result type for interpreter operations.
pub type Result<T> = std::result::Result<T, InterpreterError>;

/// Expression trait.
pub trait Expression<C, V>: Send + Sync {
    /// Interpret the expression.
    fn interpret(&self, context: &C) -> Result<V>;
}

/// Simple context with variables.
#[derive(Debug, Clone, Default)]
pub struct Context<V: Clone> {
    variables: HashMap<String, V>,
}

impl<V: Clone> Context<V> {
    /// Create new context.
    pub fn new() -> Self {
        Self {
            variables: HashMap::new(),
        }
    }

    /// Set variable.
    pub fn set(&mut self, name: impl Into<String>, value: V) {
        self.variables.insert(name.into(), value);
    }

    /// Get variable.
    pub fn get(&self, name: &str) -> Option<&V> {
        self.variables.get(name)
    }

    /// Check if variable exists.
    pub fn contains(&self, name: &str) -> bool {
        self.variables.contains_key(name)
    }

    /// Clear all variables.
    pub fn clear(&mut self) {
        self.variables.clear();
    }
}

/// Literal expression (constant value).
pub struct Literal<V: Clone + Send + Sync>(V);

impl<V: Clone + Send + Sync> Literal<V> {
    /// Create new literal.
    pub fn new(value: V) -> Self {
        Self(value)
    }
}

impl<C, V: Clone + Send + Sync> Expression<C, V> for Literal<V> {
    fn interpret(&self, _context: &C) -> Result<V> {
        Ok(self.0.clone())
    }
}

/// Variable expression (lookup in context).
pub struct Variable {
    name: String,
}

impl Variable {
    /// Create new variable.
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }
}

impl<V: Clone + Send + Sync> Expression<Context<V>, V> for Variable {
    fn interpret(&self, context: &Context<V>) -> Result<V> {
        context
            .get(&self.name)
            .cloned()
            .ok_or_else(|| InterpreterError::UndefinedVariable(self.name.clone()))
    }
}

/// Binary expression.
pub struct BinaryExpr<C, V, L, R, F>
where
    L: Expression<C, V>,
    R: Expression<C, V>,
    F: Fn(V, V) -> Result<V> + Send + Sync,
{
    left: L,
    right: R,
    op: F,
    _marker: std::marker::PhantomData<(C, V)>,
}

impl<C, V, L, R, F> BinaryExpr<C, V, L, R, F>
where
    L: Expression<C, V>,
    R: Expression<C, V>,
    F: Fn(V, V) -> Result<V> + Send + Sync,
{
    /// Create new binary expression.
    pub fn new(left: L, right: R, op: F) -> Self {
        Self {
            left,
            right,
            op,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<C: Send + Sync, V: Send + Sync, L, R, F> Expression<C, V> for BinaryExpr<C, V, L, R, F>
where
    L: Expression<C, V>,
    R: Expression<C, V>,
    F: Fn(V, V) -> Result<V> + Send + Sync,
{
    fn interpret(&self, context: &C) -> Result<V> {
        let left = self.left.interpret(context)?;
        let right = self.right.interpret(context)?;
        (self.op)(left, right)
    }
}

/// Unary expression.
pub struct UnaryExpr<C, V, E, F>
where
    E: Expression<C, V>,
    F: Fn(V) -> Result<V> + Send + Sync,
{
    expr: E,
    op: F,
    _marker: std::marker::PhantomData<(C, V)>,
}

impl<C, V, E, F> UnaryExpr<C, V, E, F>
where
    E: Expression<C, V>,
    F: Fn(V) -> Result<V> + Send + Sync,
{
    /// Create new unary expression.
    pub fn new(expr: E, op: F) -> Self {
        Self {
            expr,
            op,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<C: Send + Sync, V: Send + Sync, E, F> Expression<C, V> for UnaryExpr<C, V, E, F>
where
    E: Expression<C, V>,
    F: Fn(V) -> Result<V> + Send + Sync,
{
    fn interpret(&self, context: &C) -> Result<V> {
        let value = self.expr.interpret(context)?;
        (self.op)(value)
    }
}

/// Conditional expression.
pub struct Conditional<C, V, Cond, Then, Else>
where
    Cond: Expression<C, bool>,
    Then: Expression<C, V>,
    Else: Expression<C, V>,
{
    condition: Cond,
    then_expr: Then,
    else_expr: Else,
    _marker: std::marker::PhantomData<(C, V)>,
}

impl<C, V, Cond, Then, Else> Conditional<C, V, Cond, Then, Else>
where
    Cond: Expression<C, bool>,
    Then: Expression<C, V>,
    Else: Expression<C, V>,
{
    /// Create new conditional.
    pub fn new(condition: Cond, then_expr: Then, else_expr: Else) -> Self {
        Self {
            condition,
            then_expr,
            else_expr,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<C: Send + Sync, V: Send + Sync, Cond, Then, Else> Expression<C, V>
    for Conditional<C, V, Cond, Then, Else>
where
    Cond: Expression<C, bool>,
    Then: Expression<C, V>,
    Else: Expression<C, V>,
{
    fn interpret(&self, context: &C) -> Result<V> {
        if self.condition.interpret(context)? {
            self.then_expr.interpret(context)
        } else {
            self.else_expr.interpret(context)
        }
    }
}

/// Function expression.
pub struct FnExpr<C, V, F: Fn(&C) -> Result<V> + Send + Sync> {
    func: F,
    _marker: std::marker::PhantomData<(C, V)>,
}

impl<C, V, F: Fn(&C) -> Result<V> + Send + Sync> FnExpr<C, V, F> {
    /// Create new function expression.
    pub fn new(func: F) -> Self {
        Self {
            func,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<C: Send + Sync, V: Send + Sync, F: Fn(&C) -> Result<V> + Send + Sync> Expression<C, V>
    for FnExpr<C, V, F>
{
    fn interpret(&self, context: &C) -> Result<V> {
        (self.func)(context)
    }
}

/// Helper to create literal.
pub fn literal<V: Clone + Send + Sync + 'static>(value: V) -> Arc<dyn Expression<Context<V>, V>> {
    Arc::new(Literal::new(value))
}

/// Helper to create variable.
pub fn variable<V: Clone + Send + Sync + 'static>(
    name: impl Into<String>,
) -> Arc<dyn Expression<Context<V>, V>> {
    Arc::new(Variable::new(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_literal() {
        let ctx = Context::<i32>::new();
        let expr = Literal::new(42);
        assert_eq!(expr.interpret(&ctx).unwrap(), 42);
    }

    #[test]
    fn test_variable() {
        let mut ctx = Context::new();
        ctx.set("x", 42);

        let expr = Variable::new("x");
        assert_eq!(expr.interpret(&ctx).unwrap(), 42);

        let undefined = Variable::new("y");
        assert!(undefined.interpret(&ctx).is_err());
    }

    #[test]
    fn test_binary_expr() {
        let ctx = Context::<i32>::new();
        let expr = BinaryExpr::new(Literal::new(10), Literal::new(32), |a, b| Ok(a + b));
        assert_eq!(expr.interpret(&ctx).unwrap(), 42);
    }

    #[test]
    fn test_conditional() {
        let ctx = Context::<i32>::new();
        let expr = Conditional::new(Literal::new(true), Literal::new(42), Literal::new(0));
        assert_eq!(expr.interpret(&ctx).unwrap(), 42);

        let expr2 = Conditional::new(Literal::new(false), Literal::new(42), Literal::new(0));
        assert_eq!(expr2.interpret(&ctx).unwrap(), 0);
    }
}
