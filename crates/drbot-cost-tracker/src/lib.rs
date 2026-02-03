//! Real-time cost tracking, budgets, and alerts.
//!
//! This crate provides:
//! - Usage tracking per model/provider
//! - Budget management
//! - Cost alerts
//! - Spending reports

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;

/// Cost tracking errors.
#[derive(Debug, Error)]
pub enum CostError {
    #[error("Budget exceeded: {0}")]
    BudgetExceeded(String),

    #[error("Invalid budget: {0}")]
    InvalidBudget(String),

    #[error("Tracking failed: {0}")]
    TrackingFailed(String),
}

/// Result type for cost operations.
pub type Result<T> = std::result::Result<T, CostError>;

/// A usage record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageRecord {
    /// Record identifier.
    pub id: String,
    /// Model used.
    pub model: String,
    /// Provider.
    pub provider: String,
    /// Input tokens.
    pub input_tokens: usize,
    /// Output tokens.
    pub output_tokens: usize,
    /// Total cost.
    pub cost: f64,
    /// Request type.
    pub request_type: RequestType,
    /// User/session identifier.
    pub user_id: Option<String>,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
    /// Metadata.
    pub metadata: HashMap<String, String>,
}

/// Request types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RequestType {
    Chat,
    Completion,
    Embedding,
    ImageGeneration,
    AudioTranscription,
    FineTuning,
    Other,
}

/// A budget definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Budget {
    /// Budget identifier.
    pub id: String,
    /// Budget name.
    pub name: String,
    /// Budget limit.
    pub limit: f64,
    /// Budget period.
    pub period: BudgetPeriod,
    /// Current spend.
    pub current_spend: f64,
    /// Alert thresholds (percentages).
    pub alert_thresholds: Vec<f64>,
    /// Alerts sent.
    pub alerts_sent: Vec<f64>,
    /// Scope.
    pub scope: BudgetScope,
    /// Created at.
    pub created_at: DateTime<Utc>,
    /// Period start.
    pub period_start: DateTime<Utc>,
}

/// Budget periods.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BudgetPeriod {
    Daily,
    Weekly,
    Monthly,
    Quarterly,
    Yearly,
    Custom(u64), // seconds
}

/// Budget scope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BudgetScope {
    Global,
    Provider(String),
    Model(String),
    User(String),
    Custom(String),
}

/// A cost alert.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostAlert {
    /// Alert identifier.
    pub id: String,
    /// Alert type.
    pub alert_type: AlertType,
    /// Budget ID.
    pub budget_id: String,
    /// Current spend.
    pub current_spend: f64,
    /// Limit.
    pub limit: f64,
    /// Percentage used.
    pub percentage: f64,
    /// Message.
    pub message: String,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
}

/// Alert types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlertType {
    ThresholdReached,
    BudgetExceeded,
    UnusualSpending,
    PeriodReset,
}

/// Spending report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpendingReport {
    /// Report period start.
    pub period_start: DateTime<Utc>,
    /// Report period end.
    pub period_end: DateTime<Utc>,
    /// Total spend.
    pub total_spend: f64,
    /// Spend by provider.
    pub by_provider: HashMap<String, f64>,
    /// Spend by model.
    pub by_model: HashMap<String, f64>,
    /// Spend by user.
    pub by_user: HashMap<String, f64>,
    /// Request count.
    pub request_count: usize,
    /// Token count.
    pub total_tokens: usize,
    /// Average cost per request.
    pub avg_cost_per_request: f64,
    /// Top models by cost.
    pub top_models: Vec<(String, f64)>,
}

/// The cost tracker.
pub struct CostTracker {
    /// Usage records.
    records: Arc<RwLock<Vec<UsageRecord>>>,
    /// Budgets.
    budgets: Arc<RwLock<HashMap<String, Budget>>>,
    /// Alert channel.
    alert_tx: broadcast::Sender<CostAlert>,
    /// Model pricing.
    pricing: Arc<RwLock<HashMap<String, ModelPricing>>>,
}

/// Model pricing information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelPricing {
    /// Model ID.
    pub model: String,
    /// Cost per 1K input tokens.
    pub input_cost_per_1k: f64,
    /// Cost per 1K output tokens.
    pub output_cost_per_1k: f64,
}

impl CostTracker {
    /// Create a new cost tracker.
    pub fn new() -> Self {
        let (alert_tx, _) = broadcast::channel(100);
        Self {
            records: Arc::new(RwLock::new(Vec::new())),
            budgets: Arc::new(RwLock::new(HashMap::new())),
            alert_tx,
            pricing: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Subscribe to alerts.
    pub fn subscribe_alerts(&self) -> broadcast::Receiver<CostAlert> {
        self.alert_tx.subscribe()
    }

    /// Set model pricing.
    pub async fn set_pricing(&self, pricing: ModelPricing) {
        let mut prices = self.pricing.write().await;
        prices.insert(pricing.model.clone(), pricing);
    }

    /// Calculate cost for usage.
    pub async fn calculate_cost(
        &self,
        model: &str,
        input_tokens: usize,
        output_tokens: usize,
    ) -> f64 {
        let pricing = self.pricing.read().await;
        if let Some(price) = pricing.get(model) {
            (input_tokens as f64 / 1000.0) * price.input_cost_per_1k
                + (output_tokens as f64 / 1000.0) * price.output_cost_per_1k
        } else {
            // Default pricing
            (input_tokens as f64 / 1000.0) * 0.01 + (output_tokens as f64 / 1000.0) * 0.03
        }
    }

    /// Record usage.
    pub async fn record(&self, record: UsageRecord) -> Result<()> {
        // Check budgets
        self.check_budgets(&record).await?;

        // Store record
        let mut records = self.records.write().await;
        records.push(record.clone());

        // Keep last 100000 records
        if records.len() > 100000 {
            records.drain(0..10000);
        }

        // Update budget spend
        self.update_budget_spend(&record).await;

        Ok(())
    }

    /// Check if usage would exceed any budget.
    async fn check_budgets(&self, record: &UsageRecord) -> Result<()> {
        let budgets = self.budgets.read().await;

        for budget in budgets.values() {
            if self.budget_applies(budget, record) {
                let new_spend = budget.current_spend + record.cost;
                if new_spend > budget.limit {
                    return Err(CostError::BudgetExceeded(format!(
                        "Budget '{}' would be exceeded: ${:.2} + ${:.2} > ${:.2}",
                        budget.name, budget.current_spend, record.cost, budget.limit
                    )));
                }
            }
        }

        Ok(())
    }

    /// Check if budget applies to record.
    fn budget_applies(&self, budget: &Budget, record: &UsageRecord) -> bool {
        match &budget.scope {
            BudgetScope::Global => true,
            BudgetScope::Provider(p) => &record.provider == p,
            BudgetScope::Model(m) => &record.model == m,
            BudgetScope::User(u) => record.user_id.as_ref() == Some(u),
            BudgetScope::Custom(_) => true,
        }
    }

    /// Update budget spending.
    async fn update_budget_spend(&self, record: &UsageRecord) {
        let mut budgets = self.budgets.write().await;

        for budget in budgets.values_mut() {
            if self.budget_applies(budget, record) {
                budget.current_spend += record.cost;

                // Check alert thresholds
                let percentage = (budget.current_spend / budget.limit) * 100.0;
                for &threshold in &budget.alert_thresholds {
                    if percentage >= threshold && !budget.alerts_sent.contains(&threshold) {
                        budget.alerts_sent.push(threshold);

                        let alert = CostAlert {
                            id: Uuid::new_v4().to_string(),
                            alert_type: if percentage >= 100.0 {
                                AlertType::BudgetExceeded
                            } else {
                                AlertType::ThresholdReached
                            },
                            budget_id: budget.id.clone(),
                            current_spend: budget.current_spend,
                            limit: budget.limit,
                            percentage,
                            message: format!(
                                "Budget '{}' at {:.1}% (${:.2}/${:.2})",
                                budget.name, percentage, budget.current_spend, budget.limit
                            ),
                            timestamp: Utc::now(),
                        };

                        let _ = self.alert_tx.send(alert);
                    }
                }
            }
        }
    }

    /// Create a budget.
    pub async fn create_budget(
        &self,
        name: &str,
        limit: f64,
        period: BudgetPeriod,
        scope: BudgetScope,
    ) -> Result<String> {
        if limit <= 0.0 {
            return Err(CostError::InvalidBudget(
                "Limit must be positive".to_string(),
            ));
        }

        let budget = Budget {
            id: Uuid::new_v4().to_string(),
            name: name.to_string(),
            limit,
            period,
            current_spend: 0.0,
            alert_thresholds: vec![50.0, 75.0, 90.0, 100.0],
            alerts_sent: Vec::new(),
            scope,
            created_at: Utc::now(),
            period_start: Utc::now(),
        };

        let id = budget.id.clone();
        let mut budgets = self.budgets.write().await;
        budgets.insert(id.clone(), budget);

        Ok(id)
    }

    /// Get budget status.
    pub async fn get_budget(&self, id: &str) -> Option<Budget> {
        let budgets = self.budgets.read().await;
        budgets.get(id).cloned()
    }

    /// Reset budget for new period.
    pub async fn reset_budget(&self, id: &str) {
        let mut budgets = self.budgets.write().await;
        if let Some(budget) = budgets.get_mut(id) {
            budget.current_spend = 0.0;
            budget.alerts_sent.clear();
            budget.period_start = Utc::now();

            let alert = CostAlert {
                id: Uuid::new_v4().to_string(),
                alert_type: AlertType::PeriodReset,
                budget_id: budget.id.clone(),
                current_spend: 0.0,
                limit: budget.limit,
                percentage: 0.0,
                message: format!("Budget '{}' reset for new period", budget.name),
                timestamp: Utc::now(),
            };

            let _ = self.alert_tx.send(alert);
        }
    }

    /// Generate spending report.
    pub async fn generate_report(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> SpendingReport {
        let records = self.records.read().await;

        let filtered: Vec<_> = records
            .iter()
            .filter(|r| r.timestamp >= start && r.timestamp <= end)
            .collect();

        let mut by_provider: HashMap<String, f64> = HashMap::new();
        let mut by_model: HashMap<String, f64> = HashMap::new();
        let mut by_user: HashMap<String, f64> = HashMap::new();
        let mut total_spend = 0.0;
        let mut total_tokens = 0;

        for record in &filtered {
            total_spend += record.cost;
            total_tokens += record.input_tokens + record.output_tokens;

            *by_provider.entry(record.provider.clone()).or_insert(0.0) += record.cost;
            *by_model.entry(record.model.clone()).or_insert(0.0) += record.cost;
            if let Some(user) = &record.user_id {
                *by_user.entry(user.clone()).or_insert(0.0) += record.cost;
            }
        }

        let mut top_models: Vec<_> = by_model.iter().map(|(k, v)| (k.clone(), *v)).collect();
        top_models.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        top_models.truncate(10);

        SpendingReport {
            period_start: start,
            period_end: end,
            total_spend,
            by_provider,
            by_model,
            by_user,
            request_count: filtered.len(),
            total_tokens,
            avg_cost_per_request: if filtered.is_empty() {
                0.0
            } else {
                total_spend / filtered.len() as f64
            },
            top_models,
        }
    }

    /// Get total spend for current day.
    pub async fn today_spend(&self) -> f64 {
        let now = Utc::now();
        let start = now.date_naive().and_hms_opt(0, 0, 0).unwrap();
        let start = DateTime::<Utc>::from_naive_utc_and_offset(start, Utc);

        let records = self.records.read().await;
        records
            .iter()
            .filter(|r| r.timestamp >= start)
            .map(|r| r.cost)
            .sum()
    }

    /// Get recent usage records.
    pub async fn recent_records(&self, limit: usize) -> Vec<UsageRecord> {
        let records = self.records.read().await;
        records.iter().rev().take(limit).cloned().collect()
    }
}

impl Default for CostTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_record(cost: f64) -> UsageRecord {
        UsageRecord {
            id: Uuid::new_v4().to_string(),
            model: "gpt-4".to_string(),
            provider: "openai".to_string(),
            input_tokens: 100,
            output_tokens: 50,
            cost,
            request_type: RequestType::Chat,
            user_id: Some("user1".to_string()),
            timestamp: Utc::now(),
            metadata: HashMap::new(),
        }
    }

    #[tokio::test]
    async fn test_record_usage() {
        let tracker = CostTracker::new();

        let record = create_test_record(0.05);
        tracker.record(record).await.unwrap();

        let recent = tracker.recent_records(10).await;
        assert_eq!(recent.len(), 1);
    }

    #[tokio::test]
    async fn test_budget_creation() {
        let tracker = CostTracker::new();

        let id = tracker
            .create_budget("Monthly", 100.0, BudgetPeriod::Monthly, BudgetScope::Global)
            .await
            .unwrap();

        let budget = tracker.get_budget(&id).await.unwrap();
        assert_eq!(budget.limit, 100.0);
    }

    #[tokio::test]
    async fn test_budget_exceeded() {
        let tracker = CostTracker::new();

        tracker
            .create_budget("Small", 0.01, BudgetPeriod::Daily, BudgetScope::Global)
            .await
            .unwrap();

        let record = create_test_record(0.05);
        let result = tracker.record(record).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_calculate_cost() {
        let tracker = CostTracker::new();

        tracker
            .set_pricing(ModelPricing {
                model: "gpt-4".to_string(),
                input_cost_per_1k: 0.03,
                output_cost_per_1k: 0.06,
            })
            .await;

        let cost = tracker.calculate_cost("gpt-4", 1000, 500).await;
        assert!((cost - 0.06).abs() < 0.001);
    }

    #[tokio::test]
    async fn test_generate_report() {
        let tracker = CostTracker::new();

        tracker.record(create_test_record(0.05)).await.unwrap();
        tracker.record(create_test_record(0.03)).await.unwrap();

        let report = tracker
            .generate_report(
                Utc::now() - Duration::hours(1),
                Utc::now() + Duration::hours(1),
            )
            .await;

        assert_eq!(report.request_count, 2);
        assert!((report.total_spend - 0.08).abs() < 0.001);
    }

    #[tokio::test]
    async fn test_today_spend() {
        let tracker = CostTracker::new();

        tracker.record(create_test_record(0.05)).await.unwrap();

        let spend = tracker.today_spend().await;
        assert!((spend - 0.05).abs() < 0.001);
    }
}
