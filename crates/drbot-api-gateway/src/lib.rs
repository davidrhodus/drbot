//! API Gateway for drbot.
//!
//! This crate provides:
//! - Request routing
//! - Service aggregation
//! - Authentication/Authorization
//! - Rate limiting
//! - Request/Response transformation

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::RwLock;
use uuid::Uuid;

/// API Gateway error types.
#[derive(Error, Debug)]
pub enum GatewayError {
    #[error("Route not found: {0}")]
    RouteNotFound(String),

    #[error("Service unavailable: {0}")]
    ServiceUnavailable(String),

    #[error("Authentication required")]
    AuthRequired,

    #[error("Authorization denied")]
    AuthDenied,

    #[error("Rate limit exceeded")]
    RateLimitExceeded,

    #[error("Request timeout")]
    Timeout,

    #[error("Upstream error: {0}")]
    UpstreamError(String),

    #[error("Transform error: {0}")]
    TransformError(String),
}

/// Result type for gateway operations.
pub type Result<T> = std::result::Result<T, GatewayError>;

/// HTTP method.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Method {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Head,
    Options,
}

/// Gateway request.
#[derive(Debug, Clone)]
pub struct GatewayRequest {
    /// Request ID.
    pub id: Uuid,
    /// HTTP method.
    pub method: Method,
    /// Path.
    pub path: String,
    /// Headers.
    pub headers: HashMap<String, String>,
    /// Query parameters.
    pub query: HashMap<String, String>,
    /// Body.
    pub body: Option<Vec<u8>>,
    /// Client IP.
    pub client_ip: Option<String>,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
}

impl GatewayRequest {
    /// Create a new request.
    pub fn new(method: Method, path: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            method,
            path: path.into(),
            headers: HashMap::new(),
            query: HashMap::new(),
            body: None,
            client_ip: None,
            timestamp: Utc::now(),
        }
    }

    /// Add header.
    pub fn with_header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(key.into(), value.into());
        self
    }

    /// Add query parameter.
    pub fn with_query(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.query.insert(key.into(), value.into());
        self
    }

    /// Set body.
    pub fn with_body(mut self, body: Vec<u8>) -> Self {
        self.body = Some(body);
        self
    }

    /// Set JSON body.
    pub fn with_json<T: Serialize>(self, data: &T) -> Result<Self> {
        let body =
            serde_json::to_vec(data).map_err(|e| GatewayError::TransformError(e.to_string()))?;
        Ok(self
            .with_body(body)
            .with_header("Content-Type", "application/json"))
    }
}

/// Gateway response.
#[derive(Debug, Clone)]
pub struct GatewayResponse {
    /// Status code.
    pub status: u16,
    /// Headers.
    pub headers: HashMap<String, String>,
    /// Body.
    pub body: Option<Vec<u8>>,
    /// Upstream service.
    pub upstream: Option<String>,
    /// Response time.
    pub response_time_ms: u64,
}

impl GatewayResponse {
    /// Create a new response.
    pub fn new(status: u16) -> Self {
        Self {
            status,
            headers: HashMap::new(),
            body: None,
            upstream: None,
            response_time_ms: 0,
        }
    }

    /// Set body.
    pub fn with_body(mut self, body: Vec<u8>) -> Self {
        self.body = Some(body);
        self
    }

    /// Add header.
    pub fn with_header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(key.into(), value.into());
        self
    }

    /// Parse JSON body.
    pub fn json<T: for<'de> Deserialize<'de>>(&self) -> Result<T> {
        let body = self
            .body
            .as_ref()
            .ok_or_else(|| GatewayError::TransformError("No body".to_string()))?;
        serde_json::from_slice(body).map_err(|e| GatewayError::TransformError(e.to_string()))
    }
}

/// Route definition.
#[derive(Debug, Clone)]
pub struct Route {
    /// Route ID.
    pub id: String,
    /// Path pattern.
    pub path: String,
    /// Allowed methods.
    pub methods: Vec<Method>,
    /// Upstream service.
    pub upstream: String,
    /// Upstream path prefix.
    pub upstream_prefix: Option<String>,
    /// Strip path prefix.
    pub strip_prefix: Option<String>,
    /// Timeout.
    pub timeout: Duration,
    /// Require authentication.
    pub require_auth: bool,
    /// Required roles.
    pub required_roles: Vec<String>,
    /// Rate limit (requests per second).
    pub rate_limit: Option<u32>,
}

impl Route {
    /// Create a new route.
    pub fn new(
        id: impl Into<String>,
        path: impl Into<String>,
        upstream: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            path: path.into(),
            methods: vec![
                Method::Get,
                Method::Post,
                Method::Put,
                Method::Patch,
                Method::Delete,
            ],
            upstream: upstream.into(),
            upstream_prefix: None,
            strip_prefix: None,
            timeout: Duration::from_secs(30),
            require_auth: false,
            required_roles: Vec::new(),
            rate_limit: None,
        }
    }

    /// Set allowed methods.
    pub fn with_methods(mut self, methods: Vec<Method>) -> Self {
        self.methods = methods;
        self
    }

    /// Require authentication.
    pub fn require_auth(mut self) -> Self {
        self.require_auth = true;
        self
    }

    /// Require roles.
    pub fn with_roles(mut self, roles: Vec<String>) -> Self {
        self.required_roles = roles;
        self.require_auth = true;
        self
    }

    /// Set rate limit.
    pub fn with_rate_limit(mut self, rps: u32) -> Self {
        self.rate_limit = Some(rps);
        self
    }

    /// Check if path matches.
    pub fn matches(&self, path: &str, method: Method) -> bool {
        if !self.methods.contains(&method) {
            return false;
        }

        // Simple prefix matching (could be extended with regex)
        if self.path.ends_with("/*") {
            let prefix = &self.path[..self.path.len() - 2];
            path.starts_with(prefix)
        } else {
            path == self.path
        }
    }
}

/// Upstream service.
#[derive(Debug, Clone)]
pub struct Upstream {
    /// Service ID.
    pub id: String,
    /// Service name.
    pub name: String,
    /// Base URL.
    pub base_url: String,
    /// Health check path.
    pub health_path: Option<String>,
    /// Is healthy.
    pub healthy: bool,
    /// Last check.
    pub last_check: Option<DateTime<Utc>>,
}

impl Upstream {
    /// Create a new upstream.
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        base_url: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            base_url: base_url.into(),
            health_path: Some("/health".to_string()),
            healthy: true,
            last_check: None,
        }
    }
}

/// Authentication context.
#[derive(Debug, Clone)]
pub struct AuthContext {
    /// User ID.
    pub user_id: Option<String>,
    /// Roles.
    pub roles: Vec<String>,
    /// Claims.
    pub claims: HashMap<String, serde_json::Value>,
    /// Is authenticated.
    pub authenticated: bool,
}

impl AuthContext {
    /// Create anonymous context.
    pub fn anonymous() -> Self {
        Self {
            user_id: None,
            roles: Vec::new(),
            claims: HashMap::new(),
            authenticated: false,
        }
    }

    /// Create authenticated context.
    pub fn authenticated(user_id: impl Into<String>) -> Self {
        Self {
            user_id: Some(user_id.into()),
            roles: Vec::new(),
            claims: HashMap::new(),
            authenticated: true,
        }
    }

    /// Add role.
    pub fn with_role(mut self, role: impl Into<String>) -> Self {
        self.roles.push(role.into());
        self
    }

    /// Has role.
    pub fn has_role(&self, role: &str) -> bool {
        self.roles.contains(&role.to_string())
    }

    /// Has any role.
    pub fn has_any_role(&self, roles: &[String]) -> bool {
        roles.iter().any(|r| self.has_role(r))
    }
}

/// Authenticator trait.
#[async_trait]
pub trait Authenticator: Send + Sync {
    /// Authenticate a request.
    async fn authenticate(&self, request: &GatewayRequest) -> Result<AuthContext>;
}

/// No-op authenticator.
pub struct NoOpAuthenticator;

#[async_trait]
impl Authenticator for NoOpAuthenticator {
    async fn authenticate(&self, _request: &GatewayRequest) -> Result<AuthContext> {
        Ok(AuthContext::anonymous())
    }
}

/// Request transformer trait.
#[async_trait]
pub trait RequestTransformer: Send + Sync {
    /// Transform request.
    async fn transform(
        &self,
        request: GatewayRequest,
        auth: &AuthContext,
    ) -> Result<GatewayRequest>;
}

/// Response transformer trait.
#[async_trait]
pub trait ResponseTransformer: Send + Sync {
    /// Transform response.
    async fn transform(&self, response: GatewayResponse) -> Result<GatewayResponse>;
}

/// Upstream handler trait.
#[async_trait]
pub trait UpstreamHandler: Send + Sync {
    /// Forward request to upstream.
    async fn forward(
        &self,
        upstream: &Upstream,
        request: GatewayRequest,
    ) -> Result<GatewayResponse>;
}

/// Simple upstream handler using reqwest.
pub struct HttpUpstreamHandler {
    client: reqwest::Client,
}

impl HttpUpstreamHandler {
    /// Create a new handler.
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }
}

impl Default for HttpUpstreamHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl UpstreamHandler for HttpUpstreamHandler {
    async fn forward(
        &self,
        upstream: &Upstream,
        request: GatewayRequest,
    ) -> Result<GatewayResponse> {
        let url = format!("{}{}", upstream.base_url, request.path);

        let method = match request.method {
            Method::Get => reqwest::Method::GET,
            Method::Post => reqwest::Method::POST,
            Method::Put => reqwest::Method::PUT,
            Method::Patch => reqwest::Method::PATCH,
            Method::Delete => reqwest::Method::DELETE,
            Method::Head => reqwest::Method::HEAD,
            Method::Options => reqwest::Method::OPTIONS,
        };

        let start = std::time::Instant::now();

        let mut req = self.client.request(method, &url);

        for (key, value) in &request.headers {
            req = req.header(key, value);
        }

        if let Some(body) = request.body {
            req = req.body(body);
        }

        let resp = req
            .send()
            .await
            .map_err(|e| GatewayError::UpstreamError(e.to_string()))?;

        let status = resp.status().as_u16();
        let headers: HashMap<String, String> = resp
            .headers()
            .iter()
            .filter_map(|(k, v)| v.to_str().ok().map(|v| (k.to_string(), v.to_string())))
            .collect();

        let body = resp
            .bytes()
            .await
            .map_err(|e| GatewayError::UpstreamError(e.to_string()))?;

        Ok(GatewayResponse {
            status,
            headers,
            body: Some(body.to_vec()),
            upstream: Some(upstream.id.clone()),
            response_time_ms: start.elapsed().as_millis() as u64,
        })
    }
}

/// API Gateway.
pub struct ApiGateway<A: Authenticator, U: UpstreamHandler> {
    routes: RwLock<Vec<Route>>,
    upstreams: RwLock<HashMap<String, Upstream>>,
    authenticator: Arc<A>,
    upstream_handler: Arc<U>,
    request_transformers: RwLock<Vec<Arc<dyn RequestTransformer>>>,
    response_transformers: RwLock<Vec<Arc<dyn ResponseTransformer>>>,
}

impl<A: Authenticator, U: UpstreamHandler> ApiGateway<A, U> {
    /// Create a new gateway.
    pub fn new(authenticator: Arc<A>, upstream_handler: Arc<U>) -> Self {
        Self {
            routes: RwLock::new(Vec::new()),
            upstreams: RwLock::new(HashMap::new()),
            authenticator,
            upstream_handler,
            request_transformers: RwLock::new(Vec::new()),
            response_transformers: RwLock::new(Vec::new()),
        }
    }

    /// Add a route.
    pub async fn add_route(&self, route: Route) {
        let mut routes = self.routes.write().await;
        routes.push(route);
    }

    /// Add an upstream.
    pub async fn add_upstream(&self, upstream: Upstream) {
        let mut upstreams = self.upstreams.write().await;
        upstreams.insert(upstream.id.clone(), upstream);
    }

    /// Add request transformer.
    pub async fn add_request_transformer(&self, transformer: Arc<dyn RequestTransformer>) {
        let mut transformers = self.request_transformers.write().await;
        transformers.push(transformer);
    }

    /// Add response transformer.
    pub async fn add_response_transformer(&self, transformer: Arc<dyn ResponseTransformer>) {
        let mut transformers = self.response_transformers.write().await;
        transformers.push(transformer);
    }

    /// Handle a request.
    pub async fn handle(&self, request: GatewayRequest) -> Result<GatewayResponse> {
        // Find matching route
        let routes = self.routes.read().await;
        let route = routes
            .iter()
            .find(|r| r.matches(&request.path, request.method))
            .ok_or_else(|| GatewayError::RouteNotFound(request.path.clone()))?
            .clone();
        drop(routes);

        // Authenticate
        let auth = self.authenticator.authenticate(&request).await?;

        // Check auth requirements
        if route.require_auth && !auth.authenticated {
            return Err(GatewayError::AuthRequired);
        }

        if !route.required_roles.is_empty() && !auth.has_any_role(&route.required_roles) {
            return Err(GatewayError::AuthDenied);
        }

        // Transform request
        let mut request = request;
        let transformers = self.request_transformers.read().await;
        for transformer in transformers.iter() {
            request = transformer.transform(request, &auth).await?;
        }
        drop(transformers);

        // Get upstream
        let upstreams = self.upstreams.read().await;
        let upstream = upstreams
            .get(&route.upstream)
            .ok_or_else(|| GatewayError::ServiceUnavailable(route.upstream.clone()))?
            .clone();
        drop(upstreams);

        if !upstream.healthy {
            return Err(GatewayError::ServiceUnavailable(upstream.name));
        }

        // Forward request
        let response = self.upstream_handler.forward(&upstream, request).await?;

        // Transform response
        let mut response = response;
        let transformers = self.response_transformers.read().await;
        for transformer in transformers.iter() {
            response = transformer.transform(response).await?;
        }

        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_route_matching() {
        let route = Route::new("test", "/api/users/*", "users-service");

        assert!(route.matches("/api/users/123", Method::Get));
        assert!(route.matches("/api/users/123/profile", Method::Get));
        assert!(!route.matches("/api/orders", Method::Get));
    }

    #[test]
    fn test_exact_route_matching() {
        let route = Route::new("test", "/api/health", "health-service");

        assert!(route.matches("/api/health", Method::Get));
        assert!(!route.matches("/api/health/check", Method::Get));
    }

    #[test]
    fn test_method_filtering() {
        let route = Route::new("test", "/api/users", "users-service")
            .with_methods(vec![Method::Get, Method::Post]);

        assert!(route.matches("/api/users", Method::Get));
        assert!(route.matches("/api/users", Method::Post));
        assert!(!route.matches("/api/users", Method::Delete));
    }

    #[test]
    fn test_auth_context() {
        let auth = AuthContext::authenticated("user-123")
            .with_role("admin")
            .with_role("user");

        assert!(auth.authenticated);
        assert!(auth.has_role("admin"));
        assert!(auth.has_role("user"));
        assert!(!auth.has_role("superadmin"));
    }

    #[test]
    fn test_request_builder() {
        let request = GatewayRequest::new(Method::Post, "/api/users")
            .with_header("Authorization", "Bearer token")
            .with_query("page", "1");

        assert_eq!(request.method, Method::Post);
        assert_eq!(request.path, "/api/users");
        assert_eq!(
            request.headers.get("Authorization"),
            Some(&"Bearer token".to_string())
        );
    }

    #[test]
    fn test_response_builder() {
        let response = GatewayResponse::new(200)
            .with_header("Content-Type", "application/json")
            .with_body(b"{}".to_vec());

        assert_eq!(response.status, 200);
        assert!(response.body.is_some());
    }

    #[tokio::test]
    async fn test_gateway_route_not_found() {
        let auth = Arc::new(NoOpAuthenticator);
        let handler = Arc::new(HttpUpstreamHandler::new());
        let gateway = ApiGateway::new(auth, handler);

        let request = GatewayRequest::new(Method::Get, "/not-found");
        let result = gateway.handle(request).await;

        assert!(matches!(result, Err(GatewayError::RouteNotFound(_))));
    }
}
