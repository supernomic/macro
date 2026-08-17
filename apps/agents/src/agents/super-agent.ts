'use agent';

/**
 * The Macro super agent: the baseline assistant available to every employee
 * across Slack and Macro's native surfaces. Delegates to domain agents
 * (techops first) as those land; this composition is the trunk they overlay.
 *
 * The Flue conversation id is the durable identity; the Macro session it
 * maps to (via the session-map API) is where the trace of record lives.
 * Slack-born conversations carry the thread in their creation data, which
 * binds the reply tool and anchors the Macro session to the thread.
 */

import { useInitialData, useModel, useTool } from '@flue/runtime';
import * as v from 'valibot';
import { config } from '../config.ts';
import {
  sessionContextFor,
  superAgentRuntimeInstance,
} from '../sessions/macro-session.ts';
import { readDocument } from '../tools/documents/read-document.ts';
import { queryMyHistory } from '../tools/ledger/query-my-history.ts';
import { searchDocuments } from '../tools/search/search-documents.ts';
import { replyInSlackThread } from '../tools/slack/reply-in-slack-thread.ts';
import { bindMacroTools, type MacroToolDef } from '../tools/toolkit.ts';

const INSTRUCTIONS = `You are Macro's assistant for this organization's employees.

Ground rules:
- Ground every answer in the organization's actual data: search documents
  before answering knowledge questions, and read the sources you cite.
- Before re-attempting a problem you may have seen before, check prior
  session history (query_my_history) so you don't repeat failed approaches
  and you reuse what already worked.
- When you cannot resolve a request with your tools, say so plainly and
  summarize what you tried. Do not guess. (Escalation to a human expert
  arrives in a later capability; until then, an honest handoff summary is
  the correct ending.)
- Be concise. Answer first, cite sources after.`;

const SLACK_INSTRUCTIONS = `

Slack context: you are one participant in a Slack thread. Your ONLY way to
speak is the reply_in_slack_thread tool — plain assistant text is never
delivered. Be selective:
- Always respond when directly @-mentioned or asked a question.
- For messages that merely pass through your channels
  (slack.channel_message signals), reply only when you are confident you
  add clear value: you can answer a question others haven't, correct a
  material error, or you were implicitly addressed. Otherwise, end your
  turn WITHOUT calling the reply tool — staying silent is correct.
- Never announce that you are choosing not to reply.`;

/** The super agent. One instance per conversation id. */
export function SuperAgent({ id }: { id: string }) {
  useModel(config.superAgentModel);

  const data = useInitialData<v.InferOutput<typeof SuperAgent.initialData>>();
  const slack = data?.slack;

  const session = sessionContextFor({
    runtime: superAgentRuntimeInstance(),
    conversationId: id,
    externalThread: slack
      ? {
          kind: 'slack_thread',
          key: `${slack.channelId}:${slack.threadTs}`,
        }
      : undefined,
  });

  const tools: MacroToolDef[] = [searchDocuments, readDocument, queryMyHistory];
  if (slack) {
    tools.push(
      replyInSlackThread({
        channelId: slack.channelId,
        threadTs: slack.threadTs,
      }),
    );
  }
  for (const tool of bindMacroTools(session, tools)) {
    useTool(tool);
  }

  return slack ? INSTRUCTIONS + SLACK_INSTRUCTIONS : INSTRUCTIONS;
}

SuperAgent.agentName = 'super-agent';

SuperAgent.initialData = v.optional(
  v.object({
    slack: v.optional(
      v.object({
        teamId: v.string(),
        channelId: v.string(),
        threadTs: v.string(),
        startedBy: v.optional(v.string()),
      }),
    ),
  }),
);
