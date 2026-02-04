//! OpenClaw "skills.*" method helpers.
//!
//! OpenClaw skills are Markdown files (`SKILL.md`) with a YAML frontmatter block.
//! The frontmatter contains `name`, `description`, and an optional `metadata` JSON5
//! blob with OpenClaw-specific requirements and installer specs.

use drbot_core::Config;
use drbot_protocol::openclaw::{error_codes, ErrorShape};
use futures::StreamExt;
use reqwest::header::{ETAG, IF_MODIFIED_SINCE, IF_NONE_MATCH, LAST_MODIFIED};
use ring::digest;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tracing::{debug, warn};

// Keep in sync with OpenClaw's default config truthiness checks.
const DEFAULT_CONFIG_VALUES: &[(&str, bool)] =
    &[("browser.enabled", true), ("browser.evaluateEnabled", true)];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenclawSkillsConfigFile {
    pub version: u32,
    #[serde(default)]
    pub entries: HashMap<String, OpenclawSkillConfigEntry>,
    /// Optional allowlist/denylist gates (drbot extension).
    ///
    /// OpenClaw itself only allowlists bundled skills via `skills.allowBundled`.
    /// drbot doesn't ship bundled OpenClaw skills today, but operators may still
    /// want to explicitly allow/deny managed/workspace skills.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowlist: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub denylist: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct OpenclawSkillConfigEntry {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub env: HashMap<String, String>,
    /// Optional remote SKILL.md source URL to sync into the managed skills dir.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Whether to sync relative Markdown docs referenced by the SKILL.md (drbot extension).
    ///
    /// Defaults to `true` when unset so remote skills can be self-contained.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fetch_relative_docs: Option<bool>,
    /// Additional relative docs to sync even if not referenced (drbot extension).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extra_docs: Vec<String>,
    /// Optional remote HEARTBEAT.md URL to sync into the managed skills dir.
    #[serde(
        default,
        rename = "heartbeatUrl",
        skip_serializing_if = "Option::is_none"
    )]
    pub heartbeat_url: Option<String>,
}

#[derive(Debug, Clone)]
struct Skill {
    name: String,
    description: String,
    source: String,
    file_path: PathBuf,
    base_dir: PathBuf,
}

#[derive(Debug, Clone)]
struct SkillEntry {
    skill: Skill,
    frontmatter: HashMap<String, String>,
    metadata: Option<OpenclawSkillMetadata>,
    invocation: SkillInvocationPolicy,
}

#[derive(Debug, Clone, Default)]
struct SkillInvocationPolicy {
    user_invocable: bool,
    disable_model_invocation: bool,
}

#[derive(Debug, Clone, Default)]
struct OpenclawSkillMetadata {
    always: Option<bool>,
    skill_key: Option<String>,
    primary_env: Option<String>,
    emoji: Option<String>,
    homepage: Option<String>,
    os: Option<Vec<String>>,
    requires: Option<OpenclawSkillRequires>,
    install: Option<Vec<SkillInstallSpec>>,
}

#[derive(Debug, Clone, Default)]
struct OpenclawSkillRequires {
    bins: Vec<String>,
    any_bins: Vec<String>,
    env: Vec<String>,
    config: Vec<String>,
}

#[derive(Debug, Clone)]
struct SkillInstallSpec {
    id: Option<String>,
    kind: SkillInstallKind,
    label: Option<String>,
    bins: Vec<String>,
    os: Vec<String>,
    formula: Option<String>,
    package: Option<String>,
    module: Option<String>,
    url: Option<String>,
    archive: Option<String>,
    extract: Option<bool>,
    strip_components: Option<u64>,
    target_dir: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SkillInstallKind {
    Brew,
    Node,
    Go,
    Uv,
    Download,
}

impl SkillInstallKind {
    fn as_str(self) -> &'static str {
        match self {
            SkillInstallKind::Brew => "brew",
            SkillInstallKind::Node => "node",
            SkillInstallKind::Go => "go",
            SkillInstallKind::Uv => "uv",
            SkillInstallKind::Download => "download",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillStatusConfigCheck {
    pub path: String,
    pub value: serde_json::Value,
    pub satisfied: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillInstallOption {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub bins: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillStatusRequirements {
    pub bins: Vec<String>,
    pub any_bins: Vec<String>,
    pub env: Vec<String>,
    pub config: Vec<String>,
    pub os: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillStatusEntry {
    pub name: String,
    pub description: String,
    pub source: String,
    #[serde(rename = "filePath")]
    pub file_path: String,
    #[serde(rename = "baseDir")]
    pub base_dir: String,
    #[serde(rename = "skillKey")]
    pub skill_key: String,
    #[serde(rename = "primaryEnv", skip_serializing_if = "Option::is_none")]
    pub primary_env: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emoji: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
    pub always: bool,
    pub disabled: bool,
    #[serde(rename = "blockedByAllowlist")]
    pub blocked_by_allowlist: bool,
    pub eligible: bool,
    pub requirements: SkillStatusRequirements,
    pub missing: SkillStatusRequirements,
    #[serde(rename = "configChecks")]
    pub config_checks: Vec<SkillStatusConfigCheck>,
    pub install: Vec<SkillInstallOption>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillStatusReport {
    pub workspace_dir: String,
    pub managed_skills_dir: String,
    pub skills: Vec<SkillStatusEntry>,
    pub snapshot_version: u64,
}

/// Remote environment eligibility context (OpenClaw parity).
///
/// OpenClaw can satisfy skill requirements via connected/persisted Nodes (e.g. a macOS
/// node that can run `system.run`). drbot uses this in `skills.status` and prompt
/// injection so OpenClaw UI can show the same eligibility results.
#[derive(Debug, Clone, Default)]
pub struct RemoteSkillEligibility {
    pub platforms: BTreeSet<String>,
    pub bins: BTreeSet<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillInstallResult {
    pub ok: bool,
    pub message: String,
    pub stdout: String,
    pub stderr: String,
    pub code: Option<i64>,
}

#[derive(Debug, Clone)]
pub(crate) struct SkillInstallPlan {
    pub skill_key: String,
    pub kind: String,
    pub os: Vec<String>,
    pub bins: Vec<String>,
    pub formula: Option<String>,
    pub package: Option<String>,
    pub module: Option<String>,
    pub url: Option<String>,
    pub archive: Option<String>,
    pub extract: Option<bool>,
    pub strip_components: Option<u64>,
    pub target_dir: Option<String>,
}

pub fn resolve_managed_skills_dir(cfg: &Config) -> PathBuf {
    crate::openclaw_paths::resolve_managed_skills_dir(cfg)
}

pub fn resolve_openclaw_skills_config_path() -> PathBuf {
    // Keep drbot's skills overrides separate from OpenClaw's `config.toml`.
    if let Some(dir) = Config::config_dir() {
        return dir.join("openclaw_skills.json");
    }
    PathBuf::from("openclaw_skills.json")
}

pub fn load_skills_config_file() -> OpenclawSkillsConfigFile {
    let path = resolve_openclaw_skills_config_path();
    let raw = std::fs::read_to_string(&path).ok();
    raw.and_then(|s| serde_json::from_str::<OpenclawSkillsConfigFile>(&s).ok())
        .unwrap_or(OpenclawSkillsConfigFile {
            version: 1,
            entries: HashMap::new(),
            allowlist: Vec::new(),
            denylist: Vec::new(),
        })
}

pub fn save_skills_config_file(file: &OpenclawSkillsConfigFile) -> Result<(), ErrorShape> {
    let path = resolve_openclaw_skills_config_path();
    write_json_atomic(&path, file)
}

pub fn update_skill_config(
    params: SkillsUpdateRequest,
) -> Result<OpenclawSkillConfigEntry, ErrorShape> {
    let mut file = load_skills_config_file();
    let entry = file.entries.entry(params.skill_key.clone()).or_default();

    if let Some(enabled) = params.enabled {
        entry.enabled = Some(enabled);
    }
    if let Some(api_key) = params.api_key {
        let trimmed = api_key.trim();
        if trimmed.is_empty() {
            entry.api_key = None;
        } else {
            entry.api_key = Some(trimmed.to_string());
        }
    }
    if let Some(env) = params.env {
        for (k, v) in env {
            let key = k.trim();
            if key.is_empty() {
                continue;
            }
            let trimmed = v.trim();
            if trimmed.is_empty() {
                entry.env.remove(key);
            } else {
                entry.env.insert(key.to_string(), trimmed.to_string());
            }
        }
    }
    if let Some(url) = params.url {
        let trimmed = url.trim();
        if trimmed.is_empty() {
            entry.url = None;
        } else {
            entry.url = Some(trimmed.to_string());
        }
    }
    if let Some(fetch_relative_docs) = params.fetch_relative_docs {
        entry.fetch_relative_docs = Some(fetch_relative_docs);
    }
    if let Some(extra_docs) = params.extra_docs {
        entry.extra_docs = extra_docs
            .into_iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>();
    }
    if let Some(url) = params.heartbeat_url {
        let trimmed = url.trim();
        if trimmed.is_empty() {
            entry.heartbeat_url = None;
        } else {
            entry.heartbeat_url = Some(trimmed.to_string());
        }
    }

    let snapshot = entry.clone();
    save_skills_config_file(&file)?;
    bump_skills_snapshot_version();
    Ok(snapshot)
}

// ---------------------------------------------------------------------------
// Remote skill docs sync (drbot extension)
// ---------------------------------------------------------------------------

const DEFAULT_REMOTE_SKILL_SYNC_MIN_INTERVAL_MS: u64 = 10 * 60 * 1000;
const DEFAULT_REMOTE_SKILL_SYNC_MAX_RELATIVE_DOCS: usize = 32;
const DEFAULT_REMOTE_SKILL_SYNC_MAX_FILE_BYTES: usize = 2 * 1024 * 1024;

static SKILLS_SNAPSHOT_VERSION: AtomicU64 = AtomicU64::new(0);

fn now_ms() -> u64 {
    chrono::Utc::now()
        .timestamp_millis()
        .try_into()
        .unwrap_or(0)
}

pub(crate) fn skills_snapshot_version() -> u64 {
    SKILLS_SNAPSHOT_VERSION.load(Ordering::Relaxed)
}

pub(crate) fn bump_skills_snapshot_version() -> u64 {
    let now = now_ms();
    SKILLS_SNAPSHOT_VERSION
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |cur| {
            Some(if now <= cur { cur.saturating_add(1) } else { now })
        })
        .unwrap_or_else(|cur| cur)
}

fn sha256_hex(raw: &str) -> String {
    let d = digest::digest(&digest::SHA256, raw.as_bytes());
    drbot_hex_util::encode(d.as_ref())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RemoteFileMeta {
    url: String,
    fetched_at_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    etag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    last_modified: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    sha256: Option<String>,
}

fn resolve_remote_meta_path(path: &Path) -> PathBuf {
    PathBuf::from(format!("{}.meta.json", path.to_string_lossy()))
}

fn read_json_file<T: for<'de> Deserialize<'de>>(path: &Path) -> Option<T> {
    let raw = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&raw).ok()
}

fn write_text_atomic(path: &Path, content: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    std::fs::write(&tmp, content)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

fn write_json_atomic_io<T: Serialize>(path: &Path, value: &T) -> std::io::Result<()> {
    let raw = serde_json::to_string_pretty(value).unwrap_or_else(|_| "{}".to_string());
    write_text_atomic(path, &raw)
}

fn resolve_remote_skill_sync_min_interval_ms() -> u64 {
    std::env::var("DRBOT_OPENCLAW_REMOTE_SKILLS_SYNC_MIN_INTERVAL_MS")
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .filter(|v| *v >= 1_000)
        .unwrap_or(DEFAULT_REMOTE_SKILL_SYNC_MIN_INTERVAL_MS)
}

fn resolve_remote_skill_sync_max_relative_docs() -> usize {
    std::env::var("DRBOT_OPENCLAW_REMOTE_SKILLS_SYNC_MAX_RELATIVE_DOCS")
        .ok()
        .and_then(|v| v.trim().parse::<usize>().ok())
        .filter(|v| *v >= 1)
        .unwrap_or(DEFAULT_REMOTE_SKILL_SYNC_MAX_RELATIVE_DOCS)
}

fn resolve_remote_skill_sync_max_file_bytes() -> usize {
    std::env::var("DRBOT_OPENCLAW_REMOTE_SKILLS_SYNC_MAX_FILE_BYTES")
        .ok()
        .and_then(|v| v.trim().parse::<usize>().ok())
        .filter(|v| *v >= 1_024)
        .unwrap_or(DEFAULT_REMOTE_SKILL_SYNC_MAX_FILE_BYTES)
}

async fn fetch_remote_text(
    client: &reqwest::Client,
    url: &str,
    meta: Option<&RemoteFileMeta>,
    max_bytes: usize,
) -> Result<(Option<String>, Option<String>, u16, Option<String>), String> {
    let mut req = client.get(url);
    if let Some(meta) = meta {
        if let Some(etag) = meta.etag.as_deref() {
            req = req.header(IF_NONE_MATCH, etag);
        }
        if let Some(lm) = meta.last_modified.as_deref() {
            req = req.header(IF_MODIFIED_SINCE, lm);
        }
    }

    let res = req.send().await.map_err(|e| e.to_string())?;
    let status = res.status().as_u16();
    let etag = res
        .headers()
        .get(ETAG)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let last_modified = res
        .headers()
        .get(LAST_MODIFIED)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    if status == 304 {
        return Ok((etag, last_modified, status, None));
    }
    if status < 200 || status >= 300 {
        return Err(format!("http {} for {}", status, url));
    }
    if let Some(len) = res.content_length() {
        if len as usize > max_bytes {
            return Err(format!("remote content too large ({} bytes) for {}", len, url));
        }
    }
    let body = res
        .bytes()
        .await
        .map_err(|e| e.to_string())
        .and_then(|b| {
            if b.len() > max_bytes {
                return Err(format!(
                    "remote content too large ({} bytes) for {}",
                    b.len(),
                    url
                ));
            }
            String::from_utf8(b.to_vec()).map_err(|e| e.to_string())
        })?;
    Ok((etag, last_modified, status, Some(body)))
}

async fn sync_remote_markdown(
    client: &reqwest::Client,
    url: &str,
    path: &Path,
    min_interval_ms: u64,
) -> Result<bool, String> {
    let meta_path = resolve_remote_meta_path(path);
    let meta = read_json_file::<RemoteFileMeta>(&meta_path);
    let prev_etag = meta.as_ref().and_then(|m| m.etag.clone());
    let prev_last_modified = meta.as_ref().and_then(|m| m.last_modified.clone());

    if let Some(meta) = meta.as_ref() {
        let elapsed = now_ms().saturating_sub(meta.fetched_at_ms);
        if elapsed < min_interval_ms {
            return Ok(false);
        }
    }

    let max_bytes = resolve_remote_skill_sync_max_file_bytes();
    let (etag, last_modified, status, maybe_body) =
        fetch_remote_text(client, url, meta.as_ref(), max_bytes).await?;
    let fetched_at_ms = now_ms();

    if status == 304 {
        let next = RemoteFileMeta {
            url: url.to_string(),
            fetched_at_ms,
            etag: etag.or(prev_etag),
            last_modified: last_modified.or(prev_last_modified),
            sha256: meta.as_ref().and_then(|m| m.sha256.clone()),
        };
        let _ = write_json_atomic_io(&meta_path, &next);
        return Ok(false);
    }

    let body = maybe_body.unwrap_or_default();
    let next_sha = sha256_hex(&body);
    let prev_sha = meta
        .as_ref()
        .and_then(|m| m.sha256.as_deref())
        .unwrap_or("");
    if !prev_sha.is_empty() && prev_sha == next_sha && path.exists() {
        // Update fetchedAt, but avoid rewriting unchanged content.
        let next = RemoteFileMeta {
            url: url.to_string(),
            fetched_at_ms,
            etag: etag.or(prev_etag),
            last_modified: last_modified.or(prev_last_modified),
            sha256: Some(next_sha),
        };
        let _ = write_json_atomic_io(&meta_path, &next);
        return Ok(false);
    }

    write_text_atomic(path, &body).map_err(|e| e.to_string())?;
    let next = RemoteFileMeta {
        url: url.to_string(),
        fetched_at_ms,
        etag,
        last_modified,
        sha256: Some(next_sha),
    };
    let _ = write_json_atomic_io(&meta_path, &next);
    Ok(true)
}

fn is_markdown_doc_path(path: &str) -> bool {
    let lower = path.trim().to_ascii_lowercase();
    lower.ends_with(".md") || lower.ends_with(".markdown")
}

fn extract_markdown_inline_link_targets(markdown: &str) -> Vec<String> {
    let bytes = markdown.as_bytes();
    let mut out: Vec<String> = Vec::new();
    let mut i = 0usize;
    while i + 1 < bytes.len() {
        if bytes[i] == b']' && bytes[i + 1] == b'(' {
            let start = i + 2;
            let mut end = start;
            while end < bytes.len() && bytes[end] != b')' {
                end += 1;
            }
            if end >= bytes.len() {
                break;
            }
            if let Some(target) = markdown.get(start..end) {
                out.push(target.to_string());
            }
            i = end + 1;
            continue;
        }
        i += 1;
    }
    out
}

fn normalize_relative_doc_path_from_target(target: &str) -> Option<PathBuf> {
    let trimmed = target.trim();
    if trimmed.is_empty() {
        return None;
    }
    let token = trimmed.split_whitespace().next().unwrap_or("");
    let token = token.trim();
    if token.is_empty() || token.starts_with('#') {
        return None;
    }
    let token = token.trim_start_matches('<').trim_end_matches('>');
    if token.contains("://")
        || token.starts_with("mailto:")
        || token.starts_with("data:")
        || token.starts_with("javascript:")
    {
        return None;
    }
    let path_part = token
        .split(|c| c == '#' || c == '?')
        .next()
        .unwrap_or(token)
        .trim();
    if path_part.is_empty() || !is_markdown_doc_path(path_part) {
        return None;
    }

    let mut raw = path_part;
    while raw.starts_with("./") {
        raw = &raw[2..];
    }
    if raw.starts_with('/') || raw.starts_with('\\') {
        return None;
    }

    let mut out = PathBuf::new();
    for comp in Path::new(raw).components() {
        match comp {
            Component::CurDir => {}
            Component::Normal(seg) => out.push(seg),
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    if out.as_os_str().is_empty() {
        None
    } else {
        Some(out)
    }
}

fn resolve_remote_skill_dir(cfg: &Config, skill_key: &str) -> PathBuf {
    resolve_managed_skills_dir(cfg).join(skill_key)
}

/// Best-effort sync for any skills configured with `entries.<skillKey>.url`.
///
/// This allows drbot to consume OpenClaw/AgentSkills-style remote skills (a
/// single `skill.md` URL) without requiring a separate installer step.
pub async fn sync_configured_remote_skills_best_effort(cfg: &Config) {
    let file = load_skills_config_file();
    if file.entries.is_empty() {
        return;
    }

    let min_interval_ms = resolve_remote_skill_sync_min_interval_ms();
    let ua = format!(
        "drbot/{} (+openclaw-remote-skill-sync)",
        env!("CARGO_PKG_VERSION")
    );
    let timeout_secs = std::env::var("DRBOT_OPENCLAW_REMOTE_SKILLS_SYNC_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .filter(|v| *v >= 1)
        .unwrap_or(20);
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(timeout_secs))
        .user_agent(ua)
        .build()
    {
        Ok(c) => c,
        Err(err) => {
            warn!(error = %err, "openclaw_skills: failed to build http client for remote skills");
            return;
        }
    };

    let max_relative_docs = resolve_remote_skill_sync_max_relative_docs();

    let mut updated_any = false;
    for (skill_key, entry) in file.entries {
        if entry.enabled == Some(false) {
            continue;
        }
        let Some(url) = entry.url.as_deref().map(|s| s.trim()).filter(|s| !s.is_empty()) else {
            continue;
        };
        let dir = resolve_remote_skill_dir(cfg, &skill_key);
        let skill_path = dir.join("SKILL.md");
        match sync_remote_markdown(&client, url, &skill_path, min_interval_ms).await {
            Ok(updated) => {
                updated_any = updated_any || updated;
                debug!(
                    updated,
                    skill_key = %skill_key,
                    path = %skill_path.to_string_lossy(),
                    "openclaw_skills: remote SKILL.md sync"
                );
            }
            Err(err) => warn!(skill_key = %skill_key, error = %err, "openclaw_skills: remote SKILL.md sync failed"),
        }

        // Best-effort: remote skills can reference additional docs in the same directory.
        // Sync those so prompt injection can include them without requiring a file-read tool.
        let fetch_relative = entry.fetch_relative_docs.unwrap_or(true);
        let mut doc_targets: Vec<String> = Vec::new();
        if fetch_relative {
            if let Ok(raw) = std::fs::read_to_string(&skill_path) {
                let body = strip_frontmatter(&raw);
                doc_targets.extend(extract_markdown_inline_link_targets(&body));
            }
        }
        if !entry.extra_docs.is_empty() {
            doc_targets.extend(entry.extra_docs.clone());
        }

        if !doc_targets.is_empty() {
            if let Ok(base_url) = reqwest::Url::parse(url) {
                if let Ok(dir_url) = base_url.join(".") {
                    let dir_prefix = dir_url.path().to_string();
                    let mut seen: HashSet<PathBuf> = HashSet::new();
                    let mut fetched = 0usize;
                    for target in doc_targets {
                        if fetched >= max_relative_docs {
                            break;
                        }
                        let token = target.trim();
                        let token = token.split_whitespace().next().unwrap_or("").trim();
                        let token = token.trim_start_matches('<').trim_end_matches('>');
                        let Some(rel_path) = normalize_relative_doc_path_from_target(token) else {
                            continue;
                        };
                        let leaf = rel_path.file_name().and_then(|s| s.to_str()).unwrap_or("");
                        if leaf.eq_ignore_ascii_case("SKILL.md")
                            || leaf.eq_ignore_ascii_case("HEARTBEAT.md")
                        {
                            continue;
                        }
                        if seen.contains(&rel_path) {
                            continue;
                        }
                        seen.insert(rel_path.clone());
                        fetched += 1;

                        let resolved = match dir_url.join(token) {
                            Ok(u) => u,
                            Err(_) => continue,
                        };
                        if resolved.scheme() != base_url.scheme()
                            || resolved.host_str() != base_url.host_str()
                            || !resolved.path().starts_with(&dir_prefix)
                        {
                            continue;
                        }

                        let dest = dir.join(&rel_path);
                        match sync_remote_markdown(
                            &client,
                            resolved.as_str(),
                            &dest,
                            min_interval_ms,
                        )
                        .await
                        {
                            Ok(updated) => {
                                updated_any = updated_any || updated;
                                debug!(
                                    updated,
                                    skill_key = %skill_key,
                                    path = %dest.to_string_lossy(),
                                    url = %resolved.as_str(),
                                    "openclaw_skills: remote relative doc sync"
                                );
                            }
                            Err(err) => warn!(
                                skill_key = %skill_key,
                                path = %dest.to_string_lossy(),
                                error = %err,
                                "openclaw_skills: remote relative doc sync failed"
                            ),
                        }
                    }
                } else {
                    warn!(
                        skill_key = %skill_key,
                        url = %url,
                        "openclaw_skills: remote skill base url resolve failed"
                    );
                }
            } else {
                warn!(
                    skill_key = %skill_key,
                    url = %url,
                    "openclaw_skills: remote skill url parse failed"
                );
            }
        }

        if let Some(heartbeat_url) = entry
            .heartbeat_url
            .as_deref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            let heartbeat_path = dir.join("HEARTBEAT.md");
            match sync_remote_markdown(&client, heartbeat_url, &heartbeat_path, min_interval_ms).await {
                Ok(updated) => {
                    updated_any = updated_any || updated;
                    debug!(
                        updated,
                        skill_key = %skill_key,
                        path = %heartbeat_path.to_string_lossy(),
                        "openclaw_skills: remote HEARTBEAT.md sync"
                    );
                }
                Err(err) => warn!(skill_key = %skill_key, error = %err, "openclaw_skills: remote HEARTBEAT.md sync failed"),
            }
        }
    }

    if updated_any {
        bump_skills_snapshot_version();
    }
}

#[derive(Debug, Clone)]
pub struct SkillsUpdateRequest {
    pub skill_key: String,
    pub enabled: Option<bool>,
    pub api_key: Option<String>,
    pub env: Option<HashMap<String, String>>,
    pub url: Option<String>,
    pub fetch_relative_docs: Option<bool>,
    pub extra_docs: Option<Vec<String>>,
    pub heartbeat_url: Option<String>,
}

pub fn build_skills_status_report(workspace_dir: &Path, cfg: &Config) -> SkillStatusReport {
    build_skills_status_report_with_remote(workspace_dir, cfg, None)
}

pub fn build_skills_status_report_with_remote(
    workspace_dir: &Path,
    cfg: &Config,
    remote: Option<&RemoteSkillEligibility>,
) -> SkillStatusReport {
    let managed_dir = resolve_managed_skills_dir(cfg);
    let entries = load_workspace_skill_entries(workspace_dir, &managed_dir);
    let config_json = serde_json::to_value(cfg).unwrap_or_else(|_| json!({}));
    let skill_config = load_skills_config_file();

    let mut skills = entries
        .into_iter()
        .map(|entry| build_skill_status_entry(&entry, &config_json, &skill_config, remote))
        .collect::<Vec<_>>();
    skills.sort_by(|a, b| a.name.cmp(&b.name));

    SkillStatusReport {
        workspace_dir: workspace_dir.to_string_lossy().to_string(),
        managed_skills_dir: managed_dir.to_string_lossy().to_string(),
        skills,
        snapshot_version: skills_snapshot_version(),
    }
}

pub fn collect_skill_bins(workspace_dirs: &[PathBuf], cfg: &Config) -> Vec<String> {
    let managed_dir = resolve_managed_skills_dir(cfg);
    let mut bins: BTreeSet<String> = BTreeSet::new();
    for workspace_dir in workspace_dirs {
        let entries = load_workspace_skill_entries(workspace_dir, &managed_dir);
        for entry in entries {
            let required = entry
                .metadata
                .as_ref()
                .and_then(|m| m.requires.as_ref())
                .map(|r| r.bins.clone())
                .unwrap_or_default();
            let any_bins = entry
                .metadata
                .as_ref()
                .and_then(|m| m.requires.as_ref())
                .map(|r| r.any_bins.clone())
                .unwrap_or_default();
            let install = entry
                .metadata
                .as_ref()
                .and_then(|m| m.install.as_ref())
                .cloned()
                .unwrap_or_default();
            for bin in required.into_iter().chain(any_bins) {
                let trimmed = bin.trim();
                if !trimmed.is_empty() {
                    bins.insert(trimmed.to_string());
                }
            }
            for spec in install {
                for bin in spec.bins {
                    let trimmed = bin.trim();
                    if !trimmed.is_empty() {
                        bins.insert(trimmed.to_string());
                    }
                }
            }
        }
    }
    bins.into_iter().collect()
}

/// Collect required bins for skills that are eligible on a given platform.
///
/// This matches OpenClaw's remote-node probing logic: only `requires.bins` and
/// `requires.anyBins` are included (installer bins are not).
pub fn collect_required_skill_bins_for_platform(
    workspace_dirs: &[PathBuf],
    cfg: &Config,
    platform: &str,
) -> Vec<String> {
    let managed_dir = resolve_managed_skills_dir(cfg);
    let mut bins: BTreeSet<String> = BTreeSet::new();
    for workspace_dir in workspace_dirs {
        let entries = load_workspace_skill_entries(workspace_dir, &managed_dir);
        for entry in entries {
            let required_os = entry
                .metadata
                .as_ref()
                .and_then(|m| m.os.as_ref())
                .cloned()
                .unwrap_or_default();
            if !required_os.is_empty() && !required_os.iter().any(|os| os == platform) {
                continue;
            }
            let requires = entry
                .metadata
                .as_ref()
                .and_then(|m| m.requires.as_ref());
            let Some(requires) = requires else { continue };

            for bin in requires.bins.iter().chain(requires.any_bins.iter()) {
                let trimmed = bin.trim();
                if !trimmed.is_empty() {
                    bins.insert(trimmed.to_string());
                }
            }
        }
    }
    bins.into_iter().collect()
}

/// Build the OpenClaw-style skills prompt for a workspace.
///
/// OpenClaw formats skills into a single prompt that is injected into model runs.
/// drbot keeps the formatting simple (concatenate eligible SKILL.md bodies).
pub fn build_workspace_skills_prompt(workspace_dir: &Path, cfg: &Config) -> String {
    build_workspace_skills_prompt_with_remote(workspace_dir, cfg, None)
}

pub fn build_workspace_skills_prompt_with_remote(
    workspace_dir: &Path,
    cfg: &Config,
    remote: Option<&RemoteSkillEligibility>,
) -> String {
    let managed_dir = resolve_managed_skills_dir(cfg);
    let entries = load_workspace_skill_entries(workspace_dir, &managed_dir);
    if entries.is_empty() {
        return String::new();
    }

    let config_json = serde_json::to_value(cfg).unwrap_or_else(|_| json!({}));
    let skill_config = load_skills_config_file();

    let mut bodies: Vec<String> = Vec::new();
    for entry in entries {
        if entry.invocation.disable_model_invocation {
            continue;
        }
        let status = build_skill_status_entry(&entry, &config_json, &skill_config, remote);
        if !status.eligible {
            continue;
        }
        let mut combined = String::new();

        let Ok(raw) = std::fs::read_to_string(&entry.skill.file_path) else {
            continue;
        };
        let body = strip_frontmatter(&raw);
        let body = body.trim();
        if body.is_empty() {
            continue;
        }
        combined.push_str(body);

        // Some ecosystem skills ship additional docs via relative links (progressive disclosure).
        // Include them best-effort so models can use them without requiring a file-read tool.
        let max_extra_bytes = std::env::var("DRBOT_OPENCLAW_SKILLS_PROMPT_MAX_EXTRA_DOC_BYTES")
            .ok()
            .and_then(|v| v.trim().parse::<u64>().ok())
            .filter(|v| *v >= 1_024)
            .unwrap_or(200_000);
        let mut used_extra_bytes = 0u64;

        let mut extras: Vec<PathBuf> = Vec::new();
        let mut seen: HashSet<PathBuf> = HashSet::new();
        let mut push_extra = |path: PathBuf| {
            if seen.contains(&path) {
                return;
            }
            let leaf = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if leaf.eq_ignore_ascii_case("SKILL.md") || leaf.eq_ignore_ascii_case("HEARTBEAT.md") {
                return;
            }
            seen.insert(path.clone());
            extras.push(path);
        };

        // Curated extra docs.
        let messaging = entry.skill.base_dir.join("MESSAGING.md");
        if messaging.is_file() {
            push_extra(PathBuf::from("MESSAGING.md"));
        }

        for target in extract_markdown_inline_link_targets(body) {
            if let Some(rel) = normalize_relative_doc_path_from_target(&target) {
                push_extra(rel);
            }
        }

        if let Some(cfg_entry) = skill_config.entries.get(&status.skill_key) {
            for target in &cfg_entry.extra_docs {
                if let Some(rel) = normalize_relative_doc_path_from_target(target) {
                    push_extra(rel);
                }
            }
        }

        for rel in extras {
            if used_extra_bytes >= max_extra_bytes {
                break;
            }
            let extra = entry.skill.base_dir.join(&rel);
            if !extra.is_file() {
                continue;
            }
            if let Ok(meta) = std::fs::metadata(&extra) {
                if meta.len() > max_extra_bytes {
                    continue;
                }
                if used_extra_bytes.saturating_add(meta.len()) > max_extra_bytes {
                    continue;
                }
                used_extra_bytes = used_extra_bytes.saturating_add(meta.len());
            }
            if let Ok(raw) = std::fs::read_to_string(&extra) {
                let body = strip_frontmatter(&raw);
                let body = body.trim();
                if !body.is_empty() {
                    combined.push_str("\n\n---\n\n");
                    combined.push_str(body);
                }
            }
        }

        bodies.push(combined);
    }

    if bodies.is_empty() {
        return String::new();
    }

    let mut out = String::new();
    out.push_str("Skills:\n\n");
    for (idx, body) in bodies.iter().enumerate() {
        if idx > 0 {
            out.push_str("\n\n---\n\n");
        }
        out.push_str(body);
    }
    out
}

/// List eligible OpenClaw skills for a workspace.
///
/// This is useful for integrations that want to discover skill-adjacent files
/// (for example, `HEARTBEAT.md`) without duplicating eligibility logic.
pub fn list_eligible_skill_dirs(workspace_dir: &Path, cfg: &Config) -> Vec<(String, PathBuf)> {
    list_eligible_skill_dirs_with_remote(workspace_dir, cfg, None)
}

pub fn list_eligible_skill_dirs_with_remote(
    workspace_dir: &Path,
    cfg: &Config,
    remote: Option<&RemoteSkillEligibility>,
) -> Vec<(String, PathBuf)> {
    let managed_dir = resolve_managed_skills_dir(cfg);
    let entries = load_workspace_skill_entries(workspace_dir, &managed_dir);
    if entries.is_empty() {
        return Vec::new();
    }

    let config_json = serde_json::to_value(cfg).unwrap_or_else(|_| json!({}));
    let skill_config = load_skills_config_file();

    let mut out = Vec::new();
    for entry in entries {
        if entry.invocation.disable_model_invocation {
            continue;
        }
        let status = build_skill_status_entry(&entry, &config_json, &skill_config, remote);
        if !status.eligible {
            continue;
        }
        out.push((entry.skill.name.clone(), entry.skill.base_dir.clone()));
    }

    // Stable ordering helps keep heartbeat prompts deterministic.
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

pub async fn run_skill_install(
    cfg: &Config,
    workspace_dir: &Path,
    skill_name: &str,
    install_id: &str,
    timeout_ms: Option<u64>,
) -> SkillInstallResult {
    let timeout_ms = timeout_ms.unwrap_or(300_000).clamp(1_000, 900_000);
    let managed_dir = resolve_managed_skills_dir(cfg);
    let entries = load_workspace_skill_entries(workspace_dir, &managed_dir);
    let Some(entry) = entries.into_iter().find(|e| e.skill.name == skill_name) else {
        return SkillInstallResult {
            ok: false,
            message: format!("Skill not found: {}", skill_name),
            stdout: String::new(),
            stderr: String::new(),
            code: None,
        };
    };

    let Some((spec, _index)) = find_install_spec(&entry, install_id) else {
        return SkillInstallResult {
            ok: false,
            message: format!("Installer not found: {}", install_id),
            stdout: String::new(),
            stderr: String::new(),
            code: None,
        };
    };

    match spec.kind {
        SkillInstallKind::Download => install_download_spec(cfg, &entry, &spec, timeout_ms).await,
        SkillInstallKind::Brew
        | SkillInstallKind::Node
        | SkillInstallKind::Go
        | SkillInstallKind::Uv => install_command_spec(&spec, timeout_ms).await,
    }
}

pub(crate) fn resolve_skill_install_plan(
    cfg: &Config,
    workspace_dir: &Path,
    skill_name: &str,
    install_id: &str,
) -> Result<SkillInstallPlan, SkillInstallResult> {
    let managed_dir = resolve_managed_skills_dir(cfg);
    let entries = load_workspace_skill_entries(workspace_dir, &managed_dir);
    let Some(entry) = entries.into_iter().find(|e| e.skill.name == skill_name) else {
        return Err(SkillInstallResult {
            ok: false,
            message: format!("Skill not found: {}", skill_name),
            stdout: String::new(),
            stderr: String::new(),
            code: None,
        });
    };

    let Some((spec, _index)) = find_install_spec(&entry, install_id) else {
        return Err(SkillInstallResult {
            ok: false,
            message: format!("Installer not found: {}", install_id),
            stdout: String::new(),
            stderr: String::new(),
            code: None,
        });
    };

    Ok(SkillInstallPlan {
        skill_key: resolve_skill_key(&entry),
        kind: spec.kind.as_str().to_string(),
        os: spec.os.clone(),
        bins: spec.bins.clone(),
        formula: spec.formula.clone(),
        package: spec.package.clone(),
        module: spec.module.clone(),
        url: spec.url.clone(),
        archive: spec.archive.clone(),
        extract: spec.extract,
        strip_components: spec.strip_components,
        target_dir: spec.target_dir.clone(),
    })
}

// ---------------------------------------------------------------------------
// Skill status helpers
// ---------------------------------------------------------------------------

fn build_skill_status_entry(
    entry: &SkillEntry,
    cfg_json: &serde_json::Value,
    skills_cfg: &OpenclawSkillsConfigFile,
    remote: Option<&RemoteSkillEligibility>,
) -> SkillStatusEntry {
    let skill_key = resolve_skill_key(entry);
    let skill_cfg = skills_cfg.entries.get(&skill_key);
    let disabled = skill_cfg.and_then(|c| c.enabled).unwrap_or(true) == false;
    let blocked_by_allowlist = is_blocked_by_skill_allowlist(entry, &skill_key, skills_cfg);
    let always = entry
        .metadata
        .as_ref()
        .and_then(|m| m.always)
        .unwrap_or(false);

    let emoji = entry
        .metadata
        .as_ref()
        .and_then(|m| m.emoji.clone())
        .or_else(|| entry.frontmatter.get("emoji").cloned());
    let homepage = entry
        .metadata
        .as_ref()
        .and_then(|m| m.homepage.clone())
        .or_else(|| entry.frontmatter.get("homepage").cloned())
        .or_else(|| entry.frontmatter.get("website").cloned())
        .or_else(|| entry.frontmatter.get("url").cloned())
        .and_then(|s| {
            let trimmed = s.trim().to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        });

    let primary_env = entry.metadata.as_ref().and_then(|m| m.primary_env.clone());
    let required_bins = entry
        .metadata
        .as_ref()
        .and_then(|m| m.requires.as_ref())
        .map(|r| r.bins.clone())
        .unwrap_or_default();
    let required_any_bins = entry
        .metadata
        .as_ref()
        .and_then(|m| m.requires.as_ref())
        .map(|r| r.any_bins.clone())
        .unwrap_or_default();
    let required_env = entry
        .metadata
        .as_ref()
        .and_then(|m| m.requires.as_ref())
        .map(|r| r.env.clone())
        .unwrap_or_default();
    let required_config = entry
        .metadata
        .as_ref()
        .and_then(|m| m.requires.as_ref())
        .map(|r| r.config.clone())
        .unwrap_or_default();
    let required_os = entry
        .metadata
        .as_ref()
        .and_then(|m| m.os.clone())
        .unwrap_or_default();

    let platform = resolve_runtime_platform();

    let remote_has_bin = |bin: &str| -> bool {
        remote
            .map(|r| r.bins.contains(bin.trim()))
            .unwrap_or(false)
    };

    let missing_bins = required_bins
        .iter()
        .filter(|bin| !has_binary(bin) && !remote_has_bin(bin))
        .cloned()
        .collect::<Vec<_>>();
    let missing_any_bins =
        if required_any_bins.is_empty()
            || required_any_bins.iter().any(|bin| has_binary(bin))
            || remote
                .map(|r| required_any_bins.iter().any(|bin| r.bins.contains(bin.trim())))
                .unwrap_or(false)
        {
            Vec::new()
        } else {
            required_any_bins.clone()
        };
    let missing_os = if required_os.is_empty()
        || required_os.iter().any(|os| os == platform)
        || remote
            .map(|r| {
                r.platforms
                    .iter()
                    .any(|p| required_os.iter().any(|os| os == p))
            })
            .unwrap_or(false)
    {
        Vec::new()
    } else {
        required_os.clone()
    };

    let mut missing_env: Vec<String> = Vec::new();
    for env_name in &required_env {
        if std::env::var_os(env_name).is_some() {
            continue;
        }
        if let Some(cfg) = skill_cfg {
            if cfg.env.get(env_name).is_some() {
                continue;
            }
            if cfg.api_key.is_some() && primary_env.as_deref() == Some(env_name) {
                continue;
            }
        }
        missing_env.push(env_name.clone());
    }

    let mut config_checks: Vec<SkillStatusConfigCheck> = Vec::new();
    let mut missing_config: Vec<String> = Vec::new();
    for path_str in &required_config {
        let value = resolve_config_path(cfg_json, path_str)
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        let satisfied = is_config_path_truthy(cfg_json, path_str);
        config_checks.push(SkillStatusConfigCheck {
            path: path_str.clone(),
            value,
            satisfied,
        });
        if !satisfied {
            missing_config.push(path_str.clone());
        }
    }

    let missing = if always {
        SkillStatusRequirements {
            bins: Vec::new(),
            any_bins: Vec::new(),
            env: Vec::new(),
            config: Vec::new(),
            os: Vec::new(),
        }
    } else {
        SkillStatusRequirements {
            bins: missing_bins,
            any_bins: missing_any_bins,
            env: missing_env,
            config: missing_config,
            os: missing_os,
        }
    };

    let eligible = !disabled
        && !blocked_by_allowlist
        && (always
            || (missing.bins.is_empty()
                && missing.any_bins.is_empty()
                && missing.env.is_empty()
                && missing.config.is_empty()
                && missing.os.is_empty()));

    SkillStatusEntry {
        name: entry.skill.name.clone(),
        description: entry.skill.description.clone(),
        source: entry.skill.source.clone(),
        file_path: entry.skill.file_path.to_string_lossy().to_string(),
        base_dir: entry.skill.base_dir.to_string_lossy().to_string(),
        skill_key,
        primary_env,
        emoji,
        homepage,
        always,
        disabled,
        blocked_by_allowlist,
        eligible,
        requirements: SkillStatusRequirements {
            bins: required_bins,
            any_bins: required_any_bins,
            env: required_env,
            config: required_config,
            os: required_os,
        },
        missing,
        config_checks,
        install: normalize_install_options(entry, remote),
    }
}

fn normalize_install_options(
    entry: &SkillEntry,
    remote: Option<&RemoteSkillEligibility>,
) -> Vec<SkillInstallOption> {
    let install = entry
        .metadata
        .as_ref()
        .and_then(|m| m.install.as_ref())
        .cloned()
        .unwrap_or_default();
    if install.is_empty() {
        return Vec::new();
    }
    let platform = resolve_runtime_platform();
    let mut platforms: Vec<&str> = vec![platform];
    let remote_platforms: Vec<String> = remote
        .map(|r| r.platforms.iter().cloned().collect())
        .unwrap_or_default();
    for p in &remote_platforms {
        platforms.push(p.as_str());
    }
    let filtered = install
        .into_iter()
        .enumerate()
        .filter(|(_, spec)| {
            spec.os.is_empty() || spec.os.iter().any(|os| platforms.iter().any(|p| os == *p))
        })
        .collect::<Vec<_>>();
    if filtered.is_empty() {
        return Vec::new();
    }

    let all_downloads = filtered
        .iter()
        .all(|(_, spec)| spec.kind == SkillInstallKind::Download);
    let to_option = |spec: &SkillInstallSpec, index: usize| -> SkillInstallOption {
        let id = resolve_install_id(spec, index);
        let bins = spec.bins.clone();
        let label = build_install_label(spec, index);
        SkillInstallOption {
            id,
            kind: spec.kind.as_str().to_string(),
            label,
            bins,
        }
    };

    if all_downloads {
        return filtered
            .iter()
            .map(|(idx, spec)| to_option(spec, *idx))
            .collect();
    }

    let preferred = select_preferred_install_spec(&filtered);
    match preferred {
        Some((idx, spec)) => vec![to_option(&spec, idx)],
        None => Vec::new(),
    }
}

fn resolve_skills_install_prefer_brew() -> bool {
    match std::env::var("DRBOT_OPENCLAW_SKILLS_INSTALL_PREFER_BREW")
        .ok()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "" => true,
        "1" | "true" | "t" | "yes" | "y" | "on" => true,
        "0" | "false" | "f" | "no" | "n" | "off" => false,
        _ => true,
    }
}

fn resolve_skills_install_node_manager() -> String {
    let raw = std::env::var("DRBOT_OPENCLAW_SKILLS_INSTALL_NODE_MANAGER")
        .ok()
        .unwrap_or_default();
    let manager = raw.trim().to_ascii_lowercase();
    match manager.as_str() {
        "pnpm" | "yarn" | "bun" | "npm" => manager,
        _ => "npm".to_string(),
    }
}

fn build_node_install_command(package_name: &str) -> Vec<String> {
    match resolve_skills_install_node_manager().as_str() {
        "pnpm" => vec![
            "pnpm".to_string(),
            "add".to_string(),
            "-g".to_string(),
            package_name.to_string(),
        ],
        "yarn" => vec![
            "yarn".to_string(),
            "global".to_string(),
            "add".to_string(),
            package_name.to_string(),
        ],
        "bun" => vec![
            "bun".to_string(),
            "add".to_string(),
            "-g".to_string(),
            package_name.to_string(),
        ],
        _ => vec![
            "npm".to_string(),
            "install".to_string(),
            "-g".to_string(),
            package_name.to_string(),
        ],
    }
}

fn select_preferred_install_spec(
    specs: &[(usize, SkillInstallSpec)],
) -> Option<(usize, SkillInstallSpec)> {
    let find_kind = |k: SkillInstallKind| specs.iter().find(|(_, s)| s.kind == k).cloned();
    let brew = find_kind(SkillInstallKind::Brew);
    let node = find_kind(SkillInstallKind::Node);
    let go = find_kind(SkillInstallKind::Go);
    let uv = find_kind(SkillInstallKind::Uv);

    if resolve_skills_install_prefer_brew() && has_binary("brew") {
        if let Some(brew) = brew.clone() {
            return Some(brew);
        }
    }
    if let Some(uv) = uv {
        return Some(uv);
    }
    if let Some(node) = node {
        return Some(node);
    }
    if let Some(brew) = brew {
        return Some(brew);
    }
    if let Some(go) = go {
        return Some(go);
    }
    specs.first().cloned()
}

fn build_install_label(spec: &SkillInstallSpec, _index: usize) -> String {
    let explicit = spec.label.as_deref().unwrap_or("").trim();
    if !explicit.is_empty() {
        return explicit.to_string();
    }

    match spec.kind {
        SkillInstallKind::Brew => spec
            .formula
            .as_deref()
            .map(|f| format!("Install {} (brew)", f))
            .unwrap_or_else(|| "Run installer".to_string()),
        SkillInstallKind::Node => spec
            .package
            .as_deref()
            .map(|p| format!("Install {} ({})", p, resolve_skills_install_node_manager()))
            .unwrap_or_else(|| "Run installer".to_string()),
        SkillInstallKind::Go => spec
            .module
            .as_deref()
            .map(|m| format!("Install {} (go)", m))
            .unwrap_or_else(|| "Run installer".to_string()),
        SkillInstallKind::Uv => spec
            .package
            .as_deref()
            .map(|p| format!("Install {} (uv)", p))
            .unwrap_or_else(|| "Run installer".to_string()),
        SkillInstallKind::Download => {
            let url = spec.url.as_deref().unwrap_or("").trim();
            if url.is_empty() {
                return "Run installer".to_string();
            }
            let last = url.split('/').last().unwrap_or(url);
            format!("Download {}", if last.is_empty() { url } else { last })
        }
    }
}

fn resolve_skill_key(entry: &SkillEntry) -> String {
    entry
        .metadata
        .as_ref()
        .and_then(|m| m.skill_key.clone())
        .unwrap_or_else(|| entry.skill.name.clone())
}

fn parse_skill_list_env(var: &str) -> Vec<String> {
    std::env::var(var)
        .ok()
        .map(|raw| {
            raw.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn is_blocked_by_skill_allowlist(
    entry: &SkillEntry,
    skill_key: &str,
    skills_cfg: &OpenclawSkillsConfigFile,
) -> bool {
    let env_allow = parse_skill_list_env("DRBOT_OPENCLAW_SKILLS_ALLOWLIST");
    let env_deny = parse_skill_list_env("DRBOT_OPENCLAW_SKILLS_DENYLIST");

    let allow = if !env_allow.is_empty() {
        env_allow
    } else {
        skills_cfg.allowlist.clone()
    };

    let mut deny = skills_cfg.denylist.clone();
    deny.extend(env_deny);

    if deny.iter().any(|s| s == skill_key || s == &entry.skill.name) {
        return true;
    }

    if allow.is_empty() {
        return false;
    }
    !allow.iter().any(|s| s == skill_key || s == &entry.skill.name)
}

// ---------------------------------------------------------------------------
// Load skills
// ---------------------------------------------------------------------------

fn load_workspace_skill_entries(workspace_dir: &Path, managed_dir: &Path) -> Vec<SkillEntry> {
    let workspace_skills_dir = workspace_dir.join("skills");
    let managed = load_skill_entries_from_dir(managed_dir, "openclaw-managed");
    let workspace = load_skill_entries_from_dir(&workspace_skills_dir, "openclaw-workspace");

    let mut merged: HashMap<String, SkillEntry> = HashMap::new();
    for entry in managed {
        merged.insert(entry.skill.name.clone(), entry);
    }
    for entry in workspace {
        merged.insert(entry.skill.name.clone(), entry);
    }
    merged.into_values().collect()
}

fn load_skill_entries_from_dir(dir: &Path, source: &str) -> Vec<SkillEntry> {
    let mut out: Vec<SkillEntry> = Vec::new();
    let Ok(read_dir) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in read_dir.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let file_path = path.join("SKILL.md");
        if !file_path.is_file() {
            continue;
        }
        let Ok(raw) = std::fs::read_to_string(&file_path) else {
            continue;
        };
        let frontmatter = parse_frontmatter_block(&raw);
        let name = frontmatter
            .get("name")
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .or_else(|| {
                path.file_name()
                    .and_then(|s| s.to_str())
                    .map(|s| s.to_string())
            })
            .unwrap_or_default();
        if name.trim().is_empty() {
            continue;
        }
        let description = frontmatter
            .get("description")
            .map(|s| s.trim().to_string())
            .unwrap_or_default();

        let skill = Skill {
            name: name.clone(),
            description,
            source: source.to_string(),
            file_path: file_path.clone(),
            base_dir: path.clone(),
        };
        let metadata = resolve_openclaw_metadata(&frontmatter);
        let invocation = resolve_invocation_policy(&frontmatter);
        out.push(SkillEntry {
            skill,
            frontmatter,
            metadata,
            invocation,
        });
    }
    out
}

// ---------------------------------------------------------------------------
// Frontmatter parsing (OpenClaw-compatible)
// ---------------------------------------------------------------------------

fn parse_frontmatter_block(content: &str) -> HashMap<String, String> {
    let normalized = content.replace("\r\n", "\n").replace('\r', "\n");
    if !normalized.starts_with("---") {
        return HashMap::new();
    }
    if !normalized.starts_with("---\n") {
        return HashMap::new();
    }
    let Some(end_index) = normalized[3..].find("\n---").map(|i| i + 3) else {
        return HashMap::new();
    };
    let block = &normalized[4..end_index];
    parse_line_frontmatter(block)
}

fn strip_frontmatter(content: &str) -> String {
    let normalized = content.replace("\r\n", "\n").replace('\r', "\n");
    if !normalized.starts_with("---\n") {
        return normalized;
    }
    let Some(end_index) = normalized[3..].find("\n---").map(|i| i + 3) else {
        return normalized;
    };
    let start = (end_index + 4).min(normalized.len());
    normalized[start..].to_string()
}

fn parse_line_frontmatter(block: &str) -> HashMap<String, String> {
    let mut frontmatter: HashMap<String, String> = HashMap::new();
    let lines: Vec<&str> = block.split('\n').collect();
    let mut i = 0usize;
    while i < lines.len() {
        let line = lines[i];
        let Some((key, rest)) = parse_frontmatter_kv_line(line) else {
            i += 1;
            continue;
        };
        if key.is_empty() {
            i += 1;
            continue;
        }
        let inline_value = rest.trim();
        if inline_value.is_empty() && i + 1 < lines.len() {
            let next = lines[i + 1];
            if next.starts_with(' ') || next.starts_with('\t') {
                let (value, consumed) = extract_multiline_value(&lines, i);
                if !value.is_empty() {
                    frontmatter.insert(key.to_string(), value);
                }
                i += consumed;
                continue;
            }
        }
        let value = strip_quotes(inline_value);
        if !value.is_empty() {
            frontmatter.insert(key.to_string(), value.to_string());
        }
        i += 1;
    }
    frontmatter
}

fn parse_frontmatter_kv_line(line: &str) -> Option<(&str, &str)> {
    let trimmed = line.trim_end();
    let colon = trimmed.find(':')?;
    let key = trimmed[..colon].trim();
    if key.is_empty() {
        return None;
    }
    // Match OpenClaw's `[\w-]+` - keep conservative.
    if !key
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return None;
    }
    Some((key, &trimmed[colon + 1..]))
}

fn extract_multiline_value(lines: &[&str], start_index: usize) -> (String, usize) {
    let mut value_lines: Vec<&str> = Vec::new();
    let mut i = start_index + 1;
    while i < lines.len() {
        let line = lines[i];
        if !line.is_empty() && !line.starts_with(' ') && !line.starts_with('\t') {
            break;
        }
        value_lines.push(line);
        i += 1;
    }
    let combined = value_lines.join("\n").trim().to_string();
    (combined, i - start_index)
}

fn strip_quotes(value: &str) -> &str {
    let v = value.trim();
    if (v.starts_with('"') && v.ends_with('"')) || (v.starts_with('\'') && v.ends_with('\'')) {
        return &v[1..v.len().saturating_sub(1)];
    }
    v
}

// ---------------------------------------------------------------------------
// Metadata parsing (OpenClaw-compatible)
// ---------------------------------------------------------------------------

fn parse_boolean_value(value: Option<&str>) -> Option<bool> {
    let Some(value) = value else {
        return None;
    };
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return None;
    }
    match normalized.as_str() {
        "true" | "1" | "yes" | "on" => Some(true),
        "false" | "0" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn resolve_invocation_policy(frontmatter: &HashMap<String, String>) -> SkillInvocationPolicy {
    let user_invocable =
        parse_boolean_value(frontmatter.get("user-invocable").map(|s| s.as_str())).unwrap_or(true);
    let disable_model_invocation = parse_boolean_value(
        frontmatter
            .get("disable-model-invocation")
            .map(|s| s.as_str()),
    )
    .unwrap_or(false);
    SkillInvocationPolicy {
        user_invocable,
        disable_model_invocation,
    }
}

fn resolve_openclaw_metadata(
    frontmatter: &HashMap<String, String>,
) -> Option<OpenclawSkillMetadata> {
    let raw = frontmatter.get("metadata")?;
    let parsed: serde_json::Value = json5::from_str(raw).ok()?;
    let obj = parsed.as_object()?;
    let openclaw = obj.get("openclaw")?.as_object()?;

    let requires = openclaw
        .get("requires")
        .and_then(|v| v.as_object())
        .map(|r| OpenclawSkillRequires {
            bins: normalize_string_list(r.get("bins")),
            any_bins: normalize_string_list(r.get("anyBins")),
            env: normalize_string_list(r.get("env")),
            config: normalize_string_list(r.get("config")),
        });

    let install = openclaw
        .get("install")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(parse_install_spec)
                .collect::<Vec<_>>()
        })
        .filter(|v| !v.is_empty());

    let os = normalize_string_list(openclaw.get("os"));
    let os = if os.is_empty() { None } else { Some(os) };

    Some(OpenclawSkillMetadata {
        always: openclaw.get("always").and_then(|v| v.as_bool()),
        emoji: openclaw
            .get("emoji")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        homepage: openclaw
            .get("homepage")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        skill_key: openclaw
            .get("skillKey")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        primary_env: openclaw
            .get("primaryEnv")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        os,
        requires,
        install,
    })
}

fn normalize_string_list(value: Option<&serde_json::Value>) -> Vec<String> {
    let Some(value) = value else {
        return Vec::new();
    };
    if let Some(arr) = value.as_array() {
        return arr
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.trim().to_string()))
            .filter(|s| !s.is_empty())
            .collect();
    }
    if let Some(s) = value.as_str() {
        return s
            .split(',')
            .map(|v| v.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
    }
    Vec::new()
}

fn parse_install_spec(value: &serde_json::Value) -> Option<SkillInstallSpec> {
    let obj = value.as_object()?;
    let kind_raw = obj
        .get("kind")
        .or_else(|| obj.get("type"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    let kind = match kind_raw.as_str() {
        "brew" => SkillInstallKind::Brew,
        "node" => SkillInstallKind::Node,
        "go" => SkillInstallKind::Go,
        "uv" => SkillInstallKind::Uv,
        "download" => SkillInstallKind::Download,
        _ => return None,
    };

    let os = normalize_string_list(obj.get("os"));
    let bins = normalize_string_list(obj.get("bins"));

    let strip_components = obj
        .get("stripComponents")
        .and_then(|v| v.as_f64())
        .map(|v| v.max(0.0).floor() as u64);

    Some(SkillInstallSpec {
        id: obj
            .get("id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        kind,
        label: obj
            .get("label")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        bins,
        os,
        formula: obj
            .get("formula")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        package: obj
            .get("package")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        module: obj
            .get("module")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        url: obj
            .get("url")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        archive: obj
            .get("archive")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        extract: obj.get("extract").and_then(|v| v.as_bool()),
        strip_components,
        target_dir: obj
            .get("targetDir")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
    })
}

fn resolve_install_id(spec: &SkillInstallSpec, index: usize) -> String {
    spec.id
        .as_deref()
        .unwrap_or(&format!("{}-{}", spec.kind.as_str(), index))
        .trim()
        .to_string()
}

fn find_install_spec(entry: &SkillEntry, install_id: &str) -> Option<(SkillInstallSpec, usize)> {
    let specs = entry
        .metadata
        .as_ref()
        .and_then(|m| m.install.as_ref())
        .cloned()
        .unwrap_or_default();
    for (index, spec) in specs.into_iter().enumerate() {
        if resolve_install_id(&spec, index) == install_id {
            return Some((spec, index));
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Installer implementation
// ---------------------------------------------------------------------------

fn summarize_install_output(text: &str) -> Option<String> {
    let raw = text.trim();
    if raw.is_empty() {
        return None;
    }
    let lines = raw
        .split('\n')
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    if lines.is_empty() {
        return None;
    }
    let preferred = lines
        .iter()
        .copied()
        .find(|line| line.to_ascii_lowercase().starts_with("error"))
        .or_else(|| {
            lines.iter().copied().find(|line| {
                let lc = line.to_ascii_lowercase();
                lc.contains("err!") || lc.contains("error:") || lc.contains("failed")
            })
        })
        .or_else(|| lines.last().copied());
    let preferred = preferred?;
    let normalized = preferred.split_whitespace().collect::<Vec<_>>().join(" ");
    let max_len = 200usize;
    if normalized.chars().count() > max_len {
        Some(normalized.chars().take(max_len - 3).collect::<String>() + "...")
    } else {
        Some(normalized)
    }
}

fn format_install_failure_message(code: Option<i64>, stdout: &str, stderr: &str) -> String {
    let code_str = code
        .map(|c| format!("exit {}", c))
        .unwrap_or_else(|| "unknown exit".to_string());
    let summary = summarize_install_output(stderr).or_else(|| summarize_install_output(stdout));
    match summary {
        Some(s) => format!("Install failed ({}): {}", code_str, s),
        None => format!("Install failed ({})", code_str),
    }
}

async fn install_command_spec(spec: &SkillInstallSpec, timeout_ms: u64) -> SkillInstallResult {
    let brew_exe = resolve_brew_executable();

    let (argv, env_overrides): (Vec<String>, HashMap<String, String>) = match spec.kind {
        SkillInstallKind::Brew => {
            let Some(formula) = spec
                .formula
                .as_deref()
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
            else {
                return SkillInstallResult {
                    ok: false,
                    message: "missing brew formula".to_string(),
                    stdout: String::new(),
                    stderr: String::new(),
                    code: None,
                };
            };
            let Some(brew) = brew_exe.clone() else {
                return SkillInstallResult {
                    ok: false,
                    message: "brew not installed".to_string(),
                    stdout: String::new(),
                    stderr: String::new(),
                    code: None,
                };
            };
            (
                vec![brew, "install".to_string(), formula.to_string()],
                HashMap::new(),
            )
        }
        SkillInstallKind::Node => {
            let Some(package) = spec
                .package
                .as_deref()
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
            else {
                return SkillInstallResult {
                    ok: false,
                    message: "missing node package".to_string(),
                    stdout: String::new(),
                    stderr: String::new(),
                    code: None,
                };
            };
            (build_node_install_command(package), HashMap::new())
        }
        SkillInstallKind::Go => {
            let Some(module) = spec
                .module
                .as_deref()
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
            else {
                return SkillInstallResult {
                    ok: false,
                    message: "missing go module".to_string(),
                    stdout: String::new(),
                    stderr: String::new(),
                    code: None,
                };
            };
            // Best-effort: match OpenClaw by placing binaries into brew's bin dir when possible.
            let mut env = HashMap::new();
            if let Some(brew) = brew_exe.as_deref() {
                if let Some(bin) = resolve_brew_bin_dir(timeout_ms.min(30_000), brew).await {
                    env.insert("GOBIN".to_string(), bin);
                }
            }
            (
                vec!["go".to_string(), "install".to_string(), module.to_string()],
                env,
            )
        }
        SkillInstallKind::Uv => {
            let Some(package) = spec
                .package
                .as_deref()
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
            else {
                return SkillInstallResult {
                    ok: false,
                    message: "missing uv package".to_string(),
                    stdout: String::new(),
                    stderr: String::new(),
                    code: None,
                };
            };
            (
                vec![
                    "uv".to_string(),
                    "tool".to_string(),
                    "install".to_string(),
                    package.to_string(),
                ],
                HashMap::new(),
            )
        }
        SkillInstallKind::Download => {
            return SkillInstallResult {
                ok: false,
                message: "download install handled separately".to_string(),
                stdout: String::new(),
                stderr: String::new(),
                code: None,
            }
        }
    };

    // Match OpenClaw behavior: auto-install uv/go via brew when possible.
    if spec.kind == SkillInstallKind::Uv && !has_binary("uv") {
        if let Some(brew) = brew_exe.clone() {
            let probe = run_command_with_timeout(
                &[brew, "install".to_string(), "uv".to_string()],
                timeout_ms,
                None,
            )
            .await;
            if probe.code != Some(0) {
                return SkillInstallResult {
                    ok: false,
                    message: "Failed to install uv (brew)".to_string(),
                    stdout: probe.stdout.trim().to_string(),
                    stderr: probe.stderr.trim().to_string(),
                    code: probe.code,
                };
            }
        } else {
            return SkillInstallResult {
                ok: false,
                message: "uv not installed (install via brew)".to_string(),
                stdout: String::new(),
                stderr: String::new(),
                code: None,
            };
        }
    }

    if spec.kind == SkillInstallKind::Go && !has_binary("go") {
        if let Some(brew) = brew_exe.clone() {
            let probe = run_command_with_timeout(
                &[brew, "install".to_string(), "go".to_string()],
                timeout_ms,
                None,
            )
            .await;
            if probe.code != Some(0) {
                return SkillInstallResult {
                    ok: false,
                    message: "Failed to install go (brew)".to_string(),
                    stdout: probe.stdout.trim().to_string(),
                    stderr: probe.stderr.trim().to_string(),
                    code: probe.code,
                };
            }
        } else {
            return SkillInstallResult {
                ok: false,
                message: "go not installed (install via brew)".to_string(),
                stdout: String::new(),
                stderr: String::new(),
                code: None,
            };
        }
    }

    // brew is already resolved via resolve_brew_executable().

    let result = run_command_with_timeout(&argv, timeout_ms, Some(&env_overrides)).await;
    let success = result.code == Some(0);
    SkillInstallResult {
        ok: success,
        message: if success {
            "Installed".to_string()
        } else {
            format_install_failure_message(result.code, &result.stdout, &result.stderr)
        },
        stdout: result.stdout.trim().to_string(),
        stderr: result.stderr.trim().to_string(),
        code: result.code,
    }
}

async fn install_download_spec(
    cfg: &Config,
    entry: &SkillEntry,
    spec: &SkillInstallSpec,
    timeout_ms: u64,
) -> SkillInstallResult {
    let url = spec.url.as_deref().unwrap_or("").trim();
    if url.is_empty() {
        return SkillInstallResult {
            ok: false,
            message: "missing download url".to_string(),
            stdout: String::new(),
            stderr: String::new(),
            code: None,
        };
    }

    let filename = url
        .split('/')
        .last()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .unwrap_or("download")
        .to_string();

    let target_dir = resolve_download_target_dir(cfg, entry, spec);
    if let Err(e) = tokio::fs::create_dir_all(&target_dir).await {
        let msg = e.to_string();
        return SkillInstallResult {
            ok: false,
            message: msg.clone(),
            stdout: String::new(),
            stderr: msg,
            code: None,
        };
    }

    let archive_path = target_dir.join(&filename);
    let download = download_file(url, &archive_path, timeout_ms).await;
    let downloaded = match download {
        Ok(bytes) => bytes,
        Err(message) => {
            return SkillInstallResult {
                ok: false,
                message: message.clone(),
                stdout: String::new(),
                stderr: message,
                code: None,
            }
        }
    };

    let archive_type = resolve_archive_type(spec, &filename);
    let should_extract = spec.extract.unwrap_or(archive_type.is_some());
    if !should_extract {
        return SkillInstallResult {
            ok: true,
            message: format!("Downloaded to {}", archive_path.to_string_lossy()),
            stdout: format!("downloaded={}", downloaded),
            stderr: String::new(),
            code: Some(0),
        };
    }

    let Some(archive_type) = archive_type else {
        return SkillInstallResult {
            ok: false,
            message: "extract requested but archive type could not be detected".to_string(),
            stdout: String::new(),
            stderr: String::new(),
            code: None,
        };
    };

    let extract = extract_archive(
        &archive_path,
        &archive_type,
        &target_dir,
        spec.strip_components,
        timeout_ms,
    )
    .await;
    let success = extract.code == Some(0);
    SkillInstallResult {
        ok: success,
        message: if success {
            format!(
                "Downloaded and extracted to {}",
                target_dir.to_string_lossy()
            )
        } else {
            format_install_failure_message(extract.code, &extract.stdout, &extract.stderr)
        },
        stdout: extract.stdout.trim().to_string(),
        stderr: extract.stderr.trim().to_string(),
        code: extract.code,
    }
}

fn resolve_download_target_dir(cfg: &Config, entry: &SkillEntry, spec: &SkillInstallSpec) -> PathBuf {
    if let Some(target) = spec
        .target_dir
        .as_deref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        return crate::openclaw_paths::resolve_user_path(target);
    }
    let state_dir = crate::openclaw_paths::resolve_openclaw_state_dir(cfg)
        .or_else(|| Config::config_dir())
        .unwrap_or_else(|| PathBuf::from("."));
    let key = resolve_skill_key(entry);
    state_dir.join("tools").join(key)
}

fn resolve_archive_type(spec: &SkillInstallSpec, filename: &str) -> Option<String> {
    if let Some(explicit) = spec
        .archive
        .as_deref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        return Some(explicit.to_ascii_lowercase());
    }
    let lower = filename.to_ascii_lowercase();
    if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
        return Some("tar.gz".to_string());
    }
    if lower.ends_with(".tar.bz2") || lower.ends_with(".tbz2") {
        return Some("tar.bz2".to_string());
    }
    if lower.ends_with(".zip") {
        return Some("zip".to_string());
    }
    None
}

async fn download_file(url: &str, dest_path: &Path, timeout_ms: u64) -> Result<u64, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(timeout_ms))
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client.get(url).send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!(
            "Download failed ({} {})",
            resp.status().as_u16(),
            resp.status().canonical_reason().unwrap_or("unknown status")
        ));
    }
    let mut file = tokio::fs::File::create(dest_path)
        .await
        .map_err(|e| e.to_string())?;
    let mut written: u64 = 0;
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| e.to_string())?;
        file.write_all(&chunk).await.map_err(|e| e.to_string())?;
        written = written.saturating_add(chunk.len() as u64);
    }
    let _ = file.flush().await;
    Ok(written)
}

#[derive(Debug, Clone)]
struct CommandOutput {
    code: Option<i64>,
    stdout: String,
    stderr: String,
}

async fn run_command_with_timeout(
    argv: &[String],
    timeout_ms: u64,
    env: Option<&HashMap<String, String>>,
) -> CommandOutput {
    if argv.is_empty() {
        return CommandOutput {
            code: None,
            stdout: String::new(),
            stderr: "invalid install command".to_string(),
        };
    }
    let mut cmd = tokio::process::Command::new(&argv[0]);
    cmd.args(&argv[1..]);
    cmd.stdin(std::process::Stdio::null());
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());
    if let Some(env) = env {
        for (k, v) in env {
            cmd.env(k, v);
        }
    }
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            return CommandOutput {
                code: None,
                stdout: String::new(),
                stderr: e.to_string(),
            }
        }
    };

    // `wait_with_output()` consumes the child; we want to be able to `kill()` on timeout.
    // Read stdout/stderr concurrently so we don't deadlock on full pipes.
    let stdout_task = child.stdout.take().map(|mut stdout| {
        tokio::spawn(async move {
            use tokio::io::AsyncReadExt;
            let mut buf = Vec::new();
            let _ = stdout.read_to_end(&mut buf).await;
            buf
        })
    });
    let stderr_task = child.stderr.take().map(|mut stderr| {
        tokio::spawn(async move {
            use tokio::io::AsyncReadExt;
            let mut buf = Vec::new();
            let _ = stderr.read_to_end(&mut buf).await;
            buf
        })
    });

    let status = match tokio::time::timeout(Duration::from_millis(timeout_ms), child.wait()).await {
        Ok(Ok(status)) => Some(status),
        Ok(Err(_)) => None,
        Err(_) => {
            let _ = child.kill().await;
            None
        }
    };

    let read_join = |task: Option<tokio::task::JoinHandle<Vec<u8>>>| async move {
        let Some(task) = task else {
            return Vec::new();
        };
        match tokio::time::timeout(Duration::from_millis(1_000), task).await {
            Ok(Ok(buf)) => buf,
            Ok(Err(_)) => Vec::new(),
            Err(_) => Vec::new(),
        }
    };

    let stdout_bytes = read_join(stdout_task).await;
    let stderr_bytes = read_join(stderr_task).await;

    CommandOutput {
        code: status.and_then(|s| s.code()).map(|c| c as i64),
        stdout: String::from_utf8_lossy(&stdout_bytes).to_string(),
        stderr: if status.is_none() {
            let fallback = String::from_utf8_lossy(&stderr_bytes).to_string();
            if fallback.trim().is_empty() {
                "timed out".to_string()
            } else {
                fallback
            }
        } else {
            String::from_utf8_lossy(&stderr_bytes).to_string()
        },
    }
}

async fn extract_archive(
    archive_path: &Path,
    archive_type: &str,
    target_dir: &Path,
    strip_components: Option<u64>,
    timeout_ms: u64,
) -> CommandOutput {
    if archive_type == "zip" {
        if !has_binary("unzip") {
            return CommandOutput {
                code: None,
                stdout: String::new(),
                stderr: "unzip not found on PATH".to_string(),
            };
        }
        let argv = vec![
            "unzip".to_string(),
            "-q".to_string(),
            archive_path.to_string_lossy().to_string(),
            "-d".to_string(),
            target_dir.to_string_lossy().to_string(),
        ];
        return run_command_with_timeout(&argv, timeout_ms, None).await;
    }
    if !has_binary("tar") {
        return CommandOutput {
            code: None,
            stdout: String::new(),
            stderr: "tar not found on PATH".to_string(),
        };
    }
    let mut argv = vec![
        "tar".to_string(),
        "xf".to_string(),
        archive_path.to_string_lossy().to_string(),
        "-C".to_string(),
        target_dir.to_string_lossy().to_string(),
    ];
    if let Some(n) = strip_components {
        argv.push("--strip-components".to_string());
        argv.push(n.to_string());
    }
    run_command_with_timeout(&argv, timeout_ms, None).await
}

async fn resolve_brew_bin_dir(timeout_ms: u64, brew_exe: &str) -> Option<String> {
    let argv = vec![brew_exe.to_string(), "--prefix".to_string()];
    let out = run_command_with_timeout(&argv, timeout_ms, None).await;
    if out.code == Some(0) {
        let prefix = out.stdout.trim();
        if !prefix.is_empty() {
            return Some(
                PathBuf::from(prefix)
                    .join("bin")
                    .to_string_lossy()
                    .to_string(),
            );
        }
    }

    if let Ok(prefix) = std::env::var("HOMEBREW_PREFIX") {
        let trimmed = prefix.trim();
        if !trimmed.is_empty() {
            return Some(
                PathBuf::from(trimmed)
                    .join("bin")
                    .to_string_lossy()
                    .to_string(),
            );
        }
    }

    for candidate in ["/opt/homebrew/bin", "/usr/local/bin"] {
        if Path::new(candidate).exists() {
            return Some(candidate.to_string());
        }
    }
    None
}

fn resolve_brew_executable() -> Option<String> {
    if has_binary("brew") {
        return Some("brew".to_string());
    }
    for candidate in ["/opt/homebrew/bin/brew", "/usr/local/bin/brew"] {
        if Path::new(candidate).exists() {
            return Some(candidate.to_string());
        }
    }
    None
}

fn resolve_runtime_platform() -> &'static str {
    match std::env::consts::OS {
        "macos" => "darwin",
        "windows" => "win32",
        other => other,
    }
}

fn has_binary(bin: &str) -> bool {
    let path_env = std::env::var_os("PATH").unwrap_or_default();
    for part in std::env::split_paths(&path_env) {
        let candidate = part.join(bin);
        if is_executable(&candidate) {
            return true;
        }
        // Windows PATHEXT / common extensions (best-effort).
        if cfg!(windows) {
            for ext in [".exe", ".cmd", ".bat", ".com"] {
                let candidate = part.join(format!("{}{}", bin, ext));
                if candidate.exists() {
                    return true;
                }
            }
        }
    }
    false
}

fn is_executable(path: &Path) -> bool {
    if !path.exists() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(path) {
            return meta.permissions().mode() & 0o111 != 0;
        }
    }
    #[cfg(not(unix))]
    {
        // Best-effort: existence implies runnable for our purposes.
        return true;
    }
    false
}

fn resolve_config_path<'a>(
    config: &'a serde_json::Value,
    path_str: &str,
) -> Option<&'a serde_json::Value> {
    let mut current = config;
    for part in path_str.split('.').filter(|s| !s.is_empty()) {
        let next = current.get(part)?;
        current = next;
    }
    Some(current)
}

fn is_truthy(value: Option<&serde_json::Value>) -> bool {
    let Some(value) = value else {
        return false;
    };
    match value {
        serde_json::Value::Null => false,
        serde_json::Value::Bool(b) => *b,
        serde_json::Value::Number(n) => n.as_f64().unwrap_or(0.0) != 0.0,
        serde_json::Value::String(s) => !s.trim().is_empty(),
        _ => true,
    }
}

fn is_config_path_truthy(config: &serde_json::Value, path_str: &str) -> bool {
    let value = resolve_config_path(config, path_str);
    if value.is_none() {
        for (key, default_value) in DEFAULT_CONFIG_VALUES {
            if *key == path_str {
                return *default_value;
            }
        }
    }
    is_truthy(value)
}

fn write_json_atomic<T: Serialize>(path: &PathBuf, value: &T) -> Result<(), ErrorShape> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            ErrorShape::new(
                error_codes::UNAVAILABLE,
                format!("failed to create dir {}: {}", parent.to_string_lossy(), e),
            )
        })?;
    }
    let raw = serde_json::to_string_pretty(value).map_err(|e| {
        ErrorShape::new(
            error_codes::UNAVAILABLE,
            format!("failed to serialize json: {}", e),
        )
    })?;
    let tmp = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    std::fs::write(&tmp, raw).map_err(|e| {
        ErrorShape::new(
            error_codes::UNAVAILABLE,
            format!("failed to write {}: {}", tmp.to_string_lossy(), e),
        )
    })?;
    std::fs::rename(&tmp, path).map_err(|e| {
        ErrorShape::new(
            error_codes::UNAVAILABLE,
            format!(
                "failed to move {} -> {}: {}",
                tmp.to_string_lossy(),
                path.to_string_lossy(),
                e
            ),
        )
    })?;
    Ok(())
}
