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
import { ApprovalsClient } from '../approvals/client.ts';
import { config } from '../config.ts';
import { EscalationsClient } from '../escalations/client.ts';
import { FeedbackClient } from '../feedback/client.ts';
import { GraphClient } from '../graph/client.ts';
import { LedgerClient } from '../ledger/client.ts';
import type {
  ExternalThreadKind,
  LedgerEvent,
  NewLedgerEvent,
  SessionMapping,
} from '../ledger/events.ts';
import { macroClientFor } from '../macro/client.ts';
import { type SkillCatalogEntry, SkillsClient } from '../skills/client.ts';

/** Identity + plumbing for one agent principal. */
export interface AgentRuntime {
  /** Stable slug of the agent principal (e.g. `super-agent`). */
  agentSlug: string;
  /** Macro SDK client authenticated as this principal. */
  macro: Macro;
  /** Ledger client authenticated as this principal. */
  ledger: LedgerClient;
  /** Escalations client authenticated as this principal. */
  escalations: EscalationsClient;
  /** Approval-gate client authenticated as this principal. */
  approvals: ApprovalsClient;
  /** Skills-governance client authenticated as this principal. */
  skills: SkillsClient;
  /** Feedback sidecar client authenticated as this principal. */
  feedback: FeedbackClient;
  /** Entity-graph client authenticated as this principal. */
  graph: GraphClient;
  /** Cached governed-skill catalog (filled asynchronously after boot). */
  skillsCatalog: SkillCatalogEntry[];
  /** Conversation ids that already have a `request_header` pin. */
  pinnedHeaderConversations: Set<string>;
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
  /** Flue conversation id this session is bound to. */
  readonly conversationId: string;
  private readonly externalThread?: {
    kind: ExternalThreadKind;
    key: string;
  };
  private mapping: Promise<SessionMapping> | undefined;
  private appendChain: Promise<unknown> = Promise.resolve();
  /** Skill id → version already logged as `skill_injected` on this session. */
  private readonly injectedSkillVersions = new Map<string, string>();

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

  /**
   * Append `skill_injected` the first time this conversation sees a given
   * skill version. Re-logs if the catalog version changes.
   */
  recordSkillInjection(skill: SkillCatalogEntry): void {
    if (this.injectedSkillVersions.get(skill.id) === skill.version) {
      return;
    }
    this.injectedSkillVersions.set(skill.id, skill.version);
    void this.append({
      payload: {
        type: 'skill_injected',
        data: { skill_id: skill.id, version: skill.version },
      },
      actor_kind: 'agent',
      actor_id: this.runtime.agentSlug,
    });
  }

  /**
   * Pin this conversation's composition on the ledger once. Eval grouping
   * and training export join on `composition_id`; the snapshot is the
   * reconstructable request header for this agent definition.
   */
  pinRequestHeader(): void {
    if (this.runtime.pinnedHeaderConversations.has(this.conversationId)) {
      return;
    }
    this.runtime.pinnedHeaderConversations.add(this.conversationId);
    const skillVersions: Record<string, string> = {};
    for (const skill of this.runtime.skillsCatalog) {
      skillVersions[skill.slug] = skill.version;
    }
    void this.append({
      payload: {
        type: 'request_header',
        data: {
          rendered_system_prompt: `composition:${this.runtime.compositionId}`,
          tool_schemas: [],
          provider: 'flue',
          model: config.superAgentModel,
          sampling: {},
          skill_versions: skillVersions,
          composition_id: this.runtime.compositionId,
        },
      },
      actor_kind: 'agent',
      actor_id: this.runtime.agentSlug,
    });
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
      escalations: new EscalationsClient({
        baseUrl: config.ledgerBaseUrl,
        token: spec.token,
      }),
      approvals: new ApprovalsClient({
        baseUrl: config.ledgerBaseUrl,
        token: spec.token,
      }),
      skills: new SkillsClient({
        baseUrl: config.ledgerBaseUrl,
        token: spec.token,
      }),
      feedback: new FeedbackClient({
        baseUrl: config.ledgerBaseUrl,
        token: spec.token,
      }),
      graph: new GraphClient({
        baseUrl: config.ledgerBaseUrl,
        token: spec.token,
      }),
      skillsCatalog: [],
      pinnedHeaderConversations: new Set<string>(),
      compositionId: spec.compositionId,
    };
    runtimes.set(spec.agentSlug, runtime);
    const built = runtime;
    void built.skills
      .catalog()
      .then((entries) => {
        built.skillsCatalog = entries;
      })
      .catch(() => {
        // Catalog is best-effort. A failed fetch leaves the agent running
        // without governed skills until process restart.
      });
  }
  return runtime;
}

/** Look up the pinned composition id for a live agent slug, if mounted. */
export function compositionIdFor(
  agentSlug: string | undefined,
): string | undefined {
  if (!agentSlug) {
    return undefined;
  }
  return runtimes.get(agentSlug)?.compositionId;
}

/** The super agent's runtime (token from `MACRO_SUPER_AGENT_TOKEN`). */
export function superAgentRuntimeInstance(): AgentRuntime {
  return runtimeFor({
    agentSlug: 'super-agent',
    token: config.superAgentToken,
    compositionId: config.superAgentCompositionId,
  });
}
