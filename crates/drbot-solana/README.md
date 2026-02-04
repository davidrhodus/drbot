# drbot-solana

Solana integration for drbot, including:
- `validator_intel` (ported from `solana-clawd/validator-intel`)
- OTC agent-to-agent negotiation + on-chain escrow settlement (native SOL + SPL Token DvP)
- Optional “true P2P overlay” transport for A2A via libp2p (`drbot-a2a-p2p`)

## How A2A + P2P Works (high level)

- `drbot-a2a` provides an in-process message bus (`A2AHub`): agents register, advertise capabilities, and exchange `A2AMessage`s.
- `drbot-a2a-p2p` bridges that hub onto a libp2p swarm:
  - discovery: gossipsub announcements + optional Kademlia DHT provider records
  - delivery: request-response messages (peer-to-peer)
  - NAT traversal: optional circuit relay reservations (plus DCUtR where possible)
- `drbot-solana::otc` defines an `OTCEnvelope` that can be signed by a Solana wallet keypair for authenticity.

## OTC P2P Demo (Desk + Trader)

### 1) Start a relay (recommended for NAT traversal)

In one terminal:

```bash
cargo run -p drbot-a2a-p2p --example relay -- \
  --listen /ip4/0.0.0.0/tcp/4001 \
  --identity ./relay.key
```

Copy the printed `dial_addr` (it will look like `/ip4/.../tcp/4001/p2p/<PEER_ID>`).

### 2) Start a desk node

Mock mode (no Solana settlement; still negotiates over P2P):

```bash
cargo run -p drbot-solana --example otc_p2p_desk -- \
  --listen /ip4/0.0.0.0/tcp/0 \
  --bootstrap <RELAY_DIAL_ADDR> \
  --relay <RELAY_DIAL_ADDR> \
  --mid 150 --spread-bps 80 \
  --state-file ./otc-desk-state.json \
  --state-flush-ms 1000
```

### 3) Start a trader node

Mock mode:

```bash
cargo run -p drbot-solana --example otc_p2p_trader -- \
  --listen /ip4/0.0.0.0/tcp/0 \
  --bootstrap <RELAY_DIAL_ADDR> \
  --relay <RELAY_DIAL_ADDR> \
  --direction buy --amount-sol 1 \
  --timeout-ms 2000 \
  --state-file ./otc-trader-state.json
```

## Real Settlement on Solana (Mainnet)

To enable on-chain settlement, pass both `--wallet` and `--escrow-program-id` and use a real `--rpc-url`.

Desk:

```bash
cargo run -p drbot-solana --example otc_p2p_desk -- \
  --rpc-url https://api.mainnet-beta.solana.com \
  --wallet ~/.config/solana/desk.json \
  --fee-payer-wallet ~/.config/solana/fee_payer.json \
  --escrow-program-id <PROGRAM_PUBKEY> \
  --state-file ./otc-desk-state.json \
  --bootstrap <RELAY_DIAL_ADDR> \
  --relay <RELAY_DIAL_ADDR>
```

Trader:

```bash
cargo run -p drbot-solana --example otc_p2p_trader -- \
  --rpc-url https://api.mainnet-beta.solana.com \
  --wallet ~/.config/solana/trader.json \
  --fee-payer-wallet ~/.config/solana/fee_payer.json \
  --escrow-program-id <PROGRAM_PUBKEY> \
  --bootstrap <RELAY_DIAL_ADDR> \
  --relay <RELAY_DIAL_ADDR> \
  --direction buy --amount-sol 1 \
  --state-file ./otc-trader-state.json
```

Notes:
- SOL legs use **native SOL** deposits (lamports held by the escrow PDA).
- Auto-settlement happens in the **second** funding transaction (the escrow account closes).
- Funding order (open network default): **Party A funds first**, then the desk funds Party B after observing the on-chain deposit.
- Rent/fees: by default Party A creates the escrow; pass `--create-escrow` to let the desk create if missing.
- Token-2022 is intentionally deferred (SPL Token classic only for now).
- Trader crash safety: the trader persists funded escrows to `--state-file`; you can restart with `--watch-only` to keep auto-cancelling expired Open escrows.

Watch-only mode (run the auto-cancel watcher without placing a new RFQ):

```bash
cargo run -p drbot-solana --example otc_p2p_trader -- \
  --watch-only \
  --rpc-url https://api.mainnet-beta.solana.com \
  --wallet ~/.config/solana/trader.json \
  --escrow-program-id <PROGRAM_PUBKEY> \
  --state-file ./otc-trader-state.json
```

## Devnet Rehearsal (Recommended Before Mainnet)

1) Deploy the escrow program to devnet:

```bash
solana config set --url https://api.devnet.solana.com
cargo build-sbf -p drbot-otc-escrow-program
solana program deploy target/deploy/drbot_otc_escrow_program.so
```

2) Choose a settlement mint:
- If you want “real-ish” USDC on devnet, pass the devnet USDC mint to the desk via `--usdc-mint`.
- Or create a 6-decimal test mint and mint yourself some tokens (SPL Token classic).

3) Run desk + trader against devnet:

Desk:

```bash
cargo run -p drbot-solana --example otc_p2p_desk -- \
  --rpc-url https://api.devnet.solana.com \
  --wallet ~/.config/solana/desk.json \
  --escrow-program-id <DEVNET_PROGRAM_PUBKEY> \
  --usdc-mint <DEVNET_USDC_OR_TEST_MINT> \
  --bootstrap <RELAY_DIAL_ADDR> \
  --relay <RELAY_DIAL_ADDR>
```

Trader:

```bash
cargo run -p drbot-solana --example otc_p2p_trader -- \
  --rpc-url https://api.devnet.solana.com \
  --wallet ~/.config/solana/trader.json \
  --escrow-program-id <DEVNET_PROGRAM_PUBKEY> \
  --bootstrap <RELAY_DIAL_ADDR> \
  --relay <RELAY_DIAL_ADDR> \
  --direction buy --amount-sol 1
```

### Deploying the escrow program

The escrow program lives at `crates/drbot-otc-escrow-program/`.

Typical workflow (requires Solana CLI / toolchain installed):

```bash
# Build the SBF program artifact
cargo build-sbf -p drbot-otc-escrow-program

# Deploy to the currently-selected Solana cluster
solana program deploy target/deploy/drbot_otc_escrow_program.so
```

Use the deployed program id as `--escrow-program-id`.
