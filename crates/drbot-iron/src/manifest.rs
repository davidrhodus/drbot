use anyhow::Context;
use base64::Engine as _;
use ring::signature;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

fn default_integrity_algorithm() -> String {
    "sha256".to_string()
}

fn default_signature_algorithm() -> String {
    "ed25519".to_string()
}

pub fn decode_base64_maybe_prefixed(input: &str) -> anyhow::Result<Vec<u8>> {
    let raw = input.trim();
    let raw = raw
        .strip_prefix("base64:")
        .or_else(|| raw.strip_prefix("b64:"))
        .unwrap_or(raw)
        .trim();

    base64::engine::general_purpose::STANDARD
        .decode(raw)
        .or_else(|_| base64::engine::general_purpose::STANDARD_NO_PAD.decode(raw))
        .with_context(|| "invalid base64")
}

/// Declared host capabilities required by an Iron workflow.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IronWorkflowCapabilities {
    /// Tool names the workflow may invoke via `host.tool-invoke`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<String>,

    /// HTTP policy hints/requirements.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub http: Option<IronHttpCapability>,

    /// Secret names the workflow may request via `secrets.get`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub secrets: Vec<String>,
}

impl IronWorkflowCapabilities {
    pub fn is_empty(&self) -> bool {
        self.tools.is_empty() && self.http.is_none() && self.secrets.is_empty()
    }

    pub fn normalized(&self) -> IronWorkflowCapabilities {
        let mut out = self.clone();
        out.tools.sort();
        out.tools.dedup();
        out.secrets.sort();
        out.secrets.dedup();
        if let Some(http) = out.http.as_mut() {
            http.allow_domains.sort();
            http.allow_domains.dedup();
        }
        out
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IronHttpCapability {
    #[serde(
        default,
        rename = "allowDomains",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub allow_domains: Vec<String>,

    #[serde(default, rename = "timeoutMs", skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,

    #[serde(default, rename = "maxBytes", skip_serializing_if = "Option::is_none")]
    pub max_bytes: Option<u64>,
}

/// Integrity information for a workflow artifact.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IronWorkflowIntegrity {
    #[serde(default = "default_integrity_algorithm")]
    pub algorithm: String,

    /// Map of bundle-relative file paths to digests (e.g. `sha256:<hex>`).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub files: BTreeMap<String, String>,
}

impl Default for IronWorkflowIntegrity {
    fn default() -> Self {
        Self {
            algorithm: default_integrity_algorithm(),
            files: BTreeMap::new(),
        }
    }
}

/// Signature over the manifest (excluding the `signature` field).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IronWorkflowSignature {
    #[serde(default = "default_signature_algorithm")]
    pub algorithm: String,

    /// Public key bytes (base64 or `base64:<...>`).
    #[serde(rename = "publicKey")]
    pub public_key: String,

    /// Signature bytes (base64 or `base64:<...>`).
    pub signature: String,
}

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

    /// Declared capabilities required by this workflow.
    #[serde(default, skip_serializing_if = "IronWorkflowCapabilities::is_empty")]
    pub capabilities: IronWorkflowCapabilities,

    /// Optional integrity metadata (typically injected during bundling).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub integrity: Option<IronWorkflowIntegrity>,

    /// Optional manifest signature.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<IronWorkflowSignature>,
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

    /// Produces deterministic JSON bytes suitable for signing/verification.
    pub fn canonical_bytes_for_signing(&self) -> anyhow::Result<Vec<u8>> {
        let mut m = self.clone();
        m.signature = None;
        m.capabilities = m.capabilities.normalized();
        if let Some(integrity) = m.integrity.as_mut() {
            if integrity.algorithm.trim().is_empty() {
                integrity.algorithm = default_integrity_algorithm();
            }
        }
        serde_json::to_vec(&m).context("failed to encode manifest for signing")
    }

    /// Verifies the embedded signature (if present) and returns the public key bytes.
    pub fn verify_embedded_signature(&self) -> anyhow::Result<Vec<u8>> {
        let sig = self
            .signature
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("manifest is not signed"))?;

        let algo = sig.algorithm.trim().to_ascii_lowercase();
        if algo != "ed25519" {
            return Err(anyhow::anyhow!(
                "unsupported signature algorithm: {}",
                sig.algorithm
            ));
        }

        let public_key = decode_base64_maybe_prefixed(sig.public_key.as_str())?;
        if public_key.len() != 32 {
            return Err(anyhow::anyhow!(
                "invalid Ed25519 public key length: {}",
                public_key.len()
            ));
        }

        let signature_bytes = decode_base64_maybe_prefixed(sig.signature.as_str())?;
        if signature_bytes.len() != 64 {
            return Err(anyhow::anyhow!(
                "invalid Ed25519 signature length: {}",
                signature_bytes.len()
            ));
        }

        let payload = self.canonical_bytes_for_signing()?;
        let key = signature::UnparsedPublicKey::new(&signature::ED25519, public_key.as_slice());
        key.verify(payload.as_slice(), signature_bytes.as_slice())
            .map_err(|_| anyhow::anyhow!("invalid signature"))?;

        Ok(public_key)
    }
}
