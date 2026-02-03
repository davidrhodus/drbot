//! Computer controller for orchestrating desktop automation.
//!
//! Provides a high-level interface for executing actions with safety features.

use crate::actions::{Action, ActionResult, ActionSequence};
use crate::keyboard::KeyboardController;
use crate::mouse::MouseController;
use crate::{ComputerError, Config, Result};
use async_recursion::async_recursion;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Execution mode for actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ExecutionMode {
    /// Execute immediately without confirmation.
    Immediate,
    /// Require confirmation before each action.
    #[default]
    Confirm,
    /// Dry run - don't actually execute, just log.
    DryRun,
    /// Batch mode - collect actions and execute together.
    Batch,
}

/// Controller configuration.
#[derive(Debug, Clone)]
pub struct ControllerConfig {
    /// Default execution mode.
    pub default_mode: ExecutionMode,
    /// Action timeout in milliseconds.
    pub timeout_ms: u64,
    /// Delay between actions in milliseconds.
    pub action_delay_ms: u64,
    /// Take screenshots after each action.
    pub capture_screenshots: bool,
    /// Safe mode - disallow potentially destructive actions.
    pub safe_mode: bool,
    /// Maximum actions per minute (rate limiting).
    pub max_actions_per_minute: Option<u32>,
}

impl Default for ControllerConfig {
    fn default() -> Self {
        Self {
            default_mode: ExecutionMode::Confirm,
            timeout_ms: 30000,
            action_delay_ms: 100,
            capture_screenshots: true,
            safe_mode: false,
            max_actions_per_minute: Some(60),
        }
    }
}

impl From<Config> for ControllerConfig {
    fn from(config: Config) -> Self {
        Self {
            default_mode: if config.require_confirmation {
                ExecutionMode::Confirm
            } else {
                ExecutionMode::Immediate
            },
            timeout_ms: config.action_timeout_ms,
            action_delay_ms: config.action_delay_ms,
            capture_screenshots: config.capture_screenshots,
            safe_mode: config.safe_mode,
            max_actions_per_minute: Some(60),
        }
    }
}

/// State of the computer controller.
#[derive(Debug, Default)]
struct ControllerState {
    /// Number of actions executed.
    actions_executed: u64,
    /// Actions in the current batch.
    batch: Vec<Action>,
    /// Results from executed actions.
    results: Vec<ActionResult>,
    /// Whether a batch is in progress.
    batch_mode: bool,
}

/// High-level computer controller.
pub struct ComputerController {
    config: ControllerConfig,
    mouse: MouseController,
    keyboard: KeyboardController,
    state: Arc<RwLock<ControllerState>>,
    /// Confirmation callback for interactive mode.
    confirmation_callback: Option<Box<dyn Fn(&Action) -> bool + Send + Sync>>,
}

impl ComputerController {
    /// Create a new computer controller with default config.
    pub async fn new() -> Result<Self> {
        Self::with_config(ControllerConfig::default()).await
    }

    /// Create a new computer controller with custom config.
    pub async fn with_config(config: ControllerConfig) -> Result<Self> {
        info!("Initializing computer controller");

        Ok(Self {
            config,
            mouse: MouseController::new(),
            keyboard: KeyboardController::new(),
            state: Arc::new(RwLock::new(ControllerState::default())),
            confirmation_callback: None,
        })
    }

    /// Set the confirmation callback.
    pub fn with_confirmation<F>(mut self, callback: F) -> Self
    where
        F: Fn(&Action) -> bool + Send + Sync + 'static,
    {
        self.confirmation_callback = Some(Box::new(callback));
        self
    }

    /// Execute an action.
    pub fn execute(&mut self, action: Action) -> ActionExecutor<'_> {
        let mode = self.config.default_mode;
        ActionExecutor {
            controller: self,
            action,
            mode,
        }
    }

    /// Execute a sequence of actions.
    pub async fn execute_sequence(
        &mut self,
        sequence: ActionSequence,
    ) -> Result<Vec<ActionResult>> {
        let mut results = Vec::with_capacity(sequence.actions.len());
        let mode = self.config.default_mode;
        let delay_ms = self.config.action_delay_ms;

        if let Some(name) = &sequence.name {
            info!("Executing sequence: {}", name);
        }

        for action in sequence.actions {
            let result = self.execute(action).with_mode(mode).await?;
            results.push(result);

            // Delay between actions
            if delay_ms > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
            }
        }

        Ok(results)
    }

    /// Start batch mode.
    pub async fn start_batch(&self) {
        let mut state = self.state.write().await;
        state.batch_mode = true;
        state.batch.clear();
        info!("Started batch mode");
    }

    /// Add action to current batch.
    pub async fn add_to_batch(&self, action: Action) {
        let mut state = self.state.write().await;
        if state.batch_mode {
            state.batch.push(action);
        }
    }

    /// Execute current batch.
    pub async fn execute_batch(&mut self) -> Result<Vec<ActionResult>> {
        let actions = {
            let mut state = self.state.write().await;
            state.batch_mode = false;
            std::mem::take(&mut state.batch)
        };

        let sequence = ActionSequence::from(actions);
        self.execute_sequence(sequence).await
    }

    /// Cancel current batch.
    pub async fn cancel_batch(&self) {
        let mut state = self.state.write().await;
        state.batch_mode = false;
        state.batch.clear();
        info!("Cancelled batch");
    }

    /// Execute a single action internally.
    #[async_recursion]
    async fn execute_action_internal(
        &mut self,
        action: &Action,
        mode: ExecutionMode,
    ) -> Result<ActionResult> {
        let start = Instant::now();

        // Check safe mode
        if self.config.safe_mode && is_potentially_destructive(action) {
            return Err(ComputerError::PermissionDenied(
                "Action blocked by safe mode".into(),
            ));
        }

        // Handle different execution modes
        match mode {
            ExecutionMode::DryRun => {
                debug!("Dry run: {:?}", action);
                return Ok(ActionResult::success(action.clone(), 0));
            }
            ExecutionMode::Confirm => {
                if let Some(callback) = &self.confirmation_callback {
                    if !callback(action) {
                        return Err(ComputerError::Cancelled);
                    }
                }
            }
            _ => {}
        }

        // Execute the action
        let result: crate::Result<()> = match action {
            Action::Click { x, y, button } => {
                self.mouse.move_to(*x, *y)?;
                self.mouse.click(*button)?;
                Ok(())
            }
            Action::DoubleClick { x, y, button } => {
                self.mouse.move_to(*x, *y)?;
                self.mouse.click(*button)?;
                std::thread::sleep(std::time::Duration::from_millis(50));
                self.mouse.click(*button)?;
                Ok(())
            }
            Action::MoveMouse { x, y } => {
                self.mouse.move_to(*x, *y)?;
                Ok(())
            }
            Action::Drag {
                from_x,
                from_y,
                to_x,
                to_y,
                button,
            } => {
                self.mouse.move_to(*from_x, *from_y)?;
                self.mouse.execute(crate::mouse::MouseAction::Drag {
                    to_x: *to_x,
                    to_y: *to_y,
                    button: *button,
                })?;
                Ok(())
            }
            Action::Scroll { dx, dy } => {
                self.mouse
                    .execute(crate::mouse::MouseAction::Scroll { dx: *dx, dy: *dy })?;
                Ok(())
            }
            Action::Type { text } => {
                self.keyboard.type_text(text)?;
                Ok(())
            }
            Action::KeyPress { key, modifiers } => {
                self.keyboard.press_key(*key, modifiers)?;
                Ok(())
            }
            Action::Shortcut { keys, modifiers } => {
                for key in keys {
                    self.keyboard.press_key(*key, modifiers)?;
                }
                Ok(())
            }
            Action::Wait { duration_ms } => {
                tokio::time::sleep(std::time::Duration::from_millis(*duration_ms)).await;
                Ok(())
            }
            Action::WaitForElement {
                description,
                timeout_ms,
            } => {
                // Wait for element with polling
                let deadline =
                    std::time::Instant::now() + std::time::Duration::from_millis(*timeout_ms);
                let poll_interval = std::time::Duration::from_millis(250);

                while std::time::Instant::now() < deadline {
                    // Check if element exists using screen analysis
                    if check_element_exists(description).await {
                        debug!("Element found: {}", description);
                        return Ok(ActionResult::success(
                            action.clone(),
                            start.elapsed().as_millis() as u64,
                        ));
                    }
                    tokio::time::sleep(poll_interval).await;
                }

                warn!("Element not found within timeout: {}", description);
                Err(ComputerError::Timeout)
            }
            Action::Screenshot { region } => {
                capture_screenshot(*region)?;
                Ok(())
            }
            Action::OpenApp { name } => {
                #[cfg(target_os = "macos")]
                {
                    std::process::Command::new("open")
                        .arg("-a")
                        .arg(name)
                        .spawn()
                        .map_err(|e| ComputerError::ActionFailed(e.to_string()))?;
                    Ok(())
                }
                #[cfg(not(target_os = "macos"))]
                {
                    let _ = name;
                    Err(ComputerError::PlatformNotSupported)
                }
            }
            Action::FocusApp { name } => {
                #[cfg(target_os = "macos")]
                {
                    let script = format!(r#"tell application "{}" to activate"#, name);
                    std::process::Command::new("osascript")
                        .arg("-e")
                        .arg(&script)
                        .spawn()
                        .map_err(|e| ComputerError::ActionFailed(e.to_string()))?;
                    Ok(())
                }
                #[cfg(not(target_os = "macos"))]
                {
                    let _ = name;
                    Err(ComputerError::PlatformNotSupported)
                }
            }
            Action::CloseWindow => {
                #[cfg(target_os = "macos")]
                {
                    // Cmd+W
                    self.keyboard.press_key(
                        crate::keyboard::KeyCode::W,
                        &[crate::keyboard::Modifiers::Meta],
                    )?;
                    Ok(())
                }
                #[cfg(not(target_os = "macos"))]
                {
                    Err(ComputerError::PlatformNotSupported)
                }
            }
            Action::Sequence { actions } => {
                for action in actions {
                    self.execute_action_internal(action, mode).await?;
                }
                Ok(())
            }
            Action::Conditional {
                condition,
                then_action,
                else_action,
            } => {
                let condition_met = evaluate_condition(condition).await;
                if condition_met {
                    self.execute_action_internal(then_action, mode).await?;
                } else if let Some(else_action) = else_action {
                    self.execute_action_internal(else_action, mode).await?;
                }
                Ok(())
            }
        };

        let duration = start.elapsed().as_millis() as u64;

        // Update state
        {
            let mut state = self.state.write().await;
            state.actions_executed += 1;
        }

        match result {
            Ok(()) => Ok(ActionResult::success(action.clone(), duration)),
            Err(e) => Ok(ActionResult::failure(action.clone(), e.to_string())),
        }
    }

    /// Get the number of actions executed.
    pub async fn actions_executed(&self) -> u64 {
        self.state.read().await.actions_executed
    }

    /// Get the configuration.
    pub fn config(&self) -> &ControllerConfig {
        &self.config
    }
}

/// Builder for executing a single action with options.
pub struct ActionExecutor<'a> {
    controller: &'a mut ComputerController,
    action: Action,
    mode: ExecutionMode,
}

impl<'a> ActionExecutor<'a> {
    /// Set the execution mode.
    pub fn with_mode(mut self, mode: ExecutionMode) -> Self {
        self.mode = mode;
        self
    }

    /// Execute the action.
    pub async fn await_result(self) -> Result<ActionResult> {
        self.controller
            .execute_action_internal(&self.action, self.mode)
            .await
    }
}

impl<'a> std::future::IntoFuture for ActionExecutor<'a> {
    type Output = Result<ActionResult>;
    type IntoFuture =
        std::pin::Pin<Box<dyn std::future::Future<Output = Self::Output> + Send + 'a>>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(async move { self.await_result().await })
    }
}

/// Check if an action is potentially destructive.
fn is_potentially_destructive(action: &Action) -> bool {
    match action {
        Action::Type { text } => {
            // Check for potentially dangerous commands
            text.contains("rm ") || text.contains("sudo") || text.contains("format")
        }
        Action::Shortcut { keys, modifiers } => {
            // Block certain dangerous shortcuts
            use crate::keyboard::{KeyCode, Modifiers};
            let has_cmd = modifiers.contains(&Modifiers::Meta);
            let has_q = keys.contains(&KeyCode::Q);
            has_cmd && has_q // Cmd+Q quits apps
        }
        Action::CloseWindow => true,
        Action::Sequence { actions } => actions.iter().any(is_potentially_destructive),
        Action::Conditional {
            then_action,
            else_action,
            ..
        } => {
            is_potentially_destructive(then_action)
                || else_action
                    .as_ref()
                    .map_or(false, |a| is_potentially_destructive(a))
        }
        _ => false,
    }
}

/// Evaluate a condition.
async fn evaluate_condition(condition: &crate::actions::Condition) -> bool {
    use crate::actions::Condition;

    match condition {
        Condition::Always => true,
        Condition::ElementExists { description } => check_element_exists(description).await,
        Condition::TextVisible { text } => check_text_visible(text).await,
        Condition::WindowFocused { app_name } => check_window_focused(app_name),
    }
}

/// Check if an element exists on screen (using description matching).
async fn check_element_exists(description: &str) -> bool {
    // For now, use a simple heuristic: take screenshot and check if
    // we can find visual patterns. In production, this would use:
    // - Accessibility API (AXUIElement on macOS)
    // - Computer vision / ML model
    // - OCR for text elements
    debug!("Checking for element: {}", description);

    #[cfg(target_os = "macos")]
    {
        // Try using Accessibility API to find elements
        // This is a simplified check - real implementation would be more thorough
        use std::process::Command;

        // Use AppleScript to check for UI elements
        let script = format!(
            r#"tell application "System Events"
                set frontApp to first process whose frontmost is true
                try
                    if exists (first UI element of frontApp whose description contains "{}") then
                        return "found"
                    end if
                end try
                return "not found"
            end tell"#,
            description.replace('"', "\\\"")
        );

        let output = Command::new("osascript").arg("-e").arg(&script).output();

        if let Ok(output) = output {
            let result = String::from_utf8_lossy(&output.stdout);
            return result.trim() == "found";
        }
    }

    false
}

/// Check if text is visible on screen.
async fn check_text_visible(text: &str) -> bool {
    debug!("Checking for visible text: {}", text);

    #[cfg(target_os = "macos")]
    {
        // Use Accessibility API to search for text in UI
        use std::process::Command;

        let script = format!(
            r#"tell application "System Events"
                set frontApp to first process whose frontmost is true
                try
                    if exists (first UI element of frontApp whose value contains "{}") then
                        return "found"
                    end if
                    if exists (first static text of frontApp whose value contains "{}") then
                        return "found"
                    end if
                end try
                return "not found"
            end tell"#,
            text.replace('"', "\\\""),
            text.replace('"', "\\\"")
        );

        let output = Command::new("osascript").arg("-e").arg(&script).output();

        if let Ok(output) = output {
            let result = String::from_utf8_lossy(&output.stdout);
            return result.trim() == "found";
        }
    }

    false
}

/// Check if a window is focused.
fn check_window_focused(app_name: &str) -> bool {
    debug!("Checking if window focused: {}", app_name);

    #[cfg(target_os = "macos")]
    {
        use std::process::Command;

        let script = r#"tell application "System Events"
            set frontApp to name of first process whose frontmost is true
            return frontApp
        end tell"#;

        let output = Command::new("osascript").arg("-e").arg(script).output();

        if let Ok(output) = output {
            let focused_app = String::from_utf8_lossy(&output.stdout);
            return focused_app
                .trim()
                .to_lowercase()
                .contains(&app_name.to_lowercase());
        }
    }

    false
}

/// Capture a screenshot.
#[cfg(target_os = "macos")]
fn capture_screenshot(region: Option<crate::actions::ScreenRegion>) -> crate::Result<Vec<u8>> {
    use core_graphics::display::{CGDisplay, CGPoint, CGRect, CGSize};

    let image = if let Some(r) = region {
        // Capture specific region
        let rect = CGRect::new(
            &CGPoint::new(r.x as f64, r.y as f64),
            &CGSize::new(r.width as f64, r.height as f64),
        );
        CGDisplay::screenshot(
            rect,
            core_graphics::display::kCGWindowListOptionOnScreenOnly,
            core_graphics::display::kCGNullWindowID,
            core_graphics::display::kCGWindowImageDefault,
        )
    } else {
        // Capture full screen
        let main_display = CGDisplay::main();
        CGDisplay::screenshot(
            main_display.bounds(),
            core_graphics::display::kCGWindowListOptionOnScreenOnly,
            core_graphics::display::kCGNullWindowID,
            core_graphics::display::kCGWindowImageDefault,
        )
    };

    match image {
        Some(_img) => {
            // Convert CGImage to PNG data
            // For now, return empty vec as placeholder - real implementation
            // would use image crate or native APIs to encode
            debug!("Screenshot captured successfully");
            Ok(Vec::new())
        }
        None => Err(crate::ComputerError::ScreenCaptureFailed(
            "Failed to capture screen".into(),
        )),
    }
}

#[cfg(not(target_os = "macos"))]
fn capture_screenshot(_region: Option<crate::actions::ScreenRegion>) -> crate::Result<Vec<u8>> {
    Err(crate::ComputerError::PlatformNotSupported)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_controller_config_default() {
        let config = ControllerConfig::default();
        assert_eq!(config.default_mode, ExecutionMode::Confirm);
        assert!(config.capture_screenshots);
    }

    #[test]
    fn test_is_potentially_destructive() {
        let safe_action = Action::Click {
            x: 0,
            y: 0,
            button: crate::mouse::MouseButton::Left,
        };
        assert!(!is_potentially_destructive(&safe_action));

        let dangerous_action = Action::Type {
            text: "rm -rf /".to_string(),
        };
        assert!(is_potentially_destructive(&dangerous_action));
    }

    #[tokio::test]
    async fn test_controller_new() {
        let controller = ComputerController::new().await.unwrap();
        assert_eq!(controller.actions_executed().await, 0);
    }
}
