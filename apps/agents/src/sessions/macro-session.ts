/**
 * Session mapping between Flue conversations and Macro sessions.
 *
 * One Flue conversation ↔ one Macro session (UUID), created through the
 * session-map API before the first ledger append. Never invent session ids
 * locally. External threads (Slack `channel:thread_ts`, email thread ids)
 * anchor the mapping so re-entry from the same thread resumes the same
 * durable conversation.
 */

import type { Macro } from '@macro/sdk';
import { config } from '../config.ts';
import { LedgerClient } from '../ledger/client.ts';
import type {
  ExternalThreadKind,
  LedgerEvent,
  NewLedgerEvent,
  SessionMapping,
} from '../ledger/events.ts';
import { macroClientFor } from '../macro/client.ts';

/** Identity + plumbing for one agent principal. */
export interface AgentRuntime {
  /** Stable slug of the agent principal (e.g. `super-agent`). */
  agentSlug: string;
  /** Macro SDK client authenticated as this principal. */
  macro: Macro;
  /** Ledger client authenticated as this principal. */
  ledger: LedgerClient;
  /** Pinned composition id for this agent definition. */
  compositionId: string;
}

/**
 * Context bound to one Flue conversation: lazily opens the Macro session on
 * first use and appends ledger events on behalf of the agent.
 *
 * Ledger emission is best-effort ordered: appends are serialized through an
 * internal promise chain so events land in emission order even when emitters
 * don't await each other.
 */
export class MacroSessionContext {
  readonly runtime: AgentRuntime;
  private readonly conversationId: string;
  private readonly externalThread?: {
    kind: ExternalThreadKind;
    key: string;
  };
  private mapping: Promise<SessionMapping> | undefined;
  private appendChain: Promise<unknown> = Promise.resolve();

  constructor(opts: {
    runtime: AgentRuntime;
    conversationId: string;
    externalThread?: { kind: ExternalThreadKind; key: string };
  }) {
    this.runtime = opts.runtime;
    this.conversationId = opts.conversationId;
    this.externalThread = opts.externalThread;
  }

  /** Open (or resume) the Macro session for this conversation. */
  session(): Promise<SessionMapping> {
    this.mapping ??= this.runtime.ledger.openSession({
      runtimeConversationId: this.conversationId,
      externalThread: this.externalThread,
    });
    return this.mapping;
  }

  /**
   * Append events to this conversation's ledger, serialized in emission
   * order, returning the stored rows (with assigned `seq` and hash).
   * Ledger failures propagate — a session whose trace cannot be recorded
   * must not silently keep acting ("model-visible means logged").
   */
  append(...events: NewLedgerEvent[]): Promise<LedgerEvent[]> {
    const next = this.appendChain.then(async () => {
      const session = await this.session();
      return this.runtime.ledger.appendEvents(session.session_id, events);
    });
    // Keep the chain alive after a failure so later appends still run;
    // the failed append's caller still sees its own rejection.
    this.appendChain = next.catch(() => {});
    return next;
  }
}

const contexts = new Map<string, MacroSessionContext>();

/**
 * Get (or create) the session context for one conversation. Keyed by agent
 * slug + conversation id, so two agents never share a Macro session.
 */
export function sessionContextFor(opts: {
  runtime: AgentRuntime;
  conversationId: string;
  externalThread?: { kind: ExternalThreadKind; key: string };
}): MacroSessionContext {
  const key = `${opts.runtime.agentSlug}:${opts.conversationId}`;
  let ctx = contexts.get(key);
  if (!ctx) {
    ctx = new MacroSessionContext(opts);
    contexts.set(key, ctx);
  }
  return ctx;
}

const runtimes = new Map<string, AgentRuntime>();

/**
 * Build (or reuse) the runtime for one agent principal. Each principal gets
 * one SDK client and one ledger client for the process lifetime, keyed by
 * slug.
 */
export function runtimeFor(spec: {
  agentSlug: string;
  token: () => string;
  compositionId: string;
}): AgentRuntime {
  let runtime = runtimes.get(spec.agentSlug);
  if (!runtime) {
    runtime = {
      agentSlug: spec.agentSlug,
      macro: macroClientFor(spec.token),
      ledger: new LedgerClient({
        baseUrl: config.ledgerBaseUrl,
        token: spec.token,
      }),
      compositionId: spec.compositionId,
    };
    runtimes.set(spec.agentSlug, runtime);
  }
  return runtime;
}

/** The super agent's runtime (token from `MACRO_SUPER_AGENT_TOKEN`). */
export function superAgentRuntimeInstance(): AgentRuntime {
  return runtimeFor({
    agentSlug: 'super-agent',
    token: config.superAgentToken,
    compositionId: config.superAgentCompositionId,
  });
}
