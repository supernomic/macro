/**
 * @vitest-environment jsdom
 */

import type { AgentInboxItem } from '@service-cognition/client';
import { render, screen } from '@solidjs/testing-library';
import { describe, expect, it } from 'vitest';
import { AgentReviewList } from './agent-review-list';

const item = (
  overrides: Partial<AgentInboxItem> & Pick<AgentInboxItem, 'kind' | 'id'>
): AgentInboxItem => ({
  title: 'Needs a look',
  status: 'pending',
  created_at: '2026-08-17T12:00:00Z',
  href: `/${overrides.kind}s/${overrides.id}`,
  assigned_to_me: true,
  team_queued: false,
  ...overrides,
});

describe('AgentReviewList', () => {
  it('renders empty-list copy', () => {
    render(() => (
      <AgentReviewList items={[]} isLoading={false} isError={false} />
    ));
    expect(screen.getByText('No items to review')).toBeTruthy();
  });

  it('renders kind labels for each inbox kind', () => {
    render(() => (
      <AgentReviewList
        items={[
          item({ kind: 'escalation', id: 'esc-1', title: 'Stuck on deploy' }),
          item({ kind: 'approval', id: 'apr-1', title: 'Send email' }),
          item({
            kind: 'skill_proposal',
            id: 'sk-1',
            title: 'Update onboarding skill',
          }),
        ]}
        isLoading={false}
        isError={false}
        formatCreated={() => 'just now'}
      />
    ));
    expect(screen.getByText('Escalation')).toBeTruthy();
    expect(screen.getByText('Approval')).toBeTruthy();
    expect(screen.getByText('Skill proposal')).toBeTruthy();
    expect(screen.getByText('Stuck on deploy')).toBeTruthy();
    expect(screen.getByText('Send email')).toBeTruthy();
    expect(screen.getByText('Update onboarding skill')).toBeTruthy();
  });
});
