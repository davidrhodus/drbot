//! Routing strategies for provider selection.

use crate::{classifier::TaskComplexity, RegisteredProvider, SelectionCriteria};

/// Trait for implementing routing strategies.
pub trait RoutingStrategy: Send + Sync {
    /// Select a provider from candidates.
    fn select(
        &self,
        candidates: &[RegisteredProvider],
        complexity: TaskComplexity,
        criteria: &SelectionCriteria,
    ) -> RegisteredProvider;
}

/// Cost-optimized strategy: prefer cheaper providers when possible.
pub struct CostOptimizedStrategy;

impl RoutingStrategy for CostOptimizedStrategy {
    fn select(
        &self,
        candidates: &[RegisteredProvider],
        complexity: TaskComplexity,
        criteria: &SelectionCriteria,
    ) -> RegisteredProvider {
        // For simple tasks, use cheapest provider
        // For complex tasks, use high quality provider
        let min_quality = match complexity {
            TaskComplexity::Simple => 1,
            TaskComplexity::Medium => 2,
            TaskComplexity::Complex => 4,
            TaskComplexity::Expert => 5,
        };

        // Filter by minimum quality for complexity
        let eligible: Vec<_> = candidates
            .iter()
            .filter(|p| p.quality_tier >= min_quality)
            .collect();

        // If we have eligible providers, pick cheapest
        if !eligible.is_empty() {
            eligible
                .into_iter()
                .min_by_key(|p| p.cost_tier)
                .cloned()
                .unwrap()
        } else {
            // Fallback to cheapest overall
            candidates
                .iter()
                .min_by_key(|p| p.cost_tier)
                .cloned()
                .unwrap()
        }
    }
}

/// Quality-first strategy: prefer highest quality providers.
pub struct QualityFirstStrategy;

impl RoutingStrategy for QualityFirstStrategy {
    fn select(
        &self,
        candidates: &[RegisteredProvider],
        _complexity: TaskComplexity,
        criteria: &SelectionCriteria,
    ) -> RegisteredProvider {
        // Check for preferred provider first
        if let Some(preferred) = &criteria.preferred_provider {
            if let Some(provider) = candidates.iter().find(|p| &p.name == preferred) {
                return provider.clone();
            }
        }

        // Otherwise pick highest quality
        candidates
            .iter()
            .max_by_key(|p| p.quality_tier)
            .cloned()
            .unwrap()
    }
}

/// Balanced strategy: balance cost and quality.
pub struct BalancedStrategy {
    /// Weight for cost (0-1).
    pub cost_weight: f32,
    /// Weight for quality (0-1).
    pub quality_weight: f32,
}

impl Default for BalancedStrategy {
    fn default() -> Self {
        Self {
            cost_weight: 0.5,
            quality_weight: 0.5,
        }
    }
}

impl RoutingStrategy for BalancedStrategy {
    fn select(
        &self,
        candidates: &[RegisteredProvider],
        complexity: TaskComplexity,
        criteria: &SelectionCriteria,
    ) -> RegisteredProvider {
        // Check for preferred provider first
        if let Some(preferred) = &criteria.preferred_provider {
            if let Some(provider) = candidates.iter().find(|p| &p.name == preferred) {
                return provider.clone();
            }
        }

        // Adjust weights based on complexity
        let (cost_w, quality_w) = match complexity {
            TaskComplexity::Simple => (0.7, 0.3),
            TaskComplexity::Medium => (self.cost_weight, self.quality_weight),
            TaskComplexity::Complex => (0.3, 0.7),
            TaskComplexity::Expert => (0.1, 0.9),
        };

        // Score each provider: higher is better
        // Invert cost tier (5 - cost) so lower cost = higher score
        candidates
            .iter()
            .max_by(|a, b| {
                let score_a =
                    (5.0 - a.cost_tier as f32) * cost_w + a.quality_tier as f32 * quality_w;
                let score_b =
                    (5.0 - b.cost_tier as f32) * cost_w + b.quality_tier as f32 * quality_w;
                score_a.partial_cmp(&score_b).unwrap()
            })
            .cloned()
            .unwrap()
    }
}

/// Complexity-aware strategy: match provider to task complexity.
pub struct ComplexityAwareStrategy;

impl RoutingStrategy for ComplexityAwareStrategy {
    fn select(
        &self,
        candidates: &[RegisteredProvider],
        complexity: TaskComplexity,
        criteria: &SelectionCriteria,
    ) -> RegisteredProvider {
        // Check for preferred provider first
        if let Some(preferred) = &criteria.preferred_provider {
            if let Some(provider) = candidates.iter().find(|p| &p.name == preferred) {
                return provider.clone();
            }
        }

        // Target quality tier based on complexity
        let target_quality: u8 = match complexity {
            TaskComplexity::Simple => 2,
            TaskComplexity::Medium => 3,
            TaskComplexity::Complex => 4,
            TaskComplexity::Expert => 5,
        };

        // Find provider closest to target quality while being cost-efficient
        candidates
            .iter()
            .filter(|p| p.quality_tier >= target_quality.saturating_sub(1))
            .min_by_key(|p| {
                let quality_diff = (p.quality_tier as i32 - target_quality as i32).abs();
                // Combine quality match with cost preference
                quality_diff * 10 + p.cost_tier as i32
            })
            .cloned()
            .unwrap_or_else(|| candidates[0].clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use drbot_providers::Provider;
    use std::sync::Arc;

    fn mock_provider(name: &str, cost: u8, quality: u8) -> RegisteredProvider {
        struct DummyProvider(String);

        #[async_trait]
        impl Provider for DummyProvider {
            fn name(&self) -> &str {
                &self.0
            }
            fn models(&self) -> Vec<drbot_providers::ModelInfo> {
                vec![]
            }
            async fn chat(
                &self,
                _: &[drbot_core::message::Message],
                _: drbot_providers::ChatOptions,
            ) -> drbot_core::Result<drbot_providers::ChatResponse> {
                Err(drbot_core::Error::Provider("mock".into()))
            }
            async fn stream(
                &self,
                _: &[drbot_core::message::Message],
                _: drbot_providers::ChatOptions,
            ) -> drbot_core::Result<
                std::pin::Pin<Box<dyn futures::Stream<Item = drbot_providers::StreamEvent> + Send>>,
            > {
                Err(drbot_core::Error::Provider("mock".into()))
            }
        }

        RegisteredProvider::new(Arc::new(DummyProvider(name.to_string())), name)
            .with_cost_tier(cost)
            .with_quality_tier(quality)
    }

    #[test]
    fn test_cost_optimized_strategy_simple() {
        let strategy = CostOptimizedStrategy;
        let candidates = vec![
            mock_provider("expensive", 5, 5),
            mock_provider("cheap", 1, 2),
            mock_provider("medium", 3, 3),
        ];

        let selected = strategy.select(
            &candidates,
            TaskComplexity::Simple,
            &SelectionCriteria::default(),
        );
        assert_eq!(selected.name, "cheap");
    }

    #[test]
    fn test_cost_optimized_strategy_complex() {
        let strategy = CostOptimizedStrategy;
        let candidates = vec![
            mock_provider("expensive", 5, 5),
            mock_provider("cheap", 1, 2),
            mock_provider("medium", 3, 4),
        ];

        let selected = strategy.select(
            &candidates,
            TaskComplexity::Complex,
            &SelectionCriteria::default(),
        );
        // Should pick medium (cheapest with quality >= 4) or expensive
        assert!(selected.quality_tier >= 4);
    }

    #[test]
    fn test_quality_first_strategy() {
        let strategy = QualityFirstStrategy;
        let candidates = vec![
            mock_provider("cheap", 1, 2),
            mock_provider("best", 4, 5),
            mock_provider("medium", 3, 3),
        ];

        let selected = strategy.select(
            &candidates,
            TaskComplexity::Medium,
            &SelectionCriteria::default(),
        );
        assert_eq!(selected.name, "best");
    }
}
