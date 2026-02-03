//! OSINT bounty management.
//!
//! Handles bounty creation, claiming, submission, and lifecycle management.

use super::escrow::{BountyEscrow, EscrowManager, EscrowStatus};
use super::resolver::{EvaluationResult, SubmissionResolver};
use super::types::{
    AgentProfile, Bounty, BountyStatus, Difficulty, Evidence, FeeStructure, Resolution,
    ResolutionStatus, Reward, Submission, TokenType,
};
use crate::{Result, SolanaError};
use chrono::{Duration, Utc};
use solana_sdk::pubkey::Pubkey;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};
use uuid::Uuid;

/// Bounty creation parameters.
#[derive(Debug, Clone)]
pub struct CreateBountyParams {
    /// Research question.
    pub question: String,
    /// Detailed description.
    pub description: String,
    /// Reward amount.
    pub reward: Reward,
    /// Deadline (duration from now).
    pub deadline_hours: u32,
    /// Difficulty level.
    pub difficulty: Difficulty,
    /// Tags/categories.
    pub tags: Vec<String>,
}

/// Claim response.
#[derive(Debug, Clone)]
pub struct ClaimResponse {
    /// Whether claim was successful.
    pub success: bool,
    /// Bounty ID.
    pub bounty_id: Uuid,
    /// When the claim expires.
    pub expires_at: chrono::DateTime<chrono::Utc>,
    /// Message.
    pub message: String,
}

/// Submit response.
#[derive(Debug, Clone)]
pub struct SubmitResponse {
    /// Whether submission was accepted.
    pub success: bool,
    /// Submission ID.
    pub submission_id: Uuid,
    /// Estimated time for resolution.
    pub resolver_eta_minutes: u32,
    /// Message.
    pub message: String,
}

/// Bounty manager for the OSINT marketplace.
pub struct BountyManager {
    bounties: Arc<RwLock<HashMap<Uuid, Bounty>>>,
    escrows: Arc<RwLock<HashMap<Uuid, BountyEscrow>>>,
    agents: Arc<RwLock<HashMap<Pubkey, AgentProfile>>>,
    resolver: SubmissionResolver,
    fee_structure: FeeStructure,
    claim_duration_hours: u32,
}

impl BountyManager {
    /// Create a new bounty manager.
    pub fn new() -> Self {
        Self {
            bounties: Arc::new(RwLock::new(HashMap::new())),
            escrows: Arc::new(RwLock::new(HashMap::new())),
            agents: Arc::new(RwLock::new(HashMap::new())),
            resolver: SubmissionResolver::new(),
            fee_structure: FeeStructure::default(),
            claim_duration_hours: 24,
        }
    }

    /// Set custom fee structure.
    pub fn with_fees(mut self, fee_structure: FeeStructure) -> Self {
        self.fee_structure = fee_structure;
        self
    }

    /// Set claim duration.
    pub fn with_claim_duration(mut self, hours: u32) -> Self {
        self.claim_duration_hours = hours;
        self
    }

    /// Create a new bounty.
    pub async fn create_bounty(
        &self,
        params: CreateBountyParams,
        poster_wallet: Pubkey,
        escrow_tx: String,
    ) -> Result<Bounty> {
        let bounty_id = Uuid::new_v4();
        let now = Utc::now();
        let deadline = now + Duration::hours(params.deadline_hours as i64);

        let bounty = Bounty {
            id: bounty_id,
            question: params.question,
            description: params.description,
            reward: params.reward.clone(),
            poster_wallet,
            status: BountyStatus::Open,
            difficulty: params.difficulty,
            tags: params.tags,
            escrow_tx: Some(escrow_tx.clone()),
            created_at: now,
            deadline,
            claimed_by: None,
            claimed_at: None,
            claim_expires_at: None,
            submission: None,
            resolution: None,
        };

        // Create escrow record
        let escrow = BountyEscrow::new(
            bounty_id,
            params.reward.amount,
            params.reward.token,
            escrow_tx,
            &self.fee_structure,
        );

        // Store bounty and escrow
        self.bounties
            .write()
            .await
            .insert(bounty_id, bounty.clone());
        self.escrows.write().await.insert(bounty_id, escrow);

        info!(
            bounty_id = %bounty_id,
            poster = %poster_wallet,
            reward = params.reward.ui_amount(),
            token = %params.reward.token,
            "Bounty created"
        );

        Ok(bounty)
    }

    /// Get a bounty by ID.
    pub async fn get_bounty(&self, id: Uuid) -> Option<Bounty> {
        self.bounties.read().await.get(&id).cloned()
    }

    /// List all open bounties.
    pub async fn list_open_bounties(&self) -> Vec<Bounty> {
        self.bounties
            .read()
            .await
            .values()
            .filter(|b| b.status == BountyStatus::Open && !b.is_expired())
            .cloned()
            .collect()
    }

    /// List bounties by status.
    pub async fn list_bounties_by_status(&self, status: BountyStatus) -> Vec<Bounty> {
        self.bounties
            .read()
            .await
            .values()
            .filter(|b| b.status == status)
            .cloned()
            .collect()
    }

    /// Search bounties by tag.
    pub async fn search_by_tag(&self, tag: &str) -> Vec<Bounty> {
        let tag_lower = tag.to_lowercase();
        self.bounties
            .read()
            .await
            .values()
            .filter(|b| {
                b.status == BountyStatus::Open
                    && b.tags.iter().any(|t| t.to_lowercase().contains(&tag_lower))
            })
            .cloned()
            .collect()
    }

    /// Claim a bounty.
    pub async fn claim_bounty(
        &self,
        bounty_id: Uuid,
        agent_wallet: Pubkey,
    ) -> Result<ClaimResponse> {
        let mut bounties = self.bounties.write().await;

        let bounty = bounties
            .get_mut(&bounty_id)
            .ok_or_else(|| SolanaError::DeFiProtocolError("Bounty not found".to_string()))?;

        // Validate claim
        if !bounty.can_claim() {
            return Err(SolanaError::DeFiProtocolError(format!(
                "Bounty cannot be claimed: status is {}",
                bounty.status
            )));
        }

        if bounty.is_expired() {
            bounty.status = BountyStatus::Expired;
            return Err(SolanaError::DeFiProtocolError(
                "Bounty has expired".to_string(),
            ));
        }

        // Update bounty
        let now = Utc::now();
        let expires_at = now + Duration::hours(self.claim_duration_hours as i64);

        bounty.status = BountyStatus::Claimed;
        bounty.claimed_by = Some(agent_wallet);
        bounty.claimed_at = Some(now);
        bounty.claim_expires_at = Some(expires_at);

        // Register agent if new
        drop(bounties);
        self.ensure_agent_registered(agent_wallet).await;

        info!(
            bounty_id = %bounty_id,
            agent = %agent_wallet,
            expires_at = %expires_at,
            "Bounty claimed"
        );

        Ok(ClaimResponse {
            success: true,
            bounty_id,
            expires_at,
            message: format!(
                "Bounty claimed successfully. Submit your findings before {}",
                expires_at
            ),
        })
    }

    /// Submit findings for a bounty.
    pub async fn submit_findings(
        &self,
        bounty_id: Uuid,
        agent_wallet: Pubkey,
        answer: String,
        evidence: Vec<Evidence>,
        methodology: String,
        confidence: u8,
    ) -> Result<SubmitResponse> {
        let mut bounties = self.bounties.write().await;

        let bounty = bounties
            .get_mut(&bounty_id)
            .ok_or_else(|| SolanaError::DeFiProtocolError("Bounty not found".to_string()))?;

        // Validate submission
        if !bounty.can_submit(&agent_wallet) {
            return Err(SolanaError::DeFiProtocolError(
                "Cannot submit: bounty not claimed by you or claim expired".to_string(),
            ));
        }

        // Create submission
        let submission = Submission::new(
            bounty_id,
            agent_wallet,
            answer,
            evidence,
            methodology,
            confidence,
        );

        // Validate submission content
        let issues = self.resolver.validate_submission(&submission);
        if !issues.is_empty() {
            return Err(SolanaError::DeFiProtocolError(format!(
                "Submission validation failed: {}",
                issues.join(", ")
            )));
        }

        let submission_id = submission.id;
        bounty.submission = Some(submission);
        bounty.status = BountyStatus::Submitted;

        info!(
            bounty_id = %bounty_id,
            submission_id = %submission_id,
            agent = %agent_wallet,
            "Submission received"
        );

        Ok(SubmitResponse {
            success: true,
            submission_id,
            resolver_eta_minutes: 5,
            message: "Submission received. AI evaluation in progress.".to_string(),
        })
    }

    /// Resolve a submission with an evaluation result.
    pub async fn resolve_submission(
        &self,
        bounty_id: Uuid,
        evaluation: EvaluationResult,
    ) -> Result<Resolution> {
        let mut bounties = self.bounties.write().await;

        let bounty = bounties
            .get_mut(&bounty_id)
            .ok_or_else(|| SolanaError::DeFiProtocolError("Bounty not found".to_string()))?;

        let submission = bounty
            .submission
            .as_ref()
            .ok_or_else(|| SolanaError::DeFiProtocolError("No submission found".to_string()))?;

        // Create resolution
        let resolution = self
            .resolver
            .resolve_with_evaluation(bounty_id, submission, evaluation);

        // Update bounty
        bounty.status = BountyStatus::Resolved;
        bounty.resolution = Some(resolution.clone());

        // Update escrow status
        drop(bounties);
        let mut escrows = self.escrows.write().await;
        if let Some(escrow) = escrows.get_mut(&bounty_id) {
            if resolution.status == ResolutionStatus::Approved {
                escrow.mark_released();
            } else {
                escrow.mark_refunded();
            }
        }

        // Update agent stats
        if let Some(agent_wallet) = self
            .bounties
            .read()
            .await
            .get(&bounty_id)
            .and_then(|b| b.claimed_by)
        {
            let reward_usd = self
                .bounties
                .read()
                .await
                .get(&bounty_id)
                .map(|b| b.reward.usd_value.unwrap_or(0.0))
                .unwrap_or(0.0);

            let mut agents = self.agents.write().await;
            if let Some(agent) = agents.get_mut(&agent_wallet) {
                agent
                    .record_resolution(resolution.status == ResolutionStatus::Approved, reward_usd);
            }
        }

        info!(
            bounty_id = %bounty_id,
            status = ?resolution.status,
            "Bounty resolved"
        );

        Ok(resolution)
    }

    /// Cancel a bounty (poster only, before claim).
    pub async fn cancel_bounty(&self, bounty_id: Uuid, poster_wallet: &Pubkey) -> Result<()> {
        let mut bounties = self.bounties.write().await;

        let bounty = bounties
            .get_mut(&bounty_id)
            .ok_or_else(|| SolanaError::DeFiProtocolError("Bounty not found".to_string()))?;

        if bounty.poster_wallet != *poster_wallet {
            return Err(SolanaError::DeFiProtocolError(
                "Only the poster can cancel".to_string(),
            ));
        }

        if bounty.status != BountyStatus::Open {
            return Err(SolanaError::DeFiProtocolError(
                "Can only cancel open bounties".to_string(),
            ));
        }

        bounty.status = BountyStatus::Expired;

        // Mark escrow for refund
        drop(bounties);
        let mut escrows = self.escrows.write().await;
        if let Some(escrow) = escrows.get_mut(&bounty_id) {
            escrow.mark_refunded();
        }

        info!(bounty_id = %bounty_id, "Bounty cancelled");

        Ok(())
    }

    /// Release an expired claim.
    pub async fn release_expired_claim(&self, bounty_id: Uuid) -> Result<()> {
        let mut bounties = self.bounties.write().await;

        let bounty = bounties
            .get_mut(&bounty_id)
            .ok_or_else(|| SolanaError::DeFiProtocolError("Bounty not found".to_string()))?;

        if bounty.status != BountyStatus::Claimed {
            return Err(SolanaError::DeFiProtocolError(
                "Bounty is not claimed".to_string(),
            ));
        }

        let expired = bounty
            .claim_expires_at
            .map(|e| Utc::now() >= e)
            .unwrap_or(false);

        if !expired {
            return Err(SolanaError::DeFiProtocolError(
                "Claim has not expired".to_string(),
            ));
        }

        // Reset to open
        bounty.status = BountyStatus::Open;
        bounty.claimed_by = None;
        bounty.claimed_at = None;
        bounty.claim_expires_at = None;

        info!(bounty_id = %bounty_id, "Expired claim released");

        Ok(())
    }

    /// Dispute a resolution.
    pub async fn dispute_resolution(&self, bounty_id: Uuid, reason: String) -> Result<()> {
        let mut bounties = self.bounties.write().await;

        let bounty = bounties
            .get_mut(&bounty_id)
            .ok_or_else(|| SolanaError::DeFiProtocolError("Bounty not found".to_string()))?;

        if bounty.status != BountyStatus::Resolved {
            return Err(SolanaError::DeFiProtocolError(
                "Can only dispute resolved bounties".to_string(),
            ));
        }

        bounty.status = BountyStatus::Disputed;

        // Lock escrow
        drop(bounties);
        let mut escrows = self.escrows.write().await;
        if let Some(escrow) = escrows.get_mut(&bounty_id) {
            escrow.mark_disputed();
        }

        warn!(bounty_id = %bounty_id, reason = %reason, "Bounty disputed");

        Ok(())
    }

    /// Get agent profile.
    pub async fn get_agent(&self, wallet: &Pubkey) -> Option<AgentProfile> {
        self.agents.read().await.get(wallet).cloned()
    }

    /// Get leaderboard.
    pub async fn get_leaderboard(&self, limit: usize) -> Vec<AgentProfile> {
        let mut agents: Vec<_> = self.agents.read().await.values().cloned().collect();

        agents.sort_by(|a, b| {
            b.bounties_completed
                .cmp(&a.bounties_completed)
                .then_with(|| {
                    b.total_earnings_usd
                        .partial_cmp(&a.total_earnings_usd)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
        });

        agents.into_iter().take(limit).collect()
    }

    /// Ensure an agent is registered.
    async fn ensure_agent_registered(&self, wallet: Pubkey) {
        let mut agents = self.agents.write().await;
        agents
            .entry(wallet)
            .or_insert_with(|| AgentProfile::new(wallet));
    }

    /// Get resolver.
    pub fn resolver(&self) -> &SubmissionResolver {
        &self.resolver
    }

    /// Get fee structure.
    pub fn fee_structure(&self) -> &FeeStructure {
        &self.fee_structure
    }

    /// Get escrow for a bounty.
    pub async fn get_escrow(&self, bounty_id: Uuid) -> Option<BountyEscrow> {
        self.escrows.read().await.get(&bounty_id).cloned()
    }
}

impl Default for BountyManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_bounty() {
        let manager = BountyManager::new();

        let params = CreateBountyParams {
            question: "What is the capital of France?".to_string(),
            description: "Simple geography question".to_string(),
            reward: Reward::sol(0.5),
            deadline_hours: 24,
            difficulty: Difficulty::Easy,
            tags: vec!["geography".to_string()],
        };

        let bounty = manager
            .create_bounty(params, Pubkey::new_unique(), "test_tx".to_string())
            .await
            .unwrap();

        assert_eq!(bounty.status, BountyStatus::Open);
        assert!(bounty.can_claim());
    }

    #[tokio::test]
    async fn test_claim_bounty() {
        let manager = BountyManager::new();

        let params = CreateBountyParams {
            question: "Test question".to_string(),
            description: "Test description".to_string(),
            reward: Reward::sol(1.0),
            deadline_hours: 48,
            difficulty: Difficulty::Medium,
            tags: vec![],
        };

        let bounty = manager
            .create_bounty(params, Pubkey::new_unique(), "test_tx".to_string())
            .await
            .unwrap();

        let agent = Pubkey::new_unique();
        let response = manager.claim_bounty(bounty.id, agent).await.unwrap();

        assert!(response.success);

        let updated = manager.get_bounty(bounty.id).await.unwrap();
        assert_eq!(updated.status, BountyStatus::Claimed);
        assert_eq!(updated.claimed_by, Some(agent));
    }

    #[tokio::test]
    async fn test_submit_findings() {
        let manager = BountyManager::new();

        let params = CreateBountyParams {
            question: "Who founded SpaceX?".to_string(),
            description: "Find the founder of SpaceX".to_string(),
            reward: Reward::usdc(50.0),
            deadline_hours: 24,
            difficulty: Difficulty::Easy,
            tags: vec!["companies".to_string()],
        };

        let bounty = manager
            .create_bounty(params, Pubkey::new_unique(), "test_tx".to_string())
            .await
            .unwrap();

        let agent = Pubkey::new_unique();
        manager.claim_bounty(bounty.id, agent).await.unwrap();

        let response = manager
            .submit_findings(
                bounty.id,
                agent,
                "Elon Musk founded SpaceX in 2002. He serves as CEO and chief engineer."
                    .to_string(),
                vec![Evidence::url(
                    "https://www.spacex.com/about",
                    Some("Official website".to_string()),
                )],
                "Verified on official company website and Wikipedia.".to_string(),
                95,
            )
            .await
            .unwrap();

        assert!(response.success);

        let updated = manager.get_bounty(bounty.id).await.unwrap();
        assert_eq!(updated.status, BountyStatus::Submitted);
        assert!(updated.submission.is_some());
    }

    #[tokio::test]
    async fn test_list_open_bounties() {
        let manager = BountyManager::new();

        // Create multiple bounties
        for i in 0..5 {
            let params = CreateBountyParams {
                question: format!("Question {}", i),
                description: format!("Description {}", i),
                reward: Reward::sol(0.1 * (i + 1) as f64),
                deadline_hours: 24,
                difficulty: Difficulty::Easy,
                tags: vec!["test".to_string()],
            };

            manager
                .create_bounty(params, Pubkey::new_unique(), format!("tx_{}", i))
                .await
                .unwrap();
        }

        let open = manager.list_open_bounties().await;
        assert_eq!(open.len(), 5);
    }

    #[tokio::test]
    async fn test_search_by_tag() {
        let manager = BountyManager::new();

        let params1 = CreateBountyParams {
            question: "Question 1".to_string(),
            description: "Desc".to_string(),
            reward: Reward::sol(1.0),
            deadline_hours: 24,
            difficulty: Difficulty::Easy,
            tags: vec!["crypto".to_string(), "defi".to_string()],
        };

        let params2 = CreateBountyParams {
            question: "Question 2".to_string(),
            description: "Desc".to_string(),
            reward: Reward::sol(1.0),
            deadline_hours: 24,
            difficulty: Difficulty::Easy,
            tags: vec!["security".to_string()],
        };

        manager
            .create_bounty(params1, Pubkey::new_unique(), "tx1".to_string())
            .await
            .unwrap();
        manager
            .create_bounty(params2, Pubkey::new_unique(), "tx2".to_string())
            .await
            .unwrap();

        let crypto = manager.search_by_tag("crypto").await;
        assert_eq!(crypto.len(), 1);

        let security = manager.search_by_tag("security").await;
        assert_eq!(security.len(), 1);
    }
}
