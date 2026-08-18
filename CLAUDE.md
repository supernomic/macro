# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Architecture Overview

This is a Rust-based cloud storage microservices architecture built as a Cargo workspace with 80+ crates. The system
handles document storage, processing, search, communication, and email functionality.
When making changes, make sure to test the services individually before committing using `cargo test -p {my_service}`
from the repository root.

### Key Services

**Core Storage Services:**

- `document-storage-service`: Main document storage API
- `document-cognition-service`: Document analysis and processing
- `search_service`: Search functionality across documents
- `static_file_service`: Static file serving

**Processing Services:**

- `convert_service`: Document format conversion
- `document-text-extractor`: Text extraction from documents
- `search_processing_service`: Search indexing and processing

**Communication Services:**

- `email_service`: Email processing and management
- `notification_service`: User notifications

**Infrastructure Services:**

- `authentication_service`: User authentication
- `connection_gateway`: WebSocket gateway
- `contacts_service`: Contact management

### Data Storage

The system uses multiple databases:

- **MacroDB**: Main PostgreSQL database for documents, users, projects, Communication data (messages, channels,
  participants), Email threads, messages, metadata, and notification preferences/history
- **ContactsDB**: User connections and contacts

External storage includes S3 for document files, Redis for caching, OpenSearch for search indexing, and DynamoDB for
connection tracking.

### MacroDB Schema Changes

DB migration files are located in `crates/macro_db_client`. Use the `/dump-schema` skill to dump the current Postgres schema for reference.
If you are still getting migration errors after running `just setup_macrodb`, you may need to run `just force_drop_db`
in `crates/macro_db_client` to drop the database and re-create it
with `just setup_macrodb` at the repository root. Remember that some database table and column names may be
camelCased rather than snake_cased (use `/dump-schema` or check the migration files for actual column names).
When a column is camelCased, you need to cast it as the snake_cased version when reading from the database. E.g.
`SELECT "userId" as "user_id" FROM "UserInsights"`.
Any time you make changes to the SQL code in rust, you need to run `just prepare_db` to
update the `.sqlx` directory. Always run it inside `nix develop` and **only** from the
repository root (for example, `nix develop --command just prepare_db`) — do not run it from
individual crate directories anymore. The workspace-level recipe handles every crate that
has sqlx queries.

## Development Commands

### Alias Commands (IMPORTANT)

Use `\cd` instead of `cd` to navigate in the repository.

### Building

```bash
just build                    # Build all services
just build_lambdas           # Build all Lambda functions
just check                   # Type check without building
```

### Testing

```bash
# Setup test environment
docker compose --project-directory . -f docker/docker-compose.yml up -d postgres
just setup_test_envs         # Setup .env files for tests
just initialize_dbs          # Initialize all databases
just test                    # Run tests
```

### Pre Commit
```bash
cargo fmt                   # format
just clippy                 # extra lints / best practices
```

### Database Management

```bash
just setup_macrodb           # Setup main database
just setup_commsdb           # Setup communications database
just setup_emaildb           # Setup email database
just setup_contactsdb        # Setup contacts database
```

### Lambda Building

Individual lambda builds available for:

- `build_document_text_extractor`
- `build_docx_unzip_handler`
- `build_delete_chat_handler`
- `build_upload_extractor_lambda_handler`
- `build_email_suppression`
- `build_deleted_item_poller`

## Key Architectural Patterns

### Service Communication

Services communicate via:

- HTTP APIs (internal service clients)
- SQS queues for async processing
- Lambda triggers for event-driven processing
- Redis for caching and session management

### Database Architecture

- Each service has its own database client crate (e.g., `macro_db_client`, `comms_db_client`)
- Uses SQLx for database interactions with offline query validation
- Migrations managed per service

### AWS Integration

Heavy use of AWS services:

- S3 for file storage
- Lambda for serverless processing
- SQS for message queuing
- DynamoDB for connection tracking
- OpenSearch for search capabilities

###
Environment variables are managed in doppler. New env vars should be added to doppler. All environment variables should
_always_ be loaded with the macros in the macro_env_var crate. They should never be loaded with std::env::var.

## Development Notes

### Prerequisites

- Docker (for local databases)
- `sqlx-cli` for database migrations
- `just` for task running
- Pulumi CLI for infrastructure
- AWS CLI for deployment

### Offline Development

The project uses `SQLX_OFFLINE=true` for building without database connections. Database queries are pre-validated and
cached.

### Document Processing Pipeline

Documents go through: Upload → Text Extraction → Search Indexing → Storage → Retrieval

- DOCX files are unzipped via Lambda
- PDFs processed with pdfium library
- Text indexed in OpenSearch
- Metadata stored in PostgreSQL

## Case Study: Implementing Generic Entity Mentions

This case study documents the process of extending message mentions to support generic entity mentions (e.g., documents
mentioning other documents).

### Task Understanding & Planning

1. **Analyzed Requirements**: Extended existing MessageMention functionality to support any entity mentioning any other
   entity
2. **Created Todo List**: Used TodoWrite tool to track implementation steps
3. **Examined Existing Code**: Reviewed current message_mentions table structure and usage

### Implementation Steps

1. **Data Model Changes**
    - Created `EntityMention` struct with generic source/target fields
    - Maintained backward compatibility with existing mentions

2. **Database Migration**
    - Renamed `message_mentions` → `entity_mentions`
    - Added `source_entity_type` and `source_entity_id` columns
    - Migrated existing data (messages) to new structure
    - Updated all indexes for performance

3. **Updated Database Client**
    - Created `entity_mentions` module with create/delete functions
    - Modified `get_attachment_references` to query new table
    - Updated `create_message_mentions` to insert into new table
    - Fixed test fixtures to use new table structure

4. **API Endpoints**
    - Created POST/DELETE `/entity-mentions` endpoints
    - Used proper Extension extractors for axum handlers
    - Added OpenAPI documentation

### Testing & Debugging

1. **Compilation Issues**
    - Fixed import errors (wrong Context type, missing http import)
    - Added Clone trait to structs used in tests
    - Updated fixture references from `message_mentions` to `mentions`

2. **Test Failures**
    - Fixed `create_message_mentions` test by updating query logic
    - Query now returns all mentioned users, not just newly inserted ones
    - Updated fixtures to include entity_mentions data

3. **SQLX Offline Mode**
    - Encountered "no cached data" errors due to schema changes
    - Required running migrations before `cargo sqlx prepare`

### Database Preparation

1. Run migrations: `just migrate_db`
2. Update SQLX cache: `just prepare_db`
3. Verify with tests: `cargo test`

### Key Learnings

1. **Todo Management**: Proactive use of TodoWrite helps track complex multi-step tasks
2. **Incremental Testing**: Run tests frequently to catch issues early
3. **Fixture Management**: Update test fixtures when changing table structures
4. **SQLX Workflow**: Schema changes require migration → prepare → test cycle
5. **Axum Patterns**: Handlers take shared services via `State`, not `Extension` (see docs/STYLE_GUIDE.md CS-30; this case study predates that convention)

### Index Strategy

The migration included comprehensive indexes:

- Composite index on (entity_type, entity_id) for efficient lookups
- Index on source columns for reverse lookups
- Index on created_at for ordering
- Maintained existing performance optimizations

## Development Best Practices

### Database Query Management

- Prefer SQLx compile-time checked macros (`query!`, `query_as!`, `query_scalar!`) for database queries whenever possible instead of dynamic `sqlx::query` calls.
- Never manually create or edit `.sqlx/query-*.json` files. To update SQLx query metadata, run `just prepare_db` from the repository root.
- When creating a new SQLx migration file, run `sqlx migrate add <descriptive_name>` from the relevant database crate (or use SQLx's `--source` option) and then edit the generated file. Never manually create migration files, and never invent, copy, or guess timestamp prefixes to fake a migration filename.
- Always run tests between changes that involve changes to db queries
- Never run `cargo test` with `SQLX_OFFLINE=true`. Tests are designed to validate against the live local Postgres; offline mode forces sqlx macros to consult the cached `.sqlx` data and can either surface confusing "type annotations needed" errors when a query was not in the cache or hide regressions where a query no longer matches the schema. If tests fail with sqlx "no cached data" errors, run `just prepare_db` (with `--tests` when the failure is in test code) — do not flip offline mode on. `SQLX_OFFLINE=true` is fine for `cargo check` / `cargo build` / `cargo clippy` only.

### Rust Error Handling

- New code uses `rootcause` for error handling — it's preferred over `anyhow` (see docs/STYLE_GUIDE.md CS-46)
- In code still on anyhow: prefer `anyhow::bail!("error message")` over `Err(anyhow::anyhow!("error message"))` for early returns - it's more concise and idiomatic

### Documentation Requirements

- Add `#![deny(missing_docs)]` to `lib.rs` in new crates to enforce documentation on all public items
- This ensures all public functions, structs, enums, and modules have documentation comments
- Do not use `ignore` to except code blocks from doc tests unless explicitely directed


### Test File Organization

Place tests in a separate `test.rs` file within the same module directory, rather than inline with `#[cfg(test)]` blocks in the implementation file.

**Pattern:**
- Implementation: `foo/mod.rs` or `foo.rs`
- Tests: `foo/test.rs`

Note: You do NOT need to convert a file module (`foo.rs`) into a directory module (`foo/mod.rs`) to add tests.
Rust supports `foo.rs` alongside a `foo/` directory — just create `foo/test.rs` and it works as a submodule of `foo.rs`.

**Example structure:**
```
src/
  user.rs       # Contains: mod test;  (with #[cfg(test)]) + implementation
  user/
    test.rs     # Contains: use super::*; and test functions
```

**In `user.rs`:**
```rust
#[cfg(test)]
mod test;

// implementation code...
```

**In `test.rs`:**
```rust
use super::*;

#[tokio::test]
async fn test_something() {
    // test code
}
```

This keeps implementation files focused and makes tests easier to locate and maintain.

### Tracing

- Include `err` when adding the `tracing::instrument` attribute to functions that return `Result`. Do not include `err` on functions that return `Option`, `()`, or other non-`Result` types. Never include `level = "info"`.
- When including an error with a log, include it like so: `tracing::error!(error=?e, "error msg");`
don't inject it directly into the error message.
- Prefer using `.inspect_err` instead of `if let Err(e)` in order to do logging.

## Development Memories

### DB Crate Changes

- When making changes to a db crate you should always update tests, and run prepare

## Cursor Cloud specific instructions

This VM has no `systemd` (PID 1 is `tini`). Nix and Docker are installed into the VM
snapshot, but their daemons do not auto-start on boot. The startup/update script starts
`nix-daemon` and `dockerd` (backgrounded) before each session; if you ever hit
`cannot connect to socket ... /nix/var/nix/daemon-socket/socket` or
`Cannot connect to the Docker daemon`, start them yourself (idempotent):

```bash
sudo sh -c 'pgrep -x nix-daemon >/dev/null || nohup /nix/var/nix/profiles/default/bin/nix-daemon >/var/log/nix-daemon.log 2>&1 &'
sudo sh -c 'pgrep -x dockerd     >/dev/null || nohup dockerd >/var/log/dockerd.log 2>&1 &'
sudo chmod 666 /var/run/docker.sock   # dockerd resets socket perms on each start
```

All dev tooling (`just`, `cargo`, `bun`, `sqlx`, `zig`, `cargo-zigbuild`, `docker-compose`,
`pulumi`, `biome`, `cargo-nextest`, ...) comes only from the Nix dev shell. Run repo commands
via `nix develop --command <cmd>` from the repo root, or enter `nix develop` first. Outside the
dev shell only rustup's `cargo` and system `node` are on PATH. See `docs/RUNNING_LOCALLY.md` for
the human guide; the notes below are the non-obvious gotchas.

- Run the app (agents should prefer headless): `nix develop --command just stack up --no-doppler`.
  It builds the service binaries (host `cargo zigbuild`), builds the static frontend, brings up the
  full Docker stack, and returns when ready. Frontend/proxy is `http://localhost:8090/app/`
  (Caddy), FusionAuth `:9011`, Mailpit `http://localhost:8090/mailpit/`. Manage with
  `just stack status` / `just stack update` / `just stack down`. Interactive `just run_local
  --no-doppler` instead blocks on an `r`/`q` hotkey loop (needs a TTY) and serves the Vite dev
  server on `:3000` with Mailpit at `:8025`.
- GOTCHA (blocks `stack up`/`run_local`): the local stack starts an "SDK webhook relay" that shells
  out to `ssh`/`ssh-keygen` from PATH. The dev shell's `LD_LIBRARY_PATH` (Nix glibc 2.42) breaks the
  system `ssh-keygen` (`GLIBC_ABI_DT_X86_64_PLT not found`), so the run aborts at
  `ssh-keygen failed for the SDK webhook relay`. Fix: put a Nix `openssh` on PATH first (a
  gc-rooted one is installed via `nix profile add nixpkgs#openssh`):
  `export PATH="$(nix build --no-link --print-out-paths nixpkgs#openssh | grep -v -- '-man')/bin:$PATH"`
  then run `just stack up --no-doppler` in that same shell.
- GOTCHA (sccache): the dev shell sets `RUSTC_WRAPPER=sccache`, whose background server keeps
  stdout/stderr open. Piping a Rust build/`just` command into `tail`/`head` can look like a hang
  after it already finished. Redirect to a file (`> out.log 2>&1`) instead of piping.
- Login + seed: no accounts exist by default; passwordless login emails a one-time code to Mailpit
  (`http://localhost:8090/mailpit/` in stack mode, `:8025` in run_local). Seed a demo world:
  `nix develop --command just seed-scenario apply --file seed/scenarios/team-perms.json` (personas
  `alice`/`bob`/`carol`/`dave`/`erin`/`eve` `@seed.macro.local`). The printed login links use port
  3000 (the run_local default); in headless `stack` mode use 8090, e.g.
  `http://alice.localhost:8090/app/login?email=alice@seed.macro.local`.
- Preflight after starting daemons: `nix develop --command just doctor-local`.
- Tests hit the live local Postgres and must NOT use `SQLX_OFFLINE=true` (see above). With the stack
  up, `nix develop --command cargo test -p <crate>` works (dev shell sets `DATABASE_URL`).
- Ports: frontend/proxy 8090, FusionAuth 9011, Postgres 5432, Redis 6379, Kafka 9092,
  OpenSearch 9200, LocalStack 4566.
