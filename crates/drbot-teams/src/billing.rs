//! Usage tracking and billing.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Usage record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageRecord {
    /// Record ID.
    pub id: Uuid,
    /// Organization ID.
    pub org_id: Uuid,
    /// User ID.
    pub user_id: String,
    /// Usage type.
    pub usage_type: UsageType,
    /// Quantity.
    pub quantity: u64,
    /// Unit.
    pub unit: String,
    /// Cost (in cents).
    pub cost_cents: Option<u64>,
    /// Metadata.
    pub metadata: HashMap<String, String>,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
}

impl UsageRecord {
    /// Create a new usage record.
    pub fn new(org_id: Uuid, user_id: &str, usage_type: UsageType, quantity: u64) -> Self {
        Self {
            id: Uuid::new_v4(),
            org_id,
            user_id: user_id.to_string(),
            usage_type,
            quantity,
            unit: usage_type.default_unit().to_string(),
            cost_cents: None,
            metadata: HashMap::new(),
            timestamp: Utc::now(),
        }
    }

    /// Set cost.
    pub fn with_cost(mut self, cents: u64) -> Self {
        self.cost_cents = Some(cents);
        self
    }

    /// Add metadata.
    pub fn with_metadata(mut self, key: &str, value: &str) -> Self {
        self.metadata.insert(key.to_string(), value.to_string());
        self
    }
}

/// Usage types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UsageType {
    /// AI tokens (input).
    TokensInput,
    /// AI tokens (output).
    TokensOutput,
    /// API requests.
    ApiRequests,
    /// Storage (bytes).
    Storage,
    /// Messages.
    Messages,
    /// Voice minutes.
    VoiceMinutes,
    /// Image processing.
    Images,
}

impl UsageType {
    /// Get default unit for this type.
    pub fn default_unit(&self) -> &str {
        match self {
            UsageType::TokensInput | UsageType::TokensOutput => "tokens",
            UsageType::ApiRequests => "requests",
            UsageType::Storage => "bytes",
            UsageType::Messages => "messages",
            UsageType::VoiceMinutes => "minutes",
            UsageType::Images => "images",
        }
    }
}

/// Billing period.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BillingPeriod {
    /// Daily.
    Daily,
    /// Weekly.
    Weekly,
    /// Monthly.
    Monthly,
    /// Yearly.
    Yearly,
}

impl BillingPeriod {
    /// Get duration for this period.
    pub fn duration(&self) -> Duration {
        match self {
            BillingPeriod::Daily => Duration::days(1),
            BillingPeriod::Weekly => Duration::weeks(1),
            BillingPeriod::Monthly => Duration::days(30),
            BillingPeriod::Yearly => Duration::days(365),
        }
    }
}

/// Usage summary.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UsageSummary {
    /// Organization ID.
    pub org_id: Uuid,
    /// Period start.
    pub period_start: DateTime<Utc>,
    /// Period end.
    pub period_end: DateTime<Utc>,
    /// Total records.
    pub total_records: usize,
    /// Total cost in cents.
    pub total_cost_cents: u64,
    /// Usage by type.
    pub by_type: HashMap<UsageType, TypeUsage>,
    /// Usage by user.
    pub by_user: HashMap<String, u64>,
}

/// Usage for a specific type.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TypeUsage {
    /// Total quantity.
    pub quantity: u64,
    /// Unit.
    pub unit: String,
    /// Cost in cents.
    pub cost_cents: u64,
}

/// Usage tracker.
pub struct UsageTracker {
    records: Arc<RwLock<Vec<UsageRecord>>>,
    quotas: Arc<RwLock<HashMap<Uuid, HashMap<UsageType, u64>>>>,
}

impl UsageTracker {
    /// Create a new usage tracker.
    pub fn new() -> Self {
        Self {
            records: Arc::new(RwLock::new(Vec::new())),
            quotas: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Record usage.
    pub async fn record(&self, record: UsageRecord) {
        let mut records = self.records.write().await;
        records.push(record);
    }

    /// Record token usage.
    pub async fn record_tokens(
        &self,
        org_id: Uuid,
        user_id: &str,
        input_tokens: u64,
        output_tokens: u64,
    ) {
        if input_tokens > 0 {
            let record = UsageRecord::new(org_id, user_id, UsageType::TokensInput, input_tokens);
            self.record(record).await;
        }
        if output_tokens > 0 {
            let record = UsageRecord::new(org_id, user_id, UsageType::TokensOutput, output_tokens);
            self.record(record).await;
        }
    }

    /// Get usage summary.
    pub async fn get_summary(&self, org_id: Uuid, period: BillingPeriod) -> UsageSummary {
        let records = self.records.read().await;
        let now = Utc::now();
        let period_start = now - period.duration();

        let mut summary = UsageSummary {
            org_id,
            period_start,
            period_end: now,
            ..Default::default()
        };

        for record in records.iter() {
            if record.org_id != org_id || record.timestamp < period_start {
                continue;
            }

            summary.total_records += 1;
            if let Some(cost) = record.cost_cents {
                summary.total_cost_cents += cost;
            }

            let type_usage = summary.by_type.entry(record.usage_type).or_default();
            type_usage.quantity += record.quantity;
            type_usage.unit = record.unit.clone();
            if let Some(cost) = record.cost_cents {
                type_usage.cost_cents += cost;
            }

            *summary.by_user.entry(record.user_id.clone()).or_insert(0) += record.quantity;
        }

        summary
    }

    /// Set quota for an organization.
    pub async fn set_quota(&self, org_id: Uuid, usage_type: UsageType, limit: u64) {
        let mut quotas = self.quotas.write().await;
        quotas.entry(org_id).or_default().insert(usage_type, limit);
    }

    /// Check if quota is exceeded.
    pub async fn check_quota(
        &self,
        org_id: Uuid,
        usage_type: UsageType,
        period: BillingPeriod,
    ) -> (bool, u64, u64) {
        let quotas = self.quotas.read().await;
        let quota = quotas
            .get(&org_id)
            .and_then(|q| q.get(&usage_type))
            .copied()
            .unwrap_or(u64::MAX);

        let summary = self.get_summary(org_id, period).await;
        let used = summary
            .by_type
            .get(&usage_type)
            .map(|t| t.quantity)
            .unwrap_or(0);

        (used >= quota, used, quota)
    }

    /// Get records for an organization.
    pub async fn get_records(
        &self,
        org_id: Uuid,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Vec<UsageRecord> {
        let records = self.records.read().await;
        records
            .iter()
            .filter(|r| r.org_id == org_id && r.timestamp >= from && r.timestamp <= to)
            .cloned()
            .collect()
    }

    /// Clear old records.
    pub async fn cleanup(&self, before: DateTime<Utc>) {
        let mut records = self.records.write().await;
        records.retain(|r| r.timestamp >= before);
    }
}

impl Default for UsageTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_usage_tracking() {
        let tracker = UsageTracker::new();
        let org_id = Uuid::new_v4();

        tracker.record_tokens(org_id, "user1", 100, 50).await;
        tracker.record_tokens(org_id, "user2", 200, 100).await;

        let summary = tracker.get_summary(org_id, BillingPeriod::Daily).await;
        assert_eq!(summary.total_records, 4);
        assert_eq!(
            summary
                .by_type
                .get(&UsageType::TokensInput)
                .unwrap()
                .quantity,
            300
        );
        assert_eq!(
            summary
                .by_type
                .get(&UsageType::TokensOutput)
                .unwrap()
                .quantity,
            150
        );
    }

    #[tokio::test]
    async fn test_quota() {
        let tracker = UsageTracker::new();
        let org_id = Uuid::new_v4();

        tracker
            .set_quota(org_id, UsageType::TokensInput, 1000)
            .await;

        // Record 500 tokens
        tracker.record_tokens(org_id, "user1", 500, 0).await;

        let (exceeded, used, quota) = tracker
            .check_quota(org_id, UsageType::TokensInput, BillingPeriod::Daily)
            .await;

        assert!(!exceeded);
        assert_eq!(used, 500);
        assert_eq!(quota, 1000);

        // Record 600 more
        tracker.record_tokens(org_id, "user1", 600, 0).await;

        let (exceeded, used, _) = tracker
            .check_quota(org_id, UsageType::TokensInput, BillingPeriod::Daily)
            .await;

        assert!(exceeded);
        assert_eq!(used, 1100);
    }
}
