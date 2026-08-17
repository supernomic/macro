import * as v from 'valibot';
import { defineMacroTool } from '../toolkit.ts';

/**
 * The failure/resolution memory tool: lets an agent consult prior sessions
 * (what was tried, what worked, what escalated) instead of starting from
 * scratch. Requires the `ledger:query` scope.
 */
export const queryMyHistory = defineMacroTool({
  name: 'query_my_history',
  description:
    'Query this agent\u2019s prior session events within the organization. ' +
    'Use it before re-attempting a problem: prior sessions record what ' +
    'was already tried, what failed, and what was escalated to a human. ' +
    'Filter by event type (e.g. "escalation/created", "turn/end", ' +
    '"tool/result") to narrow the view.',
  input: v.object({
    eventTypes: v.optional(
      v.pipe(
        v.array(v.string()),
        v.description(
          'Event type discriminants to include, e.g. ["escalation/created"].',
        ),
      ),
    ),
    sessionId: v.optional(
      v.pipe(v.string(), v.description('Restrict to one session id.')),
    ),
    limit: v.optional(
      v.pipe(
        v.number(),
        v.integer(),
        v.minValue(1),
        v.maxValue(200),
        v.description('Maximum events to return (default 50).'),
      ),
    ),
  }),
  async run(data, ctx) {
    const events = await ctx.session.runtime.ledger.queryOrgEvents({
      sessionId: data.sessionId,
      eventTypes: data.eventTypes,
      limit: data.limit ?? 50,
    });
    return {
      output: {
        events: events.map((e) => ({
          session_id: e.session_id,
          seq: e.seq,
          event_type: e.event_type,
          occurred_at: e.occurred_at,
          payload: e.payload,
        })),
      },
    };
  },
});
