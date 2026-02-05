//! Tool system for agent capabilities.

use crate::{AgentError, Result};
use crate::tool_root::{
    ensure_root_dir, resolve_existing_dir, resolve_existing_file, resolve_write_file_path,
};
use crate::unified_diff::{apply_unified_diff_to_text, parse_unified_diff};
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use tokio::process::Command;
use tracing::{debug, warn};
use uuid::Uuid;

/// A tool that an agent can use.
#[async_trait]
pub trait AgentTool: Send + Sync {
    /// Tool name.
    fn name(&self) -> &str;

    /// Tool description.
    fn description(&self) -> &str;

    /// JSON schema for parameters.
    fn parameters(&self) -> Value;

    /// Execute the tool.
    async fn execute(&self, args: Value) -> Result<String>;
}

struct AliasTool {
    alias: String,
    inner: Arc<dyn AgentTool>,
}

impl AliasTool {
    fn new(alias: impl Into<String>, inner: Arc<dyn AgentTool>) -> Self {
        Self {
            alias: alias.into(),
            inner,
        }
    }
}

#[async_trait]
impl AgentTool for AliasTool {
    fn name(&self) -> &str {
        self.alias.as_str()
    }

    fn description(&self) -> &str {
        self.inner.description()
    }

    fn parameters(&self) -> Value {
        self.inner.parameters()
    }

    async fn execute(&self, args: Value) -> Result<String> {
        self.inner.execute(args).await
    }
}

fn truncate_for_context(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let truncated: String = text.chars().take(max_chars).collect();
    format!("{}...\n[truncated]", truncated)
}

async fn maybe_spool_tool_output(root: &PathBuf, tool: &str, output: String) -> Result<String> {
    const INLINE_LIMIT_CHARS: usize = 80_000;

    let char_count = output.chars().count();
    if char_count <= INLINE_LIMIT_CHARS {
        return Ok(output);
    }

    let dir = root.join(".drbot").join("tool-output");
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| AgentError::ToolError(format!("failed to create tool-output dir: {}", e)))?;

    let mut slug = tool
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>();
    slug = slug.trim_matches('-').to_string();
    if slug.is_empty() {
        slug = "tool".to_string();
    }

    let path = dir.join(format!("{}-{}.txt", slug, Uuid::new_v4()));
    tokio::fs::write(&path, output.as_bytes())
        .await
        .map_err(|e| AgentError::ToolError(format!("failed to write tool output: {}", e)))?;

    let rel = path
        .strip_prefix(root)
        .ok()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string());
    let truncated = truncate_for_context(&output, INLINE_LIMIT_CHARS);

    Ok(format!(
        "[output truncated: {} chars; full output saved to {}]\n{}",
        char_count, rel, truncated
    ))
}

/// Collection of built-in tools.
pub struct BuiltinTools;

impl BuiltinTools {
    /// Get all built-in tools.
    pub fn all(root: impl Into<PathBuf>) -> Result<Vec<Arc<dyn AgentTool>>> {
        let root = ensure_root_dir(&root.into()).map_err(AgentError::ToolError)?;

        let list_directory = Arc::new(ListDirectoryTool::new(root.clone()));
        let list_dir_alias = Arc::new(AliasTool::new("list_dir", list_directory.clone()));

        Ok(vec![
            Arc::new(BashTool::new(root.clone())),
            Arc::new(ReadFileTool::new(root.clone())),
            Arc::new(WriteFileTool::new(root.clone())),
            list_directory,
            list_dir_alias,
            Arc::new(SearchTool::new(root.clone())),
            Arc::new(ApplyPatchTool::new(root)),
            Arc::new(HttpTool),
            Arc::new(CalculatorTool),
        ])
    }
}

/// Tool for executing bash commands.
pub struct BashTool {
    root: PathBuf,
    /// Allowed command prefixes (for sandboxing).
    allowed_prefixes: Vec<String>,
    /// Timeout in seconds.
    timeout_secs: u64,
}

impl BashTool {
    pub fn new(root: PathBuf) -> Self {
        fn truthy(value: &str) -> bool {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        }

        let allow_all = std::env::var("DRBOT_OPENCLAW_AGENT_BASH_ALLOW_ALL")
            .ok()
            .as_deref()
            .map(truthy)
            .unwrap_or(false)
            || std::env::var("DRBOT_AGENT_BASH_ALLOW_ALL")
                .ok()
                .as_deref()
                .map(truthy)
                .unwrap_or(false);

        let allowlist_raw = std::env::var("DRBOT_OPENCLAW_AGENT_BASH_ALLOWLIST")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .or_else(|| {
                std::env::var("DRBOT_AGENT_BASH_ALLOWLIST")
                    .ok()
                    .filter(|v| !v.trim().is_empty())
            });

        let allowlist = allowlist_raw
            .as_deref()
            .map(|raw| {
                raw.split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let allow_all = allow_all
            || allowlist.iter().any(|s| s == "*" || s.eq_ignore_ascii_case("all"));

        let allowed_prefixes = if allow_all {
            Vec::new()
        } else if !allowlist.is_empty() {
            allowlist
        } else {
            vec![
                "ls".to_string(),
                "cat".to_string(),
                "echo".to_string(),
                "pwd".to_string(),
                "date".to_string(),
                "whoami".to_string(),
                "find".to_string(),
                "grep".to_string(),
                "head".to_string(),
                "tail".to_string(),
                "wc".to_string(),
                "sort".to_string(),
                "uniq".to_string(),
            ]
        };
        Self {
            root,
            allowed_prefixes,
            timeout_secs: 300,
        }
    }

    pub fn with_allowed_commands(mut self, commands: Vec<String>) -> Self {
        self.allowed_prefixes = commands;
        self
    }

    fn looks_like_env_assignment(token: &str) -> bool {
        let mut it = token.splitn(2, '=');
        let key = it.next().unwrap_or("");
        let value = it.next();
        if value.is_none() {
            return false;
        }
        if key.is_empty() || key.starts_with('-') {
            return false;
        }
        if !key
            .chars()
            .all(|c| c == '_' || c.is_ascii_alphanumeric())
        {
            return false;
        }
        // POSIX env var name must not start with a digit.
        if key
            .chars()
            .next()
            .map(|c| c.is_ascii_digit())
            .unwrap_or(false)
        {
            return false;
        }
        true
    }

    fn first_executable_token(part: &str) -> Option<String> {
        let tokens: Vec<&str> = part.split_whitespace().collect();
        if tokens.is_empty() {
            return None;
        }

        let mut i = 0usize;
        while i < tokens.len() && Self::looks_like_env_assignment(tokens[i]) {
            i += 1;
        }

        while i < tokens.len() {
            let tok = tokens[i].trim();
            if tok.is_empty() {
                i += 1;
                continue;
            }
            let tok = tok.trim_start_matches('\\');
            let base = tok.rsplit('/').next().unwrap_or(tok);
            match base {
                // Treat common wrappers as transparent so allow/deny checks
                // cannot be bypassed with `env rm ...` / `command rm ...`.
                "env" => {
                    i += 1;
                    while i < tokens.len()
                        && (tokens[i].starts_with('-') || Self::looks_like_env_assignment(tokens[i]))
                    {
                        i += 1;
                    }
                    continue;
                }
                "command" | "builtin" => {
                    i += 1;
                    while i < tokens.len() && tokens[i].starts_with('-') {
                        i += 1;
                    }
                    continue;
                }
                "nice" | "nohup" | "time" => {
                    i += 1;
                    while i < tokens.len() && tokens[i].starts_with('-') {
                        i += 1;
                    }
                    continue;
                }
                _ => return Some(base.to_string()),
            }
        }

        None
    }

    fn command_is_forbidden(command: &str) -> bool {
        const FORBIDDEN_COMMANDS: &[&str] = &["sudo", "rm", "mkfs", "dd", "shutdown", "reboot"];

        let cmd = command.trim();
        if cmd.is_empty() {
            return true;
        }

        let normalized = cmd.replace("&&", ";").replace("||", ";").replace('\n', ";");
        for segment in normalized.split(';') {
            let segment = segment.trim();
            if segment.is_empty() {
                continue;
            }
            for part in segment.split('|') {
                let part = part.trim();
                if part.is_empty() {
                    continue;
                }
                if let Some(first) = Self::first_executable_token(part) {
                    if FORBIDDEN_COMMANDS.contains(&first.as_str()) {
                        return true;
                    }
                }
            }
        }

        false
    }

    fn is_allowed(&self, command: &str) -> bool {
        if self.allowed_prefixes.is_empty() {
            return true; // No restrictions
        }

        let cmd = command.trim();
        if cmd.is_empty() {
            return false;
        }

        let normalized = cmd.replace("&&", ";").replace("||", ";").replace('\n', ";");
        for segment in normalized.split(';') {
            let segment = segment.trim();
            if segment.is_empty() {
                continue;
            }
            for part in segment.split('|') {
                let part = part.trim();
                if part.is_empty() {
                    continue;
                }
                let Some(exec) = Self::first_executable_token(part) else {
                    return false;
                };
                if !self
                    .allowed_prefixes
                    .iter()
                    .any(|p| exec == *p || exec.ends_with(&format!("/{}", p)))
                {
                    return false;
                }
            }
        }
        true
    }
}

#[async_trait]
impl AgentTool for BashTool {
    fn name(&self) -> &str {
        "bash"
    }

    fn description(&self) -> &str {
        "Execute a bash command and return the output"
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The bash command to execute"
                },
                "cwd": {
                    "type": "string",
                    "description": "Optional working directory under the tool root"
                }
            },
            "required": ["command"]
        })
    }

    async fn execute(&self, args: Value) -> Result<String> {
        let command = args["command"]
            .as_str()
            .ok_or_else(|| AgentError::ToolError("Missing 'command' argument".to_string()))?;

        if Self::command_is_forbidden(command) {
            return Err(AgentError::ToolError(format!(
                "Refusing to run forbidden command: {}",
                command
            )));
        }

        if !self.is_allowed(command) {
            return Err(AgentError::ToolError(format!("Command not allowed: {}", command)));
        }

        let cwd_arg = args.get("cwd").and_then(|v| v.as_str()).unwrap_or("").trim();
        let cwd = if cwd_arg.is_empty() {
            self.root.clone()
        } else {
            resolve_existing_dir(&self.root, cwd_arg).map_err(AgentError::ToolError)?
        };

        debug!("Executing bash command: {}", command);

        let output = tokio::time::timeout(
            tokio::time::Duration::from_secs(self.timeout_secs),
            Command::new("bash")
                .arg("-lc")
                .arg(command)
                .current_dir(&cwd)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output(),
        )
        .await
        .map_err(|_| AgentError::Timeout)?
        .map_err(|e| AgentError::ToolError(e.to_string()))?;

        let code = output.status.code().unwrap_or(-1);
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        let mut out = String::new();
        out.push_str(&format!("exit_code: {}\n", code));
        if !stdout.trim().is_empty() {
            out.push_str("stdout:\n");
            out.push_str(&stdout);
            if !stdout.ends_with('\n') {
                out.push('\n');
            }
        }
        if !stderr.trim().is_empty() {
            out.push_str("stderr:\n");
            out.push_str(&stderr);
            if !stderr.ends_with('\n') {
                out.push('\n');
            }
        }

        let out = maybe_spool_tool_output(&self.root, "bash", out).await?;
        if code == 0 {
            Ok(out.trim_end().to_string())
        } else {
            Err(AgentError::ToolError(out.trim_end().to_string()))
        }
    }
}

/// Tool for reading files.
pub struct ReadFileTool {
    root: PathBuf,
}

impl ReadFileTool {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

#[async_trait]
impl AgentTool for ReadFileTool {
    fn name(&self) -> &str {
        "read_file"
    }

    fn description(&self) -> &str {
        "Read the contents of a file"
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the file to read"
                }
            },
            "required": ["path"]
        })
    }

    async fn execute(&self, args: Value) -> Result<String> {
        let path = args["path"]
            .as_str()
            .ok_or_else(|| AgentError::ToolError("Missing 'path' argument".to_string()))?;

        let file = resolve_existing_file(&self.root, path).map_err(AgentError::ToolError)?;
        let bytes = tokio::fs::read(&file)
            .await
            .map_err(|e| AgentError::ToolError(format!("Failed to read file: {}", e)))?;
        let text = String::from_utf8_lossy(&bytes).to_string();
        Ok(truncate_for_context(&text, 120_000))
    }
}

/// Tool for writing files.
pub struct WriteFileTool {
    root: PathBuf,
}

impl WriteFileTool {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

#[async_trait]
impl AgentTool for WriteFileTool {
    fn name(&self) -> &str {
        "write_file"
    }

    fn description(&self) -> &str {
        "Write content to a file"
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the file to write"
                },
                "content": {
                    "type": "string",
                    "description": "Content to write to the file"
                }
            },
            "required": ["path", "content"]
        })
    }

    async fn execute(&self, args: Value) -> Result<String> {
        let path = args["path"]
            .as_str()
            .ok_or_else(|| AgentError::ToolError("Missing 'path' argument".to_string()))?;
        let content = args["content"]
            .as_str()
            .ok_or_else(|| AgentError::ToolError("Missing 'content' argument".to_string()))?;

        let file = resolve_write_file_path(&self.root, path).map_err(AgentError::ToolError)?;
        tokio::fs::write(&file, content)
            .await
            .map_err(|e| AgentError::ToolError(format!("Failed to write file: {}", e)))?;

        Ok(format!(
            "Successfully wrote {} bytes to {}",
            content.len(),
            file.display()
        ))
    }
}

/// Tool for listing directory contents.
pub struct ListDirectoryTool {
    root: PathBuf,
}

impl ListDirectoryTool {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

#[async_trait]
impl AgentTool for ListDirectoryTool {
    fn name(&self) -> &str {
        "list_directory"
    }

    fn description(&self) -> &str {
        "List files and directories in a path"
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Directory path to list (defaults to '.')"
                }
            }
        })
    }

    async fn execute(&self, args: Value) -> Result<String> {
        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
        let dir = resolve_existing_dir(&self.root, path).map_err(AgentError::ToolError)?;

        let mut entries = tokio::fs::read_dir(&dir)
            .await
            .map_err(|e| AgentError::ToolError(format!("Failed to read directory: {}", e)))?;

        let mut items = Vec::new();
        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| AgentError::ToolError(e.to_string()))?
        {
            let file_type = entry.file_type().await.ok();
            let type_indicator = if file_type.map(|t| t.is_dir()).unwrap_or(false) {
                "/"
            } else {
                ""
            };
            items.push(format!(
                "{}{}",
                entry.file_name().to_string_lossy(),
                type_indicator
            ));
        }

        items.sort();
        Ok(items.join("\n"))
    }
}

/// Tool for searching text in files.
pub struct SearchTool {
    root: PathBuf,
    timeout_secs: u64,
}

impl SearchTool {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            timeout_secs: 120,
        }
    }
}

#[async_trait]
impl AgentTool for SearchTool {
    fn name(&self) -> &str {
        "search"
    }

    fn description(&self) -> &str {
        "Search for text patterns in files"
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "Text pattern to search for"
                },
                "path": {
                    "type": "string",
                    "description": "Directory or file to search in (defaults to '.')"
                }
            },
            "required": ["pattern"]
        })
    }

    async fn execute(&self, args: Value) -> Result<String> {
        let pattern = args["pattern"]
            .as_str()
            .ok_or_else(|| AgentError::ToolError("Missing 'pattern' argument".to_string()))?;
        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");

        let target =
            crate::tool_root::resolve_existing_path(&self.root, path).map_err(AgentError::ToolError)?;

        // Prefer ripgrep; fall back to grep.
        let rg_output = tokio::time::timeout(
            tokio::time::Duration::from_secs(self.timeout_secs),
            Command::new("rg")
                .args([
                    "-n",
                    "--hidden",
                    "--no-heading",
                    pattern,
                    target.to_string_lossy().as_ref(),
                ])
                .current_dir(&self.root)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output(),
        )
        .await;

        let output = match rg_output {
            Ok(Ok(out)) => out,
            _ => {
                tokio::time::timeout(
                    tokio::time::Duration::from_secs(self.timeout_secs),
                    Command::new("grep")
                        .args(["-R", "-n", pattern, target.to_string_lossy().as_ref()])
                        .current_dir(&self.root)
                        .stdout(Stdio::piped())
                        .stderr(Stdio::piped())
                        .output(),
                )
                .await
                .map_err(|_| AgentError::Timeout)?
                .map_err(|e| AgentError::ToolError(format!("Search failed: {}", e)))?
            }
        };

        let code = output.status.code().unwrap_or(-1);
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        if code == 1 && stdout.trim().is_empty() && stderr.trim().is_empty() {
            return Ok("No matches found".to_string());
        }

        let mut out = String::new();
        out.push_str(&format!("exit_code: {}\n", code));
        if !stdout.trim().is_empty() {
            out.push_str("stdout:\n");
            out.push_str(&stdout);
            if !stdout.ends_with('\n') {
                out.push('\n');
            }
        }
        if !stderr.trim().is_empty() {
            out.push_str("stderr:\n");
            out.push_str(&stderr);
            if !stderr.ends_with('\n') {
                out.push('\n');
            }
        }

        let out = maybe_spool_tool_output(&self.root, "search", out).await?;
        if code == 0 || code == 1 {
            Ok(out.trim_end().to_string())
        } else {
            Err(AgentError::ToolError(out.trim_end().to_string()))
        }
    }
}

/// Tool for applying unified diff patches under the tool root.
pub struct ApplyPatchTool {
    root: PathBuf,
}

impl ApplyPatchTool {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

#[async_trait]
impl AgentTool for ApplyPatchTool {
    fn name(&self) -> &str {
        "apply_patch"
    }

    fn description(&self) -> &str {
        "Apply a unified diff patch to files under the tool root"
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "patch": {
                    "type": "string",
                    "description": "Unified diff patch to apply"
                }
            },
            "required": ["patch"]
        })
    }

    async fn execute(&self, args: Value) -> Result<String> {
        let patch = args["patch"]
            .as_str()
            .ok_or_else(|| AgentError::ToolError("Missing 'patch' argument".to_string()))?;

        let files = parse_unified_diff(patch).map_err(AgentError::ToolError)?;
        if files.is_empty() {
            return Err(AgentError::ToolError(
                "apply_patch: no file patches found".to_string(),
            ));
        }

        let mut out_lines: Vec<String> = Vec::new();
        for fp in files {
            let old_path = fp.old_path.clone();
            let new_path = fp.new_path.clone();

            if old_path != "/dev/null" && new_path != "/dev/null" && old_path != new_path {
                return Err(AgentError::ToolError(format!(
                    "apply_patch: renames are not supported ({} -> {})",
                    old_path, new_path
                )));
            }

            if new_path == "/dev/null" {
                // Delete file.
                let canon = crate::tool_root::resolve_existing_path(&self.root, &old_path)
                    .map_err(AgentError::ToolError)?;
                if !canon.is_file() {
                    return Err(AgentError::ToolError(format!(
                        "apply_patch: not a file: {}",
                        old_path
                    )));
                }

                let joined = crate::tool_root::join_relative(&self.root, &old_path)
                    .map_err(AgentError::ToolError)?;
                tokio::fs::remove_file(&joined)
                    .await
                    .map_err(|e| AgentError::ToolError(format!("Failed to delete '{}': {}", joined.display(), e)))?;
                out_lines.push(format!("deleted {}", old_path));
                continue;
            }

            let target_path = new_path.clone();

            // For existing files, refuse to patch symlinks (safety).
            if old_path != "/dev/null" {
                let joined = crate::tool_root::join_relative(&self.root, &target_path)
                    .map_err(AgentError::ToolError)?;
                if let Ok(meta) = std::fs::symlink_metadata(&joined) {
                    if meta.file_type().is_symlink() {
                        return Err(AgentError::ToolError(format!(
                            "apply_patch: refusing to patch symlink '{}'",
                            target_path
                        )));
                    }
                }
            }

            let original = if old_path == "/dev/null" {
                String::new()
            } else {
                let file = resolve_existing_file(&self.root, &target_path).map_err(AgentError::ToolError)?;
                let bytes = tokio::fs::read(&file)
                    .await
                    .map_err(|e| AgentError::ToolError(format!("Failed to read '{}': {}", file.display(), e)))?;
                String::from_utf8_lossy(&bytes).to_string()
            };

            let mut updated =
                apply_unified_diff_to_text(&original, &fp.hunks).map_err(AgentError::ToolError)?;
            if old_path == "/dev/null" && !updated.is_empty() && !updated.ends_with('\n') {
                updated.push('\n');
            }

            let file = resolve_write_file_path(&self.root, &target_path).map_err(AgentError::ToolError)?;
            tokio::fs::write(&file, updated)
                .await
                .map_err(|e| AgentError::ToolError(format!("Failed to write '{}': {}", file.display(), e)))?;
            out_lines.push(format!("patched {}", target_path));
        }

        Ok(out_lines.join("\n"))
    }
}

/// Tool for making HTTP requests.
pub struct HttpTool;

#[async_trait]
impl AgentTool for HttpTool {
    fn name(&self) -> &str {
        "http"
    }

    fn description(&self) -> &str {
        "Make HTTP requests to URLs"
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "method": {
                    "type": "string",
                    "enum": ["GET", "POST"],
                    "description": "HTTP method"
                },
                "url": {
                    "type": "string",
                    "description": "URL to request"
                },
                "body": {
                    "type": "string",
                    "description": "Request body (for POST)"
                }
            },
            "required": ["method", "url"]
        })
    }

    async fn execute(&self, args: Value) -> Result<String> {
        let method = args["method"]
            .as_str()
            .ok_or_else(|| AgentError::ToolError("Missing 'method' argument".to_string()))?;
        let url = args["url"]
            .as_str()
            .ok_or_else(|| AgentError::ToolError("Missing 'url' argument".to_string()))?;

        let client = reqwest::Client::new();

        let response: reqwest::Response = match method.to_uppercase().as_str() {
            "GET" => client
                .get(url)
                .send()
                .await
                .map_err(|e| AgentError::ToolError(format!("HTTP request failed: {}", e)))?,
            "POST" => {
                let body = args["body"].as_str().unwrap_or("");
                client
                    .post(url)
                    .body(body.to_string())
                    .send()
                    .await
                    .map_err(|e| AgentError::ToolError(format!("HTTP request failed: {}", e)))?
            }
            _ => {
                return Err(AgentError::ToolError(format!(
                    "Unsupported method: {}",
                    method
                )))
            }
        };

        let status = response.status();
        let text: String = response
            .text()
            .await
            .map_err(|e| AgentError::ToolError(format!("Failed to read response: {}", e)))?;

        Ok(format!("Status: {}\n\n{}", status, text))
    }
}

/// Tool for basic calculations.
pub struct CalculatorTool;

#[async_trait]
impl AgentTool for CalculatorTool {
    fn name(&self) -> &str {
        "calculator"
    }

    fn description(&self) -> &str {
        "Perform basic mathematical calculations"
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "expression": {
                    "type": "string",
                    "description": "Mathematical expression to evaluate (e.g., '2 + 2', '10 * 5')"
                }
            },
            "required": ["expression"]
        })
    }

    async fn execute(&self, args: Value) -> Result<String> {
        let expr = args["expression"]
            .as_str()
            .ok_or_else(|| AgentError::ToolError("Missing 'expression' argument".to_string()))?;

        // Simple expression evaluator
        let result = evaluate_simple_expr(expr)
            .map_err(|e| AgentError::ToolError(format!("Invalid expression: {}", e)))?;

        Ok(result.to_string())
    }
}

/// Simple expression evaluator for basic math.
fn evaluate_simple_expr(expr: &str) -> std::result::Result<f64, String> {
    let expr = expr.trim();

    // Try to parse as a single number first
    if let Ok(n) = expr.parse::<f64>() {
        return Ok(n);
    }

    // Find operator
    for op in ['+', '-', '*', '/', '%'] {
        if let Some(pos) = expr.rfind(op) {
            if pos > 0 {
                let left = evaluate_simple_expr(&expr[..pos])?;
                let right = evaluate_simple_expr(&expr[pos + 1..])?;

                return match op {
                    '+' => Ok(left + right),
                    '-' => Ok(left - right),
                    '*' => Ok(left * right),
                    '/' => {
                        if right == 0.0 {
                            Err("Division by zero".to_string())
                        } else {
                            Ok(left / right)
                        }
                    }
                    '%' => Ok(left % right),
                    _ => unreachable!(),
                };
            }
        }
    }

    Err(format!("Cannot evaluate: {}", expr))
}

/// Tool registry for managing available tools.
#[derive(Default)]
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn AgentTool>>,
}

impl ToolRegistry {
    /// Create a new registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a registry with builtin tools.
    pub fn with_builtins(root: impl Into<PathBuf>) -> Result<Self> {
        let mut registry = Self::new();
        for tool in BuiltinTools::all(root)? {
            registry.register(tool);
        }
        Ok(registry)
    }

    /// Register a tool.
    pub fn register(&mut self, tool: Arc<dyn AgentTool>) {
        self.tools.insert(tool.name().to_string(), tool);
    }

    /// Get a tool by name.
    pub fn get(&self, name: &str) -> Option<Arc<dyn AgentTool>> {
        self.tools.get(name).cloned()
    }

    /// List all tool names.
    pub fn list(&self) -> Vec<String> {
        self.tools.keys().cloned().collect()
    }

    /// Get all tools.
    pub fn all(&self) -> Vec<Arc<dyn AgentTool>> {
        self.tools.values().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "drbot-agents-tools-test-{}",
            Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        ensure_root_dir(&dir).unwrap()
    }

    #[test]
    fn test_calculator() {
        assert_eq!(evaluate_simple_expr("2 + 2").unwrap(), 4.0);
        assert_eq!(evaluate_simple_expr("10 * 5").unwrap(), 50.0);
        assert_eq!(evaluate_simple_expr("100 / 4").unwrap(), 25.0);
    }

    #[test]
    fn test_bash_tool_allowed() {
        let tool = BashTool::new(temp_root());
        assert!(tool.is_allowed("ls -la"));
        assert!(tool.is_allowed("cat file.txt"));
        assert!(!tool.is_allowed("rm -rf /"));
        assert!(BashTool::command_is_forbidden("env rm -rf /"));
        assert!(BashTool::command_is_forbidden("command rm -rf /"));
        assert!(BashTool::command_is_forbidden("sudo ls"));

        let tool = BashTool::new(temp_root()).with_allowed_commands(vec!["git".to_string()]);
        assert!(tool.is_allowed("env git status"));
    }

    #[test]
    fn test_tool_registry() {
        let mut registry = ToolRegistry::new();
        registry.register(Arc::new(CalculatorTool));

        assert!(registry.get("calculator").is_some());
        assert!(registry.get("nonexistent").is_none());
    }

    #[tokio::test]
    async fn test_apply_patch_tool_patches_creates_and_deletes() {
        let root = temp_root();
        let tool = ApplyPatchTool::new(root.clone());

        // Patch existing file.
        let file = root.join("foo.txt");
        std::fs::write(&file, "a\nb\n").unwrap();
        let patch = "\
--- a/foo.txt
+++ b/foo.txt
@@ -1,2 +1,2 @@
 a
-b
+c
";
        let out = tool
            .execute(serde_json::json!({ "patch": patch }))
            .await
            .unwrap();
        assert!(out.contains("patched foo.txt"), "out={}", out);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "a\nc\n");

        // Create new file.
        let patch = "\
--- /dev/null
+++ b/new.txt
@@ -0,0 +1,2 @@
+hello
+world
";
        tool.execute(serde_json::json!({ "patch": patch }))
            .await
            .unwrap();
        assert_eq!(
            std::fs::read_to_string(root.join("new.txt")).unwrap(),
            "hello\nworld\n"
        );

        // Delete file.
        let patch = "\
--- a/new.txt
+++ /dev/null
@@ -1,2 +0,0 @@
-hello
-world
";
        tool.execute(serde_json::json!({ "patch": patch }))
            .await
            .unwrap();
        assert!(!root.join("new.txt").exists());
    }

    #[tokio::test]
    async fn test_apply_patch_tool_rejects_rename() {
        let root = temp_root();
        std::fs::write(root.join("a.txt"), "x\n").unwrap();
        let tool = ApplyPatchTool::new(root.clone());

        let patch = "\
--- a/a.txt
+++ b/b.txt
@@ -1,1 +1,1 @@
-x
+y
";
        let err = tool
            .execute(serde_json::json!({ "patch": patch }))
            .await
            .unwrap_err();
        match err {
            AgentError::ToolError(msg) => assert!(msg.contains("renames"), "msg={}", msg),
            other => panic!("unexpected error: {:?}", other),
        }
    }
}
