/**
 * Trace-refinement job: scan recent ledger events (escalation resolutions
 * and highly-rated turns) and open skill proposals from the evidence.
 *
 * Macro's `/skill-refinements` ingest is internal-only; this job uses the
 * agent `skill:propose` surface so a runtime principal can emit proposals
 * from traces it is allowed to read.
 */

import type { LedgerEvent } from '../ledger/events.ts';
import type { AgentRuntime } from '../sessions/macro-session.ts';

/** One proposed skill distilled from a trace window. */
export interface RefineCandidate {
  slug: string;
  name: string;
  description: string;
  body: string;
  diff_summary: string;
  evidence?: unknown;
}

/** Result of a trace-refinement run. */
export interface TraceRefineResult {
  proposal_ids: string[];
  evidence_event_count: number;
  skipped: number;
}

/** Compact, joinable excerpt stored on a proposal's evidence blob. */
export interface LedgerEvidenceExcerpt {
  session_id: string;
  seq: number;
  event_type: string;
  occurred_at: string;
  payload: LedgerEvent['payload'];
}

/**
 * Whether a ledger row is evidence this job will propose from: resolved
 * escalations, and thumbs-up `feedback/record` events.
 */
export function isTraceRefineEvidence(event: LedgerEvent): boolean {
  if (event.event_type === 'escalation/resolved') {
    return true;
  }
  if (event.event_type !== 'feedback/record') {
    return false;
  }
  return (
    event.payload.type === 'feedback_record' &&
    event.payload.data.rating === true
  );
}

/** Strip a ledger event down to the excerpt a proposal stores. */
export function evidenceExcerpt(event: LedgerEvent): LedgerEvidenceExcerpt {
  return {
    session_id: event.session_id,
    seq: event.seq,
    event_type: event.event_type,
    occurred_at: event.occurred_at,
    payload: event.payload,
  };
}

function slugFromSession(sessionId: string): string {
  const compact = sessionId.replace(/-/g, '').slice(0, 12).toLowerCase();
  return `trace-refine-${compact}`;
}

/**
 * Build one org-scope candidate from a session's evidence events. Body text
 * is the ledger excerpt — this job does not invent procedure beyond what
 * the events already recorded.
 */
export function candidateFromEvidence(
  sessionId: string,
  events: readonly LedgerEvent[],
): RefineCandidate {
  const resolutions = events.filter(
    (event) => event.event_type === 'escalation/resolved',
  );
  const praise = events.filter(
    (event) => event.event_type === 'feedback/record',
  );
  const lines: string[] = [
    '# Distilled from ledger evidence',
    '',
    `Session: ${sessionId}`,
    '',
  ];
  for (const event of events) {
    lines.push(`## ${event.event_type} (seq ${event.seq})`);
    lines.push('');
    lines.push('```json');
    lines.push(JSON.stringify(event.payload, null, 2));
    lines.push('```');
    lines.push('');
  }
  return {
    slug: slugFromSession(sessionId),
    name: `Trace refinement ${sessionId.slice(0, 8)}`,
    description:
      `Procedure distilled from ${resolutions.length} resolved ` +
      `escalation(s) and ${praise.length} highly-rated turn(s) in one session.`,
    body: lines.join('\n'),
    diff_summary:
      `Opened from ${events.length} ledger evidence event(s) ` +
      `(escalation/resolved, feedback/record) in session ${sessionId}.`,
    evidence: events.map(evidenceExcerpt),
  };
}

function groupBySession(
  events: readonly LedgerEvent[],
): Map<string, LedgerEvent[]> {
  const bySession = new Map<string, LedgerEvent[]>();
  for (const event of events) {
    const list = bySession.get(event.session_id) ?? [];
    list.push(event);
    bySession.set(event.session_id, list);
  }
  return bySession;
}

/**
 * Query org history for escalation resolutions and highly-rated turns,
 * then open a proposal per evidence cluster (or per caller-supplied
 * candidate, when those are attached to real ledger events).
 *
 * No ledger evidence ⇒ no proposals. Operator-supplied candidates are
 * optional; when omitted the job distills one candidate per session from
 * the events themselves.
 */
export async function runTraceRefinement(
  runtime: AgentRuntime,
  candidates?: readonly RefineCandidate[],
): Promise<TraceRefineResult> {
  const events = await runtime.ledger.queryOrgEvents({
    eventTypes: ['escalation/resolved', 'feedback/record'],
    limit: 200,
  });
  const evidence = events.filter(isTraceRefineEvidence);
  const excerpts = evidence.map(evidenceExcerpt);

  if (evidence.length === 0) {
    return {
      proposal_ids: [],
      evidence_event_count: 0,
      skipped: candidates?.length ?? 0,
    };
  }

  const toPropose: RefineCandidate[] =
    candidates && candidates.length > 0
      ? candidates.map((candidate) => ({
          ...candidate,
          evidence: {
            supplied: candidate.evidence ?? null,
            ledger: excerpts,
          },
        }))
      : [...groupBySession(evidence).entries()].map(
          ([sessionId, sessionEvents]) =>
            candidateFromEvidence(sessionId, sessionEvents),
        );

  const proposalIds: string[] = [];
  let skipped = 0;
  for (const candidate of toPropose) {
    if (!candidate.slug || !candidate.name || !candidate.body) {
      skipped += 1;
      continue;
    }
    const proposal = await runtime.skills.propose({
      kind: 'create',
      slug: candidate.slug,
      target_scope: 'org',
      proposed_name: candidate.name,
      proposed_description: candidate.description,
      proposed_body: candidate.body,
      diff_summary: candidate.diff_summary,
      evidence: candidate.evidence ?? excerpts,
    });
    proposalIds.push(proposal.id);
  }
  return {
    proposal_ids: proposalIds,
    evidence_event_count: evidence.length,
    skipped,
  };
}
