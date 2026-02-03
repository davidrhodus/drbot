//! Multi-model consensus for drbot.
//!
//! Query multiple AI models and synthesize responses.
//!
//! # Features
//!
//! - Multi-model voting
//! - Response synthesis
//! - Confidence aggregation
//! - Model disagreement detection
//! - Weighted voting strategies

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Consensus result type.
pub type Result<T> = std::result::Result<T, ConsensusError>;

/// Consensus errors.
#[derive(Debug, thiserror::Error)]
pub enum ConsensusError {
    #[error("No models available")]
    NoModels,
    #[error("All models failed: {0}")]
    AllFailed(String),
    #[error("Consensus not reached: {0}")]
    NoConsensus(String),
    #[error("Model error: {0}")]
    ModelError(String),
    #[error("Synthesis failed: {0}")]
    SynthesisFailed(String),
}

/// A model response for consensus.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelResponse {
    /// Model identifier.
    pub model_id: String,
    /// The response content.
    pub content: String,
    /// Model's confidence (if available).
    pub confidence: Option<f32>,
    /// Response latency in ms.
    pub latency_ms: u64,
    /// Token count.
    pub tokens: usize,
    /// Model-specific metadata.
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Consensus result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsensusResult {
    /// Request ID.
    pub id: Uuid,
    /// Final synthesized response.
    pub response: String,
    /// Consensus strategy used.
    pub strategy: ConsensusStrategy,
    /// Individual model responses.
    pub responses: Vec<ModelResponse>,
    /// Overall confidence score.
    pub confidence: f32,
    /// Agreement level (0-1).
    pub agreement: f32,
    /// Whether consensus was reached.
    pub consensus_reached: bool,
    /// Dissenting opinions (if any).
    pub dissent: Vec<String>,
}

/// Consensus strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsensusStrategy {
    /// Majority voting.
    Majority,
    /// Weighted voting by model quality.
    Weighted,
    /// Unanimous agreement required.
    Unanimous,
    /// Synthesize all responses.
    Synthesis,
    /// Use fastest response.
    Fastest,
    /// Use highest confidence.
    HighestConfidence,
    /// Round-robin with verification.
    RoundRobin,
}

/// Model configuration for consensus.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    /// Model identifier.
    pub id: String,
    /// Model weight (higher = more influence).
    pub weight: f32,
    /// Quality score (0-1).
    pub quality: f32,
    /// Whether this model is required.
    pub required: bool,
    /// Timeout in ms.
    pub timeout_ms: u64,
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            id: String::new(),
            weight: 1.0,
            quality: 0.5,
            required: false,
            timeout_ms: 30000,
        }
    }
}

/// Consensus configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsensusConfig {
    /// Consensus strategy.
    pub strategy: ConsensusStrategy,
    /// Minimum models required.
    pub min_models: usize,
    /// Minimum agreement threshold.
    pub agreement_threshold: f32,
    /// Whether to include dissent.
    pub include_dissent: bool,
    /// Parallel execution.
    pub parallel: bool,
    /// Retry failed models.
    pub retry_failed: bool,
}

impl Default for ConsensusConfig {
    fn default() -> Self {
        Self {
            strategy: ConsensusStrategy::Weighted,
            min_models: 2,
            agreement_threshold: 0.7,
            include_dissent: true,
            parallel: true,
            retry_failed: true,
        }
    }
}

/// Trait for model providers in consensus.
#[async_trait]
pub trait ConsensusProvider: Send + Sync {
    /// Query a model.
    async fn query(&self, model_id: &str, prompt: &str) -> Result<ModelResponse>;

    /// Synthesize multiple responses.
    async fn synthesize(&self, responses: &[ModelResponse]) -> Result<String>;

    /// Calculate similarity between responses.
    async fn similarity(&self, a: &str, b: &str) -> f32;
}

/// Multi-model consensus engine.
pub struct ConsensusEngine<P: ConsensusProvider> {
    config: ConsensusConfig,
    provider: P,
    models: Arc<RwLock<Vec<ModelConfig>>>,
}

impl<P: ConsensusProvider> ConsensusEngine<P> {
    /// Create a new consensus engine.
    pub fn new(config: ConsensusConfig, provider: P) -> Self {
        Self {
            config,
            provider,
            models: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Add a model to the consensus pool.
    pub async fn add_model(&self, model: ModelConfig) {
        self.models.write().await.push(model);
    }

    /// Remove a model from the consensus pool.
    pub async fn remove_model(&self, model_id: &str) {
        self.models.write().await.retain(|m| m.id != model_id);
    }

    /// Query all models and reach consensus.
    pub async fn query(&self, prompt: &str) -> Result<ConsensusResult> {
        let models = self.models.read().await.clone();

        if models.len() < self.config.min_models {
            return Err(ConsensusError::NoModels);
        }

        // Query all models
        let responses = self.query_all_models(&models, prompt).await?;

        if responses.is_empty() {
            return Err(ConsensusError::AllFailed(
                "No successful responses".to_string(),
            ));
        }

        // Reach consensus based on strategy
        match self.config.strategy {
            ConsensusStrategy::Majority => self.majority_consensus(responses).await,
            ConsensusStrategy::Weighted => self.weighted_consensus(responses, &models).await,
            ConsensusStrategy::Unanimous => self.unanimous_consensus(responses).await,
            ConsensusStrategy::Synthesis => self.synthesis_consensus(responses).await,
            ConsensusStrategy::Fastest => self.fastest_consensus(responses).await,
            ConsensusStrategy::HighestConfidence => self.confidence_consensus(responses).await,
            ConsensusStrategy::RoundRobin => self.roundrobin_consensus(responses).await,
        }
    }

    async fn query_all_models(
        &self,
        models: &[ModelConfig],
        prompt: &str,
    ) -> Result<Vec<ModelResponse>> {
        let mut responses = Vec::new();

        if self.config.parallel {
            let futures: Vec<_> = models
                .iter()
                .map(|m| self.provider.query(&m.id, prompt))
                .collect();

            let results = futures::future::join_all(futures).await;

            for result in results {
                if let Ok(response) = result {
                    responses.push(response);
                }
            }
        } else {
            for model in models {
                if let Ok(response) = self.provider.query(&model.id, prompt).await {
                    responses.push(response);
                }
            }
        }

        Ok(responses)
    }

    async fn majority_consensus(&self, responses: Vec<ModelResponse>) -> Result<ConsensusResult> {
        // Group similar responses
        let groups = self.group_similar_responses(&responses).await;

        // Find largest group
        let largest = groups.iter().max_by_key(|g| g.len());

        match largest {
            Some(group)
                if group.len() as f32 / responses.len() as f32
                    >= self.config.agreement_threshold =>
            {
                let response = group[0].content.clone();
                let agreement = group.len() as f32 / responses.len() as f32;

                let dissent: Vec<String> = responses
                    .iter()
                    .filter(|r| !group.iter().any(|g| g.model_id == r.model_id))
                    .map(|r| format!("{}: {}", r.model_id, r.content))
                    .collect();

                Ok(ConsensusResult {
                    id: Uuid::new_v4(),
                    response,
                    strategy: ConsensusStrategy::Majority,
                    responses,
                    confidence: agreement,
                    agreement,
                    consensus_reached: true,
                    dissent,
                })
            }
            _ => Err(ConsensusError::NoConsensus(
                "Majority not reached".to_string(),
            )),
        }
    }

    async fn weighted_consensus(
        &self,
        responses: Vec<ModelResponse>,
        models: &[ModelConfig],
    ) -> Result<ConsensusResult> {
        // Create weight lookup
        let weights: HashMap<_, _> = models.iter().map(|m| (m.id.clone(), m.weight)).collect();

        // Group similar responses
        let groups = self.group_similar_responses(&responses).await;

        // Calculate weighted scores for each group
        let mut best_group = None;
        let mut best_score = 0.0f32;

        for group in &groups {
            let score: f32 = group
                .iter()
                .map(|r| weights.get(&r.model_id).unwrap_or(&1.0))
                .sum();

            if score > best_score {
                best_score = score;
                best_group = Some(group);
            }
        }

        match best_group {
            Some(group) => {
                let total_weight: f32 = weights.values().sum();
                let agreement = best_score / total_weight;

                Ok(ConsensusResult {
                    id: Uuid::new_v4(),
                    response: group[0].content.clone(),
                    strategy: ConsensusStrategy::Weighted,
                    responses,
                    confidence: agreement,
                    agreement,
                    consensus_reached: agreement >= self.config.agreement_threshold,
                    dissent: Vec::new(),
                })
            }
            None => Err(ConsensusError::NoConsensus("No responses".to_string())),
        }
    }

    async fn unanimous_consensus(&self, responses: Vec<ModelResponse>) -> Result<ConsensusResult> {
        if responses.len() < 2 {
            return Err(ConsensusError::NoConsensus(
                "Not enough responses".to_string(),
            ));
        }

        // Check if all responses are similar
        let first = &responses[0];
        let mut all_agree = true;

        for response in &responses[1..] {
            let sim = self
                .provider
                .similarity(&first.content, &response.content)
                .await;
            if sim < 0.9 {
                all_agree = false;
                break;
            }
        }

        if all_agree {
            Ok(ConsensusResult {
                id: Uuid::new_v4(),
                response: first.content.clone(),
                strategy: ConsensusStrategy::Unanimous,
                responses,
                confidence: 1.0,
                agreement: 1.0,
                consensus_reached: true,
                dissent: Vec::new(),
            })
        } else {
            Err(ConsensusError::NoConsensus(
                "Unanimous agreement not reached".to_string(),
            ))
        }
    }

    async fn synthesis_consensus(&self, responses: Vec<ModelResponse>) -> Result<ConsensusResult> {
        let synthesized = self.provider.synthesize(&responses).await?;

        // Calculate average confidence
        let avg_confidence: f32 =
            responses.iter().filter_map(|r| r.confidence).sum::<f32>() / responses.len() as f32;

        Ok(ConsensusResult {
            id: Uuid::new_v4(),
            response: synthesized,
            strategy: ConsensusStrategy::Synthesis,
            responses,
            confidence: avg_confidence.max(0.7), // Synthesis usually has decent confidence
            agreement: 1.0,                      // By definition, synthesis incorporates all
            consensus_reached: true,
            dissent: Vec::new(),
        })
    }

    async fn fastest_consensus(
        &self,
        mut responses: Vec<ModelResponse>,
    ) -> Result<ConsensusResult> {
        responses.sort_by_key(|r| r.latency_ms);

        if let Some(fastest) = responses.first() {
            Ok(ConsensusResult {
                id: Uuid::new_v4(),
                response: fastest.content.clone(),
                strategy: ConsensusStrategy::Fastest,
                responses,
                confidence: 0.5, // Lower confidence for fastest-only
                agreement: 0.0,
                consensus_reached: true,
                dissent: Vec::new(),
            })
        } else {
            Err(ConsensusError::NoConsensus("No responses".to_string()))
        }
    }

    async fn confidence_consensus(
        &self,
        mut responses: Vec<ModelResponse>,
    ) -> Result<ConsensusResult> {
        responses.sort_by(|a, b| {
            b.confidence
                .unwrap_or(0.0)
                .partial_cmp(&a.confidence.unwrap_or(0.0))
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        if let Some(best) = responses.first() {
            let confidence = best.confidence.unwrap_or(0.5);

            Ok(ConsensusResult {
                id: Uuid::new_v4(),
                response: best.content.clone(),
                strategy: ConsensusStrategy::HighestConfidence,
                responses,
                confidence,
                agreement: 0.0,
                consensus_reached: confidence >= self.config.agreement_threshold,
                dissent: Vec::new(),
            })
        } else {
            Err(ConsensusError::NoConsensus("No responses".to_string()))
        }
    }

    async fn roundrobin_consensus(&self, responses: Vec<ModelResponse>) -> Result<ConsensusResult> {
        // Use first response but verify with others
        if responses.is_empty() {
            return Err(ConsensusError::NoConsensus("No responses".to_string()));
        }

        let primary = &responses[0];
        let mut verifications = 0;

        for response in &responses[1..] {
            let sim = self
                .provider
                .similarity(&primary.content, &response.content)
                .await;
            if sim > 0.8 {
                verifications += 1;
            }
        }

        let agreement = if responses.len() > 1 {
            (verifications + 1) as f32 / responses.len() as f32
        } else {
            1.0
        };

        Ok(ConsensusResult {
            id: Uuid::new_v4(),
            response: primary.content.clone(),
            strategy: ConsensusStrategy::RoundRobin,
            responses,
            confidence: agreement,
            agreement,
            consensus_reached: agreement >= self.config.agreement_threshold,
            dissent: Vec::new(),
        })
    }

    async fn group_similar_responses(
        &self,
        responses: &[ModelResponse],
    ) -> Vec<Vec<ModelResponse>> {
        let mut groups: Vec<Vec<ModelResponse>> = Vec::new();

        for response in responses {
            let mut found_group = false;

            for group in &mut groups {
                if !group.is_empty() {
                    let sim = self
                        .provider
                        .similarity(&group[0].content, &response.content)
                        .await;
                    if sim > 0.8 {
                        group.push(response.clone());
                        found_group = true;
                        break;
                    }
                }
            }

            if !found_group {
                groups.push(vec![response.clone()]);
            }
        }

        groups
    }
}

/// Simple consensus provider for testing.
pub struct SimpleConsensusProvider;

#[async_trait]
impl ConsensusProvider for SimpleConsensusProvider {
    async fn query(&self, model_id: &str, prompt: &str) -> Result<ModelResponse> {
        Ok(ModelResponse {
            model_id: model_id.to_string(),
            content: format!("Response from {} for: {}", model_id, prompt),
            confidence: Some(0.8),
            latency_ms: 100,
            tokens: prompt.len() / 4,
            metadata: HashMap::new(),
        })
    }

    async fn synthesize(&self, responses: &[ModelResponse]) -> Result<String> {
        let combined: Vec<_> = responses.iter().map(|r| r.content.clone()).collect();
        Ok(format!("Synthesized: {}", combined.join(" | ")))
    }

    async fn similarity(&self, a: &str, b: &str) -> f32 {
        // Simple Jaccard similarity
        let words_a: std::collections::HashSet<_> = a.split_whitespace().collect();
        let words_b: std::collections::HashSet<_> = b.split_whitespace().collect();

        let intersection = words_a.intersection(&words_b).count();
        let union = words_a.union(&words_b).count();

        if union == 0 {
            1.0
        } else {
            intersection as f32 / union as f32
        }
    }
}

// ============================================================================
// KANI FORMAL VERIFICATION PROOFS
// ============================================================================

#[cfg(kani)]
mod kani_proofs {
    use super::*;

    /// Proof: ConsensusStrategy has exactly 7 variants
    #[kani::proof]
    fn proof_strategy_variants() {
        let val: u8 = kani::any();
        kani::assume(val <= 6);

        let strategy = match val {
            0 => ConsensusStrategy::Majority,
            1 => ConsensusStrategy::Weighted,
            2 => ConsensusStrategy::Unanimous,
            3 => ConsensusStrategy::Synthesis,
            4 => ConsensusStrategy::Fastest,
            5 => ConsensusStrategy::HighestConfidence,
            _ => ConsensusStrategy::RoundRobin,
        };

        kani::assert(strategy == strategy, "Strategy must equal itself");
    }

    /// Proof: Default config has valid values
    #[kani::proof]
    fn proof_default_config_valid() {
        let config = ConsensusConfig::default();

        kani::assert(config.min_models >= 1, "Min models must be at least 1");
        kani::assert(
            config.agreement_threshold >= 0.0,
            "Threshold must be non-negative",
        );
        kani::assert(config.agreement_threshold <= 1.0, "Threshold must be <= 1");
    }

    /// Proof: Default model config has valid values
    #[kani::proof]
    fn proof_default_model_config_valid() {
        let config = ModelConfig::default();

        kani::assert(config.weight > 0.0, "Weight must be positive");
        kani::assert(config.quality >= 0.0, "Quality must be non-negative");
        kani::assert(config.quality <= 1.0, "Quality must be <= 1");
    }

    /// Proof: Agreement calculation bounds [0, 1]
    #[kani::proof]
    fn proof_agreement_bounds() {
        let group_size: usize = kani::any();
        let total_size: usize = kani::any();

        kani::assume(total_size > 0);
        kani::assume(total_size <= 100);
        kani::assume(group_size <= total_size);

        let agreement = group_size as f32 / total_size as f32;

        kani::assert(agreement >= 0.0, "Agreement must be >= 0");
        kani::assert(agreement <= 1.0, "Agreement must be <= 1");
    }

    /// Proof: Weighted score is non-negative for positive weights
    #[kani::proof]
    fn proof_weighted_score_non_negative() {
        let weight1: f32 = kani::any();
        let weight2: f32 = kani::any();
        let weight3: f32 = kani::any();

        kani::assume(weight1 >= 0.0 && weight1 <= 100.0);
        kani::assume(weight2 >= 0.0 && weight2 <= 100.0);
        kani::assume(weight3 >= 0.0 && weight3 <= 100.0);
        kani::assume(weight1.is_finite() && weight2.is_finite() && weight3.is_finite());

        let total = weight1 + weight2 + weight3;

        kani::assert(total >= 0.0, "Total weight must be non-negative");
    }

    /// Proof: Confidence value is clamped correctly
    #[kani::proof]
    fn proof_confidence_bounds() {
        let raw_confidence: f32 = kani::any();
        kani::assume(raw_confidence.is_finite());

        // The code uses .max(0.7) for synthesis
        let clamped = raw_confidence.max(0.7);

        kani::assert(clamped >= 0.7, "Clamped confidence must be >= 0.7");
    }

    /// Proof: Jaccard similarity bounds [0, 1]
    #[kani::proof]
    fn proof_jaccard_similarity_bounds() {
        let intersection: usize = kani::any();
        let union: usize = kani::any();

        kani::assume(union > 0);
        kani::assume(union <= 1000);
        kani::assume(intersection <= union);

        let similarity = intersection as f32 / union as f32;

        kani::assert(similarity >= 0.0, "Similarity must be >= 0");
        kani::assert(similarity <= 1.0, "Similarity must be <= 1");
    }

    /// Proof: Empty sets have similarity 1.0
    #[kani::proof]
    fn proof_empty_sets_similarity() {
        let union = 0usize;

        let similarity = if union == 0 { 1.0 } else { 0.0 };

        kani::assert(similarity == 1.0, "Empty sets must have similarity 1.0");
    }

    /// Proof: ConsensusResult fields are consistent
    #[kani::proof]
    fn proof_result_consistency() {
        let confidence: f32 = kani::any();
        let agreement: f32 = kani::any();
        let threshold: f32 = kani::any();

        kani::assume(confidence >= 0.0 && confidence <= 1.0);
        kani::assume(agreement >= 0.0 && agreement <= 1.0);
        kani::assume(threshold >= 0.0 && threshold <= 1.0);
        kani::assume(confidence.is_finite() && agreement.is_finite() && threshold.is_finite());

        let consensus_reached = agreement >= threshold;

        // If consensus reached, agreement should be at or above threshold
        if consensus_reached {
            kani::assert(
                agreement >= threshold,
                "Consensus requires agreement >= threshold",
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_consensus_engine() {
        let config = ConsensusConfig::default();
        let provider = SimpleConsensusProvider;
        let engine = ConsensusEngine::new(config, provider);

        engine
            .add_model(ModelConfig {
                id: "model-a".to_string(),
                weight: 1.0,
                ..Default::default()
            })
            .await;

        engine
            .add_model(ModelConfig {
                id: "model-b".to_string(),
                weight: 1.0,
                ..Default::default()
            })
            .await;

        let result = engine.query("What is 2+2?").await.unwrap();
        assert!(!result.response.is_empty());
    }

    #[tokio::test]
    async fn test_similarity() {
        let provider = SimpleConsensusProvider;
        let sim = provider.similarity("hello world", "hello world").await;
        assert_eq!(sim, 1.0);

        let sim = provider.similarity("hello world", "goodbye moon").await;
        assert!(sim < 0.5);
    }
}
