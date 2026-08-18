import { describe, expect, test } from 'bun:test';
import { externalThreadFor } from './map.ts';

describe('externalThreadFor', () => {
  test('anchors DCS chats as native_chat keyed by conversation id', () => {
    expect(externalThreadFor('chat-abc')).toEqual({
      kind: 'native_chat',
      key: 'chat-abc',
    });
  });

  test('anchors Slack threads by channel and thread ts', () => {
    expect(
      externalThreadFor('ignored', {
        channelId: 'C123',
        threadTs: '1.2',
      }),
    ).toEqual({
      kind: 'slack_thread',
      key: 'C123:1.2',
    });
  });
});
