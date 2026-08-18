import { describe, expect, mock, test } from 'bun:test';
import type { Macro } from '@macro/sdk';
import type { LedgerClient } from '../ledger/client.ts';
import type { NewLedgerEvent } from '../ledger/events.ts';
import type { SkillCatalogEntry, SkillsClient } from '../skills/client.ts';
import { type AgentRuntime, MacroSessionContext } from './macro-session.ts';

const SKILL: SkillCatalogEntry = {
  id: 'skill-vpn',
  slug: 'vpn-reset',
  description: 'Reset an employee VPN profile.',
  body: '## Steps\n1. Reset the profile.',
  version: '3',
  scope: 'org',
  trust_tier: 'verified',
};

function mapping() {
  return {
    session_id: 'sess-1',
    runtime_conversation_id: 'conv-1',
    external_thread_kind: null,
    external_thread_key: null,
    org_id: 1,
    agent_principal_id: 'super-agent',
    created_at: '2026-01-01T00:00:00Z',
  };
}

function runtimeWithLedger(
  ledger: Pick<LedgerClient, 'openSession' | 'appendEvents'>,
  extras: Partial<AgentRuntime> = {},
): AgentRuntime {
  return {
    agentSlug: 'super-agent',
    macro: {} as Macro,
    ledger: ledger as LedgerClient,
    escalations: {} as AgentRuntime['escalations'],
    approvals: {} as AgentRuntime['approvals'],
    skills: {} as SkillsClient,
    feedback: {} as AgentRuntime['feedback'],
    graph: {} as AgentRuntime['graph'],
    skillsCatalog: [],
    catalogReady: Promise.resolve(),
    compositionId: 'super-agent/v1',
    ...extras,
  };
}

function recordingLedger(opts?: {
  onAppend?: (events: NewLedgerEvent[]) => Promise<void> | void;
}) {
  const appended: NewLedgerEvent[] = [];
  const ledger = {
    openSession: async () => mapping(),
    appendEvents: async (_sessionId: string, events: NewLedgerEvent[]) => {
      await opts?.onAppend?.(events);
      appended.push(...events);
      return events.map((event, index) => ({
        session_id: 'sess-1',
        seq: appended.length - events.length + index + 1,
        event_type: event.payload.type.replace('_', '/'),
        payload: event.payload,
        actor_kind: event.actor_kind,
        actor_id: event.actor_id,
        org_id: 1,
        occurred_at: '2026-01-01T00:00:00Z',
        source_event_seqs: event.source_event_seqs ?? [],
        hash: `h${appended.length - events.length + index + 1}`,
      }));
    },
  };
  return { ledger, appended };
}

describe('pinRequestHeader', () => {
  test('waits for a delayed catalog and pins non-empty skill_versions', async () => {
    const { ledger, appended } = recordingLedger();
    let resolveCatalog!: () => void;
    const catalogReady = new Promise<void>((resolve) => {
      resolveCatalog = resolve;
    });
    const runtime = runtimeWithLedger(ledger, {
      skillsCatalog: [],
      catalogReady,
    });
    const session = new MacroSessionContext({
      runtime,
      conversationId: 'conv-delay',
    });

    const pin = session.pinRequestHeader();
    await Promise.resolve();
    expect(appended).toHaveLength(0);

    runtime.skillsCatalog = [SKILL];
    resolveCatalog();
    await pin;

    expect(appended).toHaveLength(1);
    const payload = appended[0]?.payload;
    expect(payload?.type).toBe('request_header');
    if (payload?.type !== 'request_header') {
      throw new Error('expected request_header');
    }
    expect(payload.data.skill_versions).toEqual({ 'vpn-reset': '3' });
    expect(payload.data.composition_id).toBe('super-agent/v1');
  });

  test('failed catalog still pins empty versions and does not hang', async () => {
    const { ledger, appended } = recordingLedger();
    const runtime = runtimeWithLedger(ledger, {
      skillsCatalog: [],
      catalogReady: Promise.resolve(),
    });
    const session = new MacroSessionContext({
      runtime,
      conversationId: 'conv-fail',
    });

    const hung = new Promise<never>((_, reject) => {
      setTimeout(() => reject(new Error('pin hung on failed catalog')), 1000);
    });
    await Promise.race([session.pinRequestHeader(), hung]);

    expect(appended).toHaveLength(1);
    const payload = appended[0]?.payload;
    expect(payload?.type).toBe('request_header');
    if (payload?.type !== 'request_header') {
      throw new Error('expected request_header');
    }
    expect(payload.data.skill_versions).toEqual({});
  });

  test('skips when a header is already pinned', async () => {
    const { ledger, appended } = recordingLedger();
    const session = new MacroSessionContext({
      runtime: runtimeWithLedger(ledger),
      conversationId: 'conv-once',
    });
    await session.pinRequestHeader();
    await session.pinRequestHeader();
    expect(appended).toHaveLength(1);
  });

  test('ledger failure is logged and does not throw', async () => {
    const errorSpy = mock(() => {});
    const original = console.error;
    console.error = errorSpy;
    try {
      const ledger = {
        openSession: async () => mapping(),
        appendEvents: async () => {
          throw new Error('ledger down');
        },
      };
      const session = new MacroSessionContext({
        runtime: runtimeWithLedger(ledger),
        conversationId: 'conv-err',
      });
      await session.pinRequestHeader();
      expect(errorSpy).toHaveBeenCalled();
    } finally {
      console.error = original;
    }
  });
});

describe('recordSkillInjection', () => {
  test('awaits the ledger append before resolving', async () => {
    let appendStarted = false;
    let release!: () => void;
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    const { ledger, appended } = recordingLedger({
      onAppend: async () => {
        appendStarted = true;
        await gate;
      },
    });
    const session = new MacroSessionContext({
      runtime: runtimeWithLedger(ledger),
      conversationId: 'conv-inject',
    });

    let resolved = false;
    const injection = session.recordSkillInjection(SKILL).then(() => {
      resolved = true;
    });
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(appendStarted).toBe(true);
    expect(resolved).toBe(false);
    expect(appended).toHaveLength(0);

    release();
    await injection;
    expect(resolved).toBe(true);
    expect(appended).toHaveLength(1);
    expect(appended[0]?.payload).toEqual({
      type: 'skill_injected',
      data: { skill_id: 'skill-vpn', version: '3' },
    });
  });

  test('ledger failure is logged and does not throw', async () => {
    const errorSpy = mock(() => {});
    const original = console.error;
    console.error = errorSpy;
    try {
      const ledger = {
        openSession: async () => mapping(),
        appendEvents: async () => {
          throw new Error('ledger down');
        },
      };
      const session = new MacroSessionContext({
        runtime: runtimeWithLedger(ledger),
        conversationId: 'conv-inject-err',
      });
      await session.recordSkillInjection(SKILL);
      expect(errorSpy).toHaveBeenCalled();
    } finally {
      console.error = original;
    }
  });
});
