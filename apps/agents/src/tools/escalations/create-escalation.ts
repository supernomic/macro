/**
 * `create_escalation`: hand the conversation to a human expert through
 * Macro's escalation service. Macro routes it (rules → expert or team
 * queue), it surfaces in the assignee's Macro inbox, and when the expert
 * resolves it Macro calls this service back so the conversation resumes
 * with the answer.
 */

import * as v from 'valibot';
import { config } from '../../config.ts';
import { defineMacroTool } from '../toolkit.ts';

/** Where the resume callback for one conversation lands. */
export function escalationCallbackUrl(conversationId: string): string {
  const base = config.publicBaseUrl.replace(/\/$/, '');
  return `${base}/callbacks/escalations/${encodeURIComponent(conversationId)}`;
}

/**
 * Bind the escalation tool to one conversation. The conversation id keys
 * the resume callback; requester fields come from the channel that started
 * the conversation (e.g. Slack user + thread).
 */
export function createEscalation(opts: {
  conversationId: string;
  requesterDisplay: string;
  requesterUserId?: string;
  sourceChannel?: string;
}) {
  return defineMacroTool({
    name: 'create_escalation',
    description:
      'Escalate this conversation to a human expert when you cannot ' +
      'resolve the request with your tools. Summarize what was asked, ' +
      'what you tried, and where you got stuck — that summary is the ' +
      "expert's entire briefing. The expert's answer arrives back in " +
      'this conversation later; after escalating, tell the user an ' +
      'expert will follow up.',
    input: v.object({
      domain: v.pipe(
        v.string(),
        v.description('Domain to route to (e.g. `techops`).'),
      ),
      title: v.pipe(
        v.string(),
        v.description('Short title for the expert inbox card.'),
      ),
      summary: v.pipe(
        v.string(),
        v.description(
          'The briefing: the request, what you tried, where you got stuck.',
        ),
      ),
      tags: v.optional(
        v.pipe(
          v.array(v.string()),
          v.description('Routing tags (e.g. `vpn`, `sso`).'),
        ),
      ),
      priority: v.optional(
        v.pipe(
          v.union([
            v.literal('low'),
            v.literal('normal'),
            v.literal('high'),
            v.literal('urgent'),
          ]),
          v.description('Urgency; defaults to normal.'),
        ),
      ),
    }),
    async run(data, ctx) {
      const session = await ctx.session.session();
      const escalation = await ctx.session.runtime.escalations.create({
        domain: data.domain,
        session_id: session.session_id,
        requester_user_id: opts.requesterUserId,
        requester_display: opts.requesterDisplay,
        source_channel: opts.sourceChannel,
        title: data.title,
        summary: data.summary,
        tags: data.tags,
        priority: data.priority,
        callback_url: escalationCallbackUrl(opts.conversationId),
      });

      await ctx.session.append({
        payload: {
          type: 'escalation_created',
          data: { escalation_id: escalation.id, domain: escalation.domain },
        },
        actor_kind: 'agent',
        actor_id: ctx.session.runtime.agentSlug,
      });

      const routed = escalation.assignee_user_id
        ? `assigned directly to an expert`
        : escalation.assignee_team_id
          ? `queued for the responsible team`
          : `awaiting routing by an operator`;
      return {
        output: {
          escalation_id: escalation.id,
          status: escalation.status,
          routing: routed,
          note:
            'The expert reply will arrive in this conversation when the ' +
            'escalation is resolved. Let the user know an expert will ' +
            'follow up.',
        },
      };
    },
  });
}
