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

function stepsFromEvidence(events: readonly LedgerEvent[]): string[] {
  const steps: string[] = [];
  for (const event of events) {
    if (
      event.event_type === 'escalation/resolved' &&
      event.payload.type === 'escalation_resolved'
    ) {
      const { escalation_id, resolved_by } = event.payload.data;
      steps.push(
        `Apply the recorded resolution for escalation ${escalation_id} ` +
          `(resolved by ${resolved_by}). Do not add tools this event ` +
          'does not name.',
      );
      continue;
    }
    if (
      event.event_type === 'feedback/record' &&
      event.payload.type === 'feedback_record'
    ) {
      const note = event.payload.data.note?.trim();
      if (note) {
        steps.push(note);
      } else {
        steps.push(
          `Reuse the approach from the praised turn at ledger seq ` +
            `${event.seq}.`,
        );
      }
    }
  }
  if (steps.length === 0) {
    steps.push(
      "Consult this session's recorded outcomes before retrying the " +
        'same class of request.',
    );
  }
  return steps;
}

/**
 * Distill a procedure-shaped skill body from ledger evidence. Raw excerpts
 * stay on the proposal's `evidence` blob — never as the body.
 */
export function procedureBodyFromEvidence(
  sessionId: string,
  events: readonly LedgerEvent[],
): string {
  const steps = stepsFromEvidence(events);
  const numbered = steps.map((step, index) => `${index + 1}. ${step}`);
  return [
    '## Goal',
    '',
    `Capture a reusable procedure from session ${sessionId}'s resolved ` +
      'escalations and highly-rated turns.',
    '',
    '## When to use',
    '',
    "When a later conversation matches this session's resolved " +
      'escalation or praised turn.',
    '',
    '## Steps',
    '',
    ...numbered,
    '',
    '## Guardrails',
    '',
    '- Do not invent tools or APIs that the evidence does not mention.',
    '- Keep raw ledger excerpts in the proposal evidence blob, not in ' +
      'this procedure.',
    '- Prefer the recorded resolution or praised turn over a new guess.',
    '',
  ].join('\n');
}

/**
 * Build one org-scope candidate from a session's evidence events. Body is
 * procedure markdown derived from resolutions and praised turns; raw
 * excerpts live only on `evidence`.
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
  return {
    slug: slugFromSession(sessionId),
    name: `Trace refinement ${sessionId.slice(0, 8)}`,
    description:
      `Procedure distilled from ${resolutions.length} resolved ` +
      `escalation(s) and ${praise.length} highly-rated turn(s) in one session.`,
    body: procedureBodyFromEvidence(sessionId, events),
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

function pendingSlugsFromInbox(
  assigned: readonly { slug: string; status: string }[],
  teamQueue: readonly { slug: string; status: string }[],
): Set<string> {
  const slugs = new Set<string>();
  for (const proposal of [...assigned, ...teamQueue]) {
    if (proposal.status === 'pending' && proposal.slug) {
      slugs.add(proposal.slug);
    }
  }
  return slugs;
}

async function loadPendingProposalSlugs(
  runtime: AgentRuntime,
): Promise<Set<string>> {
  try {
    const mine = await runtime.skills.listMine();
    return pendingSlugsFromInbox(mine.assigned, mine.team_queue);
  } catch (e) {
    console.error('failed to list pending skill proposals', e);
    return new Set();
  }
}

/**
 * Query org history for escalation resolutions and highly-rated turns,
 * then open a proposal per evidence cluster (or per caller-supplied
 * candidate, when those are attached to real ledger events).
 *
 * No ledger evidence ⇒ no proposals. Operator-supplied candidates are
 * optional; when omitted the job distills one candidate per session from
 * the events themselves. A pending proposal with the same slug is skipped.
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

  const pendingSlugs = await loadPendingProposalSlugs(runtime);
  const proposalIds: string[] = [];
  let skipped = 0;
  for (const candidate of toPropose) {
    if (!candidate.slug || !candidate.name || !candidate.body) {
      skipped += 1;
      continue;
    }
    if (pendingSlugs.has(candidate.slug)) {
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
    pendingSlugs.add(candidate.slug);
    proposalIds.push(proposal.id);
  }
  return {
    proposal_ids: proposalIds,
    evidence_event_count: evidence.length,
    skipped,
  };
}
