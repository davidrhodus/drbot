//! Universal translator and cross-domain mapping.
//!
//! This crate provides bridging capabilities:
//! - Translate between different formats and schemas
//! - Map concepts across domains
//! - Adapt communication for different platforms
//! - Unify disparate data sources

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Bridge errors.
#[derive(Debug, Error)]
pub enum BridgeError {
    #[error("Translation failed: {0}")]
    TranslationFailed(String),

    #[error("Mapping not found: {0}")]
    MappingNotFound(String),

    #[error("Schema not found: {0}")]
    SchemaNotFound(String),

    #[error("Incompatible types: {0}")]
    IncompatibleTypes(String),

    #[error("Provider error: {0}")]
    ProviderError(String),
}

/// Result type for bridge operations.
pub type Result<T> = std::result::Result<T, BridgeError>;

/// A domain for translation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Domain {
    /// Domain identifier.
    pub id: String,
    /// Domain name.
    pub name: String,
    /// Domain description.
    pub description: String,
    /// Domain schema.
    pub schema: DomainSchema,
    /// Terminology.
    pub terminology: HashMap<String, String>,
}

/// Schema for a domain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainSchema {
    /// Schema name.
    pub name: String,
    /// Fields/properties.
    pub fields: Vec<SchemaField>,
    /// Relationships.
    pub relationships: Vec<SchemaRelationship>,
}

/// A field in a schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaField {
    /// Field name.
    pub name: String,
    /// Field type.
    pub field_type: FieldType,
    /// Required.
    pub required: bool,
    /// Description.
    pub description: Option<String>,
}

/// Field types.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum FieldType {
    String,
    Number,
    Boolean,
    Date,
    DateTime,
    Array(Box<FieldType>),
    Object(String),
    Any,
}

/// A relationship in a schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaRelationship {
    /// Relationship name.
    pub name: String,
    /// Source field.
    pub from: String,
    /// Target domain.
    pub to_domain: String,
    /// Target field.
    pub to_field: String,
    /// Cardinality.
    pub cardinality: Cardinality,
}

/// Relationship cardinality.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum Cardinality {
    OneToOne,
    OneToMany,
    ManyToOne,
    ManyToMany,
}

/// A mapping between domains.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainMapping {
    /// Mapping identifier.
    pub id: String,
    /// Source domain.
    pub source_domain: String,
    /// Target domain.
    pub target_domain: String,
    /// Field mappings.
    pub field_mappings: Vec<FieldMapping>,
    /// Transformation rules.
    pub transformations: Vec<Transformation>,
    /// Bidirectional.
    pub bidirectional: bool,
}

/// A field mapping.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldMapping {
    /// Source field.
    pub source: String,
    /// Target field.
    pub target: String,
    /// Transformation to apply.
    pub transformation: Option<String>,
}

/// A transformation rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transformation {
    /// Transformation ID.
    pub id: String,
    /// Transformation type.
    pub transform_type: TransformType,
    /// Parameters.
    pub params: HashMap<String, serde_json::Value>,
}

/// Types of transformations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TransformType {
    /// Direct copy.
    Copy,
    /// Rename field.
    Rename,
    /// Convert type.
    Convert { from: FieldType, to: FieldType },
    /// Format string.
    Format { template: String },
    /// Split into multiple.
    Split { delimiter: String },
    /// Combine multiple.
    Combine {
        delimiter: String,
        sources: Vec<String>,
    },
    /// Lookup/replace.
    Lookup { table: HashMap<String, String> },
    /// Custom function.
    Custom { function: String },
}

/// A translation request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslationRequest {
    /// Source domain.
    pub source_domain: String,
    /// Target domain.
    pub target_domain: String,
    /// Data to translate.
    pub data: serde_json::Value,
    /// Options.
    pub options: TranslationOptions,
}

/// Translation options.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslationOptions {
    /// Strict mode (fail on unknown fields).
    pub strict: bool,
    /// Preserve unknown fields.
    pub preserve_unknown: bool,
    /// Validate output.
    pub validate: bool,
}

impl Default for TranslationOptions {
    fn default() -> Self {
        Self {
            strict: false,
            preserve_unknown: true,
            validate: true,
        }
    }
}

/// Translation result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslationResult {
    /// Translated data.
    pub data: serde_json::Value,
    /// Fields that were mapped.
    pub mapped_fields: Vec<String>,
    /// Fields that were dropped.
    pub dropped_fields: Vec<String>,
    /// Fields that were preserved.
    pub preserved_fields: Vec<String>,
    /// Warnings.
    pub warnings: Vec<String>,
}

/// Provider for bridge operations.
#[async_trait]
pub trait BridgeProvider: Send + Sync {
    /// Infer mapping between domains.
    async fn infer_mapping(&self, source: &Domain, target: &Domain) -> Result<DomainMapping>;

    /// Translate data using a mapping.
    async fn translate(
        &self,
        data: &serde_json::Value,
        mapping: &DomainMapping,
        options: &TranslationOptions,
    ) -> Result<TranslationResult>;

    /// Translate terminology.
    async fn translate_term(
        &self,
        term: &str,
        source_domain: &str,
        target_domain: &str,
    ) -> Result<String>;
}

/// The bridge engine.
pub struct BridgeEngine {
    /// Provider for operations.
    provider: Arc<dyn BridgeProvider>,
    /// Registered domains.
    domains: Arc<RwLock<HashMap<String, Domain>>>,
    /// Registered mappings.
    mappings: Arc<RwLock<HashMap<String, DomainMapping>>>,
}

impl BridgeEngine {
    /// Create a new bridge engine.
    pub fn new(provider: Arc<dyn BridgeProvider>) -> Self {
        Self {
            provider,
            domains: Arc::new(RwLock::new(HashMap::new())),
            mappings: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a domain.
    pub async fn register_domain(&self, domain: Domain) -> Result<String> {
        let name = domain.name.clone();
        let mut domains = self.domains.write().await;
        domains.insert(name.clone(), domain);
        Ok(name)
    }

    /// Register a mapping.
    pub async fn register_mapping(&self, mapping: DomainMapping) -> Result<String> {
        let id = mapping.id.clone();
        let mut mappings = self.mappings.write().await;
        mappings.insert(id.clone(), mapping);
        Ok(id)
    }

    /// Translate data between domains.
    pub async fn translate(&self, request: TranslationRequest) -> Result<TranslationResult> {
        // Get or create mapping
        let mapping_key = format!("{}:{}", request.source_domain, request.target_domain);

        let mapping = {
            let mappings = self.mappings.read().await;
            mappings.get(&mapping_key).cloned()
        };

        let mapping = match mapping {
            Some(m) => m,
            None => {
                // Try to infer mapping
                let domains = self.domains.read().await;
                let source = domains
                    .get(&request.source_domain)
                    .ok_or_else(|| BridgeError::SchemaNotFound(request.source_domain.clone()))?
                    .clone();
                let target = domains
                    .get(&request.target_domain)
                    .ok_or_else(|| BridgeError::SchemaNotFound(request.target_domain.clone()))?
                    .clone();
                drop(domains);

                let inferred = self.provider.infer_mapping(&source, &target).await?;

                // Cache the mapping
                self.register_mapping(inferred.clone()).await?;

                inferred
            }
        };

        self.provider
            .translate(&request.data, &mapping, &request.options)
            .await
    }

    /// Translate a term between domains.
    pub async fn translate_term(
        &self,
        term: &str,
        source_domain: &str,
        target_domain: &str,
    ) -> Result<String> {
        // Check if we have direct terminology mapping
        let domains = self.domains.read().await;

        if let Some(target) = domains.get(target_domain) {
            if let Some(translated) = target.terminology.get(term) {
                return Ok(translated.clone());
            }
        }
        drop(domains);

        // Use provider for intelligent translation
        self.provider
            .translate_term(term, source_domain, target_domain)
            .await
    }

    /// Get all domains.
    pub async fn list_domains(&self) -> Vec<Domain> {
        let domains = self.domains.read().await;
        domains.values().cloned().collect()
    }

    /// Get all mappings.
    pub async fn list_mappings(&self) -> Vec<DomainMapping> {
        let mappings = self.mappings.read().await;
        mappings.values().cloned().collect()
    }

    /// Get mapping between two domains.
    pub async fn get_mapping(&self, source: &str, target: &str) -> Option<DomainMapping> {
        let key = format!("{}:{}", source, target);
        let mappings = self.mappings.read().await;
        mappings.get(&key).cloned()
    }
}

/// Builder for domains.
pub struct DomainBuilder {
    domain: Domain,
}

impl DomainBuilder {
    /// Create a new domain builder.
    pub fn new(name: &str) -> Self {
        Self {
            domain: Domain {
                id: Uuid::new_v4().to_string(),
                name: name.to_string(),
                description: String::new(),
                schema: DomainSchema {
                    name: name.to_string(),
                    fields: Vec::new(),
                    relationships: Vec::new(),
                },
                terminology: HashMap::new(),
            },
        }
    }

    /// Set description.
    pub fn description(mut self, desc: &str) -> Self {
        self.domain.description = desc.to_string();
        self
    }

    /// Add a field.
    pub fn field(mut self, name: &str, field_type: FieldType, required: bool) -> Self {
        self.domain.schema.fields.push(SchemaField {
            name: name.to_string(),
            field_type,
            required,
            description: None,
        });
        self
    }

    /// Add terminology.
    pub fn term(mut self, term: &str, definition: &str) -> Self {
        self.domain
            .terminology
            .insert(term.to_string(), definition.to_string());
        self
    }

    /// Build the domain.
    pub fn build(self) -> Domain {
        self.domain
    }
}

/// Builder for mappings.
pub struct MappingBuilder {
    mapping: DomainMapping,
}

impl MappingBuilder {
    /// Create a new mapping builder.
    pub fn new(source: &str, target: &str) -> Self {
        Self {
            mapping: DomainMapping {
                id: format!("{}:{}", source, target),
                source_domain: source.to_string(),
                target_domain: target.to_string(),
                field_mappings: Vec::new(),
                transformations: Vec::new(),
                bidirectional: false,
            },
        }
    }

    /// Map a field.
    pub fn map_field(mut self, source: &str, target: &str) -> Self {
        self.mapping.field_mappings.push(FieldMapping {
            source: source.to_string(),
            target: target.to_string(),
            transformation: None,
        });
        self
    }

    /// Map a field with transformation.
    pub fn map_field_transform(mut self, source: &str, target: &str, transform: &str) -> Self {
        self.mapping.field_mappings.push(FieldMapping {
            source: source.to_string(),
            target: target.to_string(),
            transformation: Some(transform.to_string()),
        });
        self
    }

    /// Set bidirectional.
    pub fn bidirectional(mut self) -> Self {
        self.mapping.bidirectional = true;
        self
    }

    /// Build the mapping.
    pub fn build(self) -> DomainMapping {
        self.mapping
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockProvider;

    #[async_trait]
    impl BridgeProvider for MockProvider {
        async fn infer_mapping(&self, source: &Domain, target: &Domain) -> Result<DomainMapping> {
            // Simple field name matching
            let field_mappings: Vec<_> = source
                .schema
                .fields
                .iter()
                .filter_map(|sf| {
                    target
                        .schema
                        .fields
                        .iter()
                        .find(|tf| tf.name == sf.name)
                        .map(|tf| FieldMapping {
                            source: sf.name.clone(),
                            target: tf.name.clone(),
                            transformation: None,
                        })
                })
                .collect();

            Ok(DomainMapping {
                id: format!("{}:{}", source.id, target.id),
                source_domain: source.id.clone(),
                target_domain: target.id.clone(),
                field_mappings,
                transformations: Vec::new(),
                bidirectional: false,
            })
        }

        async fn translate(
            &self,
            data: &serde_json::Value,
            mapping: &DomainMapping,
            _options: &TranslationOptions,
        ) -> Result<TranslationResult> {
            let mut result = serde_json::Map::new();
            let mut mapped = Vec::new();
            let mut dropped = Vec::new();

            if let Some(obj) = data.as_object() {
                for (key, value) in obj {
                    if let Some(fm) = mapping.field_mappings.iter().find(|m| m.source == *key) {
                        result.insert(fm.target.clone(), value.clone());
                        mapped.push(key.clone());
                    } else {
                        dropped.push(key.clone());
                    }
                }
            }

            Ok(TranslationResult {
                data: serde_json::Value::Object(result),
                mapped_fields: mapped,
                dropped_fields: dropped,
                preserved_fields: Vec::new(),
                warnings: Vec::new(),
            })
        }

        async fn translate_term(&self, term: &str, _source: &str, _target: &str) -> Result<String> {
            Ok(term.to_string())
        }
    }

    #[tokio::test]
    async fn test_register_domain() {
        let provider = Arc::new(MockProvider);
        let engine = BridgeEngine::new(provider);

        let domain = DomainBuilder::new("CRM")
            .description("Customer Relationship Management")
            .field("customer_id", FieldType::String, true)
            .field("name", FieldType::String, true)
            .build();

        let id = engine.register_domain(domain).await.unwrap();
        assert!(!id.is_empty());
    }

    #[tokio::test]
    async fn test_translate() {
        let provider = Arc::new(MockProvider);
        let engine = BridgeEngine::new(provider);

        // Register domains
        let source = DomainBuilder::new("source")
            .field("id", FieldType::String, true)
            .field("name", FieldType::String, true)
            .build();

        let target = DomainBuilder::new("target")
            .field("id", FieldType::String, true)
            .field("name", FieldType::String, true)
            .field("extra", FieldType::String, false)
            .build();

        engine.register_domain(source).await.unwrap();
        engine.register_domain(target).await.unwrap();

        // Translate
        let request = TranslationRequest {
            source_domain: "source".to_string(),
            target_domain: "target".to_string(),
            data: serde_json::json!({
                "id": "123",
                "name": "Test"
            }),
            options: TranslationOptions::default(),
        };

        let result = engine.translate(request).await.unwrap();

        assert_eq!(result.data["id"], "123");
        assert_eq!(result.data["name"], "Test");
    }

    #[tokio::test]
    async fn test_manual_mapping() {
        let provider = Arc::new(MockProvider);
        let engine = BridgeEngine::new(provider);

        let mapping = MappingBuilder::new("api", "db")
            .map_field("userId", "user_id")
            .map_field("userName", "name")
            .build();

        let id = engine.register_mapping(mapping).await.unwrap();
        assert_eq!(id, "api:db");
    }

    #[test]
    fn test_domain_builder() {
        let domain = DomainBuilder::new("Test")
            .description("Test domain")
            .field("id", FieldType::Number, true)
            .field("tags", FieldType::Array(Box::new(FieldType::String)), false)
            .term("customer", "A person who buys things")
            .build();

        assert_eq!(domain.name, "Test");
        assert_eq!(domain.schema.fields.len(), 2);
        assert!(domain.terminology.contains_key("customer"));
    }

    #[test]
    fn test_field_types() {
        let string_type = FieldType::String;
        let array_type = FieldType::Array(Box::new(FieldType::Number));

        let _ = serde_json::to_string(&string_type).unwrap();
        let _ = serde_json::to_string(&array_type).unwrap();
    }
}
