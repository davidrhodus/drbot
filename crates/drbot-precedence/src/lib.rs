//! Precedence parsing utilities for drbot.
//!
//! This crate provides:
//! - Pratt parser implementation
//! - Operator precedence handling
//! - Associativity handling

use std::collections::HashMap;
use thiserror::Error;

/// Precedence error types.
#[derive(Error, Debug, Clone)]
pub enum PrecedenceError {
    #[error("Unknown operator: {0}")]
    UnknownOperator(String),

    #[error("Parse error: {0}")]
    ParseError(String),
}

/// Result type for precedence operations.
pub type Result<T> = std::result::Result<T, PrecedenceError>;

/// Operator associativity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Associativity {
    Left,
    Right,
    None,
}

/// Operator info.
#[derive(Debug, Clone)]
pub struct OperatorInfo {
    pub precedence: u8,
    pub associativity: Associativity,
}

impl OperatorInfo {
    /// Create new operator info.
    pub fn new(precedence: u8, associativity: Associativity) -> Self {
        Self {
            precedence,
            associativity,
        }
    }

    /// Create left-associative operator.
    pub fn left(precedence: u8) -> Self {
        Self::new(precedence, Associativity::Left)
    }

    /// Create right-associative operator.
    pub fn right(precedence: u8) -> Self {
        Self::new(precedence, Associativity::Right)
    }
}

/// Operator table.
#[derive(Debug, Clone, Default)]
pub struct OperatorTable {
    operators: HashMap<String, OperatorInfo>,
}

impl OperatorTable {
    /// Create new operator table.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add operator.
    pub fn add<S: Into<String>>(&mut self, op: S, info: OperatorInfo) {
        self.operators.insert(op.into(), info);
    }

    /// Get operator info.
    pub fn get(&self, op: &str) -> Option<&OperatorInfo> {
        self.operators.get(op)
    }

    /// Check if operator exists.
    pub fn contains(&self, op: &str) -> bool {
        self.operators.contains_key(op)
    }

    /// Get precedence of operator.
    pub fn precedence(&self, op: &str) -> Option<u8> {
        self.operators.get(op).map(|info| info.precedence)
    }

    /// Get associativity of operator.
    pub fn associativity(&self, op: &str) -> Option<Associativity> {
        self.operators.get(op).map(|info| info.associativity)
    }
}

/// Standard arithmetic operator table.
pub fn arithmetic_operators() -> OperatorTable {
    let mut table = OperatorTable::new();
    table.add("+", OperatorInfo::left(1));
    table.add("-", OperatorInfo::left(1));
    table.add("*", OperatorInfo::left(2));
    table.add("/", OperatorInfo::left(2));
    table.add("%", OperatorInfo::left(2));
    table.add("^", OperatorInfo::right(3));
    table
}

/// Standard comparison operator table.
pub fn comparison_operators() -> OperatorTable {
    let mut table = OperatorTable::new();
    table.add("==", OperatorInfo::new(0, Associativity::None));
    table.add("!=", OperatorInfo::new(0, Associativity::None));
    table.add("<", OperatorInfo::new(0, Associativity::None));
    table.add(">", OperatorInfo::new(0, Associativity::None));
    table.add("<=", OperatorInfo::new(0, Associativity::None));
    table.add(">=", OperatorInfo::new(0, Associativity::None));
    table
}

/// Standard logical operator table.
pub fn logical_operators() -> OperatorTable {
    let mut table = OperatorTable::new();
    table.add("&&", OperatorInfo::left(1));
    table.add("||", OperatorInfo::left(0));
    table.add("!", OperatorInfo::right(2));
    table
}

/// Simple expression type.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Number(f64),
    Identifier(String),
    Binary {
        op: String,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Unary {
        op: String,
        operand: Box<Expr>,
    },
    Group(Box<Expr>),
}

impl Expr {
    /// Create binary expression.
    pub fn binary(op: impl Into<String>, left: Expr, right: Expr) -> Self {
        Self::Binary {
            op: op.into(),
            left: Box::new(left),
            right: Box::new(right),
        }
    }

    /// Create unary expression.
    pub fn unary(op: impl Into<String>, operand: Expr) -> Self {
        Self::Unary {
            op: op.into(),
            operand: Box::new(operand),
        }
    }

    /// Create grouped expression.
    pub fn group(expr: Expr) -> Self {
        Self::Group(Box::new(expr))
    }
}

/// Simple Pratt parser.
pub struct PrattParser {
    operators: OperatorTable,
}

impl PrattParser {
    /// Create new Pratt parser.
    pub fn new(operators: OperatorTable) -> Self {
        Self { operators }
    }

    /// Create with arithmetic operators.
    pub fn arithmetic() -> Self {
        Self::new(arithmetic_operators())
    }

    /// Get binding power for operator.
    pub fn binding_power(&self, op: &str) -> Option<(u8, u8)> {
        let info = self.operators.get(op)?;
        match info.associativity {
            Associativity::Left => Some((info.precedence * 2, info.precedence * 2 + 1)),
            Associativity::Right => Some((info.precedence * 2 + 1, info.precedence * 2)),
            Associativity::None => Some((info.precedence * 2, info.precedence * 2)),
        }
    }

    /// Check if token is an operator.
    pub fn is_operator(&self, token: &str) -> bool {
        self.operators.contains(token)
    }
}

/// Shunting-yard algorithm implementation.
pub struct ShuntingYard {
    operators: OperatorTable,
    output: Vec<String>,
    op_stack: Vec<String>,
}

impl ShuntingYard {
    /// Create new shunting-yard converter.
    pub fn new(operators: OperatorTable) -> Self {
        Self {
            operators,
            output: Vec::new(),
            op_stack: Vec::new(),
        }
    }

    /// Push operand.
    pub fn push_operand(&mut self, operand: impl Into<String>) {
        self.output.push(operand.into());
    }

    /// Push operator.
    pub fn push_operator(&mut self, op: impl Into<String>) {
        let op = op.into();
        if let Some(info) = self.operators.get(&op) {
            while let Some(top) = self.op_stack.last() {
                if top == "(" {
                    break;
                }
                if let Some(top_info) = self.operators.get(top) {
                    let should_pop = match info.associativity {
                        Associativity::Left => top_info.precedence >= info.precedence,
                        Associativity::Right => top_info.precedence > info.precedence,
                        Associativity::None => top_info.precedence >= info.precedence,
                    };
                    if should_pop {
                        self.output.push(self.op_stack.pop().unwrap());
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            }
        }
        self.op_stack.push(op);
    }

    /// Push left parenthesis.
    pub fn push_left_paren(&mut self) {
        self.op_stack.push("(".to_string());
    }

    /// Push right parenthesis.
    pub fn push_right_paren(&mut self) -> Result<()> {
        while let Some(top) = self.op_stack.pop() {
            if top == "(" {
                return Ok(());
            }
            self.output.push(top);
        }
        Err(PrecedenceError::ParseError("Mismatched parentheses".into()))
    }

    /// Finish and get output.
    pub fn finish(mut self) -> Result<Vec<String>> {
        while let Some(op) = self.op_stack.pop() {
            if op == "(" || op == ")" {
                return Err(PrecedenceError::ParseError("Mismatched parentheses".into()));
            }
            self.output.push(op);
        }
        Ok(self.output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_operator_table() {
        let table = arithmetic_operators();

        assert!(table.contains("+"));
        assert_eq!(table.precedence("+"), Some(1));
        assert_eq!(table.precedence("*"), Some(2));
        assert_eq!(table.associativity("^"), Some(Associativity::Right));
    }

    #[test]
    fn test_pratt_parser() {
        let parser = PrattParser::arithmetic();

        // Left associative: left binding is weaker
        let (l, r) = parser.binding_power("+").unwrap();
        assert!(l < r);

        // Right associative: right binding is weaker
        let (l, r) = parser.binding_power("^").unwrap();
        assert!(l > r);
    }

    #[test]
    fn test_shunting_yard() {
        let mut sy = ShuntingYard::new(arithmetic_operators());

        // Convert "3 + 4 * 2" to postfix
        sy.push_operand("3");
        sy.push_operator("+");
        sy.push_operand("4");
        sy.push_operator("*");
        sy.push_operand("2");

        let result = sy.finish().unwrap();
        assert_eq!(result, vec!["3", "4", "2", "*", "+"]);
    }

    #[test]
    fn test_shunting_yard_parens() {
        let mut sy = ShuntingYard::new(arithmetic_operators());

        // Convert "(3 + 4) * 2" to postfix
        sy.push_left_paren();
        sy.push_operand("3");
        sy.push_operator("+");
        sy.push_operand("4");
        sy.push_right_paren().unwrap();
        sy.push_operator("*");
        sy.push_operand("2");

        let result = sy.finish().unwrap();
        assert_eq!(result, vec!["3", "4", "+", "2", "*"]);
    }

    #[test]
    fn test_expr() {
        let expr = Expr::binary(
            "+",
            Expr::Number(1.0),
            Expr::binary("*", Expr::Number(2.0), Expr::Number(3.0)),
        );

        if let Expr::Binary { op, .. } = expr {
            assert_eq!(op, "+");
        } else {
            panic!("Expected binary expression");
        }
    }
}
