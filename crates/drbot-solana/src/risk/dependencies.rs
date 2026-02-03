//! Protocol dependency tracking.
//!
//! Tracks dependencies between DeFi protocols to identify
//! cascading risk exposure.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};

/// Graph of protocol dependencies.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolDependencyGraph {
    /// Protocol nodes.
    protocols: HashMap<String, ProtocolNode>,
    /// Dependency edges.
    edges: Vec<DependencyEdge>,
}

impl ProtocolDependencyGraph {
    /// Create a new dependency graph.
    pub fn new() -> Self {
        Self {
            protocols: HashMap::new(),
            edges: Vec::new(),
        }
    }

    /// Create with default Solana DeFi dependencies.
    pub fn solana_defaults() -> Self {
        let mut graph = Self::new();

        // Add major protocols
        graph.add_protocol(ProtocolNode {
            name: "Solend".to_string(),
            protocol_type: "Lending".to_string(),
            tvl_usd: 0.0,
            risk_tier: RiskTier::Established,
        });

        graph.add_protocol(ProtocolNode {
            name: "Marginfi".to_string(),
            protocol_type: "Lending".to_string(),
            tvl_usd: 0.0,
            risk_tier: RiskTier::Established,
        });

        graph.add_protocol(ProtocolNode {
            name: "Kamino".to_string(),
            protocol_type: "Vault".to_string(),
            tvl_usd: 0.0,
            risk_tier: RiskTier::Established,
        });

        graph.add_protocol(ProtocolNode {
            name: "Marinade".to_string(),
            protocol_type: "LiquidStaking".to_string(),
            tvl_usd: 0.0,
            risk_tier: RiskTier::Established,
        });

        graph.add_protocol(ProtocolNode {
            name: "Jito".to_string(),
            protocol_type: "LiquidStaking".to_string(),
            tvl_usd: 0.0,
            risk_tier: RiskTier::Established,
        });

        graph.add_protocol(ProtocolNode {
            name: "Jupiter".to_string(),
            protocol_type: "DEX".to_string(),
            tvl_usd: 0.0,
            risk_tier: RiskTier::Established,
        });

        graph.add_protocol(ProtocolNode {
            name: "Raydium".to_string(),
            protocol_type: "DEX".to_string(),
            tvl_usd: 0.0,
            risk_tier: RiskTier::Established,
        });

        graph.add_protocol(ProtocolNode {
            name: "Orca".to_string(),
            protocol_type: "DEX".to_string(),
            tvl_usd: 0.0,
            risk_tier: RiskTier::Established,
        });

        graph.add_protocol(ProtocolNode {
            name: "Pyth".to_string(),
            protocol_type: "Oracle".to_string(),
            tvl_usd: 0.0,
            risk_tier: RiskTier::Critical,
        });

        graph.add_protocol(ProtocolNode {
            name: "Switchboard".to_string(),
            protocol_type: "Oracle".to_string(),
            tvl_usd: 0.0,
            risk_tier: RiskTier::Critical,
        });

        // Add known dependencies

        // Lending protocols depend on oracles
        graph.add_dependency(DependencyEdge {
            from: "Solend".to_string(),
            to: "Pyth".to_string(),
            dependency_type: DependencyType::Oracle,
            criticality: Criticality::Critical,
        });

        graph.add_dependency(DependencyEdge {
            from: "Marginfi".to_string(),
            to: "Pyth".to_string(),
            dependency_type: DependencyType::Oracle,
            criticality: Criticality::Critical,
        });

        graph.add_dependency(DependencyEdge {
            from: "Marginfi".to_string(),
            to: "Switchboard".to_string(),
            dependency_type: DependencyType::Oracle,
            criticality: Criticality::Critical,
        });

        // Lending protocols accept LST collateral
        graph.add_dependency(DependencyEdge {
            from: "Solend".to_string(),
            to: "Marinade".to_string(),
            dependency_type: DependencyType::Collateral,
            criticality: Criticality::High,
        });

        graph.add_dependency(DependencyEdge {
            from: "Solend".to_string(),
            to: "Jito".to_string(),
            dependency_type: DependencyType::Collateral,
            criticality: Criticality::High,
        });

        graph.add_dependency(DependencyEdge {
            from: "Marginfi".to_string(),
            to: "Marinade".to_string(),
            dependency_type: DependencyType::Collateral,
            criticality: Criticality::High,
        });

        graph.add_dependency(DependencyEdge {
            from: "Marginfi".to_string(),
            to: "Jito".to_string(),
            dependency_type: DependencyType::Collateral,
            criticality: Criticality::High,
        });

        // Kamino uses DEX liquidity
        graph.add_dependency(DependencyEdge {
            from: "Kamino".to_string(),
            to: "Orca".to_string(),
            dependency_type: DependencyType::Liquidity,
            criticality: Criticality::High,
        });

        graph.add_dependency(DependencyEdge {
            from: "Kamino".to_string(),
            to: "Raydium".to_string(),
            dependency_type: DependencyType::Liquidity,
            criticality: Criticality::High,
        });

        // Jupiter aggregates DEXes
        graph.add_dependency(DependencyEdge {
            from: "Jupiter".to_string(),
            to: "Orca".to_string(),
            dependency_type: DependencyType::Liquidity,
            criticality: Criticality::Medium,
        });

        graph.add_dependency(DependencyEdge {
            from: "Jupiter".to_string(),
            to: "Raydium".to_string(),
            dependency_type: DependencyType::Liquidity,
            criticality: Criticality::Medium,
        });

        graph
    }

    /// Add a protocol to the graph.
    pub fn add_protocol(&mut self, protocol: ProtocolNode) {
        self.protocols.insert(protocol.name.clone(), protocol);
    }

    /// Add a dependency edge.
    pub fn add_dependency(&mut self, edge: DependencyEdge) {
        self.edges.push(edge);
    }

    /// Get dependencies for a protocol.
    pub fn get_dependencies(&self, protocol: &str) -> Vec<&DependencyEdge> {
        self.edges.iter().filter(|e| e.from == protocol).collect()
    }

    /// Get protocols that depend on a given protocol.
    pub fn get_dependents(&self, protocol: &str) -> Vec<&DependencyEdge> {
        self.edges.iter().filter(|e| e.to == protocol).collect()
    }

    /// Get the full dependency chain for a protocol (BFS).
    pub fn get_dependency_chain(&self, protocol: &str) -> Vec<String> {
        let mut chain = Vec::new();
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();

        queue.push_back(protocol.to_string());
        visited.insert(protocol.to_string());

        while let Some(current) = queue.pop_front() {
            for dep in self.get_dependencies(&current) {
                if !visited.contains(&dep.to) {
                    visited.insert(dep.to.clone());
                    chain.push(dep.to.clone());
                    queue.push_back(dep.to.clone());
                }
            }
        }

        chain
    }

    /// Find all protocols affected if a given protocol fails.
    pub fn get_impact_zone(&self, protocol: &str) -> Vec<String> {
        let mut affected = Vec::new();
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();

        queue.push_back(protocol.to_string());
        visited.insert(protocol.to_string());

        while let Some(current) = queue.pop_front() {
            for dep in self.get_dependents(&current) {
                if !visited.contains(&dep.from) {
                    visited.insert(dep.from.clone());
                    affected.push(dep.from.clone());
                    queue.push_back(dep.from.clone());
                }
            }
        }

        affected
    }

    /// Analyze systemic risk for a set of positions.
    pub fn analyze_systemic_risk(&self, protocols: &[String]) -> SystemicRiskAnalysis {
        let mut shared_dependencies: HashMap<String, Vec<String>> = HashMap::new();
        let mut critical_dependencies = Vec::new();

        // Find all dependencies
        for protocol in protocols {
            for dep in self.get_dependency_chain(protocol) {
                shared_dependencies
                    .entry(dep.clone())
                    .or_default()
                    .push(protocol.clone());
            }
        }

        // Identify critical shared dependencies
        for (dep, dependent_protocols) in &shared_dependencies {
            if dependent_protocols.len() > 1 {
                // Get the dependency type
                let dep_edge = self.edges.iter().find(|e| e.to == *dep);
                let dep_type = dep_edge
                    .map(|e| e.dependency_type.clone())
                    .unwrap_or(DependencyType::Other);

                critical_dependencies.push(CriticalDependency {
                    protocol: dep.clone(),
                    dependent_count: dependent_protocols.len(),
                    dependents: dependent_protocols.clone(),
                    dependency_type: dep_type,
                });
            }
        }

        // Sort by impact (number of dependents)
        critical_dependencies.sort_by(|a, b| b.dependent_count.cmp(&a.dependent_count));

        // Calculate risk score
        let risk_score =
            self.calculate_systemic_risk_score(&critical_dependencies, protocols.len());

        SystemicRiskAnalysis {
            protocols_analyzed: protocols.to_vec(),
            shared_dependencies,
            critical_dependencies,
            systemic_risk_score: risk_score,
        }
    }

    /// Calculate systemic risk score (0-10).
    fn calculate_systemic_risk_score(
        &self,
        critical_deps: &[CriticalDependency],
        total_protocols: usize,
    ) -> u8 {
        if total_protocols == 0 || critical_deps.is_empty() {
            return 1;
        }

        let mut score = 2u8;

        // Add risk for critical dependencies
        for dep in critical_deps {
            let impact_ratio = dep.dependent_count as f64 / total_protocols as f64;

            match dep.dependency_type {
                DependencyType::Oracle => {
                    // Oracle dependencies are highest risk
                    if impact_ratio > 0.5 {
                        score += 3;
                    } else {
                        score += 2;
                    }
                }
                DependencyType::Collateral => {
                    if impact_ratio > 0.5 {
                        score += 2;
                    } else {
                        score += 1;
                    }
                }
                DependencyType::Liquidity => {
                    if impact_ratio > 0.5 {
                        score += 2;
                    } else {
                        score += 1;
                    }
                }
                _ => {
                    score += 1;
                }
            }
        }

        score.min(10)
    }

    /// Get all protocols.
    pub fn protocols(&self) -> &HashMap<String, ProtocolNode> {
        &self.protocols
    }

    /// Get all edges.
    pub fn edges(&self) -> &[DependencyEdge] {
        &self.edges
    }
}

impl Default for ProtocolDependencyGraph {
    fn default() -> Self {
        Self::solana_defaults()
    }
}

/// A protocol node in the dependency graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolNode {
    /// Protocol name.
    pub name: String,
    /// Protocol type (Lending, DEX, etc.).
    pub protocol_type: String,
    /// Total value locked.
    pub tvl_usd: f64,
    /// Risk tier.
    pub risk_tier: RiskTier,
}

/// Risk tier for protocols.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskTier {
    /// Critical infrastructure (oracles, bridges).
    Critical,
    /// Established, battle-tested protocols.
    Established,
    /// Growing protocols with good track record.
    Growing,
    /// New or experimental protocols.
    Experimental,
}

/// A dependency edge between protocols.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyEdge {
    /// Protocol that has the dependency.
    pub from: String,
    /// Protocol being depended upon.
    pub to: String,
    /// Type of dependency.
    pub dependency_type: DependencyType,
    /// How critical this dependency is.
    pub criticality: Criticality,
}

/// Types of protocol dependencies.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DependencyType {
    /// Oracle price feed dependency.
    Oracle,
    /// Collateral dependency (accepts as collateral).
    Collateral,
    /// Liquidity dependency (uses for swaps/trades).
    Liquidity,
    /// Bridge dependency.
    Bridge,
    /// Other dependency.
    Other,
}

/// Criticality of a dependency.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Criticality {
    /// Failure would cause complete protocol failure.
    Critical,
    /// Failure would significantly impact protocol.
    High,
    /// Failure would have moderate impact.
    Medium,
    /// Failure would have minimal impact.
    Low,
}

/// Analysis of systemic risk across protocols.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemicRiskAnalysis {
    /// Protocols that were analyzed.
    pub protocols_analyzed: Vec<String>,
    /// Dependencies shared between protocols.
    pub shared_dependencies: HashMap<String, Vec<String>>,
    /// Critical dependencies that affect multiple protocols.
    pub critical_dependencies: Vec<CriticalDependency>,
    /// Overall systemic risk score (1-10).
    pub systemic_risk_score: u8,
}

/// A critical dependency affecting multiple protocols.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CriticalDependency {
    /// The dependency protocol.
    pub protocol: String,
    /// Number of protocols that depend on it.
    pub dependent_count: usize,
    /// Names of dependent protocols.
    pub dependents: Vec<String>,
    /// Type of dependency.
    pub dependency_type: DependencyType,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dependency_graph() {
        let graph = ProtocolDependencyGraph::solana_defaults();

        // Check protocols exist
        assert!(graph.protocols.contains_key("Solend"));
        assert!(graph.protocols.contains_key("Pyth"));

        // Check dependencies
        let solend_deps = graph.get_dependencies("Solend");
        assert!(!solend_deps.is_empty());

        // Should depend on Pyth
        assert!(solend_deps.iter().any(|d| d.to == "Pyth"));
    }

    #[test]
    fn test_dependency_chain() {
        let graph = ProtocolDependencyGraph::solana_defaults();

        let chain = graph.get_dependency_chain("Solend");
        assert!(!chain.is_empty());
        assert!(chain.contains(&"Pyth".to_string()));
    }

    #[test]
    fn test_impact_zone() {
        let graph = ProtocolDependencyGraph::solana_defaults();

        // If Pyth fails, many protocols are affected
        let impact = graph.get_impact_zone("Pyth");
        assert!(impact.contains(&"Solend".to_string()));
        assert!(impact.contains(&"Marginfi".to_string()));
    }

    #[test]
    fn test_systemic_risk() {
        let graph = ProtocolDependencyGraph::solana_defaults();

        let analysis = graph.analyze_systemic_risk(&["Solend".to_string(), "Marginfi".to_string()]);

        // Both depend on Pyth
        assert!(!analysis.critical_dependencies.is_empty());
        assert!(analysis.systemic_risk_score >= 2);
    }
}
