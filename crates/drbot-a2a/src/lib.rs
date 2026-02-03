//! AI-to-AI protocol for drbot.
//!
//! Communication protocol for AI agent interoperability.
//!
//! # Features
//!
//! - Agent discovery
//! - Capability negotiation
//! - Message passing
//! - Task delegation

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;

/// A2A result type.
pub type Result<T> = std::result::Result<T, A2AError>;

/// A2A errors.
#[derive(Debug, thiserror::Error)]
pub enum A2AError {
    #[error("Agent not found: {0}")]
    AgentNotFound(Uuid),
    #[error("Capability not supported: {0}")]
    CapabilityNotSupported(String),
    #[error("Communication failed: {0}")]
    CommunicationFailed(String),
    #[error("Protocol error: {0}")]
    ProtocolError(String),
    #[error("Timeout")]
    Timeout,
}

/// An AI agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    /// Agent ID.
    pub id: Uuid,
    /// Agent name.
    pub name: String,
    /// Agent type.
    pub agent_type: String,
    /// Capabilities.
    pub capabilities: Vec<Capability>,
    /// Endpoint.
    pub endpoint: Option<String>,
    /// Status.
    pub status: AgentStatus,
    /// Last seen.
    pub last_seen: DateTime<Utc>,
    /// Metadata.
    pub metadata: HashMap<String, serde_json::Value>,
}

impl Agent {
    /// Create a new agent.
    pub fn new(name: &str, agent_type: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.to_string(),
            agent_type: agent_type.to_string(),
            capabilities: Vec::new(),
            endpoint: None,
            status: AgentStatus::Online,
            last_seen: Utc::now(),
            metadata: HashMap::new(),
        }
    }

    /// Add a capability.
    pub fn with_capability(mut self, capability: Capability) -> Self {
        self.capabilities.push(capability);
        self
    }

    /// Check if agent has capability.
    pub fn has_capability(&self, name: &str) -> bool {
        self.capabilities.iter().any(|c| c.name == name)
    }
}

/// Agent status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    Online,
    Busy,
    Offline,
    Error,
}

/// Agent capability.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Capability {
    /// Capability name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Version.
    pub version: String,
    /// Input schema.
    pub input_schema: Option<serde_json::Value>,
    /// Output schema.
    pub output_schema: Option<serde_json::Value>,
}

impl Capability {
    /// Create a new capability.
    pub fn new(name: &str, description: &str) -> Self {
        Self {
            name: name.to_string(),
            description: description.to_string(),
            version: "1.0.0".to_string(),
            input_schema: None,
            output_schema: None,
        }
    }
}

/// A2A message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct A2AMessage {
    /// Message ID.
    pub id: Uuid,
    /// Sender agent ID.
    pub from: Uuid,
    /// Recipient agent ID.
    pub to: Uuid,
    /// Message type.
    pub message_type: MessageType,
    /// Payload.
    pub payload: serde_json::Value,
    /// Correlation ID (for request-response).
    pub correlation_id: Option<Uuid>,
    /// Sent at.
    pub sent_at: DateTime<Utc>,
    /// TTL in seconds.
    pub ttl_secs: Option<u64>,
}

impl A2AMessage {
    /// Create a new message.
    pub fn new(
        from: Uuid,
        to: Uuid,
        message_type: MessageType,
        payload: serde_json::Value,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            from,
            to,
            message_type,
            payload,
            correlation_id: None,
            sent_at: Utc::now(),
            ttl_secs: None,
        }
    }

    /// Create a request.
    pub fn request(from: Uuid, to: Uuid, capability: &str, params: serde_json::Value) -> Self {
        Self::new(
            from,
            to,
            MessageType::Request,
            serde_json::json!({
                "capability": capability,
                "params": params
            }),
        )
    }

    /// Create a response.
    pub fn response(from: Uuid, to: Uuid, correlation_id: Uuid, result: serde_json::Value) -> Self {
        let mut msg = Self::new(from, to, MessageType::Response, result);
        msg.correlation_id = Some(correlation_id);
        msg
    }

    /// Create an error response.
    pub fn error(from: Uuid, to: Uuid, correlation_id: Uuid, error: &str) -> Self {
        let mut msg = Self::new(
            from,
            to,
            MessageType::Error,
            serde_json::json!({
                "error": error
            }),
        );
        msg.correlation_id = Some(correlation_id);
        msg
    }
}

/// Message types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageType {
    /// Capability request.
    Request,
    /// Capability response.
    Response,
    /// Error.
    Error,
    /// Notification (no response expected).
    Notification,
    /// Discovery.
    Discovery,
    /// Heartbeat.
    Heartbeat,
}

/// Task delegation request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskDelegation {
    /// Delegation ID.
    pub id: Uuid,
    /// Delegating agent.
    pub from: Uuid,
    /// Target agent.
    pub to: Uuid,
    /// Task description.
    pub task: String,
    /// Required capabilities.
    pub required_capabilities: Vec<String>,
    /// Context.
    pub context: serde_json::Value,
    /// Priority.
    pub priority: Priority,
    /// Deadline.
    pub deadline: Option<DateTime<Utc>>,
    /// Status.
    pub status: TaskStatus,
    /// Result.
    pub result: Option<serde_json::Value>,
    /// Created at.
    pub created_at: DateTime<Utc>,
}

impl TaskDelegation {
    /// Create a new delegation.
    pub fn new(from: Uuid, to: Uuid, task: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            from,
            to,
            task: task.to_string(),
            required_capabilities: Vec::new(),
            context: serde_json::Value::Null,
            priority: Priority::Normal,
            deadline: None,
            status: TaskStatus::Pending,
            result: None,
            created_at: Utc::now(),
        }
    }

    /// Add required capabilities.
    pub fn with_capabilities(mut self, capabilities: Vec<String>) -> Self {
        self.required_capabilities = capabilities;
        self
    }

    /// Set context.
    pub fn with_context(mut self, context: serde_json::Value) -> Self {
        self.context = context;
        self
    }

    /// Set priority.
    pub fn with_priority(mut self, priority: Priority) -> Self {
        self.priority = priority;
        self
    }
}

/// Task priority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
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
    InProgress,
    Completed,
    Failed,
    Cancelled,
}

/// A2A configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct A2AConfig {
    /// Discovery interval in seconds.
    pub discovery_interval_secs: u64,
    /// Heartbeat interval in seconds.
    pub heartbeat_interval_secs: u64,
    /// Message timeout in seconds.
    pub message_timeout_secs: u64,
    /// Max message size.
    pub max_message_size: usize,
}

impl Default for A2AConfig {
    fn default() -> Self {
        Self {
            discovery_interval_secs: 60,
            heartbeat_interval_secs: 30,
            message_timeout_secs: 30,
            max_message_size: 1024 * 1024,
        }
    }
}

/// Trait for message handlers.
#[async_trait]
pub trait MessageHandler: Send + Sync {
    /// Handle incoming message.
    async fn handle(&self, message: A2AMessage) -> Result<Option<A2AMessage>>;
}

/// A2A hub for agent coordination.
pub struct A2AHub {
    config: A2AConfig,
    local_agent: Agent,
    agents: Arc<RwLock<HashMap<Uuid, Agent>>>,
    delegations: Arc<RwLock<HashMap<Uuid, TaskDelegation>>>,
    message_tx: broadcast::Sender<A2AMessage>,
}

impl A2AHub {
    /// Create a new A2A hub.
    pub fn new(config: A2AConfig, local_agent: Agent) -> Self {
        let (message_tx, _) = broadcast::channel(100);
        let local_id = local_agent.id;

        let mut agents = HashMap::new();
        agents.insert(local_id, local_agent.clone());

        Self {
            config,
            local_agent,
            agents: Arc::new(RwLock::new(agents)),
            delegations: Arc::new(RwLock::new(HashMap::new())),
            message_tx,
        }
    }

    /// Get local agent ID.
    pub fn local_id(&self) -> Uuid {
        self.local_agent.id
    }

    /// Register a remote agent.
    pub async fn register_agent(&self, agent: Agent) {
        self.agents.write().await.insert(agent.id, agent);
    }

    /// Discover agents with capability.
    pub async fn discover(&self, capability: &str) -> Vec<Agent> {
        self.agents
            .read()
            .await
            .values()
            .filter(|a| a.has_capability(capability) && a.status == AgentStatus::Online)
            .cloned()
            .collect()
    }

    /// Send a message.
    pub async fn send(&self, message: A2AMessage) -> Result<()> {
        // Check recipient exists
        let agents = self.agents.read().await;
        if !agents.contains_key(&message.to) {
            return Err(A2AError::AgentNotFound(message.to));
        }
        drop(agents);

        let _ = self.message_tx.send(message);

        Ok(())
    }

    /// Request a capability from an agent.
    pub async fn request(
        &self,
        to: Uuid,
        capability: &str,
        params: serde_json::Value,
    ) -> Result<A2AMessage> {
        // Check agent has capability
        let agents = self.agents.read().await;
        let agent = agents.get(&to).ok_or(A2AError::AgentNotFound(to))?;

        if !agent.has_capability(capability) {
            return Err(A2AError::CapabilityNotSupported(capability.to_string()));
        }
        drop(agents);

        let request = A2AMessage::request(self.local_agent.id, to, capability, params);
        self.send(request.clone()).await?;

        // In a real implementation, would wait for response
        // For now, return the request
        Ok(request)
    }

    /// Delegate a task.
    pub async fn delegate(&self, delegation: TaskDelegation) -> Result<Uuid> {
        let id = delegation.id;

        // Verify target agent
        let agents = self.agents.read().await;
        let target = agents
            .get(&delegation.to)
            .ok_or(A2AError::AgentNotFound(delegation.to))?;

        // Check capabilities
        for cap in &delegation.required_capabilities {
            if !target.has_capability(cap) {
                return Err(A2AError::CapabilityNotSupported(cap.clone()));
            }
        }
        drop(agents);

        self.delegations.write().await.insert(id, delegation);

        Ok(id)
    }

    /// Get delegation status.
    pub async fn delegation_status(&self, delegation_id: Uuid) -> Option<TaskStatus> {
        self.delegations
            .read()
            .await
            .get(&delegation_id)
            .map(|d| d.status)
    }

    /// Update delegation status.
    pub async fn update_delegation(
        &self,
        delegation_id: Uuid,
        status: TaskStatus,
        result: Option<serde_json::Value>,
    ) {
        let mut delegations = self.delegations.write().await;
        if let Some(d) = delegations.get_mut(&delegation_id) {
            d.status = status;
            d.result = result;
        }
    }

    /// Subscribe to messages.
    pub fn subscribe(&self) -> broadcast::Receiver<A2AMessage> {
        self.message_tx.subscribe()
    }

    /// List all agents.
    pub async fn list_agents(&self) -> Vec<Agent> {
        self.agents.read().await.values().cloned().collect()
    }

    /// List delegations.
    pub async fn list_delegations(&self) -> Vec<TaskDelegation> {
        self.delegations.read().await.values().cloned().collect()
    }

    /// Get statistics.
    pub async fn stats(&self) -> A2AStats {
        let agents = self.agents.read().await;
        let delegations = self.delegations.read().await;

        A2AStats {
            total_agents: agents.len(),
            online_agents: agents
                .values()
                .filter(|a| a.status == AgentStatus::Online)
                .count(),
            total_delegations: delegations.len(),
            pending_delegations: delegations
                .values()
                .filter(|d| d.status == TaskStatus::Pending)
                .count(),
            completed_delegations: delegations
                .values()
                .filter(|d| d.status == TaskStatus::Completed)
                .count(),
        }
    }
}

/// A2A statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct A2AStats {
    pub total_agents: usize,
    pub online_agents: usize,
    pub total_delegations: usize,
    pub pending_delegations: usize,
    pub completed_delegations: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_agent_registration() {
        let local = Agent::new("local", "coordinator");
        let hub = A2AHub::new(A2AConfig::default(), local);

        let remote = Agent::new("remote", "worker")
            .with_capability(Capability::new("search", "Search capability"));

        hub.register_agent(remote).await;

        // Small delay to let the async insert complete
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        let agents = hub.list_agents().await;
        assert_eq!(agents.len(), 2);
    }

    #[tokio::test]
    async fn test_capability_discovery() {
        let local = Agent::new("local", "coordinator");
        let hub = A2AHub::new(A2AConfig::default(), local);

        let worker1 =
            Agent::new("worker1", "worker").with_capability(Capability::new("search", "Search"));

        let worker2 =
            Agent::new("worker2", "worker").with_capability(Capability::new("analyze", "Analyze"));

        hub.register_agent(worker1).await;
        hub.register_agent(worker2).await;

        let search_agents = hub.discover("search").await;
        assert_eq!(search_agents.len(), 1);
        assert_eq!(search_agents[0].name, "worker1");
    }

    #[tokio::test]
    async fn test_task_delegation() {
        let local = Agent::new("local", "coordinator");
        let hub = A2AHub::new(A2AConfig::default(), local.clone());

        let worker = Agent::new("worker", "worker")
            .with_capability(Capability::new("process", "Process data"));

        hub.register_agent(worker.clone()).await;

        let delegation = TaskDelegation::new(local.id, worker.id, "Process this data")
            .with_capabilities(vec!["process".to_string()]);

        let delegation_id = hub.delegate(delegation).await.unwrap();

        let status = hub.delegation_status(delegation_id).await;
        assert_eq!(status, Some(TaskStatus::Pending));
    }

    #[test]
    fn test_message_creation() {
        let from = Uuid::new_v4();
        let to = Uuid::new_v4();

        let request = A2AMessage::request(from, to, "search", serde_json::json!({"query": "test"}));
        assert_eq!(request.message_type, MessageType::Request);
        assert_eq!(request.from, from);
        assert_eq!(request.to, to);

        let response =
            A2AMessage::response(to, from, request.id, serde_json::json!({"result": "found"}));
        assert_eq!(response.message_type, MessageType::Response);
        assert_eq!(response.correlation_id, Some(request.id));
    }
}
