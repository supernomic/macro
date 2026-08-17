'use agent';

/**
 * The Macro super agent: the baseline assistant available to every employee
 * across Slack and Macro's native surfaces. Delegates to domain agents
 * (techops first) as those land; this composition is the trunk they overlay.
 *
 * The Flue conversation id is the durable identity; the Macro session it
 * maps to (via the session-map API) is where the trace of record lives.
 */

import { useModel, useTool } from '@flue/runtime';
import { config } from '../config.ts';
import {
  sessionContextFor,
  superAgentRuntimeInstance,
} from '../sessions/macro-session.ts';
import { readDocument } from '../tools/documents/read-document.ts';
import { queryMyHistory } from '../tools/ledger/query-my-history.ts';
import { searchDocuments } from '../tools/search/search-documents.ts';
import { bindMacroTools } from '../tools/toolkit.ts';

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

/** The super agent. One instance per conversation id. */
export function SuperAgent({ id }: { id: string }) {
  useModel(config.superAgentModel);

  const session = sessionContextFor({
    runtime: superAgentRuntimeInstance(),
    conversationId: id,
  });

  for (const tool of bindMacroTools(session, [
    searchDocuments,
    readDocument,
    queryMyHistory,
  ])) {
    useTool(tool);
  }

  return INSTRUCTIONS;
}

SuperAgent.agentName = 'super-agent';
