//! Rule engine and evaluation for drbot.
//!
//! This crate provides:
//! - Rule definitions
//! - Condition evaluation
//! - Rule engine
//! - Action execution

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;

/// Rule error types.
#[derive(Error, Debug)]
pub enum RuleError {
    #[error("Condition evaluation failed: {0}")]
    EvaluationFailed(String),

    #[error("Action execution failed: {0}")]
    ActionFailed(String),

    #[error("Rule not found: {0}")]
    RuleNotFound(String),

    #[error("Invalid rule: {0}")]
    InvalidRule(String),
}

/// Result type for rule operations.
pub type Result<T> = std::result::Result<T, RuleError>;

/// Comparison operators.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Operator {
    /// Equal.
    Eq,
    /// Not equal.
    Ne,
    /// Less than.
    Lt,
    /// Less than or equal.
    Le,
    /// Greater than.
    Gt,
    /// Greater than or equal.
    Ge,
    /// Contains (for strings/arrays).
    Contains,
    /// Starts with.
    StartsWith,
    /// Ends with.
    EndsWith,
    /// Matches regex.
    Matches,
    /// In set.
    In,
    /// Not in set.
    NotIn,
}

/// Condition for rule evaluation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Condition {
    /// Compare field with value.
    Compare {
        field: String,
        operator: Operator,
        value: JsonValue,
    },
    /// All conditions must be true.
    And(Vec<Condition>),
    /// Any condition must be true.
    Or(Vec<Condition>),
    /// Negate condition.
    Not(Box<Condition>),
    /// Always true.
    True,
    /// Always false.
    False,
}

impl Condition {
    /// Create comparison condition.
    pub fn compare(field: impl Into<String>, operator: Operator, value: JsonValue) -> Self {
        Condition::Compare {
            field: field.into(),
            operator,
            value,
        }
    }

    /// Create AND condition.
    pub fn and(conditions: Vec<Condition>) -> Self {
        Condition::And(conditions)
    }

    /// Create OR condition.
    pub fn or(conditions: Vec<Condition>) -> Self {
        Condition::Or(conditions)
    }

    /// Create NOT condition.
    pub fn not(condition: Condition) -> Self {
        Condition::Not(Box::new(condition))
    }

    /// Evaluate condition against context.
    pub fn evaluate(&self, context: &Context) -> bool {
        match self {
            Condition::Compare {
                field,
                operator,
                value,
            } => {
                if let Some(field_value) = context.get(field) {
                    compare_values(field_value, operator, value)
                } else {
                    false
                }
            }
            Condition::And(conditions) => conditions.iter().all(|c| c.evaluate(context)),
            Condition::Or(conditions) => conditions.iter().any(|c| c.evaluate(context)),
            Condition::Not(condition) => !condition.evaluate(context),
            Condition::True => true,
            Condition::False => false,
        }
    }
}

fn compare_values(left: &JsonValue, operator: &Operator, right: &JsonValue) -> bool {
    match operator {
        Operator::Eq => left == right,
        Operator::Ne => left != right,
        Operator::Lt => compare_ord(left, right) == Some(std::cmp::Ordering::Less),
        Operator::Le => matches!(
            compare_ord(left, right),
            Some(std::cmp::Ordering::Less | std::cmp::Ordering::Equal)
        ),
        Operator::Gt => compare_ord(left, right) == Some(std::cmp::Ordering::Greater),
        Operator::Ge => matches!(
            compare_ord(left, right),
            Some(std::cmp::Ordering::Greater | std::cmp::Ordering::Equal)
        ),
        Operator::Contains => {
            if let (JsonValue::String(s), JsonValue::String(sub)) = (left, right) {
                s.contains(sub.as_str())
            } else if let JsonValue::Array(arr) = left {
                arr.contains(right)
            } else {
                false
            }
        }
        Operator::StartsWith => {
            if let (JsonValue::String(s), JsonValue::String(prefix)) = (left, right) {
                s.starts_with(prefix.as_str())
            } else {
                false
            }
        }
        Operator::EndsWith => {
            if let (JsonValue::String(s), JsonValue::String(suffix)) = (left, right) {
                s.ends_with(suffix.as_str())
            } else {
                false
            }
        }
        Operator::Matches => {
            // Simple pattern matching (not full regex for simplicity)
            if let (JsonValue::String(s), JsonValue::String(pattern)) = (left, right) {
                s.contains(pattern.as_str())
            } else {
                false
            }
        }
        Operator::In => {
            if let JsonValue::Array(arr) = right {
                arr.contains(left)
            } else {
                false
            }
        }
        Operator::NotIn => {
            if let JsonValue::Array(arr) = right {
                !arr.contains(left)
            } else {
                true
            }
        }
    }
}

fn compare_ord(left: &JsonValue, right: &JsonValue) -> Option<std::cmp::Ordering> {
    match (left, right) {
        (JsonValue::Number(a), JsonValue::Number(b)) => {
            let a = a.as_f64()?;
            let b = b.as_f64()?;
            a.partial_cmp(&b)
        }
        (JsonValue::String(a), JsonValue::String(b)) => Some(a.cmp(b)),
        _ => None,
    }
}

/// Evaluation context.
#[derive(Debug, Clone, Default)]
pub struct Context {
    data: HashMap<String, JsonValue>,
}

impl Context {
    /// Create new context.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create from JSON object.
    pub fn from_json(value: JsonValue) -> Self {
        let mut ctx = Self::new();
        if let JsonValue::Object(map) = value {
            for (k, v) in map {
                ctx.data.insert(k, v);
            }
        }
        ctx
    }

    /// Set value.
    pub fn set(&mut self, key: impl Into<String>, value: JsonValue) {
        self.data.insert(key.into(), value);
    }

    /// Get value.
    pub fn get(&self, key: &str) -> Option<&JsonValue> {
        // Support nested access with dot notation
        let parts: Vec<&str> = key.split('.').collect();
        let mut current = self.data.get(parts[0])?;

        for part in &parts[1..] {
            if let JsonValue::Object(map) = current {
                current = map.get(*part)?;
            } else {
                return None;
            }
        }

        Some(current)
    }

    /// Check if key exists.
    pub fn contains(&self, key: &str) -> bool {
        self.get(key).is_some()
    }
}

/// Action type.
pub type ActionFn = Arc<dyn Fn(&Context) -> Result<()> + Send + Sync>;

/// Rule definition.
pub struct Rule {
    /// Rule ID.
    pub id: String,
    /// Rule name.
    pub name: String,
    /// Description.
    pub description: Option<String>,
    /// Priority (higher runs first).
    pub priority: i32,
    /// Condition.
    pub condition: Condition,
    /// Actions to execute.
    actions: Vec<ActionFn>,
    /// Whether to stop processing after this rule.
    pub stop: bool,
}

impl Rule {
    /// Create new rule.
    pub fn new(id: impl Into<String>, name: impl Into<String>, condition: Condition) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            description: None,
            priority: 0,
            condition,
            actions: Vec::new(),
            stop: false,
        }
    }

    /// Set description.
    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Set priority.
    pub fn priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }

    /// Add action.
    pub fn action<F>(mut self, action: F) -> Self
    where
        F: Fn(&Context) -> Result<()> + Send + Sync + 'static,
    {
        self.actions.push(Arc::new(action));
        self
    }

    /// Set stop flag.
    pub fn stop_after(mut self) -> Self {
        self.stop = true;
        self
    }

    /// Evaluate condition.
    pub fn matches(&self, context: &Context) -> bool {
        self.condition.evaluate(context)
    }

    /// Execute actions.
    pub fn execute(&self, context: &Context) -> Result<()> {
        for action in &self.actions {
            action(context)?;
        }
        Ok(())
    }
}

/// Rule engine.
pub struct RuleEngine {
    rules: Vec<Rule>,
}

impl RuleEngine {
    /// Create new engine.
    pub fn new() -> Self {
        Self { rules: Vec::new() }
    }

    /// Add rule.
    pub fn add_rule(&mut self, rule: Rule) {
        self.rules.push(rule);
        self.rules.sort_by(|a, b| b.priority.cmp(&a.priority));
    }

    /// Remove rule by ID.
    pub fn remove_rule(&mut self, id: &str) -> bool {
        let len_before = self.rules.len();
        self.rules.retain(|r| r.id != id);
        self.rules.len() < len_before
    }

    /// Get rule by ID.
    pub fn get_rule(&self, id: &str) -> Option<&Rule> {
        self.rules.iter().find(|r| r.id == id)
    }

    /// Evaluate and execute matching rules.
    pub fn evaluate(&self, context: &Context) -> Result<EvaluationResult> {
        let mut result = EvaluationResult::new();

        for rule in &self.rules {
            if rule.matches(context) {
                result.matched_rules.push(rule.id.clone());

                match rule.execute(context) {
                    Ok(_) => result.executed_rules.push(rule.id.clone()),
                    Err(e) => result.errors.push((rule.id.clone(), e)),
                }

                if rule.stop {
                    break;
                }
            }
        }

        Ok(result)
    }

    /// Get all rules.
    pub fn rules(&self) -> &[Rule] {
        &self.rules
    }

    /// Get rule count.
    pub fn count(&self) -> usize {
        self.rules.len()
    }
}

impl Default for RuleEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Evaluation result.
#[derive(Debug)]
pub struct EvaluationResult {
    /// Rules that matched.
    pub matched_rules: Vec<String>,
    /// Rules that executed successfully.
    pub executed_rules: Vec<String>,
    /// Errors from rule execution.
    pub errors: Vec<(String, RuleError)>,
}

impl EvaluationResult {
    /// Create new result.
    pub fn new() -> Self {
        Self {
            matched_rules: Vec::new(),
            executed_rules: Vec::new(),
            errors: Vec::new(),
        }
    }

    /// Check if any rules matched.
    pub fn has_matches(&self) -> bool {
        !self.matched_rules.is_empty()
    }

    /// Check if all executed successfully.
    pub fn is_success(&self) -> bool {
        self.errors.is_empty()
    }
}

impl Default for EvaluationResult {
    fn default() -> Self {
        Self::new()
    }
}

/// Rule builder for declarative rule creation.
pub struct RuleBuilder {
    id: String,
    name: String,
    description: Option<String>,
    priority: i32,
    conditions: Vec<Condition>,
    actions: Vec<ActionFn>,
    stop: bool,
}

impl RuleBuilder {
    /// Create new builder.
    pub fn new(id: impl Into<String>) -> Self {
        let id = id.into();
        Self {
            name: id.clone(),
            id,
            description: None,
            priority: 0,
            conditions: Vec::new(),
            actions: Vec::new(),
            stop: false,
        }
    }

    /// Set name.
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    /// Set description.
    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Set priority.
    pub fn priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }

    /// Add condition.
    pub fn when(mut self, condition: Condition) -> Self {
        self.conditions.push(condition);
        self
    }

    /// Add field equals condition.
    pub fn when_eq(mut self, field: impl Into<String>, value: impl Into<JsonValue>) -> Self {
        self.conditions
            .push(Condition::compare(field, Operator::Eq, value.into()));
        self
    }

    /// Add action.
    pub fn then<F>(mut self, action: F) -> Self
    where
        F: Fn(&Context) -> Result<()> + Send + Sync + 'static,
    {
        self.actions.push(Arc::new(action));
        self
    }

    /// Stop after this rule.
    pub fn stop(mut self) -> Self {
        self.stop = true;
        self
    }

    /// Build the rule.
    pub fn build(self) -> Rule {
        let condition = if self.conditions.is_empty() {
            Condition::True
        } else if self.conditions.len() == 1 {
            self.conditions.into_iter().next().unwrap()
        } else {
            Condition::And(self.conditions)
        };

        Rule {
            id: self.id,
            name: self.name,
            description: self.description,
            priority: self.priority,
            condition,
            actions: self.actions,
            stop: self.stop,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_condition_compare() {
        let mut ctx = Context::new();
        ctx.set("age", serde_json::json!(25));
        ctx.set("name", serde_json::json!("Alice"));

        let c1 = Condition::compare("age", Operator::Gt, serde_json::json!(18));
        assert!(c1.evaluate(&ctx));

        let c2 = Condition::compare("name", Operator::Eq, serde_json::json!("Alice"));
        assert!(c2.evaluate(&ctx));
    }

    #[test]
    fn test_condition_and_or() {
        let mut ctx = Context::new();
        ctx.set("x", serde_json::json!(10));
        ctx.set("y", serde_json::json!(20));

        let c = Condition::and(vec![
            Condition::compare("x", Operator::Gt, serde_json::json!(5)),
            Condition::compare("y", Operator::Lt, serde_json::json!(30)),
        ]);
        assert!(c.evaluate(&ctx));

        let c2 = Condition::or(vec![
            Condition::compare("x", Operator::Gt, serde_json::json!(100)),
            Condition::compare("y", Operator::Lt, serde_json::json!(30)),
        ]);
        assert!(c2.evaluate(&ctx));
    }

    #[test]
    fn test_rule_engine() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();

        let mut engine = RuleEngine::new();

        let rule = RuleBuilder::new("r1")
            .name("Test Rule")
            .when_eq("status", "active")
            .then(move |_ctx| {
                counter_clone.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
            .build();

        engine.add_rule(rule);

        let mut ctx = Context::new();
        ctx.set("status", serde_json::json!("active"));

        let result = engine.evaluate(&ctx).unwrap();
        assert!(result.has_matches());
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_nested_context() {
        let mut ctx = Context::new();
        ctx.set(
            "user",
            serde_json::json!({
                "name": "Alice",
                "address": {
                    "city": "NYC"
                }
            }),
        );

        assert_eq!(ctx.get("user.name"), Some(&serde_json::json!("Alice")));
        assert_eq!(
            ctx.get("user.address.city"),
            Some(&serde_json::json!("NYC"))
        );
    }
}
