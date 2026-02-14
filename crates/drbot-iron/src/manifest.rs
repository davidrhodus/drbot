use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Manifest describing an Iron workflow artifact.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IronWorkflowManifest {
    pub name: String,
    pub version: String,

    /// Relative path to the compiled workflow component/module.
    #[serde(rename = "wasmFile")]
    pub wasm_file: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl IronWorkflowManifest {
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read manifest: {}", path.display()))?;
        let parsed = serde_json::from_str::<IronWorkflowManifest>(&raw)
            .with_context(|| format!("invalid manifest JSON: {}", path.display()))?;
        Ok(parsed)
    }

    pub fn write(path: &Path, manifest: &IronWorkflowManifest) -> anyhow::Result<()> {
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create dir: {}", parent.display()))?;
        let txt = serde_json::to_string_pretty(manifest)?;
        std::fs::write(path, txt)
            .with_context(|| format!("failed to write manifest: {}", path.display()))?;
        Ok(())
    }
}
