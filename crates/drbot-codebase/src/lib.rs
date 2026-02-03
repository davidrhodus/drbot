//! Semantic code graph with impact analysis.
//!
//! This crate provides deep code understanding capabilities:
//! - Build semantic graphs of codebases
//! - Track dependencies and relationships
//! - Perform impact analysis for changes
//! - Understand code at a structural level

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Codebase analysis errors.
#[derive(Debug, Error)]
pub enum CodebaseError {
    #[error("Analysis failed: {0}")]
    AnalysisFailed(String),

    #[error("Symbol not found: {0}")]
    SymbolNotFound(String),

    #[error("File not found: {0}")]
    FileNotFound(String),

    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Provider error: {0}")]
    ProviderError(String),
}

/// Result type for codebase operations.
pub type Result<T> = std::result::Result<T, CodebaseError>;

/// A symbol in the codebase (function, class, etc).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Symbol {
    /// Symbol identifier.
    pub id: String,
    /// Symbol name.
    pub name: String,
    /// Fully qualified name.
    pub qualified_name: String,
    /// Symbol kind.
    pub kind: SymbolKind,
    /// File containing this symbol.
    pub file_path: String,
    /// Line number.
    pub line: u32,
    /// Column number.
    pub column: u32,
    /// End line.
    pub end_line: u32,
    /// Symbol signature.
    pub signature: Option<String>,
    /// Documentation.
    pub documentation: Option<String>,
    /// Visibility.
    pub visibility: Visibility,
    /// Parent symbol (e.g., class for method).
    pub parent: Option<String>,
    /// Metadata.
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Kind of symbol.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum SymbolKind {
    File,
    Module,
    Namespace,
    Package,
    Class,
    Interface,
    Trait,
    Struct,
    Enum,
    EnumMember,
    Function,
    Method,
    Constructor,
    Property,
    Field,
    Variable,
    Constant,
    TypeAlias,
    Macro,
    Test,
    Unknown,
}

/// Symbol visibility.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Visibility {
    Public,
    Protected,
    Private,
    Internal,
    Unknown,
}

/// A reference between symbols.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reference {
    /// Reference identifier.
    pub id: String,
    /// Source symbol.
    pub from_symbol: String,
    /// Target symbol.
    pub to_symbol: String,
    /// Reference type.
    pub reference_type: ReferenceType,
    /// Location of the reference.
    pub location: Location,
}

/// Types of references.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ReferenceType {
    /// Function/method call.
    Call,
    /// Type usage.
    Type,
    /// Import/use.
    Import,
    /// Inheritance.
    Inherits,
    /// Implementation.
    Implements,
    /// Field access.
    FieldAccess,
    /// Instantiation.
    Instantiate,
    /// Override.
    Override,
    /// Annotation/attribute.
    Annotation,
    /// Generic/template parameter.
    GenericParam,
}

/// A location in code.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Location {
    /// File path.
    pub file_path: String,
    /// Start line.
    pub start_line: u32,
    /// Start column.
    pub start_column: u32,
    /// End line.
    pub end_line: u32,
    /// End column.
    pub end_column: u32,
}

/// Impact analysis result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImpactAnalysis {
    /// Changed symbol.
    pub changed_symbol: String,
    /// Directly impacted symbols.
    pub direct_impacts: Vec<Impact>,
    /// Transitively impacted symbols.
    pub transitive_impacts: Vec<Impact>,
    /// Impacted files.
    pub impacted_files: Vec<String>,
    /// Risk assessment.
    pub risk: RiskAssessment,
    /// Suggested tests to run.
    pub suggested_tests: Vec<String>,
}

/// An impact on a symbol.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Impact {
    /// Impacted symbol.
    pub symbol_id: String,
    /// Symbol name.
    pub symbol_name: String,
    /// Impact type.
    pub impact_type: ImpactType,
    /// Impact severity.
    pub severity: ImpactSeverity,
    /// Path from changed symbol.
    pub dependency_path: Vec<String>,
}

/// Types of impact.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ImpactType {
    /// Signature change affects callers.
    SignatureChange,
    /// Behavior change may affect dependents.
    BehaviorChange,
    /// Removal affects all users.
    Removal,
    /// Type change affects type users.
    TypeChange,
    /// Performance change.
    PerformanceChange,
}

/// Severity of impact.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum ImpactSeverity {
    None,
    Low,
    Medium,
    High,
    Breaking,
}

/// Risk assessment for changes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskAssessment {
    /// Overall risk level.
    pub level: RiskLevel,
    /// Risk factors.
    pub factors: Vec<RiskFactor>,
    /// Mitigation suggestions.
    pub mitigations: Vec<String>,
}

/// Risk levels.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum RiskLevel {
    Minimal,
    Low,
    Medium,
    High,
    Critical,
}

/// A factor contributing to risk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskFactor {
    /// Factor description.
    pub description: String,
    /// Contribution to risk.
    pub contribution: f64,
}

/// Codebase statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodebaseStats {
    /// Total files.
    pub total_files: usize,
    /// Total symbols.
    pub total_symbols: usize,
    /// Symbols by kind.
    pub symbols_by_kind: HashMap<String, usize>,
    /// Total references.
    pub total_references: usize,
    /// Average complexity.
    pub avg_complexity: f64,
    /// Last indexed.
    pub last_indexed: DateTime<Utc>,
}

/// Provider for code analysis.
#[async_trait]
pub trait CodeAnalysisProvider: Send + Sync {
    /// Parse a file and extract symbols.
    async fn parse_file(&self, path: &str, content: &str) -> Result<Vec<Symbol>>;

    /// Extract references from a file.
    async fn extract_references(&self, path: &str, content: &str) -> Result<Vec<Reference>>;

    /// Analyze code for complexity.
    async fn analyze_complexity(&self, symbol: &Symbol, content: &str) -> Result<f64>;

    /// Generate documentation for a symbol.
    async fn generate_docs(&self, symbol: &Symbol, content: &str) -> Result<String>;
}

/// The codebase analyzer.
pub struct CodebaseAnalyzer {
    /// Analysis provider.
    provider: Arc<dyn CodeAnalysisProvider>,
    /// Indexed symbols.
    symbols: Arc<RwLock<HashMap<String, Symbol>>>,
    /// Symbol references.
    references: Arc<RwLock<Vec<Reference>>>,
    /// File contents cache.
    files: Arc<RwLock<HashMap<String, String>>>,
    /// Dependency graph: symbol -> dependents.
    dependents: Arc<RwLock<HashMap<String, HashSet<String>>>>,
    /// Reverse graph: symbol -> dependencies.
    dependencies: Arc<RwLock<HashMap<String, HashSet<String>>>>,
}

impl CodebaseAnalyzer {
    /// Create a new codebase analyzer.
    pub fn new(provider: Arc<dyn CodeAnalysisProvider>) -> Self {
        Self {
            provider,
            symbols: Arc::new(RwLock::new(HashMap::new())),
            references: Arc::new(RwLock::new(Vec::new())),
            files: Arc::new(RwLock::new(HashMap::new())),
            dependents: Arc::new(RwLock::new(HashMap::new())),
            dependencies: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Index a file.
    pub async fn index_file(&self, path: &str, content: &str) -> Result<Vec<Symbol>> {
        // Parse symbols
        let symbols = self.provider.parse_file(path, content).await?;

        // Extract references
        let refs = self.provider.extract_references(path, content).await?;

        // Store symbols
        let mut stored_symbols = self.symbols.write().await;
        for symbol in &symbols {
            stored_symbols.insert(symbol.id.clone(), symbol.clone());
        }

        // Store references and build dependency graph
        let mut stored_refs = self.references.write().await;
        let mut dependents = self.dependents.write().await;
        let mut dependencies = self.dependencies.write().await;

        for reference in refs {
            // Add to dependents graph
            dependents
                .entry(reference.to_symbol.clone())
                .or_insert_with(HashSet::new)
                .insert(reference.from_symbol.clone());

            // Add to dependencies graph
            dependencies
                .entry(reference.from_symbol.clone())
                .or_insert_with(HashSet::new)
                .insert(reference.to_symbol.clone());

            stored_refs.push(reference);
        }

        // Cache file content
        let mut files = self.files.write().await;
        files.insert(path.to_string(), content.to_string());

        Ok(symbols)
    }

    /// Get a symbol by ID.
    pub async fn get_symbol(&self, id: &str) -> Option<Symbol> {
        let symbols = self.symbols.read().await;
        symbols.get(id).cloned()
    }

    /// Find symbols by name.
    pub async fn find_symbols(&self, name: &str) -> Vec<Symbol> {
        let symbols = self.symbols.read().await;
        symbols
            .values()
            .filter(|s| s.name.contains(name) || s.qualified_name.contains(name))
            .cloned()
            .collect()
    }

    /// Find symbols by kind.
    pub async fn find_by_kind(&self, kind: SymbolKind) -> Vec<Symbol> {
        let symbols = self.symbols.read().await;
        symbols
            .values()
            .filter(|s| s.kind == kind)
            .cloned()
            .collect()
    }

    /// Get dependents of a symbol.
    pub async fn get_dependents(&self, symbol_id: &str) -> Vec<Symbol> {
        let dependents = self.dependents.read().await;
        let symbols = self.symbols.read().await;

        dependents
            .get(symbol_id)
            .map(|deps| {
                deps.iter()
                    .filter_map(|id| symbols.get(id).cloned())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get dependencies of a symbol.
    pub async fn get_dependencies(&self, symbol_id: &str) -> Vec<Symbol> {
        let dependencies = self.dependencies.read().await;
        let symbols = self.symbols.read().await;

        dependencies
            .get(symbol_id)
            .map(|deps| {
                deps.iter()
                    .filter_map(|id| symbols.get(id).cloned())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Perform impact analysis for a change.
    pub async fn analyze_impact(
        &self,
        symbol_id: &str,
        change_type: ImpactType,
    ) -> Result<ImpactAnalysis> {
        let symbol = self
            .get_symbol(symbol_id)
            .await
            .ok_or_else(|| CodebaseError::SymbolNotFound(symbol_id.to_string()))?;

        // Get direct dependents
        let direct_deps = self.get_dependents(symbol_id).await;

        let direct_impacts: Vec<Impact> = direct_deps
            .iter()
            .map(|dep| Impact {
                symbol_id: dep.id.clone(),
                symbol_name: dep.qualified_name.clone(),
                impact_type: change_type,
                severity: self.calculate_severity(change_type, &symbol, dep),
                dependency_path: vec![symbol.qualified_name.clone(), dep.qualified_name.clone()],
            })
            .collect();

        // Get transitive dependents (BFS)
        let mut transitive_impacts = Vec::new();
        let mut visited: HashSet<String> = HashSet::new();
        visited.insert(symbol_id.to_string());
        for dep in &direct_deps {
            visited.insert(dep.id.clone());
        }

        let mut queue: Vec<(String, Vec<String>)> = direct_deps
            .iter()
            .map(|d| {
                (
                    d.id.clone(),
                    vec![symbol.qualified_name.clone(), d.qualified_name.clone()],
                )
            })
            .collect();

        while let Some((current_id, path)) = queue.pop() {
            let deps = self.get_dependents(&current_id).await;
            for dep in deps {
                if !visited.contains(&dep.id) {
                    visited.insert(dep.id.clone());
                    let mut new_path = path.clone();
                    new_path.push(dep.qualified_name.clone());

                    transitive_impacts.push(Impact {
                        symbol_id: dep.id.clone(),
                        symbol_name: dep.qualified_name.clone(),
                        impact_type: change_type,
                        severity: ImpactSeverity::Low, // Transitive impacts are usually lower
                        dependency_path: new_path.clone(),
                    });

                    if new_path.len() < 5 {
                        // Limit depth
                        queue.push((dep.id.clone(), new_path));
                    }
                }
            }
        }

        // Find impacted files
        let symbols = self.symbols.read().await;
        let impacted_files: Vec<String> = direct_impacts
            .iter()
            .chain(transitive_impacts.iter())
            .filter_map(|i| symbols.get(&i.symbol_id).map(|s| s.file_path.clone()))
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        drop(symbols);

        // Assess risk
        let risk = self.assess_risk(&direct_impacts, &transitive_impacts);

        // Find related tests
        let suggested_tests = self.find_related_tests(symbol_id).await;

        Ok(ImpactAnalysis {
            changed_symbol: symbol_id.to_string(),
            direct_impacts,
            transitive_impacts,
            impacted_files,
            risk,
            suggested_tests,
        })
    }

    /// Calculate impact severity.
    fn calculate_severity(
        &self,
        change_type: ImpactType,
        _changed: &Symbol,
        _dependent: &Symbol,
    ) -> ImpactSeverity {
        match change_type {
            ImpactType::Removal => ImpactSeverity::Breaking,
            ImpactType::SignatureChange => ImpactSeverity::High,
            ImpactType::TypeChange => ImpactSeverity::Medium,
            ImpactType::BehaviorChange => ImpactSeverity::Medium,
            ImpactType::PerformanceChange => ImpactSeverity::Low,
        }
    }

    /// Assess risk of changes.
    fn assess_risk(&self, direct: &[Impact], transitive: &[Impact]) -> RiskAssessment {
        let mut factors = Vec::new();

        let breaking_count = direct
            .iter()
            .chain(transitive.iter())
            .filter(|i| i.severity == ImpactSeverity::Breaking)
            .count();

        if breaking_count > 0 {
            factors.push(RiskFactor {
                description: format!("{} breaking changes", breaking_count),
                contribution: 0.5,
            });
        }

        let high_impact_count = direct
            .iter()
            .filter(|i| i.severity >= ImpactSeverity::High)
            .count();

        if high_impact_count > 5 {
            factors.push(RiskFactor {
                description: format!("{} high-impact dependents", high_impact_count),
                contribution: 0.3,
            });
        }

        let total_contribution: f64 = factors.iter().map(|f| f.contribution).sum();
        let level = if total_contribution > 0.7 {
            RiskLevel::Critical
        } else if total_contribution > 0.5 {
            RiskLevel::High
        } else if total_contribution > 0.3 {
            RiskLevel::Medium
        } else if total_contribution > 0.1 {
            RiskLevel::Low
        } else {
            RiskLevel::Minimal
        };

        RiskAssessment {
            level,
            factors,
            mitigations: vec!["Run full test suite".to_string()],
        }
    }

    /// Find tests related to a symbol.
    async fn find_related_tests(&self, symbol_id: &str) -> Vec<String> {
        let symbols = self.symbols.read().await;
        let deps = self.dependents.read().await;

        // Find test symbols that depend on this symbol
        deps.get(symbol_id)
            .map(|dependents| {
                dependents
                    .iter()
                    .filter_map(|id| symbols.get(id))
                    .filter(|s| s.kind == SymbolKind::Test)
                    .map(|s| s.qualified_name.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get codebase statistics.
    pub async fn get_stats(&self) -> CodebaseStats {
        let symbols = self.symbols.read().await;
        let references = self.references.read().await;

        let mut symbols_by_kind: HashMap<String, usize> = HashMap::new();
        for symbol in symbols.values() {
            *symbols_by_kind
                .entry(format!("{:?}", symbol.kind))
                .or_insert(0) += 1;
        }

        let files = self.files.read().await;
        CodebaseStats {
            total_files: files.len(),
            total_symbols: symbols.len(),
            symbols_by_kind,
            total_references: references.len(),
            avg_complexity: 0.0, // Would need to calculate
            last_indexed: Utc::now(),
        }
    }

    /// Generate documentation for a symbol.
    pub async fn generate_docs(&self, symbol_id: &str) -> Result<String> {
        let symbol = self
            .get_symbol(symbol_id)
            .await
            .ok_or_else(|| CodebaseError::SymbolNotFound(symbol_id.to_string()))?;

        let files = self.files.read().await;
        let content = files
            .get(&symbol.file_path)
            .ok_or_else(|| CodebaseError::FileNotFound(symbol.file_path.clone()))?;

        self.provider.generate_docs(&symbol, content).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockProvider;

    #[async_trait]
    impl CodeAnalysisProvider for MockProvider {
        async fn parse_file(&self, path: &str, _content: &str) -> Result<Vec<Symbol>> {
            Ok(vec![
                Symbol {
                    id: "fn_main".to_string(),
                    name: "main".to_string(),
                    qualified_name: "crate::main".to_string(),
                    kind: SymbolKind::Function,
                    file_path: path.to_string(),
                    line: 1,
                    column: 0,
                    end_line: 10,
                    signature: Some("fn main()".to_string()),
                    documentation: None,
                    visibility: Visibility::Public,
                    parent: None,
                    metadata: HashMap::new(),
                },
                Symbol {
                    id: "fn_helper".to_string(),
                    name: "helper".to_string(),
                    qualified_name: "crate::helper".to_string(),
                    kind: SymbolKind::Function,
                    file_path: path.to_string(),
                    line: 12,
                    column: 0,
                    end_line: 20,
                    signature: Some("fn helper() -> i32".to_string()),
                    documentation: None,
                    visibility: Visibility::Private,
                    parent: None,
                    metadata: HashMap::new(),
                },
            ])
        }

        async fn extract_references(&self, path: &str, _content: &str) -> Result<Vec<Reference>> {
            Ok(vec![Reference {
                id: Uuid::new_v4().to_string(),
                from_symbol: "fn_main".to_string(),
                to_symbol: "fn_helper".to_string(),
                reference_type: ReferenceType::Call,
                location: Location {
                    file_path: path.to_string(),
                    start_line: 5,
                    start_column: 4,
                    end_line: 5,
                    end_column: 12,
                },
            }])
        }

        async fn analyze_complexity(&self, _symbol: &Symbol, _content: &str) -> Result<f64> {
            Ok(5.0)
        }

        async fn generate_docs(&self, symbol: &Symbol, _content: &str) -> Result<String> {
            Ok(format!("Documentation for {}", symbol.name))
        }
    }

    #[tokio::test]
    async fn test_index_file() {
        let provider = Arc::new(MockProvider);
        let analyzer = CodebaseAnalyzer::new(provider);

        let symbols = analyzer
            .index_file("src/main.rs", "fn main() {}")
            .await
            .unwrap();
        assert_eq!(symbols.len(), 2);
    }

    #[tokio::test]
    async fn test_find_symbols() {
        let provider = Arc::new(MockProvider);
        let analyzer = CodebaseAnalyzer::new(provider);

        analyzer
            .index_file("src/main.rs", "fn main() {}")
            .await
            .unwrap();

        let results = analyzer.find_symbols("main").await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "main");
    }

    #[tokio::test]
    async fn test_get_dependents() {
        let provider = Arc::new(MockProvider);
        let analyzer = CodebaseAnalyzer::new(provider);

        analyzer
            .index_file("src/main.rs", "fn main() { helper(); }")
            .await
            .unwrap();

        let dependents = analyzer.get_dependents("fn_helper").await;
        assert_eq!(dependents.len(), 1);
        assert_eq!(dependents[0].name, "main");
    }

    #[tokio::test]
    async fn test_impact_analysis() {
        let provider = Arc::new(MockProvider);
        let analyzer = CodebaseAnalyzer::new(provider);

        analyzer
            .index_file("src/main.rs", "fn main() { helper(); }")
            .await
            .unwrap();

        let impact = analyzer
            .analyze_impact("fn_helper", ImpactType::SignatureChange)
            .await
            .unwrap();
        assert_eq!(impact.direct_impacts.len(), 1);
    }

    #[tokio::test]
    async fn test_stats() {
        let provider = Arc::new(MockProvider);
        let analyzer = CodebaseAnalyzer::new(provider);

        analyzer
            .index_file("src/main.rs", "fn main() {}")
            .await
            .unwrap();

        let stats = analyzer.get_stats().await;
        assert_eq!(stats.total_files, 1);
        assert_eq!(stats.total_symbols, 2);
    }

    #[test]
    fn test_impact_severity_ordering() {
        assert!(ImpactSeverity::Breaking > ImpactSeverity::High);
        assert!(ImpactSeverity::High > ImpactSeverity::Medium);
    }

    #[test]
    fn test_risk_level_ordering() {
        assert!(RiskLevel::Critical > RiskLevel::High);
        assert!(RiskLevel::High > RiskLevel::Medium);
    }
}
