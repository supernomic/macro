/**
 * Typed client for Macro's approval-gate API
 * (`crates/approvals/src/inbound/axum_router.rs`, mounted on the
 * document-cognition-service).
 *
 * Like the ledger and escalations clients, this is deliberately not part
 * of `@macro/sdk`: these endpoints authenticate with agent bearer tokens
 * (`mat_...`).
 */

import { LedgerApiError } from '../ledger/client.ts';

/** Lifecycle of an approval request. */
export type ApprovalStatus = 'pending' | 'approved' | 'denied' | 'cancelled';

/** An approval request as returned by Macro. */
export interface ApprovalRequest {
  id: string;
  org_id: number | null;
  agent_slug: string;
  session_id: string | null;
  requester_user_id: string | null;
  requester_display: string;
  tool_name: string;
  arguments: unknown;
  arguments_digest: string;
  summary: string;
  status: ApprovalStatus;
  assignee_user_id: string | null;
  assignee_team_id: string | null;
  callback_url: string | null;
  decided_by: string | null;
  decision_note: string | null;
  created_at: string;
  decided_at: string | null;
  consumed_at: string | null;
}

/** Outcome of gating one proposed tool call. */
export type GateOutcome =
  | { decision: 'allow' }
  | { decision: 'deny'; reason: string }
  | { decision: 'pending'; request: ApprovalRequest };

/** Request body for gating a proposed tool call. */
export interface GateToolCallRequest {
  session_id?: string;
  requester_user_id?: string;
  requester_display: string;
  tool_name: string;
  arguments: unknown;
  summary: string;
  callback_url?: string;
}

/**
 * Client for the agent-facing approval endpoints. One instance per agent
 * principal; requires the `approval:gate` / `approval:query` scopes.
 */
export class ApprovalsClient {
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

  /** Evaluate the policy floor for a proposed tool call. */
  gate(request: GateToolCallRequest): Promise<GateOutcome> {
    return this.request<GateOutcome>('POST', '/agent-approvals/gate', request);
  }

  /** Poll one approval request's status. */
  get(id: string): Promise<ApprovalRequest> {
    return this.request<ApprovalRequest>(
      'GET',
      `/agent-approvals/${encodeURIComponent(id)}`,
    );
  }
}
