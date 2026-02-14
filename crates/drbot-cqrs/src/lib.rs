//! CQRS pattern implementation for drbot.
//!
//! This crate provides:
//! - Command and query separation
//! - Command handlers
//! - Query handlers
//! - Command/query buses

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use uuid::Uuid;

/// CQRS error types.
#[derive(Error, Debug)]
pub enum CqrsError {
    #[error("Handler not found for: {0}")]
    HandlerNotFound(String),

    #[error("Command failed: {0}")]
    CommandFailed(String),

    #[error("Query failed: {0}")]
    QueryFailed(String),

    #[error("Validation error: {0}")]
    ValidationError(String),

    #[error("Concurrency conflict")]
    ConcurrencyConflict,
}

/// Result type for CQRS operations.
pub type Result<T> = std::result::Result<T, CqrsError>;

/// Command metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandMetadata {
    /// Command ID.
    pub id: Uuid,
    /// Correlation ID.
    pub correlation_id: Uuid,
    /// Causation ID.
    pub causation_id: Option<Uuid>,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
    /// User ID.
    pub user_id: Option<String>,
    /// Additional metadata.
    pub metadata: HashMap<String, String>,
}

impl CommandMetadata {
    /// Create new metadata.
    pub fn new() -> Self {
        let id = Uuid::new_v4();
        Self {
            id,
            correlation_id: id,
            causation_id: None,
            timestamp: Utc::now(),
            user_id: None,
            metadata: HashMap::new(),
        }
    }

    /// Set correlation ID.
    pub fn with_correlation_id(mut self, id: Uuid) -> Self {
        self.correlation_id = id;
        self
    }

    /// Set causation ID.
    pub fn with_causation_id(mut self, id: Uuid) -> Self {
        self.causation_id = Some(id);
        self
    }

    /// Set user ID.
    pub fn with_user_id(mut self, id: impl Into<String>) -> Self {
        self.user_id = Some(id.into());
        self
    }
}

impl Default for CommandMetadata {
    fn default() -> Self {
        Self::new()
    }
}

/// A command to be executed.
pub trait Command: Send + Sync + 'static {
    /// The result type of this command.
    type Result: Send + 'static;

    /// Get command name.
    fn name(&self) -> &'static str;
}

/// Command handler trait.
#[async_trait]
pub trait CommandHandler<C: Command>: Send + Sync {
    /// Handle a command.
    async fn handle(&self, command: C, metadata: &CommandMetadata) -> Result<C::Result>;
}

/// Query metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryMetadata {
    /// Query ID.
    pub id: Uuid,
    /// Correlation ID.
    pub correlation_id: Uuid,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
    /// User ID.
    pub user_id: Option<String>,
}

impl QueryMetadata {
    /// Create new metadata.
    pub fn new() -> Self {
        let id = Uuid::new_v4();
        Self {
            id,
            correlation_id: id,
            timestamp: Utc::now(),
            user_id: None,
        }
    }
}

impl Default for QueryMetadata {
    fn default() -> Self {
        Self::new()
    }
}

/// A query to be executed.
pub trait Query: Send + Sync + 'static {
    /// The result type of this query.
    type Result: Send + 'static;

    /// Get query name.
    fn name(&self) -> &'static str;
}

/// Query handler trait.
#[async_trait]
pub trait QueryHandler<Q: Query>: Send + Sync {
    /// Handle a query.
    async fn handle(&self, query: Q, metadata: &QueryMetadata) -> Result<Q::Result>;
}

/// Type-erased command handler.
type BoxedCommandHandler = Box<dyn Any + Send + Sync>;

/// Type-erased query handler.
type BoxedQueryHandler = Box<dyn Any + Send + Sync>;

/// Command bus for dispatching commands.
pub struct CommandBus {
    handlers: RwLock<HashMap<TypeId, BoxedCommandHandler>>,
}

impl CommandBus {
    /// Create a new command bus.
    pub fn new() -> Self {
        Self {
            handlers: RwLock::new(HashMap::new()),
        }
    }

    /// Register a command handler.
    pub async fn register<C: Command, H: CommandHandler<C> + 'static>(&self, handler: H) {
        let mut handlers = self.handlers.write().await;
        let handler: Arc<dyn CommandHandler<C>> = Arc::new(handler);
        handlers.insert(TypeId::of::<C>(), Box::new(handler));
    }

    /// Dispatch a command.
    pub async fn dispatch<C: Command>(&self, command: C) -> Result<C::Result> {
        self.dispatch_with_metadata(command, CommandMetadata::new())
            .await
    }

    /// Dispatch a command with metadata.
    pub async fn dispatch_with_metadata<C: Command>(
        &self,
        command: C,
        metadata: CommandMetadata,
    ) -> Result<C::Result> {
        let handlers = self.handlers.read().await;

        let handler = handlers
            .get(&TypeId::of::<C>())
            .ok_or_else(|| CqrsError::HandlerNotFound(command.name().to_string()))?;

        let handler = handler
            .downcast_ref::<Arc<dyn CommandHandler<C>>>()
            .ok_or_else(|| CqrsError::HandlerNotFound(command.name().to_string()))?;

        handler.handle(command, &metadata).await
    }
}

impl Default for CommandBus {
    fn default() -> Self {
        Self::new()
    }
}

/// Query bus for dispatching queries.
pub struct QueryBus {
    handlers: RwLock<HashMap<TypeId, BoxedQueryHandler>>,
}

impl QueryBus {
    /// Create a new query bus.
    pub fn new() -> Self {
        Self {
            handlers: RwLock::new(HashMap::new()),
        }
    }

    /// Register a query handler.
    pub async fn register<Q: Query, H: QueryHandler<Q> + 'static>(&self, handler: H) {
        let mut handlers = self.handlers.write().await;
        let handler: Arc<dyn QueryHandler<Q>> = Arc::new(handler);
        handlers.insert(TypeId::of::<Q>(), Box::new(handler));
    }

    /// Execute a query.
    pub async fn execute<Q: Query>(&self, query: Q) -> Result<Q::Result> {
        self.execute_with_metadata(query, QueryMetadata::new())
            .await
    }

    /// Execute a query with metadata.
    pub async fn execute_with_metadata<Q: Query>(
        &self,
        query: Q,
        metadata: QueryMetadata,
    ) -> Result<Q::Result> {
        let handlers = self.handlers.read().await;

        let handler = handlers
            .get(&TypeId::of::<Q>())
            .ok_or_else(|| CqrsError::HandlerNotFound(query.name().to_string()))?;

        let handler = handler
            .downcast_ref::<Arc<dyn QueryHandler<Q>>>()
            .ok_or_else(|| CqrsError::HandlerNotFound(query.name().to_string()))?;

        handler.handle(query, &metadata).await
    }
}

impl Default for QueryBus {
    fn default() -> Self {
        Self::new()
    }
}

/// Combined CQRS dispatcher.
pub struct CqrsDispatcher {
    /// Command bus.
    pub commands: CommandBus,
    /// Query bus.
    pub queries: QueryBus,
}

impl CqrsDispatcher {
    /// Create a new dispatcher.
    pub fn new() -> Self {
        Self {
            commands: CommandBus::new(),
            queries: QueryBus::new(),
        }
    }

    /// Dispatch a command.
    pub async fn command<C: Command>(&self, command: C) -> Result<C::Result> {
        self.commands.dispatch(command).await
    }

    /// Execute a query.
    pub async fn query<Q: Query>(&self, query: Q) -> Result<Q::Result> {
        self.queries.execute(query).await
    }
}

impl Default for CqrsDispatcher {
    fn default() -> Self {
        Self::new()
    }
}

/// Command validation trait.
pub trait Validate {
    /// Validate the command.
    fn validate(&self) -> Result<()>;
}

/// Validating command handler wrapper.
pub struct ValidatingHandler<H> {
    inner: H,
}

impl<H> ValidatingHandler<H> {
    /// Create a new validating handler.
    pub fn new(inner: H) -> Self {
        Self { inner }
    }
}

#[async_trait]
impl<C, H> CommandHandler<C> for ValidatingHandler<H>
where
    C: Command + Validate,
    H: CommandHandler<C>,
{
    async fn handle(&self, command: C, metadata: &CommandMetadata) -> Result<C::Result> {
        command.validate()?;
        self.inner.handle(command, metadata).await
    }
}

/// Command result that may produce events.
#[derive(Debug, Clone)]
pub struct CommandResult<T, E> {
    /// The result value.
    pub value: T,
    /// Events produced by the command.
    pub events: Vec<E>,
}

impl<T, E> CommandResult<T, E> {
    /// Create a new result.
    pub fn new(value: T, events: Vec<E>) -> Self {
        Self { value, events }
    }

    /// Create a result with no events.
    pub fn ok(value: T) -> Self {
        Self {
            value,
            events: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test command
    struct CreateUser {
        name: String,
    }

    impl Command for CreateUser {
        type Result = Uuid;

        fn name(&self) -> &'static str {
            "CreateUser"
        }
    }

    impl Validate for CreateUser {
        fn validate(&self) -> Result<()> {
            if self.name.is_empty() {
                return Err(CqrsError::ValidationError("Name is required".to_string()));
            }
            Ok(())
        }
    }

    // Test query
    struct GetUser {
        id: Uuid,
    }

    impl Query for GetUser {
        type Result = Option<String>;

        fn name(&self) -> &'static str {
            "GetUser"
        }
    }

    // Handlers
    struct CreateUserHandler;

    #[async_trait]
    impl CommandHandler<CreateUser> for CreateUserHandler {
        async fn handle(&self, _command: CreateUser, _metadata: &CommandMetadata) -> Result<Uuid> {
            Ok(Uuid::new_v4())
        }
    }

    struct GetUserHandler {
        users: HashMap<Uuid, String>,
    }

    #[async_trait]
    impl QueryHandler<GetUser> for GetUserHandler {
        async fn handle(
            &self,
            query: GetUser,
            _metadata: &QueryMetadata,
        ) -> Result<Option<String>> {
            Ok(self.users.get(&query.id).cloned())
        }
    }

    #[test]
    fn test_command_metadata() {
        let meta = CommandMetadata::new()
            .with_user_id("user-123")
            .with_correlation_id(Uuid::new_v4());

        assert!(meta.user_id.is_some());
    }

    #[test]
    fn test_query_metadata() {
        let meta = QueryMetadata::new();
        assert_eq!(meta.id, meta.correlation_id);
    }

    #[tokio::test]
    async fn test_command_bus() {
        let bus = CommandBus::new();
        bus.register(CreateUserHandler).await;

        let command = CreateUser {
            name: "Test".to_string(),
        };

        let result = bus.dispatch(command).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_query_bus() {
        let bus = QueryBus::new();

        let mut users = HashMap::new();
        let user_id = Uuid::new_v4();
        users.insert(user_id, "Test User".to_string());

        bus.register(GetUserHandler { users }).await;

        let query = GetUser { id: user_id };
        let result = bus.execute(query).await.unwrap();

        assert_eq!(result, Some("Test User".to_string()));
    }

    #[tokio::test]
    async fn test_handler_not_found() {
        let bus = CommandBus::new();

        let command = CreateUser {
            name: "Test".to_string(),
        };

        let result = bus.dispatch(command).await;
        assert!(matches!(result, Err(CqrsError::HandlerNotFound(_))));
    }

    #[tokio::test]
    async fn test_validating_handler() {
        let handler = ValidatingHandler::new(CreateUserHandler);

        // Valid command
        let command = CreateUser {
            name: "Test".to_string(),
        };
        let result = handler.handle(command, &CommandMetadata::new()).await;
        assert!(result.is_ok());

        // Invalid command
        let command = CreateUser {
            name: String::new(),
        };
        let result = handler.handle(command, &CommandMetadata::new()).await;
        assert!(matches!(result, Err(CqrsError::ValidationError(_))));
    }

    #[test]
    fn test_command_result() {
        let result: CommandResult<i32, String> = CommandResult::new(42, vec!["event1".to_string()]);
        assert_eq!(result.value, 42);
        assert_eq!(result.events.len(), 1);

        let result: CommandResult<i32, String> = CommandResult::ok(42);
        assert!(result.events.is_empty());
    }

    #[tokio::test]
    async fn test_cqrs_dispatcher() {
        let dispatcher = CqrsDispatcher::new();
        dispatcher.commands.register(CreateUserHandler).await;

        let command = CreateUser {
            name: "Test".to_string(),
        };

        let result = dispatcher.command(command).await;
        assert!(result.is_ok());
    }
}
