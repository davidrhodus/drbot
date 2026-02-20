# Persistent memory (defaults)

drbot’s “persistent memory” is three layers that work together:

## 1) Conversation continuity (sessions)

- All chats are persisted to SQLite (`config.storage.database_path`, default: `~/.local/share/drbot/drbot.db` on Linux/macOS).
- First-party clients (`drbot tui`, `drbot chat`) also perform a best-effort `auth.login` using a locally stored token so sessions are tied to a stable user id even when the gateway doesn’t require auth.
- When started inside a git repo, `drbot tui` and `drbot chat` will resume the most recent session for that repo by default (separately for normal chat vs tool/agent mode). If no prior session exists for that repo, a new session is started (instead of resuming a session from a different project).
- Outside a git repo, `drbot tui` / `drbot chat` behave like a normal chat app and resume the most recently updated session globally.

## 2) Personalization (stable preferences)

drbot bootstraps a default “assistant workspace” (OpenClaw-style) and injects it into chat runs:

- Workspace directory (default): `<data_dir>/agents/default/`
- Key files:
  - `IDENTITY.md` — assistant identity/name
  - `USER.md` — your stable profile (name, timezone, style, etc.)
  - `MEMORY.md` — pinned long-term notes

These files are created automatically on first use (and never overwritten).

## 3) Knowledge base (notes/docs)

Maintain longer notes as markdown files in:

- `<workspace>/memory/*.md`

On each gateway `chat.send`, OpenClaw `chat.send`/agent run, and direct CLI chat run (`drbot chat` in direct provider mode, including tool/agent mode), drbot does a best-effort semantic recall over `MEMORY.md` + `memory/*.md` and injects the top matching snippets into the system prompt (bounded by size limits).

### Project-local knowledge base (repo notes)

When chatting from within a codebase (e.g. `drbot tui` or `drbot chat`), drbot will also best-effort recall from a **project-local** knowledge base if present:

- `<repo>/.drbot/MEMORY.md` (optional)
- `<repo>/.drbot/memory/*.md` (optional)

By default, first-party clients will **auto-scaffold** this directory when launched inside a git repo
(set `DRBOT_PROJECT_KB_AUTO_INIT_ENABLED=0` to disable).

You can also scaffold it manually with:

- `drbot kb init`

The scaffold also creates `<repo>/.drbot/.gitignore` to ignore auto-generated long notes under `memory/auto/`.

This is injected client-side (so the gateway doesn’t need your current working directory) and is intended for project docs like runbooks, conventions, and architecture notes.

## 4) Autosave (automation)

By default, drbot will best-effort **persist common stable profile fields** when you state them in chat:

- Name (e.g. “my name is …”, “call me …”) → `USER.md`
- Timezone (e.g. “my timezone is …”, “tz is …”) → `USER.md`
- Some long-term style/formatting preferences (when you say “from now on…”, “always…”) → `USER.md`

It will also best-effort **auto-capture a small number of short stable facts** (conservative heuristics), e.g.:

- “We use Postgres.” → `MEMORY.md` (**Pinned**)

When chatting from inside a git repo, the same high-confidence “stable fact” auto-capture is also written to the
project-local KB at `<repo>/.drbot/MEMORY.md` (disable with `DRBOT_PROJECT_KB_AUTOSAVE_ENABLED=0`).

It will also auto-capture some **explicit project-scoped instructions**, e.g. messages that start with:

- “In this repo, …” → stored under **Conventions** (by default)
- “For this project: run …” → stored under **Runbooks** (heuristic)

It also supports explicit remember notes:

- Start a message with `/remember ...` (or `remember: ...`) to store it into `MEMORY.md` (short items go under **Pinned**; longer items are written to `memory/auto/*.md` and linked from **Knowledge base**). This is handled locally (no provider call).

It also supports project-scoped remember notes:

- Start a message with `/remember project ...` (or `remember project: ...`) to store it into `<repo>/.drbot/MEMORY.md` (same short/long behavior as above). You can prefix the note with `pinned:`, `conventions:`, `runbooks:`, or `kb:` to target sections.

It also supports explicit forget commands:

- Start a message with `/forget ...` (or `forget: ...`) to remove stored items (e.g. `/forget name`, `/forget timezone`, `/forget style`, `/forget all`, or `/forget <text>` to remove matching bullets in `MEMORY.md`). This is handled locally (no provider call).

It also supports project-scoped forget commands:

- Start a message with `/forget project ...` (or `forget project: ...`) to remove stored items from `<repo>/.drbot/MEMORY.md` (supports `all`, `pinned`, `conventions`, `runbooks`, `kb`, or a text match). If an item referenced `memory/auto/note-*.md`, that file is deleted too.

It also supports local inspection/search commands:

- `/profile` — show the current `USER.md` profile (local, no provider call).
- `/memory` — show a compact overview of workspace memory (and project `.drbot` memory when available) (local, no provider call).
- `/memory project` — show project memory only (`.drbot`) (local, no provider call).
- `/kb <query>` (or `/notes <query>`) — search recalled notes and show the top matches (local, no provider call; includes project `.drbot/memory` when available).

Controls:

- `DRBOT_GATEWAY_WORKSPACE_AUTOSAVE_ENABLED` (default: on) — controls automatic extraction (name/timezone/style) and best-effort timezone auto-fill when the `Timezone:` field is blank. `/remember` and `/forget` still work when autosave is disabled.
- `DRBOT_USER_TIMEZONE` — optional override for timezone auto-fill (e.g. `America/Los_Angeles`).

## Controls (env vars)

- `DRBOT_GATEWAY_WORKSPACE_CONTEXT_ENABLED` (default: on)
- `DRBOT_GATEWAY_WORKSPACE_NOTES_RECALL_ENABLED` (default: on)
- `DRBOT_PROJECT_KB_AUTO_INIT_ENABLED` (default: on)
- `DRBOT_PROJECT_KB_AUTOSAVE_ENABLED` (default: on)
- `DRBOT_GATEWAY_WORKSPACE_CONTEXT_MAX_BYTES`
- `DRBOT_GATEWAY_WORKSPACE_CONTEXT_MAX_FILE_BYTES`
- `DRBOT_GATEWAY_WORKSPACE_NOTES_RECALL_MAX_FILES`
- `DRBOT_GATEWAY_WORKSPACE_NOTES_RECALL_MAX_FILE_BYTES`
- `DRBOT_GATEWAY_WORKSPACE_NOTES_RECALL_MAX_RESULTS`
- `DRBOT_GATEWAY_WORKSPACE_NOTES_RECALL_MIN_SCORE`
- `DRBOT_GATEWAY_WORKSPACE_NOTES_RECALL_MAX_CHARS`
- `DRBOT_GATEWAY_WORKSPACE_NOTES_RECALL_MAX_ITEM_CHARS`

## Privacy note

Anything injected into the system prompt (workspace context + recalled snippets) is sent to your configured model provider. Keep secrets out of `USER.md`, `MEMORY.md`, and `memory/*.md`.
