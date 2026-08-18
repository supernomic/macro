import * as v from 'valibot';
import { slackClient } from '../../channels/slack-client.ts';
import { defineMacroTool, MacroToolError } from '../toolkit.ts';

/**
 * Reply tool factory bound to one Slack thread. The model selects the text;
 * it cannot select the workspace, channel, thread, or credential — those
 * are fixed by trusted code from the conversation's creation data.
 */
export function replyInSlackThread(ref: {
  channelId: string;
  threadTs: string;
}) {
  return defineMacroTool({
    name: 'reply_in_slack_thread',
    description:
      'Reply in the Slack thread this conversation is bound to. This is ' +
      'the only way your words reach the Slack participants \u2014 plain ' +
      'assistant text is not delivered. Not replying is a valid choice: ' +
      'when the conversation does not call for your input, simply do ' +
      'not use this tool.',
    input: v.object({
      text: v.pipe(
        v.string(),
        v.minLength(1),
        v.description('The message text (Slack mrkdwn).'),
      ),
    }),
    async run(data) {
      try {
        const result = await slackClient().chat.postMessage({
          channel: ref.channelId,
          thread_ts: ref.threadTs,
          text: data.text,
        });
        return { output: { ts: result.ts ?? null } };
      } catch (e) {
        throw new MacroToolError(
          'slack_send_failed',
          e instanceof Error ? e.message : String(e),
        );
      }
    },
  });
}
