---
name: flue
description: Use when building, debugging, reviewing, or documenting Flue agents, workflows, channels, skills, tools, sandboxes, targets, routing, persistence, or observability in apps/agents. Routes to version-matched Flue documentation through the CLI bundled with the installed @flue/cli.
---

# Flue

The Flue agents service lives in `apps/agents/` (standalone Bun package, not a Cargo/Bun workspace member). Always consult the documentation bundled with the **installed** `@flue/cli` version rather than web docs, so guidance matches the pinned runtime.

From `apps/agents/`:

```bash
./node_modules/.bin/flue docs                    # list every page
./node_modules/.bin/flue docs read <path>        # print a page as markdown
./node_modules/.bin/flue docs search "<query>"   # full-text search (JSON results)
```

Example: `flue docs search "durable execution"` → returns `concepts/durable-execution` → `flue docs read concepts/durable-execution`.

## Key pages for this repo

| Task | Page |
|---|---|
| Creating/configuring agents | `guide/building-agents` |
| Hooks (useModel, useTool, useInitialData, state, lifecycle) | `guide/agent-hooks` |
| Tools and valibot schemas | `guide/tools` |
| Channels (verified provider events → conversations) | `guide/channels` |
| Slack channel specifics | `ecosystem/channels/slack` |
| Durable storage (Postgres adapter) | `guide/database`, `ecosystem/databases/postgres` |
| Agent Skills (SKILL.md, `with { type: 'skill' }`) | `guide/skills` |
| Subagents / multi-agent composition | search `subagent` |
| Local runs without transport | `cli/run` |
| Blueprints for new integrations | `cli/add` |
| Braintrust / OpenTelemetry | `ecosystem/tooling/braintrust`, `ecosystem/tooling/opentelemetry` |

## Repo conventions

- Macro-specific tool wrappers: see `apps/agents/CONTRIBUTING.md` (defineMacroTool / bindMacroTools, ledger event emission).
- Every agent turn must be recorded in Macro's session ledger via `MacroSessionContext` (`apps/agents/src/sessions/macro-session.ts`).
- Agents authenticate to Macro with scoped `mat_...` tokens; never embed user credentials.
- Verify code with `bun run check` and `bun run build` from `apps/agents/`.
