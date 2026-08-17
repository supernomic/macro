/**
 * Resume-on-reply: Macro calls back here when an escalation created by
 * this service is resolved (or cancelled). The expert's answer is recorded
 * in the conversation's ledger and dispatched into the durable Flue
 * conversation so the agent picks up where it escalated.
 */

import { dispatch } from '@flue/runtime';
import { SuperAgent } from '../agents/super-agent.ts';
import {
  sessionContextFor,
  superAgentRuntimeInstance,
} from '../sessions/macro-session.ts';

/** Payload Macro posts on escalation resolution. */
export interface EscalationCallbackPayload {
  escalation_id: string;
  status: 'resolved' | 'cancelled';
  domain: string;
  resolution: string | null;
  resolved_by: string | null;
}

/**
 * Deliver an escalation outcome into its conversation. Idempotent per
 * escalation + status: redelivered callbacks reuse the same dispatch
 * idempotency key and never run a second turn.
 */
export async function resumeFromEscalation(
  conversationId: string,
  payload: EscalationCallbackPayload,
): Promise<void> {
  const session = sessionContextFor({
    runtime: superAgentRuntimeInstance(),
    conversationId,
  });

  // Record the outcome on the trace of record. Best-effort here: a ledger
  // hiccup must not swallow the expert's answer, and redelivery would
  // double-log while dispatch below dedupes.
  const events =
    payload.status === 'resolved' && payload.resolution
      ? [
          {
            payload: {
              type: 'escalation_resolved' as const,
              data: {
                escalation_id: payload.escalation_id,
                resolved_by: payload.resolved_by ?? 'unknown',
              },
            },
            actor_kind: 'system' as const,
            actor_id: 'macro:escalations',
          },
          {
            payload: {
              type: 'user_message' as const,
              data: {
                content: payload.resolution,
                source: 'escalation_reply' as const,
              },
            },
            actor_kind: 'user' as const,
            actor_id: payload.resolved_by ?? 'unknown',
          },
        ]
      : [
          {
            payload: {
              type: 'escalation_resolved' as const,
              data: {
                escalation_id: payload.escalation_id,
                resolved_by: payload.resolved_by ?? 'cancelled',
              },
            },
            actor_kind: 'system' as const,
            actor_id: 'macro:escalations',
          },
        ];
  session.append(...events).catch((e) => {
    console.error('failed to record escalation outcome in ledger', e);
  });

  const body =
    payload.status === 'resolved'
      ? `Escalation ${payload.escalation_id} was resolved by a human expert.\n\nExpert answer:\n${payload.resolution ?? '(no text)'}\n\nRelay this to the user in your own words and close out the request.`
      : `Escalation ${payload.escalation_id} was cancelled without a resolution. Let the user know and suggest next steps.`;

  await dispatch(SuperAgent, {
    id: conversationId,
    idempotencyKey: `escalation:${payload.escalation_id}:${payload.status}`,
    message: {
      kind: 'signal',
      type: 'macro.escalation_resolved',
      body,
      attributes: {
        escalationId: payload.escalation_id,
        status: payload.status,
        ...(payload.resolved_by ? { resolvedBy: payload.resolved_by } : {}),
      },
    },
  });
}
