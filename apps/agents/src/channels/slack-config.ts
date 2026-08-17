/**
 * Per-channel Slack listening configuration.
 *
 * Modes:
 * - `mentions_only` (default): the agent responds to @-mentions, and keeps
 *   participating in threads where it was already brought in.
 * - `proactive`: every plain channel message is delivered; the agent
 *   decides per message whether a reply adds value (the selectivity lives
 *   in its instructions — an undelivered reply tool call is a non-reply).
 * - `silent`: nothing is delivered from this channel.
 *
 * Configuration is env-driven for now (`SLACK_CHANNEL_MODES`); per-tenant
 * channel config served from Macro's API replaces this when the tenant
 * admin surface lands.
 */

import { config } from '../config.ts';

/** How the agent listens in one Slack channel. */
export type SlackChannelMode = 'mentions_only' | 'proactive' | 'silent';

const VALID_MODES: readonly SlackChannelMode[] = [
  'mentions_only',
  'proactive',
  'silent',
];

function parseMode(value: unknown): SlackChannelMode | undefined {
  return VALID_MODES.find((m) => m === value);
}

let cachedModes: Map<string, SlackChannelMode> | undefined;

function channelModes(): Map<string, SlackChannelMode> {
  if (!cachedModes) {
    cachedModes = new Map();
    try {
      const raw: unknown = JSON.parse(config.slackChannelModes);
      if (raw && typeof raw === 'object') {
        for (const [channelId, mode] of Object.entries(raw)) {
          const parsed = parseMode(mode);
          if (parsed) {
            cachedModes.set(channelId, parsed);
          }
        }
      }
    } catch {
      console.error('SLACK_CHANNEL_MODES is not valid JSON; ignoring');
    }
  }
  return cachedModes;
}

/** The listening mode for one Slack channel. */
export function modeForChannel(channelId: string): SlackChannelMode {
  return (
    channelModes().get(channelId) ??
    parseMode(config.slackDefaultChannelMode) ??
    'mentions_only'
  );
}
