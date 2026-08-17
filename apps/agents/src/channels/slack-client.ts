/**
 * Outbound Slack Web API client, shared by the ingress module (bot-identity
 * lookups) and the reply tools. Flue channels are ingress-only; every
 * outbound Slack call goes through this client (the provider's own SDK).
 */

import { WebClient } from '@slack/web-api';
import { config } from '../config.ts';

let cachedClient: WebClient | undefined;

/** The configured Slack Web API client (token from `SLACK_BOT_TOKEN`). */
export function slackClient(): WebClient {
  cachedClient ??= new WebClient(config.slackBotToken());
  return cachedClient;
}

let botUserIdPromise: Promise<string | undefined> | undefined;

/**
 * The bot's own Slack user id, fetched once via `auth.test`. Used to drop
 * self-authored messages and to avoid double-handling messages that also
 * fire `app_mention`. Resolves `undefined` when the lookup fails (the
 * caller should then fail open on mention filtering, closed on self).
 */
export function slackBotUserId(): Promise<string | undefined> {
  botUserIdPromise ??= slackClient()
    .auth.test()
    .then((res) => res.user_id)
    .catch((e) => {
      console.error('slack auth.test failed', e);
      botUserIdPromise = undefined;
      return undefined;
    });
  return botUserIdPromise;
}
