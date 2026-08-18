import * as v from 'valibot';
import { defineMacroTool } from '../toolkit.ts';

/** Search Macro documents by name and content. Requires `tool:search_documents`. */
export const searchDocuments = defineMacroTool({
  name: 'search_documents',
  description:
    'Search the organization\u2019s Macro documents by name and content. ' +
    'Returns the most relevant documents first, each with its id and name. ' +
    'Use read_document to fetch a result\u2019s full content.',
  input: v.object({
    query: v.pipe(
      v.string(),
      v.minLength(1),
      v.description('Search query (matched against names and content).'),
    ),
    limit: v.optional(
      v.pipe(
        v.number(),
        v.integer(),
        v.minValue(1),
        v.maxValue(25),
        v.description('Maximum results to return (default 10).'),
      ),
    ),
  }),
  async run(data, ctx) {
    const limit = data.limit ?? 10;
    const results: { id: string; name: string | undefined }[] = [];
    for await (const doc of ctx.macro.documents.search(data.query)) {
      results.push({ id: doc.id, name: await doc.name() });
      if (results.length >= limit) {
        break;
      }
    }
    return { output: { results } };
  },
});
