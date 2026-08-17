import { defineConfig } from '@flue/runtime/config';

/**
 * Flue runtime configuration for the Macro agents service.
 *
 * Durability: conversations persist in Postgres when AGENTS_DATABASE_URL is
 * set (production); otherwise Flue falls back to its local cache, which is
 * fine for `flue run` experiments but must never be relied on in deploys.
 * This database holds Flue's conversation state only — the system of record
 * (ledger, sessions, escalations) lives behind Macro's Rust APIs.
 */
export default defineConfig({
  target: 'node',
});
