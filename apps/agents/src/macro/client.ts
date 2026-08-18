/**
 * Macro SDK client factory for agent principals.
 *
 * All domain access (documents, channels, email, search, ...) goes through
 * `@macro/sdk`; never `fetch` a Macro endpoint directly from a tool. If the
 * SDK is missing an endpoint, add it there first
 * (`.claude/skills/add-sdk-endpoint/SKILL.md`).
 */

import { Macro } from '@macro/sdk';
import { config } from '../config.ts';

/**
 * Build a Macro SDK client authenticated as an agent principal.
 *
 * Agent tokens (`mat_...`) ride the standard bearer slot; Macro's API
 * boundary resolves the principal and enforces the token's scopes on every
 * call — the runtime cannot exceed them.
 */
export function macroClientFor(token: () => string): Macro {
  return new Macro({
    auth: { type: 'user', token },
    env: config.macroEnv,
  });
}
