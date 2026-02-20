use base64::Engine as _;
use reqwest::header::HeaderName;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::process::Command;

#[derive(Debug, Clone)]
pub struct IronToolHostConfig {
    pub workdir: PathBuf,
    pub fs_roots: Vec<PathBuf>,

    /// If set, only these tools may be invoked (in addition to tool-specific policies).
    pub allowed_tools: Option<BTreeSet<String>>,

    pub bash_allow_all: bool,
    pub bash_allow_prefixes: Vec<String>,
    pub bash_timeout: Duration,

    /// Maximum bytes returned by tools that emit large strings (best-effort).
    pub max_output_bytes: usize,

    /// Allowed HTTP hostnames/domains for `http.fetch`.
    pub http_allow_domains: Vec<String>,
    pub http_timeout: Duration,
    pub http_max_bytes: usize,

    /// SQLite-backed local KV store for dev.
    pub kv_path: Option<PathBuf>,
    pub kv_namespace: Option<String>,
    pub kv_max_value_bytes: usize,

    /// Host-injected secrets.
    pub secrets: BTreeMap<String, String>,

    /// If set, only these secret names may be requested.
    pub allowed_secret_names: Option<BTreeSet<String>>,
}

impl Default for IronToolHostConfig {
    fn default() -> Self {
        Self {
            workdir: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            fs_roots: Vec::new(),
            allowed_tools: None,
            bash_allow_all: false,
            bash_allow_prefixes: Vec::new(),
            bash_timeout: Duration::from_secs(60),
            max_output_bytes: 200_000,
            http_allow_domains: Vec::new(),
            http_timeout: Duration::from_secs(20),
            http_max_bytes: 1_000_000,
            kv_path: None,
            kv_namespace: None,
            kv_max_value_bytes: 1_000_000,
            secrets: BTreeMap::new(),
            allowed_secret_names: None,
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
            let err = self
                .payload
                .get("error")
                .cloned()
                .unwrap_or_else(|| json!({ "code": "UNKNOWN", "message": "unknown error" }));
            json!({ "ok": false, "error": err })
        };
        serde_json::to_string(&v).unwrap_or_else(|_| {
            "{\"ok\":false,\"error\":{\"code\":\"SERDE\",\"message\":\"failed to encode tool result\"}}"
                .to_string()
        })
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

        cfg.http_allow_domains = cfg
            .http_allow_domains
            .iter()
            .map(|s| s.trim().to_ascii_lowercase())
            .filter(|s| !s.is_empty())
            .collect();

        if let Some(ns) = cfg
            .kv_namespace
            .as_ref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            cfg.kv_namespace = Some(ns.to_string());
        } else {
            cfg.kv_namespace = None;
        }

        if let Some(kv_path) = cfg.kv_path.as_ref() {
            let resolved = if kv_path.is_absolute() {
                kv_path.clone()
            } else {
                workdir.join(kv_path)
            };
            cfg.kv_path = Some(resolved);
        }

        if let Some(allowed) = cfg.allowed_tools.as_mut() {
            let normalized: BTreeSet<String> = allowed
                .iter()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            *allowed = normalized;
        }

        if let Some(allowed) = cfg.allowed_secret_names.as_mut() {
            let normalized: BTreeSet<String> = allowed
                .iter()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            *allowed = normalized;
        }

        Self {
            workdir,
            fs_roots,
            cfg,
        }
    }

    #[allow(dead_code)] // Convenience accessor; used by some callers/tests.
    pub fn config(&self) -> &IronToolHostConfig {
        &self.cfg
    }

    pub async fn tool_invoke(&mut self, name: &str, args_json: &str) -> IronToolResult {
        let name = name.trim();
        if name.is_empty() {
            return IronToolResult::err("INVALID_REQUEST", "tool name required", None);
        }

        if let Some(allowed) = self.cfg.allowed_tools.as_ref() {
            if !allowed.contains(name) {
                return IronToolResult::err(
                    "DENIED",
                    "tool denied by policy",
                    Some(json!({ "tool": name })),
                );
            }
        }

        match name {
            "fs.read" => self.fs_read(args_json).await,
            "fs.write" => self.fs_write(args_json).await,
            "bash" => self.bash(args_json).await,
            "http.fetch" => self.http_fetch(args_json).await,
            "kv.get" => self.kv_get(args_json).await,
            "kv.put" => self.kv_put(args_json).await,
            "secrets.get" => self.secrets_get(args_json).await,
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
        let content = args.get("content").and_then(|v| v.as_str()).unwrap_or("");

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
        cmd.env_clear();
        cmd.env(
            "PATH",
            std::env::var("PATH").unwrap_or_else(|_| "/usr/bin:/bin".to_string()),
        );
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

    fn http_host_allowed(&self, host: &str) -> bool {
        if self.cfg.http_allow_domains.is_empty() {
            return false;
        }

        let host = host.trim().trim_end_matches('.').to_ascii_lowercase();
        if host.is_empty() {
            return false;
        }

        self.cfg.http_allow_domains.iter().any(|pat| {
            let pat = pat.trim().trim_end_matches('.').to_ascii_lowercase();
            if pat.is_empty() {
                return false;
            }
            if pat == "*" {
                return true;
            }
            if let Some(suffix) = pat.strip_prefix("*.") {
                return host != suffix && host.ends_with(&format!(".{}", suffix));
            }
            if let Some(suffix) = pat.strip_prefix('.') {
                return host != suffix && host.ends_with(&format!(".{}", suffix));
            }
            host == pat
        })
    }

    async fn http_fetch(&self, args_json: &str) -> IronToolResult {
        if self.cfg.http_allow_domains.is_empty() {
            return IronToolResult::err(
                "DENIED",
                "http.fetch is disabled (no allowed domains)",
                Some(json!({
                    "hint": "pass --allow-http-domain to drbot iron run/serve"
                })),
            );
        }

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

        let url = args
            .get("url")
            .or_else(|| args.get("href"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        if url.is_empty() {
            return IronToolResult::err("INVALID_REQUEST", "url required", None);
        }

        let method = args
            .get("method")
            .and_then(|v| v.as_str())
            .unwrap_or("GET")
            .trim()
            .to_ascii_uppercase();

        let parsed = match reqwest::Url::parse(url) {
            Ok(u) => u,
            Err(e) => {
                return IronToolResult::err("INVALID_REQUEST", &format!("invalid url: {}", e), None)
            }
        };

        match parsed.scheme() {
            "http" | "https" => {}
            other => {
                return IronToolResult::err(
                    "DENIED",
                    &format!("unsupported url scheme: {}", other),
                    None,
                )
            }
        }

        let host = parsed.host_str().unwrap_or("");
        if !self.http_host_allowed(host) {
            return IronToolResult::err(
                "DENIED",
                "http.fetch denied by allowlist",
                Some(json!({
                    "host": host,
                    "allowed": self.cfg.http_allow_domains.clone(),
                })),
            );
        }

        let timeout = args
            .get("timeoutMs")
            .or_else(|| args.get("timeout_ms"))
            .and_then(|v| v.as_u64())
            .map(|ms| Duration::from_millis(ms.clamp(100, 300_000)))
            .unwrap_or(self.cfg.http_timeout);

        let max_bytes = args
            .get("maxBytes")
            .or_else(|| args.get("max_bytes"))
            .and_then(|v| v.as_u64())
            .map(|v| v.max(1) as usize)
            .unwrap_or(self.cfg.http_max_bytes)
            .min(self.cfg.http_max_bytes)
            .max(1);

        let body = args.get("body").and_then(|v| v.as_str());
        if let Some(body) = body {
            if body.as_bytes().len() > max_bytes {
                return IronToolResult::err(
                    "INVALID_REQUEST",
                    "request body too large",
                    Some(json!({ "maxBytes": max_bytes })),
                );
            }
        }

        let client = match reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(timeout)
            .build()
        {
            Ok(c) => c,
            Err(e) => {
                return IronToolResult::err(
                    "HTTP_ERROR",
                    &format!("failed to build http client: {}", e),
                    None,
                )
            }
        };

        let method = match reqwest::Method::from_bytes(method.as_bytes()) {
            Ok(m) => m,
            Err(_) => {
                return IronToolResult::err(
                    "INVALID_REQUEST",
                    &format!("invalid method: {}", method),
                    None,
                )
            }
        };

        let mut req = client.request(method, parsed);

        if let Some(headers) = args.get("headers") {
            if let Some(obj) = headers.as_object() {
                for (k, v) in obj {
                    let name = match HeaderName::from_bytes(k.as_bytes()) {
                        Ok(n) => n,
                        Err(_) => continue,
                    };
                    let value = v.as_str().unwrap_or("");
                    req = req.header(name, value);
                }
            }
        }

        if let Some(body) = body {
            req = req.body(body.to_string());
        }

        let mut resp = match req.send().await {
            Ok(r) => r,
            Err(e) => {
                return IronToolResult::err("HTTP_ERROR", &format!("request failed: {}", e), None)
            }
        };

        let status = resp.status().as_u16();

        let mut headers_out = serde_json::Map::new();
        for (name, value) in resp.headers().iter() {
            if let Ok(v) = value.to_str() {
                headers_out.insert(name.as_str().to_string(), Value::String(v.to_string()));
            }
        }

        let mut buf: Vec<u8> = Vec::new();
        let mut truncated = false;
        loop {
            match resp.chunk().await {
                Ok(Some(chunk)) => {
                    let remaining = max_bytes.saturating_sub(buf.len());
                    if remaining == 0 {
                        truncated = true;
                        break;
                    }
                    if chunk.len() <= remaining {
                        buf.extend_from_slice(&chunk);
                    } else {
                        buf.extend_from_slice(&chunk[..remaining]);
                        truncated = true;
                        break;
                    }
                }
                Ok(None) => break,
                Err(e) => {
                    return IronToolResult::err(
                        "HTTP_ERROR",
                        &format!("failed to read response: {}", e),
                        None,
                    )
                }
            }
        }

        let (body_text, body_b64) = match String::from_utf8(buf.clone()) {
            Ok(s) => (Some(s), None),
            Err(_) => {
                let b64 = base64::engine::general_purpose::STANDARD_NO_PAD.encode(&buf);
                (None, Some(b64))
            }
        };

        IronToolResult::ok(json!({
            "status": status,
            "headers": headers_out,
            "bytes": buf.len(),
            "truncated": truncated,
            "body": body_text,
            "body_base64": body_b64,
        }))
    }

    async fn secrets_get(&self, args_json: &str) -> IronToolResult {
        if self.cfg.secrets.is_empty() {
            return IronToolResult::err(
                "DENIED",
                "secrets.get is disabled (no secrets configured)",
                Some(json!({
                    "hint": "pass --secret or --secrets-file to drbot iron run/serve"
                })),
            );
        }

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

        let name = args
            .get("name")
            .or_else(|| args.get("key"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        if name.is_empty() {
            return IronToolResult::err("INVALID_REQUEST", "name required", None);
        }

        if let Some(allowed) = self.cfg.allowed_secret_names.as_ref() {
            if !allowed.contains(name) {
                return IronToolResult::err(
                    "DENIED",
                    "secret name denied by policy",
                    Some(json!({ "name": name })),
                );
            }
        }

        let value = match self.cfg.secrets.get(name) {
            Some(v) => v.clone(),
            None => {
                return IronToolResult::err(
                    "NOT_FOUND",
                    "secret not found",
                    Some(json!({ "name": name })),
                )
            }
        };

        IronToolResult::ok(json!({ "name": name, "value": value }))
    }

    fn kv_enabled(&self) -> bool {
        self.cfg.kv_path.is_some()
    }

    fn kv_namespace(&self) -> String {
        self.cfg
            .kv_namespace
            .clone()
            .unwrap_or_else(|| "default".to_string())
    }

    async fn kv_get(&self, args_json: &str) -> IronToolResult {
        if !self.kv_enabled() {
            return IronToolResult::err(
                "DENIED",
                "kv.get is disabled (no kv path configured)",
                Some(json!({ "hint": "pass --kv-path to drbot iron run/serve" })),
            );
        }

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

        let key = args
            .get("key")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        if key.is_empty() {
            return IronToolResult::err("INVALID_REQUEST", "key required", None);
        }
        if key.len() > 1024 {
            return IronToolResult::err("INVALID_REQUEST", "key too long", None);
        }

        let ns = self.kv_namespace();
        let ns_db = ns.clone();
        let key_db = key.clone();
        let path = self.cfg.kv_path.clone().expect("kv path must exist");

        let res = tokio::task::spawn_blocking(move || -> anyhow::Result<Option<Vec<u8>>> {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).ok();
            }

            let conn = rusqlite::Connection::open(&path)?;
            conn.execute(
                "CREATE TABLE IF NOT EXISTS kv (\
                    namespace TEXT NOT NULL,\
                    key TEXT NOT NULL,\
                    value BLOB NOT NULL,\
                    updated_at INTEGER NOT NULL,\
                    PRIMARY KEY(namespace, key)\
                )",
                [],
            )?;

            let mut stmt = conn.prepare("SELECT value FROM kv WHERE namespace=?1 AND key=?2")?;
            let mut rows = stmt.query([ns_db.as_str(), key_db.as_str()])?;
            if let Some(row) = rows.next()? {
                let v: Vec<u8> = row.get(0)?;
                Ok(Some(v))
            } else {
                Ok(None)
            }
        })
        .await;

        let bytes = match res {
            Ok(Ok(v)) => v,
            Ok(Err(e)) => {
                return IronToolResult::err("KV_ERROR", &format!("kv.get failed: {}", e), None)
            }
            Err(e) => {
                return IronToolResult::err("KV_ERROR", &format!("kv.get join error: {}", e), None)
            }
        };

        match bytes {
            Some(bytes) => {
                let (value, value_base64) = match String::from_utf8(bytes.clone()) {
                    Ok(s) => (Some(s), None),
                    Err(_) => {
                        let b64 = base64::engine::general_purpose::STANDARD_NO_PAD.encode(&bytes);
                        (None, Some(b64))
                    }
                };
                IronToolResult::ok(json!({
                    "namespace": ns,
                    "key": key,
                    "found": true,
                    "value": value,
                    "value_base64": value_base64,
                }))
            }
            None => IronToolResult::ok(json!({
                "namespace": ns,
                "key": key,
                "found": false,
            })),
        }
    }

    async fn kv_put(&self, args_json: &str) -> IronToolResult {
        if !self.kv_enabled() {
            return IronToolResult::err(
                "DENIED",
                "kv.put is disabled (no kv path configured)",
                Some(json!({ "hint": "pass --kv-path to drbot iron run/serve" })),
            );
        }

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

        let key = args
            .get("key")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        if key.is_empty() {
            return IronToolResult::err("INVALID_REQUEST", "key required", None);
        }
        if key.len() > 1024 {
            return IronToolResult::err("INVALID_REQUEST", "key too long", None);
        }

        let value_json = args.get("value");
        let value = match value_json {
            Some(Value::String(s)) => s.clone(),
            Some(v) => match serde_json::to_string(v) {
                Ok(s) => s,
                Err(_) => return IronToolResult::err("INVALID_REQUEST", "invalid value", None),
            },
            None => return IronToolResult::err("INVALID_REQUEST", "value required", None),
        };

        if value.as_bytes().len() > self.cfg.kv_max_value_bytes {
            return IronToolResult::err(
                "INVALID_REQUEST",
                "value too large",
                Some(json!({ "maxBytes": self.cfg.kv_max_value_bytes })),
            );
        }

        let ns = self.kv_namespace();
        let ns_db = ns.clone();
        let key_db = key.clone();
        let path = self.cfg.kv_path.clone().expect("kv path must exist");

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        let bytes = value.as_bytes().to_vec();

        let res = tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).ok();
            }

            let conn = rusqlite::Connection::open(&path)?;
            conn.execute(
                "CREATE TABLE IF NOT EXISTS kv (\
                    namespace TEXT NOT NULL,\
                    key TEXT NOT NULL,\
                    value BLOB NOT NULL,\
                    updated_at INTEGER NOT NULL,\
                    PRIMARY KEY(namespace, key)\
                )",
                [],
            )?;

            conn.execute(
                "INSERT INTO kv (namespace, key, value, updated_at) VALUES (?1, ?2, ?3, ?4)\
                 ON CONFLICT(namespace, key) DO UPDATE SET value=excluded.value, updated_at=excluded.updated_at",
                rusqlite::params![ns_db, key_db, bytes, now],
            )?;

            Ok(())
        })
        .await;

        match res {
            Ok(Ok(())) => IronToolResult::ok(json!({
                "namespace": ns,
                "key": key,
                "bytes": value.len(),
            })),
            Ok(Err(e)) => IronToolResult::err("KV_ERROR", &format!("kv.put failed: {}", e), None),
            Err(e) => IronToolResult::err("KV_ERROR", &format!("kv.put join error: {}", e), None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn fs_read_denied_without_roots() {
        let mut host = IronToolHost::new(IronToolHostConfig {
            fs_roots: Vec::new(),
            ..Default::default()
        });

        let res = host
            .tool_invoke("fs.read", r#"{"path":"Cargo.toml"}"#)
            .await;
        assert!(!res.ok);
    }

    #[tokio::test]
    async fn fs_write_and_read_with_root() {
        let root = std::env::temp_dir().join(format!("drbot-iron-test-{}", uuid::Uuid::new_v4()));
        tokio::fs::create_dir_all(&root).await.unwrap();

        let mut host = IronToolHost::new(IronToolHostConfig {
            workdir: root.clone(),
            fs_roots: vec![root.clone()],
            ..Default::default()
        });

        let write_args = serde_json::json!({
            "path": "hello.txt",
            "content": "hello"
        })
        .to_string();
        let wrote = host.tool_invoke("fs.write", &write_args).await;
        assert!(wrote.ok);

        let read_args = serde_json::json!({"path":"hello.txt"}).to_string();
        let read = host.tool_invoke("fs.read", &read_args).await;
        assert!(read.ok);

        let content = read
            .payload
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        assert_eq!(content, "hello");
    }

    #[tokio::test]
    async fn bash_denied_by_default() {
        let mut host = IronToolHost::new(IronToolHostConfig::default());
        let res = host.tool_invoke("bash", r#"{"command":"echo hi"}"#).await;
        assert!(!res.ok);
    }

    #[tokio::test]
    async fn bash_allowed_with_prefix() {
        let mut host = IronToolHost::new(IronToolHostConfig {
            bash_allow_prefixes: vec!["echo".to_string()],
            ..Default::default()
        });

        let res = host.tool_invoke("bash", r#"{"command":"echo hi"}"#).await;
        assert!(res.ok);
    }

    #[tokio::test]
    async fn secrets_denied_by_default() {
        let mut host = IronToolHost::new(IronToolHostConfig::default());
        let res = host
            .tool_invoke("secrets.get", r#"{"name":"API_KEY"}"#)
            .await;
        assert!(!res.ok);
    }

    #[tokio::test]
    async fn secrets_get_ok() {
        let mut secrets = BTreeMap::new();
        secrets.insert("API_KEY".to_string(), "secret".to_string());

        let mut host = IronToolHost::new(IronToolHostConfig {
            secrets,
            ..Default::default()
        });

        let res = host
            .tool_invoke("secrets.get", r#"{"name":"API_KEY"}"#)
            .await;
        assert!(res.ok);
        assert_eq!(
            res.payload.get("value").and_then(|v| v.as_str()),
            Some("secret")
        );
    }

    #[tokio::test]
    async fn kv_put_get_roundtrip() {
        let root =
            std::env::temp_dir().join(format!("drbot-iron-kv-test-{}", uuid::Uuid::new_v4()));
        tokio::fs::create_dir_all(&root).await.unwrap();
        let kv_path = root.join("kv.sqlite");

        let mut host = IronToolHost::new(IronToolHostConfig {
            kv_path: Some(kv_path),
            kv_namespace: Some("ns".to_string()),
            ..Default::default()
        });

        let put_args = serde_json::json!({"key":"hello","value":"world"}).to_string();
        let put = host.tool_invoke("kv.put", &put_args).await;
        assert!(put.ok);

        let get_args = serde_json::json!({"key":"hello"}).to_string();
        let got = host.tool_invoke("kv.get", &get_args).await;
        assert!(got.ok);
        assert_eq!(
            got.payload.get("found").and_then(|v| v.as_bool()),
            Some(true)
        );
        assert_eq!(
            got.payload.get("value").and_then(|v| v.as_str()),
            Some("world")
        );

        tokio::fs::remove_dir_all(&root).await.ok();
    }
}
