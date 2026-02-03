//! Outcome simulation and what-if analysis.
//!
//! This crate provides:
//! - Scenario simulation
//! - Outcome prediction
//! - Monte Carlo analysis
//! - Decision trees

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Simulation errors.
#[derive(Debug, Error)]
pub enum SimulationError {
    #[error("Simulation failed: {0}")]
    SimulationFailed(String),

    #[error("Invalid scenario: {0}")]
    InvalidScenario(String),

    #[error("Provider error: {0}")]
    ProviderError(String),
}

/// Result type for simulation operations.
pub type Result<T> = std::result::Result<T, SimulationError>;

/// A scenario to simulate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scenario {
    /// Scenario identifier.
    pub id: String,
    /// Scenario name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Initial conditions.
    pub initial_state: HashMap<String, StateValue>,
    /// Actions/decisions to simulate.
    pub actions: Vec<Action>,
    /// Constraints.
    pub constraints: Vec<Constraint>,
    /// Time horizon.
    pub time_horizon: Option<TimeHorizon>,
    /// Created at.
    pub created_at: DateTime<Utc>,
}

/// A state value.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StateValue {
    Number(f64),
    Text(String),
    Boolean(bool),
    List(Vec<StateValue>),
    Map(HashMap<String, StateValue>),
}

/// An action to simulate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Action {
    /// Action identifier.
    pub id: String,
    /// Action name.
    pub name: String,
    /// Action description.
    pub description: String,
    /// Parameters.
    pub parameters: HashMap<String, StateValue>,
    /// Timing (when in sequence).
    pub timing: ActionTiming,
}

/// Action timing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ActionTiming {
    Immediate,
    Delayed { days: i32 },
    Conditional { condition: String },
    Sequence(usize),
}

/// A constraint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Constraint {
    /// Constraint name.
    pub name: String,
    /// Expression.
    pub expression: String,
    /// Is hard constraint.
    pub hard: bool,
}

/// Time horizon.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct TimeHorizon {
    /// Duration in days.
    pub days: i32,
    /// Evaluation points.
    pub checkpoints: i32,
}

/// Simulation result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationResult {
    /// Result identifier.
    pub id: String,
    /// Scenario used.
    pub scenario_id: String,
    /// Outcomes.
    pub outcomes: Vec<Outcome>,
    /// Probability distribution.
    pub distribution: OutcomeDistribution,
    /// Recommendations.
    pub recommendations: Vec<Recommendation>,
    /// Risk assessment.
    pub risk_assessment: RiskAssessment,
    /// Simulation timestamp.
    pub timestamp: DateTime<Utc>,
}

/// A simulated outcome.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Outcome {
    /// Outcome identifier.
    pub id: String,
    /// Outcome name.
    pub name: String,
    /// Final state.
    pub final_state: HashMap<String, StateValue>,
    /// Probability.
    pub probability: f64,
    /// Key events.
    pub key_events: Vec<SimEvent>,
    /// Is desirable.
    pub desirable: bool,
    /// Score.
    pub score: f64,
}

/// A simulated event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimEvent {
    /// Event name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Timing (day in simulation).
    pub day: i32,
    /// Impact.
    pub impact: EventImpact,
}

/// Event impact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventImpact {
    VeryPositive,
    Positive,
    Neutral,
    Negative,
    VeryNegative,
}

/// Outcome distribution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutcomeDistribution {
    /// Best case outcome.
    pub best_case: String,
    /// Best case probability.
    pub best_case_prob: f64,
    /// Worst case outcome.
    pub worst_case: String,
    /// Worst case probability.
    pub worst_case_prob: f64,
    /// Most likely outcome.
    pub most_likely: String,
    /// Most likely probability.
    pub most_likely_prob: f64,
    /// Expected value.
    pub expected_value: f64,
    /// Standard deviation.
    pub std_deviation: f64,
}

/// A recommendation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recommendation {
    /// Recommendation identifier.
    pub id: String,
    /// Action to take.
    pub action: String,
    /// Rationale.
    pub rationale: String,
    /// Expected improvement.
    pub expected_improvement: f64,
    /// Confidence.
    pub confidence: f64,
    /// Priority.
    pub priority: Priority,
}

/// Priority levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Priority {
    Low,
    Medium,
    High,
    Critical,
}

/// Risk assessment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskAssessment {
    /// Overall risk level.
    pub risk_level: RiskLevel,
    /// Risk score (0-1).
    pub risk_score: f64,
    /// Key risks.
    pub key_risks: Vec<Risk>,
    /// Mitigation strategies.
    pub mitigations: Vec<Mitigation>,
}

/// Risk levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

/// A risk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Risk {
    /// Risk name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Probability.
    pub probability: f64,
    /// Impact severity.
    pub impact: EventImpact,
    /// Triggers.
    pub triggers: Vec<String>,
}

/// A mitigation strategy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mitigation {
    /// Strategy name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Risk reduction.
    pub risk_reduction: f64,
    /// Cost/effort.
    pub cost: f64,
}

/// Decision tree node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionNode {
    /// Node identifier.
    pub id: String,
    /// Node type.
    pub node_type: NodeType,
    /// Label.
    pub label: String,
    /// Children.
    pub children: Vec<DecisionNode>,
    /// Probability (for chance nodes).
    pub probability: Option<f64>,
    /// Value (for terminal nodes).
    pub value: Option<f64>,
}

/// Decision tree node types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeType {
    Decision,
    Chance,
    Terminal,
}

/// Simulation provider.
#[async_trait]
pub trait SimulationProvider: Send + Sync {
    /// Run simulation.
    async fn simulate(&self, scenario: &Scenario, iterations: usize) -> Result<Vec<Outcome>>;

    /// Generate decision tree.
    async fn generate_decision_tree(&self, scenario: &Scenario) -> Result<DecisionNode>;

    /// Assess risks.
    async fn assess_risks(&self, scenario: &Scenario) -> Result<RiskAssessment>;
}

/// The simulation engine.
pub struct SimulationEngine {
    /// Provider.
    provider: Arc<dyn SimulationProvider>,
    /// Stored scenarios.
    scenarios: Arc<RwLock<HashMap<String, Scenario>>>,
    /// Simulation history.
    history: Arc<RwLock<Vec<SimulationResult>>>,
    /// Default iterations.
    default_iterations: usize,
}

impl SimulationEngine {
    /// Create a new simulation engine.
    pub fn new(provider: Arc<dyn SimulationProvider>) -> Self {
        Self {
            provider,
            scenarios: Arc::new(RwLock::new(HashMap::new())),
            history: Arc::new(RwLock::new(Vec::new())),
            default_iterations: 1000,
        }
    }

    /// Set default iterations.
    pub fn with_iterations(mut self, iterations: usize) -> Self {
        self.default_iterations = iterations;
        self
    }

    /// Create a scenario.
    pub async fn create_scenario(&self, name: &str, description: &str) -> Scenario {
        let scenario = Scenario {
            id: Uuid::new_v4().to_string(),
            name: name.to_string(),
            description: description.to_string(),
            initial_state: HashMap::new(),
            actions: Vec::new(),
            constraints: Vec::new(),
            time_horizon: None,
            created_at: Utc::now(),
        };

        let mut scenarios = self.scenarios.write().await;
        scenarios.insert(scenario.id.clone(), scenario.clone());

        scenario
    }

    /// Run simulation.
    pub async fn simulate(
        &self,
        scenario: &Scenario,
        iterations: Option<usize>,
    ) -> Result<SimulationResult> {
        let iters = iterations.unwrap_or(self.default_iterations);

        let outcomes = self.provider.simulate(scenario, iters).await?;
        let risk_assessment = self.provider.assess_risks(scenario).await?;

        // Calculate distribution
        let (best, worst, most_likely) = self.analyze_outcomes(&outcomes);

        let expected_value = outcomes.iter().map(|o| o.score * o.probability).sum();

        let mut variance: f64 = 0.0;
        for o in outcomes.iter() {
            let diff = o.score - expected_value;
            variance += diff * diff * o.probability;
        }
        let std_deviation = variance.sqrt();

        let distribution = OutcomeDistribution {
            best_case: best.0,
            best_case_prob: best.1,
            worst_case: worst.0,
            worst_case_prob: worst.1,
            most_likely: most_likely.0,
            most_likely_prob: most_likely.1,
            expected_value,
            std_deviation,
        };

        // Generate recommendations
        let recommendations = self.generate_recommendations(&outcomes, &risk_assessment);

        let result = SimulationResult {
            id: Uuid::new_v4().to_string(),
            scenario_id: scenario.id.clone(),
            outcomes,
            distribution,
            recommendations,
            risk_assessment,
            timestamp: Utc::now(),
        };

        let mut history = self.history.write().await;
        history.push(result.clone());
        if history.len() > 1000 {
            history.drain(0..100);
        }

        Ok(result)
    }

    /// Analyze outcomes.
    fn analyze_outcomes(
        &self,
        outcomes: &[Outcome],
    ) -> ((String, f64), (String, f64), (String, f64)) {
        let best = outcomes
            .iter()
            .max_by(|a, b| a.score.partial_cmp(&b.score).unwrap())
            .map(|o| (o.id.clone(), o.probability))
            .unwrap_or(("none".to_string(), 0.0));

        let worst = outcomes
            .iter()
            .min_by(|a, b| a.score.partial_cmp(&b.score).unwrap())
            .map(|o| (o.id.clone(), o.probability))
            .unwrap_or(("none".to_string(), 0.0));

        let most_likely = outcomes
            .iter()
            .max_by(|a, b| a.probability.partial_cmp(&b.probability).unwrap())
            .map(|o| (o.id.clone(), o.probability))
            .unwrap_or(("none".to_string(), 0.0));

        (best, worst, most_likely)
    }

    /// Generate recommendations.
    fn generate_recommendations(
        &self,
        outcomes: &[Outcome],
        risk_assessment: &RiskAssessment,
    ) -> Vec<Recommendation> {
        let mut recommendations = Vec::new();

        // Recommend based on outcomes
        let desirable_outcomes: Vec<_> = outcomes.iter().filter(|o| o.desirable).collect();
        let _undesirable_outcomes: Vec<_> = outcomes.iter().filter(|o| !o.desirable).collect();

        if desirable_outcomes
            .iter()
            .map(|o| o.probability)
            .sum::<f64>()
            < 0.5
        {
            recommendations.push(Recommendation {
                id: Uuid::new_v4().to_string(),
                action: "Consider alternative approach".to_string(),
                rationale: "Probability of desirable outcomes is below 50%".to_string(),
                expected_improvement: 0.2,
                confidence: 0.7,
                priority: Priority::High,
            });
        }

        // Recommend based on risks
        for risk in &risk_assessment.key_risks {
            if risk.probability > 0.3 {
                recommendations.push(Recommendation {
                    id: Uuid::new_v4().to_string(),
                    action: format!("Mitigate risk: {}", risk.name),
                    rationale: risk.description.clone(),
                    expected_improvement: risk.probability * 0.5,
                    confidence: 0.6,
                    priority: if risk.probability > 0.5 {
                        Priority::High
                    } else {
                        Priority::Medium
                    },
                });
            }
        }

        recommendations
    }

    /// Generate decision tree.
    pub async fn generate_decision_tree(&self, scenario: &Scenario) -> Result<DecisionNode> {
        self.provider.generate_decision_tree(scenario).await
    }

    /// Run what-if analysis.
    pub async fn what_if(
        &self,
        scenario: &Scenario,
        modifications: HashMap<String, StateValue>,
    ) -> Result<SimulationResult> {
        let mut modified_scenario = scenario.clone();
        modified_scenario.id = Uuid::new_v4().to_string();
        modified_scenario.initial_state.extend(modifications);

        self.simulate(&modified_scenario, None).await
    }

    /// Compare scenarios.
    pub async fn compare(&self, scenarios: &[&Scenario]) -> Result<Vec<SimulationResult>> {
        let mut results = Vec::new();

        for scenario in scenarios {
            let result = self.simulate(scenario, None).await?;
            results.push(result);
        }

        Ok(results)
    }

    /// Get simulation history.
    pub async fn get_history(&self, limit: usize) -> Vec<SimulationResult> {
        let history = self.history.read().await;
        history.iter().rev().take(limit).cloned().collect()
    }

    /// Get stored scenario.
    pub async fn get_scenario(&self, id: &str) -> Option<Scenario> {
        let scenarios = self.scenarios.read().await;
        scenarios.get(id).cloned()
    }

    /// List scenarios.
    pub async fn list_scenarios(&self) -> Vec<Scenario> {
        let scenarios = self.scenarios.read().await;
        scenarios.values().cloned().collect()
    }
}

/// Builder for scenarios.
pub struct ScenarioBuilder {
    scenario: Scenario,
}

impl ScenarioBuilder {
    /// Create a new scenario builder.
    pub fn new(name: &str) -> Self {
        Self {
            scenario: Scenario {
                id: Uuid::new_v4().to_string(),
                name: name.to_string(),
                description: String::new(),
                initial_state: HashMap::new(),
                actions: Vec::new(),
                constraints: Vec::new(),
                time_horizon: None,
                created_at: Utc::now(),
            },
        }
    }

    /// Set description.
    pub fn description(mut self, desc: &str) -> Self {
        self.scenario.description = desc.to_string();
        self
    }

    /// Add initial state.
    pub fn state(mut self, key: &str, value: StateValue) -> Self {
        self.scenario.initial_state.insert(key.to_string(), value);
        self
    }

    /// Add action.
    pub fn action(mut self, name: &str, description: &str) -> Self {
        self.scenario.actions.push(Action {
            id: Uuid::new_v4().to_string(),
            name: name.to_string(),
            description: description.to_string(),
            parameters: HashMap::new(),
            timing: ActionTiming::Immediate,
        });
        self
    }

    /// Add constraint.
    pub fn constraint(mut self, name: &str, expression: &str, hard: bool) -> Self {
        self.scenario.constraints.push(Constraint {
            name: name.to_string(),
            expression: expression.to_string(),
            hard,
        });
        self
    }

    /// Set time horizon.
    pub fn time_horizon(mut self, days: i32, checkpoints: i32) -> Self {
        self.scenario.time_horizon = Some(TimeHorizon { days, checkpoints });
        self
    }

    /// Build the scenario.
    pub fn build(self) -> Scenario {
        self.scenario
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockProvider;

    #[async_trait]
    impl SimulationProvider for MockProvider {
        async fn simulate(&self, _scenario: &Scenario, iterations: usize) -> Result<Vec<Outcome>> {
            Ok(vec![
                Outcome {
                    id: "outcome1".to_string(),
                    name: "Success".to_string(),
                    final_state: HashMap::new(),
                    probability: 0.6,
                    key_events: vec![],
                    desirable: true,
                    score: 0.8,
                },
                Outcome {
                    id: "outcome2".to_string(),
                    name: "Partial Success".to_string(),
                    final_state: HashMap::new(),
                    probability: 0.3,
                    key_events: vec![],
                    desirable: true,
                    score: 0.5,
                },
                Outcome {
                    id: "outcome3".to_string(),
                    name: "Failure".to_string(),
                    final_state: HashMap::new(),
                    probability: 0.1,
                    key_events: vec![],
                    desirable: false,
                    score: 0.1,
                },
            ])
        }

        async fn generate_decision_tree(&self, _scenario: &Scenario) -> Result<DecisionNode> {
            Ok(DecisionNode {
                id: "root".to_string(),
                node_type: NodeType::Decision,
                label: "Main Decision".to_string(),
                children: vec![DecisionNode {
                    id: "opt1".to_string(),
                    node_type: NodeType::Terminal,
                    label: "Option A".to_string(),
                    children: vec![],
                    probability: None,
                    value: Some(100.0),
                }],
                probability: None,
                value: None,
            })
        }

        async fn assess_risks(&self, _scenario: &Scenario) -> Result<RiskAssessment> {
            Ok(RiskAssessment {
                risk_level: RiskLevel::Medium,
                risk_score: 0.4,
                key_risks: vec![Risk {
                    name: "Market Risk".to_string(),
                    description: "Market conditions may change".to_string(),
                    probability: 0.3,
                    impact: EventImpact::Negative,
                    triggers: vec!["economic downturn".to_string()],
                }],
                mitigations: vec![Mitigation {
                    name: "Diversification".to_string(),
                    description: "Spread investments".to_string(),
                    risk_reduction: 0.2,
                    cost: 0.1,
                }],
            })
        }
    }

    #[tokio::test]
    async fn test_create_scenario() {
        let provider = Arc::new(MockProvider);
        let engine = SimulationEngine::new(provider);

        let scenario = engine.create_scenario("Test", "A test scenario").await;
        assert_eq!(scenario.name, "Test");
    }

    #[tokio::test]
    async fn test_simulate() {
        let provider = Arc::new(MockProvider);
        let engine = SimulationEngine::new(provider);

        let scenario = ScenarioBuilder::new("Investment")
            .description("Test investment scenario")
            .state("budget", StateValue::Number(10000.0))
            .action("invest", "Make investment")
            .build();

        let result = engine.simulate(&scenario, Some(100)).await.unwrap();

        assert!(!result.outcomes.is_empty());
        assert!(result.distribution.expected_value > 0.0);
    }

    #[tokio::test]
    async fn test_decision_tree() {
        let provider = Arc::new(MockProvider);
        let engine = SimulationEngine::new(provider);

        let scenario = ScenarioBuilder::new("Decision").build();
        let tree = engine.generate_decision_tree(&scenario).await.unwrap();

        assert_eq!(tree.node_type, NodeType::Decision);
        assert!(!tree.children.is_empty());
    }

    #[tokio::test]
    async fn test_scenario_builder() {
        let scenario = ScenarioBuilder::new("Test")
            .description("A test")
            .state("x", StateValue::Number(10.0))
            .action("do_thing", "Do something")
            .constraint("budget", "x <= 100", true)
            .time_horizon(30, 5)
            .build();

        assert_eq!(scenario.name, "Test");
        assert!(!scenario.initial_state.is_empty());
        assert_eq!(scenario.actions.len(), 1);
        assert_eq!(scenario.constraints.len(), 1);
    }

    #[tokio::test]
    async fn test_what_if() {
        let provider = Arc::new(MockProvider);
        let engine = SimulationEngine::new(provider);

        let scenario = ScenarioBuilder::new("Base")
            .state("budget", StateValue::Number(1000.0))
            .build();

        let mut modifications = HashMap::new();
        modifications.insert("budget".to_string(), StateValue::Number(2000.0));

        let result = engine.what_if(&scenario, modifications).await.unwrap();
        assert!(!result.outcomes.is_empty());
    }
}
