//! Intent recognition pipeline for drbot.
//!
//! Classify and route user intents accurately.
//!
//! # Features
//!
//! - Multi-stage intent classification
//! - Entity extraction
//! - Intent routing
//! - Confidence scoring

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Intent result type.
pub type Result<T> = std::result::Result<T, IntentError>;

/// Intent errors.
#[derive(Debug, thiserror::Error)]
pub enum IntentError {
    #[error("Classification failed: {0}")]
    ClassificationFailed(String),
    #[error("No matching intent")]
    NoMatch,
    #[error("Intent not found: {0}")]
    IntentNotFound(String),
    #[error("Router not found: {0}")]
    RouterNotFound(String),
}

/// Classified intent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassifiedIntent {
    /// Intent ID.
    pub id: Uuid,
    /// Primary intent.
    pub intent: String,
    /// Secondary intents.
    pub secondary_intents: Vec<String>,
    /// Confidence score.
    pub confidence: f32,
    /// Extracted entities.
    pub entities: Vec<Entity>,
    /// Extracted slots.
    pub slots: HashMap<String, String>,
    /// Original query.
    pub query: String,
    /// Processed query (normalized).
    pub processed_query: String,
    /// Classified at.
    pub classified_at: DateTime<Utc>,
    /// Metadata.
    pub metadata: HashMap<String, serde_json::Value>,
}

impl ClassifiedIntent {
    /// Check if intent matches.
    pub fn matches(&self, intent: &str) -> bool {
        self.intent == intent || self.secondary_intents.contains(&intent.to_string())
    }

    /// Get entity by type.
    pub fn get_entity(&self, entity_type: &str) -> Option<&Entity> {
        self.entities.iter().find(|e| e.entity_type == entity_type)
    }

    /// Get slot value.
    pub fn get_slot(&self, slot_name: &str) -> Option<&str> {
        self.slots.get(slot_name).map(|s| s.as_str())
    }
}

/// An extracted entity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity {
    /// Entity type.
    pub entity_type: String,
    /// Entity value.
    pub value: String,
    /// Normalized value.
    pub normalized: Option<String>,
    /// Confidence.
    pub confidence: f32,
    /// Start position in query.
    pub start: usize,
    /// End position in query.
    pub end: usize,
}

impl Entity {
    /// Create a new entity.
    pub fn new(entity_type: &str, value: &str, start: usize, end: usize) -> Self {
        Self {
            entity_type: entity_type.to_string(),
            value: value.to_string(),
            normalized: None,
            confidence: 0.9,
            start,
            end,
        }
    }

    /// Set normalized value.
    pub fn with_normalized(mut self, normalized: &str) -> Self {
        self.normalized = Some(normalized.to_string());
        self
    }
}

/// Intent definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentDefinition {
    /// Intent name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Example phrases.
    pub examples: Vec<String>,
    /// Required slots.
    pub required_slots: Vec<SlotDefinition>,
    /// Optional slots.
    pub optional_slots: Vec<SlotDefinition>,
    /// Handler ID.
    pub handler: Option<String>,
    /// Priority.
    pub priority: i32,
}

impl IntentDefinition {
    /// Create a new intent definition.
    pub fn new(name: &str, description: &str) -> Self {
        Self {
            name: name.to_string(),
            description: description.to_string(),
            examples: Vec::new(),
            required_slots: Vec::new(),
            optional_slots: Vec::new(),
            handler: None,
            priority: 0,
        }
    }

    /// Add example phrase.
    pub fn with_example(mut self, example: &str) -> Self {
        self.examples.push(example.to_string());
        self
    }

    /// Add required slot.
    pub fn with_required_slot(mut self, slot: SlotDefinition) -> Self {
        self.required_slots.push(slot);
        self
    }
}

/// Slot definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlotDefinition {
    /// Slot name.
    pub name: String,
    /// Slot type.
    pub slot_type: SlotType,
    /// Description.
    pub description: String,
    /// Prompt to ask for missing slot.
    pub prompt: Option<String>,
}

/// Slot types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SlotType {
    Text,
    Number,
    Date,
    Time,
    DateTime,
    Duration,
    Email,
    Url,
    Phone,
    Person,
    Location,
    Organization,
    Custom,
}

/// Intent pipeline configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentConfig {
    /// Enable intent classification.
    pub enabled: bool,
    /// Minimum confidence threshold.
    pub min_confidence: f32,
    /// Enable multi-intent detection.
    pub multi_intent: bool,
    /// Maximum secondary intents.
    pub max_secondary: usize,
    /// Enable entity extraction.
    pub extract_entities: bool,
}

impl Default for IntentConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            min_confidence: 0.6,
            multi_intent: true,
            max_secondary: 3,
            extract_entities: true,
        }
    }
}

/// Intent classification context.
#[derive(Debug, Clone, Default)]
pub struct IntentContext {
    /// Conversation history.
    pub history: Vec<String>,
    /// User ID.
    pub user_id: Option<String>,
    /// Session ID.
    pub session_id: Option<String>,
    /// Custom metadata.
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Trait for intent classifiers.
#[async_trait]
pub trait IntentClassifier: Send + Sync {
    /// Classify intent from text.
    async fn classify(
        &self,
        text: &str,
        context: &IntentContext,
        definitions: &[IntentDefinition],
    ) -> Result<ClassifiedIntent>;
}

/// Trait for entity extractors.
#[async_trait]
pub trait EntityExtractor: Send + Sync {
    /// Extract entities from text.
    async fn extract(&self, text: &str, context: &IntentContext) -> Result<Vec<Entity>>;
}

/// Intent router.
#[derive(Debug, Clone)]
pub struct IntentRoute {
    /// Intent pattern.
    pub intent_pattern: String,
    /// Handler name.
    pub handler: String,
    /// Priority.
    pub priority: i32,
    /// Conditions.
    pub conditions: Vec<RouteCondition>,
}

/// Route condition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteCondition {
    /// Condition type.
    pub condition_type: String,
    /// Field.
    pub field: String,
    /// Value.
    pub value: String,
}

/// Intent pipeline.
pub struct IntentPipeline<C: IntentClassifier, E: EntityExtractor> {
    config: IntentConfig,
    classifier: C,
    extractor: E,
    definitions: Arc<RwLock<Vec<IntentDefinition>>>,
    routes: Arc<RwLock<Vec<IntentRoute>>>,
    history: Arc<RwLock<Vec<ClassifiedIntent>>>,
}

impl<C: IntentClassifier, E: EntityExtractor> IntentPipeline<C, E> {
    /// Create a new intent pipeline.
    pub fn new(config: IntentConfig, classifier: C, extractor: E) -> Self {
        Self {
            config,
            classifier,
            extractor,
            definitions: Arc::new(RwLock::new(Vec::new())),
            routes: Arc::new(RwLock::new(Vec::new())),
            history: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Register an intent definition.
    pub async fn register_intent(&self, definition: IntentDefinition) {
        self.definitions.write().await.push(definition);
    }

    /// Register a route.
    pub async fn register_route(&self, route: IntentRoute) {
        self.routes.write().await.push(route);
    }

    /// Process a query through the pipeline.
    pub async fn process(&self, query: &str, context: &IntentContext) -> Result<ClassifiedIntent> {
        if !self.config.enabled {
            return Err(IntentError::ClassificationFailed(
                "Pipeline disabled".to_string(),
            ));
        }

        // Get definitions
        let definitions = self.definitions.read().await;

        // Classify intent
        let mut classified = self
            .classifier
            .classify(query, context, &definitions)
            .await?;

        // Extract entities if enabled
        if self.config.extract_entities {
            let entities = self.extractor.extract(query, context).await?;
            classified.entities = entities;
        }

        // Apply confidence threshold
        if classified.confidence < self.config.min_confidence {
            return Err(IntentError::NoMatch);
        }

        // Record in history
        self.history.write().await.push(classified.clone());

        Ok(classified)
    }

    /// Route an intent to a handler.
    pub async fn route(&self, intent: &ClassifiedIntent) -> Option<String> {
        let routes = self.routes.read().await;

        let matching: Vec<_> = routes
            .iter()
            .filter(|r| intent.matches(&r.intent_pattern))
            .collect();

        matching
            .iter()
            .max_by_key(|r| r.priority)
            .map(|r| r.handler.clone())
    }

    /// Get intent history.
    pub async fn history(&self, limit: usize) -> Vec<ClassifiedIntent> {
        let history = self.history.read().await;
        history.iter().rev().take(limit).cloned().collect()
    }

    /// Get all registered intents.
    pub async fn list_intents(&self) -> Vec<IntentDefinition> {
        self.definitions.read().await.clone()
    }

    /// Get statistics.
    pub async fn stats(&self) -> IntentStats {
        let history = self.history.read().await;

        let mut by_intent: HashMap<String, u64> = HashMap::new();
        let mut total_confidence: f32 = 0.0;

        for intent in history.iter() {
            *by_intent.entry(intent.intent.clone()).or_insert(0) += 1;
            total_confidence += intent.confidence;
        }

        IntentStats {
            total_classified: history.len(),
            by_intent,
            avg_confidence: if !history.is_empty() {
                total_confidence / history.len() as f32
            } else {
                0.0
            },
        }
    }
}

/// Intent statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentStats {
    pub total_classified: usize,
    pub by_intent: HashMap<String, u64>,
    pub avg_confidence: f32,
}

/// Simple intent classifier for testing.
pub struct SimpleClassifier;

#[async_trait]
impl IntentClassifier for SimpleClassifier {
    async fn classify(
        &self,
        text: &str,
        _context: &IntentContext,
        definitions: &[IntentDefinition],
    ) -> Result<ClassifiedIntent> {
        let text_lower = text.to_lowercase();

        // Find best matching intent
        let mut best_match: Option<(&IntentDefinition, f32)> = None;

        for def in definitions {
            let mut score = 0.0;

            // Check examples
            for example in &def.examples {
                if text_lower.contains(&example.to_lowercase()) {
                    score += 0.5;
                }
            }

            // Check name
            if text_lower.contains(&def.name.to_lowercase()) {
                score += 0.3;
            }

            if score > 0.0 && (best_match.is_none() || score > best_match.unwrap().1) {
                best_match = Some((def, score));
            }
        }

        let (intent, confidence) = best_match
            .map(|(d, s)| (d.name.clone(), s.min(1.0)))
            .unwrap_or(("unknown".to_string(), 0.3));

        Ok(ClassifiedIntent {
            id: Uuid::new_v4(),
            intent,
            secondary_intents: Vec::new(),
            confidence,
            entities: Vec::new(),
            slots: HashMap::new(),
            query: text.to_string(),
            processed_query: text_lower,
            classified_at: Utc::now(),
            metadata: HashMap::new(),
        })
    }
}

/// Simple entity extractor for testing.
pub struct SimpleExtractor;

#[async_trait]
impl EntityExtractor for SimpleExtractor {
    async fn extract(&self, text: &str, _context: &IntentContext) -> Result<Vec<Entity>> {
        let mut entities = Vec::new();

        // Extract numbers
        for (i, word) in text.split_whitespace().enumerate() {
            if word.parse::<f64>().is_ok() {
                let start = text.find(word).unwrap_or(0);
                entities.push(Entity::new("number", word, start, start + word.len()));
            }
        }

        // Extract emails (simple pattern)
        for word in text.split_whitespace() {
            if word.contains('@') && word.contains('.') {
                let start = text.find(word).unwrap_or(0);
                entities.push(Entity::new("email", word, start, start + word.len()));
            }
        }

        Ok(entities)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_intent_classification() {
        let config = IntentConfig {
            min_confidence: 0.3,
            ..Default::default()
        };
        let pipeline = IntentPipeline::new(config, SimpleClassifier, SimpleExtractor);

        pipeline
            .register_intent(
                IntentDefinition::new("greeting", "User greets the assistant")
                    .with_example("hello")
                    .with_example("hi"),
            )
            .await;

        let result = pipeline
            .process("hello there", &IntentContext::default())
            .await
            .unwrap();
        assert_eq!(result.intent, "greeting");
    }

    #[tokio::test]
    async fn test_entity_extraction() {
        let config = IntentConfig {
            min_confidence: 0.2,
            ..Default::default()
        };
        let pipeline = IntentPipeline::new(config, SimpleClassifier, SimpleExtractor);

        pipeline
            .register_intent(IntentDefinition::new("test", "Test intent"))
            .await;

        let result = pipeline
            .process("send 42 to test@example.com", &IntentContext::default())
            .await
            .unwrap();

        assert!(result.entities.iter().any(|e| e.entity_type == "number"));
        assert!(result.entities.iter().any(|e| e.entity_type == "email"));
    }

    #[tokio::test]
    async fn test_routing() {
        let config = IntentConfig {
            min_confidence: 0.3,
            ..Default::default()
        };
        let pipeline = IntentPipeline::new(config, SimpleClassifier, SimpleExtractor);

        pipeline
            .register_intent(IntentDefinition::new("search", "Search intent").with_example("find"))
            .await;

        pipeline
            .register_route(IntentRoute {
                intent_pattern: "search".to_string(),
                handler: "search_handler".to_string(),
                priority: 10,
                conditions: Vec::new(),
            })
            .await;

        let result = pipeline
            .process("find something", &IntentContext::default())
            .await
            .unwrap();
        let handler = pipeline.route(&result).await;

        assert_eq!(handler, Some("search_handler".to_string()));
    }

    #[test]
    fn test_classified_intent() {
        let mut intent = ClassifiedIntent {
            id: Uuid::new_v4(),
            intent: "main".to_string(),
            secondary_intents: vec!["secondary".to_string()],
            confidence: 0.9,
            entities: vec![Entity::new("test", "value", 0, 5)],
            slots: HashMap::new(),
            query: "test".to_string(),
            processed_query: "test".to_string(),
            classified_at: Utc::now(),
            metadata: HashMap::new(),
        };

        intent.slots.insert("key".to_string(), "value".to_string());

        assert!(intent.matches("main"));
        assert!(intent.matches("secondary"));
        assert!(intent.get_entity("test").is_some());
        assert_eq!(intent.get_slot("key"), Some("value"));
    }
}
