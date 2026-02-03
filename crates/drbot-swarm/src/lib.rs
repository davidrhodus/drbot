//! Multi-agent swarm for drbot.
//!
//! Coordinated AI agent orchestration.
//!
//! # Features
//!
//! - Agent spawning and lifecycle
//! - Task distribution
//! - Inter-agent communication
//! - Result aggregation
//! - Resource management

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;

/// Swarm result type.
pub type Result<T> = std::result::Result<T, SwarmError>;

/// Swarm errors.
#[derive(Debug, thiserror::Error)]
pub enum SwarmError {
    #[error("Agent not found: {0}")]
    AgentNotFound(String),
    #[error("Task failed: {0}")]
    TaskFailed(String),
    #[error("Resource exhausted: {0}")]
    ResourceExhausted(String),
    #[error("Communication error: {0}")]
    CommunicationError(String),
    #[error("Timeout: {0}")]
    Timeout(String),
}

/// Agent capability.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    Research,
    Coding,
    Writing,
    Analysis,
    Planning,
    Review,
    Testing,
    Design,
    Custom(String),
}

/// Agent status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    Idle,
    Working,
    Waiting,
    Completed,
    Failed,
    Terminated,
}

/// Agent definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    /// Agent ID.
    pub id: Uuid,
    /// Name.
    pub name: String,
    /// Role description.
    pub role: String,
    /// Capabilities.
    pub capabilities: Vec<Capability>,
    /// Status.
    pub status: AgentStatus,
    /// Current task.
    pub current_task: Option<Uuid>,
    /// Completed tasks.
    pub completed_tasks: usize,
    /// Created at.
    pub created_at: DateTime<Utc>,
    /// Model to use.
    pub model: String,
    /// System prompt.
    pub system_prompt: String,
}

impl Agent {
    /// Create a new agent.
    pub fn new(name: &str, role: &str, capabilities: Vec<Capability>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.to_string(),
            role: role.to_string(),
            capabilities,
            status: AgentStatus::Idle,
            current_task: None,
            completed_tasks: 0,
            created_at: Utc::now(),
            model: "claude-3-5-sonnet".to_string(),
            system_prompt: String::new(),
        }
    }

    /// Check if agent has capability.
    pub fn has_capability(&self, cap: &Capability) -> bool {
        self.capabilities.contains(cap)
    }

    /// Check if agent is available.
    pub fn is_available(&self) -> bool {
        self.status == AgentStatus::Idle
    }
}

/// Task priority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskPriority {
    Low,
    Normal,
    High,
    Critical,
}

/// Task status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Pending,
    Assigned,
    Running,
    Completed,
    Failed,
    Cancelled,
}

/// Swarm task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmTask {
    /// Task ID.
    pub id: Uuid,
    /// Name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Required capabilities.
    pub required_capabilities: Vec<Capability>,
    /// Priority.
    pub priority: TaskPriority,
    /// Status.
    pub status: TaskStatus,
    /// Assigned agent.
    pub assigned_agent: Option<Uuid>,
    /// Dependencies.
    pub dependencies: Vec<Uuid>,
    /// Result.
    pub result: Option<TaskResult>,
    /// Created at.
    pub created_at: DateTime<Utc>,
    /// Started at.
    pub started_at: Option<DateTime<Utc>>,
    /// Completed at.
    pub completed_at: Option<DateTime<Utc>>,
    /// Context data.
    pub context: HashMap<String, String>,
}

impl SwarmTask {
    /// Create a new task.
    pub fn new(name: &str, description: &str, capabilities: Vec<Capability>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.to_string(),
            description: description.to_string(),
            required_capabilities: capabilities,
            priority: TaskPriority::Normal,
            status: TaskStatus::Pending,
            assigned_agent: None,
            dependencies: Vec::new(),
            result: None,
            created_at: Utc::now(),
            started_at: None,
            completed_at: None,
            context: HashMap::new(),
        }
    }

    /// Check if dependencies are satisfied.
    pub fn dependencies_satisfied(&self, completed: &[Uuid]) -> bool {
        self.dependencies.iter().all(|dep| completed.contains(dep))
    }
}

/// Task result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResult {
    /// Output.
    pub output: String,
    /// Artifacts.
    pub artifacts: Vec<Artifact>,
    /// Metrics.
    pub metrics: HashMap<String, f64>,
    /// Success.
    pub success: bool,
    /// Error message.
    pub error: Option<String>,
}

/// Artifact.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    /// Name.
    pub name: String,
    /// Type.
    pub artifact_type: String,
    /// Content.
    pub content: String,
}

/// Message between agents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMessage {
    /// Message ID.
    pub id: Uuid,
    /// From agent.
    pub from: Uuid,
    /// To agent (None = broadcast).
    pub to: Option<Uuid>,
    /// Message type.
    pub msg_type: MessageType,
    /// Content.
    pub content: String,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
}

/// Message types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageType {
    Request,
    Response,
    Status,
    Handoff,
    Query,
    Result,
}

/// Swarm event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SwarmEvent {
    AgentSpawned(Uuid),
    AgentTerminated(Uuid),
    TaskCreated(Uuid),
    TaskAssigned { task: Uuid, agent: Uuid },
    TaskCompleted { task: Uuid, success: bool },
    MessageSent(AgentMessage),
}

/// Swarm configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmConfig {
    /// Maximum agents.
    pub max_agents: usize,
    /// Maximum concurrent tasks.
    pub max_concurrent_tasks: usize,
    /// Task timeout (seconds).
    pub task_timeout: u64,
    /// Enable auto-scaling.
    pub auto_scale: bool,
}

impl Default for SwarmConfig {
    fn default() -> Self {
        Self {
            max_agents: 10,
            max_concurrent_tasks: 5,
            task_timeout: 300,
            auto_scale: true,
        }
    }
}

/// Trait for task executors.
#[async_trait]
pub trait TaskExecutor: Send + Sync {
    /// Execute a task with an agent.
    async fn execute(&self, agent: &Agent, task: &SwarmTask) -> Result<TaskResult>;
}

/// Swarm orchestrator.
pub struct Swarm<E: TaskExecutor> {
    config: SwarmConfig,
    executor: E,
    agents: Arc<RwLock<HashMap<Uuid, Agent>>>,
    tasks: Arc<RwLock<HashMap<Uuid, SwarmTask>>>,
    messages: Arc<RwLock<Vec<AgentMessage>>>,
    event_tx: broadcast::Sender<SwarmEvent>,
}

impl<E: TaskExecutor> Swarm<E> {
    /// Create a new swarm.
    pub fn new(config: SwarmConfig, executor: E) -> Self {
        let (event_tx, _) = broadcast::channel(100);
        Self {
            config,
            executor,
            agents: Arc::new(RwLock::new(HashMap::new())),
            tasks: Arc::new(RwLock::new(HashMap::new())),
            messages: Arc::new(RwLock::new(Vec::new())),
            event_tx,
        }
    }

    /// Subscribe to events.
    pub fn subscribe(&self) -> broadcast::Receiver<SwarmEvent> {
        self.event_tx.subscribe()
    }

    /// Spawn an agent.
    pub async fn spawn_agent(&self, agent: Agent) -> Result<Uuid> {
        let agents = self.agents.read().await;
        if agents.len() >= self.config.max_agents {
            return Err(SwarmError::ResourceExhausted(
                "Max agents reached".to_string(),
            ));
        }
        drop(agents);

        let id = agent.id;
        self.agents.write().await.insert(id, agent);
        let _ = self.event_tx.send(SwarmEvent::AgentSpawned(id));
        Ok(id)
    }

    /// Terminate an agent.
    pub async fn terminate_agent(&self, id: Uuid) -> Result<()> {
        let mut agents = self.agents.write().await;
        if let Some(agent) = agents.get_mut(&id) {
            agent.status = AgentStatus::Terminated;
            let _ = self.event_tx.send(SwarmEvent::AgentTerminated(id));
        }
        Ok(())
    }

    /// Submit a task.
    pub async fn submit_task(&self, task: SwarmTask) -> Result<Uuid> {
        let id = task.id;
        self.tasks.write().await.insert(id, task);
        let _ = self.event_tx.send(SwarmEvent::TaskCreated(id));
        Ok(id)
    }

    /// Find best agent for task.
    pub async fn find_agent(&self, task: &SwarmTask) -> Option<Uuid> {
        let agents = self.agents.read().await;

        // Find available agents with required capabilities
        let mut candidates: Vec<_> = agents
            .values()
            .filter(|a| {
                a.is_available()
                    && task
                        .required_capabilities
                        .iter()
                        .all(|cap| a.has_capability(cap))
            })
            .collect();

        // Sort by completed tasks (prefer experienced agents)
        candidates.sort_by_key(|a| std::cmp::Reverse(a.completed_tasks));

        candidates.first().map(|a| a.id)
    }

    /// Assign task to agent.
    pub async fn assign_task(&self, task_id: Uuid, agent_id: Uuid) -> Result<()> {
        let mut tasks = self.tasks.write().await;
        let mut agents = self.agents.write().await;

        let task = tasks
            .get_mut(&task_id)
            .ok_or(SwarmError::TaskFailed("Task not found".to_string()))?;
        let agent = agents
            .get_mut(&agent_id)
            .ok_or(SwarmError::AgentNotFound(agent_id.to_string()))?;

        task.status = TaskStatus::Assigned;
        task.assigned_agent = Some(agent_id);
        agent.status = AgentStatus::Working;
        agent.current_task = Some(task_id);

        let _ = self.event_tx.send(SwarmEvent::TaskAssigned {
            task: task_id,
            agent: agent_id,
        });

        Ok(())
    }

    /// Execute assigned task.
    pub async fn execute_task(&self, task_id: Uuid) -> Result<TaskResult> {
        let task = self
            .tasks
            .read()
            .await
            .get(&task_id)
            .cloned()
            .ok_or(SwarmError::TaskFailed("Task not found".to_string()))?;

        let agent_id = task
            .assigned_agent
            .ok_or(SwarmError::TaskFailed("Task not assigned".to_string()))?;
        let agent = self
            .agents
            .read()
            .await
            .get(&agent_id)
            .cloned()
            .ok_or(SwarmError::AgentNotFound(agent_id.to_string()))?;

        // Update task status
        {
            let mut tasks = self.tasks.write().await;
            if let Some(t) = tasks.get_mut(&task_id) {
                t.status = TaskStatus::Running;
                t.started_at = Some(Utc::now());
            }
        }

        // Execute
        let result = self.executor.execute(&agent, &task).await?;

        // Update task and agent
        {
            let mut tasks = self.tasks.write().await;
            let mut agents = self.agents.write().await;

            if let Some(t) = tasks.get_mut(&task_id) {
                t.status = if result.success {
                    TaskStatus::Completed
                } else {
                    TaskStatus::Failed
                };
                t.completed_at = Some(Utc::now());
                t.result = Some(result.clone());
            }

            if let Some(a) = agents.get_mut(&agent_id) {
                a.status = AgentStatus::Idle;
                a.current_task = None;
                a.completed_tasks += 1;
            }
        }

        let _ = self.event_tx.send(SwarmEvent::TaskCompleted {
            task: task_id,
            success: result.success,
        });

        Ok(result)
    }

    /// Run task automatically (find agent, assign, execute).
    pub async fn run_task(&self, task: SwarmTask) -> Result<TaskResult> {
        let task_id = self.submit_task(task.clone()).await?;

        // Wait for dependencies
        loop {
            let completed = self.get_completed_tasks().await;
            let current_task = self.tasks.read().await.get(&task_id).cloned();
            if let Some(t) = current_task {
                if t.dependencies_satisfied(&completed) {
                    break;
                }
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }

        // Find and assign agent
        let agent_id = self
            .find_agent(&task)
            .await
            .ok_or(SwarmError::AgentNotFound("No suitable agent".to_string()))?;
        self.assign_task(task_id, agent_id).await?;

        // Execute
        self.execute_task(task_id).await
    }

    /// Get completed task IDs.
    pub async fn get_completed_tasks(&self) -> Vec<Uuid> {
        self.tasks
            .read()
            .await
            .iter()
            .filter(|(_, t)| t.status == TaskStatus::Completed)
            .map(|(id, _)| *id)
            .collect()
    }

    /// Send message between agents.
    pub async fn send_message(&self, message: AgentMessage) -> Result<()> {
        self.messages.write().await.push(message.clone());
        let _ = self.event_tx.send(SwarmEvent::MessageSent(message));
        Ok(())
    }

    /// Get messages for agent.
    pub async fn get_messages(&self, agent_id: Uuid) -> Vec<AgentMessage> {
        self.messages
            .read()
            .await
            .iter()
            .filter(|m| m.to.is_none() || m.to == Some(agent_id))
            .cloned()
            .collect()
    }

    /// Get swarm statistics.
    pub async fn stats(&self) -> SwarmStats {
        let agents = self.agents.read().await;
        let tasks = self.tasks.read().await;

        let active_agents = agents
            .values()
            .filter(|a| a.status == AgentStatus::Working)
            .count();
        let completed_tasks = tasks
            .values()
            .filter(|t| t.status == TaskStatus::Completed)
            .count();
        let failed_tasks = tasks
            .values()
            .filter(|t| t.status == TaskStatus::Failed)
            .count();

        SwarmStats {
            total_agents: agents.len(),
            active_agents,
            total_tasks: tasks.len(),
            completed_tasks,
            failed_tasks,
            pending_tasks: tasks
                .values()
                .filter(|t| t.status == TaskStatus::Pending)
                .count(),
        }
    }
}

/// Swarm statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmStats {
    pub total_agents: usize,
    pub active_agents: usize,
    pub total_tasks: usize,
    pub completed_tasks: usize,
    pub failed_tasks: usize,
    pub pending_tasks: usize,
}

/// Simple executor for testing.
pub struct SimpleExecutor;

#[async_trait]
impl TaskExecutor for SimpleExecutor {
    async fn execute(&self, agent: &Agent, task: &SwarmTask) -> Result<TaskResult> {
        // Simulate work
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        Ok(TaskResult {
            output: format!("Agent {} completed task: {}", agent.name, task.name),
            artifacts: Vec::new(),
            metrics: HashMap::from([("duration_ms".to_string(), 10.0)]),
            success: true,
            error: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_spawn_agent() {
        let swarm = Swarm::new(SwarmConfig::default(), SimpleExecutor);
        let agent = Agent::new("Researcher", "Research agent", vec![Capability::Research]);

        let id = swarm.spawn_agent(agent).await.unwrap();

        let stats = swarm.stats().await;
        assert_eq!(stats.total_agents, 1);
        assert!(swarm.agents.read().await.contains_key(&id));
    }

    #[tokio::test]
    async fn test_submit_task() {
        let swarm = Swarm::new(SwarmConfig::default(), SimpleExecutor);
        let task = SwarmTask::new("Research", "Research task", vec![Capability::Research]);

        let id = swarm.submit_task(task).await.unwrap();

        let stats = swarm.stats().await;
        assert_eq!(stats.total_tasks, 1);
        assert_eq!(stats.pending_tasks, 1);
        assert!(swarm.tasks.read().await.contains_key(&id));
    }

    #[tokio::test]
    async fn test_find_agent() {
        let swarm = Swarm::new(SwarmConfig::default(), SimpleExecutor);

        let researcher = Agent::new("Researcher", "Research", vec![Capability::Research]);
        let coder = Agent::new("Coder", "Coding", vec![Capability::Coding]);

        swarm.spawn_agent(researcher.clone()).await.unwrap();
        swarm.spawn_agent(coder).await.unwrap();

        let research_task = SwarmTask::new("Research", "Do research", vec![Capability::Research]);
        let code_task = SwarmTask::new("Code", "Write code", vec![Capability::Coding]);

        let found_researcher = swarm.find_agent(&research_task).await;
        let found_coder = swarm.find_agent(&code_task).await;

        assert!(found_researcher.is_some());
        assert!(found_coder.is_some());
        assert_ne!(found_researcher, found_coder);
    }

    #[tokio::test]
    async fn test_run_task() {
        let swarm = Swarm::new(SwarmConfig::default(), SimpleExecutor);

        let agent = Agent::new(
            "Worker",
            "General worker",
            vec![Capability::Research, Capability::Analysis],
        );
        swarm.spawn_agent(agent).await.unwrap();

        let task = SwarmTask::new("Analyze", "Analyze data", vec![Capability::Analysis]);
        let result = swarm.run_task(task).await.unwrap();

        assert!(result.success);

        let stats = swarm.stats().await;
        assert_eq!(stats.completed_tasks, 1);
    }

    #[tokio::test]
    async fn test_task_dependencies() {
        let swarm = Swarm::new(SwarmConfig::default(), SimpleExecutor);

        let agent = Agent::new(
            "Worker",
            "Worker",
            vec![Capability::Research, Capability::Analysis],
        );
        swarm.spawn_agent(agent).await.unwrap();

        let task1 = SwarmTask::new("First", "First task", vec![Capability::Research]);
        let task1_id = task1.id;

        let mut task2 = SwarmTask::new("Second", "Second task", vec![Capability::Analysis]);
        task2.dependencies.push(task1_id);

        // Task 2 shouldn't be satisfiable yet
        assert!(!task2.dependencies_satisfied(&[]));

        // Run task 1
        swarm.run_task(task1).await.unwrap();

        let completed = swarm.get_completed_tasks().await;
        assert!(task2.dependencies_satisfied(&completed));
    }

    #[tokio::test]
    async fn test_max_agents_limit() {
        let config = SwarmConfig {
            max_agents: 2,
            ..Default::default()
        };
        let swarm = Swarm::new(config, SimpleExecutor);

        swarm
            .spawn_agent(Agent::new("A1", "Agent 1", vec![]))
            .await
            .unwrap();
        swarm
            .spawn_agent(Agent::new("A2", "Agent 2", vec![]))
            .await
            .unwrap();

        let result = swarm.spawn_agent(Agent::new("A3", "Agent 3", vec![])).await;
        assert!(matches!(result, Err(SwarmError::ResourceExhausted(_))));
    }
}
