//! Moltbook integration helpers (OpenClaw skill + heartbeat).
//!
//! This module provides:
//! - Best-effort sync of Moltbook `skill.md` + `heartbeat.md` (and optional docs)
//!   into the local OpenClaw managed skills directory (so the skill can be
//!   consumed as local `SKILL.md` / `HEARTBEAT.md` files).
//! - Best-effort prefetch of Moltbook `GET /agents/status` etc for heartbeats.

use crate::openclaw_skills::{load_skills_config_file, resolve_managed_skills_dir};
use drbot_core::Config;
use drbot_protocol::openclaw::{error_codes, ErrorShape};
use reqwest::header::{ETAG, IF_MODIFIED_SINCE, IF_NONE_MATCH, LAST_MODIFIED};
use ring::digest;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tracing::{debug, warn};
use uuid::Uuid;

pub const MOLTBOOK_SKILL_KEY: &str = "moltbook";
pub const MOLTBOOK_SKILL_URL: &str = "https://www.moltbook.com/skill.md";
pub const MOLTBOOK_HEARTBEAT_URL: &str = "https://www.moltbook.com/heartbeat.md";
pub const MOLTBOOK_MESSAGING_URL: &str = "https://www.moltbook.com/messaging.md";
pub const MOLTBOOK_SKILL_JSON_URL: &str = "https://www.moltbook.com/skill.json";

const MOLTBOOK_API_BASE: &str = "https://www.moltbook.com/api/v1";
const DEFAULT_SYNC_MIN_INTERVAL_MS: u64 = 10 * 60 * 1000;

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

fn now_ms() -> u64 {
    chrono::Utc::now()
        .timestamp_millis()
        .try_into()
        .unwrap_or(0)
}

fn sha256_hex(raw: &str) -> String {
    let d = digest::digest(&digest::SHA256, raw.as_bytes());
    drbot_hex_util::encode(d.as_ref())
}

fn read_json_file<T: for<'de> Deserialize<'de>>(path: &Path) -> Option<T> {
    let raw = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&raw).ok()
}

fn write_text_atomic(path: &Path, content: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension(format!("{}.tmp", Uuid::new_v4()));
    std::fs::write(&tmp, content)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> std::io::Result<()> {
    let raw = serde_json::to_string_pretty(value).unwrap_or_else(|_| "{}".to_string());
    write_text_atomic(path, &raw)
}

fn resolve_moltbook_skill_dir() -> PathBuf {
    resolve_managed_skills_dir().join(MOLTBOOK_SKILL_KEY)
}

fn resolve_moltbook_skill_path() -> PathBuf {
    resolve_moltbook_skill_dir().join("SKILL.md")
}

fn resolve_moltbook_heartbeat_path() -> PathBuf {
    resolve_moltbook_skill_dir().join("HEARTBEAT.md")
}

fn resolve_moltbook_messaging_path() -> PathBuf {
    resolve_moltbook_skill_dir().join("MESSAGING.md")
}

fn resolve_moltbook_package_json_path() -> PathBuf {
    resolve_moltbook_skill_dir().join("package.json")
}

fn resolve_meta_path(path: &Path) -> PathBuf {
    PathBuf::from(format!("{}.meta.json", path.to_string_lossy()))
}

fn load_meta(path: &Path) -> Option<RemoteFileMeta> {
    read_json_file(&resolve_meta_path(path))
}

fn should_attempt_sync(path: &Path) -> bool {
    if std::env::var("DRBOT_OPENCLAW_MOLTBOOK_SYNC")
        .ok()
        .as_deref()
        == Some("1")
    {
        return true;
    }

    if path.exists() {
        return true;
    }

    // Intent signal: operator set a skill config entry (enabled/apiKey/env).
    let file = load_skills_config_file();
    if file.entries.contains_key(MOLTBOOK_SKILL_KEY) {
        return true;
    }

    false
}

async fn fetch_remote_text(
    client: &reqwest::Client,
    url: &str,
    meta: Option<&RemoteFileMeta>,
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
    let body = res.text().await.map_err(|e| e.to_string())?;
    Ok((etag, last_modified, status, Some(body)))
}

async fn sync_remote_text(
    client: &reqwest::Client,
    url: &str,
    path: &Path,
    min_interval_ms: u64,
) -> Result<bool, String> {
    let meta = load_meta(path);
    let prev_etag = meta.as_ref().and_then(|m| m.etag.clone());
    let prev_last_modified = meta.as_ref().and_then(|m| m.last_modified.clone());

    if let Some(meta) = meta.as_ref() {
        let elapsed = now_ms().saturating_sub(meta.fetched_at_ms);
        if elapsed < min_interval_ms {
            return Ok(false);
        }
    }

    let (etag, last_modified, status, maybe_body) =
        fetch_remote_text(client, url, meta.as_ref()).await?;
    let fetched_at_ms = now_ms();

    if status == 304 {
        if let Some(mut meta) = meta {
            meta.fetched_at_ms = fetched_at_ms;
            meta.etag = etag.or(meta.etag);
            meta.last_modified = last_modified.or(meta.last_modified);
            let _ = write_json_atomic(&resolve_meta_path(path), &meta);
        } else {
            let meta = RemoteFileMeta {
                url: url.to_string(),
                fetched_at_ms,
                etag,
                last_modified,
                sha256: None,
            };
            let _ = write_json_atomic(&resolve_meta_path(path), &meta);
        }
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
        let meta = RemoteFileMeta {
            url: url.to_string(),
            fetched_at_ms,
            etag: etag.or(prev_etag),
            last_modified: last_modified.or(prev_last_modified),
            sha256: Some(next_sha),
        };
        let _ = write_json_atomic(&resolve_meta_path(path), &meta);
        return Ok(false);
    }

    write_text_atomic(path, &body).map_err(|e| e.to_string())?;
    let meta = RemoteFileMeta {
        url: url.to_string(),
        fetched_at_ms,
        etag,
        last_modified,
        sha256: Some(next_sha),
    };
    let _ = write_json_atomic(&resolve_meta_path(path), &meta);
    Ok(true)
}

fn moltbook_skill_enabled() -> bool {
    let file = load_skills_config_file();
    let entry = file.entries.get(MOLTBOOK_SKILL_KEY);
    entry.and_then(|e| e.enabled).unwrap_or(true)
}

fn moltbook_api_key() -> Option<String> {
    let file = load_skills_config_file();
    let entry = file.entries.get(MOLTBOOK_SKILL_KEY)?;
    if entry.enabled == Some(false) {
        return None;
    }
    entry.api_key.clone().or_else(|| {
        std::env::var("MOLTBOOK_API_KEY").ok().and_then(|s| {
            let trimmed = s.trim().to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        })
    })
}

pub async fn moltbook_request(
    method: &str,
    path: &str,
    query: Option<&serde_json::Value>,
    body: Option<&serde_json::Value>,
    timeout_ms: Option<u64>,
    dry_run: bool,
    allow_write: bool,
) -> Result<serde_json::Value, ErrorShape> {
    if !moltbook_skill_enabled() {
        return Err(ErrorShape::new(
            error_codes::UNAVAILABLE,
            "moltbook skill disabled",
        ));
    }
    let api_key = moltbook_api_key().ok_or_else(|| {
        ErrorShape::new(
            error_codes::NOT_LINKED,
            "moltbook apiKey not configured (use skills.update for moltbook or set MOLTBOOK_API_KEY)",
        )
    })?;

    let method = method.trim().to_uppercase();
    let path = path.trim();
    if path.is_empty() {
        return Err(ErrorShape::new(
            error_codes::INVALID_REQUEST,
            "path is required",
        ));
    }
    if path.starts_with("http://") || path.starts_with("https://") {
        return Err(ErrorShape::new(
            error_codes::INVALID_REQUEST,
            "path must be relative (do not pass a full URL)",
        ));
    }

    let allow_write = allow_write
        || std::env::var("DRBOT_OPENCLAW_MOLTBOOK_WRITE")
        .ok()
        .as_deref()
        == Some("1");
    let is_write = matches!(method.as_str(), "POST" | "PUT" | "PATCH" | "DELETE");
    if is_write && !dry_run && !allow_write {
        return Err(ErrorShape::new(
            error_codes::UNAVAILABLE,
            "moltbook write requests disabled (set DRBOT_OPENCLAW_MOLTBOOK_WRITE=1)",
        ));
    }

    let mut url = MOLTBOOK_API_BASE.trim_end_matches('/').to_string();
    if path.starts_with('/') {
        url.push_str(path);
    } else {
        url.push('/');
        url.push_str(path);
    }

    if dry_run {
        return Ok(json!({
            "ok": true,
            "dryRun": true,
            "method": method,
            "url": url,
            "query": query.cloned().unwrap_or(serde_json::Value::Null),
            "body": body.cloned().unwrap_or(serde_json::Value::Null),
        }));
    }

    let timeout = timeout_ms
        .filter(|v| *v >= 1)
        .map(Duration::from_millis)
        .unwrap_or_else(|| Duration::from_secs(20));
    let ua = format!(
        "drbot/{} (+openclaw-moltbook-request)",
        env!("CARGO_PKG_VERSION")
    );
    let client = reqwest::Client::builder()
        .timeout(timeout)
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(ua)
        .build()
        .map_err(|e| ErrorShape::new(error_codes::UNAVAILABLE, e.to_string()))?;

    let req_method = match method.as_str() {
        "GET" => reqwest::Method::GET,
        "POST" => reqwest::Method::POST,
        "PUT" => reqwest::Method::PUT,
        "PATCH" => reqwest::Method::PATCH,
        "DELETE" => reqwest::Method::DELETE,
        _ => {
            return Err(ErrorShape::new(
                error_codes::INVALID_REQUEST,
                format!("unsupported method: {}", method),
            ))
        }
    };

    let mut req = client
        .request(req_method, &url)
        .header("Authorization", format!("Bearer {}", api_key));

    if let Some(q) = query.and_then(|v| v.as_object()) {
        let mut pairs: Vec<(String, String)> = Vec::new();
        for (k, v) in q {
            let key = k.trim();
            if key.is_empty() {
                continue;
            }
            match v {
                serde_json::Value::Null => {}
                serde_json::Value::Bool(b) => pairs.push((key.to_string(), b.to_string())),
                serde_json::Value::Number(n) => pairs.push((key.to_string(), n.to_string())),
                serde_json::Value::String(s) => pairs.push((key.to_string(), s.clone())),
                serde_json::Value::Array(arr) => {
                    for item in arr {
                        if let Some(s) = item.as_str() {
                            pairs.push((key.to_string(), s.to_string()));
                        }
                    }
                }
                serde_json::Value::Object(_) => {}
            }
        }
        if !pairs.is_empty() {
            req = req.query(&pairs);
        }
    }

    if !matches!(method.as_str(), "GET" | "DELETE") {
        if let Some(body) = body {
            if !body.is_null() {
                if let Some(raw) = body.as_str() {
                    req = req
                        .header("Content-Type", "application/json")
                        .body(raw.to_string());
                } else {
                    req = req.json(body);
                }
            }
        }
    }

    let res = req
        .send()
        .await
        .map_err(|e| ErrorShape::new(error_codes::UNAVAILABLE, e.to_string()))?;
    let status = res.status().as_u16();
    let text = res
        .text()
        .await
        .map_err(|e| ErrorShape::new(error_codes::UNAVAILABLE, e.to_string()))?;
    let json_body = serde_json::from_str::<serde_json::Value>(&text).ok();

    Ok(json!({
        "ok": status >= 200 && status < 300,
        "status": status,
        "json": json_body,
        "text": if json_body.is_some() { serde_json::Value::Null } else { json!(text) },
    }))
}

pub async fn sync_moltbook_docs_best_effort(_cfg: &Config) {
    if !moltbook_skill_enabled() {
        return;
    }

    let skill_path = resolve_moltbook_skill_path();
    if !should_attempt_sync(&skill_path) {
        return;
    }

    let ua = format!(
        "drbot/{} (+openclaw-moltbook-sync)",
        env!("CARGO_PKG_VERSION")
    );
    let timeout_secs = std::env::var("DRBOT_OPENCLAW_MOLTBOOK_SYNC_TIMEOUT_SECS")
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
            warn!(error = %err, "moltbook: failed to build http client");
            return;
        }
    };

    let min_interval_ms = std::env::var("DRBOT_OPENCLAW_MOLTBOOK_SYNC_MIN_INTERVAL_MS")
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .filter(|v| *v >= 1_000)
        .unwrap_or(DEFAULT_SYNC_MIN_INTERVAL_MS);

    let heartbeat_path = resolve_moltbook_heartbeat_path();
    let messaging_path = resolve_moltbook_messaging_path();
    let package_json_path = resolve_moltbook_package_json_path();

    match sync_remote_text(&client, MOLTBOOK_SKILL_URL, &skill_path, min_interval_ms).await {
        Ok(updated) => debug!(
            updated,
            path = %skill_path.to_string_lossy(),
            "moltbook: skill.md sync"
        ),
        Err(err) => warn!(error = %err, "moltbook: skill.md sync failed"),
    }

    match sync_remote_text(
        &client,
        MOLTBOOK_HEARTBEAT_URL,
        &heartbeat_path,
        min_interval_ms,
    )
    .await
    {
        Ok(updated) => debug!(
            updated,
            path = %heartbeat_path.to_string_lossy(),
            "moltbook: heartbeat.md sync"
        ),
        Err(err) => warn!(error = %err, "moltbook: heartbeat.md sync failed"),
    }

    // Optional docs: best-effort.
    match sync_remote_text(
        &client,
        MOLTBOOK_MESSAGING_URL,
        &messaging_path,
        min_interval_ms,
    )
    .await
    {
        Ok(updated) => debug!(
            updated,
            path = %messaging_path.to_string_lossy(),
            "moltbook: messaging.md sync"
        ),
        Err(err) => warn!(error = %err, "moltbook: messaging.md sync failed"),
    }

    match sync_remote_text(
        &client,
        MOLTBOOK_SKILL_JSON_URL,
        &package_json_path,
        min_interval_ms,
    )
    .await
    {
        Ok(updated) => debug!(
            updated,
            path = %package_json_path.to_string_lossy(),
            "moltbook: skill.json sync"
        ),
        Err(err) => warn!(error = %err, "moltbook: skill.json sync failed"),
    }
}

fn trim_array(value: &serde_json::Value, max_len: usize) -> serde_json::Value {
    let Some(arr) = value.as_array() else {
        return value.clone();
    };
    let trimmed = arr.iter().take(max_len).cloned().collect::<Vec<_>>();
    serde_json::Value::Array(trimmed)
}

fn pick_fields(obj: &serde_json::Value, fields: &[&str]) -> serde_json::Value {
    let Some(map) = obj.as_object() else {
        return obj.clone();
    };
    let mut out = serde_json::Map::new();
    for k in fields {
        if let Some(v) = map.get(*k) {
            out.insert((*k).to_string(), v.clone());
        }
    }
    serde_json::Value::Object(out)
}

fn pick_post_preview(v: &serde_json::Value) -> serde_json::Value {
    // Keep this very conservative (unknown schema): common id/title-ish fields.
    pick_fields(
        v,
        &[
            "id",
            "postId",
            "title",
            "submolt",
            "author",
            "agent",
            "createdAt",
            "created_at",
            "ts",
            "upvotes",
            "score",
            "commentCount",
            "replyCount",
            "url",
            "permalink",
        ],
    )
}

fn trim_posts(value: &serde_json::Value, max_len: usize) -> serde_json::Value {
    if let Some(posts) = value.get("posts") {
        let trimmed = posts
            .as_array()
            .map(|arr| {
                arr.iter()
                    .take(max_len)
                    .map(pick_post_preview)
                    .collect::<Vec<_>>()
            })
            .map(serde_json::Value::Array)
            .unwrap_or_else(|| trim_array(posts, max_len));
        return json!({ "posts": trimmed });
    }
    if let Some(arr) = value.as_array() {
        return serde_json::Value::Array(
            arr.iter()
                .take(max_len)
                .map(pick_post_preview)
                .collect::<Vec<_>>(),
        );
    }
    value.clone()
}

pub async fn fetch_moltbook_heartbeat_context() -> Option<serde_json::Value> {
    if !moltbook_skill_enabled() {
        return None;
    }
    let api_key = moltbook_api_key()?;

    let ua = format!(
        "drbot/{} (+openclaw-moltbook-heartbeat)",
        env!("CARGO_PKG_VERSION")
    );
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(ua)
        .build()
        .ok()?;

    let auth = ("Authorization", format!("Bearer {}", api_key));

    // 1) Agent claim/status.
    let status_url = format!("{}/agents/status", MOLTBOOK_API_BASE);
    let agent_status: Option<serde_json::Value> = match client
        .get(status_url)
        .header(auth.0, auth.1.clone())
        .send()
        .await
    {
        Ok(r) => r.json::<serde_json::Value>().await.ok(),
        Err(_) => None,
    };

    // 2) DM check summary (unread / pending requests).
    let dm_url = format!("{}/agents/dm/check", MOLTBOOK_API_BASE);
    let dm_check: Option<serde_json::Value> = match client
        .get(dm_url)
        .header(auth.0, auth.1.clone())
        .send()
        .await
    {
        Ok(r) => r.json::<serde_json::Value>().await.ok(),
        Err(_) => None,
    };

    // 3) Feed (trimmed).
    let feed_url = format!("{}/feed?sort=new&limit=15", MOLTBOOK_API_BASE);
    let feed: Option<serde_json::Value> = match client
        .get(feed_url)
        .header(auth.0, auth.1.clone())
        .send()
        .await
    {
        Ok(r) => r.json::<serde_json::Value>().await.ok(),
        Err(_) => None,
    };

    let agent_status =
        agent_status.map(|v| pick_fields(&v, &["status", "agent", "claimUrl", "claim_url"]));
    let dm_check = dm_check.map(|v| {
        pick_fields(
            &v,
            &[
                "pendingRequests",
                "unreadMessages",
                "needsHumanInput",
                "status",
            ],
        )
    });
    let feed = feed.map(|v| trim_posts(&v, 10));

    Some(json!({
        "ts": now_ms(),
        "agentStatus": agent_status,
        "dmCheck": dm_check,
        "feedNew": feed,
    }))
}
