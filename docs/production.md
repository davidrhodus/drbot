# Production checklist

This project spans local-only usage (TUI + loopback gateway) and networked deployments (gateway exposed to other machines / remote OpenClaw operators). “Production-ready” depends on which of those you’re aiming for.

## 1) Reliability gates (must-have)

- CI must be green on every merge to `main`.
- `cargo test --workspace` must be stable (no flakes/timeouts).
- Run `drbot doctor` against the intended config and treat warnings as action items.
- Add a “smoke run” that starts the gateway and hits health endpoints (optional but recommended).

## 2) Security gates (must-have for any non-loopback deployment)

- Do **not** bind to `0.0.0.0` without auth.
  - If you set `gateway.host = "0.0.0.0"`, also set either:
    - `gateway.auth_token`, and/or
    - `gateway.pairing_required = true` (recommended for remote OpenClaw operators).
- Prefer TLS when exposing the gateway outside a trusted LAN.
- Keep OpenClaw/agent tool execution restricted:
  - Avoid `--openclaw-agent-bash-allow-all`.
  - Prefer a tight allowlist (`--openclaw-agent-bash-allowlist ...`).
- Treat any “tool execution” mode as privileged:
  - Default should remain supervised/approval-gated for `exec` and filesystem writes.

## 3) Privacy + data handling (must-have)

- Know what writes to disk:
  - Sessions: SQLite DB (see `storage.*` config).
  - Media: filesystem (see `storage.media_path`).
  - Project knowledge base: `.drbot/` inside repos (auto-init can be disabled with `DRBOT_PROJECT_KB_AUTO_INIT_ENABLED=0`).
- Decide retention/backup:
  - Back up the SQLite DB and any `.drbot/` KB you care about.
  - Avoid storing secrets in workspace notes that could be recalled into prompts.

## 4) Operational readiness (recommended)

- Document “how we run it”:
  - config file location, required env vars, and the exact command used to start the gateway.
- Add basic observability:
  - Standardize `RUST_LOG` levels for prod and how logs are collected.
- Define upgrade/migration expectations:
  - Config and DB migration policy (even “best effort” should be explicit).

## Suggested “production bar” for drbot

1. Tests stable + CI green.
2. Default config safe for loopback; non-loopback requires explicit auth/pairing.
3. Tool execution remains supervised by default; “auto-approve” is an explicit opt-in.
4. Data writes/retention are documented and easy to disable.
