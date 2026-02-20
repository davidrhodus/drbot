//! Main model router implementation.

use crate::{
    classifier::{TaskClassifier, TaskComplexity},
    strategy::RoutingStrategy,
    RegisteredProvider, Result, RouterError, RoutingDecision, SelectionCriteria,
};
use drbot_core::message::Message;
use drbot_providers::{ChatOptions, ChatResponse, StreamEvent};
use futures::Stream;
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Configuration for the model router.
#[derive(Debug, Clone)]
pub struct RouterConfig {
    /// Enable automatic task classification.
    pub auto_classify: bool,
    /// Enable automatic fallback on failure.
    pub auto_fallback: bool,
    /// Maximum retry attempts.
    pub max_retries: usize,
    /// Enable cost optimization.
    pub optimize_cost: bool,
    /// Default quality tier.
    pub default_quality: u8,
}

impl Default for RouterConfig {
    fn default() -> Self {
        Self {
            auto_classify: true,
            auto_fallback: true,
            max_retries: 2,
            optimize_cost: true,
            default_quality: 3,
        }
    }
}

/// Multi-model router for intelligent request routing.
pub struct ModelRouter {
    config: RouterConfig,
    providers: RwLock<HashMap<String, RegisteredProvider>>,
    classifier: Arc<dyn TaskClassifier>,
    strategy: Arc<dyn RoutingStrategy>,
}

impl ModelRouter {
    /// Create a new model router.
    pub fn new(
        config: RouterConfig,
        classifier: Arc<dyn TaskClassifier>,
        strategy: Arc<dyn RoutingStrategy>,
    ) -> Self {
        Self {
            config,
            providers: RwLock::new(HashMap::new()),
            classifier,
            strategy,
        }
    }

    /// Register a provider.
    pub async fn register(&self, provider: RegisteredProvider) {
        let mut providers = self.providers.write().await;
        info!(name = %provider.name, "Registering provider");
        providers.insert(provider.name.clone(), provider);
    }

    /// Unregister a provider.
    pub async fn unregister(&self, name: &str) {
        let mut providers = self.providers.write().await;
        providers.remove(name);
    }

    /// Get a provider by name.
    pub async fn get_provider(&self, name: &str) -> Option<RegisteredProvider> {
        let providers = self.providers.read().await;
        providers.get(name).cloned()
    }

    /// List all registered providers.
    pub async fn list_providers(&self) -> Vec<RegisteredProvider> {
        let providers = self.providers.read().await;
        providers.values().cloned().collect()
    }

    /// Mark a provider as unhealthy.
    pub async fn mark_unhealthy(&self, name: &str) {
        let mut providers = self.providers.write().await;
        if let Some(provider) = providers.get_mut(name) {
            warn!(name, "Marking provider as unhealthy");
            provider.healthy = false;
        }
    }

    /// Mark a provider as healthy.
    pub async fn mark_healthy(&self, name: &str) {
        let mut providers = self.providers.write().await;
        if let Some(provider) = providers.get_mut(name) {
            info!(name, "Marking provider as healthy");
            provider.healthy = true;
        }
    }

    /// Select a provider based on the messages and criteria.
    pub async fn select(
        &self,
        messages: &[Message],
        criteria: SelectionCriteria,
    ) -> Result<RoutingDecision> {
        let providers = self.providers.read().await;

        if providers.is_empty() {
            return Err(RouterError::NoProviders);
        }

        // Classify the task if auto-classify is enabled
        let complexity = if self.config.auto_classify {
            self.classifier.classify(messages)
        } else {
            TaskComplexity::Medium
        };

        debug!(?complexity, "Task classified");

        // Use strategy to select provider
        let candidates: Vec<_> = providers
            .values()
            .filter(|p| criteria.matches(p))
            .cloned()
            .collect();

        if candidates.is_empty() {
            return Err(RouterError::NoProviders);
        }

        let selected = self.strategy.select(&candidates, complexity, &criteria);

        // Build fallback list
        let fallbacks: Vec<String> = candidates
            .iter()
            .filter(|p| p.name != selected.name)
            .take(self.config.max_retries)
            .map(|p| p.name.clone())
            .collect();

        Ok(RoutingDecision {
            provider_name: selected.name.clone(),
            reason: format!(
                "Selected {} (cost: {}, quality: {}) for {:?} task",
                selected.name, selected.cost_tier, selected.quality_tier, complexity
            ),
            cost_tier: selected.cost_tier,
            fallbacks,
        })
    }

    /// Chat with automatic provider selection.
    pub async fn chat(
        &self,
        messages: &[Message],
        options: ChatOptions,
        criteria: SelectionCriteria,
    ) -> Result<(ChatResponse, RoutingDecision)> {
        let decision = self.select(messages, criteria.clone()).await?;

        // Try selected provider first
        let provider = self
            .get_provider(&decision.provider_name)
            .await
            .ok_or(RouterError::NoProviders)?;

        let mut last_error = match provider.provider.chat(messages, options.clone()).await {
            Ok(response) => return Ok((response, decision.clone())),
            Err(e) => {
                warn!(provider = %decision.provider_name, error = %e, "Provider failed");
                if self.config.auto_fallback {
                    self.mark_unhealthy(&decision.provider_name).await;
                }
                e.to_string()
            }
        };

        // Try fallbacks if enabled
        if self.config.auto_fallback {
            for fallback_name in &decision.fallbacks {
                if let Some(fallback) = self.get_provider(fallback_name).await {
                    info!(provider = %fallback_name, "Trying fallback provider");
                    match fallback.provider.chat(messages, options.clone()).await {
                        Ok(response) => {
                            let mut new_decision = decision.clone();
                            new_decision.provider_name = fallback_name.clone();
                            new_decision.reason =
                                format!("Fallback to {} after primary failure", fallback_name);
                            return Ok((response, new_decision));
                        }
                        Err(e) => {
                            warn!(provider = %fallback_name, error = %e, "Fallback failed");
                            last_error = e.to_string();
                            self.mark_unhealthy(fallback_name).await;
                        }
                    }
                }
            }
        }

        Err(RouterError::AllProvidersFailed(last_error))
    }

    /// Stream with automatic provider selection.
    pub async fn stream(
        &self,
        messages: &[Message],
        options: ChatOptions,
        criteria: SelectionCriteria,
    ) -> Result<(
        Pin<Box<dyn Stream<Item = StreamEvent> + Send>>,
        RoutingDecision,
    )> {
        let mut criteria = criteria;
        criteria.require_streaming = true;

        let decision = self.select(messages, criteria).await?;

        let provider = self
            .get_provider(&decision.provider_name)
            .await
            .ok_or(RouterError::NoProviders)?;

        let stream = provider
            .provider
            .stream(messages, options)
            .await
            .map_err(|e| RouterError::ProviderError(e.to_string()))?;

        Ok((stream, decision))
    }

    /// Compare responses from multiple providers.
    pub async fn compare(
        &self,
        messages: &[Message],
        options: ChatOptions,
        provider_names: Vec<String>,
    ) -> Vec<(String, Result<ChatResponse>)> {
        let mut results = Vec::new();

        for name in provider_names {
            let result = if let Some(provider) = self.get_provider(&name).await {
                provider
                    .provider
                    .chat(messages, options.clone())
                    .await
                    .map_err(|e| RouterError::ProviderError(e.to_string()))
            } else {
                Err(RouterError::NoProviders)
            };
            results.push((name, result));
        }

        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_router_config_default() {
        let config = RouterConfig::default();
        assert!(config.auto_classify);
        assert!(config.auto_fallback);
        assert_eq!(config.max_retries, 2);
    }
}
