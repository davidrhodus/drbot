//! Background agent execution system.
//!
//! Provides infrastructure for running agents in the background with:
//! - Persistent state
//! - Human-in-the-loop checkpoints
//! - Event streaming
//! - Resumable execution

use crate::{Agent, AgentConfig, AgentError, AgentEvent, AgentMessage, AgentState, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use drbot_providers::Provider;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, RwLock};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Background agent status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackgroundStatus {
    /// Agent is queued but not started.
    Queued,
    /// Agent is running.
    Running,
    /// Agent is paused, waiting for checkpoint approval.
    WaitingForApproval,
    /// Agent is paused by user.
    Paused,
    /// Agent completed successfully.
    Completed,
    /// Agent failed.
    Failed,
    /// Agent was cancelled.
    Cancelled,
}

/// A checkpoint requiring human approval.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    /// Checkpoint ID.
    pub id: Uuid,
    /// Agent ID.
    pub agent_id: Uuid,
    /// Description of what the agent wants to do.
    pub description: String,
    /// The action the agent wants to take.
    pub pending_action: CheckpointAction,
    /// Created timestamp.
    pub created_at: DateTime<Utc>,
    /// Whether it was approved.
    pub approved: Option<bool>,
    /// Approval timestamp.
    pub resolved_at: Option<DateTime<Utc>>,
    /// User feedback/modifications.
    pub user_feedback: Option<String>,
}

/// Action pending at a checkpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CheckpointAction {
    /// Execute a tool.
    ToolExecution {
        tool_name: String,
        arguments: serde_json::Value,
    },
    /// Send a message.
    SendMessage { content: String },
    /// Perform file operation.
    FileOperation { operation: String, path: String },
    /// Execute shell command.
    ShellCommand { command: String },
    /// Custom action.
    Custom {
        name: String,
        data: serde_json::Value,
    },
}

impl Checkpoint {
    /// Create a new checkpoint.
    pub fn new(agent_id: Uuid, description: &str, action: CheckpointAction) -> Self {
        Self {
            id: Uuid::new_v4(),
            agent_id,
            description: description.to_string(),
            pending_action: action,
            created_at: Utc::now(),
            approved: None,
            resolved_at: None,
            user_feedback: None,
        }
    }

    /// Approve the checkpoint.
    pub fn approve(&mut self, feedback: Option<String>) {
        self.approved = Some(true);
        self.resolved_at = Some(Utc::now());
        self.user_feedback = feedback;
    }

    /// Reject the checkpoint.
    pub fn reject(&mut self, feedback: Option<String>) {
        self.approved = Some(false);
        self.resolved_at = Some(Utc::now());
        self.user_feedback = feedback;
    }

    /// Check if resolved.
    pub fn is_resolved(&self) -> bool {
        self.approved.is_some()
    }
}

/// Background agent state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackgroundAgentState {
    /// Agent ID.
    pub id: Uuid,
    /// Task description.
    pub task: String,
    /// Current status.
    pub status: BackgroundStatus,
    /// Messages in the conversation.
    pub messages: Vec<AgentMessage>,
    /// Current iteration.
    pub iteration: usize,
    /// Max iterations.
    pub max_iterations: usize,
    /// Started timestamp.
    pub started_at: Option<DateTime<Utc>>,
    /// Completed timestamp.
    pub completed_at: Option<DateTime<Utc>>,
    /// Error message if failed.
    pub error: Option<String>,
    /// Pending checkpoints.
    pub checkpoints: Vec<Checkpoint>,
    /// Final output.
    pub output: Option<String>,
    /// Metadata.
    pub metadata: HashMap<String, serde_json::Value>,
}

impl BackgroundAgentState {
    /// Create a new background agent state.
    pub fn new(task: &str, max_iterations: usize) -> Self {
        Self {
            id: Uuid::new_v4(),
            task: task.to_string(),
            status: BackgroundStatus::Queued,
            messages: Vec::new(),
            iteration: 0,
            max_iterations,
            started_at: None,
            completed_at: None,
            error: None,
            checkpoints: Vec::new(),
            output: None,
            metadata: HashMap::new(),
        }
    }
}

/// Configuration for background agent execution.
#[derive(Debug, Clone)]
pub struct BackgroundConfig {
    /// Whether to require checkpoints for sensitive operations.
    pub require_checkpoints: bool,
    /// Operations that require checkpoints.
    pub checkpoint_operations: Vec<String>,
    /// Maximum concurrent agents.
    pub max_concurrent: usize,
    /// Auto-approve after timeout (None = wait forever).
    pub auto_approve_timeout_secs: Option<u64>,
    /// Persist state to storage.
    pub persist_state: bool,
}

impl Default for BackgroundConfig {
    fn default() -> Self {
        Self {
            require_checkpoints: true,
            checkpoint_operations: vec![
                "shell".to_string(),
                "file_write".to_string(),
                "file_delete".to_string(),
                "network".to_string(),
            ],
            max_concurrent: 5,
            auto_approve_timeout_secs: None,
            persist_state: true,
        }
    }
}

/// Storage trait for background agent state.
#[async_trait]
pub trait BackgroundStorage: Send + Sync {
    /// Save agent state.
    async fn save_state(&self, state: &BackgroundAgentState) -> Result<()>;
    /// Load agent state.
    async fn load_state(&self, id: Uuid) -> Result<Option<BackgroundAgentState>>;
    /// List all agent states.
    async fn list_states(&self) -> Result<Vec<BackgroundAgentState>>;
    /// Delete agent state.
    async fn delete_state(&self, id: Uuid) -> Result<()>;
}

/// In-memory storage for background agents.
#[derive(Default)]
pub struct MemoryBackgroundStorage {
    states: RwLock<HashMap<Uuid, BackgroundAgentState>>,
}

impl MemoryBackgroundStorage {
    /// Create new memory storage.
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl BackgroundStorage for MemoryBackgroundStorage {
    async fn save_state(&self, state: &BackgroundAgentState) -> Result<()> {
        let mut states = self.states.write().await;
        states.insert(state.id, state.clone());
        Ok(())
    }

    async fn load_state(&self, id: Uuid) -> Result<Option<BackgroundAgentState>> {
        let states = self.states.read().await;
        Ok(states.get(&id).cloned())
    }

    async fn list_states(&self) -> Result<Vec<BackgroundAgentState>> {
        let states = self.states.read().await;
        Ok(states.values().cloned().collect())
    }

    async fn delete_state(&self, id: Uuid) -> Result<()> {
        let mut states = self.states.write().await;
        states.remove(&id);
        Ok(())
    }
}

/// Event from background agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BackgroundEvent {
    /// Agent started.
    Started { agent_id: Uuid },
    /// Agent iteration.
    Iteration { agent_id: Uuid, iteration: usize },
    /// Checkpoint created, waiting for approval.
    CheckpointCreated {
        agent_id: Uuid,
        checkpoint: Checkpoint,
    },
    /// Checkpoint resolved.
    CheckpointResolved {
        agent_id: Uuid,
        checkpoint_id: Uuid,
        approved: bool,
    },
    /// Agent event (from inner agent).
    AgentEvent { agent_id: Uuid, event: AgentEvent },
    /// Agent completed.
    Completed { agent_id: Uuid, output: String },
    /// Agent failed.
    Failed { agent_id: Uuid, error: String },
    /// Agent cancelled.
    Cancelled { agent_id: Uuid },
    /// Agent paused.
    Paused { agent_id: Uuid },
    /// Agent resumed.
    Resumed { agent_id: Uuid },
}

/// Background agent runner.
pub struct BackgroundRunner {
    config: BackgroundConfig,
    storage: Arc<dyn BackgroundStorage>,
    provider: Arc<dyn Provider>,
    agent_config: AgentConfig,
    /// Active agents.
    agents: Arc<RwLock<HashMap<Uuid, BackgroundAgentState>>>,
    /// Event broadcaster.
    events: broadcast::Sender<BackgroundEvent>,
    /// Checkpoint approval channel.
    approvals: Arc<RwLock<HashMap<Uuid, mpsc::Sender<(Uuid, bool, Option<String>)>>>>,
}

impl BackgroundRunner {
    /// Create a new background runner.
    pub fn new(
        provider: Arc<dyn Provider>,
        config: BackgroundConfig,
        agent_config: AgentConfig,
    ) -> Self {
        let storage = Arc::new(MemoryBackgroundStorage::new());
        Self::with_storage(provider, config, agent_config, storage)
    }

    /// Create with custom storage.
    pub fn with_storage(
        provider: Arc<dyn Provider>,
        config: BackgroundConfig,
        agent_config: AgentConfig,
        storage: Arc<dyn BackgroundStorage>,
    ) -> Self {
        let (events, _) = broadcast::channel(1000);
        Self {
            config,
            storage,
            provider,
            agent_config,
            agents: Arc::new(RwLock::new(HashMap::new())),
            events,
            approvals: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Subscribe to events.
    pub fn subscribe(&self) -> broadcast::Receiver<BackgroundEvent> {
        self.events.subscribe()
    }

    /// Start a background agent.
    pub async fn start(&self, task: &str) -> Result<Uuid> {
        // Check concurrent limit
        let agents = self.agents.read().await;
        let running = agents
            .values()
            .filter(|a| a.status == BackgroundStatus::Running)
            .count();
        drop(agents);

        if running >= self.config.max_concurrent {
            return Err(AgentError::ExecutionFailed(format!(
                "Max concurrent agents ({}) reached",
                self.config.max_concurrent
            )));
        }

        // Create state
        let state = BackgroundAgentState::new(task, self.agent_config.max_iterations);
        let agent_id = state.id;

        // Save state
        if self.config.persist_state {
            self.storage.save_state(&state).await?;
        }

        // Add to active agents
        {
            let mut agents = self.agents.write().await;
            agents.insert(agent_id, state);
        }

        // Create approval channel
        let (tx, rx) = mpsc::channel(10);
        {
            let mut approvals = self.approvals.write().await;
            approvals.insert(agent_id, tx);
        }

        // Spawn the agent task
        let runner = self.clone_for_spawn();
        let task_str = task.to_string();
        tokio::spawn(async move {
            runner.run_agent(agent_id, &task_str, rx).await;
        });

        // Emit event
        let _ = self.events.send(BackgroundEvent::Started { agent_id });

        info!(agent_id = %agent_id, "Started background agent");
        Ok(agent_id)
    }

    /// Clone self for spawning (shares Arc references).
    fn clone_for_spawn(&self) -> Self {
        Self {
            config: self.config.clone(),
            storage: self.storage.clone(),
            provider: self.provider.clone(),
            agent_config: self.agent_config.clone(),
            agents: self.agents.clone(),
            events: self.events.clone(),
            approvals: self.approvals.clone(),
        }
    }

    /// Run an agent.
    async fn run_agent(
        &self,
        agent_id: Uuid,
        task: &str,
        mut approval_rx: mpsc::Receiver<(Uuid, bool, Option<String>)>,
    ) {
        // Update status to running
        self.update_status(agent_id, BackgroundStatus::Running, None)
            .await;

        // Create the actual agent
        let mut agent = Agent::new(self.provider.clone(), self.agent_config.clone());

        // Run with event streaming
        let (event_tx, mut event_rx) = mpsc::channel(100);

        // Forward events
        let events = self.events.clone();
        let agent_id_for_events = agent_id;
        tokio::spawn(async move {
            while let Some(event) = event_rx.recv().await {
                let _ = events.send(BackgroundEvent::AgentEvent {
                    agent_id: agent_id_for_events,
                    event,
                });
            }
        });

        // Run the agent
        match agent.run_with_events(task, event_tx).await {
            Ok(output) => {
                self.update_status(agent_id, BackgroundStatus::Completed, None)
                    .await;
                self.set_output(agent_id, &output).await;
                let _ = self
                    .events
                    .send(BackgroundEvent::Completed { agent_id, output });
            }
            Err(e) => {
                let error = e.to_string();
                self.update_status(agent_id, BackgroundStatus::Failed, Some(&error))
                    .await;
                let _ = self
                    .events
                    .send(BackgroundEvent::Failed { agent_id, error });
            }
        }

        // Cleanup
        {
            let mut approvals = self.approvals.write().await;
            approvals.remove(&agent_id);
        }
    }

    /// Update agent status.
    async fn update_status(&self, agent_id: Uuid, status: BackgroundStatus, error: Option<&str>) {
        let mut agents = self.agents.write().await;
        if let Some(state) = agents.get_mut(&agent_id) {
            state.status = status;
            if status == BackgroundStatus::Running && state.started_at.is_none() {
                state.started_at = Some(Utc::now());
            }
            if matches!(
                status,
                BackgroundStatus::Completed
                    | BackgroundStatus::Failed
                    | BackgroundStatus::Cancelled
            ) {
                state.completed_at = Some(Utc::now());
            }
            if let Some(err) = error {
                state.error = Some(err.to_string());
            }

            // Persist
            if self.config.persist_state {
                let state_clone = state.clone();
                let storage = self.storage.clone();
                tokio::spawn(async move {
                    let _ = storage.save_state(&state_clone).await;
                });
            }
        }
    }

    /// Set agent output.
    async fn set_output(&self, agent_id: Uuid, output: &str) {
        let mut agents = self.agents.write().await;
        if let Some(state) = agents.get_mut(&agent_id) {
            state.output = Some(output.to_string());
        }
    }

    /// Get agent state.
    pub async fn get_state(&self, agent_id: Uuid) -> Option<BackgroundAgentState> {
        let agents = self.agents.read().await;
        agents.get(&agent_id).cloned()
    }

    /// List all agents.
    pub async fn list_agents(&self) -> Vec<BackgroundAgentState> {
        let agents = self.agents.read().await;
        agents.values().cloned().collect()
    }

    /// Pause an agent.
    pub async fn pause(&self, agent_id: Uuid) -> Result<()> {
        self.update_status(agent_id, BackgroundStatus::Paused, None)
            .await;
        let _ = self.events.send(BackgroundEvent::Paused { agent_id });
        Ok(())
    }

    /// Resume an agent.
    pub async fn resume(&self, agent_id: Uuid) -> Result<()> {
        self.update_status(agent_id, BackgroundStatus::Running, None)
            .await;
        let _ = self.events.send(BackgroundEvent::Resumed { agent_id });
        Ok(())
    }

    /// Cancel an agent.
    pub async fn cancel(&self, agent_id: Uuid) -> Result<()> {
        self.update_status(agent_id, BackgroundStatus::Cancelled, None)
            .await;
        let _ = self.events.send(BackgroundEvent::Cancelled { agent_id });
        Ok(())
    }

    /// Approve a checkpoint.
    pub async fn approve_checkpoint(
        &self,
        agent_id: Uuid,
        checkpoint_id: Uuid,
        feedback: Option<String>,
    ) -> Result<()> {
        let approvals = self.approvals.read().await;
        if let Some(tx) = approvals.get(&agent_id) {
            tx.send((checkpoint_id, true, feedback))
                .await
                .map_err(|_| AgentError::ExecutionFailed("Agent not running".into()))?;
        }
        let _ = self.events.send(BackgroundEvent::CheckpointResolved {
            agent_id,
            checkpoint_id,
            approved: true,
        });
        Ok(())
    }

    /// Reject a checkpoint.
    pub async fn reject_checkpoint(
        &self,
        agent_id: Uuid,
        checkpoint_id: Uuid,
        feedback: Option<String>,
    ) -> Result<()> {
        let approvals = self.approvals.read().await;
        if let Some(tx) = approvals.get(&agent_id) {
            tx.send((checkpoint_id, false, feedback))
                .await
                .map_err(|_| AgentError::ExecutionFailed("Agent not running".into()))?;
        }
        let _ = self.events.send(BackgroundEvent::CheckpointResolved {
            agent_id,
            checkpoint_id,
            approved: false,
        });
        Ok(())
    }

    /// Get pending checkpoints for an agent.
    pub async fn get_pending_checkpoints(&self, agent_id: Uuid) -> Vec<Checkpoint> {
        let agents = self.agents.read().await;
        agents
            .get(&agent_id)
            .map(|s| {
                s.checkpoints
                    .iter()
                    .filter(|c| !c.is_resolved())
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_checkpoint_creation() {
        let checkpoint = Checkpoint::new(
            Uuid::new_v4(),
            "Execute shell command",
            CheckpointAction::ShellCommand {
                command: "ls".to_string(),
            },
        );
        assert!(!checkpoint.is_resolved());
    }

    #[test]
    fn test_checkpoint_approval() {
        let mut checkpoint = Checkpoint::new(
            Uuid::new_v4(),
            "Test",
            CheckpointAction::Custom {
                name: "test".to_string(),
                data: serde_json::json!({}),
            },
        );
        checkpoint.approve(Some("Looks good".to_string()));
        assert!(checkpoint.is_resolved());
        assert_eq!(checkpoint.approved, Some(true));
    }

    #[test]
    fn test_background_state_creation() {
        let state = BackgroundAgentState::new("Test task", 10);
        assert_eq!(state.status, BackgroundStatus::Queued);
        assert_eq!(state.max_iterations, 10);
    }

    #[test]
    fn test_background_config_default() {
        let config = BackgroundConfig::default();
        assert!(config.require_checkpoints);
        assert_eq!(config.max_concurrent, 5);
    }

    #[tokio::test]
    async fn test_memory_storage() {
        let storage = MemoryBackgroundStorage::new();
        let state = BackgroundAgentState::new("Test", 5);
        let id = state.id;

        storage.save_state(&state).await.unwrap();

        let loaded = storage.load_state(id).await.unwrap();
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().task, "Test");

        let all = storage.list_states().await.unwrap();
        assert_eq!(all.len(), 1);

        storage.delete_state(id).await.unwrap();
        let deleted = storage.load_state(id).await.unwrap();
        assert!(deleted.is_none());
    }
}
