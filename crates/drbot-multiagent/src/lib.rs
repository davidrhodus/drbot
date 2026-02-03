//! Collaborative AI swarm with agent-to-agent communication.
//!
//! This crate provides multi-agent coordination capabilities:
//! - Spawn specialized agents for different tasks
//! - Agent-to-agent communication and delegation
//! - Consensus building among agents
//! - Swarm intelligence for complex problem solving

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::{broadcast, mpsc, RwLock};
use uuid::Uuid;

/// Multi-agent system errors.
#[derive(Debug, Error)]
pub enum MultiAgentError {
    #[error("Agent not found: {0}")]
    AgentNotFound(String),

    #[error("Agent creation failed: {0}")]
    CreationFailed(String),

    #[error("Communication failed: {0}")]
    CommunicationFailed(String),

    #[error("Consensus not reached: {0}")]
    ConsensusNotReached(String),

    #[error("Task routing failed: {0}")]
    RoutingFailed(String),

    #[error("Timeout waiting for agents")]
    Timeout,

    #[error("Provider error: {0}")]
    ProviderError(String),
}

/// Result type for multi-agent operations.
pub type Result<T> = std::result::Result<T, MultiAgentError>;

/// Agent specialization type.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum AgentSpecialization {
    /// General-purpose reasoning.
    Generalist,
    /// Code analysis and generation.
    Coder,
    /// Research and information gathering.
    Researcher,
    /// Critical analysis and review.
    Critic,
    /// Creative writing and ideation.
    Creative,
    /// Planning and strategy.
    Planner,
    /// Data analysis.
    Analyst,
    /// Quality assurance.
    QA,
    /// Custom specialization.
    Custom(String),
}

/// Agent state.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum AgentState {
    /// Agent is idle.
    Idle,
    /// Agent is processing a task.
    Working,
    /// Agent is waiting for other agents.
    Waiting,
    /// Agent is communicating.
    Communicating,
    /// Agent has finished its task.
    Completed,
    /// Agent encountered an error.
    Error,
}

/// An agent in the multi-agent system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    /// Unique agent identifier.
    pub id: String,
    /// Agent name.
    pub name: String,
    /// Agent specialization.
    pub specialization: AgentSpecialization,
    /// Current state.
    pub state: AgentState,
    /// Agent capabilities.
    pub capabilities: Vec<String>,
    /// Current task if any.
    pub current_task: Option<String>,
    /// Agent metrics.
    pub metrics: AgentMetrics,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
}

/// Metrics for an agent.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentMetrics {
    /// Tasks completed.
    pub tasks_completed: u32,
    /// Average task duration in ms.
    pub avg_task_duration_ms: u64,
    /// Success rate.
    pub success_rate: f64,
    /// Messages sent.
    pub messages_sent: u32,
    /// Messages received.
    pub messages_received: u32,
}

/// A message between agents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMessage {
    /// Unique message ID.
    pub id: String,
    /// Sender agent ID.
    pub from: String,
    /// Recipient agent ID (or "broadcast").
    pub to: String,
    /// Message type.
    pub message_type: MessageType,
    /// Message content.
    pub content: serde_json::Value,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
    /// Requires acknowledgment.
    pub requires_ack: bool,
}

/// Types of inter-agent messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessageType {
    /// Request to perform a task.
    TaskRequest,
    /// Response to a task request.
    TaskResponse,
    /// Information sharing.
    Information,
    /// Request for opinion/input.
    ConsultationRequest,
    /// Opinion/input response.
    ConsultationResponse,
    /// Vote in a consensus process.
    Vote,
    /// Acknowledgment.
    Ack,
    /// Error notification.
    Error,
    /// Status update.
    StatusUpdate,
}

/// A task for the multi-agent system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmTask {
    /// Task identifier.
    pub id: String,
    /// Task description.
    pub description: String,
    /// Required specializations.
    pub required_specializations: Vec<AgentSpecialization>,
    /// Minimum agents needed.
    pub min_agents: u32,
    /// Consensus threshold (0.0-1.0).
    pub consensus_threshold: f64,
    /// Task parameters.
    pub parameters: HashMap<String, serde_json::Value>,
    /// Task status.
    pub status: TaskStatus,
    /// Assigned agents.
    pub assigned_agents: Vec<String>,
    /// Agent contributions.
    pub contributions: HashMap<String, AgentContribution>,
    /// Final result.
    pub result: Option<SwarmResult>,
}

/// Task status.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum TaskStatus {
    Pending,
    Assigning,
    InProgress,
    ConsensusBuilding,
    Completed,
    Failed,
}

/// An agent's contribution to a task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentContribution {
    /// Agent ID.
    pub agent_id: String,
    /// Contribution content.
    pub content: serde_json::Value,
    /// Confidence in contribution.
    pub confidence: f64,
    /// Reasoning behind contribution.
    pub reasoning: String,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
}

/// Result from a swarm task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmResult {
    /// Synthesized result.
    pub content: serde_json::Value,
    /// Consensus level achieved.
    pub consensus_level: f64,
    /// Participating agents.
    pub participants: Vec<String>,
    /// Dissenting opinions.
    pub dissent: Vec<DissentingOpinion>,
    /// Execution time in ms.
    pub execution_time_ms: u64,
}

/// A dissenting opinion from an agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DissentingOpinion {
    /// Agent ID.
    pub agent_id: String,
    /// The dissent.
    pub opinion: String,
    /// Reasoning.
    pub reasoning: String,
}

/// Provider for agent capabilities.
#[async_trait]
pub trait AgentProvider: Send + Sync {
    /// Execute a task as an agent.
    async fn execute(
        &self,
        agent: &Agent,
        task: &str,
        context: &AgentContext,
    ) -> Result<AgentContribution>;

    /// Generate a response to a message.
    async fn respond_to_message(
        &self,
        agent: &Agent,
        message: &AgentMessage,
    ) -> Result<AgentMessage>;

    /// Vote on a proposal.
    async fn vote(&self, agent: &Agent, proposal: &str, options: &[String]) -> Result<Vote>;

    /// Synthesize contributions into a final result.
    async fn synthesize(&self, contributions: &[AgentContribution]) -> Result<serde_json::Value>;
}

/// Context for agent execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentContext {
    /// Task being worked on.
    pub task: String,
    /// Available tools.
    pub tools: Vec<String>,
    /// Information from other agents.
    pub shared_context: HashMap<String, serde_json::Value>,
    /// Constraints.
    pub constraints: AgentConstraints,
}

/// Constraints for agent execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConstraints {
    /// Maximum tokens to generate.
    pub max_tokens: u32,
    /// Time limit in seconds.
    pub time_limit_secs: u32,
    /// Must use specific tools.
    pub required_tools: Vec<String>,
}

impl Default for AgentConstraints {
    fn default() -> Self {
        Self {
            max_tokens: 4096,
            time_limit_secs: 60,
            required_tools: Vec::new(),
        }
    }
}

/// A vote in a consensus process.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vote {
    /// Agent casting the vote.
    pub agent_id: String,
    /// Chosen option.
    pub choice: String,
    /// Confidence in choice.
    pub confidence: f64,
    /// Reasoning.
    pub reasoning: String,
}

/// The multi-agent swarm coordinator.
pub struct SwarmCoordinator {
    /// Agent provider.
    provider: Arc<dyn AgentProvider>,
    /// Active agents.
    agents: Arc<RwLock<HashMap<String, Agent>>>,
    /// Active tasks.
    tasks: Arc<RwLock<HashMap<String, SwarmTask>>>,
    /// Message bus.
    message_tx: broadcast::Sender<AgentMessage>,
    /// Message receiver for new subscriptions.
    _message_rx: broadcast::Receiver<AgentMessage>,
}

impl SwarmCoordinator {
    /// Create a new swarm coordinator.
    pub fn new(provider: Arc<dyn AgentProvider>) -> Self {
        let (message_tx, message_rx) = broadcast::channel(1000);

        Self {
            provider,
            agents: Arc::new(RwLock::new(HashMap::new())),
            tasks: Arc::new(RwLock::new(HashMap::new())),
            message_tx,
            _message_rx: message_rx,
        }
    }

    /// Spawn a new agent.
    pub async fn spawn_agent(
        &self,
        name: &str,
        specialization: AgentSpecialization,
    ) -> Result<Agent> {
        let agent = Agent {
            id: Uuid::new_v4().to_string(),
            name: name.to_string(),
            specialization: specialization.clone(),
            state: AgentState::Idle,
            capabilities: self.capabilities_for_specialization(&specialization),
            current_task: None,
            metrics: AgentMetrics::default(),
            created_at: Utc::now(),
        };

        let mut agents = self.agents.write().await;
        agents.insert(agent.id.clone(), agent.clone());

        Ok(agent)
    }

    /// Get capabilities for a specialization.
    fn capabilities_for_specialization(&self, spec: &AgentSpecialization) -> Vec<String> {
        match spec {
            AgentSpecialization::Generalist => vec!["reasoning", "analysis", "synthesis"]
                .into_iter()
                .map(String::from)
                .collect(),
            AgentSpecialization::Coder => vec![
                "code_analysis",
                "code_generation",
                "debugging",
                "refactoring",
            ]
            .into_iter()
            .map(String::from)
            .collect(),
            AgentSpecialization::Researcher => vec!["search", "summarize", "fact_check", "cite"]
                .into_iter()
                .map(String::from)
                .collect(),
            AgentSpecialization::Critic => vec![
                "review",
                "critique",
                "identify_flaws",
                "suggest_improvements",
            ]
            .into_iter()
            .map(String::from)
            .collect(),
            AgentSpecialization::Creative => {
                vec!["ideation", "writing", "storytelling", "brainstorm"]
                    .into_iter()
                    .map(String::from)
                    .collect()
            }
            AgentSpecialization::Planner => vec![
                "planning",
                "scheduling",
                "resource_allocation",
                "risk_assessment",
            ]
            .into_iter()
            .map(String::from)
            .collect(),
            AgentSpecialization::Analyst => {
                vec!["data_analysis", "visualization", "statistics", "trends"]
                    .into_iter()
                    .map(String::from)
                    .collect()
            }
            AgentSpecialization::QA => {
                vec!["testing", "validation", "edge_cases", "quality_metrics"]
                    .into_iter()
                    .map(String::from)
                    .collect()
            }
            AgentSpecialization::Custom(name) => vec![name.clone()],
        }
    }

    /// Create a swarm task.
    pub async fn create_task(
        &self,
        description: &str,
        required_specializations: Vec<AgentSpecialization>,
        consensus_threshold: f64,
    ) -> Result<SwarmTask> {
        let task = SwarmTask {
            id: Uuid::new_v4().to_string(),
            description: description.to_string(),
            required_specializations,
            min_agents: 2,
            consensus_threshold,
            parameters: HashMap::new(),
            status: TaskStatus::Pending,
            assigned_agents: Vec::new(),
            contributions: HashMap::new(),
            result: None,
        };

        let mut tasks = self.tasks.write().await;
        tasks.insert(task.id.clone(), task.clone());

        Ok(task)
    }

    /// Execute a swarm task with automatic agent assignment.
    pub async fn execute_task(&self, task_id: &str) -> Result<SwarmResult> {
        let mut task = {
            let tasks = self.tasks.read().await;
            tasks
                .get(task_id)
                .cloned()
                .ok_or_else(|| MultiAgentError::AgentNotFound(task_id.to_string()))?
        };

        let start_time = std::time::Instant::now();

        // Assignment phase
        task.status = TaskStatus::Assigning;
        let assigned = self.assign_agents(&task).await?;
        task.assigned_agents = assigned.iter().map(|a| a.id.clone()).collect();

        // Execution phase
        task.status = TaskStatus::InProgress;
        let context = AgentContext {
            task: task.description.clone(),
            tools: Vec::new(),
            shared_context: HashMap::new(),
            constraints: AgentConstraints::default(),
        };

        for agent in &assigned {
            let contribution = self
                .provider
                .execute(agent, &task.description, &context)
                .await?;
            task.contributions.insert(agent.id.clone(), contribution);
        }

        // Consensus phase
        task.status = TaskStatus::ConsensusBuilding;
        let result = self.build_consensus(&task).await?;

        task.status = TaskStatus::Completed;
        task.result = Some(SwarmResult {
            content: result.clone(),
            consensus_level: self.calculate_consensus(&task),
            participants: task.assigned_agents.clone(),
            dissent: Vec::new(),
            execution_time_ms: start_time.elapsed().as_millis() as u64,
        });

        // Update task
        let mut tasks = self.tasks.write().await;
        tasks.insert(task.id.clone(), task.clone());

        Ok(task.result.unwrap())
    }

    /// Assign agents to a task.
    async fn assign_agents(&self, task: &SwarmTask) -> Result<Vec<Agent>> {
        let agents = self.agents.read().await;
        let mut assigned = Vec::new();

        for spec in &task.required_specializations {
            let matching: Vec<_> = agents
                .values()
                .filter(|a| &a.specialization == spec && a.state == AgentState::Idle)
                .cloned()
                .collect();

            if let Some(agent) = matching.first() {
                assigned.push(agent.clone());
            } else {
                // Spawn a new agent for this specialization
                drop(agents);
                let new_agent = self
                    .spawn_agent(&format!("{:?}-agent", spec), spec.clone())
                    .await?;
                assigned.push(new_agent);
                break;
            }
        }

        if assigned.len() < task.min_agents as usize {
            return Err(MultiAgentError::RoutingFailed(format!(
                "Could not assign minimum {} agents",
                task.min_agents
            )));
        }

        Ok(assigned)
    }

    /// Build consensus from agent contributions.
    async fn build_consensus(&self, task: &SwarmTask) -> Result<serde_json::Value> {
        let contributions: Vec<_> = task.contributions.values().cloned().collect();

        if contributions.is_empty() {
            return Err(MultiAgentError::ConsensusNotReached(
                "No contributions".to_string(),
            ));
        }

        self.provider.synthesize(&contributions).await
    }

    /// Calculate consensus level from contributions.
    fn calculate_consensus(&self, task: &SwarmTask) -> f64 {
        if task.contributions.is_empty() {
            return 0.0;
        }

        let total_confidence: f64 = task.contributions.values().map(|c| c.confidence).sum();

        total_confidence / task.contributions.len() as f64
    }

    /// Send a message between agents.
    pub async fn send_message(&self, message: AgentMessage) -> Result<()> {
        self.message_tx
            .send(message)
            .map_err(|e| MultiAgentError::CommunicationFailed(e.to_string()))?;
        Ok(())
    }

    /// Get all active agents.
    pub async fn get_agents(&self) -> Vec<Agent> {
        let agents = self.agents.read().await;
        agents.values().cloned().collect()
    }

    /// Get an agent by ID.
    pub async fn get_agent(&self, id: &str) -> Option<Agent> {
        let agents = self.agents.read().await;
        agents.get(id).cloned()
    }

    /// Get a task by ID.
    pub async fn get_task(&self, id: &str) -> Option<SwarmTask> {
        let tasks = self.tasks.read().await;
        tasks.get(id).cloned()
    }

    /// Subscribe to messages.
    pub fn subscribe(&self) -> broadcast::Receiver<AgentMessage> {
        self.message_tx.subscribe()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockProvider;

    #[async_trait]
    impl AgentProvider for MockProvider {
        async fn execute(
            &self,
            agent: &Agent,
            task: &str,
            _context: &AgentContext,
        ) -> Result<AgentContribution> {
            Ok(AgentContribution {
                agent_id: agent.id.clone(),
                content: serde_json::json!({
                    "analysis": format!("{} analyzed: {}", agent.name, task),
                    "specialization": format!("{:?}", agent.specialization),
                }),
                confidence: 0.85,
                reasoning: "Based on specialization".to_string(),
                timestamp: Utc::now(),
            })
        }

        async fn respond_to_message(
            &self,
            agent: &Agent,
            message: &AgentMessage,
        ) -> Result<AgentMessage> {
            Ok(AgentMessage {
                id: Uuid::new_v4().to_string(),
                from: agent.id.clone(),
                to: message.from.clone(),
                message_type: MessageType::Ack,
                content: serde_json::json!({ "received": true }),
                timestamp: Utc::now(),
                requires_ack: false,
            })
        }

        async fn vote(&self, agent: &Agent, _proposal: &str, options: &[String]) -> Result<Vote> {
            Ok(Vote {
                agent_id: agent.id.clone(),
                choice: options.first().cloned().unwrap_or_default(),
                confidence: 0.8,
                reasoning: "Best option".to_string(),
            })
        }

        async fn synthesize(
            &self,
            contributions: &[AgentContribution],
        ) -> Result<serde_json::Value> {
            Ok(serde_json::json!({
                "synthesized": true,
                "contribution_count": contributions.len(),
                "combined": contributions.iter().map(|c| &c.content).collect::<Vec<_>>(),
            }))
        }
    }

    #[tokio::test]
    async fn test_spawn_agent() {
        let provider = Arc::new(MockProvider);
        let coordinator = SwarmCoordinator::new(provider);

        let agent = coordinator
            .spawn_agent("TestCoder", AgentSpecialization::Coder)
            .await
            .unwrap();

        assert_eq!(agent.name, "TestCoder");
        assert_eq!(agent.specialization, AgentSpecialization::Coder);
        assert!(agent.capabilities.contains(&"code_analysis".to_string()));
    }

    #[tokio::test]
    async fn test_create_task() {
        let provider = Arc::new(MockProvider);
        let coordinator = SwarmCoordinator::new(provider);

        let task = coordinator
            .create_task(
                "Analyze code",
                vec![AgentSpecialization::Coder, AgentSpecialization::Critic],
                0.8,
            )
            .await
            .unwrap();

        assert_eq!(task.description, "Analyze code");
        assert_eq!(task.consensus_threshold, 0.8);
        assert_eq!(task.required_specializations.len(), 2);
    }

    #[tokio::test]
    async fn test_execute_task() {
        let provider = Arc::new(MockProvider);
        let coordinator = SwarmCoordinator::new(provider);

        // Pre-spawn required agents
        coordinator
            .spawn_agent("Coder1", AgentSpecialization::Coder)
            .await
            .unwrap();
        coordinator
            .spawn_agent("Critic1", AgentSpecialization::Critic)
            .await
            .unwrap();

        let task = coordinator
            .create_task(
                "Review implementation",
                vec![AgentSpecialization::Coder, AgentSpecialization::Critic],
                0.7,
            )
            .await
            .unwrap();

        let result = coordinator.execute_task(&task.id).await.unwrap();

        assert!(result.consensus_level > 0.0);
        assert_eq!(result.participants.len(), 2);
    }

    #[tokio::test]
    async fn test_message_broadcast() {
        let provider = Arc::new(MockProvider);
        let coordinator = SwarmCoordinator::new(provider);

        let mut receiver = coordinator.subscribe();

        let message = AgentMessage {
            id: Uuid::new_v4().to_string(),
            from: "agent1".to_string(),
            to: "broadcast".to_string(),
            message_type: MessageType::Information,
            content: serde_json::json!({ "info": "test" }),
            timestamp: Utc::now(),
            requires_ack: false,
        };

        coordinator.send_message(message.clone()).await.unwrap();

        let received = receiver.recv().await.unwrap();
        assert_eq!(received.id, message.id);
    }

    #[tokio::test]
    async fn test_agent_capabilities() {
        let provider = Arc::new(MockProvider);
        let coordinator = SwarmCoordinator::new(provider);

        let researcher = coordinator
            .spawn_agent("R1", AgentSpecialization::Researcher)
            .await
            .unwrap();
        assert!(researcher.capabilities.contains(&"search".to_string()));
        assert!(researcher.capabilities.contains(&"fact_check".to_string()));

        let creative = coordinator
            .spawn_agent("C1", AgentSpecialization::Creative)
            .await
            .unwrap();
        assert!(creative.capabilities.contains(&"ideation".to_string()));
        assert!(creative.capabilities.contains(&"brainstorm".to_string()));
    }

    #[test]
    fn test_serialization() {
        let agent = Agent {
            id: "test".to_string(),
            name: "TestAgent".to_string(),
            specialization: AgentSpecialization::Analyst,
            state: AgentState::Idle,
            capabilities: vec!["analyze".to_string()],
            current_task: None,
            metrics: AgentMetrics::default(),
            created_at: Utc::now(),
        };

        let json = serde_json::to_string(&agent).unwrap();
        let _: Agent = serde_json::from_str(&json).unwrap();
    }
}
