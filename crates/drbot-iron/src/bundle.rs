use crate::manifest::{IronWorkflowIntegrity, IronWorkflowSignature};
use crate::IronWorkflowManifest;
use anyhow::Context;
use base64::Engine as _;
use flate2::read::GzDecoder;
use flate2::{Compression, GzBuilder};
use ring::digest;
use ring::signature;
use ring::signature::KeyPair;
use serde_json::json;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::Write;
use std::path::{Component, Path};

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = digest::digest(&digest::SHA256, bytes);
    hex_lower(digest.as_ref())
}

fn append_bytes<W: Write>(
    builder: &mut tar::Builder<W>,
    name: &str,
    bytes: &[u8],
) -> anyhow::Result<()> {
    let mut header = tar::Header::new_gnu();
    header.set_size(bytes.len() as u64);
    header.set_mode(0o644);
    header.set_uid(0);
    header.set_gid(0);
    header.set_mtime(0);
    header.set_cksum();
    builder
        .append_data(&mut header, name, bytes)
        .with_context(|| format!("failed to add {}", name))?;
    Ok(())
}

/// Create a `.tar.gz` bundle containing `iron.json`, the compiled WASM, and (optionally) `wit/workflow.wit`.
///
/// Bundles include integrity metadata (SHA-256 digests) and can optionally include an Ed25519 signature
/// over the manifest (excluding the `signature` field).
pub fn create_bundle_tar_gz(
    workflow_dir: &Path,
    out_path: &Path,
    include_wit: bool,
) -> anyhow::Result<()> {
    create_bundle_tar_gz_with_signing(workflow_dir, out_path, include_wit, None)
}

/// Same as [`create_bundle_tar_gz`], but signs the manifest with an Ed25519 seed (32 bytes).
pub fn create_bundle_tar_gz_signed(
    workflow_dir: &Path,
    out_path: &Path,
    include_wit: bool,
    ed25519_seed: &[u8; 32],
) -> anyhow::Result<()> {
    create_bundle_tar_gz_with_signing(workflow_dir, out_path, include_wit, Some(ed25519_seed))
}

fn create_bundle_tar_gz_with_signing(
    workflow_dir: &Path,
    out_path: &Path,
    include_wit: bool,
    ed25519_seed: Option<&[u8; 32]>,
) -> anyhow::Result<()> {
    let manifest_path = workflow_dir.join("iron.json");
    let mut manifest = IronWorkflowManifest::load(&manifest_path)?;
    manifest.capabilities = manifest.capabilities.normalized();

    let wasm_path = workflow_dir.join(manifest.wasm_file.as_str());
    if !wasm_path.exists() {
        return Err(anyhow::anyhow!(
            "workflow wasmFile not found: {}",
            wasm_path.display()
        ));
    }

    let wasm_rel = manifest.wasm_file.replace('\\', "/");
    let wasm_bytes = std::fs::read(&wasm_path)
        .with_context(|| format!("failed to read workflow wasmFile: {}", wasm_path.display()))?;

    let mut files: BTreeMap<String, String> = BTreeMap::new();
    files.insert(
        wasm_rel.clone(),
        format!("sha256:{}", sha256_hex(&wasm_bytes)),
    );

    let wit_bytes = if include_wit {
        let wit_path = workflow_dir.join("wit").join("workflow.wit");
        if wit_path.exists() {
            let bytes = std::fs::read(&wit_path)
                .with_context(|| format!("failed to read WIT: {}", wit_path.display()))?;
            files.insert(
                "wit/workflow.wit".to_string(),
                format!("sha256:{}", sha256_hex(&bytes)),
            );
            Some(bytes)
        } else {
            None
        }
    } else {
        None
    };

    manifest.integrity = Some(IronWorkflowIntegrity {
        algorithm: "sha256".to_string(),
        files,
    });

    if let Some(seed) = ed25519_seed {
        let keypair = signature::Ed25519KeyPair::from_seed_unchecked(seed)
            .map_err(|_| anyhow::anyhow!("invalid Ed25519 signing key seed"))?;

        let to_sign = manifest.canonical_bytes_for_signing()?;
        let sig = keypair.sign(&to_sign);

        let pub_b64 =
            base64::engine::general_purpose::STANDARD_NO_PAD.encode(keypair.public_key().as_ref());
        let sig_b64 = base64::engine::general_purpose::STANDARD_NO_PAD.encode(sig.as_ref());

        manifest.signature = Some(IronWorkflowSignature {
            algorithm: "ed25519".to_string(),
            public_key: format!("base64:{}", pub_b64),
            signature: format!("base64:{}", sig_b64),
        });
    }

    let parent = out_path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)
        .with_context(|| format!("failed to create output dir: {}", parent.display()))?;

    let file = File::create(out_path)
        .with_context(|| format!("failed to create bundle: {}", out_path.display()))?;
    let encoder = GzBuilder::new()
        .mtime(0)
        .write(file, Compression::default());
    let mut builder = tar::Builder::new(encoder);

    let manifest_txt =
        serde_json::to_string_pretty(&manifest).context("failed to encode workflow manifest")?;
    append_bytes(&mut builder, "iron.json", manifest_txt.as_bytes())?;
    append_bytes(&mut builder, "manifest.json", manifest_txt.as_bytes())?;

    append_bytes(&mut builder, wasm_rel.as_str(), &wasm_bytes)?;

    if let Some(wit_bytes) = wit_bytes.as_ref() {
        append_bytes(&mut builder, "wit/workflow.wit", wit_bytes)?;
    }

    let meta = serde_json::to_vec_pretty(&json!({
        "format": "tar.gz",
        "manifest": manifest,
    }))
    .context("failed to encode bundle metadata")?;
    append_bytes(&mut builder, "bundle.json", meta.as_slice())?;

    let encoder = builder.into_inner().context("failed to finish tar")?;
    let mut file = encoder.finish().context("failed to finish gzip")?;
    file.flush().ok();
    Ok(())
}

/// Unpack a `.tar.gz` bundle into `dst`.
///
/// Performs basic validation after extraction:
/// - `iron.json` exists
/// - the referenced WASM exists
/// - if integrity metadata is present, all digests match
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

    // Validate manifest + wasm.
    let manifest_path = dst.join("iron.json");
    let manifest = IronWorkflowManifest::load(&manifest_path)?;
    let wasm_path = dst.join(manifest.wasm_file.as_str());
    if !wasm_path.exists() {
        return Err(anyhow::anyhow!(
            "bundle missing workflow wasmFile: {}",
            wasm_path.display()
        ));
    }

    // Verify integrity if present.
    if let Some(integrity) = manifest.integrity.as_ref() {
        let algo = integrity.algorithm.trim().to_ascii_lowercase();
        if algo != "sha256" {
            return Err(anyhow::anyhow!(
                "unsupported integrity algorithm: {}",
                integrity.algorithm
            ));
        }

        for (rel, expected) in integrity.files.iter() {
            let rel_path = Path::new(rel);
            if rel_path.is_absolute()
                || rel_path
                    .components()
                    .any(|c| matches!(c, Component::ParentDir))
            {
                return Err(anyhow::anyhow!("invalid integrity path: {}", rel));
            }

            let file_path = dst.join(rel_path);
            let bytes = std::fs::read(&file_path)
                .with_context(|| format!("missing bundled file: {}", file_path.display()))?;
            let actual = sha256_hex(&bytes);
            let expected = expected.trim();
            let expected_hex = expected.strip_prefix("sha256:").unwrap_or(expected);
            if actual != expected_hex {
                return Err(anyhow::anyhow!(
                    "bundle integrity mismatch for {}: expected sha256:{}, got sha256:{}",
                    rel,
                    expected_hex,
                    actual
                ));
            }
        }
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
    fn bundle_roundtrip_includes_integrity() {
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
            capabilities: Default::default(),
            integrity: None,
            signature: None,
        };
        IronWorkflowManifest::write(&src.join("iron.json"), &manifest).unwrap();
        std::fs::write(src.join("dist").join("workflow.wasm"), b"wasm-bytes").unwrap();
        std::fs::write(
            src.join("wit").join("workflow.wit"),
            "package drbot:iron@0.1.0;",
        )
        .unwrap();

        create_bundle_tar_gz(&src, &out, true).unwrap();
        unpack_bundle_tar_gz(&out, &dst).unwrap();

        let m2 = IronWorkflowManifest::load(&dst.join("iron.json")).unwrap();
        assert_eq!(m2.name, "test");
        assert_eq!(m2.wasm_file, "dist/workflow.wasm");
        assert!(m2.integrity.is_some());
        let integrity = m2.integrity.unwrap();
        assert!(integrity.files.contains_key("dist/workflow.wasm"));
        assert!(integrity.files.contains_key("wit/workflow.wit"));

        assert_eq!(
            std::fs::read(dst.join("dist").join("workflow.wasm")).unwrap(),
            b"wasm-bytes"
        );
        assert!(dst.join("wit").join("workflow.wit").exists());

        std::fs::remove_dir_all(&src).ok();
        std::fs::remove_dir_all(&dst).ok();
        std::fs::remove_file(&out).ok();
    }
}
