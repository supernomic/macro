# Macro Agents Service — Tool Layer Conventions

This document is a **shared contract**. Every lane that adds agents, tools,
channels, or instrumentation to this service follows it. Read it before
writing any tool.

The agents service is the TypeScript runtime for Macro's super agent and
domain agents, built on the Flue durable-agent framework. It is a **client**
of Macro's Rust backend: all state of record (documents, sessions, ledger,
escalations, approvals, skills) lives behind Macro's APIs. This service holds
conversation state and orchestration only.

## Architecture in one paragraph

Flue agent functions call **native TypeScript tools** that wrap
`@macro/sdk` (Macro's OpenAPI-generated client). Each agent principal has a
scoped API token (`mat_...`) minted by Macro; the token — not runtime
config — is what limits which capabilities an agent actually has, because
Macro enforces scopes at its API boundary. Everything model-visible is
logged to Macro's session ledger via the tool layer and turn hooks.

```
Flue agent fn ──> tool layer (this repo) ──> @macro/sdk ──> Macro Rust API
                        │                                       │
                        └── ledger events ──────────────────────┘
```

## Identity: agent tokens

- Every agent principal gets its own token. **Never share tokens between
  principals.** Tokens are injected via environment/config, never hardcoded.
- Tool capability = token scope. A domain agent's tool allowlist is the
  scope set on its token (e.g. techops: `tool:tickets:*`, `tool:search`,
  `ledger:append`, `escalation:create`). Registering a tool in the runtime
  without the matching scope means the tool call will fail at Macro's
  boundary — that is by design. Keep the runtime registration and the scope
  set in sync in the agent's composition definition.
- Scope grammar (mirrors `crates/agent_identity`): colon-separated segments,
  trailing `:*` wildcard. Examples: `tool:search`, `tool:*`,
  `api:documents:read`, `ledger:append`, `escalation:create`.

## Tool conventions

1. **One file per tool** under `src/tools/<area>/<toolName>.ts`, exporting a
   single tool built with the `defineMacroTool` helper from
   `src/tools/toolkit.ts`. No inline tools in agent definitions.
2. **Name** tools `snake_case`, verb-first: `search_documents`,
   `create_escalation`, `update_ticket`. The tool name must match the
   `tool:<name>` scope it requires.
3. **Schemas**: every tool declares a Valibot input schema (Flue's native
   schema library; must be a top-level object schema) and returns a typed
   result. No `any`. Optional fields must have descriptions; the model reads
   them.
4. **All Macro access goes through `@macro/sdk`.** Never `fetch` a Macro
   endpoint directly; if the SDK is missing an endpoint, add it there first
   (see `.claude/skills/add-sdk-endpoint/SKILL.md` in the monorepo root).
5. **Ledger logging is not optional.** The `defineMacroTool` wrapper emits
   `tool/call` and `tool/result` ledger events automatically, with raw
   (unparsed) model arguments preserved. Do not build tools that bypass the
   wrapper. If a tool truncates or transforms output before returning it to
   the model, the *model-visible* form is what gets logged.
6. **Errors are structured**: throw `MacroToolError` with a stable `code`
   plus a human-readable `message`. The wrapper logs it on the `tool/result`
   event and renders a model-friendly error string. Never let raw HTTP
   errors reach the model.
7. **No business policy in tools.** Authorization, approval gating, tenancy
   filtering, and routing decisions live in Macro's Rust domain services.
   A tool converts model intent into an API call and the API result into
   model-visible text/JSON. If you find yourself writing an allow/deny
   branch in a tool, it belongs in Macro.
8. **Idempotency**: tools that create entities must accept and forward an
   idempotency key derived from the ledger `(session_id, seq)` of the
   `tool/call` event so retried steps don't double-create.

## Session mapping

- One Flue conversation ↔ one Macro session (UUID), created via the
  session-map API before the first ledger append. External threads (Slack
  `channel:thread_ts`, email thread ids) key the mapping so re-entry from
  the same thread resumes the same durable conversation.
- Never invent session ids locally; always create/lookup through the
  session-map endpoints.

## Ledger events ("model-visible means logged")

The tool layer and turn hooks emit these events; agents never write ledger
rows directly:

| When | Event |
|---|---|
| turn begins/ends | `turn/start`, `turn/end` (typed reason) |
| model request state changes | `request/header` (rendered system prompt, tool schemas, model, sampling, skill versions, composition id) |
| user/agent messages | `user/message` (with source), `assistant/message` (raw, with usage) |
| tool activity | `tool/call` (raw args), `tool/result` (or structured error) |
| skill injected into context | `skill/injected` |
| approvals/escalations | `approval/requested`, `approval/decided`, `escalation/created`, `escalation/resolved` |
| human feedback | `feedback/record` |
| context compaction | `compaction` (with replaced range + summary) |
| fork/resume | `session/seed` (parent id + seed length) |

Rules:

- The `request/header` snapshot must make the request reconstructable
  byte-exactly. If you change what the model sees (prompt overlay, tool set,
  skill content), a new `request/header` must be emitted before the next
  step.
- `composition_id` pins the exact agent definition (base prompt + overlay +
  tool allowlist + model tier + skill set). Bump it on any composition
  change; evals and training export group by it.
- Feedback is immutable in-log; editable ratings live in Macro's sidecar
  table, not in the ledger.

## Instrumentation

- Braintrust spans wrap every turn and tool call with the Macro session id,
  ledger seq range, and `composition_id` as metadata, so Braintrust traces
  and ledger rows are joinable.
- Never log secrets, tokens, or raw customer document bodies to Braintrust;
  the ledger (customer-owned) is the full-fidelity record, Braintrust holds
  eval/observability projections.

## Naming

- The user-facing surface for HITL items is the **Inbox**. `soup` is a
  legacy internal crate name (from "super inbox") — never use it in
  user-facing strings, API names, or new code identifiers.

## Layout

```
apps/agents/
  src/
    app.ts         # Hono application (agent mounts + channel ingress)
    config.ts      # env configuration (tokens, hosts, composition pins)
    agents/        # agent compositions (super agent, domain overlays)
    channels/      # Slack + native channel wiring
    ledger/        # ledger client + TS mirror of the event vocabulary
    macro/         # @macro/sdk client factory for agent principals
    sessions/      # Flue conversation ↔ Macro session binding
    tools/         # tool layer (one file per tool)
      toolkit.ts   # defineMacroTool wrapper (ledger + errors + idempotency)
    instrumentation/ # Braintrust/OTel wiring
  CONTRIBUTING.md  # this file
```
