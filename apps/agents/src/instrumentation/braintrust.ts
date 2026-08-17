/**
 * Braintrust tracing for Flue runtime events.
 *
 * Initializes only when `BRAINTRUST_API_KEY` is set. Correlation metadata
 * (composition id, conversation id) is attached so Braintrust traces join
 * Macro ledger rows. Never send secrets or raw document bodies.
 *
 * See Flue docs: `./node_modules/.bin/flue docs read ecosystem/tooling/braintrust`.
 */

import { observe } from '@flue/runtime';
import {
  braintrustFlueObserver,
  initLogger,
  setMaskingFunction,
} from 'braintrust';
import { config } from '../config.ts';
import { compositionIdFor } from '../sessions/macro-session.ts';

const SECRETISH =
  /(?:api[_-]?key|token|secret|password|authorization|bearer\s+mat_)/i;

function maskValue(value: unknown): unknown {
  if (typeof value === 'string' && SECRETISH.test(value)) {
    return '[redacted]';
  }
  return value;
}

if (config.braintrustApiKey()) {
  setMaskingFunction(maskValue);
  initLogger({
    projectName: config.braintrustProjectName,
    apiKey: config.braintrustApiKey(),
  });
  observe((event, ctx) => {
    const compatible =
      event.type === 'tool' ? { ...event, type: 'tool_call' as const } : event;
    const compositionId = compositionIdFor(ctx.agentName);
    const correlated =
      compositionId === undefined
        ? ctx
        : {
            ...ctx,
            metadata: {
              composition_id: compositionId,
              conversation_id: ctx.id,
            },
          };
    braintrustFlueObserver(compatible, correlated);
  });
}
