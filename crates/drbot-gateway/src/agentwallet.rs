//! AgentWallet integration helpers (OpenClaw skill + heartbeat).
//!
//! This module provides:
//! - Best-effort sync of AgentWallet `skill.md` + `heartbeat.md` into the local
//!   OpenClaw managed skills directory (so the skill can be consumed as local
//!   `SKILL.md` / `HEARTBEAT.md` files).
//! - Best-effort prefetch of the public "network pulse" endpoint for heartbeats.

use crate::openclaw_skills::{load_skills_config_file, resolve_managed_skills_dir};
use drbot_core::Config;
use reqwest::header::{ETAG, IF_MODIFIED_SINCE, IF_NONE_MATCH, LAST_MODIFIED};
use ring::digest;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tracing::{debug, warn};
use uuid::Uuid;

pub const AGENTWALLET_SKILL_KEY: &str = "agentwallet";
pub const AGENTWALLET_SKILL_URL: &str = "https://agentwallet.mcpay.tech/skill.md";
pub const AGENTWALLET_HEARTBEAT_URL: &str = "https://agentwallet.mcpay.tech/heartbeat.md";
pub const AGENTWALLET_SKILL_JSON_URL: &str = "https://agentwallet.mcpay.tech/skill.json";

const AGENTWALLET_NETWORK_PULSE_URL: &str = "https://agentwallet.mcpay.tech/api/network/pulse";
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

fn resolve_agentwallet_skill_dir() -> PathBuf {
    resolve_managed_skills_dir().join(AGENTWALLET_SKILL_KEY)
}

fn resolve_agentwallet_skill_path() -> PathBuf {
    resolve_agentwallet_skill_dir().join("SKILL.md")
}

fn resolve_agentwallet_heartbeat_path() -> PathBuf {
    resolve_agentwallet_skill_dir().join("HEARTBEAT.md")
}

fn resolve_agentwallet_package_json_path() -> PathBuf {
    resolve_agentwallet_skill_dir().join("package.json")
}

fn resolve_meta_path(path: &Path) -> PathBuf {
    PathBuf::from(format!("{}.meta.json", path.to_string_lossy()))
}

fn load_meta(path: &Path) -> Option<RemoteFileMeta> {
    read_json_file(&resolve_meta_path(path))
}

fn should_attempt_sync(path: &Path) -> bool {
    if std::env::var("DRBOT_OPENCLAW_AGENTWALLET_SYNC")
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
    if file.entries.contains_key(AGENTWALLET_SKILL_KEY) {
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

fn agentwallet_skill_enabled() -> bool {
    let file = load_skills_config_file();
    let entry = file.entries.get(AGENTWALLET_SKILL_KEY);
    entry.and_then(|e| e.enabled).unwrap_or(true)
}

pub async fn sync_agentwallet_docs_best_effort(_cfg: &Config) {
    if !agentwallet_skill_enabled() {
        return;
    }

    let skill_path = resolve_agentwallet_skill_path();
    if !should_attempt_sync(&skill_path) {
        return;
    }

    let ua = format!(
        "drbot/{} (+openclaw-agentwallet-sync)",
        env!("CARGO_PKG_VERSION")
    );
    let timeout_secs = std::env::var("DRBOT_OPENCLAW_AGENTWALLET_SYNC_TIMEOUT_SECS")
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
            warn!(error = %err, "agentwallet: failed to build http client");
            return;
        }
    };

    let min_interval_ms = std::env::var("DRBOT_OPENCLAW_AGENTWALLET_SYNC_MIN_INTERVAL_MS")
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .filter(|v| *v >= 1_000)
        .unwrap_or(DEFAULT_SYNC_MIN_INTERVAL_MS);

    let heartbeat_path = resolve_agentwallet_heartbeat_path();
    let package_json_path = resolve_agentwallet_package_json_path();

    match sync_remote_text(&client, AGENTWALLET_SKILL_URL, &skill_path, min_interval_ms).await {
        Ok(updated) => debug!(
            updated,
            path = %skill_path.to_string_lossy(),
            "agentwallet: skill.md sync"
        ),
        Err(err) => warn!(error = %err, "agentwallet: skill.md sync failed"),
    }

    match sync_remote_text(
        &client,
        AGENTWALLET_HEARTBEAT_URL,
        &heartbeat_path,
        min_interval_ms,
    )
    .await
    {
        Ok(updated) => debug!(
            updated,
            path = %heartbeat_path.to_string_lossy(),
            "agentwallet: heartbeat.md sync"
        ),
        Err(err) => warn!(error = %err, "agentwallet: heartbeat.md sync failed"),
    }

    match sync_remote_text(
        &client,
        AGENTWALLET_SKILL_JSON_URL,
        &package_json_path,
        min_interval_ms,
    )
    .await
    {
        Ok(updated) => debug!(
            updated,
            path = %package_json_path.to_string_lossy(),
            "agentwallet: skill.json sync"
        ),
        Err(err) => warn!(error = %err, "agentwallet: skill.json sync failed"),
    }
}

pub async fn fetch_agentwallet_heartbeat_context() -> Option<serde_json::Value> {
    if !agentwallet_skill_enabled() {
        return None;
    }

    // Don't fetch anything unless the operator opted into the skill (or it exists locally).
    let skill_path = resolve_agentwallet_skill_path();
    if !should_attempt_sync(&skill_path) {
        return None;
    }

    let ua = format!(
        "drbot/{} (+openclaw-agentwallet-heartbeat)",
        env!("CARGO_PKG_VERSION")
    );
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(ua)
        .build()
        .ok()?;

    let pulse: Option<serde_json::Value> =
        match client.get(AGENTWALLET_NETWORK_PULSE_URL).send().await {
            Ok(r) => r.json::<serde_json::Value>().await.ok(),
            Err(_) => None,
        };

    Some(json!({
        "ts": now_ms(),
        "networkPulse": pulse,
    }))
}
