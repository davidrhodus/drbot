use std::path::{Path, PathBuf};

fn read_to_string_if_file(path: &Path) -> Option<String> {
    if !path.is_file() {
        return None;
    }
    std::fs::read_to_string(path).ok()
}

fn user_md_path(workspace_dir: &Path) -> Option<PathBuf> {
    let primary = workspace_dir.join("USER.md");
    if primary.is_file() {
        return Some(primary);
    }
    let alt = workspace_dir.join("user.md");
    if alt.is_file() {
        return Some(alt);
    }
    None
}

fn memory_md_path(workspace_dir: &Path) -> Option<PathBuf> {
    let primary = workspace_dir.join("MEMORY.md");
    if primary.is_file() {
        return Some(primary);
    }
    let alt = workspace_dir.join("memory.md");
    if alt.is_file() {
        return Some(alt);
    }
    None
}

fn extract_user_bullet_value(doc: &str, label: &str) -> Option<String> {
    let key = format!("- {}:", label);
    for line in doc.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with(&key) {
            let value = trimmed
                .splitn(2, ':')
                .nth(1)
                .map(|s| s.trim())
                .unwrap_or("");
            if value.is_empty() {
                return None;
            }
            return Some(value.to_string());
        }
    }
    None
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

fn count_memory_bullets_by_section(doc: &str) -> (usize, usize, usize) {
    let mut pinned = 0usize;
    let mut prefs = 0usize;
    let mut kb = 0usize;
    let mut section: Option<&str> = None;
    for line in doc.lines() {
        let t = line.trim();
        if t == "## Pinned" {
            section = Some("pinned");
            continue;
        }
        if t == "## Preferences" {
            section = Some("prefs");
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
            Some("prefs") => prefs += 1,
            Some("kb") => kb += 1,
            _ => {}
        }
    }
    (pinned, prefs, kb)
}

pub fn build_workspace_profile_overview(workspace_dir: &Path) -> String {
    if !workspace_dir.is_dir() {
        return "Workspace directory is unavailable.".to_string();
    }
    let Some(user_path) = user_md_path(workspace_dir) else {
        return "No USER.md found in workspace.".to_string();
    };
    let Some(user_doc) = read_to_string_if_file(&user_path) else {
        return "Failed to read USER.md.".to_string();
    };

    let name =
        extract_user_bullet_value(&user_doc, "Name").unwrap_or_else(|| "(unset)".to_string());
    let tz =
        extract_user_bullet_value(&user_doc, "Timezone").unwrap_or_else(|| "(unset)".to_string());
    let tone = extract_user_bullet_value(&user_doc, "Preferred tone/style")
        .unwrap_or_else(|| "(unset)".to_string());
    let formatting = extract_user_bullet_value(
        &user_doc,
        "Formatting preferences (e.g., bullets, terse/verbose)",
    )
    .unwrap_or_else(|| "(unset)".to_string());
    let defaults = extract_user_bullet_value(&user_doc, "Defaults (units, currency, locale)")
        .unwrap_or_else(|| "(unset)".to_string());
    let avoid = extract_user_bullet_value(&user_doc, "Avoid / don't do")
        .unwrap_or_else(|| "(unset)".to_string());

    format!(
        "Workspace profile:\n- Name: {}\n- Timezone: {}\n- Preferred tone/style: {}\n- Formatting: {}\n- Defaults: {}\n- Avoid: {}",
        name, tz, tone, formatting, defaults, avoid
    )
}

pub fn build_workspace_memory_overview(workspace_dir: &Path) -> String {
    if !workspace_dir.is_dir() {
        return "Workspace directory is unavailable.".to_string();
    }

    let user_doc = user_md_path(workspace_dir)
        .and_then(|p| read_to_string_if_file(&p))
        .unwrap_or_default();
    let name =
        extract_user_bullet_value(&user_doc, "Name").unwrap_or_else(|| "(unset)".to_string());
    let tz =
        extract_user_bullet_value(&user_doc, "Timezone").unwrap_or_else(|| "(unset)".to_string());

    let (pinned, prefs, kb_links) = memory_md_path(workspace_dir)
        .and_then(|p| read_to_string_if_file(&p))
        .map(|doc| count_memory_bullets_by_section(&doc))
        .unwrap_or((0, 0, 0));

    let memory_dir = workspace_dir.join("memory");
    let mut memory_files: Vec<String> = Vec::new();
    let mut auto_notes = 0usize;
    if memory_dir.is_dir() {
        for path in collect_markdown_files(&memory_dir, 200) {
            let rel = normalize_rel_path(workspace_dir, &path);
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
    out.push_str("Workspace memory:\n");
    out.push_str(&format!("- Workspace: {}\n", workspace_dir.display()));
    out.push_str(&format!("- Profile: Name={}, Timezone={}\n", name, tz));
    out.push_str(&format!(
        "- MEMORY.md: Pinned={}, Preferences={}, Knowledge base links={}\n",
        pinned, prefs, kb_links
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

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn temp_root(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "drbot-workspace-memory-view-test-{}-{}",
            name,
            Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn profile_overview_includes_fields() {
        let root = temp_root("profile");
        std::fs::write(
            root.join("USER.md"),
            r#"# User

- Name: Alice
- Timezone: Etc/UTC
- Preferred tone/style: professional
- Formatting preferences (e.g., bullets, terse/verbose): bullets
- Defaults (units, currency, locale): metric units
- Avoid / don't do: no emojis
"#,
        )
        .expect("write USER.md");

        let out = build_workspace_profile_overview(&root);
        assert!(out.contains("Name: Alice"));
        assert!(out.contains("Timezone: Etc/UTC"));
        assert!(out.contains("Formatting: bullets"));
    }

    #[test]
    fn memory_overview_counts_notes_and_sections() {
        let root = temp_root("memory");
        std::fs::create_dir_all(root.join("memory").join("auto")).expect("create memory dir");
        std::fs::write(
            root.join("memory").join("projects.md"),
            "# Projects\n\nWe use Postgres.\n",
        )
        .expect("write projects");
        std::fs::write(
            root.join("memory")
                .join("auto")
                .join(format!("note-{}.md", Uuid::new_v4())),
            "# Note\n\nLong note.\n",
        )
        .expect("write auto note");
        std::fs::write(
            root.join("USER.md"),
            r#"# User

- Name: Alice
- Timezone: Etc/UTC
"#,
        )
        .expect("write USER.md");
        std::fs::write(
            root.join("MEMORY.md"),
            r#"# Memory

## Pinned

- We use Postgres.

## Preferences

- no emojis

## Knowledge base

- memory/projects.md: projects
"#,
        )
        .expect("write MEMORY.md");

        let out = build_workspace_memory_overview(&root);
        assert!(out.contains("Pinned=1"));
        assert!(out.contains("Preferences=1"));
        assert!(out.contains("Knowledge base links=1"));
        assert!(out.contains("memory/projects.md"));
    }
}
