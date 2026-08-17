/**
 * Braintrust tracing for Flue runtime events.
 *
 * Initializes only when `BRAINTRUST_API_KEY` is set. Correlation metadata
 * (composition id, conversation id) is attached so Braintrust traces join
 * Macro ledger rows. Never send secrets or raw document bodies.
 *
 * See Flue docs: `./node_modules/.bin/flue docs read ecosystem/tooling/braintrust`.
 */

import { instrument, observe } from '@flue/runtime';
import {
  braintrustFlueInstrumentation,
  braintrustFlueObserver,
  currentSpan,
  initLogger,
  setMaskingFunction,
} from 'braintrust';
import { config } from '../config.ts';
import { compositionIdFor } from '../sessions/macro-session.ts';

const SECRETISH_KEY =
  /(?:api[_-]?key|token|secret|password|authorization|bearer)/i;
const SECRETISH_VALUE =
  /(?:api[_-]?key|token|secret|password|authorization|bearer\s+\S+|mat_[A-Za-z0-9_-]+)/i;

/**
 * Recursively redact secrets from a JSON-serializable Braintrust payload.
 * Braintrust invokes this on the whole record, not per string.
 */
export function maskSecrets(value: unknown): unknown {
  if (typeof value === 'string') {
    return SECRETISH_VALUE.test(value) ? '[redacted]' : value;
  }
  if (Array.isArray(value)) {
    return value.map(maskSecrets);
  }
  if (value !== null && typeof value === 'object') {
    const out: Record<string, unknown> = {};
    for (const [key, nested] of Object.entries(
      value as Record<string, unknown>,
    )) {
      out[key] = SECRETISH_KEY.test(key) ? '[redacted]' : maskSecrets(nested);
    }
    return out;
  }
  return value;
}

function conversationIdOf(
  event: { conversationId?: string },
  fallback: string,
): string {
  return typeof event.conversationId === 'string' && event.conversationId
    ? event.conversationId
    : fallback;
}

if (config.braintrustApiKey()) {
  setMaskingFunction(maskSecrets);
  initLogger({
    projectName: config.braintrustProjectName,
    apiKey: config.braintrustApiKey(),
  });
  const flueBt = braintrustFlueInstrumentation();
  // Event export: translate Flue `tool` → Braintrust `tool_call`, and stamp
  // conversationId so extractEventMetadata records `flue.conversation_id`.
  observe((event, ctx) => {
    const compatible =
      event.type === 'tool' ? { ...event, type: 'tool_call' as const } : event;
    const conversationId = conversationIdOf(compatible, ctx.id);
    braintrustFlueObserver({ ...compatible, conversationId }, ctx);
  });
  // Execution interceptor: Braintrust's observer ignores `ctx.metadata`,
  // so join keys are logged on the active span after it is installed.
  instrument({
    key: flueBt.key,
    observe() {
      // Events are handled by observe() above (needs the tool_call rename).
    },
    interceptor: (operation, execCtx, next) =>
      flueBt.interceptor(operation, execCtx, () => {
        const compositionId = compositionIdFor(execCtx.agentName);
        currentSpan().log({
          metadata: {
            composition_id: compositionId,
            conversation_id: execCtx.conversationId ?? execCtx.instanceId,
          },
        });
        return next();
      }),
    dispose: () => flueBt.dispose(),
  });
}
