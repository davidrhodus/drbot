//! AI code companion for drbot.
//!
//! Intelligent code assistance and analysis.
//!
//! # Features
//!
//! - Code review
//! - Code explanation
//! - Test generation
//! - Refactoring suggestions
//! - Bug detection

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Code AI result type.
pub type Result<T> = std::result::Result<T, CodeError>;

/// Code AI errors.
#[derive(Debug, thiserror::Error)]
pub enum CodeError {
    #[error("File not found: {0}")]
    FileNotFound(String),
    #[error("Unsupported language: {0}")]
    UnsupportedLanguage(String),
    #[error("Analysis failed: {0}")]
    AnalysisFailed(String),
    #[error("Generation failed: {0}")]
    GenerationFailed(String),
}

/// Programming language.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Language {
    Rust,
    Python,
    JavaScript,
    TypeScript,
    Go,
    Java,
    CSharp,
    Cpp,
    Ruby,
    Swift,
    Kotlin,
    Unknown,
}

impl Language {
    /// Detect from extension.
    pub fn from_extension(ext: &str) -> Self {
        match ext.to_lowercase().as_str() {
            "rs" => Language::Rust,
            "py" => Language::Python,
            "js" | "mjs" | "cjs" => Language::JavaScript,
            "ts" | "tsx" => Language::TypeScript,
            "go" => Language::Go,
            "java" => Language::Java,
            "cs" => Language::CSharp,
            "cpp" | "cc" | "cxx" | "c" | "h" | "hpp" => Language::Cpp,
            "rb" => Language::Ruby,
            "swift" => Language::Swift,
            "kt" | "kts" => Language::Kotlin,
            _ => Language::Unknown,
        }
    }
}

/// A code file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeFile {
    /// File ID.
    pub id: Uuid,
    /// File path.
    pub path: String,
    /// Language.
    pub language: Language,
    /// Content.
    pub content: String,
    /// Line count.
    pub lines: usize,
    /// Indexed at.
    pub indexed_at: DateTime<Utc>,
}

impl CodeFile {
    /// Create a new code file.
    pub fn new(path: &str, content: &str) -> Self {
        let ext = path.rsplit('.').next().unwrap_or("");
        Self {
            id: Uuid::new_v4(),
            path: path.to_string(),
            language: Language::from_extension(ext),
            content: content.to_string(),
            lines: content.lines().count(),
            indexed_at: Utc::now(),
        }
    }
}

/// Code review result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeReview {
    /// Review ID.
    pub id: Uuid,
    /// File ID.
    pub file_id: Uuid,
    /// Overall score (0-100).
    pub score: u32,
    /// Issues found.
    pub issues: Vec<CodeIssue>,
    /// Suggestions.
    pub suggestions: Vec<CodeSuggestion>,
    /// Metrics.
    pub metrics: CodeMetrics,
    /// Generated at.
    pub generated_at: DateTime<Utc>,
}

/// Code issue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeIssue {
    /// Issue ID.
    pub id: Uuid,
    /// Severity.
    pub severity: IssueSeverity,
    /// Issue type.
    pub issue_type: IssueType,
    /// Description.
    pub description: String,
    /// Line number.
    pub line: usize,
    /// Column.
    pub column: Option<usize>,
    /// Code snippet.
    pub snippet: Option<String>,
    /// Suggested fix.
    pub suggested_fix: Option<String>,
}

/// Issue severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueSeverity {
    Error,
    Warning,
    Info,
    Hint,
}

/// Issue type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueType {
    Bug,
    Security,
    Performance,
    Style,
    Complexity,
    Maintainability,
    BestPractice,
    Documentation,
}

/// Code suggestion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeSuggestion {
    /// Suggestion ID.
    pub id: Uuid,
    /// Category.
    pub category: SuggestionCategory,
    /// Description.
    pub description: String,
    /// Before code.
    pub before: Option<String>,
    /// After code (suggested).
    pub after: Option<String>,
    /// Impact.
    pub impact: String,
}

/// Suggestion category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SuggestionCategory {
    Refactoring,
    Optimization,
    Simplification,
    Naming,
    Structure,
    Testing,
    Documentation,
}

/// Code metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeMetrics {
    /// Lines of code.
    pub loc: usize,
    /// Cyclomatic complexity.
    pub complexity: u32,
    /// Maintainability index.
    pub maintainability: f32,
    /// Test coverage (if known).
    pub coverage: Option<f32>,
    /// Documentation ratio.
    pub doc_ratio: f32,
}

/// Code explanation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeExplanation {
    /// Explanation ID.
    pub id: Uuid,
    /// Summary.
    pub summary: String,
    /// Purpose.
    pub purpose: String,
    /// Key concepts.
    pub concepts: Vec<String>,
    /// Step by step breakdown.
    pub steps: Vec<ExplanationStep>,
    /// Dependencies used.
    pub dependencies: Vec<String>,
}

/// Explanation step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExplanationStep {
    /// Line range.
    pub lines: (usize, usize),
    /// Explanation.
    pub explanation: String,
    /// Code snippet.
    pub code: String,
}

/// Generated test.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedTest {
    /// Test ID.
    pub id: Uuid,
    /// Test name.
    pub name: String,
    /// Test code.
    pub code: String,
    /// Test type.
    pub test_type: TestType,
    /// Description.
    pub description: String,
    /// Covers function.
    pub covers: String,
}

/// Test type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TestType {
    Unit,
    Integration,
    Property,
    Snapshot,
    Fuzz,
}

/// Code AI configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeAIConfig {
    /// Enable security checks.
    pub security_checks: bool,
    /// Enable style checks.
    pub style_checks: bool,
    /// Max complexity threshold.
    pub max_complexity: u32,
    /// Test framework preference.
    pub test_framework: Option<String>,
    /// Style guide.
    pub style_guide: Option<String>,
}

impl Default for CodeAIConfig {
    fn default() -> Self {
        Self {
            security_checks: true,
            style_checks: true,
            max_complexity: 10,
            test_framework: None,
            style_guide: None,
        }
    }
}

/// Trait for code analyzers.
#[async_trait]
pub trait CodeAnalyzer: Send + Sync {
    /// Review code.
    async fn review(&self, file: &CodeFile) -> Result<CodeReview>;
    /// Explain code.
    async fn explain(&self, code: &str, language: Language) -> Result<CodeExplanation>;
    /// Detect issues.
    async fn detect_issues(&self, file: &CodeFile) -> Result<Vec<CodeIssue>>;
}

/// Trait for code generators.
#[async_trait]
pub trait CodeGenerator: Send + Sync {
    /// Generate tests.
    async fn generate_tests(&self, file: &CodeFile) -> Result<Vec<GeneratedTest>>;
    /// Generate documentation.
    async fn generate_docs(&self, file: &CodeFile) -> Result<String>;
    /// Suggest refactoring.
    async fn suggest_refactoring(&self, file: &CodeFile) -> Result<Vec<CodeSuggestion>>;
}

/// Code AI engine.
pub struct CodeAIEngine<A: CodeAnalyzer, G: CodeGenerator> {
    config: CodeAIConfig,
    analyzer: A,
    generator: G,
    files: Arc<RwLock<HashMap<Uuid, CodeFile>>>,
    reviews: Arc<RwLock<HashMap<Uuid, CodeReview>>>,
}

impl<A: CodeAnalyzer, G: CodeGenerator> CodeAIEngine<A, G> {
    /// Create a new code AI engine.
    pub fn new(config: CodeAIConfig, analyzer: A, generator: G) -> Self {
        Self {
            config,
            analyzer,
            generator,
            files: Arc::new(RwLock::new(HashMap::new())),
            reviews: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Index a code file.
    pub async fn index(&self, path: &str, content: &str) -> Result<CodeFile> {
        let file = CodeFile::new(path, content);

        if file.language == Language::Unknown {
            return Err(CodeError::UnsupportedLanguage(path.to_string()));
        }

        self.files.write().await.insert(file.id, file.clone());
        Ok(file)
    }

    /// Review a file.
    pub async fn review(&self, file_id: Uuid) -> Result<CodeReview> {
        let file = self
            .files
            .read()
            .await
            .get(&file_id)
            .cloned()
            .ok_or(CodeError::FileNotFound(file_id.to_string()))?;

        let review = self.analyzer.review(&file).await?;
        self.reviews.write().await.insert(file_id, review.clone());

        Ok(review)
    }

    /// Explain code.
    pub async fn explain(&self, code: &str, language: Language) -> Result<CodeExplanation> {
        self.analyzer.explain(code, language).await
    }

    /// Generate tests for a file.
    pub async fn generate_tests(&self, file_id: Uuid) -> Result<Vec<GeneratedTest>> {
        let file = self
            .files
            .read()
            .await
            .get(&file_id)
            .cloned()
            .ok_or(CodeError::FileNotFound(file_id.to_string()))?;

        self.generator.generate_tests(&file).await
    }

    /// Generate documentation.
    pub async fn generate_docs(&self, file_id: Uuid) -> Result<String> {
        let file = self
            .files
            .read()
            .await
            .get(&file_id)
            .cloned()
            .ok_or(CodeError::FileNotFound(file_id.to_string()))?;

        self.generator.generate_docs(&file).await
    }

    /// Get refactoring suggestions.
    pub async fn suggest_refactoring(&self, file_id: Uuid) -> Result<Vec<CodeSuggestion>> {
        let file = self
            .files
            .read()
            .await
            .get(&file_id)
            .cloned()
            .ok_or(CodeError::FileNotFound(file_id.to_string()))?;

        self.generator.suggest_refactoring(&file).await
    }

    /// Search code.
    pub async fn search(&self, query: &str) -> Vec<(CodeFile, Vec<(usize, String)>)> {
        let files = self.files.read().await;
        let query_lower = query.to_lowercase();

        files
            .values()
            .filter_map(|file| {
                let matches: Vec<_> = file
                    .content
                    .lines()
                    .enumerate()
                    .filter(|(_, line)| line.to_lowercase().contains(&query_lower))
                    .map(|(i, line)| (i + 1, line.to_string()))
                    .collect();

                if matches.is_empty() {
                    None
                } else {
                    Some((file.clone(), matches))
                }
            })
            .collect()
    }

    /// Get statistics.
    pub async fn stats(&self) -> CodeStats {
        let files = self.files.read().await;
        let reviews = self.reviews.read().await;

        let total_lines: usize = files.values().map(|f| f.lines).sum();
        let total_issues: usize = reviews.values().map(|r| r.issues.len()).sum();

        let mut by_language: HashMap<Language, usize> = HashMap::new();
        for file in files.values() {
            *by_language.entry(file.language).or_insert(0) += 1;
        }

        CodeStats {
            total_files: files.len(),
            total_lines,
            total_reviews: reviews.len(),
            total_issues,
            by_language,
        }
    }
}

/// Code statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeStats {
    pub total_files: usize,
    pub total_lines: usize,
    pub total_reviews: usize,
    pub total_issues: usize,
    pub by_language: HashMap<Language, usize>,
}

/// Simple code analyzer for testing.
pub struct SimpleAnalyzer;

#[async_trait]
impl CodeAnalyzer for SimpleAnalyzer {
    async fn review(&self, file: &CodeFile) -> Result<CodeReview> {
        let mut issues = Vec::new();

        // Simple checks
        for (i, line) in file.content.lines().enumerate() {
            if line.len() > 120 {
                issues.push(CodeIssue {
                    id: Uuid::new_v4(),
                    severity: IssueSeverity::Warning,
                    issue_type: IssueType::Style,
                    description: "Line exceeds 120 characters".to_string(),
                    line: i + 1,
                    column: None,
                    snippet: Some(line.to_string()),
                    suggested_fix: None,
                });
            }

            if line.contains("TODO") || line.contains("FIXME") {
                issues.push(CodeIssue {
                    id: Uuid::new_v4(),
                    severity: IssueSeverity::Info,
                    issue_type: IssueType::Maintainability,
                    description: "TODO/FIXME comment found".to_string(),
                    line: i + 1,
                    column: None,
                    snippet: Some(line.to_string()),
                    suggested_fix: None,
                });
            }

            if line.to_lowercase().contains("password") && line.contains("=") {
                issues.push(CodeIssue {
                    id: Uuid::new_v4(),
                    severity: IssueSeverity::Error,
                    issue_type: IssueType::Security,
                    description: "Possible hardcoded password".to_string(),
                    line: i + 1,
                    column: None,
                    snippet: Some(line.to_string()),
                    suggested_fix: Some("Use environment variables".to_string()),
                });
            }
        }

        let score = if issues.is_empty() {
            100
        } else {
            (100 - issues.len() * 5).max(0) as u32
        };

        Ok(CodeReview {
            id: Uuid::new_v4(),
            file_id: file.id,
            score,
            issues,
            suggestions: Vec::new(),
            metrics: CodeMetrics {
                loc: file.lines,
                complexity: 5,
                maintainability: 80.0,
                coverage: None,
                doc_ratio: 0.1,
            },
            generated_at: Utc::now(),
        })
    }

    async fn explain(&self, code: &str, language: Language) -> Result<CodeExplanation> {
        let lines: Vec<_> = code.lines().collect();

        Ok(CodeExplanation {
            id: Uuid::new_v4(),
            summary: format!("{:?} code with {} lines", language, lines.len()),
            purpose: "Code functionality analysis".to_string(),
            concepts: vec!["Variables".to_string(), "Functions".to_string()],
            steps: vec![ExplanationStep {
                lines: (1, lines.len()),
                explanation: "Main code block".to_string(),
                code: code.to_string(),
            }],
            dependencies: Vec::new(),
        })
    }

    async fn detect_issues(&self, file: &CodeFile) -> Result<Vec<CodeIssue>> {
        let review = self.review(file).await?;
        Ok(review.issues)
    }
}

/// Simple code generator for testing.
pub struct SimpleGenerator;

#[async_trait]
impl CodeGenerator for SimpleGenerator {
    async fn generate_tests(&self, file: &CodeFile) -> Result<Vec<GeneratedTest>> {
        Ok(vec![GeneratedTest {
            id: Uuid::new_v4(),
            name: format!("test_{}", file.path.replace('/', "_").replace('.', "_")),
            code: format!(
                "// Test for {}\n#[test]\nfn test_example() {{\n    assert!(true);\n}}",
                file.path
            ),
            test_type: TestType::Unit,
            description: "Basic test".to_string(),
            covers: file.path.clone(),
        }])
    }

    async fn generate_docs(&self, file: &CodeFile) -> Result<String> {
        Ok(format!(
            "# {}\n\n## Overview\n\n{:?} file with {} lines of code.\n\n## Usage\n\nSee code for details.",
            file.path,
            file.language,
            file.lines
        ))
    }

    async fn suggest_refactoring(&self, file: &CodeFile) -> Result<Vec<CodeSuggestion>> {
        let mut suggestions = Vec::new();

        if file.lines > 200 {
            suggestions.push(CodeSuggestion {
                id: Uuid::new_v4(),
                category: SuggestionCategory::Structure,
                description: "Consider splitting this file into smaller modules".to_string(),
                before: None,
                after: None,
                impact: "Improved maintainability".to_string(),
            });
        }

        Ok(suggestions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_index_file() {
        let engine = CodeAIEngine::new(CodeAIConfig::default(), SimpleAnalyzer, SimpleGenerator);

        let file = engine
            .index("test.rs", "fn main() {\n    println!(\"Hello\");\n}")
            .await
            .unwrap();
        assert_eq!(file.language, Language::Rust);
        assert_eq!(file.lines, 3);
    }

    #[tokio::test]
    async fn test_review() {
        let engine = CodeAIEngine::new(CodeAIConfig::default(), SimpleAnalyzer, SimpleGenerator);

        let file = engine
            .index("test.py", "# TODO: implement this\ndef hello():\n    pass")
            .await
            .unwrap();
        let review = engine.review(file.id).await.unwrap();
        assert!(!review.issues.is_empty());
    }

    #[tokio::test]
    async fn test_generate_tests() {
        let engine = CodeAIEngine::new(CodeAIConfig::default(), SimpleAnalyzer, SimpleGenerator);

        let file = engine
            .index("math.rs", "pub fn add(a: i32, b: i32) -> i32 { a + b }")
            .await
            .unwrap();
        let tests = engine.generate_tests(file.id).await.unwrap();
        assert!(!tests.is_empty());
    }

    #[tokio::test]
    async fn test_security_check() {
        let engine = CodeAIEngine::new(CodeAIConfig::default(), SimpleAnalyzer, SimpleGenerator);

        let file = engine
            .index("config.py", "password = \"secret123\"")
            .await
            .unwrap();
        let review = engine.review(file.id).await.unwrap();

        let security_issues: Vec<_> = review
            .issues
            .iter()
            .filter(|i| i.issue_type == IssueType::Security)
            .collect();
        assert!(!security_issues.is_empty());
    }
}
