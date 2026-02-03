//! Tool discovery for drbot.
//!
//! AI learns to use new tools automatically.
//!
//! # Features
//!
//! - Automatic tool detection
//! - Schema learning
//! - Usage pattern learning
//! - Tool recommendations

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Tool discovery result type.
pub type Result<T> = std::result::Result<T, DiscoveryError>;

/// Discovery errors.
#[derive(Debug, thiserror::Error)]
pub enum DiscoveryError {
    #[error("Tool not found: {0}")]
    ToolNotFound(String),
    #[error("Schema parse failed: {0}")]
    SchemaParseFailed(String),
    #[error("Learning failed: {0}")]
    LearningFailed(String),
    #[error("Incompatible tool: {0}")]
    IncompatibleTool(String),
}

/// Discovered tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredTool {
    /// Tool ID.
    pub id: Uuid,
    /// Tool name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Input schema.
    pub input_schema: ToolSchema,
    /// Output schema.
    pub output_schema: Option<ToolSchema>,
    /// Discovered capabilities.
    pub capabilities: Vec<String>,
    /// Usage examples.
    pub examples: Vec<ToolExample>,
    /// Discovery source.
    pub source: DiscoverySource,
    /// Confidence score.
    pub confidence: f32,
    /// Discovered at.
    pub discovered_at: DateTime<Utc>,
    /// Last used.
    pub last_used: Option<DateTime<Utc>>,
    /// Usage count.
    pub usage_count: u64,
    /// Success rate.
    pub success_rate: f32,
}

impl DiscoveredTool {
    /// Create a new discovered tool.
    pub fn new(name: &str, description: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.to_string(),
            description: description.to_string(),
            input_schema: ToolSchema::default(),
            output_schema: None,
            capabilities: Vec::new(),
            examples: Vec::new(),
            source: DiscoverySource::Manual,
            confidence: 0.5,
            discovered_at: Utc::now(),
            last_used: None,
            usage_count: 0,
            success_rate: 0.0,
        }
    }

    /// Record a usage.
    pub fn record_usage(&mut self, success: bool) {
        self.usage_count += 1;
        self.last_used = Some(Utc::now());

        // Update success rate
        let successes =
            (self.success_rate * (self.usage_count - 1) as f32) + if success { 1.0 } else { 0.0 };
        self.success_rate = successes / self.usage_count as f32;
    }
}

/// Tool schema.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolSchema {
    /// Schema type.
    pub schema_type: String,
    /// Parameters.
    pub parameters: Vec<SchemaParameter>,
    /// Required parameters.
    pub required: Vec<String>,
    /// Additional properties allowed.
    pub additional_properties: bool,
}

/// Schema parameter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaParameter {
    /// Parameter name.
    pub name: String,
    /// Parameter type.
    pub param_type: ParamType,
    /// Description.
    pub description: String,
    /// Default value.
    pub default: Option<serde_json::Value>,
    /// Enum values (if applicable).
    pub enum_values: Option<Vec<String>>,
}

/// Parameter types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParamType {
    String,
    Number,
    Integer,
    Boolean,
    Array,
    Object,
}

/// Tool example.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolExample {
    /// Example description.
    pub description: String,
    /// Input.
    pub input: serde_json::Value,
    /// Expected output.
    pub output: Option<serde_json::Value>,
    /// Natural language query that triggers this.
    pub trigger_query: Option<String>,
}

/// Discovery source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiscoverySource {
    Manual,
    MCP,
    OpenAPI,
    Inferred,
    UserDefined,
}

/// Tool recommendation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolRecommendation {
    /// Tool.
    pub tool: DiscoveredTool,
    /// Relevance score.
    pub relevance: f32,
    /// Reason for recommendation.
    pub reason: String,
    /// Suggested parameters.
    pub suggested_params: Option<serde_json::Value>,
}

/// Discovery configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryConfig {
    /// Enable auto-discovery.
    pub auto_discover: bool,
    /// Learn from usage.
    pub learn_from_usage: bool,
    /// Minimum confidence threshold.
    pub min_confidence: f32,
    /// Maximum tools to recommend.
    pub max_recommendations: usize,
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self {
            auto_discover: true,
            learn_from_usage: true,
            min_confidence: 0.6,
            max_recommendations: 5,
        }
    }
}

/// Trait for tool sources.
#[async_trait]
pub trait ToolSource: Send + Sync {
    /// Discover tools from this source.
    async fn discover(&self) -> Result<Vec<DiscoveredTool>>;

    /// Get source name.
    fn name(&self) -> &str;
}

/// Tool discovery engine.
pub struct ToolDiscovery {
    config: DiscoveryConfig,
    tools: Arc<RwLock<HashMap<Uuid, DiscoveredTool>>>,
    sources: Arc<RwLock<Vec<Box<dyn ToolSource>>>>,
    keyword_index: Arc<RwLock<HashMap<String, Vec<Uuid>>>>,
}

impl ToolDiscovery {
    /// Create a new tool discovery engine.
    pub fn new(config: DiscoveryConfig) -> Self {
        Self {
            config,
            tools: Arc::new(RwLock::new(HashMap::new())),
            sources: Arc::new(RwLock::new(Vec::new())),
            keyword_index: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a tool source.
    pub async fn register_source(&self, source: Box<dyn ToolSource>) {
        self.sources.write().await.push(source);
    }

    /// Discover tools from all sources.
    pub async fn discover_all(&self) -> Result<Vec<DiscoveredTool>> {
        let sources = self.sources.read().await;
        let mut all_tools = Vec::new();

        for source in sources.iter() {
            match source.discover().await {
                Ok(tools) => {
                    for tool in tools {
                        self.register_tool(tool.clone()).await;
                        all_tools.push(tool);
                    }
                }
                Err(e) => {
                    tracing::warn!("Discovery failed for {}: {}", source.name(), e);
                }
            }
        }

        Ok(all_tools)
    }

    /// Register a tool.
    pub async fn register_tool(&self, tool: DiscoveredTool) {
        let id = tool.id;

        // Index keywords
        let keywords = self.extract_keywords(&tool);
        let mut index = self.keyword_index.write().await;
        for keyword in keywords {
            index.entry(keyword).or_default().push(id);
        }

        // Store tool
        self.tools.write().await.insert(id, tool);
    }

    fn extract_keywords(&self, tool: &DiscoveredTool) -> Vec<String> {
        let mut keywords = Vec::new();

        // From name
        keywords.extend(
            tool.name
                .to_lowercase()
                .split(|c: char| !c.is_alphanumeric())
                .filter(|s| !s.is_empty())
                .map(String::from),
        );

        // From description
        keywords.extend(
            tool.description
                .to_lowercase()
                .split(|c: char| !c.is_alphanumeric())
                .filter(|s| s.len() > 3)
                .map(String::from),
        );

        // From capabilities
        for cap in &tool.capabilities {
            keywords.extend(
                cap.to_lowercase()
                    .split(|c: char| !c.is_alphanumeric())
                    .filter(|s| !s.is_empty())
                    .map(String::from),
            );
        }

        keywords
    }

    /// Get tool by ID.
    pub async fn get_tool(&self, id: Uuid) -> Option<DiscoveredTool> {
        self.tools.read().await.get(&id).cloned()
    }

    /// Get tool by name.
    pub async fn get_by_name(&self, name: &str) -> Option<DiscoveredTool> {
        self.tools
            .read()
            .await
            .values()
            .find(|t| t.name == name)
            .cloned()
    }

    /// List all tools.
    pub async fn list_tools(&self) -> Vec<DiscoveredTool> {
        self.tools.read().await.values().cloned().collect()
    }

    /// Recommend tools for a query.
    pub async fn recommend(&self, query: &str) -> Vec<ToolRecommendation> {
        let keywords: Vec<_> = query
            .to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|s| s.len() > 2)
            .map(String::from)
            .collect();

        let index = self.keyword_index.read().await;
        let tools = self.tools.read().await;

        let mut scores: HashMap<Uuid, (f32, Vec<String>)> = HashMap::new();

        for keyword in &keywords {
            if let Some(tool_ids) = index.get(keyword) {
                for id in tool_ids {
                    let entry = scores.entry(*id).or_insert((0.0, Vec::new()));
                    entry.0 += 1.0;
                    entry.1.push(keyword.clone());
                }
            }
        }

        let mut recommendations: Vec<_> = scores
            .into_iter()
            .filter_map(|(id, (score, matched_keywords))| {
                tools.get(&id).map(|tool| {
                    let relevance = score / keywords.len().max(1) as f32;
                    ToolRecommendation {
                        tool: tool.clone(),
                        relevance,
                        reason: format!("Matched keywords: {}", matched_keywords.join(", ")),
                        suggested_params: None,
                    }
                })
            })
            .filter(|r| r.relevance >= self.config.min_confidence)
            .collect();

        recommendations.sort_by(|a, b| {
            b.relevance
                .partial_cmp(&a.relevance)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        recommendations.truncate(self.config.max_recommendations);

        recommendations
    }

    /// Record tool usage.
    pub async fn record_usage(&self, tool_id: Uuid, success: bool) {
        if let Some(tool) = self.tools.write().await.get_mut(&tool_id) {
            tool.record_usage(success);
        }
    }

    /// Learn from observation.
    pub async fn learn_from_observation(
        &self,
        tool_name: &str,
        input: serde_json::Value,
        output: serde_json::Value,
        description: &str,
    ) -> Result<DiscoveredTool> {
        let mut tool = if let Some(existing) = self.get_by_name(tool_name).await {
            existing
        } else {
            DiscoveredTool::new(tool_name, description)
        };

        // Learn from input structure
        if let serde_json::Value::Object(map) = &input {
            for (key, value) in map {
                let param_type = match value {
                    serde_json::Value::String(_) => ParamType::String,
                    serde_json::Value::Number(_) => ParamType::Number,
                    serde_json::Value::Bool(_) => ParamType::Boolean,
                    serde_json::Value::Array(_) => ParamType::Array,
                    serde_json::Value::Object(_) => ParamType::Object,
                    _ => ParamType::String,
                };

                if !tool.input_schema.parameters.iter().any(|p| p.name == *key) {
                    tool.input_schema.parameters.push(SchemaParameter {
                        name: key.clone(),
                        param_type,
                        description: String::new(),
                        default: None,
                        enum_values: None,
                    });
                }
            }
        }

        // Add example
        tool.examples.push(ToolExample {
            description: description.to_string(),
            input,
            output: Some(output),
            trigger_query: None,
        });

        tool.source = DiscoverySource::Inferred;
        tool.confidence = (tool.confidence + 0.1).min(1.0);

        self.register_tool(tool.clone()).await;

        Ok(tool)
    }

    /// Get statistics.
    pub async fn stats(&self) -> DiscoveryStats {
        let tools = self.tools.read().await;

        let total = tools.len();
        let by_source: HashMap<_, _> = tools.values().fold(HashMap::new(), |mut acc, t| {
            *acc.entry(t.source).or_insert(0usize) += 1;
            acc
        });

        let avg_confidence: f32 = if total > 0 {
            tools.values().map(|t| t.confidence).sum::<f32>() / total as f32
        } else {
            0.0
        };

        let total_usage: u64 = tools.values().map(|t| t.usage_count).sum();

        DiscoveryStats {
            total_tools: total,
            by_source,
            avg_confidence,
            total_usage,
        }
    }
}

/// Discovery statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryStats {
    /// Total tools.
    pub total_tools: usize,
    /// Tools by source.
    pub by_source: HashMap<DiscoverySource, usize>,
    /// Average confidence.
    pub avg_confidence: f32,
    /// Total usage.
    pub total_usage: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_tool_discovery() {
        let discovery = ToolDiscovery::new(DiscoveryConfig::default());

        let tool = DiscoveredTool::new("file_search", "Search for files in the filesystem");
        discovery.register_tool(tool).await;

        let tools = discovery.list_tools().await;
        assert_eq!(tools.len(), 1);
    }

    #[tokio::test]
    async fn test_tool_recommendation() {
        let discovery = ToolDiscovery::new(DiscoveryConfig::default());

        let mut tool = DiscoveredTool::new("file_search", "Search for files in the filesystem");
        tool.capabilities = vec!["find files".to_string(), "search directory".to_string()];
        discovery.register_tool(tool).await;

        let recs = discovery.recommend("find a file in my directory").await;
        assert!(!recs.is_empty());
    }

    #[tokio::test]
    async fn test_learn_from_observation() {
        let discovery = ToolDiscovery::new(DiscoveryConfig::default());

        let input = serde_json::json!({
            "query": "test",
            "limit": 10
        });

        let output = serde_json::json!({
            "results": []
        });

        let tool = discovery
            .learn_from_observation("search", input, output, "Search for items")
            .await
            .unwrap();

        assert_eq!(tool.input_schema.parameters.len(), 2);
        assert!(!tool.examples.is_empty());
    }

    #[test]
    fn test_tool_usage() {
        let mut tool = DiscoveredTool::new("test", "Test tool");

        tool.record_usage(true);
        tool.record_usage(true);
        tool.record_usage(false);

        assert_eq!(tool.usage_count, 3);
        assert!((tool.success_rate - 0.666).abs() < 0.01);
    }
}
