//! Facade pattern utilities for drbot.
//!
//! This crate provides:
//! - Facade trait for simplified interfaces
//! - Subsystem coordination
//! - Composite facades

use std::sync::Arc;
use thiserror::Error;

/// Facade error types.
#[derive(Error, Debug)]
pub enum FacadeError {
    #[error("Subsystem error: {0}")]
    SubsystemError(String),

    #[error("Operation failed: {0}")]
    OperationFailed(String),

    #[error("Not initialized")]
    NotInitialized,
}

/// Result type for facade operations.
pub type Result<T> = std::result::Result<T, FacadeError>;

/// Subsystem trait that facades coordinate.
pub trait Subsystem: Send + Sync {
    /// Subsystem name.
    fn name(&self) -> &str;

    /// Initialize subsystem.
    fn init(&mut self) -> Result<()>;

    /// Shutdown subsystem.
    fn shutdown(&mut self) -> Result<()>;

    /// Check if initialized.
    fn is_initialized(&self) -> bool;
}

/// Simple facade that coordinates multiple subsystems.
pub struct SimpleFacade {
    subsystems: Vec<Box<dyn Subsystem>>,
    initialized: bool,
}

impl SimpleFacade {
    /// Create new facade.
    pub fn new() -> Self {
        Self {
            subsystems: Vec::new(),
            initialized: false,
        }
    }

    /// Add subsystem.
    pub fn add_subsystem(&mut self, subsystem: Box<dyn Subsystem>) {
        self.subsystems.push(subsystem);
    }

    /// Initialize all subsystems.
    pub fn init(&mut self) -> Result<()> {
        for subsystem in &mut self.subsystems {
            subsystem
                .init()
                .map_err(|e| FacadeError::SubsystemError(format!("{}: {}", subsystem.name(), e)))?;
        }
        self.initialized = true;
        Ok(())
    }

    /// Shutdown all subsystems.
    pub fn shutdown(&mut self) -> Result<()> {
        // Shutdown in reverse order
        for subsystem in self.subsystems.iter_mut().rev() {
            subsystem
                .shutdown()
                .map_err(|e| FacadeError::SubsystemError(format!("{}: {}", subsystem.name(), e)))?;
        }
        self.initialized = false;
        Ok(())
    }

    /// Check if initialized.
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    /// Get subsystem count.
    pub fn subsystem_count(&self) -> usize {
        self.subsystems.len()
    }

    /// Get subsystem names.
    pub fn subsystem_names(&self) -> Vec<&str> {
        self.subsystems.iter().map(|s| s.name()).collect()
    }
}

impl Default for SimpleFacade {
    fn default() -> Self {
        Self::new()
    }
}

/// Named operation for operation facade.
pub trait NamedOperation: Send + Sync {
    /// Operation name.
    fn name(&self) -> &str;

    /// Execute operation.
    fn execute(&self) -> Result<()>;
}

/// Operation facade that provides named operations.
pub struct OperationFacade {
    operations: std::collections::HashMap<String, Arc<dyn NamedOperation>>,
}

impl OperationFacade {
    /// Create new operation facade.
    pub fn new() -> Self {
        Self {
            operations: std::collections::HashMap::new(),
        }
    }

    /// Register operation.
    pub fn register(&mut self, operation: Arc<dyn NamedOperation>) {
        self.operations
            .insert(operation.name().to_string(), operation);
    }

    /// Execute named operation.
    pub fn execute(&self, name: &str) -> Result<()> {
        let op = self
            .operations
            .get(name)
            .ok_or_else(|| FacadeError::OperationFailed(format!("Unknown operation: {}", name)))?;
        op.execute()
    }

    /// List operation names.
    pub fn operation_names(&self) -> Vec<&str> {
        self.operations.keys().map(|s| s.as_str()).collect()
    }
}

impl Default for OperationFacade {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder for creating facades.
pub struct FacadeBuilder {
    subsystems: Vec<Box<dyn Subsystem>>,
}

impl FacadeBuilder {
    /// Create new builder.
    pub fn new() -> Self {
        Self {
            subsystems: Vec::new(),
        }
    }

    /// Add subsystem.
    pub fn with_subsystem(mut self, subsystem: Box<dyn Subsystem>) -> Self {
        self.subsystems.push(subsystem);
        self
    }

    /// Build facade.
    pub fn build(self) -> SimpleFacade {
        let mut facade = SimpleFacade::new();
        for subsystem in self.subsystems {
            facade.add_subsystem(subsystem);
        }
        facade
    }

    /// Build and initialize facade.
    pub fn build_and_init(self) -> Result<SimpleFacade> {
        let mut facade = self.build();
        facade.init()?;
        Ok(facade)
    }
}

impl Default for FacadeBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Lazy facade that initializes on first use.
pub struct LazyFacade<F: FnOnce() -> SimpleFacade + Send> {
    init_fn: std::sync::Mutex<Option<F>>,
    facade: std::sync::RwLock<Option<SimpleFacade>>,
}

impl<F: FnOnce() -> SimpleFacade + Send> LazyFacade<F> {
    /// Create new lazy facade.
    pub fn new(init_fn: F) -> Self {
        Self {
            init_fn: std::sync::Mutex::new(Some(init_fn)),
            facade: std::sync::RwLock::new(None),
        }
    }

    /// Get or initialize facade.
    pub fn get(&self) -> Result<std::sync::RwLockReadGuard<'_, Option<SimpleFacade>>> {
        // Check if already initialized
        {
            let read = self.facade.read().unwrap();
            if read.is_some() {
                return Ok(read);
            }
        }

        // Initialize
        let mut init_fn = self.init_fn.lock().unwrap();
        if let Some(f) = init_fn.take() {
            let mut facade = f();
            facade.init()?;
            let mut write = self.facade.write().unwrap();
            *write = Some(facade);
        }

        Ok(self.facade.read().unwrap())
    }

    /// Check if initialized.
    pub fn is_initialized(&self) -> bool {
        self.facade.read().unwrap().is_some()
    }
}

/// Scoped facade that auto-shutdowns on drop.
pub struct ScopedFacade {
    inner: SimpleFacade,
}

impl ScopedFacade {
    /// Create new scoped facade.
    pub fn new(facade: SimpleFacade) -> Self {
        Self { inner: facade }
    }

    /// Create and initialize.
    pub fn create_initialized(mut facade: SimpleFacade) -> Result<Self> {
        facade.init()?;
        Ok(Self { inner: facade })
    }

    /// Get inner facade.
    pub fn inner(&self) -> &SimpleFacade {
        &self.inner
    }

    /// Get mutable inner facade.
    pub fn inner_mut(&mut self) -> &mut SimpleFacade {
        &mut self.inner
    }
}

impl Drop for ScopedFacade {
    fn drop(&mut self) {
        let _ = self.inner.shutdown();
    }
}

impl std::ops::Deref for ScopedFacade {
    type Target = SimpleFacade;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl std::ops::DerefMut for ScopedFacade {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestSubsystem {
        name: String,
        initialized: bool,
    }

    impl TestSubsystem {
        fn new(name: &str) -> Self {
            Self {
                name: name.to_string(),
                initialized: false,
            }
        }
    }

    impl Subsystem for TestSubsystem {
        fn name(&self) -> &str {
            &self.name
        }

        fn init(&mut self) -> Result<()> {
            self.initialized = true;
            Ok(())
        }

        fn shutdown(&mut self) -> Result<()> {
            self.initialized = false;
            Ok(())
        }

        fn is_initialized(&self) -> bool {
            self.initialized
        }
    }

    #[test]
    fn test_simple_facade() {
        let mut facade = SimpleFacade::new();
        facade.add_subsystem(Box::new(TestSubsystem::new("sub1")));
        facade.add_subsystem(Box::new(TestSubsystem::new("sub2")));

        assert!(!facade.is_initialized());
        facade.init().unwrap();
        assert!(facade.is_initialized());

        facade.shutdown().unwrap();
        assert!(!facade.is_initialized());
    }

    #[test]
    fn test_facade_builder() {
        let facade = FacadeBuilder::new()
            .with_subsystem(Box::new(TestSubsystem::new("sub1")))
            .with_subsystem(Box::new(TestSubsystem::new("sub2")))
            .build_and_init()
            .unwrap();

        assert!(facade.is_initialized());
        assert_eq!(facade.subsystem_count(), 2);
    }

    #[test]
    fn test_operation_facade() {
        struct TestOp {
            name: String,
        }

        impl NamedOperation for TestOp {
            fn name(&self) -> &str {
                &self.name
            }

            fn execute(&self) -> Result<()> {
                Ok(())
            }
        }

        let mut facade = OperationFacade::new();
        facade.register(Arc::new(TestOp {
            name: "test".to_string(),
        }));

        assert!(facade.execute("test").is_ok());
        assert!(facade.execute("unknown").is_err());
    }
}
