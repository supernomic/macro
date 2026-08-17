/**
 * `propose_skill`: open a staged skill proposal in Macro. Team/org/platform
 * scopes stay pending for inbox review; user-scope creates may auto-apply.
 * Requires `skill:propose`.
 */

import * as v from 'valibot';
import { defineMacroTool } from '../toolkit.ts';

export const proposeSkill = defineMacroTool({
  name: 'propose_skill',
  description:
    'Propose a reusable skill (procedure the agent should follow next ' +
    'time). Use after you have a working resolution worth teaching ' +
    'others, or when a human asked you to capture one. Team and org ' +
    'skills go to inbox review; personal skills may apply immediately.',
  input: v.object({
    slug: v.pipe(
      v.string(),
      v.description(
        'Lowercase hyphenated name, e.g. `vpn-reset`. Max 64 chars.',
      ),
    ),
    name: v.pipe(v.string(), v.description('Short display name.')),
    description: v.pipe(
      v.string(),
      v.description(
        'What the skill does AND when to use it (the catalog line).',
      ),
    ),
    body: v.pipe(
      v.string(),
      v.description('Full markdown procedure (the skill instructions).'),
    ),
    target_scope: v.pipe(
      v.union([
        v.literal('user'),
        v.literal('team'),
        v.literal('org'),
        v.literal('platform'),
      ]),
      v.description('Who owns the skill. Team/org always require review.'),
    ),
    diff_summary: v.pipe(
      v.string(),
      v.description('Human-readable summary of what this proposal changes.'),
    ),
    kind: v.optional(
      v.pipe(
        v.union([
          v.literal('create'),
          v.literal('patch'),
          v.literal('archive'),
        ]),
        v.description('Defaults to create.'),
      ),
    ),
    skill_id: v.optional(
      v.pipe(
        v.string(),
        v.description('Existing skill id, required for patch/archive.'),
      ),
    ),
  }),
  async run(data, ctx) {
    const proposal = await ctx.session.runtime.skills.propose({
      kind: data.kind ?? 'create',
      skill_id: data.skill_id,
      slug: data.slug,
      target_scope: data.target_scope,
      proposed_name: data.name,
      proposed_description: data.description,
      proposed_body: data.body,
      diff_summary: data.diff_summary,
      evidence: [],
    });
    return {
      output: {
        proposal_id: proposal.id,
        status: proposal.status,
        slug: proposal.slug,
        note:
          proposal.status === 'pending'
            ? 'Queued for inbox review. Tell the user a reviewer will decide.'
            : 'Applied.',
      },
    };
  },
});
