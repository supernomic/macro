import { describe, expect, mock, test } from 'bun:test';
import type { LedgerEvent } from '../ledger/events.ts';
import type { AgentRuntime } from '../sessions/macro-session.ts';
import type { ProposeSkillRequest, SkillProposal } from '../skills/client.ts';
import { candidateFromEvidence, runTraceRefinement } from './trace-refine.ts';

const SESSION_ID = 'aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee';
const SLUG = 'trace-refine-aaaaaaaabbbb';

function evidenceEvent(
  overrides: Pick<LedgerEvent, 'event_type' | 'payload' | 'seq'> &
    Partial<LedgerEvent>,
): LedgerEvent {
  return {
    session_id: SESSION_ID,
    org_id: 1,
    actor_kind: 'system',
    actor_id: 'macro:escalations',
    occurred_at: '2026-01-01T00:00:00Z',
    source_event_seqs: [],
    hash: `hash-${overrides.seq}`,
    ...overrides,
  };
}

function resolvedEscalation(seq: number): LedgerEvent {
  return evidenceEvent({
    seq,
    event_type: 'escalation/resolved',
    payload: {
      type: 'escalation_resolved',
      data: {
        escalation_id: 'esc-1',
        resolved_by: 'expert-ada',
      },
    },
  });
}

function praisedTurn(seq: number, note?: string): LedgerEvent {
  return evidenceEvent({
    seq,
    event_type: 'feedback/record',
    actor_kind: 'user',
    actor_id: 'user-1',
    payload: {
      type: 'feedback_record',
      data: { rating: true, ...(note ? { note } : {}) },
    },
  });
}

function fakeRuntime(opts: {
  events: LedgerEvent[];
  listMine: () => Promise<{
    assigned: SkillProposal[];
    team_queue: SkillProposal[];
  }>;
  propose: (request: ProposeSkillRequest) => Promise<SkillProposal>;
}): AgentRuntime {
  return {
    agentSlug: 'super-agent',
    catalogReady: Promise.resolve(),
    skillsCatalog: [],
    compositionId: 'super-agent/v1',
    ledger: {
      queryOrgEvents: async () => opts.events,
    },
    skills: {
      listMine: opts.listMine,
      propose: opts.propose,
    },
  } as unknown as AgentRuntime;
}

describe('candidateFromEvidence', () => {
  test('body is procedure markdown with excerpts only in evidence', () => {
    const events = [
      resolvedEscalation(1),
      praisedTurn(2, 'Reset the VPN profile, then retry login.'),
    ];
    const candidate = candidateFromEvidence(SESSION_ID, events);

    expect(candidate.slug).toBe(SLUG);
    expect(candidate.body).toContain('## Goal');
    expect(candidate.body).toContain('## When to use');
    expect(candidate.body).toContain('## Steps');
    expect(candidate.body).toContain('## Guardrails');
    expect(candidate.body).not.toContain('```');
    expect(candidate.body).not.toContain('"type": "escalation_resolved"');
    expect(candidate.body).toContain('esc-1');
    expect(candidate.body).toContain(
      'Reset the VPN profile, then retry login.',
    );
    expect(candidate.evidence).toEqual([
      {
        session_id: SESSION_ID,
        seq: 1,
        event_type: 'escalation/resolved',
        occurred_at: '2026-01-01T00:00:00Z',
        payload: events[0]?.payload,
      },
      {
        session_id: SESSION_ID,
        seq: 2,
        event_type: 'feedback/record',
        occurred_at: '2026-01-01T00:00:00Z',
        payload: events[1]?.payload,
      },
    ]);
  });
});

describe('runTraceRefinement', () => {
  test('second run skips a pending proposal with the same slug', async () => {
    const events = [resolvedEscalation(1), praisedTurn(2)];
    const posted: ProposeSkillRequest[] = [];
    const mine: SkillProposal[] = [];

    const runtime = fakeRuntime({
      events,
      listMine: async () => ({ assigned: [...mine], team_queue: [] }),
      propose: async (request) => {
        posted.push(request);
        const proposal: SkillProposal = {
          id: `proposal-${posted.length}`,
          slug: request.slug,
          status: 'pending',
          target_scope: request.target_scope,
          diff_summary: request.diff_summary,
        };
        mine.push(proposal);
        return proposal;
      },
    });

    const first = await runTraceRefinement(runtime);
    expect(first.proposal_ids).toEqual(['proposal-1']);
    expect(first.skipped).toBe(0);
    expect(posted).toHaveLength(1);
    expect(posted[0]?.proposed_body).toContain('## Goal');
    expect(posted[0]?.proposed_body).not.toContain('```');

    const second = await runTraceRefinement(runtime);
    expect(second.proposal_ids).toEqual([]);
    expect(second.skipped).toBe(1);
    expect(posted).toHaveLength(1);
  });

  test('does not skip when the existing proposal is no longer pending', async () => {
    const events = [resolvedEscalation(1)];
    const posted: ProposeSkillRequest[] = [];

    const runtime = fakeRuntime({
      events,
      listMine: async () => ({
        assigned: [
          {
            id: 'old',
            slug: SLUG,
            status: 'approved',
            target_scope: 'org',
            diff_summary: 'already landed',
          },
        ],
        team_queue: [],
      }),
      propose: async (request) => {
        posted.push(request);
        return {
          id: 'new',
          slug: request.slug,
          status: 'pending',
          target_scope: request.target_scope,
          diff_summary: request.diff_summary,
        };
      },
    });

    const result = await runTraceRefinement(runtime);
    expect(result.proposal_ids).toEqual(['new']);
    expect(result.skipped).toBe(0);
    expect(posted).toHaveLength(1);
  });

  test('listMine failure logs and still proposes', async () => {
    const errorSpy = mock(() => {});
    const original = console.error;
    console.error = errorSpy;
    try {
      const posted: ProposeSkillRequest[] = [];
      const runtime = fakeRuntime({
        events: [resolvedEscalation(1)],
        listMine: async () => {
          throw new Error('unauthorized');
        },
        propose: async (request) => {
          posted.push(request);
          return {
            id: 'p1',
            slug: request.slug,
            status: 'pending',
            target_scope: request.target_scope,
            diff_summary: request.diff_summary,
          };
        },
      });
      const result = await runTraceRefinement(runtime);
      expect(result.proposal_ids).toEqual(['p1']);
      expect(posted).toHaveLength(1);
      expect(errorSpy).toHaveBeenCalled();
    } finally {
      console.error = original;
    }
  });
});
