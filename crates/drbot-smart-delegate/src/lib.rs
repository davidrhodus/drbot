//! Confidence-based autonomy with intelligent escalation.
//!
//! This crate provides smart delegation capabilities:
//! - Automatically handle tasks within confidence threshold
//! - Escalate uncertain decisions to human
//! - Learn appropriate autonomy levels
//! - Track delegation patterns

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Delegation errors.
#[derive(Debug, Error)]
pub enum DelegateError {
    #[error("Task handling failed: {0}")]
    HandlingFailed(String),

    #[error("Escalation required: {0}")]
    EscalationRequired(String),

    #[error("Policy violation: {0}")]
    PolicyViolation(String),

    #[error("Provider error: {0}")]
    ProviderError(String),
}

/// Result type for delegation operations.
pub type Result<T> = std::result::Result<T, DelegateError>;

/// A task that can be delegated.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    /// Task identifier.
    pub id: String,
    /// Task description.
    pub description: String,
    /// Task category.
    pub category: TaskCategory,
    /// Parameters.
    pub parameters: HashMap<String, serde_json::Value>,
    /// Urgency level.
    pub urgency: Urgency,
    /// Context.
    pub context: TaskContext,
    /// Created timestamp.
    pub created_at: DateTime<Utc>,
}

/// Task categories.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum TaskCategory {
    /// Information retrieval.
    Information,
    /// Content creation.
    Creation,
    /// Decision making.
    Decision,
    /// Communication.
    Communication,
    /// Transaction/action.
    Transaction,
    /// Analysis.
    Analysis,
    /// Administrative.
    Administrative,
    /// Custom category.
    Custom(String),
}

/// Urgency levels.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum Urgency {
    Low,
    Medium,
    High,
    Critical,
}

/// Context for a task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskContext {
    /// User who requested.
    pub user_id: String,
    /// Session/conversation.
    pub session_id: Option<String>,
    /// Previous related tasks.
    pub related_tasks: Vec<String>,
    /// Additional context.
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Assessment of whether to handle or escalate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelegationAssessment {
    /// Task being assessed.
    pub task_id: String,
    /// Can handle autonomously.
    pub can_handle: bool,
    /// Confidence in handling (0.0-1.0).
    pub confidence: f64,
    /// Reasons for assessment.
    pub reasons: Vec<AssessmentReason>,
    /// Recommended action.
    pub recommendation: DelegationAction,
    /// Risk assessment.
    pub risk: RiskLevel,
}

/// Reasons for delegation assessment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssessmentReason {
    /// Factor name.
    pub factor: String,
    /// Impact on confidence.
    pub impact: f64,
    /// Description.
    pub description: String,
}

/// Recommended delegation action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DelegationAction {
    /// Handle autonomously.
    Handle { approach: String },
    /// Escalate to human.
    Escalate {
        reason: String,
        suggested_action: String,
    },
    /// Handle with confirmation.
    ConfirmThenHandle { question: String },
    /// Partial handling.
    PartialHandle {
        autonomous_parts: Vec<String>,
        escalate_parts: Vec<String>,
    },
}

/// Risk levels.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum RiskLevel {
    None,
    Low,
    Medium,
    High,
    Critical,
}

/// Delegation policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelegationPolicy {
    /// Policy identifier.
    pub id: String,
    /// Policy name.
    pub name: String,
    /// Minimum confidence to handle.
    pub min_confidence: f64,
    /// Maximum risk to handle.
    pub max_risk: RiskLevel,
    /// Categories allowed for autonomous handling.
    pub allowed_categories: Vec<TaskCategory>,
    /// Always escalate patterns.
    pub always_escalate: Vec<String>,
    /// Never escalate patterns.
    pub never_escalate: Vec<String>,
}

impl Default for DelegationPolicy {
    fn default() -> Self {
        Self {
            id: "default".to_string(),
            name: "Default Policy".to_string(),
            min_confidence: 0.8,
            max_risk: RiskLevel::Low,
            allowed_categories: vec![TaskCategory::Information, TaskCategory::Analysis],
            always_escalate: vec![
                "financial".to_string(),
                "legal".to_string(),
                "medical".to_string(),
            ],
            never_escalate: vec![],
        }
    }
}

/// Result of handling a task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResult {
    /// Task ID.
    pub task_id: String,
    /// How it was handled.
    pub handling: HandlingOutcome,
    /// Result content.
    pub result: serde_json::Value,
    /// Timestamp.
    pub completed_at: DateTime<Utc>,
}

/// How a task was handled.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HandlingOutcome {
    /// Handled autonomously.
    Autonomous { confidence: f64 },
    /// Handled after confirmation.
    Confirmed { question: String, response: String },
    /// Escalated to human.
    Escalated { reason: String },
    /// Partially handled.
    Partial {
        autonomous: serde_json::Value,
        escalated: String,
    },
}

/// Escalation request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Escalation {
    /// Escalation ID.
    pub id: String,
    /// Task being escalated.
    pub task_id: String,
    /// Reason for escalation.
    pub reason: String,
    /// Context for human.
    pub context: String,
    /// Suggested actions.
    pub suggestions: Vec<String>,
    /// Urgency.
    pub urgency: Urgency,
    /// Status.
    pub status: EscalationStatus,
    /// Created timestamp.
    pub created_at: DateTime<Utc>,
}

/// Escalation status.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum EscalationStatus {
    Pending,
    Acknowledged,
    InProgress,
    Resolved,
    Dismissed,
}

/// Provider for delegation intelligence.
#[async_trait]
pub trait DelegateProvider: Send + Sync {
    /// Assess whether to handle or escalate.
    async fn assess(&self, task: &Task, policy: &DelegationPolicy) -> Result<DelegationAssessment>;

    /// Handle a task autonomously.
    async fn handle(&self, task: &Task) -> Result<serde_json::Value>;

    /// Learn from escalation resolution.
    async fn learn(&self, escalation: &Escalation, resolution: &str) -> Result<()>;
}

/// The smart delegation coordinator.
pub struct SmartDelegate {
    /// Provider for delegation.
    provider: Arc<dyn DelegateProvider>,
    /// Active policy.
    policy: Arc<RwLock<DelegationPolicy>>,
    /// Pending escalations.
    escalations: Arc<RwLock<HashMap<String, Escalation>>>,
    /// Task history.
    history: Arc<RwLock<Vec<TaskResult>>>,
    /// Learned patterns.
    patterns: Arc<RwLock<Vec<LearnedPattern>>>,
}

/// A learned delegation pattern.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearnedPattern {
    /// Pattern identifier.
    pub id: String,
    /// Category.
    pub category: TaskCategory,
    /// Keywords/triggers.
    pub triggers: Vec<String>,
    /// Should escalate.
    pub should_escalate: bool,
    /// Confidence adjustment.
    pub confidence_adjustment: f64,
    /// Times observed.
    pub observations: u32,
}

impl SmartDelegate {
    /// Create a new smart delegate.
    pub fn new(provider: Arc<dyn DelegateProvider>) -> Self {
        Self {
            provider,
            policy: Arc::new(RwLock::new(DelegationPolicy::default())),
            escalations: Arc::new(RwLock::new(HashMap::new())),
            history: Arc::new(RwLock::new(Vec::new())),
            patterns: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Set the delegation policy.
    pub async fn set_policy(&self, policy: DelegationPolicy) {
        let mut p = self.policy.write().await;
        *p = policy;
    }

    /// Process a task.
    pub async fn process(&self, task: Task) -> Result<TaskResult> {
        let policy = self.policy.read().await.clone();

        // Assess the task
        let assessment = self.provider.assess(&task, &policy).await?;

        // Apply learned patterns
        let assessment = self.apply_patterns(&task, assessment).await;

        // Check policy
        if !self.check_policy(&task, &assessment, &policy) {
            return self.escalate(task, "Policy requires escalation").await;
        }

        match assessment.recommendation {
            DelegationAction::Handle { .. } => {
                self.handle_autonomously(task, assessment.confidence).await
            }
            DelegationAction::Escalate { reason, .. } => self.escalate(task, &reason).await,
            DelegationAction::ConfirmThenHandle { question } => {
                // In real implementation, would wait for confirmation
                // For now, escalate
                self.escalate(task, &format!("Needs confirmation: {}", question))
                    .await
            }
            DelegationAction::PartialHandle {
                autonomous_parts,
                escalate_parts,
            } => {
                self.handle_partial(task, &autonomous_parts, &escalate_parts)
                    .await
            }
        }
    }

    /// Apply learned patterns to assessment.
    async fn apply_patterns(
        &self,
        task: &Task,
        mut assessment: DelegationAssessment,
    ) -> DelegationAssessment {
        let patterns = self.patterns.read().await;

        for pattern in patterns.iter() {
            if pattern.category == task.category {
                for trigger in &pattern.triggers {
                    if task
                        .description
                        .to_lowercase()
                        .contains(&trigger.to_lowercase())
                    {
                        assessment.confidence += pattern.confidence_adjustment;
                        if pattern.should_escalate {
                            assessment.can_handle = false;
                        }
                        break;
                    }
                }
            }
        }

        assessment.confidence = assessment.confidence.clamp(0.0, 1.0);
        assessment
    }

    /// Check if handling is allowed by policy.
    fn check_policy(
        &self,
        task: &Task,
        assessment: &DelegationAssessment,
        policy: &DelegationPolicy,
    ) -> bool {
        // Check confidence threshold
        if assessment.confidence < policy.min_confidence {
            return false;
        }

        // Check risk level
        if assessment.risk > policy.max_risk {
            return false;
        }

        // Check category
        if !policy.allowed_categories.contains(&task.category) {
            return false;
        }

        // Check always escalate patterns
        for pattern in &policy.always_escalate {
            if task
                .description
                .to_lowercase()
                .contains(&pattern.to_lowercase())
            {
                return false;
            }
        }

        true
    }

    /// Handle a task autonomously.
    async fn handle_autonomously(&self, task: Task, confidence: f64) -> Result<TaskResult> {
        let result = self.provider.handle(&task).await?;

        let task_result = TaskResult {
            task_id: task.id.clone(),
            handling: HandlingOutcome::Autonomous { confidence },
            result,
            completed_at: Utc::now(),
        };

        let mut history = self.history.write().await;
        history.push(task_result.clone());

        Ok(task_result)
    }

    /// Escalate a task.
    async fn escalate(&self, task: Task, reason: &str) -> Result<TaskResult> {
        let escalation = Escalation {
            id: Uuid::new_v4().to_string(),
            task_id: task.id.clone(),
            reason: reason.to_string(),
            context: task.description.clone(),
            suggestions: vec![],
            urgency: task.urgency,
            status: EscalationStatus::Pending,
            created_at: Utc::now(),
        };

        let mut escalations = self.escalations.write().await;
        escalations.insert(escalation.id.clone(), escalation.clone());

        let task_result = TaskResult {
            task_id: task.id,
            handling: HandlingOutcome::Escalated {
                reason: reason.to_string(),
            },
            result: serde_json::json!({ "escalation_id": escalation.id }),
            completed_at: Utc::now(),
        };

        drop(escalations);
        let mut history = self.history.write().await;
        history.push(task_result.clone());

        Ok(task_result)
    }

    /// Handle task partially.
    async fn handle_partial(
        &self,
        task: Task,
        autonomous: &[String],
        escalated: &[String],
    ) -> Result<TaskResult> {
        let result = self.provider.handle(&task).await?;

        let task_result = TaskResult {
            task_id: task.id,
            handling: HandlingOutcome::Partial {
                autonomous: result.clone(),
                escalated: escalated.join(", "),
            },
            result,
            completed_at: Utc::now(),
        };

        let mut history = self.history.write().await;
        history.push(task_result.clone());

        Ok(task_result)
    }

    /// Resolve an escalation.
    pub async fn resolve_escalation(&self, escalation_id: &str, resolution: &str) -> Result<()> {
        let mut escalations = self.escalations.write().await;

        if let Some(escalation) = escalations.get_mut(escalation_id) {
            escalation.status = EscalationStatus::Resolved;

            // Learn from resolution
            self.provider.learn(escalation, resolution).await?;
        }

        Ok(())
    }

    /// Get pending escalations.
    pub async fn get_pending_escalations(&self) -> Vec<Escalation> {
        let escalations = self.escalations.read().await;
        escalations
            .values()
            .filter(|e| e.status == EscalationStatus::Pending)
            .cloned()
            .collect()
    }

    /// Get handling statistics.
    pub async fn get_stats(&self) -> DelegationStats {
        let history = self.history.read().await;
        let escalations = self.escalations.read().await;

        let autonomous = history
            .iter()
            .filter(|r| matches!(r.handling, HandlingOutcome::Autonomous { .. }))
            .count();

        let escalated = history
            .iter()
            .filter(|r| matches!(r.handling, HandlingOutcome::Escalated { .. }))
            .count();

        DelegationStats {
            total_tasks: history.len(),
            autonomous_handled: autonomous,
            escalated: escalated,
            pending_escalations: escalations
                .values()
                .filter(|e| e.status == EscalationStatus::Pending)
                .count(),
            autonomy_rate: if history.is_empty() {
                0.0
            } else {
                autonomous as f64 / history.len() as f64
            },
        }
    }
}

/// Delegation statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelegationStats {
    /// Total tasks processed.
    pub total_tasks: usize,
    /// Tasks handled autonomously.
    pub autonomous_handled: usize,
    /// Tasks escalated.
    pub escalated: usize,
    /// Pending escalations.
    pub pending_escalations: usize,
    /// Autonomy rate.
    pub autonomy_rate: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockProvider;

    #[async_trait]
    impl DelegateProvider for MockProvider {
        async fn assess(
            &self,
            task: &Task,
            _policy: &DelegationPolicy,
        ) -> Result<DelegationAssessment> {
            let confidence = if task.description.contains("simple") {
                0.9
            } else {
                0.5
            };

            Ok(DelegationAssessment {
                task_id: task.id.clone(),
                can_handle: confidence >= 0.8,
                confidence,
                reasons: vec![],
                recommendation: if confidence >= 0.8 {
                    DelegationAction::Handle {
                        approach: "direct".to_string(),
                    }
                } else {
                    DelegationAction::Escalate {
                        reason: "Low confidence".to_string(),
                        suggested_action: "Review manually".to_string(),
                    }
                },
                risk: RiskLevel::Low,
            })
        }

        async fn handle(&self, task: &Task) -> Result<serde_json::Value> {
            Ok(serde_json::json!({ "handled": task.id }))
        }

        async fn learn(&self, _escalation: &Escalation, _resolution: &str) -> Result<()> {
            Ok(())
        }
    }

    fn create_task(description: &str) -> Task {
        Task {
            id: Uuid::new_v4().to_string(),
            description: description.to_string(),
            category: TaskCategory::Information,
            parameters: HashMap::new(),
            urgency: Urgency::Medium,
            context: TaskContext {
                user_id: "user1".to_string(),
                session_id: None,
                related_tasks: vec![],
                metadata: HashMap::new(),
            },
            created_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn test_autonomous_handling() {
        let provider = Arc::new(MockProvider);
        let delegate = SmartDelegate::new(provider);

        let task = create_task("simple question");
        let result = delegate.process(task).await.unwrap();

        assert!(matches!(
            result.handling,
            HandlingOutcome::Autonomous { .. }
        ));
    }

    #[tokio::test]
    async fn test_escalation() {
        let provider = Arc::new(MockProvider);
        let delegate = SmartDelegate::new(provider);

        let task = create_task("complex decision");
        let result = delegate.process(task).await.unwrap();

        assert!(matches!(result.handling, HandlingOutcome::Escalated { .. }));
    }

    #[tokio::test]
    async fn test_policy_enforcement() {
        let provider = Arc::new(MockProvider);
        let delegate = SmartDelegate::new(provider);

        // Set strict policy
        delegate
            .set_policy(DelegationPolicy {
                min_confidence: 0.95,
                ..Default::default()
            })
            .await;

        let task = create_task("simple question");
        let result = delegate.process(task).await.unwrap();

        // Even simple task should be escalated due to strict policy
        assert!(matches!(result.handling, HandlingOutcome::Escalated { .. }));
    }

    #[tokio::test]
    async fn test_stats() {
        let provider = Arc::new(MockProvider);
        let delegate = SmartDelegate::new(provider);

        delegate
            .process(create_task("simple task 1"))
            .await
            .unwrap();
        delegate.process(create_task("complex task")).await.unwrap();

        let stats = delegate.get_stats().await;
        assert_eq!(stats.total_tasks, 2);
    }

    #[test]
    fn test_risk_ordering() {
        assert!(RiskLevel::Critical > RiskLevel::High);
        assert!(RiskLevel::High > RiskLevel::Medium);
    }
}
