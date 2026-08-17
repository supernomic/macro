/**
 * The Macro tool toolkit: every tool in this service is defined with
 * `defineMacroTool` and mounted through `bindMacroTools`. The wrapper is
 * what makes "model-visible means logged" non-optional — it emits
 * `tool/call` and `tool/result` ledger events around every execution and
 * converts failures into structured, model-friendly errors.
 *
 * See `apps/agents/CONTRIBUTING.md` for the full conventions.
 */

import { defineTool, type JsonValue } from '@flue/runtime';
import type { Macro } from '@macro/sdk';
import type * as v from 'valibot';
import type { MacroSessionContext } from '../sessions/macro-session.ts';

/**
 * Structured tool failure. Thrown by tool bodies; the wrapper records it on
 * the `tool/result` ledger event and re-throws so the model sees a
 * descriptive error it can react to. Never let raw HTTP errors reach the
 * model.
 */
export class MacroToolError extends Error {
  /** Stable machine-readable code (e.g. `not_found`, `forbidden`). */
  readonly code: string;

  constructor(code: string, message: string) {
    super(message);
    this.name = 'MacroToolError';
    this.code = code;
  }

  /** Normalize an arbitrary thrown value into a `MacroToolError`. */
  static from(e: unknown): MacroToolError {
    if (e instanceof MacroToolError) {
      return e;
    }
    if (e instanceof Error) {
      return new MacroToolError('internal', e.message);
    }
    return new MacroToolError('internal', String(e));
  }
}

/** Context every Macro tool body receives alongside its parsed input. */
export interface MacroToolContext {
  /** Macro SDK client authenticated as the executing agent principal. */
  macro: Macro;
  /** The conversation's session context (ledger access, session id). */
  session: MacroSessionContext;
  /**
   * Idempotency key derived from the ledger position of this call's
   * `tool/call` event (`<session_id>:<seq>`). Tools that create entities
   * must forward it so retried steps don't double-create.
   */
  idempotencyKey: string;
  /** Abort signal for this call; pass it to long-running work. */
  signal: AbortSignal | undefined;
}

/** Result envelope a Macro tool body returns (a bare string is shorthand). */
export type MacroToolResult = string | { output: unknown };

/** A Macro tool definition, prior to being bound to a conversation. */
export interface MacroToolDef<
  TInput extends v.ObjectSchema<v.ObjectEntries, undefined> = v.ObjectSchema<
    v.ObjectEntries,
    undefined
  >,
> {
  /** snake_case, verb-first; must match the `tool:<name>` scope. */
  name: string;
  /** The model's only documentation for this tool — be precise. */
  description: string;
  /** Valibot input schema (top-level object, per Flue's contract). */
  input: TInput;
  /** The tool body. Throw `MacroToolError` for structured failures. */
  run(
    data: v.InferOutput<TInput>,
    ctx: MacroToolContext,
  ): Promise<MacroToolResult>;
}

/**
 * Identity helper so tool modules keep full input-schema type inference
 * without importing Flue directly.
 */
export function defineMacroTool<
  TInput extends v.ObjectSchema<v.ObjectEntries, undefined>,
>(def: MacroToolDef<TInput>): MacroToolDef<TInput> {
  return def;
}

/**
 * Bind Macro tools to one conversation's session context, producing Flue
 * tool definitions ready for `useTool`.
 *
 * Note on `arguments_raw`: at this layer Flue has already parsed and
 * validated the model's arguments, so we log the canonical JSON of the
 * parsed value. Byte-exact raw capture (including malformed calls) is the
 * instrumentation lane's job via Flue's observability surface.
 */
export function bindMacroTools(
  session: MacroSessionContext,
  tools: readonly MacroToolDef[],
) {
  return tools.map((tool) =>
    defineTool({
      name: tool.name,
      description: tool.description,
      input: tool.input,
      async run({ data, toolCallId, signal }) {
        const agentId = session.runtime.agentSlug;
        const [callEvent] = await session.append({
          payload: {
            type: 'tool_call',
            data: {
              call_id: toolCallId,
              name: tool.name,
              arguments_raw: JSON.stringify(data),
            },
          },
          actor_kind: 'agent',
          actor_id: agentId,
        });
        const idempotencyKey = callEvent
          ? `${callEvent.session_id}:${callEvent.seq}`
          : `${toolCallId}`;
        const ctx: MacroToolContext = {
          macro: session.runtime.macro,
          session,
          idempotencyKey,
          signal,
        };
        try {
          const result = await tool.run(data, ctx);
          const output = (
            typeof result === 'string' ? result : result.output
          ) as JsonValue;
          await session.append({
            payload: {
              type: 'tool_result',
              data: { call_id: toolCallId, name: tool.name, content: output },
            },
            actor_kind: 'agent',
            actor_id: agentId,
            source_event_seqs: callEvent ? [callEvent.seq] : [],
          });
          return { output };
        } catch (e) {
          const err = MacroToolError.from(e);
          await session.append({
            payload: {
              type: 'tool_result',
              data: {
                call_id: toolCallId,
                name: tool.name,
                content: null,
                error: { code: err.code, message: err.message },
              },
            },
            actor_kind: 'agent',
            actor_id: agentId,
            source_event_seqs: callEvent ? [callEvent.seq] : [],
          });
          throw err;
        }
      },
    }),
  );
}
