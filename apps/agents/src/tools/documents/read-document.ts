import * as v from 'valibot';
import { defineMacroTool, MacroToolError } from '../toolkit.ts';

const MAX_CONTENT_CHARS = 48_000;

/** Read one Macro document's content. Requires `tool:read_document`. */
export const readDocument = defineMacroTool({
  name: 'read_document',
  description:
    'Read the raw content of one Macro document by id (markdown source ' +
    'for markdown documents). Long documents are truncated; the result ' +
    'says when that happened.',
  input: v.object({
    documentId: v.pipe(
      v.string(),
      v.minLength(1),
      v.description('The Macro document id, e.g. from search_documents.'),
    ),
  }),
  async run(data, ctx) {
    const doc = ctx.macro.documents.byId(data.documentId);
    let content: string;
    try {
      content = await doc.content();
    } catch (e) {
      throw new MacroToolError(
        'not_found',
        `could not read document ${data.documentId}: ${
          e instanceof Error ? e.message : String(e)
        }`,
      );
    }
    const truncated = content.length > MAX_CONTENT_CHARS;
    return {
      output: {
        id: doc.id,
        name: await doc.name(),
        content: truncated ? content.slice(0, MAX_CONTENT_CHARS) : content,
        truncated,
      },
    };
  },
});
