/**
 * Resume-on-decision: Macro calls back here when an approval requested by
 * this service is decided (or cancelled). The decision is recorded in the
 * conversation's ledger and dispatched into the durable Flue conversation
 * so the agent retries the gated tool (on approve) or explains the denial.
 */

import { dispatch } from '@flue/runtime';
import { SuperAgent } from '../agents/super-agent.ts';
import {
  sessionContextFor,
  superAgentRuntimeInstance,
} from '../sessions/macro-session.ts';

/** Payload Macro posts on approval decision. */
export interface ApprovalCallbackPayload {
  approval_id: string;
  status: 'approved' | 'denied' | 'cancelled' | 'pending';
  tool_name: string;
  approved: boolean;
  decided_by: string | null;
  note: string | null;
}

/**
 * Deliver an approval decision into its conversation. Idempotent per
 * approval + status: redelivered callbacks reuse the same dispatch
 * idempotency key and never run a second turn.
 */
export async function resumeFromApproval(
  conversationId: string,
  payload: ApprovalCallbackPayload,
): Promise<void> {
  const session = sessionContextFor({
    runtime: superAgentRuntimeInstance(),
    conversationId,
  });

  session
    .append({
      payload: {
        type: 'approval_decided',
        data: {
          approval_id: payload.approval_id,
          approved: payload.approved,
          decided_by: payload.decided_by ?? 'unknown',
          ...(payload.note ? { note: payload.note } : {}),
        },
      },
      actor_kind: 'user',
      actor_id: payload.decided_by ?? 'unknown',
    })
    .catch((e) => {
      console.error('failed to record approval decision in ledger', e);
    });

  const body = payload.approved
    ? `Approval ${payload.approval_id} for tool \`${payload.tool_name}\` was granted${payload.note ? `: ${payload.note}` : ''}. Retry the same tool call now; the gate will let it through once.`
    : `Approval ${payload.approval_id} for tool \`${payload.tool_name}\` was ${payload.status}${payload.note ? `: ${payload.note}` : ''}. Do not retry that call. Tell the user and propose a different approach.`;

  await dispatch(SuperAgent, {
    id: conversationId,
    idempotencyKey: `approval:${payload.approval_id}:${payload.status}`,
    message: {
      kind: 'signal',
      type: 'macro.approval_decided',
      body,
      attributes: {
        approvalId: payload.approval_id,
        status: payload.status,
        toolName: payload.tool_name,
        approved: String(payload.approved),
        ...(payload.decided_by ? { decidedBy: payload.decided_by } : {}),
      },
    },
  });
}
