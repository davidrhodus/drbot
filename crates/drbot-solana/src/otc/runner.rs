//! Settlement runners for OTC negotiation over A2A.
//!
//! This module contains reusable “glue” for turning OTC negotiation messages into on-chain
//! settlement actions using the `drbot-otc-escrow-program`.

use super::a2a::{otc_notification, parse_otc_envelope};
use super::escrow::EscrowManager;
use super::negotiation::OTCNegotiationManager;
use super::protocol::{EscrowParty, OTCEnvelope, OTCMessage};
use super::settlement::build_escrow_params;
use crate::Result;
use chrono::Utc;
use drbot_a2a::{A2AHub, A2AMessage};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signer::keypair::Keypair;
use solana_sdk::signer::Signer;
use std::collections::{HashSet, VecDeque};
use std::hash::Hash;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use uuid::Uuid;

const DEFAULT_ACCEPT_REPLAY_TTL: Duration = Duration::from_secs(60 * 10);
const DEFAULT_ESCROW_CLOSE_TIMEOUT: Duration = Duration::from_secs(2);
const DEFAULT_ESCROW_CLOSE_POLL_INTERVAL: Duration = Duration::from_millis(250);
const DEFAULT_RECONCILE_INTERVAL: Duration = Duration::from_secs(10);

struct ReplayCache<K> {
    ttl: Duration,
    order: VecDeque<(tokio::time::Instant, K)>,
    set: HashSet<K>,
}

impl<K> ReplayCache<K>
where
    K: Eq + Hash + Clone,
{
    fn new(ttl: Duration) -> Self {
        Self {
            ttl,
            order: VecDeque::new(),
            set: HashSet::new(),
        }
    }

    fn insert_if_new(&mut self, key: K) -> bool {
        self.evict_expired();
        if self.set.contains(&key) {
            return false;
        }
        self.set.insert(key.clone());
        self.order.push_back((tokio::time::Instant::now(), key));
        true
    }

    fn evict_expired(&mut self) {
        let now = tokio::time::Instant::now();
        while let Some((t, _)) = self.order.front() {
            if now.duration_since(*t) <= self.ttl {
                break;
            }
            let (_, key) = self.order.pop_front().expect("front exists");
            self.set.remove(&key);
        }
    }
}

/// Spawn a settlement listener for an OTC desk.
///
/// The desk:
/// - listens for signed `Accept` messages addressed to `desk_agent_id`
/// - (optional fast path) reacts to signed `EscrowFunded(PartyA)` nudges
/// - resolves the accepted quote context from `negotiation_manager`
/// - creates/verifies escrow (optional; open-network safe default is to require Party A to create)
/// - waits for Party A to fund first
/// - funds Party B's leg (second fund auto-settles)
/// - notifies the trader with a signed `EscrowFunded` or `Settled` update
pub fn spawn_desk_settlement_service(
    hub: Arc<A2AHub>,
    negotiation_manager: Arc<OTCNegotiationManager>,
    rpc_client: Arc<RpcClient>,
    escrow_program_id: Pubkey,
    desk_agent_id: Uuid,
    desk_keypair: Keypair,
    fee_payer_keypair: Option<Keypair>,
    create_escrow_if_missing: bool,
) -> JoinHandle<()> {
    let escrow_manager =
        Arc::new(EscrowManager::new(rpc_client).with_program_id(escrow_program_id));
    let desk_keypair = Arc::new(Mutex::new(desk_keypair));
    let fee_payer_keypair = fee_payer_keypair.map(|kp| Arc::new(Mutex::new(kp)));
    let create_escrow_if_missing = Arc::new(create_escrow_if_missing);

    tokio::spawn(async move {
        let mut rx = hub.subscribe();
        let mut replay = ReplayCache::<Uuid>::new(DEFAULT_ACCEPT_REPLAY_TTL);
        let in_flight: Arc<Mutex<HashSet<Uuid>>> = Arc::new(Mutex::new(HashSet::new()));
        let mut reconcile_tick = tokio::time::interval(DEFAULT_RECONCILE_INTERVAL);

        loop {
            tokio::select! {
                msg = rx.recv() => {
                    let msg = match msg {
                        Ok(m) => m,
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    };

                    if msg.to != desk_agent_id {
                        continue;
                    }

                    let Some(envelope) = parse_otc_envelope(&msg) else {
                        continue;
                    };

                    if envelope.verify_signature().ok() != Some(true) {
                        continue;
                    }

                    match envelope.message {
                        OTCMessage::Accept {
                            quote_id,
                            accepting_wallet,
                        } => {
                            if !replay.insert_if_new(quote_id) {
                                continue;
                            }

                            let hub = hub.clone();
                            let negotiation_manager = negotiation_manager.clone();
                            let escrow_manager = escrow_manager.clone();
                            let desk_keypair = desk_keypair.clone();
                            let fee_payer_keypair = fee_payer_keypair.clone();
                            let create_escrow_if_missing = create_escrow_if_missing.clone();
                            let in_flight = in_flight.clone();
                            tokio::spawn(async move {
                                if !claim_in_flight(&in_flight, quote_id).await {
                                    return;
                                }
                                process_desk_accept(
                                    hub,
                                    negotiation_manager,
                                    escrow_manager,
                                    desk_agent_id,
                                    desk_keypair,
                                    fee_payer_keypair,
                                    *create_escrow_if_missing,
                                    msg.from,
                                    quote_id,
                                    Some(accepting_wallet),
                                )
                                .await;
                                release_in_flight(&in_flight, quote_id).await;
                            });
                        }
                        OTCMessage::EscrowFunded {
                            negotiation_id,
                            funded_by: EscrowParty::PartyA,
                            ..
                        } => {
                            // Speed up open-network settlement: when Party A reports funding,
                            // trigger a re-check (on-chain is authoritative; this is just a nudge).
                            let Some(neg) = negotiation_manager.get_negotiation(negotiation_id).await
                            else {
                                continue;
                            };
                            let Some(quote_id) = neg.selected_quote else {
                                continue;
                            };

                            let hub = hub.clone();
                            let negotiation_manager = negotiation_manager.clone();
                            let escrow_manager = escrow_manager.clone();
                            let desk_keypair = desk_keypair.clone();
                            let fee_payer_keypair = fee_payer_keypair.clone();
                            let create_escrow_if_missing = create_escrow_if_missing.clone();
                            let in_flight = in_flight.clone();
                            tokio::spawn(async move {
                                if !claim_in_flight(&in_flight, quote_id).await {
                                    return;
                                }
                                process_desk_accept(
                                    hub,
                                    negotiation_manager,
                                    escrow_manager,
                                    desk_agent_id,
                                    desk_keypair,
                                    fee_payer_keypair,
                                    *create_escrow_if_missing,
                                    msg.from,
                                    quote_id,
                                    None,
                                )
                                .await;
                                release_in_flight(&in_flight, quote_id).await;
                            });
                        }
                        _ => continue,
                    }
                }
                _ = reconcile_tick.tick() => {
                    let negotiations = negotiation_manager.get_active_negotiations().await;
                    for neg in negotiations {
                        if neg.selected_quote.is_none() {
                            continue;
                        }
                        if neg.counterparty.is_none() {
                            continue;
                        }
                        if neg.state != super::negotiation::NegotiationState::Accepted
                            && neg.state != super::negotiation::NegotiationState::EscrowFunding
                        {
                            continue;
                        }

                        let Some(counterparty_agent_id) = neg
                            .counterparty
                            .as_deref()
                            .and_then(|s| Uuid::parse_str(s).ok())
                        else {
                            continue;
                        };

                        let quote_id = neg.selected_quote.expect("checked above");

                        let hub = hub.clone();
                        let negotiation_manager = negotiation_manager.clone();
                        let escrow_manager = escrow_manager.clone();
                        let desk_keypair = desk_keypair.clone();
                        let fee_payer_keypair = fee_payer_keypair.clone();
                        let create_escrow_if_missing = create_escrow_if_missing.clone();
                        let in_flight = in_flight.clone();
                        tokio::spawn(async move {
                            if !claim_in_flight(&in_flight, quote_id).await {
                                return;
                            }
                            process_desk_accept(
                                hub,
                                negotiation_manager,
                                escrow_manager,
                                desk_agent_id,
                                desk_keypair,
                                fee_payer_keypair,
                                *create_escrow_if_missing,
                                counterparty_agent_id,
                                quote_id,
                                None,
                            )
                            .await;
                            release_in_flight(&in_flight, quote_id).await;
                        });
                    }
                }
            }
        }
    })
}

async fn claim_in_flight(in_flight: &Arc<Mutex<HashSet<Uuid>>>, quote_id: Uuid) -> bool {
    let mut set = in_flight.lock().await;
    if set.contains(&quote_id) {
        return false;
    }
    set.insert(quote_id);
    true
}

async fn release_in_flight(in_flight: &Arc<Mutex<HashSet<Uuid>>>, quote_id: Uuid) {
    in_flight.lock().await.remove(&quote_id);
}

#[allow(clippy::too_many_arguments)]
async fn process_desk_accept(
    hub: Arc<A2AHub>,
    negotiation_manager: Arc<OTCNegotiationManager>,
    escrow_manager: Arc<EscrowManager>,
    desk_agent_id: Uuid,
    desk_keypair: Arc<Mutex<Keypair>>,
    fee_payer_keypair: Option<Arc<Mutex<Keypair>>>,
    create_escrow_if_missing: bool,
    counterparty_agent_id: Uuid,
    quote_id: Uuid,
    accepting_wallet: Option<Pubkey>,
) {
    let ctx = match negotiation_manager.quote_context(quote_id).await {
        Ok(ctx) => ctx,
        Err(e) => {
            tracing::warn!(error = %e, %quote_id, "Failed to resolve accepted quote");
            return;
        }
    };

    let params = match build_escrow_params(&ctx.rfq, &ctx.quote) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(error = %e, %quote_id, "Failed to build escrow params");
            return;
        }
    };

    // If we have an Accept payload, validate it matches the RFQ initiator.
    if let Some(accepting_wallet) = accepting_wallet {
        let expected_party_a = match &ctx.rfq {
            OTCMessage::Rfq {
                initiator_wallet, ..
            } => *initiator_wallet,
            _ => Pubkey::default(),
        };
        if accepting_wallet != expected_party_a {
            tracing::warn!(
                %accepting_wallet,
                %expected_party_a,
                "Accepting wallet does not match RFQ initiator"
            );
            return;
        }
    }

    // On-chain replay protection check (if receipt exists and is consumed, do nothing).
    if let Ok(Some(status)) = escrow_manager
        .get_receipt_status(params.negotiation_id, params.party_a, params.party_b)
        .await
    {
        if status != drbot_otc_escrow_program::ReceiptStatus::Open {
            return;
        }
    }

    // Create escrow only if it appears missing. (If it was already created, avoid paying fees.)
    let (escrow_address, _) = match escrow_manager.derive_address(
        params.negotiation_id,
        params.party_a,
        params.party_b,
    ) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, "Failed to derive escrow address");
            return;
        }
    };

    let mut maybe_escrow = match escrow_manager.try_get_escrow(&escrow_address).await {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, "Failed to read escrow account");
            None
        }
    };

    let desk_kp = desk_keypair.lock().await.insecure_clone();
    let fee_kp = match &fee_payer_keypair {
        Some(kp) => Some(kp.lock().await.insecure_clone()),
        None => None,
    };

    if maybe_escrow.is_none() {
        if !create_escrow_if_missing {
            // In open-network mode, require Party A to create the escrow to avoid desk fee griefing.
            return;
        }

        let create = match fee_kp.as_ref() {
            Some(fee_payer) => {
                escrow_manager
                    .create_escrow_with_fee_payer(fee_payer, &desk_kp, params.clone())
                    .await
            }
            None => escrow_manager.create_escrow(&desk_kp, params.clone()).await,
        };

        match create {
            Ok(_) => {
                negotiation_manager
                    .record_escrow_created(params.negotiation_id, escrow_address)
                    .await;
                maybe_escrow = escrow_manager
                    .try_get_escrow(&escrow_address)
                    .await
                    .unwrap_or(None);
            }
            Err(e) => {
                // If receipt became consumed between checks, ignore.
                if let Ok(Some(status)) = escrow_manager
                    .get_receipt_status(params.negotiation_id, params.party_a, params.party_b)
                    .await
                {
                    if status != drbot_otc_escrow_program::ReceiptStatus::Open {
                        return;
                    }
                }

                tracing::warn!(error = %e, "Create escrow failed");
                return;
            }
        }
    }

    let Some(existing) = maybe_escrow else {
        return;
    };

    negotiation_manager
        .record_escrow_created(params.negotiation_id, escrow_address)
        .await;

    let desired = params.to_terms();
    if existing.state.negotiation_id != desired.negotiation_id
        || existing.state.party_a != desired.party_a
        || existing.state.party_b != desired.party_b
        || existing.state.a_owes != desired.a_owes
        || existing.state.b_owes != desired.b_owes
        || existing.state.expiry_unix_ts != desired.expiry_unix_ts
    {
        tracing::warn!(%escrow_address, "On-chain escrow terms mismatch; refusing to fund");
        return;
    }

    // If escrow exists and our leg is already funded, do not re-fund; optionally re-notify.
    if existing.state.b_funded {
        // Best-effort: resend the last known funding signature if we have one.
        if let Some(sig) = existing_party_funding_signature(
            &ctx.negotiation_id,
            &negotiation_manager,
            EscrowParty::PartyB,
        )
        .await
        {
            let reply_msg = OTCMessage::EscrowFunded {
                negotiation_id: ctx.negotiation_id,
                escrow_address,
                signature: sig,
                funded_by: EscrowParty::PartyB,
                reporting_wallet: desk_kp.pubkey(),
            };
            if let Ok(env) =
                OTCEnvelope::new(desk_agent_id.to_string(), reply_msg).sign_with(&desk_kp)
            {
                let _ = hub
                    .send(otc_notification(desk_agent_id, counterparty_agent_id, env))
                    .await;
            }
        }
        return;
    }

    // Funding order (open network): only fund after Party A has funded.
    if !existing.state.a_funded {
        return;
    }

    // Avoid retry-spam after expiry; Party A should cancel to reclaim funds.
    let now_ts = Utc::now().timestamp();
    if now_ts > existing.state.expiry_unix_ts {
        return;
    }

    let token_source = if params.b_owes.kind == drbot_otc_escrow_program::LegKind::SplToken {
        Some(EscrowManager::associated_token_address(
            &desk_kp.pubkey(),
            &params.b_owes.mint,
        ))
    } else {
        None
    };

    let fund = match fee_kp.as_ref() {
        Some(fee_payer) => {
            escrow_manager
                .fund_party_b_with_fee_payer(fee_payer, &desk_kp, escrow_address, token_source)
                .await
        }
        None => {
            escrow_manager
                .fund_party_b(&desk_kp, escrow_address, token_source)
                .await
        }
    };

    let sig = match fund {
        Ok(sig) => sig,
        Err(e) => {
            // If the escrow already settled (closed), do not spam retries.
            if let Ok(Some(status)) = escrow_manager
                .get_receipt_status(params.negotiation_id, params.party_a, params.party_b)
                .await
            {
                if status == drbot_otc_escrow_program::ReceiptStatus::Settled {
                    return;
                }
            }
            tracing::warn!(error = %e, "Fund party B failed");
            return;
        }
    };

    negotiation_manager
        .record_escrow_funded_local(params.negotiation_id, EscrowParty::PartyB, sig.to_string())
        .await;

    let settled = escrow_manager
        .await_escrow_closed(
            &escrow_address,
            DEFAULT_ESCROW_CLOSE_TIMEOUT,
            DEFAULT_ESCROW_CLOSE_POLL_INTERVAL,
        )
        .await
        .unwrap_or(false);

    let reply_msg = if settled {
        let (final_price, final_quantity) = match &ctx.quote {
            OTCMessage::Quote {
                price, quantity, ..
            } => (*price, *quantity),
            _ => (0.0, 0),
        };
        OTCMessage::Settled {
            negotiation_id: ctx.negotiation_id,
            signature: sig.to_string(),
            final_price,
            final_quantity,
            reporting_wallet: desk_kp.pubkey(),
        }
    } else {
        OTCMessage::EscrowFunded {
            negotiation_id: ctx.negotiation_id,
            escrow_address,
            signature: sig.to_string(),
            funded_by: EscrowParty::PartyB,
            reporting_wallet: desk_kp.pubkey(),
        }
    };

    let reply_env = match OTCEnvelope::new(desk_agent_id.to_string(), reply_msg).sign_with(&desk_kp)
    {
        Ok(env) => env,
        Err(e) => {
            tracing::warn!(error = %e, "Failed to sign settlement update");
            return;
        }
    };

    let reply = otc_notification(desk_agent_id, counterparty_agent_id, reply_env);
    if let Err(e) = hub.send(reply).await {
        tracing::warn!(error = %e, "Failed to send settlement update");
    }
}

async fn existing_party_funding_signature(
    negotiation_id: &Uuid,
    manager: &Arc<OTCNegotiationManager>,
    party: EscrowParty,
) -> Option<String> {
    let negotiation = manager.get_negotiation(*negotiation_id).await?;
    negotiation.history.iter().rev().find_map(|ev| match ev {
        super::negotiation::NegotiationEvent::EscrowFunded {
            party: p,
            signature,
        } if *p == party => Some(signature.clone()),
        _ => None,
    })
}

/// Await a signed `Settled` notification for the given negotiation.
pub async fn wait_for_settled_notification(
    hub: Arc<A2AHub>,
    recipient_agent_id: Uuid,
    negotiation_id: Uuid,
    timeout: Duration,
) -> Result<Option<(Uuid, OTCEnvelope)>> {
    let mut rx = hub.subscribe();
    let deadline = tokio::time::Instant::now() + timeout;

    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return Ok(None);
        }

        let msg = match tokio::time::timeout(remaining, rx.recv()).await {
            Ok(Ok(m)) => m,
            Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(_))) => continue,
            _ => return Ok(None),
        };

        if msg.to != recipient_agent_id {
            continue;
        }

        let Some(env) = parse_otc_envelope(&msg) else {
            continue;
        };

        if env.verify_signature().ok() != Some(true) {
            continue;
        }

        if matches!(
            env.message,
            OTCMessage::Settled {
                negotiation_id: id,
                ..
            } if id == negotiation_id
        ) {
            return Ok(Some((msg.from, env)));
        }
    }
}

/// Helper for building a signed OTC A2A notification.
pub fn sign_otc_notification(
    from: Uuid,
    to: Uuid,
    signer: &Keypair,
    message: OTCMessage,
) -> Result<A2AMessage> {
    let env = OTCEnvelope::new(from.to_string(), message).sign_with(signer)?;
    Ok(otc_notification(from, to, env))
}
