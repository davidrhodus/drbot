//! Cost tracking and budget management for drbot.
//!
//! Provides real-time cost tracking, budget limits, and usage reports.
//!
//! # Features
//!
//! - Real-time token usage tracking
//! - Budget limits and alerts
//! - Cost optimization recommendations
//! - Usage reports and analytics

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;

/// Cost result type.
pub type Result<T> = std::result::Result<T, CostError>;

/// Cost errors.
#[derive(Debug, thiserror::Error)]
pub enum CostError {
    #[error("Budget exceeded: {0}")]
    BudgetExceeded(String),
    #[error("Rate limit reached")]
    RateLimitReached,
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),
}

/// Token usage for a request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    /// Input tokens.
    pub input_tokens: u64,
    /// Output tokens.
    pub output_tokens: u64,
    /// Total tokens.
    pub total_tokens: u64,
}

impl TokenUsage {
    /// Create new token usage.
    pub fn new(input: u64, output: u64) -> Self {
        Self {
            input_tokens: input,
            output_tokens: output,
            total_tokens: input + output,
        }
    }
}

/// Cost record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostRecord {
    /// Record ID.
    pub id: Uuid,
    /// Session ID.
    pub session_id: String,
    /// Provider name.
    pub provider: String,
    /// Model name.
    pub model: String,
    /// Token usage.
    pub tokens: TokenUsage,
    /// Cost in USD (cents).
    pub cost_cents: u64,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
}

impl CostRecord {
    /// Create a new cost record.
    pub fn new(
        session_id: &str,
        provider: &str,
        model: &str,
        tokens: TokenUsage,
        cost_cents: u64,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            session_id: session_id.to_string(),
            provider: provider.to_string(),
            model: model.to_string(),
            tokens,
            cost_cents,
            timestamp: Utc::now(),
        }
    }
}

/// Model pricing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelPricing {
    /// Provider name.
    pub provider: String,
    /// Model name.
    pub model: String,
    /// Input price per 1M tokens (in cents).
    pub input_price_per_million: u64,
    /// Output price per 1M tokens (in cents).
    pub output_price_per_million: u64,
}

impl ModelPricing {
    /// Calculate cost for token usage.
    pub fn calculate_cost(&self, tokens: &TokenUsage) -> u64 {
        let input_cost = (tokens.input_tokens * self.input_price_per_million) / 1_000_000;
        let output_cost = (tokens.output_tokens * self.output_price_per_million) / 1_000_000;
        let total = input_cost + output_cost;
        // Minimum 1 cent if any tokens were used
        if total == 0 && tokens.total_tokens > 0 {
            1
        } else {
            total
        }
    }
}

/// Budget configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetConfig {
    /// Daily budget in cents.
    pub daily_limit_cents: Option<u64>,
    /// Weekly budget in cents.
    pub weekly_limit_cents: Option<u64>,
    /// Monthly budget in cents.
    pub monthly_limit_cents: Option<u64>,
    /// Alert threshold (percentage).
    pub alert_threshold_percent: u8,
    /// Block on exceed.
    pub block_on_exceed: bool,
}

impl Default for BudgetConfig {
    fn default() -> Self {
        Self {
            daily_limit_cents: None,
            weekly_limit_cents: None,
            monthly_limit_cents: Some(5000), // $50 default
            alert_threshold_percent: 80,
            block_on_exceed: false,
        }
    }
}

/// Budget alert.
#[derive(Debug, Clone)]
pub enum BudgetAlert {
    /// Approaching limit.
    Approaching { period: String, percent: u8 },
    /// Limit exceeded.
    Exceeded {
        period: String,
        amount: u64,
        limit: u64,
    },
}

/// Cost summary.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CostSummary {
    /// Total cost in cents.
    pub total_cost_cents: u64,
    /// Total input tokens.
    pub total_input_tokens: u64,
    /// Total output tokens.
    pub total_output_tokens: u64,
    /// Request count.
    pub request_count: u64,
    /// Cost by provider.
    pub by_provider: HashMap<String, u64>,
    /// Cost by model.
    pub by_model: HashMap<String, u64>,
    /// Period start.
    pub period_start: DateTime<Utc>,
    /// Period end.
    pub period_end: DateTime<Utc>,
}

/// Cost tracker.
pub struct CostTracker {
    records: Arc<RwLock<Vec<CostRecord>>>,
    pricing: Arc<RwLock<HashMap<String, ModelPricing>>>,
    budget: BudgetConfig,
    alert_sender: broadcast::Sender<BudgetAlert>,
}

impl CostTracker {
    /// Create a new cost tracker.
    pub fn new(budget: BudgetConfig) -> Self {
        let (sender, _) = broadcast::channel(32);

        Self {
            records: Arc::new(RwLock::new(Vec::new())),
            pricing: Arc::new(RwLock::new(Self::default_pricing())),
            budget,
            alert_sender: sender,
        }
    }

    fn default_pricing() -> HashMap<String, ModelPricing> {
        let mut pricing = HashMap::new();

        // Claude models
        pricing.insert(
            "anthropic/claude-3-opus".to_string(),
            ModelPricing {
                provider: "anthropic".to_string(),
                model: "claude-3-opus".to_string(),
                input_price_per_million: 1500,  // $15
                output_price_per_million: 7500, // $75
            },
        );
        pricing.insert(
            "anthropic/claude-3-sonnet".to_string(),
            ModelPricing {
                provider: "anthropic".to_string(),
                model: "claude-3-sonnet".to_string(),
                input_price_per_million: 300,   // $3
                output_price_per_million: 1500, // $15
            },
        );
        pricing.insert(
            "anthropic/claude-3-haiku".to_string(),
            ModelPricing {
                provider: "anthropic".to_string(),
                model: "claude-3-haiku".to_string(),
                input_price_per_million: 25,   // $0.25
                output_price_per_million: 125, // $1.25
            },
        );

        // OpenAI models
        pricing.insert(
            "openai/gpt-4".to_string(),
            ModelPricing {
                provider: "openai".to_string(),
                model: "gpt-4".to_string(),
                input_price_per_million: 3000,  // $30
                output_price_per_million: 6000, // $60
            },
        );
        pricing.insert(
            "openai/gpt-4-turbo".to_string(),
            ModelPricing {
                provider: "openai".to_string(),
                model: "gpt-4-turbo".to_string(),
                input_price_per_million: 1000,  // $10
                output_price_per_million: 3000, // $30
            },
        );
        pricing.insert(
            "openai/gpt-3.5-turbo".to_string(),
            ModelPricing {
                provider: "openai".to_string(),
                model: "gpt-3.5-turbo".to_string(),
                input_price_per_million: 50,   // $0.50
                output_price_per_million: 150, // $1.50
            },
        );

        pricing
    }

    /// Record a cost.
    pub async fn record(&self, record: CostRecord) -> Result<()> {
        // Check budget
        self.check_budget().await?;

        {
            let mut records = self.records.write().await;
            records.push(record);
        }

        // Check for alerts (must be after releasing write lock)
        self.check_alerts().await;

        Ok(())
    }

    /// Record usage and calculate cost.
    pub async fn record_usage(
        &self,
        session_id: &str,
        provider: &str,
        model: &str,
        tokens: TokenUsage,
    ) -> Result<CostRecord> {
        let cost_cents = self.calculate_cost(provider, model, &tokens).await;

        let record = CostRecord::new(session_id, provider, model, tokens, cost_cents);
        self.record(record.clone()).await?;

        Ok(record)
    }

    /// Calculate cost for tokens.
    pub async fn calculate_cost(&self, provider: &str, model: &str, tokens: &TokenUsage) -> u64 {
        let pricing = self.pricing.read().await;
        let key = format!("{}/{}", provider, model);

        if let Some(price) = pricing.get(&key) {
            price.calculate_cost(tokens)
        } else {
            // Default fallback pricing (minimum 1 cent if any tokens)
            let cost = (tokens.total_tokens * 100) / 1_000_000;
            if cost == 0 && tokens.total_tokens > 0 {
                1
            } else {
                cost
            }
        }
    }

    async fn check_budget(&self) -> Result<()> {
        if !self.budget.block_on_exceed {
            return Ok(());
        }

        let summary = self.summary(Period::Monthly).await;

        if let Some(limit) = self.budget.monthly_limit_cents {
            if summary.total_cost_cents >= limit {
                return Err(CostError::BudgetExceeded(
                    "Monthly budget exceeded".to_string(),
                ));
            }
        }

        if let Some(limit) = self.budget.weekly_limit_cents {
            let weekly = self.summary(Period::Weekly).await;
            if weekly.total_cost_cents >= limit {
                return Err(CostError::BudgetExceeded(
                    "Weekly budget exceeded".to_string(),
                ));
            }
        }

        if let Some(limit) = self.budget.daily_limit_cents {
            let daily = self.summary(Period::Daily).await;
            if daily.total_cost_cents >= limit {
                return Err(CostError::BudgetExceeded(
                    "Daily budget exceeded".to_string(),
                ));
            }
        }

        Ok(())
    }

    async fn check_alerts(&self) {
        let summary = self.summary(Period::Monthly).await;

        if let Some(limit) = self.budget.monthly_limit_cents {
            let percent = ((summary.total_cost_cents * 100) / limit) as u8;

            if percent >= self.budget.alert_threshold_percent && percent < 100 {
                let _ = self.alert_sender.send(BudgetAlert::Approaching {
                    period: "monthly".to_string(),
                    percent,
                });
            } else if percent >= 100 {
                let _ = self.alert_sender.send(BudgetAlert::Exceeded {
                    period: "monthly".to_string(),
                    amount: summary.total_cost_cents,
                    limit,
                });
            }
        }
    }

    /// Get cost summary.
    pub async fn summary(&self, period: Period) -> CostSummary {
        let records = self.records.read().await;
        let now = Utc::now();

        let period_start = match period {
            Period::Daily => now - Duration::days(1),
            Period::Weekly => now - Duration::weeks(1),
            Period::Monthly => now - Duration::days(30),
            Period::Yearly => now - Duration::days(365),
        };

        let mut summary = CostSummary {
            period_start,
            period_end: now,
            ..Default::default()
        };

        for record in records.iter() {
            if record.timestamp < period_start {
                continue;
            }

            summary.total_cost_cents += record.cost_cents;
            summary.total_input_tokens += record.tokens.input_tokens;
            summary.total_output_tokens += record.tokens.output_tokens;
            summary.request_count += 1;

            *summary
                .by_provider
                .entry(record.provider.clone())
                .or_insert(0) += record.cost_cents;
            *summary.by_model.entry(record.model.clone()).or_insert(0) += record.cost_cents;
        }

        summary
    }

    /// Subscribe to budget alerts.
    pub fn subscribe_alerts(&self) -> broadcast::Receiver<BudgetAlert> {
        self.alert_sender.subscribe()
    }

    /// Get estimated cost for tokens.
    pub async fn estimate(&self, provider: &str, model: &str, estimated_tokens: u64) -> u64 {
        let tokens = TokenUsage::new(estimated_tokens / 2, estimated_tokens / 2);
        self.calculate_cost(provider, model, &tokens).await
    }

    /// Get current spending status.
    pub async fn spending_status(&self) -> SpendingStatus {
        let daily = self.summary(Period::Daily).await;
        let weekly = self.summary(Period::Weekly).await;
        let monthly = self.summary(Period::Monthly).await;

        SpendingStatus {
            daily_cost_cents: daily.total_cost_cents,
            daily_limit_cents: self.budget.daily_limit_cents,
            weekly_cost_cents: weekly.total_cost_cents,
            weekly_limit_cents: self.budget.weekly_limit_cents,
            monthly_cost_cents: monthly.total_cost_cents,
            monthly_limit_cents: self.budget.monthly_limit_cents,
            request_count_today: daily.request_count,
            request_count_month: monthly.request_count,
        }
    }

    /// Format cost as string.
    pub fn format_cost(cents: u64) -> String {
        let dollars = cents / 100;
        let remaining_cents = cents % 100;
        format!("${}.{:02}", dollars, remaining_cents)
    }
}

/// Period for summaries.
#[derive(Debug, Clone, Copy)]
pub enum Period {
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

/// Spending status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpendingStatus {
    /// Daily spending.
    pub daily_cost_cents: u64,
    /// Daily limit.
    pub daily_limit_cents: Option<u64>,
    /// Weekly spending.
    pub weekly_cost_cents: u64,
    /// Weekly limit.
    pub weekly_limit_cents: Option<u64>,
    /// Monthly spending.
    pub monthly_cost_cents: u64,
    /// Monthly limit.
    pub monthly_limit_cents: Option<u64>,
    /// Requests today.
    pub request_count_today: u64,
    /// Requests this month.
    pub request_count_month: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_cost_tracker() {
        let tracker = CostTracker::new(BudgetConfig::default());

        let tokens = TokenUsage::new(1000, 500);
        let record = tracker
            .record_usage("session1", "anthropic", "claude-3-haiku", tokens)
            .await
            .unwrap();

        assert!(record.cost_cents > 0);

        let summary = tracker.summary(Period::Daily).await;
        assert_eq!(summary.request_count, 1);
    }

    #[test]
    fn test_model_pricing() {
        let pricing = ModelPricing {
            provider: "test".to_string(),
            model: "test".to_string(),
            input_price_per_million: 100,
            output_price_per_million: 200,
        };

        let tokens = TokenUsage::new(1_000_000, 1_000_000);
        let cost = pricing.calculate_cost(&tokens);

        assert_eq!(cost, 300);
    }

    #[test]
    fn test_format_cost() {
        assert_eq!(CostTracker::format_cost(123), "$1.23");
        assert_eq!(CostTracker::format_cost(5), "$0.05");
        assert_eq!(CostTracker::format_cost(10000), "$100.00");
    }
}
