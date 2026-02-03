//! OSINT marketplace client.
//!
//! Client for interacting with OSINT marketplace instances.

use super::types::{
    AgentProfile, Bounty, BountyStatus, Evidence, LeaderboardEntry, MarketplaceStats, Resolution,
    Submission,
};
use crate::{Result, SolanaError};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use solana_sdk::{
    pubkey::Pubkey,
    signature::{Keypair, Signer},
};
use tracing::{debug, info};
use uuid::Uuid;

/// OSINT marketplace client configuration.
#[derive(Debug, Clone)]
pub struct OsintClientConfig {
    /// Base URL of the marketplace API.
    pub base_url: String,
    /// Request timeout in seconds.
    pub timeout_secs: u64,
    /// Retry attempts for failed requests.
    pub max_retries: u32,
}

impl Default for OsintClientConfig {
    fn default() -> Self {
        Self {
            base_url: "https://osint.market".to_string(),
            timeout_secs: 30,
            max_retries: 3,
        }
    }
}

/// Authentication challenge response.
#[derive(Debug, Serialize, Deserialize)]
pub struct AuthChallenge {
    /// Challenge message to sign.
    pub challenge: String,
    /// Challenge ID.
    pub challenge_id: String,
    /// Expiration timestamp.
    pub expires_at: String,
}

/// Authentication token.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthToken {
    /// JWT token.
    pub token: String,
    /// Wallet address.
    pub wallet: String,
    /// Expiration timestamp.
    pub expires_at: String,
}

/// Paginated response wrapper.
#[derive(Debug, Deserialize)]
pub struct PaginatedResponse<T> {
    /// Items in this page.
    pub items: Vec<T>,
    /// Total count.
    pub total: u64,
    /// Current page.
    pub page: u32,
    /// Items per page.
    pub per_page: u32,
}

/// Create bounty request.
#[derive(Debug, Serialize)]
pub struct CreateBountyRequest {
    /// Research question.
    pub question: String,
    /// Detailed description.
    pub description: String,
    /// Reward amount (UI amount).
    pub reward_amount: f64,
    /// Reward token ("SOL", "USDC", etc.).
    pub reward_token: String,
    /// Deadline in hours from now.
    pub deadline_hours: u32,
    /// Difficulty level.
    pub difficulty: String,
    /// Tags.
    pub tags: Vec<String>,
    /// Escrow transaction signature.
    pub escrow_tx: String,
}

/// Submit findings request.
#[derive(Debug, Serialize)]
pub struct SubmitFindingsRequest {
    /// The answer.
    pub answer: String,
    /// Evidence items.
    pub evidence: Vec<EvidenceItem>,
    /// Methodology description.
    pub methodology: String,
    /// Confidence level (0-100).
    pub confidence: u8,
}

/// Evidence item for API.
#[derive(Debug, Serialize, Deserialize)]
pub struct EvidenceItem {
    /// Evidence type.
    #[serde(rename = "type")]
    pub evidence_type: String,
    /// Content.
    pub content: String,
    /// Optional note.
    pub note: Option<String>,
}

impl From<&Evidence> for EvidenceItem {
    fn from(e: &Evidence) -> Self {
        Self {
            evidence_type: format!("{:?}", e.evidence_type).to_lowercase(),
            content: e.content.clone(),
            note: e.note.clone(),
        }
    }
}

/// OSINT marketplace API client.
pub struct OsintClient {
    config: OsintClientConfig,
    http_client: Client,
    auth_token: Option<AuthToken>,
}

impl OsintClient {
    /// Create a new client with default configuration.
    pub fn new() -> Self {
        Self::with_config(OsintClientConfig::default())
    }

    /// Create a new client with custom configuration.
    pub fn with_config(config: OsintClientConfig) -> Self {
        let http_client = Client::builder()
            .timeout(std::time::Duration::from_secs(config.timeout_secs))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            config,
            http_client,
            auth_token: None,
        }
    }

    /// Create a client for a specific marketplace URL.
    pub fn for_marketplace(base_url: impl Into<String>) -> Self {
        let config = OsintClientConfig {
            base_url: base_url.into(),
            ..Default::default()
        };
        Self::with_config(config)
    }

    /// Get the base URL.
    pub fn base_url(&self) -> &str {
        &self.config.base_url
    }

    /// Check if authenticated.
    pub fn is_authenticated(&self) -> bool {
        self.auth_token.is_some()
    }

    /// Get authentication challenge.
    pub async fn get_auth_challenge(&self, wallet: &Pubkey) -> Result<AuthChallenge> {
        let url = format!("{}/api/auth/challenge", self.config.base_url);

        let response = self
            .http_client
            .post(&url)
            .json(&serde_json::json!({ "wallet": wallet.to_string() }))
            .send()
            .await
            .map_err(|e| SolanaError::DeFiProtocolError(format!("Auth challenge failed: {}", e)))?;

        if !response.status().is_success() {
            let error = response.text().await.unwrap_or_default();
            return Err(SolanaError::DeFiProtocolError(format!(
                "Auth challenge failed: {}",
                error
            )));
        }

        response
            .json()
            .await
            .map_err(|e| SolanaError::DeFiProtocolError(format!("Parse error: {}", e)))
    }

    /// Authenticate with wallet signature.
    pub async fn authenticate(&mut self, keypair: &Keypair) -> Result<AuthToken> {
        let wallet = keypair.pubkey();

        // Get challenge
        let challenge = self.get_auth_challenge(&wallet).await?;

        // Sign challenge
        let signature = keypair.sign_message(challenge.challenge.as_bytes());
        let signature_b58 = bs58::encode(signature.as_ref()).into_string();

        // Submit signature
        let url = format!("{}/api/auth/verify", self.config.base_url);

        let response = self
            .http_client
            .post(&url)
            .json(&serde_json::json!({
                "wallet": wallet.to_string(),
                "challenge_id": challenge.challenge_id,
                "signature": signature_b58,
            }))
            .send()
            .await
            .map_err(|e| SolanaError::DeFiProtocolError(format!("Auth verify failed: {}", e)))?;

        if !response.status().is_success() {
            let error = response.text().await.unwrap_or_default();
            return Err(SolanaError::DeFiProtocolError(format!(
                "Auth verify failed: {}",
                error
            )));
        }

        let token: AuthToken = response
            .json()
            .await
            .map_err(|e| SolanaError::DeFiProtocolError(format!("Parse error: {}", e)))?;

        self.auth_token = Some(token.clone());

        info!(wallet = %wallet, "Authenticated with OSINT marketplace");

        Ok(token)
    }

    /// Set authentication token directly.
    pub fn set_auth_token(&mut self, token: AuthToken) {
        self.auth_token = Some(token);
    }

    /// List open bounties.
    pub async fn list_bounties(
        &self,
        page: u32,
        per_page: u32,
    ) -> Result<PaginatedResponse<Bounty>> {
        let url = format!(
            "{}/api/bounties?page={}&per_page={}&status=open",
            self.config.base_url, page, per_page
        );

        let response =
            self.http_client.get(&url).send().await.map_err(|e| {
                SolanaError::DeFiProtocolError(format!("List bounties failed: {}", e))
            })?;

        if !response.status().is_success() {
            let error = response.text().await.unwrap_or_default();
            return Err(SolanaError::DeFiProtocolError(format!(
                "List bounties failed: {}",
                error
            )));
        }

        response
            .json()
            .await
            .map_err(|e| SolanaError::DeFiProtocolError(format!("Parse error: {}", e)))
    }

    /// Get a specific bounty.
    pub async fn get_bounty(&self, id: Uuid) -> Result<Bounty> {
        let url = format!("{}/api/bounties/{}", self.config.base_url, id);

        let response = self
            .http_client
            .get(&url)
            .send()
            .await
            .map_err(|e| SolanaError::DeFiProtocolError(format!("Get bounty failed: {}", e)))?;

        if !response.status().is_success() {
            let error = response.text().await.unwrap_or_default();
            return Err(SolanaError::DeFiProtocolError(format!(
                "Get bounty failed: {}",
                error
            )));
        }

        response
            .json()
            .await
            .map_err(|e| SolanaError::DeFiProtocolError(format!("Parse error: {}", e)))
    }

    /// Create a new bounty (requires authentication).
    pub async fn create_bounty(&self, request: CreateBountyRequest) -> Result<Bounty> {
        let token = self.require_auth()?;
        let url = format!("{}/api/bounties", self.config.base_url);

        let response = self
            .http_client
            .post(&url)
            .bearer_auth(&token.token)
            .json(&request)
            .send()
            .await
            .map_err(|e| SolanaError::DeFiProtocolError(format!("Create bounty failed: {}", e)))?;

        if !response.status().is_success() {
            let error = response.text().await.unwrap_or_default();
            return Err(SolanaError::DeFiProtocolError(format!(
                "Create bounty failed: {}",
                error
            )));
        }

        response
            .json()
            .await
            .map_err(|e| SolanaError::DeFiProtocolError(format!("Parse error: {}", e)))
    }

    /// Claim a bounty (requires authentication).
    pub async fn claim_bounty(&self, bounty_id: Uuid) -> Result<()> {
        let token = self.require_auth()?;
        let url = format!("{}/api/bounties/{}/claim", self.config.base_url, bounty_id);

        let response = self
            .http_client
            .post(&url)
            .bearer_auth(&token.token)
            .send()
            .await
            .map_err(|e| SolanaError::DeFiProtocolError(format!("Claim bounty failed: {}", e)))?;

        if !response.status().is_success() {
            let error = response.text().await.unwrap_or_default();
            return Err(SolanaError::DeFiProtocolError(format!(
                "Claim bounty failed: {}",
                error
            )));
        }

        info!(bounty_id = %bounty_id, "Bounty claimed");

        Ok(())
    }

    /// Submit findings for a bounty (requires authentication).
    pub async fn submit_findings(
        &self,
        bounty_id: Uuid,
        request: SubmitFindingsRequest,
    ) -> Result<Submission> {
        let token = self.require_auth()?;
        let url = format!("{}/api/bounties/{}/submit", self.config.base_url, bounty_id);

        let response = self
            .http_client
            .post(&url)
            .bearer_auth(&token.token)
            .json(&request)
            .send()
            .await
            .map_err(|e| SolanaError::DeFiProtocolError(format!("Submit failed: {}", e)))?;

        if !response.status().is_success() {
            let error = response.text().await.unwrap_or_default();
            return Err(SolanaError::DeFiProtocolError(format!(
                "Submit failed: {}",
                error
            )));
        }

        info!(bounty_id = %bounty_id, "Findings submitted");

        response
            .json()
            .await
            .map_err(|e| SolanaError::DeFiProtocolError(format!("Parse error: {}", e)))
    }

    /// Get marketplace statistics.
    pub async fn get_stats(&self) -> Result<MarketplaceStats> {
        let url = format!("{}/api/stats", self.config.base_url);

        let response = self
            .http_client
            .get(&url)
            .send()
            .await
            .map_err(|e| SolanaError::DeFiProtocolError(format!("Get stats failed: {}", e)))?;

        if !response.status().is_success() {
            let error = response.text().await.unwrap_or_default();
            return Err(SolanaError::DeFiProtocolError(format!(
                "Get stats failed: {}",
                error
            )));
        }

        response
            .json()
            .await
            .map_err(|e| SolanaError::DeFiProtocolError(format!("Parse error: {}", e)))
    }

    /// Get leaderboard.
    pub async fn get_leaderboard(&self, limit: u32) -> Result<Vec<LeaderboardEntry>> {
        let url = format!("{}/api/leaderboard?limit={}", self.config.base_url, limit);

        let response = self.http_client.get(&url).send().await.map_err(|e| {
            SolanaError::DeFiProtocolError(format!("Get leaderboard failed: {}", e))
        })?;

        if !response.status().is_success() {
            let error = response.text().await.unwrap_or_default();
            return Err(SolanaError::DeFiProtocolError(format!(
                "Get leaderboard failed: {}",
                error
            )));
        }

        response
            .json()
            .await
            .map_err(|e| SolanaError::DeFiProtocolError(format!("Parse error: {}", e)))
    }

    /// Get agent profile.
    pub async fn get_agent_profile(&self, wallet: &Pubkey) -> Result<AgentProfile> {
        let url = format!("{}/api/agents/{}", self.config.base_url, wallet.to_string());

        let response = self
            .http_client
            .get(&url)
            .send()
            .await
            .map_err(|e| SolanaError::DeFiProtocolError(format!("Get agent failed: {}", e)))?;

        if !response.status().is_success() {
            let error = response.text().await.unwrap_or_default();
            return Err(SolanaError::DeFiProtocolError(format!(
                "Get agent failed: {}",
                error
            )));
        }

        response
            .json()
            .await
            .map_err(|e| SolanaError::DeFiProtocolError(format!("Parse error: {}", e)))
    }

    /// Get escrow deposit instructions.
    pub async fn get_deposit_instructions(
        &self,
        amount: f64,
        token: &str,
    ) -> Result<DepositInstructions> {
        let url = format!("{}/api/escrow/instructions", self.config.base_url);

        let response = self
            .http_client
            .post(&url)
            .json(&serde_json::json!({
                "amount": amount,
                "token": token,
            }))
            .send()
            .await
            .map_err(|e| {
                SolanaError::DeFiProtocolError(format!("Get deposit instructions failed: {}", e))
            })?;

        if !response.status().is_success() {
            let error = response.text().await.unwrap_or_default();
            return Err(SolanaError::DeFiProtocolError(format!(
                "Get deposit instructions failed: {}",
                error
            )));
        }

        response
            .json()
            .await
            .map_err(|e| SolanaError::DeFiProtocolError(format!("Parse error: {}", e)))
    }

    /// Check API health.
    pub async fn health_check(&self) -> Result<bool> {
        let url = format!("{}/api/health", self.config.base_url);

        match self.http_client.get(&url).send().await {
            Ok(response) => Ok(response.status().is_success()),
            Err(_) => Ok(false),
        }
    }

    /// Require authentication.
    fn require_auth(&self) -> Result<&AuthToken> {
        self.auth_token
            .as_ref()
            .ok_or_else(|| SolanaError::DeFiProtocolError("Not authenticated".to_string()))
    }
}

impl Default for OsintClient {
    fn default() -> Self {
        Self::new()
    }
}

/// Deposit instructions from the API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepositInstructions {
    /// Escrow wallet address.
    pub escrow_wallet: String,
    /// Total amount to deposit.
    pub total_amount: f64,
    /// Fee amount.
    pub fee_amount: f64,
    /// Net amount after fee.
    pub net_amount: f64,
    /// Token type.
    pub token: String,
    /// Transaction memo.
    pub memo: String,
}

/// Agent specification for discovery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSpec {
    /// Spec version.
    pub version: String,
    /// Agent name.
    pub name: String,
    /// Agent description.
    pub description: String,
    /// Supported capabilities.
    pub capabilities: Vec<String>,
    /// API endpoints.
    pub endpoints: AgentEndpoints,
    /// Authentication methods.
    pub auth: AgentAuth,
}

/// Agent API endpoints.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentEndpoints {
    /// List bounties endpoint.
    pub list_bounties: String,
    /// Get bounty endpoint pattern.
    pub get_bounty: String,
    /// Claim bounty endpoint pattern.
    pub claim_bounty: String,
    /// Submit findings endpoint pattern.
    pub submit_findings: String,
    /// Auth challenge endpoint.
    pub auth_challenge: String,
    /// Auth verify endpoint.
    pub auth_verify: String,
}

/// Agent authentication specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentAuth {
    /// Auth method (e.g., "solana-wallet").
    pub method: String,
    /// Signature algorithm.
    pub algorithm: String,
    /// Challenge message format.
    pub challenge_format: String,
}

impl OsintClient {
    /// Get agent specification from a marketplace.
    pub async fn get_agent_spec(&self) -> Result<AgentSpec> {
        let url = format!("{}/.well-known/agent.json", self.config.base_url);

        let response =
            self.http_client.get(&url).send().await.map_err(|e| {
                SolanaError::DeFiProtocolError(format!("Get agent spec failed: {}", e))
            })?;

        if !response.status().is_success() {
            // Return default spec
            return Ok(AgentSpec {
                version: "1.0".to_string(),
                name: "OSINT Marketplace".to_string(),
                description: "Decentralized OSINT bounty marketplace on Solana".to_string(),
                capabilities: vec![
                    "list_bounties".to_string(),
                    "claim_bounties".to_string(),
                    "submit_findings".to_string(),
                ],
                endpoints: AgentEndpoints {
                    list_bounties: "/api/bounties".to_string(),
                    get_bounty: "/api/bounties/{id}".to_string(),
                    claim_bounty: "/api/bounties/{id}/claim".to_string(),
                    submit_findings: "/api/bounties/{id}/submit".to_string(),
                    auth_challenge: "/api/auth/challenge".to_string(),
                    auth_verify: "/api/auth/verify".to_string(),
                },
                auth: AgentAuth {
                    method: "solana-wallet".to_string(),
                    algorithm: "ed25519".to_string(),
                    challenge_format: "Sign this message to authenticate: {nonce}".to_string(),
                },
            });
        }

        response
            .json()
            .await
            .map_err(|e| SolanaError::DeFiProtocolError(format!("Parse error: {}", e)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_creation() {
        let client = OsintClient::new();
        assert_eq!(client.base_url(), "https://osint.market");
        assert!(!client.is_authenticated());
    }

    #[test]
    fn test_custom_config() {
        let config = OsintClientConfig {
            base_url: "https://test.osint.market".to_string(),
            timeout_secs: 60,
            max_retries: 5,
        };

        let client = OsintClient::with_config(config);
        assert_eq!(client.base_url(), "https://test.osint.market");
    }

    #[test]
    fn test_evidence_conversion() {
        let evidence = Evidence::url("https://example.com", Some("Test note".to_string()));

        let item: EvidenceItem = (&evidence).into();
        assert_eq!(item.evidence_type, "url");
        assert_eq!(item.content, "https://example.com");
        assert_eq!(item.note, Some("Test note".to_string()));
    }
}
