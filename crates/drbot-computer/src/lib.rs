//! Computer use and desktop automation for drbot.
//!
//! Provides full desktop control capabilities including:
//! - Mouse control (move, click, drag)
//! - Keyboard control (type, key combinations)
//! - Screenshot capture and analysis
//! - Autonomous task execution with checkpoints
//!
//! # Safety
//!
//! All actions require explicit confirmation by default. The system
//! provides checkpoints for human-in-the-loop approval.
//!
//! # Example
//!
//! ```rust,no_run
//! use drbot_computer::{ComputerController, Action, ExecutionMode, MouseButton};
//!
//! async fn example() {
//!     let mut controller = ComputerController::new().await.unwrap();
//!
//!     // Execute with confirmation
//!     controller.execute(Action::Click { x: 100, y: 100, button: MouseButton::Left })
//!         .with_mode(ExecutionMode::Confirm)
//!         .await
//!         .unwrap();
//! }
//! ```

mod actions;
mod automation;
mod controller;
mod keyboard;
mod mouse;

pub use actions::{Action, ActionResult, ActionSequence};
pub use automation::{Checkpoint, CheckpointAction, Task, TaskRunner};
pub use controller::{ComputerController, ControllerConfig, ExecutionMode};
pub use keyboard::{KeyCode, KeyboardAction, Modifiers};
pub use mouse::{MouseAction, MouseButton};

use serde::{Deserialize, Serialize};

/// Result type for computer operations.
pub type Result<T> = std::result::Result<T, ComputerError>;

/// Computer use errors.
#[derive(Debug, thiserror::Error)]
pub enum ComputerError {
    #[error("Permission denied: {0}")]
    PermissionDenied(String),
    #[error("Action failed: {0}")]
    ActionFailed(String),
    #[error("Timeout waiting for condition")]
    Timeout,
    #[error("User cancelled action")]
    Cancelled,
    #[error("Screen capture failed: {0}")]
    ScreenCaptureFailed(String),
    #[error("Platform not supported")]
    PlatformNotSupported,
    #[error("Checkpoint required")]
    CheckpointRequired(Checkpoint),
}

/// Computer use configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Require confirmation for all actions.
    pub require_confirmation: bool,
    /// Action timeout in milliseconds.
    pub action_timeout_ms: u64,
    /// Delay between actions in milliseconds.
    pub action_delay_ms: u64,
    /// Take screenshots before/after actions.
    pub capture_screenshots: bool,
    /// Safe mode - only allow read-only actions.
    pub safe_mode: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            require_confirmation: true,
            action_timeout_ms: 30000,
            action_delay_ms: 100,
            capture_screenshots: true,
            safe_mode: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert!(config.require_confirmation);
        assert!(config.capture_screenshots);
    }
}
