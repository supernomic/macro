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

import {
  useAgentStart,
  useInitialData,
  useModel,
  useSubagent,
  useTool,
} from '@flue/runtime';
import * as v from 'valibot';
import { config } from '../config.ts';
import { DOMAIN_AGENTS, domainRuntime } from '../domains/registry.ts';
import {
  sessionContextFor,
  superAgentRuntimeInstance,
} from '../sessions/macro-session.ts';
import { externalThreadFor } from '../sessions/map.ts';
import { mountGovernedSkills } from '../skills/mount.ts';
import { readDocument } from '../tools/documents/read-document.ts';
import { createEscalation } from '../tools/escalations/create-escalation.ts';
import { recordFeedback } from '../tools/feedback/record-feedback.ts';
import { lookupGraphNeighbors } from '../tools/graph/lookup-neighbors.ts';
import { queryMyHistory } from '../tools/ledger/query-my-history.ts';
import { searchDocuments } from '../tools/search/search-documents.ts';
import { proposeSkill } from '../tools/skills/propose-skill.ts';
import { replyInSlackThread } from '../tools/slack/reply-in-slack-thread.ts';
import { bindMacroTools, type MacroToolDef } from '../tools/toolkit.ts';

const INSTRUCTIONS = `You are Macro's assistant for this organization's employees.

Ground rules:
- Ground every answer in the organization's actual data: search documents
  before answering knowledge questions, and read the sources you cite.
- Before re-attempting a problem you may have seen before, check prior
  session history (query_my_history) so you don't repeat failed approaches
  and you reuse what already worked.
- When a tool is paused for human approval, tell the user an approver
  will review it and wait. When the decision arrives, retry the same
  call if approved; if denied, do not retry — explain and propose
  another approach.
- When you cannot resolve a request with your tools, escalate to a human
  expert with create_escalation instead of guessing. Your summary is the
  expert's entire briefing: state the request, what you tried, and where
  you got stuck. Then tell the user an expert will follow up. The expert's
  answer arrives back in this conversation later — relay it in your own
  words when it does.
- When a working resolution is worth teaching others, capture it with
  propose_skill rather than burying it in chat. Team/org skills go to
  inbox review.
- When the user praises, corrects, or rates an answer, record it with
  record_feedback against the relevant ledger seq.
- Be concise. Answer first, cite sources after.`;

const DELEGATION_INSTRUCTIONS = `

Domain specialists: for requests squarely inside a specialist's domain,
delegate via the task tool instead of working it yourself. The specialist
starts with a fresh context — your task prompt is its entire briefing, so
include who is asking, the full problem, and anything already tried or
learned in this conversation. Relay its answer with your own judgment;
you stay accountable for what the user receives.`;

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
    externalThread: externalThreadFor(id, slack),
  });

  const tools: MacroToolDef[] = [
    searchDocuments,
    readDocument,
    queryMyHistory,
    proposeSkill,
    recordFeedback,
    lookupGraphNeighbors,
    createEscalation({
      conversationId: id,
      requesterDisplay: slack?.startedBy
        ? `slack:${slack.startedBy}`
        : 'unknown',
      sourceChannel: slack ? 'slack' : undefined,
    }),
  ];
  if (slack) {
    tools.push(
      replyInSlackThread({
        channelId: slack.channelId,
        threadTs: slack.threadTs,
      }),
    );
  }
  for (const tool of bindMacroTools(session, tools, {
    requesterDisplay: slack?.startedBy ? `slack:${slack.startedBy}` : 'unknown',
  })) {
    useTool(tool);
  }

  const skillMount = mountGovernedSkills(
    session,
    session.runtime.skillsCatalog,
  );
  useAgentStart(async () => {
    await session.runtime.catalogReady;
    await session.pinRequestHeader();
    await skillMount;
  });

  // Domain specialists (techops first). Only domains whose principal token
  // is configured are offered; the delegate's tools run under the domain's
  // own scoped token while its trace lands on this conversation's session.
  let anyDomains = false;
  for (const spec of DOMAIN_AGENTS) {
    const runtime = domainRuntime(spec);
    if (!runtime) {
      continue;
    }
    anyDomains = true;
    useSubagent({
      name: spec.slug,
      description: spec.description,
      ...(spec.model ? { model: spec.model } : {}),
      agent: () => {
        for (const tool of bindMacroTools(session, spec.tools, {
          actor: runtime,
          requesterDisplay: slack?.startedBy
            ? `slack:${slack.startedBy}`
            : 'unknown',
        })) {
          useTool(tool);
        }
        return spec.instructions;
      },
    });
  }

  let instructions = INSTRUCTIONS;
  if (anyDomains) {
    instructions += DELEGATION_INSTRUCTIONS;
  }
  if (slack) {
    instructions += SLACK_INSTRUCTIONS;
  }
  return instructions;
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
