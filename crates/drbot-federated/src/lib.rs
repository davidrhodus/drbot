//! Federated learning for drbot.
//!
//! Privacy-preserving model improvement.
//!
//! # Features
//!
//! - Local learning aggregation
//! - Privacy-preserving updates
//! - Model synchronization
//! - Gradient aggregation

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Federated learning result type.
pub type Result<T> = std::result::Result<T, FederatedError>;

/// Federated learning errors.
#[derive(Debug, thiserror::Error)]
pub enum FederatedError {
    #[error("Aggregation failed: {0}")]
    AggregationFailed(String),
    #[error("Invalid update: {0}")]
    InvalidUpdate(String),
    #[error("No participants")]
    NoParticipants,
    #[error("Sync failed: {0}")]
    SyncFailed(String),
}

/// A federated participant.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Participant {
    /// Participant ID.
    pub id: Uuid,
    /// Participant name.
    pub name: String,
    /// Last update.
    pub last_update: Option<DateTime<Utc>>,
    /// Update count.
    pub update_count: u64,
    /// Data size (approximate).
    pub data_size: usize,
    /// Active.
    pub active: bool,
    /// Trust score.
    pub trust_score: f32,
}

impl Participant {
    /// Create a new participant.
    pub fn new(name: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.to_string(),
            last_update: None,
            update_count: 0,
            data_size: 0,
            active: true,
            trust_score: 1.0,
        }
    }
}

/// A local model update.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalUpdate {
    /// Update ID.
    pub id: Uuid,
    /// Participant ID.
    pub participant_id: Uuid,
    /// Update round.
    pub round: u64,
    /// Gradients (simplified as float vectors).
    pub gradients: Vec<f32>,
    /// Sample count.
    pub sample_count: usize,
    /// Loss value.
    pub loss: f32,
    /// Metrics.
    pub metrics: UpdateMetrics,
    /// Created at.
    pub created_at: DateTime<Utc>,
    /// Validated.
    pub validated: bool,
}

impl LocalUpdate {
    /// Create a new local update.
    pub fn new(participant_id: Uuid, round: u64, gradients: Vec<f32>, sample_count: usize) -> Self {
        Self {
            id: Uuid::new_v4(),
            participant_id,
            round,
            gradients,
            sample_count,
            loss: 0.0,
            metrics: UpdateMetrics::default(),
            created_at: Utc::now(),
            validated: false,
        }
    }

    /// Set loss value.
    pub fn with_loss(mut self, loss: f32) -> Self {
        self.loss = loss;
        self
    }
}

/// Update metrics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateMetrics {
    /// Training accuracy.
    pub accuracy: f32,
    /// Validation accuracy.
    pub val_accuracy: f32,
    /// Training time in ms.
    pub train_time_ms: u64,
}

/// Aggregated model update.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregatedUpdate {
    /// Aggregation ID.
    pub id: Uuid,
    /// Round.
    pub round: u64,
    /// Aggregated gradients.
    pub gradients: Vec<f32>,
    /// Total samples.
    pub total_samples: usize,
    /// Participant count.
    pub participant_count: usize,
    /// Average loss.
    pub avg_loss: f32,
    /// Aggregated at.
    pub aggregated_at: DateTime<Utc>,
}

/// Federated learning round.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederatedRound {
    /// Round number.
    pub round: u64,
    /// Status.
    pub status: RoundStatus,
    /// Started at.
    pub started_at: DateTime<Utc>,
    /// Completed at.
    pub completed_at: Option<DateTime<Utc>>,
    /// Updates received.
    pub updates: Vec<LocalUpdate>,
    /// Aggregated result.
    pub aggregation: Option<AggregatedUpdate>,
}

/// Round status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoundStatus {
    Collecting,
    Aggregating,
    Completed,
    Failed,
}

/// Federated learning configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederatedConfig {
    /// Minimum participants per round.
    pub min_participants: usize,
    /// Maximum participants per round.
    pub max_participants: usize,
    /// Round timeout in seconds.
    pub round_timeout_secs: u64,
    /// Aggregation strategy.
    pub aggregation: AggregationStrategy,
    /// Enable differential privacy.
    pub differential_privacy: bool,
    /// Privacy epsilon.
    pub epsilon: f32,
    /// Validate updates.
    pub validate_updates: bool,
}

impl Default for FederatedConfig {
    fn default() -> Self {
        Self {
            min_participants: 2,
            max_participants: 100,
            round_timeout_secs: 3600,
            aggregation: AggregationStrategy::FedAvg,
            differential_privacy: false,
            epsilon: 1.0,
            validate_updates: true,
        }
    }
}

/// Aggregation strategies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AggregationStrategy {
    /// Federated Averaging.
    FedAvg,
    /// Weighted by sample count.
    WeightedAvg,
    /// Median aggregation.
    Median,
    /// Trimmed mean.
    TrimmedMean,
}

/// Trait for gradient aggregators.
#[async_trait]
pub trait Aggregator: Send + Sync {
    /// Aggregate local updates.
    async fn aggregate(
        &self,
        updates: &[LocalUpdate],
        config: &FederatedConfig,
    ) -> Result<AggregatedUpdate>;
}

/// Trait for update validators.
#[async_trait]
pub trait UpdateValidator: Send + Sync {
    /// Validate an update.
    async fn validate(&self, update: &LocalUpdate) -> Result<bool>;
}

/// Federated learning coordinator.
pub struct FederatedCoordinator<A: Aggregator, V: UpdateValidator> {
    config: FederatedConfig,
    aggregator: A,
    validator: V,
    participants: Arc<RwLock<HashMap<Uuid, Participant>>>,
    rounds: Arc<RwLock<Vec<FederatedRound>>>,
    current_round: Arc<RwLock<u64>>,
}

impl<A: Aggregator, V: UpdateValidator> FederatedCoordinator<A, V> {
    /// Create a new coordinator.
    pub fn new(config: FederatedConfig, aggregator: A, validator: V) -> Self {
        Self {
            config,
            aggregator,
            validator,
            participants: Arc::new(RwLock::new(HashMap::new())),
            rounds: Arc::new(RwLock::new(Vec::new())),
            current_round: Arc::new(RwLock::new(0)),
        }
    }

    /// Register a participant.
    pub async fn register_participant(&self, participant: Participant) -> Uuid {
        let id = participant.id;
        self.participants.write().await.insert(id, participant);
        id
    }

    /// Start a new round.
    pub async fn start_round(&self) -> Result<u64> {
        let participants = self.participants.read().await;
        let active_count = participants.values().filter(|p| p.active).count();

        if active_count < self.config.min_participants {
            return Err(FederatedError::NoParticipants);
        }

        let mut round_num = self.current_round.write().await;
        *round_num += 1;
        let new_round = *round_num;

        let round = FederatedRound {
            round: new_round,
            status: RoundStatus::Collecting,
            started_at: Utc::now(),
            completed_at: None,
            updates: Vec::new(),
            aggregation: None,
        };

        self.rounds.write().await.push(round);

        Ok(new_round)
    }

    /// Submit a local update.
    pub async fn submit_update(&self, mut update: LocalUpdate) -> Result<()> {
        // Validate participant
        let participants = self.participants.read().await;
        if !participants.contains_key(&update.participant_id) {
            return Err(FederatedError::InvalidUpdate(
                "Unknown participant".to_string(),
            ));
        }
        drop(participants);

        // Validate update
        if self.config.validate_updates {
            update.validated = self.validator.validate(&update).await?;
            if !update.validated {
                return Err(FederatedError::InvalidUpdate(
                    "Validation failed".to_string(),
                ));
            }
        }

        // Add to current round
        let mut rounds = self.rounds.write().await;
        if let Some(current) = rounds.last_mut() {
            if current.status == RoundStatus::Collecting && current.round == update.round {
                current.updates.push(update.clone());

                // Update participant stats
                let mut participants = self.participants.write().await;
                if let Some(p) = participants.get_mut(&update.participant_id) {
                    p.last_update = Some(Utc::now());
                    p.update_count += 1;
                    p.data_size += update.sample_count;
                }

                // Check if ready to aggregate
                if current.updates.len() >= self.config.min_participants {
                    drop(participants);
                    drop(rounds);
                    self.aggregate_round(update.round).await?;
                }
            }
        }

        Ok(())
    }

    /// Aggregate a round.
    pub async fn aggregate_round(&self, round: u64) -> Result<AggregatedUpdate> {
        let mut rounds = self.rounds.write().await;
        let current = rounds.iter_mut().find(|r| r.round == round).ok_or(
            FederatedError::AggregationFailed("Round not found".to_string()),
        )?;

        current.status = RoundStatus::Aggregating;

        let aggregation = self
            .aggregator
            .aggregate(&current.updates, &self.config)
            .await?;

        current.aggregation = Some(aggregation.clone());
        current.status = RoundStatus::Completed;
        current.completed_at = Some(Utc::now());

        Ok(aggregation)
    }

    /// Get current round.
    pub async fn current_round(&self) -> u64 {
        *self.current_round.read().await
    }

    /// Get round status.
    pub async fn round_status(&self, round: u64) -> Option<RoundStatus> {
        self.rounds
            .read()
            .await
            .iter()
            .find(|r| r.round == round)
            .map(|r| r.status)
    }

    /// List participants.
    pub async fn list_participants(&self) -> Vec<Participant> {
        self.participants.read().await.values().cloned().collect()
    }

    /// List rounds.
    pub async fn list_rounds(&self) -> Vec<FederatedRound> {
        self.rounds.read().await.clone()
    }

    /// Get statistics.
    pub async fn stats(&self) -> FederatedStats {
        let participants = self.participants.read().await;
        let rounds = self.rounds.read().await;

        let total_samples: usize = participants.values().map(|p| p.data_size).sum();
        let completed = rounds
            .iter()
            .filter(|r| r.status == RoundStatus::Completed)
            .count();

        FederatedStats {
            total_participants: participants.len(),
            active_participants: participants.values().filter(|p| p.active).count(),
            total_rounds: rounds.len(),
            completed_rounds: completed,
            total_samples,
            current_round: *self.current_round.read().await,
        }
    }
}

/// Federated learning statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederatedStats {
    pub total_participants: usize,
    pub active_participants: usize,
    pub total_rounds: usize,
    pub completed_rounds: usize,
    pub total_samples: usize,
    pub current_round: u64,
}

/// Simple FedAvg aggregator.
pub struct FedAvgAggregator;

#[async_trait]
impl Aggregator for FedAvgAggregator {
    async fn aggregate(
        &self,
        updates: &[LocalUpdate],
        _config: &FederatedConfig,
    ) -> Result<AggregatedUpdate> {
        if updates.is_empty() {
            return Err(FederatedError::NoParticipants);
        }

        let total_samples: usize = updates.iter().map(|u| u.sample_count).sum();
        let grad_len = updates[0].gradients.len();

        let mut aggregated = vec![0.0f32; grad_len];

        for update in updates {
            let weight = update.sample_count as f32 / total_samples as f32;
            for (i, g) in update.gradients.iter().enumerate() {
                if i < aggregated.len() {
                    aggregated[i] += g * weight;
                }
            }
        }

        let avg_loss = updates.iter().map(|u| u.loss).sum::<f32>() / updates.len() as f32;
        let round = updates[0].round;

        Ok(AggregatedUpdate {
            id: Uuid::new_v4(),
            round,
            gradients: aggregated,
            total_samples,
            participant_count: updates.len(),
            avg_loss,
            aggregated_at: Utc::now(),
        })
    }
}

/// Simple update validator.
pub struct SimpleValidator;

#[async_trait]
impl UpdateValidator for SimpleValidator {
    async fn validate(&self, update: &LocalUpdate) -> Result<bool> {
        // Basic validation
        if update.gradients.is_empty() {
            return Ok(false);
        }

        if update.sample_count == 0 {
            return Ok(false);
        }

        // Check for NaN/Inf
        if update
            .gradients
            .iter()
            .any(|g| g.is_nan() || g.is_infinite())
        {
            return Ok(false);
        }

        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_participant_registration() {
        let coordinator = FederatedCoordinator::new(
            FederatedConfig::default(),
            FedAvgAggregator,
            SimpleValidator,
        );

        let p1 = Participant::new("client1");
        let p2 = Participant::new("client2");

        coordinator.register_participant(p1).await;
        coordinator.register_participant(p2).await;

        let participants = coordinator.list_participants().await;
        assert_eq!(participants.len(), 2);
    }

    #[tokio::test]
    async fn test_round_lifecycle() {
        let coordinator = FederatedCoordinator::new(
            FederatedConfig {
                min_participants: 2,
                ..Default::default()
            },
            FedAvgAggregator,
            SimpleValidator,
        );

        let p1 = Participant::new("client1");
        let p2 = Participant::new("client2");

        let p1_id = coordinator.register_participant(p1).await;
        let p2_id = coordinator.register_participant(p2).await;

        let round = coordinator.start_round().await.unwrap();
        assert_eq!(round, 1);

        // Submit updates
        let update1 = LocalUpdate::new(p1_id, 1, vec![0.1, 0.2, 0.3], 100);
        let update2 = LocalUpdate::new(p2_id, 1, vec![0.2, 0.3, 0.4], 150);

        coordinator.submit_update(update1).await.unwrap();
        coordinator.submit_update(update2).await.unwrap();

        // Check aggregation happened
        let status = coordinator.round_status(1).await;
        assert_eq!(status, Some(RoundStatus::Completed));
    }

    #[tokio::test]
    async fn test_fedavg_aggregation() {
        let aggregator = FedAvgAggregator;

        let updates = vec![
            LocalUpdate::new(Uuid::new_v4(), 1, vec![1.0, 2.0], 100),
            LocalUpdate::new(Uuid::new_v4(), 1, vec![3.0, 4.0], 100),
        ];

        let result = aggregator
            .aggregate(&updates, &FederatedConfig::default())
            .await
            .unwrap();

        // Equal weights, so average should be (1+3)/2, (2+4)/2 = 2.0, 3.0
        assert_eq!(result.gradients.len(), 2);
        assert!((result.gradients[0] - 2.0).abs() < 0.001);
        assert!((result.gradients[1] - 3.0).abs() < 0.001);
    }

    #[tokio::test]
    async fn test_validation() {
        let validator = SimpleValidator;

        let valid = LocalUpdate::new(Uuid::new_v4(), 1, vec![1.0, 2.0], 100);
        assert!(validator.validate(&valid).await.unwrap());

        let invalid_empty = LocalUpdate::new(Uuid::new_v4(), 1, vec![], 100);
        assert!(!validator.validate(&invalid_empty).await.unwrap());

        let invalid_nan = LocalUpdate::new(Uuid::new_v4(), 1, vec![f32::NAN], 100);
        assert!(!validator.validate(&invalid_nan).await.unwrap());
    }
}
