//! State machine implementation for drbot.
//!
//! This crate provides:
//! - Finite state machines
//! - State transitions with guards
//! - Entry/exit actions
//! - Hierarchical states

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use uuid::Uuid;

/// State machine error types.
#[derive(Error, Debug)]
pub enum StateMachineError {
    #[error("Invalid transition from {from} to {to}")]
    InvalidTransition { from: String, to: String },

    #[error("State not found: {0}")]
    StateNotFound(String),

    #[error("Guard rejected transition: {0}")]
    GuardRejected(String),

    #[error("Action failed: {0}")]
    ActionFailed(String),

    #[error("Machine not started")]
    NotStarted,
}

/// Result type for state machine operations.
pub type Result<T> = std::result::Result<T, StateMachineError>;

/// A state in the machine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct State {
    /// State name.
    pub name: String,
    /// Whether this is the initial state.
    pub initial: bool,
    /// Whether this is a final state.
    pub final_state: bool,
    /// State metadata.
    pub metadata: HashMap<String, serde_json::Value>,
}

impl State {
    /// Create a new state.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            initial: false,
            final_state: false,
            metadata: HashMap::new(),
        }
    }

    /// Mark as initial state.
    pub fn initial(mut self) -> Self {
        self.initial = true;
        self
    }

    /// Mark as final state.
    pub fn final_state(mut self) -> Self {
        self.final_state = true;
        self
    }

    /// Add metadata.
    pub fn with_metadata(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.metadata.insert(key.into(), value);
        self
    }
}

/// A transition between states.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transition {
    /// Transition name/event.
    pub event: String,
    /// Source state.
    pub from: String,
    /// Target state.
    pub to: String,
    /// Guard condition name (optional).
    pub guard: Option<String>,
    /// Action name (optional).
    pub action: Option<String>,
}

impl Transition {
    /// Create a new transition.
    pub fn new(event: impl Into<String>, from: impl Into<String>, to: impl Into<String>) -> Self {
        Self {
            event: event.into(),
            from: from.into(),
            to: to.into(),
            guard: None,
            action: None,
        }
    }

    /// Set guard.
    pub fn with_guard(mut self, guard: impl Into<String>) -> Self {
        self.guard = Some(guard.into());
        self
    }

    /// Set action.
    pub fn with_action(mut self, action: impl Into<String>) -> Self {
        self.action = Some(action.into());
        self
    }
}

/// Transition context.
#[derive(Debug, Clone)]
pub struct TransitionContext {
    /// Current state.
    pub current_state: String,
    /// Target state.
    pub target_state: String,
    /// Event that triggered the transition.
    pub event: String,
    /// Additional data.
    pub data: serde_json::Value,
}

/// Guard function trait.
#[async_trait]
pub trait Guard: Send + Sync {
    /// Check if transition is allowed.
    async fn check(&self, ctx: &TransitionContext) -> Result<bool>;
}

/// Action function trait.
#[async_trait]
pub trait Action: Send + Sync {
    /// Execute the action.
    async fn execute(&self, ctx: &TransitionContext) -> Result<()>;
}

/// State machine definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateMachineDefinition {
    /// Machine name.
    pub name: String,
    /// States.
    pub states: HashMap<String, State>,
    /// Transitions.
    pub transitions: Vec<Transition>,
    /// Initial state.
    pub initial_state: Option<String>,
}

impl StateMachineDefinition {
    /// Create a new definition.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            states: HashMap::new(),
            transitions: Vec::new(),
            initial_state: None,
        }
    }

    /// Add a state.
    pub fn state(mut self, state: State) -> Self {
        if state.initial {
            self.initial_state = Some(state.name.clone());
        }
        self.states.insert(state.name.clone(), state);
        self
    }

    /// Add a transition.
    pub fn transition(mut self, transition: Transition) -> Self {
        self.transitions.push(transition);
        self
    }

    /// Validate the definition.
    pub fn validate(&self) -> Result<()> {
        // Check initial state exists
        if self.initial_state.is_none() {
            return Err(StateMachineError::StateNotFound(
                "No initial state defined".to_string(),
            ));
        }

        let initial = self.initial_state.as_ref().unwrap();
        if !self.states.contains_key(initial) {
            return Err(StateMachineError::StateNotFound(initial.clone()));
        }

        // Check all transitions reference valid states
        for t in &self.transitions {
            if !self.states.contains_key(&t.from) {
                return Err(StateMachineError::StateNotFound(t.from.clone()));
            }
            if !self.states.contains_key(&t.to) {
                return Err(StateMachineError::StateNotFound(t.to.clone()));
            }
        }

        Ok(())
    }

    /// Get transitions from a state.
    pub fn transitions_from(&self, state: &str) -> Vec<&Transition> {
        self.transitions
            .iter()
            .filter(|t| t.from == state)
            .collect()
    }

    /// Get transition for event from state.
    pub fn get_transition(&self, state: &str, event: &str) -> Option<&Transition> {
        self.transitions
            .iter()
            .find(|t| t.from == state && t.event == event)
    }
}

/// State machine instance.
pub struct StateMachine {
    definition: StateMachineDefinition,
    current_state: RwLock<Option<String>>,
    guards: RwLock<HashMap<String, Arc<dyn Guard>>>,
    actions: RwLock<HashMap<String, Arc<dyn Action>>>,
    history: RwLock<Vec<TransitionRecord>>,
}

/// Record of a transition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransitionRecord {
    /// Record ID.
    pub id: Uuid,
    /// From state.
    pub from: String,
    /// To state.
    pub to: String,
    /// Event.
    pub event: String,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
    /// Success.
    pub success: bool,
    /// Error message if failed.
    pub error: Option<String>,
}

impl StateMachine {
    /// Create a new state machine.
    pub fn new(definition: StateMachineDefinition) -> Result<Self> {
        definition.validate()?;

        Ok(Self {
            definition,
            current_state: RwLock::new(None),
            guards: RwLock::new(HashMap::new()),
            actions: RwLock::new(HashMap::new()),
            history: RwLock::new(Vec::new()),
        })
    }

    /// Register a guard.
    pub async fn register_guard(&self, name: impl Into<String>, guard: Arc<dyn Guard>) {
        let mut guards = self.guards.write().await;
        guards.insert(name.into(), guard);
    }

    /// Register an action.
    pub async fn register_action(&self, name: impl Into<String>, action: Arc<dyn Action>) {
        let mut actions = self.actions.write().await;
        actions.insert(name.into(), action);
    }

    /// Start the machine.
    pub async fn start(&self) -> Result<()> {
        let initial = self
            .definition
            .initial_state
            .as_ref()
            .ok_or_else(|| StateMachineError::StateNotFound("No initial state".to_string()))?;

        *self.current_state.write().await = Some(initial.clone());
        Ok(())
    }

    /// Get current state.
    pub async fn current_state(&self) -> Option<String> {
        self.current_state.read().await.clone()
    }

    /// Check if in a specific state.
    pub async fn is_in_state(&self, state: &str) -> bool {
        self.current_state.read().await.as_deref() == Some(state)
    }

    /// Get available events from current state.
    pub async fn available_events(&self) -> Vec<String> {
        let current = self.current_state.read().await;
        if let Some(state) = current.as_ref() {
            self.definition
                .transitions_from(state)
                .iter()
                .map(|t| t.event.clone())
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Trigger an event.
    pub async fn trigger(&self, event: &str) -> Result<()> {
        self.trigger_with_data(event, serde_json::Value::Null).await
    }

    /// Trigger an event with data.
    pub async fn trigger_with_data(&self, event: &str, data: serde_json::Value) -> Result<()> {
        let current = {
            let state = self.current_state.read().await;
            state.clone().ok_or(StateMachineError::NotStarted)?
        };

        let transition = self
            .definition
            .get_transition(&current, event)
            .ok_or_else(|| StateMachineError::InvalidTransition {
                from: current.clone(),
                to: format!("(event: {})", event),
            })?
            .clone();

        let ctx = TransitionContext {
            current_state: current.clone(),
            target_state: transition.to.clone(),
            event: event.to_string(),
            data,
        };

        // Check guard
        if let Some(guard_name) = &transition.guard {
            let guards = self.guards.read().await;
            if let Some(guard) = guards.get(guard_name) {
                if !guard.check(&ctx).await? {
                    let record = TransitionRecord {
                        id: Uuid::new_v4(),
                        from: current,
                        to: transition.to.clone(),
                        event: event.to_string(),
                        timestamp: Utc::now(),
                        success: false,
                        error: Some("Guard rejected".to_string()),
                    };
                    self.history.write().await.push(record);
                    return Err(StateMachineError::GuardRejected(guard_name.clone()));
                }
            }
        }

        // Execute action
        if let Some(action_name) = &transition.action {
            let actions = self.actions.read().await;
            if let Some(action) = actions.get(action_name) {
                action.execute(&ctx).await?;
            }
        }

        // Transition
        *self.current_state.write().await = Some(transition.to.clone());

        // Record
        let record = TransitionRecord {
            id: Uuid::new_v4(),
            from: current,
            to: transition.to,
            event: event.to_string(),
            timestamp: Utc::now(),
            success: true,
            error: None,
        };
        self.history.write().await.push(record);

        Ok(())
    }

    /// Check if transition is possible.
    pub async fn can_trigger(&self, event: &str) -> bool {
        let current = self.current_state.read().await;
        if let Some(state) = current.as_ref() {
            self.definition.get_transition(state, event).is_some()
        } else {
            false
        }
    }

    /// Get transition history.
    pub async fn history(&self) -> Vec<TransitionRecord> {
        self.history.read().await.clone()
    }

    /// Check if in final state.
    pub async fn is_finished(&self) -> bool {
        let current = self.current_state.read().await;
        if let Some(state) = current.as_ref() {
            self.definition
                .states
                .get(state)
                .map(|s| s.final_state)
                .unwrap_or(false)
        } else {
            false
        }
    }
}

/// Simple guard that always allows.
pub struct AlwaysAllow;

#[async_trait]
impl Guard for AlwaysAllow {
    async fn check(&self, _ctx: &TransitionContext) -> Result<bool> {
        Ok(true)
    }
}

/// Simple guard that always denies.
pub struct AlwaysDeny;

#[async_trait]
impl Guard for AlwaysDeny {
    async fn check(&self, _ctx: &TransitionContext) -> Result<bool> {
        Ok(false)
    }
}

/// No-op action.
pub struct NoOpAction;

#[async_trait]
impl Action for NoOpAction {
    async fn execute(&self, _ctx: &TransitionContext) -> Result<()> {
        Ok(())
    }
}

// ============================================================================
// KANI FORMAL VERIFICATION PROOFS
// ============================================================================

#[cfg(kani)]
mod kani_proofs {
    use super::*;

    /// Proof: State with initial flag sets initial_state in definition
    #[kani::proof]
    fn proof_initial_state_set() {
        let def = StateMachineDefinition::new("test").state(State::new("start").initial());

        kani::assert(
            def.initial_state == Some("start".to_string()),
            "Initial state must be set",
        );
    }

    /// Proof: Definition without initial state fails validation
    #[kani::proof]
    fn proof_no_initial_fails_validation() {
        let def = StateMachineDefinition::new("test").state(State::new("a"));

        let result = def.validate();
        kani::assert(result.is_err(), "Definition without initial must fail");
    }

    /// Proof: Transition to non-existent state fails validation
    #[kani::proof]
    fn proof_invalid_transition_fails_validation() {
        let def = StateMachineDefinition::new("test")
            .state(State::new("a").initial())
            .transition(Transition::new("go", "a", "nonexistent"));

        let result = def.validate();
        kani::assert(
            result.is_err(),
            "Transition to non-existent state must fail",
        );
    }

    /// Proof: Valid definition passes validation
    #[kani::proof]
    fn proof_valid_definition_passes() {
        let def = StateMachineDefinition::new("test")
            .state(State::new("a").initial())
            .state(State::new("b"))
            .transition(Transition::new("go", "a", "b"));

        let result = def.validate();
        kani::assert(result.is_ok(), "Valid definition must pass");
    }

    /// Proof: get_transition returns correct transition
    #[kani::proof]
    fn proof_get_transition_correct() {
        let def = StateMachineDefinition::new("test")
            .state(State::new("a").initial())
            .state(State::new("b"))
            .transition(Transition::new("go", "a", "b"));

        let trans = def.get_transition("a", "go");
        kani::assert(trans.is_some(), "Transition must exist");

        if let Some(t) = trans {
            kani::assert(t.from == "a", "From state must match");
            kani::assert(t.to == "b", "To state must match");
            kani::assert(t.event == "go", "Event must match");
        }
    }

    /// Proof: get_transition returns None for non-existent event
    #[kani::proof]
    fn proof_get_transition_none_for_invalid() {
        let def = StateMachineDefinition::new("test")
            .state(State::new("a").initial())
            .state(State::new("b"))
            .transition(Transition::new("go", "a", "b"));

        let trans = def.get_transition("a", "invalid");
        kani::assert(trans.is_none(), "Invalid event must return None");

        let trans2 = def.get_transition("invalid", "go");
        kani::assert(trans2.is_none(), "Invalid state must return None");
    }

    /// Proof: transitions_from returns all transitions from a state
    #[kani::proof]
    fn proof_transitions_from_complete() {
        let def = StateMachineDefinition::new("test")
            .state(State::new("a").initial())
            .state(State::new("b"))
            .state(State::new("c"))
            .transition(Transition::new("go_b", "a", "b"))
            .transition(Transition::new("go_c", "a", "c"));

        let transitions = def.transitions_from("a");
        kani::assert(transitions.len() == 2, "Must return all transitions from a");
    }

    /// Proof: State marked as final is detected as final
    #[kani::proof]
    fn proof_final_state_detection() {
        let state = State::new("end").final_state();
        kani::assert(state.final_state, "Final state flag must be true");

        let normal_state = State::new("normal");
        kani::assert(!normal_state.final_state, "Normal state must not be final");
    }

    /// Proof: Transition with guard stores guard name
    #[kani::proof]
    fn proof_transition_guard() {
        let trans = Transition::new("event", "a", "b").with_guard("my_guard");

        kani::assert(
            trans.guard == Some("my_guard".to_string()),
            "Guard must be stored",
        );
    }

    /// Proof: Transition with action stores action name
    #[kani::proof]
    fn proof_transition_action() {
        let trans = Transition::new("event", "a", "b").with_action("my_action");

        kani::assert(
            trans.action == Some("my_action".to_string()),
            "Action must be stored",
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn traffic_light_definition() -> StateMachineDefinition {
        StateMachineDefinition::new("traffic_light")
            .state(State::new("red").initial())
            .state(State::new("green"))
            .state(State::new("yellow"))
            .transition(Transition::new("timer", "red", "green"))
            .transition(Transition::new("timer", "green", "yellow"))
            .transition(Transition::new("timer", "yellow", "red"))
    }

    #[test]
    fn test_state_creation() {
        let state = State::new("test")
            .initial()
            .with_metadata("key", serde_json::json!("value"));

        assert_eq!(state.name, "test");
        assert!(state.initial);
    }

    #[test]
    fn test_transition_creation() {
        let transition = Transition::new("event", "from", "to")
            .with_guard("check")
            .with_action("do_something");

        assert_eq!(transition.event, "event");
        assert_eq!(transition.guard, Some("check".to_string()));
    }

    #[test]
    fn test_definition_validation() {
        let def = traffic_light_definition();
        assert!(def.validate().is_ok());
    }

    #[test]
    fn test_definition_invalid_no_initial() {
        let def = StateMachineDefinition::new("test").state(State::new("a"));

        assert!(def.validate().is_err());
    }

    #[tokio::test]
    async fn test_state_machine_start() {
        let def = traffic_light_definition();
        let machine = StateMachine::new(def).unwrap();

        machine.start().await.unwrap();
        assert_eq!(machine.current_state().await, Some("red".to_string()));
    }

    #[tokio::test]
    async fn test_state_machine_transition() {
        let def = traffic_light_definition();
        let machine = StateMachine::new(def).unwrap();

        machine.start().await.unwrap();
        assert!(machine.is_in_state("red").await);

        machine.trigger("timer").await.unwrap();
        assert!(machine.is_in_state("green").await);

        machine.trigger("timer").await.unwrap();
        assert!(machine.is_in_state("yellow").await);

        machine.trigger("timer").await.unwrap();
        assert!(machine.is_in_state("red").await);
    }

    #[tokio::test]
    async fn test_invalid_transition() {
        let def = StateMachineDefinition::new("test")
            .state(State::new("a").initial())
            .state(State::new("b"))
            .transition(Transition::new("go", "a", "b"));

        let machine = StateMachine::new(def).unwrap();
        machine.start().await.unwrap();

        // Invalid event
        let result = machine.trigger("invalid").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_available_events() {
        let def = traffic_light_definition();
        let machine = StateMachine::new(def).unwrap();

        machine.start().await.unwrap();
        let events = machine.available_events().await;

        assert_eq!(events, vec!["timer"]);
    }

    #[tokio::test]
    async fn test_guard() {
        let def = StateMachineDefinition::new("test")
            .state(State::new("a").initial())
            .state(State::new("b"))
            .transition(Transition::new("go", "a", "b").with_guard("deny"));

        let machine = StateMachine::new(def).unwrap();
        machine.register_guard("deny", Arc::new(AlwaysDeny)).await;
        machine.start().await.unwrap();

        let result = machine.trigger("go").await;
        assert!(matches!(result, Err(StateMachineError::GuardRejected(_))));
        assert!(machine.is_in_state("a").await);
    }

    #[tokio::test]
    async fn test_history() {
        let def = traffic_light_definition();
        let machine = StateMachine::new(def).unwrap();

        machine.start().await.unwrap();
        machine.trigger("timer").await.unwrap();
        machine.trigger("timer").await.unwrap();

        let history = machine.history().await;
        assert_eq!(history.len(), 2);
        assert!(history[0].success);
    }

    #[tokio::test]
    async fn test_final_state() {
        let def = StateMachineDefinition::new("test")
            .state(State::new("start").initial())
            .state(State::new("end").final_state())
            .transition(Transition::new("finish", "start", "end"));

        let machine = StateMachine::new(def).unwrap();
        machine.start().await.unwrap();

        assert!(!machine.is_finished().await);

        machine.trigger("finish").await.unwrap();
        assert!(machine.is_finished().await);
    }
}
