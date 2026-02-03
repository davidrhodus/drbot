//! Similarity metrics for embedding comparison.

use crate::embedding::Embedding;

/// Trait for similarity metrics.
pub trait SimilarityMetric: Send + Sync {
    /// Calculate similarity between two embeddings.
    /// Returns a value between 0.0 (no similarity) and 1.0 (identical).
    fn similarity(&self, a: &Embedding, b: &Embedding) -> f32;

    /// Name of the metric.
    fn name(&self) -> &str;
}

/// Cosine similarity metric.
#[derive(Debug, Clone, Copy, Default)]
pub struct CosineSimilarity;

impl SimilarityMetric for CosineSimilarity {
    fn similarity(&self, a: &Embedding, b: &Embedding) -> f32 {
        if a.len() != b.len() {
            return 0.0;
        }

        let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let magnitude_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let magnitude_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

        if magnitude_a == 0.0 || magnitude_b == 0.0 {
            return 0.0;
        }

        // Clamp to [0, 1] range
        ((dot_product / (magnitude_a * magnitude_b)) + 1.0) / 2.0
    }

    fn name(&self) -> &str {
        "cosine"
    }
}

/// Euclidean distance metric (converted to similarity).
#[derive(Debug, Clone, Copy, Default)]
pub struct EuclideanSimilarity;

impl SimilarityMetric for EuclideanSimilarity {
    fn similarity(&self, a: &Embedding, b: &Embedding) -> f32 {
        if a.len() != b.len() {
            return 0.0;
        }

        let distance: f32 = a
            .iter()
            .zip(b.iter())
            .map(|(x, y)| (x - y).powi(2))
            .sum::<f32>()
            .sqrt();

        // Convert distance to similarity (closer = higher similarity)
        1.0 / (1.0 + distance)
    }

    fn name(&self) -> &str {
        "euclidean"
    }
}

/// Dot product similarity.
#[derive(Debug, Clone, Copy, Default)]
pub struct DotProductSimilarity;

impl SimilarityMetric for DotProductSimilarity {
    fn similarity(&self, a: &Embedding, b: &Embedding) -> f32 {
        if a.len() != b.len() {
            return 0.0;
        }

        let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();

        // Normalize to [0, 1] assuming normalized embeddings
        (dot_product + 1.0) / 2.0
    }

    fn name(&self) -> &str {
        "dot_product"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_identical() {
        let metric = CosineSimilarity;
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];

        let sim = metric.similarity(&a, &b);
        assert!((sim - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_cosine_orthogonal() {
        let metric = CosineSimilarity;
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];

        let sim = metric.similarity(&a, &b);
        assert!((sim - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_euclidean_identical() {
        let metric = EuclideanSimilarity;
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![1.0, 2.0, 3.0];

        let sim = metric.similarity(&a, &b);
        assert!((sim - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_different_lengths() {
        let metric = CosineSimilarity;
        let a = vec![1.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];

        let sim = metric.similarity(&a, &b);
        assert_eq!(sim, 0.0);
    }
}
