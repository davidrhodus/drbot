//! Load balancing strategies for provider distribution.

use crate::RegisteredProvider;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Trait for load balancing across providers.
pub trait LoadBalancer: Send + Sync {
    /// Select the next provider from candidates.
    fn next(&self, candidates: &[RegisteredProvider]) -> Option<RegisteredProvider>;

    /// Reset the balancer state.
    fn reset(&self);
}

/// Round-robin load balancer.
pub struct RoundRobinBalancer {
    counter: AtomicUsize,
}

impl RoundRobinBalancer {
    /// Create a new round-robin balancer.
    pub fn new() -> Self {
        Self {
            counter: AtomicUsize::new(0),
        }
    }
}

impl Default for RoundRobinBalancer {
    fn default() -> Self {
        Self::new()
    }
}

impl LoadBalancer for RoundRobinBalancer {
    fn next(&self, candidates: &[RegisteredProvider]) -> Option<RegisteredProvider> {
        if candidates.is_empty() {
            return None;
        }

        let index = self.counter.fetch_add(1, Ordering::SeqCst) % candidates.len();
        Some(candidates[index].clone())
    }

    fn reset(&self) {
        self.counter.store(0, Ordering::SeqCst);
    }
}

/// Weighted load balancer that distributes based on weights.
pub struct WeightedBalancer {
    counter: AtomicUsize,
}

impl WeightedBalancer {
    /// Create a new weighted balancer.
    pub fn new() -> Self {
        Self {
            counter: AtomicUsize::new(0),
        }
    }

    /// Calculate weight for a provider (lower cost + higher quality = higher weight).
    fn weight(provider: &RegisteredProvider) -> usize {
        // Invert cost (5 - cost) and add quality
        let cost_score = 6 - provider.cost_tier as usize;
        let quality_score = provider.quality_tier as usize;
        cost_score + quality_score
    }
}

impl Default for WeightedBalancer {
    fn default() -> Self {
        Self::new()
    }
}

impl LoadBalancer for WeightedBalancer {
    fn next(&self, candidates: &[RegisteredProvider]) -> Option<RegisteredProvider> {
        if candidates.is_empty() {
            return None;
        }

        // Calculate total weight
        let total_weight: usize = candidates.iter().map(Self::weight).sum();

        if total_weight == 0 {
            return Some(candidates[0].clone());
        }

        // Get current position in cycle
        let position = self.counter.fetch_add(1, Ordering::SeqCst) % total_weight;

        // Find provider at position
        let mut cumulative = 0;
        for provider in candidates {
            cumulative += Self::weight(provider);
            if position < cumulative {
                return Some(provider.clone());
            }
        }

        Some(candidates.last().unwrap().clone())
    }

    fn reset(&self) {
        self.counter.store(0, Ordering::SeqCst);
    }
}

/// Random load balancer.
pub struct RandomBalancer;

impl RandomBalancer {
    /// Create a new random balancer.
    pub fn new() -> Self {
        Self
    }
}

impl Default for RandomBalancer {
    fn default() -> Self {
        Self::new()
    }
}

impl LoadBalancer for RandomBalancer {
    fn next(&self, candidates: &[RegisteredProvider]) -> Option<RegisteredProvider> {
        if candidates.is_empty() {
            return None;
        }

        // Simple pseudo-random based on time
        let index = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as usize)
            .unwrap_or(0)
            % candidates.len();

        Some(candidates[index].clone())
    }

    fn reset(&self) {
        // No state to reset
    }
}

/// Least-loaded balancer (placeholder - would need actual load tracking).
pub struct LeastLoadedBalancer {
    round_robin: RoundRobinBalancer,
}

impl LeastLoadedBalancer {
    /// Create a new least-loaded balancer.
    pub fn new() -> Self {
        Self {
            round_robin: RoundRobinBalancer::new(),
        }
    }
}

impl Default for LeastLoadedBalancer {
    fn default() -> Self {
        Self::new()
    }
}

impl LoadBalancer for LeastLoadedBalancer {
    fn next(&self, candidates: &[RegisteredProvider]) -> Option<RegisteredProvider> {
        // For now, fall back to round-robin
        // In production, this would track actual request counts
        self.round_robin.next(candidates)
    }

    fn reset(&self) {
        self.round_robin.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use drbot_providers::Provider;
    use std::sync::Arc;

    fn mock_provider(name: &str) -> RegisteredProvider {
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
    }

    #[test]
    fn test_round_robin() {
        let balancer = RoundRobinBalancer::new();
        let candidates = vec![mock_provider("a"), mock_provider("b"), mock_provider("c")];

        assert_eq!(balancer.next(&candidates).unwrap().name, "a");
        assert_eq!(balancer.next(&candidates).unwrap().name, "b");
        assert_eq!(balancer.next(&candidates).unwrap().name, "c");
        assert_eq!(balancer.next(&candidates).unwrap().name, "a");
    }

    #[test]
    fn test_round_robin_reset() {
        let balancer = RoundRobinBalancer::new();
        let candidates = vec![mock_provider("a"), mock_provider("b")];

        balancer.next(&candidates);
        balancer.next(&candidates);
        balancer.reset();

        assert_eq!(balancer.next(&candidates).unwrap().name, "a");
    }

    #[test]
    fn test_empty_candidates() {
        let balancer = RoundRobinBalancer::new();
        let candidates: Vec<RegisteredProvider> = vec![];

        assert!(balancer.next(&candidates).is_none());
    }
}
