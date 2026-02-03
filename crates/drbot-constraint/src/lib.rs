//! Constraint satisfaction and validation for drbot.
//!
//! This crate provides:
//! - Constraint definitions
//! - Constraint solving
//! - Variable domains
//! - Constraint propagation

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use thiserror::Error;

/// Constraint error types.
#[derive(Error, Debug)]
pub enum ConstraintError {
    #[error("Unsatisfiable constraint: {0}")]
    Unsatisfiable(String),

    #[error("Variable not found: {0}")]
    VariableNotFound(String),

    #[error("Domain empty for: {0}")]
    EmptyDomain(String),

    #[error("Invalid constraint: {0}")]
    Invalid(String),
}

/// Result type for constraint operations.
pub type Result<T> = std::result::Result<T, ConstraintError>;

/// Variable value types.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// Integer value.
    Int(i64),
    /// Float value.
    Float(f64),
    /// String value.
    String(String),
    /// Boolean value.
    Bool(bool),
}

impl std::hash::Hash for Value {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
        match self {
            Value::Int(v) => v.hash(state),
            Value::Float(v) => v.to_bits().hash(state),
            Value::String(v) => v.hash(state),
            Value::Bool(v) => v.hash(state),
        }
    }
}

impl Eq for Value {}

impl Value {
    /// Get as integer.
    pub fn as_int(&self) -> Option<i64> {
        match self {
            Value::Int(v) => Some(*v),
            _ => None,
        }
    }

    /// Get as float.
    pub fn as_float(&self) -> Option<f64> {
        match self {
            Value::Int(v) => Some(*v as f64),
            Value::Float(v) => Some(*v),
            _ => None,
        }
    }

    /// Get as string.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(v) => Some(v),
            _ => None,
        }
    }

    /// Get as boolean.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(v) => Some(*v),
            _ => None,
        }
    }
}

/// Variable domain (possible values).
#[derive(Debug, Clone)]
pub enum Domain {
    /// Integer range.
    IntRange { min: i64, max: i64 },
    /// Discrete integer values.
    IntSet(HashSet<i64>),
    /// Boolean domain.
    Boolean,
    /// String set.
    StringSet(HashSet<String>),
    /// Any value.
    Any,
}

impl Domain {
    /// Create integer range domain.
    pub fn int_range(min: i64, max: i64) -> Self {
        Domain::IntRange { min, max }
    }

    /// Create integer set domain.
    pub fn int_set(values: impl IntoIterator<Item = i64>) -> Self {
        Domain::IntSet(values.into_iter().collect())
    }

    /// Create string set domain.
    pub fn string_set(values: impl IntoIterator<Item = String>) -> Self {
        Domain::StringSet(values.into_iter().collect())
    }

    /// Check if value is in domain.
    pub fn contains(&self, value: &Value) -> bool {
        match (self, value) {
            (Domain::IntRange { min, max }, Value::Int(v)) => v >= min && v <= max,
            (Domain::IntSet(set), Value::Int(v)) => set.contains(v),
            (Domain::Boolean, Value::Bool(_)) => true,
            (Domain::StringSet(set), Value::String(v)) => set.contains(v),
            (Domain::Any, _) => true,
            _ => false,
        }
    }

    /// Get domain size (None for infinite).
    pub fn size(&self) -> Option<usize> {
        match self {
            Domain::IntRange { min, max } => Some((max - min + 1) as usize),
            Domain::IntSet(set) => Some(set.len()),
            Domain::Boolean => Some(2),
            Domain::StringSet(set) => Some(set.len()),
            Domain::Any => None,
        }
    }

    /// Check if domain is empty.
    pub fn is_empty(&self) -> bool {
        match self {
            Domain::IntRange { min, max } => min > max,
            Domain::IntSet(set) => set.is_empty(),
            Domain::Boolean => false,
            Domain::StringSet(set) => set.is_empty(),
            Domain::Any => false,
        }
    }
}

/// Constraint types.
#[derive(Clone)]
pub enum Constraint {
    /// Equal constraint.
    Equal(String, String),
    /// Not equal constraint.
    NotEqual(String, String),
    /// Less than constraint.
    LessThan(String, String),
    /// Less than or equal constraint.
    LessOrEqual(String, String),
    /// Greater than constraint.
    GreaterThan(String, String),
    /// Greater or equal constraint.
    GreaterOrEqual(String, String),
    /// All different constraint.
    AllDifferent(Vec<String>),
    /// Sum equals value.
    Sum { vars: Vec<String>, equals: i64 },
    /// Custom constraint.
    Custom(Arc<dyn Fn(&HashMap<String, Value>) -> bool + Send + Sync>),
}

impl std::fmt::Debug for Constraint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Constraint::Equal(a, b) => f.debug_tuple("Equal").field(a).field(b).finish(),
            Constraint::NotEqual(a, b) => f.debug_tuple("NotEqual").field(a).field(b).finish(),
            Constraint::LessThan(a, b) => f.debug_tuple("LessThan").field(a).field(b).finish(),
            Constraint::LessOrEqual(a, b) => {
                f.debug_tuple("LessOrEqual").field(a).field(b).finish()
            }
            Constraint::GreaterThan(a, b) => {
                f.debug_tuple("GreaterThan").field(a).field(b).finish()
            }
            Constraint::GreaterOrEqual(a, b) => {
                f.debug_tuple("GreaterOrEqual").field(a).field(b).finish()
            }
            Constraint::AllDifferent(vars) => f.debug_tuple("AllDifferent").field(vars).finish(),
            Constraint::Sum { vars, equals } => f
                .debug_struct("Sum")
                .field("vars", vars)
                .field("equals", equals)
                .finish(),
            Constraint::Custom(_) => f.debug_tuple("Custom").field(&"<fn>").finish(),
        }
    }
}

impl Constraint {
    /// Get involved variables.
    pub fn variables(&self) -> Vec<&str> {
        match self {
            Constraint::Equal(a, b)
            | Constraint::NotEqual(a, b)
            | Constraint::LessThan(a, b)
            | Constraint::LessOrEqual(a, b)
            | Constraint::GreaterThan(a, b)
            | Constraint::GreaterOrEqual(a, b) => vec![a.as_str(), b.as_str()],
            Constraint::AllDifferent(vars) | Constraint::Sum { vars, .. } => {
                vars.iter().map(|s| s.as_str()).collect()
            }
            Constraint::Custom(_) => vec![],
        }
    }

    /// Check if constraint is satisfied.
    pub fn is_satisfied(&self, assignment: &HashMap<String, Value>) -> bool {
        match self {
            Constraint::Equal(a, b) => {
                match (assignment.get(a), assignment.get(b)) {
                    (Some(va), Some(vb)) => va == vb,
                    _ => true, // Unassigned variables don't violate
                }
            }
            Constraint::NotEqual(a, b) => match (assignment.get(a), assignment.get(b)) {
                (Some(va), Some(vb)) => va != vb,
                _ => true,
            },
            Constraint::LessThan(a, b) => {
                match (
                    assignment.get(a).and_then(|v| v.as_float()),
                    assignment.get(b).and_then(|v| v.as_float()),
                ) {
                    (Some(va), Some(vb)) => va < vb,
                    _ => true,
                }
            }
            Constraint::LessOrEqual(a, b) => {
                match (
                    assignment.get(a).and_then(|v| v.as_float()),
                    assignment.get(b).and_then(|v| v.as_float()),
                ) {
                    (Some(va), Some(vb)) => va <= vb,
                    _ => true,
                }
            }
            Constraint::GreaterThan(a, b) => {
                match (
                    assignment.get(a).and_then(|v| v.as_float()),
                    assignment.get(b).and_then(|v| v.as_float()),
                ) {
                    (Some(va), Some(vb)) => va > vb,
                    _ => true,
                }
            }
            Constraint::GreaterOrEqual(a, b) => {
                match (
                    assignment.get(a).and_then(|v| v.as_float()),
                    assignment.get(b).and_then(|v| v.as_float()),
                ) {
                    (Some(va), Some(vb)) => va >= vb,
                    _ => true,
                }
            }
            Constraint::AllDifferent(vars) => {
                let values: Vec<_> = vars.iter().filter_map(|v| assignment.get(v)).collect();
                let unique: HashSet<_> = values.iter().collect();
                values.len() == unique.len()
            }
            Constraint::Sum { vars, equals } => {
                let sum: Option<i64> = vars.iter().try_fold(0i64, |acc, v| {
                    assignment
                        .get(v)
                        .and_then(|val| val.as_int())
                        .map(|n| acc + n)
                });
                sum.map_or(true, |s| s == *equals)
            }
            Constraint::Custom(f) => f(assignment),
        }
    }
}

/// Variable definition.
#[derive(Debug, Clone)]
pub struct Variable {
    /// Variable name.
    pub name: String,
    /// Variable domain.
    pub domain: Domain,
}

impl Variable {
    /// Create new variable.
    pub fn new(name: impl Into<String>, domain: Domain) -> Self {
        Self {
            name: name.into(),
            domain,
        }
    }
}

/// Constraint satisfaction problem.
pub struct Problem {
    variables: HashMap<String, Variable>,
    constraints: Vec<Constraint>,
}

impl Problem {
    /// Create new problem.
    pub fn new() -> Self {
        Self {
            variables: HashMap::new(),
            constraints: Vec::new(),
        }
    }

    /// Add variable.
    pub fn add_variable(&mut self, variable: Variable) {
        self.variables.insert(variable.name.clone(), variable);
    }

    /// Add constraint.
    pub fn add_constraint(&mut self, constraint: Constraint) {
        self.constraints.push(constraint);
    }

    /// Get variable.
    pub fn variable(&self, name: &str) -> Option<&Variable> {
        self.variables.get(name)
    }

    /// Get all variables.
    pub fn variables(&self) -> impl Iterator<Item = &Variable> {
        self.variables.values()
    }

    /// Get all constraints.
    pub fn constraints(&self) -> &[Constraint] {
        &self.constraints
    }

    /// Check if assignment is consistent.
    pub fn is_consistent(&self, assignment: &HashMap<String, Value>) -> bool {
        // Check domain constraints
        for (name, value) in assignment {
            if let Some(var) = self.variables.get(name) {
                if !var.domain.contains(value) {
                    return false;
                }
            }
        }

        // Check all constraints
        self.constraints.iter().all(|c| c.is_satisfied(assignment))
    }

    /// Check if assignment is complete.
    pub fn is_complete(&self, assignment: &HashMap<String, Value>) -> bool {
        self.variables.keys().all(|v| assignment.contains_key(v))
    }

    /// Check if assignment is a solution.
    pub fn is_solution(&self, assignment: &HashMap<String, Value>) -> bool {
        self.is_complete(assignment) && self.is_consistent(assignment)
    }
}

impl Default for Problem {
    fn default() -> Self {
        Self::new()
    }
}

/// Simple backtracking solver.
pub struct Solver {
    max_iterations: usize,
}

impl Solver {
    /// Create new solver.
    pub fn new() -> Self {
        Self {
            max_iterations: 1_000_000,
        }
    }

    /// Set maximum iterations.
    pub fn max_iterations(mut self, max: usize) -> Self {
        self.max_iterations = max;
        self
    }

    /// Solve the problem.
    pub fn solve(&self, problem: &Problem) -> Option<HashMap<String, Value>> {
        let mut assignment = HashMap::new();
        let var_names: Vec<_> = problem.variables.keys().cloned().collect();

        let mut iterations = 0;
        if self.backtrack(problem, &var_names, 0, &mut assignment, &mut iterations) {
            Some(assignment)
        } else {
            None
        }
    }

    fn backtrack(
        &self,
        problem: &Problem,
        var_names: &[String],
        index: usize,
        assignment: &mut HashMap<String, Value>,
        iterations: &mut usize,
    ) -> bool {
        *iterations += 1;
        if *iterations > self.max_iterations {
            return false;
        }

        if index >= var_names.len() {
            return problem.is_consistent(assignment);
        }

        let var_name = &var_names[index];
        let var = match problem.variable(var_name) {
            Some(v) => v,
            None => return false,
        };

        for value in self.domain_values(&var.domain) {
            assignment.insert(var_name.clone(), value);

            if problem.is_consistent(assignment) {
                if self.backtrack(problem, var_names, index + 1, assignment, iterations) {
                    return true;
                }
            }

            assignment.remove(var_name);
        }

        false
    }

    fn domain_values(&self, domain: &Domain) -> Vec<Value> {
        match domain {
            Domain::IntRange { min, max } => (*min..=*max).map(Value::Int).collect(),
            Domain::IntSet(set) => set.iter().map(|v| Value::Int(*v)).collect(),
            Domain::Boolean => vec![Value::Bool(true), Value::Bool(false)],
            Domain::StringSet(set) => set.iter().map(|v| Value::String(v.clone())).collect(),
            Domain::Any => vec![],
        }
    }
}

impl Default for Solver {
    fn default() -> Self {
        Self::new()
    }
}

/// Constraint builder for fluent API.
pub struct ConstraintBuilder {
    problem: Problem,
}

impl ConstraintBuilder {
    /// Create new builder.
    pub fn new() -> Self {
        Self {
            problem: Problem::new(),
        }
    }

    /// Add integer variable with range.
    pub fn int_var(mut self, name: impl Into<String>, min: i64, max: i64) -> Self {
        self.problem
            .add_variable(Variable::new(name, Domain::int_range(min, max)));
        self
    }

    /// Add boolean variable.
    pub fn bool_var(mut self, name: impl Into<String>) -> Self {
        self.problem
            .add_variable(Variable::new(name, Domain::Boolean));
        self
    }

    /// Add equality constraint.
    pub fn equal(mut self, a: impl Into<String>, b: impl Into<String>) -> Self {
        self.problem
            .add_constraint(Constraint::Equal(a.into(), b.into()));
        self
    }

    /// Add inequality constraint.
    pub fn not_equal(mut self, a: impl Into<String>, b: impl Into<String>) -> Self {
        self.problem
            .add_constraint(Constraint::NotEqual(a.into(), b.into()));
        self
    }

    /// Add less than constraint.
    pub fn less_than(mut self, a: impl Into<String>, b: impl Into<String>) -> Self {
        self.problem
            .add_constraint(Constraint::LessThan(a.into(), b.into()));
        self
    }

    /// Add all different constraint.
    pub fn all_different(mut self, vars: Vec<String>) -> Self {
        self.problem.add_constraint(Constraint::AllDifferent(vars));
        self
    }

    /// Build the problem.
    pub fn build(self) -> Problem {
        self.problem
    }
}

impl Default for ConstraintBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_domain_contains() {
        let domain = Domain::int_range(1, 10);
        assert!(domain.contains(&Value::Int(5)));
        assert!(!domain.contains(&Value::Int(11)));
    }

    #[test]
    fn test_constraint_equal() {
        let c = Constraint::Equal("x".to_string(), "y".to_string());
        let mut assignment = HashMap::new();
        assignment.insert("x".to_string(), Value::Int(5));
        assignment.insert("y".to_string(), Value::Int(5));
        assert!(c.is_satisfied(&assignment));

        assignment.insert("y".to_string(), Value::Int(6));
        assert!(!c.is_satisfied(&assignment));
    }

    #[test]
    fn test_simple_solve() {
        let problem = ConstraintBuilder::new()
            .int_var("x", 1, 3)
            .int_var("y", 1, 3)
            .not_equal("x", "y")
            .less_than("x", "y")
            .build();

        let solver = Solver::new();
        let solution = solver.solve(&problem);

        assert!(solution.is_some());
        let sol = solution.unwrap();
        let x = sol.get("x").unwrap().as_int().unwrap();
        let y = sol.get("y").unwrap().as_int().unwrap();
        assert!(x < y);
    }

    #[test]
    fn test_all_different() {
        let problem = ConstraintBuilder::new()
            .int_var("a", 1, 3)
            .int_var("b", 1, 3)
            .int_var("c", 1, 3)
            .all_different(vec!["a".to_string(), "b".to_string(), "c".to_string()])
            .build();

        let solver = Solver::new();
        let solution = solver.solve(&problem);

        assert!(solution.is_some());
        let sol = solution.unwrap();
        let a = sol.get("a").unwrap();
        let b = sol.get("b").unwrap();
        let c = sol.get("c").unwrap();
        assert!(a != b && b != c && a != c);
    }
}
