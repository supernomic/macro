/**
 * Trace-refinement job: scan recent ledger events (escalation resolutions
 * and highly-rated turns) and open skill proposals from the evidence.
 *
 * Macro's `/skill-refinements` ingest is internal-only; this job uses the
 * agent `skill:propose` surface so a runtime principal can emit proposals
 * from traces it is allowed to read.
 */

import type { AgentRuntime } from '../sessions/macro-session.ts';

/** One proposed skill distilled from a trace window. */
export interface RefineCandidate {
  slug: string;
  name: string;
  description: string;
  body: string;
  diff_summary: string;
  evidence: unknown;
}

/**
 * Query org history for escalation resolutions and open a proposal per
 * candidate. Callers (cron, operator route) supply the distilled
 * candidates — this job does not invent skill text from raw events.
 */
export async function runTraceRefinement(
  runtime: AgentRuntime,
  candidates: readonly RefineCandidate[],
): Promise<{ proposal_ids: string[] }> {
  const events = await runtime.ledger.queryOrgEvents({
    eventTypes: ['escalation/resolved', 'feedback/record'],
    limit: 200,
  });
  const proposalIds: string[] = [];
  for (const candidate of candidates) {
    const proposal = await runtime.skills.propose({
      kind: 'create',
      slug: candidate.slug,
      target_scope: 'org',
      proposed_name: candidate.name,
      proposed_description: candidate.description,
      proposed_body: candidate.body,
      diff_summary: candidate.diff_summary,
      evidence: {
        candidate: candidate.evidence,
        supporting_event_count: events.length,
      },
    });
    proposalIds.push(proposal.id);
  }
  return { proposal_ids: proposalIds };
}
