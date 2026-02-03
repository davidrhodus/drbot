//! Command execution.

use crate::command::{AppCommand, Command, CommandType, FileCommand, SearchCommand, SystemCommand};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tracing::{debug, info, warn};

/// Result of command execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    /// Whether execution succeeded.
    pub success: bool,
    /// Result message.
    pub message: String,
    /// Any output data.
    pub data: Option<serde_json::Value>,
    /// Error if failed.
    pub error: Option<String>,
}

impl ExecutionResult {
    /// Create a success result.
    pub fn success(message: impl Into<String>) -> Self {
        Self {
            success: true,
            message: message.into(),
            data: None,
            error: None,
        }
    }

    /// Create a failure result.
    pub fn failure(error: impl Into<String>) -> Self {
        Self {
            success: false,
            message: String::new(),
            data: None,
            error: Some(error.into()),
        }
    }

    /// Add data to result.
    pub fn with_data(mut self, data: serde_json::Value) -> Self {
        self.data = Some(data);
        self
    }
}

/// Command executor.
#[derive(Debug, Default)]
pub struct CommandExecutor {
    // Could hold references to system services
}

impl CommandExecutor {
    /// Create a new executor.
    pub fn new() -> Self {
        Self::default()
    }

    /// Execute a command.
    pub async fn execute(&self, command: Command) -> ExecutionResult {
        info!("Executing command: {:?}", command.command_type);

        if command.requires_confirmation {
            warn!("Command requires confirmation: {:?}", command.command_type);
            return ExecutionResult {
                success: false,
                message: "Confirmation required".to_string(),
                data: Some(serde_json::json!({
                    "requires_confirmation": true,
                    "command_type": format!("{:?}", command.command_type),
                    "original": command.original,
                })),
                error: Some("Confirmation required".to_string()),
            };
        }

        match &command.command_type {
            CommandType::System(sys_cmd) => self.execute_system(sys_cmd).await,
            CommandType::Application(app_cmd) => self.execute_app(app_cmd).await,
            CommandType::File(file_cmd) => self.execute_file(file_cmd).await,
            CommandType::Search(search_cmd) => self.execute_search(search_cmd).await,
            CommandType::Custom(desc) => {
                warn!("Custom command not implemented: {}", desc);
                ExecutionResult::failure(format!("Custom command '{}' not supported", desc))
            }
            _ => {
                warn!("Unhandled command type: {:?}", command.command_type);
                ExecutionResult::failure("Command type not supported")
            }
        }
    }

    async fn execute_system(&self, cmd: &SystemCommand) -> ExecutionResult {
        match cmd {
            SystemCommand::Screenshot => {
                #[cfg(target_os = "macos")]
                {
                    match std::process::Command::new("screencapture")
                        .args(["-i", "-c"]) // Interactive, to clipboard
                        .spawn()
                    {
                        Ok(_) => ExecutionResult::success("Screenshot initiated"),
                        Err(e) => ExecutionResult::failure(format!("Screenshot failed: {}", e)),
                    }
                }
                #[cfg(not(target_os = "macos"))]
                {
                    ExecutionResult::failure("Screenshots not supported on this platform")
                }
            }
            SystemCommand::VolumeUp => {
                #[cfg(target_os = "macos")]
                {
                    let _ = std::process::Command::new("osascript")
                        .args(["-e", "set volume output volume ((output volume of (get volume settings)) + 10)"])
                        .output();
                    ExecutionResult::success("Volume increased")
                }
                #[cfg(not(target_os = "macos"))]
                ExecutionResult::failure("Volume control not supported")
            }
            SystemCommand::VolumeDown => {
                #[cfg(target_os = "macos")]
                {
                    let _ = std::process::Command::new("osascript")
                        .args(["-e", "set volume output volume ((output volume of (get volume settings)) - 10)"])
                        .output();
                    ExecutionResult::success("Volume decreased")
                }
                #[cfg(not(target_os = "macos"))]
                ExecutionResult::failure("Volume control not supported")
            }
            SystemCommand::Mute => {
                #[cfg(target_os = "macos")]
                {
                    let _ = std::process::Command::new("osascript")
                        .args(["-e", "set volume output muted true"])
                        .output();
                    ExecutionResult::success("Audio muted")
                }
                #[cfg(not(target_os = "macos"))]
                ExecutionResult::failure("Volume control not supported")
            }
            SystemCommand::Lock => {
                #[cfg(target_os = "macos")]
                {
                    let _ = std::process::Command::new("pmset")
                        .args(["displaysleepnow"])
                        .output();
                    ExecutionResult::success("Screen locked")
                }
                #[cfg(not(target_os = "macos"))]
                ExecutionResult::failure("Lock not supported")
            }
            _ => ExecutionResult::failure(format!("System command {:?} not implemented", cmd)),
        }
    }

    async fn execute_app(&self, cmd: &AppCommand) -> ExecutionResult {
        match cmd {
            AppCommand::Open(app) => {
                #[cfg(target_os = "macos")]
                {
                    match std::process::Command::new("open").args(["-a", app]).spawn() {
                        Ok(_) => ExecutionResult::success(format!("Opened {}", app)),
                        Err(e) => {
                            ExecutionResult::failure(format!("Failed to open {}: {}", app, e))
                        }
                    }
                }
                #[cfg(not(target_os = "macos"))]
                {
                    let _ = app;
                    ExecutionResult::failure("Application launch not supported")
                }
            }
            AppCommand::Close(app) => {
                #[cfg(target_os = "macos")]
                {
                    let script = format!(r#"tell application "{}" to quit"#, app);
                    match std::process::Command::new("osascript")
                        .args(["-e", &script])
                        .output()
                    {
                        Ok(_) => ExecutionResult::success(format!("Closed {}", app)),
                        Err(e) => {
                            ExecutionResult::failure(format!("Failed to close {}: {}", app, e))
                        }
                    }
                }
                #[cfg(not(target_os = "macos"))]
                {
                    let _ = app;
                    ExecutionResult::failure("Application control not supported")
                }
            }
            AppCommand::Focus(app) => {
                #[cfg(target_os = "macos")]
                {
                    let script = format!(r#"tell application "{}" to activate"#, app);
                    match std::process::Command::new("osascript")
                        .args(["-e", &script])
                        .output()
                    {
                        Ok(_) => ExecutionResult::success(format!("Focused {}", app)),
                        Err(e) => {
                            ExecutionResult::failure(format!("Failed to focus {}: {}", app, e))
                        }
                    }
                }
                #[cfg(not(target_os = "macos"))]
                {
                    let _ = app;
                    ExecutionResult::failure("Application control not supported")
                }
            }
            _ => ExecutionResult::failure(format!("App command {:?} not implemented", cmd)),
        }
    }

    async fn execute_file(&self, cmd: &FileCommand) -> ExecutionResult {
        match cmd {
            FileCommand::Open(path) => {
                #[cfg(target_os = "macos")]
                {
                    match std::process::Command::new("open").arg(path).spawn() {
                        Ok(_) => ExecutionResult::success(format!("Opened {}", path)),
                        Err(e) => ExecutionResult::failure(format!("Failed to open: {}", e)),
                    }
                }
                #[cfg(not(target_os = "macos"))]
                {
                    let _ = path;
                    ExecutionResult::failure("File operations not supported")
                }
            }
            FileCommand::Create(path) => {
                if path.trim().is_empty() {
                    return ExecutionResult::failure("Missing path");
                }

                let is_dir = path.ends_with('/') || path.ends_with('\\');
                let p = Path::new(path);

                let result = if is_dir {
                    std::fs::create_dir_all(p)
                } else {
                    if let Some(parent) = p.parent() {
                        if !parent.as_os_str().is_empty() {
                            if let Err(e) = std::fs::create_dir_all(parent) {
                                return ExecutionResult::failure(format!(
                                    "Failed to create parent directory: {}",
                                    e
                                ));
                            }
                        }
                    }
                    std::fs::File::create(p).map(|_| ())
                };

                match result {
                    Ok(_) => ExecutionResult::success(format!("Created {}", path)),
                    Err(e) => ExecutionResult::failure(format!("Create failed: {}", e)),
                }
            }
            FileCommand::Delete(path) => {
                if path.trim().is_empty() {
                    return ExecutionResult::failure("Missing path");
                }

                let p = Path::new(path);
                if !p.exists() {
                    return ExecutionResult::failure(format!("Not found: {}", path));
                }

                let result = if p.is_dir() {
                    std::fs::remove_dir_all(p)
                } else {
                    std::fs::remove_file(p)
                };

                match result {
                    Ok(_) => ExecutionResult::success(format!("Deleted {}", path)),
                    Err(e) => ExecutionResult::failure(format!("Delete failed: {}", e)),
                }
            }
            FileCommand::Move { from, to } | FileCommand::Rename { from, to } => {
                let from_p = Path::new(from);
                let to_p = Path::new(to);

                if let Some(parent) = to_p.parent() {
                    if !parent.as_os_str().is_empty() {
                        if let Err(e) = std::fs::create_dir_all(parent) {
                            return ExecutionResult::failure(format!(
                                "Failed to create destination directory: {}",
                                e
                            ));
                        }
                    }
                }

                match std::fs::rename(from_p, to_p) {
                    Ok(_) => ExecutionResult::success(format!("Moved {} -> {}", from, to)),
                    Err(e) => ExecutionResult::failure(format!("Move failed: {}", e)),
                }
            }
            FileCommand::Copy { from, to } => {
                let from_p = Path::new(from);
                let to_p = Path::new(to);

                if !from_p.exists() {
                    return ExecutionResult::failure(format!("Not found: {}", from));
                }

                if let Some(parent) = to_p.parent() {
                    if !parent.as_os_str().is_empty() {
                        if let Err(e) = std::fs::create_dir_all(parent) {
                            return ExecutionResult::failure(format!(
                                "Failed to create destination directory: {}",
                                e
                            ));
                        }
                    }
                }

                let result = if from_p.is_dir() {
                    copy_dir_recursive(from_p, to_p)
                } else {
                    std::fs::copy(from_p, to_p).map(|_| ()).map_err(|e| e)
                };

                match result {
                    Ok(_) => ExecutionResult::success(format!("Copied {} -> {}", from, to)),
                    Err(e) => ExecutionResult::failure(format!("Copy failed: {}", e)),
                }
            }
            FileCommand::Find(pattern) => {
                // Use find command
                debug!("Searching for: {}", pattern);
                ExecutionResult::success(format!("Searching for '{}'...", pattern))
            }
            FileCommand::List(path) => {
                let p = Path::new(path);
                if !p.exists() {
                    return ExecutionResult::failure(format!("Not found: {}", path));
                }
                if !p.is_dir() {
                    return ExecutionResult::failure(format!("Not a directory: {}", path));
                }

                match std::fs::read_dir(p) {
                    Ok(entries) => {
                        let items: Vec<String> = entries
                            .flatten()
                            .map(|e| e.path().to_string_lossy().to_string())
                            .collect();
                        ExecutionResult::success(format!("Listed {}", path))
                            .with_data(serde_json::json!({ "items": items }))
                    }
                    Err(e) => ExecutionResult::failure(format!("List failed: {}", e)),
                }
            }
        }
    }

    async fn execute_search(&self, cmd: &SearchCommand) -> ExecutionResult {
        match cmd {
            SearchCommand::Web(query) => {
                let url = format!("https://www.google.com/search?q={}", urlencoded(query));

                #[cfg(target_os = "macos")]
                {
                    match std::process::Command::new("open").arg(&url).spawn() {
                        Ok(_) => ExecutionResult::success(format!("Searching for '{}'", query)),
                        Err(e) => ExecutionResult::failure(format!("Failed to search: {}", e)),
                    }
                }
                #[cfg(not(target_os = "macos"))]
                {
                    ExecutionResult::failure("Web search not supported")
                }
            }
            _ => ExecutionResult::failure(format!("Search command {:?} not implemented", cmd)),
        }
    }
}

/// Simple URL encoding.
fn urlencoded(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' || c == '~' {
                c.to_string()
            } else if c == ' ' {
                "+".to_string()
            } else {
                format!("%{:02X}", c as u32)
            }
        })
        .collect()
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;

    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let path = entry.path();
        let dest_path = dst.join(entry.file_name());

        if path.is_dir() {
            copy_dir_recursive(&path, &dest_path)?;
        } else {
            std::fs::copy(&path, &dest_path)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_execution_result() {
        let success = ExecutionResult::success("Done");
        assert!(success.success);
        assert_eq!(success.message, "Done");

        let failure = ExecutionResult::failure("Error");
        assert!(!failure.success);
        assert_eq!(failure.error, Some("Error".to_string()));
    }

    #[test]
    fn test_url_encoding() {
        assert_eq!(urlencoded("hello world"), "hello+world");
        assert_eq!(urlencoded("test"), "test");
    }
}
