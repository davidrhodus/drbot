use crate::IronWorkflowManifest;
use anyhow::Context;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use serde_json::json;
use std::fs::File;
use std::io::Write;
use std::path::Path;

/// Create a `.tar.gz` bundle containing `iron.json`, the compiled WASM, and (optionally) `wit/workflow.wit`.
pub fn create_bundle_tar_gz(
    workflow_dir: &Path,
    out_path: &Path,
    include_wit: bool,
) -> anyhow::Result<()> {
    let manifest_path = workflow_dir.join("iron.json");
    let manifest = IronWorkflowManifest::load(&manifest_path)?;

    let wasm_path = workflow_dir.join(manifest.wasm_file.as_str());
    if !wasm_path.exists() {
        return Err(anyhow::anyhow!(
            "workflow wasmFile not found: {}",
            wasm_path.display()
        ));
    }

    let parent = out_path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)
        .with_context(|| format!("failed to create output dir: {}", parent.display()))?;

    let file = File::create(out_path)
        .with_context(|| format!("failed to create bundle: {}", out_path.display()))?;
    let encoder = GzEncoder::new(file, Compression::default());
    let mut builder = tar::Builder::new(encoder);

    // Manifest
    builder
        .append_path_with_name(&manifest_path, "iron.json")
        .with_context(|| format!("failed to add {}", manifest_path.display()))?;

    // WASM (use the manifest-relative path inside the archive)
    let wasm_rel = manifest.wasm_file.replace('\\', "/");
    builder
        .append_path_with_name(&wasm_path, wasm_rel.as_str())
        .with_context(|| format!("failed to add {}", wasm_path.display()))?;

    // WIT (optional)
    if include_wit {
        let wit_path = workflow_dir.join("wit").join("workflow.wit");
        if wit_path.exists() {
            builder
                .append_path_with_name(&wit_path, "wit/workflow.wit")
                .with_context(|| format!("failed to add {}", wit_path.display()))?;
        }
    }

    // Metadata (best-effort, informational)
    let meta = serde_json::to_vec_pretty(&json!({
        "format": "tar.gz",
        "manifest": manifest,
    }))
    .context("failed to encode bundle metadata")?;

    let mut header = tar::Header::new_gnu();
    header.set_size(meta.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    builder
        .append_data(&mut header, "bundle.json", meta.as_slice())
        .context("failed to add bundle.json")?;

    let encoder = builder.into_inner().context("failed to finish tar")?;
    let mut file = encoder.finish().context("failed to finish gzip")?;
    file.flush().ok();
    Ok(())
}

/// Unpack a `.tar.gz` bundle into `dst`.
///
/// This performs a basic validation after extraction that `iron.json` exists and the referenced WASM exists.
pub fn unpack_bundle_tar_gz(bundle_path: &Path, dst: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(dst)
        .with_context(|| format!("failed to create dst dir: {}", dst.display()))?;

    let file = File::open(bundle_path)
        .with_context(|| format!("failed to open bundle: {}", bundle_path.display()))?;
    let decoder = GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);
    let entries = archive.entries().context("failed to read bundle entries")?;

    for entry in entries {
        let mut entry = entry.context("failed to read bundle entry")?;
        entry
            .unpack_in(dst)
            .with_context(|| format!("failed to unpack entry into {}", dst.display()))?;
    }

    // Validate
    let manifest_path = dst.join("iron.json");
    let manifest = IronWorkflowManifest::load(&manifest_path)?;
    let wasm_path = dst.join(manifest.wasm_file.as_str());
    if !wasm_path.exists() {
        return Err(anyhow::anyhow!(
            "bundle missing workflow wasmFile: {}",
            wasm_path.display()
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn tmp_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("{}-{}", name, uuid::Uuid::new_v4()))
    }

    #[test]
    fn bundle_roundtrip() {
        let src = tmp_dir("drbot-iron-bundle-src");
        let dst = tmp_dir("drbot-iron-bundle-dst");
        let out = tmp_dir("drbot-iron-bundle").with_extension("iron.tgz");

        std::fs::create_dir_all(src.join("dist")).unwrap();
        std::fs::create_dir_all(src.join("wit")).unwrap();

        let manifest = IronWorkflowManifest {
            name: "test".to_string(),
            version: "0.1.0".to_string(),
            wasm_file: "dist/workflow.wasm".to_string(),
            description: None,
        };
        IronWorkflowManifest::write(&src.join("iron.json"), &manifest).unwrap();
        std::fs::write(src.join("dist").join("workflow.wasm"), b"wasm-bytes").unwrap();
        std::fs::write(src.join("wit").join("workflow.wit"), "package drbot:iron@0.1.0;").unwrap();

        create_bundle_tar_gz(&src, &out, true).unwrap();
        unpack_bundle_tar_gz(&out, &dst).unwrap();

        let m2 = IronWorkflowManifest::load(&dst.join("iron.json")).unwrap();
        assert_eq!(m2.name, "test");
        assert_eq!(m2.wasm_file, "dist/workflow.wasm");
        assert_eq!(std::fs::read(dst.join("dist").join("workflow.wasm")).unwrap(), b"wasm-bytes");
        assert!(dst.join("wit").join("workflow.wit").exists());

        std::fs::remove_dir_all(&src).ok();
        std::fs::remove_dir_all(&dst).ok();
        std::fs::remove_file(&out).ok();
    }
}
