//! Asset correlation analysis.
//!
//! Calculates correlation matrices between assets and identifies
//! highly correlated pairs that may represent hidden risk.

use super::PriceHistory;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Correlation matrix for portfolio assets.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorrelationMatrix {
    /// Asset names (row/column labels).
    pub assets: Vec<String>,
    /// NxN correlation matrix (row-major).
    pub matrix: Vec<Vec<f64>>,
    /// Pairs with high correlation.
    pub high_correlation_pairs: Vec<CorrelatedPair>,
}

impl CorrelationMatrix {
    /// Create a correlation matrix from price history.
    pub fn from_price_history(assets: &[String], history: &PriceHistory) -> Self {
        let n = assets.len();
        let mut matrix = vec![vec![0.0; n]; n];

        // Calculate returns for each asset
        let returns: Vec<Option<Vec<f64>>> =
            assets.iter().map(|a| history.daily_returns(a)).collect();

        // Calculate pairwise correlations
        for i in 0..n {
            for j in 0..n {
                if i == j {
                    matrix[i][j] = 1.0;
                } else if let (Some(ret_i), Some(ret_j)) = (&returns[i], &returns[j]) {
                    matrix[i][j] = calculate_correlation(ret_i, ret_j);
                }
            }
        }

        // Find high correlation pairs
        let high_correlation_pairs = Self::find_high_correlations(&assets, &matrix, 0.7);

        Self {
            assets: assets.to_vec(),
            matrix,
            high_correlation_pairs,
        }
    }

    /// Create a default correlation matrix with assumed correlations.
    pub fn default_for_assets(assets: &[String]) -> Self {
        let n = assets.len();
        let mut matrix = vec![vec![0.0; n]; n];

        // Set diagonal to 1 and estimate correlations based on asset type
        for i in 0..n {
            for j in 0..n {
                if i == j {
                    matrix[i][j] = 1.0;
                } else {
                    // Estimate correlation based on asset similarity
                    matrix[i][j] = estimate_correlation(&assets[i], &assets[j]);
                }
            }
        }

        let high_correlation_pairs = Self::find_high_correlations(&assets, &matrix, 0.7);

        Self {
            assets: assets.to_vec(),
            matrix,
            high_correlation_pairs,
        }
    }

    /// Find pairs with correlation above threshold.
    fn find_high_correlations(
        assets: &[String],
        matrix: &[Vec<f64>],
        threshold: f64,
    ) -> Vec<CorrelatedPair> {
        let n = assets.len();
        let mut pairs = Vec::new();

        for i in 0..n {
            for j in (i + 1)..n {
                let corr = matrix[i][j];
                if corr.abs() >= threshold {
                    pairs.push(CorrelatedPair {
                        asset_a: assets[i].clone(),
                        asset_b: assets[j].clone(),
                        correlation: corr,
                    });
                }
            }
        }

        // Sort by absolute correlation descending
        pairs.sort_by(|a, b| {
            b.correlation
                .abs()
                .partial_cmp(&a.correlation.abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        pairs
    }

    /// Get correlation between two assets.
    pub fn get_correlation(&self, asset_a: &str, asset_b: &str) -> Option<f64> {
        let idx_a = self.assets.iter().position(|a| a == asset_a)?;
        let idx_b = self.assets.iter().position(|a| a == asset_b)?;
        Some(self.matrix[idx_a][idx_b])
    }

    /// Get all correlations for an asset.
    pub fn get_correlations_for(&self, asset: &str) -> Option<HashMap<String, f64>> {
        let idx = self.assets.iter().position(|a| a == asset)?;

        let mut correlations = HashMap::new();
        for (i, name) in self.assets.iter().enumerate() {
            if i != idx {
                correlations.insert(name.clone(), self.matrix[idx][i]);
            }
        }

        Some(correlations)
    }

    /// Get the average correlation in the portfolio.
    pub fn average_correlation(&self) -> f64 {
        let n = self.assets.len();
        if n < 2 {
            return 0.0;
        }

        let mut sum = 0.0;
        let mut count = 0;

        for i in 0..n {
            for j in (i + 1)..n {
                sum += self.matrix[i][j];
                count += 1;
            }
        }

        if count > 0 {
            sum / count as f64
        } else {
            0.0
        }
    }
}

/// A pair of correlated assets.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorrelatedPair {
    /// First asset.
    pub asset_a: String,
    /// Second asset.
    pub asset_b: String,
    /// Correlation coefficient (-1 to 1).
    pub correlation: f64,
}

impl CorrelatedPair {
    /// Check if this is a positive correlation.
    pub fn is_positive(&self) -> bool {
        self.correlation > 0.0
    }

    /// Check if this is considered high correlation.
    pub fn is_high(&self) -> bool {
        self.correlation.abs() >= 0.7
    }

    /// Get the correlation type description.
    pub fn correlation_type(&self) -> &'static str {
        match self.correlation {
            c if c >= 0.9 => "Very High Positive",
            c if c >= 0.7 => "High Positive",
            c if c >= 0.5 => "Moderate Positive",
            c if c >= 0.3 => "Low Positive",
            c if c >= -0.3 => "Uncorrelated",
            c if c >= -0.5 => "Low Negative",
            c if c >= -0.7 => "Moderate Negative",
            c if c >= -0.9 => "High Negative",
            _ => "Very High Negative",
        }
    }
}

/// Calculate Pearson correlation coefficient between two return series.
fn calculate_correlation(returns_a: &[f64], returns_b: &[f64]) -> f64 {
    let n = returns_a.len().min(returns_b.len());
    if n < 2 {
        return 0.0;
    }

    let mean_a: f64 = returns_a.iter().take(n).sum::<f64>() / n as f64;
    let mean_b: f64 = returns_b.iter().take(n).sum::<f64>() / n as f64;

    let mut cov = 0.0;
    let mut var_a = 0.0;
    let mut var_b = 0.0;

    for i in 0..n {
        let diff_a = returns_a[i] - mean_a;
        let diff_b = returns_b[i] - mean_b;
        cov += diff_a * diff_b;
        var_a += diff_a * diff_a;
        var_b += diff_b * diff_b;
    }

    if var_a <= 0.0 || var_b <= 0.0 {
        return 0.0;
    }

    cov / (var_a.sqrt() * var_b.sqrt())
}

/// Estimate correlation based on asset type similarity.
fn estimate_correlation(asset_a: &str, asset_b: &str) -> f64 {
    // SOL-based tokens are highly correlated
    let sol_tokens = ["SOL", "mSOL", "JitoSOL", "stSOL", "bSOL"];
    let is_sol_a = sol_tokens.iter().any(|t| asset_a.contains(t));
    let is_sol_b = sol_tokens.iter().any(|t| asset_b.contains(t));

    if is_sol_a && is_sol_b {
        return 0.95;
    }

    // Stablecoins are highly correlated
    let stables = ["USDC", "USDT", "USDH", "DAI", "FRAX"];
    let is_stable_a = stables.iter().any(|t| asset_a.contains(t));
    let is_stable_b = stables.iter().any(|t| asset_b.contains(t));

    if is_stable_a && is_stable_b {
        return 0.98;
    }

    // SOL and stablecoin are uncorrelated
    if (is_sol_a && is_stable_b) || (is_sol_b && is_stable_a) {
        return 0.1;
    }

    // BTC/ETH correlation
    let btc_eth = ["BTC", "ETH", "WBTC", "WETH"];
    let is_btc_eth_a = btc_eth.iter().any(|t| asset_a.contains(t));
    let is_btc_eth_b = btc_eth.iter().any(|t| asset_b.contains(t));

    if is_btc_eth_a && is_btc_eth_b {
        return 0.85;
    }

    // SOL to BTC/ETH moderate correlation
    if (is_sol_a && is_btc_eth_b) || (is_sol_b && is_btc_eth_a) {
        return 0.7;
    }

    // Default moderate correlation for crypto assets
    0.5
}

/// Correlation analysis results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorrelationAnalysis {
    /// The correlation matrix.
    pub matrix: CorrelationMatrix,
    /// Average portfolio correlation.
    pub average_correlation: f64,
    /// Diversification ratio.
    pub diversification_ratio: f64,
    /// High-risk correlation clusters.
    pub clusters: Vec<CorrelationCluster>,
}

/// A cluster of highly correlated assets.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorrelationCluster {
    /// Assets in this cluster.
    pub assets: Vec<String>,
    /// Average internal correlation.
    pub internal_correlation: f64,
    /// Combined weight in portfolio.
    pub combined_weight: f64,
}

/// Analyze correlations and find clusters.
pub fn analyze_correlations(
    matrix: &CorrelationMatrix,
    weights: &HashMap<String, f64>,
    threshold: f64,
) -> CorrelationAnalysis {
    let average_correlation = matrix.average_correlation();

    // Simple clustering: group assets that are all highly correlated with each other
    let mut clusters: Vec<CorrelationCluster> = Vec::new();
    let mut clustered: std::collections::HashSet<String> = std::collections::HashSet::new();

    for pair in &matrix.high_correlation_pairs {
        if pair.correlation < threshold {
            continue;
        }

        // Check if either asset is already clustered
        let a_clustered = clustered.contains(&pair.asset_a);
        let b_clustered = clustered.contains(&pair.asset_b);

        if !a_clustered && !b_clustered {
            // Start new cluster
            let weight_a = weights.get(&pair.asset_a).copied().unwrap_or(0.0);
            let weight_b = weights.get(&pair.asset_b).copied().unwrap_or(0.0);

            clusters.push(CorrelationCluster {
                assets: vec![pair.asset_a.clone(), pair.asset_b.clone()],
                internal_correlation: pair.correlation,
                combined_weight: weight_a + weight_b,
            });

            clustered.insert(pair.asset_a.clone());
            clustered.insert(pair.asset_b.clone());
        } else if a_clustered && !b_clustered {
            // Add to existing cluster
            for cluster in &mut clusters {
                if cluster.assets.contains(&pair.asset_a) {
                    cluster.assets.push(pair.asset_b.clone());
                    cluster.combined_weight += weights.get(&pair.asset_b).copied().unwrap_or(0.0);
                    clustered.insert(pair.asset_b.clone());
                    break;
                }
            }
        } else if !a_clustered && b_clustered {
            for cluster in &mut clusters {
                if cluster.assets.contains(&pair.asset_b) {
                    cluster.assets.push(pair.asset_a.clone());
                    cluster.combined_weight += weights.get(&pair.asset_a).copied().unwrap_or(0.0);
                    clustered.insert(pair.asset_a.clone());
                    break;
                }
            }
        }
    }

    // Calculate diversification ratio
    let total_weight: f64 = weights.values().sum();
    let clustered_weight: f64 = clusters.iter().map(|c| c.combined_weight).sum();
    let diversification_ratio = if total_weight > 0.0 {
        1.0 - (clustered_weight / total_weight) * average_correlation
    } else {
        1.0
    };

    CorrelationAnalysis {
        matrix: matrix.clone(),
        average_correlation,
        diversification_ratio,
        clusters,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_correlation_calculation() {
        let returns_a = vec![0.01, -0.02, 0.03, -0.01, 0.02];
        let returns_b = vec![0.015, -0.018, 0.025, -0.008, 0.022];

        let corr = calculate_correlation(&returns_a, &returns_b);
        assert!(corr > 0.9); // Should be highly correlated
    }

    #[test]
    fn test_estimate_correlation() {
        // SOL tokens should be highly correlated
        assert!(estimate_correlation("SOL", "mSOL") > 0.9);
        assert!(estimate_correlation("mSOL", "JitoSOL") > 0.9);

        // Stablecoins highly correlated
        assert!(estimate_correlation("USDC", "USDT") > 0.95);

        // SOL and stable uncorrelated
        assert!(estimate_correlation("SOL", "USDC") < 0.3);
    }

    #[test]
    fn test_correlated_pair() {
        let pair = CorrelatedPair {
            asset_a: "SOL".to_string(),
            asset_b: "mSOL".to_string(),
            correlation: 0.95,
        };

        assert!(pair.is_positive());
        assert!(pair.is_high());
        assert_eq!(pair.correlation_type(), "Very High Positive");
    }

    #[test]
    fn test_correlation_matrix_default() {
        let assets = vec!["SOL".to_string(), "USDC".to_string(), "mSOL".to_string()];
        let matrix = CorrelationMatrix::default_for_assets(&assets);

        // Diagonal should be 1
        assert_eq!(matrix.matrix[0][0], 1.0);
        assert_eq!(matrix.matrix[1][1], 1.0);

        // SOL and mSOL should be highly correlated
        let sol_msol = matrix.get_correlation("SOL", "mSOL").unwrap();
        assert!(sol_msol > 0.9);

        // SOL and USDC should be uncorrelated
        let sol_usdc = matrix.get_correlation("SOL", "USDC").unwrap();
        assert!(sol_usdc < 0.3);
    }
}
