//! OTC negotiation skill for agent-to-agent trading.

use crate::otc::{
    EscrowParty, Negotiation, NegotiationState, OTCEnvelope, OTCMessage, OTCNegotiationManager,
    TradeDirection,
};
use crate::{Result, SolanaError};
use async_trait::async_trait;
use drbot_skills::{
    ManifestCapability, ManifestInput, ManifestOutput, Skill, SkillContext, SkillInput,
    SkillManifest, SkillOutput,
};
use serde::{Deserialize, Serialize};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;
use std::sync::Arc;

/// OTC negotiation skill for peer-to-peer trading.
pub struct OTCSkill {
    manifest: SkillManifest,
    manager: Arc<OTCNegotiationManager>,
}

impl OTCSkill {
    /// Create a new OTC skill.
    pub fn new(rpc_client: Arc<RpcClient>) -> Self {
        let manifest = SkillManifest {
            name: "otc".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            description: "Agent-to-agent OTC negotiation for peer-to-peer trading".to_string(),
            author: Some("drbot".to_string()),
            license: Some("MIT".to_string()),
            homepage: None,
            repository: None,
            tags: vec!["solana".to_string(), "otc".to_string(), "p2p".to_string()],
            inputs: vec![
                ManifestInput {
                    name: "action".to_string(),
                    description: "Action: rfq, quote, accept, reject, status, history".to_string(),
                    param_type: "string".to_string(),
                    required: true,
                    default: None,
                    pattern: None,
                    enum_values: vec![],
                },
                ManifestInput {
                    name: "asset".to_string(),
                    description: "Asset to trade".to_string(),
                    param_type: "string".to_string(),
                    required: false,
                    default: None,
                    pattern: None,
                    enum_values: vec![],
                },
                ManifestInput {
                    name: "direction".to_string(),
                    description: "Trade direction: buy or sell".to_string(),
                    param_type: "string".to_string(),
                    required: false,
                    default: None,
                    pattern: None,
                    enum_values: vec![],
                },
                ManifestInput {
                    name: "amount".to_string(),
                    description: "Amount to trade".to_string(),
                    param_type: "number".to_string(),
                    required: false,
                    default: None,
                    pattern: None,
                    enum_values: vec![],
                },
                ManifestInput {
                    name: "price".to_string(),
                    description: "Price for quote".to_string(),
                    param_type: "number".to_string(),
                    required: false,
                    default: None,
                    pattern: None,
                    enum_values: vec![],
                },
                ManifestInput {
                    name: "rfq_id".to_string(),
                    description: "RFQ ID to respond to".to_string(),
                    param_type: "string".to_string(),
                    required: false,
                    default: None,
                    pattern: None,
                    enum_values: vec![],
                },
                ManifestInput {
                    name: "quote_id".to_string(),
                    description: "Quote ID to accept/reject".to_string(),
                    param_type: "string".to_string(),
                    required: false,
                    default: None,
                    pattern: None,
                    enum_values: vec![],
                },
            ],
            outputs: vec![ManifestOutput {
                name: "result".to_string(),
                description: "Operation result".to_string(),
                output_type: "object".to_string(),
            }],
            capabilities: vec![
                ManifestCapability::required("otc"),
                ManifestCapability::required("trading"),
            ],
            entry_point: None,
            runtime: None,
        };

        Self {
            manifest,
            manager: Arc::new(OTCNegotiationManager::new(rpc_client)),
        }
    }
}

#[async_trait]
impl Skill for OTCSkill {
    fn manifest(&self) -> &SkillManifest {
        &self.manifest
    }

    fn validate_input(&self, input: &SkillInput) -> drbot_skills::Result<()> {
        let action = input
            .params
            .get("action")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                drbot_skills::SkillError::ValidationFailed("Missing action".to_string())
            })?;

        match action {
            "rfq" => {
                if input.params.get("asset").and_then(|v| v.as_str()).is_none() {
                    return Err(drbot_skills::SkillError::ValidationFailed(
                        "Asset required for RFQ".to_string(),
                    ));
                }
                if input
                    .params
                    .get("amount")
                    .and_then(|v| v.as_f64())
                    .is_none()
                {
                    return Err(drbot_skills::SkillError::ValidationFailed(
                        "Amount required for RFQ".to_string(),
                    ));
                }
                Ok(())
            }
            "quote" => {
                if input
                    .params
                    .get("rfq_id")
                    .and_then(|v| v.as_str())
                    .is_none()
                {
                    return Err(drbot_skills::SkillError::ValidationFailed(
                        "RFQ ID required for quote".to_string(),
                    ));
                }
                if input.params.get("price").and_then(|v| v.as_f64()).is_none() {
                    return Err(drbot_skills::SkillError::ValidationFailed(
                        "Price required for quote".to_string(),
                    ));
                }
                Ok(())
            }
            "accept" | "reject" => {
                if input
                    .params
                    .get("quote_id")
                    .and_then(|v| v.as_str())
                    .is_none()
                {
                    return Err(drbot_skills::SkillError::ValidationFailed(
                        "Quote ID required".to_string(),
                    ));
                }
                Ok(())
            }
            "status" | "history" => Ok(()),
            _ => Err(drbot_skills::SkillError::ValidationFailed(format!(
                "Unknown action: {}",
                action
            ))),
        }
    }

    async fn execute(
        &self,
        input: SkillInput,
        _context: &SkillContext,
    ) -> drbot_skills::Result<SkillOutput> {
        let action = input
            .params
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("status");

        match action {
            "rfq" => self.handle_rfq(&input).await,
            "quote" => self.handle_quote(&input).await,
            "accept" => self.handle_accept(&input).await,
            "reject" => self.handle_reject(&input).await,
            "status" => self.handle_status(&input).await,
            "history" => self.handle_history().await,
            _ => Err(drbot_skills::SkillError::ExecutionFailed(format!(
                "Action '{}' not implemented",
                action
            ))),
        }
    }
}

impl OTCSkill {
    async fn handle_rfq(&self, input: &SkillInput) -> drbot_skills::Result<SkillOutput> {
        let asset = input.params.get("asset").and_then(|v| v.as_str()).unwrap();
        let asset_mint = Pubkey::default();

        let direction = match input
            .params
            .get("direction")
            .and_then(|v| v.as_str())
            .unwrap_or("buy")
        {
            "sell" => TradeDirection::Sell,
            _ => TradeDirection::Buy,
        };

        let amount = input.params.get("amount").and_then(|v| v.as_f64()).unwrap();
        let amount_units = (amount * 1e9) as u64;

        let negotiation = self
            .manager
            .create_rfq(asset, asset_mint, direction, amount_units)
            .await
            .map_err(|e| drbot_skills::SkillError::ExecutionFailed(e.to_string()))?;

        Ok(SkillOutput::new(serde_json::json!({
            "id": negotiation.id.to_string(),
            "state": format!("{:?}", negotiation.state),
            "role": format!("{:?}", negotiation.our_role),
            "created_at": negotiation.created_at.to_rfc3339(),
        })))
    }

    async fn handle_quote(&self, input: &SkillInput) -> drbot_skills::Result<SkillOutput> {
        let rfq_id = input.params.get("rfq_id").and_then(|v| v.as_str()).unwrap();
        let rfq_uuid = uuid::Uuid::parse_str(rfq_id)
            .map_err(|e| drbot_skills::SkillError::ValidationFailed(e.to_string()))?;

        let price = input.params.get("price").and_then(|v| v.as_f64()).unwrap();
        let quantity = input
            .params
            .get("amount")
            .and_then(|v| v.as_f64())
            .map(|a| (a * 1e9) as u64)
            .unwrap_or(0);
        let settlement_asset = "USDC";
        let settlement_mint = Pubkey::default();

        let quote = self
            .manager
            .create_quote(
                rfq_uuid,
                price,
                quantity,
                settlement_asset,
                settlement_mint,
                120,
            )
            .await
            .map_err(|e| drbot_skills::SkillError::ExecutionFailed(e.to_string()))?;

        Ok(SkillOutput::new(serde_json::json!({
            "message_type": quote.message_type(),
        })))
    }

    async fn handle_accept(&self, input: &SkillInput) -> drbot_skills::Result<SkillOutput> {
        let quote_id = input
            .params
            .get("quote_id")
            .and_then(|v| v.as_str())
            .unwrap();
        let quote_uuid = uuid::Uuid::parse_str(quote_id)
            .map_err(|e| drbot_skills::SkillError::ValidationFailed(e.to_string()))?;

        let _accept = self
            .manager
            .accept_quote(quote_uuid)
            .await
            .map_err(|e| drbot_skills::SkillError::ExecutionFailed(e.to_string()))?;

        Ok(SkillOutput::new(serde_json::json!({
            "status": "accepted",
            "quote_id": quote_id,
        })))
    }

    async fn handle_reject(&self, input: &SkillInput) -> drbot_skills::Result<SkillOutput> {
        let quote_id = input
            .params
            .get("quote_id")
            .and_then(|v| v.as_str())
            .unwrap();
        let quote_uuid = uuid::Uuid::parse_str(quote_id)
            .map_err(|e| drbot_skills::SkillError::ValidationFailed(e.to_string()))?;

        let reason = input
            .params
            .get("reason")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let _reject = self
            .manager
            .reject_quote(quote_uuid, reason)
            .await
            .map_err(|e| drbot_skills::SkillError::ExecutionFailed(e.to_string()))?;

        Ok(SkillOutput::new(serde_json::json!({
            "status": "rejected",
            "quote_id": quote_id,
        })))
    }

    async fn handle_status(&self, input: &SkillInput) -> drbot_skills::Result<SkillOutput> {
        if let Some(neg_id) = input.params.get("negotiation_id").and_then(|v| v.as_str()) {
            let neg_uuid = uuid::Uuid::parse_str(neg_id)
                .map_err(|e| drbot_skills::SkillError::ValidationFailed(e.to_string()))?;

            let negotiation = self.manager.get_negotiation(neg_uuid).await;

            match negotiation {
                Some(neg) => Ok(SkillOutput::new(serde_json::json!({
                    "id": neg.id.to_string(),
                    "state": format!("{:?}", neg.state),
                    "role": format!("{:?}", neg.our_role),
                    "counterparty": neg.counterparty,
                    "quotes_count": neg.quotes.len(),
                    "created_at": neg.created_at.to_rfc3339(),
                    "updated_at": neg.updated_at.to_rfc3339(),
                }))),
                None => Err(drbot_skills::SkillError::ExecutionFailed(
                    "Negotiation not found".to_string(),
                )),
            }
        } else {
            let active = self.manager.get_active_negotiations().await;
            let output: Vec<_> = active
                .iter()
                .map(|neg| {
                    serde_json::json!({
                        "id": neg.id.to_string(),
                        "state": format!("{:?}", neg.state),
                        "role": format!("{:?}", neg.our_role),
                        "counterparty": neg.counterparty,
                        "quotes_count": neg.quotes.len(),
                        "created_at": neg.created_at.to_rfc3339(),
                    })
                })
                .collect();

            Ok(SkillOutput::new(output))
        }
    }

    async fn handle_history(&self) -> drbot_skills::Result<SkillOutput> {
        let history = self.manager.get_history().await;
        let output: Vec<_> = history
            .iter()
            .map(|neg| {
                serde_json::json!({
                    "id": neg.id.to_string(),
                    "state": format!("{:?}", neg.state),
                    "role": format!("{:?}", neg.our_role),
                    "counterparty": neg.counterparty,
                    "quotes_count": neg.quotes.len(),
                    "created_at": neg.created_at.to_rfc3339(),
                    "updated_at": neg.updated_at.to_rfc3339(),
                })
            })
            .collect();

        Ok(SkillOutput::new(output))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_otc_skill_manifest() {
        let rpc = Arc::new(RpcClient::new("https://api.devnet.solana.com".to_string()));
        let skill = OTCSkill::new(rpc);
        let manifest = skill.manifest();

        assert_eq!(manifest.name, "otc");
    }
}
