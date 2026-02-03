//! Goal decomposition for drbot.
//!
//! Break complex tasks into executable subtasks.
//!
//! # Features
//!
//! - Automatic goal decomposition
//! - Dependency tracking
//! - Progress monitoring
//! - Parallel execution support

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Goal result type.
pub type Result<T> = std::result::Result<T, GoalError>;

/// Goal errors.
#[derive(Debug, thiserror::Error)]
pub enum GoalError {
    #[error("Goal not found: {0}")]
    GoalNotFound(Uuid),
    #[error("Subtask not found: {0}")]
    SubtaskNotFound(Uuid),
    #[error("Circular dependency detected")]
    CircularDependency,
    #[error("Decomposition failed: {0}")]
    DecompositionFailed(String),
    #[error("Execution failed: {0}")]
    ExecutionFailed(String),
    #[error("Goal already completed")]
    AlreadyCompleted,
}

/// A high-level goal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Goal {
    /// Goal ID.
    pub id: Uuid,
    /// Goal description.
    pub description: String,
    /// Subtasks.
    pub subtasks: Vec<Subtask>,
    /// Current status.
    pub status: GoalStatus,
    /// Priority (0-10).
    pub priority: u8,
    /// Created at.
    pub created_at: DateTime<Utc>,
    /// Completed at.
    pub completed_at: Option<DateTime<Utc>>,
    /// Deadline.
    pub deadline: Option<DateTime<Utc>>,
    /// Result.
    pub result: Option<GoalResult>,
    /// Metadata.
    pub metadata: HashMap<String, serde_json::Value>,
}

impl Goal {
    /// Create a new goal.
    pub fn new(description: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            description: description.to_string(),
            subtasks: Vec::new(),
            status: GoalStatus::Pending,
            priority: 5,
            created_at: Utc::now(),
            completed_at: None,
            deadline: None,
            result: None,
            metadata: HashMap::new(),
        }
    }

    /// Set priority.
    pub fn with_priority(mut self, priority: u8) -> Self {
        self.priority = priority.min(10);
        self
    }

    /// Set deadline.
    pub fn with_deadline(mut self, deadline: DateTime<Utc>) -> Self {
        self.deadline = Some(deadline);
        self
    }

    /// Add a subtask.
    pub fn add_subtask(&mut self, subtask: Subtask) {
        self.subtasks.push(subtask);
    }

    /// Get progress (0.0 - 1.0).
    pub fn progress(&self) -> f32 {
        if self.subtasks.is_empty() {
            return 0.0;
        }

        let completed = self
            .subtasks
            .iter()
            .filter(|s| s.status == SubtaskStatus::Completed)
            .count();
        completed as f32 / self.subtasks.len() as f32
    }

    /// Check if all dependencies are satisfied for a subtask.
    pub fn can_execute(&self, subtask_id: Uuid) -> bool {
        let subtask = match self.subtasks.iter().find(|s| s.id == subtask_id) {
            Some(s) => s,
            None => return false,
        };

        subtask.dependencies.iter().all(|dep_id| {
            self.subtasks
                .iter()
                .any(|s| s.id == *dep_id && s.status == SubtaskStatus::Completed)
        })
    }

    /// Get next executable subtasks.
    pub fn next_subtasks(&self) -> Vec<&Subtask> {
        self.subtasks
            .iter()
            .filter(|s| s.status == SubtaskStatus::Pending && self.can_execute(s.id))
            .collect()
    }

    /// Mark goal as completed.
    pub fn complete(&mut self, result: GoalResult) {
        self.status = GoalStatus::Completed;
        self.completed_at = Some(Utc::now());
        self.result = Some(result);
    }

    /// Mark goal as failed.
    pub fn fail(&mut self, reason: &str) {
        self.status = GoalStatus::Failed;
        self.completed_at = Some(Utc::now());
        self.result = Some(GoalResult {
            success: false,
            output: reason.to_string(),
            artifacts: Vec::new(),
        });
    }
}

/// A subtask within a goal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subtask {
    /// Subtask ID.
    pub id: Uuid,
    /// Subtask description.
    pub description: String,
    /// Type of subtask.
    pub task_type: TaskType,
    /// Current status.
    pub status: SubtaskStatus,
    /// Dependencies (other subtask IDs).
    pub dependencies: Vec<Uuid>,
    /// Estimated effort (1-10).
    pub effort: u8,
    /// Actual duration in ms.
    pub duration_ms: Option<u64>,
    /// Retry count.
    pub retries: u8,
    /// Max retries allowed.
    pub max_retries: u8,
    /// Result.
    pub result: Option<String>,
    /// Error message if failed.
    pub error: Option<String>,
}

impl Subtask {
    /// Create a new subtask.
    pub fn new(description: &str, task_type: TaskType) -> Self {
        Self {
            id: Uuid::new_v4(),
            description: description.to_string(),
            task_type,
            status: SubtaskStatus::Pending,
            dependencies: Vec::new(),
            effort: 5,
            duration_ms: None,
            retries: 0,
            max_retries: 3,
            result: None,
            error: None,
        }
    }

    /// Add a dependency.
    pub fn depends_on(mut self, subtask_id: Uuid) -> Self {
        self.dependencies.push(subtask_id);
        self
    }

    /// Set effort.
    pub fn with_effort(mut self, effort: u8) -> Self {
        self.effort = effort.min(10);
        self
    }

    /// Mark as in progress.
    pub fn start(&mut self) {
        self.status = SubtaskStatus::InProgress;
    }

    /// Mark as completed.
    pub fn complete(&mut self, result: &str, duration_ms: u64) {
        self.status = SubtaskStatus::Completed;
        self.result = Some(result.to_string());
        self.duration_ms = Some(duration_ms);
    }

    /// Mark as failed.
    pub fn fail(&mut self, error: &str) {
        self.retries += 1;
        if self.retries >= self.max_retries {
            self.status = SubtaskStatus::Failed;
        } else {
            self.status = SubtaskStatus::Pending;
        }
        self.error = Some(error.to_string());
    }
}

/// Task types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskType {
    /// Information gathering.
    Research,
    /// Content creation.
    Create,
    /// Data analysis.
    Analyze,
    /// Code writing.
    Code,
    /// Testing.
    Test,
    /// Review.
    Review,
    /// Communication.
    Communicate,
    /// Decision making.
    Decide,
    /// External action.
    Execute,
    /// Other.
    Other,
}

/// Goal status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GoalStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
    Cancelled,
}

/// Subtask status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubtaskStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
    Skipped,
}

/// Goal result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoalResult {
    /// Whether the goal succeeded.
    pub success: bool,
    /// Output/summary.
    pub output: String,
    /// Produced artifacts.
    pub artifacts: Vec<Artifact>,
}

/// An artifact produced by a goal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    /// Artifact name.
    pub name: String,
    /// Artifact type.
    pub artifact_type: String,
    /// Content or path.
    pub content: String,
}

/// Trait for goal decomposers.
#[async_trait]
pub trait GoalDecomposer: Send + Sync {
    /// Decompose a goal into subtasks.
    async fn decompose(&self, goal: &str, context: &DecomposeContext) -> Result<Vec<Subtask>>;
}

/// Decomposition context.
#[derive(Debug, Clone, Default)]
pub struct DecomposeContext {
    /// Available capabilities.
    pub capabilities: Vec<String>,
    /// Previous similar goals.
    pub previous_goals: Vec<Goal>,
    /// Constraints.
    pub constraints: Vec<String>,
    /// Custom context.
    pub custom: HashMap<String, serde_json::Value>,
}

/// Goal execution configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionConfig {
    /// Maximum parallel subtasks.
    pub max_parallel: usize,
    /// Retry failed subtasks.
    pub retry_failed: bool,
    /// Stop on first failure.
    pub fail_fast: bool,
    /// Timeout per subtask in ms.
    pub subtask_timeout_ms: u64,
}

impl Default for ExecutionConfig {
    fn default() -> Self {
        Self {
            max_parallel: 3,
            retry_failed: true,
            fail_fast: false,
            subtask_timeout_ms: 60000,
        }
    }
}

/// Goal manager.
pub struct GoalManager<D: GoalDecomposer> {
    decomposer: D,
    config: ExecutionConfig,
    goals: Arc<RwLock<HashMap<Uuid, Goal>>>,
}

impl<D: GoalDecomposer> GoalManager<D> {
    /// Create a new goal manager.
    pub fn new(decomposer: D, config: ExecutionConfig) -> Self {
        Self {
            decomposer,
            config,
            goals: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create a goal from a description.
    pub async fn create_goal(&self, description: &str) -> Result<Goal> {
        let mut goal = Goal::new(description);

        // Decompose into subtasks
        let context = DecomposeContext::default();
        let subtasks = self.decomposer.decompose(description, &context).await?;

        for subtask in subtasks {
            goal.add_subtask(subtask);
        }

        let id = goal.id;
        self.goals.write().await.insert(id, goal.clone());

        Ok(goal)
    }

    /// Get a goal by ID.
    pub async fn get_goal(&self, goal_id: Uuid) -> Option<Goal> {
        self.goals.read().await.get(&goal_id).cloned()
    }

    /// Update a subtask status.
    pub async fn update_subtask(
        &self,
        goal_id: Uuid,
        subtask_id: Uuid,
        status: SubtaskStatus,
        result: Option<&str>,
    ) -> Result<()> {
        let mut goals = self.goals.write().await;
        let goal = goals
            .get_mut(&goal_id)
            .ok_or(GoalError::GoalNotFound(goal_id))?;

        let subtask = goal
            .subtasks
            .iter_mut()
            .find(|s| s.id == subtask_id)
            .ok_or(GoalError::SubtaskNotFound(subtask_id))?;

        subtask.status = status;
        if let Some(r) = result {
            subtask.result = Some(r.to_string());
        }

        // Check if goal is complete
        if goal
            .subtasks
            .iter()
            .all(|s| s.status == SubtaskStatus::Completed || s.status == SubtaskStatus::Skipped)
        {
            goal.complete(GoalResult {
                success: true,
                output: "All subtasks completed".to_string(),
                artifacts: Vec::new(),
            });
        } else if self.config.fail_fast && status == SubtaskStatus::Failed {
            goal.fail("Subtask failed");
        }

        Ok(())
    }

    /// Get next subtasks to execute.
    pub async fn next_subtasks(&self, goal_id: Uuid) -> Result<Vec<Subtask>> {
        let goals = self.goals.read().await;
        let goal = goals
            .get(&goal_id)
            .ok_or(GoalError::GoalNotFound(goal_id))?;

        Ok(goal.next_subtasks().into_iter().cloned().collect())
    }

    /// List all goals.
    pub async fn list_goals(&self) -> Vec<Goal> {
        self.goals.read().await.values().cloned().collect()
    }

    /// Validate goal dependencies (check for cycles).
    pub fn validate_dependencies(&self, goal: &Goal) -> Result<()> {
        let mut visited = HashSet::new();
        let mut stack = HashSet::new();

        fn dfs(
            subtask_id: Uuid,
            deps_map: &HashMap<Uuid, Vec<Uuid>>,
            visited: &mut HashSet<Uuid>,
            stack: &mut HashSet<Uuid>,
        ) -> bool {
            if stack.contains(&subtask_id) {
                return true; // Cycle detected
            }
            if visited.contains(&subtask_id) {
                return false;
            }

            visited.insert(subtask_id);
            stack.insert(subtask_id);

            if let Some(deps) = deps_map.get(&subtask_id) {
                for dep in deps {
                    if dfs(*dep, deps_map, visited, stack) {
                        return true;
                    }
                }
            }

            stack.remove(&subtask_id);
            false
        }

        let deps_map: HashMap<_, _> = goal
            .subtasks
            .iter()
            .map(|s| (s.id, s.dependencies.clone()))
            .collect();

        for subtask in &goal.subtasks {
            if dfs(subtask.id, &deps_map, &mut visited, &mut stack) {
                return Err(GoalError::CircularDependency);
            }
        }

        Ok(())
    }

    /// Get goal statistics.
    pub async fn stats(&self) -> GoalStats {
        let goals = self.goals.read().await;

        let total = goals.len();
        let completed = goals
            .values()
            .filter(|g| g.status == GoalStatus::Completed)
            .count();
        let in_progress = goals
            .values()
            .filter(|g| g.status == GoalStatus::InProgress)
            .count();
        let failed = goals
            .values()
            .filter(|g| g.status == GoalStatus::Failed)
            .count();

        let total_subtasks: usize = goals.values().map(|g| g.subtasks.len()).sum();
        let completed_subtasks: usize = goals
            .values()
            .flat_map(|g| &g.subtasks)
            .filter(|s| s.status == SubtaskStatus::Completed)
            .count();

        GoalStats {
            total_goals: total,
            completed_goals: completed,
            in_progress_goals: in_progress,
            failed_goals: failed,
            total_subtasks,
            completed_subtasks,
            avg_progress: if total > 0 {
                goals.values().map(|g| g.progress()).sum::<f32>() / total as f32
            } else {
                0.0
            },
        }
    }
}

/// Goal statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoalStats {
    pub total_goals: usize,
    pub completed_goals: usize,
    pub in_progress_goals: usize,
    pub failed_goals: usize,
    pub total_subtasks: usize,
    pub completed_subtasks: usize,
    pub avg_progress: f32,
}

/// Simple decomposer for testing.
pub struct SimpleDecomposer;

#[async_trait]
impl GoalDecomposer for SimpleDecomposer {
    async fn decompose(&self, goal: &str, _context: &DecomposeContext) -> Result<Vec<Subtask>> {
        // Simple keyword-based decomposition for testing
        let mut subtasks = Vec::new();

        subtasks.push(
            Subtask::new(&format!("Understand: {}", goal), TaskType::Research).with_effort(2),
        );

        if goal.to_lowercase().contains("write") || goal.to_lowercase().contains("create") {
            subtasks.push(Subtask::new("Draft content", TaskType::Create).with_effort(5));
            subtasks.push(Subtask::new("Review and refine", TaskType::Review).with_effort(3));
        }

        if goal.to_lowercase().contains("code") || goal.to_lowercase().contains("implement") {
            subtasks.push(Subtask::new("Write implementation", TaskType::Code).with_effort(6));
            subtasks.push(Subtask::new("Write tests", TaskType::Test).with_effort(4));
        }

        if goal.to_lowercase().contains("analyze") {
            subtasks.push(Subtask::new("Gather data", TaskType::Research).with_effort(4));
            subtasks.push(Subtask::new("Analyze results", TaskType::Analyze).with_effort(5));
        }

        subtasks.push(Subtask::new("Finalize and deliver", TaskType::Execute).with_effort(2));

        Ok(subtasks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_goal_creation() {
        let manager = GoalManager::new(SimpleDecomposer, ExecutionConfig::default());

        let goal = manager.create_goal("Write a blog post").await.unwrap();

        assert!(!goal.subtasks.is_empty());
        assert_eq!(goal.status, GoalStatus::Pending);
    }

    #[tokio::test]
    async fn test_goal_progress() {
        let mut goal = Goal::new("Test goal");
        goal.add_subtask(Subtask::new("Task 1", TaskType::Research));
        goal.add_subtask(Subtask::new("Task 2", TaskType::Create));

        assert_eq!(goal.progress(), 0.0);

        goal.subtasks[0].status = SubtaskStatus::Completed;
        assert_eq!(goal.progress(), 0.5);

        goal.subtasks[1].status = SubtaskStatus::Completed;
        assert_eq!(goal.progress(), 1.0);
    }

    #[test]
    fn test_dependency_validation() {
        let manager = GoalManager::new(SimpleDecomposer, ExecutionConfig::default());

        let mut goal = Goal::new("Test");
        let task1 = Subtask::new("Task 1", TaskType::Research);
        let task1_id = task1.id;
        let task2 = Subtask::new("Task 2", TaskType::Create).depends_on(task1_id);

        goal.add_subtask(task1);
        goal.add_subtask(task2);

        assert!(manager.validate_dependencies(&goal).is_ok());
    }

    #[test]
    fn test_next_subtasks() {
        let mut goal = Goal::new("Test");

        let task1 = Subtask::new("Task 1", TaskType::Research);
        let task1_id = task1.id;
        let task2 = Subtask::new("Task 2", TaskType::Create).depends_on(task1_id);

        goal.add_subtask(task1);
        goal.add_subtask(task2);

        // Only task1 can execute initially
        let next = goal.next_subtasks();
        assert_eq!(next.len(), 1);
        assert_eq!(next[0].id, task1_id);

        // After task1 completes, task2 can execute
        goal.subtasks[0].status = SubtaskStatus::Completed;
        let next = goal.next_subtasks();
        assert_eq!(next.len(), 1);
        assert!(next[0].dependencies.contains(&task1_id));
    }
}
