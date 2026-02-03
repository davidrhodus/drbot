//! Fuzzy string matching for drbot.
//!
//! This crate provides:
//! - Levenshtein distance
//! - Jaro-Winkler similarity
//! - Fuzzy search with scoring

use thiserror::Error;

/// Fuzzy matching error types.
#[derive(Error, Debug)]
pub enum FuzzyError {
    #[error("Empty input")]
    EmptyInput,

    #[error("No matches found")]
    NoMatches,
}

/// Result type for fuzzy operations.
pub type Result<T> = std::result::Result<T, FuzzyError>;

/// Calculate Levenshtein distance.
pub fn levenshtein(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();

    let m = a_chars.len();
    let n = b_chars.len();

    if m == 0 {
        return n;
    }
    if n == 0 {
        return m;
    }

    let mut matrix = vec![vec![0usize; n + 1]; m + 1];

    for i in 0..=m {
        matrix[i][0] = i;
    }
    for j in 0..=n {
        matrix[0][j] = j;
    }

    for i in 1..=m {
        for j in 1..=n {
            let cost = if a_chars[i - 1] == b_chars[j - 1] {
                0
            } else {
                1
            };

            matrix[i][j] = (matrix[i - 1][j] + 1)
                .min(matrix[i][j - 1] + 1)
                .min(matrix[i - 1][j - 1] + cost);
        }
    }

    matrix[m][n]
}

/// Calculate Levenshtein similarity (0.0 - 1.0).
pub fn levenshtein_similarity(a: &str, b: &str) -> f64 {
    let distance = levenshtein(a, b);
    let max_len = a.len().max(b.len());

    if max_len == 0 {
        return 1.0;
    }

    1.0 - (distance as f64 / max_len as f64)
}

/// Calculate Jaro similarity.
pub fn jaro(a: &str, b: &str) -> f64 {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();

    let a_len = a_chars.len();
    let b_len = b_chars.len();

    if a_len == 0 && b_len == 0 {
        return 1.0;
    }
    if a_len == 0 || b_len == 0 {
        return 0.0;
    }

    let match_window = (a_len.max(b_len) / 2).saturating_sub(1);

    let mut a_matched = vec![false; a_len];
    let mut b_matched = vec![false; b_len];

    let mut matches = 0;
    let mut transpositions = 0;

    // Find matches
    for i in 0..a_len {
        let start = i.saturating_sub(match_window);
        let end = (i + match_window + 1).min(b_len);

        for j in start..end {
            if b_matched[j] || a_chars[i] != b_chars[j] {
                continue;
            }
            a_matched[i] = true;
            b_matched[j] = true;
            matches += 1;
            break;
        }
    }

    if matches == 0 {
        return 0.0;
    }

    // Count transpositions
    let mut k = 0;
    for i in 0..a_len {
        if !a_matched[i] {
            continue;
        }
        while !b_matched[k] {
            k += 1;
        }
        if a_chars[i] != b_chars[k] {
            transpositions += 1;
        }
        k += 1;
    }

    let matches = matches as f64;
    let transpositions = transpositions as f64 / 2.0;

    (matches / a_len as f64 + matches / b_len as f64 + (matches - transpositions) / matches) / 3.0
}

/// Calculate Jaro-Winkler similarity.
pub fn jaro_winkler(a: &str, b: &str) -> f64 {
    let jaro_sim = jaro(a, b);

    // Calculate common prefix length (up to 4 chars)
    let prefix_len = a
        .chars()
        .zip(b.chars())
        .take(4)
        .take_while(|(ca, cb)| ca == cb)
        .count();

    // Winkler scaling factor
    let scaling = 0.1;

    jaro_sim + (prefix_len as f64 * scaling * (1.0 - jaro_sim))
}

/// Match result with score.
#[derive(Debug, Clone)]
pub struct FuzzyMatch<'a> {
    pub text: &'a str,
    pub score: f64,
    pub distance: usize,
}

impl<'a> FuzzyMatch<'a> {
    /// Create new match result.
    pub fn new(text: &'a str, query: &str) -> Self {
        let distance = levenshtein(text, query);
        let score = jaro_winkler(text, query);
        Self {
            text,
            score,
            distance,
        }
    }
}

/// Fuzzy search configuration.
#[derive(Debug, Clone)]
pub struct FuzzyConfig {
    /// Minimum similarity score (0.0 - 1.0).
    pub min_score: f64,
    /// Maximum edit distance (None for unlimited).
    pub max_distance: Option<usize>,
    /// Case sensitive matching.
    pub case_sensitive: bool,
}

impl Default for FuzzyConfig {
    fn default() -> Self {
        Self {
            min_score: 0.6,
            max_distance: None,
            case_sensitive: false,
        }
    }
}

/// Fuzzy search in collection.
pub fn search<'a>(
    query: &str,
    candidates: &'a [&str],
    config: &FuzzyConfig,
) -> Vec<FuzzyMatch<'a>> {
    let query_normalized = if config.case_sensitive {
        query.to_string()
    } else {
        query.to_lowercase()
    };

    let mut matches: Vec<FuzzyMatch<'a>> = candidates
        .iter()
        .filter_map(|&candidate| {
            let candidate_normalized = if config.case_sensitive {
                candidate.to_string()
            } else {
                candidate.to_lowercase()
            };

            let score = jaro_winkler(&query_normalized, &candidate_normalized);
            let distance = levenshtein(&query_normalized, &candidate_normalized);

            if score < config.min_score {
                return None;
            }

            if let Some(max_dist) = config.max_distance {
                if distance > max_dist {
                    return None;
                }
            }

            Some(FuzzyMatch {
                text: candidate,
                score,
                distance,
            })
        })
        .collect();

    // Sort by score (descending)
    matches.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());

    matches
}

/// Simple fuzzy match check.
pub fn is_fuzzy_match(query: &str, text: &str, threshold: f64) -> bool {
    jaro_winkler(&query.to_lowercase(), &text.to_lowercase()) >= threshold
}

/// Find best match.
pub fn best_match<'a>(query: &str, candidates: &'a [&str]) -> Option<&'a str> {
    candidates
        .iter()
        .max_by(|a, b| {
            let score_a = jaro_winkler(&query.to_lowercase(), &a.to_lowercase());
            let score_b = jaro_winkler(&query.to_lowercase(), &b.to_lowercase());
            score_a.partial_cmp(&score_b).unwrap()
        })
        .copied()
}

/// Calculate similarity ratio between two strings.
pub fn ratio(a: &str, b: &str) -> f64 {
    jaro_winkler(a, b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_levenshtein() {
        assert_eq!(levenshtein("kitten", "sitting"), 3);
        assert_eq!(levenshtein("hello", "hello"), 0);
        assert_eq!(levenshtein("", "abc"), 3);
        assert_eq!(levenshtein("abc", ""), 3);
    }

    #[test]
    fn test_levenshtein_similarity() {
        assert!((levenshtein_similarity("hello", "hello") - 1.0).abs() < 0.001);
        assert!(levenshtein_similarity("hello", "hallo") > 0.7);
    }

    #[test]
    fn test_jaro() {
        assert!((jaro("", "") - 1.0).abs() < 0.001);
        assert!(jaro("hello", "hello") > 0.99);
        assert!(jaro("hello", "hallo") > 0.7);
    }

    #[test]
    fn test_jaro_winkler() {
        // Strings with common prefix should score higher in Jaro-Winkler
        let jw = jaro_winkler("prefix_abc", "prefix_xyz");
        let j = jaro("prefix_abc", "prefix_xyz");
        assert!(jw >= j);
    }

    #[test]
    fn test_search() {
        let candidates = vec!["apple", "banana", "apricot", "orange"];
        let config = FuzzyConfig {
            min_score: 0.5,
            ..Default::default()
        };

        let results = search("aple", &candidates, &config);
        assert!(!results.is_empty());
        assert_eq!(results[0].text, "apple");
    }

    #[test]
    fn test_best_match() {
        let candidates = vec!["apple", "application", "apply"];
        let best = best_match("app", &candidates);
        assert!(best.is_some());
    }

    #[test]
    fn test_is_fuzzy_match() {
        assert!(is_fuzzy_match("hello", "hallo", 0.7));
        assert!(!is_fuzzy_match("hello", "world", 0.7));
    }
}
