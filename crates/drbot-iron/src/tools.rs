use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::process::Command;

#[derive(Debug, Clone)]
pub struct IronToolHostConfig {
    pub workdir: PathBuf,
    pub fs_roots: Vec<PathBuf>,
    pub bash_allow_all: bool,
    pub bash_allow_prefixes: Vec<String>,
    pub bash_timeout: Duration,
    pub max_output_bytes: usize,
}

impl Default for IronToolHostConfig {
    fn default() -> Self {
        Self {
            workdir: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            fs_roots: Vec::new(),
            bash_allow_all: false,
            bash_allow_prefixes: Vec::new(),
            bash_timeout: Duration::from_secs(60),
            max_output_bytes: 200_000,
        }
    }
}

#[derive(Debug, Clone)]
pub struct IronToolResult {
    pub ok: bool,
    pub payload: Value,
}

impl IronToolResult {
    pub fn ok(payload: Value) -> Self {
        Self { ok: true, payload }
    }

    pub fn err(code: &str, message: &str, details: Option<Value>) -> Self {
        let mut err = json!({ "code": code, "message": message });
        if let Some(d) = details {
            if let Some(obj) = err.as_object_mut() {
                obj.insert("details".to_string(), d);
            }
        }
        Self {
            ok: false,
            payload: json!({ "error": err }),
        }
    }

    pub fn to_json_string(&self) -> String {
        let v = if self.ok {
            json!({ "ok": true, "result": self.payload })
        } else {
            let err = self.payload.get("error").cloned().unwrap_or_else(|| {
                json!({ "code": "UNKNOWN", "message": "unknown error" })
            });
            json!({ "ok": false, "error": err })
        };
        serde_json::to_string(&v).unwrap_or_else(|_| "{\"ok\":false,\"error\":{\"code\":\"SERDE\",\"message\":\"failed to encode tool result\"}}".to_string())
    }
}

pub struct IronToolHost {
    workdir: PathBuf,
    fs_roots: Vec<PathBuf>,
    cfg: IronToolHostConfig,
}

impl IronToolHost {
    pub fn new(mut cfg: IronToolHostConfig) -> Self {
        let workdir = cfg
            .workdir
            .canonicalize()
            .unwrap_or_else(|_| cfg.workdir.clone());
        cfg.workdir = workdir.clone();

        let fs_roots: Vec<PathBuf> = cfg
            .fs_roots
            .iter()
            .filter_map(|p| {
                let trimmed = p.to_string_lossy();
                if trimmed.trim().is_empty() {
                    return None;
                }
                Some(p.canonicalize().unwrap_or_else(|_| p.clone()))
            })
            .collect();

        Self {
            workdir,
            fs_roots,
            cfg,
        }
    }

    pub fn config(&self) -> &IronToolHostConfig {
        &self.cfg
    }

    pub async fn tool_invoke(&mut self, name: &str, args_json: &str) -> IronToolResult {
        let name = name.trim();
        if name.is_empty() {
            return IronToolResult::err("INVALID_REQUEST", "tool name required", None);
        }

        match name {
            "fs.read" => self.fs_read(args_json).await,
            "fs.write" => self.fs_write(args_json).await,
            "bash" => self.bash(args_json).await,
            other => IronToolResult::err(
                "UNKNOWN_TOOL",
                &format!("unsupported tool: {}", other),
                None,
            ),
        }
    }

    fn expand_tilde(&self, input: &str) -> PathBuf {
        let trimmed = input.trim();
        if trimmed == "~" || trimmed.starts_with("~/") {
            if let Ok(home) = std::env::var("HOME") {
                let mut base = PathBuf::from(home);
                if trimmed.len() > 2 {
                    base.push(&trimmed[2..]);
                }
                return base;
            }
        }
        PathBuf::from(trimmed)
    }

    fn resolve_path(&self, input: &str, must_exist: bool) -> Result<PathBuf, String> {
        if self.fs_roots.is_empty() {
            return Err("filesystem access is disabled".to_string());
        }

        let raw = self.expand_tilde(input);
        let joined = if raw.is_absolute() {
            raw
        } else {
            self.workdir.join(raw)
        };

        let canon = if must_exist {
            joined
                .canonicalize()
                .map_err(|e| format!("failed to resolve path '{}': {}", joined.display(), e))?
        } else {
            let parent = joined
                .parent()
                .ok_or_else(|| format!("invalid path: {}", joined.display()))?;
            let parent_canon = parent.canonicalize().map_err(|e| {
                format!(
                    "failed to resolve parent directory '{}': {}",
                    parent.display(),
                    e
                )
            })?;
            let name = joined
                .file_name()
                .ok_or_else(|| format!("invalid path: {}", joined.display()))?;
            parent_canon.join(name)
        };

        let allowed = self.fs_roots.iter().any(|root| canon.starts_with(root));
        if !allowed {
            return Err(format!(
                "path '{}' is outside allowed roots",
                canon.display()
            ));
        }

        Ok(canon)
    }

    async fn fs_read(&self, args_json: &str) -> IronToolResult {
        let args: Value = match serde_json::from_str(args_json) {
            Ok(v) => v,
            Err(e) => {
                return IronToolResult::err(
                    "INVALID_REQUEST",
                    &format!("invalid JSON args: {}", e),
                    None,
                )
            }
        };
        let path = args
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        if path.is_empty() {
            return IronToolResult::err("INVALID_REQUEST", "path required", None);
        }

        let path = match self.resolve_path(path, true) {
            Ok(p) => p,
            Err(e) => return IronToolResult::err("DENIED", &e, None),
        };

        let content = match tokio::fs::read_to_string(&path).await {
            Ok(v) => v,
            Err(e) => {
                return IronToolResult::err(
                    "IO_ERROR",
                    &format!("failed to read {}: {}", path.display(), e),
                    None,
                )
            }
        };

        let mut content = content;
        if content.len() > self.cfg.max_output_bytes {
            content.truncate(self.cfg.max_output_bytes);
        }

        IronToolResult::ok(json!({ "path": path.to_string_lossy(), "content": content }))
    }

    async fn fs_write(&self, args_json: &str) -> IronToolResult {
        let args: Value = match serde_json::from_str(args_json) {
            Ok(v) => v,
            Err(e) => {
                return IronToolResult::err(
                    "INVALID_REQUEST",
                    &format!("invalid JSON args: {}", e),
                    None,
                )
            }
        };
        let path = args
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        if path.is_empty() {
            return IronToolResult::err("INVALID_REQUEST", "path required", None);
        }
        let content = args
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let path = match self.resolve_path(path, false) {
            Ok(p) => p,
            Err(e) => return IronToolResult::err("DENIED", &e, None),
        };

        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        if let Err(e) = tokio::fs::create_dir_all(parent).await {
            return IronToolResult::err(
                "IO_ERROR",
                &format!("failed to create dir {}: {}", parent.display(), e),
                None,
            );
        }

        if let Err(e) = tokio::fs::write(&path, content).await {
            return IronToolResult::err(
                "IO_ERROR",
                &format!("failed to write {}: {}", path.display(), e),
                None,
            );
        }

        IronToolResult::ok(json!({ "path": path.to_string_lossy(), "bytes": content.len() }))
    }

    fn bash_allowed(&self, command: &str) -> bool {
        if self.cfg.bash_allow_all {
            return true;
        }
        let cmd = command.trim();
        if cmd.is_empty() {
            return false;
        }
        self.cfg
            .bash_allow_prefixes
            .iter()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .any(|prefix| cmd.starts_with(prefix))
    }

    async fn bash(&self, args_json: &str) -> IronToolResult {
        let args: Value = match serde_json::from_str(args_json) {
            Ok(v) => v,
            Err(e) => {
                return IronToolResult::err(
                    "INVALID_REQUEST",
                    &format!("invalid JSON args: {}", e),
                    None,
                )
            }
        };

        let command = args
            .get("command")
            .or_else(|| args.get("cmd"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        if command.is_empty() {
            return IronToolResult::err("INVALID_REQUEST", "command required", None);
        }

        if !self.bash_allowed(&command) {
            return IronToolResult::err(
                "DENIED",
                "bash tool denied by policy",
                Some(json!({
                    "hint": "pass --allow-bash-prefix (or --allow-bash-all) to drbot iron run"
                })),
            );
        }

        let cwd = args.get("cwd").and_then(|v| v.as_str()).map(|s| s.trim());
        let cwd = match cwd {
            Some(cwd) if !cwd.is_empty() => match self.resolve_path(cwd, true) {
                Ok(p) => Some(p),
                Err(e) => return IronToolResult::err("DENIED", &e, None),
            },
            _ => None,
        };

        let timeout_ms = args
            .get("timeoutMs")
            .or_else(|| args.get("timeout_ms"))
            .and_then(|v| v.as_u64())
            .map(|ms| Duration::from_millis(ms.clamp(1_000, 900_000)))
            .unwrap_or(self.cfg.bash_timeout);

        let mut cmd = Command::new("sh");
        cmd.arg("-lc").arg(&command);
        if let Some(cwd) = cwd.as_ref() {
            cmd.current_dir(cwd);
        }

        let res = match tokio::time::timeout(timeout_ms, cmd.output()).await {
            Ok(Ok(out)) => out,
            Ok(Err(e)) => {
                return IronToolResult::err(
                    "EXEC_ERROR",
                    &format!("failed to execute: {}", e),
                    None,
                )
            }
            Err(_) => {
                return IronToolResult::err(
                    "TIMEOUT",
                    &format!("bash timed out after {:?}", timeout_ms),
                    None,
                )
            }
        };

        let mut stdout = String::from_utf8_lossy(&res.stdout).to_string();
        let mut stderr = String::from_utf8_lossy(&res.stderr).to_string();
        if stdout.len() > self.cfg.max_output_bytes {
            stdout.truncate(self.cfg.max_output_bytes);
        }
        if stderr.len() > self.cfg.max_output_bytes {
            stderr.truncate(self.cfg.max_output_bytes);
        }

        IronToolResult::ok(json!({
            "status": res.status.code().unwrap_or(-1),
            "success": res.status.success(),
            "stdout": stdout,
            "stderr": stderr,
        }))
    }
}
