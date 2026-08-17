# Macro Agents Service

The TypeScript runtime for Macro's super agent and domain agents, built on
the [Flue](https://flueframework.com) durable-agent framework. Agents call
native TS tools over `@macro/sdk`; Macro's Rust backend stays the system of
record (ledger, sessions, escalations, approvals, skills).

Read `CONTRIBUTING.md` before adding agents, tools, or channels — it is the
shared contract every lane follows.

## Setup

This package installs standalone (like `packages/sdk`), not as a root
workspace member: its Vite 8 toolchain must stay isolated from the web
app's Vite 6 tree.

```bash
cd apps/agents
bun install
cp .env.example .env   # fill in tokens
```

Requirements: Bun ≥ 1.3, Node ≥ 22.18 (Flue loads its TS config natively).
`@macro/sdk` is consumed from `../../packages/sdk` — build it first
(`cd packages/sdk && bun run build`) when types look stale.

## Develop

```bash
bun run dev            # vite dev server, agents served over HTTP
bun run check          # typecheck
bun run lint           # biome
bun run build          # production build -> dist/server.mjs
```

Message the super agent (one `POST` per message, `202` on admission):

```bash
curl -X POST http://localhost:5173/agents/super-agent/demo-1 \
  -H 'content-type: application/json' \
  -d '{"kind":"user","body":"What documents mention onboarding?"}'
curl "http://localhost:5173/agents/super-agent/demo-1?view=history"
```

## Slack

Verified Events API ingress is served at `POST /channels/slack/events`
(`src/channels/slack.ts`). Register that URL in the Slack app config and
subscribe to `app_mention` and `message.channels`. One Slack thread maps to
one durable conversation and one Macro session
(`slack_thread:<channel>:<thread_ts>`).

Listening is per-channel (`src/channels/slack-config.ts`):
`mentions_only` (default — mentions plus threads the agent was brought
into), `proactive` (all plain messages delivered; the agent's instructions
make it reply only when it adds clear value — its sole way to speak is the
`reply_in_slack_thread` tool, so not calling it is a non-reply), or
`silent`. Set `SLACK_DEFAULT_CHANNEL_MODE` / `SLACK_CHANNEL_MODES`.

## Domain agents

Domain specialists (TechOps first) are Flue subagents the super agent
delegates to via the built-in `task` tool. Each domain is declared in
`src/domains/` and runs under its own Macro agent principal:

```bash
# Mint the domain principal + token via Macro's agent-identity admin API
# (internal-only endpoints on document-cognition-service), then:
MACRO_TECHOPS_AGENT_TOKEN=mat_...
```

The token's scopes are the domain's effective tool allowlist, enforced at
Macro's API boundary. Leave the variable unset to disable the domain — the
model is never offered a specialist that isn't configured. See
`CONTRIBUTING.md` ("Domain agents") for the full conventions.

## Escalations (human-in-the-loop)

When an agent can't resolve a request it calls the `create_escalation` tool
(`src/tools/escalations/create-escalation.ts`), which files an escalation in
Macro (`crates/escalations`). Macro routes it to an expert or team queue via
configurable rules (availability, round-robin, claim semantics) and it
surfaces in the Macro inbox.

When the expert resolves it, Macro POSTs to
`/callbacks/escalations/:conversationId` on this service
(`src/escalations/resume.ts`), which logs the resolution to the ledger and
resumes the durable conversation with the expert's answer.

Configuration:

- `AGENTS_PUBLIC_URL` — public base URL of this service, used to build the
  callback URL sent with each escalation.
- `ESCALATION_CALLBACK_TOKEN` — shared bearer token; set the same value in
  Macro (`EscalationCallbackToken` env var on document-cognition-service) so
  callbacks are verified.

## Durability

- Flue conversation state: Postgres via `src/db.ts` when
  `AGENTS_DATABASE_URL` is set (required in deploys); local cache otherwise.
- The trace of record: Macro's session ledger (`crates/agent_ledger`),
  written through the tool layer. One Flue conversation maps to one Macro
  session via the session-map API — see `src/sessions/macro-session.ts`.
