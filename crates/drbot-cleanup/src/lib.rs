//! Cleanup utilities and finalizers for drbot.
//!
//! This crate provides:
//! - Cleanup handlers
//! - Finalizer queues
//! - Signal handling
//! - Graceful shutdown

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use thiserror::Error;
use tokio::sync::Notify;

/// Cleanup error types.
#[derive(Error, Debug)]
pub enum CleanupError {
    #[error("Cleanup failed: {0}")]
    Failed(String),

    #[error("Already shutdown")]
    AlreadyShutdown,

    #[error("Timeout")]
    Timeout,
}

/// Result type for cleanup operations.
pub type Result<T> = std::result::Result<T, CleanupError>;

/// Cleanup handler ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HandlerId(usize);

impl HandlerId {
    /// Generate new handler ID.
    pub fn new() -> Self {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        Self(COUNTER.fetch_add(1, Ordering::SeqCst))
    }
}

impl Default for HandlerId {
    fn default() -> Self {
        Self::new()
    }
}

/// Cleanup priority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    /// Run first.
    First = 0,
    /// High priority.
    High = 1,
    /// Normal priority.
    Normal = 2,
    /// Low priority.
    Low = 3,
    /// Run last.
    Last = 4,
}

impl Default for Priority {
    fn default() -> Self {
        Priority::Normal
    }
}

/// Registered cleanup handler.
struct CleanupHandler {
    id: HandlerId,
    name: String,
    priority: Priority,
    handler: Box<dyn FnOnce() + Send>,
}

/// Cleanup registry.
pub struct CleanupRegistry {
    handlers: Mutex<Vec<CleanupHandler>>,
    shutdown: AtomicBool,
}

impl CleanupRegistry {
    /// Create new registry.
    pub fn new() -> Self {
        Self {
            handlers: Mutex::new(Vec::new()),
            shutdown: AtomicBool::new(false),
        }
    }

    /// Register cleanup handler.
    pub fn register<F>(&self, name: impl Into<String>, handler: F) -> HandlerId
    where
        F: FnOnce() + Send + 'static,
    {
        self.register_with_priority(name, Priority::Normal, handler)
    }

    /// Register handler with priority.
    pub fn register_with_priority<F>(
        &self,
        name: impl Into<String>,
        priority: Priority,
        handler: F,
    ) -> HandlerId
    where
        F: FnOnce() + Send + 'static,
    {
        let id = HandlerId::new();
        let handler = CleanupHandler {
            id,
            name: name.into(),
            priority,
            handler: Box::new(handler),
        };

        self.handlers.lock().unwrap().push(handler);
        id
    }

    /// Unregister handler.
    pub fn unregister(&self, id: HandlerId) -> bool {
        let mut handlers = self.handlers.lock().unwrap();
        let len_before = handlers.len();
        handlers.retain(|h| h.id != id);
        handlers.len() < len_before
    }

    /// Run all cleanup handlers.
    pub fn run_cleanup(&self) -> CleanupResult {
        if self.shutdown.swap(true, Ordering::SeqCst) {
            return CleanupResult {
                total: 0,
                succeeded: 0,
                failed: 0,
                errors: vec![],
            };
        }

        let mut handlers = self.handlers.lock().unwrap();
        handlers.sort_by_key(|h| h.priority);

        let mut result = CleanupResult {
            total: handlers.len(),
            succeeded: 0,
            failed: 0,
            errors: vec![],
        };

        for handler in handlers.drain(..) {
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(handler.handler)) {
                Ok(_) => result.succeeded += 1,
                Err(_) => {
                    result.failed += 1;
                    result.errors.push(handler.name);
                }
            }
        }

        result
    }

    /// Check if shutdown.
    pub fn is_shutdown(&self) -> bool {
        self.shutdown.load(Ordering::SeqCst)
    }

    /// Get handler count.
    pub fn handler_count(&self) -> usize {
        self.handlers.lock().unwrap().len()
    }
}

impl Default for CleanupRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Cleanup result.
#[derive(Debug)]
pub struct CleanupResult {
    /// Total handlers.
    pub total: usize,
    /// Succeeded.
    pub succeeded: usize,
    /// Failed.
    pub failed: usize,
    /// Names of failed handlers.
    pub errors: Vec<String>,
}

impl CleanupResult {
    /// Check if all succeeded.
    pub fn is_success(&self) -> bool {
        self.failed == 0
    }
}

/// Finalizer queue for async cleanup.
pub struct FinalizerQueue {
    items: Mutex<VecDeque<Box<dyn FnOnce() + Send>>>,
    notify: Notify,
    shutdown: AtomicBool,
}

impl FinalizerQueue {
    /// Create new queue.
    pub fn new() -> Self {
        Self {
            items: Mutex::new(VecDeque::new()),
            notify: Notify::new(),
            shutdown: AtomicBool::new(false),
        }
    }

    /// Add item to finalize.
    pub fn add<F>(&self, finalizer: F)
    where
        F: FnOnce() + Send + 'static,
    {
        if !self.is_shutdown() {
            self.items.lock().unwrap().push_back(Box::new(finalizer));
            self.notify.notify_one();
        }
    }

    /// Process next item.
    pub fn process_one(&self) -> bool {
        if let Some(finalizer) = self.items.lock().unwrap().pop_front() {
            finalizer();
            true
        } else {
            false
        }
    }

    /// Process all items.
    pub fn process_all(&self) -> usize {
        let mut count = 0;
        while self.process_one() {
            count += 1;
        }
        count
    }

    /// Wait for items.
    pub async fn wait_for_items(&self) {
        self.notify.notified().await;
    }

    /// Shutdown queue.
    pub fn shutdown(&self) {
        self.shutdown.store(true, Ordering::SeqCst);
        self.notify.notify_waiters();
    }

    /// Check if shutdown.
    pub fn is_shutdown(&self) -> bool {
        self.shutdown.load(Ordering::SeqCst)
    }

    /// Get pending count.
    pub fn pending(&self) -> usize {
        self.items.lock().unwrap().len()
    }
}

impl Default for FinalizerQueue {
    fn default() -> Self {
        Self::new()
    }
}

/// Shutdown coordinator.
pub struct ShutdownCoordinator {
    shutdown_requested: AtomicBool,
    notify: Arc<Notify>,
    cleanup_registry: CleanupRegistry,
}

impl ShutdownCoordinator {
    /// Create new coordinator.
    pub fn new() -> Self {
        Self {
            shutdown_requested: AtomicBool::new(false),
            notify: Arc::new(Notify::new()),
            cleanup_registry: CleanupRegistry::new(),
        }
    }

    /// Request shutdown.
    pub fn request_shutdown(&self) {
        if !self.shutdown_requested.swap(true, Ordering::SeqCst) {
            self.notify.notify_waiters();
        }
    }

    /// Check if shutdown requested.
    pub fn is_shutdown_requested(&self) -> bool {
        self.shutdown_requested.load(Ordering::SeqCst)
    }

    /// Wait for shutdown request.
    pub async fn wait_for_shutdown(&self) {
        if !self.is_shutdown_requested() {
            self.notify.notified().await;
        }
    }

    /// Get shutdown signal for tokio select.
    pub fn shutdown_signal(&self) -> ShutdownSignal {
        ShutdownSignal {
            notify: self.notify.clone(),
            requested: &self.shutdown_requested,
        }
    }

    /// Register cleanup handler.
    pub fn on_shutdown<F>(&self, name: impl Into<String>, handler: F) -> HandlerId
    where
        F: FnOnce() + Send + 'static,
    {
        self.cleanup_registry.register(name, handler)
    }

    /// Run shutdown.
    pub fn run_shutdown(&self) -> CleanupResult {
        self.request_shutdown();
        self.cleanup_registry.run_cleanup()
    }
}

impl Default for ShutdownCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

/// Shutdown signal.
pub struct ShutdownSignal<'a> {
    notify: Arc<Notify>,
    requested: &'a AtomicBool,
}

impl<'a> ShutdownSignal<'a> {
    /// Wait for shutdown.
    pub async fn wait(&self) {
        if !self.requested.load(Ordering::SeqCst) {
            self.notify.notified().await;
        }
    }
}

/// Cleanup scope for automatic cleanup on scope exit.
pub struct CleanupScope {
    handlers: Mutex<Vec<Box<dyn FnOnce() + Send>>>,
}

impl CleanupScope {
    /// Create new scope.
    pub fn new() -> Self {
        Self {
            handlers: Mutex::new(Vec::new()),
        }
    }

    /// Add cleanup handler.
    pub fn defer<F>(&self, handler: F)
    where
        F: FnOnce() + Send + 'static,
    {
        self.handlers.lock().unwrap().push(Box::new(handler));
    }

    /// Run cleanup manually.
    pub fn cleanup(&self) {
        let mut handlers = self.handlers.lock().unwrap();
        // Run in reverse order
        while let Some(handler) = handlers.pop() {
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(handler));
        }
    }
}

impl Default for CleanupScope {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for CleanupScope {
    fn drop(&mut self) {
        self.cleanup();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cleanup_registry() {
        let registry = CleanupRegistry::new();
        let counter = Arc::new(AtomicUsize::new(0));

        let c1 = counter.clone();
        registry.register("handler1", move || {
            c1.fetch_add(1, Ordering::SeqCst);
        });

        let c2 = counter.clone();
        registry.register("handler2", move || {
            c2.fetch_add(1, Ordering::SeqCst);
        });

        let result = registry.run_cleanup();
        assert_eq!(result.total, 2);
        assert_eq!(result.succeeded, 2);
        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn test_cleanup_priority() {
        let registry = CleanupRegistry::new();
        let order = Arc::new(Mutex::new(Vec::new()));

        let o1 = order.clone();
        registry.register_with_priority("low", Priority::Low, move || {
            o1.lock().unwrap().push("low");
        });

        let o2 = order.clone();
        registry.register_with_priority("high", Priority::High, move || {
            o2.lock().unwrap().push("high");
        });

        registry.run_cleanup();

        let final_order = order.lock().unwrap();
        assert_eq!(final_order[0], "high");
        assert_eq!(final_order[1], "low");
    }

    #[test]
    fn test_finalizer_queue() {
        let queue = FinalizerQueue::new();
        let counter = Arc::new(AtomicUsize::new(0));

        let c1 = counter.clone();
        queue.add(move || {
            c1.fetch_add(1, Ordering::SeqCst);
        });

        let c2 = counter.clone();
        queue.add(move || {
            c2.fetch_add(1, Ordering::SeqCst);
        });

        assert_eq!(queue.pending(), 2);
        assert_eq!(queue.process_all(), 2);
        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn test_cleanup_scope() {
        let counter = Arc::new(AtomicUsize::new(0));

        {
            let scope = CleanupScope::new();
            let c = counter.clone();
            scope.defer(move || {
                c.fetch_add(1, Ordering::SeqCst);
            });
        }

        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_shutdown_coordinator() {
        let coordinator = ShutdownCoordinator::new();

        assert!(!coordinator.is_shutdown_requested());

        coordinator.request_shutdown();
        assert!(coordinator.is_shutdown_requested());
    }
}
