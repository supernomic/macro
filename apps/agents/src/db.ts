/**
 * Flue persistence entry (auto-discovered as `src/db.ts`, Node hosts only).
 *
 * With AGENTS_DATABASE_URL set, conversations persist in Postgres — this
 * database holds Flue's conversation state only; the system of record
 * (ledger, sessions, escalations) lives behind Macro's Rust APIs. Without
 * it (local `flue run` experiments), the runtime falls back to its local
 * cache. Deploys must always set AGENTS_DATABASE_URL: local-cache
 * durability does not survive hosts.
 */

import { type PostgresQuery, postgres } from '@flue/postgres';
import sql from 'postgres';

function makeAdapter() {
  const url = process.env.AGENTS_DATABASE_URL;
  if (!url) {
    return undefined;
  }
  const db = sql(url);
  return postgres({
    query: (text, params) => db.unsafe(text, params ?? []),
    transaction: <T>(fn: (tx: { query: PostgresQuery }) => Promise<T>) =>
      db.begin((tx) =>
        fn({ query: (text, params) => tx.unsafe(text, params ?? []) }),
      ) as Promise<T>,
    close: () => db.end(),
  });
}

export default makeAdapter();
