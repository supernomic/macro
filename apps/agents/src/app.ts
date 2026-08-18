/**
 * The agents service HTTP application.
 *
 * Every agent conversation is addressable
 * (`POST /agents/super-agent/:id`), per Flue's routing model. Channel
 * ingress (Slack) and internal triggers (email webhooks, schedules) land in
 * their own routes and `dispatch(...)` into conversations.
 */

import { createAgentRouter } from '@flue/runtime/routing';
import { Hono } from 'hono';
import { SuperAgent } from './agents/super-agent.ts';
import {
  type ApprovalCallbackPayload,
  resumeFromApproval,
} from './approvals/resume.ts';
import { channel as slack } from './channels/slack.ts';
import { config } from './config.ts';
import {
  type EscalationCallbackPayload,
  resumeFromEscalation,
} from './escalations/resume.ts';
import {
  type RefineCandidate,
  runTraceRefinement,
} from './jobs/trace-refine.ts';
import { superAgentRuntimeInstance } from './sessions/macro-session.ts';
import './instrumentation/braintrust.ts';
import './instrumentation/otel.ts';

const app = new Hono();

app.get('/healthz', (c) => c.json({ ok: true }));

app.route('/agents/super-agent', createAgentRouter(SuperAgent));

// Slack Events API endpoint: POST /channels/slack/events (verified ingress).
app.route('/channels/slack', slack.route());

// Escalation resume callback: Macro posts here when an expert resolves (or
// cancels) an escalation, and the answer is dispatched back into the
// conversation that escalated. Verified by the shared callback token.
app.post('/callbacks/escalations/:conversationId', async (c) => {
  const expected = config.escalationCallbackToken();
  if (expected) {
    const auth = c.req.header('authorization');
    if (auth !== `Bearer ${expected}`) {
      return c.json({ error: 'unauthorized' }, 401);
    }
  }
  const payload = (await c.req.json()) as EscalationCallbackPayload;
  if (!payload.escalation_id || !payload.status) {
    return c.json({ error: 'malformed payload' }, 400);
  }
  await resumeFromEscalation(c.req.param('conversationId'), payload);
  return c.json({ ok: true });
});

// Approval-decision callback: Macro posts here when an approver decides
// (or cancels) a gated tool call. Verified by the same shared token.
app.post('/callbacks/approvals/:conversationId', async (c) => {
  const expected = config.escalationCallbackToken();
  if (expected) {
    const auth = c.req.header('authorization');
    if (auth !== `Bearer ${expected}`) {
      return c.json({ error: 'unauthorized' }, 401);
    }
  }
  const payload = (await c.req.json()) as ApprovalCallbackPayload;
  if (!payload.approval_id || !payload.tool_name) {
    return c.json({ error: 'malformed payload' }, 400);
  }
  await resumeFromApproval(c.req.param('conversationId'), payload);
  return c.json({ ok: true });
});

// Operator-triggered trace refinement: read ledger evidence
// (escalation/resolved, feedback/record) and open org-scope skill
// proposals. Verified by the same shared callback token. Candidates in
// the body are optional — when omitted the job distills from the ledger.
app.post('/jobs/trace-refine', async (c) => {
  const expected = config.escalationCallbackToken();
  if (expected) {
    const auth = c.req.header('authorization');
    if (auth !== `Bearer ${expected}`) {
      return c.json({ error: 'unauthorized' }, 401);
    }
  }
  let candidates: RefineCandidate[] | undefined;
  try {
    const parsed: unknown = await c.req.json();
    if (
      parsed !== null &&
      typeof parsed === 'object' &&
      'candidates' in parsed
    ) {
      const raw = (parsed as { candidates: unknown }).candidates;
      if (!Array.isArray(raw)) {
        return c.json({ error: 'malformed payload' }, 400);
      }
      candidates = raw as RefineCandidate[];
    }
  } catch {
    // Empty or non-JSON body: the job consults the ledger itself.
  }
  const result = await runTraceRefinement(
    superAgentRuntimeInstance(),
    candidates,
  );
  return c.json(result);
});

export default app;
