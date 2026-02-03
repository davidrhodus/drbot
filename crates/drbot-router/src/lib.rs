//! Multi-model routing and orchestration for drbot.
//!
//! This crate provides intelligent routing of requests to the most appropriate
//! AI model based on task complexity, cost, and capability requirements.
//!
//! # Features
//!
//! - **Smart routing**: Automatically select the best model for each task
//! - **Cost optimization**: Route simple queries to cheaper models
//! - **Fallback chains**: Automatic failover between providers
//! - **Load balancing**: Distribute requests across multiple providers
//! - **A/B testing**: Compare responses from multiple models

mod balancer;
mod classifier;
mod inbox;
mod router;
mod strategy;
mod triage;

pub use balancer::{LoadBalancer, RoundRobinBalancer, WeightedBalancer};
pub use classifier::{TaskClassifier, TaskComplexity, TaskType};
pub use inbox::{
    Attachment, AttachmentType, ChannelType, InboxFilter, InboxStats, MessageContent,
    MessageStatus, PriorityConfig, Sender, UnifiedInbox, UnifiedMessage,
};
pub use router::{ModelRouter, RouterConfig};
pub use strategy::{
    BalancedStrategy, CostOptimizedStrategy, QualityFirstStrategy, RoutingStrategy,
};
pub use triage::{
    AutoAction, DraftReply, MessageCategory, MessageIntent, ReplyTone, RuleBasedTriageProvider,
    Sentiment, SuggestedAction, TriageConfig, TriageManager, TriageProvider, TriageResult,
    UrgencyLevel,
};

use drbot_core::message::Message;
use drbot_providers::{ChatOptions, ChatResponse, Provider, StreamEvent};
use futures::Stream;
use std::pin::Pin;
use std::sync::Arc;

/// Result type for router operations.
pub type Result<T> = std::result::Result<T, RouterError>;

/// Router errors.
#[derive(Debug, thiserror::Error)]
pub enum RouterError {
    #[error("No providers available")]
    NoProviders,
    #[error("All providers failed: {0}")]
    AllProvidersFailed(String),
    #[error("Provider error: {0}")]
    ProviderError(String),
    #[error("Classification error: {0}")]
    ClassificationError(String),
}

/// A registered provider with metadata.
#[derive(Clone)]
pub struct RegisteredProvider {
    /// The provider instance.
    pub provider: Arc<dyn Provider>,
    /// Provider name for identification.
    pub name: String,
    /// Cost tier (1 = cheapest, 5 = most expensive).
    pub cost_tier: u8,
    /// Quality tier (1 = basic, 5 = best).
    pub quality_tier: u8,
    /// Maximum context window size.
    pub max_context: usize,
    /// Whether this provider supports streaming.
    pub supports_streaming: bool,
    /// Whether this provider supports tool use.
    pub supports_tools: bool,
    /// Whether this provider supports images.
    pub supports_images: bool,
    /// Current health status.
    pub healthy: bool,
}

impl RegisteredProvider {
    /// Create a new registered provider.
    pub fn new(provider: Arc<dyn Provider>, name: impl Into<String>) -> Self {
        Self {
            provider,
            name: name.into(),
            cost_tier: 3,
            quality_tier: 3,
            max_context: 128000,
            supports_streaming: true,
            supports_tools: true,
            supports_images: true,
            healthy: true,
        }
    }

    /// Set cost tier.
    pub fn with_cost_tier(mut self, tier: u8) -> Self {
        self.cost_tier = tier.min(5).max(1);
        self
    }

    /// Set quality tier.
    pub fn with_quality_tier(mut self, tier: u8) -> Self {
        self.quality_tier = tier.min(5).max(1);
        self
    }

    /// Set max context.
    pub fn with_max_context(mut self, max: usize) -> Self {
        self.max_context = max;
        self
    }

    /// Set streaming support.
    pub fn with_streaming(mut self, supports: bool) -> Self {
        self.supports_streaming = supports;
        self
    }

    /// Set tool use support.
    pub fn with_tools(mut self, supports: bool) -> Self {
        self.supports_tools = supports;
        self
    }

    /// Set image support.
    pub fn with_images(mut self, supports: bool) -> Self {
        self.supports_images = supports;
        self
    }
}

/// Selection criteria for routing.
#[derive(Debug, Clone, Default)]
pub struct SelectionCriteria {
    /// Minimum quality tier required.
    pub min_quality: Option<u8>,
    /// Maximum cost tier allowed.
    pub max_cost: Option<u8>,
    /// Required streaming support.
    pub require_streaming: bool,
    /// Required tool support.
    pub require_tools: bool,
    /// Required image support.
    pub require_images: bool,
    /// Minimum context window required.
    pub min_context: Option<usize>,
    /// Prefer specific provider by name.
    pub preferred_provider: Option<String>,
}

impl SelectionCriteria {
    /// Check if a provider matches the criteria.
    pub fn matches(&self, provider: &RegisteredProvider) -> bool {
        if !provider.healthy {
            return false;
        }
        if let Some(min_q) = self.min_quality {
            if provider.quality_tier < min_q {
                return false;
            }
        }
        if let Some(max_c) = self.max_cost {
            if provider.cost_tier > max_c {
                return false;
            }
        }
        if self.require_streaming && !provider.supports_streaming {
            return false;
        }
        if self.require_tools && !provider.supports_tools {
            return false;
        }
        if self.require_images && !provider.supports_images {
            return false;
        }
        if let Some(min_ctx) = self.min_context {
            if provider.max_context < min_ctx {
                return false;
            }
        }
        true
    }
}

/// Routing decision with selected provider.
#[derive(Debug, Clone)]
pub struct RoutingDecision {
    /// Selected provider name.
    pub provider_name: String,
    /// Reason for selection.
    pub reason: String,
    /// Estimated cost tier.
    pub cost_tier: u8,
    /// Fallback providers in order.
    pub fallbacks: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    #[test]
    fn test_selection_criteria() {
        let _criteria = SelectionCriteria {
            min_quality: Some(3),
            max_cost: Some(4),
            require_streaming: true,
            ..Default::default()
        };

        // Would pass
        // Would need an actual provider to test
    }

    #[test]
    fn test_registered_provider_builder() {
        struct DummyProvider;

        #[async_trait]
        impl Provider for DummyProvider {
            fn name(&self) -> &str {
                "dummy"
            }
            fn models(&self) -> Vec<drbot_providers::ModelInfo> {
                vec![]
            }
            async fn chat(
                &self,
                _messages: &[Message],
                _options: ChatOptions,
            ) -> drbot_core::Result<ChatResponse> {
                Err(drbot_core::Error::Provider("test".into()))
            }
            async fn stream(
                &self,
                _messages: &[Message],
                _options: ChatOptions,
            ) -> drbot_core::Result<Pin<Box<dyn Stream<Item = StreamEvent> + Send>>> {
                Err(drbot_core::Error::Provider("test".into()))
            }
        }

        let provider = RegisteredProvider::new(Arc::new(DummyProvider), "test")
            .with_cost_tier(2)
            .with_quality_tier(4)
            .with_max_context(200000);

        assert_eq!(provider.cost_tier, 2);
        assert_eq!(provider.quality_tier, 4);
        assert_eq!(provider.max_context, 200000);
    }
}
