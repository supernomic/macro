/**
 * Typed client for Macro's skills-governance API
 * (`crates/skill_governance/src/inbound/axum_router.rs`).
 *
 * Agent tokens (`mat_...`) with `skill:read` / `skill:propose`. Not part of
 * `@macro/sdk` — same reason as the ledger client.
 */

import { LedgerApiError } from '../ledger/client.ts';

/** Who owns a governed skill. */
export type SkillScope = 'user' | 'team' | 'org' | 'platform';

/** Hermes-style trust tier. */
export type TrustTier = 'builtin' | 'verified' | 'community' | 'untrusted';

/** Catalog row served to Flue for `defineSkill` / `useSkill`. */
export interface SkillCatalogEntry {
  id: string;
  slug: string;
  description: string;
  body: string;
  version: string;
  scope: SkillScope;
  trust_tier: TrustTier;
}

/** Full skill record. */
export interface SkillRecord extends SkillCatalogEntry {
  name: string;
}

/** Staged proposal kind. */
export type ProposalKind = 'create' | 'patch' | 'archive';

/** Request to open a staged skill proposal. */
export interface ProposeSkillRequest {
  kind: ProposalKind;
  skill_id?: string;
  slug: string;
  target_scope: SkillScope;
  owner_user_id?: string;
  owner_team_id?: string;
  proposed_name: string;
  proposed_description: string;
  proposed_body: string;
  diff_summary: string;
  evidence?: unknown;
  assignee_user_id?: string;
  assignee_team_id?: string;
}

/** A staged skill proposal. */
export interface SkillProposal {
  id: string;
  slug: string;
  status: string;
  target_scope: SkillScope;
  diff_summary: string;
}

/** Personal inbox of pending proposals (`GET /skill-proposals/mine`). */
export interface UserProposals {
  /** Pending proposals assigned directly to the caller. */
  assigned: SkillProposal[];
  /** Pending proposals on the caller's team queues. */
  team_queue: SkillProposal[];
}

/**
 * Client for agent-facing skills endpoints. One instance per principal.
 */
export class SkillsClient {
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

  /** Skills this principal may inject. */
  catalog(): Promise<SkillCatalogEntry[]> {
    return this.request<SkillCatalogEntry[]>('GET', '/agent-skills');
  }

  /** Fetch one skill. */
  get(id: string): Promise<SkillRecord> {
    return this.request<SkillRecord>(
      'GET',
      `/agent-skills/${encodeURIComponent(id)}`,
    );
  }

  /** Open a staged proposal (user-scope creates may auto-apply). */
  propose(request: ProposeSkillRequest): Promise<SkillProposal> {
    return this.request<SkillProposal>('POST', '/agent-skills', request);
  }

  /** Pending proposals in the caller's inbox (assigned + team queues). */
  listMine(): Promise<UserProposals> {
    return this.request<UserProposals>('GET', '/skill-proposals/mine');
  }
}
