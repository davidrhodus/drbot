//! Skill installation from various sources.

use crate::{Result, SkillError, SkillManifest};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use tokio::process::Command;
use uuid::Uuid;

/// Installation source.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InstallSource {
    /// Local directory.
    Local { path: PathBuf },
    /// Git repository.
    Git { url: String, branch: Option<String> },
    /// URL to download.
    Url { url: String },
    /// Registry (future).
    Registry {
        name: String,
        version: Option<String>,
    },
}

/// Installation result.
#[derive(Debug, Clone)]
pub struct InstallResult {
    /// Installed path.
    pub path: PathBuf,
    /// Skill manifest.
    pub manifest: SkillManifest,
    /// Whether this was an upgrade.
    pub upgraded: bool,
}

/// Skill installer.
pub struct SkillInstaller {
    /// Installation directory.
    install_dir: PathBuf,
}

impl SkillInstaller {
    /// Create a new installer.
    pub fn new(install_dir: impl AsRef<Path>) -> Self {
        Self {
            install_dir: install_dir.as_ref().to_path_buf(),
        }
    }

    /// Install a skill.
    pub async fn install(&self, source: &InstallSource) -> Result<InstallResult> {
        match source {
            InstallSource::Local { path } => self.install_local(path).await,
            InstallSource::Git { url, branch } => self.install_git(url, branch.as_deref()).await,
            InstallSource::Url { url } => self.install_url(url).await,
            InstallSource::Registry { name, version } => {
                self.install_registry(name, version.as_deref()).await
            }
        }
    }

    /// Install from a local directory.
    async fn install_local(&self, source_path: &Path) -> Result<InstallResult> {
        // Read manifest first
        let manifest_path = source_path.join("skill.toml");
        let manifest = SkillManifest::from_file(&manifest_path)?;
        manifest.validate()?;

        // Determine install path
        let install_path = self.install_dir.join(&manifest.name);
        let upgraded = install_path.exists();

        // Remove existing if present
        if upgraded {
            std::fs::remove_dir_all(&install_path)?;
        }

        // Copy files
        copy_dir_recursive(source_path, &install_path)?;

        tracing::info!(
            skill = %manifest.name,
            version = %manifest.version,
            upgraded = upgraded,
            "Installed skill"
        );

        Ok(InstallResult {
            path: install_path,
            manifest,
            upgraded,
        })
    }

    /// Install from a Git repository.
    async fn install_git(&self, url: &str, branch: Option<&str>) -> Result<InstallResult> {
        let temp_dir = create_temp_dir("drbot-skill-git")?;

        let mut cmd = Command::new("git");
        cmd.arg("clone").arg("--depth").arg("1");
        if let Some(branch) = branch {
            cmd.arg("--branch").arg(branch).arg("--single-branch");
        }
        cmd.arg(url).arg(&temp_dir);

        let output = cmd
            .output()
            .await
            .map_err(|e| SkillError::InstallationFailed(format!("Failed to run git: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            let msg = if !stderr.trim().is_empty() {
                stderr.trim().to_string()
            } else if !stdout.trim().is_empty() {
                stdout.trim().to_string()
            } else {
                format!("git clone failed with status {}", output.status)
            };

            let _ = std::fs::remove_dir_all(&temp_dir);
            return Err(SkillError::InstallationFailed(msg));
        }

        // Don't install VCS metadata.
        let _ = std::fs::remove_dir_all(temp_dir.join(".git"));

        let source_root = find_skill_root(&temp_dir).unwrap_or_else(|| temp_dir.clone());
        let result = self.install_local(&source_root).await;

        let _ = std::fs::remove_dir_all(&temp_dir);
        result
    }

    /// Install from a URL.
    async fn install_url(&self, url: &str) -> Result<InstallResult> {
        let temp_dir = create_temp_dir("drbot-skill-url")?;

        let client = reqwest::Client::new();
        let resp = client.get(url).send().await.map_err(|e| {
            SkillError::InstallationFailed(format!("Failed to download {url}: {e}"))
        })?;

        let status = resp.status();
        let body = resp.bytes().await.map_err(|e| {
            SkillError::InstallationFailed(format!("Failed to read download body: {e}"))
        })?;

        if !status.is_success() {
            let msg = String::from_utf8_lossy(&body);
            let _ = std::fs::remove_dir_all(&temp_dir);
            return Err(SkillError::InstallationFailed(format!(
                "Download failed ({}): {}",
                status.as_u16(),
                msg
            )));
        }

        let url_lc = url.to_ascii_lowercase();
        let bytes = body.to_vec();

        let extract_result = if url_lc.ends_with(".zip") {
            extract_zip(&bytes, &temp_dir)
        } else if url_lc.ends_with(".tar.gz") || url_lc.ends_with(".tgz") {
            extract_tar_gz(&bytes, &temp_dir)
        } else if url_lc.ends_with(".tar") {
            extract_tar(&bytes, &temp_dir)
        } else {
            Err(SkillError::InstallationFailed(
                "Unsupported URL format (expected .zip, .tar.gz, .tgz, or .tar)".into(),
            ))
        };

        if let Err(e) = extract_result {
            let _ = std::fs::remove_dir_all(&temp_dir);
            return Err(e);
        }

        let source_root = find_skill_root(&temp_dir).ok_or_else(|| {
            SkillError::InvalidManifest(format!(
                "No skill.toml found in extracted archive from {url}"
            ))
        })?;

        let result = self.install_local(&source_root).await;
        let _ = std::fs::remove_dir_all(&temp_dir);
        result
    }

    /// Install from a registry.
    async fn install_registry(&self, name: &str, version: Option<&str>) -> Result<InstallResult> {
        let index = std::env::var("DRBOT_SKILL_REGISTRY_INDEX").map_err(|_| {
            SkillError::InstallationFailed(
                "Skill registry not configured (set DRBOT_SKILL_REGISTRY_INDEX)".into(),
            )
        })?;

        let index_text = if index.starts_with("http://") || index.starts_with("https://") {
            let client = reqwest::Client::new();
            let resp = client.get(&index).send().await.map_err(|e| {
                SkillError::InstallationFailed(format!("Failed to fetch registry index: {e}"))
            })?;
            let status = resp.status();
            let body = resp.text().await.map_err(|e| {
                SkillError::InstallationFailed(format!("Failed to read registry index: {e}"))
            })?;
            if !status.is_success() {
                return Err(SkillError::InstallationFailed(format!(
                    "Registry index fetch failed ({}): {}",
                    status.as_u16(),
                    body
                )));
            }
            body
        } else {
            std::fs::read_to_string(&index)?
        };

        #[derive(Debug, Deserialize)]
        struct RegistryIndex {
            skills: HashMap<String, InstallSource>,
        }

        let parsed: RegistryIndex = toml::from_str(&index_text)
            .or_else(|_| serde_json::from_str(&index_text))
            .map_err(|e| SkillError::InstallationFailed(format!("Invalid registry index: {e}")))?;

        let mut source = parsed
            .skills
            .get(name)
            .cloned()
            .ok_or_else(|| SkillError::NotFound(name.to_string()))?;

        if let Some(version) = version {
            source = apply_version_placeholders(&source, version);
        }

        Box::pin(self.install(&source)).await
    }

    /// Uninstall a skill.
    pub async fn uninstall(&self, name: &str) -> Result<()> {
        let install_path = self.install_dir.join(name);

        if !install_path.exists() {
            return Err(SkillError::NotFound(name.to_string()));
        }

        std::fs::remove_dir_all(&install_path)?;

        tracing::info!(skill = %name, "Uninstalled skill");
        Ok(())
    }

    /// List installed skills.
    pub fn list_installed(&self) -> Result<Vec<SkillManifest>> {
        let mut manifests = Vec::new();

        if !self.install_dir.exists() {
            return Ok(manifests);
        }

        for entry in std::fs::read_dir(&self.install_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                let manifest_path = path.join("skill.toml");
                if let Ok(manifest) = SkillManifest::from_file(&manifest_path) {
                    manifests.push(manifest);
                }
            }
        }

        Ok(manifests)
    }
}

/// Recursively copy a directory.
fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    std::fs::create_dir_all(dst)?;

    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let path = entry.path();
        let dest_path = dst.join(entry.file_name());

        if path.is_dir() {
            copy_dir_recursive(&path, &dest_path)?;
        } else {
            std::fs::copy(&path, &dest_path)?;
        }
    }

    Ok(())
}

fn create_temp_dir(prefix: &str) -> Result<PathBuf> {
    let dir = std::env::temp_dir().join(format!("{prefix}-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn find_skill_root(dir: &Path) -> Option<PathBuf> {
    find_skill_root_with_depth(dir, 4)
}

fn find_skill_root_with_depth(dir: &Path, depth: usize) -> Option<PathBuf> {
    if dir.join("skill.toml").exists() {
        return Some(dir.to_path_buf());
    }

    if depth == 0 {
        return None;
    }

    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find_skill_root_with_depth(&path, depth - 1) {
                return Some(found);
            }
        }
    }
    None
}

fn apply_version_placeholders(source: &InstallSource, version: &str) -> InstallSource {
    match source {
        InstallSource::Local { path } => InstallSource::Local { path: path.clone() },
        InstallSource::Git { url, branch } => InstallSource::Git {
            url: url.replace("{version}", version),
            branch: branch.clone(),
        },
        InstallSource::Url { url } => InstallSource::Url {
            url: url.replace("{version}", version),
        },
        InstallSource::Registry { name, version: v } => InstallSource::Registry {
            name: name.clone(),
            version: v.clone().or_else(|| Some(version.to_string())),
        },
    }
}

fn extract_zip(bytes: &[u8], dst: &Path) -> Result<()> {
    let cursor = Cursor::new(bytes);
    let mut archive =
        zip::ZipArchive::new(cursor).map_err(|e| SkillError::InstallationFailed(e.to_string()))?;

    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|e| SkillError::InstallationFailed(e.to_string()))?;

        let Some(enclosed) = file.enclosed_name().map(|p| p.to_owned()) else {
            continue;
        };
        let outpath = dst.join(enclosed);

        if file.is_dir() {
            std::fs::create_dir_all(&outpath)?;
            continue;
        }

        if let Some(parent) = outpath.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mut outfile = std::fs::File::create(&outpath)?;
        std::io::copy(&mut file, &mut outfile)?;
    }

    Ok(())
}

fn extract_tar(bytes: &[u8], dst: &Path) -> Result<()> {
    let cursor = Cursor::new(bytes);
    let mut archive = tar::Archive::new(cursor);
    let entries = archive
        .entries()
        .map_err(|e| SkillError::InstallationFailed(e.to_string()))?;

    for entry in entries {
        let mut entry = entry.map_err(|e| SkillError::InstallationFailed(e.to_string()))?;
        entry
            .unpack_in(dst)
            .map_err(|e| SkillError::InstallationFailed(e.to_string()))?;
    }

    Ok(())
}

fn extract_tar_gz(bytes: &[u8], dst: &Path) -> Result<()> {
    let cursor = Cursor::new(bytes);
    let decoder = flate2::read::GzDecoder::new(cursor);
    let mut archive = tar::Archive::new(decoder);
    let entries = archive
        .entries()
        .map_err(|e| SkillError::InstallationFailed(e.to_string()))?;

    for entry in entries {
        let mut entry = entry.map_err(|e| SkillError::InstallationFailed(e.to_string()))?;
        entry
            .unpack_in(dst)
            .map_err(|e| SkillError::InstallationFailed(e.to_string()))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_skill_installer() {
        let source_dir = TempDir::new().unwrap();
        let install_dir = TempDir::new().unwrap();

        // Create a test skill
        let manifest = r#"
            name = "test-skill"
            version = "1.0.0"
            description = "A test skill"
        "#;
        fs::write(source_dir.path().join("skill.toml"), manifest).unwrap();
        fs::write(source_dir.path().join("main.py"), "print('hello')").unwrap();

        let installer = SkillInstaller::new(install_dir.path());

        let result = installer
            .install(&InstallSource::Local {
                path: source_dir.path().to_path_buf(),
            })
            .await
            .unwrap();

        assert_eq!(result.manifest.name, "test-skill");
        assert!(!result.upgraded);
        assert!(result.path.exists());

        let installed = installer.list_installed().unwrap();
        assert_eq!(installed.len(), 1);

        installer.uninstall("test-skill").await.unwrap();
        assert!(installer.list_installed().unwrap().is_empty());
    }
}
