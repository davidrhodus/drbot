//! Lifecycle management for drbot components.
//!
//! This crate provides:
//! - Component lifecycle trait
//! - State machine for lifecycle
//! - Health monitoring
//! - Dependency management

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use thiserror::Error;

/// Lifecycle error types.
#[derive(Error, Debug)]
pub enum LifecycleError {
    #[error("Invalid state transition from {from:?} to {to:?}")]
    InvalidTransition { from: State, to: State },

    #[error("Component not found: {0}")]
    NotFound(String),

    #[error("Initialization failed: {0}")]
    InitFailed(String),

    #[error("Start failed: {0}")]
    StartFailed(String),

    #[error("Stop failed: {0}")]
    StopFailed(String),

    #[error("Dependency error: {0}")]
    DependencyError(String),
}

/// Result type for lifecycle operations.
pub type Result<T> = std::result::Result<T, LifecycleError>;

/// Component state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum State {
    /// Initial state.
    Created = 0,
    /// Initializing.
    Initializing = 1,
    /// Initialized.
    Initialized = 2,
    /// Starting.
    Starting = 3,
    /// Running.
    Running = 4,
    /// Stopping.
    Stopping = 5,
    /// Stopped.
    Stopped = 6,
    /// Failed.
    Failed = 7,
    /// Disposed.
    Disposed = 8,
}

impl State {
    /// Check if can transition to another state.
    pub fn can_transition_to(&self, to: State) -> bool {
        matches!(
            (self, to),
            (State::Created, State::Initializing)
                | (State::Initializing, State::Initialized)
                | (State::Initializing, State::Failed)
                | (State::Initialized, State::Starting)
                | (State::Starting, State::Running)
                | (State::Starting, State::Failed)
                | (State::Running, State::Stopping)
                | (State::Stopping, State::Stopped)
                | (State::Stopping, State::Failed)
                | (State::Stopped, State::Starting)
                | (State::Stopped, State::Disposed)
                | (State::Failed, State::Disposed)
                | (State::Failed, State::Initializing)
        )
    }

    /// Check if in terminal state.
    pub fn is_terminal(&self) -> bool {
        matches!(self, State::Disposed)
    }

    /// Check if running.
    pub fn is_running(&self) -> bool {
        matches!(self, State::Running)
    }

    /// Check if active (not stopped/failed/disposed).
    pub fn is_active(&self) -> bool {
        !matches!(self, State::Stopped | State::Failed | State::Disposed)
    }
}

impl From<u32> for State {
    fn from(value: u32) -> Self {
        match value {
            0 => State::Created,
            1 => State::Initializing,
            2 => State::Initialized,
            3 => State::Starting,
            4 => State::Running,
            5 => State::Stopping,
            6 => State::Stopped,
            7 => State::Failed,
            8 => State::Disposed,
            _ => State::Failed,
        }
    }
}

/// Lifecycle trait for components.
#[async_trait]
pub trait Lifecycle: Send + Sync {
    /// Get component name.
    fn name(&self) -> &str;

    /// Initialize component.
    async fn init(&mut self) -> Result<()>;

    /// Start component.
    async fn start(&mut self) -> Result<()>;

    /// Stop component.
    async fn stop(&mut self) -> Result<()>;

    /// Dispose component.
    async fn dispose(&mut self) -> Result<()> {
        Ok(())
    }

    /// Get health status.
    fn health(&self) -> Health {
        Health::Unknown
    }
}

/// Health status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Health {
    /// Healthy.
    Healthy,
    /// Degraded but functional.
    Degraded,
    /// Unhealthy.
    Unhealthy,
    /// Unknown.
    Unknown,
}

impl Health {
    /// Check if healthy or degraded (operational).
    pub fn is_operational(&self) -> bool {
        matches!(self, Health::Healthy | Health::Degraded)
    }
}

/// Lifecycle state machine.
pub struct LifecycleState {
    state: AtomicU32,
    name: String,
}

impl LifecycleState {
    /// Create new state machine.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            state: AtomicU32::new(State::Created as u32),
            name: name.into(),
        }
    }

    /// Get current state.
    pub fn state(&self) -> State {
        State::from(self.state.load(Ordering::SeqCst))
    }

    /// Get name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Transition to new state.
    pub fn transition(&self, to: State) -> Result<()> {
        let from = self.state();
        if from.can_transition_to(to) {
            self.state.store(to as u32, Ordering::SeqCst);
            Ok(())
        } else {
            Err(LifecycleError::InvalidTransition { from, to })
        }
    }

    /// Set state directly (for error recovery).
    pub fn set_state(&self, state: State) {
        self.state.store(state as u32, Ordering::SeqCst);
    }
}

/// Lifecycle manager.
pub struct LifecycleManager {
    components: Mutex<HashMap<String, Arc<Mutex<dyn Lifecycle>>>>,
    states: Mutex<HashMap<String, LifecycleState>>,
    dependencies: Mutex<HashMap<String, Vec<String>>>,
}

impl LifecycleManager {
    /// Create new manager.
    pub fn new() -> Self {
        Self {
            components: Mutex::new(HashMap::new()),
            states: Mutex::new(HashMap::new()),
            dependencies: Mutex::new(HashMap::new()),
        }
    }

    /// Register component.
    pub fn register(&self, component: Arc<Mutex<dyn Lifecycle>>) {
        let name = component.lock().unwrap().name().to_string();
        self.states
            .lock()
            .unwrap()
            .insert(name.clone(), LifecycleState::new(&name));
        self.components.lock().unwrap().insert(name, component);
    }

    /// Add dependency.
    pub fn add_dependency(&self, component: &str, depends_on: &str) {
        self.dependencies
            .lock()
            .unwrap()
            .entry(component.to_string())
            .or_default()
            .push(depends_on.to_string());
    }

    /// Get component state.
    pub fn state(&self, name: &str) -> Option<State> {
        self.states.lock().unwrap().get(name).map(|s| s.state())
    }

    /// Initialize all components.
    pub async fn init_all(&self) -> Result<()> {
        let order = self.get_init_order()?;

        for name in order {
            self.init_component(&name).await?;
        }

        Ok(())
    }

    /// Initialize single component.
    async fn init_component(&self, name: &str) -> Result<()> {
        let component = self
            .components
            .lock()
            .unwrap()
            .get(name)
            .cloned()
            .ok_or_else(|| LifecycleError::NotFound(name.to_string()))?;

        // Transition to initializing
        if let Some(state) = self.states.lock().unwrap().get(name) {
            state.transition(State::Initializing)?;
        }

        // Initialize
        let result = component.lock().unwrap().init().await;

        // Update state based on result
        if let Some(state) = self.states.lock().unwrap().get(name) {
            match &result {
                Ok(_) => state.transition(State::Initialized)?,
                Err(_) => state.set_state(State::Failed),
            }
        }

        result
    }

    /// Start all components.
    pub async fn start_all(&self) -> Result<()> {
        let order = self.get_init_order()?;

        for name in order {
            self.start_component(&name).await?;
        }

        Ok(())
    }

    /// Start single component.
    async fn start_component(&self, name: &str) -> Result<()> {
        let component = self
            .components
            .lock()
            .unwrap()
            .get(name)
            .cloned()
            .ok_or_else(|| LifecycleError::NotFound(name.to_string()))?;

        // Transition to starting
        if let Some(state) = self.states.lock().unwrap().get(name) {
            state.transition(State::Starting)?;
        }

        // Start
        let result = component.lock().unwrap().start().await;

        // Update state based on result
        if let Some(state) = self.states.lock().unwrap().get(name) {
            match &result {
                Ok(_) => state.transition(State::Running)?,
                Err(_) => state.set_state(State::Failed),
            }
        }

        result
    }

    /// Stop all components (reverse order).
    pub async fn stop_all(&self) -> Result<()> {
        let mut order = self.get_init_order()?;
        order.reverse();

        for name in order {
            self.stop_component(&name).await?;
        }

        Ok(())
    }

    /// Stop single component.
    async fn stop_component(&self, name: &str) -> Result<()> {
        let component = self
            .components
            .lock()
            .unwrap()
            .get(name)
            .cloned()
            .ok_or_else(|| LifecycleError::NotFound(name.to_string()))?;

        // Transition to stopping
        if let Some(state) = self.states.lock().unwrap().get(name) {
            state.transition(State::Stopping)?;
        }

        // Stop
        let result = component.lock().unwrap().stop().await;

        // Update state based on result
        if let Some(state) = self.states.lock().unwrap().get(name) {
            match &result {
                Ok(_) => state.transition(State::Stopped)?,
                Err(_) => state.set_state(State::Failed),
            }
        }

        result
    }

    /// Get initialization order (topological sort).
    fn get_init_order(&self) -> Result<Vec<String>> {
        let components = self.components.lock().unwrap();
        let dependencies = self.dependencies.lock().unwrap();

        let mut order = Vec::new();
        let mut visited = HashMap::new();

        for name in components.keys() {
            self.visit_for_order(name, &dependencies, &mut visited, &mut order)?;
        }

        Ok(order)
    }

    fn visit_for_order(
        &self,
        name: &str,
        dependencies: &HashMap<String, Vec<String>>,
        visited: &mut HashMap<String, bool>,
        order: &mut Vec<String>,
    ) -> Result<()> {
        if let Some(&in_progress) = visited.get(name) {
            if in_progress {
                return Err(LifecycleError::DependencyError(format!(
                    "Circular dependency detected at {}",
                    name
                )));
            }
            return Ok(());
        }

        visited.insert(name.to_string(), true);

        if let Some(deps) = dependencies.get(name) {
            for dep in deps {
                self.visit_for_order(dep, dependencies, visited, order)?;
            }
        }

        visited.insert(name.to_string(), false);
        order.push(name.to_string());

        Ok(())
    }

    /// Get all component names.
    pub fn components(&self) -> Vec<String> {
        self.components.lock().unwrap().keys().cloned().collect()
    }
}

impl Default for LifecycleManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state_transitions() {
        let state = LifecycleState::new("test");
        assert_eq!(state.state(), State::Created);

        state.transition(State::Initializing).unwrap();
        assert_eq!(state.state(), State::Initializing);

        state.transition(State::Initialized).unwrap();
        assert_eq!(state.state(), State::Initialized);

        // Invalid transition
        assert!(state.transition(State::Disposed).is_err());
    }

    #[test]
    fn test_state_can_transition() {
        assert!(State::Created.can_transition_to(State::Initializing));
        assert!(!State::Created.can_transition_to(State::Running));
        assert!(State::Running.can_transition_to(State::Stopping));
    }

    #[test]
    fn test_health_operational() {
        assert!(Health::Healthy.is_operational());
        assert!(Health::Degraded.is_operational());
        assert!(!Health::Unhealthy.is_operational());
    }
}
