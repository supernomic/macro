/**
 * Typed client for Macro's escalation API
 * (`crates/escalations/src/inbound/axum_router.rs`, mounted on the
 * document-cognition-service).
 *
 * Like the ledger client, this is deliberately not part of `@macro/sdk`:
 * these endpoints authenticate with agent bearer tokens (`mat_...`).
 */

import { LedgerApiError } from '../ledger/client.ts';

/** Escalation urgency. */
export type EscalationPriority = 'low' | 'normal' | 'high' | 'urgent';

/** Escalation lifecycle status. */
export type EscalationStatus = 'open' | 'claimed' | 'resolved' | 'cancelled';

/** An escalation as returned by Macro. */
export interface Escalation {
  id: string;
  org_id: number | null;
  domain: string;
  session_id: string | null;
  requester_user_id: string | null;
  requester_display: string;
  source_channel: string | null;
  title: string;
  summary: string;
  tags: string[];
  priority: EscalationPriority;
  status: EscalationStatus;
  assignee_user_id: string | null;
  assignee_team_id: string | null;
  callback_url: string | null;
  resolution: string | null;
  resolved_by: string | null;
  created_at: string;
  claimed_at: string | null;
  resolved_at: string | null;
}

/** Request to create an escalation. */
export interface CreateEscalationRequest {
  domain: string;
  session_id?: string;
  requester_user_id?: string;
  requester_display: string;
  source_channel?: string;
  title: string;
  summary: string;
  tags?: string[];
  priority?: EscalationPriority;
  callback_url?: string;
}

/**
 * Client for the agent-facing escalation endpoints. One instance per agent
 * principal; requires the `escalation:create` / `escalation:query` scopes.
 */
export class EscalationsClient {
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

  /** Create an escalation; Macro routes it to an expert or team queue. */
  create(request: CreateEscalationRequest): Promise<Escalation> {
    return this.request<Escalation>('POST', '/agent-escalations', request);
  }

  /** Poll one escalation's status. */
  get(id: string): Promise<Escalation> {
    return this.request<Escalation>(
      'GET',
      `/agent-escalations/${encodeURIComponent(id)}`,
    );
  }
}
