//! Embedding generation for semantic similarity.

use crate::{CacheError, Result};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Embedding vector type.
pub type Embedding = Vec<f32>;

/// Trait for embedding providers.
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    /// Generate embedding for text.
    async fn embed(&self, text: &str) -> Result<Embedding>;

    /// Generate embeddings for multiple texts.
    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Embedding>> {
        let mut embeddings = Vec::with_capacity(texts.len());
        for text in texts {
            embeddings.push(self.embed(text).await?);
        }
        Ok(embeddings)
    }

    /// Get the embedding dimension.
    fn dimension(&self) -> usize;
}

/// Local embedding using simple hashing (for testing/development).
/// In production, use a proper embedding model.
#[derive(Debug, Clone)]
pub struct LocalEmbedding {
    dimension: usize,
    cache: Arc<RwLock<HashMap<String, Embedding>>>,
}

impl LocalEmbedding {
    /// Create a new local embedding provider.
    pub fn new(dimension: usize) -> Self {
        Self {
            dimension,
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Simple hash-based embedding (for testing).
    fn hash_embed(&self, text: &str) -> Embedding {
        let mut embedding = vec![0.0f32; self.dimension];
        let normalized = text.to_lowercase();
        let words: Vec<&str> = normalized.split_whitespace().collect();

        for (i, word) in words.iter().enumerate() {
            let hash = simple_hash(word);
            let idx = (hash as usize) % self.dimension;
            embedding[idx] += 1.0 / (i + 1) as f32;
        }

        // Normalize
        let magnitude: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        if magnitude > 0.0 {
            for x in &mut embedding {
                *x /= magnitude;
            }
        }

        embedding
    }
}

#[async_trait]
impl EmbeddingProvider for LocalEmbedding {
    async fn embed(&self, text: &str) -> Result<Embedding> {
        // Check cache first
        {
            let cache = self.cache.read().await;
            if let Some(embedding) = cache.get(text) {
                return Ok(embedding.clone());
            }
        }

        // Generate embedding
        let embedding = self.hash_embed(text);

        // Cache it
        {
            let mut cache = self.cache.write().await;
            cache.insert(text.to_string(), embedding.clone());
        }

        Ok(embedding)
    }

    fn dimension(&self) -> usize {
        self.dimension
    }
}

impl Default for LocalEmbedding {
    fn default() -> Self {
        Self::new(384) // Common small embedding size
    }
}

/// Simple string hash function.
fn simple_hash(s: &str) -> u64 {
    let mut hash: u64 = 5381;
    for c in s.chars() {
        hash = hash.wrapping_mul(33).wrapping_add(c as u64);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_local_embedding() {
        let provider = LocalEmbedding::default();

        let embedding = provider.embed("hello world").await.unwrap();
        assert_eq!(embedding.len(), 384);

        // Same text should produce same embedding
        let embedding2 = provider.embed("hello world").await.unwrap();
        assert_eq!(embedding, embedding2);
    }

    #[tokio::test]
    async fn test_embedding_normalized() {
        let provider = LocalEmbedding::default();

        let embedding = provider.embed("test query").await.unwrap();
        let magnitude: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();

        // Should be normalized to ~1.0
        assert!((magnitude - 1.0).abs() < 0.01);
    }
}
