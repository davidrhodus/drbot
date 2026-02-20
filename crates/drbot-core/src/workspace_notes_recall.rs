//! Best-effort semantic recall over workspace memory files.
//!
//! This is intended for chat runs so the assistant can reference user-maintained notes
//! without requiring explicit tool calls.

use std::path::{Path, PathBuf};

const EMBED_DIM: usize = 384;

fn env_usize(key: &str, default: usize, min: usize, max: usize) -> usize {
    std::env::var(key)
        .ok()
        .and_then(|v| v.trim().parse::<usize>().ok())
        .unwrap_or(default)
        .clamp(min, max)
}

fn env_f64(key: &str, default: f64, min: f64, max: f64) -> f64 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.trim().parse::<f64>().ok())
        .unwrap_or(default)
        .clamp(min, max)
}

fn truncate_chars(input: &str, max_chars: usize) -> String {
    if input.chars().count() <= max_chars {
        return input.to_string();
    }
    let out: String = input.chars().take(max_chars).collect();
    format!("{}…", out)
}

fn normalize_query_for_skip(query: &str) -> String {
    let mut out = String::new();
    let mut last_was_space = false;
    for ch in query.chars() {
        let ch = ch.to_ascii_lowercase();
        if ch.is_whitespace() {
            if !last_was_space {
                out.push(' ');
            }
            last_was_space = true;
            continue;
        }
        last_was_space = false;
        out.push(ch);
    }
    let out = out
        .trim()
        .trim_matches(|c: char| c.is_ascii_punctuation())
        .trim();
    out.to_string()
}

fn should_skip_recall(query: &str) -> bool {
    let q = normalize_query_for_skip(query);
    if q.is_empty() {
        return true;
    }
    matches!(
        q.as_str(),
        "ok" | "okay"
            | "k"
            | "kk"
            | "thanks"
            | "thank you"
            | "thx"
            | "ty"
            | "yep"
            | "yup"
            | "yes"
            | "no"
            | "nope"
            | "cool"
            | "nice"
            | "continue"
            | "go on"
            | "next"
            | "more"
            | "again"
            | "same"
            | "sounds good"
            | "got it"
            | "done"
    )
}

fn simple_hash(s: &str) -> u64 {
    let mut hash: u64 = 5381;
    for c in s.chars() {
        hash = hash.wrapping_mul(33).wrapping_add(c as u64);
    }
    hash
}

fn local_embed(text: &str) -> Vec<f32> {
    let mut embedding = vec![0.0f32; EMBED_DIM];
    let words: Vec<&str> = text.split_whitespace().collect();
    for (i, word) in words.iter().enumerate() {
        let hash = simple_hash(word);
        let index = (hash as usize) % EMBED_DIM;
        embedding[index] += 1.0 / (i + 1) as f32;
    }
    let magnitude: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
    if magnitude > 0.0 {
        for x in &mut embedding {
            *x /= magnitude;
        }
    }
    embedding
}

fn dot(a: &[f32], b: &[f32]) -> f64 {
    let len = a.len().min(b.len());
    let mut sum = 0.0f64;
    for i in 0..len {
        sum += (a[i] as f64) * (b[i] as f64);
    }
    sum
}

async fn collect_markdown_files(dir: &Path, max_files: usize) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    let mut stack: Vec<(PathBuf, usize)> = vec![(dir.to_path_buf(), 0)];

    while let Some((cur, depth)) = stack.pop() {
        if out.len() >= max_files {
            break;
        }
        if depth > 8 {
            continue;
        }
        let mut rd = match tokio::fs::read_dir(&cur).await {
            Ok(v) => v,
            Err(_) => continue,
        };
        loop {
            if out.len() >= max_files {
                break;
            }
            let entry = match rd.next_entry().await {
                Ok(Some(v)) => v,
                Ok(None) => break,
                Err(_) => break,
            };
            let path = entry.path();
            let ty = entry.file_type().await.ok();
            if ty.as_ref().map(|t| t.is_dir()).unwrap_or(false) {
                stack.push((path, depth + 1));
                continue;
            }
            if !ty.as_ref().map(|t| t.is_file()).unwrap_or(false) {
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

#[derive(Debug)]
struct Hit {
    score: f64,
    path: String,
    start_line: usize,
    snippet: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RecallMode {
    Auto,
    Explicit,
}

async fn collect_files_for_root(
    root: &Path,
    display_prefix: &str,
    max_files: usize,
) -> Vec<(String, PathBuf)> {
    let prefix = display_prefix.trim();
    let mut out: Vec<(String, PathBuf)> = Vec::new();

    let memory_md = root.join("MEMORY.md");
    if tokio::fs::metadata(&memory_md).await.is_ok() {
        out.push((format!("{prefix}MEMORY.md"), memory_md));
    } else {
        let memory_alt = root.join("memory.md");
        if tokio::fs::metadata(&memory_alt).await.is_ok() {
            out.push((format!("{prefix}memory.md"), memory_alt));
        }
    }

    if out.len() < max_files {
        let memory_dir = root.join("memory");
        if tokio::fs::metadata(&memory_dir).await.is_ok() {
            let remaining = max_files.saturating_sub(out.len());
            let more = collect_markdown_files(&memory_dir, remaining).await;
            for path in more {
                let rel = path
                    .strip_prefix(root)
                    .ok()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|| path.to_string_lossy().to_string())
                    .replace('\\', "/");
                if rel.eq_ignore_ascii_case("memory/README.md") {
                    continue;
                }
                out.push((format!("{prefix}{rel}"), path));
                if out.len() >= max_files {
                    break;
                }
            }
        }
    }

    out
}

async fn recall_workspace_notes_prompt_inner_multi(
    roots: &[(PathBuf, String)],
    query: &str,
    mode: RecallMode,
) -> Option<String> {
    let enabled = std::env::var("DRBOT_GATEWAY_WORKSPACE_NOTES_RECALL_ENABLED")
        .ok()
        .map(|v| {
            !matches!(
                v.trim().to_ascii_lowercase().as_str(),
                "0" | "false" | "no" | "off"
            )
        })
        .unwrap_or(true);
    if !enabled {
        return None;
    }

    const CHUNK_LINES: usize = 24;
    const CHUNK_OVERLAP: usize = 8;

    let query = query.trim();
    if query.is_empty() {
        return None;
    }
    if mode == RecallMode::Auto && should_skip_recall(query) {
        return None;
    }

    let max_files = env_usize(
        "DRBOT_GATEWAY_WORKSPACE_NOTES_RECALL_MAX_FILES",
        200,
        1,
        5000,
    );
    let max_bytes_per_file = env_usize(
        "DRBOT_GATEWAY_WORKSPACE_NOTES_RECALL_MAX_FILE_BYTES",
        512 * 1024,
        1024,
        50 * 1024 * 1024,
    );
    let max_results = env_usize("DRBOT_GATEWAY_WORKSPACE_NOTES_RECALL_MAX_RESULTS", 6, 1, 25);
    let min_score = env_f64(
        "DRBOT_GATEWAY_WORKSPACE_NOTES_RECALL_MIN_SCORE",
        0.18,
        0.0,
        0.99,
    );
    let max_total_chars = env_usize(
        "DRBOT_GATEWAY_WORKSPACE_NOTES_RECALL_MAX_CHARS",
        4_000,
        256,
        200_000,
    );
    let max_item_chars = env_usize(
        "DRBOT_GATEWAY_WORKSPACE_NOTES_RECALL_MAX_ITEM_CHARS",
        700,
        64,
        20_000,
    );

    let mut files: Vec<(String, PathBuf)> = Vec::new();
    for (root, prefix) in roots {
        if files.len() >= max_files {
            break;
        }
        let remaining = max_files.saturating_sub(files.len());
        let more = collect_files_for_root(root, prefix, remaining).await;
        files.extend(more);
    }

    if files.is_empty() {
        return None;
    }

    let query_lower = query.to_ascii_lowercase();
    let query_embedding = local_embed(query);
    let stride = CHUNK_LINES.saturating_sub(CHUNK_OVERLAP).max(1);

    let mut hits: Vec<Hit> = Vec::new();
    for (display_path, full_path) in files {
        let mut bytes = match tokio::fs::read(&full_path).await {
            Ok(v) => v,
            Err(_) => continue,
        };
        if bytes.len() > max_bytes_per_file {
            bytes.truncate(max_bytes_per_file);
        }
        let text = String::from_utf8_lossy(&bytes).to_string();
        if text.trim().is_empty() {
            continue;
        }
        let lines = text.lines().collect::<Vec<_>>();
        if lines.is_empty() {
            continue;
        }

        let mut idx = 0usize;
        while idx < lines.len() {
            let end_idx = (idx + CHUNK_LINES).min(lines.len());
            let snippet = lines[idx..end_idx].join("\n");
            let snippet_lower = snippet.to_ascii_lowercase();
            let lexical_match = query_lower.len() >= 3 && snippet_lower.contains(&query_lower);
            let lexical_boost = if lexical_match { 0.05 } else { 0.0 };
            let emb = local_embed(&snippet);
            let mut score = dot(&query_embedding, &emb) + lexical_boost;
            if !score.is_finite() {
                score = 0.0;
            }
            if lexical_match || score >= min_score {
                hits.push(Hit {
                    score,
                    path: display_path.clone(),
                    start_line: idx + 1,
                    snippet,
                });
            }
            idx = idx.saturating_add(stride);
        }
    }

    if hits.is_empty() {
        return None;
    }

    hits.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    hits.truncate(max_results);

    let mut out = String::new();
    out.push_str("Relevant notes (workspace knowledge base):\n");
    for hit in hits {
        let citation = format!("{}#L{}", hit.path, hit.start_line);
        let snippet = truncate_chars(hit.snippet.trim(), max_item_chars);
        if snippet.is_empty() {
            continue;
        }
        out.push_str(&format!("- [{}] {}\n", citation, snippet));
        if out.chars().count() >= max_total_chars {
            break;
        }
    }

    let out = truncate_chars(out.trim_end(), max_total_chars);
    if out.trim().is_empty() {
        None
    } else {
        Some(out)
    }
}

pub async fn recall_workspace_notes_prompt(root: &Path, query: &str) -> Option<String> {
    recall_workspace_notes_prompt_inner_multi(
        &[(root.to_path_buf(), String::new())],
        query,
        RecallMode::Auto,
    )
    .await
}

pub async fn recall_workspace_notes_prompt_explicit(root: &Path, query: &str) -> Option<String> {
    recall_workspace_notes_prompt_inner_multi(
        &[(root.to_path_buf(), String::new())],
        query,
        RecallMode::Explicit,
    )
    .await
}

pub async fn recall_project_notes_prompt(project_drbot_dir: &Path, query: &str) -> Option<String> {
    recall_workspace_notes_prompt_inner_multi(
        &[(project_drbot_dir.to_path_buf(), ".drbot/".to_string())],
        query,
        RecallMode::Auto,
    )
    .await
}

pub async fn recall_project_notes_prompt_explicit(
    project_drbot_dir: &Path,
    query: &str,
) -> Option<String> {
    recall_workspace_notes_prompt_inner_multi(
        &[(project_drbot_dir.to_path_buf(), ".drbot/".to_string())],
        query,
        RecallMode::Explicit,
    )
    .await
}

pub async fn recall_workspace_notes_prompt_with_project(
    workspace_root: &Path,
    project_drbot_dir: Option<&Path>,
    query: &str,
) -> Option<String> {
    let mut roots: Vec<(PathBuf, String)> = vec![(workspace_root.to_path_buf(), String::new())];
    if let Some(extra) = project_drbot_dir {
        if extra != workspace_root {
            roots.push((extra.to_path_buf(), ".drbot/".to_string()));
        }
    }
    recall_workspace_notes_prompt_inner_multi(&roots, query, RecallMode::Auto).await
}

pub async fn recall_workspace_notes_prompt_explicit_with_project(
    workspace_root: &Path,
    project_drbot_dir: Option<&Path>,
    query: &str,
) -> Option<String> {
    let mut roots: Vec<(PathBuf, String)> = vec![(workspace_root.to_path_buf(), String::new())];
    if let Some(extra) = project_drbot_dir {
        if extra != workspace_root {
            roots.push((extra.to_path_buf(), ".drbot/".to_string()));
        }
    }
    recall_workspace_notes_prompt_inner_multi(&roots, query, RecallMode::Explicit).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn temp_root(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "drbot-workspace-notes-recall-test-{}-{}",
            name,
            Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[tokio::test]
    async fn recall_returns_none_without_any_files() {
        let root = temp_root("none");
        let out = recall_workspace_notes_prompt(&root, "hello").await;
        assert!(out.is_none());
    }

    #[tokio::test]
    async fn recall_finds_lexical_match_in_memory_dir() {
        let root = temp_root("lexical");
        let memory_dir = root.join("memory");
        std::fs::create_dir_all(&memory_dir).expect("create memory dir");

        let token = format!("TOKEN_{}", Uuid::new_v4());
        std::fs::write(memory_dir.join("test.md"), format!("# Test\n\n{}\n", token))
            .expect("write note");

        let out = recall_workspace_notes_prompt(&root, &token)
            .await
            .expect("expected recall");
        assert!(out.contains("Relevant notes (workspace knowledge base):"));
        assert!(out.contains("memory/test.md#L"));
        assert!(out.contains(&token));
    }

    #[tokio::test]
    async fn recall_with_project_prefixes_paths() {
        let root = temp_root("project");
        let workspace_memory_dir = root.join("memory");
        std::fs::create_dir_all(&workspace_memory_dir).expect("create workspace memory dir");

        let project_root = root.join("repo");
        let project_drbot = project_root.join(".drbot");
        let project_memory_dir = project_drbot.join("memory");
        std::fs::create_dir_all(&project_memory_dir).expect("create project memory dir");

        let token = format!("TOKEN_{}", Uuid::new_v4());
        std::fs::write(
            project_memory_dir.join("proj.md"),
            format!("# Project\n\n{}\n", token),
        )
        .expect("write project note");

        let out = recall_workspace_notes_prompt_with_project(&root, Some(&project_drbot), &token)
            .await
            .expect("expected recall");
        assert!(out.contains(".drbot/memory/proj.md#L"));
        assert!(out.contains(&token));
    }
}
