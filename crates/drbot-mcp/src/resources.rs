//! MCP resource definitions and registry.

use crate::protocol::{ReadResourceResult, ResourceContent, ResourceDefinition};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;

/// Trait for implementing MCP resource providers.
#[async_trait]
pub trait Resource: Send + Sync {
    /// Resource URI.
    fn uri(&self) -> &str;

    /// Resource name.
    fn name(&self) -> &str;

    /// Resource description.
    fn description(&self) -> Option<&str>;

    /// MIME type of the resource.
    fn mime_type(&self) -> Option<&str>;

    /// Read the resource content.
    async fn read(&self) -> Result<ResourceContent, String>;
}

/// Registry for MCP resources.
pub struct ResourceRegistry {
    resources: HashMap<String, Arc<dyn Resource>>,
}

impl ResourceRegistry {
    /// Create a new resource registry.
    pub fn new() -> Self {
        Self {
            resources: HashMap::new(),
        }
    }

    /// Register a resource.
    pub fn register(&mut self, resource: Arc<dyn Resource>) {
        self.resources.insert(resource.uri().to_string(), resource);
    }

    /// Get a resource by URI.
    pub fn get(&self, uri: &str) -> Option<&Arc<dyn Resource>> {
        self.resources.get(uri)
    }

    /// List all resources.
    pub fn list(&self) -> impl Iterator<Item = ResourceDefinition> + '_ {
        self.resources.values().map(|r| ResourceDefinition {
            uri: r.uri().to_string(),
            name: r.name().to_string(),
            description: r.description().map(|s| s.to_string()),
            mime_type: r.mime_type().map(|s| s.to_string()),
        })
    }

    /// Read a resource by URI.
    pub async fn read(&self, uri: &str) -> Result<ReadResourceResult, String> {
        let resource = self
            .resources
            .get(uri)
            .ok_or_else(|| format!("Resource not found: {}", uri))?;
        let content = resource.read().await?;
        Ok(ReadResourceResult {
            contents: vec![content],
        })
    }
}

impl Default for ResourceRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// A simple static text resource.
pub struct TextResource {
    uri: String,
    name: String,
    description: Option<String>,
    content: String,
}

impl TextResource {
    /// Create a new text resource.
    pub fn new(
        uri: impl Into<String>,
        name: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        Self {
            uri: uri.into(),
            name: name.into(),
            description: None,
            content: content.into(),
        }
    }

    /// Set the description.
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }
}

#[async_trait]
impl Resource for TextResource {
    fn uri(&self) -> &str {
        &self.uri
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    fn mime_type(&self) -> Option<&str> {
        Some("text/plain")
    }

    async fn read(&self) -> Result<ResourceContent, String> {
        Ok(ResourceContent {
            uri: self.uri.clone(),
            mime_type: Some("text/plain".to_string()),
            text: Some(self.content.clone()),
            blob: None,
        })
    }
}

/// A file-backed resource.
pub struct FileResource {
    uri: String,
    name: String,
    description: Option<String>,
    path: std::path::PathBuf,
    mime_type: Option<String>,
}

impl FileResource {
    /// Create a new file resource.
    pub fn new(
        uri: impl Into<String>,
        name: impl Into<String>,
        path: impl Into<std::path::PathBuf>,
    ) -> Self {
        Self {
            uri: uri.into(),
            name: name.into(),
            description: None,
            path: path.into(),
            mime_type: None,
        }
    }

    /// Set the description.
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Set the MIME type.
    pub fn with_mime_type(mut self, mime_type: impl Into<String>) -> Self {
        self.mime_type = Some(mime_type.into());
        self
    }
}

#[async_trait]
impl Resource for FileResource {
    fn uri(&self) -> &str {
        &self.uri
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    fn mime_type(&self) -> Option<&str> {
        self.mime_type.as_deref()
    }

    async fn read(&self) -> Result<ResourceContent, String> {
        let content = tokio::fs::read_to_string(&self.path)
            .await
            .map_err(|e| format!("Failed to read file: {}", e))?;

        Ok(ResourceContent {
            uri: self.uri.clone(),
            mime_type: self.mime_type.clone(),
            text: Some(content),
            blob: None,
        })
    }
}

/// A dynamic resource that computes content on read.
pub struct DynamicResource<F>
where
    F: Fn() -> Result<String, String> + Send + Sync,
{
    uri: String,
    name: String,
    description: Option<String>,
    mime_type: Option<String>,
    generator: F,
}

impl<F> DynamicResource<F>
where
    F: Fn() -> Result<String, String> + Send + Sync,
{
    /// Create a new dynamic resource.
    pub fn new(uri: impl Into<String>, name: impl Into<String>, generator: F) -> Self {
        Self {
            uri: uri.into(),
            name: name.into(),
            description: None,
            mime_type: None,
            generator,
        }
    }

    /// Set the description.
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Set the MIME type.
    pub fn with_mime_type(mut self, mime_type: impl Into<String>) -> Self {
        self.mime_type = Some(mime_type.into());
        self
    }
}

#[async_trait]
impl<F> Resource for DynamicResource<F>
where
    F: Fn() -> Result<String, String> + Send + Sync,
{
    fn uri(&self) -> &str {
        &self.uri
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    fn mime_type(&self) -> Option<&str> {
        self.mime_type.as_deref()
    }

    async fn read(&self) -> Result<ResourceContent, String> {
        let content = (self.generator)()?;
        Ok(ResourceContent {
            uri: self.uri.clone(),
            mime_type: self.mime_type.clone(),
            text: Some(content),
            blob: None,
        })
    }
}

/// Resource provider trait for discovering resources dynamically.
#[async_trait]
pub trait ResourceProvider: Send + Sync {
    /// List available resources.
    async fn list(&self) -> Vec<ResourceDefinition>;

    /// Read a specific resource.
    async fn read(&self, uri: &str) -> Result<ResourceContent, String>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_text_resource() {
        let resource = TextResource::new("test://hello", "Hello", "Hello, World!");
        assert_eq!(resource.uri(), "test://hello");
        assert_eq!(resource.name(), "Hello");

        let content = resource.read().await.unwrap();
        assert_eq!(content.text, Some("Hello, World!".to_string()));
    }

    #[test]
    fn test_resource_registry() {
        let mut registry = ResourceRegistry::new();
        registry.register(Arc::new(TextResource::new(
            "test://a",
            "Resource A",
            "Content A",
        )));
        registry.register(Arc::new(TextResource::new(
            "test://b",
            "Resource B",
            "Content B",
        )));

        assert!(registry.get("test://a").is_some());
        assert!(registry.get("test://b").is_some());
        assert!(registry.get("test://c").is_none());

        let resources: Vec<_> = registry.list().collect();
        assert_eq!(resources.len(), 2);
    }
}
