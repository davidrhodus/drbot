//! Smart model routing - routes to cheaper/faster models based on task complexity.
//!
//! This crate provides:
//! - Task complexity analysis
//! - Model capability matching
//! - Cost-aware routing
//! - Performance optimization
//! - Fallback handling

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Router errors.
#[derive(Debug, Error)]
pub enum RouterError {
    #[error("No suitable model found: {0}")]
    NoSuitableModel(String),

    #[error("Model unavailable: {0}")]
    ModelUnavailable(String),

    #[error("Routing failed: {0}")]
    RoutingFailed(String),

    #[error("Budget exceeded: {0}")]
    BudgetExceeded(String),
}

/// Result type for router operations.
pub type Result<T> = std::result::Result<T, RouterError>;

/// A model available for routing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Model {
    /// Model identifier.
    pub id: String,
    /// Model name.
    pub name: String,
    /// Provider.
    pub provider: String,
    /// Capabilities.
    pub capabilities: ModelCapabilities,
    /// Cost per 1K input tokens.
    pub cost_per_1k_input: f64,
    /// Cost per 1K output tokens.
    pub cost_per_1k_output: f64,
    /// Average latency in ms.
    pub avg_latency_ms: u64,
    /// Max context length.
    pub max_context: usize,
    /// Is available.
    pub available: bool,
    /// Priority (higher = preferred).
    pub priority: i32,
}

/// Model capabilities.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModelCapabilities {
    /// Good at coding.
    pub coding: f64,
    /// Good at reasoning.
    pub reasoning: f64,
    /// Good at creative writing.
    pub creative: f64,
    /// Good at analysis.
    pub analysis: f64,
    /// Good at summarization.
    pub summarization: f64,
    /// Good at translation.
    pub translation: f64,
    /// Good at math.
    pub math: f64,
    /// Supports vision.
    pub vision: bool,
    /// Supports function calling.
    pub function_calling: bool,
}

/// Task complexity analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskAnalysis {
    /// Complexity score (0-1).
    pub complexity: f64,
    /// Required capabilities.
    pub required_capabilities: Vec<String>,
    /// Estimated tokens.
    pub estimated_tokens: usize,
    /// Task type.
    pub task_type: TaskType,
    /// Needs vision.
    pub needs_vision: bool,
    /// Needs function calling.
    pub needs_functions: bool,
}

/// Types of tasks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskType {
    SimpleQA,
    Coding,
    Reasoning,
    Creative,
    Analysis,
    Summarization,
    Translation,
    Math,
    Multimodal,
    Complex,
}

/// Routing decision.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingDecision {
    /// Decision identifier.
    pub id: String,
    /// Selected model.
    pub model_id: String,
    /// Model name.
    pub model_name: String,
    /// Reason for selection.
    pub reason: String,
    /// Estimated cost.
    pub estimated_cost: f64,
    /// Estimated latency.
    pub estimated_latency_ms: u64,
    /// Confidence in decision.
    pub confidence: f64,
    /// Fallback models.
    pub fallbacks: Vec<String>,
    /// Timestamp.
    pub decided_at: DateTime<Utc>,
}

/// Routing constraints.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RoutingConstraints {
    /// Maximum cost.
    pub max_cost: Option<f64>,
    /// Maximum latency.
    pub max_latency_ms: Option<u64>,
    /// Required capabilities.
    pub required_capabilities: Vec<String>,
    /// Preferred providers.
    pub preferred_providers: Vec<String>,
    /// Excluded models.
    pub excluded_models: Vec<String>,
    /// Prefer cheaper.
    pub prefer_cheaper: bool,
    /// Prefer faster.
    pub prefer_faster: bool,
}

/// Provider for task analysis.
#[async_trait]
pub trait TaskAnalyzer: Send + Sync {
    /// Analyze task complexity.
    async fn analyze(&self, prompt: &str, context: &str) -> Result<TaskAnalysis>;
}

/// The smart router engine.
pub struct SmartRouter {
    /// Task analyzer.
    analyzer: Arc<dyn TaskAnalyzer>,
    /// Available models.
    models: Arc<RwLock<HashMap<String, Model>>>,
    /// Routing history.
    history: Arc<RwLock<Vec<RoutingDecision>>>,
    /// Default constraints.
    default_constraints: RoutingConstraints,
    /// Usage statistics.
    stats: Arc<RwLock<RouterStats>>,
}

/// Router statistics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RouterStats {
    /// Total requests routed.
    pub total_requests: usize,
    /// Requests by model.
    pub by_model: HashMap<String, usize>,
    /// Total estimated cost.
    pub total_cost: f64,
    /// Cost savings (vs always using expensive model).
    pub cost_savings: f64,
    /// Average latency.
    pub avg_latency_ms: f64,
}

impl SmartRouter {
    /// Create a new smart router.
    pub fn new(analyzer: Arc<dyn TaskAnalyzer>) -> Self {
        Self {
            analyzer,
            models: Arc::new(RwLock::new(HashMap::new())),
            history: Arc::new(RwLock::new(Vec::new())),
            default_constraints: RoutingConstraints::default(),
            stats: Arc::new(RwLock::new(RouterStats::default())),
        }
    }

    /// Set default constraints.
    pub fn with_constraints(mut self, constraints: RoutingConstraints) -> Self {
        self.default_constraints = constraints;
        self
    }

    /// Register a model.
    pub async fn register_model(&self, model: Model) {
        let mut models = self.models.write().await;
        models.insert(model.id.clone(), model);
    }

    /// Route a request.
    pub async fn route(
        &self,
        prompt: &str,
        context: &str,
        constraints: Option<RoutingConstraints>,
    ) -> Result<RoutingDecision> {
        let constraints = constraints.unwrap_or_else(|| self.default_constraints.clone());

        // Analyze task
        let analysis = self.analyzer.analyze(prompt, context).await?;

        // Get available models
        let models = self.models.read().await;
        let available: Vec<_> = models
            .values()
            .filter(|m| m.available)
            .filter(|m| !constraints.excluded_models.contains(&m.id))
            .filter(|m| self.meets_requirements(m, &analysis, &constraints))
            .collect();

        if available.is_empty() {
            return Err(RouterError::NoSuitableModel(
                "No models meet requirements".to_string(),
            ));
        }

        // Score and rank models
        let mut scored: Vec<_> = available
            .iter()
            .map(|m| (m, self.score_model(m, &analysis, &constraints)))
            .collect();
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

        let best = scored[0].0;
        let fallbacks: Vec<String> = scored
            .iter()
            .skip(1)
            .take(2)
            .map(|(m, _)| m.id.clone())
            .collect();

        let estimated_cost = (analysis.estimated_tokens as f64 / 1000.0)
            * (best.cost_per_1k_input + best.cost_per_1k_output);

        // Check budget
        if let Some(max_cost) = constraints.max_cost {
            if estimated_cost > max_cost {
                return Err(RouterError::BudgetExceeded(format!(
                    "Estimated cost ${:.4} exceeds budget ${:.4}",
                    estimated_cost, max_cost
                )));
            }
        }

        let decision = RoutingDecision {
            id: Uuid::new_v4().to_string(),
            model_id: best.id.clone(),
            model_name: best.name.clone(),
            reason: self.generate_reason(best, &analysis),
            estimated_cost,
            estimated_latency_ms: best.avg_latency_ms,
            confidence: scored[0].1,
            fallbacks,
            decided_at: Utc::now(),
        };

        // Update stats
        self.update_stats(&decision, &analysis).await;

        // Record decision
        let mut history = self.history.write().await;
        history.push(decision.clone());

        Ok(decision)
    }

    /// Check if model meets requirements.
    fn meets_requirements(
        &self,
        model: &Model,
        analysis: &TaskAnalysis,
        constraints: &RoutingConstraints,
    ) -> bool {
        // Check latency constraint
        if let Some(max_latency) = constraints.max_latency_ms {
            if model.avg_latency_ms > max_latency {
                return false;
            }
        }

        // Check context length
        if analysis.estimated_tokens > model.max_context {
            return false;
        }

        // Check vision requirement
        if analysis.needs_vision && !model.capabilities.vision {
            return false;
        }

        // Check function calling requirement
        if analysis.needs_functions && !model.capabilities.function_calling {
            return false;
        }

        // Check preferred providers
        if !constraints.preferred_providers.is_empty()
            && !constraints.preferred_providers.contains(&model.provider)
        {
            return false;
        }

        true
    }

    /// Score a model for the task.
    fn score_model(
        &self,
        model: &Model,
        analysis: &TaskAnalysis,
        constraints: &RoutingConstraints,
    ) -> f64 {
        let mut score = 0.0;

        // Capability match
        let capability_score = match analysis.task_type {
            TaskType::Coding => model.capabilities.coding,
            TaskType::Reasoning => model.capabilities.reasoning,
            TaskType::Creative => model.capabilities.creative,
            TaskType::Analysis => model.capabilities.analysis,
            TaskType::Summarization => model.capabilities.summarization,
            TaskType::Translation => model.capabilities.translation,
            TaskType::Math => model.capabilities.math,
            _ => 0.5,
        };

        // Weight capability more heavily for complex tasks
        let capability_weight = if analysis.complexity >= 0.6 {
            0.6 // Complex tasks need capable models
        } else {
            0.3
        };
        score += capability_score * capability_weight;

        // Cost efficiency (inverse)
        let max_cost = 0.1; // $0.10 per 1K tokens as baseline
        let cost = model.cost_per_1k_input + model.cost_per_1k_output;
        let cost_score = 1.0 - (cost / max_cost).min(1.0);
        // Reduce cost weight for complex tasks - quality matters more
        let cost_weight = if constraints.prefer_cheaper {
            0.4
        } else if analysis.complexity >= 0.6 {
            0.1
        } else {
            0.3
        };
        score += cost_score * cost_weight;

        // Latency (inverse)
        let max_latency = 5000.0; // 5 seconds as baseline
        let latency_score = 1.0 - (model.avg_latency_ms as f64 / max_latency).min(1.0);
        let latency_weight = if constraints.prefer_faster { 0.3 } else { 0.1 };
        score += latency_score * latency_weight;

        // Priority bonus
        score += (model.priority as f64 / 100.0) * 0.1;

        // Complexity match - use cheaper models for simple tasks
        if analysis.complexity < 0.3 && cost_score > 0.7 {
            score += 0.1;
        }

        score
    }

    /// Generate reason for model selection.
    fn generate_reason(&self, model: &Model, analysis: &TaskAnalysis) -> String {
        let complexity_desc = if analysis.complexity < 0.3 {
            "simple"
        } else if analysis.complexity < 0.7 {
            "moderate"
        } else {
            "complex"
        };

        format!(
            "{} selected for {} {} task (cost: ${:.4}/1K, latency: {}ms)",
            model.name,
            complexity_desc,
            format!("{:?}", analysis.task_type).to_lowercase(),
            model.cost_per_1k_input + model.cost_per_1k_output,
            model.avg_latency_ms
        )
    }

    /// Update statistics.
    async fn update_stats(&self, decision: &RoutingDecision, analysis: &TaskAnalysis) {
        let mut stats = self.stats.write().await;
        stats.total_requests += 1;
        *stats.by_model.entry(decision.model_id.clone()).or_insert(0) += 1;
        stats.total_cost += decision.estimated_cost;

        // Calculate savings (assuming most expensive model would cost $0.06/1K)
        let expensive_cost = (analysis.estimated_tokens as f64 / 1000.0) * 0.06;
        stats.cost_savings += expensive_cost - decision.estimated_cost;

        // Update average latency
        let n = stats.total_requests as f64;
        stats.avg_latency_ms =
            ((stats.avg_latency_ms * (n - 1.0)) + decision.estimated_latency_ms as f64) / n;
    }

    /// Get statistics.
    pub async fn get_stats(&self) -> RouterStats {
        let stats = self.stats.read().await;
        stats.clone()
    }

    /// Get routing history.
    pub async fn get_history(&self, limit: usize) -> Vec<RoutingDecision> {
        let history = self.history.read().await;
        history.iter().rev().take(limit).cloned().collect()
    }

    /// Update model availability.
    pub async fn set_model_available(&self, model_id: &str, available: bool) {
        let mut models = self.models.write().await;
        if let Some(model) = models.get_mut(model_id) {
            model.available = available;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockAnalyzer;

    #[async_trait]
    impl TaskAnalyzer for MockAnalyzer {
        async fn analyze(&self, prompt: &str, _context: &str) -> Result<TaskAnalysis> {
            let complexity = if prompt.len() > 100 { 0.7 } else { 0.3 };
            let task_type = if prompt.contains("code") {
                TaskType::Coding
            } else if prompt.contains("translate") {
                TaskType::Translation
            } else {
                TaskType::SimpleQA
            };

            Ok(TaskAnalysis {
                complexity,
                required_capabilities: vec![],
                estimated_tokens: prompt.len() * 2,
                task_type,
                needs_vision: false,
                needs_functions: false,
            })
        }
    }

    async fn setup_router() -> SmartRouter {
        let analyzer = Arc::new(MockAnalyzer);
        let router = SmartRouter::new(analyzer);

        // Register models
        router
            .register_model(Model {
                id: "gpt-4".to_string(),
                name: "GPT-4".to_string(),
                provider: "openai".to_string(),
                capabilities: ModelCapabilities {
                    coding: 0.95,
                    reasoning: 0.95,
                    creative: 0.9,
                    analysis: 0.95,
                    summarization: 0.9,
                    translation: 0.85,
                    math: 0.9,
                    vision: true,
                    function_calling: true,
                },
                cost_per_1k_input: 0.03,
                cost_per_1k_output: 0.06,
                avg_latency_ms: 2000,
                max_context: 128000,
                available: true,
                priority: 90,
            })
            .await;

        router
            .register_model(Model {
                id: "gpt-3.5".to_string(),
                name: "GPT-3.5 Turbo".to_string(),
                provider: "openai".to_string(),
                capabilities: ModelCapabilities {
                    coding: 0.7,
                    reasoning: 0.7,
                    creative: 0.75,
                    analysis: 0.7,
                    summarization: 0.8,
                    translation: 0.8,
                    math: 0.6,
                    vision: false,
                    function_calling: true,
                },
                cost_per_1k_input: 0.0005,
                cost_per_1k_output: 0.0015,
                avg_latency_ms: 500,
                max_context: 16000,
                available: true,
                priority: 50,
            })
            .await;

        router
    }

    #[tokio::test]
    async fn test_route_simple() {
        let router = setup_router().await;

        let decision = router.route("Hello, how are you?", "", None).await.unwrap();

        // Should route to cheaper model for simple task
        assert_eq!(decision.model_id, "gpt-3.5");
    }

    #[tokio::test]
    async fn test_route_complex() {
        let router = setup_router().await;

        let complex_prompt = "Write a complex algorithm to solve the traveling salesman problem with code optimization and detailed analysis.";
        let decision = router.route(complex_prompt, "", None).await.unwrap();

        // Should route to capable model for complex coding task
        assert_eq!(decision.model_id, "gpt-4");
    }

    #[tokio::test]
    async fn test_route_with_constraints() {
        let router = setup_router().await;

        let constraints = RoutingConstraints {
            prefer_cheaper: true,
            max_latency_ms: Some(1000),
            ..Default::default()
        };

        let decision = router
            .route("Simple question", "", Some(constraints))
            .await
            .unwrap();
        assert_eq!(decision.model_id, "gpt-3.5");
    }

    #[tokio::test]
    async fn test_stats() {
        let router = setup_router().await;

        router.route("Test 1", "", None).await.unwrap();
        router.route("Test 2", "", None).await.unwrap();

        let stats = router.get_stats().await;
        assert_eq!(stats.total_requests, 2);
    }

    #[tokio::test]
    async fn test_model_availability() {
        let router = setup_router().await;

        router.set_model_available("gpt-3.5", false).await;

        let decision = router.route("Simple question", "", None).await.unwrap();
        assert_eq!(decision.model_id, "gpt-4"); // Falls back to available model
    }
}
