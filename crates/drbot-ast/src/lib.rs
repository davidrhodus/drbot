//! AST utilities and visitor pattern for drbot.
//!
//! This crate provides:
//! - AST node traits
//! - Visitor pattern
//! - AST transformations
//! - Common expression types

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use thiserror::Error;

/// AST error types.
#[derive(Error, Debug)]
pub enum AstError {
    #[error("Invalid node: {0}")]
    InvalidNode(String),

    #[error("Visitor error: {0}")]
    VisitorError(String),

    #[error("Transform error: {0}")]
    TransformError(String),
}

/// Result type for AST operations.
pub type Result<T> = std::result::Result<T, AstError>;

/// Node ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeId(pub u64);

impl NodeId {
    /// Create new node ID.
    pub fn new(id: u64) -> Self {
        Self(id)
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{}", self.0)
    }
}

/// Span in source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Span {
    /// Start offset.
    pub start: usize,
    /// End offset.
    pub end: usize,
}

impl Span {
    /// Create new span.
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    /// Empty span.
    pub fn empty() -> Self {
        Self { start: 0, end: 0 }
    }

    /// Get length.
    pub fn len(&self) -> usize {
        self.end.saturating_sub(self.start)
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.start >= self.end
    }

    /// Merge spans.
    pub fn merge(&self, other: &Span) -> Span {
        Span {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }
}

/// AST node trait.
pub trait Node: fmt::Debug {
    /// Get node ID.
    fn id(&self) -> NodeId;

    /// Get span.
    fn span(&self) -> Span;

    /// Get children.
    fn children(&self) -> Vec<&dyn Node>;
}

/// Simple expression AST.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Expr {
    /// Integer literal.
    Integer { id: NodeId, span: Span, value: i64 },
    /// Float literal.
    Float { id: NodeId, span: Span, value: f64 },
    /// String literal.
    String {
        id: NodeId,
        span: Span,
        value: String,
    },
    /// Boolean literal.
    Bool { id: NodeId, span: Span, value: bool },
    /// Null literal.
    Null { id: NodeId, span: Span },
    /// Identifier.
    Ident {
        id: NodeId,
        span: Span,
        name: String,
    },
    /// Binary operation.
    Binary {
        id: NodeId,
        span: Span,
        op: BinaryOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    /// Unary operation.
    Unary {
        id: NodeId,
        span: Span,
        op: UnaryOp,
        operand: Box<Expr>,
    },
    /// Function call.
    Call {
        id: NodeId,
        span: Span,
        callee: Box<Expr>,
        args: Vec<Expr>,
    },
    /// Member access.
    Member {
        id: NodeId,
        span: Span,
        object: Box<Expr>,
        property: String,
    },
    /// Index access.
    Index {
        id: NodeId,
        span: Span,
        object: Box<Expr>,
        index: Box<Expr>,
    },
    /// Array literal.
    Array {
        id: NodeId,
        span: Span,
        elements: Vec<Expr>,
    },
    /// Object literal.
    Object {
        id: NodeId,
        span: Span,
        properties: Vec<(String, Expr)>,
    },
    /// Conditional (ternary).
    Conditional {
        id: NodeId,
        span: Span,
        condition: Box<Expr>,
        then_expr: Box<Expr>,
        else_expr: Box<Expr>,
    },
}

impl Expr {
    /// Get node ID.
    pub fn id(&self) -> NodeId {
        match self {
            Expr::Integer { id, .. } => *id,
            Expr::Float { id, .. } => *id,
            Expr::String { id, .. } => *id,
            Expr::Bool { id, .. } => *id,
            Expr::Null { id, .. } => *id,
            Expr::Ident { id, .. } => *id,
            Expr::Binary { id, .. } => *id,
            Expr::Unary { id, .. } => *id,
            Expr::Call { id, .. } => *id,
            Expr::Member { id, .. } => *id,
            Expr::Index { id, .. } => *id,
            Expr::Array { id, .. } => *id,
            Expr::Object { id, .. } => *id,
            Expr::Conditional { id, .. } => *id,
        }
    }

    /// Get span.
    pub fn span(&self) -> Span {
        match self {
            Expr::Integer { span, .. } => *span,
            Expr::Float { span, .. } => *span,
            Expr::String { span, .. } => *span,
            Expr::Bool { span, .. } => *span,
            Expr::Null { span, .. } => *span,
            Expr::Ident { span, .. } => *span,
            Expr::Binary { span, .. } => *span,
            Expr::Unary { span, .. } => *span,
            Expr::Call { span, .. } => *span,
            Expr::Member { span, .. } => *span,
            Expr::Index { span, .. } => *span,
            Expr::Array { span, .. } => *span,
            Expr::Object { span, .. } => *span,
            Expr::Conditional { span, .. } => *span,
        }
    }
}

/// Binary operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BinaryOp {
    // Arithmetic
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    // Comparison
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    // Logical
    And,
    Or,
    // Bitwise
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
}

impl fmt::Display for BinaryOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            BinaryOp::Add => "+",
            BinaryOp::Sub => "-",
            BinaryOp::Mul => "*",
            BinaryOp::Div => "/",
            BinaryOp::Mod => "%",
            BinaryOp::Eq => "==",
            BinaryOp::Ne => "!=",
            BinaryOp::Lt => "<",
            BinaryOp::Le => "<=",
            BinaryOp::Gt => ">",
            BinaryOp::Ge => ">=",
            BinaryOp::And => "&&",
            BinaryOp::Or => "||",
            BinaryOp::BitAnd => "&",
            BinaryOp::BitOr => "|",
            BinaryOp::BitXor => "^",
            BinaryOp::Shl => "<<",
            BinaryOp::Shr => ">>",
        };
        write!(f, "{}", s)
    }
}

/// Unary operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnaryOp {
    Neg,
    Not,
    BitNot,
}

impl fmt::Display for UnaryOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            UnaryOp::Neg => "-",
            UnaryOp::Not => "!",
            UnaryOp::BitNot => "~",
        };
        write!(f, "{}", s)
    }
}

/// Expression visitor trait.
pub trait ExprVisitor {
    /// Result type.
    type Result;

    /// Visit expression.
    fn visit(&mut self, expr: &Expr) -> Self::Result {
        match expr {
            Expr::Integer { value, .. } => self.visit_integer(*value),
            Expr::Float { value, .. } => self.visit_float(*value),
            Expr::String { value, .. } => self.visit_string(value),
            Expr::Bool { value, .. } => self.visit_bool(*value),
            Expr::Null { .. } => self.visit_null(),
            Expr::Ident { name, .. } => self.visit_ident(name),
            Expr::Binary {
                op, left, right, ..
            } => self.visit_binary(*op, left, right),
            Expr::Unary { op, operand, .. } => self.visit_unary(*op, operand),
            Expr::Call { callee, args, .. } => self.visit_call(callee, args),
            Expr::Member {
                object, property, ..
            } => self.visit_member(object, property),
            Expr::Index { object, index, .. } => self.visit_index(object, index),
            Expr::Array { elements, .. } => self.visit_array(elements),
            Expr::Object { properties, .. } => self.visit_object(properties),
            Expr::Conditional {
                condition,
                then_expr,
                else_expr,
                ..
            } => self.visit_conditional(condition, then_expr, else_expr),
        }
    }

    fn visit_integer(&mut self, value: i64) -> Self::Result;
    fn visit_float(&mut self, value: f64) -> Self::Result;
    fn visit_string(&mut self, value: &str) -> Self::Result;
    fn visit_bool(&mut self, value: bool) -> Self::Result;
    fn visit_null(&mut self) -> Self::Result;
    fn visit_ident(&mut self, name: &str) -> Self::Result;
    fn visit_binary(&mut self, op: BinaryOp, left: &Expr, right: &Expr) -> Self::Result;
    fn visit_unary(&mut self, op: UnaryOp, operand: &Expr) -> Self::Result;
    fn visit_call(&mut self, callee: &Expr, args: &[Expr]) -> Self::Result;
    fn visit_member(&mut self, object: &Expr, property: &str) -> Self::Result;
    fn visit_index(&mut self, object: &Expr, index: &Expr) -> Self::Result;
    fn visit_array(&mut self, elements: &[Expr]) -> Self::Result;
    fn visit_object(&mut self, properties: &[(String, Expr)]) -> Self::Result;
    fn visit_conditional(
        &mut self,
        condition: &Expr,
        then_expr: &Expr,
        else_expr: &Expr,
    ) -> Self::Result;
}

/// Pretty printer visitor.
pub struct PrettyPrinter {
    indent: usize,
}

impl PrettyPrinter {
    /// Create new printer.
    pub fn new() -> Self {
        Self { indent: 0 }
    }

    fn indent_str(&self) -> String {
        "  ".repeat(self.indent)
    }
}

impl Default for PrettyPrinter {
    fn default() -> Self {
        Self::new()
    }
}

impl ExprVisitor for PrettyPrinter {
    type Result = String;

    fn visit_integer(&mut self, value: i64) -> String {
        value.to_string()
    }

    fn visit_float(&mut self, value: f64) -> String {
        value.to_string()
    }

    fn visit_string(&mut self, value: &str) -> String {
        format!("\"{}\"", value)
    }

    fn visit_bool(&mut self, value: bool) -> String {
        value.to_string()
    }

    fn visit_null(&mut self) -> String {
        "null".to_string()
    }

    fn visit_ident(&mut self, name: &str) -> String {
        name.to_string()
    }

    fn visit_binary(&mut self, op: BinaryOp, left: &Expr, right: &Expr) -> String {
        format!("({} {} {})", self.visit(left), op, self.visit(right))
    }

    fn visit_unary(&mut self, op: UnaryOp, operand: &Expr) -> String {
        format!("({}{})", op, self.visit(operand))
    }

    fn visit_call(&mut self, callee: &Expr, args: &[Expr]) -> String {
        let args_str: Vec<_> = args.iter().map(|a| self.visit(a)).collect();
        format!("{}({})", self.visit(callee), args_str.join(", "))
    }

    fn visit_member(&mut self, object: &Expr, property: &str) -> String {
        format!("{}.{}", self.visit(object), property)
    }

    fn visit_index(&mut self, object: &Expr, index: &Expr) -> String {
        format!("{}[{}]", self.visit(object), self.visit(index))
    }

    fn visit_array(&mut self, elements: &[Expr]) -> String {
        let elems: Vec<_> = elements.iter().map(|e| self.visit(e)).collect();
        format!("[{}]", elems.join(", "))
    }

    fn visit_object(&mut self, properties: &[(String, Expr)]) -> String {
        let props: Vec<_> = properties
            .iter()
            .map(|(k, v)| format!("{}: {}", k, self.visit(v)))
            .collect();
        format!("{{{}}}", props.join(", "))
    }

    fn visit_conditional(
        &mut self,
        condition: &Expr,
        then_expr: &Expr,
        else_expr: &Expr,
    ) -> String {
        format!(
            "({} ? {} : {})",
            self.visit(condition),
            self.visit(then_expr),
            self.visit(else_expr)
        )
    }
}

/// Node ID generator.
#[derive(Debug, Default)]
pub struct IdGenerator {
    next: u64,
}

impl IdGenerator {
    /// Create new generator.
    pub fn new() -> Self {
        Self { next: 0 }
    }

    /// Generate next ID.
    pub fn next(&mut self) -> NodeId {
        let id = NodeId::new(self.next);
        self.next += 1;
        id
    }
}

/// Symbol table.
#[derive(Debug, Default)]
pub struct SymbolTable {
    scopes: Vec<HashMap<String, SymbolInfo>>,
}

impl SymbolTable {
    /// Create new symbol table.
    pub fn new() -> Self {
        Self {
            scopes: vec![HashMap::new()],
        }
    }

    /// Enter new scope.
    pub fn enter_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    /// Exit current scope.
    pub fn exit_scope(&mut self) {
        if self.scopes.len() > 1 {
            self.scopes.pop();
        }
    }

    /// Define symbol in current scope.
    pub fn define(&mut self, name: impl Into<String>, info: SymbolInfo) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.into(), info);
        }
    }

    /// Lookup symbol.
    pub fn lookup(&self, name: &str) -> Option<&SymbolInfo> {
        for scope in self.scopes.iter().rev() {
            if let Some(info) = scope.get(name) {
                return Some(info);
            }
        }
        None
    }

    /// Check if symbol exists in current scope.
    pub fn exists_in_current(&self, name: &str) -> bool {
        self.scopes
            .last()
            .map_or(false, |scope| scope.contains_key(name))
    }
}

/// Symbol information.
#[derive(Debug, Clone)]
pub struct SymbolInfo {
    /// Symbol kind.
    pub kind: SymbolKind,
    /// Node ID.
    pub node_id: NodeId,
    /// Type (if known).
    pub type_info: Option<String>,
}

/// Symbol kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolKind {
    Variable,
    Function,
    Parameter,
    Type,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_span() {
        let s1 = Span::new(0, 5);
        let s2 = Span::new(3, 10);
        let merged = s1.merge(&s2);
        assert_eq!(merged.start, 0);
        assert_eq!(merged.end, 10);
    }

    #[test]
    fn test_expr_id() {
        let expr = Expr::Integer {
            id: NodeId::new(1),
            span: Span::new(0, 5),
            value: 42,
        };
        assert_eq!(expr.id(), NodeId::new(1));
    }

    #[test]
    fn test_pretty_printer() {
        let mut gen = IdGenerator::new();
        let expr = Expr::Binary {
            id: gen.next(),
            span: Span::empty(),
            op: BinaryOp::Add,
            left: Box::new(Expr::Integer {
                id: gen.next(),
                span: Span::empty(),
                value: 1,
            }),
            right: Box::new(Expr::Integer {
                id: gen.next(),
                span: Span::empty(),
                value: 2,
            }),
        };

        let mut printer = PrettyPrinter::new();
        let result = printer.visit(&expr);
        assert_eq!(result, "(1 + 2)");
    }

    #[test]
    fn test_symbol_table() {
        let mut table = SymbolTable::new();

        table.define(
            "x",
            SymbolInfo {
                kind: SymbolKind::Variable,
                node_id: NodeId::new(1),
                type_info: Some("int".to_string()),
            },
        );

        assert!(table.lookup("x").is_some());
        assert!(table.lookup("y").is_none());

        table.enter_scope();
        table.define(
            "y",
            SymbolInfo {
                kind: SymbolKind::Variable,
                node_id: NodeId::new(2),
                type_info: None,
            },
        );

        assert!(table.lookup("x").is_some());
        assert!(table.lookup("y").is_some());

        table.exit_scope();
        assert!(table.lookup("y").is_none());
    }
}
