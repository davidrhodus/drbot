//! Workspace/tool-root path helpers for agent tools.
//!
//! These helpers are intentionally strict: they prevent `..` traversal and
//! refuse to operate through symlinks when creating new paths.

use std::path::{Component, Path, PathBuf};

pub fn ensure_root_dir(root: &Path) -> Result<PathBuf, String> {
    std::fs::create_dir_all(root).map_err(|e| format!("failed to create root dir: {}", e))?;
    root.canonicalize()
        .map_err(|e| format!("failed to canonicalize root dir: {}", e))
}

fn normalize_relative_path(input: &str) -> Result<PathBuf, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("path is empty".to_string());
    }
    if trimmed.starts_with('~') {
        return Err("tilde paths are not supported (use a relative path)".to_string());
    }

    let mut out = PathBuf::new();
    for comp in Path::new(trimmed).components() {
        match comp {
            Component::CurDir => {}
            Component::Normal(seg) => out.push(seg),
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(format!("invalid relative path: {}", input));
            }
        }
    }
    Ok(out)
}

pub fn resolve_existing_path(root: &Path, input: &str) -> Result<PathBuf, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("path is empty".to_string());
    }

    let candidate = Path::new(trimmed);
    let full = if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        root.join(normalize_relative_path(trimmed)?)
    };

    let canon = full
        .canonicalize()
        .map_err(|e| format!("failed to resolve '{}': {}", trimmed, e))?;
    if !canon.starts_with(root) {
        return Err(format!(
            "path '{}' is outside tool root '{}'",
            canon.display(),
            root.display()
        ));
    }
    Ok(canon)
}

pub fn join_relative(root: &Path, input: &str) -> Result<PathBuf, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("path is empty".to_string());
    }
    if Path::new(trimmed).is_absolute() {
        return Err("absolute paths are not allowed".to_string());
    }
    Ok(root.join(normalize_relative_path(trimmed)?))
}

pub fn resolve_existing_dir(root: &Path, input: &str) -> Result<PathBuf, String> {
    let canon = resolve_existing_path(root, input)?;
    if !canon.is_dir() {
        return Err(format!("not a directory: {}", input));
    }
    Ok(canon)
}

pub fn resolve_existing_file(root: &Path, input: &str) -> Result<PathBuf, String> {
    let canon = resolve_existing_path(root, input)?;
    if !canon.is_file() {
        return Err(format!("not a file: {}", input));
    }
    Ok(canon)
}

fn ensure_dir_tree_safe(root: &Path, rel_dir: &Path) -> Result<PathBuf, String> {
    let mut cur = root.to_path_buf();
    for comp in rel_dir.components() {
        match comp {
            Component::CurDir => continue,
            Component::Normal(seg) => {
                cur.push(seg);
                if cur.exists() {
                    let meta = std::fs::symlink_metadata(&cur)
                        .map_err(|e| format!("failed to stat '{}': {}", cur.display(), e))?;
                    if meta.file_type().is_symlink() {
                        return Err(format!("refusing to traverse symlink dir '{}'", cur.display()));
                    }
                    if !meta.is_dir() {
                        return Err(format!("not a directory: {}", cur.display()));
                    }
                } else {
                    std::fs::create_dir(&cur)
                        .map_err(|e| format!("failed to create dir '{}': {}", cur.display(), e))?;
                }
                let canon = cur
                    .canonicalize()
                    .map_err(|e| format!("failed to resolve dir '{}': {}", cur.display(), e))?;
                if !canon.starts_with(root) {
                    return Err(format!(
                        "path '{}' is outside tool root '{}'",
                        canon.display(),
                        root.display()
                    ));
                }
                cur = canon;
            }
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(format!("invalid relative dir: {}", rel_dir.display()));
            }
        }
    }
    Ok(cur)
}

pub fn resolve_write_file_path(root: &Path, input: &str) -> Result<PathBuf, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("path is empty".to_string());
    }
    if Path::new(trimmed).is_absolute() {
        return Err("absolute paths are not allowed for writes".to_string());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_root() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("drbot-tool-root-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        ensure_root_dir(&dir).unwrap()
    }

    #[test]
    fn join_relative_rejects_absolute_and_parent_dir() {
        let root = temp_root();
        assert!(join_relative(&root, "/etc/passwd").is_err());
        assert!(join_relative(&root, "../nope").is_err());
        assert!(join_relative(&root, "a/../b").is_err());
        assert!(join_relative(&root, "a/b.txt").is_ok());
    }

    #[test]
    fn resolve_existing_path_rejects_outside_root() {
        let root = temp_root();

        let outside = std::env::temp_dir().join(format!(
            "drbot-tool-root-outside-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&outside).unwrap();
        let outside_file = outside.join("x.txt");
        std::fs::write(&outside_file, "x").unwrap();

        let err = resolve_existing_path(&root, outside_file.to_string_lossy().as_ref())
            .unwrap_err()
            .to_string();
        assert!(err.contains("outside tool root"), "err={}", err);
    }

    #[test]
    fn resolve_write_file_path_refuses_absolute_and_root_write() {
        let root = temp_root();
        assert!(resolve_write_file_path(&root, "/tmp/x").is_err());
        assert!(resolve_write_file_path(&root, ".").is_err());
        assert!(resolve_write_file_path(&root, "").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn resolve_write_file_path_refuses_symlink_traversal() {
        use std::os::unix::fs::symlink;

        let root = temp_root();
        let outside = std::env::temp_dir().join(format!(
            "drbot-tool-root-symlink-outside-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&outside).unwrap();

        let link = root.join("link");
        symlink(&outside, &link).unwrap();

        let err = resolve_write_file_path(&root, "link/x.md")
            .unwrap_err()
            .to_string();
        assert!(err.contains("symlink"), "err={}", err);
    }
}

    let rel = normalize_relative_path(trimmed)?;
    if rel.as_os_str().is_empty() {
        return Err("refusing to write to the tool root directory".to_string());
    }
    let parent_rel = rel.parent().unwrap_or_else(|| Path::new(""));
    let _parent = ensure_dir_tree_safe(root, parent_rel)?;

    let full = root.join(&rel);
    if full.exists() {
        let meta = std::fs::symlink_metadata(&full)
            .map_err(|e| format!("failed to stat '{}': {}", full.display(), e))?;
        if meta.file_type().is_symlink() {
            return Err(format!("refusing to write through symlink '{}'", trimmed));
        }
        let canon = full
            .canonicalize()
            .map_err(|e| format!("failed to resolve '{}': {}", trimmed, e))?;
        if !canon.starts_with(root) {
            return Err(format!(
                "path '{}' is outside tool root '{}'",
                canon.display(),
                root.display()
            ));
        }
        Ok(canon)
    } else {
        Ok(full)
    }
}
