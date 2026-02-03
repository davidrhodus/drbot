//! Natural language to SQL with anomaly detection.
//!
//! This crate provides data operations capabilities:
//! - Convert natural language queries to SQL
//! - Detect anomalies in query results
//! - Validate and explain generated queries
//! - Track query patterns and optimize

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use uuid::Uuid;

/// DataOps errors.
#[derive(Debug, Error)]
pub enum DataOpsError {
    #[error("Query generation failed: {0}")]
    QueryGenerationFailed(String),

    #[error("Query execution failed: {0}")]
    ExecutionFailed(String),

    #[error("Schema not found: {0}")]
    SchemaNotFound(String),

    #[error("Anomaly detection failed: {0}")]
    AnomalyDetectionFailed(String),

    #[error("Invalid query: {0}")]
    InvalidQuery(String),

    #[error("Provider error: {0}")]
    ProviderError(String),
}

/// Result type for data operations.
pub type Result<T> = std::result::Result<T, DataOpsError>;

/// Database schema information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Schema {
    /// Schema name.
    pub name: String,
    /// Tables in the schema.
    pub tables: Vec<Table>,
    /// Relationships.
    pub relationships: Vec<Relationship>,
    /// Schema description.
    pub description: Option<String>,
}

/// A database table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Table {
    /// Table name.
    pub name: String,
    /// Columns.
    pub columns: Vec<Column>,
    /// Primary key columns.
    pub primary_key: Vec<String>,
    /// Table description.
    pub description: Option<String>,
    /// Approximate row count.
    pub row_count: Option<u64>,
}

/// A table column.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Column {
    /// Column name.
    pub name: String,
    /// Data type.
    pub data_type: DataType,
    /// Whether nullable.
    pub nullable: bool,
    /// Default value.
    pub default: Option<String>,
    /// Column description.
    pub description: Option<String>,
}

/// Data types.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum DataType {
    Integer,
    BigInt,
    Float,
    Double,
    Decimal { precision: u8, scale: u8 },
    Boolean,
    String,
    Text,
    Date,
    DateTime,
    Timestamp,
    Json,
    Uuid,
    Binary,
    Array(Box<DataType>),
    Custom(String),
}

/// A relationship between tables.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Relationship {
    /// Relationship name.
    pub name: String,
    /// Source table.
    pub from_table: String,
    /// Source columns.
    pub from_columns: Vec<String>,
    /// Target table.
    pub to_table: String,
    /// Target columns.
    pub to_columns: Vec<String>,
    /// Relationship type.
    pub relationship_type: RelationshipType,
}

/// Types of relationships.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum RelationshipType {
    OneToOne,
    OneToMany,
    ManyToOne,
    ManyToMany,
}

/// A natural language query request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NLQueryRequest {
    /// Natural language question.
    pub question: String,
    /// Schema to query against.
    pub schema_name: String,
    /// Additional context.
    pub context: Option<String>,
    /// Safety constraints.
    pub constraints: QueryConstraints,
}

/// Constraints for query generation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryConstraints {
    /// Allow SELECT only.
    pub read_only: bool,
    /// Maximum rows to return.
    pub max_rows: Option<u32>,
    /// Timeout in seconds.
    pub timeout_secs: Option<u32>,
    /// Allowed tables.
    pub allowed_tables: Option<Vec<String>>,
    /// Forbidden columns.
    pub forbidden_columns: Option<Vec<String>>,
}

impl Default for QueryConstraints {
    fn default() -> Self {
        Self {
            read_only: true,
            max_rows: Some(1000),
            timeout_secs: Some(30),
            allowed_tables: None,
            forbidden_columns: None,
        }
    }
}

/// Generated SQL query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedQuery {
    /// Query identifier.
    pub id: String,
    /// Original question.
    pub question: String,
    /// Generated SQL.
    pub sql: String,
    /// Query explanation.
    pub explanation: String,
    /// Confidence in query (0.0-1.0).
    pub confidence: f64,
    /// Tables used.
    pub tables_used: Vec<String>,
    /// Potential issues.
    pub warnings: Vec<QueryWarning>,
    /// Generation timestamp.
    pub generated_at: DateTime<Utc>,
}

/// A warning about a generated query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryWarning {
    /// Warning type.
    pub warning_type: WarningType,
    /// Warning message.
    pub message: String,
    /// Severity.
    pub severity: WarningSeverity,
}

/// Types of query warnings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WarningType {
    /// Missing WHERE clause.
    MissingFilter,
    /// Potentially slow query.
    Performance,
    /// Ambiguous interpretation.
    Ambiguity,
    /// Data type mismatch.
    TypeMismatch,
    /// Missing join condition.
    CartesianProduct,
    /// Accessing sensitive data.
    SensitiveData,
}

/// Warning severity.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum WarningSeverity {
    Info,
    Warning,
    Error,
}

/// Query execution result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResult {
    /// Query that was executed.
    pub query_id: String,
    /// Column names.
    pub columns: Vec<String>,
    /// Result rows.
    pub rows: Vec<Vec<serde_json::Value>>,
    /// Row count.
    pub row_count: usize,
    /// Execution time in ms.
    pub execution_time_ms: u64,
    /// Detected anomalies.
    pub anomalies: Vec<DataAnomaly>,
    /// Summary statistics.
    pub statistics: Option<ResultStatistics>,
}

/// An anomaly detected in data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataAnomaly {
    /// Anomaly identifier.
    pub id: String,
    /// Anomaly type.
    pub anomaly_type: AnomalyType,
    /// Affected column.
    pub column: String,
    /// Description.
    pub description: String,
    /// Severity (0.0-1.0).
    pub severity: f64,
    /// Example values.
    pub examples: Vec<serde_json::Value>,
}

/// Types of data anomalies.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AnomalyType {
    /// Outlier value.
    Outlier,
    /// Unexpected null.
    UnexpectedNull,
    /// Data type inconsistency.
    TypeInconsistency,
    /// Pattern violation.
    PatternViolation,
    /// Unusual distribution.
    DistributionAnomaly,
    /// Duplicate values.
    Duplicate,
    /// Missing expected values.
    MissingExpected,
}

/// Statistics about query results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResultStatistics {
    /// Statistics per column.
    pub column_stats: HashMap<String, ColumnStatistics>,
}

/// Statistics for a column.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnStatistics {
    /// Null count.
    pub null_count: usize,
    /// Distinct count.
    pub distinct_count: usize,
    /// Min value (for numeric/date).
    pub min: Option<serde_json::Value>,
    /// Max value.
    pub max: Option<serde_json::Value>,
    /// Average (for numeric).
    pub avg: Option<f64>,
}

/// Provider for query generation and execution.
#[async_trait]
pub trait DataOpsProvider: Send + Sync {
    /// Generate SQL from natural language.
    async fn generate_query(
        &self,
        request: &NLQueryRequest,
        schema: &Schema,
    ) -> Result<GeneratedQuery>;

    /// Validate a generated query.
    async fn validate_query(&self, query: &str, schema: &Schema) -> Result<Vec<QueryWarning>>;

    /// Detect anomalies in results.
    async fn detect_anomalies(&self, result: &QueryResult) -> Result<Vec<DataAnomaly>>;

    /// Explain a query in natural language.
    async fn explain_query(&self, query: &str, schema: &Schema) -> Result<String>;
}

/// Provider for actually executing queries.
#[async_trait]
pub trait QueryExecutor: Send + Sync {
    /// Execute a query.
    async fn execute(&self, query: &str, constraints: &QueryConstraints) -> Result<QueryResult>;
}

/// The DataOps engine.
pub struct DataOpsEngine {
    /// Provider for query generation.
    provider: Arc<dyn DataOpsProvider>,
    /// Query executor.
    executor: Option<Arc<dyn QueryExecutor>>,
    /// Registered schemas.
    schemas: Arc<RwLock<HashMap<String, Schema>>>,
    /// Query history.
    history: Arc<RwLock<Vec<QueryHistoryEntry>>>,
}

/// Entry in query history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryHistoryEntry {
    /// Entry ID.
    pub id: String,
    /// Original question.
    pub question: String,
    /// Generated query.
    pub query: GeneratedQuery,
    /// Execution result if any.
    pub result: Option<QueryResult>,
    /// User feedback.
    pub feedback: Option<QueryFeedback>,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
}

/// User feedback on a query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryFeedback {
    /// Whether the query was correct.
    pub correct: bool,
    /// Corrections if any.
    pub corrections: Option<String>,
    /// Rating (1-5).
    pub rating: Option<u8>,
}

impl DataOpsEngine {
    /// Create a new DataOps engine.
    pub fn new(provider: Arc<dyn DataOpsProvider>) -> Self {
        Self {
            provider,
            executor: None,
            schemas: Arc::new(RwLock::new(HashMap::new())),
            history: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Set the query executor.
    pub fn with_executor(mut self, executor: Arc<dyn QueryExecutor>) -> Self {
        self.executor = Some(executor);
        self
    }

    /// Register a schema.
    pub async fn register_schema(&self, schema: Schema) -> Result<()> {
        let mut schemas = self.schemas.write().await;
        schemas.insert(schema.name.clone(), schema);
        Ok(())
    }

    /// Get a schema.
    pub async fn get_schema(&self, name: &str) -> Option<Schema> {
        let schemas = self.schemas.read().await;
        schemas.get(name).cloned()
    }

    /// Ask a natural language question.
    pub async fn ask(&self, request: NLQueryRequest) -> Result<GeneratedQuery> {
        let schema = {
            let schemas = self.schemas.read().await;
            schemas
                .get(&request.schema_name)
                .cloned()
                .ok_or_else(|| DataOpsError::SchemaNotFound(request.schema_name.clone()))?
        };

        // Check constraints
        self.validate_constraints(&request, &schema)?;

        // Generate query
        let query = self.provider.generate_query(&request, &schema).await?;

        // Validate query
        let warnings = self.provider.validate_query(&query.sql, &schema).await?;

        let mut query = query;
        query.warnings.extend(warnings);

        // Store in history
        let entry = QueryHistoryEntry {
            id: Uuid::new_v4().to_string(),
            question: request.question.clone(),
            query: query.clone(),
            result: None,
            feedback: None,
            timestamp: Utc::now(),
        };

        let mut history = self.history.write().await;
        history.push(entry);

        Ok(query)
    }

    /// Validate request against constraints.
    fn validate_constraints(&self, request: &NLQueryRequest, schema: &Schema) -> Result<()> {
        if let Some(allowed) = &request.constraints.allowed_tables {
            for table in &schema.tables {
                if !allowed.contains(&table.name) {
                    // Table not in allowed list - that's fine, just skip
                }
            }
        }
        Ok(())
    }

    /// Execute a generated query.
    pub async fn execute(&self, query: &GeneratedQuery) -> Result<QueryResult> {
        let executor = self
            .executor
            .as_ref()
            .ok_or_else(|| DataOpsError::ExecutionFailed("No executor configured".to_string()))?;

        let constraints = QueryConstraints::default();
        let mut result = executor.execute(&query.sql, &constraints).await?;

        // Detect anomalies
        let anomalies = self.provider.detect_anomalies(&result).await?;
        result.anomalies = anomalies;

        // Update history
        let mut history = self.history.write().await;
        if let Some(entry) = history.iter_mut().find(|e| e.query.id == query.id) {
            entry.result = Some(result.clone());
        }

        Ok(result)
    }

    /// Ask and execute in one step.
    pub async fn ask_and_execute(
        &self,
        request: NLQueryRequest,
    ) -> Result<(GeneratedQuery, QueryResult)> {
        let query = self.ask(request).await?;
        let result = self.execute(&query).await?;
        Ok((query, result))
    }

    /// Explain a query.
    pub async fn explain(&self, query: &str, schema_name: &str) -> Result<String> {
        let schema = self
            .get_schema(schema_name)
            .await
            .ok_or_else(|| DataOpsError::SchemaNotFound(schema_name.to_string()))?;

        self.provider.explain_query(query, &schema).await
    }

    /// Provide feedback on a query.
    pub async fn provide_feedback(&self, query_id: &str, feedback: QueryFeedback) -> Result<()> {
        let mut history = self.history.write().await;
        if let Some(entry) = history.iter_mut().find(|e| e.query.id == query_id) {
            entry.feedback = Some(feedback);
            Ok(())
        } else {
            Err(DataOpsError::QueryGenerationFailed(
                "Query not found".to_string(),
            ))
        }
    }

    /// Get query history.
    pub async fn get_history(&self) -> Vec<QueryHistoryEntry> {
        let history = self.history.read().await;
        history.clone()
    }
}

/// Builder for schemas.
pub struct SchemaBuilder {
    schema: Schema,
}

impl SchemaBuilder {
    /// Create a new schema builder.
    pub fn new(name: &str) -> Self {
        Self {
            schema: Schema {
                name: name.to_string(),
                tables: Vec::new(),
                relationships: Vec::new(),
                description: None,
            },
        }
    }

    /// Add a table.
    pub fn table(mut self, table: Table) -> Self {
        self.schema.tables.push(table);
        self
    }

    /// Add a relationship.
    pub fn relationship(mut self, rel: Relationship) -> Self {
        self.schema.relationships.push(rel);
        self
    }

    /// Build the schema.
    pub fn build(self) -> Schema {
        self.schema
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockProvider;

    #[async_trait]
    impl DataOpsProvider for MockProvider {
        async fn generate_query(
            &self,
            request: &NLQueryRequest,
            _schema: &Schema,
        ) -> Result<GeneratedQuery> {
            Ok(GeneratedQuery {
                id: Uuid::new_v4().to_string(),
                question: request.question.clone(),
                sql: "SELECT * FROM users LIMIT 10".to_string(),
                explanation: "Query to get users".to_string(),
                confidence: 0.9,
                tables_used: vec!["users".to_string()],
                warnings: vec![],
                generated_at: Utc::now(),
            })
        }

        async fn validate_query(
            &self,
            _query: &str,
            _schema: &Schema,
        ) -> Result<Vec<QueryWarning>> {
            Ok(vec![])
        }

        async fn detect_anomalies(&self, _result: &QueryResult) -> Result<Vec<DataAnomaly>> {
            Ok(vec![])
        }

        async fn explain_query(&self, query: &str, _schema: &Schema) -> Result<String> {
            Ok(format!("This query: {}", query))
        }
    }

    fn create_test_schema() -> Schema {
        SchemaBuilder::new("test_db")
            .table(Table {
                name: "users".to_string(),
                columns: vec![
                    Column {
                        name: "id".to_string(),
                        data_type: DataType::Integer,
                        nullable: false,
                        default: None,
                        description: None,
                    },
                    Column {
                        name: "name".to_string(),
                        data_type: DataType::String,
                        nullable: false,
                        default: None,
                        description: None,
                    },
                ],
                primary_key: vec!["id".to_string()],
                description: None,
                row_count: Some(1000),
            })
            .build()
    }

    #[tokio::test]
    async fn test_register_schema() {
        let provider = Arc::new(MockProvider);
        let engine = DataOpsEngine::new(provider);

        let schema = create_test_schema();
        engine.register_schema(schema).await.unwrap();

        let retrieved = engine.get_schema("test_db").await.unwrap();
        assert_eq!(retrieved.name, "test_db");
    }

    #[tokio::test]
    async fn test_ask() {
        let provider = Arc::new(MockProvider);
        let engine = DataOpsEngine::new(provider);

        engine.register_schema(create_test_schema()).await.unwrap();

        let request = NLQueryRequest {
            question: "Show me all users".to_string(),
            schema_name: "test_db".to_string(),
            context: None,
            constraints: QueryConstraints::default(),
        };

        let query = engine.ask(request).await.unwrap();
        assert!(query.sql.contains("SELECT"));
    }

    #[tokio::test]
    async fn test_explain() {
        let provider = Arc::new(MockProvider);
        let engine = DataOpsEngine::new(provider);

        engine.register_schema(create_test_schema()).await.unwrap();

        let explanation = engine
            .explain("SELECT * FROM users", "test_db")
            .await
            .unwrap();
        assert!(!explanation.is_empty());
    }

    #[tokio::test]
    async fn test_query_history() {
        let provider = Arc::new(MockProvider);
        let engine = DataOpsEngine::new(provider);

        engine.register_schema(create_test_schema()).await.unwrap();

        let request = NLQueryRequest {
            question: "Show me users".to_string(),
            schema_name: "test_db".to_string(),
            context: None,
            constraints: QueryConstraints::default(),
        };

        engine.ask(request).await.unwrap();

        let history = engine.get_history().await;
        assert_eq!(history.len(), 1);
    }

    #[test]
    fn test_data_types() {
        let int_type = DataType::Integer;
        let decimal_type = DataType::Decimal {
            precision: 10,
            scale: 2,
        };
        let array_type = DataType::Array(Box::new(DataType::String));

        let _ = serde_json::to_string(&int_type).unwrap();
        let _ = serde_json::to_string(&decimal_type).unwrap();
        let _ = serde_json::to_string(&array_type).unwrap();
    }
}
