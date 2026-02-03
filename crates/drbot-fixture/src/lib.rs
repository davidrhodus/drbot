//! Test fixture utilities for drbot.
//!
//! This crate provides:
//! - Fixture management
//! - Test data generation
//! - Setup/teardown helpers
//! - Fixture loading

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use thiserror::Error;

/// Fixture error types.
#[derive(Error, Debug)]
pub enum FixtureError {
    #[error("Fixture not found: {0}")]
    NotFound(String),

    #[error("Fixture load failed: {0}")]
    LoadFailed(String),

    #[error("Setup failed: {0}")]
    SetupFailed(String),

    #[error("Teardown failed: {0}")]
    TeardownFailed(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Result type for fixture operations.
pub type Result<T> = std::result::Result<T, FixtureError>;

/// Fixture data.
#[derive(Debug, Clone)]
pub struct Fixture {
    /// Fixture name.
    pub name: String,
    /// Fixture data.
    pub data: Vec<u8>,
    /// Content type.
    pub content_type: Option<String>,
}

impl Fixture {
    /// Create new fixture.
    pub fn new(name: impl Into<String>, data: Vec<u8>) -> Self {
        Self {
            name: name.into(),
            data,
            content_type: None,
        }
    }

    /// Create from string.
    pub fn from_str(name: impl Into<String>, data: &str) -> Self {
        Self::new(name, data.as_bytes().to_vec())
    }

    /// Set content type.
    pub fn content_type(mut self, content_type: impl Into<String>) -> Self {
        self.content_type = Some(content_type.into());
        self
    }

    /// Get as string.
    pub fn as_str(&self) -> Option<&str> {
        std::str::from_utf8(&self.data).ok()
    }

    /// Get as bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.data
    }
}

/// Fixture registry.
pub struct FixtureRegistry {
    fixtures: Mutex<HashMap<String, Fixture>>,
    base_path: Option<PathBuf>,
}

impl FixtureRegistry {
    /// Create new registry.
    pub fn new() -> Self {
        Self {
            fixtures: Mutex::new(HashMap::new()),
            base_path: None,
        }
    }

    /// Create with base path.
    pub fn with_base_path(path: impl Into<PathBuf>) -> Self {
        Self {
            fixtures: Mutex::new(HashMap::new()),
            base_path: Some(path.into()),
        }
    }

    /// Register fixture.
    pub fn register(&self, fixture: Fixture) {
        self.fixtures
            .lock()
            .unwrap()
            .insert(fixture.name.clone(), fixture);
    }

    /// Register fixture from string.
    pub fn register_str(&self, name: impl Into<String>, data: &str) {
        self.register(Fixture::from_str(name, data));
    }

    /// Get fixture.
    pub fn get(&self, name: &str) -> Option<Fixture> {
        self.fixtures.lock().unwrap().get(name).cloned()
    }

    /// Load fixture from file.
    pub fn load_file(&self, name: &str, path: impl AsRef<Path>) -> Result<Fixture> {
        let full_path = if let Some(ref base) = self.base_path {
            base.join(path.as_ref())
        } else {
            path.as_ref().to_path_buf()
        };

        let data = std::fs::read(&full_path)?;
        let fixture = Fixture::new(name, data);
        self.register(fixture.clone());
        Ok(fixture)
    }

    /// Get or load fixture.
    pub fn get_or_load(&self, name: &str, path: impl AsRef<Path>) -> Result<Fixture> {
        if let Some(fixture) = self.get(name) {
            Ok(fixture)
        } else {
            self.load_file(name, path)
        }
    }

    /// Clear all fixtures.
    pub fn clear(&self) {
        self.fixtures.lock().unwrap().clear();
    }

    /// List fixture names.
    pub fn names(&self) -> Vec<String> {
        self.fixtures.lock().unwrap().keys().cloned().collect()
    }
}

impl Default for FixtureRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Setup/teardown context.
pub struct TestContext {
    setup_actions: Mutex<Vec<Box<dyn FnOnce() -> Result<()> + Send>>>,
    teardown_actions: Mutex<Vec<Box<dyn FnOnce() + Send>>>,
    fixtures: FixtureRegistry,
    data: Mutex<HashMap<String, Box<dyn std::any::Any + Send + Sync>>>,
}

impl TestContext {
    /// Create new context.
    pub fn new() -> Self {
        Self {
            setup_actions: Mutex::new(Vec::new()),
            teardown_actions: Mutex::new(Vec::new()),
            fixtures: FixtureRegistry::new(),
            data: Mutex::new(HashMap::new()),
        }
    }

    /// Add setup action.
    pub fn on_setup<F>(&self, action: F)
    where
        F: FnOnce() -> Result<()> + Send + 'static,
    {
        self.setup_actions.lock().unwrap().push(Box::new(action));
    }

    /// Add teardown action.
    pub fn on_teardown<F>(&self, action: F)
    where
        F: FnOnce() + Send + 'static,
    {
        self.teardown_actions.lock().unwrap().push(Box::new(action));
    }

    /// Run setup.
    pub fn setup(&self) -> Result<()> {
        let actions: Vec<_> = self.setup_actions.lock().unwrap().drain(..).collect();
        for action in actions {
            action()?;
        }
        Ok(())
    }

    /// Run teardown.
    pub fn teardown(&self) {
        let actions: Vec<_> = self.teardown_actions.lock().unwrap().drain(..).collect();
        for action in actions.into_iter().rev() {
            action();
        }
    }

    /// Get fixtures.
    pub fn fixtures(&self) -> &FixtureRegistry {
        &self.fixtures
    }

    /// Store data.
    pub fn set<T: Send + Sync + 'static>(&self, key: impl Into<String>, value: T) {
        self.data
            .lock()
            .unwrap()
            .insert(key.into(), Box::new(value));
    }

    /// Get data.
    pub fn get<T: Clone + 'static>(&self, key: &str) -> Option<T> {
        self.data
            .lock()
            .unwrap()
            .get(key)
            .and_then(|v| v.downcast_ref::<T>())
            .cloned()
    }
}

impl Default for TestContext {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for TestContext {
    fn drop(&mut self) {
        self.teardown();
    }
}

/// Test data builder.
pub struct DataBuilder<T> {
    template: T,
    modifiers: Vec<Box<dyn FnOnce(T) -> T>>,
}

impl<T: Clone> DataBuilder<T> {
    /// Create new builder.
    pub fn new(template: T) -> Self {
        Self {
            template,
            modifiers: Vec::new(),
        }
    }

    /// Add modifier.
    pub fn with<F>(mut self, modifier: F) -> Self
    where
        F: FnOnce(T) -> T + 'static,
    {
        self.modifiers.push(Box::new(modifier));
        self
    }

    /// Build the value.
    pub fn build(self) -> T {
        let mut value = self.template;
        for modifier in self.modifiers {
            value = modifier(value);
        }
        value
    }

    /// Build multiple values.
    pub fn build_many(self, count: usize) -> Vec<T> {
        (0..count).map(|_| self.template.clone()).collect()
    }
}

/// Fixture scope guard.
pub struct FixtureScope {
    context: Arc<TestContext>,
}

impl FixtureScope {
    /// Create new scope.
    pub fn new() -> Self {
        Self {
            context: Arc::new(TestContext::new()),
        }
    }

    /// Get context.
    pub fn context(&self) -> &TestContext {
        &self.context
    }

    /// Run with scope.
    pub fn run<F, R>(self, f: F) -> Result<R>
    where
        F: FnOnce(&TestContext) -> R,
    {
        self.context.setup()?;
        let result = f(&self.context);
        self.context.teardown();
        Ok(result)
    }
}

impl Default for FixtureScope {
    fn default() -> Self {
        Self::new()
    }
}

/// Temporary directory fixture.
pub struct TempDir {
    path: PathBuf,
    cleanup: bool,
}

impl TempDir {
    /// Create new temp directory.
    pub fn new(prefix: &str) -> Result<Self> {
        let path = std::env::temp_dir().join(format!(
            "{}-{:x}",
            prefix,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&path)?;
        Ok(Self {
            path,
            cleanup: true,
        })
    }

    /// Get path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Join path.
    pub fn join(&self, path: impl AsRef<Path>) -> PathBuf {
        self.path.join(path)
    }

    /// Write file.
    pub fn write_file(&self, name: &str, data: &[u8]) -> Result<PathBuf> {
        let path = self.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, data)?;
        Ok(path)
    }

    /// Disable cleanup.
    pub fn keep(mut self) -> Self {
        self.cleanup = false;
        self
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        if self.cleanup {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fixture_registry() {
        let registry = FixtureRegistry::new();
        registry.register_str("test", "hello world");

        let fixture = registry.get("test").unwrap();
        assert_eq!(fixture.as_str(), Some("hello world"));
    }

    #[test]
    fn test_test_context() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let context = TestContext::new();
        let counter = Arc::new(AtomicUsize::new(0));

        let c1 = counter.clone();
        context.on_setup(move || {
            c1.fetch_add(1, Ordering::SeqCst);
            Ok(())
        });

        let c2 = counter.clone();
        context.on_teardown(move || {
            c2.fetch_add(10, Ordering::SeqCst);
        });

        context.setup().unwrap();
        assert_eq!(counter.load(Ordering::SeqCst), 1);

        context.teardown();
        assert_eq!(counter.load(Ordering::SeqCst), 11);
    }

    #[test]
    fn test_data_builder() {
        #[derive(Clone)]
        struct User {
            name: String,
            age: u32,
        }

        let template = User {
            name: "Test".to_string(),
            age: 0,
        };

        let user = DataBuilder::new(template)
            .with(|mut u| {
                u.name = "Alice".to_string();
                u
            })
            .with(|mut u| {
                u.age = 30;
                u
            })
            .build();

        assert_eq!(user.name, "Alice");
        assert_eq!(user.age, 30);
    }

    #[test]
    fn test_temp_dir() {
        let temp = TempDir::new("test").unwrap();
        let file_path = temp.write_file("test.txt", b"hello").unwrap();

        assert!(file_path.exists());
        assert_eq!(std::fs::read(&file_path).unwrap(), b"hello");

        let path = temp.path().to_path_buf();
        drop(temp);

        assert!(!path.exists());
    }
}
