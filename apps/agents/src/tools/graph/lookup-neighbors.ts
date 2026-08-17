/**
 * `lookup_graph_neighbors`: read typed relationships from Macro's entity
 * graph (employee ⟷ device ⟷ application). Requires `graph:read`.
 */

import * as v from 'valibot';
import { defineMacroTool } from '../toolkit.ts';

export const lookupGraphNeighbors = defineMacroTool({
  name: 'lookup_graph_neighbors',
  description:
    'Look up neighbors of an entity-graph node (devices an employee ' +
    'owns, apps they have accounts on, etc.). Use when a TechOps ' +
    'question depends on who-owns-what rather than documents.',
  input: v.object({
    node_id: v.pipe(v.string(), v.description('Graph node UUID.')),
    relationship: v.optional(
      v.pipe(
        v.string(),
        v.description(
          'Optional relationship filter, e.g. `owns_device`, `has_account`.',
        ),
      ),
    ),
  }),
  async run(data, ctx) {
    const neighbors = await ctx.session.runtime.graph.neighbors(
      data.node_id,
      data.relationship,
    );
    return { output: { neighbors } };
  },
});
