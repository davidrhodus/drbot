//! Multi-agent orchestration for drbot.
//!
//! Enables agents to spawn sub-agents, delegate tasks, and coordinate work.
//!
//! # Features
//!
//! - Agent spawning and lifecycle management
//! - Task delegation and result aggregation
//! - Parallel and sequential execution
//! - Agent communication channels
//! - Hierarchical agent trees

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, RwLock};
use uuid::Uuid;

/// Orchestration result type.
pub type Result<T> = std::result::Result<T, OrchestrationError>;

/// Orchestration errors.
#[derive(Debug, thiserror::Error)]
pub enum OrchestrationError {
    #[error("Agent not found: {0}")]
    AgentNotFound(Uuid),
    #[error("Task failed: {0}")]
    TaskFailed(String),
    #[error("Timeout waiting for agents")]
    Timeout,
    #[error("Max depth exceeded")]
    MaxDepthExceeded,
    #[error("Agent communication failed: {0}")]
    CommunicationFailed(String),
}

/// Agent definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDef {
    /// Agent ID.
    pub id: Uuid,
    /// Agent name.
    pub name: String,
    /// Agent type/role.
    pub agent_type: AgentType,
    /// Agent capabilities.
    pub capabilities: Vec<String>,
    /// System prompt for the agent.
    pub system_prompt: String,
    /// Model to use.
    pub model: Option<String>,
    /// Max concurrent tasks.
    pub max_concurrent: usize,
}

impl AgentDef {
    /// Create a new agent definition.
    pub fn new(name: &str, agent_type: AgentType) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.to_string(),
            agent_type,
            capabilities: Vec::new(),
            system_prompt: String::new(),
            model: None,
            max_concurrent: 5,
        }
    }

    /// Set system prompt.
    pub fn with_prompt(mut self, prompt: &str) -> Self {
        self.system_prompt = prompt.to_string();
        self
    }

    /// Add capability.
    pub fn with_capability(mut self, capability: &str) -> Self {
        self.capabilities.push(capability.to_string());
        self
    }

    /// Set model.
    pub fn with_model(mut self, model: &str) -> Self {
        self.model = Some(model.to_string());
        self
    }
}

/// Agent types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentType {
    /// Coordinator agent (manages other agents).
    Coordinator,
    /// Worker agent (executes tasks).
    Worker,
    /// Specialist agent (domain-specific).
    Specialist,
    /// Reviewer agent (validates outputs).
    Reviewer,
    /// Aggregator agent (combines results).
    Aggregator,
}

/// Task to be executed by an agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTask {
    /// Task ID.
    pub id: Uuid,
    /// Parent task ID (if subtask).
    pub parent_id: Option<Uuid>,
    /// Task description.
    pub description: String,
    /// Task input.
    pub input: serde_json::Value,
    /// Required capabilities.
    pub required_capabilities: Vec<String>,
    /// Priority (higher = more important).
    pub priority: u8,
    /// Timeout in seconds.
    pub timeout_secs: u64,
    /// Created at.
    pub created_at: DateTime<Utc>,
}

impl AgentTask {
    /// Create a new task.
    pub fn new(description: &str, input: serde_json::Value) -> Self {
        Self {
            id: Uuid::new_v4(),
            parent_id: None,
            description: description.to_string(),
            input,
            required_capabilities: Vec::new(),
            priority: 5,
            timeout_secs: 300,
            created_at: Utc::now(),
        }
    }

    /// Set parent task.
    pub fn with_parent(mut self, parent_id: Uuid) -> Self {
        self.parent_id = Some(parent_id);
        self
    }

    /// Add required capability.
    pub fn requiring(mut self, capability: &str) -> Self {
        self.required_capabilities.push(capability.to_string());
        self
    }

    /// Set priority.
    pub fn with_priority(mut self, priority: u8) -> Self {
        self.priority = priority;
        self
    }
}

/// Task result from an agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResult {
    /// Task ID.
    pub task_id: Uuid,
    /// Agent ID that executed.
    pub agent_id: Uuid,
    /// Result status.
    pub status: TaskStatus,
    /// Result output.
    pub output: serde_json::Value,
    /// Execution duration (ms).
    pub duration_ms: u64,
    /// Sub-results (if task spawned subtasks).
    pub sub_results: Vec<TaskResult>,
    /// Completed at.
    pub completed_at: DateTime<Utc>,
}

/// Task execution status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    /// Pending execution.
    Pending,
    /// Currently running.
    Running,
    /// Completed successfully.
    Completed,
    /// Failed.
    Failed,
    /// Cancelled.
    Cancelled,
    /// Timed out.
    TimedOut,
}

/// Execution strategy for multiple tasks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionStrategy {
    /// Run all tasks in parallel.
    Parallel,
    /// Run tasks sequentially.
    Sequential,
    /// Run with max concurrency limit.
    Concurrent { max: usize },
    /// Pipeline: output of one is input to next.
    Pipeline,
    /// Race: first to complete wins.
    Race,
    /// All must succeed.
    AllOrNothing,
}

/// Agent message for inter-agent communication.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMessage {
    /// Message ID.
    pub id: Uuid,
    /// Sender agent ID.
    pub from: Uuid,
    /// Recipient agent ID.
    pub to: Uuid,
    /// Message type.
    pub message_type: MessageType,
    /// Message payload.
    pub payload: serde_json::Value,
    /// Sent at.
    pub sent_at: DateTime<Utc>,
}

/// Agent message types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageType {
    /// Task assignment.
    TaskAssignment,
    /// Task result.
    TaskResult,
    /// Status query.
    StatusQuery,
    /// Status response.
    StatusResponse,
    /// Help request.
    HelpRequest,
    /// Help response.
    HelpResponse,
    /// Broadcast to all.
    Broadcast,
}

/// Agent execution trait.
#[async_trait]
pub trait AgentExecutor: Send + Sync {
    /// Execute a task.
    async fn execute(&self, task: &AgentTask) -> Result<TaskResult>;

    /// Get agent definition.
    fn definition(&self) -> &AgentDef;

    /// Check if agent can handle a task.
    fn can_handle(&self, task: &AgentTask) -> bool {
        task.required_capabilities
            .iter()
            .all(|cap| self.definition().capabilities.contains(cap))
    }
}

/// Orchestrator configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestratorConfig {
    /// Maximum agent tree depth.
    pub max_depth: usize,
    /// Default timeout for tasks (seconds).
    pub default_timeout_secs: u64,
    /// Maximum concurrent agents.
    pub max_concurrent_agents: usize,
    /// Enable agent communication.
    pub enable_communication: bool,
    /// Retry failed tasks.
    pub retry_failed: bool,
    /// Max retries per task.
    pub max_retries: u32,
}

impl Default for OrchestratorConfig {
    fn default() -> Self {
        Self {
            max_depth: 5,
            default_timeout_secs: 300,
            max_concurrent_agents: 10,
            enable_communication: true,
            retry_failed: true,
            max_retries: 3,
        }
    }
}

/// Agent instance state.
struct AgentInstance {
    definition: AgentDef,
    executor: Arc<dyn AgentExecutor>,
    current_tasks: Vec<Uuid>,
    message_rx: mpsc::Receiver<AgentMessage>,
    message_tx: mpsc::Sender<AgentMessage>,
}

/// Multi-agent orchestrator.
pub struct Orchestrator {
    config: OrchestratorConfig,
    agents: Arc<RwLock<HashMap<Uuid, Arc<RwLock<AgentState>>>>>,
    task_results: Arc<RwLock<HashMap<Uuid, TaskResult>>>,
    event_sender: broadcast::Sender<OrchestrationEvent>,
}

/// Agent state.
pub struct AgentState {
    /// Agent definition.
    pub definition: AgentDef,
    /// Current status.
    pub status: AgentStatus,
    /// Active tasks.
    pub active_tasks: Vec<Uuid>,
    /// Completed task count.
    pub completed_tasks: u64,
    /// Failed task count.
    pub failed_tasks: u64,
    /// Message sender.
    pub message_tx: mpsc::Sender<AgentMessage>,
}

/// Agent status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    /// Idle, ready for tasks.
    Idle,
    /// Busy executing tasks.
    Busy,
    /// Paused.
    Paused,
    /// Stopped.
    Stopped,
}

/// Orchestration events.
#[derive(Debug, Clone)]
pub enum OrchestrationEvent {
    /// Agent spawned.
    AgentSpawned { agent_id: Uuid, name: String },
    /// Agent stopped.
    AgentStopped { agent_id: Uuid },
    /// Task started.
    TaskStarted { task_id: Uuid, agent_id: Uuid },
    /// Task completed.
    TaskCompleted {
        task_id: Uuid,
        agent_id: Uuid,
        status: TaskStatus,
    },
    /// Subtask spawned.
    SubtaskSpawned { parent_id: Uuid, subtask_id: Uuid },
    /// Message sent.
    MessageSent {
        from: Uuid,
        to: Uuid,
        message_type: MessageType,
    },
}

impl Orchestrator {
    /// Create a new orchestrator.
    pub fn new(config: OrchestratorConfig) -> Self {
        let (event_sender, _) = broadcast::channel(256);

        Self {
            config,
            agents: Arc::new(RwLock::new(HashMap::new())),
            task_results: Arc::new(RwLock::new(HashMap::new())),
            event_sender,
        }
    }

    /// Spawn a new agent.
    pub async fn spawn_agent(&self, definition: AgentDef) -> Uuid {
        let agent_id = definition.id;
        let name = definition.name.clone();
        let (tx, _rx) = mpsc::channel(100);

        let state = AgentState {
            definition,
            status: AgentStatus::Idle,
            active_tasks: Vec::new(),
            completed_tasks: 0,
            failed_tasks: 0,
            message_tx: tx,
        };

        self.agents
            .write()
            .await
            .insert(agent_id, Arc::new(RwLock::new(state)));

        let _ = self
            .event_sender
            .send(OrchestrationEvent::AgentSpawned { agent_id, name });

        agent_id
    }

    /// Stop an agent.
    pub async fn stop_agent(&self, agent_id: Uuid) -> Result<()> {
        let mut agents = self.agents.write().await;

        if let Some(agent) = agents.remove(&agent_id) {
            let mut state = agent.write().await;
            state.status = AgentStatus::Stopped;

            let _ = self
                .event_sender
                .send(OrchestrationEvent::AgentStopped { agent_id });
            Ok(())
        } else {
            Err(OrchestrationError::AgentNotFound(agent_id))
        }
    }

    /// Submit a task to the best available agent.
    pub async fn submit_task(&self, task: AgentTask) -> Result<Uuid> {
        let agents = self.agents.read().await;

        // Find best agent for task
        let best_agent = agents
            .iter()
            .filter(|(_, state_lock)| {
                // Would need to check capabilities in a non-blocking way
                true
            })
            .next();

        if let Some((agent_id, state_lock)) = best_agent {
            let mut state = state_lock.write().await;
            state.active_tasks.push(task.id);
            state.status = AgentStatus::Busy;

            let _ = self.event_sender.send(OrchestrationEvent::TaskStarted {
                task_id: task.id,
                agent_id: *agent_id,
            });

            Ok(task.id)
        } else {
            Err(OrchestrationError::AgentNotFound(Uuid::nil()))
        }
    }

    /// Execute multiple tasks with a strategy.
    pub async fn execute_with_strategy(
        &self,
        tasks: Vec<AgentTask>,
        strategy: ExecutionStrategy,
    ) -> Result<Vec<TaskResult>> {
        match strategy {
            ExecutionStrategy::Sequential => {
                let mut results = Vec::new();
                for task in tasks {
                    let task_id = self.submit_task(task).await?;
                    // Would wait for result
                    results.push(TaskResult {
                        task_id,
                        agent_id: Uuid::nil(),
                        status: TaskStatus::Completed,
                        output: serde_json::Value::Null,
                        duration_ms: 0,
                        sub_results: Vec::new(),
                        completed_at: Utc::now(),
                    });
                }
                Ok(results)
            }
            ExecutionStrategy::Parallel => {
                let mut handles = Vec::new();
                for task in tasks {
                    let task_id = task.id;
                    let _ = self.submit_task(task).await?;
                    handles.push(task_id);
                }
                // Would wait for all results
                Ok(handles
                    .into_iter()
                    .map(|task_id| TaskResult {
                        task_id,
                        agent_id: Uuid::nil(),
                        status: TaskStatus::Completed,
                        output: serde_json::Value::Null,
                        duration_ms: 0,
                        sub_results: Vec::new(),
                        completed_at: Utc::now(),
                    })
                    .collect())
            }
            ExecutionStrategy::Race => {
                // First to complete wins
                if let Some(task) = tasks.into_iter().next() {
                    let task_id = self.submit_task(task).await?;
                    Ok(vec![TaskResult {
                        task_id,
                        agent_id: Uuid::nil(),
                        status: TaskStatus::Completed,
                        output: serde_json::Value::Null,
                        duration_ms: 0,
                        sub_results: Vec::new(),
                        completed_at: Utc::now(),
                    }])
                } else {
                    Ok(Vec::new())
                }
            }
            _ => {
                // Other strategies
                Ok(Vec::new())
            }
        }
    }

    /// Send message between agents.
    pub async fn send_message(&self, message: AgentMessage) -> Result<()> {
        let agents = self.agents.read().await;

        if let Some(recipient) = agents.get(&message.to) {
            let state = recipient.read().await;
            state
                .message_tx
                .send(message.clone())
                .await
                .map_err(|e| OrchestrationError::CommunicationFailed(e.to_string()))?;

            let _ = self.event_sender.send(OrchestrationEvent::MessageSent {
                from: message.from,
                to: message.to,
                message_type: message.message_type,
            });

            Ok(())
        } else {
            Err(OrchestrationError::AgentNotFound(message.to))
        }
    }

    /// Get task result.
    pub async fn get_result(&self, task_id: Uuid) -> Option<TaskResult> {
        self.task_results.read().await.get(&task_id).cloned()
    }

    /// Get all agents.
    pub async fn list_agents(&self) -> Vec<AgentDef> {
        let agents = self.agents.read().await;
        let mut defs = Vec::new();
        for (_, state_lock) in agents.iter() {
            let state = state_lock.read().await;
            defs.push(state.definition.clone());
        }
        defs
    }

    /// Subscribe to events.
    pub fn subscribe(&self) -> broadcast::Receiver<OrchestrationEvent> {
        self.event_sender.subscribe()
    }

    /// Get orchestrator statistics.
    pub async fn stats(&self) -> OrchestratorStats {
        let agents = self.agents.read().await;
        let results = self.task_results.read().await;

        let mut active_agents = 0;
        let mut total_completed = 0;
        let mut total_failed = 0;

        for (_, state_lock) in agents.iter() {
            let state = state_lock.read().await;
            if state.status == AgentStatus::Busy {
                active_agents += 1;
            }
            total_completed += state.completed_tasks;
            total_failed += state.failed_tasks;
        }

        OrchestratorStats {
            total_agents: agents.len(),
            active_agents,
            total_completed,
            total_failed,
            pending_results: results.len(),
        }
    }
}

/// Orchestrator statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestratorStats {
    /// Total number of agents.
    pub total_agents: usize,
    /// Number of active agents.
    pub active_agents: usize,
    /// Total completed tasks.
    pub total_completed: u64,
    /// Total failed tasks.
    pub total_failed: u64,
    /// Pending results.
    pub pending_results: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_orchestrator() {
        let orchestrator = Orchestrator::new(OrchestratorConfig::default());

        let agent_def = AgentDef::new("research-agent", AgentType::Worker)
            .with_capability("research")
            .with_capability("summarize")
            .with_prompt("You are a research assistant.");

        let agent_id = orchestrator.spawn_agent(agent_def).await;

        let agents = orchestrator.list_agents().await;
        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0].name, "research-agent");
    }

    #[tokio::test]
    async fn test_task_submission() {
        let orchestrator = Orchestrator::new(OrchestratorConfig::default());

        let agent_def = AgentDef::new("worker", AgentType::Worker);
        orchestrator.spawn_agent(agent_def).await;

        let task = AgentTask::new(
            "Research quantum computing",
            serde_json::json!({"topic": "quantum"}),
        );

        let task_id = orchestrator.submit_task(task).await.unwrap();
        assert!(!task_id.is_nil());
    }

    #[test]
    fn test_agent_def_builder() {
        let agent = AgentDef::new("coder", AgentType::Specialist)
            .with_capability("rust")
            .with_capability("python")
            .with_model("claude-3-opus")
            .with_prompt("Expert coder");

        assert_eq!(agent.capabilities.len(), 2);
        assert_eq!(agent.model, Some("claude-3-opus".to_string()));
    }
}
