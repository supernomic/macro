/**
 * OpenTelemetry GenAI spans for Flue runtime events.
 *
 * Registers Flue's `@flue/opentelemetry` instrumentation when an OTLP
 * endpoint is configured. Conversation content stays on the Macro ledger;
 * these spans are content-free. Join traces to ledger rows via
 * `gen_ai.conversation.id` and the pinned `composition_id` on
 * `request/header` events.
 *
 * See Flue docs: `./node_modules/.bin/flue docs read ecosystem/tooling/opentelemetry`.
 */

import { createOpenTelemetryInstrumentation } from '@flue/opentelemetry';
import { instrument } from '@flue/runtime';
import { config } from '../config.ts';

if (config.otelExporterEndpoint()) {
  instrument(createOpenTelemetryInstrumentation({ content: false }));
}
