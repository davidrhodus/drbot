//! gRPC support for drbot.
//!
//! This crate provides:
//! - Protocol buffer handling
//! - Service definitions
//! - Streaming support
//! - Interceptors

use async_trait::async_trait;
use bytes::Bytes;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::{broadcast, mpsc, RwLock};
use uuid::Uuid;

/// gRPC error types.
#[derive(Error, Debug, Clone)]
pub enum GrpcError {
    #[error("OK")]
    Ok,

    #[error("Cancelled")]
    Cancelled,

    #[error("Unknown error: {0}")]
    Unknown(String),

    #[error("Invalid argument: {0}")]
    InvalidArgument(String),

    #[error("Deadline exceeded")]
    DeadlineExceeded,

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Already exists: {0}")]
    AlreadyExists(String),

    #[error("Permission denied")]
    PermissionDenied,

    #[error("Resource exhausted")]
    ResourceExhausted,

    #[error("Failed precondition: {0}")]
    FailedPrecondition(String),

    #[error("Aborted")]
    Aborted,

    #[error("Out of range")]
    OutOfRange,

    #[error("Unimplemented")]
    Unimplemented,

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Unavailable")]
    Unavailable,

    #[error("Data loss")]
    DataLoss,

    #[error("Unauthenticated")]
    Unauthenticated,
}

impl GrpcError {
    /// Get status code.
    pub fn code(&self) -> i32 {
        match self {
            GrpcError::Ok => 0,
            GrpcError::Cancelled => 1,
            GrpcError::Unknown(_) => 2,
            GrpcError::InvalidArgument(_) => 3,
            GrpcError::DeadlineExceeded => 4,
            GrpcError::NotFound(_) => 5,
            GrpcError::AlreadyExists(_) => 6,
            GrpcError::PermissionDenied => 7,
            GrpcError::ResourceExhausted => 8,
            GrpcError::FailedPrecondition(_) => 9,
            GrpcError::Aborted => 10,
            GrpcError::OutOfRange => 11,
            GrpcError::Unimplemented => 12,
            GrpcError::Internal(_) => 13,
            GrpcError::Unavailable => 14,
            GrpcError::DataLoss => 15,
            GrpcError::Unauthenticated => 16,
        }
    }
}

/// Result type for gRPC operations.
pub type Result<T> = std::result::Result<T, GrpcError>;

/// gRPC metadata.
#[derive(Debug, Clone, Default)]
pub struct Metadata {
    entries: HashMap<String, Vec<String>>,
}

impl Metadata {
    /// Create empty metadata.
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// Add a value.
    pub fn insert(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.entries
            .entry(key.into())
            .or_default()
            .push(value.into());
    }

    /// Get first value.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries.get(key)?.first().map(|s| s.as_str())
    }

    /// Get all values.
    pub fn get_all(&self, key: &str) -> Option<&Vec<String>> {
        self.entries.get(key)
    }

    /// Check if contains key.
    pub fn contains(&self, key: &str) -> bool {
        self.entries.contains_key(key)
    }
}

/// gRPC request.
#[derive(Debug, Clone)]
pub struct Request<T> {
    /// Request message.
    pub message: T,
    /// Metadata.
    pub metadata: Metadata,
    /// Deadline.
    pub deadline: Option<DateTime<Utc>>,
}

impl<T> Request<T> {
    /// Create a new request.
    pub fn new(message: T) -> Self {
        Self {
            message,
            metadata: Metadata::new(),
            deadline: None,
        }
    }

    /// Add metadata.
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key, value);
        self
    }

    /// Set deadline.
    pub fn with_deadline(mut self, deadline: DateTime<Utc>) -> Self {
        self.deadline = Some(deadline);
        self
    }

    /// Map message type.
    pub fn map<U, F>(self, f: F) -> Request<U>
    where
        F: FnOnce(T) -> U,
    {
        Request {
            message: f(self.message),
            metadata: self.metadata,
            deadline: self.deadline,
        }
    }
}

/// gRPC response.
#[derive(Debug, Clone)]
pub struct Response<T> {
    /// Response message.
    pub message: T,
    /// Metadata.
    pub metadata: Metadata,
    /// Trailing metadata.
    pub trailing_metadata: Metadata,
}

impl<T> Response<T> {
    /// Create a new response.
    pub fn new(message: T) -> Self {
        Self {
            message,
            metadata: Metadata::new(),
            trailing_metadata: Metadata::new(),
        }
    }

    /// Add metadata.
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key, value);
        self
    }

    /// Map message type.
    pub fn map<U, F>(self, f: F) -> Response<U>
    where
        F: FnOnce(T) -> U,
    {
        Response {
            message: f(self.message),
            metadata: self.metadata,
            trailing_metadata: self.trailing_metadata,
        }
    }
}

/// Streaming request.
pub struct Streaming<T> {
    receiver: mpsc::Receiver<Result<T>>,
}

impl<T> Streaming<T> {
    /// Create from receiver.
    pub fn new(receiver: mpsc::Receiver<Result<T>>) -> Self {
        Self { receiver }
    }

    /// Get next message.
    pub async fn message(&mut self) -> Option<Result<T>> {
        self.receiver.recv().await
    }
}

/// Service method kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MethodKind {
    /// Unary: single request, single response.
    Unary,
    /// Server streaming: single request, stream of responses.
    ServerStreaming,
    /// Client streaming: stream of requests, single response.
    ClientStreaming,
    /// Bidirectional streaming: stream of requests, stream of responses.
    BidirectionalStreaming,
}

/// Method descriptor.
#[derive(Debug, Clone)]
pub struct MethodDescriptor {
    /// Full method name.
    pub name: String,
    /// Service name.
    pub service: String,
    /// Method kind.
    pub kind: MethodKind,
    /// Input type.
    pub input_type: String,
    /// Output type.
    pub output_type: String,
}

impl MethodDescriptor {
    /// Create a new descriptor.
    pub fn new(service: impl Into<String>, name: impl Into<String>, kind: MethodKind) -> Self {
        let service = service.into();
        let name = name.into();
        Self {
            name: format!("/{}/{}", service, name),
            service,
            kind,
            input_type: String::new(),
            output_type: String::new(),
        }
    }
}

/// Service descriptor.
#[derive(Debug, Clone)]
pub struct ServiceDescriptor {
    /// Service name.
    pub name: String,
    /// Package name.
    pub package: String,
    /// Methods.
    pub methods: Vec<MethodDescriptor>,
}

impl ServiceDescriptor {
    /// Create a new descriptor.
    pub fn new(package: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            package: package.into(),
            methods: Vec::new(),
        }
    }

    /// Add method.
    pub fn with_method(mut self, method: MethodDescriptor) -> Self {
        self.methods.push(method);
        self
    }

    /// Full service name.
    pub fn full_name(&self) -> String {
        format!("{}.{}", self.package, self.name)
    }
}

/// Interceptor trait.
#[async_trait]
pub trait Interceptor: Send + Sync {
    /// Intercept request.
    async fn intercept(&self, request: Request<Bytes>) -> Result<Request<Bytes>>;
}

/// Logging interceptor.
pub struct LoggingInterceptor;

#[async_trait]
impl Interceptor for LoggingInterceptor {
    async fn intercept(&self, request: Request<Bytes>) -> Result<Request<Bytes>> {
        // Log the request (in real impl would use tracing)
        Ok(request)
    }
}

/// Auth interceptor.
pub struct AuthInterceptor {
    token: String,
}

impl AuthInterceptor {
    /// Create a new auth interceptor.
    pub fn new(token: impl Into<String>) -> Self {
        Self {
            token: token.into(),
        }
    }
}

#[async_trait]
impl Interceptor for AuthInterceptor {
    async fn intercept(&self, mut request: Request<Bytes>) -> Result<Request<Bytes>> {
        request
            .metadata
            .insert("authorization", format!("Bearer {}", self.token));
        Ok(request)
    }
}

/// gRPC channel.
pub struct Channel {
    /// Endpoint.
    pub endpoint: String,
    /// Interceptors.
    interceptors: Vec<Arc<dyn Interceptor>>,
}

impl Channel {
    /// Create a new channel.
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            interceptors: Vec::new(),
        }
    }

    /// Add interceptor.
    pub fn with_interceptor(mut self, interceptor: Arc<dyn Interceptor>) -> Self {
        self.interceptors.push(interceptor);
        self
    }
}

/// Health check response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckResponse {
    /// Service status.
    pub status: ServingStatus,
}

/// Serving status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ServingStatus {
    Unknown,
    Serving,
    NotServing,
    ServiceUnknown,
}

/// Health service.
pub struct HealthService {
    statuses: RwLock<HashMap<String, ServingStatus>>,
}

impl HealthService {
    /// Create a new health service.
    pub fn new() -> Self {
        Self {
            statuses: RwLock::new(HashMap::new()),
        }
    }

    /// Set service status.
    pub async fn set_status(&self, service: impl Into<String>, status: ServingStatus) {
        let mut statuses = self.statuses.write().await;
        statuses.insert(service.into(), status);
    }

    /// Check service health.
    pub async fn check(&self, service: &str) -> HealthCheckResponse {
        let statuses = self.statuses.read().await;
        let status = statuses
            .get(service)
            .copied()
            .unwrap_or(ServingStatus::ServiceUnknown);
        HealthCheckResponse { status }
    }
}

impl Default for HealthService {
    fn default() -> Self {
        Self::new()
    }
}

/// Reflection service for server reflection.
pub struct ReflectionService {
    services: RwLock<HashMap<String, ServiceDescriptor>>,
}

impl ReflectionService {
    /// Create a new reflection service.
    pub fn new() -> Self {
        Self {
            services: RwLock::new(HashMap::new()),
        }
    }

    /// Register a service.
    pub async fn register(&self, descriptor: ServiceDescriptor) {
        let mut services = self.services.write().await;
        services.insert(descriptor.full_name(), descriptor);
    }

    /// List services.
    pub async fn list_services(&self) -> Vec<String> {
        let services = self.services.read().await;
        services.keys().cloned().collect()
    }

    /// Get service descriptor.
    pub async fn get_service(&self, name: &str) -> Option<ServiceDescriptor> {
        let services = self.services.read().await;
        services.get(name).cloned()
    }
}

impl Default for ReflectionService {
    fn default() -> Self {
        Self::new()
    }
}

/// Server builder.
pub struct ServerBuilder {
    services: Vec<ServiceDescriptor>,
    interceptors: Vec<Arc<dyn Interceptor>>,
    addr: String,
}

impl ServerBuilder {
    /// Create a new builder.
    pub fn new() -> Self {
        Self {
            services: Vec::new(),
            interceptors: Vec::new(),
            addr: "[::1]:50051".to_string(),
        }
    }

    /// Add service.
    pub fn add_service(mut self, service: ServiceDescriptor) -> Self {
        self.services.push(service);
        self
    }

    /// Add interceptor.
    pub fn add_interceptor(mut self, interceptor: Arc<dyn Interceptor>) -> Self {
        self.interceptors.push(interceptor);
        self
    }

    /// Set address.
    pub fn addr(mut self, addr: impl Into<String>) -> Self {
        self.addr = addr.into();
        self
    }
}

impl Default for ServerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Call options.
#[derive(Debug, Clone, Default)]
pub struct CallOptions {
    /// Timeout in milliseconds.
    pub timeout_ms: Option<u64>,
    /// Wait for ready.
    pub wait_for_ready: bool,
    /// Compression.
    pub compression: Option<String>,
}

impl CallOptions {
    /// Create new options.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set timeout.
    pub fn with_timeout(mut self, ms: u64) -> Self {
        self.timeout_ms = Some(ms);
        self
    }

    /// Wait for ready.
    pub fn wait_for_ready(mut self) -> Self {
        self.wait_for_ready = true;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metadata() {
        let mut metadata = Metadata::new();
        metadata.insert("authorization", "Bearer token");
        metadata.insert("x-request-id", "123");

        assert_eq!(metadata.get("authorization"), Some("Bearer token"));
        assert!(metadata.contains("x-request-id"));
    }

    #[test]
    fn test_request_builder() {
        let request = Request::new("Hello")
            .with_metadata("key", "value")
            .with_deadline(Utc::now());

        assert_eq!(request.message, "Hello");
        assert_eq!(request.metadata.get("key"), Some("value"));
    }

    #[test]
    fn test_response_builder() {
        let response = Response::new("World").with_metadata("x-trace-id", "abc");

        assert_eq!(response.message, "World");
    }

    #[test]
    fn test_grpc_error_codes() {
        assert_eq!(GrpcError::Ok.code(), 0);
        assert_eq!(GrpcError::NotFound("".to_string()).code(), 5);
        assert_eq!(GrpcError::Unauthenticated.code(), 16);
    }

    #[test]
    fn test_method_descriptor() {
        let desc = MethodDescriptor::new("Greeter", "SayHello", MethodKind::Unary);
        assert_eq!(desc.name, "/Greeter/SayHello");
    }

    #[test]
    fn test_service_descriptor() {
        let desc = ServiceDescriptor::new("helloworld", "Greeter").with_method(
            MethodDescriptor::new("Greeter", "SayHello", MethodKind::Unary),
        );

        assert_eq!(desc.full_name(), "helloworld.Greeter");
        assert_eq!(desc.methods.len(), 1);
    }

    #[tokio::test]
    async fn test_health_service() {
        let health = HealthService::new();

        health
            .set_status("my.Service", ServingStatus::Serving)
            .await;

        let response = health.check("my.Service").await;
        assert_eq!(response.status, ServingStatus::Serving);

        let unknown = health.check("unknown").await;
        assert_eq!(unknown.status, ServingStatus::ServiceUnknown);
    }

    #[tokio::test]
    async fn test_reflection_service() {
        let reflection = ReflectionService::new();

        reflection
            .register(ServiceDescriptor::new("pkg", "Svc"))
            .await;

        let services = reflection.list_services().await;
        assert!(services.contains(&"pkg.Svc".to_string()));
    }

    #[test]
    fn test_call_options() {
        let options = CallOptions::new().with_timeout(5000).wait_for_ready();

        assert_eq!(options.timeout_ms, Some(5000));
        assert!(options.wait_for_ready);
    }
}
