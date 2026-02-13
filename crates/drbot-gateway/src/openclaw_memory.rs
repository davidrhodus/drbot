//! Semantic memory helpers for OpenClaw-compatible runs.
//!
//! This module provides a lightweight, best-effort semantic recall layer:
//! - Store user/assistant turns with cheap local embeddings.
//! - Recall top similar snippets and inject into the system prompt.

use crate::state::GatewayState;
use drbot_memory::{Memory, SearchOptions};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::OnceLock;
use tracing::warn;

const DEFAULT_EMBED_DIM: usize = 384;
const DEFAULT_VOYAGE_BASE_URL: &str = "https://api.voyageai.com/v1";
const DEFAULT_VOYAGE_MODEL: &str = "voyage-4-large";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EmbeddingProviderChoice {
    Local,
    Voyage,
    Auto,
}

fn resolve_embedding_provider_choice() -> EmbeddingProviderChoice {
    let raw = std::env::var("DRBOT_OPENCLAW_MEMORY_EMBED_PROVIDER")
        .ok()
        .unwrap_or_else(|| "local".to_string());
    match raw.trim().to_ascii_lowercase().as_str() {
        "voyage" => EmbeddingProviderChoice::Voyage,
        "auto" => EmbeddingProviderChoice::Auto,
        _ => EmbeddingProviderChoice::Local,
    }
}

fn voyage_api_key() -> Option<String> {
    std::env::var("VOYAGE_API_KEY")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn normalize_voyage_model(model: &str) -> String {
    let trimmed = model.trim();
    if trimmed.is_empty() {
        return DEFAULT_VOYAGE_MODEL.to_string();
    }
    trimmed
        .strip_prefix("voyage/")
        .unwrap_or(trimmed)
        .to_string()
}

fn voyage_model() -> String {
    let raw = std::env::var("DRBOT_OPENCLAW_MEMORY_VOYAGE_MODEL")
        .ok()
        .unwrap_or_else(|| DEFAULT_VOYAGE_MODEL.to_string());
    normalize_voyage_model(&raw)
}

fn voyage_base_url() -> String {
    std::env::var("DRBOT_OPENCLAW_MEMORY_VOYAGE_BASE_URL")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT_VOYAGE_BASE_URL.to_string())
}

fn l2_normalize(mut embedding: Vec<f32>) -> Vec<f32> {
    let magnitude: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
    if magnitude > 0.0 {
        for x in &mut embedding {
            *x /= magnitude;
        }
    }
    embedding
}

#[derive(Debug, Serialize)]
struct VoyageEmbeddingsRequest {
    model: String,
    input: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    input_type: Option<String>,
}

#[derive(Debug, Deserialize)]
struct VoyageEmbeddingsResponse {
    data: Option<Vec<VoyageEmbeddingRow>>,
}

#[derive(Debug, Deserialize)]
struct VoyageEmbeddingRow {
    embedding: Option<Vec<f32>>,
}

static VOYAGE_HTTP: OnceLock<reqwest::Client> = OnceLock::new();

fn voyage_http() -> &'static reqwest::Client {
    VOYAGE_HTTP.get_or_init(|| {
        let ua = format!("drbot/{} (+openclaw-memory)", env!("CARGO_PKG_VERSION"));
        reqwest::Client::builder()
            .user_agent(ua)
            .timeout(std::time::Duration::from_secs(20))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new())
    })
}

async fn voyage_embed_batch(texts: &[String], input_type: &str) -> Result<Vec<Vec<f32>>, String> {
    if texts.is_empty() {
        return Ok(Vec::new());
    }
    let Some(api_key) = voyage_api_key() else {
        return Err("VOYAGE_API_KEY is not set".to_string());
    };
    let model = voyage_model();
    let base_url = voyage_base_url();
    let url = format!("{}/embeddings", base_url.trim_end_matches('/'));
    let body = VoyageEmbeddingsRequest {
        model: model.clone(),
        input: texts.to_vec(),
        input_type: Some(input_type.to_string()),
    };

    let res = voyage_http()
        .post(url)
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("voyage embeddings request failed: {}", e))?;

    if !res.status().is_success() {
        let status = res.status();
        let text = res.text().await.unwrap_or_default();
        return Err(format!("voyage embeddings failed: {} {}", status, text));
    }

    let payload: VoyageEmbeddingsResponse = res
        .json()
        .await
        .map_err(|e| format!("voyage embeddings parse failed: {}", e))?;
    let data = payload.data.unwrap_or_default();
    Ok(data
        .into_iter()
        .map(|row| row.embedding.unwrap_or_default())
        .collect())
}

fn should_use_voyage(choice: EmbeddingProviderChoice) -> bool {
    match choice {
        EmbeddingProviderChoice::Voyage => true,
        EmbeddingProviderChoice::Auto => voyage_api_key().is_some(),
        EmbeddingProviderChoice::Local => false,
    }
}

fn env_usize(key: &str, default: usize, min: usize, max: usize) -> usize {
    std::env::var(key)
        .ok()
        .and_then(|v| v.trim().parse::<usize>().ok())
        .unwrap_or(default)
        .clamp(min, max)
}

fn env_f32(key: &str, default: f32, min: f32, max: f32) -> f32 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.trim().parse::<f32>().ok())
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

fn simple_hash(s: &str) -> u64 {
    let mut hash: u64 = 5381;
    for c in s.chars() {
        hash = hash.wrapping_mul(33).wrapping_add(c as u64);
    }
    hash
}

fn local_embed(text: &str) -> Vec<f32> {
    let mut embedding = vec![0.0f32; DEFAULT_EMBED_DIM];

    let words: Vec<&str> = text.split_whitespace().collect();
    for (i, word) in words.iter().enumerate() {
        let hash = simple_hash(word);
        let index = (hash as usize) % DEFAULT_EMBED_DIM;
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

pub(crate) async fn store_turn_best_effort(
    state: &GatewayState,
    session_key: &str,
    run_id: &str,
    user_text: &str,
    assistant_text: &str,
) {
    let Some(store) = state.openclaw_memory_store().cloned() else {
        return;
    };

    let max_store_chars = env_usize(
        "DRBOT_OPENCLAW_MEMORY_MAX_STORE_CHARS",
        20_000,
        256,
        2_000_000,
    );

    let session_key = session_key.trim();
    if session_key.is_empty() {
        return;
    }

    let mut items: Vec<(&'static str, String)> = Vec::new();
    let user_trimmed = user_text.trim();
    if !user_trimmed.is_empty() {
        items.push(("user", truncate_chars(user_trimmed, max_store_chars)));
    }
    let assistant_trimmed = assistant_text.trim();
    if !assistant_trimmed.is_empty() {
        items.push((
            "assistant",
            truncate_chars(assistant_trimmed, max_store_chars),
        ));
    }
    if items.is_empty() {
        return;
    }

    let choice = resolve_embedding_provider_choice();
    let mut provider = "local";
    let mut model: Option<String> = None;
    let voyage_embeddings = if should_use_voyage(choice) {
        model = Some(voyage_model());
        match voyage_embed_batch(
            &items
                .iter()
                .map(|(_, text)| text.clone())
                .collect::<Vec<_>>(),
            "document",
        )
        .await
        {
            Ok(vecs) if vecs.len() == items.len() && !vecs.is_empty() => {
                provider = "voyage";
                Some(vecs)
            }
            Ok(_) => {
                warn!("openclaw memory: voyage embeddings returned unexpected shape; falling back to local");
                None
            }
            Err(err) => {
                warn!(error = %err, "openclaw memory: voyage embeddings failed; falling back to local");
                None
            }
        }
    } else {
        None
    };

    for (idx, (role, content)) in items.into_iter().enumerate() {
        let (embedding, used_provider, used_model) = if let Some(vecs) = voyage_embeddings.as_ref()
        {
            let vec = vecs.get(idx).cloned().unwrap_or_default();
            if vec.is_empty() {
                (local_embed(&content), "local", None)
            } else {
                (l2_normalize(vec), provider, model.clone())
            }
        } else {
            (local_embed(&content), "local", None)
        };

        let memory = Memory::new(session_key, role, content)
            .with_embedding(embedding.clone())
            .with_metadata(json!({
                "source": "openclaw",
                "kind": "turn",
                "runId": run_id,
                "embedding": {
                    "provider": used_provider,
                    "model": used_model,
                    "dim": embedding.len(),
                }
            }));
        if let Err(err) = store.store(&memory).await {
            warn!(error = %err, "openclaw memory: failed to store message");
        }
    }
}

pub(crate) async fn recall_prompt(
    state: &GatewayState,
    session_key: &str,
    query: &str,
) -> Option<String> {
    let Some(store) = state.openclaw_memory_store().cloned() else {
        return None;
    };

    let query = query.trim();
    if query.is_empty() {
        return None;
    }

    let max_results = env_usize("DRBOT_OPENCLAW_MEMORY_RECALL_MAX_RESULTS", 6, 1, 25);
    let min_score = env_f32("DRBOT_OPENCLAW_MEMORY_RECALL_MIN_SCORE", 0.25, 0.0, 0.99);
    let max_total_chars = env_usize(
        "DRBOT_OPENCLAW_MEMORY_RECALL_MAX_CHARS",
        4_000,
        256,
        200_000,
    );
    let max_item_chars = env_usize(
        "DRBOT_OPENCLAW_MEMORY_RECALL_MAX_ITEM_CHARS",
        600,
        64,
        20_000,
    );

    let choice = resolve_embedding_provider_choice();
    let embedding = if should_use_voyage(choice) {
        match voyage_embed_batch(&vec![query.to_string()], "query").await {
            Ok(vecs) => vecs
                .into_iter()
                .next()
                .filter(|v| !v.is_empty())
                .map(l2_normalize)
                .unwrap_or_else(|| local_embed(query)),
            Err(err) => {
                warn!(error = %err, "openclaw memory: voyage query embedding failed; falling back to local");
                local_embed(query)
            }
        }
    } else {
        local_embed(query)
    };
    let results = store
        .search(
            &embedding,
            SearchOptions::new()
                .session(session_key.trim())
                .limit(max_results)
                .min_score(min_score),
        )
        .await
        .ok()?;

    if results.is_empty() {
        return None;
    }

    let mut out = String::new();
    out.push_str("Relevant memories (semantic recall):\n");
    for r in results {
        let role = r.memory.role.trim();
        let role = if role.is_empty() { "memory" } else { role };
        let ts = r.memory.created_at.to_rfc3339();
        let content = truncate_chars(r.memory.content.trim(), max_item_chars);
        if content.is_empty() {
            continue;
        }
        out.push_str(&format!("- [{} @ {}] {}\n", role, ts, content));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn voyage_model_normalization() {
        assert_eq!(normalize_voyage_model(""), DEFAULT_VOYAGE_MODEL);
        assert_eq!(
            normalize_voyage_model("voyage/voyage-4-large"),
            "voyage-4-large"
        );
        assert_eq!(normalize_voyage_model("voyage-3"), "voyage-3");
        assert_eq!(normalize_voyage_model("  voyage/voyage-3  "), "voyage-3");
    }

    #[test]
    fn l2_normalize_unit_length() {
        let v = l2_normalize(vec![3.0, 4.0]);
        assert!((v[0] - 0.6).abs() < 1e-6);
        assert!((v[1] - 0.8).abs() < 1e-6);
    }

    #[test]
    fn local_embed_has_expected_dim() {
        let v = local_embed("hello world");
        assert_eq!(v.len(), DEFAULT_EMBED_DIM);
    }
}
