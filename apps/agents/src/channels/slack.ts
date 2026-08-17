/**
 * Slack ingress: verified Events API deliveries routed into super-agent
 * conversations.
 *
 * One Slack thread ↔ one durable Flue conversation (via
 * `channel.instanceId`) ↔ one Macro session (anchored by
 * `slack_thread:<channel>:<thread_ts>` in the session map), so re-entry
 * from the same thread always resumes the same conversation and ledger.
 *
 * Listening behavior per channel comes from `slack-config.ts`:
 * mentions always deliver; plain messages deliver in `proactive` channels,
 * and in `mentions_only` channels only when the thread already has a
 * conversation (the agent was brought in earlier). Whether a delivered
 * message earns a reply is the agent's decision — its only way to speak is
 * the reply tool, so declining to call it is a non-reply.
 */

import { dispatch, getAgentInstance } from '@flue/runtime';
import { createSlackChannel } from '@flue/slack';
import { SuperAgent } from '../agents/super-agent.ts';
import { config } from '../config.ts';
import {
  sessionContextFor,
  superAgentRuntimeInstance,
} from '../sessions/macro-session.ts';
import { slackBotUserId } from './slack-client.ts';
import { modeForChannel } from './slack-config.ts';

interface SlackDelivery {
  teamId: string;
  channelId: string;
  threadTs: string;
  text: string;
  userId: string | undefined;
  eventId: string;
  signalType: 'slack.app_mention' | 'slack.channel_message';
}

async function deliverToSuperAgent(d: SlackDelivery): Promise<void> {
  const conversationId = channel.instanceId({
    teamId: d.teamId,
    channelId: d.channelId,
    threadTs: d.threadTs,
  });

  // Anchor the Macro session to the Slack thread and record the human
  // message. Best-effort here: Slack redelivers on slow acks, and dispatch
  // dedupes by event id while the ledger does not (yet) — a throw would
  // only trigger another redelivery.
  const session = sessionContextFor({
    runtime: superAgentRuntimeInstance(),
    conversationId,
    externalThread: {
      kind: 'slack_thread',
      key: `${d.channelId}:${d.threadTs}`,
    },
  });
  session
    .append({
      payload: {
        type: 'user_message',
        data: { content: d.text, source: 'human' },
      },
      actor_kind: 'user',
      actor_id: d.userId ?? 'slack:unknown',
    })
    .catch((e) => {
      console.error('failed to record slack message in ledger', e);
    });

  await dispatch(SuperAgent, {
    id: conversationId,
    // Slack redelivers events whose ack was slow or lost; the event id
    // names the delivery, so a retry never runs a second turn.
    idempotencyKey: d.eventId,
    initialData: {
      slack: {
        teamId: d.teamId,
        channelId: d.channelId,
        threadTs: d.threadTs,
        startedBy: d.userId,
      },
    },
    message: {
      kind: 'signal',
      type: d.signalType,
      body: d.text,
      attributes: {
        eventId: d.eventId,
        channelId: d.channelId,
        ...(d.userId ? { userId: d.userId } : {}),
      },
    },
  });
}

export const channel = createSlackChannel({
  signingSecret: config.slackSigningSecret(),

  // Served at POST /channels/slack/events (with the mount in app.ts).
  async events({ payload }) {
    if (payload.type !== 'event_callback') {
      return undefined;
    }
    const event = payload.event;

    if (event.type === 'app_mention') {
      await deliverToSuperAgent({
        teamId: payload.team_id,
        channelId: event.channel,
        threadTs: event.thread_ts ?? event.ts,
        text: event.text,
        userId: event.user,
        eventId: payload.event_id,
        signalType: 'slack.app_mention',
      });
      return undefined;
    }

    if (event.type === 'message') {
      // Plain human messages only: no edits/joins/etc., nothing
      // bot-authored (including our own replies).
      if ('subtype' in event && event.subtype !== undefined) {
        return undefined;
      }
      if ('bot_id' in event && event.bot_id) {
        return undefined;
      }
      const text = 'text' in event ? (event.text ?? '') : '';
      const userId = 'user' in event ? event.user : undefined;
      if (!text || !('channel' in event)) {
        return undefined;
      }

      const mode = modeForChannel(event.channel);
      if (mode === 'silent') {
        return undefined;
      }

      // A message that mentions the bot also fires app_mention —
      // let that handler own it.
      const botId = await slackBotUserId();
      if (botId && text.includes(`<@${botId}>`)) {
        return undefined;
      }
      if (botId && userId === botId) {
        return undefined;
      }

      const threadTs =
        'thread_ts' in event && event.thread_ts ? event.thread_ts : event.ts;

      if (mode === 'mentions_only') {
        // Only keep participating in threads the agent was already
        // brought into (via an earlier mention).
        const existing = await getAgentInstance(
          SuperAgent,
          channel.instanceId({
            teamId: payload.team_id,
            channelId: event.channel,
            threadTs,
          }),
        );
        if (!existing) {
          return undefined;
        }
      }

      await deliverToSuperAgent({
        teamId: payload.team_id,
        channelId: event.channel,
        threadTs,
        text,
        userId,
        eventId: payload.event_id,
        signalType: 'slack.channel_message',
      });
    }
    return undefined;
  },
});
