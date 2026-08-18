import { describe, expect, mock, test } from 'bun:test';

mock.module('@flue/runtime', () => ({
  defineSkill: (definition: unknown) => definition,
  useSkill: () => undefined,
}));

import type { Macro } from '@macro/sdk';
import type { LedgerClient } from '../ledger/client.ts';
import type { NewLedgerEvent } from '../ledger/events.ts';
import {
  type AgentRuntime,
  MacroSessionContext,
} from '../sessions/macro-session.ts';
import type { SkillCatalogEntry, SkillsClient } from './client.ts';
import { mountGovernedSkills } from './mount.ts';

const SKILL: SkillCatalogEntry = {
  id: 'skill-vpn',
  slug: 'vpn-reset',
  description: 'Reset an employee VPN profile.',
  body: '## Steps\n1. Reset the profile.',
  version: '3',
  scope: 'org',
  trust_tier: 'verified',
};

describe('mountGovernedSkills', () => {
  test('awaits skill_injected appends before resolving', async () => {
    const appended: NewLedgerEvent[] = [];
    let release!: () => void;
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    const runtime = {
      agentSlug: 'super-agent',
      macro: {} as Macro,
      ledger: {
        openSession: async () => ({
          session_id: 'sess-1',
          runtime_conversation_id: 'conv-1',
          external_thread_kind: null,
          external_thread_key: null,
          org_id: 1,
          agent_principal_id: 'super-agent',
          created_at: '2026-01-01T00:00:00Z',
        }),
        appendEvents: async (_id: string, events: NewLedgerEvent[]) => {
          await gate;
          appended.push(...events);
          return events.map((event, index) => ({
            session_id: 'sess-1',
            seq: index + 1,
            event_type: 'skill/injected',
            payload: event.payload,
            actor_kind: event.actor_kind,
            actor_id: event.actor_id,
            org_id: 1,
            occurred_at: '2026-01-01T00:00:00Z',
            source_event_seqs: [],
            hash: 'h1',
          }));
        },
      } as Pick<LedgerClient, 'openSession' | 'appendEvents'> as LedgerClient,
      escalations: {} as AgentRuntime['escalations'],
      approvals: {} as AgentRuntime['approvals'],
      skills: {} as SkillsClient,
      feedback: {} as AgentRuntime['feedback'],
      graph: {} as AgentRuntime['graph'],
      skillsCatalog: [SKILL],
      catalogReady: Promise.resolve(),
      compositionId: 'super-agent/v1',
    } as AgentRuntime;
    const session = new MacroSessionContext({
      runtime,
      conversationId: 'conv-mount',
    });

    let resolved = false;
    const mounted = mountGovernedSkills(session, [SKILL]).then(() => {
      resolved = true;
    });
    await Promise.resolve();
    expect(resolved).toBe(false);
    expect(appended).toHaveLength(0);

    release();
    await mounted;
    expect(resolved).toBe(true);
    expect(appended).toHaveLength(1);
    expect(appended[0]?.payload.type).toBe('skill_injected');
  });
});
