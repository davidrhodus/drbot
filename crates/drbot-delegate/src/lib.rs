//! Task delegation system for drbot
//!
//! Breaks down complex tasks, manages subtasks, and tracks progress.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;

#[derive(Error, Debug)]
pub enum DelegateError {
    #[error("Task not found: {0}")]
    TaskNotFound(String),
    #[error("Invalid state transition: {0}")]
    InvalidStateTransition(String),
    #[error("Circular dependency detected: {0}")]
    CircularDependency(String),
    #[error("Agent not available: {0}")]
    AgentNotAvailable(String),
    #[error("Provider error: {0}")]
    ProviderError(String),
}

pub type Result<T> = std::result::Result<T, DelegateError>;

// ============================================================================
// Task Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub title: String,
    pub description: String,
    pub task_type: TaskType,
    pub priority: Priority,
    pub status: TaskStatus,
    pub parent_id: Option<String>,
    pub subtask_ids: Vec<String>,
    pub dependencies: Vec<String>,
    pub assigned_to: Option<String>,
    pub created_at: u64,
    pub updated_at: u64,
    pub due_at: Option<u64>,
    pub completed_at: Option<u64>,
    pub estimated_effort: Option<EffortEstimate>,
    pub actual_effort: Option<u64>,
    pub tags: Vec<String>,
    pub metadata: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TaskType {
    Action,
    Decision,
    Research,
    Review,
    Communication,
    Automation,
    Creative,
    Custom(String),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    Critical,
    High,
    Medium,
    Low,
    None,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum TaskStatus {
    Pending,
    Ready,
    InProgress,
    Blocked,
    Paused,
    Completed,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum EffortEstimate {
    Trivial,
    Small,
    Medium,
    Large,
    ExtraLarge,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskBreakdown {
    pub original_task: String,
    pub subtasks: Vec<SubtaskDefinition>,
    pub execution_order: Vec<Vec<String>>,
    pub estimated_total_effort: EffortEstimate,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubtaskDefinition {
    pub id: String,
    pub title: String,
    pub description: String,
    pub task_type: TaskType,
    pub dependencies: Vec<String>,
    pub estimated_effort: EffortEstimate,
    pub can_automate: bool,
    pub requires_human: bool,
}

// ============================================================================
// Agent Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    pub id: String,
    pub name: String,
    pub agent_type: AgentType,
    pub capabilities: Vec<Capability>,
    pub status: AgentStatus,
    pub current_tasks: Vec<String>,
    pub max_concurrent_tasks: usize,
    pub performance_score: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AgentType {
    Human,
    AI,
    Automated,
    Hybrid,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Capability {
    Research,
    Writing,
    Coding,
    Analysis,
    Communication,
    Design,
    DataEntry,
    Review,
    Decision,
    Custom(String),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum AgentStatus {
    Available,
    Busy,
    Away,
    Offline,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Assignment {
    pub task_id: String,
    pub agent_id: String,
    pub assigned_at: u64,
    pub reason: String,
    pub confidence: f32,
}

// ============================================================================
// Execution Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionPlan {
    pub task_id: String,
    pub phases: Vec<ExecutionPhase>,
    pub checkpoints: Vec<Checkpoint>,
    pub rollback_plan: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionPhase {
    pub phase_number: u32,
    pub name: String,
    pub tasks: Vec<String>,
    pub parallel: bool,
    pub gate_condition: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    pub id: String,
    pub name: String,
    pub after_phase: u32,
    pub validation: String,
    pub requires_approval: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskProgress {
    pub task_id: String,
    pub status: TaskStatus,
    pub progress_percent: f32,
    pub subtasks_completed: usize,
    pub subtasks_total: usize,
    pub blockers: Vec<Blocker>,
    pub last_update: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Blocker {
    pub id: String,
    pub description: String,
    pub blocking_task_id: Option<String>,
    pub blocker_type: BlockerType,
    pub resolution: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum BlockerType {
    Dependency,
    Resource,
    Information,
    Approval,
    Technical,
    External,
}

// ============================================================================
// Provider Trait
// ============================================================================

#[async_trait]
pub trait DelegateProvider: Send + Sync {
    async fn break_down_task(
        &self,
        task_description: &str,
        context: Option<&str>,
    ) -> Result<TaskBreakdown>;
    async fn suggest_assignment(
        &self,
        task: &Task,
        available_agents: &[Agent],
    ) -> Result<Assignment>;
    async fn create_execution_plan(&self, task: &Task, subtasks: &[Task]) -> Result<ExecutionPlan>;
    async fn validate_completion(&self, task: &Task, result: &str) -> Result<ValidationResult>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    pub valid: bool,
    pub completeness: f32,
    pub quality_score: f32,
    pub issues: Vec<String>,
    pub suggestions: Vec<String>,
}

// ============================================================================
// Delegate Engine
// ============================================================================

pub struct DelegateEngine {
    provider: Arc<dyn DelegateProvider>,
    tasks: Arc<RwLock<HashMap<String, Task>>>,
    agents: Arc<RwLock<HashMap<String, Agent>>>,
    assignments: Arc<RwLock<HashMap<String, Assignment>>>,
    execution_plans: Arc<RwLock<HashMap<String, ExecutionPlan>>>,
    next_task_id: Arc<RwLock<u64>>,
}

impl DelegateEngine {
    pub fn new(provider: Arc<dyn DelegateProvider>) -> Self {
        Self {
            provider,
            tasks: Arc::new(RwLock::new(HashMap::new())),
            agents: Arc::new(RwLock::new(HashMap::new())),
            assignments: Arc::new(RwLock::new(HashMap::new())),
            execution_plans: Arc::new(RwLock::new(HashMap::new())),
            next_task_id: Arc::new(RwLock::new(1)),
        }
    }

    fn now() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }

    async fn generate_task_id(&self) -> String {
        let mut id = self.next_task_id.write().await;
        let task_id = format!("task-{}", *id);
        *id += 1;
        task_id
    }

    // Task Management
    pub async fn create_task(
        &self,
        title: &str,
        description: &str,
        task_type: TaskType,
        priority: Priority,
    ) -> Result<Task> {
        let task_id = self.generate_task_id().await;
        let now = Self::now();

        let task = Task {
            id: task_id.clone(),
            title: title.to_string(),
            description: description.to_string(),
            task_type,
            priority,
            status: TaskStatus::Pending,
            parent_id: None,
            subtask_ids: vec![],
            dependencies: vec![],
            assigned_to: None,
            created_at: now,
            updated_at: now,
            due_at: None,
            completed_at: None,
            estimated_effort: None,
            actual_effort: None,
            tags: vec![],
            metadata: HashMap::new(),
        };

        let mut tasks = self.tasks.write().await;
        tasks.insert(task_id, task.clone());

        Ok(task)
    }

    pub async fn get_task(&self, task_id: &str) -> Result<Task> {
        let tasks = self.tasks.read().await;
        tasks
            .get(task_id)
            .cloned()
            .ok_or_else(|| DelegateError::TaskNotFound(task_id.to_string()))
    }

    pub async fn update_task_status(&self, task_id: &str, status: TaskStatus) -> Result<Task> {
        let mut tasks = self.tasks.write().await;
        let task = tasks
            .get_mut(task_id)
            .ok_or_else(|| DelegateError::TaskNotFound(task_id.to_string()))?;

        // Validate state transition
        if !Self::is_valid_transition(task.status, status) {
            return Err(DelegateError::InvalidStateTransition(format!(
                "{:?} -> {:?}",
                task.status, status
            )));
        }

        task.status = status;
        task.updated_at = Self::now();

        if status == TaskStatus::Completed {
            task.completed_at = Some(Self::now());
        }

        Ok(task.clone())
    }

    fn is_valid_transition(from: TaskStatus, to: TaskStatus) -> bool {
        match (from, to) {
            (TaskStatus::Pending, TaskStatus::Ready) => true,
            (TaskStatus::Pending, TaskStatus::Cancelled) => true,
            (TaskStatus::Ready, TaskStatus::InProgress) => true,
            (TaskStatus::Ready, TaskStatus::Blocked) => true,
            (TaskStatus::Ready, TaskStatus::Cancelled) => true,
            (TaskStatus::InProgress, TaskStatus::Completed) => true,
            (TaskStatus::InProgress, TaskStatus::Blocked) => true,
            (TaskStatus::InProgress, TaskStatus::Paused) => true,
            (TaskStatus::InProgress, TaskStatus::Failed) => true,
            (TaskStatus::InProgress, TaskStatus::Cancelled) => true,
            (TaskStatus::Blocked, TaskStatus::Ready) => true,
            (TaskStatus::Blocked, TaskStatus::Cancelled) => true,
            (TaskStatus::Paused, TaskStatus::InProgress) => true,
            (TaskStatus::Paused, TaskStatus::Cancelled) => true,
            _ => false,
        }
    }

    pub async fn break_down_task(&self, task_id: &str) -> Result<Vec<Task>> {
        let task = self.get_task(task_id).await?;

        let breakdown = self
            .provider
            .break_down_task(&task.description, None)
            .await?;

        let mut created_subtasks = Vec::new();
        let mut id_mapping: HashMap<String, String> = HashMap::new();

        // Create subtasks
        for subtask_def in &breakdown.subtasks {
            let subtask = self
                .create_task(
                    &subtask_def.title,
                    &subtask_def.description,
                    subtask_def.task_type.clone(),
                    task.priority,
                )
                .await?;

            id_mapping.insert(subtask_def.id.clone(), subtask.id.clone());
            created_subtasks.push(subtask);
        }

        // Set up dependencies and parent relationships
        {
            let mut tasks = self.tasks.write().await;

            for (i, subtask_def) in breakdown.subtasks.iter().enumerate() {
                if let Some(subtask) = created_subtasks.get_mut(i) {
                    // Map dependencies
                    subtask.dependencies = subtask_def
                        .dependencies
                        .iter()
                        .filter_map(|dep| id_mapping.get(dep).cloned())
                        .collect();

                    subtask.parent_id = Some(task_id.to_string());
                    subtask.estimated_effort = Some(subtask_def.estimated_effort);

                    // Update in storage
                    tasks.insert(subtask.id.clone(), subtask.clone());
                }
            }

            // Update parent with subtask IDs
            if let Some(parent) = tasks.get_mut(task_id) {
                parent.subtask_ids = created_subtasks.iter().map(|t| t.id.clone()).collect();
                parent.updated_at = Self::now();
            }
        }

        // Update task statuses based on dependencies
        self.update_ready_status().await?;

        Ok(created_subtasks)
    }

    async fn update_ready_status(&self) -> Result<()> {
        let mut tasks = self.tasks.write().await;
        let task_ids: Vec<String> = tasks.keys().cloned().collect();

        for task_id in task_ids {
            let should_be_ready = {
                let task = tasks.get(&task_id).unwrap();
                if task.status != TaskStatus::Pending {
                    continue;
                }

                task.dependencies.iter().all(|dep_id| {
                    tasks
                        .get(dep_id)
                        .map(|dep| dep.status == TaskStatus::Completed)
                        .unwrap_or(true)
                })
            };

            if should_be_ready {
                if let Some(task) = tasks.get_mut(&task_id) {
                    task.status = TaskStatus::Ready;
                    task.updated_at = Self::now();
                }
            }
        }

        Ok(())
    }

    // Agent Management
    pub async fn register_agent(&self, agent: Agent) -> Result<()> {
        let mut agents = self.agents.write().await;
        agents.insert(agent.id.clone(), agent);
        Ok(())
    }

    pub async fn get_agent(&self, agent_id: &str) -> Result<Agent> {
        let agents = self.agents.read().await;
        agents
            .get(agent_id)
            .cloned()
            .ok_or_else(|| DelegateError::AgentNotAvailable(agent_id.to_string()))
    }

    pub async fn get_available_agents(&self) -> Vec<Agent> {
        let agents = self.agents.read().await;
        agents
            .values()
            .filter(|a| {
                a.status == AgentStatus::Available && a.current_tasks.len() < a.max_concurrent_tasks
            })
            .cloned()
            .collect()
    }

    // Assignment
    pub async fn assign_task(&self, task_id: &str, agent_id: &str) -> Result<Assignment> {
        let task = self.get_task(task_id).await?;
        let agent = self.get_agent(agent_id).await?;

        if agent.status != AgentStatus::Available {
            return Err(DelegateError::AgentNotAvailable(agent_id.to_string()));
        }

        let assignment = Assignment {
            task_id: task_id.to_string(),
            agent_id: agent_id.to_string(),
            assigned_at: Self::now(),
            reason: "Manual assignment".to_string(),
            confidence: 1.0,
        };

        // Update task
        {
            let mut tasks = self.tasks.write().await;
            if let Some(task_mut) = tasks.get_mut(task_id) {
                task_mut.assigned_to = Some(agent_id.to_string());
                task_mut.updated_at = Self::now();
            }
        }

        // Update agent
        {
            let mut agents = self.agents.write().await;
            if let Some(agent_mut) = agents.get_mut(agent_id) {
                agent_mut.current_tasks.push(task_id.to_string());
                if agent_mut.current_tasks.len() >= agent_mut.max_concurrent_tasks {
                    agent_mut.status = AgentStatus::Busy;
                }
            }
        }

        let mut assignments = self.assignments.write().await;
        assignments.insert(task_id.to_string(), assignment.clone());

        Ok(assignment)
    }

    pub async fn auto_assign_task(&self, task_id: &str) -> Result<Assignment> {
        let task = self.get_task(task_id).await?;
        let available = self.get_available_agents().await;

        if available.is_empty() {
            return Err(DelegateError::AgentNotAvailable(
                "No agents available".to_string(),
            ));
        }

        let assignment = self.provider.suggest_assignment(&task, &available).await?;

        // Apply assignment
        self.assign_task(task_id, &assignment.agent_id).await
    }

    // Execution
    pub async fn create_execution_plan(&self, task_id: &str) -> Result<ExecutionPlan> {
        let task = self.get_task(task_id).await?;

        let subtasks: Vec<Task> = {
            let tasks = self.tasks.read().await;
            task.subtask_ids
                .iter()
                .filter_map(|id| tasks.get(id).cloned())
                .collect()
        };

        let plan = self
            .provider
            .create_execution_plan(&task, &subtasks)
            .await?;

        let mut plans = self.execution_plans.write().await;
        plans.insert(task_id.to_string(), plan.clone());

        Ok(plan)
    }

    pub async fn get_task_progress(&self, task_id: &str) -> Result<TaskProgress> {
        let task = self.get_task(task_id).await?;

        let (completed, total) = if task.subtask_ids.is_empty() {
            let is_done = task.status == TaskStatus::Completed;
            (if is_done { 1 } else { 0 }, 1)
        } else {
            let tasks = self.tasks.read().await;
            let completed = task
                .subtask_ids
                .iter()
                .filter(|id| {
                    tasks
                        .get(*id)
                        .map(|t| t.status == TaskStatus::Completed)
                        .unwrap_or(false)
                })
                .count();
            (completed, task.subtask_ids.len())
        };

        let progress_percent = if total > 0 {
            (completed as f32 / total as f32) * 100.0
        } else {
            0.0
        };

        // Find blockers
        let blockers = {
            let tasks = self.tasks.read().await;
            task.dependencies
                .iter()
                .filter_map(|dep_id| {
                    tasks.get(dep_id).and_then(|dep| {
                        if dep.status != TaskStatus::Completed {
                            Some(Blocker {
                                id: format!("blocker-{}", dep_id),
                                description: format!("Waiting for: {}", dep.title),
                                blocking_task_id: Some(dep_id.clone()),
                                blocker_type: BlockerType::Dependency,
                                resolution: None,
                            })
                        } else {
                            None
                        }
                    })
                })
                .collect()
        };

        Ok(TaskProgress {
            task_id: task_id.to_string(),
            status: task.status,
            progress_percent,
            subtasks_completed: completed,
            subtasks_total: total,
            blockers,
            last_update: task.updated_at,
        })
    }

    pub async fn complete_task(&self, task_id: &str, result: &str) -> Result<ValidationResult> {
        let task = self.get_task(task_id).await?;

        let validation = self.provider.validate_completion(&task, result).await?;

        if validation.valid {
            self.update_task_status(task_id, TaskStatus::Completed)
                .await?;

            // Release agent
            if let Some(agent_id) = &task.assigned_to {
                let mut agents = self.agents.write().await;
                if let Some(agent) = agents.get_mut(agent_id) {
                    agent.current_tasks.retain(|t| t != task_id);
                    if agent.current_tasks.len() < agent.max_concurrent_tasks {
                        agent.status = AgentStatus::Available;
                    }
                }
            }

            // Check if parent task can be completed
            if let Some(parent_id) = &task.parent_id {
                let can_complete_parent = {
                    let tasks = self.tasks.read().await;
                    if let Some(parent) = tasks.get(parent_id) {
                        parent.subtask_ids.iter().all(|id| {
                            tasks
                                .get(id)
                                .map(|t| t.status == TaskStatus::Completed)
                                .unwrap_or(false)
                        })
                    } else {
                        false
                    }
                };

                if can_complete_parent {
                    self.update_task_status(parent_id, TaskStatus::Completed)
                        .await?;
                }
            }

            // Update ready statuses
            self.update_ready_status().await?;
        }

        Ok(validation)
    }

    // Queries
    pub async fn get_tasks_by_status(&self, status: TaskStatus) -> Vec<Task> {
        let tasks = self.tasks.read().await;
        tasks
            .values()
            .filter(|t| t.status == status)
            .cloned()
            .collect()
    }

    pub async fn get_tasks_by_priority(&self, priority: Priority) -> Vec<Task> {
        let tasks = self.tasks.read().await;
        tasks
            .values()
            .filter(|t| t.priority == priority)
            .cloned()
            .collect()
    }

    pub async fn get_agent_tasks(&self, agent_id: &str) -> Vec<Task> {
        let tasks = self.tasks.read().await;
        tasks
            .values()
            .filter(|t| t.assigned_to.as_deref() == Some(agent_id))
            .cloned()
            .collect()
    }

    pub async fn get_ready_tasks(&self) -> Vec<Task> {
        self.get_tasks_by_status(TaskStatus::Ready).await
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    struct MockProvider;

    #[async_trait]
    impl DelegateProvider for MockProvider {
        async fn break_down_task(
            &self,
            task_description: &str,
            _context: Option<&str>,
        ) -> Result<TaskBreakdown> {
            Ok(TaskBreakdown {
                original_task: task_description.to_string(),
                subtasks: vec![
                    SubtaskDefinition {
                        id: "sub-1".to_string(),
                        title: "Research".to_string(),
                        description: "Research the topic".to_string(),
                        task_type: TaskType::Research,
                        dependencies: vec![],
                        estimated_effort: EffortEstimate::Small,
                        can_automate: false,
                        requires_human: true,
                    },
                    SubtaskDefinition {
                        id: "sub-2".to_string(),
                        title: "Draft".to_string(),
                        description: "Create draft".to_string(),
                        task_type: TaskType::Action,
                        dependencies: vec!["sub-1".to_string()],
                        estimated_effort: EffortEstimate::Medium,
                        can_automate: true,
                        requires_human: false,
                    },
                ],
                execution_order: vec![vec!["sub-1".to_string()], vec!["sub-2".to_string()]],
                estimated_total_effort: EffortEstimate::Medium,
            })
        }

        async fn suggest_assignment(
            &self,
            _task: &Task,
            available_agents: &[Agent],
        ) -> Result<Assignment> {
            let agent = available_agents
                .first()
                .ok_or_else(|| DelegateError::AgentNotAvailable("No agents".to_string()))?;

            Ok(Assignment {
                task_id: "task".to_string(),
                agent_id: agent.id.clone(),
                assigned_at: 0,
                reason: "Best match".to_string(),
                confidence: 0.9,
            })
        }

        async fn create_execution_plan(
            &self,
            task: &Task,
            subtasks: &[Task],
        ) -> Result<ExecutionPlan> {
            Ok(ExecutionPlan {
                task_id: task.id.clone(),
                phases: vec![ExecutionPhase {
                    phase_number: 1,
                    name: "Execution".to_string(),
                    tasks: subtasks.iter().map(|t| t.id.clone()).collect(),
                    parallel: false,
                    gate_condition: None,
                }],
                checkpoints: vec![],
                rollback_plan: None,
            })
        }

        async fn validate_completion(
            &self,
            _task: &Task,
            _result: &str,
        ) -> Result<ValidationResult> {
            Ok(ValidationResult {
                valid: true,
                completeness: 1.0,
                quality_score: 0.9,
                issues: vec![],
                suggestions: vec![],
            })
        }
    }

    #[tokio::test]
    async fn test_task_creation() {
        let provider = Arc::new(MockProvider);
        let engine = DelegateEngine::new(provider);

        let task = engine
            .create_task(
                "Test Task",
                "Description",
                TaskType::Action,
                Priority::Medium,
            )
            .await
            .unwrap();

        assert_eq!(task.title, "Test Task");
        assert_eq!(task.status, TaskStatus::Pending);

        let retrieved = engine.get_task(&task.id).await.unwrap();
        assert_eq!(retrieved.id, task.id);
    }

    #[tokio::test]
    async fn test_task_breakdown() {
        let provider = Arc::new(MockProvider);
        let engine = DelegateEngine::new(provider);

        let task = engine
            .create_task(
                "Complex Task",
                "A complex task that needs breakdown",
                TaskType::Action,
                Priority::High,
            )
            .await
            .unwrap();

        let subtasks = engine.break_down_task(&task.id).await.unwrap();
        assert_eq!(subtasks.len(), 2);

        // Check parent was updated
        let updated_task = engine.get_task(&task.id).await.unwrap();
        assert_eq!(updated_task.subtask_ids.len(), 2);
    }

    #[tokio::test]
    async fn test_status_transitions() {
        let provider = Arc::new(MockProvider);
        let engine = DelegateEngine::new(provider);

        let task = engine
            .create_task("Task", "Description", TaskType::Action, Priority::Medium)
            .await
            .unwrap();

        // Pending -> Ready
        let updated = engine
            .update_task_status(&task.id, TaskStatus::Ready)
            .await
            .unwrap();
        assert_eq!(updated.status, TaskStatus::Ready);

        // Ready -> InProgress
        let updated = engine
            .update_task_status(&task.id, TaskStatus::InProgress)
            .await
            .unwrap();
        assert_eq!(updated.status, TaskStatus::InProgress);

        // Invalid transition should fail
        let result = engine.update_task_status(&task.id, TaskStatus::Ready).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_agent_management() {
        let provider = Arc::new(MockProvider);
        let engine = DelegateEngine::new(provider);

        let agent = Agent {
            id: "agent-1".to_string(),
            name: "Test Agent".to_string(),
            agent_type: AgentType::AI,
            capabilities: vec![Capability::Research, Capability::Writing],
            status: AgentStatus::Available,
            current_tasks: vec![],
            max_concurrent_tasks: 3,
            performance_score: 0.9,
        };

        engine.register_agent(agent).await.unwrap();

        let available = engine.get_available_agents().await;
        assert_eq!(available.len(), 1);
    }

    #[tokio::test]
    async fn test_task_assignment() {
        let provider = Arc::new(MockProvider);
        let engine = DelegateEngine::new(provider);

        let task = engine
            .create_task("Task", "Description", TaskType::Research, Priority::Medium)
            .await
            .unwrap();

        let agent = Agent {
            id: "agent-1".to_string(),
            name: "Agent".to_string(),
            agent_type: AgentType::AI,
            capabilities: vec![Capability::Research],
            status: AgentStatus::Available,
            current_tasks: vec![],
            max_concurrent_tasks: 1,
            performance_score: 0.8,
        };

        engine.register_agent(agent).await.unwrap();

        let assignment = engine.assign_task(&task.id, "agent-1").await.unwrap();
        assert_eq!(assignment.agent_id, "agent-1");

        // Agent should now be busy (max 1 task)
        let agent = engine.get_agent("agent-1").await.unwrap();
        assert_eq!(agent.status, AgentStatus::Busy);
    }

    #[tokio::test]
    async fn test_task_progress() {
        let provider = Arc::new(MockProvider);
        let engine = DelegateEngine::new(provider);

        let task = engine
            .create_task(
                "Parent Task",
                "Description",
                TaskType::Action,
                Priority::Medium,
            )
            .await
            .unwrap();

        engine.break_down_task(&task.id).await.unwrap();

        let progress = engine.get_task_progress(&task.id).await.unwrap();
        assert_eq!(progress.subtasks_total, 2);
        assert_eq!(progress.subtasks_completed, 0);
        assert_eq!(progress.progress_percent, 0.0);
    }

    #[tokio::test]
    async fn test_task_completion() {
        let provider = Arc::new(MockProvider);
        let engine = DelegateEngine::new(provider);

        let task = engine
            .create_task("Task", "Description", TaskType::Action, Priority::Medium)
            .await
            .unwrap();

        engine
            .update_task_status(&task.id, TaskStatus::Ready)
            .await
            .unwrap();
        engine
            .update_task_status(&task.id, TaskStatus::InProgress)
            .await
            .unwrap();

        let validation = engine.complete_task(&task.id, "Done!").await.unwrap();
        assert!(validation.valid);

        let completed = engine.get_task(&task.id).await.unwrap();
        assert_eq!(completed.status, TaskStatus::Completed);
    }
}
