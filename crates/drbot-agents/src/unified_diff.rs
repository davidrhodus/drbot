//! Minimal unified-diff parser + applier for `apply_patch` tools.
//!
//! This is intentionally strict (context-matching) and only supports the subset
//! of unified diffs we need for agent patch application.

#[derive(Debug, Clone)]
pub struct UnifiedDiffHunk {
    pub old_start: usize,
    pub old_count: usize,
    pub new_start: usize,
    pub new_count: usize,
    pub lines: Vec<(char, String)>,
}

#[derive(Debug, Clone)]
pub struct UnifiedDiffFile {
    pub old_path: String,
    pub new_path: String,
    pub hunks: Vec<UnifiedDiffHunk>,
}

pub fn strip_unified_diff_path(raw: &str) -> String {
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

fn parse_unified_diff_hunk_header(line: &str) -> Result<(usize, usize, usize, usize), String> {
    let trimmed = line.trim();
    if !trimmed.starts_with("@@") {
        return Err(format!("invalid hunk header: {}", line));
    }
    let Some(end) = trimmed[2..].find("@@").map(|i| i + 2) else {
        return Err(format!("invalid hunk header: {}", line));
    };
    let body = trimmed[2..end].trim();
    let mut parts = body.split_whitespace();
    let old = parts
        .next()
        .ok_or_else(|| format!("invalid hunk header: {}", line))?;
    let new = parts
        .next()
        .ok_or_else(|| format!("invalid hunk header: {}", line))?;

    fn parse_range(token: &str, sigil: char) -> Result<(usize, usize), String> {
        let t = token
            .strip_prefix(sigil)
            .ok_or_else(|| format!("invalid hunk range: {}", token))?;
        let mut it = t.split(',');
        let start = it
            .next()
            .unwrap_or("")
            .parse::<usize>()
            .map_err(|_| format!("invalid hunk start: {}", token))?;
        let count = it
            .next()
            .map(|v| v.parse::<usize>())
            .transpose()
            .map_err(|_| format!("invalid hunk count: {}", token))?
            .unwrap_or(1);
        Ok((start, count))
    }

    let (old_start, old_count) = parse_range(old, '-')?;
    let (new_start, new_count) = parse_range(new, '+')?;
    Ok((old_start, old_count, new_start, new_count))
}

pub fn parse_unified_diff(patch: &str) -> Result<Vec<UnifiedDiffFile>, String> {
    let mut files: Vec<UnifiedDiffFile> = Vec::new();
    let mut cur_old: Option<String> = None;
    let mut cur_new: Option<String> = None;
    let mut cur_hunks: Vec<UnifiedDiffHunk> = Vec::new();
    let mut cur_hunk: Option<UnifiedDiffHunk> = None;

    fn finish_hunk(cur_hunks: &mut Vec<UnifiedDiffHunk>, cur_hunk: &mut Option<UnifiedDiffHunk>) {
        if let Some(h) = cur_hunk.take() {
            cur_hunks.push(h);
        }
    }

    fn finish_file(
        files: &mut Vec<UnifiedDiffFile>,
        cur_old: &mut Option<String>,
        cur_new: &mut Option<String>,
        cur_hunks: &mut Vec<UnifiedDiffHunk>,
        cur_hunk: &mut Option<UnifiedDiffHunk>,
    ) {
        finish_hunk(cur_hunks, cur_hunk);
        if let (Some(old_path), Some(new_path)) = (cur_old.take(), cur_new.take()) {
            files.push(UnifiedDiffFile {
                old_path,
                new_path,
                hunks: std::mem::take(cur_hunks),
            });
        } else {
            cur_old.take();
            cur_new.take();
            cur_hunks.clear();
        }
    }

    for line in patch.lines() {
        if let Some(rest) = line.strip_prefix("--- ") {
            finish_file(
                &mut files,
                &mut cur_old,
                &mut cur_new,
                &mut cur_hunks,
                &mut cur_hunk,
            );
            let token = rest.trim().split_whitespace().next().unwrap_or("");
            if token.is_empty() {
                return Err(format!("invalid --- line: {}", line));
            }
            cur_old = Some(strip_unified_diff_path(token));
            continue;
        }
        if let Some(rest) = line.strip_prefix("+++ ") {
            let token = rest.trim().split_whitespace().next().unwrap_or("");
            if token.is_empty() {
                return Err(format!("invalid +++ line: {}", line));
            }
            cur_new = Some(strip_unified_diff_path(token));
            continue;
        }

        if line.starts_with("@@") {
            finish_hunk(&mut cur_hunks, &mut cur_hunk);
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

        if let Some(h) = cur_hunk.as_mut() {
            if line.starts_with('\\') {
                // "\ No newline at end of file" -> ignore.
                continue;
            }
            let (kind, rest) = line.split_at(1);
            let kind = kind.chars().next().unwrap_or(' ');
            if !matches!(kind, ' ' | '-' | '+') {
                continue;
            }
            h.lines.push((kind, rest.to_string()));
        }
    }

    finish_file(
        &mut files,
        &mut cur_old,
        &mut cur_new,
        &mut cur_hunks,
        &mut cur_hunk,
    );
    Ok(files)
}

pub fn apply_unified_diff_to_text(
    original: &str,
    hunks: &[UnifiedDiffHunk],
) -> Result<String, String> {
    let normalized = original.replace("\r\n", "\n").replace('\r', "\n");
    let had_trailing_newline = normalized.ends_with('\n');
    let mut orig_lines: Vec<String> = normalized.lines().map(|s| s.to_string()).collect();
    if had_trailing_newline {
        if orig_lines.last().is_some_and(|l| l.is_empty()) {
            orig_lines.pop();
        }
    }

    let mut out: Vec<String> = Vec::new();
    let mut orig_idx: usize = 0;

    for hunk in hunks {
        let target = hunk.old_start.saturating_sub(1);
        if target < orig_idx {
            return Err("overlapping or out-of-order hunks".to_string());
        }
        if target > orig_lines.len() {
            return Err("hunk starts past end of file".to_string());
        }

        out.extend_from_slice(&orig_lines[orig_idx..target]);
        let expected_out_len = hunk.new_start.saturating_sub(1);
        if out.len() != expected_out_len {
            return Err(format!(
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
                    let cur = orig_lines
                        .get(pos)
                        .ok_or_else(|| format!("context past end of file at line {}", pos + 1))?;
                    if cur != text {
                        return Err(format!(
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
                    let cur = orig_lines
                        .get(pos)
                        .ok_or_else(|| format!("remove past end of file at line {}", pos + 1))?;
                    if cur != text {
                        return Err(format!(
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
                other => return Err(format!("unknown hunk line kind: {}", other)),
            }
        }

        let consumed_old = pos.saturating_sub(target);
        if consumed_old != hunk.old_count {
            return Err(format!(
                "hunk old_count mismatch at -{} (expected {}, got {})",
                hunk.old_start, hunk.old_count, consumed_old
            ));
        }
        let produced_new = out.len().saturating_sub(out_start_len);
        if produced_new != hunk.new_count {
            return Err(format!(
                "hunk new_count mismatch at +{} (expected {}, got {})",
                hunk.new_start, hunk.new_count, produced_new
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
