//! OpenClaw agent workspace prompt helpers.
//!
//! OpenClaw agents often keep editable markdown files in their workspace
//! (AGENTS.md, MEMORY.md, etc). The gateway should surface these to model runs
//! as part of the system prompt so edits take effect without tool reads.

use std::path::Path;

const WORKSPACE_CONTEXT_FILES: &[&str] = &[
    "IDENTITY.md",
    "SOUL.md",
    "USER.md",
    "MEMORY.md",
    "memory.md",
    "AGENTS.md",
    "TOOLS.md",
    "BOOTSTRAP.md",
];

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

/// Build a bounded system-prompt section from an OpenClaw agent workspace.
///
/// Returns an empty string if no context files are present/readable.
pub(crate) fn build_workspace_context_prompt(workspace_dir: &Path) -> String {
    if !workspace_dir.is_dir() {
        return String::new();
    }

    let max_total_bytes = env_u64(
        "DRBOT_OPENCLAW_WORKSPACE_CONTEXT_MAX_BYTES",
        60_000,
        4_096,
        2_000_000,
    ) as usize;
    let max_file_bytes = env_u64(
        "DRBOT_OPENCLAW_WORKSPACE_CONTEXT_MAX_FILE_BYTES",
        30_000,
        1_024,
        2_000_000,
    ) as usize;

    let mut out = String::new();
    let mut used_files: std::collections::HashSet<&'static str> = std::collections::HashSet::new();

    for filename in WORKSPACE_CONTEXT_FILES {
        if out.len() >= max_total_bytes {
            break;
        }
        if !used_files.insert(filename) {
            continue;
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
