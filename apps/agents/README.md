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

## Durability

- Flue conversation state: Postgres via `src/db.ts` when
  `AGENTS_DATABASE_URL` is set (required in deploys); local cache otherwise.
- The trace of record: Macro's session ledger (`crates/agent_ledger`),
  written through the tool layer. One Flue conversation maps to one Macro
  session via the session-map API — see `src/sessions/macro-session.ts`.
