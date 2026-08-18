/**
 * Typed client for Macro's agent-ledger API
 * (`crates/agent_ledger/src/inbound/axum_router.rs`, mounted on the
 * document-cognition-service).
 *
 * This is deliberately not part of `@macro/sdk`: the ledger endpoints
 * authenticate with agent bearer tokens (`mat_...`), a scheme the SDK's
 * user/bot auth does not carry. Domain tools still go through `@macro/sdk`;
 * this client exists only for instrumentation plumbing.
 */

import type {
  ExternalThreadKind,
  LedgerEvent,
  NewLedgerEvent,
  SessionMapping,
  SessionOutcome,
} from './events.ts';

/** Error raised when a ledger API call fails. */
export class LedgerApiError extends Error {
  /** HTTP status of the failed call. */
  readonly status: number;

  constructor(status: number, message: string) {
    super(`ledger API error (${status}): ${message}`);
    this.name = 'LedgerApiError';
    this.status = status;
  }
}

/** Options for opening (or resuming) a session. */
export interface OpenSessionOptions {
  /** The Flue conversation id. */
  runtimeConversationId: string;
  /** External thread anchor, when the conversation is thread-keyed. */
  externalThread?: { kind: ExternalThreadKind; key: string };
}

/**
 * Client for the agent-facing ledger endpoints. One instance per agent
 * principal; the token decides which sessions it may touch.
 */
export class LedgerClient {
  private readonly baseUrl: string;
  private readonly token: () => string;

  constructor(opts: { baseUrl: string; token: () => string }) {
    this.baseUrl = opts.baseUrl.replace(/\/$/, '');
    this.token = opts.token;
  }

  private async request<T>(
    method: 'GET' | 'POST',
    path: string,
    body?: unknown,
  ): Promise<T> {
    const res = await fetch(`${this.baseUrl}${path}`, {
      method,
      headers: {
        authorization: `Bearer ${this.token()}`,
        ...(body !== undefined ? { 'content-type': 'application/json' } : {}),
      },
      body: body !== undefined ? JSON.stringify(body) : undefined,
    });
    if (!res.ok) {
      const text = await res.text().catch(() => '');
      throw new LedgerApiError(res.status, text || res.statusText);
    }
    return (await res.json()) as T;
  }

  /**
   * Open a session for a runtime conversation, or return the existing
   * mapping when one is already registered for that conversation id.
   */
  openSession(opts: OpenSessionOptions): Promise<SessionMapping> {
    return this.request<SessionMapping>('POST', '/agent-ledger/sessions', {
      runtime_conversation_id: opts.runtimeConversationId,
      external_thread_kind: opts.externalThread?.kind,
      external_thread_key: opts.externalThread?.key,
    });
  }

  /**
   * Look up the session anchored to an external thread (Slack thread,
   * email thread, ...), so re-entry resumes the same durable conversation.
   * Resolves `null` when no session is anchored there.
   */
  async findSessionByThread(
    kind: ExternalThreadKind,
    key: string,
  ): Promise<SessionMapping | null> {
    try {
      const params = new URLSearchParams({ kind, key });
      return await this.request<SessionMapping>(
        'GET',
        `/agent-ledger/sessions/by-thread?${params}`,
      );
    } catch (e) {
      if (e instanceof LedgerApiError && e.status === 404) {
        return null;
      }
      throw e;
    }
  }

  /** Append events, in order, to a session's ledger. */
  appendEvents(
    sessionId: string,
    events: NewLedgerEvent[],
  ): Promise<LedgerEvent[]> {
    return this.request<LedgerEvent[]>(
      'POST',
      `/agent-ledger/sessions/${sessionId}/events`,
      { events },
    );
  }

  /** List a session's events starting at `from_seq` (default 0). */
  listSessionEvents(
    sessionId: string,
    opts?: { fromSeq?: number; limit?: number },
  ): Promise<LedgerEvent[]> {
    const params = new URLSearchParams();
    if (opts?.fromSeq !== undefined) {
      params.set('from_seq', String(opts.fromSeq));
    }
    if (opts?.limit !== undefined) {
      params.set('limit', String(opts.limit));
    }
    const qs = params.size > 0 ? `?${params}` : '';
    return this.request<LedgerEvent[]>(
      'GET',
      `/agent-ledger/sessions/${sessionId}/events${qs}`,
    );
  }

  /** Record (upsert) the terminal outcome projection for a session. */
  async recordOutcome(
    sessionId: string,
    outcome: SessionOutcome,
    summary?: string,
  ): Promise<void> {
    await this.request<unknown>(
      'POST',
      `/agent-ledger/sessions/${sessionId}/outcome`,
      { outcome, summary },
    );
  }

  /**
   * Query events across the agent's organization (requires the
   * `ledger:query` scope). Backs the `query_my_history` tool.
   */
  queryOrgEvents(opts?: {
    sessionId?: string;
    /** Stored discriminants, e.g. `tool/call` (comma-joined on the wire). */
    eventTypes?: string[];
    actorId?: string;
    limit?: number;
  }): Promise<LedgerEvent[]> {
    const params = new URLSearchParams();
    if (opts?.sessionId) {
      params.set('session_id', opts.sessionId);
    }
    if (opts?.eventTypes?.length) {
      params.set('event_types', opts.eventTypes.join(','));
    }
    if (opts?.actorId) {
      params.set('actor_id', opts.actorId);
    }
    if (opts?.limit !== undefined) {
      params.set('limit', String(opts.limit));
    }
    const qs = params.size > 0 ? `?${params}` : '';
    return this.request<LedgerEvent[]>(
      'GET',
      `/agent-ledger/agent-events${qs}`,
    );
  }
}
