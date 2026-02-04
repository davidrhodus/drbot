//! Shared path resolution helpers for OpenClaw v3 compatibility.
//!
//! OpenClaw itself defaults to `~/.openclaw`, but drbot prefers co-locating
//! OpenClaw state with the configured storage directory so tests and
//! multi-instance deployments remain isolated. Operators can override this via:
//! - `OPENCLAW_STATE_DIR`
//! - `CLAWDBOT_STATE_DIR` (legacy)

use drbot_core::Config;
use std::path::PathBuf;

pub fn resolve_home_dir() -> Option<PathBuf> {
    if let Ok(home) = std::env::var("HOME") {
        let trimmed = home.trim();
        if !trimmed.is_empty() {
            return Some(PathBuf::from(trimmed));
        }
    }
    if let Ok(profile) = std::env::var("USERPROFILE") {
        let trimmed = profile.trim();
        if !trimmed.is_empty() {
            return Some(PathBuf::from(trimmed));
        }
    }
    None
}

pub fn resolve_user_path(input: &str) -> PathBuf {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return PathBuf::from(trimmed);
    }
    if trimmed.starts_with('~') {
        if let Some(home) = resolve_home_dir() {
            let expanded = trimmed.replacen('~', &home.to_string_lossy(), 1);
            let candidate = PathBuf::from(&expanded);
            return candidate.canonicalize().unwrap_or_else(|_| candidate);
        }
    }
    PathBuf::from(trimmed)
}

fn resolve_state_dir_override() -> Option<PathBuf> {
    let override_env = std::env::var("OPENCLAW_STATE_DIR")
        .ok()
        .or_else(|| std::env::var("CLAWDBOT_STATE_DIR").ok())
        .unwrap_or_default();
    let trimmed = override_env.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(resolve_user_path(trimmed))
    }
}

pub fn resolve_openclaw_state_dir(cfg: &Config) -> Option<PathBuf> {
    if let Some(dir) = resolve_state_dir_override() {
        return Some(dir);
    }

    let base = cfg
        .storage
        .database_path
        .parent()
        .map(|p| p.to_path_buf())
        .or_else(Config::data_dir)
        .or_else(|| resolve_home_dir().map(|h| h.join(".openclaw")))?;

    if base.is_absolute() {
        Some(base)
    } else {
        Some(
            std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(base),
        )
    }
}

pub fn resolve_managed_skills_dir(cfg: &Config) -> PathBuf {
    resolve_openclaw_state_dir(cfg)
        .map(|d| d.join("skills"))
        .or_else(|| Config::config_dir().map(|d| d.join("skills")))
        .unwrap_or_else(|| PathBuf::from("skills"))
}

