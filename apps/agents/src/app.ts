/**
 * The agents service HTTP application.
 *
 * Every agent conversation is addressable
 * (`POST /agents/super-agent/:id`), per Flue's routing model. Channel
 * ingress (Slack) and internal triggers (email webhooks, schedules) land in
 * their own routes and `dispatch(...)` into conversations.
 */

import { createAgentRouter } from '@flue/runtime/routing';
import { Hono } from 'hono';
import { SuperAgent } from './agents/super-agent.ts';
import { channel as slack } from './channels/slack.ts';

const app = new Hono();

app.get('/healthz', (c) => c.json({ ok: true }));

app.route('/agents/super-agent', createAgentRouter(SuperAgent));

// Slack Events API endpoint: POST /channels/slack/events (verified ingress).
app.route('/channels/slack', slack.route());

export default app;
