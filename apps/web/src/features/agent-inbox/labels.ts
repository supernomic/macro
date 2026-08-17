import type { AgentInboxItem, AgentInboxKind } from '@service-cognition/client';
import { match } from 'ts-pattern';

/** Display label for an inbox item kind. */
export function kindLabel(kind: AgentInboxKind): string {
  return match(kind)
    .with('escalation', () => 'Escalation')
    .with('approval', () => 'Approval')
    .with('skill_proposal', () => 'Skill proposal')
    .exhaustive();
}

/** Assigned-to-me vs team-queue copy for a row. */
export function assignmentLabel(
  item: Pick<AgentInboxItem, 'assigned_to_me' | 'team_queued'>
): string {
  if (item.assigned_to_me && item.team_queued) {
    return 'Assigned to me · Team queue';
  }
  if (item.assigned_to_me) return 'Assigned to me';
  if (item.team_queued) return 'Team queue';
  return 'Unassigned';
}

/** Fallback copy when the inbox list has no rows. */
export function listFallbackMessage(options: {
  itemCount: number;
  isLoading: boolean;
  isError: boolean;
}): string | undefined {
  if (options.itemCount > 0) return undefined;
  if (options.isLoading) return 'Loading…';
  if (options.isError) {
    return 'Agent Review is unavailable right now. Try again in a moment.';
  }
  return 'No items to review';
}
