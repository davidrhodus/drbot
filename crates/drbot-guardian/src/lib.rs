//! Proactive monitoring with security alerts and anomaly detection.
//!
//! This crate provides guardian capabilities that:
//! - Monitor system and user activity for anomalies
//! - Detect potential security issues proactively
//! - Generate intelligent alerts with context
//! - Learn normal patterns to identify deviations

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Guardian errors.
#[derive(Debug, Error)]
pub enum GuardianError {
    #[error("Monitoring failed: {0}")]
    MonitoringFailed(String),

    #[error("Alert creation failed: {0}")]
    AlertCreationFailed(String),

    #[error("Rule not found: {0}")]
    RuleNotFound(String),

    #[error("Pattern analysis failed: {0}")]
    PatternAnalysisFailed(String),

    #[error("Provider error: {0}")]
    ProviderError(String),
}

/// Result type for guardian operations.
pub type Result<T> = std::result::Result<T, GuardianError>;

/// Severity levels for alerts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord)]
pub enum Severity {
    Info,
    Low,
    Medium,
    High,
    Critical,
}

/// Alert status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlertStatus {
    /// Alert is active and unacknowledged.
    Active,
    /// Alert has been acknowledged.
    Acknowledged,
    /// Alert is being investigated.
    Investigating,
    /// Alert has been resolved.
    Resolved,
    /// Alert was a false positive.
    FalsePositive,
}

/// A security or anomaly alert.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alert {
    /// Unique alert identifier.
    pub id: String,
    /// Alert title.
    pub title: String,
    /// Detailed description.
    pub description: String,
    /// Alert severity.
    pub severity: Severity,
    /// Alert category.
    pub category: AlertCategory,
    /// Current status.
    pub status: AlertStatus,
    /// Source of the alert.
    pub source: AlertSource,
    /// Related context.
    pub context: AlertContext,
    /// Suggested actions.
    pub suggested_actions: Vec<SuggestedAction>,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Last update timestamp.
    pub updated_at: DateTime<Utc>,
    /// Resolution timestamp.
    pub resolved_at: Option<DateTime<Utc>>,
    /// Alert metadata.
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Categories of alerts.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum AlertCategory {
    /// Security-related alert.
    Security,
    /// Performance anomaly.
    Performance,
    /// Error rate spike.
    ErrorRate,
    /// Resource usage alert.
    Resource,
    /// Configuration issue.
    Configuration,
    /// Behavioral anomaly.
    Behavioral,
    /// Data integrity issue.
    DataIntegrity,
    /// Compliance violation.
    Compliance,
    /// Custom category.
    Custom(String),
}

/// Source of an alert.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertSource {
    /// Source type.
    pub source_type: SourceType,
    /// Source identifier.
    pub identifier: String,
    /// Additional source info.
    pub details: HashMap<String, String>,
}

/// Types of alert sources.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SourceType {
    /// System monitoring.
    System,
    /// Application logs.
    Application,
    /// User activity.
    UserActivity,
    /// Network traffic.
    Network,
    /// API calls.
    Api,
    /// Database operations.
    Database,
    /// External service.
    External,
    /// Custom source.
    Custom(String),
}

/// Context for an alert.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertContext {
    /// Related events.
    pub related_events: Vec<Event>,
    /// Affected resources.
    pub affected_resources: Vec<String>,
    /// Historical pattern.
    pub pattern: Option<Pattern>,
    /// Deviation from normal.
    pub deviation: Option<Deviation>,
}

/// An event in the monitoring system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    /// Event identifier.
    pub id: String,
    /// Event type.
    pub event_type: String,
    /// Event timestamp.
    pub timestamp: DateTime<Utc>,
    /// Event data.
    pub data: serde_json::Value,
    /// Event severity.
    pub severity: Severity,
}

/// A pattern detected in events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pattern {
    /// Pattern identifier.
    pub id: String,
    /// Pattern name.
    pub name: String,
    /// Pattern description.
    pub description: String,
    /// Frequency of pattern.
    pub frequency: f64,
    /// Confidence in pattern.
    pub confidence: f64,
    /// Pattern examples.
    pub examples: Vec<String>,
}

/// Deviation from normal behavior.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Deviation {
    /// Normal baseline value.
    pub baseline: f64,
    /// Current observed value.
    pub observed: f64,
    /// Standard deviations from normal.
    pub sigma: f64,
    /// Metric name.
    pub metric: String,
}

/// A suggested action for an alert.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuggestedAction {
    /// Action description.
    pub description: String,
    /// Action priority.
    pub priority: u32,
    /// Whether this can be automated.
    pub automatable: bool,
    /// Command to execute if automatable.
    pub command: Option<String>,
}

/// A monitoring rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringRule {
    /// Rule identifier.
    pub id: String,
    /// Rule name.
    pub name: String,
    /// Rule description.
    pub description: String,
    /// Rule condition.
    pub condition: RuleCondition,
    /// Alert settings.
    pub alert_settings: AlertSettings,
    /// Whether rule is enabled.
    pub enabled: bool,
    /// Cool-down period in seconds.
    pub cooldown_secs: u32,
    /// Last triggered timestamp.
    pub last_triggered: Option<DateTime<Utc>>,
}

/// Condition for triggering a rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RuleCondition {
    /// Threshold condition.
    Threshold {
        metric: String,
        operator: ComparisonOp,
        value: f64,
    },
    /// Rate of change condition.
    RateOfChange {
        metric: String,
        window_secs: u32,
        threshold: f64,
    },
    /// Anomaly detection.
    Anomaly { metric: String, sensitivity: f64 },
    /// Pattern match.
    Pattern { regex: String, source: String },
    /// Composite condition.
    Composite {
        operator: LogicalOp,
        conditions: Vec<RuleCondition>,
    },
}

/// Comparison operators.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ComparisonOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

/// Logical operators.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum LogicalOp {
    And,
    Or,
    Not,
}

/// Settings for alert generation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertSettings {
    /// Alert severity.
    pub severity: Severity,
    /// Alert category.
    pub category: AlertCategory,
    /// Title template.
    pub title_template: String,
    /// Description template.
    pub description_template: String,
    /// Auto-resolve after seconds.
    pub auto_resolve_secs: Option<u32>,
}

/// Provider for guardian capabilities.
#[async_trait]
pub trait GuardianProvider: Send + Sync {
    /// Analyze events for anomalies.
    async fn analyze_events(&self, events: &[Event]) -> Result<Vec<Deviation>>;

    /// Detect patterns in events.
    async fn detect_patterns(&self, events: &[Event]) -> Result<Vec<Pattern>>;

    /// Generate suggested actions for an alert.
    async fn suggest_actions(&self, alert: &Alert) -> Result<Vec<SuggestedAction>>;

    /// Classify alert severity.
    async fn classify_severity(&self, context: &AlertContext) -> Result<Severity>;
}

/// The guardian monitoring system.
pub struct Guardian {
    /// Provider for analysis.
    provider: Arc<dyn GuardianProvider>,
    /// Active alerts.
    alerts: Arc<RwLock<HashMap<String, Alert>>>,
    /// Monitoring rules.
    rules: Arc<RwLock<HashMap<String, MonitoringRule>>>,
    /// Event history.
    events: Arc<RwLock<VecDeque<Event>>>,
    /// Baseline metrics.
    baselines: Arc<RwLock<HashMap<String, MetricBaseline>>>,
    /// Configuration.
    config: GuardianConfig,
}

/// Configuration for the guardian.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardianConfig {
    /// Maximum events to keep in history.
    pub max_events: usize,
    /// Anomaly detection sensitivity (0.0-1.0).
    pub anomaly_sensitivity: f64,
    /// Baseline window in seconds.
    pub baseline_window_secs: u32,
    /// Minimum alerts before pattern detection.
    pub min_alerts_for_pattern: u32,
}

impl Default for GuardianConfig {
    fn default() -> Self {
        Self {
            max_events: 10000,
            anomaly_sensitivity: 0.8,
            baseline_window_secs: 3600,
            min_alerts_for_pattern: 5,
        }
    }
}

/// Baseline for a metric.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricBaseline {
    /// Metric name.
    pub metric: String,
    /// Mean value.
    pub mean: f64,
    /// Standard deviation.
    pub std_dev: f64,
    /// Sample count.
    pub sample_count: u32,
    /// Last update.
    pub updated_at: DateTime<Utc>,
}

impl Guardian {
    /// Create a new guardian.
    pub fn new(provider: Arc<dyn GuardianProvider>, config: GuardianConfig) -> Self {
        Self {
            provider,
            alerts: Arc::new(RwLock::new(HashMap::new())),
            rules: Arc::new(RwLock::new(HashMap::new())),
            events: Arc::new(RwLock::new(VecDeque::new())),
            baselines: Arc::new(RwLock::new(HashMap::new())),
            config,
        }
    }

    /// Record an event.
    pub async fn record_event(&self, event: Event) -> Result<Option<Alert>> {
        // Add to history
        let mut events = self.events.write().await;
        events.push_back(event.clone());
        if events.len() > self.config.max_events {
            events.pop_front();
        }
        drop(events);

        // Check rules
        self.check_rules(&event).await
    }

    /// Check all rules against an event.
    async fn check_rules(&self, event: &Event) -> Result<Option<Alert>> {
        let rules = self.rules.read().await;
        let now = Utc::now();

        for rule in rules.values() {
            if !rule.enabled {
                continue;
            }

            // Check cooldown
            if let Some(last) = rule.last_triggered {
                let elapsed = (now - last).num_seconds();
                if elapsed < rule.cooldown_secs as i64 {
                    continue;
                }
            }

            if self.evaluate_condition(&rule.condition, event).await {
                let alert = self.create_alert_from_rule(rule, event).await?;
                return Ok(Some(alert));
            }
        }

        Ok(None)
    }

    /// Evaluate a rule condition.
    async fn evaluate_condition(&self, condition: &RuleCondition, event: &Event) -> bool {
        match condition {
            RuleCondition::Threshold {
                metric,
                operator,
                value,
            } => {
                if let Some(v) = event.data.get(metric).and_then(|v| v.as_f64()) {
                    Self::compare(v, *operator, *value)
                } else {
                    false
                }
            }
            RuleCondition::Pattern { regex, source: _ } => {
                let text = event.data.to_string();
                match regex::Regex::new(regex) {
                    Ok(re) => re.is_match(&text),
                    Err(_) => false,
                }
            }
            RuleCondition::Anomaly {
                metric,
                sensitivity,
            } => {
                if let Some(v) = event.data.get(metric).and_then(|v| v.as_f64()) {
                    let baselines = self.baselines.blocking_read();
                    if let Some(baseline) = baselines.get(metric) {
                        let sigma = (v - baseline.mean).abs() / baseline.std_dev.max(0.001);
                        sigma > (1.0 / sensitivity) * 2.0
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            RuleCondition::Composite {
                operator,
                conditions,
            } => {
                let results: Vec<_> = futures::executor::block_on(async {
                    let mut results = Vec::new();
                    for c in conditions {
                        results.push(Box::pin(self.evaluate_condition(c, event)).await);
                    }
                    results
                });

                match operator {
                    LogicalOp::And => results.iter().all(|&r| r),
                    LogicalOp::Or => results.iter().any(|&r| r),
                    LogicalOp::Not => !results.first().copied().unwrap_or(false),
                }
            }
            _ => false,
        }
    }

    /// Compare values.
    fn compare(left: f64, op: ComparisonOp, right: f64) -> bool {
        match op {
            ComparisonOp::Eq => (left - right).abs() < f64::EPSILON,
            ComparisonOp::Ne => (left - right).abs() >= f64::EPSILON,
            ComparisonOp::Lt => left < right,
            ComparisonOp::Le => left <= right,
            ComparisonOp::Gt => left > right,
            ComparisonOp::Ge => left >= right,
        }
    }

    /// Create an alert from a triggered rule.
    async fn create_alert_from_rule(&self, rule: &MonitoringRule, event: &Event) -> Result<Alert> {
        let context = AlertContext {
            related_events: vec![event.clone()],
            affected_resources: Vec::new(),
            pattern: None,
            deviation: None,
        };

        let suggested_actions = self
            .provider
            .suggest_actions(&Alert {
                id: String::new(),
                title: rule.alert_settings.title_template.clone(),
                description: String::new(),
                severity: rule.alert_settings.severity,
                category: rule.alert_settings.category.clone(),
                status: AlertStatus::Active,
                source: AlertSource {
                    source_type: SourceType::System,
                    identifier: "rule".to_string(),
                    details: HashMap::new(),
                },
                context: context.clone(),
                suggested_actions: Vec::new(),
                created_at: Utc::now(),
                updated_at: Utc::now(),
                resolved_at: None,
                metadata: HashMap::new(),
            })
            .await?;

        let alert = Alert {
            id: Uuid::new_v4().to_string(),
            title: rule.alert_settings.title_template.clone(),
            description: rule.alert_settings.description_template.clone(),
            severity: rule.alert_settings.severity,
            category: rule.alert_settings.category.clone(),
            status: AlertStatus::Active,
            source: AlertSource {
                source_type: SourceType::System,
                identifier: rule.id.clone(),
                details: HashMap::new(),
            },
            context,
            suggested_actions,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            resolved_at: None,
            metadata: HashMap::new(),
        };

        let mut alerts = self.alerts.write().await;
        alerts.insert(alert.id.clone(), alert.clone());

        Ok(alert)
    }

    /// Add a monitoring rule.
    pub async fn add_rule(&self, rule: MonitoringRule) -> Result<String> {
        let id = rule.id.clone();
        let mut rules = self.rules.write().await;
        rules.insert(id.clone(), rule);
        Ok(id)
    }

    /// Get all active alerts.
    pub async fn get_active_alerts(&self) -> Vec<Alert> {
        let alerts = self.alerts.read().await;
        alerts
            .values()
            .filter(|a| a.status == AlertStatus::Active)
            .cloned()
            .collect()
    }

    /// Get alerts by severity.
    pub async fn get_alerts_by_severity(&self, min_severity: Severity) -> Vec<Alert> {
        let alerts = self.alerts.read().await;
        alerts
            .values()
            .filter(|a| a.severity >= min_severity && a.status == AlertStatus::Active)
            .cloned()
            .collect()
    }

    /// Acknowledge an alert.
    pub async fn acknowledge_alert(&self, id: &str) -> Result<()> {
        let mut alerts = self.alerts.write().await;
        if let Some(alert) = alerts.get_mut(id) {
            alert.status = AlertStatus::Acknowledged;
            alert.updated_at = Utc::now();
            Ok(())
        } else {
            Err(GuardianError::AlertCreationFailed(format!(
                "Alert not found: {}",
                id
            )))
        }
    }

    /// Resolve an alert.
    pub async fn resolve_alert(&self, id: &str, false_positive: bool) -> Result<()> {
        let mut alerts = self.alerts.write().await;
        if let Some(alert) = alerts.get_mut(id) {
            alert.status = if false_positive {
                AlertStatus::FalsePositive
            } else {
                AlertStatus::Resolved
            };
            alert.resolved_at = Some(Utc::now());
            alert.updated_at = Utc::now();
            Ok(())
        } else {
            Err(GuardianError::AlertCreationFailed(format!(
                "Alert not found: {}",
                id
            )))
        }
    }

    /// Update baseline for a metric.
    pub async fn update_baseline(&self, metric: &str, value: f64) {
        let mut baselines = self.baselines.write().await;

        let baseline = baselines
            .entry(metric.to_string())
            .or_insert(MetricBaseline {
                metric: metric.to_string(),
                mean: value,
                std_dev: 0.0,
                sample_count: 0,
                updated_at: Utc::now(),
            });

        // Incremental update using Welford's algorithm
        baseline.sample_count += 1;
        let delta = value - baseline.mean;
        baseline.mean += delta / baseline.sample_count as f64;
        let delta2 = value - baseline.mean;
        let variance = if baseline.sample_count > 1 {
            (baseline.std_dev.powi(2) * (baseline.sample_count - 1) as f64 + delta * delta2)
                / baseline.sample_count as f64
        } else {
            0.0
        };
        baseline.std_dev = variance.sqrt();
        baseline.updated_at = Utc::now();
    }

    /// Run proactive analysis on recent events.
    pub async fn analyze(&self) -> Result<Vec<Alert>> {
        let events: Vec<Event> = {
            let events = self.events.read().await;
            events.iter().cloned().collect()
        };

        // Detect anomalies
        let deviations = self.provider.analyze_events(&events).await?;

        // Detect patterns
        let patterns = self.provider.detect_patterns(&events).await?;

        let mut alerts = Vec::new();

        for deviation in deviations {
            if deviation.sigma > 3.0 {
                let context = AlertContext {
                    related_events: Vec::new(),
                    affected_resources: vec![deviation.metric.clone()],
                    pattern: None,
                    deviation: Some(deviation.clone()),
                };

                let severity = self.provider.classify_severity(&context).await?;

                let alert = Alert {
                    id: Uuid::new_v4().to_string(),
                    title: format!("Anomaly detected in {}", deviation.metric),
                    description: format!(
                        "Observed value {} deviates {:.1} sigma from baseline {}",
                        deviation.observed, deviation.sigma, deviation.baseline
                    ),
                    severity,
                    category: AlertCategory::Behavioral,
                    status: AlertStatus::Active,
                    source: AlertSource {
                        source_type: SourceType::System,
                        identifier: "anomaly_detector".to_string(),
                        details: HashMap::new(),
                    },
                    context,
                    suggested_actions: Vec::new(),
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                    resolved_at: None,
                    metadata: HashMap::new(),
                };

                let mut stored = self.alerts.write().await;
                stored.insert(alert.id.clone(), alert.clone());
                alerts.push(alert);
            }
        }

        Ok(alerts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockProvider;

    #[async_trait]
    impl GuardianProvider for MockProvider {
        async fn analyze_events(&self, events: &[Event]) -> Result<Vec<Deviation>> {
            Ok(events
                .iter()
                .filter_map(|e| {
                    e.data
                        .get("cpu")
                        .and_then(|v| v.as_f64())
                        .map(|v| Deviation {
                            baseline: 50.0,
                            observed: v,
                            sigma: (v - 50.0).abs() / 10.0,
                            metric: "cpu".to_string(),
                        })
                })
                .filter(|d| d.sigma > 2.0)
                .collect())
        }

        async fn detect_patterns(&self, _events: &[Event]) -> Result<Vec<Pattern>> {
            Ok(vec![])
        }

        async fn suggest_actions(&self, alert: &Alert) -> Result<Vec<SuggestedAction>> {
            Ok(vec![SuggestedAction {
                description: format!("Investigate {}", alert.title),
                priority: 1,
                automatable: false,
                command: None,
            }])
        }

        async fn classify_severity(&self, context: &AlertContext) -> Result<Severity> {
            if let Some(deviation) = &context.deviation {
                if deviation.sigma > 5.0 {
                    Ok(Severity::High)
                } else if deviation.sigma > 3.0 {
                    Ok(Severity::Medium)
                } else {
                    Ok(Severity::Low)
                }
            } else {
                Ok(Severity::Info)
            }
        }
    }

    #[tokio::test]
    async fn test_record_event() {
        let provider = Arc::new(MockProvider);
        let guardian = Guardian::new(provider, GuardianConfig::default());

        let event = Event {
            id: "e1".to_string(),
            event_type: "metric".to_string(),
            timestamp: Utc::now(),
            data: serde_json::json!({ "cpu": 45.0 }),
            severity: Severity::Info,
        };

        let result = guardian.record_event(event).await.unwrap();
        assert!(result.is_none()); // No alert for normal value
    }

    #[tokio::test]
    async fn test_rule_evaluation() {
        let provider = Arc::new(MockProvider);
        let guardian = Guardian::new(provider, GuardianConfig::default());

        let rule = MonitoringRule {
            id: "r1".to_string(),
            name: "High CPU".to_string(),
            description: "Alert on high CPU".to_string(),
            condition: RuleCondition::Threshold {
                metric: "cpu".to_string(),
                operator: ComparisonOp::Gt,
                value: 90.0,
            },
            alert_settings: AlertSettings {
                severity: Severity::High,
                category: AlertCategory::Performance,
                title_template: "High CPU Usage".to_string(),
                description_template: "CPU usage exceeded 90%".to_string(),
                auto_resolve_secs: None,
            },
            enabled: true,
            cooldown_secs: 60,
            last_triggered: None,
        };

        guardian.add_rule(rule).await.unwrap();

        let event = Event {
            id: "e1".to_string(),
            event_type: "metric".to_string(),
            timestamp: Utc::now(),
            data: serde_json::json!({ "cpu": 95.0 }),
            severity: Severity::Info,
        };

        let result = guardian.record_event(event).await.unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().severity, Severity::High);
    }

    #[tokio::test]
    async fn test_alert_lifecycle() {
        let provider = Arc::new(MockProvider);
        let guardian = Guardian::new(provider, GuardianConfig::default());

        let rule = MonitoringRule {
            id: "r1".to_string(),
            name: "Test".to_string(),
            description: "Test rule".to_string(),
            condition: RuleCondition::Threshold {
                metric: "value".to_string(),
                operator: ComparisonOp::Gt,
                value: 0.0,
            },
            alert_settings: AlertSettings {
                severity: Severity::Medium,
                category: AlertCategory::Custom("Test".to_string()),
                title_template: "Test Alert".to_string(),
                description_template: "Test".to_string(),
                auto_resolve_secs: None,
            },
            enabled: true,
            cooldown_secs: 0,
            last_triggered: None,
        };

        guardian.add_rule(rule).await.unwrap();

        let event = Event {
            id: "e1".to_string(),
            event_type: "test".to_string(),
            timestamp: Utc::now(),
            data: serde_json::json!({ "value": 1.0 }),
            severity: Severity::Info,
        };

        let alert = guardian.record_event(event).await.unwrap().unwrap();

        // Acknowledge
        guardian.acknowledge_alert(&alert.id).await.unwrap();
        let alerts = guardian.get_active_alerts().await;
        assert!(alerts.is_empty()); // Acknowledged alerts are not "active"

        // Resolve
        guardian.resolve_alert(&alert.id, false).await.unwrap();
    }

    #[tokio::test]
    async fn test_baseline_update() {
        let provider = Arc::new(MockProvider);
        let guardian = Guardian::new(provider, GuardianConfig::default());

        for i in 0..10 {
            guardian.update_baseline("metric", 50.0 + (i as f64)).await;
        }

        let baselines = guardian.baselines.read().await;
        let baseline = baselines.get("metric").unwrap();
        assert!(baseline.sample_count == 10);
        assert!(baseline.mean > 50.0);
    }

    #[test]
    fn test_severity_ordering() {
        assert!(Severity::Critical > Severity::High);
        assert!(Severity::High > Severity::Medium);
        assert!(Severity::Medium > Severity::Low);
        assert!(Severity::Low > Severity::Info);
    }

    #[test]
    fn test_comparison_ops() {
        assert!(Guardian::compare(5.0, ComparisonOp::Gt, 3.0));
        assert!(Guardian::compare(3.0, ComparisonOp::Lt, 5.0));
        assert!(Guardian::compare(3.0, ComparisonOp::Eq, 3.0));
        assert!(Guardian::compare(3.0, ComparisonOp::Le, 3.0));
        assert!(Guardian::compare(3.0, ComparisonOp::Ge, 3.0));
    }
}
