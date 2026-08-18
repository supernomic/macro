/**
 * Domain agent registry.
 *
 * A domain agent is a specialist delegate the super agent hands focused
 * work to (a Flue subagent: fresh context, own instructions, own tools;
 * only its final answer returns to the trunk conversation).
 *
 * Each domain runs under its OWN Macro agent principal. The domain's
 * `mat_...` token carries exactly the scopes its tool allowlist needs, so
 * the allowlist is enforced at Macro's API boundary — mounting a tool the
 * token cannot exercise fails server-side by design. A domain whose token
 * is absent from the environment is simply not offered to the model.
 *
 * Delegate tool calls are recorded on the parent conversation's ledger
 * session (one thread of work, one trace of record) under the domain
 * agent's actor id, so the trace shows who did what.
 */

import { config } from '../config.ts';
import { type AgentRuntime, runtimeFor } from '../sessions/macro-session.ts';
import type { MacroToolDef } from '../tools/toolkit.ts';
import { techOps } from './techops.ts';

/** Declaration of one domain agent. */
export interface DomainAgentSpec {
  /**
   * Principal slug; also the subagent name the model delegates to, and the
   * key for the `MACRO_<SLUG>_AGENT_TOKEN` environment variable.
   */
  slug: string;
  /**
   * One-line delegation description. This is the model's only routing
   * signal when deciding whether to hand work to this domain — write it
   * like a good tool description.
   */
  description: string;
  /**
   * Prompt overlay: the delegate's full instructions. Delegates start with
   * a fresh context, so these must stand alone (the task prompt from the
   * super agent is the only other input).
   */
  instructions: string;
  /** Domain tool allowlist, bound per-conversation at delegation time. */
  tools: readonly MacroToolDef[];
  /**
   * Pinned composition id for this domain definition. Bump on any change
   * to instructions, tools, or model — evals and training export group
   * by it.
   */
  compositionId: string;
  /**
   * Optional model override (e.g. a cheaper tier for high-volume
   * classification domains). Inherits the super agent's model when absent.
   */
  model?: string;
}

/** Every declared domain agent, whether or not its token is configured. */
export const DOMAIN_AGENTS: readonly DomainAgentSpec[] = [techOps];

/**
 * The runtime for a domain agent, or `undefined` when its principal token
 * is not configured (which disables the domain).
 */
export function domainRuntime(spec: DomainAgentSpec): AgentRuntime | undefined {
  const token = config.domainAgentToken(spec.slug);
  if (!token) {
    return undefined;
  }
  return runtimeFor({
    agentSlug: spec.slug,
    token: () => token,
    compositionId: spec.compositionId,
  });
}
