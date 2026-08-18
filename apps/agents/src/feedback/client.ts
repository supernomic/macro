/**
 * Typed client for Macro's feedback sidecar
 * (`crates/agent_feedback/src/inbound/axum_router.rs`).
 */

import { LedgerApiError } from '../ledger/client.ts';

/** Sidecar rating. */
export interface MessageRating {
  session_id: string;
  target_seq: number;
  rating: 'up' | 'down' | 'none';
  note: string | null;
  rated_by: string;
  updated_at: string;
}

/** Per-session training-export consent. */
export interface ConsentRecord {
  session_id: string;
  org_id: number | null;
  sharing_mode: 'full' | 'feedback_only' | 'disabled';
  set_by: string;
  updated_at: string;
}

/** Client for agent-facing feedback endpoints. Requires `feedback:*`. */
export class FeedbackClient {
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

  /** Upsert a sidecar rating. */
  rate(
    sessionId: string,
    body: { target_seq: number; rating: 'up' | 'down' | 'none'; note?: string },
  ): Promise<MessageRating> {
    return this.request<MessageRating>(
      'POST',
      `/agent-feedback/${encodeURIComponent(sessionId)}/ratings`,
      body,
    );
  }

  /** List sidecar ratings for a session. */
  list(sessionId: string): Promise<MessageRating[]> {
    return this.request<MessageRating[]>(
      'GET',
      `/agent-feedback/${encodeURIComponent(sessionId)}/ratings`,
    );
  }

  /** Read sharing consent. */
  getConsent(sessionId: string): Promise<ConsentRecord | null> {
    return this.request<ConsentRecord | null>(
      'GET',
      `/agent-feedback/${encodeURIComponent(sessionId)}/consent`,
    );
  }
}
