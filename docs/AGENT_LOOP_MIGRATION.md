# Migrating Rust AgentLoop surfaces onto Flue

The TypeScript agents service (`apps/agents`) is the runtime for Macro's super
agent and domain agents. The Rust `AgentLoop` in `crates/agent` remains the
in-process completion engine for **existing** DCS chat, channel-bot replies,
and scheduled-action execution. It is **frozen**: bugfixes and safety patches
only. Do not add new agent products, channels, or workflows there.

## What stays on AgentLoop (bugfixes only)

| Surface | Where | Why it stays |
|---|---|---|
| DCS native chat (`/chats`, `/stream`, `/chat/completions`) | `services/document_cognition_service` | Existing product chat still runs the Rust loop with the `ai_tools` toolset. |
| Channel bots | `crates/channel_bots` + `document_storage_service` `AgentLoopResponder` | Mention/inferred replies on existing channel threads. |
| Scheduled actions | `services/scheduled_action` | In-process executor still calls the Rust loop. |

When you must touch these, keep the change local (crash, authz, usage
recording). Do not grow the toolset or prompt overlay as a way to ship new
agent behavior.

## What belongs in Flue (`apps/agents`)

- Super agent + domain agents (techops first)
- Slack (and future channels) via Flue's channel packages
- HITL (escalations, approval gates, skill proposals) resume callbacks
- Governed skills, feedback capture, Braintrust/eval correlation
- Session ledger mapping (Flue conversation id ↔ Macro session)

New work starts with a Flue agent function, Macro-scoped `mat_...` token, and
tools under `apps/agents/src/tools/`. Macro remains the system of record.

## Cutover sketch for DCS chat

1. Keep AgentLoop serving current chat until a Flue conversation can be
   addressed per chat id (`POST /agents/super-agent/:id`).
2. Map each DCS chat to a Flue conversation + Macro session (session-map API)
   so ledger history is continuous.
3. Route new chats to Flue; leave in-flight AgentLoop streams to finish.
4. Channel bots and scheduled actions follow the same pattern: Flue dispatch
   instead of `AgentLoopResponder` / `InProcessExecutor`, once those products
   are ready to move.

Until that cutover, treat `crates/agent/src/agent_loop.rs` as a frozen adapter.
