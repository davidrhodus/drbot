//! OSINT marketplace integration.
//!
//! A decentralized marketplace for open-source intelligence bounties on Solana.
//!
//! # Overview
//!
//! The OSINT marketplace connects bounty posters with AI agents for research tasks:
//!
//! 1. **Posters** create bounties with research questions and escrow rewards
//! 2. **Agents** discover, claim, and complete bounties
//! 3. **AI Resolver** evaluates submissions for quality and accuracy
//! 4. **Escrow** handles secure payment distribution
//!
//! # Example
//!
//! ```rust,no_run
//! use drbot_solana::osint::{
//!     BountyManager, CreateBountyParams, Difficulty, Evidence, Reward,
//! };
//! use solana_sdk::pubkey::Pubkey;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Create bounty manager
//! let manager = BountyManager::new();
//!
//! // Post a bounty
//! let params = CreateBountyParams {
//!     question: "Who is the current CEO of OpenAI?".to_string(),
//!     description: "Find the current CEO with evidence".to_string(),
//!     reward: Reward::sol(0.5),
//!     deadline_hours: 24,
//!     difficulty: Difficulty::Easy,
//!     tags: vec!["tech".to_string(), "leadership".to_string()],
//! };
//!
//! let bounty = manager
//!     .create_bounty(params, Pubkey::new_unique(), "escrow_tx".to_string())
//!     .await?;
//!
//! // Agent claims the bounty
//! let agent = Pubkey::new_unique();
//! manager.claim_bounty(bounty.id, agent).await?;
//!
//! // Agent submits findings
//! manager.submit_findings(
//!     bounty.id,
//!     agent,
//!     "Sam Altman is the current CEO of OpenAI.".to_string(),
//!     vec![Evidence::url("https://openai.com/about", None)],
//!     "Verified on official website.".to_string(),
//!     95,
//! ).await?;
//! # Ok(())
//! # }
//! ```
//!
//! # Modules
//!
//! - [`types`]: Core data types (Bounty, Submission, Resolution, etc.)
//! - [`bounty`]: Bounty lifecycle management
//! - [`escrow`]: Escrow and payment handling
//! - [`resolver`]: AI-powered submission evaluation
//! - [`client`]: API client for external marketplaces

pub mod bounty;
pub mod client;
pub mod escrow;
pub mod resolver;
pub mod types;

// Re-exports
pub use types::{
    AgentProfile, Bounty, BountyStatus, Difficulty, EvaluationCriteria, Evidence, EvidenceType,
    FeeStructure, LeaderboardEntry, MarketplaceStats, Resolution, ResolutionStatus, Reward,
    Submission, TokenType, Transaction, TransactionStatus, TransactionType,
};

pub use bounty::{BountyManager, ClaimResponse, CreateBountyParams, SubmitResponse};

pub use escrow::{BountyEscrow, DepositInstructions, EscrowManager, EscrowStatus};

pub use resolver::{
    BatchResolver, EvaluationResult, PromptBuilder, ResolverConfig, SubmissionResolver,
};

pub use client::{
    AgentAuth, AgentEndpoints, AgentSpec, AuthChallenge, AuthToken, CreateBountyRequest,
    OsintClient, OsintClientConfig, PaginatedResponse, SubmitFindingsRequest,
};

/// OSINT marketplace skill actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OsintAction {
    /// List available bounties.
    ListBounties,
    /// Get bounty details.
    GetBounty,
    /// Create a new bounty.
    CreateBounty,
    /// Claim a bounty.
    ClaimBounty,
    /// Submit findings.
    SubmitFindings,
    /// Get marketplace stats.
    GetStats,
    /// Get leaderboard.
    GetLeaderboard,
    /// Search bounties.
    SearchBounties,
}

impl std::fmt::Display for OsintAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OsintAction::ListBounties => write!(f, "list"),
            OsintAction::GetBounty => write!(f, "get"),
            OsintAction::CreateBounty => write!(f, "create"),
            OsintAction::ClaimBounty => write!(f, "claim"),
            OsintAction::SubmitFindings => write!(f, "submit"),
            OsintAction::GetStats => write!(f, "stats"),
            OsintAction::GetLeaderboard => write!(f, "leaderboard"),
            OsintAction::SearchBounties => write!(f, "search"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_osint_action_display() {
        assert_eq!(OsintAction::ListBounties.to_string(), "list");
        assert_eq!(OsintAction::CreateBounty.to_string(), "create");
        assert_eq!(OsintAction::ClaimBounty.to_string(), "claim");
    }

    #[tokio::test]
    async fn test_full_bounty_lifecycle() {
        let manager = BountyManager::new();

        // 1. Create bounty
        let params = CreateBountyParams {
            question: "What year was Bitcoin created?".to_string(),
            description: "Find the year Bitcoin was created with evidence.".to_string(),
            reward: Reward::sol(0.1),
            deadline_hours: 48,
            difficulty: Difficulty::Easy,
            tags: vec!["crypto".to_string(), "history".to_string()],
        };

        let poster = solana_sdk::pubkey::Pubkey::new_unique();
        let bounty = manager
            .create_bounty(params, poster, "tx_123".to_string())
            .await
            .unwrap();

        assert_eq!(bounty.status, BountyStatus::Open);

        // 2. Agent claims
        let agent = solana_sdk::pubkey::Pubkey::new_unique();
        let claim = manager.claim_bounty(bounty.id, agent).await.unwrap();
        assert!(claim.success);

        let bounty = manager.get_bounty(bounty.id).await.unwrap();
        assert_eq!(bounty.status, BountyStatus::Claimed);

        // 3. Agent submits
        let submit = manager
            .submit_findings(
                bounty.id,
                agent,
                "Bitcoin was created in 2009 by Satoshi Nakamoto. The genesis block was mined on January 3, 2009."
                    .to_string(),
                vec![
                    Evidence::url(
                        "https://bitcoin.org/bitcoin.pdf",
                        Some("Original whitepaper".to_string()),
                    ),
                    Evidence::text(
                        "Genesis block timestamp: 2009-01-03".to_string(),
                        None,
                    ),
                ],
                "Verified through blockchain data and original whitepaper.".to_string(),
                98,
            )
            .await
            .unwrap();

        assert!(submit.success);

        let bounty = manager.get_bounty(bounty.id).await.unwrap();
        assert_eq!(bounty.status, BountyStatus::Submitted);
        assert!(bounty.submission.is_some());

        // 4. Resolve
        let evaluation = EvaluationResult {
            approved: true,
            criteria: EvaluationCriteria {
                answers_question: true,
                has_evidence: true,
                evidence_supports_answer: true,
                methodology_valid: true,
                confidence: 95,
            },
            reasoning: "Accurate answer with strong supporting evidence.".to_string(),
            suggestions: None,
        };

        let resolution = manager
            .resolve_submission(bounty.id, evaluation)
            .await
            .unwrap();
        assert_eq!(resolution.status, ResolutionStatus::Approved);

        let bounty = manager.get_bounty(bounty.id).await.unwrap();
        assert_eq!(bounty.status, BountyStatus::Resolved);
    }
}
