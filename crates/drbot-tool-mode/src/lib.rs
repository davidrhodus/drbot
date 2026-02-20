//! Shared utilities for drbot "tool mode":
//! - Parsing tool calls from assistant text
//! - Building a tool-mode system prompt
//! - Local tool execution (bash, file ops, search, apply_patch)

use anyhow::Result;
use drbot_core::config::AutonomyMode;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct ToolCallSpec {
    pub tool: String,
    pub args: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct ToolModeConfig {
    pub enabled: bool,
    pub auto_approve: bool,
    pub root: PathBuf,
    pub max_rounds: usize,
    pub autonomy_mode: AutonomyMode,
    pub tool_allowlist: Vec<String>,
    pub tool_denylist: Vec<String>,
}

pub fn expand_tilde(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return format!("{}/{}", home.trim_end_matches('/'), rest);
        }
    }
    if path == "~" {
        if let Ok(home) = std::env::var("HOME") {
            return home;
        }
    }
    path.to_string()
}

pub fn canonicalize_root(root: &Path) -> Result<PathBuf> {
    root.canonicalize()
        .map_err(|e| anyhow::anyhow!("Failed to resolve root '{}': {}", root.display(), e))
}

fn resolve_policy_path(path: &PathBuf) -> Option<PathBuf> {
    if path.as_os_str().is_empty() {
        return None;
    }
    let raw = path.to_string_lossy();
    let expanded = expand_tilde(raw.as_ref());
    let mut resolved = PathBuf::from(expanded);
    if !resolved.is_absolute() {
        if let Ok(cwd) = std::env::current_dir() {
            resolved = cwd.join(resolved);
        }
    }
    Some(resolved)
}

fn canonicalize_policy_path(path: &Path) -> Option<PathBuf> {
    if path.exists() {
        return path.canonicalize().ok();
    }
    let parent = path.parent()?;
    let parent_canon = parent.canonicalize().ok()?;
    let name = path.file_name()?;
    Some(parent_canon.join(name))
}

fn workspace_root_allowed(candidate: &Path, allowlist: &[PathBuf]) -> bool {
    if allowlist.is_empty() {
        return true;
    }
    let Some(candidate_canon) = canonicalize_policy_path(candidate) else {
        return false;
    };
    for root in allowlist {
        let Some(resolved) = resolve_policy_path(root) else {
            continue;
        };
        let Some(root_canon) = canonicalize_policy_path(resolved.as_path()) else {
            continue;
        };
        if candidate_canon.starts_with(&root_canon) {
            return true;
        }
    }
    false
}

fn default_root_from_allowlist(allowlist: &[PathBuf]) -> Option<PathBuf> {
    for root in allowlist {
        let Some(resolved) = resolve_policy_path(root) else {
            continue;
        };
        if let Ok(canon) = canonicalize_root(&resolved) {
            return Some(canon);
        }
    }
    None
}

pub fn resolve_tool_root_with_allowlist(
    candidate: PathBuf,
    allowlist: &[PathBuf],
) -> Result<(PathBuf, bool)> {
    let candidate_canon = canonicalize_root(&candidate)?;
    if allowlist.is_empty() {
        return Ok((candidate_canon, false));
    }
    if workspace_root_allowed(&candidate_canon, allowlist) {
        return Ok((candidate_canon, false));
    }
    if let Some(default_root) = default_root_from_allowlist(allowlist) {
        return Ok((default_root, true));
    }
    Ok((candidate_canon, false))
}

pub fn find_git_root_best_effort(start: &Path) -> Option<PathBuf> {
    for dir in start.ancestors().take(32) {
        let git = dir.join(".git");
        if git.is_dir() || git.is_file() {
            return Some(dir.to_path_buf());
        }
    }
    None
}

pub fn resolve_project_drbot_dir_best_effort(start: &Path) -> PathBuf {
    find_git_root_best_effort(start)
        .unwrap_or_else(|| start.to_path_buf())
        .join(".drbot")
}

pub fn project_kb_auto_init_enabled() -> bool {
    std::env::var("DRBOT_PROJECT_KB_AUTO_INIT_ENABLED")
        .ok()
        .map(|v| {
            !matches!(
                v.trim().to_ascii_lowercase().as_str(),
                "0" | "false" | "no" | "off"
            )
        })
        .unwrap_or(true)
}

pub fn project_kb_autosave_enabled() -> bool {
    std::env::var("DRBOT_PROJECT_KB_AUTOSAVE_ENABLED")
        .ok()
        .map(|v| {
            !matches!(
                v.trim().to_ascii_lowercase().as_str(),
                "0" | "false" | "no" | "off"
            )
        })
        .unwrap_or(true)
}

pub fn ensure_project_kb_bootstrap(project_drbot_dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(project_drbot_dir)?;

    let memory_path = project_drbot_dir.join("MEMORY.md");
    let memory_alt_path = project_drbot_dir.join("memory.md");
    if !memory_path.exists() && !memory_alt_path.exists() {
        let _ = std::fs::write(
            &memory_path,
            r#"# Project Memory

Project-local notes for drbot (scoped to this repo).

- Keep *always-relevant* items short and stable.
- Put longer docs/notes in `memory/` as separate Markdown files (easier to search).

## Pinned

## Conventions

## Runbooks

## Knowledge base
"#,
        );
    }

    let memory_dir = project_drbot_dir.join("memory");
    std::fs::create_dir_all(&memory_dir)?;
    let readme = memory_dir.join("README.md");
    if !readme.exists() {
        let _ = std::fs::write(
            &readme,
            r#"# Project Knowledge Base

Put project-specific notes/docs here as Markdown files (drbot can search them when chatting in this repo).

Suggested files:
- `architecture.md`
- `runbook.md`
- `conventions.md`
- `people.md`
"#,
        );
    }

    let gitignore = project_drbot_dir.join(".gitignore");
    if !gitignore.exists() {
        let _ = std::fs::write(
            &gitignore,
            r#"# drbot project knowledge base

# Ignore auto-generated long notes (links live in MEMORY.md).
memory/auto/

# Ignore atomic-write temp files (best effort).
*.tmp.*
"#,
        );
    }

    Ok(())
}

pub fn ensure_project_kb_bootstrap_best_effort(project_drbot_dir: &Path) {
    let _ = ensure_project_kb_bootstrap(project_drbot_dir);
}

fn workspace_autosave_enabled() -> bool {
    std::env::var("DRBOT_GATEWAY_WORKSPACE_AUTOSAVE_ENABLED")
        .ok()
        .map(|v| {
            !matches!(
                v.trim().to_ascii_lowercase().as_str(),
                "0" | "false" | "no" | "off"
            )
        })
        .unwrap_or(true)
}

fn write_atomic(path: &Path, content: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension(format!(
        "tmp.{}",
        uuid::Uuid::new_v4().to_string().replace('-', "")
    ));
    std::fs::write(&tmp, content)?;
    if std::fs::rename(&tmp, path).is_err() {
        let _ = std::fs::remove_file(path);
        std::fs::rename(&tmp, path)?;
    }
    let _ = std::fs::remove_file(&tmp);
    Ok(())
}

fn memory_md_path(root: &Path) -> PathBuf {
    let primary = root.join("MEMORY.md");
    if primary.is_file() {
        return primary;
    }
    let alt = root.join("memory.md");
    if alt.is_file() {
        return alt;
    }
    primary
}

fn find_section_range(lines: &[String], header: &str) -> Option<(usize, usize)> {
    let mut start = None;
    for (i, line) in lines.iter().enumerate() {
        if line.trim() == header {
            start = Some(i);
            break;
        }
    }
    let start = start?;
    let mut end = lines.len();
    for (i, line) in lines.iter().enumerate().skip(start + 1) {
        let t = line.trim();
        if t.starts_with("## ") && t != header {
            end = i;
            break;
        }
    }
    Some((start, end))
}

fn add_memory_bullet(
    project_drbot_dir: &Path,
    section_header: &str,
    bullet_line: &str,
) -> std::io::Result<bool> {
    let path = memory_md_path(project_drbot_dir);
    let mut doc = std::fs::read_to_string(&path).unwrap_or_default();
    if doc.trim().is_empty() {
        // Minimal scaffold if missing.
        doc = "# Project Memory\n\n## Pinned\n\n## Conventions\n\n## Runbooks\n\n## Knowledge base\n"
            .to_string();
    }

    let bullet_line = bullet_line.trim();
    if bullet_line.is_empty() {
        return Ok(false);
    }

    let mut lines: Vec<String> = doc.lines().map(|l| l.to_string()).collect();
    if let Some((start, end)) = find_section_range(&lines, section_header) {
        // De-dupe within the section.
        let needle = bullet_line.to_ascii_lowercase();
        for line in lines.iter().take(end).skip(start + 1) {
            if line.trim().to_ascii_lowercase() == needle {
                return Ok(false);
            }
        }

        // Insert after header + one blank line if present.
        let mut insert_at = start + 1;
        if insert_at < lines.len() && lines[insert_at].trim().is_empty() {
            insert_at += 1;
        }

        lines.insert(insert_at, bullet_line.to_string());
        let out = lines.join("\n") + "\n";
        write_atomic(&path, &out)?;
        return Ok(true);
    }

    // No section header found; append at end.
    let mut out = doc;
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str("\n");
    out.push_str(section_header);
    out.push_str("\n\n");
    out.push_str(bullet_line);
    out.push('\n');
    write_atomic(&path, &out)?;
    Ok(true)
}

fn looks_sensitive(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    let needles = [
        "api key",
        "apikey",
        "secret",
        "token",
        "password",
        "passwd",
        "private key",
        "ssh-rsa",
        "-----begin",
        "bearer ",
        "sk-",
    ];
    if needles.iter().any(|n| lower.contains(n)) {
        return true;
    }

    // Heuristic: long, high-entropy token-ish strings.
    let mut run_len = 0usize;
    let mut has_alpha = false;
    let mut has_digit = false;
    for ch in text.chars() {
        let ok = ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-');
        if ok {
            run_len += 1;
            if ch.is_ascii_alphabetic() {
                has_alpha = true;
            }
            if ch.is_ascii_digit() {
                has_digit = true;
            }
            if run_len >= 32 && has_alpha && has_digit {
                return true;
            }
        } else {
            run_len = 0;
            has_alpha = false;
            has_digit = false;
        }
    }

    false
}

fn sanitize_single_line(input: &str, max_chars: usize) -> String {
    let line = input.lines().next().unwrap_or("").trim();
    let mut out = String::new();
    for ch in line.chars() {
        if out.chars().count() >= max_chars {
            break;
        }
        if ch == '\u{0}' {
            continue;
        }
        out.push(ch);
    }
    out.trim().to_string()
}

fn looks_ephemeral_fact(text_lower: &str) -> bool {
    let needles = [
        "for now",
        "right now",
        "currently",
        "at the moment",
        "temporarily",
        "maybe",
        "might",
        "consider",
        "considering",
        "thinking about",
        "plan to",
        "planning to",
        "going to",
        "we will",
        "we'll",
    ];
    needles.iter().any(|n| text_lower.contains(n))
}

fn is_pronoun_word(word_lower: &str) -> bool {
    matches!(
        word_lower,
        "this" | "that" | "it" | "these" | "those" | "something" | "anything" | "everything"
    )
}

fn extract_auto_pinned_fact_line(user_text: &str) -> Option<String> {
    let trimmed = user_text.trim();
    if trimmed.is_empty() {
        return None;
    }

    // Keep conservative: avoid auto-capturing from large pastes / code blocks.
    if trimmed.len() > 500 || trimmed.contains("```") {
        return None;
    }

    let mut line = trimmed.lines().next().unwrap_or("").trim();
    if line.is_empty() {
        return None;
    }

    // Allow common lead-ins (FYI/Note/Heads up) before stable facts.
    for lead in ["fyi", "note", "heads up", "btw", "reminder"] {
        let lower = line.to_ascii_lowercase();
        if !lower.starts_with(lead) {
            continue;
        }
        let rest = line.get(lead.len()..).unwrap_or("").trim_start();
        let rest = rest
            .trim_start_matches(&[' ', ':', '-', ','][..])
            .trim_start();
        if !rest.is_empty() {
            line = rest;
        }
        break;
    }

    if line.ends_with('?') {
        return None;
    }

    let lower = line.to_ascii_lowercase();
    if looks_ephemeral_fact(&lower) {
        return None;
    }

    // Only capture high-confidence declarative "stable fact" patterns.
    let prefixes = [
        "we use ",
        "we're using ",
        "we are using ",
        "our stack is ",
        "our stack:",
        "our database is ",
        "our database:",
        "our db is ",
        "our db:",
        "we deploy on ",
        "we host on ",
        "we run on ",
        "stack:",
        "db:",
    ];

    let mut matched_prefix = None;
    for prefix in prefixes {
        if lower.starts_with(prefix) {
            matched_prefix = Some(prefix);
            break;
        }
    }
    let Some(prefix) = matched_prefix else {
        return None;
    };

    let rest = line.get(prefix.len()..).unwrap_or("").trim();
    let first_word = rest
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    if first_word.is_empty() || is_pronoun_word(&first_word) {
        return None;
    }

    let fact = sanitize_single_line(line, 220);
    if fact.is_empty() || looks_sensitive(&fact) {
        return None;
    }

    Some(fact)
}

fn strip_common_lead_in(line: &str) -> &str {
    let mut line = line.trim_start();
    if line.is_empty() {
        return line;
    }

    // Allow common lead-ins (FYI/Note/Heads up) before project-scoped notes.
    for lead in ["fyi", "note", "heads up", "btw", "reminder"] {
        let lower = line.to_ascii_lowercase();
        if !lower.starts_with(lead) {
            continue;
        }
        let rest = line.get(lead.len()..).unwrap_or("").trim_start();
        let rest = rest
            .trim_start_matches(&[' ', ':', '-', ','][..])
            .trim_start();
        if !rest.is_empty() {
            line = rest;
        }
        break;
    }

    line
}

fn extract_project_scope_rest(line: &str) -> Option<&str> {
    let trimmed = strip_common_lead_in(line).trim_start();
    if trimmed.is_empty() {
        return None;
    }

    let lower = trimmed.to_ascii_lowercase();
    let prefixes = [
        "in this repo",
        "for this repo",
        "in this repository",
        "for this repository",
        "in this project",
        "for this project",
        "in this codebase",
        "for this codebase",
    ];
    for prefix in prefixes {
        if !lower.starts_with(prefix) {
            continue;
        }
        let mut rest = trimmed.get(prefix.len()..).unwrap_or("").trim_start();
        rest = rest
            .trim_start_matches(&[' ', ',', ':', '-', '—', '–'][..])
            .trim_start();
        if rest.is_empty() {
            return None;
        }
        return Some(rest);
    }

    None
}

fn looks_like_runbook_instruction(text_lower: &str) -> bool {
    let prefixes = [
        "run ",
        "to run ",
        "start ",
        "to start ",
        "deploy ",
        "to deploy ",
        "release ",
        "to release ",
        "rollback ",
        "to rollback ",
        "restart ",
        "to restart ",
        "debug ",
        "to debug ",
        "troubleshoot ",
        "to troubleshoot ",
    ];
    prefixes.iter().any(|p| text_lower.starts_with(p))
}

fn extract_project_scoped_note_for_autosave(user_text: &str) -> Option<String> {
    let trimmed = user_text.trim();
    if trimmed.is_empty() {
        return None;
    }

    // Avoid auto-capturing from large pastes / code blocks.
    if trimmed.contains("```") {
        return None;
    }

    let line = trimmed.lines().next().unwrap_or("").trim();
    if line.is_empty() {
        return None;
    }

    let Some(rest) = extract_project_scope_rest(line) else {
        return None;
    };

    if rest.ends_with('?') {
        return None;
    }
    if rest.chars().count() > 600 {
        return None;
    }

    let rest_lower = rest.to_ascii_lowercase();
    if looks_ephemeral_fact(&rest_lower) {
        return None;
    }
    if looks_sensitive(rest) {
        return None;
    }

    // If the user explicitly labeled the section, keep it as-is.
    let explicit_prefixes = [
        "pinned:",
        "pin:",
        "convention:",
        "conventions:",
        "runbook:",
        "runbooks:",
        "kb:",
        "doc:",
        "docs:",
    ];
    if explicit_prefixes
        .iter()
        .any(|p| rest_lower.starts_with(p))
    {
        return Some(rest.trim().to_string());
    }

    // Stable facts should land in Pinned (even when phrased as "in this repo ...").
    if let Some(fact) = extract_auto_pinned_fact_line(rest) {
        return Some(format!("pinned: {}", fact));
    }

    let rest = rest.trim();
    if rest.is_empty() {
        return None;
    }

    if looks_like_runbook_instruction(&rest_lower) {
        Some(format!("runbooks: {}", rest))
    } else {
        Some(format!("conventions: {}", rest))
    }
}

pub fn autosave_project_kb_best_effort(project_drbot_dir: &Path, user_text: &str) {
    if !workspace_autosave_enabled() || !project_kb_autosave_enabled() {
        return;
    }
    if !project_drbot_dir.is_dir() {
        if !project_kb_auto_init_enabled() {
            return;
        }
        ensure_project_kb_bootstrap_best_effort(project_drbot_dir);
    }

    if let Some(fact) = extract_auto_pinned_fact_line(user_text) {
        let bullet = format!("- {}", fact);
        let _ = add_memory_bullet(project_drbot_dir, "## Pinned", &bullet);
    }

    if let Some(note) = extract_project_scoped_note_for_autosave(user_text) {
        let _ = remember_project_kb(project_drbot_dir, &note);
    }
}

#[derive(Debug, Default, Clone)]
pub struct MemoryUpdateOutput {
    pub applied: bool,
    pub rejected: bool,
    pub updates: Vec<String>,
}

fn parse_scoped_slash_command_arg(
    text: &str,
    verb: &str,
    scope: &str,
) -> Option<Option<String>> {
    let trimmed = text.trim_start();
    let lower = trimmed.to_ascii_lowercase();
    if !lower.starts_with(verb) {
        return None;
    }

    let after_lower = lower.get(verb.len()..).unwrap_or("");
    if !after_lower.is_empty() {
        let b = after_lower.as_bytes()[0];
        if !(b.is_ascii_whitespace() || b == b':') {
            return None;
        }
    }

    let mut rest = trimmed.get(verb.len()..).unwrap_or("");
    rest = rest.trim_start();
    if rest.starts_with(':') {
        rest = rest[1..].trim_start();
    }
    if rest.is_empty() {
        return None;
    }

    let rest_lower = rest.to_ascii_lowercase();
    if !rest_lower.starts_with(scope) {
        return None;
    }
    if rest_lower.len() > scope.len() {
        let b = rest_lower.as_bytes()[scope.len()];
        if !(b.is_ascii_whitespace() || b == b':') {
            return None;
        }
    }

    let mut arg = rest.get(scope.len()..).unwrap_or("");
    arg = arg.trim_start();
    if arg.starts_with(':') {
        arg = arg[1..].trim_start();
    }
    let arg = arg.trim();
    if arg.is_empty() {
        Some(None)
    } else {
        Some(Some(arg.to_string()))
    }
}

fn parse_scoped_colon_command_arg(
    text: &str,
    verb: &str,
    scope: &str,
) -> Option<Option<String>> {
    let trimmed = text.trim_start();
    let lower = trimmed.to_ascii_lowercase();
    if !lower.starts_with(verb) {
        return None;
    }

    let after_lower = lower.get(verb.len()..).unwrap_or("");
    if !after_lower.is_empty() && !after_lower.as_bytes()[0].is_ascii_whitespace() {
        return None;
    }

    let mut rest = trimmed.get(verb.len()..).unwrap_or("");
    rest = rest.trim_start();
    if rest.is_empty() {
        return None;
    }

    let rest_lower = rest.to_ascii_lowercase();
    if !rest_lower.starts_with(scope) {
        return None;
    }
    if rest_lower.len() > scope.len() {
        let b = rest_lower.as_bytes()[scope.len()];
        if !(b.is_ascii_whitespace() || b == b':') {
            return None;
        }
    }

    let mut arg = rest.get(scope.len()..).unwrap_or("");
    arg = arg.trim_start();
    if !arg.starts_with(':') {
        // Require a colon for the non-slash form to avoid accidental matches.
        return None;
    }
    arg = arg[1..].trim_start();
    let arg = arg.trim();
    if arg.is_empty() {
        Some(None)
    } else {
        Some(Some(arg.to_string()))
    }
}

pub fn is_project_remember_command(text: &str) -> bool {
    parse_scoped_slash_command_arg(text, "/remember", "project").is_some()
        || parse_scoped_colon_command_arg(text, "remember", "project").is_some()
}

pub fn parse_project_remember_note(text: &str) -> Option<String> {
    parse_scoped_slash_command_arg(text, "/remember", "project")
        .or_else(|| parse_scoped_colon_command_arg(text, "remember", "project"))
        .flatten()
}

pub fn is_project_forget_command(text: &str) -> bool {
    parse_scoped_slash_command_arg(text, "/forget", "project").is_some()
        || parse_scoped_colon_command_arg(text, "forget", "project").is_some()
}

pub fn parse_project_forget_arg(text: &str) -> Option<String> {
    parse_scoped_slash_command_arg(text, "/forget", "project")
        .or_else(|| parse_scoped_colon_command_arg(text, "forget", "project"))
        .flatten()
}

fn maybe_store_long_project_note_as_file(
    project_drbot_dir: &Path,
    note: &str,
) -> std::io::Result<Option<String>> {
    const MAX_INLINE: usize = 220;
    let note = note.trim();
    if note.chars().count() <= MAX_INLINE {
        return Ok(None);
    }

    let memory_dir = project_drbot_dir.join("memory").join("auto");
    std::fs::create_dir_all(&memory_dir)?;
    let id = uuid::Uuid::new_v4().to_string();
    let filename = format!("note-{}.md", id);
    let rel_path = format!("memory/auto/{}", filename);
    let full_path = memory_dir.join(&filename);

    let content = format!("# Note\n\n{}\n", note.trim());
    write_atomic(&full_path, &content)?;

    let summary = sanitize_single_line(note, 140);
    Ok(Some(format!("- {}: {}", rel_path, summary)))
}

fn strip_outer_quotes(input: &str) -> &str {
    let s = input.trim();
    if s.len() >= 2 {
        let bytes = s.as_bytes();
        let first = bytes[0];
        let last = bytes[bytes.len() - 1];
        if (first == b'"' && last == b'"') || (first == b'\'' && last == b'\'') {
            return s[1..s.len() - 1].trim();
        }
    }
    s
}

fn extract_auto_note_rel_paths(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let needle = "memory/auto/";
    let mut start = 0usize;
    while let Some(pos) = text.get(start..).and_then(|s| s.find(needle)) {
        let abs = start + pos;
        let rest = text.get(abs..).unwrap_or("");
        let Some(end) = rest.find(".md") else {
            start = abs + needle.len();
            continue;
        };
        let candidate = &rest[..end + 3];
        if is_safe_auto_note_rel_path(candidate) {
            out.push(candidate.to_string());
        }
        start = abs + needle.len();
    }
    out.sort();
    out.dedup();
    out
}

fn is_safe_auto_note_rel_path(rel: &str) -> bool {
    let p = Path::new(rel);
    let mut it = p.components();
    let Some(std::path::Component::Normal(c1)) = it.next() else {
        return false;
    };
    let Some(std::path::Component::Normal(c2)) = it.next() else {
        return false;
    };
    let Some(std::path::Component::Normal(c3)) = it.next() else {
        return false;
    };
    if it.next().is_some() {
        return false;
    }
    let Some(c1) = c1.to_str() else {
        return false;
    };
    let Some(c2) = c2.to_str() else {
        return false;
    };
    if c1 != "memory" || c2 != "auto" {
        return false;
    }
    let Some(file) = c3.to_str() else {
        return false;
    };
    file.starts_with("note-") && file.ends_with(".md")
}

fn remove_memory_bullets(
    doc: &str,
    section_headers: &[&str],
    predicate: impl Fn(&str) -> bool,
) -> (String, Vec<String>, bool) {
    let targets: std::collections::HashSet<&str> = section_headers.iter().copied().collect();
    let mut removed: Vec<String> = Vec::new();
    let mut out_lines: Vec<String> = Vec::new();
    let mut current_section: Option<String> = None;
    let mut changed = false;

    for line in doc.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("## ") {
            current_section = Some(trimmed.to_string());
            out_lines.push(line.to_string());
            continue;
        }

        let in_target = current_section
            .as_deref()
            .is_some_and(|s| targets.contains(s));
        let is_bullet = line.trim_start().starts_with("- ");

        if in_target && is_bullet && predicate(trimmed) {
            removed.push(trimmed.to_string());
            changed = true;
            continue;
        }

        out_lines.push(line.to_string());
    }

    let mut out = out_lines.join("\n");
    if !out.ends_with('\n') {
        out.push('\n');
    }
    (out, removed, changed)
}

fn split_project_note_section(note: &str) -> (&'static str, &str) {
    let trimmed = note.trim_start();
    let lower = trimmed.to_ascii_lowercase();
    let candidates = [
        ("pinned:", "## Pinned"),
        ("pin:", "## Pinned"),
        ("convention:", "## Conventions"),
        ("conventions:", "## Conventions"),
        ("runbook:", "## Runbooks"),
        ("runbooks:", "## Runbooks"),
        ("kb:", "## Knowledge base"),
        ("doc:", "## Knowledge base"),
        ("docs:", "## Knowledge base"),
    ];
    for (prefix, header) in candidates {
        if lower.starts_with(prefix) {
            let rest = trimmed.get(prefix.len()..).unwrap_or("").trim();
            if !rest.is_empty() {
                return (header, rest);
            }
        }
    }
    ("## Pinned", note)
}

pub fn remember_project_kb(project_drbot_dir: &Path, note: &str) -> std::io::Result<MemoryUpdateOutput> {
    let mut out = MemoryUpdateOutput::default();
    let note = note.trim();
    if note.is_empty() {
        return Ok(out);
    }
    if looks_sensitive(note) {
        out.rejected = true;
        return Ok(out);
    }

    ensure_project_kb_bootstrap(project_drbot_dir)?;

    let (section_header, body) = split_project_note_section(note);
    let body = body.trim();
    if body.is_empty() {
        return Ok(out);
    }

    if let Some(bullet) = maybe_store_long_project_note_as_file(project_drbot_dir, body)? {
        if add_memory_bullet(project_drbot_dir, section_header, &bullet)? {
            out.applied = true;
            out.updates.push(format!(
                "Project MEMORY.md: added {} note",
                section_header.trim_start_matches("## ").trim()
            ));
        }
        return Ok(out);
    }

    let short = sanitize_single_line(body, 220);
    if short.is_empty() {
        return Ok(out);
    }
    let bullet = format!("- {}", short);
    if add_memory_bullet(project_drbot_dir, section_header, &bullet)? {
        out.applied = true;
        out.updates.push(format!(
            "Project MEMORY.md: added {} note",
            section_header.trim_start_matches("## ").trim()
        ));
    }
    Ok(out)
}

pub fn forget_project_kb(project_drbot_dir: &Path, arg_raw: &str) -> std::io::Result<MemoryUpdateOutput> {
    let mut out = MemoryUpdateOutput::default();
    if !project_drbot_dir.is_dir() {
        return Ok(out);
    }

    let arg = strip_outer_quotes(arg_raw).trim().to_string();
    if arg.is_empty() {
        return Ok(out);
    }
    let arg_lower = arg.to_ascii_lowercase();

    let path = memory_md_path(project_drbot_dir);
    let doc_raw = std::fs::read_to_string(&path).unwrap_or_default();
    if doc_raw.trim().is_empty() {
        return Ok(out);
    }
    let mut doc = doc_raw.clone();

    let all_sections = ["## Pinned", "## Conventions", "## Runbooks", "## Knowledge base"];
    let (targets, predicate): (Vec<&str>, Box<dyn Fn(&str) -> bool>) = match arg_lower.as_str() {
        "all" => (all_sections.to_vec(), Box::new(|line| line.trim_start().starts_with("- "))),
        "pinned" | "pin" => (vec!["## Pinned"], Box::new(|line| line.trim_start().starts_with("- "))),
        "conventions" | "convention" => {
            (vec!["## Conventions"], Box::new(|line| line.trim_start().starts_with("- ")))
        }
        "runbooks" | "runbook" => {
            (vec!["## Runbooks"], Box::new(|line| line.trim_start().starts_with("- ")))
        }
        "knowledge" | "kb" | "knowledge base" => (
            vec!["## Knowledge base"],
            Box::new(|line| line.trim_start().starts_with("- ")),
        ),
        _ => {
            let needle = arg_lower.clone();
            (
                all_sections.to_vec(),
                Box::new(move |line| line.to_ascii_lowercase().contains(&needle)),
            )
        }
    };

    let (next, removed_lines, changed) = remove_memory_bullets(&doc, &targets, |line| predicate(line));
    doc = next;

    if changed && doc != doc_raw {
        write_atomic(&path, &doc)?;
        out.applied = true;
        if !removed_lines.is_empty() {
            out.updates.push(format!(
                "Project MEMORY.md: removed {} item(s)",
                removed_lines.len()
            ));
        }
    }

    let mut delete_rel_paths: Vec<String> = Vec::new();
    for line in &removed_lines {
        delete_rel_paths.extend(extract_auto_note_rel_paths(line));
    }
    delete_rel_paths.sort();
    delete_rel_paths.dedup();

    for rel in delete_rel_paths {
        let rel = rel.trim();
        if !is_safe_auto_note_rel_path(rel) {
            continue;
        }
        let full = project_drbot_dir.join(rel);
        if full.is_file() {
            let _ = std::fs::remove_file(&full);
            out.applied = true;
            out.updates.push(format!("Deleted {}", rel));
        }
    }

    Ok(out)
}

fn read_to_string_if_file(path: &Path) -> Option<String> {
    if !path.is_file() {
        return None;
    }
    std::fs::read_to_string(path).ok()
}

fn collect_markdown_files(dir: &Path, max_files: usize) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    let mut stack: Vec<(PathBuf, usize)> = vec![(dir.to_path_buf(), 0)];

    while let Some((cur, depth)) = stack.pop() {
        if out.len() >= max_files {
            break;
        }
        if depth > 8 {
            continue;
        }
        let rd = match std::fs::read_dir(&cur) {
            Ok(v) => v,
            Err(_) => continue,
        };
        for entry in rd.flatten() {
            if out.len() >= max_files {
                break;
            }
            let path = entry.path();
            let meta = match entry.metadata() {
                Ok(v) => v,
                Err(_) => continue,
            };
            if meta.is_dir() {
                stack.push((path, depth + 1));
                continue;
            }
            if !meta.is_file() {
                continue;
            }
            if path
                .extension()
                .and_then(|s| s.to_str())
                .map(|s| s.eq_ignore_ascii_case("md"))
                .unwrap_or(false)
            {
                out.push(path);
            }
        }
    }

    out.sort();
    out
}

fn normalize_rel_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .ok()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string())
        .replace('\\', "/")
}

fn count_project_memory_bullets_by_section(doc: &str) -> (usize, usize, usize, usize) {
    let mut pinned = 0usize;
    let mut conventions = 0usize;
    let mut runbooks = 0usize;
    let mut kb = 0usize;
    let mut section: Option<&str> = None;
    for line in doc.lines() {
        let t = line.trim();
        if t == "## Pinned" {
            section = Some("pinned");
            continue;
        }
        if t == "## Conventions" {
            section = Some("conventions");
            continue;
        }
        if t == "## Runbooks" {
            section = Some("runbooks");
            continue;
        }
        if t == "## Knowledge base" {
            section = Some("kb");
            continue;
        }
        if t.starts_with("## ") {
            section = None;
            continue;
        }
        if !t.starts_with("- ") {
            continue;
        }
        match section {
            Some("pinned") => pinned += 1,
            Some("conventions") => conventions += 1,
            Some("runbooks") => runbooks += 1,
            Some("kb") => kb += 1,
            _ => {}
        }
    }
    (pinned, conventions, runbooks, kb)
}

pub fn build_project_memory_overview(project_drbot_dir: &Path) -> String {
    if !project_drbot_dir.is_dir() {
        return "Project memory (.drbot) is unavailable.".to_string();
    }

    let (pinned, conventions, runbooks, kb_links) = read_to_string_if_file(&memory_md_path(project_drbot_dir))
        .map(|doc| count_project_memory_bullets_by_section(&doc))
        .unwrap_or((0, 0, 0, 0));

    let memory_dir = project_drbot_dir.join("memory");
    let mut memory_files: Vec<String> = Vec::new();
    let mut auto_notes = 0usize;
    if memory_dir.is_dir() {
        for path in collect_markdown_files(&memory_dir, 200) {
            let rel = normalize_rel_path(project_drbot_dir, &path);
            if rel.eq_ignore_ascii_case("memory/README.md") {
                continue;
            }
            if rel.starts_with("memory/auto/") {
                auto_notes += 1;
            }
            memory_files.push(rel);
        }
    }
    memory_files.sort();

    let total_notes = memory_files.len();
    let mut out = String::new();
    out.push_str("Project memory (.drbot):\n");
    out.push_str(&format!(
        "- Project: {}\n",
        project_drbot_dir.display()
    ));
    out.push_str(&format!(
        "- MEMORY.md: Pinned={}, Conventions={}, Runbooks={}, Knowledge base links={}\n",
        pinned, conventions, runbooks, kb_links
    ));
    out.push_str(&format!(
        "- Notes: {} file(s) under memory/ (auto notes: {})\n",
        total_notes, auto_notes
    ));

    if !memory_files.is_empty() {
        out.push_str("- Files:\n");
        for rel in memory_files.iter().take(12) {
            out.push_str(&format!("  - {}\n", rel));
        }
        if total_notes > 12 {
            out.push_str(&format!("  - …and {} more\n", total_notes - 12));
        }
    }

    out.trim_end().to_string()
}

fn resolve_path_under_root(root: &Path, path: &str, must_exist: bool) -> Result<PathBuf> {
    let expanded = expand_tilde(path);
    let input = Path::new(expanded.as_str());

    if input.is_absolute() {
        let canon = if must_exist {
            input.canonicalize()
        } else {
            // For writes, canonicalize the parent directory.
            let parent = input
                .parent()
                .ok_or_else(|| anyhow::anyhow!("Invalid path: {}", input.display()))?;
            let parent_canon = parent.canonicalize().map_err(|e| {
                anyhow::anyhow!(
                    "Failed to resolve parent directory '{}': {}",
                    parent.display(),
                    e
                )
            })?;
            let name = input
                .file_name()
                .ok_or_else(|| anyhow::anyhow!("Invalid path: {}", input.display()))?;
            Ok(parent_canon.join(name))
        }?;

        if !canon.starts_with(root) {
            return Err(anyhow::anyhow!(
                "Path '{}' is outside tool root '{}'",
                canon.display(),
                root.display()
            ));
        }
        return Ok(canon);
    }

    let joined = root.join(input);
    if must_exist {
        let canon = joined
            .canonicalize()
            .map_err(|e| anyhow::anyhow!("Failed to resolve path '{}': {}", joined.display(), e))?;
        if !canon.starts_with(root) {
            return Err(anyhow::anyhow!(
                "Path '{}' is outside tool root '{}'",
                canon.display(),
                root.display()
            ));
        }
        return Ok(canon);
    }

    // For writes, canonicalize parent directory (must exist).
    let parent = joined
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Invalid path: {}", joined.display()))?;
    let parent_canon = parent.canonicalize().map_err(|e| {
        anyhow::anyhow!(
            "Failed to resolve parent directory '{}': {}",
            parent.display(),
            e
        )
    })?;
    if !parent_canon.starts_with(root) {
        return Err(anyhow::anyhow!(
            "Path '{}' is outside tool root '{}'",
            parent_canon.display(),
            root.display()
        ));
    }
    let name = joined
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("Invalid path: {}", joined.display()))?;
    Ok(parent_canon.join(name))
}

pub fn build_agent_system_prompt(base: Option<String>, tool_root: &Path) -> String {
    build_agent_system_prompt_with_policy(base, tool_root, &[], &[])
}

pub fn build_agent_system_prompt_with_policy(
    base: Option<String>,
    tool_root: &Path,
    allowlist: &[String],
    denylist: &[String],
) -> String {
    let mut s = base.unwrap_or_else(|| "You are drbot, a helpful AI assistant.".to_string());
    s.push_str("\n\n");
    s.push_str("You are operating in a terminal with access to local tools.\n");
    s.push_str("When you need to run a tool, respond ONLY with a fenced code block with language drbot_tool containing JSON.\n");
    s.push_str("The JSON must be either a single object or an array of objects in this form:\n");
    s.push_str("{\"tool\":\"bash\",\"args\":{\"command\":\"git status\"}}\n");
    s.push_str("\nAvailable tools:\n");

    let mut any = false;
    let mut push_tool = |name: &str, body: &str| {
        if !tool_allowed_by_lists(allowlist, denylist, name) {
            return;
        }
        any = true;
        s.push_str("- ");
        s.push_str(name);
        s.push_str(": ");
        s.push_str(body);
        s.push_str("\n");
    };

    push_tool("bash", "Run a shell command. args: { \"command\": string, \"cwd\"?: string } (cwd must be under the tool root.)");
    push_tool("read_file", "Read a UTF-8 text file under the tool root. args: { \"path\": string }");
    push_tool("write_file", "Write/replace a UTF-8 text file under the tool root. args: { \"path\": string, \"content\": string }");
    push_tool("list_dir", "List a directory under the tool root. args: { \"path\": string }");
    push_tool("list_directory", "Alias of list_dir. args: { \"path\": string }");
    push_tool("search", "Search for a pattern under a path (uses ripgrep when available). args: { \"pattern\": string, \"path\": string }");
    push_tool("apply_patch", "Apply a unified diff patch to files under the tool root. args: { \"patch\": string }");

    if !any {
        s.push_str("- (none) No tools are enabled by policy.\n");
    }

    if !allowlist.is_empty() || !denylist.is_empty() {
        s.push_str("\nPolicy:\n");
        if !allowlist.is_empty() {
            s.push_str("- Allowlist: ");
            s.push_str(&allowlist.join(", "));
            s.push_str("\n");
        }
        if !denylist.is_empty() {
            s.push_str("- Denylist: ");
            s.push_str(&denylist.join(", "));
            s.push_str("\n");
        }
        s.push_str("- Only call tools listed above.\n");
    }

    s.push_str("\nRules:\n");
    s.push_str("- In tool mode, you are an autonomous coding agent: when the user asks you to create/modify/run something, do it with tools (don't ask the user to run commands).\n");
    s.push_str("- Use relative paths unless absolutely necessary.\n");
    s.push_str("- Each bash tool call does NOT preserve state between calls (including cd). Prefer args.cwd instead of `cd ... && ...`.\n");
    s.push_str("- Prefer safe, read-only commands (git status/diff, rg, cargo test, etc.).\n");
    s.push_str("- After a tool runs, you will receive a message starting with [Tool Result] or [Tool Denied]. Use it to continue.\n");
    s.push_str(&format!("\nTool root: {}\n", tool_root.display()));
    s
}

pub fn should_reprompt_for_tool_calls(user_text: &str, assistant_text: &str) -> bool {
    fn contains_any(haystack: &str, needles: &[&str]) -> bool {
        needles.iter().any(|n| haystack.contains(n))
    }

    let user = user_text.to_ascii_lowercase();
    let assistant = assistant_text.to_ascii_lowercase();

    // If the user is asking for actions/edits/runs, we strongly prefer tool calls.
    let user_intends_actions = contains_any(
        &user,
        &[
            "create",
            "scaffold",
            "build",
            "install",
            "run",
            "execute",
            "fix",
            "update",
            "edit",
            "write",
            "implement",
            "refactor",
            "add ",
            "remove",
            "generate",
            "test",
            "compile",
            "lint",
            "format",
            "apply",
            "patch",
        ],
    );

    // If the assistant is outputting command-like content without tool calls, reprompt.
    let assistant_looks_actionable = contains_any(
        &assistant,
        &[
            "```", // code fences (often `bash` without tool JSON)
            "$ ", "cd ", "pnpm ", "npm ", "npx ", "node ", "cargo ", "git ", "rg ", "cat <<",
        ],
    );

    user_intends_actions || assistant_looks_actionable
}

pub fn extract_tool_calls(text: &str) -> Vec<ToolCallSpec> {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum BlockKind {
        ToolJson,
        BashCommand,
    }

    fn parse_fence_lang(trimmed: &str) -> Option<String> {
        if !trimmed.starts_with("```") {
            return None;
        }
        Some(
            trimmed
                .trim_start_matches("```")
                .split_whitespace()
                .next()
                .unwrap_or("")
                .trim()
                .to_ascii_lowercase(),
        )
    }

    fn normalize_bash_block(block: String) -> String {
        // Strip common prompt prefixes (`$ `) to improve execution success.
        let mut out = String::new();
        for line in block.lines() {
            let l = line.trim_end();
            let l = l.strip_prefix("$ ").unwrap_or(l);
            out.push_str(l);
            out.push('\n');
        }
        out.trim().to_string()
    }

    let mut calls: Vec<ToolCallSpec> = Vec::new();
    let mut in_block = false;
    let mut block = String::new();
    let mut kind: Option<BlockKind> = None;

    for line in text.lines() {
        let trimmed = line.trim();
        if !in_block {
            let Some(lang) = parse_fence_lang(trimmed) else {
                continue;
            };
            let block_kind = match lang.as_str() {
                "drbot_tool" | "json" => Some(BlockKind::ToolJson),
                "bash" | "sh" | "shell" | "zsh" => Some(BlockKind::BashCommand),
                _ => None,
            };
            if let Some(bk) = block_kind {
                in_block = true;
                kind = Some(bk);
                block.clear();
            }
            continue;
        }

        if trimmed.starts_with("```") {
            // End block -> parse
            match kind {
                Some(BlockKind::ToolJson) => {
                    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&block) {
                        match value {
                            serde_json::Value::Array(items) => {
                                for item in items {
                                    if let Some(call) = parse_tool_call_value(&item) {
                                        calls.push(call);
                                    }
                                }
                            }
                            other => {
                                if let Some(call) = parse_tool_call_value(&other) {
                                    calls.push(call);
                                }
                            }
                        }
                    }
                }
                Some(BlockKind::BashCommand) => {
                    let command = normalize_bash_block(std::mem::take(&mut block));
                    if !command.is_empty() {
                        calls.push(ToolCallSpec {
                            tool: "bash".to_string(),
                            args: serde_json::json!({ "command": command }),
                        });
                    }
                }
                None => {}
            }

            in_block = false;
            kind = None;
            block.clear();
            continue;
        }

        block.push_str(line);
        block.push('\n');
    }

    if !calls.is_empty() {
        return calls;
    }

    // Allow lightweight patterns that local models often emit when "tool mode" prompts are ignored.
    // Example: `bash: cd app && pnpm test`
    for line in text.lines() {
        let trimmed = line.trim();
        let lowered = trimmed.to_ascii_lowercase();
        if let Some(rest) = lowered.strip_prefix("bash:") {
            let cmd = trimmed[trimmed.len() - rest.len()..].trim();
            if !cmd.is_empty() {
                return vec![ToolCallSpec {
                    tool: "bash".to_string(),
                    args: serde_json::json!({ "command": cmd }),
                }];
            }
        }
    }

    // Fallback: some models ignore the requested code-fence language and emit a raw JSON object/array.
    // Extract the first JSON value that looks like a tool call.
    fn extract_json_value_bounds(
        s: &str,
        start: usize,
        open: char,
        close: char,
    ) -> Option<(usize, usize)> {
        let slice = s.get(start..)?;
        let mut depth: i64 = 0;
        let mut in_string = false;
        let mut escape = false;

        for (off, ch) in slice.char_indices() {
            if in_string {
                if escape {
                    escape = false;
                    continue;
                }
                match ch {
                    '\\' => escape = true,
                    '"' => in_string = false,
                    _ => {}
                }
                continue;
            }

            match ch {
                '"' => in_string = true,
                c if c == open => depth += 1,
                c if c == close => {
                    depth -= 1;
                    if depth == 0 {
                        return Some((start, start + off + ch.len_utf8()));
                    }
                }
                _ => {}
            }
        }

        None
    }

    let bytes = text.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        let ch = bytes[i] as char;
        let (open, close) = match ch {
            '{' => ('{', '}'),
            '[' => ('[', ']'),
            _ => {
                i += 1;
                continue;
            }
        };

        let Some((start, end)) = extract_json_value_bounds(text, i, open, close) else {
            i += 1;
            continue;
        };
        let json_str = &text[start..end];
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(json_str) {
            match value {
                serde_json::Value::Array(items) => {
                    for item in items {
                        if let Some(call) = parse_tool_call_value(&item) {
                            calls.push(call);
                        }
                    }
                }
                other => {
                    if let Some(call) = parse_tool_call_value(&other) {
                        calls.push(call);
                    }
                }
            }
            if !calls.is_empty() {
                break;
            }
        }

        i = end;
    }

    calls
}

fn parse_tool_call_value(value: &serde_json::Value) -> Option<ToolCallSpec> {
    const SUPPORTED_TOOLS: &[&str] = &[
        "bash",
        "read_file",
        "write_file",
        "list_dir",
        "list_directory",
        "search",
        "apply_patch",
    ];

    let tool = value.get("tool")?.as_str()?.to_string();
    if !SUPPORTED_TOOLS.iter().any(|t| *t == tool) {
        return None;
    }
    let args = value
        .get("args")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    Some(ToolCallSpec { tool, args })
}

#[derive(Debug, Clone, Default)]
pub struct BashAutoApprovePolicy {
    pub allow_all: bool,
    pub extra_prefixes: Vec<String>,
    pub override_prefixes: Option<Vec<String>>,
}

pub fn bash_command_is_safe_for_auto_approve(
    command: &str,
    policy: &BashAutoApprovePolicy,
) -> bool {
    const DEFAULT_ALLOWED_COMMANDS: &[&str] = &[
        "git", "cargo", "rg", "ls", "cat", "sed", "grep", "find", "head", "tail", "wc", "sort",
        "uniq", "pwd", "echo",
    ];
    const FORBIDDEN_COMMANDS: &[&str] = &["sudo", "rm", "mkfs", "dd", "shutdown", "reboot"];

    let cmd = command.trim();
    if cmd.is_empty() {
        return false;
    }

    fn split_shell_sequence(input: &str) -> Vec<String> {
        fn push(out: &mut Vec<String>, buf: &mut String) {
            let trimmed = buf.trim();
            if !trimmed.is_empty() {
                out.push(trimmed.to_string());
            }
            buf.clear();
        }

        let mut out: Vec<String> = Vec::new();
        let mut buf = String::new();
        let mut in_single = false;
        let mut in_double = false;
        let mut escape = false;

        let chars: Vec<char> = input.chars().collect();
        let mut i = 0usize;
        while i < chars.len() {
            let ch = chars[i];
            if escape {
                buf.push(ch);
                escape = false;
                i += 1;
                continue;
            }

            if ch == '\\' && !in_single {
                escape = true;
                buf.push(ch);
                i += 1;
                continue;
            }

            if ch == '\'' && !in_double {
                in_single = !in_single;
                buf.push(ch);
                i += 1;
                continue;
            }
            if ch == '"' && !in_single {
                in_double = !in_double;
                buf.push(ch);
                i += 1;
                continue;
            }

            if !in_single && !in_double {
                if ch == '&' && i + 1 < chars.len() && chars[i + 1] == '&' {
                    push(&mut out, &mut buf);
                    i += 2;
                    continue;
                }
                if ch == '|' && i + 1 < chars.len() && chars[i + 1] == '|' {
                    push(&mut out, &mut buf);
                    i += 2;
                    continue;
                }
                if ch == ';' || ch == '\n' {
                    push(&mut out, &mut buf);
                    i += 1;
                    continue;
                }
            }

            buf.push(ch);
            i += 1;
        }

        push(&mut out, &mut buf);
        out
    }

    fn split_shell_pipeline(input: &str) -> Vec<String> {
        fn push(out: &mut Vec<String>, buf: &mut String) {
            let trimmed = buf.trim();
            if !trimmed.is_empty() {
                out.push(trimmed.to_string());
            }
            buf.clear();
        }

        let mut out: Vec<String> = Vec::new();
        let mut buf = String::new();
        let mut in_single = false;
        let mut in_double = false;
        let mut escape = false;
        let chars: Vec<char> = input.chars().collect();
        let mut i = 0usize;
        while i < chars.len() {
            let ch = chars[i];
            if escape {
                buf.push(ch);
                escape = false;
                i += 1;
                continue;
            }
            if ch == '\\' && !in_single {
                escape = true;
                buf.push(ch);
                i += 1;
                continue;
            }
            if ch == '\'' && !in_double {
                in_single = !in_single;
                buf.push(ch);
                i += 1;
                continue;
            }
            if ch == '"' && !in_single {
                in_double = !in_double;
                buf.push(ch);
                i += 1;
                continue;
            }

            if !in_single && !in_double && ch == '|' {
                // `|` or `|&`
                push(&mut out, &mut buf);
                if i + 1 < chars.len() && chars[i + 1] == '&' {
                    i += 2;
                } else {
                    i += 1;
                }
                continue;
            }

            buf.push(ch);
            i += 1;
        }
        push(&mut out, &mut buf);
        out
    }

    fn shell_words(input: &str) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let mut buf = String::new();
        let mut in_single = false;
        let mut in_double = false;
        let mut escape = false;

        for ch in input.chars() {
            if escape {
                buf.push(ch);
                escape = false;
                continue;
            }
            if ch == '\\' && !in_single {
                escape = true;
                continue;
            }
            if ch == '\'' && !in_double {
                in_single = !in_single;
                continue;
            }
            if ch == '"' && !in_single {
                in_double = !in_double;
                continue;
            }
            if !in_single && !in_double && ch.is_whitespace() {
                if !buf.is_empty() {
                    out.push(std::mem::take(&mut buf));
                }
                continue;
            }
            buf.push(ch);
        }
        if !buf.is_empty() {
            out.push(buf);
        }
        out
    }

    fn contains_redirection_outside_quotes(input: &str) -> bool {
        let mut in_single = false;
        let mut in_double = false;
        let mut escape = false;
        for ch in input.chars() {
            if escape {
                escape = false;
                continue;
            }
            if ch == '\\' && !in_single {
                escape = true;
                continue;
            }
            if ch == '\'' && !in_double {
                in_single = !in_single;
                continue;
            }
            if ch == '"' && !in_single {
                in_double = !in_double;
                continue;
            }
            if !in_single && !in_double && matches!(ch, '<' | '>') {
                return true;
            }
        }
        false
    }

    fn contains_background_outside_quotes(input: &str) -> bool {
        let mut in_single = false;
        let mut in_double = false;
        let mut escape = false;
        let chars: Vec<char> = input.chars().collect();
        for (i, ch) in chars.iter().copied().enumerate() {
            if escape {
                escape = false;
                continue;
            }
            if ch == '\\' && !in_single {
                escape = true;
                continue;
            }
            if ch == '\'' && !in_double {
                in_single = !in_single;
                continue;
            }
            if ch == '"' && !in_single {
                in_double = !in_double;
                continue;
            }
            if in_single || in_double {
                continue;
            }
            if ch != '&' {
                continue;
            }
            // Skip `&&` (already split, but be defensive).
            if i + 1 < chars.len() && chars[i + 1] == '&' {
                continue;
            }
            if i > 0 && chars[i - 1] == '&' {
                continue;
            }

            let prev_ws = i == 0 || chars[i - 1].is_whitespace();
            let next_ws = i + 1 == chars.len() || chars[i + 1].is_whitespace();
            if prev_ws || next_ws {
                return true;
            }
        }
        false
    }

    fn contains_command_subst_outside_single_quotes(input: &str) -> bool {
        let mut in_single = false;
        let mut in_double = false;
        let mut escape = false;
        let chars: Vec<char> = input.chars().collect();
        let mut i = 0usize;
        while i < chars.len() {
            let ch = chars[i];
            if escape {
                escape = false;
                i += 1;
                continue;
            }
            if ch == '\\' && !in_single {
                escape = true;
                i += 1;
                continue;
            }
            if ch == '\'' && !in_double {
                in_single = !in_single;
                i += 1;
                continue;
            }
            if ch == '"' && !in_single {
                in_double = !in_double;
                i += 1;
                continue;
            }

            if !in_single {
                if ch == '`' {
                    return true;
                }
                if ch == '$' && i + 1 < chars.len() && chars[i + 1] == '(' {
                    return true;
                }
            }

            i += 1;
        }
        false
    }

    fn normalize_command_name(token: &str) -> String {
        let token = token.trim();
        let token = token.trim_start_matches('\\');
        let base = token.rsplit('/').next().unwrap_or(token);
        base.to_ascii_lowercase()
    }

    fn git_is_safe(tokens: &[String]) -> bool {
        // Disallow `git` without a subcommand.
        if tokens.len() < 2 {
            return false;
        }

        // Allow only a minimal subset of global options (to avoid `-c alias...` trickery).
        let mut i = 1usize;
        while i < tokens.len() {
            let t = tokens[i].as_str();
            if t == "--" {
                i += 1;
                break;
            }
            if t == "-C" {
                if i + 1 >= tokens.len() {
                    return false;
                }
                i += 2;
                continue;
            }
            if t.starts_with('-') {
                return false;
            }
            break;
        }
        if i >= tokens.len() {
            return false;
        }

        let sub = tokens[i].to_ascii_lowercase();
        let allowed = [
            "status",
            "diff",
            "log",
            "show",
            "rev-parse",
            "ls-files",
            "grep",
            "describe",
            "branch",
        ];
        if !allowed.contains(&sub.as_str()) {
            return false;
        }

        if sub == "branch" {
            // `git branch` is read-only by default, but can delete/move/copy branches.
            let mut dangerous = false;
            for arg in tokens.iter().skip(i + 1) {
                let a = arg.as_str();
                if matches!(
                    a,
                    "-d" | "-D" | "--delete" | "-m" | "-M" | "--move" | "-c" | "-C" | "--copy"
                ) {
                    dangerous = true;
                    break;
                }
            }
            if dangerous {
                return false;
            }
        }

        true
    }

    fn cargo_is_safe(tokens: &[String]) -> bool {
        if tokens.len() < 2 {
            return false;
        }
        let mut i = 1usize;
        while i < tokens.len() {
            let t = tokens[i].as_str();
            if t.starts_with('+') {
                i += 1;
                continue;
            }
            if t.starts_with('-') {
                i += 1;
                continue;
            }
            break;
        }
        if i >= tokens.len() {
            return false;
        }
        let sub = tokens[i].to_ascii_lowercase();
        let allowed = ["test", "check", "build", "metadata", "clippy"];
        allowed.contains(&sub.as_str())
    }

    fn sed_is_safe(tokens: &[String]) -> bool {
        for arg in tokens.iter().skip(1) {
            let a = arg.as_str();
            if a == "--in-place" || a == "-i" || a.starts_with("-i") {
                return false;
            }
        }
        true
    }

    fn find_is_safe(tokens: &[String]) -> bool {
        for arg in tokens.iter().skip(1) {
            let a = arg.as_str();
            if matches!(a, "-delete" | "-exec" | "-execdir" | "-ok" | "-okdir") {
                return false;
            }
        }
        true
    }

    let mut allowed: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    if let Some(list) = &policy.override_prefixes {
        for p in list {
            let p = p.trim();
            if p.is_empty() {
                continue;
            }
            allowed.insert(p.to_ascii_lowercase());
        }
    } else {
        for p in DEFAULT_ALLOWED_COMMANDS {
            allowed.insert((*p).to_string());
        }
        for p in &policy.extra_prefixes {
            let p = p.trim();
            if p.is_empty() {
                continue;
            }
            allowed.insert(p.to_ascii_lowercase());
        }
    }

    let segments = split_shell_sequence(cmd);
    if segments.is_empty() {
        return false;
    }

    // First: always block clearly destructive commands, even if allow_all is enabled.
    for segment in &segments {
        for part in split_shell_pipeline(segment) {
            let words = shell_words(&part);
            let Some(first) = words.first() else {
                continue;
            };
            let base = normalize_command_name(first);
            if FORBIDDEN_COMMANDS.contains(&base.as_str()) {
                return false;
            }
        }
    }

    if policy.allow_all {
        return true;
    }

    for segment in &segments {
        for part in split_shell_pipeline(segment) {
            if contains_redirection_outside_quotes(&part) {
                return false;
            }
            if contains_background_outside_quotes(&part) {
                return false;
            }
            if contains_command_subst_outside_single_quotes(&part) {
                return false;
            }

            let words = shell_words(&part);
            let Some(first) = words.first() else {
                continue;
            };
            let base = normalize_command_name(first);
            if !allowed.contains(&base) {
                return false;
            }

            // Command-specific extra safety for built-ins that often mutate state.
            match base.as_str() {
                "git" => {
                    if !git_is_safe(&words) {
                        return false;
                    }
                }
                "cargo" => {
                    if !cargo_is_safe(&words) {
                        return false;
                    }
                }
                "sed" => {
                    if !sed_is_safe(&words) {
                        return false;
                    }
                }
                "find" => {
                    if !find_is_safe(&words) {
                        return false;
                    }
                }
                _ => {}
            }
        }
    }

    true
}

fn truncate_for_context(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let truncated: String = text.chars().take(max_chars).collect();
    format!("{}...\n[truncated]", truncated)
}

async fn maybe_spool_tool_output(
    root: &Path,
    tool: &str,
    output: &str,
    max_chars: usize,
) -> Result<String> {
    let char_count = output.chars().count();
    if char_count <= max_chars {
        return Ok(output.to_string());
    }

    let dir = root.join(".drbot").join("tool-output");
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| anyhow::anyhow!("failed to create tool-output dir: {}", e))?;

    let mut slug = tool
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>();
    slug = slug.trim_matches('-').to_string();
    if slug.is_empty() {
        slug = "tool".to_string();
    }

    let path = dir.join(format!("{}-{}.txt", slug, uuid::Uuid::new_v4()));
    tokio::fs::write(&path, output.as_bytes())
        .await
        .map_err(|e| anyhow::anyhow!("failed to write tool output: {}", e))?;

    let rel = path
        .strip_prefix(root)
        .ok()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string());
    let truncated = truncate_for_context(output, max_chars);

    Ok(format!(
        "[output truncated: {} chars; full output saved to {}]\n{}",
        char_count, rel, truncated
    ))
}

async fn run_bash_tool(root: &Path, cwd: &Path, command: &str) -> Result<(String, bool)> {
    use tokio::process::Command;

    let output = tokio::time::timeout(
        std::time::Duration::from_secs(300),
        Command::new("bash")
            .arg("-lc")
            .arg(command)
            .current_dir(cwd)
            .output(),
    )
    .await
    .map_err(|_| anyhow::anyhow!("bash tool timed out"))?
    .map_err(|e| anyhow::anyhow!("Failed to run bash tool: {}", e))?;

    let code = output.status.code().unwrap_or(-1);
    let is_error = code != 0;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    let mut out = String::new();
    out.push_str(&format!("exit_code: {}\n", code));
    if !stdout.trim().is_empty() {
        out.push_str("stdout:\n");
        out.push_str(&stdout);
        if !stdout.ends_with('\n') {
            out.push('\n');
        }
    }
    if !stderr.trim().is_empty() {
        out.push_str("stderr:\n");
        out.push_str(&stderr);
        if !stderr.ends_with('\n') {
            out.push('\n');
        }
    }

    let rendered = out.trim_end().to_string();
    let rendered = maybe_spool_tool_output(root, "bash", &rendered, 40_000).await?;
    Ok((rendered, is_error))
}

async fn run_read_file_tool(root: &Path, path: &str) -> Result<String> {
    let file = resolve_path_under_root(root, path, true)?;
    let bytes = tokio::fs::read(&file)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to read '{}': {}", file.display(), e))?;
    let text = String::from_utf8_lossy(&bytes).to_string();
    Ok(truncate_for_context(&text, 120_000))
}

async fn run_write_file_tool(root: &Path, path: &str, content: &str) -> Result<String> {
    let file = resolve_path_under_root(root, path, false)?;
    tokio::fs::write(&file, content)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to write '{}': {}", file.display(), e))?;
    Ok(format!(
        "Wrote {} bytes to {}",
        content.len(),
        file.display()
    ))
}

async fn run_list_dir_tool(root: &Path, path: &str) -> Result<String> {
    let dir = resolve_path_under_root(root, path, true)?;
    let mut entries = tokio::fs::read_dir(&dir)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to read dir '{}': {}", dir.display(), e))?;
    let mut items = Vec::new();
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to read dir entry: {}", e))?
    {
        let meta = entry.metadata().await.ok();
        let suffix = if meta.map(|m| m.is_dir()).unwrap_or(false) {
            "/"
        } else {
            ""
        };
        items.push(format!("{}{}", entry.file_name().to_string_lossy(), suffix));
    }
    items.sort();
    let rendered = items.join("\n");
    let rendered = maybe_spool_tool_output(root, "list_dir", &rendered, 40_000).await?;
    Ok(rendered)
}

async fn run_search_tool(root: &Path, pattern: &str, path: &str) -> Result<(String, bool)> {
    use tokio::process::Command;

    let target = resolve_path_under_root(root, path, true)?;

    // Prefer ripgrep; fall back to grep.
    let rg = Command::new("bash")
        .arg("-lc")
        .arg("command -v rg >/dev/null 2>&1")
        .current_dir(root)
        .output()
        .await;

    let (cmd, args): (&str, Vec<String>) = match rg {
        Ok(out) if out.status.success() => (
            "rg",
            vec![
                "-n".to_string(),
                "--hidden".to_string(),
                "--no-heading".to_string(),
                pattern.to_string(),
                target.to_string_lossy().to_string(),
            ],
        ),
        _ => (
            "grep",
            vec![
                "-R".to_string(),
                "-n".to_string(),
                pattern.to_string(),
                target.to_string_lossy().to_string(),
            ],
        ),
    };

    let output = tokio::time::timeout(
        std::time::Duration::from_secs(120),
        Command::new(cmd).args(&args).current_dir(root).output(),
    )
    .await
    .map_err(|_| anyhow::anyhow!("search tool timed out"))?
    .map_err(|e| anyhow::anyhow!("Failed to run search tool: {}", e))?;

    let code = output.status.code().unwrap_or(-1);
    let is_error = code != 0 && code != 1; // grep/rg use 1 for "no matches"
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    let mut out = String::new();
    out.push_str(&format!("exit_code: {}\n", code));
    if !stdout.trim().is_empty() {
        out.push_str("stdout:\n");
        out.push_str(&stdout);
        if !stdout.ends_with('\n') {
            out.push('\n');
        }
    }
    if !stderr.trim().is_empty() {
        out.push_str("stderr:\n");
        out.push_str(&stderr);
        if !stderr.ends_with('\n') {
            out.push('\n');
        }
    }

    let rendered = out.trim_end().to_string();
    let rendered = maybe_spool_tool_output(root, "search", &rendered, 40_000).await?;
    Ok((rendered, is_error))
}

#[derive(Debug, Clone)]
struct UnifiedDiffHunk {
    old_start: usize,
    old_count: usize,
    new_start: usize,
    new_count: usize,
    lines: Vec<(char, String)>,
}

#[derive(Debug, Clone)]
struct UnifiedDiffFile {
    old_path: String,
    new_path: String,
    hunks: Vec<UnifiedDiffHunk>,
}

fn strip_unified_diff_path(raw: &str) -> String {
    let token = raw.trim().trim_matches('"');
    if token == "/dev/null" {
        return token.to_string();
    }
    token
        .strip_prefix("a/")
        .or_else(|| token.strip_prefix("b/"))
        .unwrap_or(token)
        .to_string()
}

fn parse_unified_diff_hunk_header(line: &str) -> Result<(usize, usize, usize, usize)> {
    let trimmed = line.trim();
    if !trimmed.starts_with("@@") {
        return Err(anyhow::anyhow!("invalid hunk header: {}", line));
    }
    let Some(end) = trimmed[2..].find("@@").map(|i| i + 2) else {
        return Err(anyhow::anyhow!("invalid hunk header: {}", line));
    };
    let body = trimmed[2..end].trim();
    let mut parts = body.split_whitespace();
    let old = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("invalid hunk header: {}", line))?;
    let new = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("invalid hunk header: {}", line))?;

    fn parse_range(token: &str, sigil: char) -> Result<(usize, usize)> {
        let t = token
            .strip_prefix(sigil)
            .ok_or_else(|| anyhow::anyhow!("invalid hunk range: {}", token))?;
        let mut it = t.split(',');
        let start = it
            .next()
            .unwrap_or("")
            .parse::<usize>()
            .map_err(|_| anyhow::anyhow!("invalid hunk start: {}", token))?;
        let count = it
            .next()
            .unwrap_or("1")
            .parse::<usize>()
            .map_err(|_| anyhow::anyhow!("invalid hunk count: {}", token))?;
        Ok((start, count))
    }

    let (old_start, old_count) = parse_range(old, '-')?;
    let (new_start, new_count) = parse_range(new, '+')?;
    Ok((old_start, old_count, new_start, new_count))
}

fn parse_unified_diff(patch: &str) -> Result<Vec<UnifiedDiffFile>> {
    let mut files: Vec<UnifiedDiffFile> = Vec::new();
    let mut cur: Option<UnifiedDiffFile> = None;
    let mut cur_hunk: Option<UnifiedDiffHunk> = None;

    for line in patch.lines() {
        if line.starts_with("diff ") || line.starts_with("index ") {
            continue;
        }
        if let Some(path) = line.strip_prefix("--- ") {
            if let Some(hunk) = cur_hunk.take() {
                if let Some(file) = cur.as_mut() {
                    file.hunks.push(hunk);
                }
            }
            if let Some(file) = cur.take() {
                files.push(file);
            }
            cur = Some(UnifiedDiffFile {
                old_path: strip_unified_diff_path(path),
                new_path: String::new(),
                hunks: Vec::new(),
            });
            continue;
        }
        if let Some(path) = line.strip_prefix("+++ ") {
            if let Some(file) = cur.as_mut() {
                file.new_path = strip_unified_diff_path(path);
            }
            continue;
        }
        if line.starts_with("@@") {
            if let Some(hunk) = cur_hunk.take() {
                if let Some(file) = cur.as_mut() {
                    file.hunks.push(hunk);
                }
            }
            let (old_start, old_count, new_start, new_count) =
                parse_unified_diff_hunk_header(line)?;
            cur_hunk = Some(UnifiedDiffHunk {
                old_start,
                old_count,
                new_start,
                new_count,
                lines: Vec::new(),
            });
            continue;
        }
        if let Some(hunk) = cur_hunk.as_mut() {
            if line.starts_with('+') || line.starts_with('-') || line.starts_with(' ') {
                let kind = line.chars().next().unwrap_or(' ');
                let text = line.get(1..).unwrap_or("").to_string();
                hunk.lines.push((kind, text));
                continue;
            }
            if line.starts_with("\\ No newline at end of file") {
                continue;
            }
        }
    }

    if let Some(hunk) = cur_hunk.take() {
        if let Some(file) = cur.as_mut() {
            file.hunks.push(hunk);
        }
    }
    if let Some(file) = cur.take() {
        files.push(file);
    }

    Ok(files)
}

fn apply_unified_diff_to_text(original: &str, hunks: &[UnifiedDiffHunk]) -> Result<String> {
    let had_trailing_newline = original.ends_with('\n');
    let orig_lines = original.lines().map(|s| s.to_string()).collect::<Vec<_>>();
    let mut out: Vec<String> = Vec::new();
    let mut orig_idx = 0usize;

    for hunk in hunks {
        let target = hunk.old_start.saturating_sub(1);
        if target > orig_lines.len() {
            return Err(anyhow::anyhow!(
                "hunk old_start out of range: {}",
                hunk.old_start
            ));
        }
        if target < orig_idx {
            return Err(anyhow::anyhow!(
                "hunk overlaps previous hunks at -{}",
                hunk.old_start
            ));
        }

        out.extend_from_slice(&orig_lines[orig_idx..target]);

        if out.len() + 1 != hunk.new_start {
            return Err(anyhow::anyhow!(
                "hunk new_start mismatch (expected new line {}, got {})",
                hunk.new_start,
                out.len() + 1
            ));
        }
        let out_start_len = out.len();
        let mut pos = target;
        for (kind, text) in &hunk.lines {
            match *kind {
                ' ' => {
                    let cur = orig_lines.get(pos).ok_or_else(|| {
                        anyhow::anyhow!("context past end of file at line {}", pos + 1)
                    })?;
                    if cur != text {
                        return Err(anyhow::anyhow!(
                            "context mismatch at line {} (expected {:?}, found {:?})",
                            pos + 1,
                            text,
                            cur
                        ));
                    }
                    out.push(text.clone());
                    pos += 1;
                }
                '-' => {
                    let cur = orig_lines.get(pos).ok_or_else(|| {
                        anyhow::anyhow!("remove past end of file at line {}", pos + 1)
                    })?;
                    if cur != text {
                        return Err(anyhow::anyhow!(
                            "remove mismatch at line {} (expected {:?}, found {:?})",
                            pos + 1,
                            text,
                            cur
                        ));
                    }
                    pos += 1;
                }
                '+' => {
                    out.push(text.clone());
                }
                other => {
                    return Err(anyhow::anyhow!("unknown hunk line kind: {}", other));
                }
            }
        }

        let consumed_old = pos.saturating_sub(target);
        if consumed_old != hunk.old_count {
            return Err(anyhow::anyhow!(
                "hunk old_count mismatch at -{} (expected {}, got {})",
                hunk.old_start,
                hunk.old_count,
                consumed_old
            ));
        }
        let produced_new = out.len().saturating_sub(out_start_len);
        if produced_new != hunk.new_count {
            return Err(anyhow::anyhow!(
                "hunk new_count mismatch at +{} (expected {}, got {})",
                hunk.new_start,
                hunk.new_count,
                produced_new
            ));
        }
        orig_idx = pos;
    }

    out.extend_from_slice(&orig_lines[orig_idx..]);

    let mut rendered = out.join("\n");
    if had_trailing_newline {
        rendered.push('\n');
    }
    Ok(rendered)
}

async fn run_apply_patch_tool(root: &Path, patch: &str) -> Result<String> {
    let files = parse_unified_diff(patch)?;
    if files.is_empty() {
        return Err(anyhow::anyhow!("apply_patch: no file patches found"));
    }

    let mut out_lines: Vec<String> = Vec::new();
    for fp in files {
        let old_path = fp.old_path.clone();
        let new_path = fp.new_path.clone();

        if old_path != "/dev/null" && new_path != "/dev/null" && old_path != new_path {
            return Err(anyhow::anyhow!(
                "apply_patch: renames are not supported ({} -> {})",
                old_path,
                new_path
            ));
        }

        if new_path == "/dev/null" {
            let file = resolve_path_under_root(root, &old_path, true)?;
            tokio::fs::remove_file(&file)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to delete '{}': {}", file.display(), e))?;
            out_lines.push(format!("deleted {}", old_path));
            continue;
        }

        let target_path = new_path.clone();
        let original = if old_path == "/dev/null" {
            String::new()
        } else {
            let file = resolve_path_under_root(root, &target_path, true)?;
            let bytes = tokio::fs::read(&file)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to read '{}': {}", file.display(), e))?;
            String::from_utf8_lossy(&bytes).to_string()
        };

        let mut updated = apply_unified_diff_to_text(&original, &fp.hunks)?;
        if old_path == "/dev/null" && !updated.is_empty() && !updated.ends_with('\n') {
            updated.push('\n');
        }
        let _ = run_write_file_tool(root, &target_path, &updated).await?;
        out_lines.push(format!("patched {}", target_path));
    }

    Ok(out_lines.join("\n"))
}

pub async fn execute_tool_call(
    tool_cfg: &ToolModeConfig,
    call: &ToolCallSpec,
) -> Result<(String, bool)> {
    if !tool_allowed_by_policy(tool_cfg, &call.tool) {
        return Err(anyhow::anyhow!(
            "Tool '{}' is blocked by tool policy",
            call.tool
        ));
    }
    if matches!(tool_cfg.autonomy_mode, AutonomyMode::ReadOnly) && !is_readonly_tool(&call.tool) {
        return Err(anyhow::anyhow!(
            "Tool '{}' is blocked by autonomy_mode=read-only",
            call.tool
        ));
    }
    match call.tool.as_str() {
        "bash" => {
            let command = call
                .args
                .get("command")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("bash tool requires args.command"))?;
            let cwd = call
                .args
                .get("cwd")
                .and_then(|v| v.as_str())
                .map(|s| s.trim())
                .filter(|s| !s.is_empty());
            let cwd_path = if let Some(cwd) = cwd {
                let dir = resolve_path_under_root(&tool_cfg.root, cwd, true)?;
                if !dir.is_dir() {
                    return Err(anyhow::anyhow!("bash.cwd is not a directory: {}", cwd));
                }
                dir
            } else {
                tool_cfg.root.clone()
            };
            let (output, is_error) = run_bash_tool(&tool_cfg.root, &cwd_path, command).await?;
            Ok((output, is_error))
        }
        "read_file" => {
            let path = call
                .args
                .get("path")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("read_file tool requires args.path"))?;
            let output = run_read_file_tool(&tool_cfg.root, path).await?;
            Ok((output, false))
        }
        "write_file" => {
            let path = call
                .args
                .get("path")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("write_file tool requires args.path"))?;
            let content = call
                .args
                .get("content")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("write_file tool requires args.content"))?;
            let output = run_write_file_tool(&tool_cfg.root, path, content).await?;
            Ok((output, false))
        }
        "list_dir" | "list_directory" => {
            let path = call
                .args
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or(".");
            let output = run_list_dir_tool(&tool_cfg.root, path).await?;
            Ok((output, false))
        }
        "search" => {
            let pattern = call
                .args
                .get("pattern")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("search tool requires args.pattern"))?;
            let path = call
                .args
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or(".");
            let (output, is_error) = run_search_tool(&tool_cfg.root, pattern, path).await?;
            Ok((output, is_error))
        }
        "apply_patch" => {
            let patch = call
                .args
                .get("patch")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("apply_patch tool requires args.patch"))?;
            let output = run_apply_patch_tool(&tool_cfg.root, patch).await?;
            Ok((output, false))
        }
        other => Err(anyhow::anyhow!("Unknown tool: {}", other)),
    }
}

fn is_readonly_tool(tool: &str) -> bool {
    matches!(tool, "read_file" | "list_dir" | "list_directory" | "search")
}

fn tool_allowed_by_policy(cfg: &ToolModeConfig, tool: &str) -> bool {
    tool_allowed_by_lists(&cfg.tool_allowlist, &cfg.tool_denylist, tool)
}

fn tool_allowed_by_lists(allowlist: &[String], denylist: &[String], tool: &str) -> bool {
    let tool = tool.trim();
    if tool.is_empty() {
        return false;
    }
    if denylist.iter().any(|pat| tool_pattern_matches(tool, pat)) {
        return false;
    }
    if allowlist.is_empty() {
        return true;
    }
    allowlist.iter().any(|pat| tool_pattern_matches(tool, pat))
}

fn tool_pattern_matches(name: &str, raw_pattern: &str) -> bool {
    let name = name.trim();
    let pattern = raw_pattern.trim();
    if name.is_empty() || pattern.is_empty() {
        return false;
    }
    if pattern == "*" {
        return true;
    }
    if !pattern.contains('*') {
        return name == pattern;
    }

    let starts_with_wild = pattern.starts_with('*');
    let ends_with_wild = pattern.ends_with('*');
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.is_empty() {
        return false;
    }

    let mut pos = 0usize;
    let mut idx = 0usize;
    if !starts_with_wild {
        let first = parts[0];
        if !name.starts_with(first) {
            return false;
        }
        pos = first.len();
        idx = 1;
    }

    let last_idx = parts.len().saturating_sub(1);
    while idx < last_idx {
        let segment = parts[idx];
        idx += 1;
        if segment.is_empty() {
            continue;
        }
        let Some(found) = name[pos..].find(segment) else {
            return false;
        };
        pos = pos.saturating_add(found).saturating_add(segment.len());
    }

    let last = parts[last_idx];
    if last.is_empty() {
        return true;
    }
    if ends_with_wild {
        return name[pos..].contains(last);
    }
    if !name.ends_with(last) {
        return false;
    }
    let start = name.len().saturating_sub(last.len());
    start >= pos
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn extract_single_tool_call() {
        let text = r#"
```drbot_tool
{"tool":"bash","args":{"command":"git status"}}
```
"#;
        let calls = extract_tool_calls(text);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].tool, "bash");
        assert_eq!(
            calls[0].args.get("command").and_then(|v| v.as_str()),
            Some("git status")
        );
    }

    #[test]
    fn extract_multiple_tool_calls_array() {
        let text = r#"
```drbot_tool
[
  {"tool":"read_file","args":{"path":"src/main.rs"}},
  {"tool":"search","args":{"pattern":"run_chat","path":"src"}}
]
```
"#;
        let calls = extract_tool_calls(text);
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].tool, "read_file");
        assert_eq!(calls[1].tool, "search");
    }

    #[test]
    fn extract_bash_fence_as_tool_call() {
        let text = r#"
Here is the command:

```bash
cd app && pnpm test
```
"#;
        let calls = extract_tool_calls(text);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].tool, "bash");
        assert_eq!(
            calls[0].args.get("command").and_then(|v| v.as_str()),
            Some("cd app && pnpm test")
        );
    }

    #[test]
    fn extract_bash_colon_line_as_tool_call() {
        let text = "bash: cd app && pnpm build";
        let calls = extract_tool_calls(text);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].tool, "bash");
        assert_eq!(
            calls[0].args.get("command").and_then(|v| v.as_str()),
            Some("cd app && pnpm build")
        );
    }

    #[test]
    fn safe_bash_auto_approve() {
        let policy = BashAutoApprovePolicy::default();
        assert!(bash_command_is_safe_for_auto_approve("git status", &policy));
        assert!(bash_command_is_safe_for_auto_approve(
            "cargo test -q",
            &policy
        ));
        assert!(bash_command_is_safe_for_auto_approve(
            "rg \"foo|bar\" src",
            &policy
        ));
        assert!(bash_command_is_safe_for_auto_approve(
            "rg foo src | head -n 5",
            &policy
        ));
        assert!(!bash_command_is_safe_for_auto_approve(
            "rg foo src > out.txt",
            &policy
        ));
        assert!(!bash_command_is_safe_for_auto_approve(
            "git commit -m \"msg\"",
            &policy
        ));
        assert!(!bash_command_is_safe_for_auto_approve(
            "sed -i 's/a/b/' file.txt",
            &policy
        ));
        assert!(!bash_command_is_safe_for_auto_approve(
            "find . -delete",
            &policy
        ));
        assert!(!bash_command_is_safe_for_auto_approve(
            "echo $(rm -rf /)",
            &policy
        ));
        assert!(!bash_command_is_safe_for_auto_approve("rm -rf /", &policy));
        assert!(!bash_command_is_safe_for_auto_approve("sudo ls", &policy));
        assert!(!bash_command_is_safe_for_auto_approve(
            "dd if=/dev/zero of=/dev/null",
            &policy
        ));
        assert!(!bash_command_is_safe_for_auto_approve(
            "./script.sh",
            &policy
        ));
    }

    #[test]
    fn safe_bash_auto_approve_does_not_false_positive_on_add() {
        let policy = BashAutoApprovePolicy {
            allow_all: false,
            extra_prefixes: vec!["cd".to_string(), "pnpm".to_string()],
            override_prefixes: None,
        };
        assert!(bash_command_is_safe_for_auto_approve(
            "cd app && pnpm add @solana/client",
            &policy
        ));
    }

    #[test]
    fn parses_project_memory_commands() {
        assert!(is_project_remember_command("/remember project pinned: use postgres"));
        assert_eq!(
            parse_project_remember_note("/remember project pinned: use postgres"),
            Some("pinned: use postgres".to_string())
        );
        assert_eq!(
            parse_project_remember_note("remember project: pinned: use postgres"),
            Some("pinned: use postgres".to_string())
        );

        assert!(is_project_forget_command("/forget project: all"));
        assert_eq!(
            parse_project_forget_arg("/forget project: all"),
            Some("all".to_string())
        );

        assert!(is_project_remember_command("/remember project"));
        assert_eq!(parse_project_remember_note("/remember project"), None);
    }

    fn make_temp_dir() -> PathBuf {
        let base = std::env::temp_dir();
        let dir = base.join(format!(
            "drbot-tool-mode-test-{}",
            uuid::Uuid::new_v4().to_string().replace('-', "")
        ));
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn project_kb_remember_and_forget_round_trip() {
        let root = make_temp_dir();
        let project_drbot_dir = root.join(".drbot");

        let updates = remember_project_kb(&project_drbot_dir, "conventions: Use rustfmt")
            .expect("remember");
        assert!(updates.applied);

        let doc = fs::read_to_string(project_drbot_dir.join("MEMORY.md")).expect("read MEMORY.md");
        assert!(doc.contains("## Conventions"));
        assert!(doc.contains("- Use rustfmt"));
        assert!(project_drbot_dir.join(".gitignore").is_file());

        let long_note = format!("kb: {}", "A".repeat(300));
        let updates2 = remember_project_kb(&project_drbot_dir, &long_note).expect("remember long");
        assert!(updates2.applied);

        let doc2 = fs::read_to_string(project_drbot_dir.join("MEMORY.md")).expect("read MEMORY.md");
        let rel = doc2
            .lines()
            .find(|l| l.contains("memory/auto/note-") && l.contains(".md"))
            .and_then(|l| l.trim().strip_prefix("- "))
            .and_then(|l| l.split(':').next())
            .map(|s| s.trim().to_string())
            .expect("extract auto-note rel path");
        assert!(project_drbot_dir.join(&rel).is_file());

        let updates3 = forget_project_kb(&project_drbot_dir, "kb").expect("forget kb");
        assert!(updates3.applied);
        assert!(!project_drbot_dir.join(&rel).exists());

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn project_scoped_autocapture_extracts_notes() {
        assert_eq!(
            extract_project_scoped_note_for_autosave("In this repo, use pnpm (not yarn)."),
            Some("conventions: use pnpm (not yarn).".to_string())
        );
        assert_eq!(
            extract_project_scoped_note_for_autosave("For this project: run `pnpm dev` to start."),
            Some("runbooks: run `pnpm dev` to start.".to_string())
        );
        assert_eq!(
            extract_project_scoped_note_for_autosave("In this repo, we use Postgres."),
            Some("pinned: we use Postgres.".to_string())
        );
        assert_eq!(
            extract_project_scoped_note_for_autosave("In this repo, should we use pnpm?"),
            None
        );
        assert_eq!(
            extract_project_scoped_note_for_autosave("FYI: In this repo, conventions: use rustfmt."),
            Some("conventions: use rustfmt.".to_string())
        );
    }
}
