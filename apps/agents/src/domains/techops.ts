/**
 * TechOps: the first domain agent. IT support and infrastructure for the
 * organization's employees — the domain the previous LangGraph system
 * served, and the source of the 400+ learned resolutions that will be
 * distilled into skills.
 *
 * Ticketing, device (Iru/MDM), network (Meraki), and identity (Okta) tools
 * arrive with the lifecycle-integration lane; until then the domain works
 * from organizational knowledge and prior-session history.
 */

import { readDocument } from '../tools/documents/read-document.ts';
import { lookupGraphNeighbors } from '../tools/graph/lookup-neighbors.ts';
import { queryMyHistory } from '../tools/ledger/query-my-history.ts';
import { searchDocuments } from '../tools/search/search-documents.ts';
import type { DomainAgentSpec } from './registry.ts';

export const techOps: DomainAgentSpec = {
  slug: 'techops',
  description:
    'IT support and infrastructure specialist: employee hardware and ' +
    'device questions, software access and licenses, accounts and SSO, ' +
    'network/VPN issues, and IT policy. Delegate with a complete, ' +
    'self-contained briefing including who is asking and what they ' +
    'already tried.',
  instructions: `You are the TechOps specialist for this organization: IT support and
infrastructure (devices, software access, accounts/SSO, network, IT
policy).

Work the task you were briefed with; the briefing is your entire context.

Ground rules:
- Ground answers in the organization's actual documentation: search before
  answering, read what you cite.
- Check prior session history (query_my_history) before re-deriving a fix:
  earlier sessions may already contain the working resolution or a failed
  approach to avoid.
- Distinguish clearly between (a) a resolution you verified from
  documentation or history, (b) a plausible suggestion, and (c) something
  requiring a human expert. Never present (b) or (c) as (a).
- If the task needs action you have no tool for (resetting accounts,
  changing device policy, network changes), return a precise handoff
  summary: what the problem is, what you verified, what remains, and who
  plausibly owns it.
- Your final message goes back to the requesting agent verbatim: make it a
  complete, self-contained answer.`,
  tools: [searchDocuments, readDocument, queryMyHistory, lookupGraphNeighbors],
  compositionId: 'techops-agent/v1',
};
