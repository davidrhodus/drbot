//! Workspace context prompt for gateway chat runs.
//!
//! Gateway chat (`chat.send`) doesn't have the full OpenClaw agent tool surface, but we still
//! want consistent personalization and a lightweight knowledge base via markdown files.

use std::path::Path;

const CHAT_CONTEXT_FILES: &[&str] = &["IDENTITY.md", "USER.md", "MEMORY.md", "memory.md"];

fn env_u64(key: &str, default: u64, min: u64, max: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .unwrap_or(default)
        .clamp(min, max)
}

fn truncate_to_bytes(input: &str, max_bytes: usize) -> String {
    if input.len() <= max_bytes {
        return input.to_string();
    }
    let mut idx = 0usize;
    for (i, ch) in input.char_indices() {
        let next = i + ch.len_utf8();
        if next > max_bytes {
            break;
        }
        idx = next;
    }
    input[..idx].to_string()
}

pub fn build_chat_workspace_context_prompt(workspace_dir: &Path) -> String {
    if !workspace_dir.is_dir() {
        return String::new();
    }

    let enabled = std::env::var("DRBOT_GATEWAY_WORKSPACE_CONTEXT_ENABLED")
        .ok()
        .map(|v| {
            !matches!(
                v.trim().to_ascii_lowercase().as_str(),
                "0" | "false" | "no" | "off"
            )
        })
        .unwrap_or(true);
    if !enabled {
        return String::new();
    }

    let max_total_bytes = env_u64(
        "DRBOT_GATEWAY_WORKSPACE_CONTEXT_MAX_BYTES",
        24_000,
        1_024,
        2_000_000,
    ) as usize;
    let max_file_bytes = env_u64(
        "DRBOT_GATEWAY_WORKSPACE_CONTEXT_MAX_FILE_BYTES",
        12_000,
        512,
        2_000_000,
    ) as usize;

    let mut out = String::new();
    for filename in CHAT_CONTEXT_FILES {
        if out.len() >= max_total_bytes {
            break;
        }
        let path = workspace_dir.join(filename);
        if !path.is_file() {
            continue;
        }

        let Ok(raw) = std::fs::read_to_string(&path) else {
            continue;
        };
        let body = drbot_core::markdown::strip_frontmatter(&raw);
        let body = body.trim();
        if body.is_empty() {
            continue;
        }

        let mut body = if body.len() > max_file_bytes {
            let mut truncated = truncate_to_bytes(body, max_file_bytes);
            truncated.push_str("\n\n[truncated]");
            truncated
        } else {
            body.to_string()
        };

        let mut section = String::new();
        if out.is_empty() {
            section.push_str("Workspace context:\n\n");
        } else {
            section.push_str("\n\n---\n\n");
        }
        section.push_str(filename);
        section.push_str(":\n");
        section.push_str(body.trim());

        if out.len() + section.len() > max_total_bytes {
            let remaining = max_total_bytes.saturating_sub(out.len());
            if remaining < 64 {
                break;
            }
            section = truncate_to_bytes(&section, remaining);
            body.clear();
            out.push_str(&section);
            out.push_str("\n\n[truncated workspace context]");
            break;
        }

        out.push_str(&section);
    }

    out
}
