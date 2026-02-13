use std::path::{Component, Path, PathBuf};

/// Strip leading YAML frontmatter (`--- ... ---`) if present.
///
/// This is a best-effort utility for SKILL/HEARTBEAT style markdown docs.
pub fn strip_frontmatter(content: &str) -> String {
    let normalized = content.replace("\r\n", "\n").replace('\r', "\n");
    if !normalized.starts_with("---\n") {
        return normalized;
    }
    let Some(end_index) = normalized[3..].find("\n---").map(|i| i + 3) else {
        return normalized;
    };
    let start = (end_index + 4).min(normalized.len());
    normalized[start..].to_string()
}

pub fn is_markdown_doc_path(path: &str) -> bool {
    let lower = path.trim().to_ascii_lowercase();
    lower.ends_with(".md") || lower.ends_with(".markdown")
}

/// Extract raw inline markdown link targets: `[...] (target)` -> `target`.
///
/// Note: this intentionally does not attempt to fully parse markdown; it's a
/// lightweight heuristic used to discover additional `.md` docs referenced from
/// a skill document.
pub fn extract_markdown_inline_link_targets(markdown: &str) -> Vec<String> {
    let bytes = markdown.as_bytes();
    let mut out: Vec<String> = Vec::new();
    let mut i = 0usize;
    while i + 1 < bytes.len() {
        if bytes[i] == b']' && bytes[i + 1] == b'(' {
            let start = i + 2;
            let mut end = start;
            while end < bytes.len() && bytes[end] != b')' {
                end += 1;
            }
            if end >= bytes.len() {
                break;
            }
            if let Some(target) = markdown.get(start..end) {
                out.push(target.to_string());
            }
            i = end + 1;
            continue;
        }
        i += 1;
    }
    out
}

/// Extract markdown reference definition link targets: `[id]: target` -> `target`.
pub fn extract_markdown_reference_definition_targets(markdown: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in markdown.lines() {
        let trimmed = line.trim_start();
        if !trimmed.starts_with('[') {
            continue;
        }
        let Some(end) = trimmed.find("]:") else {
            continue;
        };
        let rest = trimmed[end + 2..].trim();
        let token = rest.split_whitespace().next().unwrap_or("").trim();
        if token.is_empty() {
            continue;
        }
        out.push(token.to_string());
    }
    out
}

/// Normalize a markdown link target into a relative markdown doc path (no `..`,
/// no absolute paths, no protocols).
pub fn normalize_relative_doc_path_from_target(target: &str) -> Option<PathBuf> {
    let trimmed = target.trim();
    if trimmed.is_empty() {
        return None;
    }
    let token = trimmed.split_whitespace().next().unwrap_or("").trim();
    let token = token.trim();
    if token.is_empty() || token.starts_with('#') {
        return None;
    }
    let token = token.trim_start_matches('<').trim_end_matches('>');
    if token.contains("://")
        || token.starts_with("mailto:")
        || token.starts_with("data:")
        || token.starts_with("javascript:")
    {
        return None;
    }
    let path_part = token
        .split(|c| c == '#' || c == '?')
        .next()
        .unwrap_or(token)
        .trim();
    if path_part.is_empty() || !is_markdown_doc_path(path_part) {
        return None;
    }

    let mut raw = path_part;
    while raw.starts_with("./") {
        raw = &raw[2..];
    }
    if raw.starts_with('/') || raw.starts_with('\\') {
        return None;
    }

    let mut out = PathBuf::new();
    for comp in Path::new(raw).components() {
        match comp {
            Component::CurDir => {}
            Component::Normal(seg) => out.push(seg),
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    if out.as_os_str().is_empty() {
        None
    } else {
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_frontmatter_removes_yaml_block() {
        let raw = "---\ntitle: test\n---\n\nhello\n";
        let out = strip_frontmatter(raw);
        assert_eq!(out, "\nhello\n");
    }

    #[test]
    fn extracts_inline_link_targets() {
        let md = "See [doc](docs/one.md) and [x](two.md#section).";
        let out = extract_markdown_inline_link_targets(md);
        assert_eq!(
            out,
            vec!["docs/one.md".to_string(), "two.md#section".to_string()]
        );
    }

    #[test]
    fn extracts_reference_definition_targets() {
        let md = "[a]: docs/a.md\n[b]: https://example.com\n";
        let out = extract_markdown_reference_definition_targets(md);
        assert_eq!(
            out,
            vec!["docs/a.md".to_string(), "https://example.com".to_string()]
        );
    }

    #[test]
    fn normalizes_relative_doc_paths() {
        assert_eq!(
            normalize_relative_doc_path_from_target("./docs/a.md")
                .unwrap()
                .to_string_lossy(),
            "docs/a.md"
        );
        assert_eq!(
            normalize_relative_doc_path_from_target("a.md#x")
                .unwrap()
                .to_string_lossy(),
            "a.md"
        );
        assert_eq!(
            normalize_relative_doc_path_from_target("a.md?x=1")
                .unwrap()
                .to_string_lossy(),
            "a.md"
        );
        assert!(normalize_relative_doc_path_from_target("../a.md").is_none());
        assert!(normalize_relative_doc_path_from_target("/a.md").is_none());
        assert!(normalize_relative_doc_path_from_target("https://x/a.md").is_none());
        assert!(normalize_relative_doc_path_from_target("#anchor").is_none());
    }
}
