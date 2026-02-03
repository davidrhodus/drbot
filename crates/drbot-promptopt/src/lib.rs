//! Prompt optimization for drbot.
//!
//! Automatically improve prompts for better results.
//!
//! # Features
//!
//! - Prompt analysis and scoring
//! - Automatic optimization suggestions
//! - A/B testing integration
//! - Performance tracking

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Prompt optimization result type.
pub type Result<T> = std::result::Result<T, OptimizationError>;

/// Optimization errors.
#[derive(Debug, thiserror::Error)]
pub enum OptimizationError {
    #[error("Analysis failed: {0}")]
    AnalysisFailed(String),
    #[error("Optimization failed: {0}")]
    OptimizationFailed(String),
    #[error("Prompt not found: {0}")]
    PromptNotFound(Uuid),
    #[error("No optimization available")]
    NoOptimization,
}

/// Analyzed prompt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyzedPrompt {
    /// Analysis ID.
    pub id: Uuid,
    /// Original prompt.
    pub prompt: String,
    /// Overall score (0-1).
    pub score: f32,
    /// Dimension scores.
    pub dimensions: PromptDimensions,
    /// Issues found.
    pub issues: Vec<PromptIssue>,
    /// Optimization suggestions.
    pub suggestions: Vec<OptimizationSuggestion>,
    /// Analyzed at.
    pub analyzed_at: DateTime<Utc>,
}

/// Prompt dimension scores.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PromptDimensions {
    /// Clarity score.
    pub clarity: f32,
    /// Specificity score.
    pub specificity: f32,
    /// Structure score.
    pub structure: f32,
    /// Completeness score.
    pub completeness: f32,
    /// Conciseness score.
    pub conciseness: f32,
    /// Effectiveness score.
    pub effectiveness: f32,
}

impl PromptDimensions {
    /// Calculate overall score.
    pub fn overall(&self) -> f32 {
        let scores = [
            self.clarity,
            self.specificity,
            self.structure,
            self.completeness,
            self.conciseness,
            self.effectiveness,
        ];

        scores.iter().sum::<f32>() / scores.len() as f32
    }
}

/// Prompt issue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptIssue {
    /// Issue type.
    pub issue_type: IssueType,
    /// Severity (0-1).
    pub severity: f32,
    /// Description.
    pub description: String,
    /// Location in prompt.
    pub location: Option<(usize, usize)>,
    /// Suggested fix.
    pub fix: Option<String>,
}

/// Issue types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueType {
    Vague,
    TooLong,
    TooShort,
    Ambiguous,
    MissingContext,
    Redundant,
    PoorStructure,
    UnclearGoal,
    Inconsistent,
    JargonHeavy,
}

/// Optimization suggestion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationSuggestion {
    /// Suggestion type.
    pub suggestion_type: SuggestionType,
    /// Original text.
    pub original: String,
    /// Suggested replacement.
    pub replacement: String,
    /// Explanation.
    pub explanation: String,
    /// Expected improvement.
    pub expected_improvement: f32,
    /// Confidence.
    pub confidence: f32,
}

/// Suggestion types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SuggestionType {
    Rephrase,
    AddContext,
    RemoveRedundancy,
    Restructure,
    Clarify,
    Simplify,
    Expand,
    Split,
    Merge,
}

/// Optimized prompt result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizedPrompt {
    /// Optimization ID.
    pub id: Uuid,
    /// Original prompt.
    pub original: String,
    /// Optimized prompt.
    pub optimized: String,
    /// Applied suggestions.
    pub applied_suggestions: Vec<OptimizationSuggestion>,
    /// Original score.
    pub original_score: f32,
    /// New score.
    pub new_score: f32,
    /// Improvement.
    pub improvement: f32,
    /// Optimized at.
    pub optimized_at: DateTime<Utc>,
}

/// Optimization configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationConfig {
    /// Enable automatic optimization.
    pub auto_optimize: bool,
    /// Minimum score threshold.
    pub min_score: f32,
    /// Maximum prompt length.
    pub max_length: usize,
    /// Optimize aggressively.
    pub aggressive: bool,
    /// Track performance.
    pub track_performance: bool,
}

impl Default for OptimizationConfig {
    fn default() -> Self {
        Self {
            auto_optimize: true,
            min_score: 0.7,
            max_length: 4000,
            aggressive: false,
            track_performance: true,
        }
    }
}

/// Trait for prompt analyzers.
#[async_trait]
pub trait PromptAnalyzer: Send + Sync {
    /// Analyze a prompt.
    async fn analyze(&self, prompt: &str) -> Result<AnalyzedPrompt>;
}

/// Trait for prompt optimizers.
#[async_trait]
pub trait PromptOptimizer: Send + Sync {
    /// Optimize a prompt based on analysis.
    async fn optimize(&self, prompt: &str, analysis: &AnalyzedPrompt) -> Result<OptimizedPrompt>;
}

/// Prompt performance record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptPerformance {
    /// Record ID.
    pub id: Uuid,
    /// Prompt hash.
    pub prompt_hash: u64,
    /// Prompt text.
    pub prompt: String,
    /// Usage count.
    pub usage_count: u64,
    /// Success rate.
    pub success_rate: f32,
    /// Average response quality.
    pub avg_quality: f32,
    /// Average latency in ms.
    pub avg_latency_ms: u64,
    /// First used.
    pub first_used: DateTime<Utc>,
    /// Last used.
    pub last_used: DateTime<Utc>,
}

/// Prompt optimization engine.
pub struct PromptOptEngine<A: PromptAnalyzer, O: PromptOptimizer> {
    config: OptimizationConfig,
    analyzer: A,
    optimizer: O,
    analyses: Arc<RwLock<HashMap<Uuid, AnalyzedPrompt>>>,
    optimizations: Arc<RwLock<HashMap<Uuid, OptimizedPrompt>>>,
    performance: Arc<RwLock<HashMap<u64, PromptPerformance>>>,
}

impl<A: PromptAnalyzer, O: PromptOptimizer> PromptOptEngine<A, O> {
    /// Create a new prompt optimization engine.
    pub fn new(config: OptimizationConfig, analyzer: A, optimizer: O) -> Self {
        Self {
            config,
            analyzer,
            optimizer,
            analyses: Arc::new(RwLock::new(HashMap::new())),
            optimizations: Arc::new(RwLock::new(HashMap::new())),
            performance: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Analyze a prompt.
    pub async fn analyze(&self, prompt: &str) -> Result<AnalyzedPrompt> {
        let analysis = self.analyzer.analyze(prompt).await?;
        self.analyses
            .write()
            .await
            .insert(analysis.id, analysis.clone());
        Ok(analysis)
    }

    /// Optimize a prompt.
    pub async fn optimize(&self, prompt: &str) -> Result<OptimizedPrompt> {
        let analysis = self.analyze(prompt).await?;

        if analysis.score >= self.config.min_score && !self.config.aggressive {
            return Err(OptimizationError::NoOptimization);
        }

        let optimization = self.optimizer.optimize(prompt, &analysis).await?;
        self.optimizations
            .write()
            .await
            .insert(optimization.id, optimization.clone());

        Ok(optimization)
    }

    /// Get or optimize a prompt.
    pub async fn get_or_optimize(&self, prompt: &str) -> String {
        match self.optimize(prompt).await {
            Ok(opt) => opt.optimized,
            Err(_) => prompt.to_string(),
        }
    }

    /// Record prompt performance.
    pub async fn record_performance(
        &self,
        prompt: &str,
        success: bool,
        quality: f32,
        latency_ms: u64,
    ) {
        if !self.config.track_performance {
            return;
        }

        let hash = Self::hash_prompt(prompt);
        let mut performance = self.performance.write().await;

        let record = performance
            .entry(hash)
            .or_insert_with(|| PromptPerformance {
                id: Uuid::new_v4(),
                prompt_hash: hash,
                prompt: prompt.to_string(),
                usage_count: 0,
                success_rate: 0.0,
                avg_quality: 0.0,
                avg_latency_ms: 0,
                first_used: Utc::now(),
                last_used: Utc::now(),
            });

        record.usage_count += 1;
        record.last_used = Utc::now();

        // Update running averages
        let n = record.usage_count as f32;
        record.success_rate =
            (record.success_rate * (n - 1.0) + if success { 1.0 } else { 0.0 }) / n;
        record.avg_quality = (record.avg_quality * (n - 1.0) + quality) / n;
        record.avg_latency_ms =
            ((record.avg_latency_ms as f32 * (n - 1.0) + latency_ms as f32) / n) as u64;
    }

    fn hash_prompt(prompt: &str) -> u64 {
        prompt
            .bytes()
            .fold(0u64, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u64))
    }

    /// Get best performing prompts.
    pub async fn best_prompts(&self, limit: usize) -> Vec<PromptPerformance> {
        let performance = self.performance.read().await;
        let mut sorted: Vec<_> = performance.values().cloned().collect();
        sorted.sort_by(|a, b| {
            let score_a = a.success_rate * 0.5 + a.avg_quality * 0.5;
            let score_b = b.success_rate * 0.5 + b.avg_quality * 0.5;
            score_b.partial_cmp(&score_a).unwrap()
        });
        sorted.truncate(limit);
        sorted
    }

    /// Get optimization statistics.
    pub async fn stats(&self) -> OptimizationStats {
        let analyses = self.analyses.read().await;
        let optimizations = self.optimizations.read().await;
        let performance = self.performance.read().await;

        let avg_improvement = if !optimizations.is_empty() {
            optimizations.values().map(|o| o.improvement).sum::<f32>() / optimizations.len() as f32
        } else {
            0.0
        };

        let avg_quality = if !performance.is_empty() {
            performance.values().map(|p| p.avg_quality).sum::<f32>() / performance.len() as f32
        } else {
            0.0
        };

        OptimizationStats {
            total_analyses: analyses.len(),
            total_optimizations: optimizations.len(),
            total_prompts_tracked: performance.len(),
            avg_improvement,
            avg_quality,
        }
    }
}

/// Optimization statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationStats {
    pub total_analyses: usize,
    pub total_optimizations: usize,
    pub total_prompts_tracked: usize,
    pub avg_improvement: f32,
    pub avg_quality: f32,
}

/// Simple prompt analyzer for testing.
pub struct SimpleAnalyzer;

#[async_trait]
impl PromptAnalyzer for SimpleAnalyzer {
    async fn analyze(&self, prompt: &str) -> Result<AnalyzedPrompt> {
        let mut issues = Vec::new();
        let mut suggestions = Vec::new();

        // Check length
        let word_count = prompt.split_whitespace().count();
        let conciseness = if word_count > 100 {
            issues.push(PromptIssue {
                issue_type: IssueType::TooLong,
                severity: 0.6,
                description: "Prompt may be too long".to_string(),
                location: None,
                fix: Some("Consider breaking into smaller parts".to_string()),
            });
            0.5
        } else if word_count < 5 {
            issues.push(PromptIssue {
                issue_type: IssueType::TooShort,
                severity: 0.5,
                description: "Prompt may be too short".to_string(),
                location: None,
                fix: Some("Add more context".to_string()),
            });
            0.6
        } else {
            0.85
        };

        // Check for vagueness
        let vague_words = ["something", "stuff", "thing", "whatever", "somehow"];
        let vague_count = vague_words
            .iter()
            .filter(|w| prompt.to_lowercase().contains(*w))
            .count();
        let clarity = if vague_count > 0 {
            issues.push(PromptIssue {
                issue_type: IssueType::Vague,
                severity: 0.4,
                description: format!("Prompt contains {} vague terms", vague_count),
                location: None,
                fix: Some("Use more specific language".to_string()),
            });
            0.6
        } else {
            0.8
        };

        // Check structure
        let has_question = prompt.contains('?');
        let has_instruction =
            prompt.to_lowercase().contains("please") || prompt.to_lowercase().contains("should");
        let structure = if has_question || has_instruction {
            0.85
        } else {
            0.65
        };

        // Generate suggestions
        if vague_count > 0 {
            suggestions.push(OptimizationSuggestion {
                suggestion_type: SuggestionType::Clarify,
                original: prompt.to_string(),
                replacement: prompt.replace("something", "[specific item]"),
                explanation: "Replace vague terms with specific ones".to_string(),
                expected_improvement: 0.15,
                confidence: 0.7,
            });
        }

        let dimensions = PromptDimensions {
            clarity,
            specificity: if vague_count == 0 { 0.8 } else { 0.5 },
            structure,
            completeness: if word_count > 10 { 0.8 } else { 0.5 },
            conciseness,
            effectiveness: 0.7,
        };

        Ok(AnalyzedPrompt {
            id: Uuid::new_v4(),
            prompt: prompt.to_string(),
            score: dimensions.overall(),
            dimensions,
            issues,
            suggestions,
            analyzed_at: Utc::now(),
        })
    }
}

/// Simple prompt optimizer for testing.
pub struct SimpleOptimizer;

#[async_trait]
impl PromptOptimizer for SimpleOptimizer {
    async fn optimize(&self, prompt: &str, analysis: &AnalyzedPrompt) -> Result<OptimizedPrompt> {
        let mut optimized = prompt.to_string();
        let mut applied = Vec::new();

        for suggestion in &analysis.suggestions {
            optimized = optimized.replace(&suggestion.original, &suggestion.replacement);
            applied.push(suggestion.clone());
        }

        // If still not great, add structure
        if !optimized.ends_with('?') && !optimized.ends_with('.') {
            optimized.push('.');
        }

        let new_score = (analysis.score + 0.1).min(1.0);

        Ok(OptimizedPrompt {
            id: Uuid::new_v4(),
            original: prompt.to_string(),
            optimized,
            applied_suggestions: applied,
            original_score: analysis.score,
            new_score,
            improvement: new_score - analysis.score,
            optimized_at: Utc::now(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_prompt_analysis() {
        let engine = PromptOptEngine::new(
            OptimizationConfig::default(),
            SimpleAnalyzer,
            SimpleOptimizer,
        );

        let analysis = engine.analyze("Write something about stuff").await.unwrap();

        assert!(!analysis.issues.is_empty());
        assert!(analysis.score < 1.0);
    }

    #[tokio::test]
    async fn test_prompt_optimization() {
        let config = OptimizationConfig {
            min_score: 0.9,
            ..Default::default()
        };
        let engine = PromptOptEngine::new(config, SimpleAnalyzer, SimpleOptimizer);

        let result = engine
            .optimize("Write something about things")
            .await
            .unwrap();

        assert!(result.improvement > 0.0);
        assert!(result.new_score > result.original_score);
    }

    #[tokio::test]
    async fn test_performance_tracking() {
        let engine = PromptOptEngine::new(
            OptimizationConfig::default(),
            SimpleAnalyzer,
            SimpleOptimizer,
        );

        engine
            .record_performance("Test prompt", true, 0.9, 100)
            .await;
        engine
            .record_performance("Test prompt", true, 0.8, 150)
            .await;

        let best = engine.best_prompts(10).await;
        assert!(!best.is_empty());
        assert_eq!(best[0].usage_count, 2);
    }

    #[test]
    fn test_dimensions() {
        let dims = PromptDimensions {
            clarity: 0.8,
            specificity: 0.7,
            structure: 0.9,
            completeness: 0.8,
            conciseness: 0.75,
            effectiveness: 0.85,
        };

        let overall = dims.overall();
        assert!(overall > 0.7 && overall < 0.9);
    }
}
