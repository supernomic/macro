/**
 * `record_feedback`: append an immutable `feedback/record` ledger event and
 * upsert the editable rating sidecar. Requires `ledger:append` (via the
 * wrapper) and `feedback:write` for the sidecar.
 */

import * as v from 'valibot';
import { defineMacroTool } from '../toolkit.ts';

export const recordFeedback = defineMacroTool({
  name: 'record_feedback',
  description:
    'Record human feedback on a prior event in this session (thumbs ' +
    'up/down plus an optional note). The ledger entry is immutable; the ' +
    'sidecar rating can be edited later. Use when the user praises, ' +
    'corrects, or rates an answer.',
  input: v.object({
    target_seq: v.pipe(
      v.number(),
      v.integer(),
      v.minValue(0),
      v.description('Ledger seq of the event being rated.'),
    ),
    rating: v.pipe(
      v.union([v.literal('up'), v.literal('down'), v.literal('none')]),
      v.description('Thumbs up, down, or clear.'),
    ),
    note: v.optional(
      v.pipe(v.string(), v.description('Optional free-text comment.')),
    ),
  }),
  async run(data, ctx) {
    const session = await ctx.session.session();
    const [event] = await ctx.session.append({
      payload: {
        type: 'feedback_record',
        data: {
          rating: data.rating === 'none' ? undefined : data.rating === 'up',
          note: data.note,
          target_seq: data.target_seq,
        },
      },
      actor_kind: 'agent',
      actor_id: ctx.session.runtime.agentSlug,
    });
    const sidecar = await ctx.session.runtime.feedback.rate(
      session.session_id,
      {
        target_seq: data.target_seq,
        rating: data.rating,
        note: data.note,
      },
    );
    return {
      output: {
        ledger_seq: event?.seq ?? null,
        sidecar,
      },
    };
  },
});
