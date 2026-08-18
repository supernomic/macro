/**
 * Typed client for Macro's entity-graph API
 * (`crates/entity_graph/src/inbound/axum_router.rs`).
 */

import { LedgerApiError } from '../ledger/client.ts';

/** A graph node. */
export interface GraphNode {
  id: string;
  org_id: number | null;
  node_type: string;
  display_name: string;
  attributes: unknown;
}

/** A typed edge. */
export interface GraphEdge {
  id: string;
  from_node_id: string;
  to_node_id: string;
  relationship: string;
}

/** Neighbor pair. */
export interface Neighbor {
  edge: GraphEdge;
  node: GraphNode;
}

/** Client for agent-facing graph endpoints. Requires `graph:read` / `graph:write`. */
export class GraphClient {
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

  /** Neighbors of a node. */
  neighbors(nodeId: string, relationship?: string): Promise<Neighbor[]> {
    const params = new URLSearchParams();
    if (relationship) {
      params.set('relationship', relationship);
    }
    const qs = params.size > 0 ? `?${params}` : '';
    return this.request<Neighbor[]>(
      'GET',
      `/agent-graph/nodes/${encodeURIComponent(nodeId)}/neighbors${qs}`,
    );
  }
}
