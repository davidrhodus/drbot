//! SAID Protocol API client.
//!
//! SAID is a Solana Agent Identity Protocol. This module provides a small
//! Rust client for the public SAID API (typically served at
//! `https://api.saidprotocol.com`).
//!
//! The upstream project referenced by the user was `solana-clawd/said-api`.
//! That repository was not reachable from this environment, so this client is
//! implemented from the public API contract documented on saidprotocol.com.

use crate::{Result, SolanaError};
use chrono::{DateTime, Utc};
use reqwest::Client;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use tracing::{debug, trace};

/// Default SAID API base URL.
pub const SAID_API_BASE_URL: &str = "https://api.saidprotocol.com";

/// SAID API client.
#[derive(Debug, Clone)]
pub struct SaidApiClient {
    client: Client,
    base_url: String,
}

impl SaidApiClient {
    /// Create a new SAID API client using the default base URL.
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            base_url: SAID_API_BASE_URL.to_string(),
        }
    }

    /// Create a new SAID API client using a custom base URL.
    pub fn with_url(url: impl Into<String>) -> Self {
        Self {
            client: Client::new(),
            base_url: url.into(),
        }
    }

    fn build_url(&self, path: &str) -> String {
        let base = self.base_url.trim_end_matches('/');
        let path = path.trim_start_matches('/');
        format!("{}/{}", base, path)
    }

    async fn get_json<T: DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, String)],
    ) -> Result<T> {
        let url = self.build_url(path);
        trace!(%url, "SAID API request");

        let response = self.client.get(&url).query(query).send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(SolanaError::DeFiProtocolError(format!(
                "SAID API error ({}): {}",
                status, body
            )));
        }

        Ok(response.json::<T>().await?)
    }

    /// Discover agents by type and/or skills.
    pub async fn discover(&self, request: DiscoverRequest) -> Result<DiscoverResponse> {
        let mut query: Vec<(&str, String)> = Vec::new();

        if let Some(agent_type) = request.agent_type.as_deref() {
            query.push(("type", agent_type.to_string()));
        }

        if !request.skills.is_empty() {
            query.push(("skills", request.skills.join(",")));
        }

        if let Some(limit) = request.limit {
            query.push(("limit", limit.to_string()));
        }

        if let Some(page) = request.page {
            query.push(("page", page.to_string()));
        }

        debug!(
            agent_type = request.agent_type.as_deref().unwrap_or(""),
            skills = ?request.skills,
            limit = ?request.limit,
            page = ?request.page,
            "Discovering SAID agents"
        );

        self.get_json("/discover", &query).await
    }

    /// Verify an agent by wallet address.
    pub async fn verify(&self, wallet: &str) -> Result<VerifyResponse> {
        let query = vec![("wallet", wallet.to_string())];
        self.get_json("/verify", &query).await
    }

    /// Get verification history for a wallet address.
    pub async fn history(&self, wallet: &str, limit: Option<u32>) -> Result<HistoryResponse> {
        let mut query = vec![("wallet", wallet.to_string())];
        if let Some(limit) = limit {
            query.push(("limit", limit.to_string()));
        }
        self.get_json("/history", &query).await
    }

    /// Get protocol statistics.
    pub async fn stats(&self) -> Result<StatsResponse> {
        self.get_json("/stats", &[]).await
    }

    /// List active agents.
    pub async fn active_agents(&self, limit: Option<u32>) -> Result<ActiveAgentsResponse> {
        let mut query: Vec<(&str, String)> = Vec::new();
        if let Some(limit) = limit {
            query.push(("limit", limit.to_string()));
        }
        self.get_json("/agents/active", &query).await
    }

    /// List available skills.
    pub async fn skills(&self) -> Result<SkillsResponse> {
        self.get_json("/skills", &[]).await
    }

    /// List available agent types.
    pub async fn types(&self) -> Result<TypesResponse> {
        self.get_json("/types", &[]).await
    }

    /// Low-level helper for endpoints we haven't modeled yet.
    pub async fn get_raw(&self, path: &str, query: &[(&str, String)]) -> Result<Value> {
        self.get_json(path, query).await
    }
}

impl Default for SaidApiClient {
    fn default() -> Self {
        Self::new()
    }
}

/// Request options for `/discover`.
#[derive(Debug, Clone, Default)]
pub struct DiscoverRequest {
    /// Agent type filter (e.g. `trading`).
    pub agent_type: Option<String>,
    /// Skill filters (sent as a comma-separated list).
    pub skills: Vec<String>,
    /// Maximum number of agents to return.
    pub limit: Option<u32>,
    /// Page number (1-based).
    pub page: Option<u32>,
}

/// A SAID agent record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRecord {
    /// Wallet address (base58).
    pub wallet: String,
    /// Agent type.
    #[serde(rename = "type")]
    pub agent_type: String,
    /// Skills list.
    #[serde(default)]
    pub skills: Vec<String>,
    /// Reputation score (0-100, typically).
    #[serde(default)]
    pub reputation: Option<i64>,
    /// Verification timestamp.
    #[serde(default)]
    pub verified_at: Option<DateTime<Utc>>,
    /// Last active timestamp.
    #[serde(default)]
    pub last_active: Option<DateTime<Utc>>,
    /// Any additional fields returned by the API.
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

/// Response from `/discover`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoverResponse {
    #[serde(default)]
    pub agents: Vec<AgentRecord>,
    #[serde(default)]
    pub total: Option<u64>,
    #[serde(default)]
    pub page: Option<u32>,
    #[serde(default)]
    pub limit: Option<u32>,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

/// Response from `/verify`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyResponse {
    pub wallet: String,
    #[serde(default)]
    pub verified: bool,
    #[serde(rename = "type", default)]
    pub agent_type: Option<String>,
    #[serde(default)]
    pub skills: Option<Vec<String>>,
    #[serde(default)]
    pub reputation: Option<i64>,
    #[serde(default)]
    pub verified_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub last_active: Option<DateTime<Utc>>,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

/// Response from `/history`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryResponse {
    #[serde(default)]
    pub wallet: Option<String>,
    #[serde(default)]
    pub history: Vec<HistoryEntry>,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

/// History entry for a wallet.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    #[serde(default)]
    pub event: Option<String>,
    #[serde(default)]
    pub timestamp: Option<DateTime<Utc>>,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

/// Response from `/stats`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatsResponse {
    #[serde(flatten)]
    pub data: HashMap<String, Value>,
}

/// Response from `/agents/active`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveAgentsResponse {
    #[serde(default)]
    pub agents: Vec<AgentRecord>,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

/// Response from `/skills`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillsResponse {
    #[serde(default)]
    pub skills: Vec<String>,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

/// Response from `/types`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypesResponse {
    #[serde(default)]
    pub types: Vec<String>,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_url_joins_cleanly() {
        let client = SaidApiClient::with_url("https://api.saidprotocol.com/".to_string());
        assert_eq!(
            client.build_url("/discover"),
            "https://api.saidprotocol.com/discover"
        );
    }

    #[test]
    fn test_discover_response_deserialize_from_docs_example() {
        let json = r#"
        {
          "agents": [
            {
              "wallet": "AbCd...EfGh",
              "type": "trading",
              "skills": ["solana", "defi", "arbitrage"],
              "reputation": 85,
              "verified_at": "2024-01-15T10:30:00Z",
              "last_active": "2024-06-15T14:22:00Z"
            }
          ],
          "total": 156,
          "page": 1,
          "limit": 10
        }
        "#;

        let parsed: DiscoverResponse = serde_json::from_str(json).expect("valid json");
        assert_eq!(parsed.agents.len(), 1);
        assert_eq!(parsed.agents[0].agent_type, "trading");
        assert_eq!(parsed.total, Some(156));
        assert_eq!(parsed.page, Some(1));
        assert_eq!(parsed.limit, Some(10));
    }

    #[test]
    fn test_verify_response_is_lenient() {
        let json = r#"
        {
          "wallet": "AbCd...EfGh",
          "verified": true,
          "type": "trading",
          "skills": ["solana"],
          "verified_at": "2024-01-15T10:30:00Z"
        }
        "#;

        let parsed: VerifyResponse = serde_json::from_str(json).expect("valid json");
        assert!(parsed.verified);
        assert_eq!(parsed.agent_type.as_deref(), Some("trading"));
        assert_eq!(
            parsed.skills.unwrap_or_default(),
            vec!["solana".to_string()]
        );
    }
}
