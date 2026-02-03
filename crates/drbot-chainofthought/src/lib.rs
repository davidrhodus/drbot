//! Chain-of-thought UI for drbot.
//!
//! Visualize AI reasoning process in real-time.
//!
//! # Features
//!
//! - Step-by-step reasoning display
//! - Collapsible thought chains
//! - Real-time streaming updates
//! - Reasoning confidence indicators

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;

/// Chain-of-thought result type.
pub type Result<T> = std::result::Result<T, ChainError>;

/// Chain errors.
#[derive(Debug, thiserror::Error)]
pub enum ChainError {
    #[error("Chain not found: {0}")]
    ChainNotFound(Uuid),
    #[error("Step not found: {0}")]
    StepNotFound(Uuid),
    #[error("Chain already completed")]
    AlreadyCompleted,
    #[error("Parse error: {0}")]
    ParseError(String),
}

/// A thought chain representing a reasoning process.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThoughtChain {
    /// Chain ID.
    pub id: Uuid,
    /// Query that triggered this chain.
    pub query: String,
    /// Steps in the reasoning process.
    pub steps: Vec<ThoughtStep>,
    /// Current status.
    pub status: ChainStatus,
    /// Final conclusion.
    pub conclusion: Option<String>,
    /// Overall confidence.
    pub confidence: f32,
    /// Created at.
    pub created_at: DateTime<Utc>,
    /// Completed at.
    pub completed_at: Option<DateTime<Utc>>,
    /// Metadata.
    pub metadata: HashMap<String, serde_json::Value>,
}

impl ThoughtChain {
    /// Create a new thought chain.
    pub fn new(query: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            query: query.to_string(),
            steps: Vec::new(),
            status: ChainStatus::InProgress,
            conclusion: None,
            confidence: 0.0,
            created_at: Utc::now(),
            completed_at: None,
            metadata: HashMap::new(),
        }
    }

    /// Add a step to the chain.
    pub fn add_step(&mut self, step: ThoughtStep) {
        self.steps.push(step);
        self.update_confidence();
    }

    /// Complete the chain with a conclusion.
    pub fn complete(&mut self, conclusion: &str) {
        self.conclusion = Some(conclusion.to_string());
        self.status = ChainStatus::Completed;
        self.completed_at = Some(Utc::now());
        self.update_confidence();
    }

    /// Mark the chain as failed.
    pub fn fail(&mut self, reason: &str) {
        self.conclusion = Some(reason.to_string());
        self.status = ChainStatus::Failed;
        self.completed_at = Some(Utc::now());
    }

    fn update_confidence(&mut self) {
        if self.steps.is_empty() {
            self.confidence = 0.0;
        } else {
            self.confidence =
                self.steps.iter().map(|s| s.confidence).sum::<f32>() / self.steps.len() as f32;
        }
    }

    /// Get duration in milliseconds.
    pub fn duration_ms(&self) -> Option<u64> {
        self.completed_at
            .map(|end| (end - self.created_at).num_milliseconds() as u64)
    }
}

/// A single step in the thought chain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThoughtStep {
    /// Step ID.
    pub id: Uuid,
    /// Step type.
    pub step_type: StepType,
    /// Step content.
    pub content: String,
    /// Step reasoning.
    pub reasoning: Option<String>,
    /// Confidence in this step.
    pub confidence: f32,
    /// Whether this step is collapsed in UI.
    pub collapsed: bool,
    /// Child steps (for nested reasoning).
    pub children: Vec<ThoughtStep>,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
    /// Duration in ms.
    pub duration_ms: Option<u64>,
}

impl ThoughtStep {
    /// Create a new thought step.
    pub fn new(step_type: StepType, content: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            step_type,
            content: content.to_string(),
            reasoning: None,
            confidence: 0.8,
            collapsed: false,
            children: Vec::new(),
            timestamp: Utc::now(),
            duration_ms: None,
        }
    }

    /// Add reasoning to the step.
    pub fn with_reasoning(mut self, reasoning: &str) -> Self {
        self.reasoning = Some(reasoning.to_string());
        self
    }

    /// Set confidence.
    pub fn with_confidence(mut self, confidence: f32) -> Self {
        self.confidence = confidence;
        self
    }

    /// Add a child step.
    pub fn add_child(&mut self, child: ThoughtStep) {
        self.children.push(child);
    }
}

/// Types of reasoning steps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepType {
    /// Understanding the query.
    Understanding,
    /// Breaking down the problem.
    Decomposition,
    /// Gathering information.
    Research,
    /// Analyzing data.
    Analysis,
    /// Drawing a conclusion.
    Synthesis,
    /// Verifying the result.
    Verification,
    /// Making a decision.
    Decision,
    /// Taking an action.
    Action,
    /// Reflecting on the process.
    Reflection,
    /// Custom step type.
    Custom,
}

/// Chain status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChainStatus {
    /// Chain is in progress.
    InProgress,
    /// Chain completed successfully.
    Completed,
    /// Chain failed.
    Failed,
    /// Chain was cancelled.
    Cancelled,
}

/// Chain event for streaming updates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChainEvent {
    /// Chain started.
    Started { chain_id: Uuid, query: String },
    /// Step added.
    StepAdded { chain_id: Uuid, step: ThoughtStep },
    /// Step updated.
    StepUpdated {
        chain_id: Uuid,
        step_id: Uuid,
        content: String,
    },
    /// Chain completed.
    Completed { chain_id: Uuid, conclusion: String },
    /// Chain failed.
    Failed { chain_id: Uuid, reason: String },
}

/// Configuration for chain-of-thought display.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainConfig {
    /// Show reasoning by default.
    pub show_reasoning: bool,
    /// Collapse steps by default.
    pub default_collapsed: bool,
    /// Maximum visible steps.
    pub max_visible_steps: usize,
    /// Show confidence indicators.
    pub show_confidence: bool,
    /// Animate step additions.
    pub animate: bool,
    /// Stream updates.
    pub stream_updates: bool,
}

impl Default for ChainConfig {
    fn default() -> Self {
        Self {
            show_reasoning: true,
            default_collapsed: false,
            max_visible_steps: 10,
            show_confidence: true,
            animate: true,
            stream_updates: true,
        }
    }
}

/// Chain-of-thought manager.
pub struct ChainOfThought {
    config: ChainConfig,
    chains: Arc<RwLock<HashMap<Uuid, ThoughtChain>>>,
    event_tx: broadcast::Sender<ChainEvent>,
}

impl ChainOfThought {
    /// Create a new chain-of-thought manager.
    pub fn new(config: ChainConfig) -> Self {
        let (event_tx, _) = broadcast::channel(100);
        Self {
            config,
            chains: Arc::new(RwLock::new(HashMap::new())),
            event_tx,
        }
    }

    /// Start a new thought chain.
    pub async fn start_chain(&self, query: &str) -> ThoughtChain {
        let chain = ThoughtChain::new(query);
        let id = chain.id;

        self.chains.write().await.insert(id, chain.clone());

        let _ = self.event_tx.send(ChainEvent::Started {
            chain_id: id,
            query: query.to_string(),
        });

        chain
    }

    /// Add a step to a chain.
    pub async fn add_step(&self, chain_id: Uuid, step: ThoughtStep) -> Result<()> {
        let mut chains = self.chains.write().await;
        let chain = chains
            .get_mut(&chain_id)
            .ok_or(ChainError::ChainNotFound(chain_id))?;

        if chain.status != ChainStatus::InProgress {
            return Err(ChainError::AlreadyCompleted);
        }

        let step_clone = step.clone();
        chain.add_step(step);

        let _ = self.event_tx.send(ChainEvent::StepAdded {
            chain_id,
            step: step_clone,
        });

        Ok(())
    }

    /// Complete a chain.
    pub async fn complete_chain(&self, chain_id: Uuid, conclusion: &str) -> Result<ThoughtChain> {
        let mut chains = self.chains.write().await;
        let chain = chains
            .get_mut(&chain_id)
            .ok_or(ChainError::ChainNotFound(chain_id))?;

        chain.complete(conclusion);

        let _ = self.event_tx.send(ChainEvent::Completed {
            chain_id,
            conclusion: conclusion.to_string(),
        });

        Ok(chain.clone())
    }

    /// Fail a chain.
    pub async fn fail_chain(&self, chain_id: Uuid, reason: &str) -> Result<()> {
        let mut chains = self.chains.write().await;
        let chain = chains
            .get_mut(&chain_id)
            .ok_or(ChainError::ChainNotFound(chain_id))?;

        chain.fail(reason);

        let _ = self.event_tx.send(ChainEvent::Failed {
            chain_id,
            reason: reason.to_string(),
        });

        Ok(())
    }

    /// Get a chain by ID.
    pub async fn get_chain(&self, chain_id: Uuid) -> Option<ThoughtChain> {
        self.chains.read().await.get(&chain_id).cloned()
    }

    /// Subscribe to chain events.
    pub fn subscribe(&self) -> broadcast::Receiver<ChainEvent> {
        self.event_tx.subscribe()
    }

    /// List all chains.
    pub async fn list_chains(&self) -> Vec<ThoughtChain> {
        self.chains.read().await.values().cloned().collect()
    }

    /// Get recent chains.
    pub async fn recent_chains(&self, limit: usize) -> Vec<ThoughtChain> {
        let mut chains: Vec<_> = self.chains.read().await.values().cloned().collect();
        chains.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        chains.truncate(limit);
        chains
    }

    /// Parse chain from streaming text.
    pub fn parse_streaming(&self, text: &str) -> Vec<ThoughtStep> {
        let mut steps = Vec::new();

        // Parse thinking markers
        for line in text.lines() {
            let line = line.trim();

            if line.starts_with("Understanding:") || line.starts_with("Let me understand") {
                steps.push(ThoughtStep::new(StepType::Understanding, line));
            } else if line.starts_with("Breaking down") || line.starts_with("First,") {
                steps.push(ThoughtStep::new(StepType::Decomposition, line));
            } else if line.starts_with("Looking at") || line.starts_with("Checking") {
                steps.push(ThoughtStep::new(StepType::Research, line));
            } else if line.starts_with("Analyzing") || line.starts_with("Considering") {
                steps.push(ThoughtStep::new(StepType::Analysis, line));
            } else if line.starts_with("Therefore") || line.starts_with("So,") {
                steps.push(ThoughtStep::new(StepType::Synthesis, line));
            } else if line.starts_with("Verifying") || line.starts_with("Let me check") {
                steps.push(ThoughtStep::new(StepType::Verification, line));
            }
        }

        steps
    }

    /// Render chain to markdown.
    pub fn render_markdown(&self, chain: &ThoughtChain) -> String {
        let mut output = String::new();

        output.push_str(&format!("## Thinking about: {}\n\n", chain.query));

        for (i, step) in chain.steps.iter().enumerate() {
            let icon = match step.step_type {
                StepType::Understanding => "🤔",
                StepType::Decomposition => "📋",
                StepType::Research => "🔍",
                StepType::Analysis => "📊",
                StepType::Synthesis => "💡",
                StepType::Verification => "✓",
                StepType::Decision => "⚖️",
                StepType::Action => "🎯",
                StepType::Reflection => "🔄",
                StepType::Custom => "•",
            };

            output.push_str(&format!("{}. {} {}\n", i + 1, icon, step.content));

            if self.config.show_reasoning {
                if let Some(reasoning) = &step.reasoning {
                    output.push_str(&format!("   > {}\n", reasoning));
                }
            }

            if self.config.show_confidence {
                output.push_str(&format!("   Confidence: {:.0}%\n", step.confidence * 100.0));
            }

            output.push('\n');
        }

        if let Some(conclusion) = &chain.conclusion {
            output.push_str(&format!("**Conclusion:** {}\n", conclusion));
        }

        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_thought_chain() {
        let cot = ChainOfThought::new(ChainConfig::default());

        let chain = cot.start_chain("What is 2 + 2?").await;
        let chain_id = chain.id;

        cot.add_step(
            chain_id,
            ThoughtStep::new(
                StepType::Understanding,
                "Understanding the addition problem",
            ),
        )
        .await
        .unwrap();
        cot.add_step(
            chain_id,
            ThoughtStep::new(StepType::Analysis, "Adding 2 and 2"),
        )
        .await
        .unwrap();

        let completed = cot
            .complete_chain(chain_id, "The answer is 4")
            .await
            .unwrap();

        assert_eq!(completed.steps.len(), 2);
        assert!(completed.conclusion.is_some());
        assert_eq!(completed.status, ChainStatus::Completed);
    }

    #[tokio::test]
    async fn test_chain_events() {
        let cot = ChainOfThought::new(ChainConfig::default());
        let mut rx = cot.subscribe();

        let chain = cot.start_chain("Test").await;

        if let Ok(event) = rx.try_recv() {
            match event {
                ChainEvent::Started { query, .. } => assert_eq!(query, "Test"),
                _ => panic!("Expected Started event"),
            }
        }

        cot.add_step(chain.id, ThoughtStep::new(StepType::Analysis, "Step 1"))
            .await
            .unwrap();

        if let Ok(event) = rx.try_recv() {
            match event {
                ChainEvent::StepAdded { step, .. } => assert_eq!(step.content, "Step 1"),
                _ => panic!("Expected StepAdded event"),
            }
        }
    }

    #[test]
    fn test_thought_step() {
        let step = ThoughtStep::new(StepType::Analysis, "Testing")
            .with_reasoning("Because we need to verify")
            .with_confidence(0.95);

        assert_eq!(step.content, "Testing");
        assert!(step.reasoning.is_some());
        assert_eq!(step.confidence, 0.95);
    }

    #[test]
    fn test_render_markdown() {
        let cot = ChainOfThought::new(ChainConfig::default());
        let mut chain = ThoughtChain::new("What is X?");
        chain.add_step(ThoughtStep::new(StepType::Understanding, "Understanding X"));
        chain.complete("X is Y");

        let md = cot.render_markdown(&chain);
        assert!(md.contains("What is X?"));
        assert!(md.contains("Understanding X"));
        assert!(md.contains("X is Y"));
    }
}
