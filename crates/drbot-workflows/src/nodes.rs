//! Workflow nodes for advanced flow control.
//!
//! Provides:
//! - Conditional branching
//! - Parallel execution
//! - Loops and iterations
//! - Sub-workflows

use crate::{Action, ActionResult, Result, WorkflowError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// A workflow node that can be part of a flow graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WorkflowNode {
    /// Execute a single action.
    Action(ActionNode),
    /// Conditional branching.
    Condition(ConditionNode),
    /// Parallel execution of multiple branches.
    Parallel(ParallelNode),
    /// Loop/iteration.
    Loop(LoopNode),
    /// Sub-workflow execution.
    SubWorkflow(SubWorkflowNode),
    /// Wait for an event or time.
    Wait(WaitNode),
    /// Transform data.
    Transform(TransformNode),
    /// Agent execution.
    Agent(AgentNode),
}

/// Simple action node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionNode {
    pub id: Uuid,
    pub action: Action,
    pub next: Option<Uuid>,
}

impl ActionNode {
    /// Create a new action node.
    pub fn new(action: Action) -> Self {
        Self {
            id: Uuid::new_v4(),
            action,
            next: None,
        }
    }

    /// Set the next node.
    pub fn then(mut self, next: Uuid) -> Self {
        self.next = Some(next);
        self
    }
}

/// Conditional branching node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConditionNode {
    pub id: Uuid,
    /// The condition to evaluate.
    pub condition: Condition,
    /// Branch to take if condition is true.
    pub then_branch: Uuid,
    /// Branch to take if condition is false.
    pub else_branch: Option<Uuid>,
}

impl ConditionNode {
    /// Create a new condition node.
    pub fn new(condition: Condition, then_branch: Uuid) -> Self {
        Self {
            id: Uuid::new_v4(),
            condition,
            then_branch,
            else_branch: None,
        }
    }

    /// Set the else branch.
    pub fn otherwise(mut self, else_branch: Uuid) -> Self {
        self.else_branch = Some(else_branch);
        self
    }
}

/// A condition that can be evaluated.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Condition {
    /// Compare a variable to a value.
    Compare {
        variable: String,
        operator: CompareOp,
        value: serde_json::Value,
    },
    /// Check if a variable exists.
    Exists { variable: String },
    /// Check if a variable is empty.
    IsEmpty { variable: String },
    /// Logical AND of multiple conditions.
    And(Vec<Condition>),
    /// Logical OR of multiple conditions.
    Or(Vec<Condition>),
    /// Logical NOT of a condition.
    Not(Box<Condition>),
    /// Always true.
    Always,
    /// Always false.
    Never,
    /// Custom expression.
    Expression { expr: String },
}

/// Comparison operators.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompareOp {
    Equals,
    NotEquals,
    GreaterThan,
    GreaterOrEqual,
    LessThan,
    LessOrEqual,
    Contains,
    StartsWith,
    EndsWith,
    Matches,
}

impl Condition {
    /// Evaluate the condition against a context.
    pub fn evaluate(&self, context: &WorkflowContext) -> bool {
        match self {
            Condition::Compare {
                variable,
                operator,
                value,
            } => context
                .get_variable(variable)
                .map(|v| compare_values(v, value, *operator))
                .unwrap_or(false),
            Condition::Exists { variable } => context.has_variable(variable),
            Condition::IsEmpty { variable } => context
                .get_variable(variable)
                .map(|v| is_empty_value(v))
                .unwrap_or(true),
            Condition::And(conditions) => conditions.iter().all(|c| c.evaluate(context)),
            Condition::Or(conditions) => conditions.iter().any(|c| c.evaluate(context)),
            Condition::Not(condition) => !condition.evaluate(context),
            Condition::Always => true,
            Condition::Never => false,
            Condition::Expression { expr } => {
                // Simple expression evaluation
                evaluate_expression(expr, context)
            }
        }
    }
}

/// Compare two JSON values.
fn compare_values(a: &serde_json::Value, b: &serde_json::Value, op: CompareOp) -> bool {
    match op {
        CompareOp::Equals => a == b,
        CompareOp::NotEquals => a != b,
        CompareOp::GreaterThan => compare_numeric(a, b, |x, y| x > y),
        CompareOp::GreaterOrEqual => compare_numeric(a, b, |x, y| x >= y),
        CompareOp::LessThan => compare_numeric(a, b, |x, y| x < y),
        CompareOp::LessOrEqual => compare_numeric(a, b, |x, y| x <= y),
        CompareOp::Contains => a
            .as_str()
            .and_then(|s| b.as_str().map(|t| s.contains(t)))
            .unwrap_or(false),
        CompareOp::StartsWith => a
            .as_str()
            .and_then(|s| b.as_str().map(|t| s.starts_with(t)))
            .unwrap_or(false),
        CompareOp::EndsWith => a
            .as_str()
            .and_then(|s| b.as_str().map(|t| s.ends_with(t)))
            .unwrap_or(false),
        CompareOp::Matches => {
            // Regex matching
            a.as_str()
                .and_then(|s| {
                    b.as_str().and_then(|pattern| {
                        regex::Regex::new(pattern).ok().map(|re| re.is_match(s))
                    })
                })
                .unwrap_or(false)
        }
    }
}

/// Compare numeric values.
fn compare_numeric<F>(a: &serde_json::Value, b: &serde_json::Value, f: F) -> bool
where
    F: Fn(f64, f64) -> bool,
{
    match (a.as_f64(), b.as_f64()) {
        (Some(x), Some(y)) => f(x, y),
        _ => false,
    }
}

/// Check if a value is empty.
fn is_empty_value(v: &serde_json::Value) -> bool {
    match v {
        serde_json::Value::Null => true,
        serde_json::Value::String(s) => s.is_empty(),
        serde_json::Value::Array(a) => a.is_empty(),
        serde_json::Value::Object(o) => o.is_empty(),
        _ => false,
    }
}

/// Evaluate a simple expression.
fn evaluate_expression(expr: &str, context: &WorkflowContext) -> bool {
    // Simple variable check: "${var}" evaluates to truthy
    if expr.starts_with("${") && expr.ends_with("}") {
        let var = &expr[2..expr.len() - 1];
        return context
            .get_variable(var)
            .map(|v| !is_empty_value(v) && !v.is_null() && v != &serde_json::Value::Bool(false))
            .unwrap_or(false);
    }
    // Default to false for complex expressions
    false
}

/// Parallel execution node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParallelNode {
    pub id: Uuid,
    /// Branches to execute in parallel.
    pub branches: Vec<Uuid>,
    /// How to handle branch completion.
    pub join_mode: JoinMode,
    /// Next node after all branches complete.
    pub next: Option<Uuid>,
}

impl ParallelNode {
    /// Create a new parallel node.
    pub fn new(branches: Vec<Uuid>) -> Self {
        Self {
            id: Uuid::new_v4(),
            branches,
            join_mode: JoinMode::All,
            next: None,
        }
    }

    /// Set join mode.
    pub fn join(mut self, mode: JoinMode) -> Self {
        self.join_mode = mode;
        self
    }

    /// Set next node.
    pub fn then(mut self, next: Uuid) -> Self {
        self.next = Some(next);
        self
    }
}

/// How to join parallel branches.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JoinMode {
    /// Wait for all branches to complete.
    All,
    /// Continue when any branch completes.
    Any,
    /// Continue when N branches complete.
    N(usize),
    /// Don't wait, fire and forget.
    None,
}

/// Loop node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopNode {
    pub id: Uuid,
    /// Loop type.
    pub loop_type: LoopType,
    /// Body of the loop.
    pub body: Uuid,
    /// Next node after loop completes.
    pub next: Option<Uuid>,
}

impl LoopNode {
    /// Create a new while loop.
    pub fn while_loop(condition: Condition, body: Uuid) -> Self {
        Self {
            id: Uuid::new_v4(),
            loop_type: LoopType::While { condition },
            body,
            next: None,
        }
    }

    /// Create a for-each loop.
    pub fn for_each(variable: &str, collection: &str, body: Uuid) -> Self {
        Self {
            id: Uuid::new_v4(),
            loop_type: LoopType::ForEach {
                variable: variable.to_string(),
                collection: collection.to_string(),
            },
            body,
            next: None,
        }
    }

    /// Create a counted loop.
    pub fn times(count: usize, body: Uuid) -> Self {
        Self {
            id: Uuid::new_v4(),
            loop_type: LoopType::Times { count },
            body,
            next: None,
        }
    }

    /// Set next node.
    pub fn then(mut self, next: Uuid) -> Self {
        self.next = Some(next);
        self
    }
}

/// Loop types.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LoopType {
    /// While condition is true.
    While { condition: Condition },
    /// For each item in collection.
    ForEach {
        variable: String,
        collection: String,
    },
    /// Execute N times.
    Times { count: usize },
}

/// Sub-workflow node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubWorkflowNode {
    pub id: Uuid,
    /// Workflow ID to execute.
    pub workflow_id: Uuid,
    /// Input variables to pass.
    pub inputs: HashMap<String, String>,
    /// Output variable mappings.
    pub outputs: HashMap<String, String>,
    /// Next node.
    pub next: Option<Uuid>,
}

impl SubWorkflowNode {
    /// Create a new sub-workflow node.
    pub fn new(workflow_id: Uuid) -> Self {
        Self {
            id: Uuid::new_v4(),
            workflow_id,
            inputs: HashMap::new(),
            outputs: HashMap::new(),
            next: None,
        }
    }

    /// Add input mapping.
    pub fn input(mut self, local: &str, remote: &str) -> Self {
        self.inputs.insert(local.to_string(), remote.to_string());
        self
    }

    /// Add output mapping.
    pub fn output(mut self, remote: &str, local: &str) -> Self {
        self.outputs.insert(remote.to_string(), local.to_string());
        self
    }
}

/// Wait node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WaitNode {
    pub id: Uuid,
    /// What to wait for.
    pub wait_for: WaitFor,
    /// Timeout in seconds.
    pub timeout_secs: Option<u64>,
    /// Next node.
    pub next: Option<Uuid>,
}

/// What to wait for.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WaitFor {
    /// Wait for a duration.
    Duration { seconds: u64 },
    /// Wait for an event.
    Event {
        event_type: String,
        filter: Option<serde_json::Value>,
    },
    /// Wait for user input.
    UserInput { prompt: String },
    /// Wait for a condition.
    Condition { condition: Condition },
    /// Wait for a webhook.
    Webhook { path: String },
}

/// Transform node for data manipulation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransformNode {
    pub id: Uuid,
    /// Transformations to apply.
    pub transforms: Vec<Transform>,
    /// Next node.
    pub next: Option<Uuid>,
}

/// A data transformation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Transform {
    /// Set a variable.
    Set {
        variable: String,
        value: serde_json::Value,
    },
    /// Copy a variable.
    Copy { from: String, to: String },
    /// Delete a variable.
    Delete { variable: String },
    /// Map over a collection.
    Map {
        collection: String,
        item_var: String,
        expression: String,
        output: String,
    },
    /// Filter a collection.
    Filter {
        collection: String,
        condition: Condition,
        output: String,
    },
    /// Reduce a collection.
    Reduce {
        collection: String,
        accumulator: String,
        item_var: String,
        expression: String,
    },
    /// Merge objects.
    Merge {
        sources: Vec<String>,
        output: String,
    },
    /// Parse JSON string.
    ParseJson { source: String, output: String },
    /// Stringify to JSON.
    ToJson { source: String, output: String },
}

/// Agent execution node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentNode {
    pub id: Uuid,
    /// Task for the agent.
    pub task: String,
    /// Agent configuration override.
    pub config: Option<serde_json::Value>,
    /// Variable to store result.
    pub output_variable: String,
    /// Next node.
    pub next: Option<Uuid>,
}

impl AgentNode {
    /// Create a new agent node.
    pub fn new(task: &str, output_variable: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            task: task.to_string(),
            config: None,
            output_variable: output_variable.to_string(),
            next: None,
        }
    }
}

/// Workflow execution context with variables.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkflowContext {
    /// Variables in the workflow.
    pub variables: HashMap<String, serde_json::Value>,
    /// Parent context (for sub-workflows).
    pub parent: Option<Box<WorkflowContext>>,
    /// Execution metadata.
    pub metadata: HashMap<String, serde_json::Value>,
}

impl WorkflowContext {
    /// Create a new context.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set a variable.
    pub fn set(&mut self, name: &str, value: serde_json::Value) {
        self.variables.insert(name.to_string(), value);
    }

    /// Get a variable.
    pub fn get_variable(&self, name: &str) -> Option<&serde_json::Value> {
        self.variables
            .get(name)
            .or_else(|| self.parent.as_ref().and_then(|p| p.get_variable(name)))
    }

    /// Check if variable exists.
    pub fn has_variable(&self, name: &str) -> bool {
        self.variables.contains_key(name)
            || self
                .parent
                .as_ref()
                .map(|p| p.has_variable(name))
                .unwrap_or(false)
    }

    /// Delete a variable.
    pub fn delete(&mut self, name: &str) {
        self.variables.remove(name);
    }

    /// Create a child context.
    pub fn child(&self) -> Self {
        Self {
            variables: HashMap::new(),
            parent: Some(Box::new(self.clone())),
            metadata: HashMap::new(),
        }
    }

    /// Merge variables from another context.
    pub fn merge(&mut self, other: &WorkflowContext) {
        for (k, v) in &other.variables {
            self.variables.insert(k.clone(), v.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_condition_always() {
        let ctx = WorkflowContext::new();
        assert!(Condition::Always.evaluate(&ctx));
        assert!(!Condition::Never.evaluate(&ctx));
    }

    #[test]
    fn test_condition_exists() {
        let mut ctx = WorkflowContext::new();
        ctx.set("test", serde_json::json!("value"));

        assert!(Condition::Exists {
            variable: "test".to_string()
        }
        .evaluate(&ctx));
        assert!(!Condition::Exists {
            variable: "other".to_string()
        }
        .evaluate(&ctx));
    }

    #[test]
    fn test_condition_compare() {
        let mut ctx = WorkflowContext::new();
        ctx.set("count", serde_json::json!(5));

        let condition = Condition::Compare {
            variable: "count".to_string(),
            operator: CompareOp::GreaterThan,
            value: serde_json::json!(3),
        };
        assert!(condition.evaluate(&ctx));

        let condition = Condition::Compare {
            variable: "count".to_string(),
            operator: CompareOp::Equals,
            value: serde_json::json!(5),
        };
        assert!(condition.evaluate(&ctx));
    }

    #[test]
    fn test_condition_and_or() {
        let mut ctx = WorkflowContext::new();
        ctx.set("a", serde_json::json!(true));
        ctx.set("b", serde_json::json!(false));

        let and_cond = Condition::And(vec![
            Condition::Exists {
                variable: "a".to_string(),
            },
            Condition::Exists {
                variable: "b".to_string(),
            },
        ]);
        assert!(and_cond.evaluate(&ctx));

        let or_cond = Condition::Or(vec![
            Condition::Exists {
                variable: "c".to_string(),
            },
            Condition::Exists {
                variable: "a".to_string(),
            },
        ]);
        assert!(or_cond.evaluate(&ctx));
    }

    #[test]
    fn test_workflow_context() {
        let mut ctx = WorkflowContext::new();
        ctx.set("name", serde_json::json!("test"));
        ctx.set("count", serde_json::json!(42));

        assert!(ctx.has_variable("name"));
        assert_eq!(ctx.get_variable("name"), Some(&serde_json::json!("test")));

        let child = ctx.child();
        assert!(child.has_variable("name")); // Inherited
    }

    #[test]
    fn test_parallel_node() {
        let node = ParallelNode::new(vec![Uuid::new_v4(), Uuid::new_v4()]).join(JoinMode::All);
        assert_eq!(node.branches.len(), 2);
    }

    #[test]
    fn test_loop_node() {
        let body = Uuid::new_v4();
        let loop_node = LoopNode::times(5, body);
        match loop_node.loop_type {
            LoopType::Times { count } => assert_eq!(count, 5),
            _ => panic!("Wrong loop type"),
        }
    }
}
