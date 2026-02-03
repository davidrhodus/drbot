//! OSINT marketplace data types.
//!
//! Core types for the decentralized OSINT bounty marketplace.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;
use uuid::Uuid;

/// Supported token types for bounty rewards.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum TokenType {
    /// Native SOL token.
    Sol,
    /// USDC stablecoin.
    Usdc,
    /// META token.
    Meta,
    /// ORE token.
    Ore,
}

impl TokenType {
    /// Get the mint address for this token.
    pub fn mint_address(&self) -> Option<Pubkey> {
        match self {
            TokenType::Sol => None, // Native SOL has no mint
            TokenType::Usdc => Some(
                "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"
                    .parse()
                    .unwrap(),
            ),
            TokenType::Meta => Some(
                "METAewgxyPbgwsseH8T16a39CQ5VyVxZi9zXiDPY18m"
                    .parse()
                    .unwrap(),
            ),
            TokenType::Ore => Some(
                "oreoU2P8bN6jkk3jbaiVxYnG1dCXcYxwhwyK9jSybcp"
                    .parse()
                    .unwrap(),
            ),
        }
    }

    /// Get decimals for this token.
    pub fn decimals(&self) -> u8 {
        match self {
            TokenType::Sol => 9,
            TokenType::Usdc => 6,
            TokenType::Meta => 9,
            TokenType::Ore => 9,
        }
    }
}

impl std::fmt::Display for TokenType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TokenType::Sol => write!(f, "SOL"),
            TokenType::Usdc => write!(f, "USDC"),
            TokenType::Meta => write!(f, "META"),
            TokenType::Ore => write!(f, "ORE"),
        }
    }
}

/// Bounty status in the marketplace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BountyStatus {
    /// Open for agents to claim.
    Open,
    /// Claimed by an agent, work in progress.
    Claimed,
    /// Submission received, pending evaluation.
    Submitted,
    /// Resolved (approved or rejected).
    Resolved,
    /// Deadline passed without completion.
    Expired,
    /// Under dispute resolution.
    Disputed,
}

impl BountyStatus {
    /// Check if this status allows claiming.
    pub fn is_claimable(&self) -> bool {
        matches!(self, Self::Open)
    }

    /// Check if this status allows submission.
    pub fn is_submittable(&self) -> bool {
        matches!(self, Self::Claimed)
    }

    /// Check if this is a terminal state.
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Resolved | Self::Expired)
    }
}

impl std::fmt::Display for BountyStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BountyStatus::Open => write!(f, "open"),
            BountyStatus::Claimed => write!(f, "claimed"),
            BountyStatus::Submitted => write!(f, "submitted"),
            BountyStatus::Resolved => write!(f, "resolved"),
            BountyStatus::Expired => write!(f, "expired"),
            BountyStatus::Disputed => write!(f, "disputed"),
        }
    }
}

/// Difficulty level of a bounty.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Difficulty {
    /// Simple lookup or verification.
    Easy,
    /// Requires some investigation.
    Medium,
    /// Complex research required.
    Hard,
    /// Expert-level investigation.
    Expert,
}

impl Difficulty {
    /// Get recommended time for this difficulty.
    pub fn recommended_hours(&self) -> u32 {
        match self {
            Difficulty::Easy => 1,
            Difficulty::Medium => 4,
            Difficulty::Hard => 12,
            Difficulty::Expert => 48,
        }
    }
}

/// Reward structure for a bounty.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reward {
    /// Amount in token's smallest unit.
    pub amount: u64,
    /// Token type.
    pub token: TokenType,
    /// Optional USD value estimate.
    pub usd_value: Option<f64>,
}

impl Reward {
    /// Create a new SOL reward.
    pub fn sol(amount_sol: f64) -> Self {
        Self {
            amount: (amount_sol * 1e9) as u64,
            token: TokenType::Sol,
            usd_value: None,
        }
    }

    /// Create a new USDC reward.
    pub fn usdc(amount: f64) -> Self {
        Self {
            amount: (amount * 1e6) as u64,
            token: TokenType::Usdc,
            usd_value: Some(amount),
        }
    }

    /// Get the UI amount (human readable).
    pub fn ui_amount(&self) -> f64 {
        self.amount as f64 / 10f64.powi(self.token.decimals() as i32)
    }
}

/// Type of evidence provided in a submission.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceType {
    /// URL/link to source.
    Url,
    /// Text content.
    Text,
    /// Image evidence.
    Image,
    /// Archived content (e.g., Wayback Machine).
    Archive,
    /// Document reference.
    Document,
    /// API response data.
    ApiData,
}

/// Evidence item in a submission.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evidence {
    /// Type of evidence.
    pub evidence_type: EvidenceType,
    /// Content (URL, text, base64 image, etc.).
    pub content: String,
    /// Optional note explaining the evidence.
    pub note: Option<String>,
    /// When the evidence was archived (if applicable).
    pub archived_at: Option<DateTime<Utc>>,
}

impl Evidence {
    /// Create a URL evidence item.
    pub fn url(url: impl Into<String>, note: Option<String>) -> Self {
        Self {
            evidence_type: EvidenceType::Url,
            content: url.into(),
            note,
            archived_at: None,
        }
    }

    /// Create a text evidence item.
    pub fn text(content: impl Into<String>, note: Option<String>) -> Self {
        Self {
            evidence_type: EvidenceType::Text,
            content: content.into(),
            note,
            archived_at: None,
        }
    }

    /// Create an archived evidence item.
    pub fn archive(url: impl Into<String>, archived_at: DateTime<Utc>) -> Self {
        Self {
            evidence_type: EvidenceType::Archive,
            content: url.into(),
            note: None,
            archived_at: Some(archived_at),
        }
    }
}

/// An OSINT bounty.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bounty {
    /// Unique identifier.
    pub id: Uuid,
    /// Research question to answer.
    pub question: String,
    /// Detailed description and requirements.
    pub description: String,
    /// Reward for completion.
    pub reward: Reward,
    /// Wallet that posted the bounty.
    pub poster_wallet: Pubkey,
    /// Current status.
    pub status: BountyStatus,
    /// Difficulty level.
    pub difficulty: Difficulty,
    /// Tags/categories.
    pub tags: Vec<String>,
    /// Escrow transaction signature.
    pub escrow_tx: Option<String>,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Deadline for completion.
    pub deadline: DateTime<Utc>,
    /// Agent that claimed the bounty.
    pub claimed_by: Option<Pubkey>,
    /// When the bounty was claimed.
    pub claimed_at: Option<DateTime<Utc>>,
    /// When the claim expires (must submit by this time).
    pub claim_expires_at: Option<DateTime<Utc>>,
    /// Submission (if any).
    pub submission: Option<Submission>,
    /// Resolution (if any).
    pub resolution: Option<Resolution>,
}

impl Bounty {
    /// Check if the bounty can be claimed.
    pub fn can_claim(&self) -> bool {
        self.status.is_claimable() && Utc::now() < self.deadline
    }

    /// Check if the bounty can receive submissions.
    pub fn can_submit(&self, agent: &Pubkey) -> bool {
        self.status.is_submittable()
            && self.claimed_by.as_ref() == Some(agent)
            && self
                .claim_expires_at
                .map(|e| Utc::now() < e)
                .unwrap_or(true)
    }

    /// Check if the bounty is expired.
    pub fn is_expired(&self) -> bool {
        Utc::now() >= self.deadline || self.status == BountyStatus::Expired
    }

    /// Get time remaining until deadline.
    pub fn time_remaining(&self) -> chrono::Duration {
        self.deadline - Utc::now()
    }
}

/// A submission to a bounty.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Submission {
    /// Unique identifier.
    pub id: Uuid,
    /// Bounty this submission is for.
    pub bounty_id: Uuid,
    /// Agent that submitted.
    pub agent_wallet: Pubkey,
    /// The answer to the research question.
    pub answer: String,
    /// Supporting evidence.
    pub evidence: Vec<Evidence>,
    /// Methodology description.
    pub methodology: String,
    /// Agent's confidence level (0-100).
    pub confidence: u8,
    /// When submitted.
    pub submitted_at: DateTime<Utc>,
}

impl Submission {
    /// Create a new submission.
    pub fn new(
        bounty_id: Uuid,
        agent_wallet: Pubkey,
        answer: String,
        evidence: Vec<Evidence>,
        methodology: String,
        confidence: u8,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            bounty_id,
            agent_wallet,
            answer,
            evidence,
            methodology,
            confidence: confidence.min(100),
            submitted_at: Utc::now(),
        }
    }
}

/// Resolution status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionStatus {
    /// Submission approved, reward paid.
    Approved,
    /// Submission rejected, refund issued.
    Rejected,
}

/// Resolution of a bounty.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resolution {
    /// Unique identifier.
    pub id: Uuid,
    /// Bounty that was resolved.
    pub bounty_id: Uuid,
    /// Submission that was evaluated.
    pub submission_id: Uuid,
    /// Resolution status.
    pub status: ResolutionStatus,
    /// Reasoning for the decision.
    pub reasoning: String,
    /// Who/what resolved (e.g., "claude-opus", "manual").
    pub resolver_id: String,
    /// Payment transaction signature (if approved).
    pub payment_tx: Option<String>,
    /// When resolved.
    pub resolved_at: DateTime<Utc>,
}

/// Evaluation criteria used by the AI resolver.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaluationCriteria {
    /// Does the answer address the question?
    pub answers_question: bool,
    /// Is evidence provided?
    pub has_evidence: bool,
    /// Does evidence support the answer?
    pub evidence_supports_answer: bool,
    /// Is the methodology valid?
    pub methodology_valid: bool,
    /// Resolver's confidence in the evaluation (0-100).
    pub confidence: u8,
}

impl EvaluationCriteria {
    /// Check if the submission passes all criteria.
    pub fn is_passing(&self) -> bool {
        self.answers_question
            && self.has_evidence
            && self.evidence_supports_answer
            && self.methodology_valid
    }
}

/// Transaction type in the marketplace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransactionType {
    /// Deposit into escrow.
    Deposit,
    /// Payout to agent.
    Payout,
    /// Refund to poster.
    Refund,
    /// Platform fee collection.
    Fee,
}

/// Transaction record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    /// Unique identifier.
    pub id: Uuid,
    /// Transaction type.
    pub tx_type: TransactionType,
    /// Related bounty.
    pub bounty_id: Uuid,
    /// Amount.
    pub amount: u64,
    /// Token type.
    pub token: TokenType,
    /// Source wallet.
    pub from_wallet: Pubkey,
    /// Destination wallet.
    pub to_wallet: Pubkey,
    /// Fee amount (if applicable).
    pub fee_amount: Option<u64>,
    /// Solana transaction signature.
    pub tx_signature: String,
    /// Transaction status.
    pub status: TransactionStatus,
    /// When created.
    pub created_at: DateTime<Utc>,
}

/// Transaction status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransactionStatus {
    /// Transaction pending.
    Pending,
    /// Transaction confirmed.
    Confirmed,
    /// Transaction failed.
    Failed,
}

/// Fee structure for the marketplace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeeStructure {
    /// Creation fee (basis points, e.g., 250 = 2.5%).
    pub creation_fee_bps: u16,
    /// Payout fee (basis points).
    pub payout_fee_bps: u16,
    /// Minimum SOL for gas.
    pub min_sol_for_gas: f64,
    /// Treasury wallet for fee collection.
    pub treasury_wallet: Pubkey,
}

impl Default for FeeStructure {
    fn default() -> Self {
        Self {
            creation_fee_bps: 250, // 2.5%
            payout_fee_bps: 250,   // 2.5%
            min_sol_for_gas: 0.01,
            treasury_wallet: "OSNTfee111111111111111111111111111111111111"
                .parse()
                .unwrap_or_else(|_| Pubkey::new_unique()),
        }
    }
}

impl FeeStructure {
    /// Calculate creation fee for an amount.
    pub fn calculate_creation_fee(&self, amount: u64) -> u64 {
        (amount as u128 * self.creation_fee_bps as u128 / 10000) as u64
    }

    /// Calculate payout fee for an amount.
    pub fn calculate_payout_fee(&self, amount: u64) -> u64 {
        (amount as u128 * self.payout_fee_bps as u128 / 10000) as u64
    }

    /// Calculate total fees for a bounty.
    pub fn total_fee_bps(&self) -> u16 {
        self.creation_fee_bps + self.payout_fee_bps
    }
}

/// Agent profile in the marketplace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentProfile {
    /// Agent wallet address.
    pub wallet: Pubkey,
    /// Display name.
    pub name: Option<String>,
    /// Total bounties completed.
    pub bounties_completed: u32,
    /// Total bounties claimed.
    pub bounties_claimed: u32,
    /// Success rate (0-100).
    pub success_rate: f64,
    /// Total earnings in USD.
    pub total_earnings_usd: f64,
    /// Specialization tags.
    pub specializations: Vec<String>,
    /// When registered.
    pub registered_at: DateTime<Utc>,
}

impl AgentProfile {
    /// Create a new agent profile.
    pub fn new(wallet: Pubkey) -> Self {
        Self {
            wallet,
            name: None,
            bounties_completed: 0,
            bounties_claimed: 0,
            success_rate: 0.0,
            total_earnings_usd: 0.0,
            specializations: Vec::new(),
            registered_at: Utc::now(),
        }
    }

    /// Update stats after a resolution.
    pub fn record_resolution(&mut self, approved: bool, reward_usd: f64) {
        self.bounties_claimed += 1;
        if approved {
            self.bounties_completed += 1;
            self.total_earnings_usd += reward_usd;
        }
        self.success_rate = if self.bounties_claimed > 0 {
            (self.bounties_completed as f64 / self.bounties_claimed as f64) * 100.0
        } else {
            0.0
        };
    }
}

/// Leaderboard entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeaderboardEntry {
    /// Rank position.
    pub rank: u32,
    /// Agent wallet.
    pub wallet: Pubkey,
    /// Display name.
    pub name: Option<String>,
    /// Bounties completed.
    pub completed: u32,
    /// Total earnings in USD.
    pub earnings_usd: f64,
    /// Success rate.
    pub success_rate: f64,
}

/// Marketplace statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketplaceStats {
    /// Total bounties posted.
    pub total_bounties: u64,
    /// Open bounties.
    pub open_bounties: u64,
    /// Completed bounties.
    pub completed_bounties: u64,
    /// Total value locked in escrow (USD).
    pub tvl_usd: f64,
    /// Total payouts (USD).
    pub total_payouts_usd: f64,
    /// Unique posters.
    pub unique_posters: u64,
    /// Unique agents.
    pub unique_agents: u64,
    /// Average completion time (hours).
    pub avg_completion_hours: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_type() {
        assert_eq!(TokenType::Sol.decimals(), 9);
        assert_eq!(TokenType::Usdc.decimals(), 6);
        assert!(TokenType::Sol.mint_address().is_none());
        assert!(TokenType::Usdc.mint_address().is_some());
    }

    #[test]
    fn test_bounty_status() {
        assert!(BountyStatus::Open.is_claimable());
        assert!(!BountyStatus::Claimed.is_claimable());
        assert!(BountyStatus::Claimed.is_submittable());
        assert!(BountyStatus::Resolved.is_terminal());
    }

    #[test]
    fn test_reward_creation() {
        let sol_reward = Reward::sol(1.5);
        assert_eq!(sol_reward.amount, 1_500_000_000);
        assert!((sol_reward.ui_amount() - 1.5).abs() < 0.0001);

        let usdc_reward = Reward::usdc(100.0);
        assert_eq!(usdc_reward.amount, 100_000_000);
        assert_eq!(usdc_reward.usd_value, Some(100.0));
    }

    #[test]
    fn test_fee_structure() {
        let fees = FeeStructure::default();
        assert_eq!(fees.total_fee_bps(), 500); // 5% total

        let amount = 1_000_000_000u64; // 1 SOL
        let creation_fee = fees.calculate_creation_fee(amount);
        assert_eq!(creation_fee, 25_000_000); // 2.5%
    }

    #[test]
    fn test_agent_profile_stats() {
        let mut profile = AgentProfile::new(Pubkey::new_unique());
        profile.record_resolution(true, 100.0);
        profile.record_resolution(true, 50.0);
        profile.record_resolution(false, 0.0);

        assert_eq!(profile.bounties_completed, 2);
        assert_eq!(profile.bounties_claimed, 3);
        assert!((profile.success_rate - 66.67).abs() < 0.1);
        assert!((profile.total_earnings_usd - 150.0).abs() < 0.01);
    }

    #[test]
    fn test_evaluation_criteria() {
        let passing = EvaluationCriteria {
            answers_question: true,
            has_evidence: true,
            evidence_supports_answer: true,
            methodology_valid: true,
            confidence: 90,
        };
        assert!(passing.is_passing());

        let failing = EvaluationCriteria {
            answers_question: true,
            has_evidence: false,
            evidence_supports_answer: false,
            methodology_valid: true,
            confidence: 50,
        };
        assert!(!failing.is_passing());
    }
}
