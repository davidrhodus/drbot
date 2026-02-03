//! Signal handling for drbot.
//!
//! This crate provides:
//! - Async signal handling
//! - Graceful shutdown support
//! - Signal forwarding

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use thiserror::Error;
use tokio::signal;
use tokio::sync::{broadcast, watch};
use tracing::{debug, info};

/// Signal error types.
#[derive(Error, Debug)]
pub enum SignalError {
    #[error("Signal handling error: {0}")]
    Handler(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Result type for signal operations.
pub type Result<T> = std::result::Result<T, SignalError>;

/// Signal types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Signal {
    /// Interrupt (Ctrl+C).
    Int,
    /// Terminate.
    Term,
    /// Hangup.
    Hup,
    /// Quit.
    Quit,
    /// User signal 1.
    Usr1,
    /// User signal 2.
    Usr2,
}

impl Signal {
    /// Get signal name.
    pub fn name(&self) -> &'static str {
        match self {
            Signal::Int => "SIGINT",
            Signal::Term => "SIGTERM",
            Signal::Hup => "SIGHUP",
            Signal::Quit => "SIGQUIT",
            Signal::Usr1 => "SIGUSR1",
            Signal::Usr2 => "SIGUSR2",
        }
    }
}

/// Wait for Ctrl+C signal.
pub async fn ctrl_c() -> Result<()> {
    signal::ctrl_c()
        .await
        .map_err(|e| SignalError::Handler(e.to_string()))
}

/// Wait for termination signal (SIGTERM on Unix).
#[cfg(unix)]
pub async fn terminate() -> Result<()> {
    let mut stream = signal::unix::signal(signal::unix::SignalKind::terminate())
        .map_err(|e| SignalError::Handler(e.to_string()))?;
    stream.recv().await;
    Ok(())
}

#[cfg(not(unix))]
pub async fn terminate() -> Result<()> {
    // On non-Unix, just wait for Ctrl+C
    ctrl_c().await
}

/// Wait for either Ctrl+C or SIGTERM.
pub async fn shutdown_signal() -> Signal {
    #[cfg(unix)]
    {
        let ctrl_c = async {
            signal::ctrl_c().await.ok();
            Signal::Int
        };

        let terminate = async {
            signal::unix::signal(signal::unix::SignalKind::terminate())
                .expect("failed to install SIGTERM handler")
                .recv()
                .await;
            Signal::Term
        };

        tokio::select! {
            sig = ctrl_c => sig,
            sig = terminate => sig,
        }
    }

    #[cfg(not(unix))]
    {
        signal::ctrl_c().await.ok();
        Signal::Int
    }
}

/// Shutdown coordinator for graceful shutdown.
pub struct ShutdownCoordinator {
    /// Whether shutdown has been triggered.
    triggered: Arc<AtomicBool>,
    /// Broadcast channel for shutdown notification.
    notify: broadcast::Sender<()>,
    /// Watch channel for shutdown state.
    state: watch::Sender<bool>,
}

impl ShutdownCoordinator {
    /// Create new shutdown coordinator.
    pub fn new() -> Self {
        let (notify, _) = broadcast::channel(1);
        let (state, _) = watch::channel(false);

        Self {
            triggered: Arc::new(AtomicBool::new(false)),
            notify,
            state,
        }
    }

    /// Get a shutdown signal receiver.
    pub fn subscribe(&self) -> ShutdownSignal {
        ShutdownSignal {
            triggered: Arc::clone(&self.triggered),
            notify: self.notify.subscribe(),
            state: self.state.subscribe(),
        }
    }

    /// Trigger shutdown.
    pub fn shutdown(&self) {
        if !self.triggered.swap(true, Ordering::SeqCst) {
            info!("Shutdown triggered");
            let _ = self.notify.send(());
            let _ = self.state.send(true);
        }
    }

    /// Check if shutdown has been triggered.
    pub fn is_shutdown(&self) -> bool {
        self.triggered.load(Ordering::SeqCst)
    }

    /// Install signal handlers and trigger shutdown on signal.
    pub async fn install_signal_handlers(&self) {
        let signal = shutdown_signal().await;
        info!("Received {} signal", signal.name());
        self.shutdown();
    }

    /// Spawn a task that triggers shutdown on signal.
    pub fn spawn_signal_handler(&self) -> tokio::task::JoinHandle<()> {
        let triggered = Arc::clone(&self.triggered);
        let notify = self.notify.clone();
        let state = self.state.clone();

        tokio::spawn(async move {
            let signal = shutdown_signal().await;
            info!("Received {} signal", signal.name());

            if !triggered.swap(true, Ordering::SeqCst) {
                let _ = notify.send(());
                let _ = state.send(true);
            }
        })
    }
}

impl Default for ShutdownCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for ShutdownCoordinator {
    fn clone(&self) -> Self {
        Self {
            triggered: Arc::clone(&self.triggered),
            notify: self.notify.clone(),
            state: self.state.clone(),
        }
    }
}

/// Shutdown signal receiver.
pub struct ShutdownSignal {
    triggered: Arc<AtomicBool>,
    notify: broadcast::Receiver<()>,
    state: watch::Receiver<bool>,
}

impl ShutdownSignal {
    /// Wait for shutdown signal.
    pub async fn recv(&mut self) {
        if self.triggered.load(Ordering::SeqCst) {
            return;
        }
        let _ = self.notify.recv().await;
    }

    /// Check if shutdown has been triggered.
    pub fn is_shutdown(&self) -> bool {
        self.triggered.load(Ordering::SeqCst)
    }

    /// Wait for shutdown or timeout.
    pub async fn recv_timeout(&mut self, timeout: std::time::Duration) -> bool {
        if self.triggered.load(Ordering::SeqCst) {
            return true;
        }

        tokio::select! {
            _ = self.notify.recv() => true,
            _ = tokio::time::sleep(timeout) => false,
        }
    }

    /// Clone this signal (creates new receiver).
    pub fn clone_signal(&self) -> Self {
        Self {
            triggered: Arc::clone(&self.triggered),
            notify: self.notify.resubscribe(),
            state: self.state.clone(),
        }
    }
}

impl Clone for ShutdownSignal {
    fn clone(&self) -> Self {
        self.clone_signal()
    }
}

/// Signal handler that runs callbacks.
pub struct SignalHandler {
    callbacks: Vec<Box<dyn FnOnce() + Send + 'static>>,
}

impl SignalHandler {
    /// Create new signal handler.
    pub fn new() -> Self {
        Self {
            callbacks: Vec::new(),
        }
    }

    /// Add callback to run on shutdown.
    pub fn on_shutdown<F>(mut self, f: F) -> Self
    where
        F: FnOnce() + Send + 'static,
    {
        self.callbacks.push(Box::new(f));
        self
    }

    /// Run signal handler.
    pub async fn run(self) {
        let signal = shutdown_signal().await;
        info!(
            "Received {} signal, running {} callbacks",
            signal.name(),
            self.callbacks.len()
        );

        for callback in self.callbacks {
            callback();
        }
    }
}

impl Default for SignalHandler {
    fn default() -> Self {
        Self::new()
    }
}

/// Guard that triggers shutdown on drop.
pub struct ShutdownGuard {
    coordinator: ShutdownCoordinator,
}

impl ShutdownGuard {
    /// Create new shutdown guard.
    pub fn new(coordinator: ShutdownCoordinator) -> Self {
        Self { coordinator }
    }
}

impl Drop for ShutdownGuard {
    fn drop(&mut self) {
        debug!("Shutdown guard dropped, triggering shutdown");
        self.coordinator.shutdown();
    }
}

/// Run a future until shutdown signal is received.
pub async fn run_until_shutdown<F, T>(future: F) -> Option<T>
where
    F: std::future::Future<Output = T>,
{
    tokio::select! {
        result = future => Some(result),
        _ = shutdown_signal() => {
            info!("Shutdown signal received, stopping");
            None
        }
    }
}

/// Run a function with graceful shutdown support.
pub async fn with_graceful_shutdown<F, Fut, T>(
    f: F,
    shutdown_timeout: std::time::Duration,
) -> Option<T>
where
    F: FnOnce(ShutdownSignal) -> Fut,
    Fut: std::future::Future<Output = T>,
{
    let coordinator = ShutdownCoordinator::new();
    let _handle = coordinator.spawn_signal_handler();
    let signal = coordinator.subscribe();

    let result = tokio::select! {
        result = f(signal) => Some(result),
        _ = coordinator.install_signal_handlers() => None,
    };

    // Wait for graceful shutdown
    if result.is_none() {
        info!("Waiting {:?} for graceful shutdown", shutdown_timeout);
        tokio::time::sleep(shutdown_timeout).await;
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_shutdown_coordinator() {
        let coordinator = ShutdownCoordinator::new();
        let mut signal = coordinator.subscribe();

        assert!(!coordinator.is_shutdown());
        assert!(!signal.is_shutdown());

        coordinator.shutdown();

        assert!(coordinator.is_shutdown());
        assert!(signal.is_shutdown());
    }

    #[tokio::test]
    async fn test_shutdown_signal_recv() {
        let coordinator = ShutdownCoordinator::new();
        let mut signal = coordinator.subscribe();

        // Spawn task to trigger shutdown
        let coord = coordinator.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            coord.shutdown();
        });

        signal.recv().await;
        assert!(signal.is_shutdown());
    }

    #[tokio::test]
    async fn test_shutdown_signal_timeout() {
        let coordinator = ShutdownCoordinator::new();
        let mut signal = coordinator.subscribe();

        // Should timeout without shutdown
        let result = signal
            .recv_timeout(std::time::Duration::from_millis(10))
            .await;
        assert!(!result);
    }

    #[tokio::test]
    async fn test_shutdown_guard() {
        let coordinator = ShutdownCoordinator::new();

        {
            let _guard = ShutdownGuard::new(coordinator.clone());
            assert!(!coordinator.is_shutdown());
        }

        // Guard dropped, shutdown should be triggered
        assert!(coordinator.is_shutdown());
    }

    #[test]
    fn test_signal_name() {
        assert_eq!(Signal::Int.name(), "SIGINT");
        assert_eq!(Signal::Term.name(), "SIGTERM");
    }
}
