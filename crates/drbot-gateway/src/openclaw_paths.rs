//! Shared path resolution helpers for OpenClaw v3 compatibility.
//!
//! OpenClaw itself defaults to `~/.openclaw`, but drbot prefers co-locating
//! OpenClaw state with the configured storage directory so tests and
//! multi-instance deployments remain isolated. Operators can override this via:
//! - `OPENCLAW_STATE_DIR`
//! - `CLAWDBOT_STATE_DIR` (legacy)
//! - `OPENCLAW_HOME` (OpenClaw v2026.2.9+)

use drbot_core::Config;
use std::path::PathBuf;

pub const DEFAULT_AGENT_ID: &str = "default";

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
        .or_else(|| std::env::var("OPENCLAW_HOME").ok())
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

pub fn resolve_agent_workspace_dir(agent_id: &str) -> PathBuf {
    let safe = normalize_agent_id(agent_id);

    if let Some(dir) = drbot_core::Config::config_dir() {
        return dir.join("agents").join(safe);
    }
    PathBuf::from("agents").join(safe)
}

pub fn normalize_agent_id(value: &str) -> String {
    const MAX_LEN: usize = 64;

    let trimmed = value.trim();
    if trimmed.is_empty() {
        return DEFAULT_AGENT_ID.to_string();
    }

    fn is_valid(token: &str) -> bool {
        const MAX_LEN: usize = 64;
        if token.is_empty() || token.len() > MAX_LEN {
            return false;
        }
        let bytes = token.as_bytes();
        if !bytes[0].is_ascii_alphanumeric() {
            return false;
        }
        bytes
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || *b == b'_' || *b == b'-')
    }

    if is_valid(trimmed) {
        return trimmed.to_ascii_lowercase();
    }

    // Best-effort fallback (OpenClaw parity): collapse invalid chars into "-", then strip leading/trailing dashes.
    let mut out = String::with_capacity(trimmed.len().min(MAX_LEN));
    let mut last_dash = false;
    for ch in trimmed.chars() {
        if out.len() >= MAX_LEN {
            break;
        }
        let c = ch.to_ascii_lowercase();
        if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
            out.push(c);
            last_dash = false;
            continue;
        }
        if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }

    let out = out.trim_matches('-').to_string();
    if out.is_empty() {
        DEFAULT_AGENT_ID.to_string()
    } else {
        out
    }
}

pub fn resolve_agents_base_dir() -> PathBuf {
    if let Some(dir) = drbot_core::Config::config_dir() {
        return dir.join("agents");
    }
    PathBuf::from("agents")
}

pub fn list_agent_ids() -> Vec<String> {
    use std::collections::BTreeSet;

    let base = resolve_agents_base_dir();
    let mut ids: BTreeSet<String> = BTreeSet::new();
    ids.insert(DEFAULT_AGENT_ID.to_string());

    if let Ok(entries) = std::fs::read_dir(&base) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            ids.insert(normalize_agent_id(&name));
        }
    }

    ids.into_iter().collect()
}

pub fn list_agent_workspace_dirs() -> Vec<PathBuf> {
    list_agent_ids()
        .into_iter()
        .map(|id| resolve_agent_workspace_dir(&id))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_agent_id_matches_openclaw_style_rules() {
        assert_eq!(normalize_agent_id(""), DEFAULT_AGENT_ID);
        assert_eq!(normalize_agent_id("  "), DEFAULT_AGENT_ID);
        assert_eq!(normalize_agent_id("Main"), "main");
        assert_eq!(normalize_agent_id("foo_bar"), "foo_bar");
        assert_eq!(normalize_agent_id("foo/bar"), "foo-bar");
        assert_eq!(normalize_agent_id("../evil"), "evil");
        assert_eq!(normalize_agent_id("a".repeat(200).as_str()).len(), 64);
    }
}
