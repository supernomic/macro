---
name: flue
description: Use when building, debugging, reviewing, or documenting Flue agents, workflows, channels, skills, tools, sandboxes, targets, routing, persistence, observability, or CLI usage; routes coding agents to version-matched Flue documentation through the CLI.
---

# Flue

Use `flue docs` to read the documentation bundled with the installed `@flue/cli` version. Choose relevant paths from the catalog below and run `flue docs read <path>`. If no catalog entry matches your task, run `flue docs search <query>`, then read the most relevant result with `flue docs read <path>`.

For example, `flue docs search "durable execution"` searches with the query `durable execution`. If it returns the path `guide/durability`, run `flue docs read guide/durability` to read that page.

## In this repo

The Flue agents service lives in `apps/agents/` (a standalone Bun package, not a Cargo or Bun workspace member). The CLI is installed there, so run it from `apps/agents/`:

```bash
./node_modules/.bin/flue docs                    # list every page
./node_modules/.bin/flue docs read <path>        # print a page as markdown
./node_modules/.bin/flue docs search "<query>"   # full-text search (JSON results)
```

Repo conventions on top of Flue:

- Macro-specific tool wrappers: see `apps/agents/CONTRIBUTING.md` (defineMacroTool / bindMacroTools, ledger event emission).
- Every agent turn must be recorded in Macro's session ledger via `MacroSessionContext` (`apps/agents/src/sessions/macro-session.ts`).
- Agents authenticate to Macro with scoped `mat_...` tokens; never embed user credentials.
- Verify code with `bun run check` and `bun run build` from `apps/agents/`.

## Documentation Catalog

<!-- flue-docs-catalog:start -->

```text
cli/add -- flue add
  Reference for fetching blueprint implementation guides.
cli/docs -- flue docs
  Browse the documentation bundled with the Flue CLI — list every page, print one as markdown, or search the full text.
cli/init -- flue init
  Reference for scaffolding a new Flue project, interactively or with flags.
cli/overview -- CLI
  The flue command-line interface — invocation, command catalog, global flags, and exit codes.
cli/run -- flue run
  Reference for running one agent module locally, transport-free, from the command line.
cli/update -- flue update
  Reference for fetching a blueprint guide that brings an existing integration up to the current version.
ecosystem/channels/discord -- Discord
ecosystem/channels/github -- GitHub
ecosystem/channels/google-chat -- Google Chat
ecosystem/channels/intercom -- Intercom
ecosystem/channels/linear -- Linear
ecosystem/channels/messenger -- Facebook Messenger
ecosystem/channels/notion -- Notion
ecosystem/channels/resend -- Resend
ecosystem/channels/salesforce-marketing-cloud -- Salesforce Marketing Cloud
ecosystem/channels/shopify -- Shopify
ecosystem/channels/slack -- Slack
ecosystem/channels/stripe -- Stripe
ecosystem/channels/teams -- Microsoft Teams
ecosystem/channels/telegram -- Telegram
ecosystem/channels/twilio -- Twilio
ecosystem/channels/whatsapp -- WhatsApp
ecosystem/channels/zendesk -- Zendesk
ecosystem/databases/libsql -- libSQL
ecosystem/databases/mongodb -- MongoDB
ecosystem/databases/mysql -- MySQL
ecosystem/databases/postgres -- Postgres
ecosystem/databases/redis -- Redis
ecosystem/databases/supabase -- Supabase
ecosystem/databases/turso -- Turso
ecosystem/databases/valkey -- Valkey
ecosystem/deploy/aws -- Deploy Agents on AWS
ecosystem/deploy/cloudflare -- Deploy to Cloudflare
ecosystem/deploy/docker -- Deploy Agents with Docker
ecosystem/deploy/fly -- Deploy Agents on Fly.io
ecosystem/deploy/github-actions -- Build Agents for GitHub Actions
ecosystem/deploy/gitlab-ci -- Build Agents for GitLab CI/CD
ecosystem/deploy/node -- Deploy Agents on Node.js
ecosystem/deploy/railway -- Deploy Agents on Railway
ecosystem/deploy/render -- Deploy Agents on Render
ecosystem/deploy/sst -- Deploy Agents on SST
ecosystem/sandboxes/boxd -- boxd
ecosystem/sandboxes/cloudflare -- Cloudflare Sandbox
ecosystem/sandboxes/cloudflare-computer -- Cloudflare Computer
ecosystem/sandboxes/daytona -- Daytona
ecosystem/sandboxes/e2b -- E2B
ecosystem/sandboxes/exedev -- exe.dev
ecosystem/sandboxes/islo -- islo
ecosystem/sandboxes/mirage -- Mirage
ecosystem/sandboxes/modal -- Modal
ecosystem/sandboxes/vercel -- Vercel Sandbox
ecosystem/tooling/braintrust -- Braintrust
ecosystem/tooling/jetty -- Jetty
ecosystem/tooling/opentelemetry -- OpenTelemetry
ecosystem/tooling/sentry -- Sentry
ecosystem/tooling/vitest-evals -- Vitest Evals
guide/agent-hooks -- Agent Hooks
  Compose an agent's capabilities — model, tools, skills, state, and lifecycle — with Flue's hook primitives.
guide/building-agents -- Agents
  Create an agent, configure its capabilities, and send it messages over time.
guide/channels -- Channels
  Receive verified provider events into agent conversations, and reply through the provider's own SDK.
guide/cloudflare-target -- Cloudflare
  Understand the Cloudflare-specific runtime behavior and APIs for Flue applications.
guide/database -- Database
  Configure where Flue durably stores agent conversations, from the in-memory default to SQLite, Postgres, and beyond.
guide/deploy -- Deploy
  Build your Flue application into a deployable artifact and ship it to the Node.js or Cloudflare target.
guide/durability -- Durability
  The accepted-work contract — what survives crashes, restarts, and redeploys, and how interrupted agent work recovers.
guide/evals -- Evals
  Test agent behavior by running an agent against a live model and asserting on what it does.
guide/getting-started -- Getting Started
  Set up a Flue project automatically or create your first agent manually.
guide/mcp -- MCP
  Connect agents to remote MCP servers and mount their tools.
guide/migration -- Migration Guide
  Upgrade an application from Flue 1.0.0-beta.x to v2.0.0 — build, routing, agents, tools, workflows, SDK, and deployment.
guide/models -- Models
  Choose, tune, and connect the LLM that powers your agent with the useModel hook.
guide/node-target -- Node.js
  Understand the Node.js-specific runtime behavior and APIs for Flue applications.
guide/observability -- Observability
  Observe agent activity through the runtime event stream — model turns, tool calls, logs, and token usage — and export telemetry.
guide/project-layout -- Project Layout
  Understand the source files and generated output in a Flue project.
guide/react -- React
  Build React interfaces for live agent conversations.
guide/routing -- Routing
  Mount agents, channels, and custom routes explicitly in app.ts.
guide/sandboxes -- Sandboxes
  Give your agent a workspace — the filesystem and shell where it reads, writes, and runs commands.
guide/schedules -- Schedules
  Deliver input to your agents on a cron schedule, on Node.js and Cloudflare.
guide/skills -- Skills
  Teach agents reusable expertise with progressively disclosed instructions and supporting files.
guide/subagents -- Subagents
  Delegate focused work to isolated child agents with useSubagent, defineSubagent, and the built-in GeneralSubagent.
guide/tools -- Tools
  Give agents the ability to call your application code and act on external systems.
guide/why-flue -- Why Flue?
  Build autonomous AI agents with a programmable TypeScript harness, and run them anywhere.
guide/workflows -- Workflows
  Driving agents from programs — one-shot CLI runs and CI jobs, standalone scripts, the SDK, and durable workflows.
reference/agent-api -- Agent API
  The agent module contract and the programmatic surface around agents — agent functions, statics, dispatch(), init().
reference/agent-behavior -- Agent Behavior
  How an agent behaves when you run it — the default tools, environment, message handling, context rules, and limits.
reference/agent-hooks-api -- Agent Hooks API
  Every hook callable during an agent render — useModel through the event hooks — and the render contract that governs them.
reference/configuration -- Configuration
  Reference for the flue.config file, the flue() Vite plugin and its option merging, target detection, and how vite dev works.
reference/data-persistence-api -- Data Persistence API
  The persistence adapter contract — PersistenceAdapter, the three store interfaces, the cross-cutting storage rules.
reference/errors -- Errors Reference
  The FlueError hierarchy, stable error type codes, the HTTP error envelope, settlement errors, and error classification.
reference/events -- Events Reference
  The runtime event vocabulary — the observe() and instrument() registration contracts, the event envelope, every event type.
reference/provider-api -- Provider API
  Reference for the providers config, setProvider(), model resolution, and the Cloudflare AI binding provider.
reference/sandbox-api -- Sandbox Adapter API
  The contract for building a sandbox adapter — SandboxFactory, Sandbox, SandboxDriver, the adapter tool factory.
reference/streaming-protocol -- Streaming Protocol
  The HTTP wire protocol for agent conversation reads and writes.
sdk/create-flue-client -- createFlueClient(...)
  Constructing a Flue Agent SDK client — URL semantics, fetch override, headers, and token.
sdk/errors -- Errors
  The Flue Agent SDK error classes, the HTTP error envelope, and how to discriminate failures.
sdk/events -- Events
  How the Flue Agent SDK event and conversation types map to their reference pages, plus the SDK-owned event stream.
sdk/flue-client -- FlueClient
  The Flue Agent SDK conversation client — send(), read(), wait(), abort(), history(), observe(), and attachmentUrl().
sdk/overview -- Flue Agent SDK
  The Flue Agent SDK (@flue/sdk) — installation, a minimal round trip, the HTTP surface it wraps, and a map of the SDK.
```

<!-- flue-docs-catalog:end -->
