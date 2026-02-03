//! Graceful shutdown handling for drbot.
//!
//! This crate provides:
//! - Signal handling (SIGTERM, SIGINT)
//! - Task shutdown coordination
//! - Drain periods
//! - Health check integration

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::{broadcast, mpsc, RwLock};

/// Shutdown error types.
#[derive(Error, Debug)]
pub enum ShutdownError {
    #[error("Shutdown timeout after {0:?}")]
    Timeout(Duration),

    #[error("Task failed during shutdown: {0}")]
    TaskFailed(String),

    #[error("Already shutting down")]
    AlreadyShuttingDown,

    #[error("Shutdown cancelled")]
    Cancelled,
}

/// Result type for shutdown operations.
pub type Result<T> = std::result::Result<T, ShutdownError>;

/// Shutdown phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShutdownPhase {
    /// Normal operation.
    Running,
    /// Stop accepting new work.
    Draining,
    /// Finish existing work.
    Finishing,
    /// Force termination.
    Terminating,
    /// Shutdown complete.
    Complete,
}

/// Shutdown signal.
#[derive(Debug, Clone)]
pub struct ShutdownSignal {
    /// Reason for shutdown.
    pub reason: ShutdownReason,
    /// When shutdown was initiated.
    pub initiated_at: DateTime<Utc>,
    /// Current phase.
    pub phase: ShutdownPhase,
}

/// Reason for shutdown.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShutdownReason {
    /// SIGTERM signal.
    SigTerm,
    /// SIGINT signal (Ctrl+C).
    SigInt,
    /// API request.
    ApiRequest,
    /// Health check failure.
    HealthFailure,
    /// Manual trigger.
    Manual(String),
}

/// Shutdown configuration.
#[derive(Debug, Clone)]
pub struct ShutdownConfig {
    /// Time to drain (stop accepting new work).
    pub drain_timeout: Duration,
    /// Time to finish existing work.
    pub finish_timeout: Duration,
    /// Time for force termination.
    pub force_timeout: Duration,
    /// Enable signal handling.
    pub handle_signals: bool,
}

impl Default for ShutdownConfig {
    fn default() -> Self {
        Self {
            drain_timeout: Duration::from_secs(5),
            finish_timeout: Duration::from_secs(30),
            force_timeout: Duration::from_secs(5),
            handle_signals: true,
        }
    }
}

/// Trait for shutdownable components.
#[async_trait]
pub trait Shutdownable: Send + Sync {
    /// Name of the component.
    fn name(&self) -> &str;

    /// Called when draining starts.
    async fn on_drain(&self) -> Result<()> {
        Ok(())
    }

    /// Called to finish work.
    async fn on_finish(&self) -> Result<()> {
        Ok(())
    }

    /// Called for forced termination.
    async fn on_terminate(&self) -> Result<()> {
        Ok(())
    }

    /// Check if component has finished.
    fn is_finished(&self) -> bool {
        true
    }
}

/// Shutdown coordinator.
pub struct ShutdownCoordinator {
    config: ShutdownConfig,
    phase: Arc<RwLock<ShutdownPhase>>,
    is_shutting_down: Arc<AtomicBool>,
    signal_tx: broadcast::Sender<ShutdownSignal>,
    components: Arc<RwLock<Vec<Arc<dyn Shutdownable>>>>,
    shutdown_initiated: Arc<RwLock<Option<DateTime<Utc>>>>,
}

impl ShutdownCoordinator {
    /// Create a new shutdown coordinator.
    pub fn new(config: ShutdownConfig) -> Self {
        let (signal_tx, _) = broadcast::channel(16);

        Self {
            config,
            phase: Arc::new(RwLock::new(ShutdownPhase::Running)),
            is_shutting_down: Arc::new(AtomicBool::new(false)),
            signal_tx,
            components: Arc::new(RwLock::new(Vec::new())),
            shutdown_initiated: Arc::new(RwLock::new(None)),
        }
    }

    /// Register a component.
    pub async fn register(&self, component: Arc<dyn Shutdownable>) {
        self.components.write().await.push(component);
    }

    /// Check if shutting down.
    pub fn is_shutting_down(&self) -> bool {
        self.is_shutting_down.load(Ordering::Relaxed)
    }

    /// Get current phase.
    pub async fn phase(&self) -> ShutdownPhase {
        *self.phase.read().await
    }

    /// Subscribe to shutdown signals.
    pub fn subscribe(&self) -> broadcast::Receiver<ShutdownSignal> {
        self.signal_tx.subscribe()
    }

    /// Create a shutdown token.
    pub fn token(&self) -> ShutdownToken {
        ShutdownToken {
            is_shutting_down: self.is_shutting_down.clone(),
            sender: self.signal_tx.clone(),
            receiver: self.signal_tx.subscribe(),
        }
    }

    /// Initiate shutdown.
    pub async fn shutdown(&self, reason: ShutdownReason) -> Result<()> {
        // Check if already shutting down
        if self.is_shutting_down.swap(true, Ordering::SeqCst) {
            return Err(ShutdownError::AlreadyShuttingDown);
        }

        let initiated_at = Utc::now();
        *self.shutdown_initiated.write().await = Some(initiated_at);

        // Broadcast initial signal
        let _ = self.signal_tx.send(ShutdownSignal {
            reason: reason.clone(),
            initiated_at,
            phase: ShutdownPhase::Draining,
        });

        // Phase 1: Draining
        *self.phase.write().await = ShutdownPhase::Draining;
        self.notify_phase(ShutdownPhase::Draining, &reason, initiated_at)
            .await;

        {
            let components = self.components.read().await;
            for component in components.iter() {
                if let Err(e) = component.on_drain().await {
                    tracing::warn!("Component {} drain failed: {}", component.name(), e);
                }
            }
        }

        tokio::time::sleep(self.config.drain_timeout).await;

        // Phase 2: Finishing
        *self.phase.write().await = ShutdownPhase::Finishing;
        self.notify_phase(ShutdownPhase::Finishing, &reason, initiated_at)
            .await;

        {
            let components = self.components.read().await;
            for component in components.iter() {
                if let Err(e) = component.on_finish().await {
                    tracing::warn!("Component {} finish failed: {}", component.name(), e);
                }
            }
        }

        // Wait for components to finish
        let start = std::time::Instant::now();
        while start.elapsed() < self.config.finish_timeout {
            let all_finished = {
                let components = self.components.read().await;
                components.iter().all(|c| c.is_finished())
            };

            if all_finished {
                break;
            }

            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        // Phase 3: Terminating
        *self.phase.write().await = ShutdownPhase::Terminating;
        self.notify_phase(ShutdownPhase::Terminating, &reason, initiated_at)
            .await;

        {
            let components = self.components.read().await;
            for component in components.iter() {
                if let Err(e) = component.on_terminate().await {
                    tracing::warn!("Component {} terminate failed: {}", component.name(), e);
                }
            }
        }

        tokio::time::sleep(self.config.force_timeout).await;

        // Phase 4: Complete
        *self.phase.write().await = ShutdownPhase::Complete;
        self.notify_phase(ShutdownPhase::Complete, &reason, initiated_at)
            .await;

        Ok(())
    }

    async fn notify_phase(
        &self,
        phase: ShutdownPhase,
        reason: &ShutdownReason,
        initiated_at: DateTime<Utc>,
    ) {
        let _ = self.signal_tx.send(ShutdownSignal {
            reason: reason.clone(),
            initiated_at,
            phase,
        });
    }
}

impl Default for ShutdownCoordinator {
    fn default() -> Self {
        Self::new(ShutdownConfig::default())
    }
}

/// A token for checking shutdown status.
pub struct ShutdownToken {
    is_shutting_down: Arc<AtomicBool>,
    sender: broadcast::Sender<ShutdownSignal>,
    receiver: broadcast::Receiver<ShutdownSignal>,
}

impl Clone for ShutdownToken {
    fn clone(&self) -> Self {
        Self {
            is_shutting_down: self.is_shutting_down.clone(),
            sender: self.sender.clone(),
            receiver: self.sender.subscribe(),
        }
    }
}

impl ShutdownToken {
    /// Check if shutting down.
    pub fn is_shutting_down(&self) -> bool {
        self.is_shutting_down.load(Ordering::Relaxed)
    }

    /// Wait for shutdown signal.
    pub async fn wait(&mut self) -> ShutdownSignal {
        loop {
            match self.receiver.recv().await {
                Ok(signal) => return signal,
                Err(_) => {
                    // Channel closed, create synthetic signal
                    return ShutdownSignal {
                        reason: ShutdownReason::Manual("channel closed".to_string()),
                        initiated_at: Utc::now(),
                        phase: ShutdownPhase::Complete,
                    };
                }
            }
        }
    }

    /// Wait for a specific phase.
    pub async fn wait_for_phase(&mut self, target: ShutdownPhase) -> ShutdownSignal {
        loop {
            let signal = self.wait().await;
            if signal.phase == target {
                return signal;
            }
        }
    }
}

/// Task tracker for graceful shutdown.
pub struct TaskTracker {
    active_tasks: Arc<AtomicU64>,
    shutdown_token: ShutdownToken,
    task_tx: mpsc::Sender<()>,
    task_rx: Arc<RwLock<Option<mpsc::Receiver<()>>>>,
}

impl TaskTracker {
    /// Create a new task tracker.
    pub fn new(shutdown_token: ShutdownToken) -> Self {
        let (task_tx, task_rx) = mpsc::channel(1);

        Self {
            active_tasks: Arc::new(AtomicU64::new(0)),
            shutdown_token,
            task_tx,
            task_rx: Arc::new(RwLock::new(Some(task_rx))),
        }
    }

    /// Spawn a tracked task.
    pub fn spawn<F>(&self, task: F) -> TaskGuard
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        let guard = TaskGuard {
            active_tasks: self.active_tasks.clone(),
            completed: Arc::new(AtomicBool::new(false)),
        };

        guard.active_tasks.fetch_add(1, Ordering::Relaxed);

        let guard_clone = guard.clone();
        tokio::spawn(async move {
            task.await;
            guard_clone.complete();
        });

        guard
    }

    /// Get number of active tasks.
    pub fn active_count(&self) -> u64 {
        self.active_tasks.load(Ordering::Relaxed)
    }

    /// Check if shutting down.
    pub fn is_shutting_down(&self) -> bool {
        self.shutdown_token.is_shutting_down()
    }

    /// Wait for all tasks to complete.
    pub async fn wait_for_completion(&self, timeout: Duration) -> Result<()> {
        let start = std::time::Instant::now();

        while self.active_count() > 0 {
            if start.elapsed() > timeout {
                return Err(ShutdownError::Timeout(timeout));
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        Ok(())
    }
}

/// Guard for a tracked task.
#[derive(Clone)]
pub struct TaskGuard {
    active_tasks: Arc<AtomicU64>,
    completed: Arc<AtomicBool>,
}

impl TaskGuard {
    /// Mark the task as complete.
    pub fn complete(&self) {
        if !self.completed.swap(true, Ordering::SeqCst) {
            self.active_tasks.fetch_sub(1, Ordering::Relaxed);
        }
    }
}

impl Drop for TaskGuard {
    fn drop(&mut self) {
        self.complete();
    }
}

/// Drain guard for connection draining.
pub struct DrainGuard {
    draining: Arc<AtomicBool>,
    active_connections: Arc<AtomicU64>,
}

impl DrainGuard {
    /// Create a new drain guard.
    pub fn new() -> Self {
        Self {
            draining: Arc::new(AtomicBool::new(false)),
            active_connections: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Start draining.
    pub fn start_drain(&self) {
        self.draining.store(true, Ordering::SeqCst);
    }

    /// Check if draining.
    pub fn is_draining(&self) -> bool {
        self.draining.load(Ordering::Relaxed)
    }

    /// Accept a new connection (returns None if draining).
    pub fn accept(&self) -> Option<ConnectionGuard> {
        if self.is_draining() {
            return None;
        }

        self.active_connections.fetch_add(1, Ordering::Relaxed);
        Some(ConnectionGuard {
            active_connections: self.active_connections.clone(),
        })
    }

    /// Get active connection count.
    pub fn active_connections(&self) -> u64 {
        self.active_connections.load(Ordering::Relaxed)
    }

    /// Wait for all connections to close.
    pub async fn wait_drained(&self, timeout: Duration) -> Result<()> {
        let start = std::time::Instant::now();

        while self.active_connections() > 0 {
            if start.elapsed() > timeout {
                return Err(ShutdownError::Timeout(timeout));
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        Ok(())
    }
}

impl Default for DrainGuard {
    fn default() -> Self {
        Self::new()
    }
}

/// Guard for an active connection.
pub struct ConnectionGuard {
    active_connections: Arc<AtomicU64>,
}

impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        self.active_connections.fetch_sub(1, Ordering::Relaxed);
    }
}

/// Shutdown hook function type.
pub type ShutdownHook = Box<dyn FnOnce() -> BoxFuture<'static, Result<()>> + Send + Sync>;

/// Shutdown hooks manager.
pub struct ShutdownHooks {
    hooks: RwLock<Vec<(String, ShutdownHook)>>,
}

impl ShutdownHooks {
    /// Create new hooks manager.
    pub fn new() -> Self {
        Self {
            hooks: RwLock::new(Vec::new()),
        }
    }

    /// Register a hook.
    pub async fn register(&self, name: impl Into<String>, hook: ShutdownHook) {
        self.hooks.write().await.push((name.into(), hook));
    }

    /// Run all hooks.
    pub async fn run_all(self) -> HashMap<String, Result<()>> {
        let mut results = HashMap::new();
        let hooks = self.hooks.into_inner();

        for (name, hook) in hooks {
            let result = hook().await;
            results.insert(name, result);
        }

        results
    }
}

impl Default for ShutdownHooks {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_shutdown_coordinator() {
        let coordinator = ShutdownCoordinator::new(ShutdownConfig {
            drain_timeout: Duration::from_millis(50),
            finish_timeout: Duration::from_millis(50),
            force_timeout: Duration::from_millis(50),
            handle_signals: false,
        });

        assert_eq!(coordinator.phase().await, ShutdownPhase::Running);
        assert!(!coordinator.is_shutting_down());
    }

    #[tokio::test]
    async fn test_shutdown_token() {
        let coordinator = ShutdownCoordinator::default();
        let token = coordinator.token();

        assert!(!token.is_shutting_down());
    }

    #[tokio::test]
    async fn test_task_tracker() {
        let coordinator = ShutdownCoordinator::default();
        let token = coordinator.token();
        let tracker = TaskTracker::new(token);

        assert_eq!(tracker.active_count(), 0);

        let _guard = tracker.spawn(async {
            tokio::time::sleep(Duration::from_millis(100)).await;
        });

        assert_eq!(tracker.active_count(), 1);

        tokio::time::sleep(Duration::from_millis(150)).await;
        assert_eq!(tracker.active_count(), 0);
    }

    #[tokio::test]
    async fn test_drain_guard() {
        let guard = DrainGuard::new();

        assert!(!guard.is_draining());
        assert_eq!(guard.active_connections(), 0);

        let conn1 = guard.accept().unwrap();
        assert_eq!(guard.active_connections(), 1);

        let conn2 = guard.accept().unwrap();
        assert_eq!(guard.active_connections(), 2);

        drop(conn1);
        assert_eq!(guard.active_connections(), 1);

        guard.start_drain();
        assert!(guard.is_draining());
        assert!(guard.accept().is_none());

        drop(conn2);
        assert_eq!(guard.active_connections(), 0);
    }

    #[tokio::test]
    async fn test_drain_wait() {
        let guard = DrainGuard::new();

        let conn = guard.accept().unwrap();
        guard.start_drain();

        let guard_clone = Arc::new(guard);
        let guard_for_wait = guard_clone.clone();

        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            drop(conn);
        });

        guard_for_wait
            .wait_drained(Duration::from_secs(1))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_shutdown_hooks() {
        let hooks = ShutdownHooks::new();

        hooks
            .register("hook1", Box::new(|| Box::pin(async { Ok(()) })))
            .await;

        hooks
            .register("hook2", Box::new(|| Box::pin(async { Ok(()) })))
            .await;

        let results = hooks.run_all().await;
        assert_eq!(results.len(), 2);
        assert!(results.get("hook1").unwrap().is_ok());
        assert!(results.get("hook2").unwrap().is_ok());
    }

    #[test]
    fn test_shutdown_reason() {
        let reason = ShutdownReason::Manual("test".to_string());
        assert_eq!(reason, ShutdownReason::Manual("test".to_string()));
    }

    #[test]
    fn test_shutdown_config_default() {
        let config = ShutdownConfig::default();
        assert_eq!(config.drain_timeout, Duration::from_secs(5));
        assert_eq!(config.finish_timeout, Duration::from_secs(30));
    }
}
