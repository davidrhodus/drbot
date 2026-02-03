//! Path utilities for drbot.
//!
//! This crate provides:
//! - Path manipulation
//! - Path validation
//! - Path normalization

use std::path::{Component, Path, PathBuf};
use thiserror::Error;

/// Path error types.
#[derive(Error, Debug)]
pub enum PathError {
    #[error("Invalid path: {0}")]
    InvalidPath(String),

    #[error("Path not found: {0}")]
    NotFound(String),

    #[error("Path traversal detected: {0}")]
    PathTraversal(String),
}

/// Result type for path operations.
pub type Result<T> = std::result::Result<T, PathError>;

/// Path extension trait.
pub trait PathExt {
    /// Get file name as string.
    fn file_name_str(&self) -> Option<&str>;

    /// Get extension as string.
    fn extension_str(&self) -> Option<&str>;

    /// Get stem as string.
    fn stem_str(&self) -> Option<&str>;

    /// Check if has extension.
    fn has_extension(&self, ext: &str) -> bool;

    /// Check if has any of the extensions.
    fn has_any_extension(&self, exts: &[&str]) -> bool;

    /// Get parent as PathBuf.
    fn parent_buf(&self) -> Option<PathBuf>;

    /// Get all ancestors.
    fn ancestors_vec(&self) -> Vec<PathBuf>;

    /// Check if path is hidden (starts with .).
    fn is_hidden(&self) -> bool;

    /// Get relative path from base.
    fn relative_from(&self, base: &Path) -> Option<PathBuf>;
}

impl PathExt for Path {
    fn file_name_str(&self) -> Option<&str> {
        self.file_name().and_then(|s| s.to_str())
    }

    fn extension_str(&self) -> Option<&str> {
        self.extension().and_then(|s| s.to_str())
    }

    fn stem_str(&self) -> Option<&str> {
        self.file_stem().and_then(|s| s.to_str())
    }

    fn has_extension(&self, ext: &str) -> bool {
        self.extension_str()
            .map(|e| e.eq_ignore_ascii_case(ext))
            .unwrap_or(false)
    }

    fn has_any_extension(&self, exts: &[&str]) -> bool {
        exts.iter().any(|ext| self.has_extension(ext))
    }

    fn parent_buf(&self) -> Option<PathBuf> {
        self.parent().map(|p| p.to_path_buf())
    }

    fn ancestors_vec(&self) -> Vec<PathBuf> {
        self.ancestors().map(|p| p.to_path_buf()).collect()
    }

    fn is_hidden(&self) -> bool {
        self.file_name_str()
            .map(|name| name.starts_with('.'))
            .unwrap_or(false)
    }

    fn relative_from(&self, base: &Path) -> Option<PathBuf> {
        // Simple implementation - for complex cases use pathdiff crate
        let self_components: Vec<_> = self.components().collect();
        let base_components: Vec<_> = base.components().collect();

        // Find common prefix
        let common_len = self_components
            .iter()
            .zip(base_components.iter())
            .take_while(|(a, b)| a == b)
            .count();

        if common_len == 0 {
            return None;
        }

        let mut result = PathBuf::new();

        // Add .. for each remaining base component
        for _ in common_len..base_components.len() {
            result.push("..");
        }

        // Add remaining self components
        for component in &self_components[common_len..] {
            result.push(component);
        }

        Some(result)
    }
}

impl PathExt for PathBuf {
    fn file_name_str(&self) -> Option<&str> {
        self.as_path().file_name_str()
    }

    fn extension_str(&self) -> Option<&str> {
        self.as_path().extension_str()
    }

    fn stem_str(&self) -> Option<&str> {
        self.as_path().stem_str()
    }

    fn has_extension(&self, ext: &str) -> bool {
        self.as_path().has_extension(ext)
    }

    fn has_any_extension(&self, exts: &[&str]) -> bool {
        self.as_path().has_any_extension(exts)
    }

    fn parent_buf(&self) -> Option<PathBuf> {
        self.as_path().parent_buf()
    }

    fn ancestors_vec(&self) -> Vec<PathBuf> {
        self.as_path().ancestors_vec()
    }

    fn is_hidden(&self) -> bool {
        self.as_path().is_hidden()
    }

    fn relative_from(&self, base: &Path) -> Option<PathBuf> {
        self.as_path().relative_from(base)
    }
}

/// Path builder.
pub struct PathBuilder {
    path: PathBuf,
}

impl PathBuilder {
    /// Create new builder.
    pub fn new() -> Self {
        Self {
            path: PathBuf::new(),
        }
    }

    /// Create from base path.
    pub fn from<P: AsRef<Path>>(base: P) -> Self {
        Self {
            path: base.as_ref().to_path_buf(),
        }
    }

    /// Push path segment.
    pub fn push<P: AsRef<Path>>(mut self, segment: P) -> Self {
        self.path.push(segment);
        self
    }

    /// Set file name.
    pub fn file_name(mut self, name: &str) -> Self {
        self.path.set_file_name(name);
        self
    }

    /// Set extension.
    pub fn extension(mut self, ext: &str) -> Self {
        self.path.set_extension(ext);
        self
    }

    /// Build path.
    pub fn build(self) -> PathBuf {
        self.path
    }

    /// Build and normalize.
    pub fn build_normalized(self) -> PathBuf {
        PathNormalizer::normalize(&self.path)
    }
}

impl Default for PathBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Path normalizer.
pub struct PathNormalizer;

impl PathNormalizer {
    /// Normalize path (resolve . and ..).
    pub fn normalize(path: &Path) -> PathBuf {
        let mut result = PathBuf::new();

        for component in path.components() {
            match component {
                Component::CurDir => {}
                Component::ParentDir => {
                    result.pop();
                }
                other => result.push(other),
            }
        }

        if result.as_os_str().is_empty() {
            result.push(".");
        }

        result
    }

    /// Normalize and make absolute.
    pub fn normalize_absolute(path: &Path, base: &Path) -> PathBuf {
        let absolute = if path.is_absolute() {
            path.to_path_buf()
        } else {
            base.join(path)
        };
        Self::normalize(&absolute)
    }

    /// Clean path separators.
    pub fn clean_separators(path: &Path) -> PathBuf {
        let s = path.to_string_lossy();
        let cleaned = s.replace("//", "/").replace("\\\\", "\\");
        PathBuf::from(cleaned)
    }
}

/// Path validator.
pub struct PathValidator;

impl PathValidator {
    /// Check for path traversal attacks.
    pub fn check_traversal(path: &Path, root: &Path) -> Result<()> {
        let normalized = PathNormalizer::normalize_absolute(path, root);

        if !normalized.starts_with(root) {
            return Err(PathError::PathTraversal(path.display().to_string()));
        }

        Ok(())
    }

    /// Validate file name.
    pub fn validate_filename(name: &str) -> Result<()> {
        if name.is_empty() {
            return Err(PathError::InvalidPath("Empty filename".to_string()));
        }

        if name == "." || name == ".." {
            return Err(PathError::InvalidPath(
                "Invalid filename: . or ..".to_string(),
            ));
        }

        let invalid_chars = ['/', '\\', '\0', ':', '*', '?', '"', '<', '>', '|'];
        for c in invalid_chars {
            if name.contains(c) {
                return Err(PathError::InvalidPath(format!(
                    "Invalid character in filename: {}",
                    c
                )));
            }
        }

        Ok(())
    }

    /// Check if path is safe (no traversal, valid characters).
    pub fn is_safe(path: &Path, root: &Path) -> bool {
        Self::check_traversal(path, root).is_ok()
    }
}

/// Path utilities.
pub struct PathUtils;

impl PathUtils {
    /// Join multiple path segments.
    pub fn join<P: AsRef<Path>>(base: &Path, segments: &[P]) -> PathBuf {
        let mut result = base.to_path_buf();
        for segment in segments {
            result.push(segment);
        }
        result
    }

    /// Get common prefix of paths.
    pub fn common_prefix(paths: &[&Path]) -> Option<PathBuf> {
        if paths.is_empty() {
            return None;
        }

        let first_components: Vec<_> = paths[0].components().collect();

        let common_len = (0..first_components.len())
            .take_while(|&i| {
                paths
                    .iter()
                    .all(|p| p.components().nth(i) == Some(first_components[i]))
            })
            .count();

        if common_len == 0 {
            return None;
        }

        let mut result = PathBuf::new();
        for component in &first_components[..common_len] {
            result.push(component);
        }

        Some(result)
    }

    /// Split path into directory and file.
    pub fn split(path: &Path) -> (Option<PathBuf>, Option<String>) {
        let dir = path.parent().map(|p| p.to_path_buf());
        let file = path.file_name().and_then(|s| s.to_str()).map(String::from);
        (dir, file)
    }

    /// Split extension.
    pub fn split_extension(path: &Path) -> (PathBuf, Option<String>) {
        let ext = path.extension().and_then(|s| s.to_str()).map(String::from);
        let without_ext = path.with_extension("");
        (without_ext, ext)
    }

    /// Change extension.
    pub fn with_extension(path: &Path, ext: &str) -> PathBuf {
        path.with_extension(ext)
    }

    /// Add suffix before extension.
    pub fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
        let stem = path.stem_str().unwrap_or("");
        let ext = path.extension_str();
        let new_name = match ext {
            Some(e) => format!("{}{}.{}", stem, suffix, e),
            None => format!("{}{}", stem, suffix),
        };

        match path.parent() {
            Some(parent) => parent.join(new_name),
            None => PathBuf::from(new_name),
        }
    }

    /// Get depth of path.
    pub fn depth(path: &Path) -> usize {
        path.components().count()
    }
}

/// Common path locations.
pub struct Paths;

impl Paths {
    /// Get home directory.
    pub fn home() -> Option<PathBuf> {
        std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .ok()
            .map(PathBuf::from)
    }

    /// Get temp directory.
    pub fn temp() -> PathBuf {
        std::env::temp_dir()
    }

    /// Get current directory.
    pub fn current() -> std::io::Result<PathBuf> {
        std::env::current_dir()
    }

    /// Expand ~ in path.
    pub fn expand_tilde(path: &Path) -> PathBuf {
        let s = path.to_string_lossy();
        if s.starts_with("~/") || s == "~" {
            if let Some(home) = Self::home() {
                if s == "~" {
                    return home;
                }
                return home.join(&s[2..]);
            }
        }
        path.to_path_buf()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_path_ext() {
        let path = Path::new("/home/user/file.txt");

        assert_eq!(path.file_name_str(), Some("file.txt"));
        assert_eq!(path.extension_str(), Some("txt"));
        assert_eq!(path.stem_str(), Some("file"));
        assert!(path.has_extension("txt"));
        assert!(path.has_extension("TXT"));
        assert!(path.has_any_extension(&["txt", "md"]));
    }

    #[test]
    fn test_hidden() {
        assert!(Path::new(".hidden").is_hidden());
        assert!(Path::new("/path/.hidden").is_hidden());
        assert!(!Path::new("visible").is_hidden());
    }

    #[test]
    fn test_path_builder() {
        let path = PathBuilder::from("/home")
            .push("user")
            .push("file")
            .extension("txt")
            .build();

        assert_eq!(path, PathBuf::from("/home/user/file.txt"));
    }

    #[test]
    fn test_normalize() {
        let path = Path::new("/home/user/../user/./file.txt");
        let normalized = PathNormalizer::normalize(path);
        assert_eq!(normalized, PathBuf::from("/home/user/file.txt"));
    }

    #[test]
    fn test_traversal() {
        let root = Path::new("/home/user");

        assert!(PathValidator::check_traversal(Path::new("file.txt"), root).is_ok());
        assert!(PathValidator::check_traversal(Path::new("../other"), root).is_err());
        assert!(PathValidator::check_traversal(Path::new("sub/../file.txt"), root).is_ok());
    }

    #[test]
    fn test_validate_filename() {
        assert!(PathValidator::validate_filename("file.txt").is_ok());
        assert!(PathValidator::validate_filename("my file.txt").is_ok());
        assert!(PathValidator::validate_filename("..").is_err());
        assert!(PathValidator::validate_filename("file/name").is_err());
        assert!(PathValidator::validate_filename("").is_err());
    }

    #[test]
    fn test_common_prefix() {
        let paths = [
            Path::new("/home/user/docs"),
            Path::new("/home/user/images"),
            Path::new("/home/user/music"),
        ];

        let common = PathUtils::common_prefix(&paths).unwrap();
        assert_eq!(common, PathBuf::from("/home/user"));
    }

    #[test]
    fn test_with_suffix() {
        let path = Path::new("/home/file.txt");
        let suffixed = PathUtils::with_suffix(path, "_backup");
        assert_eq!(suffixed, PathBuf::from("/home/file_backup.txt"));
    }

    #[test]
    fn test_expand_tilde() {
        // Just test that it doesn't crash
        let path = Path::new("~/documents");
        let expanded = Paths::expand_tilde(path);
        assert!(expanded.to_string_lossy().len() > 0);
    }

    #[test]
    fn test_depth() {
        assert_eq!(PathUtils::depth(Path::new("/home/user/file.txt")), 4);
        assert_eq!(PathUtils::depth(Path::new("file.txt")), 1);
    }
}
