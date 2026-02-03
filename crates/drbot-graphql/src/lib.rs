//! GraphQL support for drbot.
//!
//! This crate provides:
//! - Schema definition
//! - Query execution
//! - Subscription support
//! - DataLoader pattern

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;

/// GraphQL error types.
#[derive(Error, Debug)]
pub enum GraphqlError {
    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Validation error: {0}")]
    ValidationError(String),

    #[error("Execution error: {0}")]
    ExecutionError(String),

    #[error("Field error: {field} - {message}")]
    FieldError { field: String, message: String },

    #[error("Type error: expected {expected}, got {actual}")]
    TypeError { expected: String, actual: String },

    #[error("Resolver not found: {0}")]
    ResolverNotFound(String),
}

/// Result type for GraphQL operations.
pub type Result<T> = std::result::Result<T, GraphqlError>;

/// GraphQL value type.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    List(Vec<Value>),
    Object(HashMap<String, Value>),
}

impl Value {
    /// Check if null.
    pub fn is_null(&self) -> bool {
        matches!(self, Value::Null)
    }

    /// Get as string.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s),
            _ => None,
        }
    }

    /// Get as i64.
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Value::Int(i) => Some(*i),
            _ => None,
        }
    }

    /// Get as bool.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// Get as object.
    pub fn as_object(&self) -> Option<&HashMap<String, Value>> {
        match self {
            Value::Object(obj) => Some(obj),
            _ => None,
        }
    }
}

impl From<String> for Value {
    fn from(s: String) -> Self {
        Value::String(s)
    }
}

impl From<&str> for Value {
    fn from(s: &str) -> Self {
        Value::String(s.to_string())
    }
}

impl From<i64> for Value {
    fn from(i: i64) -> Self {
        Value::Int(i)
    }
}

impl From<bool> for Value {
    fn from(b: bool) -> Self {
        Value::Bool(b)
    }
}

/// GraphQL type kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TypeKind {
    Scalar,
    Object,
    Interface,
    Union,
    Enum,
    InputObject,
    List,
    NonNull,
}

/// Field definition.
#[derive(Debug, Clone)]
pub struct FieldDefinition {
    /// Field name.
    pub name: String,
    /// Field type.
    pub field_type: String,
    /// Description.
    pub description: Option<String>,
    /// Arguments.
    pub arguments: Vec<ArgumentDefinition>,
    /// Is nullable.
    pub nullable: bool,
}

impl FieldDefinition {
    /// Create a new field.
    pub fn new(name: impl Into<String>, field_type: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            field_type: field_type.into(),
            description: None,
            arguments: Vec::new(),
            nullable: true,
        }
    }

    /// Set description.
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Add argument.
    pub fn with_arg(mut self, arg: ArgumentDefinition) -> Self {
        self.arguments.push(arg);
        self
    }

    /// Make non-null.
    pub fn non_null(mut self) -> Self {
        self.nullable = false;
        self
    }
}

/// Argument definition.
#[derive(Debug, Clone)]
pub struct ArgumentDefinition {
    /// Argument name.
    pub name: String,
    /// Argument type.
    pub arg_type: String,
    /// Default value.
    pub default: Option<Value>,
    /// Description.
    pub description: Option<String>,
}

impl ArgumentDefinition {
    /// Create a new argument.
    pub fn new(name: impl Into<String>, arg_type: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            arg_type: arg_type.into(),
            default: None,
            description: None,
        }
    }
}

/// Type definition.
#[derive(Debug, Clone)]
pub struct TypeDefinition {
    /// Type name.
    pub name: String,
    /// Type kind.
    pub kind: TypeKind,
    /// Description.
    pub description: Option<String>,
    /// Fields (for Object, Interface).
    pub fields: Vec<FieldDefinition>,
    /// Possible types (for Union, Interface).
    pub possible_types: Vec<String>,
    /// Enum values (for Enum).
    pub enum_values: Vec<String>,
}

impl TypeDefinition {
    /// Create a scalar type.
    pub fn scalar(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            kind: TypeKind::Scalar,
            description: None,
            fields: Vec::new(),
            possible_types: Vec::new(),
            enum_values: Vec::new(),
        }
    }

    /// Create an object type.
    pub fn object(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            kind: TypeKind::Object,
            description: None,
            fields: Vec::new(),
            possible_types: Vec::new(),
            enum_values: Vec::new(),
        }
    }

    /// Create an enum type.
    pub fn enum_type(name: impl Into<String>, values: Vec<String>) -> Self {
        Self {
            name: name.into(),
            kind: TypeKind::Enum,
            description: None,
            fields: Vec::new(),
            possible_types: Vec::new(),
            enum_values: values,
        }
    }

    /// Add field.
    pub fn with_field(mut self, field: FieldDefinition) -> Self {
        self.fields.push(field);
        self
    }
}

/// GraphQL schema.
#[derive(Debug, Clone)]
pub struct Schema {
    /// Types.
    pub types: HashMap<String, TypeDefinition>,
    /// Query type name.
    pub query_type: String,
    /// Mutation type name.
    pub mutation_type: Option<String>,
    /// Subscription type name.
    pub subscription_type: Option<String>,
}

impl Schema {
    /// Create a new schema.
    pub fn new(query_type: impl Into<String>) -> Self {
        let mut schema = Self {
            types: HashMap::new(),
            query_type: query_type.into(),
            mutation_type: None,
            subscription_type: None,
        };

        // Add built-in scalars
        schema.add_type(TypeDefinition::scalar("String"));
        schema.add_type(TypeDefinition::scalar("Int"));
        schema.add_type(TypeDefinition::scalar("Float"));
        schema.add_type(TypeDefinition::scalar("Boolean"));
        schema.add_type(TypeDefinition::scalar("ID"));

        schema
    }

    /// Add type.
    pub fn add_type(&mut self, type_def: TypeDefinition) {
        self.types.insert(type_def.name.clone(), type_def);
    }

    /// Set mutation type.
    pub fn with_mutation(mut self, mutation_type: impl Into<String>) -> Self {
        self.mutation_type = Some(mutation_type.into());
        self
    }

    /// Set subscription type.
    pub fn with_subscription(mut self, subscription_type: impl Into<String>) -> Self {
        self.subscription_type = Some(subscription_type.into());
        self
    }

    /// Get type.
    pub fn get_type(&self, name: &str) -> Option<&TypeDefinition> {
        self.types.get(name)
    }
}

/// Operation type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationType {
    Query,
    Mutation,
    Subscription,
}

/// GraphQL request.
#[derive(Debug, Clone, Deserialize)]
pub struct GraphqlRequest {
    /// Query string.
    pub query: String,
    /// Operation name.
    #[serde(rename = "operationName")]
    pub operation_name: Option<String>,
    /// Variables.
    pub variables: Option<HashMap<String, Value>>,
}

impl GraphqlRequest {
    /// Create a new request.
    pub fn new(query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            operation_name: None,
            variables: None,
        }
    }

    /// Set operation name.
    pub fn with_operation(mut self, name: impl Into<String>) -> Self {
        self.operation_name = Some(name.into());
        self
    }

    /// Set variables.
    pub fn with_variables(mut self, variables: HashMap<String, Value>) -> Self {
        self.variables = Some(variables);
        self
    }
}

/// GraphQL response.
#[derive(Debug, Clone, Serialize)]
pub struct GraphqlResponse {
    /// Data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
    /// Errors.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<GraphqlErrorResponse>,
    /// Extensions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extensions: Option<HashMap<String, Value>>,
}

impl GraphqlResponse {
    /// Create a successful response.
    pub fn data(data: Value) -> Self {
        Self {
            data: Some(data),
            errors: Vec::new(),
            extensions: None,
        }
    }

    /// Create an error response.
    pub fn error(error: GraphqlErrorResponse) -> Self {
        Self {
            data: None,
            errors: vec![error],
            extensions: None,
        }
    }
}

/// GraphQL error response.
#[derive(Debug, Clone, Serialize)]
pub struct GraphqlErrorResponse {
    /// Error message.
    pub message: String,
    /// Error locations.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub locations: Vec<Location>,
    /// Error path.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub path: Vec<Value>,
    /// Extensions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extensions: Option<HashMap<String, Value>>,
}

impl GraphqlErrorResponse {
    /// Create a new error.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            locations: Vec::new(),
            path: Vec::new(),
            extensions: None,
        }
    }
}

/// Source location.
#[derive(Debug, Clone, Serialize)]
pub struct Location {
    /// Line number.
    pub line: u32,
    /// Column number.
    pub column: u32,
}

/// Resolver context.
#[derive(Debug, Clone)]
pub struct ResolverContext {
    /// Parent value.
    pub parent: Value,
    /// Arguments.
    pub arguments: HashMap<String, Value>,
    /// Variables.
    pub variables: HashMap<String, Value>,
    /// Request ID.
    pub request_id: Uuid,
}

impl ResolverContext {
    /// Create a new context.
    pub fn new() -> Self {
        Self {
            parent: Value::Null,
            arguments: HashMap::new(),
            variables: HashMap::new(),
            request_id: Uuid::new_v4(),
        }
    }

    /// Get argument.
    pub fn arg<T>(&self, name: &str) -> Option<T>
    where
        T: TryFrom<Value>,
    {
        self.arguments
            .get(name)
            .cloned()
            .and_then(|v| T::try_from(v).ok())
    }
}

impl Default for ResolverContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Resolver trait.
#[async_trait]
pub trait Resolver: Send + Sync {
    /// Resolve a field.
    async fn resolve(&self, ctx: ResolverContext) -> Result<Value>;
}

/// Function resolver.
pub struct FnResolver<F>
where
    F: Fn(ResolverContext) -> Result<Value> + Send + Sync,
{
    f: F,
}

impl<F> FnResolver<F>
where
    F: Fn(ResolverContext) -> Result<Value> + Send + Sync,
{
    /// Create a new function resolver.
    pub fn new(f: F) -> Self {
        Self { f }
    }
}

#[async_trait]
impl<F> Resolver for FnResolver<F>
where
    F: Fn(ResolverContext) -> Result<Value> + Send + Sync,
{
    async fn resolve(&self, ctx: ResolverContext) -> Result<Value> {
        (self.f)(ctx)
    }
}

/// DataLoader for batching.
pub struct DataLoader<K, V>
where
    K: Eq + std::hash::Hash + Clone + Send,
    V: Clone + Send,
{
    cache: RwLock<HashMap<K, V>>,
    batch_fn:
        Arc<dyn Fn(Vec<K>) -> futures::future::BoxFuture<'static, HashMap<K, V>> + Send + Sync>,
}

impl<K, V> DataLoader<K, V>
where
    K: Eq + std::hash::Hash + Clone + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    /// Create a new loader.
    pub fn new<F, Fut>(batch_fn: F) -> Self
    where
        F: Fn(Vec<K>) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = HashMap<K, V>> + Send + 'static,
    {
        Self {
            cache: RwLock::new(HashMap::new()),
            batch_fn: Arc::new(move |keys| Box::pin(batch_fn(keys))),
        }
    }

    /// Load a single value.
    pub async fn load(&self, key: K) -> Option<V> {
        // Check cache
        {
            let cache = self.cache.read().await;
            if let Some(value) = cache.get(&key) {
                return Some(value.clone());
            }
        }

        // Load from batch function
        let results: HashMap<K, V> = (self.batch_fn)(vec![key.clone()]).await;

        // Cache results
        {
            let mut cache = self.cache.write().await;
            for (k, v) in results.iter() {
                cache.insert(k.clone(), v.clone());
            }
        }

        results.get(&key).cloned()
    }

    /// Load multiple values.
    pub async fn load_many(&self, keys: Vec<K>) -> HashMap<K, V> {
        let mut results = HashMap::new();
        let mut missing = Vec::new();

        // Check cache
        {
            let cache = self.cache.read().await;
            for key in keys {
                if let Some(value) = cache.get(&key) {
                    results.insert(key, value.clone());
                } else {
                    missing.push(key);
                }
            }
        }

        if missing.is_empty() {
            return results;
        }

        // Load missing
        let loaded: HashMap<K, V> = (self.batch_fn)(missing).await;

        // Cache and add to results
        {
            let mut cache = self.cache.write().await;
            for (k, v) in loaded {
                cache.insert(k.clone(), v.clone());
                results.insert(k, v);
            }
        }

        results
    }

    /// Clear cache.
    pub async fn clear(&self) {
        let mut cache = self.cache.write().await;
        cache.clear();
    }
}

/// Subscription event.
#[derive(Debug, Clone)]
pub struct SubscriptionEvent {
    /// Event ID.
    pub id: Uuid,
    /// Subscription ID.
    pub subscription_id: Uuid,
    /// Data.
    pub data: Value,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
}

/// Subscription manager.
pub struct SubscriptionManager {
    subscriptions: RwLock<HashMap<Uuid, SubscriptionInfo>>,
    event_tx: broadcast::Sender<SubscriptionEvent>,
}

#[derive(Debug, Clone)]
struct SubscriptionInfo {
    id: Uuid,
    query: String,
    variables: HashMap<String, Value>,
    created_at: DateTime<Utc>,
}

impl SubscriptionManager {
    /// Create a new manager.
    pub fn new() -> Self {
        let (event_tx, _) = broadcast::channel(1000);
        Self {
            subscriptions: RwLock::new(HashMap::new()),
            event_tx,
        }
    }

    /// Subscribe.
    pub async fn subscribe(&self, query: String, variables: HashMap<String, Value>) -> Uuid {
        let id = Uuid::new_v4();
        let info = SubscriptionInfo {
            id,
            query,
            variables,
            created_at: Utc::now(),
        };

        let mut subs = self.subscriptions.write().await;
        subs.insert(id, info);
        id
    }

    /// Unsubscribe.
    pub async fn unsubscribe(&self, id: Uuid) {
        let mut subs = self.subscriptions.write().await;
        subs.remove(&id);
    }

    /// Publish event.
    pub fn publish(&self, subscription_id: Uuid, data: Value) {
        let event = SubscriptionEvent {
            id: Uuid::new_v4(),
            subscription_id,
            data,
            timestamp: Utc::now(),
        };
        let _ = self.event_tx.send(event);
    }

    /// Subscribe to events.
    pub fn events(&self) -> broadcast::Receiver<SubscriptionEvent> {
        self.event_tx.subscribe()
    }
}

impl Default for SubscriptionManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_value_types() {
        assert!(Value::Null.is_null());
        assert_eq!(Value::String("test".to_string()).as_str(), Some("test"));
        assert_eq!(Value::Int(42).as_i64(), Some(42));
        assert_eq!(Value::Bool(true).as_bool(), Some(true));
    }

    #[test]
    fn test_schema_creation() {
        let mut schema = Schema::new("Query");

        let user_type = TypeDefinition::object("User")
            .with_field(FieldDefinition::new("id", "ID").non_null())
            .with_field(FieldDefinition::new("name", "String"));

        schema.add_type(user_type);

        assert!(schema.get_type("User").is_some());
        assert!(schema.get_type("String").is_some());
    }

    #[test]
    fn test_request_builder() {
        let request = GraphqlRequest::new("{ users { id } }").with_operation("GetUsers");

        assert_eq!(request.operation_name, Some("GetUsers".to_string()));
    }

    #[test]
    fn test_response() {
        let response = GraphqlResponse::data(Value::Object(HashMap::new()));
        assert!(response.data.is_some());
        assert!(response.errors.is_empty());
    }

    #[tokio::test]
    async fn test_data_loader() {
        let loader = DataLoader::new(|keys: Vec<i32>| async move {
            keys.into_iter().map(|k| (k, k * 2)).collect()
        });

        let result = loader.load(5).await;
        assert_eq!(result, Some(10));

        // Should be cached
        let cached = loader.load(5).await;
        assert_eq!(cached, Some(10));
    }

    #[tokio::test]
    async fn test_subscription_manager() {
        let manager = SubscriptionManager::new();

        let sub_id = manager
            .subscribe("subscription { messages }".to_string(), HashMap::new())
            .await;

        let mut receiver = manager.events();

        manager.publish(sub_id, Value::String("Hello".to_string()));

        let event = receiver.recv().await.unwrap();
        assert_eq!(event.subscription_id, sub_id);
    }
}
