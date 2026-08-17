# Migrating Rust AgentLoop surfaces onto Flue

The TypeScript agents service (`apps/agents`) is the runtime for Macro's super
agent and domain agents. The Rust `AgentLoop` in `crates/agent` remains the
in-process completion engine for **existing** DCS chat, channel-bot replies,
and scheduled-action execution. It is **frozen**: bugfixes and safety patches
only. Do not add new agent products, channels, or workflows there.

Do not delete `crates/agent/src/agent_loop.rs`. Rollback for the DCS chat
experiment is unset `FLUENT_DCS_CHAT` (and/or `FLUENT_BASE_URL`).

## Gated DCS chat cutover (`FLUENT_DCS_CHAT`)

`POST /stream/chat/message` still defaults to `AgentLoop`. An **off-by-default**
flag can send **this handler invocation** to Flue instead:

| Env | Role |
|---|---|
| `FLUENT_DCS_CHAT` | Cutover switch. Truthy only for `1` / `true` / `on` (case-insensitive). Unset, empty, or any other value → AgentLoop. |
| `FLUENT_BASE_URL` | Flue origin, e.g. `http://127.0.0.1:5173`. Required when the flag is on. |

Read only via `macro_env_var::maybe_env_var!`. Default (both unset) is AgentLoop.
If the flag is on but the base URL is missing or not a URL, DCS logs an error and
**falls back to AgentLoop** — it does not 500 the user.

When the flag is on and the base URL is set:

1. DCS `chat_id` is the Flue conversation id (no new Macro table).
2. `POST {FLUENT_BASE_URL}/agents/super-agent/{chat_id}` with
   `{ "kind": "user", "body": "<user text>" }`.
3. Long-poll `GET ...?view=updates&offset=...&live=long-poll` and map Flue
   chunks onto existing `ChatStream` events (`ChatUserMessage` is still
   emitted by DCS; `message-delta` → `ChatMessageResponse` text/thinking;
   `tool-input` / `tool-output` / `tool-output-error` → tool parts;
   `StreamEnd` still closes the SSE envelope).
4. Dual-write the final assistant message through `store_conversation_messages`
   (Postgres chat UI) while Flue keeps its own conversation ledger.
5. User cancel → `POST .../abort`. In-flight AgentLoop streams (flag flipped
   mid-request) are left alone.

### Dual-write

The chat UI continues to read Macro Postgres messages. Flue's stream is a
second ledger, not a replacement. History in Flue starts at the first flagged
turn for that `chat_id`; prior AgentLoop turns are not replayed into Flue.

### Tool gap — not production tool parity

Flue tools are the small set under `apps/agents/src/tools` (documents search/
read, ledger history, graph lookup, escalations, skill proposals, feedback,
Slack reply). Rust chat uses `ai_tools::all_tools()` plus the user's MCP
servers. Do not claim the cutover is prod-ready for tool behavior.

### Auth gap — flag must stay off in production

Flue tools authenticate with `mat_...` agent tokens. DCS tools use the **user
JWT**. This slice is DCS→Flue HTTP only: it does not delegate the caller's
JWT into Flue, and Flue does not run DCS `ai_tools` on the user's behalf.
Leave `FLUENT_DCS_CHAT` unset in production until user-delegation is solved.

## What stays on AgentLoop (bugfixes only)

| Surface | Where | Why it stays |
|---|---|---|
| DCS native chat (`/chats`, `/stream`, `/chat/completions`) | `services/document_cognition_service` | Default path is still the Rust loop with the `ai_tools` toolset. Flue is flag-gated on `POST /stream/chat/message` only. |
| Channel bots | `crates/channel_bots` + `document_storage_service` `AgentLoopResponder` | Mention/inferred replies on existing channel threads. |
| Scheduled actions | `services/scheduled_action` | In-process executor still calls the Rust loop. |
| Memory generation | `crates/memory` | Still calls `AgentLoop` in-process. |
| Import | `crates/import` | Still calls `AgentLoop` in-process. |
| `ai_projections` | `crates/ai_projections` | Still calls `AgentLoop` in-process. |

When you must touch these, keep the change local (crash, authz, usage
recording). Do not grow the toolset or prompt overlay as a way to ship new
agent behavior. Do not add governed-skill injection, feedback capture, graph
lookup, HITL resume, or eval/trace-refine tools to the Rust `ai_tools`
toolset — those belong in Flue.

## What belongs in Flue (`apps/agents`)

- Super agent + domain agents (techops first)
- Slack (and future channels) via Flue's channel packages
- HITL (escalations, approval gates, skill proposals) resume callbacks
- Governed skills, feedback capture, Braintrust/eval correlation
- Session ledger mapping (Flue conversation id ↔ Macro session)

New work starts with a Flue agent function, Macro-scoped `mat_...` token, and
tools under `apps/agents/src/tools/`. Macro remains the system of record.
`crates/agent` stays the in-process completion engine for existing DCS chat
until cutover; it is not the place to prototype new agents.

## Cutover sketch for DCS chat

1. Keep AgentLoop serving current chat until a Flue conversation can be
   addressed per chat id (`POST /agents/super-agent/:id`).
2. Map each DCS chat to a Flue conversation + Macro session (session-map API)
   so ledger history is continuous. **Not done in this slice** — DCS `chat_id`
   is used directly as the Flue conversation id, with dual-write to Postgres.
3. Route new chats to Flue behind `FLUENT_DCS_CHAT`; leave in-flight AgentLoop
   streams to finish. **Partial:** flag-on handler invocations go to Flue;
   default remains AgentLoop.
4. Channel bots and scheduled actions follow the same pattern: Flue dispatch
   instead of `AgentLoopResponder` / `InProcessExecutor`, once those products
   are ready to move.

Until that cutover, treat `crates/agent/src/agent_loop.rs` as a frozen adapter.
