//! Best-effort workspace autosave for common persistent memory fields.
//!
//! This module updates OpenClaw-style workspace files (`USER.md`, `MEMORY.md`, `memory/*.md`)
//! when the user states stable facts/preferences (e.g. name, timezone, style) or explicitly
//! asks to remember something.
//!
//! Goals:
//! - Be conservative (avoid saving secrets / prompt-injection-y content).
//! - Never overwrite user-authored content except the specific field we are updating.
//! - Be best-effort: failures should not break chat flows.

use regex::Regex;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

#[derive(Debug, Default, Clone)]
pub struct AutosaveOutput {
    pub applied: bool,
    pub updates: Vec<String>,
}

fn autosave_enabled() -> bool {
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
    static TOKEN_RE: OnceLock<Regex> = OnceLock::new();
    let re = TOKEN_RE
        .get_or_init(|| Regex::new(r"(?i)\b[a-z0-9_\-]{32,}\b").expect("token regex compiles"));
    for m in re.find_iter(text) {
        let s = m.as_str();
        let has_alpha = s.chars().any(|c| c.is_ascii_alphabetic());
        let has_digit = s.chars().any(|c| c.is_ascii_digit());
        if has_alpha && has_digit {
            return true;
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

fn sanitize_timezone_value(raw: &str) -> Option<String> {
    let mut tz = sanitize_single_line(raw, 40);
    if tz.starts_with(':') {
        tz = tz.trim_start_matches(':').trim().to_string();
    }
    if tz.is_empty() {
        return None;
    }
    if tz.chars().any(|c| c.is_whitespace()) {
        return None;
    }
    Some(tz)
}

fn normalize_timezone_input(raw: &str) -> Option<String> {
    let tz = sanitize_timezone_value(raw)?;
    let lower = tz.to_ascii_lowercase();

    let mapped = match lower.as_str() {
        "utc" | "gmt" | "z" => Some("Etc/UTC".to_string()),
        "pt" | "pst" | "pdt" | "pacific" | "us/pacific" => Some("America/Los_Angeles".to_string()),
        "mt" | "mst" | "mdt" | "mountain" | "us/mountain" => Some("America/Denver".to_string()),
        "ct" | "cst" | "cdt" | "central" | "us/central" => Some("America/Chicago".to_string()),
        "et" | "est" | "edt" | "eastern" | "us/eastern" => Some("America/New_York".to_string()),
        "akst" | "akdt" | "alaska" => Some("America/Anchorage".to_string()),
        "hst" | "hawaii" => Some("Pacific/Honolulu".to_string()),
        _ => None,
    };
    if let Some(v) = mapped {
        return Some(v);
    }

    if let Ok(parsed) = tz.parse::<chrono_tz::Tz>() {
        return Some(parsed.to_string());
    }

    Some(tz)
}

pub fn detect_local_timezone() -> Option<String> {
    if let Ok(v) = std::env::var("DRBOT_USER_TIMEZONE") {
        if let Some(tz) = normalize_timezone_input(&v) {
            return Some(tz);
        }
    }
    if let Ok(v) = std::env::var("TZ") {
        if let Some(tz) = normalize_timezone_input(&v) {
            return Some(tz);
        }
    }
    iana_time_zone::get_timezone()
        .ok()
        .and_then(|tz| normalize_timezone_input(&tz))
}

#[derive(Debug, Clone, Copy)]
enum FieldMode {
    Set,
    Append,
}

fn format_user_bullet_field_line(label: &str, value: &str) -> String {
    let v = value.trim();
    if v.is_empty() {
        format!("- {}:", label)
    } else {
        format!("- {}: {}", label, v)
    }
}

fn upsert_user_bullet_field(
    doc: &str,
    label: &str,
    value: &str,
    mode: FieldMode,
) -> (String, bool) {
    let key = format!("- {}:", label);
    let mut lines: Vec<String> = doc.lines().map(|l| l.to_string()).collect();

    for line in &mut lines {
        if line.trim_start().starts_with(&key) {
            let current = line.splitn(2, ':').nth(1).map(|s| s.trim()).unwrap_or("");
            let next_value = match mode {
                FieldMode::Set => value.trim().to_string(),
                FieldMode::Append => {
                    if current.is_empty() {
                        value.trim().to_string()
                    } else if current
                        .to_ascii_lowercase()
                        .contains(&value.to_ascii_lowercase())
                    {
                        current.to_string()
                    } else {
                        format!("{}; {}", current, value.trim())
                    }
                }
            };

            let new_line = format_user_bullet_field_line(label, &next_value);
            if line.trim_end() == new_line {
                return (doc.to_string(), false);
            }
            *line = new_line;
            return (lines.join("\n") + "\n", true);
        }
    }

    // Not found; append to the end (best effort).
    let mut out = doc.to_string();
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(&format!(
        "{}\n",
        format_user_bullet_field_line(label, value)
    ));
    (out, true)
}

fn upsert_user_bullet_field_if_blank(doc: &str, label: &str, value: &str) -> (String, bool) {
    let key = format!("- {}:", label);
    let mut lines: Vec<String> = doc.lines().map(|l| l.to_string()).collect();

    for line in &mut lines {
        if line.trim_start().starts_with(&key) {
            let current = line.splitn(2, ':').nth(1).map(|s| s.trim()).unwrap_or("");
            if !current.is_empty() {
                return (doc.to_string(), false);
            }
            let new_line = format_user_bullet_field_line(label, value);
            if line.trim_end() == new_line {
                return (doc.to_string(), false);
            }
            *line = new_line;
            return (lines.join("\n") + "\n", true);
        }
    }

    // Not found; append to the end (best effort).
    let mut out = doc.to_string();
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(&format!(
        "{}\n",
        format_user_bullet_field_line(label, value)
    ));
    (out, true)
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

fn memory_md_path(workspace_dir: &Path) -> PathBuf {
    let primary = workspace_dir.join("MEMORY.md");
    if primary.is_file() {
        return primary;
    }
    let alt = workspace_dir.join("memory.md");
    if alt.is_file() {
        return alt;
    }
    primary
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
    std::fs::rename(&tmp, path)?;
    Ok(())
}

fn add_memory_bullet(
    workspace_dir: &Path,
    section_header: &str,
    bullet_line: &str,
) -> std::io::Result<bool> {
    let path = memory_md_path(workspace_dir);
    let mut doc = std::fs::read_to_string(&path).unwrap_or_default();
    if doc.trim().is_empty() {
        // Minimal scaffold if missing.
        doc = "# Memory\n\n## Pinned\n\n## Preferences\n\n## Knowledge base\n".to_string();
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

fn maybe_store_long_note_as_file(
    workspace_dir: &Path,
    note: &str,
) -> std::io::Result<Option<String>> {
    const MAX_INLINE: usize = 220;
    let note = note.trim();
    if note.chars().count() <= MAX_INLINE {
        return Ok(None);
    }

    let memory_dir = workspace_dir.join("memory").join("auto");
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

fn extract_name(text: &str) -> Option<String> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"(?i)\b(?:my name is|call me)\s+([A-Za-z][A-Za-z .'\-]{0,60})")
            .expect("name regex compiles")
    });
    let mut out = None;
    for cap in re.captures_iter(text) {
        let raw = cap.get(1).map(|m| m.as_str()).unwrap_or("");
        let value = sanitize_single_line(raw, 64);
        if !value.is_empty() {
            out = Some(value);
        }
    }
    out
}

fn extract_timezone(text: &str) -> Option<String> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"(?i)\b(?:my\s+time\s*zone\s+is|my\s+timezone\s+is|timezone\s+is|tz\s+is)\s+([A-Za-z0-9_\-+/:]{1,40})")
            .expect("tz regex compiles")
    });
    let mut out = None;
    for cap in re.captures_iter(text) {
        let raw = cap.get(1).map(|m| m.as_str()).unwrap_or("");
        if let Some(value) = normalize_timezone_input(raw) {
            if !value.is_empty() {
                out = Some(value);
            }
        }
    }
    out
}

pub fn parse_remember_command(text: &str) -> Option<String> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"(?is)^\s*(?:/remember\b\s*:?\s*|remember:\s*)(.+?)\s*$")
            .expect("remember regex compiles")
    });
    let cap = re.captures(text)?;
    let raw = cap.get(1).map(|m| m.as_str()).unwrap_or("");
    let note = raw.trim();
    if note.is_empty() {
        None
    } else {
        Some(note.to_string())
    }
}

pub fn parse_forget_command(text: &str) -> Option<String> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"(?is)^\s*(?:/forget\b\s*:?\s*|forget:\s*)(.+?)\s*$")
            .expect("forget regex compiles")
    });
    let cap = re.captures(text)?;
    let raw = cap.get(1).map(|m| m.as_str()).unwrap_or("");
    let arg = raw.trim();
    if arg.is_empty() {
        None
    } else {
        Some(arg.to_string())
    }
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

fn remove_matching_semicolon_items(current: &str, query: &str) -> (String, bool) {
    let query = query.trim();
    if current.trim().is_empty() || query.is_empty() {
        return (current.trim().to_string(), false);
    }
    let query_lower = query.to_ascii_lowercase();
    let mut kept: Vec<String> = Vec::new();
    let mut removed_any = false;
    for item in current.split(';') {
        let t = item.trim();
        if t.is_empty() {
            continue;
        }
        if t.to_ascii_lowercase().contains(&query_lower) {
            removed_any = true;
            continue;
        }
        kept.push(t.to_string());
    }
    if !removed_any {
        return (current.trim().to_string(), false);
    }
    (kept.join("; "), true)
}

fn extract_auto_note_rel_paths(text: &str) -> Vec<String> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"(?i)\b(memory/auto/note-[0-9a-f\-]{8,}\.md)\b")
            .expect("auto note path regex compiles")
    });
    re.captures_iter(text)
        .filter_map(|cap| cap.get(1).map(|m| m.as_str().to_string()))
        .collect()
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

pub fn forget_workspace(workspace_dir: &Path, user_text: &str) -> std::io::Result<AutosaveOutput> {
    let mut out = AutosaveOutput::default();
    if !workspace_dir.is_dir() {
        return Ok(out);
    }

    let Some(arg_raw) = parse_forget_command(user_text) else {
        return Ok(out);
    };
    let arg = strip_outer_quotes(&arg_raw).trim().to_string();
    if arg.is_empty() {
        return Ok(out);
    }
    let arg_lower = arg.to_ascii_lowercase();

    // USER.md mutations (clear fields or remove a specific preference item).
    let mut user_doc_path = workspace_dir.join("USER.md");
    if !user_doc_path.is_file() {
        user_doc_path = workspace_dir.join("user.md");
    }
    let user_doc_raw = std::fs::read_to_string(&user_doc_path).unwrap_or_default();
    let mut user_doc = user_doc_raw.clone();
    let mut user_changed = false;

    let clear_user_fields = |doc: &str, labels: &[&str]| -> (String, Vec<String>, bool) {
        let mut next = doc.to_string();
        let mut cleared: Vec<String> = Vec::new();
        let mut changed = false;
        for label in labels {
            let (d, c) = upsert_user_bullet_field(&next, label, "", FieldMode::Set);
            next = d;
            if c {
                changed = true;
                cleared.push((*label).to_string());
            }
        }
        (next, cleared, changed)
    };

    let mut query_removed_from_user = false;
    let mut cleared_user_labels: Vec<String> = Vec::new();
    let is_memory_only_command = matches!(
        arg_lower.as_str(),
        "pinned" | "knowledge" | "kb" | "knowledge base" | "memory"
    );
    match arg_lower.as_str() {
        "name" => {
            let (next, cleared, changed) = clear_user_fields(&user_doc, &["Name"]);
            user_doc = next;
            if changed {
                user_changed = true;
                cleared_user_labels.extend(cleared);
            }
        }
        "timezone" | "tz" => {
            let (next, cleared, changed) = clear_user_fields(&user_doc, &["Timezone"]);
            user_doc = next;
            if changed {
                user_changed = true;
                cleared_user_labels.extend(cleared);
            }
        }
        "style" | "preferences" => {
            let (next, cleared, changed) = clear_user_fields(
                &user_doc,
                &[
                    "Preferred tone/style",
                    "Formatting preferences (e.g., bullets, terse/verbose)",
                    "Defaults (units, currency, locale)",
                    "Avoid / don't do",
                ],
            );
            user_doc = next;
            if changed {
                user_changed = true;
                cleared_user_labels.extend(cleared);
            }
        }
        "formatting" => {
            let (next, cleared, changed) = clear_user_fields(
                &user_doc,
                &["Formatting preferences (e.g., bullets, terse/verbose)"],
            );
            user_doc = next;
            if changed {
                user_changed = true;
                cleared_user_labels.extend(cleared);
            }
        }
        "defaults" => {
            let (next, cleared, changed) =
                clear_user_fields(&user_doc, &["Defaults (units, currency, locale)"]);
            user_doc = next;
            if changed {
                user_changed = true;
                cleared_user_labels.extend(cleared);
            }
        }
        "avoid" => {
            let (next, cleared, changed) = clear_user_fields(&user_doc, &["Avoid / don't do"]);
            user_doc = next;
            if changed {
                user_changed = true;
                cleared_user_labels.extend(cleared);
            }
        }
        "profile" | "user" => {
            let (next, cleared, changed) = clear_user_fields(
                &user_doc,
                &[
                    "Name",
                    "Timezone",
                    "Preferred tone/style",
                    "Formatting preferences (e.g., bullets, terse/verbose)",
                    "Defaults (units, currency, locale)",
                    "Avoid / don't do",
                ],
            );
            user_doc = next;
            if changed {
                user_changed = true;
                cleared_user_labels.extend(cleared);
            }
        }
        "all" => {
            let (next, cleared, changed) = clear_user_fields(
                &user_doc,
                &[
                    "Name",
                    "Timezone",
                    "Preferred tone/style",
                    "Formatting preferences (e.g., bullets, terse/verbose)",
                    "Defaults (units, currency, locale)",
                    "Avoid / don't do",
                ],
            );
            user_doc = next;
            if changed {
                user_changed = true;
                cleared_user_labels.extend(cleared);
            }
        }
        _ => {
            if is_memory_only_command {
                // Don't try to interpret memory-clearing commands as user preference items.
            } else if arg_lower.starts_with("file:")
                || (arg_lower.starts_with("memory/") && arg_lower.ends_with(".md"))
            {
                // Explicit file unlink/deletion; don't touch USER.md.
            } else {
                // Try to remove an item from list-style preference fields (best effort).
                let query = arg.as_str();
                let labels = [
                    "Preferred tone/style",
                    "Formatting preferences (e.g., bullets, terse/verbose)",
                    "Defaults (units, currency, locale)",
                    "Avoid / don't do",
                ];
                let mut lines: Vec<String> = user_doc.lines().map(|l| l.to_string()).collect();
                for line in &mut lines {
                    for label in labels {
                        let key = format!("- {}:", label);
                        let snapshot = line.clone();
                        let trimmed = snapshot.trim_start();
                        if trimmed.starts_with(&key) {
                            let current = trimmed
                                .splitn(2, ':')
                                .nth(1)
                                .map(|s| s.trim())
                                .unwrap_or("");
                            let (next_value, removed_any) =
                                remove_matching_semicolon_items(current, query);
                            if removed_any {
                                let new_line = format_user_bullet_field_line(label, &next_value);
                                if line.trim_end() != new_line {
                                    *line = new_line;
                                    user_changed = true;
                                    query_removed_from_user = true;
                                }
                            }
                        }
                    }
                }
                if user_changed {
                    user_doc = lines.join("\n") + "\n";
                }
            }
        }
    }

    if user_changed && user_doc != user_doc_raw {
        write_atomic(&user_doc_path, &user_doc)?;
        out.applied = true;
        if !cleared_user_labels.is_empty() {
            for label in cleared_user_labels {
                out.updates.push(format!("USER.md: cleared {}", label));
            }
        } else if query_removed_from_user {
            out.updates
                .push("USER.md: removed matching preference item".to_string());
        }
    }

    // MEMORY.md mutations (remove bullets or clear sections).
    let memory_path = memory_md_path(workspace_dir);
    let mem_raw = std::fs::read_to_string(&memory_path).unwrap_or_default();
    let mut mem_doc = mem_raw.clone();
    let mut removed_lines: Vec<String> = Vec::new();
    let mut delete_rel_paths: Vec<String> = Vec::new();

    let is_explicit_rel_path = {
        let a = arg.trim();
        a.starts_with("memory/") && a.ends_with(".md")
    };
    let explicit_rel_path = if is_explicit_rel_path {
        Some(arg.trim().to_string())
    } else if arg_lower.starts_with("file:") {
        Some(arg[5..].trim().to_string())
    } else {
        None
    };

    if !mem_doc.trim().is_empty() {
        match arg_lower.as_str() {
            "pinned" => {
                let (next, removed, changed) =
                    remove_memory_bullets(&mem_doc, &["## Pinned"], |_| true);
                mem_doc = next;
                if changed {
                    removed_lines.extend(removed);
                }
            }
            "knowledge" | "kb" | "knowledge base" => {
                let (next, removed, changed) =
                    remove_memory_bullets(&mem_doc, &["## Knowledge base"], |_| true);
                mem_doc = next;
                if changed {
                    removed_lines.extend(removed);
                }
            }
            "memory" => {
                let (next, removed, changed) = remove_memory_bullets(
                    &mem_doc,
                    &["## Pinned", "## Preferences", "## Knowledge base"],
                    |_| true,
                );
                mem_doc = next;
                if changed {
                    removed_lines.extend(removed);
                }
            }
            "all" => {
                let (next, removed, changed) = remove_memory_bullets(
                    &mem_doc,
                    &["## Pinned", "## Preferences", "## Knowledge base"],
                    |_| true,
                );
                mem_doc = next;
                if changed {
                    removed_lines.extend(removed);
                }
            }
            _ => {
                if let Some(rel) = explicit_rel_path.as_deref() {
                    let needle = rel.to_ascii_lowercase();
                    let (next, removed, changed) = remove_memory_bullets(
                        &mem_doc,
                        &["## Pinned", "## Preferences", "## Knowledge base"],
                        |line| line.to_ascii_lowercase().contains(&needle),
                    );
                    mem_doc = next;
                    if changed {
                        removed_lines.extend(removed);
                        delete_rel_paths.push(rel.to_string());
                    }
                } else {
                    let needle = arg.to_ascii_lowercase();
                    let (next, removed, changed) = remove_memory_bullets(
                        &mem_doc,
                        &["## Pinned", "## Preferences", "## Knowledge base"],
                        |line| line.to_ascii_lowercase().contains(&needle),
                    );
                    mem_doc = next;
                    if changed {
                        removed_lines.extend(removed);
                    }
                }
            }
        }
    }

    for line in &removed_lines {
        for rel in extract_auto_note_rel_paths(line) {
            delete_rel_paths.push(rel);
        }
    }
    delete_rel_paths.sort();
    delete_rel_paths.dedup();

    if mem_doc != mem_raw && !mem_raw.trim().is_empty() {
        write_atomic(&memory_path, &mem_doc)?;
        out.applied = true;
        if !removed_lines.is_empty() {
            out.updates.push(format!(
                "MEMORY.md: removed {} item(s)",
                removed_lines.len()
            ));
        }
    }

    // Optionally delete auto note files when unlinked.
    for rel in delete_rel_paths {
        let rel = rel.trim();
        if !is_safe_auto_note_rel_path(rel) {
            continue;
        }
        let path = workspace_dir.join(rel);
        if path.is_file() {
            let _ = std::fs::remove_file(&path);
            out.applied = true;
            out.updates.push(format!("Deleted {}", rel));
        }
    }

    Ok(out)
}

pub fn forget_workspace_best_effort(workspace_dir: &Path, user_text: &str) -> AutosaveOutput {
    match forget_workspace(workspace_dir, user_text) {
        Ok(v) => v,
        Err(err) => {
            tracing::warn!(
                error = %err,
                workspace = %workspace_dir.display(),
                "Workspace forget failed"
            );
            AutosaveOutput::default()
        }
    }
}

fn extract_style_updates(text: &str) -> (Vec<String>, Vec<String>, Vec<String>, Vec<String>) {
    let lower = text.to_ascii_lowercase();

    let mut preferred_style: Vec<String> = Vec::new();
    let mut formatting: Vec<String> = Vec::new();
    let mut defaults: Vec<String> = Vec::new();
    let mut avoid: Vec<String> = Vec::new();

    let long_term_signal = [
        "from now on",
        "going forward",
        "always",
        "in the future",
        "please always",
    ]
    .iter()
    .any(|n| lower.contains(n));

    // A few high-confidence preferences we store automatically even without long-term phrasing.
    if lower.contains("no emojis")
        || lower.contains("don't use emojis")
        || lower.contains("do not use emojis")
    {
        avoid.push("no emojis".to_string());
    }

    // Style/formatting: only persist when the user indicates it should stick.
    if long_term_signal {
        if lower.contains("be concise")
            || lower.contains("keep it short")
            || lower.contains("be brief")
        {
            formatting.push("terse/concise".to_string());
        }
        if lower.contains("be verbose")
            || lower.contains("more detail")
            || lower.contains("more detailed")
        {
            formatting.push("verbose/detailed".to_string());
        }
        if lower.contains("bullet points") || lower.contains("use bullets") {
            formatting.push("bullets".to_string());
        }
        if lower.contains("professional tone") {
            preferred_style.push("professional".to_string());
        }
        if lower.contains("casual tone") {
            preferred_style.push("casual".to_string());
        }
        if lower.contains("use metric") {
            defaults.push("metric units".to_string());
        }
        if lower.contains("use imperial") {
            defaults.push("imperial units".to_string());
        }
        if lower.contains("use usd") || lower.contains("in usd") {
            defaults.push("currency: USD".to_string());
        }
    }

    (preferred_style, formatting, defaults, avoid)
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

pub fn autosave_workspace(
    workspace_dir: &Path,
    user_text: &str,
) -> std::io::Result<AutosaveOutput> {
    let mut out = AutosaveOutput::default();
    if !workspace_dir.is_dir() {
        return Ok(out);
    }

    let user_text = user_text.trim();
    if user_text.is_empty() {
        return Ok(out);
    }

    // USER.md updates (name/timezone/style).
    let autosave_enabled = autosave_enabled();
    let mut user_doc_path = workspace_dir.join("USER.md");
    if !user_doc_path.is_file() {
        // Don't fail; just skip.
        user_doc_path = workspace_dir.join("user.md");
    }
    let user_doc_raw = std::fs::read_to_string(&user_doc_path).unwrap_or_default();
    let mut user_doc = user_doc_raw.clone();
    let mut user_changed = false;

    if autosave_enabled {
        if let Some(name) = extract_name(user_text) {
            let (next, changed) =
                upsert_user_bullet_field(&user_doc, "Name", &name, FieldMode::Set);
            user_doc = next;
            if changed {
                user_changed = true;
                out.updates.push("USER.md: Name".to_string());
            }
        }

        if let Some(tz) = extract_timezone(user_text) {
            let (next, changed) =
                upsert_user_bullet_field(&user_doc, "Timezone", &tz, FieldMode::Set);
            user_doc = next;
            if changed {
                user_changed = true;
                out.updates.push("USER.md: Timezone".to_string());
            }
        }

        let (preferred_style, formatting, defaults, avoid) = extract_style_updates(user_text);
        for item in preferred_style {
            let (next, changed) = upsert_user_bullet_field(
                &user_doc,
                "Preferred tone/style",
                &item,
                FieldMode::Append,
            );
            user_doc = next;
            if changed {
                user_changed = true;
                out.updates
                    .push("USER.md: Preferred tone/style".to_string());
            }
        }
        for item in formatting {
            let (next, changed) = upsert_user_bullet_field(
                &user_doc,
                "Formatting preferences (e.g., bullets, terse/verbose)",
                &item,
                FieldMode::Append,
            );
            user_doc = next;
            if changed {
                user_changed = true;
                out.updates
                    .push("USER.md: Formatting preferences".to_string());
            }
        }
        for item in defaults {
            let (next, changed) = upsert_user_bullet_field(
                &user_doc,
                "Defaults (units, currency, locale)",
                &item,
                FieldMode::Append,
            );
            user_doc = next;
            if changed {
                user_changed = true;
                out.updates.push("USER.md: Defaults".to_string());
            }
        }
        for item in avoid {
            let (next, changed) =
                upsert_user_bullet_field(&user_doc, "Avoid / don't do", &item, FieldMode::Append);
            user_doc = next;
            if changed {
                user_changed = true;
                out.updates.push("USER.md: Avoid".to_string());
            }
        }
    }

    if user_changed && user_doc != user_doc_raw {
        write_atomic(&user_doc_path, &user_doc)?;
        out.applied = true;
    }

    // Auto-capture a small number of high-confidence stable facts into MEMORY.md.
    //
    // This is intentionally conservative: we only store short declarative "we use X" style
    // statements and refuse anything that looks sensitive.
    if autosave_enabled && parse_remember_command(user_text).is_none() {
        if let Some(fact) = extract_auto_pinned_fact_line(user_text) {
            let bullet = format!("- {}", fact);
            if add_memory_bullet(workspace_dir, "## Pinned", &bullet)? {
                out.applied = true;
                out.updates
                    .push("MEMORY.md: added pinned note (auto)".to_string());
            }
        }
    }

    // Explicit remember notes.
    if let Some(note) = parse_remember_command(user_text) {
        if !looks_sensitive(&note) {
            if let Some(bullet) = maybe_store_long_note_as_file(workspace_dir, &note)? {
                if add_memory_bullet(workspace_dir, "## Knowledge base", &bullet)? {
                    out.applied = true;
                    out.updates
                        .push("MEMORY.md: added knowledge base note".to_string());
                }
            } else {
                let short = sanitize_single_line(&note, 220);
                let bullet = format!("- {}", short);
                if add_memory_bullet(workspace_dir, "## Pinned", &bullet)? {
                    out.applied = true;
                    out.updates.push("MEMORY.md: added pinned note".to_string());
                }
            }
        }
    }

    // Explicit forget.
    if parse_forget_command(user_text).is_some() {
        let forgot = forget_workspace(workspace_dir, user_text)?;
        if forgot.applied {
            out.applied = true;
            out.updates.extend(forgot.updates);
        }
    }

    Ok(out)
}

pub fn autosave_workspace_best_effort(workspace_dir: &Path, user_text: &str) -> AutosaveOutput {
    match autosave_workspace(workspace_dir, user_text) {
        Ok(v) => v,
        Err(err) => {
            tracing::warn!(
                error = %err,
                workspace = %workspace_dir.display(),
                "Workspace autosave failed"
            );
            AutosaveOutput::default()
        }
    }
}

pub fn ensure_user_timezone_best_effort(workspace_dir: &Path) -> Option<String> {
    if !autosave_enabled() {
        return None;
    }
    if !workspace_dir.is_dir() {
        return None;
    }
    let tz = detect_local_timezone()?;

    let user_doc_path = if workspace_dir.join("USER.md").is_file() {
        workspace_dir.join("USER.md")
    } else if workspace_dir.join("user.md").is_file() {
        workspace_dir.join("user.md")
    } else {
        return None;
    };

    let user_doc_raw = std::fs::read_to_string(&user_doc_path).ok()?;
    if user_doc_raw.trim().is_empty() {
        return None;
    }

    let (next, changed) = upsert_user_bullet_field_if_blank(&user_doc_raw, "Timezone", &tz);
    if !changed || next == user_doc_raw {
        return None;
    }
    write_atomic(&user_doc_path, &next).ok()?;
    Some(tz)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};
    use uuid::Uuid;

    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .expect("env lock")
    }

    fn temp_root(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "drbot-workspace-autosave-test-{}-{}",
            name,
            Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn write_user_template(root: &Path) {
        std::fs::write(
            root.join("USER.md"),
            r#"# User

- Name:
- Timezone:
- Preferred tone/style:
- Formatting preferences (e.g., bullets, terse/verbose):
- Defaults (units, currency, locale):
- Avoid / don't do:
"#,
        )
        .expect("write USER.md");
    }

    fn write_memory_template(root: &Path) {
        std::fs::write(
            root.join("MEMORY.md"),
            r#"# Memory

## Pinned

## Preferences

## Knowledge base
"#,
        )
        .expect("write MEMORY.md");
    }

    #[test]
    fn autosave_sets_name() {
        let root = temp_root("name");
        write_user_template(&root);
        write_memory_template(&root);

        let out = autosave_workspace(&root, "My name is Alice.").expect("autosave");
        assert!(out.applied);
        let user = std::fs::read_to_string(root.join("USER.md")).expect("read USER.md");
        assert!(user.contains("- Name: Alice"));
    }

    #[test]
    fn autosave_sets_timezone() {
        let root = temp_root("tz");
        write_user_template(&root);
        write_memory_template(&root);

        let out =
            autosave_workspace(&root, "My timezone is America/Los_Angeles.").expect("autosave");
        assert!(out.applied);
        let user = std::fs::read_to_string(root.join("USER.md")).expect("read USER.md");
        assert!(user.contains("- Timezone: America/Los_Angeles"));
    }

    #[test]
    fn autosave_auto_pins_stable_fact() {
        let root = temp_root("auto-fact");
        write_user_template(&root);
        write_memory_template(&root);

        let out = autosave_workspace(&root, "We use Postgres.").expect("autosave");
        assert!(out.applied);

        let mem = std::fs::read_to_string(root.join("MEMORY.md")).expect("read MEMORY.md");
        assert!(mem.contains("## Pinned"));
        assert!(mem.contains("- We use Postgres."));
    }

    #[test]
    fn autosave_auto_pins_stable_fact_with_leadin() {
        let root = temp_root("auto-fact-leadin");
        write_user_template(&root);
        write_memory_template(&root);

        let out = autosave_workspace(&root, "FYI: We use Postgres.").expect("autosave");
        assert!(out.applied);

        let mem = std::fs::read_to_string(root.join("MEMORY.md")).expect("read MEMORY.md");
        assert!(mem.contains("- We use Postgres."));
    }

    #[test]
    fn autosave_does_not_auto_pin_ephemeral_fact() {
        let root = temp_root("auto-ephemeral");
        write_user_template(&root);
        write_memory_template(&root);

        let out = autosave_workspace(&root, "We use Postgres for now.").expect("autosave");
        assert!(!out.applied);

        let mem = std::fs::read_to_string(root.join("MEMORY.md")).expect("read MEMORY.md");
        assert!(!mem.contains("Postgres"));
    }

    #[test]
    fn autosave_does_not_auto_pin_pronoun_fact() {
        let root = temp_root("auto-pronoun");
        write_user_template(&root);
        write_memory_template(&root);

        let out = autosave_workspace(&root, "We use this to test.").expect("autosave");
        assert!(!out.applied);

        let mem = std::fs::read_to_string(root.join("MEMORY.md")).expect("read MEMORY.md");
        assert!(!mem.contains("We use this"));
    }

    #[test]
    fn autosave_does_not_auto_pin_sensitive_fact() {
        let root = temp_root("auto-sensitive");
        write_user_template(&root);
        write_memory_template(&root);

        let out = autosave_workspace(&root, "We use sk-1234567890abcdef.").expect("autosave");
        assert!(!out.applied);

        let mem = std::fs::read_to_string(root.join("MEMORY.md")).expect("read MEMORY.md");
        assert!(!mem.contains("sk-"));
    }

    #[test]
    fn remember_adds_pinned_bullet() {
        let root = temp_root("remember");
        write_user_template(&root);
        write_memory_template(&root);

        let out = autosave_workspace(&root, "/remember We use Postgres.").expect("autosave");
        assert!(out.applied);
        let mem = std::fs::read_to_string(root.join("MEMORY.md")).expect("read MEMORY.md");
        assert!(mem.contains("## Pinned"));
        assert!(mem.contains("- We use Postgres."));
    }

    #[test]
    fn remember_long_note_creates_file_and_links() {
        let root = temp_root("long");
        write_user_template(&root);
        write_memory_template(&root);

        let long = "remember: ".to_string() + &"x".repeat(600);
        let out = autosave_workspace(&root, &long).expect("autosave");
        assert!(out.applied);

        let auto_dir = root.join("memory").join("auto");
        let entries = std::fs::read_dir(&auto_dir)
            .expect("read auto dir")
            .filter_map(|e| e.ok())
            .collect::<Vec<_>>();
        assert!(!entries.is_empty(), "expected an auto note file");

        let mem = std::fs::read_to_string(root.join("MEMORY.md")).expect("read MEMORY.md");
        assert!(mem.contains("## Knowledge base"));
        assert!(mem.contains("memory/auto/note-"));
    }

    #[test]
    fn forget_clears_name() {
        let root = temp_root("forget-name");
        write_user_template(&root);
        write_memory_template(&root);

        let _ = autosave_workspace(&root, "My name is Alice.").expect("autosave");
        let out = forget_workspace(&root, "/forget name").expect("forget");
        assert!(out.applied);

        let user = std::fs::read_to_string(root.join("USER.md")).expect("read USER.md");
        assert!(user.contains("- Name:"));
        assert!(!user.contains("- Name: Alice"));
    }

    #[test]
    fn forget_removes_pinned_bullet_by_query() {
        let root = temp_root("forget-query");
        write_user_template(&root);
        write_memory_template(&root);

        let _ = autosave_workspace(&root, "/remember We use Postgres.").expect("autosave");
        let _ = autosave_workspace(&root, "/remember We use Redis.").expect("autosave");

        let out = forget_workspace(&root, "/forget postgres").expect("forget");
        assert!(out.applied);

        let mem = std::fs::read_to_string(root.join("MEMORY.md")).expect("read MEMORY.md");
        assert!(!mem.contains("We use Postgres."));
        assert!(mem.contains("We use Redis."));
    }

    #[test]
    fn forget_deletes_auto_note_when_unlinked() {
        let root = temp_root("forget-file");
        write_user_template(&root);
        write_memory_template(&root);

        let long = "remember: ".to_string() + &"x".repeat(600);
        let _ = autosave_workspace(&root, &long).expect("autosave");

        let auto_dir = root.join("memory").join("auto");
        let entry = std::fs::read_dir(&auto_dir)
            .expect("read auto dir")
            .filter_map(|e| e.ok())
            .find(|e| e.path().is_file())
            .expect("expected note file");
        let file_name = entry.file_name().to_string_lossy().to_string();
        let rel = format!("memory/auto/{}", file_name);
        assert!(root.join(&rel).is_file());

        let out = forget_workspace(&root, &format!("/forget {}", rel)).expect("forget");
        assert!(out.applied);

        assert!(!root.join(&rel).is_file());
        let mem = std::fs::read_to_string(root.join("MEMORY.md")).expect("read MEMORY.md");
        assert!(!mem.contains(&rel));
    }

    #[test]
    fn forget_all_clears_profile_and_memory() {
        let root = temp_root("forget-all");
        write_user_template(&root);
        write_memory_template(&root);

        let _ = autosave_workspace(&root, "My name is Alice.").expect("autosave");
        let _ = autosave_workspace(&root, "My timezone is America/Los_Angeles.").expect("autosave");
        let _ = autosave_workspace(&root, "/remember We use Postgres.").expect("autosave");

        let out = forget_workspace(&root, "/forget all").expect("forget");
        assert!(out.applied);

        let user = std::fs::read_to_string(root.join("USER.md")).expect("read USER.md");
        assert!(!user.contains("Alice"));
        assert!(!user.contains("America/Los_Angeles"));

        let mem = std::fs::read_to_string(root.join("MEMORY.md")).expect("read MEMORY.md");
        assert!(!mem.contains("We use Postgres."));
    }

    #[test]
    fn ensure_timezone_fills_blank_timezone() {
        let _lock = env_lock();
        let old_tz = std::env::var("DRBOT_USER_TIMEZONE").ok();
        let old_autosave = std::env::var("DRBOT_GATEWAY_WORKSPACE_AUTOSAVE_ENABLED").ok();
        std::env::set_var("DRBOT_USER_TIMEZONE", "Etc/UTC");
        std::env::set_var("DRBOT_GATEWAY_WORKSPACE_AUTOSAVE_ENABLED", "1");

        let root = temp_root("ensure-tz");
        write_user_template(&root);
        write_memory_template(&root);

        let applied = ensure_user_timezone_best_effort(&root).expect("expected applied tz");
        assert_eq!(applied, "Etc/UTC");
        let user = std::fs::read_to_string(root.join("USER.md")).expect("read USER.md");
        assert!(user.contains("- Timezone: Etc/UTC"));

        match old_tz {
            Some(v) => std::env::set_var("DRBOT_USER_TIMEZONE", v),
            None => std::env::remove_var("DRBOT_USER_TIMEZONE"),
        }
        match old_autosave {
            Some(v) => std::env::set_var("DRBOT_GATEWAY_WORKSPACE_AUTOSAVE_ENABLED", v),
            None => std::env::remove_var("DRBOT_GATEWAY_WORKSPACE_AUTOSAVE_ENABLED"),
        }
    }
}
