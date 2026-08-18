/**
 * Session-map anchors for Flue conversations.
 *
 * Macro's ledger owns `agent_session_map`. Flue opens (or resumes) a row
 * through `LedgerClient.openSession` before the first append. This helper
 * picks the external-thread key so DCS native chats and Slack threads
 * resume the same durable session.
 */

import type { ExternalThreadKind } from '../ledger/events.ts';

/** Slack thread identity carried on Flue conversation creation data. */
export type SlackThreadRef = {
  channelId: string;
  threadTs: string;
};

/** External thread passed to the session-map API. */
export type ExternalThread = {
  kind: ExternalThreadKind;
  key: string;
};

/**
 * Map a Flue conversation to a Macro session-map anchor.
 *
 * Slack: `slack_thread` keyed by `channel:thread_ts`.
 * DCS native chat (and any other non-Slack conversation): `native_chat`
 * keyed by the Flue conversation id, which DCS sets to the Macro chat id.
 */
export function externalThreadFor(
  conversationId: string,
  slack?: SlackThreadRef,
): ExternalThread {
  if (slack) {
    return {
      kind: 'slack_thread',
      key: `${slack.channelId}:${slack.threadTs}`,
    };
  }
  return { kind: 'native_chat', key: conversationId };
}
