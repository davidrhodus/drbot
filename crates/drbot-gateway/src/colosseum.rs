//! Colosseum Agent Hackathon integration helpers.
//!
//! This module provides:
//! - Best-effort sync of Colosseum `skill.md` + `heartbeat.md` into the local
//!   OpenClaw skills directory (so OpenClaw-style skills can be consumed as
//!   local `SKILL.md` / `HEARTBEAT.md` files).
//! - Best-effort prefetch of Colosseum `GET /agents/status` etc for heartbeats.

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

pub const COLOSSEUM_SKILL_KEY: &str = "colosseum-agent-hackathon";
pub const COLOSSEUM_SKILL_URL: &str = "https://colosseum.com/skill.md";
pub const COLOSSEUM_HEARTBEAT_URL: &str = "https://colosseum.com/heartbeat.md";

const COLOSSEUM_API_BASE: &str = "https://agents.colosseum.com/api";
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

fn resolve_colosseum_skill_dir() -> PathBuf {
    resolve_managed_skills_dir().join(COLOSSEUM_SKILL_KEY)
}

fn resolve_colosseum_skill_path() -> PathBuf {
    resolve_colosseum_skill_dir().join("SKILL.md")
}

fn resolve_colosseum_heartbeat_path() -> PathBuf {
    resolve_colosseum_skill_dir().join("HEARTBEAT.md")
}

fn resolve_meta_path(path: &Path) -> PathBuf {
    PathBuf::from(format!("{}.meta.json", path.to_string_lossy()))
}

fn load_meta(path: &Path) -> Option<RemoteFileMeta> {
    read_json_file(&resolve_meta_path(path))
}

fn should_attempt_sync(path: &Path, meta: Option<&RemoteFileMeta>, min_interval_ms: u64) -> bool {
    if std::env::var("DRBOT_OPENCLAW_COLOSSEUM_SYNC")
        .ok()
        .as_deref()
        == Some("1")
    {
        return true;
    }

    if path.exists() {
        return true;
    }

    // If the operator created a skill config entry (even without an apiKey yet),
    // treat that as intent to use this skill and allow syncing.
    let file = load_skills_config_file();
    if file.entries.contains_key(COLOSSEUM_SKILL_KEY) {
        return true;
    }

    // Otherwise, do not auto-fetch remote skill content.
    if let Some(meta) = meta {
        let elapsed = now_ms().saturating_sub(meta.fetched_at_ms);
        if elapsed < min_interval_ms {
            return false;
        }
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

async fn sync_remote_markdown(
    client: &reqwest::Client,
    url: &str,
    path: &Path,
    min_interval_ms: u64,
) -> Result<bool, String> {
    let meta = load_meta(path);
    let prev_etag = meta.as_ref().and_then(|m| m.etag.clone());
    let prev_last_modified = meta.as_ref().and_then(|m| m.last_modified.clone());
    if meta.is_some() {
        let elapsed = now_ms().saturating_sub(meta.as_ref().unwrap().fetched_at_ms);
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

fn colosseum_skill_enabled() -> bool {
    let file = load_skills_config_file();
    let entry = file.entries.get(COLOSSEUM_SKILL_KEY);
    entry.and_then(|e| e.enabled).unwrap_or(true)
}

fn colosseum_api_key() -> Option<String> {
    // Prefer explicit skill config (OpenClaw-style).
    let file = load_skills_config_file();
    let entry = file.entries.get(COLOSSEUM_SKILL_KEY)?;
    if entry.enabled == Some(false) {
        return None;
    }
    entry.api_key.clone().or_else(|| {
        // Allow env override for convenience.
        std::env::var("COLOSSEUM_API_KEY").ok().and_then(|s| {
            let trimmed = s.trim().to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        })
    })
}

pub async fn colosseum_request(
    method: &str,
    path: &str,
    query: Option<&serde_json::Value>,
    body: Option<&serde_json::Value>,
    timeout_ms: Option<u64>,
    dry_run: bool,
    allow_write: bool,
) -> Result<serde_json::Value, ErrorShape> {
    if !colosseum_skill_enabled() {
        return Err(ErrorShape::new(
            error_codes::UNAVAILABLE,
            "colosseum skill disabled",
        ));
    }
    let api_key = colosseum_api_key().ok_or_else(|| {
        ErrorShape::new(
            error_codes::NOT_LINKED,
            "colosseum apiKey not configured (use skills.update for colosseum-agent-hackathon or set COLOSSEUM_API_KEY)",
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

    let allow_write = allow_write
        || std::env::var("DRBOT_OPENCLAW_COLOSSEUM_WRITE")
        .ok()
        .as_deref()
        == Some("1");
    let is_write = matches!(method.as_str(), "POST" | "PUT" | "PATCH" | "DELETE");
    if is_write && !dry_run && !allow_write {
        return Err(ErrorShape::new(
            error_codes::UNAVAILABLE,
            "colosseum write requests disabled (set DRBOT_OPENCLAW_COLOSSEUM_WRITE=1)",
        ));
    }

    let mut url = COLOSSEUM_API_BASE.trim_end_matches('/').to_string();
    if path.starts_with('/') {
        url.push_str(path);
    } else {
        url.push('/');
        url.push_str(path);
    }

    // Return request preview without side effects.
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
        "drbot/{} (+openclaw-colosseum-request)",
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
                serde_json::Value::Object(_) => {
                    // Ignore nested structures for query params.
                }
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
                    // If body is provided as a raw string, send it as-is.
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

pub async fn sync_colosseum_docs_best_effort(cfg: &Config) {
    if !colosseum_skill_enabled() {
        return;
    }

    let skill_path = resolve_colosseum_skill_path();
    let heartbeat_path = resolve_colosseum_heartbeat_path();

    let meta_skill = load_meta(&skill_path);
    if !should_attempt_sync(
        &skill_path,
        meta_skill.as_ref(),
        DEFAULT_SYNC_MIN_INTERVAL_MS,
    ) {
        return;
    }

    let ua = format!(
        "drbot/{} (+openclaw-colosseum-sync)",
        env!("CARGO_PKG_VERSION")
    );
    let timeout_secs = std::env::var("DRBOT_OPENCLAW_COLOSSEUM_SYNC_TIMEOUT_SECS")
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
            warn!(error = %err, "colosseum: failed to build http client");
            return;
        }
    };

    let min_interval_ms = std::env::var("DRBOT_OPENCLAW_COLOSSEUM_SYNC_MIN_INTERVAL_MS")
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .filter(|v| *v >= 1_000)
        .unwrap_or(DEFAULT_SYNC_MIN_INTERVAL_MS);

    match sync_remote_markdown(&client, COLOSSEUM_SKILL_URL, &skill_path, min_interval_ms).await {
        Ok(updated) => {
            debug!(
                updated,
                path = %skill_path.to_string_lossy(),
                "colosseum: skill.md sync"
            );
        }
        Err(err) => warn!(error = %err, "colosseum: skill.md sync failed"),
    }

    match sync_remote_markdown(
        &client,
        COLOSSEUM_HEARTBEAT_URL,
        &heartbeat_path,
        min_interval_ms,
    )
    .await
    {
        Ok(updated) => {
            debug!(
                updated,
                path = %heartbeat_path.to_string_lossy(),
                "colosseum: heartbeat.md sync"
            );
        }
        Err(err) => warn!(error = %err, "colosseum: heartbeat.md sync failed"),
    }

    let _ = cfg;
}

pub fn load_colosseum_local_heartbeat() -> Option<String> {
    let path = resolve_colosseum_heartbeat_path();
    std::fs::read_to_string(path).ok()
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

pub async fn fetch_colosseum_heartbeat_context() -> Option<serde_json::Value> {
    if !colosseum_skill_enabled() {
        return None;
    }
    let api_key = colosseum_api_key()?;

    let ua = format!(
        "drbot/{} (+openclaw-colosseum-heartbeat)",
        env!("CARGO_PKG_VERSION")
    );
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(ua)
        .build()
        .ok()?;

    // 1) Agent status (primary pull signal).
    let status_url = format!("{}/agents/status", COLOSSEUM_API_BASE);
    let agent_status: Option<serde_json::Value> = match client
        .get(status_url)
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await
    {
        Ok(r) => r.json::<serde_json::Value>().await.ok(),
        Err(_) => None,
    };

    // 2) Active hackathon + top leaderboard.
    let active_url = format!("{}/hackathons/active", COLOSSEUM_API_BASE);
    let hackathon_active: Option<serde_json::Value> = match client.get(active_url).send().await {
        Ok(r) => r.json::<serde_json::Value>().await.ok(),
        Err(_) => None,
    };

    let hackathon_id = hackathon_active
        .as_ref()
        .and_then(|v| v.get("hackathonId").and_then(|x| x.as_i64()))
        .or_else(|| {
            hackathon_active
                .as_ref()
                .and_then(|v| v.get("id").and_then(|x| x.as_i64()))
        });

    let leaderboard: Option<serde_json::Value> = if let Some(id) = hackathon_id {
        let url = format!(
            "{}/hackathons/{}/leaderboard?limit=10",
            COLOSSEUM_API_BASE, id
        );
        match client.get(url).send().await {
            Ok(r) => r.json::<serde_json::Value>().await.ok(),
            Err(_) => None,
        }
    } else {
        None
    };

    // 3) Forum: newest posts (public).
    let forum_url = format!("{}/forum/posts?sort=new&limit=20", COLOSSEUM_API_BASE);
    let forum_posts: Option<serde_json::Value> = match client.get(forum_url).send().await {
        Ok(r) => r.json::<serde_json::Value>().await.ok(),
        Err(_) => None,
    };

    // 4) Authenticated snapshots: my project/team + my forum activity.
    let auth_header = ("Authorization", format!("Bearer {}", api_key));

    let my_team_url = format!("{}/my-team", COLOSSEUM_API_BASE);
    let my_team: Option<serde_json::Value> = match client
        .get(my_team_url)
        .header(auth_header.0, auth_header.1.clone())
        .send()
        .await
    {
        Ok(r) => r.json::<serde_json::Value>().await.ok(),
        Err(_) => None,
    };

    let my_project_url = format!("{}/my-project", COLOSSEUM_API_BASE);
    let my_project: Option<serde_json::Value> = match client
        .get(my_project_url)
        .header(auth_header.0, auth_header.1.clone())
        .send()
        .await
    {
        Ok(r) => r.json::<serde_json::Value>().await.ok(),
        Err(_) => None,
    };

    let my_posts_url = format!("{}/forum/me/posts?sort=new&limit=20", COLOSSEUM_API_BASE);
    let forum_me_posts: Option<serde_json::Value> = match client
        .get(my_posts_url)
        .header(auth_header.0, auth_header.1.clone())
        .send()
        .await
    {
        Ok(r) => r.json::<serde_json::Value>().await.ok(),
        Err(_) => None,
    };

    let my_comments_url = format!("{}/forum/me/comments?sort=new&limit=20", COLOSSEUM_API_BASE);
    let forum_me_comments: Option<serde_json::Value> = match client
        .get(my_comments_url)
        .header(auth_header.0, auth_header.1)
        .send()
        .await
    {
        Ok(r) => r.json::<serde_json::Value>().await.ok(),
        Err(_) => None,
    };

    // Keep payload small (and stable) for prompts.
    let agent_status =
        agent_status.map(|v| pick_fields(&v, &["status", "hackathon", "engagement", "nextSteps"]));
    let hackathon_active = hackathon_active.map(|v| {
        pick_fields(
            &v,
            &[
                "id",
                "hackathonId",
                "name",
                "active",
                "endsAt",
                "endDate",
                "endAt",
                "status",
            ],
        )
    });
    let leaderboard = leaderboard.map(|v| {
        if v.get("entries").and_then(|x| x.as_array()).is_some() {
            let mut next = v.clone();
            if let Some(entries) = next.get("entries") {
                let trimmed = trim_array(entries, 10);
                if let Some(map) = next.as_object_mut() {
                    map.insert("entries".to_string(), trimmed);
                }
            }
            next
        } else {
            v
        }
    });
    let forum_posts = forum_posts.map(|v| {
        // Common shapes: { posts: [...] } or [...].
        if let Some(posts) = v.get("posts") {
            let mut next = serde_json::Map::new();
            next.insert("posts".to_string(), trim_array(posts, 20));
            serde_json::Value::Object(next)
        } else {
            trim_array(&v, 20)
        }
    });

    let my_team = my_team.map(|v| pick_fields(&v, &["team", "inviteCode", "invite_code"]));
    let my_project = my_project.map(|v| {
        pick_fields(
            &v,
            &[
                "project",
                "status",
                "name",
                "slug",
                "repoLink",
                "repo_link",
                "technicalDemoLink",
                "technical_demo_link",
                "presentationLink",
                "presentation_link",
                "tags",
            ],
        )
    });
    let forum_me_posts = forum_me_posts.map(|v| {
        // Common shapes: { posts: [...] } or [...].
        if let Some(posts) = v.get("posts") {
            let mut next = serde_json::Map::new();
            next.insert("posts".to_string(), trim_array(posts, 20));
            serde_json::Value::Object(next)
        } else {
            trim_array(&v, 20)
        }
    });
    let forum_me_comments = forum_me_comments.map(|v| {
        // Common shapes: { comments: [...] } or [...].
        if let Some(comments) = v.get("comments") {
            let mut next = serde_json::Map::new();
            next.insert("comments".to_string(), trim_array(comments, 20));
            serde_json::Value::Object(next)
        } else {
            trim_array(&v, 20)
        }
    });

    Some(json!({
        "ts": now_ms(),
        "agentStatus": agent_status,
        "hackathonActive": hackathon_active,
        "leaderboard": leaderboard,
        "forumPostsNew": forum_posts,
        "myTeam": my_team,
        "myProject": my_project,
        "forumMePosts": forum_me_posts,
        "forumMeComments": forum_me_comments,
    }))
}
