# OpenClaw Parity Checklist (drbot)

This checklist tracks what drbot still needs to implement to reach practical feature parity with
upstream OpenClaw (especially the Control UI) as of 2026-02-13 (OpenClaw v2026.2.12).

Parity scope (in order):
1) OpenClaw Control UI works end-to-end against drbot's `/openclaw/ws` gateway.
2) Security/ops expectations from recent OpenClaw releases (auth fail-closed, SSRF/tool hardening,
   usage dashboards).
3) Provider + integration coverage (models/providers/channels).

Out of scope (current project decision): WebChat parity work.

Legend:
- `[x]` done
- `[ ]` missing
- `[ ] (partial)` implemented but not yet OpenClaw-equivalent

## Current baseline (already in drbot)

- OpenClaw Gateway v3 endpoint: `/openclaw/ws` (`crates/drbot-gateway/src/openclaw.rs`)
- Core method/event surface covers OpenClaw's base list (no missing base methods/events; drbot adds
  a few extra methods) (`METHODS`, `EVENTS`)
- Sessions CRUD/preview + compact, skills status/install/update, exec approvals, nodes/devices pairing
- Send/poll via configured channels + approvals gate
- Cron jobs store + scheduler loop
- [x] Browser request routes now cover most OpenClaw browser tool actions (`/`, `/start`, `/stop`,
      `/reset-profile`, `/profiles`, `/profiles/create`, `DELETE /profiles/:name`,
      `/tabs`, `/tabs/open`, `/tabs/focus`, `/tabs/action`,
      `DELETE /tabs/:id`, `/navigate`, `/snapshot`, `/screenshot`, `/pdf`, `/act`,
      `/cookies`, `/cookies/set`, `/cookies/clear`,
      `/storage/local`, `/storage/session`, `/storage/*/set`, `/storage/*/clear`,
      `/set/offline`, `/set/headers`, `/set/credentials`, `/set/geolocation`, `/set/media`,
      `/set/timezone`, `/set/locale`, `/set/device`) with a persistent headless browser runtime per
      profile (default `openclaw`) and snapshot→act ref mapping (refs `e1..` map to CSS selectors).
      `act` is best-effort but now supports `click`/`type`/`press`/`hover`/`drag`/`select`/`fill`/
      `scrollIntoView`/`resize`/`wait`/`evaluate`/`close` (and `wait` supports basic
      `text`/`textGone`/`selector`/`url`/`loadState` conditions).
      Debug endpoints are best-effort: `/console`, `/errors`, `/requests` buffer CDP events per
      attached tab.
      Hooks are best-effort: `/hooks/file-chooser` (uploads) and `/hooks/dialog`, plus helper routes
      `/highlight` and `/response/body`, and downloads via `/wait/download` + `/download`.
      Traces are best-effort: `/trace/start`, `/trace/stop` produce a zip containing `trace.json`
      via CDP Tracing.
      Supports `cdpUrl` profiles (ws/wss) for connecting to an existing browser (loopback allowed by
      default; private-network CDP blocked unless allow-private env is set).
      Still missing: `profile=chrome` extension relay, true Playwright/ARIA refs, and OpenClaw-style
      label overlays, and full Playwright device-descriptor parity for `/set/device` (drbot supports
      a small set of presets).
      (`crates/drbot-gateway/src/openclaw.rs`,
      `crates/drbot-gateway/src/openclaw_agent_tools.rs`, `crates/drbot-browser/src/*`)
- TTS methods (system + OpenAI + ElevenLabs) (best-effort)

## P0 (Blockers for parity / safe deployment)

### Gateway security & transport

- [x] Implement TLS/WSS in the gateway server (honor `gateway.tls_enabled`, `gateway.tls_cert`,
      `gateway.tls_key`) and enforce a TLS 1.3 minimum (OpenClaw 2026.2.1).
      (`crates/drbot-gateway/src/server.rs`)
- [x] Raise OpenClaw WS payload/buffer limits so ~5,000,000-byte attachments work reliably
      (OpenClaw v2026.2.12). (`crates/drbot-gateway/src/openclaw.rs`)
- [x] Make gateway auth fail-closed by default for non-loopback binds (refuse public binds without
      `gateway.auth_token`). (`crates/drbot-gateway/src/server.rs`, `crates/drbot-gateway/src/state.rs`)
- [x] Redact secrets in `config.get` responses (auth tokens + provider/channel credentials) and
      preserve existing secrets on write when users keep the redaction placeholder.
      (`crates/drbot-gateway/src/openclaw.rs`)
- [x] Config redaction: do not redact maxToken/maxTokens-style fields (prevents round-trip validation failures)
      (OpenClaw v2026.2.12). (`crates/drbot-gateway/src/openclaw.rs`)
- [x] Strip embedded line breaks from pasted API keys/tokens before persisting config.
      (`crates/drbot-gateway/src/openclaw.rs`)
- [x] Add OpenClaw webhooks endpoints: HTTP `/hooks/wake` + `/hooks/agent` with `hooks.token`
      auth, constant-time secret comparison, per-client auth-failure throttling (429+Retry-After),
      and default denial of `sessionKey` overrides (OpenClaw v2026.2.12). Note: `stream=true` is
      not yet supported. (`crates/drbot-gateway/src/openclaw_webhooks.rs`,
      `crates/drbot-core/src/config.rs`, `crates/drbot-gateway/src/router.rs`)
- [x] SSRF guard for URL fetch surfaces (browser screenshots, remote skill sync, download installs)
      with default private-network blocking + allowlist escape hatch.
      (`crates/drbot-gateway/src/ssrf.rs`, `crates/drbot-gateway/src/openclaw.rs`,
      `crates/drbot-gateway/src/openclaw_skills.rs`)
- [x] Confine mirrored remote skill sync destinations to the managed `skills/` root (no path traversal)
      (OpenClaw v2026.2.12). (`crates/drbot-gateway/src/openclaw_skills.rs`)

### Usage & cost

- [x] Replace `usage.status` stub with real windows (tokens used + next reset-at if known).
      (`crates/drbot-gateway/src/openclaw_usage.rs`, `crates/drbot-gateway/src/openclaw.rs`)
- [x] Persist per-request usage records (JSONL under OpenClaw state dir) and aggregate them for
      `usage.cost` / `usage.status` (do not rely only on session totals).
      (`crates/drbot-gateway/src/openclaw_usage.rs`, `crates/drbot-gateway/src/openclaw.rs`)
- [x] Cost attribution for `usage.cost` (built-in fallback table + optional `openclaw_costs.json`
      overrides).
      (`crates/drbot-gateway/src/openclaw_usage.rs`)

### Control UI critical behaviors

- [x] Broadcast inbound channel messages as OpenClaw `chat` events (currently persisted only).
      (`crates/drbot-gateway/src/openclaw_inbound.rs`, `crates/drbot-gateway/src/openclaw.rs`)
- [x] Use group-aware channel ids in session keys (Slack `channel:`/`group:` + Telegram negative-id
      groups), canonicalizing legacy `slack:C123` / `telegram:-100…` keys to the derived form,
      while still delivering outbound messages to raw channel ids.
      (`crates/drbot-gateway/src/openclaw.rs`, `crates/drbot-gateway/src/openclaw_inbound.rs`)
- [x] Ensure `channels.status` includes enough detail for Control UI (configured/enabled/connected,
      last error, plus UI metadata like `channelMeta` and per-channel required fields like
      `signal.baseUrl`, and support `{probe:true}` refreshes with `probe` + `lastProbeAt`).
      (`crates/drbot-gateway/src/channel_manager.rs`, `crates/drbot-gateway/src/openclaw.rs`)
- [x] Support `channels.logout` flows used by Control UI (WhatsApp session logout + Telegram token
      clear).
      (`crates/drbot-gateway/src/openclaw.rs`, `crates/drbot-gateway/src/channel_manager.rs`)
- [x] Cron API parity: canonical `schedule.at` RFC3339 UTC, `job.delivery` support for agentTurn
      summaries, `consecutiveErrors` + backoff, anchored `every` schedules, and disable one-shot
      `at` jobs after terminal runs (ok/error/skipped).
      (`crates/drbot-gateway/src/openclaw.rs`)
- [x] Cron delivery parity: support `delivery: "announce"` (OpenClaw v2026.2.3) for isolated jobs
      that should broadcast a lightweight status update rather than running an agent turn.
      (`crates/drbot-gateway/src/openclaw.rs`)
- [x] Chat parity: support `responsePrefix` overrides (OpenClaw v2026.2.3) to force a response
      prefix for a single `chat.send` run (useful for code/JSON-only modes).
      (`crates/drbot-gateway/src/openclaw.rs`)
- [x] Messages parity: response prefix cascade across channels (OpenClaw v2026.2.3+).
      Implemented: `channels.<ch>.accounts.<id>.responsePrefix` (per-account via `accountId`)
      → `channels.<ch>.responsePrefix` (per-channel) → `messages.responsePrefix` (global), including
      `auto` (derives `[agent.name]`) and empty-string overrides to disable inherited prefixes.
      Note: `accountId` currently affects prefix selection only (drbot still uses a single runtime
      credential set per channel).
      (`crates/drbot-core/src/config.rs`, `crates/drbot-gateway/src/openclaw.rs`,
      `crates/drbot-gateway/src/openclaw_agent_tools.rs`)

- [x] Include per-session message/event `count` in `sessions.list` rows (OpenClaw v2026.2.12).
      (`crates/drbot-gateway/src/openclaw.rs`)

### Stubs that the UI will surface

- [ ] (partial) `wizard.*` now implements a real OpenClaw-compatible step flow for **Gateway basics**
      (auth token + bind host + port) plus a **security warning / risk acknowledgement**, optional
      **provider setup** (default provider + API key + default model), and optional **channel setup**
      (enable channels + basic credentials for Telegram/Discord/Slack/Matrix/Signal/WhatsApp/WebChat).
      It writes via `config.patch` and now hot-applies provider/channel changes (no restart required
      for those), but it still does **not** implement full OpenClaw onboarding (skills/hooks/daemon/
      tailscale/etc).
      (`crates/drbot-gateway/src/openclaw.rs`)
- [x] Hot-apply `config.set`/`config.patch` changes for provider + channels runtime (Control UI
      onboarding works without a manual restart, and writes target the active config path; bind/TLS
      changes still require a restart).
      (`crates/drbot-gateway/src/openclaw.rs`, `crates/drbot-gateway/src/state.rs`,
      `crates/drbot-gateway/src/channel_manager.rs`)
- [x] Implement `update.run` (RPC + `gateway` tool action) via `drbot-update`:
      fetch manifest, download/verify, self-replace the current executable, write a
      `restart-sentinel.json`, and schedule SIGUSR1 restart when an update is applied.
      (`crates/drbot-gateway/src/openclaw.rs`, `crates/drbot-gateway/src/openclaw_agent_tools.rs`,
      `crates/drbot-update/*`)
- [x] Implement `gateway.restart` (agent tool): schedules an authorized SIGUSR1 restart request,
      writes a `restart-sentinel.json`, drains in-flight OpenClaw turns before restart (OpenClaw v2026.2.12),
      and the `drbot gateway` process performs a graceful shutdown followed by an `exec()` restart
      (no external supervisor required on Unix).
      (`crates/drbot-gateway/src/openclaw_agent_tools.rs`, `src/main.rs`, `crates/drbot-gateway/src/server.rs`)

## P1 (Parity with recent OpenClaw releases)

### Providers & model catalog

- [x] Add a generic OpenAI-compatible provider surface (configure OpenRouter/xAI/etc via
      `providers.openai_compatible` + select with `providers.default_provider`).
      (`crates/drbot-core/src/config.rs`, `crates/drbot-gateway/src/state.rs`,
      `crates/drbot-openai/src/client.rs`)
- [x] Update built-in model lists (e.g., Opus 4.6, GPT-5.3-codex) and/or make `models.list` dynamic.
      (`crates/drbot-openai/src/client.rs`, `crates/drbot-anthropic/src/client.rs`,
      `crates/drbot-gateway/src/openclaw.rs`)
- [x] Cloudflare AI Gateway support: configure via provider `base_url` + per-provider `headers`
      (OpenAI + Anthropic), with config redaction preserving sensitive header values.
      (`crates/drbot-core/src/config.rs`, `crates/drbot-gateway/src/state.rs`,
      `crates/drbot-openai/src/client.rs`, `crates/drbot-anthropic/src/client.rs`,
      `crates/drbot-gateway/src/openclaw.rs`)

### Agent runner hardening

- [x] Implement OpenClaw-style `safeBins` enforcement for the bash tool (stdin-only / no path-like
      args) and thread `autoAllowSkills` from exec approvals into the bash allowlist.
      (`crates/drbot-agents/src/tools.rs`, `crates/drbot-gateway/src/openclaw_skills.rs`,
      `crates/drbot-gateway/src/openclaw_exec_approvals.rs`)
- [x] Block dynamic linker and similar runtime override env vars for host exec (LD*/DYLD*/PATH/etc)
      to prevent injection via `env` (OpenClaw 2026.2.1).
      (`crates/drbot-agents/src/tools.rs`, `crates/drbot-gateway/src/openclaw_agent_tools.rs`)
- [x] Expose OpenClaw tool-name aliases (`exec`/`read`/`write`/`edit`) and accept common param aliases
      (`cmd`, `workdir`, `file_path`, `old_string`/`new_string`) to reduce tool-call loops
      (OpenClaw + Claude Code style).
      (`crates/drbot-agents/src/tools.rs`, `crates/drbot-gateway/src/openclaw_agent_tools.rs`)
- [x] Add OpenClaw `web_fetch` tool with SSRF protection + redirect limits for agent runs and
      `/tools/invoke` (reduces reliance on `bash` + curl for web reads).
      (`crates/drbot-gateway/src/openclaw_agent_tools.rs`, `crates/drbot-gateway/src/openclaw.rs`,
      `crates/drbot-gateway/src/router.rs`)
- [x] Add OpenClaw `web_search` tool (search + citations + caching by default) for agent runs and
- [x] Add OpenClaw `mcp` tool (best-effort) for agent runs and `/tools/invoke` (OpenClaw v2026.2.12):
      reads `<OPENCLAW_STATE_DIR>/mcp.json` and supports `stdio` + `http` servers (http is guarded by
      SSRF policy envs `DRBOT_OPENCLAW_MCP_ALLOW_PRIVATE` / `DRBOT_OPENCLAW_MCP_ALLOWED_HOSTNAMES`).
      (`crates/drbot-gateway/src/openclaw_agent_tools.rs`)
      `/tools/invoke` (OpenClaw v2026.2.6+).
      (`crates/drbot-gateway/src/openclaw_agent_tools.rs`, `crates/drbot-gateway/src/openclaw.rs`,
      `crates/drbot-gateway/src/router.rs`)
- [x] Security/Web tools: wrap browser/web outputs as untrusted external content and strip `toolResult.details`
      from model-facing transcripts to reduce prompt-injection replay risk (OpenClaw v2026.2.12).
      (`crates/drbot-agents/src/agent.rs`, `crates/drbot-gateway/src/openclaw_agent_tools.rs`)
- [x] Add OpenClaw session tools: `sessions_list`, `sessions_history`, `sessions_send`,
      `sessions_spawn`, `session_status` for agent runs and `/tools/invoke`.
      (`crates/drbot-gateway/src/openclaw_agent_tools.rs`, `crates/drbot-gateway/src/openclaw.rs`,
      `crates/drbot-gateway/src/router.rs`)
- [x] Add OpenClaw memory file tools: `memory_search`, `memory_get` for agent runs and `/tools/invoke`.
      (`crates/drbot-gateway/src/openclaw_agent_tools.rs`, `crates/drbot-gateway/src/openclaw.rs`,
      `crates/drbot-gateway/src/router.rs`)
- [ ] (partial) `memory_search` now does embedding-based semantic search over `MEMORY.md` +
      `memory/*.md` (local hash embeddings + small lexical boost).
      Still missing: hosted backend parity.
      (`crates/drbot-gateway/src/openclaw_agent_tools.rs`)
- [ ] (partial) Add OpenClaw `exec` tool (command/workdir/env/yieldMs/background/timeout) integrated
      with `process` sessions (enables background continuation).
      Still missing: true `elevated` execution (flag is accepted, but drbot does not provide
      privilege escalation), true sandbox isolation (`host="sandbox"` now uses an isolated cwd under
      the OpenClaw state dir and clears the environment on Unix, but does not provide OS-level
      filesystem/network isolation),
      and `pty` on `host="node"` (gateway `pty` is now supported).
      `host="node"` is supported via node `system.run` (paired node required).
      (`crates/drbot-gateway/src/openclaw_agent_tools.rs`, `crates/drbot-gateway/src/openclaw.rs`,
      `crates/drbot-gateway/src/router.rs`)
- [x] Add OpenClaw `process` tool for background processes (start/list/poll/log/kill/remove/clear)
      for agent runs and `/tools/invoke`.
      (`crates/drbot-gateway/src/openclaw_agent_tools.rs`, `crates/drbot-gateway/src/openclaw.rs`,
      `crates/drbot-gateway/src/router.rs`)
- [ ] (partial) OpenClaw `process` interactive actions (`write`, `send-keys`, `paste`, `submit`)
      are implemented for stdin-piped sessions and PTY sessions (PTY uses `portable-pty`).
      Still missing: OpenClaw's full shared PTY session registry across builtin tools and sandbox
      isolation. (`process` now supports `resize` for PTY sessions.)
      (`crates/drbot-gateway/src/openclaw_agent_tools.rs`)
- [x] Add OpenClaw `message` tool (maps to drbot channel send/poll) for agent runs and `/tools/invoke`.
      (`crates/drbot-gateway/src/openclaw_agent_tools.rs`, `crates/drbot-gateway/src/openclaw.rs`,
      `crates/drbot-gateway/src/router.rs`)
- [ ] (partial) Expand `message` beyond send/poll. drbot now supports `reply`/`thread-reply`
      (aliasing to `replyTo`) and `broadcast` (best-effort; requires enabled+configured channels).
      Still missing: attachments/cards/buttons/reactions/edits/etc.
      (`crates/drbot-gateway/src/openclaw_agent_tools.rs`)
- [ ] (partial) Add OpenClaw UI tools: `browser` and `canvas`.
      - `browser`: largely implemented; `BrowserTool` maps actions to `browser.request` routes
        (local or node proxy) including tabs/snapshot/act (best-effort). Still missing: OpenClaw's
        `profile=chrome` extension relay, Playwright/ARIA ref parity, and label-overlay UX.
      - `canvas`: supports present/hide/navigate/eval/snapshot/A2UI by invoking node commands.
      (`crates/drbot-gateway/src/openclaw_agent_tools.rs`, `crates/drbot-gateway/src/openclaw.rs`)
- [ ] (partial) Add OpenClaw `nodes` tool (status/describe/pending/approve/reject/notify/
      camera_list/camera_snap/camera_clip/screen_record/location_get/run/invoke) with media saved
      under `.drbot/nodes/*` and file paths returned.
      Still missing: deeper OpenClaw node UX parity + any extra upstream actions beyond these
      helpers.
      (`crates/drbot-gateway/src/openclaw_agent_tools.rs`, `crates/drbot-gateway/src/openclaw.rs`,
      `crates/drbot-gateway/src/router.rs`)
- [ ] (partial) Add OpenClaw `image` tool for vision analysis (file paths + data: + http(s) with SSRF).
      Supports **Anthropic** and **OpenAI/OpenAI-compatible** provider configs (with optional
      `provider` override / `DRBOT_OPENCLAW_IMAGE_PROVIDER`) and now honors `agents.defaults.imageModel`
      (`primary` + `fallbacks`) for multi-model fallback attempts (accepts `provider/model` refs or
      bare model ids).
      drbot also supports per-agent `imageModel` overrides via `agents.update` / `agents.json`.
      Still missing: OpenClaw's auth-profile-aware pairing/scan defaults.
      (`crates/drbot-gateway/src/openclaw_agent_tools.rs`, `crates/drbot-gateway/src/openclaw.rs`)
- [x] Cap oversized tool results to reduce context overflow risk (OpenClaw 2026.2.9+).
      (`crates/drbot-agents/src/agent.rs`)
- [x] Multi-agent session key isolation: canonicalize `sessionKey` as `agent:<agentId>:...` and
      migrate legacy stored sessions/system-event keys on access.
      (`crates/drbot-gateway/src/openclaw.rs`, `crates/drbot-gateway/src/openclaw_heartbeat.rs`)
- [x] Use per-agent default model from `agents.json` when a session has no explicit `model`.
      (`crates/drbot-gateway/src/openclaw.rs`, `crates/drbot-gateway/src/openclaw_heartbeat.rs`)
- [x] Add OpenClaw agent tools: `cron` and `gateway` shims (agent runner + `/tools/invoke`).
      (`crates/drbot-gateway/src/openclaw_agent_tools.rs`, `crates/drbot-gateway/src/openclaw.rs`,
      `crates/drbot-gateway/src/router.rs`)
- [ ] (partial) Add OpenClaw-style before-tool hooks / per-sender + group tool policies (e.g. tool
      allow/deny by sender key, plus group overrides used by Discord/Slack/Telegram). drbot now
      supports **group tool allow/deny rules** via `<OPENCLAW_STATE_DIR>/tool-policy.json`
      (match `keyPrefix`/`channel`/`chatType`, allow/deny glob patterns + tool groups, optional
      `hardDeny`, and OpenClaw-style `exec`→`apply_patch` allowlist inference), enforced for
      **agent runs** and `/tools/invoke`. drbot also enforces per-session `sendPolicy` (ask/allow/deny)
      for `send`/`poll`/`deliver` and per-session `execAsk` (ask/allow/deny) for dangerous builtin tools
      (`bash`/`write_file`/`apply_patch`) in agent runs, plus per-session `toolPolicy` (allow/ask/deny
      per tool) for approvals. `toolsBySender` is applied best-effort by inferring `senderId` from the
      most recent stored user message metadata in the session transcript. drbot also supports
      **global + per-agent tool profiles/allow/deny/alsoAllow** via `<OPENCLAW_STATE_DIR>/openclaw_tools.json`
      (global) and `agents.json` (per-agent), enforced for agent runs and `/tools/invoke`, including
      `message` tool policy aliasing for drbot's `send`/`poll`. Still missing: provider-scoped tool
      policies and OpenClaw's full policy resolution surface.
      (`crates/drbot-agents/*`, `crates/drbot-gateway/src/openclaw.rs`,
      `crates/drbot-gateway/src/openclaw_agent_tools.rs`)
- [x] Improve session compaction (summarize + archive) instead of truncation-only.
      (`crates/drbot-gateway/src/openclaw.rs`)

### Memory

- [x] Wire memory recall into chat/agent runs (vector search + long-term memory; uses local hash
      embeddings by default).
      (`crates/drbot-memory/*`, `crates/drbot-gateway/src/state.rs`,
      `crates/drbot-gateway/src/openclaw.rs`, `crates/drbot-gateway/src/openclaw_memory.rs`)
- [x] Add external embeddings for OpenClaw semantic recall (Voyage AI) + normalization, with
      best-effort fallback to local embeddings. Controlled by `DRBOT_OPENCLAW_MEMORY_EMBED_PROVIDER`
      (`local`/`voyage`/`auto`) + `VOYAGE_API_KEY` (optional: `DRBOT_OPENCLAW_MEMORY_VOYAGE_MODEL`,
      `DRBOT_OPENCLAW_MEMORY_VOYAGE_BASE_URL`). (`crates/drbot-gateway/src/openclaw_memory.rs`)
- [x] QMD memory backend parity (OpenClaw 2026.2.2): opt-in `memory.backend="qmd"` for `memory_search`
      + `memory_get` (best-effort external `qmd` binary) plus `memory.qmd.paths` (strings or
      `{name,path,pattern}` objects) for extra roots.
      Supports `memory.qmd.sessions.enabled` to export recent session transcripts into a QMD
      `sessions` collection (best-effort).
      Stores config in `<OPENCLAW_STATE_DIR>/openclaw_memory.json`, includes it in `config.get` +
      `config.patch` baseHash, and falls back to local scanning when `qmd` is unavailable.
      (`crates/drbot-gateway/src/openclaw.rs`, `crates/drbot-gateway/src/openclaw_agent_tools.rs`,
      `crates/drbot-gateway/src/router.rs`, `tests/openclaw_gateway.rs`)

### UX

- [x] Add CLI shell completions generation. (`src/main.rs`, `crates/drbot-cli/*`)
- [x] `logs.tail` lines are prefixed with local-time timestamps (OpenClaw v2026.2.12).
      (`crates/drbot-gateway/src/openclaw_logs.rs`)
- [x] VoiceWake parity: support language selection fields in `voicewake.get`/`voicewake.set`
      (OpenClaw 2026.1.30+).
      (`crates/drbot-gateway/src/openclaw.rs`)
- [ ] (skipped) WebChat: image paste/upload + image-only send (OpenClaw WebChat parity).
      (`crates/drbot-webchat/src/*`, `crates/drbot-core/src/message.rs`)
- [x] Agent management RPCs: `agents.create`/`agents.update`/`agents.delete`, `agents.json` store,
      workspace bootstrap, and `agent.identity.get` backed by `agents.json`.
      (`crates/drbot-gateway/src/openclaw.rs`)
- [x] Per-agent skills allowlist: when `agents.list[].skills` is set in `agents.json`, only those
      skills are included in the injected skills prompt (agent runs + heartbeat runs).
      (`crates/drbot-gateway/src/openclaw.rs`, `crates/drbot-gateway/src/openclaw_skills.rs`,
      `crates/drbot-gateway/src/openclaw_heartbeat.rs`)
- [x] Default subagent thinking config parity (OpenClaw 2026.2.2): support `agents.defaults.subagents.thinking`
      and per-agent `agents.list[].subagents.thinking` defaults for spawned subagents (wired into
      `sessions_spawn` → `sessions.patch thinkingLevel`, and surfaced in the runtime line).
      (`crates/drbot-gateway/src/openclaw.rs`, `crates/drbot-gateway/src/openclaw_agent_tools.rs`,
      `tests/openclaw_gateway.rs`)
- [x] Expose runtime shell metadata on agents + agent run envelopes (OpenClaw 2026.2.9+).
      (`crates/drbot-gateway/src/openclaw.rs`)
- [ ] (partial) Agents dashboard UX parity: `agents.list` now includes `modelName` (best-effort),
      plus `skills` + `tools` config for each agent, and `agents.update` supports `emoji`.
      Still missing: `canThink` model metadata + any Control UI tool-checkmark UX expectations.
      (`crates/drbot-gateway/src/openclaw.rs`)
- [x] Path override parity: support `OPENCLAW_HOME` (OpenClaw 2026.2.9) as an alternative to
      `OPENCLAW_STATE_DIR` for state resolution.
      (`crates/drbot-gateway/src/openclaw_paths.rs`)

## P2 (Nice-to-have / long-tail parity)

### Channel depth

- [ ] Telegram parity: richer outbound options (silent send, reply formatting, edit), inbound media
      handling, sticker/file handling. Note: basic `replyTo` threading is now wired through
      OpenClaw `send` + agent `send/message` tools. (`crates/drbot-telegram/*`,
      `crates/drbot-gateway/src/openclaw.rs`, `crates/drbot-gateway/src/openclaw_agent_tools.rs`)
- [ ] Slack/Discord/Matrix parity: edits, quotes, attachments. Note: basic threading via `replyTo`
      is now wired through OpenClaw `send` + agent `send/message` tools (Slack thread_ts, Discord
      message_reference; Matrix still missing). (`crates/drbot-slack/*`, `crates/drbot-discord/*`,
      `crates/drbot-matrix/*`, `crates/drbot-gateway/src/openclaw.rs`,
      `crates/drbot-gateway/src/openclaw_agent_tools.rs`)
- [ ] Feishu/Lark + other plugin-based channels supported by OpenClaw (if we choose to match).
      (new crates)

### Ops

- [x] "Doctor" parity (minimal): `drbot doctor` now checks gateway exposure/auth + TLS-on-public,
      SSRF allow-private envs, approvals bypass envs, and channel allowlist empties.
      (`src/main.rs`)

## Validation

- Run OpenClaw protocol regression tests: `cargo test --test openclaw_gateway`
- Smoke-test OpenClaw Control UI against drbot: start `drbot gateway` and point the Control UI at
  `ws://<host>:<port>/openclaw/ws` (or `wss://...` when TLS is enabled). Confirm auth/tls behavior
  matches the intended policy.
