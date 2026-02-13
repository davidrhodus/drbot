//! Tool system for agent capabilities.

use crate::tool_root::{
    ensure_root_dir, resolve_existing_dir, resolve_existing_file, resolve_write_file_path,
};
use crate::unified_diff::{apply_unified_diff_to_text, parse_unified_diff};
use crate::{AgentError, Result};
use async_trait::async_trait;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
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

#[derive(Debug, Clone, Default)]
pub struct BuiltinToolsOptions {
    pub bash_allowed_prefixes: Option<Vec<String>>,
    pub bash_safe_bins: Option<Vec<String>>,
    pub bash_extra_allowed_prefixes: Vec<String>,
}

impl BuiltinTools {
    /// Get all built-in tools with optional per-tool overrides.
    pub fn all_with_options(
        root: impl Into<PathBuf>,
        options: BuiltinToolsOptions,
    ) -> Result<Vec<Arc<dyn AgentTool>>> {
        let root = ensure_root_dir(&root.into()).map_err(AgentError::ToolError)?;

        let mut bash = BashTool::new(root.clone());
        if let Some(allowed) = options.bash_allowed_prefixes {
            bash = bash.with_allowed_commands(allowed);
        }
        if let Some(safe) = options.bash_safe_bins {
            bash = bash.with_safe_bins(safe);
        }
        if !options.bash_extra_allowed_prefixes.is_empty() {
            bash = bash.with_extra_allowed_commands(options.bash_extra_allowed_prefixes);
        }
        let bash = Arc::new(bash);
        let exec_alias = Arc::new(AliasTool::new("exec", bash.clone()));

        let read_file = Arc::new(ReadFileTool::new(root.clone()));
        let read_alias = Arc::new(AliasTool::new("read", read_file.clone()));

        let write_file = Arc::new(WriteFileTool::new(root.clone()));
        let write_alias = Arc::new(AliasTool::new("write", write_file.clone()));

        let edit_tool = Arc::new(EditTool::new(root.clone()));

        let list_directory = Arc::new(ListDirectoryTool::new(root.clone()));
        let list_dir_alias = Arc::new(AliasTool::new("list_dir", list_directory.clone()));

        Ok(vec![
            bash,
            exec_alias,
            read_file,
            read_alias,
            write_file,
            write_alias,
            edit_tool,
            list_directory,
            list_dir_alias,
            Arc::new(SearchTool::new(root.clone())),
            Arc::new(ApplyPatchTool::new(root)),
            Arc::new(HttpTool),
            Arc::new(CalculatorTool),
        ])
    }

    /// Get all built-in tools.
    pub fn all(root: impl Into<PathBuf>) -> Result<Vec<Arc<dyn AgentTool>>> {
        Self::all_with_options(root, BuiltinToolsOptions::default())
    }
}

/// Tool for executing bash commands.
pub struct BashTool {
    root: PathBuf,
    /// Allowed command prefixes (for sandboxing).
    allowed_prefixes: Vec<String>,
    /// Safe binaries allowed in allowlist mode (stdin-only / no path-like args).
    safe_bins: HashSet<String>,
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
            || allowlist
                .iter()
                .any(|s| s == "*" || s.eq_ignore_ascii_case("all"));

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

        let safe_bins_raw = std::env::var("DRBOT_OPENCLAW_AGENT_BASH_SAFE_BINS")
            .ok()
            .or_else(|| std::env::var("DRBOT_AGENT_BASH_SAFE_BINS").ok());
        let default_safe_bins = [
            "jq", "grep", "cut", "sort", "uniq", "head", "tail", "tr", "wc",
        ];
        let safe_bins_list: Vec<String> = match safe_bins_raw.as_deref().map(|s| s.trim()) {
            Some("") => Vec::new(),
            Some(raw) => raw
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
            None => default_safe_bins.iter().map(|s| s.to_string()).collect(),
        };
        let safe_bins: HashSet<String> = safe_bins_list
            .into_iter()
            .map(|s| s.trim().to_ascii_lowercase())
            .filter(|s| !s.is_empty())
            .collect();

        Self {
            root,
            allowed_prefixes,
            safe_bins,
            timeout_secs: 300,
        }
    }

    pub fn with_allowed_commands(mut self, commands: Vec<String>) -> Self {
        self.allowed_prefixes = commands;
        self
    }

    pub fn with_extra_allowed_commands(mut self, commands: Vec<String>) -> Self {
        if self.allowed_prefixes.is_empty() {
            return self;
        }
        let mut seen: HashSet<String> = self.allowed_prefixes.iter().cloned().collect();
        for cmd in commands {
            let trimmed = cmd.trim();
            if trimmed.is_empty() {
                continue;
            }
            if seen.insert(trimmed.to_string()) {
                self.allowed_prefixes.push(trimmed.to_string());
            }
        }
        self
    }

    pub fn with_safe_bins(mut self, bins: Vec<String>) -> Self {
        self.safe_bins = bins
            .into_iter()
            .map(|s| s.trim().to_ascii_lowercase())
            .filter(|s| !s.is_empty())
            .collect();
        self
    }

    fn strip_outer_quotes(mut token: &str) -> &str {
        loop {
            let bytes = token.as_bytes();
            if bytes.len() < 2 {
                return token;
            }
            let first = bytes[0];
            let last = bytes[bytes.len() - 1];
            if (first == b'"' && last == b'"') || (first == b'\'' && last == b'\'') {
                token = &token[1..bytes.len() - 1];
                continue;
            }
            return token;
        }
    }

    fn is_path_like_token(token: &str) -> bool {
        let trimmed = token.trim();
        if trimmed.is_empty() {
            return false;
        }
        if trimmed == "-" {
            return false;
        }
        if trimmed.starts_with("./") || trimmed.starts_with("../") || trimmed.starts_with('~') {
            return true;
        }
        if trimmed.starts_with('/') {
            return true;
        }
        // Windows drive letter path.
        let bytes = trimmed.as_bytes();
        if bytes.len() >= 3
            && bytes[1] == b':'
            && (bytes[2] == b'\\' || bytes[2] == b'/')
            && bytes[0].is_ascii_alphabetic()
        {
            return true;
        }
        false
    }

    fn safe_bin_usage(&self, part: &str, cwd: &std::path::Path) -> bool {
        if self.safe_bins.is_empty() {
            return false;
        }

        let lowered = part.to_ascii_lowercase();
        if lowered.contains("$(")
            || lowered.contains('`')
            || lowered.contains('>')
            || lowered.contains('<')
        {
            return false;
        }

        let tokens: Vec<&str> = part.split_whitespace().collect();
        if tokens.is_empty() {
            return false;
        }

        let mut i = 0usize;
        while i < tokens.len() && Self::looks_like_env_assignment(tokens[i]) {
            i += 1;
        }

        let exec_index = loop {
            if i >= tokens.len() {
                return false;
            }
            let tok_raw = tokens[i].trim();
            if tok_raw.is_empty() {
                i += 1;
                continue;
            }
            let tok_unquoted = Self::strip_outer_quotes(tok_raw);
            let tok = tok_unquoted.trim_start_matches('\\');
            let base = tok.rsplit('/').next().unwrap_or(tok);
            match base {
                "env" => {
                    i += 1;
                    while i < tokens.len()
                        && (tokens[i].starts_with('-')
                            || Self::looks_like_env_assignment(tokens[i]))
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
                _ => break i,
            }
        };

        let exec_raw = tokens[exec_index].trim();
        let exec_unquoted = Self::strip_outer_quotes(exec_raw);
        let exec_tok = exec_unquoted.trim_start_matches('\\');
        let exec_base = exec_tok.rsplit('/').next().unwrap_or(exec_tok);
        let exec_lower = exec_base.to_ascii_lowercase();
        if !self.safe_bins.contains(exec_lower.as_str()) {
            return false;
        }

        let args = &tokens[(exec_index + 1)..];
        for token_raw in args {
            let token_raw = token_raw.trim();
            if token_raw.is_empty() {
                continue;
            }
            let token = Self::strip_outer_quotes(token_raw);
            if token.is_empty() {
                continue;
            }
            if token == "-" {
                continue;
            }
            if token.starts_with('-') {
                if let Some((_, value)) = token.split_once('=') {
                    let value = Self::strip_outer_quotes(value);
                    if value.is_empty() {
                        continue;
                    }
                    if Self::is_path_like_token(value) {
                        return false;
                    }
                    let candidate = cwd.join(value);
                    if candidate.exists() {
                        return false;
                    }
                }
                continue;
            }

            if Self::is_path_like_token(token) {
                return false;
            }
            let candidate = cwd.join(token);
            if candidate.exists() {
                return false;
            }
        }

        true
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
        if !key.chars().all(|c| c == '_' || c.is_ascii_alphanumeric()) {
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
            let tok = Self::strip_outer_quotes(tok);
            let tok = tok.trim_start_matches('\\');
            let base = tok.rsplit('/').next().unwrap_or(tok);
            match base {
                // Treat common wrappers as transparent so allow/deny checks
                // cannot be bypassed with `env rm ...` / `command rm ...`.
                "env" => {
                    i += 1;
                    while i < tokens.len()
                        && (tokens[i].starts_with('-')
                            || Self::looks_like_env_assignment(tokens[i]))
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

    fn is_allowed(&self, command: &str, cwd: &std::path::Path) -> bool {
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
                if self
                    .allowed_prefixes
                    .iter()
                    .any(|p| exec == *p || exec.ends_with(&format!("/{}", p)))
                {
                    continue;
                }

                if self.safe_bin_usage(part, cwd) {
                    continue;
                }

                return false;
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
        "Execute a shell command and return the output"
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The shell command to execute"
                },
                "cmd": {
                    "type": "string",
                    "description": "Alias for command"
                },
                "cwd": {
                    "type": "string",
                    "description": "Optional working directory under the tool root"
                },
                "workdir": {
                    "type": "string",
                    "description": "Alias for cwd"
                },
                "timeout": {
                    "type": "number",
                    "description": "Optional timeout in seconds (overrides default)"
                },
                "env": {
                    "type": "object",
                    "description": "Optional environment variables (PATH and dynamic linker variables are forbidden)",
                    "additionalProperties": { "type": "string" }
                }
            },
            "required": ["command"]
        })
    }

    async fn execute(&self, args: Value) -> Result<String> {
        let command = args
            .get("command")
            .and_then(|v| v.as_str())
            .filter(|s| !s.trim().is_empty())
            .or_else(|| {
                args.get("cmd")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.trim().is_empty())
            })
            .ok_or_else(|| AgentError::ToolError("Missing 'command' argument".to_string()))?;

        let cwd_arg = args
            .get("cwd")
            .and_then(|v| v.as_str())
            .or_else(|| args.get("workdir").and_then(|v| v.as_str()))
            .unwrap_or("")
            .trim();
        let cwd = if cwd_arg.is_empty() {
            self.root.clone()
        } else {
            resolve_existing_dir(&self.root, cwd_arg).map_err(AgentError::ToolError)?
        };

        let mut env_vars: HashMap<String, String> = HashMap::new();
        if let Some(env) = args.get("env").and_then(|v| v.as_object()) {
            const DANGEROUS_HOST_ENV_VARS: &[&str] = &[
                "LD_PRELOAD",
                "LD_LIBRARY_PATH",
                "LD_AUDIT",
                "DYLD_INSERT_LIBRARIES",
                "DYLD_LIBRARY_PATH",
                "NODE_OPTIONS",
                "NODE_PATH",
                "PYTHONPATH",
                "PYTHONHOME",
                "RUBYLIB",
                "PERL5LIB",
                "BASH_ENV",
                "ENV",
                "GCONV_PATH",
                "IFS",
                "SSLKEYLOGFILE",
            ];

            for (key, value) in env {
                let key_trimmed = key.trim();
                if key_trimmed.is_empty() {
                    continue;
                }
                let normalized_key = key_trimmed.to_string();
                if normalized_key.chars().enumerate().any(|(idx, ch)| {
                    if idx == 0 {
                        !(ch == '_' || ch.is_ascii_alphabetic())
                    } else {
                        !(ch == '_' || ch.is_ascii_alphanumeric())
                    }
                }) {
                    return Err(AgentError::ToolError(format!(
                        "Invalid env var name: {}",
                        key_trimmed
                    )));
                }

                let upper = normalized_key.to_ascii_uppercase();
                if upper == "PATH" {
                    return Err(AgentError::ToolError(
                        "Refusing to run with custom PATH".to_string(),
                    ));
                }
                if upper.starts_with("LD_") || upper.starts_with("DYLD_") {
                    return Err(AgentError::ToolError(format!(
                        "Refusing to run with forbidden env var: {}",
                        key_trimmed
                    )));
                }
                if DANGEROUS_HOST_ENV_VARS
                    .iter()
                    .any(|v| v.eq_ignore_ascii_case(&upper))
                {
                    return Err(AgentError::ToolError(format!(
                        "Refusing to run with forbidden env var: {}",
                        key_trimmed
                    )));
                }

                let value_str = value.as_str().ok_or_else(|| {
                    AgentError::ToolError(format!("Invalid env var value for {}", key_trimmed))
                })?;
                env_vars.insert(normalized_key, value_str.to_string());
            }
        }

        if Self::command_is_forbidden(command) {
            return Err(AgentError::ToolError(format!(
                "Refusing to run forbidden command: {}",
                command
            )));
        }

        if !self.is_allowed(command, &cwd) {
            return Err(AgentError::ToolError(format!(
                "Command not allowed: {}",
                command
            )));
        }

        debug!("Executing shell command: {}", command);

        let timeout_secs = args
            .get("timeout")
            .and_then(|v| v.as_u64())
            .filter(|v| *v > 0)
            .unwrap_or(self.timeout_secs)
            .min(3600);

        fn resolve_powershell_path() -> String {
            let system_root = std::env::var("SystemRoot")
                .ok()
                .or_else(|| std::env::var("WINDIR").ok());
            if let Some(root) = system_root {
                let trimmed = root.trim();
                if !trimmed.is_empty() {
                    let candidate = PathBuf::from(trimmed)
                        .join("System32")
                        .join("WindowsPowerShell")
                        .join("v1.0")
                        .join("powershell.exe");
                    if candidate.exists() {
                        return candidate.to_string_lossy().to_string();
                    }
                }
            }
            "powershell.exe".to_string()
        }

        let output = if cfg!(windows) {
            let output = tokio::time::timeout(tokio::time::Duration::from_secs(timeout_secs), {
                let mut cmd = Command::new(resolve_powershell_path());
                cmd.arg("-NoProfile")
                    .arg("-NonInteractive")
                    .arg("-Command")
                    .arg(command)
                    .current_dir(&cwd)
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped());
                for (k, v) in &env_vars {
                    cmd.env(k, v);
                }
                cmd.output()
            })
            .await
            .map_err(|_| AgentError::Timeout)?;
            output.map_err(|e| AgentError::ToolError(e.to_string()))?
        } else {
            let bash_output =
                tokio::time::timeout(tokio::time::Duration::from_secs(timeout_secs), {
                    let mut cmd = Command::new("bash");
                    cmd.arg("-lc")
                        .arg(command)
                        .current_dir(&cwd)
                        .stdout(Stdio::piped())
                        .stderr(Stdio::piped());
                    for (k, v) in &env_vars {
                        cmd.env(k, v);
                    }
                    cmd.output()
                })
                .await
                .map_err(|_| AgentError::Timeout)?;

            match bash_output {
                Ok(output) => output,
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                    let sh_output =
                        tokio::time::timeout(tokio::time::Duration::from_secs(timeout_secs), {
                            let mut cmd = Command::new("sh");
                            cmd.arg("-c")
                                .arg(command)
                                .current_dir(&cwd)
                                .stdout(Stdio::piped())
                                .stderr(Stdio::piped());
                            for (k, v) in &env_vars {
                                cmd.env(k, v);
                            }
                            cmd.output()
                        })
                        .await
                        .map_err(|_| AgentError::Timeout)?;
                    sh_output.map_err(|e| AgentError::ToolError(e.to_string()))?
                }
                Err(err) => return Err(AgentError::ToolError(err.to_string())),
            }
        };

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
        let path = args
            .get("path")
            .and_then(|v| v.as_str())
            .filter(|s| !s.trim().is_empty())
            .or_else(|| {
                args.get("file_path")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.trim().is_empty())
            })
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
        let path = args
            .get("path")
            .and_then(|v| v.as_str())
            .filter(|s| !s.trim().is_empty())
            .or_else(|| {
                args.get("file_path")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.trim().is_empty())
            })
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

/// Tool for making precise edits to an existing file by replacing a specific
/// substring (`oldText`) with a new value (`newText`).
pub struct EditTool {
    root: PathBuf,
}

impl EditTool {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

#[async_trait]
impl AgentTool for EditTool {
    fn name(&self) -> &str {
        "edit"
    }

    fn description(&self) -> &str {
        "Make a precise edit to a file by replacing oldText with newText."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Path to the file to edit." },
                "file_path": { "type": "string", "description": "Alias for path." },
                "oldText": { "type": "string", "description": "Exact substring to replace." },
                "old_string": { "type": "string", "description": "Alias for oldText." },
                "newText": { "type": "string", "description": "Replacement substring." },
                "new_string": { "type": "string", "description": "Alias for newText." },
                "replaceAll": { "type": "boolean", "description": "If true, replace all occurrences (default false)." }
            },
            "required": ["path", "oldText", "newText"]
        })
    }

    async fn execute(&self, args: Value) -> Result<String> {
        let path = args
            .get("path")
            .and_then(|v| v.as_str())
            .filter(|s| !s.trim().is_empty())
            .or_else(|| {
                args.get("file_path")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.trim().is_empty())
            })
            .ok_or_else(|| AgentError::ToolError("Missing 'path' argument".to_string()))?;
        let old_text = args
            .get("oldText")
            .and_then(|v| v.as_str())
            .or_else(|| args.get("old_string").and_then(|v| v.as_str()))
            .ok_or_else(|| AgentError::ToolError("Missing 'oldText' argument".to_string()))?;
        let new_text = args
            .get("newText")
            .and_then(|v| v.as_str())
            .or_else(|| args.get("new_string").and_then(|v| v.as_str()))
            .ok_or_else(|| AgentError::ToolError("Missing 'newText' argument".to_string()))?;
        let replace_all = args
            .get("replaceAll")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if old_text.is_empty() {
            return Err(AgentError::ToolError(
                "'oldText' must be non-empty".to_string(),
            ));
        }

        let file = resolve_existing_file(&self.root, path).map_err(AgentError::ToolError)?;
        let bytes = tokio::fs::read(&file)
            .await
            .map_err(|e| AgentError::ToolError(format!("Failed to read file: {}", e)))?;
        let text = String::from_utf8_lossy(&bytes).to_string();

        let occurrences = text.match_indices(old_text).count();
        if occurrences == 0 {
            return Err(AgentError::ToolError(format!(
                "oldText not found in {}",
                file.display()
            )));
        }
        if occurrences > 1 && !replace_all {
            return Err(AgentError::ToolError(format!(
                "oldText occurs {} times in {} (set replaceAll=true to replace all)",
                occurrences,
                file.display()
            )));
        }

        let updated = if replace_all {
            text.replace(old_text, new_text)
        } else {
            text.replacen(old_text, new_text, 1)
        };

        tokio::fs::write(&file, updated.as_bytes())
            .await
            .map_err(|e| AgentError::ToolError(format!("Failed to write file: {}", e)))?;

        Ok(format!(
            "Successfully edited {} (replaced {} occurrence{})",
            file.display(),
            if replace_all { occurrences } else { 1 },
            if occurrences == 1 { "" } else { "s" }
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

        let target = crate::tool_root::resolve_existing_path(&self.root, path)
            .map_err(AgentError::ToolError)?;

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
            _ => tokio::time::timeout(
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
            .map_err(|e| AgentError::ToolError(format!("Search failed: {}", e)))?,
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
                tokio::fs::remove_file(&joined).await.map_err(|e| {
                    AgentError::ToolError(format!("Failed to delete '{}': {}", joined.display(), e))
                })?;
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
                let file = resolve_existing_file(&self.root, &target_path)
                    .map_err(AgentError::ToolError)?;
                let bytes = tokio::fs::read(&file).await.map_err(|e| {
                    AgentError::ToolError(format!("Failed to read '{}': {}", file.display(), e))
                })?;
                String::from_utf8_lossy(&bytes).to_string()
            };

            let mut updated =
                apply_unified_diff_to_text(&original, &fp.hunks).map_err(AgentError::ToolError)?;
            if old_path == "/dev/null" && !updated.is_empty() && !updated.ends_with('\n') {
                updated.push('\n');
            }

            let file =
                resolve_write_file_path(&self.root, &target_path).map_err(AgentError::ToolError)?;
            tokio::fs::write(&file, updated).await.map_err(|e| {
                AgentError::ToolError(format!("Failed to write '{}': {}", file.display(), e))
            })?;
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
        let dir = std::env::temp_dir().join(format!("drbot-agents-tools-test-{}", Uuid::new_v4()));
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
        assert!(tool.is_allowed("ls -la", &tool.root));
        assert!(tool.is_allowed("cat file.txt", &tool.root));
        assert!(!tool.is_allowed("rm -rf /", &tool.root));
        assert!(BashTool::command_is_forbidden("env rm -rf /"));
        assert!(BashTool::command_is_forbidden("command rm -rf /"));
        assert!(BashTool::command_is_forbidden("sudo ls"));
        assert!(BashTool::command_is_forbidden("\"rm\" -rf /"));

        let tool = BashTool::new(temp_root()).with_allowed_commands(vec!["git".to_string()]);
        assert!(tool.is_allowed("env git status", &tool.root));
    }

    #[test]
    fn test_bash_tool_safe_bins_allow_stdio_use() {
        let root = temp_root();
        std::fs::write(root.join("file.json"), r#"{"x":1}"#).unwrap();

        let tool = BashTool::new(root.clone())
            .with_allowed_commands(vec!["git".to_string()])
            .with_safe_bins(vec!["jq".to_string()]);

        assert!(tool.is_allowed("jq '.x'", &root));
        assert!(!tool.is_allowed("jq file.json", &root));
        assert!(!tool.is_allowed("jq .", &root));
        assert!(!tool.is_allowed("jq ./file.json", &root));
    }

    #[test]
    fn test_tool_registry() {
        let mut registry = ToolRegistry::new();
        registry.register(Arc::new(CalculatorTool));

        assert!(registry.get("calculator").is_some());
        assert!(registry.get("nonexistent").is_none());
    }

    #[test]
    fn test_builtin_tools_include_openclaw_aliases() {
        let tools = BuiltinTools::all(temp_root()).unwrap();
        let names: HashSet<String> = tools.into_iter().map(|t| t.name().to_string()).collect();
        assert!(names.contains("exec"));
        assert!(names.contains("read"));
        assert!(names.contains("write"));
        assert!(names.contains("edit"));
    }

    #[tokio::test]
    async fn test_read_file_tool_accepts_file_path_alias() {
        let root = temp_root();
        std::fs::write(root.join("foo.txt"), "hello").unwrap();
        let tool = ReadFileTool::new(root.clone());
        let out = tool
            .execute(serde_json::json!({ "file_path": "foo.txt" }))
            .await
            .unwrap();
        assert!(out.contains("hello"), "out={}", out);
    }

    #[tokio::test]
    async fn test_write_file_tool_accepts_file_path_alias() {
        let root = temp_root();
        let tool = WriteFileTool::new(root.clone());
        tool.execute(serde_json::json!({ "file_path": "bar.txt", "content": "world" }))
            .await
            .unwrap();
        assert_eq!(
            std::fs::read_to_string(root.join("bar.txt")).unwrap(),
            "world"
        );
    }

    #[tokio::test]
    async fn test_edit_tool_accepts_claude_style_aliases() {
        let root = temp_root();
        std::fs::write(root.join("edit.txt"), "hello world\n").unwrap();
        let tool = EditTool::new(root.clone());
        tool.execute(serde_json::json!({
            "file_path": "edit.txt",
            "old_string": "world",
            "new_string": "universe"
        }))
        .await
        .unwrap();
        assert_eq!(
            std::fs::read_to_string(root.join("edit.txt")).unwrap(),
            "hello universe\n"
        );
    }

    #[tokio::test]
    async fn test_bash_tool_accepts_cmd_and_workdir_aliases() {
        let root = temp_root();
        let tool = BashTool::new(root);
        let out = tool
            .execute(serde_json::json!({ "cmd": "echo hello", "workdir": "." }))
            .await
            .unwrap();
        assert!(out.contains("exit_code:"), "out={}", out);
        assert!(out.contains("hello"), "out={}", out);
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
