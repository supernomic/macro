import { describe, expect, it } from 'vitest';
import { assignmentLabel, kindLabel, listFallbackMessage } from './labels';

describe('kindLabel', () => {
  it('maps every inbox kind to review copy', () => {
    expect(kindLabel('escalation')).toBe('Escalation');
    expect(kindLabel('approval')).toBe('Approval');
    expect(kindLabel('skill_proposal')).toBe('Skill proposal');
  });
});

describe('assignmentLabel', () => {
  it('distinguishes assigned vs team queue', () => {
    expect(assignmentLabel({ assigned_to_me: true, team_queued: false })).toBe(
      'Assigned to me'
    );
    expect(assignmentLabel({ assigned_to_me: false, team_queued: true })).toBe(
      'Team queue'
    );
    expect(assignmentLabel({ assigned_to_me: true, team_queued: true })).toBe(
      'Assigned to me · Team queue'
    );
    expect(assignmentLabel({ assigned_to_me: false, team_queued: false })).toBe(
      'Unassigned'
    );
  });
});

describe('listFallbackMessage', () => {
  it('returns empty-list copy when there is nothing to review', () => {
    expect(
      listFallbackMessage({ itemCount: 0, isLoading: false, isError: false })
    ).toBe('No items to review');
  });

  it('prefers loading and error over the empty copy', () => {
    expect(
      listFallbackMessage({ itemCount: 0, isLoading: true, isError: false })
    ).toBe('Loading…');
    expect(
      listFallbackMessage({ itemCount: 0, isLoading: false, isError: true })
    ).toBe('Agent Review is unavailable right now. Try again in a moment.');
  });

  it('is silent when the list has rows', () => {
    expect(
      listFallbackMessage({ itemCount: 1, isLoading: false, isError: false })
    ).toBeUndefined();
  });
});
